//! Scalar inflate (RFC 1951) and the zlib wrapper (RFC 1950).
//!
//! Deliberately a plain, decent scalar decoder: 64-bit bit buffer with branchless refill,
//! 10-bit first-level lookup tables with a canonical-code slow path for longer codes,
//! overlap-aware copies. Why it stays scalar is REPORT.md §6: the length of each Huffman code
//! is only known after it has been decoded, so the start of symbol i+1 depends on the decode
//! of symbol i — a true serial dependency through the bit position with a data-dependent table
//! lookup inside it.

use crate::Error;

const FAST_BITS: u32 = 10;
const FAST_SIZE: usize = 1 << FAST_BITS;
const MAXBITS: usize = 15;

struct Huff {
    /// (symbol << 4) | length, or 0 if the code is longer than FAST_BITS
    fast: Box<[u16; FAST_SIZE]>,
    /// left-justified "first code of the next length", can be 0x10000 (hence u32)
    maxcode: [u32; MAXBITS + 2],
    firstcode: [u16; MAXBITS + 1],
    firstsymbol: [u16; MAXBITS + 1],
    value: [u16; 288],
    size: [u8; 288],
}

impl Huff {
    fn new() -> Self {
        Huff { fast: Box::new([0; FAST_SIZE]), maxcode: [0; MAXBITS + 2], firstcode: [0; MAXBITS + 1],
               firstsymbol: [0; MAXBITS + 1], value: [0; 288], size: [0; 288] }
    }

    fn build(&mut self, lens: &[u8]) -> Result<(), Error> {
        let mut sizes = [0i32; MAXBITS + 2];
        let mut next_code = [0i32; MAXBITS + 2];
        self.fast.fill(0);
        for &l in lens { sizes[l as usize] += 1; }
        sizes[0] = 0;
        for i in 1..=MAXBITS { if sizes[i] > (1 << i) { return Err(Error::Zlib); } }
        let mut code = 0i32;
        let mut k = 0i32;
        for i in 1..=MAXBITS {
            next_code[i] = code;
            self.firstcode[i] = code as u16;
            self.firstsymbol[i] = k as u16;
            code += sizes[i];
            if sizes[i] != 0 && code - 1 >= (1 << i) { return Err(Error::Zlib); } // over-subscribed
            self.maxcode[i] = (code as u32) << (16 - i);
            code <<= 1;
            k += sizes[i];
        }
        self.maxcode[MAXBITS + 1] = 0x10000;
        for (i, &len) in lens.iter().enumerate() {
            let len = len as usize;
            if len == 0 { continue; }
            let c = (next_code[len] - self.firstcode[len] as i32 + self.firstsymbol[len] as i32) as usize;
            self.size[c] = len as u8;
            self.value[c] = i as u16;
            if len as u32 <= FAST_BITS {
                let fastv = ((i as u16) << 4) | len as u16;
                let mut j = bit_reverse(next_code[len] as u32, len) as usize;
                while j < FAST_SIZE { self.fast[j] = fastv; j += 1 << len; }
            }
            next_code[len] += 1;
        }
        Ok(())
    }
}

#[inline]
fn bit_reverse(v: u32, n: usize) -> u32 { v.reverse_bits() >> (32 - n) }

#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub literals: u64,
    pub matches: u64,
    pub match_bytes: u64,
    pub blocks_stored: u64,
    pub blocks_fixed: u64,
    pub blocks_dynamic: u64,
}

struct State<'a> {
    input: &'a [u8],
    pos: usize,       // next byte to load into the bit buffer
    bits: u64,        // bit buffer, LSB first
    nbits: u32,
    zeros_fed: u32,   // phantom zero bytes fed past the end of input
    out: &'a mut [u8],
    opos: usize,
    st: Stats,
}

const LEN_BASE: [u16; 29] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const LEN_EXTRA: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DIST_BASE: [u16; 30] = [1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577];
const DIST_EXTRA: [u8; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];
const CLEN_ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];

impl<'a> State<'a> {
    /// top up to >= 56 bits; past the end of input, feed zeros (errors surface later)
    #[inline(always)]
    fn refill(&mut self) {
        if self.pos + 8 <= self.input.len() {
            let w = u64::from_le_bytes(self.input[self.pos..self.pos + 8].try_into().unwrap());
            self.bits |= w << self.nbits;
            self.pos += ((63 - self.nbits) >> 3) as usize;
            self.nbits |= 56;
        } else {
            while self.nbits <= 56 {
                let b = if self.pos < self.input.len() { let b = self.input[self.pos]; self.pos += 1; b } else { self.zeros_fed += 1; 0 };
                self.bits |= (b as u64) << self.nbits;
                self.nbits += 8;
            }
        }
    }

    #[inline(always)]
    fn getbits(&mut self, n: u32) -> u32 {
        if self.nbits < n { self.refill(); }
        let v = (self.bits & ((1u64 << n) - 1)) as u32;
        self.bits >>= n;
        self.nbits -= n;
        v
    }

    #[cold]
    fn decode_slow(&mut self, h: &Huff) -> Result<u32, Error> {
        // 16-bit MSB-first view of the next bits
        let k = bit_reverse((self.bits & 0xFFFF) as u32, 16);
        let mut len = FAST_BITS as usize + 1;
        loop {
            if len > MAXBITS { return Err(Error::Zlib); }
            if k < h.maxcode[len] { break; }
            len += 1;
        }
        let b = (k >> (16 - len)) as i32 - h.firstcode[len] as i32 + h.firstsymbol[len] as i32;
        if b < 0 || b >= 288 || h.size[b as usize] as usize != len { return Err(Error::Zlib); }
        self.bits >>= len;
        self.nbits -= len as u32;
        Ok(h.value[b as usize] as u32)
    }

    #[inline(always)]
    fn decode(&mut self, h: &Huff) -> Result<u32, Error> {
        if self.nbits < 16 { self.refill(); }
        let e = h.fast[(self.bits & (FAST_SIZE as u64 - 1)) as usize];
        if e != 0 {
            let len = (e & 15) as u32;
            self.bits >>= len;
            self.nbits -= len;
            return Ok((e >> 4) as u32);
        }
        self.decode_slow(h)
    }

    fn inflate_block(&mut self, lit: &Huff, dist: &Huff) -> Result<(), Error> {
        loop {
            let sym = self.decode(lit)?;
            if sym < 256 {
                if self.opos >= self.out.len() { return Err(Error::Zlib); }
                self.out[self.opos] = sym as u8;
                self.opos += 1;
                self.st.literals += 1;
            } else if sym == 256 {
                return Ok(());
            } else {
                let sym = (sym - 257) as usize;
                if sym >= 29 { return Err(Error::Zlib); }
                let mut len = LEN_BASE[sym] as usize;
                if LEN_EXTRA[sym] != 0 { len += self.getbits(LEN_EXTRA[sym] as u32) as usize; }
                let dsym = self.decode(dist)? as usize;
                if dsym >= 30 { return Err(Error::Zlib); }
                let mut d = DIST_BASE[dsym] as usize;
                if DIST_EXTRA[dsym] != 0 { d += self.getbits(DIST_EXTRA[dsym] as u32) as usize; }
                if d > self.opos || self.opos + len > self.out.len() { return Err(Error::Zlib); }
                self.st.matches += 1;
                self.st.match_bytes += len as u64;
                let dst = self.opos;
                let src = dst - d;
                self.opos += len;
                if d >= len {
                    // non-overlapping: a plain memcpy (copy_within would be a memmove)
                    let (before, after) = self.out.split_at_mut(dst);
                    after[..len].copy_from_slice(&before[src..src + len]);
                } else if d == 1 {
                    let b = self.out[src];
                    self.out[dst..dst + len].fill(b);
                } else {
                    // overlapping copy: copy in chunks of d, doubling as the valid span grows
                    let mut done = 0;
                    let mut span = d;
                    while done < len {
                        let chunk = span.min(len - done);
                        self.out.copy_within(src..src + chunk, dst + done);
                        done += chunk;
                        span += chunk;
                    }
                }
            }
        }
    }

    fn read_dynamic(&mut self, lit: &mut Huff, dist: &mut Huff) -> Result<(), Error> {
        let hlit = self.getbits(5) as usize + 257;
        let hdist = self.getbits(5) as usize + 1;
        let hclen = self.getbits(4) as usize + 4;
        if hlit > 286 || hdist > 30 { return Err(Error::Zlib); }
        let mut clens = [0u8; 19];
        for i in 0..hclen { clens[CLEN_ORDER[i]] = self.getbits(3) as u8; }
        let mut clh = Huff::new();
        clh.build(&clens)?;
        let mut lens = [0u8; 288 + 32];
        let mut n = 0usize;
        while n < hlit + hdist {
            let sym = self.decode(&clh)?;
            if sym < 16 {
                lens[n] = sym as u8;
                n += 1;
            } else {
                let (fill, rep) = match sym {
                    16 => { if n == 0 { return Err(Error::Zlib); } (lens[n - 1], 3 + self.getbits(2) as usize) }
                    17 => (0, 3 + self.getbits(3) as usize),
                    _ => (0, 11 + self.getbits(7) as usize),
                };
                if n + rep > hlit + hdist { return Err(Error::Zlib); }
                lens[n..n + rep].fill(fill);
                n += rep;
            }
        }
        if lens[256] == 0 { return Err(Error::Zlib); }
        lit.build(&lens[..hlit])?;
        dist.build(&lens[hlit..hlit + hdist])?;
        Ok(())
    }
}

/// Raw DEFLATE. Returns (bytes written, bytes of `src` consumed). `dst` must hold the whole output.
pub fn inflate(src: &[u8], dst: &mut [u8], stats: Option<&mut Stats>) -> Result<(usize, usize), Error> {
    let mut s = State { input: src, pos: 0, bits: 0, nbits: 0, zeros_fed: 0, out: dst, opos: 0, st: Stats::default() };
    let mut lit = Huff::new();
    let mut dist = Huff::new();
    loop {
        let fin = s.getbits(1);
        let ty = s.getbits(2);
        match ty {
            0 => {
                // discard to the byte boundary: nbits % 8 bits
                let drop = s.nbits & 7;
                s.getbits(drop);
                let len = s.getbits(16) as usize;
                let nlen = s.getbits(16) as usize;
                if len ^ 0xFFFF != nlen { return Err(Error::Zlib); }
                if s.opos + len > s.out.len() { return Err(Error::Zlib); }
                s.st.blocks_stored += 1;
                let mut len = len;
                // the bit buffer may still hold up to 7 whole bytes: drain those first
                while len > 0 && s.nbits > 0 {
                    s.out[s.opos] = (s.bits & 0xFF) as u8;
                    s.opos += 1;
                    s.bits >>= 8;
                    s.nbits -= 8;
                    len -= 1;
                }
                if len > 0 {
                    // buffer empty (nbits == 0); bits above nbits may be stale copies of input
                    // left by the branchless refill and must go before we skip input bytes
                    s.bits = 0;
                    if len > s.input.len() - s.pos { return Err(Error::Truncated); }
                    s.out[s.opos..s.opos + len].copy_from_slice(&s.input[s.pos..s.pos + len]);
                    s.opos += len;
                    s.pos += len;
                }
            }
            1 => {
                let mut lens = [0u8; 288 + 32];
                for (i, l) in lens.iter_mut().enumerate() {
                    *l = match i { 0..=143 => 8, 144..=255 => 9, 256..=279 => 7, 280..=287 => 8, _ => 5 };
                }
                lit.build(&lens[..288])?;
                dist.build(&lens[288..])?;
                s.st.blocks_fixed += 1;
                s.inflate_block(&lit, &dist)?;
            }
            2 => {
                s.st.blocks_dynamic += 1;
                s.read_dynamic(&mut lit, &mut dist)?;
                s.inflate_block(&lit, &dist)?;
            }
            _ => return Err(Error::Zlib),
        }
        if fin == 1 { break; }
    }
    // unused whole bytes still in the bit buffer are given back; if the decoder ran past the
    // real input (phantom zero bytes were consumed) the stream was truncated
    let unused_real = (s.nbits >> 3) as i64 - s.zeros_fed as i64;
    if unused_real < 0 { return Err(Error::Truncated); }
    if let Some(st) = stats { *st = s.st; }
    Ok((s.opos, s.pos - unused_real as usize))
}

/// zlib stream (RFC 1950): header, raw deflate, Adler-32 trailer. Returns (bytes written,
/// trailer Adler-32). If `adler32` is given the trailer is verified with it.
pub fn zlib_inflate(src: &[u8], dst: &mut [u8], stats: Option<&mut Stats>, adler32: Option<fn(u32, &[u8]) -> u32>) -> Result<(usize, u32), Error> {
    if src.len() < 6 { return Err(Error::Zlib); }
    if src[0] & 0x0F != 8 || ((src[0] as u32) << 8 | src[1] as u32) % 31 != 0 || src[1] & 0x20 != 0 { return Err(Error::Zlib); }
    let (n, consumed) = inflate(&src[2..], dst, stats)?;
    if 2 + consumed + 4 > src.len() { return Err(Error::Truncated); }
    let t = &src[2 + consumed..2 + consumed + 4];
    let trailer = u32::from_be_bytes([t[0], t[1], t[2], t[3]]);
    if let Some(f) = adler32 {
        if f(1, &dst[..n]) != trailer { return Err(Error::Adler); }
    }
    Ok((n, trailer))
}
