# Session

```text
task: U125
goal: evenly spreads a row of boards from the first one, and the count is what fits

steps:
  [x] `model::fill_row` answers the row: count, pitch, gaps, the leftover at the far end
  [x] `Session::fill_row` writes N `panel.create` steps as one batch and one undo entry
  [x] the card's «Равномерно» turn, the count row and the two pins read apart
  [x] the two-click gesture in `FillGizmo`/`Document`, Escape, the status bar's count
  [x] tests: the model's arithmetic, the batch, the shell's row state, both clicks and Escape
  [ ] `cargo xtask ci`, the commit and the close

now: the row is green: model query, batch commit, card choice and the two-click gesture
blocked: nothing
run: cargo xtask task U125
resume: cargo xtask ci, then commit and close
```
