//! AVX-512 VBMI2 percent codec (REPORT.md §7).
//!
//! The generic kernels pay for variable-length output: the encoder expands 4-byte groups through
//! a mask-indexed shuffle table, issues overlapping 16-byte stores in order and needs `3W + 16`
//! bytes of output slack; the decoder compacts through eight table lookups per 64 bytes. AVX-512
//! with VBMI2 removes all of that:
//!
//! * **encode**, per 16 input bytes: a `vpermi2b` and a `vpermb` spread `[byte | hex hi | hex lo]`
//!   into 48 slots (three per byte), a blend writes `%` into the first slot of each escaped byte,
//!   and `vpcompressb` keeps one slot per safe byte and all three per escaped byte. The keep
//!   mask is `pdep(escapes, every third bit)` widened to the two following slots. A masked store
//!   writes exactly `16 + 2 * escapes` bytes. An all-safe 64-byte block is one plain store.
//! * **decode**, per 64 input bytes: the digits after each `%` come from loads shifted by 1 and 2
//!   (as in the generic kernel), a blend puts the decoded byte on the `%`, and one `vpcompressb`
//!   drops the two digit positions; one masked store.
//! * masked stores never touch bytes past the result, so the output check is exact: no slack.
//!
//! Byte classification and hex conversion stay per-lane `vpshufb` lookups as in the generic
//! kernels, with k-masks (`vptestmb`, `vpcmpeqb`) instead of `movemask`.
//!
//! The kernels are written against [`Ops512`], the 64-byte operations they need, so the tests
//! can run them on [`Emu512`] (plain arrays: the operations' reference semantics). The development
//! machine has no AVX-512 and QEMU does not emulate it, so the emulation is what checks the
//! algorithm here; what remains unverified is the mapping of each operation to its intrinsic in
//! the `Avx512` impl at the bottom (one intrinsic per method).

use crate::hex::{HEX_ADD, HEX_CLASS_HI, HEX_CLASS_LO};
use crate::simd::repl;
use crate::url::{AsciiSet, HEX_UPPER, HI_LUT};

/// The 64-byte operations of the percent kernels, with `u64` k-masks.
pub trait Ops512: Copy {
    type V: Copy;
    unsafe fn loadu(self, p: *const u8) -> Self::V;
    unsafe fn storeu(self, p: *mut u8, v: Self::V);
    /// Store the bytes whose bit is set in `k`; the others are not accessed.
    unsafe fn store_masked(self, p: *mut u8, k: u64, v: Self::V);
    fn from_array(self, a: [u8; 64]) -> Self::V;
    fn splat(self, b: u8) -> Self::V;
    fn and(self, a: Self::V, b: Self::V) -> Self::V;
    fn or(self, a: Self::V, b: Self::V) -> Self::V;
    fn add_u8(self, a: Self::V, b: Self::V) -> Self::V;
    /// Per 16-bit lane `>> 4` / `<< 4` (bits move across the byte boundary, as with `vpsrlw`).
    fn shr4_u16(self, a: Self::V) -> Self::V;
    fn shl4_u16(self, a: Self::V) -> Self::V;
    /// `vpshufb`: per 16-byte lane `l`, `idx & 0x80 ? 0 : table[16 l + (idx & 15)]`.
    fn shuffle_lanes(self, table: Self::V, idx: Self::V) -> Self::V;
    /// `vpermb`: `a[idx & 63]`.
    fn permute(self, idx: Self::V, a: Self::V) -> Self::V;
    /// `vpermi2b`: `(idx & 64 ? b : a)[idx & 63]`.
    fn permute2(self, a: Self::V, idx: Self::V, b: Self::V) -> Self::V;
    /// `vpcmpeqb` into a k-mask.
    fn eq_mask(self, a: Self::V, b: Self::V) -> u64;
    /// `vptestmb`: bit `i` set iff `a[i] & b[i] != 0`.
    fn test_mask(self, a: Self::V, b: Self::V) -> u64;
    /// `vpblendmb`: `k ? b : a`.
    fn blend(self, k: u64, a: Self::V, b: Self::V) -> Self::V;
    /// `vpcompressb` (zero-masking): the bytes whose bit is set in `k`, packed to the front.
    fn compress(self, k: u64, a: Self::V) -> Self::V;
    /// `pdep`: the low bits of `src`, deposited at the set bits of `mask`.
    fn pdep(self, src: u64, mask: u64) -> u64;
}

/// The low `n` bits (`n <= 64`).
#[inline(always)]
fn low_bits(n: usize) -> u64 {
    if n >= 64 { !0 } else { (1u64 << n) - 1 }
}

/// Bit `3j` for `j < 16`: the first of each input byte's three output slots.
const FIRST: u64 = {
    let mut m = 0u64;
    let mut j = 0;
    while j < 16 {
        m |= 1 << (3 * j);
        j += 1;
    }
    m
};
/// The third slot of each input byte.
const THIRD: u64 = FIRST << 2;

/// `vpermi2b` indices spreading chunk `c` (input bytes `16c .. 16c + 16`): slot `3j` takes the
/// input byte, slot `3j + 1` its high hex digit (second operand, `+ 64`).
const fn spread_byte_hi(c: usize) -> [u8; 64] {
    let mut t = [0u8; 64];
    let mut j = 0;
    while j < 16 {
        t[3 * j] = (16 * c + j) as u8;
        t[3 * j + 1] = (64 + 16 * c + j) as u8;
        j += 1;
    }
    t
}
/// `vpermb` indices: slot `3j + 2` takes the low hex digit of input byte `16c + j`.
const fn spread_lo(c: usize) -> [u8; 64] {
    let mut t = [0u8; 64];
    let mut j = 0;
    while j < 16 {
        t[3 * j + 2] = (16 * c + j) as u8;
        j += 1;
    }
    t
}

/// Percent-encode whole 64-byte blocks while their output fits. Returns `(consumed, written)`.
#[inline(always)]
pub unsafe fn encode<O: Ops512>(o: O, input: &[u8], output: &mut [u8], set: &AsciiSet) -> (usize, usize) {
    let lo_lut = o.from_array(repl::<64>(set.lo_lut));
    let hi_lut = o.from_array(repl::<64>(HI_LUT));
    let hex = o.from_array(repl::<64>(*HEX_UPPER));
    let nib = o.splat(0x0F);
    let pct = o.splat(b'%');
    let byte_hi = [0, 1, 2, 3].map(|c| o.from_array(spread_byte_hi(c)));
    let lo_idx = [0, 1, 2, 3].map(|c| o.from_array(spread_lo(c)));
    let (mut i, mut n) = (0, 0);
    while i + 64 <= input.len() {
        let v = o.loadu(input.as_ptr().add(i));
        let hi = o.and(o.shr4_u16(v), nib);
        let lo = o.and(v, nib);
        let esc = !o.test_mask(o.shuffle_lanes(lo_lut, lo), o.shuffle_lanes(hi_lut, hi));
        let len = 64 + 2 * esc.count_ones() as usize;
        if n + len > output.len() {
            break;
        }
        let out = output.as_mut_ptr().add(n);
        if esc == 0 {
            o.storeu(out, v);
        } else {
            let hex_hi = o.shuffle_lanes(hex, hi);
            let hex_lo = o.shuffle_lanes(hex, lo);
            let mut off = 0;
            for c in 0..4 {
                let e = (esc >> (16 * c)) & 0xFFFF;
                let slots = o.blend(THIRD, o.permute2(v, byte_hi[c], hex_hi), o.permute(lo_idx[c], hex_lo));
                let pct_slots = o.pdep(e, FIRST);
                let slots = o.blend(pct_slots, slots, pct);
                let keep = FIRST | (pct_slots << 1) | (pct_slots << 2);
                let k = 16 + 2 * e.count_ones() as usize;
                o.store_masked(out.add(off), low_bits(k), o.compress(keep, slots));
                off += k;
            }
        }
        i += 64;
        n += len;
    }
    (i, n)
}

/// Percent-decode whole 64-byte blocks (plus a 2-byte lookahead) while their output fits; stops
/// before a block with a malformed escape. Returns `(consumed, written)`.
#[inline(always)]
pub unsafe fn decode<O: Ops512>(o: O, input: &[u8], output: &mut [u8], plus_as_space: bool) -> (usize, usize) {
    let class_hi = o.from_array(repl::<64>(HEX_CLASS_HI));
    let class_lo = o.from_array(repl::<64>(HEX_CLASS_LO));
    let add = o.from_array(repl::<64>(HEX_ADD));
    let nib = o.splat(0x0F);
    let high_nib = o.splat(0xF0);
    let (pct, plus, space) = (o.splat(b'%'), o.splat(b'+'), o.splat(b' '));
    // (digit value, k-mask of non-hex bytes), as hex::hex_values_lut
    let hex_values = |x: O::V| {
        let h = o.and(o.shr4_u16(x), nib);
        let l = o.and(x, nib);
        let bad = o.test_mask(o.shuffle_lanes(class_lo, l), o.shuffle_lanes(class_hi, h));
        (o.add_u8(l, o.shuffle_lanes(add, h)), bad)
    };
    let (mut i, mut n) = (0, 0);
    while i + 66 <= input.len() {
        let p = input.as_ptr().add(i);
        let v = o.loadu(p);
        let m = o.eq_mask(v, pct);
        let base = if plus_as_space { o.blend(o.eq_mask(v, plus), v, space) } else { v };
        if m == 0 {
            if n + 64 > output.len() {
                break;
            }
            o.storeu(output.as_mut_ptr().add(n), base);
            i += 64;
            n += 64;
            continue;
        }
        let (h1, bad1) = hex_values(o.loadu(p.add(1)));
        let (h2, bad2) = hex_values(o.loadu(p.add(2)));
        if (bad1 | bad2) & m != 0 {
            break; // malformed escape: the scalar path reports it
        }
        let val = o.or(o.and(o.shl4_u16(h1), high_nib), h2);
        let keep = !((m << 1) | (m << 2));
        let len = keep.count_ones() as usize;
        if n + len > output.len() {
            break;
        }
        o.store_masked(output.as_mut_ptr().add(n), low_bits(len), o.compress(keep, o.blend(m, base, val)));
        n += len;
        // escapes whose digits lie past the block consume them too
        i += 64 + if m >> 63 != 0 { 2 } else if (m >> 62) & 1 != 0 { 1 } else { 0 };
    }
    (i, n)
}

#[cfg(target_arch = "x86_64")]
pub use avx512::{decode_avx512, encode_avx512};

#[cfg(target_arch = "x86_64")]
mod avx512 {
    use super::Ops512;
    use crate::backend::Avx512;
    use crate::url::AsciiSet;
    use core::arch::x86_64::*;

    /// [`super::encode`] on AVX-512. Safety: the CPU supports [`Avx512`]'s features.
    #[inline(always)]
    pub unsafe fn encode_avx512(input: &[u8], output: &mut [u8], set: &AsciiSet) -> (usize, usize) {
        super::encode(Avx512::new_unchecked(), input, output, set)
    }
    /// [`super::decode`] on AVX-512. Safety: the CPU supports [`Avx512`]'s features.
    #[inline(always)]
    pub unsafe fn decode_avx512(input: &[u8], output: &mut [u8], plus_as_space: bool) -> (usize, usize) {
        super::decode(Avx512::new_unchecked(), input, output, plus_as_space)
    }

    // SAFETY (every unsafe block below): an `Avx512` token exists only if the CPU supports
    // AVX-512 F/BW/VBMI/VBMI2 and BMI2.
    impl Ops512 for Avx512 {
        type V = __m512i;
        #[inline(always)]
        unsafe fn loadu(self, p: *const u8) -> __m512i {
            _mm512_loadu_si512(p.cast())
        }
        #[inline(always)]
        unsafe fn storeu(self, p: *mut u8, v: __m512i) {
            _mm512_storeu_si512(p.cast(), v)
        }
        #[inline(always)]
        unsafe fn store_masked(self, p: *mut u8, k: u64, v: __m512i) {
            _mm512_mask_storeu_epi8(p.cast(), k, v)
        }
        #[inline(always)]
        fn from_array(self, a: [u8; 64]) -> __m512i {
            unsafe { _mm512_loadu_si512(a.as_ptr().cast()) }
        }
        #[inline(always)]
        fn splat(self, b: u8) -> __m512i {
            unsafe { _mm512_set1_epi8(b as i8) }
        }
        #[inline(always)]
        fn and(self, a: __m512i, b: __m512i) -> __m512i {
            unsafe { _mm512_and_si512(a, b) }
        }
        #[inline(always)]
        fn or(self, a: __m512i, b: __m512i) -> __m512i {
            unsafe { _mm512_or_si512(a, b) }
        }
        #[inline(always)]
        fn add_u8(self, a: __m512i, b: __m512i) -> __m512i {
            unsafe { _mm512_add_epi8(a, b) }
        }
        #[inline(always)]
        fn shr4_u16(self, a: __m512i) -> __m512i {
            unsafe { _mm512_srli_epi16::<4>(a) }
        }
        #[inline(always)]
        fn shl4_u16(self, a: __m512i) -> __m512i {
            unsafe { _mm512_slli_epi16::<4>(a) }
        }
        #[inline(always)]
        fn shuffle_lanes(self, table: __m512i, idx: __m512i) -> __m512i {
            unsafe { _mm512_shuffle_epi8(table, idx) }
        }
        #[inline(always)]
        fn permute(self, idx: __m512i, a: __m512i) -> __m512i {
            unsafe { _mm512_permutexvar_epi8(idx, a) }
        }
        #[inline(always)]
        fn permute2(self, a: __m512i, idx: __m512i, b: __m512i) -> __m512i {
            unsafe { _mm512_permutex2var_epi8(a, idx, b) }
        }
        #[inline(always)]
        fn eq_mask(self, a: __m512i, b: __m512i) -> u64 {
            unsafe { _mm512_cmpeq_epi8_mask(a, b) }
        }
        #[inline(always)]
        fn test_mask(self, a: __m512i, b: __m512i) -> u64 {
            unsafe { _mm512_test_epi8_mask(a, b) }
        }
        #[inline(always)]
        fn blend(self, k: u64, a: __m512i, b: __m512i) -> __m512i {
            unsafe { _mm512_mask_blend_epi8(k, a, b) }
        }
        #[inline(always)]
        fn compress(self, k: u64, a: __m512i) -> __m512i {
            unsafe { _mm512_maskz_compress_epi8(k, a) }
        }
        #[inline(always)]
        fn pdep(self, src: u64, mask: u64) -> u64 {
            unsafe { _pdep_u64(src, mask) }
        }
    }
}

/// [`Ops512`] on plain arrays: the reference semantics the tests run the kernels with.
#[cfg(test)]
#[derive(Clone, Copy, Debug)]
pub struct Emu512;

#[cfg(test)]
impl Ops512 for Emu512 {
    type V = [u8; 64];
    unsafe fn loadu(self, p: *const u8) -> [u8; 64] {
        core::ptr::read_unaligned(p as *const [u8; 64])
    }
    unsafe fn storeu(self, p: *mut u8, v: [u8; 64]) {
        core::ptr::write_unaligned(p as *mut [u8; 64], v)
    }
    unsafe fn store_masked(self, p: *mut u8, k: u64, v: [u8; 64]) {
        for (i, &b) in v.iter().enumerate() {
            if k >> i & 1 == 1 {
                *p.add(i) = b;
            }
        }
    }
    fn from_array(self, a: [u8; 64]) -> [u8; 64] {
        a
    }
    fn splat(self, b: u8) -> [u8; 64] {
        [b; 64]
    }
    fn and(self, a: [u8; 64], b: [u8; 64]) -> [u8; 64] {
        core::array::from_fn(|i| a[i] & b[i])
    }
    fn or(self, a: [u8; 64], b: [u8; 64]) -> [u8; 64] {
        core::array::from_fn(|i| a[i] | b[i])
    }
    fn add_u8(self, a: [u8; 64], b: [u8; 64]) -> [u8; 64] {
        core::array::from_fn(|i| a[i].wrapping_add(b[i]))
    }
    fn shr4_u16(self, a: [u8; 64]) -> [u8; 64] {
        let w = |i: usize| u16::from_le_bytes([a[i & !1], a[i | 1]]) >> 4;
        core::array::from_fn(|i| w(i).to_le_bytes()[i & 1])
    }
    fn shl4_u16(self, a: [u8; 64]) -> [u8; 64] {
        let w = |i: usize| u16::from_le_bytes([a[i & !1], a[i | 1]]) << 4;
        core::array::from_fn(|i| w(i).to_le_bytes()[i & 1])
    }
    fn shuffle_lanes(self, table: [u8; 64], idx: [u8; 64]) -> [u8; 64] {
        core::array::from_fn(|i| if idx[i] & 0x80 != 0 { 0 } else { table[(i & !15) + (idx[i] & 15) as usize] })
    }
    fn permute(self, idx: [u8; 64], a: [u8; 64]) -> [u8; 64] {
        core::array::from_fn(|i| a[(idx[i] & 63) as usize])
    }
    fn permute2(self, a: [u8; 64], idx: [u8; 64], b: [u8; 64]) -> [u8; 64] {
        core::array::from_fn(|i| if idx[i] & 64 != 0 { b } else { a }[(idx[i] & 63) as usize])
    }
    fn eq_mask(self, a: [u8; 64], b: [u8; 64]) -> u64 {
        (0..64).fold(0, |m, i| m | ((a[i] == b[i]) as u64) << i)
    }
    fn test_mask(self, a: [u8; 64], b: [u8; 64]) -> u64 {
        (0..64).fold(0, |m, i| m | ((a[i] & b[i] != 0) as u64) << i)
    }
    fn blend(self, k: u64, a: [u8; 64], b: [u8; 64]) -> [u8; 64] {
        core::array::from_fn(|i| if k >> i & 1 == 1 { b[i] } else { a[i] })
    }
    fn compress(self, k: u64, a: [u8; 64]) -> [u8; 64] {
        let mut r = [0u8; 64];
        let mut n = 0;
        for (i, &b) in a.iter().enumerate() {
            if k >> i & 1 == 1 {
                r[n] = b;
                n += 1;
            }
        }
        r
    }
    fn pdep(self, mut src: u64, mask: u64) -> u64 {
        let mut r = 0;
        for i in 0..64 {
            if mask >> i & 1 == 1 {
                r |= (src & 1) << i;
                src >>= 1;
            }
        }
        r
    }
}

#[cfg(test)]
mod tests {
    use super::{decode, encode, Emu512};
    use crate::stream::run_to_vec;
    use crate::url::{UrlDecoder, UrlEncoder, COMPONENT, FORM, UNRESERVED};
    use crate::Backend;
    use simdlab::rng::SplitMix64;
    use simdlab::testing::iterations;

    const CANARY: u8 = 0xA5;

    fn pick(r: &mut SplitMix64, from: &[u8]) -> u8 {
        from[(r.next_u64() % from.len() as u64) as usize]
    }
    /// Mostly bytes from `from`, with one in `noise` arbitrary.
    fn gen(r: &mut SplitMix64, n: usize, from: &[u8], noise: u64) -> Vec<u8> {
        (0..n).map(|_| if r.next_u64() % noise == 0 { r.next_u64() as u8 } else { pick(r, from) }).collect()
    }

    fn scalar_encode(set: crate::url::AsciiSet, input: &[u8]) -> Vec<u8> {
        run_to_vec(&mut UrlEncoder::new(Backend::Scalar, set), input).unwrap()
    }

    /// The kernel's output equals the scalar encoding of what it consumed, it never writes past
    /// the output slice, and it only stops early when the next block's encoding does not fit.
    #[test]
    fn encode_matches_scalar() {
        let mut r = SplitMix64::new(11);
        for it in 0..iterations(3000) {
            let n = (r.next_u64() % 400) as usize;
            let input = match it % 4 {
                0 => (0..n).map(|_| r.next_u64() as u8).collect(),
                1 => gen(&mut r, n, b"abcdefghijklmnopqrstuvwxyz", 8),
                2 => gen(&mut r, n, b"abcdefghij0123456789-._~", 1 << 40),
                _ => gen(&mut r, n, b"ab /?&=%+", 50),
            };
            let set = [UNRESERVED, COMPONENT, FORM][it % 3];
            let cap = if r.next_u64() % 2 == 0 { 3 * n } else { (r.next_u64() % (3 * n as u64 + 1)) as usize };
            let mut out = vec![CANARY; cap + 64];
            let (i, w) = unsafe { encode(Emu512, &input, &mut out[..cap], &set) };
            assert!(i % 64 == 0 && i <= n && w <= cap, "it={it} i={i} w={w} n={n} cap={cap}");
            assert_eq!(&out[..w], &scalar_encode(set, &input[..i])[..], "it={it}");
            assert!(out[cap..].iter().all(|&b| b == CANARY), "it={it}: wrote past the output");
            if i + 64 <= n {
                assert!(w + scalar_encode(set, &input[i..i + 64]).len() > cap, "it={it}: stopped early at {i}");
            }
        }
    }

    /// The kernel's output equals the scalar decoding of what it consumed (which must not end
    /// inside an escape), and it never writes past the output slice.
    #[test]
    fn decode_matches_scalar() {
        let mut r = SplitMix64::new(12);
        for it in 0..iterations(3000) {
            let n = (r.next_u64() % 500) as usize;
            let input = match it % 3 {
                0 => gen(&mut r, n, b"abc%%0123456789ABCDEFabcdef+", 30),
                1 => gen(&mut r, n, b"abcdefgh%4142+", 1 << 40),
                _ => gen(&mut r, n, b"%%%41", 20),
            };
            let plus = it % 2 == 0;
            let cap = if r.next_u64() % 2 == 0 { n } else { (r.next_u64() % (n as u64 + 1)) as usize };
            let mut out = vec![CANARY; cap + 64];
            let (i, w) = unsafe { decode(Emu512, &input, &mut out[..cap], plus) };
            assert!(i <= n && w <= cap, "it={it}");
            let expect = run_to_vec(&mut UrlDecoder::new(Backend::Scalar, plus), &input[..i]);
            assert_eq!(expect, Ok(out[..w].to_vec()), "it={it} i={i}");
            assert!(out[cap..].iter().all(|&b| b == CANARY), "it={it}: wrote past the output");
        }
    }

    /// On well-formed input with room to spare, decoding stops only for the 66-byte window.
    #[test]
    fn decode_consumes_wellformed_input() {
        let mut r = SplitMix64::new(13);
        for it in 0..iterations(500) {
            let mut input = Vec::new();
            let n = (r.next_u64() % 600) as usize;
            while input.len() < n {
                if r.next_u64() % 4 == 0 {
                    input.extend_from_slice(format!("%{:02X}", r.next_u64() as u8).as_bytes());
                } else {
                    input.push(pick(&mut r, b"abcdef+"));
                }
            }
            let mut out = vec![0u8; input.len()];
            let (i, w) = unsafe { decode(Emu512, &input, &mut out, false) };
            assert!(i + 66 > input.len(), "it={it}: stopped at {i} of {}", input.len());
            let expect = run_to_vec(&mut UrlDecoder::new(Backend::Scalar, false), &input[..i]).unwrap();
            assert_eq!(&out[..w], &expect[..], "it={it}");
        }
    }

    /// `%XX` with the `%` on every position around the end of the first block.
    #[test]
    fn escapes_across_the_block_boundary() {
        for pos in 55..70 {
            let mut input = vec![b'a'; 200];
            input[pos..pos + 3].copy_from_slice(b"%41");
            let mut out = vec![0u8; 200];
            let (i, w) = unsafe { decode(Emu512, &input, &mut out, false) };
            let expect = run_to_vec(&mut UrlDecoder::new(Backend::Scalar, false), &input[..i]).unwrap();
            assert_eq!(&out[..w], &expect[..], "pos={pos}");
            assert!(i + 66 > input.len(), "pos={pos} i={i}");
        }
    }
}
