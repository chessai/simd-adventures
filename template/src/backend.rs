//! Backends of this experiment.

use simdlab::{BackendInfo, Backends};
use simdlab::Token as _; // FEATURES below: the requirement lists are the tokens' target features

#[doc(hidden)]
pub use simdlab::require;

simdlab::target_token!(
    /// Plain Rust, no target features.
    pub struct Portable;
);
simdlab::target_token!(
    /// `std::simd` (nightly `portable_simd`), compiled for the build target's features (AVX2 with the
    /// workspace's `-C target-cpu=native` on this machine, NEON on AArch64).
    pub struct StdSimd;
);
#[cfg(target_arch = "x86_64")]
simdlab::target_token!(
    /// AVX2.
    pub struct Avx2: "avx2";
);
#[cfg(target_arch = "aarch64")]
simdlab::target_token!(
    /// NEON (part of the AArch64 baseline, so always detected).
    pub struct Neon: "neon";
);

/// Every backend, best first. Variants exist only on the architectures they compile for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Backend {
    #[cfg(target_arch = "x86_64")]
    Avx2,
    #[cfg(target_arch = "aarch64")]
    Neon,
    /// The generic kernel on `std::simd::u32x8`.
    StdSimd,
    /// The generic kernel on `[u32; 8]` arrays (LLVM may auto-vectorise it).
    Portable,
    /// A plain loop: the reference.
    Scalar,
}

impl Backends for Backend {
    const REFERENCE: Self = Backend::Scalar;

    fn all() -> &'static [Self] {
        &[
            #[cfg(target_arch = "x86_64")]
            Backend::Avx2,
            #[cfg(target_arch = "aarch64")]
            Backend::Neon,
            Backend::StdSimd,
            Backend::Portable,
            Backend::Scalar,
        ]
    }

    fn info(self) -> &'static BackendInfo {
        match self {
            #[cfg(target_arch = "x86_64")]
            Backend::Avx2 => &BackendInfo { name: "avx2", requires: Avx2::FEATURES, summary: "8 x u32 lanes per ymm" },
            #[cfg(target_arch = "aarch64")]
            Backend::Neon => &BackendInfo { name: "neon", requires: Neon::FEATURES, summary: "2 x 4 x u32 lanes per step" },
            Backend::StdSimd => &BackendInfo { name: "stdsimd", requires: &[], summary: "std::simd u32x8, for the build target's features" },
            Backend::Portable => &BackendInfo { name: "portable", requires: &[], summary: "generic kernel on [u32; 8] arrays" },
            Backend::Scalar => &BackendInfo { name: "scalar", requires: &[], summary: "plain loop" },
        }
    }
}

/// Calls `$f(token, $args...)` with the token for `$backend`, where `$f` is generic over the
/// token type, so each backend gets its own monomorphised copy. Dispatch once, outside hot loops.
#[macro_export]
macro_rules! with_backend {
    ($backend:expr, $($f:ident)::+ ( $($args:expr),* $(,)? )) => {{
        use $crate::backend::Backend as B;
        match $backend {
            #[cfg(target_arch = "x86_64")]
            B::Avx2 => $($f)::+($crate::backend::require::<$crate::backend::Avx2>(), $($args),*),
            #[cfg(target_arch = "aarch64")]
            B::Neon => $($f)::+($crate::backend::require::<$crate::backend::Neon>(), $($args),*),
            B::StdSimd => $($f)::+($crate::backend::StdSimd, $($args),*),
            B::Portable => $($f)::+($crate::backend::Portable, $($args),*),
            B::Scalar => $($f)::+($crate::kernels::Scalar, $($args),*),
        }
    }};
}
