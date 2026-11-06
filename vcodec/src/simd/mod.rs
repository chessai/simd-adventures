//! A deliberately thin SIMD abstraction layer.
//!
//! Every codec kernel in this crate is written **once**, generically over the
//! [`Simd`] trait, and then monomorphised for each backend:
//!
//! * [`StdSimd`](crate::backend::StdSimd) – `std::simd::u8x16`, for the build target's features.
//! * [`Portable`](crate::backend::Portable) – plain `[u8; 16]` arithmetic. Works everywhere,
//!   lets the compiler auto-vectorise what it can, and doubles as a reference
//!   implementation of the *trait semantics* that the other backends must match.
//! * [`Sse41`](crate::backend::Sse41) – SSSE3 + SSE4.1, 128-bit.
//! * [`Avx2`](crate::backend::Avx2) – AVX2, 256-bit, modelled as **two independent 128-bit lanes**.
//! * [`Neon`](crate::backend::Neon) – AArch64 NEON, 128-bit.
//!
//! The design rule is *lane-local semantics*: a vector is `BYTES / 16` lanes of
//! 16 bytes and every operation acts within a lane. That is the only model that
//! AVX2 (`vpshufb`, `vpunpck*`, `vpalignr` are all per-128-bit-lane) and NEON
//! (`tbl` over a 16-byte table) can both implement cheaply. The handful of places
//! where a codec needs lane-crossing movement (base64's 12-byte lane loads and
//! stores, hex's interleave) are exposed as dedicated trait methods so that each
//! backend can do them its own way.
//!
//! All methods are `unsafe fn` because the x86/ARM implementations call
//! intrinsics that require a CPU feature to be present. The backend types are the
//! tokens of [`crate::backend`], and `define_dispatch!` obtains the token (the
//! runtime check) before calling a kernel: the only place the invariant is established.

pub mod portable;
pub mod stdsimd;

#[cfg(target_arch = "x86_64")]
pub mod avx2;
#[cfg(target_arch = "aarch64")]
pub mod neon;
#[cfg(target_arch = "x86_64")]
pub mod sse;

/// The SIMD backend contract. See the module docs for the lane model.
pub trait Simd: Copy + 'static {
    /// The vector type.
    type V: Copy;
    /// Width in bytes (16 or 32).
    const BYTES: usize;
    /// Number of 16-byte lanes.
    const LANES: usize = Self::BYTES / 16;
    /// Human-readable name for benchmarks and reports.
    const NAME: &'static str;

    // ---- memory ---------------------------------------------------------
    unsafe fn loadu(p: *const u8) -> Self::V;
    unsafe fn storeu(p: *mut u8, v: Self::V);

    // ---- constants ------------------------------------------------------
    unsafe fn splat(b: u8) -> Self::V;
    unsafe fn splat_u16(w: u16) -> Self::V;
    unsafe fn splat_u32(d: u32) -> Self::V;
    /// Build a vector from a 16-byte lane pattern replicated into every lane.
    unsafe fn from_lane(lane: [u8; 16]) -> Self::V;
    unsafe fn zero() -> Self::V;

    // ---- bitwise --------------------------------------------------------
    unsafe fn and(a: Self::V, b: Self::V) -> Self::V;
    unsafe fn or(a: Self::V, b: Self::V) -> Self::V;
    unsafe fn xor(a: Self::V, b: Self::V) -> Self::V;
    /// `a & !b` (note the operand order: the *second* operand is negated, so it
    /// reads like `andnot(keep, mask) = keep & !mask`).
    unsafe fn andnot(a: Self::V, b: Self::V) -> Self::V;

    // ---- byte arithmetic -------------------------------------------------
    unsafe fn add_u8(a: Self::V, b: Self::V) -> Self::V;
    unsafe fn sub_u8(a: Self::V, b: Self::V) -> Self::V;
    unsafe fn subs_u8(a: Self::V, b: Self::V) -> Self::V;
    unsafe fn adds_u8(a: Self::V, b: Self::V) -> Self::V;
    /// Per-byte equality; all-ones for equal bytes.
    unsafe fn cmpeq_u8(a: Self::V, b: Self::V) -> Self::V;
    /// Signed per-byte `a > b`; all-ones where true.
    unsafe fn cmpgt_i8(a: Self::V, b: Self::V) -> Self::V;
    /// Unsigned per-byte `a > b`; all-ones where true. (Default via max.)
    unsafe fn cmpgt_u8(a: Self::V, b: Self::V) -> Self::V {
        // a > b  <=>  min(a, b) != a ... use saturating subtraction: a - b > 0.
        let d = Self::subs_u8(a, b);
        let z = Self::cmpeq_u8(d, Self::zero());
        Self::xor(z, Self::splat(0xFF))
    }
    /// Unsigned per-byte `a < b`.
    unsafe fn cmplt_u8(a: Self::V, b: Self::V) -> Self::V {
        Self::cmpgt_u8(b, a)
    }

    // ---- 16-bit / 32-bit lane arithmetic ----------------------------------
    unsafe fn shr_u16<const N: i32>(v: Self::V) -> Self::V;
    unsafe fn shl_u16<const N: i32>(v: Self::V) -> Self::V;
    unsafe fn shr_u32<const N: i32>(v: Self::V) -> Self::V;
    unsafe fn shl_u32<const N: i32>(v: Self::V) -> Self::V;
    /// Per-u16 `(a * b) >> 16` (unsigned high half).
    unsafe fn mulhi_u16(a: Self::V, b: Self::V) -> Self::V;
    /// Per-u16 `(a * b) & 0xFFFF`.
    unsafe fn mullo_u16(a: Self::V, b: Self::V) -> Self::V;
    /// For each 16-bit lane: `a[2i] * mul_even + a[2i+1] * mul_odd` where the
    /// `a[..]` are unsigned bytes. Results are small (< 2^15) for every use in
    /// this crate, so backends may use `pmaddubsw` or widening multiplies.
    unsafe fn madd_u8_pairs(a: Self::V, mul_even: u8, mul_odd: u8) -> Self::V;
    /// For each 32-bit lane: `a[2i] * mul_even + a[2i+1] * mul_odd` over
    /// unsigned 16-bit elements (results < 2^31).
    unsafe fn madd_u16_pairs(a: Self::V, mul_even: u16, mul_odd: u16) -> Self::V;

    // ---- permutes -----------------------------------------------------------
    /// `pshufb` / `tbl` semantics, per lane: `out[i] = idx[i] & 0x80 ? 0 : table[idx[i] & 15]`.
    unsafe fn shuffle_bytes(table: Self::V, idx: Self::V) -> Self::V;
    /// Per-lane `palignr`: concatenate `hi:lo` and take 16 bytes starting at byte `N`.
    unsafe fn alignr<const N: i32>(hi: Self::V, lo: Self::V) -> Self::V;
    /// Full-width byte interleave with correct cross-lane ordering:
    /// returns `(lo, hi)` where `lo = [a0,b0,a1,b1,...]` for the first `BYTES/2`
    /// elements of `a`/`b` and `hi` continues with the second half.
    unsafe fn interleave(a: Self::V, b: Self::V) -> (Self::V, Self::V);
    /// Saturating pack of 16-bit lanes (per lane: 8 from `a` then 8 from `b`)
    /// **in full-width order** (i.e. the result's byte `i` is the `i`-th 16-bit
    /// element of `a:b` as a whole).
    unsafe fn pack_u16_u8(a: Self::V, b: Self::V) -> Self::V;

    // ---- reductions / masks ---------------------------------------------
    /// One bit per byte (bit 7 of each byte), byte `i` → bit `i`.
    unsafe fn movemask(v: Self::V) -> u32;
    /// True if every bit is zero.
    unsafe fn is_zero(v: Self::V) -> bool;

    // ---- codec-specific lane-crossing helpers ----------------------------------
    /// Base64 encode input load: place input bytes `[12*l .. 12*l + 16)` into
    /// lane `l` (the last 4 bytes of each lane are don't-care but must be
    /// readable). Reads [`B64_ENC_READ`](Self::B64_ENC_READ) bytes.
    unsafe fn load_b64_enc(p: *const u8) -> Self::V;
    /// Bytes read by `load_b64_enc` (16 for 128-bit, 28 for AVX2).
    const B64_ENC_READ: usize;
    /// Bytes consumed per `load_b64_enc` (12 per lane).
    const B64_ENC_IN: usize = 12 * Self::LANES;
    /// Base64 decode output store: each lane holds 12 valid bytes followed by 4
    /// zero bytes; store them contiguously (12 per lane) and up to `BYTES` bytes
    /// total may be written (trailing garbage beyond `12 * LANES`).
    unsafe fn store_b64_dec(p: *mut u8, v: Self::V);
    /// Bytes written by `store_b64_dec` (always `BYTES`).
    const B64_DEC_WRITE: usize = Self::BYTES;
    /// Per lane: write the low 8 bytes of each lane `l` to `p` after
    /// shuffling with a 16-byte index vector; used by the URL decoder's
    /// compaction. Implemented generically here via a temporary.
    unsafe fn lane_lo_u64(v: Self::V, lane: usize) -> u64 {
        let mut tmp = [0u8; 32];
        Self::storeu(tmp.as_mut_ptr(), v);
        u64::from_le_bytes(tmp[lane * 16..lane * 16 + 8].try_into().unwrap())
    }
    unsafe fn lane_hi_u64(v: Self::V, lane: usize) -> u64 {
        let mut tmp = [0u8; 32];
        Self::storeu(tmp.as_mut_ptr(), v);
        u64::from_le_bytes(tmp[lane * 16 + 8..lane * 16 + 16].try_into().unwrap())
    }
    /// Extract one 16-byte lane as a 128-bit value in a fresh vector's lane 0
    /// (other lanes unspecified). Backends with one lane return `v`.
    unsafe fn lane(v: Self::V, lane: usize) -> Self::V;
    /// Store only the low 16 bytes (lane 0).
    unsafe fn store_lane0(p: *mut u8, v: Self::V);
    /// Store 16 bytes of lane `lane` (0 or 1).
    unsafe fn store_lane(p: *mut u8, v: Self::V, lane: usize) {
        debug_assert_eq!(lane, 0);
        Self::store_lane0(p, v)
    }
    /// Build a vector whose lane 0 is `l0` and lane 1 (if any) is `l1`.
    unsafe fn from_lane_pair(l0: [u8; 16], _l1: [u8; 16]) -> Self::V {
        Self::from_lane(l0)
    }
}

/// Helper: replicate a 16-byte lane pattern across `N` lanes.
#[inline(always)]
pub const fn repl<const N: usize>(lane: [u8; 16]) -> [u8; N] {
    let mut out = [0u8; N];
    let mut i = 0;
    while i < N {
        out[i] = lane[i % 16];
        i += 1;
    }
    out
}
