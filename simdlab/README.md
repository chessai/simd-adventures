# simdlab — shared infrastructure for the experiments

| module | what |
|---|---|
| `backend` | `Backends` (implemented by each experiment's `Backend` enum): detection, best/available, by-name lookup, `table()` |
| `cpu` | runtime CPU feature detection by name (`has`, `missing`, `KNOWN`) |
| `Token`, `target_token!` | zero-sized proof of CPU support with `vectorize(|| ...)`, which runs a closure in a `#[target_feature]` function |
| `cli` | clap arguments: `BackendArg` (`--backend NAME`), `BackendsArg` (`--backend a,b`), `--list-backends`, env `SIMDLAB_BACKEND`; `parse_size` |
| `perf` | `Counters` (perf_event_open, one fd per event, multiplexing-scaled, NaN when refused), `Sample`, `fixed_counter` (TSC), `core_clock` |
| `rng` | `SplitMix64`, `XorShift64`, `XorShift64Star` (their sequences are part of recorded results: never change them) |
| `buf` | `AlignedBuf`: zeroed, 64-byte aligned, 2 MiB-aligned `MADV_HUGEPAGE` mappings from 4 MiB |
| `arch` | `prefetch_read`, `prefetch_write` |
| `testing` | `for_each_backend` (runs supported backends, reports skipped ones), `iterations` (`SIMDLAB_TEST_SCALE`) |

The `simdlab` binary has the experiment-independent tools:

```sh
simdlab cpu          # CPU model, detected features, which perf counters open
simdlab corefreq     # core clock relative to the TSC (for TSC-only machines)
simdlab membench     # random 64-byte access latency and memory-level parallelism, 1 MiB .. 2 GiB
```
