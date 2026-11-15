---
name: drafting
description: "Read and draw a technical drawing — the GOST/ISO grammar of lines, views, sections and dimensions, the six dimension types, and the rules an icon, a dimension tool or an export follows. Use when a drawing surface, a dimension or its icon, a view placement, a section or a drawing export appears."
license: MIT
compatibility: pi
metadata:
  audience: agents
  workflow: drafting
---

# Read and draw a drawing

A drawing is a contract with the shop: a wrong dimension is a scrapped panel. The model is the
source of truth and the drawing is a presentation of it — never a second place to edit.

This skill holds the grammar the drawing surfaces, the dimension tools and their icons share. It is
written in our words; the standards are cited by number, never copied into the repository.

## Sources of truth

| What | Where |
|---|---|
| The standards and their rules | GOST 2.301–2.307 and 2.316, ISO 128, ISO 129-1, ISO 5455, ISO 7200 — cite the number, not the text |
| The drawing's terms | `docs/domain/` (`panels.md`, `products.md`, `order-export.md`) |
| The drawing's look, when it is an icon | `assets/icons/STYLE_GUIDE.md`, `assets/README.md` |
| The icon's inks and metrics | `crates/ui/src/theme.rs` |
| Why a convention is ours | `docs/decisions/`; a new convention is an ADR |
| Where the drawing feature lives | `plan/map.md` |

## Lines and views

- **One thick weight** for visible contours (0.5–0.7 mm on an A3); hidden contours dashed, centre and
  axis lines dash-dot, dimension, extension and leader lines thin (0.25–0.35). The ratio is 2:1–3:1:
  a drawing has one thick line and everything else steps back.
- **First-angle projection** (GOST 2.305, ISO 128-30): the top view sits below the front view and the
  left view to its right. A third-angle drawing (a US customer) is marked as such; never mix the two
  on one sheet.
- **Sections and cuts**: marked A–A with two arrows at the cut; hatching thin at 45°, one direction per
  part. A panel's section shows the board thickness and the edge band as its own layer.
- **Hidden lines only where they add meaning**; a section replaces them where it shows the shape.
- The full table — dash patterns, scales, formats, the title block — is in
  `references/lines-and-views.md`.

## Dimensions

These are the rules the six dimension types share; their arrangements and the icon grammar are in
`references/dimensions.md`.

- **Each size once.** No closed chain without one reference link, written in parentheses.
- **The dimension line** runs parallel to the measured edge, outside the contour; its **extension
  lines** start 1–2 mm from the contour and pass the dimension line by 2–5 mm.
- **The smallest dimension is nearest the object and the largest is farthest.** Baseline dimensions
  are stacked with the largest outermost; the extension line of a near point stops 2–5 mm beyond its
  own dimension line and never crosses the outer one.
- **Chain dimensions** share extension lines; the overall size is not repeated (or is a reference).
- **Radius**: the leader leaves the centre at an angle — never horizontal or vertical — with one
  arrow at the arc and a thin cross at the centre. **Diameter**: one line through the centre with two
  arrows at the circle, `Ø`; a full circle needs no centre cross.
- **Angular**: an arc at a radius between the two rays, arrows at the rays, the value in degrees.
- **Text** sits above the dimension line and is readable from the bottom or the right; the millimetre
  unit appears once, not per size; reference and theoretical sizes are marked as such.
- **Furniture sizes**: length × width × thickness, the banded face and edge with material and
  thickness, the grain direction, the sheet and the kerf, a hole's diameter, depth and side. The
  order package is `docs/domain/order-export.md`; a drawing is not the order.

## Icon grammar

An icon is not a drawing: it keeps the grammar and adds air so it reads at 26 px.

- The measured object, the base, the source and a control point are `view` (neutral).
- The tool's result is its class ink; a dimension's annotation is `reference` (yellow).
- A control point is a circle of radius 2.0, separated from the contour by a real path break of 1.15
  units — not a mask and not a background patch.
- An arrow or a tick is a terminator, not a node; its tip keeps 1.0–1.1 units of air from the line it
  points at.
- No digits and no unit inside an icon; the value and the words belong to the interface.
- The arrangement is the method: one line between two witnesses is the overall size, stacked lines
  from one witness are from the base, a row of witnesses is the chain, an arc is the angle, one
  leader is the radius, a line through the centre is the diameter.
- Footprint and coverage: ink inside `2.25..21.75` of the 24 grid, coverage 10–40 % of the 20 × 20
  working area.

## Procedure

1. Name the drawing kind — an assembly, a part, a cutting layout, a section — then the scale and the
   projection.
2. Lay out the views; the main view carries the most shape.
3. Dimension from the largest to the smallest: overall, from the base or the chain, then the small
   features; never twice.
4. Answer the shop's questions: which panel, which edge, which side, how thick, what grain, how many.
5. For an icon, choose the arrangement that names the method, then the terminator and the colours,
   then the gap; check it at 26 px and at 200 %.
6. Run `cargo xtask ui-rules`, `cargo xtask skills` and `cargo xtask ci`.

## Stop and ask the owner

- A convention that contradicts the shop's practice: first- or third-angle, a shop standard, a
  customer's title block.
- A dimension that needs document data the model does not have: a tolerance, a fit, a datum id.
- A new drawing surface, export format or icon family — that is a plan entry, and often an ADR.

## Checklist

- [ ] Scale and projection are stated; no mix of first and third angle.
- [ ] One thick weight for visible contours; every other line is thin.
- [ ] Each size is dimensioned once; no closed chain without a reference link.
- [ ] The smallest size is nearest the object; the largest is farthest.
- [ ] The near point's extension stops at its own dimension line.
- [ ] Radius leaders are angled; diameter lines pass through the centre.
- [ ] Text sits above the line, readable, with the unit stated once.
- [ ] The icon keeps the source neutral, the result in its class ink and the dimension in `reference`.
- [ ] A control point has a real gap; an arrow has air from the line it points at.
- [ ] The order package, not the drawing, carries the placeable order data.
