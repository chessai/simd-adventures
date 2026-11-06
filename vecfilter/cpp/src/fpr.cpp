// fpr.cpp -- false-positive rate vs bits/key, Bloom variants vs cuckoo at
// EQUAL MEMORY.  The cuckoo table is fixed at 2 MiB (power-of-two buckets);
// for each bits/key target the key count n is chosen so every structure gets
// exactly 2 MiB, and the cuckoo load factor follows from that (n/a if > 95%).
#include "vf/blocked_bloom.h"
#include "vf/classic_bloom.h"
#include "vf/cuckoo.h"
#include "vf/keygen.h"
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <string>
#include <vector>

using namespace vf;

int main(int argc, char** argv) {
  std::string dist = argc > 1 ? argv[1] : "uniform";
  const size_t M = size_t(2) << 20;
  const size_t nq = size_t(4) << 20;
  const double bpks[] = { 8.5, 10, 12, 14, 17, 20, 24, 32 };
  std::printf("dist=%s  memory=%zu bytes for every structure, %zu absent queries\n", dist.c_str(), M, nq);
  std::printf("%6s %10s | %10s %10s %10s %10s | %16s %16s\n", "bpk", "n keys", "classic", "blocked32", "blocked64", "blk64x2", "cuckoo8(load)", "cuckoo16(load)");
  std::printf("%6s %10s | %10s %10s %10s %10s | %16s %16s\n", "", "", "k=opt", "k=8", "k=8", "k=16", "FPR", "FPR");
  for (double bpk : bpks) {
    size_t n = (size_t)(M * 8 / bpk);
    auto keys = make_keys(dist, n, 1234);
    auto q = make_queries("absent", keys, nq, 999);
    int kopt = std::max(1, (int)std::lround(bpk * std::log(2.0)));
    ClassicBloom cb(M, kopt);
    BlockedBloom<BestOps, 32> b32(M);
    BlockedBloom<BestOps, 64> b64(M);
    BlockedBloom<BestOps, 64, 2> b64x2(M);
    CuckooFilter<BestOps, u8> c8(M);
    CuckooFilter<BestOps, u16> c16(M);
    for (u64 k : keys) cb.insert(k);
    b32.insert_batch(keys.data(), n);
    b64.insert_batch(keys.data(), n);
    b64x2.insert_batch(keys.data(), n);
    double l8 = (double)n / c8.capacity(), l16 = (double)n / c16.capacity();
    size_t f8 = l8 <= 0.95 ? c8.insert_batch(keys.data(), n) : 1, f16 = l16 <= 0.95 ? c16.insert_batch(keys.data(), n) : 1;
    size_t fcb = 0;
    for (u64 k : q) fcb += cb.contains(k);
    double r32 = (double)b32.count_batch(q.data(), nq) / nq, r64 = (double)b64.count_batch(q.data(), nq) / nq;
    double r64x2 = (double)b64x2.count_batch(q.data(), nq) / nq;
    double r8 = (double)c8.count_batch(q.data(), nq) / nq, r16 = (double)c16.count_batch(q.data(), nq) / nq;
    char s8[32], s16[32];
    if (f8 == 0) std::snprintf(s8, sizeof s8, "%.5f (%.2f)", r8, l8); else std::snprintf(s8, sizeof s8, "n/a (%.2f)", l8);
    if (f16 == 0) std::snprintf(s16, sizeof s16, "%.6f (%.2f)", r16, l16); else std::snprintf(s16, sizeof s16, "n/a (%.2f)", l16);
    std::printf("%6.1f %10zu | %10.5f %10.5f %10.5f %10.5f | %16s %16s   (classic k=%d)\n", bpk, n, (double)fcb / nq, r32, r64, r64x2, s8, s16, kopt);
  }
  return 0;
}
