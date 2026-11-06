// libFuzzer harness: interprets the input as a program of insert/erase/query
// operations over a *tiny* key space (so fingerprint and block collisions are
// frequent), runs it against every compiled-in backend plus the scalar model,
// and aborts on any divergence or false negative.
//
//   clang++ -std=c++17 -O1 -g -fsanitize=fuzzer,address,undefined -mavx2 -Iinclude tests/fuzz_ops.cpp -o fuzz_ops
//
// Without libFuzzer, build with -DVF_FUZZ_MAIN to get a driver that feeds
// random inputs through the same entry point (used by `ctest`).
#include "vf/blocked_bloom.h"
#include "vf/cuckoo.h"
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <unordered_map>
#include <unordered_set>
#include <vector>

using namespace vf;

namespace {

struct Reader {
  const uint8_t* p; size_t n, i = 0;
  uint8_t byte() { return i < n ? p[i++] : 0; }
  uint64_t key() {  // small key space, two "kinds" to mix small ints and wide values
    uint8_t b = byte();
    return (b & 0x80) ? (uint64_t(b & 0x7F) << 32) | byte() : uint64_t(b);
  }
};

#define FUZZ_ASSERT(c, msg) do { if (!(c)) { std::fprintf(stderr, "fuzz failure: %s\n", msg); std::abort(); } } while (0)

template <class Ops>
void run_bloom(const uint8_t* data, size_t size) {
  Reader rd{ data, size };
  size_t bytes = 32 + (rd.byte() % 8) * 32;
  size_t window = 1 + rd.byte() % 40;
  BlockedBloom<Ops, 32> a(bytes, window);
  BlockedBloom<Portable, 32> ra(bytes);
  BlockedBloom<Ops, 64, 2> b(bytes, window);
  BlockedBloom<Portable, 64, 2> rb(bytes);
  std::unordered_set<uint64_t> model;
  std::vector<uint64_t> pending;
  while (rd.i < rd.n) {
    uint8_t op = rd.byte() % 4;
    uint64_t k = rd.key();
    if (op == 0) { a.insert(k); ra.insert(k); b.insert(k); rb.insert(k); model.insert(k); }
    else if (op == 1) { pending.push_back(k); }
    else if (op == 2) {
      a.insert_batch(pending.data(), pending.size()); ra.insert_batch(pending.data(), pending.size());
      b.insert_batch(pending.data(), pending.size()); rb.insert_batch(pending.data(), pending.size());
      for (auto x : pending) model.insert(x);
      pending.clear();
    } else {
      bool x = a.contains(k), y = ra.contains(k), z = b.contains(k), w = rb.contains(k);
      FUZZ_ASSERT(x == y && z == w, "bloom backend divergence");
      if (model.count(k)) FUZZ_ASSERT(x && z, "bloom false negative");
    }
  }
  FUZZ_ASSERT(std::memcmp(a.data(), ra.data(), a.bytes()) == 0, "bloom32 array divergence");
  FUZZ_ASSERT(std::memcmp(b.data(), rb.data(), b.bytes()) == 0, "bloom64 array divergence");
  std::vector<uint64_t> q(model.begin(), model.end());
  for (int i = 0; i < 50; i++) q.push_back(i * 7919);
  std::vector<uint8_t> o1(q.size()), o2(q.size());
  a.contains_batch(q.data(), q.size(), o1.data());
  ra.contains_batch(q.data(), q.size(), o2.data());
  for (size_t i = 0; i < q.size(); i++) {
    FUZZ_ASSERT(o1[i] == o2[i] && o1[i] == (uint8_t)a.contains(q[i]), "bloom batch divergence");
    if (model.count(q[i])) FUZZ_ASSERT(o1[i], "bloom batch false negative");
  }
}

template <class Ops, typename FP>
void run_cuckoo(const uint8_t* data, size_t size) {
  Reader rd{ data, size };
  size_t bytes = 16 + (rd.byte() % 8) * 16;
  size_t window = 1 + rd.byte() % 40;
  CuckooFilter<Ops, FP> c(bytes, window);
  CuckooFilter<Portable, FP> r(bytes);
  std::unordered_map<uint64_t, int> model;
  std::vector<uint64_t> live;
  while (rd.i < rd.n) {
    uint8_t op = rd.byte() % 4;
    uint64_t k = rd.key();
    if (op == 0) {
      bool x = c.insert(k), y = r.insert(k);
      FUZZ_ASSERT(x == y, "cuckoo insert divergence");
      if (x) { model[k]++; live.push_back(k); }
    } else if (op == 1) {
      if (!live.empty()) {
        size_t j = (size_t)k % live.size();
        uint64_t e = live[j];
        bool x = c.erase(e), y = r.erase(e);
        FUZZ_ASSERT(x == y, "cuckoo erase divergence");
        FUZZ_ASSERT(x, "cuckoo erase of present key failed");
        live[j] = live.back(); live.pop_back();
        if (--model[e] == 0) model.erase(e);
      }
    } else {
      bool x = c.contains(k), y = r.contains(k);
      FUZZ_ASSERT(x == y, "cuckoo contains divergence");
      if (model.count(k)) FUZZ_ASSERT(x, "cuckoo false negative");
    }
  }
  std::vector<uint64_t> q(live.begin(), live.end());
  for (int i = 0; i < 50; i++) q.push_back(i * 104729);
  std::vector<uint8_t> o1(q.size()), o2(q.size());
  c.contains_batch(q.data(), q.size(), o1.data());
  r.contains_batch(q.data(), q.size(), o2.data());
  for (size_t i = 0; i < q.size(); i++) {
    FUZZ_ASSERT(o1[i] == o2[i] && o1[i] == (uint8_t)c.contains(q[i]), "cuckoo batch divergence");
    if (model.count(q[i])) FUZZ_ASSERT(o1[i], "cuckoo batch false negative");
  }
}

void run_all(const uint8_t* data, size_t size) {
  run_bloom<BestOps>(data, size);
  run_cuckoo<BestOps, uint16_t>(data, size);
  run_cuckoo<BestOps, uint8_t>(data, size);
}

}  // namespace

extern "C" int LLVMFuzzerTestOneInput(const uint8_t* data, size_t size) {
  run_all(data, size);
  return 0;
}

#ifdef VF_FUZZ_MAIN
#include "vf/keygen.h"
int main(int argc, char** argv) {
  long iters = argc > 1 ? std::atol(argv[1]) : 20000;
  Rng rng(0xF00D);
  std::vector<uint8_t> buf;
  for (long it = 0; it < iters; it++) {
    size_t len = (size_t)(rng.next() % 2048);
    buf.resize(len);
    for (auto& b : buf) b = (uint8_t)rng.next();
    // bias towards tiny key spaces for some runs so collisions are dense
    if (rng.next() & 1) for (auto& b : buf) b &= 0x0F;
    run_all(buf.data(), buf.size());
  }
  std::printf("fuzz driver: %ld random programs on backend %s, no divergence\n", iters, BestOps::name);
  return 0;
}
#endif
