# Architecture

This file uses [Prose form](AGENTS.md#prose-form). It records
the design choices for the crate and the 0.1.0 starting point.
Code structure sections will grow as the ladder lands; the
choices below are the founding state (2026-07-21, pre-scaffold).

## Overview

The crate is a `no_std`, no-alloc reimplementation of the
HdrHistogram log-linear bucketing scheme (Gil Tene et al.,
[HdrHistogram](https://github.com/HdrHistogram/HdrHistogram)),
sized and shaped for tprobe-class continuous monitoring: tens
to hundreds of probes per process, each recording tick-counter
deltas run-to-completion, analyzed at the end of the run.

- The one hot operation is `record(value)`: find the bucket
  (clz + shift), saturating-increment its counter. No floats,
  no allocation, no panics, no data-dependent work.
- Everything else — quantiles, merge, iteration — is
  O(buckets), `no_std`-capable, and off the measured path.
- Lifecycle for 0.1.0 is run-to-completion: create, record,
  analyze at end. No snapshot/swap/reset API yet (see
  [Deferred directions](#deferred-directions)).

## Parameterization (h2)

The h2 restatement of HdrHistogram's bucketing — borrowed from
iopsystems' [histogram](https://github.com/iopsystems/histogram)
crate, written up at [h2histogram.org](https://h2histogram.org/)
— replaces `(lowest, highest, significant_figures)` with
two powers, chosen per instance at runtime:

- `grouping_power` (g) — each power-of-two range is split
  into 2ᵍ linear sub-buckets; relative value error ≤ 2⁻ᵍ.
- `max_value_power` (n) — max trackable value is 2ⁿ−1;
  over-range values clamp into the top bucket (for lateness
  detection the overflow *is* the signal).
- Values below 2ᵍ⁺¹ are recorded exactly.
- `buckets = (n − g + 1) · 2ᵍ`.

## Storage: config / counts split

The load-bearing structural choice: config and counts storage
are separate, and the core histogram type *borrows* its counts.

- `Config` — the two powers plus derived index math; `const fn`
  so callers can size storage at compile time.
- `Histogram<'a, C>` — config + `&'a mut [C]` counts; no
  derived state (totals are summed at read time, keeping the
  record path minimal). The caller owns the memory: a static
  buffer, a stack array, or (with `std`) a heap slice.
- `HistogramArray<const LEN, Cnt>` — owned inline `[Cnt; LEN]`
  convenience wrapper with a size check against
  `Config::total_buckets()`. Rust's stable const generics
  can't derive LEN from (g, n) directly (`generic_const_exprs`
  is unstable), so LEN is explicit and checked.
- Why borrowing is the default: the future buffer-swap model —
  a background task handing a probe a fresh zeroed slice and
  taking the full one — needs detachable storage. Keeping
  config and counts unfused now means that model lands later
  without reshaping the type.

## Counts

- Counter type is generic (`u8`/`u16`/`u32`/`u64`) with
  **u32 the default**; increments saturate — no panic, no
  wraparound lies.
- We think u32 is the right monitoring default because a
  deployment that watches stability will bound per-histogram
  totals (by run length now, by snapshot interval later);
  u64 is for run-forever accumulation and doubles the
  footprint for it.

## Feature map

- Core: `#![cfg_attr(not(feature = "std"), no_std)]`,
  no dependencies.
- `std` (default): the render module (`report`) and any
  std-needing helpers.
- Band-table reporting **is** in this crate as of 0.1.3,
  split along the device/service line: the `no_std` modules
  build report *structures* (`bands`, `stats`, `table`) — the
  artifacts a device would ship over a wire — and the
  `std`-gated `report` module renders them as text on the
  service side. This reverses the founding decision to leave
  reporting to tprobe — see
  [Readout requirements](#readout-requirements-band-tables).
- Dev-dependencies (tests/benches only): iopsystems
  `histogram` and `hdrhistogram` 7 as correctness oracles,
  plus a bench harness.

## API sketch (provisional)

```rust
pub struct Config { /* grouping_power, max_value_power */ }
impl Config {
    pub const fn new(g: u8, n: u8) -> Result<Config, Error>;
    pub const fn total_buckets(&self) -> usize;
    pub const fn index_for(&self, value: u64) -> usize;
    pub const fn value_range(&self, index: usize) -> (u64, u64);
}

pub struct Histogram<'a, Cnt: Counter = u32> {
    // config, counts: &'a mut [Cnt]
}
impl<Cnt: Counter> Histogram<'_, Cnt> {
    pub fn record(&mut self, value: u64);
    pub fn record_n(&mut self, value: u64, count: u64);
    pub fn quantile(&self, fraction: f64) -> Option<u64>;
    pub fn merge_from(&mut self, other: &Self) -> Result<(), Error>;
    pub fn buckets(&self) -> impl Iterator<Item = Bucket>;
}

pub struct HistogramArray<const LEN: usize, Cnt: Counter = u32> {
    // owned [Cnt; LEN], checked against Config::total_buckets()
}
```

## Size tradeoffs

`bytes = (n − g + 1) · 2ᵍ · sizeof(C)` + ~48 B header
(min/max/config if the hot-path extras are adopted).

What a max value of 2ⁿ ticks spans in time depends on the tick
source's rate:

| n  | 100 Hz  | 1 kHz   | 3 GHz   |
|----|---------|---------|---------|
| 16 | ~11 min | ~66 s   | ~22 µs  |
| 20 | ~2.9 h  | ~17 min | ~350 µs |
| 24 | ~1.9 d  | ~4.7 h  | ~5.6 ms |
| 32 | ~1.4 y  | ~50 d   | ~1.4 s  |
| 36 | ~22 y   | ~2.2 y  | ~23 s   |
| 40 | ~349 y  | ~35 y   | ~6 min  |

A slow tick source therefore wants a much smaller n, which
shrinks the footprint directly: e.g. a 1 kHz tick watching
1 ms..65 s lateness needs only n = 16 — at g = 4/u32 that is
(16−4+1)·2⁴ = 208 buckets ≈ 832 B per probe.

| g  | n  | C   | rel. err | buckets | bytes   |
|----|----|-----|----------|---------|---------|
| 2  | 32 | u16 | 25%      | 124     | ~250 B  |
| 4  | 36 | u32 | 6.3%     | 528     | ~2.1 KB |
| 6  | 40 | u32 | 1.6%     | 2,240   | ~9 KB   |
| 7  | 40 | u32 | 0.8%     | 4,352   | ~17 KB  |
| 7  | 40 | u64 | 0.8%     | 4,352   | ~34 KB  |
| 10 | 40 | u32 | 0.1%     | 31,744  | ~124 KB |

- rel. err is the value-quantization bound 2⁻ᵍ: each
  power-of-two range holds 2ᵍ equal-width buckets, so a
  bucket spans ≤ 2⁻ᵍ of the values in it. Counts are exact;
  only the value axis is quantized — any value read back
  (quantile, boundary) is within 2⁻ᵍ of the recorded one.
  Values below 2ᵍ⁺¹ are exact (width-1 buckets).
  - 2⁻ᵍ is the full uncertainty band: reporting a bucket
    endpoint gives one-sided error up to 2⁻ᵍ; reporting the
    midpoint halves it to ±2⁻ᵍ⁄2. Which convention
    `quantile()` uses is decided at 0.1.0-5 (oracle parity
    with `hdrhistogram`'s upper-bound convention weighs in).
- g=7/u32 is parity with `hdrhistogram` at 2 significant
  figures; g=10 approximates 3.
- We think g=4..6 is the monitoring sweet spot: error is on
  the *value* axis, and flagging a p99.9 that moved 10× does
  not need 2-sigfig fidelity.
- Fleet math: 100 probes at g=6/n=40/u32 ≈ 900 KB per
  process; double it if a snapshot model later
  double-buffers each probe.

## Readout requirements (band tables)

The consumer-side bar is iiac-perf's band table: z/p/n quantile
bands, each showing first/last/range/count/mean, plus overall
and quantile-trimmed mean/stdev. As of 0.1.3 that bar is met
*in this crate* rather than left to consumers, because the
duplication the original decision meant to avoid turned out to
run the other way: the same accumulate-then-render loop existed
four times (h2demo plus iiac-perf's `harness.rs`,
`band_table.rs`, `probe.rs`), none shared. One `no_std`
implementation at the bottom of the stack is what removes it.

As built (0.1.3): `bands` (Ladder / Boundary, integer-rational
fences; `BandAssign` with the `RankSplit` and `MidRank`
conventions, self-named via `name()`), `stats` (rank-window
count/mean/variance, two-pass), `table` (`BandTable`, sized at
compile time from a `const Ladder`), and the `std`-gated
`report` / `numfmt` render modules. The trim anchor is the n2
fence. The demo is the integration proof.

Everything derives from the bucket iterator:

- first/last/range/count — band's first/last non-empty bucket
  bounds and summed counts; bands are quantile fences, so the
  iteration API must make **cumulative** counts easy.
- mean/stdev (overall, per-band, trimmed) — bucket-midpoint
  weighted, accurate to rel. err; original HdrHistogram
  computes them the same way.
  - `core` has no `sqrt`, so the structure carries
    **variance** and `stdev()` is offered where a `sqrt`
    exists (`std` today; a `libm` feature stays open). The
    two differ by a square root, so nothing is lost.
  - Variance is accumulated two-pass as `(value - mean)²`,
    not as `sumsq/n - mean²` — the latter cancels badly when
    the mean is large relative to the spread, which is the
    latency case. Both passes are off the hot path.
- exact overall mean would need a running `sum` at record
  time (one u64 add) — a hot-path candidate decided by bench
  at 0.1.0-8, alongside exact min/max.
- calibration adjustment (iiac-perf's "adjusted" column) is
  presentation-layer; stays in the consumer.

## Validation

- Exhaustive small-config tests: for tiny (g, n), walk every
  value in 0..2ⁿ, assert indices are monotone and bucket
  ranges exactly partition the domain.
- Index-parity oracle: iopsystems `histogram` uses the same
  h2 scheme, so `index_for` must match exactly.
- Quantile-parity oracle: `hdrhistogram` 7 on identical
  random streams, agreeing within equivalent-value tolerance
  (the g↔sigfig mapping is approximate).
- `no_std` proof: a check script building
  `--no-default-features` for a bare-metal target.
- Record-path bench: ns/record vs a raw array store and vs
  `hdrhistogram`. Outcome (0.1.0-8): 2.6 ns/record vs
  hdrhistogram's 4.9; exact min/max/sum would add ~28%, so
  they stay **off** the hot path (bench numbers and decision
  in chores-01).

## Deferred directions

Kept open, not designed (see TODO.md):

- Buffer-swap servicing: per-probe 1-bit "needs service soon"
  signal; a background task swaps a fresh buffer for the full
  one. Enabled by the config/storage split.
- Interval snapshot / reset; atomic concurrent recording
  (`portable-atomic`); HdrHistogram V2 wire format; `alloc`
  feature (Vec-backed storage, auto-resize).

## See also

- [`README.md`](README.md) — user-facing overview and goals.
- [`AGENTS.md`](AGENTS.md) — bot workflow, versioning,
  commit/push conventions, code conventions.
- [`TODO.md`](TODO.md) — live task list and the 0.1.0 ladder.
- [`notes/chores/`](notes/chores) — chores-*.md files contain
  discussion and notes on various chores in github compatible
  markdown.
