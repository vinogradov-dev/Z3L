# 0197. A body boolean freezes the shape it cuts with

- Status: Accepted; supersedes its own first form (a live reference to the tool node)
- Date: 2026-11-21

## Context

`SolidPlan` has carried boolean steps since ADR 0041, and `docs/domain/geometry.md` has named "a
boolean with another object" as the next feature since the extrusion landed, but the document's
`Feature` held one `Extrude`: no body could be cut, joined or intersected from the interface.

The first form of this record decided that a step names the tool's **node**, so one cutter serves
several bodies and moving or editing it re-cuts every one of them. The owner tried it on a real
part and the picture answered: the cutter stayed drawn, so the scene read as two overlapping bodies
rather than one cut body, and dragging the cutter moved the hole with it. Both are the same
decision — a live operand is a second body in everything but name — and the owner ruled for a
baked result: *"the result of a boolean subtraction does not exist as a body of its own"*.

The second form also has to answer what a workshop means by a **tool**. A cutter is a thing the
shop owns and applies again; the mark that already exists on an object (`Object::is_tool`, ADR
0194) says exactly that, and it is the one word the document has for "this is not a part".

## Decision

**The step carries the shape, not a reference.** `Feature::Boolean { op, tool }` holds a
`BooleanTool { name, body, placement }`: the cutter's own feature history and its placement in the
target's frame, frozen when the step was applied. The name is the source's, kept as a label so the
object card can say "difference · cup bore" long after the node is gone. A step is therefore
self-contained: nothing outside the body can change its shape, no file can name a node that was
deleted, and no cycle or dangling reference is possible.

**The operation edits the target in place and spends the cutter.** One command appends the step and
removes the cutter's node in the same revision and the same undo entry, so what is left is one body
with a hole instead of a body and the shape that cut it. A cutter the maker **marked as a tool** is
kept — that is what a tool is for — and may be applied again; each application freezes its shape
again, so editing the tool afterwards changes the bodies already cut and nothing else.

**The mark also lets a tool pass through parts.** The placement guard and both drag rules read
`Object::is_tool` and let a marked body interpenetrate whatever it stands in (`X72`): a cutter that
cannot be carried into the body cuts only a body it happened to be born inside. Every other pair
keeps the rule that a part may not come to rest inside another.

**A boolean pair is one shape, not a collision.** Because the mark says "this is a tool", the
document needs no stored relationship between two nodes to know that they may overlap.

**A cutter is swept one micrometre past its own ends.** The commonest cut of a workshop is a hole
drawn from the board's own surface, so the cutter's face and the body's face are one plane, and the
kernel answers a boolean between bodies that share a face with nothing at all. The cutter's sweep
therefore starts one micrometre below and stops one micrometre above its own ends — below the
document's own resolution, and the same trade the panel cutouts and the end bevel already make. The
body being cut is never extended.

**Nesting is bounded where the shape is stored.** A tool may itself be a body with steps, so the
step recurses; `Constructive::new` and `push_boolean` refuse a history that nests deeper than
`MAX_BOOLEAN_DEPTH`, which keeps every walk over a document finite.

**A node the kernel refuses is a diagnostic, not a dead frame.** A body whose step leaves no solid
draws nothing and says so in the status bar; the rest of the scene still draws and still follows the
pointer, and an armed boolean whose preview is not a solid drops its ✓ instead of storing a body
nobody can build.

**The mode is the contour boolean's, with bodies instead of rings.** The three Model cells arm it,
the target is the selected body, the next picked body is the tool, the result fills the session
overlay (ADR 0093), Enter dispatches `object.boolean.set` and Escape writes nothing. The object card
lists each step with its operation and the cutter's name, removes one from its row, and carries the
tool mark.

## Consequences

- The result is a body: one node, one shape, one mesh — and the picture agrees with the tree.
- Moving the cutter afterwards changes nothing, which is the trade the owner chose. Changing a hole
  means cutting again; a shape a maker wants to keep editable is kept as a tool and re-applied.
- A body's mesh depends on nothing but its own payload, so the evaluation cache is back to one key
  and the derivation cannot go stale because another node moved.
- The schema moves to 31 and the golden file carries a step with its frozen shape.
- The tool mark now has a control (the object card) and a command (`object.tool.set`); it was a
  document field with no surface before.
- A boolean operand is a constructed body: a pedestrian's sphere, cone or cylinder is a solid but
  not yet an operand, because the plan compiler can only sweep a ring. Filed as its own task; until
  then the three cells say what they wait for instead of sitting dead.
- Revisit if a maker wants a *live* cut: that is a second kind of step (a reference) and a different
  picture, not a switch on this one.
