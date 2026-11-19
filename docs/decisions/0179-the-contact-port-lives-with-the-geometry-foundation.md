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
surface, in the body's own frame, millimetres), `Pose`, `Penetration` and two traits: `Contact`, which
the adapter implements and which is asked **once** per gesture to turn a shape and its pose into a
prepared body, and `Solid`, which that body implements — `bounds`, `shifted`, `travel`, `penetration`,
with `overlap` a free function over the last one. `geom` is the L0
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

**The shape comes from the parameters, never from the render mesh.** `model` builds it in
`project::contact_shape`: an object with a baked mesh is its triangles, an object with a primitive is
the convex hull of the surface those parameters name, and everything else — a panel, an envelope, any
container — is the box the rule already measured. A body the document cannot describe (a constructive
payload, a drawn path, a mesh past the port's caps) answers `None`, and the caller keeps the box. A
fallback that measures less precisely is honest; a shape invented from a screen tessellation is not.

**One seam, and `None` is the reading of the day.** `Project::move_rule` and
`Project::resting_conflict` take `Option<&dyn Contact>`. `None` means no adapter is installed and every
pair is measured by its world box, which is what the document did before the port existed. The drag
and the guard must be handed an adapter together: a preview measured by a surface and a release
measured by a box would refuse the place the drag just showed (ADR 0166). The engine's write guard
passes `None` today, so both still read boxes; handing the adapter to a session is the step that flips
them, and the furniture reading — a panel inside a case is a slab in a frame — is deliberately left on
the box (ADR 0165).

**A prepared body, and no trait object in the rule.** `Contact::prepare` returns `Shared` — an
`Arc<dyn Solid>` the adapter made — so a drag converts each body once when its rule is built and asks
the same bodies on every frame. The rule stores bodies, not shapes and not an adapter: a `&dyn
Contact` threaded through `MoveRule` would put a lifetime on a type the shell keeps across frames, and
every question about a pair needs the adapter only because the shapes were not prepared yet. A body of
another adapter answers `None` from a pair query, never a panic.

**Three questions, not five.** `travel` (how far the mover goes along a heading before it first
touches), `penetration` (how deep the two are sunk and along which normal) and `overlap` (penetration
against the document's own rounding) are what a drag, the write guard and their log read today;
`bounds` and `shifted` exist so a rule can keep one body and move it per proposal. A gap
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
