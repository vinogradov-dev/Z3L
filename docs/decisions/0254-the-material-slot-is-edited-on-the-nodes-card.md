# 0254. The render material slot is edited on the node's own card, by name

- Status: Accepted; closes the command and the card 0245 left open, and makes room for 0252's
  picture placement beside it
- Date: 2026-11-26

## Context

ADR 0245 made the render material document data — `model::MaterialRef` on a panel and on an object —
and said the command and the card that assign it are a task of their own. That task is U129, and
until it landed a shop could only name a material by editing the scene file: the frame resolved a
slot nothing in the window wrote.

Three facts shaped the answer. A panel already has a **catalogue** material row (the `ItemId` that
decides its thickness and its cut list), drawn as a combo box over `Directory::boards()` with a "No
material" row and a danger sentence for a record the catalogue lost. The **object card has one
reading order and no headers** (ADR 0116), so it cannot grow a section. And the **palette cannot
guess a record**: it builds parameters from the selection, and `node.rename` already refuses with a
sentence that names the card instead of inventing a value.

## Decision

- **One command, `node.material.set`.** `{ node, material: Option<MaterialRef> }`: a slot or its
  absence, on a panel or an object, never the root, never a container. One revision and one undo
  entry. A reference the workshop's library does not hold is **stored, not refused** — a scene
  travels between workshops with the numbers it named (ADR 0245) — and the frame draws the
  placeholder tone.
- **The row lives where the node does.** The panel card draws it as a `Surface` section directly
  after its catalogue `Material` section; the object card draws the same row, with its label, between
  the head and the numbers. Both call one function (`crates/ui/src/material_slot.rs`), so the two
  cards cannot offer two different lists, and 0252's picture placement joins the same place.
- **The picker lists the library by name.** A combo box whose first row is the clear ("No material")
  and whose other rows are `Session::materials()` in the library's own order: names are unique in a
  library, so a name is enough. An empty library says so once under the row instead of opening onto
  nothing. A slot whose record the library lost keeps its number, prints `Missing material 7` as the
  face and one sentence in the danger ink that says to choose another — a placeholder tone is drawn,
  never a refusal and never a silent swap.
- **The palette names the door.** The inventory lists the id in the palette, and the palette refuses
  it with a sentence that sends the person to the node's card, the way rename does. An entry that
  silently picked a record would be a guess about the workshop's library.
- **The row carries the `actions/material` icon** for the surfaces that draw artwork; the card row
  itself prints the record's name, and no destructive action is ever an icon alone.

## Consequences

- The scene file is unchanged: schema 33 already writes `render_material` on a node that names one.
- Assigning a material moves the document revision, so the frame, the tracer and the cut-list stage
  rebuild through the cache they already key on a revision; a library edit still moves only
  `materials_version` (ADR 0245).
- Y17's decor mapping row now has the place 0252 promised it: next to this one, in the same
  selection of the same node.
- A face list — a different material per face — stays out; that is O18's composition and not a
  second slot here.
