# histogram-no-std

A `no_std`, no-alloc, HdrHistogram-style log-linear histogram
for continuous monitoring of latency, jitter, and real-time
lateness — built to be the core recording structure of
[tprobe](../tprobe), but standalone and generally usable.

Status: scaffolded, pre-code — the 0.1.0 cycle is in progress
(see [TODO.md](TODO.md)). Build / test / bench / run sections
will be added here as the ladder steps that make them true
land.

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

The h2 parameterization (`grouping_power`, `max_value_power`)
restates HdrHistogram's log-linear bucketing with cleaner
integer math: relative error ≤ 2⁻ᵍ, max value 2ⁿ−1, and
`buckets = (n−g+1)·2ᵍ`. Correctness is proven against two
oracles — iopsystems `histogram` (exact index parity, same
scheme) and `hdrhistogram` 7 (quantile parity within
equivalent-value tolerance). Full design, API sketch, and
tradeoffs: [ARCHITECTURE.md](ARCHITECTURE.md).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.
