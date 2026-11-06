//! Runtime CPU feature detection by name.
//!
//! Backends declare the CPU features they need as data (`&["avx2", "pclmulqdq"]`), using the
//! names of `is_x86_feature_detected!` / `is_aarch64_feature_detected!`, which are also the
//! names `#[target_feature(enable = "...")]` takes.

macro_rules! feature_table {
    ($detect:ident: $($name:tt)*) => {
        /// Every feature name [`has`] knows on this architecture.
        pub const KNOWN: &[&str] = &[$($name),*];

        /// Whether this CPU supports `feature`.
        ///
        /// Panics on a name not in [`KNOWN`]: a typo in a backend's requirement list should fail
        /// loudly rather than make the backend silently unavailable.
        pub fn has(feature: &str) -> bool {
            match feature {
                $($name => std::arch::$detect!($name),)*
                _ => panic!("simdlab::cpu::has: unknown CPU feature {feature:?} (known: {})", KNOWN.join(" ")),
            }
        }
    };
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
feature_table!(is_x86_feature_detected:
    "sse2" "sse3" "ssse3" "sse4.1" "sse4.2" "popcnt" "pclmulqdq" "aes" "sha"
    "avx" "avx2" "fma" "f16c" "bmi1" "bmi2" "lzcnt" "movbe"
    "avx512f" "avx512cd" "avx512bw" "avx512dq" "avx512vl" "avx512ifma" "avx512vbmi" "avx512vbmi2"
    "avx512vnni" "avx512bitalg" "avx512vpopcntdq" "gfni" "vaes" "vpclmulqdq"
);

#[cfg(target_arch = "aarch64")]
feature_table!(is_aarch64_feature_detected:
    "neon" "aes" "pmull" "sha2" "sha3" "crc" "lse" "rdm" "dotprod" "fp16" "fhm" "i8mm" "bf16" "sve" "sve2"
);

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64")))]
pub const KNOWN: &[&str] = &[];
#[cfg(not(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64")))]
pub fn has(feature: &str) -> bool {
    panic!("simdlab::cpu::has: no feature detection on this architecture ({feature:?})")
}

pub fn has_all(features: &[&str]) -> bool {
    features.iter().all(|f| has(f))
}

/// The features in `features` this CPU lacks.
pub fn missing<'a>(features: &[&'a str]) -> Vec<&'a str> {
    features.iter().copied().filter(|f| !has(f)).collect()
}

/// The CPU model string, where the OS exposes one.
pub fn model_name() -> Option<String> {
    let info = std::fs::read_to_string("/proc/cpuinfo").ok()?;
    info.lines()
        .find(|l| l.starts_with("model name") || l.starts_with("Model"))
        .and_then(|l| l.split_once(':'))
        .map(|(_, v)| v.trim().to_string())
}
