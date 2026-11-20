# 0163. The furniture move carries a piece on every axis its class allows

- Status: Accepted; supersedes the single-axis latch of [0151](0151-the-furniture-move-takes-the-piece-by-a-click-and-follows-the-pointer.md)
  and narrows the one-axis carry of [0153](0153-a-carried-piece-is-measured-by-the-hand-not-by-the-ray.md);
  its camera-plane carry is superseded by [0164](0164-a-carried-piece-is-met-on-the-plane-its-class-allows.md),
  and its world-box contact for a body that is not furniture is narrowed by
  [0165](0165-the-move-tool-is-the-object-move-and-a-solid-body-is-measured-by-its-faces.md)
- Date: 2026-11-19

## Context

ADR 0151 gave the furniture move its gesture — a click takes the piece, the pointer carries it, the
next click puts it down — and latched **one** allowed axis from the hand's travel. That is right for a
board: a shelf travels in its thickness and the two in-plane directions are not a direction at all
(ADR 0147). It is wrong for a frame. N22's own Done says a frame moves on three world axes and a floor
frame keeps its height, but the carry made that three axes **one at a time**: a base cabinet cannot be
moved across the floor in one gesture, and a wall cabinet cannot be moved in the plane it hangs on.

The owner reported the second half of the same defect: changing the class in the inspector seemed to
change nothing about the move. It did change the axes the rule allows — a floor class refuses the
vertical and a wall class does not — but under a one-axis latch both pieces still move along one line,
so the class only chose which line the hand could name. The move has to show the rule, not hide it.

## Decision

**A piece free on more than one axis is carried on the camera's own plane through its pivot at the
take.** The pointer meets that plane at one world point, and the world difference from where it took
the piece is the proposed delta. The hand still carries the piece with no button held, the next click
still puts it down, and a camera gesture still moves no piece (ADR 0156): the carry is held while the
eye moves, the plane turns with the camera, and the grab is read again so the frame that ends the
gesture carries nothing.

**The document answers the proposal axis by axis.** `MoveRule::limits` measures each world axis with
the same snapshot the single-axis `limit` uses: a disallowed axis is zero, an allowed axis snaps to the
nearer coordinate plane, frame face or part face, and is cut at its first contact. A face that stops X
therefore leaves Y and Z carrying the piece, and the piece **slides along the face** instead of stopping
dead. The floor-bound class keeps the vertical at zero by the same rule that refused it before; it is
the same `Mount` trait, read where the rule is read.

**A one-axis piece keeps the old carry.** A panel still offers only its thickness, so the hand's
travel names that axis and the hand's own distance along it is the move (ADR 0153). The exact-distance
fields and a typed gap stay exactly as they were there.

**The fields follow the dominant axis of a multi-axis carry.** A piece moving on a plane has no single
gap to measure, so the fields measure the allowed axis the carried delta travels furthest along, and a
typed number pins that component while the other axes stay the hand's (ADR 0103). The rule cannot move
a face that was typed, exactly as before.

**The class is read at the take and never cached across one.** Changing `class` in the properties card
changes `Mount` on the next take, which changes both the axes and the vertical rule; nothing about a
carry is stored in the document.

## Consequences

- A floor frame crosses the floor in one diagonal gesture and keeps its height; a wall frame follows
  the hand on the camera's plane and may rise. The class now reads in the gesture itself.
- The pointer is two-dimensional, so a plane carry moves in the plane facing the eye: to move a piece
  in depth the maker turns the view, as with every other plane gesture in the editor. The piece is
  never dragged by the camera change itself.
- Contact is resolved per axis, which is what sliding means. The rule still measures the start boxes,
  so a diagonal drag into a corner can pass a corner the axis-aligned test does not see; measuring a
  swept volume is the larger change and nothing asks for it yet.
- Two frames that meet flush now slide across each other's face instead of one stopping the other;
  the write guard of ADR 0162 still refuses a placement that would come to rest inside a part.
- The typed gap remains one number on one axis. A carry diagonal enough to change its dominant axis
  re-measures the field, which a maker sees as the card moving to the other gap.
- A camera or a light, which carries no box and so has no rule, moves freely on the camera plane.
- Nothing new is written: the release is still one `node.placement.set_many`, one revision and one
  undo entry.

## References

- `crates/model/src/project/move_query.rs` — `MoveRule::axes`, `MoveRule::limits`, `MoveAxis`,
  `Project::move_rule`, `Project::furniture_axes`.
- `app/src/translate_gizmo.rs` — `Follow`, `Follow::Plane`, `continue_follow`.
- `app/src/gizmo_math.rs` — `view_plane_through`, `dominant_axis`, `planar`.
- `app/src/frame_tests.rs` — `a_floor_frame_is_carried_across_the_floor_in_one_gesture`,
  `a_frames_class_decides_whether_it_may_be_hung`.
- ADR [0147](0147-the-furniture-move-is-its-own-tool-and-a-panel-slides-along-its-thickness.md) — the
  panel axis and the two moves this keeps.
- ADR [0151](0151-the-furniture-move-takes-the-piece-by-a-click-and-follows-the-pointer.md) and ADR
  [0153](0153-a-carried-piece-is-measured-by-the-hand-not-by-the-ray.md) — the gesture and the
  one-axis carry whose latch this replaces.
- Plan: X74.
