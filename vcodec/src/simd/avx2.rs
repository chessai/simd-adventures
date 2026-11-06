//! AVX2 backend (256-bit, two 128-bit lanes).
//!
//! Everything that AVX2 does per-128-bit-lane (`vpshufb`, `vpunpck*`,
//! `vpalignr`, `vpackus*`) maps directly onto the trait's lane model; the few
//! full-width-order methods (`interleave`, `pack_u16_u8`, base64 lane
//! load/store) pay one `vperm2i128` / `vpermq` / `vpermd` to fix up order.

use super::Simd;
use crate::backend::Avx2;
use core::arch::x86_64::*;

/// `asm!` with `ymm_reg` operands needs the `avx` feature on the enclosing fn,
/// so this lives in its own `#[target_feature]` helper; it inlines into the
/// AVX2 dispatch wrapper.
#[target_feature(enable = "avx2")]
#[inline]
unsafe fn mulhi_asm(a: __m256i, b: __m256i) -> __m256i {
    let r: __m256i;
    core::arch::asm!("vpmulhuw {r}, {a}, {b}", r = lateout(ymm_reg) r, a = in(ymm_reg) a, b = in(ymm_reg) b,
        options(pure, nomem, nostack, preserves_flags));
    r
}

impl Simd for Avx2 {
    type V = __m256i;
    const BYTES: usize = 32;
    const NAME: &'static str = "avx2";
    /// Two 16-byte loads at `p` and `p + 12`.
    const B64_ENC_READ: usize = 28;

    #[inline(always)]
    unsafe fn loadu(p: *const u8) -> __m256i {
        _mm256_loadu_si256(p as *const __m256i)
    }
    #[inline(always)]
    unsafe fn storeu(p: *mut u8, v: __m256i) {
        _mm256_storeu_si256(p as *mut __m256i, v)
    }
    #[inline(always)]
    unsafe fn splat(b: u8) -> __m256i {
        _mm256_set1_epi8(b as i8)
    }
    #[inline(always)]
    unsafe fn splat_u16(w: u16) -> __m256i {
        _mm256_set1_epi16(w as i16)
    }
    #[inline(always)]
    unsafe fn splat_u32(d: u32) -> __m256i {
        _mm256_set1_epi32(d as i32)
    }
    #[inline(always)]
    unsafe fn from_lane(lane: [u8; 16]) -> __m256i {
        _mm256_broadcastsi128_si256(_mm_loadu_si128(lane.as_ptr() as *const __m128i))
    }
    #[inline(always)]
    unsafe fn zero() -> __m256i {
        _mm256_setzero_si256()
    }
    #[inline(always)]
    unsafe fn and(a: __m256i, b: __m256i) -> __m256i {
        _mm256_and_si256(a, b)
    }
    #[inline(always)]
    unsafe fn or(a: __m256i, b: __m256i) -> __m256i {
        _mm256_or_si256(a, b)
    }
    #[inline(always)]
    unsafe fn xor(a: __m256i, b: __m256i) -> __m256i {
        _mm256_xor_si256(a, b)
    }
    #[inline(always)]
    unsafe fn andnot(a: __m256i, b: __m256i) -> __m256i {
        _mm256_andnot_si256(b, a)
    }
    #[inline(always)]
    unsafe fn add_u8(a: __m256i, b: __m256i) -> __m256i {
        _mm256_add_epi8(a, b)
    }
    #[inline(always)]
    unsafe fn sub_u8(a: __m256i, b: __m256i) -> __m256i {
        _mm256_sub_epi8(a, b)
    }
    #[inline(always)]
    unsafe fn subs_u8(a: __m256i, b: __m256i) -> __m256i {
        _mm256_subs_epu8(a, b)
    }
    #[inline(always)]
    unsafe fn adds_u8(a: __m256i, b: __m256i) -> __m256i {
        _mm256_adds_epu8(a, b)
    }
    #[inline(always)]
    unsafe fn cmpeq_u8(a: __m256i, b: __m256i) -> __m256i {
        _mm256_cmpeq_epi8(a, b)
    }
    #[inline(always)]
    unsafe fn cmpgt_i8(a: __m256i, b: __m256i) -> __m256i {
        _mm256_cmpgt_epi8(a, b)
    }
    #[inline(always)]
    unsafe fn cmpgt_u8(a: __m256i, b: __m256i) -> __m256i {
        let le = _mm256_cmpeq_epi8(_mm256_max_epu8(a, b), b);
        _mm256_xor_si256(le, _mm256_set1_epi8(-1))
    }
    #[inline(always)]
    unsafe fn shr_u16<const N: i32>(v: __m256i) -> __m256i {
        _mm256_srli_epi16::<N>(v)
    }
    #[inline(always)]
    unsafe fn shl_u16<const N: i32>(v: __m256i) -> __m256i {
        _mm256_slli_epi16::<N>(v)
    }
    #[inline(always)]
    unsafe fn shr_u32<const N: i32>(v: __m256i) -> __m256i {
        _mm256_srli_epi32::<N>(v)
    }
    #[inline(always)]
    unsafe fn shl_u32<const N: i32>(v: __m256i) -> __m256i {
        _mm256_slli_epi32::<N>(v)
    }
    #[inline(always)]
    unsafe fn mulhi_u16(a: __m256i, b: __m256i) -> __m256i {
        // `_mm256_mulhi_epu16` is a generic zext/mul/shr/trunc in stdarch, and
        // LLVM (as of 1.97) fails to re-match it into one `vpmulhuw`, emitting a
        // zero-extend / 32-bit multiply / pack sequence instead. Inline asm pins
        // the instruction (see REPORT.md, "compiler fights").
        mulhi_asm(a, b)
    }
    #[inline(always)]
    unsafe fn mullo_u16(a: __m256i, b: __m256i) -> __m256i {
        _mm256_mullo_epi16(a, b)
    }
    #[inline(always)]
    unsafe fn madd_u8_pairs(a: __m256i, me: u8, mo: u8) -> __m256i {
        debug_assert!(me < 128 && mo < 128);
        _mm256_maddubs_epi16(a, _mm256_set1_epi16(((mo as u16) << 8 | me as u16) as i16))
    }
    #[inline(always)]
    unsafe fn madd_u16_pairs(a: __m256i, me: u16, mo: u16) -> __m256i {
        debug_assert!(me < 32768 && mo < 32768);
        _mm256_madd_epi16(a, _mm256_set1_epi32(((mo as u32) << 16 | me as u32) as i32))
    }
    #[inline(always)]
    unsafe fn shuffle_bytes(table: __m256i, idx: __m256i) -> __m256i {
        _mm256_shuffle_epi8(table, idx)
    }
    #[inline(always)]
    unsafe fn alignr<const N: i32>(hi: __m256i, lo: __m256i) -> __m256i {
        _mm256_alignr_epi8::<N>(hi, lo)
    }
    #[inline(always)]
    unsafe fn interleave(a: __m256i, b: __m256i) -> (__m256i, __m256i) {
        let t0 = _mm256_unpacklo_epi8(a, b); // lane0: a0..7/b0..7   lane1: a16..23/b16..23
        let t1 = _mm256_unpackhi_epi8(a, b); // lane0: a8..15/b8..15 lane1: a24..31/b24..31
        (
            _mm256_permute2x128_si256::<0x20>(t0, t1),
            _mm256_permute2x128_si256::<0x31>(t0, t1),
        )
    }
    #[inline(always)]
    unsafe fn pack_u16_u8(a: __m256i, b: __m256i) -> __m256i {
        // vpackuswb gives [a0-7, b0-7 | a8-15, b8-15]; vpermq(0xD8) fixes it.
        _mm256_permute4x64_epi64::<0xD8>(_mm256_packus_epi16(a, b))
    }
    #[inline(always)]
    unsafe fn movemask(v: __m256i) -> u32 {
        _mm256_movemask_epi8(v) as u32
    }
    #[inline(always)]
    unsafe fn is_zero(v: __m256i) -> bool {
        _mm256_testz_si256(v, v) != 0
    }
    #[inline(always)]
    unsafe fn load_b64_enc(p: *const u8) -> __m256i {
        let lo = _mm_loadu_si128(p as *const __m128i);
        let hi = _mm_loadu_si128(p.add(12) as *const __m128i);
        _mm256_inserti128_si256::<1>(_mm256_castsi128_si256(lo), hi)
    }
    #[inline(always)]
    unsafe fn store_b64_dec(p: *mut u8, v: __m256i) {
        // lanes: [12 valid, 4 zero | 12 valid, 4 zero] -> [24 valid, 8 junk]
        let idx = _mm256_setr_epi32(0, 1, 2, 4, 5, 6, 3, 7);
        _mm256_storeu_si256(p as *mut __m256i, _mm256_permutevar8x32_epi32(v, idx))
    }
    #[inline(always)]
    unsafe fn lane_lo_u64(v: __m256i, lane: usize) -> u64 {
        if lane == 0 {
            _mm256_extract_epi64::<0>(v) as u64
        } else {
            _mm256_extract_epi64::<2>(v) as u64
        }
    }
    #[inline(always)]
    unsafe fn lane_hi_u64(v: __m256i, lane: usize) -> u64 {
        if lane == 0 {
            _mm256_extract_epi64::<1>(v) as u64
        } else {
            _mm256_extract_epi64::<3>(v) as u64
        }
    }
    #[inline(always)]
    unsafe fn lane(v: __m256i, lane: usize) -> __m256i {
        if lane == 0 {
            v
        } else {
            _mm256_permute2x128_si256::<0x11>(v, v)
        }
    }
    #[inline(always)]
    unsafe fn store_lane0(p: *mut u8, v: __m256i) {
        _mm_storeu_si128(p as *mut __m128i, _mm256_castsi256_si128(v))
    }
    #[inline(always)]
    unsafe fn store_lane(p: *mut u8, v: __m256i, lane: usize) {
        if lane == 0 {
            _mm_storeu_si128(p as *mut __m128i, _mm256_castsi256_si128(v))
        } else {
            _mm_storeu_si128(p as *mut __m128i, _mm256_extracti128_si256::<1>(v))
        }
    }
    #[inline(always)]
    unsafe fn from_lane_pair(l0: [u8; 16], l1: [u8; 16]) -> __m256i {
        let a = _mm_loadu_si128(l0.as_ptr() as *const __m128i);
        let b = _mm_loadu_si128(l1.as_ptr() as *const __m128i);
        _mm256_inserti128_si256::<1>(_mm256_castsi128_si256(a), b)
    }
}
