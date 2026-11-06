//! Throughput benchmarks: vcodec backends vs the scalar baseline vs popular
//! Rust crates (`hex`, `hex-simd`, `base64`, `base64-simd`, `percent-encoding`)
//! and OpenSSL's `EVP_EncodeBlock`/`EVP_DecodeBlock` (C, via FFI; feature `openssl`).
//!
//! Run everything:      cargo bench -p vcodec --features openssl
//! Filter:              cargo bench -p vcodec -- 'base64/decode'
//! Summarise:           python3 scripts/summarize_criterion.py

use base64::Engine;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::time::Duration;
use vcodec::base64::{Alphabet, Base64Decoder, Base64Encoder};
use vcodec::hex::{HexDecoder, HexEncoder};
use vcodec::stream::{run_to_vec, Transform};
use vcodec::url::{UrlDecoder, UrlEncoder, UNRESERVED};
use simdlab::Backends;
use vcodec::workload::{self, Dist};
use vcodec::Backend;

/// The same inputs as `vcodec_bench` (`vcodec::workload`).
fn gen_raw(dist: &str, n: usize) -> Vec<u8> {
    let dist = <Dist as clap::ValueEnum>::from_str(dist, false).unwrap();
    workload::raw(dist, n)
}

fn sizes() -> Vec<usize> {
    if let Ok(s) = std::env::var("VCODEC_SIZES") {
        s.split(',').map(|x| x.parse().unwrap()).collect()
    } else {
        vec![64, 1024, 65536, 4 << 20]
    }
}

fn label(size: usize, dist: &str) -> String {
    let s = if size >= 1 << 20 {
        format!("{}MiB", size >> 20)
    } else if size >= 1024 {
        format!("{}KiB", size >> 10)
    } else {
        format!("{size}B")
    };
    format!("{s}/{dist}")
}

/// Run a vcodec transform one-shot into a preallocated buffer.
#[inline]
fn run<T: Transform>(t: &mut T, input: &[u8], out: &mut [u8]) -> usize {
    t.reset();
    let s = t.update(input, out);
    debug_assert_eq!(s.consumed, input.len());
    let f = t.finish(&mut out[s.written..]);
    s.written + f.written
}

fn bench_hex(c: &mut Criterion) {
    for size in sizes() {
        let raw = gen_raw("random", size);
        let encoded = run_to_vec(&mut HexEncoder::new(Backend::Scalar, false), &raw).unwrap();
        let mut out = vec![0u8; 2 * size + 64];

        let mut g = c.benchmark_group("hex/encode");
        g.throughput(Throughput::Bytes(size as u64));
        for b in Backend::available() {
            let mut t = HexEncoder::new(b, false);
            g.bench_with_input(
                BenchmarkId::new(format!("vcodec-{}", b.name()), label(size, "random")),
                &raw,
                |bn, inp| bn.iter(|| run(&mut t, inp, &mut out)),
            );
        }
        g.bench_with_input(
            BenchmarkId::new("hex-crate", label(size, "random")),
            &raw,
            |bn, inp| bn.iter(|| hex::encode_to_slice(inp, &mut out[..2 * inp.len()]).unwrap()),
        );
        g.bench_with_input(
            BenchmarkId::new("hex-simd", label(size, "random")),
            &raw,
            |bn, inp| {
                bn.iter(|| {
                    hex_simd::encode(
                        inp,
                        hex_simd::Out::from_slice(&mut out[..2 * inp.len()]),
                        hex_simd::AsciiCase::Lower,
                    )
                    .len()
                })
            },
        );
        g.finish();

        let mut g = c.benchmark_group("hex/decode");
        g.throughput(Throughput::Bytes(encoded.len() as u64));
        for b in Backend::available() {
            let mut t = HexDecoder::new(b);
            g.bench_with_input(
                BenchmarkId::new(format!("vcodec-{}", b.name()), label(size, "random")),
                &encoded,
                |bn, inp| bn.iter(|| run(&mut t, inp, &mut out)),
            );
            if b != Backend::Scalar {
                let mut t = HexDecoder::with_classifier(b, vcodec::hex::Classifier::RangeCheck);
                g.bench_with_input(
                    BenchmarkId::new(
                        format!("vcodec-{}-rangecheck", b.name()),
                        label(size, "random"),
                    ),
                    &encoded,
                    |bn, inp| bn.iter(|| run(&mut t, inp, &mut out)),
                );
            }
        }
        g.bench_with_input(
            BenchmarkId::new("hex-crate", label(size, "random")),
            &encoded,
            |bn, inp| bn.iter(|| hex::decode_to_slice(inp, &mut out[..inp.len() / 2]).unwrap()),
        );
        g.bench_with_input(
            BenchmarkId::new("hex-simd", label(size, "random")),
            &encoded,
            |bn, inp| {
                bn.iter(|| {
                    hex_simd::decode(inp, hex_simd::Out::from_slice(&mut out[..inp.len() / 2]))
                        .unwrap()
                        .len()
                })
            },
        );
        g.finish();
    }
}

fn bench_base64(c: &mut Criterion) {
    let std_engine = base64::engine::general_purpose::STANDARD;
    for size in sizes() {
        let raw = gen_raw("random", size);
        let encoded = run_to_vec(
            &mut Base64Encoder::new(Backend::Scalar, Alphabet::Standard, true),
            &raw,
        )
        .unwrap();
        let mut out = vec![0u8; (size + 2) / 3 * 4 + 64];

        let mut g = c.benchmark_group("base64/encode");
        g.throughput(Throughput::Bytes(size as u64));
        for b in Backend::available() {
            let mut t = Base64Encoder::new(b, Alphabet::Standard, true);
            g.bench_with_input(
                BenchmarkId::new(format!("vcodec-{}", b.name()), label(size, "random")),
                &raw,
                |bn, inp| bn.iter(|| run(&mut t, inp, &mut out)),
            );
        }
        g.bench_with_input(
            BenchmarkId::new("base64-crate", label(size, "random")),
            &raw,
            |bn, inp| bn.iter(|| std_engine.encode_slice(inp, &mut out).unwrap()),
        );
        g.bench_with_input(
            BenchmarkId::new("base64-simd", label(size, "random")),
            &raw,
            |bn, inp| {
                bn.iter(|| {
                    base64_simd::STANDARD
                        .encode(inp, base64_simd::Out::from_slice(&mut out))
                        .len()
                })
            },
        );
        #[cfg(feature = "openssl")]
        g.bench_with_input(
            BenchmarkId::new("openssl-EVP_EncodeBlock", label(size, "random")),
            &raw,
            |bn, inp| {
                bn.iter(|| unsafe {
                    openssl_sys::EVP_EncodeBlock(out.as_mut_ptr(), inp.as_ptr(), inp.len() as _)
                })
            },
        );
        g.finish();

        let mut g = c.benchmark_group("base64/decode");
        g.throughput(Throughput::Bytes(encoded.len() as u64));
        for b in Backend::available() {
            let mut t = Base64Decoder::new(b, Alphabet::Standard);
            g.bench_with_input(
                BenchmarkId::new(format!("vcodec-{}", b.name()), label(size, "random")),
                &encoded,
                |bn, inp| bn.iter(|| run(&mut t, inp, &mut out)),
            );
        }
        g.bench_with_input(
            BenchmarkId::new("base64-crate", label(size, "random")),
            &encoded,
            |bn, inp| bn.iter(|| std_engine.decode_slice(inp, &mut out).unwrap()),
        );
        g.bench_with_input(
            BenchmarkId::new("base64-simd", label(size, "random")),
            &encoded,
            |bn, inp| {
                bn.iter(|| {
                    base64_simd::STANDARD
                        .decode(inp, base64_simd::Out::from_slice(&mut out))
                        .unwrap()
                        .len()
                })
            },
        );
        #[cfg(feature = "openssl")]
        g.bench_with_input(
            BenchmarkId::new("openssl-EVP_DecodeBlock", label(size, "random")),
            &encoded,
            |bn, inp| {
                bn.iter(|| unsafe {
                    openssl_sys::EVP_DecodeBlock(out.as_mut_ptr(), inp.as_ptr(), inp.len() as _)
                })
            },
        );
        g.finish();
    }
}

fn bench_url(c: &mut Criterion) {
    const PE_SET: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
        .remove(b'-')
        .remove(b'.')
        .remove(b'_')
        .remove(b'~');
    for size in sizes() {
        for dist in ["safe", "text", "pct10", "binary"] {
            let raw = gen_raw(dist, size);
            let encoded =
                run_to_vec(&mut UrlEncoder::new(Backend::Scalar, UNRESERVED), &raw).unwrap();
            let mut out = vec![0u8; 3 * size + 64];

            let mut g = c.benchmark_group("url/encode");
            g.throughput(Throughput::Bytes(size as u64));
            for b in Backend::available() {
                let mut t = UrlEncoder::new(b, UNRESERVED);
                g.bench_with_input(
                    BenchmarkId::new(format!("vcodec-{}", b.name()), label(size, dist)),
                    &raw,
                    |bn, inp| bn.iter(|| run(&mut t, inp, &mut out)),
                );
            }
            g.bench_with_input(
                BenchmarkId::new("percent-encoding", label(size, dist)),
                &raw,
                |bn, inp| {
                    bn.iter(|| {
                        percent_encoding::percent_encode(inp, PE_SET)
                            .to_string()
                            .len()
                    })
                },
            );
            g.finish();

            let mut g = c.benchmark_group("url/decode");
            g.throughput(Throughput::Bytes(encoded.len() as u64));
            for b in Backend::available() {
                let mut t = UrlDecoder::new(b, false);
                g.bench_with_input(
                    BenchmarkId::new(format!("vcodec-{}", b.name()), label(size, dist)),
                    &encoded,
                    |bn, inp| bn.iter(|| run(&mut t, inp, &mut out)),
                );
            }
            g.bench_with_input(
                BenchmarkId::new("percent-encoding", label(size, dist)),
                &encoded,
                |bn, inp| {
                    bn.iter(|| {
                        percent_encoding::percent_decode(inp)
                            .collect::<Vec<u8>>()
                            .len()
                    })
                },
            );
            g.finish();
        }
    }
}

fn config() -> Criterion {
    Criterion::default()
        .warm_up_time(Duration::from_millis(300))
        .measurement_time(Duration::from_millis(900))
        .sample_size(20)
        .noise_threshold(0.03)
}

criterion_group! { name = benches; config = config(); targets = bench_hex, bench_base64, bench_url }
criterion_main!(benches);
