//! Decode every PNG in `corpus/` with every backend and compare pixel-for-pixel with the `png`
//! crate (image-rs), normalised to the same canonical layout (G8 / GA8 / RGBA8, 16-bit stripped
//! to the high byte). Also checks that a flipped IDAT byte is caught by the CRC.

use pngsimd::{Backend, Error, Options};
use simdlab::Backends;
use std::path::Path;

fn reference(data: &[u8]) -> (u32, u32, usize, Vec<u8>) {
    let mut dec = png::Decoder::new(data);
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut r = dec.read_info().unwrap();
    let mut buf = vec![0u8; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).unwrap();
    assert_eq!(info.bit_depth, png::BitDepth::Eight);
    let w = info.width as usize;
    let (ch, px): (usize, Vec<u8>) = match info.color_type {
        png::ColorType::Grayscale => (1, buf[..info.buffer_size()].to_vec()),
        png::ColorType::GrayscaleAlpha => (2, buf[..info.buffer_size()].to_vec()),
        png::ColorType::Rgba => (4, buf[..info.buffer_size()].to_vec()),
        png::ColorType::Rgb => {
            let mut v = Vec::with_capacity(w * info.height as usize * 4);
            for p in buf[..info.buffer_size()].chunks_exact(3) { v.extend_from_slice(p); v.push(255); }
            (4, v)
        }
        png::ColorType::Indexed => unreachable!("EXPAND"),
    };
    (info.width, info.height, ch, px)
}

fn corpus_dir() -> Option<std::path::PathBuf> {
    for p in ["corpus", "c/corpus"] {
        if Path::new(p).join("manifest.tsv").exists() { return Some(p.into()); }
    }
    None
}

#[test]
fn corpus_identical_to_png_crate() {
    let Some(dir) = corpus_dir() else { eprintln!("no corpus/ (run scripts/make_corpus.py); skipping"); return; };
    let mut names: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path())
        .filter(|p| p.extension().map_or(false, |x| x == "png")).collect();
    names.sort();
    assert!(!names.is_empty());
    let backends = Backend::available();
    for path in &names {
        let mut data = std::fs::read(path).unwrap();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let (rw, rh, rch, rpx) = reference(&data);
        for &b in &backends {
            let opt = Options { backend: Some(b), ..Default::default() };
            let img = pngsimd::decode(&data, &opt).unwrap_or_else(|e| panic!("{name} [{}]: {e}", b.name()));
            assert_eq!((img.width, img.height, img.channels as usize), (rw, rh, rch), "{name} [{}] geometry", b.name());
            if img.pixels != rpx {
                let j = img.pixels.iter().zip(&rpx).position(|(a, b)| a != b).unwrap();
                panic!("{name} [{}]: pixel data differs at byte {j} (row {}): {:02x} vs {:02x}", b.name(), j / img.stride, img.pixels[j], rpx[j]);
            }
        }
        // corruption: flip one byte inside the first IDAT -> CRC error expected (default options)
        let mut pos = 8usize;
        let mut idat = 0usize;
        while pos + 12 <= data.len() {
            let clen = u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
            if &data[pos + 4..pos + 8] == b"IDAT" { idat = pos + 8 + clen / 2; break; }
            pos += 12 + clen;
        }
        assert!(idat != 0);
        data[idat] ^= 0x40;
        assert_eq!(pngsimd::decode(&data, &Options::default()).err(), Some(Error::Crc), "{name}: corrupted IDAT byte not caught by CRC");
        // without CRC checking the inflate/adler layer must fail cleanly (or, rarely, collide)
        let _ = pngsimd::decode(&data, &Options { verify_crc: false, ..Default::default() });
    }
    eprintln!("{} files x {} backends identical to the png crate", names.len(), backends.len());
}
