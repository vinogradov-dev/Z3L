# Status

STATE: milestone=M7 phase=T task=- state=idle base=7491499 updated=2026-11-21
STREAMS: agent=J catalog=O prototypes=Q render=Y,L

Read this file first; the two commands print the state and one task, `AGENTS.md` §1.1 orders a broken plan.

## Where we are

The final render is a path tracer, and the studio it draws a scene in is a setting of the render
rather than the document: three lobes — a matte board, a GGX conductor completed with its multiple
scattering and a refractive pane — direct light sampled by emitted power through an alias table,
Owen-scrambled Sobol for the samples and Russian roulette past three bounces, all of it written and
tested in Rust with the WGSL as its transcription (Y2, ADR 0213). The frame carries a `Surface` on
every drawable and a `Lighting::environment`; `render.final` fills the latter from its own settings,
which the Render place offers as a percentage of a white room and the command line as `--studio`.

## Next

Nothing is scheduled: J17 landed, so a caller over the channel names a product, lands parts inside
it and reads which product a batch ran in. J18 is the next step of that build; J14 waits with the
network adapter, its consent card and the key's two homes, with P18 and P19 beside it.

## Index

| File | Holds | Read when |
|---|---|---|
| `plan/BACKLOG.md` | one line per task, ids, order, file hints | always |
| `plan/SESSION.md` | the live task: steps, blocker, next command | `state=doing` |
| `plan/debt.md`, `plan/questions.md` | accepted gaps; decisions for the owner | a task touches that area |
| `plan/rename.md` | the pre-release rename: inventory, procedure, gotchas | task R1, or a change that adds a place the product name appears |
| `plan/refactor.md` | design rules, subtask template, order (S7, S8–S22) | any S subtask |
| `plan/archive/done.md` | history up to schema 11 | never, `git log` replaces it |
