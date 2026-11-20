# 0176. The plan shows a stream, and one line keeps one writer

- Status: Accepted; changes the plan contract `state-contract` enforces, and amends the
  one-live-task rule of ADR 0085
- Date: 2026-11-19

## Context

The plan holds exactly one live task. That was true while one session worked one checkout, and it
stopped being true when the tree split into four streams — the application on `dev`, `stream/agent`,
`stream/catalog` and `stream/prototypes` — each in its own worktree (AGENTS.md §6.2). The rule that
`plan/` has one writer, the primary checkout, is right and stays: four branches must not fight over
one backlog file. But it left the state line saying `idle` while three other branches were mid-task,
and a stream's progress lived only in its own commits, which nobody reads at the start of a session.
The worktree rule papered over the gap; the plan did not know about it.

## Decision

**A stream is a set of phases, declared beside the state line.** `plan/STATUS.md` may carry one
optional line under `STATE:`:

```text
STREAMS: agent=J catalog=O prototypes=Q
```

A task belongs to a stream by the phase letter of its own id, so a file cannot say one thing about a
task and another about the checkout that works it. The primary stream is `app`: it owns every letter
no other stream names — in Z3L that is every area letter the backlog uses — and it is the plan's own
checkout. A plan without the line has one stream and every rule it always had, which is what a
single-checkout project keeps.

**Each stream may hold one task in progress.** `## Now` gains a `[~]` entry per working stream instead
of at most one in the whole file, and two entries of one stream are still refused, with both ids
named. Each stream that works a task keeps its own session note: `app` keeps `plan/SESSION.md`, a
declared stream keeps `plan/SESSION.<key>.md`, and a note whose path names no declared stream is a
violation rather than a file nobody reads.

**The state line stays the plan's own.** `STATE: state=` and `STATE: task=` describe the `app` stream
and nothing else, and `STATE:` does not learn a field per stream. One line, one writer: a stream
beside the plan's own is read from its `[~]` entry, its note and its row in `cargo xtask status`,
which prints one row per stream — key, live task, and the sub-step the note names.

**The writer follows the task's stream.** `cargo xtask task start J1` marks `[~]`, writes
`plan/SESSION.agent.md` and leaves `plan/STATUS.md` untouched; `task close J1` clears the marker,
deletes that note and leaves the `## Next` paragraph alone; `task step J1` edits the note the task's
stream keeps. `cargo xtask state streams agent=J catalog=O` writes the declaration itself, because a
plan file is written by a verb and never by hand (ADR 0085).

## Consequences

- A session that opens any checkout reads four rows and knows what is being built elsewhere, without
  reading four branches of history.
- The backlog stays one file in one checkout. The one-writer rule is unchanged, so the price of a
  stream is a note and a marker, not a second plan.
- Two ideas are now separate on purpose: *which stream works what* (the id's phase, the declaration,
  the notes) and *what this checkout does* (`STATE:`). A project that later wants the state line to
  speak for the whole plan chooses a new ADR; this one keeps the line small.
- The session contract in `AGENTS.md` and `templates/AGENTS.section.md` gains the stream paragraph,
  and the gate's own sentences name the stream a refusal is about.
- A plan can now say `state=idle` while other checkouts work. That is not a lie: `state=` was always
  "this checkout is between tasks", and `cargo xtask status` prints the rest.
