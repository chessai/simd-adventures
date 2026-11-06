//! vecfilter — cache-line-blocked Bloom filter and cuckoo filter with SIMD backends.
//!
//! A Rust port of the header-only C++17 library in `cpp/`. The structure follows the
//! C++ one-to-one:
//!
//! | module | C++ header |
//! |---|---|
//! | [`backend`] | `vf/config.h`: which backends exist; here chosen at run time |
//! | [`simd`] | `vf/simd.h`: the [`simd::Ops`] trait for `Portable`, `StdSimd`, `Avx2`, `Avx512`, `Neon` |
//! | [`hash`] | `vf/hash.h`: 64-bit key → (`hb`, `hl`) with 32-bit arithmetic, scalar and 8-wide bit-identical |
//! | [`blocked_bloom`] | `vf/blocked_bloom.h` |
//! | [`cuckoo`] | `vf/cuckoo.h` |
//! | [`classic_bloom`] | `vf/classic_bloom.h` |
//! | [`keygen`] | `vf/keygen.h` |
//! | `simdlab::buf`, `simdlab::perf` | `vf/alloc.h`, `vf/perf.h` |
//!
//! Filters are generic over a backend token: `BlockedBloom::<Avx2, 32>::new(token, bytes)`, with
//! the token from detection (`simdlab::Token::detect`). To pick the backend at run time, call a
//! function generic over the token through [`with_backend!`].

#![feature(portable_simd)] // the `stdsimd` backend

pub mod backend;
pub mod blocked_bloom;
pub mod classic_bloom;
pub mod cuckoo;
pub mod hash;
pub mod keygen;
pub mod simd;

pub use backend::Backend;
pub use blocked_bloom::BlockedBloom;
pub use classic_bloom::ClassicBloom;
pub use cuckoo::CuckooFilter;
