//! Scalar reference kernels: the correctness oracle for every vector backend and the baseline
//! for all speedup numbers.
//!
//! The filters carry the left pixel (and the upper-left pixel for Paeth) in registers rather
//! than re-reading it from the row — the textbook `row[i] += row[i-bpp]` loop is bound by
//! store-to-load forwarding latency and is kept separately as [`unfilter_row_naive`] for the
//! benchmarks only. The per-pixel inner loops are over a const-generic `BPP`, so the carried
//! pixel is `BPP` scalar registers after unrolling.

#[inline(always)]
fn paeth_pred(a: i32, b: i32, c: i32) -> u8 {
    // the PNG spec's formulation, branches and all (as libpng's C code has it)
    let p = a + b - c;
    let pa = (p - a).abs();
    let pb = (p - b).abs();
    let pc = (p - c).abs();
    (if pa <= pb && pa <= pc { a } else if pb <= pc { b } else { c }) as u8
}

#[inline(always)]
fn paeth_pred_branchless(a: i32, b: i32, c: i32) -> u8 {
    let p = a + b - c;
    let (pa, pb, pc) = ((p - a).abs(), (p - b).abs(), (p - c).abs());
    let use_a = -((pa <= pb && pa <= pc) as i32);
    let use_b = -((pb <= pc) as i32);
    let bc = c ^ ((b ^ c) & use_b);
    (bc ^ ((a ^ bc) & use_a)) as u8
}

fn sub_loop<const BPP: usize>(row: &mut [u8], len: usize) {
    let mut a = [0u8; BPP];
    a.copy_from_slice(&row[..BPP]);
    let mut it = row[BPP..len].chunks_exact_mut(BPP);
    for px in &mut it {
        for j in 0..BPP {
            a[j] = a[j].wrapping_add(px[j]);
            px[j] = a[j];
        }
    }
    let rem = it.into_remainder();
    let start = len - rem.len();
    for i in start..len { row[i] = row[i].wrapping_add(row[i - BPP]); }
}

fn avg_loop<const BPP: usize>(row: &mut [u8], prev: &[u8], len: usize) {
    let mut a = [0u8; BPP];
    for j in 0..BPP { a[j] = row[j].wrapping_add(prev[j] >> 1); row[j] = a[j]; }
    let it = row[BPP..len].chunks_exact_mut(BPP);
    let pit = prev[BPP..len].chunks_exact(BPP);
    let mut done = BPP;
    for (px, pp) in it.zip(pit) {
        for j in 0..BPP {
            a[j] = px[j].wrapping_add(((a[j] as u16 + pp[j] as u16) >> 1) as u8);
            px[j] = a[j];
        }
        done += BPP;
    }
    for i in done..len { row[i] = row[i].wrapping_add(((row[i - BPP] as u16 + prev[i] as u16) >> 1) as u8); }
}

fn avg0_loop<const BPP: usize>(row: &mut [u8], len: usize) {
    let mut a = [0u8; BPP];
    a.copy_from_slice(&row[..BPP]);
    let mut done = BPP;
    for px in row[BPP..len].chunks_exact_mut(BPP) {
        for j in 0..BPP { a[j] = px[j].wrapping_add(a[j] >> 1); px[j] = a[j]; }
        done += BPP;
    }
    for i in done..len { row[i] = row[i].wrapping_add(row[i - BPP] >> 1); }
}

fn paeth_loop<const BPP: usize, const BRANCHLESS: bool>(row: &mut [u8], prev: &[u8], len: usize) {
    let mut a = [0u8; BPP];
    let mut c = [0u8; BPP];
    for j in 0..BPP { c[j] = prev[j]; a[j] = row[j].wrapping_add(c[j]); row[j] = a[j]; }
    let it = row[BPP..len].chunks_exact_mut(BPP);
    let pit = prev[BPP..len].chunks_exact(BPP);
    let mut done = BPP;
    for (px, pp) in it.zip(pit) {
        for j in 0..BPP {
            let b = pp[j];
            let pred = if BRANCHLESS { paeth_pred_branchless(a[j] as i32, b as i32, c[j] as i32) }
                       else { paeth_pred(a[j] as i32, b as i32, c[j] as i32) };
            a[j] = px[j].wrapping_add(pred);
            px[j] = a[j];
            c[j] = b;
        }
        done += BPP;
    }
    for i in done..len {
        row[i] = row[i].wrapping_add(paeth_pred(row[i - BPP] as i32, prev[i] as i32, prev[i - BPP] as i32));
    }
}

macro_rules! by_bpp {
    ($bpp:expr, $f:ident, $($args:expr),*) => {
        match $bpp {
            1 => $f::<1>($($args),*), 2 => $f::<2>($($args),*), 3 => $f::<3>($($args),*),
            4 => $f::<4>($($args),*), 6 => $f::<6>($($args),*), 8 => $f::<8>($($args),*),
            _ => unreachable!(),
        }
    };
}

/// The textbook formulation (correct, slow: store-forwarding bound). Benchmarks only.
pub fn unfilter_row_naive(filter: u8, bpp: usize, row: &mut [u8], prev: Option<&[u8]>, len: usize) {
    match filter {
        1 => for i in bpp..len { row[i] = row[i].wrapping_add(row[i - bpp]); },
        2 => if let Some(prev) = prev { for i in 0..len { row[i] = row[i].wrapping_add(prev[i]); } },
        3 => match prev {
            Some(prev) => {
                for i in 0..bpp.min(len) { row[i] = row[i].wrapping_add(prev[i] >> 1); }
                for i in bpp..len { row[i] = row[i].wrapping_add(((row[i - bpp] as u16 + prev[i] as u16) >> 1) as u8); }
            }
            None => for i in bpp..len { row[i] = row[i].wrapping_add(row[i - bpp] >> 1); },
        },
        4 => match prev {
            Some(prev) => {
                for i in 0..bpp.min(len) { row[i] = row[i].wrapping_add(prev[i]); }
                for i in bpp..len {
                    row[i] = row[i].wrapping_add(paeth_pred(row[i - bpp] as i32, prev[i] as i32, prev[i - bpp] as i32));
                }
            }
            None => for i in bpp..len { row[i] = row[i].wrapping_add(row[i - bpp]); },
        },
        _ => {}
    }
}

/// The baseline. `row[..len]` is reconstructed in place; `prev` is the previous reconstructed
/// row (`None` on the first row, treated as zeros).
pub fn unfilter_row(filter: u8, bpp: usize, row: &mut [u8], prev: Option<&[u8]>, len: usize) {
    if len < bpp || !matches!(bpp, 1 | 2 | 3 | 4 | 6 | 8) {
        return unfilter_row_naive(filter, bpp, row, prev, len);
    }
    match (filter, prev) {
        (1, _) | (4, None) => by_bpp!(bpp, sub_loop, row, len),
        (2, Some(prev)) => for i in 0..len { row[i] = row[i].wrapping_add(prev[i]); },
        (3, Some(prev)) => by_bpp!(bpp, avg_loop, row, prev, len),
        (3, None) => by_bpp!(bpp, avg0_loop, row, len),
        (4, Some(prev)) => match bpp {
            1 => paeth_loop::<1, false>(row, prev, len), 2 => paeth_loop::<2, false>(row, prev, len),
            3 => paeth_loop::<3, false>(row, prev, len), 4 => paeth_loop::<4, false>(row, prev, len),
            6 => paeth_loop::<6, false>(row, prev, len), _ => paeth_loop::<8, false>(row, prev, len),
        },
        _ => {}
    }
}

/// Branchless scalar Paeth (benchmarks only): shows how much of the scalar cost is misprediction.
pub fn paeth_branchless(bpp: usize, row: &mut [u8], prev: Option<&[u8]>, len: usize) {
    match (prev, bpp) {
        (Some(prev), 1) if len >= 1 => paeth_loop::<1, true>(row, prev, len),
        (Some(prev), 2) if len >= 2 => paeth_loop::<2, true>(row, prev, len),
        (Some(prev), 3) if len >= 3 => paeth_loop::<3, true>(row, prev, len),
        (Some(prev), 4) if len >= 4 => paeth_loop::<4, true>(row, prev, len),
        (Some(prev), 6) if len >= 6 => paeth_loop::<6, true>(row, prev, len),
        (Some(prev), 8) if len >= 8 => paeth_loop::<8, true>(row, prev, len),
        _ => unfilter_row(4, bpp, row, prev, len),
    }
}

// ---- CRC-32 ------------------------------------------------------------------------------

struct CrcTables([[u32; 256]; 8]);

static CRC_TAB: CrcTables = {
    let mut t = [[0u32; 256]; 8];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut j = 0;
        while j < 8 { c = if c & 1 != 0 { (c >> 1) ^ 0xEDB8_8320 } else { c >> 1 }; j += 1; }
        t[0][i] = c;
        i += 1;
    }
    let mut i = 0;
    while i < 256 {
        let mut j = 1;
        while j < 8 { t[j][i] = (t[j - 1][i] >> 8) ^ t[0][(t[j - 1][i] & 0xFF) as usize]; j += 1; }
        i += 1;
    }
    CrcTables(t)
};

/// One table lookup per byte: what naive code does. Benchmarks only.
pub fn crc32_bytewise(crc: u32, p: &[u8]) -> u32 {
    let t = &CRC_TAB.0[0];
    let mut c = !crc;
    for &b in p { c = (c >> 8) ^ t[((c ^ b as u32) & 0xFF) as usize]; }
    !c
}

/// Slicing-by-8 table CRC (zlib's long-time implementation): the scalar baseline.
pub fn crc32(crc: u32, p: &[u8]) -> u32 {
    let t = &CRC_TAB.0;
    let mut c = !crc;
    let mut chunks = p.chunks_exact(8);
    for ch in &mut chunks {
        let lo = u32::from_le_bytes([ch[0], ch[1], ch[2], ch[3]]) ^ c;
        let hi = u32::from_le_bytes([ch[4], ch[5], ch[6], ch[7]]);
        // the four `hi` lookups depend only on the data: sum them off the crc chain first
        // (LLVM otherwise emits one serial xor chain through all eight, ~1.5x slower)
        let data_part = (t[3][(hi & 0xFF) as usize] ^ t[2][((hi >> 8) & 0xFF) as usize])
                      ^ (t[1][((hi >> 16) & 0xFF) as usize] ^ t[0][(hi >> 24) as usize]);
        let crc_part = (t[7][(lo & 0xFF) as usize] ^ t[6][((lo >> 8) & 0xFF) as usize])
                     ^ (t[5][((lo >> 16) & 0xFF) as usize] ^ t[4][(lo >> 24) as usize]);
        c = crc_part ^ data_part;
    }
    for &b in chunks.remainder() { c = (c >> 8) ^ t[0][((c ^ b as u32) & 0xFF) as usize]; }
    !c
}

// ---- Adler-32 ----------------------------------------------------------------------------

pub const ADLER_BASE: u32 = 65521;
/// largest n such that 255n(n+1)/2 + (n+1)(BASE-1) <= 2^32-1
pub const ADLER_NMAX: usize = 5552;

pub fn adler32(adler: u32, mut p: &[u8]) -> u32 {
    let (mut s1, mut s2) = (adler & 0xFFFF, adler >> 16);
    while !p.is_empty() {
        let k = p.len().min(ADLER_NMAX);
        let (blk, rest) = p.split_at(k);
        let mut ch = blk.chunks_exact(8);
        for c in &mut ch {
            s1 += c[0] as u32; s2 += s1; s1 += c[1] as u32; s2 += s1;
            s1 += c[2] as u32; s2 += s1; s1 += c[3] as u32; s2 += s1;
            s1 += c[4] as u32; s2 += s1; s1 += c[5] as u32; s2 += s1;
            s1 += c[6] as u32; s2 += s1; s1 += c[7] as u32; s2 += s1;
        }
        for &b in ch.remainder() { s1 += b as u32; s2 += s1; }
        s1 %= ADLER_BASE;
        s2 %= ADLER_BASE;
        p = rest;
    }
    (s2 << 16) | s1
}

// ---- pixel format conversions ------------------------------------------------------------

/// `src.len() == 3 * npix`, `dst.len() >= 4 * npix`
pub fn rgb_to_rgba(src: &[u8], dst: &mut [u8], alpha: u8) {
    let npix = src.len() / 3;
    for (s, d) in src.chunks_exact(3).zip(dst[..4 * npix].chunks_exact_mut(4)) {
        d[0] = s[0]; d[1] = s[1]; d[2] = s[2]; d[3] = alpha;
    }
}

/// Big-endian 16-bit samples -> 8-bit by keeping the high byte. `src.len() >= 2 * dst.len()`.
pub fn u16be_to_u8(src: &[u8], dst: &mut [u8]) {
    let n = dst.len();
    for (s, d) in src[..2 * n].chunks_exact(2).zip(dst.iter_mut()) { *d = s[0]; }
}

pub(crate) static KERNELS: super::Kernels = super::Kernels {
    name: "scalar",
    unfilter_row,
    crc32,
    adler32,
    rgb_to_rgba,
    u16be_to_u8,
};
