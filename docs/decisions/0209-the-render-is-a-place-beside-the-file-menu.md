# 0209. The final render is a place beside the File menu, not a dialog

- Status: Accepted; the surface ADR 0208's entry point is reached from
- Date: 2026-11-21

## Context

The final render exists and can be reached from the command line (`z3l render`, ADR 0208), and the
owner asked for it in the interface: the render has to be one tab in the window, next to the File
menu. The tree already had two shapes for a surface that is not the document — a modal (Home, the
Settings window, a destructive question) and a docked place (the workspace: viewport, ribbon,
floating panels) — and the choice between them decides how a job that runs for minutes behaves.

A render is not a question and not a confirmation. It takes minutes, its progress has to be visible
while the person does something else, and a dialog that owns the keys and the frame while a plate is
drawn would make the window useless for the length of the job. It is also not a document: it writes a
file and changes nothing in the scene.

## Decision

- **A tab of its own, first in the tab row, beside the File menu.** It is drawn with the document
  tabs' metrics — the same margins, corner radius, accent fill and bottom bar — but it has no close
  cross and no unsaved marker, and its word never elides: it is a place, not a file. While it is
  open it is the pressed tab and no document tab is, because the workspace shows the place.
- **Opening and closing is one toggle.** A press opens it; a second press, Escape, or a press on a
  document tab goes back to that document. Nothing loads, nothing is discarded, and no revision
  moves: the tab is view state.
- **The place replaces the workspace, and nothing floats over it.** The ribbon, the viewport, the two
  floating panels, the view cluster and the tool card are not drawn while it is open, and the chrome
  the shell reports to the pointer is empty (`Chrome::empty`), so a press on the page is the page's
  own. The header, the document tabs and the status bar stay: the person can still switch documents,
  read a message and reach Settings.
- **The frame is the viewport's, taken once.** Pressing Render clones the frame the viewport draws —
  the same triangles, the same lights, the same camera — and the document's revision, and hands them
  to the worker thread. An edit made while the plate is drawn cannot change the image under it, and a
  second press of the same document reuses the tracer's BVH by that revision.
- **The run is a thread, the progress is a number, the cancel is a flag.** The page reads
  `final::Progress` between frames and sets the same value's cancel from its button; the finished
  answer is taken at the frame boundary, like a directory or a file-manager answer. The place keeps
  its numbers, its destination and its last report while the person works in another document.
- **The page edits numbers through the one field (§0092), a preset fills three of them, and the
  destination is chosen in the system file manager (§0182)**: a new `PickMode::Image` asks for a PNG
  in the folder the scene lives in, offered under the scene's own name. A render with no destination
  cannot start, and the button says why rather than failing a minute later.
- **The caps are the tracer's own, handed in.** `ui` cannot depend on `final` (it would pull `wgpu`
  into the interface crate), so the shell passes `final`'s ranges as `render_page::Bounds` and the
  page clamps every number inside them.

## Consequences

- A render no longer needs a window-wide modal: the person watches the percentage, goes back to the
  tree, and finds the result where they left it.
- The tab row holds one thing that is not a file. The header's contract is unchanged — it returns one
  action and the shell decides what a tab means (§0064) — and the row's own rule ("one tab per open
  scene", U45) now reads "one tab per open scene, and one place beside them".
- The page is where Y2–Y5's knobs land: an aperture, a clamp, a denoiser switch and a preview of the
  last image all belong to this surface, and the preview is the first of them (it needs a texture the
  shell registers, which is why it did not come with this record).
- A guest surface in the header is cheap to add and easy to overuse: the only place that earns one is
  a long job that is a view of the document rather than a question about it.
