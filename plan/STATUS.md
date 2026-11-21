# Status

STATE: milestone=M7 phase=T task=X95 state=idle base=15f10f8 updated=2026-11-21
STREAMS: agent=J catalog=O prototypes=Q

Read this file first; the two commands print the state and one task, `AGENTS.md` §1.1 orders a broken plan.

## Where we are

The command line is the one headless door into the document: `z3l commands` lists every id with the
schema of the parameters it takes, derived from the struct the command's own parser reads, so the two
cannot drift (J2, ADR 0195); `z3l query` prints a JSON report that reuses the derivation which already
owns the answer (J1, ADR 0183); and `z3l run` hands its recipe to `Session::batch`, which plans the
list on a copy of the document and installs it as one undo entry only when it may land — all or
nothing, or the steps before a refusal — reporting the nodes it created, changed and removed beside
the command's own refusal, while `--dry-run` moves neither the revision nor the history (J4, ADR 0198).

The nomenclature directory is edited by a window of its own: `z3l directory [<folder>]` opens the
item tree, the record's card and the registry view, and a save is one record plus the package's
revision, written behind one lock, record first and the manifest last (O6, ADR 0158). A package
travels as its folder — an export copies every file and its manifest last, an import lands the copy
and checks it before it writes the connected list last — and two folders that claim one number, or a
`requires` the set cannot meet, are refused with both packages named (O7). The catalogue carries
money and stock apart, and a plug follows the corpus it decorates through six ordered steps whose
miss names the one that failed (O8, O9).

A drilled hole is derived and never stored: the crossing of a drilling object with the panel's own
material — its core minus the through cutouts — is clipped in the panel's frame and cached by the
panel, its placement and the objects, so a moved hinge leaves the body's mesh alone. Nothing carries
a drilling object into a scene yet, so the body is drawn undrilled (O10, ADR 0150).

The status bar is three rectangles of text: each zone draws inside its own and clips to it, and the
command keeps its floor on the narrowest window while the selection elides into what it leaves, so
no zone paints under another (X94). It reports and never acts: the five gestures are one band in the
header, a draft's and a contour edit's accept and drop are the one row of the armed tool's card, the
snap's switch is the View tab's Snaps group and the creation parent is the container card's own
(U115).

A contour boolean reads any node that draws one closed chain — a path object's chain or the profile
a body was extruded from — so a drawn figure and the body extruded from it combine alike (X98).

A boolean between two bodies is a step of the target's own history that names another constructive
body: the cutter stays in the tree, may cut several bodies and re-derives them when it moves, while
the Model tab's three cells arm the mode and the object card lists the steps (W1, ADR 0197).

The snaps are one set and one resolver: ten kinds in priority order — a corner, a crossing, a middle, a
centre, a perpendicular foot, a tangent point, a point on an edge or a face, a world axis and the grid
— answered by `Project::snap` from the pointer's ray and the sketch plane, so the drawing tool and the
ruler read one atom and the object move reads the same kinds (U116, ADR 0199). Each kind is a switch of
the workspace, a cell in the View tab's Snaps group with a drawing of its own, a row in the Settings
page, a glyph at the snapped point and a word in the status bar's readout.

## Next

X95 is next: the post's icon reads as a leg, not as a vertical panel

## Index

| File | Holds | Read when |
|---|---|---|
| `plan/BACKLOG.md` | one line per task, ids, order, file hints | always |
| `plan/SESSION.md` | the live task: steps, blocker, next command | `state=doing` |
| `plan/debt.md`, `plan/questions.md` | accepted gaps; decisions for the owner | a task touches that area |
| `plan/rename.md` | the pre-release rename: inventory, procedure, gotchas | task R1, or a change that adds a place the product name appears |
| `plan/refactor.md` | design rules, subtask template, order (S7, S8–S22) | any S subtask |
| `plan/archive/done.md` | history up to schema 11 | never, `git log` replaces it |
