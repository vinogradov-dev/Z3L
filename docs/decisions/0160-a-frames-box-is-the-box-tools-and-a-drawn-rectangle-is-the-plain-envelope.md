# 0160. A frame's box is the box tool's, and a drawn rectangle is the plain envelope

- Status: Accepted; amends the creation gesture of 0101, refines 0138 and 0159
- Date: 2026-11-19

## Context

ADR 0101 made the frame and the box one tool *in intent* — draw a base, pull a height — but the two
drew their base twice. The box cell armed the sketch's own rectangle (`start_body_drawing`,
`ToolKind::Body`), with the sketch's plates, its typed width and depth, its Enter and its preview; the
frame kept a `BoxDraft` of its own, which mapped the pointer onto the plane for a live base rectangle
and labelled it in paint. The frame's height was a second card again: a title word, `mm` beside the
field and the ✓/✕ pair, where the solid's pull drew the one lettered `H` plate of ADR 0119.

The owner's report was that a furniture maker sees two tools where the program has one document shape.
The cost is not only looks: the frame's base could not be typed, so a 600 x 720 cabinet had to be
dragged to the millimetre before its first number was read.

## Decision

**The frame cell arms the sketch's rectangle, exactly as the box cell does.** `ToolKind::ProductBox(tool)`
carries the method, and `DrawIntent::ProductBox` says what the finished figure becomes; the plates, the
typed sides, the Enter and the preview are the sketch's, and the frame adds nothing beside them. The
draft that follows is `BoxDraft` reduced to what the pull needs — a fixed base and a height.

**The method decides the shape the figure reserves.** A figure drawn as `DrawTool::Rectangle` is the
plain box: its minimum corner is the frame's zero, its bounds are the width and the depth, and the
envelope carries no footprint, so the drawn frame is the same document the card writes and keeps the
eight corner grips a box has. Every other figure is kept as the ring it was drawn as (ADR 0159),
including a rectangle drawn with the free outline, because that method promises a shape and not a box.
`PathShape` cannot make this call: the tilted rectangle tags itself `Rectangle` while its bounds are
not its sides, so the tool is what the conversion reads.

**The pull is one plate.** The frame's height draws `ToolView::pull` — `H`, no unit, no button pair,
the gesture's Enter and Escape as its commit and its cancel — so a frame and a solid ask for their
height at the same place in the same words (ADR 0119, U104). The pull starts at the smallest height
rather than at zero, because a field may not show a number outside its own range and a commit at zero
is a refusal rather than a frame.

## Consequences

- The frame's base is typed: the W and H plates place the second corner with Enter, and the committed
  document is what the plates promised.
- `BoxDraft` loses its own clicks, its live corner and `BoxDraftError::FlatBase`; `BoxEvent` and the
  plane click of `box_gizmo::update_box` go with them. The shell's `update_box` reads the pointer along
  the plane normal and a click accepts the height.
- A contour frame drawn as a rectangle still stores a ring, and a box frame drawn as a rectangle stores
  no footprint. Two methods, two promises; the card edits whichever the piece carries.
- The titled plate with the ✓/✕ pair stays part of the card's anatomy and has no shipping user. A tool
  that wants an explicit pair still can; nothing has needed one since the pull became lettered.
- The pull still takes the keyboard only after a click (`Document::tool_focus` is unwritten). Both pulls
  need the same counted ask; that is one change and it is not made here.

## Related

- [0101](0101-a-frame-and-a-box-are-one-tool.md) — the gesture this completes for the frame.
- [0138](0138-a-solid-body-is-a-drawn-profile-swept-on-z.md) — the sketch draft a body is drawn with.
- [0159](0159-a-product-frame-reserves-a-footprint.md) — the ring a free outline reserves.
- [0119](0119-the-armed-tool-shows-its-parameters-at-the-pointer.md)
  — the card, its three commit states and the lettered plate.
