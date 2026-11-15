# Dimensions

The rules here are GOST 2.307 and ISO 129-1 in our words, plus the icon grammar that follows them.
Read the arrangement first, then the terminator, then the air; an icon that gets the first two right
still fails if the glyph merges at 26 px.

## The six types

| Type | Arrangement | Terminators | Order |
|---|---|---|---|
| Overall | one dimension line between the two outer edges | arrows at both ends | one size; no duplicate with a chain |
| From base | several dimension lines starting at one shared witness, one per measured point | arrows at both ends of each line | the **largest outermost**, the smallest nearest the object |
| Chain | one row of dimensions sharing witnesses, end to end | arrows at every witness in a drawing; at 26 px only at the outer ends, the middle witness carries the split | consecutive sizes; the overall size is not repeated |
| Angular | an arc at a radius between the two rays | arrows at the rays in a drawing; 45° ticks at 26 px | the value in degrees, never a linear size |
| Radius | a leader from the centre to the arc, at an angle | one arrow at the arc, a thin cross at the centre | the leader is never horizontal or vertical |
| Diameter | one line through the centre, arc to arc | two arrows at the circle | a full circle; no centre cross |

## Rules that are easy to get wrong

- **Extension lines** start 1–2 mm from the contour and pass the dimension line by 2–5 mm. The icon
  uses 1.1 units of air at the contour and 2.5 units past the dimension line, because the icon's
  units are not millimetres.
- **A near point's extension** stops 2–5 mm beyond **its own** dimension line; it must not run on and
  cross the outer dimension line. In a baseline stack the longest line is outermost and its witness
  is the only one that reaches the far end.
- **The base witness is shared** by every dimension in a baseline stack; it is the one that extends
  to the outermost line.
- **A chain** never closes on itself: the overall size is omitted or written as a reference
  `(…)`; the sum of the links is not dimensioned twice.
- **Radius vs diameter**: a radius has one arrow and a centre mark; a diameter passes through the
  centre and has two arrows. Never use a diameter for an arc whose centre is outside the view.
- **Arrow tips touch the extension line in a drawing.** An icon keeps 1.0–1.1 units of air so the
  tip does not merge with the line at 26 px; the drawing rule and the icon rule differ on purpose.
- **Text** is above the line and readable from the bottom or the right; the unit is stated once on
  the sheet, not per size; reference sizes are in parentheses; a theoretical size is framed.
- **No dimension to a hidden edge** where a section can show the same shape; no size twice; no
  dimension inside a hatched area.

## Icon grammar

The measured object and the base are `view` (neutral); the annotation — witnesses, dimension line,
arrows or ticks — is `reference`. A control point is a circle of radius 2.0 with 1.15 units of air,
and the contour is broken by a real path, never by a mask or a background patch.

Common geometry on the 24 grid:

- object line at `y = 18`, `x = 4.5 … 19.5`, neutral, 1.3;
- witnesses at the measured points, from the object to 2.5 units above the topmost dimension line;
- overall and chain dimension line at `y = 12`; baseline at `y = 10` (outer) and `y = 14` (inner);
- arrow: 2.4 long and 2.2 wide, solid; ticks: 1.4 long at 45°;
- arrow tip 1.1 from the witness or the circle; radius centre node radius 2.0, gap 1.15;
- footprint `2.25..21.75`; coverage 10–40 % of the 20 × 20 area.

### Overall

One object line, two witnesses, one dimension line with arrows at both ends. The smallest complete
dimension, and the one that establishes the scale of the sheet.

### From base

The base witness is shared; the short dimension sits nearest the object and the long one above it.
The near point's witness ends 2.5 above the short line, not at the outer line. The reader sees
"one datum, several sizes", and the largest size is the outer one.

### Chain

Three witnesses in a row, one continuous dimension line, arrows at the outer ends. At 26 px four
arrows at the middle merge into a blob, so the middle witness is the split and the outer arrows keep
the direction. In a drawing every witness carries its arrows; an icon may simplify, and the skill
records the simplification so the next session does not call it a mistake.

### Angular

Two neutral rays meeting at the vertex, a yellow arc at a radius, and 45° ticks at the arc ends.
Arrowheads on the arc lose their shape below ~40 px; the ticks are the standard architectural
terminator and survive 26 px.

### Radius

A neutral circle, a neutral centre node, a yellow leader at 45° from the centre to the arc, one
arrow at the arc. The leader is angled because a horizontal radius reads as a diameter bar and
because the drawing convention avoids horizontal and vertical leaders.

### Diameter

A neutral circle, a yellow line through the centre at 45°, two arrows at the arc ends, no centre
node. The absence of the node and the second arrow are what tell it apart from the radius.

## Colour

- Source/base/object/control: `view` — `#B8C2CE` on dark, `#5A6674` on paper.
- Annotation: `reference` — `#E8D44D` on dark, `#7A6A00` on paper.
- No third hue inside a dimension icon; a tone of the same ink carries a passive face.

## Where the standard ends and we decide

The standard fixes the grammar; it does not fix an icon at 24 units. The icon rules — 2.5 units
instead of 2–5 mm, 1.1 units of air instead of a touching arrow, 45° ticks for the angle — are our
decisions and are recorded here so the set stays one family. A drawing surface that departs from
GOST for a shop's practice says so on the sheet and in an ADR.
