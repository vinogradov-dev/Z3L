# 0177. The invisible floor stops a body, and it is not the floor snap

- Status: Accepted; amends [0146](0146-snapping-is-a-workspace-setting-and-the-coordinate-planes-stand-in-for-walls.md)'s scope to dragging
- Date: 2026-11-19

## Context

The floor existed in this program three times and nowhere as a rule. A coordinate plane at Z = 0
captured a dragged face onto itself within the workspace's tolerance
([0146](0146-snapping-is-a-workspace-setting-and-the-coordinate-planes-stand-in-for-walls.md)), so a body only stopped on the floor when the
snapping happened to be on and the hand happened to come near it. A floor-bound product class kept its
height and was the only thing that could not be carried under the bench. Everything else — a solid
body, a loose board, a wall cabinet — was free to sink in a perspective view, where the carry plane is
horizontal and the pointer's own screen travel says nothing about the world's vertical.

The owner asked for a floor that is a fact of the workspace rather than a capture: "невидимый пол по
умолчанию, который останавливает любое тело, с галочкой, которая его отключает" — on by default, a
stop for every body, and a switch that turns it off.

## Decision

**The floor is a lower bound on the vertical axis of every drag, not a capture.** `MoveRule` is handed
the workspace's `MoveSettings` when a piece is taken, and it reads one number from the moved bodies:
`floor_bound` is the lowest the drag may travel down the world Z, taken from the moved boxes' own
`min.z`, so a body standing on the floor may not sink at all and one standing a hand's width above it
may descend that far. A capture moves the piece onto a surface the maker is near; a bound refuses a
place that is under the floor, whatever the pointer is near. The two answers are therefore separate
kinds of object: `Capture` says where the piece jumped, `Stop` says what refused it.

**The floor names itself.** `MoveLimit::contact` and `MoveRule::blocked_at` answer `Stop`, which is
either a part of the document with the surface that met the piece and its world bounds, or
`Stop::Floor`. The log and any later report write `Some(Floor)` instead of borrowing a node id for a
surface no node owns, and the box arithmetic stays in the one variant that has a box.

**It is a workspace value beside the captures.** The switch lives in the *Snapping* section of the
workspace settings, because it answers the same question the snapping boxes do — where a dragged body
may come to rest — and it is persisted with them, default on, applied to the next drag and never to the
one under the hand ([0146](0146-snapping-is-a-workspace-setting-and-the-coordinate-planes-stand-in-for-walls.md)). It is no document data: it
costs no revision and no undo entry. The one value per drag travels as `MoveSettings`, which also names
the axes the drag offers, so the object move (document axes, workspace snaps) and the free move (all
three axes, no snap) are two readings of one workspace choice instead of two entry points.

**Contact and the floor are one reading.** `MoveRule::free_at` is now `blocked_at` without a name:
a place is free when nothing and no floor is in the way, so the destination-first rule of
[0166](0166-the-move-answers-the-place-the-piece-lands.md) cannot accept a place the axis walk would
refuse, and the log's own question is the drag's own answer ([0170](0170-a-report-names-the-part-that-was-in-the-way.md)).

**The floor binds a drag that starts above it.** A body the document already holds below the floor —
a hand edit, a typed coordinate — is not pushed up by it: the bound is `min(0, -min.z)`, so it forbids
going deeper and nothing else, and a piece left that way is lifted out by hand. That is the same
reading [0162](0162-a-placement-may-not-put-a-part-inside-another.md) gives a pair that already
overlaps.

## Consequences

- No body can be dragged under the floor by accident, in any view, with the snapping off, and the log
  says `Floor` rather than a node.
- A class that keeps its height is unchanged: its vertical axis is not offered at all, so the floor
  adds nothing to it and never reports a contact it did not cause. A wall or ceiling class now meets the
  floor on its way down, which is the only behaviour that changed for a product.
- The floor stops and never captures, so it draws no capture line and clears no snap: a piece that
  lands on it reads as contact, which is what the status band and the exact-distance fields already
  know how to show.
- The write guard does not carry the workspace switch: a typed coordinate may still place a part below
  Z = 0, because the floor is a maker's help and not a document invariant. A document written that way
  loads, and every drag out of it is a way out.
- `MoveSettings` replaces `Project::contact_rule`; the free move asks `MoveSettings::free`, which is
  the same value with all three axes and nothing to snap to.
