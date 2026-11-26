# 0243. A surface is a base and one coat, and a finish is a number in percent

- Status: Accepted; extends 0213's `Surface`, and the record Y8 builds carries it
- Date: 2026-11-25

## Context

The owner's requirement, 2026-11-25: the materials a workshop names will be **veneer, lacquer and
patina**, each a material in its own right, and a facade is to be assembled from them so that the
catalogue does not grow a record per combination — *шпон, потом наложить лак 50 % или лак 90 %*, and
the picture shows it. The renderer's side of that stayed to be decided by us, "as the popular renderers
do and within our means".

What the tracer has: three surfaces (ADR 0213) — a Lambertian board, a GGX conductor and a refractive
pane. `bsdf::probabilities` puts every opaque surface on one axis, `diffuse = 1 − metallic` and
`specular = metal · (1 − multiple)`, so at `metallic = 0` an opaque surface has **no specular lobe at
all**. A lacquered veneer, a melamine face and a gloss acrylic front are therefore a matte rectangle
with a picture on it, and no denoiser (Y4) turns that into a catalogue image. The number is already
carried and read nowhere: `Surface::ior` reaches the tracer and is used for no opaque surface.

## Decision

**A surface is one base and at most one coat, and the coat is the only layer.**

- **Base** — what the part is made of: albedo and decor (ADR 0216), roughness, metallic, ior,
  transmission and absorption. This is today's `Surface` and nothing in it moves.
- **Coat** — a clear dielectric layer on the base: `weight` in `0..=1`, `gloss` in percent, and its own
  `ior` (1.5 by default). A weight of zero is exactly today's picture, sample for sample: no lobe is
  drawn and no extra number is taken from the sampler. A weight of one is a full dielectric layer over
  a base darkened by the coat's transmittance.
- **The owner's three words land in two places, not three kinds.** Veneer is a base with a decor.
  Lacquer is the coat — over a painted base or over a veneer. Patina is a **base**: a metal whose own
  albedo and roughness vary (one decor picture today; a roughness map when maps exist), because a coat
  is a clear dielectric and patina is the thing that is seen.
- **The gloss is a percentage and the roughness is derived.** Percent is the number a workshop says;
  the frame carries `alpha = (1 − gloss / 100)²`, the mapping V-Ray and Cycles use, computed once where
  the record becomes a frame, so the shader never sees a percent and the catalogue never sees an alpha.
  90 % is a lacquer, 50 % is a satin.
- **The coat costs no bytes and one lobe.** `GpuFace` keeps its 144 bytes: the coat's weight, alpha and
  ior go into channels that are spare (`colour.w`, `tint.w`, `uv_b.z/w`, `decor.y/z/w`), so no face
  straddles one more memory sector (`docs/performance.md` §2). The sampler is not widened either: the
  coat's discrete pick is a draw from the dimension the lobes already share.
- **The estimator rule.** The base's radiance is multiplied by the coat's transmittance
  `1 − weight · F_coat(cos)` and the coat adds `weight · F_coat` of its own GGX reflection at the
  coat's alpha; the four events (diffuse, conductor, coat, transmitted) carry probabilities that sum
  to one, so the model conserves energy by construction and a test measures it rather than trusting it.
  A dielectric Fresnel lobe over a base with no coat is **not** modelled: no furniture finish needs it,
  and it would be a second lobe where the record has one field.
- **The record carries both groups and no combination kind.** `veneer`, `lacquer` and `patina` are not
  three kinds of object in the renderer: a record is a base plus an optional coat, so the library grows
  with the surfaces a workshop names and not with the product of its lists. How a part names a base and
  a finish is the catalogue's decision (O17); the frame merges the two into one `Surface` before the
  tracer sees it, so composing costs no extra lobe.
- **Not decided here and not built:** anisotropy, sheen, a coloured coat, subsurface, dispersion,
  roughness and metal maps, and any stack deeper than one coat. Each is its own record when a picture
  asks for it.

## Consequences

- A board can be glossy at 90 % over a rough veneer: one surface, two roughnesses, one record. That is
  the whole point of the layer, and it is what the reference picture's lacquered miniature shows.
- The combinations stay additive: 30 veneers and 5 finishes are 35 records and 30 × 5 drawings, not
  150 records. The composition itself is O17's, and until it lands a material is one record that names
  its own coat.
- The noise grows only where a coat is thin — a weight between 0 and 1 splits the samples between two
  lobes — and not at all for the boards that carry no coat, which is most of a cabinet.
- Y9 builds the lobe, Y10 takes the same machinery into a pane's transmission. The pane keeps its own
  roughness and is never a coat: a frosted pane scatters what it transmits, where a coat reflects.
- Revisit if a finish must carry its own colour — a tinted lacquer, a patinated layer. Today a tint is
  a property of the base, and a tinted coat would be a base change, not a coat field.
