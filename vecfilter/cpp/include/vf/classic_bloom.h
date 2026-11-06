// vf/classic_bloom.h -- the simple scalar baseline: a plain Bloom filter with
// k independent bit positions (Kirsch-Mitzenmacher double hashing), each one
// a potentially different cache line, tested in a loop with early exit.
#pragma once
#include "alloc.h"
#include "config.h"
#include "hash.h"
#include <cstdlib>
#include <cstring>
#include <stdexcept>

namespace vf {

class ClassicBloom {
 public:
  explicit ClassicBloom(size_t bytes, int k = 7) : k_(k) {
    nbits_ = bytes < 8 ? 64 : bytes * 8;
    bits_ = (u64*)alloc_zeroed_aligned(nbits_ / 8);
  }
  ~ClassicBloom() { free_aligned(bits_, nbits_ / 8); }
  ClassicBloom(const ClassicBloom&) = delete;
  ClassicBloom& operator=(const ClassicBloom&) = delete;
  size_t bytes() const { return nbits_ / 8; }
  int k() const { return k_; }

  VF_INLINE u64 index(u64 h) const {
    // (h * nbits) >> 64 : one mul instruction, works for any size.
    return (u64)(((unsigned __int128)h * nbits_) >> 64);
  }
  void insert(u64 key) {
    u32 hb, hl;
    hash1(key, hb, hl);
    u64 h1 = ((u64)hb << 32) | hl, h2 = (((u64)hl << 32) | hb) | 1;
    for (int i = 0; i < k_; i++) {
      u64 idx = index(h1);
      bits_[idx >> 6] |= u64(1) << (idx & 63);
      h1 += h2;
    }
  }
  bool contains(u64 key) const {
    u32 hb, hl;
    hash1(key, hb, hl);
    u64 h1 = ((u64)hb << 32) | hl, h2 = (((u64)hl << 32) | hb) | 1;
    for (int i = 0; i < k_; i++) {
      u64 idx = index(h1);
      if (!((bits_[idx >> 6] >> (idx & 63)) & 1)) return false;  // early exit (branchy, as in real code)
      h1 += h2;
    }
    return true;
  }

 private:
  u64* bits_ = nullptr;
  size_t nbits_ = 0;
  int k_ = 7;
};

}  // namespace vf
