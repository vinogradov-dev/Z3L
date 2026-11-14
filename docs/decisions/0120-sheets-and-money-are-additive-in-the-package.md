# 0120. Sheet stock and money join the package as additive columns, and the rules are declared

- Status: Accepted
- Date: 2026-11-14

## Context

Version 1 of the order package (0095, K2) carries areas and stock groups but no sheet plan and no
money, because the catalogue stores only a nominal thickness and a colour. A workshop cannot buy from
an area: it buys sheets. So the catalogue now carries what it actually buys — the stock sheet, the
workshop's own stock group and the price of one sheet — and the package has to hand that over without
breaking a reader written against version 1.

Two temptations had to be refused. The first is a sheet count: `ceil(total area / sheet area)` looks
like a cutting plan and is a nesting strategy with a hundred percent efficiency, exactly what 0095
forbids. The second is money without a currency: a bare integer that a reader would have to guess.

## Decision

**The catalogue grows an optional stock block and an optional currency.** A panel material may name
its stock sheet (length and width), a stock group (the workshop's own label) and the price of one
sheet in whole minor units. The catalogue names the currency once — an ISO 4217 code and how many
minor units make one unit — and refuses a priced material while it names no currency. A price is
unsigned: a discount is not a price.

**The package adds columns and one table at the end of the contract, never inside it.** `materials`
gains `sheet_length_um`, `sheet_width_um`, `stock_group` and `price_minor`; `parts` gains
`price_minor`, the part's share of one sheet's price by core area, rounded once; `products` gains the
sum of its parts. A new `sheets` table carries one row per stocked material the order cuts from: the
sheet, its usable rectangle after the trim, the price, and the parts' count and area. `version` stays
1, because appending is not a breaking change and 0095 says so.

**The manifest declares the rules, the reader nests.** `rules` names the kerf and the per-side trim;
`money` names the currency and the minor scale, and is absent when nothing is priced. The `sheets`
row carries no layout and no sheet count: where a part lands and how many sheets that takes stays the
nesting program's job, and it now has every input it needs.

**Version 1 declares the programme's default saw rules** — a 4 mm kerf and a 10 mm trim per side —
because a workshop's saw is not catalogue data and no shop setting exists yet. The numbers are data:
a later setting replaces them without touching the layout of the contract.

## Consequences

- A version-1 reader that ignores unknown columns and tables keeps working, and one that looks for
  sheets or prices finds them without a new major version.
- A package from a catalogue with no stock carries an empty `sheets` table and no `money` block, which
  is a complete package rather than a broken one.
- Edge-banding material is bought per roll and has no sheet, so its `materials` row writes zeroes in
  the four new columns; a roll price needs a unit of its own and is a later, separate decision.
- A part's price is a cost allocation by area and never a purchase order: the reader's nesting decides
  how many sheets are actually bought.
- The default catalogue in the application ships sheet sizes and stock groups but no currency and no
  price, because inventing a shop's money is not the sample catalogue's job.

## Related

- [0095](0095-the-order-package-is-a-published-snapshot.md) — the package, its versioning and the
  rule that nesting stays with the reader.
- [0005](0005-material-catalogue.md) — the catalogue is external and session-scoped, which is why the
  package embeds its rows.
- [`docs/domain/order-export.md`](../domain/order-export.md) — the table and column contract.
- [`docs/domain/materials.md`](../domain/materials.md) — the catalogue record and its validation.
