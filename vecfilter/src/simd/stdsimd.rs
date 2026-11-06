//! `std::simd` backend: the same operations on `u32x8` / `u64x8` / `u8x16`, compiled for the build
//! target's features (with the workspace's `-C target-cpu=native`: AVX2 on this machine, AVX-512
//! on an AVX-512 host; NEON on AArch64). It measures what LLVM makes of portable vector code next
//! to the hand-written backends.
//!
//! One semantic difference shows up in the code: `std::simd` shifts take the shift amount modulo
//! the lane width, so `shl_var_32` / `mask_512_from_pos` carry a mask the intrinsics backends
//! (`vpsllvd` zeroes for amounts >= 32) do not need.

use super::Ops;
use crate::backend::StdSimd;
use std::simd::cmp::SimdPartialEq;
use std::simd::num::SimdUint;
use std::simd::{u16x8, u32x8, u64x8, u8x16, ToBytes};

impl Ops for StdSimd {
    const NAME: &'static str = "stdsimd";
    type V32 = u32x8;
    type B512 = u64x8;
    type V128 = u8x16;

    #[inline(always)]
    fn set1_32(self, x: u32) -> u32x8 {
        u32x8::splat(x)
    }
    #[inline(always)]
    unsafe fn loadu_32(self, p: *const u32) -> u32x8 {
        // SAFETY: caller guarantees 32 readable bytes.
        u32x8::from_array(unsafe { p.cast::<[u32; 8]>().read_unaligned() })
    }
    #[inline(always)]
    unsafe fn storeu_32(self, p: *mut u32, a: u32x8) {
        // SAFETY: caller guarantees 32 writable bytes.
        unsafe { p.cast::<[u32; 8]>().write_unaligned(a.to_array()) }
    }
    #[inline(always)]
    fn add_32(self, a: u32x8, b: u32x8) -> u32x8 {
        a + b
    }
    #[inline(always)]
    fn xor_32(self, a: u32x8, b: u32x8) -> u32x8 {
        a ^ b
    }
    #[inline(always)]
    fn or_32(self, a: u32x8, b: u32x8) -> u32x8 {
        a | b
    }
    #[inline(always)]
    fn and_32(self, a: u32x8, b: u32x8) -> u32x8 {
        a & b
    }
    #[inline(always)]
    fn mullo_32(self, a: u32x8, b: u32x8) -> u32x8 {
        a * b
    }
    #[inline(always)]
    fn shr_32<const N: i32>(self, a: u32x8) -> u32x8 {
        a >> u32x8::splat(N as u32)
    }
    #[inline(always)]
    fn shl_var_32(self, a: u32x8, n: u32x8) -> u32x8 {
        a << n
    }
    #[inline(always)]
    fn mulhi_32(self, a: u32x8, b: u32x8) -> u32x8 {
        ((a.cast::<u64>() * b.cast::<u64>()) >> u64x8::splat(32)).cast::<u32>()
    }
    #[inline(always)]
    fn split_keys8(self, k: &[u64; 8]) -> (u32x8, u32x8) {
        let k = u64x8::from_array(*k);
        (k.cast::<u32>(), (k >> u64x8::splat(32)).cast::<u32>())
    }
    #[inline(always)]
    fn test_all_set_32(self, block: u32x8, mask: u32x8) -> bool {
        (!block & mask).reduce_or() == 0
    }

    #[inline(always)]
    unsafe fn load_512(self, p: *const u8) -> u64x8 {
        // SAFETY: caller guarantees 64 readable bytes.
        u64x8::from_array(unsafe { p.cast::<[u64; 8]>().read_unaligned() })
    }
    #[inline(always)]
    unsafe fn store_512(self, p: *mut u8, a: u64x8) {
        // SAFETY: caller guarantees 64 writable bytes.
        unsafe { p.cast::<[u64; 8]>().write_unaligned(a.to_array()) }
    }
    #[inline(always)]
    fn or_512(self, a: u64x8, b: u64x8) -> u64x8 {
        a | b
    }
    #[inline(always)]
    fn mask_512_from_pos(self, pos: u32x8) -> u64x8 {
        u64x8::splat(1) << pos.cast::<u64>()
    }
    #[inline(always)]
    fn test_all_set_512(self, block: u64x8, mask: u64x8) -> bool {
        (!block & mask).reduce_or() == 0
    }

    #[inline(always)]
    unsafe fn load2x64(self, a: *const u8, b: *const u8) -> u8x16 {
        let mut r = [0u8; 16];
        // SAFETY: caller guarantees 8 readable bytes at each of a and b.
        unsafe {
            std::ptr::copy_nonoverlapping(a, r.as_mut_ptr(), 8);
            std::ptr::copy_nonoverlapping(b, r.as_mut_ptr().add(8), 8);
        }
        u8x16::from_array(r)
    }
    #[inline(always)]
    unsafe fn load2x32(self, a: *const u8, b: *const u8) -> u8x16 {
        let mut r = [0u8; 16];
        // SAFETY: caller guarantees 4 readable bytes at each of a and b.
        unsafe {
            std::ptr::copy_nonoverlapping(a, r.as_mut_ptr(), 4);
            std::ptr::copy_nonoverlapping(b, r.as_mut_ptr().add(4), 4);
        }
        u8x16::from_array(r)
    }
    #[inline(always)]
    fn match_mask_u16(self, hay: u8x16, needle: u16) -> u32 {
        u16x8::from_ne_bytes(hay).simd_eq(u16x8::splat(needle)).to_bitmask() as u32
    }
    #[inline(always)]
    fn match_mask_u8(self, hay: u8x16, needle: u8) -> u32 {
        hay.simd_eq(u8x16::splat(needle)).to_bitmask() as u32
    }
}
