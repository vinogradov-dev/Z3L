# 0116. The object card is one reading order, and a fold only hides what is long

- Status: Accepted
- Date: 2026-11-15

## Context

The object card was the screen a person reads most, and the longest thing in the inspector. In
1779 lines it drew eight `CollapsingHeader`s, every one open by default, stacked between the
placement card (two headers), the pivot card (one) and the properties card (one). A mesh object met
three of them, a primitive five, a constructive object six — and the two that half of the kinds met
were spent refusing:

- a primitive and a constructive opened *Mesh editing* to say "collapse the object to a mesh first",
  and a path opened it to say "a path object has no mesh";
- a multi-selection printed a different "select only this object" sentence in the mesh block, the
  contour block and the collapse block;
- a path being reshaped in the viewport printed two sentences about the same state, one over the
  disabled fields and one where the entry button had been.

A primitive's parameters — which its own card calls the source of truth — sat under three headers and
two refusals, so a person who opened a box's card scrolled past counts and non-answers to reach the
three numbers they came for. The measurement, the inventory of every block per kind and the options
that lost are in `plan/notes/U93.md`.

## Decision

**One order, and no headers.** The card draws, top to bottom: a head, the fields the selected kind
can edit, one row of actions, and the detail of a mode while it runs.

- The head is the kind's badge, the name field, one measure line and the sentence that says what the
  document keeps for this kind ("the parameters are the source of truth; the mesh is derived"). It
  holds no editable number, so nothing on it repeats a field below.
- The fields are the figure's dimensions, the primitive's parameters or the ring typed as corners —
  only the editor the kind has, and the only block that carries a separator above it.
- The action row draws at most one mode entry and, at its right end, the one destructive entry the
  kind has. A mesh draws *Edit mesh*; a constructive object or a path draws *Edit contour*; only a
  primitive or a constructive object offers *Collapse to mesh…*.
- The mode's detail — the element choice, the selected vertices or faces — is drawn only while that
  mode runs, and leaves with it.

**Nothing in the card folds.** Folding is the answer for a list that does not fit: the card's longest
block is a six-row corner editor or twelve selected vertices, and both belong to a mode that ends.
The fold state the card kept per section is gone with the sections.

**One refusal, one home.** A selection holding more than one node draws one sentence in the head and
disables the action row, so the refusal has a second cue and one wording.

**The destructive action asks in place.** *Collapse to mesh…* keeps its two-step question, but the
question takes the action row's place instead of growing a header of its own, and a selection that
stops being one object drops it.

**The name commits when the box is left**, like every number in the card (ADR 0092). The *Rename*
button beside the field was a second commit path for one value and goes.

## Consequences

- A primitive card went from five headers and fifteen rows to three lines of head, its three fields
  and one button; a mesh card from three headers to a head and one button; a constructive object from
  six headers to a head, its fields and a row.
- Eleven messages lost their only use (`SectionGeometry`, `SectionFigure`, `SectionParameters`,
  `SectionMeshEditing`, `SectionContourEditing`, `SectionContour`, `SectionCollapse`,
  `CollapseBeforeEditing`, `PathHasNoMesh`, `FigureEditingInViewport`, plus two of the three
  `SelectSingleObject…` sentences) and three refusals became one, `ObjectSelectSingle`.
- The card's order is now the same for every kind, which is what lets one test assert four kinds at
  once: `crates/ui/src/object_card.rs` (`the_card_draws_only_the_blocks_the_selected_kind_can_use`).
- The panel card keeps its thirteen headers. It is the same defect on a bigger surface and needs its
  own audit, which `plan/debt.md` records rather than this decision guessing at it.
- Revisit this if a block arrives that genuinely does not fit one screen: a fold belongs to that
  block, and the head and the action row stay unfolded.
