//! AVX-512 backend. Identical to AVX2 for V32/V128; the 64-byte block becomes a single zmm
//! register and the membership test becomes one masked compare. An `Avx512` token exists only if
//! the CPU supports AVX2 and AVX-512 F/BW/DQ/VL.

use super::Ops;
use super::avx2::avx2_v32_v128;
use crate::backend::Avx512;
use core::arch::x86_64::*;

impl Ops for Avx512 {
    const NAME: &'static str = "avx512";
    type V32 = __m256i;
    type B512 = __m512i;
    type V128 = __m128i;

    avx2_v32_v128!();

    #[inline(always)]
    unsafe fn load_512(self, p: *const u8) -> __m512i {
        unsafe { _mm512_load_si512(p.cast()) }
    }
    #[inline(always)]
    unsafe fn store_512(self, p: *mut u8, a: __m512i) {
        unsafe { _mm512_store_si512(p.cast(), a) }
    }
    #[inline(always)]
    fn or_512(self, a: __m512i, b: __m512i) -> __m512i {
        unsafe { _mm512_or_si512(a, b) }
    }
    #[inline(always)]
    fn mask_512_from_pos(self, pos: __m256i) -> __m512i {
        // precondition: pos < 64
        unsafe { _mm512_sllv_epi64(_mm512_set1_epi64(1), _mm512_cvtepu32_epi64(pos)) }
    }
    #[inline(always)]
    fn test_all_set_512(self, block: __m512i, mask: __m512i) -> bool {
        // bit i of the k-mask set iff lane i has (~block & mask) != 0, i.e. a required bit missing.
        unsafe { _mm512_test_epi64_mask(_mm512_andnot_si512(block, mask), mask) == 0 }
    }

    const HAS_TEST2_BLOCKS32: bool = true;
    /// One zmm compares two 32-byte blocks (two keys) at once.
    #[inline(always)]
    unsafe fn test2_blocks32(self, a: *const u8, b: *const u8, ma: __m256i, mb: __m256i) -> u32 {
        unsafe {
            let blocks = _mm512_inserti64x4::<1>(
                _mm512_castsi256_si512(_mm256_load_si256(a.cast())),
                _mm256_load_si256(b.cast()),
            );
            let masks = _mm512_inserti64x4::<1>(_mm512_castsi256_si512(ma), mb);
            let missing = _mm512_test_epi32_mask(_mm512_andnot_si512(blocks, masks), masks);
            // bit0 = key A present, bit1 = key B present
            ((missing & 0x00FF) == 0) as u32 | ((((missing & 0xFF00) == 0) as u32) << 1)
        }
    }
}
