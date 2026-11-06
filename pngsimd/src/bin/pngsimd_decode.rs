//! Command-line decoder: `pngsimd_decode in.png [out.ppm]` — prints the stage breakdown.
use clap::Parser;
use pngsimd::{Backend, Options, STAGE_NAMES};
use simdlab::cli::BackendArg;
use simdlab::Backends;
use std::io::Write;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = env!("CARGO_BIN_NAME"), about = "Decode a PNG, print the per-stage time breakdown, optionally write a PPM/PGM")]
struct Cli {
    /// PNG to decode.
    input: PathBuf,
    /// Write the decoded pixels here (PGM for grey, PPM otherwise; alpha dropped).
    output: Option<PathBuf>,
    #[command(flatten)]
    backend: BackendArg<Backend>,
}

fn main() {
    let cli = Cli::parse();
    let backend = cli.backend.backend;
    let data = std::fs::read(&cli.input).expect("read");
    let opt = Options { backend: Some(backend), ..Default::default() };
    let img = match pngsimd::decode(&data, &opt) { Ok(i) => i, Err(e) => { eprintln!("decode failed: {e}"); std::process::exit(1); } };
    println!("{}x{} depth {} colour type {} -> {} channels, backend {}", img.width, img.height, img.bit_depth, img.color_type, img.channels, backend.name());
    println!("filters: none {} sub {} up {} avg {} paeth {}", img.filter_hist[0], img.filter_hist[1], img.filter_hist[2], img.filter_hist[3], img.filter_hist[4]);
    for (i, name) in STAGE_NAMES.iter().enumerate() {
        println!("  {:<12} {:8.3} ms  ({} bytes)", name, img.stage_ns[i] as f64 / 1e6, img.stage_bytes[i]);
    }
    if let Some(out) = cli.output {
        let mut f = std::io::BufWriter::new(std::fs::File::create(out).expect("create"));
        let (w, h) = (img.width as usize, img.height as usize);
        if img.channels == 1 { write!(f, "P5\n{} {}\n255\n", w, h).unwrap(); } else { write!(f, "P6\n{} {}\n255\n", w, h).unwrap(); }
        for row in img.pixels.chunks_exact(img.stride) {
            match img.channels {
                1 => f.write_all(row).unwrap(),
                2 => for px in row.chunks_exact(2) { f.write_all(&[px[0], px[0], px[0]]).unwrap(); },
                c => for px in row.chunks_exact(c as usize) { f.write_all(&px[..3]).unwrap(); },
            }
        }
    }
}
