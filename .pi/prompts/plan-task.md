---
description: "Draft a plan entry for a future task without writing anything"
argument-hint: "<idea, defect or feature>"
---
You are the planner. This session is read-only: it has no `edit`, no `write` and no `bash`. Do not
ask for them and do not try to work around them. Your whole output is text that a writing session
files with `cargo xtask task add`.

The idea: ${@:-nothing was given — say so in one line and stop}

## Read, in this order

1. `AGENTS.md` §1 (cheap session start, and the plan commands) and §6.1 (what a task entry is for).
2. `plan/STATUS.md` (the `STATE:` line and `## Next`), then `## Now`, `## Next` and `## Bugs` of
   `plan/BACKLOG.md`, and `cargo xtask task list`.
3. `plan/debt.md` and `plan/questions.md` — search them for the files the idea touches; do not read
   them whole.
4. The row of `plan/map.md` for the feature, then only the files that row names and their direct
   callers. Use the `rust-analyzer` tools for signatures and callers instead of reading a file whole.
5. The ADRs the row cites, when the decision is about behaviour rather than code.

## Answer with these six parts

1. **Duplicate check.** Is this already a task in `## Now`, `## Next` or `## Done`, or part of an
   existing one? Name the id. If it is done, say when and stop; if it is part of another entry,
   propose extending that entry instead of adding a second one.
2. **Lead.** One sentence, what the work makes true, in the voice of the existing entries — a
   result, not an activity.
3. **Why.** What is wrong today and why it costs: the user's problem, the measured number, the
   defect's reproduction. Two to five lines. No marketing.
4. **Shape.** The decision and the files, without code: which module owns it, what the value or the
   flow is, what is explicitly not in scope. If there are two plausible designs, name both and the
   cost of each; a planning session that hides a fork makes the writing session guess. A command id
   may be named only with what its parameters accept — a Shape that prescribes an id it has not
   looked up sends the session to re-decide it, which is how U90 spent six minutes on
   `panel.contour.create` and shipped `panel.create` instead (ADR 0114). Otherwise the Shape names
   the outcome and leaves the id to the writing session.
5. **Done.** The tests that would fail if the rule were broken, at the surface where a person meets
   it — not the tests that would fail before the change, which a test of the implementation also
   satisfies: X40 asserted the square over the panel the owner now exempts, and U88 asserted
   `Crosshair` where the rule asked for the square (0122). Then the gate that must be green, and the
   eye check named with the surface that has to be looked at, which is where the defect would show.
6. **The command**, ready to run:

```text
cargo xtask task add --phase <P> --at end --files "<existing, paths>" \
  --why "<…>" --shape "<…>" --done "<…>" "<lead>"
```

## Rules the entry must satisfy, because the gate checks them

- The `files:` hint names **existing** paths only; a file to create is described in Shape, never in
  the hint.
- The id is derived by the tool, so do not invent one; the phase letter comes from the neighbouring
  entries of the same area.
- Every bold id, every section and every budget in `plan/BACKLOG.md` is the tool's business: never
  write a plan file by hand, and never suggest that the writing session do it.
- A task is one coherent commit. If the idea needs two, propose two entries that each stand alone.
- A `Done:` clause that could not fail is the trap the gate cannot see: it asserts a test exists, not
  that the test holds the rule. Write each clause so that a reader can name the change that would
  break it.
- The entry cites `plan/map.md`, ADR numbers and paths; it does not copy a value, a colour or a
  sentence that will drift.
- A decision the owner holds — the domain, the document schema, a default the user meets on first
  launch, a new mode — is listed as a question, not decided here (`AGENTS.md` §4.9, §8).

## Finish with

- the ADR question: is this decision reversible? If not, say which ADR records it and what it must
  claim;
- the open questions for the owner, numbered, each with the default you would pick and why;
- one paragraph: what the writing session should do first.

Do not propose edits to code, tests or plan files as part of this answer; describe them.
