//! `vcodec_bench`: throughput and hardware counters of the streaming transforms.
//!
//! ```text
//! vcodec_bench                                    # every codec x op x backend x size x dist
//! vcodec_bench --codec base64 --op decode --backend avx2 --size 64k --dist random
//! ```
//!
//! TSV on stdout, one row per combination: GB/s and ns/byte of input, and where the PMU is
//! reachable cycles/byte, IPC, branch-misses, L1D and LLC load misses per KiB ("n/a" otherwise,
//! with `pmu` = no). The same binary is driven by `perf stat`, cachegrind and llvm-mca in
//! `scripts/`.

use clap::Parser;
use simdlab::cli::{parse_size, BackendsArg};
use simdlab::perf::{Counters, Event, Sample};
use simdlab::Backends;
use vcodec::stream::Status;
use vcodec::workload::{self, Codec, Dist, Op};
use vcodec::Backend;

#[derive(Parser)]
#[command(name = env!("CARGO_BIN_NAME"), about = "Throughput and hardware counters of the vcodec transforms")]
struct Cli {
    #[command(flatten)]
    backends: BackendsArg<Backend>,
    /// Codecs to run.
    #[arg(long, visible_alias = "codecs", value_delimiter = ',', default_values = ["hex", "base64", "url"])]
    codec: Vec<Codec>,
    /// Directions to run.
    #[arg(long, visible_alias = "ops", value_delimiter = ',', default_values = ["encode", "decode"])]
    op: Vec<Op>,
    /// Input sizes in bytes (decoders get encoded input of at most this size).
    #[arg(long, visible_alias = "sizes", value_delimiter = ',', value_parser = parse_size, default_value = "64,1k,64k,4M")]
    size: Vec<usize>,
    /// Input distributions [default: url: safe,text,pct10,binary; hex, base64: random].
    #[arg(long, visible_alias = "dists", value_delimiter = ',')]
    dist: Vec<Dist>,
    /// Transform calls per measurement [default: 256 MiB of input, clamped to 8 ..= 4M calls].
    #[arg(long)]
    iters: Option<usize>,
    /// Measurements per row; the fastest is reported (the most robust estimate on a noisy machine).
    #[arg(long, default_value_t = 3)]
    repeat: usize,
}

const EVENTS: [Event; 6] =
    [Event::Cycles, Event::Instructions, Event::Branches, Event::BranchMisses, Event::L1dReadMisses, Event::LlcReadMisses];

/// One measurement of `iters` one-shot runs (reset, update, finish) over `input`.
fn measure(counters: &Counters, codec: Codec, op: Op, backend: Backend, input: &[u8], iters: usize) -> Sample {
    let mut t = workload::transform(codec, op, backend);
    let mut out = vec![0u8; t.max_output_len(input.len()) + 64];
    // sanity: one run must complete
    let s = t.update(input, &mut out);
    assert_eq!(s.status, Status::Done, "{}/{}/{}: {:?}", codec.name(), op.name(), backend.name(), s.status);
    assert_eq!(s.consumed, input.len());
    assert_eq!(t.finish(&mut out[s.written..]).status, Status::Done);

    let mut run = |n: usize| {
        let mut sink = 0usize;
        for _ in 0..n {
            t.reset();
            let s = t.update(input, &mut out);
            let f = t.finish(&mut out[s.written..]);
            sink = sink.wrapping_add(s.written + f.written);
        }
        std::hint::black_box(sink);
    };
    run((iters / 16).max(1)); // warm-up
    counters.measure(|| run(iters))
}

fn main() {
    let cli = Cli::parse();
    let counters = Counters::new(&EVENTS);
    eprintln!("perf counters: {}", counters.status());
    let pmu = if counters.any_available() { "yes" } else { "no" };
    let fmt = |x: f64| if x.is_nan() { "n/a".to_string() } else { format!("{x:.3}") };

    println!("codec\top\tbackend\tsize\tdist\tGB/s\tns/byte\tcyc/byte\tIPC\tbr-miss/KiB\tL1d-miss/KiB\tLLC-miss/KiB\tpmu");
    for &codec in &cli.codec {
        let dists = if cli.dist.is_empty() { codec.default_dists() } else { &cli.dist[..] };
        for &op in &cli.op {
            for &backend in &cli.backends.backends {
                for &size in &cli.size {
                    for &dist in dists {
                        let input = workload::input(codec, op, dist, size);
                        let iters = cli.iters.unwrap_or_else(|| ((256usize << 20) / input.len().max(1)).clamp(8, 1 << 22));
                        let best = (0..cli.repeat.max(1))
                            .map(|_| measure(&counters, codec, op, backend, &input, iters))
                            .min_by(|a, b| a.ns.total_cmp(&b.ns))
                            .unwrap();
                        let bytes = (iters * input.len()) as f64;
                        let kib = bytes / 1024.0;
                        println!(
                            "{}\t{}\t{}\t{size}\t{}\t{:.3}\t{:.4}\t{}\t{}\t{}\t{}\t{}\t{pmu}",
                            codec.name(),
                            op.name(),
                            backend.name(),
                            dist.name(),
                            bytes / best.ns,
                            best.ns / bytes,
                            fmt(best.get(Event::Cycles) / bytes),
                            fmt(best.ipc()),
                            fmt(best.get(Event::BranchMisses) / kib),
                            fmt(best.get(Event::L1dReadMisses) / kib),
                            fmt(best.get(Event::LlcReadMisses) / kib),
                        );
                    }
                }
            }
        }
    }
}
