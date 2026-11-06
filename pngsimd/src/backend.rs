//! Backends of this experiment.
//!
//! Every backend for the target architecture is compiled in; the best one this CPU supports is
//! used unless the caller picks another ([`crate::Options::backend`], `--backend`). A backend's
//! kernels are reached through [`Backend::kernels`], which checks that the CPU supports it.

use crate::kernels::{portable, scalar, stdsimd, Kernels};
use simdlab::{BackendInfo, Backends};

/// Every backend, best first. The scalar backend beats the plain-Rust "portable" vectors on every
/// tested ISA (REPORT-rust.md §3.5), so it comes before it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Backend {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    Avx512,
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    Avx2,
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    Sse41,
    #[cfg(target_arch = "aarch64")]
    Neon,
    StdSimd,
    Scalar,
    Portable,
}

impl Backends for Backend {
    const REFERENCE: Self = Backend::Scalar;

    fn all() -> &'static [Self] {
        &[
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            Backend::Avx512,
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            Backend::Avx2,
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            Backend::Sse41,
            #[cfg(target_arch = "aarch64")]
            Backend::Neon,
            Backend::StdSimd,
            Backend::Scalar,
            Backend::Portable,
        ]
    }

    // The requirement lists are the union of the `#[target_feature]`s of the backend's kernels
    // in kernels/x86.rs and kernels/neon.rs.
    fn info(self) -> &'static BackendInfo {
        match self {
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            Backend::Avx512 => &BackendInfo {
                name: "avx512",
                requires: &["avx512f", "avx512bw", "avx512vl", "avx512vbmi", "vpclmulqdq", "pclmulqdq"],
                summary: "VPCLMULQDQ CRC (2 zmm), 512-bit Adler, vpternlog Paeth select, vpermb RGB->RGBA",
            },
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            Backend::Avx2 => &BackendInfo {
                name: "avx2",
                requires: &["avx2", "pclmulqdq", "sse4.1"],
                summary: "PCLMULQDQ CRC, 256-bit Adler/Up/expand, 128-bit VEX filters",
            },
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            Backend::Sse41 => &BackendInfo {
                name: "sse41",
                requires: &["ssse3", "sse4.1", "pclmulqdq"],
                summary: "PCLMULQDQ CRC, pmaddubsw Adler, 128-bit filters, pshufb expand",
            },
            #[cfg(target_arch = "aarch64")]
            Backend::Neon => &BackendInfo {
                name: "neon",
                requires: &["neon"],
                summary: "128-bit filters, vmull Adler, tbl expand; PMULL CRC when the CPU has it",
            },
            Backend::StdSimd => &BackendInfo {
                name: "stdsimd",
                requires: &[],
                summary: "the generic Vec128 kernels and Adler-32 on std::simd, for the build target's features; scalar CRC",
            },
            Backend::Scalar => &BackendInfo {
                name: "scalar",
                requires: &[],
                summary: "register-carried filters, slicing-by-8 CRC",
            },
            Backend::Portable => &BackendInfo {
                name: "portable",
                requires: &[],
                summary: "the generic Vec128 kernels on [u8; 16] (plain Rust)",
            },
        }
    }
}

impl Backend {
    /// The backend's kernel table. Panics if this CPU does not support the backend (backends from
    /// [`Backends::best`], [`Backends::available`] or `--backend` always are).
    pub fn kernels(self) -> &'static Kernels {
        assert!(self.is_supported(), "backend {} is not supported by this CPU (missing: {})", self.name(), self.missing_features().join(" "));
        match self {
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            Backend::Avx512 => &crate::kernels::x86::KERNELS_AVX512,
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            Backend::Avx2 => &crate::kernels::x86::KERNELS_AVX2,
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            Backend::Sse41 => &crate::kernels::x86::KERNELS_SSE41,
            #[cfg(target_arch = "aarch64")]
            Backend::Neon => &crate::kernels::neon::KERNELS_NEON,
            Backend::StdSimd => &stdsimd::KERNELS,
            Backend::Scalar => &scalar::KERNELS,
            Backend::Portable => &portable::KERNELS,
        }
    }
}
