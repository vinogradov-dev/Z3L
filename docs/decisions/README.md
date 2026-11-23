# Architecture decisions

One file per decision, short: the situation, the decision, and what it costs us. Decisions are
superseded, never rewritten — add a new record and mark the old one `Superseded by NNNN`.

| # | Title | Status |
|---|---|---|
| [0001](0001-geometry-kernel.md) | Geometry kernel behind a port, implemented with truck | Accepted |
| [0002](0002-vendored-truck-shapeops.md) | Vendored `truck-shapeops` without the VTK stack | Accepted |
| [0003](0003-length-is-integer-micrometres.md) | Lengths are integer micrometres | Accepted |
| [0004](0004-panel-local-frame.md) | Panel-local frame, faces and topology ids | Accepted; frame origin superseded by 0009 |
| [0005](0005-material-catalogue.md) | Material catalogue is external and session-scoped | Superseded by 0148 |
| [0006](0006-panel-nominal-and-resolved-geometry.md) | Panels store nominal size; geometry is resolved | Accepted; core-origin placement superseded by 0009 |
| [0007](0007-edge-banding-v1.md) | Edge banding v1: ordered bands and butt joints | Accepted; per-path banding unit superseded by 0014 |
| [0008](0008-panel-diagnostics.md) | Panel diagnostics and command invalidation | Accepted |
| [0009](0009-contour-first-panel-model.md) | Contour-first panel geometry and banding paths | Accepted; core-local machining superseded by 0011; banding-path unit superseded by 0014 |
| [0010](0010-analytic-line-arc-contours.md) | Analytic line and arc contours for panel offsets | Accepted |
| [0011](0011-panel-local-machining.md) | Machining uses the nominal panel frame | Accepted |
| [0012](0012-in-panel-cutout-union.md) | In-house panel cutout union | Accepted |
| [0013](0013-geometry-edits-are-never-blocked.md) | Geometry edits are never blocked by banding or machining | Accepted |
| [0014](0014-banding-belongs-to-segments.md) | Banding belongs to one contour segment | Accepted |
| [0015](0015-band-joint-trim.md) | Band joints extend and trim in application order | Accepted; arc scope extended by 0016 |
| [0016](0016-arc-band-joints.md) | Arc band joints use the same rules as straight ones | Accepted |
| [0017](0017-panels-are-atomic-assemblies-are-groups.md) | Panels are atomic leaves, assemblies are editable groups | Superseded by 0021 |
| [0018](0018-picking-ray-casts-the-scene.md) | Picking ray-casts the scene description and returns document ids | Accepted |
| [0019](0019-standard-views-from-a-corner-widget.md) | Standard views come from a projected corner orientation widget | Accepted; pole clamp superseded by 0057 |
| [0020](0020-the-translate-gizmo-previews-and-commits-one-placement.md) | The translate gizmo previews a frame and commits one placement | Accepted; batch commit extended by 0033 |
| [0021](0021-assemblies-are-components-groups-are-containers.md) | Assemblies are components, groups are containers | Superseded by 0034; supersedes 0017 |
| [0022](0022-closed-containers-pick-as-one-object.md) | A closed container picks as one object | Accepted |
| [0023](0023-fasad-furniture-and-standard-views.md) | Fasad furniture and standard views | Accepted |
| [0024](0024-rotation-is-a-quaternion-shown-as-xyz-euler-angles.md) | Rotation is a quaternion shown as intrinsic XYZ Euler angles | Accepted |
| [0025](0025-one-manipulation-pivot.md) | Move and Rotate share one pivot at the first corner of the local bounds | Superseded by 0026 |
| [0026](0026-the-manipulation-pivot-is-stored-node-data.md) | The manipulation pivot is stored, editable node data | Accepted; supersedes 0025 |
| [0027](0027-the-rotate-gizmo-uses-world-aligned-rings.md) | The rotate gizmo uses world-aligned rings | Accepted; batch commit extended by 0033 |
| [0028](0028-the-assistant-is-a-client-of-the-command-registry.md) | The assistant is a client of the command registry | Accepted; registry narrowed by 0185, layering by 0186, listening clarified by 0188 |
| [0029](0029-view-state-belongs-to-the-session.md) | View state belongs to the session | Accepted; camera and light objects carved out by 0036 |
| [0030](0030-construction-knowledge-is-code.md) | Construction knowledge is code, not prose | Superseded by 0034 |
| [0031](0031-external-resources-keep-their-own-system-of-record.md) | External resources keep their own system of record | Accepted |
| [0032](0032-an-order-keeps-its-versions-as-folders.md) | An order keeps its versions as numbered folders | Accepted |
| [0033](0033-a-multi-selection-is-placed-by-one-command.md) | A multi-selection is placed by one command | Accepted |
| [0034](0034-nodes-carry-typed-properties.md) | Nodes carry typed properties; containers are one kind | Accepted; supersedes 0021 and 0030 |
| [0035](0035-furniture-types-form-a-numbered-tree.md) | Furniture types form a numbered tree | Accepted |
| [0036](0036-cameras-and-lights-are-scene-objects.md) | Cameras and lights are scene objects | Accepted |
| [0037](0037-a-geometry-object-is-a-mesh-or-a-feature-history.md) | A geometry object is a baked mesh or a feature history | Accepted |
| [0038](0038-the-scene-file-lives-in-its-own-crate.md) | The scene file lives in its own crate | Accepted |
| [0039](0039-point-lights-use-watts-and-inverse-square-falloff.md) | Point lights use watts and inverse-square falloff | Accepted |
| [0040](0040-a-constructive-contour-is-a-ring-of-segments.md) | A constructive contour is a ring of directed segments | Accepted |
| [0041](0041-every-solid-compiles-to-one-solid-plan.md) | Every solid compiles to one domain-free plan | Accepted |
| [0042](0042-a-contour-panel-keeps-its-ring-and-edits-a-working-copy.md) | A contour panel keeps its ring and edits a working copy | Accepted |
| [0043](0043-region-booleans-trace-the-boundary-of-the-set.md) | Region booleans trace the boundary of the set | Accepted |
| [0044](0044-one-outline-source-resolves-the-panel-core.md) | One outline source resolves the panel core | Accepted |
| [0045](0045-machining-is-an-ordered-list-of-anchored-operations.md) | Machining is an ordered list of anchored operations | Accepted |
| [0046](0046-machining-compiles-to-staged-effects.md) | Machining compiles to staged effects through one descriptor table | Accepted |
| [0047](0047-icons-are-one-ink-on-a-24-grid.md) | Icons are one ink on a 24 grid, in three families | Accepted; refined by 0055; one-ink clause superseded by 0090 |
| [0048](0048-a-flat-drawing-is-a-path-of-segments.md) | A flat drawing is a path of segments | Accepted |
| [0049](0049-the-viewport-toggles-its-own-projection.md) | The viewport toggles its own projection | Accepted |
| [0050](0050-the-work-plane-follows-the-view.md) | The work plane follows the view | Accepted |
| [0051](0051-snapping-composes-the-grid-the-axis-and-the-draft-ends.md) | Snapping composes the grid, the axis and the draft ends | Accepted |
| [0052](0052-a-drawing-commits-as-one-path-on-the-work-plane.md) | A drawing commits as one path on the work plane | Accepted; parent rule superseded by 0056 |
| [0053](0053-the-evaluation-cache-keys-derivation-inputs.md) | The evaluation cache is keyed by derivation inputs | Accepted |
| [0054](0054-a-primitive-keeps-the-parameters-that-built-it.md) | A primitive keeps the parameters that built it | Accepted |
| [0055](0055-the-ui-rasterises-icons-and-emphasis-is-tonal.md) | The UI rasterises icons itself, and emphasis is tonal | Accepted; one-ink clause superseded by 0090, the sizes and role table by 0124 |
| [0056](0056-creation-tools-land-at-the-scene-root.md) | Creation tools land at the scene root | Accepted |
| [0057](0057-the-camera-reaches-the-poles.md) | The camera reaches the poles | Accepted; roll added by 0078 |
| [0058](0058-a-new-drawing-pivots-on-its-own-shape.md) | A new drawing pivots on its own shape | Accepted |
| [0059](0059-a-selected-line-is-a-screen-space-overlay.md) | A selected line is a screen-space overlay | Accepted |
| [0060](0060-an-open-path-is-edited-by-its-corners.md) | An open path is edited by its corners | Accepted |
| [0061](0061-a-rectangle-outline-bakes-into-a-ring.md) | A rectangle outline bakes into a ring | Accepted |
| [0062](0062-a-large-ring-samples-to-fit-its-cap.md) | A large ring samples to fit its cap | Accepted |
| [0063](0063-one-theme-owns-the-window.md) | One theme owns the window's colour and rhythm | Accepted; light variant superseded by 0069 |
| [0064](0064-the-header-is-chrome-that-returns-an-action.md) | The header is chrome that returns an action | Accepted |
| [0065](0065-visibility-and-lock-are-session-state.md) | Visibility and lock are session state | Accepted |
| [0066](0066-the-ribbon-owns-the-tabs-and-a-tool-is-a-labelled-button.md) | The ribbon owns the tabs, and a tool is a labelled button | Accepted |
| [0067](0067-a-ribbon-tool-is-one-measured-cell.md) | A ribbon tool is one measured cell, and a caption never sizes it | Accepted; cell metrics and caption rule superseded by 0091 |
| [0068](0068-the-workspace-lives-outside-the-document.md) | The workspace lives outside the document | Accepted |
| [0069](0069-the-light-variant-is-a-second-palette.md) | The light variant is a second palette | Accepted |
| [0070](0070-display-styles-are-frame-data.md) | Display styles are frame data | Accepted |
| [0071](0071-a-contact-shadow-is-one-transparent-ground-ellipse.md) | A contact shadow is one transparent ground ellipse | Superseded by 0142 |
| [0072](0072-a-contour-joint-is-judged-with-the-drawing-tolerance.md) | A contour joint is judged with the drawing tolerance | Accepted |
| [0073](0073-a-drawn-figure-remembers-its-shape.md) | A drawn figure remembers its shape | Accepted |
| [0074](0074-an-extrusion-owns-the-path-it-grew-from.md) | An extrusion owns the path it grew from | Accepted |
| [0075](0075-a-grip-names-what-its-number-means.md) | A grip names what its number means | Accepted |
| [0076](0076-a-drawing-tool-separates-the-segment-kind-from-the-snaps.md) | A drawing tool separates the segment kind from the snaps | Accepted; the Tab key superseded by 0128 |
| [0077](0077-a-pending-arc-is-the-closing-segment.md) | A pending arc is the closing segment | Accepted |
| [0078](0078-a-view-rolls-about-its-own-axis.md) | A view rolls about its own axis | Accepted |
| [0079](0079-every-layout-remembers-its-own-views.md) | Every layout remembers its own views | Accepted; refit-on-open superseded by 0081 |
| [0080](0080-the-tool-strips-float-over-the-design-space.md) | The tool strips float over the design space | Accepted |
| [0081](0081-one-document-per-tab.md) | One document per tab; the window keeps only what it owns | Accepted |
| [0082](0082-the-local-zero-is-the-creation-anchor.md) | The local zero is the creation anchor | Accepted |
| [0083](0083-a-panel-is-anchored-at-its-base-corner.md) | A panel is anchored at its base corner | Accepted |
| [0084](0084-shared-test-fixtures-behind-a-testing-feature.md) | Shared test fixtures live in the model behind a `testing` feature | Accepted |
| [0085](0085-the-plan-stays-markdown.md) | The plan is markdown, written through one tool — not a database | Accepted |
| [0086](0086-the-interface-speaks-two-languages.md) | The interface speaks two languages, and a generated name keeps the one that made it | Accepted |
| [0087](0087-the-viewports-own-colours-follow-the-theme.md) | The viewport's own colours follow the theme variant; material colours never do | Accepted |
| [0088](0088-origin-and-pivot-are-different-points.md) | Origin and pivot are different points | Accepted; refines 0026 and 0082, settles 0083 |
| [0089](0089-three-palettes-are-data-and-derive-their-states.md) | Three palettes are data: one ramp per variant, derived states, one accent override | Accepted; the P2 and P4 accents and their names moved by 0125 |
| [0090](0090-icons-are-coloured-by-semantic-family.md) | An icon is coloured by its semantic family, and the theme maps the literal | Accepted; supersedes the one-ink clause of 0047 and 0055, extended to a seventh role by 0124, the accent separation by 0125 |
| [0091](0091-the-ribbon-is-one-const-map.md) | The ribbon is one const map, and one tool is one glyph in a 34 x 30 cell | Accepted; supersedes the cell metrics and the caption rule of 0067; fields removed by its first amendment, a split cell made a cell plus its caret by the second, the cell grown to 38 x 34 by 0124 |
| [0092](0092-a-field-is-a-number-with-its-unit-outside.md) | A numeric field is a number with its unit outside, and one edit is one undo entry | Accepted |
| [0093](0093-a-preview-fills-a-session-overlay.md) | A previewing operation fills a session overlay, not the document, and one command bar applies it | Accepted |
| [0094](0094-a-class-registry-gives-the-machine-its-semantics.md) | A class registry gives the machine its semantics; the code identifies, the traits decide | Accepted; supersedes the registry role and top-level grouping of 0035 |
| [0095](0095-the-order-package-is-a-published-snapshot.md) | The order package is a published snapshot, not the internal database | Accepted |
| [0096](0096-before-the-first-release-the-format-has-no-past.md) | Before the first release the data format has no past | Accepted; amends the compatibility clause of `AGENTS.md` §4.5; retires the migration clauses until 1.0 |
| [0097](0097-a-product-is-a-container-with-an-authored-envelope.md) | A product is a container with an authored envelope | Accepted; amends the trait list of 0094; its group-10 sentence and code examples superseded by 0145 |
| [0098](0098-one-product-is-open-and-parts-are-born-inside-it.md) | One product is open, and furniture parts are born inside it | Accepted |
| [0099](0099-a-niche-is-the-space-a-frame-reserves.md) | A niche is the space a frame reserves | Accepted; refines 0097 |
| [0100](0100-a-frame-is-created-opened-and-drawn.md) | A frame is created, opened and drawn: the lifecycle | Accepted; amends 0097 and 0098 |
| [0101](0101-a-frame-and-a-box-are-one-tool.md) | A frame and a box are one tool: draw the base, pull the height | Accepted; retires the cube primitive |
| [0102](0102-a-furniture-entity-is-a-surface.md) | A furniture entity is a surface; a frame is a shell | Accepted; amends the probe and collision clauses of 0099 and 0097 |
| [0103](0103-one-probe-serves-the-niche-the-distances-and-the-move.md) | One probe serves the niche, the distances and the move | Accepted; refines 0099 and 0102 |
| [0104](0104-a-ghost-of-a-missing-node-is-shell-paint.md) | A ghost of a missing node is shell paint, not the overlay | Accepted; narrows 0093 |
| [0105](0105-a-panel-stance-is-the-flat-frame-turned.md) | A panel stance is the flat frame turned, never mirrored | Accepted |
| [0106](0106-a-niche-is-a-convex-cell-of-wall-planes.md) | A niche is a convex cell of wall planes | Accepted; refines 0099 and 0103; coverage clause superseded by 0109 |
| [0107](0107-an-end-bevel-is-an-angle-about-its-edge.md) | An end bevel is an angle about its outline edge | Accepted |
| [0108](0108-a-shaped-fill-bevels-its-ends-onto-the-walls.md) | A shaped fill bevels its ends onto the walls, and turns the board when it must | Accepted; refines 0105 and 0107; its two refusals are superseded by 0109 |
| [0109](0109-a-niche-is-bounded-by-the-faces-the-pointer-sees.md) | A niche is bounded by the faces the pointer sees | Accepted; supersedes the coverage clause of 0106 |
| [0110](0110-one-fill-cell-and-its-build-method.md) | One fill cell, and its build method in the cell's own list | Accepted; refines 0106 and the ribbon list of U74; the list's captions dropped by its amendment (U84) |
| [0111](0111-a-guide-is-one-amber-in-screen-space.md) | A guide is one amber, drawn in screen space | Accepted; derives the ink of 0089 and the frame inks of 0087 |
| [0112](0112-a-destructive-key-asks-when-undo-is-not-enough.md) | A destructive key asks when undo is not enough | Accepted |
| [0113](0113-select-is-the-state-a-window-opens-in.md) | Select is the state a window opens in | Accepted |
| [0114](0114-a-panel-is-cut-from-a-ring.md) | A panel is cut from a ring, so the panel cell goes | Accepted; refines 0042 and 0083 |
| [0115](0115-a-right-click-opens-the-nodes-actions.md) | A right click opens the node's actions, and a right drag pans the view | Accepted |
| [0116](0116-the-object-card-is-one-reading-order.md) | The object card is one reading order, and a fold only hides what is long | Accepted |
| [0117](0117-the-interface-is-one-inventory-of-capabilities.md) | The interface is one inventory of capabilities: every command has a place | Accepted |
| [0118](0118-a-mechanism-used-twice-is-a-tool.md) | A mechanism used twice is a tool, and a measurement is not a dimension | Accepted |
| [0119](0119-the-armed-tool-shows-its-parameters-at-the-pointer.md) | The armed tool shows its parameters at the pointer, and the inspector shows the same fields | Accepted |
| [0120](0120-sheets-and-money-are-additive-in-the-package.md) | Sheet stock and money join the package as additive columns, and the rules are declared | Accepted |
| [0121](0121-the-pointer-is-a-two-tone-square-above-the-chrome.md) | The pointer is a two-tone square, painted above the chrome | Accepted; scope superseded by 0122, layer by 0123 |
| [0122](0122-the-custom-pointer-belongs-to-the-work-area.md) | The custom pointer belongs to the work area | Accepted; layer clarified by 0123 |
| [0123](0123-work-area-paint-sits-under-the-chrome.md) | Work-area paint sits under the chrome | Accepted |
| [0124](0124-the-icon-language-is-v2.md) | The icon language is v2: a 24 grid, a 20 × 20 footprint and a seventh role | Accepted |
| [0125](0125-an-accent-keeps-its-hue-distance-from-every-role-ink.md) | An accent keeps its hue distance from every role ink | Accepted |
| [0126](0126-the-point-being-placed-shows-its-numbers-at-the-cursor.md) | The point being placed shows its numbers at the cursor, and one of them holds the keys | Accepted |
| [0127](0127-a-sketch-has-two-guides.md) | A sketch has two guides: a construction line and a reference line | Accepted; dot sizes superseded by 0133 |
| [0128](0128-a-sketch-angle-reads-from-the-nearer-x-end.md) | A sketch angle is read from the nearer end of the X axis, and Tab belongs to the plate | Accepted; end choice superseded by 0134 |
| [0129](0129-a-move-is-latched-by-a-second-click.md) | A move is latched by a second click, and its delta is read from the old position | Accepted |
| [0130](0130-an-arc-from-its-chord-and-radius-bows-away-from-the-pointer.md) | An arc from its chord and radius bows away from the pointer that named its centre | Accepted |
| [0131](0131-a-stage-shows-the-numbers-its-figure-reads.md) | A stage shows the numbers its figure reads, and a figure's own number is on every stage | Accepted |
| [0132](0132-the-plates-of-one-point-are-one-layout.md) | The plates of one point are one layout, so no plate hides another | Accepted |
| [0133](0133-a-reference-lies-in-the-plane-and-reads.md) | A reference lies on the work plane and reads at a glance | Accepted |
| [0134](0134-a-sketch-angle-changes-side-once-per-turn.md) | A sketch angle changes side once per turn, not twice | Accepted; supersedes the end choice of 0128 |
| [0135](0135-the-product-is-z3l.md) | The product is named Z3L: a zero and three laws | Accepted |
| [0136](0136-the-wordmark-is-drawn-in-the-text-ink.md) | The header draws the wordmark in the text ink | Accepted |
| [0137](0137-a-rubber-band-selects-by-the-direction-it-is-dragged.md) | A rubber band selects by the direction it is dragged | Accepted |
| [0138](0138-a-solid-body-is-a-drawn-profile-swept-on-z.md) | A solid body is a drawn profile swept on Z | Accepted |
| [0139](0139-a-sphere-is-drawn-around-its-centre.md) | A sphere is drawn around its centre | Accepted |
| [0140](0140-the-corner-display-is-a-diagnostic-block-the-maker-can-read.md) | The corner display is a diagnostic block the maker can read | Accepted |
| [0141](0141-a-cone-is-drawn-by-its-base-circle.md) | A cone is drawn by its base circle | Accepted |
| [0142](0142-the-ground-shadow-is-the-silhouette-the-key-light-casts.md) | The ground shadow is the silhouette the key light casts on a floor plate | Accepted; supersedes 0071; the plate's place in the pass and its opacity superseded by 0144 |
| [0143](0143-the-shadow-map-is-the-key-lights-depth-pass.md) | The shadow map is the key light's depth pass, read by every face | Accepted |
| [0144](0144-the-floor-plate-is-blended-so-it-tints-what-stands-under-it.md) | The floor plate is blended, so it tints what stands under it and keeps its shadow | Accepted |
| [0145](0145-the-group-digits-follow-the-order-of-the-work.md) | The group digits follow the order of the work | Accepted; reassigns the group digits of 0094 and supersedes the group-10 sentence of 0097 |
| [0146](0146-snapping-is-a-workspace-setting-and-the-coordinate-planes-stand-in-for-walls.md) | Snapping is a workspace setting, and the coordinate planes stand in for walls | Accepted; the snap constants of 0102 and 0103 become settings, and the floor snap joins the three planes |
| [0147](0147-the-furniture-move-is-its-own-tool-and-a-panel-slides-along-its-thickness.md) | The furniture move is its own tool, and a panel slides along its thickness | Accepted; adds a fourth transform mode, takes the furniture rule and its snapping out of the free move, and adds the panel axis; its handle half is superseded by 0151 |
| [0148](0148-a-nomenclature-directory-is-a-set-of-packages.md) | A nomenclature directory is a set of packages | Accepted; supersedes 0005 |
| [0149](0149-a-catalogue-part-is-a-document-instantiated-in-the-scene.md) | A catalogue part is a document, and the scene holds an instance of it | Accepted |
| [0150](0150-machining-is-derived-and-rules-are-tables.md) | Machining is derived from geometry, and placement rules are tables | Accepted |
| [0151](0151-the-furniture-move-takes-the-piece-by-a-click-and-follows-the-pointer.md) | The furniture move takes the piece by a click and follows the pointer | Accepted; supersedes the handle half of 0147; its single-axis latch is superseded by 0163 |
| [0152](0152-a-piece-in-hand-blocks-nothing-and-the-view-moves-under-it.md) | A piece in hand blocks nothing, and the view moves under it | Accepted; narrows 0151 |
| [0153](0153-a-carried-piece-is-measured-by-the-hand-not-by-the-ray.md) | A carried piece is measured by the hand, not by the ray that points at it | Accepted; replaces 0151's distance mapping; its one-axis carry is narrowed by 0163 |
| [0154](0154-the-wheel-zooms-towards-the-pointer.md) | The wheel zooms towards the pointer | Accepted |
| [0155](0155-a-carried-piece-keeps-its-place-under-the-hand.md) | A carried piece keeps its place under the hand | Superseded by 0156 |
| [0156](0156-the-pointer-comes-back-to-a-carried-piece.md) | The pointer comes back to a carried piece | Accepted; replaces 0155 |
| [0157](0157-a-fill-is-carried-by-the-hand-across-a-board.md) | A fill is carried by the hand across a board | Accepted; refines 0103 and 0109 |
| [0158](0158-the-directory-is-edited-by-a-window-of-its-own.md) | The directory is edited by a window of its own, one record per save | Accepted; implements `nomenclature.md` §12–§14, refines 0148 |
| [0159](0159-a-product-frame-reserves-a-footprint.md) | A product frame reserves a footprint, and a cut corner is two offsets | Accepted; refines 0097, 0100 and 0101 |
| [0160](0160-a-frames-box-is-the-box-tools-and-a-drawn-rectangle-is-the-plain-envelope.md) | A frame's box is the box tool's, and a drawn rectangle is the plain envelope | Accepted; amends the creation gesture of 0101, refines 0138 and 0159 |
| [0161](0161-a-carried-pieces-gaps-are-dimensioned-in-the-scene.md) | A carried piece's gaps are dimensioned in the scene | Accepted; amends 0119's placement for the furniture move, refines 0103 |
| [0162](0162-a-placement-may-not-put-a-part-inside-another.md) | A placement may not put a part inside another part | Accepted; carries 0102's contact rules into the write, narrows 0147's free move and X65's crossing clause |
| [0163](0163-the-furniture-move-carries-on-every-allowed-axis.md) | The furniture move carries a piece on every axis its class allows | Accepted; supersedes the single-axis latch of 0151 and narrows the one-axis carry of 0153; its camera-plane carry is superseded by 0164 |
| [0164](0164-a-carried-piece-is-met-on-the-plane-its-class-allows.md) | A carried piece is met on the plane its class allows | Accepted; supersedes 0163's camera-plane carry |
| [0165](0165-the-move-tool-is-the-object-move-and-a-solid-body-is-measured-by-its-faces.md) | The move tool is the object move, and a solid body is measured by its faces | Accepted; supersedes 0147's furniture-only scope, narrows 0162's snapping clause and 0163's world-box contact |
| [0166](0166-the-move-answers-the-place-the-piece-lands.md) | The move answers the place the piece lands, not the path it crosses | Accepted; supersedes 0165's swept stop and 0163's contact reading, narrows 0102's containment clause |
| [0167](0167-a-jump-is-held-for-a-moment-before-the-carry-takes-it.md) | A jump is held for a moment before the carry takes it | Accepted; refines 0166's destination rule and the touch reading of 0162's contact |
| [0168](0168-the-shell-writes-one-log-and-the-carry-logs-its-frame.md) | The shell writes one log, and the carry logs its frame | Accepted; installs the `log` sink the stack table promises, and measures a jump against the hand |
| [0169](0169-what-the-move-log-said.md) | What the move log said: a typed zero, a lost bond, and a stale hold | Accepted; fixes what 0168's first session showed, carries 0092's zero convention into the carry |
| [0170](0170-a-report-names-the-part-that-was-in-the-way.md) | A report names the part that was in the way | Accepted; extends what 0168 writes, so a cut names the obstacle and a take names the piece |
| [0171](0171-the-place-the-hand-names-is-asked-before-the-axes.md) | The place the hand names is asked before the axes | Accepted; narrows how 0166 resolves a blocked destination, so a free slot is taken whole |
| [0172](0172-a-crossing-rests-on-the-face-before-it-is-taken.md) | A crossing rests on the face before it is taken | Accepted; replaces 0167's step comparison with a geometric crossing and a cursor gate |
| [0173](0173-a-cut-is-measured-from-where-the-piece-stands.md) | A cut is measured from where the piece stands | Accepted; completes 0171's axis walk, so a piece resting past a body is not pulled back across it |
| [0174](0174-the-move-answers-places-and-the-jump-stays.md) | The move answers places, and the jump stays | Accepted; the reference record of the move at `21d94d2`, and the owner's call that the jump crosses material too |
| [0175](0175-a-hidden-part-is-sight-not-material.md) | A hidden part is sight, not material: the tools pass it and the part stops at its faces | Accepted |
| [0176](0176-the-plan-shows-a-stream-and-one-line-keeps-one-writer.md) | The plan shows a stream, and one line keeps one writer | Accepted; amends 0085's one-live-task rule |
| [0177](0177-the-invisible-floor-stops-a-body-and-not-a-drag-only-snap.md) | The invisible floor stops a body, and it is not the floor snap | Accepted; amends 0146's scope to dragging |
| [0178](0178-a-piece-the-document-holds-inside-another-may-only-come-out.md) | A piece the document holds inside another may only come out | Accepted; amends 0162's exemption and 0166's answer |
| [0179](0179-the-contact-port-lives-with-the-geometry-foundation.md) | The contact port lives with the geometry foundation, and parry answers it | Accepted; the port half of X85, amends 0165's world-box reading |
| [0181](0181-a-drag-prepares-the-bodies-it-reaches.md) | A drag prepares the bodies it reaches, and the port can cast a ray | Accepted; the rule's build is a walk over boxes, and the probe keeps the box reading until X92 |
| [0182](0182-the-file-manager-chooses-the-path.md) | The file manager chooses the path, and the window keeps drawing | Accepted |
| [0183](0183-the-command-line-is-the-one-headless-door.md) | The command line is the one headless door into the document | Accepted; "one headless door" narrowed by 0185; parameter-schema gap closed by 0195; the report set moved into the session by 0196 |
| [0184](0184-a-price-names-its-money-and-its-source.md) | A price names its money, its source and its age; stock is an overlay | Accepted |
| [0185](0185-the-tool-registry-is-one-table-and-every-door-reads-it.md) | The tool registry is one table, and the CLI and the MCP server are its doors | Accepted; narrows 0028 and 0183 |
| [0186](0186-the-mcp-crate-sits-beside-the-ui.md) | The MCP crate sits beside the UI, and the chat panel lives in the shell | Accepted; narrows 0028 |
| [0187](0187-a-chat-turn-is-one-undo-entry-and-a-plan-first.md) | A chat turn is one undo entry, and the first turn is a plan | Accepted; its two modes named by 0190 |
| [0188](0188-a-local-control-channel-is-not-a-network-service.md) | A local control channel is not a network service | Accepted; clarifies 0028 |
| [0189](0189-the-model-is-an-openai-compatible-endpoint.md) | The model is an OpenAI-compatible endpoint, and the key is not in the settings file | Accepted |
| [0190](0190-plan-and-build-are-the-persons-switch.md) | Plan and build are the person's switch | Accepted; refines 0187 |
| [0191](0191-the-inspectors-first-seven-findings.md) | The inspector's first seven findings | Accepted |
| [0192](0192-a-recipe-is-rust-that-emits-commands.md) | A recipe is Rust that emits commands, and the recipe language waits | Accepted |
| [0193](0193-an-agent-task-is-the-acceptance-test.md) | An agent task is the acceptance test | Accepted |
| [0195](0195-the-parameter-schema-derives-where-its-type-lives.md) | The parameter schema derives where its type lives | Accepted |
| [0180](0180-control-belongs-to-the-object-tool.md) | Control belongs to the object tool: the free move and the rotation are not measured | Accepted; amends 0162's every-write rule for the two hand-driven gestures |
| [0194](0194-a-contour-boolean-is-a-mode-that-stores-a-new-object.md) | A contour boolean is a mode that stores a new object | Accepted; implements 0043, the tool mark follows the file schema |
| [0196](0196-the-read-model-is-one-typed-question.md) | The read model is one typed question, and its stable answers are resources | Accepted; implements 0183's report set and 0185's "session read-only queries" |
| [0197](0197-a-body-boolean-is-a-step-that-names-its-tool.md) | A body boolean is a step that names its tool | Accepted; implements 0041's boolean steps as a document feature |
| [0198](0198-a-batch-is-planned-on-a-copy-and-lands-in-one-piece.md) | A batch is planned on a copy and lands in one piece | Accepted; implements the session operation 0190 asked for |
| [0199](0199-one-set-of-scene-snaps.md) | One set of scene snaps, resolved in the model | Accepted; extends 0051 and narrows its "no document" clause |
| [0200](0200-a-wall-fills-a-void-and-a-back-board-is-born-from-the-gap.md) | A wall fills a void, and a back board is born from the gap | Accepted; amends the `fill` trait of 0097 and the fill rule of 0099 |
| [0201](0201-the-niche-starts-from-the-frames-footprint.md) | The niche starts from the frame's footprint | Accepted; amends 0106 and the containment rule of 0099; the chord rule for a section's curve superseded by 0202; the non-convex refusal superseded by 0203 |
| [0202](0202-a-section-keeps-the-frames-curve.md) | A section keeps the frame's curve | Accepted; supersedes the chord rule of 0201 for the section |
| [0203](0203-a-frame-drawn-as-a-ring-builds-the-part-the-pointer-names.md) | A frame drawn as a ring builds the part the pointer names | Accepted; supersedes the non-convex refusal of 0201 and the box shortcut of 0106 for a shaped frame; its piece-only seed superseded by 0204 for the section and the box |
| [0204](0204-a-niche-keeps-the-whole-shape-the-frame-draws.md) | A niche keeps the whole shape the frame draws | Accepted; supersedes the piece-only seed of 0203 for the section and the box; amends the chord rule of 0201 for a shelf's section |
| [0205](0205-the-areas-of-a-drawing-are-derived-on-read.md) | The areas of a drawing are derived on read | Accepted; completes the contour sets of 0194 and takes the hole question of G3 |
| [0206](0206-a-boolean-result-is-one-object.md) | A boolean result is one object, a hole is part of one region, and splitting is a command | Accepted; the tree rule the contour boolean, a body boolean and the profile of 0205 read |
| [0207](0207-the-token-comes-first-and-the-window-answers-between-frames.md) | The token comes first, and the window answers between frames | Accepted; fills in the handshake, the mailbox and `ui_inspect` of 0188; its `busy` answer is amended by 0223 |
| [0208](0208-a-primitive-is-a-body-a-boolean-cuts.md) | A primitive is a body, so a boolean cuts it in both directions | Accepted; closes the consequence of 0197 |
| [0209](0209-the-assistant-panel-is-a-window-and-its-turn-runs-between-frames.md) | The assistant's panel is a window, and its turn is driven between frames | Accepted; places the panel 0186 and 0187 asked for; its chrome place extended by 0222 |
| [0210](0210-the-final-render-is-our-own-tracer-and-a-job.md) | The final render is our own tracer in `crates/final`, and it is a job rather than a document command | Accepted; the road Y2, Y3, Y4 and Y5 build on |
| [0211](0211-the-render-is-a-place-beside-the-file-menu.md) | The final render is a place beside the File menu, not a dialog | Accepted; the surface the entry point of 0210 is reached from |
| [0212](0212-a-batch-names-the-product-it-works-in.md) | A batch names the product it works in, and opening is not a step | Accepted; the field 0098 leaves a caller without |
| [0213](0213-a-frame-carries-a-finish-and-the-tracer-samples-by-power.md) | A frame carries a finish, and the final render samples the light it is given | Accepted; the frame contract Y3 fills from the catalogue |
| [0214](0214-a-batch-step-names-what-it-made.md) | A batch step names what it made, and a later step refers to the name | Accepted; the hole 0198 leaves a recipe with |
| [0215](0215-the-domain-is-taught-and-a-construction-is-composed.md) | The program teaches its domain, and a construction is composed | Accepted; supersedes the recipe of 0192 |
| [0216](0216-a-decor-is-the-boards-picture-its-size-and-its-grain.md) | A decor is the board's picture, the size it covers and its grain | Accepted; the render half of the material library (Y3) |
| [0217](0217-supply-names-four-sources.md) | Supply names four sources, and a product is assembled | Accepted; amends the trait list of 0094 |
| [0218](0218-an-overlays-pair-of-inks-is-the-themes.md) | An overlay's pair of inks is the theme's, and its quiet half is a neutral | Accepted; pays part of the overlay leftover of 0087 `[gizmo]` |
| [0219](0219-the-base-of-an-armed-boolean-has-an-ink-of-its-own.md) | The base of an armed boolean has an ink of its own | Accepted |
| [0220](0220-a-drawing-becomes-a-body-per-ring.md) | A drawing becomes a body per ring, and the area pull is the Extrude cell's second method | Accepted; the Extrude cell's gesture of 0205 becomes its second method |
| [0221](0221-a-ring-is-one-closed-chain-not-three-segments.md) | A ring is one closed chain, not three segments | Accepted; corrects the minimum the domain stated for a contour |
| [0222](0222-the-assistants-panel-has-one-header-cell.md) | The assistant's panel has one header cell | Accepted; extends the place 0209 left to the key |
| [0223](0223-a-call-waits-for-the-frame-that-has-the-window.md) | A call waits for the frame that has the window, and a caller that has gone is not performed | Accepted; amends the `busy` answer of 0207 |
| [0224](0224-two-plan-writers-in-one-checkout-are-one-at-a-time.md) | Two plan writers in one checkout are one at a time | Accepted; amends the `plan/.lock` consequence of 0085 |
| [0225](0225-a-queued-call-rings-the-window.md) | A queued call rings the window, and the frame that lands it asks for the next one | Accepted; completes the repainting clause 0223 left to X111 |
| [0226](0226-a-command-names-the-piece-and-the-interface-keeps-it-open.md) | A command names the piece it writes in, and the interface keeps it open | Accepted; amends the command-vs-button clause of 0098 and the `enabled` column of 0185 |
| [0227](0227-a-released-build-carries-its-words-on-the-surface.md) | A released build carries its words on the surface | Accepted; states the units clause of 0185 with an annotation, refuses the side-vocabulary half of X118 |
| [0228](0228-the-tool-server-asks-for-a-picture-it-is-handed.md) | The tool server asks for a picture it is handed | Accepted; states the screenshot channel 0028 left open |
| [0230](0230-an-agent-process-owns-the-model-loop.md) | An agent process owns the model loop, and the bridge is what it holds | Accepted |
| [0231](0231-the-list-is-short-rows-and-a-schema-is-read-by-id.md) | The list is short rows, and a schema is read one id at a time | Accepted; amends the `command_list` answer of 0185 and the dictionary clause of 0227 |
| [0232](0232-the-chat-offers-the-tools-the-door-publishes.md) | The chat offers the tools the door publishes, and the history keeps every call | Accepted; supersedes the no-read-tool ruling of J14, refines 0185, 0189 and 0190 |
| [0233](0233-a-tab-owns-its-assistant-conversation.md) | A tab owns its assistant conversation, and one panel draws the tab in front | Accepted; tab routing of a process's calls is debt |
| [0234](0234-a-batch-moves-the-revision-once.md) | A batch moves the revision once, and the answer says how many entries it left | Accepted |
| [0235](0235-a-provider-is-data-and-the-endpoint-lists-the-model.md) | A provider is data: presets fill the address, and the endpoint lists the model | Accepted; amends 0189; the anthropic wire is debt |

## Format

```text
# NNNN. Title

- Status: Accepted | Superseded by NNNN | Accepted; <aspect> superseded by NNNN
- Date: YYYY-MM-DD

## Context
What forced the decision.

## Decision
What we do, in the present tense.

## Consequences
What it buys, what it costs, and the condition under which it should be revisited.
```

The code and `AGENTS.md` stay the authority for rules; a record keeps the reason.
