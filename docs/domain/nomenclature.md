# Nomenclature directory

Status: domain specification, target state. This document describes the directory of nomenclature a
workshop works from: its items, their kinds, properties and families, the packages they arrive in,
the part documents that carry their geometry, the rules that pick and place them, and the contract
the editor reads them through. The program today reads one package: the starter rows in
`catalog::default_directory`, loaded as a `Directory` through the item envelope and its registries
(ADR 0148); a workshop's own packages arrive as folders beside the workspace settings. The instance
node and the part documents are the part of this specification that has not landed yet.

Related documents:

- [Scene document](document.md) — the scene file, the instance node and what a project stores.
- [Processing and rules](technology.md) — machinery, tools, placement rules and derived holes.
- [Panels and edge banding](panels.md) — the panel that references a material.
- [Order package](order-export.md) — the frozen copy an order is written as.
- [Prototypes](prototypes.md) — the parametric definitions that consume the directory.
- [Semantic classes](classes.md) — the class registry a part's objects take their class from.

Related decisions: [0094](../decisions/0094-a-class-registry-gives-the-machine-its-semantics.md),
[0148](../decisions/0148-a-nomenclature-directory-is-a-set-of-packages.md),
[0149](../decisions/0149-a-catalogue-part-is-a-document-instantiated-in-the-scene.md),
[0150](../decisions/0150-machining-is-derived-and-rules-are-tables.md),
[0031](../decisions/0031-external-resources-keep-their-own-system-of-record.md),
[0096](../decisions/0096-before-the-first-release-the-format-has-no-past.md),
[0120](../decisions/0120-sheets-and-money-are-additive-in-the-package.md).

## 1. Terms

- **Directory** — the whole set of packages a workshop has connected.
- **Package** — one transferable unit of the directory: a manifest and the records, part documents and
  rule tables it carries. A supplier's range, a board maker's decor list, a furniture series, a
  workshop's standards and its machine park are packages.
- **Item** — one nomenclature record: an `ItemId`, a kind, a name, typed properties.
- **Kind** — a registry row saying what an item is, which properties it may carry and which traits it
  has.
- **Trait** — a closed behaviour flag code reads instead of a kind code: sheet, linear, piece, banded,
  nestable, consumable, bought, shop, and the price unit.
- **Property** — a registry row declaring one attribute: type, unit, range, requiredness, external key.
- **Family** — a registry row grouping records for substitution, with a rank inside the family.
- **Part document** — a scene document under a narrow contract that carries one item's geometry and
  meaning.
- **Instance** — a scene node that is an item: its placement is document data, its contents come from
  the item's part document.
- **Overlay** — a package of volatile values attached to items by id: stock on hand, a supplier's
  price list.
- **Revision** — a monotonic counter of the directory's content. A reader holds one snapshot of one
  revision.

## 2. Three layers, kept apart

| Layer | How fast it moves | What it is |
|---|---|---|
| **Nomenclature** | slowly, by a person | items, kinds, properties, families, appearance, finish, part documents, compatibility |
| **Rules** | with the nomenclature | selection rules, fastener patterns and supplier tables (see [Processing and rules](technology.md)) |
| **Overlays** | often, from outside | stock on hand, a supplier's price list, external keys an importer fills |

A layer may reference the one before it and never the one after it: an item knows nothing of a
price list, and an overlay names items by `ItemId` only. Removing an overlay leaves the directory
complete; removing a package leaves items *absent*, which is a diagnostic and never a silent loss.

## 3. The item envelope

| Field | Rule |
|---|---|
| `id` | `ItemId`, composite `(package: u32, serial: u32)`, non-zero, unique in the directory, never reused, not a document `Id` |
| `kind` | a `KindCode` that exists in the connected packages' registry |
| `name` | non-empty, shown to people |
| `slug` | non-empty, ASCII, unique within the kind; for search, export and generated names |
| `archived` | when true the item still resolves and every card says it is out of production |
| `props` | the typed property set, validated against the kind's schema |
| `appearance` | optional: colour, texture reference, roughness — the render side (§7) |
| `finish` | optional: the reportable finish (§7) |
| `part` | optional: the part document (§8) |
| `external` | zero or more `ExternalKey` rows (§5.3) |

Rules:

- an item with no part document and no price is still a complete record: a workshop may know a material
  it does not stock;
- an item's kind never changes under the same id; a record that becomes a different thing is a new item
  and the old one is archived;
- a document that references an absent item keeps working from its fallback (§9.3), and the card
  reports it.

## 4. Kinds and traits

A kind row declares: `code`, `slug`, `label`, the property schema (the property codes it accepts,
which are required and which are optional), the traits it carries, and the categories its card groups
its properties under.

**Traits are the closed vocabulary and the only thing code reads** (ADR 0094 in the class registry,
repeated here for the directory):

| Trait | Means |
|---|---|
| `Stock::Sheet` | bought as a sheet: a stock block with two sides |
| `Stock::Linear` | bought by length: a stock block with one length or a profile |
| `Stock::Piece` | bought by the piece or the set |
| `Banded` | a panel made from it can be edge-banded, and the item can be the banding |
| `Nestable` | a part made from it is nested into a sheet |
| `Consumable` | not tracked on a part; a paint, an adhesive, a screw |
| `Supply::Bought` | the workshop buys the finished thing |
| `Supply::Shop` | the workshop makes it from bought stock |
| `PriceUnit::{Sheet, Metre, Piece, Set}` | what one price buys |
| `Expandable` | the item may carry a part document that instantiates other items |

The starter kinds, grouped the way the workshop speaks, are rows and not code:

| Group | Kinds |
|---|---|
| Boards and banding | sheet material, edge banding, worktop, wall panel, plinth, moulding profile |
| Long stock | profile, trim, plinth blank, handrail |
| Fixings | fastener (dowel, minifix, confirmat, screw, cam, shelf pin), plug, cap, glue |
| Turns and paint | finish material (veneer, lacquer, film, enamel, paint, stain), thinner, abrasive |
| Hardware | hinge, runner, drawer system, lift, handle, lock, leg, lighting |
| Doors and facades | facade, door slab, glass unit, mirror |
| Turned and carved | baluster, newel, carved detail |
| Tools | cutter, drill, saw blade (see [Processing and rules](technology.md)) |
| Decorative and other | panel decor, packaging, delivery service |

A new kind is a registry row in a package. A new trait is a code change and a decision, because a
trait is behaviour.

## 5. Properties

### 5.1. Value types

| Type | Payload |
|---|---|
| `Length` | integer micrometres (ADR 0003), with an accepted range per property |
| `Count` | an integer count, unsigned |
| `Money` | whole minor units plus the catalogue's currency |
| `Text` | a string with a maximum length |
| `Enum` | a slug from the property's own closed list |
| `Ref` | an `ItemId`; the referenced item exists, or the value is a diagnostic |
| `Date` | a calendar date |
| `Color` | a colour in the document's colour form, for display |

No floating-point physical size ever enters a property. A value is stored typed, so a reader never
interprets a string.

### 5.2. Registry row and validation

A property row declares `code`, `slug`, `label`, type, unit or enum list, minimum and maximum,
requiredness, and the categories of kinds that accept it. Loading a package refuses, with the package,
item, property and value named: an unknown type, a value of the wrong type, a value outside its
range, a missing required property, a property the kind does not accept, a duplicated property code,
and a `Ref` that resolves to nothing in the connected packages.

### 5.3. External keys

An item carries `ExternalKey { system, key }`. `system` is an `ExternalSystemId` from a registry row
(`1c`, `erp`, `supplier-egger`, …), never a free string, so `1C` and `1с` cannot become two worlds.
A property may carry its own external key, which is how an importer knows where a supplier's column
lands. Nothing is matched by name: a record without the declared key is not updated by an import.

## 6. Families

A family is one registry row per grouping a workshop reasons with: `oak`, `white`, `anthracite`,
`satin steel`. Fields: `code`, `slug`, `label`, `rank`, and an optional reference sample. Rank orders
records *inside* a family, from the closest to the reference sample outwards.

One registry serves every record, so a board decor, its matching edge banding and a plug meet in the
same family; that is what "oak corpus, brown plug" needs, and a colour comparison cannot give it. A
record may be in one family. Ranks are unique within a family and a duplicate is a validation error.

## 7. Appearance and finish

Two independent blocks, because they answer two different questions:

| Block | Question | Consumers |
|---|---|---|
| `Appearance` | what does it look like in the scene | `scene` and `render`, through the shell's translation |
| `Finish` | what is it finished with, as the shop writes it | reports, order package, card |

`Appearance` is colour, an optional texture reference inside the package, roughness and metallic
level. The shell translates it into scene material data; `scene`, `render`, `model` and `param` never
see a directory type.

`Finish` is structured facets: kind (`veneer`, `lacquer`, `film`, `enamel`, `paint`, `stain`),
material or species (`oak`, `beech`, `aluminium`), effect (`patina-gold`, `matte`, `gloss`), coat
count, and gloss. **The report line is composed, never stored**: `crates/text` turns the facets into
the sentence, so word order, cases and localisation live in one place. A finish detail that is an
object in its own right — a moulding, a carved element — is an item of its own kind, not a facet.

The two may disagree on purpose: a white facade (`Appearance`) with a matte enamel finish (`Finish`)
is a normal record, not a data error.

## 8. Part documents

An item may carry a part document: our own scene file, stored in the package, referenced by a
relative path and a content hash (ADR 0149).

The contract a part document must satisfy:

- exactly one root container;
- no camera, no light, no named view, no workspace or session state;
- only node kinds the scene format already has, plus the instance node;
- the objects that carry meaning are ordinary nodes with a class from the class registry and typed
  properties: an anchor, a drilling object (a path with diameter and depth), a fastener point (a
  property reference to the fastener's item).

Violating the contract is a readable load error naming the path and the rule. There is one scene
format and one loader; a part document is a second *use* of the first.

The hash is the version: a part document that changes is a new version, the item records the hash it
was linked against, and a changed file is reported to a person as "accept the new version", never
applied silently — a moved cup bore changes the holes the shop drills.

## 9. Instances in the scene

### 9.1. The node

A scene node may be an instance of an item (`document.md` §2.4): the placement, the pivot, the
instance's own properties and its user-added machining are document data; the contents — geometry,
anchors, drilling objects, nested fasteners — are derived from the part document and cached by the
item's version and the directory revision.

An instance is one object for selection and the gizmos. Its internals are not separately editable;
that is what `catalogue.detach` is for: it turns the instance into ordinary nodes, drops the link and
says so. One command, one undo entry, and the default is always the instance.

### 9.2. Dependencies

Part documents may instantiate other items — a hinge brings its screws. The dependency graph is
**acyclic**:

- a diamond is legal and normal: two hinges may use the same screw item;
- a cycle is refused with the chain spelled out (`hinge → screw → hinge`), never truncated at a depth;
- two guards: the directory application refuses to save a part document that closes a cycle over the
  connected packages, and the resolve path holds a depth limit of 8 and a budget of 500 resolved
  parts, so a corrupt package is an error and not a hang;
- a *property reference* is not a dependency and never enters the check.

### 9.3. Absent and archived

| State | What the scene does |
|---|---|
| item present | full resolution |
| item archived | full resolution; every card says out of production |
| item absent | placeholder drawn from the item's recorded bounds if the package is gone, or from the last resolved bounds; diagnostic on the node; the project opens |
| part file missing | same as absent, with the path in the diagnostic |
| part file hash differs | resolved from the recorded version if the file is still available, otherwise a notice to re-link |

A fallback is a derived placeholder, not stored geometry: the document stores the id and its own
placements, never a derived value (`AGENTS.md` §4.2).

## 10. Compatibility and selection

The workshop's rule is not "find me a plug", it is "find me the plug that belongs to this corpus".
Selection is therefore a resolution with a fixed step order, and every step is a closed predicate
over the registries:

1. **a person's explicit choice** — always wins, and a rule never overrides it silently;
2. **compatibility** with the anchor: host constraints, a size or thickness range, `Stock`/`Banded`
   agreement;
3. **the same family** as the anchor's family (`oak` corpus → an `oak` plug);
4. **the nearest rank** inside the family, from the family's reference sample outwards;
5. **the item's own default**;
6. **nothing found** — a diagnostic naming the anchor, the slot kind and the last step that failed
   (`for corpus "Дуб Сонома" no confirmat plug in family "oak"`), never a silent `None`.

A selection result is cached by `(anchor node, slot kind, revision)`. The rule that names the slot
(`plug.confirmat`, `plug.minifix`, `runner.colour`) lives in the directory beside the kinds; the
engine executes it. A person's choice is stored on the document node as an ordinary property, so a
later directory update cannot take it away.

## 11. Prices, stock and overlays

A price is an item property with provenance: `value` in whole minor units, `unit` from the kind's
`PriceUnit`, `currency`, `source` and `date`.

- the directory application edits one price at a time, one row, one history entry;
- an importer writes a batch and the batch is one history entry;
- a price with no currency, or a currency that is not a usable ISO 4217 code with a minor scale, is
  refused;
- a card and a report show the date and warn when a price is older than the workshop's limit;
- **stock on hand is not a property**: it is volatile, it belongs to an overlay package with its own
  source and date, and it names items by `ItemId`.

## 12. Packages

A manifest declares `id`, `name`, `version`, `requires` (package ids and the registry revisions it
needs: kinds, properties, families), the part documents it carries and the rule tables it carries.

- **connect / disconnect** — a package is enabled or disabled as a unit; disabling it makes its items
  absent (§9.3), which is visible, not silent;
- **export / import** — a package travels as a folder: the manifest, its records, its part documents,
  its rule tables; a workshop extends its directory by importing a package and exporting its own;
- **conflicts refuse**: an id claimed twice, a package whose `requires` cannot be satisfied, a part
  document breaking the contract, a rule table that is incomplete or overlapping — each is a refusal
  naming the package and the row; no merge, no precedence by load order;
- a package that fails validation leaves the previous snapshot in place and reports the sentence.

## 13. Revision and hot reload

- The directory carries a revision. The directory application bumps it on every committed write.
- The editor's shell watches it **off the frame path** (a polled file revision, never a per-frame
  stat) and validates a new snapshot when the revision moved.
- The new snapshot is applied **at a frame boundary, whole**: parse, validate, build the index
  (items by id, by kind, by slug, by family, by external key), then swap one value.
- What is recomputed afterwards is derived data only: a panel's material thickness and appearance, an
  instance's resolved contents and holes, selection results, diagnostics. Untouched nodes are not
  rebuilt (the revision keys the cache).
- **Undo is untouched**: the directory is not the document, and a directory update invalidates nothing
  a person did. A person's explicit choices live on the document and survive.
- A failed load keeps the old snapshot and shows the refusal; the editor never runs on half a
  directory.
- A folder that lists no package — a first run, or a workshop that disconnected everything — is the
  starter package, because the editor needs rows to offer and the starter is the one every workshop
  has. Which packages are connected is stated by the directory application, not by an empty editor.

## 14. Storage and the port

`DirectorySource` offers `snapshot()` and `revision()`. The port and its folder live in `crates/catalog`
and not in `model`, because `model` must not depend on the directory: a scene stores an `ItemId`, and
the record behind it is not document data. The first implementation is a folder:

```text
nomenclature/
  directory.json                  the connected packages and their order
  packages/<package>/manifest.json   the package's number, name and version
  packages/<package>/registry.json   the kinds, properties and families the package declares
  packages/<package>/items/*.json one record per file, stable order, diffable
  packages/<package>/parts/*.z3l  part documents
  packages/<package>/rules/*.json rule tables
  packages/<package>/overlays/*.json prices, stock on hand
  lock                            the writer's lock file
```

- a record names its kind and its properties by **slug**, not by code, so a file a person edits
  reads `"thickness": 18000`; the loader resolves a slug through the registry and refuses one no row
  declares;
- writes are atomic per file through the existing writer (temporary file, then rename), and a write
  that changes several files bumps the revision last;
- one writer at a time, guarded by the lock file; readers never lock and never write;
- the folder's `revision()` is the hash of every file a snapshot reads — its name, size and
  modification time — so it needs no file of its own and a folder edited by hand is watched like one
  written by the application. A file that is not there is part of it too: a folder that gained its
  first package moves the stamp, and a reader that cannot tell `NotFound` from a failed stat cannot
  watch a folder at all. A store that keeps a numbered revision behind the same port is free to; the
  editor compares the value and never interprets it;
- the folder copies as a folder — a workshop moves its directory by copying it — and an archive of one
  package is its folder;
- an embedded pure-Rust store may replace the implementation behind the same port when the schema
  settles or the directory outgrows a load-into-memory snapshot; the model is normalized so its
  tables map one to one. A store that binds C is refused by `AGENTS.md` §2.

## 15. Queries consumers use

- `item(id) -> Option<&Item>`;
- `items(kind, filter) -> iterator` and `search(text, kind?) -> [ItemRef]` with a stable order;
- `by_external(system, key) -> Option<&Item>`;
- `family(code)`, `family_members(family) -> [ItemRef]` ordered by rank;
- `board(id) -> Option<BoardView>`, `banding(id) -> Option<BandingView>`, `tool(id) -> Option<ToolView>`
  — narrow typed views that check the kind's traits, so a consumer never matches a kind code;
- `price(id) -> Option<Price>`, `on_hand(id) -> Option<Count>`;
- `select(slot, anchor) -> Selection` (§10);
- `resolve_instance(item) -> ResolvedPart` with the graph guards of §9.2.

`param`, `mesh` and `engine` reach the directory through these queries only; they never read a file,
never see a package and never hold a revision.

## 16. Validation checklist

Loading refuses, naming package and row, when: an id is zero or duplicated; a name or slug is empty;
a slug repeats within its kind; a kind is unknown; a required property is missing; a property is not
accepted by its kind; a value has the wrong type or leaves its range; a `Ref` resolves to nothing; a
family code is unknown or a rank repeats; an external system is unknown; a part document breaks its
contract or its hash link; the dependency graph has a cycle; a rule table is incomplete or
overlapping; a price has no usable currency; a package `requires` a registry revision it cannot
satisfy; the revision file is older than the newest file it names.

## 17. Tests

- an item of each starter kind loads and is found by id, kind, slug, family and external key;
- a duplicate id, a missing required property, an out-of-range length and a wrong value type are each
  refused with the package and the row named;
- an archived item resolves and a card says so; an absent item leaves the scene open with a
  diagnostic and a placeholder of the recorded size;
- a part document that instantiates another item resolves recursively; a diamond resolves twice; a
  cycle is refused with the chain; depth and part-budget limits are enforced;
- a part document that violates the contract is refused, and so is a part document that changes
  without a re-link;
- a changed directory revision re-derives a panel's thickness without a restart and leaves the undo
  history intact;
- `detach` turns an instance into ordinary nodes in one undo entry;
- selection walks the six steps: an explicit choice wins, family and rank pick the plug, and a miss
  names the failing step;
- a package export imports into a second directory unchanged, and a package that claims an existing id
  is refused;
- a price batch writes one history entry and a price without a usable currency is refused;
- the directory never appears in a serialised scene, and no scene field stores a derived value.

## 18. Open questions

1. Who allocates a package's `id` block: the directory application, a registry file, or a range the
   workshop reserves? Two workshops extending the same supplier package must not collide.
2. Localisation of names, labels and finish facets: one string per record now, a table of translations
   later, or never.
3. Textures: a file inside the package, a hash into a shared resource directory, or a path the
   workshop manages.
4. Grain direction, sheet nesting and stock accounting are properties of a board and rules of the
   cutting plan; their shape is not fixed here.
5. Multi-user editing: the same folder behind a lock with one writer, or a small server behind the same
   port.
6. Importers beyond CSV: Excel, 1C and a supplier's API each need an owner decision on where the
   adapter lives.
7. Whether the furniture-series catalogue is a package of this directory or a sibling directory that
   references it (the owner leans to a sibling that depends on the nomenclature).
