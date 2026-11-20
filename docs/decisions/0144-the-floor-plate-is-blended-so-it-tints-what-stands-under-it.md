# 0144. The floor plate is blended, so it tints what stands under it and keeps its shadow

- Status: Accepted; supersedes the plate's place in the pass and its opacity in 0142
- Date: 2026-11-17

## Context

ADR 0142 drew the floor as an opaque unlit plate at the front of the view pass, so the shadow it
carries costs no pass of its own. U108 then made the plate write no depth: a part standing under the
floor was no longer culled behind it, and it drew *over* the plate instead. That is not a floor — an
opaque surface was letting a body behind it through, untinted, and the owner read it as a bug: below
the floor you saw the part, not the part through the floor.

What was missing is the tint. A plate that writes no depth can be blended over whatever stands under
it, but only if it is drawn *after* that body — and the plate was drawn first, which was the one
thing ADR 0142 chose about its place in the pass. Blending it opens a second question: an opaque
plate at 40 % would wash its own shadow into the background, and the shadow is the reason the plate
exists at all.

## Decision

**The floor has an opacity, and it is workspace data.** `ViewportSettings::floor_opacity` is a whole
percent, 0 to 100, defaulting to 100 so a workspace that never touches it draws the floor it always
drew; *Reset display* restores it beside the three display strengths. It reaches the frame as
`SceneFrame::floor_opacity` and the renderer writes it into the camera uniform the plate already
reads — one more `f32` slot, because `shadow_plane` is full. A drag of the slider therefore writes
one uniform and uploads no buffer, exactly like the shadow's strength and the grid's ink.

**The plate moves to just after the opaque bodies, and is alpha-blended.** That is the smallest
move that lets it tint: the opaque pass is the only one that writes depth, so the plate must precede
every pass that does not — blended faces, the line chains, the edges, the envelopes. Drawn after a
ghost, an opaque plate would paint the floor over the whole model. It still writes no depth, so it
still cannot erase what it tints (U108), and the grid stays on its surface, drawn immediately after
it.

**The plate's alpha is `max(opacity, cover · strength)`, and zero is special.** The fragment mixes
the mask into the plate's colour — the mix fraction does not change with the opacity, so the shadow
is not diluted by it — and the alpha is the larger of the plate's own opacity and the shadow's
weight: a shadow stronger than the opacity keeps its own darkness instead of being blended away, and
a floor at 100 % reduces to the old formula exactly. Below one, however, `max()` would still draw a
shadow over an invisible floor, so an opacity of zero draws nothing at all: a shadow needs a surface
to darken.

**The shadow is not a second blended layer.** Two layers over one receiver would be either a second
pass with its own blend, or a second colour attachment, or a pre-composited copy of what is behind
the plate — which a fragment cannot read. One blend carrying the larger alpha is the cheap honest
approximation, and its cost is stated below.

## Consequences

At 100 % the picture is the one ADR 0142 produced with alpha 1, pixel for pixel: the mix is the old
mix. Below 100 % a part below the floor is seen through the tint, and where the shadow is stronger
than the opacity the floor stays as dark as it was — which is what the owner asked for. At 0 % the
plate and its shadow are gone.

What it costs: an opaque plate now *hides* a part that stands under it, because that is what opaque
means — the pass-through of U108 was a side effect of the plate being drawn first, and it is gone.
The form outlines of such a part still show through, because the plate writes no depth (U108), so an
underground object reads as a wireframe ghost rather than a solid one. A blended display style draws
its ghosts over the plate, since they are blended after it; the tint is a property of the opaque
scene, which is the one the owner judges a cabinet in.

Revisit if a shadow on a see-through floor needs to be exact rather than approximate: that wants the
plate and its shadow as two layers, and two layers want a pass or an attachment.
