---
name: design-operation
description: "Design one Z3L operation end to end: the armed state machine, the preview before the commit, the typed number, the cancel, the single undo entry, the feedback budget, the cursor and the awkward cases. Use when a gesture, a drag, a mode or a tool appears or changes — a drawing tool, a gizmo, a previewing command, a scrub or a transient atom."
license: MIT
compatibility: pi
metadata:
  audience: agents
  workflow: interface
---

# Design one operation

The feel of a professional editor lives in the operation, not in the panel that starts it: what the
pointer shows, what a number does while it is typed, what Escape restores, how many undo entries one
gesture costs. This is the procedure for one operation — a tool, its methods and its states. It
takes the place in the inventory from the `place-capability` skill and the surface rules from
`ui-design`; it owns the state machine between them.

## The five-line brief

The design skill's decomposition is the form; fill every line in our words before code:

```text
intent:      what the person is trying to achieve, in the domain's words
input:       which button, modifier, click or drag; what starts it, what cancels it
feedback:    what changes on screen, in which order, within which budget
recovery:    what Escape, a second click or one undo restores
edge:        the awkward case — nothing selected, two objects, a locked or nested node
```

"As in Blender" is refused: it is a quality bar, not a decision. An operation that cannot fill the
five lines is not ready, and the line that is empty is the question to the owner.

## The six artefacts

### 1. The state machine

Name every state and every transition before the first line of code:

```text
idle --arm--> armed --pointer--> collecting --commit--> idle
                 |                   |
                 +----- Escape ------+--> idle   (nothing was written)
```

- One owner holds the arm — `crates/ui/src/editor/` (`fill_arm`, `pending_primitive`, `box_draft`,
  `drawing`) for the mode, the shell for the ephemeral draft (`app/src/document.rs`,
  `handle_keys`).
- The key path is ordered, and every step answers Escape: focused field, modal, running tool,
  container level, selection. A new tool joins that order; it does not add a parallel key reader.
- A tool armed under a dialog or a palette must not take the next click; `handle_keys` receives the
  modal answer for exactly that reason.
- Starting another tool ends the running draft. Two armed modes at once is a defect.

### 2. The preview, before the commit

- A changeable value fills the session overlay (`Session::set_preview_geometry`, `clear_preview` in
  `crates/engine/src/session.rs`), never a document field (ADR 0093). The viewport draws the preview
  instead of the node's geometry, and an undo stack that never saw it stays honest.
- A preview with a number shows the tool card (`crates/ui/src/tool_card.rs`) beside the pointer, and
  the same rows in the inspector; it writes nothing and reports the numbers and the button pressed
  this frame (ADR 0119).
- Feedback starts on the press frame. A rebuild that overruns `SLOW_REBUILD` (250 ms, measured in
  `app/src/document.rs`) says so on the status bar before it finishes; it never blocks
  the frame and never runs on the UI thread once per pointer move.
- A preview is not a second renderer: it travels as geometry through the same scene and the same
  pick, so what the person sees is what the commit produces.

### 3. The typed number

- One field: the shared scrub (`crates/ui/src/scrub.rs`, ADR 0092). A bespoke drag arithmetic is a
  second numeric field and a defect.
- The unit is outside the box; the term is the glossary's; Enter commits the field and then the
  operation, once. Escape while the field is active returns that direction to the pointer instead of
  dropping the tool (ADR 0103, `crates/ui/src/distance.rs`).
- A typed value and the pointer must agree: typing moves the preview, and moving the pointer updates
  the field. Neither is the source of truth for the other.

### 4. The cancel

- Every state answers Escape with one well-defined result, and a cancel writes nothing that undo
  would have to remove. A draft never touched the document, so it costs nothing.
- An edit scope that did write (a live resize, a grip drag) is rolled back on cancel; the inverse is
  captured before the first write, so the rollback is exact.
- A refused commit keeps the draft and the tool alive with a readable error, so the person can fix
  the input or cancel; it never silently drops the work.
- Escape never leaves two levels at once: the tool, then the creation parent, then the container
  level, then the selection.

### 5. One undo entry

- One operation, one `Revision`, one undo entry: the captures happen before the mutation and the
  inverse restores ids and geometry bit for bit. A drag that wrote per frame is a broken operation,
  not a fast one.
- The commit is idempotent: the apply button disappears with the bar, so a second press has nothing
  to hit.
- Cancelling a preview leaves undo untouched; committing adds exactly one entry, and undo after the
  commit restores the pre-command state with no preview left behind.

### 6. The cursor and the guides

- Inside the work area the operation shows its own pointer and its guides, drawn in the scene under
  the chrome (ADR 0121, 0122, 0123); every panel, field and border keeps the system icon. The pointer
  says what the next click does, not what the last one did.
- A snapped point is visible as a marker and a guide before the click; a snap that only changes the
  result of the click is invisible and therefore a bug.
- The status bar carries the words: the left zone says what to do next, the centre zone the live
  value (`app/src/status_bar.rs`).

## The awkward cases

Answer each in the brief, not in a bug report: no selection and no node under the pointer; two or
many objects selected; a nested container level; a locked or hidden node; a zero-length or inverted
value; a draft whose next click would self-intersect; a tool armed while a text field has focus; a
dialog or palette opened mid-operation; a second tool chosen while a draft runs; the same operation
reached from a menu instead of the ribbon.

## Where an existing operation lives

| Operation | State | Files |
|---|---|---|
| Drawing draft (line, circle, arc, polyline) | `crates/ui/src/editor/` (`drawing`), `crates/ui/src/draw.rs` | `app/src/draw_gizmo.rs`, keys in `handle_keys` |
| Box draft (frame and cuboid) | `crates/ui/src/editor/` (`box_draft`) | `app/src/box_gizmo.rs` |
| Extrude pull | arm in the editor, preview in the session | `app/src/extrude_gizmo.rs` |
| Move and rotate gizmos | drag with a committed scope | `app/src/translate_gizmo.rs`, `app/src/rotate_gizmo.rs`, `app/src/gizmo_math.rs` |
| Contour grips and vertex drag | `crates/ui/src/contour_grips.rs` | `app/src/contour_gizmo.rs`, `app/src/vertex_gizmo.rs` |
| Fill arm and primitive arm | `crates/ui/src/editor/` (`fill`, `creation`) | `app/src/fill_gizmo.rs` |
| Exact distances | probe value, live field | `crates/ui/src/distance.rs` |
| Work cursor | hover and arm | `app/src/work_cursor.rs` |

A new operation that shares one of these mechanisms uses it; it does not copy it (ADR 0118). If the
mechanism is private in one tool and a second needs it, that is the atom to extract — and the row is
filed with `place-capability`.

## Verify

Silent by default: drive an `egui::Context` with `RawInput` and assert the rectangle, the armed
state, the field focus, the Escape result, the revision and the undo depth. What only the eye can
judge — the feel of the drag, the snap marker, the pointer at its drawn size, the preview over the
model — needs an offscreen render or a live run, and the task says which one was used (AGENTS.md
§4.7).

## Stop and ask the owner

- A new mode, a second way to do one thing, or a gesture that collides with an existing one.
- Removing an affordance, or a new default the person meets on first launch.
- A number the operation needs that the document does not describe (AGENTS.md §4.9).

## Checklist

- [ ] The five lines are filled, in our words, before the code.
- [ ] Every state and transition is named, with one owner and one key reader.
- [ ] The preview is session state; the document sees nothing until the commit.
- [ ] The number is the shared scrub, the unit is outside, Enter commits once.
- [ ] Escape answers in every state and writes nothing that undo must remove.
- [ ] One operation is one revision and one undo entry, and the commit is idempotent.
- [ ] The pointer says what the next click does; the snap is visible before it acts.
- [ ] The awkward cases are decided, not discovered.
- [ ] A shared mechanism is a named atom, not a second copy.
- [ ] The inventory row exists (`place-capability`), and `cargo xtask ci` is green.

## Anti-patterns

- Logic in an egui event handler; a write to the document outside a command.
- A preview implemented by mutating the document and undoing it later.
- A per-frame undo entry, or an inverse captured after the first write.
- A private drag arithmetic, a private snap list or a private probe.
- A tool that swallows Escape from a focused field, or a modal that lets a tool take the click.
- A mode that is only visible in the status bar, or a cursor that changes after the act.
