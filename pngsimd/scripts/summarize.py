#!/usr/bin/env python3
"""Turn results/*.tsv into the markdown tables used in REPORT.md."""
import csv, os, sys, collections, statistics

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
R = os.path.join(ROOT, sys.argv[1] if len(sys.argv) > 1 else 'results')

def read_tsv(name):
    with open(os.path.join(R, name)) as f:
        rows = [l for l in f if not l.startswith('#') and l.strip()]
    return list(csv.DictReader(rows, delimiter='\t'))

def table(headers, rows):
    out = ['| ' + ' | '.join(headers) + ' |', '|' + '|'.join('---' for _ in headers) + '|']
    for r in rows: out.append('| ' + ' | '.join(str(x) for x in r) + ' |')
    return '\n'.join(out)

# ---------------------------------------------------------------- kernels
# core cycles = TSC ticks * core_per_tsc (bench/corefreq.c); the VM exposes no PMU
CORE_PER_TSC = 1.0
try:
    import re
    CORE_PER_TSC = float(re.search(r'core_per_tsc=([0-9.]+)', open(os.path.join(R, 'corefreq.txt')).read()).group(1))
except Exception:
    pass
K = read_tsv('bench_kernels.tsv')
for r in K:
    r['bytes'] = int(r['bytes']); r['cpb'] = float(r['cycles_per_byte']) * CORE_PER_TSC; r['ns'] = float(r['ns_per_call'])
backends = [b for b in ['scalar', 'portable', 'sse41', 'avx2', 'avx512', 'neon'] if any(r['backend'] == b for r in K)]

def cpb(backend, kernel, variant, dist, size):
    for r in K:
        if (r['backend'], r['kernel'], r['variant'], r['dist'], r['bytes']) == (backend, kernel, variant, dist, size): return r['cpb']
    return None

print(f'## Kernel cycles per byte (32 KiB buffers, "photo" byte distribution; core cycles = TSC ticks x {CORE_PER_TSC:.2f}, see bench/corefreq.c)\n')
print('Speedup is scalar / backend. "scalar-naive" is the textbook row[i] += row[i-bpp] loop; "scalar" is the register-carried baseline used for all speedups.\n')
hdr = ['kernel'] + [f'{b} c/B' for b in backends] + [f'{b} ×' for b in backends if b != 'scalar']
rows = []
for f in ['sub', 'up', 'avg', 'paeth']:
    for bpp in [1, 2, 3, 4, 6, 8]:
        v = f'{f}/{bpp}'
        c = {b: cpb(b, 'filter', v, 'photo', 32768) for b in backends}
        naive = cpb('scalar', 'filter_naive', v, 'photo', 32768)
        row = [v] + [f'{c[b]:.2f}' if c[b] else '-' for b in backends] + [f'{c["scalar"]/c[b]:.1f}' if c[b] else '-' for b in backends if b != 'scalar']
        rows.append(row)
        if f in ('sub', 'avg', 'paeth') and naive:
            rows.append([f'{v} scalar-naive', f'{naive:.2f}'] + ['' for _ in backends[1:]] + ['' for _ in backends[1:]])
        if f == 'paeth' and bpp in (1, 3, 4):
            bl = cpb('scalar', 'paeth_branchless', v, 'photo', 32768)
            if bl: rows.append([f'{v} scalar-branchless', f'{bl:.2f}'] + ['' for _ in backends[1:]] + [f'{bl/c[b]:.1f}' if c[b] else '-' for b in backends if b != 'scalar'])
for kern in ['crc32', 'adler32', 'rgb_to_rgba', 'u16_to_u8', 'memcpy']:
    c = {b: cpb(b, kern, '-' if kern != 'memcpy' else 'restore', 'photo', 32768) for b in backends}
    rows.append([kern if kern != 'memcpy' else 'memcpy (row restore, included in every filter row above)'] + [f'{c[b]:.3f}' if c[b] else '-' for b in backends] + [f'{c["scalar"]/c[b]:.1f}' if c[b] else '-' for b in backends if b != 'scalar'])
w = cpb('avx2', 'sub4wide', 'sub/4', 'photo', 32768)
if w: rows.append(['sub/4 AVX2 256-bit scan (experiment)', '', '', '', f'{w:.2f}', '', '', '', '', ''][:len(hdr)])
print(table(hdr, rows))

print('\n## Buffer-size sweep (photo distribution, cycles per byte)\n')
sizes = [1024, 32768, 262144, 8388608]
hdr = ['kernel', 'backend'] + ['1 KiB', '32 KiB', '256 KiB', '8 MiB']
rows = []
for kern, var in [('filter', 'up/4'), ('filter', 'sub/4'), ('filter', 'paeth/4'), ('crc32', '-'), ('adler32', '-'), ('rgb_to_rgba', '-'), ('u16_to_u8', '-')]:
    for b in backends:
        if b == 'portable': continue
        vals = [cpb(b, kern, var, 'photo', s) for s in sizes]
        rows.append([var if kern == 'filter' else kern, b] + [f'{v:.3f}' if v else '-' for v in vals])
print(table(hdr, rows))

print('\n## Data-distribution effect (32 KiB, cycles per byte)\n')
hdr = ['kernel', 'backend', 'uniform', 'photo (small residuals)', 'zeros']
rows = []
for kern, var in [('filter', 'paeth/4'), ('filter', 'paeth/3'), ('filter', 'avg/4'), ('paeth_branchless', 'paeth/4'), ('filter', 'sub/1'), ('crc32', '-'), ('adler32', '-')]:
    for b in (['scalar', 'sse41'] if kern != 'paeth_branchless' else ['scalar']):
        vals = [cpb(b, kern, var, d, 32768) for d in ['uniform', 'photo', 'zeros']]
        rows.append([(var if kern == 'filter' else kern + ('' if kern != 'paeth_branchless' else '/4')), b] + [f'{v:.2f}' if v else '-' for v in vals])
print(table(hdr, rows))

# ---------------------------------------------------------------- decode
D = read_tsv('bench_decode.tsv')
for r in D:
    for k in ['total_ns', 'parse_crc_ns', 'inflate_ns', 'adler_ns', 'unfilter_ns', 'expand_ns', 'out_bytes', 'file_bytes']: r[k] = int(r[k])
variants = []
for r in D:
    if r['variant'] not in variants: variants.append(r['variant'])
files = []
for r in D:
    if r['file'] not in files: files.append(r['file'])

def agg(subset_files, variant):
    rs = [r for r in D if r['variant'] == variant and r['file'] in subset_files]
    t = sum(r['total_ns'] for r in rs)
    st = [sum(r[k] for r in rs) for k in ['parse_crc_ns', 'inflate_ns', 'adler_ns', 'unfilter_ns', 'expand_ns']]
    out = sum(r['out_bytes'] for r in rs)
    return t, st, out

print('\n## Whole-decoder: corpus totals (sum of per-file best-of-N wall-clock times)\n')
hdr = ['variant', 'total ms', 'pixel MB/s', 'speedup vs scalar', 'speedup vs libpng', 'parse+crc %', 'inflate %', 'adler %', 'unfilter %', 'expand %', 'other %']
rows = []
t_scalar = agg(files, 'scalar')[0]; t_libpng = agg(files, 'libpng')[0] if 'libpng' in variants else t_scalar
for v in variants:
    t, st, out = agg(files, v)
    pct = [f'{s/t*100:.1f}' for s in st] if sum(st) else ['-'] * 5
    other = f'{(t-sum(st))/t*100:.1f}' if sum(st) else '-'
    rows.append([v, f'{t/1e6:.1f}', f'{out/t*1e3:.0f}', f'{t_scalar/t:.2f}', f'{t_libpng/t:.2f}'] + pct + [other])
print(table(hdr, rows))

groups = collections.OrderedDict([
    ('RGB8 photos, adaptive filters (9 files)', [f for f in files if f.startswith('rgb8_') and '_pil' not in f and not any(f.endswith(f'_{p}.png') for p in ['none', 'sub', 'up', 'avg', 'paeth']) and '_2x' not in f and '_4x' not in f]),
    ('RGB8 photos, upscaled 2x/4x (4 files, 1.4-6.5 MB raw)', [f for f in files if '_2x' in f or '_4x' in f]),
    ('RGB8 Pillow-encoded, levels 1/6/9 (9 files)', [f for f in files if '_pil' in f]),
    ('gray8 photos (10 files)', [f for f in files if f.startswith('gray8_') and not any(f.endswith(f'_{p}.png') for p in ['none', 'sub', 'up', 'avg', 'paeth'])]),
    ('RGBA8 (7 files)', [f for f in files if f.startswith('rgba8_')]),
    ('16-bit (6 files)', [f for f in files if '16_' in f]),
    ('palette / sub-byte (5 files)', [f for f in files if f.startswith('pal') or f.startswith('gray1') or f.startswith('gray4')]),
])
for pol in ['none', 'sub', 'up', 'avg', 'paeth']:
    groups[f'hubble RGB8 + moon gray8, all rows filter={pol}'] = [f for f in files if f.endswith(f'_{pol}.png') and ('hubble' in f or 'moon' in f)]

print('\n## Whole-decoder by image class (ms totals; speedups relative to scalar backend)\n')
ref_names = [v for v in ('libpng', 'spng', 'png-crate') if v in variants]
vs = [v for v in variants if v in ('scalar', 'sse41', 'avx2', 'avx512') or v in ref_names]
hdr = ['image class'] + [f'{v} ms' for v in vs] + ['best SIMD ×scalar'] + [f'best SIMD ×{r}' for r in ref_names] + ['unfilter % scalar→SIMD', 'inflate % scalar→SIMD']
rows = []
for name, fs in groups.items():
    if not fs: continue
    ts = {v: agg(fs, v) for v in vs}
    best = min((ts[v][0], v) for v in vs if v in ('sse41', 'avx2', 'avx512'))
    sc = ts['scalar']; bs = ts[best[1]]
    rows.append([f'{name}'] + [f'{ts[v][0]/1e6:.1f}' for v in vs] +
                [f'{sc[0]/best[0]:.2f} ({best[1]})'] + [f'{ts[r][0]/best[0]:.2f}' for r in ref_names] +
                [f'{sc[1][3]/sc[0]*100:.0f}→{bs[1][3]/bs[0]*100:.0f}', f'{sc[1][1]/sc[0]*100:.0f}→{bs[1][1]/bs[0]*100:.0f}'])
print(table(hdr, rows))

print('\n## Per-stage speedups measured inside the decoder (corpus totals, scalar backend ÷ best SIMD backend)\n')
best_v = min((agg(files, v)[0], v) for v in vs if v in ('sse41', 'avx2', 'avx512'))[1]
sc = agg(files, 'scalar'); bs = agg(files, best_v)
hdr = ['stage', 'scalar ms', f'{best_v} ms', 'speedup', 'share of scalar total', f'share of {best_v} total']
rows = []
for i, nm in enumerate(['parse+crc32', 'inflate (scalar in both)', 'adler32', 'unfilter', 'expand']):
    rows.append([nm, f'{sc[1][i]/1e6:.1f}', f'{bs[1][i]/1e6:.1f}', f'{sc[1][i]/max(1,bs[1][i]):.1f}', f'{sc[1][i]/sc[0]*100:.1f}%', f'{bs[1][i]/bs[0]*100:.1f}%'])
rows.append(['total', f'{sc[0]/1e6:.1f}', f'{bs[0]/1e6:.1f}', f'{sc[0]/bs[0]:.2f}', '100%', '100%'])
print(table(hdr, rows))

# ---------------------------------------------------------------- cachegrind
if os.path.exists(os.path.join(R, 'cachegrind.tsv')):
    C = read_tsv('cachegrind.tsv')
    print('\n## Instruction and miss counts (cachegrind simulation: 48 KiB L1D, LL = L3; 64 KiB buffers, photo distribution)\n')
    print(f'IPC = instructions per core cycle, using the 32 KiB-buffer cycles/byte from the microbenchmark (same build, core cycles = TSC x {CORE_PER_TSC:.2f}). For the filter rows the row-restore memcpy (valgrind substitutes a 1-instruction-per-byte memcpy, 0.05 TSC-cycles/byte natively) has been subtracted from both instructions and cycles.\n')
    hdr = ['kernel', 'backend', 'instr/byte', 'cycles/byte', 'IPC', 'D1 miss/KB', 'LL miss/KB', 'branches/KB', 'mispredict/KB', 'mispredict rate']
    rows = []
    mc = {r['backend']: r for r in C if r['kernel'] == 'memcpy'}
    for r in C:
        kern = r['variant'] if r['kernel'] in ('filter',) else (r['kernel'] + ('/4' if r['kernel'] == 'paeth_branchless' else ''))
        if r['kernel'] == 'memcpy': continue
        c = cpb(r['backend'], r['kernel'], r['variant'], 'photo', 32768)
        ipb = float(r['instr_per_byte'])
        if r['kernel'] in ('filter', 'paeth_branchless') and r['backend'] in mc:
            ipb -= float(mc[r['backend']]['instr_per_byte'])
            if c: c -= cpb(r['backend'], 'memcpy', 'restore', 'photo', 32768)
        rows.append([kern, r['backend'], f'{ipb:.3f}', f'{c:.3f}' if c else '-', f'{ipb/c:.2f}' if c else '-', f'{float(r["D1_miss_per_KB"]):.2f}', f'{float(r["LL_miss_per_KB"]):.3f}', f'{float(r["branches_per_KB"]):.0f}', f'{float(r["br_mispred_per_KB"]):.2f}', f'{float(r["mispred_rate_%"]):.1f}%'])
    print(table(hdr, rows))
