# Status

STATE: milestone=M7 phase=T task=- state=idle base=f9209e6 updated=2026-11-19

Read this file first; the two commands print the state and one task, `AGENTS.md` §1.1 orders a broken plan.

## Where we are

The carry keeps a hand of its own: the drawn square is what carries a piece, so a
camera gesture can hide it and put it back on the piece without touching the
window's pointer (ADR 0156, X69).

The nomenclature directory is edited by a window of its own: `z3l directory
[<folder>]` opens the item tree, the record's card and the registry view, and a
save is one record plus the package's revision — written behind one lock, record
first and the manifest last, so the editor re-derives the panels that name it at
its next frame boundary (O6, ADR 0158, `nomenclature.md` §12–§14).

A package travels as its folder: an export copies every file of it and its
manifest last, an import lands the copy, checks it with the loader's own rows
beside what is already connected, and writes the connected list last, so a
window sees one change once the whole package is in place. Two folders that
claim one number, or a manifest `requires` the set cannot meet, are refused with
both packages named (O7).

## Next

Nothing is scheduled: `## Now` is empty and the backlog keeps the work. X73 is next — a hidden part
is still in the way of the tools — and it enters the order with `cargo xtask task take X73`.

## Index

| File | Holds | Read when |
|---|---|---|
| `plan/BACKLOG.md` | one line per task, ids, order, file hints | always |
| `plan/SESSION.md` | the live task: steps, blocker, next command | `state=doing` |
| `plan/debt.md`, `plan/questions.md` | accepted gaps; decisions for the owner | a task touches that area |
| `plan/rename.md` | the pre-release rename: inventory, procedure, gotchas | task R1, or a change that adds a place the product name appears |
| `plan/refactor.md` | design rules, subtask template, order (S7, S8–S22) | any S subtask |
| `plan/archive/done.md` | history up to schema 11 | never, `git log` replaces it |
