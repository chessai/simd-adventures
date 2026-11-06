// vf/hash.h -- key hashing, written once in terms of 32-bit multiply/xor/shift
// so the same function can run per-key (scalar) and 8-keys-at-a-time (vector)
// and give bit-identical results.  A 64-bit key is split into two 32-bit
// halves; two 32-bit outputs are produced:
//   hb  -> selects the block (Bloom) or bucket (cuckoo)
//   hl  -> seeds the per-lane bit positions (Bloom) or fingerprint (cuckoo)
//
// Why not a 64-bit mixer (wyhash/murmur fmix64)?  AVX2 has no 64x64->64
// multiply (vpmullq is AVX-512DQ) and NEON has none either, so a 64-bit mixer
// would need a 4-instruction emulation per multiply.  32-bit multiplies are
// native on both.  See REPORT.md for the AVX-512 discussion.
#pragma once
#include "config.h"
#include "simd.h"

namespace vf {

namespace hc {
constexpr u32 FOLD_A = 0x85EBCA77u;
constexpr u32 FOLD_B = 0xC2B2AE3Du;
constexpr u32 M1 = 0x7FEB352Du, M2 = 0x846CA68Bu;   // lowbias32
constexpr u32 M3 = 0xED5AD4BBu, M4 = 0xAC4C1B51u;   // triple32 (first two rounds)
}  // namespace hc

VF_INLINE void hash1(u64 key, u32& hb, u32& hl) {
  u32 lo = u32(key), hi = u32(key >> 32);
  u32 a = lo ^ (hi * hc::FOLD_A);
  a ^= a >> 16; a *= hc::M1; a ^= a >> 15; a *= hc::M2; a ^= a >> 16;
  u32 b = hi ^ (lo * hc::FOLD_B) ^ a;
  b ^= b >> 17; b *= hc::M3; b ^= b >> 11; b *= hc::M4; b ^= b >> 15;
  hb = a; hl = b;
}

template <class Ops>
VF_INLINE void hash8(typename Ops::v32 lo, typename Ops::v32 hi, typename Ops::v32& hb, typename Ops::v32& hl) {
  using V = typename Ops::v32;
  V a = Ops::xor_32(lo, Ops::mullo_32(hi, Ops::set1_32(hc::FOLD_A)));
  a = Ops::xor_32(a, Ops::template shr_32<16>(a));
  a = Ops::mullo_32(a, Ops::set1_32(hc::M1));
  a = Ops::xor_32(a, Ops::template shr_32<15>(a));
  a = Ops::mullo_32(a, Ops::set1_32(hc::M2));
  a = Ops::xor_32(a, Ops::template shr_32<16>(a));
  V b = Ops::xor_32(Ops::xor_32(hi, Ops::mullo_32(lo, Ops::set1_32(hc::FOLD_B))), a);
  b = Ops::xor_32(b, Ops::template shr_32<17>(b));
  b = Ops::mullo_32(b, Ops::set1_32(hc::M3));
  b = Ops::xor_32(b, Ops::template shr_32<11>(b));
  b = Ops::mullo_32(b, Ops::set1_32(hc::M4));
  b = Ops::xor_32(b, Ops::template shr_32<15>(b));
  hb = a; hl = b;
}

// Hash 8 keys from memory: split halves, mix.
template <class Ops>
VF_INLINE void hash8_keys(const u64* keys, typename Ops::v32& hb, typename Ops::v32& hl) {
  typename Ops::v32 lo, hi;
  Ops::split_keys8(keys, lo, hi);
  hash8<Ops>(lo, hi, hb, hl);
}

// Lemire's fast range reduction: maps a uniform 32-bit value to [0, n).
VF_INLINE u32 fastrange32(u32 x, u32 n) { return u32(((u64)x * n) >> 32); }

// Salts for the per-lane multiply-shift (one odd constant per lane).  These
// are the constants used by Impala/Parquet split-block Bloom filters.
alignas(32) constexpr u32 kLaneSalts[8] = { 0x47b6137bu, 0x44974d91u, 0x8824ad5bu, 0xa2b7289du,
                                            0x705495c7u, 0x2df1424bu, 0x9efc4947u, 0x5c6bfb31u };
alignas(32) constexpr u32 kLaneSalts2[8] = { 0x6a7f2d6bu, 0x3b9d1e3du, 0x9c3f1b27u, 0x58f4a8c5u,
                                             0xd1c6e3f9u, 0x1f0b39b3u, 0x7e9d6a71u, 0xb3e27f05u };

}  // namespace vf
