//! The shared streaming API.
//!
//! Every encoder and decoder in this crate implements [`Transform`]:
//!
//! ```text
//!   ┌───────────┐  update(input_chunk, output_buf) -> Step   ┌──────────┐
//!   │ input     │ ─────────────────────────────────────────▶ │ output   │
//!   │ chunks    │  (repeat until Status::Done, re-call with  │ bounded  │
//!   │ any size  │   fresh output when Status::OutputFull)    │ buffer   │
//!   └───────────┘                                            └──────────┘
//!                 finish(output_buf) -> Step   (flush tail / padding)
//! ```
//!
//! * **Chunked input**: `update` may be called with arbitrary slices; a
//!   multi-byte unit (`%XX`, a base64 quantum, a hex pair) that straddles two
//!   calls is carried in the transform's state.
//! * **Bounded output**: `update` never writes past `output`. If it fills the
//!   buffer it returns [`Status::OutputFull`] with the number of input bytes it
//!   consumed; the caller supplies more output and passes the *unconsumed*
//!   input again. Partially emitted units are stashed inside the transform, so
//!   output can be as small as one byte.
//! * **Resumable**: the whole state is a few bytes; a transform can be
//!   serialised mid-stream (`Clone`) and resumed later.
//! * **Errors** carry the absolute offset (bytes since `reset`) of the
//!   offending input byte.

use core::fmt;

/// Result of a `update`/`finish` call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step {
    /// Input bytes consumed (always 0 for `finish`).
    pub consumed: usize,
    /// Output bytes written.
    pub written: usize,
    pub status: Status,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// All of `input` was consumed (for `finish`: the stream is complete).
    Done,
    /// The output buffer is full; call again with more output space (and the
    /// remaining input, for `update`).
    OutputFull,
    /// Malformed input. The transform is poisoned until `reset`.
    Error(Error),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Error {
    pub kind: ErrorKind,
    /// Absolute input offset (bytes since construction / `reset`) of the
    /// offending byte, or of the end of input for truncation errors.
    pub offset: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    /// A byte outside the codec's alphabet.
    InvalidByte,
    /// Input ended in the middle of a unit (odd hex length, `%X`, a 1-char
    /// base64 tail).
    Truncated,
    /// Misplaced or malformed base64 padding.
    InvalidPadding,
    /// Data after base64 padding.
    TrailingData,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} at input offset {}", self.kind, self.offset)
    }
}
impl std::error::Error for Error {}

/// A streaming encoder or decoder.
pub trait Transform {
    /// Process `input`, writing to `output`.
    fn update(&mut self, input: &[u8], output: &mut [u8]) -> Step;
    /// Flush any tail (base64 padding, pending bytes). May return
    /// `OutputFull`; call again with more space until `Done`.
    fn finish(&mut self, output: &mut [u8]) -> Step;
    /// Forget all state and start a new stream.
    fn reset(&mut self);
    /// Upper bound on the output produced for `input_len` more input bytes
    /// plus `finish` (used by the one-shot helpers to size buffers).
    fn max_output_len(&self, input_len: usize) -> usize;
}

/// Small FIFO for output bytes that did not fit in the caller's buffer.
#[derive(Clone, Copy, Debug, Default)]
pub struct Stash {
    buf: [u8; 8],
    len: u8,
    pos: u8,
}

impl Stash {
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.pos == self.len
    }
    #[inline]
    pub fn clear(&mut self) {
        self.len = 0;
        self.pos = 0;
    }
    /// Replace contents with `bytes` (must be empty first).
    #[inline]
    pub fn set(&mut self, bytes: &[u8]) {
        debug_assert!(self.is_empty());
        debug_assert!(bytes.len() <= 8);
        self.buf[..bytes.len()].copy_from_slice(bytes);
        self.len = bytes.len() as u8;
        self.pos = 0;
    }
    /// Copy as much as fits into `out`; returns bytes written.
    #[inline]
    pub fn flush(&mut self, out: &mut [u8]) -> usize {
        let avail = (self.len - self.pos) as usize;
        let n = avail.min(out.len());
        out[..n].copy_from_slice(&self.buf[self.pos as usize..self.pos as usize + n]);
        self.pos += n as u8;
        if self.is_empty() {
            self.clear();
        }
        n
    }
}

/// Emit `unit` into `out[*o..]` directly if it fits, otherwise write the
/// prefix that fits and stash the rest. Returns `true` if everything fit.
#[inline]
pub fn emit(out: &mut [u8], o: &mut usize, unit: &[u8], stash: &mut Stash) -> bool {
    let room = out.len() - *o;
    if unit.len() <= room {
        out[*o..*o + unit.len()].copy_from_slice(unit);
        *o += unit.len();
        true
    } else {
        stash.set(unit);
        *o += stash.flush(&mut out[*o..]);
        false
    }
}

/// Drive a transform over the whole of `input` into a growable vector.
pub fn run_to_vec<T: Transform + ?Sized>(t: &mut T, input: &[u8]) -> Result<Vec<u8>, Error> {
    let mut out = vec![0u8; t.max_output_len(input.len()).max(16)];
    let mut i = 0;
    let mut o = 0;
    loop {
        let step = t.update(&input[i..], &mut out[o..]);
        i += step.consumed;
        o += step.written;
        match step.status {
            Status::Done => break,
            Status::OutputFull => out.resize(out.len() * 2, 0),
            Status::Error(e) => return Err(e),
        }
    }
    loop {
        let step = t.finish(&mut out[o..]);
        o += step.written;
        match step.status {
            Status::Done => break,
            Status::OutputFull => out.resize(out.len() * 2 + 8, 0),
            Status::Error(e) => return Err(e),
        }
    }
    out.truncate(o);
    Ok(out)
}

/// Drive a transform with explicit input chunking and output buffer size –
/// the property tests use this to hammer the resumable paths.
pub fn run_chunked<T: Transform + ?Sized>(
    t: &mut T,
    input: &[u8],
    chunks: &[usize],
    out_buf: usize,
) -> Result<Vec<u8>, Error> {
    let mut result = Vec::new();
    let mut buf = vec![0u8; out_buf.max(1)];
    let mut i = 0;
    let mut ci = 0;
    while i < input.len() {
        let n = chunks
            .get(ci)
            .copied()
            .unwrap_or(usize::MAX)
            .max(1)
            .min(input.len() - i);
        ci += 1;
        let chunk = &input[i..i + n];
        let mut c = 0;
        loop {
            let step = t.update(&chunk[c..], &mut buf);
            c += step.consumed;
            result.extend_from_slice(&buf[..step.written]);
            match step.status {
                Status::Done => break,
                Status::OutputFull => continue,
                Status::Error(e) => return Err(e),
            }
        }
        i += n;
    }
    loop {
        let step = t.finish(&mut buf);
        result.extend_from_slice(&buf[..step.written]);
        match step.status {
            Status::Done => break,
            Status::OutputFull => continue,
            Status::Error(e) => return Err(e),
        }
    }
    Ok(result)
}
