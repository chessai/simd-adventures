//! Helpers for the experiments' tests.

use crate::Backends;

/// Environment variable scaling every test's iteration count (a float; default 1).
pub const SCALE_ENV: &str = "SIMDLAB_TEST_SCALE";

/// `default` scaled by `SIMDLAB_TEST_SCALE` (e.g. `0.1` for a quick run, `10` for a soak), at least 1.
pub fn iterations(default: usize) -> usize {
    let scale = std::env::var(SCALE_ENV).ok().and_then(|s| s.parse::<f64>().ok()).unwrap_or(1.0);
    ((default as f64 * scale) as usize).max(1)
}

/// Runs `f` for every backend this CPU supports, then reports which backends were tested and
/// which were skipped (shown with `cargo test -- --nocapture`).
pub fn for_each_backend<B: Backends>(mut f: impl FnMut(B)) {
    let mut tested = Vec::new();
    let mut skipped = Vec::new();
    for &b in B::all() {
        if b.is_supported() {
            f(b);
            tested.push(b.name());
        } else {
            skipped.push(format!("{} (missing {})", b.name(), b.missing_features().join(" ")));
        }
    }
    let skipped = if skipped.is_empty() { "none".to_string() } else { skipped.join(", ") };
    eprintln!("backends tested: {}; not supported by this CPU: {skipped}", tested.join(", "));
}
