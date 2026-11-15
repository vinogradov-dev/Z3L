# 0126. The point being placed shows its numbers at the cursor, and one of them holds the keys

- Status: Accepted
- Date: 2026-11-15

## Context

A sketch tool asked for no number at all: `draw_gizmo.rs` painted one `size_label` string under the
pointer and the commit was a click at a snapped point. A line to 600 mm at 30° was therefore drawn by
eye, and the angle had no guide at all. Meanwhile the contour editor had already grown a private
numeric entry beside a grip (`NumericEntry` in `contour_gizmo.rs`), which is the second copy of a
mechanism ADR 0118 says must be an atom. The owner's reference (OpenCAD Studio) shows the number on
the segment it measures, a second number outside the arc that measures the angle, and a field that is
always ready for typing.

A pointed number cannot live in the armed tool's card (ADR 0119): the card is one plate at one anchor,
while a point's length sits on the segment and its angle on the arc, each beside the thing it
measures. Nor can a plate be a text box that holds the caret all the time: a caret in a number under
the pointer would eat the click that places the point.

## Decision

**One field set owns the numbers of the point being placed, one plate per parameter at the anchor of
the thing it measures.**

- `crates/ui/src/point_fields.rs` holds `FieldSet`: the rows, the focus, the typed locks and the keys.
  A row is the parameter, its label or symbol, its unit, its range, its decimals and its screen
  anchor; the tool computes the anchor from its own geometry.
- Every plate shows the live value the pointer produces. A typed number locks its parameter and
  `set_live` never overwrites it; a typed `0` gives the parameter back to the pointer. Tab moves the
  keyboard, Enter places the point with every plate, and Escape drops the locks first and cancels the
  tool second, which is the order the person expects from a CAD field.
- The plate is the shared scrub of ADR 0092 with its unit outside the box, drawn in the armed tool
  card's frame, and nothing here writes a document: the draft is view state, so the field mints no
  edit scope and returns its numbers to the caller.
- The plate floats under the chrome (ADR 0123), clears its anchor diagonally and leaves the view
  through the side with more room, so it never covers the point or the snap marker it belongs to.
- While a plate holds the keyboard the tool's own key reader stands aside, because egui reports the
  text field as focused; the atom surrenders the focus on Escape so the next one is the tool's.

## Consequences

- The contour editor's private numeric entry becomes a call site of the atom, and every drawing tool
  gains its plates by declaring rows rather than by growing a field of its own (C2).
- A number typed at a plate is a promise for one point: the locks reset when the point is placed, so
  the next point starts from the pointer again.
- The atom is the only owner of the focus order and of the typed lock; a tool that wants another
  number adds a `FieldParam`, not a key handler.
- The keyboard of an operation is now split by ownership rather than by widget: the tool reads its
  own keys when no plate is focused, and the atom reads them while one is. Two readers in one frame
  is the defect this record exists to keep out, so a new tool with fields takes the plate path.

## References

- `crates/ui/src/point_fields.rs` — the atom, its grammar and its `RawInput` tests.
- ADR [0092](0092-a-field-is-a-number-with-its-unit-outside.md) — the one numeric field; ADR [0119](0119-the-armed-tool-shows-its-parameters-at-the-pointer.md)
  — the armed tool's card, which stays the multi-row surface.
- ADR [0118](0118-a-mechanism-used-twice-is-a-tool.md) — why a third copy is not allowed.
- Plan: C1.
