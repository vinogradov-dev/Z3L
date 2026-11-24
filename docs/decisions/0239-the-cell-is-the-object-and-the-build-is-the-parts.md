# 0239. The cell is the object, and what it becomes is a property of the part

- Status: Accepted; supersedes 0110, refines 0099 and 0118
- Date: 2026-11-24

## Context

ADR 0110 answered the owner's complaint that two builds of one panel looked like two panels by keeping
one cell per *kind* — post, shelf, wall — and moving the build (largest fit or the niche's shape) into
the cell's method list. What stayed was the class in the cell: the ribbon asked a maker to name the
class before the part, three cells differed only in the class their `fill` trait read, and the class
could not be changed after the click — `panel.create` takes it and no command rewrote it, so a wrong
class cost a delete and a redraw. The two builds and a repeat count had no home but a method list that
read as a second panel.

## Decision

**The Panels group is three cells: Panel, Panel from contour, Panel by points.** The three class cells
and the niche cell retire with the ids they pinned, and a workspace that pinned one of them loads as
no pin.

**What a panel becomes is chosen on the part.** One creation menu holds the three rows a maker
decides: the class (post, shelf, wall, and later the overlay wall), the build (the largest fit or the
niche's shape) and, from its own task, the count. Before the click the rows are the tool card's
(ADR 0119), at the pointer; after the click the same rows are the node's context menu, through one
command that rewrites the class and the build from the same void in one undo entry. A part born with
no class carries the role its plane implies (ADR 0238).

**The empty space is an atom, not a cell.** `probe.void` keeps its key, its row in the atom table and
the open product's own context menu; the fill plans from it (ADR 0118). A ribbon cell that only
reported a measurement gave the interface nothing a status bar does not already say.

## Consequences

- A fill cell no longer says which class it will build, so the pressed state answers "a fill owns the
  next click" rather than "this exact class" — one cell, one build, one pointer mode.
- The `.shape` method rows retire with their ids: the build is the part's property, and the tool
  card carries it. `panel.fill` already takes the class and the mode as parameters, so the card feeds
  the command it always could.
- The class stops being a one-way choice: the context menu's command is what finally lets a maker
  correct a class after the click, which is the complaint the three cells were created to avoid.
- The workspace's pinned method now means "fill the niche or draw the board" — the two ways a Panel
  cell can make one — instead of a build.

## Related

- [0099](0099-the-class-fill-trait-names-the-size-pair.md) — the class names the size pair the fill takes.
- [0110](0110-one-fill-cell-and-its-build-method-in-the-cells-own-list.md) — the record this supersedes.
- [0118](0118-the-niche-is-an-atom.md) — the empty space as one measured value.
- [0119](0119-the-armed-tool-shows-its-parameters-at-the-pointer.md) — the card the class and build rows live in.
- [0238](0238-a-panels-role-is-the-plane-it-lies-in.md) — the class a part is born with.
- [`crates/ui/src/toolbars/spec.rs`](../../crates/ui/src/toolbars/spec.rs) — the three cells.
