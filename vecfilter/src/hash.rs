//! Key hashing, written once in terms of 32-bit multiply/xor/shift so the same function can run
//! per-key (scalar) and 8-keys-at-a-time (vector) and give bit-identical results. A 64-bit key
//! is split into two 32-bit halves; two 32-bit outputs are produced:
//!   * `hb` selects the block (Bloom) or bucket (cuckoo)
//!   * `hl` seeds the per-lane bit positions (Bloom) or fingerprint (cuckoo)
//!
//! Why not a 64-bit mixer (wyhash/murmur fmix64)? AVX2 has no 64x64->64 multiply (`vpmullq` is
//! AVX-512DQ) and NEON has none either, so a 64-bit mixer would need a 4-instruction emulation
//! per multiply. 32-bit multiplies are native on both. See vecfilter/REPORT.md.

use crate::simd::Ops;

const FOLD_A: u32 = 0x85EBCA77;
const FOLD_B: u32 = 0xC2B2AE3D;
const M1: u32 = 0x7FEB352D; // lowbias32
const M2: u32 = 0x846CA68B;
const M3: u32 = 0xED5AD4BB; // triple32 (first two rounds)
const M4: u32 = 0xAC4C1B51;

/// Scalar hash: `(hb, hl)`.
#[inline(always)]
pub fn hash1(key: u64) -> (u32, u32) {
    let (lo, hi) = (key as u32, (key >> 32) as u32);
    let mut a = lo ^ hi.wrapping_mul(FOLD_A);
    a ^= a >> 16;
    a = a.wrapping_mul(M1);
    a ^= a >> 15;
    a = a.wrapping_mul(M2);
    a ^= a >> 16;
    let mut b = hi ^ lo.wrapping_mul(FOLD_B) ^ a;
    b ^= b >> 17;
    b = b.wrapping_mul(M3);
    b ^= b >> 11;
    b = b.wrapping_mul(M4);
    b ^= b >> 15;
    (a, b)
}

/// 8-wide hash of split keys: `(hb, hl)`, lane i bit-identical to `hash1` of key i.
#[inline(always)]
pub fn hash8<O: Ops>(o: O, lo: O::V32, hi: O::V32) -> (O::V32, O::V32) {
    let mut a = o.xor_32(lo, o.mullo_32(hi, o.set1_32(FOLD_A)));
    a = o.xor_32(a, o.shr_32::<16>(a));
    a = o.mullo_32(a, o.set1_32(M1));
    a = o.xor_32(a, o.shr_32::<15>(a));
    a = o.mullo_32(a, o.set1_32(M2));
    a = o.xor_32(a, o.shr_32::<16>(a));
    let mut b = o.xor_32(o.xor_32(hi, o.mullo_32(lo, o.set1_32(FOLD_B))), a);
    b = o.xor_32(b, o.shr_32::<17>(b));
    b = o.mullo_32(b, o.set1_32(M3));
    b = o.xor_32(b, o.shr_32::<11>(b));
    b = o.mullo_32(b, o.set1_32(M4));
    b = o.xor_32(b, o.shr_32::<15>(b));
    (a, b)
}

/// Hash 8 keys from memory: split halves, mix.
#[inline(always)]
pub fn hash8_keys<O: Ops>(o: O, keys: &[u64; 8]) -> (O::V32, O::V32) {
    let (lo, hi) = o.split_keys8(keys);
    hash8(o, lo, hi)
}

/// Lemire's fast range reduction: maps a uniform 32-bit value to [0, n).
#[inline(always)]
pub fn fastrange32(x: u32, n: u32) -> u32 {
    ((x as u64 * n as u64) >> 32) as u32
}

/// Salts for the per-lane multiply-shift (one odd constant per lane). These are the constants
/// used by Impala/Parquet split-block Bloom filters.
pub const LANE_SALTS: [u32; 8] =
    [0x47b6137b, 0x44974d91, 0x8824ad5b, 0xa2b7289d, 0x705495c7, 0x2df1424b, 0x9efc4947, 0x5c6bfb31];
pub const LANE_SALTS2: [u32; 8] =
    [0x6a7f2d6b, 0x3b9d1e3d, 0x9c3f1b27, 0x58f4a8c5, 0xd1c6e3f9, 0x1f0b39b3, 0x7e9d6a71, 0xb3e27f05];
