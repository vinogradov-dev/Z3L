# 0156. The pointer comes back to a carried piece

- Status: Accepted; supersedes ADR 0155; the window-carried pointer of the first draft is replaced by
  the carry's own hand (X69)
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

The pointer comes back to the **middle of the piece**: the centre of its world box, moved by the
carry, which is where a maker reaches for a panel. Not the pivot — a panel's pivot is its base corner
— and not the grab: the point a click made on a face is not a place anyone can find again.

Three defects of the first attempt, all found by the owner reporting that nothing had changed:

- The hiding read the document's *armed navigation mode* instead of the input of a camera move, so a
  right-button pan under a carried piece hid nothing. It now reads the middle button, the right
  button and the wheel, and re-sends `CursorVisible` every frame the pointer stays hidden.
- A gesture that ended never asked for the pointer back: the request was gated on the camera having
  moved on *that* frame, and on the frames it moved the gesture was still running. The gizmo now
  remembers `moved_by_gesture` and asks on the first frame after the gesture ends.
- The middle was projected from the document's box, without the carry: the pointer flew to where the
  piece stood when it was taken, not where the hand holds it.

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

## The mechanism had to change: this session is Wayland

The first draft asked the *window* to put the pointer back (`ViewportCommand::CursorPosition`) and to
hide it (`CursorVisible`). Both were wrong on the desktop the owner works on:

- Wayland cannot move the pointer. `winit`'s `set_cursor_position` answers `NotSupported` unless the
  pointer is *locked*, so the request was dropped and the pointer never came back — three reports of
  "nothing has changed" were this one fact.
- `CursorVisible` fights `egui-winit`, which shows the cursor again whenever the icon changes, and the
  icon changes every frame the scene is drawn. It also fought the app's own rule that the work area
  draws its own square and keeps the system arrow away (X40).

So the carry keeps a **hand of its own**:

- The hand is the system pointer while it tracks the mouse, so ordinary carrying is unchanged.
- A camera gesture under a carried piece captures nothing and draws no pointer: the square is simply
  not painted for the length of the gesture.
- On the first frame after the camera moved, the hand is put on the middle of the piece (the box
  centre plus the carry), and the piece—hand bond is re-read there, so the frame carries nothing.
- From then on the hand moves by the mouse's *own movement*, so dragging still carries the piece 1:1
  from the piece; the work cursor paints the square at the hand, and the exact-distance card hangs
  from it. The system pointer stays where the desktop put it, which costs one thing: a press is
  hit-tested at the system pointer. In the work area that is the click that puts the piece down
  anywhere, so the cost is invisible; over a panel the press belongs to the panel, as it should.
