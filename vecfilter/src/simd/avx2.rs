//! AVX2 backend. An `Avx2` token exists only if the CPU supports AVX2 (+BMI2), which is the
//! safety argument for every `unsafe` intrinsic call below.

use super::Ops;
use crate::backend::Avx2;
use core::arch::x86_64::*;

#[derive(Clone, Copy)]
pub struct B512x2 {
    pub lo: __m256i,
    pub hi: __m256i,
}

/// The V32 and V128 halves of the AVX2 backend, shared with AVX-512 (which in the C++ inherits
/// them from `Avx2`).
macro_rules! avx2_v32_v128 {
    () => {
        #[inline(always)]
        fn set1_32(self, x: u32) -> __m256i {
            unsafe { _mm256_set1_epi32(x as i32) }
        }
        #[inline(always)]
        unsafe fn loadu_32(self, p: *const u32) -> __m256i {
            unsafe { _mm256_loadu_si256(p.cast()) }
        }
        #[inline(always)]
        unsafe fn storeu_32(self, p: *mut u32, a: __m256i) {
            unsafe { _mm256_storeu_si256(p.cast(), a) }
        }
        #[inline(always)]
        fn add_32(self, a: __m256i, b: __m256i) -> __m256i {
            unsafe { _mm256_add_epi32(a, b) }
        }
        #[inline(always)]
        fn xor_32(self, a: __m256i, b: __m256i) -> __m256i {
            unsafe { _mm256_xor_si256(a, b) }
        }
        #[inline(always)]
        fn or_32(self, a: __m256i, b: __m256i) -> __m256i {
            unsafe { _mm256_or_si256(a, b) }
        }
        #[inline(always)]
        fn and_32(self, a: __m256i, b: __m256i) -> __m256i {
            unsafe { _mm256_and_si256(a, b) }
        }
        #[inline(always)]
        fn mullo_32(self, a: __m256i, b: __m256i) -> __m256i {
            unsafe { _mm256_mullo_epi32(a, b) }
        }
        #[inline(always)]
        fn shr_32<const N: i32>(self, a: __m256i) -> __m256i {
            unsafe { _mm256_srli_epi32::<N>(a) }
        }
        #[inline(always)]
        fn shl_var_32(self, a: __m256i, n: __m256i) -> __m256i {
            unsafe { _mm256_sllv_epi32(a, n) } // precondition: n < 32
        }
        #[inline(always)]
        fn mulhi_32(self, a: __m256i, b: __m256i) -> __m256i {
            // AVX2 has no 32x32->high multiply; do even and odd lanes with mul_epu32 (32x32->64).
            unsafe {
                let even = _mm256_mul_epu32(a, b); // lanes 0,2,4,6 -> 64-bit products
                let odd = _mm256_mul_epu32(_mm256_srli_epi64::<32>(a), _mm256_srli_epi64::<32>(b));
                let even_hi = _mm256_srli_epi64::<32>(even); // high halves in even u32 lanes
                _mm256_blend_epi32::<0xAA>(even_hi, odd) // odd products' high halves already sit in odd lanes
            }
        }
        #[inline(always)]
        fn split_keys8(self, k: &[u64; 8]) -> (__m256i, __m256i) {
            unsafe {
                let a = _mm256_loadu_si256(k.as_ptr().cast()); // k0lo k0hi k1lo k1hi | k2lo k2hi k3lo k3hi
                let b = _mm256_loadu_si256(k.as_ptr().add(4).cast());
                let (fa, fb) = (_mm256_castsi256_ps(a), _mm256_castsi256_ps(b));
                let l = _mm256_castps_si256(_mm256_shuffle_ps::<0x88>(fa, fb)); // _MM_SHUFFLE(2,0,2,0): k0 k1 k4 k5 | k2 k3 k6 k7
                let h = _mm256_castps_si256(_mm256_shuffle_ps::<0xDD>(fa, fb)); // _MM_SHUFFLE(3,1,3,1)
                (
                    _mm256_permute4x64_epi64::<0xD8>(l), // _MM_SHUFFLE(3,1,2,0): k0..k7 in order
                    _mm256_permute4x64_epi64::<0xD8>(h),
                )
            }
        }
        #[inline(always)]
        fn test_all_set_32(self, block: __m256i, mask: __m256i) -> bool {
            unsafe { _mm256_testc_si256(block, mask) != 0 }
        }

        #[inline(always)]
        unsafe fn load2x64(self, a: *const u8, b: *const u8) -> __m128i {
            unsafe { _mm_unpacklo_epi64(_mm_loadl_epi64(a.cast()), _mm_loadl_epi64(b.cast())) }
        }
        #[inline(always)]
        unsafe fn load2x32(self, a: *const u8, b: *const u8) -> __m128i {
            unsafe {
                _mm_unpacklo_epi32(
                    _mm_cvtsi32_si128(a.cast::<i32>().read_unaligned()),
                    _mm_cvtsi32_si128(b.cast::<i32>().read_unaligned()),
                )
            }
        }
        #[inline(always)]
        fn match_mask_u16(self, hay: __m128i, needle: u16) -> u32 {
            unsafe {
                let eq = _mm_cmpeq_epi16(hay, _mm_set1_epi16(needle as i16));
                (_mm_movemask_epi8(_mm_packs_epi16(eq, _mm_setzero_si128())) as u32) & 0xFF
            }
        }
        #[inline(always)]
        fn match_mask_u8(self, hay: __m128i, needle: u8) -> u32 {
            unsafe { _mm_movemask_epi8(_mm_cmpeq_epi8(hay, _mm_set1_epi8(needle as i8))) as u32 }
        }
    };
}
pub(super) use avx2_v32_v128;

impl Ops for Avx2 {
    const NAME: &'static str = "avx2";
    type V32 = __m256i;
    type B512 = B512x2;
    type V128 = __m128i;

    avx2_v32_v128!();

    #[inline(always)]
    unsafe fn load_512(self, p: *const u8) -> B512x2 {
        unsafe { B512x2 { lo: _mm256_load_si256(p.cast()), hi: _mm256_load_si256(p.add(32).cast()) } }
    }
    #[inline(always)]
    unsafe fn store_512(self, p: *mut u8, a: B512x2) {
        unsafe {
            _mm256_store_si256(p.cast(), a.lo);
            _mm256_store_si256(p.add(32).cast(), a.hi);
        }
    }
    #[inline(always)]
    fn or_512(self, a: B512x2, b: B512x2) -> B512x2 {
        unsafe { B512x2 { lo: _mm256_or_si256(a.lo, b.lo), hi: _mm256_or_si256(a.hi, b.hi) } }
    }
    #[inline(always)]
    fn mask_512_from_pos(self, pos: __m256i) -> B512x2 {
        unsafe {
            let one = _mm256_set1_epi64x(1); // precondition: pos < 64
            let plo = _mm256_cvtepu32_epi64(_mm256_castsi256_si128(pos));
            let phi = _mm256_cvtepu32_epi64(_mm256_extracti128_si256::<1>(pos));
            B512x2 { lo: _mm256_sllv_epi64(one, plo), hi: _mm256_sllv_epi64(one, phi) }
        }
    }
    #[inline(always)]
    fn test_all_set_512(self, block: B512x2, mask: B512x2) -> bool {
        // (block & mask) == mask for both halves. testc(a,b) == ((~a & b) == 0).
        unsafe { (_mm256_testc_si256(block.lo, mask.lo) & _mm256_testc_si256(block.hi, mask.hi)) != 0 }
    }
}
