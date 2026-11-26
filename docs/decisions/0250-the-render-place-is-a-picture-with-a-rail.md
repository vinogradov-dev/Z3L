# 0250. The render place is a picture with a rail, and the picture stays

- Status: Accepted; supersedes the deferred preview of 0211
- Date: 2026-11-26

## Context

ADR 0211 made the final render a place instead of a dialog, and deferred one thing in writing: "a
preview of the last image ... needs a texture the shell registers, which is why it did not come with
this record". `plan/debt.md` carries the other half of the gap — the place has no status line of its
own — and a person who presses Render on a plate waits minutes for a percentage, cannot see a wrong
exposure or a wrong camera before the run ends, and leaves the application to look at the result.

The tracer made that worse than it had to be: it traced strip by strip, finishing every sample of one
band of rows before starting the next, so the only moment the image existed was the end. The owner's
ask (2026-11-21) is that the render window show the picture, hand it over step by step where that is
possible, and grow toward a plate a workshop can use.

## Decision

- **The picture is the surface.** The place is one picture, fitted to the image's own aspect and
  centred in its room, letterboxed against the panel ink — never stretched to fill it and never
  cropped to it, because a plate is judged by its proportions. `ui::render_page::fitted` is the one
  rule; a 3:2 photograph in a square room keeps its ratio and leaves bars.
- **The numbers are a rail beside it, 320 points wide.** Measured from the widest row it draws — the
  two preset buttons and a size field with its unit — so the picture takes everything else. A narrow
  window keeps the rail at 45 % of the place and shrinks the picture; the picture is still the larger
  half at the smallest window the shell allows. The rail is where every control lives: the presets,
  the four numbers, the destination, and the one primary button (Render, or Cancel while a run is in
  flight). Nothing floats over the picture.
- **The status row under the picture is the run's own line**: the resolution, the samples taken and
  owed, the elapsed time, the estimate from those samples, and the file. Running it sits under a thin
  progress bar; done it says the file was written; cancelled it says nothing was written and keeps
  the last picture; a refusal prints its sentence in the danger ink. A picture whose size or sample
  count no longer matches the rail says so in its own line, because the exposure is the only setting
  that acts on a finished picture.
- **The tracer traces sample-major and publishes checkpoints.** One sample over every band of the
  image, then the next, with the accumulators held on the device for the whole run: the first
  checkpoint is the whole frame at a low sample count and every later one refines it. A checkpoint
  averages the accumulators down to a display-sized picture on the device — bands own whole blocks of
  that picture, so a snapshot pixel is never averaged across two of them — and only that picture
  crosses the bus. `SNAPSHOT_LONG_SIDE` is 1024: a bigger picture is bytes nobody sees, and a plate's
  accumulator is half a gigabyte.
- **The mailbox holds one picture, not a queue.** A snapshot nobody has taken is replaced, and both
  counts — published, replaced — are values a test reads. A window that is busy, hidden or slow
  therefore loses pictures instead of growing memory behind a thread that renders faster than the
  screen refreshes.
- **A snapshot is linear radiance at display size, and the shell tones it.** The curve is the file's
  own (`Image::to_rgba8` with the exposure), so dragging the exposure re-tones the picture the place
  already has instead of paying for another run; a change of size or samples is a different picture
  and the row says so. The texture is the shell's (`register_native_texture`, as the viewport's
  target is), because `ui` never sees `wgpu`.
- **The last picture outlives the run.** It stays on the page when the run ends, when a person
  cancels, and when they go back to the document and return — the place is a place in the work, not
  a question about the document. The finished image is published after the file is written, so the
  picture the page keeps is the picture the person can open.

## Consequences

- A plate shows itself growing: an exposure, a camera or a framing that is wrong is visible in the
  first seconds rather than after the last sample.
- The accumulators live on the device for the whole run: a 2480 × 3508 plate holds about 0.5 GB of
  them, and a request past the crate's caps is refused by name rather than by an allocation. A band is
  still one buffer of at most a million pixels, because that is what a storage binding is.
- A checkpoint costs one reduce pass per band and one read-back of the display picture — at most
  16 MB, four times a second at worst — which is why the interval is a count of samples *and* a
  quarter of a second: a draft that takes 60 ms still publishes a handful of pictures, and a plate
  publishes about four a second.
- The estimate is the samples' own arithmetic (`elapsed × left / done`) and is left out when it is
  meaningless: no sample yet, or more than a day. It is a reading, not a promise.
- `plan/debt.md`'s `[ui]` entry for a missing preview and status line is paid off by this record, and
  ADR 0211's deferred paragraph is answered rather than rewritten.
