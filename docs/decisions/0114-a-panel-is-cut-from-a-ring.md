# 0114. A panel is cut from a ring, so the panel cell goes

- Status: Accepted
- Date: 2026-11-14

## Context

The *Furniture › Panels* group held a *Panel* cell that created a 600 × 400 mm board from a fixed
size, at the armed container or at the scene root. It was the pre-frame way of making a board: it
knew nothing about a product, asked nothing of the pointer and answered nothing the person was
looking at. Since the product workspace landed (ADR 0098), a board is made where a niche has measured
it — *Post* and *Shelf* fill a void — so the loose cell was a second way to do the same thing, and the
one that ignored the piece being built.

A board outside a niche has no section to fill: its own outline is the only honest input. The
approved map already held `furniture.panels.contour` («Panel by contour») as a planned cell for
exactly that, and the two cells sat side by side, one live and wrong, one planned and right.

`panel.contour.create` cannot serve the cell. By ADR 0042 it copies the ring of a *constructive
object*: the drawing draft would have to be committed as a path object first, so one gesture would
write two nodes, take two undo entries and leave a sketch beside the board. `panel.create` already
carries a ring as its `outline` and already rebases it at the panel's zero corner (ADR 0083), which
is the whole of what a drawn ring needs.

## Decision

**The *Panel* cell and its path go.** `ToolKind::Panel`, `ToolAction::CreatePanel` and
`Editor::create_panel` are deleted, together with the default it typed into the document. A break in
an affordance is the owner's call, and the owner made it: one way to make a board, and it is the one
the pointer describes.

**`furniture.panels.contour` is live, and the cell draws.** A click arms the free-outline tool on the
work plane; the person clicks the outline and closes it, and the close is the one `Close` every
surface shares (ADR 0052). The command is one `panel.create` carrying the ring as its `outline`, so
the board is one node, one revision and one undo entry, and no path is left beside it.

**The draft keeps one shape and gains an intent.** `DrawDraft` is unchanged; the editor stores what
the running draft will become (`DrawIntent::Path` or `DrawIntent::Panel`), the ribbon reads that to
press the contour cell rather than the sketch cell whose tool the draft borrows, and
`start_drawing_with` resets it so a sketch tool never inherits a board's intent.

**The cell asks for what the command needs.** It is enabled only inside an open product and only with
a catalogue material, because a panel is a part of a piece (ADR 0098) and takes its thickness from a
material. Its hint says the gesture, and an open chain is refused with a sentence — closing the ring
is what the tool asks for — instead of the document's reach error.

## Consequences

- One gesture makes one board. Nothing sits beside it, so the parts list, the drawing and the export
  see the panel alone.
- A board is now born either measured (a niche) or drawn (a ring). Neither path asks for a size in
  millimetres, and a plain rectangle is drawn rather than typed.
- `create_panel_sized` returns the id of the part it created, so a caller that drew the ring can name
  the node without reading the selection back.
- The ring is anchored at the panel's zero corner by the command, so the drawn figure's placement is
  the work plane's frame and the UI does not re-derive the panel's origin.
- `panel.contour.create` stays an API-only command with no cell: copying a body's ring into a panel
  is not what the *Panel from contour* cell means, and U94 places it or declares it `Api` with the
  reason.
- The contour cell has no keyboard shortcut. It shares the pointer with the sketch tools, whose keys
  are theirs; a key for it would need its own decision.

## References

- `crates/ui/src/draw.rs` — `DrawIntent`, `DrawnNode`.
- `crates/ui/src/editor/drawing.rs` — `start_panel_drawing`, `commit_drawing`, `commit_panel`.
- `crates/ui/src/editor/creation.rs` — `create_panel_sized`.
- `crates/ui/src/toolbars/spec.rs` — the `furniture.panels` group, `ToolKind::PanelContour`.
- `crates/ui/src/toolbars/ready.rs` — the cell's enablement and its hint.
- `crates/engine/src/panel_cmds/panel.rs` — `PanelCreate::outline`, `panel.contour.create`.
- `apps/furnicraft/src/frame_tests.rs` — `the_contour_cell_cuts_a_panel_to_the_ring_the_pointer_drew`.
- ADR [0034](0034-nodes-carry-typed-properties.md), [0042](0042-a-contour-panel-keeps-its-ring-and-edits-a-working-copy.md),
  [0052](0052-a-drawing-commits-as-one-path-on-the-work-plane.md),
  [0083](0083-a-panel-is-anchored-at-its-base-corner.md),
  [0098](0098-one-product-is-open-and-parts-are-born-inside-it.md).
- Plan: U90, with `plan/notes/U90.md`.
