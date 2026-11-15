# Session

```text
task: C1
goal: the point being placed shows its numbers at the cursor, and a sketch has two guide kinds

steps:
  [x] starting the task
  [x] the atom shape is decided: one Scrub per field at its own anchor, a closed FieldParam, the focus and the typed locks in the atom
  [ ] crates/ui/src/point_fields.rs with its RawInput test
  [ ] the guide painter and the theme pair
  [ ] contour_gizmo and draw_gizmo draw through the atom

now: crates/ui/src/point_fields.rs: FieldParam, FieldSet and the plate, before any call site changes
blocked: nothing
run: cargo clippy -p furnicraft-ui --all-targets
resume: write FieldParam and FieldSet in crates/ui/src/point_fields.rs, then run the clippy gate
```
