# template — the smallest complete experiment

A wrapping sum of a `u32` slice with `scalar` (reference), `portable`, `stdsimd`, `avx2` and `neon`
backends. It exists to be copied: it follows every workspace convention (`../README.md`) in as
few lines as possible, and it is built and tested like the real experiments, so it cannot rot.

| file | role |
|---|---|
| `src/backend.rs` | `Backend` enum (`simdlab::Backends`), token types, `with_backend!` |
| `src/ops.rs` | the vector operations the algorithm needs, one impl per token |
| `src/kernels.rs` | the algorithm, written once over the ops, and the scalar reference |
| `src/lib.rs` | the safe API: `sum_u32(backend, xs)` |
| `tests/backends.rs` | every supported backend against the reference |
| `src/bin/bench.rs` | `template_bench`: `--backend`, `--sizes`, counters, TSV |
| `scripts/run_all.sh` | full run into `results/` |

```sh
cargo test --release -p template
cargo run --release --bin template_bench -- --sizes 1k,1M
cargo run --release --bin template_bench -- --list-backends
```

To start a new experiment from it, follow "Starting a new experiment" in `../README.md`.
