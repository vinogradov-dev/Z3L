# Status

STATE: milestone=M7 phase=T task=N23 state=idle base=9fec8f9 updated=2026-11-18

Read this file first; the two commands print the state and one task, `AGENTS.md` §1.1 orders a broken plan.

## Where we are

- M7 — objects and the scene file, after M6. `Container` is the only container kind, every node
carries typed properties and `FurnType` a six-digit tree (0034, 0035); `Contour` is a closed ring of
stable-id segments (0040, schema 13, tolerance joint 0072) and `Path` the chain `to_contour` turns
into a ring (0073); an extrusion owns its path (0074) and a primitive its parameters (0054); a
figure
shows its grips and a corner moves freely or along the picked segment (0075); cameras and lights are
scene objects (0036); N11 bevels a panel end about its outline edge (0107, schema 24), C4 adds a
circle by diameter and an arc by radius (0130), and a sphere is drawn around its centre (0139).
- `crates/param` compiles one domain-free solid plan and one planar conversion (0041, 0043), every
panel outline resolves to one `geom::Region` and machining is one ordered list of anchored records
(0044–0046, schema 16); `crates/mesh` caches by derivation inputs, so a rebuild reuses every
untouched node and a collapsed object keeps its shape (0053).
- The editor: U45 opens several scenes as header tabs, and history, selection, camera and split
layout travel with one (0081); U38–U42 landed the visual pass — `theme.rs` owns two palettes and the
type scale (0063, 0069), the ribbon (0066), the folded side panels, `settings.json` and *Home*
(0068),
the `Ctrl+K` palette and the split with an orientation widget (0079, 0080), one display style per
view (0070) — U65 the one 76 x 22 field with its unit outside and one undo entry per drag (0092),
U88 the work cursor and the scene-layer paint under the chrome (0121–0123), U103 the rubber band
that selects by the direction it is dragged (0137).
- ADR 0117 makes one const table the capability inventory every surface filters and the gate checks;
0118 names the atom, U98 the first — the ruler measures with two clicks and writes nothing, 0119 the
tool card, U104 its one lettered `H` field. `crates/export` writes the order package, a manifest and
twelve flat tables, and `z3l export` runs it (K2–K4, 0120).
- S7 audited the design rules and filed S8–S22 under `plan/refactor.md`; S2–S4, G1, G2 and D4–D7 are
complete and F4 waits in `plan/questions.md`.
- Faber's architecture is recorded (0028–0032) but not implemented. `state-contract` (L0) is the
whole plan tool — gate, report, writer, dashboard, ADR index (P4–P7, P9, 0085); `xtask` keeps the
layering rules and `ci` (P8); the plan gate follows every `files:` hint, a closed entry's ADR, the
`STATE:` keys and a stale `base`; `dev` feeds `main` through one merge; the tree is split into four
streams in four worktrees (the application in the primary checkout, the agent, the catalogue and the
prototypes), and `plan/` is written only in the primary checkout (`AGENTS.md` §6.2).

- The prototype mechanism, MCP and the materials catalogue carry their own letters (`J`, `O`, `Q`)
and their first tasks; the prototype design is written down in `docs/domain/prototypes.md` §0–18 and
waits for four decisions before its ADR.

## Next

N23 is next: the empty-space ghost passes the neighbour it meets and holds at the frame envelope

## Index

| File | Holds | Read when |
|---|---|---|
| `plan/BACKLOG.md` | one line per task, ids, order, file hints | always |
| `plan/SESSION.md` | the live task: steps, blocker, next command | `state=doing` |
| `plan/debt.md`, `plan/questions.md` | accepted gaps; decisions for the owner | a task touches that area |
| `plan/rename.md` | the pre-release rename: inventory, procedure, gotchas | task R1, or a change that adds a place the product name appears |
| `plan/refactor.md` | design rules, subtask template, order (S7, S8–S22) | any S subtask |
| `plan/archive/done.md` | history up to schema 11 | never, `git log` replaces it |
