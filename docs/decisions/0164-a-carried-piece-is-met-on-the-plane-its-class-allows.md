# 0164. A carried piece is met on the plane its class allows

- Status: Accepted; supersedes the camera-plane carry of [0163](0163-the-furniture-move-carries-on-every-allowed-axis.md)
- Date: 2026-11-19

## Context

ADR 0163 gave the furniture move every axis a piece's class allows and met the pointer on **the plane
facing the eye** through the pivot at the take. That plane is right for a piece that may travel
anywhere: a delta in depth is a delta such a piece can take. It is wrong for a piece whose class
forbids one axis. A floor-bound frame may only move across the floor, so the rule keeps the horizontal
part of a delta measured in a plane turned away from the floor, and the piece follows a foreshortened
copy of the hand: in the reported gesture the pointer travelled 120 points across the screen and 80
up, and the frame moved 113 and 21. The owner's word for it was *sluggish* — the cursor ran away from
the piece — and a carry that loses four fifths of the hand's depth travel is not a carry.

The second half of the same report is the same mistake in the overlay. The corner brackets of the
level that is open were collected from the frame the document still holds, so a carried frame left
eight corner marks at the place it was taken while the piece itself followed the pointer.

## Decision

**The carry plane is the plane the piece may really travel in.** Two allowed axes are met on the plane
across the axis the class refuses — the floor, for a floor-bound product — and only a piece whose three
axes are free (a camera, a light, a body) is met on the camera's own plane. The plane is read at the
take, from the same axes the rule is built from, and it is held for the whole gesture. A one-axis piece
is unchanged: a panel keeps the hand-travel latch and its exact distance (ADR 0153).

**The piece hangs from the hand by a screen offset, not by a world difference.** The click lands on
the body, so the pivot sits at an offset from the pointer; that offset is kept in points and the
pointer's own place, moved by it, is what meets the plane every frame. The piece's projected pivot is
therefore exactly the hand plus the offset the click gave it, on every frame of the carry — which is
what "the piece keeps the hand" means to a person. Measuring the offset in millimetres instead does
not survive perspective: the plane meets the eye at a slant, so a world delta that matches the pointer
at the take drifts as the piece changes depth, and the piece slides out of the hand towards the
horizon.

**The plane's own point is taken in front of the eye.** A plane the ray runs along, or one behind the
camera, has no point under the pointer, and the answer is `None`: the carry keeps the distance it
already has rather than throwing the piece at a point at infinity. A proposed delta is bounded by the
same maximum distance the one-axis carry takes.

**The overlay reads the drag's preview.** The corner brackets of an open level, and of an open
product's authored box, are collected with the drag the gizmo is previewing: a carried level is
bracketed where the preview draws it, the marks travel with the piece in hand, they come back on
Escape, and a rotation preview turns them the same way. The brackets remain view state as before —
nothing is written, and a carry still evaluates no node.

## Consequences

- A floor frame carried diagonally keeps the hand on both floor directions; a wall frame keeps it
  while it rises, because the camera's plane is the plane a fully free piece travels in. The class now
  reads in the gesture twice: in the axes the rule allows and in the plane the hand meets.
- The pointer is two-dimensional, so a piece carried on a plane cannot be moved along the plane's
  normal. For a floor piece that normal is the axis its class refuses anyway; for a free piece it is
  the depth of the well-known plane gesture, and a maker turns the view to move in it.
- The bracket preview follows the same `Preview` the renderer takes, so the picture and the marks
  cannot drift apart, and a camera or a light drag keeps turning the boxes it carries.
- A snap or a contact still moves the piece off the hand for as long as the snap holds: the rule is the
  document's answer and the hand is only a proposal, exactly as it was before.
- Nothing new is written: the release is still one `node.placement.set_many`, one revision and one
  undo entry.

## References

- `app/src/gizmo_math.rs` — `CarryPlane`, `plane_point`, `Preview::moved_point`.
- `app/src/translate_gizmo.rs` — `Follow::Plane`, `Hand::offset`, `continue_follow`, `bounded`.
- `app/src/open_frame.rs` — `OpenBoxes::collect`, `carried_corner`.
- `app/src/document.rs` — the overlay is collected with the live preview.
- `app/src/frame_tests.rs` — `a_floor_frame_keeps_the_hand_on_both_floor_directions`,
  `a_frames_class_decides_whether_it_may_be_hung`,
  `a_frames_brackets_travel_with_the_piece_in_hand`.
- ADR [0163](0163-the-furniture-move-carries-on-every-allowed-axis.md) — the axes, the per-axis rule
  and the sliding contact this keeps.
- ADR [0153](0153-a-carried-piece-is-measured-by-the-hand-not-by-the-ray.md) — the screen bond the
  one-axis carry already used and this carries further.
- Plan: X75.
