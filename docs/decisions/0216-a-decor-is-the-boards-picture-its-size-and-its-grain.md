# 0216. A decor is the board's picture, the size it covers and its grain

- Status: Accepted; the render half of the material library (Y3)
- Date: 2026-11-21

## Context

Y2 gave the tracer three surfaces and Y3 has to give it the board a workshop buys: a decor at its real
texel scale, the grain along the direction the piece is cut, and nothing invented in between. Two
questions had to be answered before a line was written, and both are the owner's.

The first is where a picture's size comes from. "At its real texel scale" needs a number, and no
renderer can derive it: a decor file is a scan of a board sample, and only the workshop's record knows
how many millimetres that sample was. A constant in the tracer would silently rescale every scan a
shop owns. The second is grain. `PROJECT.md` lists "how is grain direction stored" as an open model
question, so this task must not answer it — but the picture has to know which way its figure runs.

A third question shaped the work rather than the data: the owner's answer was one material library, and
a viewport and a final render that read the same material. Two consumers of one value, in two crates
that must not depend on each other.

## Decision

- **The picture is one value, `scene::Decor`**: the texels, how many millimetres of board one image
  width spans, and the quarter turns its grain is turned by. It sits on `scene::Drawable` beside the
  colour, the colour multiplies it, and a drawable that names none is the colour alone. Texels are
  square — the height follows from the image's aspect — and a coordinate past the picture repeats,
  because a sample is a window onto a repeating print.
- **The size is the record's, and a texture without one is not renderable.** `Decor::new` refuses a
  size that covers no board; a record that names a picture but no size falls back to its flat colour.
- **Grain is the renderer's vocabulary, not the document's.** The turn travels with the decor and the
  shell decides what a record says. The day `PROJECT.md`'s grain question is answered, the shell
  changes and neither the viewport nor the tracer does.
- **One projection, in the drawable's own frame.** The coordinate is computed once, from the panel's
  local position (`ADR 0004`), and baked into what each consumer reads: the viewport's vertex, the
  tracer's `Face`. The grain is defined in that frame, so both consumers agree about where a stripe
  sits without a second implementation to keep in step.
- **The viewport samples; the tracer filters its own.** The viewport binds one texture per decor, the
  tracer carries the pictures as a storage buffer like its Sobol directions, and each does the
  bilinear fetch with repeat addressing. The two are one arithmetic: the tracer's shader converts
  sRGB to linear with the curve the device's sRGB format applies for the viewport.

## Consequences

- A material is a value a frame carries, and a GPU cache keys it by `DecorId`, so three hundred boards
  of one oak are one texture, one run of texels and one table entry.
- The picture's cost is per drawable, not per frame: the coordinate is in the vertex the drawable
  already uploads, and a decor that changes size or turns the grain rebuilds that drawable's vertices
  and no other.
- The materials a shop edits are still nothing: a library file, its editor and the catalogue's
  reference to it are the next task's (`plan/BACKLOG.md`). Until then a frame may carry a decor that
  nothing authors, exactly as Y2's surfaces were carried before this record.
- A viewport and a final render that disagree about a material is now a defect with one owner: the
  frame they both read.
