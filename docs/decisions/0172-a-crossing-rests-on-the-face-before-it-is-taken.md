# 0172. A crossing rests on the face before it is taken

- Status: Accepted; shapes how
  [0166](0166-the-move-answers-the-place-the-piece-lands.md) is presented to the hand
- Date: 2026-11-19

## Context

0166 answers the place the piece lands, so a piece whose whole body fits beyond a neighbour leaps
there. The shell held that leap for 0.12 s by comparing the answer's step with the hand's own step
([0167](0167-a-jump-is-held-for-a-moment-before-the-carry-takes-it.md)) — and that comparison fails
for the hand this product is built for. The third session's log has single frames in which the pointer
travelled 500 to 1000 mm: the answer leaps a whole body's thickness, the hand travelled as far, and
the hold never fires. No frame ever shows the piece resting on the face.

The owner's report names the mechanism he wants: "уперся и дождался, когда курсор пересечёт толщину
вертикального блока" — the piece should stop at the face and wait for the cursor to come out on the
far side, and the behaviour must read the same in both directions.

## Decision

**A crossing is detected geometrically, not by comparing steps.** `MoveRule::crossing` asks whether the
straight path from where the piece stands to the place the hand names passes through a body, using the
same swept face test the axis drag uses. A place the rule grants whole beyond such a body is held: the
piece stays at its previous delta (the face) for `JUMP_HOLD`, however fast the hand moved. An answer
the rule cuts is already the face and is never held.

**The hand gates the crossing.** `MoveRule::enclosing` names the body the hand's own point stands
inside; while it names one, the hold keeps its age at zero, so the wait is the cursor crossing the
body's thickness and not a timer that runs out underneath it. The cursor coming out on the far side
starts the 0.12 s that takes the place. A product's envelope does not gate — work happens inside
cases — only a panel or an object does.

Both questions are asked of the same rule that answers the drag, so the log, the preview and the
release cannot disagree.

## Consequences

- A piece no longer passes a body without being seen on its face, at any pointer speed, and the same
  hold applies whether the free place is beyond the body in X or reached by clearing it in Y.
- A long diagonal carry pauses once per body it passes. That pause *is* the contact being read.
- The rule gains two pure queries; the shell gains one boolean and one id per carried frame.
- The hold still never applies to a typed distance: the maker's own number is not a crossing.
