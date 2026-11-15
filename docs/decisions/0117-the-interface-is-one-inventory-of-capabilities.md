# 0117. The interface is one inventory of capabilities: every command has a place

- Status: Accepted
- Date: 2026-11-14

## Context

The owner's rule is that every feature has a representation in the interface, and that the tool a
person needs next is under the hand. Today that rule is kept by hand in registers that do not know
each other: the engine's command registry (`CommandSpec`, 56 ids, the count its own test asserts),
the ribbon map (`ToolSpec` and `ToolKind`: 29 live methods and 79 declared rows that do not run
yet) and the palette's `params_for`, which builds parameters for twelve ids and refuses the
rest. Nothing checks that a command has a home, so three ids are named only inside
`crates/engine` (`CONTOUR_CREATE_ID`, `NODE_ORIGIN_REBASE_ID`,
`NODE_PROPERTY_SET_MANY_ID`), and nothing stops a surface from growing a fourth hand-written
list.

The same response that adds a button is therefore free to place the capability where that one
response found room, which is how an interface stops being learnable.

## Decision

**One const table is the inventory of capabilities.** A row is keyed by a stable id — a command's
id, or an atom's id when the mechanism has no command (0118) — and names the places it appears in,
the context that makes it available, its default key, its icon and its label message. A place
without an icon or a message is a *planned* row: it stays in the table so the map reads whole and it
is not drawn.

**A surface filters the table; none keeps its own list.** The roles are fixed:

| Surface | What it is |
|---|---|
| Ribbon | the lexicon: a stable, learnable position for every tool, changeable only by a decision |
| Context menu | the grammar: the rows whose context matches the selection and the pointer |
| Tool card | the parameters of the armed tool, at the pointer (see 0119) |
| Palette | recall: filtered by typed text over the same rows |
| Inspector card | the data of the selected node, not the list of actions |

**Context is data, not code.** Each row declares `Context` — it applies to this selection or
operation — or `Always` — a general action. A context menu is then a filter over the two, and
changing its composition is a data edit rather than a rewrite.

**Three speeds, one id.** The ribbon teaches the tool, the context menu and the key make it fast,
the palette rescues a forgotten one; the label, icon, key and enabled rule are the same in all three.
A capability reachable only by one of them is a finding, not a style: it either gets a place or
declares `Api` with the reason it is meant for a CLI or the assistant only.

**A key is a column.** The table carries the default key of a method, and `shortcuts.rs` reads it
instead of keeping a second list, so user remapping becomes data rather than a refactor.

**A gate, not a habit.** The check fails when a registered command has no row, when a row names an
unknown id, message, icon file or surface, when a live row has no art, or when one id carries two
different labels or keys. One icon file shared by two unrelated actions is reported for review and
is not a failure, because a service action may legitimately reuse one drawing.

**No moving toolbar.** Dynamic actions live in the context menu at the pointer and in the tool card.
A strip that changes with the selection would break the ribbon's spatial memory, and the program
already has three dynamic surfaces. Revisit this only with a measurement of how often a needed action
is absent from all of them.

## Consequences

- Adding a command is two edits — the engine and one row — and the gate names the second when it is
  missing. A defect like the three orphan ids cannot ship quietly again.
- The context menu (U92) becomes a filter over the table and the Snaps group (U91) a set of rows, so
  neither can drift from the ribbon; the palette keeps its parameter builders but each id it can run
  needs a row.
- The table is a register to keep current, and the gate is the only thing that makes it worth having;
  a row that no surface reads is a defect the gate reports.
- The key column is its own const table (`KEYS` in the same module), keyed by the same ids: a key
  runs a *method*, and one command can carry six, so four fields — id, chord, key, printed label —
  say a binding once. The shell consumes it by id, the ribbon prints the label and the Settings page
  walks the rows; an action the shell owns rather than the document (`file.save`, `edit.undo`) is one
  row of `ACTIONS` with its words (U96).
- `plan/map.md` keeps saying where a feature lives in code; the inventory says where it lives in the
  interface. They are different questions and each has one owner.

## Related

- [0028](0028-the-assistant-is-a-client-of-the-command-registry.md) — one registry, many clients.
- [0066](0066-the-ribbon-owns-the-tabs-and-a-tool-is-a-labelled-button.md),
  [0091](0091-the-ribbon-is-one-const-map.md) — the ribbon as a const map.
- [0092](0092-a-field-is-a-number-with-its-unit-outside.md) — the one numeric field.
- [0118](0118-a-mechanism-used-twice-is-a-tool.md) — the atoms a row can name.
- [0119](0119-the-armed-tool-shows-its-parameters-at-the-pointer.md) — the tool card.
