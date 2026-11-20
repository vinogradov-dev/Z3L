# Processing and rules

Status: domain specification, target state. This document describes how a panel is machined, where
the trades and tools come from, how drilling is derived from the hardware that causes it, and how the
placement rules a workshop works by are stored and applied (ADR 0150). Today a panel carries one
ordered list of anchored machining records and the list has one kind, a cutout
(`crates/model/src/panel/machining.rs`, ADR 0045, 0046). The crossing that turns a drilling object
into a hole is computed: `model::derive_holes` clips one resolved drilling object against the panel's
own material and the evaluation cache keys the result by the panel, its placement and the objects
(`crates/model/src/panel/hole.rs`, `crates/mesh/src/lib.rs`). What carries those objects — a part
document and the instance that places it — is not in the program yet, so no scene drills a panel, and
the body is drawn undrilled.

Related documents:

- [Nomenclature directory](nomenclature.md) — items, kinds, part documents and instances.
- [Panels and edge banding](panels.md) — the panel and its machining list.
- [Scene document](document.md) — the file a machining record lives in.
- [Order package](order-export.md) — the frozen copy that carries the resulting programs.

Related decisions: [0045](../decisions/0045-machining-is-an-ordered-list-of-anchored-operations.md),
[0046](../decisions/0046-machining-compiles-to-staged-effects.md),
[0094](../decisions/0094-a-class-registry-gives-the-machine-its-semantics.md),
[0149](../decisions/0149-a-catalogue-part-is-a-document-instantiated-in-the-scene.md),
[0150](../decisions/0150-machining-is-derived-and-rules-are-tables.md).

## 1. Two kinds of machining

| | Where it comes from | Where it lives | Rebuilt when |
|---|---|---|---|
| **Authored** | a person draws it on the panel: a cutout, a groove, a contour milling | the document, the panel's machining list | the panel or the record changes |
| **Derived** | hardware: a hinge's cup bore, a runner's screw holes, a shelf pin | nowhere; computed on demand and cached | the panel, the instance placements or the directory revision change |

The split is the reason a panel has no drilling parameter: a hole belongs to the thing that makes
it, and a panel that stored its own holes would disagree with the hardware the moment a hinge model
changed.

## 2. Authored machining

The record keeps the shape it has: a stable `MachiningId`, an anchor (a face or an edge), typed
parameters and an ordered place in the list. The kinds the plan adds:

| Kind | Parameters | Notes |
|---|---|---|
| `Cutout` | region, depth mode | exists |
| `Groove` | anchor, width, depth, path | the width is the slot, the tool is chosen later (§4) |
| `Milling` | the panel's contour, `ItemId` of the cutter, depth, offset | the tool is stored, because its radius is the corner radius (§4) |
| `Bore` | anchor, diameter, depth | only for a hole no hardware causes; a shelf pin placed by hand. Its shape is an open question (§8) |

Every kind declares its capabilities — which anchor it accepts, whether it cuts through or stops at a
depth — and the record is refused when it is stored on an anchor its kind does not accept, as the
cutout already is.

## 3. Derived machining: a hole is a crossing

A part document carries **drilling objects**: a path node with a class from the class registry
(ADR 0094) and typed properties for diameter and depth, plus a direction. A **fastener point** is the
same kind of object whose property references the fastener's item (ADR 0149 §8).

The rule:

1. the instance is resolved into world space by its placement;
2. every drilling object is intersected with every panel **body** it crosses — the panel's own faces,
   not its nominal outline, so a hole near an edge follows the geometry the person actually made;
3. a crossing becomes a hole: a centre point in the panel's local frame, a diameter, a depth or a
   through mark, and the id of the object that caused it;
4. the hole list is ordered by panel-local coordinates, so two builds produce the same output and a
   report diff is readable;
5. the result is cached by `(panel revision, ordered instance placements, directory revision)` —
   a frame never computes it, and an untouched cabinet never recomputes it.

A crossing that is degenerate — tangent, grazing the face, a diameter larger than the panel — is a
diagnostic naming the object and the panel, never a silently skipped hole.

## 4. Tools

A cutter, a drill and a saw blade are catalogue items of their own kinds with their own properties
(diameter, shank, cutting length, material, life). The **machine park** is a separate package: which
tool sits in which slot, its feeds and speeds, its coolant, its entry rules. A cutter is bought once
and mounted on three machines; the park is the workshop's, the item is the supplier's.

Where the tool is referenced and where it is selected is decided by one question — *does the tool
change the shape?*

| Operation | Document stores | Tool |
|---|---|---|
| contour milling | the cutter's `ItemId`, depth, offset | **stored**: the radius is the corner radius and the offset |
| groove, kerf | width, depth | selected at post-processing from the park |
| drilled hole | diameter, depth (from the drilling object) | selected at post-processing |

- A stored cutter that is no longer in the directory is a diagnostic that names the tool; the last
  resolved offset is the fallback, so the panel still draws and the report says what it lost.
- A groove whose width no tool in the park can cut is a **refusal with the width in the sentence**;
  the program never widens or narrows a cut to fit a tool.
- Cutting parameters — feed, speed, coolant, entry point, order of operations — belong to the park
  package and never to the document: the same panel goes to a second machine without its geometry
  changing.

## 5. Rules are tables

Two families, because they are two different things a workshop knows:

| Family | Owner | Example |
|---|---|---|
| **Fastener patterns** | a workshop standard, a prototype or the fastener's own item | system 32 rows on a side panel: pitch 32, 37 mm from the front edge, one row per shelf |
| **Hardware tables** | the supplier's package | a hinge: overlay × door thickness × plate height → plate, cup distance, setbacks |

Both are `RuleTable` records:

- **axes**: a property (or a derived measure such as panel thickness), with a kind — a value, a
  range, or a step;
- **cells**: a key per axis and a result: a length, a slug, an `ItemId`, or a set of positions;
- **validation on load**: a table that leaves a combination uncovered is refused naming the axis and
  the value; two cells claiming one combination are refused too; a cell whose result is an unknown
  slug or an absent item is refused;
- **precedence**: the item's own table, then the prototype's, then the workshop standard's — and the
  card shows which table answered and from which package;
- **one result, one command**: applying a table produces the hardware instances and the machining it
  causes as one undo entry, with the reason visible in the panel card.

A supplier table ships inside the supplier's package, versioned with it: it is the supplier's data,
not our code. A workshop's fastener patterns ship in its standards package, because two workshops
drill their panels differently.

## 6. No expression language

Placement, selection and supplier data are closed templates and tables. Arithmetic lives in a
template's parameters, not in a program inside a package. The one place where arithmetic over
parameters is genuinely open is the prototype's formula surface (`prototypes.md` §11); if it is
granted, one shared evaluator serves prototypes, selection and rules rather than three half-languages.

## 7. What a report, a drawing and a program see

One derived hole list per panel, computed once per revision: the report counts holes, the drawing
places them, the program orders them. They cannot disagree, because they read one value. The order
package is the frozen copy: it names the directory revision, the part hashes and the tool each
operation used, and it does not move after it is written (ADR 0120).

## 8. Open questions

1. A **lone hole** with no hardware behind it — a shelf pin placed by hand: an authored `Bore` record
   on the panel, or a shop-made part document with one drilling object? The owner's rule is that a
   panel has no drilling parameter, so this needs a decision.
2. Where the **entry point, order and tool change** of a program are authored: in the park package, in
   the machine's post-processor, or per operation.
3. Whether a derived hole list is ever pinned into a document (an approved program) or always derived
   with the order package as the only freeze.
4. The **kinematics** of a hinge or a runner — separate mechanism, not designed here.
5. How a **feature** on a part document is selected and edited: a drilling object is a path with
   properties, and the tool that adds one is an operation in the editor's own right.

## 9. Tests

- an authored groove and a contour milling survive a save and a load, and one with a width outside any
  tool's range is a refusal, not a narrowed cut;
- a hinge instance on a door yields an exact hole set at exact coordinates, including a hole near a
  corner where the panel body differs from its nominal outline — the crossing is tested today with a
  resolved drilling object (`crates/model/src/panel/hole.rs`); the instance that resolves it lands
  with the instance node;
- the same hinge on two doors shares one resolved part and one mesh — waits for the instance node;
- a hole list is stable and ordered across two builds, and an untouched panel is not derived again;
- a fastener pattern on a side panel produces the expected positions for system 32, and a panel
  thickness outside the table's axes is a refusal naming the axis;
- a hinge table answers with the plate the supplier's table names, and an uncovered overlay is a
  refusal; the card names the table and its package;
- a cutter that leaves the directory yields a diagnostic on its panels and keeps the last offsets;
- the directory revision changes and the hole list of an untouched panel is not recomputed.
