#!/usr/bin/env bash
# Simulated micro-architectural counters with valgrind/cachegrind.
#
# Hardware PMUs are frequently unavailable (containers, Firecracker/most VMs).
# Cachegrind simulates an I1/D1/LL cache hierarchy and a branch predictor, so it
# gives *deterministic* instruction counts, L1/LL miss counts and branch
# mispredictions per kernel. They are a model, not the real chip — but the
# ratios between backends are meaningful, and instruction counts are exact.
#
# Method: run vcodec_bench for N and 3N iterations of the same kernel and take the
# difference, which cancels data generation / process start-up.
#
#   scripts/cachegrind.sh [size] [iters]  >  results/cachegrind.tsv
#
# Runs every backend this CPU supports.
set -euo pipefail
cd "$(dirname "$0")/.."
SIZE=${1:-65536}
N=${2:-16}
BIN=../target/release/vcodec_bench
cargo build --release -p vcodec --bin vcodec_bench >/dev/null 2>&1
TMP=$(mktemp -d)
# backends this CPU supports, from the --list-backends table ("[*] name yes ...")
BACKENDS=$("$BIN" --list-backends | awk '{ n = $1; s = $2 } $1 == "*" { n = $2; s = $3 } s == "yes" { print n }')

run() { # codec op backend dist iters -> prints "Ir D1mr DLmr D1mw DLmw Bcm Bim"
  valgrind --tool=cachegrind --cache-sim=yes --branch-sim=yes \
    --I1=32768,8,64 --D1=32768,8,64 --LL=16777216,16,64 \
    --cachegrind-out-file="$TMP/cg.out" \
    "$BIN" --codec "$1" --op "$2" --backend "$3" --dist "$4" --size "$SIZE" --iters "$5" --repeat 1 \
    >/dev/null 2>"$TMP/cg.err"
  # summary line: "summary: Ir I1mr ILmr Dr D1mr DLmr Dw D1mw DLmw Bc Bcm Bi Bim"
  grep '^summary:' "$TMP/cg.out" | awk '{print $2, $6, $7, $9, $10, $12, $14}'
}

echo -e "codec\top\tbackend\tdist\tsize\tinstr/byte\tD1-miss/KiB\tLL-miss/KiB\tbr-mispred/KiB\tind-mispred/KiB"
for codec in hex base64 url; do
  if [ "$codec" = url ]; then dists="safe text pct10 binary"; else dists="random"; fi
  for op in encode decode; do
    for backend in $BACKENDS; do
      for dist in $dists; do
        a=$(run $codec $op $backend $dist $N 2>/dev/null) || continue
        b=$(run $codec $op $backend $dist $((3 * N)) 2>/dev/null) || continue
        python3 - "$codec" "$op" "$backend" "$dist" "$SIZE" "$N" "$a" "$b" <<'PY'
import sys
codec,op,backend,dist,size,n,a,b=sys.argv[1:9]
a=list(map(int,a.split())); b=list(map(int,b.split()))
d=[y-x for x,y in zip(a,b)]
bytes_=2*int(n)*int(size); kib=bytes_/1024
ir,d1mr,dlmr,d1mw,dlmw,bcm,bim=d
print(f"{codec}\t{op}\t{backend}\t{dist}\t{size}\t{ir/bytes_:.3f}\t{(d1mr+d1mw)/kib:.3f}\t{(dlmr+dlmw)/kib:.3f}\t{bcm/kib:.3f}\t{bim/kib:.3f}")
PY
      done
    done
  done
done
rm -rf "$TMP"
