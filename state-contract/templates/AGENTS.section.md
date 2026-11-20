### Answering the next step

This is the session contract for {{PROJECT}}: the plan files are the memory of the work, and a fresh
session continues from them instead of reconstructing the state.

A session starts by reading:

1. `plan/STATUS.md` — the `STATE:` line and its `Next` paragraph.
2. `plan/SESSION.md` — only while `state=doing`.
3. `{{TASK_COMMAND}}` — the task `STATE:` names, in full: its fields, its `files:` hint with the
   production and test lines and the last commit that touched each file, the `plan/map.md` rows that
   name those paths, the debt and questions the task touches, the ADRs it cites and
   `plan/notes/<ID>.md`, the research note a previous session left. `{{TASK_COMMAND}} <ID>` answers
   the same for any task. Prompt it instead of grepping `plan/*.md`, `docs/` or `git log`.
4. The files the report names, and their direct callers.

It does not read the workspace at large, the conversation history, `git diff` or `git log`. The diff
is a fallback for one case only — `SESSION.md` is missing or contradicts `STATE` — and even then only
for the paths the task names. Research a session had to do goes into `plan/notes/<ID>.md`
(`templates/NOTE.md`): the task report prints it, so the next session does not repeat the search.

A project that works in several checkouts declares them under the state line (`STREAMS: agent=J
catalog=O`), and then the plan holds one live task per stream: a task belongs to a stream by the phase
letter of its id, the primary stream `app` owns every letter no other stream names, and each declared
stream keeps its own note (`plan/SESSION.<key>.md`). `STATE: task=` and `state=` speak about `app`
alone — that is the checkout the plan is written from — so a stream beside it is read from its `[~]`
entry, its note and its row in the status block, never from the state line.

**A task list belongs to one stream, and the checkout says which.** The stream is the one this
checkout serves: `app` in the primary checkout, and otherwise the declared stream whose worktree the
session runs in, which its branch names. A question about the current tasks, the next task or "what is
left" is answered for that stream alone — its `[~]` entry, its own note and its own entries in `## Now`
and `## Next` — and the answer opens with the stream it is about. Another stream's entry is named only
to say that it is not ours. Do not merge the four streams into one list, and do not ask which stream
was meant when the checkout already answers it.

**The primary checkout collects.** A stream pulls `dev` in from its own worktree; the primary pulls a
stream back, because git will not move a branch another worktree has checked out and only this
checkout writes `plan/`. The integration session merges every finished stream before it takes the next
task, and closes the entry with the merged hash.

A request for the next step is answered from the plan files alone, in this order. Stop at the first
case that applies.

1. **An interrupted session.** `state=doing`, `plan/SESSION.md` exists, `STATE: base` differs from
   `HEAD`, and `steps` still has an open item. Say so and offer to continue, quoting the note's
   `task:`, `now:`, `blocked:` and `run:`. Resume at `now:` instead of starting the task over.
2. **A stream left in progress.** `## Now` holds a `[~]` entry: the plan's own stream's while
   `state=doing`, or a declared one's whatever the state line says. Ask whether to continue it or put
   it back to `[ ]`; do not pick the next task silently, and do not take a declared stream's entry
   for the plan's own.
3. **A contradiction.** A `[x]` without a commit hash, a `base` that is not an ancestor of `HEAD`, a
   `files:` path that does not exist, a `SESSION.md` without `goal`/`now`/`run`. Report what is
   broken and fix it with the tool before answering: the plan gate names every case, and
   `--repair` on a write lets a plan with several violations be brought back one verb at a time, so
   a broken plan is never a reason to edit `plan/` by hand. Stop only when the tool cannot do it.
4. **Only then** answer from `STATE` and `## Now`: the current task, its `files:`, and the one after
   it. Short.
5. **An empty order.** `task=-` and `## Next` says nothing is scheduled: say so and name what waits
   in `## Next` — do not invent a task, and do not take one into the order without being asked.
   `{{TASK_COMMAND}} take <id> --at top` puts one back.

Success looks like this: the answer needs no code read, no history and no diff, and an interrupted
session continues from `now:` instead of starting from zero.
