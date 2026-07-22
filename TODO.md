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

**feat: no_std h2 histogram core**

A `no_std`, no-alloc, HdrHistogram-style log-linear histogram
crate for [tprobe](../tprobe)-class continuous monitoring.
tprobe's 0.1.0-8 survey found no existing crate fits
(`hdrhistogram` is std-only; the `no_std` options have the
wrong bucketing), so this crate is that hand-roll, promoted to
its own repo. Core: O(1) integer-only record path over
caller-supplied storage; analysis off the hot path; u32
saturating counts by default. Design detail in
[ARCHITECTURE.md](ARCHITECTURE.md); founding discussion in
[chores-01](notes/chores/chores-01.md#feat-no_std-h2-histogram-core).

Cycle branch `0.1.0-no-std-hdrhistogram`; close-out is a
non-ff merge into `main` (manual jj steps for now — see the
[Merge non-ff recipe](notes/cycle-protocol.md#merge-non-ff-recipe)).

   - [[N]] 0.1.0-0 chore: h2 histogram plan capture (done)
     - capture the founding conversation: chores-01 section,
       README goals, ARCHITECTURE + size table, this ladder
     - version.toml → 0.1.0-0
   - [[N]] 0.1.0-1 chore: scaffold h2 histogram crate (done)
     - Cargo.toml from CargoRust.toml seed; version-of-record
       moves to Cargo.toml, version.toml retires
     - lib.rs skeleton, `std` feature
       (`#![cfg_attr(not(feature = "std"), no_std)]`)
   - [[N]] 0.1.0-2 feat: h2 histogram config and index math (done)
     - `Config { grouping_power, max_value_power }` +
       validation
     - `total_buckets` / `index_for` / `value_range` as
       `const fn`
     - exhaustive small-config tests (walk every value,
       assert monotone indices, ranges partition domain)
   - [[N]] 0.1.0-3 feat: h2 histogram record path (done)
     - `Counter` trait (u8/u16/u32/u64, default u32,
       saturating add)
     - borrowed-storage `Histogram`, `record` / `record_n`,
       top-bucket clamp for over-range values
   - [[N]] 0.1.0-4 feat: h2 histogram owned-array wrapper (done)
     - const-generic `HistogramArray<N, C>` with size check
       against `Config::total_buckets()`
   - [[N]] 0.1.0-5 feat: h2 histogram quantile merge and iter (done)
     - `quantile`, `merge_from`, bucket iterator (range,
       count); decide f64-vs-integer quantile input here
   - [[N]] 0.1.0-6 test: h2 histogram oracle parity suite (done)
     - dev-dep iopsystems `histogram`: exact index parity
     - dev-dep `hdrhistogram` 7: quantile parity within
       equivalent-value tolerance, randomized streams
   - [[N]] 0.1.0-7 feat: h2 histogram demo example
     - `examples/h2demo.rs`: record synthetic latency stream,
       print an iiac-perf-style band table (z/p/n bands with
       first/last/range/count/mean, trimmed mean/stdev)
     - installable: `cargo install --path . --example h2demo`
   - [[N]] 0.1.0-8 chore: h2 histogram no_std check and bench
     - script building `--no-default-features` for a bare
       target (e.g. `thumbv7em-none-eabihf`)
     - record-path bench vs raw array store and
       `hdrhistogram`; decide on data which extras stay on
       the hot path: exact min/max, running sum (exact mean)
   - [[N]] 0.1.0 feat: no_std h2 histogram core
     - close-out bookkeeping; non-ff merge into `main`

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

## Ideas

- Interval snapshot / reset API (monitoring cadence; makes
  u32-vs-u64 count-width pressure moot).
- Atomic concurrent recording via `portable-atomic`.
- HdrHistogram V2 wire/log format compatibility (zigzag
  LEB128 is `no_std`-fine; compression needs a dep).
- `alloc` feature: Vec-backed storage, auto-resize.
- Crate rename before any publish (`publish = false` today,
  so the name only competes locally).

## Bugs

See [bugs.md](notes/bugs.md).

## Done

Completed tasks are moved from `## Todo` to here, `## Done`, as they are completed
and older `## Done` sections are moved to [done.md](notes/done.md) to keep this file small.

# References

[1]: notes/chores/chores-01.md#deferred-buffer-swap-servicing-model
