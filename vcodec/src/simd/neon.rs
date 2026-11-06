//! AArch64 NEON backend (128-bit, one lane).
//!
//! Notes on the emulated operations (see REPORT.md, "what didn't vectorise"):
//! * `mulhi_u16` / `mullo_u16` exist on NEON only as widening multiplies, so
//!   the base64 encoder's multiply-add repacking costs 2× the x86 instruction
//!   count here. A native NEON design would use `vshlq_u16` with per-element
//!   shift vectors (or `vsri`/`vsli`), which the trait's x86-shaped interface
//!   does not express.
//! * `madd_u8_pairs` / `madd_u16_pairs` (`pmaddubsw`/`pmaddwd`) are emulated
//!   with mask + shift + two multiplies.
//! * `movemask` has no direct equivalent and costs ~4 instructions.
//! * `shuffle_bytes` needs one extra `and` to reproduce `pshufb`'s "bit 7 ⇒
//!   zero, else index & 15" semantics on top of `tbl`'s "index ≥ 16 ⇒ zero".

use super::Simd;
use crate::backend::Neon;
use core::arch::aarch64::*;

impl Simd for Neon {
    type V = uint8x16_t;
    const BYTES: usize = 16;
    const NAME: &'static str = "neon";
    const B64_ENC_READ: usize = 16;

    #[inline(always)]
    unsafe fn loadu(p: *const u8) -> uint8x16_t {
        vld1q_u8(p)
    }
    #[inline(always)]
    unsafe fn storeu(p: *mut u8, v: uint8x16_t) {
        vst1q_u8(p, v)
    }
    #[inline(always)]
    unsafe fn splat(b: u8) -> uint8x16_t {
        vdupq_n_u8(b)
    }
    #[inline(always)]
    unsafe fn splat_u16(w: u16) -> uint8x16_t {
        vreinterpretq_u8_u16(vdupq_n_u16(w))
    }
    #[inline(always)]
    unsafe fn splat_u32(d: u32) -> uint8x16_t {
        vreinterpretq_u8_u32(vdupq_n_u32(d))
    }
    #[inline(always)]
    unsafe fn from_lane(lane: [u8; 16]) -> uint8x16_t {
        vld1q_u8(lane.as_ptr())
    }
    #[inline(always)]
    unsafe fn zero() -> uint8x16_t {
        vdupq_n_u8(0)
    }
    #[inline(always)]
    unsafe fn and(a: uint8x16_t, b: uint8x16_t) -> uint8x16_t {
        vandq_u8(a, b)
    }
    #[inline(always)]
    unsafe fn or(a: uint8x16_t, b: uint8x16_t) -> uint8x16_t {
        vorrq_u8(a, b)
    }
    #[inline(always)]
    unsafe fn xor(a: uint8x16_t, b: uint8x16_t) -> uint8x16_t {
        veorq_u8(a, b)
    }
    #[inline(always)]
    unsafe fn andnot(a: uint8x16_t, b: uint8x16_t) -> uint8x16_t {
        vbicq_u8(a, b)
    }
    #[inline(always)]
    unsafe fn add_u8(a: uint8x16_t, b: uint8x16_t) -> uint8x16_t {
        vaddq_u8(a, b)
    }
    #[inline(always)]
    unsafe fn sub_u8(a: uint8x16_t, b: uint8x16_t) -> uint8x16_t {
        vsubq_u8(a, b)
    }
    #[inline(always)]
    unsafe fn subs_u8(a: uint8x16_t, b: uint8x16_t) -> uint8x16_t {
        vqsubq_u8(a, b)
    }
    #[inline(always)]
    unsafe fn adds_u8(a: uint8x16_t, b: uint8x16_t) -> uint8x16_t {
        vqaddq_u8(a, b)
    }
    #[inline(always)]
    unsafe fn cmpeq_u8(a: uint8x16_t, b: uint8x16_t) -> uint8x16_t {
        vceqq_u8(a, b)
    }
    #[inline(always)]
    unsafe fn cmpgt_i8(a: uint8x16_t, b: uint8x16_t) -> uint8x16_t {
        vcgtq_s8(vreinterpretq_s8_u8(a), vreinterpretq_s8_u8(b))
    }
    #[inline(always)]
    unsafe fn cmpgt_u8(a: uint8x16_t, b: uint8x16_t) -> uint8x16_t {
        vcgtq_u8(a, b)
    }
    #[inline(always)]
    unsafe fn shr_u16<const N: i32>(v: uint8x16_t) -> uint8x16_t {
        vreinterpretq_u8_u16(vshrq_n_u16::<N>(vreinterpretq_u16_u8(v)))
    }
    #[inline(always)]
    unsafe fn shl_u16<const N: i32>(v: uint8x16_t) -> uint8x16_t {
        vreinterpretq_u8_u16(vshlq_n_u16::<N>(vreinterpretq_u16_u8(v)))
    }
    #[inline(always)]
    unsafe fn shr_u32<const N: i32>(v: uint8x16_t) -> uint8x16_t {
        vreinterpretq_u8_u32(vshrq_n_u32::<N>(vreinterpretq_u32_u8(v)))
    }
    #[inline(always)]
    unsafe fn shl_u32<const N: i32>(v: uint8x16_t) -> uint8x16_t {
        vreinterpretq_u8_u32(vshlq_n_u32::<N>(vreinterpretq_u32_u8(v)))
    }
    #[inline(always)]
    unsafe fn mulhi_u16(a: uint8x16_t, b: uint8x16_t) -> uint8x16_t {
        let a = vreinterpretq_u16_u8(a);
        let b = vreinterpretq_u16_u8(b);
        let lo = vmull_u16(vget_low_u16(a), vget_low_u16(b));
        let hi = vmull_high_u16(a, b);
        let r = vshrn_high_n_u32::<16>(vshrn_n_u32::<16>(lo), hi);
        vreinterpretq_u8_u16(r)
    }
    #[inline(always)]
    unsafe fn mullo_u16(a: uint8x16_t, b: uint8x16_t) -> uint8x16_t {
        vreinterpretq_u8_u16(vmulq_u16(vreinterpretq_u16_u8(a), vreinterpretq_u16_u8(b)))
    }
    #[inline(always)]
    unsafe fn madd_u8_pairs(a: uint8x16_t, me: u8, mo: u8) -> uint8x16_t {
        let w = vreinterpretq_u16_u8(a);
        let even = vandq_u16(w, vdupq_n_u16(0x00FF));
        let odd = vshrq_n_u16::<8>(w);
        let r = vmlaq_u16(
            vmulq_u16(even, vdupq_n_u16(me as u16)),
            odd,
            vdupq_n_u16(mo as u16),
        );
        vreinterpretq_u8_u16(r)
    }
    #[inline(always)]
    unsafe fn madd_u16_pairs(a: uint8x16_t, me: u16, mo: u16) -> uint8x16_t {
        let w = vreinterpretq_u32_u8(a);
        let even = vandq_u32(w, vdupq_n_u32(0xFFFF));
        let odd = vshrq_n_u32::<16>(w);
        let r = vmlaq_u32(
            vmulq_u32(even, vdupq_n_u32(me as u32)),
            odd,
            vdupq_n_u32(mo as u32),
        );
        vreinterpretq_u8_u32(r)
    }
    #[inline(always)]
    unsafe fn shuffle_bytes(table: uint8x16_t, idx: uint8x16_t) -> uint8x16_t {
        // pshufb: bit 7 set -> 0, else index & 15.  tbl: index >= 16 -> 0.
        vqtbl1q_u8(table, vandq_u8(idx, vdupq_n_u8(0x8F)))
    }
    #[inline(always)]
    unsafe fn alignr<const N: i32>(hi: uint8x16_t, lo: uint8x16_t) -> uint8x16_t {
        vextq_u8::<N>(lo, hi)
    }
    #[inline(always)]
    unsafe fn interleave(a: uint8x16_t, b: uint8x16_t) -> (uint8x16_t, uint8x16_t) {
        (vzip1q_u8(a, b), vzip2q_u8(a, b))
    }
    #[inline(always)]
    unsafe fn pack_u16_u8(a: uint8x16_t, b: uint8x16_t) -> uint8x16_t {
        vcombine_u8(
            vqmovn_u16(vreinterpretq_u16_u8(a)),
            vqmovn_u16(vreinterpretq_u16_u8(b)),
        )
    }
    #[inline(always)]
    unsafe fn movemask(v: uint8x16_t) -> u32 {
        let bits = vld1q_u8([1u8, 2, 4, 8, 16, 32, 64, 128, 1, 2, 4, 8, 16, 32, 64, 128].as_ptr());
        let t = vandq_u8(v, bits);
        let lo = vaddv_u8(vget_low_u8(t)) as u32;
        let hi = vaddv_u8(vget_high_u8(t)) as u32;
        lo | (hi << 8)
    }
    #[inline(always)]
    unsafe fn is_zero(v: uint8x16_t) -> bool {
        vmaxvq_u8(v) == 0
    }
    #[inline(always)]
    unsafe fn load_b64_enc(p: *const u8) -> uint8x16_t {
        vld1q_u8(p)
    }
    #[inline(always)]
    unsafe fn store_b64_dec(p: *mut u8, v: uint8x16_t) {
        vst1q_u8(p, v)
    }
    #[inline(always)]
    unsafe fn lane_lo_u64(v: uint8x16_t, _lane: usize) -> u64 {
        vgetq_lane_u64::<0>(vreinterpretq_u64_u8(v))
    }
    #[inline(always)]
    unsafe fn lane_hi_u64(v: uint8x16_t, _lane: usize) -> u64 {
        vgetq_lane_u64::<1>(vreinterpretq_u64_u8(v))
    }
    #[inline(always)]
    unsafe fn lane(v: uint8x16_t, _lane: usize) -> uint8x16_t {
        v
    }
    #[inline(always)]
    unsafe fn store_lane0(p: *mut u8, v: uint8x16_t) {
        vst1q_u8(p, v)
    }
}
