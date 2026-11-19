# Backlog

One entry per task, stable id in front; a long entry may wrap onto indented lines. `[ ]` open, `[~]`
in progress (exactly one, in `## Now`, matching `plan/SESSION.md`), `[x]` done and in `## Done` with
the commit that closed it. Older entries are kept as a short tail so the current position reads at a
glance.

The `files:` hint names where the work starts and is required on every unfinished task: each path
must exist, so name a directory for a file the task creates. The hint is a hint, not a boundary: read
those files and their callers, not the workspace.

Order is the order of work. Renumbering is not allowed; a new task takes the next free id in its
phase. The first unfinished task in `## Now` is the one `STATE: task=` must name; `## Bugs` and
`## Next` are not part of that order.
Phases come from the shape-base plan; `cargo xtask task phase` owns this list:
`A` — contour
`B` — contour operations
`C` — sketch
`D` — panel variants
`E` — 2D primitives
`F` — revolution and sweep
`G` — 2D booleans
`H` — hardware and accessories
`J` — MCP and the assistant: the tools an agent calls and the rules it obeys
`K` — the contract other programs read: the scene file's compatibility and the order package
`L` — scene lighting
`M` — a milestone's own work, numbered by the milestone it was filed against
`N` — the furniture product: its entity, the frame it reserves, the parts inside it and how they are
  placed
`O` — the materials catalogue and the workshop standards: the files a build reads for its numbers
`P` — the plan and `xtask` itself
`Q` — the prototype mechanism: the parts, the card, the dissolve and the library
`R` — the pre-release rename
`S` — a structural refactor that changes module boundaries only
`T` — the semantic classes: the registry table, its traits and the editors it gates
`U` — the UI: toolbars, drawing tools, view controls and icon resources
`V` — primitive parameters
`X` — a defect in behaviour that already shipped

A defect is filed under `## Bugs` with an `X` id next to the feature that owns it. Its entry states
the observation, the expectation and the suspected cause, and never a guess at a fix that has not
been reproduced. Taking it moves the line into `## Now` at its place in the work order, so `STATE` can
name it; fixing it moves it to `## Done` with the commit, like any task.

## Now
## Bugs
## Next
- `[ ]` **Q2** the prototype mechanism is recorded as a decision, and the four open questions are the owner's
  Why: Sections 15 to 18 settle the three kinds, the section frame, the plane skeleton, the
   dissolve targets and the card, and no task may cite a decision that does not exist. Four
   questions are still open, so the record waits for the owner's answers.
  Shape: An ADR from sections 15 to 18, the draft trimmed to what was accepted, and the four
   open questions in plan/questions.md.
  Done: cargo xtask docs lists the record, section 18 keeps the seed list, and
   plan/questions.md carries the four questions.
  `files: docs/domain/prototypes.md,docs/decisions/README.md`
- `[ ]` **O2** a workshop's own numbers live in a file the build reads, not in the code
  Why: Panel thickness, shelf pitch, the maximum span, drill distances and gaps belong to the
   workshop, and today they are literals in a scenario. Where the file lives is an open
   question, so the task waits for its answer.
  Shape: One versioned file read through the atomic writer in app/src/settings_store.rs,
   validated on load, offering defaults to the card; a value outside its range is a sentence,
   not a fallback.
  Done: A test loads a standards file, a command takes its default from it, and a stale file is
   a notice.
  `files: app/src/settings_store.rs,crates/model/src/`
- `[ ]` **P17** the plan shows one line per stream, so four worktrees do not hide their work behind an idle state
  Why: The plan holds exactly one live task, so a stream that works its entry leaves the state
   saying idle while four branches move. The worktree rule papers over the gap: the plan is
   written in one checkout and a stream's progress lives only in its commits.
  Shape: state-contract grows a stream: one plan/SESSION.<letter>.md per stream, one
   in-progress entry per stream in the order, and a status line that prints a row per stream.
   The one-writer rule stays for the backlog file itself.
  Done: A gate test accepts one in-progress entry per declared stream and refuses two in the
   same stream, status prints one row per stream, and the backlog stays one file.
  `files: state-contract/src/,plan/BACKLOG.md,xtask/src/main.rs`
- `[ ]` **J1** the editor can be driven from the command line: list its commands, run a recipe, ask the scene a question
  Why: The assistant and the prototype expander both need the document without a window, and
   only export reaches the engine from the command line today. A generator that writes the
   scene file directly bypasses Session::dispatch, so it escapes undo, validation and the
   command gate.
  Shape: commands --json prints an id, a label and the shape of its parameters; run <scene>
   --commands <jsonl> [--out <scene>] [--dry-run] opens one session, dispatches each line,
   groups them as one undo entry and writes once; query <scene>
   tree|box|niche|parts|diagnostics [--id <id>] prints the report as JSON. Reuse
   Session::dispatch and SceneFile: no second mutation path.
  Done: A CLI test builds a fixture scene from a three-line recipe in one undo entry, --dry-run
   leaves the file untouched, and query tree prints the ids the fixture created.
  `files: app/src/cli.rs,app/src/main.rs,crates/engine/src/`
- `[ ]` **Q1** a node can say it is a compartment, what kind of prototype built it, and that it is frozen by hand
  Why: The prototype work needs three facts the document cannot store today: which container is
   a section, which prototype and card produced a node, so a command works while the library is
   missing, and which released node the parent must no longer rewrite.
  Shape: Three reserved node properties validated by type: compartment (a flag), prototype.kind
   (case, fitout or assembly) and manual (a flag). A wrong type is readable error; a scene
   opens without the library.
  Done: A test round-trips a node carrying all three keys, a wrong type is a readable error,
   and a missing prototype library changes nothing in the load.
  `files: crates/model/src/properties.rs,crates/model/src/project/query.rs`
- `[ ]` **O8** prices carry provenance and stock is an overlay
  Why: Prices go stale and arrive from outside; stock on hand is not a property of an item.
  Shape: Price with provenance and unit from the kind, a batched import as one history entry,
   an on-hand overlay package, external keys with a system registry, a freshness notice.
  Done: A CSV batch writes one history entry, and a price without a usable currency is refused.
  `files: crates/catalog/src/view.rs,crates/export/src/rows.rs,app/src/settings_store.rs`
- `[ ]` **O9** selection walks six steps and says why
  Why: A plug must follow the corpus it decorates, and the shop reasons in families, not in
   RGB.
  Shape: Six ordered steps ending in a named diagnostic, family equality then rank, the
   person's explicit choice stored on the node, results cached by revision.
  Done: An oak corpus picks an oak plug, a white corpus re-picks it, and a miss names the step
   that failed.
  `files: crates/catalog/src/view.rs,crates/engine/src/resolve.rs,crates/ui/src/panel_card.rs`
- `[ ]` **O10** holes are derived from the hardware that causes them
  Why: A hole belongs to the hardware that causes it, and a panel must not store one.
  Shape: Drilling objects resolved through instances, intersected with the panel body, ordered
   in panel-local coordinates, cached by panel, placements and revision.
  Done: A known hinge on a known door yields an exact hole set at exact coordinates, and an
   untouched panel is not recomputed.
  `files: crates/model/src/panel/machining.rs,crates/param/src/lib.rs,crates/mesh/src/lib.rs`
- `[ ]` **O11** placement rules are typed tables
  Why: System 32 and a supplier's hinge table are data a workshop owns, not code we write.
  Shape: RuleTable with typed axes and cells, completeness and overlap checked on load,
   precedence item then prototype then standard, one command and one undo entry.
  Done: A 32-panel yields the expected positions, and an uncovered overlay or door thickness is
   refused with the axis named.
  `files: crates/catalog/src/directory.rs,crates/engine/src/panel_cmds/panel.rs,crates/ui/src/panel_card.rs`
- `[ ]` **O12** a contour names its cutter, a groove does not
  Why: A contour milled with a 2 mm cutter has 2 mm corners, so the document must name the
   tool; a groove only needs its width.
  Shape: Milling stores a cutter ItemId, depth and offset; a groove stores width and depth;
   tools are items and the park is its own package; a missing cutter is a diagnostic with the
   last offsets.
  Done: A milling record round-trips with its cutter and a cutter outside the park is a
   diagnostic, never a narrowed cut.
  `files: crates/model/src/panel/machining.rs,crates/catalog/src/property.rs,crates/kernel/src/lib.rs`
- `[ ]` **O13** a part document is authored once and instantiated everywhere
  Why: A door must pick up an edited hinge, and a copy frozen at insertion cannot.
  Shape: A part-document contract checked on load, an instance node resolved with the cycle,
   depth and budget guards, catalogue.detach, a placeholder from the recorded bounds, STL
   import into an empty document.
  Done: A changed hinge reaches two doors without a rebuild, a cycle is refused with its chain,
   and detach turns an instance into nodes in one undo entry.
  `files: crates/model/src/node.rs,crates/model/src/tree.rs,crates/project/src/lib.rs,crates/engine/src/scene_cmds.rs`
- `[ ]` **O14** insertion and a fastener line are designed operations
  Why: Inserting an item and adding a fastener line are gestures, not fields in a card.
  Shape: Two operations by .pi/skills/design-operation: the armed state machine, the preview,
   the cancel, one undo entry, the feedback budget and the cursor.
  Done: An item inserts from a query with a preview, and a fastener line is drawn, cancelled
   and committed as one entry each.
  `files: crates/ui/src/inventory.rs,crates/ui/src/panel_card.rs,app/src/draw_gizmo.rs`
- `[ ]` **X85** a body is measured by its own surface and not by the box around it
  Why: A turned cone is a box for contact: its invisible cuboid hits a plate long before the
   apex reaches the corner between two plates, which is the owner report of the sixth session.
   Every body is measured by an axis-aligned box, so a surface never lives by its own faces.
   The owner asked for near-physical interaction: any surface lives by its faces, never by the
   box around it.
  Shape: Stage A: a new `contact` port crate (L1, next to `kernel`) with a `Shape` (convex
   polyhedron or triangle mesh), a pose, and four queries — overlap, distance, first touch
   along a direction, penetration depth and normal — implemented by a `contact-parry` adapter
   over parry3d 0.31 (Apache-2.0, pure Rust, glam-based, f64 build): a cone, a cylinder, a ball
   and a convex panel are exact convex shapes, a product keeps its envelope as a shell, a
   notched panel is a triangle mesh, and a non-convex mover is a compound of convex parts.
   `model` builds the shape of a node from its own parametric data — never from the render mesh
   — and `move_query`, `niche` and `resting_conflict` measure the same shapes. The answer keeps
   its meaning (ADR 0174): the destination is still asked whole, the jump still crosses
   material, and the hold still rests the piece on the face first — only what counts as a face
   changes. Stage B: a broad phase and per-revision shape caches so a big scene keeps its frame
   budget. The mesh never decides contact; the rule stays pure and stateless.
  Done: A turned cone reaches with its apex into a corner between two plates as far as its own
   surface allows and stops on the plate face; a ball slips through a hole in a blob when its
   place is clear and rubs the rim when it is not; a cylinder rests its barrel on a face; a box
   and a panel answer exactly as today; the drag, the release guard and the log read one
   measurement; the frame budget is tested; the gate is green.
  `files: crates/geom/src/lib.rs, crates/model/src/project/move_query.rs, crates/model/src/project/surface.rs, crates/model/src/primitive.rs, crates/model/src/project/query.rs`
- `[ ]` **X86** a piece already inside another may only come out
  Why: Today a pair the document already holds overlapping is exempt for the whole drag: a
   piece left inside another by a hand edit or a typed coordinate can be dragged deeper as
   easily as out. The owner asked that choosing the object move on such a piece makes it come
   out.
  Shape: The exemption of a pair that already overlaps is kept only while the overlap does not
   grow: the destination is measured against the start depth and refused when it is deeper, so
   every motion of the drag is a way out. The depth is the shared volume the rule already reads
   for a box pair and the penetration EPA answers for hulls.
  Done: A piece standing inside another moves out and never deeper while it still overlaps; a
   pair that only touches is untouched; the gate is green.
  `files: crates/model/src/project/move_query.rs, crates/model/src/project/surface.rs`
- `[ ]` **X87** an invisible floor stops every body unless the workspace turns it off
  Why: In a perspective or axonometric view a body dragged on a horizontal carry plane can end
   up below the floor, and nothing in the document says there is one: only a floor-bound
   product class keeps its height. The owner wants an invisible floor by default that stops any
   body, with a checkbox that turns it off, and the furniture classes unchanged — a lower
   cabinet already never sinks below the floor.
  Shape: The move rule gains a virtual floor at Z = 0 as a lower bound on every moved body,
   reported as a contact of its own kind so the log says Floor and not a node id. It is a stop
   and not a capture, so it does not depend on the snap planes being on; a floor-bound product
   keeps its class rules exactly as now. The workspace owns the switch — a checkbox beside the
   snapping boxes, a new text message with a hint, default on, persisted with the other
   settings, applied to the next drag and never to the one under the hand (ADR 0146) — and off
   means no floor at all. An ADR records the rule and its scope.
  Done: A body dragged down in a perspective view stops on the invisible floor and the log
   names it; the checkbox turns the floor off for the next drag, costs no undo entry and
   survives a restart; a floor-bound product behaves exactly as before; a wall or ceiling class
   is unaffected; the gate is green.
  `files: crates/model/src/project/move_query.rs, crates/ui/src/settings.rs, crates/text/src/messages.rs, app/src/document.rs, app/src/settings_store.rs, docs/decisions/README.md`

## Done

- `[x]` **X73** a hidden part is still in the way of the tools
  why: Observation, from the owner: he switched a frame's facades off in the tree, armed the
   shelf fill, and the ghost stuck in the phantom facades — a part he cannot see still cut the
   void. The owner's rule, given to this entry: a switched-off part is invisible to the eye and
   to the pointer, so a maker reaches and fills the niche behind it, while nothing may pass
   through the part itself — a hidden part "allows building through itself but not being
   intersected": the part placed there stops at the hidden part's own faces. Today the hidden
   set is session view state (`ui::ViewFlags`, whose `hides` walks the parent chain) and only
   the consumers that project the whole tree ask it: the frame builder and the light rig
   (`app/src/scene_build.rs`), picking (`SceneFrame::pick_surface`) and the camera glyphs. The
   model's own measurements walk the document, where the flag has no home: `Project::probe` and
   `Project::niche` measure every panel, object and shell
   (`crates/model/src/project/niche.rs`), the niche atom hands that cell to the fill
   (`crates/ui/src/void.rs`), `move_rule`'s obstacles stop a carried piece at a hidden panel
   (`move_query.rs::obstacle_boxes`), and the exact-distance fields read the same walls. So the
   drawing and the measuring disagree about what is there.
  shape: Two halves, which land together because either one alone is wrong. **The sight.** One
   visibility value in the model — the nodes the user switched off plus the parent walk that
   makes a hidden container hide its subtree, the walk `ViewFlags::hides` does today — passed
   in the way `SnapSettings` is passed into `move_rule`. Every query that looks at surfaces
   takes it: `probe`, `niche`, the pick, the walls the distance fields measure and the move
   rule's obstacles. The ray then passes a hidden part, the cell behind it is measured, and a
   void that holds one is no longer refused as "not one convex cell". **The boundary.** The
   part still stops at it: the hidden bodies a cell holds are walls for the material and not
   for the ray. The plan trims the footprint at every hidden body's face on the axes the part
   takes — the largest rectangle of `Fit` and the ring of `Shape` — and the placement guard of
   X72 refuses anything that would still intersect one, so a shelf behind a hidden facade takes
   the depth the facade allows instead of sticking in it or crossing it. The awkward cases are
   decisions: a hidden body wholly inside the cell (the rectangle goes around it; a shape that
   cannot be drawn around it is refused with the niche's own sentence), a hidden part thinner
   than the board, a hidden facade that spans the whole opening (the part takes what is left,
   and a void with nothing left says so), and a hidden container whose children are visible
   (the subtree goes with it, as the frame already does).
  done: With a frame's facades hidden the shelf fill measures the case's own void instead of
   the phantom planes and the ghost lands in the clear space, its depth stopping at the
   facade's inner face; a pointer's ray passes a hidden panel to reach the space behind it,
   while a part created or carried there stops at the hidden faces and never intersects them; a
   hidden body standing inside a void is a boundary of the part and never a slice of it, in
   both builds; the niche row, the distance fields, the picking and the drawing name the same
   parts in both states; showing a part again restores every reading and changes no document
   field; a hidden container hides its whole subtree for the tools as it does for the frame;
   cargo xtask ci is green.
  `files: crates/ui/src/view_flags.rs,crates/model/src/project/niche.rs,crates/model/src/project/move_query.rs,crates/model/src/project/surface.rs,crates/model/src/project/fill.rs,crates/ui/src/void.rs,app/src/document.rs,app/src/scene_build.rs` — `36a1cd4`

- `[x]` **X84** a cut is measured from where the piece stands
  Why: In the fourth session the block was carried past the bar and then pulled 1330 mm back
   across it when the pointer drifted 1 mm into the bar: every axis cut is resolved against the
   piece start box, though the piece has been carried since. The owner reads it as the block
   jumping left when it should have stood still.
  Shape: limits_from(from, proposed) resolves the same proposal from the place the piece was
   drawn last frame; limits(proposed) stays limits_from(ZERO) so the axis handle and every
   existing answer keep their meaning. The destination is still asked whole first; a retry pass
   may free an axis but never lose the face the drag rests on, and a sliver thinner than
   CONTACT_MM at a face is a touch, not an overlap.
  Done: A piece resting past a body stays at its face when the hand drifts into the body; the
   near side is still reachable when the destination is there; both directions along one body
   answer the same; the gate is green.
  `files: crates/model/src/project/move_query.rs, app/src/translate_gizmo.rs, docs/decisions/0173-a-cut-is-measured-from-where-the-piece-stands.md` — `21d94d2`

- `[x]` **X83** a crossing rests on the face
  Why: The owner pushes a horizontal block butt-first into a vertical one: it passes to the
   other side at once, though it should rest on the face and wait for the cursor to cross the
   body. The other direction freezes and jumps much later, so the two directions read
   differently. The hold compares the answer step with the hand step, and this session logged
   single frames of 500-1000 mm of pointer travel, so a leap across a whole body is read as
   ordinary travel and no frame shows the piece on the face.
  Shape: The rule names the body a straight carry passes through (crossing) and the body the
   hand itself stands inside (enclosing), both with the swept face test the drag already uses.
   The carry holds a granted place beyond such a body for JUMP_HOLD, whatever the hand did, and
   keeps the hold age at zero while the hand is still over the body: the wait is the cursor
   crossing the thickness, not a timer. A cut answer is the face already and is never held.
  Done: A place granted whole beyond a body is held on the face for JUMP_HOLD even when the
   pointer crossed the whole body in one frame; the hold keeps no age while the hand is over
   the body and takes the place the frame it comes out; both directions read the same; a cut
   answer and a typed number are never held; the gate is green.
  `files: app/src/translate_gizmo.rs, crates/model/src/project/move_query.rs, docs/decisions/0172-a-crossing-rests-on-the-face-before-it-is-taken.md` — `6af7d87`

- `[x]` **X82** the free slot is not refused by the staircase
  Why: The third logged session has the hand at (-171, 236) and the answer at (-94, 236), held
   while the pointer kept travelling, and its own blocked=none proves the destination was free.
   resolve walks the axes one at a time and never asks the placement they add up to, so a free
   slot in front of a standing body is refused whenever every single axis to it is blocked. The
   owner reports exactly that: a vertical part passes neither in front of nor behind a
   horizontal one, though the space is there.
  Shape: resolve asks the whole proposal first: if the piece may rest there, all three axes are
   taken and no contact is reported. Only a refused destination is walked axis by axis, as 0166
   decided, and the walk runs up to three passes so an axis cut on the first pass is asked
   again once the axes after it have moved the piece. The boxes of the log become a test: the
   slot in front, the slot behind, and the face when there is none.
  Done: The whole destination is asked before the axes and taken when free; a cut axis is
   re-asked after the other axes move; the reported board passes the standing body in front of
   it and behind it; in line with it, it still stops at its face; the gate is green.
  `files: crates/model/src/project/move_query.rs, docs/decisions/0171-the-place-the-hand-names-is-asked-before-the-axes.md` — `c6eb56a`

- `[x]` **X81** a report names the part that was in the way
  Why: The second logged session ends with a panel clamped at 466 mm while the hand asks for
   2000 and the line names only Id(3). The numbers prove a cut and say nothing about what cut
   it: a bare id, no kind, no box, and the take line names a bare Id(2) as well. The owner
   reads the same log and reports that the mechanism works wrongly; neither side can settle it
   while the obstacle is anonymous.
  Shape: The take line names the piece: its kind, its name, its world box and its axes. The
   carry line gains blocked=…, read from MoveRule::blocked_at — one function beside free_at, so
   the log cannot disagree with the drag — naming the first part the destination would rest
   inside, its kind and its box. The node itself is never printed, because it carries geometry.
  Done: A take line says kind, name, box and axes of the piece; every cut names the obstacle
   with its kind and world box; the log and the drag ask one predicate; the log line still
   carries no geometry, and the gate is green.
  `files: app/src/translate_gizmo.rs, crates/model/src/project/move_query.rs, docs/decisions/0170-a-report-names-the-part-that-was-in-the-way.md` — `6cb53d6`

- `[x]` **X80** what the log said: a typed zero, a lost bond and a stale hold
  Why: The owner ran the first logged session and said "плохо. проверь логи". The file answered
   it in three lines: a typed zero in the carry distance field pinned the piece where it
   started (0156 frames with asked far from delta and typed=Some(0.0)); the hand-to-pivot bond
   was lost on 135 frames and the plane branch never read it again, so the carry stood still;
   and the jump hold was skipped under a typed number, leaving a pending request that would
   fire on release. The file also carried 13 megabytes of winit tracing spans because the sink
   passed every crate at debug.
  Shape: A typed zero releases the face in Distances::place, as every other typed field of the
   editor already does (0092, 0126), so the pointer takes the face back instead of the part
   being pinned against the wall. The carry reads the hand-to-pivot offset again when it is
   missing, from the piece pivot and the hand. A typed number clears the pending jump. The sink
   passes another crate target only at a warning, and the carry line gains typed, held, offset
   and the hold age so the next report separates a rule cut from a pin from a hold.
  Done: A zero typed into a carried piece distance field gives the axis back to the hand and
   the piece follows again; a camera gesture or a pointer that leaves the work area no longer
   freezes the carry; a pending jump does not fire when a typed number is released; the log of
   a session is kilobytes of our own lines and no winit spans; every existing carry and
   distance test keeps its answer, and the gate is green.
  `files: crates/ui/src/distance.rs, app/src/translate_gizmo.rs, app/src/logging.rs, docs/decisions/0169-what-the-move-log-said.md` — `352e7da`

- `[x]` **X79** the shell logs the carry, and a jump is measured against the hand
  Why: The owner asks for a logging system that carries what a bug report needs: the moved
   object, its coordinate and the cursor's coordinate, so a gesture's behaviour can be read
   after the fact. The same message reports a regression of X78: the piece now jerks and jumps
   instead of travelling. The cause is that the jump hold compared the rule's answer with an
   absolute 15 mm step and the pointer's travel in points, so at a zoom where a few points mean
   a hundred millimetres every ordinary frame was read as a jump and the piece moved in held
   bursts.
  Shape: The shell installs one `log` sink before anything else: a level from `Z3L_LOG`, a file
   beside the workspace settings bounded at four megabytes, and the same lines on stderr. The
   carry writes the node, its world coordinate, the cursor in points and the hand's world
   point, the delta the hand asked for, the delta the rule resolved, the delta the carry
   settled on, the jump hold and the contact/capture per axis; the held drag writes its
   one-axis line; take, drop and cancel are info. The jump is measured against the hand's own
   travel instead of an absolute step, so ordinary motion is never held.
  Done: A run with `Z3L_LOG=debug` writes a line per carry frame with the object, its
   coordinate, the cursor and all three deltas, and `Z3L_LOG=trace` adds the rest; the file is
   bounded and appended across runs; a drag at any zoom follows the hand without held bursts,
   and only a crossing of an obstacle is still held for a sixth of a second; the gate is green.
  `files: app/src/logging.rs, app/src/main.rs, app/src/translate_gizmo.rs, README.md` — `a412412`

- `[x]` **X78** a jump is held for a moment, and the object move has its icon
  Why: Two reports from the owner on one screenshot of a room. A post carried up to a slab's
   underside flips above the slab instead of resting on its face, which is the destination rule
   (ADR 0166) crossing a solid in one frame as soon as a place beyond it fits; the owner asks
   for a pause or a tolerance. And the object move has no icon while the band that names it is
   text only, though the drawn drafts were approved.
  Shape: The carry holds a crossing for a sixth of a second: a delta that changes by more than
   a jump while the pointer barely moved keeps the previous place, and the same request
   accepted for 0.12 s takes it, so a hand that brushed a face never flips the piece over it. A
   face that already touches and is carried into is a stop at zero rather than a separation
   that never closes, which is what made a post touching a slab report no contact at all. The
   approved drawing becomes assets/icons/transform/object.svg on the 24 grid with the model and
   modify inks, a transform theme in the icon module, a row in assets/README.md, a glyph beside
   the object move's word in the status band, and the fallback to the word alone while the
   asset is a placeholder.
  Done: A post whose top touches a slab's underside stays on the face while the hand asks for a
   few millimetres into the slab, and only a continuing request carries it past; a jump is held
   for a sixth of a second and a fast hand takes it at once; the object move's band cell draws
   the cube and the arrow, the other three cells stay words, and the narrowest window still
   holds the band; every contact test keeps its answer where the face is already touched, and
   the gate is green.
  `files: app/src/translate_gizmo.rs, crates/model/src/project/move_query.rs, assets/icons/transform/object.svg, crates/ui/src/icons.rs, app/src/status_bar.rs` — `5f156d1`

- `[x]` **X77** the move answers the place the piece lands, not the path it crosses
  Why: Reported by the owner with screenshots of a room: two plain solids side by side stop at
   each other and cannot be carried into the empty place beyond, a post cannot be carried past
   a shelf although the shelf ends, and in perspective and axonometry solids and the carcass
   pass through one another. The cause is one: the drag's rule stops the piece at the first
   contact along the path it would travel from the start boxes, so a place beyond an obstacle
   is unreachable by construction, whatever the pointer names. The second cause is that a shell
   and a body are skipped entirely in a drag and in the write's guard, because a shelf in a
   case is a body in a shell, so a carcass and a solid body pass through one another. The third
   is that per-axis answers combine into a destination inside a turned neighbour, which is how
   a diagonal carry cuts a corner.
  Shape: The rule answers the place the drag lands and not the path it crosses: when the whole
   destination the hand names is free, the piece goes there, jumping past whatever stood
   between; when it is not, each axis is clamped at its first contact as today, so the piece
   rests against the face and slides along it. The destination is tested once for the whole
   vector, so a diagonal carry cannot cut a corner. A legitimate nesting — a piece in a case, a
   unit inside another — is legal and never a collision, while a shell and a body that are not
   nested stop each other like two bodies. The write's guard uses the same pair rule, so the
   drag and the release cannot disagree.
  Done: A solid body carried to a free place beyond another body lands there, while a carry
   into it stops at its face; a body and a carcass stop each other, and a shelf inside a case
   still moves freely within it; a diagonal carry whose per-axis limits would cut a corner is
   stopped; a panel jumps past a neighbouring panel into the free place beyond it; every
   existing contact test keeps its answer where the destination is blocked, and the gate is
   green.
  `files: crates/model/src/project/move_query.rs, crates/model/src/project/surface.rs, app/src/translate_gizmo.rs` — `fb7b34a`

- `[x]` **N27** the move tool is the object move, and a solid body stops at another body's faces
  Why: Owner's request: the move tool is not the furniture move but the object move, because a
   room is furnished with more than furniture. For a furniture piece nothing changes; a solid
   body is measured by its own faces, so a turned chair stops where its face touches the
   neighbouring body and not at the world box around it. Today the tool measures every contact
   by the world box, which is early by the slack a turned body carries, and it hands a
   selection that is not furniture no snapping at all, so a loose body cannot be put on the
   floor plane or on the millimetre grid. The object-type idea (wall, floor, ceiling, skirting,
   cornice) is the owner's next step and is not this entry.
  Shape: The rule keeps one reading of contact for a furniture piece and gains an exact one for
   a solid: a swept separating-axis test over the two boxes' own frames answers the first
   distance at which the two bodies touch along the world axis the drag runs on, so a contact
   is measured by faces and a contact that cuts the drag is rounded towards the start, which
   keeps the release from landing a placement the write would refuse. Every selection now gets
   the workspace's snapping; a face-measured mover takes the coordinate planes and the grid and
   not another part's world box, which is the coarse reading it no longer shares. The mode, its
   id, its key row and its words become Object, and the model's furniture_axes becomes
   move_axes because it answers any move. The furniture rules (a panel's thickness axis, a
   floor class's height) are untouched. An ADR records the split between the two readings and
   why.
  Done: A turned solid body dragged at a neighbour stops where its own face meets the
   neighbour, which is later than the world box would allow, and its contact is the same box
   the write refuses; a loose body drags onto the coordinate planes and the 1 mm grid and is
   not captured by another part's world box; every furniture test keeps its answer and its
   snapping; the mode reads Object in the band, the menu and the Settings page, and draws no
   icon yet; the gate is green and an ADR records the decision.
  `files: crates/model/src/project/move_query.rs, crates/model/src/project/surface.rs, crates/ui/src/transform.rs, crates/ui/src/inventory.rs, crates/text/src/messages.rs, app/src/gizmo_math.rs, app/src/translate_gizmo.rs, plan/map.md` — `8366029`

- `[x]` **X76** a carried frame leaves its grips behind, and a grip steals the press that puts it down
  why: Observation, from the owner's screenshot of a carried frame: the frame moved and its
   fourteen grip rings — eight corners and six face centres — stayed at the place it was taken.
   The grips are read from the document (`BoxGrips::paint` calls `BoxBody::read(project,
   node)`) while the body itself follows the drag preview, so the overlay and the picture
   disagree for the whole gesture. The same defect has a second half: a press within the grip
   radius of a carried piece arms a grip resize instead of putting the piece down, because the
   shell's `gripping` test does not know a piece is in hand. Expected: a carried frame's grips
   ride with it as its corner brackets do, and a press while a piece is in hand is the click
   that puts it down.
  shape: The grips read the drag preview the way the brackets and the renderer do: the body's
   world placement goes through `Preview::moved_world` before a grip is projected, so a
   translation carries the rings and a rotation turns them. The shell does not arm a grip while
   the furniture move holds a piece — the next press belongs to the carry.
  done: The grips of a frame in hand sit on the previewed body in both tools, a translation and
   a rotation; a press over a carried piece's old grip position puts the piece down instead of
   resizing it; the envelope's sides are unchanged by that press; `cargo xtask ci` is green.
  `files: app/src/box_gizmo.rs,app/src/document.rs,app/src/frame_tests.rs` — `0d85cba`

- `[x]` **X75** a carried frame lags the hand and leaves its brackets behind
  Why: Observation, from the owner, on the furniture move of a product frame bound to the
   floor: the piece is sluggish — the hand runs away from it — and the corners of the open
   frame's box stay where the piece was taken. Reproduced headlessly: a floor frame carried 120
   points across the screen and 80 points up moves 113 and 21, so the piece gives up nearly
   four fifths of the hand's depth travel. Cause: a multi-axis carry meets the pointer on the
   camera's own plane through the pivot, and the rule then keeps only the horizontal part of a
   delta measured in a plane turned away from the floor (ADR 0163), so the piece follows a
   foreshortened copy of the hand. The brackets are collected from the frame the document still
   holds, so they do not follow a drag at all. Expected: the piece keeps the screen offset the
   click gave it, on the plane its class allows — exactly under the hand across the floor — and
   every mark of the carried level, its corner brackets included, travels with it.
  Shape: The carry meets the pointer on the plane the piece may travel in: the plane across the
   one axis its class forbids (the floor, for a floor-bound frame), the camera's own plane only
   when its three axes are free, and the hand-travel latch for a one-axis panel. The brackets
   read the drag's preview instead of the placements the document still holds.
  Done: A floor frame carried by the hand keeps it, its screen travel equal to the pointer's on
   both screen directions; the same frame given a wall class still follows the hand in depth; a
   one-axis panel keeps its latch, its fields and a typed gap; the brackets of an open product
   or group stay on the carried level while the piece is in hand and go back with it; one carry
   is still one undo entry; ADR 0163 is amended and the map rows are current; cargo xtask ci is
   green.
  `files: app/src/translate_gizmo.rs,app/src/gizmo_math.rs,app/src/open_frame.rs,app/src/document.rs,app/src/frame_tests.rs,docs/decisions` — `c718acb`

- `[x]` **X74** the furniture move carries a frame on every axis its class allows and slides it along the face it meets
  Why: The owner, using the furniture move on a product frame: it travels one axis at a time,
   so a floor cabinet cannot be moved across the floor in one gesture and a wall cabinet cannot
   be moved in the plane it hangs on. The mount trait therefore reads as if it changed nothing,
   because both a floor and a wall piece still obey the one axis the hand named (ADR 0151,
   0153). The frame must move on every axis its class allows — a floor piece across the floor
   plane, a wall piece everywhere — and a face it meets must stop that axis while the other
   axes keep carrying it.
  Shape: The furniture carry stops latching one axis for a piece that allows more than one. The
   pointer is taken on the camera plane through the pivot at the take, so the hand proposes a
   world vector; the rule answers it axis by axis, which zeroes a forbidden axis (a floor
   piece's height, a panel's in-plane directions), snaps each allowed axis to a surface and
   cuts each at its first contact, so a blocked axis stops and the free ones slide. A one-axis
   piece keeps the hand-travel latch, its exact-distance fields and its typed gap; a multi-axis
   delta keeps the fields on its dominant allowed axis, so a typed number still pins one face.
   The class is read where the rule is read, so changing it in the properties card changes the
   carry at the next take, and a test proves the same node changes behaviour.
  Done: A floor frame carried diagonally changes X and Y in one gesture and keeps Z; the same
   frame given a wall class moves in Z as well; a frame dragged into a neighbour stops on that
   axis and still slides on the free one; a panel still offers its thickness axis only and its
   typed gap still lands; one carry is one undo entry; an ADR and the map rows are updated;
   cargo xtask ci is green.
  `files: app/src/translate_gizmo.rs,app/src/gizmo_math.rs,crates/model/src/project/move_query.rs,app/src/document.rs,app/src/frame_tests.rs,docs/decisions` — `cea450f`

- `[x]` **X72** a moved part may come to rest inside another part
  Why: Observation, from the owner: moving a panel, it sinks into an existing panel, and a part
   may not come to rest inside another part. Reproduced paths around the rule, not inside it:
   the rule itself does stop a body at a body (`stop_at_contact` and `blocking` in
   `move_query.rs`), but it lives in the two gestures and nowhere else. The free `Move` applies
   no rule at all — ADR 0147 took the furniture rule out of it on purpose and X65's `Done` asks
   that "the free move still crosses it" — and `Project::move_rule` answers `None` for a panel
   that is not inside a product (`is_furniture`), so the furniture tool also carries a loose
   board through anything. Nothing checks the placement a command is asked to write, so a typed
   distance, a palette command or the assistant can land a board inside another one as well.
   The live path the owner used is not yet identified; the two open ends are the free move and
   a panel outside a product.
  shape: The overlap is refused where the document is written, not only where a gesture runs.
   The contact test leaves `MoveRule` for one function both callers share (`move_query.rs`,
   with `SurfaceBox` from `surface.rs`): does the moved box come to rest inside a body or a
   shell that stays behind — touching faces are contact, not overlap, and a panel inside its
   own frame's envelope stays legal. **Hidden parts are read like visible ones**: the owner's
   rule is that a switched-off part is invisible to the eye and to the pointer, never to the
   material (X73), so the guard asks the document, not the sight. The placement commands
   (`node.placement.set`, `set_many`, the fill's `panel.create`) ask it and refuse with a
   sentence naming the part that is in the way; the gestures read the same answer first, so a
   piece stops at a neighbour instead of reaching a refusal, and the free move stops too —
   X65's crossing clause is narrowed by a new ADR, and the class rules, the axes and the
   snapping are untouched. A part that already overlaps another (a file written before this
   rule) can still be dragged out of the overlap: the test refuses a placement that makes an
   overlap worse, never one that makes it no worse. The awkward cases are decisions, not
   accidents: a selection of several parts against one neighbour, a shell inside a shell (a
   piece inside a case), a board that only touches, and a part carried with the camera moving.
  Done: A panel dragged into another panel stops at its face with both moves; a
   `node.placement.set` asked for the same overlap is refused with a sentence naming the part
   and writes nothing, so the revision does not move; a part already overlapping another can be
   dragged out of it and a touch is never a refusal; a piece inside a case's envelope and a
   selection of several parts are unchanged; the ADR is written, `docs/decisions/README.md`
   lists it and X65's clause is amended; cargo xtask ci is green.
  `files: app/src/translate_gizmo.rs,crates/model/src/project/move_query.rs,crates/model/src/project/surface.rs,crates/engine/src/node_cmds.rs,crates/engine/src/panel_cmds/panel.rs,app/src/frame_tests.rs` — `f9209e6`

- `[x]` **N26** a carried piece measures its gaps with arrows in the scene, not with a card at the hand
  why: The carried piece's gaps are numbers in a floating card that follows the hand, and the
   owner wants them in the scene: a yellow dimension arrow from each face of the piece to the
   nearest object, with the editable gap in the middle of the arrow (reference picture:
   Screenshot from 2026-11-18 20-58-47.png in the owner's Pictures/Screenshots). The card also
   lags while a number is typed, drifts away from the pieces it measures after a camera gesture
   and covers the pointer (X66, X69).
  shape: The two gaps of a carried piece are drawn where they are measured: an arrow per side,
   from the piece's face along the free axis to the surface the model probe answered (the
   neighbour face or the frame envelope), in the assistant line's own ink, with an arrowhead at
   each end and the gap's own field sitting in the middle of the run. The field keeps the
   behaviour the card's rows have today: a fresh gesture focuses the nearest one, Tab walks to
   the other, Enter locks the face, Escape gives the piece back to the pointer; a number pins
   the face and the opposite face follows. The floating card stops being drawn for the
   furniture move; the inspector still shows the same two rows from the same Distances (ADR
   0119), and a box, a pull or a niche keeps its own card. The arrows are clipped to the work
   area, thin and dim while the pointer is elsewhere, and are never drawn for a panel whose
   axis points at the camera. Dimension grammar comes from the drafting skill.
  done: A carried piece draws two arrows with a field in the middle of each; typing, Tab, Enter
   and Escape behave as the rows do today; the furniture move draws no card; the inspector
   still measures the same numbers; the arrow is not drawn for a view-aligned axis; a frame
   test drives a gap through the arrow's field and asserts one command and one undo entry.
  `files: crates/ui/src/distance.rs,crates/ui/src/tool_card.rs,app/src/document.rs,app/src/gizmo_math.rs` — `4870510`

- `[x]` **N20** the card says the position its class gives, and never asks
  Why: The owner asked the card for a position, then settled it: the class already dictates how
   the piece moves (mount: Floor | Wall, ADR 0097), so a product field would be the second
   source of truth that record refuses. What is missing is only the sentence: the card names a
   class code and says nothing about whether the piece stands or hangs, so a maker cannot tell
   why a floor cabinet will not rise or why a wall cabinet does. No field and no filter: the
   card reads the trait.
  Shape: One row under the class: the position the chosen class carries, said in words (On the
   floor / On the wall), dim, and absent when the class has no mount rather than guessed. The
   class list is not filtered and the class stays the only stored fact. A height above the
   floor stays the makers placement, as ADR 0100 has it, so the row describes the rule and
   never sets a height. Two messages carry the words in both languages.
  Done: A headless test opens the card on the placeholder and reads On the floor, chooses the
   wall cabinet and reads On the wall; a class without a mount draws no position row; cargo
   xtask ci is green.
  `files: crates/ui/src/product_card.rs,crates/model/src/class.rs,crates/text/src/messages.rs` — `2cb817d`

- `[x]` **O7** a package travels between workshops
  Why: A workshop extends its directory and hands the extension to another workshop.
  Shape: Package export and import as a folder, connect and disconnect, requires on registry
   revisions, a refusal on a duplicated id.
  Done: A package exports from one directory and imports into a second unchanged, and an id
   clash is refused with both packages named.
  `files: crates/catalog/src/source.rs,crates/project/src/lib.rs` — `b2a651b`

- `[x]` **N19** the frame is drawn by the box tool it inherits from the cuboid, not by a card beside it
  Why: The owner: the frame kept the old card where the box has a typed height field, and the
   base tool should be one tool shared by both. Today the frame draws through BoxDraft, which
   labels its two base sides but takes no typed number, while the cuboid draws the sketch
   rectangle with the sketch field plates and pulls on Z; and the frame pull shows a titled
   pane with a check and a cancel where the cuboid shows the lettered H plate. Two gestures
   that build one document read as two tools.
  Shape: The frame drawn method arms the same rectangle tool the cuboid cell arms, differing
   only in the owner of the commit (product.create against object.primitive.create) and in the
   extras the tool card offers while the draft lives (the class, and later the footprint and
   the position). The base plates, the height pull and its lettered H plate, Enter, Esc and the
   one undo entry are the shared ones; BoxDraft keeps only what the envelope grips still read
   from it, or retires with box_tool.rs.
  Done: The frame cell and the cuboid cell draw the same base plates and the same H plate,
   compared row for row in a headless test; Esc drops the frame draft with no history entry;
   one undo takes the created frame; cargo xtask ci is green.
  `files: crates/ui/src/editor/creation.rs,crates/ui/src/box_tool.rs,app/src/document.rs,app/src/box_gizmo.rs,app/src/extrude_gizmo.rs` — `91063ac`

- `[x]` **O6** the directory application edits a package
  Why: A directory nobody can edit is a file format.
  Shape: A second binary on the same stack: item tree, card, property editor, kind registry
   view; one writer behind the lock, atomic writes, revision bumped last.
  Done: A new item with properties is authored, saved and read back by the editor on a new
   revision.
  `files: crates/catalog/src/source.rs,crates/catalog/src/wire.rs,app/src/cli.rs,app/Cargo.toml` — `bcd0aa1`

- `[x]` **N17** the frame says its footprint: a cut corner or a closed contour, and a height
  Why: The owner asks for a frame whose reserved space is not a box: a corner kitchen cabinet
   with one 45-degree cut, an end table with a 120-degree cut, and a frame drawn from a closed
   contour. Today an envelope is three lengths, and ADR 0097 says in so many words that a
   sloped or reshaped one is a new field and a new decision, so neither the card nor the
   gesture can offer a shape before that decision is recorded.
  Shape: The envelope gains a footprint beside the height: the rectangle it is today, a
   rectangle with a cut at one or more corners, or a ring drawn on the plane, which is always a
   reserved space and never a body. A cut stores offsets, not an angle: the offset along each
   edge is what a maker measures on a board, and it reuses the panel corner language,
   CornerFeatureKind (a chamfer of two setbacks), so one corner rule serves both. The corner
   type is chosen first and names the row that follows: a chamfer offers its two offsets, a
   fillet offers a radius only after the type is chosen. x and y stay the footprint bounds so
   the readers, the export and the containment diagnostic keep an answer; the scene draws the
   wireframe from the footprint, picking and the grips read it. An ADR records the footprint
   and the corner rule (offsets for a chamfer, a radius for a fillet, the type first).
  Done: A frame with a 45-degree and a frame with a 120-degree cut round-trip through the file
   as their offsets, draw as the cut outline and are picked inside it; a fillet radius is
   offered only after the corner type is chosen; a contour frame stores its ring as a reserved
   space and pulls its height; a cut that eats an edge is a readable refusal; the ADR is
   written and docs/decisions/README.md lists it; cargo xtask ci is green.
  `files: crates/model/src/container.rs,crates/scene/src/lib.rs,app/src/scene_build.rs,app/src/box_gizmo.rs,crates/ui/src/product_card.rs` — `85cf0b2`

- `[x]` **O5** the editor hot-reloads the directory
  Why: The directory is edited elsewhere; the editor must apply a new revision without a
   restart.
  Shape: A revision watched off the frame path, a snapshot swapped whole at a frame boundary,
   derived data recomputed and the undo history untouched.
  Done: A changed fixture re-derives a panel's thickness and appearance live, and the undo
   stack is intact.
  `files: app/src/main.rs,crates/engine/src/session.rs,crates/engine/src/lib.rs` — `b0d1c49`

- `[x]` **O1** the default catalogue carries a 4 mm back sheet, so an applied or a grooved back resolves
  Why: Every back type but the inset one needs a 4 mm sheet, and the default catalogue has no
   such row, so the back falls back to a number written in code, which is the one thing the
   material rules forbid.
  Shape: A row in the default catalogue carrying the thickness, the sheet stock and the edge
   rule; the back type's planes read it, and an out-of-range material stays a refusal.
  Done: A test builds an applied back and finds its thickness in the catalogue, and the inset
   type still uses the 18 mm stock.
  `files: app/src/scene_build.rs,crates/catalog/src/starter.rs` — `b0d1c49`

- `[x]` **O4** the directory crate holds the item envelope, the registries and the index
  Why: The directory needs one item envelope, typed registries and an index before anything can
   read it.
  Shape: ItemId, kind and trait registry, property registry with typed values, families, an
   index and the DirectorySource port; the mock, MaterialId and the old loader are deleted in
   this change.
  Done: A fixture package of every starter kind loads, a bad row is refused with its package
   named, and the mock file is gone.
  `files: crates/catalog/src/lib.rs,crates/catalog/src/directory.rs,crates/model/src/ids.rs,app/src/scene_build.rs` — `b0d1c49`

- `[x]` **N23** the empty-space ghost passes the neighbour it meets and holds at the frame envelope
  Why: The owner, confirmed: a panel created in a frame runs into its neighbour and freezes
   there, and should instead stop at it and jump past it into the free space beyond when the
   pointer crosses; and a panel dragged in an empty frame or an empty part of one should stop
   at the frame envelope and not vanish when the pointer leaves it. Today the fill plan clamps
   the free-axis position inside the void and refuses a point outside the open product, so the
   ghost sticks at the panel it meets and disappears at the envelope.
  Shape: The point under the pointer names the cell, the ghost fits that cell, and crossing a
   part re-plans into the cell beyond instead of clamping at the part; a point outside the
   product clamps to the envelope inner face, so the ghost stays the last part it could place
   and the click creates what was drawn. A refusal that is not about the pointer (a void
   thinner than the board, a cell that is not convex, another product void) still says why. The
   ghost stays the one measurement the click stores, so the picture, the numbers and the
   panel.create cannot disagree.
  Done: A shelf ghost at a panel face jumps into the cell above it when the pointer crosses the
   panel; a pointer beyond the envelope keeps the last ghost and the click creates that part; a
   void thinner than the board still refuses with its sentence; one click is one undo entry;
   cargo xtask ci is green.
  `files: crates/model/src/project/fill.rs,crates/model/src/project/niche.rs,app/src/fill_gizmo.rs,app/src/document.rs,crates/ui/src/void.rs` — `0bede29`

- `[x]` **X71** the carry's hand is the pointer the viewport hit-tests while it is displaced
  `files: app/src/document.rs,app/src/translate_gizmo.rs,app/src/frame_tests.rs` — `0fc31df`

- `[x]` **X70** the carry's hand is the drawn pointer, not the system one
  `files: app/src/translate_gizmo.rs,app/src/document.rs,app/src/main.rs,app/src/work_cursor.rs` — `58c2988`

- `[x]` **X69** the camera gesture hides the pointer and returns it to the middle of the piece
  `files: app/src/document.rs,app/src/translate_gizmo.rs,app/src/frame_tests.rs` — `3de9d3f`

- `[x]` **X68** a camera change keeps the piece and returns the pointer to it
  `files: app/src/translate_gizmo.rs,app/src/document.rs,app/src/frame_tests.rs,crates/scene/src/camera.rs` — `58324e5`

- `[x]` **X67** a camera change leaves the carried piece in the hand
  `files: app/src/translate_gizmo.rs,app/src/gizmo_math.rs,app/src/document.rs,app/src/view_layout.rs,crates/scene/src/camera.rs` — `bac9750`

- `[x]` **X65** a carried piece runs away from the cursor when the hand strays off its axis
  Why: Observation: a board carried by the furniture move runs away from the cursor —
   reproduced in the divider fixture, where a pointer that moved sideways and up carried the
   board from a 50 mm contact to -715 mm. Expectation: a carried piece follows the hand, and a
   hand that moves across the piece's axis moves it none. Suspected cause, reproduced: the
   distance is the projection of the pointer's ray onto the axis line, and that projection is
   unstable as soon as the pointer strays sideways from the line — 69 mm of travel for a purely
   sideways 200 pt move in the default view, hundreds once the axis is oblique to the view.
  Shape: Measure the carry by the hand's own travel: the pointer's movement projected onto the
   axis' screen direction, scaled where the piece was anchored (the local slope of the
   projection, one point along that direction). The anchor is set when the axis is named and on
   every frame that drives the camera, and nowhere else, so the piece cannot move when the view
   does and cannot run away from the hand. The held handle keeps the ray's projection, because
   a handle sits on the line.
  Done: A pointer that moves across the axis' screen direction changes no distance, at any
   distance from the line; a pointer that moves along it carries the piece at the scale the
   projection gives at the take; an axis that points at the camera carries nothing; the rule,
   the fields and the capture are unchanged; the divider stops at its neighbour and the free
   move crosses it (amended by X72, ADR 0162: the free move stops at the neighbour too, and the
   clause is the one X72's `Done` carries).
  `files: app/src/translate_gizmo.rs,app/src/gizmo_math.rs,app/src/frame_tests.rs` — `824859f`

- `[x]` **X66** the carried piece's distance plates take the hand that carries it
  Why: Observation: the gap plates of a carried piece take the hand. The card is anchored at
   the pointer, so the plates sit where the hand is; a press on one does not put the piece
   down, it starts the plate's scrub, which pins the number the drag passes through — the piece
   then jumps to that gap and stops following the pointer, exactly as the owner's two
   screenshots show. Expectation: while a piece is carried nothing between the hand and the
   scene may take the pointer, because the press that ends a carry is the drop. Suspected
   cause: `crates/ui/src/tool_card.rs` draws an interactive `egui::Area`, and the scrub of an
   unfocused row commits live, so the plate's number is pinned by the drag.
  Shape: The tool card of a carried piece is drawn without pointer interaction: it keeps the
   keyboard (the field nearest the pointer still takes it, and typing a gap still pins) and
   loses every pointer sense, so a press anywhere over it reaches the viewport and puts the
   piece down. The card of every other gesture keeps its pointer, because those gestures have a
   button in the hand and the plate is a deliberate target.
  Done: A press over the carried piece's card puts the piece down and commits one placement; a
   press-and-drag over the card scrubs nothing and pins nothing; typing a gap on that card
   still pins the piece; the fill, box, extrude and sketch plates stay interactive; frame tests
   and cargo xtask ci are green.
  `files: crates/ui/src/tool_card.rs,crates/ui/src/distance.rs,app/src/document.rs,app/src/frame_tests.rs` — `824859f`

- `[x]` **N25** a piece in hand blocks nothing, and the view may move under it
  Why: The owner, using N24: a piece in hand freezes the viewport. Orbit, pan and the wheel are
   refused while the furniture move carries a piece, so a maker who took a shelf cannot turn
   the case to see where it goes — the one thing the gesture needs. The hold owns no button, so
   nothing about it justifies owning the view.
  Shape: The carried piece blocks nothing. The camera keeps orbiting, panning and zooming under
   it, and when the view moves the hold re-anchors instead of dragging the piece along: the
   piece keeps the distance it has and the hand continues from the pointer's new place. A tool
   that takes the pointer — a draft, a pull, a grip, the ruler, the niche — or a command that
   lands under the hand (undo, delete, a rename) ends the hold and puts the piece back, because
   the hold is not a mode. A held handle drag keeps freezing the camera, because the button
   under the hand is the drag.
  Done: The camera orbits, pans and zooms while a piece is in hand, and the piece keeps its
   distance through all three; the next click commits exactly the distance the pointer added
   after the view moved; a running tool or a command under the hand puts the piece back and
   writes nothing; a held handle drag is unchanged; frame tests and cargo xtask ci are green.
  `files: app/src/translate_gizmo.rs,app/src/document.rs,docs/decisions` — `fb6bbc0`

- `[x]` **N24** the furniture move takes the piece by a click and follows the pointer
  Why: The owner, after using the furniture move: the axis handles are the wrong affordance for
   it. A maker takes a board by its body, not by a three-point arrow, and holding the button
   through a 900 mm slide is the long part of the gesture. The tool should take the piece by
   any part of it, follow the pointer with the hand free, and put it down on a click.
  Shape: The furniture move draws no handle. A click on a movable body selects it and takes it
   in hand; from then on the pointer moves it through the document's rule (contact, snapping,
   the panel axis) with no button held, and the next click drops it as one
   node.placement.set_many. Escape puts it back, changing the mode does the same, and the
   camera stays put while a piece is in hand. The axis is the one allowed axis nearest the
   hand's travel, latched at the first movement, so a panel still only slides along its
   thickness; the exact-distance fields stay, because a latched axis is a number as before. The
   free Move keeps its arrows and its held drag.
  Done: A click on a panel in Furniture mode takes it and the pointer moves it with no button
   held; a second click commits exactly one revision and one undo entry; Escape restores the
   placement and writes nothing; a click that never moved writes nothing; a body that may
   travel nowhere is not taken; a locked piece is not taken; the handles are not drawn and no
   axis is offered in that mode; the panel still moves only along its thickness; the fields
   still type a distance; the frame tests and cargo xtask ci are green.
  `files: app/src/translate_gizmo.rs,app/src/document.rs,app/src/status_bar.rs,crates/text/src/messages.rs,docs/decisions` — `e0e7a37`

- `[x]` **O3** the directory is decided: three records, the domain documents and the task seeds
  Why: The catalogue outgrew two typed tables; the directory replaces it and every later task
   reads this decision.
  Shape: 0148, 0149 and 0150 plus the rewritten domain documents and a pointer where
   materials.md was.
  Done: The three records are accepted, the two domain documents state the directory, and the
   task seeds exist.
  `files: docs/domain/nomenclature.md,docs/domain/technology.md,docs/decisions/README.md` — `9fec8f9`

- `[x]` **U113** select, move and rotate answer keys, and the status bar prints them
  Why: The owner asks for keys on the move and rotate tools. The three transform modes are one
   band in the status bar and have no row in KEYS, so the Settings page cannot list them and a
   maker reaches for the mouse for the most repeated action in the editor. The one key table
   already exists (ADR 0117); the modes are simply absent from it.
  Shape: A binding for Select, Move, Rotate and the furniture mode in inventory KEYS on chords
   no drawing tool owns: W, E and Q beside Shift+W for the furniture move, with R, M, A, C, N,
   L, P, V, G, S, O and F already taken by the sketch, the view and the niche. The status bar
   band carries four states and prints the key beside each label; the node context menu of the
   tree and the viewport offers the four as session entries in the list node_actions already
   keeps beside its command rows, because they are view state and write no document, so the
   action has a place in the menu without inventing a command for it. The Settings page lists
   the same four rows with the rest. This task waits for N22 to exist before it names the
   fourth mode; the other three can land first.
  Done: Each of the four modes answers its key from the shell and the status bar cell shows it;
   the tree row menu and the viewport menu both offer the four; the Settings shortcut list
   holds all four; the table test proves no two rows share a key and every binding prints in
   both languages; cargo xtask ci is green.
  `files: crates/ui/src/inventory.rs,crates/ui/src/node_actions.rs,app/src/shortcuts.rs,app/src/status_bar.rs,app/src/main.rs,crates/text/src/messages.rs` — `ff5f72c`

- `[x]` **N22** the furniture move is a tool of its own, and the free move keeps its hands off the rules
  Why: The owner wants two tools: the move the editor has stays free and turns the furniture
   snapping off, and a furniture move moves the object by its own rules — a panel only along
   its own Z, parallel to its AB planes, and a frame on all three axes but seeing its
   neighbours and the walls — with the distance to the neighbour or the wall as a typed field.
   Today one Move mode decides under the hand: the translate gizmo reads Project::move_rule and
   applies it whenever the selection is furniture (ADR 0102, 0103), so the same handle is free
   for a loose body and ruled for a cabinet, and no rule gives a panel its own axis at all.
  Shape: A fourth transform mode beside Select, Move and Rotate: Furniture. It is view state,
   never a document field (ADR 0113). The free Move drops the furniture rule and its snapping
   and keeps the exact micrometre drag; the exact-distance fields stay in both modes, because
   they are the typed path (ADR 0103) and the owner answer named the snapping alone. The
   furniture mode reads Project::move_rule and adds the panel rule: a panel moves only along
   its own local Z, the thickness axis, so it slides parallel to its AB planes and keeps them
   parallel; a shelf rises and falls in its thickness direction, a divider crosses the case, a
   back moves front and back. A frame moves on the three world axes subject to its class mount
   (a floor piece keeps its height), and every drag sees the neighbours, the planes and the
   walls of N21 through the distance fields of this mode, whose number is the gap to the face
   that stops it. One undo entry per drag either way; an ADR records the mode, the panel axis
   and what the free move no longer does.
  Done: A selected shelf moves only along its own Z and the two in-plane directions are
   refused; a divider moves across the case and a back moves in and out; a frame moves on three
   axes, a floor frame keeps its height and a neighbour stops it; the free move keeps a 97.4 mm
   drag where the furniture move stops at the wall, and the free move does not snap; a typed
   gap lands the face where typed and the field shows the same number; each drag is one undo
   entry; the ADR is written and docs/decisions/README.md lists it; cargo xtask ci is green.
  `files: app/src/main.rs,app/src/gizmo_math.rs,app/src/translate_gizmo.rs,app/src/status_bar.rs,app/src/document.rs,app/src/frame_tests.rs,crates/model/src/project/move_query.rs,crates/text/src/messages.rs,crates/ui/src/distance.rs` — `cd44446`

- `[x]` **N21** snapping is a setting: the coordinate planes, the frames and the tolerance
  Why: The owner: frames stick to each other, and the tolerance is the distance at which a
   piece jumps to another, like a snap; frames stick to the floor and to walls, and until a
   house-wall tool exists the virtual coordinate planes stand in for walls. Today the capture
   is two constants in move_query.rs (a 5 mm snap radius and a 1 mm grid), no coordinate plane
   is a candidate, and the maker cannot turn any of it off. Snapping is a taste and a workshop
   standard, not a code literal, so it belongs in the workspace settings like the grid step and
   the gizmo handle.
  Shape: One Settings section for snapping (in the View page, beside the grid): the coordinate
   planes on or off, the frames on or off, and the tolerance in millimetres that captures a
   drag and makes the piece jump — it lands flush, because the tolerance is the distance at
   which the snap fires and not a gap to keep. The move rule reads a SnapSettings value the
   shell passes in instead of the constants; the candidates gain the X=0, Y=0 and Z=0 planes
   (the floor included) beside the envelopes, the virtual faces and the part faces, and a
   capture still lands the faces together. A captured face draws a line on the plane or the
   neighbour and its field says the number, so a snap nobody can see is not a drag that feels
   broken. One ADR records the settings, the planes as stand-in walls and the tolerance
   reading.
  Done: A workspace file round-trips planes, frames and tolerance; a frame dragged within the
   tolerance of Y=0 jumps onto the plane and one dragged past it does not; two frames within
   the tolerance meet flush and beyond it stay apart; turning the planes off leaves the piece
   where the pointer put it; the field at the captured face shows the same number the rule
   used; the ADR is written and docs/decisions/README.md lists it; cargo xtask ci is green.
  `files: crates/model/src/project/move_query.rs,crates/ui/src/settings.rs,app/src/document.rs,app/src/translate_gizmo.rs,app/src/settings_store.rs,crates/text/src/messages.rs` — `5499a93`

- `[x]` **U112** the product card aligns its numbers and folds its class list
  Why: The card is the numeric path into `product.create` and it is about to grow fields
   (shape, position, corners), so its layout is read before it grows. Today each row label is
   as wide as its word, so the three number boxes start at three different x positions; the
   class list is a flat scroll of seventeen rows with no fold, so a maker hunting a wall
   cabinet reads the whole registry; and the paragraph under the title explains a rule the
   maker learns from the frame itself and cannot act on.
  Shape: A fixed label column (LABEL_WIDTH, at least one tree indent) with the shared scrub
   field beside it, so Width, Depth and Height start on one x. The class list becomes the
   registry tree the picker already builds: a foldable family caption, a foldable type row that
   is itself selectable, and its variant rows under it, drawn with egui CollapsingState like
   the scene tree; the type that holds the current class opens by default and a search opens
   every group it keeps. `class_picker::product_rows` returns that tree instead of a flat list
   of depths. The ProductCardHint message is deleted from the table in both languages.
  Done: A headless test draws the card and finds the three fields labels and numbers on one
   column; a closed type lists no variant and the type of the chosen class lists them; the hint
   is gone from both languages; cargo xtask ci is green.
  `files: crates/ui/src/product_card.rs,crates/ui/src/class_picker.rs,crates/text/src/messages.rs` — `b2f83b7`

- `[x]` **T10** the group digits follow the order of the work, so the case is 10 and the seat is 80
  Why: T9 made the picker sort the families by a presentation constant, which leaves the stored
   digits in the old order: a saved node still says 30.01 for a cabinet and 10.01 for a chair,
   and every export, report and reader still meets the seating first. The order belongs in the
   number, as the owner asked: the case starts at 10 and the seat ends at 80. No build has
   shipped, so a code may still move (ADR 0094 immortalises a code only once it ships).
  Shape: Remap the group digit of every class, host prefix, constant, doc table and test
   literal: 30 to 10 storage, 70 to 20 boards and trim, 40 to 30 surfaces, 20 to 40 lying, 80
   to 70 general, 10 to 80 seating; 50 and 60 keep their numbers. The type and variant digits
   stay, so 30.01.02 becomes 10.01.02 and the old 10.01 cabinet of furntypes.md is true again.
   presentation_rank and PRESENTATION_ORDER leave model, the picker and the product card walk
   registry() again, and docs/domain/classes.md section 7 lists the families ascending, which
   is now the order of the work. One ADR records that the digits follow the work and that group
   10 names storage, not the product role of 0097.
  Done: The cabinet is 10.01 and the chair is 80.01, the picker's first family is 10 and its
   last is 80 with no sort in the way, and a test proves every stored code resolves to the row
   the document gives it.
  `files: crates/model/src/class.rs,docs/domain/classes.md,crates/ui/src/class_picker.rs` — `f0f8f68`

- `[x]` **T9** the class list follows what a shop builds, not the order of the digits
  Why: A person meets the chair and the stool first, because the registry is ordered by family
   code, and the case a cabinet shop actually builds sits third while the placeholder product
   sits last. The picker's tree and the product card's list are both drawn in that order, so
   the first row of a new product is a scene prop.
  Shape: A presentation order in model: a constant list of family codes and presentation_rank
   by family, read by the class picker's tree and the product card's list; the table stays
   ascending by code because ClassCode::row binary-searches it. The order is 80 general, 30
   storage, 70 boards and trim, 40 surfaces, 20 lying, 50 fittings, 60 appliances, 10 seating.
   docs/domain/classes.md section 7 lists the heading paragraph in that order, and the existing
   family test compares the sequence instead of only the count.
  Done: The picker's first family is 80 and seating is last, the product card's first row is
   the placeholder and its last row is a seating class, and a family left out of the order
   fails a test.
  `files: crates/model/src/class.rs,crates/ui/src/class_picker.rs,docs/domain/classes.md` — `7322b60`

- `[x]` **U111** the drawing's words are fixed: a spacing is the clear opening, left is +X, and the depth includes the fronts
  Why: Three conventions decide every wardrobe dimension, and one of them was read backwards:
   340 and 320 were taken as axis spacing instead of clear openings, so the shelves landed on
   the wrong side. The code has no single place that says which side Left is, so a reader
   infers it from a mirrored picture.
  Shape: A Side helper with Left, Right, Front and Back and their axes, tied by a test to
   OpeningView::Left.direction(); a glossary paragraph in docs/domain/products.md fixing that a
   spacing is a clear opening, that depth is measured with the fronts, and that left is +X seen
   from the front.
  Done: A test asserts that Side::Left equals OpeningView::Left.direction(), the glossary
   states the three conventions, and cargo test -p model -p ui is green.
  `files: crates/ui/src/display.rs,crates/model/src/,docs/domain/products.md` — `17aee02`

- `[x]` **U109** the floor can be made see-through, and its shadow survives it
  Why: Owner, 2026-11-17: the floor should be see-through with a setting of its own, and the
   shadow must not suffer for it. U108 already made the plate write no depth, so a part under
   the floor is drawn over it; what is still missing is the *tint*: the plate is opaque, so
   below the floor you see the part instead of the part through the floor. The plate is drawn
   first in the view pass today (ADR 0142 keeps it there so the shadow costs no pass of its
   own), so blending it over what stands under it is an order change, not a colour change.
  Shape: A workspace value beside the other display strengths:
   `ViewportSettings::floor_opacity` in whole percent, sanitized, reset by *Reset display* and
   round-tripped by the settings test, with its MIN/MAX and one `Message` in both languages. It
   reaches the frame as frame data (`SceneFrame::floor_opacity`, read at draw time) and the
   renderer writes it into the per-view camera uniform the plate already reads — one more `f32`
   slot, because `shadow_plane` is full — so a drag costs no upload. The plate moves after the
   opaque and translucent bodies and before the edges, with `ALPHA_BLENDING` and still no depth
   write (U108), so it tints what stands under it and never hides it; the grid keeps the order
   U108 gave it. The shadow keeps its strength: the fragment mixes the mask into the plate's
   colour and the plate's alpha is `max(floor_opacity, cover * shadow_strength)`, so a shadowed
   patch of floor stays as dark as it was at 100 % and a transparent floor cannot erase its own
   shadow. At 0 % the plate and its shadow are gone, because a shadow needs a surface to darken
   — the field's hint says so. An ADR is required: it changes two consequences of ADR 0142 (the
   plate's place in the pass and an opaque plate), and it records the `max()` rule and why the
   shadow is not a separate blended layer.
  Done: At 100 % the floor looks exactly as it does today; at 40 % a shelf standing at -500 mm
   is seen through the floor's tint and the shadow beside it is as dark as at 100 %; a change
   of the value re-uploads nothing; the value survives a restart and Reset display restores it;
   a shot pair at 100 % and 40 % with a part under the floor and a shadow on it; the ADR is
   written and docs/decisions/README.md lists it; cargo xtask ci is green.
  `files: crates/render/src/ground.rs,crates/render/src/pipelines.rs,crates/render/src/scene.wgsl,crates/render/src/lib.rs,crates/scene/src/lib.rs,crates/ui/src/settings.rs,crates/text/src/messages.rs,app/src/document.rs,app/src/ui_shots.rs` — `b03823d`

- `[x]` **L1** the shadow map reaches the panels, so a shelf darkens the back panel and a niche reads
  Why: The ground shadow says what stands on the floor and says nothing about the inside of a
   cabinet: every joint between two surfaces is lit as if the two panels met at an infinite
   plane, so a shelf, a niche and a drawer front read as flat paint. The products that convince
   here — SketchUp, Rhino, Revit, Fusion — light the model from one shadow-casting sun and let
   every surface receive it. Owner's decision, 2026-11-17: filed now, after the ground shadow
   lands.
  Shape: One depth map from the key light over the scene's box — 2048^2, rebuilt on the same
   keys as the ground mask (a scene upload and a key-light turn) — sampled with PCF in
   `fs_mesh`, so a face darkens where another face stands between it and the sun. Bias is the
   whole design: a normal-offset bias keeps a panel from shadowing its own face, and the two
   coplanar cases — a band on a panel face and a shelf flush with a side — decide the constant
   and get a test each. The map reuses the scene's world-space baked vertex buffers, so it
   costs one depth target and no new geometry, and it needs its own ADR because it is the first
   pass that can cost a frame. Rejected for now: cascades (one cabinet is two metres), a second
   authored sun (ADR 0036 keeps one key), and screen-space AO, which cannot darken a surface
   that nothing covers.
  Done: A shelf 300 mm inside an open cabinet darkens the back panel behind it, and stops
   darkening it when the cabinet is opened to the sky; a lit box shows no acne on its own faces
   and no gap under a band; the ground mask and the map agree on one light; a probe shows the
   map does not rebuild while the camera orbits; the ADR is written and `cargo xtask ci` is
   green.
  `files: crates/render/src/lib.rs,crates/render/src/scene.wgsl,crates/render/src/pipelines.rs,crates/scene/src/lib.rs` — `ad54f85`

- `[x]` **U110** the ribbon keeps the shadow switch alone, and its intensity lives in Settings
  Why: Owner, 2026-11-17: «интенсивность оставить в настройках, в ленте оставить только
   вкл/выкл тени». U107 put the amount beside the switch so one feature had one site; the owner
   then could not find it there (X64 put the slider back in Settings › View beside the other
   two strengths) and has now decided the split: the ribbon carries the per-view switch alone,
   the workspace value is edited in Settings only.
  Shape: Remove the ribbon's value machinery: spec's VALUES, GroupValue, ValueSpec, ValueKind,
   value_of and VALUE_COLUMNS, toolbars' show_value, value_current, value_action, the
   shadow_strength field of RibbonView and RibbonContext, ToolAction::ShadowStrength and
   RibbonViewAction::ShadowStrength, scrub's width and COMPACT_FIELD_WIDTH, and the RibbonAsk
   field Document::show_ribbon returns for it — which goes back to reporting the product card.
   The shadow cell, its ToolKind, its key S, its inventory row and its artwork stay; the
   settings slider is the only editor of the intensity, which is what the value's own row in
   ViewportSettings already documents. A theme or strength repaint still goes through
   Display::repaints (X64), so nothing about the frame changes.
  Done: The View tab's Display group holds the Shadows switch and no number; the arrow keys and
   the field's drags no longer exist in the ribbon; Settings › View still edits the intensity
   and the reset restores it; a shot of the View tab shows the group without a field; cargo
   xtask ci is green.
  `files: crates/ui/src/toolbars.rs,crates/ui/src/toolbars/spec.rs,crates/ui/src/scrub.rs,crates/ui/src/editor/ribbon.rs,crates/ui/src/editor/tests.rs,app/src/document.rs,app/src/main.rs,plan/questions.md` — `c85bba4`

- `[x]` **U108** the view display: a compact shadow field, black outlines and a floor that does not hide what stands below it
  Why: Owner, 2026-11-17: the Display group's intensity field is wider than a three-digit
   percent needs; the form outlines are light on the dark theme where the owner wants them
   black; and a body below the floor is hidden by the plate, which he asks to see through while
   the shadow keeps working, or else as a floor-opacity setting.
  Shape: Scrub::width with COMPACT_FIELD_WIDTH, one cell wide, for a value whose range is a
   whole percent, and VALUE_COLUMNS = 1. The edge ink becomes one black constant for both
   variants: the edge buffer is built once and shared by every style (ADR 0070), so the ink
   cannot follow the style, and the wireframe then draws black lines on the dark background —
   plan/debt.md records it. The floor plate stops writing depth: it is drawn first and carries
   the shadow in its own fragment, so it keeps darkening the floor while a body below it draws
   over the plate instead of being culled behind it.
  Done: The Display group field is one cell wide and shows 100 %; a body below the floor shows
   through the plate and the plate still draws its shadow; the dark theme draws black form
   outlines; a shot pair shows the floor case; cargo xtask ci is green.
  `files: crates/ui/src/scrub.rs,crates/ui/src/toolbars.rs,crates/ui/src/toolbars/spec.rs,crates/render/src/geometry.rs,crates/render/src/pipelines.rs,crates/render/src/lib.rs,app/src/ui_shots.rs,plan/debt.md` — `8fe2eb5`

- `[x]` **X64** the grid and outline display strengths reach nothing
  Why: Owner, 2026-11-17: «не работает настройка силы сетки и контура». Measured:
   `ViewportSettings::grid_strength` and `edge_strength` are read by nothing but the Settings
   window — no crate outside `crates/ui/src/settings.rs` names either constant — so U64 shipped
   two sliders that paint no pixel. The shadow's slider was the only one ever wired, and only
   because U44/X62 routed it through the frame.
  Shape: One conversion (`ui::display::display_strength`) from the whole percent to 0..1, and
   the frame carries the three numbers as data the renderer reads when it builds the lines:
   `Grid::ink_strength` scales the grid inks' alpha in `grid_vertices`,
   `SceneFrame::edge_strength` is the outline ink's alpha in `edge_color`, and the shadow keeps
   its per-view uniform (U107). The line pipeline gains `ALPHA_BLENDING` — without it an alpha
   is ignored — which leaves every opaque line (the selection, a path, an envelope) unchanged.
   `Display::repaints` decides the re-upload: the three strengths live in the ink the buffers
   are built from, so a change repaints the frame and re-evaluates no node, while the shadow's
   intensity stays a uniform and costs nothing.
  Done: A grid at 0 % hides and at 100 % draws the theme's ink; an outline at 0 % leaves the
   fill alone; a theme switch keeps all three strengths;
   `target/ui-shots/display-strength-{full,none}.png` show the three at their strengths and at
   zero; `cargo xtask ci` is green.
  `files: crates/scene/src/lib.rs,crates/render/src/lib.rs,crates/render/src/geometry.rs,crates/render/src/pipelines.rs,app/src/document.rs,crates/ui/src/settings.rs` — `2648f39`

- `[x]` **U107** a shadow is an attribute of the view, with one intensity control beside it
  Why: The shadow is controlled in two places today: a checkbox in the viewport cluster's
   display-settings popover and a strength slider in Settings › View, and the frame takes the
   result globally, so two split views cannot disagree. The owner asks for the shadow to be a
   view mode with its own intensity control, 2026-11-17.
  Shape: A view owns its shading style; it now owns its shadow too. `Leaf` gains the switch
   beside the style, `ViewSpec` carries the resolved `ContactShadows`, and the frame keeps
   colour, strength and the mask as workspace data, so one split cell draws a shadow while its
   neighbour does not. One control site: the View tab's display group holds a *Shadows* toggle
   with its key and, beside it, the intensity slider that leaves Settings › View, reading and
   writing the active view; the popover checkbox goes, because two controls for one value
   disagree eventually. A sixth `ShadingStyle` is rejected: a shadow combines with every face
   style, so the list would have to grow to ten. The switch, the key, the icon, the label in
   both languages and the inventory row ship in one change (the place-capability procedure).
  Done: The View tab row toggles the active split view and leaves its neighbour drawing what it
   drew; the intensity reaches the frame without an upload; the removed popover control leaves
   no orphan message, key or test; the icon is authored under `assets/icons`, has its README
   row and its empty-file fallback test; `cargo xtask ui-rules`, `cargo xtask skills` and
   `cargo xtask ci` are green.
  `files: crates/scene/src/lib.rs,crates/render/src/lib.rs,crates/ui/src/display.rs,crates/ui/src/toolbars.rs,crates/ui/src/toolbars/spec.rs,crates/ui/src/toolbars/ready.rs,crates/ui/src/inventory.rs,crates/ui/src/settings.rs,crates/text/src/messages.rs,crates/ui/src/icons.rs,app/src/view_layout.rs,app/src/view_cluster.rs,app/src/document.rs,app/src/main.rs,app/src/shortcuts.rs,app/src/ui_shots.rs,assets/icons,assets/README.md` — `85f1bfe`

- `[x]` **X62** a turned round body loses its contact shadow or draws one of the box of its box
  Why: Measured 2026-11-17: `target/ui-shots/probe-shadow-on.png` and `probe-shadow-off.png`
   are identical pixel for pixel, so the pass that shipped draws nothing a person can see.
   Three independent reasons. (1) The ellipse is hidden by construction:
   `SHADOW_FOOTPRINT_SCALE` keeps it inside the body's own footprint while the body stands on
   the same plane, so the body covers all of it; the only pixels it ever owns are the ones seen
   through the sub-millimetre gap `FLOOR_TOLERANCE_MM` allows, which is why lifting a body by 1
   mm removes it and lifting it by 0.4 mm makes it appear. (2) It exists only for a body that
   rests on the floor, and almost nothing in furniture does. (3) Black over a near-black
   background is no shadow: at 1.6x the footprint and 100 % strength it is still invisible on
   the dark theme, and only the light theme reads (`probe-light-on.png`); the same measurement
   at the shipped 0.92 and 40 % shows nothing in either theme. The shape is the box of the
   turned box, which is the defect the entry was filed for. Owner's decision, 2026-11-17: the
   shadow is replaced rather than repaired.
  Shape: Delete the ellipse and everything that derives it: `ContactShadow`,
   `ContactShadow::from_bounds`, `SHADOW_FOOTPRINT_SCALE`, `FLOOR_TOLERANCE_MM`,
   `SHADOW_GROUND_Z_MM`, `shadow_geometry`, `SHADOW_SEGMENTS`, `MAX_CONTACT_SHADOWS`, the
   shadow pipeline and its depth state, the shadow-centre fixups in `SceneFrame`'s move and
   turn, and `SubtreeBounds::solid` with them — a shadow that is no longer derived from a bound
   is what closes this defect. In its place, three parts. (1) A floor plate: one unlit quad on
   `z = 0` across the grid's extent, single-sided so a view from below does not lose the model
   behind it, its colour a `SceneColors::floor` per variant (the dark one a few steps above the
   background, because a shadow has to darken something). (2) The mask: the frame's drawables
   projected along the key light onto that plane into one 2048^2 single-channel texture — a
   coverage render with no depth compare, so it has no bias and no acne — blurred by two
   separable passes to a 30–40 mm penumbra and sampled by the plate's fragment shader as `1 -
   strength * mask`. (3) The rebuild: once per scene upload and once per key-light turn, never
   per frame; the camera moving costs nothing. `ContactShadows` (switch, strength, colour)
   stays as the frame's settings and keeps meaning what it says,
  Done: A box standing on the floor casts a mask whose offset matches the key light's elevation
   (a test on the projection, no GPU), so the shadow is never under the body; a body 1 mm above
   the floor and a body 300 mm up both cast one; a turned 50 mm ball casts a disc and not a
   box; orbitting the camera and moving the pointer rebuild nothing while an edit rebuilds the
   mask once (a probe that counts rebuilds); the shadow reads in both themes, shown by a
   picture in `target/ui-shots`; and `cargo xtask ci` is green.
  `files: crates/scene/src/lib.rs,crates/render/src/lib.rs,crates/render/src/geometry.rs,crates/render/src/pipelines.rs,crates/render/src/scene.wgsl,crates/render/src/gpu.rs,crates/render/src/cache.rs,app/src/scene_build.rs,app/src/document.rs` — `52874fd`

- `[x]` **X61** a fine sphere loses its meridians, so its poles read bald
  Why: Observation: on a 50 mm ball, raising the side count removes longitudinal lines instead
   of adding them, and the poles lose every meridian, so the cap draws as bare concentric rings
   and reads bald. Measured on the mesh, 16 bands, counting display edges by direction: 32
   sides gives 480 ring + 448 meridian edges, 64 gives 960 + 760, 128 gives 1920 + 512 and the
   survivors sit only in the equatorial band (|z| <= 14.4 of 50 mm), while 160, 200 and 256
   sides give 2400, 3000 and 3840 ring edges and no meridian at all. Expectation: the edge pass
   draws the mesh it has, so a finer ball shows more meridians, never fewer, and the pole keeps
   them up to the last band. Suspected cause, measured: mesh_edges in crates/mesh/src/lib.rs
   keeps an edge only when the two faces across it turn by more than COPLANAR_COS, and that
   constant is 0.999, which is acos 2.56 degrees. The margin is read from face normals, not
   from the ideal geometry, and the normal of a latitude quad tilts with latitude: near the
   pole the band is short and the quad is a long radial strip, so its normal is tilted and the
   fold across a meridian edge falls under 2.56 degrees there first; the same ball's ring fold
   is 180 / bands = 11.25 degrees at 16 bands, which is why only the meridians go. The margin
   hides the longitudinal edges of any finely tessellated smooth body, not only a sphere's.
   Measured while choosing a new value: 0.999999 (0.08 degrees) keeps every meridian of a 256 x
   16 ball and still drops a quad's diagonal and a flat cap's subdivision, while 0.99999999
   starts drawing the quads' diagonals as hatching.
  Shape: One margin, lowered to what coplanar means for micrometre-rounded coordinates: about 1
   - 1e-6, 0.08 degrees, instead of 0.999. The meaning of the test stays: a quad's diagonal and
   every inner edge of a subdivided flat cap are dropped, a cube's ninety-degree fold and a
   fine sphere's meridians are drawn. Say in the constant's doc comment that the margin guards
   rounding noise and is not a display radius, and that a viewer-facing smooth ball keeps its
   graticule. What is not in scope: no change to the triangulation, the pole fan or the
   sphere's parameters, and no change to the 4 M line-vertex budget, which X60 owns; a 256 x 16
   ball does grow from 3840 to 7936 display edges, so re-measure the [perf] entries in
   plan/debt.md with X60 rather than here.
  Done: A 50 mm 256 x 16 sphere's display edges hold a meridian at every band and a ring at
   every latitude, and the default 32 x 16 wireframe is unchanged; a flat quad still shows its
   four boundary edges and no diagonal, and a flat cap split into many triangles shows no inner
   edge; the constant's doc says the margin covers rounding noise, not a display radius; cargo
   xtask ci is green.
  `files: crates/mesh/src/lib.rs` — `be5f4a7`

- `[x]` **X59** a click ray-casts every triangle of every object in the scene
  Why: Observation: a click costs every triangle in the scene. ray_hit_drawable
   (crates/scene/src/ray.rs) loops over the whole index buffer of the drawable and transforms
   every vertex it touches, and pick_scene (app/src/mesh_pick.rs) calls it for every drawable
   of the frame, so the work is O(total triangles) per click with no early reject. Measured in
   the headless probe: 16 / 64 / 161 ms per pick at 500 / 2 000 / 5 000 spheres (0.48 M / 1.9 M
   / 4.8 M triangles); 5 000 boxes pick in 0.2 / 0.8 / 2.2 ms because they are cheap. A click
   that takes 160 ms reads as a frozen window, and the press that opens the rubber band tests
   every object as well (pick_surface and the band's start). Expectation: a click costs a small
   multiple of the objects, not of their triangles.
  Shape: Reject first: give every drawable its world AABB (the value X56 and X58 cache) and
   intersect the ray with it before touching vertices; the box test is eight comparisons and
   drops almost every object of a dense scene at the first step. Then, for the drawables that
   survive, keep the current per-triangle loop until a measurement says otherwise — a BVH over
   the frame's triangles is the next step and needs a build cost the cache can pay once per
   mesh. The band's start test and pick_surface go through the same narrowing.
  Done: A test builds a dense scene and asserts a pick over empty space touches no triangle (a
   counter is exposed for the test), and that a pick over the model returns the same id as
   before; the measured pick time at 5 000 spheres drops below 20 ms in the probe and the
   [perf] entry in plan/debt.md is re-measured.
  `files: crates/scene/src/ray.rs,app/src/mesh_pick.rs` — `bb3ec52`

- `[x]` **X58** the viewport draws every object in the scene, on screen or not
  Why: Observation: the viewport draws every drawable of the frame, on screen or not.
   draw_order is built as the whole index range (crates/render/src/lib.rs), the draw loop walks
   it, and the edge pass is one flattened list of every mesh edge in the scene; nothing tests a
   drawable against the camera's frustum. A cabinet with a wall of 5 000 objects therefore pays
   for all of them in every frame — triangles, edges and stencil passes — even when the view
   holds one shelf, which is what an orbit or a zoom on a dense scene feels like. Measured in
   the headless probe: 500 / 2 000 / 5 000 spheres are 0.48 M / 1.9 M / 4.8 M triangles and 9.5
   ms of an empty frame at the top count, so the GPU cost is already the dominant term and it
   is spent on invisible geometry.
  Shape: Keep a world AABB per drawable in the frame (the same box X56 caches, so the two tasks
   share one value) and skip a drawable whose box lies outside the camera's frustum before it
   enters draw_order and the edge list. The draw order stays the same for what survives, and
   the stencil slot and the centres are indexed by the same drawable, so the culling is one
   filter over the frame's own data and not a second pass. Frustum planes come from the camera
   the renderer already reads.
  Done: A test with the camera inside a dense scene asserts that only the drawables whose world
   box meets the frustum are drawn and their edges are kept, that a drawable partially behind
   the eye is not dropped, and that framing the whole scene draws every drawable again; the
   count of drawn drawables is exposed for the test.
  `files: crates/render/src/lib.rs,crates/scene/src/lib.rs` — `9167b0d`

- `[x]` **V6** there is no cone primitive
  Why: There is no cone: the Model tab lists cuboid, cylinder and sphere, and
   `assets/icons/primitives/cone.svg` is empty although U46 counted it as declared. The owner
   needs a truncated cone for a leg or a turned blank, and a sharp one when the top radius is
   zero.
  Shape: Add `Primitive::Cone { base radius, top radius, height, segments }` with the same
   validation as the other bodies, its parameter row in the card, and a draw method that takes
   the base circle first (by the circle stages, once V4 lands) and then pulls the height, with
   the top circle as a typed radius on the pull card (zero allowed). `crates/param` builds the
   frustum, and zero top radius builds the apex without a degenerate cap.
  Done: A cone is drawn from a base circle and a height, the card edits both radii and the
   height, a zero top radius builds a sharp cone with no top face, and tests cover the frustum
   volume and the sharp case; the icon is drawn and passes the icon test.
  `files: crates/model/src/primitive.rs,crates/mesh/src/primitive.rs,crates/ui/src/draw.rs,crates/ui/src/editor/drawing.rs,crates/ui/src/object_card.rs,crates/ui/src/toolbars/spec.rs,app/src/extrude_gizmo.rs,assets/icons/primitives/cone.svg` — `00c8795`

- `[x]` **X63** identical created objects all carry the same name in the tree and the card
  Why: Observation: every created object is named after its shape alone (ui/editor/creation.rs:
   primitive.kind().label(), and the fixed words Assembly, Camera, Light), so a scene of five
   identical spheres shows five rows called Sphere in the tree and five identical titles in the
   card and the undo history; nothing on a tree row tells one from another, and a maker who
   wants the third sphere has to guess by eye. Expectation: a created node carries a number
   that no sibling already carries — Sphere, Sphere 2, Sphere 3 — so every row of the tree
   names one object, and a rename or a delete leaves the rest as they were.
  Shape: One document query, Project::unique_child_name(parent, base), in model: the first free
   base, base 2, base 3 among the parent's children, decided on read, so nothing rewrites a
   stored name and no command learns a second naming rule. The creation paths in
   ui/editor/creation.rs ask it for the default name they already build; a name the user types
   stays theirs. The tree draws the stored names, so the tree, the card, the status bar and the
   undo history read one string.
  Done: A model test creates three siblings through unique_child_name and asserts Sphere,
   Sphere 2, Sphere 3, that a free number is reused after a sibling is renamed away, and that
   two different parents each start at Sphere; an editor test asserts the third created
   sphere's stored name ends in 3.
  `files: crates/model/src/project/query.rs,crates/ui/src/editor/creation.rs,crates/ui/src/scene_tree.rs` — `2cf4064`

- `[x]` **U106** the projection readout becomes an on-screen display with a frame rate and the scene's cost
  Why: Observation: the corner display under the orientation cube names only pitch, yaw, roll
   and the projection, and the program reports no frame rate, no frame time and no scene cost
   anywhere. A 5 000-object scene therefore reads as jerky with nothing to measure it against:
   the two filed defects (X58 draws every drawable, X59 ray-casts every triangle per click)
   were found with a throwaway probe, not with the editor, and a maker who reports a slow scene
   has no number to send. Expectation: the switch that draws the corner text draws one
   diagnostic block instead: the camera line it has today, the frame rate and frame time, the
   object and triangle count of the frame, and the cost of the last rebuild, upload and pick,
   each measured where the work happens.
  Shape: Rename the switch to Show the on-screen display and make the block its own module
   (app/src/osd.rs) instead of a line owned by gizmo.rs: the cube keeps the cube, the block
   keeps the numbers, and the block no longer disappears when the cube is turned off. The frame
   rate is an exponential average of egui's stable_dt, so one slow frame does not swing it; the
   scene cost is a sum over the uploaded frame's drawables, counted only while the block is on.
   The rebuild, upload and pick durations are recorded by the code that spends them
   (Document::refresh, upload_scene, the pick helper) and read on the next frame. While the
   block is on the window repaints every frame, because a rate measured on an idle window is a
   lie.
  Done: A test drives the block with a fixed frame time and asserts the printed rate; a test
   asserts the block names the frame's objects and triangles for two drawables; a test asserts
   the block is drawn only while its switch is on; the camera line keeps its existing test.
  `files: app/src,crates/text/src/messages.rs` — `8a3bb66`

- `[x]` **X60** a dense scene silently loses the wireframe after four million line vertices
  Why: Observation: in the edges styles (shown with edges, or edges through the faces) the 5
   000-sphere stress scene draws the wireframe on the nearest balls only — about 1 400 of 5
   000, fewer than half — and the rest render as solid triangles. Reproduced on 2026-11-17 in
   the live window with target/stress-5000.z3l. Expectation: the style draws the edges of every
   drawable in the frame, or the frame is refused with a sentence; it never silently stops
   after a fixed budget. Suspected cause, measured: edge_geometry in
   crates/render/src/geometry.rs flattens every drawable's edges into one line list and breaks
   when the list reaches MAX_VERTICES = 4 000 000, after which every later drawable's slice has
   vertex_count 0 and the pass draws nothing for it. A default 32 x 16 sphere has 1 440 edges
   (480 ring, 512 spoke, 448 triangle diagonal) = 2 880 line vertices, so the budget covers 1
   388 spheres — 27.8 % of 5 000 — and the same cap truncates the polylines and the product
   envelopes of a frame with as many items. The face pass is per drawable and is not affected;
   only the flattened line buffers are.
  Shape: Keep the allocation guard, drop the silence. Until X57 gives every mesh its own edge
   buffer, split the frame's line list into as many buffers as the budget needs and draw them
   all: a LineList owns a small vector of buffers and each slice names buffer plus range, so 4
   M stays a per-buffer limit instead of a per-frame cut-off. A frame that truly exceeds what
   the device can hold is refused with a readable sentence instead of being drawn in part. When
   X57 lands, an edge buffer belongs to the mesh and the truncation disappears with the shared
   list.
  Done: The stress scene at 5 000 spheres shows the wireframe on every ball in both edges
   styles, with the same stencil rule per drawable as before; a test builds a line list of more
   than 4 M vertices and asserts that every slice keeps its vertices and that the number of
   buffers grew instead of the vertex count being cut, and that an upload past what the device
   accepts is the readable error; the [perf] entries in plan/debt.md are re-measured.
  `files: crates/render/src/geometry.rs,crates/render/src/lib.rs` — `62d2bab`

- `[x]` **X57** an edit re-uploads every GPU buffer and the whole edge list
  Why: Observation: an edit re-uploads the whole scene. ViewportRenderer::upload
   (crates/render/src/lib.rs) creates a vertex and an index buffer for every drawable of the
   frame on each rebuild, and edge_geometry (crates/render/src/geometry.rs) rebuilds one
   flattened line list over every mesh edge; the grid, the polylines, the product envelopes and
   the contact shadows are recreated too. A rebuild happens on every revision move, so changing
   one panel's thickness in a cabinet of 5 000 objects recreates 5 000 buffers and re-uploads
   millions of triangles and their edges. Measured CPU per edit is 27 / 109 / 285 ms at 500 / 2
   000 / 5 000 spheres, and the GPU side is not measured at all today because the headless
   probe has no adapter. Expectation: a rebuild uploads only the drawables whose mesh handle or
   transform changed, and the edge list grows and shrinks by the same difference.
  Shape: Key the GPU buffers by what they were built from — the mesh Arc pointer plus the world
   transform (or by node id and the transform pair) — so an unchanged drawable keeps its
   buffers and only the changed ones are recreated; keep one buffer per unique mesh and a
   per-drawable transform where the transform can stop being baked into the vertices. Build the
   edge list per mesh and concatenate the slices for the frame instead of regenerating line
   vertices for every edge on every upload. Free the buffers of drawables that left the frame.
   Full detail lives in the two [perf] entries of plan/debt.md.
  Done: A test with two scenes that differ in one drawable asserts that the upload creates one
   new vertex and one new index buffer, and that the untouched drawables keep the same wgpu
   buffer handles; the edge list of an unchanged drawable is not rebuilt; the [perf] entry in
   plan/debt.md is re-measured in the live window.
  `files: crates/render/src/lib.rs,crates/render/src/geometry.rs,app/src/document.rs` — `35a6fb7`

- `[x]` **X56** an edit rebuilds the world box of every mesh, vertex by vertex
  Why: Observation: one edit anywhere costs the whole scene. Measured in a headless probe
   (app/src/frame_tests.rs, dev profile with opt-level 2, 5 000 root spheres of 32 x 16): 27 /
   109 / 285 ms of CPU on every revision move at 500 / 2 000 / 5 000 objects, and the numbers
   are identical whether zero or one mesh was rebuilt (evaluate_warm equals one_changed in the
   probe recorded in plan/debt.md). Expectation: an edit costs its changed nodes plus one walk
   of the tree, not the vertex count of the whole scene. Suspected cause, measured:
   Bounds::include_mesh in app/src/scene_build.rs transforms every vertex of every mesh to find
   its world box, and collect_panel and collect_object call it twice — once into the subtree's
   all box and once into solid; a 32 x 16 sphere is 896 vertices, so 5 000 of them are 9 M
   transforms per rebuild, while 5 000 boxes of 8 vertices cost only 24 ms, which is the walk
   itself (about 5 us per node).
  Shape: Compute a local AABB once per mesh (geom::MeshData or the mesh cache entry) and turn
   its eight corners into the world box in one pass per drawable, feeding all and solid from
   the same box, so a rebuild stops looking at vertices. If the eight-corner box is too loose
   for a rotated object — it is the box of the rotated box — keep the tight world box on the
   cache entry keyed by (mesh, placement) and copy it when the node did not move. The container
   level bounds still come from the children, and the frame-all bounds stay conservative by the
   same rule.
  Done: A headless test builds 5 000 spheres, moves one and asserts the rebuild transforms no
   vertex and stays under 50 ms on this machine; the bounds still contain every drawable and
   match the per-vertex bounds at the default scene; the [perf] entry in plan/debt.md is
   re-measured with the new numbers.
  `files: app/src/scene_build.rs,crates/geom/src/lib.rs,crates/mesh/src/lib.rs` — `808ba22`

- `[x]` **X55** a sphere reads sharp at its poles, as if the radial segments were missing there
  Why: Observation: at the default 32 x 16 the ball reads sharp at both poles — the cap is a
   fan of 32 long thin triangles and the edge pass draws the fan as a star, so the sphere looks
   like it lacks radial segments at the ends. Expectation: at the default tessellation the ball
   reads round at the poles, with a cap no wider than the band beside it, while the stored side
   and band counts keep promising the same faces and vertices. Suspected cause, measured:
   sphere_mesh in crates/mesh/src/primitive.rs spaces the latitude bands uniformly in z (z = -r
   + 2 r band / bands), so with 16 bands the first ring has the radius sqrt(r^2 - (r -
   2r/16)^2) = 0.484 r, and the pole is joined to that ring by one triangle per side; the
   ShadedWithEdges pass paints those long edges verbatim. The counts themselves are right:
   face_count is segments x bands and vertex_count is 2 + segments x (bands - 1).
  Shape: Spacing only: distribute the bands by latitude angle instead of by z, z = r cos(pi
   band / bands) and ring = r sin(pi band / bands), so every band spans one meridian step and
   the pole cap is the first band (0.195 r at 16 bands). Keep the stored segments and bands and
   the counts they promise, so the card, the schema and the mesh_counts test are untouched, and
   the phantom (drawn from the radius alone) needs nothing.
  Done: The default 32 x 16 sphere has its first ring r sin(pi/16) from the pole, every band
   covers one equal meridian step within rounding, and the counts, bounds and outward normals
   are unchanged; a mesh test asserts the pole ring's radius and that no band spans more than
   twice another, and the ball is looked at once in the live window at the default size.
  `files: crates/mesh/src/primitive.rs` — `06ae440`

- `[x]` **K5** the program cannot open a scene named on its command line
  Why: Observation: `z3l cabinet.z3l` prints «`cabinet.z3l` is not a command; run `z3l --help`»
   and the window opens only with no argument at all, so a double click on a scene in a file
   manager, a script or a debug session that wants one saved scene needs a manual Open through
   Home first — a 5 000-sphere stress scene cannot be handed to the editor at all. Expectation:
   one scene path as the argument opens that scene in a tab, exactly as the File menu's Open
   does, and a path that cannot be read leaves the window open on the readable sentence instead
   of exiting. Suspected cause: cli::parse recognises no argument, --help and export only, and
   main passes nothing to run_window.
  Shape: One new command: Command::Scene(PathBuf) for a first word that ends in .z3l. Whether
   the file exists is the opener's business, so a typo reaches the window's error line instead
   of an exit; a second path or a word that is not a .z3l stays a usage error, and USAGE names
   the form. main::run_window takes the path and calls the existing App::open_tab after
   App::new, reporting a failure through App::failed so the window stays open; nothing else
   about opening changes, because the tab, the recent list and the catalogue are the same path
   the menu uses.
  Done: z3l <scene.z3l> opens that file as the active tab; a missing or malformed scene keeps
   the window open and shows the sentence in the status bar; z3l, z3l --help and z3l export are
   unchanged; parse tests cover one path, one missing .z3l, a second path and a bare word.
  `files: app/src/cli.rs,app/src/main.rs` — `adec943`

- `[x]` **V5** a sphere has one click and one method
  Why: A sphere is put down in one click and sized in the card: there is no first point, no
   phantom and no second gesture, and it has one method. The owner asked for a step-by-step
   build (centre, then drag the radius with a phantom of the sphere) and for the three methods
   a solid modeller offers: centre and diameter, and three points that may be picked as
   tangencies on a face plane.
  Shape: Make the sphere a drawn primitive like the others: stage one places the centre, stage
   two drags the radius with a translucent phantom sphere and the R/Ø plate; the card keeps the
   segment counts. Add the diameter method (the plate reads Ø, the sphere the same) and the
   three-point method, which circumscribes the sphere through three points — each point may
   come from a face-plane tangency snap, so the snap result is a point, not a new pick mode.
   Methods live in `DrawTool` and the ribbon map like the circle methods.
  Done: A sphere is built centre-then-radius with a phantom, by centre and diameter, and by
   three points including tangency picks; each method is a stage table entry with its plates,
   and a test asserts the radius each method produces against a known three-point set.
  `files: crates/model/src/primitive.rs,crates/param/src/lib.rs,crates/ui/src/draw.rs,app/src/document.rs` — `949cf7e`

- `[x]` **V4** a box and a cylinder are drawn by their own gesture, not by the contour grammar
  Why: A box and a cylinder are drawn by their own box gesture, with their own rules, so a
   primitive does not know the sketch grammar: a rectangle cannot be built from its centre, its
   edge or its diagonals, and a circle cannot be built by a diameter or by three points
   although both figures exist for a flat contour. The owner asked for one grammar: the profile
   is drawn as a flat contour with every method the sketch offers, then the body is built on Z.
  Shape: Compose the primitive out of the two things that already exist. The base is an
   ordinary `DrawDraft` on the work plane — the rectangle tools for a box, the circle tools for
   a cylinder — committed to a profile (a `Path` in the local plane); the height is the pull
   the extrusion already reads, with the same H field. The primitive stores its profile and
   height instead of three sides (schema bump under ADR 0096), `crates/param` builds the same
   solid from the profile, and the primitive parameter card edits the profile through the
   contour editor. The box gesture stays for a product envelope, which is not a primitive.
  Done: A box is drawn as a rectangle by any of the four rectangle methods and a cylinder as a
   circle by any of the three circle methods, each with its own plates; the body is built on Z
   from the profile; the card edits the profile; a test builds each method and asserts the
   resulting solid matches the old parameter shape.
  `files: crates/model/src/primitive.rs,crates/ui/src/draw.rs,app/src/document.rs,crates/ui/src/toolbars/spec.rs` — `6768efb`

- `[x]` **U104** the extrusion card offers create and cancel beside its height
  Why: The floating card of an extrusion offers a create tick and a cancel cross beside its
   height. The owner wants the card to carry only the height — no units, no words, just H —
   because the gesture already has the Enter and the Escape, and the card is a number, not a
   dialog.
  Shape: Drop the two buttons from the extrusion card and keep the one height field, labelled
   `H` and drawn without a unit (the plate rule of the sketch fields). Enter commits and Escape
   cancels exactly as they do now, and the card keeps its place beside the pointer. The field
   becomes the only control that takes the keyboard.
  Done: The card draws one H field and no tick or cross; a frame test asserts the card holds
   exactly one field and that Enter commits while Escape cancels.
  `files: app/src/extrude_gizmo.rs` — `29a3d47`

- `[x]` **U103** there is no rubber-band selection
  Why: There is no way to select several objects with the pointer: a press either picks one
   node or orbits. The owner asked for the AutoCAD rectangle — left-to-right and top-to-bottom
   takes what it encloses, right-to-left and bottom-to-top takes everything it touches.
  Shape: Add a selection drag to the viewport: a press on empty space draws the rubber band,
   the direction of the drag decides the rule (screen-space corner order, not the world), and
   the release applies it through the selection API the Shift/Ctrl work introduces, so a band
   can also add. The band is chrome paint under the work cursor, and a press with no travel
   stays the plain click that orbits nothing. Picking must test the projected silhouette of
   every scene object, not the mesh.
  Done: A band dragged left-to-right selects only the objects it encloses, one dragged the
   other way selects every object it touches, the rule follows the drag direction in a test
   with objects inside and straddling the band, and the band draws only while the pointer drags
   on empty space.
  `files: app/src/document.rs,crates/ui/src/editor/selection.rs` — `f251903`

- `[x]` **U102** a selection cannot be added to or removed from
  Why: A selection is one object: a click replaces it, and there is no way to add or remove
   one. The owner asked for the CAD habit — Shift adds, Ctrl removes — and for the order of
   selection to be remembered, because a loft through several sections needs to know which
   profile came first.
  Shape: Add a selection order and the two modifiers in the one place a viewport click and a
   tree click both reach (`select_viewport_hit`, `Selection`). Shift on a click adds the picked
   id at the end of the order (or removes it on a second click, the toggle a person expects);
   Ctrl removes it and only it; a plain click replaces. Keep the order as view state in the
   session, not in the document, and expose it read-only for the future loft. The tree and the
   viewport must agree.
  Done: Shift-click builds an ordered set, Ctrl-click removes one member, a plain click
   replaces, and a test asserts the order survives a toggle of a member and is cleared by the
   replacement; the scene draws the added members as selected.
  `files: crates/ui/src/editor/selection.rs,app/src/document.rs,crates/engine/src/session.rs` — `803e74a`

- `[x]` **X54** a click on a widget over the scene commits the running draft's next point
  Why: Observation: with a line tool armed and waiting for its next point — the segment after a
   straight line's start, a point of an arc — a click meant for something other than the scene
   fixes the plate: the length is set and the line commits at it, the arc takes its second
   point there. The owner names the surfaces: the axes widget in the view's top-right corner,
   and any of the two panels. Reproduced on 2026-11-16 in a headless frame test: a line with
   its start placed, the pointer 200 × 37 points right of it, a press on the left panel, then a
   hover 400 points further on — the length plate hands over the rounded text of its live value
   (772.3 for a live 772.3341245859851) when it loses the keyboard, locks `Length = 772.3`, and
   the length stays there wherever the pointer goes. Expectation: a press outside the plates
   takes nothing from them — the point being placed keeps following the pointer, and only a
   number the person typed, or the Enter key, becomes a promise. Suspected cause, measured:
   `Scrub` commits its text whenever the field loses the focus (`crates/ui/src/scrub.rs`, the
   `enter || lost || due` branch, `commit = parsed != old`), and the text is not the value but
   `number_text(old, decimals)` — one decimal for every parameter but `Sides`
   (`FieldParam::decimals`) — so a live value almost never round-trips and a blur is a commit;
   `FieldSet::show` turns every non-zero commit into a lock, and the plate takes the keyboard
   by itself because `FieldSet::new` requests it. The press does reach the widget it was aimed
   at: the trace of a real run shows the axes widget swallowing the click (`on_chrome=false`,
   `clicked_p=false`) while the plate behind it hands its number over.
  `files: crates/ui/src/scrub.rs,crates/ui/src/point_fields.rs,app/src/draw_gizmo.rs,app/src/frame_tests.rs` — `4e609a7`

- `[x]` **X53** the line list has a by-angle row that draws nothing
  Why: The line list offers «by angle» with no glyph and no action: the row is declared as a
   planned method (`icon: None, run: None`), so it draws blank and runs nothing. The owner
   tried it and it did nothing.
  Shape: Make it a real method: a start point, then an angle and a length typed in the plates
   (the same length/angle pair a segment already reads), committed as one open `Path` of a
   single line. The angle must be read with the rule ADR 0134 settles, and the stage declares
   its two numbers so `every_shipped_tool_declares_its_numbers` covers it. Add
   `assets/icons/contour/line-angle.svg` (a line at an angle to a dashed datum, draw ink plus
   the reference ink for the datum) and its row in `assets/README.md`.
  Done: The row draws a glyph, runs, places a line of the typed angle and length from the
   clicked start, and a test asserts the committed segment; the icon passes the footprint and
   coverage test.
  `files: crates/ui/src/toolbars/spec.rs,crates/ui/src/draw.rs,crates/ui/src/icons.rs,assets/icons/contour` — `c7cd17d`

- `[x]` **X52** the circle list has a tangent-to-two-lines row that draws nothing
  Why: The circle list offers «tangent to two lines» with no glyph and no action: the entry is
   declared as a planned method (`icon: None, run: None`), so it draws a blank row that runs
   nothing. The owner tried it and it did nothing.
  Shape: Build it as a real method: a `DrawTool::CircleTangent` that takes two picked line
   targets and a radius (or takes three clicks — two on the lines and a point), computes the
   centre in the wedge the two lines make, and commits one circle through the same
   `object.path.create` path as the other circle methods. Add
   `assets/icons/contour/circle-tangent.svg` to the guide (two lines tangent to a circle, draw
   ink) and its row in `assets/README.md`; declare the tool in `spec.rs` with its fields in
   `draw_fields.rs` and its stage table test.
  Done: The method draws its glyph, appears enabled in the circle list, builds one tangent
   circle from two picked lines, and a test places it against a known pair of lines; the icon
   passes the footprint and coverage test.
  `files: crates/ui/src/toolbars/spec.rs,crates/ui/src/draw.rs,crates/ui/src/icons.rs,assets/icons/contour` — `0883152`

- `[x]` **U105** the header carries the product wordmark, drawn from one asset
  Why: The header draws the name as a text label beside an 18-point placeholder mark whose
   drawing is a generic board with two shelves. A text label is not a mark: it renders in
   whatever face the platform picks, it cannot be sized or aligned to the row, and it repeats
   in a system font what the product's own drawing says better. The placeholder also drew in
   the accent, and the interface reserves that ink for action and selection.
  Shape: The asset becomes the Z3L wordmark the owner kept — one ink, viewBox 0 0 42 16 — and
   icons gains LOGO_ASPECT plus wordmark(ui, height, ink), which allocates the aspect's width
   and rasterises through the existing asset cache. The header draws it in the text ink at the
   row's height and keeps the name only as the hover and accessibility label; icons::mark goes,
   because a square slot for a wide mark can only letterbox it. assets/README.md gains a
   section saying this file is a mark and not an icon, and plan/map.md names it beside the
   header row.
  Done: The header draws the wordmark at the row's height in the text ink, and a test asserts
   that a textured rect of the wordmark's aspect is drawn there; the name is still reachable as
   a hover and accessibility label and the window title still reads PRODUCT_NAME; the asset is
   one ink and the loader tints it; assets/README.md, plan/map.md and ADR 0136 record the rule;
   cargo xtask ci is green.
  `files: crates/ui/src/icons.rs, app/src/header.rs, assets/logo.svg, assets/README.md, plan/map.md, docs/decisions/0136-the-wordmark-is-drawn-in-the-text-ink.md` — `8543b94`

- `[x]` **P16** the gate rejects a crate whose name equals a module it depends on
  Why: The crates carry bare names (ADR 0135), so a name can collide with a module: a crate
   that declares `mod scene;` and depends on the crate `scene` loses the crate, and `scene::…`
   resolves to the module — `E0425` at a line that does not name the cause (`plan/rename.md`
   §6). Layering alone does not prevent it, because a crate may legitimately name a module
   after a crate below it (`render` + `mod scene;`, `ui` + `mod model;`), and a prefix would
   only move the collision to a less likely name. Measured: with the module in place the check
   reports it, and it is clear on this tree.
  Shape: `cargo xtask layers` reads every workspace package from `cargo metadata`, parses its
   crate root (`src/lib.rs`, `src/main.rs`) with `syn`, and fails when a declared module has
   the same name as one of the package's non-build dependencies, naming both and the two ways
   out: rename the module, or write `::name::…`. A pure test covers the match and a
   comment-only `mod` line, and this tree must stay green.
  Done: `cargo xtask layers` fails on a crate that declares `mod <dependency>;` and passes on
   this tree; a test proves a comment-only `// mod fake;` is not a module; AGENTS.md §3 and the
   architecture skill state the rule; `cargo xtask ci` is green.
  `files: xtask/src/main.rs, AGENTS.md, .pi/skills/enforce-architecture/SKILL.md, .opencode/skills/enforce-architecture/SKILL.md` — `a895cd0`

- `[x]` **R1** draft: the pre-release rename gives the product name one home before anything is released
  Why: This task was not in the queue and no session started it: it ran when the owner
   commanded the release, and that command also named the product, which was the one input the
   entry could not supply. Until then it kept its place at the end of `## Next`, so the
   measured procedure in `plan/rename.md` did not get lost. The product name was hardcoded in
   the window title, the app name, the error text, the `.z3l` default and its filter, the
   settings directory and the path hints, so renaming after a release would have been a
   refactor instead of a constant.
  Shape: (1) Give the identity one home — `theme::PRODUCT_NAME` for the words, an `APP_ID` for
   the window class and the settings directory set through `ViewportBuilder::with_app_id`,
   `project::FORMAT` + `project::EXTENSION` for the file — and delete the hardcoded duplicates
   in `main.rs` (title, app name, error), `file_menu.rs` (the default file name and the filter)
   and `settings_store.rs` (three platform branches). (2) Rename the fifteen crates to bare
   names and move `apps/furnicraft` to `app/` at the workspace root, with the `sed` and `mv` in
   `plan/rename.md` and `cargo fmt --all` after them. (3) The four registers that must agree in
   that commit — the `CRATES` table and its tests in `xtask`, the crate map in `AGENTS.md` and
   `PROJECT.md`, the layer list in the architecture skill, and the path hints in
   `plan/BACKLOG.md`, `plan/map.md` and `plan/debt.md`, which the plan gate rejects once the
   path is gone. (4) The envelope `format` string and the golden's second line, leaving the
   `generator` fixture alone. No compatibility path: nothing is released, so the file format
   and the settings directory change outright.
  Done: A single constant decides the product name, and a test asserts that the window title,
   `app_id`, the settings directory and the open-dialog filter all read it, so a second rename
   cannot miss a copy. Measured on this tree: 2208 tests pass, clippy clean, `layers: ok`,
   `ui-rules: ok`, `plan: ok`, `docs: ok`, `skills: ok`; the mechanical commit touches 338
   files (2835 insertions, 3136 deletions) and changes no behaviour beyond the file extension
   and the settings path. `cargo xtask ci` is green.
  `files: plan/rename.md, crates/ui/src/theme.rs, crates/project/src/lib.rs` — `d4a56f3`

- `[x]` **X51** the arc's second click reads a length it never draws
  Why: While a three-point arc is being drawn, the second click shows a length plate whose
   number is the chord from the start to the through point — a distance the figure does not
   contain, drawn only as the dashed construction line. The owner could not tell what the
   number measured: the arc itself is not drawn yet, so the plate names a length against a
   figure that is not on screen.
  shape: Decide what the second stage of Arc reads and draw the thing it measures. The owner's
   words — «draw L1 at once so it is clear what the distance is» — suggest the chord is the
   measurement and must be drawn as a real draft segment (solid draft ink from the start to the
   placed point) rather than as the dashed construction guide, with the plate on its midpoint,
   so the number and the line are one object. `PointState::construction` already carries the
   pair; the change is in `paint_guides`, `DrawDraft::preview` and the plate anchor. Confirm
   with the owner whether L1 names the first chord of the arc or the first plate of the set
   before drawing.
  Done: The second click of an arc draws the segment its length plate measures, in draft ink,
   and the plate sits on it; a frame test asserts the drawn segment and the plate's value agree
   at several pointer positions. The owner confirms the reading of L1.
  `files: crates/ui/src/draw_fields.rs,app/src/draw_gizmo.rs,crates/ui/src/guides.rs` — `17ae4b5`

- `[x]` **X50** the reference arc changes side every 90 degrees
  Why: The auxiliary arc and its plate flipped to the other end of the X axis at 90 and at 270
   degrees, so a pointer crossing the vertical axis made the guide jump sides while the hand
   was still drawing. The owner asked for the side to change only when the sweep passes 180
   degrees.
  Done: sketch_angle picks the end by the half-plane — +X for 0..=180, −X below — so the pair
   changes side once per turn, and a direction at exactly 180 stays with +X. ADR 0134
   supersedes the end choice of 0128 and states the cost; the plate and reference-arc tests are
   rewritten to the new table.
  `files: crates/ui/src/draw_fields.rs,docs/decisions/0134-a-sketch-angle-changes-side-once-per-turn.md` — `430d59b`

- `[x]` **U101** a dimension is capped at ten metres
  Why: Every dimension stopped at ten metres: MAX_CONTOUR_COORDINATE, the panel side, the
   primitive size and the container envelope were each a 10 m literal, so a field refused what
   a solid modeller accepts. The owner asked for a 1,000,000 mm ceiling, the SolidWorks one.
  Done: The four ceilings are 1000 m in one table of constants, the panel card and the
   primitive card take their field range from those constants instead of a literal, and the
   three tests that measured the old cap measure the new one.
  `files: crates/model/src/contour.rs,crates/model/src/panel/mod.rs,crates/model/src/primitive.rs,crates/model/src/container.rs` — `bdc7545`

- `[x]` **U100** a sketch plate writes its unit and accepts any character
  Why: The sketch plates carried their unit as a caption outside the box, which doubled the
   width of a field whose symbol (L, W, H, R, Ø, ∠) already names the quantity, and the
   editable text took any character and refused it only on commit. The owner asked for a
   minimalist field that takes a number and nothing else.
  Done: A sketch plate and a grip field draw no unit (the cards keep theirs), and the text
   buffer keeps digits, one decimal point and a leading minus, reading a comma as the point. A
   test holds the sanitiser and the coverage is a frame test.
  `files: crates/ui/src/point_fields.rs,crates/ui/src/scrub.rs` — `923b5d7`

- `[x]` **X49** a sketch plate stops under the pointer and the sketch stalls
  Why: A plate could settle under the pointer and stop the hand steering the sketch. Two
   causes: egui's constrain_to moved an area back inside the view at an edge after the layout
   chose its corner, and the stacking rule that keeps two plates apart could push one onto the
   pointer. The layout also did not know where the pointer was.
  Done: The layout fits each plate itself — so the rectangle it reasons about is the one that
   draws — and puts the pointer in the set of rectangles a plate may not cover: a plate the
   pointer reaches steps to the other side of its anchor, and one pushed onto it steps past it.
   The frame test measures the clearance at every stage of every tool and of a pending arc.
  `files: crates/ui/src/point_fields.rs,app/src/frame_tests.rs` — `923b5d7`

- `[x]` **X48** the extrude icon is a cuboid and shows no operation
  Why: The body-extrude glyph was a second cuboid: the same drawing the primitive cell uses,
   with no source, no operation and no result, which is the formula assets/icons/STYLE_GUIDE.md
   §2 asks for and the transition §8.3 describes. The owner asked for the familiar CAD sign — a
   plane with an arrow rising from it — and for the guide rule to be checked.
  Done: The icon is the flat profile lying on the work plane in the draw ink, the upward arrow
   of the operation in the model ink and the extruded face above it, inside the footprint and
   the 10–40 % coverage band. The guide already states OBJECT + ACTION METHOD + RESULT (§2,
   §8.3) and the emphasis ladder (§5.1); the extrude file was the one that broke it, and no
   other icon needed a redraw.
  `files: assets/icons/bodies/extrude.svg` — `2ac0df8`

- `[x]` **X47** a closed product offers envelope grips and can be resized
  Why: Selecting a product that was not the piece being worked in showed the three-length
   envelope grips, and a drag through them wrote product.envelope.set on a closed cabinet. A
   closed product is one object in the scene, not a box being sized (ADR 0098).
  Done: The grip target admits a product frame only while it is the open product and keeps
   admitting any cuboid part; the rule is one function, offers_grips, with its own test.
  `files: app/src/document.rs` — `d335ddd`

- `[x]` **X46** a furniture tool writes «open a product first» instead of its own name
  Why: With the product closed, every furniture cell wrote «Open a product first» in place of
   its own name and method, so the ribbon stopped saying which tool a dim cell is. The owner
   asked for that phrase to go.
  Done: The hint of a furniture cell names the tool and its method whether or not a product is
   open, and only a missing material is still spelled out; the cell stays disabled. The two
   toolbar tests assert the new wording.
  `files: crates/ui/src/toolbars/ready.rs,crates/ui/src/toolbars.rs` — `f39d580`

- `[x]` **X45** an inactive tool carries no tooltip
  Why: A disabled cell showed no tooltip at all. Every icon helper built its widget inside
   add_enabled_ui, and a widget inside a disabled ui is never hovered, so on_hover_text had
   nothing to answer; a greyed tool read as a dead square with no name.
  Done: A helper answers the pointer with on_disabled_hover_text when the control cannot run
   and on_hover_text when it can, so the hint — the button's accessible name too — is always
   there.
  `files: crates/ui/src/icons.rs` — `f39d580`

- `[x]` **X44** a rectangle side midpoint near the base corner collapses the figure
  Why: The owner dragged the midpoint of a rectangle side near the base corner and the figure
   collapsed to the minimum extent; the two sides away from that corner were fine.
   move_rectangle_midpoint read a side's sign from its parity alone, so side 3 (the negative
   first-axis side) and side 2 were treated as if they sat where sides 1 and 0 do: the dragged
   side and the frozen side shared one coordinate and the first pointer move folded the
   rectangle.
  Done: The sign is a table keyed by the side, and every one of the four sides is dragged in a
   test that asserts the frozen side stays and only the dragged extent changes.
  `files: crates/model/src/contour_frames.rs,crates/model/src/contour_edit.rs` — `1c552ca`

- `[x]` **X43** the sketch plates, the reference guide and the switch between tools
  Why: The owner's reclamation against C1-C5. (1) The first point of every sketch tool shows
   only the Y plate: the first stage reads the point itself as X and Y, both rows name the same
   anchor, and plate_corner places each plate at that anchor alone, so the later plate covers
   the earlier one. (2) Two plates overlap whenever their anchors land close: a segment's
   length plate sits on the chord's midpoint and its angle plate on the arc, and at some
   pointer positions the two rectangles intersect (screenshots 09-52-25, 09-55-03). Every plate
   is placed independently. (3) The reference guide is almost invisible: its dot is 0.4 points
   with a 0.22 light centre, and on a dark board the dark rim and the faint centre read as
   nothing (REFERENCE_DOT, REFERENCE_DOT_LIGHT, ADR 0127). (4) The reference axis and its sweep
   arc are drawn in screen space from the work-plane angles: guides::reference_between takes
   from/to in radians and a screen radius and draws a circle in the view, so in a perspective
   or axonometric view the arc is not the ellipse the plane really projects and the axis points
   in the wrong screen direction. Both the sketch gizmo and the contour latch's guide share the
   call.
  Shape: Lay the plates of one point out as one set: resolve every row's rectangle before
   drawing, using the plate size the last frame measured (a conservative estimate on the first
   frame of a stage) and stack a row that collides with an already placed one away from its
   anchor, so X and Y of the first point are two visible plates and no two plates ever cover
   each other. Sample the reference on the work plane — the axis as two points, the sweep as a
   polyline of one sample per degree — project every sample with the camera and hand the
   projected polylines to guides::reference, so the guide lies in the plane in every
   projection; the same change serves the latched corner move. Raise the reference dot and its
   light centre and widen the dot pitch until the line reads on a dark board, keeping the fixed
   two-tone pair of ADR 0121 and recording the new sizes in an ADR. Add frame tests that walk
   every drawing tool and every stage, hover each stage at several viewport positions and
   assert each declared row is drawn, inside the view and clear of the others, and that
   switching tools or to another creation mode mid-draft starts a clean stage.
  Done: No sketch stage hides a plate: the first point of every tool always draws X and Y
   apart, and no two plates intersect at any hover position or viewport edge; the reference
   axis and sweep are projected from the work plane in perspective and axonometric views,
   verified against a screen-space circle; the reference dot reads on a dark board; a table
   test covers every DrawTool and every stage of it, and a switch test covers tool changes
   after zero, one and two points, a pending arc and a non-drawing tool; cargo xtask ci is
   green.
  `files: crates/ui/src/point_fields.rs,crates/ui/src/guides.rs,app/src/draw_gizmo.rs,app/src/contour_gizmo.rs,crates/ui/src/theme.rs` — `1657e1e`

- `[x]` **C5** three ways to build a regular polygon and a rectangle on an angled base edge
  Why: The polygon cell offers one construction (inscribed, centre and rim) and declares a
   second that runs nothing (sketch.create.polygon.edge); the owner needs the circumscribed
   polygon and the polygon by an edge, each a cell of its own with the parameters at the
   cursor, and a rectangle grown from an angled base edge, which the tool set lacks.
   RectangleFromCenterDirected exists but predates the field rules.
  Shape: One draft builds all three: inscribed as today, circumscribed by the apothem (rim =
   apothem / cos(pi/n)), by edge (rim = side / (2 sin(pi/n)), phase shifted by half a step,
   built on the pointer side). The method is not stored: the result is the same regular polygon
   and the angle is a construction parameter, so no schema bump. Fields at the cursor are the
   radius or apothem, the second point angle and the side count. A new
   DrawTool::RectangleTilted takes a corner, a base edge and a height;
   RectangleFromCenterDirected is brought onto the same field rules. Three icons.
  Done: Each of the three polygon methods and the tilted rectangle builds the expected ring
   from typed values, tested against the model own vertices; the side count field is live and
   clamped to the model range; the phantom follows the pointer on the chosen side; three icons
   exist with README rows, the fallback test and inventory rows; no polygon or rectangle cell
   with run: None remains; cargo xtask ci is green.
  `files: crates/ui/src/draw.rs, crates/ui/src/toolbars/spec.rs, assets/icons/contour, crates/text/src/messages.rs` — `4fdda6f`

- `[x]` **C4** a circle by diameter and an arc from its start, end and radius
  Why: The circle cell and the arc cell each declare a method that runs nothing
   (sketch.create.circle.two-points, sketch.create.arc.three-points), and two constructions the
   owner needs are missing: a circle whose field is the diameter, and an arc given by its chord
   and radius, where the two possible centres are chosen by the side the pointer is on. The arc
   through three points already exists and stays the arc main face.
  Shape: DrawTool::CircleDiameter reads a diameter field at the second point;
   DrawTool::ArcByRadius takes a start, an end and a radius, and Path gains one constructor
   that picks the centre on the pointer side and refuses a radius under half the chord. The
   ribbon circle.two-points becomes circle.diameter with its own label and icon,
   arc.three-points is deleted because the main face is that method, and arc.start-end-radius
   is added. Two icons with their README rows, the fallback test and the inventory rows.
  Done: Both cells run and build the expected ring from typed values; the arc takes the centre
   on the side the pointer chose and a radius under half the chord is a readable error the
   draft keeps; a circle diameter field equals twice its radius; the two icons exist with rows
   and the fallback test; no cell with run: None remains for these two tools; cargo xtask ci is
   green.
  `files: crates/model/src/contour.rs, crates/ui/src/draw.rs, crates/ui/src/toolbars/spec.rs, assets/icons/contour, crates/text/src/messages.rs` — `a27016f`

- `[x]` **C3** a contour node moves in a latched mode, with its distance and angle at the cursor
  Why: Moving a corner needs the button held for the whole gesture today, and the owner wants
   the select, latch, drop grammar a native editor has. The old position is the only fixed
   reference of an edit, so the construction line and the reference axis must start there, and
   the delta needs the same field atom C1 builds for creation: length from the old position and
   angle from +X.
  Shape: A click on an unselected node selects it; a click on a selected node latches the move;
   a ghost and the reference axis and arc follow the pointer from the old position; the next
   click drops the node with one command and one undo entry; Escape restores the stored
   position. A press-and-drag on a node keeps the one-gesture move, so the latch adds a path
   rather than replacing one. The fields are the delta: length from the old position and angle
   from +X, 0 returns a parameter to the pointer. Only contour nodes: object placement and the
   grips keep their grammar. One ADR records the latched move.
  Done: select, latch and drop write one command and one revision, tested through RawInput;
   Escape after a latch leaves the document untouched; a drop without motion writes nothing;
   the construction line starts at the old position and the angle arc radius equals the
   distance moved; the fields move the node by the typed distance and angle; a press-and-drag
   still moves a node in one gesture; cargo xtask ci is green.
  `files: app/src/contour_gizmo.rs, crates/ui/src/contour_grips.rs, crates/ui/src/editor/contour.rs, app/src/document.rs` — `06c28a3`

- `[x]` **C2** every sketch tool places its point from numbers at the cursor
  Why: The drawing tools show one text label today (size_label in draw_gizmo.rs) and take only
   the pointer, so a line cannot be drawn to 600 mm at 30 degrees except by eye, and the guide
   that would show the angle does not exist. The grammar is fixed: every field carries the live
   value, one field is always focused, Tab hops, a typed number locks its parameter and
   outranks a snap, typing is never snapped, 0 returns the parameter to the pointer, Enter
   places the point with every field, and the angle is the smaller one from the nearer end of
   the X axis (0..360 typed, at most 180 shown).
  Shape: Per tool: the first point takes X and Y; a line, an arc chord and a polygon edge take
   length and angle; a rectangle takes width and height, each at the outer midpoint of its own
   side; a circle takes a radius; an arc draws its chord as a construction line and every
   segment gets the reference axis and an arc whose radius is the segment length. A fixed tool
   last point commits the figure as a click does; a polyline keeps its chain and its line/arc
   switch moves to the right-click menu, because Tab now belongs to the fields. The plate never
   covers the point being placed or the snap marker and flips at the edge of the workspace. One
   ADR records the smaller-angle rule and the typed-field grammar.
  Done: Every shipped sketch tool shows its fields; a typed length and angle place the point
   exactly and the preview follows; the angle display and its typing obey the smaller-angle
   rule, tested as a table across the four quadrants; a typed number survives a snap and a 0
   gives the parameter back; Tab, Enter and Escape are tested through RawInput; the polyline
   segment kind is switched from the context menu and Tab no longer toggles it; a fixed tool
   commits once and one undo entry; cargo xtask ci is green.
  `files: crates/ui/src/draw.rs, crates/ui/src/editor/drawing.rs, app/src/draw_gizmo.rs, app/src/document.rs, crates/ui/src/editor/ribbon.rs` — `dc33a92`

- `[x]` **C1** the point being placed shows its numbers at the cursor, and a sketch has two guide kinds
  Why: Every sketch tool needs the same two things: the number of the point under the cursor
   and a guide that shows zero and the sweep. Both exist twice today — tool_card (ADR 0119)
   plus the private NumericEntry in contour_gizmo.rs, and the paint_size label in draw_gizmo.rs
   — so a third copy would be the defect ADR 0118 names. The reference (OpenCAD Studio) puts
   the length on the segment, the angle outside an arc whose radius is the segment length, and
   a dashed axis along +X from the anchor; a pale single colour cannot pass the guides 3:1 rule
   on a light board.
  Shape: One atom in crates/ui: FieldSet owns the plates, the always-on focus, the typed locks,
   Tab, Enter, Escape and the 0 that returns a parameter to the pointer; each plate is the
   shared scrub (ADR 0092) in the tool card frame at its own anchor, so the plate at the point
   and the inspector cannot drift. One painter owns the guides: Guide::Construction (the dashed
   amber of the measured chord) and Guide::Reference (the +X axis from the anchor and the angle
   arc, radius equal to the segment length), both in screen space, the reference drawn as the
   two-stroke pair (light inside dark) and thinner than every other guide, with the theme
   accessor and the contrast test. contour_gizmo private numeric entry is replaced by the atom;
   the drawing tool keeps its label until C2 puts its plates there. Two ADRs: the field at the
   point extends 0119, the two guide kinds amend 0111.
  Done: contour_gizmo draws its number through the one atom and keeps no private copy; a
   RawInput test drives focus, Tab, typing, Enter, Escape and 0 and asserts the value, the lock
   and the plate rectangle each frame; a theme test asserts one stroke of the reference pair
   reads on the header, a panel and the workspace of every preset and that its dot is thinner
   than every guide; both ADRs are in docs/decisions and the README lists them; cargo xtask ci
   is green.
  `files: crates/ui/src/tool_card.rs, crates/ui/src/scrub.rs, crates/ui/src/theme.rs, app/src/contour_gizmo.rs, app/src/draw_gizmo.rs, docs/decisions` — `bf165a1`

- `[x]` **U99** the niche becomes a tool of its own, and fill, a shelf and a report consume the same value
  Why: The owner example: the geometry of the empty space was written for one tool, and a
   mechanism two tools need must be a tool itself (ADR 0118). Today `fill_gizmo` computes the
   niche inside the fill arm, so no other tool can ask for one and a report or the assistant
   has no way to name the space.
  Shape: One niche atom (`probe.void`) in `crates/ui/src` with its own arm, cursor, ghost and
   cancel, returning the cell from the model probe (0103, 0106, 0109); a ribbon row in the
   Furniture tab with a `Context` place and a key. Fill becomes its first consumer and reads
   the identical value, so the ghost and the entry stay one code path. A second consumer proves
   the atom: the inspector or a report row names the niche the pointer holds. No document write
   and no second probe.
  Done: The niche tool arms from the ribbon and from the fill own affordance, and both return
   the same cell for the same pointer; a test calls the atom once and asserts fill and the
   inspector see the same value; cancelling either leaves no document change; `plan/map.md`
   gains the atom row; `cargo xtask ci` is green.
  `files: app/src/fill_gizmo.rs, crates/ui/src/editor/fill.rs, crates/ui/src/toolbars/spec.rs` — `ada0639`

- `[x]` **T8** the P2 and P4 accents separate from the model and scene inks
  Why: The P2 azure accent is 7.2/8.6 Delta E from the model blue and the P4 mint accent is
   7.0/1.9 Delta E from the scene green, so the accent stops reading as chrome beside those
   glyphs, contrary to U70. The role inks are preset-independent, so the owner chose to move
   the presets' accents and keep the inks (option A, 2026-11-15).
  Shape: Option A, chosen by the owner: keep the seven role inks and move the P2 and P4 accents
   so each is at least 20 Delta E from every role ink and keeps 3:1 contrast on panel, ribbon
   and surface in both variants. Measure candidates against all seven inks including reference;
   decide the preset label and stored-name consequence (a saved settings name must keep
   loading); add the hue-distance test; update U71 and the ADR if the accent rule changes. No
   icon redraw.
  Done: The P2 and P4 accents are chosen and measured: each at least 20 Delta E from every role
   ink and at least 3:1 on panel, ribbon and surface for every preset and variant; the preset
   labels and stored names are consistent and a saved settings file still loads; the
   hue-distance test sits beside the contrast test; U71 and the ADR/docs are updated; cargo
   xtask ci is green.
  `files: crates/ui/src/theme.rs, crates/ui/src/icons.rs, plan/notes/U70.md, docs/decisions` — `a390583`

- `[x]` **T7** the rest of the icon set follows the v2 language, one tab at a time
  Why: T6 migrates the Sketch tab only; the Model, Furniture, Scene, View, Actions and Window
   icons still carry the v1 weight and footprint, so the set is half-migrated.
  Shape: Redraw one tab per commit using the T6 rules; update the README rows and the icon
   tests; no family beyond reference; keep the role inks.
  Done: Every declared icon is drawn in the v2 language; no v1 icon remains; the icon gate and
   cargo xtask ci are green; the live ribbon reads as one set.
  `files: assets/icons, crates/ui/src/icons.rs, assets/README.md` — `0a0f053`

- `[x]` **T6** the icon language v2 starts with the Sketch tab and six dimension types
  Why: The owner approved the v2 language — 24-grid, 2.25..21.75 footprint, 1.5/1.2 strokes,
   butt/miter, circle nodes r2 with a 1.15 gap, neutral view for source and control, class ink
   for the result, reference for a dimension annotation — but it lives only in a /tmp demo. The
   pipeline, the role table and the gate still describe v1, and the six dimension icons are the
   first surface that needs it.
  Shape: Add the reference role to Family and RoleRamp and to the two icon documents; move the
   gate to 2.25..21.75 and 10-40% of 20x20; raise the ribbon icon to 26 pt in a 38 px cell
   (toolbar 22 pt); redraw the Sketch tab and the six dimension images (overall, from base,
   chain, angular, radius, diameter) with the approved order, angle and gap rules. The approved
   measurements and the six arrangements are in plan/notes/T6.md. The rest of the set stays v1
   until T7.
  Done: ADR 0124 and the style guide/README record the language; the pipeline maps the seventh
   role and the gate measures the new footprint/coverage; every Sketch cell and all six
   dimension icons ship in v2; the role contrast and chosen-glyph tests pass; cargo xtask ci is
   green; the live ribbon at 100% and 200% is checked.
  `files: assets/icons, crates/ui/src/icons.rs, crates/ui/src/theme.rs, assets/README.md, assets/icons/STYLE_GUIDE.md, docs/decisions` — `58b80f6`

- `[x]` **T5** the drawing rules are a skill, and a dimension follows them
  Why: Drawings and the tools that name them — dimensions, views, sections, exports — are being
   designed from general CAD habit, so each session re-derives the GOST/ISO rules and the
   dimension icons disagree about order, the radius angle and the arrow gaps. The knowledge
   belongs in one place the next session reads.
  Shape: A `drafting` skill under `.pi/skills/drafting`: the six dimension types and their
   order, line weights, first-angle views, sections and furniture notes in two references; the
   icon grammar (source `view`, result class, dimension `reference`, node gap, witness
   extension) with the standards cited by number, not copied. `ui-design` and `draw-icon` route
   to it.
  Done: The skill and both references exist within the line budgets, `cargo xtask skills` names
   no missing path, and the six dimension types plus the baseline and radius corrections are
   written as rules a session can apply; `ui-design` and `draw-icon` name the skill; `cargo
   xtask ci` is green.
  `files: .pi/skills, .pi/skills/ui-design/SKILL.md, .pi/skills/draw-icon/SKILL.md` — `1ce501f`

- `[x]` **U98** the ruler is a transient measuring tool: two clicks, the length at the pointer and in the status bar, nothing written
  Why: The owner wants a measurement that is information, not geometry, after the SketchUp tape
   measure: the value on screen and in the status bar, several measurements in a row, each
   starting fresh. The program has no measuring atom, and a niche is measured only by the fill
   tool. ADR 0118 makes the ruler an atom — no command, no undo — and forbids a second point
   resolver or probe.
  Shape: A `measure` atom in `crates/ui/src/measure.rs` holding the first point and the current
   value; it consumes the shared snap resolver (0051) and the probe (0103) and formats lengths
   through `units`. The state machine: idle, first point, measured; a third click
   starts a new measurement rather than extending the last; Escape leaves and writes nothing.
   The length is painted at the pointer and written to the centre zone of the status bar, while
   the left zone says what to do. The ribbon gains a `sketch.measure.length` row with its icon
   in `assets/icons/contour`, a `Context` and `Always` place and a key. No command id, no
   document field, no undo entry.
  Done: A headless test clicks two snapped points and reads the value, clicks a third and
   asserts a new measurement, and presses Escape and asserts nothing was written and no
   revision moved; the status bar shows the value and the next step; the icon file, its README
   row and the ribbon cell land together; `plan/map.md` and `references/surfaces.md` gain the
   row; `cargo xtask ci` is green.
  `files: crates/ui/src, app/src/status_bar.rs, assets/icons/contour` — `08f51e4`

- `[x]` **U97** an armed tool shows its parameters at the pointer, and the inspector shows the same fields
  Why: The owner asked for both ends of one rule: the number at the cursor and the full data in
   the inspector. Today a tool has no home for its own parameters — a fill distance lives in
   the exact-distance fields, a side count in the ribbon typed defaults, a panel size in the
   inspector — so a new tool invents a fourth place. ADR 0119 makes one small card the surface
   for a tool parameters.
  Shape: A `ToolCard` surface drawn near the pointer while a tool is armed and parameterised,
   holding the shared scrub fields (0092): it appears with the arm, closes with the commit, the
   cancel or Escape, is declared to `Chrome::covers` and is one row of `HOST_OWNERS`. The
   inspector shows the same values from the same draft, and both commit through the one
   command. The exact-distance fields become the card first instance: one layout, one commit
   path, one edit scope. Typing pins the value the pointer drove and the preview follows;
   clearing the field returns it to the pointer.
  Done: A box, a fill and an extrusion show their parameters in the card and in the inspector,
   and a headless frame test finds the two never disagree; a press inside the card is chrome
   and no pick; Escape closes it and writes nothing; the card rectangle is asserted at two UI
   scales; `plan/map.md` and `references/surfaces.md` gain the surface; ADR 0119 is followed;
   `cargo xtask ci` is green.
  `files: crates/ui/src/tool_card.rs, crates/ui/src/distance.rs, app/src/document.rs` — `01c7533`

- `[x]` **U96** the keymap is a column of the inventory, so a key is data and remapping needs no refactor
  Why: `shortcuts.rs` reads `Ctrl+S`, `Ctrl+N`, `Ctrl+W`, `Ctrl+Z`, `Ctrl+Y`, `F`, `O`, `Alt+W`
   and `Ctrl+K` from a hand-written match, and the ribbon carries a second shortcut label per
   method (`shortcut_label`). Two registers mean two places to change, and a user-assignable
   key would have to replace both (owner answer: fixed defaults first, remapping later). ADR
   0117 makes the key a column of the inventory.
  Shape: Every inventory row (U94) carries its default key; `shortcuts.rs` asks the table by id
   instead of matching key codes, and the ribbon label is derived from the same row. A test
   asserts the two agree for every live method and that no key is bound twice; the guard that
   keeps a key out of a focused text field stays as it is. Remapping is not in this task: the
   table is the data a later Settings section writes.
  Done: One default key per method, read from the table by `shortcuts.rs` and by the ribbon
   label; no key is bound twice; a headless test presses each default key and asserts the same
   command runs; remapping stays planned and `plan/debt.md` gains it if it is deferred; `cargo
   xtask ci` is green.
  `files: app/src/shortcuts.rs, crates/ui/src/toolbars/spec.rs` — `d1dacb1`

- `[x]` **U95** two skills carry the interface rules to where they are needed: placing a capability and designing an operation
  Why: `ui-design` is one procedure for a surface, and it fires when a file under `crates/ui`
   is touched — after the decision that matters most, where a capability lives (ADR 0117), is
   already made. Nothing owns the operation lifecycle (arm, preview, typed number, commit,
   cancel, undo), which is where the feel of a professional editor lives, and nothing in the
   checklist asks whether a mechanism is private (ADR 0118).
  Shape: `.pi/skills/place-capability/SKILL.md` activates when a command id or a place is
   added, renamed or removed: it writes the inventory row, the places, the context, the key and
   the icon, names the atoms it consumes, and ends with the gate command.
   `.pi/skills/design-operation/SKILL.md` activates when a gesture or a mode appears: it
   produces the state machine, the feedback budget, the typed input, the cancel, the one undo
   entry, the awkward cases and the cursor, in the five-line decomposition the ui-design skill
   already uses. `ui-design` keeps the rubric and points at both; `references/checklist.md`
   gains the inventory question, the atom question and the icon question;
   `references/surfaces.md` stops being the only knowledge of places and points at the
   inventory. `AGENTS.md` §1.3 names the new skills, and `cargo xtask skills` checks their
   paths, frontmatter and line budgets.
  Done: Both skills load, each names its trigger and its output, and neither is over 200 lines;
   `AGENTS.md` §1.3 names them; the checklist carries the three new questions with an evidence
   form; `cargo xtask skills` and `cargo xtask ci` are green.
  `files: .pi/skills, AGENTS.md` — `eec5ad7`

- `[x]` **U93** the object card is reviewed against the rubric and redesigned as one reading order
  Why: The card a person reads most is 1780 lines and up to seven collapsible sections stacked
   under two other cards, so the same fields appear in two places, the first screen is mostly
   headers, and a mesh object is offered contour, figure, primitive and mesh sections of which
   at most two can apply. The owner asked for the card to be studied, the excess removed and
   the design improved; this is the one task of the pass that starts from a measurement rather
   than from a bug.
  shape: First the audit the ui-design skill orders: a brief in five lines, the inventory of
   every section and field with the node kinds that can show it, and three options with their
   cost in code and in learned conventions. Then the chosen shape — most likely one summary
   band (kind, size, material, one warning chip), only the fields the selected node can use,
   the modes (mesh edit, contour edit) as one row of actions rather than four collapsible
   sections, and a fold state remembered per node kind. Every number stays the shared scrub
   field, every mutation stays a command, nothing new enters the document; a removed section or
   a changed reading order is a break the owner approves and ADR 0116 records. The measurement
   and the rejected options go to `plan/notes/U93.md`.
  done: The audit and the options are in `plan/notes/U93.md`; the card draws no section that
   cannot apply to the selected node and no field twice on one screen; a headless test opens a
   panel, an object and a product frame and asserts the sections drawn for each; an offscreen
   shot of the three cards in both themes is looked at; ADR 0116 is written when a break ships;
   `cargo xtask ci` is green.
  `files: crates/ui/src/object_card.rs, crates/ui/src/editor/cards.rs, crates/text/src/messages.rs, app/src/ui_shots.rs` — `43ef744`

- `[x]` **U92** a right click opens the actions of the node under the pointer, from one list the tree and the viewport share
  Why: The tree row menu is the only context menu in the program: a right click in the viewport
   draws nothing, so the actions a selected object has are two or three clicks away in the
   tree, and the tree's own list is built inside an 850-line file. The owner asked for context
   menus that carry the actions of the object under the pointer.
  Shape: One module builds the action list for a node kind — a panel, an object, a product
  container, a group, a camera or a light, the root and empty space — from the command
  registry and the editor's flags, each entry a command id, its label message, its enabled
  check and the reason it cannot run. The list is the capability inventory (U94) filtered
  by the node kind and the `Context` flag, not a list this module writes, so U94 is taken
  before this task and the three orphans are placed there. The row menu and a new popup in the
  viewport draw that list, so an entry cannot exist in one and not the other; the popup is
  anchored at the pointer, closes on Escape and on an outside click, and is declared to
  `Chrome::covers`, so a press on it is chrome and never a pick. The viewport reads a right
  *click* (press and release within a few points) and leaves the right *drag* to the camera
  pan it already owns. The new module is one row of `HOST_OWNERS` in
  `xtask/src/ui_rules.rs`. ADR 0115 records the click-versus-drag split and the one-list rule.
  Done: Right-clicking a panel, a group, a camera and empty space each opens the list that
   object owns, with the same entries the tree shows for it; right-dragging still pans and
   opens nothing; a headless frame test right-clicks a panel and asserts the menu's rectangle
   and its first entry, and that the press changed no document revision; `plan/map.md` and the
   ui-design skill's `references/surfaces.md` gain the surface; `cargo xtask ci` is green.
  `files: crates/ui/src/scene_tree.rs, crates/ui/src/lib.rs, app/src/document.rs` — `32a39fb`

- `[x]` **U91** the snap toggle leaves View › Display for a group of its own captioned Snaps
  Why: The grid-snap toggle sits under View › Display among style, contours, shadows and
   section switches, so the one control that changes how a click behaves is filed with the ones
   that change how the scene looks. The owner asked for the caption «Привязки»; a group of its
   own is the smallest change that says where a snap lives, and it gives the endpoint, axis and
   object snaps a home.
  Shape: The map gains a `view.snaps` group (label `GroupSnaps`, «Привязки») holding the grid
   snap as `view.snaps.grid` with its `G` key and hint unchanged; `view.display` keeps its
   style, contour, shadow and section tools. The message table gains `GroupSnaps` in both
   languages, the ids stay namespaced by their group, and the spec's uniqueness and live-group
   tests cover the new row. No new dependency and no document change.
  Done: The View tab shows a group captioned «Привязки» with the grid-snap cell and a Display
   group without it; every id is unique and namespaced; the `G` key still toggles the snap;
   `cargo xtask ci` is green.
  `files: crates/ui/src/toolbars/spec.rs, crates/text/src/messages.rs` — `d73bb1d`

- `[x]` **P15** the plan tool teaches its grammar instead of refusing it
  Why: Measured over the six tasks that carry a log: the plan tool cost ~16 of ~119 minutes, in
   four of the six tasks, and every minute was a refusal that did not say what it wanted. U88
   spent 7 minutes and four attempts on ## Where we are, because the refusal prints the result
   (94 lines, over the 70-line budget) and not the cause (the body re-wraps at PROSE_WIDTH, so
   every submit grows); derive dry-run prints a diff and no line count. Four close refusals in
   four tasks were the same Nothing is scheduled rule, which the tool can write itself when ##
   Now is empty. Three sessions typed take then start because take leaves state=idle and start
   refuses a task outside the order. Two accepted gaps in plan/debt.md take minutes from every
   session: no verb updates a running session's own now:/resume:, and --field accepts a label
   the entry does not have.
  Shape: One decision, one commit: the plan tool teaches its grammar instead of refusing it.
   validate: the budget refusal names the wrap width beside the count, and state set prints
   candidate lines, budget and width under --dry-run. write: task start <ID> takes a head of ##
   Next as well as the current task, so take and start collapse into one verb for the common
   case and take stays for the top of a running order; task close writes the Nothing is
   scheduled paragraph itself when ## Now is empty and --next is absent. A verb that writes a
   running session's now: and resume: beside step, and --field refusing a label outside
   ENTRY_LABELS instead of appending it. What is not in scope: no change to any rule the gate
   enforces, only to what it says and to which verbs make a refused sequence unnecessary; no
   new plan file.
  Done: A test per sentence: the budget refusal names the width, --dry-run prints the three
   numbers, task start takes a ## Next head, task close without --next is legal when ## Now is
   empty, a running session updates now: with the new verb, and --field on an unknown label is
   refused; the two matching plan/debt.md entries are deleted in the same change; cargo xtask
   ci is green.
  `files: state-contract/src/cli/write.rs, state-contract/src/cli/mod.rs, state-contract/src/edit.rs, state-contract/src/validate.rs` — `355c353`

- `[x]` **P14** an entry's Done names the rule and the session reads the skill the route names
  Why: Measured over the six tasks that carry a log (U88, U89, U90, U94, X41, X42; ~119 min):
   62 of those minutes went into one feature built in three passes and 22 of them were repair.
   Both repairs (X41, X42) blame the same two mechanisms — a Done clause that asserted the
   implementation instead of the rule, and a session that read the entry and the code but never
   the skill AGENTS.md 1.3 routes it to — and that route already held the answer in both cases.
   The log itself is the third defect: dead-end is listed as a phase and is also inside
   read/test/edit, so U88's shares sum to 103 percent and U89's to 128; U88 guessed four
   timestamps and wrote them out of order; no log records the start of the session or which
   turn came from the owner.
  Shape: Three prompt files, no Rust. plan-task: Done becomes the tests that would fail if the
   rule were broken, at the surface where a person meets it, replacing the tests that would
   fail before the change; Shape may name a command id only with what its parameters accept,
   otherwise it names the outcome. next-task: the Start step reads the routed skill before the
   first edit (the design skill for anything under crates/ui or the app chrome), replacing the
   sentence that names only the notes and the ADRs; and the log loses its HH:MM stamp, which
   cost a date call per line and produced the format's only lie, keeping the +m cost that
   cannot be guessed. The log gains an attribute form for dead-end, a first line +0 start, the
   owner phase and test red-mine | red-tree. time-review says five parts where it lists five.
   What is not in scope: no new phase vocabulary beyond these, no scraper or metrics tool (six
   tasks are not a series), no rewrite of the 73 notes that predate the format.
  Done: All four prompt files carry the new wording and cargo xtask skills is green; plan-task
   turns a defect into an entry with a Done that could fail (checked against the X41
   mechanism), and next-task names the design skill in its first section (checked against the
   X42 mechanism); the log format names no clock command, so a session needs no date call to
   keep it; time-review lists five parts under the word five; the plan gate is green.
  `files: .pi/prompts/plan-task.md, .pi/prompts/next-task.md, .pi/prompts/time-review.md, .pi/prompts/fix.md` — `f43eb48`

- `[x]` **U94** the capability inventory is one table of places, and a gate refuses an orphan command
  Why: Measured 2026-11-14: 56 command ids (the count the registry test asserts), three of which
  (`CONTOUR_CREATE_ID`, `NODE_ORIGIN_REBASE_ID`, `NODE_PROPERTY_SET_MANY_ID`) are named only
  inside `crates/engine`; the ribbon map holds 29 live methods and 62 planned rows; the palette
  builds parameters for twelve ids and refuses the rest. Nothing checks that a command has a
  home, so every new surface decides its own list and the owner rule — every feature has a
  representation, the next tool is under the hand — is kept by memory alone (ADR 0117).
  Shape: A new `crates/ui/src/inventory.rs` holds one const table keyed by id: the places
   (`Ribbon`, `Context`, `ToolCard`, `Palette`, `Card`, `Api`), `Context` or `Always`, the
   default key, the icon and the label `Message`. The ribbon, the palette and the planned
   context menu (U92) read it instead of declaring their own lists; `params_for` stays the
   parameter builder, but each id it can run gets a row. `cargo xtask ui-rules` gains the
   coverage check: a registered command with no row, a row naming an unknown id, message, icon
   or surface, a live row without art and one id with two labels or keys all fail, while one
   icon file shared by two unrelated actions is printed for review. The three orphans are
   placed or declared `Api` with the reason. The audit that seeds the table — where each of the
   56 commands lives today — goes to `plan/notes/U94.md`. U92 and U91 are taken after this
   task, so both filter the table instead of writing a fourth list.
  Done: `cargo xtask ui-rules` fails on a command with no row and passes on the tree; every
   live ribbon row names an icon file and a message; the three orphans have a place or an `Api`
   reason; a test adds a fake command and asserts the check names it; `plan/map.md` gains the
   module row; `cargo xtask ci` is green.
  `files: crates/ui/src, xtask/src/ui_rules.rs, crates/engine/src/commands.rs` — `633442f`

- `[x]` **U90** the Furniture tab loses the loose Panel cell and offers a panel by contour instead
  Why: `furniture.panels.panel` creates a panel from a fixed size in a card, outside any
   product — the pre-frame way of making a board; the product's post and shelf cells make a
   board where a niche has measured one, and the approved map already holds
   `furniture.panels.contour` («Panel by contour») as planned. The owner asked for the old cell
   and its path to go rather than stay as a second way to do the same thing, and the contour
   cell is what fills the gap.
  Shape: Delete the cell, `ToolKind::Panel`, `ToolAction::CreatePanel`, `Editor::create_panel`
   and the ribbon fields only it read; `panel.create` stays, because fill, the palette and the
   contour cell use it. Promote `furniture.panels.contour` to a live cell that arms a ring
   drawn in the work plane and dispatches `panel.contour.create` when the draft closes, reusing
   the drawing draft and the envelope the panel card already has; if the armed drawing cannot
   land in the same change, the task keeps the cell planned and says so in `plan/notes/U90.md`
   instead of shipping a second dead cell. The live-cell test, the palette entry and
   `plan/map.md` follow. Removing an affordance is a break the owner approved, recorded in ADR
   0114.
  Done: The Furniture tab draws *Product frame*, *Post*, *Shelf* and *Panel by contour*; no
   ribbon path reaches `ToolKind::Panel`; no test or spec mentions the removed id; a frame test
   draws a ring with the contour cell into an open product and finds one panel whose outline is
   that ring; ADR 0114 is written; `cargo xtask ci` is green.
  `files: crates/ui/src/toolbars/spec.rs, crates/ui/src/editor/creation.rs, crates/ui/src/toolbars/ready.rs` — `4d83409`

- `[x]` **U89** an open group or product draws its corner brackets, so the open level reads in the viewport
  Why: An open container and a closed one look identical over the scene: only the tree says
   which level is open, and once a click inside an open frame selects a part (X38) that
   difference decides what the next click does. The owner asked for small brackets at the
   corners — an orientation cue and a statement that the level is open — not a wireframe, which
   would fight the form edges the renderer already draws.
  Shape: `scene_build` already computes a `SubtreeBounds` per container in
   `Evaluation::collect`; the shell collects the bounds of `editor.open` plus
   `session.active_product()`, projects the eight corners with the view camera and paints, per
   corner, three short arms along the box's own edges, each arm a fraction (about a fifth,
   clamped to 6..=18 pt) of the shortest projected edge, in `theme::helper()` at the bracket
   width (U83). The paint is screen space on `Order::Middle` with the viewport's clip rect,
   after the image and before the gizmos, so no render pass changes. A closed container, a
   hidden one and a product that is not open draw nothing; the brackets are view state and no
   revision moves.
  Done: Headless geometry tests on a fixed camera pin the eight corner points and the arm
   length at two zoom levels; a frame test opens a group and counts exactly eight brackets,
   closes it and counts none, and the same for an open product; the ink is `theme::helper()`;
   `plan/map.md` gains the row; `cargo xtask ci` is green.
  `files: app/src/open_frame.rs, app/src/scene_build.rs, app/src/document.rs, crates/ui/src/editor/selection.rs, app/src/frame_tests.rs` — `d231f22`

- `[x]` **X42** the work-area guide and pointer sit under the chrome, and expertise challenges a brief before code
  Why: The square and the crosshair are work-area paint, but they still sit on Order::Tooltip,
   so the crosshair crosses the side cards and the pointer can paint over their edge. The owner
   chose A: move both under the chrome instead of clipping around it. The same exchange asked
   for a standing instruction: the agent's domain and UI/UX expertise must challenge a brief
   that contradicts it, with one question before the tree is touched.
  Shape: Put the work cursor's layer on Order::Background, painted after the scene image and
   its gizmos, so every Middle/Foreground surface covers it. Keep the scene clip and the
   pointer-only-on-scene rule. Add ADR 0123 for the layer. Replace the 'Ask when unsure' bullet
   of AGENTS.md 8 with 'Expertise leads, the owner decides', and add the same z-order clause to
   the UI skill's floating-surface bullet without growing the instruction by a line.
  Done: The layer test that says the square and crosshair are Background fails today and passes
   after; the offscreen shots in both themes show the crosshair interrupted by the side cards
   and the square over the scene; the AGENTS bullet names expertise before execution; cargo
   xtask ci and cargo xtask docs are green with ADR 0123.
  `files: app/src/work_cursor.rs, app/src/frame_tests.rs, AGENTS.md, .pi/skills/ui-design/SKILL.md, docs/decisions` — `063ad6a`

- `[x]` **X41** the custom pointer belongs to the work area and every system element keeps the system cursor
  Why: The owner's rule is the one U88 started from and X40 lost: the custom square belongs to
   the work area only. Over a panel, a widget, a text field or a resizing border the system
   cursor is the affordance, and a menu keeps it too. X40 read the owner's 'everything but
   menus' literally and moved the square onto the panels and the floating cluster; its Shape
   said 'every top layer that is not Foreground' and its Done asserted the square over a panel
   and the cluster, so the gate proved the wrong rule. The owner's instruction was wrong too,
   but the session did not raise the conflict with U88's 'a panel keeps the arrow' or with the
   system icons a panel needs.
  Shape: The shell paints the square only when the pointer is inside the scene rectangle, the
   top layer at the pointer is Background, the corner orientation widget is not under it and no
   widget asked for an icon of its own. The square and crosshair are clipped to the scene
   rectangle; every panel, floating surface, text field, resizing divider and menu keeps the
   system cursor. The two-tone outline and the layer stay. ADR 0122 supersedes 0121's scope and
   keeps its outline; the UI skill gains the surface rule in the floating-surface bullet.
  Done: A test that fails today asserts that the pointer over the left panel is Default with no
   square, and the same over the floating cluster, the corner orientation widget and a
   split-view border; a text field keeps its I-beam; the scene and an armed contour/primitive
   still hide the arrow and draw the square; the product card keeps the arrow; the offscreen
   shot points into the scene and shows the two-tone square in both themes; cargo xtask ci and
   cargo xtask docs are green with ADR 0122.
  `files: app/src/work_cursor.rs, app/src/main.rs, app/src/document.rs, app/src/frame_tests.rs, docs/decisions, .pi/skills/ui-design/SKILL.md` — `d7c26d9`

- `[x]` **X40** the work cursor is the shell's pointer everywhere but a menu, and its two-tone outline shows on any surface
  Why: U88 made the square the scene's fallback and let each gesture keep an OS icon, so a
   contour mode showed an arrow and an armed primitive a crosshair; it painted the square
   inside the viewport on the scene layer, so the floating cluster covered it, and a modal
   drawn above it left no pointer at all; one amber ink vanished on a light board. The owner
   found this in a live session, while the headless geometry tests U88 passed asserted only the
   square's rectangle and the icon in two states.
  Shape: Move WorkCursor out of document.rs into a shell module. The frame resets
   CursorIcon::Default, draws every surface, then paints the square on Order::Tooltip at the
   pointer unless the top layer there is Foreground (a menu, a popup, a modal) or a widget
   asked for its own icon (a text field, a divider, a scrub). The viewport keeps only the
   divider's resize icon: the owned_cursor chain and the tools' cursor methods go. The square,
   dot and crosshair are a 3 pt dark and 1 pt light pair from theme.rs; the square clips to the
   window, the crosshair to the scene rectangle.
  Done: Headless tests assert the icon is None and the square is centred over the scene with a
   contour tool, an armed primitive and a box; over the bottom cluster and a panel; the arrow
   returns and no square is drawn over an open menu, popup or modal (the product card); a text
   field and a divider keep their own icons; the two outline inks differ and sit at opposite
   ends of the brightness range; an offscreen shot in both themes with the pointer over the
   bottom cluster shows the square above it; cargo xtask ci and cargo xtask docs are green with
   the new ADR listed.
  `files: app/src, app/src/document.rs, crates/ui/src/theme.rs, app/src/frame_tests.rs, docs/decisions` — `6fe7481`

- `[x]` **U88** the viewport draws its own work cursor, sized in settings, with an optional full crosshair
  Why: The OS arrow hides the point a click lands on and reads as a pointer to buttons rather
   than to geometry; a small square with a centre dot says this is the point, and a full
   crosshair lets a person line up two boards across a view. The owner asked for the square, a
   size that is a setting with limits, and the crosshair as an option. A custom bitmap cursor
   is not available through eframe 0.37 (`egui::CustomCursorImage` is web-only in the native
   backend), so the shell draws the cursor itself.
  Shape: A `PointerSettings { cursor_size: u32, crosshair: bool }` section in
   `crates/ui/src/settings.rs` (limits 6..=32 pt) with a scrub field and a switch in Settings ›
   View. While the pointer is over the active scene the shell sets `CursorIcon::None` and
   paints, on `Order::Middle` over the image and under the chrome, a square of `cursor_size`
   points centred on the pointer with a 1 pt outline and a 2 pt centre dot in the helper ink
   (U83), plus, when `crosshair` is on, one 1 pt hairline across the whole viewport width and
   height. A gesture that owns its own cursor (grab, resize, divider, scrub) keeps the OS icon
   and the square is not painted; the paint is chrome, so it takes no click, and it never
   enters the document.
  Done: Headless tests assert the OS cursor is `None` exactly while the pointer is over the
   scene and `Default` over a panel, that the square is centred on the pointer at the minimum
   and the maximum size, and that the crosshair spans the viewport; settings round-trip clamps
   2 to 6 and 64 to 32; an offscreen shot in both themes shows the square and the crosshair
   (the eye check this task names); `cargo xtask ci` is green.
  `files: app/src/document.rs, crates/ui/src/settings.rs, app/src/main.rs` — `b3a6d40`

- `[x]` **U87** the transform mode can be off, and Select is the state a window opens in
  Why: One of the two gizmos is always up, so a person who only wants to look, pick and read
   works with handles they did not ask for, and a stray press on an axis moves the cabinet. The
   owner asked for the third state and for Select as the default, which is what a professional
   editor opens in.
  Shape: `TransformMode` gains `Select`, the shell defaults to it, and the status bar draws the
   three as one segmented control. `document.rs` draws and updates a gizmo only in `Move` and
   `Rotate`; in `Select` a click selects, shift adds, a double click still enters a level, and
   a press where a handle would be does nothing. The ribbon's two cells stay pressed only while
   the matching mode is on. The mode stays view state, per run, with no undo entry and no
   `.fcs` field. A default a user meets on launch is an owner-visible break, so ADR 0113
   records it.
  Done: A fresh window draws no handles and a click selects; `Move` and `Rotate` each draw
   their own gizmo and nothing else; a headless frame test asserts no handle consumes the click
   in `Select`; the status bar control changes the mode; ADR 0113 is written; `cargo xtask ci`
   is green.
  `files: app/src/main.rs, app/src/status_bar.rs, app/src/document.rs` — `5d08a7d`

- `[x]` **U86** the transform gizmos are smaller and their size is a setting
  Why: The translate handles are 84 pt (`gizmo_math::HANDLE_LENGTH`) and the rotate rings are 9
   % of the camera distance (`rotate_gizmo::RING_RADIUS_FACTOR`), so on a small part the gizmo
   is larger than the part and covers the face the person is judging and the distances the drag
   measures. A hand that works at 4K and a hand at 1080p want different sizes, so the number
   belongs in Settings › View rather than in a constant.
  Shape: A `GizmoSettings { handle_points: u32, ring_percent: u32 }` section in
   `crates/ui/src/settings.rs` with limits (`MIN`/`MAX_HANDLE_POINTS`,
   `MIN`/`MAX_RING_PERCENT`), a default well below today's 84 pt and 9 %, and a scrub field for
   each, clamped on read and repaired like `roll_step_degrees`. `paint_axis_handles` and
   `hit_axis` take the handle length from the caller so one value serves drawing and picking,
   `rotate_gizmo` takes its radius factor the same way, and the exact-distance fields follow
   the anchor. The shell hands the resolved settings to every tab the way it hands
   `ViewportSettings`; nothing enters the document.
  Done: Both gizmos are visibly smaller at the default and scale with the two fields;
   `settings.json` round-trips, clamps 8 and 400, and repairs a broken value instead of costing
   the file; a headless test drags an axis at the minimum and at the maximum handle length and
   the hit radius follows; no document revision moves when the size changes; an offscreen shot
   shows the small and the large gizmo; `cargo xtask ci` is green.
  `files: app/src/gizmo_math.rs, app/src/rotate_gizmo.rs, crates/ui/src/settings.rs` — `115d4f2`

- `[x]` **U85** the Delete key removes the whole selection behind one question
  Why: Shift-click builds a multi-selection but nothing deletes it: the tree's row menu removes
   one node at a time, so a keyboard user cannot remove what they selected, and looping
   `node.delete` leaves one undo entry per node. The owner asked for the whole set at once,
   with a confirmation when the key does it.
  Shape: A new `node.delete.many` in `crates/engine/src/node_cmds.rs` takes `{ nodes: Vec<Id>
   }`, is enabled when at least one node exists, skips a node whose ancestor is also in the
   list, removes the rest and returns one changed outcome, so the session records one undo
   entry; the registry and its tests gain it. `Del` is read in `shortcuts.rs` only with no text
   field focused and no modal up, and dispatches it for `editor.selected_nodes()`: one node
   deletes at once because undo covers it, two or more open the question host `file_menu.rs`
   already owns (`ConfirmClose`) with the count and *Delete*/*Cancel*, Escape cancelling. A
   locked node refuses the whole operation with the existing `locked_command_refusal` sentence.
   ADR 0112 records when a destructive key asks and when undo is enough.
  Done: Tests cover one node, three nodes where one contains another, a locked node and undo
   restoring all of them in one step; a headless frame test presses Del over two selected nodes
   and asserts one question with no document change until *Delete*; the label and the question
   are message ids; ADR 0112 is written; `cargo xtask ci` is green.
  `files: crates/engine/src/node_cmds.rs, crates/engine/src/commands.rs, crates/model/src/project/node_cmds.rs, app/src/shortcuts.rs, app/src/document.rs, app/src/file_menu.rs, app/src/main.rs, app/src/frame_tests.rs, crates/ui/src/palette.rs, crates/text/src/messages.rs` — `4586041`

- `[x]` **U84** the method list is one flat list of names: the section captions go
  Why: The owner asked for the captions to go: a «Build» or «Tangents» heading over one or two
   rows is a second line to read, it makes a small menu look like a form, and the row's own
   name already says how the figure is built. They exist only inside the ribbon's variant list
   (U67, U74) and are the only place a method is grouped.
  Shape: Delete `Method::section` and `VariantSpec::section` from `toolbars/spec.rs` and the
   section inheritance in `ToolSpec::default_method`, the `CAPTION_HEIGHT` constant and the
   caption drawing from `toolbars/list.rs`, and the three `Section*` messages from
   `crates/text`. The rows keep the order the map declares, so the list is the header plus one
   row per method; a test asserts the drawn order equals the declared order and that a list's
   height is `HEADER_HEIGHT` plus rows times `ROW_HEIGHT`. `plan/map.md`'s ribbon row stops
   naming captions.
  Done: No caption is drawn for any tool; the circle's list is four rows of names; no
   `Section*` message remains in `crates/text`; the list-height test passes; `cargo xtask ci`
   is green.
  `files: crates/ui/src/toolbars/spec.rs, crates/ui/src/toolbars/list.rs, crates/text/src/messages.rs` — `7d378bd`

- `[x]` **K4** the package learns sheets and money once the catalogue carries stock sizes and prices
  Why: Version 1 of the package carries areas and stock groups but no per-sheet plan and no
   money, because the catalogue stores only nominal thickness and colour: sheet dimensions and
   prices do not exist yet. A cutting list per sheet and a purchase price cannot be invented in
   the writer.
  Shape: The catalogue grows sheet sizes, a stock group and an optional price in integer minor
   units; the exporter adds the additive columns and declares kerf and trim in the manifest
   rules block. The package major version does not move.
  Done: A package carries a per-sheet cutting plan for one material and a price per line; the
   columns are additive; the golden file records them; cargo xtask ci is green.
  now: export: the parts price, the sheets table and the manifest rules and money blocks
  `files: crates/export/src/rows.rs,crates/catalog/src/view.rs` — `2a634da`

- `[x]` **K3** the order package is written from a command line, without opening a window
  Why: A package is useless if producing it needs a window and a person. The report pipeline
   runs over saved files, and the assistant milestone already needs a headless job over a
   project, so the exporter has to be reachable from a command line in the same binary.
  Shape: furnicraft export <scene.fcs> --out <dir> [--format bundle|single] [--no-timestamp]:
   the subcommand is handled before the window starts, prints one line per written table, and
   exits non-zero with a readable message on a bad path, an unreadable scene or an unknown
   format. No second binary, no new dependency.
  Done: A CLI test writes a package from a fixture and checks its manifest and one table; no
   window opens; the help text names the flags; cargo xtask ci is green.
  `files: app/src/main.rs, app/Cargo.toml` — `39ae4c3`

- `[x]` **K2** the order package writer: one manifest and ten flat tables other programs read
  Why: The reason for the whole discussion: reports, drawings and CNC are written outside our
   process, possibly in Python, and neither the document JSON nor the internal database is a
   contract they can rely on (ADR 0095). The package is that contract, and the class traits,
   the product envelope and the exact contours only pay off when they travel with it. It
   follows the product entity (N1) because its products table is the envelope plus product_id,
   and the classes snapshot carries mount and fill.
  Shape: A new crate crates/export at layer 3, registered in the layer map and PROJECT.md, with
   one column table as the only source for the manifest, the JSONL writer and the CSV copies.
   Tables: products, nodes, parts, banding, contours, facades, fittings, machining, materials,
   classes, properties; integer micrometres, millidegrees, square millimetres and
   scaled-integer rotation, no floats. Products carry the authored envelope plus the derived
   content bounds; parts carry product_id, measured sizes, area, banding totals; contours hand
   over the stored segments and arcs without recomputation; the manifest carries the optional
   order number and version. The package embeds the catalogue rows the order uses and the class
   rows it references, so it is self-contained. Ordering by key and no timestamp in the tables
   make one revision produce byte-identical output.
  Done: A golden package from the fixture scene; two exports of one revision are identical; the
   manifest column list equals the writer table and each CSV copy carries exactly those
   columns; an order with two cabinets groups correctly by product_id; cargo xtask layers and
   cargo xtask ci are green.
  `files: Cargo.toml, xtask/src/main.rs, PROJECT.md, AGENTS.md, crates/export, docs/domain/order-export.md, plan/map.md, plan/rename.md` — `b0d1d2f`

- `[x]` **X39** the inspector's last row hides under the horizontal scroller, and the tree scrolls by a different route
  Why: Observation: the right panel's content ends flush with the panel's rounded bottom edge
   and the horizontal scrollbar the inspector needs crosses the last card, so the bottom reads
   as cut off; the left tree uses its own vertical `ScrollArea` and its own margins, so the two
   panels do not agree. Expected: both panels keep the same air at the bottom as at the sides,
   the last row clears the scrollbar, and one helper draws both bodies. Suspected cause:
   `scroll_body` bounds the body to the panel's inner width but reserves nothing at the bottom,
   and `main.rs` gives the tree a plain `ScrollArea::vertical` instead of that helper.
  Shape: `scroll_body` becomes the one scroller of both panels (the tree's private `ScrollArea`
   goes), with shrinking off, the panel's side inset and a bottom inset of one step plus the
   scrollbar's own height reserved, so the last row can always be scrolled clear of it.
   `panel_rect` keeps its geometry; a golden pins the inner content rectangle.
  Done: A headless test measures the inner content rectangle of both panels and asserts the
   bottom inset is at least the side inset plus the scrollbar height; an offscreen shot of the
   longest card and the deepest tree shows no clipped row in either theme; `cargo xtask ci` is
   green.
  `files: app/src/side_panels.rs, app/src/main.rs` — `09f4c02`

- `[x]` **X38** inside an open product a click selects the whole product instead of the part under the pointer
  Why: Observation: with a product opened by a double click, every click in the viewport
   selects the product, while the tree selects and highlights the part under the pointer; an
   open group already behaves correctly. Expected: an open level exposes its children to the
   click, so a click lands on the part, a double click enters one level further and Escape
   leaves. Suspected cause: the open product is `Session::active_product`, view state separate
   from `Editor.open`, and `target::resolve_viewport_pick` reads only the open set, so the walk
   up from the hit node stops at the product container.
  Shape: The rule takes the open product too — one argument to `resolve_viewport_pick`, or the
   editor folds it into the set it passes — so a closed product still picks as one object and
   an open one exposes its parts. The rule keeps its shape: a document id in, a document id or
   `None` out, no mutation.
  Done: A unit test on `resolve_viewport_pick` with an open product returns the part and with a
   closed one the product; a frame test opens the fixture product, clicks a panel and asserts
   the selection and the status bar name the part; `plan/map.md`'s picking row stops claiming a
   click inside a product picks the product; `cargo xtask ci` is green.
  `files: crates/ui/src/target.rs, crates/ui/src/editor/selection.rs, app/src/document.rs` — `cbb6a8b`

- `[x]` **X37** an open method list owns the pointer only across part of its own rectangle
  Why: Observation: with a drawing tool armed, the pointer over an open method list keeps the
   list's cursor across a row's glyph and name, but the tool cursor comes back over the
   right-hand part of the row, where the key label and the pin sit, although the whole 298 pt
   popup is the list. Expected: the popup owns the pointer across its whole width and height —
   header, rows, key column and pin — and the tool cursor belongs to the viewport, never to a
   surface drawn over it. Suspected cause: nothing claims the cursor for the popup's own
   rectangle, so the cursor over it is decided by whichever surface wrote last, and the key
   label and the reserved pin column fall back to what the viewport wrote for the frame.
  Shape: One place decides the cursor for the frame: the list claims its own cursor while the
   pointer is inside the popup rectangle (one rectangle, read from the popup's response, not
   per row), and the viewport reads the chrome before it writes a tool cursor. A headless test
   drives `RawInput` with the pointer at four positions across the open list — the header, the
   name, the key and the pin — asserts one cursor for all four, and asserts the armed tool's
   cursor returns on the frame the list closes.
  Done: The whole open list shows one cursor and the frame after it closes shows the tool's;
   the test fails before the change and passes after; `cargo xtask ci` is green.
  `files: crates/ui/src/toolbars/list.rs, app/src/document.rs` — `caf5f08`

- `[x]` **X36** a split ribbon cell squeezes its main face to 24 pt, so the glyph reads as cut off
  Why: Observation: a cell is `RIBBON_CELL_WIDTH` (34) x `RIBBON_ROW_HEIGHT` (30), and a tool
   with methods splits it into a 24 pt main face and a 10 pt caret (`RIBBON_CARET_WIDTH`), so a
   22 pt glyph sits 1 pt from each edge and the face reads as cut off beside every plain cell.
   Expected: the face of a tool with methods is a full cell, exactly like a tool without one,
   and the caret is added to its right. Suspected cause: `ribbon_split_cell` is handed
   `RIBBON_CELL_WIDTH` and subtracts the caret from it, and `icons.rs`'s own anatomy test
   asserts the 24 pt face.
  Shape: The split cell takes `RIBBON_CELL_WIDTH + RIBBON_CARET_WIDTH`, keeps the main face at
   the full cell and puts the caret outside it, so one row of cells keeps one glyph size and
   one rhythm; the anatomy test, the width assertion and the ribbon's group columns follow. The
   caret stays 10 pt, so its target is unchanged.
  Done: A split cell's main face measures `RIBBON_CELL_WIDTH` and its caret
   `RIBBON_CARET_WIDTH`, the two touch, the anatomy tests assert the new sizes, a row of mixed
   plain and split cells reads as one rhythm in an offscreen shot under `target/ui-shots`, and
   `cargo xtask ci` is green.
  `files: crates/ui/src/icons.rs, crates/ui/src/toolbars.rs, crates/ui/src/theme.rs` — `55f372b`

- `[x]` **U83** the program's own guides get one ink and one stroke: the helper ink
  why: The corner brackets of an open level (U89) and the viewport's own cursor (U88) are the
   first things Z3L draws to help rather than to show the furniture, and the owner asked
   for the colour and thickness rule before more helper geometry arrives. A guide whose colour
   is a literal in a feature module is the defect `cargo xtask ui-rules` exists to stop (ADR
   0063, 0089), and a guide that disappears into the light theme is worse than no guide.
  Shape: `theme::helper()` returns the ink — a warm amber, mixed from the palette so it keeps
   its contrast in both variants of all three presets — and `theme::HELPER_STROKE` the rest
   width; a test pins the contrast against `surface`, `panel` and the viewport background
   (`scene::SceneColors`) at 3:1 or better and asserts the ink is never the accent, the danger
   colour or the selection colour. `scene::SceneColors` carries the same ink for a guide the
   frame draws, resolved once per frame the way the shell resolves it today. The widths are one
   rule: 1 pt for a full-viewport crosshair, 1.5 pt for a bracket at rest, 2 pt for the bracket
   that marks the open level. ADR 0111 records the ink, the widths and the rule that a guide is
   screen space and never document data.
  Done: The ink and the widths exist in `theme.rs`, the contrast test passes in both variants
   of all three presets, ADR 0111 is written and listed in `docs/decisions/README.md`, `cargo
   xtask ui-rules` and `cargo xtask plan` are green, and `plan/map.md` names the ink's home.
  `files: crates/ui/src/theme.rs, crates/scene/src/lib.rs, docs/decisions` — `2fad7f3`

- `[x]` **N16** a changed bevel angle rebuilds the panel body
  Why: The mesh cache's panel inputs compared the outline, size, machining and banding but not
   the end bevels, so a bevel angle change bumped the revision and the evaluator still returned
   the cached body.
  Done: PanelInputs records and compares the bevels, and a test changes an angle and asserts
   the body is rebuilt.
  `files: crates/mesh/src/lib.rs` — `933416a`

- `[x]` **N15** a shaped fill starts its ring at the panel's zero corner
  Why: The niche returns the section in its own angular order, so a shaped panel's first corner
   was not its zero corner; on a board turned about its length axis it sat at the far end and
   read as a second half-turn.
  Done: The ring and its bevels rotate together until the zero corner leads; a test on a turned
   shape asserts vertex 1 is the panel's zero.
  `files: crates/model/src/project/fill.rs` — `93ce0ce`

- `[x]` **N13** the fill cell has one panel kind and a visible build method
  Why: The owner reports that the empty-space tool's menu reads like a choice between two
   panels: the Shape row has no icon and, after picking it, the cell keeps showing the pinned
   Fit, so the choice looks like it did not take. He asks for one panel whose build method
   lives in a secondary menu, with a visible selected state.
  Shape: Keep one cell per panel kind (post, shelf). Put both build methods under a 'Build
   method' caption in the cell's own method list, give the shape row the polygon glyph, and
   always draw the tool's glyph on the cell face so the mode never becomes a second panel.
   While a mode is armed the face shows that method, pressed, with its own hint and its own
   action (a second click drops the tool); the pinned row keeps marking the workspace default.
   Rename the rows 'Largest rectangle' and 'Niche shape' / 'Наибольший прямоугольник' and 'По
   форме ниши'. Record the decision in an ADR.
  Done: Tests: both fill methods carry a glyph and the build caption; the open list draws the
   caption and both rows; the armed shape is shown on the cell while Fit is pinned, keeps the
   post glyph and its click toggles the armed mode; the face keeps the tool glyph for both
   rows; the existing pin and language tests pass; cargo xtask ci is green.
  `files: crates/ui/src/toolbars/spec.rs,crates/ui/src/toolbars.rs,crates/ui/src/toolbars/ready.rs,crates/text/src/messages.rs` — `cb99243`

- `[x]` **N14** the niche is bounded by the faces the pointer sees, not by the envelope
  Why: The owner reports that N9-N12 do not deliver the behaviour: turned panels still blind
   the pointer, an empty carcass with a top that covers only part of the frame is refused ('the
   empty space here is not one straight shape'), and pointing at empty space through a
   world-bounds corner of a turned product answers 'point at the empty space inside the open
   product'. The pointer also gets no say in what it points at: the shell intersects the ray
   with the world-axis box around the product envelope, then hands the niche that entry point,
   and the niche refuses any wall plane that does not cover a whole face of the envelope.
  Shape: Measure from the face the pointer points at: enter the product's own envelope (never
   its world bounds), probe along the view ray to the first surface, and place the measured
   point a micrometre in front of that face; a ray that meets only the far envelope keeps the
   entry point. Let the face plane bound the cell and drop the whole-face coverage requirement,
   so a wall that stops halfway or a top that covers part of the case still leaves a niche;
   keep holds_body for a surface standing inside the cell. Make Shape fall back to the slab cut
   with no end bevels when the wall planes cannot all be met by one flat end face, instead of
   refusing. Record the changed rule in ADR 0109 and mark 0106's coverage clause superseded.
  Done: Tests: a pointer on a wall's inner face measures from that face and the ghost ends at
   it; a turned product is entered through its own box and a ray through the old world-bounds
   corner answers nothing; a niche under a short top and between short walls still fills; a
   wall that stops halfway still yields a full-height niche beside it; Shape under a leaned
   ceiling takes the slab section with no bevels instead of refusing; the splayed and box fills
   keep their answers; the shell's one-click-one-undo tests pass with the ghost captured on the
   press; cargo xtask ci is green.
  `files: crates/model/src/project/niche.rs,crates/model/src/project/fill.rs,app/src/fill_gizmo.rs,app/src/document.rs,docs/domain/products.md,docs/decisions/README.md` — `9773ddc`

- `[x]` **N12** the shaped fill bevels its ends onto the walls, so a board meets a joined panel flush
  Why: This is the point of the bevel: once a part takes the niche's shape, a shelf or a post
   in a niche bounded by boards turned in space must meet those boards with its end face in
   their plane, or it stands with a gap. Fit mode is safe but loose; Shape with a square end
   either cuts in or leaves a wedge of air, which is exactly where the owner saw the failure.
  Shape: The fill plan of N10, in Shape mode, takes the section at the board's reference face B
   and gives each outline edge the bevel its wall asks for: with `n` the wall's free-side unit
   normal in the panel's frame, the wall's rise across the thickness gives `beta = atan(-n_z /
   abs(n_xy))`, positive for the cut that lands the A edge in the wall, and a wall
   perpendicular to the board gives zero and no record. Because a bevel only removes material,
   B must be the wide face: the plan reads the sign of every wall's n_z in the panel's frame,
   turns the board about its length axis when the wide face is the other side, which is a
   placement and not a new stance, and refuses a cell whose walls disagree in sign, where no
   flat board can fill the void. A wall nearly parallel to the board, a floor or a ceiling
   crossing the slab, makes the void a wedge no flat board can fill: Shape refuses with a
   sentence and Fit remains. When no edge needs a bevel and the section classifies as a
   rectangle or a chamfered rectangle, the fill uses the standard path of N10; a beveled board
   is a contour with bevel records. `panel.create` gains an optional bevel list, so a fill
   stays one command and one undo entry; the ghost draws the beveled A rim and the wall it
   meets.
  Done: Tests: a shelf against a wall leaned 10 degrees gets the exact angle and the wall
   plane's distance at both ends of the A edge is zero within a micrometre; the same wall
   leaned the other way turns the board and still meets it; a plumb case gives no bevel and the
   rectangle panel; a wedge void is refused; the trapezoid of two angled posts stays a contour
   with no bevels, since the walls are vertical; the ghost's far rim equals the panel's; a
   click is one undo entry; `cargo xtask ci` is green. A new ADR records the reverse placement
   and the wedge refusal.
  `files: crates/model/src/project/, crates/engine/src/panel_cmds/panel.rs, app/src/fill_gizmo.rs, crates/ui/src/editor/fill.rs, app/src/document.rs, docs/domain/products.md, docs/decisions/README.md` — `925d809`

- `[x]` **N11** a panel's end can be given a bevel angle about its edge
  Why: A board that butts a wall turned in two planes cannot meet it with a square end: its end
   face must lie in the wall's plane. The panel is one region extruded along its thickness, so
   every end face is perpendicular to the two large faces and the only angle it can express is
   in its own plane; a contour cannot hold the skew. The owner named this as the second
   difficulty, and without the parameter the fill can only leave a gap or cut in.
  Shape: `Panel` stores an optional bevel per outline edge: a signed angle in millidegrees
   about the edge's line, tilting the end face across the thickness. The outline is the edge on
   face B; face A's edge is set back by `thickness * tan beta`, so a positive angle removes
   material and the board never grows past its outline, which keeps its nominal size its stock
   size. The record is keyed by a stable source, an `EdgeId` of the nominal rectangle or a
   `ContourSegmentId` of a ring, one per source, line segments only, with the angle capped
   below 89 degrees and the resolved body required to keep one piece and a positive thickness.
   `param` folds one `PlanStep::Difference` per bevel with a wedge cutter: a small triangle in
   the plane across the edge, from the B edge to the A edge, extruded along the segment with
   the usual 1 µm overshoot through `extrusion_plan`; the core region, the analytic rectangle
   path, the cutouts and the other edges' bands are untouched. A beveled edge cannot carry a
   band, because the band would follow the nominal edge, so `panel.bevel.set` refuses a banded
   edge and the banding command refuses a beveled one, each with a sentence. The card gains the
   angle field and a clear action; the commands are `panel.bevel.set` and `panel.bevel.clear`;
   the schema moves to 24 and the goldens follow (ADR 0096); a new ADR records the parameter
   and why the outline is the B face.
  Done: Tests: a 45-degree bevel on a 100 x 50 x 18 panel removes the expected wedge and keeps
   the nominal size; a second bevel on another edge accumulates; an arc segment, a duplicate
   source, a banded edge and an angle at or past the cap are refused with typed errors; banding
   a beveled edge is refused; the card writes and clears the value; a save/load golden carries
   the record and a schema other than 24 is a readable error; one command is one undo entry;
   `cargo xtask ci` is green.
  `files: crates/model/src/panel/mod.rs, crates/model/src/panel_features.rs, crates/model/src/project/panel_cmds.rs, crates/model/src/project/mod.rs, crates/engine/src/panel_cmds/panel.rs, crates/engine/src/panel_cmds/mod.rs, crates/engine/src/commands.rs, crates/param/src/lib.rs, crates/param/src/machining.rs, crates/ui/src/panel_card.rs, crates/text/src/messages.rs, crates/project/tests/golden, crates/project/tests/scene_file.rs, docs/domain/panels.md, docs/decisions/README.md` — `82f61db`

- `[x]` **N10** the fill has two modes and prefers a standard panel when the section allows one
  Why: The owner asks for two behaviours: the part takes the niche's minimal size so it fits
   without cutting in, or it takes the whole shape the walls allow. Today there is one shape
   and no choice, and a post or a shelf is measured only one way. He also asks the program to
   decide whether the shape can be a standard panel with corner cuts or radii before it falls
   back to a contour: a contour panel loses `panel.resize`, the rectangle card and its nominal
   fields (ADR 0042), so the standard path is the one to prefer when it is exact.
  Shape: `Project` answers one fill plan for the class's `Fill` pair, a material thickness and
   a mode, and both modes start from the N9 section. Fit returns the largest rectangle with
   sides on the panel's own axes that lies inside the section, computed exactly from the convex
   polygon's breakpoints, as a nominal rectangle, and a rectangular section gives itself. Shape
   returns the section as the panel outline. Before the panel is chosen, a classifier generates
   the candidate rectangle rings and compares them with the section vertex by vertex within a
   micrometre: the nominal rectangle stays `PanelOutline::Rect`; a rectangle with asymmetric
   corner chamfers, the existing `CornerFeatureKind::Chamfer { incoming, outgoing }` with a
   positive clear length left on every edge, becomes `Rect` with those features; a ring with
   corner radii becomes `Rect` with `Radius` features; only a section that matches nothing is a
   contour. The classifier is a pure model function over a ring, so the radius path is tested
   directly even though the plane-only cell cannot produce an arc until a curved surface query
   exists. The mode is a method of the fill cell through the ribbon's existing split list: the
   primary method is Fit, the variant is Shape, the pinned method is the session's choice, and
   `ToolAction::Fill` carries the mode into `FillArm`. The post and the shelf both offer both
   modes, and the ghost, the distance fields and `panel.create` read the same plan. A section
   that is degenerate, not convex, or has no rectangle of a usable size is refused with a
   sentence.
  Done: Tests: a trapezoid gives the largest rectangle in Fit and the trapezoid ring in Shape;
   a plain box gives the same rectangle in both modes; the fit rectangle lies inside every wall
   over the whole thickness; a rectangle with one asymmetric corner cut becomes `Rect` with one
   chamfer; a rounded ring given to the classifier directly becomes a radius; a section whose
   cut would consume an edge is a contour; the mode survives a tool switch and is the one the
   ghost and the panel carry; a click is one undo entry; `cargo xtask ci` is green.
  `files: crates/model/src/project/, crates/model/src/lib.rs, app/src/fill_gizmo.rs, crates/ui/src/toolbars/spec.rs, crates/ui/src/toolbars.rs, crates/ui/src/toolbars/ready.rs, crates/ui/src/editor/fill.rs, crates/ui/src/editor/ribbon.rs, app/src/document.rs, crates/text/src/messages.rs, docs/domain/products.md` — `114d79e`

- `[x]` **N9** the niche is a convex cell of wall planes, so boards turned in space are measured
  Why: The owner reports that a niche bounded by panels turned out of the vertical gives no
   size and no shape, so neither a post nor a shelf is built; the N7 trapezoid only works when
   every side board stands plumb. Measured on this tree: a back wall leaned 15 degrees answers
   `NicheRefusal::Unrepresentable` at every cursor height, a ceiling leaned 10 degrees is
   measured at the cursor alone (696, 651 and 605 mm of height at y = 20, 280 and 540, so a
   shelf would cut into it), and a product turned 45 degrees is refused with
   `NicheRefusal::Turned` although the section is taken in the product's own frame. The cause
   is the N7 query: four rays along the product's axes are cut with the section plane at the
   cursor's height and the result is extruded along the height, which is exact only while every
   wall is vertical and the floor and ceiling are horizontal.
  Shape: The free space is the intersection of half-spaces. `Project::niche` builds a convex
   cell in the product's frame: it starts from the envelope's six planes, probes from the point
   along every current face normal and every cell vertex (plus the envelope's 26 axis, edge and
   corner directions in the first round), clips the cell by each met face's plane, and repeats
   until a round adds no plane, with a capped loop that refuses instead of spinning. Each
   face's plane must still be a surface over the face's extent, which is the current `covered`
   check run at every face vertex and at the face's middle rather than only at the void's floor
   and ceiling; no other surface's box corner may lie strictly inside the cell, which is what
   refuses a void that holds an obstacle, a notch, or two boards meeting halfway. The query
   reports the product frame's supporting planes (unit free-side normal and offset) and the
   cell's vertices; `local_min` and `size` stay the box around the cell for the callouts, and a
   turned product is measured in its own frame, so the `Turned` refusal is deleted. The fill
   keeps building a board that fits: a shelf takes the cell's section clipped over its
   thickness, each wall's half-plane evaluated at the far face, as a rectangle or a contour,
   and a post takes the same section in its own board plane (the panel's local XY against the
   product's ZY), so a splayed case offers a post again with the depth the slab really has. The
   box cases answer exactly as they do today. A curved boundary is measured by the box around
   its body, which is conservative, so a part fits with a gap and never cuts in; an exact
   curved section needs a surface query that can meet a curved face and is not part of this
   task.
  Done: Tests: a leaned wall gives a cell and a shelf that fits it; a leaned ceiling gives the
   true height over its footprint, not the cursor's; a 45-degree turned product answers instead
   of `Turned`; the splayed pair, the box, the shelf-split and the half-wall cases keep their
   current answers, the half wall still refusing; a post stands in the splayed case with the
   slab's depth; a body standing inside the cell is refused; the ghost equals the created panel
   in every case; `cargo xtask ci` is green. A new ADR records the cell and the refusal of a
   non-convex void, and `docs/domain/products.md` section 4 describes it.
  `files: crates/model/src/project/niche.rs, crates/model/src/project/surface.rs, crates/model/src/lib.rs, app/src/fill_gizmo.rs, docs/domain/products.md, docs/decisions/README.md` — `09a29e0`

- `[x]` **X35** a post stands on the niche's ceiling, so its first corner is at the top and edge 1 is the top edge
  Why: The owner read the corner order off a created post and reported it upside down. The fill
   tool stands the board with from_rotation_y(+FRAC_PI_2), which is the post stance of ADR 0105
   turned a half-turn about the same axis: the panel's length runs downward, its zero corner —
   the origin, a corner of the bottom face — lands on the void's ceiling, and edge 1 is the top
   edge. Nothing looks broken until a band ordered for the top edge arrives on the bottom,
   which is what id-addressed banding and machining exist to prevent.
  Shape: In fill_gizmo::post stand the board in the stance — DQuat::from_rotation_y(-FRAC_PI_2)
   — with the origin at the void's floor corner instead of its ceiling. One detail waits for
   the owner: with the stance the panel's z = 0 face is the face at the translation and the
   thickness runs toward -X, so either the pointer's whole-millimetre plane marks that face
   (body toward -X, the free range starts at minimum.x + gap) or it marks the far face
   (translation x + gap, body toward +X, which keeps today's ghost in its 18 mm band). The
   shelf keeps the pointer's height on its own z = 0 face and grows along local +Z, so the
   first option is the consistent one. niche.rs's angled_post helper builds the same board for
   the probe tests and follows the stance: the face on the through line stays put, the body
   stays behind it, so the rotation flips and the translation loses the thickness offset. The
   height and depth spans do not change.
  Done: A test in fill_gizmo.rs asserts the stance: the ghost's corner 1 is the bottom corner
   of the rear edge, corner 2 is 720 mm directly above it, the panel's zero lies on the void's
   floor, the thickness runs on the chosen side of the pointer plane, and the ghost stays
   inside the void at both ends of the free travel. The niche and fill tests pass with the
   updated helper and cargo xtask ci is green.
  `files: app/src/fill_gizmo.rs, crates/model/src/project/niche.rs, docs/domain/panels.md` — `8abe395`

- `[x]` **P13** the work order may be empty, and the plan says so instead of inventing a task
  Why: The order cannot be emptied: closing or deferring the last task in `## Now` is refused,
   because `STATE: task=` is required and must name the first unfinished task there. A
   milestone that parks its work in `## Next` — which is exactly where R1, K2, K3 and K4 sit
   now — leaves the plan unable to say so, so a session either invents a task or leaves a
   finished one in progress.
  Shape: The empty order becomes a legal plan state: `STATE: task=-` says no task is scheduled,
   and the gate accepts that value only while `## Now` holds no unfinished task and nothing is
   `[~`. `close` and `defer` that empty the order write that value and a `## Next` paragraph
   that starts with `Nothing is scheduled`; `take` from `## Next` claims the head again and
   moves `task=` back to an id. `status`, `next` and the dashboard read the empty state instead
   of failing on a task id of `-`.
  Done: Tests: closing the last task leaves the order empty and the plan valid; deferring the
   last one does the same; `task=-` with an open task in `## Now`, or with a `[~` task, is a
   violation; taking a task from `## Next` restores `task=` and the plan passes; `cargo xtask
   next` and `status` describe the parked work without a panic; cargo xtask ci is green.
  `files: state-contract/src/validate.rs, state-contract/src/cli/write.rs` — `67fd5b9`

- `[x]` **P12** a phase can be declared through the plan tool, not by hand-editing the legend
  Why: Filed from the class/export planning session: `task add --phase K` created the first
   task of a new phase, but the paragraph that declares what each phase letter means
   (plan/BACKLOG.md, "Phases come from the shape-base plan") is not owned by any verb, and
   `cargo xtask file` refuses BACKLOG.md on purpose. The plan therefore has two live phases (T,
   K) the legend does not explain, and the only way to fix it is a hand edit the rules forbid.
  Shape: `task add` takes an optional phase description, or a `task phase <letter> --name
   <text>` verb appends or updates the legend line in one gated write, so a session can declare
   a phase when it opens it and the paragraph can never drift from the ids in the file. A test
   covers adding a phase that the legend does not mention and updating one that it does.
  Done: A new phase can be declared without editing plan/BACKLOG.md by hand; the legend names
   every phase letter that a task id uses; cargo xtask plan and cargo xtask ci are green.
  `files: state-contract/src/cli/write.rs, plan/BACKLOG.md` — `4bb43e5`

- `[x]` **V3** the frame joins the box editor: corner and face grips, and the drawn creation gesture
  Why: The owner wants the frame created and edited exactly like the box, not with a separate
   toolkit: the reserved space and the solid are the same three lengths. Without this the frame
   is card-only while the box has grips, and the two owners of a box disagree.
  Shape: The frame plugs into the box editor of V2 as its second owner: eight corner grips and
   six face grips edit the envelope through product.envelope.set, a corner changing two sizes
   and a face one, the card keeping the three numbers and the type. Dragging a min face
   translates the placement so the opposite face stays put in the world (0082, 0088). The drawn
   gesture — base rectangle, then height — dispatches product.create, so the card and the draw
   reach the same command. The grips show whenever the frame is selected, including the
   closed-with-parts state where its dashed lines are hidden (0100). Distances typed in the
   fields during the gesture land with N8.
  Done: Tests: a corner grip changes two envelope sizes as one undo entry; a face grip changes
   one; a min-face drag keeps the opposite face still; the drawn gesture creates a frame of
   exactly the ghost size with the placeholder class; the card and the gesture produce the same
   document; the grips are checked offscreen and the drag feel by a live run; cargo xtask ci is
   green.
  `files: app/src/box_gizmo.rs, crates/ui/src/box_tool.rs` — `6f8b62c`

- `[x]` **U82** the editor opens empty: a new scene carries no demonstration panels, edges or groups
  Why: The startup document is the maker's work surface. Three sample panels, two cutouts,
   three edges, a group and a drawer teach nothing after the first run and stand in front of
   the first real piece; a maker who wants a piece now draws one with the frame and box tools.
  Shape: The shell builds the empty session at launch instead of the demonstration one: a root
   container named «Scene» and the default catalogue, which is not scene data and stays. The
   demonstration document survives only where a test needs something to draw, so it becomes a
   test fixture with a doc comment that says so, and the imports and doc lines that exist only
   for it go with it.
  Done: Tests: launching the editor yields a document with no children and a frame that draws
   nothing, asserted through the function the launcher calls; the fixture still evaluates in
   the tests that use it; cargo xtask ci is green.
  `files: app/src/scene_build.rs, app/src/main.rs` — `03a076b`

- `[x]` **V2** the cube becomes a cuboid: drawn base, pulled height, edited by corner, face and card
  Why: The only solid primitive is a cube with one size for three edges, which nothing in
   cabinet making uses. The owner asked for a parallelepiped with independent width, depth and
   height, created the way the extrusion tool already works: draw the base, then pull the
   height. ADR 0101 makes it the shared box the product frame will use too.
  Shape: PrimitiveKind::Cube and its single size are deleted; Cuboid carries width, depth and
   height, each validated by the existing primitive bounds, and the mesh builder derives the
   six faces from them. The ribbon cube cell becomes the box tool: draw the base rectangle in
   the active work plane in 1 mm steps, fix it, pull the height with the wireframe-prism
   preview through the session overlay (ADR 0093), and commit one object.primitive.create. The
   box anchors at its base min corner, so its local zero is the corner of the rectangle, and a
   drag on a max face changes one size while a min face changes it and compensates the
   placement. Cube leaves the message table, the icon set, the scene tree, the fixtures and the
   golden file in the same change; the schema is bumped with no migration (0096).
  Done: Tests: a cuboid with three different sizes builds the expected mesh and bounds; the
   drawn base and pulled height produce exactly the ghost size; a corner grip changes two sizes
   and a face grip one, each one undo entry; a min-face drag keeps the opposite face still in
   the world; cylinder and sphere still load and build; the ribbon cell, the icon and the
   messages no longer mention a cube; cargo xtask ci is green.
  `files: crates/model/src/primitive.rs, crates/mesh/src/primitive.rs, crates/engine/src/object_cmds.rs, crates/ui/src/toolbars.rs` — `ce8ce93`

- `[x]` **N8** the exact-distance fields: type a distance, lock it, centre it, with the keyboard
  Why: A maker cannot place a post to the millimetre with a mouse, and the same need appears in
   three gestures: filling a niche, drawing a box or a frame, and moving a piece. The probe
   exists (N6, ADR 0103); without the fields it is a measurement nobody can state exactly.
   Studied as behaviour in an established editor, the mechanism is a live distance field per
   direction with one active field — not a dialog.
  Shape: A session overlay draws one shared number field per direction of the work plane from
   the new face to the first surface the probe found, with the unit outside (ADR 0092); exactly
   one field is active and shows the focus ring from the theme. Typing takes the mouse out of
   the gesture for that direction, Enter commits, Escape returns the mouse, Tab moves to the
   opposite direction. A field offers the semantic value «По центру» / Centred in its menu,
   which places the part in the middle of the void and yields a computed coordinate. A
   committed distance shows a lock and the next is measured from the locked face. A direction
   with no neighbour disables the field with a sentence; an impossible value is refused with a
   message and writes nothing; the whole gesture is one undo entry. The overlay is shared by
   the niche fill, the box gesture and the move tool, and the numbers come from the model
   probe, never from a second computation.
  Done: Tests over the pure parts: the active field, Tab, Enter and Escape move focus and
   commit or abandon without a write; a lock makes the second distance relative to the locked
   face; the centred value produces the midpoint coordinate; a missing neighbour disables the
   field; an impossible value is refused. The overlay is checked offscreen for the focus ring
   and the lock glyph, and a live run for the typing feel; cargo xtask rules and ci are green.
  `files: crates/ui/src/scrub.rs, app/src/main.rs, crates/ui/src/theme.rs` — `168362e`

- `[x]` **N7** a shaped niche: an angled pair of posts gives a trapezoid, and the shelf takes its outline
  Why: The owner named the trapezoid case as part of the feature, not as a later refinement:
   two posts standing at an angle reserve a trapezoid, and a shelf in it must take that shape
   instead of a box that does not fit. The six probes already report faces, so the outline is a
   consequence of the query, not a new concept (ADR 0099).
  Shape: The niche query returns the probed faces as well as the distances, builds the
   cross-section of the free space in the vertical plane facing the viewer and extrudes it
   along the depth into a geom::Region, so a slanted boundary becomes a polygon and not a
   bounding box. The shelf fill takes that region as the panel outline through the existing
   contour conversion (ADR 0042), and the ghost shows the polygon before the click. A piece
   whose free section cannot be described by one extruded polygon is refused with a sentence
   rather than approximated.
  Done: Tests: two angled posts give a trapezoid region with the expected vertices; a shelf
   created in it has the contour outline, not a rectangle; a rectangular case still produces
   the rectangle through the same path; an unrepresentable section is a readable refusal; the
   preview and the created panel agree; cargo xtask ci is green.
  `files: crates/model/src/project/niche.rs, crates/model/src/project/surface.rs, app/src/fill_gizmo.rs, crates/engine/src/panel_cmds/panel.rs, crates/ui/src/editor/creation.rs` — `0efd3b4`

- `[x]` **N6** the empty-space tool: a post or a shelf follows the cursor and is created to fit the void
  Why: This is the owner entrance to the whole product idea and, after the wall cabinet, the
   feature the program is built around: the niche is the space the frame reserves, a post is
   cut to it and a shelf spans it. The query has to exist in the model, because the tool, a
   drawing and a later report must agree on what the void is, and because a shelf crossing the
   piece must split one niche into two without anything being stored.
  Shape: The model answers the measurement (ADR 0103): probe(point, direction) reports the
   distance to the first furniture surface and what it hit — a panel face, a hardware body,
   another product shell or the envelope virtual face — and niche(point) aggregates the axes
   into a size, a world position and a shape (a box today). Both are pure document queries,
   derived and never stored, tested without a UI. The tool is bound to a class whose fill trait
   says which axis pair it takes — HeightAndDepth for a post, WidthAndDepth for a shelf — and
   draws the ghost through the session preview overlay (ADR 0093); it moves in 1 mm steps and a
   click dispatches one panel.create with exactly the ghost size and placement, so history
   holds one entry. Only the open product offers niches. The exact-distance fields and the
   centred value land in N8.
  Done: Headless tests: a probe stops at the envelope face, at a panel and at another product
   shell, and reports which; a niche shrinks around an existing part on the right axis; a shelf
   at full width splits the piece into two niches and the query above and below returns the two
   boxes; a niche reports a world position and a shape, not only three lengths; the ghost size
   equals the created panel size; a click is one undo entry; the tool refuses when no product
   is open; only classes with a fill can be chosen; the ghost is checked offscreen and the 1 mm
   feel by a live run.
  `files: crates/model/src/project/niche.rs, crates/model/src/project/surface.rs, crates/ui/src/editor/fill.rs, app/src/fill_gizmo.rs, app/src/document.rs` — `13a42ad`

- `[x]` **N5** the furniture move tool: floor-bound or free by class, no interpenetration, with snapping
  Why: The current translate tool moves anything anywhere, which is right for geometry and
   wrong for furniture: a floor cabinet must not fly, and two pieces must not pass through each
   other. The mount trait exists for exactly this and today has no consumer.
  Shape: A furniture move tool reads mount from the product class: Floor constrains the drag to
   the floor plane (the vertical is fixed at zero), Wall stays free. Collision follows ADR
   0102: a body stops at contact with another body and a shell stops at contact with another
   shell, so no product is half inside another envelope while containment stays legal. Snapping
   to the floor, to the frame virtual faces of the product and its neighbours, to a
   neighbouring part face and to the 1 mm grid, with the constraint applied to the preview and
   one node.placement.set on release, so a drag is one undo entry. The drag reads the probe
   (ADR 0103) for contact and snap targets; typed distances during the drag arrive with N8.
  Done: Tests over the pure constraint math: a Floor product cannot change height; a Wall
   product can; a drag that would intersect another product stops at contact; snapping lands on
   the grid and on a face; one undo restores the whole drag. The frame and the class attributes
   drive the behaviour without a code literal check.
  `files: app/src/translate_gizmo.rs, app/src/gizmo_math.rs, crates/model/src/project/move_query.rs` — `1c82740`

- `[x]` **N4** the creation guard: a furniture part is created only inside the open product
  Why: The rule "no furniture part outside an open product" is worth nothing if a palette
   entry, a shortcut or a future assistant can bypass the button that enforces it. ADR 0098
   therefore puts the guard in the command enabled check, where every dispatch path meets it.
  Shape: panel.create, facade and fitting creation read session.active_product and the class
   level: a class of level Part or Fitting is refused when no product is open or when the
   target parent is not inside it, with one of two messages and no write. Primitives, drawn
   figures, mesh objects and hardware models stay free. Every fixture and test that created a
   panel under the root gains a product and an opened session, or asserts the refusal; the
   testkit fixture grows a product so the mechanical change stays in one place.
  Done: Tests: refuse with no product open and with a closed product; refuse when the parent is
   outside the open product; allow when it is inside, including a nested group; a primitive
   still creates without a product; nothing is written on a refusal and the history is
   untouched; cargo xtask ci is green.
  `files: crates/engine/src/class_rules.rs, crates/engine/src/commands.rs, crates/engine/src/session.rs, crates/engine/src/panel_cmds/panel.rs, crates/engine/src/object_cmds.rs, crates/text/src/messages.rs, crates/model/src/testing.rs, crates/testkit/src/lib.rs, crates/ui/src/editor/tests.rs` — `187d42e`

- `[x]` **N3** one product is open at a time, opened by a double click and never written to the file
  Why: The owner ruled that one product is open at a time and that a furniture piece is created
   through a card and is open the moment it exists: press add frame, enter the dimensions, and
   the next part goes inside it without another gesture. Without this the creation guard has
   nothing to point at and a new project has no first step.
  Shape: Session carries active_product: Option<Id>, opened and closed by view operations,
   cleared per tab and never serialised; opening another replaces the previous one, and
   product.create (N1) opens the new frame in the same user action — a command followed by a
   session operation, never an open flag in the file. The ribbon gains one cell, add frame,
   which opens a card: width, depth and height in 1 mm steps and a searchable list of product
   classes defaulting to the placeholder. Placement is deterministic: the origin in an empty
   scene, otherwise beside the nearest product on the floor, clear of every envelope, and
   selected. The double click in the viewport and the click on the tree both open; nothing here
   dispatches a document command. The card is the numeric path; the drawn gesture (rect then
   height) reaches the same command and arrives with the box tool (V3), so this task does not
   wait for it.
  Done: Tests: adding a frame opens it and the next panel lands inside; in an empty scene the
   frame is at the origin, otherwise beside the nearest product and clear of every envelope;
   opening A then B leaves B open; closing leaves none; the state is not in the saved file; a
   double click on a part does not open a product that does not contain it; each tab keeps its
   own open product; the card is driven through an egui context; cargo xtask ci is green.
  `files: crates/engine/src/session.rs, crates/ui/src/scene_tree.rs, app/src/main.rs` — `5f5a232`

- `[x]` **N2** the product frame is drawn as a dashed wireframe that never picks
  Why: The envelope is useless if nobody can see it and nobody can click it: the owner
   described the frame as a dashed wireframe that is picked to select, open and move the piece,
   and the picture has to say whether the piece is open, closed and empty, or closed with
   parts. It also has to be visibly not a body, or it will be grabbed by mistake.
  Shape: The scene description gains one frame item per product — the envelope in world space,
   lines only; the renderer draws it dashed with no fill after the solid pass, with no depth
   write. Three states per ADR 0100: open and active in the active line colour, closed and
   empty in the inactive line colour, closed with parts not drawn at all. Picking uses the
   envelope, not only the lines: a single click on a line or on empty space inside the piece
   selects the frame, a click that hits a solid picks the solid, and when several products
   contain the point the innermost wins. The shell rebuilds the item with the rest of the scene
   and keeps it out of the solid pipelines.
  Done: An open product draws active dashed lines, a closed empty one inactive dashed lines, a
   closed one with parts draws none; a click inside empty space selects the frame, a click on a
   panel selects the panel, a click on a nested product selects the inner one; a click through
   the frame behind it still reaches what is behind; the colours come from the theme and are
   checked offscreen; cargo xtask ci is green.
  `files: crates/scene/src/lib.rs, crates/render/src/lib.rs, app/src/scene_build.rs` — `36d4dde`

- `[x]` **N1** the product entity: a container with a class and an authored envelope, created by a command
  Why: Reports, the export and the creation guard all need the same answer — which piece is
   this part in and how big is that piece — and today nothing in the document carries it. ADR
   0097 settled the shape: a product is an ordinary container with a class of level Product and
   an authored envelope, not a new node kind.
  Shape: Container gains an optional Envelope (three micrometre lengths) with validation, and
   the model refuses the two mismatches: an envelope on a container whose class is not level
   Product, and a product class without one. node.duplicate copies it,
   node.component.convert_to_group clears it with the class, and a product is never a
   descendant of a product. product.create takes parent, name, class, envelope and placement,
   and assigns the registry placeholder «изделие общего назначения» when no class is given, so
   a product always has one. product.envelope.set edits the box and never a child size.
   Accessors product_of(id) and products() walk the tree once; parent_id and product_id stay
   uncomputed in the document. The schema is bumped; ADR 0096 means no migration, only a
   regenerated golden file.
  Done: Tests: create a product with an envelope, and one with the placeholder class; refuse an
   envelope without a product class and a product class without an envelope; refuse a product
   under a product; duplicate copies the envelope; convert_to_group drops both; a bad envelope
   is a typed error; product_of returns the nearest product for a shelf two levels down; cargo
   xtask ci is green.
  `files: crates/model/src/container.rs, crates/model/src/class.rs, crates/model/src/project/product_cmds.rs, crates/model/src/project/query.rs, crates/model/src/project/property_cmds.rs, crates/model/src/project/mod.rs, crates/engine/src/product_cmds.rs, crates/engine/src/commands.rs` — `a049094`

- `[x]` **T4** class traits gate the editor: what the class forbids, the command and the gizmo refuse
  Why: Traits only matter if the program refuses what the class forbids. Without this the class
   table is documentation: a worktop still stretches across its width, a hinge still offers
   resize, and the cutting list still has no flag to read.
  Shape: The enabled check reads extent, operations and host: panel.resize and the mesh
   commands refuse a LengthOnly or None class on the forbidden axis, edge banding refuses a
   class without EdgeBand, cutouts refuse a class without Cutout, and mounting checks host. The
   gizmo offers free, length-only or no handles from the same accessor. One accessor exposes
   nesting and measure for the cutting list and the export that follow.
  Done: Tests: a worktop resizes by length only, a hinge offers no resize and no banding, a
   fitting never enters nesting, and changing a class row flips the behaviour without touching
   the command; no code path compares a class literal outside a test.
  `files: crates/engine/src/class_rules.rs, crates/engine/src/commands.rs, crates/engine/src/node_cmds.rs, app/src/gizmo_math.rs` — `173acb4`

- `[x]` **T3** the inspector assigns a class from the registry tree instead of typing six digits
  Why: docs/domain/classes.md §6 says the inspector assigns a class from the registry tree;
   today it edits a raw GG.TT.VV text field, so a saved project can hold a code the registry
   never heard of, and nothing shows a user what the digits mean.
  Shape: A picker per the ui-design skill: one search field over label, slug and alias, the
   tree group to type to variant, retired rows hidden, an explicit unclassified row, and the
   current class marked. The scene-tree badge shows the label with the code in the tooltip; the
   property card shows a class row instead of a text edit. No new command: assignment stays
   node.property.set with the reserved key.
  Done: A click assigns a class and one undo entry restores the previous one; searching полка,
   shelf and the alias finds the same row; a retired class cannot be assigned but still
   renders; the picker state is tested through an egui context and the result is checked
   offscreen.
  `files: crates/ui/src/properties_card.rs, crates/ui/src/scene_tree.rs` — `fcc5041`

- `[x]` **T2** the class registry is data in the model, and the program can see what it is looking at
  Why: Today the model validates the shape of a code (group non-zero, a variant needs a type)
   but not its meaning: 20.01.09 is accepted although no such type exists, and no algorithm can
   ask what an object is. The class registry turns the code into something the program can see
   with, and ADR 0094 is the accepted design.
  Shape: furntype.rs becomes the class module: ClassCode (six digits, three levels, 000000
   unclassified), TraitSet as a bit mask, and a const registry table whose inheritance is
   resolved at compile time so a lookup never walks parents. PropertyValue::FurnType becomes
   PropertyValue::Class under the reserved key class, query.rs renames with_furntype_under to
   with_class_under, assignment rejects a code the registry does not know while loading keeps
   an unknown number. A test compares the registry with the table in docs/domain/classes.md.
   The schema is bumped because the stored serde tag changes, and ADR 0096 means no migration.
  Done: Tests: lookup and inheritance per level; a contradictory override is rejected; an
   unknown code is refused on set and kept on load; the document table equals the registry row
   for row; no code literal is compared outside a test; cargo xtask ci is green.
  `files: crates/model/src/class.rs, crates/model/src/properties.rs, crates/model/src/query.rs` — `de739a0`

- `[x]` **K1** the document format loses its past: migrations, aliases and older golden files are deleted
  Why: Measured 2026-11-12: nine golden files scene-v7..v20, two tree-walking migration passes,
   serde aliases for renamed fields, a separate error path for a newer schema, and a
   version-by-version history in two domain documents — all of it protecting files no user has
   ever saved. ADR 0096 makes the rule explicit and this is the commit that spends it.
  Shape: Delete the migration functions and their tests, the older golden files, and every
   serde alias that exists only for a previous field name. Keep one schema_version, one golden
   file for the current schema, and one readable error for any other number (older or newer).
   The envelope keeps format_version as it is. docs/domain/scene-file.md §7 and
   docs/domain/document.md §8 already state the rule; this task makes the code match them. A
   default stays only where the field is optional in the current layout.
  Done: cargo test -p model -p project is green with one golden file; a
   file with any other schema number is rejected by the same sentence; rg finds no migration
   pass and no alias for an older name; the two domain documents no longer describe a
   migration; cargo xtask ci is green.
  `files: crates/model/src/project/wire.rs, crates/project/tests/scene_file.rs, crates/project/src/lib.rs, docs/domain/scene-file.md` — `10b1079`

- `[x]` **T1** the class table is written down and approved, so the registry has content before it has code
  Why: ADR 0094 fixes the mechanism, not the content: a registry without rows is a shape with
   nothing to say, and every later task reads the table (the model, the picker, the trait
   gates, the export). Until the rows are written and approved, none of them can start without
   inventing classes in code.
  Shape: docs/domain/classes.md grows the full tree: groups 10 seating, 20 lying, 30 storage,
   40 work and dining surfaces, 50 fittings, 60 appliances, 70 boards and trim, with 80 and 90
   reserved, plus the editor placeholder row «изделие общего назначения» / general product
   (level = Product, never offered for a part) that product.create assigns when the card names
   no type. Every type and variant carries its traits (supply, measure, nesting, extent,
   operations, host, level, opening, mount, fill) and its slug; TT 90..99 stays reserved for
   local codes. Product rows get level = Product and a mount, part rows get a fill, and a
   product class is named by its function (a wall cabinet is 30.01.02), never by a product code
   range: 0094 removed that axis. Each row must earn its code by a rule difference or a
   separate report line, and the hinge example (card versus spring, then
   overlay/inset/half-overlay) is the worked case. furntypes.md and PROJECT.md point at the new
   table; the old registry stays only as history.
  Done: The table covers every family the milestone needs; every row names all ten traits it
   applies to; the product rows carry level = Product and mount, the part rows carry fill; the
   placeholder product row exists and is never offered for a part; a fixture set (cabinet,
   shelf, side, back, door, spring hinge, runner, worktop, plinth) resolves against it and
   shows the intended report and editor behaviour; the owner has approved the rows entry by
   entry.
  `files: docs/domain/classes.md, docs/domain/furntypes.md, PROJECT.md` — `92276c7`

- `[x]` **U81** the interface renders offscreen to a PNG, so colour, contrast and translucency are checked without a window on the owner's screen
  Why: The eyes-only list in .pi/skills/ui-design/references/checklist.md — the light theme, a
   55 % panel over the model, 200 % scale — cannot be answered from rectangles, and today the
   only way to answer it is to start the app while the owner is working. An offscreen render
   into target/ui-shots answers it silently and leaves a picture the session can read.
  Shape: After U77's seam: a wgpu instance and adapter with no surface, a device and queue,
   egui_wgpu::Renderer on the app's own frame, a texture with RENDER_ATTACHMENT and COPY_SRC,
   copy_texture_to_buffer and a PNG writer under target/ui-shots. One ignored test renders the
   matrix: three presets, dark and light, panel opacity 55, 92 and 100, panels expanded and
   folded, 100 and 200 % scale, on the same fixture scene. No adapter prints one sentence and
   skips, so the check is safe on a machine without a GPU; the PNGs are looked at, never
   compared pixel by pixel in CI.
  Done: A run writes one PNG per cell of the matrix into target/ui-shots with a name that says
   the cell; a machine without an adapter skips with a readable sentence; the image at 55 %
   shows the model through the panel and the light image is not washed out; cargo test
   --workspace and cargo xtask ci stay green without a GPU.
  `files: app/src/main.rs, crates/render/src/lib.rs, crates/ui/src/theme.rs, crates/testkit/src/lib.rs` — `b370740`

- `[x]` **U80** cargo xtask ui-rules fails a literal colour and a raw egui host outside the module that owns it
  Why: theme.rs owns the colours and three modules own the overlay hosts, but nothing checks
   it: a literal Color32 in a card looks right in the dark palette and breaks the light one,
   and a private egui::Window escapes Chrome::covers and the one overlay order, then swallows a
   click that should have been a pick. Review catches this sometimes; a parse catches it every
   time.
  Shape: A new xtask/src/ui_rules.rs that parses the source with syn rather than grepping text,
   so an alias cannot slip past. Refused outside crates/ui/src/theme.rs: Color32::from_rgb,
   from_rgba*, from_gray, the named constants and a raw struct literal. Refused outside a table
   of owners (the shell's side_panels, status_bar, view_cluster, strip and ui's palette,
   settings, home): egui::Area, Window, Panel, SidePanel, TopBottomPanel, CentralPanel, Popup.
   The table is a const in the check, so adding a surface is one row and no allowance comment.
   Join cargo xtask ci after layers.
  Done: Tests: a literal colour in ui/panel_card.rs fails and the message names file and line;
   a host in a card fails; theme.rs and every owner in the table pass; an alias (use
   egui::Color32 as C) is still caught; the real tree passes; cargo xtask ci runs the step.
  `files: xtask/src/ui_rules.rs, xtask/src/main.rs, crates/ui/src/theme.rs, crates/ui/src/icons.rs, crates/ui/src/panel_card.rs, app/src/side_panels.rs` — `7d78778`

- `[x]` **U79** cargo xtask skills keeps the project skills honest: every path, command and sibling they name exists, and the frontmatter is what the loader needs
  Why: A skill that names a file or a `cargo xtask` command that is gone sends the next session
   to nothing, and the router line in `AGENTS.md` §1.3 is the only thing that makes the design
   skill load reliably. The move to `.pi/skills` also showed that pi parses frontmatter as
   strict YAML: an unquoted colon in a description silently loaded no skill at all, and nothing
   in the gate noticed.
  Shape: A new `xtask/src/skills.rs` in the shape of the other checks. It reads `AGENTS.md`,
   `.pi/skills/**/*.md` and `.pi/prompts/*.md` and fails with a sentence when: a path named in
   backticks or as a markdown link does not exist; a `cargo xtask <name>` is not a command of
   the tool; a `references/*.md` a skill names is missing; the frontmatter `name` is not the
   folder name; `description` is missing, empty or unquoted; a `SKILL.md` is over 200 lines or
   a reference over 150; `AGENTS.md` §1.3 does not name the design skill. Tests use fixture
   trees, one per refusal, and the real tree must pass. Join `cargo xtask ci` after `docs`.
  Done: Each refusal has a test that names the offending file; the real tree passes; cargo
   xtask ci runs the new step and fails when a path in a skill is renamed away.
  `files: xtask/src/main.rs, AGENTS.md, .pi/skills/ui-design/SKILL.md, .pi/prompts/ui-review.md` — `5ff6bb7`

- `[x]` **U78** the shell's frame is checked without a window: the scene rectangle, the layer order, Chrome::covers and the keys are asserted on the real app
  Why: U66 and U72 are accepted with sentences like "the panels narrow no view", "a click on a
   panel selects nothing" and "both themes checked by eye"; each of those is re-verified today
   by starting the app on the owner's screen. After U77 the same frame runs in a test, so the
   sentence becomes an assertion and a regression cannot come back quietly.
  Shape: A test module beside the shell (app/src/frame_tests.rs, declared from
   main.rs) with one helper that builds the app headless and drives RawInput. Cover: the scene
   rectangle and the active camera aspect are identical with both panels expanded, folded and
   hidden and with the ribbon open and folded; a click on a panel is chrome and starts no pick,
   no border drag and no camera gesture, while a click just outside selects; two-column and
   quad tile the view rect exactly and their dividers stay grabbable; a modal owns the keys, so
   no save, undo, reframe or tool change happens under it; no fold, hide or camera move writes
   a revision. Keep the assertions on public shell facts, not on private fields.
  Done: Every listed assertion is a test that fails when the rule is broken, proven by
   temporarily reverting one line of Chrome::content and one of the key handler; cargo xtask ci
   green.
  `files: app/src/frame_tests.rs, app/src/main.rs, app/src/document.rs, app/src/shortcuts.rs` — `04e6a7d`

- `[x]` **U77** the shell's one frame is callable without a window, so a session can check the real interface silently
  Why: Z3LApp implements eframe::App::ui and holds render_state: egui_wgpu::RenderState,
   so only the running window can drive the real interface: the checks in crates/ui exercise a
   card or a field in isolation, and a shell rule — the scene rectangle, the layer order,
   Chrome::covers, a key while a modal is up — is verified by starting the app. The owner wants
   those checks to run while he uses the machine.
  Shape: Move everything App::ui does today into an inherent Z3LApp::frame(&mut self,
   ui: &mut egui::Ui); the trait method delegates and ignores its Frame. Make the GPU handle
   optional, so a test builds the app with no device, no adapter and no window and the paint
   callback is skipped instead of registered. Give SettingsStore a constructor on an explicit
   path, so a test never reads or writes the workspace file in the OS config directory; build
   the document, the tabs and the settings from testkit fixtures. No behaviour change in the
   window: the same frame, the same order.
  Done: Tests: a headless egui::Context runs frame twice and the second frame is stable (no
   panic, no document revision); the app constructs and runs with no GPU and no window; the
   workspace settings file is byte-identical after a headless run; the window still opens and
   draws by eye. cargo xtask ci green.
  `files: app/src/main.rs, app/src/tabs.rs, app/src/settings_store.rs, crates/testkit/src/lib.rs` — `d6d0a4b`

- `[x]` **U76** the ribbon holds tools only: the polygon sides, primitive size and extrude height fields leave the tool row and their typed values become session defaults
  Why: Three `CellSpec::Field` cells take two 34 pt cells each and turn a row of icons into a
   form; `show_field` draws a `TextEdit`, the state carries three strings, and `FieldRefusal`
   exists only to explain a bad field. The object card already edits a polygon sides and a
   primitive parameters, and the extrude pull has its own numeric popup. The owner wants no
   value fields in the toolbar.
  Shape: Delete the three `CellSpec::Field` entries, `FieldSpec`, `FieldKind`,
   `CellSpec::Field`, `show_field`, `FieldSpec::COLUMNS` and `field_text_mut`;
   `GroupSpec::cells` becomes a tool list (or keeps one tool variant) and `columns()` counts
   live tools. Keep the three values as typed session defaults on `Toolbars` (`primitive_size:
   Length`, `extrude_height: Length`, `polygon_sides: u32`) initialized from the existing
   constants; `Editor::set_extrude_height` updates the typed default instead of a text. `ready`
   always sees a valid value, so `FieldRefusal` and its messages go. The object card stays the
   editor for polygon sides and primitive parameters; the extrude pull and its popup stay the
   editor for a height. If a value loses its only editor, record it in `plan/debt.md` rather
   than keeping a ribbon field.
  Done: Tests: no tab draws a `TextEdit` or a numeric cell; `GroupSpec` columns and the tab
   width count tools only; a polygon is created with the default side count and its sides can
   be changed in the object card; a primitive is created at the default size and its parameters
   can be changed in the card; an extrusion uses the default height and the pull still changes
   it and makes that the next default; no `FieldKind`, `FieldRefusal` or field message remains;
   `cargo xtask ci` green.
  `files: crates/ui/src/toolbars.rs, crates/ui/src/toolbars/spec.rs, crates/ui/src/toolbars/state.rs, crates/ui/src/toolbars/ready.rs, crates/ui/src/editor/ribbon.rs, crates/ui/src/object_card.rs, crates/text/src/messages.rs` — `81f9e6b`

- `[x]` **U75** the ribbon group caption has air above and below and sits one quiet step below the control tone instead of on the darkest chrome
  Why: The caption band is 12 pt with a 9.5 pt face, so the capital text touches the rule and
   the tool row, and `theme::band` is `extreme_bg_color`, the darkest chrome, which makes the
   strip read as a hole. The owner wants a calmer band, slightly darker than the buttons.
  Shape: Raise `RIBBON_CAPTION_HEIGHT` from 12 to 16 pt so the 9.5 pt capitals have at least 3
   pt above and below; update `toolbars::height` (open 72 to 76, folded stays 29),
   `plan/map.md`, the U68 metric note and every golden that names 72. Add
   `theme::caption_band(ui)` as a small mix of `control(ui)` toward `band(ui)` (about 30 %) and
   use it for the group caption instead of `theme::band`; keep `text_dim` and the hairline. The
   tool row stays 30 pt and no tool moves.
  Done: Tests: the caption is optically centred with at least 3 pt clearance at 9.5 pt;
   `caption_band` is darker than `control` and lighter than `band` in every preset x variant,
   and `text_dim` on it clears 3:1; `height(true)` is 76 and `height(false)` is 29, and the
   drawn strip matches; the tool row still measures 30 pt; both themes checked by eye on all
   five tabs.
  `files: crates/ui/src/toolbars.rs, crates/ui/src/theme.rs, plan/map.md` — `154b007`

- `[x]` **U74** a tool with variants is a split button: the main face runs the pinned method, a joined caret face opens the list, and pinning changes the main face without covering a shortcut
  Why: U67 made the whole 34 x 30 cell open the list when a tool has variants, so the common
   method needs two clicks and the glyph never says which method a click runs. The pin is a 20
   x 12 control in the same right-to-left row as the shortcut, so it covers the key label, and
   pinning leaves the main cell unchanged. The owner wants two independent faces inside one
   button.
  Shape: Split the cell into two hit rectangles inside the same 34 x 30 control: a main face
   that runs `pinned_index` and a caret face of at least 10 x 10 that opens or closes the list.
   Draw one resting fill and let hover, press and the armed tint apply to the face under the
   pointer; no gap between the faces. The main face draws the pinned method icon when it has
   one, otherwise `tool.icon`, and its hint names the pinned method and its shortcut; the caret
   hint names the list. A main-face click while the list is open runs the pinned method and
   closes it; the caret only toggles. Keep the keyboard rules. In `list.rs`, reserve fixed
   columns `[glyph][label with ellipsis][shortcut][pin]`, with at least 6 pt apart, so the pin
   never overlaps the shortcut. The pin is not drawn at rest: it appears on the row under the
   pointer and on the row the keyboard highlights, while its column stays reserved so the label
   and shortcut never shift; the pinned row keeps its accent bar and fill as the persistent
   marker, and the pin hint says it becomes the main method. Pinning writes
   `ribbon.pinned_variants` and redraws the main face at once; the list may stay open under the
   new accent.
  Done: Tests: a main-face click on a multi-method tool runs exactly the pinned method and
   opens nothing; a caret click opens the list and runs nothing; the caret closes an open list;
   after a pin, a main-face click runs the new method and the cell draws its icon and hint; a
   pinned method without artwork falls back to the tool icon; pin and shortcut rects do not
   intersect on any row in either language; the pin is not drawn on an idle row, appears on
   hover and on the keyboard-highlighted row, and its reserved column keeps the label and
   shortcut from moving; the pinned row stays marked when the pin is hidden; Escape, outside
   click and tab change still close; opening, caret and pin write no document revision. Both
   themes and 100 and 200 % scale checked by eye.
  `files: crates/ui/src/toolbars.rs, crates/ui/src/toolbars/spec.rs, crates/ui/src/toolbars/list.rs, crates/ui/src/toolbars/state.rs, crates/ui/src/icons.rs, crates/text/src/messages.rs` — `b459feb`

- `[x]` **U73** the corner orientation widget avoids the ribbon and both side panels, and the display-settings popover can hide it
  Why: `gizmo::widget_rect` takes the top-right of the active view minus the ribbon only, so
   the inspector covers the cube and its home and projection buttons. The widget has no on/off
   switch, while the owner wants one in the bottom cluster settings.
  Shape: Add `ViewCluster::show_widget: bool` (default true) and a checkbox in
   `settings_popover` beside the angle readout, with a `text` label and hint;
   `Document::viewport` skips `Gizmo::show` when it is false and keeps the gizmo state. Place
   the widget from the part of the view no chrome covers: start with the view rect, remove the
   ribbon band at the top, then remove the right panel when its rect overlaps the candidate
   top-right corner (expanded or folded), then the left panel on the mirrored corner; never
   return a rect that intersects `PanelRects` or the ribbon. If neither top corner has room,
   return `None` as today. Keep the function pure and the readout outside the hit rect.
  Done: Tests: the widget rect never intersects a panel or the ribbon for every panel
   visible/expanded pair, both layouts and a 1280 and 640 pt window; expanding the right panel
   moves the widget left of it and folding it leaves the widget clear; hiding the widget draws
   no cube, home or projection button and consumes no click; the switch does not change the
   document revision; the label and hint resolve in both languages. Checked by eye in both
   themes and at 200 % scale.
  `files: app/src/gizmo.rs, app/src/document.rs, app/src/view_cluster.rs, app/src/side_panels.rs, crates/text/src/messages.rs` — `45283da`

- `[x]` **U72** the side panels are a true overlay: the scene keeps one rectangle and one camera however a panel folds, and the translucent panel shows the furniture drawn under it
  Why: U66 gave both panels `Area`s over a full-width viewport, but `Chrome::content` still
   narrows the rectangle the split views tile by the panel width. Folding the tree therefore
   changes the camera aspect and pans the image, and at 92 % the panel blends with the clear
   colour instead of the furniture. The owner wants a continuous scene under the chrome.
  Shape: Make the scene rectangle depend on the ribbon band only: `Chrome::content` (or a new
   `Chrome::scene`) returns the area below the ribbon and ignores both panels.
   `Layout::regions`, the render target, the egui image and the active-view camera aspect read
   that rectangle, so folding, hiding, collapsing or expanding a panel changes no view rect and
   no camera. The panels remain `Area`s on `Order::Middle` above the image; `Chrome::covers`
   still makes a pointer on a panel chrome, never a pick, a border drag or a camera gesture.
   `frame_all` now fits the view below the ribbon, because the panels no longer narrow it; if
   that is not wanted, the task records the question rather than reintroducing the shrink.
   Update ADR 0080 and the U66 note to replace the split-tiles-only-uncovered clause.
  Done: Tests: `Layout::regions` and the active camera aspect are identical with both panels
  expanded, folded and hidden, and with the ribbon open and folded; the scene image is painted
  in the full view rect and a 55 % panel visibly shows it; a click on a panel selects nothing
  and starts no camera or border drag, while a click just outside selects; two-column and quad
  layouts still tile the view rect exactly and their dividers stay grabbable outside the
  panels; no document revision on any fold or hide. Both themes checked by eye with the panels
  at 55, 92 and 100 %.
  `files: app/src/side_panels.rs, app/src/document.rs, app/src/main.rs, app/src/strip.rs, docs/decisions/0080-the-tool-strips-float-over-the-design-space.md` — `c23e14b`

- `[x]` **M72** the figure, primitive and typed-property numbers use the shared field
  Why: U65 converted the placement, pivot, panel, camera and light cards to the shared 76 px
   field with the unit outside and a per-field commit policy. The figure and primitive
   parameter fields still draw their own 100 px text edit with a hint, behind an Apply button,
   and a typed Length or Angle property is a text edit as well.
  Shape: Route every number in these cards through Scrub: field of view and height style units
   outside the box, Live for a value the user feels out, OnRelease for a figure parameter that
   costs a rebuild. Remove ApplyFigure and the primitive Apply; a property Length or Angle
   keeps its numeric draft keyed by the property name and commits through the field.
  Done: Tests: a typed figure parameter commits once after the debounce and one drag is one
   undo entry; no numeric TextEdit is left in either card; an invalid entry keeps the value and
   marks the box red.
  `files: crates/ui/src/object_card.rs, crates/ui/src/properties_card.rs, crates/text/src/messages.rs` — `78abf51`

- `[x]` **M71** a previewing operation gets a command bar with a check and a cross, and a slow rebuild reports busy
  Why: U65 delivered the field anatomy and the commit policy; the previewing half of the
   owner's request has no operation to preview: the ribbon's boolean tools are `planned`, and
   extrude is a shell gizmo with its own Enter/Escape number popup.
  Shape: Add a session preview: an engine-side overlay of the document that a previewing
   command fills without a revision or an undo entry, discarded on cancel. Then one command bar
   at the bottom centre of the viewport — command name, the shared 76 px field with its unit
   outside, check (Enter) and cross (Escape), idempotent on a double click — starting with
   extrude and boolean. While a rebuild runs longer than ~250 ms the status bar says
   «Перестроение…», the last good geometry stays, and Escape cancels the pending command.
  Done: Tests: the preview changes no revision and no history; the check applies once however
   often it is clicked; the cross restores the pre-command state; a simulated slow rebuild
   reports busy and the window stays responsive.
  `files: app/src/status_bar.rs` — `3abe271`

- `[x]` **U64** the status bar shows the tool and counts; the owner wants the selection context in the centre — name, kind, size, material, edge banding, warnings — and the three display sliders renamed and moved to Settings → View.
  Why: The card should not carry derived context, and the display names are ambiguous: «Сетка»
   is the floor grid, not the 10 mm snap; «Рёбра» collides with furniture edge banding. The
   zones, the summary fields, the slider mapping and the reset values are in
   `plan/notes/U64.md`.
  Shape: Build one `StatusSummary` per frame from the already-resolved selection (no document
   scan): kind, name, count, size, material, edge banding and a warning slot; draw it as the
   centre zone of the status bar with chips and an ellipsis, the command prompt on the left and
   snap/projection/state on the right. Rename the cluster sliders: «Сетка пола» (grid
   brightness), «Контуры» (form outline strength), «Контактная тень» (contact shadow strength);
   keep on/off toggles in the cluster plus a «Настроить…» link; move the three strengths and a
   «Сброс» (60/70/40) to Settings → View; move «все рёбра треугольников» under «Дополнительно»
   as a diagnostic. Label the snap toggle «Привязка 10 мм», not «Сетка». Future parameters are
   new fields on the summary, not a new row.
  Done: Tests: the summary is derived from the selection without a second scan; a single
   selection shows name, kind, size and material; a multi-selection shows the count; a long
   line elides and keeps a tooltip; the status bar reports «Перестроение…» while a rebuild
   runs; the settings round-trip keeps the three strengths and the reset restores 60/70/40; the
   cluster no longer draws the triangle diagnostic; the status bar reads «Привязка 10 мм». Both
   themes and a narrow window are checked by eye.
  `files: app/src/status_bar.rs, app/src/view_cluster.rs, crates/ui/src/settings.rs, crates/ui/src/theme.rs` — `7703a06`

- `[x]` **U65** inspector fields carry units inside the editable text, are wide, and some edits need Apply; the owner wants numbers only at 76 px, immediate feedback with a short debounce, one undo per value, and a command bar with a check and a cross for operations that preview.
  Why: A unit inside the field is selected with the number and reads as data; Apply adds a
   click to every change; typing «120» rebuilds three times. The field inventory, the commit
   policy and the undo rules are in `plan/notes/U65.md`.
  Shape: Keep `scrub.rs` as the drag source and add a per-field commit policy: `Live` (scrub,
   every frame), `Debounced(150 ms)` (typed text and the numeric preset fields) and `OnEnter`
   (the rare value that must not rebuild while typing). Typed text commits on Enter or blur;
   invalid text keeps the old value and shows an inline error; one edit session per value
   coalesces the keystrokes and the drag into one undo entry. Units leave the editable string:
   they go to the section header («Размеры, мм», «Поворот, °»); fields are 76×22,
   right-aligned, tabular. The cards keep scrolling and folding. For operations with a preview
   (extrude, boolean), add a command bar with one ✓/✕ pair and Enter/Esc; no per-field Apply
   remains. A slow rebuild shows «Перестроение…» in the status bar and keeps the last good
   geometry; Esc cancels.
  Done: Tests: an invalid entry leaves the value and the revision untouched and shows the
   error; typing 1-2-0 commits once after the debounce; a drag commits one undo entry and
   Escape during the drag restores the value; Enter and blur commit; no unit glyph is part of
   the editable string; the command bar applies exactly once (a double click on ✓ is
   idempotent) and Esc restores; a simulated slow rebuild reports busy and the window stays
   responsive; every inspector card is checked for the new field width and headers.
  `files: crates/ui/src/scrub.rs, crates/ui/src/panel_card.rs, crates/ui/src/placement_card.rs, crates/ui/src/pivot_card.rs, crates/ui/src/settings.rs` — `8e7e5bb`

- `[x]` **U66** side panels are docked and opaque; the owner approved the M2 layout — panels floating over the scene at 92 % opacity, one opacity setting for ribbon, panels and clusters, collapse-all and hide-all buttons, neutral chevrons and an animated inset.
  Why: The panels are egui panels in the layout flow while the ribbon floats; the fold chevrons
   are ambiguous and there is no way to clear the chrome. M2 is the most invasive part of the
   accepted combo: the viewport must render under the panels and picking must ignore the
   covered rects. Geometry, contracts and tests are in `plan/notes/U66.md`; metrics in
   `plan/notes/U71.md`.
  Shape: Turn both side panels into `Area`s at `Order::Middle` over a full-width viewport, with
   a `panel` fill at the shared opacity; pass the covered rectangles to the shell so the scene
   gizmo, the selection rectangle and the split-view layout ignore them. The top inset follows
   the ribbon band plus 8 px; animate the ribbon height (150 ms) so the panels move smoothly on
   a fold. Add the three tab-row buttons: collapse all (both to 34 px strips, remembering the
   prior state), hide all (both hidden, remembering), fold ribbon. Replace the chevrons with
   the neutral `panel-left`/`panel-right` glyphs pointing to the collapse edge, mirrored in the
   strip; add tooltips and shortcuts `Ctrl+0` (hide all) and `Ctrl+Shift+0` (collapse all)
   after checking for conflicts. Persist `appearance.panel_opacity` (55–100, default 92) and
   the per-panel visible state.
  Done: Tests: a click inside a covered rect never reaches the viewport (scene gizmo and
   selection rectangle); hide-all then restore returns the exact prior state of both panels;
   collapse-all and a single fold both round-trip; the opacity setting round-trips and repaints
   all three surfaces; the panel tops stay aligned with the ribbon during the animation (a
   frame test or a golden of the inset function); the split-view layout tiles only the
   uncovered area; no document write. Both themes and 55/92/100 % are checked by eye.
  `files: app/src/side_panels.rs, app/src/main.rs, app/src/document.rs, app/src/header.rs, app/src/view_cluster.rs, app/src/shortcuts.rs, app/src/strip.rs, crates/ui/src/settings.rs, crates/ui/src/theme.rs, crates/ui/src/icons.rs, crates/text/src/messages.rs` — `cd4db46`

- `[x]` **U67** variants of one tool — circle by centre and radius, by two points, by three points, tangent — live in a dropdown under the tool; a click always opens it; it closes on Esc, an outside click and a tab change; the pinned variant is a global setting.
  Why: The ten `DrawTool` values are siblings with their own icons, so the map cannot say
   «Круг» once and keep its input methods; the owner approved the F1 list (not the shelf, not
   repeat-last). The variant tables and the list anatomy are in `plan/notes/U67.md`, the
   metrics in `plan/notes/U71.md`.
  Shape: Add `variants` to `ToolSpec`; draw a caret when it is non-empty. Hold `open_tool:
   Option<ToolId>` in `Toolbars` — view state, not the document. Draw the list as an
   `egui::Popup`/`Area` at `Order::Foreground`, anchored to the cell and flipped to
   right-aligned at the window edge: 298 px, 30 px rows, header with the tool name and
   shortcut, optional section captions, rows with icon, label, shortcut and pin, no footer.
   Keyboard up/down/Enter, Esc closes; outside click and a tab change close. The pin writes
   `settings.ribbon.pinned_variants` (global `BTreeMap<ToolId, VariantId>`, default first); an
   unknown id falls back to the first. A plain click runs the pinned variant; the shortcut runs
   it without opening.
  Done: Tests: opening the list writes no document revision; Esc, an outside click and a tab
   change close it; Enter runs the highlighted variant; a plain click runs the pinned variant;
   an unknown pinned id falls back to the first; the list never leaves the window rect (both
   edge cases); the master shortcut runs the pinned variant with the list closed; the list has
   no footer. The existing draw tools are migrated to variants (line/polyline stay separate
   tools) and the old per-variant ribbon cells are gone.
  `files: crates/ui/src/toolbars.rs, crates/ui/src/settings.rs, crates/ui/src/icons.rs` — `b16dfb7`

- `[x]` **U68** the ribbon is three `match` tables; the approved map — tabs Эскиз, Модель, Мебель, Сцена, Вид and groups such as Создать, Изменить, Опорные, Размеры, Булевы — becomes one const spec rendered by one loop with the L3 metrics.
  Why: Adding a tool means a new arm in three matches; the map cannot be expressed (22 tools in
   Эскиз, namespaced ids, planned rows), and `columns()` is hardcoded per tab. The full map,
   the id convention and the metrics are in `plan/notes/U68.md`; the shared design is
   `plan/notes/U71.md`.
  Shape: Add `TabSpec { id, label, groups }`, `GroupSpec { id, label, tools }`, `ToolSpec { id,
   label, icon, family, shortcut, variants, state }` and `VariantSpec` in one `ribbon` module;
   stable namespaced ids (`sketch.create.line`). The renderer iterates the spec and measures
   the cell from the widest tab instead of a constant. Availability stays code
   (`enabled(context)`), the table holds order and labels; a `Planned` tool (no command yet) is
   skipped, so a tab with no live tool is not drawn. Adopt the metrics: 28 px tab row, 30 px
   tool row, 12 px caption band, 34×30 cells, 72 px open / 29 px collapsed; persist the open
   state; labels and hints go through `text`. This task keeps one cell per tool —
   variants come in U67.
  Done: The five tabs draw the live tools of the map; every implemented `ToolAction` sits in
   exactly one tool; a planned row is skipped and an empty tab is not drawn; ids are unique and
   stable across runs; the cell width is computed from the spec; the body is 72 px open and 29
   collapsed; tests cover the map (every action once, unique ids, computed columns, empty tabs,
   height) and the open state round-trips; the old `RibbonTab`/`ToolGroup` matches and their
   tests are gone.
  `files: crates/ui/src/toolbars.rs, app/src/main.rs, crates/ui/src/icons.rs, crates/text/src/messages.rs` — `2e71b9c`

- `[x]` **U69** the approved sketch map needs sixteen tools with no artwork — trim, extend, offset, fillet, chamfer, break, four construction tools, three dimensions and three booleans — and the ten existing contour icons must be re-inked in the draw role.
  Why: A zero-byte asset draws its label, so the ribbon at 22 px would read as text; the mock's
   glyphs are a starting point, not a specification. The names, meanings and composition rules
   are in `plan/notes/U69.md`, the role palette in `plan/notes/U70.md`.
  Shape: Draw under `assets/icons/contour/`: `trim`, `extend`, `offset`, `fillet`, `chamfer`,
   `break`, `construction-horizontal`, `construction-vertical`, `construction-parallel`,
   `construction-angle`, `dimension-linear`, `dimension-angular`, `dimension-radius`,
   `boolean-union`, `boolean-subtract`, `boolean-intersect`. Keep the 24 grid, ink inside
   3..21, 20–45 % coverage and the role inks from U70; add one `Icon` const per file and its
   README row; re-ink the ten existing contour files to the draw role; one emphasis per icon
   and no colour as the only signal.
  Done: Every new file rasterises non-empty at 16, 20 and 22 px, stays inside 3..21 and the
   coverage band, and contains only role inks; the theme table in `assets/README.md` and the
   `ALL` list move together; the empty-file fallback test still passes; both themes are
   reviewed at 22 px on the ribbon built by U68.
  `files: assets/icons/contour, assets/README.md, crates/ui/src/icons.rs` — `9a5a3d0`

- `[x]` **U70** icons are one white ink; the owner approved colour by semantic family — draw, modify, model, scene, view, danger — with a light-theme remap and a monochrome fallback below a 20 px raster.
  Why: ADR 0047 and ADR 0055, `assets/README.md` and `STYLE_GUIDE.md` §5 make one ink
   normative, so a coloured set is impossible without superseding them. The set must not grow a
   second file per theme: the master carries role inks and the rasteriser maps them to the
   active theme's ramp. The role table, the threshold and the settings field are in
   `plan/notes/U70.md`.
  Shape: Allow up to three role inks per file from a fixed table of literal hex values
   (validated by a test); tag each icon with its family; `icons.rs` maps each role hex through
   a `RoleRamp` for the active theme (dark or light), preserves the alpha silhouette and bakes
   the result into the texture. Add `appearance.icon_color { Auto, Always, Never }` from U71.
   At a raster under 20 px, or when the mode is `Never`, rasterise the alpha and tint it with
   the theme icon colour — today's path — so the same master serves both. Cache by `(uri, px,
   theme, mode)`. Disabled stays an alpha multiply; hover and press are one lighten step, not a
   second colour.
  Done: Tests assert the expected role colour is present in the uploaded pixels for every
   preset × theme; the alpha silhouette is byte-identical between dark and light; the mono
   fallback at 16 px equals today's tint path; no file carries a colour outside the role table;
   each role hue clears 3:1 against panel and ribbon. `assets/README.md`, `STYLE_GUIDE.md`
   §5/§7, `AUDIT.md` and the icon skill are updated in the same commit, and the existing files
   are re-inked.
  Decisions: ADR 0090 records the role inks and supersedes the one-ink clause of ADR 0047 and
   ADR 0055; the rasteriser and the tonal emphasis of 0055 stand.
  `files: crates/ui/src/icons.rs, assets/README.md, assets/icons/STYLE_GUIDE.md, assets/icons/AUDIT.md` — `87487c7`

- `[x]` **U71** the window draws one palette, and the owner approved three — P1 graphite·copper, P2 ink·azure, P4 slate·mint — each in a dark and a light variant, chosen in Settings with one accent override.
  Why: `theme.rs` holds one dark and one light `Palette`, and the settings store only keeps the
   accent, so a second look means a second palette file and a second code path. The owner wants
   three switchable looks and one Appearance page that also hosts the panel opacity (92 %
   default) and the icon-colour mode the icon pipeline (U70) reads. The shared design —
   palettes, tokens, metrics and behaviour — is in `plan/notes/U71.md`.
  Shape: Add `ThemePreset { GraphiteCopper, InkAzure, SlateMint }` with a display name and a
   dark and a light ramp; `apply_with` picks the preset's ramp for the active variant and
   leaves the accent override in place. Derive hover, pressed, line and disabled from the base
   tokens with `mix` so six ramps stay small and a new token costs one field per ramp. Store
   `appearance.palette` with `#[serde(default)]`; an unknown preset loads P1 and reports one
   notice in the status bar, never a panic. The Settings window gains an Appearance page: three
   preset cards with swatches, the accent picker, a panel-opacity slider (55–100, default 92),
   the icon-colour mode (auto/always/never, consumed by U70) and a Reset. No per-preset icon or
   asset files.
  Done: A test loops every preset × dark/light and asserts contrast: text/panel ≥ 4.5:1, dim ≥
   3:1, accent/surface ≥ 3:1, and each icon role hue against panel and ribbon ≥ 3:1. Switching
   a preset redraws the open window without restart or texture reload; the settings round-trip
   keeps preset, accent and opacity; an unknown preset and a corrupt settings file load P1 and
   report one notice; both themes of all three presets are checked by eye on the ribbon, the
   panels and the viewport chrome.
  `files: crates/ui/src/theme.rs, crates/ui/src/settings.rs, app/src/main.rs` — `c0d6edc`

- `[x]` **P11** the plan tool cannot take a bug: a session that fixes an entry in `## Bugs` has no verb that moves it into `## Now`, so it edits `plan/BACKLOG.md` by hand, which ADR 0085 forbids. Add `cargo xtask task take <ID> [--at top|end]` (default end): move an open `## Bugs` entry into `## Now` at the chosen place, keeping its text, fields and `files:` hint, in one validated write. Refuse a done task, an unknown id, a task already in `## Now`, and `--at top` while another task is in progress. Into an occupied `## Now` it changes no `STATE`; into an empty one it sets `STATE: task=` to the taken task, rewrites `## Next` and leaves `state=idle`. Tests: a bug moves with every line intact, taking it into an occupied `## Now` leaves `STATE` alone, taking it into an empty one makes it `STATE`, and a done or unknown task is refused.
  `files: state-contract/src/cli/write.rs, state-contract/src/edit.rs, state-contract/src/cli/mod.rs` — `9d63579`

- `[x]` **X31** an along-contour drag on a sub-millimetre segment panics: dragging a corner along a
  segment shorter than twice the clamp margin (2 µm) aborts the editor instead of clamping or
  refusing. Expectation: the corner stops at the segment's own ends, or the edit is refused with an
  error the user can read; a furniture project never crashes. Reproduced: a path with a 1 µm first
  segment and `move_corner_at(PathCorner::End(id 1), point(50, 0), CornerMove::Along(id 1))` panics
  in `f64::clamp` with `min = 1.0, max = 0.0`. Suspected cause: `Segment::clamp_to_extent` turns the
  1 µm margin into a parameter fraction by dividing it by the segment's length, so for a segment
  shorter than 2 µm the lower bound exceeds the upper; the document's `clamp_to_curve` computed the
  same fraction before S12, so the move inherited the defect unchanged. A 1 µm line is a legal
  document segment: `ContourSegment::line` refuses only a coincident pair. Tests: a drag along a
  1 µm line and along a 1 µm arc returns `Err` instead of panicking; a drag along a normal segment
  still stops one micrometre short of the far end.
  `files: crates/geom/src/segment.rs, crates/model/src/contour_edit.rs` — `2e4bf61`

- `[x]` **X22** a free corner drag on an arc makes a near-full circle: dragging the corner where an
  arc meets a line in *Free move* turns the arc into the long way round, and the draft draws a huge
  loop whose visible branch reads as a straight line through the joint and across the viewport.
  Expectation: a corner drag keeps the arc's class — a shallow arc stays shallow, a deliberate major
  arc stays major — and the draft never draws outside the figure. Suspected cause:
  `Arc::with_radius_through` picks the new centre on the side of the old arc's midpoint against the
  *new* chord and then returns the first direction whose midpoint is on that side. For a shallow arc
  a 1 mm corner drag rotates the new chord enough to move the old midpoint to the other side, so the
  test matches the major arc's midpoint and the sweep becomes ~360°. Reproduced with chord 100 mm,
  sagitta 0.5 mm (R 2500 mm, sweep -2.292°): a 1 mm corner drag yields sweep 357.731° and a sampled
  chain spanning x -2425..2575, y -4899..101 mm; the joints still meet, so the ring validates and
  Apply stores it. Introduced by X13 (`64c11d3`); S12 moved the maths, not the defect. Fix: choose
  the direction by its sweep magnitude against the old arc's — nearest |sweep|, tie broken by the
  old sign — and pass the old sweep into the rebuild; the centre side stays as it is. Tests: a
  shallow arc keeps a small sweep and its points stay inside the figure; a major arc stays major;
  the three existing `a_free_arc_end_*` tests stay green; a grip drag on a shallow arc keeps its
  class.
  `files: crates/geom/src/segment.rs, crates/ui/src/contour_grips.rs` — `8275ae9`

- `[x]` **X34** origin and pivot are separate concepts, and the code names, commands, card and schema must say so
  Why: origin (the local zero, named by Placement::translation) and pivot (the authored local
   offset, the Pivot field) are still one word in the creation API and in the panel card, so a
   general rebase can silently move a pivot and a panel's derived base corner is presented as
   an authored handle.
  Shape: P0 renames the origin vocabulary, Pivot to PivotLocal and the wire field to
   pivot_local with a serde alias; P1 makes rebase_origin a general operation that preserves
   both world vertices and pivot_world and adds origin_local/origin_world; P2 keeps a panel's
   pivot derived and zero; P3 splits the inspector into Origin and Pivot; P4 bumps the schema
   without moving the world; P5 writes ADR 0088 and corrects the decisions and domain docs.
  Done: every creation path names its origin, PivotLocal is the only pivot, rebase_origin
   preserves world vertices and pivot_world, set_pivot/center_pivot change only pivot_local,
   placement.world_zero changes only translation, a panel's pivot is read-only zero, old
   goldens load unchanged through the alias, ADR 0088 exists, cargo xtask ci is green.
  `files: crates/model/src/frame.rs, crates/model/src/placement.rs, crates/model/src/contour.rs, crates/model/src/project/object_cmds.rs, crates/model/src/project/node_cmds.rs, crates/model/src/project/panel_cmds.rs, crates/model/src/project/query.rs, crates/model/src/project/wire.rs, crates/model/src/project/frame_contract.rs, crates/engine/src/node_cmds.rs, crates/engine/src/session.rs, crates/ui/src/pivot_card.rs, crates/ui/src/placement_card.rs, crates/ui/src/coords.rs, crates/text/src/messages.rs, docs/decisions/0088-origin-and-pivot-are-different-points.md, docs/decisions/0026-the-manipulation-pivot-is-stored-node-data.md, docs/decisions/0082-the-local-zero-is-the-creation-anchor.md, docs/decisions/0083-a-panel-is-anchored-at-its-base-corner.md, docs/domain/document.md, docs/domain/geometry.md, docs/domain/panels.md, docs/domain/scene-file.md, crates/project/tests/golden/scene.z3l, PROJECT.md, AGENTS.md, plan/map.md` — `c250d09`

- `[x]` **X8** the scene stays dark in the light theme: with the workspace on *Light*, the viewport
  still clears to the dark chrome, the floor grid keeps its light-on-dark lines, and the rendered
  frame reads as a dark hole in a light window. Expectation: a theme change repaints the scene
  background, the grid and the non-material lines to the active variant at once, leaving the
  furniture's own material colours untouched, and switching back restores the dark values.
  Suspected cause: the frame background and the grid line colours are literals in `crates/scene`,
  and the shipped frame never gets them from the active `egui::Visuals` when `theme::install_with`
  changes. Tests: a light frame carries the light background and grid; switching variant changes the
  uploaded scene without touching the document revision.
  `files: crates/scene/src/lib.rs, crates/render/src/lib.rs, app/src/main.rs` — `eba2f51`

- `[x]` **X33** a drag on the UI scale makes the window jump and loses the scale
  Why: Dragging the UI-scale field makes the window jump between values: with the Settings
   window open on Appearance, pressing the scale field and moving the pointer slowly to the
   right scales the interface up, then throws it to 75 %, then up to 200 %, and the scale that
   is written to settings.json and restored on the next launch is whatever the oscillation
   ended on (75 % was seen). Measured: the drag is read as a point distance, and a scale change
   re-lays out every widget, so the same physical pointer position is a different number of
   points in the next pass — the drag adds its own rescaling as travel. Reproduced in a test
   that runs the real input pipeline (positions in points converted with the current scale,
   native density 1 and 2): the percentage ran 100, 101, 104, 105, 90, 87, 154, 171, 75, 200.
  Shape: The field measures the pointer's own motion, which no screen coordinate can disturb:
   the raw motion the platform reports when it has one (winit forwards its device motion as
   Event::MouseMoved), and otherwise the difference between two pointer places, each in the
   pixels of the scale it was measured at. The drag accumulates from the value the button went
   down on, including the trip egui counts before it calls a press a drag. The trips are kept
   so the number, and the percentage the control reports, are the ones the hand made.
  Done: Tests: a drag in the window's own pipeline climbs without ever falling back, with and
   without raw motion; the same physical travel on a 2x monitor moves the place-based path the
   same amount; the field tests still pass with a growing widget. Live: settings.json holding
   125 opens the window at 125 % with the 125 % preset selected, and a run leaves the file
   untouched.
  `files: crates/ui/src/scrub.rs, crates/ui/src/ui_scale.rs` — `7c01b95`

- `[x]` **U52** the UI scale is a scrubbable number with presets: the Settings slider becomes a numeric
  field that changes while the pointer is held and moved left or right, with the cursor hidden for
  the duration and only the horizontal delta read, plus buttons for 75, 100, 125, 150, 175 and 200.
  The same *hide the pointer, read the delta* rule applies to every numeric field that scrubs on a
  horizontal drag, so a value that resizes its own widget — the scale is the worst case — no longer
  chases the cursor. A typed value is clamped to the offered range and applied through
  `ctx.set_zoom_factor`; a preset applies exactly and draws as selected. Tests: a drag reads the
  delta and never the absolute pointer position; the value does not jump when the widget changes
  size under it; a preset sets its exact percent; a typed value outside the range clamps.
  Session notes: `ui_scale::show` is the only scale control; `Response::drag_delta` or
  `ui.input(|i| i.pointer.delta())` is the input to read, and `CursorIcon::None` hides the pointer.
  Find every scrubbing numeric field through `egui::DragValue` in `crates/ui/src`.
  `files: crates/ui/src/ui_scale.rs, crates/ui/src/settings.rs, crates/ui/src` — `929b780`

- `[x]` **U50** the panel header loses the count and gains a scene-side fold: the digit next to
  *Scene* and *Inspector* disappears — the tree and the card already say what they hold, and the
  number cost a document scan on every rebuild — and the fold chevron moves to the edge facing the
  viewport, so the left panel folds from its right and the right panel from its left. The collapsed
  strip keeps its icon and turned title, and the click, edge-double-click and outward-drag restore
  rules from U40 stay. Tests: neither header draws a count; a left panel's chevron sits at its right
  edge and a right panel's at its left; folding and restoring keeps the expanded width.
  Session notes: `panel_header` and `count_chip` in `app/src/main.rs` draw the current
  row; `node_count` is cached only for that chip. `PanelSide` already names which edge a panel sits
  on, so the chevron side follows from it.
  now: committing the finished header and running the gate
  `files: app/src/main.rs, crates/ui/src/theme.rs` — `873515c`

- `[x]` **U49** the interface speaks English and Russian: one typed message table behind a language
  switch, and no user-facing string left inline. A new `crates/text` (L0, no dependencies, registered
  in the layer map and `PROJECT.md` in the same change) holds `Lang::{En, Ru}` and one `Message`
  variant per string; `Message::text(self, lang) -> &'static str` reads a `&[&str; Lang::COUNT]`
  table, so adding a third language changes `Lang` and turns every missing translation into a
  compile error instead of an English fallback. Every call site in `crates/ui` and
  `app` draws through that table: the header, the ribbon tabs, group captions and button
  captions, the scene tree context menu, all inspector cards and section titles, the status bar,
  the file and modal dialogs, and the argument messages of `PanelDiagnostic`
  (`crates/ui/src/panel_card.rs` `diagnostic_message` moves into the table and stops being a
  `String` builder where the variant carries no argument). `CommandSpec::label` becomes a
  `Message` too — all 48 specs — so the palette (U47) and any future CLI or assistant print the
  same words; the `id` stays the identity and is never translated. Russian needs real plurals, so
  a counting message is a constructor rather than one format string: `Message::more(n)` and the
  like pick one/few/many themselves. The language is workspace settings, never document data: it is
  chosen in the U41 *Settings* window, applies at once, and a saved `.fcs` never carries it. A name
  the editor generates on its own — `{} body`, `{} drawing` — is fixed in the language that created
  it, and switching the language never renames a node; that rule goes into ADR 0067 with the rest.
  Not translated: command ids, `FurnType` numbers, material and node names, `mm`, and the
  `thiserror` messages of `model`, `units` and `project`, which stay English for logs and tests and
  are mapped to a `Message` at the UI boundary instead of being shown raw. Tests: every `Message`
  variant is non-empty in both languages; a missing language cannot compile; every command spec
  label resolves in both languages; switching the language redraws the header, the ribbon and a
  card without touching the project revision or the undo stack; a scene saved in Russian loads in
  English with its node names unchanged; the longest Russian ribbon caption fits the 48 pt labelled
  button of U39 without clipping. Session notes: `Lang` is a plain enum and the table is
  `const`, so `crates/text` needs no serde, no `egui` and no file I/O — it is data, like
  `units`. The 48 labels are the `CommandSpec::new` calls in
  `crates/engine/src/commands.rs`; `label()` is read in `crates/ui/src/toolbars.rs`,
  `crates/ui/src/lib.rs` and `app/src/header.rs`. Existing inline strings are ~530 in
  `crates/ui` and ~200 in `app`; `theme::PRODUCT_NAME` and the logo stay as they are.
  Do this after U39–U48 so the labels stop moving, and before M8/M9, which add hardware, cutting
  lists and CNC text on top of the same surface. A future move to per-language files (a translator
  working outside the repository) reuses `Message` as the key, so this does not lock the format.
  `files: crates/ui/src, app/src, xtask/src/main.rs` — `c027309`

- `[x]` **U46** the icon set catches up with the new surfaces: the header, the ribbon groups, the
  viewport bar and the split layout get their drawings under `assets/icons`, so no chrome surface
  is left as a text fallback. New: `view/orbit`, `view/pan`, `view/zoom`, `view/layout`,
  `view/shaded-edges`, `view/wireframe`, `view/xray`, `view/translucent`, `view/sliders`,
  `view/front`, `view/top`, `view/right`, `view/axon`, `actions/command` for the palette, plus the
  remaining `actions` files the header and the settings use. Each follows `STYLE_GUIDE.md` and `assets/README.md`, gets its README row, its
  button and the existing footprint/coverage test, and a shared drawing stays two files per theme.
  The tree's four `actions` toggles (`visible`, `hidden`, `lock`, `unlock`) are part of this task and
  are needed by U48.
  A placeholder that is still empty keeps the text fallback, so this task can land in slices without breaking a surface.
  Session notes: the `window` theme and `actions/{file-save,undo,redo,settings}` are drawn;
  `assets/README.md` lists both. `crates/ui/src/icons.rs` guards every non-empty file with the 24-grid,
  footprint and coverage tests; `Theme::ALL` and the uri count (`56`) move together with a new icon.
  `files: assets/icons, crates/ui/src/icons.rs` — `21a2d6a`

- `[x]` **U48** the tree gains the eye and the padlock: every row of the scene tree carries a
  visibility toggle and a lock toggle, drawn at 16 pt and revealed on hover or when the flag is set,
  and both are session state keyed by node `Id` — no command, no schema, no undo entry (ADR 0065).
  Hiding a row hides its whole subtree from the viewport: the scene builder skips a hidden node, and
  picking skips it too, so a hidden node cannot be selected by a click while the tree can still show
  it as hidden and restore it. Locking a row keeps it drawn and selectable but refuses a gizmo drag
  and a destructive command, and the status bar says which node is locked; the refusal happens in the
  shell before a dispatch, because it is a guard on a gesture rather than a document rule. Both sets
  are pruned like the open containers, so a deleted or undone node never leaves a stale flag, and an
  *Isolate* / *Show all* pair in the tree's context menu covers the common case of focusing on one
  assembly. Tests: hiding a container hides its children from the frame but not from the tree;
  picking returns nothing for a hidden node; a locked node still selects and still resists a drag and
  a delete; a deleted node's flags are dropped and an undone deletion starts visible and unlocked;
  the flags survive a selection change and die with the session.
  Session notes: `Editor` owns `open: HashSet<Id>` in `crates/ui/src/lib.rs` and prunes it in
  `retain_open`, which is the shape to copy for `hidden` and `locked`; `scene_build::evaluate` in
  `app/src/scene_build.rs` is where a skipped subtree disappears, and
  `mesh_pick::pick_scene` plus `scene_gizmo::pick` are where picking must agree. `icons::actions`
  already declares `VISIBLE`, `HIDDEN`, `LOCK`, `UNLOCK` as zero-byte placeholders, so the buttons
  draw their text label until U46 fills them.
  `files: crates/ui/src/scene_tree.rs, app/src/scene_build.rs` — `6b5c502`

- `[x]` **X32** the cross on a tab does not close it, the File menu offers no Close and no Exit, the active tab is barely distinguishable and the new-scene button is a text label
  Done: the cross closes its tab (a click on the cross is a close, a click on the body
   activates), File carries Close tab and Exit (Exit asks once when any scene has unsaved
   changes), the active tab draws an accent bar and an accent-tinted fill, and the new-scene
   button draws assets/icons/actions/add.svg instead of its label; a test proves the cross wins
   over the body and the two File entries land in the right path
  `files: app/src/header.rs, app/src/file_menu.rs, app/src/main.rs, assets/icons/actions/add.svg` — `71a636f`

- `[x]` **U45** several scenes open at once: the tab row in the header holds one tab per open file,
  each with its own session, undo history, selection, camera and dirty marker, and activating a tab
  swaps the whole workspace without touching any `.fcs`. Closing an unsaved tab asks first, closing
  the last one leaves an empty *Untitled* scene, and *Save*, *Undo* and *Redo* in the header act on
  the active tab only. This is the one task in the set that changes an architecture boundary rather
  than a look: `Session`, `Editor`, `EvaluationCache`, `FileMenu` and the gizmo state must move from
  fields of the application into one `Document` per tab with its own renderer target, so it gets a
  plan of its own and lands last. Tests: a tab keeps its own undo stack and dirty flag; switching
  tabs restores the selection and the camera; a close with unsaved changes asks and a discard drops only that tab.
  Plan (approved by the owner 2026-11-09; not started): `app/src/tabs.rs` is the pure
  policy — `TabId`, `Tabs` with the order, the active tab, `activate`, and `close` returning
  `CloseOutcome { Closed, NeedsConfirmation, LastTab }` — testable without a window or a GPU like
  `view_layout`. `app/src/document.rs` holds the scene-bound payload: `Session`, `Editor`,
  `TruckKernel`, `EvaluationCache`, `SceneFrame`, `scene_revision`, `Layout` (its cameras),
  `home_camera`, `renderer`, `texture_id`, `FileMenu`, `PreservedSections`, the gizmos, `highlighted`,
  `selected_chains`, `uploaded_preview`, `node_count` and `last_error`; `sync_scene`, `upload_scene`,
  `viewport` and the `apply_*` handlers move onto it. `Z3LApp` stays the shell: the tabs, the
  settings and store, Settings/Home, the palette, the panel folds, the viewport cluster, the transform
  mode and the pointer. Every tab registers its own `TextureId` from its own `TextureView` and a close
  calls `Renderer::free_texture` for that id; the last close leaves one empty *Untitled*, never zero.
  `header.rs` gains the tab row and the `ActivateTab` / `NewTab` / `CloseTab` actions, and the dirty
  close reuses the `FileMenu` confirm dialog. Owner decisions: *Open* and *New* open a new tab and
  re-opening a path already open activates that tab instead of a second copy; the split layout travels
  with the tab because it already owns the cameras. Four commits: (1) `tabs.rs` and its tests with no
  behaviour change; (2) extract `Document` at one tab, all tests green; (3) the tab row, activation,
  close and per-tab textures; (4) ADR 0081, the map row, the plan close and the gate.
  `files: app/src/main.rs, app/src/file_menu.rs` — `c6d3836`

- `[x]` **S22** `main.rs` keeps the frame loop and gives up its chrome — rules: B1, B4, B8, H5.
  Why: after S21 the file is ~1980 lines, and four chrome pieces each have their own reason to change:
  the status bar (`StatusInfo`, `StatusOut`, `status_state_ui`, `apply_status`, `hint`, `rule`), the
  folding side panels (`PanelHead`, `side_panel`, `panel_header`, `count_chip`, `collapsed_strip`), the
  ribbon-band geometry (`strip_rect`, `strip_area`, `uncovered`, `fit_uncovered`, `view_region`) and
  the shortcut readers (`Shortcuts`, `take_shortcuts`, `take_drawing_shortcut`) — F1, B4.
  Shape: they move to `app/src/status_bar.rs`, `side_panels.rs`, `strip.rs` and
  `shortcuts.rs`; `main.rs` 1980 -> ~1460, and U45 then takes `viewport` (635) onto `Document` and
  leaves the shell near 800.
  Done: suite unchanged (the same 34 tests, 14 of them left in `main.rs`); each module states
   what it draws or reads and carries `//! Changes when:`; the `plan/map.md` rows for the
   status bar, the folding panels, the split views, the ribbon and the scene tree name the new
   files. Measured: `main.rs` 2608 -> 2037 production and 859 -> 265 test lines;
   `status_bar.rs` 287, `side_panels.rs` 159, `strip.rs` 114, `shortcuts.rs` 82 production
   lines, no ceiling of `plan/refactor.md` §3 crossed.
  Commits: one move.
  `files: app/src/main.rs, app/src` — `a543599`

- `[x]` **S21** the shell's Home surface moves out of `main.rs` — rules: B4, B8, H2, H5.
  Why: `home_surface` is one 632-line function inside `impl Z3LApp` (1620 lines, 40 fields),
  four times B4's 150-line function ceiling (F1).
  Shape: `home_surface` becomes a free function in `app/src/home.rs` (~640) over the state
  it reads, called from `ui()`; `main.rs` 2609 -> ~1980.
  Done: the existing Home tests move unchanged; `home.rs` states that it draws the start surface and
  returns one action, that it never touches the document, and `//! Changes when:`.
  Commits: one move.
  `files: app/src/main.rs, app/src` — `c83ace5`

- `[x]` **S20** `ui/contour_edit.rs` gives the drawing draft its own module — rules: B1, B4, B8.
  Why: 1112 production lines in one file cover two operations: editing an existing contour
   (`ContourEdit`, `ContourTarget`, `ContourDraft`, `ContourEntry`) and drawing a new one
   (`DrawTool`, `DrawDraft`, 425-1112) — a new draw tool and a new grip rule reopen one file,
   which is the crossing the audit measured beyond the filed findings.
  Shape: `DrawTool` and `DrawDraft` with their impls move to `crates/ui/src/draw.rs` (~690);
  `contour_edit.rs` 1112 -> ~425; `lib.rs` re-exports `DrawTool` and `DrawDraft` so the shell changes
  at most one `use`.
  Done: the draft tests move unchanged; both headers cite the decisions they implement (ADR 0048,
  0052, 0076, 0077) and state `//! Changes when:`.
  Commits: one move.
  `files: crates/ui/src/contour_edit.rs, crates/ui/src/lib.rs` — `ecf924f`

- `[x]` **S19** `crates/ui/src/lib.rs` becomes a re-export surface and `Editor` splits by mode — rules:
  B1, B3, B4, B5, B8.
  Why: 1085 production lines with `impl Editor` at 966, 18 fields and 60 public methods: four
   pointer modes, ribbon state and eight cards in one impl (F2, B4, B5).
  Shape: the `Editor` type and its state move to `crates/ui/src/editor/mod.rs`; the methods
   split by what they own — `selection.rs` (select, enter and exit a level, `retain_open`),
   `mesh.rs` (the mesh edit mode), `contour.rs` (the contour edit mode and its grips),
   `drawing.rs` (the draft and its commit), `creation.rs` (armed container, primitives, panels,
   assemblies, cameras, lights) and `cards.rs` (`show_scene_tree`, `show_card`, `run_action`);
   `lib.rs` 1085 -> ~150 and holds `mod` and `pub use` only (B3).
  Done: suite unchanged apart from paths and `use` lines (T1); the crate header still states the layer
  and that the UI dispatches and never writes the document; every new header names its mode and
  `//! Changes when:`; no module over 600 lines.
  Commits: one move; T2's sibling test files are optional and never in the same commit.
  `files: crates/ui/src/lib.rs, crates/ui/src` — `bc3b179`

- `[x]` **S18** `ui/lib.rs`'s `Editor` gives up the document rules it holds — rules: C7, D1, B1, H2.
  Why: three document rules live inside the UI's edit session (F2): `placement_in_parent` repeats the
  model's frame composition, `NEW_PANEL_LENGTH_MM`/`NEW_PANEL_WIDTH_MM` (600 x 400) are a document
  default held in the UI, and `resolve_viewport_pick`, `creation_target` and `child_containing` are the
  picking and creation-target rules (C7: the UI never holds a document default).
  Shape: `placement_in_parent` becomes `Project::placement_in_parent` beside `world_placement`
   in `crates/model/src/project/query.rs`; `default_panel_size` becomes
   `PanelCreate::default_nominal_size` in `crates/engine/src/panel_cmds/panel.rs` and the UI
   asks it; the pick and target helpers move to `crates/ui/src/target.rs`. Measured:
   `ui/lib.rs` 1158 -> 1085 production lines, `target.rs` 75, `impl Editor` still 966, so S19
   follows.
  Done: a model test for a rotated parent and for the root's identity frame; a created panel is still
  600 x 400; suite unchanged; the palette builds the same params.
  Commits: one move.
  `files: crates/ui/src/lib.rs, crates/model/src/project/query.rs, crates/engine/src/panel_cmds/panel.rs` — `cbc253e`

- `[x]` **S17** `render/lib.rs` gives up the vertex builders — rules: B1, B4, B8.
  Why: after S16 the file was 1136 production lines (`2c183d4`) and still held the CPU-side
   builders (`edge_geometry`, `shadow_geometry`, `world_vertices`, `flat_vertices`,
   `grid_vertices`, `polyline_vertices`, `mesh_centre`, `stencil_slot`, `edge_color`,
   `sort_back_to_front`).
  Shape: they moved to `crates/render/src/geometry.rs` (433 production lines); `render/lib.rs`
   1136 -> 742 and `impl ViewportRenderer` stays at 421, so both the file and the impl clear
   B4's ceilings (1000, 800).
  Done: suite unchanged; `geometry.rs` states that a builder reads frame data and writes vertices,
  never a device, and names the units it works in (H1, H2, H6).
  Commits: one move.
  `files: crates/render/src/lib.rs` — `1f361e1`

- `[x]` **S16** `render/lib.rs` gives up the pipelines and the GPU buffer helpers — rules: B1, B4, B8, H2.
  Why: 1446 production lines (F5) mix the renderer with eleven pipeline constructors, the depth-state
  builders and the buffer wrappers, so a shader change and an upload change reopen one file.
  Shape: `create_triangle_pipeline`, `create_line_pipeline`, `create_shadow_pipeline`, the
  `*_depth_state` builders, `face_target`, `vertex_layout`, `FACE_DEPTH_BIAS` and the stencil constants
  move to `crates/render/src/pipelines.rs` (~300); `GpuMesh`, `GpuLineList`, `GpuTriangles`,
  `line_list`, `triangle_list` and `clamp_size` move to `crates/render/src/gpu.rs` (~180);
  `render/lib.rs` 1446 -> ~1000.
  Done: suite unchanged; `pipelines.rs` states the depth, blend and stencil contract it encodes and
  `gpu.rs` states that it owns device objects and no frame data (H2, H6).
  Commits: one move.
  `files: crates/render/src/lib.rs` — `2c183d4`

- `[x]` **S15** `scene/lib.rs` gives up the camera and the ray — rules: B1, B4, B6, B8, H2.
  Why: 1277 production lines and 82 public items in one file cover the camera and its projection, the
  ray, picking, the grid, the lights, the contact shadows and the frame (F5).
  Shape: `Camera`, `Projection`, `FixedBasis`, `CameraError`, the fov, pitch and distance constants and
  the projection helpers move to `crates/scene/src/camera.rs` (~600); `Ray`, `ray_segment`,
  `ray_triangle`, `ray_hit_drawable` and `ray_hit_polyline` move to `crates/scene/src/ray.rs` (~180);
  `scene/lib.rs` 1277 -> ~520 (grid, lights, shadows, drawables, `SceneFrame`), under the ceiling, so
  no further split is filed.
  Done: suite unchanged; `scene/lib.rs` re-exports every moved name; both headers state millimetres,
  radians, near and far planes and the pick radius tolerance (H2).
  Commits: one move.
  `files: crates/scene/src/lib.rs` — `7372e80`

- `[x]` **P10** the gate follows the links the plan relies on
  Why: The gate checks that a hint names a path and that a map row names a file, and nothing
   else: five closed tasks point at files a later move deleted (`xtask/src/task.rs`,
   `geom/region.rs`, `panel_cmds.rs`), S14's own arithmetic disagrees with the tool's report
   (547 and 172 against 574 and 212), `plan/refactor.md` still offers `geom/region.rs` as B1's
   example, S20 cites an F10 the audit never filed, `plan/questions.md` is read by nothing, and
   `base` may be any ancestor, so a plan can describe a tree a hundred commits old and stay
   green.
  Shape: Every `files:` hint names a path that exists, closed entries included; a closed
   entry's ADR citation names a decision in `docs/decisions/`; `STATE:` carries only its six
   keys; `plan/questions.md` and `plan/debt.md` carry no status marker and no `STATE:` line,
   because they are not the plan; `state=idle` with `base` behind `HEAD` passes only when every
   commit in between touched `plan/` and nothing else.
  Done: The five dead hints point at the code that holds the work now; S14's numbers and B1's
   example are what the tree says; the plan stays green through `cargo xtask ci`; one test per
   rule.
  Commits: five: the repair path, the missing verbs, the hint splice, the links, the section
   rewrite.
  `files: state-contract/src/lib.rs, state-contract/src/validate.rs, state-contract/src/git.rs, plan/refactor.md, plan/BACKLOG.md, AGENTS.md` — `e353f02`

- `[x]` **S14** `engine/panel_cmds.rs` gives up its banding commands — rules: B1, B4, B6, B8.
  Why: 1143 production lines and 21 commands in one file with no test module, 547 of them banding
  (`panel.edge_banding.*`, `panel.corner_banding.*`, `panel.ring_banding.*`, `panel.corner.*`,
  `panel.bow.*`, lines 596-1142) — B4's ceiling and B6's public surface at once.
  Shape: the ten banding commands move to `crates/engine/src/panel_cmds/banding.rs`; the eleven
   that remain divide by value family into `panel.rs` (creation, outline, name, size, material)
   and `machining.rs` (the four records), because `mod.rs` holds declarations and re-exports
   only (B3) and 580 lines of logic is not one family (B1); `panel_cmds/mod.rs` re-exports
   every id and spec, so the `COMMANDS` table in `commands.rs` keeps its paths; `panel_cmds`
   1143 -> mod.rs 44 + panel.rs 411 + banding.rs 574 + machining.rs 212 production lines (1241:
   every new module carries its own header and imports), none past 600.
  Done: the registry test still finds every id; suite unchanged; `mod.rs` holds declarations and
  re-exports only (B3) and `banding.rs` states its one reason to change (H5).
  Commits: one move.
  `files: crates/engine/src/panel_cmds/mod.rs, crates/engine/src/panel_cmds/panel.rs, crates/engine/src/panel_cmds/banding.rs, crates/engine/src/panel_cmds/machining.rs` — `4e6d535`

- `[x]` **P9** the tool documents itself — rules: B1, C7.
  Why: the contract is written three times (`AGENTS.md` §1/§6.1, `templates/AGENTS.section.md`, the
  crate README), `plan/state.conf` does not exist although a second project needs its layout, and the
  usage blocks are hand-written.
  Shape: `cli::Config` with defaults (`runner`, `decisions`, `notes`) overridable from
  `plan/state.conf`; `agents` prints the embedded `templates/AGENTS.section.md` with the placeholders
  filled, `init` scaffolds a plan that passes the gate; `AGENTS.md` and the root `README.md` keep the
  invariants and point at the tool instead of repeating it.
  Done: a fresh `state-contract init` in an empty directory passes `check`; `agents` prints the
  section with the project's runner; `AGENTS.md` loses the duplicated command list; suite and
  `cargo xtask ci` green.
  Commits: one.
  `files: state-contract/src/main.rs, state-contract/README.md, AGENTS.md` — `c27002f`

- `[x]` **P8** the plan tool is one crate a project imports, not code in `xtask` — rules: B1, B3, B7, C7.
  Why: the write verbs, `list`, the agent form (`--md`/`--paths`), the code facts and the ADR index
  check live in `xtask` — 1 500 lines a second project would have to copy — while the crate that owns
  the format exposes only three read commands and two hand-written usage blocks that already differ.
  Shape: `state-contract/src/cli/` gains `table` (a `Command` list that drives dispatch, `--help` and
  `help <verb>`), `read` (status, dashboard, task, list, `--md`, `--paths`), `write` (the ten verbs),
  `facts` (production and test lines, the last commit, the ADR titles) and `docs`; the crate's own
  binary runs the whole table, and `xtask` keeps only its product rules (`layers`, the C-vendoring
  check, the `ci` step list) registered into the same table.
  Done: `cargo xtask <every command it has today>` prints the same output; `cargo xtask help` and
  `cargo xtask help task` are generated from the table; `cargo run -p state-contract -- help` lists
  the same commands minus the product ones; `xtask/src` holds no plan logic; the suite moves with the
  code and `cargo xtask ci` is green.
  Commits: one or two.
  `files: state-contract/src/main.rs, state-contract/src/lib.rs, xtask/src/main.rs` — `123afcb`

- `[x]` **P7** the gate checks the plan it already parses — rules: B1, C7.
  Why: `plan/map.md` is unchecked although a stale row sends a reader to a file that no longer exists;
  `## Next` in `plan/STATUS.md` is the first prose a session reads and can name another task than
  `STATE`; a debt tag with a typo silently drops out of the task report's selection; the ADR index can
  miss a new decision.
  Shape: `validate.rs` gains three checks — a map row naming a path that does not exist (only
  `.rs`/`.md`/`.toml` tokens and existing directories count), the first task id of `## Next` equal to
  `STATE: task=`, and every debt tag declared in the `Tags:` line; `xtask docs` compares
  `docs/decisions/README.md` with the files it indexes.
  Done: each check has a fixture that fails and the current plan still passes; `cargo xtask ci` green.
  Commits: one.
  `files: state-contract/src/validate.rs, state-contract/src/lib.rs, xtask/src/main.rs` — `5df8549`

- `[x]` **P6** the agent writes the plan through the tool, never by hand — rules: B1, B8, C7.
  Why: 194 commits touched `plan/BACKLOG.md` by hand, and a close is five edits — the marker, the
  hash, the move to `## Done`, `STATE` and deleting `SESSION.md` — each one a chance to break the
  order the gate enforces.
  Shape: `state-contract/src/edit.rs` splices line ranges in `plan/BACKLOG.md` and `plan/STATUS.md`,
  so a command rewrites only its own lines and the rest of the file stays byte-identical;
  `xtask task add|start|set|step|note|close|reopen|drop` and `xtask state set` build the new text,
  validate it with the gate in memory and write it through a temp file and a rename. Every verb takes
  `--dry-run` and prints its diff; `plan/.lock` keeps two writers apart.
  Done: each verb has a test that a refused command writes nothing; a round-trip test proves
  `parse(edit(parse(x)))` and that untouched bytes stay; `AGENTS.md` says the plan is written only
  through `cargo xtask`.
  Commits: one per verb family.
  `files: state-contract/src/parse.rs, state-contract/src/task.rs, xtask/src/main.rs` — `a35f3d5`

- `[x]` **P5** a session start needs no reasoning the tool can do — rules: B1, B7, C7.
  Why: `status` drops `resume:` and prints no interrupted flag, so `AGENTS.md` §1.1 case 1 makes the
  reader compare `base`/`HEAD` and inspect `steps` by hand; `task` prints only its own note, so
  research on another task that touched the same files is invisible; there is no way to enumerate
  tasks, the report does not say how many debt or question entries it filtered out, and a done task
  does not say which commit closed it. `next:` is also computed for a task outside `## Now`, where it
  means nothing.
  Shape: `Summary` carries `interrupted`, computed once in `check_with` (the dashboard stops deriving
  it); `render` prints `goal`, `resume` and the flag. `TaskView` gains `related` — other tasks' notes
  whose text meets the same `touches` needles — and `closed` (hash, date and subject of a done task).
  `xtask task list` filters by section, status, phase and text; the human and `--md` forms print how
  many entries were filtered out; `next` is `None` outside `## Now`; `state-contract` denies the
  panic lints at the crate root.
  Done: a session start prints `interrupted: yes` and `resume:` while the note has open steps; another
  task's note that names one of this task's files appears under `RELATED`; `list --phase X
  --status open` prints one line per task; the report says `+N other entries (--all)`; a done task
  shows `closed:`; the suite covers each; `cargo xtask ci` green.
  Commits: one or two. `[x]` `c884b81`
  `files: state-contract/src/lib.rs, state-contract/src/task.rs`

- `[x]` **P4** `cargo xtask task` prints one task in full — rules: B1, B7, C7, C10.
  Why: starting S14 costs `status`, then a grep of `plan/BACKLOG.md`, `plan/map.md`, `plan/debt.md`,
  `docs/decisions/` and `git log`, plus the production-line loop in `plan/refactor.md`; the gate
  already parses every one of those files, so the reads are one command's work, not the agent's.
  Shape: `state-contract` parses the labelled fields of an entry (`Rules`, `Why`, `Shape`, `Done`,
  `Commits`), reads a per-task note `plan/notes/<ID>.md`, and renders one task as a block a human
  reads (`task`, and `dashboard --task <ID>`); `xtask` adds what the plan does not carry — production
  and test lines per hinted file, the last commit that touched it, the ADR titles — and an agent
  form (`--md`, `--paths`) that needs no grep. The gate checks a note's id, staleness and budget.
  Done: `cargo xtask task S14` prints the entry with its fields, both files' line counts and last
  commits, the map row, matching debt, the ADRs, the note and the next task; `--md` carries no box
  drawing and `--paths` one path per line; a note for an unknown or finished task fails `plan`;
  `status` prints the lead sentence instead of the whole entry; the suite covers the new parse.
  Commits: one. `[x]` `7df9401`
  `files: state-contract/src/lib.rs, state-contract/src/parse.rs, xtask/src/main.rs`

- `[x]` **S13** `model/contour_edit.rs` splits into corner moves and shape frames — rules: B1, B4, B8.
  Why: after S12 the module is ~1280 lines, over B4's ceiling, and holds two reasons to change: moving
  a corner of any ring (`CornerMove`, `move_corner*`, trim and split, `PathCorner`) and the constrained
  drags of a tagged figure (`CircleFrame`, `RectangleFrame`, `PolygonFrame`, `ArcFrame` and the numeric
  setters, ADR 0073, ADR 0075).
  Shape: the four frames and their drags and setters move to `crates/model/src/contour_frames.rs`;
  `contour_edit.rs` keeps `CornerMove`, the error, `impl Path`'s corner moves and `impl Contour`.
  Done: suite unchanged; both headers name the constraint they enforce (free or along the picked
  segment; a tagged figure keeps its own) and their `//! Changes when:`; the `plan/map.md` row for
  contour edit math names both files.
  Commits: one move.
  Measured: 1481 -> 811 (`contour_edit.rs`) + 707 (`contour_frames.rs`) production lines, both under
  the ceiling though neither reaches the 600 target, because the shared corner machinery
  (`with_corner`, `geometry_arc`, `distance`, `SUPPORT_TOLERANCE`, `Path::position`/`replaced`) has one
  definition in `contour_edit.rs` and is `pub(crate)` instead of copied. The tests stayed in
  `contour_edit.rs` (T2 forbids mixing a test move into a logic commit); they reach the moved code
  through `Path` and the frame values, so only the two private `RectangleFrame` axes widened to
  `pub(crate)`.
  `d8864be`
  `files: crates/model/src/contour_edit.rs, crates/model/src/contour_frames.rs`

- `[x]` **S12** `model/contour_edit.rs` stops computing the trigonometry `geom` already owns — rules:
  D1, D2, B8, H2, H4.
  Why: 1572 production lines with 27 `millimetres_f64`/`radians_f64` conversions and its own
  `arc_with_radius`, `arc_between`, `arc_midpoint`, projection (`clamp_to_curve`) and split
  (`split_document`), although `geom` has `Arc::with_start_point`, `with_end_point`, `split_at`,
  `Line::split_at`, `nearest_on_support` and `from_endpoints_sagitta` (F3); `model/contour.rs` still
  calls itself the only place a segment becomes millimetres.
  Shape: the arc-from-radius, arc-between, projection and split maths become free functions beside the
  segments in `crates/geom/src/segment.rs`, each taking its tolerance as an argument (D2);
  `contour_edit.rs` keeps the `Length`/`Degrees` conversion at the call and computes nothing
  (conversions 27 -> ~8), 1572 -> ~1280. The `model/contour.rs` header states that the document
  converts once at its edge and that the maths lives in `geom`.
  Done: every contour-edit test passes unchanged; if a test disagrees with the `geom` formula, stop and
  file an `X` task instead of bending the test (§12); `cargo test -p model -p geom`.
  Commits: one move.
  Measured: `geom/segment.rs` grew 701 -> 875 production lines and holds `Arc::with_radius_through`,
  `Arc::through_ends`, `Arc::sweep_to`, `Arc::midpoint`, `Segment::clamp_to_extent`,
  `Segment::support_point_within` and `point_on_circle`; `contour_edit.rs` drops 1571 -> 1481 and keeps
  no cos, sin or split trigonometry. The conversion target was optimistic: 25 -> 21
  `millimetres_f64`/`radians_f64`, and the 21 that remain are all the document edge §4.3 requires (the
  rectangle half-extents, a length ratio, the radius argument of an arc rebuild, `geometry_arc`). The
  339 model and 97 geom tests pass unchanged; the move exposed X31 and did not fix it.
  `[x]` `392ebff`
  `files: crates/model/src/contour_edit.rs, crates/geom/src/segment.rs`

- `[x]` **S11** `geom/region.rs` becomes the general boolean alone — rules: B1, B3, B4, B8, H4.
  Why: after S10 the file is ~1120 lines and still mixes the ring model `Region` with the boolean
  algorithm, so a change to "what a region is" and a change to "how sets combine" reopen one file.
  Shape: `Region`, `RegionSamples`, `RingKind`, `RegionError`, the area, containment and validation
  helpers and `loop_to_contour` move to `crates/geom/src/ring.rs` (~450); what remains is the one
  named algorithm, renamed `crates/geom/src/boolean.rs` (~700, inside B4's 600-1200 band); `geom/lib.rs`
  keeps every public path.
  Done: suite unchanged; `boolean.rs` cites the method it implements and the sweep it does not do
  (self-intersection, tolerance), with `//! Changes when:` (H4, H6); the `plan/map.md` row for planar
  regions points at `ring.rs` and `boolean.rs`.
  Commits: one move; the rename is part of the move, not a separate edit.
  `7f1bc27`
  `files: crates/geom/src/boolean.rs, crates/geom/src/ring.rs, crates/geom/src/lib.rs`

- `[x]` **S10** `geom/region.rs` gives up the exact rectangle booleans — rules: B1, B4, B8, H2.
  Why: 1660 production lines, the largest geometry module, holds two unrelated layers (F5): the exact
  orthogonal boolean (`Rect`, `RectError`, `total_area`, `union_rects`, `subtract_rects`,
  `connected_count`, `regions_from_rects` and its loop tracer, 432-980) and the general contour
  boolean over `Region` (981-1660).
  Shape: the rectangle layer moves to `crates/geom/src/rect.rs` (~570) with its tests; `region.rs`
  1660 -> ~1120; `geom/lib.rs` re-exports `Rect` and the rectangle functions.
  Done: the 23 rectangle tests move unchanged (T1); `rect.rs` holds 575 production lines, `region.rs`
  drops 1660 -> 1126, the moved block and the moved tests are byte-identical, and the 62 geom tests
  pass unchanged; `rect.rs` states that a corner is a point and never a sample, that its arithmetic
  is exact, and its `//! Changes when:` (H2, H4); the `plan/map.md` row and ADR 0043 name both files.
  `186a5bc`
  `files: crates/geom/src/boolean.rs, crates/geom/src/lib.rs`

- `[x]` **S9** `geom/contour.rs` keeps the ring and gives the segment family its own module — rules:
  B1, B4, B8, H2, H5.
  Why: 1046 production lines crossed B4's 1000-line ceiling and the file held two value families with
  74 public items between them: `Line`, `Arc` and `Segment` with their impls (108-775) and the ring
  `Contour` with its operations (776-1046).
  Shape: `Line`/`Arc`/`Segment`, their impls and the private `positive_angle` moved verbatim to
  `crates/geom/src/segment.rs` (1046 -> 701 production lines); `ContourError`, `Contour`, the joint
  tolerances and the sampling caps stayed in `contour.rs` (1046 -> 374); `EPSILON`,
  `SUPPORT_TOLERANCE` and `MAX_ARC_EXTENSION` moved with the code that reads them; `geom/lib.rs`
  re-exports both modules and now also `MAX_ARC_EXTENSION`, which a model doc link already named.
  Done: 97 geom tests pass with one `use` line added for the constants the test module used to inherit
  (T1, B8); both headers state their family, millimetres and radians, the tolerance that matters and
  `//! Changes when:` (H1, H2, H5); the `plan/map.md` row names both files; `cargo xtask ci` green.
  `4a16ee8`
  `files: crates/geom/src/contour.rs, crates/geom/src/segment.rs, crates/geom/src/lib.rs`

- `[x]` **S8** the shared test fixtures move into `testkit` — rules: C7, T1, C4.
  Why: `fn millimetres` was written 20 times across 8 crates, `open_path` 5 times and `sample_project`
  twice, although `testkit` exists for exactly this (F7). Test-only: no production line
  change, no ceiling crossed — the finding was duplicated knowledge in 26 test modules.
  Shape: the three helpers have one definition in `model::testing`, behind the non-default `testing`
  feature that `testkit` enables and re-exports them from; `model` itself cannot dev-depend
  on the testkit, because the cycle compiles the library twice and its types stop matching (ADR 0084).
  `crates/model/src/fixtures.rs` keeps the fixtures only the model's own tests need; `param` and `mesh`
  gained the testkit `[dev-dependencies]` entry, `model` reaches the helpers through the feature.
  A shape one crate needs (the 100 × 60 chain, the 600 × 400 one, the parameterised L) stays local and
  is built through `open_path`.
  Done: 20 `millimetres` copies and 4 `open_path` shapes collapse into 3 definitions; the suite is the
  same 1378 tests with no assertion touched (T1, B8); `cargo xtask layers` green.
  `22e0e71`
  `files: crates/model/src/testing.rs, crates/testkit/src/lib.rs`

- `[x]` **S7** the design rules become an ordered refactor plan instead of an analysis: a read-only
  audit re-measures the tree, checks every finding against the code as it stands then, and files one
  small behaviour-preserving subtask per finding (`S8`, `S9`, … in dependency order) in `## Now`, then
  stops. The audit commit touches `plan/` only — no source file and no test. It runs right after S6
  because it is cheap and gives the rest of `## Now` its order; a subtask that edits `main.rs`,
  `crates/ui/src/lib.rs` or `app/src/file_menu.rs` lands before U45, which moves exactly
  those files into `Document` and `tabs.rs`, and U45 then re-plans against the smaller result; a
  subtask inside `geom`, `model`, `scene`, `render` or `param` is independent of U45.
  Findings measured on 2026-11-10, a baseline to re-measure and not a requirement: `F1`
  `app/src/main.rs` 2609 production lines, `impl Z3LApp` ≈1620, `viewport` 637,
  40 fields, 113 functions, ≥8 reasons to change; `F2` `crates/ui/src/lib.rs` `Editor` 18 fields, 60
  public methods, `impl` 1039, four pointer modes plus ribbon state plus eight cards, and three
  document rules inside it (`placement_in_parent`, `NEW_PANEL_LENGTH_MM/WIDTH_MM`,
  `resolve_viewport_pick`); `F3` `crates/model/src/contour_edit.rs` 1572 production lines with 27
  `millimetres_f64`/`radians_f64` conversions and its own projection, split and arc-from-radius
  trigonometry, which L0 `geom` already owns, while the `model/contour.rs` header still claims to be
  the single conversion point; `F4` `Project` ≈90 public methods and `Panel` 63, so one new panel
  field edits `model/panel`, `model/project/panel_cmds`, `engine/panel_cmds` (id, label, params,
  enabled, run), the `COMMANDS` table, `engine/lib.rs`, `ui/panel_card` and often `ui/palette`; `F5`
  file-monoliths with no submodule — `geom/region.rs` 1660 (exact rectangle booleans and general
  contour booleans), `scene/lib.rs` 1277 (camera, ray, grid, light, shadow, frame, picking),
  `render/lib.rs` 1446; `F6` `ui/panel_card.rs` and `ui/object_card.rs` ≈989 each (`impl PanelCard`
  ≈857, `run_action` 74) and `ui/toolbars.rs` 982 (`show_group_row` 146); `F7` shared fixtures
  rewritten in four crates (`millimetres`, `sample_project`, `open_path`) although
  `testkit` exists. Clean at filing time and not a finding: `cargo xtask layers` green, no
  `kernel` or GPU edge in `model`, no `egui` or `model` edge in `render`, the UI dispatches and never
  writes the document, the kernel port is five methods, tests are 39% of the tree.
  Instructions: read `plan/refactor.md` in full before the audit — it holds the measurement recipe
  (§2), the boundary, data, clean-code, documentation and test rules (§3–§7) with ids (`B1`, `D1`,
  `C4`, `H2`, `T1`), the legibility test that defines done (§8), what to borrow from SolidCraft and
  what never to copy (§9), the audit procedure (§10), the subtask template (§11) and the order and
  stopping rules (§12). The goal there is one sentence: a tree a single maintainer can hold in their
  head, where a panel attribute is a three-file change. In short: one module one operation, one
  reason to change stated in the header, geometry below the data and never inside a document type
  (it fixes F3 structurally, the way SolidCraft's `Seg2Geom for Seg2` does), a move never shares a
  commit with a fix, the existing suite passes unchanged, and every new or moved module header names
  its contract, its tolerance, what it refuses and `//! Changes when: …`. Documented debt is not
  re-filed: `plan/debt.md` `[view-state]` already owns selection and open containers in `Editor`, and
  F4's `Project::apply(change)` rewrite touches every command, so it is a question for the owner
  (`plan/questions.md`), not a subtask.
  Re-verify at the start, because the tree will have moved: S6 rewrites the frame tests and the panel
  and inspector documents, so `model/project`, `model/panel` and ADRs 0009, 0026, 0052, 0058, 0082,
  0083 are re-read, not quoted; U45, if it lands first, moves `Session`, `Editor`, `viewport` and
  `FileMenu` into `document.rs`/`tabs.rs`, so F1 and F2 are re-measured on the post-U45 files and the
  audit files no split that U45 already performs; U46, U48, U49, U50 and U52 edit `toolbars.rs`,
  `scene_tree.rs`, `main.rs` and the cards, so a subtask over one of those re-measures it first; R1
  (`plan/rename.md`) renames 13 crates and moves `apps/` to `app/`, so every path here and in
  `plan/map.md` is re-read after it, never copied from this entry.
  Done when `cargo xtask ci` is green on the pre-audit tree, every finding carries a verdict and
  either a subtask id with its `files:` hint or a `plan/debt.md` line under `[design]`, the subtasks
  sit in `## Now` in dependency order, each cites the `plan/refactor.md` rule ids it obeys with the
  production-line number it must reach, `plan/map.md` still describes the tree, and `plan/STATUS.md`
  names the first subtask. Tests: `cargo xtask plan` accepts every new hint and id, and the audit
  commit touches only `plan/`.
  Audit 2026-11-10: re-measured `geom/region.rs` 1660, `model/contour_edit.rs` 1572,
  `render/lib.rs` 1446, `scene/lib.rs` 1277, `ui/lib.rs` 1159, `engine/panel_cmds.rs` 1143,
  `ui/contour_edit.rs` 1112 and `geom/contour.rs` 1046 production lines, `main.rs` 2609; F1–F7
  confirmed except F6 (the two cards at 989 and `toolbars.rs` at 982 are under B4's ceiling, their
  impls under 800) and F4 (one `apply(Change)` per area is an owner decision, filed in
  `plan/questions.md`). Verdicts: F1 -> S21 (Home) and S22 (the chrome), with the `viewport` function
  and the 40-field impl deferred to U45; F2 -> S18 (the three document rules) and S19 (the `Editor`
  split); F3 -> S12 (the maths down to `geom`) and S13 (the split); F5 -> S10, S11 (`region`), S15
  (`scene`), S16, S17 (`render`); F7 -> S8; the three crossings the audit measured beyond the filed
  set -> S9 (`geom/contour`), S14 (`engine/panel_cmds`), S20 (`ui/contour_edit`). `plan/debt.md`
  `[design]` carries the four no-action verdicts. `model/panel/mod.rs` (967) and
  `model/panel/contour.rs` (889) stay under the ceiling and were not filed. `plan/map.md` still
  describes the tree, so no row moved.
  `9deed44`
  `files: plan/refactor.md, plan/BACKLOG.md, crates/model/src/contour_edit.rs, crates/ui/src/lib.rs`

- `[x]` **S6** the frame contract is enforced and written down: the invariants the earlier steps proved
  one by one become one test module, and the documents stop describing the old behaviour. Tests: for
  every creation path (line, polyline, arc, rectangle, circle, polygon, contour, extrusion, primitive,
  panel, group, duplicate) the local zero is the creation anchor and `pivot_local` is zero;
  `vertex_world = translation + rotation * vertex_local` and
  `pivot_world = translation + rotation * pivot_local` for every payload; a geometry edit changes no
  component of `translation`, `rotation` or `pivot_local` (an object edit; a panel outline edit
  re-anchors by X30 and compensates the translation instead); a move changes only `translation`; a turn
  about a pivot keeps `pivot_world`; world-zero keeps vertices, pivot and rotation; duplicate copies
  local fields exactly; undo/redo restores the frame bit for bit; a round trip at the current schema
  keeps every invariant. Docs: ADR 0082 and 0083 (written
  with S5 and X26) are checked against the code, ADRs 0009, 0026, 0052 and 0058 are corrected,
  `PROJECT.md` §"Space and views", `AGENTS.md` §4.10 and the `plan/map.md` rows for the frame, the
  panel and the inspector are updated. Two commits: the invariants with any fix they force, then the
  documents and `cargo xtask ci`.
  `d7ad707`
  `files: crates/model/src/project/mod.rs, docs/decisions, PROJECT.md, plan/map.md, AGENTS.md`
- `[x]` **X30** a panel's frame follows its parameters, its pivot does not: `move_frame_to` shifted the
  pivot by `-min` when a contour edit moved the ring's minimum corner, so the pivot drifted off the
  base corner, and the inspector let every node author a pivot. Expectation: a panel's pivot is
  derived data. An outline edit, a resize, a contour replacement or a rectangle baked into a ring
  creates the bbox, anchors the outline at zero, compensates `translation` so the frame's corner
  keeps its world position and sets `pivot_local = 0`; machining shifts with the outline. A panel's
  pivot is not authored: `set_node_pivot`/`center_node_pivot` return `PanelPivotIsDerived`, both
  committed commands are disabled for a panel, `move_mesh_vertices` never reaches one, and a loaded
  panel with a non-zero pivot is a readable error (ADR 0083). Tests: an outline edit keeps every
  drawn world point and every machining point, `translation` is the new world base corner and the
  pivot is zero; a resize keeps the translation; the contour-create panel reloads after an outline
  edit; the pivot commands refuse a panel and change nothing; the load error.
  `524f58f`
  `files: crates/model/src/panel/mod.rs, crates/model/src/project/panel_cmds.rs, crates/model/src/project/node_cmds.rs, crates/model/src/project/mod.rs`
- `[x]` **X29** a group is anchored at its bounds centre: `group_nodes` builds the container with an
  identity placement while `Container::new` puts the pivot at the centre of the children's bounds, so
  the card shows `Translation (0, 0, 0)` while the pivot sits in the middle of the group — the same
  class of defect as a drawing's frame, and a nested group compounds it. Expectation: grouping
  re-anchors with `rebase_at_anchor(CenterOfBounds)` — the new container's translation is the
  parent-frame centre of the children's union, every child placement moves by `-centre`,
  `pivot_local = 0`, and the children's world transforms do not change. `Container::new` stops
  inventing a pivot (a new container is born at zero), so no caller gets a hidden centre; the
  pre-schema-6 migration keeps `default_container_pivot` and no later pass re-normalizes it. Tests:
  a group of two panels keeps their world bounds and reports the centre as its translation; a group
  inside a rotated container keeps world positions and the parent-frame centre; a group of objects and
  panels uses the same union; no child's rotation changes; one undo entry.
  `1e04142`
  `files: crates/model/src/project/node_cmds.rs, crates/model/src/container.rs, crates/model/src/tree.rs`

- `[x]` **X27** the inspector tells the local and the world truth: the Transform card's X/Y/Z is the
  placement *relative to the parent* and the Pivot card's X/Y/Z is the pivot in the node's own frame,
  so at a nested node nothing on screen says where the node is in the world or what point the gizmo
  turns about, and there is no way to zero a nested node's translation. Expectation: the editable
  fields are labelled `Translation (local to <parent>)`, a read-only `World position` row reads
  `project.world_placement`, the Pivot card adds a read-only `Pivot (world)` from `pivot_world`, and a
  new `node.placement.world_zero` command sets `translation` to zero in the parent frame, keeping the
  rotation (the owner split a rotation reset into its own future command); at the root that is the
  world origin, inside a container the read-only row shows the parent's origin. For a panel the pivot
  fields are read-only: `Local pivot = 0` with the note "base corner" and `Pivot (world) = World
  position`, because the pivot is derived from the geometry, not authored. The card never computes a
  world value itself. Tests: the world rows match `world_placement`/`pivot_world` after a gizmo move
  and a rotation about the pivot; world-zero on a root node and on a node inside a rotated parent
  leaves the world rotation composed exactly as before, leaves vertices and `pivot_local`
  bit-identical and is one undo entry; a panel's pivot fields cannot be edited; the command is in the
  registry and refuses the root.
  `62501ad`
  `files: crates/ui/src/placement_card.rs, crates/ui/src/pivot_card.rs, crates/engine/src/node_cmds.rs`

- `[x]` **X26** a panel is anchored at its base corner: a panel is a parametric entity, not a mesh —
  the core draws a closed contour, extrudes the thickness and then explicitly re-anchors the frame with
  `rebase_at_anchor(BBoxBaseCorner)`, so the local zero *is* the base corner (min-X/min-Y for a
  rectangle, the chosen bbox corner for any other ring), `pivot_local = 0`, and `translation` is the
  parent-frame position of that corner. This is the owner's exception to "a closed body pivots at its
  bounds centre", not to "the local zero is the pivot" (ADR 0083). `add_panel_contour` today subtracts
  the ring's minimum corner when it anchors the outline but never adds it back to the placement, so a
  panel made from a scene contour moves; it is rebased instead, and `panel.contour.create` places the
  panel over its source: the source's world placement converted into the target parent's frame, with
  `params.placement` composed on top as a parent-frame offset (identity means "exactly over the
  source"). Machining, banding and the nominal rectangle keep their panel-local frame. Tests: a new
  rectangle panel is at the parent origin with a zero pivot; a ring panel's world vertices equal the
  source contour's and its pivot sits at the bbox base corner; a ring whose bounds do not start at zero
  is anchored without moving a world point; a nested parent and a rotated panel placement both keep the
  world corner equal to `world_placement ∘ (0, 0, 0)`; one undo entry.
  `c992043`
  `files: crates/model/src/panel/mod.rs, crates/model/src/project/panel_cmds.rs, crates/engine/src/panel_cmds/panel.rs`

- `[x]` **X25** a primitive lands under the pointer in a nested container: `place_primitive` handed
  `WorkPlane::placement_at` — a world placement — to `object.primitive.create` as a placement relative
  to the parent, so an armed container that was moved or turned put the cube somewhere else.
  Expectation: the editor converts the placement with the parent's inverse exactly as X24 does, and
  `object.primitive.create` keeps its mesh centred with a zero pivot, so the primitive's translation is
  the parent-frame position of the clicked point. Tests: the same click at the root and inside a
  rotated, moved container puts the primitive on the pointer; its pivot is zero, its mesh bounds are
  symmetric about the local zero, and the document changes once.
  `5370454`
  `files: crates/ui/src/lib.rs, app/src/gizmo_math.rs, crates/engine/src/object_cmds.rs`

- `[x]` **X24** a drawing is born at its own zero: a line drawn at (600, 400) stores
  `translation = (0, 0, 0)` and puts (600, 400) into `pivot_local`, so the card honestly reports a
  translation of zero and the node cannot be moved to the world origin by its translation; a drawing
  made inside a moved or rotated container lands somewhere else, because the editor passes the work
  plane's *world* placement where the command expects a placement *relative to the parent*.
  Expectation: `object.path.create` and `object.extrude.create` rebase the figure with
  `rebase_at_anchor(CreationPivot)` and store `Pivot::ZERO`, so an open line's local zero is its first
  point, a closed ring's is its bounds centre, and every world point stays on the clicked point; the
  editor converts the one world placement the shell hands it into the target parent's frame with
  `Placement::inverse(parent_world)` in `commit_drawing`. `object.path.extrude` turns a node into a
  body in place and leaves its frame alone. The command docs that call the path "local" and the
  `add_object_with_pivot` doc are corrected here; the ADRs that describe the old behaviour are
  rewritten in S6. Tests: a line at (600, 400) has its first point in `translation`, zero in
  `pivot_local` and vertices around zero; a closed rectangle has its bounds centre in `translation`; a
  figure drawn into a rotated, moved container has `world_placement ∘ vertices_local` on the clicked
  points and a parent-frame `translation`; extrusion keeps the frame of the path it grew from; each
  creation is one undo entry.
  `617a973`
  `files: crates/engine/src/object_cmds.rs, crates/model/src/project/object_cmds.rs, crates/ui/src/lib.rs`

- `[x]` **S5** one operation chooses a node's frame: a new `crates/model/src/frame.rs` holds
  `rebase_at_anchor(strategy)` and the three strategies the owner approved — `CreationPivot` (an open
  chain's first point, a closed ring's bounds centre), `BBoxBaseCorner` (the local min-X/min-Y corner,
  or a named corner on request) and `CenterOfBounds` (the centre of a group's children). One rule with
  a small adapter per content kind (a `Path`, a `Contour`, a container's children): move the content by
  `-anchor`, keep the rotation, set the translation to where the anchor was, and leave the pivot at
  zero. `Path::translated([Length; 2])` mirrors `Contour::translated` — both ends of a line move, an
  arc moves by its centre, ids and angles stay — and `Placement::inverse` inverts a rigid frame (the
  conjugate quaternion and `-R⁻¹t`), so `inverse(parent) ∘ world` is the parent-frame placement of a
  world frame; it is the missing half of the existing `Placement::compose`.
  No command calls the new code in this step, so nothing visible moves. Tests: an open chain anchored
  at its first point and a closed ring at its bounds centre keep every world point within a
  micrometre; a rebased contour keeps its segment ids, centres and angles; `inverse` round-trips
  through `compose` and maps a world point back to its local one under a rotated parent;
  `BBoxBaseCorner` answers min-X/min-Y by default and a named corner on request; `CenterOfBounds` of
  mixed panels and objects is the centre of their union. ADR 0082 records the strategies and the rule
  that a frame is chosen only at creation.
  `d6a09a5`
  `files: crates/model/src, crates/model/src/placement.rs, crates/model/src/contour.rs`

- `[x]` **X23** the ribbon's *Close* leaves the ring a draft: a polyline closed from the floating
  tool strip becomes a ring on screen but is never committed, so the closed chain stays a draft with
  the last rubber band still pointing at the strip and the document never learns about the figure
  (observed 2026-11-10; the status bar's *Close* and a click on the first point do commit).
  Expectation: closing is one operation — close, then commit — whichever entry point asked for it
  (ADR 0052: "*Close* closes and commits in one step"). Suspected cause: `Editor::close_drawing` is a
  public primitive that only closes; the pair is written out by hand in the status bar, inferred from
  `is_finished` for a click in the viewport, and forgotten by `ToolAction::CloseDrawing`, while the
  gizmo keeps its cached preview because a closed draft's `preview` answers `Ok(None)`. Fix: one
  shell method `close_draft` owns close+commit, the ribbon and the status bar call it, the gizmo
  reports a close instead of performing it, and `Editor::close_and_commit_drawing` reopens the ring
  when the commit refuses it. The floating strip also leaves a band of itself hoverable, so the rubber
  band can follow the pointer onto the toolbar: the strip blocks the pointer over its whole
  rectangle. Tests: the ribbon reports the close and writes nothing; `close_and_commit_drawing`
  closes and commits as one undo entry and a refused close changes nothing; a closed draft drops the
  cached preview; a pointer over the strip band does not hover the viewport behind it. ADR 0052
  already decides the behaviour, so this adds no record.
  `ed0e8af`
  `files: app/src/main.rs, app/src/draw_gizmo.rs, crates/ui/src/lib.rs, crates/ui/src/toolbars.rs`

- `[x]` **U51** the toolbars float over the design space: the ribbon is an `Order::Middle` overlay
  anchored to the window below the header instead of a top panel, so the scene image runs under it,
  a translucent ribbon blends with the furniture, and resizing a side panel never reflows or jumps
  the strip. The side panels are inset by the reserved band; the orientation widget and *Frame all*
  use the uncovered part of the view, the fit distance scaled by the covered share and the target
  panned half the covered height. The reserved height is deterministic from the open state
  (`toolbars::height`) and the measured rectangle takes over once egui has sized the area. Tests: the
  strip keeps its anchor and width across side-panel resizes and covers the rendered rect; a click on
  it never reaches the viewport; the drawn band equals the reserved one; frame-all keeps the bounds
  inside the uncovered view. ADR 0080.
  `f5270e2`
  `files: app/src/main.rs, crates/ui/src/toolbars.rs, crates/ui/src/theme.rs`

- `[x]` **U63** the display style belongs to one view: `Leaf` carries `ShadingStyle`, the cluster's
  five style buttons read and write the active leaf's style, and `ViewCluster` stops owning a
  global style. `ViewportRenderer::render_views` takes a `ViewSpec { camera, region, shading }` per
  view and builds `StylePasses` inside the pass loop, so *Edges*, *Shaded*, *Wire*, *Hidden* and
  *Ghost* differ between neighbours while one texture and one edge buffer still serve the
  workspace. The diagnostic `all_edges` switch stays global. `SceneFrame::shading` remains the
  single-view fallback and follows the active leaf. Tests: two views keep different styles through
  a frame build and a style change reaches only the active leaf; the renderer's per-view pass
  mapping matches `StylePasses`; opening a scene keeps each layout's styles; the settings default
  applies to a new view and to the active one when changed.
  `009a73f`
  `files: crates/render/src/lib.rs, app/src/view_layout.rs, app/src/view_cluster.rs, app/src/main.rs`

- `[x]` **U62** the active view expands over the whole workspace and comes back: `Layout` gains
  `maximized: Option<LeafId>`; `regions()` returns one full-area rectangle for that leaf and
  `dividers()` is empty, so borders and picking see one surface. `toggle_maximized()` expands the
  active view and restores the tree unchanged — every ratio and camera stays where the user left
  it — and `set_kind` clears the mode. The cluster gains a pressed *Expand* / *Restore* button
  (`view/maximize`, `view/restore` drawn assets), `ClusterAction::ToggleMaximize` and the `Alt+W`
  shortcut; the settings shortcut table names it. A single-view layout disables the button and the
  key does nothing. Tests: expand returns one region on the whole area and no borders; restore
  returns the same four rectangles, ratios and cameras; a layout switch leaves the mode; `Alt+W`
  is consumed and the single layout refuses it; the icon files pass the existing footprint and
  coverage tests and the URI count is 74.
  `2cbc453`
  `files: app/src/view_layout.rs, app/src/view_cluster.rs, app/src/main.rs, crates/ui/src/settings.rs, assets/icons/view, crates/ui/src/icons.rs`

- `[x]` **U61** *Frame all* fits the objects the scene has now: the app stores `scene_bounds` from
  the latest `evaluate`, and `frame_all` computes `scene_build::home_camera(scene_bounds)` on every
  press, then frames only the active view. `Leaf::frame` keeps the view's projection kind: a
  perspective view stays perspective and an orthographic view stays orthographic with the fit
  height, while the direction and roll follow the existing rule. The one-shot `home_camera` field
  and `opening_camera` helper go away; `Layout::new`/`reset_cameras` take the fit camera and the
  opening view, so a standard `Single` view is still the opening preset. Tests: a larger scene
  gives a larger fit distance; a custom orthographic view stays orthographic and fits; a standard
  view keeps its direction and ortho; an empty scene still yields a usable camera. A standard view
  the user made perspective also keeps that projection.
  `33c2275`
  `files: app/src/main.rs, app/src/view_layout.rs, app/src/scene_build.rs`

- `[x]` **U60** every layout remembers its own views: `Layout` holds one `LayoutState` per
  `LayoutKind` (tree, leaves, active, next id) instead of one tree that `set_kind` reshapes. First
  use of a layout builds its standard state: `Quad` = top-left `Front`, bottom-left `Top`,
  top-right `Left`, bottom-right `Axonometric`; `TwoColumns` = left `Front`, right `Axonometric`;
  `TwoRows` = top `Front`, bottom `Axonometric`; `Single` = the settings opening view. Switching
  away and back restores the cameras, presets, active view and every split ratio the user left;
  opening a file refits every stored layout with `reset_cameras(home, opening_view)` while keeping
  each layout's standard presets. The active view is the only target: `main.rs` drops
  `pointer_leaf`/`target_leaf = hovered`; orbit, pan, zoom, `O`, `Home`, presets, selection, gizmos
  and the cluster act on `layout.active()`, a press of any button over a view activates it, a
  border press still resizes without changing it, and the wheel only zooms when the pointer is over
  the active view. Tests: the three split layouts carry their standard views and ortho; a layout
  round-trip restores camera, preset, ratio and active; opening a scene refits every layout; a
  gesture over an inactive view is ignored until a press activates it; a border drag keeps the
  active view. ADR 0079 records the layout state.
  `c014bf9`
  `files: app/src/view_layout.rs, app/src/main.rs`

- `[x]` **U59** the standard views become the six the owner names: `OpeningView` gains `Left` and
  `Bottom` and takes the owner's eye convention — `Front` looks from `+Y` to `-Y`, `Left` from `+X`
  to `-X`, `Top` from `+Z` down, `Bottom` from `-Z` up, `Right` from `-X` to `+X` — while
  `Axonometric` stays the three-quarter default. `ALL`, `label()` and `direction()` carry all six;
  `is_standard()` names the five non-axonometric views. Applying a standard preset sets the camera
  orthographic without moving its target, distance or visible height (`Camera::set_orthographic`
  converts a perspective view by the same height rule as the toggle); the corner widget's axis balls
  do the same through `Leaf::look_from`, so a click on a world axis is an orthographic view. Standard
  views are defaults, not modes: `O` still toggles projection in every view and leaves the preset
  name alone. Tests: every direction and label; `ALL` has six; a standard preset and an axis snap
  are orthographic; an axonometric preset and a free direction keep the projection; the toggle works
  in a standard view and the preset stays; `set_orthographic` preserves the visible height.
  `7dc71d1`
  `files: crates/ui/src/display.rs, crates/scene/src/lib.rs, app/src/view_layout.rs`

- `[x]` **U58** the corner widget gains the roll ring and its step setting: a functional ring on a
  52 pt radius in a 124 pt square rotates the camera about its view axis, always visible with the
  widget, with four fixed nodes at the top, left, bottom and right carrying 0°, 90°, 180° and 270°
  and an accent handle at the camera's roll. A node click sets the absolute roll (0° resets); a drag
  anywhere on the annulus turns the camera with the pointer, quantised to the step from the workspace
  settings. The drag accumulates the unwrapped visual angle and applies the difference from what was
  already applied, so a slow drag never loses a step and the ±180° seam does not jump; the axis balls
  keep their hit squares and win an overlapping hit, and a press on a node never starts a hand turn.
  `Gizmo` owns the drag state; `GizmoAction` gains `RollTo(Degrees)` and `RollBy(Degrees)`. The shell
  passes the step and keeps the ring's drag out of the armed navigation and middle-button orbit.
  `ViewportSettings.roll_step_degrees: u32` defaults to 1, clamps to `1..=15` and is drawn in the
  Viewport section as a `DragValue`; the settings JSON needs no version bump because the container
  already has `#[serde(default)]`. Tests: node positions and degrees; a 37° drag at step 1 gives 37°
  and at step 5 gives 35°; a sub-step drag does nothing; the remainder carries; the seam does not
  jump; a node click wins over a drag; the ring hit excludes the balls; the handle follows the roll;
  orbit keeps the roll and a preset resets it; a ring drag is reported to the shell so it cannot
  orbit the viewport; the setting clamps and applies live.
  `8c329ad`
  `files: app/src/gizmo.rs, crates/ui/src/settings.rs, app/src/main.rs`

- `[x]` **U57** the camera rolls about its own view axis: `Camera` has no roll, so `right()` is
  horizontal, `up()` follows yaw and pitch only, and the readout's roll is always zero. Add a
  `roll: f64` (radians, view state like yaw and pitch, never document data): rotate the analytic base
  `right`/`up` about `forward` by the roll, keeping `up = right × forward`; a positive roll turns the
  image counter-clockwise. `roll()`, `roll_to(f64)` and `roll_by(f64)` normalise into `0..TAU`,
  ignore non-finite input and refuse a pinned `FixedBasis`, like `orbit`. `look_from` and
  `look_from_oriented` reset the roll; `orbit`, `pan_pixels`, `zoom`, `toggle_projection` and
  `with_projection` keep it. Presets, Home, Frame all and axis balls reset through
  `look_from_oriented`, orbit keeps. The readout takes `signed_degrees(camera.roll())`; ADR 0078
  records why the field is on the camera rather than a post-multiplied matrix, and ADR 0057 notes
  that a pole's canonical up is the zero-roll case. `Leaf::roll_to`/`roll_by` land with U58 because
  they would be dead code until the ring can call them.
  Tests: +90° sends `right` into `up` and the image of a world point from right to up; the view
  matrix stays orthonormal; picking and panning follow the rolled basis; a pole rolls without moving
  forward, eye or target; snaps reset while orbit keeps; a pinned camera ignores the roll; the
  readout shows the field.
  `6cd7ee6`
  `files: crates/scene/src/lib.rs, app/src/view_layout.rs, app/src/gizmo.rs`

- `[x]` **U56** the projection toggle joins the corner widget as a flat icon: the corner orientation
  widget has no projection control, and its Home button is the only control under the cube, drawn as
  a filled plate. Add a 20 pt projection icon in a 22 pt cell directly under the cube, centred, with
  no filled cell at rest — only the glyph — and a faint rounded wash on hover or press; a placeholder
  asset falls back to a frameless text button. It is absent with the widget while an authored camera
  is active. `GizmoAction::ToggleProjection` joins `LookFrom` and `Home`; the shell factors the two
  existing projection arms into one `toggle_view_projection(leaf)` helper, so the ribbon, the
  Display popover and the new button cannot disagree. The readout moves one row down and
  `widget_rect` grows by the button row; the stale "Home is under the cube" comments are fixed.
  Tests: the projection rect is inside the widget and below the cube; a click returns
  `ToggleProjection`; the icon and hint follow the projection; a flat button paints no filled rect
  at rest and one on hover; an undrawn placeholder draws its label.
  `879f456`
  `files: app/src/gizmo.rs, crates/ui/src/icons.rs, app/src/main.rs`

- `[x]` **X20** a pole view has a canonical up: clicking the top view inherited whatever yaw the
  camera had, so the elevation arrived rolled. `Camera::look_from_oriented(direction, up)` takes the
  roll from a world up when the direction is a pole and keeps the existing yaw rule otherwise, and
  `canonical_up` is `+Y` for the top and bottom views and `+Z` for the rest. The widget's
  `GizmoAction::LookFrom`, `Leaf::apply_preset`, `Leaf::look_from`, `Leaf::frame` and
  `opening_camera` use it, so a top view looks down with `+Y` up and `+X` right, a bottom view looks
  up with `+Y` up and `-X` right, and the side and front views keep `+Z` up; the target, the distance
  and the projection do not move. ADR 0057 records the snap. Tests: an arbitrary yaw still lands on
  the canonical basis at both poles; a side view ignores the requested up; a pole with an up
  parallel to the view keeps its yaw; a preset and the widget produce the same camera.
  `b6a974f`
  `files: crates/scene/src/lib.rs, app/src/view_layout.rs`

- `[x]` **X19** the extrusion pulls a phantom body: the arrow detached from the body while the pull
  was open and the body only appeared after release. The pull now draws a wireframe prism at the
  preview height — the sampled contour at the base and at the preview, joined by a vertical edge at
  every sampled corner — and the arrow sits on its top face, so the preview moves with the pointer
  while the document keeps the last committed height. `Body` keeps the contour's sampled points,
  `ghost_edges` builds the prism, `paint` draws it before the arrow in a translucent accent, and
  Escape drops the pull without touching the body. Tests: the ghost top is at the preview height
  and the base at zero; a drag changes the ghost with no revision or undo entry; release writes one
  height command and one undo restores it; Escape keeps the committed height. A solid preview is
  recorded in `plan/debt.md`.
  `efb153c`
  `files: app/src/extrude_gizmo.rs`

- `[x]` **X18** a figure's geometry is editable in its card: the object card had no radius for a
  circle, no width and height for a rectangle and no side count for a polygon, and a constructive
  body's drawn path was not in the summary at all. The card now shows the fields the figure has —
  circle radius, rectangle width and height, polygon sides (`3..=128`) and radius, a single arc's
  radius and end angle — for a `Path` node and for the constructive body that owns a path, and Apply
  writes one command with one undo entry. `Constructive::path()` already existed; the summary now
  carries it, and a *Figure* section reads the fields through
  `circle_frame`/`rectangle_frame`/`polygon_frame`/`arc_frame` and writes them through the existing
  model setters. The edit dispatches `object.contour.replace` for a body and `object.path.replace`
  for a path node, both of which store the `Path` itself, so the shape tag and the segment ids
  survive; the fields are disabled while the viewport owns the working copy. Tests: each field
  reshapes the path and keeps its tag; a constructive body keeps its tag; a card edit during a
  viewport edit is refused; one undo restores the figure.
  `073fdb6`
  `files: crates/ui/src/object_card.rs, crates/model/src/object.rs, crates/engine/src/object_cmds.rs`

- `[x]` **X17** along-contour movement has a length, a stop and a collapse: the mode shows no length
  field, the node can pass the opposite node, and a zero-length segment cannot be finished.
  Expectation: a length field shows the distance from the segment's opposite end and accepts a
  typed length; the node cannot pass the opposite node; at 0 the corner reaches it, and Apply
  removes the zero-length segment and joins the two nodes; a ring keeps at least three segments.
  Suspected cause: `numeric_entry` opens only when `grip_number` returns a number, and an untagged
  polyline corner has none, while an arc corner numbers an angle or a radius; nothing clamps at the
  opposite node and nothing records a collapse. Fix: build the entry from the drag motion (`Length`,
  label `L`, value from the other end, Enter moves the corner that far along the support, arc length
  for an arc); clamp to `0..=segment_length`, record `collapse` on `ContourEdit`, remove collapsed
  segments in `apply`, refuse a ring below three, and keep one undo entry. Tests: a typed length
  moves the corner; a drag cannot pass the opposite node; reaching 0 collapses and Apply joins the
  nodes; a ring of three refuses; the undo entry is one.
  `fc2d1f3`
  `files: app/src/contour_gizmo.rs, crates/model/src/contour_edit.rs, crates/ui/src/contour_edit.rs`

- `[x]` **X16** a rectangle and a polygon keep their figure: dragging a side midpoint collapses a
  rectangle to a line, a rotated rectangle turns into a parallelogram and a corner drag then yields
  a width of thousands of millimetres. Expectation: a midpoint drag changes one extent and never
  makes it zero, a rectangle keeps right angles and its axes at any angle, a corner drag stays in
  the document range, and a figure edited along a side stops being that figure and becomes a plain
  polyline. Owner decision: a midpoint crossing the opposite side clamps at a minimum extent of
  1 mm instead of mirroring. Suspected cause: `move_rectangle_midpoint` refuses a non-positive half
  but leaves a sliver; `rectangle_frame` never orthogonalises the axes and
  `PathShape::fits(Rectangle)` only counts four lines; `move_rectangle_corner` does not clamp;
  `rebuild` keeps the tag through an along-contour edit. Fix: orthonormalise the axes and require
  right angles within a micrometre, clamp the midpoint at the minimum, clamp the corner, and drop
  the tag on a topology change. Tests: a rotated rectangle keeps right angles through a midpoint
  drag; a midpoint drag never yields zero; a corner drag cannot leave the range; an along-contour
  edit drops the tag; a tagged rectangle that is not right-angled is a load error.
  `bf07643`
  `files: crates/model/src/contour_edit.rs, crates/model/src/contour.rs, crates/ui/src/contour_grips.rs`

- `[x]` **X14** the grip set matches the figure: a regular polygon shows a phantom node outside its
  first side whose drag cannot be finished, and dragging a circle's rim deforms one quarter until
  the path is invalid. Expectation: a polygon shows its vertices and centre only and takes its
  radius from a vertex and from the card; a circle can be pulled from any point on its rim in any
  mode and only its radius changes; no grip sits off the figure. Suspected cause:
  `PolygonFrame::radius_point` sits on the circumcircle midway between the first two vertices,
  outside the first side; a tagged circle is four quarter arcs, and `grip_at` offers the synthetic
  `ArcRadius` on any hovered arc, which deforms one quarter. Fix: drop
  `ContourGrip::PolygonRadius` from the grip list and its number/value/label/drag/set tables; add a
  circle-rim grip for a `PathShape::Circle` draft that calls `set_circle_radius_at` and numbers `R`;
  an untagged arc chain keeps `ArcPoint`/`ArcRadius`. Tests: a circle pulled at 37° stays round and
  takes the radius from the centre; a polygon has no grip off the figure; a vertex still scales it;
  an arc chain keeps its arc grips.
  `916b15e`
  `files: crates/ui/src/contour_grips.rs, crates/model/src/contour_edit.rs, app/src/contour_gizmo.rs`

- `[x]` **X15** the pointer grabs the node it is on: a press that visually lands on a grip often does
  not grab it, and the cursor turns into a hand over a node. Expectation: a press within the grip's
  hit radius grabs it even when the pointer moves a few points before egui's drag threshold, and
  the cursor stays the default arrow. Suspected cause: `ContourGizmo::update` resolves the grip from
  the current pointer when `drag_started` fires, which is after the threshold, and `cursor()`
  returns `Grab`/`Grabbing`/`PointingHand`. Fix: resolve from `pointer.press_origin()`, keep the
  grabbed grip for the whole drag, keep the 9–12 pt hit radius and let the enlarged hovered grip
  carry the feedback; `cursor()` returns `Default` and the shell's contour branch goes. Tests: a
  press exactly on a grip grabs it; a press 12 pt away does not; a press followed by a 5 pt move
  before the threshold still grabs; the cursor is the default.
  `52f1d0f`
  `files: app/src/contour_gizmo.rs, app/src/main.rs`

- `[x]` **X13** an arc follows its nodes: an arc cannot be reshaped by dragging its nodes — the
  endpoints only slide along the arc and change its angle — and dragging the middle node makes the
  radius jump to an absurd value. Expectation: every node follows the pointer; a free endpoint
  rebuilds the adjoining arc through its other end and its current mid point, the mid node rebuilds
  it through both endpoints and the new mid point, the centre still moves the whole arc, and
  sliding along the arc stays an explicit mode. Suspected cause: `ContourGizmo::dragged_point`
  projects every free corner onto an adjoining arc's circle, and the `ArcPoint`/`ArcRadius` drags
  call `Path::set_arc_radius_at`, which takes the radius from the pointer's distance to the centre.
  Fix: project only for `CornerMove::Along`; rebuild in `Path::free_corner` and a new
  `set_arc_through_midpoint`; refuse a degenerate rebuild, keep the last valid arc and cap the
  radius at the document range. Tests: a free arc end follows the pointer and changes the radius;
  the other end stays; the mid node stays finite; a collinear mid point is refused; the along mode
  keeps centre and radius. ADR 0075 is updated.
  `64c11d3`
  `files: crates/model/src/contour_edit.rs, app/src/contour_gizmo.rs`

- `[x]` **X12** a drawing tool can be switched and cancelled: choosing another drawing tool while one
  runs does nothing, Escape does not reset the tool, and only Ctrl+Esc discards. Expectation:
  choosing another tool ends the running one and starts the new one, Escape cancels the draft and
  leaves the tool with nothing written, Enter and the right button commit a valid draft, the
  drawing shortcuts work while a draft runs, and the status line names the keys. Owner decision: a
  tool switch discards the running draft, because nothing was written and undo gains no half
  figure. Suspected cause: `Editor::start_drawing_with` refuses while `drawing.is_some()`, the shell
  skips the shortcut table while a draft runs, and Escape and Enter both commit (U53/ADR 0076).
  Fix: replace the draft instead of refusing, route Escape to `cancel_drawing`, keep Enter and the
  right button on commit, and let a failed commit keep the draft and the tool. Tests: start a
  polyline, press R, the rectangle tool is live with an empty draft; Escape leaves the tool and the
  document untouched; Enter commits; a shortcut starts a tool while a draft runs; a failed commit
  keeps it. ADR 0076 is updated.
  `62cfc45`
  `files: crates/ui/src/lib.rs, app/src/main.rs, app/src/draw_gizmo.rs`

- `[x]` **X21** a polyline closes through the arc it is drawing: with arc mode on and the arc's start
  and middle already placed, the *Close* button refused with "the arc is still waiting for its end
  point", so the ring could not be finished by that arc, and the first node was not highlighted as
  the closing joint while the arc waited. Expectation: *Close* completes the pending arc from the
  chain's stored end through its middle point to the first start and closes the ring, both close
  buttons enable from two segments on — the count a closing segment already needs — and the first
  node turns green when a click there would close the chain, in arc mode too. Suspected cause:
  `DrawDraft::close` refused while `pending_arc.is_some()`, `DrawDraft::can_close` demanded
  `pending_arc.is_none()` so the green first-point highlight and the ribbon's *Close* were off, and
  `draft_can_close` plus the status bar demanded three segments. Fix: `closing_segment` builds the
  pending arc as the closing segment through the first start and `close`/`to_path` share it,
  `can_close` counts the pending arc, both close buttons enable from two segments through
  `draft_can_close`, and `closes_at` lights the first node. Undo reopens a closed chain before it
  takes the waiting arc; `push` refuses a closed draft first. Tests: a two-line chain with a pending
  arc closes and reaches a contour; its closing segment is the arc; a degenerate closing arc is
  refused and leaves the draft editable; undo keeps the closing arc; the first node lights up with a
  pending arc; both Close buttons enable from two segments; the gesture commits one closed path and
  one undo entry. ADR 0077.
  `8ff248f`
  `files: crates/ui/src/contour_edit.rs, app/src/draw_gizmo.rs, crates/ui/src/toolbars.rs, app/src/main.rs`

- `[x]` **X11** a polyline closed by an arc is one ring: a chain whose last segment is an arc never
  closes — the third click on the first point does not finish it and the command reports "path
  segment 2 does not meet the next segment" — and the arc preview blinks at an intermediate click.
  Expectation: an arc's derived endpoints join their neighbours within the drawing tolerance, a
  closing click in arc mode ends the arc on the first point and closes the ring, and the preview
  keeps the pending arc and never blanks. Suspected cause: the joint tolerance is a constant
  micrometre (ADR 0072), while an arc endpoint is derived from a rounded centre, radius and angle
  and misses by `radius × 8.73e-6` — about 5 µm on a 600 mm arc; `DrawDraft::finish_arc` sets
  `closed` without validating the ring, and `preview` clears the cache when it returns `Ok(None)`.
  Fix: a pair rule, `1 µm + arc_slack(first) + arc_slack(next)` with `arc_slack = radius × 8.73e-6`
  (zero for a line), used by `Path::assemble`, `Contour::assemble` and `to_geometry`; `finish_arc`
  closes through `close()`; `preview` keeps the last valid path and paints the pending arc.
  Tests: a 600 mm arc closes a polyline and reaches a contour; the same ring extrudes; a 20 µm gap
  on a 100 mm arc is still refused; a rejected preview keeps the last valid one; a 1 µm gap loads.
  Correct ADR 0072; note explicit arc endpoints (schema 20) in `plan/debt.md`.
  `f3123d7`
  `files: crates/model/src/contour.rs, crates/ui/src/contour_edit.rs, app/src/draw_gizmo.rs`

- `[x]` **U55** a fresh extrusion pulls its height in the viewport: the ribbon creates the body at
  the last height used in the session (100 mm at first) and reports it, so an arrow runs from the
  contour's bounds centre along the body's local `+Z`. Dragging it writes one
  `object.extrude.height.set` on release and keeps the pull open; a bare click or Enter finishes,
  Escape drops the pull at the height the document already holds, and a typed number applies the
  same way. The height is session view state until the command, never a document default. Tests: a
  drag maps to millimetres along the body's own axis, a click finishes at the pull's height, a
  pulled height becomes the next extrusion's default, and a path has no pull.
  `5efa195`
  `files: app/src/main.rs`

- `[x]` **U54** every figure exposes its grips: a circle its centre and four quadrants, an arc its
  centre, a midpoint and a radius grip on a hovered curve, a rectangle its corners, four side
  midpoints and centre, a regular polygon its vertices, centre and radius point. A number field
  follows a grip drag and names its own dimension — a radius for a circle, arc or polygon, an angle
  for an arc end, `W` or `H` for a rectangle side; Enter applies it without leaving the edit and Esc
  restores the drag-start value. Free movement is the default, an HUD toggle switches to the segment
  the pointer picked at the drag start, and a tagged figure keeps its constraint through every drag.
  The polygon side count stays in the object card. Tests: a quadrant drag keeps the circle round; an
  arc centre moves the arc; a rectangle midpoint changes one extent; a polygon vertex keeps every
  side; a numeric entry parses its kind and applies it.
  ADR 0075 — `1321fee`
  `files: app/src/contour_gizmo.rs`

- `[x]` **U53** the drawing tools that finish a shape: `DrawTool` gains arc from centre (three
  clicks), circle through three points, rectangle from its centre (two clicks), a directed
  rectangle as its own button (centre → side direction → corner, `Shift+D`) and a regular polygon
  (centre plus a rim point, `N` from the ribbon, `3..=128` sides). Tab switches polyline input
  between a straight segment and an arc and Alt ignores the end and grid snaps; the closing segment
  of a polyline is an arc when the closing click is taken in arc mode, and it always starts at the
  stored end. A preview that cannot become a path keeps the last valid preview and the raw chain
  stays drawn, so a drawing never blinks out at an intermediate click; the status line names the
  reason instead. Escape or the right button commits, Ctrl+Esc or Cancel discards, and a committed
  figure opens its own edit mode. Tests: Tab builds a straight then an arc segment; an arc closes
  the chain; a degenerate arc step is refused; each new tool builds and commits its path; an
  invalid intermediate preview keeps the previous one; a committed shape is in edit mode.
  ADR 0076 — `9778d35`
  `files: crates/ui/src/contour_edit.rs`

- `[x]` **A11** an extrusion owns the path it grew from: `Constructive` stores `{ path, features }`
  and `Extrude` keeps only its height, so the contour is no longer copied and "absorbed" means the
  body holds the one path. `object.path.extrude` turns the selected `Path` node into a constructive
  body in place (no sibling, no orphan), `object.contour.replace` edits the path inside the body and
  the mesh cache keys on it, and `object.extrude.height.set` drives the pull. Schema 19 migrates a
  version 18 `extrude.contour` into `constructive.path`. Tests: extruding a path replaces the node
  and leaves no second object; editing the contour inside the body rebuilds its mesh; the height
  command keeps the path; an old file loads into the new shape and saves without a contour copy.
  ADR 0074 — `c98f590`
  `files: crates/model/src/object.rs`

- `[x]` **A10** a corner moves freely or along the segment picked at drag start: `move_corner_at`
  takes a `CornerMove` (`Free` or `AlongContour`) plus the target resolved once by the caller from
  the initial drag vector, so a junction does not change its mind mid-drag. A free arc endpoint takes
  the point and rebuilds the arc through the other end and its current mid-arc point, so the radius
  follows the drag and the arc can be shortened; a degenerate step is refused and the grip stays on
  the last valid arc. Numeric grip values land in the document layer: arc radius and end angle,
  circle radius, rectangle width and height, polygon radius. Tests: a free arc end changes the
  radius; the arc can be shortened to its chord; a degenerate target is refused; the along-contour
  target is the segment chosen at the start; each numeric value keeps the figure's own constraint.
  `9553175`
  `files: crates/model/src/contour_edit.rs`

- `[x]` **A9** a path remembers the shape it was drawn as: `Path` gains an optional `PathShape`
  (`Rectangle`, `Circle`, `RegularPolygon`) at schema 19, an edit-intent tag rather than stored
  dimensions — the centre, radius and side count are derived from the segments, the tag only says
  which constraint set the grips apply. Structural constructors land with it: arc from centre
  (centre, start, end), circle through three points, rectangle from its centre, rectangle from
  centre plus a side direction, and a regular polygon from centre, rim point and `3..=128` sides.
  A topology change (split, join, dropped segment) clears the tag; a tag that contradicts the
  geometry is a readable load error, not a silent reinterpretation. Tests: each constructor sets its
  tag and its geometry; a rectangle edited by a corner keeps the tag; a split clears it; a version 18
  file loads without a tag; an unknown tag is an error.
  ADR 0073 — `9d7476f`
  `files: crates/model/src/contour.rs`

- `[x]` **A8** one joint tolerance for a ring: `Contour::assemble` compares a joint with the same
  one-micrometre bound `Path` already uses instead of demanding exact equality. An arc stores its
  centre, radius and angles, so its endpoints are derived and can miss the drawn point by a
  micrometre after rounding, while `Path` accepted the joint and `Contour` refused it: a polyline
  closed with an arc failed with "path segment 2 does not meet the next segment", the arc-arc joint
  recorded in `plan/debt.md` failed the same way, and `DrawingDraft::closing_segment` could only add
  a straight line. One rule, stated in ADR 0072, covers the path, the ring and `to_geometry`.
  Tests: a ring whose arc endpoint misses the next start by one micrometre loads and converts to
  geometry; two micrometres still fail; an arc that closes a polyline builds.
  ADR 0072 — `a00cb6f`
  `files: crates/model/src/contour.rs`

- `[x]` **U44** a ground shadow: every object that rests on the floor casts a soft contact shadow, so
  a cabinet reads as standing on something instead of floating in a grid. `ContactShadow` is frame
  data in `crates/scene` beside `Grid`: a node `Id`, a centre on the ground plane and two half-axes
  derived from the object's world bounds, covering 92 % of its footprint so the ellipse stays inside
  the outline. One shadow per root node whose solid subtree touches the floor, with a 0.5 mm
  tolerance; a path or drawing on the work plane casts nothing. The renderer draws one unlit
  transparent triangle fan per shadow with a radial falloff in the shader — no shadow map, no extra
  pass, no texture — on a plane just below the grid, and the switch and strength travel in the camera
  uniform, so toggling uploads nothing. A checkbox and the strength slider sit in the display-settings
  popover. Tests: a bound with no floor contact draws nothing; the ellipse stays inside the footprint;
  the switch empties the pass; the sample scene emits exactly one shadow. ADR 0071 — `4e0348f`
  `files: crates/scene/src/lib.rs, crates/render/src/lib.rs`

- `[x]` **X10** every triangle edge is drawn, and far edges show through near panels: a flat panel
  draws the diagonals of its triangulation instead of its outline, and a panel a metre behind another
  shows its edges through it in *shaded with edges* and *wireframe*. A dark viewport draws dark ink,
  so edges vanish on the default background. Expectation: the default edge pass draws form edges only
  — a boundary or a real crease, so a box shows twelve, not eighteen, and a triangular body nine —
  the full triangulation is a switch in the display settings, the ink is light on a dark viewport,
  and an edge behind a face is not drawn unless the *hidden edges* style is on, which shows one
  object's own inside but never the object behind it.
  Suspected cause: `crates/mesh` deduplicated edges by position but kept every one, so coplanar
  diagonals survived; the `vs_edge` shader moved clip-space depth by a constant, which is ~90 mm of
  world offset at one metre and >1 m at five, so a far line overtook the near face in front of it;
  `edge_color` was a single dark constant; and the hidden pass compared depth with `Always` and
  nothing else, so it drew through every object.
  Tests: a coplanar quad drops its diagonal and keeps four boundary edges; a 90° fold keeps its
  shared edge; a cube yields twelve edges and eighteen in the diagnostic list; a triangular prism
  yields nine; an occluded drawable's edge slice is separate from the near one and carries its own
  stencil slot; the face pass owns the depth bias and stamps the slot; the hidden pass accepts only
  its own slot; the ink follows the background. ADR 0070 — `afeba77`
  `files: crates/mesh/src/lib.rs, crates/render/src/lib.rs, crates/scene/src/lib.rs`

- `[x]` **U43** the display styles the bar offers become real: *shaded with edges*, *shaded*,
  *wireframe*, *shaded with hidden edges* and *translucent*. Edges are derived data — one deduplicated
  edge list per mesh, cached in `crates/mesh` against the same derivation inputs as the mesh itself —
  and drawn as a line pass with a depth bias, because `PolygonMode::Line` is unavailable on Metal and
  a 1 px line cannot be widened (ADR 0059). *Shaded with hidden edges* draws the edge pass with
  `CompareFunction::Always`; *translucent* draws with blending and no depth write and orders objects
  by their centre, which is honest for a cabinet of flat panels; *wireframe* draws the edge pass
  alone. `ShadingStyle` lives in `crates/scene` as frame data and the renderer interprets it, so a
  test can build a frame without a GPU. The bar's buttons and the settings default stay in step, and
  a style change re-uploads nothing but the style. Tests: an edge list deduplicates a shared edge and
  keeps a boundary edge; each style maps to the pipeline state it promises; a translucent pass writes
  no depth; a wireframe frame draws no triangle. ADR 0070 — `fc585cc`
  `files: crates/mesh/src/lib.rs, crates/render/src/lib.rs, crates/scene/src/lib.rs`

- `[x]` **X9** a split viewport draws each camera in the wrong half: with two views stacked or four in
  a grid, orbiting, panning or picking in the top view moved the bottom one, and clicking an axis on
  the orientation widget in one view snapped its neighbour. Expectation: a view draws the camera its
  own rectangle belongs to, so a gesture and a widget click act on the view under the pointer, in
  both directions and in all four quadrants. Suspected cause: `ViewRegion::scissor` flipped the
  region's `y` as though the framebuffer grew upwards, while the pointer was tested against the
  top-left rectangles the shell paints; a one-view workspace hid it because the whole target is one
  region and a flip of it is the identity — `6690ae6`
  `files: crates/render/src/lib.rs, app/src/main.rs`

- `[x]` **U42** the workspace divides into views: the central area holds a layout tree of leaves and
  splits — one view by default, then *two vertical*, *two horizontal* and *four* from the bar, and
  any border between two views is draggable with a minimum size of 120 pt per side and a ratio
  clamped to 0.15–0.85. Each leaf owns its camera and its preset (*Front*, *Top*, *Right*,
  *Axonometric*), *Home* and *Frame all* act on the view under the pointer or on the active one, and
  the active view carries a thin accent frame. The layout, the cameras and the active leaf are view
  state in the shell and never reach the document; a preset writes yaw and pitch only, so switching
  views never moves the model. The orientation widget gains a *home* button that returns the active
  leaf to *Axonometric*, and an optional readout under it — `Pitch 32.1° · Yaw -51.7° · Roll 0.0° ·
  Ortho` — drawn only when the switch U40 added to the display-settings popover is on; the numbers
  come from `Camera::pitch`/`yaw` and `Projection`, formatted through `units`, so no new
  document data and no second source of truth. The renderer draws one offscreen target and one view
  per pass with a scissored region and a uniform slot per view, rather than one texture per view, so
  memory does not grow with the grid and egui keeps a single texture id. Tests: a split and a join
  round-trip; a drag clamps at both ends and never collapses a view; a preset produces the expected
  orthographic basis; four views produce four scissor rectangles that tile the area exactly; the home
  button restores the axonometric preset; the readout is hidden when its switch is off and names
  `Ortho` or `Persp` in step with the projection.
  Session notes: `viewport()` owns the single target and calls `resize` +
  `update_egui_texture_from_wgpu_texture`. A split view needs a scissored pass per leaf into the same
  target (`set_scissor_rect`), and `ViewportRenderer` writes one `CameraUniform` per
  `render_with_lighting`, so a slot array with a dynamic offset keeps one buffer. Respect ADR 0049 per
  leaf: an authored camera pins its own projection. Reuse `scene_build::home_camera` for a leaf's fit —
  `502f870`
  `files: app/src, crates/render/src/lib.rs, crates/scene/src/lib.rs`

- `[x]` **U47** a command palette on `Ctrl+K`: one search field over `CommandRegistry::specs()`,
  listing a command's label, its id and whether it is enabled for the current session; the chosen
  entry dispatches it with parameters built from the current selection, and an entry that cannot run
  says why instead of failing silently. The palette is the reference's "command search" and it costs
  no new command: the registry already carries 48 specs with a stable id, a label and a pure
  `enabled` check, so the work is a fuzzy filter, a keyboard loop (`↑`/`↓`/`Enter`/`Esc`) and one
  parameter builder per command family — node deletion, duplication, renaming and the view actions
  first, with the rest reported as "needs a selection" until a builder exists. Command history and
  the last-used list are session state, never document data. Tests: a prefix and a substring both
  find `node.delete`; a disabled command is listed but not runnable and explains itself; `Esc`
  closes without dispatching; a chosen entry dispatches exactly one command and one undo entry.
  Session notes: `CommandRegistry::specs()` returns `&'static [CommandSpec]`, `spec.id()` and
  `spec.label()` are the search fields and `spec.enabled(session, &params)` is the pure check;
  `crate::command::dispatch` in `crates/ui` is the one dispatch path. `Ctrl+K` is free — `F`, `O`,
  `G` and the drawing letters are taken, and `ui.input(|i| i.modifiers.command)` must not fire while
  a text field has focus.
  `8066ec8`
  `files: app/src, crates/ui/src`

- `[x]` **U41** the settings window and the one place the workspace is remembered: a *Settings*
  modal with the sections *Appearance* (theme dark/light/system, accent, UI scale — the control U40
  left without a surface moves here), *Viewport* (default display style, grid on and its
  step, edges, the view a new viewport opens with), *Ribbon* (background opacity) and *Shortcuts*
  (the existing table). The window is a view, not a document surface: a change applies at once and
  never touches the `.fcs`, and a scene saved by another machine carries none of it. The state is
  written as JSON under the operating system's config directory through a small `settings` module
  with an atomic write, the same shape `project` uses, so a crash during a save cannot
  leave a half-written file; a missing, unreadable or older file falls back to the defaults with the
  reason in the status bar instead of a panic. The same module gains the *Home* surface that the
  header's house button opens: new, open, the recent files and the empty template, drawn over the
  workspace and dismissed by Escape. Tests: a round trip through JSON restores every field; a
  truncated or unknown-version file yields defaults; a value outside a range is clamped on read; the settings never appear in a serialised document.
  `c83ace5`, `bbef3e2`
  `files: app/src, crates/ui/src`

- `[x]` **U40** the side panels become windows that fold and the viewport gets its floating cluster:
  the scene tree and the inspector each get a header row — a title, a count chip and a fold chevron —
  and folding them leaves a 26 pt vertical strip with the icon and the rotated title, which a click,
  a double-click on its edge or a drag outward restores. The tree keeps only the tree — no tools, no
  heading of its own — and still supports selection and the context menu. The view controls become a
  translucent cluster floating over the centre of the viewport, anchored `CENTER_BOTTOM` with a small
  offset: *orbit / pan / zoom* · *frame all / home* · the display-style buttons · the layout entry and
  the three-slider display settings popover, which also carries the *show projection and angles*
  switch that draws the pitch/yaw/roll and the projection word under the orientation cube (U42). The
  bottom status bar stays thin and holds only the armed tool, the gesture it expects, the counts and
  the last error, so the viewport starts at the scene and not at a wall of labels. A row of buttons is
  not foldable; only a panel with content is. Tests: folding and restoring a panel keeps its width;
  an armed container still names itself in the bar and clears from it; a closed panel draws no body;
  the status line names the armed tool and its gesture; the cluster is anchored to the bottom centre
  and does not move when the side panels resize; the settings popover toggles the angle readout —
  `4934ebf`
  `files: crates/ui/src/lib.rs, app/src/main.rs`

- `[x]` **X7** a ribbon button is one measured cell, so a caption never sizes it: the tool row of
  every tab draws buttons of one width and one height, whatever their caption and whatever the
  window width, and a caption that does not fit is elided with `…` instead of widening the button.
  A placeholder asset keeps the same cell and draws its caption as the face, so a half-drawn icon
  set has one rhythm. The vertical divider between two groups spans the whole body — the tool row
  and the caption band — instead of the 16 pt header hairline, and the header's own row follows the
  same rule: `File` and the icon buttons are one 26 pt control, and its hairlines are as tall as they
  are. Observed: the Sketch row had buttons from 41 to 83 pt wide, Rectangle was the widest and the
  Arc tool the narrowest, the divider stopped in the middle of the row, and a maximised window
  stretched every caption into its own width. Expected: one cell, one pitch, one divider height, and
  captions that read as a column. The cell is measured once per row from the window and clamped to
  a minimum and a maximum — the owner's second observation — and the minimum is what the longest
  Russian caption needs, which is the U49 requirement. Tests: every caption of the ribbon yields the
  same cell width and height; a caption past the inner width is elided; the Russian tool captions fit
  the narrowest cell; a placeholder occupies the same cell as a drawn icon; a narrow window gives the
  floor and a maximised one the ceiling; the group divider is as tall as the tool row plus its caption
  band; every tab's column count is right. ADR 0067 — `7d4895c`
  `files: crates/ui/src/icons.rs, crates/ui/src/theme.rs, crates/ui/src/toolbars.rs`

- `[x]` **U39** the ribbon replaces the strip that lives inside the scene tree: one row of tabs
  (*Furniture, Hardware, Model, Sketch, Scene, View*) sits under the header, and the body under it
  draws the tool groups of the active tab separated by vertical dividers, heading each group
  underneath in 10.5 pt caps. A tab whose every group is empty is not drawn at all, so no greyed
  "coming soon" tool ever appears; the tab that is open is the accent, a collapsed ribbon keeps only
  its tab row, and clicking the active tab or double-clicking its label collapses and restores the
  body. The groups map onto the actions that already exist — Sketch: line, polyline, arc, rectangle,
  circle, close, cancel; Model: cube, cylinder, sphere, cone, extrude; Scene: panel, assembly,
  camera, light; View: frame all, projection, grid — and the numeric fields (primitive size,
  extrusion height) become a labelled field inside their group rather than a floating row. A button
  in the ribbon is a *labelled* button: a 22 pt icon over a 10.5 pt caption, on a 48 pt row, with the
  group's own 14 pt caption underneath, so the body is about 90 pt tall like the reference the owner
  showed. `theme::RIBBON_ROW_HEIGHT` grows to 48, `theme::RIBBON_CAPTION_HEIGHT` is added, and a new
  `icons::ribbon_button` draws icon-plus-caption; `icons::tool_button` stays the icon-only button for
  the header, the panel headers and the floating viewport cluster, because a caption there would
  double a 26 pt row. The armed drawing tool or primitive still draws pressed, and the creation chip
  moves to the status bar (U40). Everything is view state: no command, no schema, no undo entry.
  Tests: every implemented action appears in exactly one group and every group has at least one
  action; an empty tab is not drawn; collapsing keeps the tab row; the pressed tool and the disabled
  extrude hint survive the move; a labelled button paints both its icon and its caption.
  — `36a9763`

- `[x]` **U38** the window reads as one frame: one theme owns the colour and the rhythm, and chrome
  is separated from content. `crates/ui/src/theme.rs` holds the palette (chrome, surface, panel,
  control, line, text, dim, disabled, one accent, danger, warn), a 4 pt spacing step, row heights, one
  rounding and one type scale, and `install` applies the same style to both of `egui`'s variants so a
  system light preference cannot leave half the window in the stock palette. `header.rs` draws one row
  — mark, *Z3L*, *Home*, *File*, *Save*, *Undo*, *Redo*, the open tab with its unsaved dot, and
  the gear pinned to the far edge with `Sides` — and returns a `HeaderAction` the shell performs, so
  the row owns no document state; the window keeps its system frame, so no window buttons are drawn.
  The status text, the view controls, the work plane and the UI scale moved out of the viewport's first
  row into a bottom `status_bar`, drawn from a snapshot that applies its one action after both sides
  have drawn. The scene tree and the inspector lost their stock headings for a themed title and
  hairline. New assets: `assets/logo.svg`, the `window` theme (`minimize`, `maximize`, `restore`,
  `close`, drawn for a future frameless mode) and `actions/{file-save,undo,redo,settings}`; `view/home`.
  ADR 0063 (theme) and 0064 (header) — `c7ad880`
  `files: app/src/main.rs, crates/ui/src/toolbars.rs`
- `[x]` **U29** an arc inside the polyline: the Polyline tool appends either a straight segment or a
  two-click arc, and a chain that mixes both still closes and extrudes. The arc leaves the last
  vertex: the first click is the point it passes through, the second is its end, and the next
  segment starts where the stored arc actually ended. `DrawDraft` therefore stops holding a point
  list and holds the segments it has built, so `to_path` becomes a rebuild and every vertex after an
  arc is the stored `end()`, not the raw pointer point. That last rule is the trap: `arc_through`
  rounds centre, radius and angle to whole document units, `Path::assemble` tolerates a 1 µm joint
  while `Contour::assemble` demands exact equality, so a chain built from unrounded clicks can draw
  in the viewport and then fail `to_contour` — the only way to an extrusion. The draft also needs a
  third state, an arc waiting for its end, which `push`, `undo` (cancel the pending arc before the
  last segment), `is_complete`, `preview_path` and `size_label` all have to know; closing still adds
  one straight segment from the last stored end to the first stored start, and the pointer keeps
  snapping to the first vertex to close. A dragged bulge and a tangent continuation were rejected
  for now: the gizmo owns clicks, not drags. No document change — no schema, no command, no undo
  entry — and ADR 0048 gains one sentence saying a drawn vertex follows the stored arc end. Tests: a
  path whose arc end misses the next line by one micrometre converts through `to_contour` and by two
  is refused, a draft mixes both kinds and closes, an arc endpoint becomes the next vertex, undo
  cancels a pending arc, and the commit is still one `object.path.create`.
  Shipped as one draft of segments with Alt+click arcs — `ad95254`
  `files: crates/ui/src/contour_edit.rs, app/src/draw_gizmo.rs, crates/model/src/contour.rs`
- `[x]` **X2** a valid contour with more than 1024 sampled points cannot become an extrusion.
  Observed: the command now refuses it with "one extrusion cap holds at most 1024", where before
  A6 it was stored and drew nothing. Expected: any simple ring the user can draw becomes a solid.
  Cause: one extrusion cap is one document face, capped by `MAX_FACE_VERTICES`; the ring samples at
  50 µm, so a smooth ring of a few turns exceeds it while the ring itself is legal (at most 256
  segments). `object.collapse` fails on such an object and `object_mesh` cannot draw it. Fixed by ADR
  0062: the chord is 1e-4 of the shape's diagonal above a 0.05 mm floor, and a ring that still
  overflows is re-sampled coarser within a bounded search — `cb89f2e`
  `files: crates/param/src/profile.rs`
- `[x]` **U28** mass and one projection: `fill-opacity` is allowed (still no `defs`, `mask`,
  `clipPath`, gradient or CSS), so a passive face fills at 12–18 % and the result or changed geometry
  at 45–60 % under the heavy outline. The 18 drawn icons were redrawn inside the fixed `3..21`
  footprint at 20–45 % coverage on the one published axonometry; `STYLE_GUIDE.md` §5, §7.1, §7.3,
  §7.4, §9 and §15, `README.md`, `AUDIT.md`, the draw-icon skill and `crates/ui/src/icons.rs` carry
  the numbers, and a test reads footprint and coverage off the rasterised alpha. ADR 0055 — `42f6cde`
  `files: assets/icons/STYLE_GUIDE.md, crates/ui/src/icons.rs`

- `[x]` **U27** the theme reaches the icon: `icon_widget` rasterises an embedded white master with
  `egui_extras` at `pt × pixels_per_point`, bakes `WidgetVisuals::fg_stroke.color` into the pixels,
  caches a `TextureHandle` per `(uri, px, colour)` and paints it through `Painter::image`; the frame,
  hover/press and disabled alpha come from the same widget visuals a text button uses. Tools draw at
  20 pt and inline buttons at 16 pt; the 18 masters and the format docs say `#fff`, and the toolbar
  tests read the accessible name instead of `Image`'s alt text. The pixel test draws one icon in both
  themes and asserts the theme colour, one alpha silhouette and `round(pt × ppp)` at 1.0, 1.25 and
  2.0. ADR 0055 — `84a4675`
  `files: crates/ui/src/icons.rs, assets/README.md`

- `[x]` **U37** a cutout ring is edited in the viewport: `ContourTarget::Cutout { panel, machining }`
  reads the ring from `panel.machining_by_id`, `ContourEdit::apply` rebuilds the record through one
  `panel.machining.update` while keeping the anchor and the `CutoutDepth`, and the cutout card puts
  *Edit contour* beside the ring summary. `ContourEntry` names the editing, busy, ready and
  single-selection states, so the outline and the cutout never both claim the one live edit. Tests:
  an edit is one revision and one undo entry, the anchor and depth are unchanged, and a ring outside
  the core is refused while the draft stays — `65b1d31`
  `files: crates/ui/src/machining/cutout.rs, crates/ui/src/contour_edit.rs`

- `[x]` **U36** a rectangle panel outline becomes a ring once: `PanelOutline::Rect` stores corners and
  bows (`crates/model/src/panel/mod.rs:49-107`) and has no ring to edit, so *Edit contour* is offered
  only for a panel that is already a ring (`:194-207`). Add `panel.outline.to_contour`
  (`crates/engine/src/panel_cmds.rs`) that bakes the current outline through `Panel::nominal_contour`
  (`crates/model/src/panel/contour.rs:165`) in one revision and one undo entry, then opens the ordinary
  edit mode. The hard part is banding: a ring panel cannot keep `EdgeBanding` or `CornerBanding` (the
  loader refuses `ContourWithBanding`, `crates/model/src/panel/mod.rs:899-903`), so the command
  translates each edge band and each corner band into the ring segments `contour_with_sources_for` says
  it produced, and refuses with a readable reason when a corner band has no unique ring source. The card
  warns that the corner and bow parameters are gone; undo brings them back. Tests: the ring keeps the
  outline bounds; the converted banding names the same segments; undo restores the rectangle with its
  features; a second conversion is refused. ADR 0061 — `8ae7de5`
  `files: crates/engine/src/panel_cmds/panel.rs, crates/model/src/panel/contour.rs`

- `[x]` **U35** the path gizmo: the contour gizmo painted every draft as a closed ring and put a handle
  on each segment start, so an open chain lost its last point and no free end could be grabbed.
  `ContourGizmo` now paints a `ContourDraft`: a closed ring and a closed chain paint as
  `Shape::closed_line`, an open chain as `Shape::line`, and the open end gets a `PathCorner::End`
  handle. Handles, the drag and the arc projection are addressed with `PathCorner`, so a drag on the
  last point edits the last segment and the free start and free end project against their one
  segment. The object card offers *Edit contour* for a path as well as a constructive object — `f73a8ab`
  `files: app/src/contour_gizmo.rs, crates/ui/src/object_card.rs`

- `[x]` **U34** the path edit mode in the editor: `ContourTarget` gains `Path(Id)` and `ContourEdit`
  holds a `ContourDraft` that is either a ring or a chain, so an open line or a circle drawn by the
  tools has an editor. Moving, splitting and resetting a chain go through the model's `PathCorner`
  helpers, `apply` dispatches one `object.path.replace`, and `start`, `is_editable`, `is_dirty`,
  `reset` and `retain_contour_edit` keep their meaning for both drafts. The open chain is painted
  and dragged by the path gizmo of U35; the ring gizmo ignores a chain draft — `3970c0b`
  `files: crates/ui/src/contour_edit.rs, crates/ui/src/lib.rs`

- `[x]` **U33** path edit math in the model: a ring names a corner by the segment that starts on it,
  which wraps, so an open chain's last point had no name. `PathCorner::Start` and `End` name a corner
  explicitly; `Path::move_corner_at`, `split_segment_at`, `remove_segment` and `close` share
  `with_corner`, `join_corner` and `split_document` with the ring code. Ids are never reused,
  `Path::rebuild_as` keeps the counter while `close` flips the flag, a removal joins the neighbours at
  the previous end, and `close` refuses a bridge that would cross the chain (ADR 0060) — `7d6ca68`

- `[x]` **X5** a selected line, contour or circle is invisible: a selection tint moved a chain's
  channels by less than the eye resolves, and a one-pixel `LineList` cannot be widened in wgpu. A
  selected chain now takes one explicit `SELECTION_LINE_COLOR`, and the shell keeps the selected
  chains of the frame it just uploaded — after the drag preview — and repaints them over the viewport
  image: a dark 3.5 pt underlay under a 2.25 pt bright stroke. A pure `selection_outline` projects the
  stored points and cuts a chain behind the camera; the renderer keeps its one-pixel pipeline
  (ADR 0059) — `0e289e0`

- `[x]` **U32** a new flat figure pivots at its centre or at its first point: `Path::creation_pivot`
  and `Contour::creation_pivot` return the local bounds centre for a closed figure and the start of
  an open chain's first stored segment; the midpoint is summed in `i128`, so two extremes near the
  coordinate limits cannot overflow, and a flat figure's pivot lies on `z = 0`.
  `Project::add_object_with_pivot` is the only creation path that takes a pivot, `add_object`
  delegates with `Pivot::ZERO`, the path and extrusion commands use it, a replacement leaves it
  alone, and an older file that stores no pivot loads with zero. ADR 0058 — `bef8527`

- `[x]` **U31** a child is created through an explicit arm: `Editor` holds `creation_parent`,
  `creation_target(project, armed)` falls back to the root when the armed id vanished, the container
  menu and card arm a group or an assembly, and the strip shows *Adding to: Frame ✕*; Escape and the
  container's removal clear the arm, while a primitive resolves its parent at the click — `478ba44`

- `[x]` **X6** the top and bottom views look at an angle to their plane: `MIN_PITCH`/`MAX_PITCH` are
  ±85° and `look_from` clamps to them, so a click on the Z ball left the camera five degrees off
  vertical and the grid, the work plane and the drawing all skewed. The limits are now `±FRAC_PI_2`;
  `right()` is analytic, `(-sin yaw, cos yaw, 0)`, the exact limit of the old `forward × Z`, and
  stays unit at the pole; `up()` is `right × forward` with no fallback; `view_matrix` uses `self.up()`,
  so an authored camera's pinned up is not overwritten. Tests cover both poles, the centre ray, the
  target projection, the work plane, the widget flip and an orbit off the pole. ADR 0057 replaces the
  five-degree clamp of ADR 0019 (`Accepted; pole clamp superseded by 0057`) — `7b0a02e`

- `[x]` **U30** the armed tool is highlighted: `icons::tool_button` takes `on: bool` and draws
  `Button::selected`, which egui paints with `visuals.selection.bg_fill`, and the text fallback
  takes the same flag. `ToolActivity` names the two arming tools and the pure `tool_activity`/
  `is_armed` selectors press exactly one button; the strip takes `active` and the editor derives it
  from the running draft or the waiting primitive. A one-shot creation never arms. The duplicate
  "Draw" row is gone, so the strip is the only creator; toggled-on stays a theme state, not a second
  SVG (ADR 0055) — `78b4d76`

- `[x]` **X4** every creation tool lands at the scene root: `creation_root(project)` is the only
  default parent, `ToolAction` carries no parent and `drawing_parent` is gone, so a selected nested
  assembly receives none of the six creations; `extrude_path` stays a sibling of its path. ADR 0056
  supersedes the parent rule of ADR 0052 — `c135511`

- `[x]` **X3** the projection toggle draws the mode it is in, not the one it switches to: the icon
  and the hint come from one pure `projection_button(Projection)`, so a perspective view shows the
  perspective glyph and the hint names the switch; a table test covers both modes and the
  authored-camera disable stays where it was — `9bebec0`

- `[x]` **V1** a primitive keeps the parameters that built it: `Geometry::Primitive(Primitive)`
  stores the shape and its parameters, `crates/model/src/primitive.rs` validates them on construction
  and on load, `crates/mesh/src/primitive.rs` derives the mesh, `object.primitive.create` stores the
  recipe and `object.primitive.replace` edits it in one revision and one undo entry;
  `object.collapse` bakes either a constructive or a primitive payload. Schema 18, the
  `scene-v18.fcs` golden and ADR 0054; a cube keeps six faces and only the curved shapes carry a
  face count — `56b876c`

- `[x]` **U26** the toolbar icon rollout: cube, cylinder and sphere (`97ee5ce`), then panel,
  assembly, camera, light and extrude; the viewport strip draws frame-all and the projection toggle
  from `view/*`, and one test rasterises every drawn asset as a 24 grid SVG — `3fd6ed1`

- `[x]` **P3** `STATE: task=` must name the first unfinished task in `## Now`: `first_unfinished`
  takes the first not-`[x]` line there, a later task is a violation that names the skipped one,
  `## Bugs`/`## Next` stay outside the order, and a stray `[x]` in `## Now` keeps its own error —
  `0190f91`

- `[x]` **P2** `updated=` is a `YYYY-MM-DD` calendar date and never earlier than the `base` commit's
  date: `calendar_date` checks the shape and the month, `Git::commit_date` reads `%cs`, and the
  comparison runs only when the base resolves — `38057a2`

- `[x]` **P1** `cargo xtask status` prints `head <short hash>` and, while `state=doing`, the note's
  `task`/`now`/`blocked`/`run` block, so an interrupted session resumes in one command: `Summary`
  carries the parsed note and the head commit, and only a backtick pair wrapping a whole note value
  is unwrapped — `b03e88b`

- `[x]` **S4** revision-keyed evaluation cache: `(Revision, Id)` to evaluated body and mesh in
  `mesh`; `scene_build` and `object.collapse` share one evaluator and a rebuild touches
  only changed nodes. The key is the node id plus the derivation inputs, because the revision is
  document-wide and would miss on every edit; `finish` prunes unvisited nodes — `42fc9a6`
- `[x]` **U23** extrude a selected closed path: a height field and one Extrude button in the Bodies
  panel, `Path::to_contour` under `object.extrude.create`, the body a sibling at the path's
  placement, and an open path refused with a readable reason — `098b4a3`
- `[x]` **U22** cylinder and sphere: `Mesh::cylinder` and `Mesh::sphere`, their `PrimitiveKind`
  variants and three Primitive-panel buttons — `49a8246`
- `[x]` **U21** a primitive from its panel: an edge-length field, a Cube tool that arms a placement,
  and one `object.primitive.create` where the work plane is clicked — `b1912b4`
- `[x]` **U20** the scene tree only shows and restructures: every create button and `TreeAction`
  variant is gone and the toolbar strip is the only creator — `8a099f1`
- `[x]` **U19** fill the toolbar panels: five contour tools, four scene creators, the cube and the
  extrude, each a hinted icon button with a text fallback and shared shortcuts — `14b2c76`
- `[x]` **U18** themed toolbars: `ToolPanel` and `Toolbars` group the tools by theme as collapsible
  sections, with a hinted tool button and a text fallback — `d38046b`
- `[x]` **U17** commit a drawn path: one `object.path.create` on the work plane, the node selected
  and the tool reset, in one undo entry (ADR 0052) — `a8e75b6`
- `[x]` **U16** the viewport preview: `DrawGizmo` draws the draft, the snap marker and a live size
  while it owns the pointer, and the strip starts each tool — `b203763`
- `[x]` **U15** the drawing tools: line and polyline, an arc, a rectangle and a circle each build a
  `Path` through `Path::polyline`, `arc_through`, `rectangle` and `circle_through` — `5c487dd`
- `[x]` **U14** a drawing draft in the UI: `DrawDraft` collects plane points, undoes the last
  action, closes into a ring and converts to a `Path`, with the document untouched — `307d745`
- `[x]` **U13** snapping on the work plane: a draft end, an axis lock and the 10 mm grid as pure
  functions, with deterministic ties and table tests — `6d64dc7`
- `[x]` **U12** the work plane: `WorkPlane` maps the pointer onto XY, XZ or YZ from the six standard
  ortho views, world XY otherwise, with a strip label — `6c61962`
- `[x]` **U11** orthographic ↔ perspective as view state: `Camera::toggle_projection` keeps the
  visible height, the strip has a button and `O`, and an authored camera is untouched — `c195d45`
- `[x]` **U10** shape constructors: `Path::rectangle` from two corners, `Path::circle` as four
  quarter arcs and `Path::arc_through`, with degenerate input refused — `13c90b3`
- `[x]` **U9** `object.path.create` and `object.path.replace`, one undo entry each, with ADR 0048 and
  the path payload in the geometry document — `546fb28`
- `[x]` **U8** pick a path: `SceneFrame::pick` measures the ray-to-segment closest approach and
  returns a document `Id` within six viewport points, through one shell helper — `7c544a5`
- `[x]` **U7** draw a stored path: a `Polyline` line primitive in the scene frame, the renderer's
  shared line pipeline, and the shell's sampling of a `Path` into a chain with its bounds — `9ce56be`
- `[x]` **U6** a path object: `Geometry::Path(Path)` at schema 17, with `Project::object_path` and
  `Project::set_object_path`, its chain extents in the node bounds, and older files still
  loading — `847cbbb`
- `[x]` **U5** `Path::to_contour`: a path becomes the ring a panel or an extrusion needs only when it
  is closed and simple, otherwise a readable error — `1c74ea5`
- `[x]` **U4** a flat contour value: `Path` is an ordered chain of line and arc segments with stable
  ids, open or closed, at least one segment, joints meeting within a micrometre and one chain per
  path; tests for a single segment, an arc, an open polyline and a broken joint — `25e330f`
- `[x]` **U25** the contour icon set: `line`, `polyline` and `arc` as flat constructions with
  control points, `rectangle` from two opposite corners, `circle` from its centre, `close` as an
  open chain joined by a heavy segment and `cancel` as the service X; every file reads at 16 px on
  the 24 grid — `d122b85`
- `[x]` **U3** UI scale: a 75–200 % slider in the viewport strip bound to `ctx.set_zoom_factor`, so
  icons are rasterised at the new pixel size instead of stretched, and egui's keyboard zoom stays
  visible in the slider — `152a514`
- `[x]` **U2** icon resources and icon buttons: `egui_extras` with the `svg` feature embeds
  `assets/icons/**` as one module per theme; the shell installs the SVG loader, and `icon_button`
  draws at a fixed 16 pt tinted with the theme text colour, falling back to its text label while an
  asset is empty — `2b453a0`
- `[x]` **U24** the icon contract: one ink on a 24 grid, three families (geometry, view, service), the
  service grammar and its `actions` theme, ADR 0047, and `AGENTS.md` §4.12 plus the `draw-icon` skill
  so the guide is read only while an icon is drawn or edited — `e473342`
- `[x]` **U1** the icon resource tree: `assets/icons/` with one directory per toolbar theme and one
  zero-byte SVG per icon (contour, primitives, bodies, scene, view — 21 placeholders), plus
  `assets/README.md` with the naming, the 24 × 24 grid, the geometry-only format and the empty-file
  fallback — `19fbb49`
- `[x]` **D7** the panel card lists and edits machining from the descriptor table: common operation
  chrome (fold, kind and anchor, reorder, remove) with a section module per kind, an add menu built
  from `MACHINING_DESCRIPTORS` and grouped by category, diagnostics naming a `MachiningId` shown with
  their operation while the Diagnostics section keeps the panel-wide ones, and the open rows kept as
  card view state — `b067bc4`
- `[x]` **D6** the machining pipeline: `CompileMachining` and `OpEffect` (cut region, subtract, add,
  attachment), a `PanelPlan` that folds operations, and a static descriptor table with kind,
  category, stage, capabilities and default; the panel builder contains no kind match, and a
  post/deform stage stays reserved for the deferred bend — `0f1a58a`
- `[x]` **D5** a typed machining list: ordered `Machining` records with stable `MachiningId`s, a
  tagged `MachiningParams` and shared face/edge anchors; `panel.machining.add/update/remove/move`
  replace `cutout.create`, schema 16 writes the record form while legacy cutout entries still load —
  `d87a726`
- `[x]` **D4** one panel outline source: a rectangle keeps its corner and bow parameters as a contour
  generator, a contour panel stores the ring, both produce a `geom::Region`; `PanelGeometry` derives
  core and overall from the region, and no machining code branches on the outline kind —
  `67ccca2`
- `[x]` **G2** exact 2D region booleans on `Contour` rings: union, difference and intersection, used
  to union through cutouts and subtract them from the core instead of the rectangle-only tracer;
  `geom::region` traces the boundary of the set and keeps rings exact — `a0a8e94`
- `[x]` **G1** `geom::Region` holds exact `Contour` rings (outer and holes): rectangle constructor,
  bounds, area, containment and sampling, with the rectangle boolean helpers still passing — `c3c00a4`
- `[x]` **S3** one contour conversion: `crates/param/src/profile.rs` turns a document ring into an
  analytic contour, a sampled `geom::Region` and a `kernel::Profile`; the rectangular core, shaped
  core, cutout tool and constructive extrusion share one sampler and one chord tolerance, and
  `Contour::new_with_tolerance` accepts the one-micrometre joint a stored ring can close to — `82bd63b`
- `[x]` **A7** the contour panel and the contour edit mode are written down in `docs/domain/` and
  ADR 0042: the outline variant, the anchored ring, ring banding, the cutout ring rules and the
  working-copy edit mode — `020a6cb`
- `[x]` **S2** one domain-free solid plan: `SolidPlan` is a base extrusion plus boolean steps, a
  `PlanBuilder` assembles it, and one evaluator drives both the kernel and the analytic
  single-extrusion backend; panels, bands, cutout tools and constructive objects compile to it,
  `object.collapse` keeps the exact mesh, and ADR 0041 records the boundary — `bd33056`
- `[x]` **S1** decompose `crates/model/src/lib.rs` by responsibility: one module per value family,
  panel rules split into geometry, banding and machining, `Project` split into wire, error, derived
  queries and command families, tree traversal in one place; public API, serde and behaviour
  unchanged — `43583b4`
- `[x]` **D3** a cutout by an arbitrary contour: `CutoutShape::Contour`, kept inside the core, through
  and blind, the tool built by extruding the sampled ring; the schema moves to 14 — `2167e2b`
- `[x]` **D2** the contour panel card: the ring summary, the material-driven thickness, the material
  and one banding row per ring segment through `panel.ring_banding.set`/`clear`, no corners, bows or
  cutouts, an `Edit contour` entry; a missing ring-band material is a diagnostic — `f88a995`
- `[x]` **A6** contour edit commands: `panel.outline.set` and `object.contour.replace` share their
  `enabled` check with `run`, validate the ring before the document changes, and record one undo
  entry per apply; a contour too detailed for one extrusion cap is refused instead of stored — `6f3e82b`
- `[x]` **X1** a concave contour cap drew a flap triangle across the notch: `object_mesh` fanned
  every face, so the cap of an extruded object left its own contour; faces are ear-clipped now —
  `a2859fc`
- `[x]` **A3** the contour edit mode: enter on a panel outline or an object contour, edit a working copy
  in the viewport with handles and a preview, leave by applying one command or by cancelling — `1428441`
- `[x]` **A5** contour edit math: move a corner, trim, split, and rely on the segment ring — `dda986c`
- `[x]` **D1** a panel whose outline is a contour ring, banding per ring segment — `d9827ed`
- `[x]` **A4** stable ids for contour segments — `ea3153a`
- `[x]` **A2** contour as a ring of segments — `0833b4c`
- `[x]` **A1** the contour segment type — `e7551ca`
