# 0115. A right click opens the node's actions, and a right drag pans the view

- Status: Accepted
- Date: 2026-11-15

## Context

A right click over the scene did nothing, and a right drag panned the camera. The only context menu
in the program was the scene tree row's, built inside `scene_tree.rs` from five hand-written menu
functions, so the actions of the piece under the hand were two or three clicks away in the left
panel, and the piece had to be found there first. ADR 0117 made the inventory the one table of
capabilities and named the context menu as the surface that filters it; nothing had yet drawn that
filter, and a second hand-written menu in the viewport would have been exactly the fourth register
the inventory ends.

The right button carries two meanings already, and they have to be told apart without asking the
person to change the hand: the viewport pans on `dragged_by(Secondary)`, and a drawing draft commits
its figure on `clicked_by(Secondary)`.

## Decision

**One list, drawn by both surfaces.** `crates/ui/src/node_actions.rs` answers `entries(subject,
source)` for a node, the root frame or empty space: the inventory rows whose `Applies::Context` names
that node's kind, in the table's order, plus the session entries that dispatch no command — the eye,
the padlock, *Isolate*, *Show all*, *Add child* and the open product. Each capability entry carries
its id, its label, whether it can run and the sentence that says why not. The tree row and the
viewport popup both draw that one list through the same `show`, so an entry cannot exist in one and
miss the other.

**A click is a click, a drag is a drag.** The viewport opens the menu on `clicked_by(Secondary)`,
which egui reports only while the pointer stayed inside its click distance, and leaves
`dragged_by(Secondary)` to the pan it already owned. A drawing draft keeps its right-click commit,
because the draft owns the pointer while it runs and the menu opens only when no tool does.

**The menu names its node and does not select it.** A right click acts on the node under the
pointer, exactly as a tree row's menu does, so it neither changes the selection nor moves the
inspector; the entry carries its own node id into the command. Empty space owns the document-level
view action only, *Show all*, because creation is armed from the ribbon with its parameters
(0117, 0119).

**A floating surface is chrome.** The popup is an `egui::Area` on `Order::Middle`, anchored at the
pointer, declared to `Chrome::covers` through the rectangle it drew. A press inside it is the entry
under the pointer and never a pick, a border drag or a camera gesture (0080). Escape closes it, and
so does a press outside it; that press is spent on the dismissal, so a menu never edits the scene it
was closed over.

**A destructive entry is drawn last.** The hand is already on the row under the pointer, so the
entry that deletes is never the one that happens to be there.

## Consequences

- A capability added with `Place::ContextMenu` appears in both menus without either file changing,
  and `cargo xtask ui-rules` keeps the table the only list.
- The tree's row menu lost its five builders: the wording, the order and the enablement of an entry
  are the list's, and the row cannot drift from the viewport.
- `node.rename` and `node.delete.many` lost their `Place::ContextMenu` mark: a menu carries no text
  entry, and a batch delete belongs to the key and the palette (0112). Rename from a menu needs a
  field, which is the card's, not a menu's.
- The right button now means three things in a viewport: pan, commit a figure, and open the actions
  of what is under the pointer. The split is the pointer's movement, not a modifier, and the
  drawing tool's commit keeps priority because a draft owns the pointer while it runs.
- Revisit this if a second dynamic list appears over the scene: the tool card (0119) is a parameter
  surface, not an action list, and the ribbon stays the stable lexicon.
