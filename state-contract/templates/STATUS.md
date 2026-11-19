# Status

STATE: milestone={{MILESTONE}} phase=A task={{TASK_ID}} state=idle base=HEAD updated={{DATE}}

A project that works in more than one checkout declares the others under the state line, one
`key=PHASES` pair each — for example `STREAMS: agent=J catalog=O`. A task belongs to a stream by the
phase letter of its id, the primary stream `app` owns every letter no other stream names, and each
declared stream keeps its own note (`plan/SESSION.<key>.md`). `cargo xtask state streams agent=J`
writes the line; a plan without it has one stream and every rule it always had.

Read this file first. While `state=doing`, read `plan/SESSION.md` next; otherwise skip it. Then read
only the files named by the current task in `plan/BACKLOG.md`. Do not scan the workspace: the next
step comes from `STATE` and the `## Now` section, and the plan gate prints both.

## Where we are

- One bullet per fact a fresh session needs: what is live, what is recorded, what is not built yet.

## Next

`{{TASK_ID}}` is the first open task. Name the files it starts from and the order of the work; three
sentences is enough. The paragraph must name the task `STATE: task=` does — the gate rejects a
`## Next` about another task, and a command that changes the current task passes the new text with
`cargo xtask task close <id> --next <text>`. An empty order is written as `task=-` and a paragraph
that opens with `Nothing is scheduled`.

## Index

| File | Holds | Read when |
|---|---|---|
| `plan/BACKLOG.md` | one entry per task, ids, order, file hints | always |
| `plan/SESSION.md` | the live task: steps, blocker, next command | only while `state=doing` |
| `plan/SESSION.<key>.md` | the same for one declared stream | that stream has a `[~]` entry |
| `plan/debt.md` | accepted gaps, tagged by area | a task touches that area |
| `plan/questions.md` | decisions waiting for the owner | a task touches the question |
