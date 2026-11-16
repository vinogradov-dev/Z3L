# 0137. A rubber band selects by the direction it is dragged

- Status: Accepted
- Date: 2026-11-16

## Context

There was no way to take several objects with the pointer: a press either picked one node or started
the camera. The owner asked for the AutoCAD rectangle — drag one way and it takes what it encloses,
drag the other way and it takes everything it touches — and named the reason it matters: a cabinet
has dozens of parts, and picking them one at a time is not how a maker works.

The gesture collides with two rules that were already written: a click on empty space inside a
product selects the product (ADR 0100), and a press on empty space with a navigation mode armed
orbits the view. Both had to answer before the band could exist.

## Decision

**Five lines, in our words.**

```text
intent:    take several objects at once with one gesture
input:     primary press on empty scene space, drag, release; Shift adds, Ctrl removes
feedback:  a translucent rectangle in the selection ink from the frame the pointer starts moving;
           a solid outline when it encloses, a dashed one when it touches
recovery:  Escape drops a live band and writes nothing; a release that never travelled is a click
edge:      no object under the press, a closed cabinet that holds several parts, a band drawn over
           a chrome panel, a press over a product's envelope
```

**The dominant axis decides the rule, and the forward way encloses.** A drag whose horizontal travel
is at least its vertical one is read on X, otherwise on Y; rightward or downward is a *window* that
takes what it encloses, leftward or upward is a *crossing* band that takes what it touches. Screen
directions only, never the world: the same gesture must mean the same thing from every camera.

**An object is selected whole.** A closed container is one object (ADR 0022), so a window selects it
only when **every** shape it draws is inside; a crossing band selects it when **any** shape reaches
the rectangle. That is what stops a band around one board of a closed cabinet from taking the cabinet,
and it is why the frame reports every shape it holds, missed ones included.

**The band measures the projected silhouette, not the mesh.** `scene` projects every shape's vertices
and takes their convex hull — for a box, exactly the outline the viewport draws — and tests that hull
against the rectangle. A mesh is never walked triangle by triangle, and an object's screen bounds are
not used either: a diamond's bounds have corners the shape never visits, and a band there must take
nothing. Paths and product envelopes are shapes too, so a drawn chain and a closed piece are covered.

**It starts only where nothing is.** A press that lands on a shape, a gizmo handle, a camera glyph, a
chrome panel or an armed tool never starts a band; the tool and the pick keep their press. A product's
envelope is the exception: it is a container's own target, not a shape, so the empty part of an open
piece is still empty space a rectangle can begin in (`SceneFrame::pick_surface`).

**It is chrome paint, in the selection ink.** The rectangle is drawn with `egui` over the image, on
the scene layer, so the work cursor's own layer is above it and every panel, card and menu masks it
(ADR 0123); it is clipped to the view it was drawn in. The stroke and the fill are the cyan the
selected objects are highlighted in (`scene::SELECTION_LINE_COLOR`, `theme::selection_band`), so the
band and what it will take are one colour; the guide amber stays the program's help (ADR 0111). The
fill is 26/255 so a board under the band keeps its own colour.

**It writes nothing.** A band changes view state only: no command, no revision, no undo entry, and
Escape is a cancel that costs nothing because the rectangle never touched the document. A press that
never travels stays the plain click, so an empty-space click still clears the selection. Shift unions
the covered set into the selection and Ctrl subtracts it; a band never toggles a member the way a
Shift-click does, because one release is one gesture over a set.

## Consequences

- A navigation mode armed in the cluster owns the primary drag, so the band is available only while
  no orbit, pan or zoom button is armed. The cluster says which mode is armed, and Escape clears it.
- Picking gained a second question — "what shape is under this point" — beside "what object" and
  "what does this rectangle cover": `SceneFrame::pick_surface`, `NdcRect`, `BandCoverage`,
  `SceneFrame::band_hits`. The three are one family and live together.
- The cost is one projection per vertex and one hull per shape, paid on release, not per frame.
- The band draws no number and has no card: it is a gesture, not a tool, so it adds no inventory row
  and no key of its own.

## References

- `crates/scene/src/band.rs` — `NdcRect`, `BandCoverage`, `SceneFrame::band_hits`, the hull.
- `crates/ui/src/editor/selection.rs` — `SelectionBand`, `BandRule`, `BandView`, `Editor::select_band`,
  `SelectionBand::paint`; `crates/ui/src/theme.rs` — `selection_band`, `BAND_*`.
- `app/src/document.rs` — `Document::update_band`, `Document::band`, the paint call; `app/src/mesh_pick.rs`
  (`pick_surface`); `app/src/frame_tests.rs`, `app/src/ui_shots.rs` — the headless gesture and the shots.
- ADR [0022](0022-closed-containers-pick-as-one-object.md), [0100](0100-a-frame-is-created-opened-and-drawn.md),
  [0111](0111-a-guide-is-one-amber-in-screen-space.md), [0123](0123-work-area-paint-sits-under-the-chrome.md).
- Plan: U103.
