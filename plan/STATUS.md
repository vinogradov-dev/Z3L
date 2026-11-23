# Status

STATE: milestone=M7 phase=T task=- state=idle base=c426c9f updated=2026-11-23
STREAMS: agent=J catalog=O prototypes=Q render=Y,L

Read this file first; the two commands print the state and one task, `AGENTS.md` §1.1 orders a broken plan.

## Where we are

A workshop can hand the model loop to a program it already runs: `AssistantSettings.agent` is one
line, the process it names speaks ACP version one over its own stdio, and the session it opens has
one MCP server, `z3l mcp bridge` — the pipe that reads `channel.json`, presents the token and copies
bytes both ways, so the token never enters the agent's argv (J11, ADR 0230). The tools stay the
window's: a batch that arrives during a turn joins the turn's edit scope, so one message is one undo
entry and a process that dies mid-turn leaves the document as the last completed batch left it; a
tool call is a line of its own, the answer's chunks are drawn as they are spoken, and the stop
becomes `session/cancel` on the frame it is taken.

An inspector reads the seven mistakes ADR 0191 names in one pass and returns one list in severity
order — each finding names the document ids it blames, says what a person would see and carries the
commands a fix would dispatch; the rules reuse the derivation that owns each answer and read the
workshop's numbers from `model::WorkshopProfile`, `inspect` is the fifth coarse tool, `z3l inspect
<scene>` the same list on the command line, and the assistant panel re-reads it once per revision so
the model explains the mistake and never computes it (J9, ADR 0191, ADR 0193).

The command line and the tool server are two doors to one document, and a run, a batch and a served
call are one operation: `z3l run` reads a recipe as JSON Lines and `command_batch` takes the same
steps, one undo entry either way, all or nothing, a dry run that writes nothing (J4, ADR 0198); a
batch names the product it works in (J17, ADR 0212), and a step names what it creates with `as`, so
a later step says `{"$ref": "<name>"}` and the wardrobe recipe builds its 38 steps without writing a
single document id (J18, ADR 0214).

The guide is what a model reads before its first command: `engine::guide()` returns the two worlds a
request can belong to and the order of a build from scratch, generated from the registries so a name
cannot drift, and every command carries the one family word it serves (J20, ADR 0215). A released
build ships no prose, so the surface is the dictionary: every schema annotates the unit of each
field that carries one, and the sides are the caller's reading rather than the program's vocabulary
(J19, ADR 0227).

A drawing becomes bodies in one press: `object.path.extrude` turns a closed path into one body in
place and a contour set into one body per ring, in one revision, the node keeping the first ring
(X100, ADR 0205, ADR 0220). The final render is a path tracer whose studio is a setting of the
render and not the document — three lobes, direct light sampled by emitted power, Owen-scrambled
Sobol and Russian roulette, written and tested in Rust (Y2, ADR 0213) — and the frame carries a
`Surface` on every drawable, whose `Lighting::environment` `render.final` fills from its own
settings.

## Next

Nothing is scheduled: 31 open tasks wait in `## Next`.

## Index

| File | Holds | Read when |
|---|---|---|
| `plan/BACKLOG.md` | one line per task, ids, order, file hints | always |
| `plan/SESSION.md` | the live task: steps, blocker, next command | `state=doing` |
| `plan/debt.md`, `plan/questions.md` | accepted gaps; decisions for the owner | a task touches that area |
| `plan/rename.md` | the pre-release rename: inventory, procedure, gotchas | task R1, or a change that adds a place the product name appears |
| `plan/refactor.md` | design rules, subtask template, order (S7, S8–S22) | any S subtask |
| `plan/archive/done.md` | history up to schema 11 | never, `git log` replaces it |
