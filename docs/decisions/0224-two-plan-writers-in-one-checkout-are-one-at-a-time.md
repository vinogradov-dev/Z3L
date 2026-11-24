# 0224. Two plan writers in one checkout are one at a time

- Status: Accepted; amends the `plan/.lock` consequence of 0085
- Date: 2026-11-22

## Context

ADR 0085 settled that the plan is markdown written through one tool, and its consequences say
`plan/.lock` keeps two writers apart. The tool took that file too late: `apply` created it after the
verb had already read `plan/BACKLOG.md` and computed what to write, so the lock serialised the two
renames and none of the cycle before them.

Measured 2026-11-21 (P25): two `cargo xtask task add` runs started in parallel in one checkout both
read the same backlog, both computed the next free id (`X107`), and the later write replaced the
earlier entry. The gate then reported `plan: ok`, because nothing was duplicated or missing by its
rules — the lost entry had to be filed again by hand. Nothing in the tool noticed, and nothing could:
the read-modify-write cycle was two steps the shell could see and the process did not own.

Two facts decide this. A verb's cycle is *read the plan, splice the lines it owns, validate the whole
result, write*: the read is as much part of the transaction as the write, because the id, the marker
and the `STATE:` line are all derived from what the read saw. And the tool runs as a short-lived
process, so the only lock that reaches two shells is a file, not a mutex.

## Decision

- **The lock is taken at the verb's entrance and released when the verb ends.** Every verb that
  writes the plan — `task add|start|set|step|note|close|reopen|take|defer|unstart|drop|phase`, `state` and
  `file` — acquires `plan/.lock` before it reads a plan file and holds it until the bytes have
  landed or the verb has refused. `create_new` is the test-and-set, so no writer ever sees a
  half-taken lock.
- **`apply` cannot land a change without one.** It takes the guard as an argument, so a verb that
  forgets to acquire one does not compile; the guard's own drop removes the file, which is why a
  refused verb and a `--dry-run` leave none behind.
- **A second writer waits, then refuses with the file named.** It retries every 10 ms for up to two
  seconds — a verb's cycle is tens of milliseconds on a plan of any size — and then answers
  `` `plan/.lock` is taken: another session is writing the plan, and this verb waited 2000 ms for it
  and wrote nothing; run it again once that session ends, and delete the file only if no session
  runs``. Waiting is what makes both writes land; refusing at once would turn a race into a lost
  command for whoever lost the toss, and a bare wait with no bound would hang behind a stale file.
- **A `--dry-run` holds the lock too.** A preview is a promise about what a write would land, and a
  reading that another session is in the middle of replacing makes the preview wrong; the lock costs
  a file create and gives back the one honest answer. The run still writes no plan content.

## Consequences

- Two writers in one checkout can no longer compute the same next id, so the failure P25 measured
  cannot be reproduced: the second writer reads the text the first one left. The gate needed no new
  rule, because the state it now sees is the state two writers always should have produced.
- A writer pays one file create and, under contention, up to two seconds. A session killed while it
  wrote leaves `plan/.lock` behind; every later verb then waits two seconds and refuses with a
  sentence that says what to do, which is the price of a lock no operating system reaps for us.
- The lock is per checkout. Cross-worktree serialisation stays the rule of AGENTS.md §6.2 and the
  stream refusal of P26: two checkouts hold two locks, which is correct because only the primary
  checkout writes `plan/` at all.
- A verb that reads the plan to refuse — the stream rule, `own_task_refusal` — now reads it under the
  lock as well, so a refusal is decided on text nobody is rewriting.

## Related

- [0085](0085-the-plan-stays-markdown.md) — the writer, its gate and `plan/.lock`.
- [0176](0176-the-plan-shows-a-stream-and-one-line-keeps-one-writer.md) — which checkout writes
  `plan/`; this record says when two of its writes may overlap (never).
- `state-contract/src/cli/write.rs` (`Lock`, `Lock::acquire_within`, `PATIENCE`, `apply`).
