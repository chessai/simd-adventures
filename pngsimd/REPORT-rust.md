# The Rust port: same algorithms, a safe API, and what LLVM does differently from GCC

This is the companion to the C version's REPORT.md (not in this repository; it would live in `c/`). The algorithms, the corpus, the
test protocol and the benchmark protocol are the same; this report concentrates on what the port
changed, what the Rust toolchain did differently, and the numbers for the Rust crate. All numbers
come from `results/` (regenerate with `sh scripts/run_all.sh`); §5 is pasted from
`results/summary.md`; the C-vs-Rust table in §4 is from `scripts/compare_c_rust.py` run on the
two result directories measured back-to-back in the same session.

Same machine and the same caveats as the C report: a 2-vCPU VM on a Sapphire/Emerald-Rapids-class
Xeon with **no PMU** (cycles are TSC ticks × the `corefreq` ratio, here 1.23 → ~2.6 GHz core;
instruction/miss/mispredict counts are a cachegrind simulation, AVX-512 excluded because valgrind
cannot run it), and run-to-run noise of ±15–30 %. Toolchain: rustc 1.97.0, `-O` with fat LTO and
`codegen-units = 1`; gcc 13.3 `-O2` for the C side.

## 1. Headline numbers (Rust crate, 61-file corpus, 48.8 MB PNG, 139 MB of pixels)

| variant | corpus total | pixels/s | vs scalar | vs png crate | vs libpng | inflate share | unfilter share |
|---|---|---|---|---|---|---|---|
| scalar backend | 827 ms | 168 MB/s | 1.00× | 0.59× | 1.03× | 48 % | 37 % |
| sse41 backend | 547 ms | 254 MB/s | **1.51×** | 0.89× | 1.56× | 73 % | 21 % |
| avx2 backend | 581 ms | 240 MB/s | 1.42× | 0.84× | 1.47× | 69 % | 25 % |
| avx512 backend | 547 ms | 255 MB/s | 1.51× | 0.89× | 1.56× | 73 % | 21 % |
| sse41 + system zlib inflate | 507 ms | 275 MB/s | 1.63× | 0.96× | 1.68× | 72 % | 22 % |
| sse41 + `fdeflate` inflate | **397 ms** | **350 MB/s** | 2.08× | **1.22×** | 2.15× | 65 % | 28 % |
| `png` crate 0.17 (pure Rust: fdeflate + its own unfilter) | 485 ms | 287 MB/s | 1.70× | 1.00× | 1.76× | – | – |
| libpng 1.6.43, simplified API (system build, no SSE) | 854 ms | 163 MB/s | 0.97× | 0.57× | 1.00× | – | – |

Three things stand out against the C report:

1. **The whole-decoder speedup over the scalar backend is 1.51×, not 1.86×.** Not because the
   SIMD got slower — the SIMD backends decode the corpus in 547 ms in Rust vs 531 ms in C in the
   same session (3 %) — but because the *scalar baseline got faster*: 827 ms vs 909 ms. LLVM
   compiles the spec-style, branchy Paeth predictor into branchless `cmov` code and
   auto-vectorises the Up filter, GCC `-O2` does neither (§3.1, §3.2). The honest scalar
   baseline is whatever the compiler makes of straightforward code, and the two compilers make
   different things of it.
2. **A better scalar inflate is worth more than every SIMD kernel combined.** Swapping the
   inflate stage for the `fdeflate` crate (a pure-Rust, scalar, PNG-specialised inflater: 12-bit
   lookups that decode up to two literals per step, RLE fast paths) takes the SIMD decoder from
   547 to 397 ms — a bigger absolute saving than vectorising unfilter+CRC+Adler+expand gave
   (827 → 547). That is exactly the Amdahl argument of the C report, now with a measurement:
   inflate was 48 % of the scalar decoder and is 65 % *even with fdeflate*.
3. **The `png` crate is faster than our SIMD decoder with our own inflater** (485 vs 547 ms) and
   slower than it with fdeflate (485 vs 397). The png crate *is* fdeflate plus an unfilter that
   LLVM auto-vectorises; our SIMD unfilter + CRC + Adler are what make up the 1.22×.

Per stage, inside the decoder, corpus totals:

| stage | scalar | sse41 | speedup | share before → after |
|---|---|---|---|---|
| parse + CRC-32 | 38.9 ms | 8.2 ms | 4.7× | 4.7 % → 1.5 % |
| inflate (same safe-Rust code in both) | 400 ms | 397 ms | 1.0× | 48 % → 73 % |
| Adler-32 | 42.2 ms | 8.8 ms | 4.8× | 5.1 % → 1.6 % |
| unfilter | 302 ms | 112 ms | 2.7× | 37 % → 21 % |
| expand | 43.8 ms | 21.3 ms | 2.1× | 5.3 % → 3.9 % |

## 2. What the port changed

* **The abstraction is a trait.** `Vec128` (`src/v128/mod.rs`) has an associated vector type and
  ~40 `unsafe fn` methods, implemented for `__m128i` (SSE), `uint8x16_t` (NEON) and `[u8; 16]`
  (portable). The generic kernels (`src/kernels/generic.rs`) are `#[inline(always)]` and are
  instantiated inside `#[target_feature(enable = …)]` entry points per backend, which is the
  standard way to get feature-specific code out of generic Rust: the intrinsics inline into a
  function that has the feature, so nothing is called across a feature boundary.
* **The public API is safe.** `Kernels` holds safe `fn` pointers; each wrapper asserts the slice
  contract (`row.len() >= len + ROW_SLACK`, `prev.len() >= len`, …) before calling the
  `unsafe` entry point, and the dispatcher only puts a table in the list after
  `is_x86_feature_detected!` has confirmed every feature it needs. Inside, the kernels use raw
  pointers for the loads that straddle `len` — but every such access is within the slice the
  caller handed over, so the `unsafe` blocks are locally justifiable rather than "trust the
  caller". The decoder gets the row and the previous row from one buffer with `split_at_mut`.
* **The previous-row slack went away.** The C kernels read up to 4 bytes past `len` of *both*
  rows; with Rust borrows the previous row is an immutable slice that must not overlap the
  mutable current row, so the Avg/Paeth vector loops now stop one pixel early (`while i + 4 <=
  len`) and the scalar tail finishes from the stored bytes. Cost: one scalar pixel per row.
  `ROW_SLACK` remains only for the *current* row, where the 3- and 6-bpp Sub scan loads 16 bytes
  for a 12-byte block, and it is only ever read, never written (property-tested with canaries).
* **The three x86 "select" encodings are marker types**: `Sse` (`pblendvb`), `SseVex`
  (`and/andnot/or`) and `SseEvex` (`vpternlogd`), sharing one macro-generated impl. §3.4 says what
  LLVM made of that.
* **Width is a const generic.** The C code switched on `bpp` to choose 4- or 8-byte loads with a
  runtime flag inside the pixel loop; LLVM kept that flag test inside the loop (one extra
  branch per pixel), so the Rust version instantiates `filter_avg_w::<X, 4>` and `::<X, 8>`.
* **Inflate is 100 % safe Rust** (slices, `chunks_exact`, `copy_within`/`copy_from_slice`), same
  algorithm as the C version. It runs within 2–3 % of the C version in the same session
  (397 vs 388 ms for the corpus). Two experiments: removing the two hottest bounds checks with
  `get_unchecked` changed nothing (LLVM had already proved them away); using `copy_from_slice`
  (memcpy) instead of `copy_within` (memmove) for non-overlapping matches was worth 3 %, kept.
* **Tests** are the same families, as `cargo test`: `tests/kernels.rs` (every backend vs scalar
  with random lengths/alignments/distributions and write-past-end canaries, CRC vs `crc32fast`,
  Adler vs `adler`, incremental = one-shot, plus the naive and branchless scalar variants vs the
  baseline), `tests/inflate.rs` (round-trips through `flate2` at every level over 7 data kinds,
  bit-flipped and truncated streams, hand-made fixed-Huffman and stored streams),
  `tests/png_corpus.rs` (all 61 files × all 5 x86 backends pixel-identical to the `png` crate,
  which — since the C version was pixel-identical to libpng — also cross-checks the png crate
  against libpng on this corpus; corrupted IDAT byte caught by the CRC). The kernel tests also
  run clean under valgrind memcheck (the closest thing to a sanitizer build that reaches the
  intrinsics; Miri does not execute x86 SIMD intrinsics, and valgrind masks AVX-512 so that
  backend is covered only by the native run).
* **A bug found by the port.** The C project's benchmark-only branchless scalar Paeth selected
  the wrong operand (`a ^ ((bc ^ a) & mask)` picks `bc` when the mask says "a"); it had never been
  correctness-tested because it was "just a benchmark variant". The Rust test suite tests the
  variants too, found it in the first run, and both versions are fixed (its timing was unaffected
  — same instruction count — and the C test suite now covers the variants as well).
* **A better Paeth formulation found by the port.** While looking at LLVM's AVX-512 codegen (§3.3)
  the min+min+cmpeq+cmpeq+select+select selection was rewritten as two compares — `pb > pc ? c : b`
  then `pa > min(pb,pc) ? that : a` — which implements the spec's tie-breaking exactly with an
  8-op loop-carried chain instead of 10. Ported back to C. Measured gain ≤10 % (the pixel loop is
  not purely chain-bound), but it is the shorter chain.

## 3. What LLVM did differently (and what it cost or saved)

### 3.1 The branchy scalar Paeth is not branchy

`paeth_pred` is written the way the PNG spec writes it, with `if`s, in both languages. GCC 13 keeps
the branches; on photo residuals cachegrind's predictor mispredicts 15–21 % of them and the loop
runs at 18–25 core cycles/byte. LLVM turns the same source into `cmov`s: the Rust scalar Paeth/4
runs at **6.0 c/B** with no mispredicts, and the "branchless" scalar variant written with masks
measures 5.8 — within noise of the branchy one, which is the point: they are the same code after
LLVM. So:

| Paeth/4, core c/B | scalar | sse41 | SIMD speedup |
|---|---|---|---|
| C (gcc) | 18.9 | 2.6 | 7.3× |
| Rust (LLVM) | 6.0 | 2.4 | 2.5× |

The SIMD kernel is the same code within noise; the speedup number is a property of the compiler's
treatment of the baseline. The C report said "whether you report 7× or 2× depends on whether
your scalar code branches"; in Rust you do not get to choose.

### 3.2 LLVM auto-vectorises the scalar Up filter (and some of the others)

The scalar `Up` is `for i in 0..len { row[i] = row[i].wrapping_add(prev[i]) }`. GCC `-O2` emits a
byte loop (1.0–1.1 c/B); LLVM's vectoriser is on at `-O` and emits an SSE2 loop (the binary is
compiled for the baseline x86-64 target), so the Rust scalar Up runs at 0.15–0.17 c/B — the same
as the hand-written SSE/AVX2/AVX-512 kernels. **In Rust the SIMD Up filter buys nothing.** The same is true to a lesser degree for the
const-generic Avg loops (scalar Avg/8: 0.64 c/B in Rust vs 2.14 in C, Avg/4 1.11 vs 2.20), the 16→8 conversion and
the portable backend (§3.5).

### 3.3 LLVM rewrote the AVX-512 Paeth chain into something longer

The chain ends with `a = (zext(x) + pred) & 0x00FF` (keep the 16-bit lanes in 0..255 for the next
pixel). With the mask constant visible, LLVM rewrites this as `zext(trunc(x + pred))`, which is
mathematically identical and on AVX-512BW lowers to `vpmovwb` + `vpaddb` + `vpmovzxbw`: three
instructions with ~6 cycles of latency on the loop-carried chain instead of `vpaddw` + `vpand`
(two, 2 cycles). The AVX-512 Paeth/4 measured 3.1 TSC-c/B against 2.1 for the SSE4.1 build of the
*same source* (where no narrowing instruction exists, so the rewrite does not fire). The fix was
to hide the constant from the optimiser — `X::set1_u16(core::hint::black_box(0x00FF))`, once per
row — after which the AVX-512 build is back to 2.0. The lesson generalises: with intrinsics you
are expressing *semantics*, not instructions, and LLVM will happily re-select a narrower
instruction sequence that is longer in latency.

Two smaller instances in the same loop: LLVM rewrote `min(pa, q) == pa` into `pa <= q`, which on
AVX-512 is a compare into a mask register plus `vpmovm2w` back (2 ops, 3–4 cycles) instead of
`vpcmpeqw` (1 op) — the two-compare reformulation of the selection (§2) sidesteps it; and the
`pack16` saturating pack gets `vpmaxsw`/`vpminsw` clamps inserted before `vpackuswb` (redundant
but off the critical path).

### 3.4 You cannot avoid `vpblendvb` from intrinsics

The C report found that VEX-encoded `vpblendvb` (2 uops, 2 cycles on Intel) costs the AVX2 build
20–25 % on Paeth, and worked around it with `and/andnot/or`. In Rust the `SseVex` marker type does
the same — and LLVM recognises the three-instruction idiom as a select and emits `vpblendvb`
anyway. The AVX2 Paeth/4 is 3.2 c/B vs 2.4 for both SSE4.1 and AVX-512 (`vpternlogd`, which LLVM
leaves alone because it *is* the canonical form there). The only ways out are inline asm or
AVX-512VL; the AVX2 backend keeps the penalty and the dispatcher ranks `avx512` and `sse41` above
it for this reason (they tie on the corpus; AVX2 is 6 % slower).

### 3.5 The portable backend is less bad under LLVM, and still not good

The `[u8; 16]` implementation of `Vec128` compiles to 1.0–1.8 c/B for the Sub scan (GCC: 3.5–9.5)
and 0.44 c/B for RGB→RGBA — *faster than the scalar baseline's 0.76* — because LLVM recovers
vector shuffles from the byte loops. Paeth and Avg through the union are still 1.4–3.5× slower
than scalar,
so the dispatcher still ranks `scalar` above `portable`. (Under GCC it was 7–15× slower; the
conclusion "a portable fallback should be scalar code" survives, just less dramatically.)

### 3.6 The slicing-by-8 CRC needed its xor tree spelled out

The scalar CRC-32 was 1.5× slower in Rust than in C until the eight table lookups were summed as
two explicit sub-trees: LLVM emitted a serial chain of seven `xor`s through all eight lookups, so
the four lookups that depend only on the data bytes (and could overlap the previous iteration)
were waiting on the crc-dependent ones; GCC reassociated. With `(t3^t2)^(t1^t0)` computed first
the Rust version matches the C one (1.40 vs 1.40 c/B).

### 3.7 Things that were the same

CRC-32 folding (12× at 128 bits, 37× for two zmm VPCLMULQDQ accumulators — the x86 kernels are the
same intrinsics and measure identically: 0.12 / 0.12 / 0.04 c/B in both languages), Adler-32
(7×/11×/18×), the 128-bit Sub scan (1.3× for bpp 4, 0.8× for bpp 8, 3.3–3.7× for bpp 1), the
pixel-serial Avg (1.0–1.4×) and the chain-bound Paeth (2.4 c/B SSE4.1 in both).

## 4. C vs Rust, same session

<!-- COMPARE -->
### C (gcc 13 -O2) vs Rust (rustc 1.97 -O, fat LTO): core cycles per byte, 32 KiB buffers, photo distribution (core/TSC: C run 1.24, Rust run 1.24)

| kernel | scalar C / Rust | sse41 C / Rust | avx2 C / Rust | avx512 C / Rust | portable C / Rust |
|---|---|---|---|---|---|
| sub/1 | 1.92 / 1.01 | 0.27 / 0.30 | 0.29 / 0.28 | 0.30 / 0.27 | 9.46 / 1.77 |
| sub/3 | 0.70 / 0.68 | 0.37 / 0.33 | 0.35 / 0.36 | 0.34 / 0.36 | 7.92 / 1.49 |
| sub/4 | 0.34 / 0.37 | 0.25 / 0.29 | 0.25 / 0.28 | 0.27 / 0.33 | 4.81 / 1.50 |
| sub/8 | 0.29 / 0.20 | 0.23 / 0.24 | 0.23 / 0.29 | 0.23 / 0.25 | 3.48 / 0.96 |
| up/1 | 1.12 / 0.15 | 0.15 / 0.16 | 0.14 / 0.15 | 0.16 / 0.13 | 0.15 / 0.16 |
| up/3 | 1.03 / 0.16 | 0.15 / 0.15 | 0.13 / 0.15 | 0.15 / 0.14 | 0.14 / 0.15 |
| up/4 | 1.04 / 0.17 | 0.17 / 0.16 | 0.14 / 0.15 | 0.16 / 0.13 | 0.15 / 0.16 |
| up/8 | 1.06 / 0.16 | 0.15 / 0.16 | 0.14 / 0.17 | 0.14 / 0.15 | 0.16 / 0.15 |
| avg/1 | 3.81 / 3.87 | 3.98 / 3.76 | 3.79 / 3.76 | 3.77 / 3.91 | 3.87 / 3.84 |
| avg/3 | 1.62 / 1.84 | 1.20 / 1.36 | 1.19 / 1.39 | 1.05 / 1.37 | 18.57 / 2.35 |
| avg/4 | 2.20 / 1.11 | 0.99 / 1.02 | 0.90 / 1.13 | 0.87 / 0.96 | 16.07 / 1.76 |
| avg/8 | 2.14 / 0.64 | 0.49 / 0.60 | 0.49 / 0.56 | 0.50 / 0.46 | 5.67 / 2.27 |
| paeth/1 | 24.57 / 12.51 | 20.25 / 13.86 | 20.83 / 14.54 | 20.02 / 12.97 | 22.04 / 13.14 |
| paeth/3 | 17.87 / 7.58 | 3.25 / 3.49 | 3.73 / 4.61 | 3.02 / 3.30 | 41.60 / 11.09 |
| paeth/4 | 18.93 / 5.98 | 2.59 / 2.41 | 2.85 / 3.16 | 2.39 / 2.44 | 26.12 / 8.24 |
| paeth/8 | 21.43 / 2.29 | 1.20 / 1.28 | 1.43 / 1.61 | 1.17 / 1.25 | 13.02 / 6.24 |
| paeth/4 scalar-branchless | 5.17 / 5.81 | - | - | - | - |
| paeth/4 scalar-naive | 18.79 / 12.31 | - | - | - | - |
| crc32 | 1.40 / 1.40 | 0.12 / 0.12 | 0.12 / 0.12 | 0.04 / 0.04 | 1.39 / 1.41 |
| crc32_bytewise | 7.33 / 7.37 | 7.48 / 7.31 | 7.56 / 7.25 | 7.31 / 7.20 | 7.47 / 7.38 |
| adler32 | 1.00 / 0.97 | 0.15 / 0.14 | 0.08 / 0.09 | 0.07 / 0.05 | 0.99 / 0.84 |
| rgb_to_rgba | 0.75 / 0.76 | 0.14 / 0.16 | 0.11 / 0.12 | 0.11 / 0.10 | 3.35 / 0.44 |
| u16_to_u8 | 0.47 / 0.53 | 0.08 / 0.04 | 0.04 / 0.05 | 0.04 / 0.04 | 0.65 / 0.34 |
| memcpy (row restore) | 0.07 / 0.07 | 0.07 / 0.07 | 0.07 / 0.07 | 0.07 / 0.07 | 0.07 / 0.07 |

### Whole-decoder corpus totals, same session (ms; stage shares in %)

| variant | C total | C inflate | C unfilter | Rust total | Rust inflate | Rust unfilter |
|---|---|---|---|---|---|---|
| scalar | 909 | 383 (42%) | 417 (46%) | 827 | 400 (48%) | 302 (36%) |
| sse41 | 531 | 381 (72%) | 111 (21%) | 547 | 397 (73%) | 112 (20%) |
| avx2 | 553 | 387 (70%) | 129 (23%) | 581 | 401 (69%) | 142 (24%) |
| avx512 | 531 | 388 (73%) | 108 (20%) | 547 | 398 (73%) | 112 (20%) |
| scalar+zlib | 829 | 346 (42%) | 418 (50%) | 747 | 369 (49%) | 296 (40%) |
| sse41+zlib | 477 | 342 (72%) | 107 (22%) | 507 | 365 (72%) | 112 (22%) |
| sse41+fdeflate | - | - | - | 397 | 258 (65%) | 112 (28%) |
| libpng | 839 | - | - | 854 | - | - |
| spng | 628 | - | - | - | - | - |
| png-crate | - | - | - | 485 | - | - |

<!-- /COMPARE -->

## 5. Measured tables (Rust)

(Pasted from `results/summary.md` by `scripts/insert_summary.py`.)

<!-- SUMMARY -->
### Kernel cycles per byte (32 KiB buffers, "photo" byte distribution; core cycles = TSC ticks x 1.24, see bench/corefreq.c)

Speedup is scalar / backend. "scalar-naive" is the textbook row[i] += row[i-bpp] loop; "scalar" is the register-carried baseline used for all speedups.

| kernel | scalar c/B | portable c/B | sse41 c/B | avx2 c/B | avx512 c/B | portable × | sse41 × | avx2 × | avx512 × |
|---|---|---|---|---|---|---|---|---|---|
| sub/1 | 1.01 | 1.77 | 0.30 | 0.28 | 0.27 | 0.6 | 3.3 | 3.6 | 3.7 |
| sub/1 scalar-naive | 7.28 |  |  |  |  |  |  |  |  |
| sub/2 | 0.61 | 1.38 | 0.29 | 0.30 | 0.30 | 0.4 | 2.1 | 2.0 | 2.0 |
| sub/2 scalar-naive | 3.63 |  |  |  |  |  |  |  |  |
| sub/3 | 0.68 | 1.49 | 0.33 | 0.36 | 0.36 | 0.5 | 2.1 | 1.9 | 1.9 |
| sub/3 scalar-naive | 2.52 |  |  |  |  |  |  |  |  |
| sub/4 | 0.37 | 1.50 | 0.29 | 0.28 | 0.33 | 0.2 | 1.3 | 1.3 | 1.1 |
| sub/4 scalar-naive | 1.91 |  |  |  |  |  |  |  |  |
| sub/6 | 0.38 | 1.72 | 0.36 | 0.33 | 0.33 | 0.2 | 1.0 | 1.2 | 1.2 |
| sub/6 scalar-naive | 1.67 |  |  |  |  |  |  |  |  |
| sub/8 | 0.20 | 0.96 | 0.24 | 0.29 | 0.25 | 0.2 | 0.8 | 0.7 | 0.8 |
| sub/8 scalar-naive | 1.47 |  |  |  |  |  |  |  |  |
| up/1 | 0.15 | 0.16 | 0.16 | 0.15 | 0.13 | 1.0 | 1.0 | 1.0 | 1.1 |
| up/2 | 0.16 | 0.15 | 0.15 | 0.15 | 0.15 | 1.0 | 1.0 | 1.1 | 1.1 |
| up/3 | 0.16 | 0.15 | 0.15 | 0.15 | 0.14 | 1.0 | 1.0 | 1.0 | 1.1 |
| up/4 | 0.17 | 0.16 | 0.16 | 0.15 | 0.13 | 1.1 | 1.1 | 1.2 | 1.3 |
| up/6 | 0.17 | 0.16 | 0.21 | 0.16 | 0.14 | 1.1 | 0.8 | 1.1 | 1.2 |
| up/8 | 0.16 | 0.15 | 0.16 | 0.17 | 0.15 | 1.1 | 1.0 | 1.0 | 1.1 |
| avg/1 | 3.87 | 3.84 | 3.76 | 3.76 | 3.91 | 1.0 | 1.0 | 1.0 | 1.0 |
| avg/1 scalar-naive | 8.17 |  |  |  |  |  |  |  |  |
| avg/2 | 2.13 | 2.14 | 2.30 | 2.12 | 2.13 | 1.0 | 0.9 | 1.0 | 1.0 |
| avg/2 scalar-naive | 4.48 |  |  |  |  |  |  |  |  |
| avg/3 | 1.84 | 2.35 | 1.36 | 1.39 | 1.37 | 0.8 | 1.4 | 1.3 | 1.3 |
| avg/3 scalar-naive | 3.30 |  |  |  |  |  |  |  |  |
| avg/4 | 1.11 | 1.76 | 1.02 | 1.13 | 0.96 | 0.6 | 1.1 | 1.0 | 1.2 |
| avg/4 scalar-naive | 2.18 |  |  |  |  |  |  |  |  |
| avg/6 | 0.94 | 2.90 | 0.71 | 0.75 | 0.62 | 0.3 | 1.3 | 1.2 | 1.5 |
| avg/6 scalar-naive | 2.11 |  |  |  |  |  |  |  |  |
| avg/8 | 0.64 | 2.27 | 0.60 | 0.56 | 0.46 | 0.3 | 1.1 | 1.1 | 1.4 |
| avg/8 scalar-naive | 1.91 |  |  |  |  |  |  |  |  |
| paeth/1 | 12.51 | 13.14 | 13.86 | 14.54 | 12.97 | 1.0 | 0.9 | 0.9 | 1.0 |
| paeth/1 scalar-naive | 17.67 |  |  |  |  |  |  |  |  |
| paeth/1 scalar-branchless | 14.96 |  |  |  |  | 1.1 | 1.1 | 1.0 | 1.2 |
| paeth/2 | 6.96 | 6.90 | 9.26 | 9.78 | 8.73 | 1.0 | 0.8 | 0.7 | 0.8 |
| paeth/2 scalar-naive | 9.80 |  |  |  |  |  |  |  |  |
| paeth/3 | 7.58 | 11.09 | 3.49 | 4.61 | 3.30 | 0.7 | 2.2 | 1.6 | 2.3 |
| paeth/3 scalar-naive | 7.94 |  |  |  |  |  |  |  |  |
| paeth/3 scalar-branchless | 5.58 |  |  |  |  | 0.5 | 1.6 | 1.2 | 1.7 |
| paeth/4 | 5.98 | 8.24 | 2.41 | 3.16 | 2.44 | 0.7 | 2.5 | 1.9 | 2.5 |
| paeth/4 scalar-naive | 12.31 |  |  |  |  |  |  |  |  |
| paeth/4 scalar-branchless | 5.81 |  |  |  |  | 0.7 | 2.4 | 1.8 | 2.4 |
| paeth/6 | 4.38 | 8.26 | 1.68 | 2.19 | 1.59 | 0.5 | 2.6 | 2.0 | 2.8 |
| paeth/6 scalar-naive | 7.40 |  |  |  |  |  |  |  |  |
| paeth/8 | 2.29 | 6.24 | 1.28 | 1.61 | 1.25 | 0.4 | 1.8 | 1.4 | 1.8 |
| paeth/8 scalar-naive | 10.21 |  |  |  |  |  |  |  |  |
| crc32 | 1.395 | 1.414 | 0.119 | 0.116 | 0.038 | 1.0 | 11.7 | 12.0 | 36.7 |
| adler32 | 0.972 | 0.840 | 0.136 | 0.086 | 0.053 | 1.2 | 7.1 | 11.4 | 18.2 |
| rgb_to_rgba | 0.764 | 0.442 | 0.161 | 0.119 | 0.102 | 1.7 | 4.7 | 6.4 | 7.5 |
| u16_to_u8 | 0.532 | 0.336 | 0.039 | 0.047 | 0.040 | 1.6 | 13.5 | 11.4 | 13.3 |
| memcpy (row restore, included in every filter row above) | 0.073 | 0.073 | 0.070 | 0.072 | 0.074 | 1.0 | 1.0 | 1.0 | 1.0 |
| sub/4 AVX2 256-bit scan (experiment) |  |  |  | 0.24 |  |  |  |  |  |

### Buffer-size sweep (photo distribution, cycles per byte)

| kernel | backend | 1 KiB | 32 KiB | 256 KiB | 8 MiB |
|---|---|---|---|---|---|
| up/4 | scalar | 0.131 | 0.172 | 0.152 | 0.403 |
| up/4 | sse41 | 0.186 | 0.155 | 0.151 | 0.429 |
| up/4 | avx2 | 0.112 | 0.149 | 0.149 | 0.429 |
| up/4 | avx512 | 0.076 | 0.133 | 0.137 | 0.408 |
| sub/4 | scalar | 0.296 | 0.372 | 0.314 | 0.445 |
| sub/4 | sse41 | 0.382 | 0.285 | 0.330 | 0.399 |
| sub/4 | avx2 | 0.244 | 0.276 | 0.279 | 0.397 |
| sub/4 | avx512 | 0.250 | 0.325 | 0.279 | 0.414 |
| paeth/4 | scalar | 6.055 | 5.979 | 6.095 | 6.292 |
| paeth/4 | sse41 | 2.498 | 2.413 | 2.450 | 2.667 |
| paeth/4 | avx2 | 3.299 | 3.163 | 3.386 | 3.462 |
| paeth/4 | avx512 | 2.444 | 2.436 | 2.468 | 2.616 |
| crc32 | scalar | 1.426 | 1.395 | 1.388 | 1.486 |
| crc32 | sse41 | 0.129 | 0.119 | 0.124 | 0.131 |
| crc32 | avx2 | 0.138 | 0.116 | 0.115 | 0.140 |
| crc32 | avx512 | 0.087 | 0.038 | 0.040 | 0.097 |
| adler32 | scalar | 0.946 | 0.972 | 0.982 | 0.996 |
| adler32 | sse41 | 0.149 | 0.136 | 0.167 | 0.158 |
| adler32 | avx2 | 0.091 | 0.086 | 0.095 | 0.114 |
| adler32 | avx512 | 0.066 | 0.053 | 0.062 | 0.120 |
| rgb_to_rgba | scalar | 0.683 | 0.764 | 0.781 | 1.033 |
| rgb_to_rgba | sse41 | 0.195 | 0.161 | 0.154 | 0.244 |
| rgb_to_rgba | avx2 | 0.112 | 0.119 | 0.119 | 0.245 |
| rgb_to_rgba | avx512 | 0.085 | 0.102 | 0.101 | 0.246 |
| u16_to_u8 | scalar | 0.514 | 0.532 | 0.481 | 0.487 |
| u16_to_u8 | sse41 | 0.050 | 0.039 | 0.046 | 0.143 |
| u16_to_u8 | avx2 | 0.047 | 0.047 | 0.061 | 0.155 |
| u16_to_u8 | avx512 | 0.040 | 0.040 | 0.050 | 0.156 |

### Data-distribution effect (32 KiB, cycles per byte)

| kernel | backend | uniform | photo (small residuals) | zeros |
|---|---|---|---|---|
| paeth/4 | scalar | 6.02 | 5.98 | 5.99 |
| paeth/4 | sse41 | 2.42 | 2.41 | 2.43 |
| paeth/3 | scalar | 6.84 | 7.58 | 5.98 |
| paeth/3 | sse41 | 3.45 | 3.49 | 3.27 |
| avg/4 | scalar | 1.11 | 1.11 | 1.10 |
| avg/4 | sse41 | 1.03 | 1.02 | 1.09 |
| paeth_branchless/4 | scalar | 5.94 | 5.81 | 6.17 |
| sub/1 | scalar | 1.03 | 1.01 | 1.00 |
| sub/1 | sse41 | 0.30 | 0.30 | 0.31 |
| crc32 | scalar | 1.40 | 1.40 | 1.39 |
| crc32 | sse41 | 0.12 | 0.12 | 0.12 |
| adler32 | scalar | 0.93 | 0.97 | 1.02 |
| adler32 | sse41 | 0.13 | 0.14 | 0.15 |

### Whole-decoder: corpus totals (sum of per-file best-of-N wall-clock times)

| variant | total ms | pixel MB/s | speedup vs scalar | speedup vs libpng | parse+crc % | inflate % | adler % | unfilter % | expand % | other % |
|---|---|---|---|---|---|---|---|---|---|---|
| avx512 | 546.7 | 255 | 1.51 | 1.56 | 1.2 | 72.9 | 1.1 | 20.5 | 4.3 | 0.0 |
| avx2 | 580.6 | 240 | 1.42 | 1.47 | 1.5 | 69.1 | 1.2 | 24.5 | 3.8 | 0.0 |
| sse41 | 547.2 | 254 | 1.51 | 1.56 | 1.5 | 72.5 | 1.6 | 20.5 | 3.9 | 0.0 |
| scalar | 826.6 | 168 | 1.00 | 1.03 | 4.7 | 48.4 | 5.1 | 36.5 | 5.3 | 0.0 |
| avx512+zlib | 508.0 | 274 | 1.63 | 1.68 | 1.3 | 72.0 | 0.0 | 22.5 | 4.3 | 0.0 |
| avx2+zlib | 538.0 | 259 | 1.54 | 1.59 | 1.5 | 68.2 | 0.0 | 26.3 | 3.9 | 0.0 |
| sse41+zlib | 506.7 | 275 | 1.63 | 1.68 | 1.6 | 72.1 | 0.0 | 22.1 | 4.2 | 0.0 |
| scalar+zlib | 747.5 | 186 | 1.11 | 1.14 | 5.2 | 49.4 | 0.0 | 39.6 | 5.7 | 0.0 |
| avx512+fdeflate | 399.4 | 349 | 2.07 | 2.14 | 1.6 | 65.1 | 0.0 | 28.0 | 5.3 | 0.0 |
| avx2+fdeflate | 430.1 | 324 | 1.92 | 1.99 | 1.9 | 60.5 | 0.0 | 32.8 | 4.8 | 0.0 |
| sse41+fdeflate | 397.3 | 350 | 2.08 | 2.15 | 1.9 | 65.1 | 0.0 | 28.1 | 5.0 | 0.0 |
| scalar+fdeflate | 639.5 | 218 | 1.29 | 1.34 | 6.1 | 41.2 | 0.0 | 46.3 | 6.4 | 0.0 |
| png-crate | 485.4 | 287 | 1.70 | 1.76 | - | - | - | - | - | - |
| libpng | 853.8 | 163 | 0.97 | 1.00 | - | - | - | - | - | - |

### Whole-decoder by image class (ms totals; speedups relative to scalar backend)

| image class | avx512 ms | avx2 ms | sse41 ms | scalar ms | png-crate ms | libpng ms | best SIMD ×scalar | best SIMD ×libpng | best SIMD ×png-crate | unfilter % scalar→SIMD | inflate % scalar→SIMD |
|---|---|---|---|---|---|---|---|---|---|---|---|
| RGB8 photos, adaptive filters (9 files) | 61.2 | 65.0 | 61.6 | 91.1 | 57.0 | 92.9 | 1.49 (avx512) | 1.52 | 0.93 | 34→20 | 51→75 |
| RGB8 photos, upscaled 2x/4x (4 files, 1.4-6.5 MB raw) | 147.2 | 160.0 | 145.7 | 248.4 | 143.9 | 236.5 | 1.70 (sse41) | 1.62 | 0.99 | 46→25 | 40→65 |
| RGB8 Pillow-encoded, levels 1/6/9 (9 files) | 134.1 | 144.5 | 133.2 | 221.0 | 124.3 | 224.4 | 1.66 (sse41) | 1.68 | 0.93 | 45→25 | 42→69 |
| gray8 photos (10 files) | 14.3 | 14.3 | 14.3 | 15.5 | 8.1 | 18.3 | 1.09 (avx2) | 1.28 | 0.57 | 43→47 | 47→52 |
| RGBA8 (7 files) | 52.4 | 56.2 | 53.1 | 75.7 | 41.0 | 83.0 | 1.45 (avx512) | 1.59 | 0.78 | 35→20 | 52→74 |
| 16-bit (6 files) | 49.7 | 50.5 | 50.4 | 63.1 | 39.8 | 77.7 | 1.27 (avx512) | 1.56 | 0.80 | 10→8 | 67→87 |
| palette / sub-byte (5 files) | 12.0 | 12.0 | 12.1 | 13.7 | 8.5 | 16.9 | 1.14 (avx2) | 1.41 | 0.71 | 21→24 | 52→59 |
| hubble RGB8 + moon gray8, all rows filter=none | 14.9 | 14.8 | 15.0 | 17.3 | 11.6 | 17.9 | 1.17 (avx2) | 1.21 | 0.78 | 0→0 | 81→95 |
| hubble RGB8 + moon gray8, all rows filter=sub | 14.9 | 15.0 | 14.9 | 17.9 | 11.7 | 20.6 | 1.20 (avx512) | 1.38 | 0.79 | 4→2 | 78→93 |
| hubble RGB8 + moon gray8, all rows filter=up | 14.3 | 14.8 | 14.3 | 17.0 | 11.2 | 18.4 | 1.19 (avx512) | 1.29 | 0.78 | 1→1 | 80→94 |
| hubble RGB8 + moon gray8, all rows filter=avg | 16.5 | 16.8 | 16.7 | 19.5 | 13.7 | 21.6 | 1.18 (avx512) | 1.31 | 0.83 | 10→9 | 73→87 |
| hubble RGB8 + moon gray8, all rows filter=paeth | 18.7 | 19.9 | 19.2 | 30.4 | 16.5 | 31.6 | 1.63 (avx512) | 1.69 | 0.89 | 43→23 | 46→73 |

### Per-stage speedups measured inside the decoder (corpus totals, scalar backend ÷ best SIMD backend)

| stage | scalar ms | avx512 ms | speedup | share of scalar total | share of avx512 total |
|---|---|---|---|---|---|
| parse+crc32 | 38.8 | 6.6 | 5.8 | 4.7% | 1.2% |
| inflate (scalar in both) | 400.5 | 398.5 | 1.0 | 48.4% | 72.9% |
| adler32 | 41.9 | 6.1 | 6.9 | 5.1% | 1.1% |
| unfilter | 301.6 | 111.9 | 2.7 | 36.5% | 20.5% |
| expand | 43.8 | 23.6 | 1.9 | 5.3% | 4.3% |
| total | 826.6 | 546.7 | 1.51 | 100% | 100% |

### Instruction and miss counts (cachegrind simulation: 48 KiB L1D, LL = L3; 64 KiB buffers, photo distribution)

IPC = instructions per core cycle, using the 32 KiB-buffer cycles/byte from the microbenchmark (same build, core cycles = TSC x 1.24). For the filter rows the row-restore memcpy (valgrind substitutes a 1-instruction-per-byte memcpy, 0.05 TSC-cycles/byte natively) has been subtracted from both instructions and cycles.

| kernel | backend | instr/byte | cycles/byte | IPC | D1 miss/KB | LL miss/KB | branches/KB | mispredict/KB | mispredict rate |
|---|---|---|---|---|---|---|---|---|---|
| sub/1 | scalar | 2.751 | 0.934 | 2.95 | 48.34 | 0.000 | 1280 | 0.03 | 0.0% |
| sub/3 | scalar | 2.668 | 0.611 | 4.37 | 48.37 | 0.000 | 1195 | 0.03 | 0.0% |
| sub/4 | scalar | 1.126 | 0.299 | 3.76 | 48.37 | 0.000 | 1152 | 0.03 | 0.0% |
| up/1 | scalar | 0.349 | 0.081 | 4.28 | 64.34 | 0.000 | 1057 | 0.05 | 0.0% |
| up/3 | scalar | 0.349 | 0.083 | 4.21 | 64.34 | 0.000 | 1057 | 0.05 | 0.0% |
| up/4 | scalar | 0.349 | 0.099 | 3.51 | 64.34 | 0.000 | 1057 | 0.05 | 0.0% |
| avg/1 | scalar | 7.501 | 3.797 | 1.98 | 64.34 | 0.000 | 1536 | 0.03 | 0.0% |
| avg/3 | scalar | 7.001 | 1.763 | 3.97 | 64.37 | 0.000 | 1365 | 0.03 | 0.0% |
| avg/4 | scalar | 3.501 | 1.038 | 3.37 | 64.36 | 0.000 | 1280 | 0.03 | 0.0% |
| paeth/1 | scalar | 33.768 | 12.441 | 2.71 | 64.39 | 0.000 | 3072 | 132.94 | 4.3% |
| paeth/3 | scalar | 33.516 | 7.503 | 4.47 | 64.41 | 0.000 | 2389 | 129.13 | 5.4% |
| paeth/4 | scalar | 13.001 | 5.907 | 2.20 | 64.39 | 0.000 | 1280 | 0.03 | 0.0% |
| paeth_branchless/4 | scalar | 13.750 | 5.739 | 2.40 | 64.34 | 0.000 | 1280 | 0.03 | 0.0% |
| crc32 | scalar | 2.876 | 1.395 | 2.06 | 16.19 | 0.004 | 128 | 0.02 | 0.0% |
| adler32 | scalar | 3.381 | 0.972 | 3.48 | 16.19 | 0.000 | 129 | 0.21 | 0.2% |
| rgb_to_rgba | scalar | 3.001 | 0.764 | 3.93 | 37.50 | 0.000 | 171 | 0.02 | 0.0% |
| u16_to_u8 | scalar | 1.595 | 0.532 | 3.00 | 24.16 | 0.000 | 32 | 0.02 | 0.1% |
| sub/1 | sse41 | 1.376 | 0.233 | 5.90 | 48.37 | 0.000 | 1088 | 0.03 | 0.0% |
| sub/3 | sse41 | 1.418 | 0.263 | 5.40 | 48.37 | 0.000 | 1109 | 0.03 | 0.0% |
| sub/4 | sse41 | 0.939 | 0.215 | 4.36 | 48.36 | 0.000 | 1088 | 0.03 | 0.0% |
| up/1 | sse41 | 0.407 | 0.088 | 4.62 | 64.36 | 0.000 | 1056 | 0.03 | 0.0% |
| up/3 | sse41 | 0.407 | 0.080 | 5.08 | 64.36 | 0.000 | 1056 | 0.03 | 0.0% |
| up/4 | sse41 | 0.407 | 0.085 | 4.81 | 64.36 | 0.000 | 1056 | 0.03 | 0.0% |
| avg/1 | sse41 | 7.502 | 3.688 | 2.03 | 64.44 | 0.000 | 1536 | 0.03 | 0.0% |
| avg/3 | sse41 | 5.335 | 1.286 | 4.15 | 64.42 | 0.000 | 1365 | 0.04 | 0.0% |
| avg/4 | sse41 | 4.001 | 0.950 | 4.21 | 64.42 | 0.000 | 1280 | 0.03 | 0.0% |
| paeth/1 | sse41 | 33.769 | 13.787 | 2.45 | 64.45 | 0.000 | 3072 | 132.94 | 4.3% |
| paeth/3 | sse41 | 11.335 | 3.415 | 3.32 | 64.44 | 0.000 | 1365 | 0.03 | 0.0% |
| paeth/4 | sse41 | 8.501 | 2.343 | 3.63 | 64.44 | 0.000 | 1280 | 0.03 | 0.0% |
| crc32 | sse41 | 0.455 | 0.119 | 3.83 | 16.22 | 0.000 | 16 | 0.02 | 0.1% |
| adler32 | sse41 | 0.635 | 0.136 | 4.67 | 16.22 | 0.000 | 33 | 0.20 | 0.6% |
| rgb_to_rgba | sse41 | 0.836 | 0.161 | 5.19 | 37.55 | 0.000 | 86 | 0.02 | 0.0% |
| u16_to_u8 | sse41 | 0.251 | 0.039 | 6.39 | 24.17 | 0.000 | 16 | 0.02 | 0.1% |
| sub/1 | avx2 | 1.064 | 0.207 | 5.15 | 48.36 | 0.000 | 1088 | 0.03 | 0.0% |
| sub/3 | avx2 | 1.168 | 0.287 | 4.07 | 48.36 | 0.000 | 1109 | 0.03 | 0.0% |
| sub/4 | avx2 | 0.814 | 0.204 | 3.99 | 48.34 | 0.000 | 1088 | 0.03 | 0.0% |
| up/1 | avx2 | 0.251 | 0.082 | 3.07 | 64.34 | 0.000 | 1056 | 0.03 | 0.0% |
| up/3 | avx2 | 0.251 | 0.077 | 3.25 | 64.34 | 0.000 | 1056 | 0.03 | 0.0% |
| up/4 | avx2 | 0.251 | 0.077 | 3.27 | 64.34 | 0.000 | 1056 | 0.03 | 0.0% |
| avg/1 | avx2 | 7.502 | 3.684 | 2.04 | 64.42 | 0.000 | 1536 | 0.03 | 0.0% |
| avg/3 | avx2 | 4.668 | 1.321 | 3.53 | 64.41 | 0.000 | 1365 | 0.03 | 0.0% |
| avg/4 | avx2 | 3.501 | 1.057 | 3.31 | 64.41 | 0.000 | 1280 | 0.03 | 0.0% |
| paeth/1 | avx2 | 33.769 | 14.469 | 2.33 | 64.44 | 0.000 | 3072 | 132.94 | 4.3% |
| paeth/3 | avx2 | 9.335 | 4.541 | 2.06 | 64.41 | 0.000 | 1365 | 0.03 | 0.0% |
| paeth/4 | avx2 | 7.001 | 3.091 | 2.26 | 64.41 | 0.000 | 1280 | 0.03 | 0.0% |
| crc32 | avx2 | 0.455 | 0.116 | 3.92 | 16.22 | 0.000 | 16 | 0.02 | 0.1% |
| adler32 | avx2 | 0.278 | 0.086 | 3.25 | 16.20 | 0.000 | 17 | 0.20 | 1.2% |
| rgb_to_rgba | avx2 | 0.461 | 0.119 | 3.88 | 37.52 | 0.000 | 43 | 0.02 | 0.0% |
| u16_to_u8 | avx2 | 0.142 | 0.047 | 3.06 | 24.17 | 0.000 | 8 | 0.02 | 0.2% |

<!-- /SUMMARY -->

## 6. Inflate

Nothing changes in the analysis of why the Huffman/LZ77 stage resists vectorisation (C report
§6): the bit position and the output position are both loop-carried through data-dependent
operations, and a DEFLATE block is one stream. What the Rust port adds is the measurement of what
a *good scalar* inflater does: `fdeflate` is ≈1.5× faster than both zlib and our decoder on this
corpus (258 vs 365–400 ms), by (a) a 12-bit first-level table whose entries pack up to two
literals, so common pairs of short codes cost one lookup, (b) special-casing runs, and (c) a
tighter bit buffer. None of that is SIMD (fdeflate uses `simd-adler32` for the trailer only), all
of it is the instruction-level work the C report listed as the real next step, and it moved the
whole decoder by 1.38× — more than any single SIMD kernel.

## 7. NEON

`src/v128/neon.rs` and `src/kernels/neon.rs` are one-to-one transcriptions of the C backend that
*was* run under qemu. In this environment the aarch64 Rust standard library could not be
downloaded (the toolchain mirror is not reachable), so the Rust NEON code has been neither
compiled nor run. It is the smallest backend (the generic kernels are shared and tested through
the SSE and portable instantiations; the NEON-specific code is the 40 trait methods, the PMULL
CRC and the `vmull_u8` Adler), but it should be treated as reviewed, not verified, until
`cargo test --target aarch64-unknown-linux-gnu` has been run.

**Update (simd_adventures workspace):** the aarch64 build failed on `vextq_u8::<{ 16 - N }>` (a
generic const expression, not allowed on stable); replaced with a match in `v128::neon::bsl_u8`.
With that fix the full test suite, including the corpus comparison against the `png` crate,
passes under qemu-aarch64 `-cpu max` (NEON + PMULL).

## 8. Summary

| kernel | 128-bit gain vs Rust scalar | vs C scalar (C report) | why they differ |
|---|---|---|---|
| CRC-32 | 12× (37× 2×zmm) | 12× (37×) | identical |
| Adler-32 | 7× / 11× / 18× | 7× / 14× / 15× | identical within noise |
| 16→8, RGB→RGBA | 11–13×, 5–7.5× | 10–15×, 9–13× | identical SIMD; LLVM's scalar RGB→RGBA was 0.76 vs 0.75 this session |
| Up | **1.0–1.3×** | 7× | LLVM auto-vectorises the scalar loop |
| Sub | 3.3–3.7× (bpp 1) … 0.8× (bpp 8) | 4.6× … 1.3× | same story; LLVM's scalar Sub/1 is 2× GCC's |
| Avg | 1.0–1.4× | 1.3–4.5× | LLVM's scalar Avg is 2–3× faster than GCC's |
| Paeth bpp 3/4/6/8 | 1.8–2.8× | 5–17× | LLVM's scalar Paeth is branchless (6 vs 19 c/B) |
| **whole decoder** | **1.51×** (2.08× with fdeflate) | 1.86× | faster scalar baseline; inflate dominates |

Rust-specific conclusions: the safe API costs nothing measurable; the intrinsics are the same but
the *optimiser* is not — it made the scalar baselines faster (so the SIMD looks less impressive),
it re-selected longer instruction sequences in two latency-bound loops (fixable, once you look at
the assembly), and it undid one C-side workaround (VEX blendv). The single most valuable change
on the table remains a better scalar inflater.
