# 0183. The command line is the one headless door into the document

- Status: Accepted
- Date: 2026-11-20

## Context

The assistant and the prototype expander both need the document without a window: a task builds
parts by dispatching commands and then asks the scene what it holds (`docs/domain/prototypes.md`
§8, ADR 0028). Measured before this record: `Session::dispatch` and `dispatch_in` existed and each
command parsed one JSON object into a typed `serde` struct with `deny_unknown_fields`; `engine` still
had no `wgpu` or `egui`. But nothing outside the process could call them — `app/src/cli.rs` offered
`export` and nothing else. A generator that wrote the scene file directly would have bypassed
`Session::dispatch`, and with it the undo stack, the class guard of ADR 0098 and every validation a
command performs before it mutates.

`export` already proved the shape works: a subcommand is read and performed in `main` before eframe
creates a surface, so the same binary does a headless job with no GPU and no person (ADR 0095).

## Decision

Three subcommands join `export`, and all of them reach the document through the existing door.

- **`z3l commands [--json]`** lists the registry: the stable id, the English label and what the
  command creates — the class level ADR 0098 gates, read through `CommandSpec::creation_level`. It
  is the discovery surface for a recipe author, and it prints no second truth: the ids are
  `CommandRegistry::specs()`.
- **`z3l run <scene> --commands <recipe.jsonl> [--out <scene>] [--product <node>] [--dry-run]`**
  reads the scene once, dispatches every line through `Session::dispatch_in` inside one edit scope,
  and writes the scene once at the end. A recipe is JSON Lines, one `{"command", "params"}` object
  per line, because a line is a step and a diff of two recipes reads as a diff of two runs.
  - **One undo entry.** The whole recipe is one step in the history, which is what a task made of
    several commands needs.
  - **All or nothing.** A step that fails reports its line number and the command's own sentence, and
    nothing is written — a half-applied recipe would be a scene nobody asked for.
  - **One product.** A part is born only inside the open product, and opening one is session state
    rather than a command (ADR 0029). `--product` names one; a scene with exactly one product opens it
    by itself, because one product is no guess. A scene with several and no flag leaves the workspace
    closed, and the command's refusal says so.
  - **`--out` absent means the scene read is rewritten**, atomically through `SceneFile`, with the
    top-level sections this build does not understand carried back unchanged. `--dry-run` dispatches
    everything and writes nothing, so a recipe is checked against a real document first.
- **`z3l query <scene> tree|box|parts|diagnostics|niche [--id] [--at]`** prints one JSON report and
  never writes. Each report reuses the derivation that already owns the answer, so a report and the
  editor cannot disagree: `tree` is the document's own node walk, `parts` is the published `parts`
  table of the order package, `diagnostics` is `Session::panel_diagnostics`, `box` is a product's
  authored envelope with its world origin, and `niche` is `Project::niche` through the shell's own
  contact adapter. The envelope names the query, the file and the revision it read, because a caller
  that stores an answer must be able to say what it is about. Lengths are whole micrometres as the
  document stores them; a world coordinate is millimetres under a `_mm` key.
- **The catalogue is the build's default.** A scene file carries none, so both `export` and the new
  commands open the session the window would open on a fresh start.
- **A cell has one JSON reading.** `Cell::json` is the mapping the JSON Lines writer, the single-file
  package and the `parts` report share.

## Consequences

- An assistant, a prototype expander or a shell script can build and read a document without a
  window, and cannot bypass the command layer: no code path in `cli` mutates a `Project` except
  through `dispatch_in`.
- The engine grew by one accessor (`CommandSpec::creation_level`) and the export crate by one
  method (`Cell::json`), which its own writer now uses; nothing else moved.
- **The report cannot describe the parameters of a command.** Its typed structs have no schema, so
  `commands --json` prints the id, the label and what the command creates, and a caller writes the
  parameters from the command's own source; a wrong one comes back as the command's own sentence.
  The gap stays in `plan/debt.md` and closes when a parameter schema exists.
- A recipe cannot stop halfway, so a long task that must survive a failure has to be split into
  recipes. That is the price of one undo entry, and it is the right price while undo is the only
  recovery a user has.
- `query` measures through `Sight::none()`: a query has no session and therefore no hidden set, so it
  reads every part as visible. A caller that needs a person's view asks the editor, not the CLI.
