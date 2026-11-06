//! Shared infrastructure for the SIMD experiments in this workspace.
//!
//! Every experiment crate works the same way (see `../README.md`):
//!
//! * its backends are a `Backend` enum implementing [`Backends`]: name, required CPU features,
//!   one-line summary, best first, plus the reference backend the others are tested against;
//! * the backend is chosen at run time: the best one this CPU supports, unless `--backend`
//!   ([`cli::BackendArg`] / [`cli::BackendsArg`], env `SIMDLAB_BACKEND`) says otherwise;
//! * SIMD code is reachable only after the runtime check (a [`Token`] or a checked accessor), so
//!   the safe API stays sound on any CPU;
//! * tests run every supported backend against the reference ([`testing::for_each_backend`]);
//! * benchmarks use the same timing, counter and RNG code ([`perf`], [`rng`], [`buf`]).
//!
//! The `simdlab` binary has the experiment-independent tools: `cpu`, `corefreq`, `membench`.

pub mod arch;
pub mod backend;
pub mod buf;
pub mod cli;
pub mod cpu;
pub mod perf;
pub mod rng;
pub mod testing;
mod token;

pub use backend::{BackendError, BackendInfo, Backends};
pub use token::{require, Token};
