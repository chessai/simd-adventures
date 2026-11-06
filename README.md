rustc version: rustc 1.94.0-nightly (0aced202c 2026-01-06)
# simd_adventures

SIMD experiments in Rust. One Cargo workspace, one toolchain (the Nix flake), and one set of
conventions, so every experiment is built, run, tested and measured the same way.

| directory | what |
|---|---|
| [`simdlab/`](simdlab) | Shared infrastructure: backend detection and selection, the `--backend` CLI argument, perf counters, RNGs, aligned buffers. The `simdlab` tool: `cpu`, `corefreq`, `membench`. |
| [`template/`](template) | The smallest complete experiment (wrapping `u32` sum). Copy it to start a new one. |
| [`pngsimd/`](pngsimd) | PNG decoder whose CRC-32, Adler-32, unfilter and expand stages are vectorised (port of a C project). |
| [`vecfilter/`](vecfilter) | Cache-line-blocked Bloom and cuckoo filters with SIMD backends (port of the C++ original in [`vecfilter/cpp/`](vecfilter/cpp), kept as the reference). |
| [`vcodec/`](vcodec) | Hex, base64 and percent (URL) codecs on one SIMD trait with one streaming API; the hard case is percent encoding's variable-length output. NEON-native and AVX-512 VBMI2 kernels where the trait fits the ISA badly. |

Each experiment directory is a Rust crate (a workspace member). An implementation in another
language, such as the original an experiment was ported from, lives inside it in a directory
named for the language: `c/`, `cpp/`, ... (`vecfilter/cpp/`).

## Build and test

```sh
nix develop                      # or direnv: toolchain, aarch64 cross linker, qemu, perf, python
cargo test --release --workspace                                        # every backend this CPU supports
cargo test --release --workspace --target aarch64-unknown-linux-gnu     # NEON backends, under qemu-aarch64
nix flake check                  # both of the above in the sandbox, plus an all-features build
cargo run --release --bin simdlab -- cpu                                # what this machine supports
```

The dev shell sets `-C target-cpu=native` for x86-64 builds (`.cargo/config.toml`). The toolchain
is a pinned nightly: every experiment has a `std::simd` backend (`#![feature(portable_simd)]`).
One `std::simd` pitfall on x86-64: `swizzle_dyn` (dynamic byte shuffle) picks its implementation
when *std* is compiled, and the prebuilt std targets baseline x86-64 (no SSSE3), so it is a scalar
loop regardless of `-C target-cpu`. Rebuild std to measure `std::simd` fairly:
`cargo build --release -Zbuild-std --target x86_64-unknown-linux-gnu` (vcodec: 6-30x faster).
New files must be `git add`ed before `nix flake check` sees them.

## Conventions: every experiment works the same way

1. **Backends** are a `pub enum Backend` in `src/backend.rs`, best first, with variants that exist
   only on their architecture. It implements `simdlab::Backends`: a name, the CPU features it
   `requires` (`simdlab::cpu` names, which are also `#[target_feature]` names), a one-line summary,
   and the `REFERENCE` backend everything is tested against. Naming: `scalar` is plain scalar code,
   `portable` the generic vector-shaped code on plain arrays, `stdsimd` the same generic code on
   `std::simd` vectors (no runtime requirements: it uses whatever the build target enables, AVX2
   here with `-C target-cpu=native`, NEON on AArch64), and ISA names (`sse41`, `avx2`, `avx512`,
   `neon`) are intrinsics backends. Every experiment has a `stdsimd` backend, so each one also
   measures portable SIMD against hand-written intrinsics.
2. **Selection happens at run time.** Every backend for the target architecture is compiled; the
   default is the best one this CPU supports. `--backend NAME` (or `SIMDLAB_BACKEND`) overrides,
   and `--list-backends` prints the table. Libraries take a backend as an argument and never read
   the environment.
3. **SIMD code is reachable only after the runtime check**, so the safe API is sound on any CPU:
   through a token (`simdlab::target_token!`, `Token::detect`) or a checked accessor
   (`pngsimd::Backend::kernels`).
4. **SIMD code runs inside a `#[target_feature]` function**, so generic `#[inline(always)]` code is
   compiled for the backend's features. There are two dispatch shapes, chosen by granularity:
   - fine-grained APIs (one key, one element): generic over the token type and dispatched once at
     the top with the crate's `with_backend!` macro; the bodies run in `Token::vectorize`. With
     `-C target-cpu=native` these calls inline into the caller's loop (`vecfilter`, `template`);
   - bulk kernels (whole buffers): dispatched per call, which costs nothing per byte. Either a
     table of function pointers per backend whose entries are `#[target_feature]` functions
     (`pngsimd`), or kernels generic over the token type behind a `match` that gets the token
     with `simdlab::require` and runs the kernel in `Token::vectorize` (`vcodec`'s
     `define_dispatch!`, which also lets a backend replace a generic kernel with one written for
     its ISA).
5. **Tests** run every supported backend against the reference with
   `simdlab::testing::for_each_backend`, which also reports the backends this CPU skipped.
   Iteration counts go through `simdlab::testing::iterations(n)`, scaled by `SIMDLAB_TEST_SCALE`
   (e.g. `0.1` for a quick run, `10` for a soak). The aarch64 flake check runs them under QEMU.
6. **Binaries** are named `<crate>_<tool>` and use clap, with
   `#[command(name = env!("CARGO_BIN_NAME"))]` and a flattened `simdlab::cli::BackendArg` (one
   backend) or `BackendsArg` (a list, default every supported backend). Sizes are parsed with
   `simdlab::cli::parse_size` (`32k`, `1.5M`, `1G`).
7. **Measurements** use `simdlab::perf::Counters`; a `Sample` holds wall time, TSC ticks and the
   counters, with NaN for counters the machine refuses. Inputs come from `simdlab::rng` and large
   buffers from `simdlab::buf::AlignedBuf`.
8. **Results**: `scripts/run_all.sh [dir]` writes everything into `results/`, including `cpu.txt`
   (`simdlab cpu`) and `backends.txt` (`--list-backends`), so every result set records the machine
   it came from. Report scripts read only that directory.
9. **The README** of an experiment covers what it is, its layout, how to build and run it, and its
   verification status: what was run where, and what wasn't.

## Starting a new experiment

1. `cp -r template myexp`, rename `template` to `myexp` in `myexp/Cargo.toml` and in the code, and
   add `"myexp"` to the workspace `members` in `Cargo.toml`.
2. Rewrite `src/ops.rs` (the vector operations the algorithm needs, one impl per token),
   `src/kernels.rs` (the algorithm once, generic over the ops, plus the scalar reference) and
   `src/backend.rs` (which backends exist and what they require).
3. Keep the shape of `tests/backends.rs` (every backend against the reference) and of
   `src/bin/bench.rs` (shared arguments and counters), and update `scripts/run_all.sh`.
4. `git add myexp && nix flake check`: the native and aarch64 checks pick up the new tests.

For bulk kernels, a `pngsimd`-style table of `#[target_feature]` functions may fit better than
tokens; the backend enum, CLI, tests and results conventions stay the same either way.

## Hardware notes

The development machine (AMD Zen 3) has AVX2 but not AVX-512, and QEMU does not emulate AVX-512,
so the AVX-512 backends are compiled and checked but have not been run here. Where it matters
most (vcodec's VBMI2 kernels), the kernels are written against a small ops trait that the tests
also run on an array emulation, so the algorithm is tested and only the one-intrinsic-per-op
mapping is not. NEON runs under qemu-aarch64: correct results, but meaningless timings.
