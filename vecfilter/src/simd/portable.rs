//! Portable backend (scalar reference).
//!
//! The C++ builds this backend's binaries with `-fno-tree-vectorize -fno-tree-slp-vectorize` so
//! its benchmark numbers are really scalar. rustc has no per-function equivalent; for scalar
//! benchmark numbers build with `-C no-vectorize-loops -C no-vectorize-slp` (scripts/run_all.sh
//! does).

use super::Ops;
use crate::backend::Portable;

impl Ops for Portable {
    const NAME: &'static str = "portable";
    type V32 = [u32; 8];
    type B512 = [u64; 8];
    type V128 = [u8; 16];

    #[inline(always)]
    fn set1_32(self, x: u32) -> [u32; 8] {
        [x; 8]
    }
    #[inline(always)]
    unsafe fn loadu_32(self, p: *const u32) -> [u32; 8] {
        // SAFETY: caller guarantees 32 readable bytes.
        unsafe { p.cast::<[u32; 8]>().read_unaligned() }
    }
    #[inline(always)]
    unsafe fn storeu_32(self, p: *mut u32, a: [u32; 8]) {
        // SAFETY: caller guarantees 32 writable bytes.
        unsafe { p.cast::<[u32; 8]>().write_unaligned(a) }
    }
    #[inline(always)]
    fn add_32(self, a: [u32; 8], b: [u32; 8]) -> [u32; 8] {
        std::array::from_fn(|i| a[i].wrapping_add(b[i]))
    }
    #[inline(always)]
    fn xor_32(self, a: [u32; 8], b: [u32; 8]) -> [u32; 8] {
        std::array::from_fn(|i| a[i] ^ b[i])
    }
    #[inline(always)]
    fn or_32(self, a: [u32; 8], b: [u32; 8]) -> [u32; 8] {
        std::array::from_fn(|i| a[i] | b[i])
    }
    #[inline(always)]
    fn and_32(self, a: [u32; 8], b: [u32; 8]) -> [u32; 8] {
        std::array::from_fn(|i| a[i] & b[i])
    }
    #[inline(always)]
    fn mullo_32(self, a: [u32; 8], b: [u32; 8]) -> [u32; 8] {
        std::array::from_fn(|i| a[i].wrapping_mul(b[i]))
    }
    #[inline(always)]
    fn shr_32<const N: i32>(self, a: [u32; 8]) -> [u32; 8] {
        std::array::from_fn(|i| a[i] >> N)
    }
    #[inline(always)]
    fn shl_var_32(self, a: [u32; 8], n: [u32; 8]) -> [u32; 8] {
        std::array::from_fn(|i| a[i] << (n[i] & 31))
    }
    #[inline(always)]
    fn mulhi_32(self, a: [u32; 8], b: [u32; 8]) -> [u32; 8] {
        std::array::from_fn(|i| ((a[i] as u64 * b[i] as u64) >> 32) as u32)
    }
    #[inline(always)]
    fn split_keys8(self, k: &[u64; 8]) -> ([u32; 8], [u32; 8]) {
        (std::array::from_fn(|i| k[i] as u32), std::array::from_fn(|i| (k[i] >> 32) as u32))
    }
    #[inline(always)]
    fn test_all_set_32(self, block: [u32; 8], mask: [u32; 8]) -> bool {
        let mut acc = 0;
        for i in 0..8 {
            acc |= !block[i] & mask[i];
        }
        acc == 0
    }

    #[inline(always)]
    unsafe fn load_512(self, p: *const u8) -> [u64; 8] {
        // SAFETY: caller guarantees 64 readable bytes.
        unsafe { p.cast::<[u64; 8]>().read_unaligned() }
    }
    #[inline(always)]
    unsafe fn store_512(self, p: *mut u8, a: [u64; 8]) {
        // SAFETY: caller guarantees 64 writable bytes.
        unsafe { p.cast::<[u64; 8]>().write_unaligned(a) }
    }
    #[inline(always)]
    fn or_512(self, a: [u64; 8], b: [u64; 8]) -> [u64; 8] {
        std::array::from_fn(|i| a[i] | b[i])
    }
    #[inline(always)]
    fn mask_512_from_pos(self, pos: [u32; 8]) -> [u64; 8] {
        std::array::from_fn(|i| 1u64 << (pos[i] & 63))
    }
    #[inline(always)]
    fn test_all_set_512(self, block: [u64; 8], mask: [u64; 8]) -> bool {
        let mut acc = 0;
        for i in 0..8 {
            acc |= !block[i] & mask[i];
        }
        acc == 0
    }

    #[inline(always)]
    unsafe fn load2x64(self, a: *const u8, b: *const u8) -> [u8; 16] {
        let mut r = [0u8; 16];
        // SAFETY: caller guarantees 8 readable bytes at each of a and b.
        unsafe {
            std::ptr::copy_nonoverlapping(a, r.as_mut_ptr(), 8);
            std::ptr::copy_nonoverlapping(b, r.as_mut_ptr().add(8), 8);
        }
        r
    }
    #[inline(always)]
    unsafe fn load2x32(self, a: *const u8, b: *const u8) -> [u8; 16] {
        let mut r = [0u8; 16];
        // SAFETY: caller guarantees 4 readable bytes at each of a and b.
        unsafe {
            std::ptr::copy_nonoverlapping(a, r.as_mut_ptr(), 4);
            std::ptr::copy_nonoverlapping(b, r.as_mut_ptr().add(4), 4);
        }
        r
    }
    #[inline(always)]
    fn match_mask_u16(self, hay: [u8; 16], needle: u16) -> u32 {
        let mut m = 0;
        for i in 0..8 {
            let x = u16::from_ne_bytes([hay[2 * i], hay[2 * i + 1]]);
            m |= ((x == needle) as u32) << i;
        }
        m
    }
    #[inline(always)]
    fn match_mask_u8(self, hay: [u8; 16], needle: u8) -> u32 {
        let mut m = 0;
        for i in 0..16 {
            m |= ((hay[i] == needle) as u32) << i;
        }
        m
    }
}
