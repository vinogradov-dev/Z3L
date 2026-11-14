---
description: "Read the time logs of finished tasks, name where the sessions lose it, and propose the instruction change that removes each loss"
argument-hint: "[how many finished tasks to review, empty for every logged one]"
---

You are the reviewer. This session is read-only: no `edit`, no `write`, no `bash`. Your whole output
is text — a verdict and a small set of proposed instruction changes, which the owner files as tasks.

## Read

1. `plan/BACKLOG.md` — the ids under `## Done`, newest first: these are the tasks to review
   (${@:-every one that has a log}).
2. `plan/notes/<ID>.md` for each of them — the `## Log`, `## Where the time went`, `## The cheap path`
   and `## For the strategy` sections, plus `## What went wrong upstream` and `## The sentence that
   would have caught it` where a repair wrote them. Name how many notes predate the log format and
   exclude them; a review over three tasks says so rather than pretending to a trend.
3. `AGENTS.md`, `.pi/prompts/next-task.md` and `.pi/prompts/fix.md` — the instructions the sessions
   were given, so a finding can say whether a rule was missing or present and ignored.

## Add the columns up

- **Phase totals** across the reviewed tasks, and per task: total, and the worst single stretch.
- **Dead-end reasons**, grouped. A group is a mechanism, not an anecdote: the same wrong assumption
  in four tasks is one finding, not four.
- **Hint quality**: how many files each task opened before its first edit that the `files:` hint did
  not name, and why. Also: which tasks changed their `files:` hint, and what the hint missed.
- **Gate cadence**: `ci` runs per task, minutes in `wait`, and tasks whose first `clippy` or test run
  came back red — that is the measure of "edited before deciding".
- **Search shape**: `search` before `read` of the right file, or `read` of a whole file where a
  symbol would have answered.
- **Rework**, counted separately and never folded into the phases: every note carrying `## What went
  wrong upstream` is a defect the first gate let through. Its bullets name the task that shipped it,
  so charge its minutes to that task as well as to the repair, and list the repair's own total — a
  repair is a task whose whole cost is wasted work.

## Classify every loss

Each finding gets exactly one mechanism from this list, so the table can be ordered:

`hint` (the entry named the wrong file), `map` (no row led to the code), `api` (the signature was
unknown and cost a compile loop), `doc` (a stale or missing sentence sent the session wrong), `gate`
(edited before deciding, so the first build was red), `fixture` (a harness was rebuilt that existed),
`size` (a file too large to read in the range), `env` (build, toolchain or test slowness), `over-read`
(nothing learned, time spent), `scope` (the task was two tasks and the entry hid it).

A rework finding arrives with the defect vocabulary of `/fix` — `shape`, `done`, `verify`, `context`,
`contract`, `review` — and keeps its own row rather than being forced into a loss token. The shared
words mean the same thing in both tables, and a mechanism that appears in both is the most valuable
row in the report: it costs minutes and quality at once. Add a token to one list and to the other in
the same change, or the two vocabularies drift apart and the tables stop comparing.

## Answer with these four parts

1. **The table**: mechanism | minutes | tasks (ids) | what it cost in one line. Ordered by minutes.
   A number without task ids behind it is a guess — say `~` or leave it out.
2. **Three to five changes**, each one line of the instruction it edits. For every one: the file and
   the section, the sentence to add, the sentence to delete, the tasks that evidence it, and the
   minutes it would have saved. Name the smallest set, ordered by saving per line of instruction.
3. **The rework verdict** — how many of the reviewed tasks needed a repair, which upstream task each
   repair blames, and the share of the period's minutes that went into writing what had to be
   written twice. This paragraph decides more than the table does: a task that took forty minutes and
   shipped is cheaper than a task that took ten and was repaired.
4. **The one thing to stop doing** — a habit the log shows, with the ids.
5. **What the log could not answer**, and the line to add to the format so the next review can.

## Rules

- A change that adds a rule without deleting one is refused. The instructions are already long, and a
  rule nobody can find is a rule that does not exist.
- If `AGENTS.md` already says it and the session ignored it, the finding is not "say it again": the
  fix is where the sentence sits, or the command that would have made ignoring it impossible. Say
  which.
- No finding rests on one task unless the loss is large; a mechanism seen once is a note, a mechanism
  seen three times is a change.
- Do not propose code, tests or file edits — describe them. The owner decides, and `/plan-task`
  files what he accepts.
