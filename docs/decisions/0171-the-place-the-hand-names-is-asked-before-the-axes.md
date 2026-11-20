# 0171. The place the hand names is asked before the axes

- Status: Accepted; narrows how
  [0166](0166-the-move-answers-the-place-the-piece-lands.md) resolves a blocked destination
- Date: 2026-11-19

## Context

0166 settled that the move answers the destination, not the path: a free place beyond an obstacle is
taken whole, and only a refused destination is cut. The implementation then walked the three world axes
one at a time — an axis whose *step* was free was taken, one whose step was not was cut at a face — and
never asked the destination the three steps add up to.

The third logged session shows what that costs. A board 620 × 90 × 100 stood beside a body
60 × 400 × 100, overlapping it on 26 mm of Y and all of Z. The hand asked `(-171, 236)`: forward and
cleared in Y, a free slot in front of the standing body. The answer was `(-94, 236)` — X cut at the
standing body's face — and it stayed there while the pointer travelled another 1.5 s. The log's own
`blocked=none` proves the destination was free the whole time: every single axis was blocked, the place
they made together was not.

The owner's words: a vertical part cannot pass either in front of or behind a horizontal one despite
the space being there; the contact judgement must rest on the faces of the surfaces. It must rest on
them, and it must be asked about the face the piece arrives at — not about the faces a straight line
crosses one axis at a time.

## Decision

**`MoveRule::resolve` asks the whole proposal first.** The three asked distances form one placement; if
the piece may rest there, all three are taken and no axis reports a contact. Refusing a free slot
because the road to it is blocked is not a rule about faces, it is a rule about a staircase.

**Only a refused destination is walked axis by axis**, as 0166 decided, and the walk now runs up to
three passes — one per axis — so an axis cut on the first pass is asked again once the axes after it
have moved the piece. An axis that is still cut keeps its contact, so the shell can still say what
stopped it.

The jump is a gesture concern, unchanged: the rule answers instantly, and the shell's hold
([0167](0167-a-jump-is-held-for-a-moment-before-the-carry-takes-it.md)) decides when a large answer is
taken.

## Consequences

- A board passes a standing body in front of it or behind it as soon as the hand names the free slot,
  regardless of the order the axes are listed in.
- A diagonal carry still lands on a placement the write's guard accepts: the whole-destination answer
  is checked by the same predicate, and the axis walk is unchanged for refused destinations.
- The rule is slower by one `free_at` per frame. It is pure, and there is nothing to cache.
- `contact` is empty whenever the destination is free, even when the straight path crosses a face. That
  is the meaning of 0166, now actually implemented: the answer is about where the piece rests.
