# 0157. A fill is carried by the hand across a board

- Status: Accepted; refines ADR 0103 and ADR 0109; the carriage is shell state, the measurement
  stays in the model
- Date: 2026-11-18

## Context

The owner reported the empty-space tool from use: a panel created in a frame runs into its neighbour
and freezes there, and should stop at it and jump past it into the free space beyond when the pointer
crosses; and a panel dragged in an empty frame should stop at the frame's envelope and not vanish
when the pointer leaves it.

The first half is a property of the measurement, not of the clamp. A fill's free axis is one
dimension — a shelf's height, a post's place across the void — and it was read off the face the
pointer rests on (ADR 0109). That is exact while the face runs *along* the axis: a wall behind a
shelf holds every height, so the point under the pointer is the height. A face square to the axis
holds one: a shelf's top names 368 for the whole face, so while the pointer travels over that face the
part stays at 368. Measured in a headless frame with the default camera (a 600 × 560 × 720 frame, a
shelf at 350, the pointer sweeping up a screen line through the piece): the ghost held 368 for six of
twenty-five steps — 206 points of pointer travel — and then jumped 152 mm, because the ray left the
shelf's silhouette and the near wall behind it was 520 at that image position.

Two things were wrong with the answer, and only the second is about the clamp:

1. A face square to the axis cannot say where the hand is along it. Any number a *fixed* projection
   gives there is also the number another part of the piece would give — the projection has to be
   anchored somewhere, and the anchor's depth changes the reading by the frame's own depth times the
   tangent of the camera's pitch (about 250 mm at the far wall of a 560 mm deep frame).
2. The plan clamped the free place inside the cell, so a place the hand reached beyond a board came
   back to that board's face.

## Decision

**The point names the cell and its measured sides; the hand carries the place along the free axis.**

- A face that runs along the axis reads the hand exactly, and the tool uses the point's own place.
- A face square to the axis starts a **carriage**: the tool remembers the place that face named, and
  from then on every millimetre the hand travels on the cell's own free axis (`Ray::
  closest_parameter_on_line`, the rule ADR 0153 gives every drag) moves the part by one. A new face,
  a different plane or a new cell starts a new carriage at the place it names; a pointer that names no
  face, or no cell, ends it.
- The plan still keeps the part inside the cell, so the cell's walls — the frame's envelope among
  them — are what stop it. A part is never placed through a board, and a ghost is never drawn inside
  one.
- A pointer that names no cell keeps the ghost the tool already holds, so the click creates what was
  drawn (ADR 0104's shell paint is the same ghost). A refusal that is not about the pointer — a void
  thinner than the board, a cell that is not convex, another product's void — still says why.

The carriage is view state of the gesture, held by the shell beside the distance fields; the model
keeps its pure plan and takes the place as one number in `FillRequest::free`.

## Consequences

- The ghost stops on the board it meets and passes it as the hand does; the same sweep now reads 368
  at the face and 403, 436, 468, 498, 528, 556, 583 over it, with no freeze.
- The hand's own place is the cell's middle depth, so on a face square to the axis the part moves at
  the rate the projection gives there — about a third faster than the pointer's apparent travel at
  that face. The alternative was a still part; a rate is the smaller error, and the typed distance
  fields remain the exact way to place a part.
- Stepping off a square face onto a wall that runs along the axis answers the point again, which can
  be a step: 583 back to 520 in the same sweep. That step is the honest difference between two
  readings, and it is what "jump past it into the free space beyond" asks for.
- The typed distance field still wins over the hand, so a number typed into the card pins the part
  while the hand keeps moving (ADR 0103).
- A pointer outside the piece leaves the ghost standing; it is dropped by the tool's arm, the open
  product, the material's thickness or the card's own ─✕, never by the pointer's absence.

## Related

- [0103](0103-one-probe-serves-the-niche-the-distances-and-the-move.md) — the probe and the fields
  the carriage is written beside.
- [0104](0104-a-ghost-of-a-missing-node-is-shell-paint.md) — the ghost the carried place is drawn in.
- [0109](0109-a-niche-is-bounded-by-the-faces-the-pointer-sees.md) — the point the carriage starts at.
- [0153](0153-a-carried-piece-is-measured-by-the-hand-not-by-the-ray.md) — the hand's own travel,
  which this record reuses for an axis instead of a piece.
- [`docs/domain/products.md`](../domain/products.md) — the fill in prose.
