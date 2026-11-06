//! Property tests: every backend × every codec against the scalar oracle,
//! under arbitrary chunking and arbitrary output-buffer sizes, plus error
//! offsets and known-answer vectors.

use proptest::prelude::*;
use vcodec::base64::{Alphabet, Base64Decoder, Base64Encoder};
use vcodec::hex::{HexDecoder, HexEncoder};
use vcodec::stream::{run_chunked, run_to_vec, Error, Transform};
use vcodec::url::{UrlDecoder, UrlEncoder, COMPONENT, FORM, UNRESERVED};
use simdlab::testing::{for_each_backend, iterations};
use simdlab::Backends;
use vcodec::Backend;

fn backends() -> Vec<Backend> {
    Backend::available()
}

fn arb_bytes() -> impl Strategy<Value = Vec<u8>> {
    prop_oneof![
        // sizes around every block boundary that matters (16/28/32/48/64 ± 3)
        (0usize..=140).prop_flat_map(|n| proptest::collection::vec(any::<u8>(), n)),
        proptest::collection::vec(any::<u8>(), 0..4096),
        // text-like (mostly URL-safe) so the URL fast path is exercised
        proptest::collection::vec(
            prop_oneof![9 => (b'a'..=b'z').prop_map(|c| c), 1 => any::<u8>()],
            0..2048
        ),
    ]
}

fn arb_chunks() -> impl Strategy<Value = Vec<usize>> {
    proptest::collection::vec(1usize..200, 0..64)
}

fn check_stream<T: Transform>(
    mk: impl Fn() -> T,
    input: &[u8],
    chunks: &[usize],
    out_buf: usize,
) -> Result<Vec<u8>, Error> {
    let one_shot = run_to_vec(&mut mk(), input);
    let chunked = run_chunked(&mut mk(), input, chunks, out_buf);
    prop_assert_eq_res(&one_shot, &chunked);
    one_shot
}

fn prop_assert_eq_res(a: &Result<Vec<u8>, Error>, b: &Result<Vec<u8>, Error>) {
    assert_eq!(a, b, "one-shot vs chunked/bounded streaming differ");
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(iterations(600) as u32))]

    // ---------------- hex ----------------
    #[test]
    fn hex_roundtrip_all_backends(data in arb_bytes(), chunks in arb_chunks(), out_buf in 1usize..100, upper in any::<bool>()) {
        let expect = run_to_vec(&mut HexEncoder::new(Backend::Scalar, upper), &data).unwrap();
        for b in backends() {
            let enc = check_stream(|| HexEncoder::new(b, upper), &data, &chunks, out_buf).unwrap();
            prop_assert_eq!(&enc, &expect, "hex encode {:?}", b);
            let dec = check_stream(|| HexDecoder::new(b), &enc, &chunks, out_buf).unwrap();
            prop_assert_eq!(&dec, &data, "hex decode {:?}", b);
            let dec = check_stream(|| HexDecoder::with_classifier(b, vcodec::hex::Classifier::RangeCheck), &enc, &chunks, out_buf).unwrap();
            prop_assert_eq!(&dec, &data, "hex decode (range-check) {:?}", b);
        }
        // mixed case decodes too
        let mixed: Vec<u8> = expect.iter().enumerate().map(|(i, &c)| if i % 3 == 0 { c.to_ascii_uppercase() } else { c }).collect();
        for b in backends() {
            prop_assert_eq!(run_to_vec(&mut HexDecoder::new(b), &mixed).unwrap(), data.clone());
        }
    }

    #[test]
    fn hex_decode_errors_match_scalar(data in proptest::collection::vec(prop_oneof![20 => (b'0'..=b'9').prop_map(|c| c), 10 => (b'a'..=b'f').prop_map(|c| c), 1 => any::<u8>()], 0..600), chunks in arb_chunks(), out_buf in 1usize..64) {
        let expect = run_to_vec(&mut HexDecoder::new(Backend::Scalar), &data);
        for b in backends() {
            let got = check_stream(|| HexDecoder::new(b), &data, &chunks, out_buf);
            prop_assert_eq!(&got, &expect, "hex decode errors {:?}", b);
            let got = check_stream(|| HexDecoder::with_classifier(b, vcodec::hex::Classifier::RangeCheck), &data, &chunks, out_buf);
            prop_assert_eq!(&got, &expect, "hex decode errors (range-check) {:?}", b);
        }
    }

    // ---------------- base64 ----------------
    #[test]
    fn base64_roundtrip_all_backends(data in arb_bytes(), chunks in arb_chunks(), out_buf in 1usize..100, url in any::<bool>(), pad in any::<bool>()) {
        let alpha = if url { Alphabet::UrlSafe } else { Alphabet::Standard };
        let expect = run_to_vec(&mut Base64Encoder::new(Backend::Scalar, alpha, pad), &data).unwrap();
        // cross-check against the `base64` crate
        let eng = match (url, pad) {
            (false, true) => base64::engine::general_purpose::STANDARD,
            (false, false) => base64::engine::general_purpose::STANDARD_NO_PAD,
            (true, true) => base64::engine::general_purpose::URL_SAFE,
            (true, false) => base64::engine::general_purpose::URL_SAFE_NO_PAD,
        };
        use base64::Engine;
        prop_assert_eq!(std::str::from_utf8(&expect).unwrap(), eng.encode(&data));
        for b in backends() {
            let enc = check_stream(|| Base64Encoder::new(b, alpha, pad), &data, &chunks, out_buf).unwrap();
            prop_assert_eq!(&enc, &expect, "b64 encode {:?}", b);
            let dec = check_stream(|| Base64Decoder::new(b, alpha), &enc, &chunks, out_buf).unwrap();
            prop_assert_eq!(&dec, &data, "b64 decode {:?}", b);
        }
    }

    #[test]
    fn base64_decode_errors_match_scalar(data in proptest::collection::vec(prop_oneof![30 => (b'A'..=b'Z').prop_map(|c| c), 10 => (b'a'..=b'z').prop_map(|c| c), 3 => Just(b'='), 2 => Just(b'+'), 2 => Just(b'/'), 1 => any::<u8>()], 0..600), chunks in arb_chunks(), out_buf in 1usize..64, url in any::<bool>()) {
        let alpha = if url { Alphabet::UrlSafe } else { Alphabet::Standard };
        let expect = run_to_vec(&mut Base64Decoder::new(Backend::Scalar, alpha), &data);
        for b in backends() {
            let got = check_stream(|| Base64Decoder::new(b, alpha), &data, &chunks, out_buf);
            prop_assert_eq!(&got, &expect, "b64 decode errors {:?}", b);
        }
    }

    // ---------------- url ----------------
    #[test]
    fn url_roundtrip_all_backends(data in arb_bytes(), chunks in arb_chunks(), out_buf in 1usize..100, which in 0u8..3) {
        let set = match which { 0 => UNRESERVED, 1 => COMPONENT, _ => FORM };
        let expect = run_to_vec(&mut UrlEncoder::new(Backend::Scalar, set), &data).unwrap();
        if which == 0 {
            // cross-check against percent-encoding crate (NON_ALPHANUMERIC minus -._~)
            const SET: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC.remove(b'-').remove(b'.').remove(b'_').remove(b'~');
            let reference: String = percent_encoding::percent_encode(&data, SET).to_string();
            prop_assert_eq!(std::str::from_utf8(&expect).unwrap(), reference);
        }
        for b in backends() {
            let enc = check_stream(|| UrlEncoder::new(b, set), &data, &chunks, out_buf).unwrap();
            prop_assert_eq!(&enc, &expect, "url encode {:?}", b);
            let dec = check_stream(|| UrlDecoder::new(b, false), &enc, &chunks, out_buf).unwrap();
            prop_assert_eq!(&dec, &data, "url decode {:?}", b);
        }
    }

    #[test]
    fn url_decode_arbitrary_matches_scalar(data in proptest::collection::vec(prop_oneof![6 => (b'a'..=b'z').prop_map(|c| c), 3 => Just(b'%'), 2 => (b'0'..=b'9').prop_map(|c| c), 2 => (b'A'..=b'F').prop_map(|c| c), 1 => Just(b'+'), 1 => any::<u8>()], 0..1200), chunks in arb_chunks(), out_buf in 1usize..64, plus in any::<bool>()) {
        let expect = run_to_vec(&mut UrlDecoder::new(Backend::Scalar, plus), &data);
        for b in backends() {
            let got = check_stream(|| UrlDecoder::new(b, plus), &data, &chunks, out_buf);
            prop_assert_eq!(&got, &expect, "url decode {:?}", b);
        }
        // and against percent-encoding when there is no error
        if let Ok(ref v) = expect {
            // (when we return Ok every `%` was a well-formed escape, so the
            // lenient percent-encoding decoder must agree)
            if !plus {
                let reference: Vec<u8> = percent_encoding::percent_decode(&data).collect();
                prop_assert_eq!(v, &reference);
            }
        }
    }
}

// ---------------- known answers & error offsets ----------------

#[test]
fn known_answers() {
    for_each_backend(|b: Backend| {
        assert_eq!(
            vcodec::hex::encode_with(b, b"\x00\xffAz", false),
            "00ff417a"
        );
        assert_eq!(vcodec::hex::encode_with(b, b"\x00\xffAz", true), "00FF417A");
        assert_eq!(
            vcodec::hex::decode_with(b, b"00FF417a").unwrap(),
            b"\x00\xffAz"
        );
        assert_eq!(
            vcodec::base64::encode_with(b, Alphabet::Standard, true, b"Man"),
            "TWFu"
        );
        assert_eq!(
            vcodec::base64::encode_with(b, Alphabet::Standard, true, b"Ma"),
            "TWE="
        );
        assert_eq!(
            vcodec::base64::encode_with(b, Alphabet::Standard, true, b"M"),
            "TQ=="
        );
        assert_eq!(
            vcodec::base64::encode_with(b, Alphabet::Standard, false, b"M"),
            "TQ"
        );
        assert_eq!(
            vcodec::base64::encode_with(b, Alphabet::UrlSafe, true, b"\xfb\xff\xbf"),
            "-_-_"
        );
        assert_eq!(
            vcodec::base64::decode_with(b, Alphabet::UrlSafe, b"-_-_").unwrap(),
            b"\xfb\xff\xbf"
        );
        assert_eq!(
            vcodec::base64::decode_with(b, Alphabet::Standard, b"TWE=").unwrap(),
            b"Ma"
        );
        assert_eq!(
            vcodec::base64::decode_with(b, Alphabet::Standard, b"TWE").unwrap(),
            b"Ma"
        );
        assert_eq!(
            vcodec::base64::decode_with(b, Alphabet::Standard, b"TQ==").unwrap(),
            b"M"
        );
        let long = b"The quick brown fox jumps over the lazy dog. The quick brown fox jumps over the lazy dog!";
        let enc = vcodec::base64::encode_with(b, Alphabet::Standard, true, long);
        assert_eq!(enc, "VGhlIHF1aWNrIGJyb3duIGZveCBqdW1wcyBvdmVyIHRoZSBsYXp5IGRvZy4gVGhlIHF1aWNrIGJyb3duIGZveCBqdW1wcyBvdmVyIHRoZSBsYXp5IGRvZyE=");
        assert_eq!(
            vcodec::base64::decode_with(b, Alphabet::Standard, enc.as_bytes()).unwrap(),
            long
        );
        assert_eq!(
            vcodec::url::encode_with(b, &UNRESERVED, b"a b/c?d=\xe2\x82\xac~"),
            "a%20b%2Fc%3Fd%3D%E2%82%AC~"
        );
        assert_eq!(
            vcodec::url::decode_with(b, false, b"a%20b%2fc+d%E2%82%AC").unwrap(),
            b"a b/c+d\xe2\x82\xac"
        );
        assert_eq!(
            vcodec::url::decode_with(b, true, b"a+b%2Bc").unwrap(),
            b"a b+c"
        );
    });
}

#[test]
fn error_offsets_are_absolute_and_backend_independent() {
    use vcodec::ErrorKind::*;
    let cases: Vec<(&str, Vec<u8>, vcodec::ErrorKind, usize)> = vec![
        (
            "hex",
            b"00112233445566778899aabbccddeeffg0".to_vec(),
            InvalidByte,
            32,
        ),
        (
            "hex",
            b"00112233445566778899aabbccddeeff0".to_vec(),
            Truncated,
            33,
        ),
        (
            "b64",
            b"QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVo=x".to_vec(),
            TrailingData,
            36,
        ),
        (
            "b64",
            b"QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVo!".to_vec(),
            InvalidByte,
            35,
        ),
        (
            "b64",
            b"QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVpBQ".to_vec(),
            Truncated,
            37,
        ),
        ("b64", b"QUJD=UJD".to_vec(), InvalidPadding, 4),
        ("b64", b"QUJDQU=D".to_vec(), InvalidPadding, 7),
        (
            "url",
            b"abcdefghijklmnopqrstuvwxyz0123456789%4".to_vec(),
            Truncated,
            38,
        ),
        (
            "url",
            b"abcdefghijklmnopqrstuvwxyz0123456789%4G".to_vec(),
            InvalidByte,
            38,
        ),
        (
            "url",
            b"abcdefghijklmnopqrstuvwxyz%%41".to_vec(),
            InvalidByte,
            27,
        ),
    ];
    for (codec, input, kind, offset) in cases {
        for b in backends() {
            let r: Result<Vec<u8>, Error> = match codec {
                "hex" => vcodec::hex::decode_with(b, &input),
                "b64" => vcodec::base64::decode_with(b, Alphabet::Standard, &input),
                _ => vcodec::url::decode_with(b, false, &input),
            };
            assert_eq!(
                r,
                Err(Error { kind, offset }),
                "{codec} {:?} {:?}",
                b,
                String::from_utf8_lossy(&input)
            );
        }
    }
}

#[test]
fn url_escape_spanning_block_boundaries() {
    // Place `%XX` so that the `%` lands on every position around the 16/32-byte
    // boundaries, with the digits in the next block.
    for pct_pos in 10..70usize {
        let mut s = vec![b'a'; 100];
        s[pct_pos] = b'%';
        s[pct_pos + 1] = b'4';
        s[pct_pos + 2] = b'1';
        let expect = vcodec::url::decode_with(Backend::Scalar, false, &s).unwrap();
        for b in backends() {
            assert_eq!(
                vcodec::url::decode_with(b, false, &s).unwrap(),
                expect,
                "{:?} pos {pct_pos}",
                b
            );
            // And with the chunk boundary cutting through the escape.
            for cut in [pct_pos, pct_pos + 1, pct_pos + 2] {
                let got = run_chunked(&mut UrlDecoder::new(b, false), &s, &[cut], 7).unwrap();
                assert_eq!(got, expect, "{:?} pos {pct_pos} cut {cut}", b);
            }
        }
    }
}

/// A quick in-process fuzz loop (the real fuzz targets live in `fuzz/`): random
/// bytes, random chunking, random output sizes, every backend vs scalar.
#[test]
fn mini_fuzz() {
    let mut seed = 0x1234_5678_9abc_def0u64;
    let mut rnd = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let iters = iterations(3000);
    let bks = backends();
    for _ in 0..iters {
        let n = (rnd() % 300) as usize;
        let mode = rnd() % 4;
        let data: Vec<u8> = (0..n)
            .map(|_| match mode {
                0 => rnd() as u8,
                1 => b"abcdefABCDEF0123456789%=+/-_~. "[(rnd() % 31) as usize],
                2 => {
                    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/="
                        [(rnd() % 65) as usize]
                }
                _ => b"abc%4"[(rnd() % 5) as usize],
            })
            .collect();
        let chunks: Vec<usize> = (0..(rnd() % 6))
            .map(|_| 1 + (rnd() % 40) as usize)
            .collect();
        let out_buf = 1 + (rnd() % 50) as usize;
        macro_rules! cmp {
            ($mk:expr) => {{
                let expect = run_to_vec(&mut $mk(Backend::Scalar), &data);
                for &b in &bks {
                    assert_eq!(run_to_vec(&mut $mk(b), &data), expect, "{:?} {:?}", b, data);
                    assert_eq!(
                        run_chunked(&mut $mk(b), &data, &chunks, out_buf),
                        expect,
                        "{:?} chunked {:?}",
                        b,
                        data
                    );
                }
            }};
        }
        cmp!(|b| HexEncoder::new(b, false));
        cmp!(|b| HexDecoder::new(b));
        cmp!(|b| HexDecoder::with_classifier(b, vcodec::hex::Classifier::RangeCheck));
        cmp!(|b| Base64Encoder::new(b, Alphabet::Standard, true));
        cmp!(|b| Base64Decoder::new(b, Alphabet::Standard));
        cmp!(|b| Base64Decoder::new(b, Alphabet::UrlSafe));
        cmp!(|b| UrlEncoder::new(b, UNRESERVED));
        cmp!(|b| UrlDecoder::new(b, false));
        cmp!(|b| UrlDecoder::new(b, true));
    }
}
