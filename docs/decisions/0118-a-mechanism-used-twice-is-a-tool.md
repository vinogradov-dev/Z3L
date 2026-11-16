# 0118. A mechanism used twice is a tool, and a measurement is not a dimension

- Status: Accepted
- Date: 2026-11-14

## Context

The owner's example: to build a panel by naming the empty space, a session wrote a niche
calculation; that calculation is a mechanism in its own right and deserves its own tool. The same
question now stands before the ruler: a person wants to measure a gap without creating anything.

The code already carries the pattern without a name. One probe answers the niche, the exact
distances and the move (0103); one box editor serves a cuboid and a product envelope (0101); the
snap resolver composes the grid, the axis and the draft ends for every drawing tool (0051). But a
mechanism written inside one tool is still allowed and invisible, and a measurement is one gesture
away from being mistaken for a dimension object in the document.

## Decision

**Four words.** A **capability** is a command: a stable id, parameters, one undo entry. An **atom**
is a mechanism: it reads input, computes and returns a value, and writes nothing. A **tool** is a
capability or an atom with an input method and a place in the inventory (0117). A **method** is one
input variant of a tool.

**The test.** A mechanism that two tools need, or that returns a value a command consumes, is an
atom and gets its own row: an id, a name, a key, a cursor, a cancel and a place. A mechanism
implemented inside one tool is a defect of the same size as a second numeric field.

**An atom reuses the shared input and geometry.** Point resolution and snapping come from the one
resolver (0051); the surfaces an atom meets come from the one probe (0103). A second snap list or a
second probe is the defect, whatever the tool it hides in.

**The ruler is a transient atom, not a command.** No document object, no undo entry, no `.z3l`
field: its result is information. Two clicks set a length; the value is drawn at the pointer and in
the centre zone of the status bar, while the left zone says what to do next. The next click starts a
new measurement rather than extending the previous one, and Escape leaves and writes nothing, because
nothing was written. Lengths are formatted by `units` like every other number.

**A dimension on a drawing is a different thing.** It is a document object with an id and undo —
`sketch.dimensions.linear` in the ribbon map — and it stays planned until drawings exist. Merging the
two would put view state in the document, which 0029 forbids.

**The niche becomes a tool of its own.** Selecting a niche is a place with a name, and fill, a shelf,
a drawer bank, a report and the assistant consume the same value from the same probe. One code path,
two entry points: the niche tool, and the niche under the pointer while fill is armed.

## Consequences

- The ruler cannot be dispatched by the palette or the assistant, by design. An atom that must be
  scriptable becomes a read — a query — and a query is not a command.
- Each atom adds a surface and a place to keep current; the inventory row is what stops it from being
  a private mechanism again.
- Fill loses its monopoly on the niche calculation: the probe is called in one place, and the fill
  tool is a consumer like the others.
- A second measuring need (an angle, a radius, a thickness) adds a method to the same atom rather than
  a second tool.

## Related

- [0117](0117-the-interface-is-one-inventory-of-capabilities.md) — the row an atom gets.
- [0029](0029-view-state-belongs-to-the-session.md) — why a measurement is not document data.
- [0051](0051-snapping-composes-the-grid-the-axis-and-the-draft-ends.md) — the one snap resolver.
- [0103](0103-one-probe-serves-the-niche-the-distances-and-the-move.md) — the one probe.
- [0106](0106-a-niche-is-a-convex-cell-of-wall-planes.md),
  [0109](0109-a-niche-is-bounded-by-the-faces-the-pointer-sees.md) — what a niche is.
