//! Hexadecimal — the easy one.
//!
//! **Encode**: split each byte into nibbles (`v >> 4`, `v & 15`), look both up
//! in a 16-entry table with one byte shuffle each, and interleave. 16 input
//! bytes → 32 output bytes in ~7 vector instructions.
//!
//! **Decode**: classify each ASCII byte by range check (`d = v - '0' <= 9`,
//! `l = (v | 0x20) - 'a' <= 5`), select the value, OR the two class masks to
//! get validity, then combine nibble pairs with one multiply-add (`hi*16 + lo`)
//! and pack 16-bit → 8-bit. Any invalid byte in a block makes the kernel stop
//! *before* the block; the scalar unit path then pinpoints the error.

use crate::simd::Simd;
use crate::stream::{emit, Error, ErrorKind, Stash, Status, Step, Transform};
use crate::Backend;
use simdlab::Backends as _;

pub(crate) const LOWER: &[u8; 16] = b"0123456789abcdef";
pub(crate) const UPPER: &[u8; 16] = b"0123456789ABCDEF";

/// 256-entry decode table; 0xFF = invalid.
const DECODE_TABLE: [u8; 256] = {
    let mut t = [0xFFu8; 256];
    let mut i = 0;
    while i < 10 {
        t[b'0' as usize + i] = i as u8;
        i += 1;
    }
    let mut i = 0;
    while i < 6 {
        t[b'a' as usize + i] = 10 + i as u8;
        t[b'A' as usize + i] = 10 + i as u8;
        i += 1;
    }
    t
};

#[inline(always)]
pub fn nibble_value(b: u8) -> Option<u8> {
    let v = DECODE_TABLE[b as usize];
    if v == 0xFF {
        None
    } else {
        Some(v)
    }
}

// ---------------------------------------------------------------------------
// Scalar baseline
// ---------------------------------------------------------------------------

/// Encode as many bytes as fit. Returns `(consumed, written)`.
pub fn encode_scalar(input: &[u8], output: &mut [u8], upper: bool) -> (usize, usize) {
    let table = if upper { UPPER } else { LOWER };
    let n = input.len().min(output.len() / 2);
    for (i, &b) in input[..n].iter().enumerate() {
        output[2 * i] = table[(b >> 4) as usize];
        output[2 * i + 1] = table[(b & 15) as usize];
    }
    (n, 2 * n)
}

/// Decode whole pairs; stops before the first invalid pair.
pub fn decode_scalar(input: &[u8], output: &mut [u8]) -> (usize, usize) {
    let n = (input.len() / 2).min(output.len());
    let mut i = 0;
    while i < n {
        let hi = DECODE_TABLE[input[2 * i] as usize];
        let lo = DECODE_TABLE[input[2 * i + 1] as usize];
        if (hi | lo) == 0xFF {
            // one of them invalid (0xFF has all bits set; valid values < 16)
            break;
        }
        output[i] = (hi << 4) | lo;
        i += 1;
    }
    (2 * i, i)
}

// ---------------------------------------------------------------------------
// Generic vector kernels
// ---------------------------------------------------------------------------

/// Vectorised encode of whole `S::BYTES` blocks.
#[inline(always)]
pub unsafe fn encode_kernel<S: Simd>(
    input: &[u8],
    output: &mut [u8],
    upper: bool,
) -> (usize, usize) {
    let w = S::BYTES;
    let table = S::from_lane(*if upper { UPPER } else { LOWER });
    let nib = S::splat(0x0F);
    let mut i = 0;
    let mut o = 0;
    while i + w <= input.len() && o + 2 * w <= output.len() {
        #[cfg(feature = "mca-markers")]
        core::arch::asm!("# LLVM-MCA-BEGIN hex_encode", options(nomem, nostack));
        let v = S::loadu(input.as_ptr().add(i));
        let hi = S::and(S::shr_u16::<4>(v), nib);
        let lo = S::and(v, nib);
        let hi = S::shuffle_bytes(table, hi);
        let lo = S::shuffle_bytes(table, lo);
        let (a, b) = S::interleave(hi, lo);
        S::storeu(output.as_mut_ptr().add(o), a);
        S::storeu(output.as_mut_ptr().add(o + w), b);
        #[cfg(feature = "mca-markers")]
        core::arch::asm!("# LLVM-MCA-END hex_encode", options(nomem, nostack));
        i += w;
        o += 2 * w;
    }
    (i, o)
}

/// `x <= k` per byte (unsigned), as an all-ones mask.
#[inline(always)]
pub(crate) unsafe fn le_const<S: Simd>(x: S::V, k: u8) -> S::V {
    S::cmpeq_u8(S::subs_u8(x, S::splat(k)), S::zero())
}

/// Classify + convert one vector of ASCII hex digits.
/// Returns `(values, invalid_mask)`.
#[inline(always)]
pub(crate) unsafe fn hex_values<S: Simd>(v: S::V) -> (S::V, S::V) {
    let d = S::sub_u8(v, S::splat(b'0'));
    let is_digit = le_const::<S>(d, 9);
    let l = S::sub_u8(S::or(v, S::splat(0x20)), S::splat(b'a'));
    let is_alpha = le_const::<S>(l, 5);
    let val = S::or(
        S::and(d, is_digit),
        S::and(S::add_u8(l, S::splat(10)), is_alpha),
    );
    let valid = S::or(is_digit, is_alpha);
    (val, S::xor(valid, S::splat(0xFF)))
}

/// [`hex_values_lut`] tables: bits per high nibble (1 = 0x3_, 2 = 0x4_/0x6_, 4 = the rest).
pub(crate) const HEX_CLASS_HI: [u8; 16] = [4, 4, 4, 1, 2, 4, 2, 4, 4, 4, 4, 4, 4, 4, 4, 4];
/// Per low nibble, the classes for which it is invalid: class 1 (0x3_) when l > 9, class 2
/// (0x4_/0x6_) when l == 0 or l > 6, class 4 always.
pub(crate) const HEX_CLASS_LO: [u8; 16] = {
    let mut t = [0u8; 16];
    let mut l = 0;
    while l < 16 {
        t[l] = 4 | if l > 9 { 1 } else { 0 } | if l == 0 || l > 6 { 2 } else { 0 };
        l += 1;
    }
    t
};
/// Added to the low nibble: 9 for the letter classes.
pub(crate) const HEX_ADD: [u8; 16] = [0, 0, 0, 0, 9, 0, 9, 0, 0, 0, 0, 0, 0, 0, 0, 0];

/// Nibble-LUT classification + conversion (the alternative to [`hex_values`]).
///
/// `lo_lut[lo] & hi_lut[hi] != 0` ⇔ invalid (same trick as the base64
/// decoder: one bit per distinct "valid low nibbles" pattern of a high
/// nibble — `0x3_` has `0..=9`, `0x4_`/`0x6_` have `1..=6`, the rest nothing),
/// and `value = lo + add_lut[hi]` with `add_lut[4] = add_lut[6] = 9`.
/// 7 vector ops per 16 bytes with no compare chains for LLVM to duplicate.
#[inline(always)]
pub(crate) unsafe fn hex_values_lut<S: Simd>(v: S::V) -> (S::V, S::V) {
    const HI: [u8; 16] = HEX_CLASS_HI;
    const LO: [u8; 16] = HEX_CLASS_LO;
    const ADD: [u8; 16] = HEX_ADD;
    let nib = S::splat(0x0F);
    let hi = S::and(S::shr_u16::<4>(v), nib);
    let lo = S::and(v, nib);
    let bad = S::and(
        S::shuffle_bytes(S::from_lane(LO), lo),
        S::shuffle_bytes(S::from_lane(HI), hi),
    );
    let val = S::add_u8(lo, S::shuffle_bytes(S::from_lane(ADD), hi));
    (val, bad)
}

/// Vectorised decode of whole `2*S::BYTES` → `S::BYTES` blocks using the
/// range-check classifier (`RANGE = true`) or the nibble LUTs (`RANGE = false`).
#[inline(always)]
pub unsafe fn decode_kernel_impl<S: Simd, const RANGE: bool>(
    input: &[u8],
    output: &mut [u8],
) -> (usize, usize) {
    let w = S::BYTES;
    let mut i = 0;
    let mut o = 0;
    while i + 2 * w <= input.len() && o + w <= output.len() {
        #[cfg(feature = "mca-markers")]
        core::arch::asm!("# LLVM-MCA-BEGIN hex_decode", options(nomem, nostack));
        let a = S::loadu(input.as_ptr().add(i));
        let b = S::loadu(input.as_ptr().add(i + w));
        let (va, ia) = if RANGE {
            hex_values::<S>(a)
        } else {
            hex_values_lut::<S>(a)
        };
        let (vb, ib) = if RANGE {
            hex_values::<S>(b)
        } else {
            hex_values_lut::<S>(b)
        };
        if !S::is_zero(S::or(ia, ib)) {
            break;
        }
        // [h0 l0 h1 l1 ...] -> u16: h*16 + l
        let pa = S::madd_u8_pairs(va, 16, 1);
        let pb = S::madd_u8_pairs(vb, 16, 1);
        let packed = S::pack_u16_u8(pa, pb);
        S::storeu(output.as_mut_ptr().add(o), packed);
        #[cfg(feature = "mca-markers")]
        core::arch::asm!("# LLVM-MCA-END hex_decode", options(nomem, nostack));
        i += 2 * w;
        o += w;
    }
    (i, o)
}

#[inline(always)]
pub unsafe fn decode_kernel<S: Simd>(input: &[u8], output: &mut [u8]) -> (usize, usize) {
    decode_kernel_impl::<S, true>(input, output)
}
#[inline(always)]
pub unsafe fn decode_kernel_lut<S: Simd>(input: &[u8], output: &mut [u8]) -> (usize, usize) {
    decode_kernel_impl::<S, false>(input, output)
}

crate::define_dispatch!(
    encode_bulk,
    encode_kernel,
    fn(input: &[u8], output: &mut [u8], upper: bool) -> (usize, usize),
    scalar = encode_scalar,
    neon = crate::native::neon::hex_encode
);
crate::define_dispatch!(
    decode_bulk_range,
    decode_kernel,
    fn(input: &[u8], output: &mut [u8]) -> (usize, usize),
    scalar = decode_scalar,
    neon = crate::native::neon::hex_decode
);
crate::define_dispatch!(
    decode_bulk_lut,
    decode_kernel_lut,
    fn(input: &[u8], output: &mut [u8]) -> (usize, usize),
    scalar = decode_scalar,
    neon = crate::native::neon::hex_decode
);

/// Which vector classifier the hex decoder uses. See REPORT.md: the LUT
/// variant is ~1.5× faster on AVX2 because the range-check version's compare
/// masks get rematerialised by LLVM after the validity branch. Backends with a native hex decoder
/// (`neon`) ignore it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Classifier {
    /// `d = v - '0' <= 9`, `l = (v | 0x20) - 'a' <= 5`, select + OR validity.
    RangeCheck,
    /// Two nibble lookups for validity, one for the value offset (default).
    NibbleLut,
}

#[inline]
pub fn decode_bulk(
    backend: Backend,
    input: &[u8],
    output: &mut [u8],
    classifier: Classifier,
) -> (usize, usize) {
    match classifier {
        Classifier::RangeCheck => decode_bulk_range(backend, input, output),
        Classifier::NibbleLut => decode_bulk_lut(backend, input, output),
    }
}

// ---------------------------------------------------------------------------
// Streaming transforms
// ---------------------------------------------------------------------------

/// Streaming hex encoder. State: a ≤2-byte output stash.
#[derive(Clone, Debug)]
pub struct HexEncoder {
    backend: Backend,
    upper: bool,
    stash: Stash,
}

impl HexEncoder {
    pub fn new(backend: Backend, upper: bool) -> Self {
        Self {
            backend,
            upper,
            stash: Stash::default(),
        }
    }
}

impl Transform for HexEncoder {
    fn update(&mut self, input: &[u8], output: &mut [u8]) -> Step {
        let mut o = self.stash.flush(output);
        if !self.stash.is_empty() {
            return Step {
                consumed: 0,
                written: o,
                status: Status::OutputFull,
            };
        }
        let (c, w) = encode_bulk(self.backend, input, &mut output[o..], self.upper);
        let mut i = c;
        o += w;
        let table = if self.upper { UPPER } else { LOWER };
        while i < input.len() {
            let b = input[i];
            let unit = [table[(b >> 4) as usize], table[(b & 15) as usize]];
            i += 1;
            if !emit(output, &mut o, &unit, &mut self.stash) {
                return Step {
                    consumed: i,
                    written: o,
                    status: Status::OutputFull,
                };
            }
        }
        Step {
            consumed: i,
            written: o,
            status: Status::Done,
        }
    }
    fn finish(&mut self, output: &mut [u8]) -> Step {
        let o = self.stash.flush(output);
        let status = if self.stash.is_empty() {
            Status::Done
        } else {
            Status::OutputFull
        };
        Step {
            consumed: 0,
            written: o,
            status,
        }
    }
    fn reset(&mut self) {
        self.stash.clear();
    }
    fn max_output_len(&self, input_len: usize) -> usize {
        2 * input_len + 2
    }
}

/// Streaming hex decoder. State: an optional carried first nibble.
#[derive(Clone, Debug)]
pub struct HexDecoder {
    backend: Backend,
    classifier: Classifier,
    carry: Option<u8>,
    pos: usize,
    err: Option<Error>,
}

impl HexDecoder {
    pub fn new(backend: Backend) -> Self {
        Self::with_classifier(backend, Classifier::NibbleLut)
    }
    pub fn with_classifier(backend: Backend, classifier: Classifier) -> Self {
        Self {
            backend,
            classifier,
            carry: None,
            pos: 0,
            err: None,
        }
    }
    fn fail(&mut self, kind: ErrorKind, offset: usize, consumed: usize, written: usize) -> Step {
        let e = Error { kind, offset };
        self.err = Some(e);
        Step {
            consumed,
            written,
            status: Status::Error(e),
        }
    }
}

impl Transform for HexDecoder {
    fn update(&mut self, input: &[u8], output: &mut [u8]) -> Step {
        if let Some(e) = self.err {
            return Step {
                consumed: 0,
                written: 0,
                status: Status::Error(e),
            };
        }
        let mut i = 0;
        let mut o = 0;
        if let Some(hi) = self.carry {
            if input.is_empty() {
                return Step {
                    consumed: 0,
                    written: 0,
                    status: Status::Done,
                };
            }
            if output.is_empty() {
                return Step {
                    consumed: 0,
                    written: 0,
                    status: Status::OutputFull,
                };
            }
            match nibble_value(input[0]) {
                Some(lo) => output[0] = (hi << 4) | lo,
                None => return self.fail(ErrorKind::InvalidByte, self.pos, 0, 0),
            }
            self.carry = None;
            i = 1;
            o = 1;
        }
        let (c, w) = decode_bulk(self.backend, &input[i..], &mut output[o..], self.classifier);
        i += c;
        o += w;
        while i + 2 <= input.len() {
            if o >= output.len() {
                self.pos += i;
                return Step {
                    consumed: i,
                    written: o,
                    status: Status::OutputFull,
                };
            }
            let hi = DECODE_TABLE[input[i] as usize];
            let lo = DECODE_TABLE[input[i + 1] as usize];
            if hi == 0xFF {
                return self.fail(ErrorKind::InvalidByte, self.pos + i, i, o);
            }
            if lo == 0xFF {
                return self.fail(ErrorKind::InvalidByte, self.pos + i + 1, i, o);
            }
            output[o] = (hi << 4) | lo;
            i += 2;
            o += 1;
        }
        if i < input.len() {
            match nibble_value(input[i]) {
                Some(v) => self.carry = Some(v),
                None => return self.fail(ErrorKind::InvalidByte, self.pos + i, i, o),
            }
            i += 1;
        }
        self.pos += i;
        Step {
            consumed: i,
            written: o,
            status: Status::Done,
        }
    }
    fn finish(&mut self, _output: &mut [u8]) -> Step {
        if let Some(e) = self.err {
            return Step {
                consumed: 0,
                written: 0,
                status: Status::Error(e),
            };
        }
        if self.carry.is_some() {
            return self.fail(ErrorKind::Truncated, self.pos, 0, 0);
        }
        Step {
            consumed: 0,
            written: 0,
            status: Status::Done,
        }
    }
    fn reset(&mut self) {
        self.carry = None;
        self.pos = 0;
        self.err = None;
    }
    fn max_output_len(&self, input_len: usize) -> usize {
        input_len / 2 + 1
    }
}

// ---------------------------------------------------------------------------
// One-shot convenience
// ---------------------------------------------------------------------------

pub fn encode(input: &[u8]) -> String {
    encode_with(Backend::best(), input, false)
}

pub fn encode_with(backend: Backend, input: &[u8], upper: bool) -> String {
    let v = crate::stream::run_to_vec(&mut HexEncoder::new(backend, upper), input).unwrap();
    // SAFETY: output is ASCII.
    unsafe { String::from_utf8_unchecked(v) }
}

pub fn decode(input: &[u8]) -> Result<Vec<u8>, Error> {
    decode_with(Backend::best(), input)
}

pub fn decode_with(backend: Backend, input: &[u8]) -> Result<Vec<u8>, Error> {
    crate::stream::run_to_vec(&mut HexDecoder::new(backend), input)
}
