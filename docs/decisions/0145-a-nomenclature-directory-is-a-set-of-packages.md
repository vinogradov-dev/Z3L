# 0145. A nomenclature directory is a set of packages

- Status: Accepted
- Date: 2026-11-18
- Supersedes [0005](0005-material-catalogue.md)

## Context

The catalogue began as two tables — panel materials and edge banding — read from one mock JSON
file. The domain is larger than that. A workshop buys sheet stock, edge banding, fasteners, plugs,
profiles, long stock, worktops, plinths, paint, hinges, runners, drawer systems, doors and facades,
and it keeps furniture series, prototypes, tools and its own standards. The programs the owner
worked with — К3-Мебель and Базис-Мебельщик — put all of it in one relational database, because the
same record is read by a report, a cutting plan, a price list and an order.

Three requirements decide the shape, and none of them is met by two typed tables:

1. **The directory is edited in its own application**, which runs standalone or is started from the
   editor, and the editor picks the change up without a restart.
2. **One workshop at a time first, portable by copying**, with multi-user use possible later.
3. **Catalogues arrive from outside** — a supplier's hardware range, a board manufacturer's decor
   list, a furniture series — and are connected or disconnected as units, while a workshop's own
   additions must travel to another workshop as a file.

A relational engine would be the obvious answer, and it is not available: SQLite binds C
(`libsqlite3-sys`), which `AGENTS.md` §2 forbids, and a pure-Rust embedded store is a dependency we
have not measured a need for yet.

## Decision

**The directory is a set of packages of nomenclature items, and every consumer reads an immutable
snapshot of it.**

**One item envelope.** A catalogue record is a `NomenclatureItem`: a stable `ItemId`, a kind, a
name, a slug, an archived flag, typed properties, and nothing else that a consumer can special-case.

**`ItemId` is composite and has no meaning.** `(package, serial)`, rendered `0712-000034`: two `u32`,
allocated by the package that owns the record. It is unique without randomness, readable in a file,
and it says which package brought the item. It is never reused, and it is not a document `Id`.
An item that leaves the directory is *absent*; an item the workshop retired is *archived* and still
resolves, so a saved project keeps finding the hinge it was built from.

**Kinds, properties and families are registry rows, not code.** A kind row names the property schema
an item of that kind may carry and a set of traits from a closed vocabulary — `Stock::Sheet`,
`Stock::Linear`, `Stock::Piece`, `Banded`, `Nestable`, `Consumable`, `Supply::Bought`,
`Supply::Shop`, `PriceUnit` — and code reads a trait, never a kind code (§4.9: the digits identify,
the traits decide, as in ADR 0094). A property row declares its type, unit, range, requiredness and
an optional external key. A family row groups records for substitution and carries a rank; it is one
registry for every record, so a board decor, its edge banding and its plug meet in one place, which
is what "oak corpus, brown plug" needs and what an RGB comparison cannot give.

**A property value is a closed typed value**, not free JSON: `Length`, `Count`, `Money`, `Text`,
`Enum`, `Ref(ItemId)`, `Date`, `Color`. The set is checked against the schema when a package loads,
so no reader has to interpret a value it did not expect. A workshop's own property is a registry row
added in the directory application — that is also the integration surface: a property can declare
the external key a supplier writes, and an importer then needs no hard-coded mapping.

**Appearance and finish are two different things and both are stored.** `Appearance` — colour, a
texture reference, roughness — exists for the scene and the renderer and is translated into scene
data by the shell. `Finish` — kind, material or species, effect, coats, gloss — exists for the
report, and its facets are composed into words by `crates/text`, never concatenated in an exporter:
"фасад Вероника · шпон · орех · патина золото" is data plus a message, not a stored string.

**Prices are item properties with provenance**: value, unit (taken from the kind's `PriceUnit`),
currency, source and date. They are edited one row at a time in the directory application and
written in batches by an importer, one batch being one history entry. Stock on hand is not a
property — it is volatile, and it arrives as an overlay package with its own date and source.

**External systems are named, not spelled.** An item carries `ExternalKey { system, key }` rows and
`system` comes from a registry, so `1C`, `1с` and `1C:ERP` cannot become three worlds. Importers are
ports in the directory application: a validated batch, a report of applied, skipped and conflicting
rows, and a refusal naming system and key on a conflict. Nothing is matched by name.

**A package is a manifest and its rows.** The manifest names the package, its version, the packages
and registry revisions it requires, and it lists its part documents and rule tables. A package is
connected, disconnected, exported and imported as a unit; an id that two packages claim is a
refusal, never a merge. Registry revisions are what a package requires, so a supplier package that
needs a property this directory does not have is refused with that sentence.

**Readers hold a snapshot and swap it whole.** A directory carries a revision; the shell watches it
off the frame path, validates the new packages, and replaces the snapshot at a frame boundary. The
editor's derived data — panel thickness, appearance, resolved hardware, diagnostics — is recomputed
for the new revision, and the undo history is untouched, because a directory is not the document.
An update invalidates nothing a person did.

**Storage is behind a port and is not a decision yet.** `DirectorySource` in `model` offers a
snapshot and a revision. v1 implements it as a folder of packages written atomically, which copies
as a folder, diffs as text and needs no dependency. An embedded store can replace it behind the same
port when the schema settles or the directory outgrows a load-into-memory snapshot; the model is
normalized now so that tables map one to one then.

**The directory is an application resource, not project data.** A read-only bundled default plus a
writable user folder; the project stores item ids and fallbacks only. The workshop profile — the
numbers a shop sets: panel thickness, shelf pitch, maximum span, gaps — is a package of the same
directory, because it is written by the same application, and it is not nomenclature.

**Before 1.0 the format has no past** (ADR 0096): the mock file, the `MaterialId` type and the two
typed tables are deleted by the change that introduces the envelope, with one fixture per current
schema.

## Consequences

- Adding "baluster" or "worktop blank" is a registry row in a package. Adding a *trait* is a code
  change, and that is the line the design is drawn on: a new behaviour is a decision, a new kind is
  data.
- A package is the unit of sharing, of supply and of blame: a bad row names its package, and
  disabling a package is a visible act rather than a silent loss of items.
- The editor depends on a revision it does not own. That is the price of editing in a separate
  application, and it is paid once, in the snapshot swap, instead of in every panel card.
- The directory application is a second binary on the same stack — `egui`/`eframe`, `scene` and
  `render` as libraries — and it is registered in the layer map and `PROJECT.md` in the same change.
- Validation is ours. A malformed package is a sentence with a package name, a kind, a property and
  a value, and every consumer refuses it rather than reading around it.
- The export package is the frozen copy of a project, so it must name the directory revision it was
  built against; a directory that moved on cannot silently change an approved order.
- Revisit when a directory no longer fits in memory as a snapshot or when several people must edit
  one directory at once: then an embedded store or a server wraps `DirectorySource` and the model
  does not change.

## References

- ADR [0005](0005-material-catalogue.md) (superseded), [0031](0031-external-resources-keep-their-own-system-of-record.md),
  [0034](0034-nodes-carry-typed-properties.md), [0094](0094-a-class-registry-gives-the-machine-its-semantics.md),
  [0096](0096-before-the-first-release-the-format-has-no-past.md), [0120](0120-sheets-and-money-are-additive-in-the-package.md).
- `docs/domain/nomenclature.md` — the structure this record decides.
- `docs/domain/materials.md` — the record this one replaces.
- Plan: O3, O4–O9.
