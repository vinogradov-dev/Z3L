# 0194. A contour boolean is a mode that stores a new object

- Status: Accepted; the tool mark arrives with it
- Date: 2026-11-20

## Context

`geom::boolean` has held the exact 2D overlay of ADR 0043 since G2, and the ribbon has carried
three `sketch.booleans` cells since U69 with no command behind them: a maker could draw two rings
and do nothing with them. Two questions had to be answered before a command could exist, and both
are about the document rather than about the geometry.

The first is what the operation *writes*. The nearest habit — replace the target's ring and delete
the cutter — loses the operands a maker often wants to keep, cannot express a result that is two
separate pieces, and leaves the pair (outline, hole) with nowhere to live: `Path` is one chain and
a panel outline is one ring, so a hole has no field there. The second is whether the cutter stays a
live operand, so an edit to it re-derives every result. That is the parametric answer, and it costs
a stored feature list, a derivation that depends on another node's placement and a cache key that
carries it — the whole machinery a body's feature history already has, spent on a flat drawing.

The owner ruled both: a boolean creates a new object, and a source that must survive is *marked as
a tool* rather than linked.

## Decision

**Three commands, one per membership rule**: `contour.boolean.union`, `contour.boolean.subtract`
and `contour.boolean.intersect` take a base ring (`base`), one or more rings beside it (`operands`)
and a name. The base is the ring the mode started on; it supplies the frame and the container the
result is born in, and every operand is read into that frame through the placements between them.

**The result is a new object.** The sources are consumed in the same command and the same undo
entry — except an object marked as a tool, which stays in the tree. The mark is a field of the
object and nothing else: a tool is a stamp that survives the operation, never a live operand, so
editing the cutter afterwards changes nothing. One ring becomes a `Path`; several rings become a
contour set (`Geometry::Contours`), a flat drawing that draws one line chain per ring.

**A hole is an area, not a ring.** Subtracting a cutter that never crosses the outer boundary leaves
a figure with a hole, and a hole needs the region vocabulary of G4. Such an operation creates
nothing and says so, exactly as an empty intersection or a result equal to one operand does: a
refusal writes nothing, and the sources stay.

**A panel outline is never an operand.** A panel's cutouts are its parameters, and a cutout that
would split the board into two is that command's refusal, not a boolean over a panel's contour.

**The operation is a mode, and its ✓ is one command.** The three cells arm it; the picks are the
selection, so a plain click and a Shift-click pick operands with the habit every other selection
has. The result draws before the commit in place of the base ring through the session overlay
(ADR 0093), and `ContourBooleanPlan` is that same calculation without the write, so the frame the
maker judges and the object the ✓ stores cannot disagree.

## Consequences

- A drawing tool that reads a ring keeps reading one ring: `object.path.extrude` refuses a contour
  set, and extruding the area between its rings belongs to G4 with the rest of the region work.
- `Object` gained the `tool` field and `Geometry` gained the `Contours` payload; the scene schema
  moved to 29, and a file with any other number is the one readable error (ADR 0096).
- A boolean whose cutter is a tool can be run twice on the same drawing, so the created object takes
  the first free numbered name rather than a second copy of the same name.
- Revisit if a maker asks for a live cutter: that is the feature-list design this record rejects,
  and it would need the cache key that today only a panel's derived holes carry (ADR 0150).
