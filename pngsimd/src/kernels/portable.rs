//! Portable backend: the generic kernels on the plain-Rust `[u8; 16]` vector type.

use super::{check_rgba_contract, check_row_contract, check_u16_contract, generic, scalar, Kernels};
use crate::v128::portable::Portable;

fn unfilter_row(filter: u8, bpp: usize, row: &mut [u8], prev: Option<&[u8]>, len: usize) {
    check_row_contract(bpp, row, prev, len);
    unsafe { generic::unfilter_row::<Portable>(filter, bpp, row, prev, len) }
}
fn rgb_to_rgba(src: &[u8], dst: &mut [u8], alpha: u8) {
    check_rgba_contract(src, dst);
    unsafe { generic::rgb_to_rgba::<Portable>(src, dst, alpha) }
}
fn u16be_to_u8(src: &[u8], dst: &mut [u8]) {
    check_u16_contract(src, dst);
    unsafe { generic::u16be_to_u8::<Portable>(src, dst) }
}

/// Adler-32 in "block of 16, weighted sum" form so the auto-vectoriser sees two independent
/// reductions per block (the naive loop's dependency chain prevents vectorising it directly).
pub fn adler32(adler: u32, mut p: &[u8]) -> u32 {
    let (mut s1, mut s2) = (adler & 0xFFFF, adler >> 16);
    while p.len() >= 16 {
        let k = p.len().min(scalar::ADLER_NMAX) & !15;
        let (blk, rest) = p.split_at(k);
        for c in blk.chunks_exact(16) {
            let mut sum = 0u32;
            let mut wsum = 0u32;
            for i in 0..16 { sum += c[i] as u32; wsum += (16 - i as u32) * c[i] as u32; }
            s2 += 16 * s1 + wsum;
            s1 += sum;
        }
        s1 %= scalar::ADLER_BASE;
        s2 %= scalar::ADLER_BASE;
        p = rest;
    }
    if !p.is_empty() { return scalar::adler32((s2 << 16) | s1, p); }
    (s2 << 16) | s1
}

pub(crate) static KERNELS: Kernels = Kernels {
    name: "portable",
    unfilter_row,
    crc32: scalar::crc32,
    adler32,
    rgb_to_rgba,
    u16be_to_u8,
};
