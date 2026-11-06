//! Property tests (port of tests/test_props.cpp): every ISA backend must agree bit-for-bit with
//! the portable (scalar) backend, batched APIs must agree with per-key APIs, and the filters must
//! never produce false negatives against a reference model.
//!
//! The C++ counts failures and keeps going; these stop at the first one. Models use BTree
//! collections so runs are deterministic (the C++ iterates unordered containers).

use std::collections::{BTreeMap, BTreeSet};
use vecfilter::hash::{fastrange32, hash1, hash8_keys};
use simdlab::rng::SplitMix64 as Rng;
use simdlab::testing::for_each_backend;
use vecfilter::backend::Portable;
use vecfilter::cuckoo::Fingerprint;
use vecfilter::keygen::{make_keys, make_queries};
use vecfilter::simd::Ops;
use vecfilter::{with_backend, Backend, BlockedBloom, ClassicBloom, CuckooFilter};

fn interesting_keys() -> Vec<u64> {
    let mut k = vec![0, 1, 2, 3, 0xFFFFFFFF, 0x100000000, !0, 0x8000000000000000, 0x7FFFFFFFFFFFFFFF];
    let mut r = Rng::new(42);
    k.extend((0..1000).map(|_| r.next_u64()));
    k.extend(0..1000u64);
    k.extend((0..1000u64).map(|i| i << 32));
    k
}

fn test_hash_agreement<O: Ops>(o: O) {
    let mut keys = interesting_keys();
    while keys.len() % 8 != 0 {
        keys.push(keys.last().unwrap() + 1);
    }
    for chunk in keys.chunks_exact(8) {
        let (hb, hl) = hash8_keys(o, chunk.try_into().unwrap());
        let (b, l) = (o.to_array(hb), o.to_array(hl));
        for j in 0..8 {
            let (sb, sl) = hash1(chunk[j]);
            assert!(
                sb == b[j] && sl == l[j],
                "{} hash8 != hash1 for key {:x} (lane {j}): {:08x}/{:08x} vs {sb:08x}/{sl:08x}",
                O::NAME,
                chunk[j],
                b[j],
                l[j]
            );
        }
    }
    // mulhi_32 vs fastrange32
    let mut r = Rng::new(7);
    for _ in 0..200 {
        let a: [u32; 8] = std::array::from_fn(|_| r.next_u64() as u32);
        let n: [u32; 8] = std::array::from_fn(|_| (r.next_u64() % 50000000 + 1) as u32);
        let out = o.to_array(o.mulhi_32(o.from_array(&a), o.from_array(&n)));
        for j in 0..8 {
            assert_eq!(out[j], fastrange32(a[j], n[j]), "{} mulhi lane {j}", O::NAME);
        }
    }
}

const BATCH_SIZES: [usize; 9] = [0, 1, 7, 8, 9, 31, 33, 257, 1001];

fn test_blocked_bloom<O: Ops, const BB: usize, const BPL: usize>(o: O, bytes: usize, nops: usize, seed: u64, dist: &str) {
    let name = format!("{} BB{BB}/{BPL}", O::NAME);
    let mut f = BlockedBloom::<O, BB, BPL>::with_window(o, bytes, 8 + (seed % 60) as usize);
    let mut reference = BlockedBloom::<Portable, BB, BPL>::new(Portable, bytes);
    let mut fb = BlockedBloom::<O, BB, BPL>::with_window(o, bytes, 16); // filled via insert_batch
    let mut model = BTreeSet::new();
    let keys = make_keys(dist, nops, seed);
    let mut r = Rng::new(seed ^ 0xABC);

    // per-key inserts (f, reference) interleaved with queries
    for &k in &keys {
        if r.next_u64() & 3 != 0 {
            f.insert(k);
            reference.insert(k);
            model.insert(k);
        } else {
            let q = if r.next_u64() & 1 != 0 { k } else { r.next_u64() };
            let (a, b) = (f.contains(q), reference.contains(q));
            assert_eq!(a, b, "{name} contains mismatch vs portable");
            if model.contains(&q) {
                assert!(a, "{name} false negative");
            }
        }
    }
    assert!(f.data() == reference.data(), "{name} bit array differs from portable");

    // batched insert of the same model set must give the identical bit array
    let mk: Vec<u64> = model.iter().copied().collect();
    fb.insert_batch(&mk);
    assert!(fb.data() == reference.data(), "{name} insert_batch array differs");

    // batched contains == per-key contains, for odd sizes / windows
    for n in BATCH_SIZES {
        let q: Vec<u64> = (0..n)
            .map(|_| if r.next_u64() & 1 != 0 && !mk.is_empty() { mk[(r.next_u64() % mk.len() as u64) as usize] } else { r.next_u64() })
            .collect();
        let (mut out, mut out_ref) = (vec![0xEEu8; n + 1], vec![0xEEu8; n + 1]);
        f.contains_batch(&q, &mut out[..n]);
        reference.contains_batch(&q, &mut out_ref[..n]);
        for i in 0..n {
            assert_eq!(out[i], f.contains(q[i]) as u8, "{name} contains_batch != contains (n={n} i={i})");
            assert_eq!(out[i], out_ref[i], "{name} contains_batch != portable batch");
        }
        assert_eq!(out[n], 0xEE, "contains_batch wrote past the end");
        assert_eq!(f.count_batch(&q), out[..n].iter().filter(|&&x| x == 1).count(), "count_batch mismatch");
    }
}

fn test_cuckoo<O: Ops, FP: Fingerprint>(o: O, bytes: usize, nops: usize, seed: u64, dist: &str) {
    let name = format!("{} cuckoo{}", O::NAME, FP::BITS);
    let mut c = CuckooFilter::<O, FP>::with_window(o, bytes, 8 + (seed % 60) as usize);
    let mut reference = CuckooFilter::<Portable, FP>::new(Portable, bytes);
    let mut model: BTreeMap<u64, u32> = BTreeMap::new(); // multiset
    let keys = make_keys(dist, nops, seed);
    let mut r = Rng::new(seed ^ 0xDEF);
    for &k in &keys {
        let op = r.next_u64() % 10;
        if op < 6 {
            let (a, b) = (c.insert(k), reference.insert(k));
            assert_eq!(a, b, "{name} insert result differs from portable");
            if a {
                *model.entry(k).or_default() += 1;
            }
        } else if op < 8 {
            let q = if r.next_u64() & 1 != 0 { k } else { r.next_u64() };
            let (a, b) = (c.contains(q), reference.contains(q));
            assert_eq!(a, b, "{name} contains differs from portable");
            if model.contains_key(&q) {
                assert!(a, "{name} false negative");
            }
        } else if !model.is_empty() {
            // erase only keys that are in the model (deleting a never-inserted key is UB for cuckoo filters)
            let skip = (r.next_u64() % model.len().min(16) as u64) as usize;
            let q = *model.keys().nth(skip).unwrap();
            let (a, b) = (c.erase(q), reference.erase(q));
            assert_eq!(a, b, "{name} erase differs from portable");
            assert!(a, "{name} erase of present key failed");
            let cnt = model.get_mut(&q).unwrap();
            *cnt -= 1;
            if *cnt == 0 {
                model.remove(&q);
            } else {
                assert!(c.contains(q), "{name} remaining copy lost after erase");
            }
        }
    }
    assert_eq!(c.size(), reference.size(), "cuckoo size mismatch");
    // Batched == per-key
    let mk: Vec<u64> = model.keys().copied().collect();
    for n in BATCH_SIZES {
        let q: Vec<u64> = (0..n)
            .map(|_| if r.next_u64() & 1 != 0 && !mk.is_empty() { mk[(r.next_u64() % mk.len() as u64) as usize] } else { r.next_u64() })
            .collect();
        let (mut out, mut out_ref) = (vec![0xEEu8; n + 1], vec![0xEEu8; n + 1]);
        c.contains_batch(&q, &mut out[..n]);
        reference.contains_batch(&q, &mut out_ref[..n]);
        for i in 0..n {
            assert_eq!(out[i], c.contains(q[i]) as u8, "{name} contains_batch != contains");
            assert_eq!(out[i], out_ref[i], "{name} contains_batch != portable batch");
            if model.contains_key(&q[i]) {
                assert_eq!(out[i], 1, "{name} batched false negative");
            }
        }
        assert_eq!(out[n], 0xEE, "contains_batch wrote past the end");
    }
    // insert_batch must give the same table as per-key inserts (same deterministic kick RNG)
    let mut c2 = CuckooFilter::<O, FP>::new(o, bytes);
    let mut c3 = CuckooFilter::<O, FP>::new(o, bytes);
    let fail2 = c2.insert_batch(&mk);
    let fail3 = mk.iter().filter(|&&k| !c3.insert(k)).count();
    assert_eq!(fail2, fail3, "{name} insert_batch failure count differs");
    for &k in &mk {
        assert!(c2.contains(k), "{name} insert_batch false negative");
    }
}

fn test_fpr_sanity<O: Ops>(o: O) {
    // ~10 bits/key, 1M keys: blocked32 should be ~1-2%, cuckoo16 ~0.01%, cuckoo8 ~3%
    let n = ((1 << 20) as f64 * 0.95) as usize; // 95% of a 2^20-slot cuckoo table
    let keys = make_keys("uniform", n, 5);
    let mut b32 = BlockedBloom::<O, 32>::new(o, n * 10 / 8);
    let mut b64 = BlockedBloom::<O, 64>::new(o, n * 10 / 8);
    b32.insert_batch(&keys);
    b64.insert_batch(&keys);
    let mut c16 = CuckooFilter::<O, u16>::new(o, 2 << 20); // 2^18 buckets x 4 slots
    let mut c8 = CuckooFilter::<O, u8>::new(o, 1 << 20);
    let (f16, f8) = (c16.insert_batch(&keys), c8.insert_batch(&keys));
    assert_eq!(f16, 0, "cuckoo16 rejected {f16} inserts at load {:.3}", c16.load());
    assert_eq!(f8, 0, "cuckoo8 rejected {f8} inserts at load {:.3}", c8.load());
    let q = make_queries("absent", &keys, n, 77);
    let r32 = b32.count_batch(&q) as f64 / n as f64;
    let r64 = b64.count_batch(&q) as f64 / n as f64;
    let r16 = c16.count_batch(&q) as f64 / n as f64;
    let r8 = c8.count_batch(&q) as f64 / n as f64;
    println!(
        "  [{}] FPR @ ~10 bpk: blocked32={r32:.4} blocked64={r64:.4}  cuckoo16(load {:.2})={r16:.5} cuckoo8(load {:.2})={r8:.4}",
        O::NAME,
        c16.load(),
        c8.load()
    );
    assert!(r32 > 0.002 && r32 < 0.03, "blocked32 FPR out of range {r32:.4}");
    assert!(r64 > 0.002 && r64 < 0.03, "blocked64 FPR out of range {r64:.4}");
    assert!(r16 < 0.001, "cuckoo16 FPR out of range {r16:.5}");
    assert!(r8 > 0.01 && r8 < 0.06, "cuckoo8 FPR out of range {r8:.4}");
    for &k in keys.iter().step_by(97) {
        assert!(b32.contains(k) && b64.contains(k) && c16.contains(k) && c8.contains(k), "false negative in fpr test");
    }
}

fn run_backend<O: Ops>(o: O) {
    test_hash_agreement(o);
    let mut seed = 1;
    for dist in ["uniform", "sequential", "clustered"] {
        for bytes in [32, 64, 100, 4096, 1 << 16] {
            let nops = if bytes < 128 { 200 } else if bytes <= 4096 { 3000 } else { 60000 };
            test_blocked_bloom::<O, 32, 1>(o, bytes, nops, seed, dist);
            test_blocked_bloom::<O, 64, 1>(o, bytes, nops, seed + 1, dist);
            test_blocked_bloom::<O, 32, 2>(o, bytes, nops, seed + 2, dist);
            test_blocked_bloom::<O, 64, 2>(o, bytes, nops, seed + 3, dist);
            test_cuckoo::<O, u16>(o, bytes, nops, seed + 4, dist);
            test_cuckoo::<O, u8>(o, bytes, nops, seed + 5, dist);
            seed += 6;
        }
    }
    test_fpr_sanity(o);
}

#[test]
fn all_backends() {
    for_each_backend(|b: Backend| with_backend!(b, run_backend()));
}

#[test]
fn classic_bloom() {
    let mut b = ClassicBloom::new(1 << 16, 7);
    let mut model = BTreeSet::new();
    let mut r = Rng::new(99);
    for _ in 0..20000 {
        let k = r.next_u64();
        b.insert(k);
        model.insert(k);
    }
    for &k in &model {
        assert!(b.contains(k), "classic bloom false negative");
    }
    let fp = (0..100000).filter(|_| b.contains(r.next_u64())).count();
    assert!(fp < 5000, "classic bloom FPR implausibly high: {fp}/100000");
}
