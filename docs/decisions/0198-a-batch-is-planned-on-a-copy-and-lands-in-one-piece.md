# 0198. A batch is planned on a copy and lands in one piece

- Status: Accepted; implements the session operation 0190 asked for
- Date: 2026-11-20

## Context

ADR 0183 gave a recipe three promises — one edit scope, all or nothing, and a `dry_run` that writes
nothing — and ADR 0190 said the third must become an operation of the session, because a plan a
person approves and a run the command line performs have to be the same answer. Measured before this
record: all three lived in `app/src/cli/recipe.rs` — a loop of `Session::dispatch_in` between
`begin_edit` and `end_edit`, keyed to a file on disk, with a `CliError` of its own and no report at
all. `crates/mcp` does not exist yet, so nothing on the session could run a list of commands, and the
"all or nothing" the CLI keeps is the shell's: a headless run that fails throws its session away, so
nothing is written. A batch inside a window cannot throw the session away.

Two mechanisms were on hand and neither answers alone. `Session::cancel_edit` restores the content of
an edit that failed, but it goes through `Project::restore_from`, which advances the revision on
purpose (caches key on it and it never moves backwards), so the document would be changed and changed
back: "untouched" would be a claim about the content only. And nothing anywhere counted what a batch
created, changed and removed: the CLI printed one line per step and the summary ADR 0190 promised had
no owner.

## Decision

- **A batch is one operation of the session.** `Session::batch(&[BatchStep], BatchOptions) ->
  BatchReport` in `crates/engine/src/batch.rs`: the step is a command id plus the JSON its own parser
  takes, the options are two switches, and the answer is a report. The command line calls it today,
  the tool that serves a model calls the same function in J5, so the two doors cannot disagree.
- **Two switches, not one.** `atomic` speaks about what a refusal costs, `dry_run` about whether the
  document is written at all; the four combinations are four honest answers, and a flag that meant
  both would have to be read twice.
- **The steps run on a copy of the session.** `Session::trial` builds a session that shares the
  directory and the contact adapter (both are `Arc`s) and starts from the same document, saved marker
  and open product, with an empty history. The copy is installed as one undo entry only when the batch
  may land. That one decision buys all three promises without a new revision:
  - a refusal under `atomic` installs nothing, so the document, its revision, its history and its
    saved marker are exactly as they were — not restored, never changed;
  - `dry_run` installs nothing either, so it costs no undo entry and no revision, and the report is
    what would have landed;
  - a batch that keeps what it applied installs the steps that ran, as one entry, and the report says
    so instead of claiming the whole list applied.
- **The report names ids, not steps.** `created`, `changed` and `removed` are document ids read from
  the two documents' own walks, so a command that renames, moves or deletes is reported like one that
  creates and no command has to describe itself. A container is compared by its own fields
  (`Container::same_node`) and never by its children, because a container's value carries them: a
  panel born inside a product reports the panel and not the product as well. Where no id names the
  change — the active camera — `content_changed` answers, and it is the same predicate that decides
  whether the copy is installed, so the report and the document cannot drift.
- **A refusal is an answer, not an error.** The report carries the command's own `CommandError` and
  the number of the step that raised it; the command line turns that into its exit code and a tool
  returns it as data, which is the "readable answer" of ADR 0190. The sentence is the command's own,
  with no batch words added.
- **A batch stops at its first refusal.** The steps after it were written against a document that
  does not hold the step before, so running them would be running a program whose earlier lines
  failed. A caller that must past a failure sends two batches.

## Consequences

- A refused batch costs one document clone and nothing else: no revision, no history entry, no dirty
  flag. A landed batch costs one entry and the revisions the commands themselves made.
- The copy takes the document twice — once as content and once as the saved marker the session keeps
  beside it — which is one clone more than a single dispatch takes. A batch is a turn, not a frame.
- `BatchEffect::RolledBack` says "nothing of this batch is in the document". That is also the answer
  when the very first step refuses under `atomic: false`, so the variant describes the document and
  not the mechanism.
- The command line keeps `atomic: true` and grows no flag: a recipe that half applies is the scene
  nobody asked for. The tool exposes both, because a turn that stops between calls may keep what
  worked (ADR 0187).
- The report is data with ids and sentences but no `serde` derive yet: `CommandOutcome` and
  `CommandError` are the engine's own values. The door that serves a model chooses the wire form in
  J5 — the command line prints lines, a tool prints an object — and this record says so rather than
  pretending the report is already JSON.
- `Container::same_node` is the model's rule, not the batch's: any caller that asks "what did this
  command change" needs it, and a container that answered by its children would answer "everything
  above" every time.

## Related

- [0183](0183-the-command-line-is-the-one-headless-door.md) — the three promises and the recipe this
  record moves onto the session.
- [0190](0190-plan-and-build-are-the-persons-switch.md) — "`dry_run` becomes a first-class operation
  of the session", and the refusal that is a readable answer.
- [0187](0187-a-chat-turn-is-one-undo-entry-and-a-plan-first.md) — one turn, one undo entry, and a
  stop at the call boundary.
- [0192](0192-a-recipe-is-rust-that-emits-commands.md) — the other kind of batch, where one command
  builds the whole carcass.
- `AGENTS.md` §4.1, §4.11; `docs/domain/prototypes.md` §8.
