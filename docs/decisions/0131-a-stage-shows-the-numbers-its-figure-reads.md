# 0131. A stage shows the numbers its figure reads, and a figure's own number is on every stage

- Status: Accepted
- Date: 2026-11-16

## Context

C5 gives the sketch three ways to build one regular polygon — by its radius, by its apothem and from
one of its edges — and a rectangle that stands on an angled base edge. Two rules fall out of that,
and neither is visible in a single tool's code.

The first is a defect the directed rectangle has carried since it was written: its second click only
names an axis, so `Path::rectangle_from_center_directed` reads the *direction* of `centre → direction`
and nothing else. The stage still declared a length plate, exactly as the shared segment stage does,
and the number moved a construction marker without moving a side. A plate that changes nothing is a
promise the tool cannot keep, and the person has no way to tell which of the two numbers does what.

The second is the side count. Every polygon method shares one number — how many sides — and it is not
a property of any point: it does not move, it does not get snapped, and it survives the click that
places a vertex. Until now it lived only in the ribbon's default and no surface could edit it.

## Decision

**A stage declares exactly the numbers its figure reads, and a number that belongs to the whole
figure is on every stage of its method and writes the draft rather than the point.**

- `Shape::Direction` is the stage whose point is a direction: it has an angle plate and a reference
  arc, no length plate, and it keeps the pointer's own distance so the point lands under the hand.
  `DrawTool::RectangleFromCenterDirected` uses it for its second click. A length the figure ignores is
  not shown, not locked and not stored.
- `Stage` carries the polygon's side count, so `RegularPolygon`, `PolygonApothem` and `PolygonByEdge`
  show `FieldParam::Sides` beside the point's own plates. The count is clamped to the model's range
  where the draft is written (`DrawDraft::set_sides`), which is the same clamp the ribbon's count
  takes, so a typed number can never build a polygon the document refuses.
- The plate writes the draft through the caller that owns it (`Editor::set_drawing_sides`): the draft
  is view state, the document sees nothing until the commit, and the number is live in the preview.
  Zero still means "release the lock", so a released plate leaves the count the draft already has.
- The three polygon methods build one figure: the clicked pair is converted into the centre and the
  rim point that `Path::regular_polygon` takes, so the method stays a construction parameter and the
  stored path is the same ring whichever pair the person clicked (C1).

## Consequences

- The directed rectangle loses one plate and gains nothing else: its axis is typed as an angle, which
  is what the figure always read.
- A new sketch number that is not a point follows this record: it goes on `Stage`, its plate writes
  the object it belongs to, and the draft clamps it at the model's own range.
- `Path::rectangle_from_edge` is a constructor of its own rather than a conversion through a centre:
  the clicked edge has to stay exact, and a centre-and-direction rectangle rounds its four corners.
- Nothing in the document changes: the construction method is not stored, so there is no schema bump
  and the inspector keeps reporting the circumradius (C1).

## References

- `crates/ui/src/draw_fields.rs` — `Shape::Direction`, `Shape::Apothem`, `Shape::Rise`, `Stage::sides`.
- `crates/ui/src/draw.rs` — `DrawTool::PolygonApothem`, `DrawTool::PolygonByEdge`,
  `DrawTool::RectangleTilted`, `apothem_rim`, `edge_rim`, `DrawDraft::set_sides`.
- `crates/ui/src/point_fields.rs` — `FieldParam::Apothem`.
- `crates/model/src/contour.rs` — `Path::rectangle_from_edge`, `Path::regular_polygon`.
- ADR [0126](0126-the-point-being-placed-shows-its-numbers-at-the-cursor.md) — the plate that shows a
  point's numbers; ADR [0119](0119-the-armed-tool-shows-its-parameters-at-the-pointer.md) — the card
  at the pointer.
- Plan: C5.
