//! Throughput + hardware-counter benchmark for all filters (port of src/bench.cpp).
//!
//! The C++ builds one binary per ISA backend (bench_avx2, bench_avx512, ...); here every backend
//! is compiled in and `--backend` picks one (default: the best this CPU supports). Output is the
//! same text and CSV (one row per measurement) that scripts/make_report.py turns into tables.

use clap::{Parser, ValueEnum};
use simdlab::cli::{parse_size, BackendArg};
use simdlab::perf::{Counters, Event, Sample};
use simdlab::rng::{mix64, SplitMix64};
use simdlab::{buf, Backends};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use vecfilter::cuckoo::Fingerprint;
use vecfilter::simd::Ops;
use vecfilter::{with_backend, Backend, BlockedBloom, ClassicBloom, CuckooFilter};

#[derive(Parser)]
#[command(name = env!("CARGO_BIN_NAME"), about = "Throughput and hardware counters for every filter (port of vecfilter/src/bench.cpp)")]
struct Cli {
    #[command(flatten)]
    backend: BackendArg<Backend>,
    /// Filter sizes in bytes (K/M/G suffixes).
    #[arg(long, value_delimiter = ',', value_parser = parse_size, default_value = "256K,16M,1G")]
    sizes: Vec<usize>,
    /// Lookups per measurement.
    #[arg(long, default_value_t = 16 << 20)]
    queries: usize,
    /// Distribution of the inserted keys.
    #[arg(long, value_enum, default_value_t = Dist::Uniform)]
    dist: Dist,
    /// Lookup stream: 50/50 present/absent, all absent, or Zipf-skewed present keys.
    #[arg(long, value_enum, default_value_t = QDist::Mixed)]
    qdist: QDist,
    /// Bits per key at the design point (sets the key count for each size).
    #[arg(long, default_value_t = 10.0)]
    bpk: f64,
    /// Fraction of the design-point key count to insert.
    #[arg(long, default_value_t = 1.0)]
    fill: f64,
    /// Prefetch window of the batched APIs.
    #[arg(long, default_value_t = 32)]
    window: usize,
    /// Lookup repetitions (the fastest is reported).
    #[arg(long, default_value_t = 3)]
    reps: u32,
    /// Append one CSV row per measurement to this file.
    #[arg(long)]
    csv: Option<PathBuf>,
    /// Only structures whose name contains one of these substrings.
    #[arg(long, value_delimiter = ',')]
    only: Vec<String>,
    /// Batched lookups for prefetch windows 1..256 instead of the normal runs.
    #[arg(long)]
    window_sweep: bool,
    /// Do not request transparent huge pages.
    #[arg(long)]
    no_hugepages: bool,
}

const WINDOWS: [usize; 8] = [1, 4, 8, 16, 32, 64, 128, 256];

/// Key streams that don't need the whole key set in memory (a 1 GiB filter at 10 bits/key holds
/// 858M keys).
#[derive(Clone, Copy, ValueEnum)]
enum Dist {
    Uniform,
    Sequential,
    Clustered,
}

#[derive(Clone, Copy, ValueEnum)]
enum QDist {
    Mixed,
    Absent,
    Zipf,
}

fn value_name(v: impl ValueEnum) -> String {
    v.to_possible_value().unwrap().get_name().to_string()
}

#[derive(Clone, Copy)]
struct KeyStream {
    dist: Dist,
    seed: u64,
}

impl KeyStream {
    #[inline(always)]
    fn at(&self, i: u64) -> u64 {
        match self.dist {
            Dist::Sequential => self.seed.wrapping_add(i),
            Dist::Clustered => (mix(self.seed ^ (i >> 6)) & !63) + (i & 63),
            Dist::Uniform => mix(self.seed ^ i),
        }
    }
}

#[inline(always)]
fn mix(z: u64) -> u64 {
    mix64(z.wrapping_add(0x9E3779B97F4A7C15))
}

struct Ctx {
    g: Cli,
    counters: Counters,
    csv: Option<File>,
}

struct Row<'a> {
    structure: &'a str,
    api: &'a str,
    op: &'a str,
    bytes: usize,
    nkeys: usize,
    bpk: f64,
    window: usize,
    nops: usize,
    c: Sample,
    extra: f64, // hit rate for lookups, failure count for cuckoo inserts
}

/// `printf("%W.Pf")`, including C's "nan" spelling.
fn cf(x: f64, w: usize, p: usize) -> String {
    if x.is_nan() { format!("{:>w$}", "nan") } else { format!("{x:w$.p$}") }
}

impl Ctx {
    fn emit(&mut self, backend: &str, r: &Row) {
        let n = r.nops as f64;
        let c = &r.c;
        let (cycles, src) = c.cycles();
        let from_tsc = src == simdlab::perf::CycleSource::FixedCounter;
        let ipc = if from_tsc { f64::NAN } else { c.ipc() };
        let (l1, llc, br) = (c.get(Event::L1dReadMisses), c.get(Event::LlcReadMisses), c.get(Event::BranchMisses));
        println!(
            "{:<12} {:<7} {:<7} {:9.1} MiB  n={:<10} {:5.2} bpk  W={:<3}  {:7.2} ns/op {:7.1} Mops/s  {} cyc/op{}  IPC {}  L1m {}  LLCm {}  BRm {}  {}={:.4}",
            r.structure,
            r.api,
            r.op,
            r.bytes as f64 / 1048576.0,
            r.nkeys,
            r.bpk,
            r.window,
            c.ns / n,
            n / c.ns * 1e3,
            cf(cycles / n, 7, 1),
            if from_tsc { "(tsc)" } else { "" },
            cf(ipc, 5, 2),
            cf(l1 / n, 5, 2),
            cf(llc / n, 5, 2),
            cf(br / n, 5, 3),
            if r.op == "lookup" { "hit" } else { "fail" },
            r.extra
        );
        let (dist, qdist) = (value_name(self.g.dist), value_name(self.g.qdist));
        if let Some(f) = &mut self.csv {
            writeln!(
                f,
                "{backend},{},{},{},{},{},{:.3},{dist},{qdist},{},{},{:.4},{:.3},{},{},{},{},{},{},{:.6}",
                r.structure,
                r.api,
                r.op,
                r.bytes,
                r.nkeys,
                r.bpk,
                r.window,
                r.nops,
                c.ns / n,
                n / c.ns * 1e3,
                cf(cycles / n, 0, 3),
                src.name(),
                cf(ipc, 0, 4),
                cf(l1 / n, 0, 4),
                cf(llc / n, 0, 4),
                cf(br / n, 0, 5),
                r.extra
            )
            .and_then(|_| f.flush())
            .expect("write csv");
        }
    }

    /// `--only` accepts a list of substrings.
    fn want(&self, name: &str) -> bool {
        self.g.only.is_empty() || self.g.only.iter().any(|o| !o.is_empty() && name.contains(o.as_str()))
    }

    /// Query stream: "present" keys are stream keys with index < present_limit.
    fn make_query_stream(&self, ks: &KeyStream, present_limit: usize, nq: usize, seed: u64) -> Vec<u64> {
        let mut r = SplitMix64::new(seed);
        match self.g.qdist {
            QDist::Absent => (0..nq).map(|_| r.next_u64()).collect(),
            QDist::Zipf => {
                let ln_n = (present_limit as f64).ln();
                (0..nq)
                    .map(|_| {
                        let rank = (r.unit() * ln_n).exp() as usize;
                        ks.at(rank.min(present_limit - 1) as u64)
                    })
                    .collect()
            }
            QDist::Mixed => (0..nq)
                .map(|_| if r.next_u64() & 1 != 0 { ks.at(r.next_u64() % present_limit as u64) } else { r.next_u64() })
                .collect(),
        }
    }
}
type InsertFn<F> = fn(&mut F, &[u64]) -> usize;
type LookupFn<F> = fn(&F, &[u64], &mut [u8]) -> usize;

/// Insert keys [0,n) from the stream in chunks (<=64K keys), alternating per-key and batched
/// APIs chunk by chunk so both see the same load-factor progression.
#[allow(clippy::too_many_arguments)]
fn timed_inserts<F>(
    ctx: &mut Ctx,
    backend: &str,
    f: &mut F,
    ks: &KeyStream,
    n: usize,
    name: &str,
    bytes: usize,
    bpk: f64,
    per_key: InsertFn<F>,
    batch: Option<InsertFn<F>>,
) {
    // Small chunks so the two APIs interleave finely across the load-factor curve
    // (with 1M chunks the batched API got the last, kick-heavy, cuckoo chunk every time).
    let chunk = (1usize << 16).min((n / 32).max(1024));
    let mut buf = vec![0u64; chunk];
    let mut total = [Sample::ZERO; 2];
    let (mut failed, mut nops) = ([0usize; 2], [0usize; 2]);
    let mut api = 0;
    let mut s = 0;
    while s < n {
        let e = n.min(s + chunk);
        for i in s..e {
            buf[i - s] = ks.at(i as u64);
        }
        let keys = &buf[..e - s];
        let c = ctx.counters.measure(|| {
            failed[api] += if api == 0 { per_key(f, keys) } else { batch.unwrap()(f, keys) };
        });
        total[api] += &c;
        nops[api] += e - s;
        s += chunk;
        api ^= batch.is_some() as usize;
    }
    for a in 0..2 {
        if nops[a] > 0 {
            let row = Row {
                structure: name,
                api: if a == 0 { "per-key" } else { "batch" },
                op: "insert",
                bytes,
                nkeys: n,
                bpk,
                window: ctx.g.window,
                nops: nops[a],
                c: total[a],
                extra: failed[a] as f64,
            };
            ctx.emit(backend, &row);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn timed_lookups<F>(
    ctx: &mut Ctx,
    backend: &str,
    f: &F,
    q: &[u64],
    name: &str,
    bytes: usize,
    n: usize,
    bpk: f64,
    per_key: LookupFn<F>,
    batch: Option<LookupFn<F>>,
    window: usize,
) {
    let mut out = vec![0u8; q.len()];
    for api in 0..2 {
        if api == 1 && batch.is_none() {
            break;
        }
        if ctx.g.window_sweep && api == 0 {
            continue;
        }
        let mut best = Sample::ZERO;
        let mut hits = 0;
        for r in 0..ctx.g.reps {
            let c = ctx.counters.measure(|| {
                hits = if api == 0 { per_key(f, q, &mut out) } else { batch.unwrap()(f, q, &mut out) };
            });
            if r == 0 || c.ns < best.ns {
                best = c;
            }
        }
        let row = Row {
            structure: name,
            api: if api == 0 { "per-key" } else { "batch" },
            op: "lookup",
            bytes,
            nkeys: n,
            bpk,
            window,
            nops: q.len(),
            c: best,
            extra: hits as f64 / q.len() as f64,
        };
        ctx.emit(backend, &row);
    }
}

fn bench_blocked<O: Ops, const BB: usize, const BPL: usize>(
    o: O,
    ctx: &mut Ctx,
    name: &str,
    bytes: usize,
    n: usize,
    ks: &KeyStream,
    q: &[u64],
) {
    if !ctx.want(name) {
        return;
    }
    let mut f = BlockedBloom::<O, BB, BPL>::with_window(o, bytes, ctx.g.window);
    let bpk = f.bytes() as f64 * 8.0 / n as f64;
    let fbytes = f.bytes();
    if !ctx.g.window_sweep {
        timed_inserts(
            ctx,
            O::NAME,
            &mut f,
            ks,
            n,
            name,
            fbytes,
            bpk,
            |f, k| {
                k.iter().for_each(|&k| f.insert(k));
                0
            },
            Some(|f, k| {
                f.insert_batch(k);
                0
            }),
        );
    } else {
        f.insert_batch(&q[..q.len() / 2]); // sweep mode: cheap fill
    }
    let per_key: LookupFn<BlockedBloom<O, BB, BPL>> = |f, k, out| {
        let mut h = 0;
        for (o, &k) in out.iter_mut().zip(k) {
            *o = f.contains(k) as u8;
            h += *o as usize;
        }
        h
    };
    let batch: LookupFn<BlockedBloom<O, BB, BPL>> = |f, k, out| {
        f.contains_batch(k, out);
        out[..k.len()].iter().map(|&x| x as usize).sum()
    };
    if ctx.g.window_sweep {
        for w in WINDOWS {
            f.set_window(w);
            timed_lookups(ctx, O::NAME, &f, q, name, fbytes, n, bpk, per_key, Some(batch), w);
        }
    } else {
        let w = ctx.g.window;
        timed_lookups(ctx, O::NAME, &f, q, name, fbytes, n, bpk, per_key, Some(batch), w);
    }
}

fn bench_cuckoo<O: Ops, FP: Fingerprint>(o: O, ctx: &mut Ctx, name: &str, bytes: usize, n: usize, ks: &KeyStream, q: &[u64]) {
    if !ctx.want(name) {
        return;
    }
    let mut f = CuckooFilter::<O, FP>::with_window(o, bytes, ctx.g.window);
    let nk = n.min((f.capacity() as f64 * 0.95) as usize);
    let bpk = f.bytes() as f64 * 8.0 / nk as f64;
    let fbytes = f.bytes();
    if !ctx.g.window_sweep {
        timed_inserts(
            ctx,
            O::NAME,
            &mut f,
            ks,
            nk,
            name,
            fbytes,
            bpk,
            |f, k| k.iter().filter(|&&k| !f.insert(k)).count(),
            Some(|f, k| f.insert_batch(k)),
        );
    } else {
        f.insert_batch(&q[..(q.len() / 2).min(nk)]);
    }
    let per_key: LookupFn<CuckooFilter<O, FP>> = |f, k, out| {
        let mut h = 0;
        for (o, &k) in out.iter_mut().zip(k) {
            *o = f.contains(k) as u8;
            h += *o as usize;
        }
        h
    };
    let batch: LookupFn<CuckooFilter<O, FP>> = |f, k, out| {
        f.contains_batch(k, out);
        out[..k.len()].iter().map(|&x| x as usize).sum()
    };
    // q was built by main() so that its "present" keys are among the nk keys actually inserted.
    if ctx.g.window_sweep {
        for w in WINDOWS {
            f.set_window(w);
            timed_lookups(ctx, O::NAME, &f, q, name, fbytes, nk, bpk, per_key, Some(batch), w);
        }
    } else {
        let w = ctx.g.window;
        timed_lookups(ctx, O::NAME, &f, q, name, fbytes, nk, bpk, per_key, Some(batch), w);
    }
}

fn bench_classic(ctx: &mut Ctx, backend: &str, bytes: usize, n: usize, ks: &KeyStream, q: &[u64]) {
    if !ctx.want("classic") || ctx.g.window_sweep {
        return;
    }
    let mut f = ClassicBloom::new(bytes, 7);
    let bpk = f.bytes() as f64 * 8.0 / n as f64;
    let fbytes = f.bytes();
    timed_inserts(
        ctx,
        backend,
        &mut f,
        ks,
        n,
        "classic-k7",
        fbytes,
        bpk,
        |f, k| {
            k.iter().for_each(|&k| f.insert(k));
            0
        },
        None,
    );
    let w = ctx.g.window;
    timed_lookups(
        ctx,
        backend,
        &f,
        q,
        "classic-k7",
        fbytes,
        n,
        bpk,
        |f, k, out| {
            let mut h = 0;
            for (o, &k) in out.iter_mut().zip(k) {
                *o = f.contains(k) as u8;
                h += *o as usize;
            }
            h
        },
        None,
        w,
    );
}

fn run<O: Ops>(o: O, ctx: &mut Ctx) {
    for bytes in ctx.g.sizes.clone() {
        let n = (bytes as f64 * 8.0 / ctx.g.bpk * ctx.g.fill) as usize;
        let ks = KeyStream { dist: ctx.g.dist, seed: 0x5EED0000 + bytes as u64 };
        let nq = ctx.g.queries;
        println!(
            "=== {bytes} bytes ({:.1} MiB), {n} keys, dist={} qdist={}, {nq} queries ===",
            bytes as f64 / 1048576.0,
            value_name(ctx.g.dist),
            value_name(ctx.g.qdist),
        );
        let q = ctx.make_query_stream(&ks, n, nq, 0xC0FFEE);
        bench_classic(ctx, O::NAME, bytes, n, &ks, &q);
        bench_blocked::<O, 32, 1>(o, ctx, "blocked32", bytes, n, &ks, &q);
        bench_blocked::<O, 64, 1>(o, ctx, "blocked64", bytes, n, &ks, &q);
        bench_blocked::<O, 64, 2>(o, ctx, "blocked64x2", bytes, n, &ks, &q);
        // Cuckoo filters hold fewer keys at equal memory: use a query stream whose present keys come from the smaller set.
        let n16 = n.min((CuckooFilter::<O, u16>::capacity_for(bytes) as f64 * 0.95) as usize);
        let n8 = n.min((CuckooFilter::<O, u8>::capacity_for(bytes) as f64 * 0.95) as usize);
        let q16 = if n16 == n { q.clone() } else { ctx.make_query_stream(&ks, n16, nq, 0xC0FFEE) };
        bench_cuckoo::<O, u16>(o, ctx, "cuckoo16", bytes, n, &ks, &q16);
        drop(q16);
        let q8 = if n8 == n { q } else { ctx.make_query_stream(&ks, n8, nq, 0xC0FFEE) };
        bench_cuckoo::<O, u8>(o, ctx, "cuckoo8", bytes, n, &ks, &q8);
    }
}

fn main() {
    let g = Cli::parse();
    if g.no_hugepages {
        buf::set_hugepages(false);
    }
    let csv = g.csv.as_ref().map(|path| {
        let fresh = !path.exists();
        let mut f = File::options().create(true).append(true).open(path).expect("open csv");
        if fresh {
            writeln!(f, "backend,structure,api,op,bytes,nkeys,bpk,dist,qdist,window,nops,ns_per_op,mops,cycles_per_op,cycles_src,ipc,l1_miss_per_op,llc_miss_per_op,brmiss_per_op,extra").expect("write csv");
        }
        f
    });
    let backend = g.backend.backend;
    let mut ctx = Ctx { g, counters: Counters::standard(), csv };
    println!(
        "backend={}  hugepages={}  perf counters: {}",
        backend.name(),
        if buf::hugepages() { "madvise" } else { "off" },
        ctx.counters.status()
    );
    with_backend!(backend, run(&mut ctx));
}
