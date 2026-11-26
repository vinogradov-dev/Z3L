# 0253. The tiling grid is drawn from the frame's own coordinate, beside the shadow switch, and off by default

- Status: Accepted; shapes Y16, builds on 0252's per-node picture placement and 0244's record, and
  reads 0111's guide ink
- Date: 2026-11-26

## Context

A material's record says how much board its picture covers, and a node may place that picture its own
way (ADR 0252). None of it is visible until a final render is drawn: a person cannot tell whether a
veneer's picture spans 600 mm or 1200 mm, where the seam falls on a door, or whether the grain runs
along the board. Every 3D editor the person knows answers that with an overlay — a UV or checker grid
— and the viewport already samples the same `scene::Decor`: the picture the tracer reads, on the
coordinate `Decor::uv` computes.

Two things had to be decided: **where the grid is computed** and **who owns the switch**.

## Decision

- **The step is the decor's and the grid is the shader's.** `Decor::uv` already divides by the
  record's `covers` and adds the record's turns and the node's mapping, so one cell of its coordinate
  is one whole picture and the seam is exactly where the sampler repeats. The fragment therefore
  draws a line where the coordinate is a whole number: one calculation over the vertex's own value,
  no second projection, and no way for the overlay to disagree with the print. A line pass over
  polylines would have to re-derive where a picture wraps on every board, in world space, and would
  draw the seams of a turned or mapped picture in the wrong place as soon as one of those two facts
  moved.
- **The weight is the theme's and the ink is the guide's.** `ui::theme::DECOR_GRID_STROKE` is a rule
  in points and `DECOR_GRID_INK_ALPHA` is its opacity; the shell turns the points into the viewport's
  own pixels and hands the renderer the frame's helper ink, which is the amber every guide over the
  model wears (ADR 0111). The shader keeps no colour and no size of its own: a line that grew with
  the zoom would read as geometry on the board, and a second colour role for one overlay would be an
  ink the palette does not measure.
- **`Decor::uv` keeps its one caller.** The overlay reads the *vertex's* coordinate — the one the
  vertex builder baked — so the coordinate function still has exactly the two callers it had (the
  vertex builder and the tracer's soup), and a third consumer would have to be given the same
  coordinate and not a copy of it.
- **The switch is the view's, like the ground shadow's** (ADR 0029, U107): a leaf of the layout
  carries it, so one cell of a split may read the tiling while its neighbour looks at the picture;
  the tab keeps the answer, and a document switch or a fresh document starts with it off.
  `ViewSpec::decor_grid` is `None` for a frame drawn whole, because the guide needs the screen's own
  scale, which only the shell knows.
- **A surface with no picture keeps its plain tone.** The face pass binds one white stand-in group
  for every undecorated drawable; that group's own layer says it is not a picture, so the shader
  never lays a tiling over a board nobody can tile — and no per-vertex flag or branch is needed.

## Consequences

- The display group of the View tab holds two switches, not one, and the inventory gains the atom
  row `view.display.decor_grid`: a view toggle is a mechanism with no command, no undo entry and no
  document write.
- The camera uniform grows two slots and the decor group grows one 16-byte uniform; turning the grid
  on costs one uniform write per view and no upload, so a switch flips at no frame cost.
- The decor group's layout has a third binding, and a device that was already built needs no
  migration: the group is rebuilt with its picture, exactly as a replaced scan already was.
- The checker the entry offered as the alternative stays out: it would tint the picture the guide is
  there to help read, and the seam — the number a maker checks — would be a tone change rather than a
  line.
- A hatch or a per-face unwrap remains out; both need a projection this record does not add.
