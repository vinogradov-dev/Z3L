# 0191. The inspector's first seven findings

- Status: Accepted
- Date: 2026-11-21

## Context

The assistant is worth more as a reviewer than as a builder: a person can draw a carcass, and a
wardrobe with a shelf 4 mm too wide, a rail through a door and a back sheet cut from a panel longer
than the sheet is the case that costs a workshop money. The owner names the split of labour that makes
this cheap and honest: **the code finds the mistake, the model explains it and orders the fixes**. A
model that "looks" at the scene cannot be trusted to notice a gap; a rule can, and a rule can be tested
with a fixture that fails before it passes.

Measured: no finding type, no severity, no inspector pass exists in the tree; the numbers a rule needs
are scattered but present — the product's authored envelope (`Project::niche`, ADR 0103), the class
registry's dimensions, the contact port's overlap answer, the sheet size the standards carry, and the
workshop profile the settings now hold (`PROJECT.md`, §Domain specifications).

## Decision

- **A finding is a report, never document data**: `Finding { code, severity, nodes, text,
  suggested_commands }`, where `code` is a stable English identifier, `severity` is error, warning or
  info, `nodes` are the document's own ids (never mesh indices, `AGENTS.md` §4.10), `text` is a
  sentence a person can read and `suggested_commands` is an ordered list of command ids with the
  parameters the fix would dispatch.
- **Seven rules, and only seven, in the first set.** Each has a code, a unit, a threshold and a fixture
  pair (one scene that finds it, one that does not):

  | Code | Rule | Severity |
  |---|---|---|
  | `envelope_out` | a node's bounds leave the product's authored envelope by more than 0 mm | error |
  | `body_overlap` | two bodies intersect by volume; a container inside a container is legal | error |
  | `shelf_fit` | a shelf's width is the inner opening − 2 × gap; wider is an error, 2–5 mm narrower is "loose", bearing under 30 mm is "barely supported" | error, warning |
  | `rail_place` | the rail runs along the width, inside the compartment, its centre 40 mm (±5) below the underside of the top, at least 50 mm from a shelf, inside the envelope at the back and the front | error, warning |
  | `edge_band` | a visible edge (the front ends of sides, bottom, top, shelves and fronts) without banding; a hidden edge that carries banding | warning, info |
  | `door_gap` | a front is the opening − 2 × 2 mm per side (±0.5 warns); two fronts of a pair neither intersect nor run onto a side | error, warning |
  | `sheet_fit` | a part longer or wider than the sheet after the grain is honoured and no allowed turn is left | error |

- **Every number comes from the profile, not from code**: the panel thickness, the plinth, the gap, the
  sheet and the rail height are the settings the build seeds from the workshop standards
  (`PROJECT.md`, §Domain specifications). The values in this record are the defaults of that profile,
  and a rule that hardcodes one is a defect.
- **Hardware findings wait for M8.** There is no hardware catalogue yet, so a fastener rule would
  invent the thing it checks. They join the set when the catalogue exists.
- **The inspector is a derivation, and it reuses the derivation that already owns each answer**: the
  envelope is the product's, the overlap is the contact port's, the sheet and the profile are the
  standards', the edges are the banding's. No rule re-derives a mesh.
- **The model never computes a finding.** It reads the list, explains what a person would see in the
  workshop and proposes the order of the fixes; a fix is a `suggested_commands` list a person can run,
  in `plan` first (ADR 0190).

## Consequences

- A false positive costs as much as a miss: a warning nobody can act on teaches people to ignore the
  panel. Each rule ships with its fixture pair, and a rule that cannot name the node it blames does not
  ship.
- The set is a *set*: one pass, one list, stable order by severity then code. Seven separate tools
  would be seven things a model has to remember and a person has to read.
- `suggested_commands` is the contract with the agent, so a rule without a fix names an empty list
  rather than a guess.
- The numbers now have a second reader, which makes the profile a real setting rather than a plan: a
  workshop that uses 18 mm board changes one value and the rules follow.
- A new rule costs a code, a fixture pair and a sentence — and it is the natural place for the domain
  knowledge the owner keeps adding.

## Related

- [0190](0190-plan-and-build-are-the-persons-switch.md) — a fix is proposed in `plan` first.
- [0185](0185-the-tool-registry-is-one-table-and-every-door-reads-it.md) — `inspect` is a tool of the
  same registry, not a second surface.
- [0103](0103-one-probe-serves-the-niche-the-distances-and-the-move.md) — the product envelope a
  finding measures against.
- `docs/domain/products.md`, `docs/domain/panels.md`; `AGENTS.md` §4.10.
