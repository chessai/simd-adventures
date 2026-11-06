#!/usr/bin/env python3
"""Instruction counts, simulated L1D/LL misses and branch mispredicts per kernel, via
valgrind --tool=cachegrind. Used because this VM exposes no hardware PMU (perf_event_open
returns ENOENT); on a machine with a PMU bench_kernels reports real counters directly.

Each configuration is run in its own process (pngsimd_bench_kernels once ...) so the process
totals are attributable to one kernel; the fixed setup cost is measured with a 0-rep run
and subtracted. Output: results/cachegrind.tsv

Valgrind 3.22 does not implement AVX-512, so the avx512 backend is skipped here.
"""
import os, subprocess, sys, re, csv

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BENCH = os.path.join(ROOT, '..', 'target', 'release', 'pngsimd_bench_kernels')  # workspace target dir
OUT = os.path.join(ROOT, 'results', 'cachegrind.tsv')
SCRATCH = os.environ.get('CG_SCRATCH', '/tmp/pngsimd-cg')
os.makedirs(SCRATCH, exist_ok=True)
os.makedirs(os.path.dirname(OUT), exist_ok=True)

BYTES = 65536
REPS = int(os.environ.get('CG_REPS', '256'))        # 16 MiB of kernel work per configuration
backends = sys.argv[1].split(',') if len(sys.argv) > 1 else ['scalar', 'sse41', 'avx2']
dist = os.environ.get('CG_DIST', 'photo')
configs = []
for f in ['sub', 'up', 'avg', 'paeth']:
    for bpp in [1, 3, 4]:
        configs.append(('filter', f'{f}/{bpp}'))
configs += [('paeth_branchless', 'paeth/4'), ('crc32', '-'), ('adler32', '-'), ('rgb_to_rgba', '-'), ('u16_to_u8', '-'), ('memcpy', 'restore')]

EVENTS = ['Ir', 'I1mr', 'ILmr', 'Dr', 'D1mr', 'DLmr', 'Dw', 'D1mw', 'DLmw', 'Bc', 'Bcm', 'Bi', 'Bim']

def run(backend, kernel, variant, reps):
    cgf = os.path.join(SCRATCH, f'cg.{backend}.{kernel}.{variant.replace("/", "_")}.{reps}')
    cmd = ['valgrind', '--tool=cachegrind', '--cache-sim=yes', '--branch-sim=yes', '--I1=32768,8,64', '--D1=49152,12,64', '--LL=2097152,16,64',
           f'--cachegrind-out-file={cgf}', '-q', BENCH, 'once', '--backend', backend, '--kernel', kernel,
           '--variant', variant, '--dist', dist, '--bytes', str(BYTES), '--reps', str(reps)]
    subprocess.run(cmd, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    events = None; summary = None
    with open(cgf) as fh:
        for line in fh:
            if line.startswith('events:'): events = line.split()[1:]
            if line.startswith('summary:'): summary = [int(x) for x in line.split()[1:]]
    return dict(zip(events, summary))

rows = []
for b in backends:
    if b not in ('scalar', 'portable', 'sse41', 'avx2', 'neon'):
        print(f'skipping {b} (not supported under valgrind)'); continue
    for kernel, variant in configs:
        if kernel == 'paeth_branchless' and b != 'scalar': continue
        base = run(b, kernel, variant, 0)
        full = run(b, kernel, variant, REPS)
        d = {e: full[e] - base[e] for e in EVENTS}
        n = BYTES * REPS
        row = {'backend': b, 'kernel': kernel, 'variant': variant, 'dist': dist, 'bytes_total': n,
               'instr_per_byte': d['Ir'] / n, 'D1_miss_per_KB': (d['D1mr'] + d['D1mw']) * 1024 / n,
               'LL_miss_per_KB': (d['DLmr'] + d['DLmw']) * 1024 / n, 'branches_per_KB': d['Bc'] * 1024 / n,
               'br_mispred_per_KB': (d['Bcm'] + d['Bim']) * 1024 / n, 'mispred_rate_%': 100 * (d['Bcm'] + d['Bim']) / max(1, d['Bc'] + d['Bi'])}
        rows.append(row)
        print(f"{b:8s} {kernel:16s} {variant:8s} Ir/B={row['instr_per_byte']:.3f} D1miss/KB={row['D1_miss_per_KB']:.2f} LLmiss/KB={row['LL_miss_per_KB']:.3f} brmiss/KB={row['br_mispred_per_KB']:.2f} ({row['mispred_rate_%']:.1f}%)", flush=True)

with open(OUT, 'w', newline='') as fh:
    w = csv.DictWriter(fh, fieldnames=list(rows[0].keys()), delimiter='\t')
    w.writeheader(); w.writerows(rows)
print('wrote', OUT)
