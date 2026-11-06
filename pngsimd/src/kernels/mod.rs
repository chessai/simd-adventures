//! Kernel tables and runtime backend selection.

pub mod generic;
pub mod scalar;
pub mod portable;
pub mod stdsimd;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub mod x86;
#[cfg(target_arch = "aarch64")]
pub mod neon;

pub use generic::ROW_SLACK;

/// One backend's set of kernels, obtained from [`crate::Backend::kernels`]. All function pointers
/// are safe to call; each checks the slice contract (documented per field) and panics on violation.
#[derive(Clone, Copy)]
pub struct Kernels {
    pub name: &'static str,
    /// In-place reconstruction of one scanline: `row[..len]` holds the filtered bytes on entry
    /// and the reconstructed bytes on exit. `filter` is the PNG filter type 0..4, `bpp` the
    /// bytes per complete pixel (1, 2, 3, 4, 6, 8), `prev` the previous reconstructed row or
    /// `None` on the first row. Requires `row.len() >= len + ROW_SLACK` (the vector kernels
    /// load whole registers at the tail; they never *write* past `len`) and `prev.len() >= len`.
    pub unfilter_row: fn(filter: u8, bpp: usize, row: &mut [u8], prev: Option<&[u8]>, len: usize),
    /// zlib-compatible CRC-32 (reflected, polynomial 0xEDB88320); `crc` is the public value.
    pub crc32: fn(crc: u32, data: &[u8]) -> u32,
    /// zlib-compatible Adler-32; start with 1.
    pub adler32: fn(adler: u32, data: &[u8]) -> u32,
    /// 3-byte RGB -> 4-byte RGBA with constant alpha. `src.len() % 3 == 0`, `dst.len() >= src.len() / 3 * 4`.
    pub rgb_to_rgba: fn(src: &[u8], dst: &mut [u8], alpha: u8),
    /// Big-endian 16-bit samples -> 8-bit by keeping the high byte. `src.len() >= 2 * dst.len()`.
    pub u16be_to_u8: fn(src: &[u8], dst: &mut [u8]),
}

impl std::fmt::Debug for Kernels {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "Kernels({})", self.name) }
}

#[inline]
pub(crate) fn check_row_contract(bpp: usize, row: &[u8], prev: Option<&[u8]>, len: usize) {
    assert!(matches!(bpp, 1..=8), "bpp must be 1..=8");
    assert!(row.len() >= len + ROW_SLACK, "row needs len + ROW_SLACK bytes ({} < {} + {})", row.len(), len, ROW_SLACK);
    if let Some(p) = prev { assert!(p.len() >= len, "prev shorter than len"); }
}
#[inline]
pub(crate) fn check_rgba_contract(src: &[u8], dst: &[u8]) {
    assert!(src.len() % 3 == 0 && dst.len() >= src.len() / 3 * 4, "rgb_to_rgba: bad slice sizes");
}
#[inline]
pub(crate) fn check_u16_contract(src: &[u8], dst: &[u8]) {
    assert!(src.len() >= 2 * dst.len(), "u16be_to_u8: src too short");
}
