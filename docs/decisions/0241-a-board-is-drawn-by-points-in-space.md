# 0241. A board is drawn by points in space, and the first three fix its plane

- Status: Accepted; refines 0114 (a board is no longer drawn only on a pre-chosen work plane) and
  0238 (the plane's role is what a by-points board carries)
- Date: 2026-11-25

## Context

A board was born in one of two ways: a niche measured by the empty-space tool (ADR 0099), or a ring
drawn on a work plane (`furniture.panels.contour`, ADR 0114). Both choose a plane before the first
click. A board lying on a leaned face, or between points taken on different faces, could not be drawn
at all: the work plane is picked first, and a maker who wants the board's plane to follow the part
has to orbit the view to the right plane and hope the click lands where the board is.

The owner's ask: a point tool that names points in space, the first three fixing the plane, as many
points as the maker wants, the ring finished by a checkbox; refined to two methods, so a board that is
a plain rectangle is one gesture and a board that is a ring is another.

## Decision

**One cell, two methods.** `furniture.panels.points` sits in the Panels group beside the Panel and
the Panel-from-contour cells. Its own method is **three points**: the first two are one edge, the
third fixes the width — its distance from that edge, measured square to it in the plane — so the
fourth corner is computed, drawn as a ghost, and the board commits on the third click. Its second
method is **any number of points**: the first point starts the ring, the second gives its direction,
the third the plane, every later point is projected onto that plane, and the card's finish (or Enter)
closes the ring as it stands.

**The points are the snap resolver's, never a work plane.** Each click names a place through the one
3D resolver — the probe's face, or the snap resolver's edge, corner, axis and grid — so the first
click may stand anywhere on the part and the plane is what the first three points span
(ADR 0103, 0051, 0118). Three points on one line are refused with a sentence and the point is not
kept: a plane guessed from the first two would put the board where nobody showed it.

**The rectangle is a standard panel; the ring is a contour.** Three points store the nominal
rectangle the board is — the only way this program makes a standard board out of a plane it was given
— and a ring stores its outline, both anchored at their base corner by the model (ADR 0042, 0083).
One commit is one `panel.create` carrying the ring and the class, so the click chain is one undo
entry.

**The class is the plane's role, and the card's class row overrides it.** The tool has no niche to
name a class, so the plane's own normal decides (ADR 0238). The card asks the class before the click —
`From the plane`, Post, Shelf, Wall — and a chosen role travels in the same `panel.create`; nothing
rewrites a created part's class, which is 0239's rule.

**The finish is the card's pair, not a modifier.** A count above four reaches the document through the
finish alone: no click, no double click and no modifier commits a long ring.

## Ruled out

- **A modifier** — Enter for four corners, Shift+Enter for three. The same three clicks would mean two
  things and one meaning would hide behind a key.
- **One tool with an optional finish on the third point.** The maker would have to guess which commit
  their next click is in.
- **A second toolbar cell per method.** A method row says it out loud, and the cell's pinned method
  remembers the choice (U74).
- **Committing the ring on a click on its first point.** For three or four points it would work and for
  five it would silently not; one finish for every count is the rule a maker can rely on, and the card
  draws it.
- **A new command for the by-points board.** `panel.create` already takes an outline, a class and a
  placement, so the tool has nothing the command lacks.

## Consequences

- A board can be drawn across several faces: only the first three points fix the plane, and a later
  click is measured where it projects, which is what a ring across a leaned side needs.
- The work plane keeps its role for the sketch and the contour cell; the by-points tool ignores it.
- A rectangle drawn by three points is editable as a standard panel — the nominal sides, the corner
  grips and the resize — where the contour cell's board is a ring (ADR 0042).
- A ring that crosses itself is judged at the finish, not at the click that closes it in the maker's
  head, because a chain still being drawn may pass itself on the way; the refusal keeps the points.
- The gesture adds no document field: the points, the class and the refusal live in the editor, and
  the one command is the one that stores anything.

## Related

- [0114](0114-a-panel-is-cut-from-a-ring.md) — the contour cell this one stands beside.
- [0238](0238-a-panels-role-is-the-plane-it-lies-in.md) — `role_of_plane`, the class a by-points board carries.
- [0239](0239-the-cell-is-the-object-and-the-build-is-the-parts.md) — the class row, and a created part keeps its role.
- [0118](0118-the-niche-is-an-atom.md) — the one probe and the one snap resolver the points come from.
- [`crates/model/src/panel/points.rs`](../../crates/model/src/panel/points.rs) — the plane the points span.
