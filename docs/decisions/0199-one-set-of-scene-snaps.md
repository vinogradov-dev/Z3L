# 0199. One set of scene snaps, resolved in the model

- Status: Accepted; extends 0051 and narrows its "no document" clause
- Date: 2026-11-21

## Context

Snapping existed twice. The drawing tools and the ruler carried a plane-local resolver in
`app/src/gizmo_math.rs` — the ends of the draft, an axis lock, the 10 mm grid — while the object
move carried `SnapSettings` in `model`, with the coordinate planes, another product's frames and a
tolerance. Two mechanisms answered one question, so a third tool that wanted to snap had to choose
one of them and copy it. ADR 0051 left the door open ("a later snap to panel vertices, edges or
intersections extends the request and `SnapTarget`; it does not add a second snapping path or touch
the document") but the document is exactly where a panel's corners live.

## Decision

- **One atom, in the model.** `Project::snap` answers a point and a kind from the pointer's ray, the
  sketch plane and a tolerance; `Project::snap_marks` is the same candidate set without the pointer.
  It reads the document through the queries that already own the shapes — a box's corners and edge
  middles, a drawn ring's segments and arcs, a product's envelope, a primitive's bounds — so a corner
  is derived where the corner is, and no tool keeps its own copy of one.
- **Ten kinds, one priority order.** `model::SnapKind` declares, in this order: vertex,
  intersection, midpoint, centre, perpendicular, tangent, edge, face, axis, grid. A corner beats a
  nearer edge, because a maker pointing next to both means the corner; inside one kind the nearest
  candidate wins.
- **The workspace owns the switches.** `model::SnapKinds` is one value with ten switches; the
  workspace file holds it and a tab keeps the live copy, so a kind turned off in one tab is off in
  every tab and survives a restart. The grid's own switch moved from the drawing gizmo's view state
  into the same set: a snap the maker turns off is a setting, and `G` is its cell's key.
- **The plane kinds are the plane's.** Intersections, perpendicular feet and tangent points are
  solved in the sketch plane, because that is where the maker draws; a scene point off the plane is
  taken at its footprint, and its marker stands on the candidate itself.
- **The face is the tool's reading.** A tool hands in the face it measured under the pointer — the
  ruler steps past a product's reserved box to the board behind it — and the atom only ranks it.
  That face is also the eye's limit: a candidate behind it does not capture, because a marker inside
  a board names no place a maker can work at.
- **A centre is a surface's middle, not a solid's.** A panel or a body offers the middle of each of
  its six faces; the inside of the material is no snap. A primitive's own centre and the middle of a
  product's reserved space are, because both are places empty or built around.
- **The move consumes the same value.** Its rule reads the same `SnapKinds`, and the centre kind adds
  the one coordinate a box's faces cannot offer: the middle of another product's reserved space.

## Consequences

- A new snapping tool asks the atom; it does not write a resolver. A second copy is now a defect the
  reader can see.
- The resolver walks the document once per pointer frame. That costs a tree walk and a few hundred
  candidates for a workshop-sized scene, and it is why the walk skips a hidden subtree and stops at
  the nesting limit; a scene that outgrows it gets a cache keyed by the revision, not a second path.
- A disabled kind never captures, and the tolerance decides the rest. The grid is the exception in
  one direction only: it is the plane's quantum and rounds wherever the pointer is.
- The fill and the niche ghost keep the niche atom's cursor rule (`ui::pointer_point`) instead of the
  snap resolver: the point they name is the *space* the pointer is in, and a snap that moved it a few
  millimetres would name the same cell. `plan/debt.md` records the reading.
- ADR 0051's "touches no document" is superseded for the resolver and kept for the commit: snapping
  reads the document and writes nothing.
