# 0194. The parameter schema derives where its type lives

- Status: Accepted; widens where ADR 0185 and ADR 0186 put `schemars`
- Date: 2026-11-21

## Context

ADR 0185 decided that a command declares its parameters from its own type, so the schema `z3l
commands --json` prints is a description of the struct the parser validates rather than a second,
hand-written table. ADR 0186 then measured that `schemars` "lives in `engine`, where the parameter
structs live".

Building that report measured what "its own type" really reaches. The 57 parameter structs of
`crates/engine` name 20 types of `model` — `Id`, `ItemId`, `MachiningId`, `Placement`, `PivotLocal`,
`ClassCode`, `PropertyValue`, `PanelNominalSize`, `PanelOutline`, `PanelRect`, `FaceSide`, `Contour`,
`Path`, `Primitive`, `Panel` features, and the ids of a ring — and two of `units`: `Length` and
`Degrees`. A schema cannot be derived for a struct whose fields have none, and the alternatives are
both wrong: a `#[schemars(with = "i64")]` on each field is a hand claim about the wire form that
nothing checks, and re-declaring the parameter structs in `engine` would duplicate the document.

Two of those types are also the interesting case. `Contour`, `Path`, `PanelNominalSize`, `Envelope`
and `Placement` are written in a wire form (`#[serde(try_from = "…Wire")]`), and `schemars` describes
such a type *through its wire form* — which is exactly what a caller has to send, not a privilege of
the stored type. Deriving in `model` is what makes the report true about the parser.

## Decision

- **`schemars` derives where its type lives.** `units` and `model` carry the dependency as a derive
  over `serde` types, and gain `JsonSchema` on the types a command's parameters carry — and on the
  wire form of a type that has one, because that is the shape the parser accepts.
- **`engine` names the struct once per command.** `CommandSpec::new::<P>` takes the parameter type,
  and the schema comes from the same `P` the command's `run` parses, so a command cannot publish a
  shape it does not accept.
- **No overrides, no table.** A field whose type has no schema is a compile error naming the type,
  not a `with = "…"` escape and not a fields table that could drift.
- **The report prints it.** `z3l commands --json` gains a `params` object per row; the text form
  keeps naming ids, labels and levels, because a schema on a terminal line is noise.

## Consequences

- A caller reads the parameter contract from the program: required fields, optional ones, nested
  shapes, and the closed set of names (`additionalProperties: false`, because every parameter struct
  denies unknown fields).
- Doc comments become the schema's descriptions, so the report carries the words the command's own
  type already spells out. The whole registry is about 220 KiB of JSON, which is a discovery call
  paid once — and the reason the MCP tool list will filter by id.
- `model` and `units` now depend on `schemars`, a pure-Rust derive with no build script; the document
  still holds no I/O, no GPU and no kernel type.
- A new field type in a parameter struct costs one `JsonSchema` derive beside that type, and the
  compiler demands it. A type that arrives from another crate with no derive is a stop-and-think,
  which is the right moment to ask whether it belongs on the wire at all.
- The schema describes shape, not meaning: `Length` is an integer of micrometres and `Degrees` an
  integer of millidegrees, and no schema says so. A caller still reads the command's label and its
  refusal, and the unit conventions of ADR 0185 stay words rather than schema keywords.
