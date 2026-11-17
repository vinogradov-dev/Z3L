# 0140. The corner display is a diagnostic block the maker can read

- Status: Accepted
- Date: 2026-11-17

## Context

The switch called *Show projection and angles* drew one line under the orientation cube: pitch, yaw,
roll and the projection. It answered "where is my view" and nothing else. The owner ran a scene of
5 000 spheres and reported a jerky pointer, a drag that starts late and an interface with no number
to blame; the two causes were found with a throwaway headless probe instead of with the program
(X58 draws every drawable whatever the camera sees, X59 ray-casts every triangle in the scene on a
click: 161 ms at 5 000 spheres). A furniture editor has no profiler a maker can read, and a report
that says "it is slow" cannot be acted on.

The line was also hostage to the cube: the whole corner widget has its own switch, and a maker who
wants the corner free for the model loses the numbers with it.

## Decision

**The corner draws one block, and it carries the numbers a frame costs.**

```text
intent:    understand why the scene answers slowly
input:     the Show the on-screen display switch in the display popover; no click, no key
feedback:  a right-aligned block in the free corner: the camera line, the frame rate and frame time,
           the frame's objects and triangles, and the last rebuild, upload and pick in milliseconds
recovery:  the switch turns the block off; nothing is written to the document
edge:      a corner too narrow for the cube, a headless frame, a number that was never measured
```

**Measured where the work happens, read one frame later.** The frame rate is an exponential average
(one quarter of the newest frame) of the interval `egui` reports, so one hitch does not swing it; the
rebuild, the upload of the frame and the pick are timed by the code that spends them and stored in
one meter on the document. A number that has no measurement is not printed at all: the block never
claims zero.

**While the block is on, the window repaints every frame.** A rate read off a window that repaints
only on input is the idle rate, not the frame rate; the block is a diagnostic mode and pays its own
frames. It is off by default.

**The block outlives the cube.** Two switches, two surfaces: the cube is the view control, the block
is the diagnostic readout, and the block hangs from the same corner whether the cube is drawn or not.
The cube's rectangle is still the block's anchor, so both stay clear of the side panels and the
ribbon band by the rule that already placed them (U73).

## Consequences

- The words live in `crates/text` (`OsdRate`, `OsdScene`, `OsdRebuild`, `OsdUpload`, `OsdPick`) beside
  the camera line, and the block is one module (`app/src/osd.rs`) that owns its meter, its lines and
  its paint. The cube keeps the cube.
- The pick timer sits on the three paths that pick the whole scene — a click, a right click and the
  press that arms the rubber band — so the number describes one scene, not one call site.
- The block is paint: it takes no click, it never becomes chrome, and the corner keeps the scene when
  the cube is off.
- The scene cost is counted only while the block is on (one pass over the frame's drawables, no
  vertex walked), so a maker who never opens it pays nothing.
- Revisit when a real profiler view (a frame graph, a timeline) is worth a surface of its own; this
  block is the smallest thing that answers "why is it slow" without one.
