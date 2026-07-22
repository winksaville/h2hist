# histogram-no-std

A `no_std`, no-alloc implementation of iopsystems'
[h2 histogram](https://h2histogram.org/) — their base-2
redesign of [HdrHistogram](http://hdrhistogram.org/)'s
log-linear bucketing — for continuous monitoring of latency,
jitter, and real-time lateness. Built to be the core recording
structure of [tprobe](../tprobe), but standalone and generally
usable; see
[Relation to iopsystems' histogram](#relation-to-iopsystems-histogram)
for how it differs from their crate.

Status: 0.1.0 cycle in progress (see [TODO.md](TODO.md));
core, analysis, oracle tests, demo, no_std check, and bench
have landed.

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
mean/stdev):

- `cargo run --example h2demo` — run in place.
- `cargo install --path . --example h2demo` — install the
  demo as a `h2demo` binary.

## no_std check and bench

- `./scripts/check-no-std.sh` — builds the core
  (`--no-default-features`) for every installed bare-metal
  (`*-none-*`) target.
- `cargo bench` — record-path cost vs a raw store and
  `hdrhistogram` (hand-rolled harness; indicative numbers
  and the resulting design decision are in
  [chores-01](notes/chores/chores-01.md)).

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
- **`no_std` core, `std` convenience** — analysis (quantiles,
  merge, iteration) stays `no_std`-capable but off the hot
  path; anything needing std is feature-gated.

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
