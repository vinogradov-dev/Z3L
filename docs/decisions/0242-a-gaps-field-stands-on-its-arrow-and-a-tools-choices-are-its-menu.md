# 0242. A gap's field stands on its arrow, and an armed tool's choices are its own menu

- Status: Accepted; supersedes 0119's placement for a measured gap, and its choice rows; extends 0161 to every measured gap
- Date: 2026-11-24

## Context

Two surfaces grew around one idea — the tool that asks for a number draws it at the pointer — and by
now both are in the wrong place.

The niche fill's card carried four choices (class, build, spread, mounting) and two gap fields with
the class words, a «Панель» title and the words «Нижний зазор»/«Верхний зазор» over two numbers. The
card is anchored on the gesture's first frame and cleared the moment it vanishes, so it re-appears at
the next pointer position instead of standing still: the owner reported that the panel *runs away from
the cursor* — a maker aiming at «Полка» loses the row before the hand arrives.

The two gaps are distances between the board's face and the wall that stops it, and they were drawn as
a 1 pt line in the dim text ink with no heads at all, while the same measurement on a carried piece was
an amber dimension with two heads (ADR 0161). The owner's words: *я этих линий не вижу*.

The owner decided all of it on 2026-11-24:

1. the fill's choices move into the viewport's context menu, and «Одна доска» disappears — the spread
   is one «Равномерно» switch;
2. the gap numbers move out of the card and onto the arrows that measure them, without labels and
   without units, and the arrows get the amber ink and the width of a dimension;
3. the same grammar is used everywhere a distance to an obstacle is typed: the fill's two gaps, a
   row's two outer gaps, and both layouts of the furniture move.

## Decision

**A measured gap is one field in the middle of its own arrow; an armed tool's choices are the
operation's context menu.**

- **One painter.** `Document::paint_dimension` draws every measured gap: the run starts on the part's
  own plane, carries a **tick** there and **one solid head** on the mass that stops it, in the guide's
  amber (`theme::helper`, ADR 0111) at 3.5 pt, dimmed by one tone while the hand is elsewhere. One
  head, not two: the run says which way it is measured by pointing at the thing on the other end. The
  fill's hairline and the carry's double-headed dimension are both gone.
- **A plate is a number and nothing else.** Each gap's field sits in the middle of its run: no title,
  no label word, no unit. The unit stays in the inspector, where the same number is written beside its
  word. The plate is narrower than an inspector column so it hugs its number, and its `TextEdit` is
  still the shared field of ADR 0092.
- **The count is not a distance.** A row's count rides beside the hand, on a plate with a `×` and no
  run, because a number that says *how many* does not belong on a dimension. That plate is the one
  plate that senses the hand: it stands beside the pointer and on nothing, so a press on it is a press
  on the number.
- **The arrow's direction is the part's.** Both of the move's gaps start on the piece's own faces and
  point at their walls, carried or held: the held drag's card is gone with the fill's.
- **A step of the row is read, not typed.** The pitch between two boards is a thin coaxial run in the
  dim ink with no field at all: it is derived from the two outer numbers and the count.
- **The row is live.** «Равномерно» needs no anchoring click (the owner, 2026-11-24): as soon as the
  pointer names a void the boards are already spread through it — the free length is shared between
  the boards and the two ends alike, so the air at the near wall is the air between the boards — and
  one click writes them as one batch. A typed far gap keeps its own distance and the neighbours
  share the rest.
- **The hand names the count, and the near half is calm.** The count is how far the hand travelled
  past the first board towards the far wall, as a share of the space it had left, measured in the
  void's own **screen** extent. Anywhere in the near half the row is one board — the board that
  splits the void in two — and only past it does the count move, one board at a time, until the far
  wall gives every board that fits (the owner, 2026-11-24, correcting the first law, which grew from
  the first pixel and so could never be left at one). A maker judges the row by what the window
  shows, so a narrow void in a far corner asks for the same travel as a wide one nearby, and a count
  in force needs a whole step of travel before it moves, so a twitch of the mouse changes nothing.
- **A row's distances are whole millimetres and it keeps one pitch.** A maker reads a dimension, not
  a fraction: the near air, the gaps between the boards and the far air are whole millimetres, and
  the boards stand one pitch apart or it is not a row. The leftover of the division therefore falls
  in the last space nothing pins — the far air, unless a field pinned it, and then the near air. A row
  with both ends pinned gives the count way when the boards no longer fit between them.
- **The pitch is a reading.** The distance between two neighbours is drawn as a thin run in the quiet
  ink with its number on it, coaxially with the two airs, and owns no field: it is what the count and
  the two airs add up to. The two airs are the numbers a maker types; the row's ends are two
  independent numbers of one gesture, so pinning one never kills the other.
- **A zero gives a field back.** A count typed `0` returns the number to the hand, and the same rule
  already releases a pinned gap face (ADR 0092, ADR 0126): a zero in any of these fields means *let
  the pointer decide*, never *set it to zero*.
- **The operation's menu.** A right press while the fill is armed opens the popup with the fill's own
  rows and no row of the node: the three classes (the tool's words — post, shelf, wall — not the class
  registry's), the two builds, the «Равномерно» switch and, for a wall alone, the mounting. The tree's
  menu is untouched and still describes the node, whatever tool is armed.
- **Marked rows.** `node_actions::Entry` gains a `Mark`: a tick in a column left of the label for the
  alternative in force, an empty or ticked box for a switch. Rows are drawn rather than handed to a
  button, so the labels of a section share one left edge and a mark never moves its own words. The
  popup keeps a gutter on the right so a long word never reads as cropped.
- **The armed fill no longer swallows the right press.** The consumed click that keeps a pick off the
  left button used to close the secondary one too, which is why the menu could not open at all.
- The tool card keeps sizes — a pull's height, a radius — and the inspector keeps its two gap rows as
  the typed path when the arrows have left the screen.

## Consequences

- The maker reads a gap where it is: the number is inside its run, the run says which side it
  measures and which way it points, and nothing floats over the pointer. The card's three buttons and
  its «Панель» title are gone from the niche gesture, and the popup that replaces them stands still.
- A choice needs a right press, so the class is changed by one gesture instead of a click at the
  pointer. The fill's tool keeps its choice for the next click, so a maker who builds four shelves
  right-clicks once.
- The card at the pointer survives only for numbers that describe a part (a block's height, a pull's
  rise, a cone's top radius). A future tool that asks for a distance to a neighbour starts on this
  grammar instead of inventing a card.
- The popup is the operation's while the fill is armed, so the node's own rows are one Escape away in
  the tree.
- Revisit if a measured gap must show a witness line, an offset or the arrow of a datum: that is the
  drawing surface's grammar (the `drafting` skill), not this overlay's.
