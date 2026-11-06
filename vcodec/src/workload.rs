//! Benchmark workloads, shared by `vcodec_bench` and the criterion benches: which codec and
//! direction to run, and the input distributions (generated with `simdlab::rng::XorShift64Star`
//! from one fixed seed, so every run and every tool sees the same bytes).

use crate::base64::{Alphabet, Base64Decoder, Base64Encoder};
use crate::hex::{HexDecoder, HexEncoder};
use crate::stream::{run_to_vec, Transform};
use crate::url::{UrlDecoder, UrlEncoder, UNRESERVED};
use crate::Backend;
use simdlab::rng::XorShift64Star;

/// Seed of every generated input.
pub const SEED: u64 = 0x9E37_79B9_7F4A_7C15;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, clap::ValueEnum)]
pub enum Codec {
    Hex,
    Base64,
    Url,
}

impl Codec {
    pub const ALL: [Codec; 3] = [Codec::Hex, Codec::Base64, Codec::Url];

    pub fn name(self) -> &'static str {
        match self {
            Codec::Hex => "hex",
            Codec::Base64 => "base64",
            Codec::Url => "url",
        }
    }

    /// The distributions a matrix run measures by default.
    pub fn default_dists(self) -> &'static [Dist] {
        match self {
            Codec::Url => &[Dist::Safe, Dist::Text, Dist::Pct10, Dist::Binary],
            _ => &[Dist::Random],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, clap::ValueEnum)]
pub enum Op {
    Encode,
    Decode,
}

impl Op {
    pub const ALL: [Op; 2] = [Op::Encode, Op::Decode];

    pub fn name(self) -> &'static str {
        match self {
            Op::Encode => "encode",
            Op::Decode => "decode",
        }
    }
}

/// Raw (pre-encoding) input data.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, clap::ValueEnum)]
pub enum Dist {
    /// Uniform random bytes.
    Random,
    /// Uniform random bytes (the name used for URL encoding: ~75% of bytes escape).
    Binary,
    /// Alphanumerics only (no URL escapes).
    Safe,
    /// English-like words, spaces and punctuation (~17% URL escapes).
    Text,
    /// Alphanumerics with 1% URL-special bytes.
    Pct1,
    /// Alphanumerics with 10% URL-special bytes.
    Pct10,
    /// Alphanumerics with 30% URL-special bytes.
    Pct30,
}

impl Dist {
    pub fn name(self) -> &'static str {
        match self {
            Dist::Random => "random",
            Dist::Binary => "binary",
            Dist::Safe => "safe",
            Dist::Text => "text",
            Dist::Pct1 => "pct1",
            Dist::Pct10 => "pct10",
            Dist::Pct30 => "pct30",
        }
    }
}

const ALNUM: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
const URL_SPECIAL: &[u8] = b"/?&=: #\"<>{}|\\^`\x80\xC3\xE2";

/// `n` bytes of raw data from `dist`.
pub fn raw(dist: Dist, n: usize) -> Vec<u8> {
    let mut r = XorShift64Star::new(SEED);
    let mut v = Vec::with_capacity(n);
    let mut pct = |p: u32| {
        for _ in 0..n {
            if r.below(100) < p {
                v.push(URL_SPECIAL[r.below(URL_SPECIAL.len() as u32) as usize]);
            } else {
                v.push(ALNUM[r.below(ALNUM.len() as u32) as usize]);
            }
        }
    };
    match dist {
        Dist::Pct1 => pct(1),
        Dist::Pct10 => pct(10),
        Dist::Pct30 => pct(30),
        Dist::Random | Dist::Binary => v.extend((0..n).map(|_| r.next_u8())),
        Dist::Safe => v.extend((0..n).map(|_| ALNUM[r.below(ALNUM.len() as u32) as usize])),
        Dist::Text => {
            while v.len() < n {
                let wl = 1 + r.below(9) as usize;
                for _ in 0..wl {
                    v.push(b'a' + r.below(26) as u8);
                }
                match r.below(10) {
                    0 => v.extend_from_slice(b", "),
                    1 => v.extend_from_slice(b". "),
                    _ => v.push(b' '),
                }
            }
            v.truncate(n);
        }
    }
    v
}

/// The transform a workload runs: lower-case hex, padded standard base64, and percent encoding
/// of everything outside the RFC 3986 unreserved set (`+` stays `+` when decoding).
pub fn transform(codec: Codec, op: Op, backend: Backend) -> Box<dyn Transform> {
    match (codec, op) {
        (Codec::Hex, Op::Encode) => Box::new(HexEncoder::new(backend, false)),
        (Codec::Hex, Op::Decode) => Box::new(HexDecoder::new(backend)),
        (Codec::Base64, Op::Encode) => Box::new(Base64Encoder::new(backend, Alphabet::Standard, true)),
        (Codec::Base64, Op::Decode) => Box::new(Base64Decoder::new(backend, Alphabet::Standard)),
        (Codec::Url, Op::Encode) => Box::new(UrlEncoder::new(backend, UNRESERVED)),
        (Codec::Url, Op::Decode) => Box::new(UrlDecoder::new(backend, false)),
    }
}

/// The input of a workload, `size` bytes or slightly fewer: raw data for encoders; for decoders,
/// the scalar encoding of raw data, cut to `size` at a unit boundary (so throughputs compare per
/// input byte).
pub fn input(codec: Codec, op: Op, dist: Dist, size: usize) -> Vec<u8> {
    let raw = raw(dist, size);
    if op == Op::Encode {
        return raw;
    }
    let mut out = run_to_vec(transform(codec, Op::Encode, Backend::Scalar).as_mut(), &raw).unwrap();
    out.truncate(size);
    // Do not cut a unit in half (hex pair / base64 quad / %XX).
    match codec {
        Codec::Hex => out.truncate(out.len() & !1),
        Codec::Base64 => out.truncate(out.len() & !3),
        Codec::Url => {
            let l = out.len();
            if l >= 2 && out[l - 2] == b'%' {
                out.truncate(l - 2);
            } else if l >= 1 && out[l - 1] == b'%' {
                out.truncate(l - 1);
            }
        }
    }
    out
}
