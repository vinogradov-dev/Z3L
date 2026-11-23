# Status

STATE: milestone=M7 phase=T task=- state=idle base=c0d5a63 updated=2026-11-23
STREAMS: agent=J catalog=O prototypes=Q render=Y,L

Read this file first; the two commands print the state and one task, `AGENTS.md` §1.1 orders a broken plan.

## Where we are

The assistant can reach a real model, and what leaves the machine is what a person agreed to send:
`AssistantSettings.endpoint` and `model` make one OpenAI-compatible wire, and a message posts one
completion on a worker thread so no frame waits, and the answer's `command_batch` call is the batch
the turn dispatches — its steps alone, because the mode, the save and the product are not a model's
to choose (J14, ADR 0189). The key comes from `Z3L_AGENT_KEY`, else from `agent-key` with mode 0600,
and never enters a log line; the first message to an endpoint stops at the panel's consent card.

An acceptance test runs whole tasks the way a foreign client does: a child `z3l mcp serve` over its
own stdio, the protocol written out as newline-delimited JSON-RPC, and the model a list of calls,
so the five scenarios of ADR 0193 need no network and no key (J13). A build is one batch of the
recipe's 38 steps and its fourteen panels, a refusal names the parameter to change, and the copy's
rewind shows one entry per turn.

A workshop can hand the model loop to a program it already runs: `AssistantSettings.agent` is one
line, the process it names speaks ACP version one over its own stdio, and its one MCP server is
`z3l mcp bridge`, the pipe that presents the token out of `channel.json` (J11, ADR 0230) — so it
never enters the agent's argv. The tools stay the window's: a batch that arrives during a turn joins
the turn's edit scope, so one message is one undo entry, and the stop becomes `session/cancel` on
the frame it is taken.

An inspector reads the seven mistakes ADR 0191 names in one pass and returns one list in severity
order — each names the ids it blames, says what a person would see and carries the commands a fix
would dispatch; `inspect` is the fifth coarse tool, and `z3l inspect <scene>`
prints the same list on the command line (J9, ADR 0191).

The command line and the tool server are two doors to one document: `z3l run` reads a recipe as JSON
Lines and `command_batch` takes the same steps, one undo entry either way, a batch names the product
it works in, and a step names what it creates with `as`, so a later step says `{"$ref": "<name>"}`
(J4, J17, J18, ADRs 0198, 0212, 0214).

The guide is what a model reads before its first command: `engine::guide()` returns the two worlds a
request can belong to and the order of a build from scratch, generated from the registries, and
every command carries the one family word it serves (J20, ADR 0215). A released
build ships no prose, so the surface is the dictionary: every schema annotates its field's unit, and
the sides are the caller's reading rather than the program's vocabulary (J19, ADR 0227). The list it
reads first is sized like one: a short row per command — id, label, world, what it creates — and one
command's schema answered for the id that is named, so the answer is 23 278 bytes against 189 989
and `LIST_BUDGET_BYTES` is the promise a test holds; the rows carry no verdict (ADR 0226), and a
plan session says once why nothing it runs here writes (J15, ADR 0231).

A drawing becomes bodies in one press, one per ring, in one revision (X100, ADR 0205, ADR 0220).
The final render is our own path tracer — three lobes, light sampled by emitted power,
Owen-scrambled Sobol, Russian roulette (Y2, ADR 0213) — whose studio is a setting of the render,
not of the document, and every drawable carries the `Surface` that `render.final` fills from those
settings.

## Next

Nothing is scheduled: 33 open tasks wait in `## Next`.

## Index

| File | Holds | Read when |
|---|---|---|
| `plan/BACKLOG.md` | one line per task, ids, order, file hints | always |
| `plan/SESSION.md` | the live task: steps, blocker, next command | `state=doing` |
| `plan/debt.md`, `plan/questions.md` | accepted gaps; decisions for the owner | a task touches that area |
| `plan/rename.md` | the pre-release rename: inventory, procedure, gotchas | task R1, or a change that adds a place the product name appears |
| `plan/refactor.md` | design rules, subtask template, order (S7, S8–S22) | any S subtask |
| `plan/archive/done.md` | history up to schema 11 | never, `git log` replaces it |
