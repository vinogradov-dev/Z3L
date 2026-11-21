# Session

```text
task: J5
goal: a client we did not write drives the editor, headless

steps:
  [ ] starting the task

now: the door, the crate and the tests are green; committing
blocked: nothing
run: cargo xtask task J5
resume: merge stream/agent into dev, then cargo xtask task close J5 --commit <merged hash>
```
