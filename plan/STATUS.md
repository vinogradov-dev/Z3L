# Status

STATE: milestone=M7 phase=T task=- state=idle base=f17a049 updated=2026-11-26
STREAMS: agent=J catalog=O prototypes=Q render=Y,L

Read this file first; the two commands print the state and one task, `AGENTS.md` §1.1 orders a broken plan.

## Where we are

The render has a soft source. A light can be a **window**: `LightKind::Area` carries a `LightSize`
face in the document's own units, emits from one side along its local `-Z`, and reads its intensity
as watts over that face; `light.create` and `light.params.set` take the kind, the colour, the watts
and the face together, the schema is 34 and the one golden file carries a window (L2, ADR 0247).
`scene::Lighting.areas` carries up to four faces as geometry and watts, `AreaLight::radiance` is the
one watts-to-radiance conversion, and the normal is derived from the half-edges so the side the
shader samples cannot disagree with the side it emits from. The viewport draws the face as a source
— `FaceShading::Source`, unlit and blended in a pass of its own, out of every shaded pass, the
stencil, the shadow map and the contact-shadow mask — with an outline that picks the light, and it
lights the room from the middle of the face at the same power. The tracer samples the rectangle by
solid angle with next-event estimation and Veach's power heuristic, which finally has two strategies
to combine: an alias table over the rectangles' power, a second sampler pair on the face, and a
surface sample that meets a face weighted the other way. The sampler carries eight dimensions and
`faces_of` skips a source, so a shadow ray passes through the pane that gives the light.

Before that, the assistant reached a real model and the denoiser measured its own cost (J14, J11,
Y4); the render's materials are one portable library and a node wears one record (Y8, Y7); and the
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
