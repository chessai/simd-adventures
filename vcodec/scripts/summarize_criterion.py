#!/usr/bin/env python3
"""Collect criterion estimates into one TSV + markdown tables.

usage: scripts/summarize_criterion.py [../target/criterion] > results/criterion.md
"""
import json, os, sys, re
root = sys.argv[1] if len(sys.argv) > 1 else "../target/criterion"
rows = []
for group in sorted(os.listdir(root)):
    gdir = os.path.join(root, group)
    if not os.path.isdir(gdir) or group == "report":
        continue
    for impl in sorted(os.listdir(gdir)):
        idir = os.path.join(gdir, impl)
        if not os.path.isdir(idir) or impl == "report":
            continue
        for case in sorted(os.listdir(idir)):
            est = os.path.join(idir, case, "new", "estimates.json")
            bench = os.path.join(idir, case, "new", "benchmark.json")
            if not os.path.exists(est):
                continue
            e = json.load(open(est)); b = json.load(open(bench))
            ns = e["mean"]["point_estimate"]  # ns per iteration
            nbytes = b["throughput"]["Bytes"]
            size, dist = case.split("_", 1) if "_" in case else case.split("/", 1)
            rows.append((group, impl, size, dist, nbytes, ns, nbytes / ns))

def size_key(s):
    m = re.match(r"(\d+)(B|KiB|MiB)", s)
    return int(m.group(1)) * {"B": 1, "KiB": 1024, "MiB": 1 << 20}[m.group(2)]

print("group\timpl\tsize\tdist\tbytes\tns\tGB/s")
for r in rows:
    print("\t".join(str(x) if not isinstance(x, float) else f"{x:.4g}" for x in r))

print("\n\n# Markdown tables (GB/s, higher is better; speedup vs vcodec-scalar in parentheses)\n")
groups = sorted({r[0] for r in rows})
for g in groups:
    grs = [r for r in rows if r[0] == g]
    impls = sorted({r[1] for r in grs}, key=lambda s: (not s.startswith("vcodec"), s))
    cases = sorted({(r[2], r[3]) for r in grs}, key=lambda c: (size_key(c[0]), c[1]))
    print(f"\n## {g}\n")
    print("| size/dist | " + " | ".join(impls) + " |")
    print("|---|" + "---|" * len(impls))
    for (sz, d) in cases:
        base = next((r[6] for r in grs if r[1] == "vcodec-scalar" and (r[2], r[3]) == (sz, d)), None)
        cells = []
        for im in impls:
            v = next((r[6] for r in grs if r[1] == im and (r[2], r[3]) == (sz, d)), None)
            if v is None:
                cells.append("–")
            elif base:
                cells.append(f"{v:.2f} ({v / base:.1f}×)")
            else:
                cells.append(f"{v:.2f}")
        print(f"| {sz} {d} | " + " | ".join(cells) + " |")
