//! Property tests: every available backend against the scalar reference (and CRC-32/Adler-32
//! against crc32fast/adler). Random lengths, alignments, values and value distributions;
//! canaries detect any write past the declared length.

use pngsimd::kernels::{scalar, Kernels, ROW_SLACK};
use pngsimd::Backend;
use simdlab::testing::{for_each_backend, iterations};
use simdlab::Backends;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13; x ^= x >> 7; x ^= x << 17;
        self.0 = x;
        (x >> 32) as u32
    }
    fn below(&mut self, n: u32) -> u32 { if n == 0 { 0 } else { self.next() % n } }
    fn len(&mut self) -> usize {
        match self.below(6) {
            0 => self.below(8) as usize,
            1 => self.below(40) as usize,
            2 => self.below(300) as usize,
            3 => self.below(5000) as usize,
            4 => 16 * self.below(200) as usize,
            _ => self.below(70000) as usize,
        }
    }
    /// value distributions: the filters' behaviour depends on them (Paeth branches!)
    fn fill(&mut self, p: &mut [u8], dist: u32) {
        match dist {
            0 => for b in p { *b = self.next() as u8; },                                   // uniform
            1 => for b in p { *b = (self.below(5) as i32 - 2) as u8; },                     // small signed deltas
            2 => for b in p { *b = if self.below(4) != 0 { 0 } else { self.next() as u8 }; }, // mostly zero
            3 => { let v = self.below(256) as u8; p.fill(v); }                                // constant
            4 => for (i, b) in p.iter_mut().enumerate() { *b = (i as u32 * 37 + (self.next() & 3)) as u8; }, // ramp
            _ => for b in p { *b = if self.next() & 1 != 0 { 255 } else { 0 }; },            // extremes
        }
    }
}

/// Iterations per kernel family (scaled by `SIMDLAB_TEST_SCALE`).
fn iters() -> usize { iterations(2000) }

fn test_filters(k: &Kernels, rng: &mut Rng, iters: usize) {
    const MAXLEN: usize = 70000;
    const PAD: usize = 64;
    let bpps = [1usize, 2, 3, 4, 6, 8];
    let mut buf_a = vec![0u8; MAXLEN + 2 * PAD + 64];
    let mut buf_b = vec![0u8; MAXLEN + 2 * PAD + 64];
    let mut buf_p = vec![0u8; MAXLEN + 2 * PAD + 64];
    for _ in 0..iters {
        let filter = rng.below(5) as u8;
        let bpp = bpps[rng.below(6) as usize];
        let dist = rng.below(6);
        let len = rng.len();
        let (off, poff) = (rng.below(16) as usize, rng.below(16) as usize);
        let have_prev = rng.below(8) != 0;
        let pdist = rng.below(6);
        rng.fill(&mut buf_a[PAD + off..PAD + off + len + ROW_SLACK], dist);
        rng.fill(&mut buf_p[PAD + poff..PAD + poff + len + ROW_SLACK], pdist);
        // canaries: the slack after len must be readable but never written
        for i in 0..ROW_SLACK { buf_a[PAD + off + len + i] = 0xA5 ^ i as u8; }
        buf_b[PAD + off..PAD + off + len + ROW_SLACK].copy_from_slice(&buf_a[PAD + off..PAD + off + len + ROW_SLACK]);
        let prev = if have_prev { Some(&buf_p[PAD + poff..PAD + poff + len]) } else { None };
        scalar::unfilter_row(filter, bpp, &mut buf_a[PAD + off..PAD + off + len + ROW_SLACK], prev, len);
        (k.unfilter_row)(filter, bpp, &mut buf_b[PAD + off..PAD + off + len + ROW_SLACK], prev, len);
        let (ra, rb) = (&buf_a[PAD + off..PAD + off + len], &buf_b[PAD + off..PAD + off + len]);
        if ra != rb {
            let j = ra.iter().zip(rb).position(|(x, y)| x != y).unwrap();
            panic!("{} unfilter filter={filter} bpp={bpp} len={len} off={off} prev={have_prev} dist={dist}: first diff at {j} ({:02x} vs {:02x})", k.name, ra[j], rb[j]);
        }
        for i in 0..ROW_SLACK {
            assert_eq!(buf_b[PAD + off + len + i], 0xA5 ^ i as u8, "{} unfilter filter={filter} bpp={bpp} len={len} wrote past end (+{i})", k.name);
        }
    }
}

fn test_checksums(k: &Kernels, rng: &mut Rng, iters: usize) {
    const MAXLEN: usize = 70000;
    let mut buf = vec![0u8; MAXLEN + 64];
    for _ in 0..iters {
        let len = rng.len();
        let off = rng.below(16) as usize;
        let dist = rng.below(6);
        rng.fill(&mut buf[off..off + len], dist);
        let p = &buf[off..off + len];
        let init = rng.next();
        let (a, b) = ((k.crc32)(init, p), scalar::crc32(init, p));
        assert_eq!(a, b, "{} crc32 len={len} off={off} init={init:08x}", k.name);
        let mut h = crc32fast::Hasher::new_with_initial(init);
        h.update(p);
        assert_eq!(b, h.finalize(), "scalar crc32 len={len} vs crc32fast");
        let init = (rng.below(65521) << 16) | rng.below(65521);
        let (a, b) = ((k.adler32)(init, p), scalar::adler32(init, p));
        assert_eq!(a, b, "{} adler32 len={len} off={off} init={init:08x}", k.name);
        let mut ad = adler::Adler32::from_checksum(init);
        ad.write_slice(p);
        assert_eq!(b, ad.checksum(), "scalar adler32 len={len} vs adler crate");
    }
    // incremental == one-shot
    for _ in 0..iters / 10 {
        let len = rng.len();
        let cut = rng.below(len as u32 + 1) as usize;
        rng.fill(&mut buf[..len], 0);
        let p = &buf[..len];
        assert_eq!((k.crc32)(0, p), (k.crc32)((k.crc32)(0, &p[..cut]), &p[cut..]), "{} crc32 incremental len={len} cut={cut}", k.name);
        assert_eq!((k.adler32)(1, p), (k.adler32)((k.adler32)(1, &p[..cut]), &p[cut..]), "{} adler32 incremental len={len} cut={cut}", k.name);
    }
}

fn test_convert(k: &Kernels, rng: &mut Rng, iters: usize) {
    const MAXPIX: usize = 20000;
    let mut src = vec![0u8; MAXPIX * 4 + 64];
    let mut d1 = vec![0u8; MAXPIX * 4 + 64];
    let mut d2 = vec![0u8; MAXPIX * 4 + 64];
    for _ in 0..iters {
        let n = if rng.below(2) != 0 { rng.below(64) } else { rng.below(MAXPIX as u32) } as usize;
        let (off, doff) = (rng.below(16) as usize, rng.below(16) as usize);
        let alpha = rng.next() as u8;
        rng.fill(&mut src[off..off + 3 * n], 0);
        d1.fill(0x5A); d2.fill(0x5A);
        scalar::rgb_to_rgba(&src[off..off + 3 * n], &mut d1[doff..], alpha);
        (k.rgb_to_rgba)(&src[off..off + 3 * n], &mut d2[doff..], alpha);
        assert!(d1[doff..doff + 4 * n] == d2[doff..doff + 4 * n], "{} rgb_to_rgba npix={n} off={off} doff={doff}", k.name);
        assert!(d2[doff + 4 * n..doff + 4 * n + 32].iter().all(|&b| b == 0x5A), "{} rgb_to_rgba wrote past end npix={n}", k.name);

        rng.fill(&mut src[off..off + 2 * n], 0);
        d1.fill(0x5A); d2.fill(0x5A);
        scalar::u16be_to_u8(&src[off..off + 2 * n], &mut d1[doff..doff + n]);
        (k.u16be_to_u8)(&src[off..off + 2 * n], &mut d2[doff..doff + n]);
        assert!(d1[doff..doff + n] == d2[doff..doff + n], "{} u16be_to_u8 n={n} off={off} doff={doff}", k.name);
        assert!(d2[doff + n..doff + n + 32].iter().all(|&b| b == 0x5A), "{} u16be_to_u8 wrote past end n={n}", k.name);
    }
}

#[test]
fn all_backends_match_scalar() {
    let iters = iters();
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for_each_backend(|b: Backend| {
        let k = b.kernels();
        test_filters(k, &mut rng, iters);
        test_checksums(k, &mut rng, iters);
        test_convert(k, &mut rng, iters);
        eprintln!("{:<9} ok ({iters} iterations per kernel family)", k.name);
    });
}

#[test]
fn naive_and_branchless_scalar_variants_match() {
    let mut rng = Rng(0x1234_5678_9ABC_DEF1);
    let mut a = vec![0u8; 5000 + ROW_SLACK];
    let mut b = vec![0u8; 5000 + ROW_SLACK];
    let mut p = vec![0u8; 5000];
    for _ in 0..3000 {
        let len = rng.below(5000) as usize;
        let bpp = [1usize, 2, 3, 4, 6, 8][rng.below(6) as usize];
        let filter = rng.below(5) as u8;
        let (d1, d2) = (rng.below(6), rng.below(6));
        rng.fill(&mut a[..len], d1); rng.fill(&mut p[..len], d2);
        b[..len].copy_from_slice(&a[..len]);
        let prev = if rng.below(4) != 0 { Some(&p[..len]) } else { None };
        scalar::unfilter_row(filter, bpp, &mut a, prev, len);
        scalar::unfilter_row_naive(filter, bpp, &mut b, prev, len);
        assert!(a[..len] == b[..len], "naive vs register-carried filter={filter} bpp={bpp} len={len}");
        if filter == 4 {
            let mut c = vec![0u8; 5000 + ROW_SLACK];
            let d3 = rng.below(6);
            rng.fill(&mut c[..len], d3);
            b[..len].copy_from_slice(&c[..len]);
            scalar::unfilter_row(4, bpp, &mut c, prev, len);
            scalar::paeth_branchless(bpp, &mut b, prev, len);
            assert!(b[..len] == c[..len], "branchless paeth bpp={bpp} len={len}");
        }
    }
    let mut buf = vec![0u8; 10000];
    rng.fill(&mut buf, 0);
    assert_eq!(scalar::crc32_bytewise(0x1234, &buf), scalar::crc32(0x1234, &buf));
}

#[test]
fn contract_violations_panic() {
    let k = Backend::best().kernels();
    let mut row = vec![0u8; 100];
    assert!(std::panic::catch_unwind(move || (k.unfilter_row)(1, 3, &mut row, None, 100)).is_err(), "missing slack must be rejected");
}

/// Adler-32 with every byte 0xFF, around the 5552-byte reduction blocks: the largest sums the
/// vector accumulators ever hold (random bytes stay far from the overflow bounds).
#[test]
fn adler32_worst_case_bytes() {
    let ones = vec![0xFFu8; 4 * 5552 + 100];
    for_each_backend(|b: Backend| {
        let k = b.kernels();
        for len in [5551, 5552, 5553, 5552 + 32, 2 * 5552 - 1, 4 * 5552 + 100] {
            for init in [1, 0xFFF0_FFF0] {
                let p = &ones[..len];
                assert_eq!((k.adler32)(init, p), scalar::adler32(init, p), "{} adler32 0xFF len={len} init={init:08x}", k.name);
            }
        }
    });
}
