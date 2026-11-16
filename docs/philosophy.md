# Philosophy

What this program believes about furniture, about the work, and about itself. `AGENTS.md` holds the
rules; this page holds the reason they exist, so a decision the rules do not cover can still be
decided the same way twice.

**A piece of furniture is a countable product, not a picture.** FurniCraft counts parts. The image is
a consequence: the mesh, the drawing, the cutting list and the CNC program are derived from one
parts list, and none of them may disagree with it.

## The three laws

The product is named `Z3L`: a zero and three laws — `Local`, `Literal`, `Lasting`.

### Local — the measure is local and exact

Every part is measured from its own zero, and that zero does not move. A panel's frame starts at the
minimum corner of its nominal rectangle: `+X` along the length, `+Y` along the width, `+Z` through the
thickness, face `B` at `z = 0` (ADR 0004, 0083). The world's zero is the floor. A placement is rigid:
no scale and no shear, because a part is not stretched, it is remade.

What it forbids: floating-point millimetres travelling through the domain — a `Length` is an integer
number of micrometres (ADR 0003); a scale or a shear in a `Placement`; a resize that silently moves
existing machining; "approximately". A size is validated before anything is allocated, so a ten-metre
panel is a readable error and not an out-of-memory.

Where it is in the code: `crates/units` and `crates/geom` for the numbers, `crates/model` for the
frames and placements, `crates/param` for the conversion at the kernel boundary.

### Literal — the data is the truth, and it is plain

The document is the only truth. It is serializable data with stable ids, and it knows nothing about
the GPU, the interface, the undo stack or the file. A mesh, a bounding box, a parts list and a drawing
are computed from it and cached against `Revision` (ADR 0029, 0037).

What it forbids: render data or view state inside the document; a mutation that is not a command;
a derived value stored as if a person had authored it; a hidden mode that changes what a number means.

Where it is in the code: `crates/model`, the caches in `crates/mesh` and `crates/scene`, the command
registry in `crates/engine`.

### Lasting — nothing is lost, and the tool outlives the program

Furniture is repaired rather than replaced, and the tool that designs it has to be repairable too.
The file is readable, an older file keeps opening, an unknown field survives a round trip, and no
account, network call or subscription stands between a maker and their own work.

What it forbids: a panic where a typed error belongs; dropping an unknown field on save; a format
change without a migration; a dependency that compiles C and cannot be rebuilt in ten years.

Where it is in the code: `crates/project` for the envelope and the goldens, the panic lints at every
crate root, the layer and dependency gates in `xtask`.

## What the program refuses

| Refusal | Against |
|---|---|
| a cloud the work depends on | furniture software sold as a subscription |
| an automatic layout that decides for the maker | programs that guess and do not say why |
| an opaque file format | CAD whose projects die with the licence |
| an assistant with privileges a person does not have | assistants that can do what their user cannot (ADR 0028) |
| scaling instead of remaking | the CAD habit of pulling a handle to change a size |

## The name

`Z3L` is the zero and the three laws: `Z` for the zero every part is measured from, `3` for their
number, `L` for `Local`, `Literal`, `Lasting`. It is read as a word — "zel" — in Russian and in
English alike, and it sounds like the German `Ziel`, "goal".

Two further readings of the same string are recognised and recorded here, because a name a person can
adopt is a name that carries more than one true reading:

- **`Z-axis · 3 levels · Ladder`** — the world's up axis, the three levels of a product (part, unit,
  product) and the climb from one to the next.
- **`Z · 3 limits`** — the three boundaries a project refuses to cross: a panel no longer than
  10 000 mm, no thicker than 1 000 mm, and no value outside the domain at all.

The search that produced the name — nineteen candidates, their trademark checks, the domain and store
inventory, and the road each one closed — is recorded in `plan/notes/R1.md` and ADR [0135](../decisions/0135-the-product-is-z3l.md).
