//! Experiment-independent tools.
//!
//!   simdlab cpu                         CPU model, detected features, perf counter availability
//!   simdlab corefreq                    core clock relative to the TSC (for TSC-only machines)
//!   simdlab membench [MAX_MIB]          random 64-byte access latency / memory-level parallelism

use clap::{Parser, Subcommand};
use simdlab::buf::{self, AlignedBuf};
use simdlab::perf::{self, Counters, Event};
use simdlab::rng::SplitMix64;
use std::time::Instant;

#[derive(Parser)]
#[command(name = env!("CARGO_BIN_NAME"), about = "Experiment-independent tools for the SIMD experiments")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// CPU model, the features simdlab can detect (yes/no), and which perf counters open.
    Cpu,
    /// Estimate the core clock relative to the TSC with a dependent imul chain (x86-64; ~10 s).
    /// Benchmarks that fall back to TSC ticks convert with core cycles = ticks x core_per_tsc.
    Corefreq,
    /// Memory-system roofline for random 64-byte accesses: one dependent pointer chase (latency)
    /// and K interleaved chases (memory-level parallelism), for working sets of 1 MiB .. MAX_MIB.
    Membench {
        /// Largest working set in MiB (sizes go 1, 4, 16, ... up to this).
        #[arg(default_value_t = 2048)]
        max_mib: usize,
        /// Do not request transparent huge pages.
        #[arg(long)]
        no_hugepages: bool,
    },
}

fn main() {
    match Cli::parse().cmd {
        Cmd::Cpu => cpu(),
        Cmd::Corefreq => match perf::core_clock(7) {
            Some(c) => println!("tsc_ghz={:.3} core_per_tsc={:.3} core_ghz={:.2}", c.tsc_ghz, c.core_per_tsc, c.core_ghz()),
            None => println!("tsc_ghz=0 core_per_tsc=1.000 core_ghz=0"),
        },
        Cmd::Membench { max_mib, no_hugepages } => {
            if no_hugepages {
                buf::set_hugepages(false);
            }
            membench(max_mib);
        }
    }
}

fn cpu() {
    println!("arch: {}", std::env::consts::ARCH);
    if let Some(m) = simdlab::cpu::model_name() {
        println!("model: {m}");
    }
    let (yes, no): (Vec<&str>, Vec<&str>) = simdlab::cpu::KNOWN.iter().partition(|f| simdlab::cpu::has(f));
    println!("features: {}", yes.join(" "));
    println!("not supported: {}", if no.is_empty() { "-".to_string() } else { no.join(" ") });
    println!("perf counters: {}", Counters::new(&Event::ALL).status());
}

/// Random cyclic permutation over cache lines (Sattolo's algorithm).
fn make_cycle(nlines: usize, seed: u64) -> Vec<u32> {
    let mut next: Vec<u32> = (0..nlines as u32).collect();
    let mut r = SplitMix64::new(seed);
    for i in (1..nlines).rev() {
        next.swap(i, (r.next_u64() % i as u64) as usize);
    }
    next
}

/// `next` is stored one u32 per 64-byte line (stride 16 u32) so each hop is a new line.
fn chase<const K: usize>(next: &[u32], nlines: usize, steps: usize, sink: &mut u64) -> f64 {
    let mut p: [u32; K] = std::array::from_fn(|k| (k as u64 * 7919 % nlines as u64) as u32);
    let t0 = Instant::now();
    for _ in 0..steps {
        for x in &mut p {
            *x = next[*x as usize * 16];
        }
    }
    let ns = t0.elapsed().as_nanos() as f64;
    *sink += p.iter().map(|&x| x as u64).sum::<u64>();
    ns / (steps * K) as f64
}

fn membench(max_mib: usize) {
    println!("hugepages={}", if buf::hugepages() { "madvise" } else { "off" });
    println!("working-set   latency(1)   mlp=4    mlp=8   mlp=16   mlp=32   [ns per random 64B line]");
    let mut mb = 1;
    while mb <= max_mib {
        let nlines = (mb << 20) / 64;
        let perm = make_cycle(nlines, 1);
        let mut lines_buf = AlignedBuf::zeroed(nlines * 64);
        let lines = lines_buf.as_mut_slice_of::<u32>();
        for (i, &p) in perm.iter().enumerate() {
            lines[i * 16] = p;
        }
        drop(perm);
        let steps = (nlines * 2).min(8 << 20);
        let mut sink = 0;
        let l1 = chase::<1>(lines, nlines, steps, &mut sink);
        let l4 = chase::<4>(lines, nlines, steps / 4, &mut sink);
        let l8 = chase::<8>(lines, nlines, steps / 8, &mut sink);
        let l16 = chase::<16>(lines, nlines, steps / 16, &mut sink);
        let l32 = chase::<32>(lines, nlines, steps / 32, &mut sink);
        println!("{mb:7} MiB   {l1:8.1}   {l4:6.2}   {l8:6.2}   {l16:6.2}   {l32:6.2}");
        if sink == 42 {
            println!("{sink}");
        }
        mb *= 4;
    }
}
