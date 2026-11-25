# 0246. The tracer tracks a pixel's variance, and an edge-avoiding filter spends it

- Status: Accepted; builds on 0210, 0213 and Y2, and shapes Y4
- Date: 2026-11-25

## Context

`render.final` (ADR 0210) is our own path tracer: three lobes, next-event estimation, a stratified
sampler, one dispatch per sample (Y2). Its error falls with the inverse square root of the samples a
pixel takes, so the picture a person waits for at 128 to 256 samples carries noise of about a percent,
which is three code values of a mid grey and reads as grain. The reference project measured its own
à-trous filter as taking about a third off that error at eight samples and recorded where the filter
stops paying and starts smearing; shipping one without our own numbers would repeat the mistake it
documents (`plan/notes/Y1.md`).

What the tree had at the start of Y4: an accumulator that sums radiance per pixel and knows nothing
about the spread of the samples behind it, and a picture written straight from that sum.

## Decision

**A pixel's variance is accumulated beside its radiance, and an edge-avoiding à-trous filter of three
levels — guided by the primary hit's albedo, shading normal and camera distance — replaces the noise
it can see, weighted by that variance.**

- **The sum of squares is the shader's, the arithmetic is Rust's.** The integrator writes
  `sum` and `sum(x²)` into one `Pixel` record per pixel of a strip, beside the guide. The unbiased
  variance of the mean is `(Σx² − (Σx)²/n) / (n(n−1))`, written once in `denoise::variance_of_mean` and
  transcribed into the shader's retirement test from the same numbers the host passes in a uniform —
  no threshold is retyped in WGSL.
- **The guide is what the primary ray met, and it is written on the first sample only.** Its albedo,
  the normal that faced the ray and the distance the ray travelled are enough to say where one surface
  ends: two pixels of one board agree on all three, a paint line breaks the albedo, a fold breaks the
  normal, and a step in depth breaks the distance. A ray that met nothing reports the studio's colour,
  its own direction and the tracer's far plane, so the background is one surface and never mixes into
  a board.
- **The distance stop predicts, it does not tolerate.** A floor seen at a glancing angle changes its
  depth by hundreds of millimetres across the image and is still one floor; a fixed tolerance would
  either blur a real step or stop the filter everywhere. The centre pixel's depth gradient — the
  smaller of its two one-sided differences on each axis, so a step is not "explained" by its own
  jump — extrapolates the depth a neighbour should have, and the residual is what the stop measures
  (Dammertz et al. 2010; Schied et al. 2017).
- **Three levels, because the noise is white.** One pixel's error says nothing about its neighbour's,
  so the first level takes most of it — the five-by-five B3 kernel's own spread is a quarter of one
  tap's — and three levels reach eight pixels at a hundredth of the noise. Further levels smooth more
  and buy a bias instead: a board lit by a lamp is a convex falloff, and a kernel wider than the
  curvature's scale flattens it under the picture. The measurements are in `plan/notes/Y4.md`.
- **The filter runs on the host, not in a second shader.** It is one implementation: the Rust the tests
  measure is the Rust that ships, so there is no WGSL twin to drift from it. A level is one pass over
  the image and its rows are shared between threads. A strip of the tracer is traced with the pad the
  widest tap reaches into, so a denoised image carries no seam where two strips met.
- **The blend is the pixel's own error bar: `α = r² / (r² + t²)`.** `r` is its relative error and `t`
  is the noise the file can hide — a fifth of a percent, under a tenth of a code value of sRGB at a mid
  grey. A converged pixel keeps exactly what it was traced, so the filter leaves no permanent blur
  under the picture, and a noisy one takes the filtered value.
- **Adaptive retirement is the same threshold.** A pixel that has taken at least thirty-two samples and
  whose error is under `t` stops being traced: the samples it would take cannot move it visibly. Its
  count is per pixel, and the mean is divided by that count, so a retired pixel is not darkened.

## Consequences

- `Accumulation` carries two pictures: the raw mean, which a reference is measured from, and the
  filtered one, which the file is written of. **A request for 4096 samples is a ceiling**: a pixel that
  reaches the threshold stops before it, so `Report::samples` is what each pixel was *offered*.
- A strip owns fewer rows than before (a million pixels a strip instead of two) because the guide and
  the ping-pong buffer of the filter live beside the accumulator on the device; the rows the filter
  reaches into are traced twice.
- The filter costs about 0.2 seconds a megapixel on the host (sixteen cores), against a render that is
  minutes: recorded in `docs/performance.md` with the rest.
- What the filter cannot see it cannot protect: a surface whose radiance varies within a guide's own
  terms — a rough metal's highlight, a texture that is not in the albedo — is smoothed like noise. The
  blend bounds the damage to the pixels whose error bar is above the file's resolution, and a later
  task that needs more has to widen the guide rather than the kernel.
