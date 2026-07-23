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

Commits: [[10]]

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

### Record-cost bench and the hot-path-extras decision

`benches/record.rs` (hand-rolled, harness=false; g=7 n=30,
8M-value heavy-tailed stream, best of 3, 3900X, unpinned —
indicative, not a lab record):

- raw streaming store 0.87 ns/record
- histogram u32 2.57 ns/record
- histogram u32 + min/max/sum extras 3.29 ns/record
- hdrhistogram (2 sigfig) 4.95 ns/record

**Decision: extras stay off the hot path.** Exact min/max and
running sum would cost ~+28% per record; band tables already
get bucket-resolution first/last and midpoint means from the
iterator (h2demo proves the readout), so the default record
path keeps only the bucket increment + total. Revisit only if
a consumer needs exact mean/min/max — that would be an opt-in
wrapper, not a change to `record`. We think the histogram-vs-
raw-store gap (2.6 vs 0.9 ns) overstates the field cost: the
raw store here is cache-warm and the histogram price buys a
fixed footprint, per tprobe's round-3 verdict.

### As-built ladder

- [[1]] 0.1.0-0 chore: h2 histogram plan capture — the
  founding-conversation capture: this section, README goals,
  ARCHITECTURE + size table, TODO ladder;
  version.toml → 0.1.0-0.
- [[2]] 0.1.0-1 chore: scaffold h2 histogram crate —
  CargoRust.toml renamed to Cargo.toml (name
  `histogram-no-std`, `std` feature, version-of-record moves
  here at 0.1.0-1); version.toml retired; src/lib.rs
  skeleton with module doc + `no_std` gate.
- [[3]] 0.1.0-2 feat: h2 histogram config and index math —
  `Config` (validated powers, `const fn` bucket count /
  value→index / index→range), `Error`; exhaustive
  small-config walk proves indices monotone and ranges an
  exact partition; over-range clamps to top bucket, n=64
  edge covered.
- [[4]] 0.1.0-3 feat: h2 histogram record path — `Counter`
  trait (u8..u64, saturating, u32 default) and the
  borrowed-storage `Histogram`:
  - `new` checks storage length and *recomputes* `total`
    from the counts, so a slice handed back by a future
    swap servicer rebinds consistently;
  - `record`/`record_n` never panic (`get_mut` guarded by
    the tested index invariant); `into_counts` releases the
    borrow — the swap hand-off shape.
- [[5]] 0.1.0-4 feat: h2 histogram owned-array wrapper —
  `HistogramArray<N, C>` owns `[C; N]`; N is explicit
  (stable Rust can't derive it from Config powers) and
  checked in `new`; record path shared with the borrowed
  type via `record_into`; `as_histogram()` gives one
  analysis surface for both storage shapes (carries the
  crate's first non-test `unwrap`, `// OK:` justified —
  constructor-checked invariant).
- [[6]] 0.1.0-5 feat: h2 histogram quantile merge and iter —
  `analysis` module shared by both storage shapes: `Bucket` /
  `Buckets` iterator carrying **cumulative** counts (the
  band-table requirement), `quantile` (upper-bound
  convention — hdrhistogram's highest-equivalent — chosen
  for oracle parity; integer ceil, no std float intrinsics),
  `merge_from` (saturating, `ConfigMismatch` on unequal
  configs). Decision recorded: f64 quantile input kept.
- [[7]] 0.1.0-6 test: h2 histogram oracle parity suite —
  tests/oracle.rs with seeded splitmix64 streams (no rand
  dep). vs iopsystems `histogram`: **identical counts
  arrays** on uniform streams over four configs plus
  power-of-two boundary walk — index math is bit-identical.
  vs `hdrhistogram` (2 sigfig): quantiles p0..p100 on a
  200k heavy-tailed stream within combined tolerance
  (2⁻ᵍ + 1%); totals equal.
- [[8]] 0.1.0-7 feat: h2 histogram demo example —
  examples/h2demo.rs (drafted by a Sonnet subagent to a
  fixed spec, reviewed): 1M-sample synthetic stream, band
  table built in ONE `buckets()` pass off the cumulative
  field (fences never call `quantile()`); band `last`s
  cross-check `quantile()` exactly. Installable via
  `cargo install --path . --example h2demo`. README gains
  Build-and-test + Demo sections.
- [[9]] 0.1.0-8 chore: h2 histogram no_std check and bench —
  scripts/check-no-std.sh builds the core for every
  installed `*-none-*` target (passes on riscv32imac + four
  thumb targets); benches/record.rs (hand-rolled,
  harness=false) measures the record path; numbers and the
  extras-off decision in the design subsection above.
  README gains the no_std-check + bench section.
- [[10]] 0.1.0 feat: no_std h2 histogram core — close-out
  bookkeeping (Done entry, chores finalization, version →
  0.1.0, README/ARCHITECTURE credit docs); non-ff merge
  into `main`.

### Outcome

Cycle complete: the 0.1.0 core landed with zero runtime
dependencies, proven against both oracles, demoed, no_std-
checked on five bare-metal targets, and benched (extras kept
off the hot path).

- Close-out docs give the scheme's authors first billing:
  README opens as an implementation of iopsystems'
  h2 histogram ([h2histogram.org](https://h2histogram.org/)),
  links [hdrhistogram.org](http://hdrhistogram.org/), and
  gains a "Relation to iopsystems' `histogram`" section —
  differently-scoped, not better: theirs std/heap for
  services, ours no_std/borrowed-storage for embedded.
  ARCHITECTURE links the h2 write-up beside the crate link.
- Crate rename to `h2histogram-no-std` decided (algorithm
  name, like `sha2`; both names unclaimed on crates.io as of
  2026-07-22). Runs as its own cycle next — see the TODO
  entry "Crate rename to h2histogram-no-std".

## chore: rename crate to h2hist

Commits:

Rename `histogram-no-std` → `h2hist` (crate, GitHub repo,
local dir) and the bot repo `histogram-no-std.claude` →
`h2hist.claude`. The old name claims a generic space and frames
`no_std` — a capability the storage model enables — as the
crate's identity. The name decision walked several candidates:

- `h2histogram-no-std` — accurate lineage, but `-no-std`
  frames a capability as the identity.
- Bare `h2histogram` — rejected as presumptuous: it is the
  algorithm's published identity
  ([h2histogram.org](https://h2histogram.org/)), left
  unclaimed.
- `h2h-*`, `h2gram`, `h2histocore`, `h2datadist` — rejected
  for losing the searchable `hist`/`histogram` stem or
  reading as noise.
- `h2hist` — chosen: h2 lineage + the universally understood
  `hist` abbreviation, and hyphen-free so the crate name is
  the import path (`use h2hist::Histogram`). Unclaimed on
  crates.io as of 2026-07-22 (as were all candidates;
  bare `hist` is taken).

On "frequency distribution" (a considered synonym): in
statistics *frequency* means occurrence count, so the term is
technically what a histogram stores — but in a systems/latency
context "frequency" reads as Hz, so the histogram stem stays.

A single-commit cycle (bare 0.1.1, Preparation omitted): the
crate/package rename, self-reference sweep, GitHub repo
rename's URL sweep, the 0.1.0 backfill ([[10]]), and this
section land together. Outside the commit, as env steps: the
GitHub renames (`h2hist`, `h2hist.claude` — the latter delayed
by a GitHub API outage during the cycle), both repos' remote
URL updates (`.git/config` is sandbox-protected), the local
dir rename, and the bot-repo symlink re-key
(`~/.claude/projects/…`) — then a fresh session under the new
path key. Old GitHub URLs redirect, so recorded commit URLs
and pushes survive the transition.

# References

[1]: https://github.com/winksaville/h2hist/commit/45901cdb0b70 "45901cdb0b70a02f2dd32b03c78ddcb59d25293f"
[2]: https://github.com/winksaville/h2hist/commit/2f4c05cb1e38 "2f4c05cb1e38b0eff68454bae84392c8c86485fc"
[3]: https://github.com/winksaville/h2hist/commit/da442fcfc9cf "da442fcfc9cf7a950bb6b9430bb3b86fe158b457"
[4]: https://github.com/winksaville/h2hist/commit/ba474051f812 "ba474051f812c143b514cf348644483e329f9b1b"
[5]: https://github.com/winksaville/h2hist/commit/dcfa6c1f3271 "dcfa6c1f32713bb09c4efee062528945c5c5975b"
[6]: https://github.com/winksaville/h2hist/commit/ea4623857ee4 "ea4623857ee46f46d6bf4c5919d3925c83080482"
[7]: https://github.com/winksaville/h2hist/commit/ee3d537035ac "ee3d537035ac14923ed12e73cfce97ad693cf045"
[8]: https://github.com/winksaville/h2hist/commit/f950c382ef40 "f950c382ef40f313cea6c95cb282be4ae5f6fd16"
[9]: https://github.com/winksaville/h2hist/commit/d7ac06b84ad9 "d7ac06b84ad9f06285d0db6459ec2496ef507dd6"
[10]: https://github.com/winksaville/h2hist/commit/18f2b9f10aee "18f2b9f10aee585b0d7a52180725db799dc1bdc4"
