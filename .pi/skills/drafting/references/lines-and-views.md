# Lines and views

The rules here are GOST 2.301–2.305 and ISO 128 in our words; the numbers are the standard's, the
sentences are ours.

## Formats and scales

- **Formats**: A4 210 × 297, A3 297 × 420, A2 420 × 594, A1 594 × 841, A0 841 × 1189 mm. A4 is used
  upright for text sheets and may be landscape for a small part.
- **Scales** (GOST 2.302, ISO 5455): reduction 1:2, 1:2.5, 1:4, 1:5, 1:10, 1:15, 1:20, 1:25, 1:40,
  1:50, 1:100; enlargement 2:1, 2.5:1, 4:1, 5:1, 10:1. Furniture drawings live at 1:1, 1:2, 1:5,
  1:10 and 1:20; a detail may take another scale, named under it.
- Every drawing states its scale in the title block; a detail that differs says so beside the detail.

## Lines

| Line | Use | Weight (A3) | Pattern |
|---|---|---|---|
| Solid thick | visible contour | 0.5–0.7 mm | continuous |
| Solid thin | dimension, extension, leader, hatching, leader shelves | 0.25–0.35 mm | continuous |
| Dashed thin | hidden contour | 0.25–0.35 mm | 2–8 mm stroke, 1–1.5 mm gap |
| Dash-dot thin | centre and axis | 0.25–0.35 mm | long-short-long, 3–5 mm beyond the circle |
| Solid thin wavy | break line | 0.25–0.35 mm | hand-drawn break |
| Dash-dot with two dashes | chain line | 0.25–0.35 mm | for a surface developed from a part |

One drawing has **one thick weight**. A second thick line always means a second drawing, a section,
or a detail; it never means emphasis in the same view. The ratio thick:thin is 2:1 or 3:1.

In an icon the same hierarchy becomes: result 1.5, annotation 1.2–1.3, control node radius 2.0. The
icon has no dashed hidden lines — a 24-unit grid cannot hold a dash pattern that survives 26 px.

## Views and projection

- **First-angle** (GOST 2.305, ISO 128-30): the object sits between the viewer and the plane, so the
  top view is placed **below** the front view and the left view to its **right**. A view is named by
  the side that faces the viewer, not by where it is drawn.
- **Third-angle** (US practice) mirrors the arrangement: top above, left view to the left. A sheet
  that uses it says so, usually with the projection symbol beside the title block. Never mix.
- **The main view** carries the most shape and the fewest hidden edges; it is the view the shop reads
  first. A turned part may need a view from the axis, an elongated part a broken view.
- **Sections and cuts**: the cut is marked with two arrows and a letter (A–A); the section is placed
  in projection or on another sheet with its letter. Hatching is thin, at 45°, one direction per
  part; adjacent parts take opposite directions. A panel's section shows the board as a thin
  rectangle with its edge band as a second thin layer.
- **Details** are circled on the view and drawn enlarged under it, with the scale stated.
- **Hidden lines** are omitted where a section shows the same shape; a view crowded with dashes is a
  sign that a section is missing.

## The title block

GOST 2.104 / ISO 7200 fields that matter to us: the part name and designation, the material, the
scale, the sheet count, the projection symbol, the drawing number, the mass where it matters, and
the signatures. A drawing that leaves the shop carries the revision and the sheet number.

## Furniture notes

- A panel is named by its role (side, shelf, back, door), not only by its size; the size is
  length × width × thickness.
- Edge banding belongs to a **face and an edge**: state the material, the thickness and which edge;
  a 2 mm band on a 18 mm board is a visible layer in the section.
- Grain direction is a thin arrow on the face, or a note; a part whose grain matters is never left to
  the shop's guess.
- Sheet size and kerf belong to the cutting layout, not to the part drawing: the layout shows the
  nesting and the offcuts, the part shows its finished size.
- Hardware is a hole with a diameter, a depth and a side (A or B), plus the fitting's class; a
  mounting pattern is a group of holes with one datum.
- The order package (`docs/domain/order-export.md`) is the machine-readable contract; a drawing is
  the human-readable view of the same model, and neither may invent a size the other does not have.

## Icon translation

For the dimension and view icons the line hierarchy is:

| Role | Icon |
|---|---|
| The measured object, base, source, control point | `view`, 1.2–1.3 |
| The result of the tool | the class ink, 1.5 |
| The dimension annotation | `reference`, 1.2–1.3 |
| A control point | circle radius 2.0, real gap 1.15 |
| An arrow or a tick | solid, 2.4 × 2.2 or a 1.4 slash, tip air 1.1 |

The icon is a sign, not a drawing: it carries the grammar and the air, and no digits.
