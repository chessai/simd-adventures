// vf/simd.h -- a deliberately thin SIMD abstraction layer.
//
// Each backend is a struct of static functions over three logical widths:
//   v32  : 8 lanes of u32  (a 256-bit logical vector; NEON emulates it with two q-registers)
//   b512 : 8 lanes of u64  (a 64-byte Bloom block; one zmm on AVX-512, two ymm on AVX2,
//                           four q-registers on NEON, an array in portable)
//   v128 : 16 x u8 / 8 x u16 (one cuckoo bucket pair)
//
// Only the handful of operations the filters need are exposed. The portable
// backend is also the scalar reference used by the property tests, so its
// semantics define what the ISA backends must match bit-for-bit.
#pragma once
#include "config.h"
#include <cstring>

#if VF_HAVE_AVX2 || VF_HAVE_AVX512
#  include <immintrin.h>
#endif
#if VF_HAVE_NEON
#  include <arm_neon.h>
#endif

namespace vf {

// ---------------------------------------------------------------------------
// Portable backend (scalar reference)
// ---------------------------------------------------------------------------
struct Portable {
  static constexpr const char* name = "portable";
  static constexpr int kLanes32 = 8;

  struct v32 { u32 v[8]; };
  struct b512 { u64 v[8]; };
  struct v128 { u8 v[16]; };

  static VF_INLINE v32 set1_32(u32 x) { v32 r; VF_NOVEC for (int i = 0; i < 8; i++) r.v[i] = x; return r; }
  static VF_INLINE v32 loadu_32(const u32* p) { v32 r; std::memcpy(r.v, p, 32); return r; }
  static VF_INLINE void storeu_32(u32* p, v32 a) { std::memcpy(p, a.v, 32); }
  static VF_INLINE v32 add_32(v32 a, v32 b) { v32 r; VF_NOVEC for (int i = 0; i < 8; i++) r.v[i] = a.v[i] + b.v[i]; return r; }
  static VF_INLINE v32 xor_32(v32 a, v32 b) { v32 r; VF_NOVEC for (int i = 0; i < 8; i++) r.v[i] = a.v[i] ^ b.v[i]; return r; }
  static VF_INLINE v32 or_32(v32 a, v32 b) { v32 r; VF_NOVEC for (int i = 0; i < 8; i++) r.v[i] = a.v[i] | b.v[i]; return r; }
  static VF_INLINE v32 and_32(v32 a, v32 b) { v32 r; VF_NOVEC for (int i = 0; i < 8; i++) r.v[i] = a.v[i] & b.v[i]; return r; }
  static VF_INLINE v32 mullo_32(v32 a, v32 b) { v32 r; VF_NOVEC for (int i = 0; i < 8; i++) r.v[i] = a.v[i] * b.v[i]; return r; }
  template <int N> static VF_INLINE v32 shr_32(v32 a) { v32 r; VF_NOVEC for (int i = 0; i < 8; i++) r.v[i] = a.v[i] >> N; return r; }
  // Variable shifts: callers guarantee n < lane width (positions come from a >> 27 / >> 26);
  // the reference masks anyway so it can never be UB.
  static VF_INLINE v32 shl_var_32(v32 a, v32 n) { v32 r; VF_NOVEC for (int i = 0; i < 8; i++) r.v[i] = a.v[i] << (n.v[i] & 31); return r; }
  // (a*b)>>32, unsigned, per lane.
  static VF_INLINE v32 mulhi_32(v32 a, v32 b) { v32 r; VF_NOVEC for (int i = 0; i < 8; i++) r.v[i] = u32(((u64)a.v[i] * b.v[i]) >> 32); return r; }
  static VF_INLINE void split_keys8(const u64* k, v32& lo, v32& hi) {
    VF_NOVEC for (int i = 0; i < 8; i++) { lo.v[i] = u32(k[i]); hi.v[i] = u32(k[i] >> 32); }
  }
  static VF_INLINE bool test_all_set_32(v32 block, v32 mask) {
    u32 acc = 0;
    VF_NOVEC for (int i = 0; i < 8; i++) acc |= (~block.v[i]) & mask.v[i];
    return acc == 0;
  }

  static VF_INLINE b512 load_512(const void* p) { b512 r; std::memcpy(r.v, p, 64); return r; }
  static VF_INLINE void store_512(void* p, b512 a) { std::memcpy(p, a.v, 64); }
  static VF_INLINE b512 or_512(b512 a, b512 b) { b512 r; VF_NOVEC for (int i = 0; i < 8; i++) r.v[i] = a.v[i] | b.v[i]; return r; }
  // pos holds 8 values in [0,64): one bit per u64 lane.
  static VF_INLINE b512 mask_512_from_pos(v32 pos) { b512 r; VF_NOVEC for (int i = 0; i < 8; i++) r.v[i] = u64(1) << (pos.v[i] & 63); return r; }
  static VF_INLINE bool test_all_set_512(b512 block, b512 mask) {
    u64 acc = 0;
    VF_NOVEC for (int i = 0; i < 8; i++) acc |= (~block.v[i]) & mask.v[i];
    return acc == 0;
  }

  // 128-bit: a bucket pair.
  static VF_INLINE v128 load2x64(const void* a, const void* b) { v128 r; std::memcpy(r.v, a, 8); std::memcpy(r.v + 8, b, 8); return r; }
  static VF_INLINE v128 load2x32(const void* a, const void* b) { v128 r; std::memset(r.v, 0, 16); std::memcpy(r.v, a, 4); std::memcpy(r.v + 4, b, 4); return r; }
  // Returns a bitmask over the 8 u16 lanes (bit i set iff lane i == needle).
  static VF_INLINE u32 match_mask_u16(v128 hay, u16 needle) {
    u32 m = 0;
    VF_NOVEC for (int i = 0; i < 8; i++) { u16 x; std::memcpy(&x, hay.v + 2 * i, 2); m |= u32(x == needle) << i; }
    return m;
  }
  static VF_INLINE u32 match_mask_u8(v128 hay, u8 needle) {
    u32 m = 0;
    VF_NOVEC for (int i = 0; i < 16; i++) m |= u32(hay.v[i] == needle) << i;
    return m;
  }
};

// ---------------------------------------------------------------------------
// AVX2 backend
// ---------------------------------------------------------------------------
#if VF_HAVE_AVX2
struct Avx2 {
  static constexpr const char* name = "avx2";
  static constexpr int kLanes32 = 8;

  using v32 = __m256i;
  struct b512 { __m256i lo, hi; };
  using v128 = __m128i;

  static VF_INLINE v32 set1_32(u32 x) { return _mm256_set1_epi32((int)x); }
  static VF_INLINE v32 loadu_32(const u32* p) { return _mm256_loadu_si256((const __m256i*)p); }
  static VF_INLINE void storeu_32(u32* p, v32 a) { _mm256_storeu_si256((__m256i*)p, a); }
  static VF_INLINE v32 add_32(v32 a, v32 b) { return _mm256_add_epi32(a, b); }
  static VF_INLINE v32 xor_32(v32 a, v32 b) { return _mm256_xor_si256(a, b); }
  static VF_INLINE v32 or_32(v32 a, v32 b) { return _mm256_or_si256(a, b); }
  static VF_INLINE v32 and_32(v32 a, v32 b) { return _mm256_and_si256(a, b); }
  static VF_INLINE v32 mullo_32(v32 a, v32 b) { return _mm256_mullo_epi32(a, b); }
  template <int N> static VF_INLINE v32 shr_32(v32 a) { return _mm256_srli_epi32(a, N); }
  static VF_INLINE v32 shl_var_32(v32 a, v32 n) { return _mm256_sllv_epi32(a, n); }  // precondition: n < 32
  static VF_INLINE v32 mulhi_32(v32 a, v32 b) {
    // AVX2 has no 32x32->high multiply; do even and odd lanes with mul_epu32 (32x32->64).
    __m256i even = _mm256_mul_epu32(a, b);                                   // lanes 0,2,4,6 -> 64-bit products
    __m256i odd  = _mm256_mul_epu32(_mm256_srli_epi64(a, 32), _mm256_srli_epi64(b, 32));
    __m256i even_hi = _mm256_srli_epi64(even, 32);                           // high halves in even u32 lanes
    return _mm256_blend_epi32(even_hi, odd, 0xAA);                           // odd products' high halves already sit in odd lanes
  }
  static VF_INLINE void split_keys8(const u64* k, v32& lo, v32& hi) {
    __m256i a = _mm256_loadu_si256((const __m256i*)k);      // k0lo k0hi k1lo k1hi | k2lo k2hi k3lo k3hi
    __m256i b = _mm256_loadu_si256((const __m256i*)(k + 4));
    __m256 fa = _mm256_castsi256_ps(a), fb = _mm256_castsi256_ps(b);
    __m256i l = _mm256_castps_si256(_mm256_shuffle_ps(fa, fb, _MM_SHUFFLE(2, 0, 2, 0)));  // k0 k1 k4 k5 | k2 k3 k6 k7
    __m256i h = _mm256_castps_si256(_mm256_shuffle_ps(fa, fb, _MM_SHUFFLE(3, 1, 3, 1)));
    lo = _mm256_permute4x64_epi64(l, _MM_SHUFFLE(3, 1, 2, 0));                             // k0..k7 in order
    hi = _mm256_permute4x64_epi64(h, _MM_SHUFFLE(3, 1, 2, 0));
  }
  static VF_INLINE bool test_all_set_32(v32 block, v32 mask) { return _mm256_testc_si256(block, mask) != 0; }

  static VF_INLINE b512 load_512(const void* p) {
    return { _mm256_load_si256((const __m256i*)p), _mm256_load_si256((const __m256i*)p + 1) };
  }
  static VF_INLINE void store_512(void* p, b512 a) {
    _mm256_store_si256((__m256i*)p, a.lo); _mm256_store_si256((__m256i*)p + 1, a.hi);
  }
  static VF_INLINE b512 or_512(b512 a, b512 b) { return { _mm256_or_si256(a.lo, b.lo), _mm256_or_si256(a.hi, b.hi) }; }
  static VF_INLINE b512 mask_512_from_pos(v32 pos) {
    __m256i one = _mm256_set1_epi64x(1);  // precondition: pos < 64
    __m256i plo = _mm256_cvtepu32_epi64(_mm256_castsi256_si128(pos));
    __m256i phi = _mm256_cvtepu32_epi64(_mm256_extracti128_si256(pos, 1));
    return { _mm256_sllv_epi64(one, plo), _mm256_sllv_epi64(one, phi) };
  }
  static VF_INLINE bool test_all_set_512(b512 block, b512 mask) {
    // (block & mask) == mask for both halves. testc(a,b) == ((~a & b) == 0).
    return (_mm256_testc_si256(block.lo, mask.lo) & _mm256_testc_si256(block.hi, mask.hi)) != 0;
  }

  static VF_INLINE v128 load2x64(const void* a, const void* b) {
    return _mm_unpacklo_epi64(_mm_loadl_epi64((const __m128i*)a), _mm_loadl_epi64((const __m128i*)b));
  }
  static VF_INLINE v128 load2x32(const void* a, const void* b) {
    return _mm_unpacklo_epi32(_mm_cvtsi32_si128(*(const int*)a), _mm_cvtsi32_si128(*(const int*)b));
  }
  static VF_INLINE u32 match_mask_u16(v128 hay, u16 needle) {
    __m128i eq = _mm_cmpeq_epi16(hay, _mm_set1_epi16((short)needle));
    return (u32)_mm_movemask_epi8(_mm_packs_epi16(eq, _mm_setzero_si128())) & 0xFF;
  }
  static VF_INLINE u32 match_mask_u8(v128 hay, u8 needle) {
    return (u32)_mm_movemask_epi8(_mm_cmpeq_epi8(hay, _mm_set1_epi8((char)needle)));
  }
};
#endif

// ---------------------------------------------------------------------------
// AVX-512 backend. Identical to AVX2 for v32/v128; the 64-byte block becomes a
// single zmm register and the membership test becomes one masked compare.
// ---------------------------------------------------------------------------
#if VF_HAVE_AVX512
struct Avx512 : Avx2 {
  static constexpr const char* name = "avx512";
  using b512 = __m512i;
  static VF_INLINE b512 load_512(const void* p) { return _mm512_load_si512(p); }
  static VF_INLINE void store_512(void* p, b512 a) { _mm512_store_si512(p, a); }
  static VF_INLINE b512 or_512(b512 a, b512 b) { return _mm512_or_si512(a, b); }
  static VF_INLINE b512 mask_512_from_pos(v32 pos) {
    return _mm512_sllv_epi64(_mm512_set1_epi64(1), _mm512_cvtepu32_epi64(pos));  // precondition: pos < 64
  }
  static VF_INLINE bool test_all_set_512(b512 block, b512 mask) {
    // bit i of the k-mask set iff lane i has (~block & mask) != 0, i.e. a required bit missing.
    return _mm512_test_epi64_mask(_mm512_andnot_si512(block, mask), mask) == 0;
  }
  // AVX-512-only: one zmm compares two 32-byte blocks (two keys) at once.
  static VF_INLINE u32 test2_blocks32(const void* blkA, const void* blkB, __m256i maskA, __m256i maskB) {
    __m512i blocks = _mm512_inserti64x4(_mm512_castsi256_si512(_mm256_load_si256((const __m256i*)blkA)),
                                        _mm256_load_si256((const __m256i*)blkB), 1);
    __m512i masks = _mm512_inserti64x4(_mm512_castsi256_si512(maskA), maskB, 1);
    __mmask16 missing = _mm512_test_epi32_mask(_mm512_andnot_si512(blocks, masks), masks);
    // returns bit0 = key A present, bit1 = key B present
    return (u32)(((missing & 0x00FF) == 0) | (((missing & 0xFF00) == 0) << 1));
  }
};
#endif

// ---------------------------------------------------------------------------
// NEON (AArch64) backend. A logical 256-bit vector is a pair of q-registers.
// ---------------------------------------------------------------------------
#if VF_HAVE_NEON
struct Neon {
  static constexpr const char* name = "neon";
  static constexpr int kLanes32 = 8;

  struct v32 { uint32x4_t a, b; };
  struct b512 { uint64x2_t v[4]; };
  using v128 = uint8x16_t;

  static VF_INLINE v32 set1_32(u32 x) { return { vdupq_n_u32(x), vdupq_n_u32(x) }; }
  static VF_INLINE v32 loadu_32(const u32* p) { return { vld1q_u32(p), vld1q_u32(p + 4) }; }
  static VF_INLINE void storeu_32(u32* p, v32 x) { vst1q_u32(p, x.a); vst1q_u32(p + 4, x.b); }
  static VF_INLINE v32 add_32(v32 x, v32 y) { return { vaddq_u32(x.a, y.a), vaddq_u32(x.b, y.b) }; }
  static VF_INLINE v32 xor_32(v32 x, v32 y) { return { veorq_u32(x.a, y.a), veorq_u32(x.b, y.b) }; }
  static VF_INLINE v32 or_32(v32 x, v32 y) { return { vorrq_u32(x.a, y.a), vorrq_u32(x.b, y.b) }; }
  static VF_INLINE v32 and_32(v32 x, v32 y) { return { vandq_u32(x.a, y.a), vandq_u32(x.b, y.b) }; }
  static VF_INLINE v32 mullo_32(v32 x, v32 y) { return { vmulq_u32(x.a, y.a), vmulq_u32(x.b, y.b) }; }
  template <int N> static VF_INLINE v32 shr_32(v32 x) { return { vshrq_n_u32(x.a, N), vshrq_n_u32(x.b, N) }; }
  static VF_INLINE v32 shl_var_32(v32 x, v32 n) {  // precondition: n < 32
    return { vshlq_u32(x.a, vreinterpretq_s32_u32(n.a)), vshlq_u32(x.b, vreinterpretq_s32_u32(n.b)) };
  }
  static VF_INLINE uint32x4_t mulhi4(uint32x4_t x, uint32x4_t y) {
    uint64x2_t lo = vmull_u32(vget_low_u32(x), vget_low_u32(y));
    uint64x2_t hi = vmull_high_u32(x, y);
    return vuzp2q_u32(vreinterpretq_u32_u64(lo), vreinterpretq_u32_u64(hi));  // odd 32-bit halves = high words
  }
  static VF_INLINE v32 mulhi_32(v32 x, v32 y) { return { mulhi4(x.a, y.a), mulhi4(x.b, y.b) }; }
  static VF_INLINE void split_keys8(const u64* k, v32& lo, v32& hi) {
    uint32x4_t k0 = vreinterpretq_u32_u64(vld1q_u64(k)), k1 = vreinterpretq_u32_u64(vld1q_u64(k + 2));
    uint32x4_t k2 = vreinterpretq_u32_u64(vld1q_u64(k + 4)), k3 = vreinterpretq_u32_u64(vld1q_u64(k + 6));
    lo = { vuzp1q_u32(k0, k1), vuzp1q_u32(k2, k3) };
    hi = { vuzp2q_u32(k0, k1), vuzp2q_u32(k2, k3) };
  }
  static VF_INLINE bool test_all_set_32(v32 block, v32 mask) {
    uint32x4_t ma = vbicq_u32(mask.a, block.a), mb = vbicq_u32(mask.b, block.b);  // mask & ~block
    return vmaxvq_u32(vorrq_u32(ma, mb)) == 0;
  }

  static VF_INLINE b512 load_512(const void* p) {
    const u64* q = (const u64*)p;
    return { { vld1q_u64(q), vld1q_u64(q + 2), vld1q_u64(q + 4), vld1q_u64(q + 6) } };
  }
  static VF_INLINE void store_512(void* p, b512 x) {
    u64* q = (u64*)p;
    vst1q_u64(q, x.v[0]); vst1q_u64(q + 2, x.v[1]); vst1q_u64(q + 4, x.v[2]); vst1q_u64(q + 6, x.v[3]);
  }
  static VF_INLINE b512 or_512(b512 x, b512 y) {
    return { { vorrq_u64(x.v[0], y.v[0]), vorrq_u64(x.v[1], y.v[1]), vorrq_u64(x.v[2], y.v[2]), vorrq_u64(x.v[3], y.v[3]) } };
  }
  static VF_INLINE b512 mask_512_from_pos(v32 pos) {  // precondition: pos < 64
    uint32x4_t pa = pos.a, pb = pos.b;
    uint64x2_t one = vdupq_n_u64(1);
    return { { vshlq_u64(one, vreinterpretq_s64_u64(vmovl_u32(vget_low_u32(pa)))),
               vshlq_u64(one, vreinterpretq_s64_u64(vmovl_high_u32(pa))),
               vshlq_u64(one, vreinterpretq_s64_u64(vmovl_u32(vget_low_u32(pb)))),
               vshlq_u64(one, vreinterpretq_s64_u64(vmovl_high_u32(pb))) } };
  }
  static VF_INLINE bool test_all_set_512(b512 block, b512 mask) {
    uint64x2_t acc = vorrq_u64(vorrq_u64(vbicq_u64(mask.v[0], block.v[0]), vbicq_u64(mask.v[1], block.v[1])),
                               vorrq_u64(vbicq_u64(mask.v[2], block.v[2]), vbicq_u64(mask.v[3], block.v[3])));
    return vmaxvq_u32(vreinterpretq_u32_u64(acc)) == 0;
  }

  static VF_INLINE v128 load2x64(const void* a, const void* b) {
    return vreinterpretq_u8_u64(vcombine_u64(vld1_u64((const u64*)a), vld1_u64((const u64*)b)));
  }
  static VF_INLINE v128 load2x32(const void* a, const void* b) {
    u32 x, y; std::memcpy(&x, a, 4); std::memcpy(&y, b, 4);
    uint32x4_t r = vdupq_n_u32(0);
    r = vsetq_lane_u32(x, r, 0); r = vsetq_lane_u32(y, r, 1);
    return vreinterpretq_u8_u32(r);
  }
  static VF_INLINE u32 match_mask_u16(v128 hay, u16 needle) {
    uint16x8_t eq = vceqq_u16(vreinterpretq_u16_u8(hay), vdupq_n_u16(needle));
    // Narrow 8 x u16 (0 or 0xFFFF) to 8 x u8 and pick one bit per lane.
    uint8x8_t n = vmovn_u16(eq);
    static const uint8_t bits[8] = { 1, 2, 4, 8, 16, 32, 64, 128 };
    return vaddv_u8(vand_u8(n, vld1_u8(bits)));
  }
  static VF_INLINE u32 match_mask_u8(v128 hay, u8 needle) {
    uint8x16_t eq = vceqq_u8(hay, vdupq_n_u8(needle));
    static const uint8_t bits[16] = { 1, 2, 4, 8, 16, 32, 64, 128, 1, 2, 4, 8, 16, 32, 64, 128 };
    uint8x16_t m = vandq_u8(eq, vld1q_u8(bits));
    return (u32)vaddv_u8(vget_low_u8(m)) | ((u32)vaddv_u8(vget_high_u8(m)) << 8);
  }
};
#endif

// The best backend this translation unit was compiled for.
#if VF_HAVE_AVX512
using BestOps = Avx512;
#elif VF_HAVE_AVX2
using BestOps = Avx2;
#elif VF_HAVE_NEON
using BestOps = Neon;
#else
using BestOps = Portable;
#endif

}  // namespace vf
