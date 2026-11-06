//! A thin 128-bit vector abstraction.
//!
//! The generic kernels in [`crate::kernels::generic`] are written once against this trait and
//! monomorphised per backend. Every method is `unsafe` because the SIMD implementations call
//! intrinsics that are only valid when the caller has enabled the matching target features; the
//! per-backend entry points in `kernels/*.rs` are `#[target_feature]` functions that the
//! `#[inline(always)]` trait methods get inlined into, which is the standard way to get
//! feature-specific code generation out of generic Rust.
//!
//! Byte-lane conventions follow memory order: lane 0 is the lowest address, `bsl::<N>` moves
//! bytes to *higher* lanes (x86 `pslldq`, NEON `ext` against zero).

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub mod sse;
#[cfg(target_arch = "aarch64")]
pub mod neon;
pub mod portable;
pub mod stdsimd;

pub trait Vec128: Copy + 'static {
    type V: Copy;

    unsafe fn load(p: *const u8) -> Self::V;
    unsafe fn store(p: *mut u8, v: Self::V);
    /// 4 bytes into lanes 0..4, rest zero
    unsafe fn load32(p: *const u8) -> Self::V;
    unsafe fn store32(p: *mut u8, v: Self::V);
    /// 8 bytes into lanes 0..8, rest zero
    unsafe fn load64(p: *const u8) -> Self::V;
    unsafe fn store64(p: *mut u8, v: Self::V);
    unsafe fn to_bytes(v: Self::V) -> [u8; 16];
    unsafe fn from_bytes(b: &[u8; 16]) -> Self::V;

    unsafe fn zero() -> Self::V;
    unsafe fn set1_u8(x: u8) -> Self::V;
    unsafe fn set1_u16(x: u16) -> Self::V;

    unsafe fn add8(a: Self::V, b: Self::V) -> Self::V;
    unsafe fn sub8(a: Self::V, b: Self::V) -> Self::V;
    unsafe fn and(a: Self::V, b: Self::V) -> Self::V;
    /// `!a & b`
    unsafe fn andnot(a: Self::V, b: Self::V) -> Self::V;
    unsafe fn or(a: Self::V, b: Self::V) -> Self::V;
    unsafe fn xor(a: Self::V, b: Self::V) -> Self::V;
    /// `floor((a + b) / 2)` per byte
    unsafe fn avg_floor_u8(a: Self::V, b: Self::V) -> Self::V;

    /// shift bytes towards higher lanes by N (zeros enter at lane 0)
    unsafe fn bsl<const N: i32>(v: Self::V) -> Self::V;
    /// shift bytes towards lower lanes by N
    unsafe fn bsr<const N: i32>(v: Self::V) -> Self::V;
    /// byte permute; an index with bit 7 set yields 0 (pshufb / tbl semantics)
    unsafe fn shuffle8(v: Self::V, idx: Self::V) -> Self::V;

    /// zero-extend lanes 0..8 to 16-bit lanes
    unsafe fn unpacklo8(v: Self::V) -> Self::V;
    /// narrow two vectors of 16-bit lanes to bytes (unsigned saturation)
    unsafe fn pack16(lo: Self::V, hi: Self::V) -> Self::V;
    unsafe fn add16(a: Self::V, b: Self::V) -> Self::V;
    unsafe fn sub16(a: Self::V, b: Self::V) -> Self::V;
    unsafe fn abs16(a: Self::V) -> Self::V;
    unsafe fn min_u16(a: Self::V, b: Self::V) -> Self::V;
    unsafe fn cmpeq16(a: Self::V, b: Self::V) -> Self::V;
    /// signed 16-bit `a > b` -> 0xFFFF / 0
    unsafe fn cmpgt16(a: Self::V, b: Self::V) -> Self::V;
    /// `mask ? b : a` bytewise (mask lanes are 0x00 or 0xFF)
    unsafe fn select(mask: Self::V, b: Self::V, a: Self::V) -> Self::V;
}
