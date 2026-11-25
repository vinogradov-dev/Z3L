# Session

```text
task: Y4
goal: the finished image is not noisy, and the filter proves itself against a reference

steps:
  [x] reading the tracer's accumulator and the reference's à-trous filter

now: the filter, the note and the gate are done; the stream commits
blocked: nothing
run: cargo xtask task
resume: commit, then the integration closes Y4 with the merged hash
```
