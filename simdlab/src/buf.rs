//! 64-byte-aligned, zeroed buffers with an opt-in request for transparent huge pages.
//!
//! Large buffers accessed at random addresses (filters, hash tables) pay a TLB miss + page walk
//! per access without 2 MiB pages; on vecfilter's benchmark machine that was a large fraction of
//! the per-lookup cost (vecfilter/REPORT.md).

use std::alloc::{alloc_zeroed, dealloc, handle_alloc_error, Layout};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};

static HUGEPAGES: AtomicBool = AtomicBool::new(true);

/// Process-wide knob for benchmarks: request `MADV_HUGEPAGE` on large allocations.
pub fn hugepages() -> bool {
    HUGEPAGES.load(Ordering::Relaxed)
}
pub fn set_hugepages(on: bool) {
    HUGEPAGES.store(on, Ordering::Relaxed)
}

const HUGE: usize = 2 << 20;
const MMAP_THRESHOLD: usize = 4 << 20;

fn rounded(bytes: usize) -> usize {
    let s = (bytes + 63) & !63;
    if s == 0 { 64 } else { s }
}

/// Plain-old-data element types an [`AlignedBuf`] can be viewed as.
pub trait Pod: Copy + sealed::Sealed {}
mod sealed {
    pub trait Sealed {}
}
macro_rules! pod {
    ($($t:ty),*) => { $(impl sealed::Sealed for $t {} impl Pod for $t {})* };
}
pod!(u8, u16, u32, u64);

/// A zeroed, 64-byte-aligned buffer. Allocations of 4 MiB or more always come from a
/// 2 MiB-aligned private mapping (so they are THP-eligible); smaller ones from the global
/// allocator.
pub struct AlignedBuf {
    ptr: NonNull<u8>,
    len: usize,
    mapped: Option<usize>,
}

// SAFETY: AlignedBuf uniquely owns its memory, like a Vec<u8>.
unsafe impl Send for AlignedBuf {}
unsafe impl Sync for AlignedBuf {}

impl AlignedBuf {
    pub fn zeroed(bytes: usize) -> Self {
        let sz = rounded(bytes);
        #[cfg(target_os = "linux")]
        if sz >= MMAP_THRESHOLD {
            let aligned = (sz + HUGE - 1) & !(HUGE - 1);
            // SAFETY: anonymous private mapping; the slack around the 2 MiB-aligned region is
            // unmapped again, leaving exactly [a, a + aligned) mapped.
            unsafe {
                let p = libc::mmap(
                    std::ptr::null_mut(),
                    aligned + HUGE,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                    -1,
                    0,
                );
                if p == libc::MAP_FAILED {
                    handle_alloc_error(Layout::from_size_align(aligned, HUGE).unwrap());
                }
                let p = p as usize;
                let a = (p + HUGE - 1) & !(HUGE - 1);
                if a > p {
                    libc::munmap(p as *mut _, a - p); // trim head slack
                }
                if a + aligned < p + aligned + HUGE {
                    libc::munmap((a + aligned) as *mut _, (p + aligned + HUGE) - (a + aligned)); // trim tail
                }
                if hugepages() {
                    libc::madvise(a as *mut _, aligned, libc::MADV_HUGEPAGE);
                }
                // fresh anonymous memory is zero
                return AlignedBuf { ptr: NonNull::new_unchecked(a as *mut u8), len: sz, mapped: Some(aligned) };
            }
        }
        let layout = Layout::from_size_align(sz, 64).unwrap();
        // SAFETY: layout has non-zero size.
        let p = unsafe { alloc_zeroed(layout) };
        let Some(ptr) = NonNull::new(p) else { handle_alloc_error(layout) };
        AlignedBuf { ptr, len: sz, mapped: None }
    }

    /// Size in bytes (the request rounded up to a multiple of 64).
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        false
    }
    pub fn as_ptr(&self) -> *const u8 {
        self.ptr.as_ptr()
    }
    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.ptr.as_ptr()
    }
    pub fn as_slice_of<T: Pod>(&self) -> &[T] {
        // SAFETY: 64-byte alignment covers every Pod type, the memory is initialised (zeroed),
        // and any bit pattern is a valid Pod value.
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr().cast(), self.len / size_of::<T>()) }
    }
    pub fn as_mut_slice_of<T: Pod>(&mut self) -> &mut [T] {
        // SAFETY: as for as_slice_of, and &mut self gives unique access.
        unsafe { std::slice::from_raw_parts_mut(self.ptr.as_ptr().cast(), self.len / size_of::<T>()) }
    }
}

impl Drop for AlignedBuf {
    fn drop(&mut self) {
        match self.mapped {
            #[cfg(target_os = "linux")]
            Some(map_len) => unsafe {
                libc::munmap(self.ptr.as_ptr().cast(), map_len);
            },
            #[cfg(not(target_os = "linux"))]
            Some(_) => unreachable!(),
            None => unsafe { dealloc(self.ptr.as_ptr(), Layout::from_size_align(self.len, 64).unwrap()) },
        }
    }
}
