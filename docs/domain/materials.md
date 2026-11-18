# Materials and catalogue

Status: pointer. The catalogue outgrew this document: it is the workshop's directory of nomenclature —
items, kinds, properties, families, packages, part documents, suppliers' prices and the rules that pick
and place them (ADR 0145).

The specification lives in:

- [Nomenclature directory](nomenclature.md) — items, kinds, properties, families, appearance and finish,
  part documents, packages, prices, revision and storage;
- [Processing and rules](technology.md) — machining, tools, placement rules and derived holes.

The two typed tables this document used to describe — `PanelMaterial` and `EdgeBandingMaterial` in
`crates/model/src/catalog.rs` — the `MaterialId` type and the mock catalogue file are temporary. They
are deleted by the change that lands the directory (ADR 0096: before the first release the format has
no past). The rules that survive the change and apply to a sheet material and to an edge banding are
those of the directory: a stable id that is never reused, lengths as integer micrometres, prices as
whole minor units with a declared currency, a missing record is a diagnostic and never a substitution,
and the directory is not part of the scene file.
