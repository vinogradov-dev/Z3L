# Status

STATE: milestone=M7 phase=T task=- state=idle base=886101674144bf1e961be17ab8adcbfe75a01870 updated=2026-11-21
STREAMS: agent=J catalog=O prototypes=Q render=Y,L

Read this file first; the two commands print the state and one task, `AGENTS.md` §1.1 orders a broken plan.

## Where we are

The command line and the tool server are two doors to one document: `z3l commands` lists every id with
the schema its own parser reads (J2, ADR 0195), `z3l query` reuses the derivation that owns each
answer (J1, ADR 0183), and `z3l run` and `z3l mcp serve` hand their steps to `Session::batch`, which
plans a copy and installs one undo entry only when it may land — all or nothing, a dry run moving
neither revision nor history (J4, ADR 0198); the served session projects that registry into four
coarse tools over its standard streams, `command_list` naming each schema and why the session cannot
run it (J5, ADR 0185, ADR 0186). The window offers the same tools over a loopback channel, on by
default and closed on the Assistant page: it publishes a per-run token in the settings folder, answers
every call at the top of a frame against the tab a person is looking at, reports itself through
`ui_inspect` and says `write: window` for a batch, while `z3l mcp serve --attach` is that channel
without a window (J6, ADR 0188, ADR 0207). The window also has a chat: `Ctrl+J` opens the assistant
panel, one message is one turn, and the mode switch is the session's — `plan` answers every agent
batch with the command line's dry run and `build` lands one undo entry per message, while the person's
own commands are never gated (J7, ADR 0187, ADR 0190, ADR 0209).

The nomenclature directory is edited by a window of its own: `z3l directory [<folder>]` opens the item
tree, the record's card and the registry view, and a save is one record plus the package's revision,
behind one lock, record first and manifest last (O6, ADR 0158). A package travels as its folder — an
export copies every file and its manifest last, an import checks the copy before it writes the
connected list — and two folders claiming one number, or a `requires` the set cannot meet, are refused
with both named (O7).

A drilled hole is derived, not stored: a drilling object crossing a panel's material is clipped in the
panel's frame and cached by it, its placement and the objects, so a moved hinge leaves the mesh alone;
nothing carries a drilling object into a scene yet, so the body is drawn undrilled (O10, ADR 0150).

The status bar is three rectangles of text: each zone draws inside its own and clips to it, the
command keeps its floor on the narrowest window and the selection elides into what it leaves (X94). It
reports and never acts: the five gestures are one band in the header, a draft's and a contour edit's
accept and drop are the armed tool's row, the snap's switch the View tab's Snaps group (U115).

A contour boolean reads any node that draws one closed chain — a path object's chain or the profile a
body was extruded from — so a drawn figure and the body extruded from it combine alike (X98). A
boolean between two bodies is a step of the target's own history that freezes the other's shape, and a
primitive is a body in both directions — a block notches a ball, a ball holes a board — because it
carries the steps cut from it (W1, X102, ADR 0197, ADR 0208).

The snaps are one set and one resolver: ten kinds in priority order — a corner, a crossing, a middle,
a centre, a perpendicular foot, a tangent point, a point on an edge or a face, a world axis and the
grid — answered by `Project::snap` from the pointer's ray and the sketch plane, so the drawing tool,
the ruler and the object move read one atom (U116, ADR 0199). Each kind is a workspace switch, a cell
in the View tab's Snaps group, a Settings row, a glyph at the snapped point and a word in the status
bar, and each payload answers its own places: a cylinder its rims and axis ends, a cone its rims and
apex, a ball its poles and quadrants (U120).

## Next

Nothing is scheduled: X102 landed and the app work order is empty again. J14 waits in `## Next` with
the network adapter, its consent card and the key's two homes, and P18 and P19 beside it.

## Index

| File | Holds | Read when |
|---|---|---|
| `plan/BACKLOG.md` | one line per task, ids, order, file hints | always |
| `plan/SESSION.md` | the live task: steps, blocker, next command | `state=doing` |
| `plan/debt.md`, `plan/questions.md` | accepted gaps; decisions for the owner | a task touches that area |
| `plan/rename.md` | the pre-release rename: inventory, procedure, gotchas | task R1, or a change that adds a place the product name appears |
| `plan/refactor.md` | design rules, subtask template, order (S7, S8–S22) | any S subtask |
| `plan/archive/done.md` | history up to schema 11 | never, `git log` replaces it |
