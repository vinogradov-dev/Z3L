# Session

```text
task: Y14
goal: a material is created and edited in the window, and the file it writes is the one the render reads

steps:
  [ ] starting the task
  [ ] the window, the store's copy, the model's walk and the shell are green under cargo xtask ci

now: read the offscreen shot and report to the owner
blocked: nothing
run: cargo xtask ci; cargo test -p app --bin z3l the_material_window_is_painted_offscreen -- --ignored
resume: the stream's merge into dev and the Y14 close
```
