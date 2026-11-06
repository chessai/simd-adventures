//! `template_bench`: throughput of every selected backend over a range of sizes.
//!
//! TSV on stdout: backend, elements, ns per call, GB/s, cycles per element (+ source), IPC.

use clap::Parser;
use simdlab::cli::{parse_size, BackendsArg};
use simdlab::perf::{Counters, Sample};
use simdlab::rng::SplitMix64;
use simdlab::Backends;
use template::{kernels, with_backend, Backend};

#[derive(Parser)]
#[command(name = env!("CARGO_BIN_NAME"), about = "Throughput of the sum kernels per backend")]
struct Cli {
    #[command(flatten)]
    backends: BackendsArg<Backend>,
    /// Elements per call.
    #[arg(long, value_delimiter = ',', value_parser = parse_size, default_value = "1k,32k,1M")]
    sizes: Vec<usize>,
    /// Seconds per measurement.
    #[arg(long, default_value_t = 0.2)]
    secs: f64,
}

/// Calls the kernel `reps` times, generic over the token so the loop is monomorphised per backend.
fn run<K: kernels::Sum>(k: K, xs: &[u32], reps: u64) -> u32 {
    let mut sink = 0u32;
    for _ in 0..reps {
        sink = sink.wrapping_add(k.sum_u32(std::hint::black_box(xs)));
    }
    sink
}

fn main() {
    let cli = Cli::parse();
    let counters = Counters::standard();
    eprintln!("perf counters: {}", counters.status());
    println!("backend\telements\tns_per_call\tGB_per_s\tcycles_per_elem\tcycles_src\tIPC");
    let mut r = SplitMix64::new(42);
    for &n in &cli.sizes {
        let xs: Vec<u32> = (0..n).map(|_| r.next_u64() as u32).collect();
        for &b in &cli.backends.backends {
            // calibrate a batch of ~2 ms, then measure for --secs
            let mut reps = 1u64;
            while counters.measure(|| { std::hint::black_box(with_backend!(b, run(&xs, reps))); }).ns < 2e6 && reps < 1 << 30 {
                reps *= 2;
            }
            let mut total = Sample::ZERO;
            let mut calls = 0;
            while total.ns < cli.secs * 1e9 {
                total += &counters.measure(|| { std::hint::black_box(with_backend!(b, run(&xs, reps))); });
                calls += reps;
            }
            let s = total.per(calls as f64);
            let (cycles, src) = s.cycles();
            println!("{}\t{n}\t{:.1}\t{:.2}\t{:.3}\t{}\t{:.2}", b.name(), s.ns, (n * 4) as f64 / s.ns, cycles / n as f64, src.name(), s.ipc());
        }
    }
}
