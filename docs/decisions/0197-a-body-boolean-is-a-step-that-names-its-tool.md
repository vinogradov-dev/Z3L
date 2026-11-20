# 0197. A body boolean is a step that names its tool

- Status: Accepted
- Date: 2026-11-21

## Context

`SolidPlan` has carried boolean steps since ADR 0041, and `docs/domain/geometry.md` has named "a
boolean with another object" as the next feature since the extrusion landed, but the document's
`Feature` held one `Extrude`: no body could be cut, joined or intersected from the interface, and
the Model tab's three `model.booleans` cells had no command behind them.

Two questions decide the shape, and neither is about the geometry.

The first is what the operation *writes*. The habit a maker brings from other editors — click a
body, click a cutter, and get a new body — spends the cutter, so a second body that wants the same
cut needs a second copy of it, and an edit to the cutter reaches nothing. The second is where the
result lives: a boolean between two solids has no exact polygon answer, so it needs the kernel, and
the kernel is a service the shell owns, not the document.

The owner ruled the first: the source object is a reusable tool, the cutter is not spent, and one
cutter may cut several bodies. The second follows from the layering: the plan is derived, the
document keeps the reference, and only evaluation touches the kernel.

## Decision

**A boolean is a feature, not a new object.** `Feature::Boolean { op, tool }` is appended after the
extrusion: `op` is one of the three operations and `tool` is the id of another constructive object.
The target is edited in place in one revision and one undo entry; nothing is created, so the tree
keeps one node per body and the collapsed body stays the escape hatch (ADR 0037).

**The tool is a reference, never a copy.** It stays in the tree, it may serve any number of bodies,
and it may be a body that carries steps of its own. `Project::boolean_tools_of` resolves the whole
closure on read, each tool's placement in the frame of the body that names it, so moving, editing or
re-tooling a cutter changes every body that names it. A body that copies another body's history
names the same tools, so a duplicate is cut by the same cutter.

**The reference is validated where the document is.** The tree walk refuses a tool that is not a
constructive body, a body that names itself and a graph that closes a cycle; `node.delete` refuses
to remove a node a step names, so a file is never written with a dangling reference. The same
question — `validate_boolean_tool` — decides the armed cell, the preview and the command, so a
picture on screen is always a step the ✓ can store.

**The plan is compiled in the target's frame.** `param::constructive_plan` walks the history with a
tool resolver; each step appends a nested plan whose base frame is the tool's own placement, in
millimetres, in the same coordinate space as the target. Nesting is bounded by `MAX_PLAN_DEPTH`, and
the document's own cycle rule is the first line of defence.

**The cache keys on both.** A constructive body's entry holds its history *and* its tool closure
(with the placement of each). A tool that moves or changes re-derives every body that names it; an
unrelated edit re-derives none. A history with one extrusion keeps the analytic path and needs no
kernel at all; a boolean body is built by the kernel and tessellated by the call the panel path
already uses.

**The mode is the contour boolean's, with bodies instead of rings.** The three Model cells arm it,
the target is the selected body, the next picked body is the tool, the result fills the session
overlay (ADR 0093), Enter dispatches `object.boolean.set` and Escape writes nothing. The object card
lists each step with its operation and its tool's name and removes one from its row.

## Consequences

- A cutter is reusable by construction, which is what the owner asked for, and the operation costs
  one reference instead of one copy of the cutter's geometry.
- A body's mesh now depends on nodes other than itself, so the cache carries a closure. Deleting a
  tool is refused until the steps that name it are gone: a removal is work the person does in two
  visible steps rather than one silent cascade.
- `object.collapse` bakes only what the analytic backend builds exactly, so it still refuses a body
  with a boolean step: the command path has no kernel to evaluate one. The gap is filed in
  `plan/debt.md`; nothing is lost, because a boolean body keeps its history by design.
- The schema moves to 30 and the golden file carries a step and its tool.
- Revisit if a boolean ever needs to reference a face or an edge of the result: naming a sub-shape is
  the design question [0037](0037-a-geometry-object-is-a-mesh-or-a-feature-history.md) left open.
