# 0152. A piece in hand blocks nothing, and the view moves under it

- Status: Accepted; narrows ADR 0151, whose hold froze the camera
- Date: 2026-11-18

## Context

ADR 0151 took the piece by a click and carried it with no button held, and — copying the rule of a
handle drag — froze the camera for the length of the hold: `TranslateGizmo::is_dragging` was one
answer for both tools, and the orbit, the pan and the wheel all read it.

The owner used it: a piece in hand is exactly when a maker needs to turn the case to see where the
shelf goes, and the gesture refused. A drag with a button down must own the pointer, because the
axis and the parameter under the hand are read through the camera; a hold that owns no button has no
such reason. It was a state that had borrowed another state's caution.

## Decision

**The hold never freezes the view.** Orbit, pan, the wheel, a folded panel and a moved split all work
while a piece is in hand, and the hold re-anchors instead of dragging the piece along: it keeps the
distance it already has and the hand continues from wherever the pointer ended up.

**While the camera is being driven, the piece stands still.** The pointer travels to turn the view,
and none of that travel belongs to the piece, so every frame of a middle-button drag, a right-button
pan or a wheel turn re-anchors the hold at the pointer of that frame. After the gesture the piece is
where it was and the next movement of the pointer is a movement of the piece again.

**A hold is not a mode.** Anything that takes the pointer or changes the document ends it and puts
the piece back, writing nothing: a draft, a pull, a grip, the ruler, the niche, a locked node, and any
command that lands under the hand — an undo, a delete, a rename from the tree. The document revision
the targets and the rule were read from is stored with the hold, so one comparison covers every
command there is and there is no list of entry points to keep in step.

**A held handle drag keeps its old rule.** The free `Move` holds a button, so the camera stays frozen
under it: the axis, the pointer's parameter and the capture are all read through the camera, and an
orbit mid-drag would move the piece under the hand.

## Consequences

- The two moves differ in one more way than the arrows: the furniture move leaves the whole viewport
  usable, and the free move owns it until the button is released. The band and its hover already say
  which tool is armed, so this is a property of the gesture and not a hidden mode.
- A carried piece cannot be moved *and* have the camera turned in the same motion, which is the
  honest answer: the pointer is one, and the camera gesture has it.
- The primary button still belongs to the hold — a click puts the piece down — so a rubber band
  starts after the piece is down and not instead of that click.
- The distance fields of a held piece are chrome around the hand, so a carried piece keeps the
  pointer "in the gesture" over them (the shell's `capturing`), exactly as a handle drag does.
- A revision change under the hand is detected on the next frame and the hold ends there: the piece
  goes back to the placement the document now has, which is the state undo or delete left behind.

## References

- `app/src/translate_gizmo.rs` — `Drag::revision`, `Drag::view`, `View`, `TranslateGizmo::is_held`,
  `cancel`, `continue_follow`.
- `app/src/document.rs` — `dragging` (the camera gate) reads `is_held`, `capturing` reads
  `is_dragging`, and the tool list that cancels the hold.
- `app/src/frame_tests.rs` — `a_piece_in_hand_blocks_no_view_and_the_view_moves_no_piece`,
  `a_tool_or_a_command_under_the_hand_puts_the_piece_back`.
- ADR [0151](0151-the-furniture-move-takes-the-piece-by-a-click-and-follows-the-pointer.md) — the
  gesture this narrows.
- ADR [0147](0147-the-furniture-move-is-its-own-tool-and-a-panel-slides-along-its-thickness.md) — the
  tool and its rules, unchanged.
- Plan: N25, whose note (`plan/notes/N25.md`) records what was measured.
