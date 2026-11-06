# pngsimd (Rust) — SIMD-accelerated PNG decoding stages, inflate deliberately scalar

A Rust port of a C project (not in this repository; it would live in `c/`): a small PNG decoder
whose byte-crunching stages are vectorised and measured, with a safe public API and the SIMD
confined to `#[target_feature]` entry points behind contract-checking wrappers.

| stage | kernel | scalar baseline | SSE4.1/PCLMUL | AVX2 | AVX-512 | NEON | `std::simd` | portable |
|---|---|---|---|---|---|---|---|---|
| chunk CRC | CRC-32, carry-less-multiply folding (4 × 128-bit accumulators, Barrett finish) | slicing-by-8 table | PCLMULQDQ | PCLMULQDQ | VPCLMULQDQ, 2 zmm | PMULL | table (no carry-less multiply) | table |
| zlib trailer | Adler-32, widening multiply-add, modulo deferred 5552 bytes | 8× unrolled | pmaddubsw/psadbw | 256-bit | 512-bit | vmull/vpadal | `u8x32` widening, 32..1 weights | block form |
| unfilter | Sub (log-step prefix scan), Up, Avg, Paeth for 1/2/3/4/6/8 bpp | register-carried, const-generic | `Vec128` | `Vec128` (+256-bit Up) | `Vec128` (+vpternlog select, 64-byte Up) | `Vec128` | `Vec128` on `u8x16` | `Vec128` on `[u8;16]` |
| expand | RGB→RGBA (byte shuffle), 16→8 (mask+pack) | iterators | pshufb / packus | 256-bit | vpermb / vpmovwb | tbl / movn | `Vec128` | plain Rust |
| inflate | **scalar, 100 % safe Rust** (see REPORT-rust.md §6) | — | — | — | — | — | — | — |

`Vec128` (`src/v128/mod.rs`) is a ~40-method trait with an associated vector type, implemented for
`__m128i` (three marker types that differ only in how `select` is encoded), `uint8x16_t`,
`std::simd::u8x16` (byte shifts as const swizzles, `shuffle8` as `swizzle_dyn`) and `[u8; 16]`;
the generic kernels in `src/kernels/generic.rs` are monomorphised once per backend.
Backends are compiled for the target architecture and chosen at run time: the best one this CPU
supports, unless `--backend` / `Options::backend` picks another (`src/backend.rs`).

`stdsimd` (Zen 3, 32 KiB, pinned) is within ±10% of `sse41` on the unfilter and expand kernels,
0.58x on Adler-32 (3.9x scalar), and uses the scalar CRC table. Its `shuffle8` is `swizzle_dyn`,
a scalar loop on x86-64 with the prebuilt std (see `../README.md`), but every shuffle here has
constant indices, which LLVM folds into `pshufb` anyway.

**Read `REPORT-rust.md`** for the measurements and for what changed between the C and Rust
versions — several of the differences are the compilers', not the languages'.

## Build and run

From the workspace root, inside `nix develop` (the workspace conventions are in `../README.md`):

```
cargo test --release -p pngsimd                   # property tests vs scalar/crc32fast/adler, inflate vs flate2, corpus vs the png crate
cargo test --release -p pngsimd --target aarch64-unknown-linux-gnu   # the NEON backend, under qemu-aarch64
nix build .#pngsimd-corpus                        # the corpus (or: python3 scripts/make_corpus.py, needs scikit-image + Pillow)
cargo run --release --bin pngsimd_decode -- pngsimd/corpus/rgb8_astronaut.png out.ppm
cargo run --release --bin pngsimd_bench_kernels -- --sizes 32k --dists photo --kernels filter
pngsimd/scripts/run_all.sh                     # everything -> pngsimd/results/ (builds with all features)
```

Every binary takes `--backend NAME` (default: the best this CPU supports; `SIMDLAB_BACKEND`
sets it too) and `--list-backends`. The benchmarks accept a list (`--backend scalar,avx2`) and
default to every supported backend (`pngsimd_bench_decode` leaves out `portable` unless asked).

Features: `zlib` (system zlib for the inflate stage, "+zlib" rows), `fdeflate` (the `fdeflate`
crate for the inflate stage, "+fdeflate" rows), `pngcrate` (compare with the pure-Rust `png`
crate), `libpng` (compare with the system libpng16 through its simplified API). The library's
only required dependency is the workspace's `simdlab` crate (backend selection, perf counters).

Core-clock conversion for TSC-only machines is `simdlab corefreq` (was `corefreq` here).

## Layout

```
src/lib.rs                 crate root, Error
src/backend.rs             Backend enum: names, CPU requirements, Backend::kernels()
src/v128/{mod,sse,neon,stdsimd,portable}.rs   the Vec128 trait and its implementations
src/kernels/generic.rs     generic filters / RGB->RGBA / 16->8 over Vec128
src/kernels/{scalar,x86,neon,stdsimd,portable}.rs   backends; x86.rs also holds the CRC/Adler kernels
src/kernels/mod.rs         Kernels table, contract checks
src/inflate.rs             scalar RFC 1950/1951 (safe Rust)
src/png.rs                 chunk walk, unfilter, expand, per-stage timers
src/bin/                   pngsimd_decode, pngsimd_bench_kernels (counters when available), pngsimd_bench_decode
tests/                     kernels.rs (property/fuzz), inflate.rs (round-trip + mutation), png_corpus.rs
scripts/                   corpus generator, CRC constant derivation, cachegrind driver, summariser
results/                   output of the last scripts/run_all.sh (from the machine in results/machine.txt)
```
