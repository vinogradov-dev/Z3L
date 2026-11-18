# Status

STATE: milestone=M7 phase=T task=X71 state=doing base=58c2988 updated=2026-11-18

Read this file first; the two commands print the state and one task, `AGENTS.md` §1.1 orders a broken plan.

## Where we are

The carry keeps a hand of its own: the drawn square is what carries a piece, so a camera gesture can
hide it and put it back on the piece without touching the window's pointer (ADR 0156, X69).

## Next

X71 is next: the carry's hand is the pointer the viewport hit-tests while it is displaced

## Index

| File | Holds | Read when |
|---|---|---|
| `plan/BACKLOG.md` | one line per task, ids, order, file hints | always |
| `plan/SESSION.md` | the live task: steps, blocker, next command | `state=doing` |
| `plan/debt.md`, `plan/questions.md` | accepted gaps; decisions for the owner | a task touches that area |
| `plan/rename.md` | the pre-release rename: inventory, procedure, gotchas | task R1, or a change that adds a place the product name appears |
| `plan/refactor.md` | design rules, subtask template, order (S7, S8–S22) | any S subtask |
| `plan/archive/done.md` | history up to schema 11 | never, `git log` replaces it |
