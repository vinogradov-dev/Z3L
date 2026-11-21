# 0203. A frame drawn as a ring builds the part the pointer names

- Status: Accepted; supersedes the non-convex refusal of 0201 and the box shortcut of 0106 for a shaped
  frame; its piece-only seed is superseded by 0204 for the section and the box
- Date: 2026-11-21

## Context

A frame can be drawn from a contour (0159), and the owner drew one: 600 x 560 x 720 worth of an L — one
step in the top edge and a 1.75 m arc for the bottom — and saved it as `/home/alx/skaf.z3l`. **No panel
built in it, in any mode or class.** Measured on the file: the ring turns right once, at
`(1684, 3703)` µm, so 0201's rule — *a ring that is not one convex ring reserves no one cell* — answered
`NicheRefusal::Footprint` at every point inside the frame.

Two further things were measured while fixing it, and both are the reason this record is longer than
"stop refusing":

- **A stored ring's joints do not meet exactly.** The document keeps an arc as a centre, radius and
  angles in millidegrees, so its endpoints are derived and can miss the line they meet by the document's
  own rounding — 4 to 15 µm on this ring, growing with the radius (0072). The exact readers (`Region`,
  the boolean, a half-plane cut) assume one boundary, and a 15 µm gap was enough for the boolean to hand
  back nothing, which surfaced as `Outside` at first and then as `Unrepresentable`.
- **A convex ring's chords cut its own bulge.** The cell's planes are the ring's chords (0201), and a
  chord lies inside the arc it spans. A point between a chord and its arc is outside every plane and
  inside the frame — for the owner's arc the bulge is up to 274 mm deep, so it is a large part of the
  frame.

## Decision

**The ring is welded where it leaves the frame.** `Footprint::outline` closes every joint of the ring it
draws, on the segment which can move without moving a curve: a line's endpoint takes the arc's, and two
arcs close by shortening the later one's start. `Contour::welded` judges each joint by the tolerance
already computed for it (`joint_tolerance_mm`), and a ring the document cannot close is handed on as it
came, so nothing that loaded before stops loading. `Contour::placed` no longer revalidates a rigid move,
for the same reason: a ring converted from document units keeps the rounding of its arcs.

**The ring is read at the pointer, and every edge whose free side holds the point clips it.** What is
left is the convex piece of the frame the pointer names: an L answers its bar for a point in the bar, its
column for a point in the column, and the square both arms share for a point in the corner — the tool
cannot know which arm a hand in the corner meant, and the square is the piece both answer. A convex ring
answers itself with no clip at all, so a rectangle and a cut-corner frame keep exactly the cell they had.
A ring that is still not convex after the clip — an arc bulging into its own shape — keeps
`NicheRefusal::Footprint`.

**The cell keeps the frame's ring beside its planes, and the containment test asks both.** The planes
stay the chords: they are the wall a probe meets, the corner enumeration, and the safe bound of the
largest fit. The ring is the boundary where a chord is not — so a point of the bulge is inside the cell,
and the section a board is cut to keeps the curve (0202). A plane over an arc is flagged as a chord and
leaves the containment test, which is the one place a chord must not speak for the frame.

**A cell that keeps a ring is never a `Box`.** The box shortcut (0106) compares the cell's corners with
the box they span, and a chord polygon can span the very box the ring bulges past. A shaped frame answers
`Section` instead, which is what it is.

## Consequences

- A frame of any shape now builds a part: the owner's L gives a shelf in each arm, and the pointer names
  the arm. `Fit` stays the rectangle inside the chord polygon, which is inside the frame by construction;
  `Shape` takes the exact section, curve and all, so the part follows a curved wall.
- The niche's own **box**, its corners and the void tool's wireframe remain the chord polygon's, so a
  frame with a deep bulge reports a smaller void than it reserves and the overlay leaves the bulge out.
  The section, the part and the ghost are exact; the callout is not (filed in `plan/debt.md`).
- A frame's sides are its ring's bounds at creation, and welding can move the ring by up to the joint
  tolerance, so the box and the ring can disagree by microns. Anything that assumes the ring lies inside
  the box must allow that — the containment rule already allows a micrometre and is the one reader that
  asks both.
- The corner of an L answers the square both arms share, not the bigger arm. A maker who wants the arm
  moves the pointer into it; a rule that guessed would be wrong half the time.

## Related

- [0072](0072-a-contour-joint-is-judged-with-the-drawing-tolerance.md) — the joint tolerance a stored
  ring's arcs need, and the welding this record does with it.
- [0106](0106-a-niche-is-a-convex-cell-of-wall-planes.md) — the convex cell, whose box shortcut a shaped
  frame no longer takes.
- [0159](0159-a-product-frame-reserves-a-footprint.md) — the footprint, the ring among its shapes.
- [0201](0201-the-niche-starts-from-the-frames-footprint.md) — the seed, whose non-convex refusal this
  supersedes.
- [0202](0202-a-section-keeps-the-frames-curve.md) — the section's curve, which the bulge needed.
- [0043](0043-region-booleans-trace-the-boundary-of-the-set.md) — the exact boolean every clip reads.
