//! `std::simd` backend: the generic kernels on [`StdSimd`] vectors, plus an Adler-32 written with
//! `std::simd` (widening adds and a multiply by the 32..1 weights). CRC-32 stays the scalar
//! slicing-by-8 table: `std::simd` has no carry-less multiply.

use super::{check_rgba_contract, check_row_contract, check_u16_contract, generic, scalar, Kernels};
use crate::v128::stdsimd::StdSimd;
use std::simd::num::SimdUint;
use std::simd::{u32x32, u8x32};

fn unfilter_row(filter: u8, bpp: usize, row: &mut [u8], prev: Option<&[u8]>, len: usize) {
    check_row_contract(bpp, row, prev, len);
    unsafe { generic::unfilter_row::<StdSimd>(filter, bpp, row, prev, len) }
}
fn rgb_to_rgba(src: &[u8], dst: &mut [u8], alpha: u8) {
    check_rgba_contract(src, dst);
    unsafe { generic::rgb_to_rgba::<StdSimd>(src, dst, alpha) }
}
fn u16be_to_u8(src: &[u8], dst: &mut [u8]) {
    check_u16_contract(src, dst);
    unsafe { generic::u16be_to_u8::<StdSimd>(src, dst) }
}

/// Adler-32 over 32-byte chunks with lane-wise accumulators, reduced once per `ADLER_NMAX` block.
/// For chunk `x` with the running sums `s1`, `s2` before it: `s2 += 32 * s1 + sum((32 - i) * x_i)`
/// and `s1 += sum(x_i)`. Lane-wise, `v1` sums the bytes, `v2` the weighted bytes, and `vp` the
/// value of `v1` before each chunk (so `sum(vp)` is the sum of the earlier chunks' byte sums).
/// Within one block (at most 173 chunks) every lane and every reduced sum fits in a u32.
pub fn adler32(adler: u32, mut p: &[u8]) -> u32 {
    const W: [u32; 32] = {
        let mut w = [0u32; 32];
        let mut i = 0;
        while i < 32 {
            w[i] = 32 - i as u32;
            i += 1;
        }
        w
    };
    let weights = u32x32::from_array(W);
    let (mut s1, mut s2) = (adler & 0xFFFF, adler >> 16);
    while p.len() >= 32 {
        let k = p.len().min(scalar::ADLER_NMAX) & !31;
        let (blk, rest) = p.split_at(k);
        let (mut v1, mut v2, mut vp) = (u32x32::splat(0), u32x32::splat(0), u32x32::splat(0));
        for c in blk.chunks_exact(32) {
            let x = u8x32::from_slice(c).cast::<u32>();
            vp += v1;
            v1 += x;
            v2 += x * weights;
        }
        let n = (k / 32) as u32;
        let base = scalar::ADLER_BASE;
        s2 = (s2 + 32 * (n * s1 % base) + 32 * (vp.reduce_sum() % base) + v2.reduce_sum()) % base;
        s1 = (s1 + v1.reduce_sum()) % scalar::ADLER_BASE;
        p = rest;
    }
    if !p.is_empty() {
        return scalar::adler32((s2 << 16) | s1, p);
    }
    (s2 << 16) | s1
}

pub(crate) static KERNELS: Kernels = Kernels {
    name: "stdsimd",
    unfilter_row,
    crc32: scalar::crc32,
    adler32,
    rgb_to_rgba,
    u16be_to_u8,
};
