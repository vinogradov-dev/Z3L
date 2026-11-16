# 0138. A solid body is a drawn profile swept on Z

- Status: Accepted
- Date: 2026-11-17

## Context

The *Model* tab offered a box, a cylinder and a sphere, and each of the three arrived by one click
from a typed size. The box had grown a gesture of its own (ADR 0101) — a base rectangle and a height
pull — but that gesture knew exactly two points and one rule: a rectangle between them. The
sketch, meanwhile, had learned four ways to draw a rectangle and four ways to draw a circle
(ADR 0130, 0131), and none of them reached a body. A maker who could draw a rectangle from its
centre in the sketch could not do it for a box, and a cylinder could not be drawn by its diameter or
through three points at all.

The body's parameters knew no better: `Cuboid` stored three sides and `Cylinder` a radius, a height
and a side count, so the only figure a primitive could be was the one those numbers describe. The
owner asked for one grammar: draw the profile as a flat contour with every method the sketch offers,
then build the body on Z.

## Decision

**A body is a profile and a sweep, not a parameter set.** The box and cylinder cells carry the
sketch's own rectangle and circle methods as their variants, and the draft that draws them is the
sketch draft. What the draft commits is an ordinary closed path — the profile — and the body is one
linear extrusion of it along the work plane's normal, so `crates/param` builds the solid from the
ring with the code the panel and the extrusion already use. The box a maker draws as a rectangle and
the cylinder he draws as a circle are therefore one kind of object, `Geometry::Constructive`: a path
and an extrusion feature.

**The profile is a node for the length of the pull.** The commit stores `object.path.create` in an
open edit scope, and the shell starts the same height pull an extrusion gets: a lettered `H`, a
shaded preview of the solid at that height, the arrow on its top face. The ✓ runs one
`object.path.extrude`, which turns the node into the body and keeps its id; the ✕ or Escape rolls the
scope back, so a cancelled create leaves neither a profile nor a history entry. Both commands fold
into one undo entry, exactly as a grip drag does (ADR 0092).

**The card edits the profile and the sweep.** A body's card shows the figure's own dimensions — a
rectangle's width and depth, a circle's radius — its extrusion height, and the contour mode that
reshapes the ring in the viewport. So the box a cell drew keeps the three numbers the retired cuboid
showed (width, depth, height), and a profile that is not a rectangle or a circle is edited as corners.

**The retired shapes leave the format.** `Primitive::Cuboid` and `Primitive::Cylinder` are deleted
with schema 25 and no alias remains (ADR 0096): nothing is released, so a file that carries one is a
readable error rather than a silent conversion. The sphere stays a primitive until V5 gives it its own
stages.

**The frame box keeps its gesture.** A product frame reserves a box that is not a solid body
(ADR 0097, 0101), so the base-rectangle-plus-height gesture stays for it alone, with its eight corner
grips. The solid's grips are the contour's corner grips, because its profile is a ring.

## Consequences

- Every rectangle and circle method reaches a body, and a future profile method reaches one without a
  new gesture: the cells and the sketch share `DrawTool`.
- A body's segment count is the profile's own: a circle is sampled at the chord tolerance by
  `crates/param`, so there is no `segments` field to type and no second tessellation rule in the
  document. A body that needs a fixed facet count needs a profile that says so.
- The two commands of the gesture are one entry only while the scope stays open; a plain dispatch
  between them (undo, another tool) closes it and leaves the profile as the drawing it is. Undo
  during the pull cancels the pull, because the node it was growing disappears.
- A drawn box is anchored at the centre of its base ring, not at its minimum corner: the ring rule
  (ADR 0082) wins over ADR 0083's base corner, which is a panel's anchor.
- The primitive card keeps a sphere's radius, sides and bands; the box and cylinder arms of
  `PrimitiveFields` are deleted with the shapes.

## References

- `crates/ui/src/draw.rs` — `DrawIntent::Body`, `DrawDraft::commit_in`; `crates/ui/src/editor/drawing.rs`
  — `start_body_drawing`, `commit_drawing`, `body_name`.
- `crates/ui/src/toolbars/spec.rs` — the box and cylinder cells and their variants;
  `crates/ui/src/toolbars.rs` — `ToolKind::Body`, `ToolAction::DrawBody`, `ToolActivity::Body`.
- `app/src/document.rs` — `commit_drawing`, `finish_extrude`, `cancel_extrude`, `previewed_body`;
  `app/src/extrude_gizmo.rs` — `ExtrudeGizmo::start_profile`, `is_growing`, `Body::read`.
- `crates/ui/src/object_card.rs` — `height_field`, `replace_body_height`.
- ADR [0054](0054-a-primitive-keeps-the-parameters-that-built-it.md),
  [0082](0082-the-local-zero-is-the-creation-anchor.md),
  [0092](0092-a-field-is-a-number-with-its-unit-outside.md),
  [0096](0096-before-the-first-release-the-format-has-no-past.md),
  [0101](0101-a-frame-and-a-box-are-one-tool.md).
- Plan: V4.
