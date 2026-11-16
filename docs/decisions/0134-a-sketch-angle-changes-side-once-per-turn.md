# 0134. A sketch angle changes side once per turn, not twice

- Status: Accepted; supersedes the end choice of 0128
- Date: 2026-11-16

## Context

ADR 0128 read a segment's angle from the *nearer* end of the X axis: a direction in the right
half-plane read from `+X`, one in the left half-plane from `−X`, and the plate showed the smaller of
the two. That keeps the number small, but it makes the end — and with it the reference guide's axis
and its sweep arc — change side at 90° and at 270°, twice per turn.

The owner dragged a pointer across the vertical axis while drawing and watched the auxiliary arc
jump from one side of the point to the other at 90°, which reads as the tool changing its mind about
which side it measures. A measuring reference that flips twice a turn is one more thing to notice
while the hand is busy; a reference that flips once is a quadrant boundary a person already knows.

## Decision

**A direction in the upper half-plane reads from `+X`, one in the lower half-plane from `−X`; the
plate and the guide show the sweep from that end, never more than half a turn.**

- `sketch_angle` picks the end by the sign of the direction's `y` — `+X` for `0..=180`, `−X` below —
  so the plate reads `0..180` from `+X` above the axis and `0..180` from `−X` below it.
- A direction at exactly 180° stays with `+X`, so a sweep changes side only once it has *passed*
  half a turn.
- A typed angle is still a direction from `+X`, 0..360: typing `120` now reads 120 from `+X` and
  typing `240` reads 60 from `−X`. The typed number is a lock and the plate keeps it (ADR 0128).
- The guide arc keeps its side with the number, because both come from the same `SketchAngle`
  (ADR 0127, 0133).

## Consequences

A pointer that crosses the vertical axis no longer moves the guide or renumbers the plate; the cost
is that angles between 90 and 180 are now shown from `+X` as 90..180 instead of from `−X` as 0..90,
so a right angle leaning left reads `120` where it used to read `60`. That is the same measurement,
read from the other end, and it is the end the person is looking at.

Revisit if a future operation needs the smaller number for a fit, snap or typed constraint: that
consumer can take `min(sweep, 180 - sweep)` from the same `SketchAngle` without changing the guide.
