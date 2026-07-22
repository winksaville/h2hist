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

## Done

Completed tasks are moved from `## Todo` to here, `## Done`, as they are completed
and older `## Done` sections are moved to [done.md](notes/done.md) to keep this file small.

# References
