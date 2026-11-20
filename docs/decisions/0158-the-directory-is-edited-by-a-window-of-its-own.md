# 0158. The directory is edited by a window of its own, one record per save

- Status: Accepted; implements `docs/domain/nomenclature.md` §12–§14; refines ADR 0148
- Date: 2026-11-18

## Context

ADR 0148 made the directory a set of packages that a consumer reads as one validated snapshot, and
O4–O5 built the crate, the folder store, the hot reload and the derived views. What was missing is the
writer: a folder nobody can edit is a file format, and a supplier's package arrives as JSON a person
has to correct by hand.

Three facts decided the shape.

- **The editor must not write.** The directory is not the document. A write from the editor would sit
  beside the undo stack, the scene's dirty flag and the tab's save, and a person who edited a board
  while a panel was selected would have to reason about which of the two the button belonged to.
- **The folder has readers already.** The editor polls the folder's stamp off the frame path and
  applies a new snapshot whole at a frame boundary (§13), so a writer's job is to make one change that
  a poller sees once, after every file of it is in place.
- **A record is not a row of a table.** Every record is one file with its kind and its properties named
  by slug, so the file a workshop reads says `"thickness": 18000`; a writer that rewrote a table would
  turn a diffable folder into a blob and a merge into a conflict.

## Decision

**The directory application is its own screen, and its one write is one record plus the revision.**

- `catalog::FolderWriter` takes the folder's `lock` file exclusively (`create_new`, never a wait) and
  refuses a second writer with a sentence. The lock names its process, and dropping the writer removes
  it; the reader never locks.
- A save validates first: the candidate directory — every record the folder carries, this one in place
  of the record of the same id — is built and checked with the loader's own registries and rules. A
  value the reader would refuse is a sentence and an untouched folder, never a file the editor then
  cannot read.
- The record is written to a temporary sibling and renamed over `items/<kind>-<serial>.json`, then the
  package's manifest is written last. The manifest is one of the files the reader's stamp watches, so
  the revision moves once, after the record is in place — which is what "the revision is bumped last"
  means for a folder whose revision is a hash and not a counter.
- A record's file name is its kind's slug and its serial, never its name's slug: a rename rewrites one
  file, and a person who edits the same row twice sees one path in `git diff`.
- The screen is `ui::DirectoryEditor`: the item tree, the card of the open record, the kinds and
  properties view, and one Save. The card's fields come from the kind's accepted properties, so a
  workshop's own vocabulary is editable the day its package declares it; a length is typed in
  millimetres through the one number field and converted to micrometres once, when the draft becomes
  a record.
- The window is `z3l directory [<folder>]`, a mode of the one binary. `app` is a binary root without a
  library, so a second `[[bin]]` could not read `settings.json` and would carry a second copy of the
  rule for where `nomenclature/` lives.

## Consequences

- A maker edits a board, saves, and the editor re-cuts the panels that name it at the next frame
  boundary, with the undo history intact. The editor never learns the folder's path: it holds one
  snapshot and one stamp.
- Two windows cannot edit one directory, and a crashed writer leaves a `lock` naming its process: the
  second window says so instead of racing, and the person removes a stale file deliberately.
- A save costs one whole directory validation. That is the price of "a save and a reload cannot
  disagree"; a package large enough for it to be felt is the trigger to give `Directory::new` an
  incremental path, not to weaken the gate.
- The screen has no undo and no destructive action: a record is editable until it is saved, and an
  archived record still resolves, so a wrong edit is a second edit and never a dialog.
- The day `app` splits into a library and thin binaries, this window moves to its own `[[bin]]`
  without a behavioural change (`plan/debt.md`).
