# 0201. The niche starts from the frame's footprint

- Status: Accepted; amends 0106 and the containment rule of 0099; the chord rule for a section's curve superseded by 0202
- Date: 2026-11-21

## Context

A product became a container with an authored envelope, and the envelope became a **footprint**: the
base rectangle, its cut corners, or a ring drawn on the plane (0159). A corner cabinet reserves
`[0..x] × [0..y]` minus the corner it cut away, and an end table may reserve a ring; the sides stay
the bounds of whatever shape it is, so every reader that only wants the box still gets one answer.

The niche did not follow. `Cell::envelope` seeded the free space from the envelope's six planes, so in
a cut-corner case the cut face was not a wall: a probe crossed the space the frame had given away and
stopped at the box the sides describe. Two consequences, both audible in the editor: a fill in such a
case grew its board into space the frame never reserved, and `FillMode::Fit` answered a rectangle that
was not inside the case. N17's tests never covered a non-box frame, so nothing failed.

The corner language of a footprint is the panel's own (`CornerFeatureKind`): a chamfer carries the two
setbacks a maker measures, a fillet a radius. `Footprint::outline` draws either shape as one ring of
directed lines and arcs, and it is the same generator a board cut to that profile uses.

## Decision

**The cell is seeded from the footprint's ring, not from the box around it.** The ring's edges become
the cell's upright planes: a rectangle answers the six planes it always did, one per side and one per
height face, and a cut corner or a drawn ring answers one plane per edge. Every probe then clips that
cell as before, so `Niche::cut` — and therefore both fill modes — answers the frame's own section.

**A curved edge is taken by its chord.** The cell is planes only (0106), so an arc is not a wall it can
hold — *for the section, 0202 keeps the frame's own curve; the cell's planes keep the chord*. The chord is the safe direction of the two available: a convex ring's arc bulges away from the
shape it cuts, so its chord lies inside the frame, and a part cut to it stands inside the piece rather
than in the material the frame kept. A rounded corner is therefore measured as the chamfer its radius
spans, which `classify_outline` still reads as the standard rectangle with one corner feature.

**A footprint that is not one convex ring is refused by name.** A ring that turns right somewhere — a
notch, or an arc bulging into its own shape — reserves a space no one convex cell bounds, and the
plane-only approximation of it would stand in material. `NicheRefusal::Footprint` says so, and the
point rule says the same thing: an envelope now answers containment by its ring's own curve, so the
corner a cut removed belongs to no piece, exactly as picking already reads it.

**The ring budget is the plane budget.** A cell carries up to 32 planes, so a frame of up to thirty
sides is measured and a finer ring is unrepresentable rather than measured with a wall quietly
dropped. Measured on an empty case: 4 sides 16 µs, 16 sides 43 µs, 24 sides 55 µs per niche.

## Consequences

- A shelf, post or wall created in a cut-corner or drawn-ring case stands inside the frame's own
  shape; `Fit` answers the largest rectangle inside that shape and `Shape` the shape itself, still as
  a standard panel whenever a rectangle with corner features draws it.
- A rounded corner gave a **chamfer** of the radius' span, not a radius: the part fitted and classified,
  and its corner was one straight cut where the frame is round. 0202 closed that gap without widening
  the cell — the section keeps the ring's own curve, and the planes keep the chord.
- A non-convex frame reserves no fillable cell at all until the query can answer several cells. The
  refusal names the frame, so the maker is not told the gap is merely odd.
- The niche's box is still the footprint's bounds: the size callout and the niche tool's wireframe
  describe a box that can reach into a cut corner. The cell's own ring would say the same space the
  fill does, and that is a drawing change of its own.
- Cost: seeding now generates the footprint's ring on every niche measurement (a few segments), and a
  shaped envelope costs one extra ring test per containment question. Both measured under a tenth of
  a millisecond at the widest ring this allows.

## Related

- [0099](0099-a-niche-is-the-space-a-frame-reserves.md) — the niche a fill is measured against.
- [0106](0106-a-niche-is-a-convex-cell-of-wall-planes.md) — the convex cell this record seeds.
- [0109](0109-a-wall-bounds-the-void-at-its-own-face.md) — the probe rule that kept clipping.
- [0159](0159-a-product-frame-reserves-a-footprint.md) — the footprint itself, and why the sides stay
  the bounds of any shape.
- [0099](0099-a-niche-is-the-space-a-frame-reserves.md),
  [0103](0103-a-fill-is-measured-in-whole-millimetres.md) — the fill plan built on the cell.
