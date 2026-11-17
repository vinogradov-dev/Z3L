# 0142. The ground shadow is the silhouette the key light casts on a floor plate

- Status: Accepted; supersedes 0071
- Date: 2026-11-17

## Context

ADR 0071 drew one soft ellipse per floor-standing object, at 92 % of the object's footprint, on the
ground plane. It was invisible, and the measurement of 2026-11-17 says why: an ellipse inside the
footprint of a body that stands on the same plane is covered by that body — every pixel of it. The
only thing it ever coloured was the sub-millimetre gap the floor tolerance allowed, so lifting an
object by a millimetre removed its shadow and lifting it by four tenths made one appear. Two further
reasons: a body that rests on the floor is rare in furniture, where almost everything hangs or sits
inside something; and black over a near-black background darkens nothing, so at 1.6× the footprint
and full strength the ellipse was still invisible on the dark theme.

The owner's purpose for the feature was never "a shadow exists": it was volume, comfort and a scene
that does not read as flat emptiness. A blob under the body cannot deliver that, and neither can any
choice of strength or colour while the floor is the clear colour.

## Decision

**A shadow is a silhouette, not a footprint.** The frame's drawables are projected along the key
light onto the floor plane into a coverage mask. The projection is the only geometry rule: a point at
height `z` lands at `xy - light.xy * z / light.z`. Nothing asks whether a body touches the floor, so a
body at any height casts, a turned body casts its own shape, and the shadow always starts beside the
body rather than under it.

**The mask is coverage, not a depth map.** The receiver is one plane and the question is only whether
something stands between the light and a point of it, so the mask pass has no depth attachment and no
comparison — which is why it has no bias, no acne and no "peter-panning" to tune. It is one
single-channel texture of 1024² over the plate, a megabyte, and two separable blur passes give the
penumbra a world size of 60 mm instead of a screen-space one. A mask is rebuilt when the scene is
uploaded or the key light turns, and never when the camera moves: it is world-space.

**The floor is a plate, because a shadow needs a surface.** One unlit quad on `z = 0` across the
square the grid draws, in `SceneColors::floor` — a few steps above the background in the dark variant
and above the paper in the light one. It is drawn single-sided so a view from below keeps the cabinet
instead of losing it behind the ground, and it carries the shadow in its own fragment entry: the
shadow costs one texture fetch inside a pass the frame already draws, not a pass of its own and not a
blended layer over the scene.

**Both are frame data.** A plate has no thickness, no material, no outline and no machining, and it
must not appear in the tree, in undo, in a pick, in *Frame all* or in an exported order. If the
product ever needs a floor that is a design decision of the maker — a room, a floor board that appears
in a drawing — that is a document entity with its own record, not this plate.

**A strength, not a switch alone.** `ContactShadows` keeps its switch and gains its meaning: the
strength is how far the plate darkens under a body, read at draw time, so a slider costs no upload.
A key light that shines from below the floor zeroes the strength, because a shadow cast upward is not
a thing the plate can show.

## Consequences

A cabinet stands on the floor and reads as standing on it, at one texture of a megabyte, one mask
rebuild per edit, and one texture fetch per plate fragment. `ContactShadow`, its falloff constants,
its triangle fan and the two boxes it needed per subtree are gone, and `plan/debt.md` no longer
carries the rewritten box of a turned body as a shadow defect.

What it does not do: the shadow falls on the ground and nowhere else, so the inside of a cabinet is
still lit by faces that do not occlude each other, and a shelf casts nothing on the back panel. That
is ADR-worthy work of a different size and is scheduled as `L1`, which samples a real map in the face
shader. Nor does the penumbra grow with the shadow's distance from its caster, which is what a
physically honest soft shadow does and what a single two-pass blur cannot; the effect is visible on a
tall body's far end and is accepted for now.
