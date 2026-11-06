//! # vcodec — vectorised hex / base64 / percent codecs on one framework
//!
//! Three codecs, in increasing order of vectorisation difficulty:
//!
//! 1. [`hex`] — fixed 1→2 ratio, pure per-byte lookups. Almost free to vectorise.
//! 2. [`base64`] — fixed 3→4 ratio but bits move across byte boundaries;
//!    needs the Muła/Lemire multiply-add repacking and lane-crossing loads.
//! 3. [`url`] — *variable* 1→{1,3} ratio with escapes that can span block
//!    boundaries; needs mask classification, popcount prefix sums and
//!    shuffle-table compaction/expansion.
//!
//! All three share:
//! * the [`simd::Simd`] abstraction with portable / `std::simd` / SSE4.1 / AVX2 / NEON
//!   backends (plus ISA-specific kernels in [`native`]) and runtime dispatch ([`Backend`], see [`backend`]);
//! * the [`stream::Transform`] streaming API (chunked input, bounded output,
//!   resumable state);
//! * a scalar baseline in each module (`*_scalar` kernels) that is also the
//!   oracle for the property tests.

#![feature(portable_simd)] // the `stdsimd` backend
#![warn(missing_debug_implementations)]
// The `Simd` trait methods are `unsafe` for one uniform reason (CPU-feature
// requirement), documented once in `simd/mod.rs`.
#![allow(clippy::missing_safety_doc)]

pub mod backend;
pub mod base64;
pub mod hex;
pub mod native;
pub mod simd;
pub mod stream;
pub mod url;
pub mod workload;

pub use backend::Backend;
pub use stream::{Error, ErrorKind, Status, Step, Transform};
