# 0127. A sketch has two guides: a construction line and a reference line

- Status: Accepted
- Date: 2026-11-15

## Context

ADR 0111 decided that a guide is one amber in screen space, and its test demands 3 : 1 against the
window surface, a panel and the workspace. A sketch needs a second, quieter thing: the zero an angle
is read from and the arc that shows the sweep. It lies over an arbitrary board — a white panel, a dark
panel, a shaded face — and it must be almost invisible, so a single pale grey ink fails the 3 : 1 rule
on a light board and a single amber reads as heavily as the construction line it is measured from.
ADR 0121 met the same problem for the pointer and answered it with two fixed inks, one of which reads
on any surface. The owner's reference draws the reference line as a fine dotted line with a small
step, and the arc's radius is the length of the segment it measures.

## Decision

**A sketch draws two guides, and only the construction line takes the guide amber.**

- The **construction line** is the chord being measured — an arc's start to its end, a node's old
  position to the one under the pointer. It is `theme::helper`, dashed 6 / 4 points, at
  `theme::HELPER_STROKE`.
- The **reference line** is the +X axis from the anchor at the radius of the segment and an arc of
  that same radius up to the direction under the pointer. It is dotted with a 3-point step, drawn as
  a light dot (`REFERENCE_DOT_LIGHT`) inside a dark one (`REFERENCE_DOT`), both fixed and both taken
  from the pointer's pair through `theme::reference_inks()`.
- The reference dot is under half of `HELPER_CROSSHAIR_STROKE`, which makes it the thinnest mark the
  scene draws. A theme test proves that one stroke of the pair clears 3 : 1 on the header, a panel and
  the workspace of every preset in both variants.
- Both guides are painted in screen points by `crates/ui/src/guides.rs`. The tool maps its camera and
  work plane to the screen and hands over points, so a guide never grows with the zoom (ADR 0111).
- The reference always shows while a direction is being placed; the construction line shows only for
  an arc's chord and for a moved node's old position, because those are the only two places where a
  second fixed point exists to measure from.

## Consequences

- ADR 0111 keeps its rule and gains its second exception: the work cursor (ADR 0121) and the
  reference line are over the scene rather than on the window, and both draw a fixed pair.
- A new guide ink is no longer a palette decision alone; it must say whether it lies on the window or
  over the scene, and a scene-side ink owes the pair test.
- The reference is not a dimension: it has no value object, no extension lines and no document. A
  measured angle the user keeps is a dimension, which is a different operation (ADR 0118).

## References

- `crates/ui/src/guides.rs` — the painter, the arc sampling and its tests.
- `crates/ui/src/theme.rs` — `REFERENCE_DOT`, `REFERENCE_DOT_LIGHT`, `REFERENCE_SPACING`,
  `reference_inks` and the pair test.
- ADR [0111](0111-a-guide-is-one-amber-in-screen-space.md) — the one amber this record carves a
  second exception out of; ADR [0121](0121-the-pointer-is-a-two-tone-square-above-the-chrome.md) —
  the fixed pair the reference borrows.
- Plan: C1.
