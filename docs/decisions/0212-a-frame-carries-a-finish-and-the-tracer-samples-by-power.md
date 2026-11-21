# 0212. A frame carries a finish, and the final render samples the light it is given

- Status: Accepted; the frame contract Y3 fills from the catalogue
- Date: 2026-11-21

## Context

A viewport shades a drawable by its colour and its display style, and that was enough while the final
render drew the same paint everywhere. Y2 gave the tracer three surfaces — a matte board, a rough
conductor and a refractive pane — and the frame had no way to say which of them a drawable is. The
document's material is a catalogue record (kind, nominal thickness, decor, grain availability; ADR
0148), and a tracer that read that record directly would put a catalogue's vocabulary into a crate
whose input is a frame. Nor could the tracer shade a room it could not see: a ray that left the scene
returned black, so every shadow and every through-cutout was a hole.

Two smaller pressures met in the same place. The document's point lights are in watts with inverse
square falloff (ADR 0039), and drawing one of eight uniformly spends seven eighths of a picture's
light samples on the lamps that give least. And the picture's noise at the sample counts a workshop
waits for is decided by the sampler, which Y1 left as white noise.

## Decision

- **`scene::Drawable` carries a `Surface`**: roughness, metallic, index of refraction, transmission
  and a Beer-Lambert tint per millimetre. It defaults to `Surface::MATTE`, so a frame that names no
  finish shades as the paint every drawable was, and every existing test and scene keeps its picture.
  It is the tracer's vocabulary, not the document's: `catalog`'s record is mapped onto it by the render
  that reads it (Y3), and `ui` and `render` ignore it.
- **`scene::Lighting` carries an `environment`**: one linear radiance, zero by default, which is what a
  ray sees where the frame has no geometry. It is scene data and never the viewport's background
  colour, which is view state (ADR 0029). Zero means every frame renders as it did, and the value that
  fills it is a later decision — a studio backdrop is not a view setting and not yet a document object.
- **The final render samples its light, not its lights' count**: the key light is traced at every
  vertex, and one point light is drawn by emitted power through an alias table (`crates/final/src/
  light.rs`). A light sample is weighted by Veach's power heuristic, which is one for a delta emitter
  and becomes meaningful when L2 gives the frame a light with an area.
- **The estimator's lobes, sampler and energy table live in `crates/final` in pure Rust**, and the WGSL
  is their transcription; the two tables the shader needs travel as buffers filled from the same Rust
  functions the tests run (`plan/notes/Y2.md`).

## Consequences

- A frame describes what a surface is made of without describing a material catalogue, and the crate
  that knows both is the one that reads the record.
- The viewport does not change: it keeps shading by colour and style, and the finish is invisible to
  it. A future viewport preview of a finish is a new decision, not a consequence of this one.
- A picture without an environment still shows black where there is nothing, which is the limit Y1
  recorded; the mechanism to close it exists and no surface authors it yet.
- Y3's mapping is a fill of `Surface`, not a change to it. If the catalogue needs a field this struct
  does not carry, that is a change to this record and to the tracer together.
