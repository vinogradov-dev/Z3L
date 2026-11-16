# 0123. Work-area paint sits under the chrome

- Status: Accepted
- Date: 2026-11-14

## Context

X41 confined the custom square and the crosshair to the scene rectangle but left them on the
`Tooltip` layer, so the crosshair crossed the floating side cards and the square could paint over
their edge. The owner chose the simple z-order correction over clipping the guide around every
panel: the guide and the pointer are work-area paint, and the chrome is above the work area.

## Decision

**The square and the crosshair are painted on the scene layer, under every chrome surface.** They
use a non-area `Background` layer created after the central panel, so they lie above the scene image
and its gizmos, and below every `Middle` panel, ribbon, cluster and command bar and every
`Foreground` menu or modal. The scene clip stays: they are still only drawn inside the work area, and
the pointer still has to be over the scene.

A translucent panel lets the guide show through its fill; that is the accepted cost of the layer
choice. Clipping the crosshair around each panel (no show-through at all) is a separate change, not
a silent follow-up.

## Consequences

A panel's text and controls can never be crossed by the guide: the panel masks it. A pointer near a
card edge can have part of its square masked, which is correct — that part is no longer the work
area. The paint layer is below menus, so a menu also masks both. A new floating surface is already
`Middle` and needs no cursor or guide code; a new guide that must sit above chrome would need its own
decision, because this record puts work-area paint below it.

## References

- `app/src/work_cursor.rs` — `layer`, `paint`, the scene clip.
- ADR [0121](0121-the-pointer-is-a-two-tone-square-above-the-chrome.md) — the outline and the layer
  this record changes.
- ADR [0122](0122-the-custom-pointer-belongs-to-the-work-area.md) — the scope this record layers.
- Plan: X42.
