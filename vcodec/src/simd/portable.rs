//! Portable backend: a 16-byte array and plain loops.
//!
//! This is both the fallback for targets without an intrinsic backend and the
//! executable *specification* of the [`Simd`] trait semantics – the property
//! tests compare every intrinsic backend against it.

use super::Simd;
use crate::backend::Portable;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C, align(16))]
pub struct V128(pub [u8; 16]);

#[inline(always)]
fn map2(a: V128, b: V128, f: impl Fn(u8, u8) -> u8) -> V128 {
    let mut o = [0u8; 16];
    for i in 0..16 {
        o[i] = f(a.0[i], b.0[i]);
    }
    V128(o)
}

#[inline(always)]
fn map_u16(a: V128, b: V128, f: impl Fn(u16, u16) -> u16) -> V128 {
    let mut o = [0u8; 16];
    for i in 0..8 {
        let x = u16::from_le_bytes([a.0[2 * i], a.0[2 * i + 1]]);
        let y = u16::from_le_bytes([b.0[2 * i], b.0[2 * i + 1]]);
        o[2 * i..2 * i + 2].copy_from_slice(&f(x, y).to_le_bytes());
    }
    V128(o)
}

#[inline(always)]
fn map_u32(a: V128, f: impl Fn(u32) -> u32) -> V128 {
    let mut o = [0u8; 16];
    for i in 0..4 {
        let x = u32::from_le_bytes(a.0[4 * i..4 * i + 4].try_into().unwrap());
        o[4 * i..4 * i + 4].copy_from_slice(&f(x).to_le_bytes());
    }
    V128(o)
}

impl Simd for Portable {
    type V = V128;
    const BYTES: usize = 16;
    const NAME: &'static str = "portable";
    const B64_ENC_READ: usize = 16;

    #[inline(always)]
    unsafe fn loadu(p: *const u8) -> V128 {
        V128(core::ptr::read_unaligned(p as *const [u8; 16]))
    }
    #[inline(always)]
    unsafe fn storeu(p: *mut u8, v: V128) {
        core::ptr::write_unaligned(p as *mut [u8; 16], v.0)
    }
    #[inline(always)]
    unsafe fn splat(b: u8) -> V128 {
        V128([b; 16])
    }
    #[inline(always)]
    unsafe fn splat_u16(w: u16) -> V128 {
        let [l, h] = w.to_le_bytes();
        V128([l, h, l, h, l, h, l, h, l, h, l, h, l, h, l, h])
    }
    #[inline(always)]
    unsafe fn splat_u32(d: u32) -> V128 {
        let b = d.to_le_bytes();
        V128([
            b[0], b[1], b[2], b[3], b[0], b[1], b[2], b[3], b[0], b[1], b[2], b[3], b[0], b[1],
            b[2], b[3],
        ])
    }
    #[inline(always)]
    unsafe fn from_lane(lane: [u8; 16]) -> V128 {
        V128(lane)
    }
    #[inline(always)]
    unsafe fn zero() -> V128 {
        V128([0; 16])
    }
    #[inline(always)]
    unsafe fn and(a: V128, b: V128) -> V128 {
        map2(a, b, |x, y| x & y)
    }
    #[inline(always)]
    unsafe fn or(a: V128, b: V128) -> V128 {
        map2(a, b, |x, y| x | y)
    }
    #[inline(always)]
    unsafe fn xor(a: V128, b: V128) -> V128 {
        map2(a, b, |x, y| x ^ y)
    }
    #[inline(always)]
    unsafe fn andnot(a: V128, b: V128) -> V128 {
        map2(a, b, |x, y| x & !y)
    }
    #[inline(always)]
    unsafe fn add_u8(a: V128, b: V128) -> V128 {
        map2(a, b, |x, y| x.wrapping_add(y))
    }
    #[inline(always)]
    unsafe fn sub_u8(a: V128, b: V128) -> V128 {
        map2(a, b, |x, y| x.wrapping_sub(y))
    }
    #[inline(always)]
    unsafe fn subs_u8(a: V128, b: V128) -> V128 {
        map2(a, b, |x, y| x.saturating_sub(y))
    }
    #[inline(always)]
    unsafe fn adds_u8(a: V128, b: V128) -> V128 {
        map2(a, b, |x, y| x.saturating_add(y))
    }
    #[inline(always)]
    unsafe fn cmpeq_u8(a: V128, b: V128) -> V128 {
        map2(a, b, |x, y| if x == y { 0xFF } else { 0 })
    }
    #[inline(always)]
    unsafe fn cmpgt_i8(a: V128, b: V128) -> V128 {
        map2(a, b, |x, y| if (x as i8) > (y as i8) { 0xFF } else { 0 })
    }
    #[inline(always)]
    unsafe fn cmpgt_u8(a: V128, b: V128) -> V128 {
        map2(a, b, |x, y| if x > y { 0xFF } else { 0 })
    }
    #[inline(always)]
    unsafe fn shr_u16<const N: i32>(v: V128) -> V128 {
        map_u16(v, v, |x, _| x >> N)
    }
    #[inline(always)]
    unsafe fn shl_u16<const N: i32>(v: V128) -> V128 {
        map_u16(v, v, |x, _| x << N)
    }
    #[inline(always)]
    unsafe fn shr_u32<const N: i32>(v: V128) -> V128 {
        map_u32(v, |x| x >> N)
    }
    #[inline(always)]
    unsafe fn shl_u32<const N: i32>(v: V128) -> V128 {
        map_u32(v, |x| x << N)
    }
    #[inline(always)]
    unsafe fn mulhi_u16(a: V128, b: V128) -> V128 {
        map_u16(a, b, |x, y| ((x as u32 * y as u32) >> 16) as u16)
    }
    #[inline(always)]
    unsafe fn mullo_u16(a: V128, b: V128) -> V128 {
        map_u16(a, b, |x, y| x.wrapping_mul(y))
    }
    #[inline(always)]
    unsafe fn madd_u8_pairs(a: V128, me: u8, mo: u8) -> V128 {
        let mut o = [0u8; 16];
        for i in 0..8 {
            let s = a.0[2 * i] as u16 * me as u16 + a.0[2 * i + 1] as u16 * mo as u16;
            o[2 * i..2 * i + 2].copy_from_slice(&s.to_le_bytes());
        }
        V128(o)
    }
    #[inline(always)]
    unsafe fn madd_u16_pairs(a: V128, me: u16, mo: u16) -> V128 {
        let mut o = [0u8; 16];
        for i in 0..4 {
            let x = u16::from_le_bytes([a.0[4 * i], a.0[4 * i + 1]]) as u32;
            let y = u16::from_le_bytes([a.0[4 * i + 2], a.0[4 * i + 3]]) as u32;
            let s = x * me as u32 + y * mo as u32;
            o[4 * i..4 * i + 4].copy_from_slice(&s.to_le_bytes());
        }
        V128(o)
    }
    #[inline(always)]
    unsafe fn shuffle_bytes(table: V128, idx: V128) -> V128 {
        let mut o = [0u8; 16];
        for i in 0..16 {
            let j = idx.0[i];
            o[i] = if j & 0x80 != 0 {
                0
            } else {
                table.0[(j & 15) as usize]
            };
        }
        V128(o)
    }
    #[inline(always)]
    unsafe fn alignr<const N: i32>(hi: V128, lo: V128) -> V128 {
        let mut cat = [0u8; 32];
        cat[..16].copy_from_slice(&lo.0);
        cat[16..].copy_from_slice(&hi.0);
        let mut o = [0u8; 16];
        o.copy_from_slice(&cat[N as usize..N as usize + 16]);
        V128(o)
    }
    #[inline(always)]
    unsafe fn interleave(a: V128, b: V128) -> (V128, V128) {
        let mut lo = [0u8; 16];
        let mut hi = [0u8; 16];
        for i in 0..8 {
            lo[2 * i] = a.0[i];
            lo[2 * i + 1] = b.0[i];
            hi[2 * i] = a.0[8 + i];
            hi[2 * i + 1] = b.0[8 + i];
        }
        (V128(lo), V128(hi))
    }
    #[inline(always)]
    unsafe fn pack_u16_u8(a: V128, b: V128) -> V128 {
        let mut o = [0u8; 16];
        for i in 0..8 {
            let x = u16::from_le_bytes([a.0[2 * i], a.0[2 * i + 1]]);
            let y = u16::from_le_bytes([b.0[2 * i], b.0[2 * i + 1]]);
            o[i] = x.min(255) as u8;
            o[8 + i] = y.min(255) as u8;
        }
        V128(o)
    }
    #[inline(always)]
    unsafe fn movemask(v: V128) -> u32 {
        let mut m = 0u32;
        for i in 0..16 {
            m |= ((v.0[i] >> 7) as u32) << i;
        }
        m
    }
    #[inline(always)]
    unsafe fn is_zero(v: V128) -> bool {
        v.0 == [0; 16]
    }
    #[inline(always)]
    unsafe fn load_b64_enc(p: *const u8) -> V128 {
        Self::loadu(p)
    }
    #[inline(always)]
    unsafe fn store_b64_dec(p: *mut u8, v: V128) {
        Self::storeu(p, v)
    }
    #[inline(always)]
    unsafe fn lane(v: V128, _lane: usize) -> V128 {
        v
    }
    #[inline(always)]
    unsafe fn store_lane0(p: *mut u8, v: V128) {
        Self::storeu(p, v)
    }
}
