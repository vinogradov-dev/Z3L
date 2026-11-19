# 0165. The move tool is the object move, and a solid body is measured by its faces

- Status: Accepted; supersedes the furniture-only scope of
  [0147](0147-the-furniture-move-is-its-own-tool-and-a-panel-slides-along-its-thickness.md), and
  narrows the snapping clause of [0162](0162-a-placement-may-not-put-a-part-inside-another.md) and
  the world-box contact of
  [0163](0163-the-furniture-move-carries-on-every-allowed-axis.md) for a body that is not furniture
- Date: 2026-11-19

## Context

The fourth transform mode was named for furniture because furniture is what a cabinet editor
furnished. ADR 0147 made it the only gesture that obeys the document's rules and left the free move
with the pointer's own distance; ADR 0162 carried contact into the write; ADR 0163 and 0164 gave the
carry its axes and its plane. The owner now furnishes a room, not only a carcass: a wall, a lamp, a
sofa, a chandelier. The same gesture has to take any of them, and it has to stop them at the faces of
what stands beside them.

Two readings of one contact already lived in the model. `SurfaceBox::meets` is the exact
separating-axis test of two boxes in their own frames, and the write's guard reads it (ADR 0162). The
drag read the world boxes instead, because a furniture piece is a slab whose box is the box the rule
wants. The two answers differ by the slack a turn adds: a 200 x 20 board turned 45 degrees has a world
box that reaches 141.42 where the board itself reaches 100, so the drag stopped up to tens of
millimetres short of the face it was heading for and could not be brought flush to an angled
neighbour.

The same gate had a second effect. `Project::move_rule` handed the workspace's snapping to a
selection that was furniture and to nothing else, so a loose body — a lamp, a chair, a solid drawn
from a contour — could not be landed on the floor plane or on the millimetre grid at all.

## Decision

**One tool moves a body.** The mode, its inventory id (`view.transform.object`), its key and its words
are the object move. The gesture is unchanged: a click takes the body by any part of it, the pointer
carries it, the next click puts it down as one `node.placement.set_many` (ADR 0151, 0163).

**The moved node chooses the reading of contact, and both readings stay.** A furniture piece — a
product, or any node inside one — keeps the world-box contact it always had, because a board in a case
is a slab and its box is the case's own measure. Every other solid body is measured by its own faces.

**The face reading is the swept separating-axis test.** For every axis the pair can be separated
along, the projections of the two boxes are intervals that the drag closes at the rate the direction's
projection names, so that axis stops separating at `gap / rate`. The two solids first touch where the
last separating axis closes, which is the largest of those distances; an axis that never closes keeps
them apart and answers no contact, and a pair that already shares an axis is the maker's to repair,
exactly as the write's guard reads it (ADR 0162). The axis set is one function, shared by the drag's
stop and the write's refusal, so the two cannot disagree about which way two turned bodies are apart.

**The face reading is never later than the old one.** A world box contains its body, so the box
contact happens at or before the true touch: the drag gains the slack of a turn and never gains the
right to pass into anything. A pair that only touches is contact, and a contact is rounded to the
document's micrometre like every other distance, which puts the piece at most half a micrometre into
the touch — under the guard's own one-micrometre threshold, so the release still lands a placement the
write accepts.

**Snapping belongs to the tool, not to the class of the piece.** Every selection is handed the
workspace's settings. A face-measured mover takes the coordinate planes and the 1 mm grid and not
another part's world box, because a face that jumps onto a box is a gap the solid does not have; a
furniture piece keeps the frames and the part faces it had. Contact is not a taste and is never turned
off (ADR 0146, 0162).

**Nothing else about furniture changes.** A panel still offers only its thickness axis, a floor-bound
class still keeps its height, a shell still contains a body and still stops at a shell, and the class
is still read at the take (ADR 0097, 0147, 0163).

## Consequences

- A turned body is brought flush to an angled neighbour and slides along that face, and a loose body
  lands on the floor plane and on the grid, so a room can be furnished with more than panels.
- The furniture move's answers are unchanged, which its tests state: the mode is the same gesture with
  a wider subject, not a second rule.
- One more reading to keep true. It is the exact one the write already used, and the axis set is
  shared, so the cost is a second caller of one function rather than a second rule.
- A solid body is not captured by another part's face yet: a face landing on a world box would be
  wrong, so it takes the planes and the grid until the exact contact surface is carried into the snap
  candidates. That is a debt, not a decision.
- The mode's id moved from `view.transform.furniture` to `view.transform.object`, so a workspace that
  rebound the old id forgets the binding. Before the first release a schema bump keeps no past
  (ADR 0096), and a key binding is a preference, not a document.
- The tool is named for what it moves, and the class vocabulary keeps the word *furniture*: an object
  type — a wall, a floor, a ceiling — is a separate concept the owner has not decided yet, and
  nothing here reserves a place for it.
