//! Percent (URL) encoding — the hard one: variable-length output.
//!
//! **Encode**: classify every byte with two nibble lookups (`lo_lut[b & 15] &
//! hi_lut[b >> 4]`, exact for any ASCII set; bytes ≥ 0x80 always escape) and
//! take the `movemask`. In the common all-safe case the block is stored as-is
//! — no scalar fallback. Otherwise the block is expanded in 4-byte groups:
//! a 16-byte source vector `[in0..3 | hex_hi0..3 | hex_lo0..3 | '%' ...]` is
//! assembled with three constant shuffles and one 16-entry **shuffle table
//! indexed by the group's 4-bit escape mask** produces the 4–12 output bytes.
//! Output positions are the **prefix sums `4·g + 2·popcount(mask below g)`**,
//! computed with scalar `popcnt` on the movemask.
//!
//! **Decode**: load the block plus two lookahead loads shifted by 1 and 2
//! bytes, so a `%XX` whose digits lie in the *next* block is still decoded
//! here; the kernel then simply advances by `W + 1` or `W + 2` and continues
//! unaligned. Hex digits are validated and converted with the hex module's
//! range-check kernel, the decoded byte is blended into the `%` position, and
//! the two digit positions are removed with a 256-entry × 8-byte compaction
//! shuffle table per 8-byte half (positions again via popcount). Anything
//! malformed stops the kernel; the scalar unit path reports the exact offset.

use crate::hex::{hex_values_lut, nibble_value};
use crate::simd::Simd;
use crate::stream::{emit, Error, ErrorKind, Stash, Status, Step, Transform};
use crate::Backend;
use simdlab::Backends as _;

/// A set of ASCII bytes that are emitted verbatim; everything else is escaped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AsciiSet {
    /// bit `h` of `lo_lut[l]` ⇔ byte `(h << 4) | l` is safe (h < 8).
    pub(crate) lo_lut: [u8; 16],
    /// 1 = needs escaping (scalar fast path).
    escape: [u8; 256],
}

impl AsciiSet {
    /// Build from the list of safe ASCII bytes.
    pub const fn new(safe: &[u8]) -> AsciiSet {
        let mut lo_lut = [0u8; 16];
        let mut escape = [1u8; 256];
        let mut i = 0;
        while i < safe.len() {
            let b = safe[i];
            assert!(b < 128, "AsciiSet only holds ASCII");
            lo_lut[(b & 15) as usize] |= 1 << (b >> 4);
            escape[b as usize] = 0;
            i += 1;
        }
        AsciiSet { lo_lut, escape }
    }
    #[inline(always)]
    pub fn needs_escape(&self, b: u8) -> bool {
        self.escape[b as usize] != 0
    }
}

const ALNUM: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

const fn concat<const N: usize>(a: &[u8], b: &[u8]) -> [u8; N] {
    let mut out = [0u8; N];
    let mut i = 0;
    while i < a.len() {
        out[i] = a[i];
        i += 1;
    }
    let mut j = 0;
    while j < b.len() {
        out[i + j] = b[j];
        j += 1;
    }
    out
}

/// RFC 3986 unreserved: `A-Z a-z 0-9 - . _ ~` (what `encodeURIComponent`
/// does, minus `!*'()`).
pub const UNRESERVED: AsciiSet = AsciiSet::new(&concat::<66>(ALNUM, b"-._~"));
/// JavaScript `encodeURIComponent`: unreserved plus `! * ' ( )`.
pub const COMPONENT: AsciiSet = AsciiSet::new(&concat::<71>(ALNUM, b"-._~!*'()"));
/// `application/x-www-form-urlencoded` safe set (`*-._`), spaces → `+` is
/// *not* applied by the encoder; use the decoder's `plus_as_space` to accept it.
pub const FORM: AsciiSet = AsciiSet::new(&concat::<66>(ALNUM, b"*-._"));

pub(crate) const HEX_UPPER: &[u8; 16] = b"0123456789ABCDEF";
pub(crate) const HI_LUT: [u8; 16] = [1, 2, 4, 8, 16, 32, 64, 128, 0, 0, 0, 0, 0, 0, 0, 0];

// ---------------------------------------------------------------------------
// Scalar baseline
// ---------------------------------------------------------------------------

pub fn encode_scalar(input: &[u8], output: &mut [u8], set: &AsciiSet) -> (usize, usize) {
    let mut i = 0;
    let mut o = 0;
    while i < input.len() && o + 3 <= output.len() {
        let b = input[i];
        if set.escape[b as usize] == 0 {
            output[o] = b;
            o += 1;
        } else {
            output[o] = b'%';
            output[o + 1] = HEX_UPPER[(b >> 4) as usize];
            output[o + 2] = HEX_UPPER[(b & 15) as usize];
            o += 3;
        }
        i += 1;
    }
    (i, o)
}

/// Decode; stops before a malformed escape or when fewer than 3 input bytes remain.
pub fn decode_scalar(input: &[u8], output: &mut [u8], plus_as_space: bool) -> (usize, usize) {
    let mut i = 0;
    let mut o = 0;
    while i + 3 <= input.len() && o < output.len() {
        let b = input[i];
        if b == b'%' {
            match (nibble_value(input[i + 1]), nibble_value(input[i + 2])) {
                (Some(h), Some(l)) => {
                    output[o] = (h << 4) | l;
                    i += 3;
                }
                _ => break,
            }
        } else {
            output[o] = if plus_as_space && b == b'+' { b' ' } else { b };
            i += 1;
        }
        o += 1;
    }
    (i, o)
}

// ---------------------------------------------------------------------------
// Shuffle tables (built at compile time)
// ---------------------------------------------------------------------------

/// Encode expansion table: for a 4-bit escape mask, the 16-byte shuffle that
/// turns `[in0..3 | hi0..3 | lo0..3 | '%' x x x]` into the escaped output.
const EXPAND: [[u8; 16]; 16] = {
    let mut t = [[0x80u8; 16]; 16];
    let mut m = 0;
    while m < 16 {
        let mut o = 0;
        let mut k = 0;
        while k < 4 {
            if (m >> k) & 1 == 1 {
                t[m][o] = 12; // '%'
                t[m][o + 1] = 4 + k as u8; // hi
                t[m][o + 2] = 8 + k as u8; // lo
                o += 3;
            } else {
                t[m][o] = k as u8;
                o += 1;
            }
            k += 1;
        }
        m += 1;
    }
    t
};

/// Constant shuffles that move group `g`'s 4 bytes of (input | hex-hi | hex-lo)
/// to positions 0..4 | 4..8 | 8..12 of the assembly vector.
const fn group_idx(g: usize, dst: usize) -> [u8; 16] {
    let mut t = [0x80u8; 16];
    let mut k = 0;
    while k < 4 {
        t[dst + k] = (4 * g + k) as u8;
        k += 1;
    }
    t
}
const PCT_AT_12: [u8; 16] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, b'%', 0, 0, 0];

/// Decode compaction tables: for an 8-bit *keep* mask, the 16-byte shuffle
/// that packs the kept bytes of the low (resp. high) 8-byte half of a lane to
/// the front (0x80 = zero fill). 2 × 4 KiB.
const fn compress_table(hi: bool) -> [[u8; 16]; 256] {
    let mut t = [[0x80u8; 16]; 256];
    let mut m = 0;
    while m < 256 {
        let mut o = 0;
        let mut k = 0;
        while k < 8 {
            if (m >> k) & 1 == 1 {
                t[m][o] = k as u8 + if hi { 8 } else { 0 };
                o += 1;
            }
            k += 1;
        }
        m += 1;
    }
    t
}
static COMPRESS_LO: [[u8; 16]; 256] = compress_table(false);
static COMPRESS_HI: [[u8; 16]; 256] = compress_table(true);

// ---------------------------------------------------------------------------
// Generic vector kernels
// ---------------------------------------------------------------------------

/// Vectorised percent-encode. Requires `3*W + 16` bytes of output room per block.
#[inline(always)]
pub unsafe fn encode_kernel<S: Simd>(
    input: &[u8],
    output: &mut [u8],
    set: &AsciiSet,
) -> (usize, usize) {
    let w = S::BYTES;
    let lo_lut = S::from_lane(set.lo_lut);
    let hi_lut = S::from_lane(HI_LUT);
    let hex = S::from_lane(*HEX_UPPER);
    let nib = S::splat(0x0F);
    let zero = S::zero();
    let pct = S::from_lane(PCT_AT_12);
    let idx_in = [
        S::from_lane(group_idx(0, 0)),
        S::from_lane(group_idx(1, 0)),
        S::from_lane(group_idx(2, 0)),
        S::from_lane(group_idx(3, 0)),
    ];
    let idx_hi = [
        S::from_lane(group_idx(0, 4)),
        S::from_lane(group_idx(1, 4)),
        S::from_lane(group_idx(2, 4)),
        S::from_lane(group_idx(3, 4)),
    ];
    let idx_lo = [
        S::from_lane(group_idx(0, 8)),
        S::from_lane(group_idx(1, 8)),
        S::from_lane(group_idx(2, 8)),
        S::from_lane(group_idx(3, 8)),
    ];
    let mut i = 0;
    let mut o = 0;
    while i + w <= input.len() && o + 3 * w + 16 <= output.len() {
        #[cfg(feature = "mca-markers")]
        core::arch::asm!("# LLVM-MCA-BEGIN url_encode", options(nomem, nostack));
        let v = S::loadu(input.as_ptr().add(i));
        let hi_n = S::and(S::shr_u16::<4>(v), nib);
        let lo_n = S::and(v, nib);
        let cls = S::and(
            S::shuffle_bytes(lo_lut, lo_n),
            S::shuffle_bytes(hi_lut, hi_n),
        );
        let esc = S::cmpeq_u8(cls, zero);
        let m = S::movemask(esc);
        i += w;
        if m == 0 {
            // Common case: nothing to escape, straight copy.
            S::storeu(output.as_mut_ptr().add(o), v);
            o += w;
            #[cfg(feature = "mca-markers")]
            core::arch::asm!("# LLVM-MCA-END url_encode", options(nomem, nostack));
            continue;
        }
        let hex_hi = S::shuffle_bytes(hex, hi_n);
        let hex_lo = S::shuffle_bytes(hex, lo_n);
        // Output offsets are prefix sums of the escape mask: lane 1's bytes
        // start after all of lane 0's (16 + 2 * popcount(lane-0 mask)).
        let mut outs = [S::zero(); 4];
        for g in 0..4 {
            // Assemble [in | hi | lo | %] for group g — in every lane at once —
            // and expand with the per-lane table entry for that lane's mask.
            let src = S::or(
                S::or(
                    S::shuffle_bytes(v, idx_in[g]),
                    S::shuffle_bytes(hex_hi, idx_hi[g]),
                ),
                S::or(S::shuffle_bytes(hex_lo, idx_lo[g]), pct),
            );
            let mg0 = ((m >> (4 * g)) & 15) as usize;
            let idx = if S::LANES == 1 {
                S::from_lane(EXPAND[mg0])
            } else {
                S::from_lane_pair(EXPAND[mg0], EXPAND[((m >> (16 + 4 * g)) & 15) as usize])
            };
            outs[g] = S::shuffle_bytes(src, idx);
        }
        // Every store writes 16 bytes of which 4..=12 are valid, so stores must
        // be issued in output order (later stores overwrite earlier garbage).
        // The output offset of group k is the prefix sum 4k + 2·popcount(mask
        // below k). Computing each one *independently* from `m` (masked popcnt)
        // instead of as a running total removes a serial popcnt→add→address
        // chain that otherwise caps the escape path at ~1 group per 3 cycles.
        for lane in 0..S::LANES {
            for g in 0..4 {
                let k = 4 * lane + g;
                let below = m & ((1u32 << (4 * k)) - 1);
                let off = 4 * k + 2 * below.count_ones() as usize;
                S::store_lane(output.as_mut_ptr().add(o + off), outs[g], lane);
            }
        }
        o += w + 2 * m.count_ones() as usize;
        #[cfg(feature = "mca-markers")]
        core::arch::asm!("# LLVM-MCA-END url_encode", options(nomem, nostack));
    }
    (i, o)
}

/// Vectorised percent-decode. Requires `W + 2` input bytes (lookahead) and
/// `W` output bytes per block.
#[inline(always)]
pub unsafe fn decode_kernel<S: Simd>(
    input: &[u8],
    output: &mut [u8],
    plus_as_space: bool,
) -> (usize, usize) {
    let w = S::BYTES;
    let pct = S::splat(b'%');
    let plus = S::splat(b'+');
    let plus_xor = S::splat(b'+' ^ b' ');
    let wmask: u64 = if w == 64 { u64::MAX } else { (1u64 << w) - 1 };
    let mut i = 0;
    let mut o = 0;
    while i + w + 2 <= input.len() && o + w <= output.len() {
        #[cfg(feature = "mca-markers")]
        core::arch::asm!("# LLVM-MCA-BEGIN url_decode", options(nomem, nostack));
        let p = input.as_ptr().add(i);
        let v = S::loadu(p);
        let is_pct = S::cmpeq_u8(v, pct);
        let m = S::movemask(is_pct) as u64;
        let base = if plus_as_space {
            S::xor(v, S::and(S::cmpeq_u8(v, plus), plus_xor))
        } else {
            v
        };
        if m == 0 {
            S::storeu(output.as_mut_ptr().add(o), base);
            i += w;
            o += w;
            #[cfg(feature = "mca-markers")]
            core::arch::asm!("# LLVM-MCA-END url_decode", options(nomem, nostack));
            continue;
        }
        // Lookahead loads: v1[k] = in[k+1], v2[k] = in[k+2] — valid for all k
        // because we required w + 2 bytes; this is what lets a `%XX` at the end
        // of the block be decoded here even though its digits lie in the next.
        let v1 = S::loadu(p.add(1));
        let v2 = S::loadu(p.add(2));
        let (h1, bad1) = hex_values_lut::<S>(v1);
        let (h2, bad2) = hex_values_lut::<S>(v2);
        if !S::is_zero(S::and(is_pct, S::or(bad1, bad2))) {
            break; // malformed escape: let the scalar path report it
        }
        let val = S::or(S::shl_u16::<4>(h1), h2);
        let merged = S::or(S::and(is_pct, val), S::andnot(base, is_pct));
        // Drop the two digit positions after every '%'.
        let drop = ((m << 1) | (m << 2)) & wmask;
        let keep = !drop & wmask;
        // Compact 8 bytes at a time through the 256-entry tables. As in the
        // encoder, each half's output offset is an independent masked popcount
        // (kept bytes before it) rather than a running sum.
        for lane in 0..S::LANES {
            let ml = S::lane(merged, lane);
            for h in 0..2 {
                let k = 2 * lane + h;
                let kh = ((keep >> (8 * k)) & 0xFF) as usize;
                let off = (keep & ((1u64 << (8 * k)) - 1)).count_ones() as usize;
                let table = if h == 0 { &COMPRESS_LO } else { &COMPRESS_HI };
                let c = S::lane_lo_u64(S::shuffle_bytes(ml, S::from_lane(table[kh])), 0);
                core::ptr::write_unaligned(output.as_mut_ptr().add(o + off) as *mut u64, c.to_le());
            }
        }
        o += keep.count_ones() as usize;
        // Escapes whose digits spill past the block consume extra input.
        let extra = if (m >> (w - 1)) & 1 == 1 {
            2
        } else if (m >> (w - 2)) & 1 == 1 {
            1
        } else {
            0
        };
        i += w + extra;
        #[cfg(feature = "mca-markers")]
        core::arch::asm!("# LLVM-MCA-END url_decode", options(nomem, nostack));
    }
    (i, o)
}

crate::define_dispatch!(
    encode_bulk,
    encode_kernel,
    fn(input: &[u8], output: &mut [u8], set: &AsciiSet) -> (usize, usize),
    scalar = encode_scalar,
    avx512 = crate::native::vbmi2::encode_avx512
);
crate::define_dispatch!(
    decode_bulk,
    decode_kernel,
    fn(input: &[u8], output: &mut [u8], plus_as_space: bool) -> (usize, usize),
    scalar = decode_scalar,
    avx512 = crate::native::vbmi2::decode_avx512
);

// ---------------------------------------------------------------------------
// Streaming transforms
// ---------------------------------------------------------------------------

/// Streaming percent-encoder. State: ≤3 stashed output bytes.
#[derive(Clone, Debug)]
pub struct UrlEncoder {
    backend: Backend,
    set: AsciiSet,
    stash: Stash,
}

impl UrlEncoder {
    pub fn new(backend: Backend, set: AsciiSet) -> Self {
        Self {
            backend,
            set,
            stash: Stash::default(),
        }
    }
}

impl Transform for UrlEncoder {
    fn update(&mut self, input: &[u8], output: &mut [u8]) -> Step {
        let mut o = self.stash.flush(output);
        if !self.stash.is_empty() {
            return Step {
                consumed: 0,
                written: o,
                status: Status::OutputFull,
            };
        }
        let (c, w) = encode_bulk(self.backend, input, &mut output[o..], &self.set);
        let mut i = c;
        o += w;
        while i < input.len() {
            let b = input[i];
            i += 1;
            let ok = if self.set.escape[b as usize] == 0 {
                emit(output, &mut o, &[b], &mut self.stash)
            } else {
                emit(
                    output,
                    &mut o,
                    &[
                        b'%',
                        HEX_UPPER[(b >> 4) as usize],
                        HEX_UPPER[(b & 15) as usize],
                    ],
                    &mut self.stash,
                )
            };
            if !ok {
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
        3 * input_len + 3
    }
}

/// Streaming percent-decoder. State: a partial `%X` carried across chunks.
#[derive(Clone, Debug)]
pub struct UrlDecoder {
    backend: Backend,
    plus_as_space: bool,
    carry: [u8; 3],
    carry_len: u8,
    pos: usize,
    err: Option<Error>,
}

impl UrlDecoder {
    pub fn new(backend: Backend, plus_as_space: bool) -> Self {
        Self {
            backend,
            plus_as_space,
            carry: [0; 3],
            carry_len: 0,
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

impl Transform for UrlDecoder {
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
        // Complete a carried `%` / `%X`.
        while self.carry_len > 0 && i < input.len() {
            if o >= output.len() {
                self.pos += i;
                return Step {
                    consumed: i,
                    written: o,
                    status: Status::OutputFull,
                };
            }
            self.carry[self.carry_len as usize] = input[i];
            self.carry_len += 1;
            i += 1;
            if self.carry_len == 3 {
                self.carry_len = 0;
                let start = self.pos + i - 3;
                let h = match nibble_value(self.carry[1]) {
                    Some(h) => h,
                    None => return self.fail(ErrorKind::InvalidByte, start + 1, i, o),
                };
                let l = match nibble_value(self.carry[2]) {
                    Some(l) => l,
                    None => return self.fail(ErrorKind::InvalidByte, start + 2, i, o),
                };
                output[o] = (h << 4) | l;
                o += 1;
            }
        }
        let (c, w) = decode_bulk(
            self.backend,
            &input[i..],
            &mut output[o..],
            self.plus_as_space,
        );
        i += c;
        o += w;
        while i < input.len() {
            if o >= output.len() {
                self.pos += i;
                return Step {
                    consumed: i,
                    written: o,
                    status: Status::OutputFull,
                };
            }
            let b = input[i];
            if b == b'%' {
                if i + 3 > input.len() {
                    // carry the partial escape
                    let n = input.len() - i;
                    self.carry[..n].copy_from_slice(&input[i..]);
                    self.carry_len = n as u8;
                    i = input.len();
                    break;
                }
                let h = match nibble_value(input[i + 1]) {
                    Some(h) => h,
                    None => return self.fail(ErrorKind::InvalidByte, self.pos + i + 1, i, o),
                };
                let l = match nibble_value(input[i + 2]) {
                    Some(l) => l,
                    None => return self.fail(ErrorKind::InvalidByte, self.pos + i + 2, i, o),
                };
                output[o] = (h << 4) | l;
                i += 3;
            } else {
                output[o] = if self.plus_as_space && b == b'+' {
                    b' '
                } else {
                    b
                };
                i += 1;
            }
            o += 1;
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
        if self.carry_len > 0 {
            return self.fail(ErrorKind::Truncated, self.pos, 0, 0);
        }
        Step {
            consumed: 0,
            written: 0,
            status: Status::Done,
        }
    }
    fn reset(&mut self) {
        self.carry_len = 0;
        self.pos = 0;
        self.err = None;
    }
    fn max_output_len(&self, input_len: usize) -> usize {
        input_len + 1
    }
}

// ---------------------------------------------------------------------------
// One-shot convenience
// ---------------------------------------------------------------------------

pub fn encode(input: &[u8]) -> String {
    encode_with(Backend::best(), &UNRESERVED, input)
}

pub fn encode_with(backend: Backend, set: &AsciiSet, input: &[u8]) -> String {
    let v = crate::stream::run_to_vec(&mut UrlEncoder::new(backend, *set), input).unwrap();
    unsafe { String::from_utf8_unchecked(v) }
}

pub fn decode(input: &[u8]) -> Result<Vec<u8>, Error> {
    decode_with(Backend::best(), false, input)
}

pub fn decode_with(backend: Backend, plus_as_space: bool, input: &[u8]) -> Result<Vec<u8>, Error> {
    crate::stream::run_to_vec(&mut UrlDecoder::new(backend, plus_as_space), input)
}
