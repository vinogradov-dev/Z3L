# 0180. Control belongs to the object tool: the free move and the rotation are not measured

- Status: Accepted; amends [0162](0162-a-placement-may-not-put-a-part-inside-another.md)'s
  "every write obeys the rule" for the two gestures the maker drives by hand, and closes the
  `[gizmo]` debt entry about the rotation and contact
- Date: 2026-11-19

## Context

X85 gave a body its own surface, and X72, X86 and X87 gave the drag a contact, a depth rule and a
floor. All of that arrived through `MoveSettings` and the move rule, which **both** translate tools
read: the object tool (which obeys the document's rules and the class's axes) and the free tool
(which keeps all three arrows). X87 then put the floor into the free tool's reading too, on the
argument that the floor is the workspace's stop and not a tool's taste.

The owner rejected that. The object tool is the one that honours the body laws — a piece set down on
a face stops on it, a turned cone may reach into a corner and no further. **The free move and the
rotation are the maker's own hand: a change of place without control.** A board a person drags
through a neighbour, a cabinet turned so that it overlaps the wall behind it, is that person's
decision and not a rule's to refuse: the free gestures are for arranging a scene quickly, and a body
that ends up inside another is repaired by the same hand that put it there.

The rotation was worse than that: it was not measured while the pointer moved, and then refused at
the release (`RestingInTheWay` from `node.placement.set_many`) with the piece snapped back — a
gesture that showed a place and then took it away, which is the one behaviour the rules name as never
acceptable (ADR 0166).

## Decision

**`MoveSettings` carries whether the drag is measured.** `measured: bool`, true in `Default`, and
`MoveSettings::free()` reads `{ axes: Every, snaps: None, floor: false, measured: false }`: the free
move gets no obstacles at all (so no contact and no cut), no floor and no snapping, and only the
pointer's own distance reaches the placement.

**The rotation is never measured.** `commit_rotation` produces a commit with `measured: false`;
`commit_translation` takes the tool's answer as an argument, so the free drag is false and the object
drag is true. `PlacementCommit` carries the flag to the shell, which is the only place that knows how
the write was asked for.

**The write says how it was asked for.** `node.placement.set` and `node.placement.set_many` take
`unmeasured: bool` (`#[serde(default)]`, so every existing caller and every older client keeps the
guarded behaviour) and skip `refuse_resting` when it is set. The command id does not change: it is
still "write this placement", and the difference is the maker's own hand rather than a second
operation. The inspector's typed fields, a palette command, the assistant and the box gizmo leave it
false and keep the refusal of ADR 0162.

**One adapter, one reading — where control exists.** The session still owns the contact adapter and
the object tool still reads it, so a preview and its release cannot disagree. The free gestures ask
the same session for nothing.

## Consequences

- A free move passes through a neighbour, goes below the floor and costs one undo entry; a rotation
  writes the turn it showed, however it overlaps. The object tool keeps every rule of ADR 0162, 0166,
  0177 and 0178.
- Two behaviours that the tests of X72/X87 asserted are now wrong and are re-stated: the free move
  keeps its three axes and no control at all, and the frame test that asked both tools to stop flush
  now asks that of the object tool and asserts that the free move passes the same face.
- The `[gizmo]` debt entry about the rotation and the contact rule leaves `plan/debt.md`: the answer
  is not a clamp per obstacle but no measurement at all.
- The shape builder treats a convex mesh as a hull, because a triangle surface is a skin: a body
  lying wholly inside another would otherwise share no face with it and read as no overlap. A dented
  body stays a surface, and a flat sheet stays a sheet.
- The floor switch in Settings keeps meaning "the object tool's floor stops it"; the free move is not
  its subject.
