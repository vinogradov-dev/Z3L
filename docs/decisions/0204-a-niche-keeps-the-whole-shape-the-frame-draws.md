# 0204. A niche keeps the whole shape the frame draws

- Status: Accepted; supersedes the piece-only seed of 0203 for the section and the box, and amends the
  chord rule of 0201 for a shelf's section
- Date: 2026-11-21

## Context

0203 made a frame drawn as a ring build *something*: it read the ring at the pointer and answered the
convex piece around it. The owner then built a cabinet out of that frame — an L 2244 x 4423 x 3885 with a
1746 mm arc along its long side — and reported four things on the running build:

1. «Простая панель не все места определяет» — the largest fit was searched in the piece, so in the left
   arm it was 1654 x 2688 where the frame holds 2087 x 2703, and a hand in the arc's bulge got a
   rectangle measured somewhere else entirely.
2. «Со стороны радиуса вообще не определяется ниша» — no niche at all near the arc.
3. «Фигурная панель не заливает всю форму каркаса» — the Shape mode filled one arm of the L.
4. «При частичной заливки вместо радиуса скос» — a part stopped by a hidden body came out as the chord
   polygon, so the frame's radius arrived as a chamfer.
5. «Я ожидаю, что фигурная панель сможет заливать всю форму ниши с неважно каким каркасом.»

Measured on the saved scene: every point of the L answered a niche (1 and 2 are not refusals), the Shape
plan was the arm's piece, and the pointer point came from `ui::pointer_face`, which entered the product's
**box** — so a ray over the corner the arc cut away named a point outside the ring, and *that* is why the
radius side answered nothing.

## Decision

**One frame, two readings, and the shape is the frame's own.** The **ring** is the space the frame
reserves: `frame_ring` turns a drawn ring counter-clockwise and stops there, any shape, and the pointer
only has to stand inside it. The **piece** remains the convex cell around the pointer that 0203 defined,
because planes and corners are what the probes, the stances and the corner enumeration are written in.
The shape a part takes is the ring; the planes are the cell.

**The box a callout draws is the ring's.** `Niche::local_min`, `size` and `corners` come from the ring's
own bounds and its two heights, so an L reports the L. A callout that reported the piece would understate
the space a part fills, and the piece is smaller than the frame it was read from.

**The section a shelf takes is the ring clipped by the walls, not the chords of the ring.** A probe that
meets a part's face clips the ring around the pointer (`Region::clip_half_plane_around`, exact, arcs
kept), and a hidden body clips it by the very half-plane the polygon trim chooses — one decision, two
consumers — so a part stopped by a hidden sheet still keeps the frame's radius.

**The pointer enters the reserved prism.** `ui::pointer_face` intersects the ray with the footprint's ring
carried from the floor to the ceiling (`Region::line_intervals` for the plan, one interval for the
height) instead of the envelope's box, so a ray passes the corner the frame never gave away and finds the
space behind it.

**A body standing in the ring bounds the part, whether or not a ray met it.** Every probe starts at the
pointer, so a wall none of its rays meet — a post in the far arm of an L, a divider the hand is not
looking at — is invisible to the cell while the part it draws would run through it. A surface of the
surroundings that stands in the ring and spans the cell's own height is such a wall: the ring is clipped
at its face, by the rule a hidden body already trims by (0175). A body that stands inside without spanning
the height leaves a void no one ring describes, and that is refused as before.

**A fit goes where the hand is.** The largest rectangle of a shape that is not convex can stand in
another arm — an L's largest is a column the pointer may be nowhere near — so the fit is the largest
rectangle **through the pointer**, searched over the whole section and validated against it: `band_span`
takes the extreme vertical span over every breakpoint between the sides, and `rect_inside` refuses any
candidate whose interior a boundary crosses (Liang–Barsky).

## Consequences

- A frame of any shape now fills whole: on the owner's scene every one of 884 sampled points answers a
  `Shape` plan whose ring is the frame's own six-segment outline with its arc, and no outline point of
  either mode stands outside the frame.
- A fit is bounded by the hand: it is the largest rectangle through the pointer, not the largest in the
  case. Where the pointer stands in an arc's outward bulge — outside the ring's chord polygon — no
  rectangle of the outline contains it, and the fit falls back to the largest rectangle of the shape,
  which puts the part elsewhere. Filed in `plan/debt.md`; the `Shape` mode is exact there.
- The cell's planes stay the piece's, so the stances that are not a shelf's (a post across the void, a
  divider) are still cut from the convex piece. A vertical fill in a shaped frame therefore answers the
  piece's box, not the frame's shape — the shelf is the case the ring is written for.
- A body standing inside the region is a wall only while it spans the cell's height, and it bounds the
  part at one face: a shape cut *around* it — a post in the middle of a shelf with material on both sides
  — needs a ring with a hole and the corners of a non-convex cell, and is not written.

## Related

- [0099](0099-a-void-is-the-space-a-product-reserves.md) — the void, and the probe that bounds it.
- [0106](0106-a-niche-is-a-convex-cell-of-wall-planes.md) — the convex cell, which stays the planes'
  reading.
- [0159](0159-a-product-frame-reserves-a-footprint.md) — the footprint, the ring among its shapes.
- [0201](0201-the-niche-starts-from-the-frames-footprint.md) — the footprint seed, whose chord rule a
  shelf's section leaves.
- [0202](0202-a-section-keeps-the-frames-curve.md) — the section's curve, now kept through a trim too.
- [0203](0203-a-frame-drawn-as-a-ring-builds-the-part-the-pointer-names.md) — the piece around the
  pointer, which this record keeps for the planes and moves out of the shape.
