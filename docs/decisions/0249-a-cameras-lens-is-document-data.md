# 0249. A camera's lens is document data

- Status: Accepted; supersedes the lens half of [0248](0248-a-lens-is-the-renders-setting-and-a-sample-is-a-point-of-the-aperture.md)
- Date: 2026-11-26

## Context

Y5 gave the final render depth of field and put the two numbers where the studio had gone: on the
render's own `Settings`, written over the copy of the camera the tracer reads ([0248](0248-a-lens-is-the-renders-setting-and-a-sample-is-a-point-of-the-aperture.md),
following [0213](0213-a-frame-carries-a-finish-and-the-tracer-samples-by-power.md)). The owner's
answer was that a camera's settings are saved with the camera.

The argument is the one the record already half made: a photographic camera *is* its lens, so an
aperture and a focus distance chosen for a scene are the scene's, not a property of the run that
produced the picture. Set on the render instead, the same saved scene yields different pictures from
the window, the command line and the assistant — the one thing the whole design avoids — and a number
a maker spends an afternoon on dies with the session.

## Decision

- **`model::CameraLens` lives on `CameraNode`**, beside the projection: an aperture radius and a focus
  distance, both `units::Length`, validated (`0..=500 mm` of radius, `1 mm..=1 km` of focus) on the
  value, on the file and on the node, with `CameraLens::PINHOLE` as the state a camera is born in. A
  radius of zero is the camera every scene had before there was a lens, and then the focus is a number
  nothing reads. The wire always carries the lens, the schema moves to 35 and the one golden file is
  regenerated (ADR 0096).
- **One command writes it: `camera.lens.set`.** A drag of either field is one undo entry, a value
  outside the range is a sentence, and an id that is not a camera is refused — the projection's rules,
  because the lens is the projection's neighbour on the same card. The camera card gains the two
  fields in millimetres; the row is a `Card` capability of the camera context in the inventory (ADR
  0117).
- **The frame carries the lens and the render does not choose one.** `scene_build::authored_camera`
  maps the node's lens into `scene::Camera::with_lens`, so `render.final` renders the camera it was
  handed, and the Render place renders through `scene_build::scene_camera`, which is the document's
  camera when the scene has one — the same camera the viewport draws through.
- **The render's own lens goes.** `Settings::with_lens`, its two fields, the Render place's two fields,
  its `Bounds::max_aperture_mm` and the line's `--aperture`/`--focus` are removed: one number, one
  home. A render that wants another aperture edits the camera, and that edit is undoable and saved.
  The camera a scene does not have — the fitted view of `--camera fit` — stays a pinhole.

## Consequences

- A saved scene renders the same picture wherever it is opened, and a workshop can keep a lensed
  presentation camera beside a pinhole catalogue one as two nodes of one document.
- A camera node can now say something the renderer refuses past: a lens of a radius over half a metre
  or a focus of no distance is a sentence on the value, on the file and in the command, not a picture
  that is silently a pinhole.
- The `--camera fit` line has no lens, because a fitted view is not a camera the document holds. A
  maker who wants a lensed image headlessly authors a camera and renders `--camera scene`.
- An override for one image is a change to this record, not a field added back later in silence.

## Related

- [0248](0248-a-lens-is-the-renders-setting-and-a-sample-is-a-point-of-the-aperture.md) — the thin
  lens, the concentric disc and the circle of confusion, which this record keeps; only the home of the
  two numbers moves.
- [0036](0036-cameras-and-lights-are-scene-objects.md) — a camera is a scene object with undo.
- [0096](0096-the-data-format-has-no-past.md) — the schema bump and the one golden file.
