# 0149. A catalogue part is a document, and the scene holds an instance of it

- Status: Accepted
- Date: 2026-11-18

## Context

A hinge, a runner, a handle, a screw, a door facade is a small three-dimensional thing with a
meaning: a mounting frame, anchors, the drilling lines that bore the panel it sits on, and the
fasteners it brings with it. A supplier ships it as a mesh; the mesh alone says nothing about where
the cup bore goes or which screw goes into which hole.

Two shapes were available. A mesh with an overlay of our own ("bake the mesh, attach lines in the
catalogue") invents a second geometry format, a second cache and a second viewer. A parametric recipe
in the package makes every supplier model a program we have to write.

The program already has a document format, a node tree, typed properties and a class registry
(ADR 0034, 0094), and it already reads and writes that format atomically (`crates/project`). A hinge
modelled as a small document needs no new format at all: an imported STL becomes a node in an empty
document, the maker adds anchor and drilling objects, and the result is saved like any scene.

The second half of the problem is freshness. A workshop edits its hinge model — a longer cup bore, a
second screw — and every door that uses that hinge must pick the change up; that is why those
programs rebuild a door on every facade edit. An insertion that copies nodes into the project cannot
do that: the copy is frozen at the moment of insertion.

## Decision

**A catalogue item may point at a part document: our own scene file, stored inside the package,
referenced by a relative path and a content hash.** The item keeps the commercial record; the part
document keeps the geometry and its meaning. A price never lives in a part document, and a drilling
line never lives in an item card.

**A part document is a scene document under a contract.** Exactly one root container, no cameras, no
lights, no named views, no workspace state, and only the node kinds the contract allows. Violating
it is a readable load error. There is one scene format and one loader (ADR 0096): a part file is not
a second format, it is a second *use* of the first.

**The scene holds an instance, not a copy.** A node may be an instance of an item: the placement is
document data, the contents are derived from the item's part document and cached by the item's
version and the directory revision (ADR 0148). A changed part document therefore reaches every
project that uses the item — which is the whole point — and no rebuild is triggered by an unrelated
edit.

**A copy is a deliberate act: `catalogue.detach`.** Detaching turns an instance into ordinary nodes,
drops the link and warns that future changes to the item will no longer arrive. One command, one undo
entry. The default is always the instance.

**Hardware dependencies form a directed acyclic graph, and a cycle is refused by name.** A hinge's
part document may instantiate the screw's item; the same screw may be used by two hinges — a diamond
is normal sharing and stays legal. A cycle is a modelling error: `hinge → screw → hinge` is refused
with the chain spelled out, never truncated at a depth. Two guards, both needed: the directory
application refuses to save a part document that closes a cycle, and the resolve path holds a depth
limit of 8 and a budget of 500 resolved parts, so a hostile or corrupt package is an error rather
than a hang (§4.4).

**Two kinds of reference, two rules.** An *instance* is a geometric dependency and enters the cycle
check. A *property reference* — "the fastener for this hinge is screw X", a material, a decor — is
not a dependency and cannot close a cycle.

**A file that moved or changed is a notice, not a substitution.** The item records the hash it was
linked against; a file whose hash changed is reported and re-linked by a person, because a hinge
whose cup bore moved changes the holes the shop drills. A file that is missing leaves the node
drawable: the item carries a coarse bounding box, so the scene paints a placeholder of the right size
and reports a diagnostic, and the project opens.

**The live document re-derives; the order package is the frozen copy.** Holes are not panel data
(ADR 0150), so an open project always shows the current hinge; an exported order names the directory
revision and the part hashes it was built from, and it does not move afterwards (ADR 0120).

**Insertion is a session flow, not a data write.** Inserting an item opens its part document through
the same loader, validates the contract, and places one instance; the drawer of available items is a
query over the directory, not a file dialog. Authoring a part document is the editor's ordinary work
— an empty document, an imported mesh as a node, anchor and drilling objects, save — and it needs no
second editor and no second viewer.

## Consequences

- The instance node is a scene-schema change and is decided before any caller: the id it stores, the
  fallback it keeps and the way it resolves are one signature, settled at the model layer first.
- Picking returns the instance's node id; the parts inside belong to the item, not to the document,
  so they cannot be selected, renamed or deleted one by one. Detaching is how a maker owns them.
- The mesh cache keys on the instance's item version, so two doors with the same hinge share one
  mesh, and a runtime directory update re-derives exactly the instances whose item changed.
- A part document that instantiates another item makes the directory a graph, so the directory
  application must validate before saving, and its report names the chain.
- The editor gained the ability to author things it does not place: a document with one root and no
  camera is a normal document with a narrower contract.
- Thumbnails are not decided here; if the palette needs pictures, the editor can write one when it
  saves a part document, and the directory application only displays it.
- Kinematics — a hinge that opens a door, a runner that moves a drawer — is a separate mechanism and
  is not part of this record.

## References

- ADR [0034](0034-nodes-carry-typed-properties.md), [0094](0094-a-class-registry-gives-the-machine-its-semantics.md),
  [0096](0096-before-the-first-release-the-format-has-no-past.md), [0120](0120-sheets-and-money-are-additive-in-the-package.md),
  [0148](0148-a-nomenclature-directory-is-a-set-of-packages.md), [0150](0150-machining-is-derived-and-rules-are-tables.md).
- `docs/domain/nomenclature.md` — items, packages and the part-document contract.
- `docs/domain/document.md` — the scene file and the instance node.
- Plan: O10, O13, O14.
