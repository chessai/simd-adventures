//! Random-program driver (port of tests/fuzz_ops.cpp with VF_FUZZ_MAIN): interprets random bytes
//! as a program of insert/erase/query operations over a *tiny* key space (so fingerprint and
//! block collisions are frequent), runs it against every compiled-in backend plus the portable
//! reference and a model, and panics on any divergence or false negative.
//!
//! 5000 programs per backend (as in the C++ ctest run), scaled by `SIMDLAB_TEST_SCALE`.
//! The libFuzzer harness is not ported (it would be a `cargo fuzz` target calling `run_all`).

use std::collections::{BTreeMap, BTreeSet};
use simdlab::rng::SplitMix64;
use simdlab::testing::{for_each_backend, iterations};
use vecfilter::backend::Portable;
use vecfilter::cuckoo::Fingerprint;
use vecfilter::simd::Ops;
use vecfilter::{with_backend, Backend, BlockedBloom, CuckooFilter};

struct Reader<'a> {
    p: &'a [u8],
    i: usize,
}

impl Reader<'_> {
    fn done(&self) -> bool {
        self.i >= self.p.len()
    }
    fn byte(&mut self) -> u8 {
        let b = self.p.get(self.i).copied().unwrap_or(0);
        self.i += 1;
        b
    }
    /// small key space, two "kinds" to mix small ints and wide values
    fn key(&mut self) -> u64 {
        let b = self.byte();
        if b & 0x80 != 0 { ((b as u64 & 0x7F) << 32) | self.byte() as u64 } else { b as u64 }
    }
}

fn run_bloom<O: Ops>(o: O, data: &[u8]) {
    let mut rd = Reader { p: data, i: 0 };
    let bytes = 32 + (rd.byte() % 8) as usize * 32;
    let window = 1 + (rd.byte() % 40) as usize;
    let mut a = BlockedBloom::<O, 32>::with_window(o, bytes, window);
    let mut ra = BlockedBloom::<Portable, 32>::new(Portable, bytes);
    let mut b = BlockedBloom::<O, 64, 2>::with_window(o, bytes, window);
    let mut rb = BlockedBloom::<Portable, 64, 2>::new(Portable, bytes);
    let mut model = BTreeSet::new();
    let mut pending = Vec::new();
    while !rd.done() {
        let op = rd.byte() % 4;
        let k = rd.key();
        match op {
            0 => {
                a.insert(k);
                ra.insert(k);
                b.insert(k);
                rb.insert(k);
                model.insert(k);
            }
            1 => pending.push(k),
            2 => {
                a.insert_batch(&pending);
                ra.insert_batch(&pending);
                b.insert_batch(&pending);
                rb.insert_batch(&pending);
                model.extend(pending.drain(..));
            }
            _ => {
                let (x, y, z, w) = (a.contains(k), ra.contains(k), b.contains(k), rb.contains(k));
                assert!(x == y && z == w, "{} bloom backend divergence", O::NAME);
                if model.contains(&k) {
                    assert!(x && z, "{} bloom false negative", O::NAME);
                }
            }
        }
    }
    assert!(a.data() == ra.data(), "{} bloom32 array divergence", O::NAME);
    assert!(b.data() == rb.data(), "{} bloom64 array divergence", O::NAME);
    let mut q: Vec<u64> = model.iter().copied().collect();
    q.extend((0..50).map(|i| i * 7919));
    let (mut o1, mut o2) = (vec![0u8; q.len()], vec![0u8; q.len()]);
    a.contains_batch(&q, &mut o1);
    ra.contains_batch(&q, &mut o2);
    for i in 0..q.len() {
        assert!(o1[i] == o2[i] && o1[i] == a.contains(q[i]) as u8, "{} bloom batch divergence", O::NAME);
        if model.contains(&q[i]) {
            assert_eq!(o1[i], 1, "{} bloom batch false negative", O::NAME);
        }
    }
}

fn run_cuckoo<O: Ops, FP: Fingerprint>(o: O, data: &[u8]) {
    let mut rd = Reader { p: data, i: 0 };
    let bytes = 16 + (rd.byte() % 8) as usize * 16;
    let window = 1 + (rd.byte() % 40) as usize;
    let mut c = CuckooFilter::<O, FP>::with_window(o, bytes, window);
    let mut r = CuckooFilter::<Portable, FP>::new(Portable, bytes);
    let mut model: BTreeMap<u64, u32> = BTreeMap::new();
    let mut live = Vec::new();
    while !rd.done() {
        let op = rd.byte() % 4;
        let k = rd.key();
        match op {
            0 => {
                let (x, y) = (c.insert(k), r.insert(k));
                assert_eq!(x, y, "{} cuckoo insert divergence", O::NAME);
                if x {
                    *model.entry(k).or_default() += 1;
                    live.push(k);
                }
            }
            1 => {
                if !live.is_empty() {
                    let j = (k % live.len() as u64) as usize;
                    let e = live.swap_remove(j);
                    let (x, y) = (c.erase(e), r.erase(e));
                    assert_eq!(x, y, "{} cuckoo erase divergence", O::NAME);
                    assert!(x, "{} cuckoo erase of present key failed", O::NAME);
                    let cnt = model.get_mut(&e).unwrap();
                    *cnt -= 1;
                    if *cnt == 0 {
                        model.remove(&e);
                    }
                }
            }
            _ => {
                let (x, y) = (c.contains(k), r.contains(k));
                assert_eq!(x, y, "{} cuckoo contains divergence", O::NAME);
                if model.contains_key(&k) {
                    assert!(x, "{} cuckoo false negative", O::NAME);
                }
            }
        }
    }
    let mut q = live.clone();
    q.extend((0..50).map(|i| i * 104729));
    let (mut o1, mut o2) = (vec![0u8; q.len()], vec![0u8; q.len()]);
    c.contains_batch(&q, &mut o1);
    r.contains_batch(&q, &mut o2);
    for i in 0..q.len() {
        assert!(o1[i] == o2[i] && o1[i] == c.contains(q[i]) as u8, "{} cuckoo batch divergence", O::NAME);
        if model.contains_key(&q[i]) {
            assert_eq!(o1[i], 1, "{} cuckoo batch false negative", O::NAME);
        }
    }
}

fn run_all<O: Ops>(o: O, data: &[u8]) {
    run_bloom(o, data);
    run_cuckoo::<O, u16>(o, data);
    run_cuckoo::<O, u8>(o, data);
}

fn drive<O: Ops>(o: O) {
    let iters = iterations(5000);
    let mut rng = SplitMix64::new(0xF00D);
    let mut buf = Vec::new();
    for _ in 0..iters {
        let len = (rng.next_u64() % 2048) as usize;
        buf.clear();
        buf.extend((0..len).map(|_| rng.next_u64() as u8));
        // bias towards tiny key spaces for some runs so collisions are dense
        if rng.next_u64() & 1 != 0 {
            buf.iter_mut().for_each(|b| *b &= 0x0F);
        }
        run_all(o, &buf);
    }
    println!("fuzz driver: {iters} random programs on backend {}, no divergence", O::NAME);
}

#[test]
fn fuzz_driver() {
    for_each_backend(|b: Backend| with_backend!(b, drive()));
}
