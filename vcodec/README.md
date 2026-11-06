rustc version: rustc 1.94.0-nightly (0aced202c 2026-01-06)
# vcodec — vectorised hex / base64 / percent codecs on one framework

Three codecs, one SIMD abstraction, one streaming API, one scalar oracle. The design, the
measurements and what did and did not vectorise are in [REPORT.md](REPORT.md); the workspace
conventions are in [`../README.md`](../README.md).

| codec | encode kernel | decode kernel | difficulty |
|---|---|---|---|
| `hex` | nibble split → 2× `pshufb` lookup → interleave | nibble-LUT (or range-check) classify, `pmaddubsw` combine, `packus` | easy: fixed ratio, byte-local |
| `base64` (std + URL-safe) | Muła/Lemire `pshufb` + `pmulhuw`/`pmullw` bit repack, 13-entry offset LUT | nibble-LUT validity bitmask, roll LUT, `pmaddubsw`+`pmaddwd` pack, `pshufb` compact | medium: fixed ratio, bits cross bytes, lane-crossing I/O |
| `url` (percent) | nibble-LUT classify → `movemask`; all-safe ⇒ straight store; else 4-byte-group expansion via mask-indexed shuffle table, offsets = masked `popcnt` prefix sums | `%` mask + two lookahead loads (handles `%XX` across block boundary), hex kernel reuse, blend, 256-entry compaction tables | hard: variable-length output |

## Backends

The codecs are written once against the `Simd` trait; most backends are instantiations of it,
and two replace some kernels with ones written for their ISA (`src/native/`), where the trait's
x86-shaped model fits the ISA badly.

| backend | hex | base64 | percent (url) |
|---|---|---|---|
| `avx512` (F/BW/VBMI/VBMI2, BMI2) | as `avx2` | as `avx2` | **native**: `vpcompressb` + masked stores, 64 bytes per step (REPORT.md §7) |
| `avx2` | generic, 2 × 16-byte lanes | generic | generic |
| `sse41` | generic, 16 bytes | generic | generic |
| `neon` | **native**: `vld2`/`vst2`, 64-entry `tbx` | **native**: `vld3`/`vst4`, `vld4`/`vst3`, 64-entry `tbl`/`tbx` | generic |
| `neon-generic` | generic (`pmaddubsw`, `pmulhuw`, `movemask` emulated) | generic | generic |
| `stdsimd` | generic on `std::simd::u8x16`, for the build target's features | generic | generic |
| `scalar` (the reference; the fallback without SIMD) | hand-written loop | hand-written loop | hand-written loop |
| `portable` (a correctness backend) | generic on `[u8; 16]` arrays | generic | generic |

`neon-generic` keeps the trait-based NEON code next to the native kernels, so the two can be
measured against each other on AArch64 hardware. `stdsimd` is 16 bytes wide because the trait's
lane model is 16-byte shuffles: `swizzle_dyn` on `u8x16` is exactly `pshufb` / `tbl`.

## Layout

```
src/backend.rs                             Backend enum, tokens, define_dispatch! (with per-ISA kernel overrides)
src/simd/{mod,portable,stdsimd,sse,avx2,neon}.rs   thin Simd trait (lane-local semantics), implemented by the tokens
src/native/neon.rs                         NEON-native hex and base64 kernels
src/native/vbmi2.rs                        AVX-512 VBMI2 percent codec, on an ops trait with an array emulation for tests
src/stream.rs                              Transform trait, Step/Status/Error, Stash, drivers
src/hex.rs src/base64.rs src/url.rs        scalar baseline + generic kernel + streaming transform
src/workload.rs                            benchmark workloads: codec x op x input distribution
src/bin/bench.rs                           vcodec_bench: GB/s + hardware counters (simdlab::perf)
benches/codecs.rs                          criterion: vs scalar, hex, hex-simd, base64, base64-simd, OpenSSL, percent-encoding
tests/props.rs  fuzz/                      property tests (proptest) + cargo-fuzz targets
scripts/                                   run_all.sh; perf_stat.sh, cachegrind.sh, mca.sh; summarize_criterion.py, tables.py
results/xeon-vm/                           the original run that REPORT.md describes (Xeon VM, no PMU)
```

## Build and run

From the workspace root, inside `nix develop`:

```sh
cargo test --release -p vcodec                                       # every supported backend vs scalar
cargo test --release -p vcodec --target aarch64-unknown-linux-gnu    # neon, neon-generic, stdsimd under qemu-aarch64
SIMDLAB_TEST_SCALE=30 cargo test --release -p vcodec                 # soak: 18k cases per property
cargo run --release --bin vcodec_bench -- --list-backends
cargo run --release --bin vcodec_bench -- --codec base64 --op decode --size 64k   # every supported backend
cargo run --release --bin vcodec_bench                               # full matrix, TSV
cargo bench -p vcodec --features openssl                             # criterion vs other crates (~10 min)
(cd vcodec/fuzz && cargo fuzz run roundtrip)                         # libFuzzer (its own workspace)
vcodec/scripts/run_all.sh                                            # everything above into vcodec/results/
```

`vcodec_bench` takes comma-separated lists for `--codec`, `--op`, `--size`, `--dist` and the
workspace-wide `--backend` (default: every backend this CPU supports; `SIMDLAB_BACKEND` sets it
too), runs every combination, and reports the fastest of `--repeat` measurements.

## Using the library

```rust
use simdlab::Backends;
use vcodec::{Backend, Transform, Status};
use vcodec::base64::{Base64Encoder, Alphabet};

// one-shot (the best backend this CPU supports)
let s = vcodec::base64::encode(b"hello");
let v = vcodec::url::decode(b"a%20b")?;

// streaming: chunked input, bounded output, resumable
let mut enc = Base64Encoder::new(Backend::best(), Alphabet::UrlSafe, /*pad*/ false);
let mut out = [0u8; 64];
for chunk in input.chunks(1000) {
    let mut i = 0;
    loop {
        let step = enc.update(&chunk[i..], &mut out);
        sink.extend_from_slice(&out[..step.written]);
        i += step.consumed;
        match step.status {
            Status::Done => break,
            Status::OutputFull => continue,        // `out` was too small: loop
            Status::Error(e) => return Err(e),     // e.offset = absolute input offset
        }
    }
}
let step = enc.finish(&mut out); // padding / tail
```

A transform holds a `Backend`; each `update` dispatches once to the bulk kernel for that backend
(`define_dispatch!`), which obtains the backend's token (a free check when the build enables the
feature, as the workspace's `-C target-cpu=native` does) and runs the kernel inside the token's
`vectorize`. A backend the CPU does not support panics instead of running.

## Changes from the standalone crate

REPORT.md describes the crate as it was before it joined this workspace. Since then:

- **Backend selection** follows the workspace conventions: `Backend` implements
  `simdlab::Backends` (`Backend::best()` replaces `detect()`, `available()` replaces
  `all_supported()`), and `sse4.1` is now named `sse41`. Holding an unsupported `Backend` used to
  fall back to `Portable` at dispatch; it now panics, like the other experiments.
- **Dispatch** uses simdlab tokens (`Sse41`, `Avx2`, `Neon`, `Portable` implement `Simd`) and
  `Token::vectorize` instead of hand-written `#[target_feature]` wrappers per ISA.
- **`perfbench` is `vcodec_bench`**, with clap lists instead of `--matrix`, and
  `simdlab::perf` counters instead of its own `perf_event_open` code. The input generator moved
  to `vcodec::workload` (on `simdlab::rng::XorShift64Star`), shared with the criterion bench:
  the bytes are unchanged.
- **Tests** scale with `SIMDLAB_TEST_SCALE` (it replaces `VCODEC_FUZZ_ITERS`), and the OpenSSL
  comparison is behind the `openssl` feature, so the default build needs no system libraries.
- **New backends**: `stdsimd`, the NEON-native hex/base64 kernels (`neon`, with the old NEON as
  `neon-generic`), and `avx512` with the VBMI2 percent codec sketched in REPORT.md §7.

## Verification status

All on the development machine (AMD Zen 3: AVX2, no AVX-512), 2026-11-05.

- **x86**: every test passes for `avx2`, `sse41`, `stdsimd`, `scalar` and `portable`, also at
  `SIMDLAB_TEST_SCALE=30` (before the new backends); both libFuzzer targets ran clean for 20 s
  each.
- **NEON** (`neon`, `neon-generic`, `stdsimd` under qemu-aarch64): every property test, known
  answer, error offset and the in-process fuzzer pass. The NEON-native kernels are also tested
  directly (`native::neon::tests`), because the transform tests would pass even if a kernel did
  nothing and left the work to the scalar path; they consume every whole block and stop before
  the block with an invalid character. Speed is unmeasured: QEMU timings mean nothing.
- **AVX-512** (`avx512`): compiled, never run (no hardware, no emulator). The VBMI2 percent
  kernels are tested on the array emulation of their ops (`native::vbmi2::tests`: output equals
  the scalar codec, nothing written past the output, no early stop, escapes across the block
  boundary); the mapping of each op to its intrinsic is not. `cargo test --release -p vcodec` on
  an AVX-512 VBMI2 machine (Ice Lake or later, Zen 4 or later) runs the real thing.
- **`std::simd`** (64 KiB, GB/s, pinned): with the prebuilt standard library `stdsimd` reaches
  only 3-10% of `sse41` on everything but all-safe percent decoding, often below `scalar`:
  `Simd::swizzle_dyn` (the `pshufb` of every kernel here) chooses its implementation with
  `cfg(target_feature)` when *std* is compiled, and the prebuilt x86-64 std has no SSSE3, so it
  is a scalar loop whatever `-C target-cpu` says. Rebuilding std for this CPU
  (`cargo build --release -Zbuild-std --target x86_64-unknown-linux-gnu`) gives 0.67-0.75x of
  `sse41` on base64 (no `pmaddubsw` / `pmulhuw` in `std::simd`), 0.82-1.12x on hex and
  0.93-1.21x on percent encoding, at the same 16-byte width. AArch64 is not affected (NEON is
  in its baseline).
- **Against the standalone crate** (same toolchain, `-C target-cpu=native`, fat LTO): wall clock,
  pinned to one core, best of 3 interleaved rounds of `scalar`/`sse41`/`avx2` × 64 B and 64 KiB
  × every codec, op and default distribution (72 rows): geomean 1.07x the original's throughput
  at 64 KiB and 1.035x at 64 B, rows between 0.82x and 1.33x. PMU instruction counts per byte
  (108 rows, before the new backends): geomean +1.0%, rows between -8% and +7%. The hot loop
  bodies are unchanged; the differences come from how LLVM lays out the large `update` functions
  the kernels inline into (scalar url encode gains one `jmp` per byte from a different loop
  rotation; the AVX2 base64 decode loop spills its output offset), not from the token dispatch:
  putting the original hand-written `#[target_feature]` wrapper back gives the same counts.
- `scripts/mca.sh` (static analysis of the AVX2/SSE hot loops) matches the original's
  `results/xeon-vm/mca.txt`: identical hex encode and base64 loops, hex decode 27 vs 28
  instructions, url escape paths within ±2 instructions.
- The criterion benchmarks ran here only as a smoke test, so the numbers in REPORT.md are still
  those of the original Xeon VM (`results/xeon-vm/`).
