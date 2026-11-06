//! Backends of this experiment and their tokens.
//!
//! The C++ selects the backend at compile time (one binary per ISA). Here every backend for the
//! target architecture is compiled, and the best one this CPU supports is picked at run time
//! (`--backend` overrides). Generic filter code runs inside the token's
//! [`Token::vectorize`](simdlab::Token::vectorize), so with `-C target-cpu=native` (the workspace
//! default) the per-key calls still inline into the caller's loop as in the C++.

use simdlab::{BackendInfo, Backends};

simdlab::target_token!(
    /// Portable backend: plain Rust on arrays, no target features. The scalar reference.
    pub struct Portable;
);
simdlab::target_token!(
    /// `std::simd` (nightly `portable_simd`), compiled for the build target's features.
    pub struct StdSimd;
);
#[cfg(target_arch = "x86_64")]
simdlab::target_token!(
    /// AVX2 (+BMI2, as the C++ `-mavx2 -mbmi2` build).
    pub struct Avx2: "avx2", "bmi2";
);
#[cfg(target_arch = "x86_64")]
simdlab::target_token!(
    /// AVX-512 F/BW/DQ/VL; the 32-bit-lane operations are AVX2's.
    pub struct Avx512: "avx2", "bmi2", "avx512f", "avx512bw", "avx512dq", "avx512vl";
);
#[cfg(target_arch = "aarch64")]
simdlab::target_token!(
    /// NEON (part of the AArch64 baseline).
    pub struct Neon: "neon";
);

/// Every backend, best first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Backend {
    #[cfg(target_arch = "x86_64")]
    Avx512,
    #[cfg(target_arch = "x86_64")]
    Avx2,
    #[cfg(target_arch = "aarch64")]
    Neon,
    StdSimd,
    Portable,
}

impl Backends for Backend {
    const REFERENCE: Self = Backend::Portable;

    fn all() -> &'static [Self] {
        &[
            #[cfg(target_arch = "x86_64")]
            Backend::Avx512,
            #[cfg(target_arch = "x86_64")]
            Backend::Avx2,
            #[cfg(target_arch = "aarch64")]
            Backend::Neon,
            Backend::StdSimd,
            Backend::Portable,
        ]
    }

    fn info(self) -> &'static BackendInfo {
        match self {
            #[cfg(target_arch = "x86_64")]
            Backend::Avx512 => &BackendInfo {
                name: "avx512",
                requires: Avx512::FEATURES,
                summary: "64-byte Bloom block in one zmm, masked-compare membership test",
            },
            #[cfg(target_arch = "x86_64")]
            Backend::Avx2 => &BackendInfo {
                name: "avx2",
                requires: Avx2::FEATURES,
                summary: "8-wide hashing, vptest membership, one 128-bit compare per cuckoo bucket pair",
            },
            #[cfg(target_arch = "aarch64")]
            Backend::Neon => &BackendInfo {
                name: "neon",
                requires: Neon::FEATURES,
                summary: "256-bit logical vectors as q-register pairs, bic+umaxv membership",
            },
            Backend::StdSimd => &BackendInfo {
                name: "stdsimd",
                requires: &[],
                summary: "std::simd u32x8 / u64x8 / u8x16, for the build target's features",
            },
            Backend::Portable => &BackendInfo {
                name: "portable",
                requires: &[],
                summary: "plain Rust on arrays",
            },
        }
    }
}

use simdlab::Token as _; // FEATURES above: the requirement lists are the tokens' target features
#[doc(hidden)]
pub use simdlab::require;

/// Calls `$f(token, $args...)` with the token for `$backend`, where `$f` is generic over the
/// [`Ops`](crate::simd::Ops) token type, so each backend gets its own monomorphised copy (dispatch
/// once, outside hot loops). Panics if this CPU does not support the backend.
#[macro_export]
macro_rules! with_backend {
    ($backend:expr, $($f:ident)::+ ( $($args:expr),* $(,)? )) => {{
        use $crate::backend::{self as bk, Backend as B};
        match $backend {
            #[cfg(target_arch = "x86_64")]
            B::Avx512 => $($f)::+($crate::backend::require::<bk::Avx512>(), $($args),*),
            #[cfg(target_arch = "x86_64")]
            B::Avx2 => $($f)::+($crate::backend::require::<bk::Avx2>(), $($args),*),
            #[cfg(target_arch = "aarch64")]
            B::Neon => $($f)::+($crate::backend::require::<bk::Neon>(), $($args),*),
            B::StdSimd => $($f)::+(bk::StdSimd, $($args),*),
            B::Portable => $($f)::+(bk::Portable, $($args),*),
        }
    }};
}
