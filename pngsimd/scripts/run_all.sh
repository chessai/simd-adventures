#!/bin/sh
# Full reproduction for the Rust port: build, test, benchmark. Results land in results/.
set -e
cd "$(dirname "$0")/.."
mkdir -p results
cargo build --release --features zlib,pngcrate,libpng,fdeflate
cargo build --release -p simdlab
echo "== tests"
cargo test --release 2>&1 | grep -E "^test |test result" | tee results/tests.txt
echo "== machine"
{ grep -m1 'model name' /proc/cpuinfo; nproc; lscpu | grep -E 'cache'; rustc --version; uname -r; } > results/machine.txt
cat results/machine.txt
../target/release/simdlab cpu > results/cpu.txt
../target/release/pngsimd_bench_kernels --list-backends > results/backends.txt
../target/release/simdlab corefreq | tee results/corefreq.txt
echo "== kernel microbenchmarks (~12 minutes)"
../target/release/pngsimd_bench_kernels --sizes 1k,32k,256k,8m --dists uniform,photo,zeros --secs 0.2 > results/bench_kernels.tsv
echo "== whole-decoder benchmark on the corpus"
../target/release/pngsimd_bench_decode corpus --reps-ms 300 --zlib --fdeflate > results/bench_decode.tsv
echo "== cachegrind (instruction / miss / mispredict counts)"
python3 scripts/run_cachegrind.py scalar,sse41,avx2 > results/cachegrind.log
echo "== summary tables"
python3 scripts/summarize.py > results/summary.md
python3 scripts/insert_summary.py
[ -f c/results/bench_kernels.tsv ] && python3 scripts/compare_c_rust.py c/results results > results/compare_c_rust.md
echo done
