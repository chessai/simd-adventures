//! Small deterministic generators for test data and benchmark inputs.
//!
//! The sequences are part of the experiments' recorded results (vecfilter's FPR tables are
//! compared byte-for-byte with the C++ version), so never change what these return.

/// splitmix64 (vecfilter's key generator).
#[derive(Clone, Debug)]
pub struct SplitMix64 {
    s: u64,
}

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        SplitMix64 { s: seed }
    }
    #[inline(always)]
    pub fn next_u64(&mut self) -> u64 {
        self.s = self.s.wrapping_add(0x9E3779B97F4A7C15);
        mix64(self.s)
    }
    /// Uniform in [0, 1) with 53 random bits.
    #[inline(always)]
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / 9007199254740992.0)
    }
}

/// The splitmix64 output function: a strong 64-bit mixer (stateless).
#[inline(always)]
pub fn mix64(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

/// xorshift64 (13, 7, 17) (pngsimd's benchmark data generator).
#[derive(Clone, Debug)]
pub struct XorShift64 {
    s: u64,
}

impl XorShift64 {
    /// `seed` must not be zero.
    pub fn new(seed: u64) -> Self {
        assert!(seed != 0, "xorshift64 seed must be non-zero");
        XorShift64 { s: seed }
    }
    #[inline(always)]
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.s;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.s = x;
        x
    }
    /// The high 32 bits of the next state.
    #[inline(always)]
    pub fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }
}

/// xorshift64* (12, 25, 27) (vcodec's benchmark data generator).
#[derive(Clone, Debug)]
pub struct XorShift64Star {
    s: u64,
}

impl XorShift64Star {
    /// `seed` must not be zero.
    pub fn new(seed: u64) -> Self {
        assert!(seed != 0, "xorshift64* seed must be non-zero");
        XorShift64Star { s: seed }
    }
    #[inline(always)]
    pub fn next_u64(&mut self) -> u64 {
        self.s ^= self.s >> 12;
        self.s ^= self.s << 25;
        self.s ^= self.s >> 27;
        self.s.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    /// The top byte of the next output.
    #[inline(always)]
    pub fn next_u8(&mut self) -> u8 {
        (self.next_u64() >> 56) as u8
    }
    /// Uniform in `0..n`, from the high 32 bits of the next output (multiply-shift).
    #[inline(always)]
    pub fn below(&mut self, n: u32) -> u32 {
        (((self.next_u64() >> 32) * n as u64) >> 32) as u32
    }
}
