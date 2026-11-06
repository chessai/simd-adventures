//! A deliberately thin SIMD abstraction layer (port of `vf/simd.h`).
//!
//! Each backend token implements [`Ops`] over three logical widths:
//!   * `V32`  : 8 lanes of u32 (a 256-bit logical vector; NEON emulates it with two q-registers)
//!   * `B512` : 8 lanes of u64 (a 64-byte Bloom block; one zmm on AVX-512, two ymm on AVX2,
//!     four q-registers on NEON, an array in portable)
//!   * `V128` : 16 x u8 / 8 x u16 (one cuckoo bucket pair)
//!
//! Only the handful of operations the filters need are exposed. The portable backend is also the
//! scalar reference used by the property tests, so its semantics define what the ISA backends must
//! match bit-for-bit.
//!
//! The methods take the token (`self`): a token exists only if the CPU supports its features
//! (see [`crate::backend`]), so they are safe. They are `#[inline(always)]` so that, called inside
//! [`Token::vectorize`](simdlab::Token::vectorize), they compile with the token's features.

mod portable;
mod stdsimd;

#[cfg(target_arch = "x86_64")]
mod avx2;
#[cfg(target_arch = "x86_64")]
mod avx512;
#[cfg(target_arch = "aarch64")]
mod neon;

pub trait Ops: simdlab::Token {
    const NAME: &'static str;
    type V32: Copy;
    type B512: Copy;
    type V128: Copy;

    fn set1_32(self, x: u32) -> Self::V32;
    /// # Safety
    /// `p` must be valid for reading 32 bytes.
    unsafe fn loadu_32(self, p: *const u32) -> Self::V32;
    /// # Safety
    /// `p` must be valid for writing 32 bytes.
    unsafe fn storeu_32(self, p: *mut u32, a: Self::V32);
    fn add_32(self, a: Self::V32, b: Self::V32) -> Self::V32;
    fn xor_32(self, a: Self::V32, b: Self::V32) -> Self::V32;
    fn or_32(self, a: Self::V32, b: Self::V32) -> Self::V32;
    fn and_32(self, a: Self::V32, b: Self::V32) -> Self::V32;
    fn mullo_32(self, a: Self::V32, b: Self::V32) -> Self::V32;
    fn shr_32<const N: i32>(self, a: Self::V32) -> Self::V32;
    /// Per-lane `a << n`. Callers guarantee `n < 32` (positions come from `x >> 27` / `x >> 26`);
    /// the portable reference masks anyway.
    fn shl_var_32(self, a: Self::V32, n: Self::V32) -> Self::V32;
    /// `(a * b) >> 32`, unsigned, per lane.
    fn mulhi_32(self, a: Self::V32, b: Self::V32) -> Self::V32;
    /// Low and high 32-bit halves of 8 keys.
    fn split_keys8(self, k: &[u64; 8]) -> (Self::V32, Self::V32);
    /// `(block & mask) == mask`
    fn test_all_set_32(self, block: Self::V32, mask: Self::V32) -> bool;

    /// # Safety
    /// `p` must be 64-byte aligned and valid for reading 64 bytes.
    unsafe fn load_512(self, p: *const u8) -> Self::B512;
    /// # Safety
    /// `p` must be 64-byte aligned and valid for writing 64 bytes.
    unsafe fn store_512(self, p: *mut u8, a: Self::B512);
    fn or_512(self, a: Self::B512, b: Self::B512) -> Self::B512;
    /// `pos` holds 8 values in [0, 64): one bit per u64 lane.
    fn mask_512_from_pos(self, pos: Self::V32) -> Self::B512;
    fn test_all_set_512(self, block: Self::B512, mask: Self::B512) -> bool;

    /// Two 8-byte buckets into one register.
    /// # Safety
    /// `a` and `b` must be valid for reading 8 bytes each.
    unsafe fn load2x64(self, a: *const u8, b: *const u8) -> Self::V128;
    /// Two 4-byte buckets into lanes 0..8, rest zero.
    /// # Safety
    /// `a` and `b` must be valid for reading 4 bytes each.
    unsafe fn load2x32(self, a: *const u8, b: *const u8) -> Self::V128;
    /// Bitmask over the 8 u16 lanes (bit i set iff lane i == needle).
    fn match_mask_u16(self, hay: Self::V128, needle: u16) -> u32;
    /// Bitmask over the 16 u8 lanes.
    fn match_mask_u8(self, hay: Self::V128, needle: u8) -> u32;

    /// Whether [`Ops::test2_blocks32`] is implemented (AVX-512 only).
    const HAS_TEST2_BLOCKS32: bool = false;
    /// Test two 32-byte blocks (two keys) at once: bit 0 = key A present, bit 1 = key B present.
    /// # Safety
    /// `a` and `b` must be 32-byte aligned and valid for reading 32 bytes each.
    unsafe fn test2_blocks32(self, _a: *const u8, _b: *const u8, _ma: Self::V32, _mb: Self::V32) -> u32 {
        unreachable!("test2_blocks32 is only implemented by backends with HAS_TEST2_BLOCKS32")
    }

    #[inline(always)]
    fn from_array(self, a: &[u32; 8]) -> Self::V32 {
        // SAFETY: a is 32 readable bytes.
        unsafe { self.loadu_32(a.as_ptr()) }
    }
    #[inline(always)]
    fn to_array(self, v: Self::V32) -> [u32; 8] {
        let mut a = [0u32; 8];
        // SAFETY: a is 32 writable bytes.
        unsafe { self.storeu_32(a.as_mut_ptr(), v) };
        a
    }
}
