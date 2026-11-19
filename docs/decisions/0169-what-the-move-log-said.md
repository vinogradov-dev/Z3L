# 0169. What the move log said: a typed zero, a lost bond, and a stale hold

- Status: Accepted; fixes what the log of
  [0168](0168-the-shell-writes-one-log-and-the-carry-logs-its-frame.md) showed, and carries the zero
  convention of [0092](0092-a-number-is-typed-into-the-scene.md) and
  [0126](0126-a-plate-shows-one-parameter-and-zero-gives-it-back.md) into the carry's distance fields
- Date: 2026-11-19

## Context

The first session with the log on ended with a carried piece standing still while the pointer crossed
the viewport. The file answered it in one line, twice:

- `asked=(591.215,118.633,0.000) resolved=(591.000,119.000,0.000) delta=(0.000,-0.000,0.000)
  typed=Some(0.0)` — the distance field held a typed `0`, and a typed number outranks the hand
  (ADR 0103), so the piece was pinned where it started. The field's own convention says a typed zero
  *releases* a face (ADR 0092, ADR 0126); the carry's `Distances::place` pinned it instead.
- `hand=none` on 135 frames — the bond between the hand and the piece's pivot was cleared (a camera
  gesture, or the pointer leaving the work area) and the plane branch only read an offset it already
  had, so the carry never recovered and stood still for the rest of the gesture.
- `jump=true` on 1157 frames, almost all of them with a typed number in force: the jump hold was
  skipped under a typed value, so its pending request stayed in the struct and would have fired the
  moment the number was released.

## Decision

**A typed zero releases the face, in the carry too.** `Distances::place` unpins the side and drops its
lock on a zero, so the pointer takes the face back; the same convention as every other typed field.

**A lost bond is read again.** The carry re-reads the hand-to-pivot offset when it is missing, from the
piece's own pivot and the hand, instead of waiting for a camera gesture to put it back.

**A typed number clears a pending jump.** While a number holds the piece, the hand is not asking for a
crossing, and a request remembered from before the number would fire on the release.

## Consequences

- A number typed into a carried piece's distance field still pins that axis (ADR 0103), and `0` is the
  way out — the same gesture the drawing plates teach, so a maker does not have to learn a second one.
- A carry survives the camera moving under it and the pointer leaving the work area; a piece no longer
  freezes until the gesture ends.
- The log is what made these three visible at once, in one reading, from a report of "it doesn't
  move". A second session with the new lines carries `typed`, `held`, `offset` and the hold's age, so
  the next report distinguishes a rule cut from a pin from a hold without a rebuild.
