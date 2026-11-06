#![no_main]
//! Decoding arbitrary (mostly invalid) bytes must produce the same result —
//! including the same error kind and offset — on every backend as the scalar
//! decoder, regardless of chunking.
use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use vcodec::base64::{Alphabet, Base64Decoder};
use vcodec::hex::HexDecoder;
use vcodec::stream::{run_chunked, run_to_vec, Transform};
use vcodec::url::UrlDecoder;
use simdlab::Backends;
use vcodec::Backend;

#[derive(Arbitrary, Debug)]
struct Input {
    data: Vec<u8>,
    chunks: Vec<u8>,
    out_buf: u8,
    flags: u8,
}

fn check<D: Transform>(inp: &Input, dec: impl Fn(Backend) -> D) {
    let chunks: Vec<usize> = inp.chunks.iter().map(|&c| c as usize + 1).collect();
    let ob = inp.out_buf as usize + 1;
    let expect = run_to_vec(&mut dec(Backend::Scalar), &inp.data);
    for b in Backend::available() {
        assert_eq!(run_to_vec(&mut dec(b), &inp.data), expect, "{b:?}");
        assert_eq!(run_chunked(&mut dec(b), &inp.data, &chunks, ob), expect, "{b:?} chunked");
    }
}

fuzz_target!(|inp: Input| {
    let alpha = if inp.flags & 2 != 0 { Alphabet::UrlSafe } else { Alphabet::Standard };
    check(&inp, |b| HexDecoder::new(b));
    check(&inp, |b| Base64Decoder::new(b, alpha));
    check(&inp, |b| UrlDecoder::new(b, inp.flags & 1 != 0));
});
