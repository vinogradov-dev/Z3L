# 0175. A hidden part is sight, not material: the tools pass it and the part stops at its faces

- Status: Accepted; carries ADR 0065's visibility flag into the model's measurements and keeps
  ADR 0162's rule that the placement guard reads the document
- Date: 2026-11-19

## Context

The owner switched a frame's facades off in the tree, armed the shelf fill and watched the ghost
stick in the phantom facades. The rule he gave with the report: a switched-off part is invisible to
the eye and to the pointer, so a maker reaches and fills the niche behind it, while nothing may pass
*through* the part itself — a hidden part "allows building through itself but not being
intersected".

The flag itself was right. ADR 0065 made visibility session state, and every consumer that projects
the whole tree already asked it: the frame builder, the pick, the light rig and the camera glyphs.
What was missing is the consumers that **measure** the document: `Project::probe` and
`Project::niche` walked the tree straight, so a switched-off facade was a wall of the void, its
plane cut the cell short and a body it stood in front of made the cell "not one convex cell". The
drawing and the measuring disagreed about what was there, and the fill was the tool that showed it.

## Decision

**One sight value, handed in.** `model::Sight` is the nodes the user switched off plus the parent
walk that makes a hidden container hide its subtree — the walk `ViewFlags::hides` made, now in one
place. It borrows the session's set (`Sight::of`), so it costs no copy and no revision, and it is a
parameter of a query, never a field: the document holds no view state (ADR 0065) and the model reads
none. `ViewFlags::sight` produces it; `ViewFlags::hides` is the same walk for the consumers that ask
one id at a time.

**The tools take it, the material does not.**

- `Project::probe`, `Project::niche`, `Project::fill_plan` and `Project::fill_plan_in` take a sight,
  and so do the surfaces built on them: the niche atom (`ui::Void`), the cursor rule
  (`ui::pointer_face`, `pointer_point`, `free_place`) and the ruler (`MeasureGizmo`). A ray passes a
  hidden part; a hidden product is neither the frame the point stands in nor a shell it stops at.
- The **placement guard** keeps reading the document: `Project::resting_conflict` takes no sight,
  because what a hidden part loses is its picture and its pointer, never its body (ADR 0162). A drag
  that would come to rest inside one is refused with the sentence that names it.
- The **drag's own fields** ask `Project::probe` with `Sight::none`: they measure how far the piece
  may travel, which is the material's answer and not the eye's.

**A hidden body in the cell is a boundary, not a slice.** A body the sight switched off no longer
makes the niche "not one convex cell": the cell spans the space the eye reads, and the fill plan
trims the footprint it would otherwise take at every hidden body's own face — on the two axes the
board takes, in the product's frame, with the body's extent along the board's thickness first
checked against the slab the board occupies. The face cut is the one the measured point stands outside of: a door at the front trims the depth, a
post at the side trims the width. Where the point stands inside the body's own footprint — a hidden
shelf under the hand, a facade the ray crossed on its way in — the cut that keeps the most of the
void takes it, because the part need not contain the point: the largest rectangle decides where the
part lands. The cut is a half-plane of the convex footprint, so `Fit` still takes the largest
rectangle and `Shape` still takes the ring, and neither ever holds the body.

The awkward cases are decisions, not accidents:

- a hidden body **that leaves nothing** — one lying across the board's whole slab, or standing
  across the whole void — answers `FillRefusal::TooSmall`: the void has no room left at that place;
- a hidden body that only **reaches the slab's faces** is contact and no cut, in the document's own
  micrometre, exactly as a pair of boxes that share a face shares no volume;
- a hidden **facade that spans the whole opening** leaves whatever the other walls leave, and a void
  with nothing left is `FillRefusal::TooSmall`;
- a hidden part **thinner than the board** is cut in the board's plane, so the part goes around it
  rather than being thinned;
- a hidden **container** takes its whole subtree with it, for the tools as for the frame.

**The cell does not outlive the sight.** A pointer that holds still re-measures nothing (the atom's
own step), but switching a part off changes the void without moving the pointer, so the editor drops
the held cell when the visibility revision moves and the same frame measures the new space.

## Consequences

- A hidden part is now one rule and not three: `Sight` is the answer to "what does the eye pass", the
  guard is the answer to "what does the material allow", and both are asked by every tool that needs
  the one or the other. Showing a part again restores every reading and writes no document field.
- The cost is one extra argument on four model queries and the callers that mean them. A sight with
  nothing hidden answers `false` without walking, so the common case is one comparison.
- The trim reads a **turned** hidden body as the box around it in the product's frame, which cuts a
  little more than it must and never less; a hidden body standing against a leaned wall is therefore
  trimmed conservatively. Filed as debt rather than measured here.
- The trim works in the board's own plane, so the **free axis is not moved**: a hidden board lying at
  the same height as the new shelf cuts the footprint instead of pushing the shelf up, and a part
  that leaves nothing says `TooSmall`. The maker moves the hand.
- A shaped part takes a straight edge at a hidden body's face and no end bevel on it, because the
  bevel is read from the cell's own wall planes and the body is not one of them.
- The move tool is untouched: it already answers the place the piece lands (ADR 0174), so a hidden
  part stops a piece that would rest inside it and never a piece carried past it.

## References

- `crates/model/src/project/sight.rs` — `Sight`, `Sight::hides`, `Sight::is_hidden`.
- `crates/model/src/project/niche.rs` — `Project::probe`, `Project::niche`, `product_containing`,
  `surroundings_around`, `body_surfaces`.
- `crates/model/src/project/fill.rs` — `Project::fill_plan_in`, `hidden_boxes`, `trim_footprint`,
  `clip_half_plane`, `polygon_hits_rect`.
- `crates/ui/src/view_flags.rs` — `ViewFlags::sight`, `ViewFlags::hides`.
- `crates/ui/src/void.rs` — `Void::measure`, `pointer_face`, `free_place`.
- `crates/ui/src/editor/void.rs` — `Editor::void_measure` and the visibility revision.
- `crates/model/src/project/move_query.rs` — `Project::resting_conflict`, which takes no sight.
