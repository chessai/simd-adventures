//! x86 SSSE3/SSE4.1 implementation of [`Vec128`].
//!
//! Three marker types share everything except `select`, because the encoding of the select
//! changes the length of the Paeth dependency chain (see REPORT.md §4.3):
//! * [`Sse`]: legacy `pblendvb` — 1 uop, 1-cycle latency on Intel.
//! * [`SseVex`]: `and/andnot/or` — VEX `vpblendvb` is 2 uops / 2 cycles on Skylake..Sapphire
//!   Rapids, and the AVX2 backend compiles this file with VEX encoding.
//! * [`SseEvex`]: `vpternlogd` (AVX-512VL) — 1 uop again.

use super::Vec128;
#[cfg(target_arch = "x86")]
use core::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;

#[derive(Clone, Copy)]
pub struct Sse;
#[derive(Clone, Copy)]
pub struct SseVex;
#[derive(Clone, Copy)]
pub struct SseEvex;

macro_rules! impl_sse {
    ($t:ty, $select:expr) => {
        impl Vec128 for $t {
            type V = __m128i;

            #[inline(always)] unsafe fn load(p: *const u8) -> __m128i { _mm_loadu_si128(p as *const __m128i) }
            #[inline(always)] unsafe fn store(p: *mut u8, v: __m128i) { _mm_storeu_si128(p as *mut __m128i, v) }
            #[inline(always)] unsafe fn load32(p: *const u8) -> __m128i { _mm_cvtsi32_si128(core::ptr::read_unaligned(p as *const i32)) }
            #[inline(always)] unsafe fn store32(p: *mut u8, v: __m128i) { core::ptr::write_unaligned(p as *mut i32, _mm_cvtsi128_si32(v)) }
            #[inline(always)] unsafe fn load64(p: *const u8) -> __m128i { _mm_loadl_epi64(p as *const __m128i) }
            #[inline(always)] unsafe fn store64(p: *mut u8, v: __m128i) { _mm_storel_epi64(p as *mut __m128i, v) }
            #[inline(always)] unsafe fn to_bytes(v: __m128i) -> [u8; 16] { let mut b = [0u8; 16]; _mm_storeu_si128(b.as_mut_ptr() as *mut __m128i, v); b }
            #[inline(always)] unsafe fn from_bytes(b: &[u8; 16]) -> __m128i { _mm_loadu_si128(b.as_ptr() as *const __m128i) }

            #[inline(always)] unsafe fn zero() -> __m128i { _mm_setzero_si128() }
            #[inline(always)] unsafe fn set1_u8(x: u8) -> __m128i { _mm_set1_epi8(x as i8) }
            #[inline(always)] unsafe fn set1_u16(x: u16) -> __m128i { _mm_set1_epi16(x as i16) }

            #[inline(always)] unsafe fn add8(a: __m128i, b: __m128i) -> __m128i { _mm_add_epi8(a, b) }
            #[inline(always)] unsafe fn sub8(a: __m128i, b: __m128i) -> __m128i { _mm_sub_epi8(a, b) }
            #[inline(always)] unsafe fn and(a: __m128i, b: __m128i) -> __m128i { _mm_and_si128(a, b) }
            #[inline(always)] unsafe fn andnot(a: __m128i, b: __m128i) -> __m128i { _mm_andnot_si128(a, b) }
            #[inline(always)] unsafe fn or(a: __m128i, b: __m128i) -> __m128i { _mm_or_si128(a, b) }
            #[inline(always)] unsafe fn xor(a: __m128i, b: __m128i) -> __m128i { _mm_xor_si128(a, b) }
            /// pavgb rounds up; subtract the carry bit (a^b)&1 — 2 ops on the chain instead of 4.
            #[inline(always)] unsafe fn avg_floor_u8(a: __m128i, b: __m128i) -> __m128i {
                _mm_sub_epi8(_mm_avg_epu8(a, b), _mm_and_si128(_mm_xor_si128(a, b), _mm_set1_epi8(1)))
            }

            #[inline(always)] unsafe fn bsl<const N: i32>(v: __m128i) -> __m128i { _mm_slli_si128::<N>(v) }
            #[inline(always)] unsafe fn bsr<const N: i32>(v: __m128i) -> __m128i { _mm_srli_si128::<N>(v) }
            #[inline(always)] unsafe fn shuffle8(v: __m128i, idx: __m128i) -> __m128i { _mm_shuffle_epi8(v, idx) }

            #[inline(always)] unsafe fn unpacklo8(v: __m128i) -> __m128i { _mm_cvtepu8_epi16(v) }
            #[inline(always)] unsafe fn pack16(lo: __m128i, hi: __m128i) -> __m128i { _mm_packus_epi16(lo, hi) }
            #[inline(always)] unsafe fn add16(a: __m128i, b: __m128i) -> __m128i { _mm_add_epi16(a, b) }
            #[inline(always)] unsafe fn sub16(a: __m128i, b: __m128i) -> __m128i { _mm_sub_epi16(a, b) }
            #[inline(always)] unsafe fn abs16(a: __m128i) -> __m128i { _mm_abs_epi16(a) }
            #[inline(always)] unsafe fn min_u16(a: __m128i, b: __m128i) -> __m128i { _mm_min_epu16(a, b) }
            #[inline(always)] unsafe fn cmpeq16(a: __m128i, b: __m128i) -> __m128i { _mm_cmpeq_epi16(a, b) }
            #[inline(always)] unsafe fn cmpgt16(a: __m128i, b: __m128i) -> __m128i { _mm_cmpgt_epi16(a, b) }
            #[inline(always)] unsafe fn select(mask: __m128i, b: __m128i, a: __m128i) -> __m128i { ($select)(mask, b, a) }
        }
    };
}

impl_sse!(Sse, |mask, b, a| _mm_blendv_epi8(a, b, mask));
impl_sse!(SseVex, |mask, b, a| _mm_or_si128(_mm_and_si128(mask, b), _mm_andnot_si128(mask, a)));
impl_sse!(SseEvex, |mask, b, a| _mm_ternarylogic_epi32::<0xCA>(mask, b, a));
