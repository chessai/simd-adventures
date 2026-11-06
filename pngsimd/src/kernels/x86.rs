//! x86 backends: SSE4.1+PCLMULQDQ, AVX2, AVX-512 (F/BW/VL/VBMI + VPCLMULQDQ).
//!
//! Every entry point is a `#[target_feature]` function wrapped in a safe function; the generic
//! kernels inline into it and pick up the features. The 128-bit generic kernels are reused by
//! the wider backends (where they become VEX/EVEX encoded); only throughput-bound kernels get
//! 256/512-bit versions, because width is irrelevant to the pixel-serial filters.

use super::{check_rgba_contract, check_row_contract, check_u16_contract, generic, scalar, Kernels};
use crate::v128::sse::{Sse, SseEvex, SseVex};
#[cfg(target_arch = "x86")]
use core::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;

// ============================================================================================
// CRC-32 by carry-less-multiply folding. Constants and the final reduction were derived and
// checked in scripts/crc_constants.py (see crc32_clmul_x86.h in the C version for the long
// comment). Frame: bit i of a 128-bit register is the coefficient of x^(127-i).
//   clmul(qword Q, K(k)) == q(x) * (x^(k-32) mod P) * x^32,   K(k) = reflect32(x^(k-32) mod P) << 1
// ============================================================================================
const CRC_K1088: i64 = 0x1e88ef372;
const CRC_K1024: i64 = 0x14a7fe880;
const CRC_K576: i64 = 0x154442bd4;
const CRC_K512: i64 = 0x1c6e41596;
const CRC_K192: i64 = 0x1751997d0;
const CRC_K128: i64 = 0x0ccaa009e;
const CRC_K96: i64 = 0x163cd6124;
const CRC_MU: i64 = 0x1f7011641; // floor(x^64 / P), reflected, 33 bits
const CRC_PR: i64 = 0x1db710641; // P, reflected, 33 bits

#[inline(always)]
unsafe fn fold128(x: __m128i, k: __m128i) -> __m128i {
    _mm_xor_si128(_mm_clmulepi64_si128::<0x00>(x, k), _mm_clmulepi64_si128::<0x11>(x, k))
}

/// 128-bit accumulator -> raw (not yet complemented) 32-bit CRC
#[inline(always)]
unsafe fn reduce128(acc: __m128i) -> u32 {
    let k96 = _mm_set_epi64x(0, CRC_K96);
    let mu = _mm_set_epi64x(0, CRC_MU);
    let pr = _mm_set_epi64x(0, CRC_PR);
    let m_mid = _mm_set_epi32(0, -1, -1, 0);    // bits 32..95
    let m_hi96 = _mm_set_epi32(-1, -1, -1, 0);  // bits 32..127
    let m_highq = _mm_set_epi32(-1, -1, 0, 0);  // bits 64..127
    let m_lo32 = _mm_set_epi32(0, 0, 0, -1);
    // 1. CRC = A*x^32 mod P: t = l*x^96 + h*x^32                        (bits 1..95)
    let t = _mm_xor_si128(_mm_clmulepi64_si128::<0x00>(acc, k96), _mm_and_si128(_mm_srli_si128::<4>(acc), m_mid));
    // 2. fold the top 32 degrees down: t2 = V*x^32, deg V <= 63        (bits 32..95)
    let t2 = _mm_xor_si128(_mm_clmulepi64_si128::<0x00>(_mm_slli_si128::<4>(t), k96), _mm_and_si128(t, m_hi96));
    // 3. fold once more, shifting the product so the frame's x^32 is removed: Z in bits 64..127
    let z = _mm_xor_si128(_mm_slli_si128::<4>(_mm_clmulepi64_si128::<0x00>(t2, k96)), _mm_and_si128(t2, m_highq));
    // 4. Barrett: q = floor(z_hi * mu / x^32), r = (Z + q*P) mod x^32
    let q = _mm_and_si128(_mm_clmulepi64_si128::<0x00>(_mm_and_si128(_mm_srli_si128::<8>(z), m_lo32), mu), m_lo32);
    let r = _mm_xor_si128(_mm_clmulepi64_si128::<0x00>(q, pr), _mm_srli_si128::<8>(z));
    _mm_extract_epi32::<1>(r) as u32
}

#[inline(always)]
unsafe fn ld128(p: &[u8], off: usize) -> __m128i { _mm_loadu_si128(p.as_ptr().add(off) as *const __m128i) }

#[target_feature(enable = "pclmulqdq,sse4.1")]
unsafe fn crc32_clmul_impl(crc: u32, p: &[u8]) -> u32 {
    if p.len() < 64 { return scalar::crc32(crc, p); }
    let k512 = _mm_set_epi64x(CRC_K512, CRC_K576);
    let k128 = _mm_set_epi64x(CRC_K128, CRC_K192);
    let crc = !crc;
    let mut x0 = _mm_xor_si128(ld128(p, 0), _mm_cvtsi32_si128(crc as i32));
    let mut x1 = ld128(p, 16);
    let mut x2 = ld128(p, 32);
    let mut x3 = ld128(p, 48);
    let mut off = 64;
    // four independent accumulators hide the ~7-cycle pclmul latency
    while p.len() - off >= 64 {
        x0 = _mm_xor_si128(fold128(x0, k512), ld128(p, off));
        x1 = _mm_xor_si128(fold128(x1, k512), ld128(p, off + 16));
        x2 = _mm_xor_si128(fold128(x2, k512), ld128(p, off + 32));
        x3 = _mm_xor_si128(fold128(x3, k512), ld128(p, off + 48));
        off += 64;
    }
    let mut acc = _mm_xor_si128(fold128(x0, k128), x1);
    acc = _mm_xor_si128(fold128(acc, k128), x2);
    acc = _mm_xor_si128(fold128(acc, k128), x3);
    while p.len() - off >= 16 {
        acc = _mm_xor_si128(fold128(acc, k128), ld128(p, off));
        off += 16;
    }
    let raw = reduce128(acc);
    scalar::crc32(!raw, &p[off..])   // up to 15 trailing bytes through the table
}

fn crc32_clmul(crc: u32, p: &[u8]) -> u32 { unsafe { crc32_clmul_impl(crc, p) } }

// ============================================================================================
// Adler-32 with widening multiply-add (see adler32_x86.h for the derivation and bounds).
// ============================================================================================
#[target_feature(enable = "ssse3,sse4.1")]
unsafe fn adler32_sse_impl(adler: u32, mut p: &[u8]) -> u32 {
    let (mut s1, mut s2) = (adler & 0xFFFF, adler >> 16);
    let zero = _mm_setzero_si128();
    let ones = _mm_set1_epi16(1);
    let w = _mm_setr_epi8(16, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1);
    while p.len() >= 16 {
        let k = p.len().min(scalar::ADLER_NMAX) & !15;
        let (blk, rest) = p.split_at(k);
        let mut vs1 = _mm_cvtsi32_si128(s1 as i32);
        let mut vs2 = _mm_cvtsi32_si128(s2 as i32);
        let mut vs1_0 = zero;
        for c in blk.chunks_exact(16) {
            let d = _mm_loadu_si128(c.as_ptr() as *const __m128i);
            vs1_0 = _mm_add_epi32(vs1_0, vs1);
            vs1 = _mm_add_epi32(vs1, _mm_sad_epu8(d, zero));
            vs2 = _mm_add_epi32(vs2, _mm_madd_epi16(_mm_maddubs_epi16(d, w), ones));
        }
        let mut t = [0u32; 4];
        _mm_storeu_si128(t.as_mut_ptr() as *mut __m128i, vs1);
        let h1 = t[0] as u64 + t[2] as u64;
        _mm_storeu_si128(t.as_mut_ptr() as *mut __m128i, vs2);
        let mut h2 = t.iter().map(|&x| x as u64).sum::<u64>();
        _mm_storeu_si128(t.as_mut_ptr() as *mut __m128i, vs1_0);
        h2 += 16 * t.iter().map(|&x| x as u64).sum::<u64>();
        s1 = (h1 % scalar::ADLER_BASE as u64) as u32;
        s2 = (h2 % scalar::ADLER_BASE as u64) as u32;
        p = rest;
    }
    if !p.is_empty() { return scalar::adler32((s2 << 16) | s1, p); }
    (s2 << 16) | s1
}
fn adler32_sse(adler: u32, p: &[u8]) -> u32 { unsafe { adler32_sse_impl(adler, p) } }

#[target_feature(enable = "avx2")]
unsafe fn adler32_avx2_impl(adler: u32, mut p: &[u8]) -> u32 {
    let (mut s1, mut s2) = (adler & 0xFFFF, adler >> 16);
    let zero = _mm256_setzero_si256();
    let ones = _mm256_set1_epi16(1);
    let w = _mm256_setr_epi8(32, 31, 30, 29, 28, 27, 26, 25, 24, 23, 22, 21, 20, 19, 18, 17,
                             16, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1);
    while p.len() >= 32 {
        let k = p.len().min(scalar::ADLER_NMAX) & !31;
        let (blk, rest) = p.split_at(k);
        let mut vs1 = _mm256_zextsi128_si256(_mm_cvtsi32_si128(s1 as i32));
        let mut vs2 = _mm256_zextsi128_si256(_mm_cvtsi32_si128(s2 as i32));
        let mut vs1_0 = zero;
        for c in blk.chunks_exact(32) {
            let d = _mm256_loadu_si256(c.as_ptr() as *const __m256i);
            vs1_0 = _mm256_add_epi32(vs1_0, vs1);
            vs1 = _mm256_add_epi32(vs1, _mm256_sad_epu8(d, zero));
            vs2 = _mm256_add_epi32(vs2, _mm256_madd_epi16(_mm256_maddubs_epi16(d, w), ones));
        }
        let mut t = [0u32; 8];
        _mm256_storeu_si256(t.as_mut_ptr() as *mut __m256i, vs1);
        let h1 = t[0] as u64 + t[2] as u64 + t[4] as u64 + t[6] as u64;
        _mm256_storeu_si256(t.as_mut_ptr() as *mut __m256i, vs2);
        let mut h2 = t.iter().map(|&x| x as u64).sum::<u64>();
        _mm256_storeu_si256(t.as_mut_ptr() as *mut __m256i, vs1_0);
        h2 += 32 * t.iter().map(|&x| x as u64).sum::<u64>();
        s1 = (h1 % scalar::ADLER_BASE as u64) as u32;
        s2 = (h2 % scalar::ADLER_BASE as u64) as u32;
        p = rest;
    }
    if !p.is_empty() { return adler32_sse_impl((s2 << 16) | s1, p); }
    (s2 << 16) | s1
}
fn adler32_avx2(adler: u32, p: &[u8]) -> u32 { unsafe { adler32_avx2_impl(adler, p) } }

// ============================================================================================
// SSE4.1 backend
// ============================================================================================
#[target_feature(enable = "ssse3,sse4.1")]
unsafe fn unfilter_row_sse41_impl(filter: u8, bpp: usize, row: &mut [u8], prev: Option<&[u8]>, len: usize) {
    generic::unfilter_row::<Sse>(filter, bpp, row, prev, len)
}
fn unfilter_row_sse41(filter: u8, bpp: usize, row: &mut [u8], prev: Option<&[u8]>, len: usize) {
    check_row_contract(bpp, row, prev, len);
    unsafe { unfilter_row_sse41_impl(filter, bpp, row, prev, len) }
}
#[target_feature(enable = "ssse3,sse4.1")]
unsafe fn rgb_to_rgba_sse41_impl(src: &[u8], dst: &mut [u8], alpha: u8) { generic::rgb_to_rgba::<Sse>(src, dst, alpha) }
fn rgb_to_rgba_sse41(src: &[u8], dst: &mut [u8], alpha: u8) { check_rgba_contract(src, dst); unsafe { rgb_to_rgba_sse41_impl(src, dst, alpha) } }
#[target_feature(enable = "ssse3,sse4.1")]
unsafe fn u16be_to_u8_sse41_impl(src: &[u8], dst: &mut [u8]) { generic::u16be_to_u8::<Sse>(src, dst) }
fn u16be_to_u8_sse41(src: &[u8], dst: &mut [u8]) { check_u16_contract(src, dst); unsafe { u16be_to_u8_sse41_impl(src, dst) } }

pub(crate) static KERNELS_SSE41: Kernels = Kernels {
    name: "sse41",
    unfilter_row: unfilter_row_sse41,
    crc32: crc32_clmul,
    adler32: adler32_sse,
    rgb_to_rgba: rgb_to_rgba_sse41,
    u16be_to_u8: u16be_to_u8_sse41,
};

// ============================================================================================
// AVX2 backend
// ============================================================================================
#[target_feature(enable = "avx2")]
unsafe fn avx2_filter_up(row: *mut u8, prev: *const u8, len: usize) {
    let mut i = 0usize;
    while i + 32 <= len {
        _mm256_storeu_si256(row.add(i) as *mut __m256i,
            _mm256_add_epi8(_mm256_loadu_si256(row.add(i) as *const __m256i), _mm256_loadu_si256(prev.add(i) as *const __m256i)));
        i += 32;
    }
    while i + 16 <= len {
        _mm_storeu_si128(row.add(i) as *mut __m128i,
            _mm_add_epi8(_mm_loadu_si128(row.add(i) as *const __m128i), _mm_loadu_si128(prev.add(i) as *const __m128i)));
        i += 16;
    }
    while i < len { *row.add(i) = (*row.add(i)).wrapping_add(*prev.add(i)); i += 1; }
}

/// Experimental 256-bit Sub scan for bpp = 4 (exported for the benchmark). In-lane byte shifts
/// do the log-step scan inside each 128-bit lane; `vpermd` then carries the low lane's last
/// pixel into the high lane; finally the previous block's last pixel is added. The loop-carried
/// chain is permute+add (4 cycles per 32 bytes) versus pshufd+add (2 cycles per 16 bytes) at
/// 128 bits: the same bytes/cycle at the latency bound, with more uops. Measured within noise.
#[target_feature(enable = "avx2")]
pub unsafe fn filter_sub4_wide_impl(row: &mut [u8], len: usize) {
    let r = row.as_mut_ptr();
    let lane_bc = _mm256_setr_epi32(0, 0, 0, 0, 3, 3, 3, 3);
    let lane_mask = _mm256_setr_epi32(0, 0, 0, 0, -1, -1, -1, -1);
    let bc7 = _mm256_set1_epi32(7);
    let mut carry = _mm256_setzero_si256();
    let mut i = 0usize;
    while i + 32 <= len {
        let mut x = _mm256_loadu_si256(r.add(i) as *const __m256i);
        x = _mm256_add_epi8(x, _mm256_slli_si256::<4>(x));
        x = _mm256_add_epi8(x, _mm256_slli_si256::<8>(x));
        x = _mm256_add_epi8(x, _mm256_and_si256(_mm256_permutevar8x32_epi32(x, lane_bc), lane_mask));
        x = _mm256_add_epi8(x, carry);
        _mm256_storeu_si256(r.add(i) as *mut __m256i, x);
        carry = _mm256_permutevar8x32_epi32(x, bc7);
        i += 32;
    }
    if i < 4 { i = 4; }
    while i < len { *r.add(i) = (*r.add(i)).wrapping_add(*r.add(i - 4)); i += 1; }
}
pub fn filter_sub4_wide(row: &mut [u8], len: usize) {
    assert!(row.len() >= len && len >= 4 && is_x86_feature_detected!("avx2"));
    unsafe { filter_sub4_wide_impl(row, len) }
}

#[target_feature(enable = "avx2")]
unsafe fn unfilter_row_avx2_impl(filter: u8, bpp: usize, row: &mut [u8], prev: Option<&[u8]>, len: usize) {
    match filter {
        1 => generic::filter_sub::<SseVex>(bpp, row.as_mut_ptr(), len),
        2 => if let Some(p) = prev { avx2_filter_up(row.as_mut_ptr(), p.as_ptr(), len) },
        3 => generic::filter_avg::<SseVex>(bpp, row, prev, len),
        4 => generic::filter_paeth::<SseVex>(bpp, row, prev, len),
        _ => {}
    }
}
fn unfilter_row_avx2(filter: u8, bpp: usize, row: &mut [u8], prev: Option<&[u8]>, len: usize) {
    check_row_contract(bpp, row, prev, len);
    unsafe { unfilter_row_avx2_impl(filter, bpp, row, prev, len) }
}

/// RGB -> RGBA, 8 pixels per iteration. `vpshufb` is lane-local, so each 128-bit lane is loaded
/// from its own 12-byte source offset.
#[target_feature(enable = "avx2")]
unsafe fn rgb_to_rgba_avx2_impl(src: &[u8], dst: &mut [u8], alpha: u8) {
    let npix = src.len() / 3;
    let shuf = _mm256_broadcastsi128_si256(_mm_loadu_si128(generic::RGBA_SHUF.as_ptr() as *const __m128i));
    let amask = _mm256_set1_epi32(((alpha as u32) << 24) as i32);
    let (s, d) = (src.as_ptr(), dst.as_mut_ptr());
    let mut i = 0usize;
    while i + 10 <= npix {   // 3i + 12 + 16 <= 3 * npix
        let lo = _mm_loadu_si128(s.add(3 * i) as *const __m128i);
        let hi = _mm_loadu_si128(s.add(3 * i + 12) as *const __m128i);
        let x = _mm256_inserti128_si256::<1>(_mm256_castsi128_si256(lo), hi);
        _mm256_storeu_si256(d.add(4 * i) as *mut __m256i, _mm256_or_si256(_mm256_shuffle_epi8(x, shuf), amask));
        i += 8;
    }
    generic::rgb_to_rgba::<SseVex>(&src[3 * i..], &mut dst[4 * i..], alpha)
}
fn rgb_to_rgba_avx2(src: &[u8], dst: &mut [u8], alpha: u8) { check_rgba_contract(src, dst); unsafe { rgb_to_rgba_avx2_impl(src, dst, alpha) } }

/// 16 -> 8, 32 samples per iteration; `vpackuswb` packs per lane, so a `vpermq` fixes the order.
#[target_feature(enable = "avx2")]
unsafe fn u16be_to_u8_avx2_impl(src: &[u8], dst: &mut [u8]) {
    let n = dst.len();
    let lo8 = _mm256_set1_epi16(0x00FF);
    let (s, d) = (src.as_ptr(), dst.as_mut_ptr());
    let mut i = 0usize;
    while i + 32 <= n {
        let a = _mm256_and_si256(_mm256_loadu_si256(s.add(2 * i) as *const __m256i), lo8);
        let b = _mm256_and_si256(_mm256_loadu_si256(s.add(2 * i + 32) as *const __m256i), lo8);
        let p = _mm256_permute4x64_epi64::<0xD8>(_mm256_packus_epi16(a, b));
        _mm256_storeu_si256(d.add(i) as *mut __m256i, p);
        i += 32;
    }
    generic::u16be_to_u8::<SseVex>(&src[2 * i..], &mut dst[i..])
}
fn u16be_to_u8_avx2(src: &[u8], dst: &mut [u8]) { check_u16_contract(src, dst); unsafe { u16be_to_u8_avx2_impl(src, dst) } }

pub(crate) static KERNELS_AVX2: Kernels = Kernels {
    name: "avx2",
    unfilter_row: unfilter_row_avx2,
    crc32: crc32_clmul,
    adler32: adler32_avx2,
    rgb_to_rgba: rgb_to_rgba_avx2,
    u16be_to_u8: u16be_to_u8_avx2,
};

// ============================================================================================
// AVX-512 backend (F/BW/VL/VBMI + VPCLMULQDQ). See REPORT.md §8 for what each instruction buys.
// ============================================================================================
const AVX512: &str = "avx512f,avx512bw,avx512vl,avx512vbmi,vpclmulqdq,pclmulqdq";

#[target_feature(enable = "avx512f,avx512bw,avx512vl")]
unsafe fn avx512_filter_up(row: *mut u8, prev: *const u8, len: usize) {
    let mut i = 0usize;
    while i + 64 <= len {
        _mm512_storeu_si512(row.add(i) as *mut _,
            _mm512_add_epi8(_mm512_loadu_si512(row.add(i) as *const _), _mm512_loadu_si512(prev.add(i) as *const _)));
        i += 64;
    }
    if i < len {
        let m: __mmask64 = (1u64 << (len - i)) - 1;   // len - i < 64
        let a = _mm512_maskz_loadu_epi8(m, row.add(i) as *const i8);
        let b = _mm512_maskz_loadu_epi8(m, prev.add(i) as *const i8);
        _mm512_mask_storeu_epi8(row.add(i) as *mut i8, m, _mm512_add_epi8(a, b));
    }
}

#[target_feature(enable = "avx512f,avx512bw,avx512vl")]
unsafe fn unfilter_row_avx512_impl(filter: u8, bpp: usize, row: &mut [u8], prev: Option<&[u8]>, len: usize) {
    match filter {
        1 => generic::filter_sub::<SseEvex>(bpp, row.as_mut_ptr(), len),
        2 => if let Some(p) = prev { avx512_filter_up(row.as_mut_ptr(), p.as_ptr(), len) },
        3 => generic::filter_avg::<SseEvex>(bpp, row, prev, len),
        4 => generic::filter_paeth::<SseEvex>(bpp, row, prev, len),
        _ => {}
    }
}
fn unfilter_row_avx512(filter: u8, bpp: usize, row: &mut [u8], prev: Option<&[u8]>, len: usize) {
    check_row_contract(bpp, row, prev, len);
    unsafe { unfilter_row_avx512_impl(filter, bpp, row, prev, len) }
}

/// CRC-32 with VPCLMULQDQ: one zmm holds four 128-bit accumulators (2 multiplies + 1 ternary
/// XOR per 64 bytes). One zmm alone is a latency chain (~9 B/cycle), so two accumulators 128
/// bytes apart (constants for x^1024) are used for the bulk.
#[target_feature(enable = "avx512f,avx512bw,avx512vl,vpclmulqdq,pclmulqdq")]
unsafe fn crc32_vpclmul_impl(crc: u32, p: &[u8]) -> u32 {
    if p.len() < 256 { return crc32_clmul_impl(crc, p); }
    let k512 = _mm512_set4_epi64(CRC_K512, CRC_K576, CRC_K512, CRC_K576);
    let k1024 = _mm512_set4_epi64(CRC_K1024, CRC_K1088, CRC_K1024, CRC_K1088);
    let k128 = _mm_set_epi64x(CRC_K128, CRC_K192);
    let ld = |off: usize| _mm512_loadu_si512(p.as_ptr().add(off) as *const _);
    let crc = !crc;
    let mut x = _mm512_xor_si512(ld(0), _mm512_zextsi128_si512(_mm_cvtsi32_si128(crc as i32)));
    let mut off = 64;
    if p.len() - off >= 192 {
        let mut y = ld(off);
        off += 64;
        while p.len() - off >= 128 {
            x = _mm512_ternarylogic_epi64::<0x96>(_mm512_clmulepi64_epi128::<0x00>(x, k1024), _mm512_clmulepi64_epi128::<0x11>(x, k1024), ld(off));
            y = _mm512_ternarylogic_epi64::<0x96>(_mm512_clmulepi64_epi128::<0x00>(y, k1024), _mm512_clmulepi64_epi128::<0x11>(y, k1024), ld(off + 64));
            off += 128;
        }
        x = _mm512_ternarylogic_epi64::<0x96>(_mm512_clmulepi64_epi128::<0x00>(x, k512), _mm512_clmulepi64_epi128::<0x11>(x, k512), y);
    }
    while p.len() - off >= 64 {
        x = _mm512_ternarylogic_epi64::<0x96>(_mm512_clmulepi64_epi128::<0x00>(x, k512), _mm512_clmulepi64_epi128::<0x11>(x, k512), ld(off));
        off += 64;
    }
    let mut acc = _mm_xor_si128(fold128(_mm512_castsi512_si128(x), k128), _mm512_extracti32x4_epi32::<1>(x));
    acc = _mm_xor_si128(fold128(acc, k128), _mm512_extracti32x4_epi32::<2>(x));
    acc = _mm_xor_si128(fold128(acc, k128), _mm512_extracti32x4_epi32::<3>(x));
    while p.len() - off >= 16 {
        acc = _mm_xor_si128(fold128(acc, k128), ld128(p, off));
        off += 16;
    }
    let raw = reduce128(acc);
    scalar::crc32(!raw, &p[off..])
}
fn crc32_vpclmul(crc: u32, p: &[u8]) -> u32 { unsafe { crc32_vpclmul_impl(crc, p) } }

/// Adler-32, 64-byte blocks: weights 64..1 → (64+63)·255 = 32385, just inside pmaddubsw's 16-bit lanes.
#[target_feature(enable = "avx512f,avx512bw,avx512vl")]
unsafe fn adler32_avx512_impl(adler: u32, mut p: &[u8]) -> u32 {
    let (mut s1, mut s2) = (adler & 0xFFFF, adler >> 16);
    let zero = _mm512_setzero_si512();
    let ones = _mm512_set1_epi16(1);
    let mut wt = [0u8; 64];
    for (i, w) in wt.iter_mut().enumerate() { *w = (64 - i) as u8; }
    let w = _mm512_loadu_si512(wt.as_ptr() as *const _);
    while p.len() >= 64 {
        let k = p.len().min(scalar::ADLER_NMAX) & !63;
        let (blk, rest) = p.split_at(k);
        let mut vs1 = _mm512_zextsi128_si512(_mm_cvtsi32_si128(s1 as i32));
        let mut vs2 = _mm512_zextsi128_si512(_mm_cvtsi32_si128(s2 as i32));
        let mut vs1_0 = zero;
        for c in blk.chunks_exact(64) {
            let d = _mm512_loadu_si512(c.as_ptr() as *const _);
            vs1_0 = _mm512_add_epi32(vs1_0, vs1);
            vs1 = _mm512_add_epi32(vs1, _mm512_sad_epu8(d, zero));
            vs2 = _mm512_add_epi32(vs2, _mm512_madd_epi16(_mm512_maddubs_epi16(d, w), ones));
        }
        let h1 = _mm512_reduce_add_epi64(vs1) as u64;
        // both reductions stay below 2^32 by the NMAX bound (REPORT.md §5.2)
        let h2 = (_mm512_reduce_add_epi32(vs2) as u32) as u64 + 64 * ((_mm512_reduce_add_epi32(vs1_0) as u32) as u64);
        s1 = (h1 % scalar::ADLER_BASE as u64) as u32;
        s2 = (h2 % scalar::ADLER_BASE as u64) as u32;
        p = rest;
    }
    if !p.is_empty() { return adler32_avx2_impl((s2 << 16) | s1, p); }
    (s2 << 16) | s1
}
fn adler32_avx512(adler: u32, p: &[u8]) -> u32 { unsafe { adler32_avx512_impl(adler, p) } }

/// RGB -> RGBA with VPERMB: 16 pixels per cross-lane byte permute (masked-zero so the alpha OR works).
#[target_feature(enable = "avx512f,avx512bw,avx512vl,avx512vbmi")]
unsafe fn rgb_to_rgba_avx512_impl(src: &[u8], dst: &mut [u8], alpha: u8) {
    let npix = src.len() / 3;
    let mut t = [0u8; 64];
    for j in 0..16 { t[4 * j] = (3 * j) as u8; t[4 * j + 1] = (3 * j + 1) as u8; t[4 * j + 2] = (3 * j + 2) as u8; }
    let idx = _mm512_loadu_si512(t.as_ptr() as *const _);
    let amask = _mm512_set1_epi32(((alpha as u32) << 24) as i32);
    let (s, d) = (src.as_ptr(), dst.as_mut_ptr());
    let mut i = 0usize;
    while i + 22 <= npix {   // 3i + 64 <= 3 * npix
        let x = _mm512_loadu_si512(s.add(3 * i) as *const _);
        _mm512_storeu_si512(d.add(4 * i) as *mut _, _mm512_or_si512(_mm512_maskz_permutexvar_epi8(0x7777_7777_7777_7777, idx, x), amask));
        i += 16;
    }
    generic::rgb_to_rgba::<SseEvex>(&src[3 * i..], &mut dst[4 * i..], alpha)
}
fn rgb_to_rgba_avx512(src: &[u8], dst: &mut [u8], alpha: u8) { check_rgba_contract(src, dst); unsafe { rgb_to_rgba_avx512_impl(src, dst, alpha) } }

/// 16 -> 8 with VPMOVWB, a real narrowing instruction.
#[target_feature(enable = "avx512f,avx512bw,avx512vl")]
unsafe fn u16be_to_u8_avx512_impl(src: &[u8], dst: &mut [u8]) {
    let n = dst.len();
    let (s, d) = (src.as_ptr(), dst.as_mut_ptr());
    let mut i = 0usize;
    while i + 32 <= n {
        _mm256_storeu_si256(d.add(i) as *mut __m256i, _mm512_cvtepi16_epi8(_mm512_loadu_si512(s.add(2 * i) as *const _)));
        i += 32;
    }
    generic::u16be_to_u8::<SseEvex>(&src[2 * i..], &mut dst[i..])
}
fn u16be_to_u8_avx512(src: &[u8], dst: &mut [u8]) { check_u16_contract(src, dst); unsafe { u16be_to_u8_avx512_impl(src, dst) } }

pub(crate) static KERNELS_AVX512: Kernels = Kernels {
    name: "avx512",
    unfilter_row: unfilter_row_avx512,
    crc32: crc32_vpclmul,
    adler32: adler32_avx512,
    rgb_to_rgba: rgb_to_rgba_avx512,
    u16be_to_u8: u16be_to_u8_avx512,
};

#[allow(dead_code)]
const _: &str = AVX512;
