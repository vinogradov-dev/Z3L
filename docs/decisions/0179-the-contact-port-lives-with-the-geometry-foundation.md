# 0179. The contact port lives with the geometry foundation, and parry answers it

- Status: Accepted; the port half of X85, amending
  [0165](0165-the-move-tool-is-the-object-move-and-a-solid-body-is-measured-by-its-faces.md)'s
  world-box reading
- Date: 2026-11-19

## Context

A body's contact is measured by the world box around it: `SurfaceBox::meets`, `MoveRule::limit`,
`resting_inside` and the probe all read a box. The box is a coarse reading — a turned cone is a
cuboid, so it meets a plate long before its apex reaches the corner between two plates — and the owner
asked for near-physical interaction: any surface lives by its own faces, never by the box around it.

The port that answers with a body's own surface needs a third-party collision engine, and this project
has one rule for that: the document never sees the engine (`kernel` and `kernel-truck`, ADR 0001). Two
facts fixed the shape of the port:

- `model` (L1) is what measures a body during a drag, a probe and a write, and a crate may only depend
  on **lower** layers, so a port crate of its own would have to sit at L0;
- a port at L0 that speaks vectors must depend on `geom`, which is L0 as well, and the layer gate
  refuses an edge to a crate of the same layer — a rule no task may spend.

The owner chose the injection shape over a single crate that binds the engine internally, so parry
never leaves its adapter and a test can answer contact with a stub.

## Decision

**The port is a module of `geom`.** `geom::contact` holds `Shape` (a convex hull or a closed triangle
surface, in the body's own frame, millimetres), `Pose`, `Penetration`, `Prepared` and the `Contact`
trait with `prepare`, `travel` and `penetration` (`overlap` is provided from them). `geom` is the L0
crate every layer already depends on, it already speaks millimetres at the geometry boundary, and it
already holds operations rather than only primitives (`boolean`). The port crate the entry imagined
would have been a same-layer edge into `geom`, which the gate forbids and which nothing else needed.

**The adapter is its own crate at L2.** `crates/contact-parry` implements the port with
`parry3d-f64` 0.31 and is the only crate that may depend on it; `cargo xtask layers` enforces that
exactly as it enforces `kernel-truck`/`truck`, and the crate is composition-only, so a test outside the
shell may use it only as a dev-dependency. It is injected as `&dyn Contact` through the document's
queries — a drag, the write guard and the probe — and the shell constructs it once.

**Double precision, in millimetres.** `parry3d-f64` is the double build: the document stores
micrometres and a workshop is metres wide, which a single-precision build cannot resolve. parry 0.31
speaks millimetres here with no conversion, because the port's unit *is* a millimetre. parry's own
`glam` is one minor version behind the workspace's, so a rotation crosses that boundary as four
numbers.

**A prepared shape, not a rebuilt one.** `Contact::prepare` returns the adapter's own representation in
an opaque `Prepared` handle, so a drag converts each body once when its rule is built and asks the same
two shapes on every frame. A handle of the wrong type answers `None`, never a panic.

**Three questions, not five.** `travel` (how far the mover goes along a heading before it first
touches), `penetration` (how deep the two are sunk and along which normal) and `overlap` (penetration
against the document's own rounding) are what a drag, the write guard and their log read today. A gap
distance and a broad phase are filed for the stage that needs them: an unused query is a promise the
code cannot keep, and the frame budget is measured before a cache is designed.

## Consequences

- The document keeps its one reading of contact: the drag, the release guard and the log ask the same
  port, so a preview cannot show a place the release refuses.
- The port's types are reachable from every layer with no new crate, and `cargo xtask layers` keeps
  parry out of all of them but its adapter.
- A shape the adapter cannot build — a degenerate hull, a triangle surface with a broken index — is
  `None` from `prepare`, so the document falls back to the box reading it has today rather than
  refusing to open a file.
- `geom` gains `thiserror` and one module that mentions a `dyn Any` handle; the crate's doc says what
  the module is for.
- The world-box reading is not deleted in this step: the shape build, the `MoveRule` wiring, the probe
  and the per-revision caches land behind it, and only then does the box stop deciding contact (X85
  stage A and B).
