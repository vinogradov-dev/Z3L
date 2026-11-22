# UI icon resources

One SVG per icon, grouped by the theme of the surface that shows it. `crates/ui/src/icons.rs` embeds
this tree and draws an icon into a themed button; the theme and the icon name are the keys the UI
asks for, so this layout is part of the code's contract.

This file is normative for the **file format** — viewBox, allowed elements, naming, the theme table.
`STYLE_GUIDE.md` is normative for **meaning and composition**. When the two disagree, the guide wins
on meaning and this file wins on format; fix the losing file in the same change. Read both only when
adding, editing or reviewing an icon. `AUDIT.md` in this directory holds the measured state of the
set and the pipeline, not the rules.

## Layout

```text
assets/icons/<theme>/<name>.svg
```

| Theme | Icons | Shown by |
|---|---|---|
| `contour` | `line`, `line-angle`, `polyline`, `arc`, `arc-centre`, `arc-radius`, `rectangle`, `rectangle-centre`, `rectangle-centre-directed`, `rectangle-edge`, `circle`, `circle-diameter`, `circle-three-points`, `circle-tangent`, `polygon`, `polygon-apothem`, `polygon-edge`, `trim`, `extend`, `offset`, `fillet`, `chamfer`, `break`, `construction-horizontal`, `construction-vertical`, `construction-parallel`, `construction-angle`, `dimension-linear`, `dimension-baseline`, `dimension-chain`, `dimension-angular`, `dimension-radius`, `dimension-diameter`, `measure`, `boolean-union`, `boolean-subtract`, `boolean-intersect`, `close`, `cancel` | The ribbon *Sketch* tab |
| `primitives` | `cuboid`, `cylinder`, `sphere`, `cone` | The ribbon *Model* tab |
| `bodies` | `extrude`, `pull-area`, `revolve`, `sweep` | The ribbon *Model* tab |
| `scene` | `product`, `panel`, `panel-shape`, `board`, `overlay`, `row`, `fit`, `post`, `shelf`, `wall`, `void`, `assembly`, `camera`, `light` | The ribbon *Furniture* and *Scene* tabs |
| `view` | `orthographic`, `perspective`, `frame-all`, `maximize`, `restore`, `home`, `grid-snap`, `snap-all`, `snap-vertex`, `snap-intersection`, `snap-midpoint`, `snap-centre`, `snap-perpendicular`, `snap-tangent`, `snap-edge`, `snap-face`, `snap-axis`, `snap-space`, `snap-foot`, `orbit`, `pan`, `zoom`, `layout-single`, `layout-columns`, `layout-rows`, `layout-quad`, `shaded`, `shaded-edges`, `wireframe`, `xray`, `translucent`, `sliders`, `shadows`, `axon`, `front`, `left`, `top`, `right`, `bottom` | The ribbon *View* tab, the floating viewport cluster, the header |
| `actions` | `confirm`, `dismiss`, `cancel`, `undo`, `redo`, `delete`, `add`, `remove`, `duplicate`, `rename`, `copy`, `paste`, `file-new`, `file-open`, `file-save`, `file-export`, `file-import`, `print`, `settings`, `search`, `command`, `visible`, `hidden`, `lock`, `unlock`, `expand`, `collapse`, `panel-left`, `panel-right`, `warning`, `error`, `info`, `help`, `pin` | Menus, dialogs, strips, the command palette and the shortcut reference |
| `window` | `minimize`, `maximize`, `restore`, `close` | The window row of the header; drawn only when the system frame is off |
| `transform` | `object` | The transform band's object-move cell and the node menu's gesture entries |

A theme has no other files: no sprites, no sheets, no per-scale variants. The UI rasterises one file
at the pixel size it draws, so a second size would only drift from the first.

`assets/icons/STYLE_GUIDE.md` is documentation, not an icon: the loader reads only
`assets/icons/<theme>/*.svg`.

## Brand mark

`assets/logo.svg` is the product wordmark, and it is not an icon: no theme, no 24-unit grid, no role
ink. It is one file with one white ink and `viewBox="2.2 2.2 36 12"` — 36 x 12 units, aspect 3 — so the
loader multiplies it with whatever ink the surface asks for and `crates/ui/src/icons.rs` keeps the
ratio in `LOGO_ASPECT`. A redraw changes this file and that constant, and nothing else. The rules
below — the grid, the footprint, the role inks, the theme table — describe `assets/icons/` and do not
apply here (ADR 0136).

## Naming

- lowercase ASCII `kebab-case`, extension `.svg`; the file stem is the icon id.
- no `icon-` prefix, no numeric prefix, no size suffix: the theme directory already says what the icon
  is for, and the UI scale already says how large it is drawn.
- one drawing per file, one file per id. If one drawing serves two themes, it is two files: a theme
  owns its icons and may change one without touching the other.
- a theme directory is named after the toolbar panel or the UI surface it serves; `actions` is the
  chrome theme of `STYLE_GUIDE.md` §19. Renaming a theme or an icon is an API change: the same commit
  updates `crates/ui/src/icons.rs` and the table above.

## Format

- SVG 1.1, one root `<svg>` with `xmlns="http://www.w3.org/2000/svg"`, `viewBox="0 0 24 24"`,
  `width="24"`, `height="24"`, `stroke-linecap="butt"` and `stroke-linejoin="miter"`. The file is
  self-contained: no references to sibling files.
- a 24 × 24 unit drawing grid. The ink stays inside `2.25..21.75` on both axes and covers 10–40 % of
  the 20 × 20 working area, measured from the rasterised alpha (`STYLE_GUIDE.md` §7.4). The quarter
  unit inset is measured, not cosmetic: at 64 px a curve drawn to `2.2` reports `1.88`, because its
  edge lands in the boundary pixel. The rasteriser follows the UI scale (16 pt inline, 22 pt in a
  toolbar, 26 pt in the ribbon, times `pixels_per_point`), so a detail thinner than one unit
  disappears at a low scale.
- stroke geometry: `stroke` is a role ink from the table below. The v2 weights are `1.5` for a
  result, a silhouette or a contour and `1.2–1.3` for an annotation, an inner edge or a tick
  (`STYLE_GUIDE.md` §7.3). Caps are butt and joins are miter, set once on the root, so a stroke ends
  where its path ends; `stroke-linecap="round"` is allowed on a path that is itself a dot. A
  stroke-only shape keeps `fill="none"`, and a thinner than `1.2` stroke is not drawn at all.
- a control point is a `<circle>` of radius `2`, filled with the `view` ink, separated from the
  contour by a **real path break** of `1.15` (`STYLE_GUIDE.md` §7.6). The contour is shortened along
  its own direction; a mask, a clip, a dash pattern or a background-coloured patch is not a gap.
- tone: a face takes the same role ink with `fill-opacity` `0.12–0.18` (passive) or `0.45–0.60`
  (result or changed geometry) and keeps its full-strength outline. The tone is alpha on the role
  ink, so no theme needs a second file. `fill-opacity` is the only opacity attribute: `opacity`
  would dim the outline too. A dot, an arrowhead or a solid silhouette may use the role ink with
  `fill-opacity="1"` or no stroke.
- role ink only: one to three literals from the table below, never a colour of your own, and the
  icon's family ink is always one of them. The file says what an element means; the UI replaces each
  literal with the active theme's ink before it rasterises, so one master serves dark and light and
  a theme change needs no asset change. The table is closed: `crates/ui/src/icons.rs` asserts it and
  the tests reject a file that carries anything else.
- one-ink fallback: below a 20 px raster (or when the workspace asks for one ink) the UI rasterises
  the same master and multiplies its alpha by the theme's icon colour, so a file with role inks
  still reads as a monochrome glyph. Nothing in the file has to know which path drew it.

## Role inks

A family is what an icon means, not which toolbar shows it: `contour/arc` is `draw`,
`actions/delete` is `danger`, `view/grid-snap` is `reference`. The literal a master writes is the
dark ink; the UI maps it to the light ink on paper.

A file may carry two or three of these literals when it shows two or three things at once, and only
then: the dimension icons are the case the table exists for (`STYLE_GUIDE.md` §5.1) — the measured
object and the base are `view`, the annotation is `reference`, and the ink is what tells the two
registers apart at a glance.

| Family | Master (dark) | Light ink | Used by |
|---|---|---|---|
| `draw` | `#58C8DC` | `#1C7C92` | Create or modify sketch geometry |
| `modify` | `#A98BF0` | `#6B4BB0` | Transform and edit commands: undo, copy, rename, visibility |
| `model` | `#6E9EF7` | `#2E5CB8` | Solids, bodies and booleans |
| `scene` | `#55C7A4` | `#237A5E` | Panels, hardware, cameras and lights |
| `reference` | `#E8D44D` | `#7A6A00` | Rulers, dimensions, construction geometry, snaps, work planes and guides |
| `view` | `#B8C2CE` | `#5A6674` | Navigation, projection, display and the neutral chrome |
| `danger` | `#E06A6A` | `#B33A3A` | Destructive commands only |

The action accent is UI chrome and is never a role ink: an active tool is a background and a
border, not a recoloured glyph. Each preset's accent is also held at least 20 ΔE (CIE76) from every
ink of this table in both variants, because contrast alone cannot see a hue collision (ADR 0125).
The measures behind the table — the threshold, the contrast and hue floors and the rejected
alternatives — are in `AUDIT.md`, `plan/notes/U70.md` and `plan/notes/T8.md`.
- geometry only: `path`, `rect`, `circle`, `ellipse`, `line`, `polyline`, `polygon`, `g` and their
  `transform`, with the paint attributes `fill`, `fill-opacity`, `stroke`, `stroke-width`,
  `stroke-linecap`, `stroke-linejoin`, and nothing else. No `<text>` (a glyph that must be a character
  belongs in the UI, not in an icon), no `<defs>`, `<clipPath>`, `<mask>`, `<style>`, `<image>`,
  `<foreignObject>`, `<script>`, font, filter, pattern or gradient — the loader is `resvg`, and the
  effects it can do either cost raster time or read wrong at button size.
- two decimals at most, no editor metadata namespaces, no `<title>`/`<desc>`: the button's tooltip is
  the accessible name, and the icon carries no prose.

## Placeholders

Every file is empty (zero bytes) until the surface that owns it is built. An empty file means "not
drawn yet": the icon button draws its text label instead, so a surface works while its artwork is
still arriving. Keep the file empty rather than half-drawn — a whitespace-only file is still a broken
drawing, not a placeholder.

To fill an icon, replace the whole file with a drawing that follows the format above. The path, the
theme and the file name stay unchanged.

## Ownership

The icons are drawn for Z3L and carry the project license, MIT OR Apache-2.0. Do not copy an
icon set from another product (`AGENTS.md` §4.6): a third-party icon keeps its license file next to
the asset and gets a row in `ATTRIBUTION.md` in the same change.
