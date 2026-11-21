# The wardrobe recipe

The first furniture build an agent is measured against (J13): one hinged wardrobe, written as one
recipe, so the same numbers are a test rather than a conversation. The owner's words:

> распашной шкаф ш800 в2200 г460. Фасады в кромке 2мм. Сверху антресольная полка 360мм. У шкафа две
> секции: справа 5 полок, слева штанга под одежду. Крыша накладная. Дно опорное. 4 ножки высотой
> 50мм, диаметром 60мм.

Later decisions from the same conversation: the doors are **overlaid**, the hardware is **drawn
simply** while the model has no furniture entity for it, and every part is banded on its **visible
ends**, with 0.5 or 1 mm edge — whichever the workshop stocks.

## The files

| file | what it is |
|---|---|
| `wardrobe-800x460x2200.prompt.md` | the same job as a person words it: the title, the two sentences the owner gave, and every number read out of them — this is the text an agent is handed |
| `wardrobe-800x460x2200.jsonl` | the recipe: 38 commands, one per line, one batch and one undo entry |
| `wardrobe-start.z3l` | the scene it starts from: an empty document with the product frame (id 2) already made |
| `empty-scene.z3l` | the document `wardrobe-start.z3l` was made from, kept so the frame can be re-made |
| `catalogue/` | the workshop rows the recipe names: an 18 mm board, a 4 mm back sheet and a 1 mm edge |

## Running it

```sh
mkdir -p /tmp/verify/z3l && cp -r app/tests/recipes/catalogue /tmp/verify/z3l/nomenclature
XDG_CONFIG_HOME=/tmp/verify target/debug/z3l run app/tests/recipes/wardrobe-start.z3l \
  --commands app/tests/recipes/wardrobe-800x460x2200.jsonl --out /tmp/wardrobe.z3l
XDG_CONFIG_HOME=/tmp/verify target/debug/z3l query /tmp/wardrobe.z3l parts
```

`--dry-run` plans all 38 steps and writes nothing. The catalogue is a fixture because the ids in a
recipe name records of one package: `1-2` is the board, `1-3` the back and `1-103` the edge here,
and the machine's own workshop package names other rows with the same numbers.

## The numbers

Millimetres, in the product's own frame: the envelope is `[0..800] × [0..460] × [0..2200]`, and the
depth includes the overlaid fronts, so the carcass is 442 deep and the back sheet takes 4 of it.

**The two units of one command.** A recipe line carries `nominal_size` as `Length` — integer
micrometres — and `placement.translation` as `[f64; 3]` **millimetres** (`crates/model/src/placement.rs`
is where placement coordinates meet floating-point millimetres). The JSON schema states neither, so a
recipe that writes micrometres in both is a wardrobe a thousand times its size: measured on
2026-11-21, 2200 mm became 2 200 000 mm, `query diagnostics` said nothing (X116) and the render asked
for a 3 585 601 mm camera distance. Every recipe here writes sizes in micrometres and positions in
millimetres (X115).

| part | size | position | class |
|---|---|---|---|
| Дно опорное | 800 × 438 × 18 | z 50..68, the sides stand on it | 10.11 |
| Стойка левая / правая | 2114 × 438 × 18 | left is `+X`: 782..800 / 0..18, z 68..2182 | 10.10 |
| Крыша накладная | 800 × 438 × 18 | z 2182..2200, over both sides | 10.11 |
| Перегородка | 1736 × 438 × 18 | x 391..409, z 68..1804, up to the антресоль | 10.10 |
| Антресольная полка | 764 × 438 × 18 | z 1804..1822, 360 clear above it | 10.11 |
| Полка 1..5 (справа, `−X`) | 373 × 438 × 18 | x 18..391, 274 clear apart, the top cell 276 | 10.11 |
| Задняя стенка | 800 × 2150 × 4 | y 0..4, HDF `1-3` | 10.12 |
| Дверь левая / правая | 399 × 2150 × 18 | y 442..460, z 50..2200, 2 mm gap; left is x 401..800 | 10.20 |
| Ножка 1..4 | Ø60 × 50 | inset 60 from the carcass edges | — |
| Штанга | Ø25 × 373 | axis y 223, z 1700, in the left section x 409..782 | — |

The five right-hand cells are one clear opening each, by the domain rule that a quoted spacing is the
opening between two surfaces, not a pitch between centres: 1646 mm of space over six cells gives
274, and the last cell takes the 2 mm the division leaves.

A panel's local frame has its zero at the minimum corner, the plate in XY and the thickness along
`+Z`, so a part stands up by being placed with a quarter turn. The turns are the model's own stances
(`crates/model/src/project/fill.rs`, ADR 0105): the post is `rotation_y(-90°)` — `[0, -s, 0, s]` —
which puts the board's zero corner at the **bottom** and its thickness along `+X`; a `+90°` turn
hangs the origin at the top of the board instead (X117). The wall — the back and the doors — is
`[-0.5, -0.5, -0.5, 0.5]`: length up, width across `+X`, thickness along `+Y`. `RectEdge::Top` —
edge 3, the `+Y` edge — is the one a maker sees on a carcass board, so that is the edge banded; the
doors are banded on all four.

**Left is `+X`.** The front view looks from `+Y`, so what a person standing in front of the piece
sees on their left lies at `+X` (`crates/model/src/side.rs`): the shelves are the right-hand section
(`−X`) and the rail hangs in the left-hand one (`+X`). Writing "left" into a coordinate without that
rule is how the first build came out mirrored (X118).

## What was measured on 2026-11-21

- 38 steps, one undo entry, one write; the run leaves the document at revision 53.
- 14 panel parts: 3 × `10.10`, 9 × `10.11`, 1 × `10.12`, 2 × `10.20`; plus 5 bodies (4 legs, 1 rail).
- `query parts` reports the sizes above; `query diagnostics` reports no issue on any of the 14 parts.
- The world box of every part was computed from the recipe and checked by hand: the furthest
  coordinate is 2200 mm, nothing lies outside the envelope, and no two parts overlap. The divider
  was shortened to 1736 because a full-height one crossed the антресоль shelf.
- Banding totals 19 edges: 11 × 1 m and 2 × 5.098 m on the doors.
- `z3l render --camera fit` draws it: 800 × 1000 at 24 samples, a closed carcass with two overlaid
  doors, a top that overhangs the sides by nothing and the gap between the doors where it should be.
  The image is not kept in the tree — the command in "What is not verified" below is the record —
  because this project stores no pictures.

## What is not verified, and why

- **Nothing checks a part against its product.** `query diagnostics` answers panel problems only, so a
  part placed kilometres away reports nothing; the only reason the render above is right is that the
  recipe was read line by line (X116). The wardrobe's own failure is the proof: it built, listed and
  passed with every coordinate a thousand times too large.
- **The machine's own workshop directory could not build this wardrobe**: it stocks a 25 mm board and
  a 2 mm edge, so the recipe runs against the fixture in `catalogue/` instead (O15).
- **The recipe cannot make its own product.** The CLI opens the product before it runs the batch, so
  the frame has to exist already; `wardrobe-start.z3l` is that frame (J17).
