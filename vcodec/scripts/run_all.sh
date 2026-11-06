#!/usr/bin/env bash
# Full benchmark run for this experiment. Usage: scripts/run_all.sh [results-dir]
# (results/xeon-vm holds the original run that REPORT.md describes.)
set -euo pipefail
cd "$(dirname "$0")/.."
R=${1:-results}
mkdir -p "$R"
cargo build --release -p vcodec -p simdlab
../target/release/simdlab cpu > "$R/cpu.txt"
../target/release/vcodec_bench --list-backends > "$R/backends.txt"
echo "== vcodec_bench matrix (GB/s + hardware counters, a few minutes)"
../target/release/vcodec_bench > "$R/bench.tsv"
echo "== criterion: vcodec vs hex, hex-simd, base64, base64-simd, OpenSSL, percent-encoding (~10 minutes)"
cargo bench -p vcodec --features openssl --bench codecs -- --noplot > "$R/criterion.log" 2>&1
python3 scripts/summarize_criterion.py > "$R/criterion.md"
echo "== cachegrind (simulated misses, exact instruction counts)"
scripts/cachegrind.sh > "$R/cachegrind.tsv"
echo "== llvm-mca (static throughput of the AVX2/SSE hot loops)"
scripts/mca.sh > "$R/mca.txt"
for t in tldr cachegrind mca; do python3 scripts/tables.py "$t" "$R" > "$R/table_$t.md"; done
echo "done: $R/"
