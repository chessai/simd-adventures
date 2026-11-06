//! NEON-native hex and base64 kernels.
//!
//! The generic kernels make NEON emulate x86: `pmaddubsw`/`pmaddwd` (mask, shift, two
//! multiplies), `pmulhuw` (two widening multiplies and a narrowing shift), `pshufb`'s index
//! semantics (an extra `and`) and the 12-byte-per-lane base64 layout. NEON has better tools for
//! these codecs:
//!
//! * structured loads and stores, which (de)interleave 2, 3 or 4 byte streams for free:
//!   `vld2` splits hex digit pairs, `vst2` interleaves them; `vld3`/`vst4` and `vld4`/`vst3` turn
//!   base64's 3-byte/4-char groups into four (three) whole registers of sextets (bytes);
//! * `tbl`/`tbx` over four registers: a 64-entry table, the whole base64 alphabet, or two lookups
//!   covering ASCII 0..127 for decoding; out-of-range indices give 0 (`tbl`) or keep the
//!   destination (`tbx`), which doubles as the validity check;
//! * shift-insert (`vsli`, `vsri`) to merge bit fields.
//!
//! For base64 this is the approach of the NEON64 codec in aklomp/base64. Each kernel has the
//! contract of the generic kernel it replaces; the property tests compare them with the scalar
//! code under qemu-aarch64 (correctness only; speed needs real hardware).

use crate::base64::Alphabet;
use core::arch::aarch64::*;

/// Hex encode, 16 bytes → 32 digits per step: two 16-entry lookups, `vst2` interleaves them.
#[inline(always)]
pub unsafe fn hex_encode(input: &[u8], output: &mut [u8], upper: bool) -> (usize, usize) {
    let table = vld1q_u8(if upper { crate::hex::UPPER } else { crate::hex::LOWER }.as_ptr());
    let nib = vdupq_n_u8(0x0F);
    let (mut i, mut o) = (0, 0);
    while i + 16 <= input.len() && o + 32 <= output.len() {
        let v = vld1q_u8(input.as_ptr().add(i));
        let hi = vqtbl1q_u8(table, vshrq_n_u8::<4>(v));
        let lo = vqtbl1q_u8(table, vandq_u8(v, nib));
        vst2q_u8(output.as_mut_ptr().add(o), uint8x16x2_t(hi, lo));
        i += 16;
        o += 32;
    }
    (i, o)
}

/// `HEX_FROM_0[c - '0']`: the value of hex digit `c` for `'0' <= c < '0' + 64`, else 0xFF.
const HEX_FROM_0: [u8; 64] = {
    let mut t = [0xFFu8; 64];
    let mut k = 0;
    while k < 10 {
        t[k] = k as u8;
        k += 1;
    }
    let mut k = 0;
    while k < 6 {
        t[(b'A' - b'0') as usize + k] = 10 + k as u8;
        t[(b'a' - b'0') as usize + k] = 10 + k as u8;
        k += 1;
    }
    t
};

/// Hex decode, 32 digits → 16 bytes per step: `vld2` splits high and low digits, one 64-entry
/// `tbx` per register converts and validates (everything outside `'0'..'0'+64` keeps 0xFF), and
/// `vsli` merges the nibbles. Stops before a block with a non-hex digit.
#[inline(always)]
pub unsafe fn hex_decode(input: &[u8], output: &mut [u8]) -> (usize, usize) {
    let table = vld1q_u8_x4(HEX_FROM_0.as_ptr());
    let zero_char = vdupq_n_u8(b'0');
    let invalid = vdupq_n_u8(0xFF);
    let (mut i, mut o) = (0, 0);
    while i + 32 <= input.len() && o + 16 <= output.len() {
        let d = vld2q_u8(input.as_ptr().add(i));
        let hi = vqtbx4q_u8(invalid, table, vsubq_u8(d.0, zero_char));
        let lo = vqtbx4q_u8(invalid, table, vsubq_u8(d.1, zero_char));
        if vmaxvq_u8(vorrq_u8(hi, lo)) > 15 {
            break;
        }
        vst1q_u8(output.as_mut_ptr().add(o), vsliq_n_u8::<4>(lo, hi));
        i += 32;
        o += 16;
    }
    (i, o)
}

/// Base64 encode, 48 bytes → 64 characters per step: `vld3` gives the first, second and third
/// byte of 16 groups, shifts and shift-inserts cut the four sextets, a 64-entry `tbl` per
/// register maps them to the alphabet, and `vst4` interleaves the four characters.
#[inline(always)]
pub unsafe fn base64_encode(input: &[u8], output: &mut [u8], alphabet: Alphabet) -> (usize, usize) {
    let table = vld1q_u8_x4(alphabet.symbols().as_ptr());
    let low6 = vdupq_n_u8(0x3F);
    let (mut i, mut o) = (0, 0);
    while i + 48 <= input.len() && o + 64 <= output.len() {
        let g = vld3q_u8(input.as_ptr().add(i));
        let (a, b, c) = (g.0, g.1, g.2);
        let s0 = vshrq_n_u8::<2>(a);
        let s1 = vandq_u8(vsriq_n_u8::<4>(vshlq_n_u8::<4>(a), b), low6); // (a & 3) << 4 | b >> 4
        let s2 = vandq_u8(vsriq_n_u8::<6>(vshlq_n_u8::<2>(b), c), low6); // (b & 15) << 2 | c >> 6
        let s3 = vandq_u8(c, low6);
        let chars = uint8x16x4_t(
            vqtbl4q_u8(table, s0),
            vqtbl4q_u8(table, s1),
            vqtbl4q_u8(table, s2),
            vqtbl4q_u8(table, s3),
        );
        vst4q_u8(output.as_mut_ptr().add(o), chars);
        i += 48;
        o += 64;
    }
    (i, o)
}

/// Base64 decode, 64 characters → 48 bytes per step: `vld4` gives the four characters of 16
/// groups, two chained 64-entry `tbx` lookups map ASCII 0..127 to sextets (0xFF for anything
/// else, including `=` and bytes >= 0x80), three shift/or pairs rebuild the bytes, and `vst3`
/// interleaves them. Stops before a block with anything but alphabet characters.
#[inline(always)]
pub unsafe fn base64_decode(input: &[u8], output: &mut [u8], alphabet: Alphabet) -> (usize, usize) {
    let dec = &alphabet.tables().decode; // 0..63 valid, 0xFE '=', 0xFF invalid
    let lut_lo = vld1q_u8_x4(dec.as_ptr());
    let lut_hi = vld1q_u8_x4(dec.as_ptr().add(64));
    let invalid = vdupq_n_u8(0xFF);
    let c64 = vdupq_n_u8(64);
    #[inline(always)]
    unsafe fn sextets(x: uint8x16_t, invalid: uint8x16_t, c64: uint8x16_t, lo: uint8x16x4_t, hi: uint8x16x4_t) -> uint8x16_t {
        // x < 64: lo[x]; 64 <= x < 128: hi[x - 64]; else stays 0xFF
        vqtbx4q_u8(vqtbx4q_u8(invalid, lo, x), hi, vsubq_u8(x, c64))
    }
    let (mut i, mut o) = (0, 0);
    while i + 64 <= input.len() && o + 48 <= output.len() {
        let g = vld4q_u8(input.as_ptr().add(i));
        let a = sextets(g.0, invalid, c64, lut_lo, lut_hi);
        let b = sextets(g.1, invalid, c64, lut_lo, lut_hi);
        let c = sextets(g.2, invalid, c64, lut_lo, lut_hi);
        let d = sextets(g.3, invalid, c64, lut_lo, lut_hi);
        if vmaxvq_u8(vorrq_u8(vorrq_u8(a, b), vorrq_u8(c, d))) >= 64 {
            break;
        }
        let bytes = uint8x16x3_t(
            vorrq_u8(vshlq_n_u8::<2>(a), vshrq_n_u8::<4>(b)),
            vorrq_u8(vshlq_n_u8::<4>(b), vshrq_n_u8::<2>(c)),
            vorrq_u8(vshlq_n_u8::<6>(c), d),
        );
        vst3q_u8(output.as_mut_ptr().add(o), bytes);
        i += 64;
        o += 48;
    }
    (i, o)
}

#[cfg(test)]
mod tests {
    //! The property tests compare whole transforms, which would also pass if these kernels did
    //! nothing (the scalar unit path would do the work); these check the kernels themselves.
    use super::*;

    fn data() -> Vec<u8> {
        (0..1000u32).map(|i| (i.wrapping_mul(2654435761) >> 13) as u8).collect()
    }

    #[test]
    fn hex_kernels_consume_whole_blocks() {
        let data = data();
        for upper in [false, true] {
            let mut enc = vec![0u8; 2000];
            let (i, o) = unsafe { hex_encode(&data, &mut enc, upper) };
            assert_eq!((i, o), (992, 1984));
            let mut expect = vec![0u8; 1984];
            crate::hex::encode_scalar(&data[..992], &mut expect, upper);
            assert_eq!(enc[..o], expect[..]);

            let mut dec = vec![0u8; 1000];
            let (i, o) = unsafe { hex_decode(&enc[..1984], &mut dec) };
            assert_eq!((i, o), (1984, 992));
            assert_eq!(dec[..o], data[..992]);

            enc[100] = b'g'; // stops before the block containing it (digits 96..128)
            assert_eq!(unsafe { hex_decode(&enc[..1984], &mut dec) }, (96, 48));
        }
    }

    #[test]
    fn base64_kernels_consume_whole_blocks() {
        let data = data();
        for alphabet in [Alphabet::Standard, Alphabet::UrlSafe] {
            let mut enc = vec![0u8; 1400];
            let (i, o) = unsafe { base64_encode(&data, &mut enc, alphabet) };
            assert_eq!((i, o), (960, 1280));
            let mut expect = vec![0u8; 1280];
            crate::base64::encode_scalar(&data[..960], &mut expect, alphabet);
            assert_eq!(enc[..o], expect[..]);

            let mut dec = vec![0u8; 1000];
            let (i, o) = unsafe { base64_decode(&enc[..1280], &mut dec, alphabet) };
            assert_eq!((i, o), (1280, 960));
            assert_eq!(dec[..o], data[..960]);

            enc[300] = b'='; // stops before the block containing it (characters 256..320)
            assert_eq!(unsafe { base64_decode(&enc[..1280], &mut dec, alphabet) }, (256, 192));
        }
    }
}
