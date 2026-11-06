//! Base64 (standard and URL-safe alphabets) — the medium one.
//!
//! **Encode** (Muła/Lemire): load 12 bytes per 16-byte lane, byte-shuffle so
//! every 32-bit word holds `[b, a, c, b]`, then isolate the four sextets with
//! two masks and move them into place with one `mulhi_u16` and one
//! `mullo_u16` (multiplication by powers of two = per-16-bit-lane shifts, which
//! SSE/AVX2 lack). The 6-bit indices become ASCII with a 13-entry offset table
//! indexed by `saturating_sub(idx, 51) + (idx > 25)`.
//!
//! **Decode**: two nibble lookups give a per-byte validity bitmask (`lo_lut[lo]
//! & hi_lut[hi] != 0` ⇒ invalid), a third lookup on the high nibble gives the
//! ASCII→value offset ("roll"), then `pmaddubsw`/`pmaddwd` fold 4 sextets into
//! 24 bits and a byte shuffle compacts 16 → 12 bytes per lane. All three
//! lookup tables are **derived at compile time from the alphabet** by a
//! `const fn`, so the URL-safe alphabet costs nothing extra.
//!
//! Padding and the tail (1–3 trailing bytes/chars) are handled by the scalar
//! unit path of the streaming driver; the kernels only ever see whole blocks
//! of valid symbols and simply stop at the first block containing anything
//! else (including `=`).

use crate::simd::Simd;
use crate::stream::{emit, Error, ErrorKind, Stash, Status, Step, Transform};
use crate::Backend;
use simdlab::Backends as _;

/// A base64 alphabet. Only RFC 4648 §4 (standard) and §5 (URL-safe) are
/// provided; the compile-time LUT derivation assumes `A-Za-z0-9` + 2 symbols.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alphabet {
    Standard,
    UrlSafe,
}

const STD_SYMBOLS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const URL_SYMBOLS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

impl Alphabet {
    #[inline(always)]
    pub const fn symbols(self) -> &'static [u8; 64] {
        match self {
            Alphabet::Standard => STD_SYMBOLS,
            Alphabet::UrlSafe => URL_SYMBOLS,
        }
    }
    #[inline(always)]
    pub(crate) const fn tables(self) -> &'static Tables {
        match self {
            Alphabet::Standard => &STD_TABLES,
            Alphabet::UrlSafe => &URL_TABLES,
        }
    }
}

/// All lookup tables for one alphabet, derived at compile time.
#[derive(Debug)]
pub struct Tables {
    /// ASCII → value, 0xFF invalid, 0xFE for '='.
    pub(crate) decode: [u8; 256],
    /// Encode: offset added to a 6-bit index, indexed by the reduced index.
    enc_lut: [u8; 16],
    /// Decode validity: `lo_lut[v & 15] & hi_lut[v >> 4] != 0` ⇒ invalid.
    lo_lut: [u8; 16],
    hi_lut: [u8; 16],
    /// Decode roll: `value = v + roll_lut[(v >> 4) | (v == special ? 8 : 0)]`.
    roll_lut: [u8; 16],
    /// The one symbol whose roll differs from the rest of its high-nibble class
    /// (`/` for standard, `_` for URL-safe — both index 63).
    special: u8,
}

const fn build_tables(sym: &[u8; 64]) -> Tables {
    let mut decode = [0xFFu8; 256];
    let mut i = 0;
    while i < 64 {
        decode[sym[i] as usize] = i as u8;
        i += 1;
    }
    decode[b'=' as usize] = 0xFE;

    // Encode LUT (assumes A-Z a-z 0-9 ordering).
    let mut enc_lut = [0u8; 16];
    enc_lut[0] = sym[0]; // idx 0..=25 : +65
    enc_lut[1] = sym[26].wrapping_sub(26); // idx 26..=51 : +71
    let mut k = 2;
    while k < 12 {
        enc_lut[k] = sym[52].wrapping_sub(52); // idx 52..=61 : -4
        k += 1;
    }
    enc_lut[12] = sym[62].wrapping_sub(62);
    enc_lut[13] = sym[63].wrapping_sub(63);

    // Validity patterns per high nibble, one bit per distinct pattern.
    let mut pattern = [0u16; 16];
    let mut h = 0;
    while h < 16 {
        let mut l = 0;
        while l < 16 {
            if decode[(h << 4) | l] < 64 {
                pattern[h] |= 1 << l;
            }
            l += 1;
        }
        h += 1;
    }
    let mut hi_lut = [0u8; 16];
    let mut seen = [0u16; 8];
    let mut seen_bits = [0u8; 8];
    let mut nseen = 0;
    let mut h = 0;
    while h < 16 {
        let mut j = 0;
        let mut bit = 0u8;
        while j < nseen {
            if seen[j] == pattern[h] {
                bit = seen_bits[j];
            }
            j += 1;
        }
        if bit == 0 {
            assert!(nseen < 8, "alphabet needs more than 8 validity classes");
            bit = 1 << nseen;
            seen[nseen] = pattern[h];
            seen_bits[nseen] = bit;
            nseen += 1;
        }
        hi_lut[h] = bit;
        h += 1;
    }
    let mut lo_lut = [0u8; 16];
    let mut l = 0;
    while l < 16 {
        let mut h = 0;
        while h < 16 {
            if decode[(h << 4) | l] >= 64 {
                lo_lut[l] |= hi_lut[h];
            }
            h += 1;
        }
        l += 1;
    }

    // Roll table.
    let special = sym[63];
    let mut roll_lut = [0u8; 16];
    let mut h = 0;
    while h < 16 {
        let mut roll: i32 = -1;
        let mut l = 0;
        while l < 16 {
            let b = ((h << 4) | l) as u8;
            let v = decode[b as usize];
            if v < 64 && b != special {
                let r = v.wrapping_sub(b) as i32;
                assert!(
                    roll == -1 || roll == r,
                    "inconsistent roll within high-nibble class"
                );
                roll = r;
            }
            l += 1;
        }
        roll_lut[h] = if roll < 0 { 0 } else { roll as u8 };
        h += 1;
    }
    roll_lut[((special >> 4) | 8) as usize] = 63u8.wrapping_sub(special);

    Tables {
        decode,
        enc_lut,
        lo_lut,
        hi_lut,
        roll_lut,
        special,
    }
}

static STD_TABLES: Tables = build_tables(STD_SYMBOLS);
static URL_TABLES: Tables = build_tables(URL_SYMBOLS);

// ---------------------------------------------------------------------------
// Scalar baseline
// ---------------------------------------------------------------------------

/// Encode whole 3-byte groups. Returns `(consumed, written)`.
pub fn encode_scalar(input: &[u8], output: &mut [u8], alphabet: Alphabet) -> (usize, usize) {
    let sym = alphabet.symbols();
    let n = (input.len() / 3).min(output.len() / 4);
    for g in 0..n {
        let a = input[3 * g] as u32;
        let b = input[3 * g + 1] as u32;
        let c = input[3 * g + 2] as u32;
        let x = (a << 16) | (b << 8) | c;
        output[4 * g] = sym[(x >> 18) as usize & 63];
        output[4 * g + 1] = sym[(x >> 12) as usize & 63];
        output[4 * g + 2] = sym[(x >> 6) as usize & 63];
        output[4 * g + 3] = sym[x as usize & 63];
    }
    (3 * n, 4 * n)
}

/// Decode whole 4-char groups of valid symbols; stops before the first group
/// containing anything else (including `=`).
pub fn decode_scalar(input: &[u8], output: &mut [u8], alphabet: Alphabet) -> (usize, usize) {
    let t = &alphabet.tables().decode;
    let n = (input.len() / 4).min(output.len() / 3);
    let mut g = 0;
    while g < n {
        let v0 = t[input[4 * g] as usize] as u32;
        let v1 = t[input[4 * g + 1] as usize] as u32;
        let v2 = t[input[4 * g + 2] as usize] as u32;
        let v3 = t[input[4 * g + 3] as usize] as u32;
        if (v0 | v1 | v2 | v3) & 0xC0 != 0 {
            break;
        }
        let x = (v0 << 18) | (v1 << 12) | (v2 << 6) | v3;
        output[3 * g] = (x >> 16) as u8;
        output[3 * g + 1] = (x >> 8) as u8;
        output[3 * g + 2] = x as u8;
        g += 1;
    }
    (4 * g, 3 * g)
}

// ---------------------------------------------------------------------------
// Generic vector kernels
// ---------------------------------------------------------------------------

/// Byte shuffle that turns 12 input bytes into 16 bytes `[b,a,c,b]` per word.
const ENC_SHUF: [u8; 16] = [1, 0, 2, 1, 4, 3, 5, 4, 7, 6, 8, 7, 10, 9, 11, 10];
/// Compaction shuffle for decode: 24-bit words `[c,b,a,·]` → `[a,b,c]`.
const DEC_PACK: [u8; 16] = [
    2, 1, 0, 6, 5, 4, 10, 9, 8, 14, 13, 12, 0x80, 0x80, 0x80, 0x80,
];

#[inline(always)]
pub unsafe fn encode_kernel<S: Simd>(
    input: &[u8],
    output: &mut [u8],
    alphabet: Alphabet,
) -> (usize, usize) {
    let t = alphabet.tables();
    let shuf = S::from_lane(ENC_SHUF);
    let lut = S::from_lane(t.enc_lut);
    let m0 = S::splat_u32(0x0fc0_fc00);
    let m2 = S::splat_u32(0x003f_03f0);
    // The multipliers are powers of two per 16-bit lane. If LLVM can see that,
    // it "strength-reduces" the single `vpmulhuw`/`vpmullw` into a 7- and a
    // 3-instruction shift/blend sequence (there is no per-lane-variable 16-bit
    // shift before AVX-512BW). Hiding the constants behind `black_box` keeps
    // the multiplies: ~30% faster on AVX2. See REPORT.md.
    let m1 = S::splat_u32(0x0400_0040);
    let m3 = core::hint::black_box(S::splat_u32(0x0100_0010));
    let c51 = S::splat(51);
    let c25 = S::splat(25);
    let mut i = 0;
    let mut o = 0;
    while i + S::B64_ENC_READ <= input.len() && o + S::BYTES <= output.len() {
        #[cfg(feature = "mca-markers")]
        core::arch::asm!("# LLVM-MCA-BEGIN b64_encode", options(nomem, nostack));
        let v = S::load_b64_enc(input.as_ptr().add(i));
        let v = S::shuffle_bytes(v, shuf);
        // bit repacking: idx = [s0 s1 s2 s3] per 32-bit word
        let t0 = S::and(v, m0);
        let t1 = S::mulhi_u16(t0, m1);
        let t2 = S::and(v, m2);
        let t3 = S::mullo_u16(t2, m3);
        let idx = S::or(t1, t3);
        // 6-bit index -> ASCII via 13-entry offset table
        let red = S::subs_u8(idx, c51);
        let gt = S::cmpgt_i8(idx, c25);
        let red = S::sub_u8(red, gt);
        let ascii = S::add_u8(idx, S::shuffle_bytes(lut, red));
        S::storeu(output.as_mut_ptr().add(o), ascii);
        #[cfg(feature = "mca-markers")]
        core::arch::asm!("# LLVM-MCA-END b64_encode", options(nomem, nostack));
        i += S::B64_ENC_IN;
        o += S::BYTES;
    }
    (i, o)
}

#[inline(always)]
pub unsafe fn decode_kernel<S: Simd>(
    input: &[u8],
    output: &mut [u8],
    alphabet: Alphabet,
) -> (usize, usize) {
    let t = alphabet.tables();
    let lo_lut = S::from_lane(t.lo_lut);
    let hi_lut = S::from_lane(t.hi_lut);
    let roll_lut = S::from_lane(t.roll_lut);
    let pack = S::from_lane(DEC_PACK);
    let special = S::splat(t.special);
    let nib = S::splat(0x0F);
    let eight = S::splat(8);
    let out_per = 12 * S::LANES;
    let mut i = 0;
    let mut o = 0;
    while i + S::BYTES <= input.len() && o + S::B64_DEC_WRITE <= output.len() {
        #[cfg(feature = "mca-markers")]
        core::arch::asm!("# LLVM-MCA-BEGIN b64_decode", options(nomem, nostack));
        let v = S::loadu(input.as_ptr().add(i));
        let hi = S::and(S::shr_u16::<4>(v), nib);
        let lo = S::and(v, nib);
        let bad = S::and(S::shuffle_bytes(lo_lut, lo), S::shuffle_bytes(hi_lut, hi));
        if !S::is_zero(bad) {
            break;
        }
        let sel = S::or(hi, S::and(S::cmpeq_u8(v, special), eight));
        let vals = S::add_u8(v, S::shuffle_bytes(roll_lut, sel));
        // [s0 s1 s2 s3] -> 24-bit words
        let m1 = S::madd_u8_pairs(vals, 0x40, 0x01);
        let m2 = S::madd_u16_pairs(m1, 0x1000, 0x0001);
        let packed = S::shuffle_bytes(m2, pack);
        S::store_b64_dec(output.as_mut_ptr().add(o), packed);
        #[cfg(feature = "mca-markers")]
        core::arch::asm!("# LLVM-MCA-END b64_decode", options(nomem, nostack));
        i += S::BYTES;
        o += out_per;
    }
    (i, o)
}

crate::define_dispatch!(
    encode_bulk,
    encode_kernel,
    fn(input: &[u8], output: &mut [u8], alphabet: Alphabet) -> (usize, usize),
    scalar = encode_scalar,
    neon = crate::native::neon::base64_encode
);
crate::define_dispatch!(
    decode_bulk,
    decode_kernel,
    fn(input: &[u8], output: &mut [u8], alphabet: Alphabet) -> (usize, usize),
    scalar = decode_scalar,
    neon = crate::native::neon::base64_decode
);

// ---------------------------------------------------------------------------
// Streaming transforms
// ---------------------------------------------------------------------------

#[inline(always)]
fn encode_group(sym: &[u8; 64], a: u8, b: u8, c: u8) -> [u8; 4] {
    let x = ((a as u32) << 16) | ((b as u32) << 8) | c as u32;
    [
        sym[(x >> 18) as usize & 63],
        sym[(x >> 12) as usize & 63],
        sym[(x >> 6) as usize & 63],
        sym[x as usize & 63],
    ]
}

/// Streaming base64 encoder. State: ≤2 carried input bytes, ≤4 stashed output bytes.
#[derive(Clone, Debug)]
pub struct Base64Encoder {
    backend: Backend,
    alphabet: Alphabet,
    pad: bool,
    carry: [u8; 3],
    carry_len: u8,
    stash: Stash,
    finished: bool,
}

impl Base64Encoder {
    pub fn new(backend: Backend, alphabet: Alphabet, pad: bool) -> Self {
        Self {
            backend,
            alphabet,
            pad,
            carry: [0; 3],
            carry_len: 0,
            stash: Stash::default(),
            finished: false,
        }
    }
}

impl Transform for Base64Encoder {
    fn update(&mut self, input: &[u8], output: &mut [u8]) -> Step {
        let sym = self.alphabet.symbols();
        let mut o = self.stash.flush(output);
        if !self.stash.is_empty() {
            return Step {
                consumed: 0,
                written: o,
                status: Status::OutputFull,
            };
        }
        let mut i = 0;
        // Complete a carried group first.
        while self.carry_len > 0 && i < input.len() {
            self.carry[self.carry_len as usize] = input[i];
            self.carry_len += 1;
            i += 1;
            if self.carry_len == 3 {
                self.carry_len = 0;
                let unit = encode_group(sym, self.carry[0], self.carry[1], self.carry[2]);
                if !emit(output, &mut o, &unit, &mut self.stash) {
                    return Step {
                        consumed: i,
                        written: o,
                        status: Status::OutputFull,
                    };
                }
            }
        }
        let (c, w) = encode_bulk(self.backend, &input[i..], &mut output[o..], self.alphabet);
        i += c;
        o += w;
        while i + 3 <= input.len() {
            let unit = encode_group(sym, input[i], input[i + 1], input[i + 2]);
            i += 3;
            if !emit(output, &mut o, &unit, &mut self.stash) {
                return Step {
                    consumed: i,
                    written: o,
                    status: Status::OutputFull,
                };
            }
        }
        while i < input.len() {
            self.carry[self.carry_len as usize] = input[i];
            self.carry_len += 1;
            i += 1;
        }
        Step {
            consumed: i,
            written: o,
            status: Status::Done,
        }
    }

    fn finish(&mut self, output: &mut [u8]) -> Step {
        if !self.finished {
            self.finished = true;
            let sym = self.alphabet.symbols();
            match self.carry_len {
                1 => {
                    let g = encode_group(sym, self.carry[0], 0, 0);
                    if self.pad {
                        self.stash.set(&[g[0], g[1], b'=', b'=']);
                    } else {
                        self.stash.set(&g[..2]);
                    }
                }
                2 => {
                    let g = encode_group(sym, self.carry[0], self.carry[1], 0);
                    if self.pad {
                        self.stash.set(&[g[0], g[1], g[2], b'=']);
                    } else {
                        self.stash.set(&g[..3]);
                    }
                }
                _ => {}
            }
            self.carry_len = 0;
        }
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
        self.carry_len = 0;
        self.stash.clear();
        self.finished = false;
    }

    fn max_output_len(&self, input_len: usize) -> usize {
        (input_len + 2) / 3 * 4 + 4
    }
}

/// Decode one quad. Returns `(bytes, n_out, saw_padding)` or the error kind and
/// the index of the offending character within the quad.
#[inline]
fn decode_quad(t: &[u8; 256], q: [u8; 4]) -> Result<([u8; 3], usize, bool), (ErrorKind, usize)> {
    let v0 = t[q[0] as usize];
    let v1 = t[q[1] as usize];
    for (k, v) in [v0, v1].iter().enumerate() {
        if *v >= 64 {
            return Err((
                if *v == 0xFE {
                    ErrorKind::InvalidPadding
                } else {
                    ErrorKind::InvalidByte
                },
                k,
            ));
        }
    }
    let v2 = t[q[2] as usize];
    if v2 == 0xFE {
        if q[3] != b'=' {
            return Err((ErrorKind::InvalidPadding, 3));
        }
        let x = ((v0 as u32) << 18) | ((v1 as u32) << 12);
        return Ok(([(x >> 16) as u8, 0, 0], 1, true));
    }
    if v2 >= 64 {
        return Err((ErrorKind::InvalidByte, 2));
    }
    let v3 = t[q[3] as usize];
    if v3 == 0xFE {
        let x = ((v0 as u32) << 18) | ((v1 as u32) << 12) | ((v2 as u32) << 6);
        return Ok(([(x >> 16) as u8, (x >> 8) as u8, 0], 2, true));
    }
    if v3 >= 64 {
        return Err((ErrorKind::InvalidByte, 3));
    }
    let x = ((v0 as u32) << 18) | ((v1 as u32) << 12) | ((v2 as u32) << 6) | v3 as u32;
    Ok(([(x >> 16) as u8, (x >> 8) as u8, x as u8], 3, false))
}

/// Streaming base64 decoder. Accepts padded or unpadded input; rejects
/// anything after padding. State: ≤3 carried chars, ≤3 stashed output bytes.
#[derive(Clone, Debug)]
pub struct Base64Decoder {
    backend: Backend,
    alphabet: Alphabet,
    carry: [u8; 4],
    carry_len: u8,
    stash: Stash,
    padded: bool,
    finished: bool,
    pos: usize,
    err: Option<Error>,
}

impl Base64Decoder {
    pub fn new(backend: Backend, alphabet: Alphabet) -> Self {
        Self {
            backend,
            alphabet,
            carry: [0; 4],
            carry_len: 0,
            stash: Stash::default(),
            padded: false,
            finished: false,
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

impl Transform for Base64Decoder {
    fn update(&mut self, input: &[u8], output: &mut [u8]) -> Step {
        if let Some(e) = self.err {
            return Step {
                consumed: 0,
                written: 0,
                status: Status::Error(e),
            };
        }
        let t = &self.alphabet.tables().decode;
        let mut o = self.stash.flush(output);
        if !self.stash.is_empty() {
            return Step {
                consumed: 0,
                written: o,
                status: Status::OutputFull,
            };
        }
        if self.padded && !input.is_empty() {
            return self.fail(ErrorKind::TrailingData, self.pos, 0, o);
        }
        let mut i = 0;
        while self.carry_len > 0 && i < input.len() {
            self.carry[self.carry_len as usize] = input[i];
            self.carry_len += 1;
            i += 1;
            if self.carry_len == 4 {
                self.carry_len = 0;
                match decode_quad(t, self.carry) {
                    Ok((bytes, n, padded)) => {
                        self.padded = padded;
                        if !emit(output, &mut o, &bytes[..n], &mut self.stash) {
                            self.pos += i;
                            return Step {
                                consumed: i,
                                written: o,
                                status: Status::OutputFull,
                            };
                        }
                        if padded && i < input.len() {
                            return self.fail(ErrorKind::TrailingData, self.pos + i, i, o);
                        }
                    }
                    Err((kind, k)) => return self.fail(kind, self.pos + i - 4 + k, i, o),
                }
            }
        }
        let (c, w) = decode_bulk(self.backend, &input[i..], &mut output[o..], self.alphabet);
        i += c;
        o += w;
        while i + 4 <= input.len() {
            let q = [input[i], input[i + 1], input[i + 2], input[i + 3]];
            match decode_quad(t, q) {
                Ok((bytes, n, padded)) => {
                    i += 4;
                    self.padded = padded;
                    if !emit(output, &mut o, &bytes[..n], &mut self.stash) {
                        self.pos += i;
                        return Step {
                            consumed: i,
                            written: o,
                            status: Status::OutputFull,
                        };
                    }
                    if padded && i < input.len() {
                        return self.fail(ErrorKind::TrailingData, self.pos + i, i, o);
                    }
                }
                Err((kind, k)) => return self.fail(kind, self.pos + i + k, i, o),
            }
        }
        while i < input.len() {
            self.carry[self.carry_len as usize] = input[i];
            self.carry_len += 1;
            i += 1;
        }
        self.pos += i;
        Step {
            consumed: i,
            written: o,
            status: Status::Done,
        }
    }

    fn finish(&mut self, output: &mut [u8]) -> Step {
        if let Some(e) = self.err {
            return Step {
                consumed: 0,
                written: 0,
                status: Status::Error(e),
            };
        }
        if !self.finished {
            self.finished = true;
            let t = &self.alphabet.tables().decode;
            let n = self.carry_len as usize;
            self.carry_len = 0;
            match n {
                0 => {}
                1 => return self.fail(ErrorKind::Truncated, self.pos, 0, 0),
                _ => {
                    // 2 or 3 chars, no padding: validate and emit n-1 bytes.
                    let mut x = 0u32;
                    for k in 0..n {
                        let v = t[self.carry[k] as usize];
                        if v >= 64 {
                            let kind = if v == 0xFE {
                                ErrorKind::InvalidPadding
                            } else {
                                ErrorKind::InvalidByte
                            };
                            return self.fail(kind, self.pos - n + k, 0, 0);
                        }
                        x |= (v as u32) << (18 - 6 * k);
                    }
                    let bytes = [(x >> 16) as u8, (x >> 8) as u8];
                    self.stash.set(&bytes[..n - 1]);
                }
            }
        }
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
        self.carry_len = 0;
        self.stash.clear();
        self.padded = false;
        self.finished = false;
        self.pos = 0;
        self.err = None;
    }

    fn max_output_len(&self, input_len: usize) -> usize {
        input_len / 4 * 3 + 3
    }
}

// ---------------------------------------------------------------------------
// One-shot convenience
// ---------------------------------------------------------------------------

pub fn encode(input: &[u8]) -> String {
    encode_with(Backend::best(), Alphabet::Standard, true, input)
}

pub fn encode_with(backend: Backend, alphabet: Alphabet, pad: bool, input: &[u8]) -> String {
    let v =
        crate::stream::run_to_vec(&mut Base64Encoder::new(backend, alphabet, pad), input).unwrap();
    unsafe { String::from_utf8_unchecked(v) }
}

pub fn decode(input: &[u8]) -> Result<Vec<u8>, Error> {
    decode_with(Backend::best(), Alphabet::Standard, input)
}

pub fn decode_with(backend: Backend, alphabet: Alphabet, input: &[u8]) -> Result<Vec<u8>, Error> {
    crate::stream::run_to_vec(&mut Base64Decoder::new(backend, alphabet), input)
}

#[cfg(test)]
mod lut_tests {
    use super::*;

    #[test]
    fn derived_luts_match_hand_values() {
        // Values from Muła's "base64 decoding with SIMD" for the standard alphabet.
        assert_eq!(STD_TABLES.roll_lut[2], 19);
        assert_eq!(STD_TABLES.roll_lut[10], 16);
        assert_eq!(STD_TABLES.roll_lut[3], 4);
        assert_eq!(STD_TABLES.roll_lut[4], 0xBF);
        assert_eq!(STD_TABLES.roll_lut[6], 0xB9);
        assert_eq!(STD_TABLES.special, b'/');
        assert_eq!(URL_TABLES.special, b'_');
        assert_eq!(URL_TABLES.roll_lut[13], (63u8).wrapping_sub(b'_'));
        // Validity classification is exact for every byte, both alphabets.
        for (tab, sym) in [(&STD_TABLES, STD_SYMBOLS), (&URL_TABLES, URL_SYMBOLS)] {
            for b in 0..=255u8 {
                let invalid = tab.lo_lut[(b & 15) as usize] & tab.hi_lut[(b >> 4) as usize] != 0;
                assert_eq!(invalid, !sym.contains(&b), "byte {b:#x}");
                if sym.contains(&b) {
                    let sel = (b >> 4) | if b == tab.special { 8 } else { 0 };
                    assert_eq!(
                        b.wrapping_add(tab.roll_lut[sel as usize]),
                        tab.decode[b as usize]
                    );
                }
            }
            for idx in 0..64u8 {
                let red = idx.saturating_sub(51) + (idx > 25) as u8;
                assert_eq!(
                    idx.wrapping_add(tab.enc_lut[red as usize]),
                    sym[idx as usize]
                );
            }
        }
    }
}
