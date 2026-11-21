# Open questions

Decisions waiting for the owner. Kept out of `STATUS.md` so the start of a session stays short; read
this file only when a task touches the question.

- Folder naming, version statuses and the draft-to-production transition in the order registry.
- What API the material and order programs expose locally (CLI, socket, HTTP), and whether they ship
  with Z3L or stay external.
- Whether reference retrieval (catalogues, norms, past jobs) is needed, and in what form.
- Voice input and output: which platform or service, if any.
- The orientation widget has no keyboard or menu equivalent; decide whether the six views deserve
  shortcuts before the list grows.
- Around Y = ±90° the Euler fields are gimbal-locked: editing X or Z moves the other angle. Confirm
  whether the card should show a gimbal warning or switch to a quaternion readout there.
- Double-click enters one container level and Escape leaves it. Confirm whether the tree should also
  show and toggle that open state, or stay an always-expanded explicit path.
- Open containers are view state and reset when the session starts. Confirm that this is preferable
  to persisting them with the workspace.
- Orbit is now on MMB (Shift+MMB pan, Ctrl+MMB zoom), LMB is selection. Confirm the wheel should
  remain a zoom duplicate of Ctrl+MMB.
- An included band on one tangent segment steps the core at the joint with its neighbour. Confirm the
  step is acceptable in practice; if not, revisit before changing the core offsets.
- At a sharp joint between two included paths with different thicknesses, the model uses butt
  connectors and leaves the step as-is. Confirm whether later-band coverage should replace it.
- Whether a camera or light node is an ordinary tree child or a separate scene collection.
- The future material database schema stays open; see `docs/domain/materials.md` › Open questions.
- Icons (ADR 0055): whether one semantic accent token joins the one-ink rule for the "result"
  fill/stroke and toggled-on view buttons, and if so whether it covers every result or only
  creation/edit results; whether 20 pt tool buttons fit the current 16 pt row height or the set keeps
  16 pt everywhere and relies on tone alone; and whether `assets/README.md`'s "a shared drawing is two
  files" rule survives the icon alias table.
- `Project`'s mutations are one command per attribute (`panel.resize`, `panel.material.set`,
  `panel.edge_banding.set`, …), so a new panel attribute edits `model/panel`,
  `model/project/panel_cmds`, `engine/panel_cmds`, `engine/commands`, `engine/lib.rs`, `ui/panel_card`
  and often `ui/palette` — seven files in three layers, against the refactor's three-file locality stick
  (F4 in the S7 audit). One `Project::apply(Change)` value per area would collapse the engine side to
  one module and the UI to one card, at the cost of rewriting every command or keeping the stable ids
  as thin wrappers over the change values. Decide before M8 puts hardware commands on the same pattern.
- How a workshop-made part is decomposed for production. `10.15` tamburat and `10.16` bent panels
  (ADR 0104) are classes because the shop assembles them, and `supply = shop` keeps them counted once
  and out of the cutting list. Whether their components travel as a recipe on the object — the two
  8 mm skins and the rails, expanded by the exporter — or as ordinary sheet parts in the tree is not
  decided. The class rows do not change either way; the production mechanism does. The owner left it
  to practice after the first real tamburat order.
- The host rule of ADR 0094 and the approved table disagree: the record says a variant's `host`
  must contain its type's, while `50.03.03` undermount runner narrows `50.03` to `10.3` and drops
  `10.10`, which is what an under-drawer runner physically does. T2 follows the table and the model
  test now rejects a variant that *widens* its host instead. Confirm the reading, or restore the
  record's wording and give `50.03.03` the type's hosts back.
- The prototype's open decisions (`docs/domain/prototypes.md` §11): which formula surface a parameter
  may use (arithmetic and min/max only, or also conditions and table lookups); whether a split's
  threshold belongs to the workshop standard, the prototype's own parameter or both, and whether its
  children are named left/right or 1..n; and whether a fit-out's minimum width may steer the split.
  The standards' home is the workshop-profile question above.
- The creation menu's build word: the cell says «По форме ниши» / "Niche shape" today, and the owner
  offers «по форме области» with the name still open. «Ниша» is the workshop's own word for the
  space a product reserves (`docs/domain/products.md`), but the tool fills any cell parts bound, not
  only a frame's, so the narrower word may promise less than the build does. Pick one word, and it
  becomes `Message::FillMethodShape` in both languages.
- The overlay wall's lap: which of the four edges the lap covers — all of them, or only the ones not
  against a wall or a part that stands proud of the face — and whether it is one number or four. The
  task assumes one default out of the project's factory settings and one field per edge, with a lap
  that would cross a part refused rather than silently cut.
- The niche cell (`furniture.panels.void`) in a three-cell Panels group: it creates nothing, it is a
  probe every fill reads (ADR 0118), and it is enabled only inside an open product. Keep it as a
  fourth cell of the group, move it to the View tab beside the snaps, or let the panel tool toggle it?
  The task that restructures the group waits for this answer.
- The panel-by-points finish: the task proposes a method list — «Полигон» (n points, the triangle at
  three) and «Четырёхугольник по кромке» (three points, the third fixing the width, so three clicks
  give the standard board) — with «Закончить» in the card and Enter doing the same. That keeps one
  meaning per click and no modifier. Confirm the shape, or name the finish that should mean the
  standard board instead.
