# vecfilter — cache-line-blocked Bloom filter and cuckoo filter with SIMD backends

Header-only C++17. Three filters, four backends, one benchmark/test harness.

| | |
|---|---|
| `include/vf/simd.h` | thin SIMD layer: `Portable` (scalar reference), `Avx2`, `Avx512`, `Neon` |
| `include/vf/hash.h` | 64-bit key → (`hb`, `hl`) using only 32-bit mul/xor/shift, scalar and 8-wide forms are bit-identical |
| `include/vf/blocked_bloom.h` | `BlockedBloom<Ops, BlockBytes=32\|64, BitsPerLane=1\|2>`: one block per key, k = 8 or 16, mask built with one multiply-shift, one vector test |
| `include/vf/cuckoo.h` | `CuckooFilter<Ops, u8\|u16>`: 4-slot buckets, both candidate buckets scanned by one 128-bit compare, insert/erase/victim stash |
| `include/vf/classic_bloom.h` | scalar baseline: k=7 double-hashed bit positions, early-exit loop |
| `include/vf/perf.h` | `perf_event_open` counters (cycles, instructions, branch-misses, L1D/LLC load misses) with TSC fallback |
| `include/vf/alloc.h` | 64-byte-aligned allocator; ≥4 MiB allocations are 2 MiB-aligned mappings with `MADV_HUGEPAGE` |
| `src/bench.cpp` | throughput + counters, per-key vs batched, sizes/distributions/window sweep, CSV output |
| `src/fpr.cpp` | FPR and bits/key at equal memory, Bloom variants vs cuckoo |
| `src/membench.cpp` | random-access latency / memory-level-parallelism roofline |
| `tests/test_props.cpp` | property tests: every backend vs portable, batched vs per-key, no false negatives vs model |
| `tests/fuzz_ops.cpp` | libFuzzer harness (`-DVF_ENABLE_FUZZ=ON`, clang) and a plain random driver used by `ctest` |
| `REPORT.md` | design, measurements, and the analysis of what vectorised and what didn't |

## Build and run

```sh
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build -j
ctest --test-dir build                     # property tests + random-program driver, every backend
build/bench_avx2 --sizes 256K,1M,16M,1G --queries 16777216 --csv results/main.csv
build/bench_avx2 --window-sweep --sizes 1G --only blocked32,cuckoo16
build/fpr_avx2 uniform
build/membench 1024
scripts/run_all.sh build results           # the whole matrix (~15 min); then
python3 scripts/make_report.py results     # -> results/tables.md
```

Each backend gets its own binaries (`bench_portable`, `bench_avx2`, `bench_avx512`, `bench_neon` on AArch64).
The portable binary is built with `-fno-tree-vectorize -fno-tree-slp-vectorize` so its numbers are really scalar.

`bench` options: `--sizes` (filter bytes, K/M/G suffixes), `--queries`, `--dist uniform|sequential|clustered`
(inserted keys), `--qdist mixed|absent|zipf` (lookup stream), `--bpk` (default 10), `--fill` (fraction of the
design-point key count), `--window` (prefetch window, default 32), `--reps`, `--only a,b`, `--window-sweep`,
`--no-hugepages`, `--csv file`.

## Hardware counters

`PerfGroup` opens one `perf_event_open` fd per event (so an unsupported event does not take the group down) and
reports whatever the kernel grants; `perf_event_paranoid` ≤ 2 is enough for user-space counting. Inside a VM without
a vPMU the harness falls back to TSC ticks for cycles and prints `n/a` for the rest. On bare metal `perf stat -e
cycles,instructions,branch-misses,L1-dcache-load-misses,LLC-load-misses build/bench_avx2 ...` gives the same
figures for the whole process.

## NEON

Cross-compile and run the tests under qemu:

```sh
aarch64-linux-gnu-g++ -std=c++17 -O2 -static -Iinclude tests/test_props.cpp -o test_neon && qemu-aarch64-static ./test_neon
```

Throughput numbers under qemu are meaningless; the NEON backend has only been *validated*, not benchmarked, here.

## Fuzzing

```sh
cmake -S . -B build-fuzz -DVF_ENABLE_FUZZ=ON -DCMAKE_CXX_COMPILER=clang++
cmake --build build-fuzz --target fuzz_ops
build-fuzz/fuzz_ops -max_len=4096 -max_total_time=300 corpus/
```

Note: GCC 13's AddressSanitizer miscompiles the AVX-512 build (spurious SEGV in 64-byte struct copies at `-O0`);
use clang for sanitizer builds, or GCC without `-fsanitize=address`.
