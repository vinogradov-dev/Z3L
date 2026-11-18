# 0147. The furniture move is its own tool, and a panel slides along its thickness

- Status: Accepted; adds a fourth transform mode, takes the furniture rule and its snapping out of the
  free move, and adds the panel axis 0102 and 0103 did not have
- Date: 2026-11-18

## Context

ADR 0102, 0103 and 0146 built one Move tool that decided under the hand: the translate gizmo read
`Project::move_rule` and, whenever the selection was furniture, stopped the drag at a neighbour,
snapped it to a plane or a frame and rounded it to the 1 mm grid. The same arrow was a free
micrometre drag for a loose body and a ruled one for a cabinet, so what a handle does depended on
what happened to be selected. N5 filed a second mode as ruled out for that reason — "a mode would
make a piece movable two ways" — and the owner has since taken the other reading: the editor wants
**two tools**, the move it has, which stays free and turns the furniture snapping off, and a
furniture move that moves a piece by its own rules.

The rules also had a gap a shop notices at once. Nothing gave a panel its own axis: a shelf could be
dragged sideways into the wall of its case and a back pulled out through the front, because the rule
only knew about contact, mount and snapping — and two panels overlapping is contact, not a rule about
which way a board may travel.

## Decision

**`TransformMode` gains a fourth state, `Furniture`,** beside `Select`, `Move` and `Rotate`, and the
status bar's band draws four cells. It is view state, as `Select` is: no document field, no undo
entry, no revision (ADR 0113). The two moves sit next to each other because they are one decision
made two ways, and each cell carries a hover line naming which of the two it is.

**The free `Move` applies no rule and no snapping.** It keeps the pointer's own distance on the three
world axes, quantised to the micrometre the document stores, which is what the free move of loose
geometry always was. What it loses is the furniture rule: no contact, no coordinate plane, no frame
face, no 1 mm grid (ADR 0102, 0146).

**The `Furniture` move reads `Project::move_rule` per drag and adds the panel rule.** A panel offers
exactly one axis: the world axis its own thickness points most nearly at. The panel keeps its local Z
as its thickness (`docs/domain/products.md`), so a shelf rises and falls in its thickness direction,
a divider crosses the case and a back moves in and out — the three drags a shop actually makes. The
two in-plane directions are not refused by an armed handle: they have **no handle at all**, because
an armed handle that will not drag is worse than no handle (ADR 0113). Everything else keeps the
axes it had: a frame moves on three world axes subject to its class's `mount` trait, so a floor piece
keeps its height, and a body inside a case is free on three.

**A selection offers what every one of its nodes offers.** The gizmo draws the intersection, so a
shelf beside a back draws nothing and a shelf beside a wall-hung frame slides the shelf's way. Two
boards that can travel rigidly nowhere therefore offer no handle, which is the honest answer rather
than a drag that moves one of them.

**Both tools keep the exact-distance fields.** A typed gap is the maker's number on either side and
outranks a snap (ADR 0103), and both tools commit one `node.placement.set_many` on release, so a drag
is one undo entry whether the rule measured it or not.

**The axes are derived once and read by both halves.** `Project::furniture_axes` answers the
per-frame handle gate and `MoveRule` carries the same answer, so the handles a person sees and the
directions the rule allows cannot disagree.

## Consequences

- The tool a person is in says what a drag will do, not the selection: the free arrow never surprises
  with a snap, and the furniture arrow never crosses a board's face. The cost is one decision — which
  of the two moves — that the status bar now asks on every drag, and a fourth cell in that band.
- A board turned off the world axes slides along the axis it leans towards, not along its exact
  normal, because every surface the rule measures is a world box (ADR 0102). A 30°-turned shelf
  therefore travels in one of the three world directions; measuring boards in their own frames is the
  larger change and nothing asks for it yet.
- The panel axis is the tool's, not the document's: a board at the root, with no product around it,
  still offers only its thickness. Inside no product there is no contact and no snap to obey, so that
  drag keeps the exact micrometre distance and only its direction is ruled.
- A typed gap is not clipped by contact, exactly as it was not clipped by a snap: the maker's number
  wins, and the field says which surface it is measured to.
- A keyboard cell for each mode is not introduced here; the keys of the three old states are a task
  of their own (`U113`), and that task owes the fourth its key too.
- `cargo xtask ui-rules` is unchanged: the control is drawn by the status bar's own module and takes
  its colours from `theme::control` and the theme's selection visuals.

## References

- `crates/model/src/project/move_query.rs` — `Project::furniture_axes`, `MoveRule::axes`,
  `MoveRule::allows`, `Project::panel_axis`.
- `app/src/main.rs` — `TransformMode::Furniture`, `Transform::Furniture`, `TransformMode::move_kind`.
- `app/src/gizmo_math.rs` — `MoveKind`, `Target::move_kind`.
- `app/src/translate_gizmo.rs` — the handle spec and the rule chosen by the kind of move.
- `app/src/status_bar.rs` — `transform_mode_ui`, one band of four cells.
- ADR [0113](0113-select-is-the-state-a-window-opens-in.md) — the mode is view state and a state of
  one control; an armed handle that refuses is worse than none.
- ADR [0102](0102-a-furniture-entity-is-a-surface-a-frame-is-a-shell.md),
  [0103](0103-one-probe-serves-the-niche-the-distances-and-the-move.md),
  [0146](0146-snapping-is-a-workspace-setting-and-the-coordinate-planes-stand-in-for-walls.md) — the
  rules the furniture move applies and the free move no longer does.
- ADR [0097](0097-a-product-is-a-container-with-an-authored-envelope.md) — the `mount` trait.
- Plan: N22, whose `Why` records the owner's request and whose note (`plan/notes/N22.md`) records
  what was measured.
