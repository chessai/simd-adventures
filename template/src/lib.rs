//! Template experiment: wrapping sum of a `u32` slice.
//!
//! The smallest crate that follows the workspace conventions (`../README.md`); copy it to
//! start a new experiment. The pieces:
//!
//! * [`backend`]: the `Backend` enum (names, CPU requirements, reference) and its token types;
//! * [`ops`]: the narrow vector-operation trait this experiment needs, implemented per token;
//! * [`kernels`]: the algorithm, written once over [`ops::SumOps`], plus the scalar reference;
//! * the safe, runtime-dispatched API below;
//! * `tests/backends.rs`: every supported backend against the reference;
//! * `src/bin/bench.rs`: `template_bench`, with the shared `--backend` argument and counters.

#![feature(portable_simd)] // the `stdsimd` backend

pub mod backend;
pub mod kernels;
pub mod ops;

pub use backend::Backend;

/// Wrapping sum of `xs`, computed with `backend`.
pub fn sum_u32(backend: Backend, xs: &[u32]) -> u32 {
    with_backend!(backend, kernels::sum_u32(xs))
}
