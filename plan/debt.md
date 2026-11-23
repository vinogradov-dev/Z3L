# Known debt

Gaps accepted for now. Read the entries whose tag matches the area of the task in hand; the whole file
is not part of a session start. An entry leaves this file when the fix lands.

Tags: `[view-state]` session versus document, `[render]` lighting and drawing, `[mesh]` geometry
editing, `[object]` constructive objects, `[panel]` panels and banding, `[scene]` tree and files,
`[gizmo]` manipulation, `[types]` properties and furniture types, `[engine]` commands and history,
`[perf]` frame and picking cost, `[assistant]`, `[ui]` panels, toolbars and icon artwork, `[design]`
module size, layer arrows and duplicated knowledge, `[plan]` the plan tool and its format,
`[vendor]`.

- `[assistant]` `openssl-sys` is the one counted exception to the C-vendoring rule: the assistant's
  transport is `ureq` over the operating system's own TLS, and on Linux that is the system OpenSSL.
  `cargo xtask layers` allows the crate while the resolved graph leaves its `vendored` feature off and
  fails the moment a dependency turns it on, so what is forbidden is building OpenSSL from the C
  sources inside the crate, not binding the library the system already carries (J14, ADR 0189). It
  leaves when a maintained pure-Rust TLS provider exists whose crypto is neither `ring` nor
  `aws-lc-rs`; until then the vendored build is the breach, and the feature is the switch.

- `[assistant]` A turn of the assistant panel advances one step per frame but asks for no frame of
  its own (`Chat::is_busy`, `App::serve_chat`), so a turn that is not stopped from outside still waits
  for the pointer between two calls: the channel was given its ring in X111, and the panel's own turn
  was not (X122, ADR 0225). Ask for the next frame while a turn is in flight, the way a landed call
  does.

- `[plan]` The first two lines of `plan/map.md` are `148` and `p` — the stray tail of a pipeline
  that wrote the file in an older session, left above the preamble. They are not part of any row and
  the gate does not read this file (X111).

- `[object]` A fit is the largest rectangle **through the pointer**, searched in the section's own chord
  polygon. A pointer standing in an arc's outward bulge is outside that polygon, so no rectangle of it
  contains the hand and the plan falls back to the largest rectangle of the shape, which puts the part
  somewhere else: on `/home/alx/skaf.z3l` 97 of 884 sampled points of the L were such (the arc's sliver,
  up to 274 mm deep). The shape mode is exact there; the fit needs the search to run on the ring's arcs
  rather than their chords (N35, ADR 0204).

- `[gizmo]` A frame that keeps a ring answers its section exactly, but the niche's **box**, its corner
  list and the void tool's wireframe are the box of the ring's *chords*, so a frame with a deep bulge —
  the owner's arc is 274 mm deep — reports a smaller void than it reserves and the overlay leaves the
  bulge out. The cell's own ring would say it; the corners are a plane-only enumeration today (N34,
  ADR 0203).

- `[gizmo]` The niche tool draws the cell's **box**, so in a cut-corner or drawn-ring frame its
  wireframe reaches into the space the frame does not reserve and the callout prints the bounds of the
  shape rather than the shape (N29, ADR 0201). Draw the cell's own prism ring when a maker needs the
  niche tool to read the space the fill does.

- `[design]` `app` is a binary root without a library, so a second binary cannot share the shell's
  rules: `z3l directory` is a mode of the one program, because a second `[[bin]]` would write the rule
  for where `nomenclature/` lives (`settings_store::config_dir`) a second time. Split `app` into
  `lib.rs` plus thin binaries when a second window needs the shell, and move the directory window out
  of `app/src/directory_app.rs` then (ADR 0158).

- `[gizmo]` A turn is bounded by the bodies around the selection and not by the workspace's floor, so a
  ring swung far enough carries a piece below Z = 0 where a carry would have been held: the rotate rule
  reads no `MoveSettings` and no floor (X88, ADR 0177). Give the turn the same bound when a maker turns
  a piece through the bench.

- `[gizmo]` A turn has no typed angle: the shared scrub field belongs to a translate drag, and a ring
  drag answers the pointer alone (X88, ADR 0092). Add the field to the rotate gesture when a maker asks
  to turn by a number.

- `[gizmo]` A measured turn answers the *destination*, not the path (ADR 0166 read for a ring: the last
  angle the rule grants is always a free placement, but a hand that drags more than half a revolution can
  be granted an angle that swung the body through a neighbour on the way). Bisect the blocked interval
  instead of the whole angle when a maker turns a door right round.

- `[view-state]` The fill trims a hidden body by the box around it in the open product's frame, so a
  turned hidden part cuts a little more than it must (X73, ADR 0175). Trim by the body's own faces
  when a maker reports a shelf that is shorter than the void behind a leaned hidden wall.

- `[view-state]` The fill trims a hidden body on the board's own two axes only, so a hidden board
  lying across the board's thickness at the same height cuts the footprint instead of moving the
  free axis (X73, ADR 0175). `TooSmall` is the honest answer today; a fill that steps over a hidden
  slab is a tool of its own.

- `[view-state]` Selection and open containers still live in `ui::Editor` and the view camera in the
  shell, although ADR 0029 places them in the session. Cameras and lights are document nodes with
  viewport icons.
- `[render]` The frame uses the first authored sun light plus up to eight point lights; a ninth and
  later point light is stored but does not shade the viewport, and there is no culling by importance
  or distance. Point brightness uses a fixed shader exposure rather than a scene exposure, so a close
  or very powerful lamp clips its highlights; there is no tone mapper. Only the key light casts
  (ADR 0143): a point light lights through a panel, and one map of 2048² serves the whole scene, so
  two cabinets a workshop apart share the same texels.
- `[render]` An authored camera uses the scene default near and far planes because the model does not
  store them.
- `[render]` An authored camera pins the view and disables navigation, so the only way back to the
  navigable view is clearing the active camera in the inspector; a viewport-level "leave camera view"
  control is not implemented.
- `[object]` A constructive object supports one `Extrude` feature; there are no booleans, fillets or
  other feature kinds yet. Collapse is a one-way door and only undo brings the history back. The
  object card still offers `X, Y` text lines for a line-only ring beside the viewport editor.
- `[object]` A contour's self-intersection test samples each segment into chords; a crossing finer than
  0.1 mm inside an arc can slip through. An exact segment-pair test can replace it when the sketch
  editor needs the last tenth of a millimetre.
- `[object]` An arc stores a rounded centre, radius and angle rather than its two endpoints, so a
  joint at an arc is judged with a radius-dependent slack (ADR 0072). Storing explicit endpoints at
  schema 20 would make the arc end exactly the point the user drew, return the tolerance to one
  micrometre for every pair, and drop the pair rule from `to_geometry` and `joint_tolerance_mm`.
- `[object]` Along-contour movement at a corner shared by two arcs is refused with `ArcJointMismatch`
  when no point sits on both circles at millidegree resolution, because neither stored circle may be
  bent. Free movement rebuilds both arcs and is the default, so the grip only needs to avoid that
  motion at such a corner.
- `[mesh]` Vertex markers and face fills are painted over the viewport image without an occlusion test,
  so a vertex on the far side of the mesh is visible and selectable through it. A depth readback or a
  pick pass can hide the back vertices when the first large mesh makes it matter.
- `[mesh]` Mesh editing selects and moves vertices and whole faces; there is no edge selection and no
  face extrusion yet.
- `[perf]` Vertex picking and marker painting walk every vertex of the edited mesh each frame; a large
  mesh needs a spatial index or a GPU id pass. Face picking walks every polygon and its fan triangles
  each click.
- `[perf]` The gizmo preview rebuilds every drawable's world vertices on each pointer move. Camera and
  light drags already skip the upload, but a geometry drag still does not. An incremental transform
  buffer per drawable can replace the full upload when a large cabinet makes it visible. A vertex drag
  also re-tessellates the edited mesh on every pointer move instead of moving only the affected
  triangle vertices.
- `[perf]` The selection highlight re-uploads every mesh when the selection changes; an incremental
  colour update or a GPU id pass can replace it.
- `[perf]` A drawable's world transform is still baked into its vertex and edge buffers, so a body
  that moves re-uploads its vertices even though its mesh is unchanged (X57 keys a buffer by the
  transform as well as the mesh). One buffer per unique mesh plus a per-drawable transform in a
  storage buffer or an instance attribute would turn a move into a small write; a uniform slot per
  drawable cannot hold a scene, because a uniform binding is capped at 64 KiB.
- `[engine]` History records one entry per command. A drag of a whole selection is one command, but a
  task made of several commands (a future assistant task, for example) cannot yet be grouped into one
  step.
- `[assistant]` Faber is designed (ADRs 0028–0032) but not implemented; the scene store now gives a
  background job a file to open, but there is no headless binary or query surface yet.
- `[scene]` Save and load exist in the shell. Gaps: the window's own close button does not ask about
  unsaved changes (only the menu's *Exit* does), and there is no autosave. `app` still starts from
  `sample_session()`; a new empty scene is one menu click away.
- `[scene]` *Open* and *Save as* ask the system file manager, and a pick that never comes back holds
  the pair: `rfd` waits on the portal's D-Bus call with no timeout, so a crashed
  `xdg-desktop-portal` leaves the picker busy until the window restarts, and the binding reports no
  refusal of its own — a portal that is not there answers `None`, the same as a cancelled pick, so
  the status bar cannot say why nothing happened. The dialog is also not owned by our window, because
  no parent handle is passed: on Windows and Wayland the file manager is a window of its own rather
  than a sheet of the editor's (ADR 0182, X91).
- `[scene]` A scene written by an earlier build is not migrated to the current frame: there are no
  releases and no user files yet (`plan/rename.md`), so an old pre-release document may load with a
  different world shape instead of being converted. Owner's decision, 2026-11-10; revisit when a
  format ships to a user.
- `[types]` Parametric cabinets do not exist yet: M6 provides classification and properties, and
  product prototypes arrive later as user scripts over those properties.
- `[types]` A component has no separate "dissolve" operation yet: `node.component.convert_to_group`
  clears its marker, prototype keys and furniture type and then flattens it into the parent. There is
  no way to keep the component and detach its prototype record on its own.
- `[types]` The property catalogue is not designed: user properties are free-form and the inspector has
  no type editor for `Structured` values yet.
- `[types]` Furniture types have no registry-backed picker: the inspector edits the raw `GG.TT.VV` code
  and the scene tree shows the stored code, while names and ancestry live in
  `docs/domain/furntypes.md`.
- `[types]` Queries exist in the model and the scene tree selects a furniture-type subtree, but there
  is no engine command or saved report over a query yet.
- `[gizmo]` The rotate rings do not snap: an angle is the pointer's own. A translate drag snaps to
  the coordinate planes, the frames and the part faces inside the workspace's tolerance (ADR 0146);
  no angle has a step the maker can set.
- `[gizmo]` The rotate rings are world-aligned only; a local-axis mode is not implemented.
- `[gizmo]` A panel's furniture drag runs along the world axis its thickness points most nearly at,
  not along its exact normal, because every surface the rule measures is a world box (ADR 0102); a
  board turned off the world axes therefore travels in one of the three directions. Measuring a
  board in its own frame is the larger change and nothing asks for it yet (ADR 0147, N22).
- `[gizmo]` The pivot is edited only in the inspector; there is no viewport pivot-pick tool yet.
- `[gizmo]` The orientation widget is hidden in a viewport smaller than 124 pt wide plus margins
  (148 pt), or 148 pt tall plus margins; there is no menu fallback for the six views or the roll
  ring yet.
- `[gizmo]` The screen-space overlays over the viewport that this entry still owns — the drawing
  draft, its preview and its phantom, the box tool's size label and its backing panel — carry fixed
  dark-theme colours, so on the light workspace the pale draft line and the near-white size label
  read weakly against the paper. They are `egui` paint, not frame ink, so they are the leftover of
  X8 (ADR 0087). The contour overlay is paid: its four inks are theme functions (ADR 0218), and the
  colour rule now reads the shell, with the modules that still owe an ink named one by one in
  `SHELL_INK_DEBT` in `xtask/src/ui_rules.rs` — `box_gizmo`, `document`, `draw_gizmo`,
  `extrude_gizmo`, `gizmo`, `gizmo_math`, `rotate_gizmo` and `scene_gizmo`, all under `app/src/`.
  Paying one takes its name out of that list, and the list cannot grow.
- `[gizmo]` An extrusion pull previews its new height as a wireframe prism, not a shaded solid. A
  solid preview needs the evaluation cache to accept a previewed height without a document write.
- `[panel]` The core outline at a line-arc or arc-arc joint still joins per-segment offsets with a
  straight connector; only the band strips are intersected analytically.
- `[panel]` Band trimming relies on 3D booleans; when truck finds no solid the earlier band is rebuilt
  untrimmed instead of failing the build.
- `[panel]` A legacy file can still carry a corner banding on a point corner; the card does not show it
  and it is removed when the corner next becomes a segment.
- `[panel]` Blind cutouts that overlap any other cutout or cross the core edge are still rejected until
  the 3D edge-pocket stage.
- `[panel]` The panel card does not edit cutouts yet.
- `[panel]` A contour banding row names its ring segment by number only; the card cannot highlight the
  segment in the viewport, so the user has to count segments to band the one they mean.
- `[panel]` Command validation (`panel.resize`, `panel.edge_banding.set`) still uses stored fallbacks
  for some checks; rendering and diagnostics use the catalogue.
- `[panel]` A bent panel is not implemented. The agreed construction is a flat panel with all its
  machining, then a bend applied to the finished body; the operation pipeline needs a post/deform
  stage and the panel card a bend section. No geometry is stored until that stage exists.
- `[panel]` The saw rules an order package declares — a 4 mm kerf and a 10 mm trim per side — are
  constants in `crates/export/src/manifest.rs`, not workshop data. A shop with another blade gets the
  default in its package until a workspace setting (or the catalogue) carries the rule; the contract
  block is already there, so only the source of the numbers changes.
- `[panel]` An edge-banding material has no stock block: a roll is bought by length and the package has
  no unit for a roll price, so its `materials` row writes zero in the four stock columns. A roll price
  needs its own unit and its own column, which is a later decision.
- `[panel]` Nothing in the editor shows or edits a material's stock block or price: the catalogue is a
  file the shell loads, and until a catalogue editor exists the workshop edits the JSON by hand.
- `[vendor]` `vendor/truck-shapeops` is updated by hand when `truck` moves; see `vendor/AGENTS.md`.
- `[ui]` The header is one fixed row: at the narrowest window (640 pt) in Russian the band, the four
  view switches and the gear leave `Файл` as the last left-hand control that fits, so *Save*, *Undo*
  and *Redo* and the whole tab row are clipped away (they keep their keys and the File menu). The
  band is always visible by the owner's decision (U115). A responsive header — hide the wordmark,
  then the tab row, then the dividers by measured width, and never a control that is the only way to
  an action — is its own task and needs the owner's shape.

- `[ui]` A widget inside an `egui::Area` is missing from the accessibility tree: the frame's
  `accesskit_update` carries the root ui's widgets only, so the tool card's ✓ and ✕ — and every other
  floating surface built as an area — carry no name a screen reader can read (found while aiming a
  test at the card's buttons, U115). Give those surfaces an accessible parent when `egui` exposes
  one, or draw the card in a layer the tree walks.

- `[ui]` The panel card is the last inspector surface that still stacks a header per group: thirteen
  of them on one panel node (name, dimensions, nominal size, resolved, material, shape, outline,
  bowed edges, bevels, banding, machining, diagnostics), every one open by default, so a 700 x 500
  shelf's card is longer than the panel itself. U93 gave the object card one reading order (ADR 0116)
  and left this surface alone; the same audit — what a node kind can use, what is derived, what is a
  mode — is owed to it.
- `[ui]` A creation tool lands at the scene root since X4; a node inside a container needs the
  explicit *Add child* arm of U31. Until then the only way is to create at the root and move the row
  in the tree.
- `[ui]` The regular polygon's `Sides` field has no refusal state: a value outside `3..=128` is
  clamped and a non-number falls back to six, while the field keeps the typed text. The tool builds
  a valid polygon instead of failing, but a typo is not named. A validated field can land with the
  polygon dialog, if one is ever added.
- `[ui]` A ring exposes corner grips only: the shape tag lives on `Path`, so a panel outline or a
  cutout ring has no circle, rectangle or polygon grips and no arc radius handle even when its
  segments are arcs. Ring-level shape metadata is needed before those handles can exist.
- `[ui]` The number field opens when a grip drag starts, not when a grip is merely hovered or
  clicked, so typing a value without moving the pointer first needs a tiny drag. A click-only path
  would need the field to survive a press that never becomes a drag.
- `[ui]` The pull arrow opens only when the ribbon's *Extrude* button creates the body: the same
  `object.path.extrude` from the command palette or an API, and `object.extrude.create` from a
  contour, leave the body without a pull. The shell reads the height from the command result, so
  another entry point needs the same signal before it can pull too.

- `[ui]` A frame's height is typed only after a click: its card's field cannot take the keyboard on
  its own, because the height is what the hand pulls and a focused field owns the number instead of
  the pointer (`Document::update_box`'s `text_edit_focused` guard), and Escape in the field drops the
  whole draft instead of giving the number back to the pointer. A frame whose height is typed before
  it is pulled needs the field to pin the pointer and Escape to release the number — the distance
  fields' rule (ADR 0103) — which is a key path, not a flag (N19).
- `[design]` `app/src/main.rs` `viewport` is one 635-line function. U45 moves it onto
  `Document` with the rest of the scene-bound state, so the S7 audit filed no split; it is measured
  again and split there.
- `[design]` `impl Z3LApp` is 1620 lines over 40 fields. U45 extracts about twenty fields into
  `Document` and the viewport and file state into `tabs.rs`, so the audit filed only the two moves U45
  does not perform (S21 Home, S22 the chrome) and the impl is re-measured after U45.
- `[design]` `ui/panel_card.rs` (989) and `ui/object_card.rs` (989) sit under B4's 1000-line ceiling
  and their impls under 800 (`PanelCard` 432, `ObjectCard` 469), so the audit filed no split although
  F6 named them.
- `[design]` Twenty modules carry 25 or more public items (B6). Each is one value family or one command
  family whose header states what holds it together (`geom` segments, `model/contour.rs`,
  `model/primitive.rs`, `engine/node_cmds.rs`, `model/panel/machining.rs`, `ui/theme.rs`); the audit
  filed only the two over B4's size ceiling (S9, S14).
- `[design]` The task report selects `plan/debt.md` and `plan/questions.md` entries by the task's
  `files:` paths, their crate directory, a file stem and the crate tag (`[engine]`); an entry that
  names the area in prose only is not selected, and `cargo xtask task --all` prints every entry.
  Keeping the tags and the `files:` hints honest is what makes the selection useful (P4).
- `[design]` `cargo xtask task set --field files` replaces a `files:` hint only when it sits on its own
  line, which is the layout `task add` writes; an inline hint on the marker line must be moved by
  hand first. Every entry in `plan/BACKLOG.md` already uses the line form (P6).
- `[design]` The gate cannot see a number. `plan/refactor.md` §2 says an entry's "N production lines"
  and the report's measurement must agree, and nothing compares them: S14 closed with 547 and 172
  against the 574 and 212 the report printed, and the arithmetic of the two sums was wrong for a
  week. `cargo xtask task <ID>` prints both, so a closing session can see the drift; a rule would have
  to parse prose (P10).
- `[design]` `plan/map.md` is checked in one direction only: a row must name a path that exists, but a
  new module needs no row, so a split that creates `geometry.rs` leaves the map silent and the gate
  green. A path-to-row rule would fire on every `mod.rs` (P10).
- `[design]` An open task may cite an ADR number it will write — U45 reserves 0081, a hole in the
  sequence — so the gate checks a citation only once the task closes. A reserved number can rot
  unnoticed until then, and the report prints `(no file in docs/decisions/)` (P10).
- `[design]` Nothing proves a plan edit came from the tool: the gate validates what the files say, not
  how they got there, so a consistent hand edit lands and `cargo xtask ci` stays green. `--repair`,
  `file` and the missing verbs closed every reason to type the plan by hand; the rule that a session
  does not stays a promise in `AGENTS.md` §6.1 (P10).
- `[perf]` A hidden branch is released from the evaluation cache like a deleted node, because
  `EvaluationCache::finish` drops every entry the pass did not touch. Hiding a large assembly and
  showing it again rebuilds every mesh in it; keeping the entries while the flag is set means the
  cache has to know the hidden set instead of only the revision (U48).
- `[ui]` The message table covers the captions, hints and menus of `crates/ui` and the shell, but the
  detail of an engine `CommandError` and of a drag refusal in `gizmo_math` still reaches the status
  bar as its English `thiserror` sentence. The `units` refusals are mapped (`length_error`,
  `degrees_error`); the engine and drag ones need the same mapping at the UI boundary, or the status
  bar says half a sentence in the other language (U49).
- `[ui]` A preset tints the chrome but not the viewport: `scene::SceneColors` still names exactly two
  frames, dark and light, whose clear colour, grid and line ink were chosen for the neutral palette,
  so P2 ink·azure and P4 slate·mint draw their scene on a neutral background and the tinted ribbon
  reads against it. Deriving the frame's ink from the active palette — a `SceneColors` per preset, or
  a clear colour mixed from the palette's surface — closes the seam (U71, ADR 0089).

- `[ui]` The view cluster centres on the whole scene rectangle, so in a window narrower than both
  expanded panels plus the cluster (about 760 pt; the minimum window is 640 pt) the panels draw over
  it and it disappears until one is folded. The corner widget had the same problem and U73 clears it
  there; the cluster waits for the same treatment.

· `[ui]` The inspector's widest rows — the name field beside its *Rename* button and the property rows — need about 355 pt, so a 226 pt floating panel draws them clipped or scrolled sideways (`side_panels::scroll_body`). The rows should reflow to the panel width (a wrapping label, a narrower field) so the horizontal scrollbar goes away; the panel keeps its rectangle either way (U73).

- `[engine]` The mounting gate checks `host` when a class is assigned, but no command reparents a
  classed node: `node.group`, `node.ungroup` and a future move can put a hinge under a host its row
  does not name. The same check belongs on the reparent once one exists, or the gate is half a rule
  (T4).
- `[engine]` Holes, grooves, cup bores and mounting patterns have no machining kind yet, so
  `Operation::Holes`, `Grooves`, `CupBore` and `MountPattern` have no command to refuse. The hardware
  commands must read `NodeRules::admits` when they land, as T4 did for contour, edge banding and
  cutouts.
- `[mesh]` A panel's derived holes are computed and cached but never drawn: the viewport paints the
  undrilled body, so a cup bore is invisible until a task decides how a bore is painted — a decal, a
  marker, or a real boolean per hole, which costs one kernel step for each of a door's dozen holes.
  The record says what is wrong and never guesses the fix.
- `[panel]` The material a drilled hole is clipped against is the core minus its through cutouts,
  across the full thickness. A blind cutout that thins the board where the bore lands and an end
  bevel that shortens the material beside it are not read yet, so both answer a hole that the shop
  would have to correct; the crossing needs a material that varies with the point, or two
  diagnostics that name the cutout and the bevel (O10).
- `[ui]` The panel card's size fields, banding menu, shape section and machining add menu name the
  class's reason when it forbids them, but the class picker still offers every row: a person can
  choose a class whose host the assignment then refuses and reads the engine's English sentence in
  the status bar. The picker should filter its rows by `host`, the way it already filters a query
  (T3, T4).
- `[gizmo]` The empty-space tool refuses a post in a splayed niche, although a post spans the depth as a
  rectangle: the free x-range it may stand in is the intersection of the section's width over the whole
  depth, which the trapezoid's two wall lines answer exactly, and the model could hand that range to
  the post. Until then a maker fills a splayed niche with shelves only (N7).
- `[gizmo]` A shaped niche's walls are checked for height at the void's floor and ceiling, not along
  the whole span: two boards that meet in the middle with the envelope behind them would pass as one
  wall. The union of the coplanar faces' extents would close it; the samples are one micrometre apart
  and no furniture case this tree builds has such a gap (N7).
- `[gizmo]` A niche bounded by a curved surface — a bent panel, a rounded body, a cylinder — is
  measured by the box around that body: the cell is conservative, so a part fits with a gap and never
  cuts in, but it does not follow the curve. An exact curved section needs a surface query that can
  meet a curved face and a section that can hold arcs; the cell, the fill and the bevel are planes
  only (N9, N12).
- `[gizmo]` The corner brackets of an open level draw from the bounds the last evaluation measured,
  so while a part inside the open group is dragged the bracket stays on the committed box and snaps
  when the move commits. Feeding the previewed placements into `LevelBounds` would make the guide
  follow the ghost the way the frame does (U89).

- `[engine]` `panel.contour.create` is registered, labelled and parameterised, and no surface runs it:
  the free-outline cell of U90 builds a `panel.create` with an outline, so the command duplicates a
  path that exists. It is declared `Api` in `crates/ui/src/inventory.rs` today; either a surface takes
  it or the command goes, and deleting it is the smaller change (U94).

- `[engine]` `panel.fill` is the one command over the niche measurement, and no surface runs it: the
  three fill cells stay the empty-space gesture, which paints a ghost through the parts the person has
  switched off (`Sight`, ADR 0175) and writes that ghost through `panel.create`. The command measures
  every part in the way, because a caller has no sight to hand in, so routing the gesture through it
  would make the click create a part the ghost did not show when a hidden part is inside the void. One
  path for both needs the hidden set in the command's parameters, which is view state in a write
  (ADR 0226); a caller that wants the command's answer asks `query niche` first (N37).

- `[engine]` `object.cuboid.create` names the box by the word the ribbon prints, and no surface runs
  it: the box cell is the rectangle gesture (ADR 0138), so the cell cannot dispatch three typed
  sides, and the palette has no field to ask for them. The command line and the assistant are its
  doors, which is the caller the cell could not serve; a ribbon face that types the three sides
  would be a dialog the ribbon does not have yet (X105).

- `[ui]` A key is data (`KEYS` in `crates/ui/src/inventory.rs`) but a *person* cannot yet reassign
  one: the table is a const, so the Settings page reads the defaults and draws no field. Remapping
  needs the table to become a value the workspace settings carry — a per-id override merged over the
  defaults, one write per edit and a "Restore defaults" button — with the same tests this task added
  walking the merged table rather than the const. It waits for the owner's word and for a Settings
  section that can hold it (U96).

- `[gizmo]` A sketch tool hands the snap resolver no face, so the `face` kind never captures for a
  drawing point: the point lands at the footprint of the corner or the edge the pointer names, and
  the ruler's own surface reading (which steps past a product's reserved box) is not shared with it
  (V5, U116). The atom already ranks a face the tool measured; the missing piece is one probe the
  drawing tools would have to ask before they hand it in. Add it when the owner asks to draw on the
  face under the pointer rather than on the work plane.

- `[gizmo]` The fill and the niche ghost keep the niche atom's cursor rule (`ui::pointer_point`) and
  do not read the snap resolver: the point they name is the *space* the pointer is in, so a snap that
  moved it by the tolerance would name the same cell and change nothing on screen (U116, ADR 0199).
  Revisit if a fill ever needs the pointer's *place* along its free axis as a snapped coordinate.

- `[perf]` Measured on 2026-11-17 in a headless run (`app/src/scene_build.rs`, the probe
  `an_edit_costs_the_walk_of_the_tree_and_not_the_vertices`, dev profile with opt-level 2): an edit
  anywhere costs 0.42 / 3.6 / 18.4 ms at 500 / 2 000 / 5 000 root spheres of 32 x 16, which is the
  same as a rebuild that changes nothing (0.41 / 3.67 / 18.4 ms) — the walk of the tree at about
  3.7 µs per node, now that a drawable's world box comes from the local box the mesh cache measured
  (X56; the same edit cost 27 / 109 / 285 ms while the walk transformed every vertex twice).
  A frame that changes nothing still costs 1.7 / 4.2 / 9.5 ms in the window, and that cost is the
  chrome (the scene tree draws a row per node), not the geometry. Cold evaluation is 0.58 / 2.2 /
  5.5 s, dominated by meshing (1.1 ms per 32 x 16 sphere). A click rejects an object by the box its
  mesh cache measured and casts only what the ray reaches since X59, which re-measured this entry
  (the probe `a_click_costs_the_objects_and_not_their_triangles`, best of five): 0.06 / 0.25 /
  0.62 ms over empty space and 0.14 / 0.35 / 0.79 ms through the field at 500 / 2 000 / 5 000
  objects, against 16 / 64 / 161 ms while the cast walked every triangle of the frame. The
  remaining cheap fix in this area: let the mesher write one flat index list while it builds
  instead of filling `model::Mesh`'s `Vec<Vec<u32>>` and flattening it afterwards. A picking BVH is
  the larger step a measurement no longer asks for, and the world box of a turned round body is the
  box of its box, filed as X62.
- `[perf]` The upload follows the same rule since X57 (`app/src/scene_build.rs`, the probe
  `an_edit_uploads_one_drawable_and_not_the_scene`, real adapter, 1280 x 800 target, best of five):
  at 500 / 2 000 spheres one edit costs 0.26 / 0.92 ms and an unchanged frame 0.17 / 0.89 ms, while
  the whole edited frame into an empty renderer is 102 / 371 ms, which is what every edit cost
  before. The frame-wide edge list with its 4 M line-vertex budget went with it: each drawable owns
  its lines, so the 5 000-sphere stress scene draws every ball's wireframe, and a single drawable
  past one buffer is a readable refusal instead of a cut (X60).
- `[render]` The outline ink is one black constant for both variants since U108, and the edge buffer
  is one of the values a line buffer is built from: a strength drag (X64) and any change of the ink
  re-upload every edge buffer, once per change rather than per frame at rest. Two consequences wait
  for a line uniform: a strength drag costs the X57 upload of the whole frame (0.9 ms at 2 000
  spheres, ~370 ms in the 5 000-sphere stress scene), and the *wireframe* and *shaded with hidden
  edges* styles draw black lines on the dark background, so they read faintly
  (`target/ui-shots/wireframe-ink.png`). Both go away with the same change: a line ink and alpha in
  the per-pass uniform (a third dynamic offset beside the camera and ground slots), so a style and a
  strength become draw-time values instead of buffer keys.
- `[render]` The floor plate writes no depth (U108), so a body under an opaque floor is hidden by the
  plate's blend while its form outlines still draw: at 100 % opacity an underground part reads as a
  wireframe ghost instead of nothing (`target/ui-shots/floor-opacity-100.png`, U109). Either the
  plate writes depth when it is opaque — which gives up the U108 pass-through the see-through floor
  replaced — or the edge pass learns the plate as a second occluder, which wants the plate's depth in
  a buffer a line can test against.

- `[plan]` `cargo xtask file <reference>` takes the *whole* body of the file, so a session that means
  to append one entry passes its own text and silently replaces every entry the file held: one session
  did exactly that to `plan/questions.md` and committed the single line it meant to add, and the file
  had to be restored from git. The gate cannot see the loss — the short body is still a legal file —
  and `plan/BACKLOG.md` is refused outright, so the verb that owns the reference files has no append
  form at all. An `--add` that appends one entry through the same gate, or a guard that refuses a body
  far shorter than the file it replaces, closes it.
- `[object]` `object.collapse` builds a body through the analytic backend, which answers exactly one
  extrusion; a body with a boolean step is refused with `UnsupportedConstructive`, so a maker who
  wants the cut baked as a mesh cannot have it until the command path can reach the geometry kernel.
  The command signature carries the project, the directory, the params and the contact adapter, and
  an evaluation service beside the contact one is the seam that closes it: one factory installed by
  the shell, one ADR, and the same seam serves the CLI, which has no kernel today either (W1,
  ADR 0197).

- `[plan]` `cargo xtask task note` lifts the body's first line into the heading `# <ID> — …` and
  keeps that line in the body, so a note that names its subject in its first line shows the sentence
  twice. Strip the first line from the body before it is written when it is the heading (X95).

- `[render]` The frame carries one key light and up to eight point lights, so the final render traces
  no area light: a window, a softbox or a strip is a point or nothing. Y2 samples the point lights by
  emitted power through an alias table and weights a light sample by the power heuristic, but every
  light in the frame is a delta emitter, so that weight is one and the BSDF sample never competes for
  the same path. A rectangular emitter is L2's (the document gains the object, the frame carries it);
  until it lands, multiple importance sampling has nothing to combine.

- `[render]` A tinted pane seen through glass reads above the hand-summed series of its internal
  reflections: 3, 8 and 21 percent in red, green and blue for a 400 mm slab at 40 degrees, while the
  clear pane is exact and a matched-index slab reproduces Beer-Lambert to five decimals. The
  discrepancy grows with the tint, so it points at a further internal path or a path length the series
  does not model rather than at the absorption law (Y2, `plan/notes/Y2.md`).

- `[render]` Glass casts a black shadow: a pane is a geometric occluder for every shadow ray, so a
  floor lit through a window is in shadow rather than lit by the window. Closing it needs a light that
  transmits, which is L2's area emitter and a shadow ray that knows what it crossed.

- `[ui]` The render place has no preview of the last image and no status line of its own: the picture
  is written and named, and the status bar still describes the node the selection points at while the
  page is open. The preview needs a texture the shell registers for the page, and the status row needs
  a zone the place owns; both are the place's next increment.

- `[scene]` The authored camera of `crates/project/tests/golden/scene.z3l` sits at the origin looking
  along `-Z` while the model it is meant to see stands in `+Z`, so a render from the scene's own camera
  is black and `z3l render` needs `--camera fit`. The fixture's camera is the defect, not the render:
  the golden file is regenerated by `crates/project/tests/make_golden.rs` when the schema changes, and
  its camera placement belongs to that generator.

- `[plan]` A field label is found wherever a word starts, not only where a line starts, because a lead line
  closes with `— rules: B1`. The cost is that a body whose sentence carries a label word is read as two
  fields: P19 said `… to run its four commits: plan/rename.md …` and the half after the colon became a
  `Commits` field, which a later `task set --field Done` rewrote on its own line (P21 measured this). A
  re-wrap by the writer can produce the same split by landing a label word on a line break, so the entry
  format and the parser do not agree about what a label is.

- `[mesh]` The arrangement welds at a nanometre (`VERTEX_TOLERANCE = 1e-6` mm) while the document's
  joint rule allows tens of micrometres (ADR 0072), and `Contour::to_geometry` now closes the
  difference for every stored ring. A caller that hands `geom::union_regions` a ring built directly
  in the geometry layer with a joint gap is still outside the contract, and what it gets is not even
  a consistent refusal: a single gap is bridged when the neighbouring edge's supporting line crosses
  the arc's circle (measured on X103: a 600 mm sector with a 10 µm gap unions correctly), and a ring
  with a gap at each side of its arc loses every piece, which is how the owner's `cross.z3l` answered
  "the rings do not meet". The boolean should refuse an open operand with a sentence instead of
  repairing one gap and dropping the rest.

- `[assistant]` The inspector's `shelf_fit` has no bearing check: ADR 0191's "bearing under 30 mm"
  is a fact about the shelf's support — a pin, a cleat, a groove — and the document has no hardware
  entity to measure it against. The width fit and the loose warning ship now; the clause joins the
  set when the hardware catalogue does (J9, M8).

- `[assistant]` `sheet_fit` allows both quarter turns of a part because grain is not document data
  (ADR 0216): a material's grain arrives with the decor the shell decides, not with the panel. The
  day grain is stored the rule reads it and drops the turn the grain forbids (J9).

- `[types]` The hanging rail's row `20.06` names `10.02` as its host, and `10.02` is not a class of
  the registry (the cornice's row `20.03` names it too), so `node.property.set` refuses to class a
  rail in a cabinet. The inspector reads a rail a loaded scene already carries, and the fixture sets
  the class on the document directly; fixing the row's host is a registry change and an owner
  decision (`plan/questions.md`, J9).

- `[assistant]` A front's mounting form — overlay, inset or half-overlay — is carried by the hinge
  class and not by the door, so `door_gap` reads the opening as the product's width and a pair's
  outer edges as flush. An inset front in a case can draw a false warning until the form is document
  data (`plan/questions.md`, J9).

- `[assistant]` The assistant panel lists six findings and an "and N more" line; a row selects no
  node and a suggested fix is not a button. The tool answer carries every finding and every command,
  so the model can act on the list a person cannot yet click (J9).

- `[plan]` A task that has been started cannot go back into the order: `task defer` refuses a `[~]`
  entry, `task reopen` takes only a done one, and `task drop` deletes the entry with its note, so a
  session that starts a task and finds it blocked (J12, 2026-11-23, whose premise the owner retired in
  the same turn) can only close it as done — which is a lie — or move the marker by hand, which the
  gate validates and does not see. Add the verb that returns a started task to `## Next`: one marker
  write, the entry moved like `defer` moves it, its `plan/SESSION.<key>.md` removed, in one gated
  write.

- `[engine]` A piece cannot be put into a frame it is not a child of. Measured by the acceptance test
  (J13, scenario 5): `product.placement.set` on a product whose box would land inside another
  product's envelope is refused — "`Распашной шкаф 800×460×2200` would come to rest inside `Ниша`" —
  and nothing reparents an authored piece: `product.create` takes the root or a plain group as its
  parent, `node.group` makes a group out of siblings and `node.ungroup` the reverse. The owner's
  sentence "поставь его в нишу 820 × 2140 × 580" therefore ends in a refusal, and the 60 mm by which
  the piece is taller than the niche is arithmetic the model does, because no rule measures a piece
  against a frame it is not part of. What would land it: a `node.reparent` command with the write
  guard's containment rule unchanged, or a "put this piece in that frame" operation that also records
  the intent, and either one is the owner's decision.

- `[assistant]` The panel's model cannot see the scene. The tool server draws a picture
  (`render_preview`, ADR 0228) and a process that owns the loop may ask for it over the channel, but
  the chat wire offers no such tool: its answer would have to travel on the OpenAI wire as an image,
  which the port does not carry (J29 lists `command_list`, `query` and `inspect` and stops there).
  A model in the panel therefore proves a closed cabinet, a door gap or a mirror by `query parts`,
  `query bounds` and `inspect`, and never by looking. It leaves when the wire carries one picture and
  the port answers it — the `Previewer` the shell already holds is half of that.

- `[assistant]` A call an agent process makes lands in the scene in front, not in the tab that started
  it. Each tab owns its chat and its own backend (`Document::chat`, ADR 0233), but a process reaches the
  document through the window's one channel — `z3l mcp bridge` presents `channel.json`'s token and
  copies the process's tool calls to the socket — and `Call` names no tab, so `App::answer_channel`
  answers `self.tabs.active_mut()`. A turn in a scene behind therefore cannot pause its process, and a
  batch that process lands edits the front document; the endpoint kind pauses honestly, because its
  whole turn is driven by `serve_chat`. What would land it: the bridge carries the tab it was spawned
  for, `Call` gains that field, and the window answers that tab instead of the active one — or one
  channel per tab, which costs a socket and a token file each. Either is a change to the tool server's
  contract for every client, so it needs its own task and record (J24).
