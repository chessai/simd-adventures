#!/usr/bin/env python3
"""Side-by-side table of the C and Rust kernel microbenchmarks (same machine, same session),
plus the whole-decoder corpus totals. Usage: compare_c_rust.py c/results results (the C original, when present, lives in c/)"""
import csv, os, re, sys

def read_tsv(path):
    with open(path) as f:
        rows = [l for l in f if not l.startswith('#') and l.strip()]
    return list(csv.DictReader(rows, delimiter='\t'))

def core_per_tsc(d):
    try: return float(re.search(r'core_per_tsc=([0-9.]+)', open(os.path.join(d, 'corefreq.txt')).read()).group(1))
    except Exception: return 1.0

cdir, rdir = sys.argv[1], sys.argv[2]
C = read_tsv(os.path.join(cdir, 'bench_kernels.tsv')); R = read_tsv(os.path.join(rdir, 'bench_kernels.tsv'))
cf, rf = core_per_tsc(cdir), core_per_tsc(rdir)

def cpb(rows, f, b, k, v, d='photo', s=32768):
    for r in rows:
        if (r['backend'], r['kernel'], r['variant'], r['dist'], int(r['bytes'])) == (b, k, v, d, s): return float(r['cycles_per_byte']) * f
    return None

def table(headers, rows):
    out = ['| ' + ' | '.join(headers) + ' |', '|' + '|'.join('---' for _ in headers) + '|']
    for r in rows: out.append('| ' + ' | '.join(str(x) for x in r) + ' |')
    return '\n'.join(out)

print(f'## C (gcc 13 -O2) vs Rust (rustc 1.97 -O, fat LTO): core cycles per byte, 32 KiB buffers, photo distribution (core/TSC: C run {cf:.2f}, Rust run {rf:.2f})\n')
backends = ['scalar', 'sse41', 'avx2', 'avx512', 'portable']
hdr = ['kernel'] + [f'{b} C / Rust' for b in backends]
rows = []
def fmt(a, b):
    if a is None or b is None: return '-'
    return f'{a:.2f} / {b:.2f}'
for f in ['sub', 'up', 'avg', 'paeth']:
    for bpp in [1, 3, 4, 8]:
        v = f'{f}/{bpp}'
        rows.append([v] + [fmt(cpb(C, cf, b, 'filter', v), cpb(R, rf, b, 'filter', v)) for b in backends])
rows.append(['paeth/4 scalar-branchless'] + [fmt(cpb(C, cf, 'scalar', 'paeth_branchless', 'paeth/4'), cpb(R, rf, 'scalar', 'paeth_branchless', 'paeth/4'))] + ['-'] * 4)
rows.append(['paeth/4 scalar-naive'] + [fmt(cpb(C, cf, 'scalar', 'filter_naive', 'paeth/4'), cpb(R, rf, 'scalar', 'filter_naive', 'paeth/4'))] + ['-'] * 4)
for k in ['crc32', 'crc32_bytewise', 'adler32', 'rgb_to_rgba', 'u16_to_u8']:
    rows.append([k] + [fmt(cpb(C, cf, b, k, '-'), cpb(R, rf, b, k, '-')) for b in backends])
rows.append(['memcpy (row restore)'] + [fmt(cpb(C, cf, b, 'memcpy', 'restore'), cpb(R, rf, b, 'memcpy', 'restore')) for b in backends])
print(table(hdr, rows))

def totals(d):
    rows = read_tsv(os.path.join(d, 'bench_decode.tsv'))
    t = {}
    for r in rows:
        v = r['variant']; t.setdefault(v, [0, [0] * 5])
        t[v][0] += int(r['total_ns'])
        for i, k in enumerate(['parse_crc_ns', 'inflate_ns', 'adler_ns', 'unfilter_ns', 'expand_ns']): t[v][1][i] += int(r[k])
    return t
tc, tr = totals(cdir), totals(rdir)
print('\n## Whole-decoder corpus totals, same session (ms; stage shares in %)\n')
hdr = ['variant', 'C total', 'C inflate', 'C unfilter', 'Rust total', 'Rust inflate', 'Rust unfilter']
rows = []
for v in ['scalar', 'sse41', 'avx2', 'avx512', 'scalar+zlib', 'sse41+zlib', 'sse41+fdeflate', 'libpng', 'spng', 'png-crate']:
    c = tc.get(v); r = tr.get(v)
    def cell(x, i):
        if x is None: return '-'
        return f'{x[1][i]/1e6:.0f} ({x[1][i]/x[0]*100:.0f}%)' if x[1][1] else '-'
    rows.append([v, f'{c[0]/1e6:.0f}' if c else '-', cell(c, 1), cell(c, 3), f'{r[0]/1e6:.0f}' if r else '-', cell(r, 1), cell(r, 3)])
print(table(hdr, rows))
