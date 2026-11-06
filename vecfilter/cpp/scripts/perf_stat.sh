#!/usr/bin/env bash
# Whole-process counters via `perf stat`, for machines where perf_event_open is
# available to perf but you want an independent cross-check of the in-process
# PerfGroup numbers.  Usage: scripts/perf_stat.sh build/bench_avx2 --sizes 1G --only blocked32
set -euo pipefail
exec perf stat -e cycles,instructions,branch-misses,L1-dcache-load-misses,LLC-load-misses,dTLB-load-misses -- "$@"
