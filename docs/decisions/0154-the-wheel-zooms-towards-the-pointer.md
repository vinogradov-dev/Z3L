# 0154. The wheel zooms towards the pointer

- Status: Accepted
- Date: 2026-11-18

## Context

`Camera::zoom` scaled the orbit distance and the visible height about the camera's **target**, which
is the view's centre. Everything the maker was looking at therefore slid away from the pointer on
every wheel step: a wheel turn of two steps at 1.6× moved a point that sat 200 points off the centre
to 320 points, and a piece being carried by that pointer left the hand by the same share.

The owner reported it as "при приближении и отдалении деталь улетает от курсора" — while carrying a
panel — and it was measurable in a frame test: three wheel steps at 0.8 scaled the piece 140 points
away from the hand.

## Decision

**Zooming keeps the point under the pointer where it is.** `Camera::zoom_about(factor, anchor)` scales
the view as before and then slides the target towards the anchor by the share the zoom did not take,
in the view's own plane:

```text
applied = distance' / distance
target += (anchor − target)⊥forward · (1 − applied)
```

The share is exact in both projections for a point on the plane through the target — the plane the
pointer's ray meets — and it is where the projection's scale is defined. A target that moved along
the view direction would change the distance the maker just asked for, so only the planar part is
used.

The wheel and the two drags that zoom (`Ctrl`+middle and the armed *Zoom* navigation mode) all reach
this through one function on the document, `zoom_about_pointer`, so the three entry points cannot
disagree. A shade or a test with no point to keep asks for `None` and gets the old zoom about the
target.

## Consequences

- A maker zooms towards what they are looking at, which is what every 3D editor does and what the
  hand already assumed.
- A carried piece stays under the hand through the wheel (X67). The piece still has to slide along its
  own axis for the part of the drift its axis cannot take — see ADR 0155.
- A point in front of or behind the target's plane moves a little, because the projection's scale at
  that depth is not the one at the target. This is the usual approximation of "zoom to cursor" and is
  invisible at the wheel's own step.
