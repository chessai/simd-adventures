#!/usr/bin/env python3
"""Render a results directory into the markdown tables used by REPORT.md.

usage: scripts/tables.py {tldr|cachegrind|mca} [results-dir]   (default: results)

Reads criterion.md (bench.md in the archived results/xeon-vm), cachegrind.tsv and mca.txt.
Backend names: sse41 (sse4.1 / sse in the archived results).
"""
import csv, os, sys, re

DIR = sys.argv[2] if len(sys.argv) > 2 else "results"

def path(*names):
    for n in names:
        p = os.path.join(DIR, n)
        if os.path.exists(p):
            return p
    sys.exit(f"none of {names} in {DIR}")

def read_tsv(p):
    with open(p) as f:
        return list(csv.DictReader(f, delimiter="\t"))

def bench_rows():
    rows = []
    for l in open(path("criterion.md", "bench.md")):
        if l.startswith("\n") or l.startswith("#"): break
        parts = l.rstrip("\n").split("\t")
        if parts[0] == "group": continue
        rows.append(parts)
    return rows  # group impl size dist bytes ns GB/s

def tldr():
    rows = bench_rows()
    def get(group, impls, size, dist):
        for impl in impls:
            for r in rows:
                if r[0] == group and r[1] == impl and r[2] == size and r[3] == dist:
                    return float(r[6])
        return None
    ext = {"hex_encode": ["hex-crate", "hex-simd"], "hex_decode": ["hex-crate", "hex-simd"],
           "base64_encode": ["base64-crate", "base64-simd", "openssl-EVP_EncodeBlock"],
           "base64_decode": ["base64-crate", "base64-simd", "openssl-EVP_DecodeBlock"],
           "url_encode": ["percent-encoding"], "url_decode": ["percent-encoding"]}
    out = []
    for group in ["hex_encode", "hex_decode", "base64_encode", "base64_decode", "url_encode", "url_decode"]:
        codec, op = group.split("_")
        dists = ["random"] if codec != "url" else ["safe", "text", "binary"]
        for d in dists:
            sc = get(group, ["vcodec-scalar"], "64KiB", d)
            ss = get(group, ["vcodec-sse41", "vcodec-sse4.1"], "64KiB", d)
            av = get(group, ["vcodec-avx2"], "64KiB", d)
            best = max(ss, av)
            bext = max((v, e) for e in ext[group] if (v := get(group, [e], "64KiB", d)) is not None)
            out.append(f"| {codec} ({d}) | {op} | {sc:.2f} | {ss:.1f} | {av:.1f} | **{best/sc:.1f}×** | {best/bext[0]:.2f}× vs `{bext[1]}` ({bext[0]:.1f}) |")
    return "\n".join(out)

def cachegrind():
    rows = read_tsv(path("cachegrind.tsv"))
    hdr = "| codec | op | dist | backend | instr/byte | D1 miss/KiB | LL miss/KiB | branch mispred/KiB |\n|---|---|---|---|---|---|---|---|"
    lines = [hdr]
    order = {"scalar": 0, "portable": 1, "sse": 2, "sse41": 2, "avx2": 3, "neon": 4}
    rows.sort(key=lambda r: (["hex", "base64", "url"].index(r["codec"]), r["op"], r["dist"], order.get(r["backend"], 9)))
    for r in rows:
        if float(r['instr/byte']) == 0.0:  # backend not supported on that CPU (archived results)
            continue
        lines.append(f"| {r['codec']} | {r['op']} | {r['dist']} | {r['backend']} | {float(r['instr/byte']):.2f} | {float(r['D1-miss/KiB']):.1f} | {float(r['LL-miss/KiB']):.2f} | {float(r['br-mispred/KiB']):.1f} |")
    return "\n".join(lines)

def mca():
    txt = open(path("mca.txt")).read()
    lines = ["| kernel | ISA | instructions / block | block bytes (in) | RThroughput (cycles/block) | bytes/cycle | mca IPC |", "|---|---|---|---|---|---|---|"]
    blk = {"hex_encode": (16, 32), "hex_decode": (32, 64), "b64_encode": (12, 24), "b64_decode": (16, 32), "url_encode": (16, 32), "url_decode": (16, 32)}
    for m in re.finditer(r"--- (\w+) \[(\w+)\].*?\n(.*?)(?=\n---|\Z)", txt, re.S):
        name, isa, body = m.groups()
        ins = int(re.search(r"Instructions:\s+(\d+)", body).group(1)) // 100
        rt = float(re.search(r"Block RThroughput:\s+([\d.]+)", body).group(1))
        ipc = float(re.search(r"IPC:\s+([\d.]+)", body).group(1))
        b = blk[name][0 if isa == "sse" else 1]
        note = " (escape path)" if name.startswith("url") else ""
        lines.append(f"| {name}{note} | {isa} | {ins} | {b} | {rt} | {b/rt:.1f} | {ipc} |")
    return "\n".join(lines)

if __name__ == "__main__":
    print(globals()[sys.argv[1]]())
