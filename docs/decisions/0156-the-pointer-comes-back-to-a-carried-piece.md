# 0156. The pointer comes back to a carried piece

- Status: Accepted; supersedes ADR 0155
- Date: 2026-11-18

## Context

The owner tested the carry again with a sequence of his own: take a panel, put it 200 mm from its
neighbour, then pan the view a long way with the right button and come back to the carry. The panel
was far from the pointer, and the two fixes before this one were both wrong in the same way — they
kept the *piece* attached to the pointer instead of the pointer to the piece:

- ADR 0153 measured the carry by the hand's travel, which is right for a hand and blind to the eye.
- ADR 0155 then kept the piece's *screen* place, so a camera change slid the piece along its axis.
  A maker who pans, orbits or zooms while carrying then reads a gap that changed for no reason they
  can see, and the distance they set — 200 mm — is no longer 200 mm.

The owner's own words for the rule: a panel set 200 mm from another panel must stay 200 mm away while
the view moves, so after a camera change the *cursor* has to come back to the piece; and while the
view is being turned that cursor should be hidden.

## Decision

**A view gesture writes nothing, and the pointer returns to the piece.** Two halves:

1. A camera gesture is the eye's: while one is running the carry is *held* (`TranslateGizmo::
   continue_follow` applies no distance), and the piece keeps the millimetres it has. The document
   does not even update the gizmo for a gesture, so nothing can leak through.
2. On the first frame after the camera has moved — a gesture that ended, a wheel, a preset, Home — the
   carry asks the window to put the pointer back on the piece: `ViewportCommand::CursorPosition` at
   `screen(pivot) − offset`, the bond it holds the piece by, read the other way. The carry is held
   meanwhile, so the frame that carries the request out moves nothing.

The pointer is hidden for the length of a gesture (`ViewportCommand::CursorVisible`), because a cursor
standing still while the world slides under it is the picture of a piece being left behind — and the
opposite is true.

A window may drop the command and a platform may refuse it (a Wayland session cannot move the
pointer): the request is re-sent every frame and abandoned after `HOME_TRIES`, and the carry then
re-reads its offset from the pointer where it is. Nothing jumps in that fallback — the piece keeps
its millimetres and the hand carries on from where the pointer stands.

The grab offset is read from the click, in `TranslateGizmo::begin`, and not from the frame the axis
is named: by then the hand has already travelled past `NAMING_TRAVEL`, and an offset read there
swallows that travel and leaves the piece standing still.

## Consequences

- A maker can lay a panel down to the millimetre, look around, and find the pointer still on the
  panel; the number in the card is the number they set.
- The pointer jumps back by the distance the gesture travelled. That is the accepted cost of the
  rule, and it is why the cursor is hidden while the gesture runs.
- The carry's distance is a screen bond against the pointer, so a hand that moves carries the piece
  1:1 along its axis and a hand that moves across that axis carries nothing (ADR 0153's own rule
  falls out of it).
- A piece the new view pushed out of the window has nowhere to ask for, so the carry keeps its
  distance until the maker brings the piece back into view.
- Every surface that zooms still goes through `zoom_about_pointer` (ADR 0154), so a wheel turn keeps
  the point under the pointer where it is and rarely needs the pointer moved back at all.
