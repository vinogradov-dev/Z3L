# 0248. A lens is the render's setting, and a sample is a point of the aperture

- Status: Superseded by [0249](0249-a-cameras-lens-is-document-data.md) in the home of the two
  numbers; the thin lens, the disc sampler and the circle of confusion stand. The lens is the
  camera's and is saved with it.
- Date: 2026-11-26

## Context

The final render is a pinhole camera: every ray of a pixel leaves the eye along one direction, so
everything in the picture is equally sharp. The image the owner points at reads the way it does
because of depth of field — a narrow band of focus across the miniature furniture and the rest
melted — and behind that sit the three capabilities the plan already carries: bounce light, contact
shadows and a denoised estimate. A pinhole renders a product sharp, which is what a catalogue image
usually wants; an aperture is what turns the same scene into an atmosphere shot.

The camera the tracer reads is `scene::Camera`, and the document's authored camera node projects into
it. The studio had the same question one task earlier — a scene is not a studio — and the answer was
that the render's `Settings` carries it and writes it over the frame's own value on the copy the
tracer reads ([0213](0213-a-frame-carries-a-finish-and-the-tracer-samples-by-power.md)).

## Decision

- **The lens belongs to the render's `Settings`, not to the document.** `Settings::with_lens(aperture,
  focus)` takes two `units::Length`s — the radius of the aperture and the distance the lens focuses at
  — and `run` puts them on the copy of the camera the tracer reads. Zero radius is the pinhole every
  render had before this, so a caller that names nothing pays nothing and gets the picture it always
  got. The Render place offers the two numbers in millimetres and `z3l render` takes
  `--aperture`/`--focus`; the shell converts the page's readings into lengths once, as it already does
  for the studio's percentage. The document, the schema and the viewport are untouched: a document
  object that claims a lens is a new decision, exactly as one that claims a studio would be.
- **`scene::Camera` carries the lens** — `aperture()` (a radius in millimetres), `focus()` and
  `with_lens`, which validates both against `MAX_APERTURE_MM` (500 mm) and a focus of at least a
  millimetre. It is the frame's camera that the tracer reads, so the numbers travel where the camera
  travels, and the default is a pinhole.
- **A lens sample is a point of a concentric disc**, drawn from the sampler's second pair — the pair
  the sampler's own documentation reserved for "the pixel and the lens", because those are the two
  draws a person notices first. `sampler::concentric_disc` is Shirley and Chiu's map: a point of the
  unit square becomes a point of the unit disc, uniform by area, with no seam running out of the
  middle of the blur. The WGSL is its transcription.
- **The ray runs from the lens point to the focal point.** The pinhole ray of the sample meets the
  plane at the focus distance; the lensed ray starts at a point of the aperture and ends there, which
  is the thin-lens camera every renderer means by depth of field. A point at distance `z` is spread
  over `2 a |z - F| / F` — the circle of confusion — and `view::circle_of_confusion`,
  `view::confusion_in_pixels` and the device test that renders a board's edge agree on that number.
  An orthographic view has no eye to put a disc at and keeps its pinhole rays.

## Consequences

- Depth of field costs one comparison per sample when the aperture is zero, and two more sampler
  numbers when it is not. The dimension budget did not grow: the pair was already there.
- The ray a pixel takes now depends on two draws instead of one, so a lensed picture carries more
  variance than the same picture through a pinhole. Measured on the fixture: 256 samples per pixel
  stay within the two percent the pinhole image reaches at 200 (`plan/notes/Y5.md`).
- The denoiser's guide already carries the distance to the primary hit ([0246](0246-the-noise-a-pixel-carries-is-measured-and-spent.md)),
  which is what keeps the filter from smearing across the focus band instead of along it.
- A lens is a camera's, so the picture the *viewport* shows stays sharp and a render of the same view
  does not: that difference is the point, and the Render place is where a person decides it.

## Related

- [0213](0213-a-frame-carries-a-finish-and-the-tracer-samples-by-power.md) — the studio, which is the
  same kind of setting in the same place.
- [0246](0246-the-noise-a-pixel-carries-is-measured-and-spent.md) — the guide the filter reads.
- [`crates/final/src/view.rs`](../../crates/final/src/view.rs) — the lensed ray and the circle of
  confusion, both tested without a device.
