# 0139. A sphere is drawn around its centre

- Status: Accepted
- Date: 2026-11-16

## Context

The *Model* tab still placed the sphere the way every primitive once arrived: the cell armed the
next work-plane click, and the click stored a sphere of the size typed in the ribbon. The box and
the cylinder left that path with ADR 0138 — they are profiles the sketch draws and sweeps — but the
sphere kept a bare click and one method, so the last shape with stored parameters was also the only
one where the person could not see the ball before committing it.

The sketch already knows three ways to describe a round: a centre and a rim point whose radius the
plate names, the same pair whose plate names the diameter, and three points on the rim
(`DrawTool::Circle`, `CircleDiameter`, `CircleThreePoints`). A sphere is the figure those clicks
describe turned about the plane's normal: its equator is the circle through them.

## Decision

**The sphere cell carries three rounds, and the drawn circle is the sphere's equator.** The cell
starts a draft exactly as the box and cylinder cells do, and the methods live in `DrawTool` beside
the circle's. The first click places the centre, the second drags the radius behind a phantom ball
and the lettered `R` (or `Ø`) plate, and the third method takes three points on the rim. A tangency
snap is how one of those points is found, so it needs no pick mode of its own.

**The drawn circle is not stored.** `DrawIntent::Primitive` commits one `object.primitive.create`
whose primitive is `Primitive::Sphere`: the circle's centre becomes the node's origin — the plane's
rotation keeps the sphere's poles on the plane's normal — and the circle's radius becomes the
sphere's own number, read back from the chain with `Path::circle_center_radius`. The card keeps
editing the radius, the side count and the latitude bands, so the number a maker drew is the number
he can type over.

**The phantom is a wireframe ball, not a preview of a node.** The sphere does not exist in the
document until the second click, so there is no session preview to paint: the draft's own preview is
the equator, and two great circles through the plane's normal make the figure read as a ball from
any viewpoint. They are held to the radius exactly, so the phantom cannot promise a size the commit
would refuse.

**The typed primitive size retires.** `Toolbars::primitive_size` existed because a bare click needed
a number to build from; the plates supply it now, and the ribbon keeps only the extrusion height and
the polygon's side count as typed defaults.

## Consequences

- Every circle method the sketch has reaches a sphere, and a future round method reaches one without
  a new gesture: the cell and the sketch share `DrawTool` and the stage table.
- The sphere stays a parameter set rather than a profile and a sweep: a ball is not a ring extruded
  on Z, and its tessellation is a number the card owns (ADR 0054). `PrimitiveKind` keeps one member
  until a shape that is genuinely a parameter set joins it.
- The gesture is one command and one undo entry from the first click: no edit scope is needed,
  because nothing enters the document before the round is finished.
- A drawn radius outside the primitive's accepted range is the model's readable error, not a
  clamp: the phantom is painted from the same radius, so a refused commit leaves the draft editable.
- The three-point method takes the circumradius of its points, so the sphere's centre lies on the
  given plane. A sphere whose centre leaves that plane would need a fourth number, which is a
  different method and not this one.

## References

- `crates/ui/src/draw.rs` — `DrawTool::Sphere`, `SphereDiameter`, `SphereThreePoints`,
  `DrawIntent::Primitive`, `DrawTool::primitive_kind`; `crates/ui/src/draw_fields.rs` — the stage
  table arms that read each round.
- `crates/model/src/contour.rs` — `Path::circle_center_radius`;
  `crates/model/src/primitive.rs` — `Primitive::sphere`.
- `crates/ui/src/editor/drawing.rs` — `start_primitive_drawing`, `commit_primitive`,
  `drawn_primitive`; `crates/ui/src/editor/creation.rs` — `create_primitive_node`.
- `crates/ui/src/toolbars/spec.rs` — the sphere cell and its variants;
  `crates/ui/src/toolbars.rs` — `ToolKind::Primitive`, `ToolAction::DrawPrimitive`.
- `app/src/draw_gizmo.rs` — `paint_phantom`, `phantom_rings`.
- ADR [0054](0054-a-primitive-keeps-the-parameters-that-built-it.md),
  [0130](0130-an-arc-from-its-chord-and-radius-bows-away-from-the-pointer.md),
  [0131](0131-a-stage-shows-the-numbers-its-figure-reads.md),
  [0138](0138-a-solid-body-is-a-drawn-profile-swept-on-z.md).
- Plan: V5.
