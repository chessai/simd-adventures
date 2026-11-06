//! Kernel microbenchmarks (same protocol as the C harness).
//!
//!   pngsimd_bench_kernels [--sizes 1k,32k,256k,8m] [--dists uniform,photo,zeros] [--backend a,b] [--kernels ...] [--secs 0.25]
//!      TSV: backend kernel variant dist bytes ns_per_call MB/s cycles/byte(TSC) IPC L1D/KB LLC/KB brmiss/KB speedup
//!   pngsimd_bench_kernels once --backend B --kernel K --variant V --dist D --bytes N --reps R
//!      one configuration, R times (for valgrind --tool=cachegrind attribution)
//!
//! cycles_per_byte counts TSC ticks (`simdlab corefreq` converts to core cycles); IPC and the
//! miss columns come from perf_event hardware counters when the kernel exposes them, else 0.

use clap::{Args, Parser, Subcommand};
use pngsimd::kernels::{scalar, Kernels, ROW_SLACK};
use pngsimd::Backend;
use simdlab::cli::{parse_size, BackendArg, BackendsArg};
use simdlab::perf::{Counters, Event, Sample};
use simdlab::rng::XorShift64;
use simdlab::Backends;

#[derive(Parser)]
#[command(
    name = env!("CARGO_BIN_NAME"),
    about = "Kernel microbenchmarks: every kernel x backend x size x byte distribution, as TSV",
    args_conflicts_with_subcommands = true
)]
struct Cli {
    #[command(subcommand)]
    once: Option<Once>,
    #[command(flatten)]
    matrix: Matrix,
}

#[derive(Args)]
struct Matrix {
    #[command(flatten)]
    backends: BackendsArg<Backend>,
    /// Buffer sizes in bytes (K/M suffixes).
    #[arg(long, value_delimiter = ',', value_parser = parse_size, default_value = "1k,32k,256k,8m")]
    sizes: Vec<usize>,
    /// Byte distributions of the input buffers.
    #[arg(long, value_delimiter = ',', value_parser = DISTS, default_value = "uniform,photo")]
    dists: Vec<String>,
    /// Only these kernels or variants (e.g. `filter,crc32` or `paeth/4`).
    #[arg(long, value_delimiter = ',')]
    kernels: Vec<String>,
    /// Seconds per measurement.
    #[arg(long, default_value_t = 0.25)]
    secs: f64,
}

#[derive(Subcommand)]
enum Once {
    /// Run one configuration REPS times and exit (process totals for cachegrind attribution).
    Once {
        #[command(flatten)]
        backend: BackendArg<Backend>,
        /// Kernel: filter, filter_naive, paeth_branchless, sub4wide, memcpy, crc32, crc32_bytewise,
        /// adler32, rgb_to_rgba, u16_to_u8.
        #[arg(long)]
        kernel: String,
        /// Variant: `FILTER/BPP` for the filter kernels (e.g. `paeth/4`), `-` otherwise.
        #[arg(long, default_value = "-")]
        variant: String,
        #[arg(long, value_parser = DISTS, default_value = "photo")]
        dist: String,
        #[arg(long, value_parser = parse_size)]
        bytes: usize,
        #[arg(long)]
        reps: u64,
    },
}

/// uniform bytes, small filtered-photo residuals, all zeros, a ramp.
const DISTS: [&str; 4] = ["uniform", "photo", "zeros", "ramp"];

fn fill(rng: &mut XorShift64, p: &mut [u8], dist: &str) {
    match dist {
        "uniform" => p.iter_mut().for_each(|b| *b = rng.next_u32() as u8),
        "photo" => p.iter_mut().for_each(|b| *b = ((rng.next_u32() % 7) as i32 - 3) as u8), // small residuals: filtered photo rows
        "zeros" => p.fill(0),
        _ => p.iter_mut().enumerate().for_each(|(i, b)| *b = i as u8),
    }
}

#[derive(Clone)]
struct Config { kernel: &'static str, variant: String, filter: u8, bpp: usize }

struct Bufs { row: Vec<u8>, prev: Vec<u8>, dst: Vec<u8>, src_copy: Vec<u8>, sink: u32 }

impl Bufs {
    fn new(maxbytes: usize) -> Bufs {
        Bufs { row: vec![0; maxbytes + 4096], prev: vec![0; maxbytes + 4096], dst: vec![0; maxbytes * 2 + 4096], src_copy: vec![0; maxbytes + 64], sink: 0 }
    }
    fn prepare(&mut self, rng: &mut XorShift64, dist: &str, bytes: usize) {
        fill(rng, &mut self.row[..bytes + ROW_SLACK], dist);
        fill(rng, &mut self.prev[..bytes + ROW_SLACK], dist);
        self.src_copy[..bytes].copy_from_slice(&self.row[..bytes]);
    }
}

const FILTER_NAMES: [&str; 5] = ["none", "sub", "up", "avg", "paeth"];

fn config(kernel: &'static str, variant: &str) -> Config {
    let (filter, bpp) = if let Some((f, b)) = variant.split_once('/') {
        (FILTER_NAMES.iter().position(|&n| n == f).unwrap_or(0) as u8, b.parse().unwrap_or(4))
    } else {
        (0, 4)
    };
    Config { kernel, variant: variant.to_string(), filter, bpp }
}

/// one call of the configured kernel over `bytes` bytes; filters restore the row first (in-place kernels)
#[inline(never)]
fn run_once(k: &Kernels, c: &Config, bytes: usize, b: &mut Bufs) {
    let prev = Some(&b.prev[..bytes]);
    match c.kernel {
        "filter" => { b.row[..bytes].copy_from_slice(&b.src_copy[..bytes]); (k.unfilter_row)(c.filter, c.bpp, &mut b.row[..bytes + ROW_SLACK], prev, bytes); }
        "filter_naive" => { b.row[..bytes].copy_from_slice(&b.src_copy[..bytes]); scalar::unfilter_row_naive(c.filter, c.bpp, &mut b.row, prev, bytes); }
        "paeth_branchless" => { b.row[..bytes].copy_from_slice(&b.src_copy[..bytes]); scalar::paeth_branchless(c.bpp, &mut b.row, prev, bytes); }
        "sub4wide" => {
            b.row[..bytes].copy_from_slice(&b.src_copy[..bytes]);
            #[cfg(target_arch = "x86_64")]
            pngsimd::kernels::x86::filter_sub4_wide(&mut b.row, bytes);
        }
        "memcpy" => b.row[..bytes].copy_from_slice(&b.src_copy[..bytes]),   // the restore alone
        "crc32" => b.sink = b.sink.wrapping_add((k.crc32)(b.sink, &b.row[..bytes])),
        "crc32_bytewise" => b.sink = b.sink.wrapping_add(scalar::crc32_bytewise(b.sink, &b.row[..bytes])),
        "adler32" => b.sink = b.sink.wrapping_add((k.adler32)(1, &b.row[..bytes])),
        "rgb_to_rgba" => { let n = bytes / 3; (k.rgb_to_rgba)(&b.row[..3 * n], &mut b.dst[..4 * n], 255) }
        "u16_to_u8" => { let n = bytes / 2; (k.u16be_to_u8)(&b.row[..2 * n], &mut b.dst[..n]) }
        _ => {}
    }
}

/// Per-call averages.
fn measure(k: &Kernels, c: &Config, dist: &str, bytes: usize, secs: f64, counters: &Counters, b: &mut Bufs, rng: &mut XorShift64) -> Sample {
    b.prepare(rng, dist, bytes);
    run_once(k, c, bytes, b);
    run_once(k, c, bytes, b);
    // calibrate the batch to ~2 ms, then run for `secs`
    let mut batch = 1u64;
    loop {
        let s = counters.measure(|| for _ in 0..batch { run_once(k, c, bytes, b); });
        if s.ns > 2e6 || batch > 1 << 24 { break; }
        batch *= 2;
    }
    let mut total = Sample::ZERO;
    let mut reps = 0;
    while total.ns < secs * 1e9 {
        total += &counters.measure(|| for _ in 0..batch { run_once(k, c, bytes, b); });
        reps += batch;
    }
    total.per(reps as f64)
}

/// The counters reported in the TSV (LLC = the generic `cache-misses` event, as in the C harness).
const EVENTS: [Event; 6] = [Event::Cycles, Event::Instructions, Event::L1dReadMisses, Event::CacheMisses, Event::BranchMisses, Event::Branches];

fn zero_if_nan(x: f64) -> f64 {
    if x.is_nan() { 0.0 } else { x }
}

fn main() {
    let cli = Cli::parse();
    let counters = Counters::new(&EVENTS);
    let mut rng = XorShift64::new(0x2545_F491_4F6C_DD1D);

    if let Some(Once::Once { backend, kernel, variant, dist, bytes, reps }) = cli.once {
        let k = backend.backend.kernels();
        let c = config(Box::leak(kernel.into_boxed_str()), &variant);
        let mut b = Bufs::new(bytes);
        b.prepare(&mut rng, &dist, bytes);
        for _ in 0..reps { run_once(k, &c, bytes, &mut b); }
        println!("done {} {} {} {} {} x {} (sink {})", backend.backend.name(), c.kernel, c.variant, dist, bytes, reps, b.sink);
        return;
    }

    let m = cli.matrix;
    let maxbytes = *m.sizes.iter().max().unwrap();
    let mut b = Bufs::new(maxbytes);

    let mut cfgs: Vec<Config> = Vec::new();
    let bpps = [1usize, 2, 3, 4, 6, 8];
    for f in 1..5 { for &bpp in &bpps { cfgs.push(config("filter", &format!("{}/{}", FILTER_NAMES[f], bpp))); } }
    cfgs.push(config("memcpy", "restore"));
    for f in 1..5 { for &bpp in &bpps { cfgs.push(config("filter_naive", &format!("{}/{}", FILTER_NAMES[f], bpp))); } }
    for &bpp in &bpps[..4] { cfgs.push(config("paeth_branchless", &format!("paeth/{}", bpp))); }
    cfgs.push(config("sub4wide", "sub/4"));
    for kname in ["crc32", "crc32_bytewise", "adler32", "rgb_to_rgba", "u16_to_u8"] { cfgs.push(config(kname, "-")); }

    println!("# cycles_per_byte: TSC ticks; IPC / miss columns: {}", if counters.any_available() { "perf_event hardware counters" } else { "no PMU exposed (0)" });
    println!("backend\tkernel\tvariant\tdist\tbytes\tns_per_call\tMB_per_s\tcycles_per_byte\tIPC\tL1D_miss_per_KB\tLLC_miss_per_KB\tbr_miss_per_KB\tspeedup_vs_scalar");
    let scalar = Backend::Scalar.kernels();
    for c in &cfgs {
        if !m.kernels.is_empty() && !m.kernels.iter().any(|x| x == c.kernel || *x == c.variant) { continue; }
        for &bytes in &m.sizes { for dist in &m.dists {
            let ms = measure(scalar, c, dist, bytes, m.secs, &counters, &mut b, &mut rng);
            for &backend in &m.backends.backends {
                let s = if backend == Backend::Scalar { ms } else { measure(backend.kernels(), c, dist, bytes, m.secs, &counters, &mut b, &mut rng) };
                let per_kb = |e: Event| zero_if_nan(s.get(e)) * 1024.0 / bytes as f64;
                println!("{}\t{}\t{}\t{}\t{}\t{:.1}\t{:.1}\t{:.4}\t{:.2}\t{:.3}\t{:.3}\t{:.3}\t{:.2}",
                    backend.name(), c.kernel, c.variant, dist, bytes, s.ns, bytes as f64 / s.ns * 1e3, s.ticks / bytes as f64,
                    zero_if_nan(s.ipc()), per_kb(Event::L1dReadMisses), per_kb(Event::CacheMisses), per_kb(Event::BranchMisses), ms.ns / s.ns);
            }
        } }
    }
}
