# 0143. The shadow map is the key light's depth pass, read by every face

- Status: Accepted
- Date: 2026-11-17

## Context

ADR 0142 gave the frame a shadow on the floor: the scene projected along the key light onto a plate
and blurred. That answered "what stands on the ground" and nothing about the inside of a cabinet.
Every joint between two surfaces was lit as if the two panels met at an infinite plane, so a shelf, a
niche and a drawer front read as flat paint — the model had the shape of furniture and the light of a
diagram. The furniture programs that convince here light the model from one sun and let every surface
receive it.

The ground's mask cannot be reused for this. It is a *coverage* render into one plane, which works
precisely because the receiver is a plane and the question is "does anything stand between the sun
and this point of the floor". A panel is any plane at any angle, so the question becomes "what is the
nearest surface along this light ray", which needs a depth.

## Decision

**One depth map from the key light over the scene's box.** The box is the union of the drawables'
world boxes, so it holds both the caster and the surface it darkens, and the projection covers the
sphere around that box: a 2048² depth texture, `Depth32Float`, 16 MB, about a millimetre and a half
per texel for a two-metre cabinet. The sphere rather than the light-space box keeps the map square in
the light's own axes, so turning the sun changes the picture and not the resolution.

**It is rebuilt on the same keys as the ground mask and never on a camera move.** A scene upload and
a turned key light redraw it; orbit, pan, zoom and a hundred idle frames cost nothing, because a
shadow map is world-space geometry. Both keys quantise the light through one function, so the floor
and the panels can never disagree about where the sun is. The pass reuses the world-space vertex
buffers the upload already built: no new geometry, one depth-only pass, no second vertex layout.

**Bias is the design, and it has two parts.** A receiver is sampled at a point moved along its own
normal by a texel-and-a-half of the map's world size — scaled up at grazing angles — so a face is
compared with a point in front of itself rather than with its own depth; a depth bias of one and a
half texels plus a slope term covers what the offset cannot. The comparison is `LessEqual`, which is
what decides the two coplanar cases: a band lying on a panel face and a shelf whose end meets a side
face exactly are at the caster's own depth, and a surface equal to the map is **lit**. Both cases have
a pixel test, because a bias that leaks draws a dark seam under every band and along every shelf in
every cabinet.

**One strength, one switch, one sun.** The shadow the plate draws and the shadow a face loses are the
same light, so they share the workspace's strength and the view's switch (U107). A shadow does not
merely paint the plate: it removes the key light's diffuse and specular term from the surface, leaving
the ambient and the point lights, so a shadowed face darkens by the strength instead of turning to
paint. A strength of zero therefore gives back the flat shading the frame had before, from one slider.

**No cascades, no second sun, no screen-space AO.** One cabinet is two metres across, so one map over
the whole scene is finer than a cascade would be at every level and costs one pass. A second authored
sun is not a thing the document has (ADR 0036). Screen-space AO cannot darken a surface that nothing
covers — which is exactly the niche and the shelf.

The map carries no penumbra of its own: the 3×3 comparison kernel spans two texels, about three
millimetres, so the shadow is the sun's hard silhouette. The ground keeps its 60 mm blur because a
contact shadow on a floor reads better soft; a face's shadow at three millimetres of softness reads as
a line, which is what it is.

## Consequences

A shelf darkens the back panel behind it, a niche reads as a volume, and a turned drawer front lights
its own side. The cost is one depth pass per edit or turned light, one 2048² texture, and nine
comparison taps in every face fragment — the first pass in the renderer that can cost a frame, which
is why the rebuild is keyed and probed by a test that counts it.

What it does not do: only the key light casts. A point light still lights through a panel, and a
shadowed face keeps its ambient term, so a closed cabinet's inside is lit by ambient rather than black.
The shadow is the nearest surface along one direction, so a translucent display style receives it as
if it were opaque. And the map holds one resolution for the whole scene: two cabinets a workshop apart
share the same 2048 texels, and framing one of them does not sharpen the other. Those are the prices
of `plan/notes/L1.md`; the alternatives were measured there and refused.
