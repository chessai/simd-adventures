//! Key distributions for benchmarks and tests.

use simdlab::rng::SplitMix64;

/// Distributions over the *inserted* set:
///   * `uniform`    : random 64-bit keys
///   * `sequential` : base + i (low entropy, stresses the hash)
///   * `clustered`  : runs of 64 consecutive keys at random bases (e.g. ids per tenant)
///
/// Anything else is treated as `uniform`.
pub fn make_keys(dist: &str, n: usize, seed: u64) -> Vec<u64> {
    let mut r = SplitMix64::new(seed);
    match dist {
        "sequential" => {
            let base = r.next_u64();
            (0..n as u64).map(|i| base.wrapping_add(i)).collect()
        }
        "clustered" => {
            let mut base = 0;
            (0..n as u64)
                .map(|i| {
                    if i % 64 == 0 {
                        base = r.next_u64() & !63;
                    }
                    base + i % 64
                })
                .collect()
        }
        _ => (0..n).map(|_| r.next_u64()).collect(),
    }
}

/// Query streams:
///   * `mixed`  : 50% keys drawn uniformly from `present`, 50% fresh random keys (absent w.h.p.)
///   * `absent` : 100% fresh random keys (the common "negative lookup" case)
///   * `zipf`   : queries drawn from `present` with a Zipf(1) popularity skew -- hot blocks stay cached
///
/// Anything else is treated as `mixed`.
pub fn make_queries(qdist: &str, present: &[u64], nq: usize, seed: u64) -> Vec<u64> {
    let mut r = SplitMix64::new(seed);
    let np = present.len();
    match qdist {
        "absent" => (0..nq).map(|_| r.next_u64()).collect(),
        "zipf" => {
            let ln_n = (np as f64).ln();
            (0..nq)
                .map(|_| {
                    let u = r.unit();
                    let rank = ((u * ln_n).exp() as usize).min(np - 1); // P(rank <= x) = ln x / ln N  => density ~ 1/x
                    present[rank]
                })
                .collect()
        }
        _ => (0..nq)
            .map(|_| if r.next_u64() & 1 != 0 { present[(r.next_u64() % np as u64) as usize] } else { r.next_u64() })
            .collect(),
    }
}
