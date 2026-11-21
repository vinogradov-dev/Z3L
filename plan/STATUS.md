# Status

STATE: milestone=M7 phase=T task=- state=idle base=9601198073db34cec1e25ed1b5d033bc6a67f1d8 updated=2026-11-21
STREAMS: agent=J catalog=O prototypes=Q render=Y,L

Read this file first; the two commands print the state and one task, `AGENTS.md` §1.1 orders a broken plan.

## Where we are

The command line and the tool server are two doors to one document: `z3l commands` lists every id with
the schema its parser reads (J2, ADR 0195), `z3l query` reuses the derivation that owns each answer
(J1, ADR 0183), and `z3l run` and `z3l mcp serve` hand their steps to `Session::batch`, which installs
one undo entry only when the whole list may land (J4, ADR 0198); the served session projects that
registry into four coarse tools over its standard streams (J5, ADR 0185, ADR 0186). The window offers
the same tools over a loopback channel, on by default and closed on the Assistant page: a per-run
token in the settings folder, every call answered at the top of a frame against the tab in front,
`ui_inspect`, and `z3l mcp serve --attach` as that channel without a window (J6, ADR 0188, ADR 0207).
The window also has a chat: `Ctrl+J` opens the panel, one message is one turn, and `plan` answers
every agent batch with the command line's dry run while `build` lands one undo entry per message, the
person's own commands never gated (J7, ADR 0187, ADR 0190, ADR 0209).

The nomenclature directory is edited by a window of its own: `z3l directory [<folder>]` opens the item
tree, the record's card and the registry view, and a save is one record plus the package's revision,
behind one lock, record first and manifest last (O6, ADR 0158). A package travels as its folder — an
export copies every file and its manifest last, an import checks the copy before it writes the
connected list — and two folders claiming one number, or a `requires` the set cannot meet, are refused
with both named (O7).

A drilled hole is derived, not stored: a drilling object crossing a panel's material is clipped in the
panel's frame and cached by it, its placement and the objects, so a moved hinge leaves the mesh alone;
nothing carries a drilling object into a scene yet, so the body is drawn undrilled (O10, ADR 0150).

The status bar is three rectangles of text: each zone clips to its own, the command keeps its floor on
the narrowest window and the selection elides into what it leaves (X94). It reports and never acts:
the five gestures are one band in the header, and a draft's accept and drop are the armed tool's row
(U115).

A contour boolean reads any node that draws one closed chain — a path object's chain or the profile a
body was extruded from — so a drawn figure and the body extruded from it combine alike (X98). A
boolean between two bodies is a step of the target's own history that freezes the other's shape, and a
primitive is a body in both directions — a block notches a ball, a ball holes a board — because it
carries the steps cut from it (W1, X102, ADR 0197, ADR 0208).

The snaps are one set and one resolver: ten kinds in priority order, answered by `Project::snap` from
the pointer's ray and the sketch plane, so the drawing tool, the ruler and the object move read one
atom (U116, ADR 0199). Each kind is a workspace switch, a View-tab cell, a Settings row, a glyph and a
word in the status bar, and each payload answers its own places (U120).

The final render is our own tracer and a place beside the File menu: `crates/final` builds a binned
BVH keyed by the revision, traces a sample per dispatch so cancel and progress are its counters, and
tones and encodes in Rust; `z3l render` is the same entry point headless (Y1, ADR 0210, ADR 0211).

## Next

Nothing is scheduled: Y1 landed, so the editor draws a final image from the place beside the File
menu, and the render stream's order holds the quality behind it (Y2 to Y5). J14 waits with the
network adapter, its consent card and the key's two homes, and P18 and P19 beside it.

## Index

| File | Holds | Read when |
|---|---|---|
| `plan/BACKLOG.md` | one line per task, ids, order, file hints | always |
| `plan/SESSION.md` | the live task: steps, blocker, next command | `state=doing` |
| `plan/debt.md`, `plan/questions.md` | accepted gaps; decisions for the owner | a task touches that area |
| `plan/rename.md` | the pre-release rename: inventory, procedure, gotchas | task R1, or a change that adds a place the product name appears |
| `plan/refactor.md` | design rules, subtask template, order (S7, S8–S22) | any S subtask |
| `plan/archive/done.md` | history up to schema 11 | never, `git log` replaces it |
