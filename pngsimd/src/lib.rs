//! pngsimd — a minimal PNG decoder whose byte-crunching stages are vectorised.
//!
//! ```text
//! chunk parse + CRC-32  ->  inflate (scalar) + Adler-32  ->  unfilter  ->  expand (RGB->RGBA, 16->8)
//! ```
//!
//! Every SIMD kernel has a scalar reference implementation ([`kernels::scalar`]) and backends for
//! x86 SSE4.1+PCLMUL, AVX2, AVX-512 and AArch64 NEON+PMULL, plus `std::simd` and portable fallbacks, all written
//! against the same 128-bit abstraction ([`v128::Vec128`]). The backend is chosen at run time
//! ([`Backend`]: the best one this CPU supports unless the caller picks another). See
//! REPORT-rust.md.

#![feature(portable_simd)] // the `stdsimd` backend

pub mod backend;
pub mod v128;
pub mod kernels;
pub mod inflate;
pub mod png;

pub use backend::Backend;
pub use kernels::{Kernels, ROW_SLACK};
pub use png::{decode, Image, Inflater, Options, Stage, STAGE_COUNT, STAGE_NAMES};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Signature,
    Chunk,
    Crc,
    Ihdr,
    /// interlaced, or an odd depth/colour combination
    Unsupported,
    Zlib,
    Adler,
    Filter,
    Truncated,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Error::Signature => "bad signature",
            Error::Chunk => "bad chunk structure",
            Error::Crc => "chunk CRC mismatch",
            Error::Ihdr => "bad IHDR",
            Error::Unsupported => "unsupported image (interlaced?)",
            Error::Zlib => "corrupt zlib stream",
            Error::Adler => "Adler-32 mismatch",
            Error::Filter => "bad filter type",
            Error::Truncated => "truncated",
        })
    }
}
impl std::error::Error for Error {}
