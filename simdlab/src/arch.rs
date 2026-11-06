//! Architecture-specific helpers that are not tied to a backend.

/// Prefetch for reading into all cache levels (`__builtin_prefetch(p, 0, 3)`). Any address is
/// allowed: prefetches never fault.
#[inline(always)]
pub fn prefetch_read(p: *const u8) {
    #[cfg(target_arch = "x86_64")]
    // SAFETY: prefetch is a hint and never faults; SSE is part of the x86-64 baseline.
    unsafe {
        core::arch::x86_64::_mm_prefetch::<{ core::arch::x86_64::_MM_HINT_T0 }>(p.cast())
    }
    #[cfg(target_arch = "aarch64")]
    // SAFETY: prefetch is a hint and never faults.
    unsafe {
        core::arch::asm!("prfm pldl1keep, [{0}]", in(reg) p, options(nostack, readonly, preserves_flags))
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    let _ = p;
}

/// Prefetch for writing (`__builtin_prefetch(p, 1, 3)`). On x86 this is the `prefetcht0` GCC
/// emits when PREFETCHW is not among the target features.
#[inline(always)]
pub fn prefetch_write(p: *const u8) {
    #[cfg(target_arch = "aarch64")]
    // SAFETY: prefetch is a hint and never faults.
    unsafe {
        core::arch::asm!("prfm pstl1keep, [{0}]", in(reg) p, options(nostack, readonly, preserves_flags))
    }
    #[cfg(not(target_arch = "aarch64"))]
    prefetch_read(p)
}
