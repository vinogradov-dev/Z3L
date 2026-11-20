# 0182. The file manager chooses the path, and the window keeps drawing

- Status: Accepted
- Date: 2026-11-20

## Context

*Open* and *Save as* asked for a path in a text field drawn by the editor, with a list of nearby
`.z3l` files beside it. The owner reported the field as unusable: one Backspace deleted two
characters and moved the cursor by two, so a path could only be edited by retyping it. The field
also meant the build needed no native dialog, which is why it was written that way (X91).

A path is chosen in a file manager, not typed into a string. What was missing was a binding that
respects the two rules a dialog must not break: no dependency that compiles C (§2), and no `unsafe`
in our own code, which `unsafe_code = "forbid"` makes absolute. Hand-written platform FFI was
therefore not an option, and neither was GTK: it is a second GUI toolkit (§2).

## Decision

- **`rfd` is the binding**, with `default-features = false, features = ["xdg-portal", "wayland"]`.
  It is MIT, pure Rust, and compiles nothing: on Linux it talks to the desktop portal over D-Bus,
  loading `libdbus` at run time; on macOS it drives `NSOpenPanel`/`NSSavePanel` through `objc2`; on
  Windows it drives the common item dialog through `windows-sys`. The `gtk3` and `gtk4` backends
  stay off — that is what keeps it from being a second toolkit.
- **The dialog runs on a worker thread** (`z3l-file-manager`), and the frame takes its answer at the
  next frame boundary, beside the directory reload it already takes there. A file manager is
  drawn by another process; making the frame wait for a person to browse would freeze the window,
  and a frozen editor is the defect this task replaces one with. The worker wakes the frame with
  `Context::request_repaint`, so nothing polls and a still window costs nothing.
- **One pick at a time.** A second *Open* while the dialog is up asks for nothing: one gesture, one
  dialog, one answer.
- **The order carries everything the dialog needs** — mode, the folder of the document in front of
  the person, the name the save field offers and the language — so a test asserts the whole request
  without a window: filtered to `z3l`, started where the document lives, named after it.
- **The pick belongs to the tab that asked** (ADR 0081). The answer activates that tab before it is
  performed, so a save asked for in one scene never lands in another one activated behind the dialog.
- **A cancel is an answer.** The dialog is closed, so nothing is opened and nothing is written, and
  the status bar stays quiet. A picked save whose name carries no extension gets `.z3l` appended: the
  dialog filters by it but does not add it, and a scene saved as `cabinet` must be visible to the
  next *Open*.
- **The text field and the nearby-scenes list leave with it.** The system dialog has its own title,
  its own name field and its own *Cancel*, so nothing of the old modal survives; *Open* and *Save as*
  ask the file manager directly, and `crates/text` drops the five messages that only the modal drew.

## Consequences

- The owner's gesture is the one every other desktop program offers: a filtered picker, a folder that
  follows the document, a name that follows the file. The double-delete defect cannot occur, because
  there is no text field.
- The frame path keeps its budget: no pick costs anything, and one pick costs one channel send and
  one repaint request.
- The picker's own failure is readable — a worker that cannot start says so in the status bar — but a
  missing `xdg-desktop-portal` is not: `rfd` reports it as `None`, the same answer as a cancel. The
  gap is filed in `plan/debt.md`.
- The dialog is not yet owned by our window: `rfd` takes a parent handle and we pass none, so on
  Windows and Wayland the file manager is not a sheet of the editor's own window. The dependency is
  there for it; it waits for a measured need.
- `pollster` moves from a test-only dependency to the shell's: the worker blocks on one future, which
  is what `pollster` exists for and which the tree already carried.
