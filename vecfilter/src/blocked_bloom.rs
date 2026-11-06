//! Cache-line-blocked ("split block") Bloom filter.
//!
//! Every key maps to exactly one block of `BB` bytes (32 or 64). The block is viewed as 8 lanes
//! (u32 lanes for 32-byte blocks, u64 lanes for 64-byte blocks) and the key sets `BPL` bits in
//! *every* lane, so k = 8 or 16. Lane i's bit position is `(hl * salt_i) >> (32 - log2(lane_bits))`:
//! one vector multiply, one shift, one variable shift to build the whole k-bit mask, no per-key
//! loop. Membership is a single "all bits of mask set in block" test (vptest on x86, bic+umaxv on
//! NEON).

use simdlab::buf::AlignedBuf;
use simdlab::arch::{prefetch_read, prefetch_write};
use crate::hash::{fastrange32, hash1, hash8_keys, LANE_SALTS, LANE_SALTS2};
use crate::simd::Ops;

pub const MAX_WINDOW: usize = 256;

pub struct BlockedBloom<O: Ops, const BB: usize, const BPL: usize = 1> {
    ops: O,
    data: AlignedBuf,
    nblocks: usize,
    window: usize,
}

impl<O: Ops, const BB: usize, const BPL: usize> BlockedBloom<O, BB, BPL> {
    pub const BLOCK_BYTES: usize = BB;
    pub const K: usize = 8 * BPL;

    pub fn new(ops: O, bytes: usize) -> Self {
        Self::with_window(ops, bytes, 32)
    }

    pub fn with_window(ops: O, bytes: usize, window: usize) -> Self {
        const {
            assert!(BB == 32 || BB == 64, "block must be 32 or 64 bytes");
            assert!(BPL == 1 || BPL == 2, "1 or 2 bits per lane");
        }
        let nblocks = (bytes / BB).max(1);
        assert!(nblocks <= u32::MAX as usize, "too many blocks for 32-bit block index");
        BlockedBloom {
            ops,
            data: AlignedBuf::zeroed(nblocks * BB),
            nblocks,
            window: window.clamp(1, MAX_WINDOW),
        }
    }

    pub fn bytes(&self) -> usize {
        self.nblocks * BB
    }
    pub fn num_blocks(&self) -> usize {
        self.nblocks
    }
    pub fn data(&self) -> &[u8] {
        &self.data.as_slice_of::<u8>()[..self.bytes()]
    }
    pub fn set_window(&mut self, w: usize) {
        self.window = w.clamp(1, MAX_WINDOW);
    }

    pub fn ops(&self) -> O {
        self.ops
    }

    // Every public operation runs inside `self.ops.vectorize`, so the generic code below is
    // compiled with the backend's target features.

    // ---- per-key API --------------------------------------------------------
    pub fn insert(&mut self, key: u64) {
        self.ops.vectorize(|| {
            let (hb, hl) = hash1(key);
            self.insert_at(fastrange32(hb, self.nblocks as u32), hl)
        })
    }
    pub fn contains(&self, key: u64) -> bool {
        self.ops.vectorize(|| {
            let (hb, hl) = hash1(key);
            self.test_at(fastrange32(hb, self.nblocks as u32), hl)
        })
    }

    // ---- batched API ---------------------------------------------------------
    pub fn insert_batch(&mut self, keys: &[u64]) {
        let (o, nb, w, base) = (self.ops, self.nblocks as u32, self.window, self.data.as_ptr());
        o.vectorize(|| {
            run_batched::<O, BB, false>(o, keys, nb, w, base, true, |blk, hl, _| self.insert_at(blk, hl), |_, _, _, _, _| {})
        })
    }

    /// `out[i] = contains(keys[i])` for every key. Panics if `out` is shorter than `keys`.
    pub fn contains_batch(&self, keys: &[u64], out: &mut [u8]) {
        assert!(out.len() >= keys.len(), "contains_batch: out shorter than keys");
        let (o, nb, w, base) = (self.ops, self.nblocks as u32, self.window, self.data.as_ptr());
        // Opt-in: measured ~35% slower than one-key-per-ymm in cache on Emerald Rapids (REPORT.md §5.6).
        let paired = cfg!(feature = "avx512-paired-lookup") && O::HAS_TEST2_BLOCKS32 && BB == 32;
        o.vectorize(|| {
            if paired {
                // AVX-512: two keys' blocks and masks share one zmm; one masked test gives both answers.
                let out = std::cell::Cell::from_mut(out).as_slice_of_cells();
                run_batched::<O, BB, true>(
                    o,
                    keys,
                    nb,
                    w,
                    base,
                    false,
                    |blk, hl, i| out[i].set(self.test_at(blk, hl) as u8),
                    |blk_a, hl_a, blk_b, hl_b, i| {
                        debug_assert!((blk_a as usize) < self.nblocks && (blk_b as usize) < self.nblocks);
                        // SAFETY: block indices come from fastrange32(_, nblocks); blocks are 32-byte aligned.
                        let r = unsafe {
                            o.test2_blocks32(
                                base.add(blk_a as usize * 32),
                                base.add(blk_b as usize * 32),
                                self.mask32(hl_a),
                                self.mask32(hl_b),
                            )
                        };
                        out[i].set((r & 1) as u8);
                        out[i + 1].set((r >> 1) as u8);
                    },
                );
            } else {
                run_batched::<O, BB, false>(o, keys, nb, w, base, false, |blk, hl, i| out[i] = self.test_at(blk, hl) as u8, |_, _, _, _, _| {});
            }
        })
    }

    /// Same as `contains_batch` but returns the number of hits (for benchmarks).
    pub fn count_batch(&self, keys: &[u64]) -> usize {
        let (o, nb, w, base) = (self.ops, self.nblocks as u32, self.window, self.data.as_ptr());
        o.vectorize(|| {
            let mut c = 0;
            run_batched::<O, BB, false>(o, keys, nb, w, base, false, |blk, hl, _| c += self.test_at(blk, hl) as usize, |_, _, _, _, _| {});
            c
        })
    }

    // ---- building blocks ------------------------------------------------------
    #[inline(always)]
    fn insert_at(&mut self, blk: u32, hl: u32) {
        // Private: callers pass blk = fastrange32(_, nblocks).
        debug_assert!((blk as usize) < self.nblocks);
        let o = self.ops;
        // SAFETY: blk < nblocks, so the block is in bounds; blocks are BB-aligned (64-byte base).
        unsafe {
            let p = self.data.as_mut_ptr().add(blk as usize * BB);
            if BB == 32 {
                let m = self.mask32(hl);
                o.storeu_32(p.cast(), o.or_32(o.loadu_32(p.cast()), m));
            } else {
                let m = self.mask64(hl);
                o.store_512(p, o.or_512(o.load_512(p), m));
            }
        }
    }
    #[inline(always)]
    fn test_at(&self, blk: u32, hl: u32) -> bool {
        debug_assert!((blk as usize) < self.nblocks);
        let o = self.ops;
        // SAFETY: as for insert_at.
        unsafe {
            let p = self.data.as_ptr().add(blk as usize * BB);
            if BB == 32 {
                o.test_all_set_32(o.loadu_32(p.cast()), self.mask32(hl))
            } else {
                o.test_all_set_512(o.load_512(p), self.mask64(hl))
            }
        }
    }

    /// Per-lane bit positions: `(hl * salt) >> (32 - log2(lane bits))`.
    #[inline(always)]
    fn positions(o: O, h: O::V32, salts: &[u32; 8]) -> O::V32 {
        let x = o.mullo_32(h, o.from_array(salts));
        // `o.shr_32::<{ 32 - log2(lane bits) }>` is not expressible on stable (generic const expr).
        if BB == 32 { o.shr_32::<27>(x) } else { o.shr_32::<26>(x) }
    }
    /// The k-bit mask of a key with lane hash `hl`, for 32-byte blocks.
    #[inline(always)]
    pub fn mask32(&self, hl: u32) -> O::V32 {
        let o = self.ops;
        let h = o.set1_32(hl);
        let one = o.set1_32(1);
        let mut m = o.shl_var_32(one, Self::positions(o, h, &LANE_SALTS));
        if BPL == 2 {
            m = o.or_32(m, o.shl_var_32(one, Self::positions(o, h, &LANE_SALTS2)));
        }
        m
    }
    /// The k-bit mask of a key with lane hash `hl`, for 64-byte blocks.
    #[inline(always)]
    pub fn mask64(&self, hl: u32) -> O::B512 {
        let o = self.ops;
        let h = o.set1_32(hl);
        let mut m = o.mask_512_from_pos(Self::positions(o, h, &LANE_SALTS));
        if BPL == 2 {
            m = o.or_512(m, o.mask_512_from_pos(Self::positions(o, h, &LANE_SALTS2)));
        }
        m
    }
}

/// Software-pipelined driver shared by insert_batch / contains_batch:
///   * stage 1 (window w+1): hash 8 keys per vector op, compute block indices, store them, issue
///     one prefetch per block
///   * stage 2 (window w): do the actual mask build + test/or on blocks that were prefetched one
///     window ago
///
/// `base` is used only to compute prefetch addresses. With `PAIRED`, consecutive keys go to `f2`
/// two at a time (`f2(blk_a, hl_a, blk_b, hl_b, i)`), the odd one out to `f`.
#[allow(clippy::too_many_arguments)]
#[inline(always)]
fn run_batched<O: Ops, const BB: usize, const PAIRED: bool>(
    o: O,
    keys: &[u64],
    nb: u32,
    w: usize,
    base: *const u8,
    write: bool,
    mut f: impl FnMut(u32, u32, usize),
    mut f2: impl FnMut(u32, u32, u32, u32, usize),
) {
    let n = keys.len();
    let nwin = n.div_ceil(w);
    if nwin == 0 {
        return;
    }
    let mut blk = [[0u32; MAX_WINDOW]; 2];
    let mut lh = [[0u32; MAX_WINDOW]; 2];

    stage1::<O, BB>(o, &keys[..w.min(n)], nb, base, write, &mut blk[0], &mut lh[0]);
    for win in 0..nwin {
        let s = win * w;
        if win + 1 < nwin {
            let b = (win + 1) & 1;
            stage1::<O, BB>(o, &keys[s + w..(s + 2 * w).min(n)], nb, base, write, &mut blk[b], &mut lh[b]);
        }
        let (blk, lh) = (&blk[win & 1], &lh[win & 1]);
        let len = (s + w).min(n) - s;
        let mut j = 0;
        if PAIRED {
            while j + 2 <= len {
                f2(blk[j], lh[j], blk[j + 1], lh[j + 1], s + j);
                j += 2;
            }
        }
        while j < len {
            f(blk[j], lh[j], s + j);
            j += 1;
        }
    }
}

/// Hash one window of keys into block indices + lane hashes, then prefetch every block.
#[inline(always)]
fn stage1<O: Ops, const BB: usize>(
    o: O,
    keys: &[u64],
    nb: u32,
    base: *const u8,
    write: bool,
    blk: &mut [u32; MAX_WINDOW],
    lh: &mut [u32; MAX_WINDOW],
) {
    let nbv = o.set1_32(nb);
    let mut i = 0;
    while i + 8 <= keys.len() {
        let (hb, hl) = hash8_keys(o, keys[i..i + 8].try_into().unwrap());
        blk[i..i + 8].copy_from_slice(&o.to_array(o.mulhi_32(hb, nbv)));
        lh[i..i + 8].copy_from_slice(&o.to_array(hl));
        i += 8;
    }
    while i < keys.len() {
        // tail: scalar hash, same function
        let (hb, hl) = hash1(keys[i]);
        blk[i] = fastrange32(hb, nb);
        lh[i] = hl;
        i += 1;
    }
    for &b in &blk[..keys.len()] {
        let p = base.wrapping_add(b as usize * BB);
        if write { prefetch_write(p) } else { prefetch_read(p) }
    }
}
