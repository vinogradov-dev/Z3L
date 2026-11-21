# 0206. A boolean result is one object, a hole is part of one region, and splitting is a command

- Status: Accepted; the tree rule the contour boolean, G5 and a solid boolean all read
- Date: 2026-11-21

## Context

A boolean can leave several disconnected pieces, and a subtraction can leave a hole. The tree has
to say, once, what the result of one operation is: one object or several, and where a hole lives.
The programs we may study for behaviour answer it the same way, though their vocabularies differ:

- **The operation is one step; how many shells it left is a property of the shape.** AutoCAD's
  `UNION` over solids that do not touch gives one 3D solid with several lumps, and `SOLIDEDIT → Body
  → Separate` splits them. Blender's Boolean modifier keeps one mesh and its islands stay in it;
  `Mesh → Separate → By Loose Parts` is a separate command. OpenSCAD returns one polyhedron with as
  many shells as the shape has, and FreeCAD returns one object — a `Compound` when the pieces are
  apart — that `Part → Compound → Explode` opens. Fusion 360's `Combine` gives one body when the
  result is connected and several when it is not, and its API calls the parts of a body *lumps*, so a
  body of several lumps is a shape the kernel already allows; `Split Body` is the way to make several
  bodies of them. Rhino is the exception that names the other reading: its booleans hand back several
  polysurfaces and `BooleanSplit` is the explicit splitter.
- **A hole never becomes a part of its own.** In SketchUp a circle drawn inside a rectangle makes one
  face with an inner loop, not two faces; in a solid modeller a hole is a hole of one body, and a
  panel's core in this tree is already a region with holes (`param::machining`).
- **Production wants one connected piece.** A part of a kitchen is one board or one carcass, not a
  set of lumps, so a drawing, a cutting list or an order package reads a piece, and a body of several
  lumps is a shape an operation may leave on the way, not a thing a shop is asked to make.

## Decision

- **One operation, one node.** A boolean stores exactly one object carrying its own result — a
  drawing of several rings, or a body with its feature history — whatever the number of pieces the
  shape came out in. The pieces are a property of the shape; the tree records the operation.
- **A hole belongs to the region it is in.** A hole is never a ring of a drawing and never a body of
  its own: it is an inner loop of one area (a drawing's face) and, for a solid, a hole in one body's
  profile. So an area with a hole is one area, and the frame between two nested rings is one face.
- **Several pieces stay one node until a command says otherwise.** A body that a boolean leaves as
  several shells is one body. Splitting it into several bodies is an explicit command of its own
  (a later entry: it is a change of meaning, not a step of the boolean), and it is that command — or
  the refusal that names it — that production walks into when one node is not one piece.
- **The door for a hole is the profile.** Turning an area with a hole into a body is what G5 adds: a
  profile that stores its holes. A body is never assembled from one extrusion per ring, and a hole is
  never drawn as a subtraction step, because a step's tool is frozen at the height it was stored
  while the body's height is what a pull changes (ADR 0197, ADR 0205).

## Consequences

- The contour boolean keeps its present tree shape (`Geometry::Path` for one ring, `Geometry::Contours`
  for several), and a body's boolean step keeps appending to the target's history (ADR 0197): neither
  creates one node per piece, and no command gains a "split" flag by accident.
- An area of a drawing may carry holes (ADR 0205), and the body built from it carries them too once
  G5 lands. Until then a face with a hole is refused with a sentence rather than approximated by
  several bodies.
- A future split is its own command with its own id, its own undo entry and its own name in the tree;
  it is the one place where a node becomes several, and it says so.
- A workshop-facing surface (a drawing, a parts list, the order package) may meet a node of several
  lumps. Its answer is a sentence that names the split, never a silent list of pieces.
