# 0124. The icon language is v2: a 24 grid, a 20 × 20 footprint and a seventh role

- Status: Accepted
- Date: 2026-11-15

## Context

U46 drew a set in which every icon carried a contour of `1.6–2.6` units, a `3..21` footprint and a
20–45 % coverage band, and U70 gave the set six role inks. That language was built for a 16–22 pt
glyph and a glyph that is one shape: a line, a panel, a bin. The dimension set breaks both
assumptions. A dimension is two registers at once — the measured object and its annotation — and its
witness, dimension line, tick and arrowhead are four weights of line inside one glyph. Drawn at v1's
weights they became a blob; drawn at v1's neutral ink the annotation disappeared into the object.

The owner reviewed a rendered demo of a second language on 2026-11-15 and approved it: a 24 grid, an
ink footprint of `2.25..21.75`, `1.5` for a contour and `1.2–1.3` for an annotation, butt caps and
miter joins, control nodes of radius `2` with a real `1.15` break, one class ink for the result and
the neutral `view` ink for a source or a control point, and a seventh role — `reference` — for the
measuring register. The demo lived in `/tmp`; the measurements are in `plan/notes/T6.md`.

## Decision

**The v2 language is the icon language of FurniCraft.** It states its own grid, footprint, weights,
caps and node gap, and the pipeline, the gate and both icon documents follow it:

- the ink stays inside `2.25..21.75` on the 24 grid and covers 10–40 % of the 20 × 20 working area,
  read off the rasterised alpha by the test in `crates/ui/src/icons.rs`;
- a contour, a result and a silhouette are `1.5`; an annotation, an inner edge and a tick are
  `1.2–1.3`; caps are butt and joins are miter, set once on the root;
- a control point is a `view` circle of radius `2`, and the contour it belongs to stops `3.15` from
  its centre — a real path break, never a mask, a clip or a background patch;
- `reference` is the seventh `Family`, with `#E8D44D` on dark and `#7A6A00` on paper, preset
  independent like the other six; a dimension icon carries `view` for the object, the base and the
  control points and `reference` for the annotation;
- the ribbon glyph is 26 pt in a 38 × 34 pt cell and a toolbar glyph is 22 pt, because the thinner
  line and the wider footprint need the points that v1 spent on stroke weight.

The language is applied a tab at a time: the *Sketch* tab, the ruler and the six dimension types
ship with this record, and T7 redraws the rest of the set. Until a tab is redrawn its files keep
the v1 metrics and stay inside the new gate, which is why the coverage band widened rather than
moved.

## Consequences

One band now accepts a v1 contour and a v2 annotation, so a redrawn tab and an unredrawn one can
live in the tree at once and the gate never has to be switched. The cost is that the band no longer
says "this is v2" on its own: the weights and the node gap do, and a drawing session reads them
from `STYLE_GUIDE.md` §7.3 and §7.6 rather than inferring them from a failing test.

A 26 pt ribbon glyph makes the open ribbon 80 points tall instead of 76, so the floating ribbon and
the side panels moved together; the metrics are asserted in `crates/ui/src/theme.rs`.

A seventh role is a second hue in one file, which the one-ink rule now has to name: an icon may carry
up to three literals from the table when it shows three things, and the `view`/class pair is the
case the rule exists for. A file that carries two inks for decoration is still a defect, and the
`reference` yellow must keep clearing 3:1 on the panel and the ribbon in both variants — the test
that measures it now walks seven families.

## References

- `assets/icons/STYLE_GUIDE.md` §1.1, §5.1, §7.1–§7.6 — the language as a drawing rule.
- `assets/README.md` — the role table, the format and the node rule.
- `crates/ui/src/icons.rs` — `Family::Reference`, `RoleRamp`, the footprint gate.
- `crates/ui/src/theme.rs` — `RIBBON_ICON_POINT_SIZE`, `RIBBON_CELL_WIDTH`, `RIBBON_ROW_HEIGHT`.
- `.pi/skills/draw-icon/SKILL.md`, `.pi/skills/drafting/references/dimensions.md` — what a drawing
  session reads before it draws.
- Plan: T6.
