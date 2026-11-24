# 0240. A lap is the board's own number, and the mounting is the act that lays it

- Status: Accepted; governs `FillMount`, `PanelTrims` and the scene's `settings` section
- Date: 2026-11-24

## Context

N28 put an overlay/inset mounting outside the fill's question — "a property of a mounted front" —
and the owner has since ruled it a **creation key**: the wall cell may lay its board over the frame
instead of inside the opening. Two facts had no home. How far past the opening the board runs was
nobody's data, so a maker could not read or correct it; and the form itself lived on the hinge class
(`plan/questions.md`), where a door could not answer whether it was flush or inset.

The owner's own reading, given twice in the task: a workshop types **подрезка торца** — an end trim,
one signed number per end, positive cutting the end in and negative letting the board out past its
size — and the size follows the number. At creation the four numbers come from the settings and from
what the frame actually offers: the lap runs only where a part stands behind the opening to run over,
up to that part's own end face, and it never rides onto a part standing in front of the face. A side
with no part behind it takes no lap, because an end with nothing under it would hang in the air.

## Decision

**The trim is document data on the panel.** `PanelTrims` is four signed lengths, one per nominal
edge, in the numbering the card prints (1 the end at zero width, 2 at the full length, 3 at the full
width, 4 at zero length). A positive number cuts that end in, a negative one lets it out, and an
ordinary board carries none — the field is skipped in the file whenever all four are zero, so a scene
that never had a front in it is written exactly as before.

**A number belongs to the end and never accumulates.** `panel.trim.set` moves the side by the
difference between the number asked and the number stored, so typing the same number twice moves
nothing and typing the first one back puts the end where it was. An end at the panel's own zero
corner carries the origin with it, which is why the placement travels in the same revision as the
size.

**There is no mounting form to store.** The mounting is an operation and not a field: a front is
recognised by its placement and its trims, and turning one into the other is a later operation that
moves it in depth and rewrites its ends (and, once hinges exist, their model). What the *creation*
records is what the board is: `panel.create` writes the trims the plan measured, in the same revision
and the same undo entry as the outline, so one click stays one `panel.create`.

**The frame decides the lap, inside the workshop's cap.** An overlay board is the opening's rectangle
laid one thickness in front of the void's start plane — its inner face *is* that plane. Each end runs
outward only where a body reaches the opening's plane from behind: the lap is that body's face
measured from the opening's edge, capped by the workshop's number, and the nearest such face wins. A
body standing proud of the plane stops the end at its near face, which trims the board back inside
the opening rather than burying the neighbour; a body proud of the face *inside* the opening is
refused with a sentence, because no end can stop it. Only the frontal pair takes a mounting — a shelf
or a post stands across the void — and the model says so by name.

**The lap's default is the scene's, not the workspace's.** The envelope gains a `settings` section
carrying the factory's numbers; today one, `overlay_lap`, fourteen millimetres. A file written before
the section existed answers that default, which is the gap the workshop's standards file (O2) closes;
`plan/debt.md` records the flip. The key is additive and preserved by a build that does not know it,
so the framing version does not move for it.

## Consequences

- The four numbers are the same data a door-gap or a hinge rule needs to tell a flush front from an
  inset one; the trims answer it for a front that laps, and a zero-trim front is a front that does not.
- A person corrects a lap on the part, in millimetres, and reads the size move with it. The cost is
  that a hand resize of the same side leaves the number where it was: the trim records how far the
  end was moved from where the tool left it, not a distance from an opening the panel does not store.
- The overlay board is always a rectangle — a facade covers the opening, so its section is the
  opening's box and the two builds give the same board. The card hides the build row for it.
- Re-planning a front (`panel.build.set`) measures its cell from its own inner face stepped one
  micrometre into the opening, and asks for the ends it already has: a frame that loses a neighbour
  does not silently widen a front the person sized.
- The two caps are the document's: an end trim of up to a metre, a lap computed from the parts' faces,
  and a board that would take the whole opening is refused rather than inverted.

## Related

- [0097](0097-the-class-fill-trait-names-the-size-pair.md), [0099](0099-the-class-fill-trait-names-the-size-pair.md)
  — the class names the pair of the void's sides a board takes.
- [0105](0105-the-stance-table-fixes-the-boards-axes.md) — the wall's stance, whose thickness axis is the
  depth the front is placed along.
- [0175](0175-the-sight-is-view-state-and-the-plan-still-stops.md) — a hidden part is sight, not material.
- [0239](0239-the-cell-is-the-object-and-the-build-is-the-parts.md) — the card rows a creation key lives in.
- [`crates/model/src/project/fill.rs`](../../crates/model/src/project/fill.rs) — `FillMount`, the overlay plan.
- [`crates/model/src/panel_features.rs`](../../crates/model/src/panel_features.rs) — `PanelTrims`.
- [`crates/project/src/lib.rs`](../../crates/project/src/lib.rs) — the scene's `settings` section.
