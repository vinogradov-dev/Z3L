# Session

```text
task: J18
goal: a batch step cannot name the id an earlier step creates, so one build is several batches and several undo entries

steps:
  [ ] starting the task
  [ ] engine: BatchStep::named,  resolution and its refusals
  [ ] doors: the tool step and the recipe line read `as`, the answer echoes it
  [ ] the wardrobe recipe names instead of hard-coding ids, run end to end at revision 53

now: commit
blocked: nothing
run: cargo xtask task J18
resume: git add the code, the ADR and the note; commit
```
