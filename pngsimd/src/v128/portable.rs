//! Plain-Rust implementation of [`Vec128`] on `[u8; 16]`.
//!
//! Every op is a loop over 16 bytes (or 8 `u16`s). What LLVM's auto-vectoriser makes of it is
//! what you get — on x86 that turned out to be much worse than the scalar baseline for most
//! kernels (REPORT.md §4.5), which is why the dispatcher ranks `scalar` above `portable`.

use super::Vec128;

#[derive(Clone, Copy)]
pub struct Portable;

type B = [u8; 16];

#[inline(always)]
fn map2(a: B, b: B, f: impl Fn(u8, u8) -> u8) -> B {
    let mut r = [0u8; 16];
    for i in 0..16 { r[i] = f(a[i], b[i]); }
    r
}
#[inline(always)]
fn halves(v: B) -> [u16; 8] {
    let mut h = [0u16; 8];
    for i in 0..8 { h[i] = u16::from_le_bytes([v[2 * i], v[2 * i + 1]]); }
    h
}
#[inline(always)]
fn from_halves(h: [u16; 8]) -> B {
    let mut v = [0u8; 16];
    for i in 0..8 { let b = h[i].to_le_bytes(); v[2 * i] = b[0]; v[2 * i + 1] = b[1]; }
    v
}
#[inline(always)]
fn map2h(a: B, b: B, f: impl Fn(u16, u16) -> u16) -> B {
    let (x, y) = (halves(a), halves(b));
    let mut r = [0u16; 8];
    for i in 0..8 { r[i] = f(x[i], y[i]); }
    from_halves(r)
}

impl Vec128 for Portable {
    type V = B;

    #[inline(always)] unsafe fn load(p: *const u8) -> B { core::ptr::read_unaligned(p as *const B) }
    #[inline(always)] unsafe fn store(p: *mut u8, v: B) { core::ptr::write_unaligned(p as *mut B, v) }
    #[inline(always)] unsafe fn load32(p: *const u8) -> B { let mut r = [0u8; 16]; core::ptr::copy_nonoverlapping(p, r.as_mut_ptr(), 4); r }
    #[inline(always)] unsafe fn store32(p: *mut u8, v: B) { core::ptr::copy_nonoverlapping(v.as_ptr(), p, 4) }
    #[inline(always)] unsafe fn load64(p: *const u8) -> B { let mut r = [0u8; 16]; core::ptr::copy_nonoverlapping(p, r.as_mut_ptr(), 8); r }
    #[inline(always)] unsafe fn store64(p: *mut u8, v: B) { core::ptr::copy_nonoverlapping(v.as_ptr(), p, 8) }
    #[inline(always)] unsafe fn to_bytes(v: B) -> [u8; 16] { v }
    #[inline(always)] unsafe fn from_bytes(b: &[u8; 16]) -> B { *b }

    #[inline(always)] unsafe fn zero() -> B { [0; 16] }
    #[inline(always)] unsafe fn set1_u8(x: u8) -> B { [x; 16] }
    #[inline(always)] unsafe fn set1_u16(x: u16) -> B { from_halves([x; 8]) }

    #[inline(always)] unsafe fn add8(a: B, b: B) -> B { map2(a, b, |x, y| x.wrapping_add(y)) }
    #[inline(always)] unsafe fn sub8(a: B, b: B) -> B { map2(a, b, |x, y| x.wrapping_sub(y)) }
    #[inline(always)] unsafe fn and(a: B, b: B) -> B { map2(a, b, |x, y| x & y) }
    #[inline(always)] unsafe fn andnot(a: B, b: B) -> B { map2(a, b, |x, y| !x & y) }
    #[inline(always)] unsafe fn or(a: B, b: B) -> B { map2(a, b, |x, y| x | y) }
    #[inline(always)] unsafe fn xor(a: B, b: B) -> B { map2(a, b, |x, y| x ^ y) }
    #[inline(always)] unsafe fn avg_floor_u8(a: B, b: B) -> B { map2(a, b, |x, y| ((x as u16 + y as u16) >> 1) as u8) }

    #[inline(always)] unsafe fn bsl<const N: i32>(v: B) -> B {
        let mut r = [0u8; 16];
        let n = N as usize;
        r[n..].copy_from_slice(&v[..16 - n]);
        r
    }
    #[inline(always)] unsafe fn bsr<const N: i32>(v: B) -> B {
        let mut r = [0u8; 16];
        let n = N as usize;
        r[..16 - n].copy_from_slice(&v[n..]);
        r
    }
    #[inline(always)] unsafe fn shuffle8(v: B, idx: B) -> B {
        let mut r = [0u8; 16];
        for i in 0..16 { r[i] = if idx[i] & 0x80 != 0 { 0 } else { v[(idx[i] & 15) as usize] }; }
        r
    }

    #[inline(always)] unsafe fn unpacklo8(v: B) -> B { let mut h = [0u16; 8]; for i in 0..8 { h[i] = v[i] as u16; } from_halves(h) }
    #[inline(always)] unsafe fn pack16(lo: B, hi: B) -> B {
        let (l, h) = (halves(lo), halves(hi));
        let sat = |x: u16| -> u8 { let s = x as i16; if s < 0 { 0 } else if s > 255 { 255 } else { s as u8 } };
        let mut r = [0u8; 16];
        for i in 0..8 { r[i] = sat(l[i]); r[8 + i] = sat(h[i]); }
        r
    }
    #[inline(always)] unsafe fn add16(a: B, b: B) -> B { map2h(a, b, |x, y| x.wrapping_add(y)) }
    #[inline(always)] unsafe fn sub16(a: B, b: B) -> B { map2h(a, b, |x, y| x.wrapping_sub(y)) }
    #[inline(always)] unsafe fn abs16(a: B) -> B { let h = halves(a); let mut r = [0u16; 8]; for i in 0..8 { r[i] = (h[i] as i16).wrapping_abs() as u16; } from_halves(r) }
    #[inline(always)] unsafe fn min_u16(a: B, b: B) -> B { map2h(a, b, |x, y| x.min(y)) }
    #[inline(always)] unsafe fn cmpeq16(a: B, b: B) -> B { map2h(a, b, |x, y| if x == y { 0xFFFF } else { 0 }) }
    #[inline(always)] unsafe fn cmpgt16(a: B, b: B) -> B { map2h(a, b, |x, y| if (x as i16) > (y as i16) { 0xFFFF } else { 0 }) }
    #[inline(always)] unsafe fn select(mask: B, b: B, a: B) -> B { let mut r = [0u8; 16]; for i in 0..16 { r[i] = (mask[i] & b[i]) | (!mask[i] & a[i]); } r }
}
