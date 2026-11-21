# 0208. A primitive is a body, so a boolean cuts it in both directions

- Status: Accepted; closes the consequence ADR 0197 left open
- Date: 2026-11-21

## Context

ADR 0197 made a body boolean a step frozen into the target, and closed with a consequence: *"a
boolean operand is a constructed body: a pedestrian's sphere, cone or cylinder is a solid but not yet
an operand, because the plan compiler can only sweep a ring"*. A maker met that line directly: they
selected a sphere, pressed **Subtract**, and the three Model cells were dead.

The owner asked the question the record should have answered first: *why does a boolean need the
target's history at all?* It does not. A step needs a **home on the body it built** — that is what
keeps the result editable and undoable as one body — and a primitive had no home, so it could not be
a target. And a tool needs to be frozen as a **shape**, which a primitive's parameters are just as
much as a path and a history are.

Two shapes were measured before the road was chosen (`plan/notes/X102.md`):

- The geometry kernel builds a solid from a **closed triangle mesh** — one planar face per triangle,
  vertices and edges shared by index — and booleans it: a 32 x 16 sphere (960 triangles) is built in
  4 ms and cuts a board in 12 ms, and a cut result matches the analytic volume to a fraction of a
  percent.
- The kernel **cannot divide a face that holds a ring of the other body's vertices**. A ball is drawn
  around its centre (ADR 0139), which puts that centre on the face it was drawn on, and every uv
  sphere with an equator ring has a whole ring of vertices in that plane: the boolean answers with a
  broken shell instead of a cut. The same ball tessellated with no ring at the equator cuts to the
  analytic half-volume.
- A solid of revolution built from exact surfaces is **not** an alternative: `truck`'s boolean
  support does not cover the surface type a revolution produces, and every cut against it answers
  "produced no solid".

## Decision

- **A primitive body carries the steps cut from it.** `Primitive` is a `PrimitiveShape` — a sphere's
  or a cone's parameters — plus `steps: Vec<Boolean>`. A block subtracted from a ball leaves a
  rectangular notch, two balls leave a spherical scoop, and the ball keeps its radius: the same
  arrangement a drawn body has, with the shape's parameters where the swept ring is.
- **A step freezes either shape.** `BooleanTool::body` is a `ToolBody`: a drawn body's history or a
  primitive's parameters, tagged so a file needs no node and no library to rebuild the result. A
  drawn body and a primitive are both targets and both tools; a baked mesh and a panel are neither,
  and `Project::apply_boolean` says so with `NotASolidBody`.
- **The port learns the mesh body.** `kernel::MeshBody` is a validated closed, consistently wound
  triangle surface in millimetres, and `SolidKernel::mesh` builds it. That is the one capability a
  shape no extrusion describes needs, and the one an imported part will need.
- **The primitive mesh builder moves from `mesh` to `param`.** The compile runs in `param`, and a
  crate cannot call one above it; `mesh` already called `param` for the drawn body's mesh, so the
  direction of the dependency and the one owner of the evaluator both keep.
- **The kernel body of a sphere stands its rings half a step off the equator.** The drawn mesh keeps
  its rings and its full pole fan (X55); the body the kernel divides puts no ring of vertices in the
  plane through its centre, because that plane is the face a ball is drawn on.
- **The three Model cells arm on either body.** A primitive is no longer a dead cell with an
  explanation: the cells enable on it, the preview shows the notch, and the ✓ stores the step.

## Consequences

- The owner's gesture works, and so does the reverse: a ball cuts a board.
- `object.primitive.replace` keeps the steps: a ball whose radius changes keeps its notch.
- `object.collapse` refuses a primitive that carries a step in words, exactly as it refuses a drawn
  body with a step, instead of baking the plain shape.
- The schema moves to 32; the golden file carries a ball cut by a block and a top cut by a ball.
- A ball whose centre lies exactly on a face now cuts; a *cone* whose base rim lies exactly in that
  face still depends on the cutter's micrometre of overshoot, and a coincidence the kernel cannot
  divide is a diagnostic on the node rather than a crash (ADR 0197's rule).
- The mesh body makes an imported triangle mesh a boolean operand as soon as something produces one;
  nothing in this change reads one, so that stays the import task's own work.
- Revisit if a maker wants a primitive **target** with steps of many kinds: the step list is booleans
  only, because a primitive is not swept and has no height to extrude.
