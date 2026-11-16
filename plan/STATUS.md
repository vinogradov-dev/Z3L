# Status

STATE: milestone=M7 phase=T task=- state=idle base=7f5517c updated=2026-11-16

Read this file first; the two commands print the state and one task, `AGENTS.md` §1.1 orders a broken plan.

## Where we are

- M7 — objects and the scene file, after M6. `Container` is the only container kind, every node
  carries typed properties, `FurnType` a six-digit tree (0034–0035). `Contour` is a closed ring of
  stable-id segments (0040, schema 13) with the tolerance joint (0072); `Path` is the chain
  `to_contour` turns into a ring, a drawn figure carries a `PathShape` tag (0073), an extrusion owns
  its path (0074), a primitive stores its parameters (0054); a figure shows its grips and a corner
  moves freely or along the picked segment (0075); an extrusion pulls its height from a wireframe
  prism (X19); S5's frame rule reached a drawing, a group and a panel (0082, 0083), proved once in
  `frame_contract.rs`; cameras and lights are scene objects (0036). ADR 0105 makes the three stances
  one flat frame turned, X35 stands the fill post in it, N11 gives a panel end a bevel angle about
  its outline edge (0107, schema 24); C4 adds a circle by diameter, an arc by radius (0130).
- S7 audited the design rules and filed S8–S22 under `plan/refactor.md`: S8–S13 split `geom` (0084),
  S14–S17 `panel_cmds`, the camera, ray and pipelines, S18–S19 the document rules and `Editor` (50),
  S20–S22 the drawing draft and the chrome (2608 -> 2037), U45 the scene (0081); F4 is in questions.
- U45 opens several scenes at once: a header tab per open file with its dirty marker and cross;
  *New* and *Open* open a tab (an open path activates it), *Save*, *Undo* and *Redo* act on the
  active one, a close with unsaved changes asks and the last leaves *Untitled*; history, selection,
  camera and split layout travel with the tab (0081).
- S2, S3, G1, G2, D4–D7 and S4 are complete: `crates/param` compiles one domain-free solid plan and
  one planar conversion (0041, 0043), every panel outline resolves to one `geom::Region`, and
  machining is one ordered list of anchored records driving staged effects and the cards (0044–0046,
  schema 16). `crates/mesh` caches by derivation inputs, so a rebuild reuses every untouched node
  and a collapsed object keeps its shape (0053).
- The UI plan (U1–U26) is shipped: SVG icons, a work plane from the ortho view and draw/pick/snap
  for a `Path` (0047–0052); X3–X6, U30–U37, X1, X2 and X7 are fixed (0056–0062, 0067), X20 gives a
  pole view a canonical up (0057), U59 names the six views, U46 drew every declared icon but
  `primitives/cone` (0055) and U27 caches them per `(uri, px, colour)`.
- U38–U42 and the X-fixes landed the visual pass: `theme.rs` owns two palettes and the type scale
  (0063, 0069), `header.rs` returns a `HeaderAction` (0064), U39 the ribbon (0066), U40 the folded
  side panels and the bottom-centre cluster, U41 *Settings*, `settings.json` and *Home* (0068), U47
  the `Ctrl+K` palette, U42 the one/two/four-view split with an orientation widget, U60 a tree,
  cameras and active view per layout (0079), U61/U62/U51 framing, expand and the floating ribbon
  (0080). U43/U63 gave each view its style (0070), X10 the edges mean the form, U44 a shadow under a
  standing object (0071), X11 closed a ring by an arc (0072), X12 a tool switch replaces the draft,
  X23 every *Close* commits. U65 gave every number one 76 x 22 field, right-aligned with its unit
  outside the string and one undo entry per drag (0092); U88 draws the work cursor in the settings
  size, X41 confines it to the work area, and X42 puts that paint under the chrome (0121–0123).
- `crates/export` writes the order package — the manifest and twelve flat tables — and `furnicraft
  export` runs it: sheets, a price per line, kerf, trim and currency (K2–K4, 0120).
- ADR 0117 makes one const table the capability inventory every surface filters and the gate checks;
  0118 names the atom and U98 is the first one — the ruler measures with two clicks and writes
  nothing, 0119 the tool card, U94–U99 file that work (`plan/notes/U94.md`).
- Faber's architecture is recorded (0028–0032) but not implemented. `state-contract` (L0) is the
  whole plan tool — gate, report, writer, dashboard, ADR index, `agents`/`init` (P4–P7, P9, 0085);
  `xtask` keeps the layering rules and `ci` (P8), the gate follows every `files:` hint, a closed
  entry's ADR, the `STATE:` keys and a stale `base`; `/next-task` keeps the log `/time-review`
  reads; `dev` → `main`.
- X43: one plate layout and a reference that lies in the plane (0132, 0133).

## Next

Nothing is scheduled: 1 open task waits in `## Next`.

## Index

| File | Holds | Read when |
|---|---|---|
| `plan/BACKLOG.md` | one line per task, ids, order, file hints | always |
| `plan/SESSION.md` | the live task: steps, blocker, next command | `state=doing` |
| `plan/debt.md`, `plan/questions.md` | accepted gaps; decisions for the owner | a task touches that area |
| `plan/rename.md` | the pre-release rename: inventory, procedure, gotchas | task R1, or a change that adds a place the product name appears |
| `plan/refactor.md` | design rules, subtask template, order (S7, S8–S22) | any S subtask |
| `plan/archive/done.md` | history up to schema 11 | never, `git log` replaces it |
