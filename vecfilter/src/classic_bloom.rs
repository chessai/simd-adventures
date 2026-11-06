//! The simple scalar baseline: a plain Bloom filter with k independent bit positions
//! (Kirsch-Mitzenmacher double hashing), each one a potentially different cache line, tested in a
//! loop with early exit.

use simdlab::buf::AlignedBuf;
use crate::hash::hash1;

pub struct ClassicBloom {
    bits: AlignedBuf,
    nbits: usize,
    k: u32,
}

impl ClassicBloom {
    pub fn new(bytes: usize, k: u32) -> Self {
        let nbits = if bytes < 8 { 64 } else { bytes * 8 };
        ClassicBloom { bits: AlignedBuf::zeroed(nbits / 8), nbits, k }
    }
    pub fn bytes(&self) -> usize {
        self.nbits / 8
    }
    pub fn k(&self) -> u32 {
        self.k
    }

    /// `(h * nbits) >> 64`: one mul instruction, works for any size.
    #[inline(always)]
    fn index(&self, h: u64) -> usize {
        ((h as u128 * self.nbits as u128) >> 64) as usize
    }
    #[inline(always)]
    fn hashes(key: u64) -> (u64, u64) {
        let (hb, hl) = hash1(key);
        (((hb as u64) << 32) | hl as u64, (((hl as u64) << 32) | hb as u64) | 1)
    }

    pub fn insert(&mut self, key: u64) {
        let (mut h1, h2) = Self::hashes(key);
        for _ in 0..self.k {
            let idx = self.index(h1);
            self.bits.as_mut_slice_of::<u64>()[idx >> 6] |= 1 << (idx & 63);
            h1 = h1.wrapping_add(h2);
        }
    }
    pub fn contains(&self, key: u64) -> bool {
        let (mut h1, h2) = Self::hashes(key);
        let words = self.bits.as_slice_of::<u64>();
        for _ in 0..self.k {
            let idx = self.index(h1);
            if (words[idx >> 6] >> (idx & 63)) & 1 == 0 {
                return false; // early exit (branchy, as in real code)
            }
            h1 = h1.wrapping_add(h2);
        }
        true
    }
}
