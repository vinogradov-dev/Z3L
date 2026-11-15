# 0128. A sketch angle is read from the nearer end of the X axis, and Tab belongs to the plate

- Status: Accepted
- Date: 2026-11-15

## Context

ADR 0126 put a plate on every number of the point being placed and gave the plates Tab, Enter and
Escape. Two questions were left open by that record and could not be answered by the widget: what a
segment's angle actually measures, and what the polyline's line/arc switch does once Tab is taken.

An angle in a plane needs a reference, and the X axis has two ends. A CAD sketch shows the *smaller*
angle; a person typing `30` means thirty degrees, and a person typing `240` means the other side of
the axis. Meanwhile Tab was the polyline's switch between a straight segment and an arc (ADR 0076),
so the same key would have meant "next plate" while a plate holds the keyboard and "arc" while it
does not — the double reader ADR 0126 exists to keep out.

## Decision

**A segment's angle is the smaller one, read from the nearer end of the X axis, and the sweep the
guide draws is the same signed sweep the plate shows.**

- A direction in the right half-plane reads from `+X`, one in the left half-plane from `−X`; the
  plate shows `|sweep|`, at most 180°, and the sweep is signed, positive counter-clockwise
  (`SketchAngle` in `crates/ui/src/draw_fields.rs`).
- The reference guide draws its axis along that end and its arc the short way to the direction being
  placed, so the arc and the number are the same measurement (ADR 0127).
- A typed number is a direction from `+X`, 0..360 counter-clockwise, so a value above 180 flips the
  reference to `−X` — the same rule read the other way round. The plate keeps the typed number: a
  lock is a promise, and the guide shows where that promise points.
- The plate's range is 0..=360 for every angle, and the point a sketch places is the pointer with
  every locked number in its place (`PointState::placed`), so a typed length and angle outrank the
  snap instead of being rounded into it.

**Tab is the plate's key, and the polyline's segment kind moves to the right-click menu.**

- Tab moves the keyboard between the plates and never changes the figure; the drawing tool no longer
  reads it (`handle_keys` in `apps/furnicraft/src/document.rs`).
- A right click on a running polyline opens `crates/ui/src/draw_menu.rs`: the next segment is
  straight or an arc, and the chain can finish from the same menu. A fixed tool keeps the CAD habit
  of finishing on the right button.
- The menu is its own surface, not a row of the node context menu: that list answers what the node
  under the pointer owns, and a running draft names no node. It is drawn in the chrome that keeps a
  press inside it from reaching the scene (ADR 0080, U92).

## Consequences

- The four-quadrant table is a test (`the_angle_reads_from_the_nearer_end_of_the_x_axis`), and so is
  the typed round trip: every tool's `Length`-and-`Angle` stage is measured against it.
- A tool that wants another angle kind adds a `FieldParam` (`Rotation` is the plain 0..360 direction a
  polygon's rim point uses); it does not add a second angle rule.
- The first Escape after a typed number gives the whole point back to the pointer — every lock, not
  only the focused plate's — and hands the keyboard to the tool, so the second Escape ends the tool.
- Removing Tab from the polyline costs the fastest way to draw a mixed chain; the menu is one click
  and the status bar already names the segment kind. If Tab is ever wanted back, it comes back as a
  plate key, not as a second meaning in the same frame.

## References

- `crates/ui/src/draw_fields.rs` — `sketch_angle`, `XEnd`, `SketchAngle`, `PointState`, `Reference`.
- `crates/ui/src/draw_menu.rs` — the running polyline's menu.
- ADR [0126](0126-the-point-being-placed-shows-its-numbers-at-the-cursor.md) — the plate and its
  grammar; ADR [0127](0127-a-sketch-has-two-guides.md) — the construction line and the reference.
- ADR [0111](0111-a-guide-is-one-amber-in-screen-space.md) — the guide amber the arc reads from.
- ADR [0076](0076-tab-switches-a-polyline-segment-between-a-line-and-an-arc.md) — the key this record
  takes away.
- Plan: C2.
