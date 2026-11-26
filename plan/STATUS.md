# Status

STATE: milestone=M7 phase=T task=- state=idle base=5827d1b updated=2026-11-26
STREAMS: agent=J catalog=O prototypes=Q render=Y,L

Read this file first; the two commands print the state and one task, `AGENTS.md` §1.1 orders a broken plan.

## Where we are

The final render has a camera with a lens. `scene::Camera` carries an aperture radius and a focus
distance (zero radius is the pinhole every render had), `render.final`'s `Settings::with_lens`
writes
them onto the copy of the camera the tracer reads, and the document, the schema and the viewport
stay
pinholes (Y5, ADR 0248). A sample spends the sampler's second pair on `concentric_disc` — a square
to
a disc, uniform by area — and `view::camera_ray_through` starts its ray there, aimed at the point of
the focal plane the pinhole ray would have met: a point at `z` is spread over `2 a |z - F| / F`,
which
the unit tests and a rendered board's edge agree on within 2%. The Render place offers both numbers
in
millimetres and `z3l render` takes `--aperture`/`--focus`; the dimension budget did not grow, and a
lensed picture needs 256 samples per pixel to sit inside the two percent the pinhole reaches at 200.

Before that, a light can be a window: `LightKind::Area` carries a face, the frame samples it by
solid
angle with next-event estimation and multiple importance sampling, and the viewport draws it as a
source (L2, ADR 0247). The denoiser measures the noise it spends and the render's materials are one
portable library (Y4, Y8, Y7), while the assistant reaches a real model over its own wire (J14,
J11).
(The plan's work order opens with a catalogue record that refers to a library material instead of
describing it (O16).)

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
