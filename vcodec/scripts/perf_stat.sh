#!/usr/bin/env bash
# Real hardware counters, two ways. Run on bare metal (or a VM with a vPMU).
#
#  1) vcodec_bench's built-in perf_event_open counters (per-thread, user-space only,
#     works with perf_event_paranoid=2):
#        scripts/perf_stat.sh matrix [sizes]   > results/bench.tsv
#  2) `perf stat` around one kernel, for cross-checking (1):
#        scripts/perf_stat.sh one base64 decode avx2 65536 random
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release -p vcodec --bin vcodec_bench >/dev/null 2>&1
BIN=../target/release/vcodec_bench
case "${1:-matrix}" in
  matrix)
    "$BIN" --sizes "${2:-64,1k,64k,4M}" ;;
  one)
    shift
    perf stat -e cycles:u,instructions:u,branches:u,branch-misses:u,L1-dcache-load-misses:u,LLC-load-misses:u,LLC-loads:u \
      "$BIN" --codec "$1" --op "$2" --backend "$3" --size "$4" --dist "$5" --repeat 1 ;;
  *) echo "usage: $0 matrix [sizes] | one codec op backend size dist"; exit 1 ;;
esac
