// vf/blocked_bloom.h -- cache-line-blocked ("split block") Bloom filter.
//
// Every key maps to exactly one block of BlockBytes (32 or 64).  The block is
// viewed as 8 lanes (u32 lanes for 32-byte blocks, u64 lanes for 64-byte
// blocks) and the key sets BitsPerLane bits in *every* lane, so k = 8 or 16.
// Lane i's bit position is ((hl * salt_i) >> (32 - log2(lane_bits))): one
// vector multiply, one shift, one variable shift to build the whole k-bit
// mask, no per-key loop.  Membership is a single "all bits of mask set in
// block" test (vptest on x86, bic+umaxv on NEON).
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

// Detects a backend that can test two 32-byte blocks in one 512-bit op.
template <class O, class = void> struct has_test2_blocks32 : std::false_type {};
template <class O>
struct has_test2_blocks32<O, std::void_t<decltype(O::test2_blocks32(nullptr, nullptr, std::declval<typename O::v32>(), std::declval<typename O::v32>()))>>
    : std::true_type {};

template <class Ops, int BlockBytes, int BitsPerLane = 1>
class BlockedBloom {
  static_assert(BlockBytes == 32 || BlockBytes == 64, "block must be 32 or 64 bytes");
  static_assert(BitsPerLane == 1 || BitsPerLane == 2, "1 or 2 bits per lane");

 public:
  static constexpr int kBlockBytes = BlockBytes;
  static constexpr int kLaneBits = BlockBytes == 32 ? 32 : 64;
  static constexpr int kPosShift = 32 - (BlockBytes == 32 ? 5 : 6);
  static constexpr int kK = 8 * BitsPerLane;
  static constexpr size_t kMaxWindow = 256;
  using V = typename Ops::v32;

  explicit BlockedBloom(size_t bytes, size_t window = 32) : window_(std::min(window, kMaxWindow)) {
    nblocks_ = std::max<size_t>(1, bytes / BlockBytes);
    if (nblocks_ > 0xFFFFFFFFull) throw std::runtime_error("too many blocks for 32-bit block index");
    data_ = (u8*)alloc_zeroed_aligned(nblocks_ * BlockBytes);
  }
  ~BlockedBloom() { free_aligned(data_, nblocks_ * BlockBytes); }
  BlockedBloom(const BlockedBloom&) = delete;
  BlockedBloom& operator=(const BlockedBloom&) = delete;

  size_t bytes() const { return nblocks_ * BlockBytes; }
  size_t num_blocks() const { return nblocks_; }
  const u8* data() const { return data_; }
  void set_window(size_t w) { window_ = std::min(w, kMaxWindow); }

  // ---- per-key API --------------------------------------------------------
  void insert(u64 key) {
    u32 hb, hl;
    hash1(key, hb, hl);
    insert_at(fastrange32(hb, (u32)nblocks_), hl);
  }
  bool contains(u64 key) const {
    u32 hb, hl;
    hash1(key, hb, hl);
    return test_at(fastrange32(hb, (u32)nblocks_), hl);
  }

  // ---- batched API ---------------------------------------------------------
  void insert_batch(const u64* keys, size_t n) {
    run_batched(keys, n, [this](u32 blk, u32 hl, size_t) { insert_at(blk, hl); }, /*write=*/true);
  }
  void contains_batch(const u64* keys, size_t n, u8* out) const {
    auto one = [this, out](u32 blk, u32 hl, size_t i) { out[i] = (u8)test_at(blk, hl); };
    // Opt-in: measured ~35% slower than one-key-per-ymm in cache on Emerald Rapids (REPORT.md §5.6).
#ifdef VF_AVX512_PAIRED_LOOKUP
    constexpr bool kPaired = has_test2_blocks32<Ops>::value;
#else
    constexpr bool kPaired = false;
#endif
    if constexpr (BlockBytes == 32 && kPaired) {
      // AVX-512: two keys' blocks and masks share one zmm; one masked test gives both answers.
      auto two = [this, out](u32 blkA, u32 hlA, u32 blkB, u32 hlB, size_t i) {
        const u8* base = data_;
        u32 r = Ops::test2_blocks32(base + (size_t)blkA * 32, base + (size_t)blkB * 32, mask32(hlA), mask32(hlB));
        out[i] = (u8)(r & 1); out[i + 1] = (u8)(r >> 1);
      };
      run_batched(keys, n, one, /*write=*/false, two);
    } else {
      run_batched(keys, n, one, /*write=*/false);
    }
  }

  // Same as contains_batch but returns the number of hits (for benchmarks).
  size_t count_batch(const u64* keys, size_t n) const {
    size_t c = 0;
    run_batched(keys, n, [this, &c](u32 blk, u32 hl, size_t) { c += test_at(blk, hl); }, false);
    return c;
  }

  // ---- building blocks (exposed for tests) ---------------------------------
  VF_INLINE void insert_at(u32 blk, u32 hl) {
    u8* p = data_ + (size_t)blk * BlockBytes;
    if constexpr (BlockBytes == 32) {
      V m = mask32(hl);
      Ops::storeu_32((u32*)p, Ops::or_32(Ops::loadu_32((const u32*)p), m));
    } else {
      auto m = mask64(hl);
      Ops::store_512(p, Ops::or_512(Ops::load_512(p), m));
    }
  }
  VF_INLINE bool test_at(u32 blk, u32 hl) const {
    const u8* p = data_ + (size_t)blk * BlockBytes;
    if constexpr (BlockBytes == 32) {
      return Ops::test_all_set_32(Ops::loadu_32((const u32*)p), mask32(hl));
    } else {
      return Ops::test_all_set_512(Ops::load_512(p), mask64(hl));
    }
  }
  static VF_INLINE V mask32(u32 hl) {
    V h = Ops::set1_32(hl);
    V pos = Ops::template shr_32<kPosShift>(Ops::mullo_32(h, Ops::loadu_32(kLaneSalts)));
    V m = Ops::shl_var_32(Ops::set1_32(1), pos);
    if constexpr (BitsPerLane == 2) {
      V pos2 = Ops::template shr_32<kPosShift>(Ops::mullo_32(h, Ops::loadu_32(kLaneSalts2)));
      m = Ops::or_32(m, Ops::shl_var_32(Ops::set1_32(1), pos2));
    }
    return m;
  }
  static VF_INLINE typename Ops::b512 mask64(u32 hl) {
    V h = Ops::set1_32(hl);
    V pos = Ops::template shr_32<kPosShift>(Ops::mullo_32(h, Ops::loadu_32(kLaneSalts)));
    auto m = Ops::mask_512_from_pos(pos);
    if constexpr (BitsPerLane == 2) {
      V pos2 = Ops::template shr_32<kPosShift>(Ops::mullo_32(h, Ops::loadu_32(kLaneSalts2)));
      m = Ops::or_512(m, Ops::mask_512_from_pos(pos2));
    }
    return m;
  }

 private:
  // Software-pipelined driver shared by insert_batch / contains_batch:
  //   stage 1 (window w+1): hash 8 keys per vector op, compute block indices,
  //                         store them, issue one prefetch per block
  //   stage 2 (window w):   do the actual mask build + test/or on blocks that
  //                         were prefetched one window ago
  template <class F, class F2 = std::nullptr_t>
  VF_INLINE void run_batched(const u64* keys, size_t n, F&& f, bool write, F2 f2 = nullptr) const {
    const size_t W = window_;
    const u32 nb = (u32)nblocks_;
    alignas(32) u32 blk[2][kMaxWindow];
    alignas(32) u32 lh[2][kMaxWindow];
    const size_t nwin = (n + W - 1) / W;
    if (nwin == 0) return;

    auto stage1 = [&](size_t w, int buf) {
      const size_t s = w * W, e = std::min(s + W, n);
      size_t i = s;
      V nbv = Ops::set1_32(nb);
      for (; i + 8 <= e; i += 8) {
        V hb, hl;
        hash8_keys<Ops>(keys + i, hb, hl);
        Ops::storeu_32(blk[buf] + (i - s), Ops::mulhi_32(hb, nbv));
        Ops::storeu_32(lh[buf] + (i - s), hl);
      }
      for (; i < e; i++) {  // tail: scalar hash, same function
        u32 hb, hl;
        hash1(keys[i], hb, hl);
        blk[buf][i - s] = fastrange32(hb, nb);
        lh[buf][i - s] = hl;
      }
      const u8* base = data_;
      if (write) {
        for (size_t j = 0; j < e - s; j++) VF_PREFETCH_W(base + (size_t)blk[buf][j] * BlockBytes);
      } else {
        for (size_t j = 0; j < e - s; j++) VF_PREFETCH_R(base + (size_t)blk[buf][j] * BlockBytes);
      }
    };
    auto stage2 = [&](size_t w, int buf) {
      const size_t s = w * W, e = std::min(s + W, n);
      size_t j = 0;
      if constexpr (!std::is_same_v<F2, std::nullptr_t>) {
        for (; j + 2 <= e - s; j += 2) f2(blk[buf][j], lh[buf][j], blk[buf][j + 1], lh[buf][j + 1], s + j);
      }
      for (; j < e - s; j++) f(blk[buf][j], lh[buf][j], s + j);
    };

    stage1(0, 0);
    for (size_t w = 0; w < nwin; w++) {
      if (w + 1 < nwin) stage1(w + 1, (int)((w + 1) & 1));
      stage2(w, (int)(w & 1));
    }
  }

  u8* data_ = nullptr;
  size_t nblocks_ = 0;
  size_t window_ = 32;
};

}  // namespace vf
