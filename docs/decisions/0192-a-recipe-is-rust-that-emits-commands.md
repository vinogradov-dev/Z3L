# 0192. A recipe is Rust that emits commands, and the recipe language waits

- Status: Superseded by 0215: a recipe is not a Rust function; its units, its typed refusal and "the model chooses and explains" stand
- Date: 2026-11-21

## Context

"Build a hinged wardrobe 600 × 2200 × 500" is the sentence this whole track exists for, and it is the
one thing a language model is worst at: arithmetic. A model that computes shelf widths produces a
wardrobe that is 4 mm wrong, and nothing in the document tells it so. The tree already holds the
answer's shape — construction knowledge is code and a prototype is a record (ADR 0034) — and the owner
names the split: **the model chooses and explains, the code builds and counts**.

Measured: no `product.build` command exists; no recipe function exists; the class registry already
carries a fill trait that picks a panel's size pair from a slot (the post and the shelf), which is the
same kind of knowledge at a smaller scale. The prototype mechanism itself is still a plan entry and a
domain document (`docs/domain/prototypes.md`), not an accepted record, so this record decides only what
a recipe is and where its parameters come from.

## Decision

- **A recipe is a Rust function with typed parameters that emits commands.** `carcass.box` and
  `wardrobe.hinged` are the first two; the parameters are `Length` and `Degrees` like everywhere else
  (ADR 0003), so a recipe cannot receive a millimetre float, and geometry comes out of `param`, not out
  of the assistant.
- **One command builds, and it is one undo entry.** `product.build` takes a recipe name and its
  parameters, runs the recipe inside one `EditScope`, and leaves the document with the panels,
  placements, class codes and the prototype reference the recipe decided. The model never dispatches
  twenty commands to build a carcass.
- **A wrong parameter is a typed refusal, not a guess.** The recipe validates its own parameters
  (a 0 mm height, a thickness no material offers, a compartment narrower than its shelves) and refuses
  with a sentence that names the parameter and the range — the same contract every command keeps.
- **The model picks a recipe and fills its parameters, and may not invent one.** The list of recipes is
  a query, with its parameters and their ranges, generated from the functions themselves. A name that
  is not in the list is refused like an unknown command id.
- **Learning is data, not generated code.** The workshop profile (settings, seeded from the
  standards) becomes skill cards and those become recorded prototypes; the builder exists only for the
  geometry a recorded prototype cannot express. A model that learns a shop's habits changes a record,
  never a Rust file at runtime.
- **A recipe language waits, with a stated return condition**: two real cases that a recorded prototype
  cannot express. Until then a recipe is Rust, which is testable, typed and free.
- **A recipe is testable without a model**: it takes parameters and returns the commands it would
  dispatch, so its own test asserts the panel count, the sizes and the placements — and
  `product.build`'s test asserts that one undo takes the whole carcass back.

## Consequences

- The arithmetic is ours and is covered by tests; the model's job shrinks to choosing, which is what it
  is good at, and to explaining, which is what a person wants.
- A new recipe is a code change and a test, not a configuration file — so a shop cannot add one by
  itself. That is the price of determinism, and the prototype registry (ADR 0034) is where that limit
  will be felt first.
- The recipe parameters become a tool schema (ADR 0185) and a card in the interface (ADR 0117), from
  one declaration, so the model and the person see the same ranges.
- `product.build` is one big command, which means one failure refuses the whole build. A half-built
  carcass would be worse than a refusal, and the refusal names the parameter that failed.
- Recording the prototype reference in the document keeps the "changing the recipe does not silently
  rewrite existing work" property the prototype registry already promises.

## Related

- [0185](0185-the-tool-registry-is-one-table-and-every-door-reads-it.md) — where a recipe's parameters
  become a tool schema; [0190](0190-plan-and-build-are-the-persons-switch.md) — a build is planned
  first.
- [0034](0034-nodes-carry-typed-properties.md) — typed node properties, the record a prototype's own
  reference travels in.
- [0187](0187-a-chat-turn-is-one-undo-entry-and-a-plan-first.md) — one turn, one undo entry.
- `PROJECT.md` §Assistant; `docs/domain/prototypes.md`.
