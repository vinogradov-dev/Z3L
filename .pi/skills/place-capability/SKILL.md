---
name: place-capability
description: "Place a Z3L capability in the interface: its inventory row, the places that draw it, its context, key, icon and label, and the atoms it consumes. Use when a command, a method or an atom is added, renamed or removed, or when a capability moves between the ribbon, the context menu, the tool card, the palette, an inspector card and the API."
license: MIT
compatibility: pi
metadata:
  audience: agents
  workflow: interface
---

# Place a capability

A capability that no surface draws is invisible, and one drawn by two hand-written lists drifts
apart the moment either moves. This is the procedure for the one table that prevents both: the
inventory in `crates/ui/src/inventory.rs` (ADR 0117). Run it whenever an id or a place changes —
before the button, not after it. The shape of the surface itself is the design skill's job.

## The words, before the row (ADR 0118)

| Word | What it is |
|---|---|
| capability | a command: a stable id, typed params, exactly one undo entry |
| atom | a mechanism: it reads input, computes and writes nothing (the snap resolver, the probe, the ruler) |
| tool | a capability or an atom with an input method and a place in the inventory |
| method | one input variant of a tool, e.g. a straight segment and an arc are two methods |

A mechanism that two tools need, or whose value a command consumes, **is an atom** and gets its own
row: an id, a name, a key, a cursor, a cancel and a place. A mechanism written inside one tool is the
same defect as a second numeric field.

## The row

One `Row` per capability, keyed by the command's stable id: `id`, `label`, `places`, `applies`, `key`,
`icon`. A row carries no behaviour — the command stays in `engine`.

1. **Name the id.** `<area>.<verb>`, lowercase and stable forever: a new meaning gets a new id. Add
   the command to `crates/engine/src/commands.rs` (`COMMANDS`) from its own
   `crates/engine/src/<area>_cmds.rs` module, then add one row in `ROWS` in the same order.
2. **Write the places.** The list is closed by ADR 0117: `Ribbon` (the lexicon), `ContextMenu` (the
   grammar), `ToolCard` (the armed tool's parameters), `Palette` (recall), `Card` (an inspector card)
   and `Api`. Add a place only when that surface draws the capability today, never in advance; a row
   with no surface is `Api` and says in `plan/debt.md` why the CLI or the assistant is its only
   home. A new `Place` variant is a decision, not an edit.
3. **Declare the context.** `Applies::Always` for a creation, a view toggle or a document-wide
   operation; `Applies::Context(ActsOn::…)` for anything acting on the node under the pointer. The
   context menu, the card and the tool card all filter by it, so a wrong context is a capability
   offered where it cannot run.
4. **Claim the key, once.** A binding is one row of `KEYS` in `crates/ui/src/inventory.rs`: the id it
   runs, `Chord` (the modifiers), `egui::Key` and the label the interface prints, checked against the
   pair by the inventory's test. One key runs one id and one id has one key. A command's key is filed
   under its id; a drawing or view tool's under the *method* id of the ribbon map, because a command
   reached by six methods has six keys; a shell action (`file.save`, `edit.undo`) is one row of
   `ACTIONS` with its words. The readers ask the table by id — `consume` in
   `app/src/shortcuts.rs` for the shell, `inventory::key_label` for the ribbon hint and
   the Settings page — so a remapped key is a data edit. A key that shadows a text field or a running
   tool is a finding (ADR 0076).
5. **Name the icon, or leave it empty.** `icon` is the *one* button that draws the capability, taken
   from `crates/ui/src/icons.rs`; it is `None` when no button draws it. A new drawing is authored as
   SVG under `assets/icons/`, follows `assets/icons/STYLE_GUIDE.md` and `assets/README.md`, and a
   destructive action keeps its label (AGENTS.md §4.12). Draw it with the `draw-icon` skill, not from
   memory.
6. **Use the command's label.** `label` is the command's own `Message`, never a second string, so the
   ribbon, the menu, the palette and the undo history print one word for one id. Words live in
   `crates/text/src/messages.rs` (ADR 0086).
7. **List the atoms it consumes.** For every mechanism the capability uses, check that the tool that
   already needed it did not write its own copy. A second consumer promotes the mechanism to an atom
   in `ATOMS`, with the row above; a first consumer may keep it private, and the next one is the
   trigger, not an excuse.
8. **Let the surface filter the table.** The palette (`crates/ui/src/palette.rs`, `params_for`) and
   the node actions (`crates/ui/src/node_actions.rs`) read `in_place`/`row`; the armed tool's card is
   the same filter (ADR 0119), and a surface that grows its own list of ids is the defect. The ribbon
   stays curated on purpose: a `ToolSpec` in `crates/ui/src/toolbars/spec.rs` may name a method, so a
   cell is not always one row.
9. **Prove it, then stop.** The row is a measurement: the test in `crates/ui/src/inventory.rs` that
   walks the registry, and the gate below, both read it.

## When an id moves or retires

Renaming means a new id and a retired row, never a reused one: the palette, saved macros and the
assistant reach ids by string. A removed command loses its row, its key and its places in the same
change, and `plan/map.md` loses the row for the feature if the feature went with it.

## The gate

```sh
cargo xtask ui-rules
cargo xtask skills
```

`cargo xtask ui-rules` runs the inventory check in `xtask/src/inventory.rs`: a registered command
with no row, a row naming an id that is neither a command nor declared in `ATOMS`, an `Api` row that
also claims a surface, a second row for one id, a live ribbon cell with no artwork, a key bound
twice, a binding or an action naming an id no command, atom, action or ribbon method holds, and a
shell action with no key all fail, each with a sentence naming the file. One icon file shared by two actions is
printed for review and passes. `cargo xtask ci` runs the same rule with the rest of the gate.

## Stop and ask the owner

- A command that should be reachable only from the ribbon or only from a menu — that is ADR 0117's
  finding, not a style.
- A new place, a second home for one capability, or a key that collides with an existing gesture.
- A document field the command needs but the model does not have (AGENTS.md §4.9).

## Checklist

- [ ] The id is namespaced, stable and not reused; the row follows the registry's order.
- [ ] Every place is true today; an `Api` row is exclusive and has its reason recorded.
- [ ] The context matches every surface that filters by it.
- [ ] The key is the column's, bound once, and does not shadow a field or a tool.
- [ ] The icon exists and suits the action, or the row has none.
- [ ] The label is the command's `Message`, in both languages.
- [ ] Each shared mechanism is an atom with its own row, not a second copy in a tool.
- [ ] No surface keeps its own list of ids.
- [ ] `cargo xtask ui-rules`, `cargo xtask skills` and the inventory test are green.

## Anti-patterns

- Adding a button and writing the id in a match arm, with the row added later (or never).
- Declaring a place in advance so the row "looks complete"; a place is added by the surface that
  draws it.
- A planned tool that is not in the ribbon map, or a ribbon cell that runs a method no row names.
- An icon borrowed from another product, drawn in code, or reused for an unrelated action without a
  review note.
- A second label for an existing id, or a second id for an existing behaviour.
