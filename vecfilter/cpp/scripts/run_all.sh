#!/usr/bin/env bash
# Runs the full benchmark matrix.  Usage: scripts/run_all.sh <build-dir> <results-dir>
set -euo pipefail
B=${1:-build}; R=${2:-results}
mkdir -p "$R"
Q=16777216
run() { echo "### $*"; "$@"; }
best=avx2
[ -x "$B/bench_avx512" ] && best=avx512
# 1. Main matrix on the AVX2 backend (the common x86 case), uniform keys, mixed queries.
run $B/bench_avx2 --sizes 256K,1M,16M,1G --queries $Q --reps 3 --dist uniform --qdist mixed --csv $R/main.csv
# 2. Query distribution: all-absent and Zipf-skewed, lookups only (inserts are the same).
for qd in absent zipf; do
  run $B/bench_avx2 --sizes 1M,16M --queries $Q --reps 3 --dist uniform --qdist $qd --csv $R/main.csv
done
# 3. Key distribution: sequential and clustered keys.
for d in sequential clustered; do
  run $B/bench_avx2 --sizes 1M,16M --queries $Q --reps 3 --dist $d --qdist mixed --csv $R/main.csv
done
# 4. Scalar/portable and AVX-512 backends, same main matrix.
run $B/bench_portable --sizes 256K,1M,16M,1G --queries $Q --reps 3 --only blocked32,blocked64,cuckoo16 --csv $R/main.csv
[ -x "$B/bench_avx512" ] && run $B/bench_avx512 --sizes 256K,1M,16M,1G --queries $Q --reps 3 --only blocked32,blocked64,cuckoo16 --csv $R/main.csv
# 5. Prefetch window sweep.
run $B/bench_avx2 --window-sweep --sizes 1M,16M,1G --queries $Q --reps 3 --only blocked32,cuckoo16 --csv $R/window_sweep.csv
# 6. Huge pages off, to show the TLB share of the memory-bound cost.
run $B/bench_avx2 --no-hugepages --sizes 1G --queries $Q --reps 3 --only blocked32,cuckoo16,classic --csv $R/nohuge.csv
# 7. FPR / bits-per-key at equal memory.
for d in uniform sequential clustered; do run $B/fpr_$best $d; done > $R/fpr.txt
# 8. Memory roofline.
run $B/membench 1024 > $R/membench.txt
run $B/membench 1024 --no-hugepages > $R/membench_nohuge.txt
echo "done"
