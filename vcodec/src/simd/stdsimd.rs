//! `std::simd` backend (nightly `portable_simd`): the trait on `u8x16`, compiled for the build
//! target's features (with the workspace's `-C target-cpu=native`: AVX2 on this machine, NEON on
//! AArch64). It shows what LLVM makes of the same kernels written with the portable SIMD API.
//!
//! 16 bytes, one lane, because the trait's model is lane-local 16-byte shuffles: `swizzle_dyn` on
//! `u8x16` is exactly `pshufb` / `tbl`, while on `u8x32` it is a full-width permute that AVX2 can
//! only emulate. The ops without a `std::simd` counterpart are spelled out with widening
//! (`mulhi_u16`), masks and shifts (`madd_*`), and `to_bitmask` (`movemask`).

use super::Simd;
use crate::backend::StdSimd;
use std::simd::cmp::{SimdOrd, SimdPartialEq, SimdPartialOrd};
use std::simd::num::{SimdInt, SimdUint};
use std::simd::{simd_swizzle, u16x8, u32x4, u64x2, u8x16, u8x8, Swizzle, ToBytes};

type V = u8x16;

#[inline(always)]
fn w16(v: V) -> u16x8 {
    u16x8::from_le_bytes(v)
}
#[inline(always)]
fn w32(v: V) -> u32x4 {
    u32x4::from_le_bytes(v)
}

/// `alignr::<N>(hi, lo)`: bytes `N .. N + 16` of `lo:hi`.
struct AlignR<const N: i32>;
impl<const N: i32> Swizzle<16> for AlignR<N> {
    const INDEX: [usize; 16] = {
        let mut ix = [0usize; 16];
        let mut i = 0;
        while i < 16 {
            ix[i] = i + N as usize;
            i += 1;
        }
        ix
    };
}

impl Simd for StdSimd {
    type V = V;
    const BYTES: usize = 16;
    const NAME: &'static str = "stdsimd";
    const B64_ENC_READ: usize = 16;

    #[inline(always)]
    unsafe fn loadu(p: *const u8) -> V {
        V::from_array(core::ptr::read_unaligned(p as *const [u8; 16]))
    }
    #[inline(always)]
    unsafe fn storeu(p: *mut u8, v: V) {
        core::ptr::write_unaligned(p as *mut [u8; 16], v.to_array())
    }
    #[inline(always)]
    unsafe fn splat(b: u8) -> V {
        V::splat(b)
    }
    #[inline(always)]
    unsafe fn splat_u16(w: u16) -> V {
        u16x8::splat(w).to_le_bytes()
    }
    #[inline(always)]
    unsafe fn splat_u32(d: u32) -> V {
        u32x4::splat(d).to_le_bytes()
    }
    #[inline(always)]
    unsafe fn from_lane(lane: [u8; 16]) -> V {
        V::from_array(lane)
    }
    #[inline(always)]
    unsafe fn zero() -> V {
        V::splat(0)
    }
    #[inline(always)]
    unsafe fn and(a: V, b: V) -> V {
        a & b
    }
    #[inline(always)]
    unsafe fn or(a: V, b: V) -> V {
        a | b
    }
    #[inline(always)]
    unsafe fn xor(a: V, b: V) -> V {
        a ^ b
    }
    #[inline(always)]
    unsafe fn andnot(a: V, b: V) -> V {
        a & !b
    }
    #[inline(always)]
    unsafe fn add_u8(a: V, b: V) -> V {
        a + b
    }
    #[inline(always)]
    unsafe fn sub_u8(a: V, b: V) -> V {
        a - b
    }
    #[inline(always)]
    unsafe fn subs_u8(a: V, b: V) -> V {
        a.saturating_sub(b)
    }
    #[inline(always)]
    unsafe fn adds_u8(a: V, b: V) -> V {
        a.saturating_add(b)
    }
    #[inline(always)]
    unsafe fn cmpeq_u8(a: V, b: V) -> V {
        a.simd_eq(b).to_int().cast::<u8>()
    }
    #[inline(always)]
    unsafe fn cmpgt_i8(a: V, b: V) -> V {
        a.cast::<i8>().simd_gt(b.cast::<i8>()).to_int().cast::<u8>()
    }
    #[inline(always)]
    unsafe fn cmpgt_u8(a: V, b: V) -> V {
        a.simd_gt(b).to_int().cast::<u8>()
    }
    #[inline(always)]
    unsafe fn shr_u16<const N: i32>(v: V) -> V {
        (w16(v) >> u16x8::splat(N as u16)).to_le_bytes()
    }
    #[inline(always)]
    unsafe fn shl_u16<const N: i32>(v: V) -> V {
        (w16(v) << u16x8::splat(N as u16)).to_le_bytes()
    }
    #[inline(always)]
    unsafe fn shr_u32<const N: i32>(v: V) -> V {
        (w32(v) >> u32x4::splat(N as u32)).to_le_bytes()
    }
    #[inline(always)]
    unsafe fn shl_u32<const N: i32>(v: V) -> V {
        (w32(v) << u32x4::splat(N as u32)).to_le_bytes()
    }
    #[inline(always)]
    unsafe fn mulhi_u16(a: V, b: V) -> V {
        let p = w16(a).cast::<u32>() * w16(b).cast::<u32>();
        (p >> 16).cast::<u16>().to_le_bytes()
    }
    #[inline(always)]
    unsafe fn mullo_u16(a: V, b: V) -> V {
        (w16(a) * w16(b)).to_le_bytes()
    }
    #[inline(always)]
    unsafe fn madd_u8_pairs(a: V, me: u8, mo: u8) -> V {
        let w = w16(a);
        let even = w & u16x8::splat(0x00FF);
        let odd = w >> u16x8::splat(8);
        (even * u16x8::splat(me as u16) + odd * u16x8::splat(mo as u16)).to_le_bytes()
    }
    #[inline(always)]
    unsafe fn madd_u16_pairs(a: V, me: u16, mo: u16) -> V {
        let w = w32(a);
        let even = w & u32x4::splat(0xFFFF);
        let odd = w >> u32x4::splat(16);
        (even * u32x4::splat(me as u32) + odd * u32x4::splat(mo as u32)).to_le_bytes()
    }
    #[inline(always)]
    unsafe fn shuffle_bytes(table: V, idx: V) -> V {
        // pshufb: bit 7 set -> 0, else index & 15; swizzle_dyn: index >= 16 -> 0.
        table.swizzle_dyn(idx & V::splat(0x8F))
    }
    #[inline(always)]
    unsafe fn alignr<const N: i32>(hi: V, lo: V) -> V {
        AlignR::<N>::concat_swizzle(lo, hi)
    }
    #[inline(always)]
    unsafe fn interleave(a: V, b: V) -> (V, V) {
        a.interleave(b)
    }
    #[inline(always)]
    unsafe fn pack_u16_u8(a: V, b: V) -> V {
        let lo: u8x8 = w16(a).simd_min(u16x8::splat(255)).cast::<u8>();
        let hi: u8x8 = w16(b).simd_min(u16x8::splat(255)).cast::<u8>();
        simd_swizzle!(lo, hi, [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15])
    }
    #[inline(always)]
    unsafe fn movemask(v: V) -> u32 {
        v.cast::<i8>().is_negative().to_bitmask() as u32
    }
    #[inline(always)]
    unsafe fn is_zero(v: V) -> bool {
        v.reduce_or() == 0
    }
    #[inline(always)]
    unsafe fn load_b64_enc(p: *const u8) -> V {
        Self::loadu(p)
    }
    #[inline(always)]
    unsafe fn store_b64_dec(p: *mut u8, v: V) {
        Self::storeu(p, v)
    }
    #[inline(always)]
    unsafe fn lane_lo_u64(v: V, _lane: usize) -> u64 {
        u64x2::from_le_bytes(v)[0]
    }
    #[inline(always)]
    unsafe fn lane_hi_u64(v: V, _lane: usize) -> u64 {
        u64x2::from_le_bytes(v)[1]
    }
    #[inline(always)]
    unsafe fn lane(v: V, _lane: usize) -> V {
        v
    }
    #[inline(always)]
    unsafe fn store_lane0(p: *mut u8, v: V) {
        Self::storeu(p, v)
    }
}
