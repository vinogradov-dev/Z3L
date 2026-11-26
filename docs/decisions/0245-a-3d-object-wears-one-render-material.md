# 0245. A 3D object wears one render material, and the frame never reads the catalogue

- Status: Accepted; builds on 0244 and shapes Y7, O16 and O18
- Date: 2026-11-25

## Context

The owner's model, 2026-11-25: *any 3D object, any surface must have a material from the render's
material list, as in any 3D program. A furniture panel references a material of the nomenclature,
which in turn references a render material; but the panel is in fact an ordinary 3D object and is
assigned a render material, to all its surfaces.*

What the tree had: `crates/materials` owns the library (ADR 0244) and `crates/scene` carries a
`Surface` and a `Decor` on every drawable (Y3), but `app/src/scene_build.rs` painted every panel from
one of three placeholder tones, every band from one grey constant and every object from a debug
colour. No document node named a render material, so the viewport and the tracer could not show what
the workshop buys.

## Decision

**The material of a surface is document data: every node that draws a surface names one record of the
render's library, and the frame resolves that reference and nothing else.**

- **`model::MaterialRef` is a non-zero `u32` and lives in `model`.** The document must not depend on
  `materials` (L2), so it names the record by the number the library handed out. The shell converts a
  `MaterialRef` into a `materials::MaterialId` in one place. The render's crate stays free of the
  document, which is what lets it be lifted into another project (ADR 0244).
- **A panel and an object carry one slot; the whole body wears it.** Both faces, the cut edges, every
  band and every face of an object are painted from the one record, because that is what a 3D object
  carries. A container, a camera and a light draw no surface and are refused
  (`ProjectError::NodeHasNoMaterial`). More than one material per body — a face list, an edge list —
  is O18's composition, not a second slot here.
- **The frame resolves the reference; the catalogue is not on this path.** `FrameMaterials` reads the
  session's `Library`, the shell's `Store` for a decor picture and the tab's `scene::Decors`, and
  turns a record into the drawable's colour, `Surface` and `Decor`. The panel's catalogue material
  still decides its thickness and its cut-list row; filling the render slot from the catalogue
  record is O16's bridge in `panel.material.set`, one line where the two exist, and never a read of
  the catalogue from the frame.
- **A quiet answer keeps the placeholder, and a refusal is a sentence.** No slot, a reference the
  library does not hold, a record with no base (a finish alone: a layer has nothing to be seen
  through, so a record that names only a coat still paints the placeholder)
  and a frame built with no workshop folder all paint the placeholder tone; a picture that is named
  and cannot be read is its record's flat colour and a log line naming the record and the file, never
  a black rectangle.
- **The slot is authored data and the frame follows the library, not the document.** A library edit
  moves no revision; the tab re-derives its frame because the session's library version moved, and a
  replaced picture arrives under a new `DecorId` (ADR 0244).

## What this record does not decide

- The command and the card that assign a slot. The model setter exists so a scene file and a
  fixture can carry one; a user-facing `node.material.set` with its inventory row, its undo entry
  and its card is its own task, and until it lands the slot is written by the file and by the
  catalogue bridge.
- What happens when the catalogue record a panel names points at another render material later:
  O16 decides whether the slot follows the record or the assignment is a copy.
- The scene file's other material words. `Panel::material` keeps its catalogue `ItemId`; the two
  fields say different things and neither replaces the other.

## Consequences

- The scene schema is 33: `render_material` is written on a panel or an object that names one and
  omitted otherwise, so a scene that names no material is written exactly as before.
- A scene travels between workshops with the ids it named; a library that lacks one draws the
  placeholder and says so, which is ADR 0148's rule for an absent catalogue record.
- The viewport and the tracer read one `Drawable`, so they cannot disagree about a board: the
  picture, the grain's turn and the surface channels are the same values for both (Y3, Y7).
- The placeholder tones stay in the code and are still what an unfilled catalogue looks like, so a
  workshop that has not built a library still sees boards.
