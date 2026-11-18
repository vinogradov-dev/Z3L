# 0146. Snapping is a workspace setting, and the coordinate planes stand in for walls

- Status: Accepted; the snap constants of 0102 and 0103 become settings, and the floor snap joins
  the three planes
- Date: 2026-11-18

## Context

ADR 0102 and ADR 0103 gave a furniture drag a snap: a face of the moved box within 5 mm of the floor,
of a neighbouring frame's virtual face or of a part's face lands on it, and everything else rounds to
the 1 mm grid. Two constants in `move_query.rs` decided it, and the maker could not reach either.

The owner's reading of a furniture move is different in one respect and wider in another. Different:
frames stick to each other, the tolerance is the distance at which a piece jumps to another, and a
workshop standard differs from shop to shop — so the tolerance and the two families of target are the
workspace's, like the grid step and the gizmo handle, not the model's. Wider: frames stick to the
floor and to walls, and until a house-wall tool exists nothing in the tree is a wall at all, so a
frame at the back of a room has nothing to stand against.

A snap is also invisible. The piece moves on its own, the pointer did not travel that far, and the
distance field beside the face measured the first surface a probe met — which for a virtual plane is
nothing at all. A drag that jumps without a line and a number that names another surface is a drag
that feels broken.

## Decision

**The three facts of a snap are workspace settings, and the workspace file carries them.** The View
page of the Settings window grows a *Snapping* group beside the grid step: the coordinate planes on or
off, the frames on or off, and the tolerance in millimetres between 1 and 50, at 5 by default. The
values are `ui::settings::SnapSettings`; the model names their meaning as `model::SnapSettings`
(`planes`, `frames`, `tolerance: Length`) and owns the default, and the shell hands that value to
`Project::move_rule` when a drag starts. A change therefore applies to the next drag and never to the
one under the hand, and the preview, the fields and the release read the same snapshot.

**The tolerance is where the snap fires, not a gap the piece keeps.** A face within the tolerance of a
candidate lands exactly on it: 2 mm from a neighbour becomes flush, not 2 mm away. At the minimum, one
millimetre, only a drag that has already landed captures; at the maximum, five centimetres, a piece
jumps to a surface it was a hand's width from — past that the drag stops feeling like the maker's own,
which is why the field stops there.

**The three coordinate planes X=0, Y=0 and Z=0 are candidates on every axis, and the floor is the
Z=0 plane.** They run through the whole world, so they capture without the perpendicular test a
neighbouring face needs: a frame's back against Y=0 and a frame standing on the floor are the same
snap, and until a wall tool exists they are the stand-in a room does not otherwise have. They belong
to the same switch as the floor because the floor *is* one of them; turning the planes off is how a
maker asks for a drag that keeps the pointer's own distance down to the grid.

**A switch removes a candidate, never a stop.** With the frames off, a neighbouring frame no longer
captures the drag and the piece stays where the pointer put it, but a shell still stops at another
shell: contact is the document's rule (ADR 0102), and a taste that could carry one cabinet through
another would be a defect, not a preference. The 1 mm grid is not a setting either — it is the
document's own clearance, and it is what a drag lands on when nothing captures it.

**The rule reports the capture, and the shell draws it.** `MoveLimit` carries
`capture: Option<Capture>`: which face landed (`SnapEnd`), the world coordinate it landed at, and the
surface behind it (`SnapSurface::Plane` or `Frame(Id)` or `Face(Id)`). A contact that cuts the drag
short of the snapped distance reports no capture, because the face landed on the contact instead. The
shell draws a line in that surface across the moved box, in the helper ink the other guide geometry
uses, and measures the drag's distance field against that same coordinate — replacing the probe's
answer on the captured side — so the field names the surface the release lands on.

## Consequences

- `Project::move_rule` takes a `SnapSettings`; `TranslateGizmo::update` reads it from
  `gizmo_math::Target`, which is where a drag's snapshot values travel rather than an eighth
  parameter. A drag outside furniture still has no rule and keeps its exact micrometre distance.
- The 5 mm literal and the `SNAP_RADIUS_MM` constant are gone; `SnapSettings::default()` is the only
  place the tolerance has a value, and a test holds `ui`'s default to it.
- A settings change moves no revision and leaves no undo entry: snapping is a view of how a drag
  behaves, never document data, and a `.z3l` carries none of it.
- The floor snap is now switchable with the other planes. A class's `mount` trait still keeps a
  floor-bound product at its height regardless of the setting, so the switch cannot make a base
  cabinet float.
- A snap to a plane the probe cannot see needed the capture to be reported, not merely applied; the
  same value is what makes the line and the number possible, and it will be the field of the
  furniture move of N22 without a second measurement.
- Revisit when a house-wall tool exists: the coordinate planes are a stand-in, and a room's walls are
  a document surface the planes cannot be.
