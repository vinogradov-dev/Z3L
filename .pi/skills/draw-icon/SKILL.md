---
name: draw-icon
description: "Create, edit or review a Z3L icon: a toolbar geometry icon or a service action (confirm, cancel, undo, file, warning). Use only when adding or changing an SVG under assets/icons."
license: MIT
compatibility: pi
metadata:
  audience: agents
  workflow: ui-assets
---

# Draw a Z3L icon

Icons are assets, not code (`AGENTS.md` §4.12). This skill is the checklist for adding, editing or
reviewing one; it does not apply to code that merely displays an icon.

## Source of truth

| Document | Normative for |
|---|---|
| `assets/icons/STYLE_GUIDE.md` | meaning, composition, the three families, service grammar |
| `assets/README.md` | viewBox, allowed elements, naming, folder layout, theme table |

When they disagree, the guide wins on meaning and the README wins on format; fix the losing file in
the same change. Read only the sections you need — the guide is long.

A dimension, section or view icon also reads `.pi/skills/drafting/SKILL.md`: the drawing grammar
decides the arrangement, the terminator and the order; the icon adds the air a 26 px glyph needs.

`assets/icons/AUDIT.md` is evidence, not a rule: it records what the pipeline measures today and which
alternatives were rejected, and is read when reconsidering the pipeline (ADR 0055), not before drawing
an icon.

## 1. Name the family before drawing

- `geometry` — a CAD operation: line, extrude, panel, hole, array. Follows the
  `OBJECT + ACTION METHOD + RESULT` formula (guide §2) and the 2D/3D rules (§8).
- `view` — navigation and projection: orbit, pan, frame all, ortho/perspective. Flat, monochrome,
  heavy.
- `service` — chrome: confirm, cancel, undo, save, warning. Follows guide §19, not §8–§10.

A command in a toolbar stays a geometry icon; a save action next to tools stays a service icon.

Grammar says how the drawing is composed; the **role ink** says what it means and is a second
choice. The seven role families are `draw`, `modify`, `model`, `scene`, `reference`, `view` and
`danger`, and their literal hexes are in `assets/README.md`:

- a sketched entity (`contour/*`) is `draw`; a solid, body or boolean is `model`; a panel, camera,
  light or hardware is `scene`;
- undo, copy, paste, rename, visibility and other edit commands are `modify`;
- a ruler, a dimension, construction geometry, a snap, a work plane or a guide is `reference`;
- navigation, projection, display and neutral chrome such as file or help are `view`;
- `danger` is for destructive commands only (`actions/delete`), never a warning or an error status.

A dimension icon carries two registers in one file: the measured object, the base and every control
point are `view`, the annotation is `reference` (`STYLE_GUIDE.md` §5.1, the `drafting` skill).

## 2. Place it

- Geometry and view icons live in the theme named after the toolbar or surface:
  `assets/icons/<theme>/<name>.svg`.
- Service icons live in `assets/icons/actions/<name>.svg`; the full name set is in guide §19.6.
- The file stem is the id: lowercase ASCII kebab-case, no prefix, no size suffix.
- If the file does not exist yet, create it empty: an empty file is the "not drawn yet" placeholder
  and the button falls back to its text label. Never leave a half-drawn file.
- Add or update the row in the README theme table in the same change.

## 3. Answer the pre-draw questions

For a geometry icon (guide §16):

```text
Object:      what is depicted?
Action:      what changes or is created?
Method:      which input elements explain the method?
Result:      which part carries the emphasis?
Unnecessary: what can be removed?
```

For a service icon add:

```text
Convention:  which standard glyph does the user already know for this verb,
             and which enclosure (circle, triangle) marks its status?
```

If an element cannot be labelled with its role (guide §3), remove it.

## 4. Draw inside the contract

- `viewBox="0 0 24 24"`, ink inside `2.25..21.75`; centre near `(12, 12)`; coordinates on half units.
- Footprint and mass: the ink stays inside the fixed footprint and covers 10–40 % of the 20 × 20
  working area, measured as alpha-weighted ink (sum of alpha / 255). Below 10 % reads as a sketch,
  above 40 % as a blob. `crates/ui/src/icons.rs` asserts both from the raster.
- One ink for the drawing, and it is the family's literal from the table in `assets/README.md`
  (`stroke="#58C8DC"` for a draw icon, `#E8D44D` for a reference one). Butt caps and miter joins on
  the root, `stroke-width` `1.5` for a contour and `1.2–1.3` for an annotation. A stroke-only shape
  keeps `fill="none"`. A shape that genuinely means two things may use a second or third literal
  from the same table — never a colour of its own.
- A control point is a `view` circle of radius `2` with a real `1.15` path break; the contour stops
  `3.15` from its centre, never under a mask, a clip or a background patch (§7.6).
- Tone is alpha on the family ink: a passive face is the ink with `fill-opacity="0.12–0.18"` under
  a full-strength `1.5` contour; the result or changed geometry is `0.45–0.60` under the same `1.5`
  result line. A dot, an arrowhead or a solid silhouette may fill with the ink and take no stroke. Never
  a third tone, never a fill without an outline, never `opacity`.
- The UI maps each literal to the active theme's ink and falls back to one ink below a 20 px raster;
  it never needs a second file. No colour outside the table, no class, no CSS variable, no theme
  token in the file.
- Weight by role: annotation 1.2–1.3, contour and result 1.5, service 1.5–1.8 or solid. Nothing
  below 1.2 units, because the icon may be drawn at 16 px.
- Allowed elements: `path`, `rect`, `circle`, `ellipse`, `line`, `polyline`, `polygon`, `g`,
  `transform`. No `defs`, `mask`, `clipPath`, `style`, filter, gradient, pattern, image, font or text.
- Gaps are real path breaks, never a background-coloured patch or a mask.
- 3D uses the one canonical isometry of guide §9.1: `x = 0.866 (u − v)`, `y = 0.5 (u + v) − w`; a
  face circle is a `0.577` ellipse; a plate is a plate, not a cube. 2D icons stay flat (§8.1).
- Similar actions share a pattern; opposite actions share a grammar and differ in the distinguishing
  element, not in style.

## 5. Service icons

- One glyph = one action, identical in menu, toolbar, dialog and shortcut reference.
- Bare glyph = action; enclosed glyph (circle, triangle) = status.
- At most one modifier mark (plus, minus, exclamation) in a consistent corner with a real 1.5-unit
  gap.
- Heavier and simpler than geometry icons; drawn for 16 px, not shrunk from a toolbar drawing.
- A destructive action keeps its label or confirmation; an icon never replaces it.

## 6. Verify

- the action is clear without text, at 16 px, in one ink;
- the ink is the family's literal from the table and the file carries no other colour;
- the result is the strongest element; auxiliary geometry does not compete;
- the largest face carries the result tone: when a passive face is the biggest shape the glyph shows
  another form, as a narrow slab's top face turns an upright panel into a column;
- no decorative lines, no random detail, no white patch;
- the metaphor matches the operation, not a neighbouring command;
- a service icon uses the same glyph on every surface and distinguishes status by enclosure;
- the SVG parses and rasterises at the size the button draws;
- `cargo test -p ui --lib icons` passes: footprint inside `2.25..21.75`, coverage
  10–40 % of the 20 × 20 area, non-empty at 16 and 20 px.

## 7. Register and close

- The same commit adds the file, the README table row, the button that draws it and a test for the
  empty-file fallback; a rename also updates `crates/ui/src/icons.rs`.
- A new theme or a moved icon is an API change: update the README table and `plan/map.md`.
- Never copy an icon from another product; a third-party asset keeps its licence and gets a row in
  `ATTRIBUTION.md` (AGENTS §4.6).
- Run `cargo xtask ci` before finishing; icon-only changes still pass the plan and format gates.
