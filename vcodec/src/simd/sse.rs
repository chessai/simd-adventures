//! SSSE3 + SSE4.1 backend (128-bit, one lane).

use super::Simd;
use crate::backend::Sse41;
use core::arch::x86_64::*;

impl Simd for Sse41 {
    type V = __m128i;
    const BYTES: usize = 16;
    const NAME: &'static str = "sse41";
    const B64_ENC_READ: usize = 16;

    #[inline(always)]
    unsafe fn loadu(p: *const u8) -> __m128i {
        _mm_loadu_si128(p as *const __m128i)
    }
    #[inline(always)]
    unsafe fn storeu(p: *mut u8, v: __m128i) {
        _mm_storeu_si128(p as *mut __m128i, v)
    }
    #[inline(always)]
    unsafe fn splat(b: u8) -> __m128i {
        _mm_set1_epi8(b as i8)
    }
    #[inline(always)]
    unsafe fn splat_u16(w: u16) -> __m128i {
        _mm_set1_epi16(w as i16)
    }
    #[inline(always)]
    unsafe fn splat_u32(d: u32) -> __m128i {
        _mm_set1_epi32(d as i32)
    }
    #[inline(always)]
    unsafe fn from_lane(lane: [u8; 16]) -> __m128i {
        _mm_loadu_si128(lane.as_ptr() as *const __m128i)
    }
    #[inline(always)]
    unsafe fn zero() -> __m128i {
        _mm_setzero_si128()
    }
    #[inline(always)]
    unsafe fn and(a: __m128i, b: __m128i) -> __m128i {
        _mm_and_si128(a, b)
    }
    #[inline(always)]
    unsafe fn or(a: __m128i, b: __m128i) -> __m128i {
        _mm_or_si128(a, b)
    }
    #[inline(always)]
    unsafe fn xor(a: __m128i, b: __m128i) -> __m128i {
        _mm_xor_si128(a, b)
    }
    #[inline(always)]
    unsafe fn andnot(a: __m128i, b: __m128i) -> __m128i {
        // x86 negates the *first* operand.
        _mm_andnot_si128(b, a)
    }
    #[inline(always)]
    unsafe fn add_u8(a: __m128i, b: __m128i) -> __m128i {
        _mm_add_epi8(a, b)
    }
    #[inline(always)]
    unsafe fn sub_u8(a: __m128i, b: __m128i) -> __m128i {
        _mm_sub_epi8(a, b)
    }
    #[inline(always)]
    unsafe fn subs_u8(a: __m128i, b: __m128i) -> __m128i {
        _mm_subs_epu8(a, b)
    }
    #[inline(always)]
    unsafe fn adds_u8(a: __m128i, b: __m128i) -> __m128i {
        _mm_adds_epu8(a, b)
    }
    #[inline(always)]
    unsafe fn cmpeq_u8(a: __m128i, b: __m128i) -> __m128i {
        _mm_cmpeq_epi8(a, b)
    }
    #[inline(always)]
    unsafe fn cmpgt_i8(a: __m128i, b: __m128i) -> __m128i {
        _mm_cmpgt_epi8(a, b)
    }
    #[inline(always)]
    unsafe fn cmpgt_u8(a: __m128i, b: __m128i) -> __m128i {
        // a > b  <=>  max(a,b) != b
        let le = _mm_cmpeq_epi8(_mm_max_epu8(a, b), b);
        _mm_xor_si128(le, _mm_set1_epi8(-1))
    }
    #[inline(always)]
    unsafe fn shr_u16<const N: i32>(v: __m128i) -> __m128i {
        _mm_srli_epi16::<N>(v)
    }
    #[inline(always)]
    unsafe fn shl_u16<const N: i32>(v: __m128i) -> __m128i {
        _mm_slli_epi16::<N>(v)
    }
    #[inline(always)]
    unsafe fn shr_u32<const N: i32>(v: __m128i) -> __m128i {
        _mm_srli_epi32::<N>(v)
    }
    #[inline(always)]
    unsafe fn shl_u32<const N: i32>(v: __m128i) -> __m128i {
        _mm_slli_epi32::<N>(v)
    }
    #[inline(always)]
    unsafe fn mulhi_u16(a: __m128i, b: __m128i) -> __m128i {
        // See avx2.rs: pin the instruction, LLVM does not reliably form pmulhuw.
        let mut r = a;
        #[cfg(target_feature = "avx")]
        core::arch::asm!("vpmulhuw {r}, {r}, {b}", r = inout(xmm_reg) r, b = in(xmm_reg) b,
            options(pure, nomem, nostack, preserves_flags));
        #[cfg(not(target_feature = "avx"))]
        core::arch::asm!("pmulhuw {r}, {b}", r = inout(xmm_reg) r, b = in(xmm_reg) b,
            options(pure, nomem, nostack, preserves_flags));
        r
    }
    #[inline(always)]
    unsafe fn mullo_u16(a: __m128i, b: __m128i) -> __m128i {
        _mm_mullo_epi16(a, b)
    }
    #[inline(always)]
    unsafe fn madd_u8_pairs(a: __m128i, me: u8, mo: u8) -> __m128i {
        debug_assert!(me < 128 && mo < 128);
        _mm_maddubs_epi16(a, _mm_set1_epi16(((mo as u16) << 8 | me as u16) as i16))
    }
    #[inline(always)]
    unsafe fn madd_u16_pairs(a: __m128i, me: u16, mo: u16) -> __m128i {
        debug_assert!(me < 32768 && mo < 32768);
        _mm_madd_epi16(a, _mm_set1_epi32(((mo as u32) << 16 | me as u32) as i32))
    }
    #[inline(always)]
    unsafe fn shuffle_bytes(table: __m128i, idx: __m128i) -> __m128i {
        _mm_shuffle_epi8(table, idx)
    }
    #[inline(always)]
    unsafe fn alignr<const N: i32>(hi: __m128i, lo: __m128i) -> __m128i {
        _mm_alignr_epi8::<N>(hi, lo)
    }
    #[inline(always)]
    unsafe fn interleave(a: __m128i, b: __m128i) -> (__m128i, __m128i) {
        (_mm_unpacklo_epi8(a, b), _mm_unpackhi_epi8(a, b))
    }
    #[inline(always)]
    unsafe fn pack_u16_u8(a: __m128i, b: __m128i) -> __m128i {
        _mm_packus_epi16(a, b)
    }
    #[inline(always)]
    unsafe fn movemask(v: __m128i) -> u32 {
        _mm_movemask_epi8(v) as u32
    }
    #[inline(always)]
    unsafe fn is_zero(v: __m128i) -> bool {
        _mm_testz_si128(v, v) != 0
    }
    #[inline(always)]
    unsafe fn load_b64_enc(p: *const u8) -> __m128i {
        Self::loadu(p)
    }
    #[inline(always)]
    unsafe fn store_b64_dec(p: *mut u8, v: __m128i) {
        Self::storeu(p, v)
    }
    #[inline(always)]
    unsafe fn lane_lo_u64(v: __m128i, _lane: usize) -> u64 {
        _mm_cvtsi128_si64(v) as u64
    }
    #[inline(always)]
    unsafe fn lane_hi_u64(v: __m128i, _lane: usize) -> u64 {
        _mm_extract_epi64::<1>(v) as u64
    }
    #[inline(always)]
    unsafe fn lane(v: __m128i, _lane: usize) -> __m128i {
        v
    }
    #[inline(always)]
    unsafe fn store_lane0(p: *mut u8, v: __m128i) {
        Self::storeu(p, v)
    }
}
