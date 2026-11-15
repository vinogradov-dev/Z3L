---
description: "Take the scheduled task, work it under the project contract, and keep the time log a later review reads"
argument-hint: "[task id — empty takes the one `STATE:` names]"
---

Take the task ${@:-that `cargo xtask next` schedules} to a commit and a closed plan entry, and keep
the log below while you work. The log is not decoration: a later session reads it to find out why a
task that looked like two lines took half an hour, and it is the only evidence that outlives the
session.

## Start

```text
cargo xtask next
cargo xtask task <ID>
```

When the plan says `state=doing`, a previous session stopped mid-task: `plan/SESSION.md` names the
live step in `now:`, and that is where you resume — not at the top of the entry. Then read the first
path of the `files:` hint and its direct callers (the `rust-analyzer` tools for signatures and
references, `rg` for strings), the notes and ADRs the entry cites, and — before any edit — the skill
`AGENTS.md` §1.3 routes you to: `.pi/skills/ui-design/SKILL.md` for anything under `crates/ui` or the
app chrome, `.pi/skills/place-capability/SKILL.md` when the change adds, renames or removes a
command id or a place, and `.pi/skills/design-operation/SKILL.md` when it adds a gesture or a mode.
That route is not decoration: both repairs of the reviewed period (X41, X42) exist
because the session read the entry and the code and never the skill, whose floating-surface bullet
already answered the question. Do not scan the workspace: when the hint is wrong, the fix is the
hint, in the same change.

## The log

`## Log` is the **first** section of the note `plan/notes/<ID>.md` (start it from
`state-contract/templates/NOTE.md` when it does not exist), and it is written as you go — one line
per stop. A note that already exists keeps every section it has: `## What went wrong upstream` and
`## The sentence that would have caught it` are a repair's record of the tasks that failed before
it, and a feature session never edits them.

```text
+m · phase · what · why [· attribute]
```

- `+m` is the wall minutes **since the previous line**, written at the stop from a clock you can see;
  a `~` in front marks a guess. The column adds up to the task's total, so there is no total to quote
  and no separate cost field, and no stamp can be written out of order. The first line is always
  `+0 · start`.
- `phase` from this closed list, so the review can sum the column: `start`, `read`, `search`, `edit`,
  `build`, `test`, `gate`, `decide`, `owner`, `close`, `wait`. `owner` is a turn that came from a
  person; `close` is the map row, the commits and the plan write.
- `what` is a path, a symbol, a command or a number — never "looked around".
- `why` is the question that sent you there, in four words.
- `attribute` is optional and is never a phase, because a loss counted twice is a loss no column can
  read — that is how the first version of this format summed to 128 %: `dead-end` for work that
  produced nothing, `red-mine` or `red-tree` after a red test or `clippy`, `guessed` for a `~` delta
  you could not reconstruct.

Two things are mandatory, because they are what a review can act on:

- **`dead-end`** — anything read, tried or built that produced nothing. The most valuable attribute
  in the log: it is where the time went, and the next session would repeat it for free.
- **the first file the `files:` hint did not name** — with the reason you opened it. The count of
  these across tasks decides whether the hints or the map are the real problem.

Log every `clippy`, `test` and `ci` run as its own line with its duration in `what` (`wait` covers a
build you did not provoke), and every change of plan with the observation that forced it. Never
reconstruct the log at the end: a delta invented afterwards makes the whole column worthless, and the
`guessed` attribute is the honest way out when a session has lost the thread.

The note is at most 150 lines with the log inside it: compress to one line per loop rather than one
per file, and never drop a `dead-end`.

## Close the log before you close the task

Three sections at the end of the note:

1. `## Where the time went` — a tally `phase | minutes | share` that adds up to 100 %, a verdict line
   each; then `dead-end`, `red` and `guessed` as separate counts, never rows of that table; then files
   read, commands run, gate runs and how many were red at the first try.
2. `## The cheap path` — one to three lines: what would have made this task one edit. Name the
   missing line, symbol or doc sentence and where it should live, not "better docs".
3. `## For the strategy` — three sentences at most, addressed to whoever writes the next agent
   instruction. No praise, no narrative.

`cargo xtask task <ID>` prints the note beside the entry, so the next session reads these for free.

## Work

`AGENTS.md` is the contract; a long session forgets three of its rules first:

- §1.5 — order the work by the cost of being wrong, and gate the structural decision (a signature, a
  type, both end points of a new number) on `clippy` before propagating it to callers.
- §6.1 — commit each completed operation; the commit is the rollback point.
- `cargo xtask ci` once, at the end; `clippy -p <crate> --all-targets` and that crate's tests while
  iterating.

Keep `now:`, `run:` and `resume:` current with `cargo xtask task step` after every green run, so a
session that dies loses one sub-step. Close with `cargo xtask task close <ID> --commit <hash> --next
<text>`; the plan files are the tool's business, never a text edit.
