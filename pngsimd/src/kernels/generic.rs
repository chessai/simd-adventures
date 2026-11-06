//! Generic SIMD kernels written against [`Vec128`]; monomorphised per backend.
//!
//! Contracts (checked by the safe wrappers in `mod.rs`, relied upon here):
//! * `unfilter_row`: `row.len() >= len + ROW_SLACK`, `prev.len() >= len` — the Sub filter for
//!   3- and 6-byte pixels loads 16 bytes at offsets up to `len - 12`; nothing else reads past
//!   `len`, and nothing ever writes past it.
//! * `rgb_to_rgba`: `src.len() == 3 * npix`, `dst.len() >= 4 * npix`; the vector loop stops
//!   while a full 16-byte source load is still in bounds.
//! * `u16be_to_u8`: `src.len() >= 2 * dst.len()`.
//!
//! All functions are `#[inline(always)]` so that they are compiled inside the backend's
//! `#[target_feature]` entry point.

use crate::v128::Vec128;
use super::scalar;

pub const ROW_SLACK: usize = 32;

// broadcast-last-pixel shuffle masks (12-byte blocks for bpp 3 and 6)
const BC1: [u8; 16] = [15; 16];
const BC2: [u8; 16] = [14, 15, 14, 15, 14, 15, 14, 15, 14, 15, 14, 15, 14, 15, 14, 15];
const BC3: [u8; 16] = [9, 10, 11, 9, 10, 11, 9, 10, 11, 9, 10, 11, 9, 10, 11, 9];
const BC4: [u8; 16] = [12, 13, 14, 15, 12, 13, 14, 15, 12, 13, 14, 15, 12, 13, 14, 15];
const BC6: [u8; 16] = [6, 7, 8, 9, 10, 11, 6, 7, 8, 9, 10, 11, 6, 7, 8, 9];
const BC8: [u8; 16] = [8, 9, 10, 11, 12, 13, 14, 15, 8, 9, 10, 11, 12, 13, 14, 15];

/// Sub: `x[i] = f[i] + x[i-bpp]` — a prefix sum with stride bpp.
///
/// The serial dependency is broken with a log-step scan inside the register: shifting the
/// vector by bpp, 2·bpp, 4·bpp… bytes and adding gives every lane the sum of all pixels to its
/// left in the block. The only loop-carried dependency left is one add of the previous block's
/// last pixel (broadcast with a byte shuffle): ~2 ops per 16 bytes instead of 1 op per byte.
#[inline(always)]
pub unsafe fn filter_sub<X: Vec128>(bpp: usize, row: *mut u8, len: usize) {
    let mut i = 0usize;
    let mut carry = X::zero();
    match bpp {
        1 => {
            let bc = X::from_bytes(&BC1);
            while i + 16 <= len {
                let mut x = X::load(row.add(i));
                x = X::add8(x, X::bsl::<1>(x));
                x = X::add8(x, X::bsl::<2>(x));
                x = X::add8(x, X::bsl::<4>(x));
                x = X::add8(x, X::bsl::<8>(x));
                x = X::add8(x, carry);
                X::store(row.add(i), x);
                carry = X::shuffle8(x, bc);
                i += 16;
            }
        }
        2 => {
            let bc = X::from_bytes(&BC2);
            while i + 16 <= len {
                let mut x = X::load(row.add(i));
                x = X::add8(x, X::bsl::<2>(x));
                x = X::add8(x, X::bsl::<4>(x));
                x = X::add8(x, X::bsl::<8>(x));
                x = X::add8(x, carry);
                X::store(row.add(i), x);
                carry = X::shuffle8(x, bc);
                i += 16;
            }
        }
        3 => {
            // 16 is not a multiple of 3: 12-byte (4-pixel) blocks, 16-byte loads (ROW_SLACK), 8+4-byte stores
            let bc = X::from_bytes(&BC3);
            while i + 12 <= len {
                let mut x = X::load(row.add(i));
                x = X::add8(x, X::bsl::<3>(x));
                x = X::add8(x, X::bsl::<6>(x));
                x = X::add8(x, carry);
                X::store64(row.add(i), x);
                X::store32(row.add(i + 8), X::bsr::<8>(x));
                carry = X::shuffle8(x, bc);
                i += 12;
            }
        }
        4 => {
            let bc = X::from_bytes(&BC4);
            while i + 16 <= len {
                let mut x = X::load(row.add(i));
                x = X::add8(x, X::bsl::<4>(x));
                x = X::add8(x, X::bsl::<8>(x));
                x = X::add8(x, carry);
                X::store(row.add(i), x);
                carry = X::shuffle8(x, bc);
                i += 16;
            }
        }
        6 => {
            let bc = X::from_bytes(&BC6);
            while i + 12 <= len {
                let mut x = X::load(row.add(i));
                x = X::add8(x, X::bsl::<6>(x));
                x = X::add8(x, carry);
                X::store64(row.add(i), x);
                X::store32(row.add(i + 8), X::bsr::<8>(x));
                carry = X::shuffle8(x, bc);
                i += 12;
            }
        }
        8 => {
            let bc = X::from_bytes(&BC8);
            while i + 16 <= len {
                let mut x = X::load(row.add(i));
                x = X::add8(x, X::bsl::<8>(x));
                x = X::add8(x, carry);
                X::store(row.add(i), x);
                carry = X::shuffle8(x, bc);
                i += 16;
            }
        }
        _ => {}
    }
    if i < bpp { i = bpp; }
    while i < len {
        *row.add(i) = (*row.add(i)).wrapping_add(*row.add(i - bpp));
        i += 1;
    }
}

/// Up: `x[i] = f[i] + prev[i]` — embarrassingly parallel.
#[inline(always)]
pub unsafe fn filter_up<X: Vec128>(row: *mut u8, prev: *const u8, len: usize) {
    let mut i = 0usize;
    while i + 32 <= len {
        X::store(row.add(i), X::add8(X::load(row.add(i)), X::load(prev.add(i))));
        X::store(row.add(i + 16), X::add8(X::load(row.add(i + 16)), X::load(prev.add(i + 16))));
        i += 32;
    }
    while i + 16 <= len {
        X::store(row.add(i), X::add8(X::load(row.add(i)), X::load(prev.add(i))));
        i += 16;
    }
    while i < len {
        *row.add(i) = (*row.add(i)).wrapping_add(*prev.add(i));
        i += 1;
    }
}

/// Average: `x[i] = f[i] + floor((x[i-bpp] + prev[i]) / 2)`.
///
/// The floor makes the recurrence non-linear, so the Sub trick does not apply: a pixel can only
/// be computed once its left neighbour is final. What we can do is process all bytes of one
/// pixel in parallel (3/4 lanes, or 6/8 for 16-bit) and keep the running pixel in a register.
/// The next pixel's filtered bytes are loaded before the current result is stored, so the
/// 4-byte store's spill-over byte (for bpp = 3) is harmless. The vector loop stops one pixel
/// early so that no load touches `prev[len..]`; the scalar tail finishes from the stored row.
#[inline(always)]
pub unsafe fn filter_avg<X: Vec128>(bpp: usize, row: &mut [u8], prev: Option<&[u8]>, len: usize) {
    match bpp {
        3 | 4 => filter_avg_w::<X, 4>(bpp, row, prev, len),
        6 | 8 => filter_avg_w::<X, 8>(bpp, row, prev, len),
        // bpp 1 and 2: a 1- or 2-lane vector loses to the register-carried scalar loop (REPORT.md)
        _ => scalar::unfilter_row(3, bpp, row, prev, len),
    }
}

/// `W` = 4 or 8: the load/store width (one branch-free instantiation per width)
#[inline(always)]
unsafe fn filter_avg_w<X: Vec128, const W: usize>(bpp: usize, row: &mut [u8], prev: Option<&[u8]>, len: usize) {
    let w = W;
    if len < w { return scalar::unfilter_row(3, bpp, row, prev, len); }
    let r = row.as_mut_ptr();
    let ld = |p: *const u8| if W == 8 { X::load64(p) } else { X::load32(p) };
    let st = |p: *mut u8, v| if W == 8 { X::store64(p, v) } else { X::store32(p, v) };
    let mut i;
    let mut a;
    match prev {
        None => {
            a = ld(r);
            i = bpp;
            while i + w <= len {
                let x = ld(r.add(i));
                st(r.add(i - bpp), a);
                a = X::add8(x, X::avg_floor_u8(a, X::zero()));
                i += bpp;
            }
            let t = X::to_bytes(a);
            core::ptr::copy_nonoverlapping(t.as_ptr(), r.add(i - bpp), bpp);
            while i < len { *r.add(i) = (*r.add(i)).wrapping_add(*r.add(i - bpp) >> 1); i += 1; }
        }
        Some(prev) => {
            let p = prev.as_ptr();
            let x = ld(r);
            let b = ld(p);
            a = X::add8(x, X::avg_floor_u8(X::zero(), b));
            i = bpp;
            while i + w <= len {
                let x = ld(r.add(i));
                let b = ld(p.add(i));
                st(r.add(i - bpp), a);
                a = X::add8(x, X::avg_floor_u8(a, b));
                i += bpp;
            }
            let t = X::to_bytes(a);
            core::ptr::copy_nonoverlapping(t.as_ptr(), r.add(i - bpp), bpp);
            while i < len {
                *r.add(i) = (*r.add(i)).wrapping_add((((*r.add(i - bpp)) as u16 + (*p.add(i)) as u16) >> 1) as u8);
                i += 1;
            }
        }
    }
}

/// Paeth: `pred` = closest of (a, b, c) to a + b − c.
///
/// Non-linear in `a`, so the dependency chain runs one pixel at a time; the parallelism is
/// across the bytes of a pixel, in 16-bit lanes so differences do not wrap. `pa = |b−c|` does
/// not depend on `a` and is computed off the critical path; `c` for the next pixel is this
/// pixel's `b`. The loop-carried chain is sub → abs → (add, abs) → min → cmpgt → select → add
/// → and: 8 single-cycle ops.
#[inline(always)]
pub unsafe fn filter_paeth<X: Vec128>(bpp: usize, row: &mut [u8], prev: Option<&[u8]>, len: usize) {
    let prev = match prev {
        None => return filter_sub::<X>(bpp, row.as_mut_ptr(), len), // b = c = 0: Paeth degenerates to Sub
        Some(p) => p,
    };
    match bpp {
        3 | 4 => filter_paeth_w::<X, 4>(bpp, row, prev, len),
        6 | 8 => filter_paeth_w::<X, 8>(bpp, row, prev, len),
        _ => scalar::unfilter_row(4, bpp, row, Some(prev), len),
    }
}

#[inline(always)]
unsafe fn filter_paeth_w<X: Vec128, const W: usize>(bpp: usize, row: &mut [u8], prev: &[u8], len: usize) {
    let w = W;
    if len < w { return scalar::unfilter_row(4, bpp, row, Some(prev), len); }
    let r = row.as_mut_ptr();
    let p = prev.as_ptr();
    let ld = |q: *const u8| if W == 8 { X::load64(q) } else { X::load32(q) };
    let st = |q: *mut u8, v| if W == 8 { X::store64(q, v) } else { X::store32(q, v) };
    // The mask constant is hidden from the optimiser on purpose: with a known 0x00FF LLVM
    // rewrites `(zext(x) + pred) & 0xFF` into `zext(trunc(x + pred))`, which on AVX-512 becomes
    // vpmovwb + vpaddb + vpmovzxbw (3 ops, ~6 cycles) in place of vpaddw + vpand (2 ops, 2
    // cycles) on the loop-carried chain. See REPORT-rust.md.
    let lo8 = X::set1_u16(core::hint::black_box(0x00FFu16));
    let mut b = X::unpacklo8(ld(p));
    // first pixel: a = c = 0 -> the predictor is b
    let mut a = X::and(X::add16(X::unpacklo8(ld(r)), b), lo8);
    let mut i = bpp;
    while i + w <= len {
        let c = b;
        let x = ld(r.add(i));
        b = X::unpacklo8(ld(p.add(i)));
        let pa = X::sub16(b, c);          // b - c   (independent of a)
        let pb = X::sub16(a, c);          // a - c
        let pc = X::add16(pa, pb);        // a + b - 2c
        let pa = X::abs16(pa);
        let pb = X::abs16(pb);
        let pc = X::abs16(pc);
        // spec: a if pa <= pb && pa <= pc; else b if pb <= pc; else c — written with two
        // compares instead of min+min+cmpeq+cmpeq: the chain is 8 ops long instead of 10
        let pred = X::select(X::cmpgt16(pb, pc), c, b);                 // pb > pc ? c : b
        let pred = X::select(X::cmpgt16(pa, X::min_u16(pb, pc)), pred, a); // pa > min(pb,pc) ? that : a
        st(r.add(i - bpp), X::pack16(a, a));              // store the previous pixel meanwhile
        a = X::and(X::add16(X::unpacklo8(x), pred), lo8);
        i += bpp;
    }
    let t = X::to_bytes(X::pack16(a, a));
    core::ptr::copy_nonoverlapping(t.as_ptr(), r.add(i - bpp), bpp);
    while i < len {
        let (aa, bb, cc) = ((*r.add(i - bpp)) as i32, (*p.add(i)) as i32, (*p.add(i - bpp)) as i32);
        let pp = aa + bb - cc;
        let (pa, pb, pc) = ((pp - aa).abs(), (pp - bb).abs(), (pp - cc).abs());
        let pred = if pa <= pb && pa <= pc { aa } else if pb <= pc { bb } else { cc };
        *r.add(i) = (*r.add(i)).wrapping_add(pred as u8);
        i += 1;
    }
}

#[inline(always)]
pub unsafe fn unfilter_row<X: Vec128>(filter: u8, bpp: usize, row: &mut [u8], prev: Option<&[u8]>, len: usize) {
    match filter {
        1 => filter_sub::<X>(bpp, row.as_mut_ptr(), len),
        2 => if let Some(p) = prev { filter_up::<X>(row.as_mut_ptr(), p.as_ptr(), len) },
        3 => filter_avg::<X>(bpp, row, prev, len),
        4 => filter_paeth::<X>(bpp, row, prev, len),
        _ => {}
    }
}

pub const RGBA_SHUF: [u8; 16] = [0, 1, 2, 0x80, 3, 4, 5, 0x80, 6, 7, 8, 0x80, 9, 10, 11, 0x80];

/// RGB -> RGBA: a byte shuffle spreads 4 pixels (12 bytes) over 16 bytes, then alpha is OR'd in.
/// Reads 16 source bytes per 4 pixels, so the loop stops early; the last <= 5 pixels are scalar.
#[inline(always)]
pub unsafe fn rgb_to_rgba<X: Vec128>(src: &[u8], dst: &mut [u8], alpha: u8) {
    let npix = src.len() / 3;
    let shuf = X::from_bytes(&RGBA_SHUF);
    let am = [0, 0, 0, alpha, 0, 0, 0, alpha, 0, 0, 0, alpha, 0, 0, 0, alpha];
    let amask = X::from_bytes(&am);
    let (s, d) = (src.as_ptr(), dst.as_mut_ptr());
    let mut i = 0usize;
    while i + 6 <= npix {   // 3i + 16 <= 3 * npix
        X::store(d.add(4 * i), X::or(X::shuffle8(X::load(s.add(3 * i)), shuf), amask));
        i += 4;
    }
    scalar::rgb_to_rgba(&src[3 * i..], &mut dst[4 * i..], alpha);
}

/// 16-bit big-endian -> 8-bit (high byte): mask the low byte of each 16-bit lane, pack two.
#[inline(always)]
pub unsafe fn u16be_to_u8<X: Vec128>(src: &[u8], dst: &mut [u8]) {
    let n = dst.len();
    let lo8 = X::set1_u16(0x00FF);
    let (s, d) = (src.as_ptr(), dst.as_mut_ptr());
    let mut i = 0usize;
    while i + 16 <= n {
        let a = X::and(X::load(s.add(2 * i)), lo8);
        let b = X::and(X::load(s.add(2 * i + 16)), lo8);
        X::store(d.add(i), X::pack16(a, b));
        i += 16;
    }
    scalar::u16be_to_u8(&src[2 * i..], &mut dst[i..]);
}
