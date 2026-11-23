# Session

```text
task: X117
goal: a panel placed by a command can hang its origin above its own geometry, and the anchor rule lives only in the fill tool

steps:
  [ ] starting the task

now: the design is on paper; the first edit is model::PanelType
blocked: nothing
run: cargo test -p model --lib panel_type
resume: read the first `files:` path and its callers
```
