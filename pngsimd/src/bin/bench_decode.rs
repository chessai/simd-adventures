//! Whole-decoder benchmark on a corpus: per-stage breakdown for every backend, plus wall-clock
//! comparison with the `png` crate (feature `pngcrate`) and the system libpng through its
//! simplified API (feature `libpng`), and the `+zlib` rows (feature `zlib`).
//!
//!   pngsimd_bench_decode [corpus_dir] [--reps-ms 300] [--backend scalar,sse41,avx2,avx512] [--zlib] [--fdeflate]
//!
//! TSV rows: file variant total_ns parse_crc inflate adler unfilter expand out_bytes file_bytes note
use clap::Parser;
use pngsimd::{Backend, Inflater, Options, STAGE_COUNT};
use simdlab::cli::BackendsArg;
use simdlab::Backends;
use std::path::PathBuf;
use std::time::Instant;

#[derive(Parser)]
#[command(name = env!("CARGO_BIN_NAME"), about = "Whole-decoder benchmark on a PNG corpus: per-stage times per backend and inflater (TSV)")]
struct Cli {
    /// Directory of .png files.
    #[arg(default_value = "corpus")]
    corpus: PathBuf,
    /// Time budget per file and variant, in milliseconds (best of at least 3 runs).
    #[arg(long, default_value_t = 300)]
    reps_ms: u64,
    #[command(flatten)]
    backends: BackendsArg<Backend>,
    /// Also run every backend with the system zlib as the inflater (needs `--features zlib`).
    #[arg(long)]
    zlib: bool,
    /// Also run every backend with the fdeflate crate as the inflater (needs `--features fdeflate`).
    #[arg(long)]
    fdeflate: bool,
}

#[derive(Clone, Copy, Default)]
struct Res { total: u64, stage: [u64; STAGE_COUNT], out_bytes: u64 }

fn decode_ours(data: &[u8], backend: Backend, inflater: Inflater) -> Option<Res> {
    let opt = Options { backend: Some(backend), inflater, ..Default::default() };
    let t0 = Instant::now();
    let img = pngsimd::decode(data, &opt).ok()?;
    let total = t0.elapsed().as_nanos() as u64;
    Some(Res { total, stage: img.stage_ns, out_bytes: (img.stride * img.height as usize) as u64 })
}

#[cfg(feature = "pngcrate")]
fn decode_pngcrate(data: &[u8]) -> Option<Res> {
    let t0 = Instant::now();
    let mut dec = png::Decoder::new(data);
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut r = dec.read_info().ok()?;
    let mut buf = vec![0u8; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).ok()?;
    // same output layout as ours: RGB gets an alpha channel added (the png crate has no filler transform)
    let out_bytes = if info.color_type == png::ColorType::Rgb {
        let mut v = Vec::with_capacity(info.buffer_size() / 3 * 4);
        for p in buf[..info.buffer_size()].chunks_exact(3) { v.extend_from_slice(p); v.push(255); }
        v.len() as u64
    } else { info.buffer_size() as u64 };
    Some(Res { total: t0.elapsed().as_nanos() as u64, stage: [0; STAGE_COUNT], out_bytes })
}

#[cfg(feature = "libpng")]
mod libpng {
    use super::Res;
    use std::time::Instant;
    #[repr(C)]
    pub struct PngImage { opaque: *mut u8, version: u32, width: u32, height: u32, format: u32, flags: u32, colormap_entries: u32, warning_or_error: u32, message: [u8; 64] }
    #[link(name = "png16")]
    extern "C" {
        fn png_image_begin_read_from_memory(image: *mut PngImage, memory: *const u8, size: usize) -> i32;
        fn png_image_finish_read(image: *mut PngImage, background: *const u8, buffer: *mut u8, row_stride: i32, colormap: *mut u8) -> i32;
        fn png_image_free(image: *mut PngImage);
    }
    const PNG_FORMAT_FLAG_ALPHA: u32 = 1;
    const PNG_FORMAT_FLAG_COLOR: u32 = 2;
    /// libpng's simplified API (no setjmp needed from the caller). 16-bit inputs are converted
    /// to 8-bit sRGB by libpng (gamma-aware), which is more work than our plain strip; noted.
    pub fn decode(data: &[u8]) -> Option<(Res, bool)> {
        unsafe {
            let t0 = Instant::now();
            let mut img: PngImage = std::mem::zeroed();
            img.version = 1;
            if png_image_begin_read_from_memory(&mut img, data.as_ptr(), data.len()) == 0 { return None; }
            let linear = img.format & 4 != 0;   // PNG_FORMAT_FLAG_LINEAR: 16-bit source
            let has_color = img.format & PNG_FORMAT_FLAG_COLOR != 0;
            let has_alpha = img.format & PNG_FORMAT_FLAG_ALPHA != 0;
            img.format = if has_color { PNG_FORMAT_FLAG_COLOR | PNG_FORMAT_FLAG_ALPHA } else if has_alpha { PNG_FORMAT_FLAG_ALPHA } else { 0 };
            let ch = if has_color { 4 } else if has_alpha { 2 } else { 1 };
            let size = img.width as usize * img.height as usize * ch;
            let mut buf = vec![0u8; size];
            if png_image_finish_read(&mut img, std::ptr::null(), buf.as_mut_ptr(), 0, std::ptr::null_mut()) == 0 { png_image_free(&mut img); return None; }
            let total = t0.elapsed().as_nanos() as u64;
            Some((Res { total, stage: [0; super::STAGE_COUNT], out_bytes: size as u64 }, linear))
        }
    }
}

fn repeat(budget_ms: u64, mut f: impl FnMut() -> Option<Res>) -> Res {
    let start = Instant::now();
    let mut best = Res { total: u64::MAX, ..Default::default() };
    let mut n = 0;
    loop {
        match f() { Some(r) => if r.total < best.total { best = r; }, None => return Res::default() }
        n += 1;
        if start.elapsed().as_millis() as u64 >= budget_ms && n >= 3 { break; }
    }
    best
}

fn main() {
    let cli = Cli::parse();
    let (budget_ms, with_zlib, with_fdeflate) = (cli.reps_ms, cli.zlib, cli.fdeflate);
    if with_zlib && !cfg!(feature = "zlib") { eprintln!("--zlib needs --features zlib"); std::process::exit(2); }
    if with_fdeflate && !cfg!(feature = "fdeflate") { eprintln!("--fdeflate needs --features fdeflate"); std::process::exit(2); }
    // by default every supported backend except "portable" (always slower than scalar)
    let mut ks = cli.backends.backends;
    if !cli.backends.explicit {
        ks.retain(|&b| b != Backend::Portable);
    }
    let mut names: Vec<_> = std::fs::read_dir(&cli.corpus).expect("corpus dir").filter_map(|e| e.ok()).map(|e| e.path())
        .filter(|p| p.extension().map_or(false, |x| x == "png")).collect();
    names.sort();

    let mut variants: Vec<String> = ks.iter().map(|k| k.name().to_string()).collect();
    if with_zlib { for k in &ks { variants.push(format!("{}+zlib", k.name())); } }
    if with_fdeflate { for k in &ks { variants.push(format!("{}+fdeflate", k.name())); } }
    if cfg!(feature = "pngcrate") { variants.push("png-crate".into()); }
    if cfg!(feature = "libpng") { variants.push("libpng".into()); }
    let nv = variants.len();
    let mut agg_total = vec![0f64; nv];
    let mut agg_stage = vec![[0f64; STAGE_COUNT]; nv];
    let (mut agg_out, mut agg_file) = (0f64, 0f64);

    println!("file\tvariant\ttotal_ns\tparse_crc_ns\tinflate_ns\tadler_ns\tunfilter_ns\texpand_ns\tout_bytes\tfile_bytes\tnote");
    let emit = |name: &str, v: usize, r: &Res, len: usize, note: &str, agg_total: &mut Vec<f64>, agg_stage: &mut Vec<[f64; STAGE_COUNT]>| {
        print!("{}\t{}\t{}", name, variants[v], r.total);
        for s in 0..STAGE_COUNT { print!("\t{}", r.stage[s]); }
        println!("\t{}\t{}\t{}", r.out_bytes, len, note);
        agg_total[v] += r.total as f64;
        for s in 0..STAGE_COUNT { agg_stage[v][s] += r.stage[s] as f64; }
    };
    for path in &names {
        let data = std::fs::read(path).unwrap();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let mut v = 0;
        for (j, k) in ks.iter().enumerate() {
            let best = repeat(budget_ms, || decode_ours(&data, *k, Inflater::Builtin));
            emit(&name, v, &best, data.len(), "", &mut agg_total, &mut agg_stage); v += 1;
            if j == 0 { agg_out += best.out_bytes as f64; agg_file += data.len() as f64; }
        }
        if with_zlib {
            for k in &ks {
                let best = repeat(budget_ms, || decode_ours(&data, *k, Inflater::Zlib));
                emit(&name, v, &best, data.len(), "inflate+adler by zlib", &mut agg_total, &mut agg_stage); v += 1;
            }
        }
        if with_fdeflate {
            for k in &ks {
                let best = repeat(budget_ms, || decode_ours(&data, *k, Inflater::Fdeflate));
                emit(&name, v, &best, data.len(), "inflate+adler by fdeflate", &mut agg_total, &mut agg_stage); v += 1;
            }
        }
        #[cfg(feature = "pngcrate")]
        { let best = repeat(budget_ms, || decode_pngcrate(&data)); emit(&name, v, &best, data.len(), "", &mut agg_total, &mut agg_stage); v += 1; }
        #[cfg(feature = "libpng")]
        {
            let mut linear = false;
            let best = repeat(budget_ms, || libpng::decode(&data).map(|(r, l)| { linear = l; r }));
            emit(&name, v, &best, data.len(), if linear { "libpng simplified API converts 16-bit through sRGB gamma" } else { "" }, &mut agg_total, &mut agg_stage);
            v += 1;
        }
        let _ = v;
    }
    println!("\n# corpus summary ({} files, {:.1} MB of PNG, {:.1} MB of pixels): sum of per-file best times", names.len(), agg_file / 1e6, agg_out / 1e6);
    println!("# variant\ttotal_ms\tpixels_MB_per_s\tspeedup_vs_{}\tparse_crc%\tinflate%\tadler%\tunfilter%\texpand%\tother%", variants[0]);
    for v in 0..nv {
        let t = agg_total[v];
        print!("# {}\t{:.1}\t{:.1}\t{:.2}", variants[v], t / 1e6, agg_out / t * 1e3, agg_total[0] / t);
        let mut acc = 0.0;
        for s in 0..STAGE_COUNT { print!("\t{:.1}", agg_stage[v][s] / t * 100.0); acc += agg_stage[v][s]; }
        println!("\t{:.1}", if acc > 0.0 { (t - acc) / t * 100.0 } else { 0.0 });
    }
}
