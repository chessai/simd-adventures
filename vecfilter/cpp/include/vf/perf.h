// vf/perf.h -- hardware performance counters via perf_event_open (Linux),
// with graceful degradation: any counter the kernel/VM refuses is reported as
// NaN, and cycles fall back to the TSC / virtual counter so cycles-per-op
// is still available (TSC ticks at a fixed rate, not core clock -- flagged in
// the output).
#pragma once
#include "config.h"
#include <chrono>
#include <cmath>
#include <cstdio>
#include <cstring>
#include <string>
#include <vector>

#if defined(__linux__)
#  include <linux/perf_event.h>
#  include <sys/ioctl.h>
#  include <sys/syscall.h>
#  include <unistd.h>
#endif

namespace vf {

VF_INLINE u64 fixed_counter() {
#if defined(__x86_64__)
  unsigned lo, hi;
  __asm__ __volatile__("rdtsc" : "=a"(lo), "=d"(hi));
  return ((u64)hi << 32) | lo;
#elif defined(__aarch64__)
  u64 v;
  __asm__ __volatile__("mrs %0, cntvct_el0" : "=r"(v));
  return v;
#else
  return (u64)std::chrono::steady_clock::now().time_since_epoch().count();
#endif
}

struct PerfCounts {
  double ns = 0;
  double cycles = NAN, instructions = NAN, branch_misses = NAN, l1d_misses = NAN, llc_misses = NAN;
  bool cycles_from_tsc = false;
};

class PerfGroup {
 public:
  struct Ev { const char* name; u32 type; u64 config; int fd = -1; bool ok = false; };

  PerfGroup() {
#if defined(__linux__)
    evs_ = {
      { "cycles", PERF_TYPE_HARDWARE, PERF_COUNT_HW_CPU_CYCLES },
      { "instructions", PERF_TYPE_HARDWARE, PERF_COUNT_HW_INSTRUCTIONS },
      { "branch-misses", PERF_TYPE_HARDWARE, PERF_COUNT_HW_BRANCH_MISSES },
      { "L1-dcache-load-misses", PERF_TYPE_HW_CACHE,
        PERF_COUNT_HW_CACHE_L1D | (PERF_COUNT_HW_CACHE_OP_READ << 8) | (PERF_COUNT_HW_CACHE_RESULT_MISS << 16) },
      { "LLC-load-misses", PERF_TYPE_HW_CACHE,
        PERF_COUNT_HW_CACHE_LL | (PERF_COUNT_HW_CACHE_OP_READ << 8) | (PERF_COUNT_HW_CACHE_RESULT_MISS << 16) },
    };
    for (auto& e : evs_) {
      perf_event_attr a;
      std::memset(&a, 0, sizeof a);
      a.type = e.type; a.size = sizeof a; a.config = e.config;
      a.disabled = 1; a.exclude_kernel = 1; a.exclude_hv = 1;
      e.fd = (int)syscall(SYS_perf_event_open, &a, 0, -1, -1, 0);  // one fd per event: no group-scheduling failures
      e.ok = e.fd >= 0;
    }
#endif
  }
  ~PerfGroup() {
#if defined(__linux__)
    for (auto& e : evs_) if (e.ok) close(e.fd);
#endif
  }
  bool available() const { for (auto& e : evs_) if (e.ok) return true; return false; }
  std::string status() const {
    std::string s;
    for (auto& e : evs_) { s += e.name; s += e.ok ? ":ok " : ":n/a "; }
    return s;
  }

  template <class F>
  PerfCounts measure(F&& body) {
    PerfCounts c;
#if defined(__linux__)
    for (auto& e : evs_) if (e.ok) { ioctl(e.fd, PERF_EVENT_IOC_RESET, 0); ioctl(e.fd, PERF_EVENT_IOC_ENABLE, 0); }
#endif
    u64 t0 = fixed_counter();
    auto w0 = std::chrono::steady_clock::now();
    body();
    auto w1 = std::chrono::steady_clock::now();
    u64 t1 = fixed_counter();
#if defined(__linux__)
    for (auto& e : evs_) if (e.ok) ioctl(e.fd, PERF_EVENT_IOC_DISABLE, 0);
    double* slots[5] = { &c.cycles, &c.instructions, &c.branch_misses, &c.l1d_misses, &c.llc_misses };
    for (size_t i = 0; i < evs_.size(); i++) {
      if (!evs_[i].ok) continue;
      u64 v = 0;
      if (read(evs_[i].fd, &v, sizeof v) == (ssize_t)sizeof v) *slots[i] = (double)v;
    }
#endif
    c.ns = std::chrono::duration<double, std::nano>(w1 - w0).count();
    if (std::isnan(c.cycles)) { c.cycles = (double)(t1 - t0); c.cycles_from_tsc = true; }
    return c;
  }

 private:
  std::vector<Ev> evs_;
};

}  // namespace vf
