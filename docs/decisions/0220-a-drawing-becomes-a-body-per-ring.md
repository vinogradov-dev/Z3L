# 0220. A drawing becomes a body per ring, and the area pull is the Extrude cell's second method

- Status: Accepted; the Extrude cell's gesture of 0205 becomes that record's second method
- Date: 2026-11-22

## Context

A contour boolean stores its result as a drawing: one ring is a `Geometry::Path`, several are a
`Geometry::Contours` (ADR 0194). An intersection of three rings can leave two separate pieces, and
a union that leaves two pieces does the same — and nothing turned such a set into bodies:
`object.path.extrude` read a path object and nothing else, so the Extrude cell had no press that
made a set solid.

Since 0205 the same cell arms the area pull on a drawing, because a drawing encloses *areas* and the
pointer is how a maker names one. That left one cell claimed by two meanings: one press that turns
the whole drawing into bodies, and one press that hands the pointer the choice of a face. X100 was
filed before that record and still said the cell "goes off" for a set.

## Decision

- **One press turns the whole drawing into bodies, and a set gives one body per ring.** A body
  carries one ring, and a solid of two disconnected pieces is not a part a shop cuts, so a set is
  expanded instead of being widened: `object.path.extrude` takes a path node or a contour set node,
  and the height is the same typed session default either way.
- **The node keeps the first ring.** The first ring of the set takes the node it grew from: the id,
  name, placement, pivot and properties stay, exactly as a path extrusion keeps them, so the
  selection, the undo entry and a batch's `$ref` keep pointing at one node. The other rings land
  beside it, in the same container and in the drawing's own frame, named after it (`Union`,
  `Union 2`) with a pivot of zero — the pivot a new object starts at (ADR 0088). The command reports
  a change rather than a creation, like every other in-place extrusion.
- **The area pull is the cell's second method, not its face.** The Extrude cell is a split cell:
  its main face extrudes the whole drawing, and its list offers the gesture of 0205 — the pointer
  names one area of the drawing, a click stores it as a body and the drawing stays where it is. The
  gesture a maker uses rarely is a row, and the one they reach for on a drawing of pieces is the
  press. `ToolKind` carries the two methods as two kinds, so readiness, the pressed state and the
  hint are decided per method with no second code path.
- **The area method needs a set.** A closed path is one ring and has no areas to choose, so the row
  is off for it with the sentence that says what to select; `object.area.extrude` is unchanged.
- **Every ring is validated before the tree moves.** The command checks each ring against the mesh
  cap (`validate_extrusion_contour`) before the document changes, so a ring that cannot become a
  body writes nothing; the whole expansion is one revision and one undo entry.

## Consequences

- A maker who builds a shape out of three drawings can turn the result into bodies with one press,
  and the pieces are separate objects they can extrude, turn and boolean on their own.
- Extruding a *nested* set now gives one solid per ring — the outer block and the filler — rather
  than the frame between them. That is the literal reading of "a ring is a profile", and the area
  pull is the door to the frame; the profile that carries holes (G5) is the record that makes the
  frame one body.
- The cell grows a caret and a method list, so the Model tab's row reads *Extrude* with a list
  beside it. Nothing else about the cell changes: the height stays a typed default and the shell's
  pull arrow is still armed on the node the press left behind.
- The per-ring mesh cap check has no reachable fixture: `Contours` refuses a self-crossing ring
  before a set exists, and a ring that does not cross itself cannot sample past the cap at the
  coarsest chord the parametriser may take. The check stays as the guard for a future profile with
  holes, and the note of X100 records the measurement.
