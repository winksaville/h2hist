# h2hist

A `no_std`, no-alloc implementation of iopsystems'
[h2 histogram](https://h2histogram.org/) — their base-2
redesign of [HdrHistogram](http://hdrhistogram.org/)'s
log-linear bucketing — for continuous monitoring of latency,
jitter, and real-time lateness. Built to be the core recording
structure of [tprobe](../tprobe), but standalone and generally
usable; see
[Relation to iopsystems' histogram](#relation-to-iopsystems-histogram)
for how it differs from their crate.

Status: 0.1.3 (see [TODO.md](TODO.md)); core, analysis,
oracle tests, demo, no_std check, and the comparison bench
have landed; record path tuned (see [Bench](#bench)); band
report modules landed (see
[Band report modules](#band-report-modules)).

## Build and test

- `cargo build` — host build (std feature on by default).
- `cargo build --no-default-features` — the `no_std`
  configuration.
- `cargo test` — unit tests, doctests, and the oracle parity
  suite (dev-deps on `hdrhistogram` and iopsystems
  `histogram` as correctness oracles).

## Demo

`examples/h2demo.rs` records 1M synthetic latency ticks and
prints an iiac-perf-style band table (z/p/n quantile bands
with first/last/range/count/mean, overall and tail-trimmed
mean/stdev), built entirely on the library's band report
modules — assemble a `BandTable` over the z4/n10 ladder with
the `RankSplit` convention (named in the title line), render
with `render_band_table`:

- `cargo run --example h2demo` — run in place.
- `cargo install --path . --example h2demo` — install the
  demo as a `h2demo` binary.

## Band report modules

The band-table capability lives in the crate, split along the
device/service line: `no_std` modules build report
*structures* (what a device would ship over a wire), and the
`std`-gated modules render them as text.

- `bands` (`no_std`) — the z/p/n boundary [`Ladder`] generated
  from two tail depths; fences are exact integer rationals.
  Band assignment via the `BandAssign` trait with two
  conventions (this crate's coinages, so reports name the one
  in use):
  - `RankSplit` — a bucket's rank span splits exactly across
    the fences it crosses; band counts are exact.
  - `MidRank` — whole bucket to its Hazen mid-rank band,
    right-closed (iiac-perf's convention); value edges stay
    disjoint, counts approximate.
- `stats` (`no_std`) — midpoint-weighted count/mean/variance
  over a rank window; two-pass variance (no cancellation);
  `stdev()` under `std` (`core` has no `sqrt`).
- `table` (`no_std`) — `BandTable`: bands + overall + trimmed
  stats in one fixed-capacity structure, sized at compile time
  from a `const Ladder`.
- `report`, `numfmt` (`std`) — `render_band_table` with
  measured or fixed column widths, boundary labels
  (zpn / fraction / both), comma-grouped number formatting.

## no_std check

- `./scripts/check-no-std.sh` — builds the core
  (`--no-default-features`) for every installed bare-metal
  (`*-none-*`) target.

## Bench

`cargo bench --bench record` times the record path over a
pre-generated heavy-tailed stream (8M samples, `g=7 n=30`,
3072 buckets) and reports best-of-3 ns/record per variant
(hand-rolled harness, no criterion). Indicative output
(AMD Ryzen 9 3900X):

```text
variant                stored       ns/record
raw streaming store    u64 sample       0.931
h2hist u32             u32 counter      0.881
h2hist u32 + total     u32 counter      0.980
h2hist u64             u64 counter      0.863
index_for + wrap u32   u32 counter      0.811
h2hist u32 + extras    u32 counter      1.305
histogram u64          u64 counter      1.613
hdrhistogram 2sf       u64 counter      4.965
```

The `stored` column names what the variant writes per
record. On the x86_64 host the width is near-invisible, but
it is the number that matters on a 32-bit target (a u64
store/increment is two words on a Cortex-M, and the counts
array doubles) — these host numbers do not transfer to
embedded CPUs; measure on target.

The rows and how to read them:

- `raw streaming store` — the "keep every raw sample"
  alternative: each sample written to the next slot of a
  preallocated `Vec<u64>`, no bucketing. Not a lower bound:
  it streams `8 B × len` through DRAM while a histogram's
  counts stay L1-resident, so the histogram rows can (and
  do) beat it — and it costs 8 B per sample where the
  histogram costs zero.
- `h2hist u32` — this crate as shipped: index + saturating
  u32 bucket increment, nothing else (totals are summed at
  read time, off the hot path).
- `h2hist u32 + total` — diagnostic, not an API: the same
  plus a caller-side running `saturating_add(1)` per record.
  Documents the measured cost of the retired always-on total
  and prices the keep-your-own-counter pattern for callers
  who want a live total.
- `h2hist u64` — counter-width check: u64 counters bench the
  same as u32, so u32 stays the default on footprint alone.
- `index_for + wrap u32` — diagnostic, not an API:
  `Config::index_for` plus a plain wrapping add on a bare
  slice — the index math alone. Matching `h2hist u32` shows
  the saturating counter update costs nothing measurable.
- `h2hist u32 + extras` — the same plus inline min/max/sum
  tracking: the data for the parked hot-path-extras
  decision.
- `histogram u64` — iopsystems `histogram`, the same h2
  scheme (the peer comparison; u64 heap-allocated counts).
- `hdrhistogram 2sf` — the reference implementation at 2
  significant figures (relative error ≤ 1%, close to g=7's
  ≤ 0.78%), so the comparison runs at like precision.

Caveats: a tight-loop microbench. Code-layout shifts between
recompiles move individual rows by ~±0.2 ns, so compare rows
within one run, not across binaries; treat the numbers as
indicative, not rigorous. History and the resulting design
decisions are in [chores-01](notes/chores/chores-01.md).

## Goals

- **O(1) record path** — clz + shift + saturating counter
  increment; no floats, no allocation, no panics, no
  data-dependent work. A recorded sample costs zero bytes;
  the footprint is fixed at creation.
- **Per-instance sizing** — each histogram picks its own
  `(grouping_power, max_value_power, count type)`; footprints
  range from ~250 B (coarse embedded) to ~250 KB (3-sigfig
  benchmarking), with u32 counts the default. See the
  [size-tradeoff table](ARCHITECTURE.md#size-tradeoffs).
- **Caller-supplied storage** — the core borrows its counts
  slice, keeping a future buffer-swap servicing model open
  (a background task exchanging a fresh buffer for a full
  one) without API changes.
- **`no_std` core, `std` rendering** — analysis (quantiles,
  merge, iteration) and the report structures stay
  `no_std`-capable but off the hot path; turning structures
  into text (`report`, `numfmt`) is `std`-gated.

## Initial design

The [h2](https://h2histogram.org/) parameterization
(`grouping_power`, `max_value_power`)
restates HdrHistogram's log-linear bucketing with cleaner
integer math: relative error ≤ 2⁻ᵍ, max value 2ⁿ−1, and
`buckets = (n−g+1)·2ᵍ`. Correctness is proven against two
oracles — iopsystems `histogram` (exact index parity, same
scheme) and `hdrhistogram` 7 (quantile parity within
equivalent-value tolerance). Full design, API sketch, and
tradeoffs: [ARCHITECTURE.md](ARCHITECTURE.md).

## Relation to iopsystems' `histogram`

The bucketing scheme is theirs: [h2](https://h2histogram.org/),
a base-2 redesign of HdrHistogram, and their
[histogram](https://github.com/iopsystems/histogram) crate is
one of this crate's two correctness oracles. The two differ in
scope, not quality:

- **iopsystems `histogram`** — std, heap-allocated counts
  (`Box<[Count]>`), atomic / sparse / cumulative-read-only
  variants, serde support; built for services and telemetry
  pipelines. Dependencies: `thiserror` (mandatory),
  `serde` / `schemars` (optional).
- **this crate** — `no_std`, no-alloc core with
  caller-supplied storage, u8–u64 saturating counters,
  clamp-on-over-range record path, zero dependencies; built
  for embedded and real-time recording where heap allocation
  and per-record `Result`s are unaffordable.

Pick theirs on a host with std; pick this one when the
histogram must live in a static buffer.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.
