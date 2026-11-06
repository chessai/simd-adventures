//! [`Vec128`] on `std::simd::u8x16` (nightly `portable_simd`), compiled for the build target's
//! features: what LLVM makes of the same kernels written with the portable SIMD API instead of
//! intrinsics. Byte shifts are const swizzles, `shuffle8` is `swizzle_dyn`.

use super::Vec128;
use std::simd::cmp::{SimdOrd, SimdPartialEq, SimdPartialOrd};
use std::simd::num::{SimdInt, SimdUint};
use std::simd::{i16x8, simd_swizzle, u16x8, u8x16, u8x8, Swizzle, ToBytes};

#[derive(Clone, Copy, Debug)]
pub struct StdSimd;

type V = u8x16;

#[inline(always)]
fn h(v: V) -> u16x8 {
    u16x8::from_ne_bytes(v)
}
#[inline(always)]
fn b(v: u16x8) -> V {
    v.to_ne_bytes()
}
#[inline(always)]
fn concat(lo: u8x8, hi: u8x8) -> V {
    simd_swizzle!(lo, hi, [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15])
}

/// `bsl::<N>`: lanes move up by N, zeros (lanes 16.. of the concatenation) enter at lane 0.
struct ShiftUp<const N: i32>;
impl<const N: i32> Swizzle<16> for ShiftUp<N> {
    const INDEX: [usize; 16] = {
        let mut ix = [16usize; 16];
        let mut i = N as usize;
        while i < 16 {
            ix[i] = i - N as usize;
            i += 1;
        }
        ix
    };
}
/// `bsr::<N>`: lanes move down by N, zeros enter at the top.
struct ShiftDown<const N: i32>;
impl<const N: i32> Swizzle<16> for ShiftDown<N> {
    const INDEX: [usize; 16] = {
        let mut ix = [16usize; 16];
        let mut i = 0;
        while i + (N as usize) < 16 {
            ix[i] = i + N as usize;
            i += 1;
        }
        ix
    };
}

impl Vec128 for StdSimd {
    type V = V;

    #[inline(always)] unsafe fn load(p: *const u8) -> V { V::from_array(core::ptr::read_unaligned(p as *const [u8; 16])) }
    #[inline(always)] unsafe fn store(p: *mut u8, v: V) { core::ptr::write_unaligned(p as *mut [u8; 16], v.to_array()) }
    #[inline(always)] unsafe fn load32(p: *const u8) -> V { let mut r = [0u8; 16]; core::ptr::copy_nonoverlapping(p, r.as_mut_ptr(), 4); V::from_array(r) }
    #[inline(always)] unsafe fn store32(p: *mut u8, v: V) { core::ptr::copy_nonoverlapping(v.as_array().as_ptr(), p, 4) }
    #[inline(always)] unsafe fn load64(p: *const u8) -> V { let mut r = [0u8; 16]; core::ptr::copy_nonoverlapping(p, r.as_mut_ptr(), 8); V::from_array(r) }
    #[inline(always)] unsafe fn store64(p: *mut u8, v: V) { core::ptr::copy_nonoverlapping(v.as_array().as_ptr(), p, 8) }
    #[inline(always)] unsafe fn to_bytes(v: V) -> [u8; 16] { v.to_array() }
    #[inline(always)] unsafe fn from_bytes(x: &[u8; 16]) -> V { V::from_array(*x) }

    #[inline(always)] unsafe fn zero() -> V { V::splat(0) }
    #[inline(always)] unsafe fn set1_u8(x: u8) -> V { V::splat(x) }
    #[inline(always)] unsafe fn set1_u16(x: u16) -> V { b(u16x8::splat(x)) }

    #[inline(always)] unsafe fn add8(x: V, y: V) -> V { x + y }
    #[inline(always)] unsafe fn sub8(x: V, y: V) -> V { x - y }
    #[inline(always)] unsafe fn and(x: V, y: V) -> V { x & y }
    #[inline(always)] unsafe fn andnot(x: V, y: V) -> V { !x & y }
    #[inline(always)] unsafe fn or(x: V, y: V) -> V { x | y }
    #[inline(always)] unsafe fn xor(x: V, y: V) -> V { x ^ y }
    // floor((x + y) / 2) without widening
    #[inline(always)] unsafe fn avg_floor_u8(x: V, y: V) -> V { (x & y) + ((x ^ y) >> V::splat(1)) }

    #[inline(always)] unsafe fn bsl<const N: i32>(v: V) -> V { ShiftUp::<N>::concat_swizzle(v, V::splat(0)) }
    #[inline(always)] unsafe fn bsr<const N: i32>(v: V) -> V { ShiftDown::<N>::concat_swizzle(v, V::splat(0)) }
    // bit 7 set -> index >= 16 -> 0; otherwise index & 15
    #[inline(always)] unsafe fn shuffle8(v: V, idx: V) -> V { v.swizzle_dyn(idx & V::splat(0x8F)) }

    #[inline(always)] unsafe fn unpacklo8(v: V) -> V { b(simd_swizzle!(v, [0, 1, 2, 3, 4, 5, 6, 7]).cast::<u16>()) }
    #[inline(always)] unsafe fn pack16(lo: V, hi: V) -> V {
        let sat = |x: V| -> u8x8 { h(x).cast::<i16>().simd_clamp(i16x8::splat(0), i16x8::splat(255)).cast::<u8>() };
        concat(sat(lo), sat(hi))
    }
    #[inline(always)] unsafe fn add16(x: V, y: V) -> V { b(h(x) + h(y)) }
    #[inline(always)] unsafe fn sub16(x: V, y: V) -> V { b(h(x) - h(y)) }
    #[inline(always)] unsafe fn abs16(x: V) -> V { b(h(x).cast::<i16>().abs().cast::<u16>()) }
    #[inline(always)] unsafe fn min_u16(x: V, y: V) -> V { b(h(x).simd_min(h(y))) }
    #[inline(always)] unsafe fn cmpeq16(x: V, y: V) -> V { b(h(x).simd_eq(h(y)).to_int().cast::<u16>()) }
    #[inline(always)] unsafe fn cmpgt16(x: V, y: V) -> V { b(h(x).cast::<i16>().simd_gt(h(y).cast::<i16>()).to_int().cast::<u16>()) }
    #[inline(always)] unsafe fn select(mask: V, y: V, x: V) -> V { (mask & y) | (!mask & x) }
}
