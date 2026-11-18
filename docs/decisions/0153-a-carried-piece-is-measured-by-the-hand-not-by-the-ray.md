# 0153. A carried piece is measured by the hand, not by the ray that points at it

- Status: Accepted; replaces the distance mapping of ADR 0151's carry
- Date: 2026-11-18

## Context

ADR 0151 carried a piece by the pointer and measured the move the way a handle drag does: the
pointer's ray meets the axis line at its closest point, and the distance is the difference of those
parameters. That is right for a handle, because a handle *sits on the line*: the pointer is pressed
within a few points of it and the projection stays well conditioned.

A carried piece is taken by its **body**, so the pointer can be a hundred points off the axis. The
projection of a ray onto a line it does not pass near is unstable: as the pointer strays sideways,
the closest point runs along the line and the sign can even flip. The owner reported the symptom
before it was understood — "the panel runs away from the cursor" — and the divider fixture reproduced
it: a hand that moved sideways and up carried the board from a 50 mm contact to −715 mm in four
frames. In the default view a purely sideways move of 200 points already moves such a projection by
69 mm, and an axis oblique to the view turns that into hundreds.

## Decision

**A carry is measured by the hand's own travel.** The distance is the pointer's movement projected
onto the axis' *screen* direction and scaled where the piece was anchored; one anchored point of
travel along that direction means the millimetres the ray's projection gives there, which is the
local slope and exact at the take. The mapping is linear from the anchor, so it is bounded, monotone
in the hand's direction and identical for every pointer position that travels the same way.

**An anchor is a fact about the hand, not about the frame.** It is set where the axis is named, and
again on every frame that drives the camera; the piece keeps the distance it already has across an
anchor, so the view never moves the piece and the pointer never re-scales the mapping under itself.

**A held handle keeps the ray's projection.** It is the right measurement there — a handle is the
line — and its own dead zone keeps the pointer near it.

**Nothing else about the carry changes.** The rule still cuts the distance (contact, coordinate
planes, frame faces, the 1 mm grid), the typed number still outranks the pointer, the capture line
still names the surface it landed on, and the piece is still put down by one click as one
`node.placement.set_many`.

## Consequences

- The piece can no longer run away from the hand, and the hand's direction is what it obeys: a move
  across the axis carries nothing, so dragging a shelf sideways nudges it by the small part of the
  movement that lies along its thickness, which is honest and slightly less forgiving than an exact
  projection would be if it were stable.
- A piece moved far along its axis travels at the scale measured where it was taken, so in a
  perspective view a very long carry is a linear approximation: it may lag the ray's exact answer by
  a few per cent at the ends. The alternative is a mapping that is exact and unbounded, which is the
  defect.
- The typed distance, the fields and the capture are untouched: they read `drag_delta` and the axis,
  and the axis and the axis' meaning are the same as before.
- A camera gesture anchors the carry every frame it runs, so the hand's travel during an orbit is not
  retroactively applied when the orbit ends (ADR 0152).

## References

- `app/src/gizmo_math.rs` — `travel_frame`, `named_axis`, `VIEW_ALIGNMENT_LIMIT`.
- `app/src/translate_gizmo.rs` — `Travel`, `Travel::distance`, `continue_follow`.
- `app/src/frame_tests.rs` — `a_sideways_hand_never_carries_a_piece_away`,
  `the_furniture_move_takes_a_panel_by_its_body_and_slides_it_its_own_way`.
- ADR [0151](0151-the-furniture-move-takes-the-piece-by-a-click-and-follows-the-pointer.md) — the
  carry this measures, whose distance rule it replaces.
- ADR [0152](0152-a-piece-in-hand-blocks-nothing-and-the-view-moves-under-it.md) — the camera moves
  under the hold and the anchor follows it.
- Plan: X65, whose note (`plan/notes/X65.md`) records what was measured.
