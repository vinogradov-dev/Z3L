# 0227. A released build carries its words on the surface

- Status: Accepted; states the units clause of 0185 with a mechanism, extended by 0215's guide, and
  refuses the side-vocabulary half of X118
- Date: 2026-11-22

## Context

The owner, 2026-11-21: «как будет решать задачу понимания агентом о чём речь, когда человек говорит
о левой и правой стороне? когда выйдет релиз, у него не будет readme и других инструкций». A released
build ships the binary and no prose, so everything a caller must know to be correct has to travel with
the tool that needs it — and the first wardrobe a model built came out mirrored (X118), a thousand
times its size (X115) and standing on the wrong end (X117).

Measured 2026-11-22 (J19): the four served tools carry the parameter schemas, the answers and the
refusals, and the guide of ADR 0215 teaches the worlds, the build order, the stances and the classes.
What none of them carried:

- **The unit of a number.** A schema is derived from the parameter struct, and `schemars` writes a
  field's own doc comment beside the reference to its type — which *replaces* the type's description.
  `Length`'s doc says "integer micrometres", so its fields lost that sentence the moment they
  described themselves: seven length fields (`object.cuboid.create`'s three sides, four extrusion
  heights, `panel.bow.set`'s sagitta) and all eleven `Placement` fields arrived without a unit. The
  instructions a client read first said "a length on the wire is millimetres", which is the thousand
  the model built at.
- **The form of a class.** The class rows answer code, slug, label, level, supply and the words a person
  types, but not the axis pair the registry stores (`Fill`): a caller that hears «полка» found `10.11`
  and still had to guess that a shelf takes the void's width and depth while a post takes its height
  and depth.
- **The reading of a description.** No command, query, class or answer names a side, and that stayed
  true by the owner's rule: the program must not carry the meaning of «слева», because the next person
  may read the same sentence the other way round.

## Decision

- **Every schema field whose type carries a unit is annotated with it.** `params_schema` writes
  `unit` beside every `$ref` to `Length` (`micrometres`), `Degrees` (`millidegrees`) or `Placement`
  (`millimetres`), from one table of types — not from a field table and not from per-field prose,
  which is what drifted. A field's doc comment may say more; it cannot take the unit away, and a test
  walks all sixty-six schemas and fails the first occurrence that lost it.
- **A class row carries the axis pair it fills**, in the words `docs/domain/classes.md` already uses:
  `h+d` for a post (`10.10`), `w+d` for a shelf (`10.11`), `w+h` for a back (`10.12`), and no key for a
  class that fills no void. The word is derived from `TraitSet::fill`, so the row and the fill tool
  cannot disagree.
- **The program states the frame; the caller brings the meaning of a side.** The guide's `sides`
  states the front (`+Y`), up (`+Z`), that the side a person in front of the piece sees on their left
  is `+X`, and how the piece's box answer is read; it also tells the caller to write its reading back
  («полки на +X, штанга на -X»), so a person corrects the caller instead of the document. It states
  no side vocabulary of its own: no command, query, class or answer takes or returns a side, because a
  side parameter would bake one reading of «слева» into the document and be wrong for the next reader.
- **The instructions a client reads first state the units and the reading.** An opening paragraph that
  said "a length is millimetres" was itself the X115 mistake; it now names micrometres, the
  placement's floating-point millimetres, integer millidegrees, and the front/left reading above.

## Consequences

- The surface is the dictionary: a session with the binary alone can read what a number is in, what a
  class is called and what axes it takes, and which axis a person's «слева» lands on — and the
  acceptance test builds five shelves and a rod from those answers alone (`crates/mcp/tests/served.rs`).
- The `unit` annotation is an extra JSON Schema key. Validators ignore annotations, so nothing rejects
  it; the cost is that `params_schema` is a derive plus one walk instead of a derive alone, and that
  the table must name a new unit-bearing type the day one appears — which the walk's test reports as a
  stale row.
- A caller that reads «слева» the other way round gets a piece on the mirrored side and no refusal:
  the alternative is a document that encodes one person's viewpoint, and the milestone's answer to a
  wrong reading is the reading said back in the answer, not a refusal (ADR 0226's rule about the
  interface's conditions and the command's). X118 keeps its first half — the words reach the caller —
  and loses the second: no command takes a side.
- X115's thousand times is the annotation's job; X117's "wrong end" is the reader's, because a part is
  placed by the caller's own placement and the anchor rule is stated where the command describes it.

## Revisit if

A maker's workflow needs a *typed* side on a document entity — a door that knows it is the left leaf,
not merely a panel at `+X`. That is a domain field with a drawing meaning, and it needs its own record
and a rule for what happens when the two readers disagree.
