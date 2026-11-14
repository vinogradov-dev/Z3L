---
description: "Explain why the tasks that built a defect passed the gate, fix the defect, and record the answer where the review reads it"
argument-hint: "<what is wrong — a defect, a missing behaviour or a bad decision, in your words>"
---

The owner says:

${@:-nothing was given — ask for one sentence and stop}

This is a repair of work that already shipped, so the first question is not "where do I change it"
but "why did the tasks that built it leave the gate green". Answer both: the second answer is the
part that stops the next one, and a fix without it is half a task. If the ask is ambiguous, ask one
question before you touch the tree — never silently narrow the owner's sentence into the fix that is
easy to write.

## 1. Find the site, then the responsibility

- Locate the code: the row of `plan/map.md` for the feature, then `rg` for the words the owner used,
  then the `rust-analyzer` tools for the symbol. One sentence: what it does today.
- `git log --oneline -- <path>` names the commits, `git log -S '<string>'` the one that introduced
  the behaviour. `## Done` of `plan/BACKLOG.md` records the hash each task landed with, so a commit
  resolves to its entry: `cargo xtask task <ID>` prints that entry with its `Why`, `Shape` and
  `Done`.
- That commit → entry link is the whole point: judge the entry, not the session. A session that
  followed a vague `Shape` and a `Done` clause that cannot fail did what the plan asked.

## 2. Say why it happened, one mechanism each

Each finding is one line, in this shape:

```text
U88 — left <what> out — <mechanism> — <the observation that would have shown it, and where it was>
```

`mechanism` from this closed list, so `/time-review` can compare it with the losses it tallies:

- `hint` — the entry's `files:` named the wrong file or too narrow a set;
- `shape` — the `Shape` left a decision open, so the session decided silently;
- `done` — the `Done:` clause could not fail: no test, or a test asserting the code instead of the
  behaviour;
- `verify` — the check by eye was skipped, or was impossible but was not said to be;
- `context` — the session never read the ADR, note or sibling module that said otherwise;
- `contract` — an `AGENTS.md` rule covered it and was not routed, so it was never read;
- `gate` — the first build came after the edit, and the compiler or clippy said no;
- `scope` — two tasks hid in one entry, and the second half was never finished;
- `review` — the diff was never read as a stranger would read it, before the commit.

A finding needs an id, a file with a line, or a commit. "Careless", "missed" and "forgot" are not
mechanisms; if nothing else fits, `scope` or `done` usually does, and saying so is the finding.

## 3. File the entry, then fix

- A repair the owner asks for is an interjection, so it enters the order at the top:
  `cargo xtask task add --at top --phase <P> --files "<existing paths>" --why "<…>" --shape "<…>"
  --done "<…>" "<lead>"`, then `cargo xtask task start <ID>`. The entry exists from the first minute
  because it carries the id the note, the commit and the review need.
- Its `Done:` names the test that fails today. Write that test first (AGENTS §4.7); a fix without a
  failing-first test is a fix that will come back.
- The smallest change that makes the owner's sentence true, decided before it is propagated: a
  signature, a type or both end points of a number is gated on `clippy -p` before the callers follow
  (AGENTS §1.5).
- Look sideways once: `rg` for the same mistake in the sibling code — the mechanism, not the file. Fix
  it now or name it in the entry; say which and why.
- If the defect contradicts an ADR, that is a question for the owner (`plan/questions.md`), not a
  silent reversal of the decision in this commit.

## 4. Record it where the review reads it

In `plan/notes/<ID>.md`, after the research sections, two sections:

```text
## What went wrong upstream

- `U88` — left the panel's beta unchecked — `done`: its Done clause asserted the menu opens, never
  that a typed value round-trips.

## The sentence that would have caught it

- `AGENTS.md` §4.7 — "<the exact line to add or move>", because <the observation it would have
  forced, and when>.
```

- The bullets are the strongest evidence `/time-review` ever sees: a defect is a loss that already
  cost twice, once to write and once to repair.
- The second section is not a wish. It names a file and a literal sentence. When no sentence could
  have caught it, write `no instruction — the cost is a fixture, not a rule` and name the fixture,
  its crate and the test it belongs in.
- Keep the session log too, in the form of `/next-task` (`+m · phase · what · why [· dead-end]`): the
  minutes a repair burns land in the same table as the minutes a feature burns, and a repair that took
  twenty minutes is itself a finding.

## 5. Close

Narrow gate while iterating (`clippy -p <crate> --all-targets` and that crate's tests), then
`cargo xtask ci` once. Commit the fix and the note together, then
`cargo xtask task close <ID> --commit <hash> --next <text>`. The plan files are the tool's business.

## Rules

- Do not fix more than the sentence asked for and the same mechanism elsewhere. A repair is not the
  place to improve the neighbourhood.
- A change that adds an instruction line and deletes none is refused: the instructions are already
  long, and a rule nobody can find is a rule that does not exist.
- Never edit a plan file, an ADR or this prompt to make the tree agree with the fix. If the tree and
  the document disagree, the fix moves the tree, or the owner moves the document.
