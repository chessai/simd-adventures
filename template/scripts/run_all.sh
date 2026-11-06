#!/usr/bin/env bash
# Full benchmark run for this experiment. Usage: scripts/run_all.sh [results-dir]
set -euo pipefail
cd "$(dirname "$0")/.."
R=${1:-results}
mkdir -p "$R"
cargo build --release -p template -p simdlab
../target/release/simdlab cpu > "$R/cpu.txt"
../target/release/template_bench --list-backends > "$R/backends.txt"
../target/release/template_bench --sizes 1k,32k,1M,64M > "$R/bench.tsv"
echo "done: $R/"
