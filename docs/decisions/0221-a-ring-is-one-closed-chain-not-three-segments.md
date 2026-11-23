# 0221. A ring is one closed chain, not three segments

- Status: Accepted
- Date: 2026-11-22

## Context
The document refused a closed contour of fewer than three segments. That count was a shortcut for
"closed and with an area" on line-only figures: two lines that walk out and back enclose nothing, and
three corners are the smallest polygon. Two kinds of figure the product already produces break it.

- A union that meets a whole circle simplifies its four quarter arcs into one 360° arc —
  `simplify_loop` joins consecutive pieces that lie on one circle — and the document then refused the
  ring the boolean had just computed: "a contour needs at least three segments, got 1". The drawing
  could not be stored at all, so a circle beside another shape was a dead drawing (X104).
- A half circle closed by its diameter is two segments. The drawing tools make it, and a panel
  outline of that shape could not be stored either.

Both are closed, both enclose an area, and both are what the user drew or the kernel computed.

## Decision
A ring needs **one** segment. What makes it a ring is what `Contour::assemble` already checks: every
segment's end meets the next segment's start — the last one meets the first — within the drawing's
joint tolerance (0072), the signed area is not zero, and no two edges cross. A full arc closes on
itself and encloses `π r²`; two lines that walk out and back have zero area and are refused by that
rule rather than by a count.

- `Contour::new` and `Contour::assemble` refuse only an empty ring, with their own sentence.
- `Contour::from_polygon` still needs three corners: a polygon of two points is not a polygon, and
  the refusal names the corners, not the segments.
- `Contour::sampled_points` still needs three points, because a kernel profile of two is not a face.
- The **editor** keeps its own minimum: `ContourEdit::remove_segment` refuses to leave fewer than
  three segments, because what is left of a smaller ring has no corner to join. It says so in its own
  sentence instead of borrowing the ring's.

## Consequences
- A union that leaves a circle stores it: one arc, one id, one edge. The ring is what the boolean
  produced, not the circle cut into arcs to satisfy a count.
- A ring of one segment has one vertex, so its corner list is that one point: corner-driven edits
  address it, banding has one edge, and `Footprint::outline`'s welding (0203) leaves such a ring as
  it came, because `Contour::welded` needs three segments.
- A full circle is an area like any other: `object.area.extrude` pulls it into one body, and the
  viewport samples it as the chain it is.
- Every reader of a stored contour — the file, the order package, the mesh — must expect a segment
  count of one or two. `docs/domain/geometry.md` and `docs/domain/panels.md` say so.

## Related
- [0072](0072-a-contour-joint-is-judged-with-the-drawing-tolerance.md) — the joint tolerance a full
  arc closes on itself with, and the welding that keeps a ring exact at the geometry boundary.
- [0043](0043-region-booleans-trace-the-boundary-of-the-set.md) — the boolean whose simplification
  produces a one-arc ring, and the nanometre it welds at.
