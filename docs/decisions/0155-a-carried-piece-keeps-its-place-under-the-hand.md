# 0155. A carried piece keeps its place under the hand

- Status: Accepted; replaces the "a view that moves keeps the piece's millimetres" reading of ADR 0152
- Date: 2026-11-18

## Context

ADR 0152 said a camera gesture carries nothing: the hold re-anchored whenever the view moved, so the
piece kept the millimetres it had. That is right about the *hand* — an orbit drag is not a carry —
and wrong about the *eye*: a zoom, a view preset or Home moves the projection under the piece, so the
piece separates from the pointer that holds it. The owner moved a panel, switched to *Front*, pressed
Home and could not find the panel near the cursor.

Measured in a frame test: a two-step wheel zoom at 1.6× left the piece 140 points away from the hand;
a *Front* preset and Home moved it by 100 points and more, in whichever screen direction the new
projection put it.

## Decision

**The screen place is the invariant, not the millimetres.** A carried piece keeps the place on screen
the hand gave it: when the camera changes, the piece slides along its own axis until its projected
pivot is where it was. `TranslateGizmo::keeps_its_place` solves for the distance, taking the error the
piece has left on screen and removing it with the axis' own scale at that place — a projection is not
linear in depth, so one first-order step per iteration, eight at most.

- The hand's own travel is still the only thing that carries the piece (ADR 0153); a camera gesture's
  pointer movement is not travel.
- The correction runs on every camera change, whatever caused it: a wheel, a preset, Home, an orbit.
- The piece can only keep the part of its place that lies along its own axis. The share across the
  axis is the projection's and no carried panel can take it out; the zoom of ADR 0154 makes that
  share small, which is why the two records belong together.
- A camera change therefore changes the gap the card reads. The card shows the number live, and
  Escape restores the piece, so the change is visible and reversible.

## Consequences

- A maker who zooms or switches the view while carrying a shelf keeps holding it, and the gaps follow
  the view rather than the pointer leaving the piece behind.
- An orbit under a carry slides the piece along its axis, because the projection moved and the hand
  did not. The hold is a gesture, not a commitment: nothing is written until the click that puts the
  piece down, and that click is one command and one undo entry.
- A view change with no carry is untouched, and a carried piece whose axis points at the camera has
  no screen place to keep, so it stays where it is.
