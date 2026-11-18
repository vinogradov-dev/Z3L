# 0145. The group digits follow the order of the work

- Status: Accepted; reassigns the group digits of 0094 and supersedes the group-`10` sentence of 0097
- Date: 2026-11-17

## Context

ADR 0094 made the first two digits the object's *function family* — seating, lying, storage — and the
numbers were handed out in the order the families were written down: `10` seating, `20` lying, `30`
storage, `40` surfaces, `50` fittings, `60` appliances, `70` boards and trim, `80` general. A picker
that walks the registry therefore opens on the chair and the stool and reaches the cabinet third, and
a product card lists a scene prop before the case a cabinet shop actually builds.

A workspace that wants the list to read as the work has two places to put that: an order the surface
applies, or the digits themselves. A presentation order was built first and rejected: it leaves the
stored number lying, so a project file says `10.01` for a chair and every export, report and external
reader still meets a scene prop first. The digits are the one order every consumer already reads, and
`docs/domain/furntypes.md` — the registry of the previous milestone — wrote the cabinet as `10.01`.
Matching it costs nothing: no build has shipped, so ADR 0094's immortality rule ("a code, once in a
released build, is never reused, never renumbered") has not started.

## Decision

**The group digits are assigned in the order of the work, and the type and variant digits do not
move.** A row keeps its place inside its family; only its group changes:

| Group | Family | Was |
|---|---|---|
| `10` | storage — the case, its panels, facades and drawers | `30` |
| `20` | boards and trim — worktop, plinth, cornice, filler, wall panel, rail | `70` |
| `30` | surfaces — the table and its forms | `40` |
| `40` | lying — the bed and its mattress | `20` |
| `50` | fittings — unchanged | `50` |
| `60` | appliances — unchanged | `60` |
| `70` | general — the placeholder product | `80` |
| `80` | seating — chair, stool, bench, sofa | `10` |

So `30.01.02` becomes `10.01.02`, the wall cabinet; `70.01` becomes `20.01`, the worktop; `10.01`
becomes `80.01`, the chair, and the placeholder `product.create` assigns becomes `70.01`. The case
opens the registry, the trim that goes with it follows, then the products a shop makes less often,
then what is bought, then the placeholder, and the seat closes it, because a chair or a sofa belongs
to a scene rather than to a project.

**The order is a decision about the digits, not about the interface.** Every surface walks
`registry()` in code order and no surface sorts: `ClassCode::row` binary-searches the table, so an
ascending table is a requirement, and a separate presentation order beside it was the thing this
record replaces.

**The placeholder is not first, and that is deliberate.** A product card pre-selects the placeholder
silently, so the row a maker has to *find* is the real class, not the fallback; `70` puts the
fallback after every function it could stand for.

**Group `10` names a function, not a role.** 0097 removed the old `10` = "product" group because the
top level classified the role an object plays in an assembly, and that reading stands: `10` is where
a case lives, and a table is `30.01.01` while both are `level = Product`. The role is still a trait,
never a code range.

## Consequences

- `docs/domain/classes.md` §7 is written in the new numbers, in ascending order, and the test that
  compares the table row for row keeps the document and the table from drifting.
- The exporter's `classes` table, the golden order package and every test literal carry the new
  numbers; nothing else in the format moved, and no schema version was bumped, because the registry
  is semantics and the document stores the integer either way.
- A workshop that read the old numbers in a build from this milestone must re-learn seven groups. No
  released build exists and no saved project carries the old codes, so this is the last moment the
  reassignment is free.
- Any future family takes the next free group, and the *number* it takes is now a decision about
  where it sits in the order of the work, not about where it was appended.
