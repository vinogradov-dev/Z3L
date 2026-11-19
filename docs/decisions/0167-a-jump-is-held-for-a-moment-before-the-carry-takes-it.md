# 0167. A jump is held for a moment before the carry takes it

- Status: Accepted; refines the destination rule of
  [0166](0166-the-move-answers-the-place-the-piece-lands.md)
- Date: 2026-11-19

## Context

ADR 0166 made the rule answer the place a drag lands: a destination that is free is taken whole, so a
piece is carried past an obstacle into the free place beyond it. The owner then carried a post up to a
slab's underside and reported the other face of the same rule: as soon as a place above the slab fits,
the piece crosses it in one frame, so a board brought up to a face flips to the far side instead of
resting on it. His words: "если это защита, чтобы он не застрял, то надо сделать или паузу какую-то
или какой-то допуск".

The same report held a second, quieter defect: a face that already touches and is carried into
reported *no contact at all*. The sweep treated the zero gap as an axis that had stopped separating
before the drag began and skipped it, so a post whose top already rested on a slab had nothing in its
way and the rule answered the hand's own distance.

## Decision

**The destination is still the answer, and the crossing is the hand's to insist on.** A carried delta
that changes by more than a jump while the pointer barely moved is not travel, it is a crossing: the
carry keeps the previous place and remembers the request. The same request, accepted again on the
following frames, takes the place once it has been asked for a sixth of a second; a request for a
different place or one the hand abandons clears it, and a fast hand — one that moved the pointer far
enough in the frame — is travel and crosses at once. The rule stays stateless; the pause is the
gesture's, because a document must not remember how a hand moved.

**A touch is a stop when the drag carries the piece into the face.** A projection gap at or under the
document's contact threshold is contact, not a separation that never closes: the axis stops at zero
when the direction is into the face and imposes nothing when it is along or away from it. That is what
lets a post whose end already rests on a slab feel the slab at all.

## Consequences

- A board brought to a face rests on it, and the flip over it needs a continuing pull: the piece is
  stable exactly where a maker aims it, and still reachable beyond the obstacle.
- The pause is measured in time, not in millimetres, so it feels the same at every zoom; a fast drag
  is never delayed, because a jump the hand travelled to is not a jump.
- One number to tune: a sixth of a second is under the time it takes to read the crossing, and long
  enough that a passing hand does not flip a piece. A workspace setting is not warranted yet.
- The zero-gap stop costs one branch in the sweep and fixes a class of "the piece went through the
  face I was pressing it against" reports.
- The typed distance still bypasses both rules (ADR 0103): a number is the maker's face and is never
  delayed by a pause.
