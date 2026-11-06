//! False-positive rate vs bits/key, Bloom variants vs cuckoo at EQUAL MEMORY (port of
//! src/fpr.cpp). The cuckoo table is fixed at 2 MiB (power-of-two buckets); for each bits/key
//! target the key count n is chosen so every structure gets exactly 2 MiB, and the cuckoo load
//! factor follows from that (n/a if > 95%).

use clap::Parser;
use simdlab::cli::BackendArg;
use vecfilter::keygen::{make_keys, make_queries};
use vecfilter::simd::Ops;
use vecfilter::{with_backend, Backend, BlockedBloom, ClassicBloom, CuckooFilter};

#[derive(Parser)]
#[command(name = env!("CARGO_BIN_NAME"), about = "False-positive rate vs bits/key at equal memory (port of vecfilter/src/fpr.cpp)")]
struct Cli {
    /// Distribution of the inserted keys.
    #[arg(default_value = "uniform", value_parser = ["uniform", "sequential", "clustered"])]
    dist: String,
    #[command(flatten)]
    backend: BackendArg<Backend>,
}

fn run<O: Ops>(o: O, dist: &str) {
    const M: usize = 2 << 20;
    const NQ: usize = 4 << 20;
    let bpks = [8.5, 10.0, 12.0, 14.0, 17.0, 20.0, 24.0, 32.0];
    println!("dist={dist}  memory={M} bytes for every structure, {NQ} absent queries");
    println!(
        "{:>6} {:>10} | {:>10} {:>10} {:>10} {:>10} | {:>16} {:>16}",
        "bpk", "n keys", "classic", "blocked32", "blocked64", "blk64x2", "cuckoo8(load)", "cuckoo16(load)"
    );
    println!("{:>6} {:>10} | {:>10} {:>10} {:>10} {:>10} | {:>16} {:>16}", "", "", "k=opt", "k=8", "k=8", "k=16", "FPR", "FPR");
    for bpk in bpks {
        let n = (M as f64 * 8.0 / bpk) as usize;
        let keys = make_keys(dist, n, 1234);
        let q = make_queries("absent", &keys, NQ, 999);
        let kopt = ((bpk * 2f64.ln()).round() as u32).max(1);
        let mut cb = ClassicBloom::new(M, kopt);
        let mut b32 = BlockedBloom::<O, 32>::new(o, M);
        let mut b64 = BlockedBloom::<O, 64>::new(o, M);
        let mut b64x2 = BlockedBloom::<O, 64, 2>::new(o, M);
        let mut c8 = CuckooFilter::<O, u8>::new(o, M);
        let mut c16 = CuckooFilter::<O, u16>::new(o, M);
        keys.iter().for_each(|&k| cb.insert(k));
        b32.insert_batch(&keys);
        b64.insert_batch(&keys);
        b64x2.insert_batch(&keys);
        let (l8, l16) = (n as f64 / c8.capacity() as f64, n as f64 / c16.capacity() as f64);
        let f8 = if l8 <= 0.95 { c8.insert_batch(&keys) } else { 1 };
        let f16 = if l16 <= 0.95 { c16.insert_batch(&keys) } else { 1 };
        let fcb = q.iter().filter(|&&k| cb.contains(k)).count();
        let rate = |hits: usize| hits as f64 / NQ as f64;
        let (r32, r64, r64x2) = (rate(b32.count_batch(&q)), rate(b64.count_batch(&q)), rate(b64x2.count_batch(&q)));
        let (r8, r16) = (rate(c8.count_batch(&q)), rate(c16.count_batch(&q)));
        let s8 = if f8 == 0 { format!("{r8:.5} ({l8:.2})") } else { format!("n/a ({l8:.2})") };
        let s16 = if f16 == 0 { format!("{r16:.6} ({l16:.2})") } else { format!("n/a ({l16:.2})") };
        println!(
            "{bpk:6.1} {n:10} | {:10.5} {r32:10.5} {r64:10.5} {r64x2:10.5} | {s8:>16} {s16:>16}   (classic k={kopt})",
            rate(fcb)
        );
    }
}

fn main() {
    let cli = Cli::parse();
    with_backend!(cli.backend.backend, run(&cli.dist));
}
