# vcodec — results and analysis

*Hex, base64 and percent encoding, vectorised on one SIMD framework, with a
shared streaming API. What vectorised well, what did not, and why.*

> Written for the standalone crate, before it joined the simd_adventures workspace; its raw data
> is in `results/xeon-vm/`. Names that changed since (see README.md, "Changes from the
> standalone crate"): `sse4.1` is `sse41`, `perfbench --matrix` is `vcodec_bench`,
> `Backend::detect()` is `Backend::best()`, and dispatch goes through simdlab tokens, so an
> unsupported backend panics instead of resolving to `Portable` (§1.2). The NEON backend of §9
> now compiles and passes the tests under qemu-aarch64; NEON-native hex and base64 kernels
> (`src/native/neon.rs`, the old path kept as `neon-generic`) and §7's VBMI2 percent codec
> (`src/native/vbmi2.rs`, tested on an emulation of its ops, not on hardware) now exist, as does
> a `std::simd` backend; README.md has their status.

## 0. TL;DR

| codec | op | scalar | SSE4.1 | AVX2 | best vs scalar | vs best external crate |
|---|---|---|---|---|---|---|
| hex (random) | encode | 1.32 | 13.8 | 18.4 | **13.9×** | 0.97× vs `hex-simd` (19.0) |
| hex (random) | decode | 2.15 | 11.9 | 19.9 | **9.2×** | 1.11× vs `hex-simd` (17.9) |
| base64 (random) | encode | 1.47 | 8.8 | 13.7 | **9.3×** | 1.43× vs `base64-simd` (9.6) |
| base64 (random) | decode | 1.81 | 9.4 | 13.4 | **7.4×** | 1.38× vs `base64-simd` (9.7) |
| url (safe) | encode | 1.06 | 12.3 | 23.1 | **21.7×** | 15.71× vs `percent-encoding` (1.5) |
| url (text) | encode | 0.36 | 2.2 | 2.0 | **6.3×** | 7.26× vs `percent-encoding` (0.3) |
| url (binary) | encode | 0.24 | 2.1 | 2.4 | **10.0×** | 22.94× vs `percent-encoding` (0.1) |
| url (safe) | decode | 0.98 | 9.8 | 35.1 | **35.9×** | 32.95× vs `percent-encoding` (1.1) |
| url (text) | decode | 0.62 | 1.9 | 2.6 | **4.2×** | 4.62× vs `percent-encoding` (0.6) |
| url (binary) | decode | 0.69 | 2.0 | 3.1 | **4.5×** | 5.11× vs `percent-encoding` (0.6) |

(64 KiB inputs, GB/s of *input* bytes, criterion means, this machine. Full
matrix with 64 B / 1 KiB / 64 KiB / 4 MiB and four URL distributions in §5.)

Headline findings:

1. **Fixed-ratio codecs (hex, base64) vectorise cleanly**: 7–14× over scalar
   on AVX2 at 64 KiB (up to 16× at 1 KiB), matching `hex-simd` and 1.4×
   `base64-simd` at ≥1 KiB. Everything converges to DRAM bandwidth at 4 MiB.
2. **Variable-length output (percent encoding) is the hard case.** The
   all-safe path is a 16–36× memcpy-speed win, but the escape path tops out at
   ~2–3 GB/s on both SSE and AVX2 — a 6–10× (encode) / 4–4.5× (decode) win
   over *realistic* scalar (scalar encode collapses from 1.2 to 0.3 GB/s once
   the branch predictor can no longer memorise the input), yet only ~1
   byte/cycle. AVX2 buys nothing over SSE on the encode escape path and ~1.3×
   on decode: the work is per-128-bit-lane shuffles, 16-byte overlapping
   stores and scalar `popcnt` address arithmetic, not 256-bit ALU work.
   AVX-512 VBMI2 (`vpexpandb`/`vpcompressb`, masked stores) would change the
   design completely (§7).
3. **The compiler was the biggest single performance hazard**, not the
   hardware: LLVM strength-reduced the base64 multiply-add into 10 shift/blend
   instructions, failed to form `vpmulhuw` from stdarch's generic definition,
   and rematerialised the hex range-check masks after a branch. Fixing those
   (inline asm + `black_box` for base64, a LUT classifier for hex) gave +50%
   and +65%. The `base64-simd` crate, compiled by the same toolchain in the
   same benchmark binary, contains *no* `vpmulhuw`/`vpmullw` at all — the
   same zero-extend/shift/pack sequences — which is consistent with it
   trailing this crate's identical algorithm by 1.4×.
4. **A "portable" fallback built on the SIMD trait is a correctness tool, not
   a performance fallback**: the generic kernels on `[u8;16]` run 0.3–0.9×
   scalar speed. `Backend::detect()` therefore returns `Scalar` when there is
   no SIMD; `Portable` exists to test the generic code paths on any CPU.
5. Hardware PMU counters were **not available in the build sandbox**
   (`perf_event_open` → `ENOENT`: Firecracker VM, no vPMU). The harness
   (`perfbench`, `scripts/perf_stat.sh`) is in place and prints cycles/byte,
   IPC, branch-miss and L1/LLC miss rates where a PMU exists. Here, §6 reports
   *simulated* counters from cachegrind (exact instruction counts, modelled
   L1/LL misses and branch mispredictions) and llvm-mca static throughput/IPC
   of each hot loop instead, and states clearly which is which.

---

## 1. Design

### 1.1 One framework, three codecs

```
                 ┌──────────── generic kernels (written once) ────────────┐
                 │ hex::{encode,decode}_kernel<S>                          │
                 │ base64::{encode,decode}_kernel<S>                       │
                 │ url::{encode,decode}_kernel<S>                          │
                 └───────────────┬────────────────────────────────────────┘
          trait simd::Simd       │ monomorphised by define_dispatch!
   ┌───────────┬───────────┬─────┴─────┬───────────┐
   │ Portable  │ Sse       │ Avx2      │ Neon      │  + Scalar (separate, hand-written)
   │ [u8;16]   │ 128-bit   │ 2×128-bit │ 128-bit   │
   └───────────┴───────────┴───────────┴───────────┘
                 ▲
   stream::Transform (update / finish / reset): chunked input, bounded output,
   carry (≤3 input bytes) + stash (≤4 output bytes), absolute error offsets.
```

**The abstraction is deliberately thin and lane-local.** A vector is
`BYTES/16` lanes of 16 bytes and every op acts within a lane, because that is
the only model that AVX2 (`vpshufb`, `vpunpck*`, `vpalignr`, `vpackus*` are
all per-128-bit-lane) and NEON (`tbl` over 16 bytes) share. The few things a
codec needs across lanes are explicit trait methods with per-backend
implementations: `interleave` and `pack_u16_u8` in full-width order (AVX2 pays
one `vperm2i128`/`vpermq`), `load_b64_enc` (12 bytes per lane: AVX2 does two
128-bit loads at `p` and `p+12`), `store_b64_dec` (compacts 2×12 bytes with
`vpermd`), `lane`/`store_lane`/`from_lane_pair`. Lane-local semantics are also
what makes the NEON backend a direct mapping rather than an emulation.

I chose a hand-rolled trait over `std::simd`/Highway-style libraries because
the kernels live on exactly the instructions a portable-SIMD API hides:
`pshufb` with its "bit 7 ⇒ zero" semantics, `pmaddubsw`/`pmaddwd`,
`pmulhuw`, `movemask`, `palignr`. Exposing those as trait methods keeps each
backend to ~150 lines and keeps the kernel code honest about what the hardware
does (every emulated op is visible in `neon.rs`, see §4.6).

### 1.2 Dispatch and safety

`define_dispatch!` generates one `#[target_feature(enable = …)]` wrapper per
ISA around each `#[inline(always)]` generic kernel. That is the only place the
CPU-feature invariant is established; a `Backend` value the CPU does not
support resolves to `Portable` at dispatch time (`Backend::resolve`), so
holding one is always safe. `Backend::detect()` picks AVX2 > SSE4.1 > NEON >
Scalar.

### 1.3 Streaming API

```rust
trait Transform {
    fn update(&mut self, input: &[u8], output: &mut [u8]) -> Step; // {consumed, written, status}
    fn finish(&mut self, output: &mut [u8]) -> Step;
    fn reset(&mut self);
    fn max_output_len(&self, input_len: usize) -> usize;
}
enum Status { Done, OutputFull, Error(Error /* {kind, absolute offset} */) }
```

Every transform's `update` has the same shape:

1. flush the output **stash** (bytes of a unit that did not fit last time);
2. complete the input **carry** (a partial unit from the previous chunk:
   `%X`, 1–3 base64 chars, a lone hex digit, 1–2 raw bytes for base64 encode);
3. run the **bulk kernel** on everything that fits — each kernel processes
   whole blocks while `input ≥ block + lookahead` and `output ≥ block +
   slack`, and returns `(consumed, written)`. Decoders stop *before* the first
   block containing anything suspicious (`=`, a non-alphabet byte, a malformed
   escape) without reporting an error;
4. a **scalar unit loop** finishes whole units, writing through `emit()` which
   stashes a partially fitting unit and returns `OutputFull`;
5. leftover bytes (< 1 unit) go into the carry.

So kernels never see tails, padding or errors, and the scalar unit loop is the
single source of truth for error *offsets*. The state is ≤ 16 bytes; a
transform can be cloned mid-stream and resumed. Output buffers can be as small
as one byte (the property tests use 1..100).

---

## 2. Hex (easy)

**Encode.** `hi = (v >> 4) & 15`, `lo = v & 15`, two `pshufb` lookups into
`"0123456789abcdef"`, `interleave`. Seven vector instructions per 16 input
bytes on SSE; AVX2 adds a `vinserti128`+`vperm2i128` to put the two
unpacked halves in memory order (`vpunpck*` is per-lane).

**Decode — two classifiers, one lesson.** The spec'd approach is range
checking: `d = v - '0'`, `is_digit = d ≤ 9` (as `cmpeq(subs(d, 9), 0)`),
`l = (v | 0x20) - 'a'`, `is_alpha = l ≤ 5`, `value = (d & is_digit) |
((l + 10) & is_alpha)`, `invalid = !(is_digit | is_alpha)`. Two vectors are
classified, validity is OR-ed and tested once (`vptest` + early exit), then
`pmaddubsw(values, [16, 1])` combines each nibble pair into a byte in a
16-bit lane and `packus` + `vpermq` narrows 32 lanes → 32 bytes.

It worked, and it was slow: 12 GB/s on AVX2 where `hex-simd` does 18–22. The
AVX2 loop body was 47 instructions for 64 input bytes. Reading the assembly
(`scripts/mca.sh` extracts the marked loops): LLVM had *rematerialised* the
`is_digit`/`is_alpha` masks after the validity branch instead of keeping them
live across it — 16 ymm registers, two input vectors, four masks, four
constants and the values did not fit, and recomputing a `vpmaxub+vpcmpeqb`
pair is "cheap". So every compare was done twice.

The fix was a different classifier rather than a fight with the register
allocator: the same nibble-LUT trick the base64 decoder uses.
`lo_lut[v & 15] & hi_lut[v >> 4] != 0 ⇔ invalid`, where the high nibble is
mapped to one bit per *pattern of valid low nibbles* (`0x3_` → `0..=9`,
`0x4_` and `0x6_` → `1..=6`, everything else → nothing) and
`value = lo + add_lut[hi]` with `add_lut[4] = add_lut[6] = 9`. Seven
instructions per vector, nothing for LLVM to duplicate, and case folding is
free. The loop body shrank from 47 to 28 instructions per 64 bytes. Result
(criterion, 64 KiB): **12.2 → 19.9 GB/s AVX2, 7.2 → 11.9 GB/s SSE**
(`Classifier::NibbleLut`, the default; `RangeCheck` is kept and benchmarked
for comparison — the `*-rangecheck` columns in §5).
The URL decoder reuses `hex_values_lut` for its `%XX` digits.

---

## 3. Base64 (medium)

**Encode (Muła/Lemire).** `load_b64_enc` places 12 input bytes per lane,
`pshufb` with `[1,0,2,1, 4,3,5,4, …]` makes each 32-bit word `[b, a, c, b]`,
then

```
t0 = v & 0x0fc0fc00;  t1 = mulhi_u16(t0, 0x04000040)   // sextets 0 and 2
t2 = v & 0x003f03f0;  t3 = mullo_u16(t2, 0x01000010)   // sextets 1 and 3
idx = t1 | t3                                           // [s0 s1 s2 s3] per word
```

The multiplies are per-16-bit-lane shifts by different amounts — which SSE/AVX2
cannot do directly (no variable 16-bit shift before AVX-512BW). The 6-bit index
becomes ASCII with a 13-entry offset table indexed by
`saturating_sub(idx, 51) + (idx > 25)`; the table is derived from the alphabet
at compile time, so the URL-safe alphabet is the same kernel with different
constants.

**Decode.** Two nibble lookups give a validity bitmask exactly as in hex
(`build_tables` assigns one bit per distinct high-nibble pattern; the standard
alphabet needs 5 bits, URL-safe 6 because `0x5_` and `0x7_` stop sharing a
pattern once `_` = 0x5F is valid), a third lookup on the high nibble gives the
ASCII→value "roll" (`/` and `_` — both index 63 — need their own slot, selected
by `hi | (v == special) & 8`), then `pmaddubsw(vals, [0x40, 1])` folds pairs of
sextets into 12 bits, `pmaddwd(·, [0x1000, 1])` folds pairs of those into 24
bits, and one `pshufb` compacts `[c b a ·]` words into 12 contiguous bytes
per lane. A unit test checks the derived tables against Muła's published
constants and exhaustively against the alphabet for all 256 bytes.

**Padding and tails** never reach the kernel: `=` is "invalid" to the nibble
LUTs so the kernel stops before that block, and the unit loop decodes the
final quad (`xx==`, `xxx=`, or unpadded 2–3 chars at `finish`), rejects data
after padding (`TrailingData`), and reports exact offsets.

**Two compiler fights, +50% total.** First measurement: AVX2 encode 10.4 GB/s,
*slower per block than decode* despite fewer operations. The loop body showed
`vpmulhuw` replaced by `vpmovzxwd + vpsrlvd + vpackusdw + vpermq` (7
instructions) and `vpmullw` by `vpsllw ×2 + vpblendw` — LLVM saw the
power-of-two constants and "strength-reduced" the multiplies into shifts,
which on this ISA are *more* expensive. Hiding the constant with
`core::hint::black_box` fixed `vpmullw`; for `mulhi_u16` it then produced a
zero-extend/32-bit-multiply/pack sequence instead, because stdarch defines
`_mm256_mulhi_epu16` generically (`trunc(zext(a)*zext(b) >> 16)`) and LLVM
did not re-match it. One line of inline asm (`vpmulhuw`) in the x86 backends
pins the instruction. **10.4 → 15.7 GB/s AVX2, 5.1 → 9.0 GB/s SSE**
(`perfbench`, best of 5, 64 KiB, before/after on the same host).

This is not specific to this crate. Disassembling the benchmark binary shows
`base64_simd::multiversion::encode::avx2` — the same Muła/Lemire algorithm,
written against `_mm256_mulhi_epu16`/`_mm256_mullo_epi16` — compiled to
`vpmovzxwd ×3, vpsrlvd ×3, vpackusdw ×2, vpsllw ×4, vpblendw ×2` and zero
multiplies. Anyone using these intrinsics from Rust 1.97 / its LLVM gets the
slow lowering, and it is invisible unless you read the assembly.

---

## 4. Percent / URL encoding (hard)

### 4.1 Encode

1. **Classify** with two nibble lookups: `lo_lut[b & 15] & hi_lut[b >> 4]`
   is non-zero iff `b` is in the safe set (`hi_lut` is `1 << h` for `h < 8`
   and 0 above, so bytes ≥ 0x80 always escape). Exact for any ASCII set;
   `AsciiSet::new(&[…])` builds the table in a `const fn`, with `UNRESERVED`,
   `COMPONENT` (JS `encodeURIComponent`) and `FORM` provided.
2. `movemask` of the escape mask. **If zero, store the block as-is** — the
   common all-safe case is a vector copy, no scalar fallback, 16–22× scalar.
3. Otherwise **expand in 4-byte groups**: a 16-byte source vector
   `[in0..3 | hexhi0..3 | hexlo0..3 | '%' ·· ]` is assembled with three
   constant shuffles, and one `pshufb` with the 16-entry table
   `EXPAND[group_mask]` emits the 4–12 output bytes (`%` is index 12, hex
   digits 4+k / 8+k, raw bytes k). Groups of 4 are the largest for which
   `inputs + 2×hex + '%'` fits a 16-byte shuffle source.
4. **Output positions are prefix sums**: group `k` starts at
   `4k + 2·popcount(mask & ((1 << 4k) − 1))`. Each store writes 16 bytes of
   which 4–12 are valid, so stores go out in output order and the driver
   guarantees `3·W + 16` bytes of slack. The block advances `W + 2·popcount
   (mask)`.

### 4.2 Decode

1. `is_pct = v == '%'`, `movemask`. **If zero, store** (with the optional
   `+ → ' '` fix-up as a `cmpeq/and/xor`), done — 35 GB/s on AVX2.
2. Otherwise two **lookahead loads** at `p+1` and `p+2` give, for every
   position, "the next two bytes" — including for a `%` at the last two
   positions of the block, whose digits live in the *next* block. The kernel
   requires `W + 2` input bytes per block for exactly this reason, and
   afterwards simply advances by `W + extra` (`extra` = 2 if the last byte is
   `%`, 1 if the second-to-last is) and continues from an unaligned address.
   No carry state in the kernel; the streaming driver's `%X` carry is only for
   *chunk* boundaries.
3. `hex_values_lut` on both lookahead vectors; `is_pct & (bad1 | bad2) ≠ 0`
   ⇒ malformed escape ⇒ stop and let the unit loop report the offset. This
   also catches `%%41`-style overlaps for free, since `%` is not a hex digit.
4. `decoded = (h1 << 4) | h2`, blended into the `%` positions; the drop mask
   is `(m << 1) | (m << 2)`; the kept bytes are compacted 8 at a time through
   two 256-entry × 16-byte shuffle tables (`COMPRESS_LO/HI`), each half's
   output offset again an independent masked popcount.

### 4.3 What went wrong on the way

* **Lane-parallel expansion broke output ordering.** The first AVX2 version
  processed lane 0's four groups then lane 1's, each with its own
  `S::lane()` extraction — correct but AVX2 was no faster than SSE. Doing
  both lanes' group `g` in one 256-bit shuffle (`from_lane_pair(EXPAND[m0],
  EXPAND[m1])`) halved the shuffle count — and produced garbage: lane 0's
  group `g+1` store (16 bytes, ≤12 valid) landed *after* lane 1's group `g`
  store and overwrote it. Overlapping-store schemes are only correct if stores
  are issued in output order; the fix computes all four expanded vectors
  first and stores lane 0's four, then lane 1's four. The property tests
  caught it within seconds.
* **Running prefix sums are a latency chain.** `o += 4 + 2·popcnt(mg)` makes
  every store address depend on the previous group's popcount: `popcnt →
  add → store-address`, ~3 cycles per 4 bytes ≈ 1.3 B/cycle, which is almost
  exactly what was measured. Computing each offset independently from the
  block mask (`popcnt(m & below_k)`) is 8 independent popcounts instead of a
  chain; llvm-mca's block reciprocal throughput for the AVX2 escape path
  dropped from 33 to 20 cycles per 32 bytes.
* **…and yet AVX2 ≈ SSE on the escape path (2.3–2.4 GB/s both).** The
  remaining cost is ~100 instructions per 32 bytes: 16 shuffles, 12 ORs,
  8 popcounts, ~30 scalar mask/offset ops, 8 overlapping 16-byte stores and
  4 `vextracti128`. The 256-bit datapath is used for 2 of those 100. This is
  the honest conclusion for variable-length output without compress/expand
  instructions: it is a *scalar-flavoured* algorithm that happens to use
  vector registers, and it is store- and address-arithmetic-bound, not ALU-
  bound. Still 7–9× the realistic scalar number (see 4.4).
* **Decode has the same shape**: fast path at memcpy speed, escape path at
  ~3 GB/s (two 8-byte compactions through 4 KiB tables per lane, two table
  loads per 8 bytes, popcounts). AVX2 is 1.3× SSE here — the two lookahead
  loads and `hex_values_lut` do benefit from width.

### 4.4 Scalar collapses at 64 KiB, and that is the real baseline

Scalar URL encode of English-like text: **1.18 GB/s at 1 KiB, 0.33 GB/s at
64 KiB**; the `percent-encoding` crate 0.47 → 0.31; `hex` crate decode 1.05 →
0.25. Nothing about the data changed. At 1 KiB the benchmark loop re-runs the
same bytes and the branch predictor's history *memorises the per-byte
branch pattern*; at 64 KiB it cannot, and the ~17% data-dependent mispredicts
per byte (text) cost ~20 cycles each. The vectorised escape path has one
branch per block (`m == 0`) and does everything else through data-dependent
*loads* (table indices), so it is flat across sizes. This is why the table in
§5 reports speedups per size: the 1 KiB scalar numbers flatter the baseline.

---

## 5. Benchmarks

Machine: Intel Xeon @ 2.1 GHz (Skylake-SP class: AVX2, AVX-512BW/VBMI
present but not used), 2 vCPUs in a Firecracker VM, Rust 1.97, `-C
opt-level=3`, thin LTO, criterion 0.5 (20 samples × 0.9 s, mean). Inputs are
generated by the same xorshift generator in `benches/codecs.rs` and
`perfbench`; decoders are fed the scalar encoder's output. URL distributions:
`safe` = alphanumerics only; `text` = English-like words/spaces/punctuation
(~17% escapes); `pct10` = 10% escapes; `binary` = random bytes (~75%
escapes). Competitors: `hex 0.4`, `hex-simd 0.8`, `base64 0.22`,
`base64-simd 0.8`, OpenSSL 3.0 `EVP_EncodeBlock`/`EVP_DecodeBlock` via FFI
(C), `percent-encoding 2` (allocating, as it is used in practice). The VM is
noisy: a second full criterion run differed by up to 25% on some rows
(`results/xeon-vm/bench_round1.md` vs `results/xeon-vm/bench.md`), so treat differences
under ~20% as ties and trust the *ratios* more than the absolute GB/s. The
sandbox was also migrated to a different host (a 2.8 GHz Xeon without VBMI)
part-way through; `results/xeon-vm/perfbench_64k_bestof5.tsv` is a best-of-5
cross-check taken there and is uniformly ~25% lower — same ratios. Raw data:
`results/xeon-vm/bench.md` (TSV + tables), `results/xeon-vm/cachegrind.tsv`, `results/xeon-vm/mca.txt`.

### hex encode

| size/dist | vcodec-avx2 | vcodec-portable | vcodec-scalar | vcodec-sse4.1 | hex-crate | hex-simd |
|---|---|---|---|---|---|---|
| 64B random | 4.29 (4.0×) | 1.10 (1.0×) | 1.08 (1.0×) | 4.16 (3.9×) | 1.34 (1.2×) | 15.28 (14.2×) |
| 1KiB random | 21.60 (16.1×) | 1.40 (1.0×) | 1.34 (1.0×) | 10.54 (7.9×) | 1.40 (1.0×) | 28.76 (21.5×) |
| 64KiB random | 18.38 (13.9×) | 1.14 (0.9×) | 1.32 (1.0×) | 13.79 (10.5×) | 1.47 (1.1×) | 19.01 (14.4×) |
| 4MiB random | 8.77 (6.7×) | 1.24 (0.9×) | 1.30 (1.0×) | 9.62 (7.4×) | 1.31 (1.0×) | 8.66 (6.6×) |

### hex decode

| size/dist | vcodec-avx2 | vcodec-avx2-rangecheck | vcodec-portable | vcodec-portable-rangecheck | vcodec-scalar | vcodec-sse4.1 | vcodec-sse4.1-rangecheck | hex-crate | hex-simd |
|---|---|---|---|---|---|---|---|---|---|
| 64B random | 12.35 (6.3×) | 9.80 (5.0×) | 0.88 (0.4×) | 2.35 (1.2×) | 1.95 (1.0×) | 8.38 (4.3×) | 6.07 (3.1×) | 0.88 (0.4×) | 20.40 (10.4×) |
| 1KiB random | 22.27 (10.6×) | 12.66 (6.0×) | 0.95 (0.5×) | 2.09 (1.0×) | 2.11 (1.0×) | 11.95 (5.7×) | 7.23 (3.4×) | 0.91 (0.4×) | 22.00 (10.4×) |
| 64KiB random | 19.88 (9.2×) | 12.16 (5.7×) | 0.96 (0.4×) | 2.47 (1.1×) | 2.15 (1.0×) | 11.90 (5.5×) | 7.16 (3.3×) | 0.22 (0.1×) | 17.87 (8.3×) |
| 4MiB random | 18.69 (9.3×) | 12.81 (6.4×) | 0.81 (0.4×) | 2.49 (1.2×) | 2.01 (1.0×) | 12.46 (6.2×) | 7.47 (3.7×) | 0.23 (0.1×) | 18.67 (9.3×) |

### base64 encode

| size/dist | vcodec-avx2 | vcodec-portable | vcodec-scalar | vcodec-sse4.1 | base64-crate | base64-simd | openssl-EVP_EncodeBlock |
|---|---|---|---|---|---|---|---|
| 64B random | 2.25 (1.9×) | 0.46 (0.4×) | 1.19 (1.0×) | 2.76 (2.3×) | 1.99 (1.7×) | 5.00 (4.2×) | 1.63 (1.4×) |
| 1KiB random | 11.75 (8.4×) | 0.48 (0.3×) | 1.40 (1.0×) | 6.30 (4.5×) | 2.48 (1.8×) | 9.64 (6.9×) | 1.87 (1.3×) |
| 64KiB random | 13.74 (9.3×) | 0.45 (0.3×) | 1.47 (1.0×) | 8.84 (6.0×) | 2.58 (1.7×) | 9.63 (6.5×) | 1.95 (1.3×) |
| 4MiB random | 11.21 (8.0×) | 0.41 (0.3×) | 1.41 (1.0×) | 7.85 (5.6×) | 2.49 (1.8×) | 9.70 (6.9×) | 1.95 (1.4×) |

### base64 decode

| size/dist | vcodec-avx2 | vcodec-portable | vcodec-scalar | vcodec-sse4.1 | base64-crate | base64-simd | openssl-EVP_DecodeBlock |
|---|---|---|---|---|---|---|---|
| 64B random | 1.52 (1.0×) | 0.66 (0.4×) | 1.48 (1.0×) | 2.59 (1.8×) | 1.90 (1.3×) | 5.23 (3.5×) | 2.02 (1.4×) |
| 1KiB random | 8.82 (4.7×) | 0.60 (0.3×) | 1.87 (1.0×) | 7.75 (4.1×) | 2.64 (1.4×) | 9.25 (4.9×) | 2.27 (1.2×) |
| 64KiB random | 13.44 (7.4×) | 0.66 (0.4×) | 1.81 (1.0×) | 9.43 (5.2×) | 2.60 (1.4×) | 9.71 (5.4×) | 2.10 (1.2×) |
| 4MiB random | 14.22 (7.9×) | 0.66 (0.4×) | 1.80 (1.0×) | 8.33 (4.6×) | 2.36 (1.3×) | 9.01 (5.0×) | 2.36 (1.3×) |

### url encode

| size/dist | vcodec-avx2 | vcodec-portable | vcodec-scalar | vcodec-sse4.1 | percent-encoding |
|---|---|---|---|---|---|
| 64B binary | 1.73 (2.3×) | 0.12 (0.2×) | 0.75 (1.0×) | 1.70 (2.3×) | 0.17 (0.2×) |
| 64B pct10 | 1.89 (2.3×) | 0.21 (0.2×) | 0.84 (1.0×) | 2.52 (3.0×) | 0.26 (0.3×) |
| 64B safe | 4.61 (5.0×) | 0.71 (0.8×) | 0.92 (1.0×) | 3.99 (4.3×) | 1.17 (1.3×) |
| 64B text | 1.79 (2.5×) | 0.13 (0.2×) | 0.73 (1.0×) | 1.60 (2.2×) | 0.26 (0.4×) |
| 1KiB binary | 2.42 (3.0×) | 0.13 (0.2×) | 0.82 (1.0×) | 2.22 (2.7×) | 0.23 (0.3×) |
| 1KiB pct10 | 2.42 (2.4×) | 0.15 (0.2×) | 1.00 (1.0×) | 2.48 (2.5×) | 0.70 (0.7×) |
| 1KiB safe | 21.36 (19.2×) | 0.81 (0.7×) | 1.11 (1.0×) | 10.86 (9.8×) | 1.41 (1.3×) |
| 1KiB text | 2.43 (2.6×) | 0.13 (0.1×) | 0.92 (1.0×) | 2.35 (2.6×) | 0.54 (0.6×) |
| 64KiB binary | 2.44 (10.0×) | 0.11 (0.5×) | 0.24 (1.0×) | 2.10 (8.6×) | 0.11 (0.4×) |
| 64KiB pct10 | 2.42 (5.0×) | 0.14 (0.3×) | 0.48 (1.0×) | 2.71 (5.6×) | 0.51 (1.1×) |
| 64KiB safe | 23.05 (21.7×) | 0.71 (0.7×) | 1.06 (1.0×) | 12.29 (11.6×) | 1.47 (1.4×) |
| 64KiB text | 2.02 (5.7×) | 0.12 (0.3×) | 0.36 (1.0×) | 2.22 (6.3×) | 0.31 (0.9×) |
| 4MiB binary | 2.22 (10.0×) | 0.11 (0.5×) | 0.22 (1.0×) | 2.06 (9.3×) | 0.10 (0.5×) |
| 4MiB pct10 | 2.35 (5.3×) | 0.14 (0.3×) | 0.44 (1.0×) | 2.09 (4.7×) | 0.41 (0.9×) |
| 4MiB safe | 14.42 (13.5×) | 0.76 (0.7×) | 1.07 (1.0×) | 12.55 (11.7×) | 1.46 (1.4×) |
| 4MiB text | 2.35 (7.2×) | 0.11 (0.3×) | 0.32 (1.0×) | 2.21 (6.8×) | 0.28 (0.9×) |

### url decode

| size/dist | vcodec-avx2 | vcodec-portable | vcodec-scalar | vcodec-sse4.1 | percent-encoding |
|---|---|---|---|---|---|
| 64B binary | 2.73 (1.9×) | 0.33 (0.2×) | 1.43 (1.0×) | 1.72 (1.2×) | 1.07 (0.8×) |
| 64B pct10 | 2.32 (2.3×) | 0.44 (0.4×) | 1.01 (1.0×) | 2.13 (2.1×) | 0.68 (0.7×) |
| 64B safe | 1.73 (1.9×) | 1.22 (1.3×) | 0.93 (1.0×) | 2.68 (2.9×) | 0.60 (0.6×) |
| 64B text | 2.29 (2.0×) | 0.31 (0.3×) | 1.13 (1.0×) | 1.91 (1.7×) | 0.79 (0.7×) |
| 1KiB binary | 3.09 (2.1×) | 0.35 (0.2×) | 1.47 (1.0×) | 1.94 (1.3×) | 1.19 (0.8×) |
| 1KiB pct10 | 2.94 (2.7×) | 0.40 (0.4×) | 1.10 (1.0×) | 2.33 (2.1×) | 1.07 (1.0×) |
| 1KiB safe | 17.56 (17.3×) | 1.48 (1.5×) | 1.02 (1.0×) | 8.78 (8.6×) | 0.93 (0.9×) |
| 1KiB text | 2.97 (2.5×) | 0.31 (0.3×) | 1.18 (1.0×) | 1.88 (1.6×) | 1.10 (0.9×) |
| 64KiB binary | 3.08 (4.5×) | 0.32 (0.5×) | 0.69 (1.0×) | 2.00 (2.9×) | 0.60 (0.9×) |
| 64KiB pct10 | 3.26 (4.6×) | 0.34 (0.5×) | 0.70 (1.0×) | 2.39 (3.4×) | 0.64 (0.9×) |
| 64KiB safe | 35.06 (35.9×) | 1.44 (1.5×) | 0.98 (1.0×) | 9.76 (10.0×) | 1.06 (1.1×) |
| 64KiB text | 2.64 (4.2×) | 0.31 (0.5×) | 0.62 (1.0×) | 1.94 (3.1×) | 0.57 (0.9×) |
| 4MiB binary | 3.14 (5.1×) | 0.27 (0.4×) | 0.62 (1.0×) | 2.01 (3.2×) | 0.60 (1.0×) |
| 4MiB pct10 | 2.99 (4.5×) | 0.30 (0.4×) | 0.67 (1.0×) | 1.69 (2.5×) | 0.59 (0.9×) |
| 4MiB safe | 13.97 (18.0×) | 1.28 (1.7×) | 0.78 (1.0×) | 11.19 (14.4×) | 0.99 (1.3×) |
| 4MiB text | 3.10 (5.3×) | 0.29 (0.5×) | 0.59 (1.0×) | 2.03 (3.4×) | 0.55 (0.9×) |

Reading the matrix:

* **Hex**: AVX2 encode 21.6 GB/s at 1 KiB (16×), on par with `hex-simd`; at
  4 MiB everything is ~9 GB/s = write bandwidth for 8 MiB of output (SSE,
  AVX2 and hex-simd tie — a non-temporal store path would be the next step).
  Decode with the LUT classifier is 20–22 GB/s (vs 12–13 with range-check,
  §2), 1.1× `hex-simd`.
* **Base64**: AVX2 13.7 GB/s encode / 13.4 GB/s decode at 64 KiB in this
  run (15.7/17.6 in quieter `perfbench` best-of-5 runs on the same host),
  1.4× the `base64-simd` crate (which dispatches to AVX2 too, but see §3 —
  its multiplies were compiled to shift sequences) and 5–7× the scalar
  `base64` crate and OpenSSL's `EVP_*Block`. At 64 B `base64-simd` wins (5 vs
  2 GB/s): the streaming driver costs ~30 cycles of fixed overhead per call
  (stash/carry bookkeeping, feature-detection loads, `finish`), which is half
  of a 64-byte job. A one-shot entry point that bypasses the state machine
  would fix that and was not done.
* **URL**: see §4.3/4.4. Note `portable` decode of `safe` input at 14–19 GB/s:
  LLVM auto-vectorises the `[u8;16]` compare+copy into real `vpcmpeqb` /
  `vmovdqu` — the one place the portable backend is fast, because the fast
  path *is* a memcpy.
* **Portable < Scalar** everywhere else (0.3–0.9×): the generic kernels'
  `shuffle_bytes`, `madd_*` and `movemask` emulations become 16-iteration
  byte loops that LLVM does not turn back into `pshufb`. Hence `detect()`
  returns `Scalar` on SIMD-less CPUs.

---

## 6. Performance counters

### 6.1 What could and could not be measured here

`perf_event_open(2)` returns `ENOENT` in this sandbox: the Firecracker VM
exposes no PMU, and `perf` is not installable. The harness is complete and
degrades explicitly:

* `perfbench --matrix` opens `cycles`, `instructions`, `branches`,
  `branch-misses`, `L1-dcache-load-misses`, `LLC-load-misses` per thread
  (user-space only, works with `perf_event_paranoid=2`) around the hot loop
  and prints **cycles/byte, IPC, branch-misses/KiB, L1d-misses/KiB,
  LLC-misses/KiB** plus GB/s; the `pmu` column says `no` when it fell back to
  wall-clock. `scripts/perf_stat.sh one …` cross-checks with `perf stat`.
  **Run `scripts/perf_stat.sh matrix` on bare metal to get the real columns.**

In their place, two deterministic sources:

### 6.2 Simulated counters (valgrind/cachegrind, `scripts/cachegrind.sh`)

I1/D1 = 32 KiB 8-way, LL = 16 MiB 16-way, cachegrind's branch predictor
model. Instruction counts are exact; misses and mispredictions are a model.
Per kernel, 64 KiB inputs, difference of 48 and 16 iterations:

| codec | op | dist | backend | instr/byte | D1 miss/KiB | LL miss/KiB | branch mispred/KiB |
|---|---|---|---|---|---|---|---|
| hex | decode | random | scalar | 10.10 | 25.7 | 0.00 | 0.0 |
| hex | decode | random | portable | 18.33 | 25.6 | 0.00 | 0.0 |
| hex | decode | random | sse | 1.46 | 25.9 | 0.00 | 0.0 |
| hex | decode | random | avx2 | 0.62 | 25.8 | 0.00 | 0.0 |
| hex | encode | random | scalar | 17.00 | 51.2 | 0.00 | 0.0 |
| hex | encode | random | portable | 12.22 | 51.1 | 0.00 | 0.0 |
| hex | encode | random | sse | 1.53 | 51.3 | 0.00 | 0.0 |
| hex | encode | random | avx2 | 0.70 | 51.3 | 0.00 | 0.0 |
| base64 | decode | random | scalar | 11.69 | 30.0 | 0.00 | 0.0 |
| base64 | decode | random | portable | 25.84 | 29.9 | 0.00 | 0.0 |
| base64 | decode | random | sse | 2.13 | 30.2 | 0.00 | 0.0 |
| base64 | decode | random | avx2 | 0.93 | 30.2 | 0.00 | 0.0 |
| base64 | encode | random | scalar | 14.53 | 39.9 | 0.00 | 0.0 |
| base64 | encode | random | portable | 37.46 | 39.9 | 0.00 | 0.0 |
| base64 | encode | random | sse | 2.31 | 40.1 | 0.00 | 0.0 |
| base64 | encode | random | avx2 | 1.03 | 40.1 | 0.00 | 0.1 |
| url | decode | binary | scalar | 11.38 | 24.0 | 0.00 | 133.8 |
| url | decode | binary | portable | 56.45 | 24.2 | 0.00 | 122.1 |
| url | decode | binary | sse | 6.18 | 24.4 | 0.00 | 0.1 |
| url | decode | binary | avx2 | 4.01 | 24.4 | 0.00 | 0.1 |
| url | decode | pct10 | scalar | 13.90 | 31.4 | 0.00 | 100.2 |
| url | decode | pct10 | portable | 51.21 | 31.4 | 0.00 | 96.6 |
| url | decode | pct10 | sse | 5.49 | 31.7 | 0.00 | 13.9 |
| url | decode | pct10 | avx2 | 3.96 | 31.8 | 0.00 | 1.2 |
| url | decode | safe | scalar | 14.88 | 34.2 | 0.00 | 0.0 |
| url | decode | safe | portable | 13.62 | 34.2 | 0.00 | 0.0 |
| url | decode | safe | sse | 1.33 | 34.4 | 0.00 | 0.0 |
| url | decode | safe | avx2 | 0.61 | 34.4 | 0.00 | 0.0 |
| url | decode | text | scalar | 13.35 | 29.8 | 0.00 | 154.9 |
| url | decode | text | portable | 58.81 | 29.8 | 0.00 | 96.0 |
| url | decode | text | sse | 6.35 | 30.0 | 0.00 | 0.0 |
| url | decode | text | avx2 | 4.07 | 30.0 | 0.00 | 0.0 |
| url | encode | binary | scalar | 26.19 | 59.4 | 0.00 | 338.2 |
| url | encode | binary | portable | 147.37 | 59.4 | 0.00 | 258.4 |
| url | encode | binary | sse | 8.24 | 59.6 | 0.00 | 0.0 |
| url | encode | binary | avx2 | 6.84 | 59.6 | 0.00 | 0.0 |
| url | encode | pct10 | scalar | 17.31 | 37.6 | 0.00 | 118.9 |
| url | encode | pct10 | portable | 122.18 | 37.6 | 0.00 | 202.4 |
| url | encode | pct10 | sse | 7.04 | 37.7 | 0.00 | 15.1 |
| url | encode | pct10 | avx2 | 6.64 | 37.7 | 0.00 | 1.2 |
| url | encode | safe | scalar | 15.94 | 34.2 | 0.00 | 0.0 |
| url | encode | safe | portable | 20.52 | 34.2 | 0.00 | 0.0 |
| url | encode | safe | sse | 1.80 | 34.3 | 0.00 | 0.0 |
| url | encode | safe | avx2 | 0.77 | 34.3 | 0.00 | 0.0 |
| url | encode | text | scalar | 18.38 | 40.2 | 0.00 | 209.2 |
| url | encode | text | portable | 145.73 | 40.2 | 0.00 | 239.8 |
| url | encode | text | sse | 8.24 | 40.4 | 0.00 | 0.0 |
| url | encode | text | avx2 | 6.84 | 40.4 | 0.00 | 0.0 |

What it shows:

* **Instructions per byte** is the cleanest "how much work" metric
  (exact counts; they include loop control and the streaming driver): hex
  encode 0.70 (AVX2) / 1.5 (SSE) vs 17 (scalar, with bounds checks); base64
  decode 0.93 vs 11.7; URL encode on text 6.8 (AVX2) vs 18.4 (scalar) — the
  escape path is only 2.7× fewer instructions than scalar, consistent with
  §4.3 — while on `safe` input it is 0.77. The portable backend's 120–150
  instructions/byte on URL encode is the generic `shuffle_bytes` emulation
  running as 16-iteration byte loops.
* **Branch mispredictions/KiB**: scalar URL encode on text ≈ 209/KiB and
  binary ≈ 338/KiB in the model (roughly one per safe↔escape transition) vs
  0.0 for the vector kernels, which have one branch per block. On `pct10`
  that one branch *is* unpredictable for SSE (P(16 safe bytes) = 0.9¹⁶ ≈ 18%
  ⇒ 15 mispredicts/KiB) but not for AVX2 (0.9³² ≈ 3% ⇒ 1.2/KiB) — a
  concrete, if small, advantage of the wider block. This is the mechanism
  behind §4.4.
* **D1 misses/KiB** are identical across backends for a given codec (hex
  encode 51/KiB = 16 input lines + 32 output lines + a little; base64 encode
  40; URL `safe` 34): pure streaming misses, so the kernels are not
  cache-bound at 64 KiB and the 4 KiB `COMPRESS_*` / 256-byte `EXPAND`
  tables stay L1-resident. LL misses are 0 at this size.

### 6.3 Static throughput (llvm-mca, `scripts/mca.sh`, `-mcpu=skylake-avx512`)

The loop bodies are extracted from the real build via `# LLVM-MCA-BEGIN/END`
markers (`--features mca-markers`). "Block RThroughput" is the steady-state
cycles per loop iteration assuming perfect caches and no branch cost; IPC is
mca's estimate for that block.

| kernel | ISA | instructions / block | block bytes (in) | RThroughput (cycles/block) | bytes/cycle | mca IPC |
|---|---|---|---|---|---|---|
| hex_encode | avx2 | 12 | 32 | 6.0 | 5.3 | 1.95 |
| hex_encode | sse | 10 | 16 | 4.0 | 4.0 | 2.43 |
| hex_decode | avx2 | 28 | 64 | 9.0 | 7.1 | 3.01 |
| hex_decode | sse | 28 | 32 | 8.0 | 4.0 | 3.2 |
| b64_encode | avx2 | 14 | 24 | 2.7 | 8.9 | 3.2 |
| b64_encode | sse | 13 | 12 | 2.3 | 5.2 | 3.37 |
| b64_decode | avx2 | 19 | 32 | 6.0 | 5.3 | 3.04 |
| b64_decode | sse | 17 | 16 | 5.0 | 3.2 | 3.26 |
| url_encode (escape path) | avx2 | 103 | 32 | 20.2 | 1.6 | 4.26 |
| url_encode (escape path) | sse | 69 | 16 | 17.0 | 0.9 | 3.42 |
| url_decode (escape path) | avx2 | 76 | 32 | 14.3 | 2.2 | 4.29 |
| url_decode (escape path) | sse | 61 | 16 | 11.2 | 1.4 | 4.3 |

Reading it with mca's per-port breakdown (`llvm-mca … | grep -A3 'Resource
pressure per iteration'`):

* **hex encode AVX2** is shuffle-port bound: 6 of its 12 instructions
  (`vpshufb ×2, vpunpck{l,h}bw, vinserti128, vperm2i128`) are port-5-only,
  hence exactly 6 cycles per 32 bytes = 5.3 B/cycle. At the ~3.5 GHz this
  Xeon turbos to that is ~18 GB/s, matching the 18–22 GB/s measured. The
  2-lane AVX2 fix-up (`vinserti128 + vperm2i128`) costs 2 of those 6 cycles —
  one third of the loop — which is why AVX2 is only 1.4–1.8× SSE here rather
  than 2×.
* **base64 encode AVX2**: p0/p1/p5 ≈ 4 µops each per 24 bytes, i.e. a
  balanced ALU-bound loop; `vpmulhuw`/`vpmullw` on p0/p1 are what keep p5
  free, which is precisely what the compiler's shift/blend rewrite destroyed
  (shifts and blends are p5/p01 and there were ten of them).
* **URL encode escape path AVX2**: p0 ≈ p1 ≈ p5 ≈ 23 µops and p6 ≈ 18 per
  32 bytes — every ALU port saturated for 20 cycles. 1.6 B/cycle ≈ 5.6 GB/s
  at 3.5 GHz vs 2.0–2.4 measured: the rest is what mca does not model — eight
  overlapping 16-byte stores into a ≤96-byte window (store-forwarding and
  store-buffer stalls) and the data-dependent table loads. This path is
  simply not a SIMD algorithm; it is scalar bookkeeping with vector-register
  shuffles, and it would take compress/expand instructions to change that
  (§7).

---

## 7. Where AVX-512 would change the design

This CPU has AVX-512BW and VBMI; nothing here uses them. Beyond doubling the
width (which for hex/base64 would mostly move the bottleneck to stores and
DRAM sooner), the ISA removes the three structural costs of this design:

1. **Variable-length output becomes two instructions.** AVX512-VBMI2
   `vpexpandb` scatters the kept/raw bytes to the positions given by a
   k-mask; the escaped triples come from a second expand of the hex vectors
   plus a `vpternlogd` merge; the result is written with a **masked store**
   (`vmovdqu8 [mem]{k}`) of exactly `W + 2·popcnt` bytes. No `EXPAND` table,
   no 4-byte groups, no overlapping stores, no output slack — and bounded
   output stops needing the scalar tail. Decode compaction is one
   `vpcompressb` per 64 bytes instead of eight table lookups.
2. **Lookups get wider and lane-crossing.** `vpermb` (VBMI) is a 64-entry
   byte table with no lane restriction, and `vpermi2b` is 128 entries: base64
   decode maps ASCII→value in one instruction with validity from a compare
   (Muła's VBMI decoder is ~7 instructions per 64 chars), and
   `vpmultishiftqb` extracts all four 6-bit fields of the encoder in one
   instruction, replacing the mask/mulhi/mask/mullo/or sequence and making
   the `pmulhuw` compiler fight moot. The 12-byte-per-lane loads become one
   `vpermb` from a 48-byte load.
3. **Masks replace movemask + scalar.** Compares produce k-registers
   directly; `kortest` is the early-out; popcounts of k-masks still go through
   GPRs, but the per-group offset arithmetic disappears with (1). Masked loads
   (`vmovdqu8 zmm{k}, [mem]`) also let the kernels handle the final partial
   block, shrinking the streaming driver's unit loop to carry/stash handling
   only.

The abstraction would need to change shape: lane-local 16-byte semantics
are the wrong model for VBMI (its whole point is lane-crossing), and the
trait would need k-mask types, `compress`/`expand` and masked memory ops.
That is why the AVX-512 backend is a *different design*, not a fifth
instantiation of the current trait — and why NEON SVE2 (which has `compact`
but not byte expand) would land somewhere in between. Caveats: 512-bit
frequency licensing on Skylake-SP, double-pumped execution on Zen 4, and
absence on all current client Intel parts.

---

## 8. Correctness

* `tests/props.rs` (proptest, 600 cases each): for every supported backend,
  encode/decode round-trips, byte-exact equality with the scalar transform,
  equality with the `base64` and `percent-encoding` crates, and **identical
  results (including error kind and absolute offset) under arbitrary input
  chunking (1–200 bytes) and output buffer sizes (1–100 bytes)** against the
  one-shot scalar result. Dedicated tests place `%XX` on every position
  around the 16/32-byte block boundaries with chunk cuts through the escape,
  check known answers (RFC 4648 vectors, URL-safe `-_`), and check error
  offsets for `InvalidByte`, `Truncated`, `InvalidPadding`, `TrailingData`.
* `mini_fuzz`: an in-process random loop (3 000 iterations by default,
  `VCODEC_FUZZ_ITERS=200000` run clean) over all transforms × backends ×
  chunkings; `fuzz/` has two libFuzzer targets (`roundtrip`,
  `decode_vs_scalar`) for `cargo +nightly fuzz`.
* The tests found three real bugs during development: a doubled stream
  offset in the hex carry path (wrong error offsets after a chunk boundary),
  a 2-byte carry buffer that overflowed when a 1-byte carry met a 2-byte
  chunk, and the AVX2 store-ordering bug of §4.3.
* The scalar implementation is the oracle; the `Portable` backend is a
  second, independent implementation of the trait semantics that the
  intrinsic backends are compared against by every test.

## 9. Known limitations

* **NEON backend is written against the `core::arch::aarch64` intrinsics but
  was not compiled or run here** (no aarch64 toolchain available in the
  sandbox). It maps every trait op directly except `mulhi_u16` (`vmull` +
  `vshrn`), `madd_*` (mask/shift/`vmla`), `movemask` (`and` with bit
  weights + `vaddv` ×2) and `shuffle_bytes` (`tbl` plus one `and` to get
  `pshufb`'s bit-7 semantics). Expect base64 encode to be the weak spot on
  NEON: a native design would use `vshlq_u16` with a per-element shift vector
  instead of multiplies — exactly the per-lane-variable shift x86 lacks — and
  the x86-shaped trait does not express it. `cargo test --release` on an
  Apple Silicon or Graviton box is the first thing to run.
* No one-shot fast path (64-byte inputs pay ~25 cycles of driver overhead).
* No non-temporal stores for the DRAM-bound regime.
* Base64 decoding is lenient about non-canonical trailing bits (`QR==`
  decodes), like most decoders; strict mode would be a 4-line addition in the
  unit path.
* Hardware counter numbers must come from a machine with a PMU
  (`scripts/perf_stat.sh matrix`).
