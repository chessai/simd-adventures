//! NEON (AArch64) backend. A logical 256-bit vector is a pair of q-registers. A `Neon` token
//! exists only if the CPU supports NEON (always, on AArch64).

use super::Ops;
use crate::backend::Neon;
use core::arch::aarch64::*;

#[derive(Clone, Copy)]
pub struct V32x2 {
    pub a: uint32x4_t,
    pub b: uint32x4_t,
}

#[inline(always)]
fn mulhi4(x: uint32x4_t, y: uint32x4_t) -> uint32x4_t {
    unsafe {
        let lo = vmull_u32(vget_low_u32(x), vget_low_u32(y));
        let hi = vmull_high_u32(x, y);
        vuzp2q_u32(vreinterpretq_u32_u64(lo), vreinterpretq_u32_u64(hi)) // odd 32-bit halves = high words
    }
}

const BITS8: [u8; 8] = [1, 2, 4, 8, 16, 32, 64, 128];
const BITS16: [u8; 16] = [1, 2, 4, 8, 16, 32, 64, 128, 1, 2, 4, 8, 16, 32, 64, 128];

impl Ops for Neon {
    const NAME: &'static str = "neon";
    type V32 = V32x2;
    type B512 = [uint64x2_t; 4];
    type V128 = uint8x16_t;

    #[inline(always)]
    fn set1_32(self, x: u32) -> V32x2 {
        unsafe { V32x2 { a: vdupq_n_u32(x), b: vdupq_n_u32(x) } }
    }
    #[inline(always)]
    unsafe fn loadu_32(self, p: *const u32) -> V32x2 {
        unsafe { V32x2 { a: vld1q_u32(p), b: vld1q_u32(p.add(4)) } }
    }
    #[inline(always)]
    unsafe fn storeu_32(self, p: *mut u32, x: V32x2) {
        unsafe {
            vst1q_u32(p, x.a);
            vst1q_u32(p.add(4), x.b);
        }
    }
    #[inline(always)]
    fn add_32(self, x: V32x2, y: V32x2) -> V32x2 {
        unsafe { V32x2 { a: vaddq_u32(x.a, y.a), b: vaddq_u32(x.b, y.b) } }
    }
    #[inline(always)]
    fn xor_32(self, x: V32x2, y: V32x2) -> V32x2 {
        unsafe { V32x2 { a: veorq_u32(x.a, y.a), b: veorq_u32(x.b, y.b) } }
    }
    #[inline(always)]
    fn or_32(self, x: V32x2, y: V32x2) -> V32x2 {
        unsafe { V32x2 { a: vorrq_u32(x.a, y.a), b: vorrq_u32(x.b, y.b) } }
    }
    #[inline(always)]
    fn and_32(self, x: V32x2, y: V32x2) -> V32x2 {
        unsafe { V32x2 { a: vandq_u32(x.a, y.a), b: vandq_u32(x.b, y.b) } }
    }
    #[inline(always)]
    fn mullo_32(self, x: V32x2, y: V32x2) -> V32x2 {
        unsafe { V32x2 { a: vmulq_u32(x.a, y.a), b: vmulq_u32(x.b, y.b) } }
    }
    #[inline(always)]
    fn shr_32<const N: i32>(self, x: V32x2) -> V32x2 {
        unsafe { V32x2 { a: vshrq_n_u32::<N>(x.a), b: vshrq_n_u32::<N>(x.b) } }
    }
    #[inline(always)]
    fn shl_var_32(self, x: V32x2, n: V32x2) -> V32x2 {
        // precondition: n < 32
        unsafe {
            V32x2 {
                a: vshlq_u32(x.a, vreinterpretq_s32_u32(n.a)),
                b: vshlq_u32(x.b, vreinterpretq_s32_u32(n.b)),
            }
        }
    }
    #[inline(always)]
    fn mulhi_32(self, x: V32x2, y: V32x2) -> V32x2 {
        V32x2 { a: mulhi4(x.a, y.a), b: mulhi4(x.b, y.b) }
    }
    #[inline(always)]
    fn split_keys8(self, k: &[u64; 8]) -> (V32x2, V32x2) {
        unsafe {
            let p = k.as_ptr();
            let k0 = vreinterpretq_u32_u64(vld1q_u64(p));
            let k1 = vreinterpretq_u32_u64(vld1q_u64(p.add(2)));
            let k2 = vreinterpretq_u32_u64(vld1q_u64(p.add(4)));
            let k3 = vreinterpretq_u32_u64(vld1q_u64(p.add(6)));
            (
                V32x2 { a: vuzp1q_u32(k0, k1), b: vuzp1q_u32(k2, k3) },
                V32x2 { a: vuzp2q_u32(k0, k1), b: vuzp2q_u32(k2, k3) },
            )
        }
    }
    #[inline(always)]
    fn test_all_set_32(self, block: V32x2, mask: V32x2) -> bool {
        unsafe {
            let ma = vbicq_u32(mask.a, block.a); // mask & ~block
            let mb = vbicq_u32(mask.b, block.b);
            vmaxvq_u32(vorrq_u32(ma, mb)) == 0
        }
    }

    #[inline(always)]
    unsafe fn load_512(self, p: *const u8) -> [uint64x2_t; 4] {
        let q = p.cast::<u64>();
        unsafe { [vld1q_u64(q), vld1q_u64(q.add(2)), vld1q_u64(q.add(4)), vld1q_u64(q.add(6))] }
    }
    #[inline(always)]
    unsafe fn store_512(self, p: *mut u8, x: [uint64x2_t; 4]) {
        let q = p.cast::<u64>();
        unsafe {
            vst1q_u64(q, x[0]);
            vst1q_u64(q.add(2), x[1]);
            vst1q_u64(q.add(4), x[2]);
            vst1q_u64(q.add(6), x[3]);
        }
    }
    #[inline(always)]
    fn or_512(self, x: [uint64x2_t; 4], y: [uint64x2_t; 4]) -> [uint64x2_t; 4] {
        unsafe { [vorrq_u64(x[0], y[0]), vorrq_u64(x[1], y[1]), vorrq_u64(x[2], y[2]), vorrq_u64(x[3], y[3])] }
    }
    #[inline(always)]
    fn mask_512_from_pos(self, pos: V32x2) -> [uint64x2_t; 4] {
        // precondition: pos < 64
        unsafe {
            let one = vdupq_n_u64(1);
            [
                vshlq_u64(one, vreinterpretq_s64_u64(vmovl_u32(vget_low_u32(pos.a)))),
                vshlq_u64(one, vreinterpretq_s64_u64(vmovl_high_u32(pos.a))),
                vshlq_u64(one, vreinterpretq_s64_u64(vmovl_u32(vget_low_u32(pos.b)))),
                vshlq_u64(one, vreinterpretq_s64_u64(vmovl_high_u32(pos.b))),
            ]
        }
    }
    #[inline(always)]
    fn test_all_set_512(self, block: [uint64x2_t; 4], mask: [uint64x2_t; 4]) -> bool {
        unsafe {
            let acc = vorrq_u64(
                vorrq_u64(vbicq_u64(mask[0], block[0]), vbicq_u64(mask[1], block[1])),
                vorrq_u64(vbicq_u64(mask[2], block[2]), vbicq_u64(mask[3], block[3])),
            );
            vmaxvq_u32(vreinterpretq_u32_u64(acc)) == 0
        }
    }

    #[inline(always)]
    unsafe fn load2x64(self, a: *const u8, b: *const u8) -> uint8x16_t {
        unsafe { vreinterpretq_u8_u64(vcombine_u64(vld1_u64(a.cast()), vld1_u64(b.cast()))) }
    }
    #[inline(always)]
    unsafe fn load2x32(self, a: *const u8, b: *const u8) -> uint8x16_t {
        unsafe {
            let x = a.cast::<u32>().read_unaligned();
            let y = b.cast::<u32>().read_unaligned();
            let r = vsetq_lane_u32::<0>(x, vdupq_n_u32(0));
            vreinterpretq_u8_u32(vsetq_lane_u32::<1>(y, r))
        }
    }
    #[inline(always)]
    fn match_mask_u16(self, hay: uint8x16_t, needle: u16) -> u32 {
        unsafe {
            let eq = vceqq_u16(vreinterpretq_u16_u8(hay), vdupq_n_u16(needle));
            // Narrow 8 x u16 (0 or 0xFFFF) to 8 x u8 and pick one bit per lane.
            let n = vmovn_u16(eq);
            vaddv_u8(vand_u8(n, vld1_u8(BITS8.as_ptr()))) as u32
        }
    }
    #[inline(always)]
    fn match_mask_u8(self, hay: uint8x16_t, needle: u8) -> u32 {
        unsafe {
            let eq = vceqq_u8(hay, vdupq_n_u8(needle));
            let m = vandq_u8(eq, vld1q_u8(BITS16.as_ptr()));
            vaddv_u8(vget_low_u8(m)) as u32 | ((vaddv_u8(vget_high_u8(m)) as u32) << 8)
        }
    }
}
