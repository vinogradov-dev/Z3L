# 0252. A node places its material's picture on its own plane, and the record stays one texture

- Status: Accepted; answers the question Y15's entry held open for the owner, and builds on 0244's
  record, 0245's per-node material and 0251's window
- Date: 2026-11-26

## Context

A material's record says how much board its picture covers (`covers`) and how its grain runs
(`turns`). Every board cut from that record therefore wears the same veneer at the same scale and the
same turn: in one cabinet the door's grain cannot run along its height while the shelf's runs along
its width, and a small part cannot be shown at a finer scale than a large one. `scene::Decor::uv`
projects a drawable's local `XY` by those two numbers, and both consumers — the viewport's vertex
builder and the tracer's triangle soup — call it.

The owner's ruling, 2026-11-26: **the panel stays in its local plane, and the node carries only a
scale, an offset and a quarter turn.** The alternative the entry named — a projection axis per face
(top, front, side) — was ruled out with it: a real unwrap is not this feature, a contour part is a
prism, and a shop aligns a grain rather than unwrapping a body.

## Decision

- **The mapping is the node's**: `model::DecorMapping { scale, offset, turns }` on `Panel` and on
  `Object`, absent by default. Absent means the record's own numbers, so a document written before
  the field existed draws exactly as it did. The scale is a length per picture width — a smaller
  number is a finer grain — the offset shifts the picture along the node's own `X` and `Y` in
  millimetres of board, and the turn adds to the record's quarter turns, which keeps a scan's own
  direction meaning what it meant.
- **The value is validated where it is built**: a scale of zero or one past the document ceiling, an
  offset past it and a turn count past three are refusals, not repaired numbers. A layer's thickness
  is data with a fallback; a coordinate that divides by the scale is not.
- **The picture stays one texture.** The frame composes the node's mapping with the record's decor at
  the moment it computes the coordinate, so a cabinet's forty boards keep one `DecorId`, one upload
  and one picture, and the two frames keep reading one `scene::Decor`. A decor cloned per node would
  upload the same scan once per board.
- **One command, one undo entry**: `node.decor.set` stores or clears the mapping, and clearing it is
  the record's numbers again. A node may place a picture before it names a board; the order of two
  commands is not a rule.
- **Placement on the card waits for the material slot.** The command is reachable by id today and the
  card that edits it arrives with U129's render material slot, beside which it belongs.

## Consequences

- The scene schema is 36 and the golden carries one board that places its picture and one that does
  not, so the field is exercised both ways.
- The viewport's vertex buffers are keyed by the node's mapping as well as the record's numbers: two
  boards of one mesh, one placement and one picture, placed differently, hold different coordinates.
- A per-face projection, a real unwrap, a continuous turn in degrees and a second copy of the
  projection function stay out; a body that needs an unwrap is a later record with its own evidence.
