# 0202. A section keeps the frame's curve

- Status: Accepted; supersedes the chord rule of 0201 for the section
- Date: 2026-11-21

## Context

0201 seeded the niche's cell from the frame's footprint and then took every **arc of that ring by its
chord**, because a cell is planes only (0106). A case with a 200 mm rounded corner therefore answered a
section whose corner was the chord: a shelf created with the empty-space tool's *Shape* mode came out as
a rectangle with a 200/200 chamfer, and a saved scene shows exactly that — a corner cabinet whose frame
is round and whose shelf is not. The chord is safe (it lies inside the frame) but it is the wrong shape,
and 0201's own Shape promised the case's outline; `classify_outline` has drawn radii since 0106's
neighbourhood and never got one to draw.

The reason the chord was needed is that the *cell* is a set of wall planes. The **section** is not: it is
a ring in one plane, and both the document (`Contour`, analytic lines and arcs) and the geometry layer
(`Region`, exact rings) already hold arcs. `geom`'s boundary-of-the-set boolean keeps rings exact from
input to output, so one half-plane cut through a ring costs an intersection with a covering rectangle and
nothing is sampled.

## Decision

**The cell keeps the frame's ring beside its planes.** The planes stay the chords, because they are what a
probe and the corner enumeration meet and what makes the largest fit safe; the ring travels with them, with
the index where the planes a probe added begin, so a clip never cuts the ring with the chords it seeded.

**A prism's section is that ring clipped by the upright walls.** `Cell::exact_section` clips through
`Region::clip_half_plane`, which is the boolean against a covering half-plane rectangle: lines stay lines,
an arc stays the shorter arc, and a wall that does not reach the ring hands the ring back unchanged. The
section is then a document `Contour` with its arcs, so `NicheShape::Section` describes the shape the frame
reserves rather than a polygon near it.

**`FillMode::Shape` classifies that ring.** A shelf takes the frame's own radius and the created panel is
`Rect` with `CornerFeatureKind::Radius` — the standard panel, with the case's own corner — or a `Contour`
with arcs where the section is not a rectangle. The plan reports both: the outline is what the panel
stores, and the polygon beside it is the same ring sampled at a quarter of a millimetre, which is what the
ghost draws and the tolerance the shell already samples a frame's footprint at.

**Three limits, each a rule that is already there.** A frame that is a plain box keeps the plane path, so
its answers do not move. A post or a wall stands across the cell, where the section is a projection of the
ring and not the ring, so it keeps the plane reading. And a hidden body trims the footprint by boxes, which
is a polygon rule, so a trimmed part keeps the chord reading rather than a curve the trim cannot follow.

## Consequences

- A shelf, wall or post in a cut-corner case still stands inside the frame, and the shelf now stands inside
  it *with the frame's corner*: a radius stays a radius, and a corner cabinet keeps one standard panel
  feature instead of a chamfer that is nearly right.
- The largest fit still reads the chords, so it keeps its old answer and its old safety; a rectangle
  inscribed in the exact section would be a fraction larger and is not the tool's promise.
- A curved **body** — a bent panel, a cylinder, a rounded ball — is still measured by the box around it:
  this record is about a curve the frame *authored*, which the document already holds as a ring. A curved
  wall met by a probe still needs a surface query that can meet a curved face, and that entry stays in
  `plan/debt.md`.
- Cost: one exact clip per upright wall of a shaped frame, measured at 10.6 µs for a rounded 600 x 560
  case against 5.1 µs for the same case as a box, and no change at all for a box.
- The chord rule of 0201 survives exactly where it belongs: the cell's planes. A reader who wonders why a
  cell still carries chords finds the reason in the probes and the corner enumeration, not in the fill.

## Related

- [0106](0106-a-niche-is-a-convex-cell-of-wall-planes.md) — the cell of wall planes this keeps.
- [0159](0159-a-product-frame-reserves-a-footprint.md) — the footprint whose ring this reads.
- [0201](0201-the-niche-starts-from-the-frames-footprint.md) — the seed, whose chord rule this supersedes
  for the section.
- [0042](0042-a-contour-panel-keeps-its-ring-and-edits-a-working-copy.md) — the contour panel a shaped
  part may still become.
- [0043](0043-region-booleans-trace-the-boundary-of-the-set.md) — the exact boolean the clip reads.
