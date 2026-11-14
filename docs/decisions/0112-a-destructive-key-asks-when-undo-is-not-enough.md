# 0112. A destructive key asks when undo is not enough

- Status: Accepted
- Date: 2026-11-14

## Context

Shift-click built a multi-selection in the tree and the viewport but nothing removed it: the tree's
row menu deletes one node, and a keyboard user could not take what they had selected. Looping
`node.delete` over the selection was the obvious fix and the wrong one — one dispatch is one undo
entry, so a ten-node removal would come back one node per Ctrl+Z, and undo is the recovery the owner
depends on (`AGENTS.md` §4.1).

The interface already carries two rules that pull in opposite directions. The skill's invariants say
nothing destructive is confirmed by a dialog when undo would do (`.pi/skills/ui-design/SKILL.md`),
and the window already asks before it discards unsaved work (`Confirm`, U45). A Delete over one row
and a Delete over twenty are not the same gesture for the person, even though they are the same
command.

## Decision

**One command for the whole selection.** `node.delete.many` takes `{ nodes: [Id] }`, is enabled when
the list names at least one node below the root, reduces the ids with `Project::top_most_nodes` — a
node whose ancestor is listed too is already taken with that ancestor, and an id the tree no longer
holds is stale view state, not a refusal — removes the rest and returns one changed outcome. The
session therefore records one revision bump and one undo entry per gesture.

**Delete does not ask about one node.** A single row is what the person just clicked, and undo brings
it straight back, so the key removes it at once. This is the skill's rule, and it stays.

**Delete asks about two or more.** A batch is removed in one step, so a row the person did not mean to
take is easy to miss and one Ctrl+Z has to bring all of them back; the question says how many rows go
and that undo is one step. The question is the one the window already owns — the unsaved-changes one
in `apps/furnicraft/src/file_menu.rs` — widened rather than copied: `Confirm` now asks about a tab,
the window or a selection, answers with *Proceed* or *Cancel*, and the answer performs the gesture the
question named. Three kinds of question in one host means one modal, one Escape and one place to
change how a destructive answer reads.

**A question owns the keys.** The pending question is part of the shell's modal set — the `modals`
gate in the frame's key handling — for the frame it is up, so a second Delete cannot stack a second
question, another tab's keys stay quiet, and Escape cancels. Escape is the question's second
*Cancel*: a modal that swallows the way out traps the person who pressed a destructive key by
accident.

**A lock refuses the whole gesture.** The selection is checked before either path — delete at once or
ask — and a locked node anywhere in it refuses everything, with the sentence the tree already uses
for a locked row (`Editor::locked_refusal`, ADR 0065). A lock is a guard on a gesture, not a document
rule, so the engine command never sees it.

## Consequences

- The tree's row menu keeps `node.delete` for one row; the key and the question use
  `node.delete.many` even for one node, because the batch is the general command and both paths
  behave the same when one row is removed. The palette gains a builder for the new id ("Delete
  selection"), so a command never sits in the registry without a surface that can run it — the
  inventory rule of ADR 0117.
- A multi-selection removal is one undo entry, one rebuild and one dialogs-worth of chrome: no second
  question per row, no second command id per count.
- The question is a window modal, so it survives a tab switch and applies to the tab it was asked in,
  even when the person activates another scene while it is up.
- The cost is one extra decision for a gesture the person may consider obvious: a person who selects
  two rows and presses Delete now presses two keys, not one. The owner asked for exactly that, and
  the escape hatch is that the answer is one click and the question is never asked for one row.
- `cargo xtask ui-rules`'s owner row for `apps/furnicraft/src/file_menu.rs` now reads "the file
  dialogs and the destructive question", because that module hosts all three questions.

## References

- `crates/engine/src/node_cmds.rs` — `node.delete.many` and `delete_targets`.
- `crates/model/src/project/node_cmds.rs` — `Project::remove_nodes`, the one-bump batch.
- `apps/furnicraft/src/shortcuts.rs` — the `Delete` key, read only outside a text field and a modal.
- `apps/furnicraft/src/document.rs` — `delete_request`, the rule the key follows.
- `apps/furnicraft/src/file_menu.rs` — `Confirm::delete`, the question and its Escape.
- ADR [0065](0065-locked-nodes-are-a-session-flag.md) — a lock is a guard on a gesture.
- ADR [0076](0076-a-tool-owns-the-next-click.md) — a running gesture owns the next click; a modal
  owns the keys for the frame it is up.
- ADR [0033](0033-a-multi-selection-is-placed-by-one-command.md) — the same rule for a drag.
- ADR [0117](0117-the-interface-is-one-inventory-of-capabilities.md) — every command has a place in
  the interface, which is why the palette builds the new command's parameters.
- Plan: U85.
