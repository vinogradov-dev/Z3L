# 0173. A cut is measured from where the piece stands

- Status: Accepted; completes
  [0171](0171-the-place-the-hand-names-is-asked-before-the-axes.md)'s axis walk
- Date: 2026-11-19

## Context

A drag's proposal is a distance from the start of the drag, and the rule resolved every axis against
the piece's **start** box. That is exact for a fresh drag and wrong for a carried one: the piece is
not at the start any more, it is where the last frame put it.

The fourth session shows the cost. A block 60 × 950 × 100 stands beside a bar 1270 × 70 × 100, so it
has exactly two homes: the bar's near face (`delta = 200`) and its far face (`delta = 1530`). The hand
carried it past the bar to 1533, then drifted back one millimetre to 1529 — a place 1 mm inside the
bar. Measured from the start, the answer was the *near* face, so the block flew 1330 mm back across
the bar; measured from where it stood, the answer is the far face it rests on, and it stays.

The owner's rule is the same sentence from the other side: the object stops, and it stands there until
the place at the cursor is one where it fits.

## Decision

**`MoveRule::limits_from(from, proposed)`** resolves the same proposal from the place the piece was
drawn on the previous frame. `from` is the carried delta the shell already holds; `limits(proposed)`
is `limits_from(ZERO, proposed)` for a drag that has not moved yet, so the axis handle and every
existing answer keep their meaning.

The whole destination is still asked first (0171): a free place is taken whole. Only a refused
destination is walked, and every cut inside that walk is measured from the standing place, including
the travel still to make along an axis.

Two consequences of measuring from a contact were fixed with it:

- A sliver thinner than `CONTACT_MM` at a face is a touch, not an overlap, so a retry pass does not
  read the face the piece rests on as a pair already sunk into one another and lose the contact.
- A retry pass takes an axis only when it is closer to the hand's request than the answer already is:
  a pass may free an axis after the axes beside it moved, never lose the face the drag rests on.

## Consequences

- A piece stops at a face and stays there, at any pointer speed, and the same answer reads in both
  directions along the same body.
- Drawing a piece back to the near side of a body is a jump of the piece's whole length, because that
  is the nearest place on that side where it fits: the rule answers places, not distances travelled.
- The shell passes one extra vector per carried frame; the rule stays pure and stateless.
