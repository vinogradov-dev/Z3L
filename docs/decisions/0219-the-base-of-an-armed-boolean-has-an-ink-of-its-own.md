# 0219. The base of an armed boolean has an ink of its own

- Status: Accepted
- Date: 2026-11-22

## Context

A boolean is a mode, not a button: it arms on a base — the frame its result is born in — and every
other picked shape joins as an operand (ADR 0093, 0194). The picks are the selection, so a plain
click on the second ring replaces the first one's highlight while the first is still an operand, and
a Shift-click makes the last-picked ring the primary and the brightest. Two rings then look like one
selection, and nothing on screen says which of them the subtraction cuts into (X99).

The selection ink cannot answer it. It says "picked", and both roles are picked: the cyan is the
renderer's, drawn on every operand and repeated by the shell as a widened outline over the chain, so
any second meaning it carries would have to be a second weight of one colour — which is the reading
the two rings already fail at.

## Decision

- **The base takes a scene ink of its own**, `SceneColors::boolean_base`, drawn on the base chain by
  the same frame pass that paints the selection cyan on the operands, and widened by the same
  overlay so a one-pixel `LineList` still reads. The base keeps it while the pointer is over it and
  while it is also picked: it is a role of the operation, not a state of the pointer.
- **The ink is rose**, because it is the hue no other role claims: the selection is cyan, a guide and
  the unselected path are amber, a refusal is red, and the grid axes are red and green. One value
  per variant — `(242, 115, 168)` dark, `(158, 46, 112)` light — 6.33:1 and 5.17:1 against the two
  workspaces, 5.58:1 and 6.42:1 against the panels the tree row prints it on, and 58–93 ΔE from the
  cyan, the guide ink, the path ink and the beech face.
- **The value lives in the frame, the role in the theme.** A chain is drawn by the renderer without a
  `Ui`, so the constant is the frame's, exactly as the selection cyan is; `theme::boolean_base` reads
  it as chrome ink for the scene tree's row, and `BOOLEAN_BASE_CONTRAST` is the floor a test pins on
  both variants of all three presets. This is the arrangement ADR 0111 already gave the guide ink.
- **Three surfaces say one thing.** The chain in the frame, the widened outline over it, and the
  scene tree's row for the same node; the armed mode's card prints the base by its object name under
  the operation's name, so the words and the picture agree.

## Consequences

An armed boolean now costs a re-upload of the frame when it arms, when its base changes and when it
drops, because the arm is view state that the picture follows — the same rule the selection already
had. The scene tree row reads the base ink instead of the row's own text colour, so a base that is
also selected keeps one role rather than two.

The cost is a fourth ink over the model — after the selection cyan, the guide amber and the overlay
pair — and a red line through the palette: a new role must find a hue the others have not taken, and
the rose was picked because the four neighbouring hues were spoken for.

Revisit this if a boolean ever takes more than one target: two bases would need a second mark of
their own, and the argument for a hue no role claims stops being enough.
