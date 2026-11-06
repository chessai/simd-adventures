//! The vector operations this experiment needs, one implementation per backend token.
//!
//! Methods take the token (`self`) so they are safe: the token proves the CPU supports them.
//! They are `#[inline(always)]` so they compile into the caller's [`simdlab::Token::vectorize`]
//! context with its target features.

use crate::backend::{Portable, StdSimd};
use simdlab::Token;
use std::simd::num::SimdUint;
use std::simd::u32x8;

pub trait SumOps: Token {
    /// 8 lanes of u32.
    type V: Copy;
    fn zero(self) -> Self::V;
    fn load(self, x: &[u32; 8]) -> Self::V;
    fn add(self, a: Self::V, b: Self::V) -> Self::V;
    /// Wrapping sum of the 8 lanes.
    fn reduce(self, v: Self::V) -> u32;
}

impl SumOps for Portable {
    type V = [u32; 8];
    #[inline(always)]
    fn zero(self) -> [u32; 8] {
        [0; 8]
    }
    #[inline(always)]
    fn load(self, x: &[u32; 8]) -> [u32; 8] {
        *x
    }
    #[inline(always)]
    fn add(self, a: [u32; 8], b: [u32; 8]) -> [u32; 8] {
        std::array::from_fn(|i| a[i].wrapping_add(b[i]))
    }
    #[inline(always)]
    fn reduce(self, v: [u32; 8]) -> u32 {
        v.iter().fold(0, |s, &x| s.wrapping_add(x))
    }
}

impl SumOps for StdSimd {
    type V = u32x8;
    #[inline(always)]
    fn zero(self) -> u32x8 {
        u32x8::splat(0)
    }
    #[inline(always)]
    fn load(self, x: &[u32; 8]) -> u32x8 {
        u32x8::from_array(*x)
    }
    #[inline(always)]
    fn add(self, a: u32x8, b: u32x8) -> u32x8 {
        a + b // wrapping, like every std::simd integer op
    }
    #[inline(always)]
    fn reduce(self, v: u32x8) -> u32 {
        v.reduce_sum()
    }
}

#[cfg(target_arch = "x86_64")]
mod avx2 {
    use super::SumOps;
    use crate::backend::Avx2;
    use core::arch::x86_64::*;

    // SAFETY (every block below): an `Avx2` token exists only if the CPU supports AVX2.
    impl SumOps for Avx2 {
        type V = __m256i;
        #[inline(always)]
        fn zero(self) -> __m256i {
            unsafe { _mm256_setzero_si256() }
        }
        #[inline(always)]
        fn load(self, x: &[u32; 8]) -> __m256i {
            unsafe { _mm256_loadu_si256(x.as_ptr().cast()) }
        }
        #[inline(always)]
        fn add(self, a: __m256i, b: __m256i) -> __m256i {
            unsafe { _mm256_add_epi32(a, b) }
        }
        #[inline(always)]
        fn reduce(self, v: __m256i) -> u32 {
            unsafe {
                let s = _mm_add_epi32(_mm256_castsi256_si128(v), _mm256_extracti128_si256::<1>(v));
                let s = _mm_add_epi32(s, _mm_shuffle_epi32::<0b01_00_11_10>(s));
                let s = _mm_add_epi32(s, _mm_shuffle_epi32::<0b10_11_00_01>(s));
                _mm_cvtsi128_si32(s) as u32
            }
        }
    }
}

#[cfg(target_arch = "aarch64")]
mod neon {
    use super::SumOps;
    use crate::backend::Neon;
    use core::arch::aarch64::*;

    // SAFETY (every block below): a `Neon` token exists only if the CPU supports NEON.
    impl SumOps for Neon {
        type V = (uint32x4_t, uint32x4_t);
        #[inline(always)]
        fn zero(self) -> Self::V {
            unsafe { (vdupq_n_u32(0), vdupq_n_u32(0)) }
        }
        #[inline(always)]
        fn load(self, x: &[u32; 8]) -> Self::V {
            unsafe { (vld1q_u32(x.as_ptr()), vld1q_u32(x.as_ptr().add(4))) }
        }
        #[inline(always)]
        fn add(self, a: Self::V, b: Self::V) -> Self::V {
            unsafe { (vaddq_u32(a.0, b.0), vaddq_u32(a.1, b.1)) }
        }
        #[inline(always)]
        fn reduce(self, v: Self::V) -> u32 {
            unsafe { vaddvq_u32(vaddq_u32(v.0, v.1)) }
        }
    }
}
