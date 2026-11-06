//! AArch64 NEON backend (+ PMULL CRC when the CPU has it).
//!
//! Transcribed from the C backend and `v128_neon.h`. Verified with `cargo test --target
//! aarch64-unknown-linux-gnu` under qemu-aarch64 (see the simd_adventures flake).

use super::{check_rgba_contract, check_row_contract, check_u16_contract, generic, scalar, Kernels};
use crate::v128::neon::{bsl_u8, Neon};
use core::arch::aarch64::*;

fn unfilter_row(filter: u8, bpp: usize, row: &mut [u8], prev: Option<&[u8]>, len: usize) {
    check_row_contract(bpp, row, prev, len);
    unsafe { generic::unfilter_row::<Neon>(filter, bpp, row, prev, len) }
}
fn rgb_to_rgba(src: &[u8], dst: &mut [u8], alpha: u8) {
    check_rgba_contract(src, dst);
    unsafe { generic::rgb_to_rgba::<Neon>(src, dst, alpha) }
}
fn u16be_to_u8(src: &[u8], dst: &mut [u8]) {
    check_u16_contract(src, dst);
    unsafe { generic::u16be_to_u8::<Neon>(src, dst) }
}

/// sum b_i: vpaddlq_u8 then vpadalq_u16; weighted sum: vmull_u8/vmlal_u8 with weights 16..1.
fn adler32(adler: u32, mut p: &[u8]) -> u32 {
    let (mut s1, mut s2) = (adler & 0xFFFF, adler >> 16);
    const WT: [u8; 16] = [16, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1];
    unsafe {
        let wlo = vld1_u8(WT.as_ptr());
        let whi = vld1_u8(WT.as_ptr().add(8));
        while p.len() >= 16 {
            let k = p.len().min(scalar::ADLER_NMAX) & !15;
            let (blk, rest) = p.split_at(k);
            let mut vs1 = vsetq_lane_u32::<0>(s1, vdupq_n_u32(0));
            let mut vs2 = vsetq_lane_u32::<0>(s2, vdupq_n_u32(0));
            let mut vs1_0 = vdupq_n_u32(0);
            for c in blk.chunks_exact(16) {
                let d = vld1q_u8(c.as_ptr());
                vs1_0 = vaddq_u32(vs1_0, vs1);
                vs1 = vpadalq_u16(vs1, vpaddlq_u8(d));
                let w = vmlal_u8(vmull_u8(vget_low_u8(d), wlo), vget_high_u8(d), whi);
                vs2 = vpadalq_u16(vs2, w);
            }
            let h1 = vaddvq_u32(vs1) as u64;
            let h2 = vaddvq_u32(vs2) as u64 + 16 * vaddvq_u32(vs1_0) as u64;
            s1 = (h1 % scalar::ADLER_BASE as u64) as u32;
            s2 = (h2 % scalar::ADLER_BASE as u64) as u32;
            p = rest;
        }
    }
    if !p.is_empty() { return scalar::adler32((s2 << 16) | s1, p); }
    (s2 << 16) | s1
}

// ---- CRC-32 with PMULL: same constants and reduction as the x86 version --------------------
const CRC_K576: u64 = 0x154442bd4;
const CRC_K512: u64 = 0x1c6e41596;
const CRC_K192: u64 = 0x1751997d0;
const CRC_K128: u64 = 0x0ccaa009e;
const CRC_K96: u64 = 0x163cd6124;
const CRC_MU: u64 = 0x1f7011641;
const CRC_PR: u64 = 0x1db710641;

#[inline(always)]
unsafe fn clmul_lo(a: uint64x2_t, k: u64) -> uint64x2_t { vreinterpretq_u64_p128(vmull_p64(vgetq_lane_u64::<0>(a), k)) }
#[inline(always)]
unsafe fn clmul_hi(a: uint64x2_t, k: u64) -> uint64x2_t { vreinterpretq_u64_p128(vmull_p64(vgetq_lane_u64::<1>(a), k)) }
#[inline(always)]
unsafe fn fold128(x: uint64x2_t, klo: u64, khi: u64) -> uint64x2_t { veorq_u64(clmul_lo(x, klo), clmul_hi(x, khi)) }
#[inline(always)]
unsafe fn ld(p: &[u8], off: usize) -> uint64x2_t { vreinterpretq_u64_u8(vld1q_u8(p.as_ptr().add(off))) }
/// byte shifts in the x86 sense: slli = bytes move to higher lanes
#[inline(always)]
unsafe fn slli<const N: i32>(v: uint64x2_t) -> uint64x2_t { vreinterpretq_u64_u8(bsl_u8::<N>(vreinterpretq_u8_u64(v))) }
#[inline(always)]
unsafe fn srli<const N: i32>(v: uint64x2_t) -> uint64x2_t { vreinterpretq_u64_u8(vextq_u8::<N>(vreinterpretq_u8_u64(v), vdupq_n_u8(0))) }
#[inline(always)]
unsafe fn and_mask(a: uint64x2_t, m: [u32; 4]) -> uint64x2_t { vreinterpretq_u64_u32(vandq_u32(vreinterpretq_u32_u64(a), vld1q_u32(m.as_ptr()))) }

#[inline(always)]
unsafe fn reduce128(acc: uint64x2_t) -> u32 {
    let t = veorq_u64(clmul_lo(acc, CRC_K96), and_mask(srli::<4>(acc), [0, !0, !0, 0]));
    let t2 = veorq_u64(clmul_lo(slli::<4>(t), CRC_K96), and_mask(t, [0, !0, !0, !0]));
    let z = veorq_u64(slli::<4>(clmul_lo(t2, CRC_K96)), and_mask(t2, [0, 0, !0, !0]));
    let q = and_mask(clmul_lo(and_mask(srli::<8>(z), [!0, 0, 0, 0]), CRC_MU), [!0, 0, 0, 0]);
    let r = veorq_u64(clmul_lo(q, CRC_PR), srli::<8>(z));
    vgetq_lane_u32::<1>(vreinterpretq_u32_u64(r))
}

#[target_feature(enable = "aes")]   // PMULL lives in the "aes" (crypto) feature set
unsafe fn crc32_pmull_impl(crc: u32, p: &[u8]) -> u32 {
    if p.len() < 64 { return scalar::crc32(crc, p); }
    let crc = !crc;
    let mut x0 = veorq_u64(ld(p, 0), vreinterpretq_u64_u32(vsetq_lane_u32::<0>(crc, vdupq_n_u32(0))));
    let mut x1 = ld(p, 16);
    let mut x2 = ld(p, 32);
    let mut x3 = ld(p, 48);
    let mut off = 64;
    while p.len() - off >= 64 {
        x0 = veorq_u64(fold128(x0, CRC_K576, CRC_K512), ld(p, off));
        x1 = veorq_u64(fold128(x1, CRC_K576, CRC_K512), ld(p, off + 16));
        x2 = veorq_u64(fold128(x2, CRC_K576, CRC_K512), ld(p, off + 32));
        x3 = veorq_u64(fold128(x3, CRC_K576, CRC_K512), ld(p, off + 48));
        off += 64;
    }
    let mut acc = veorq_u64(fold128(x0, CRC_K192, CRC_K128), x1);
    acc = veorq_u64(fold128(acc, CRC_K192, CRC_K128), x2);
    acc = veorq_u64(fold128(acc, CRC_K192, CRC_K128), x3);
    while p.len() - off >= 16 {
        acc = veorq_u64(fold128(acc, CRC_K192, CRC_K128), ld(p, off));
        off += 16;
    }
    let raw = reduce128(acc);
    scalar::crc32(!raw, &p[off..])
}

fn crc32(crc: u32, p: &[u8]) -> u32 {
    if std::arch::is_aarch64_feature_detected!("aes") { unsafe { crc32_pmull_impl(crc, p) } } else { scalar::crc32(crc, p) }
}

pub(crate) static KERNELS_NEON: Kernels = Kernels {
    name: "neon",
    unfilter_row,
    crc32,
    adler32,
    rgb_to_rgba,
    u16be_to_u8,
};
