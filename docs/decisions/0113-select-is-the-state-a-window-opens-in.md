# 0113. Select is the state a window opens in

- Status: Accepted
- Date: 2026-11-14

## Context

The viewport offered two transform modes, `Move` and `Rotate`, and one of them was always up: a
selection drew its arrows or its rings whether the person asked for them or not. A press on an axis
answered with a drag, so a person who only wanted to look, pick and read worked among handles they
had not asked for, and a stray press on an axis moved the cabinet.

The window also opened in `Move`, which is the state that draws the most clutter on the first frame
of a new session. Professionals open an editor in a state that does nothing: the model is what the
window is about, the tools are what you take up.

## Decision

**`TransformMode` gains `Select`, and `Select` is the third state of one control.** Three states of
one value belong to one control, so the status bar draws them side by side in one band instead of a
pair of switches; the filled cell is the mode the viewport is in.

**`Select` is the state a window opens in.** A fresh window draws no arrows and no rings. This is a
default the person meets on launch, and the owner asked for it: a fresh editor is for looking. The
cost is one press or one glance to arm a transform, and that press is the same one the two gestures
always needed.

**`Select` offers no handle at all.** The viewport asks the mode for a `Transform` (`Move` or
`Rotate`) and does nothing where it gets `None`; the gizmo is neither updated nor painted, so a press
where a handle would be cannot be consumed by a handle that is not there. A click selects, a shifted
click adds to the selection, a double click still enters a level, and a drag moves nothing. A
half-live gizmo — drawn, but refusing the press — would be the worst of the three: the person cannot
tell an unarmed handle from a broken one.

**The mode stays view state.** It is the window's, per run: no `.fcs` field, no undo entry, no
revision. It is not a document property because it mutates nothing, and it is not a session
property because a tab switch in one window should not change the tool under the hand that is still
holding it (ADR 0029).

## Consequences

- `TransformMode::transform()` is the one place that says which states own a gizmo, so the update,
  the painted overlay, the "did a handle take this press" flag and the move-distance readout all
  read the same answer instead of matching three states four times.
- A window opened with a scene now shows the model and its selection outline only. A person who
  arrives from another editor finds the arrows after one press on *Move* in the status bar.
- The status bar's segmented control is the only place the mode changes, so the three states share
  one width and one rule; a keyboard shortcut for the modes is not introduced here and would need
  its own decision, because `W`, `E` and `R` are drawn by the drawing tools.
- The cost of a wrong default is one press. The cost of the old default was a moved cabinet from a
  stray click, which undo fixes but a person does not expect.
- `cargo xtask ui-rules` is unchanged: the control is drawn by the status bar's own module and takes
  its colours from `theme::control` and the theme's selection visuals.

## References

- `apps/furnicraft/src/main.rs` — `TransformMode`, `Transform`, `Shell::mode`.
- `apps/furnicraft/src/document.rs` — `Document::viewport`, the one frame that asks the mode.
- `apps/furnicraft/src/status_bar.rs` — `transform_mode_ui`, the one control of three states.
- `apps/furnicraft/src/frame_tests.rs` — the headless assertion that one press picks in `Select` and
  moves in `Move`.
- ADR [0029](0029-view-state-belongs-to-the-session.md) — view state, and why the mode is the
  window's.
- ADR [0020](0020-the-translate-gizmo-previews-and-commits-one-placement.md) and
  [0027](0027-the-rotate-gizmo-uses-world-aligned-rings.md) — the two gestures `Select` turns off.
- Plan: U87.
