// Property tests: every ISA backend must agree bit-for-bit with the portable
// (scalar) backend, batched APIs must agree with per-key APIs, and the filters
// must never produce false negatives against a reference model.
#include "vf/blocked_bloom.h"
#include "vf/classic_bloom.h"
#include "vf/cuckoo.h"
#include "vf/hash.h"
#include "vf/keygen.h"
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <unordered_map>
#include <unordered_set>
#include <vector>

using namespace vf;

static int g_fail = 0, g_checks = 0;
#define CHECK(cond, ...) do { g_checks++; if (!(cond)) { g_fail++; std::fprintf(stderr, "FAIL %s:%d: ", __FILE__, __LINE__); std::fprintf(stderr, __VA_ARGS__); std::fprintf(stderr, "\n"); if (g_fail > 20) std::exit(1); } } while (0)

static std::vector<u64> interesting_keys() {
  std::vector<u64> k = { 0, 1, 2, 3, 0xFFFFFFFFull, 0x100000000ull, ~0ull, 0x8000000000000000ull, 0x7FFFFFFFFFFFFFFFull };
  Rng r(42);
  for (int i = 0; i < 1000; i++) k.push_back(r.next());
  for (int i = 0; i < 1000; i++) k.push_back(i);
  for (int i = 0; i < 1000; i++) k.push_back((u64)i << 32);
  return k;
}

template <class Ops>
static void test_hash_agreement() {
  auto keys = interesting_keys();
  while (keys.size() % 8) keys.push_back(keys.back() + 1);
  for (size_t i = 0; i + 8 <= keys.size(); i += 8) {
    typename Ops::v32 hb, hl;
    hash8_keys<Ops>(keys.data() + i, hb, hl);
    alignas(32) u32 b[8], l[8];
    Ops::storeu_32(b, hb);
    Ops::storeu_32(l, hl);
    for (int j = 0; j < 8; j++) {
      u32 sb, sl;
      hash1(keys[i + j], sb, sl);
      CHECK(sb == b[j] && sl == l[j], "%s hash8 != hash1 for key %llx (lane %d): %08x/%08x vs %08x/%08x", Ops::name,
            (unsigned long long)keys[i + j], j, b[j], l[j], sb, sl);
    }
  }
  // mulhi_32 vs fastrange32
  Rng r(7);
  for (int it = 0; it < 200; it++) {
    alignas(32) u32 a[8], n[8], o[8];
    for (int j = 0; j < 8; j++) { a[j] = (u32)r.next(); n[j] = (u32)(r.next() % 50000000 + 1); }
    Ops::storeu_32(o, Ops::mulhi_32(Ops::loadu_32(a), Ops::loadu_32(n)));
    for (int j = 0; j < 8; j++) CHECK(o[j] == fastrange32(a[j], n[j]), "%s mulhi lane %d", Ops::name, j);
  }
}

template <class Ops, int BB, int BPL>
static void test_blocked_bloom(size_t bytes, size_t nops, u64 seed, const char* dist) {
  using F = BlockedBloom<Ops, BB, BPL>;
  using R = BlockedBloom<Portable, BB, BPL>;
  F f(bytes, 8 + (seed % 60));
  R ref(bytes);
  F fb(bytes, 16);  // filled via insert_batch
  std::unordered_set<u64> model;
  auto keys = make_keys(dist, nops, seed);
  Rng r(seed ^ 0xABC);

  // per-key inserts (f, ref) interleaved with queries
  for (size_t i = 0; i < nops; i++) {
    u64 k = keys[i];
    if ((r.next() & 3) != 0) {
      f.insert(k); ref.insert(k); model.insert(k);
    } else {
      u64 q = (r.next() & 1) ? k : r.next();
      bool a = f.contains(q), b = ref.contains(q);
      CHECK(a == b, "%s BB%d/%d contains mismatch vs portable", Ops::name, BB, BPL);
      if (model.count(q)) CHECK(a, "%s BB%d/%d false negative", Ops::name, BB, BPL);
    }
  }
  CHECK(std::memcmp(f.data(), ref.data(), f.bytes()) == 0, "%s BB%d/%d bit array differs from portable", Ops::name, BB, BPL);

  // batched insert of the same model set must give the identical bit array
  std::vector<u64> mk(model.begin(), model.end());
  fb.insert_batch(mk.data(), mk.size());
  CHECK(std::memcmp(fb.data(), ref.data(), fb.bytes()) == 0, "%s BB%d/%d insert_batch array differs", Ops::name, BB, BPL);

  // batched contains == per-key contains, for odd sizes / windows
  for (size_t n : { size_t(0), size_t(1), size_t(7), size_t(8), size_t(9), size_t(31), size_t(33), size_t(257), size_t(1001) }) {
    std::vector<u64> q(n);
    for (auto& x : q) x = (r.next() & 1) && !mk.empty() ? mk[r.next() % mk.size()] : r.next();
    std::vector<u8> out(n + 1, 0xEE), out_ref(n + 1, 0xEE);
    f.contains_batch(q.data(), n, out.data());
    ref.contains_batch(q.data(), n, out_ref.data());
    for (size_t i = 0; i < n; i++) {
      CHECK(out[i] == (u8)f.contains(q[i]), "%s BB%d/%d contains_batch != contains (n=%zu i=%zu)", Ops::name, BB, BPL, n, i);
      CHECK(out[i] == out_ref[i], "%s BB%d/%d contains_batch != portable batch", Ops::name, BB, BPL);
    }
    CHECK(out[n] == 0xEE, "contains_batch wrote past the end");
    CHECK(f.count_batch(q.data(), n) == (size_t)std::count(out.begin(), out.begin() + n, 1), "count_batch mismatch");
  }
}

template <class Ops, typename FP>
static void test_cuckoo(size_t bytes, size_t nops, u64 seed, const char* dist) {
  using C = CuckooFilter<Ops, FP>;
  using R = CuckooFilter<Portable, FP>;
  C c(bytes, 8 + (seed % 60));
  R ref(bytes);
  std::unordered_map<u64, int> model;  // multiset
  auto keys = make_keys(dist, nops, seed);
  Rng r(seed ^ 0xDEF);
  size_t full_rejects = 0;
  for (size_t i = 0; i < nops; i++) {
    u64 k = keys[i];
    u32 op = (u32)(r.next() % 10);
    if (op < 6) {
      bool a = c.insert(k), b = ref.insert(k);
      CHECK(a == b, "%s cuckoo%d insert result differs from portable", Ops::name, (int)sizeof(FP) * 8);
      if (a) model[k]++; else full_rejects++;
    } else if (op < 8) {
      u64 q = (r.next() & 1) ? k : r.next();
      bool a = c.contains(q), b = ref.contains(q);
      CHECK(a == b, "%s cuckoo%d contains differs from portable", Ops::name, (int)sizeof(FP) * 8);
      if (model.count(q)) CHECK(a, "%s cuckoo%d false negative", Ops::name, (int)sizeof(FP) * 8);
    } else {
      // erase only keys that are in the model (deleting a never-inserted key is UB for cuckoo filters)
      if (!model.empty()) {
        auto it = model.begin();
        std::advance(it, r.next() % std::min<size_t>(model.size(), 16));
        u64 q = it->first;
        bool a = c.erase(q), b = ref.erase(q);
        CHECK(a == b, "%s cuckoo%d erase differs from portable", Ops::name, (int)sizeof(FP) * 8);
        CHECK(a, "%s cuckoo%d erase of present key failed", Ops::name, (int)sizeof(FP) * 8);
        if (--it->second == 0) model.erase(it);
        else CHECK(c.contains(q), "%s cuckoo%d remaining copy lost after erase", Ops::name, (int)sizeof(FP) * 8);
      }
    }
  }
  CHECK(c.size() == ref.size(), "cuckoo size mismatch");
  // Batched == per-key
  std::vector<u64> mk;
  for (auto& kv : model) mk.push_back(kv.first);
  for (size_t n : { size_t(0), size_t(1), size_t(7), size_t(8), size_t(9), size_t(31), size_t(33), size_t(257), size_t(1001) }) {
    std::vector<u64> q(n);
    for (auto& x : q) x = (r.next() & 1) && !mk.empty() ? mk[r.next() % mk.size()] : r.next();
    std::vector<u8> out(n + 1, 0xEE), out_ref(n + 1, 0xEE);
    c.contains_batch(q.data(), n, out.data());
    ref.contains_batch(q.data(), n, out_ref.data());
    for (size_t i = 0; i < n; i++) {
      CHECK(out[i] == (u8)c.contains(q[i]), "%s cuckoo%d contains_batch != contains", Ops::name, (int)sizeof(FP) * 8);
      CHECK(out[i] == out_ref[i], "%s cuckoo%d contains_batch != portable batch", Ops::name, (int)sizeof(FP) * 8);
      if (model.count(q[i])) CHECK(out[i] == 1, "%s cuckoo%d batched false negative", Ops::name, (int)sizeof(FP) * 8);
    }
    CHECK(out[n] == 0xEE, "contains_batch wrote past the end");
  }
  // insert_batch must give the same table as per-key inserts (same deterministic kick RNG)
  {
    C c2(bytes), c3(bytes);
    size_t fail2 = c2.insert_batch(mk.data(), mk.size());
    size_t fail3 = 0;
    for (u64 k : mk) fail3 += !c3.insert(k);
    CHECK(fail2 == fail3, "insert_batch failure count differs");
    for (u64 k : mk) CHECK(c2.contains(k), "insert_batch false negative");
  }
  (void)full_rejects;
}

static void test_classic_bloom() {
  ClassicBloom b(1 << 16);
  std::unordered_set<u64> model;
  Rng r(99);
  for (int i = 0; i < 20000; i++) { u64 k = r.next(); b.insert(k); model.insert(k); }
  for (u64 k : model) CHECK(b.contains(k), "classic bloom false negative");
  int fp = 0;
  for (int i = 0; i < 100000; i++) fp += b.contains(r.next());
  CHECK(fp < 100000 * 0.05, "classic bloom FPR implausibly high: %d/100000", fp);
}

template <class Ops>
static void test_fpr_sanity() {
  // ~10 bits/key, 1M keys: blocked32 should be ~1-2%, cuckoo16 ~0.01%, cuckoo8 ~3%
  const size_t n = (size_t)((1 << 20) * 0.95);  // 95% of a 2^20-slot cuckoo table
  auto keys = make_keys("uniform", n, 5);
  BlockedBloom<Ops, 32> b32(n * 10 / 8);
  BlockedBloom<Ops, 64> b64(n * 10 / 8);
  b32.insert_batch(keys.data(), n);
  b64.insert_batch(keys.data(), n);
  CuckooFilter<Ops, u16> c16(size_t(2) << 20);  // 2^18 buckets x 4 slots
  CuckooFilter<Ops, u8> c8(size_t(1) << 20);
  size_t f16 = c16.insert_batch(keys.data(), n), f8 = c8.insert_batch(keys.data(), n);
  CHECK(f16 == 0, "cuckoo16 rejected %zu inserts at load %.3f", f16, c16.load());
  CHECK(f8 == 0, "cuckoo8 rejected %zu inserts at load %.3f", f8, c8.load());
  auto q = make_queries("absent", keys, n, 77);
  double r32 = (double)b32.count_batch(q.data(), n) / n;
  double r64 = (double)b64.count_batch(q.data(), n) / n;
  double r16 = (double)c16.count_batch(q.data(), n) / n;
  double r8 = (double)c8.count_batch(q.data(), n) / n;
  std::printf("  [%s] FPR @ ~10 bpk: blocked32=%.4f blocked64=%.4f  cuckoo16(load %.2f)=%.5f cuckoo8(load %.2f)=%.4f\n",
              Ops::name, r32, r64, c16.load(), r16, c8.load(), r8);
  CHECK(r32 > 0.002 && r32 < 0.03, "blocked32 FPR out of range %.4f", r32);
  CHECK(r64 > 0.002 && r64 < 0.03, "blocked64 FPR out of range %.4f", r64);
  CHECK(r16 < 0.001, "cuckoo16 FPR out of range %.5f", r16);
  CHECK(r8 > 0.01 && r8 < 0.06, "cuckoo8 FPR out of range %.4f", r8);
  for (size_t i = 0; i < n; i += 97) {
    CHECK(b32.contains(keys[i]) && b64.contains(keys[i]) && c16.contains(keys[i]) && c8.contains(keys[i]), "false negative in fpr test");
  }
}

template <class Ops>
static void run_backend() {
  std::printf("backend %s\n", Ops::name);
  test_hash_agreement<Ops>();
  const char* dists[] = { "uniform", "sequential", "clustered" };
  u64 seed = 1;
  for (const char* d : dists) {
    for (size_t bytes : { size_t(32), size_t(64), size_t(100), size_t(4096), size_t(1 << 16) }) {
      size_t nops = bytes < 128 ? 200 : bytes <= 4096 ? 3000 : 60000;
      test_blocked_bloom<Ops, 32, 1>(bytes, nops, seed++, d);
      test_blocked_bloom<Ops, 64, 1>(bytes, nops, seed++, d);
      test_blocked_bloom<Ops, 32, 2>(bytes, nops, seed++, d);
      test_blocked_bloom<Ops, 64, 2>(bytes, nops, seed++, d);
      test_cuckoo<Ops, u16>(bytes, nops, seed++, d);
      test_cuckoo<Ops, u8>(bytes, nops, seed++, d);
    }
  }
  test_fpr_sanity<Ops>();
}

int main(int argc, char** argv) {
  int rounds = argc > 1 ? std::atoi(argv[1]) : 1;
  for (int r = 0; r < rounds; r++) {
    run_backend<Portable>();
#if VF_HAVE_AVX2
    run_backend<Avx2>();
#endif
#if VF_HAVE_AVX512
    run_backend<Avx512>();
#endif
#if VF_HAVE_NEON
    run_backend<Neon>();
#endif
  }
  test_classic_bloom();
  std::printf("%d checks, %d failures\n", g_checks, g_fail);
  return g_fail ? 1 : 0;
}
