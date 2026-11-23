# Status

STATE: milestone=M7 phase=T task=- state=idle base=df2f3d2 updated=2026-11-23
STREAMS: agent=J catalog=O prototypes=Q render=Y,L

Read this file first; the two commands print the state and one task, `AGENTS.md` §1.1 orders a broken plan.

## Where we are

The assistant reaches a real model, and what leaves the machine is what a person agreed to send:
`AssistantSettings.endpoint` and `model` make one OpenAI-compatible wire, one message posts one
completion on a worker thread so no frame waits, and the answer's `command_batch` call is the batch
the turn dispatches — its steps alone, because the mode, the save and the product are not a model's
to choose (J14, ADR 0189). A step's parameters are read whether the wire sends the object or its
JSON inside a string (J28), and anything that is not one object is refused with the field and the
shape to send (J16). The key comes from `Z3L_AGENT_KEY` or `agent-key` (0600) and never enters a log
line; the first message to an endpoint stops at the consent card. A workshop can hand the loop to a
program it runs instead: `AssistantSettings.agent` is one line, the process speaks ACP version one
over its own stdio, and its one MCP server is `z3l mcp bridge` (J11, ADR 0230).

A model in the panel reads what the tool server publishes and guesses nothing: `command_list` (one
command's schema), `query` (the tree, a box, bounds, the parts, the diagnostics, the catalogue's
`materials`, a niche) and `inspect` are answered by `mcp::reads::MessageDoor` over the same
functions a foreign client calls (J29, ADR 0232). The history carries every call and the answer tied
to it by the wire's id, a turn ends on an answer that names no call, a repeated batch is answered,
not run twice, and the panel draws Stop for a turn (J27).

An acceptance test runs whole tasks as a foreign client does: a child `z3l mcp serve` over its own
stdio and the model a list of calls, so ADR 0193's five scenarios need no network and no key (J13).
The recipe's prompt names the check that proves a build — `inspect`, `envelope_out` first — and
reads its materials through `query materials` (J29).

An inspector reads the seven mistakes ADR 0191 names in one pass — each names the ids it blames and
the commands a fix would dispatch; `inspect` is the fifth coarse tool and `z3l inspect <scene>`
prints the same list (J9). The guide names the two checks and which question each answers, and what
a panel's `nominal_size` is: two sides, the third from the material (J29).

The command line and the tool server are two doors to one document: `z3l run` reads a recipe as JSON
Lines and `command_batch` takes the same steps, and a step names what it creates with `as` (J4, J17,
J18, ADRs 0198, 0212, 0214).

The guide is what a model reads before its first command: `engine::guide()` returns the two worlds a
request can belong to and the order of a build from scratch, and every command carries the one
family word it serves (J20, ADR 0215). The surface is the dictionary: every schema annotates its
unit, and the list is a short row per command plus one command's schema for the id named (J15, J19,
ADRs 0227, 0231).

A drawing becomes bodies in one press, one per ring, in one revision (X100, ADR 0205). The final
render is our own path tracer, and every drawable carries the `Surface` that `render.final` fills
from the render's settings (Y2, ADR 0213).

## Next

Nothing is scheduled: 32 open tasks wait in `## Next`.

## Index

| File | Holds | Read when |
|---|---|---|
| `plan/BACKLOG.md` | one line per task, ids, order, file hints | always |
| `plan/SESSION.md` | the live task: steps, blocker, next command | `state=doing` |
| `plan/debt.md`, `plan/questions.md` | accepted gaps; decisions for the owner | a task touches that area |
| `plan/rename.md` | the pre-release rename: inventory, procedure, gotchas | task R1, or a change that adds a place the product name appears |
| `plan/refactor.md` | design rules, subtask template, order (S7, S8–S22) | any S subtask |
| `plan/archive/done.md` | history up to schema 11 | never, `git log` replaces it |
