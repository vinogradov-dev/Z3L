# 0130. An arc from its chord and radius bows away from the pointer that named its centre

- Status: Accepted
- Date: 2026-11-15

## Context

C4 fills two sketch cells the map has declared but never ran. The first is a circle whose number is
the diameter rather than the radius; the second is an arc given by its start, its end and a radius,
which is what a cabinet maker measures off a drawing when two points are fixed and the arc's radius
is known. The three-point arc already exists and stays the arc cell's main face.

The circle needs no rule: half the number is the radius, and `Path::circle_through` already rounds a
rim point into one. The arc does, because two ends and a radius describe two figures, not one: the
centre may sit on either side of the chord, and around either centre two arcs reach both ends. A
pointer names a side and can name nothing else, so the moment the radius is typed the model has to
decide the rest — and every future caller, a file, an agent or a second tool, inherits that decision.

## Decision

**`Path::arc_from_chord(start, end, radius, side)` stores the minor arc whose centre is on `side`'s
side of the chord, and refuses a radius under half the chord.**

- The chord fixes how far a centre can stand from its midpoint, `h = √(R² − (c/2)²)`, so the two
  candidates are the midpoint ± `h` along the chord's normal. `side` — the pointer that placed the
  radius — names the candidate on its own side.
- The stored arc is the minor one, so it bows away from its own centre and therefore away from the
  pointer. A radius equal to half the chord is the semicircle, which has one centre and still two
  arcs; the same rule picks the one on the far side, so crossing the chord flips the figure in one
  step instead of turning it inside out.
- A radius under half the chord joins no two points: `PathError::ArcRadiusUnderHalfChord` names both
  numbers, and the sentence reaches the status line through the draft, which keeps its clicks.
- The draft's third click stands exactly one radius away from the arc's start (`Shape::Radius`
  anchored at the start), so the number the plate shows is encoded in the point it places and a typed
  radius lands the point itself. The plate reads the pointer's distance from the start, which is the
  radius whenever the pointer is the centre the person is picturing (ADR 0126).
- A click that completes a fixed figure is judged against the figure before it is stored, the rule
  `close` already followed: the arc's short radius and the circle's three collinear points come back
  as the model's sentence and the draft keeps the points that can still be corrected, instead of
  holding a point that no commit can ever use.
- The diameter cell needs no constructor of its own: its plate shows twice the pointer's distance and
  typing halves it, so the rim point carries the diameter and `Path::circle_through` stores the
  radius (C4).
- Neither method carries a key. Both plain and shifted chords belong to the methods of the same cell
  — `C` for centre-and-radius, `Shift+C` for through-three-points — so the cells reach them through
  their method lists, and the workspace's pin lets the cell's own key run them (U67, U96). Only the
  tools a key can start are in `CONTOUR_TOOLS`.

## Consequences

- Mirrored clicks give mirrored arcs, and the tests hold that: a chord of 100 mm with a radius of
  130 mm puts a centre 120 mm out and the peak 10 mm on the other side of the chord.
- The arc's stored ends are the clicked ones up to the rounding of the stored centre, radius and
  angle, exactly as `arc_from_center` and `arc_through` round theirs.
- A caller that wants the other arc, or a major arc, needs a new constructor and an ADR: this one
  answers "start, end and radius", which is a minor arc by construction.
- The refusal lives in the model, not in the widget, so a file, a script or the assistant gets the
  same sentence for the same numbers.

## References

- `crates/model/src/contour.rs` — `Path::arc_from_chord`, `PathError::ArcRadiusUnderHalfChord`.
- `crates/ui/src/draw.rs` — `DrawTool::ArcByRadius`, `DrawTool::CircleDiameter`, `DrawDraft::path_with`.
- `crates/ui/src/draw_fields.rs` — `Shape::Diameter`, the arc-by-radius stage, `construction`.
- ADR [0126](0126-the-point-being-placed-shows-its-numbers-at-the-cursor.md) — the plate a radius is
  typed into; ADR [0127](0127-a-sketch-has-two-guides.md) — the chord as a construction line.
- ADR [0048](0048-a-flat-drawing-is-a-path-of-segments.md) — the path a drawn figure becomes.
- Plan: C4.
