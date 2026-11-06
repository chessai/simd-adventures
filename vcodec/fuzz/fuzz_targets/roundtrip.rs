#![no_main]
//! encode(x) must equal the scalar encode on every backend, under arbitrary
//! chunking / output sizes, and decode(encode(x)) == x.
use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use vcodec::base64::{Alphabet, Base64Decoder, Base64Encoder};
use vcodec::hex::{HexDecoder, HexEncoder};
use vcodec::stream::{run_chunked, run_to_vec, Transform};
use vcodec::url::{UrlDecoder, UrlEncoder, COMPONENT, UNRESERVED};
use simdlab::Backends;
use vcodec::Backend;

#[derive(Arbitrary, Debug)]
struct Input {
    data: Vec<u8>,
    chunks: Vec<u8>,
    out_buf: u8,
    flags: u8,
}

fn check<E: Transform, D: Transform>(inp: &Input, enc: impl Fn(Backend) -> E, dec: impl Fn(Backend) -> D) {
    let chunks: Vec<usize> = inp.chunks.iter().map(|&c| c as usize + 1).collect();
    let ob = inp.out_buf as usize + 1;
    let expect = run_to_vec(&mut enc(Backend::Scalar), &inp.data).unwrap();
    for b in Backend::available() {
        let e = run_chunked(&mut enc(b), &inp.data, &chunks, ob).unwrap();
        assert_eq!(e, expect, "{b:?}");
        let d = run_chunked(&mut dec(b), &e, &chunks, ob).unwrap();
        assert_eq!(d, inp.data, "{b:?}");
    }
}

fuzz_target!(|inp: Input| {
    let upper = inp.flags & 1 != 0;
    let url = inp.flags & 2 != 0;
    let pad = inp.flags & 4 != 0;
    let alpha = if url { Alphabet::UrlSafe } else { Alphabet::Standard };
    let set = if inp.flags & 8 != 0 { COMPONENT } else { UNRESERVED };
    check(&inp, |b| HexEncoder::new(b, upper), |b| HexDecoder::new(b));
    check(&inp, |b| Base64Encoder::new(b, alpha, pad), |b| Base64Decoder::new(b, alpha));
    check(&inp, |b| UrlEncoder::new(b, set), |b| UrlDecoder::new(b, false));
});
