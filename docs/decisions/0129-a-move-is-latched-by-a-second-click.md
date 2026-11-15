# 0129. A move is latched by a second click, and its delta is read from the old position

- Status: Accepted
- Date: 2026-11-15

## Context

Moving a contour corner needed the primary button held for the whole gesture: press, drag, release,
and the corner stayed in the working copy until Enter wrote it. That is the grammar of a paint
program, not of a CAD editor: a person who wants a corner at a distance measured on the drawing has
to hold the button while typing, and the hand that types is the hand that steers.

ADR 0126 gave the point being placed its numbers at the cursor, and ADR 0127 gave a sketch two
guides — a dashed construction line and the reference axis with an arc of the segment's radius. A
move has the same reading: it needs a length and an angle. But it has one thing a drawn segment does
not: a *fixed* second point, the position the node is leaving. That position is the only reference
the move has, and the owner's reference draws the construction line from exactly there.

The awkward cases are the pointer and the plates. The plates of a move sit beside the point they
measure, so the pointer crosses one within a few points of moving at all; a plate that swallowed the
pointer would freeze the move it belongs to. And a press on a plate means "let me type here", never
"put the node down".

## Decision

**A contour corner moves in a latched mode, and the latch measures the delta from the old position.**

- A click on a corner selects it (view state, nothing written); a click on the selected corner
  latches the move; the corner then follows the pointer; the next click drops it.
- The drop is one command and one undo entry: the working copy is written exactly as Enter writes it,
  and the mode restarts from what the document now stores, so the next corner is latched against the
  new shape. A drop that did not move the corner writes nothing at all.
- The plates are the two numbers of the delta: `Length` from the old position and `Angle` from `+X`
  of the node's own plane. They are the point-field atom (`FieldSet`), so a typed number locks its
  parameter, a typed `0` gives it back to the pointer, and Enter drops the move with the numbers the
  plates hold.
- The old position is the anchor of both guides: the dashed construction line runs from it to the
  point under the pointer, and the reference arc's radius is the distance moved (ADR 0127). A hollow
  ghost marks the position the corner leaves.
- Escape gives up the move before it gives up the mode: the corner returns to where it was latched,
  the working copy stays clean and the mode keeps reshaping. The second Escape leaves the mode.
- The plates of a latched move are the one chrome surface that does not take the pointer: the scene's
  hover is replaced by the raw pointer while a latch is live, and a press inside a plate never drops
  the corner. A press that becomes a drag keeps the one-gesture move and arms no latch.
- Only a corner of a contour is latched. Object placement keeps its gizmo, and a figure's own grips
  (a circle's quadrant, a rectangle's side) keep the press-and-drag they had.

## Consequences

- The old position is now a number's anchor; there is no second stored copy of it. The latch keeps it
  in view state for as long as the move lives, and Escape restores from the same value.
- The delta is a `PointState` (`PointState::moved`), so a moved node and a drawn segment cannot drift
  apart: the smaller-angle rule, the plate anchors and the reference are the sketch's own
  (ADR 0128).
- A drop re-enters the mode, so a contour edit survives a commit. That is what lets a person move
  three corners with three drops; the cost is one command per corner instead of one per edit, which
  is the grammar the owner asked for.
- A refused placement is still silent: the working copy keeps its last valid shape, so a latch
  dragged into a self-crossing ring simply stops.
- The along-contour toggle stays the drag's: a latched move takes the motion the toggle resolves, and
  a typed angle then only picks where along the segment the corner lands.

## References

- `crates/ui/src/draw_fields.rs` — `PointState::moved`, the measurement a move shares with a segment.
- `apps/furnicraft/src/contour_gizmo.rs` — `CornerLatch`, `follow`, `end_latch`, `latched_move`,
  `paint_latch` and the selection the click sets.
- ADR [0126](0126-the-point-being-placed-shows-its-numbers-at-the-cursor.md) — the plate, its focus
  and its locks; ADR [0127](0127-a-sketch-has-two-guides.md) — the two guides; ADR
  [0128](0128-a-sketch-angle-reads-from-the-nearer-x-end.md) — the smaller angle.
- Plan: C3.
