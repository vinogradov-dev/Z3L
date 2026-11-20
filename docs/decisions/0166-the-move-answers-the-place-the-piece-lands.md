# 0166. The move answers the place the piece lands, not the path it crosses

- Status: Accepted; supersedes the swept stop of
  [0165](0165-the-move-tool-is-the-object-move-and-a-solid-body-is-measured-by-its-faces.md) and the
  contact reading of
  [0163](0163-the-furniture-move-carries-on-every-allowed-axis.md), and narrows the "a shell may
  contain another" clause of [0102](0102-a-furniture-entity-is-a-surface.md)
- Date: 2026-11-19

## Context

The owner furnished a room and reported three symptoms with screenshots of one scene: two plain
solids side by side could not be carried into the empty place beyond each other; a post could not be
carried past a shelf although the shelf ends; and in perspective and axonometry solids and the
carcass passed through one another.

They were three faces of one decision. Since ADR 0162 the rule has answered *the path*: it measured
the first contact along the drag from the start boxes and cut the travel there. A place beyond an
obstacle is then unreachable by construction, however far the pointer goes — the piece can only ever
rest against the face it meets. The per-axis answers were each exact and still combined into a place
inside a turned neighbour, which is how a diagonal carry in perspective cut a corner: each axis was
measured with the other two at zero. And a shell and a body were skipped as a pair, because a shelf
in a case is a body in a shell, so a carcass and a solid body ignored each other in the drag and in
the write's guard alike.

## Decision

**The rule answers the destination.** The whole delta the hand names is tested once: when the place
it lands is free, the piece goes there, passing whatever stood between it and the place — the hand
jumped over it, and the maker sees the piece where the pointer is. When the place is not free, the
axes are resolved in world-axis order, each one decided **where the axes before it put the piece**:
an axis whose destination is free is taken whole, and one whose destination is not is cut at the
first face in its way from there. An axis whose cut cannot clear the pair keeps the piece where it
stood rather than preview a placement the release would refuse. The three answers are therefore one
placement, tested as one, and no combination of them can land a piece inside a turned neighbour.

**A carcass is a body to everything that is not its own piece.** A body stops at a body, a shell at a
shell, and a shell at a solid body. A legitimate nesting is exempt and only that: a node that is a
child of the case whose shell it is inside is a piece in its case, and a shell wholly inside another
is a nested unit. Everything else is measured, so a cabinet no longer passes through a solid and a
solid is no longer pushed into a cabinet's envelope. A pair the document already holds overlapping is
still the maker's to repair, exactly as the write's guard reads it (ADR 0162).

**The guard reads the same pair rule.** `resting_conflict` asks the nesting question too, so a shelf
inside its own case stays legal, and a body that ends halfway through a carcass is refused with a
sentence.

**The per-axis sweep survives only as the clamp.** `blocking_faces` still answers the distance to the
touch, and it now runs from where the resolved axes put the piece; the destination test above it is
what decides whether the piece may travel at all.

## Consequences

- A panel can be set down behind a panel, a body behind a body, a post past a shelf: the free place is
  reachable, which is what furnishing a room needs.
- A diagonal carry no longer cuts a corner, and a drag whose destination is inside a neighbour stops
  flush on its face — the two behaviours the same rule gives, and the reason the rule is stated about
  the destination rather than the path.
- The preview jumps when the hand crosses an obstacle: continuity of travel is given up on purpose.
  A thin wall can be passed through in one gesture, because the place beyond it is free; if that ever
  reads wrong, the answer is a swept test on top of the destination test, not a return to the path.
- A shell no longer hides a body: a solid body standing inside a cabinet's envelope is a body in the
  way, not a piece in a case, and the maker will feel it as a new collision in scenes where a lamp
  stood inside a cabinet's footprint. That is the point of the report.
- A piece in a case can be carried out of the case when the place it lands is clear. The containment
  rule no longer fights the hand; it only refuses a placement that ends halfway through a wall.
- A typed distance still bypasses the rule, as ADR 0103 says it does, and a typed value that lands
  inside a neighbour is refused by the write with a sentence and no undo entry.
