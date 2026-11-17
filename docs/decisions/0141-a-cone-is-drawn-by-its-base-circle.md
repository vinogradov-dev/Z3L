# 0141. A cone is drawn by its base circle

- Status: Accepted
- Date: 2026-11-17

## Context

The *Model* tab lists a box, a cylinder and a sphere. The box and the cylinder left the parameter set
with ADR 0138 — they are profiles the sketch draws and `crates/param` sweeps on Z — and the sphere kept
its parameters while gaining its own stages (ADR 0139). No shape was left that would make a cone,
although a turned leg or a blank needs one, and `assets/icons/primitives/cone.svg` was an empty file
counted as a declared cell.

A cone cannot follow the box: a frustum is not the extrusion of a ring, it narrows as it rises. It is
also not a sphere: it has a direction, a base and a top, so a single drawn round does not place it.
What the two round cells already share is the drawing grammar — a centre and a rim point, the same pair
whose plate names the diameter, and three points on the rim — and what the extrusion already has is a
height pulled in the viewport with a typed field.

## Decision

**The cone is a primitive.** `Primitive::Cone` stores a base radius, a top radius, a height and a side
count, and the mesh is derived from them in `crates/mesh` beside the sphere's. The base circle lies on
the object's local `z = 0` and the body rises on its local `+Z`, so the round a maker draws on the work
plane is the base and the node's origin is its centre, as a drawn profile's centre is (ADR 0082).

**A zero top radius is the sharp cone.** It is the apex, not a missing number: the derived mesh ends in
one vertex and has no top face at all, so nothing downstream sees a zero-area cap. The model refuses
only the range between zero and the smallest positive dimension — a ring a micron across is neither an
apex nor a circle. A top wider than the base is allowed and needs no rule; it is a splayed blank.

**The cell carries the sketch's circle methods and hands over to the height pull.** The first stage
draws the base with the circle grammar the sphere and the sketch use; the commit stores the cone at the
ribbon's extrusion height inside an edit scope and starts the pull the extrusion already uses
(ADR 0138). The pull's card gains one row, the top radius, so the height and the taper are one number
pair at the hand; dragging the arrow previews the solid through the session overlay and writes nothing.
The ✓ runs one `object.primitive.replace` and closes the scope — one undo entry for the whole create —
and the ✕ or Escape rolls the scope back, so a cancelled cone leaves neither a node nor a history
entry.

**The card keeps the numbers.** Both radii, the height and the side count are edited after the commit
through the same command, so the number a maker typed or dragged is the number he can type over, and
the pull is a convenience rather than the only way in.

**`crates/param` is untouched.** A cone never enters the solid plan: it is a parameter set, not a
profile swept on Z, so its mesh comes from the parameters exactly as the sphere's does. The task that
filed this work expected a change there; the sweep is not where a cone lives.

**No schema bump.** `Primitive` is a tagged enum inside a payload that already exists, and a new tag
adds no field to a stored value, so a file written before the shape existed still loads (ADR 0096). A
file that carries a cone is readable only by a build that knows the shape, which is the ordinary rule
for an unknown enum variant.

## Consequences

- Every circle method the sketch has reaches a cone, because the cell and the sketch share `DrawTool`
  and the stage table; a future round method reaches one without a new gesture.
- The pull is shared, so a second shape with a height adds a row to the same card instead of a second
  gizmo. A shape whose taper is not a radius would need a different row, not a different mechanism.
- A cone is a mesh-only payload like a sphere: booleans and `crates/param` see it only after
  `object.collapse`. A leg that must subtract from a panel needs that bake, and paying for it is the
  subject of a later task rather than of this one.
- The mesh is the polygonal inscription of the two circles, so its volume undershoots the analytic
  frustum by about half a percent at the default 32 sides — a number the tests assert rather than
  hide.

## References

- `crates/model/src/primitive.rs` — `Primitive::Cone`, `Cone`, `PrimitiveError::InvalidTopRadius`;
  `crates/mesh/src/primitive.rs` — `cone_mesh`.
- `crates/ui/src/draw.rs` — `DrawTool::Cone`, `DrawIntent::Primitive`; `crates/ui/src/editor/drawing.rs`
  — `drawn_primitive`, `commit_primitive`.
- `crates/ui/src/toolbars/spec.rs` — the cone cell and its methods; `crates/ui/src/object_card.rs` —
  `PrimitiveFields`, `length_row_from`.
- `app/src/extrude_gizmo.rs` — the pull, its card and the top-radius row.
- ADR [0082](0082-the-local-zero-is-the-creation-anchor.md),
  [0054](0054-a-primitive-keeps-the-parameters-that-built-it.md),
  [0096](0096-before-the-first-release-the-format-has-no-past.md),
  [0138](0138-a-solid-body-is-a-drawn-profile-swept-on-z.md),
  [0139](0139-a-sphere-is-drawn-around-its-centre.md).
- Plan: V6.
