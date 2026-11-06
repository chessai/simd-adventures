//! Cuckoo filter (Fan, Andersen, Kaminsky, Mitzenmacher 2014) with 4-slot buckets and SIMD
//! bucket scans.
//!
//!   * `FP = u16`: 8-byte buckets, FPR ~ 2*4/2^16 = 1.2e-4, ~16.8 bits/key at 95% load
//!   * `FP = u8` : 4-byte buckets, FPR ~ 2*4/2^8  = 3.1%,   ~8.4 bits/key at 95% load
//!
//! A lookup loads both candidate buckets into one 128-bit register and does a single
//! lane-equality compare + movemask. Insert/erase reuse the same scan (needle = 0 finds free
//! slots). Deletion is supported; a "victim" stash keeps the no-false-negative guarantee if an
//! insert hits the kick limit.

use crate::hash::{hash1, hash8_keys};
use crate::simd::Ops;
use simdlab::arch::{prefetch_read, prefetch_write};
use simdlab::buf::{AlignedBuf, Pod};

pub const SLOTS: usize = 4;
pub const MAX_KICKS: usize = 500;
pub const MAX_WINDOW: usize = 256;
const ALT_MUL: u32 = 0x5bd1e995;

/// Fingerprint type: `u8` or `u16`.
pub trait Fingerprint: Pod + Eq + Default + std::fmt::Debug + 'static {
    const BITS: u32;
    fn from_u32(x: u32) -> Self;
    fn to_u32(self) -> u32;
    /// Per-lane `hl >> (32 - BITS)`.
    fn top_bits<O: Ops>(o: O, hl: O::V32) -> O::V32;
    /// Bitmask over the 8 slots of buckets `a` (lanes 0..4) and `b` (lanes 4..8) equal to `needle`.
    /// # Safety
    /// `a` and `b` must each point to a whole bucket (`SLOTS` fingerprints).
    unsafe fn scan_pair<O: Ops>(o: O, a: *const Self, b: *const Self, needle: Self) -> u32;
}

impl Fingerprint for u16 {
    const BITS: u32 = 16;
    fn from_u32(x: u32) -> Self {
        x as u16
    }
    fn to_u32(self) -> u32 {
        self as u32
    }
    #[inline(always)]
    fn top_bits<O: Ops>(o: O, hl: O::V32) -> O::V32 {
        o.shr_32::<16>(hl)
    }
    #[inline(always)]
    unsafe fn scan_pair<O: Ops>(o: O, a: *const u16, b: *const u16, needle: u16) -> u32 {
        // SAFETY: caller guarantees two 8-byte buckets.
        o.match_mask_u16(unsafe { o.load2x64(a.cast(), b.cast()) }, needle)
    }
}

impl Fingerprint for u8 {
    const BITS: u32 = 8;
    fn from_u32(x: u32) -> Self {
        x as u8
    }
    fn to_u32(self) -> u32 {
        self as u32
    }
    #[inline(always)]
    fn top_bits<O: Ops>(o: O, hl: O::V32) -> O::V32 {
        o.shr_32::<24>(hl)
    }
    #[inline(always)]
    unsafe fn scan_pair<O: Ops>(o: O, a: *const u8, b: *const u8, needle: u8) -> u32 {
        // SAFETY: caller guarantees two 4-byte buckets.
        o.match_mask_u8(unsafe { o.load2x32(a, b) }, needle) & 0xFF
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Loc<FP> {
    pub i1: u32,
    pub i2: u32,
    pub fp: FP,
}

#[derive(Clone, Copy, Default)]
struct Victim<FP> {
    valid: bool,
    i: u32,
    fp: FP,
}

pub struct CuckooFilter<O: Ops, FP: Fingerprint> {
    ops: O,
    buf: AlignedBuf,
    nbuckets: usize,
    mask: u32,
    count: usize,
    window: usize,
    rng: u64,
    victim: Victim<FP>,
}

impl<O: Ops, FP: Fingerprint> CuckooFilter<O, FP> {
    pub const BUCKET_BYTES: usize = SLOTS * size_of::<FP>();

    pub fn new(ops: O, bytes: usize) -> Self {
        Self::with_window(ops, bytes, 32)
    }

    pub fn with_window(ops: O, bytes: usize, window: usize) -> Self {
        let nb = Self::buckets_for(bytes);
        assert!(nb <= u32::MAX as usize, "too many buckets");
        CuckooFilter {
            ops,
            buf: AlignedBuf::zeroed(nb * Self::BUCKET_BYTES),
            nbuckets: nb,
            mask: (nb - 1) as u32,
            count: 0,
            window: window.clamp(1, MAX_WINDOW),
            rng: 0x9E3779B97F4A7C15,
            victim: Victim::default(),
        }
    }

    /// Power of two so i2 = i1 ^ alt(fp) stays in range.
    fn buckets_for(bytes: usize) -> usize {
        let want = (bytes / Self::BUCKET_BYTES).max(2);
        let mut nb = 1;
        while nb * 2 <= want {
            nb *= 2;
        }
        nb
    }
    /// The capacity a filter of `bytes` bytes would have, without allocating it.
    pub fn capacity_for(bytes: usize) -> usize {
        Self::buckets_for(bytes) * SLOTS
    }

    pub fn bytes(&self) -> usize {
        self.nbuckets * Self::BUCKET_BYTES
    }
    pub fn capacity(&self) -> usize {
        self.nbuckets * SLOTS
    }
    pub fn size(&self) -> usize {
        self.count
    }
    pub fn load(&self) -> f64 {
        self.count as f64 / self.capacity() as f64
    }
    pub fn set_window(&mut self, w: usize) {
        self.window = w.clamp(1, MAX_WINDOW);
    }

    #[inline(always)]
    pub fn alt_index(&self, i: u32, fp: FP) -> u32 {
        i ^ (fp.to_u32().wrapping_mul(ALT_MUL) & self.mask)
    }
    #[inline(always)]
    pub fn locate(&self, key: u64) -> Loc<FP> {
        let (hb, hl) = hash1(key);
        self.locate_h(hb, hl)
    }
    #[inline(always)]
    pub fn locate_h(&self, hb: u32, hl: u32) -> Loc<FP> {
        let f = hl >> (32 - FP::BITS);
        let fp = FP::from_u32(if f == 0 { 1 } else { f }); // 0 means "empty slot"
        let i1 = hb & self.mask;
        Loc { i1, i2: self.alt_index(i1, fp), fp }
    }

    pub fn ops(&self) -> O {
        self.ops
    }

    // Every public operation runs inside `self.ops.vectorize`, so the generic code below is
    // compiled with the backend's target features.

    // ---- per-key API --------------------------------------------------------
    pub fn contains(&self, key: u64) -> bool {
        self.ops.vectorize(|| self.contains_loc(&self.locate(key)))
    }

    pub fn insert(&mut self, key: u64) -> bool {
        self.ops.vectorize(|| {
            let l = self.locate(key);
            self.insert_loc(&l)
        })
    }

    /// Removes one copy of `key`. Erasing a key that was never inserted may remove another key's
    /// fingerprint (inherent to cuckoo filters).
    pub fn erase(&mut self, key: u64) -> bool {
        self.ops.vectorize(|| self.erase_impl(key))
    }

    fn erase_impl(&mut self, key: u64) -> bool {
        let l = self.locate(key);
        let m = self.scan_pair(l.i1, l.i2, l.fp);
        if m != 0 {
            let s = m.trailing_zeros() as usize;
            *self.slot_mut(l.i1, l.i2, s) = FP::default();
            self.count -= 1;
            if self.victim.valid {
                // try to re-home the stash now that there is space
                let v = self.victim;
                self.victim.valid = false;
                self.count -= 1;
                let alt = self.alt_index(v.i, v.fp);
                self.insert_loc(&Loc { i1: v.i, i2: alt, fp: v.fp });
            }
            return true;
        }
        if self.victim_hit(&l) {
            self.victim.valid = false;
            self.count -= 1;
            return true;
        }
        false
    }

    // ---- batched API ---------------------------------------------------------
    /// `out[i] = contains(keys[i])` for every key. Panics if `out` is shorter than `keys`.
    pub fn contains_batch(&self, keys: &[u64], out: &mut [u8]) {
        assert!(out.len() >= keys.len(), "contains_batch: out shorter than keys");
        let (o, mask, w, base) = (self.ops, self.mask, self.window, self.buf.as_ptr());
        o.vectorize(|| run_batched::<O, FP>(o, keys, mask, w, base, false, |l, i| out[i] = self.contains_loc(&l) as u8))
    }
    pub fn count_batch(&self, keys: &[u64]) -> usize {
        let (o, mask, w, base) = (self.ops, self.mask, self.window, self.buf.as_ptr());
        o.vectorize(|| {
            let mut c = 0;
            run_batched::<O, FP>(o, keys, mask, w, base, false, |l, _| c += self.contains_loc(&l) as usize);
            c
        })
    }
    /// Returns the number of keys that could NOT be inserted (filter full).
    pub fn insert_batch(&mut self, keys: &[u64]) -> usize {
        let (o, mask, w, base) = (self.ops, self.mask, self.window, self.buf.as_ptr());
        o.vectorize(|| {
            let mut failed = 0;
            run_batched::<O, FP>(o, keys, mask, w, base, true, |l, _| failed += !self.insert_loc(&l) as usize);
            failed
        })
    }

    // ---- building blocks ------------------------------------------------------
    /// Bitwise, not logical, OR: a data-dependent branch on the match result mispredicts ~50% of
    /// the time on mixed hit/miss streams (see REPORT.md).
    #[inline(always)]
    fn contains_loc(&self, l: &Loc<FP>) -> bool {
        ((self.scan_pair(l.i1, l.i2, l.fp) != 0) as u32 | self.victim_hit(l) as u32) != 0
    }

    /// Bitmask over 8 lanes: lanes 0..4 = bucket i1, 4..8 = bucket i2.
    #[inline(always)]
    fn scan_pair(&self, i1: u32, i2: u32, needle: FP) -> u32 {
        // Bucket indices are always masked with `self.mask` (< nbuckets).
        debug_assert!((i1 as usize) < self.nbuckets && (i2 as usize) < self.nbuckets);
        let b = self.buf.as_ptr().cast::<FP>();
        // SAFETY: i1, i2 < nbuckets, so both buckets are in bounds.
        unsafe { FP::scan_pair(self.ops, b.add(i1 as usize * SLOTS), b.add(i2 as usize * SLOTS), needle) }
    }
    #[inline(always)]
    fn scan_one(&self, i: u32, needle: FP) -> u32 {
        self.scan_pair(i, i, needle) & 0xF
    }
    /// Slot `s` of the pair (i1, i2): 0..4 in bucket i1, 4..8 in bucket i2.
    #[inline(always)]
    fn slot_mut(&mut self, i1: u32, i2: u32, s: usize) -> &mut FP {
        let slots = self.buf.as_mut_slice_of::<FP>();
        if s < SLOTS { &mut slots[i1 as usize * SLOTS + s] } else { &mut slots[i2 as usize * SLOTS + s - SLOTS] }
    }

    fn insert_loc(&mut self, l: &Loc<FP>) -> bool {
        if self.victim.valid {
            return false;
        }
        let free = self.scan_pair(l.i1, l.i2, FP::default());
        if free != 0 {
            *self.slot_mut(l.i1, l.i2, free.trailing_zeros() as usize) = l.fp;
            self.count += 1;
            return true;
        }
        // Kick loop.
        let mut i = if self.rng_next() & 1 != 0 { l.i1 } else { l.i2 };
        let mut fp = l.fp;
        for _ in 0..MAX_KICKS {
            let s = (self.rng_next() & 3) as usize;
            std::mem::swap(&mut fp, self.slot_mut(i, i, s));
            i = self.alt_index(i, fp);
            let f = self.scan_one(i, FP::default());
            if f != 0 {
                *self.slot_mut(i, i, f.trailing_zeros() as usize) = fp;
                self.count += 1;
                return true;
            }
        }
        self.victim = Victim { valid: true, i, fp };
        self.count += 1;
        false
    }

    /// Branch-free on purpose (it sits on the lookup path).
    #[inline(always)]
    fn victim_hit(&self, l: &Loc<FP>) -> bool {
        let v = &self.victim;
        (v.valid as u32 & (v.fp == l.fp) as u32 & ((v.i == l.i1) as u32 | (v.i == l.i2) as u32)) != 0
    }
    #[inline(always)]
    fn rng_next(&mut self) -> u32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        self.rng as u32
    }
}

/// Vector hashing + prefetch of both buckets, one window ahead (see the blocked Bloom filter's
/// `run_batched` for the pipeline shape). `base` is used only for prefetch addresses.
#[inline(always)]
fn run_batched<O: Ops, FP: Fingerprint>(
    o: O,
    keys: &[u64],
    mask: u32,
    w: usize,
    base: *const u8,
    write: bool,
    mut f: impl FnMut(Loc<FP>, usize),
) {
    let n = keys.len();
    let nwin = n.div_ceil(w);
    if nwin == 0 {
        return;
    }
    let mut bufs = [[[0u32; MAX_WINDOW]; 3]; 2]; // [buf][i1, i2, fp]

    stage1::<O, FP>(o, &keys[..w.min(n)], mask, base, write, &mut bufs[0]);
    for win in 0..nwin {
        let s = win * w;
        if win + 1 < nwin {
            stage1::<O, FP>(o, &keys[s + w..(s + 2 * w).min(n)], mask, base, write, &mut bufs[(win + 1) & 1]);
        }
        let [i1, i2, fp] = &bufs[win & 1];
        for j in 0..(s + w).min(n) - s {
            f(Loc { i1: i1[j], i2: i2[j], fp: FP::from_u32(fp[j]) }, s + j);
        }
    }
}

#[inline(always)]
fn stage1<O: Ops, FP: Fingerprint>(
    o: O,
    keys: &[u64],
    mask: u32,
    base: *const u8,
    write: bool,
    [i1, i2, fp]: &mut [[u32; MAX_WINDOW]; 3],
) {
    let maskv = o.set1_32(mask);
    let altc = o.set1_32(ALT_MUL);
    let ones = o.set1_32(u32::MAX);
    let mut i = 0;
    while i + 8 <= keys.len() {
        let (hb, hl) = hash8_keys(o, keys[i..i + 8].try_into().unwrap());
        let mut f = FP::top_bits(o, hl);
        f = o.or_32(f, o.shr_32::<31>(o.add_32(f, ones))); // f==0 -> 1
        let a = o.and_32(hb, maskv);
        let b = o.xor_32(a, o.and_32(o.mullo_32(f, altc), maskv));
        i1[i..i + 8].copy_from_slice(&o.to_array(a));
        i2[i..i + 8].copy_from_slice(&o.to_array(b));
        fp[i..i + 8].copy_from_slice(&o.to_array(f));
        i += 8;
    }
    while i < keys.len() {
        let (hb, hl) = hash1(keys[i]);
        let f = hl >> (32 - FP::BITS);
        let f = if f == 0 { 1 } else { f };
        let a = hb & mask;
        i1[i] = a;
        i2[i] = a ^ (f.wrapping_mul(ALT_MUL) & mask);
        fp[i] = f;
        i += 1;
    }
    let bucket = SLOTS * size_of::<FP>();
    for j in 0..keys.len() {
        let (p, q) = (base.wrapping_add(i1[j] as usize * bucket), base.wrapping_add(i2[j] as usize * bucket));
        if write {
            prefetch_write(p);
            prefetch_write(q);
        } else {
            prefetch_read(p);
            prefetch_read(q);
        }
    }
}
