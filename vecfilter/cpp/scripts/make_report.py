#!/usr/bin/env python3
"""Turn results/*.csv into markdown tables (results/tables.md)."""
import csv, sys, os, collections

R = sys.argv[1] if len(sys.argv) > 1 else "results"

def load(name):
    p = os.path.join(R, name)
    if not os.path.exists(p):
        return []
    with open(p) as f:
        rows = list(csv.DictReader(f))
    for r in rows:
        for k in ("bytes", "nkeys", "window", "nops"):
            r[k] = int(r[k])
        for k in ("bpk", "ns_per_op", "mops", "cycles_per_op", "ipc", "l1_miss_per_op", "llc_miss_per_op", "brmiss_per_op", "extra"):
            r[k] = float(r[k])
    return rows

def mib(b):
    return f"{b/1048576:g} MiB" if b >= 1048576 else f"{b/1024:g} KiB"

def table(headers, rows):
    out = ["| " + " | ".join(headers) + " |", "|" + "|".join("---" for _ in headers) + "|"]
    for r in rows:
        out.append("| " + " | ".join(str(x) for x in r) + " |")
    return "\n".join(out) + "\n"

main = load("main.csv")
sweep = load("window_sweep.csv")
nohuge = load("nohuge.csv")
out = []

def sel(rows, **kw):
    return [r for r in rows if all(r[k] == v for k, v in kw.items())]

structs = ["classic-k7", "blocked32", "blocked64", "blocked64x2", "cuckoo16", "cuckoo8"]
sizes = sorted(set(r["bytes"] for r in main))

# 1. Lookup ns/op by size, AVX2, uniform/mixed
for op in ("lookup", "insert"):
    out.append(f"### {op}: ns per operation (lower is better), AVX2 backend, uniform keys, mixed 50/50 queries\n")
    hdr = ["structure", "API"] + [mib(s) for s in sizes]
    rows = []
    for s in structs:
        for api in ("per-key", "batch"):
            cells = []
            for b in sizes:
                m = sel(main, backend="avx2", structure=s, api=api, op=op, bytes=b, dist="uniform", qdist="mixed")
                cells.append(f"{m[0]['ns_per_op']:.2f}" if m else "–")
            if any(c != "–" for c in cells):
                rows.append([s, api] + cells)
    out.append(table(hdr, rows))

# 2. Backends
out.append("### Backend comparison: batched lookup ns/op (uniform keys, mixed queries)\n")
backends = [b for b in ("portable", "avx2", "avx512", "neon") if sel(main, backend=b)]
hdr = ["structure", "API", "size"] + backends
rows = []
for s in structs:
    for api in ("per-key", "batch"):
        for b in sizes:
            cells = []
            for be in backends:
                m = sel(main, backend=be, structure=s, api=api, op="lookup", bytes=b, dist="uniform", qdist="mixed")
                cells.append(f"{m[0]['ns_per_op']:.2f}" if m else "–")
            if any(c != "–" for c in cells):
                rows.append([s, api, mib(b)] + cells)
out.append(table(hdr, rows))

# 3. Query distribution
out.append("### Query distribution: batched lookup ns/op, AVX2 (uniform keys)\n")
qds = ["absent", "mixed", "zipf"]
hdr = ["structure", "API", "size"] + [f"{q} (hit rate)" for q in qds]
rows = []
for s in structs:
    for api in ("per-key", "batch"):
        for b in sizes:
            cells = []
            for q in qds:
                m = sel(main, backend="avx2", structure=s, api=api, op="lookup", bytes=b, dist="uniform", qdist=q)
                cells.append(f"{m[0]['ns_per_op']:.2f} ({m[0]['extra']:.2f})" if m else "–")
            if sum(c != "–" for c in cells) >= 2:
                rows.append([s, api, mib(b)] + cells)
out.append(table(hdr, rows))

# 4. Key distribution
out.append("### Key distribution: lookup ns/op and hit rate, AVX2 (mixed queries)\n")
ds = ["uniform", "sequential", "clustered"]
hdr = ["structure", "API", "size"] + [f"{d} (hit rate)" for d in ds]
rows = []
for s in structs:
    for api in ("per-key", "batch"):
        for b in sizes:
            cells = []
            for d in ds:
                m = sel(main, backend="avx2", structure=s, api=api, op="lookup", bytes=b, dist=d, qdist="mixed")
                cells.append(f"{m[0]['ns_per_op']:.2f} ({m[0]['extra']:.3f})" if m else "–")
            if sum(c != "–" for c in cells) >= 2:
                rows.append([s, api, mib(b)] + cells)
out.append(table(hdr, rows))

# 5. Window sweep
if sweep:
    out.append("### Prefetch window sweep: batched lookup ns/op vs window size W (keys hashed+prefetched one window ahead)\n")
    ws = sorted(set(r["window"] for r in sweep))
    hdr = ["structure", "size"] + [f"W={w}" for w in ws]
    rows = []
    for s in ("blocked32", "cuckoo16"):
        for b in sorted(set(r["bytes"] for r in sweep)):
            cells = []
            for w in ws:
                m = sel(sweep, structure=s, api="batch", op="lookup", bytes=b, window=w)
                cells.append(f"{m[0]['ns_per_op']:.2f}" if m else "–")
            rows.append([s, mib(b)] + cells)
    out.append(table(hdr, rows))

# 6. Hugepages
if nohuge:
    out.append("### Transparent huge pages on/off at 1 GiB (AVX2, uniform, mixed): ns/op\n")
    hdr = ["structure", "API", "op", "THP (madvise)", "4 KiB pages", "ratio"]
    rows = []
    for s in ("classic-k7", "blocked32", "cuckoo16"):
        for api in ("per-key", "batch"):
            for op in ("lookup", "insert"):
                a = sel(main, backend="avx2", structure=s, api=api, op=op, bytes=1 << 30, dist="uniform", qdist="mixed")
                n = sel(nohuge, structure=s, api=api, op=op, bytes=1 << 30)
                if a and n:
                    rows.append([s, api, op, f"{a[0]['ns_per_op']:.2f}", f"{n[0]['ns_per_op']:.2f}", f"{n[0]['ns_per_op']/a[0]['ns_per_op']:.2f}x"])
    out.append(table(hdr, rows))

# 7. Counters (if any were available)
hw = [r for r in main if r["cycles_src"] == "hw"]
if hw:
    out.append("### Hardware counters per operation (AVX2, uniform, mixed)\n")
    hdr = ["structure", "API", "op", "size", "cycles", "IPC", "L1d miss", "LLC miss", "branch miss"]
    rows = [[r["structure"], r["api"], r["op"], mib(r["bytes"]), f"{r['cycles_per_op']:.1f}", f"{r['ipc']:.2f}", f"{r['l1_miss_per_op']:.2f}", f"{r['llc_miss_per_op']:.2f}", f"{r['brmiss_per_op']:.3f}"]
            for r in hw if r["backend"] == "avx2" and r["dist"] == "uniform" and r["qdist"] == "mixed"]
    out.append(table(hdr, rows))
else:
    out.append("_Hardware counters were not available in the environment where these numbers were collected (perf_event_open returned ENOENT inside the KVM guest); cycles/op in the CSV are TSC ticks at the nominal 2.1 GHz._\n")

for name in ("fpr.txt", "membench.txt", "membench_nohuge.txt"):
    p = os.path.join(R, name)
    if os.path.exists(p):
        out.append(f"### {name}\n\n```\n{open(p).read()}```\n")

open(os.path.join(R, "tables.md"), "w").write("\n".join(out))
print("\n".join(out))
