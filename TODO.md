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

_No cycle currently in progress._

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
   first. See [[2]].

## Ideas

- Interval snapshot / reset API (monitoring cadence; makes
  u32-vs-u64 count-width pressure moot).
- Atomic concurrent recording via `portable-atomic`.
- HdrHistogram V2 wire/log format compatibility (zigzag
  LEB128 is `no_std`-fine; compression needs a dep).
- `alloc` feature: Vec-backed storage, auto-resize.
- Terminal histogram / bar-chart renderer over
  `Buckets`/`BandTable` (a graph, not a table); count-axis
  scaling (log or max-normalized) and a CDF percentile-plot
  variant TBD.

## Bugs

See [bugs.md](notes/bugs.md).

## Done

Completed tasks are moved from `## Todo` to here, `## Done`, as they are completed
and older `## Done` sections are moved to [done.md](notes/done.md) to keep this file small.

- feat: no_std band report modules [[2]]

# References

[1]: notes/chores/chores-01.md#deferred-buffer-swap-servicing-model
[2]: notes/chores/chores-01.md#feat-no_std-band-report-modules
