# 0150. Machining is derived from geometry, and placement rules are tables

- Status: Accepted
- Date: 2026-11-18

## Context

A — the panel carries one ordered list of anchored machining records (ADR 0045), and today the list
has one kind: a cutout. B — the shop drills dozens of holes per cabinet, most of them put there by a hinge, a
runner or a shelf pin rather than by a person. C — the shop's own programs handle that differently:
in К3-Мебель and Базис-Мебельщик a hole is not a drilling parameter of a panel at all. A segment
carries drilling attributes, the segment lives inside the three-dimensional model of the hardware,
and when it crosses a panel the crossing is what the report, the drawing and the CNC program turn
into a hole. The owner states the same for Z3L: a panel will not have a drilling parameter.

D — where the tool is concerned the practice splits, and the split is not arbitrary. A groove is
described by depth and width; a contour is milled with a specific cutter whose radius sets the corner
radii and the offset, so the cutter is part of what the person authored. E — supplier data for
hinges is a table: overlay, door thickness and plate height pick a cell and the cell names the plate,
the cup distance and the setbacks. It is not a formula, it is a lookup.

## Decision

**A panel stores what a person authored and nothing else.** The machining list keeps its anchored
records: a cutout, a groove, a contour milling. It never stores a drilled hole. Holes are derived.

**A drilled hole is the crossing of a drilling object with a panel.** A part document carries
drilling objects — a path with a class from the registry (ADR 0094) and typed properties for diameter
and depth — and a fastener point is the same object with a property reference to the fastener's item
(ADR 0149). Where such an object crosses a panel, the crossing becomes a hole. It is computed when a
report, a drawing or a CNC program asks for it, and cached by the panel's revision, the instance
placements and the directory revision, so a frame never computes it and an untouched cabinet never
recomputes it.

**A tool is referenced only where the tool changes the result.** A contour milling stores an
`ItemId` of a cutter, a depth and an offset, because a 2 mm radius in the corner is the cutter's
radius. A groove stores its width and depth, and the saw or the cutter is selected at post-processing
from the workshop's park. A hole stores neither: its diameter comes from the drilling object.

**Tools are nomenclature, and the machine park is a package.** A cutter, a drill and a saw are
ordinary items with their own properties; "on this machine it sits in slot 7 at 3000 rpm" is a row of
a machine-park package that references the same `ItemId` (ADR 0148). A milling whose cutter is no
longer in the directory is a diagnostic naming the missing tool; the last resolved offset is used as
a fallback, exactly as a panel keeps its last known thickness, so a project opens and says what it
lost. Nothing is substituted silently: a groove whose width no tool can cut is a refusal with the
width in the sentence, never a quietly widened cut.

**Placement rules are two families of typed tables.** Fastener patterns — a dowel, a minifix, a
confirmat, a screw on a panel pair: pitch (system 32), edge setback, minimum corner distance, row
count, symmetry, panel thickness to fastener length. Supplier tables — a hinge or a runner: axes such
as overlay, door thickness and plate height, cells naming a plate, a cup distance and setbacks. Both
are `RuleTable` records: typed axes, typed cells, no expressions. A table is checked for **completeness
and overlap** when its package loads: a combination that no cell covers is a refusal naming the axis
and the value, and two cells claiming one combination is a refusal too. An owner chain decides which
table applies — the item's own table, then the prototype's, then the workshop standard's — and the
card shows which one was used.

**Supplier tables ship inside the supplier's package**, versioned with it, because they are the
supplier's data and not our code. A workshop's fastener patterns ship in its standards package.

**There is no expression language.** Placement, selection and supplier data are closed templates and
tables; a needed arithmetic is a parameter of a template, not a program inside a package. The one
place where arithmetic over parameters is genuinely open is the prototype's own formula surface
(`docs/domain/prototypes.md` §11). If it is granted, it is one shared evaluator for prototypes,
selection and rules — not three half-languages grown separately.

**Cutting parameters and platform geometry stay out of the document.** Feed, speed, coolant, entry
point and the rest belong to the machine-park package; the document carries the geometry and the
authored parameters that decide the shape.

## Consequences

- A report, a drawing and a CNC program see one derived hole list, so they cannot disagree with each
  other; the panel card shows the holes read-only and says which instance put each one there.
- Correctness moves into the crossing computation, which is quiet when it is wrong. It needs a
  reference test: a known hinge on a known door yields an exact hole set at exact coordinates, and
  that test is the acceptance criterion of the task.
- An authored hole that belongs to no hardware — a lone shelf pin — has no home yet. It will be an
  authored operation of its own kind on the panel, the same list, a different source. Deciding it is
  a task, not this record.
- A tool change is visible: a cutter that leaves the directory is a diagnostic on every panel that
  referenced it, and the export names the tool it used.
- Supplier tables are data we validate, not code we trust, so a supplier package that is incomplete
  fails at load with the axis named instead of drilling a hole in the wrong place.
- No expressions means some shop habit cannot be expressed until a template is added; that is the
  cost of testability, and the line is re-drawn by an ADR when prototypes need arithmetic.
- The order package is the only frozen copy: an open project re-derives its holes, an exported order
  names the directory revision and the part hashes (ADR 0120, 0149).

## References

- ADR [0045](0045-machining-is-an-ordered-list-of-anchored-operations.md),
  [0046](0046-machining-compiles-to-staged-effects.md), [0094](0094-a-class-registry-gives-the-machine-its-semantics.md),
  [0120](0120-sheets-and-money-are-additive-in-the-package.md), [0148](0148-a-nomenclature-directory-is-a-set-of-packages.md),
  [0149](0149-a-catalogue-part-is-a-document-instantiated-in-the-scene.md).
- `docs/domain/technology.md` — the structure this record decides.
- `crates/model/src/panel/machining.rs` — the authored record and its kinds;
  `crates/model/src/class.rs` — the class a drilling object takes from the registry.
- Plan: O10, O11, O12.
