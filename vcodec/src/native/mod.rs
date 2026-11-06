//! Kernels written for one ISA instead of against the [`Simd`](crate::simd::Simd) trait, for the
//! operations where the trait's x86-shaped model costs that ISA most. Each has the signature and
//! contract of the generic kernel it replaces (whole blocks only; decoders stop before a block
//! the scalar unit path must handle), and `define_dispatch!` selects it per backend.
//!
//! * [`neon`]: hex and base64 with structured loads/stores and 64-byte table lookups.
//! * [`vbmi2`]: the percent codec with AVX-512 VBMI2 compress and masked stores (REPORT.md §7).

#[cfg(target_arch = "aarch64")]
pub mod neon;
#[cfg(any(target_arch = "x86_64", test))]
pub mod vbmi2;
