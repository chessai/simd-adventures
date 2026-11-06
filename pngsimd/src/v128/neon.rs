//! AArch64 NEON implementation of [`Vec128`].
//!
//! Transcribed from `v128_neon.h`. Verified with `cargo test --target aarch64-unknown-linux-gnu`
//! under qemu-aarch64 (see the simd_adventures flake).

use super::Vec128;
use core::arch::aarch64::*;

/// Shift bytes towards higher lanes by `N` (zeros enter at lane 0), i.e. `vextq_u8(zero, v, 16 - N)`.
/// Spelled as a match because `{ 16 - N }` is not a valid const argument without
/// `generic_const_exprs`; `N` is a constant, so the match folds to a single `ext`.
#[inline(always)]
pub(crate) unsafe fn bsl_u8<const N: i32>(v: uint8x16_t) -> uint8x16_t {
    let z = vdupq_n_u8(0);
    match N {
        0 => v,
        1 => vextq_u8::<15>(z, v),
        2 => vextq_u8::<14>(z, v),
        3 => vextq_u8::<13>(z, v),
        4 => vextq_u8::<12>(z, v),
        5 => vextq_u8::<11>(z, v),
        6 => vextq_u8::<10>(z, v),
        7 => vextq_u8::<9>(z, v),
        8 => vextq_u8::<8>(z, v),
        9 => vextq_u8::<7>(z, v),
        10 => vextq_u8::<6>(z, v),
        11 => vextq_u8::<5>(z, v),
        12 => vextq_u8::<4>(z, v),
        13 => vextq_u8::<3>(z, v),
        14 => vextq_u8::<2>(z, v),
        15 => vextq_u8::<1>(z, v),
        _ => z,
    }
}

#[derive(Clone, Copy)]
pub struct Neon;

impl Vec128 for Neon {
    type V = uint8x16_t;

    #[inline(always)] unsafe fn load(p: *const u8) -> uint8x16_t { vld1q_u8(p) }
    #[inline(always)] unsafe fn store(p: *mut u8, v: uint8x16_t) { vst1q_u8(p, v) }
    #[inline(always)] unsafe fn load32(p: *const u8) -> uint8x16_t {
        let x = core::ptr::read_unaligned(p as *const u32);
        vreinterpretq_u8_u32(vsetq_lane_u32::<0>(x, vdupq_n_u32(0)))
    }
    #[inline(always)] unsafe fn store32(p: *mut u8, v: uint8x16_t) {
        core::ptr::write_unaligned(p as *mut u32, vgetq_lane_u32::<0>(vreinterpretq_u32_u8(v)))
    }
    #[inline(always)] unsafe fn load64(p: *const u8) -> uint8x16_t { vcombine_u8(vld1_u8(p), vdup_n_u8(0)) }
    #[inline(always)] unsafe fn store64(p: *mut u8, v: uint8x16_t) { vst1_u8(p, vget_low_u8(v)) }
    #[inline(always)] unsafe fn to_bytes(v: uint8x16_t) -> [u8; 16] { let mut b = [0u8; 16]; vst1q_u8(b.as_mut_ptr(), v); b }
    #[inline(always)] unsafe fn from_bytes(b: &[u8; 16]) -> uint8x16_t { vld1q_u8(b.as_ptr()) }

    #[inline(always)] unsafe fn zero() -> uint8x16_t { vdupq_n_u8(0) }
    #[inline(always)] unsafe fn set1_u8(x: u8) -> uint8x16_t { vdupq_n_u8(x) }
    #[inline(always)] unsafe fn set1_u16(x: u16) -> uint8x16_t { vreinterpretq_u8_u16(vdupq_n_u16(x)) }

    #[inline(always)] unsafe fn add8(a: uint8x16_t, b: uint8x16_t) -> uint8x16_t { vaddq_u8(a, b) }
    #[inline(always)] unsafe fn sub8(a: uint8x16_t, b: uint8x16_t) -> uint8x16_t { vsubq_u8(a, b) }
    #[inline(always)] unsafe fn and(a: uint8x16_t, b: uint8x16_t) -> uint8x16_t { vandq_u8(a, b) }
    #[inline(always)] unsafe fn andnot(a: uint8x16_t, b: uint8x16_t) -> uint8x16_t { vbicq_u8(b, a) }
    #[inline(always)] unsafe fn or(a: uint8x16_t, b: uint8x16_t) -> uint8x16_t { vorrq_u8(a, b) }
    #[inline(always)] unsafe fn xor(a: uint8x16_t, b: uint8x16_t) -> uint8x16_t { veorq_u8(a, b) }
    /// halving add: floor in one instruction
    #[inline(always)] unsafe fn avg_floor_u8(a: uint8x16_t, b: uint8x16_t) -> uint8x16_t { vhaddq_u8(a, b) }

    #[inline(always)] unsafe fn bsl<const N: i32>(v: uint8x16_t) -> uint8x16_t { bsl_u8::<N>(v) }
    #[inline(always)] unsafe fn bsr<const N: i32>(v: uint8x16_t) -> uint8x16_t { vextq_u8::<N>(v, vdupq_n_u8(0)) }
    #[inline(always)] unsafe fn shuffle8(v: uint8x16_t, idx: uint8x16_t) -> uint8x16_t { vqtbl1q_u8(v, idx) }

    #[inline(always)] unsafe fn unpacklo8(v: uint8x16_t) -> uint8x16_t { vreinterpretq_u8_u16(vmovl_u8(vget_low_u8(v))) }
    #[inline(always)] unsafe fn pack16(lo: uint8x16_t, hi: uint8x16_t) -> uint8x16_t {
        vcombine_u8(vqmovun_s16(vreinterpretq_s16_u8(lo)), vqmovun_s16(vreinterpretq_s16_u8(hi)))
    }
    #[inline(always)] unsafe fn add16(a: uint8x16_t, b: uint8x16_t) -> uint8x16_t { vreinterpretq_u8_u16(vaddq_u16(vreinterpretq_u16_u8(a), vreinterpretq_u16_u8(b))) }
    #[inline(always)] unsafe fn sub16(a: uint8x16_t, b: uint8x16_t) -> uint8x16_t { vreinterpretq_u8_u16(vsubq_u16(vreinterpretq_u16_u8(a), vreinterpretq_u16_u8(b))) }
    #[inline(always)] unsafe fn abs16(a: uint8x16_t) -> uint8x16_t { vreinterpretq_u8_s16(vabsq_s16(vreinterpretq_s16_u8(a))) }
    #[inline(always)] unsafe fn min_u16(a: uint8x16_t, b: uint8x16_t) -> uint8x16_t { vreinterpretq_u8_u16(vminq_u16(vreinterpretq_u16_u8(a), vreinterpretq_u16_u8(b))) }
    #[inline(always)] unsafe fn cmpeq16(a: uint8x16_t, b: uint8x16_t) -> uint8x16_t { vreinterpretq_u8_u16(vceqq_u16(vreinterpretq_u16_u8(a), vreinterpretq_u16_u8(b))) }
    #[inline(always)] unsafe fn cmpgt16(a: uint8x16_t, b: uint8x16_t) -> uint8x16_t { vreinterpretq_u8_u16(vcgtq_s16(vreinterpretq_s16_u8(a), vreinterpretq_s16_u8(b))) }
    #[inline(always)] unsafe fn select(mask: uint8x16_t, b: uint8x16_t, a: uint8x16_t) -> uint8x16_t { vbslq_u8(mask, b, a) }
}
