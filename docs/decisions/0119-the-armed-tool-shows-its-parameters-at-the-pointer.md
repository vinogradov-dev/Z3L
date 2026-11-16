# 0119. The armed tool shows its parameters at the pointer, and the inspector shows the same fields

- Status: Accepted
- Date: 2026-11-14

## Context

A tool that asks for a number — a fill distance, a box side, an extrusion height, a polygon's sides —
puts it either next to the hand that is deciding or at the far edge of the window. The owner asked
for both ends of one rule: the number is at the cursor, and the full data stays in the inspector.
Today the previewing operations already fill a session overlay and the exact-distance fields already
sit at the pointer (0092, 0093, 0103), but a tool's other parameters have no home: a side count is
typed into the ribbon's typed defaults, a panel's size into the inspector, and a new tool invents a
third place.

## Decision

**One tool card at the pointer.** An armed tool that has parameters draws one small surface near the
pointer holding exactly those parameters, one field per value, in the shared numeric field (0092)
with its unit outside the box. It appears with the arm and disappears with the commit or the cancel;
it takes no click from the scene beyond its own rectangle, closes on Escape and is declared to
`Chrome::covers` as one row of `HOST_OWNERS`.

**The inspector shows the same fields, not a copy.** For the same armed tool the right panel shows the
same values with room for the full data around them — the envelope, the material, the class. Both
surfaces read one draft and commit through one command; neither recomputes a value the other derives,
and a field's edit scope is the same one undo entry (0092).

**The number the gesture reads is the number the field shows.** Typing in the card pins the value the
pointer was driving and the preview follows the typed number immediately; clearing the field returns
the value to the pointer.

**No dialog.** A parameter the person may change while the model is visible is never a modal window,
and a tool with one primary action does not grow a second one in its card.

**The card is a number, not a dialog.** A tool whose value the hand is already committing in the scene
draws that value alone: a symbol in front of the number (`H`), no unit repeating what the symbol says
and no button pair, because the gesture's own Enter and Escape are the commit and the cancel. The
sketch plate's rule — a symbol names the quantity, so the plate carries no unit — reaches the card
here, and the field is the only control that takes the keyboard (U104).

## Consequences

- A new tool declares its parameters once and gets both surfaces; a tool that shows nothing at the
  pointer must say in its note why the parameter belongs to the inspector alone.
- The card is a surface, so it adds a `HOST_OWNERS` row and a rectangle test; a press inside it is
  chrome and never a pick or a camera gesture (0080).
- The ribbon keeps only the persistent typed defaults that outlive a gesture; a per-tool value lives
  in the card and the inspector.
- The exact-distance fields become the first instance of the card's anatomy rather than a private
  overlay, so `distance.rs` and the card share one layout and one commit path.
- A card asks for the keyboard on the two frames its area settles: `egui` builds a widget after it is
  asked for the focus, so the request of the frame a new card appears is lost, and asking longer
  would hold the keys against the person's next click.

## Related

- [0092](0092-a-field-is-a-number-with-its-unit-outside.md) — the one numeric field and its edit scope.
- [0093](0093-a-preview-fills-a-session-overlay.md) — the preview the card drives.
- [0080](0080-the-tool-strips-float-over-the-design-space.md) — a floating surface over the scene.
- [0103](0103-one-probe-serves-the-niche-the-distances-and-the-move.md) — the fields this generalises.
- [0117](0117-the-interface-is-one-inventory-of-capabilities.md) — the inventory that names the card
  as a place.
