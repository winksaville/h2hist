# Chores-01

Chores-XX files use [Prose form](../../AGENTS.md#prose-form). They
contain discussions and notes on various chores in github compatible
markdown. There is also a [TODO.md](../../TODO.md) file that tracks
tasks and in general there should be a chore section for each task
with the why and how this task will be completed.

Each task's `##` header is its commit title (see
[Conventional-commit shape](../../AGENTS.md#conventional-commit-shape-ladder--chores--commit)),
with a `Commits:` line first under it — empty until backfilled —
then the narrative: conceptual bullets, never a per-file edit
list (see
[Chores conventions](../../AGENTS.md#chores-conventions)).

## feat: no_std h2 histogram core

Commits:

A `no_std`, no-alloc, HdrHistogram-style log-linear histogram
crate, to become tprobe's core recording structure. tprobe's
chores-01 (its 0.1.0-8 decision) surveyed crates.io and found
no fit — `hdrhistogram` 7.5.4 is std-only, iopsystems
`histogram` is heap-backed, and the `no_std` options have the
wrong bucketing — and decided to hand-roll; this repo is that
hand-roll promoted to its own crate. Cycle runs on branch
`0.1.0-no-std-hdrhistogram` with a non-ff merge close-out
(manual jj steps until vc-x1 grows the capability, after its
current refactoring).

### Founding discussion (2026-07-21)

The founding conversation (feasibility → sizing → plan) landed
these decisions; sizing detail and the API sketch live in
[ARCHITECTURE.md](../../ARCHITECTURE.md), not restated here.

- Feasibility: the record path is clz + shift + increment —
  integer-only, O(1), trivially `no_std`. The std-bound parts
  of existing implementations (dynamic sizing, auto-resize,
  compressed wire format, f64 conveniences) are separable and
  deferred or feature-gated.
- h2 parameterization `(grouping_power, max_value_power)`
  over the original `(lowest, highest, sigfigs)` — same
  bucketing, cleaner integer math, `const fn`-friendly
  bucket-count formula. Config chosen per probe instance.
- Config / counts-storage split, core borrowing
  `&mut [Count]` — the load-bearing choice; keeps the
  deferred buffer-swap model open without reshaping the type.
- u32 saturating counts as default. We think monitoring
  bounds per-histogram totals (run length now, snapshot
  interval later), so u64 is reserved for run-forever
  accumulation.
- Lifecycle for 0.1.0: one histogram per probe,
  run-to-completion, analysis at the end. No snapshot /
  reset / swap API yet.
- Validation via two oracles: iopsystems `histogram` for
  exact index parity (same scheme), `hdrhistogram` 7 for
  quantile parity within equivalent-value tolerance; plus
  exhaustive small-config walks and a `no_std` target build.
- Demo ships as `examples/h2demo.rs`, installable via
  `cargo install --path . --example h2demo` (cargo installs
  example targets with `--example`).
- Readout bar: iiac-perf's band table (z/p/n bands,
  first/last/range/count/mean, trimmed mean/stdev) must be
  derivable from the bucket iterator — drives cumulative
  counts in the iteration API; `h2demo` prints that table.
- Parked open questions: hot-path extras — exact min/max and
  running sum for exact mean — decided on bench data at
  0.1.0-8; f64-vs-integer quantile input (decide at
  0.1.0-5); crate rename before any publish.

### Sizing analysis

A sample costs zero bytes — footprint is fixed at creation:
`bytes = (n−g+1)·2ᵍ·sizeof(C)`, ~250 B to ~250 KB across
realistic configs (table in
[ARCHITECTURE.md](../../ARCHITECTURE.md#size-tradeoffs)).
We think g=4..6 is the monitoring sweet spot — error is on
the value axis, and flagging a p99.9 that moved 10× doesn't
need 2-sigfig fidelity — putting a 100-probe process near
1 MB, noise on a host and shrinkable for embedded.

### Deferred: buffer-swap servicing model

The intended growth path once run-to-completion is limiting:
each probe raises a 1-bit "needs service soon" signal (a
watermark on total count or a near-saturated bucket); a
background task hands the probe a fresh zeroed counts buffer
and takes the full one for analysis. No hard decisions made —
the config/storage split is the only accommodation. Watermark
semantics, signal transport, and hand-off protocol are all
open; tracked as the "Buffer-swap servicing model" Todo.

### As-built ladder

- [[N]] 0.1.0-0 chore: h2 histogram plan capture — the
  founding-conversation capture: this section, README goals,
  ARCHITECTURE + size table, TODO ladder;
  version.toml → 0.1.0-0.
- [[N]] 0.1.0-1 chore: scaffold h2 histogram crate —
  CargoRust.toml renamed to Cargo.toml (name
  `histogram-no-std`, `std` feature, version-of-record moves
  here at 0.1.0-1); version.toml retired; src/lib.rs
  skeleton with module doc + `no_std` gate.
- [[N]] 0.1.0-2 feat: h2 histogram config and index math —
  `Config` (validated powers, `const fn` bucket count /
  value→index / index→range), `Error`; exhaustive
  small-config walk proves indices monotone and ranges an
  exact partition; over-range clamps to top bucket, n=64
  edge covered.

# References
