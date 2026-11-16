# 0133. A reference lies on the work plane and reads at a glance

- Status: Accepted
- Date: 2026-11-16

## Context

ADR 0127 gave a sketch a reference line: the X end an angle is read from and the arc that shows the
sweep, drawn as two dots — light inside dark — because the guide lies over an arbitrary board. Two
things about that record did not survive a real drawing.

The first is the geometry. `guides::reference_between` took the work plane's angles and a screen
radius and struck an arc from the anchor in screen space. That is correct only when the plane faces
the camera. In perspective or in an axonometric view a circle of the plane projects to an ellipse and
the X axis to a direction that is not the angle it has in the plane, so the arc was a circle the
figure does not contain and the axis pointed somewhere the plane's X does not go. The same call drew
the latched corner's guide, so the defect was in both sketch surfaces.

The second is the weight. A dot of 0.4 points with a 0.22 light centre is two pixels of antialiasing
on a normal display: on the dark workspace the dark rim vanished into the background and the light
centre was too small to carry the mark. The reference is the only thing a sketch shows before its
first click, so a line nobody can find is not quiet, it is absent.

## Decision

**The reference is sampled on the work plane and projected point by point, and its dot is large
enough to read.**

- `Reference::samples` returns the axis as its two ends and the sweep as a polyline of one sample per
  degree of the short turn, all in work-plane millimetres. `guides::reference` takes the two
  projected polylines and draws the dots along them; it no longer sees an angle or a radius.
- The tool projects every sample with its camera and its work plane, so the axis is the projection of
  the plane's X direction and the arc is the ellipse the plane's circle projects to. The latched
  corner's guide uses the same path.
- A sample the camera cannot place is dropped, and a path with fewer than two finite points draws
  nothing, exactly as before.
- The dot grows to `REFERENCE_DOT` 0.7 with a `REFERENCE_DOT_LIGHT` 0.38 centre, and the step to
  `REFERENCE_SPACING` 4 points. The pair is still the fixed two-tone ink of ADR 0121, and the dot's
  diameter stays under `HELPER_STROKE`, so the reference is still quieter than a guide the program
  draws around the model.

## Consequences

- ADR 0127's sizes are superseded; its decision — two guides, one ink for construction and the fixed
  pair for the reference, with the plane sampled at the segment's own length — stands. The reference
  is no longer the thinnest *mark* by diameter: a dot that cannot be seen is not quiet, and a dotted
  line reads quieter than a stroke at the same width.
- A reference has no screen-space shortcut left: a new projection or a new plane changes what the
  guide draws because the samples change, not because a second drawing rule exists.
- The sampling is one sample per degree, bounded at eight and 360. It is screen-independent and cheap;
  the dot step stays in screen points, so a zoom changes the marks between samples, not the shape.
- A theme change to the reference ink does not change the geometry, and a geometry change does not
  change the ink.

## References

- `crates/ui/src/draw_fields.rs` — `Reference::samples`, `sweep_steps`.
- `crates/ui/src/guides.rs` — `reference`, `dots`.
- `crates/ui/src/theme.rs` — `REFERENCE_DOT`, `REFERENCE_DOT_LIGHT`, `REFERENCE_SPACING`.
- `apps/furnicraft/src/draw_gizmo.rs` — `paint_guides`, `reference_screen`.
- `apps/furnicraft/src/contour_gizmo.rs` — `paint_latch_guides`, `reference_screen`.
- ADR [0127](0127-a-sketch-has-two-guides.md) — the two guides and the pair this record resizes;
  ADR [0121](0121-the-pointer-is-a-two-tone-square-above-the-chrome.md) — the fixed ink pair;
  ADR [0128](0128-a-sketch-angle-reads-from-the-nearer-x-end.md) — the end the sweep is read from.
- Plan: X43.
