// membench.cpp -- memory-system roofline for random 64-byte accesses, so the
// filter numbers can be placed against what the machine can physically do.
//   latency : single dependent pointer chase (1 access in flight)
//   mlp=K   : K independent chases interleaved (K accesses in flight)
// For a filter whose lookups touch one random line each, the best possible
// batched/pipelined rate is ~ the mlp=K ceiling; the worst is the latency.
#include "vf/alloc.h"
#include "vf/keygen.h"
#include "vf/perf.h"
#include <algorithm>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <numeric>
#include <string>
#include <vector>

using namespace vf;

static std::vector<u32> make_cycle(size_t nlines, u64 seed) {
  // Random cyclic permutation over cache lines (Sattolo's algorithm).
  std::vector<u32> next(nlines);
  std::iota(next.begin(), next.end(), 0u);
  Rng r(seed);
  for (size_t i = nlines - 1; i > 0; i--) std::swap(next[i], next[r.next() % i]);
  return next;
}

template <int K>
static double chase(const u32* next, size_t nlines, size_t steps, u64* sink) {
  // next[] is stored one u32 per 64-byte line (stride 16 u32) so each hop is a new line.
  u32 p[K];
  for (int k = 0; k < K; k++) p[k] = (u32)((u64)k * 7919 % nlines);
  auto t0 = std::chrono::steady_clock::now();
  for (size_t s = 0; s < steps; s++)
    for (int k = 0; k < K; k++) p[k] = next[(size_t)p[k] * 16];
  auto t1 = std::chrono::steady_clock::now();
  u64 acc = 0;
  for (int k = 0; k < K; k++) acc += p[k];
  *sink += acc;
  return std::chrono::duration<double, std::nano>(t1 - t0).count() / (double)(steps * K);
}

int main(int argc, char** argv) {
  size_t max_mb = argc > 1 ? std::atol(argv[1]) : 2048;
  if (argc > 2 && std::string(argv[2]) == "--no-hugepages") AllocPolicy::hugepages() = false;
  std::printf("hugepages=%s\n", AllocPolicy::hugepages() ? "madvise" : "off");
  std::printf("working-set   latency(1)   mlp=4    mlp=8   mlp=16   mlp=32   [ns per random 64B line]\n");
  for (size_t mb = 1; mb <= max_mb; mb *= 4) {
    size_t bytes = mb << 20, nlines = bytes / 64;
    auto perm = make_cycle(nlines, 1);
    u32* lines = (u32*)alloc_zeroed_aligned(nlines * 64);
    for (size_t i = 0; i < nlines; i++) lines[i * 16] = perm[i];
    size_t steps = std::min<size_t>(nlines * 2, size_t(8) << 20);
    u64 sink = 0;
    double l1 = chase<1>(lines, nlines, steps, &sink);
    double l4 = chase<4>(lines, nlines, steps / 4, &sink);
    double l8 = chase<8>(lines, nlines, steps / 8, &sink);
    double l16 = chase<16>(lines, nlines, steps / 16, &sink);
    double l32 = chase<32>(lines, nlines, steps / 32, &sink);
    std::printf("%7zu MiB   %8.1f   %6.2f   %6.2f   %6.2f   %6.2f\n", mb, l1, l4, l8, l16, l32);
    if (sink == 42) std::printf("%llu\n", (unsigned long long)sink);
    free_aligned(lines, nlines * 64);
  }
  return 0;
}
