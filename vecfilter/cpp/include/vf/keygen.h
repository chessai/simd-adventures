// vf/keygen.h -- key distributions for benchmarks and tests.
#pragma once
#include "config.h"
#include <cmath>
#include <string>
#include <vector>

namespace vf {

struct Rng {  // splitmix64
  u64 s;
  explicit Rng(u64 seed = 0x1234567) : s(seed) {}
  VF_INLINE u64 next() {
    u64 z = (s += 0x9E3779B97F4A7C15ull);
    z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9ull;
    z = (z ^ (z >> 27)) * 0x94D049BB133111EBull;
    return z ^ (z >> 31);
  }
  VF_INLINE double unit() { return (double)(next() >> 11) * (1.0 / 9007199254740992.0); }
};

// Distributions over the *inserted* set:
//   uniform    : random 64-bit keys
//   sequential : base + i  (low entropy, stresses the hash)
//   clustered  : runs of 64 consecutive keys at random bases (e.g. ids per tenant)
inline std::vector<u64> make_keys(const std::string& dist, size_t n, u64 seed) {
  std::vector<u64> k(n);
  Rng r(seed);
  if (dist == "sequential") {
    u64 base = r.next();
    for (size_t i = 0; i < n; i++) k[i] = base + i;
  } else if (dist == "clustered") {
    u64 base = 0;
    for (size_t i = 0; i < n; i++) { if (i % 64 == 0) base = r.next() & ~u64(63); k[i] = base + (i % 64); }
  } else {
    for (size_t i = 0; i < n; i++) k[i] = r.next();
  }
  return k;
}

// Query streams:
//   mixed     : 50% keys drawn uniformly from `present`, 50% fresh random keys (absent w.h.p.)
//   absent    : 100% fresh random keys (the common "negative lookup" case)
//   zipf      : queries drawn from `present` with a Zipf(1) popularity skew -- hot blocks stay cached
inline std::vector<u64> make_queries(const std::string& qdist, const std::vector<u64>& present, size_t nq, u64 seed) {
  std::vector<u64> q(nq);
  Rng r(seed);
  const size_t np = present.size();
  if (qdist == "absent") {
    for (size_t i = 0; i < nq; i++) q[i] = r.next();
  } else if (qdist == "zipf") {
    double lnN = std::log((double)np);
    for (size_t i = 0; i < nq; i++) {
      double u = r.unit();
      size_t rank = (size_t)std::exp(u * lnN);  // P(rank <= x) = ln x / ln N  => density ~ 1/x
      if (rank >= np) rank = np - 1;
      q[i] = present[rank];
    }
  } else {  // mixed
    for (size_t i = 0; i < nq; i++) q[i] = (r.next() & 1) ? present[r.next() % np] : r.next();
  }
  return q;
}

}  // namespace vf
