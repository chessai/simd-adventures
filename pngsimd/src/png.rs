//! Minimal PNG decoder: chunk walk, inflate, unfilter, expand — with per-stage timers.

use crate::inflate;
use crate::kernels::ROW_SLACK;
use crate::Backend;
use simdlab::Backends;
use crate::Error;
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum Stage {
    /// chunk walk + CRC-32 of every chunk
    ParseCrc = 0,
    /// Huffman/LZ77 decode (scalar)
    Inflate,
    /// Adler-32 of the inflated stream
    Adler,
    /// scanline reconstruction
    Unfilter,
    /// RGB->RGBA / 16->8 / palette / sub-byte unpack
    Expand,
}
pub const STAGE_COUNT: usize = 5;
pub const STAGE_NAMES: [&str; STAGE_COUNT] = ["parse+crc32", "inflate", "adler32", "unfilter", "expand"];

#[derive(Clone, Debug, Default)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub bit_depth: u8,
    pub color_type: u8,
    /// output channels: 1 (G), 2 (GA), 3 (RGB, only if expansion is off) or 4 (RGBA)
    pub channels: u8,
    /// `width * height * channels` bytes, 8-bit
    pub pixels: Vec<u8>,
    pub stride: usize,
    // instrumentation
    pub stage_ns: [u64; STAGE_COUNT],
    pub stage_bytes: [u64; STAGE_COUNT],
    /// rows per filter type
    pub filter_hist: [u64; 5],
    pub idat_bytes: u64,
    pub raw_bytes: u64,
    pub inflate_stats: inflate::Stats,
}

#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// `None` = the best backend this CPU supports
    pub backend: Option<Backend>,
    /// check every chunk CRC (libpng's default behaviour)
    pub verify_crc: bool,
    /// check the zlib trailer
    pub verify_adler: bool,
    /// 3-channel output becomes 4-channel
    pub expand_rgb_to_rgba: bool,
    pub alpha_fill: u8,
    /// which inflater runs the inflate stage (the alternatives need the `zlib` / `fdeflate` features)
    pub inflater: Inflater,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Inflater {
    /// this crate's scalar inflate
    Builtin,
    /// the system zlib through flate2 (feature `zlib`)
    Zlib,
    /// the `fdeflate` crate, a pure-Rust inflater specialised for PNG data (feature `fdeflate`)
    Fdeflate,
}

impl Default for Options {
    fn default() -> Self {
        Options { backend: None, verify_crc: true, verify_adler: true, expand_rgb_to_rgba: true, alpha_fill: 255, inflater: Inflater::Builtin }
    }
}

fn be32(p: &[u8]) -> u32 { u32::from_be_bytes([p[0], p[1], p[2], p[3]]) }

/// unpack 1/2/4-bit samples to one byte each (gray scaled to 0..255 if `scale`; palette indices kept)
fn unpack_bits(src: &[u8], dst: &mut [u8], depth: u32, scale: bool) {
    let per = 8 / depth;
    let mask = (1u32 << depth) - 1;
    let mul = if scale { 255 / mask } else { 1 };
    for (i, d) in dst.iter_mut().enumerate() {
        let byte = src[i / per as usize] as u32;
        let shift = 8 - depth * (1 + (i as u32) % per);
        *d = (((byte >> shift) & mask) * mul) as u8;
    }
}

struct Timer(Instant);
impl Timer {
    fn start() -> Self { Timer(Instant::now()) }
    fn lap(&mut self) -> u64 { let now = Instant::now(); let d = now.duration_since(self.0).as_nanos() as u64; self.0 = now; d }
}

pub fn decode(data: &[u8], opt: &Options) -> Result<Image, Error> {
    const SIG: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
    let k = opt.backend.unwrap_or_else(Backend::best).kernels();
    let mut img = Image::default();
    if data.len() < 8 || data[..8] != SIG { return Err(Error::Signature); }

    // ---- pass 1: walk the chunks, verify CRCs, collect headers and IDAT size ----
    let mut t = Timer::start();
    let (mut width, mut height, mut depth, mut ctype, mut interlace) = (0u32, 0u32, 0u32, 0u8, 0u8);
    let mut palette = [0xFFu8; 256 * 4];
    let mut have_plte = false;
    let mut idat_total = 0usize;
    let mut pos = 8usize;
    while pos + 12 <= data.len() {
        let clen = be32(&data[pos..]) as usize;
        if clen > 0x7FFF_FFFF || pos + 12 + clen > data.len() { return Err(Error::Truncated); }
        let ctype_b = &data[pos + 4..pos + 8];
        let cdata = &data[pos + 8..pos + 8 + clen];
        if opt.verify_crc {
            let want = be32(&data[pos + 8 + clen..]);
            if (k.crc32)(0, &data[pos + 4..pos + 8 + clen]) != want { return Err(Error::Crc); }
        }
        match ctype_b {
            b"IHDR" => {
                if clen != 13 { return Err(Error::Ihdr); }
                width = be32(cdata); height = be32(&cdata[4..]);
                depth = cdata[8] as u32; ctype = cdata[9]; interlace = cdata[12];
                if width == 0 || height == 0 || width > 1 << 24 || height > 1 << 24 || cdata[10] != 0 || cdata[11] != 0 { return Err(Error::Ihdr); }
            }
            b"PLTE" => {
                if clen % 3 != 0 || clen > 768 { return Err(Error::Chunk); }
                for (i, e) in cdata.chunks_exact(3).enumerate() { palette[4 * i..4 * i + 3].copy_from_slice(e); palette[4 * i + 3] = 255; }
                have_plte = true;
            }
            b"tRNS" => {
                if ctype == 3 { for (i, &a) in cdata.iter().take(256).enumerate() { palette[4 * i + 3] = a; } }
                // colour-key transparency of gray/RGB images is not applied (libpng's default without png_set_tRNS_to_alpha)
            }
            b"IDAT" => idat_total += clen,
            b"IEND" => break,
            _ => {}
        }
        pos += 12 + clen;
    }
    if width == 0 { return Err(Error::Ihdr); }
    if interlace != 0 { return Err(Error::Unsupported); }
    let in_ch: usize = match (ctype, depth) {
        (0, 1 | 2 | 4 | 8 | 16) => 1,
        (2, 8 | 16) => 3,
        (3, 1 | 2 | 4 | 8) if have_plte => 1,
        (4, 8 | 16) => 2,
        (6, 8 | 16) => 4,
        _ => return Err(Error::Ihdr),
    };

    // ---- pass 2: concatenate IDAT ----
    let mut idat = Vec::with_capacity(idat_total);
    let mut pos = 8usize;
    while pos + 12 <= data.len() {
        let clen = be32(&data[pos..]) as usize;
        match &data[pos + 4..pos + 8] {
            b"IDAT" => idat.extend_from_slice(&data[pos + 8..pos + 8 + clen]),
            b"IEND" => break,
            _ => {}
        }
        pos += 12 + clen;
    }
    img.stage_ns[Stage::ParseCrc as usize] += t.lap();
    img.stage_bytes[Stage::ParseCrc as usize] += data.len() as u64;
    img.idat_bytes = idat_total as u64;

    // ---- inflate ----
    let rowbytes = ((width as usize) * in_ch * depth as usize + 7) / 8;
    let bpp = (in_ch * depth as usize + 7) / 8;
    let raw_size = height as usize * (rowbytes + 1);
    let mut raw = vec![0u8; raw_size + ROW_SLACK];
    let (n, trailer, checked_by_inflater) = match opt.inflater {
        Inflater::Builtin => {
            let (n, trailer) = inflate::zlib_inflate(&idat, &mut raw[..raw_size], Some(&mut img.inflate_stats), None)?;
            (n, trailer, false)
        }
        #[cfg(feature = "zlib")]
        Inflater::Zlib => {
            use std::io::Read;
            let mut z = flate2::read::ZlibDecoder::new(&idat[..]);
            let mut n = 0usize;
            loop {
                let got = z.read(&mut raw[n..raw_size]).map_err(|_| Error::Zlib)?;
                if got == 0 { break; }
                n += got;
            }
            (n, 0u32, true)   // zlib verifies the Adler-32 itself, inside this timing
        }
        #[cfg(feature = "fdeflate")]
        Inflater::Fdeflate => {
            let mut d = fdeflate::Decompressor::new();
            let mut n = 0usize;
            let mut ipos = 0usize;
            while !d.is_done() {
                let (ci, co) = d.read(&idat[ipos..], &mut raw[..raw_size], n, true).map_err(|_| Error::Zlib)?;
                ipos += ci; n += co;
                if ci == 0 && co == 0 { break; }
            }
            (n, 0u32, true)   // fdeflate verifies the Adler-32 itself (with simd-adler32), inside this timing
        }
        #[allow(unreachable_patterns)]
        _ => return Err(Error::Unsupported),
    };
    img.stage_ns[Stage::Inflate as usize] += t.lap();
    drop(idat);
    if n != raw_size { return Err(Error::Truncated); }
    img.stage_bytes[Stage::Inflate as usize] += raw_size as u64;
    img.raw_bytes = raw_size as u64;

    // ---- Adler-32 of the inflated stream (timed separately from the Huffman decode) ----
    if opt.verify_adler && !checked_by_inflater {
        let got = (k.adler32)(1, &raw[..raw_size]);
        img.stage_ns[Stage::Adler as usize] += t.lap();
        img.stage_bytes[Stage::Adler as usize] += raw_size as u64;
        if got != trailer { return Err(Error::Adler); }
    }

    // ---- unfilter, in place ----
    {
        let mut row_start = 0usize;
        for y in 0..height as usize {
            let f = raw[row_start];
            if f > 4 { return Err(Error::Filter); }
            img.filter_hist[f as usize] += 1;
            let (before, rest) = raw.split_at_mut(row_start + 1);
            let prev = if y == 0 { None } else { Some(&before[row_start - rowbytes..row_start]) };
            (k.unfilter_row)(f, bpp, &mut rest[..rowbytes + ROW_SLACK], prev, rowbytes);
            row_start += rowbytes + 1;
        }
    }
    img.stage_ns[Stage::Unfilter as usize] += t.lap();
    img.stage_bytes[Stage::Unfilter as usize] += raw_size as u64;

    // ---- expand to 8-bit output ----
    let out_ch: usize = match ctype { 2 => if opt.expand_rgb_to_rgba { 4 } else { 3 }, 3 => 4, _ => in_ch };
    let w = width as usize;
    let stride = w * out_ch;
    let mut pixels = vec![0u8; stride * height as usize];
    {
        let mut tmp = if depth == 16 && ctype == 2 && out_ch == 4 { vec![0u8; w * 3] } else { Vec::new() };
        let mut idx = vec![0u8; w];
        for (y, dst) in pixels.chunks_exact_mut(stride).enumerate() {
            let row = &raw[y * (rowbytes + 1) + 1..y * (rowbytes + 1) + 1 + rowbytes];
            match ctype {
                0 => match depth {
                    8 => dst.copy_from_slice(row),
                    16 => (k.u16be_to_u8)(row, dst),
                    _ => unpack_bits(row, dst, depth, true),
                },
                2 => match (depth, out_ch) {
                    (8, 4) => (k.rgb_to_rgba)(row, dst, opt.alpha_fill),
                    (8, _) => dst.copy_from_slice(row),
                    (_, 4) => { (k.u16be_to_u8)(row, &mut tmp); (k.rgb_to_rgba)(&tmp, dst, opt.alpha_fill); }
                    (_, _) => (k.u16be_to_u8)(row, dst),
                },
                3 => {
                    if depth == 8 { idx.copy_from_slice(row); } else { unpack_bits(row, &mut idx, depth, false); }
                    for (d, &i) in dst.chunks_exact_mut(4).zip(idx.iter()) { d.copy_from_slice(&palette[4 * i as usize..4 * i as usize + 4]); }
                }
                4 | 6 => if depth == 8 { dst.copy_from_slice(row) } else { (k.u16be_to_u8)(row, dst) },
                _ => unreachable!(),
            }
        }
    }
    img.stage_ns[Stage::Expand as usize] += t.lap();
    img.stage_bytes[Stage::Expand as usize] += (stride * height as usize) as u64;
    img.width = width; img.height = height; img.bit_depth = depth as u8; img.color_type = ctype;
    img.channels = out_ch as u8; img.stride = stride; img.pixels = pixels;
    Ok(img)
}
