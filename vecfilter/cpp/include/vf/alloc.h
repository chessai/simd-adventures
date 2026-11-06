// vf/alloc.h -- 64-byte-aligned, zeroed allocation with an opt-in request for
// transparent huge pages.  Large filters are accessed at random addresses, so
// without 2 MiB pages every lookup is also a TLB miss + page walk; on the
// benchmark machine that is a large fraction of the per-lookup cost (REPORT.md).
#pragma once
#include "config.h"
#include <cstdlib>
#include <cstring>
#include <new>

#if defined(__linux__)
#  include <sys/mman.h>
#endif

namespace vf {

struct AllocPolicy {
  static bool& hugepages() { static bool v = true; return v; }  // process-wide knob for benchmarks
  static constexpr size_t kHuge = size_t(2) << 20;
  static constexpr size_t kMmapThreshold = size_t(4) << 20;
  static size_t rounded(size_t bytes) { size_t s = (bytes + 63) & ~size_t(63); return s ? s : 64; }
};

// Allocations >= 4 MiB always come from a 2 MiB-aligned private mapping (so they
// are THP-eligible); smaller ones from posix_memalign.
inline void* alloc_zeroed_aligned(size_t bytes) {
  const size_t sz = AllocPolicy::rounded(bytes);
#if defined(__linux__)
  if (sz >= AllocPolicy::kMmapThreshold) {
    const size_t huge = AllocPolicy::kHuge;
    const size_t aligned = (sz + huge - 1) & ~(huge - 1);
    char* p = (char*)mmap(nullptr, aligned + huge, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (p == MAP_FAILED) throw std::bad_alloc();
    char* a = (char*)(((uintptr_t)p + huge - 1) & ~(uintptr_t)(huge - 1));
    if (a > p) munmap(p, a - p);                                   // trim head slack
    if (a + aligned < p + aligned + huge) munmap(a + aligned, (p + aligned + huge) - (a + aligned));  // trim tail
    if (AllocPolicy::hugepages()) madvise(a, aligned, MADV_HUGEPAGE);
    return a;  // fresh anonymous memory is zero
  }
#endif
  void* p = nullptr;
  if (posix_memalign(&p, 64, sz) != 0) throw std::bad_alloc();
  std::memset(p, 0, sz);
  return p;
}

inline void free_aligned(void* p, size_t bytes) {
  if (!p) return;
  const size_t sz = AllocPolicy::rounded(bytes);
#if defined(__linux__)
  if (sz >= AllocPolicy::kMmapThreshold) {
    const size_t huge = AllocPolicy::kHuge;
    munmap(p, (sz + huge - 1) & ~(huge - 1));
    return;
  }
#endif
  std::free(p);
}

}  // namespace vf
