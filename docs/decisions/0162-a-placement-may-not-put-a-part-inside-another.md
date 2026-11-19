# 0162. A placement may not put a part inside another part

- Status: Accepted; carries ADR 0102's two contact rules from one gesture into the document, and
  narrows ADR 0147's free move and the crossing clause of X65. Its "the drag stops at the world
  box" clause is narrowed by
  [0165](0165-the-move-tool-is-the-object-move-and-a-solid-body-is-measured-by-its-faces.md)
- Date: 2026-11-20

## Context

ADR 0102 states two rules as rules: bodies do not intersect bodies, and shells do not cross shells,
while a shell may contain another. They were enforced in exactly one place — the furniture move,
which stopped a drag at contact. Every other way a placement reaches the document wrote whatever it
was handed:

- the free `Move` applies no rule at all, which ADR 0147 chose on purpose so a loose body keeps the
  pointer's own distance;
- `node.placement.set` and `node.placement.set_many` validate that the nodes exist and nothing about
  where they land, so a typed distance in the inspector, a palette command, a future assistant or a
  script could put a board inside another board;
- `Project::move_rule` answered `None` for a selection that is not furniture, so the furniture move
  of a loose board carried it through anything as well.

The owner's observation is the report: dragging a panel, it sank into an existing panel. The
gesture's own rule lived beside the write instead of in it, and a rule that lives in one caller is
not a rule.

## Decision

**The document answers the question and the write obeys the answer.** `Project::resting_conflict`
takes the placements a caller asks for — the nodes and the placement each receives — measures the
boxes the tree would hold afterwards (`Project::world_placement_with`, so a batch that names a
container and one of its children is measured as the tree would hold it), and names the first part a
moved box would come to rest inside. `node.placement.set` and `node.placement.set_many` ask it before
they write and refuse with a sentence naming both parts; nothing is written, so the revision does not
move and the history gains no entry. `node.placement.world_zero` goes through the same command path
and is refused the same way.

**A pair that already overlaps is the maker's to repair.** The refusal is `now` overlaps and `was`
did not: a file written before this rule opens, its overlapping parts stay draggable, and the maker
pulls one out by hand. A touch is never a refusal, because a pair that shares a face shares no
volume.

**The test is exact, not the world box.** A turned board's world box is larger than the board
(ADR 0102), so a *refusal* may not be read from it: the fill's shaped shelf is the section between
two leaned posts, its face lies exactly on a post's face, and the two world boxes overlap deeply.
`SurfaceBox::meets` is the separating-axis test over the two boxes' own axes and the nine cross
products of their edges, in the document's own unit: a shared volume thinner than one micrometre on
some axis is rounding and contact. The move rule keeps the world box, because a conservative stop
costs the maker a millimetre while a wrong refusal costs them the command.

**Contact is the document's, so both moves stop at a neighbour.** The free `Move` reads
`Project::contact_rule`: every axis, no snapping, no grid — the pointer's own micrometre distance,
exactly as ADR 0147 says — and the same contact cut. ADR 0147's "no contact" clause is narrowed to
what it was about: the furniture *snapping*. The furniture move reads `Project::move_rule`, which now
answers wherever a piece can be measured: for a selection no product holds it carries contact and no
snapping at all, so a loose board stops at its neighbour and still keeps the pointer's distance. What
neither tool does is reach a refusal: a drag is cut at the contact the write would refuse, so the
piece stops at the face.

**A created part is not refused by this test.** `panel.create` was the one write path left out, and
deliberately: a part that does not exist yet has no `was`, so every overlap would be a new one, and a
created part's box is the rectangle around an outline that may be a trapezoid or a beveled ring — the
fill's own flush fit in a splayed case would be refused as an overlap. The fill measures its void
analytically against the true faces (ADR 0106, ADR 0103) and is the safer guarantee; a hand-made
`panel.create` is a creation, and the placement the maker then edits is guarded like any other.

**Hidden is a matter of sight, not of material.** The guard reads the document: a part switched off in
the tree is as much in the way as a visible one, because what a hidden part loses is its picture and
its pointer, never its body (X73 makes the tools agree).

## Consequences

- The rule a maker meets in a drag and the rule the document keeps are one function
  (`resting_inside`), so a piece cannot stop where the write refuses, and the sentence that names a
  part in the way says what the drag had already shown.
- A **rotation** that lands a board inside another is refused too, because a rotation is a placement
  write: a shelf turned inside a case whose sides it sweeps is stopped with a sentence. That is honest
  — a real 700 mm shelf cannot turn inside a 1000 mm case — and the rotate gizmo does not yet read the
  same answer first, so it reaches the refusal instead of stopping at it. Filed as debt, not built
  here.
- A typed distance is the maker's own number and outranks the rule (ADR 0103): a number that lands
  the part inside another is refused on release and the part stays where it was, rather than being
  silently clipped to something the maker did not ask for.
- The furniture move and the free move differ in one thing and no longer in two: the snapping and the
  axes are the tools', contact is the document's.
- The cost is one tree walk per placement write. A drag commits once on release, so the frame path
  pays nothing; the pairs are filtered by the world box before the exact test runs.
- X65's `Done` said "the free move still crosses it"; that clause no longer holds and is amended to
  say both moves stop flush against the neighbour.
- A rotated body measured against a rotated body still uses the world box in the *drag*; a drag of a
  board turned off the world axes therefore stops a little early. Refusing writes would not tolerate
  that error, which is why the two readings exist and why the write's is the exact one.

## References

- `crates/model/src/project/move_query.rs` — `Project::resting_conflict`, `Project::contact_rule`,
  `Project::move_rule`, `MoveRule::limit`, `resting_inside`, `blocking`, `Resting`.
- `crates/model/src/project/surface.rs` — `SurfaceBox::meets`, `SurfaceBox::shared`, `CONTACT_MM`.
- `crates/model/src/project/query.rs` — `Project::world_placement_with`.
- `crates/engine/src/node_cmds.rs` — `refuse_resting`, the two placement commands.
- `crates/engine/src/commands.rs` — `CommandError::RestingInTheWay`.
- `app/src/translate_gizmo.rs` — the rule each tool reads at the press.
- ADR [0102](0102-a-furniture-entity-is-a-surface.md) — the two rules this record carries into the
  document.
- ADR [0147](0147-the-furniture-move-is-its-own-tool-and-a-panel-slides-along-its-thickness.md) — the
  free move's "no rule at all", narrowed here to no *snapping*.
- ADR [0103](0103-one-probe-serves-the-niche-the-distances-and-the-move.md),
  [0146](0146-snapping-is-a-workspace-setting-and-the-coordinate-planes-stand-in-for-walls.md) — the
  typed distance and the snapping, both unchanged.
- Plan: X72, whose note (`plan/notes/X72.md`) records what was measured and ruled out.
