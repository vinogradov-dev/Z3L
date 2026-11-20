# 0184. A price names its money, its source and its age; stock is an overlay

- Status: Accepted
- Date: 2026-11-20

## Context

ADR 0148 decided that a price is an item property with provenance and that stock on hand is an
overlay. The code had neither: `PropertyValue::Money` stored a bare count of minor units, the
currency was a field only the starter package ever set, `PriceUnit` was four traits and no type, and
nothing read a supplier's price list. A priced record was therefore readable only by the one
directory that happened to name a currency — a folder written by a workshop lost the money it was
priced in.

## Decision

**The currency is inside the value.** `PropertyValue::Money` carries a `catalog::Price`:
`{ minor, currency, source, date }`. A currency is a validated `Currency` (ISO 4217 shape, non-zero
minor scale), so "a price without a usable currency" is not a case to check — it cannot be built. The
one way a package can offer such a price is a file, and the reader refuses it with the ISO 4217
sentence.

**The unit stays out of the value.** What one price buys is a trait of the kind:
`PriceUnit::of(kind)`, `Directory::price(item) -> PriceView { price, unit }`. A kind that names two
price units is refused when its row loads, because two would leave the reading to the order the file
lists them in. A directory's currency is derived from its priced records and a second currency is
refused naming both records, so the order package's `money` block keeps one answer.

**Provenance is typed, not a string.** `PriceSource` is `Typed` or `Imported { system }`, and the
system is the registry row of §5.3 — an item's foreign keys now resolve through a declared system,
and `Directory::by_external` answers the record a `(system, key)` pair names. Two records claiming one
pair are refused, because an import would otherwise land a supplier's price on whichever record came
last.

**Stock is a file beside the records.** `StockOverlay` is `packages/<package>/overlays/<name>.json`:
a source, a date and one count per `ItemId`. `Directory::on_hand` answers the count with the overlay,
source and date it came from, the folder's revision stamp watches the file so a new count is one
reload, and removing the overlay leaves every other query answering as before. A record that no
overlay names answers `None` and not a zero.

**An import is one committed change.** `PriceBatch` reads `system,key,price_minor,currency,date` and
applies or refuses each row with a sentence; `FolderWriter::save_items` validates the candidate
directory once for the whole batch and bumps the package's manifest once. A directory's history is
its revision (§13), so a batch of forty prices is one entry in it, not forty.

**Age is a comparison of two dates, and the clock is the caller's.** `Price::freshness(today, limit)`
is pure; the directory application's card takes today and the workshop's limit from the shell, and
the order package judges every price against its own `exported_at`, writing `price_date` and
`price_stale` in the `materials` and `sheets` rows. A typed price has no date, so it is `Undated`
rather than old.

## Consequences

- A packet of a directory that names no currency can no longer hold a price; the starter package
  refuses a stocked board without one, which is ADR 0120's rule made structural.
- The item file's price shape changes, so the format has no past (ADR 0096) — no migration, no alias.
- The order package gains two columns per priced table: appending is not a breaking change
  (ADR 0120), and a version-1 reader that ignores unknown columns keeps working.
- The workshop's shelf life is a number the shell supplies; where a shop's own numbers live is O2, and
  until it lands the directory window carries a 365-day default recorded in `plan/debt.md`.
- A supplier's price list must carry the foreign keys its records were matched by; a list that names
  articles instead is not read until an importer maps them, and a mistyped key is a refused row, never
  a price on the wrong record.

## References

- [0148](0148-a-nomenclature-directory-is-a-set-of-packages.md) — the directory of packages, prices as
  properties, stock as an overlay.
- [0120](0120-sheets-and-money-are-additive-in-the-package.md) — money is never a bare integer, and the
  package grows by appending.
- [0096](0096-before-the-first-release-the-format-has-no-past.md) — the shape change needs no migration.
- [0031](0031-external-resources-keep-their-own-system-of-record.md) — a foreign system keeps its own
  record, and a key names it.
- [`docs/domain/nomenclature.md`](../domain/nomenclature.md) §5.3, §11; [`docs/domain/order-export.md`](../domain/order-export.md) §3–§5.
