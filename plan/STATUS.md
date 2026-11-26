# Status

STATE: milestone=M7 phase=T task=- state=idle base=3bed423 updated=2026-11-26
STREAMS: agent=J catalog=O prototypes=Q render=Y,L

Read this file first; the two commands print the state and one task, `AGENTS.md` §1.1 orders a broken plan.

## Where we are

The Render place shows the picture it is tracing: the tracer takes one sample over every band of the
image at a time with the accumulators held for the run, a checkpoint averages them down to a
display-sized picture on the device, and a mailbox of one carries it to the shell, which tones it
into an egui texture. The page is image-first — the picture fitted and letterboxed beside a
320-point rail of numbers, the run's own line under it: resolution, samples, elapsed and estimated
time, the file — and the last picture outlives the run, a cancel and a document switch, while the
exposure re-tones it in place (Y6, ADR 0250). Before that, the final render had a camera with a lens
that is the document's `model::CameraLens`, validated on the value, the file and the node, written
by `camera.lens.set` and drawn by the camera card in millimetres, and the tracer spends the
sampler's second pair on a point of the aperture disc (Y11, ADR 0249). A light can be a window (L2,
ADR 0247), the denoiser measures the noise it spends and the render's materials are one portable
library (Y4, Y8, Y7), and the assistant reaches a real model over its own wire (J14, J11). The
plan's work order still opens with a catalogue record that refers to a library material instead of
describing it (O16).

## Next

Nothing is scheduled: 15 open tasks wait in `## Next`.

## Index

| File | Holds | Read when |
|---|---|---|
| `plan/BACKLOG.md` | one line per task, ids, order, file hints | always |
| `plan/SESSION.md` | the live task: steps, blocker, next command | `state=doing` |
| `plan/debt.md`, `plan/questions.md` | accepted gaps; decisions for the owner | a task touches that area |
| `plan/rename.md` | the pre-release rename: inventory, procedure, gotchas | task R1, or a change that adds a place the product name appears |
| `plan/refactor.md` | design rules, subtask template, order (S7, S8–S22) | any S subtask |
| `plan/archive/done.md` | history up to schema 11 | never, `git log` replaces it |
