# vecfilter (Rust) — cache-line-blocked Bloom filter and cuckoo filter with SIMD backends

A Rust port of the C++17 library in [`cpp/`](cpp) (kept as the reference implementation, with
its REPORT.md and measured results). Same filters, same four backends plus a `std::simd` one, same
hash, same benchmark/test harness. Read [`cpp/README.md`](cpp/README.md) and
[`cpp/REPORT.md`](cpp/REPORT.md) for the design and the measurements; the workspace conventions
are in [`../README.md`](../README.md).

| Rust | C++ (`cpp/`) |
|---|---|
| `src/backend.rs` — `Backend` enum + tokens `Portable`, `StdSimd`, `Avx2`, `Avx512`, `Neon` | `include/vf/config.h` |
| `src/simd/{mod,portable,stdsimd,avx2,avx512,neon}.rs` — `Ops` trait per token | `include/vf/simd.h` (no `std::simd` backend) |
| `src/hash.rs` | `include/vf/hash.h` |
| `src/blocked_bloom.rs` — `BlockedBloom<O, 32\|64, 1\|2>` | `include/vf/blocked_bloom.h` |
| `src/cuckoo.rs` — `CuckooFilter<O, u8\|u16>` | `include/vf/cuckoo.h` |
| `src/classic_bloom.rs`, `src/keygen.rs` | `include/vf/classic_bloom.h`, `keygen.h` |
| `simdlab::buf`, `simdlab::perf` (workspace crate) | `include/vf/alloc.h`, `perf.h` |
| `vecfilter_bench`, `vecfilter_fpr` | `bench_<isa>`, `fpr_<isa>` |
| `simdlab membench` | `membench` |
| `tests/props.rs`, `tests/fuzz_ops.rs` | `tests/test_props.cpp`, `tests/fuzz_ops.cpp` (random driver) |

## Build and run

From the workspace root, inside `nix develop`:

```sh
cargo test --release -p vecfilter                                            # every supported backend vs portable
cargo test --release -p vecfilter --target aarch64-unknown-linux-gnu         # NEON, under qemu-aarch64
cargo run --release --bin vecfilter_bench -- --sizes 256K,1M,16M --queries 16777216
cargo run --release --bin vecfilter_fpr -- uniform
cargo run --release --bin vecfilter_bench -- --list-backends
vecfilter/scripts/run_all.sh results && python3 vecfilter/scripts/make_report.py vecfilter/results
```

`vecfilter_bench` takes the same options as the C++ `bench` plus the workspace-wide
`--backend portable|stdsimd|avx2|avx512|neon` (default: the best this CPU supports; `SIMDLAB_BACKEND`
sets it too) and `--list-backends`. Text and CSV output use the C++ formats, so `make_report.py`
works unchanged.

## Using the filters

Filters are generic over a backend token, which only exists if the CPU supports the backend:

```rust
use simdlab::Token;
use vecfilter::{backend::Avx2, BlockedBloom};

let avx2 = Avx2::detect().expect("no AVX2");
let mut f = BlockedBloom::<Avx2, 32>::new(avx2, 1 << 20);
f.insert(42);
assert!(f.contains(42));
```

To choose at run time, write the code generic over `O: vecfilter::simd::Ops` and call it through
`vecfilter::with_backend!(backend, f(args))`, which passes the token for a `Backend` (from
`Backend::best()`, `Backend::available()` or `--backend`). Dispatch once, outside hot loops: each
filter method runs inside the token's `vectorize`, a `#[target_feature]` function, and with the
workspace's `-C target-cpu=native` those calls inline into the caller's loop exactly as the C++
does with its compile-time backend.

## Differences from the C++

- **Backend selection.** The C++ picks the backend at compile time (one binary per ISA). Here every
  backend for the target architecture is compiled and the best supported one is chosen at run time
  (`--backend` overrides). An interleaved A/B against the earlier compile-time version of this port
  measured no difference beyond noise (±3 %, cuckoo per-key insert +5 %).
- **Scalar baseline.** rustc cannot turn off auto-vectorisation per function, so the portable
  backend in the native build is LLVM-vectorised. `scripts/run_all.sh` builds a separate
  `-C no-vectorize-loops -C no-vectorize-slp` binary for the portable rows, like the C++
  `-fno-tree-vectorize` build.
- **Generic const expressions.** `shr_32::<{ 32 - log2(lane bits) }>` is not expressible on stable
  Rust, so the shift amounts are selected with `if BB == 32 { .. } else { .. }` on the const
  parameter (folded at compile time).
- **Tests** stop at the first failure (the C++ counts failures and continues) and use `BTreeMap` /
  `BTreeSet` models so runs are deterministic. The libFuzzer harness is not ported; the random
  program driver is (5000 programs per backend, scaled by `SIMDLAB_TEST_SCALE`).
- **AVX-512 paired lookup** (`VF_AVX512_PAIRED_LOOKUP`) is the cargo feature `avx512-paired-lookup`.
- **`std::simd` backend** (`stdsimd`, no C++ counterpart): the `Ops` trait on `u32x8`, `u64x8`
  and `u8x16`, compiled for the build target's features. `std::simd` shifts take the amount
  modulo the lane width, so the variable shifts carry an `and` that `vpsllvd` does not need.
  Measured (Zen 3, 256 KiB and 16 MiB filters, pinned): within ±15% of `avx2` on most rows,
  slower on 16 MiB `blocked64x2` batch lookups (1.47x the time), and reproducibly *faster* on
  `cuckoo8` inserts (1.2-2.4x), which points at something in the AVX2 insert path; not yet
  investigated.

## Verification status

- `vecfilter_fpr` output is byte-identical to the C++ `fpr` built with the same backend (checked:
  AVX2 uniform and clustered, portable sequential, NEON uniform under qemu), which exercises the
  hash, both Bloom layouts, both cuckoo fingerprint widths with kicks, and the key generators.
- Property tests and the random-program driver pass for portable + AVX2 (native) and portable +
  NEON (qemu-aarch64). The tests print which backends they ran and which this CPU skipped.
- **AVX-512 is compiled but not run** on the development machine (AVX2-only; QEMU does not emulate
  AVX-512). `cargo test --release -p vecfilter` on an AVX-512 machine covers it.
- Not yet benchmarked in full; `results/` is produced by `scripts/run_all.sh`.
