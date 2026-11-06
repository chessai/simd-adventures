//! Every supported backend against the reference, on random lengths and values.

use simdlab::rng::SplitMix64;
use simdlab::testing::{for_each_backend, iterations};
use simdlab::Backends;
use template::{sum_u32, Backend};

#[test]
fn backends_match_reference() {
    for_each_backend(|b: Backend| {
        let mut r = SplitMix64::new(1);
        for _ in 0..iterations(2000) {
            let len = (r.next_u64() % 300) as usize;
            let xs: Vec<u32> = (0..len).map(|_| r.next_u64() as u32).collect();
            assert_eq!(sum_u32(b, &xs), sum_u32(Backend::REFERENCE, &xs), "{} len={len}", b.name());
        }
        assert_eq!(sum_u32(b, &[u32::MAX; 9]), u32::MAX.wrapping_mul(9), "{} wrapping", b.name());
    });
}

/// On aarch64 (qemu in the flake checks) the NEON backend must be the one picked.
#[cfg(target_arch = "aarch64")]
#[test]
fn neon_is_best_on_aarch64() {
    assert_eq!(std::env::consts::ARCH, "aarch64");
    assert_eq!(Backend::best(), Backend::Neon);
}
