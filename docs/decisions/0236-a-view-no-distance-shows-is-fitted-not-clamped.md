# 0236. A view no distance can show is fitted, not clamped

- Status: Accepted
- Date: 2026-11-23

## Context

[0049](0049-the-viewport-toggles-its-own-projection.md) made a projection switch keep the visible
height at the target: an orthographic view becomes perspective with the default field of view and
the distance that shows the same height. The distance was then clamped into
`MIN_DISTANCE_MM..=MAX_DISTANCE_MM` without a word. Above the cap — 828 427 mm of visible height at
the 1 km limit — the promise broke silently: the camera stood at the cap and showed another frame
than the one the maker had.

The owner reported the switch leaving the camera far away and unable to come back ("словно движется
по микрометрам"), which is the wheel's own defect (0237), but the clamp is what made a camera stand
a kilometre out in the first place, with nothing on screen saying why.

## Decision

**`Camera::toggle_projection` returns what it did with the height.** `ProjectionToggle` is one of
three:

- `Kept` — the projection changed and the visible height at the target is the one the view had;
- `Pinned` — the camera is an authored basis, which the viewport may not switch;
- `NoDistance { height }` — no distance the camera supports shows that height, so nothing moved.

A height within a billionth of a limit counts as that limit, because the height of a view at the cap
is a multiplication away from the distance that shows it and a rounding step is not a refusal.

**The document repairs a refusal by fitting the view on the scene.** `Document::toggle_view_projection`
calls *Frame all* — which keeps the view's direction, preset and projection — and switches again: a
fit height always has a distance, so the second switch is `Kept`. The bar says the view was refitted
for the same six seconds a followed fit says so (`Message::StatusRefitted`), because a frame that
changed has to say why.

`Camera::visible_height` is the new public read of the one number both projections agree on.

## Consequences

- No switch leaves the camera on a limit it did not ask for. A view that lost nothing says nothing;
  one that was refitted says it was.
- A refusal costs the maker their zoom: the view is fitted on the whole scene. That is deliberate —
  the alternative is a frame that silently differs from the one they had.
- A navigable view cannot reach a height no distance shows, because the wheel stops at the same cap,
  so the repair is a guard rather than a path the maker walks. It exists because a projection can be
  written from outside the viewport (`Camera::with_projection`) and because the cap belongs to the
  type, not to the scene.
- The wheel is what brings a distant camera back; that is 0237.
