# Status

STATE: milestone=M7 phase=T task=- state=idle base=a510037 updated=2026-11-22
STREAMS: agent=J catalog=O prototypes=Q render=Y,L

Read this file first; the two commands print the state and one task, `AGENTS.md` §1.1 orders a broken plan.

## Where we are

The command line and the tool server are two doors to one document, and a run, a batch and a
served call are one operation of the session: `z3l run` reads a recipe as JSON Lines and
`command_batch` takes the same steps, one undo entry either way, all or nothing, a dry run that
writes nothing and a refusal that carries the command's own sentence (J4, ADR 0198). A batch names
the product it works in, which opens it before the steps and leaves no revision behind (J17, ADR
0212); a step names what it creates with `as`, and a later step refers to that name as
`{"$ref": "<name>"}`, resolved as the batch runs, so a part and its banding are one call and one
undo entry, and the wardrobe recipe in `app/tests/recipes` builds its 38 steps without writing a
single document id (J18, ADR 0214).

The guide is what a model reads before its first command: `engine::guide()` returns the two worlds
a request can belong to — a product frame with the parts inside it, or a free body — and the two
that serve neither, the order of a build from scratch, the shapes a frame takes with a real
`Envelope` as the radius-corner example, the three stances of ADR 0105, the class registry grouped
by family with the words a person types, the units and the four sides, all of it generated from the
registries so a name cannot drift; every command carries the one family word it serves, so
`command_list` and `z3l guide` teach the same world from one function (J20, ADR 0215).

The final render is a path tracer, and the studio it draws a scene in is a setting of the render
rather than the document: three lobes — a matte board, a GGX conductor completed with its multiple
scattering and a refractive pane — direct light sampled by emitted power through an alias table,
Owen-scrambled Sobol for the samples and Russian roulette past three bounces, all of it written and
tested in Rust with the WGSL as its transcription (Y2, ADR 0213). The frame carries a `Surface` on
every drawable and a `Lighting::environment`; `render.final` fills the latter from its own settings,
which the Render place offers as a percentage of a white room and the command line as `--studio`.

## Next

Nothing is scheduled: J20 landed, so the program teaches its own domain — the two worlds, the order
of a build, the frame shapes, the stances, the classes and the units ride on `command_list` and
print as `z3l guide`, and every command says which world it serves (ADR 0215). J9 is the next agent
step — the inspector finds the seven mistakes and the model explains them — and it is what catches a
wrong number now that no Rust recipe computes one; J14 waits with the network adapter, its consent
card and the key's two homes; P18 and P19 sit beside them, and P18 has now bitten for real: this
close numbered its record 0214 because the render stream had taken 0213.

## Index

| File | Holds | Read when |
|---|---|---|
| `plan/BACKLOG.md` | one line per task, ids, order, file hints | always |
| `plan/SESSION.md` | the live task: steps, blocker, next command | `state=doing` |
| `plan/debt.md`, `plan/questions.md` | accepted gaps; decisions for the owner | a task touches that area |
| `plan/rename.md` | the pre-release rename: inventory, procedure, gotchas | task R1, or a change that adds a place the product name appears |
| `plan/refactor.md` | design rules, subtask template, order (S7, S8–S22) | any S subtask |
| `plan/archive/done.md` | history up to schema 11 | never, `git log` replaces it |
