# 0247. A rectangular light is a face at watts, and the tracer samples it by solid angle

- Status: Accepted
- Date: 2026-11-26

## Context

The document carried a sun and a point light ([0039](0039-point-lights-use-watts-and-inverse-square-falloff.md)).
A point light cannot make a shadow soft at one distance and hard at another, and it has no visible
body, so an interior lit by a window stayed flat in the final render: the picture the owner points at
is lit by one large source whose shadow hardens where the unit meets the floor, whose niche is filled
by bounce, and whose pane clips to white. The tracer already samples one point light by emitted power
through an alias table and weights a sample by Veach's power heuristic
([0213](0213-a-frame-carries-a-finish-and-the-tracer-samples-by-power.md)), but every light in the
frame was a delta emitter, so both halves of the multiple-importance-sampling formula were written
and untested: the weight was always one.

## Decision

- **A third light kind, `LightKind::Area`**, is a rectangle with a size in the document's own units
  (`LightSize`: two `units::Length`, 10 mm to 10 m a side, a metre by a metre by default). The face
  is centred on the light's origin and lies in its local `XY`, so a placement turns the rectangle
  without moving its centre, and the light emits from one side, along its local `-Z` — the convention
  a sun already uses for its direction. A window lights the room and not the street, and a rectangle
  seen from behind is a black occluder.
- **The size is carried by every light**, like the intensity: the wire has one layout for the three
  kinds, a sun and a point light read the default and never use it, and switching a light's kind
  keeps its face instead of losing an edit. The schema moves to 34 and the one golden file is
  regenerated; there is no migration (ADR 0096).
- **The frame carries the face, not a radiance.** `scene::Lighting.areas` holds up to
  `MAX_AREA_LIGHTS` (four) rectangles: the centre, the two half-edges, the colour and the watts. The
  emitter's radiance is derived once, `AreaLight::radiance` = `P / (PI * A)` for a Lambertian source
  in watts over its area in square metres, which is the one function the conversion is tested by.
  The face's normal is derived from the two half-edges in the ring's own order and never stored, so
  the edges the shader samples and the side it emits from cannot disagree.
- **The tracer samples the face by solid angle, with next-event estimation and multiple importance
  sampling.** A second pair of sampler dimensions picks which rectangle by emitted power through the
  alias table, a third pair lands uniformly on its face, and the density is converted from area to
  solid angle (`d^2 / (A * cos)`) and weighted by the power heuristic against the surface's own
  density at that direction (`bsdf::pdf`, transcribed as `bsdf_pdf`). A surface sample that walks
  into a rectangle's face adds the face's radiance weighted by the same heuristic in the other
  direction, so the two strategies share one path instead of counting it twice.
- **A light's face is a source, not a body.** `scene::Drawable` gains `FaceShading`, and a `Source`
  drawable is drawn unlit and blended over the scene in a pass of its own, in every style that draws
  faces: it is kept out of the shaded passes, out of the stencil, out of the shadow map and out of
  the contact-shadow mask, and the tracer's soup skips it — a shadow ray passes through the face that
  gives the light. The viewport draws the same rectangle as a translucent pane with an outline, and a
  click on it picks the light.
- **The viewport is lit by an area light's centre, at its own power.** The viewport's shading model
  is one key light and eight point lights, and it stays that way: the middle of the face enters that
  list as a point light of the same watts, which is an approximation of the softness and never of the
  brightness. Without it a room the final render fills with daylight draws as a black box.

## Consequences

- A window is one object for the maker: a face with a size, a colour and watts, created by
  `light.create` and edited by `light.params.set` together with its kind, colour and watts.
- The final render spends four more sampler dimensions and a little more uniform space per frame, and
  its light loop has two strategies instead of one. A frame with no rectangle is exactly the picture
  it was: the area arrays are empty and the branch is not taken.
- The glass shadow stays a debt entry: a pane is still an occluder for a shadow ray, and closing it
  needs a shadow ray that knows what it crossed.
- A rectangle is the only shape. A strip, a disc or an IES profile is a new kind with its own
  sampling, not a field on this one.

## Related

- [0039](0039-point-lights-use-watts-and-inverse-square-falloff.md) — a light's watts and falloff.
- [0213](0213-a-frame-carries-a-finish-and-the-tracer-samples-by-power.md) — the frame's finish and
  the point lights' alias table this record extends.
- [0096](0096-the-data-format-has-no-past.md) — the schema bump and the one golden file.
- [`crates/final/src/light.rs`](../../crates/final/src/light.rs) — the alias table and the power
  heuristic, which this record gives its second strategy.
