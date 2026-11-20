# Session

```text
task: J2
goal: a command says what it takes, so a model can call it

steps:
  [x] starting the task
  [x] the schema of every command is derived from its parameter struct and printed by `z3l commands --json` (3acda02)
  [x] the plan, the map, the ADR and the debt entry carry it
  [ ] the primary merges stream/agent and closes J2 with the merged hash

now: the schema ships in 3acda02 and the plan in the J2 commit; the enabled reason moved to J5
blocked: nothing
run: cargo xtask task J2
resume: the primary merges stream/agent and closes J2 with the merged hash
```
