//! Inflate round-trip and robustness tests against flate2.
//!  - round trip: flate2 deflate (all levels, varied data) -> pngsimd inflate == original
//!  - robustness: bit-flipped / truncated streams must return an error, never panic or overrun
//!    (the decoder is 100 % safe Rust, so an overrun would be a panic here)

use pngsimd::inflate::{inflate, zlib_inflate, Stats};
use simdlab::Backends;
use std::io::Write;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u32 { let mut x = self.0; x ^= x << 13; x ^= x >> 7; x ^= x << 17; self.0 = x; (x >> 32) as u32 }
    fn below(&mut self, n: u32) -> u32 { if n == 0 { 0 } else { self.next() % n } }
}

fn gen_data(rng: &mut Rng, n: usize, kind: u32) -> Vec<u8> {
    let mut p = vec![0u8; n];
    match kind {
        0 => for b in &mut p { *b = rng.next() as u8; },                           // incompressible
        1 => for b in &mut p { *b = (rng.next() % 4) as u8; },                     // low entropy
        2 => for (i, b) in p.iter_mut().enumerate() { *b = (i % 251) as u8; },      // long periodic matches
        3 => for i in 0..n { p[i] = if i % 7 == 0 { rng.next() as u8 } else { p[i - 1] }; }, // runs (dist 1)
        4 => { let t = b"the quick brown fox jumps over the lazy dog ";
               for (i, b) in p.iter_mut().enumerate() { *b = t[i % t.len()] + (rng.next() % 8 == 0) as u8; } }
        5 => for (i, b) in p.iter_mut().enumerate() {   // PNG-like: filtered rows with small deltas
                 *b = if i % 1025 == 0 { (rng.next() % 5) as u8 } else { ((rng.next() % 5) as i32 - 2) as u8 }; },
        _ => { let v = rng.next() as u8; p.fill(v); }
    }
    p
}

fn compress(data: &[u8], level: u32) -> Vec<u8> {
    let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::new(level));
    e.write_all(data).unwrap();
    e.finish().unwrap()
}

#[test]
fn roundtrip_all_levels_and_data_kinds() {
    let iters: usize = simdlab::testing::iterations(300);
    let mut rng = Rng(0x1234_5678_8765_4321);
    let k = pngsimd::Backend::best().kernels();
    for it in 0..iters {
        let n = if rng.below(4) == 0 { rng.below(64) } else if rng.below(2) != 0 { rng.below(4096) } else { rng.below(1 << 20) } as usize;
        let kind = rng.below(7);
        let level = rng.below(10);
        let src = gen_data(&mut rng, n, kind);
        let comp = compress(&src, level);
        let mut dec = vec![0u8; n];
        let (r, _) = zlib_inflate(&comp, &mut dec, None, Some(k.adler32)).unwrap_or_else(|e| panic!("it={it} n={n} kind={kind} level={level}: {e}"));
        assert_eq!(r, n);
        assert!(src == dec, "data mismatch it={it} n={n} kind={kind} level={level}");
        // the raw interface with an exact-size cap and a too-small cap
        let mut st = Stats::default();
        let (r, consumed) = inflate(&comp[2..], &mut dec, Some(&mut st)).unwrap();
        assert_eq!((r, consumed), (n, comp.len() - 6), "raw inflate n={n}");
        if st.blocks_stored == 0 { assert_eq!(st.literals + st.match_bytes, n as u64); }
        if n > 0 {
            let mut small = vec![0u8; n - 1];
            assert!(inflate(&comp[2..], &mut small, None).is_err(), "raw inflate with a small cap must fail (n={n})");
        }
    }
}

#[test]
fn mutated_and_truncated_streams_never_panic() {
    let iters: usize = simdlab::testing::iterations(1500);
    let mut rng = Rng(0xDEAD_BEEF_CAFE_F00D);
    let k = pngsimd::Backend::best().kernels();
    let (mut errors, mut oks) = (0, 0);
    for _ in 0..iters {
        let n = rng.below(1 << 16) as usize;
        let kind = rng.below(7);
        let src = gen_data(&mut rng, n, kind);
        let mut comp = compress(&src, rng.below(10));
        let nflip = 1 + rng.below(8);
        for _ in 0..nflip { if !comp.is_empty() { let i = rng.below(comp.len() as u32) as usize; comp[i] ^= 1 << rng.below(8); } }
        if rng.below(3) == 0 { let l = rng.below(comp.len() as u32 + 1) as usize; comp.truncate(l); }
        let mut dec = vec![0u8; n];
        match zlib_inflate(&comp, &mut dec, None, Some(k.adler32)) { Ok(_) => oks += 1, Err(_) => errors += 1 }
    }
    eprintln!("robustness: {iters} mutated streams, {errors} rejected, {oks} accepted (accepted only if the Adler-32 still matches)");
}

#[test]
fn handmade_streams() {
    // zlib.compress(b"aaaaaaaaaa", 9)
    let z = [0x78u8, 0xda, 0x4b, 0x4c, 0x84, 0x01, 0x00, 0x14, 0xe1, 0x03, 0xcb];
    // stored block "hello" + adler
    let st = [0x78u8, 0x01, 0x01, 0x05, 0x00, 0xfa, 0xff, b'h', b'e', b'l', b'l', b'o', 0x06, 0x2c, 0x02, 0x15];
    let k = pngsimd::Backend::best().kernels();
    let mut out = [0u8; 32];
    let (r, _) = zlib_inflate(&z, &mut out, None, Some(k.adler32)).unwrap();
    assert_eq!(&out[..r], b"aaaaaaaaaa");
    let (r, _) = zlib_inflate(&st, &mut out, None, Some(k.adler32)).unwrap();
    assert_eq!(&out[..r], b"hello");
    for i in 0..z.len() - 4 { assert!(zlib_inflate(&z[..i], &mut out, None, None).is_err(), "truncated at {i} accepted"); }
}
