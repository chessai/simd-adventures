// bench.cpp -- throughput + hardware-counter benchmark for all filters.
//
// One binary per ISA backend (see CMakeLists.txt): bench_avx2, bench_avx512,
// bench_portable, bench_neon.  Output is CSV (one row per measurement) that
// scripts/make_report.py turns into the tables in REPORT.md.
#include "vf/alloc.h"
#include "vf/blocked_bloom.h"
#include "vf/classic_bloom.h"
#include "vf/cuckoo.h"
#include "vf/keygen.h"
#include "vf/perf.h"
#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <functional>
#include <string>
#include <vector>

using namespace vf;

// ---------------------------------------------------------------------------
// Key streams that don't need the whole key set in memory (a 1 GiB filter at
// 10 bits/key holds 858M keys).
// ---------------------------------------------------------------------------
struct KeyStream {
  std::string dist;
  u64 seed;
  VF_INLINE u64 at(u64 i) const {
    if (dist == "sequential") return seed + i;
    if (dist == "clustered") return (mix(seed ^ (i >> 6)) & ~u64(63)) + (i & 63);
    return mix(seed ^ i);
  }
  static VF_INLINE u64 mix(u64 z) {
    z += 0x9E3779B97F4A7C15ull;
    z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9ull;
    z = (z ^ (z >> 27)) * 0x94D049BB133111EBull;
    return z ^ (z >> 31);
  }
};

struct Opts {
  std::vector<size_t> sizes = { size_t(256) << 10, size_t(16) << 20, size_t(1) << 30 };
  size_t nqueries = size_t(16) << 20;
  std::string dist = "uniform", qdist = "mixed";
  double bpk = 10.0, fill = 1.0;
  size_t window = 32;
  int reps = 3;
  std::string csv, only;
  bool window_sweep = false;
  std::vector<size_t> windows = { 1, 4, 8, 16, 32, 64, 128, 256 };
};

static Opts g;
static PerfGroup g_perf;
static FILE* g_csv = nullptr;

struct Row {
  const char* structure; const char* api; const char* op;
  size_t bytes, nkeys; double bpk; size_t window; size_t nops;
  PerfCounts c; double extra;  // extra = hit rate for lookups, failure count for cuckoo inserts
};

static void emit(const Row& r) {
  double n = (double)r.nops;
  double ipc = std::isnan(r.c.instructions) || r.c.cycles_from_tsc ? NAN : r.c.instructions / r.c.cycles;
  std::printf("%-12s %-7s %-7s %9.1f MiB  n=%-10zu %5.2f bpk  W=%-3zu  %7.2f ns/op %7.1f Mops/s  %7.1f cyc/op%s  IPC %5.2f  L1m %5.2f  LLCm %5.2f  BRm %5.3f  %s=%.4f\n",
              r.structure, r.api, r.op, r.bytes / 1048576.0, r.nkeys, r.bpk, r.window, r.c.ns / n, n / r.c.ns * 1e3,
              r.c.cycles / n, r.c.cycles_from_tsc ? "(tsc)" : "", ipc, r.c.l1d_misses / n, r.c.llc_misses / n,
              r.c.branch_misses / n, std::strcmp(r.op, "lookup") == 0 ? "hit" : "fail", r.extra);
  if (g_csv) {
    std::fprintf(g_csv, "%s,%s,%s,%s,%zu,%zu,%.3f,%s,%s,%zu,%zu,%.4f,%.3f,%.3f,%s,%.4f,%.4f,%.4f,%.5f,%.6f\n",
                 BestOps::name, r.structure, r.api, r.op, r.bytes, r.nkeys, r.bpk, g.dist.c_str(), g.qdist.c_str(), r.window,
                 r.nops, r.c.ns / n, n / r.c.ns * 1e3, r.c.cycles / n, r.c.cycles_from_tsc ? "tsc" : "hw", ipc,
                 r.c.l1d_misses / n, r.c.llc_misses / n, r.c.branch_misses / n, r.extra);
    std::fflush(g_csv);
  }
}

static PerfCounts& acc(PerfCounts& a, const PerfCounts& b) {
  a.ns += b.ns; a.cycles += b.cycles; a.instructions += b.instructions; a.branch_misses += b.branch_misses;
  a.l1d_misses += b.l1d_misses; a.llc_misses += b.llc_misses; a.cycles_from_tsc |= b.cycles_from_tsc;
  return a;
}

// Insert keys [0,n) from the stream in chunks (<=64K keys), alternating per-key and
// batched APIs chunk by chunk so both see the same load-factor progression.
template <class F>
static void timed_inserts(F& f, const KeyStream& ks, size_t n, const char* name, size_t bytes, double bpk,
                          std::function<size_t(F&, const u64*, size_t)> per_key,
                          std::function<size_t(F&, const u64*, size_t)> batch) {
  // Small chunks so the two APIs interleave finely across the load-factor curve
  // (with 1M chunks the batched API got the last, kick-heavy, cuckoo chunk every time).
  const size_t chunk = std::min<size_t>(size_t(1) << 16, std::max<size_t>(1024, n / 32));
  std::vector<u64> buf(chunk);
  PerfCounts total[2];
  size_t failed[2] = { 0, 0 }, nops[2] = { 0, 0 };
  for (auto& t : total) t.cycles = t.instructions = t.branch_misses = t.l1d_misses = t.llc_misses = 0;
  int api = 0;
  for (size_t s = 0; s < n; s += chunk, api ^= (batch ? 1 : 0)) {
    size_t e = std::min(n, s + chunk);
    for (size_t i = s; i < e; i++) buf[i - s] = ks.at(i);
    PerfCounts c = g_perf.measure([&] { failed[api] += api == 0 ? per_key(f, buf.data(), e - s) : batch(f, buf.data(), e - s); });
    acc(total[api], c);
    nops[api] += e - s;
  }
  for (int a = 0; a < 2; a++)
    if (nops[a]) emit(Row{ name, a == 0 ? "per-key" : "batch", "insert", bytes, n, bpk, g.window, nops[a], total[a], (double)failed[a] });
}

template <class F>
static void timed_lookups(const F& f, const std::vector<u64>& q, const char* name, size_t bytes, size_t n, double bpk,
                          std::function<size_t(const F&, const u64*, size_t, u8*)> per_key,
                          std::function<size_t(const F&, const u64*, size_t, u8*)> batch, size_t window) {
  std::vector<u8> out(q.size());
  for (int api = 0; api < 2; api++) {
    if (api == 1 && !batch) break;
    if (g.window_sweep && api == 0) continue;
    PerfCounts best;
    size_t hits = 0;
    for (int r = 0; r < g.reps; r++) {
      PerfCounts c = g_perf.measure([&] { hits = api == 0 ? per_key(f, q.data(), q.size(), out.data()) : batch(f, q.data(), q.size(), out.data()); });
      if (r == 0 || c.ns < best.ns) best = c;
    }
    emit(Row{ name, api == 0 ? "per-key" : "batch", "lookup", bytes, n, bpk, window, q.size(), best, (double)hits / q.size() });
  }
}

static bool want(const char* name) {  // --only accepts a comma-separated list of substrings
  if (g.only.empty()) return true;
  std::string n = name, o = g.only;
  for (size_t s = 0; s <= o.size();) {
    size_t e = o.find(',', s); if (e == std::string::npos) e = o.size();
    if (e > s && n.find(o.substr(s, e - s)) != std::string::npos) return true;
    s = e + 1;
  }
  return false;
}

template <class Ops, int BB, int BPL>
static void bench_blocked(const char* name, size_t bytes, size_t n, const KeyStream& ks, const std::vector<u64>& q) {
  if (!want(name)) return;
  using F = BlockedBloom<Ops, BB, BPL>;
  F f(bytes, g.window);
  double bpk = (double)f.bytes() * 8 / n;
  if (!g.window_sweep)
    timed_inserts<F>(f, ks, n, name, f.bytes(), bpk,
                     [](F& f, const u64* k, size_t m) { for (size_t i = 0; i < m; i++) f.insert(k[i]); return size_t(0); },
                     [](F& f, const u64* k, size_t m) { f.insert_batch(k, m); return size_t(0); });
  else
    f.insert_batch(q.data(), q.size() / 2);  // sweep mode: cheap fill
  auto per_key = [](const F& f, const u64* k, size_t m, u8* out) { size_t h = 0; for (size_t i = 0; i < m; i++) { out[i] = f.contains(k[i]); h += out[i]; } return h; };
  auto batch = [](const F& f, const u64* k, size_t m, u8* out) { f.contains_batch(k, m, out); size_t h = 0; for (size_t i = 0; i < m; i++) h += out[i]; return h; };
  if (g.window_sweep) {
    for (size_t w : g.windows) { f.set_window(w); timed_lookups<F>(f, q, name, f.bytes(), n, bpk, per_key, batch, w); }
  } else {
    timed_lookups<F>(f, q, name, f.bytes(), n, bpk, per_key, batch, g.window);
  }
}

template <class Ops, typename FP>
static void bench_cuckoo(const char* name, size_t bytes, size_t n, const KeyStream& ks, const std::vector<u64>& q) {
  if (!want(name)) return;
  using F = CuckooFilter<Ops, FP>;
  F f(bytes, g.window);
  size_t nk = std::min(n, (size_t)(f.capacity() * 0.95));
  double bpk = (double)f.bytes() * 8 / nk;
  if (!g.window_sweep)
    timed_inserts<F>(f, ks, nk, name, f.bytes(), bpk,
                     [](F& f, const u64* k, size_t m) { size_t fl = 0; for (size_t i = 0; i < m; i++) fl += !f.insert(k[i]); return fl; },
                     [](F& f, const u64* k, size_t m) { return f.insert_batch(k, m); });
  else
    f.insert_batch(q.data(), std::min(q.size() / 2, nk));
  auto per_key = [](const F& f, const u64* k, size_t m, u8* out) { size_t h = 0; for (size_t i = 0; i < m; i++) { out[i] = f.contains(k[i]); h += out[i]; } return h; };
  auto batch = [](const F& f, const u64* k, size_t m, u8* out) { f.contains_batch(k, m, out); size_t h = 0; for (size_t i = 0; i < m; i++) h += out[i]; return h; };
  // q was built by main() so that its "present" keys are among the nk keys actually inserted.
  if (g.window_sweep) {
    for (size_t w : g.windows) { f.set_window(w); timed_lookups<F>(f, q, name, f.bytes(), nk, bpk, per_key, batch, w); }
  } else {
    timed_lookups<F>(f, q, name, f.bytes(), nk, bpk, per_key, batch, g.window);
  }
}

static void bench_classic(size_t bytes, size_t n, const KeyStream& ks, const std::vector<u64>& q) {
  if (!want("classic")) return;
  if (g.window_sweep) return;
  ClassicBloom f(bytes, 7);
  double bpk = (double)f.bytes() * 8 / n;
  timed_inserts<ClassicBloom>(f, ks, n, "classic-k7", f.bytes(), bpk,
                              [](ClassicBloom& f, const u64* k, size_t m) { for (size_t i = 0; i < m; i++) f.insert(k[i]); return size_t(0); }, nullptr);
  timed_lookups<ClassicBloom>(f, q, "classic-k7", f.bytes(), n, bpk,
                              [](const ClassicBloom& f, const u64* k, size_t m, u8* out) { size_t h = 0; for (size_t i = 0; i < m; i++) { out[i] = f.contains(k[i]); h += out[i]; } return h; },
                              nullptr, g.window);
}

// Query stream: "present" keys are stream keys with index < present_limit.
static std::vector<u64> make_query_stream(const KeyStream& ks, size_t present_limit, size_t nq, u64 seed) {
  std::vector<u64> q(nq);
  Rng r(seed);
  if (g.qdist == "absent") {
    for (auto& x : q) x = r.next();
  } else if (g.qdist == "zipf") {
    double lnN = std::log((double)present_limit);
    for (auto& x : q) { size_t rank = (size_t)std::exp(r.unit() * lnN); x = ks.at(std::min(rank, present_limit - 1)); }
  } else {
    for (auto& x : q) x = (r.next() & 1) ? ks.at(r.next() % present_limit) : r.next();
  }
  return q;
}

static std::vector<size_t> parse_sizes(const char* s) {
  std::vector<size_t> v;
  for (const char* p = s; *p;) {
    char* e; double x = std::strtod(p, &e);
    size_t mult = 1;
    if (*e == 'K' || *e == 'k') { mult = 1024; e++; } else if (*e == 'M' || *e == 'm') { mult = 1 << 20; e++; } else if (*e == 'G' || *e == 'g') { mult = 1 << 30; e++; }
    v.push_back((size_t)(x * mult));
    p = *e == ',' ? e + 1 : e;
    if (e == p && *p) break;
  }
  return v;
}

int main(int argc, char** argv) {
  for (int i = 1; i < argc; i++) {
    std::string a = argv[i];
    auto next = [&] { return std::string(i + 1 < argc ? argv[++i] : ""); };
    if (a == "--sizes") g.sizes = parse_sizes(next().c_str());
    else if (a == "--queries") g.nqueries = (size_t)std::atoll(next().c_str());
    else if (a == "--dist") g.dist = next();
    else if (a == "--qdist") g.qdist = next();
    else if (a == "--bpk") g.bpk = std::atof(next().c_str());
    else if (a == "--fill") g.fill = std::atof(next().c_str());
    else if (a == "--window") g.window = (size_t)std::atoll(next().c_str());
    else if (a == "--reps") g.reps = std::atoi(next().c_str());
    else if (a == "--csv") g.csv = next();
    else if (a == "--only") g.only = next();
    else if (a == "--window-sweep") g.window_sweep = true;
    else if (a == "--no-hugepages") AllocPolicy::hugepages() = false;
    else { std::fprintf(stderr, "unknown arg %s\n", a.c_str()); return 2; }
  }
  if (!g.csv.empty()) {
    bool fresh = !fopen(g.csv.c_str(), "r");
    g_csv = fopen(g.csv.c_str(), "a");
    if (fresh) std::fprintf(g_csv, "backend,structure,api,op,bytes,nkeys,bpk,dist,qdist,window,nops,ns_per_op,mops,cycles_per_op,cycles_src,ipc,l1_miss_per_op,llc_miss_per_op,brmiss_per_op,extra\n");
  }
  std::printf("backend=%s  hugepages=%s  perf counters: %s\n", BestOps::name, AllocPolicy::hugepages() ? "madvise" : "off", g_perf.status().c_str());
  for (size_t bytes : g.sizes) {
    size_t n = (size_t)(bytes * 8 / g.bpk * g.fill);
    KeyStream ks{ g.dist, 0x5EED0000 + bytes };
    std::printf("=== %zu bytes (%.1f MiB), %zu keys, dist=%s qdist=%s, %zu queries ===\n", bytes, bytes / 1048576.0, n, g.dist.c_str(), g.qdist.c_str(), g.nqueries);
    auto q = make_query_stream(ks, n, g.nqueries, 0xC0FFEE);
    bench_classic(bytes, n, ks, q);
    bench_blocked<BestOps, 32, 1>("blocked32", bytes, n, ks, q);
    bench_blocked<BestOps, 64, 1>("blocked64", bytes, n, ks, q);
    bench_blocked<BestOps, 64, 2>("blocked64x2", bytes, n, ks, q);
    // Cuckoo filters hold fewer keys at equal memory: use a query stream whose present keys come from the smaller set.
    {
      CuckooFilter<BestOps, u16> tmp16(bytes); CuckooFilter<BestOps, u8> tmp8(bytes);
      size_t n16 = std::min(n, (size_t)(tmp16.capacity() * 0.95)), n8 = std::min(n, (size_t)(tmp8.capacity() * 0.95));
      auto q16 = n16 == n ? q : make_query_stream(ks, n16, g.nqueries, 0xC0FFEE);
      bench_cuckoo<BestOps, u16>("cuckoo16", bytes, n, ks, q16);
      auto q8 = n8 == n ? q : make_query_stream(ks, n8, g.nqueries, 0xC0FFEE);
      bench_cuckoo<BestOps, u8>("cuckoo8", bytes, n, ks, q8);
    }
  }
  if (g_csv) fclose(g_csv);
  return 0;
}
