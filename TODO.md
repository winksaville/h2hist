# Todo

This file uses [Prose form](AGENTS.md#prose-form). It
contains near term tasks with a short description and
uses links or reference links for more details.

## In Progress

When a `## Todo` item is picked up, its text moves here: the
problem overview and its list of things to do. That is followed
by the "plan" — a bullet + sub-bullets list of the development
"ladder". Each step starts with a `[[N]]` reference slot —
replaced with the step's commit ref on first backfill, once
the step is pushed — then the version, the title, and a
`(done|current)` marker as appropriate:
   - [[N]] 0.xx.y-0 blah (current)
   - [[N]] 0.xx.y-1 blah blah
   - [[N]] 0.xx.y-2 blah blah blah
   - [[N]] 0.xx.y close-out and validation

At close-out the ladder lives on as the cycle's chores
`### As-built ladder`, where the refs are preserved
(renumbered to that file's local `# References` slots).

**feat: no_std band report modules**

The band-table capability lives in the demo, not the crate:
`FENCES`, `build_bands`, `print_table`, `commas` — ~200 of
`examples/h2demo.rs`'s 288 lines — while `src/` computes no band,
mean, or stdev. The same accumulate-then-render loop exists three
more times in `../iiac-perf` (`harness.rs`, `band_table.rs`,
`probe.rs`). Separately `SplitMix64`, the heavy-tailed stream, and
the `(7, 30)` config are copied verbatim across `tests/oracle.rs`,
`benches/record.rs`, and `examples/h2demo.rs`, so a change to one
silently diverges from the others.

Everything practical stays `no_std`: the new modules build report
*structures*; only writing them to stdout is `std`-gated. This
reverses ARCHITECTURE.md's "band-table reporting stays in tprobe"
decision — see [[5]].

- [[N]] 0.1.3-0 chore: band report cycle setup (done)
  - version-of-record, this ladder, chores section, the
    ARCHITECTURE.md reversal, and the iiac-perf adoption todo
- [[N]] 0.1.3-1 refactor: dev module for test, bench, demo
  - `dev/{mod,consts,rng,stream}.rs` behind one `#[path]`
    include per consumer; retires 3 `SplitMix64` copies, 3
    stream copies, and the scattered `(7, 30)` / seed / sigfig
    literals
  - folds in the `src/` unit-test literals: each
    `Config::new(g, n)` + hand-computed `[C; N]` pair becomes
    `const C` + `const N: usize = C.total_buckets()`
- [[N]] 0.1.3-2 feat: no_std band ladder and labels
  - `src/bands.rs`: `Boundary`/`BoundaryKind`, const ladder from
    `Z_DEPTH`/`N_DEPTH`, `BandLabels` with alloc-free label
    writing; fences as integer rationals, so no `floor`/`powi`
- [[N]] 0.1.3-3 feat: band assignment trait
  - `BandAssign` with `RankSplit` (h2demo's exact rank split)
    and `MidRank` (iiac-perf's right-closed Hazen mid-rank);
    tests pin a case where the two legitimately disagree
- [[N]] 0.1.3-4 feat: midpoint-weighted mean and variance
  - `src/stats.rs`: two-pass variance, avoiding the
    `sumsq/n - mean²` cancellation the demo has today; `stdev()`
    where a `sqrt` exists
- [[N]] 0.1.3-5 feat: band table structure
  - `src/table.rs`: fixed-capacity `BandTable` built in one pass
    over `Buckets`
- [[N]] 0.1.3-6 feat: band table rendering
  - `src/report.rs`: renders into any `core::fmt::Write` with
    two-pass width measurement; `std` adds the stdout convenience
- [[N]] 0.1.3-7 refactor: h2demo on library report path
  - demo ~288 → ~60 lines, gated on output identical to today's
    at matching ladder depths
- [[N]] 0.1.3-8 docs: band report modules
  - README, ARCHITECTURE, and the switch to the fuller z4..n10
    ladder as a visible change
- [[N]] 0.1.3 feat: no_std band report modules
  - close-out and validation

## Todo

 Entries are in **strict priority rank** — #1 highest,
 descending. Reprioritize by moving an entry, then
 `vc-x1 fix-todo --no-dry-run TODO.md` to renumber.
 The numbers are positional rank, not stable IDs — to refer
 to a Todo, name it by its **title** (a greppable mention;
 a numbered list item has no anchor to link to), not its
 number. Long-tail entries
 live in [todo-backlog.md](notes/todo-backlog.md). Use the
 [Prose Form in AGENTS.md](AGENTS.md#prose-form); deeper
 detail goes in `notes/chores/chores-NN.md` design
 subsections (link via `[N]` ref).

1. **Buffer-swap servicing model.** Today a probe has one
   histogram recorded to completion. Future: a 1-bit
   "needs service soon" signal per probe; a background task
   hands over a fresh zeroed buffer and takes the full one
   for analysis. Deferred with no hard decisions; the
   config/storage split keeps it open. See [[1]].
2. **tprobe adoption.** Replace `hdrhistogram` in tprobe's
   `examples/tp_pc`, then adopt in its core recording path.
   The integration cycle runs in tprobe's repo; this entry
   tracks API gaps it surfaces here.
3. **iiac-perf adoption.** Retire the three copies of the
   accumulate-then-render loop (`harness.rs::print_report`,
   `band_table.rs::render`, `probe.rs::report`) onto this
   crate's band/report modules; `Bucket`'s fields are `pub`,
   so iiac-perf maps `hdrhistogram::iter_recorded()` into
   `Bucket`s with no dependency change here. The integration
   cycle runs in iiac-perf's repo; this entry tracks the API
   gaps it surfaces — the `adjusted` column extension point
   first. See [[5]].

## Ideas

- Interval snapshot / reset API (monitoring cadence; makes
  u32-vs-u64 count-width pressure moot).
- Atomic concurrent recording via `portable-atomic`.
- HdrHistogram V2 wire/log format compatibility (zigzag
  LEB128 is `no_std`-fine; compression needs a dep).
- `alloc` feature: Vec-backed storage, auto-resize.

## Bugs

See [bugs.md](notes/bugs.md).

## Done

Completed tasks are moved from `## Todo` to here, `## Done`, as they are completed
and older `## Done` sections are moved to [done.md](notes/done.md) to keep this file small.

- feat: no_std h2 histogram core [[2]]
- chore: rename crate to h2hist [[3]]
- perf: record path inlining, read-time total [[4]]

# References

[1]: notes/chores/chores-01.md#deferred-buffer-swap-servicing-model
[2]: notes/chores/chores-01.md#feat-no_std-h2-histogram-core
[3]: notes/chores/chores-01.md#chore-rename-crate-to-h2hist
[4]: notes/chores/chores-01.md#perf-record-path-inlining-read-time-total
[5]: notes/chores/chores-01.md#feat-no_std-band-report-modules
