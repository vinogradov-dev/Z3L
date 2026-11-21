# 0215. The program teaches its domain, and a construction is composed

- Status: Accepted; supersedes the recipe of 0192
- Date: 2026-11-21

## Context

ADR 0192 answered "build a wardrobe 600 × 2200 × 500" with Rust recipes: `carcass.box` and
`wardrobe.hinged`, one command `product.build`, and the arithmetic in code because a model that
computes a shelf width gets it 4 mm wrong. The owner rejected that answer on 2026-11-21 in one
sentence: **a construction must not be hardcoded; the model composes, and the program teaches it the
domain.** The reason is the domain itself — a shop makes a carcass with an overlay top, an inset top,
a back in a groove or a back nailed on, and a table of constructions is a table a shop cannot extend.
A recipe per construction also fixes one reading of every shared word and calls it the truth.

Measured before this record:

- `command_list` answers the id, the label, the level a command creates, whether it is enabled and
  the schema of its parameters — 51 commands with **no statement of which world a command serves**. A
  nut, a carcass and a scene light are three ids among 51, and the only way to tell them apart is the
  label, which is a sentence (`crates/mcp/src/server.rs`).
- Nothing in the machine-readable tree says how a build starts. The real wardrobe in
  `app/tests/recipes` was built by hand in the window, and the mistakes it made are all words nobody
  supplied: millimetres fed to a micrometre field (X115), a panel origin that hung above its own
  board (X117), a side named `left` and placed on the viewer's right (X118).
- The knowledge does exist, in three places that a caller cannot read: `docs/domain/*.md`, the class
  registry (`model::class::registry()`, 70 rows with slugs, labels and the words a person types —
  «стойка», «полка», «штанга» at `20.06`), and ADR 0105's stance table. The class registry already
  says a hanging rail is a class; no command creates one, and nothing said so.
- The frame shapes exist and are reachable: `Envelope` stores a `Footprint` — the rectangle, corners
  cut by a chamfer or a fillet, or a drawn ring — so a radius corner is `product.create` with
  `envelope.footprint`, which nothing told a caller.

## Decision

- **The program teaches its domain as one value.** `engine::guide()` returns one JSON document: the
  words of the domain and which reading wins, the four worlds, the order of a build from scratch, the
  shapes a frame takes with a real envelope as the worked example, the three stances with the
  quaternion each one is, the class registry grouped by family with the aliases a person types, the
  units and the four sides. It travels in the `command_list` answer and as `z3l guide`, from one
  function, so a client and a shell cannot be told two different things.
- **A construction is composed, never written.** There is no `carcass.box`, no `product.build` and no
  wardrobe function: a build is the commands a caller dispatches, and a batch makes it one call and
  one undo entry (ADR 0198, ADR 0214). The guide names the instruments and the order; the numbers and
  the choices are the caller's.
- **Every command declares one world.** `Family` — `furniture`, `geometry`, `tree`, `scene` — is
  derived from the area in front of a command's id, so a new command inherits its world from the
  module that owns it and a test that walks the registry fails the day an area has no row. A caller
  asked for a body never sifts the panel tools, and the report says the world beside the id.
- **What the guide states is generated wherever a table owns the answer.** The command lists are the
  registry's, the classes are the registry's, the stances are a table in `engine` held to ADR 0105 by
  a test that turns a panel, the frame example is an `Envelope` the model itself accepts. Prose is
  what no table can state: the order of a build, which reading of a word wins, which units travel
  where.
- **A world with no tool says so instead of inventing one.** Fittings are named by the class registry
  and created by no command; the guide says a caller builds one as a body in the geometry world or
  leaves it out, and the hardware tasks fill that hole rather than prose hiding it.
- **The units and the sides are taught, not assumed.** Micrometres for a size, millimetres for a
  placement translation, millidegrees for an angle, a quaternion for a rotation, and `left` is `+X`
  because the front view looks from `+Y`: three conventions, each guessed wrong once (X115, X118).
- **The domain record is the guide, not a prototype.** What a build was made of stays where it always
  was — as the document the commands produced and the undo entry that took it back. Recording a
  prototype reference on the nodes is Q1's decision, untouched here.

## Consequences

- A model's first call teaches it the program: about 15 KB of compact JSON, once per session, and
  four worlds and 51 commands it can now read without guessing. A client that already lists commands
  gets this for no extra call.
- The arithmetic is the caller's, which is the cost of not hardcoding constructions: a wrong shelf
  width is not prevented by a function any more. What catches it is the inspector and its findings
  (ADR 0191, J9) plus the diagnostics the model already reads — a trade the owner chose, and the
  reason J9 comes before any build the model performs alone.
- A new command area is one row in `Family`; a forgotten row is a failing test, not a silent `None`
  in a report.
- A new construction is a sentence in the guide only if it changes what the program can express; a
  shop's own carcass is a sequence of commands it composes, which is what lets a shop extend the
  program without us.
- The guide grows as the worlds do: a fitting family, a second frame shape and a new stance each add
  one entry, and a test holds each entry to the registry that owns it.
- 0192's recipe half is gone; its reading of units, of a typed refusal and of "the model chooses and
  explains" stands.

## Related

- [0192](0192-a-recipe-is-rust-that-emits-commands.md) — the recipe this record supersedes, and the
  parts of it that stand.
- [0185](0185-the-tool-registry-is-one-table-and-every-door-reads-it.md) — the registry this record
  gives a second field.
- [0105](0105-a-panel-stance-is-the-flat-frame-turned.md) — the stance table the guide publishes.
- [0094](0094-a-class-registry-gives-the-machine-its-semantics.md) — the classes and their words.
- [0098](0098-one-product-is-open-and-furniture-parts-are-born-inside-it.md) — why a build begins
  with the frame.
- [0191](0191-the-inspectors-first-seven-findings.md), [0193](0193-an-agent-task-is-the-acceptance-test.md) —
  what catches a wrong number now that a function does not.
- `docs/domain/classes.md`, `docs/domain/products.md`, `docs/domain/prototypes.md` §1.
