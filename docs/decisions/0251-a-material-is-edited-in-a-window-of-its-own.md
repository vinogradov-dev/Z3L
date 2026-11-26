# 0251. A material is edited in a window of its own, opened from the render place, and a deleted record leaves the reference behind

- Status: Accepted; replaces "a page of the Settings window" from ADR 0244's consequences, and shapes
  Y14 in the render stream
- Date: 2026-11-26

## Context

ADR 0244 decided what a render material is, where it lives (`library.json` in the workshop's settings
folder, with its `textures/`) and that nothing edits it: a shop wrote the file by hand and dropped its
scans into the folder. It filed the editor as its own task, and Y14's entry first placed it as a page of
the Settings window — where the workshop's workspace already lives.

The owner's ruling, 2026-11-26: **a material is edited the way every graphics program edits one — in its
own window, opened from the render place — and never inside the settings.** The settings hold how the
*workspace* looks; a material is what the render is made of, and a person who says "this board is wrong"
is looking at the Render place when they say it.

The owner also ruled what a deletion means: **a material assigned to an object may be deleted, the
object keeps the reference, the row that names it is shown in the danger ink, and the object is drawn
with the default material.**

## Decision

- **The editor is a window of the shell, opened from the render place's rail.** `ui::material_window`
  owns the `egui::Window` (one row of `HOST_OWNERS`), the list of records, the record's fields and the
  swatch; the rail's *Materials…* row opens it and Escape closes it before the place behind it. It
  floats, so the person keeps the picture in sight while a decor is fixed, and it outlives a tab switch
  the way the render place does — the library is the workshop's, not a scene's.
- **The library is edited live, one atomic write per settled act.** A switch, a picture and a new or a
  deleted record are one act each and are written at once; a number or a name waits 0.4 s of quiet, the
  same pause the one scrubbing field waits for (ADR 0092). The window is a view: it returns an act —
  file this record, remove this one, ask for a picture — and the shell validates the whole library,
  writes it through `materials::Store` and swaps the snapshot into every open tab. The document does not
  move: no revision, no undo entry, and every frame is rebuilt from the snapshot (`materials_version`).
- **Deleting is allowed and the references stay.** A node keeps the `MaterialRef` it carries; the frame
  resolves it, finds nothing and draws the object with the shell's default tone, which is the answer a
  missing record already had (ADR 0245). The window lists every id a document still names that the
  library no longer holds, in the danger ink, and a click on that row starts a record under the same
  number — the way a maker puts a material back on the objects that want it. Nothing rewrites a scene.
- **A picture is chosen, never typed.** The picker gains a read order filtered to PNG; the store decodes
  the chosen file, copies it into `textures/` under a free name (`oak.png`, `oak-2.png`) and answers the
  relative path the record carries, so a second record never replaces the first one's scan.
- **The swatch is the colour and the picture tiled at the record's own coverage and turn**, drawn by
  `ui` from texels the shell decoded: `ui` never reads a file, and the preview is a flat swatch and not a
  traced board, because `ui` never sees `wgpu`.

## What we do not decide here

- The objects' own slot: what a card shows for the material a node wears, and the command that writes
  it, are U129's.
- A per-object decor mapping (a scale, an offset and a turn on the object rather than the record) is
  Y15's, and the tiling grid in the viewport is Y16's.
- A second picture format, or a picture read from anywhere but the library's folder.

## Consequences

- The material window is a second surface beside the render place, so a later editor of the same library
  (a card, a browser) is a place in this window rather than a new file.
- `library.json` is written by the window's own act and by a hand edit alike; the watcher keeps working,
  and a write the window made comes back through it once, which costs a frame and never a wrong picture.
- A deleted record is a state the documents can hold: an id with no record. It is drawn as the default
  material and it is visible in the window, so it is a fact a person can fix rather than a silent hole.
- The reference does not break the file: an id is validated on load as it always was, and a library that
  no longer holds it is a diagnostic and a default tone, not a refusal.
