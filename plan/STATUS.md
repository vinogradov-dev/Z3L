# Status

STATE: milestone=M7 phase=T task=- state=idle base=cb34ee2c7c3596db4f7ca20110b4eb1251ee8fc0 updated=2026-11-26
STREAMS: agent=J catalog=O prototypes=Q render=Y,L

Read this file first; the two commands print the state and one task, `AGENTS.md` §1.1 orders a broken plan.

## Where we are

The final render has a camera with a lens, and the lens is the document's. `model::CameraLens` is an
aperture radius and a focus distance on the camera node, validated on the value, the file and the
node, born a pinhole; `camera.lens.set` writes both as one undo entry and the camera card draws them
in millimetres (Y11, ADR 0249, superseding 0248's home). `scene_build::authored_camera` carries the
lens into the frame's camera, the tracer spends the sampler's second pair on a point of the aperture
disc, and a point at `z` blurs by the thin lens' `2 a |z - F| / F`, which a rendered edge and the
unit tests agree on within 2%. The Render place renders through `scene_camera`, so the document's
camera is what its button uses, and the render no longer chooses a lens of its own.

Before that, a light can be a window: `LightKind::Area` carries a face, the frame samples it by
solid angle with next-event estimation and multiple importance sampling, and the viewport draws it
as a source (L2, ADR 0247). The denoiser measures the noise it spends and the render's materials are
one portable library (Y4, Y8, Y7), while the assistant reaches a real model over its own wire (J14,
J11). The plan's work order still opens with a catalogue record that refers to a library material
instead of describing it (O16).

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
