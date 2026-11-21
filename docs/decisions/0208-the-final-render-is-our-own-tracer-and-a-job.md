# 0208. The final render is our own tracer in `crates/final`, and it is a job rather than a document command

- Status: Accepted; the road Y2, Y3, Y4 and Y5 build on
- Date: 2026-11-21

## Context

The owner needs a final render, not a screenshot: one still image at print resolution, from the camera
the viewport already shows. The reference project was measured before this road was chosen
(`plan/notes/Y1.md`): 18.6k lines, of which about 40% is what a furniture scene needs — the
integrator, the sampler and the denoiser — while the part it is clever at, CWBVH compression and mesh
streaming, is for scenes of millions of primitives and a whole kitchen is tens of thousands.

Taking it as a dependency is permitted by its Apache-2.0 licence and still wrong: it would add a third
`wgpu` pin beside `render` and `egui-wgpu`, which must bump together (`AGENTS.md` §5), and it would hand
the quality tuning to a project with one maintainer. The reason for our own is ownership, not the
licence. Copying anything out of it is forbidden by §4.6: its technique is taken as published
algorithm, its text never.

The second question is where the trigger lives. Everything in the engine's registry writes the
document and lands in one undo entry (§4.1); a render writes a file, takes minutes and must be
stoppable halfway. Neither shape is the other's.

## Decision

- **`crates/final` at L5, beside `render`.** It owns the BVH, the sampler, the WGSL integrator, the
  tone map and the PNG, and it borrows the `wgpu` device the shell already made: no instance, no
  adapter choice, no surface. It depends on `scene`, `geom`, `wgpu`, `bytemuck` and `png` — never on
  `render` (a sibling whose pipelines are its own), on `egui`, on a window or on `model`.
- **The BVH is ours and is keyed by the document's revision.** A binned-SAH BVH2 built in Rust (Wald
  2007), flattened into two storage buffers with the children of a node adjacent and a leaf a
  contiguous run of faces; a revision that has not changed rebuilds nothing, and the tracer's build
  count is a number a test reads instead of a wall clock (§4.2).
- **Y1 traces direct light only.** One primary ray per sample, uniform sampling of the lights of the
  frame — the key light as an irradiance and the document's point lights in watts with inverse-square
  falloff (ADR 0039) — and a Lambertian surface. Diffuse only, no bounce, no environmental light: the
  three lobes, next-event estimation with multiple importance sampling, Owen-scrambled Sobol and
  Russian roulette are Y2, the catalogue's materials are Y3 and the denoiser is Y4.
- **One sample per dispatch, and the image is traced strip by strip.** A submit is where a cancel is
  honoured and where a progress counter moves, so the loop is one dispatch per sample; the
  accumulator is a storage buffer bounded by the device's largest binding, so a print-sized image is
  traced in strips of at most two million pixels and averaged into the host's buffer.
- **The tone map is Rust, not a shader.** The accumulator is read back as linear `f32` radiance and
  divided by the sample count, then Narkowicz's 2015 ACES fit, an exposure in stops and the sRGB
  transfer function run on the CPU, where a test can assert the curve without a GPU.
- **`render.final` is one entry point, not an engine command.** `final::job::run` takes one `Input`
  (frame, camera, revision), one `Settings` (request, exposure, destination) and one `Progress`, and
  returns a `Report` or a sentence. The id is `render.final`; the progress and the cancel are atomics
  a window reads between frames. The engine's registry is untouched, and no undo entry answers for a
  render, because the document did not change.
- **The file is written once, at the end, through a temporary file and a rename.** A cancel at any
  sample leaves the previous image exactly as it was and no partial file beside it.
- **`png` rather than `image`.** This crate writes one eight-bit RGBA PNG and nothing else, and `png`
  is already in the tree for the interface shots; a codec package for every format a person might
  want would be a dependency nothing here reads (§4.8).
- **Every technique is implemented from its published source, cited where it is used** — Veach 1997
  and *Physically Based Rendering* for the estimator, Möller and Trumbore 1997 for the triangle test,
  Wald 2007 for the build, Narkowicz 2015 and the sRGB standard for the image — so no file, comment,
  test or golden image of another project enters the tree and `ATTRIBUTION.md` gains nothing (§4.6).
- **Deliberate limits.** No spectral renderer, no volumes, no hair, no displacement, no requirement
  for hardware ray tracing (`RAY_QUERY` is exposed by `wgpu` 30, and a laptop without it must still
  produce the catalogue image); no bloom, no saturation, no HDRI and no EXR, because the owner's answer
  on all four was that they do not matter and a catalogue image is graded in the print pipeline.

## Consequences

- The quality tuning is ours: a wrong image is a bug in this crate, argued against a paper and a test
  rather than against another project's changelog.
- The final render cannot drift into a second `wgpu` version: it uses the shell's device, and `render`
  and `egui-wgpu` stay the only pins that must move together.
- The image is the viewport's view by construction: the shader's camera ray is `Camera::ray_ndc`
  transcribed, and a test asserts that the panel's centre lands on the pixel the viewport projects it
  to, so "the final render draws what I was looking at" is checked rather than promised.
- What is missing until Y2–Y5: soft light, colour bleeding, a transparent pane, a decor texture, a
  denoised image and a lens. Y1's image is correct and hard-edged, and the note says so.
- The interface surface — a Render dialog that sets the size, the samples and the destination and draws
  the progress bar — is not part of Y1: the entry point, the request and the progress it needs already
  exist, and the surface lands with the place that draws it. Until then the render is reachable from a
  test and from a CLI call, which is what the task's own acceptance test uses.
- A device that cannot compile the integrator or allocate the frame is a sentence the shell shows,
  never a black image: every refusal names the field, the cap or the driver's own reason.
