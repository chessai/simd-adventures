//! Backends of this experiment.
//!
//! The tokens double as the [`Simd`](crate::simd::Simd) implementations: the generic kernels are
//! instantiated with the token type, and [`define_dispatch!`](crate::define_dispatch) obtains the
//! token (the proof that the CPU supports it) before running them inside its `vectorize`. Some
//! backends replace a generic kernel with one written for their ISA ([`crate::native`]).

use simdlab::{BackendInfo, Backends};
use simdlab::Token as _; // FEATURES below: the requirement lists are the tokens' target features

#[doc(hidden)]
pub use simdlab::require;

simdlab::target_token!(
    /// The generic kernels on `[u8; 16]` arrays: the executable specification of the
    /// [`Simd`](crate::simd::Simd) semantics.
    pub struct Portable;
);
simdlab::target_token!(
    /// `std::simd` (nightly `portable_simd`) on `u8x16`, compiled for the build target's features
    /// (AVX2 with the workspace's `-C target-cpu=native` on this machine, NEON on AArch64).
    pub struct StdSimd;
);
#[cfg(target_arch = "x86_64")]
simdlab::target_token!(
    /// SSSE3 + SSE4.1, 128-bit.
    pub struct Sse41: "ssse3", "sse4.1";
);
#[cfg(target_arch = "x86_64")]
simdlab::target_token!(
    /// AVX2, 256-bit as two 128-bit lanes.
    pub struct Avx2: "avx2";
);
#[cfg(target_arch = "x86_64")]
simdlab::target_token!(
    /// AVX-512 with VBMI/VBMI2 (byte permutes, compress) and BMI2 (`pdep`): the percent codec of
    /// [`crate::native::vbmi2`]; hex and base64 run the AVX2 kernels.
    pub struct Avx512: "avx2", "bmi2", "avx512f", "avx512bw", "avx512vbmi", "avx512vbmi2";
);
#[cfg(target_arch = "aarch64")]
simdlab::target_token!(
    /// NEON, 128-bit (part of the AArch64 baseline, so always detected).
    pub struct Neon: "neon";
);

/// Every backend, best first. Variants exist only on the architectures they compile for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Backend {
    #[cfg(target_arch = "x86_64")]
    Avx512,
    #[cfg(target_arch = "x86_64")]
    Avx2,
    #[cfg(target_arch = "x86_64")]
    Sse41,
    /// NEON-native hex and base64 kernels ([`crate::native::neon`]), the generic percent codec.
    #[cfg(target_arch = "aarch64")]
    Neon,
    /// Every codec through the generic kernels on [`Neon`] (NEON emulating the x86-shaped trait):
    /// the baseline the NEON-native kernels are measured against.
    #[cfg(target_arch = "aarch64")]
    NeonGeneric,
    /// The generic kernels on [`StdSimd`].
    StdSimd,
    /// Plain scalar loops, not generic over `Simd`: the reference, and the fallback without SIMD
    /// (1.5-3x faster than the generic kernels on `[u8; 16]`, see REPORT.md).
    Scalar,
    /// The generic kernels instantiated with [`Portable`]: a correctness backend.
    Portable,
}

impl Backends for Backend {
    const REFERENCE: Self = Backend::Scalar;

    fn all() -> &'static [Self] {
        &[
            #[cfg(target_arch = "x86_64")]
            Backend::Avx512,
            #[cfg(target_arch = "x86_64")]
            Backend::Avx2,
            #[cfg(target_arch = "x86_64")]
            Backend::Sse41,
            #[cfg(target_arch = "aarch64")]
            Backend::Neon,
            #[cfg(target_arch = "aarch64")]
            Backend::NeonGeneric,
            Backend::StdSimd,
            Backend::Scalar,
            Backend::Portable,
        ]
    }

    fn info(self) -> &'static BackendInfo {
        match self {
            #[cfg(target_arch = "x86_64")]
            Backend::Avx512 => &BackendInfo { name: "avx512", requires: Avx512::FEATURES, summary: "percent codec with vpcompressb and masked stores (64 bytes per step); hex, base64 as avx2" },
            #[cfg(target_arch = "x86_64")]
            Backend::Avx2 => &BackendInfo { name: "avx2", requires: Avx2::FEATURES, summary: "32 bytes per step as two 16-byte lanes" },
            #[cfg(target_arch = "x86_64")]
            Backend::Sse41 => &BackendInfo { name: "sse41", requires: Sse41::FEATURES, summary: "16 bytes per step (pshufb, pmaddubsw, ptest)" },
            #[cfg(target_arch = "aarch64")]
            Backend::Neon => &BackendInfo { name: "neon", requires: Neon::FEATURES, summary: "NEON-native hex and base64 (vld2/3/4, vst2/3/4, 64-byte tbl); generic percent codec" },
            #[cfg(target_arch = "aarch64")]
            Backend::NeonGeneric => &BackendInfo { name: "neon-generic", requires: Neon::FEATURES, summary: "the generic kernels on NEON (pmaddubsw, pmulhuw and movemask emulated)" },
            Backend::StdSimd => &BackendInfo { name: "stdsimd", requires: &[], summary: "the generic kernels on std::simd u8x16, for the build target's features" },
            Backend::Scalar => &BackendInfo { name: "scalar", requires: &[], summary: "hand-written scalar loops" },
            Backend::Portable => &BackendInfo { name: "portable", requires: &[], summary: "generic kernels on [u8; 16] arrays" },
        }
    }
}

/// Defines `pub fn $name(backend: Backend, args..) -> R` that runs the generic kernel
/// `$kernel::<S>(args..)` (an `#[inline(always)] unsafe fn` generic over `S: Simd`, unsafe only
/// for its CPU features) for the backend's token `S`, or `$scalar(args..)` for `Scalar`.
/// `neon = path` / `avx512 = path` replace the kernel for that backend with an ISA-specific
/// `unsafe fn` of the same signature; without `avx512 = ...` the AVX-512 backend runs the AVX2
/// kernel.
///
/// The token is checked on every call (free once the build enables the features, e.g. with
/// `-C target-cpu=native`), and the kernel runs inside the token's `vectorize`, so its inlined
/// intrinsics are compiled for the backend's features. An unsupported backend panics.
#[doc(hidden)]
#[macro_export]
macro_rules! define_dispatch {
    ($name:ident, $kernel:ident, fn($($p:ident : $t:ty),*) -> $r:ty, scalar = $scalar:path
     $(, neon = $neon:path)? $(, avx512 = $avx512:path)? $(,)?) => {
        #[inline]
        pub fn $name(backend: $crate::Backend, $($p: $t),*) -> $r {
            use simdlab::Token as _;
            use $crate::backend::{self as bk, require, Backend as B};
            // SAFETY (every arm): the kernels' only precondition is the token's CPU features.
            match backend {
                B::Scalar => $scalar($($p),*),
                B::Portable => unsafe { $kernel::<bk::Portable>($($p),*) },
                B::StdSimd => unsafe { $kernel::<bk::StdSimd>($($p),*) },
                #[cfg(target_arch = "x86_64")]
                B::Sse41 => require::<bk::Sse41>().vectorize(|| unsafe { $kernel::<bk::Sse41>($($p),*) }),
                #[cfg(target_arch = "x86_64")]
                B::Avx2 => require::<bk::Avx2>().vectorize(|| unsafe { $kernel::<bk::Avx2>($($p),*) }),
                #[cfg(target_arch = "x86_64")]
                B::Avx512 => require::<bk::Avx512>()
                    .vectorize(|| unsafe { $crate::__kernel_or!($($avx512)?; $kernel::<bk::Avx2>)($($p),*) }),
                #[cfg(target_arch = "aarch64")]
                B::Neon => require::<bk::Neon>()
                    .vectorize(|| unsafe { $crate::__kernel_or!($($neon)?; $kernel::<bk::Neon>)($($p),*) }),
                #[cfg(target_arch = "aarch64")]
                B::NeonGeneric => require::<bk::Neon>().vectorize(|| unsafe { $kernel::<bk::Neon>($($p),*) }),
            }
        }
    };
}

/// `define_dispatch!` helper: the ISA-specific kernel if one was given, else the generic one.
#[doc(hidden)]
#[macro_export]
macro_rules! __kernel_or {
    (; $($generic:tt)*) => { $($generic)* };
    ($native:path; $($generic:tt)*) => { $native };
}
