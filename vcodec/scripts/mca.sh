#!/usr/bin/env bash
# Static throughput analysis of the hot loops with llvm-mca.
# Builds the vcodec library with `# LLVM-MCA-BEGIN/END` markers inside each
# kernel's loop body, extracts the marked regions from the generated assembly,
# and runs llvm-mca on each with the given CPU model (default: skylake-avx512
# -- pass e.g. `-mcpu=znver3` or `-mcpu=haswell`).
set -euo pipefail
cd "$(dirname "$0")/.."
MCPU=${1:--mcpu=skylake-avx512}
# absolute: cargo runs rustc from the workspace root
OUT=$(cd .. && pwd)/target/mca
mkdir -p "$OUT"
# no debuginfo: llvm-mca rejects the .loc directives the workspace profile (debug = 1) emits
CARGO_PROFILE_RELEASE_DEBUG=0 RUSTFLAGS="-C target-cpu=x86-64-v3 --emit=asm -C codegen-units=1 -C lto=off" \
  cargo rustc --release -p vcodec --features mca-markers --lib -- -o "$OUT/vcodec.s" >/dev/null 2>&1 || true
ASM=$(ls -t "$OUT"/vcodec*.s 2>/dev/null | head -1)
if [ -z "$ASM" ]; then
  ASM=$(ls -t ../target/release/deps/vcodec-*.s | head -1)
fi
echo "asm: $ASM"
for region in hex_encode hex_decode b64_encode b64_decode url_encode url_decode; do
  # The generic kernel is instantiated once per backend; every instance carries the
  # same marker name, so we split occurrences and keep the AVX2 one (uses ymm).
  python3 - "$ASM" "$region" "$OUT" <<'PY'
import sys,re
asm,region,out=sys.argv[1:4]
lines=open(asm).read().split('\n')
blocks=[];cur=None
for l in lines:
    if f'LLVM-MCA-BEGIN {region}' in l: cur=[]
    elif f'LLVM-MCA-END {region}' in l:
        if cur is not None: blocks.append(cur); cur=None
    elif cur is not None: cur.append(l)
for i,b in enumerate(blocks):
    txt='\n'.join(b)
    # The Portable instance is auto-vectorised scalar code (huge); real
    # intrinsic instances are compact. Classify by register width.
    isa='avx2' if 'ymm' in txt else ('sse' if 'xmm' in txt else 'portable')
    if len(b) > 400: isa='portable'
    open(f'{out}/{region}.{isa}.{i}.s','w').write(f'# LLVM-MCA-BEGIN {region}\n{txt}\n# LLVM-MCA-END\n')
print(f'{region}: {len(blocks)} instances')
PY
done
echo
echo "== llvm-mca ($MCPU) =="
for region in hex_encode hex_decode b64_encode b64_decode url_encode url_decode; do
  for isa in avx2 sse; do
    best=$(ls -Sr "$OUT"/$region.$isa.*.s 2>/dev/null | head -1)
    [ -n "$best" ] || continue
    echo "--- $region [$isa] ($best)"
    llvm-mca $MCPU -iterations=100 "$best" 2>/dev/null | grep -E "^(Iterations|Instructions|Total Cycles|Total uOps|Dispatch Width|uOps Per Cycle|IPC|Block RThroughput)" | sed 's/^/    /'
  done
done
