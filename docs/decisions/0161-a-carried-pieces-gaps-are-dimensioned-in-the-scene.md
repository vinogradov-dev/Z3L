# 0161. A carried piece's gaps are dimensioned in the scene

- Status: Accepted; amends 0119's placement for the furniture move, refines 0103; extended to every measured gap by 0242
- Date: 2026-11-19

## Context

The two exact-distance fields of a drag have always been a card beside the pointer (ADR 0119), and
they were the first instance of that card's anatomy. For a held handle that reads: the hand is on the
handle and the number sits beside it.

The furniture move holds no button (ADR 0151), so the card follows a hand that is carrying the piece
through the scene. The owner reported three consequences of that one placement:

- the card covers the pointer, which is the very thing the gesture is steering;
- it drifts away from the gaps it measures after a camera gesture moved the view under the piece
  (ADR 0156), so a number and the run it names end up a room apart;
- it lags while a number is typed, because it is a plate drawn from a measurement that is read one
  frame late.

What was missing is where the number belongs. A gap has two witnesses — the piece's face and the
surface that stops it — and the number is the distance between them, not a fact about the pointer.

## Decision

**A carried piece's two gaps are drawn where they are measured: dimensioned in the scene.**

- One arrow per side runs from the piece's face along its free axis to the surface the model probe
  answered — the neighbour's face or the frame's envelope — with a solid head at each end, in the
  guide's own amber (`theme::helper`, ADR 0111). The grammar is the drafting skill's; the heads are
  the terminator and the line is the dimension line.
- The gap's own field sits in the **middle of that run**: one plate per side, the shared numeric field
  with its unit outside and nothing else — no title, no label word, no button pair. Two plates are two
  ids, so each gap keeps its own keyboard focus as the plate travels with the piece.
- The plates take the **keyboard and never the pointer**. A carried piece holds no button, so a press
  anywhere in the work area is the click that puts it down; a plate that sensed the hand would take
  that click and scrub a number into the gap the piece would then jump to (X66). They are chrome for
  nothing, so the shell reports no card rectangle while a piece is in hand.
- The run is drawn thin, and dimmed by one tone of the same ink while the pointer is elsewhere: two
  numbers never shout over the model.
- An axis that points at the eye has no run on screen. Then the scene draws neither the arrow nor its
  field, and the numbers live in the inspector alone.
- The **inspector draws the same two rows** from the same `Distances` (ADR 0119), for the carried piece
  and for a held drag alike: it carries the words, the lock glyph, the refusal and «По центру» that a
  plate cannot.
- A **held** handle keeps its card beside the pointer. The hand is on the handle there, so a plate
  beside it is a deliberate target and the fields may sense both the pointer and the keyboard.
- A box, a pull and a niche keep their cards unchanged.

## Consequences

- The number is where the maker is looking, and it cannot drift from the run it measures: both are
  projected from the same two world points every frame.
- The furniture move draws no card, which closes X66's original problem instead of routing around it:
  nothing floats over the pointer and nothing has to be pulled back into the work area.
- The two tools that share one measurement now read differently on purpose — an arrow with a number in
  it while the hand carries, a plate while the hand holds — and the inspector is the surface common to
  both.
- A plate is a field in the scene, so it is not a place a capability is named: it draws its number and
  nothing else, and the words stay in the inspector.
- A very short gap draws the plate over its own heads. That is accepted: the number is what the maker
  is setting, and a run under about 16 points cannot hold two heads anyway.
- Revisit if a dimension in the scene needs a witness line or an offset from the contour: that is the
  drawing surface's grammar, not this overlay's.
