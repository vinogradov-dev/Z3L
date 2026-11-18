# 0151. The furniture move takes the piece by a click and follows the pointer

- Status: Accepted; supersedes the handle half of ADR 0147 — the furniture move draws no arrow and
  is not held by a button. Its frozen camera is narrowed by
  [0152](0152-a-piece-in-hand-blocks-nothing-and-the-view-moves-under-it.md)
- Date: 2026-11-18

## Context

ADR 0147 made the furniture move a tool of its own, with the free `Move` beside it, and gave its
directions to the document: a panel offers only the axis its own thickness points at, a frame the
axes its class allows. It drew those directions as arrows, as the free move draws its three, and kept
one gesture for both tools — press a handle, drag, release.

The owner used it and named the cost. A maker takes a **board**, not a three-point arrow: finding a
150 pt arrow on a shelf that is 18 mm thick, and then holding the button through a 900 mm slide, is
the long way round the most repeated action the shop makes. The arrows also answered a question
nobody asked — *which of the allowed directions?* — before the hand had said anything, and a refused
direction was a handle that was simply absent (ADR 0113), so a person looking for the missing arrow
could not tell a rule from a rumour.

## Decision

**The furniture move draws no handle at all.** Its directions are the document's answer, not an
affordance: a panel still offers only its own thickness axis, a frame the axes its mount allows
(ADR 0102, 0147, unchanged). Nothing is projected over the piece.

**A click takes the piece, the pointer carries it, a click puts it down.** The press that lands on a
movable body selects it and takes it in hand; from that frame on the piece follows the pointer with no
button held, and the next click commits the move as one `node.placement.set_many` — one revision, one
undo entry. The body is the grab region, so any part of the piece takes it. The click that puts a
piece down is the gizmo's and never a pick, so the node under it is not reselected.

**The axis is the one the hand's travel names, latched once.** Among the axes the tool may travel,
the one whose projected screen direction lies nearest the pointer's travel from the point the piece
was taken wins, skipping the axes that point at the camera exactly as the handles do. It is latched at
the first movement past four points and kept for the hold, so the piece never changes direction under
the hand; the sign comes from the ray's projection along that axis, so a piece follows the hand in
both directions. A piece with one direction — the board, the shelf, the divider — is taken in that
direction whatever the hand does.

**Escape, the mode and the camera answer the hold.** Escape puts the piece back and writes nothing;
leaving the furniture move cancels the hold the same way, because a hand reaching for another tool has
not said where the piece goes. The camera is frozen while a piece is in hand — narrowed by
[0152](0152-a-piece-in-hand-blocks-nothing-and-the-view-moves-under-it.md), which lets the view move
and re-anchors the hold instead.

**Everything else the tool had, it keeps.** The document's rule still cuts the distance: contact,
coordinate planes, frame faces and the 1 mm grid (ADR 0102, 0103, 0146). The exact-distance fields
still type the number — they appear on the axis the hold latched, which is the one a typed gap is
measured along (ADR 0103). The free `Move` keeps its three arrows and its held drag unchanged.

## Consequences

- The tool reads like a piece of furniture in a hand: take it anywhere, put it down where it goes.
  The cost is a gesture with two clicks and no drag, so a hand trained on the arrows has to learn
  that the piece moves *after* the click, not while the button is down.
- A board may only travel along its thickness, and the pointer may travel anywhere: the two in-plane
  directions of the hand are discarded by the document, not refused by an absent arrow. A maker who
  pulls a shelf sideways sees it rise or fall by what the pull had along the axis, which is the same
  answer the ruled drag gave and needs no new rule.
- The exact-distance fields appear one frame late — on the axis the hand named, never before it. A
  piece taken and put down without travelling costs nothing: a zero delta is no command.
- A camera, a light and a loose body inside a product may be taken too, because the tool is the one
  that says what a body may do; outside a product there is no contact and no snap to obey, so the
  move keeps the document's micrometre and only its direction is ruled (ADR 0147, unchanged).
- The furniture move is no longer discoverable by looking: the band's cell and its hover say what a
  click does, and the status bar says what ends the hold while a piece is in hand. A pointer shape of
  its own is not introduced here; the work cursor is one square for the whole work area (ADR 0121).
- The rest of ADR 0147 stands: the fourth mode exists, the free move applies no rule, the two moves
  sit side by side, and a selection offers what every one of its nodes offers.

## References

- `app/src/translate_gizmo.rs` — `Hold`, `Drag`, `TranslateGizmo::grab`, `begin`,
  `continue_follow`, `paint`.
- `app/src/gizmo_math.rs` — `named_axis`, `axis_handles`, `VIEW_ALIGNMENT_LIMIT`.
- `app/src/document.rs` — `take_furniture`, `holding_piece`, `carried_delta`, the click that takes.
- `app/src/status_bar.rs` — `StatusInfo::holding`, `Message::FurnitureHold`.
- `crates/text/src/messages.rs` — `FurnitureHint`, `FurnitureHold`.
- ADR [0113](0113-select-is-the-state-a-window-opens-in.md) — the mode is view state, and an armed
  handle that refuses is worse than none.
- ADR [0147](0147-the-furniture-move-is-its-own-tool-and-a-panel-slides-along-its-thickness.md) — the
  tool this changes, and the rule it keeps.
- ADR [0102](0102-a-furniture-entity-is-a-surface-a-frame-is-a-shell.md),
  [0103](0103-one-probe-serves-the-niche-the-distances-and-the-move.md),
  [0146](0146-snapping-is-a-workspace-setting-and-the-coordinate-planes-stand-in-for-walls.md) — the
  rule, the probe and the snapping the carried piece still obeys.
- Plan: N24, whose note (`plan/notes/N24.md`) records what was measured.
