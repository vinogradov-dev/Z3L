# Status

STATE: milestone=M7 phase=T task=- state=idle base=9ead440 updated=2026-11-26
STREAMS: agent=J catalog=O prototypes=Q render=Y,L

Read this file first; the two commands print the state and one task, `AGENTS.md` §1.1 orders a broken plan.

## Where we are

A catalogue plate shows the board's own grain: the final render's denoiser stops at a woodgrain's
step rather than at a paint line, and the guide it reads is one ray from the pixel's centre and
never the sampler's jitter, so a decor reaches a print unfiltered by accident only when the
operator asks for it (Y13). A glass front can be frosted by the roughness a pane already had: the
transmitted beam is drawn about the same microfacet normal the reflection lobe uses, so a
sandblasted door scatters what stands behind it while a clear one keeps yesterday's smooth
refraction, sample for sample (Y10, ADR 0243). A varnished board shows its sheen: the tracer draws
a clear layer over a base as a fourth lobe — a weight, a GGX width and its own index, read as one
Fresnel for the whole event, so the gloss a workshop says becomes the width once where a record
turns into a frame (Y9). The Render place shows the picture it traces: one sample over every band
at a time with the accumulators held for the run, a checkpoint averaged down to a display-sized
picture on the device and carried by a mailbox of one to the shell, which tones it into an egui
texture (Y6, ADR 0250). The final render has a camera with a lens that is the document's
`model::CameraLens` (Y11, ADR 0249), a light can be a window (L2, ADR 0247), the denoiser measures
the noise it spends and the render's materials are one portable library (Y4, Y8, Y7), and the
assistant reaches a real model over its own wire (J14, J11). The plan's work order still opens with
a catalogue record that refers to a library material instead of describing it (O16).

## Next

Nothing is scheduled: 14 open tasks wait in `## Next`, and the render stream's next entry is Y12.

## Index

| File | Holds | Read when |
|---|---|---|
| `plan/BACKLOG.md` | one line per task, ids, order, file hints | always |
| `plan/SESSION.md` | the live task: steps, blocker, next command | `state=doing` |
| `plan/debt.md`, `plan/questions.md` | accepted gaps; decisions for the owner | a task touches that area |
| `plan/rename.md` | the pre-release rename: inventory, procedure, gotchas | task R1, or a change that adds a place the product name appears |
| `plan/refactor.md` | design rules, subtask template, order (S7, S8–S22) | any S subtask |
| `plan/archive/done.md` | history up to schema 11 | never, `git log` replaces it |
