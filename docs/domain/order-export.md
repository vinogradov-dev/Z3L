# Order package

Status: domain specification. This document is the published contract between Z3L and the
programs that read an order: reports, drawings, cutting lists, CNC. It defines the package layout,
its tables and columns, the number rules, what the package resolves for the reader, and how the
contract is versioned.

Related decisions: [0095](../decisions/0095-the-order-package-is-a-published-snapshot.md),
[0094](../decisions/0094-a-class-registry-gives-the-machine-its-semantics.md),
[0005](../decisions/0005-material-catalogue.md).

## 1. What a package is

A one-way snapshot of one order, written to be read by software we do not control. It is **not** a
second document format: Z3L never reads a package back, and the scene file (`.z3l`) remains the
only import format. It is also **not** a dump of our internal structures — that is what makes reports
break when a program changes. A package is a contract: it promises a small, boring, stable set of
flat tables, and every change to them follows §7.

Two facts shape it:

- the document is not self-contained — a panel stores a material `id`, while names, thicknesses and
  banding profiles live in the catalogue — so the package **embeds the catalogue rows the order
  uses** and is self-contained;
- most of what a report needs is derivation (measured size with banding, area, banding length, the
  product a part belongs to), so the package **contains those numbers already resolved**. A reader
  that re-derives them will eventually disagree with the program.

## 2. Layout

```text
cabinet-2026-11-12.fcpkg/
  manifest.json      the entry point: format, version, units, tables, their columns and keys
  products.jsonl
  nodes.jsonl
  parts.jsonl
  banding.jsonl
  contours.jsonl
  facades.jsonl
  fittings.jsonl
  machining.jsonl
  materials.jsonl
  sheets.jsonl
  classes.jsonl
  properties.jsonl
  products.csv …     one convenience copy per table, for spreadsheets; not the contract
```

`--format single` writes the same content as one JSON file with the tables embedded, for sending by
mail. Both forms carry the same manifest.

The program writes a package from a saved scene, with no window and no GPU:

```sh
z3l export cabinet.z3l --out cabinet-2026-11-12.fcpkg                       # the directory
z3l export cabinet.z3l --out cabinet-2026-11-12.json --format single         # one file
```

`--no-timestamp` leaves `exported_at` out, so two exports of one revision are byte-identical and a
package can be compared in a test. The manifest records the scene's file name and the document
revision; `order` and `document.hash` are filled by a caller that knows them, not by the command
line, which has neither an order number nor a hash.

A table is **JSON Lines**: one record per line, one JSON object per record, UTF-8, LF, no BOM. That
choice removes every dialect question a CSV has — quoting, separators, decimal comma, encoding — and
any language reads it in a few lines. Every table also has a CSV copy for people and spreadsheets. A
test asserts that each CSV copy carries exactly the columns its JSONL table declares. The manifest
lists the tables in the order the directory uses.

## 3. Manifest

```json
{
  "format": "furniture.order",
  "version": 1,
  "app": "0.1.0",
  "exported_at": "2026-11-12T21:40:00Z",
  "document": { "file": "cabinet.z3l", "revision": 41, "hash": "3f9c…" },
  "order": { "number": "2026-114", "version": 3 },
  "units": { "length": "um", "angle": "mdeg", "area": "mm2", "rotation_scale": 1000000000 },
  "rules": { "kerf_um": 4000, "trim_um": 10000 },
  "money": { "currency": "RUB", "minor_scale": 100 },
  "counts": { "products": 3, "parts": 57, "…": 0 },
  "tables": [
    {
      "name": "parts",
      "file": "parts.jsonl",
      "key": "node_id",
      "columns": [
        { "name": "node_id", "type": "integer" },
        { "name": "length_um", "type": "integer", "unit": "um" }
      ]
    }
  ]
}
```

The manifest describes its own tables, so a reader needs no document of ours to identify a column. A
column entry names its type (`integer`, `string`, `boolean` or `json`) and its unit where one
applies. `document.revision` is always present; `document.file`, `document.hash`, `order`,
`exported_at` and `money` are optional. The manifest and the tables are generated from one column
table in the exporter, so a declaration and a record cannot disagree.

The `rules` block declares the cutting rules the package applies to its own numbers and a reader
applies to its own: `kerf_um` is the material a saw blade removes between two neighbouring parts and
`trim_um` is the strip cut off **each** side of a sheet before the first part. Version 1 declares the
programme's default; the numbers are data, and a workspace setting replaces them without a new major
version.

The `money` block gives the currency every `price_minor` is counted in — the ISO 4217 code and how
many minor units make one unit (`100` means 12345 is 123.45). It is absent when the catalogue carries
no prices, and then every `price_minor` is zero. There is no price without a currency: a bare number
is not money.

A price also carries the day it was measured (`price_date`) and, when the caller names a shelf life,
a verdict (`price_stale`). The age is judged **against the package's own `exported_at`** and not
against a clock read later: a package is a frozen copy, and one file must not say two things about
the same price. `--no-timestamp` therefore writes the dates and no verdict.


## 4. Numbers

- Lengths: integer **micrometres**.
- Angles: integer **millidegrees**.
- Areas: integer **square millimetres**.
- Money: integer **minor units** of the currency the manifest's `money` block names. Zero means the
  catalogue carries no price for that sheet.
- Rotations: four integer quaternion components scaled by `rotation_scale` (1e9), because a float in
  the contract would let a reader and the program disagree in the last bits. `units.length` and the
  scale are declared once in the manifest.
- No floats anywhere in the machine tables. Formatted values (`720.0`, `мм`) exist only in the CSV
  copies and in `name`/`label` columns meant for people.

## 5. Tables

Every table is flat, carries its keys and is ordered by its key, so two exports of one revision are
byte-identical. The column lists below are the contract; the manifest repeats them for a reader.

### `products`

| Column | Meaning |
|---|---|
| `node_id` | the product node |
| `parent_id` | its parent in the tree |
| `name` | the user's name |
| `class` | the class code, e.g. `10.01.03` |
| `level` | `Product` |
| `width_um`, `height_um`, `depth_um` | the product's envelope: authored on the container, never derived |
| `content_width_um`, `content_height_um`, `content_depth_um` | the derived bounds of its parts in the product frame, so a reader sees whether anything pokes out |
| `part_count` | panels that belong to it |
| `sheet_area_mm2` | total core area of those panels |
| `banding_um` | total banding length |
| `fitting_count` | fittings inside it |
| `price_minor` | the sum of its parts' prices |

The envelope is the product frame's width, depth and height: `width_um` is X, `depth_um` is Y and
`height_um` is Z. Content bounds are the union of the nominal boxes of the panels below the product;
a product with no panels reports three zeroes.

### `nodes`

The tree, for drawings and for anything a flat table does not answer:
`node_id`, `parent_id`, `name`, `class`, `level`, `path` (the names from the root, `/`-separated),
`x_um`, `y_um`, `z_um`, `qw`, `qx`, `qy`, `qz` (the local placement, rotation scaled as in §4). The
document root is included with `parent_id = 0` and its own name as `path`; no other row has a zero
parent.

### `parts`

One row per panel that is cut, with every number a cutting list or a parts report needs:
`node_id`, `product_id` (the nearest product ancestor, computed by one tree walk and never stored — this is what
makes a per-cabinet report a `GROUP BY`; zero outside every product), `class`, `material_id`,
`thickness_um`, `length_um`, `width_um` (nominal), `measured_length_um`, `measured_width_um` (with
banding and the panel's end trims), `area_mm2` (the core region, which is what is cut from the sheet), `grain` (`none`,
`length`, `width`), `nesting` (`member` or `never`, from the class; empty when the class says
nothing), `banding_total_um`, `price_minor` (the part's share of one sheet's price, rounded to the
nearest minor unit; zero when the material has no price). Version 1 writes `grain = none` because the
document does not store a grain direction yet; the column is present so a later document can fill it
without a new table.

The price share is the part's core area over the nominal sheet area, with no nesting efficiency
applied: how many parts a sheet really holds is the reader's layout, and a guessed efficiency would
be a rule the package never declared.

### `banding`

One row per banded unit, because banding belongs to a contour segment and a contour part has more than
four edges: `node_id`, `unit` (`edge:<id>`, `corner:<id>` or `segment:<id>`), `material_id`,
`thickness_um`, `width_um`, `included_in_size` (whether the banding was added to the nominal size),
`length_um` (the nominal length of the unit it covers).

### `contours`

A panel's outline is stored exactly as segments in micrometres, and the package hands it over without
recomputation — this is what a drawing, a nesting program and a CNC post need, and it is why no
floating-point millimetres appear anywhere: `node_id`, `ring` (`outline`; hole ids are reserved),
`seq` (travel order from zero), `kind` (`line` or `arc`), `x_um`, `y_um` (the segment start),
`x2_um`, `y2_um` (the end, lines; zero on arcs), `cx_um`, `cy_um` (the arc centre, zero on lines),
`clockwise` (arcs; false on lines). A generated rectangle starts at the panel's zero corner with its
end trims applied, so a cut end starts inside the frame and a let-out end reaches past it; a contour
panel is handed over in the order its ring stores. The banding lengths and the measured sizes come
from that same ring. An arc's radius and angle follow from its stored start, its centre and the next
segment.

### `facades`

One row per panel whose class closes an opening: `node_id`, `product_id`, `class`, `material_id`,
`length_um`, `width_um`, `thickness_um`, `supply` (the class's purchase channel: `sheet`, `linear`,
`piece` or `shop`), `glass` (the panel material's kind names glass). A hinge count waits for the
hardware catalogue (M8), which is where a fitting becomes more than a classed node.

### `fittings`

`node_id` (the fitting), `host_id` (its parent, the object it is mounted in), `product_id`, `class`,
`article` (empty until the catalogue carries articles), `qty` (one per node in version 1). A fitting
is a node whose class has `level = Fitting`; the exporter special-cases nothing else.

### `machining`

`machining_id`, `node_id`, `op` (`cutout`), `face` (`A`, `B`, or empty on an edge anchor),
`x_um`, `y_um`, `diameter_um` (reserved for holes), `depth_um` (zero for a through cut), `params`
(a JSON object for the operation-specific fields, so a new operation does not need a new table). A
rectangular cutout carries `shape`, `width_um`, `height_um` and `depth` (`through` or `blind`); a
contour cutout carries its `segments` in the same shape as the `contours` table.

### `materials`

The directory rows the order uses — this is what makes the package self-contained:
`material_id`, `name`, `kind`, `thickness_um`, `width_um` (the banding profile width, zero for a
panel material), `color`, `sheet_length_um`, `sheet_width_um` (the stock sheet the panel material is
bought in, zero when it is not stocked), `stock_group` (the workshop's own grouping, empty when it has
none), `price_minor` (the price of one sheet, zero when the catalogue carries no price), `price_date`
(the day the price was measured, `YYYY-MM-DD`, empty when the record carries no date) and
`price_stale` (1 when the caller declared a shelf life and the price is older than it, 0 otherwise).
The stock columns describe a panel material's stock; a banding material is not cut from a sheet and
writes zeroes and an empty group.

`material_id` is the directory item's written id, `package-serial` such as `712-34`: an item's
identity is composite, so one number cannot name it (ADR 0148).

### `sheets`

One row per stocked panel material the order cuts from — the per-sheet cutting plan:
`material_id`, `stock_group`, `sheet_length_um`, `sheet_width_um` (the nominal sheet),
`usable_length_um`, `usable_width_um` (the sheet after the declared `trim_um` on each side),
`price_minor` (one sheet), `price_date` and `price_stale` (the same two columns the `materials` row
carries, so a reader that prices from the sheet row needs no join), `part_count` (the parts the order
takes from this material),
`part_area_mm2` (the sum of their core areas) and `sheet_area_mm2` (the nominal sheet's area).

This is a plan, not a layout: the package hands over the sheet, its usable rectangle, the rules and
the parts grouped by material, and the nesting program decides where each part lands and how many
sheets that takes. A sheet count computed from an area ratio would be a nesting strategy — a hundred
percent efficiency no saw reaches — and this contract states the inputs instead.

A material the order does not cut from has no row, and a material the catalogue does not stock has no
sheet row at all, so the table is empty for an order whose materials carry no stock sizes.

### `classes`

The registry rows the order uses and every registered ancestor: `code`, `parent`, `slug`, `label`,
`supply`, `measure`, `nesting`, `extent`, `level`, `opening`, `mount`, `fill`, `operations` (a JSON
array), `retired`. A reader decides "worktop goes to linear purchase and never to nesting" from this
table, not from our digits. A family row carries its code, slug and label and empty traits. An
unknown code used by a document still gets a row, with the code and the empty fields, so no node
points at a missing row.

### `properties`

The user's own typed properties in long form: `node_id`, `key`, `kind`, `value`. The reserved system
keys (`class`, `component`, `prototype.*`) are already resolved in the `nodes` and `classes` tables
and are not repeated. A value is a JSON scalar where one suffices — boolean, integer, length in
micrometres, angle in millidegrees, text — and a JSON object for a choice (`key`, `label`) or a
structured value; a reference is an id and a class is its dotted code. A custom report reads this
table instead of asking us for a column.

## 6. What the package resolves, and what it does not

**Resolved by us:** measured sizes with banding, areas, banding length per unit and in total, the
owning product of every part, the nesting flag, the purchase channel and measure unit from the class,
material names and thicknesses, sheet sizes and stock groups, a part's share of a sheet's price and a
product's total, machining records in panel-local coordinates, report grouping.

**Left to the reader:** the nesting layout and the sheet count, drawings and dimensioning, CNC
toolpaths and post-processing. The package hands over the inputs — part sizes, grain, material,
banding, machining records, sheet sizes, prices — and the declared rules (kerf, trim) in the manifest,
never a chosen strategy.

## 7. Versioning

- `version` is a major number. A breaking change — removing, renaming, splitting or re-uniting a
  column, or changing a unit — is a new major and a new decision record. Adding a column at the end is
  not a breaking change.
- A reader must ignore unknown columns and unknown tables; this is part of the contract, not advice.
- One golden package per major lives in the tree at `crates/export/tests/golden/package`; a test
  asserts that a fixed revision produces byte-identical output, and another asserts that two exports
  of one revision agree. The golden is built without a timestamp; a real export puts one only in the
  manifest (`--no-timestamp` is the command line's spelling, K3).
- The class registry and the catalogue are append-only, so a package always resolves its own codes
  without asking which version of our program wrote it.

## 8. Reading it

Any language: parse `manifest.json`, then read one JSON object per line from each `*.jsonl`. In
Python, for example (documentation only — no Python is shipped or executed by Z3L):

```python
import json, pathlib
root = pathlib.Path("cabinet-2026-11-12.fcpkg")
parts = [json.loads(line) for line in (root / "parts.jsonl").read_text(encoding="utf-8").splitlines()]
by_cabinet = {}
for part in parts:
    by_cabinet.setdefault(part["product_id"], []).append(part)
```

## 9. Out of scope

An importer, a scripting runtime inside the application (a second mutation path, forbidden by the
project rules), an embedded database (`SQLite` would drag C sources in), and any column that exists
only to serve one consumer's report.
