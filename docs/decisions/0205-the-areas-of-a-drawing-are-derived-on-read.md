# 0205. The areas of a drawing are derived on read

- Status: Accepted; completes G3's contour sets and the hole question G4 left open
- Date: 2026-11-21

## Context

A contour boolean stores its result as a drawing: one ring is a `Geometry::Path`, several are a
`Geometry::Contours` (ADR 0194). Each stored ring encloses material, and the rings' own boundaries
divide that material into the **areas** a maker points at — two nested rectangles leave the frame
between them and the opening inside it, two crossing circles leave three pieces. Nothing could read
one: the push/pull saw only rings, so a maker who drew a frame could not turn the opening it leaves
into a part (G4).

The document could not simply store the areas. `Geometry::Constructive` carries **one** path plus a
feature history, `geom::Region` is the only shape that carries holes, and a set's rings are edited by
the drawing tools. A stored area would be a second source of truth for a number every ring edit
changes, which is the defect ADR 0118 names.

## Decision

- **The areas are a derivation, not a document field.** `geom::partition_regions` is the exact
  overlay of the set's own rings: the material is `union_regions` of the rings, and every ring then
  splits each face into the part it covers and the part it leaves. The result is disjoint, together
  the faces cover exactly the rings' material, and a ring a face lies inside is that face's hole.
  `model::Contours::areas` carries the rings into millimetres in the set's own plane and answers
  them; `Contours::area_at` answers the one face a point falls in.
- **An area has no id, and a point names it.** A command, a pointer and a saved macro all name an
  area by a point inside it, in the drawing's own plane, because an area is a query result. Two
  faces that share a boundary answer the first of them, which is a boundary point rather than a
  place a pointer rests.
- **The pull creates a body beside the drawing.** `object.area.extrude` takes the drawing, the point
  and a height, and stores a `Geometry::Constructive` of the face's outer ring in the drawing's
  parent, standing in the drawing's own frame. The drawing keeps its rings: the maker pulled one
  area out of a drawing, not the drawing.
- **A face with a hole is refused, with a sentence.** A body carries one ring until the profile
  stores holes, and the alternative — a body cut with the hole as a subtraction step — would stop
  matching its area the moment its height changed, because a tool body's own height is frozen when
  the step is stored (ADR 0197). The refusal is the honest half of the feature, and the profile with
  holes is the task that closes it.
- **The gesture is the cone's, on another payload.** The Extrude cell arms the pull when the
  selection is a drawing; the pointer names a face through the `ui::Area` atom and the shell paints
  it; a click runs the command in an edit scope and hands the body to the existing pull arrow, so the
  arrow's height joins the same undo entry, the ✓ closes it and the ✕ rolls the body back
  (ADR 0138, ADR 0141). The atom caches the faces by the drawing and the revision they were derived
  at, because a hover must not re-derive a subdivision on every frame.

## Consequences

- A ring edit divides the drawing again in the same frame: nothing about an area outlives the rings,
  and no migration, file field or invariant is added.
- The derivation is quadratic in the rings — every ring is intersected with and subtracted from every
  face — so `geom::MAX_PARTITION_FACES` refuses a drawing that would leave more faces than one screen
  can show, rather than pausing without a reason. A scene that outgrows it wants a cache keyed by the
  drawing, which is what `ui::Area` already is.
- A second consumer of an area (a report row, the assistant, a face move) reads `Contours::area_at`
  and the atom; a second partition is a defect the reader can see.
- The atom's cache is keyed by the document revision, so a moved drawing re-measures its faces
  although the faces are local to it. That costs a containment test, not a subdivision, and it is the
  price of never showing a highlight that no longer stands on the drawing.
- The refusal is where the next record starts: a profile with holes changes `Constructive`, the file
  format and the extrusion compiler, and this record's command becomes the door that takes one.
