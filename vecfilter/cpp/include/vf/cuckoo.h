// vf/cuckoo.h -- cuckoo filter (Fan, Andersen, Kaminsky, Mitzenmacher 2014)
// with 4-slot buckets and SIMD bucket scans.
//
//   FP = u16 : 8-byte buckets, FPR ~ 2*4/2^16 = 1.2e-4, ~16.8 bits/key at 95% load
//   FP = u8  : 4-byte buckets, FPR ~ 2*4/2^8  = 3.1%,   ~8.4 bits/key at 95% load
//
// A lookup loads both candidate buckets into one 128-bit register and does a
// single lane-equality compare + movemask.  Insert/erase reuse the same scan
// (needle = 0 finds free slots).  Deletion is supported; a "victim" stash keeps
// the no-false-negative guarantee if an insert hits the kick limit.
#pragma once
#include "alloc.h"
#include "config.h"
#include "hash.h"
#include "simd.h"
#include <algorithm>
#include <cstdlib>
#include <cstring>
#include <stdexcept>
#include <type_traits>

namespace vf {

template <class Ops, typename FP>
class CuckooFilter {
  static_assert(std::is_same_v<FP, u8> || std::is_same_v<FP, u16>, "FP must be u8 or u16");

 public:
  static constexpr int kSlots = 4;
  static constexpr int kFpBits = 8 * sizeof(FP);
  static constexpr int kMaxKicks = 500;
  static constexpr size_t kMaxWindow = 256;
  struct Bucket { FP s[kSlots]; };
  static constexpr size_t kBucketBytes = sizeof(Bucket);
  using V = typename Ops::v32;

  explicit CuckooFilter(size_t bytes, size_t window = 32) : window_(std::min(window, kMaxWindow)) {
    size_t want = std::max<size_t>(2, bytes / kBucketBytes);
    size_t nb = 1;
    while (nb * 2 <= want) nb *= 2;  // power of two so i2 = i1 ^ alt(fp) stays in range
    if (nb > 0xFFFFFFFFull) throw std::runtime_error("too many buckets");
    nbuckets_ = nb;
    mask_ = (u32)(nb - 1);
    b_ = (Bucket*)alloc_zeroed_aligned(nb * kBucketBytes);
  }
  ~CuckooFilter() { free_aligned(b_, nbuckets_ * kBucketBytes); }
  CuckooFilter(const CuckooFilter&) = delete;
  CuckooFilter& operator=(const CuckooFilter&) = delete;

  size_t bytes() const { return nbuckets_ * kBucketBytes; }
  size_t capacity() const { return nbuckets_ * kSlots; }
  size_t size() const { return count_; }
  double load() const { return (double)count_ / (double)capacity(); }
  void set_window(size_t w) { window_ = std::min(w, kMaxWindow); }

  struct Loc { u32 i1, i2; FP fp; };

  VF_INLINE u32 alt_index(u32 i, FP fp) const { return i ^ ((u32(fp) * 0x5bd1e995u) & mask_); }
  VF_INLINE Loc locate(u64 key) const {
    u32 hb, hl;
    hash1(key, hb, hl);
    return locate_h(hb, hl);
  }
  VF_INLINE Loc locate_h(u32 hb, u32 hl) const {
    Loc l;
    u32 f = hl >> (32 - kFpBits);
    l.fp = (FP)(f == 0 ? 1 : f);  // 0 means "empty slot"
    l.i1 = hb & mask_;
    l.i2 = alt_index(l.i1, l.fp);
    return l;
  }

  // ---- per-key API --------------------------------------------------------
  bool contains(u64 key) const {
    Loc l = locate(key);
    // Bitwise, not logical, OR: a data-dependent branch on the match result
    // mispredicts ~50% of the time on mixed hit/miss streams (see REPORT.md).
#ifdef VF_CUCKOO_BRANCHY  // benchmark-only: the version with the branch, for REPORT.md
    return scan_pair(l.i1, l.i2, l.fp) != 0 || victim_hit(l);
#else
    return ((unsigned)(scan_pair(l.i1, l.i2, l.fp) != 0) | (unsigned)victim_hit(l)) != 0;
#endif
  }

  bool insert(u64 key) {
    Loc l = locate(key);
    return insert_loc(l);
  }

  bool erase(u64 key) {
    Loc l = locate(key);
    u32 m = scan_pair(l.i1, l.i2, l.fp);
    if (m) {
      int s = __builtin_ctz(m);
      (s < kSlots ? b_[l.i1].s[s] : b_[l.i2].s[s - kSlots]) = 0;
      count_--;
      if (victim_.valid) {  // try to re-home the stash now that there is space
        Victim v = victim_;
        victim_.valid = false;
        count_--;
        insert_loc(Loc{ v.i, alt_index(v.i, v.fp), v.fp });
      }
      return true;
    }
    if (victim_hit(l)) { victim_.valid = false; count_--; return true; }
    return false;
  }

  // ---- batched API ---------------------------------------------------------
  void contains_batch(const u64* keys, size_t n, u8* out) const {
    run_batched(keys, n, [this, out](const Loc& l, size_t i) { out[i] = (u8)((unsigned)(scan_pair(l.i1, l.i2, l.fp) != 0) | (unsigned)victim_hit(l)); });
  }
  size_t count_batch(const u64* keys, size_t n) const {
    size_t c = 0;
    run_batched(keys, n, [this, &c](const Loc& l, size_t) { c += (unsigned)(scan_pair(l.i1, l.i2, l.fp) != 0) | (unsigned)victim_hit(l); });
    return c;
  }
  // Returns the number of keys that could NOT be inserted (filter full).
  size_t insert_batch(const u64* keys, size_t n) {
    size_t failed = 0;
    run_batched(keys, n, [this, &failed](const Loc& l, size_t) { failed += !insert_loc(l); }, /*write=*/true);
    return failed;
  }

  // ---- building blocks (exposed for tests) ---------------------------------
  // Bitmask over 8 (u16) or 16 (u8) lanes: lanes 0..3 = bucket i1, 4..7 = bucket i2.
  VF_INLINE u32 scan_pair(u32 i1, u32 i2, FP needle) const {
    const Bucket* b = b_;
    if constexpr (std::is_same_v<FP, u16>) {
      return Ops::match_mask_u16(Ops::load2x64(b + i1, b + i2), needle);
    } else {
      return Ops::match_mask_u8(Ops::load2x32(b + i1, b + i2), needle) & 0xFF;
    }
  }
  VF_INLINE u32 scan_one(u32 i, FP needle) const { return scan_pair(i, i, needle) & 0xF; }

  bool insert_loc(const Loc& l) {
    if (victim_.valid) return false;
    Bucket* b = b_;
    u32 free = scan_pair(l.i1, l.i2, (FP)0);
    if (free) {
      int s = __builtin_ctz(free);
      (s < kSlots ? b[l.i1].s[s] : b[l.i2].s[s - kSlots]) = l.fp;
      count_++;
      return true;
    }
    // Kick loop.
    u32 i = (rng_next() & 1) ? l.i1 : l.i2;
    FP fp = l.fp;
    for (int n = 0; n < kMaxKicks; n++) {
      int s = (int)(rng_next() & 3);
      std::swap(fp, b[i].s[s]);
      i = alt_index(i, fp);
      u32 f = scan_one(i, (FP)0);
      if (f) {
        b[i].s[__builtin_ctz(f)] = fp;
        count_++;
        return true;
      }
    }
    victim_ = Victim{ true, i, fp };
    count_++;
    return false;
  }

 private:
  struct Victim { bool valid = false; u32 i = 0; FP fp = 0; };
  // Branch-free on purpose (it sits on the lookup path).
  VF_INLINE bool victim_hit(const Loc& l) const {
    return ((unsigned)victim_.valid & (unsigned)(victim_.fp == l.fp) & ((unsigned)(victim_.i == l.i1) | (unsigned)(victim_.i == l.i2))) != 0;
  }
  VF_INLINE u32 rng_next() {
    rng_ ^= rng_ << 13; rng_ ^= rng_ >> 7; rng_ ^= rng_ << 17;
    return (u32)rng_;
  }

  // Vector hashing + prefetch of both buckets, one window ahead (see
  // BlockedBloom::run_batched for the pipeline shape).
  template <class F>
  VF_INLINE void run_batched(const u64* keys, size_t n, F&& f, bool write = false) const {
    const size_t W = window_;
    alignas(32) u32 i1[2][kMaxWindow], i2[2][kMaxWindow], fp[2][kMaxWindow];
    const size_t nwin = (n + W - 1) / W;
    if (nwin == 0) return;
    const V maskv = Ops::set1_32(mask_);
    const V altc = Ops::set1_32(0x5bd1e995u);
    const V ones = Ops::set1_32(0xFFFFFFFFu);

    auto stage1 = [&](size_t w, int buf) {
      const size_t s = w * W, e = std::min(s + W, n);
      size_t i = s;
      for (; i + 8 <= e; i += 8) {
        V hb, hl;
        hash8_keys<Ops>(keys + i, hb, hl);
        V f = Ops::template shr_32<32 - kFpBits>(hl);
        f = Ops::or_32(f, Ops::template shr_32<31>(Ops::add_32(f, ones)));  // f==0 -> 1
        V a = Ops::and_32(hb, maskv);
        V b = Ops::xor_32(a, Ops::and_32(Ops::mullo_32(f, altc), maskv));
        Ops::storeu_32(i1[buf] + (i - s), a);
        Ops::storeu_32(i2[buf] + (i - s), b);
        Ops::storeu_32(fp[buf] + (i - s), f);
      }
      for (; i < e; i++) {
        Loc l = locate(keys[i]);
        i1[buf][i - s] = l.i1; i2[buf][i - s] = l.i2; fp[buf][i - s] = l.fp;
      }
      const Bucket* b = b_;
      if (write) {
        for (size_t j = 0; j < e - s; j++) { VF_PREFETCH_W(b + i1[buf][j]); VF_PREFETCH_W(b + i2[buf][j]); }
      } else {
        for (size_t j = 0; j < e - s; j++) { VF_PREFETCH_R(b + i1[buf][j]); VF_PREFETCH_R(b + i2[buf][j]); }
      }
    };
    auto stage2 = [&](size_t w, int buf) {
      const size_t s = w * W, e = std::min(s + W, n);
      for (size_t j = 0; j < e - s; j++) f(Loc{ i1[buf][j], i2[buf][j], (FP)fp[buf][j] }, s + j);
    };
    stage1(0, 0);
    for (size_t w = 0; w < nwin; w++) {
      if (w + 1 < nwin) stage1(w + 1, (int)((w + 1) & 1));
      stage2(w, (int)(w & 1));
    }
  }

  Bucket* b_ = nullptr;
  size_t nbuckets_ = 0;
  u32 mask_ = 0;
  size_t count_ = 0;
  size_t window_ = 32;
  u64 rng_ = 0x9E3779B97F4A7C15ull;
  Victim victim_;
};

}  // namespace vf
