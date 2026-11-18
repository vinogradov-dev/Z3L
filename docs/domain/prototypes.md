# Prototypes

Status: **discussion draft — not accepted, nothing implemented, no task filed.** This file records
the design conversation of 2026-11-17 so the owner can approve or reject the mechanism as a whole.
Where it says *proposed*, only the discussion said it; where it says *measured*, the code or a run
showed it. The record of a decision comes after the owner answers §11, and then the relevant parts of
this file move into an ADR (the next free number) and the specification below is trimmed to what was
accepted. Rounds three to five record the owner's confirmations (§15–§17) and the interface word
«Узел»; §18 lists the task seeds and the conditions that block filing them.

Related decisions: [0028](../decisions/0028-the-assistant-is-a-client-of-the-command-registry.md),
[0031](../decisions/0031-external-resources-keep-their-own-system-of-record.md),
[0034](../decisions/0034-nodes-carry-typed-properties.md),
[0035](../decisions/0035-furniture-types-form-a-numbered-tree.md),
[0094](../decisions/0094-a-class-registry-gives-the-machine-its-semantics.md),
[0095](../decisions/0095-the-order-package-is-a-published-snapshot.md),
[0097](../decisions/0097-a-product-is-a-container-with-an-authored-envelope.md),
[0099](../decisions/0099-a-niche-is-the-space-a-frame-reserves.md),
[0103](../decisions/0103-one-probe-serves-the-niche-the-distances-and-the-move.md),
[0105](../decisions/0105-a-panel-stance-is-the-flat-frame-turned.md),
[0106](../decisions/0106-a-niche-is-a-convex-cell-of-wall-planes.md).
`PROJECT.md` › open model questions holds the earlier wording this file answers: *user scripts*,
*derived versus edited*, *dimensions*.

## 0. The question in one paragraph

A furniture maker must be able to add a prototype, choose a type, get a piece with default sizes,
open its card, change the numbers and see the piece rebuild — and author new prototypes without
programming. k3 does this with a Python script per prototype; the owner rejected the script. This
draft proposes what replaces it: **a prototype is data in three layers** — a construction, a
parameter card and a small rule vocabulary — expanded into commands by the engine.

## 1. What a prototype is

### 1.1 The reference behaviour

k3's shape is the behaviour to match, studied as behaviour only (clean room, `AGENTS.md` §4.6): *Add
prototype* → choose a type → a piece appears with default sizes → double-click → a card of parameters
→ edit → the piece rebuilds. The card is built from the type's key parameters, and the script holds
the combinations (door count, hardware, shelf spacing, drawers or a basket instead of a drawer).

### 1.2 The three layers

A script mixes three things that the data form separates, because each is authored and edited on its
own surface:

- **Construction** — which parts exist and how they relate (sides, bottom, top, back, shelves, rod,
  doors), and where they sit. Materialised geometry in the document.
- **Card** — the parameters a person edits: sizes, door count, hardware, shelf opening, shelf count,
  a choice between alternatives. Typed values, ranges, defaults, groups.
- **Logic** — derived values, alternatives, conditions, repetitions. The part a script would hold as
  `if` and `for`.

The document keeps the materialised result plus the pinned reference (`prototype.id`,
`prototype.version`, `prototype.parameters`, already reserved keys in
[0034](../decisions/0034-nodes-carry-typed-properties.md) and
[0035](../decisions/0035-furniture-types-form-a-numbered-tree.md)). It never stores the knowledge.

### 1.3 The machinery that already exists

- Classes and traits give every part its meaning and its behaviour
  ([0094](../decisions/0094-a-class-registry-gives-the-machine-its-semantics.md)): a shelf fills a
  void by width and depth, a post by height and depth.
- Typed properties, reserved prototype keys and a component marker
  ([0034](../decisions/0034-nodes-carry-typed-properties.md)).
- A product is a container with an authored envelope
  ([0097](../decisions/0097-a-product-is-a-container-with-an-authored-envelope.md)); parts are born
  inside the one open product ([0098](../decisions/0098-one-product-is-open-and-furniture-parts-are-born-inside-it.md)).
- A niche is a convex cell of wall planes, measured by probes
  ([0099](../decisions/0099-a-niche-is-the-space-a-frame-reserves.md),
  [0103](../decisions/0103-one-probe-serves-the-niche-the-distances-and-the-move.md),
  [0106](../decisions/0106-a-niche-is-a-convex-cell-of-wall-planes.md)), with one fill plan for
  every consumer ([0108](../decisions/0108-a-shaped-fill-bevels-its-ends-onto-the-walls.md)).
- Every change is a command, a batch of placements is one command and one edit is one undo entry
  ([0028](../decisions/0028-the-assistant-is-a-client-of-the-command-registry.md), ADR 0033).

What does not exist: the prototype format, its expander, the card, the authoring surfaces.

## 2. Dependencies are voids, not formulas

The owner's phrase was "the dependencies of parameters on other parts". In furniture that is almost
never a cross-part formula. A board that sits in a compartment takes the compartment's free space;
the program already answers that with a probe and a fill, so the prototype says **intent**, not
arithmetic: "a horizontal shelf in the left compartment" rather than
`shelf.width = divider.x - side.thickness`. The size is derived from the geometry that exists, which
survives a construction change.

Formulas stay for the few numbers no void answers: the case's own sizes, the door widths from the
reveal and the count, the count of shelves that fits an opening.

## 3. The rule vocabulary

Proposed: seven shapes, closed. Each is one card in the interface; none is free text; none can do
I/O. A new *shape* is a code change with a test; a new prototype is data.

| Shape | What it is | What it replaces in a script | The wardrobe example |
|---|---|---|---|
| Parameter | a typed named value (length, integer, bool, choice) with a range and a default | an argument of the script | width 800, height 2200, depth 560, doors 2, opening 320 |
| Derived value | arithmetic over parameters and standards: `+ − × ÷`, min/max, floor/ceil, names only | an assignment line | door width `(W − 3 × gap) / 2` |
| Void fill | a part of a class fills a named compartment; the model measures it | `Width = inner - 2*t` | fixed shelf, shelf row, divider |
| Repeat row | how many fit an opening, and where the remainder goes | `for i in range(n)` | `1806 = 4 × 320 + 4 × 18 + 454`, remainder at the bottom |
| Slot with alternatives | a position whose variant is another prototype with its own card | `if drawers: … else: basket …` | drawer / basket |
| Condition | if a parameter holds, add, remove or set | `if` | three doors add a middle post |
| Decision table | rows are cases, columns are ranges, cells are values | a lookup function | hardware by door count and height |
| Check | what must hold, reported as a diagnostic | an `assert` | hanger clearance ≥ 450; every part inside the envelope |

## 4. Authoring without code

1. **By example (proposed as the primary entrance).** The maker builds one piece in the scene with
   the ordinary tools (commands), then *Save as prototype*. The program proposes which numbers became
   parameters, which parts are optional, and which dependencies it derived from the geometry; the
   maker confirms and edits. Nothing is invented: every proposal is a candidate the maker accepts.
2. **Card.** Built from the parameter list, never hand-drawn: type, range, default, group. The card
   is a projection of the data, so it cannot disagree with the logic.
3. **Slots and nested prototypes.** "A drawer or a basket" is a slot whose alternatives are
   prototypes (`drawer`, `basket`) with their own cards. A wardrobe need not know how a drawer is
   built; complexity is composed, not scripted.
4. **Rules.** The vocabulary of §3, as cards: a table, a dropdown, two fields "if / then", a number
   with a range.

**The block diagram is the model, not the surface.** Parameters, derived values, slots and rules form
a directed graph, and a graph view is worth having for the advanced author. But it is not the primary
surface: a graph cannot express repetition, enumeration and choice — which is most of furniture
logic — it shows no geometry, and a mistyped link is found by eye instead of by a sentence. Visual
programming is programming with pictures; the author here is a maker who thinks in "doors: 2",
"shelves: 4", "opening: 320".

**Formulas are not a language.** A derived value is an expression tree over named values with five
arithmetic operators, min/max/floor/ceil and references to standard values. No loops, no I/O, no
recursion. The interface offers operands and operators; the text field, if any, autocompletes the
known names.

## 5. Prior art

Studied as behaviour, never copied. Product names are a quality bar, not a specification.

- **Product configurators**: Tacton, Configit. An engineer authors a *constraint model* — variables,
  domains, rules, tables, bill of materials — with no code, and a configurator drives the sale. The
  classic survey is Sabin & Weigel, *Product Configuration Frameworks* (1998): rule-based,
  constraint-based and resource-based techniques. This is the closest thing to "a prototype without
  programming", mature for decades, but authored by a configurator engineer.
- **Parametric CAD**: SolidWorks (equations and Design Tables: a table of variants), Fusion 360
  (user parameters and a feature timeline), FreeCAD (Spreadsheet, expressions, VarSet), Onshape
  (FeatureScript — a language, the opposite pole). What to take: parameter, derived value, variant.
- **Furniture packages**: k3 (Python), imos and 2020 Design (catalogues with parameters and
  formulas), Cabinet Vision (user-created standards: rules, but written in a language), Polyboard
  (parametric methods with options), Базис-Мебельщик, bCAD, Woody, KD. In all of them the serious
  library is authored by a technologist who is half a programmer.
- **Visual programming**: Grasshopper, Dynamo, Blender Geometry Nodes, Unreal Blueprints. Powerful
  for geometry generation, poor for a maker: no cardinality, no variants, hard to debug. In a
  Rust-only product a graph would be data anyway, expanded by Rust — never a runtime.
- **Research**: programming by demonstration (learning a construction from one shown example),
  knowledge-based engineering, constraint-based and feature-based parametric design.

Verdict: every piece exists separately. The combination the owner wants — *build one piece, let the
program derive the dependencies, edit rules and tables, get a card* — is close to absent from the
market, and it is the differentiator this file proposes to specify.

## 6. Where the knowledge lives

Nothing here is an embedded database or a runtime: [0095](../decisions/0095-the-order-package-is-a-published-snapshot.md)
forbids a scripting runtime and an embedded database (`SQLite` would drag C sources in), and
`AGENTS.md` §2 forbids the language. The knowledge is files with a version, read by Rust types, plus
the shop's own system behind its API ([0031](../decisions/0031-external-resources-keep-their-own-system-of-record.md)).

| Artefact | Owner | Form | Today | At a factory | Changes |
|---|---|---|---|---|---|
| Class registry | the product | Rust table, compiled in | `crates/model/src/class.rs` | same; a private group would need it as a loaded resource (`furntypes.md` §9, open) | with a release |
| Workshop standards | the workshop | versioned JSON, validated on load, domain units | proposed beside the workspace settings (`app/src/settings_store.rs` uses the OS config directory) | the shop's resource system behind an API, read-through cache | often, no build |
| Prototype library | the workshop | versioned JSON per prototype, closed step vocabulary | — | a folder in the workspace, or the resource system behind an API | constantly, no build |
| The document `.z3l` | the job | materialised geometry + `prototype.id/version/parameters` | — | — | never holds the knowledge |

The workshop standards carry the technology defaults: board thicknesses and stock, sheet size and
kerf, door reveal and overlay, the ladder of standard openings, rod diameter and drop, the back
panel and how it is fixed, banding per class, the hardware set, tolerances, mounting. A command reads
its default from the standards, so the factory's rule is one place and not a prompt.

Every prototype and standard carries a version, and the document pins them, so a job issued in 2026
rebuilds identically after the library changes. The order package pins the same versions
([0095](../decisions/0095-the-order-package-is-a-published-snapshot.md),
ADR 0032 for the numbered folder).

## 7. How standards grow

1. The assistant chooses the closest prototype and fills the card from the job.
2. Whatever the prototype did not cover — a number invented, a convention guessed — goes into an
   **assumptions report**: the value, why, and which standard was missing.
3. The owner or the technologist approves: the assumption becomes a **standard value** or a **new
   prototype version**. This is a data change with a review and a golden scene, not a code change.
4. The next job uses the new version; earlier jobs keep their pinned one.
5. The assistant may *propose* a standard or prototype change as a diff; it never changes an approved
   one silently. Approval is a person's, as with the plan and the assistant's audit trail (ADR 0028).

The budget is honest: a new cabinet or a new opening ladder is data (JSON plus a golden test, no
release); a new *kind* of construction — a bent tamburat panel, a profiled door — is a new step in
the vocabulary, so code and a release with a test.

## 8. The assistant's path

Proposed, and partly measured as missing:

- **Commands.** The assistant fills a typed card and dispatches commands. It never writes
  coordinates, never serialises a scene. Prototypes are chosen, not authored; a proposed prototype
  version is a diff for the owner.
- **A headless channel.** Measured today: `Session::dispatch` and `dispatch_in` exist, each
  command parses one JSON object into a typed `serde` struct with `deny_unknown_fields` (as
  `PanelCreate` does), and `engine` has no `wgpu`/`egui`; but nothing outside the process can call
  them — `app/src/cli.rs` offers only
  `export`. Proposed: `z3l run <scene.z3l> --commands <recipe.jsonl> [--out] [--dry-run]`, one
  session, one undo group per task, outcomes printed; `z3l commands --json` for discovery with a
  schema per command; `z3l query <scene.z3l> …` for the numeric read model (tree, box, niche,
  diagnostics, parts). Verification is numeric, not visual.
- **Enforcement.** A gate that keeps agent-facing code out of `Project::add_*` and
  `SceneFile::write`, so the command layer is the only door.

## 9. The worked example

The conversation's test case, verified by evaluation, is a hinged wardrobe, 800 wide × 560 deep ×
2200 high, an 18 mm white board, standing on the world floor with its base minimum corner at the
origin, its front towards +Y. It produced 14 parts and two conventions the owner confirmed.

| Part | L × W × T (mm) | X | Y | Z |
|---|---|---|---|---|
| Left side | 2200 × 542 × 18 | 0–18 | 0–542 | 0–2200 |
| Right side | 2200 × 542 × 18 | 782–800 | 0–542 | 0–2200 |
| Bottom | 764 × 542 × 18 | 18–782 | 0–542 | 0–18 |
| Top | 764 × 542 × 18 | 18–782 | 0–542 | 2182–2200 |
| Back (inset) | 764 × 2164 × 18 | 18–782 | 0–18 | 18–2182 |
| Fixed shelf | 764 × 524 × 18 | 18–782 | 18–542 | 1824–1842 |
| Divider | 1806 × 524 × 18 | 391–409 | 18–542 | 18–1824 |
| Shelf 1–4 | 373 × 524 × 18 | 409–782 | 18–542 | 472, 810, 1148, 1486 (bottom) |
| Hanging rod, Ø25 × 373 | — | 18–391 | axis 280 | axis 1724 |
| Door left | 397 × 2196 × 18 | 401–798 | 542–560 | 2–2198 |
| Door right | 397 × 2196 × 18 | 2–399 | 542–560 | 2–2198 |

The arithmetic closes, which is why the reading was trusted: width
`18 + 373 + 18 + 373 + 18 = 800`; depth `18 (back) + 524 (inside) + 18 (doors) = 560`; height
`18 (bottom) + 1806 (below the fixed shelf) + 18 + 340 + 18 = 2200`; and the left column
`1806 = 4 × 320 + 4 × 18 + 454`, the 454 remainder in the bottom cell. Edge banding is 2 mm white
tape on the front edges of the carcass and the shelves and on all four edges of the doors; the back
is bare. The rod is a round body because the document has no hardware entity yet (M8), so no hinges,
handles, flanges or shelf pins are modelled.

The scene was built by a generator that used the model API and wrote the file, then read it back; it
is not committed. **That path is the defect §8 closes**: it bypassed `enabled`, the creation guard
([0098](../decisions/0098-one-product-is-open-and-furniture-parts-are-born-inside-it.md)) and
history, although `Session::dispatch` was available in the same process.

## 10. Words that must become domain vocabulary

A convention that lives only in a head is guessed wrong. These were used by the example and must be
stated in one place (an ADR or `docs/domain/`), with a test where the code can hold them:

| The words | The reading that was used | Why |
|---|---|---|
| "a shelf at 340" | a **clear opening** of 340 from the underside above to the shelf's top face | a maker quotes what fits; a "step" or "on centres" would be named as such; 340 as a step would need a reference face nobody named |
| "shelves 320 apart" | a **clear opening** of 320, the count derived, one cell absorbs the remainder | 5 × 320 + 5 × 18 does not fit 1806, while 4 × 320 + 4 × 18 + 454 does; the 454 remainder is an ordinary cell, 116 is not |
| "depth 560" | the **overall** depth including the 18 mm doors; the carcass is 542 | a piece's depth is what it occupies in the room |
| "on the left" | the **+X** side: the front view looks from +Y, so +X is on the viewer's left, and `OpeningView::Left` is `[1, 0, 0]` (measured in `crates/ui/src/display.rs`) | the code already answers; the first build named the near-zero side "left" and had to be mirrored |
| "left/right door" | named by the same rule | a name that contradicts the view is a defect, not a preference |

## 11. Open decisions for the owner

1. **The authoring surface** — by example plus rules and tables (proposed), with the graph as an
   optional view; or the graph as the primary editor.
2. **Hand edits inside an instance** — a part may be detached from its prototype (proposed), refused
   at rebuild, or overwritten with a report. Silent loss is not an option (`AGENTS.md` §4.4).
3. **Part identity** — a prototype's parts carry stable role names (`side.left`, `shelf.row`,
   `door.1`) and the rebuild matches by role; the matching rule and its failure behaviour are the
   hardest piece and must be decided before the card.
4. **Where the standards and the library live** — the open question in `plan/questions.md`, and
   whether the class registry becomes a loaded resource (`furntypes.md` §9).
5. **The word "script"** — ADR 0034 and `PROJECT.md` say product templates are user scripts, while
   `AGENTS.md` §2 forbids a scripting runtime. The proposal is to amend the wording to "prototype",
   meaning data in this file's shape. A real language would be a constitutional change with its own
   ADR.
6. **Absolute placements in a prototype** — proposed: forbidden; a prototype addresses anchors and
   voids, never coordinates, so an instance can be moved and the prototype cannot contradict the
   product frame.
7. **The formula surface** — which functions beyond `+ − × ÷`, min/max, floor/ceil, and whether any
   text entry is offered at all.
8. **What the assistant may propose** — a standard value, a prototype version, both as a diff (proposed),
   and who approves.
9. **The word for the stored space** — «секция» / compartment while «ниша» keeps meaning the
   measured void, or a rename of the void tool so one word carries the stored thing (§14.1).
   **Confirmed: «секция» (§15).**
10. **What a compartment stores** — a container with the identity placement and product-frame
   children (proposed), or a frame written by the rebuild command so the children are
   compartment-local (§14.3). **Confirmed: the section has a frame and its children are relative to
   it (§15).**
11. **The split rule** — the threshold's home (a standard, a prototype parameter, or both), the
   naming of the children, and what happens to hand-placed parts when a branch disappears (§14.5).
   **Partly resolved: an instance holds no hand-placed parts (§15); the threshold and the naming of
   children remain.**
12. **The case variants as data or code** — the slot and plane vocabulary is code; whether the
   variants themselves ship as a `case` prototype in the library or stay built in (§14.6).
   **Confirmed: built-in and user prototypes share one data format (§15).**
13. **The load-bearing bottom and the plinth** — whether they are one slot or two, since the second
   decides which plane the sides span (§14.6). **Confirmed: two slots, as the k3 card had them
   (§15).**

## 12. A first implementation, in order (not tasks)

1. The glossary of §10 and the workshop standards as data, with the two conventions as tests.
2. `z3l run` / `commands --json` / `query` over the existing dispatch, so every later step is
   checked numerically and the assistant has a door.
3. The prototype format in `model` (like `catalog.rs`): parameters, parts, rules, checks, a version,
   `deny_unknown_fields`, readable errors.
4. The expander in `engine`: prototype + parameters → commands, one undo entry; the rebuild matches
   parts by role and reports what it would replace.
5. The card, generated from the parameter list.
6. Authoring by example: build one piece, save as prototype, review the proposed parameters and
   dependencies.
7. The rules and tables interface; the graph view last, if at all.
8. The library, the version pinning and the assistant's assumptions report.

§18 turns this order into task seeds with their `Done:` clauses, their `files:` hints and their
blockers.

## 13. Process notes

- **Read the program's own convention before naming.** `OpeningView::Left` was one search away and
  the first build ignored it; the sides were mirrored because the authoring order (from the
  document's minimum corner) was mistaken for the viewer's frame.
- **A scene built outside the command layer is not the product's path.** The generator was useful for
  the numbers, but it skipped the checks and history that make the command layer trustworthy; the
  command channel of §8 is what the next scene must use.

## 14. Compartments and the case skeleton (second round)

The owner's model, recorded as proposed: a niche is a first-class thing that appears in the tree as a
parent, parts are built inside it, and a niche may be split by a condition (`if the width exceeds W,
divide it in two`), each half receiving its own fit-out — shelves on one side, a rod on the other.

### 14.1 The conflict, and the two words

An accepted decision says the niche is **derived, never stored** — no id, no node, no command, no
field ([0099](../decisions/0099-a-niche-is-the-space-a-frame-reserves.md),
[0106](../decisions/0106-a-niche-is-a-convex-cell-of-wall-planes.md), `docs/domain/products.md` §4) —
and the interface already uses the word for the probe: the `V` tool measures the empty space and the
status bar names «Ниша» (`crates/ui/src/void.rs`, `crates/text/src/messages.rs`). A stored node with
the same name would fight both.

Proposed resolution: **two things, two words.**

- **Niche** — unchanged: the measured void, a query (`Niche`, `fill_plan_in`).
- **Compartment** (секция) — stored: a `Container` with a role property, holding the parts and the
  nested compartments; it *asks* the niche for its own bounds, so no size is stored.

The compartment stores the author's intent (this space exists, this is what fills it, this is how it
divides); the niche stays the measured space, so ADR 0099's rule holds: nothing stores a box that
can go stale.

### 14.2 A compartment is not a class

The class registry is compiled in and identifies a physical object with a supply, a measure and a
nesting rule (`Level` = Product, Component, Part, Fitting). A compartment has none of those; it is
space plus a fit-out. The fit-out type — empty, shelves, drawers, rod — must change without a
release, so it lives in the prototype library as data and is referenced from the compartment. If the
registry ever becomes a loaded resource (`furntypes.md` §9), a compartment row can be added then; the
mechanism does not need it.

### 14.3 The tree

```text
Wardrobe (product, envelope 800 × 560 × 2200)
├─ Left side, Right side, Bottom, Top, Back          the case
├─ Door left, Door right                             assembly units, case front layout
├─ Compartment "Top"     fit-out: empty               the 340 cavity
└─ Compartment "Lower"   split: width > max span → two
   ├─ Post                                             owned by the split, not by a child
   ├─ Compartment "Left"   fit-out: shelves ×4, opening 320, remainder bottom
   └─ Compartment "Right"  fit-out: rod, Ø25, drop 100
```

A compartment's placement is the identity and its children keep product-frame placements; its bounds
are a query. The alternative — a derived compartment frame written by the rebuild command, as
`node.group` writes a group's centre — gives nesting coordinates at the cost of rewriting placements
on every card edit (§11, item 10). **The owner chose the nested frame: see §15.**

### 14.4 Fit-out types and what each one constrains

| Fit-out | Produces | Parameters | Checks and needs |
|---|---|---|---|
| Empty | nothing; holds hand-placed parts | — | a minimum size |
| Shelves | n boards and pins | opening, remainder policy | max span → a split; a minimum opening |
| Drawers | boxes, fronts, runners | count, front heights, depth | runner length and clearance; a front width rule |
| Rod | one round body and its supports | diameter, drop, support | interior depth ≥ hanger width; clearance below |
| Basket / pull-out | a front, a body, runners | footprint, front height | width and depth from the hardware |
| Appliance | a bay with no front | envelope, ventilation | the appliance's own box; a socket |
| Shoes / trays | a pitch rule | pitch, count | depth against the shoe's length |
| Fronts | doors, flaps, sliders | type, count, overlay, gap | max door width → count; room for a hinge |
| Corner | an L-shaped space | leg widths | two depths, one post |

The closure trait already exists: a front's class carries `Opening::Closes`, so "this compartment is
closed by two doors" is a query, not a special case.

A row of drawers and a door are **assembly units**, not fit-outs of their own: the fit-out decides how
many units and where, and each unit is its own prototype instance with its own card (§17). The
`Fronts` row above is the case's front layout, which instantiates door units.

### 14.5 The split

A split is one rule: a threshold (from the standards — for example the maximum unsupported shelf
span), the divider part, and children named by role (`left`, `right`, or `1..n`). It is the only
place a prototype branches, it is bounded (a depth cap and a count cap), and it is reversible:
reducing the width merges the children and the rebuild reports what it removes. The divider belongs
to the parent, so the children's voids are measured without it.

### 14.6 The outer case: named planes, and variants that move them

The owner's question is how to parameterise the case's own construction — plinth, back, roof,
supports, whether the bottom is load-bearing. Booleans per feature multiply, and every combination
then needs a special case. Proposed: **a skeleton of named planes plus one variant per slot, where a
variant declares which planes it moves.**

Planes: `case.left`, `case.right`, `case.front`, `case.back`, `back.face`, `inner.bottom`,
`inner.top`, `plinth.top`, `roof.top`, `floor`. Dimensions are the distances; a part attaches to two
planes — a side spans `base.plane..roof.plane`, the bottom sits between `inner.bottom` and its own
top face. One side part, not one per feature.

| Slot | Alternatives | What each alternative moves or adds |
|---|---|---|
| Base | none · plinth (front only / full) · legs · wall rail | moves `plinth.top`; decides whether the sides reach the floor; adds 1–3 plinth boards or legs; shortens the fronts when they do not cover the plinth |
| Load-bearing bottom | yes · no | yes: the sides stand on the bottom (`base.plane = bottom.top`); no: the sides reach the floor and the bottom sits between them |
| Roof | inset · over the sides · overhang · none | moves `roof.top`; changes the side length and the width of the roof |
| Back | none · inset 4 · applied 4 · inset 18 | moves `back.face` (the interior depth); adds a part or a groove |
| Fronts | overlay · inset; hinged · sliding · flap · none | moves `front.face`; sets the gaps; the count rule per compartment |
| Connection | confirmats · dowels · cam and dowel | machining and hardware only, no size |
| Supports | adjustable feet · glides · legs | hardware and positions; `base` decides |

The rule that keeps this finite: a variant may move the named planes and add or remove its own
parts; it may not edit another variant's parts. The interior then reads the planes and needs no
special case, and a check rejects an impossible combination — an applied back plus a deep plinth in a
560 carcass can leave less than a hanger's depth.

The variants are themselves data (a `case` prototype), so a workshop adds its own plinth without a
release; the plane and slot vocabulary is code and arrives with a test. §12's format step therefore
includes compartments, the plane skeleton and the variant slots.

## 15. Decisions confirmed by the owner (2026-11-17, third round)

The owner answered §11 and corrected the model. The five answers below are decisions; the mechanism
as a whole is still a draft.

1. **The word is «секция».** The niche keeps meaning the measured void (§14.1).
2. **A section has a frame, and its children are relative to it.** A child placement is expressed in
   its section's frame, so the world transform of a part is
   `product ∘ section ∘ child`. The frame is *not* measured from the section's own content: the
   parent writes it from the named planes and the split — x from the split's ranges, y between the
   back's inner face and the front plane, z between the section's floor and ceiling planes. It is
   written by the rebuild command, so it is authored data that no query can make stale, and a card
   change rewrites the frames and the children's placements in one command and one undo entry. A
   section depends on its siblings only through the parent that divides them: a fit-out that needs a
   minimum width is a check on the split, not a reference to a neighbour's coordinates.
3. **A prototype instance is closed.** No part inside it is hand-placed; every part is generated from
   the card. To edit by hand the maker runs **dissolve** — one command, one undo entry — which keeps
   the materialised sections, frames and parts, drops the prototype reference and the card, and
   leaves an ordinary product that can be edited and can be captured as a new prototype by example
   (§4). An *empty* section is reserved space, not a place for hand parts. The owner later gave the
   command a depth (§16).
4. **Built-in and user prototypes share one format.** The built-ins ship with the build as data (the
   same JSON, each with a version); a user prototype lives in the library (§6). A built-in is
   read-only: *Duplicate to my library* copies it and the copy is edited. A job pins the version it
   used, built-in or user.
5. **The case vocabulary is the one the workshop already uses** — k3's card parameters, adopted:
   - bottom: `опорное` (load-bearing: the sides stand on it) · `вкладное` (between the sides);
   - roof: `накладная` (over the sides) · `вкладная` (between them);
   - back: `накладная` (applied to the rear, 4 mm HDF) · `врезная` (in a groove, 4 mm HDF) ·
     `вкладная` (between the sides, 18 mm chipboard).

   The bottom type decides where the sides begin and end, which is why the further logic follows from
   it, exactly as in k3. The owner corrected the material rule below: the back is a type **and** a
   material, two card parameters (§16). The default catalogue now carries the 4 mm HDF sheet beside the
   two 18 mm carcass boards (`catalog::default_directory`), so an applied or grooved back resolves its
   planes from a row and not from a number in code; the *type* itself waits for the plane skeleton.

Consequences settled later: dissolve keeps the section containers and takes a depth (§16); the card
is one form with a selector that jumps to a section (§16). Still open: the split's threshold and
child naming, and whether a fit-out's minimum width may steer the split.

## 16. Decisions confirmed by the owner (2026-11-17, fourth round)

1. **Dissolve takes a depth, with a full dissolve at its end.** Proposed reading: the depth counts
   *instance levels from the top*, so the released nodes form a prefix of the tree. `1` releases the
   case (its parts and the section containers become ordinary nodes with their frames as authored
   data), while a deeper fit-out keeps its card and still rebuilds its own content; `2` also releases
   the fit-outs of the sections; **full** releases every instance down to plain panels and objects.
   Nothing is re-created: ids stay, the revision bumps once, one undo entry.
   A released level is *frozen*: no rebuild writes it again, and the parent's checks report a
   mismatch instead of moving it (a released section that no longer fits its case is a diagnostic,
   not an edit). The alternative — releasing one branch inside a live parent — needs a pin marker on
   the released child, because ownership would no longer follow the prototype reference; not chosen
   yet. **Confirm the prefix reading.**
2. **The section frame is not measured from the section's own content** — confirmed: the parent
   writes it from the named planes and the split (§15, item 2).
3. **One card for the product, with a selector.** The form is one document grouped by the prototype's
   parameter tree (case, sections, fronts); picking a section in the viewport or the tree jumps the
   form to that section's group and highlights it, so a section's parameters are two clicks away
   without a second window. A released level has no group; a still-parametric nested section keeps
   its own group.
4. **The back is a type *and* a material.** Two card parameters, not one variant carrying both:
   - the **type** decides the construction (applied / in a groove / inset), which named planes it
     moves, and the thickness range it accepts (a thin sheet for applied and grooved, a board for
     inset);
   - the **material** is the catalogue entry, and its resolved thickness feeds the planes — never a
     number written into the variant;
   - a material outside the type's range is a refusal with a sentence, not a silent fallback;
   - the card opens with the workshop's default material for that type (a standards entry), and the
     maker may change it within the range.

   The same two-parameter shape applies to every part whose material is a choice: the construction
   type owns the geometry rule, the material owns the stock. The default catalogue must therefore
   carry a 4 mm HDF sheet before an applied or grooved back resolves; the 560-deep example becomes
   18 mm door + 538 mm carcass + 4 mm back under the applied type, and 18 + 524 + 18 under the inset
   one, with no formula changed.

The format step of §12 now covers: sections with frames, the plane skeleton, the variant slots, the
dissolve depth, the one-form card and the type/material pair.

## 17. Dissolve targets and assembly units (fifth round)

The owner's correction: a drawer or a door is an **assembly unit**, not a section, and dissolving a
wardrobe must be able to stop at those units — a drawer must not fall into panels and runners unless
the maker asks for it. A numeric depth is the wrong parameter: the number changes meaning as the
library grows. The proposed parameter is a **target plus a scope**.

### 17.1 Three prototype kinds

- **Case** (корпус) — builds the case, its named planes, the sections and the front layout.
- **Fit-out** (наполнение) — fills one section: shelves, a rod, a row of drawers, a basket.
- **Assembly unit** (сборочная единица, «узел») — a reusable sub-assembly: a drawer, a door, a
  basket, a set of legs. The class registry already has the word: a container marked as a component
  is an assembly (`Level::Component`, ADR 0034), so the instance is an assembly and the library entry
  is an assembly prototype. **The owner chose «Узел» for the interface** (2026-11-17); «сборочная
  единица» stays the full term in documents and `assembly` the code word.

A drawer row is a fit-out that instantiates n drawer units; a door is a unit instantiated by the
case's front layout (or by a section, when the fronts are per section). `prototype.kind` is stored on
the instance, so a command works while the library is missing — the failure policy the discussion
already fixed: a missing prototype is a warning, never a load failure.

### 17.2 The dissolve parameter

`prototype.dissolve { target, scope }`:

| Target | Releases | Keeps parametric |
|---|---|---|
| `sections` («до секций») | case prototypes | sections and assembly units |
| `assemblies` («до узлов») | case and fit-out prototypes | assembly units |
| `nothing` («полностью») | everything | nothing — plain panels and hardware |

- A released instance stays a container with its frame and children; it loses the prototype reference
  and keeps its structural role (`compartment`), so the tree still reads "Section «Left»" with
  ordinary parts.
- A released **section whose children are all sections** is collapsed into its parent (the owner's
  rule), because such a node only carried a split: `node.ungroup`-like, frames composed so no world
  point moves.
- One command, one revision, one undo entry; ids are reused, nothing is re-created.
- A released level is frozen: the parent's rebuild does not write it, and a mismatch (a released
  section that no longer fits) is a diagnostic, not a silent edit.

### 17.3 Scope, and the fourth option

- `scope = whole product` — the three targets above.
- `scope = this node` — release one instance and its subtree, leaving the parent parametric. This
  needs one thing the other options do not: a **`manual` marker** on the released node, because
  ownership no longer follows the prototype reference. The parent's rebuild skips a `manual` child and
  reports when it stops fitting. That is the flexible answer to "this drawer is custom, leave it
  alone", at the cost of one reserved property.

Considered and refused: a numeric depth (fragile as the library grows), "dissolve one part of a live
instance" (meaningless once every part is generated), and "dissolve and re-create the nodes" (loses
ids and history).

### 17.4 The interface

A dialog rather than three menu items, because the three targets are two facts: two checkboxes —
*keep sections parametric*, *keep assembly units parametric* — cover exactly the three targets
(`sections` = both checked, `assemblies` = the second only, `nothing` = neither), and the maker sees
what will stay alive before pressing the button. A context entry *Dissolve this unit* is the
`scope = this node` form.

### 17.5 What this changes elsewhere

- §14.3's tree: doors and drawers are assembly units, not compartments; the case owns the front
  layout, a section owns what fills it.
- The tree badge and the card group come from `prototype.kind`, so a unit reads as a unit without the
  library.
- "Dissolve, edit, capture as a prototype" is the loop of §4; dissolving to assemblies is its common
  case.

## 18. Task seeds and what blocks filing (handoff)

Written so a later session can file the work without reading the conversation again. The mechanism is
**not yet accepted**: §11 still holds four open decisions — where the workshop standards live, the
formula surface, the split's threshold and the naming of its children, and whether a fit-out's
minimum width may steer the split. No task has been filed: the plan is a single-writer surface
(`AGENTS.md` §1.4) and another session held it (`state=doing`), and the gate requires an ADR to cite,
a declared id prefix, `files:` hints that exist and no duplicate id.

### 18.1 What readiness means

A task can be filed when all five hold:

1. the decision it implements is recorded — an ADR exists before a task cites it;
2. its id prefix is declared in the plan's phase legend (`cargo xtask task phase` owns the list);
3. the open questions that change its shape are answered, not guessed;
4. the plan is free: `state=idle`, no `[~]` task, no other session's `SESSION.md`;
5. the task fits one session, its `Done:` is checkable without a window, and every `files:` path
   exists (a directory when the task creates a file).

### 18.2 Ready, blocked, or waiting for code

- **Ready now** (no open decision): the convention glossary and the `Side` type; the headless command
  channel (`commands --json`, `run --commands`, `query`); the reserved keys `compartment`, `prototype.kind` and `manual`.
- **Blocked by a decision**: the workshop standards file (its home is in `plan/questions.md`), the
  prototype format (the formula surface), and the split (threshold, child naming, minimum width).
- **Waiting for code**: drawer and door units wait for M8 — runners, hinges and their machining. A
  panels-only prototype (case, shelves, rod) can ship before it.
- **Waiting for the record**: a task that cites the prototype ADR waits for the ADR, and the ADR
  waits for §11.

### 18.3 The seeds

| # | Task | `Done:` | `files:` (existing paths) | Waits for |
|---|---|---|---|---|
| S1 | ADR «a prototype is data, not a script», with the section, the plane skeleton, the three kinds and the dissolve targets | the ADR is accepted and listed, §15–§17 are folded into the spec, the open decisions are in `plan/questions.md` | `docs/domain/prototypes.md`, `docs/decisions/README.md` | §11 |
| S2 | The convention glossary and `Side` | a test ties `Side::Left` to `OpeningView::Left.direction()`, and the glossary says 340/320 = a clear opening and depth = with the fronts | `crates/ui/src/display.rs`, `crates/model/src/`, `docs/domain/products.md` | nothing |
| S3 | The headless command channel | a scene is built from a JSONL recipe in one undo group, and `--dry-run` writes nothing | `app/src/cli.rs`, `app/src/main.rs`, `crates/engine/src/` | nothing |
| S4 | A 4 mm HDF row in the default catalogue | an applied or grooved back resolves to a material instead of a fallback | `app/src/scene_build.rs`, `crates/catalog/src/starter.rs` | landed as O1 (the row); the back *type* waits for §15 |
| S5 | The workshop standards file | it loads with validation, a command takes its default from it, and staleness is a notice | `app/src/settings_store.rs`, `crates/model/src/` | where it lives |
| S6 | Reserved keys `compartment`, `prototype.kind`, `manual` | the gate rejects a wrong type and a scene opens without the prototype library | `crates/model/src/properties.rs`, `crates/model/src/project/query.rs` | nothing |
| S7 | The prototype format (kinds, parameters, planes, slots, eight rule shapes, checks, bounded formula tree) | an unknown rule kind or version is a readable error and every parameter validates | `crates/model/src/`, `docs/domain/prototypes.md` | the formula surface |
| S8 | The expander into commands | a prototype and its parameters become commands in one undo group, deterministically | `crates/engine/src/`, `crates/model/src/project/` | S7 |
| S9 | The rebuild by role | a card change rewrites the frames and children in one command and reports the diff | `crates/engine/src/`, `crates/model/src/project/node_cmds.rs` | S8 |
| S10 | Dissolve `{ target, scope }` | the three targets, the pass-through collapse and the `manual` marker work; ids stay and one undo covers it | `crates/engine/src/`, `crates/model/src/project/node_cmds.rs` | S9 |
| S11 | The library: built-in and user, versions, duplicate | a built-in is read-only, its copy is edited, and the job pins the version | `crates/project/src/`, `app/src/` | S7 |
| S12 | One card with a selector | picking a section opens its group, and a released level has none | `crates/ui/src/`, `app/src/document.rs` | S9 |
| S13 | Authoring by example | a built piece is saved as a prototype and the proposed parameters are reviewed | `crates/ui/src/`, `crates/engine/src/` | S7 |
| S14 | Rules and tables in the interface | all eight shapes are editable and a refusal is a sentence | `crates/ui/src/` | S7 |
| S15 | The first golden: the 800 × 560 × 2200 wardrobe | the fourteen parts of §9 are checked by a test | `crates/model/src/`, `crates/testkit/src/` | S8 |
| S16 | The assistant's assumptions report | an uncovered decision appears as a numbered assumption and a proposed standard | `crates/engine/src/`, `app/src/` | S3, S9, S12 |
| S17 | The gate: agent-facing code cannot mutate the document | a command layer is the only door; the gate fails on a direct `Project::add_*` or `SceneFile::write` | `xtask/src/`, `crates/engine/src/` | S3 |

### 18.4 The first hour of the next session

1. `cargo xtask status`; wait for `state=idle` or take the task the state names.
2. Read this file, §0–§18.
3. Write the ADR (S1) and fold §15–§17 into the specification; move the four open decisions into
   `plan/questions.md` through `cargo xtask file`.
4. Decide the phase for these ids: the legend has no letter for prototypes. Either a new letter
   (`cargo xtask task phase`, e.g. `Q` — prototypes and recipes) or a new milestone M11 in
   `PROJECT.md` with its tasks under `M`. This is the owner's decision, and the tool refuses an
   undeclared prefix.
5. File S2–S6 first: they depend on no open decision and keep the work moving while §11 is answered.
   S7 and everything above it waits for the formula surface and the standards' home.
