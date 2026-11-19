# 0178. A piece the document holds inside another may only come out

- Status: Accepted; amends [0162](0162-a-placement-may-not-put-a-part-inside-another.md) and
  [0166](0166-the-move-answers-the-place-the-piece-lands.md)
- Date: 2026-11-19

## Context

[0162](0162-a-placement-may-not-put-a-part-inside-another.md) refuses a placement that brings a part
into an overlap it did not have, and exempts a pair that already overlaps so a file written before the
rule opens and its maker repairs it by hand. The exemption was unconditional: such a piece could be
carried out of the other one as easily as further in, and the owner asked for the second to stop —
"выбрал объект — он выезжает", the object move on such a piece brings it out.

The depth of a pair exists in the document as one reading already: the drag and the write both ask
whether two boxes overlap, and a slide along a face is not that. The question was which number says
"deeper".

## Decision

**The place the part stands in is the deepest one it may keep.** A pair the document already holds
overlapping is exempt only while the overlap does not grow, so every placement deeper than the one the
part occupies is refused and every way out is granted. Both readers use one rule: the drag's
`MoveRule::blocked_at` and the write's `resting_inside`, so a preview never shows a place the release
refuses and a typed number reaches the same sentence.

**The measure is the sunk depth, not the shared volume.** `SurfaceBox::depth` is the least overlap over
every axis that could separate the pair — the travel needed to push them apart. It grows exactly when a
body is pressed further into another, it answers zero for a pair that only touches, and it does not
move when a board is slid along the face it lies in, because that board's thinnest overlap is its own
thickness whichever way it slides. The shared volume was measured and rejected: a turn of an already
overlapping part grows the volume of a world box without sinking anything, so a rotation would have
been refused, and the sample scene's coplanar shelf and drawer front — a pair no rule may freeze —
loses volume under a slide nobody calls "coming out".

**A legitimate nesting is not that pair.** A piece inside its own case, and a unit inside another, keep
the containment rule of [0102](0102-a-furniture-entity-is-a-surface.md): the destination is exempt while it
stays inside, and a shell that straddles its holder's wall is still cut at that wall. Only a pair that
overlaps without nesting is measured by depth.

**A deeper destination is refused, not trimmed.** The rule names the part it will not go into and the
axis keeps the piece where it stands; it does not compute a "half as deep" place, because the depth a
pair is sunk to is not a value the maker picked and a drag cannot stop at it. The log's `blocked=` names
the body the piece is leaving.

## Consequences

- The reported behaviour is gone: a part left inside another by a hand edit or a typed coordinate can
  no longer be pushed further in, in any view, by a drag or by the inspector.
- A drag out of an overlap is granted as one whole destination, so a piece comes out in a frame instead
  of being trimmed to a face the rule could not name.
- A board that already lies in another board keeps every placement along the shared plane, which is
  what the sample scene needs and what a maker does when they slide a back panel behind a shelf.
- The measure is the world box's: a turned body is still measured by the box around it here, as it is
  for the contact reading of a furniture piece ([0165](0165-the-move-tool-is-the-object-move-and-a-solid-body-is-measured-by-its-faces.md)
  narrows only the contact). X85 replaces both readings with a contact port that answers penetration
  depth on a body's own surface; the rule and its one exemption stay as they are.
