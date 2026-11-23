# 0200. A wall fills a void, and a back board is born from the gap

- Status: Accepted; amends the `fill` trait of 0097 and the fill rule of 0099
- Date: 2026-11-21

## Context

The owner asked for a wall tool — a furniture panel in the frontal plane — beside the post and the
shelf, and answered the one question the request raised: overlay or inset («накладная»/«вкладная») is
**not** the tool's question. The tool creates the frontal panel; how it is mounted is a property
chosen later.

The fill trait carried exactly two pairs: `10.10` vertical panel fills a void's height and depth,
`10.11` horizontal panel its width and depth. The table's third sheet part, `10.12` back panel,
carried `fill —` with the editor rule «no void fill», so a board in the frontal plane could only be
drawn or typed. The class table is the one statement of that rule, and a ribbon cell bound to a row
without the trait is disabled by `crates/ui/src/toolbars/ready.rs` — so the request changes the table,
not only the toolbar.

The stance already existed. ADR 0105 fixed the three positions a panel takes in a cabinet and called
the third one the wall: the post turned a quarter-turn to the right about `+Z`, its contour in `XZ`
and its material extruded towards `+Y`, face `A` pointing at the maker. Nothing had created it.

## Decision

**`fill` gains its third pair: `WidthAndHeight`.** A part carrying it takes the niche's width and
height, and the material's thickness is what the free axis — the product's depth — leaves. The row
`10.12` back panel carries it, so the same empty-space tool offers the wall as it offers the post and
the shelf, and the row answers to «стенка» among its aliases.

**The wall's creation asks nothing about mounting.** The part is born as the frontal fill of the gap
the pointer names: the outline is the section the walls leave at the pointer's depth, its base face on
the plane the pointer sits on, its material running towards the open front. Overlay and inset are
properties of a mounted front and belong to the part's own card, not to the gesture that creates it.

**The stance is 0105's, not a fourth one.** The wall is the post turned a quarter-turn to the right
about `+Z`, so its local `+X` (length) runs up the case, its local `+Y` (width) runs across the front
and its local `+Z` (thickness) towards `+Y`. The fill's free axis is therefore the product's depth,
the pointer picks it in whole millimetres (0103), and the two builds — the largest rectangle inside
the slab and the section itself — work unchanged. A wall whose two ends meet posts that lean in plan
takes an end bevel on each, by the rule 0107 and 0108 already state.

**One row changes and one string appears in the package.** `docs/domain/classes.md` §7 and §10, the
registry's `10.12` row and the order package's `fill` column all move together; the package spells the
pair `width-and-height`, the way its other two are spelled.

## Consequences

- A back board is created from the gap it closes, in one click and one undo entry, exactly as a post
  or a shelf is; the tool stops being the only pair of the trait the workshop actually needs.
- The toolbar gains a cell `furniture.panels.wall` beside the post and the shelf, with the same two
  builds and the same ghost; nothing new is stored and no settings key appears.
- The fill trait now answers three of the four positions a panel takes in a cabinet. A door in its
  opening is a wall whose placement is derived from the opening, not a fourth pair, and a drawer front
  is the same board on a different host; both stay out until their host rule exists.
- Overlay versus inset is a property that does not exist yet in the document. A wall born from a gap
  is neither until the mounting rule lands, and nothing in this record decides which one it defaults
  to.
- Cost: the trait's bit field widens by one value inside the same two bits, so the order package and
  the class table must be read together — a reader that knows two spellings refuses the third loudly
  rather than silently.
- Revisit if a fourth pair is wanted. It would mean the fill trait is no longer about a panel's own
  axes but about a host, and that is a different record.

## Related

- [0097](0097-a-product-is-a-container-with-an-authored-envelope.md) — the trait vocabulary and the
  `fill` trait this record widens.
- [0099](0099-a-niche-is-the-space-a-frame-reserves.md) — the niche a wall is measured against.
- [0105](0105-a-panel-stance-is-the-flat-frame-turned.md) — the three stances, the wall among them.
- [0107](0107-an-end-bevel-is-an-angle-about-its-edge.md),
  [0108](0108-a-shaped-fill-bevels-its-ends-onto-the-walls.md) — the end bevel a shaped wall takes.
- [`docs/domain/classes.md`](../domain/classes.md) — §7 the row, §10 its editor behaviour.
- [`docs/domain/products.md`](../domain/products.md) — §7, filling a niche.
