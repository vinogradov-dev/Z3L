# 0132. The plates of one point are one layout, so no plate hides another

- Status: Accepted
- Date: 2026-11-16

## Context

ADR 0126 gave a sketch point one plate per parameter and let each plate place itself: the tool names
an anchor, and `plate_corner` puts the plate diagonally beside it, on the side of the view with more
room. That rule is right for a lone plate and wrong for a set of them.

Two numbers of one point can name the same anchor. The first click of every sketch tool is read as
X and Y, and both rows carry the point itself as their anchor, so the two plates were drawn at one
corner and the later one covered the earlier one completely: the person saw a Y plate and no X plate
at all. Anchors that merely come close collide as well: a segment's length plate sits on the chord's
midpoint and its angle plate on the reference arc, and at some pointer positions the two rectangles
intersected and one hid half of the other.

## Decision

**The plates of one point are laid out as one set, and every declared row is always drawn.**

- `FieldSet` resolves the whole set before it draws a row. A row still starts at the corner its own
  anchor leaves free, so a lone plate is placed exactly as before.
- A row whose rectangle collides with one already placed moves away from its anchor until it clears,
  in the direction `plate_corner` already leaves the point by: down for an anchor in the upper half of
  the view, up for an anchor in the lower one. The shift is measured against the rectangle it hit, not
  by a fixed pitch, because anchors can be offset by any amount.
- The size a row is laid out with is the rectangle the previous frame measured. A new stage has no
  measured rectangle yet and uses a conservative estimate that is larger than the widest plate a
  sketch declares, so its first drawn frame is already laid out; the measurement takes over on the
  next frame. An egui `Area` sizes itself on its first pass, so that pass is not the frame the person
  reads.
- The keyboard, the typed locks, the focus order and the hit test are unchanged: this record moves
  rectangles, it does not give a row a second identity.

## Consequences

- The first point of every sketch tool shows both X and Y, and no stage of any tool can hide one of
  its numbers behind another. A test walks every tool and every stage at seven view positions.
- A stage can grow a number without a layout decision: its row joins the set and the set places it.
- The estimate is presentation, not a promise about a plate's size. A stage whose plate is wider than
  the estimate would overlap for the one frame before it was measured; a test keeps the estimate
  above every plate the sketch declares today, and a future plate with a word label has to raise it.
- A plate near a viewport edge is still constrained by the `Area` itself; when there is genuinely no
  room for two plates the one at the edge wins, as it did before.

## References

- `crates/ui/src/point_fields.rs` — `FieldSet::plate_layout`, `PLATE_ESTIMATE`, `PLATE_SPACING`.
- `crates/ui/src/draw_fields.rs` — the anchors a stage gives its rows, including the first click's X
  and Y and the length-and-angle pair.
- ADR [0126](0126-the-point-being-placed-shows-its-numbers-at-the-cursor.md) — the plate and the
  field set this record lays out.
- Plan: X43.
