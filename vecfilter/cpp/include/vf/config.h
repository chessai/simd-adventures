// vf/config.h -- ISA detection and small portability helpers.
#pragma once
#include <cstdint>
#include <cstddef>

#if defined(VF_FORCE_PORTABLE)
#  define VF_HAVE_AVX2 0
#  define VF_HAVE_AVX512 0
#  define VF_HAVE_NEON 0
#else
#  if defined(__AVX512F__) && defined(__AVX512BW__) && defined(__AVX512DQ__) && defined(__AVX512VL__)
#    define VF_HAVE_AVX512 1
#  else
#    define VF_HAVE_AVX512 0
#  endif
#  if defined(__AVX2__)
#    define VF_HAVE_AVX2 1
#  else
#    define VF_HAVE_AVX2 0
#  endif
#  if defined(__ARM_NEON) || defined(__aarch64__)
#    define VF_HAVE_NEON 1
#  else
#    define VF_HAVE_NEON 0
#  endif
#endif

#if defined(__GNUC__) || defined(__clang__)
#  define VF_INLINE inline __attribute__((always_inline))
#  define VF_NOINLINE __attribute__((noinline))
#  define VF_LIKELY(x) __builtin_expect(!!(x), 1)
#  define VF_UNLIKELY(x) __builtin_expect(!!(x), 0)
#  define VF_PREFETCH_R(p) __builtin_prefetch((p), 0, 3)
#  define VF_PREFETCH_W(p) __builtin_prefetch((p), 1, 3)
#else
#  define VF_INLINE inline
#  define VF_NOINLINE
#  define VF_LIKELY(x) (x)
#  define VF_UNLIKELY(x) (x)
#  define VF_PREFETCH_R(p) ((void)0)
#  define VF_PREFETCH_W(p) ((void)0)
#endif

// Loop pragma asking the compiler NOT to auto-vectorise. Used in the portable
// backend so the "scalar" numbers in the benchmark really are scalar.
#if defined(__clang__)
#  define VF_NOVEC _Pragma("clang loop vectorize(disable) interleave(disable)")
#elif defined(__GNUC__) && __GNUC__ >= 14
#  define VF_NOVEC _Pragma("GCC novector")
#else
#  define VF_NOVEC
#endif

namespace vf {
using u8 = uint8_t;
using u16 = uint16_t;
using u32 = uint32_t;
using u64 = uint64_t;

// Backend availability as constexpr booleans so templates can branch.
constexpr bool kHaveAvx2 = VF_HAVE_AVX2 != 0;
constexpr bool kHaveAvx512 = VF_HAVE_AVX512 != 0;
constexpr bool kHaveNeon = VF_HAVE_NEON != 0;
}  // namespace vf
