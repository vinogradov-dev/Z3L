# Status

STATE: milestone=M7 phase=T task=- state=idle base=7af290a updated=2026-11-20
STREAMS: agent=J catalog=O prototypes=Q

Read this file first; the two commands print the state and one task, `AGENTS.md` §1.1 orders a broken plan.

## Where we are

The carry keeps a hand of its own: the drawn square is what carries a piece, so a camera gesture can
hide it and put it back on the piece without touching the window's pointer (ADR 0156, X69).

The command line is the one headless door into the document: `z3l commands` lists every id, `z3l
run` dispatches a JSON Lines recipe through the session's one edit scope and writes once, all or
nothing, and `z3l query` prints a JSON report that reuses the derivation which already owns the
answer — the node walk, the published order tables, the panel diagnostics and the measured niche
(J1, ADR 0183).

The nomenclature directory is edited by a window of its own: `z3l directory [<folder>]` opens the
item tree, the record's card and the registry view, and a save is one record plus the package's
revision — written behind one lock, record first and the manifest last, so the editor re-derives the
panels that name it at its next frame boundary (O6, ADR 0158, `nomenclature.md` §12–§14).

A package travels as its folder: an export copies every file of it and its manifest last, an import
lands the copy, checks it with the loader's own rows beside what is already connected, and writes
the connected list last, so a window sees one change once the whole package is in place. Two folders
that claim one number, or a manifest `requires` the set cannot meet, are refused with both packages
named (O7).

The plan shows a stream: `plan/STATUS.md` declares the three checkouts beside the plan's own, each
working one `[~]` entry of its phases and keeping its own session note while `STATE:` stays the
plan's own, so four worktrees no longer hide behind an idle line (P17, ADR 0176).

## Next

Nothing is scheduled: the contact port answers a real depth for a crossed pair, so the carry and the
turn stop at a neighbour however far the pointer jumped

## Index

| File | Holds | Read when |
|---|---|---|
| `plan/BACKLOG.md` | one line per task, ids, order, file hints | always |
| `plan/SESSION.md` | the live task: steps, blocker, next command | `state=doing` |
| `plan/debt.md`, `plan/questions.md` | accepted gaps; decisions for the owner | a task touches that area |
| `plan/rename.md` | the pre-release rename: inventory, procedure, gotchas | task R1, or a change that adds a place the product name appears |
| `plan/refactor.md` | design rules, subtask template, order (S7, S8–S22) | any S subtask |
| `plan/archive/done.md` | history up to schema 11 | never, `git log` replaces it |
