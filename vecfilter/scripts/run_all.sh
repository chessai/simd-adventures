#!/usr/bin/env bash
# Full benchmark matrix for the Rust port (same runs as cpp/scripts/run_all.sh).
# Usage: scripts/run_all.sh [results-dir]    then: python3 scripts/make_report.py results
set -euo pipefail
cd "$(dirname "$0")/.."
R=${1:-results}
mkdir -p "$R"
# Every backend for this architecture; the bench picks one with --backend (best by default).
cargo build --release -p vecfilter -p simdlab
# Scalar baseline with auto-vectorisation off, like the C++ portable build (-fno-tree-vectorize).
# RUSTFLAGS replaces the config's target-cpu=native; the portable rows use --backend portable.
RUSTFLAGS="-C no-vectorize-loops -C no-vectorize-slp" cargo build --release -p vecfilter --target-dir ../target/novec
B=../target/release/vecfilter_bench
P=../target/novec/release/vecfilter_bench
F=../target/release/vecfilter_fpr
M="../target/release/simdlab membench"
Q=16777216
run() { echo "### $*"; "$@"; }
# Machine and backends, for the record (the backend table also tells us whether AVX-512 runs here).
../target/release/simdlab cpu > "$R/cpu.txt"
$B --list-backends > "$R/backends.txt"
has_avx512=false
grep -Eq "^[ *] avx512 +yes" "$R/backends.txt" && has_avx512=true
# 1. Main matrix on the AVX2 backend (the common x86 case), uniform keys, mixed queries.
run $B --backend avx2 --sizes 256K,1M,16M,1G --queries $Q --reps 3 --dist uniform --qdist mixed --csv $R/main.csv
# 2. Query distribution: all-absent and Zipf-skewed, lookups only (inserts are the same).
for qd in absent zipf; do
  run $B --backend avx2 --sizes 1M,16M --queries $Q --reps 3 --dist uniform --qdist $qd --csv $R/main.csv
done
# 3. Key distribution: sequential and clustered keys.
for d in sequential clustered; do
  run $B --backend avx2 --sizes 1M,16M --queries $Q --reps 3 --dist $d --qdist mixed --csv $R/main.csv
done
# 4. Scalar/portable and AVX-512 backends, same main matrix.
run $P --backend portable --sizes 256K,1M,16M,1G --queries $Q --reps 3 --only blocked32,blocked64,cuckoo16 --csv $R/main.csv
$has_avx512 && run $B --backend avx512 --sizes 256K,1M,16M,1G --queries $Q --reps 3 --only blocked32,blocked64,cuckoo16 --csv $R/main.csv
# 5. Prefetch window sweep.
run $B --backend avx2 --window-sweep --sizes 1M,16M,1G --queries $Q --reps 3 --only blocked32,cuckoo16 --csv $R/window_sweep.csv
# 6. Huge pages off, to show the TLB share of the memory-bound cost.
run $B --backend avx2 --no-hugepages --sizes 1G --queries $Q --reps 3 --only blocked32,cuckoo16,classic --csv $R/nohuge.csv
# 7. FPR / bits-per-key at equal memory (best backend; the output is backend-independent).
for d in uniform sequential clustered; do run $F $d; done > $R/fpr.txt
# 8. Memory roofline.
run $M 1024 > $R/membench.txt
run $M 1024 --no-hugepages > $R/membench_nohuge.txt
echo "done"
