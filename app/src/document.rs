//! One open scene and everything bound to it.
//!
//! A document is the whole payload of one tab: the session (project, undo history, dirty marker),
//! the editor (selection, tools, cards), the evaluation cache and the kernel, the frame evaluated
//! from them, the split layout and its cameras, the renderer target of this tab and the gizmos that
//! act on it. Activating another tab swaps this object and nothing else happens — no file is read
//! or written, so a workspace holds several scenes at once (U45).
//!
//! The split is the one ADR 0029 draws: a document never stores a viewport, a hover highlight or a
//! window fold. What the *window* decides — the pointer, the armed navigation gesture, the
//! transform mode, the measured ribbon band, the viewport rectangle and the display defaults —
//! arrives as [`Shell`] and as the workspace [`Settings`], so two tabs share one window without
//! sharing one scene.
//!
//! Changes when: a scene gains a piece of derived state (a cache, a frame, a renderer target) or a
//! tool that acts on it; not when a panel or a card gains a field.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui;
use eframe::egui_wgpu;
use eframe::wgpu;
use engine::mesh_cmds::MESH_VERTICES_MOVE_ID;
use engine::node_cmds::{NODE_DELETE_MANY_ID, NODE_PLACEMENT_SET_MANY_ID};
use engine::{MeshVertexMove, MeshVerticesMove, NodeDeleteMany, NodePlacement, NodePlacementSetMany, Session};
use kernel_truck::TruckKernel;
use mesh::{EvaluationCache, object_mesh};
use model::{Catalog, CornerMove, Geometry, Id, NodeQuery, Placement, Project, Revision, StructuralKind};
use project::SceneFile;
use render::{ViewSpec, ViewportRenderer};
use scene::{Camera, Lighting, Polyline, Projection, SELECTION_LINE_COLOR, SceneColors, SceneFrame};
use text::Lang;
use ui::settings::{Settings, ViewportSettings};
use ui::{
    BoxDraft, ContourGrip, DistanceFields, DistanceOutcome, Distances, DrawDraft, Editor, FieldPlacement, GripValue,
    MeshElement, Message, PaletteContext, Pick, Side, ToolAction, ToolOutcome, ToolParam, ToolView, TransformMode,
    ViewFlags, tool_card,
};
use units::Length;

use crate::box_gizmo::{self, BoxEvent, BoxGrips, GripUpdate};
use crate::contour_gizmo;
use crate::contour_gizmo::ContourGizmo;
use crate::draw_gizmo::{DrawAction, DrawGizmo, DrawModifiers};
use crate::extrude_gizmo::{ExtrudeEvent, ExtrudeGizmo};
use crate::file_menu::{FileAction, FileMenu, PreservedSections};
use crate::fill_gizmo::FillGizmo;
use crate::gizmo;
use crate::gizmo_math;
use crate::gizmo_math::{MoveKind, PlacementCommit, Preview, PreviewKey, WorkPlane};
use crate::measure_gizmo::{MeasureGizmo, MeasureModifiers};
use crate::mesh_pick;
use crate::open_frame;
use crate::osd;
use crate::rotate_gizmo::RotateGizmo;
use crate::scene_build::{Bounds, LevelBounds, evaluate, home_camera};
use crate::scene_gizmo;
use crate::shortcuts::{DraftKey, Shortcuts, drawing_key, take_drawing_shortcut};
use crate::side_panels::Chrome;
use crate::strip::{strip_area, view_region};
use crate::translate_gizmo::TranslateGizmo;
use crate::vertex_gizmo;
use crate::vertex_gizmo::{MeshVertexCommit, VertexGizmo, VertexPreview};
use crate::view_cluster;
use crate::view_cluster::{ClusterAction, ClusterView, NavigationMode, ViewCluster};
use crate::view_layout::{Layout, LayoutKind, LeafId};
use crate::void_gizmo::{self, VoidGizmo};
use crate::{Shell, Transform, theme};

/// Orbit sensitivity in radians per point dragged.
const ORBIT_RADIANS_PER_POINT: f64 = 0.008;

/// Zoom sensitivity per scroll point.
const ZOOM_PER_SCROLL_POINT: f64 = 0.004;

/// Colour and radius of an unselected mesh vertex marker.
const MESH_VERTEX_COLOR: egui::Color32 = egui::Color32::from_rgb(226, 226, 226);
const MESH_VERTEX_RADIUS: f32 = 4.0;

/// Translucent fill of a selected face.
const MESH_FACE_FILL: egui::Color32 = egui::Color32::from_rgba_premultiplied(70, 41, 11, 70);

/// Width of the bright stroke that repeats a selected line chain over the viewport image.
const SELECTION_OUTLINE_WIDTH: f32 = 2.25;

/// Width of the dark underlay below [`SELECTION_OUTLINE_WIDTH`], which keeps the stroke readable
/// over a light panel.
const SELECTION_UNDERLAY_WIDTH: f32 = 3.5;

/// The selected-chain stroke, converted from the scene's explicit line colour so the overlay and
/// the uploaded chain agree.
const SELECTION_OUTLINE: egui::Color32 = egui::Color32::from_rgb(
    (SELECTION_LINE_COLOR[0] * 255.0 + 0.5) as u8,
    (SELECTION_LINE_COLOR[1] * 255.0 + 0.5) as u8,
    (SELECTION_LINE_COLOR[2] * 255.0 + 0.5) as u8,
);

/// Dark underlay of the selected-chain stroke, premultiplied so the panel still shows through.
const SELECTION_UNDERLAY: egui::Color32 = egui::Color32::from_rgba_premultiplied(10, 16, 19, 220);

/// How far the line of a snapped drag reaches past the moved box, in millimetres.
const CAPTURE_OVERHANG_MM: f64 = 12.0;

/// The display state the window owns and every frame of this tab is built with.
///
/// The floor grid, the edge list, the ground shadow's strength, the floor's opacity and the colours
/// of the active theme variant are the window's (ADR 0029), so the shell hands them to the active
/// document; a frame rebuilt by a command, a card or a shortcut then draws the same display state
/// the viewport shows. The shadow's *switch* is the view's, so it is not here (U107).
#[derive(Clone, Copy, Debug)]
struct Display {
    /// Whether the floor grid draws.
    grid: bool,
    /// The minor grid spacing, in millimetres.
    grid_step_mm: u32,
    /// Whether the edge pass draws the full triangulation instead of only the form edges.
    all_edges: bool,
    /// How far the ground shadow darkens the floor plate, from 0 to 1.
    shadow_strength: f32,
    /// How opaque the floor plate is, from 0 to 1: zero draws neither plate nor shadow (U109).
    floor_opacity: f32,
    /// How much of the floor grid's inks shows, from 0 to 1.
    grid_strength: f32,
    /// How opaque the form-outline ink is, from 0 to 1.
    edge_strength: f32,
    /// The background, grid and line ink of the theme variant the window draws with.
    colors: SceneColors,
}

impl Display {
    /// Reads the display state the window currently shows.
    fn of(viewport: &ViewportSettings, colors: SceneColors) -> Self {
        Self {
            grid: viewport.grid,
            grid_step_mm: viewport.grid_step_mm,
            all_edges: viewport.all_edges,
            shadow_strength: ui::display::display_strength(viewport.shadow_strength),
            floor_opacity: ui::display::floor_opacity(viewport.floor_opacity),
            grid_strength: ui::display::display_strength(viewport.grid_strength),
            edge_strength: ui::display::display_strength(viewport.edge_strength),
            colors,
        }
    }

    /// Writes the display state into a frame.
    ///
    /// The shadow's switch is not here: it is the view's, so the active leaf has already put it in
    /// place and a whole-struct write here would overwrite it (U107). Its strength is, because the
    /// intensity is one workspace value — and so are the grid's and the outline's, which the frame
    /// carries as opacities the renderer reads when it builds the lines. The floor's opacity is a
    /// number the plate's fragment reads, so it travels the same way (U109).
    fn apply(self, scene: &mut SceneFrame) {
        scene.grid.visible = self.grid;
        scene.grid.spacing_mm = f64::from(self.grid_step_mm.max(1));
        scene.grid.ink_strength = self.grid_strength;
        scene.all_edges = self.all_edges;
        scene.edge_strength = self.edge_strength;
        scene.contact_shadows.strength = self.shadow_strength;
        scene.floor_opacity = self.floor_opacity;
        scene.apply_colors(self.colors);
    }

    /// Whether a repaint and an upload of the frame's own buffers must follow a change.
    ///
    /// The three strengths live in the colours and the inks the line buffers are built from, so a
    /// change to one costs a re-upload of the grid and the outlines, not a re-evaluation of the
    /// document (U64, U107). The floor's opacity and the shadow's strength are absent on purpose:
    /// they are read from the uniform when the plate draws, so turning one moves no buffer at all.
    fn repaints(self, previous: Self) -> bool {
        self.colors != previous.colors
            || self.grid_strength != previous.grid_strength
            || self.edge_strength != previous.edge_strength
    }
}

/// How long a rebuild may take before the shell announces the next one.
///
/// The evaluation is synchronous, so the only honest moment to paint «Перестроение…» is the frame
/// *before* it runs: a scene whose last rebuild overran this budget waits one frame, keeps the last
/// good geometry on screen and paints the busy state, and the work runs on the next frame, so the
/// window answers input in between. A cheap scene never shows the message at all.
pub(crate) const SLOW_REBUILD: Duration = Duration::from_millis(250);

/// What the last rebuild cost, and whether the pending one has announced itself yet.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Rebuild {
    /// How long the last evaluation took, if this scene has been evaluated since it opened.
    last: Option<Duration>,
    /// Whether the pending rebuild already painted its busy state.
    announced: bool,
}

impl Rebuild {
    /// Whether the last rebuild overran [`SLOW_REBUILD`].
    pub(crate) fn slow(&self) -> bool {
        self.last.is_some_and(|last| last >= SLOW_REBUILD)
    }

    /// Whether the status bar says «Перестроение…» while a rebuild is pending.
    pub(crate) fn busy(&self, pending: bool) -> bool {
        pending && (self.slow() || self.announced)
    }

    /// Whether the pending rebuild waits one frame so its busy state can be painted first.
    ///
    /// A cheap rebuild is never announced and never waits: the extra frame is paid only by the
    /// scenes that would have blocked the frame that asked for it.
    pub(crate) fn defer(&mut self, pending: bool) -> bool {
        if !pending {
            self.announced = false;
            return false;
        }
        if self.slow() && !self.announced {
            self.announced = true;
            return true;
        }
        false
    }

    /// Records how long a rebuild took.
    pub(crate) fn record(&mut self, elapsed: Duration) {
        self.last = Some(elapsed);
        self.announced = false;
    }
}

/// The GPU handles one tab draws with: the window's render state, this tab's render target and the
/// texture egui draws that target into.
///
/// The window always has one. A test builds the application without a device, an adapter or a
/// window, and then the document holds none: the frame still lays out, picks and paints its
/// overlays, it only draws no scene image (U77).
struct Target {
    /// The window's handles, cloned once, so a tab never reaches back into the shell.
    render_state: egui_wgpu::RenderState,
    /// This tab's colour target: one image for every view of the split layout.
    renderer: ViewportRenderer,
    /// The egui texture that target is drawn through.
    texture_id: egui::TextureId,
}

impl Target {
    /// Creates a `width` x `height` pixel target and registers it with egui.
    fn new(render_state: &egui_wgpu::RenderState, width: u32, height: u32) -> Self {
        let renderer = ViewportRenderer::new(&render_state.device, width, height);
        let texture_id = render_state.renderer.write().register_native_texture(
            &render_state.device,
            renderer.color_view(),
            wgpu::FilterMode::Linear,
        );
        Self { render_state: render_state.clone(), renderer, texture_id }
    }

    /// Uploads one frame of this scene into the target.
    fn upload(&mut self, scene: &SceneFrame) -> Result<(), String> {
        self.renderer.upload(&self.render_state.device, scene).map_err(|error| error.to_string())
    }

    /// Follows the viewport's pixel size; a size the target already has costs nothing.
    fn resize(&mut self, width: u32, height: u32) {
        let device = self.render_state.device.clone();
        if self.renderer.resize(&device, width, height) {
            self.render_state.renderer.write().update_egui_texture_from_wgpu_texture(
                &device,
                self.renderer.color_view(),
                wgpu::FilterMode::Linear,
                self.texture_id,
            );
        }
    }

    /// One pass per view into this target, in the order the layout tiles them.
    fn render(&mut self, scene: &SceneFrame, lighting: &Lighting, views: &[ViewSpec]) {
        self.renderer.render_views(&self.render_state.device, &self.render_state.queue, scene, lighting, views);
    }

    /// Hands the texture back to the window's renderer, so a closed document leaves no target behind.
    fn release(self) {
        self.render_state.renderer.write().free_texture(&self.texture_id);
    }
}

/// One open scene and everything derived from it.
pub(crate) struct Document {
    pub(crate) session: Session,
    pub(crate) editor: Editor,
    kernel: TruckKernel,
    /// Derived node geometry keyed by the inputs it was built from; a rebuild reuses every node
    /// the editing did not touch.
    cache: EvaluationCache,
    scene: SceneFrame,
    /// This scene's views: their cameras, presets and the tree that lays them out. View state in
    /// the tab; the document never sees it (ADR 0029).
    layout: Layout,
    /// World-space bounds of the latest evaluation, so *Frame all* fits the objects the scene has
    /// now instead of the ones it had when the file opened.
    scene_bounds: Bounds,
    scene_revision: Revision,
    /// The editor's visibility revision the frame was built from. A hidden subtree changes the
    /// frame without touching the document revision, so the rebuild watches both (ADR 0065).
    scene_visibility: u64,
    /// The preview overlay's version the frame was built from. A previewing operation shows a node
    /// the document does not hold yet, so the frame follows the overlay as it follows the revision.
    scene_preview: u64,
    /// The open product the frame was built from. Openness is session view state, not a revision
    /// (ADR 0098), so the rebuild watches it here: opening a piece repaints its envelope ink
    /// without touching the document.
    scene_product: Option<Id>,
    /// How the last rebuild cost, so a scene that takes more than a frame announces the next one.
    rebuild: Rebuild,
    /// The frame meter the on-screen display prints: the smoothed frame time and the last rebuild,
    /// upload and pick, each recorded where the work happens (U106).
    osd: osd::Meter,
    /// The rectangle the tool card drew last frame, so a press on it is chrome and never a pick.
    tool_card_rect: Option<egui::Rect>,
    /// The row of the tool card that takes the keyboard, consumed by the frame that draws it.
    tool_focus: Option<ToolParam>,
    /// The point the tool card hangs from, frozen while the pointer is on the card itself.
    ///
    /// A card that follows the pointer would move away from the hand reaching for its field: the
    /// anchor is the last point in the scene, and it is kept while the pointer is over the card
    /// (ADR 0119).
    tool_card_anchor: Option<egui::Pos2>,
    /// The context menu a right click opened over the scene, and what it is about (U92).
    ///
    /// It is view state of this document: the menu closes when the pointer picks outside it, when
    /// the tab changes, and on Escape, so no surface keeps a stale rectangle.
    context_menu: ui::ViewportMenu,
    /// The rectangle the last drawn frame gave the split views: the scene under the chrome. The
    /// floating panels never narrow it (U72). The shell reads it to clip the work crosshair, and a
    /// frame drawn without a window reports the same rectangle, so the frame tests assert the
    /// geometry the window really uses (U78).
    scene_rect: Option<egui::Rect>,
    /// The window's GPU handles and this tab's render target; `None` in a headless run (U77).
    target: Option<Target>,
    file: FileMenu,
    /// Top-level sections of the loaded file that this build does not understand; written back on
    /// save so an older build never silently destroys a newer build's data.
    extra: PreservedSections,
    translate_gizmo: TranslateGizmo,
    /// Whether the window is currently hiding the pointer for the carry (ADR 0156).
    cursor_hidden: bool,
    rotate_gizmo: RotateGizmo,
    vertex_gizmo: VertexGizmo,
    pub(crate) contour_gizmo: ContourGizmo,
    pub(crate) draw_gizmo: DrawGizmo,
    pub(crate) extrude_gizmo: ExtrudeGizmo,
    /// The empty-space tool's ghost; armed by the ribbon, it follows the pointer and creates a part.
    pub(crate) fill_gizmo: FillGizmo,
    /// The ruler's snapped point; the measurement itself lives in the editor (ADR 0118).
    pub(crate) measure_gizmo: MeasureGizmo,
    /// The niche tool's overlay; the cell itself lives in the editor (ADR 0118).
    pub(crate) void_gizmo: VoidGizmo,
    /// The grips of the selected box: four base corners and six faces, one drag each.
    pub(crate) box_grips: BoxGrips,
    /// The rubber band a primary drag on empty space draws, between its press and its release (U103).
    ///
    /// Only work-area paint and the selection read it: no mode owns the rectangle, and the document
    /// never sees one.
    band: Option<ui::SelectionBand>,
    /// The exact-distance fields of the running fill gesture: their pins, locks and keyboard.
    pub(crate) distance_fields: DistanceFields,
    /// The measurement the fields draw, rebuilt from the ghost every frame (ADR 0103).
    pub(crate) distances: Option<Distances>,
    /// The exact distances of the translate drag in progress, measured once when it starts.
    move_distances: Option<MoveDistances>,
    /// The corner orientation widget, which owns the roll-ring drag across frames.
    gizmo: gizmo::Gizmo,
    /// The right-click menu of a running polyline: the kind of the next segment and the way out.
    draft_menu: ui::DraftMenu,
    highlighted: Vec<Id>,
    /// The selected line chains of the frame last uploaded, already carrying any drag preview, so
    /// the screen-space outline follows a moved path without a frame of lag.
    selected_chains: Vec<Polyline>,
    /// The world box of every container from the last rebuild. Which of them are open is read from
    /// the editor when the brackets are painted, so opening a level evaluates no node (U89).
    level_bounds: Vec<LevelBounds>,
    /// The brackets the last frame painted, so a frame test counts what the viewport really draws.
    pub(crate) brackets: Vec<open_frame::Bracket>,
    uploaded_preview: Option<PreviewKey>,
    /// The failure this tab last reported, drawn by the status bar of the window.
    pub(crate) last_error: Option<String>,
    /// The window's display state, kept so a frame rebuilt outside the viewport draws what the
    /// viewport would.
    display: Display,
}

impl Document {
    /// Builds a document around a session: evaluates the scene, fits every view and, when the window
    /// has a GPU, registers its own render target.
    ///
    /// The workspace defaults come from `settings` and the display diagnostics, including the
    /// active theme's viewport colours, from the window's `shell`; none of it ever enters the scene
    /// file. `render_state` is `None` in a headless run, where a test drives the real frame with no
    /// device and no window (U77).
    pub(crate) fn new(
        render_state: Option<&egui_wgpu::RenderState>,
        settings: &Settings,
        shell: &Shell,
        session: Session,
        extra: PreservedSections,
    ) -> Result<Self, String> {
        let mut kernel = TruckKernel::new();
        let mut cache = EvaluationCache::new();
        let evaluated = evaluate(&mut cache, &mut kernel, &session, &ViewFlags::default(), session.active_product())?;
        let scene_bounds = evaluated.bounds;
        let level_bounds = evaluated.levels;
        let fit = home_camera(scene_bounds)?;
        let layout = Layout::new(fit, settings.viewport.opening_view, settings.viewport.style);
        let display = Display::of(&settings.viewport, shell.colors);
        let mut scene = evaluated.frame;
        layout.apply_frame_display(&mut scene);
        display.apply(&mut scene);
        scene.camera = crate::scene_build::scene_camera(session.project(), layout.active_camera());

        let mut editor = Editor::new();
        // The fold and the pinned method of each tool are the ribbon state the workspace remembers; a
        // new tab opens the ribbon the way the user left the last one.
        editor.set_ribbon_open(settings.ribbon.open);
        editor.set_pinned_variants(settings.ribbon.pinned_variants.clone());
        let mut target = render_state.map(|render_state| Target::new(render_state, 1280, 800));
        if let Some(target) = target.as_mut() {
            target.upload(&scene)?;
        }

        Ok(Self {
            scene_revision: session.project().revision(),
            scene_visibility: 0,
            scene_preview: session.preview_version(),
            scene_product: session.active_product(),
            rebuild: Rebuild::default(),
            osd: osd::Meter::default(),
            tool_card_rect: None,
            tool_focus: None,
            tool_card_anchor: None,
            context_menu: ui::ViewportMenu::default(),
            scene_rect: None,
            session,
            editor,
            kernel,
            cache,
            scene,
            layout,
            scene_bounds,
            target,
            file: FileMenu::new(),
            extra,
            translate_gizmo: TranslateGizmo::default(),
            cursor_hidden: false,
            rotate_gizmo: RotateGizmo::default(),
            vertex_gizmo: VertexGizmo::default(),
            contour_gizmo: ContourGizmo::default(),
            draw_gizmo: DrawGizmo::new(),
            extrude_gizmo: ExtrudeGizmo::default(),
            fill_gizmo: FillGizmo::default(),
            measure_gizmo: MeasureGizmo::default(),
            void_gizmo: VoidGizmo,
            box_grips: BoxGrips::default(),
            band: None,
            distance_fields: DistanceFields::default(),
            distances: None,
            move_distances: None,
            gizmo: gizmo::Gizmo::default(),
            draft_menu: ui::DraftMenu::default(),
            highlighted: Vec::new(),
            selected_chains: Vec::new(),
            uploaded_preview: None,
            level_bounds,
            brackets: Vec::new(),
            last_error: None,
            display,
        })
    }

    /// Reads a scene file into a new document.
    ///
    /// Opening is not an undoable document edit: it installs a session, so this tab starts with an
    /// empty history, an empty selection and a fresh evaluation. The catalogue is the workspace's —
    /// it is session state, not scene data, and the file never carries it.
    pub(crate) fn open(
        render_state: Option<&egui_wgpu::RenderState>,
        settings: &Settings,
        shell: &Shell,
        path: &Path,
        catalog: &Catalog,
    ) -> Result<Self, String> {
        let scene = SceneFile::read(path).map_err(|error| error.to_string())?;
        let extra = scene.extra().clone();
        let project = scene.into_document();
        let session = Session::with_catalog(project, catalog.clone());
        let mut document = Self::new(render_state, settings, shell, session, extra)?;
        document.file.set_path(Some(path.to_owned()));
        Ok(document)
    }

    /// Writes the scene and marks the document as saved.
    pub(crate) fn save(&mut self, path: &Path) -> Result<(), String> {
        SceneFile::new(self.session.project().clone())
            .with_extra(self.extra.clone())
            .write(path)
            .map_err(|error| error.to_string())?;
        self.session.mark_saved();
        self.file.set_path(Some(path.to_owned()));
        Ok(())
    }

    /// The file the scene came from, if it has one.
    pub(crate) fn path(&self) -> Option<&Path> {
        self.file.path()
    }

    /// The name the tab row draws for this scene.
    pub(crate) fn file_name(&self, lang: text::Lang) -> String {
        self.file.file_name(lang)
    }

    /// Whether the document differs from what was last saved.
    pub(crate) fn is_dirty(&self) -> bool {
        self.session.is_dirty()
    }

    /// Whether the header's undo button has an entry to take back.
    pub(crate) fn can_undo(&self) -> bool {
        self.session.can_undo()
    }

    /// Whether the header's redo button has an entry to reapply.
    pub(crate) fn can_redo(&self) -> bool {
        self.session.can_redo()
    }

    /// Frees this tab's render target and drops the document.
    ///
    /// The texture is registered on the window's egui renderer and outlives the document that drew
    /// into it, so a workspace that opens and closes scenes has to hand it back; a headless
    /// document has none to give.
    pub(crate) fn release(mut self) {
        self.release_target();
    }

    /// Hands the render target back to the window's renderer, leaving the document alive.
    ///
    /// A closed scene gives it back once; a shot drawn offscreen has to give it back after every
    /// picture, because the window's renderer would otherwise hold one texture per shot (U81).
    pub(crate) fn release_target(&mut self) {
        if let Some(target) = self.target.take() {
            target.release();
        }
    }

    /// Whether the ribbon of this scene is unfolded.
    pub(crate) fn ribbon_open(&self) -> bool {
        self.editor.ribbon_open()
    }

    /// Folds or unfolds the ribbon of this scene, returning its new state.
    ///
    /// The fold is window chrome rather than scene data, so the shell writes it to the workspace
    /// settings and hands it to every open tab; the document only carries it to its editor.
    pub(crate) fn set_ribbon_open(&mut self, open: bool) -> bool {
        self.editor.set_ribbon_open(open)
    }

    /// The method each tool is pinned to, which is window chrome rather than scene data.
    pub(crate) fn pinned_variants(&self) -> &std::collections::BTreeMap<String, String> {
        self.editor.pinned_variants()
    }

    /// Replaces the pinned methods of this scene's ribbon from the workspace settings.
    pub(crate) fn set_pinned_variants(&mut self, pinned: std::collections::BTreeMap<String, String>) {
        self.editor.set_pinned_variants(pinned);
    }

    /// The selection and creation target the palette's parameter builders read.
    pub(crate) fn palette_context(&self) -> PaletteContext {
        PaletteContext::new(self.editor.selected_nodes().to_vec(), self.editor.creation_target(self.session.project()))
    }

    /// Draws the scene tree's body.
    ///
    /// The tree writes the session's hidden and locked flags directly, so the frame it just changed
    /// is rebuilt here instead of waiting a frame: `refresh` returns at once when neither the
    /// document nor the visible set moved.
    pub(crate) fn show_scene_tree(&mut self, ui: &mut egui::Ui) {
        self.editor.show_scene_tree(ui, &mut self.session);
        if let Err(error) = self.refresh() {
            self.last_error = Some(error);
        }
    }

    /// Draws the inspector's body: the armed tool's parameters first, then the selection's card.
    ///
    /// The tool section draws the same rows the card at the pointer draws, from the same draft, and
    /// the numbers it commits take the same path as the card's (ADR 0119).
    pub(crate) fn show_card(&mut self, ui: &mut egui::Ui) {
        let mut tool = self.tool_view();
        let reported = self.editor.show_card(ui, &mut self.session, tool.as_mut());
        self.apply_tool_outcome(ui.ctx(), reported);
    }

    /// Whether the tool card drew last frame, for the shot that looks at it.
    #[cfg(test)]
    pub(crate) fn tool_card_is_up(&self) -> bool {
        self.tool_card_rect.is_some()
    }

    /// Takes the transform mode a node menu asked for, for the shell to write.
    ///
    /// The mode belongs to the window and this tab only records the wish, so the shell drains it
    /// once per frame after the tree and the viewport have drawn (U113).
    pub(crate) fn take_mode_request(&mut self) -> Option<TransformMode> {
        self.editor.take_mode_request()
    }

    /// The rectangle of the armed tool's card, which the shell hands to `Chrome`.
    pub(crate) const fn tool_card_rect(&self) -> Option<egui::Rect> {
        self.tool_card_rect
    }

    /// The rectangle the running polyline's right-click menu drew last frame, when one is up.
    #[cfg(test)]
    pub(crate) fn draft_menu_rect(&self) -> Option<egui::Rect> {
        self.draft_menu.rect()
    }

    /// Takes back one command and reports a refusal in the status bar.
    pub(crate) fn undo(&mut self) {
        if let Err(error) = self.session.undo() {
            self.last_error = Some(error.to_string());
        }
    }

    /// Reapplies one undone command and reports a refusal in the status bar.
    pub(crate) fn redo(&mut self) {
        if let Err(error) = self.session.redo() {
            self.last_error = Some(error.to_string());
        }
    }

    /// The refusal a destructive palette command gets while the primary selection is locked.
    pub(crate) fn locked_command_refusal(&self, id: &str, lang: Lang) -> Option<String> {
        locked_command_refusal(&self.editor, self.session.project(), id, lang)
    }

    /// The save the header's button and `Ctrl+S` ask for: the current path, or the path dialog.
    pub(crate) fn request_save(&mut self) -> Option<FileAction> {
        self.file.save()
    }

    /// Opens the Open dialog from the Home surface, which has no File menu of its own.
    pub(crate) fn request_open(&mut self) {
        self.file.request_open();
    }

    /// Draws the File menu button and its dialogs.
    pub(crate) fn show_file_menu(&mut self, ui: &mut egui::Ui) -> Option<FileAction> {
        self.file.show(ui)
    }

    /// Draws the dialogs the File menu opened and returns the operation one of them asked for.
    pub(crate) fn show_dialogs(&mut self, ctx: &egui::Context) -> Option<FileAction> {
        self.file.show_dialogs(ctx)
    }

    /// Re-uploads the frame when the selection moved.
    ///
    /// Selecting is view state, so the revision does not move; the uploaded highlight is the copy
    /// the last frame was built with, and a difference means the drawn outline is stale.
    pub(crate) fn sync_selection(&mut self) -> Result<(), String> {
        match self.editor.selected_nodes() == self.highlighted {
            true => Ok(()),
            false => self.upload_scene(),
        }
    }

    /// The nodes the frame last uploaded draws as selected, oldest selection first.
    ///
    /// It is the selection the outline was built from, so a test can tell a highlighted member
    /// from a node that merely left the selection after the upload.
    #[cfg(test)]
    pub(crate) fn highlighted_nodes(&self) -> &[Id] {
        &self.highlighted
    }

    /// The rubber band the last frame drew, or `None` while no drag is live (U103).
    #[cfg(test)]
    pub(crate) fn rubber_band(&self) -> Option<ui::SelectionBand> {
        self.band
    }

    /// The frame the last upload built, so a frame test can ask which shapes the viewport drew.
    #[cfg(test)]
    pub(crate) fn frame(&self) -> &SceneFrame {
        &self.scene
    }

    /// Applies the viewport section of the workspace settings to this scene.
    ///
    /// The style default applies to the active view at once and the other views keep theirs; a grid,
    /// edge-list or theme change rebuilds the uploaded frame, while an edge style is frame data the
    /// renderer reads at draw time.
    pub(crate) fn apply_viewport_settings(
        &mut self,
        before: &ViewportSettings,
        settings: &ViewportSettings,
        shell: &Shell,
    ) {
        let previous = self.display;
        self.display = Display::of(settings, shell.colors);
        self.layout.set_active_style(settings.style);
        self.layout.apply_frame_display(&mut self.scene);
        self.display.apply(&mut self.scene);
        let geometry_changed = settings.grid != before.grid
            || settings.grid_step_mm != before.grid_step_mm
            || settings.all_edges != before.all_edges
            || self.display.repaints(previous);
        if geometry_changed && let Err(error) = self.upload_scene() {
            self.last_error = Some(error);
        }
    }

    /// Runs the global shortcuts the shell read.
    ///
    /// The keys are read by the shell, because a modal or a focused text field takes them first, but
    /// everything they act on belongs to the active document: one tab's undo never touches another.
    /// The save is returned rather than performed: writing a file is the shell's job, because it
    /// also remembers the path in the workspace settings.
    pub(crate) fn handle_shortcuts(&mut self, shortcuts: Shortcuts) -> Option<FileAction> {
        let save = if shortcuts.save { self.file.save() } else { None };
        if shortcuts.undo {
            self.undo();
        }
        if shortcuts.redo {
            self.redo();
        }
        if shortcuts.frame {
            self.frame_all();
        }
        if shortcuts.projection {
            self.toggle_view_projection();
        }
        if shortcuts.maximize {
            self.layout.toggle_maximized();
        }
        save
    }

    /// Turns the work-plane grid on or off for the tool in front of the person.
    ///
    /// The key is the shell's, so it is read with the rest of the key table even when no tool runs;
    /// outside a draft or the ruler the action has nothing to snap and changes nothing. The ruler
    /// reads the same switch as the drawing tools, because both snap on the one work plane (0051).
    pub(crate) fn toggle_draw_grid(&mut self) {
        if self.editor.drawing().is_some() || self.editor.measure().is_some() {
            self.draw_gizmo.toggle_grid();
        }
    }

    /// Removes nodes in one `node.delete.many`, so the whole batch is one undo entry.
    ///
    /// The refusal lands in the status bar like every other failure, and a valid batch leaves the
    /// scene to the next frame's rebuild, exactly as the undo the key next to it runs (ADR 0112).
    pub(crate) fn delete_nodes(&mut self, nodes: &[Id]) {
        let request = NodeDeleteMany { nodes: nodes.to_vec() };
        let dispatched = serde_json::to_value(request)
            .map_err(|error| format!("could not encode the node batch: {error}"))
            .and_then(|params| self.session.dispatch(NODE_DELETE_MANY_ID, &params).map_err(|error| error.to_string()));
        if let Err(error) = dispatched {
            self.last_error = Some(error);
        }
    }

    /// The projection the active view is drawing with, for the status bar's readout.
    pub(crate) fn projection(&self) -> Projection {
        self.camera().projection()
    }

    /// The camera the last drawn frame looked through.
    ///
    /// A camera or a light the document owns pins the view, so this is the authored camera in that
    /// case and the active view's own otherwise.
    pub(crate) fn camera(&self) -> Camera {
        self.scene.camera
    }

    /// Zooms the active view by `factor`, the way the wheel does: above one moves out.
    ///
    /// A shot composes its picture through the same view state a gesture writes, so the model can
    /// be put under the floating panels, which is where the translucency check looks for it (U81).
    #[cfg(test)]
    pub(crate) fn zoom_active_view(&mut self, factor: f64) {
        let active = self.layout.active();
        if let Some(leaf) = self.layout.leaf_mut(active) {
            leaf.zoom_about(factor, None);
        }
    }

    /// The scene rectangle the last drawn frame gave the split views, or `None` before the first
    /// frame.
    ///
    /// It is the part of the viewport below the floating ribbon band; the side panels float over it
    /// and never narrow it (U72). The shell reads it to clip the work crosshair, and a frame drawn
    /// with no window reports the same rectangle, so the frame tests assert the geometry the window
    /// really uses (U78).
    pub(crate) fn scene_rect(&self) -> Option<egui::Rect> {
        self.scene_rect
    }

    /// This tab's split views: their cameras, presets and the tree that lays them out.
    #[cfg(test)]
    pub(crate) fn layout(&self) -> &Layout {
        &self.layout
    }

    /// Whether the status bar should say «Перестроение…» on this frame.
    ///
    /// The shell asks before it syncs, so the frame that announces a heavy rebuild is the one that
    /// paints the message and the last good geometry, and the work runs on the frame after it. A
    /// cheap rebuild is never announced: it costs less than a frame and the message would be noise.
    pub(crate) fn busy(&self, viewport: &ViewportSettings, colors: SceneColors) -> bool {
        self.rebuild.busy(self.rebuild_pending(viewport, colors))
    }

    /// Whether the next [`Document::sync_scene`] will evaluate the scene.
    fn rebuild_pending(&self, viewport: &ViewportSettings, colors: SceneColors) -> bool {
        Display::of(viewport, colors).colors != self.display.colors
            || self.session.project().revision() != self.scene_revision
            || self.editor.visibility_revision() != self.scene_visibility
            || self.session.preview_version() != self.scene_preview
            || self.session.active_product() != self.scene_product
    }

    /// Takes the window's display state and rebuilds the frame when the document moved.
    ///
    /// The shell calls this once per frame for the active tab: the display switches belong to the
    /// window, the geometry to the tab, and a scene that did not move costs nothing. A scene whose
    /// last rebuild overran [`SLOW_REBUILD`] waits one frame here, so the busy state the shell just
    /// painted is on screen while the work runs; `ctx` asks for that frame.
    pub(crate) fn sync_scene(
        &mut self,
        ctx: &egui::Context,
        viewport: &ViewportSettings,
        colors: SceneColors,
    ) -> Result<(), String> {
        let previous = self.display;
        self.display = Display::of(viewport, colors);
        if self.display.repaints(previous) {
            // A theme switch or a display strength touches only the frame's own colours and inks, so
            // the uploaded frame is repainted without evaluating a single node: the document revision
            // does not move (X8, U64). The line buffers are keyed by those colours, so the upload
            // rebuilds the grid and the outlines and keeps every face and mesh it already has.
            self.display.apply(&mut self.scene);
            return self.upload_scene();
        }
        let pending = self.rebuild_pending(viewport, colors);
        if self.rebuild.defer(pending) {
            ctx.request_repaint();
            return Ok(());
        }
        self.refresh()
    }

    /// Re-evaluates the scene when the document revision, the visibility or the preview moved.
    ///
    /// Every operation that changes the document ends here, so a new frame is uploaded before the
    /// viewport draws again: the evaluated geometry, this tab's per-view styles and the window's
    /// display state are composed into one frame. The evaluation is timed, so the next rebuild of a
    /// scene that takes longer than a frame is announced before it runs (M71).
    fn refresh(&mut self) -> Result<(), String> {
        let revision = self.session.project().revision();
        let visibility = self.editor.visibility_revision();
        let preview = self.session.preview_version();
        let product = self.session.active_product();
        if revision == self.scene_revision
            && visibility == self.scene_visibility
            && preview == self.scene_preview
            && product == self.scene_product
        {
            return Ok(());
        }
        // A delete or an undo can take a flag with it; pruning before the rebuild keeps a stale
        // hidden or locked id out of the frame the user is about to see.
        self.editor.retain_flags(self.session.project());
        let visibility = self.editor.visibility_revision();
        let started = Instant::now();
        let evaluated = evaluate(&mut self.cache, &mut self.kernel, &self.session, self.editor.flags(), product)?;
        self.scene_bounds = evaluated.bounds;
        self.level_bounds = evaluated.levels;
        let mut scene = evaluated.frame;
        self.layout.apply_frame_display(&mut scene);
        self.display.apply(&mut scene);
        scene.camera = crate::scene_build::scene_camera(self.session.project(), self.layout.active_camera());
        self.scene = scene;
        self.scene_revision = revision;
        self.scene_visibility = visibility;
        self.scene_preview = preview;
        self.scene_product = product;
        let uploaded = self.upload_scene();
        let elapsed = started.elapsed();
        self.rebuild.record(elapsed);
        self.osd.record_rebuild(elapsed);
        uploaded
    }

    pub(crate) fn upload_scene(&mut self) -> Result<(), String> {
        let selection = self.editor.selected_nodes();
        let mut geometry = Vec::new();
        for node in self.session.project().top_most_nodes(selection) {
            geometry.extend(self.session.project().geometry_ids_under(node).unwrap_or_default());
        }
        let frame = self.scene.with_selection_highlight(&geometry);
        let frame = match self.preview() {
            Some(Preview::Translation { drawables, delta }) => frame.with_world_translation(drawables, delta),
            Some(Preview::Rotation { drawables, pivot, rotation }) => {
                frame.with_world_rotation(drawables, pivot, rotation)
            }
            None => frame,
        };
        let frame = match self.vertex_gizmo.preview() {
            Some(preview) => preview_vertex_mesh(frame, self.session.project(), preview)?,
            None => frame,
        };
        // Keep the chains of the frame that is about to be uploaded, after every preview, so the
        // screen-space outline is drawn where the renderer draws the path.
        self.selected_chains = frame.polylines().iter().filter(|chain| geometry.contains(&chain.id)).cloned().collect();
        let started = Instant::now();
        if let Some(target) = self.target.as_mut() {
            target.upload(&frame)?;
        }
        self.osd.record_upload(started.elapsed());
        self.highlighted.clear();
        self.highlighted.extend_from_slice(selection);
        self.uploaded_preview = self.preview_key();
        Ok(())
    }

    /// The scene change the active gizmo is previewing.
    fn preview(&self) -> Option<Preview<'_>> {
        if self.editor.contour_edit().is_some() || self.editor.mesh_edit().is_some() {
            return None;
        }
        // Only the armed gizmo is ever updated, so at most one of them holds a drag; asking both
        // means the frame follows the drag that is actually live even if the mode changed under it.
        self.translate_gizmo.preview().or_else(|| self.rotate_gizmo.preview())
    }

    /// The preview currently shown by the renderer.
    ///
    /// A contour edit paints its working copy as an overlay; the document solid is not rebuilt until
    /// the single command that applies the draft.
    fn preview_key(&self) -> Option<PreviewKey> {
        if self.editor.contour_edit().is_some() {
            return None;
        }
        if self.editor.mesh_edit().is_some() {
            return self.vertex_gizmo.preview_key();
        }
        self.translate_gizmo.preview_key().or_else(|| self.rotate_gizmo.preview_key())
    }

    /// Applies the live contour draft as one command and rebuilds the scene.
    pub(crate) fn apply_contour_edit(&mut self) -> Result<(), String> {
        self.editor.apply_contour_edit(&mut self.session)?;
        self.contour_gizmo.reset();
        self.refresh()
    }

    /// Runs what the contour gizmo asked for this frame.
    ///
    /// The gizmo owns the pointer but not the document, so a plate's number and a dropped move
    /// come back here: a number is applied to the working copy, and a drop is the same one command
    /// Enter runs (ADR 0129).
    fn apply_contour_action(&mut self, ctx: &egui::Context, action: contour_gizmo::GizmoAction) {
        use contour_gizmo::{GizmoAction, NumericAction};
        match action {
            GizmoAction::Idle => {}
            GizmoAction::Changed => ctx.request_repaint(),
            GizmoAction::Abandon => {
                if self.contour_gizmo.abandon_latch(&mut self.editor) {
                    ctx.request_repaint();
                }
            }
            GizmoAction::Drop => {
                // The plates may have locked a number on the frame the drop arrives, so the point
                // they name is applied before the working copy is written.
                if let Some((grip, point, motion)) = self.contour_gizmo.latched_move() {
                    let _ = self.editor.contour_edit_drag_grip(grip, point, motion);
                }
                if let Err(error) = self.drop_contour_move() {
                    self.last_error = Some(error);
                }
            }
            GizmoAction::Numeric(NumericAction::Apply(grip, value) | NumericAction::Restore(grip, value)) => {
                match self.editor.contour_edit_set_grip_value(grip, value) {
                    Ok(()) => ctx.request_repaint(),
                    Err(error) => self.last_error = Some(error.to_string()),
                }
            }
            GizmoAction::Numeric(NumericAction::ApplyAlong(grip, value, motion)) => {
                let result = if let (ContourGrip::Corner(_), CornerMove::Along(segment), GripValue::Length(length)) =
                    (grip, motion, value)
                {
                    self.editor.contour_edit_along_length(grip, segment, length)
                } else {
                    self.editor.contour_edit_set_grip_value(grip, value)
                };
                match result {
                    Ok(()) => ctx.request_repaint(),
                    Err(error) => self.last_error = Some(error.to_string()),
                }
            }
        }
    }

    /// Writes a latched corner move as one command and keeps reshaping the same contour.
    ///
    /// The drop is a command, so the mode restarts from what the document now stores: the next
    /// corner is latched against the new shape, and the undo history holds one entry per drop.
    fn drop_contour_move(&mut self) -> Result<(), String> {
        let Some(target) = self.editor.contour_edit().map(ui::ContourEdit::target) else {
            return Ok(());
        };
        self.apply_contour_edit()?;
        self.editor.start_contour_edit(self.session.project(), target);
        Ok(())
    }

    /// Leaves contour editing and throws the working copy away.
    pub(crate) fn cancel_contour_edit(&mut self) {
        self.editor.end_contour_edit();
        self.contour_gizmo.reset();
    }

    /// Leaves a drawing tool and throws the draft away.
    pub(crate) fn cancel_drawing(&mut self) {
        self.editor.cancel_drawing();
        self.draw_gizmo.reset();
        self.draft_menu.close();
    }

    /// Runs what a drawing click, the keyboard or a plate asked for.
    ///
    /// A fixed tool is done on its last point and a polyline is closed through its first point: both
    /// finish the drawing the way a CAD tool does, but only a close commits through the shared close
    /// path (ADR 0052). A cancel has already dropped the draft in the editor, so only the shell's
    /// pointer state is left. Returns whether the tool ended.
    fn apply_draw_action(&mut self, ctx: &egui::Context, action: DrawAction) -> bool {
        let lang = ui::language(ctx);
        match action {
            DrawAction::Point => {
                ctx.request_repaint();
                if !self.editor.drawing().is_some_and(DrawDraft::is_finished) {
                    return false;
                }
                if let Err(error) = self.commit_drawing(lang) {
                    self.last_error = Some(error);
                }
                true
            }
            DrawAction::Close => {
                ctx.request_repaint();
                if let Err(error) = self.close_draft(lang) {
                    self.last_error = Some(error);
                }
                true
            }
            DrawAction::Cancel => {
                self.cancel_drawing();
                true
            }
            DrawAction::Nothing => false,
        }
    }

    /// Commits the live drawing on the work plane and ends the tool.
    ///
    /// The plane is the one the draft started on, so a commit does not depend on where the camera
    /// has orbited since. A refused path leaves the draft live and reports why. A body cell's profile
    /// is not finished here: the shell starts the height pull on the path it just committed, in the
    /// same edit scope, so the box or cylinder is built on Z by the one gesture (ADR 0138).
    pub(crate) fn commit_drawing(&mut self, lang: Lang) -> Result<(), String> {
        let placement = self.drawing_placement()?;
        let drawn = self.editor.commit_drawing(&mut self.session, placement, lang)?;
        let profile = (drawn.intent == ui::DrawIntent::Body)
            .then_some(drawn)
            .and_then(|drawn| Some((drawn.object, drawn.scope?)));
        // A cone is drawn by its base and finished by its pull: the node is already in the document,
        // sharp, at the ribbon's height, and the pull chooses both numbers in the scope of that
        // create (ADR 0141).
        let cone = matches!(drawn.intent, ui::DrawIntent::Primitive(model::PrimitiveKind::Cone))
            .then_some(drawn)
            .and_then(|drawn| Some((drawn.object, drawn.scope?)));
        self.end_drawing()?;
        if let Some((object, scope)) = profile {
            // The ring is in the document and the height pull grows its body from it; the preview
            // shows the body at the typed height before the pointer moves (ADR 0138).
            self.extrude_gizmo.start_profile(object, scope, self.editor.extrude_height());
            if let Err(error) = self.preview_extrude(object, self.editor.extrude_height(), None) {
                self.last_error = Some(error);
            }
        } else if let Some((object, scope)) = cone {
            let height = self.editor.extrude_height();
            self.extrude_gizmo.start_cone(object, scope, height);
            if let Err(error) = self.preview_extrude(object, height, Some(Length::ZERO)) {
                self.last_error = Some(error);
            }
        }
        Ok(())
    }

    /// Closes the running ring and commits it in one step.
    ///
    /// The only close path: a click on the first draft point, the ribbon's *Close* and the status
    /// bar's *Close* all end here, so no surface can close a ring and forget to commit it (ADR 0052).
    /// The ring is closed before the commit and reopened if the document refuses it, so a refusal
    /// leaves the shape the user drew editable instead of a finished draft that cannot be changed.
    pub(crate) fn close_draft(&mut self, lang: Lang) -> Result<(), String> {
        let placement = self.drawing_placement()?;
        self.editor.close_and_commit_drawing(&mut self.session, placement, lang)?;
        self.end_drawing()
    }

    /// The placement of the work plane the live draft draws on.
    fn drawing_placement(&self) -> Result<Placement, String> {
        self.draw_gizmo
            .plane(self.scene.camera)
            .placement()
            .ok_or_else(|| "the work plane has no usable placement".to_owned())
    }

    /// Drops the pointer state a finished drawing leaves behind and rebuilds the scene.
    pub(crate) fn end_drawing(&mut self) -> Result<(), String> {
        self.draw_gizmo.reset();
        self.highlighted.clear();
        self.refresh()
    }

    /// Re-uploads the scene when the drag preview moved to a new position.
    ///
    /// A finished or cancelled drag also lands here with no preview, which restores the base
    /// upload. The key comparison keeps a still pointer from rebuilding the geometry every frame.
    /// A camera or light drag moves no drawable, so it only updates the lighting uniform and the
    /// glyphs; uploading the unchanged geometry again would cost a full vertex pass per frame.
    fn sync_preview(&mut self) -> Result<(), String> {
        if self.preview_key() == self.uploaded_preview {
            return Ok(());
        }
        if let Some(preview) = self.preview()
            && preview.drawables().is_empty()
        {
            self.uploaded_preview = self.preview_key();
            return Ok(());
        }
        self.upload_scene()
    }

    /// World placements the active drag previews for cameras and lights.
    ///
    /// A camera or a light has no drawable, so the scene frame cannot carry its preview. This list
    /// lets the icon layer and the lighting follow the pointer in the same frame as the axis
    /// handles instead of waiting for the command that lands on release.
    fn previewed_scene_placements(&self) -> Vec<(Id, Placement)> {
        preview_nodes(self.session.project(), self.editor.selected_nodes(), self.preview())
    }

    /// The vertices the active sub-object selection moves.
    ///
    /// Vertex mode is the stored list; face mode is the union of every selected polygon, so the
    /// gizmo, the preview and the dispatched command all see one deduplicated set.
    fn mesh_selection_vertices(&self, object: Id) -> Vec<u32> {
        let Some(edit) = self.editor.mesh_edit() else {
            return Vec::new();
        };
        let Some(mesh) = self.session.project().object(object).and_then(|object| object.geometry().as_mesh()) else {
            return Vec::new();
        };
        edit.selected_vertices(mesh)
    }

    /// Applies one event from the pull arrow.
    ///
    /// A drag only previews the height: the overlay shows the body the pull would produce and the
    /// document is not written. A click finishes the pull, and finishing is the one command.
    fn apply_extrude_event(&mut self, event: ExtrudeEvent, lang: text::Lang) -> Result<(), String> {
        match event {
            ExtrudeEvent::Preview { object, height, top } => self.preview_extrude(object, height, top),
            ExtrudeEvent::Finish { object, height, top } => self.finish_extrude(object, height, top, lang),
        }
    }

    /// Shows the pull's height as a session preview, without writing the document.
    ///
    /// The preview is the body itself at that height, evaluated from the same parameters the ✓
    /// would store, so the user sees the solid and not a stand-in for it. It moves no revision and
    /// leaves no undo entry (M71).
    pub(crate) fn preview_extrude(&mut self, object: Id, height: Length, top: Option<Length>) -> Result<(), String> {
        // The pull's own numbers are what its fields show, so a number typed into the card is
        // adopted here: the next frame draws the field and the arrow at the value just committed
        // instead of snapping back to the dragged one.
        self.extrude_gizmo.set_height(height);
        self.extrude_gizmo.set_top(top);
        let geometry = previewed_body(self.session.project(), object, height, top)?;
        self.session.set_preview_geometry(object, geometry);
        self.refresh()
    }

    /// Writes the pull's height once and drops the preview.
    ///
    /// A body already in the document takes the height alone. A profile the body cells drew has none:
    /// the ✓ is the command that builds its body from the ring the pull grew, and it joins the edit
    /// scope the profile was created in so the whole gesture is one undo entry (ADR 0138).
    ///
    /// Idempotent by construction: after the first call the pull is gone, so a second ✓ — a double
    /// click, a key and a button in one frame — has no object to write and dispatches nothing.
    pub(crate) fn finish_extrude(
        &mut self,
        object: Id,
        height: Length,
        top: Option<Length>,
        lang: text::Lang,
    ) -> Result<(), String> {
        if self.extrude_gizmo.is_growing() {
            let scope = self.extrude_gizmo.scope();
            self.editor.extrude_path_in(scope, &mut self.session, object, height, lang)?;
            self.session.end_edit();
        } else if let Some(top) = top {
            // A cone's ✓ writes both of its numbers into the scope the create opened, so the base
            // and the taper are one entry and the ✓ applies once (ADR 0141).
            let scope = self.extrude_gizmo.scope();
            let primitive = cone_at(self.session.project(), object, height, top)?;
            self.editor.replace_primitive_in(scope, &mut self.session, object, primitive)?;
            self.session.end_edit();
        } else if extrude_writes(self.extrude_gizmo.committed(), height) {
            self.editor.set_extrude_height(&mut self.session, object, height)?;
        }
        self.extrude_gizmo.cancel();
        self.session.clear_preview();
        self.refresh()
    }

    /// Drops the pull and its preview, leaving the body at the height the document holds.
    ///
    /// This is the ✕ and the Escape of the command bar: for a body nothing was written, so restoring
    /// the pre-command state is dropping the overlay and evaluating the document again. For a profile
    /// the gesture did write the ring, so the pull's edit scope is rolled back and the profile goes
    /// with it — a cancelled create leaves no trace (ADR 0138).
    pub(crate) fn cancel_extrude(&mut self) {
        if !self.extrude_gizmo.is_active() {
            return;
        }
        if let Some(scope) = self.extrude_gizmo.scope() {
            // The gesture created the profile it was pulling, so rolling the scope back leaves no
            // node for the selection to name.
            self.session.cancel_edit(scope);
            self.editor.select_node(None);
        }
        self.extrude_gizmo.cancel();
        self.session.clear_preview();
        if let Err(error) = self.refresh() {
            self.last_error = Some(error);
        }
    }

    /// The single frame the grips act on: the selection, when it is exactly one product frame,
    /// which owns the three lengths the grips move (ADR 0101).
    fn grip_target(&self) -> Option<Id> {
        let selected = self.editor.selected_nodes();
        let [only] = selected else {
            return None;
        };
        offers_grips(self.session.project(), *only, self.session.active_product()).then_some(*only)
    }

    /// The scene object under `pointer` in `rect`, timed for the on-screen display.
    ///
    /// Every pick of the whole scene goes through here — a click, a right click and the press that
    /// arms the rubber band — so the block reports what one pick of this scene costs instead of
    /// what one call site happened to measure (U106).
    fn pick_scene(&mut self, rect: egui::Rect, pointer: egui::Pos2) -> Option<Id> {
        let started = Instant::now();
        let hit = mesh_pick::pick_scene(rect, &self.scene, pointer);
        self.osd.record_pick(started.elapsed());
        hit
    }

    /// The shape under `pointer`, timed like [`Document::pick_scene`].
    fn pick_surface(&mut self, rect: egui::Rect, pointer: egui::Pos2) -> Option<Id> {
        let started = Instant::now();
        let hit = mesh_pick::pick_surface(rect, &self.scene, pointer);
        self.osd.record_pick(started.elapsed());
        hit
    }

    /// Whether `pointer` rests on empty space: no scene glyph, no shape and no product envelope.
    ///
    /// A rubber band starts only there, and the two picks it asks are the expensive ones, so this
    /// is the one question the press pays for and the block measures.
    fn over_empty_space(&mut self, rect: egui::Rect, pointer: egui::Pos2, previewed: &[(Id, Placement)]) -> bool {
        let glyph =
            scene_gizmo::pick(self.scene.camera, rect, self.session.project(), pointer, previewed, self.editor.flags());
        glyph.is_none() && self.pick_surface(rect, pointer).is_none()
    }

    /// Reads one frame of the rubber band: the press that arms it, the pointer that draws it and
    /// the release that selects with it (U103).
    ///
    /// `starts` is the caller's verdict on this frame's press — the scene is under it, nothing else
    /// is — so the band never begins on an object, a handle or a tool. The band lives between the
    /// press and the release: a press that never travels is a click, which the viewport handles on
    /// its own, and only a release egui calls the end of a drag selects. Escape is not needed and
    /// does nothing: a band writes only on its release, so there is nothing to take back.
    fn update_band(&mut self, ui: &mut egui::Ui, response: &egui::Response, rect: egui::Rect, starts: bool) {
        let pressed = ui.input(|input| input.pointer.primary_pressed());
        if pressed {
            self.band = match (starts, response.interact_pointer_pos()) {
                (true, Some(pointer)) if rect.is_positive() => Some(ui::SelectionBand::new(pointer, pointer)),
                _ => None,
            };
            return;
        }
        let Some(band) = self.band else {
            return;
        };
        // The pointer keeps drawing the band after it leaves the view: the rectangle is clamped to
        // the view it was drawn in when it is painted, not while it is dragged.
        let pointer = response.interact_pointer_pos().or_else(|| ui.input(|input| input.pointer.latest_pos()));
        let band = match pointer {
            Some(pointer) => band.with_to(pointer),
            None => band,
        };
        self.band = Some(band);
        if ui.input(|input| input.pointer.primary_down()) {
            return;
        }
        self.band = None;
        if !response.drag_stopped_by(egui::PointerButton::Primary) {
            // A click after all: the viewport's own click handling is the whole gesture, so an
            // empty-space click still clears the selection and a press on nothing writes nothing.
            return;
        }
        let pick = Pick::from_modifiers(&ui.input(|input| input.modifiers));
        let view = ui::BandView { scene: &self.scene, camera: self.scene.camera, viewport: rect };
        self.editor.select_band(&self.session, view, band, pick);
        if let Err(error) = self.upload_scene() {
            self.last_error = Some(error);
        }
        ui.ctx().request_repaint();
    }

    /// Reads one frame of the box gesture: the live base, the pulled height, and the click that
    /// anchors it, fixes it or accepts it.
    ///
    /// A click that fixes the base starts the pull; every frame of the pull reads the pointer along
    /// the plane's normal, so the height is a number of millimetres rather than a screen distance.
    /// The last click, or the command bar's ✓, is the one command the gesture writes (ADR 0101).
    fn update_box(&mut self, ui: &mut egui::Ui, response: &egui::Response, rect: egui::Rect) {
        let plane = WorkPlane::from_camera(self.scene.camera);
        let Some(draft) = self.editor.box_draft().copied() else {
            return;
        };
        let pointer = response.hover_pos().or_else(|| response.interact_pointer_pos());
        if draft.is_pulling() {
            // A focused field owns its number: while the maker types the height into the card, the
            // pointer may not overwrite it, and leaving the field hands the height back to the
            // hand (ADR 0119).
            if !ui.ctx().text_edit_focused()
                && let (Some(pointer), Some(base)) = (pointer, draft.base())
                && let Some(height) = box_gizmo::pulled_height(self.scene.camera, rect, plane, base.corner, pointer)
                && let Err(error) = self.editor.pull_box_height(height)
            {
                self.last_error = Some(error.to_string());
            }
        } else if let Some(pointer) = pointer
            && let Some(corner) = box_gizmo::plane_point(self.scene.camera, rect, plane, pointer)
        {
            self.editor.set_box_corner(corner);
        }
        match box_gizmo::update_box(ui, response, rect, self.scene.camera, plane, &draft) {
            Some(BoxEvent::Click(corner)) => {
                if let Err(error) = self.editor.click_box(corner) {
                    self.last_error = Some(error.to_string());
                }
            }
            Some(BoxEvent::Commit) => {
                if let Err(error) = self.finish_box(ui::language(ui.ctx())) {
                    self.last_error = Some(error);
                }
            }
            None => {}
        }
    }

    /// Reads one frame of the box grips and writes the sides they reach.
    ///
    /// One press mints the edit scope and every move writes through it, so a whole drag is one undo
    /// entry; the release ends the gesture and writes nothing more (ADR 0092).
    fn update_box_grips(&mut self, ui: &mut egui::Ui, response: &egui::Response, rect: egui::Rect) {
        let Some(object) = self.grip_target() else {
            return;
        };
        let update = self.box_grips.update(
            ui,
            rect,
            self.scene.camera,
            self.session.project(),
            Some(object),
            response.hover_pos(),
        );
        match update {
            GripUpdate::Press(grip) => {
                let scope = self.session.edit_scope();
                self.box_grips.begin(object, grip, scope);
            }
            GripUpdate::Edit(edit) => {
                let Some(scope) = self.box_grips.scope() else {
                    return;
                };
                match box_gizmo::apply_grip_edit(&mut self.session, edit, scope) {
                    Ok(()) => {
                        if let Err(error) = self.refresh() {
                            self.last_error = Some(error);
                        }
                    }
                    Err(error) => self.last_error = Some(error),
                }
            }
            GripUpdate::Idle => {}
        }
    }

    /// Commits the box the pull shows; idempotent, because the committed gesture leaves no draft.
    ///
    /// `lang` names the frame a drawn product frame becomes, as the card's create does.
    pub(crate) fn finish_box(&mut self, lang: text::Lang) -> Result<(), String> {
        let plane = WorkPlane::from_camera(self.scene.camera);
        box_gizmo::commit_box(&mut self.session, &mut self.editor, plane, lang)?;
        self.refresh()
    }

    /// Drops the box gesture and, with it, a grip drag; the document keeps what its last command
    /// wrote unless the drag had not ended, in which case its edit is rolled back.
    pub(crate) fn cancel_box(&mut self) {
        if let Some(scope) = self.box_grips.scope()
            && self.session.cancel_edit(scope)
            && let Err(error) = self.refresh()
        {
            self.last_error = Some(error);
        }
        self.box_grips.end();
        self.editor.cancel_box();
    }

    /// Measures the void under the pointer and keeps the ghost the next click would create.
    ///
    /// The model answers the measurement; the shell only maps the pointer to the face of the open
    /// product it points at. A pointer that leaves the scene drops the ghost, and a still pointer
    /// re-measures nothing.
    fn update_fill(
        &mut self,
        ui: &mut egui::Ui,
        response: &egui::Response,
        rect: egui::Rect,
        arm: Option<ui::FillArm>,
        card_before: Option<egui::Rect>,
        opacity: u32,
    ) {
        let Some(arm) = arm else {
            self.reset_fill(ui.ctx());
            return;
        };
        let Some(product) = self.session.active_product() else {
            // A closed, deleted or undone product ends the tool: a niche is only offered inside the
            // open piece (ADR 0098), so the arm would have nothing to point at.
            self.editor.cancel_fill();
            self.reset_fill(ui.ctx());
            return;
        };
        let Some(thickness) = self.fill_thickness() else {
            self.reset_fill(ui.ctx());
            return;
        };
        // A press or a hover on the tool card is chrome, so the viewport's own response calls it a
        // miss; the card still belongs to this gesture, so the raw pointer measures through it.
        let on_field = |pointer: egui::Pos2| card_before.is_some_and(|card| card.contains(pointer));
        let pointer = response
            .hover_pos()
            .or_else(|| response.interact_pointer_pos())
            .or_else(|| ui.input(|input| input.pointer.hover_pos()).filter(|pointer| on_field(*pointer)));
        let card_anchor = self.card_anchor(pointer, card_before);
        let point = pointer.and_then(|pointer| {
            void_gizmo::point_at(self.session.project(), product, self.scene.camera, rect, pointer)
        });
        let Some(point) = point.or_else(|| {
            // The card sits beside the pointer now and then over the envelope's silhouette; the
            // gesture keeps the point it was measured at rather than dropping the ghost under the
            // maker's cursor.
            ui.input(|input| input.pointer.hover_pos())
                .filter(|pointer| on_field(*pointer))
                .and_then(|_| self.editor.void().point())
        }) else {
            self.reset_fill(ui.ctx());
            return;
        };
        // The fill is the niche atom's first consumer: one measurement serves the ghost, the report
        // row and the part the click creates, so the pointer names one space in this frame.
        let measured = self.editor.void_measure(self.session.project(), point);
        let niche = self.editor.void().niche().cloned();
        let planned = match niche.as_ref() {
            Some(niche) => {
                // A number typed into a field pins the free position; the pointer drives it otherwise.
                let free = self.distances.as_ref().and_then(Distances::pinned_low);
                self.fill_gizmo.hover(
                    self.session.project(),
                    product,
                    point,
                    niche,
                    arm.fill,
                    arm.mode,
                    thickness,
                    free,
                )
            }
            None => {
                let reason =
                    self.editor.void().refusal().map(|refusal| refusal.words().text(ui::language(ui.ctx())).to_owned());
                self.fill_gizmo.refuse(reason)
            }
        };
        if measured || planned {
            ui.ctx().request_repaint();
        }
        let Some(ghost) = self.fill_gizmo.ghost().cloned() else {
            self.distances = None;
            self.tool_card_rect = None;
            return;
        };
        let Some(parent_world) = self.session.project().world_placement(product) else {
            self.distances = None;
            self.tool_card_rect = None;
            return;
        };
        let axis = model::free_axis(arm.fill);
        let unit = axis_unit(axis);
        let low_wall = ghost.niche.local_min[axis];
        let high_wall = ghost.niche.local_max()[axis];
        let low_face = ghost.free_low;
        let high_face = ghost.free_high;
        let mut distances = match self.distances.take() {
            Some(mut measured) => {
                measured.sync(Some(low_wall), Some(high_wall), low_face, high_face);
                measured
            }
            None => {
                let Some(measured) = Distances::new(Some(low_wall), Some(high_wall), low_face, high_face) else {
                    self.tool_card_rect = None;
                    return;
                };
                measured
            }
        };
        let (Some(low_point), Some(high_point)) = (ghost.low_face(), ghost.high_face()) else {
            self.distances = Some(distances);
            self.tool_card_rect = None;
            return;
        };
        let axis_world = parent_world.transform_vector(unit);
        let mut placements = Vec::new();
        let painter = ui.painter().with_clip_rect(rect);
        for (side, face, face_scalar) in [(Side::Low, low_point, low_face), (Side::High, high_point, high_face)] {
            let wall = if side == Side::Low { low_wall } else { high_wall };
            let wall_point = face + axis_world * (wall - face_scalar);
            let middle = (face + wall_point) * 0.5;
            let (Some(face_screen), Some(wall_screen), Some(middle_screen)) = (
                gizmo_math::screen_point(self.scene.camera, rect, face),
                gizmo_math::screen_point(self.scene.camera, rect, wall_point),
                gizmo_math::screen_point(self.scene.camera, rect, middle),
            ) else {
                continue;
            };
            painter.line_segment([face_screen, wall_screen], egui::Stroke::new(1.0, ui::theme::text_dim(ui)));
            placements.push(FieldPlacement { side, position: middle_screen });
        }
        let outcome: DistanceOutcome =
            // A fill holds the pointer with its own gesture, so its plates are a deliberate target.
            self.distance_fields.show(ui, &mut distances, &placements, rect, card_anchor, arm.class.label(), opacity, true);
        self.tool_card_rect = outcome.rect;
        if let Some(message) = self.distance_fields.error() {
            self.last_error = Some(message.text(ui::language(ui.ctx())).to_owned());
        }
        let pinned = distances.pinned_low();
        if outcome.changed {
            // The typed number or the midpoint moved the ghost: plan it again at that exact
            // coordinate before the frame paints, so the picture and the field agree. The cell is the
            // one the atom holds, so the typed number moves the part and not the space.
            if let Some(niche) = self.editor.void().niche().cloned() {
                self.fill_gizmo.hover(
                    self.session.project(),
                    product,
                    point,
                    &niche,
                    arm.fill,
                    arm.mode,
                    thickness,
                    pinned,
                );
            }
            ui.ctx().request_repaint();
        }
        self.distances = Some(distances);
    }

    /// Drops the empty-space ghost and the exact-distance card that measured it.
    fn reset_fill(&mut self, ctx: &egui::Context) {
        self.fill_gizmo.reset();
        self.distance_fields.dismiss(ctx);
        self.distances = None;
        self.tool_card_rect = None;
    }

    /// Arms the ruler, or leaves it when it already owns the pointer.
    ///
    /// The key and the ribbon cell land here, so both toggle the tool the same way; the editor
    /// clears every other pointer mode and this clears the shell state that belonged to them.
    pub(crate) fn toggle_measure(&mut self, ctx: &egui::Context) {
        if self.editor.measure().is_some() {
            self.cancel_measure();
        } else {
            self.start_measure(ctx);
        }
    }

    /// Arms the ruler and leaves the shell state of the tool it replaced.
    ///
    /// The measurement is view state: nothing reaches the document, the undo stack or the session
    /// preview. A running draft, a fill ghost and an untyped distance field are all left behind,
    /// because two armed modes at once is a defect.
    pub(crate) fn start_measure(&mut self, ctx: &egui::Context) {
        self.editor.arm_measure();
        self.draw_gizmo.reset();
        self.cancel_extrude();
        self.reset_fill(ctx);
        self.measure_gizmo.reset();
    }

    /// Leaves the ruler and drops its points; nothing was written, so undo is untouched.
    pub(crate) fn cancel_measure(&mut self) {
        self.editor.cancel_measure();
        self.measure_gizmo.reset();
    }

    /// Arms the niche tool, or leaves it when it already owns the pointer.
    ///
    /// The key, the ribbon cell and the context menu entry land here, so every entry point toggles the
    /// tool the same way; the editor clears every other pointer mode, and this clears the shell state
    /// those modes held.
    pub(crate) fn toggle_void(&mut self, ctx: &egui::Context) {
        if self.editor.void_armed() {
            self.cancel_void();
        } else if self.session.active_product().is_none() {
            // A niche belongs to the open product (ADR 0098): the key says what is missing instead
            // of arming a tool with nothing to point into.
            self.last_error = Some(Message::FillNeedsProduct.text(ui::language(ctx)).to_owned());
        } else {
            self.start_void(ctx);
        }
    }

    /// Arms the niche tool and leaves the shell state of the tool it replaced.
    ///
    /// The niche is a measurement: nothing reaches the document, the undo stack or the session
    /// preview, and a running draft, a fill ghost and an untyped distance field are left behind
    /// because two armed modes at once is a defect (ADR 0118).
    pub(crate) fn start_void(&mut self, ctx: &egui::Context) {
        self.editor.arm_void();
        self.draw_gizmo.reset();
        self.cancel_extrude();
        self.reset_fill(ctx);
        self.measure_gizmo.reset();
    }

    /// Leaves the niche tool and drops the cell it named; nothing was written, so undo is untouched.
    pub(crate) fn cancel_void(&mut self) {
        self.editor.cancel_void();
        self.void_gizmo.reset();
    }

    /// Feeds the pointer to the armed niche and keeps the reading in the editor's one atom.
    ///
    /// A pointer outside the open product, or a pointer on a widget, leaves the value as it stands:
    /// the tool keeps the cell under the maker's hand instead of dropping it on chrome. The niche
    /// writes nothing, so this is a read of the document, not a command.
    fn update_void(
        &mut self,
        ui: &mut egui::Ui,
        response: &egui::Response,
        rect: egui::Rect,
        pointer_over_widget: bool,
    ) {
        let changed = self.void_gizmo.update(
            response,
            rect,
            self.scene.camera,
            self.session.project(),
            &mut self.editor,
            self.session.active_product(),
            pointer_over_widget,
        );
        if changed {
            ui.ctx().request_repaint();
        }
    }

    /// Whether this frame's press landed on the tool card rather than on the scene.
    fn distance_covers(&self, response: &egui::Response) -> bool {
        response
            .interact_pointer_pos()
            .is_some_and(|pointer| self.tool_card_rect.is_some_and(|card| card.contains(pointer)))
    }

    /// The world distance a typed move distance has fixed, relative to the drag's start.
    fn move_pinned(&self) -> Option<f64> {
        let measure = self.move_distances.as_ref()?;
        let low = measure.distances.pinned_low()?;
        Some(low - measure.start_low)
    }

    /// Measures the moving box and the two surfaces along the drag axis, once per drag.
    fn measure_move(&self, node: Id, axis: gizmo_math::Axis) -> Option<MoveDistances> {
        measure_move(self.session.project(), node, axis)
    }

    /// Draws the exact-distance fields of the translate drag and reads their numbers.
    fn show_move_distances(&mut self, ui: &mut egui::Ui, rect: egui::Rect, opacity: u32) {
        let (Some(axis), Some(node)) = (self.translate_gizmo.axis(), self.translate_gizmo.anchor()) else {
            self.clear_move_distances(ui.ctx());
            return;
        };
        if !self.translate_gizmo.exact_axis() {
            self.clear_move_distances(ui.ctx());
            return;
        }
        // The carry's own hand is read before the measurement is borrowed: the card hangs from it.
        let hand = self.carry_hand();
        if self.move_distances.as_ref().is_none_or(|measured| measured.node != node || measured.axis != axis) {
            self.distance_fields.dismiss(ui.ctx());
            self.move_distances = self.measure_move(node, axis);
        }
        let Some(measure) = self.move_distances.as_mut() else {
            self.tool_card_rect = None;
            return;
        };
        let direction = axis.direction();
        let delta = self.translate_gizmo.drag_delta().dot(direction);
        let low = measure.start_low + delta;
        let high = low + measure.extent;
        let capture = self.translate_gizmo.capture();
        let (wall_low, wall_high) = captured_walls(measure, capture);
        measure.distances.sync(wall_low, wall_high, low, high);
        let low_face = measure.low_face + direction * delta;
        let high_face = measure.high_face + direction * delta;
        // The snap moved the piece on its own, so it draws the surface the face landed on.
        if let Some(capture) = capture {
            let centre = match capture.end {
                model::SnapEnd::Low => low_face,
                model::SnapEnd::High => high_face,
            };
            paint_capture(
                &ui.painter().with_clip_rect(rect),
                self.scene.camera,
                rect,
                measure,
                centre,
                axis,
                ui::theme::helper(ui),
            );
        }
        // The walls stay where the start measurement found them; only the faces travel.
        let low_wall_point = measure.wall_low.map(|wall| measure.low_face + direction * (wall - measure.start_low));
        let high_wall_point =
            measure.wall_high.map(|wall| measure.high_face + direction * (wall - (measure.start_low + measure.extent)));
        let painter = ui.painter().with_clip_rect(rect);
        let mut placements = Vec::new();
        for (side, face, wall_point) in
            [(Side::Low, low_face, low_wall_point), (Side::High, high_face, high_wall_point)]
        {
            let middle = wall_point.map_or(face, |wall_point| {
                let (Some(face_screen), Some(wall_screen)) = (
                    gizmo_math::screen_point(self.scene.camera, rect, face),
                    gizmo_math::screen_point(self.scene.camera, rect, wall_point),
                ) else {
                    return face;
                };
                painter.line_segment([face_screen, wall_screen], egui::Stroke::new(1.0, ui::theme::text_dim(ui)));
                (face + wall_point) * 0.5
            });
            let Some(middle_screen) = gizmo_math::screen_point(self.scene.camera, rect, middle) else {
                continue;
            };
            let position = egui::Pos2::new(
                middle_screen.x.clamp(rect.left() + 90.0, (rect.right() - 90.0).max(rect.left() + 90.0)),
                middle_screen.y.clamp(rect.top() + 26.0, (rect.bottom() - 26.0).max(rect.top() + 26.0)),
            );
            placements.push(FieldPlacement { side, position });
        }
        // The card hangs from the carry's own hand while it is away from the system pointer: after a
        // camera gesture the hand is on the piece, and a card left at the pointer would sit across
        // the room from the gaps it names (X69).
        let anchor = hand.or_else(|| ui.input(|input| input.pointer.hover_pos())).unwrap_or_else(|| rect.center());
        // The furniture move carries a piece with no button held, so its card keeps the keyboard and
        // never the pointer: a press over it is the drop, not a scrub (X66). Every other gesture
        // reaches this function too, so the flag is the hold's own.
        let sense_pointer = !self.translate_gizmo.is_following();
        let outcome = self.distance_fields.show(
            ui,
            &mut measure.distances,
            &placements,
            rect,
            anchor,
            None,
            opacity,
            sense_pointer,
        );
        self.tool_card_rect = outcome.rect;
        if let Some(message) = self.distance_fields.error() {
            self.last_error = Some(message.text(ui::language(ui.ctx())).to_owned());
        }
        if outcome.changed {
            ui.ctx().request_repaint();
        }
    }

    /// Drops the translate drag's measurement and its fields.
    fn clear_move_distances(&mut self, ctx: &egui::Context) {
        if self.move_distances.take().is_some() {
            self.distance_fields.dismiss(ctx);
            self.tool_card_rect = None;
        }
    }

    /// The thickness a filled part takes: the catalogue's first panel material.
    fn fill_thickness(&self) -> Option<Length> {
        self.session.catalog().panel_materials().first().map(|material| material.nominal_thickness())
    }

    /// Creates the ghost's part through one `panel.create`, class included.
    ///
    /// The parameters are the ghost's own size and placement, so the part the maker sees is exactly
    /// the part the history records, and the command is the only write of the gesture. The tool
    /// stays armed, because a piece usually needs several posts.
    fn create_fill(&mut self, lang: ui::Lang) -> Result<(), String> {
        let arm = self.editor.fill_arm().ok_or_else(|| "The empty-space tool is not armed.".to_owned())?;
        let ghost = self
            .fill_gizmo
            .ghost()
            .cloned()
            .ok_or_else(|| "Point at the empty space inside the open product.".to_owned())?;
        let name = arm.class.label().map_or_else(|| Message::ToolPanel.text(lang), |label| label.text(lang)).to_owned();
        self.editor
            .create_panel_sized(
                &mut self.session,
                ghost.parent,
                name,
                ghost.nominal_size,
                Some(ghost.outline.clone()),
                ghost.bevels.clone(),
                ghost.placement,
                Some(arm.class),
            )
            .map(|_| ())
    }

    /// Draws the armed tool's card at the pointer and applies what it asked for (ADR 0119).
    ///
    /// A box's height and a pull's height are the two tools whose parameters sit at the hand: the
    /// card's field drives the preview live, its ✓ commits it with a single command and its ✕ drops
    /// it, and a number that leaves the field's range is refused by the field itself, so the preview
    /// keeps the last height the document could hold. A niche's two gaps draw the same card from
    /// `update_fill`, and the inspector draws the same rows from the same draft.
    fn show_tool_card(
        &mut self,
        ui: &mut egui::Ui,
        viewport: egui::Rect,
        pointer: Option<egui::Pos2>,
        card_before: Option<egui::Rect>,
        opacity: u32,
    ) {
        let Some(mut view) = self.height_view() else {
            // Another mode owns the card this frame — a niche's two gaps were drawn by `update_fill`
            // — and the rectangle is not this path's to clear.
            return;
        };
        let anchor = self.card_anchor(pointer, card_before);
        // A pull that just appeared takes the keyboard, so its height can be typed without a click;
        // the flag is cleared by the read so the field never holds the focus against the next press.
        let focus = self.tool_focus.take().or_else(|| self.extrude_gizmo.take_focus().then_some(ToolParam::Height));
        let (reported, rect) = tool_card::show(ui, viewport, anchor, &mut view, focus, opacity, true);
        // A press on the card is chrome, exactly like a press on a panel: the card floats over the
        // scene, so its rectangle is handed to the viewport's own hit test next frame.
        self.tool_card_rect = Some(rect);
        self.apply_tool_outcome(ui.ctx(), reported);
    }

    /// The point the card hangs from: the pointer, or the last scene point while the hand is on the
    /// card.
    ///
    /// A card that followed the pointer would slide out from under the hand reaching for its field,
    /// so the anchor is frozen across the whole time the pointer is inside the card's rectangle. It
    /// is the same rule `egui` applies to a tooltip and the one the context menu relies on: a
    /// floating surface must not move while it is being used.
    fn card_anchor(&mut self, pointer: Option<egui::Pos2>, card_before: Option<egui::Rect>) -> egui::Pos2 {
        let pointer = pointer.or_else(|| self.scene_rect.map(|rect| rect.center())).unwrap_or_default();
        if card_before.is_some_and(|card| card.contains(pointer))
            && let Some(anchor) = self.tool_card_anchor
        {
            return anchor;
        }
        self.tool_card_anchor = Some(pointer);
        pointer
    }

    /// The view of the tool whose parameters are dragged at the hand, if one is running.
    ///
    /// The frame's reserved height and the pull's height are the two tools this surface draws; a
    /// niche's gaps are drawn by `update_fill` from the same [`Distances`], and every tool reaches
    /// the inspector through [`Self::tool_view`].
    fn height_view(&self) -> Option<ToolView> {
        if let Some(draft) = self.editor.box_draft().copied().filter(BoxDraft::is_pulling) {
            let maximum = box_gizmo::MAX_SIDE_MM as f64;
            // A frame's pull is the height it reserves, not a solid's edge (ADR 0101).
            let range = box_gizmo::MIN_SIDE_MM as f64..=maximum;
            return Some(ToolView::height(Message::FrameHeight, draft.height().millimetres_f64(), range, 3));
        }
        self.extrude_gizmo.object()?;
        let height = self.extrude_gizmo.height()?;
        let top = self.extrude_gizmo.top();
        let maximum = (model::MAX_CONTOUR_COORDINATE.micrometres() / 1_000) as f64;
        let range = crate::extrude_gizmo::MIN_HEIGHT_MM..=maximum;
        // A pull's card is a number, not a dialog: `H` names it, no unit repeats it and the gesture's
        // own Enter and Escape commit and cancel, so the field is the only control it draws (U104).
        Some(ToolView::pull(height.millimetres_f64(), top.map(Length::millimetres_f64), range, 3))
    }

    /// Every armed tool's parameters, as the inspector mirrors them (ADR 0119).
    ///
    /// The same rows the card draws, built from the same draft: a box's height, a pull's height, or
    /// the two gaps of a niche. Two surfaces read one draft, so they cannot show two numbers for one
    /// face.
    pub(crate) fn tool_view(&self) -> Option<ToolView> {
        if let Some(height) = self.height_view() {
            return Some(height);
        }
        let arm = self.editor.fill_arm()?;
        let distances = self.distances.as_ref()?;
        Some(ToolView { title: arm.class.label(), rows: distances.rows(), commit: ui::ToolCommit::Gesture })
    }

    /// Takes what a tool's fields committed into the document's armed state.
    ///
    /// The card at the pointer and the inspector's section both end here: a height re-pulls the box
    /// or re-previews the body, and a gap goes through the distance card's own `apply`, so one number
    /// has one refusal whichever surface the maker typed it into. The ✓ and the ✕ are the shell's
    /// own `finish_*`/`cancel_*` paths, so the commit is one command either way.
    fn apply_tool_outcome(&mut self, ctx: &egui::Context, reported: ToolOutcome) {
        let mut repull = false;
        for edit in &reported.edits {
            let Some(next) = Length::from_millimetres(edit.value.round() as i64) else {
                continue;
            };
            match edit.param {
                ToolParam::Height => {
                    if self.editor.box_draft().is_some_and(BoxDraft::is_pulling) {
                        if let Err(error) = self.editor.pull_box_height(next) {
                            self.last_error = Some(error.to_string());
                        }
                    } else if self.extrude_gizmo.object().is_some() {
                        self.extrude_gizmo.set_height(next);
                        repull = true;
                    }
                }
                // A cone's taper is the pull's second number: the same card, the same preview, so a
                // typed radius shows the solid without a second gesture (ADR 0141).
                ToolParam::TopRadius => {
                    self.extrude_gizmo.set_top(Some(next));
                    repull = true;
                }
                _ => continue,
            }
            ctx.request_repaint();
        }
        if repull
            && let (Some(object), Some(height), top) =
                (self.extrude_gizmo.object(), self.extrude_gizmo.height(), self.extrude_gizmo.top())
            && let Err(error) = self.preview_extrude(object, height, top)
        {
            self.last_error = Some(error);
        }
        if (reported.edits.iter().any(|edit| edit.param != ToolParam::Height)
            || reported.lock.is_some()
            || reported.centre.is_some())
            && let Some(distances) = self.distances.as_mut()
        {
            if self.distance_fields.apply(distances, &reported) {
                ctx.request_repaint();
            }
            if let Some(message) = self.distance_fields.error() {
                self.last_error = Some(message.text(ui::language(ctx)).to_owned());
            }
        }
        match reported.action {
            Some(ToolAction::Apply) => {
                if self.editor.box_draft().is_some_and(BoxDraft::is_pulling) {
                    if let Err(error) = self.finish_box(ui::language(ctx)) {
                        self.last_error = Some(error);
                    }
                } else if let (Some(object), Some(height), top) =
                    (self.extrude_gizmo.object(), self.extrude_gizmo.height(), self.extrude_gizmo.top())
                    && let Err(error) = self.finish_extrude(object, height, top, ui::language(ctx))
                {
                    self.last_error = Some(error);
                }
            }
            Some(ToolAction::Cancel) => {
                if self.editor.box_draft().is_some() {
                    self.cancel_box();
                } else {
                    self.cancel_extrude();
                }
            }
            None => {}
        }
    }

    /// Dispatches one `node.placement.set_many` for a finished drag and rebuilds the scene.
    fn apply_gizmo_commit(&mut self, commit: PlacementCommit) -> Result<(), String> {
        // Defence in depth: the viewport refuses a locked selection before the drag starts, and the
        // commit refuses it again, so no other path can move a locked node either.
        if let Some(name) = locked_change(self.session.project(), &commit, &self.editor) {
            return Err(format!("{name} is locked. Unlock it in the scene tree to move it."));
        }
        let request = NodePlacementSetMany {
            placements: commit
                .changes
                .iter()
                .map(|change| NodePlacement { node: change.node, placement: change.placement })
                .collect(),
        };
        let params =
            serde_json::to_value(request).map_err(|error| format!("could not encode the placement batch: {error}"))?;
        self.session.dispatch(NODE_PLACEMENT_SET_MANY_ID, &params).map_err(|error| error.to_string())?;
        // The preview already showed the new position; rebuilding here keeps the document, the
        // scene and the inspector in agreement without waiting a frame.
        self.refresh()
    }

    /// Dispatches one `mesh.vertices.move` for a finished vertex drag and rebuilds the scene.
    fn apply_mesh_commit(&mut self, commit: MeshVertexCommit) -> Result<(), String> {
        let request = MeshVerticesMove {
            object: commit.object,
            moves: commit
                .moves
                .iter()
                .map(|(vertex, position)| MeshVertexMove { vertex: *vertex, position: *position })
                .collect(),
        };
        let params =
            serde_json::to_value(request).map_err(|error| format!("could not encode the vertex batch: {error}"))?;
        self.session.dispatch(MESH_VERTICES_MOVE_ID, &params).map_err(|error| error.to_string())?;
        self.refresh()
    }

    /// Fits the active view on the scene's current bounds.
    ///
    /// The fit camera is computed from the latest evaluation on every press, so *Frame all* follows
    /// objects created or moved since the file opened. Only the active view moves: its direction
    /// and its projection kind stay as the user left them. The fit targets the part of the view the
    /// scene rectangle covers — everything below the floating ribbon — so the scene is never framed
    /// under the strip (U51). The side panels float over that rectangle without narrowing it, so a
    /// fit is unchanged by folding or hiding one (U72).
    pub(crate) fn frame_all(&mut self) {
        let fit = match home_camera(self.scene_bounds) {
            Ok(fit) => fit,
            Err(error) => {
                self.last_error = Some(error);
                return;
            }
        };
        // The view rectangle already starts below the ribbon band and the panels never narrow it,
        // so the fit needs no further narrowing: the whole scene lands below the strip (U72).
        self.layout.active_leaf_mut().frame(fit);
        self.scene.camera = crate::scene_build::scene_camera(self.session.project(), self.layout.active_camera());
    }

    /// Returns the active view to *Axonometric* without refitting the scene.
    ///
    /// The preset writes yaw and pitch only, so the model does not move and the view keeps its
    /// target, its distance and its projection (ADR 0049).
    pub(crate) fn home_view(&mut self) {
        self.layout.active_leaf_mut().home();
        self.scene.camera = crate::scene_build::scene_camera(self.session.project(), self.layout.active_camera());
    }

    /// Zooms one view about the point under the pointer, which keeps that point where it is.
    ///
    /// The wheel and every drag that zooms go through here: a zoom about the view's centre slides
    /// everything the maker is looking at out from under the pointer, and a piece carried by that
    /// pointer left the hand on every step (X67).
    fn zoom_about_pointer(&mut self, leaf: LeafId, viewport: egui::Rect, pointer: egui::Pos2, factor: f64) {
        let Some(leaf) = self.layout.leaf_mut(leaf) else {
            return;
        };
        let anchor = gizmo_math::view_plane_point(pointer, viewport, leaf.camera());
        leaf.zoom_about(factor, anchor);
    }

    /// Turns the active view to a standard preset, the way the corner widget and the view menu do.
    ///
    /// The preset writes the camera's direction and its projection, so this is view state and no
    /// command (ADR 0049); the shell's camera is refreshed here so one entry point cannot leave the
    /// two disagreeing.
    pub(crate) fn apply_view_preset(&mut self, preset: ui::display::OpeningView) {
        self.layout.active_leaf_mut().apply_preset(preset);
        let camera = self.layout.active_camera();
        self.scene.camera = crate::scene_build::scene_camera(self.session.project(), camera);
    }

    /// Switches the active view's projection and refreshes the camera the viewport draws with.
    ///
    /// The ribbon, the Display popover and the corner widget all end here, so the three entry points
    /// cannot disagree. The projection is view state, so this is not a command and leaves no undo
    /// entry (ADR 0049).
    pub(crate) fn toggle_view_projection(&mut self) {
        self.layout.active_leaf_mut().toggle_projection();
        let camera = self.layout.active_camera();
        self.scene.camera = crate::scene_build::scene_camera(self.session.project(), camera);
    }

    pub(crate) fn viewport(
        &mut self,
        ui: &mut egui::Ui,
        shell: &mut Shell,
        settings: &Settings,
        chrome: &Chrome,
        opacity: u32,
    ) {
        // The transform gizmos ask this workspace for their size once per frame, so the drawing and
        // the hit test of a drag read one number and a settings change costs no rebuild (U86).
        let handle_length = settings.gizmo.handle_length();
        let ring_factor = settings.gizmo.ring_factor();
        // The toolbar can be wider than the panel in a narrow window; egui then reports the
        // overflowing width as available. Intersecting with the clip rect keeps the viewport and
        // its orientation widget inside the central panel.
        let area = ui.available_rect_before_wrap().intersect(ui.clip_rect());
        if area.width() < 1.0 || area.height() < 1.0 {
            return;
        }
        let (_, response) = ui.allocate_exact_size(area.size(), egui::Sense::click_and_drag());
        if !ui.is_rect_visible(area) {
            return;
        }
        // One frame of the window is one line of the display: the rate is the interval egui
        // measured, and while the block is on the shell asks for the next frame itself, because a
        // rate read off a window that repaints only on input is the idle rate (U106).
        self.osd.tick(ui.input(|input| input.stable_dt));
        if shell.cluster.osd_visible() {
            ui.ctx().request_repaint();
        }
        // The render target covers the whole central area, including the bands under the floating
        // ribbon and the translucent panels, so the scene image runs under the chrome (M2). The
        // split views tile the scene rectangle, which depends on the ribbon band only: the panels
        // float over it and folding one moves no view and no camera (U72). A pointer on the chrome
        // is never a viewport hit.
        shell.pointer_position = ui.input(|input| input.pointer.hover_pos());
        let pointer = shell.pointer_position;
        // A press outside the open context menu dismisses it and picks nothing, the way a question
        // answers itself with no second click: the menu owns the press that closes it (U92). A
        // press inside is chrome through `Chrome::covers`, so the entry under the pointer is what
        // the press is for and the model behind it keeps its revision.
        let dismiss_press = [self.context_menu.rect(), self.draft_menu.rect()].into_iter().flatten().any(|menu| {
            ui.input(|input| input.pointer.primary_pressed()) && pointer.is_some_and(|pointer| !menu.contains(pointer))
        });
        if dismiss_press {
            self.context_menu.close();
            self.draft_menu.close();
        }
        // A press on the tool card is chrome too: the card floats over the scene, so a click on its
        // field or its buttons is never a pick. The rectangle is the one the card drew last frame,
        // which is the one on screen while the card stays open. The plates of the point being placed
        // are the same kind of surface: a press on a number is the number's, never a point (ADR 0126).
        let card_before = self.tool_card_rect;
        let pointer_on_chrome = dismiss_press
            || pointer.is_some_and(|pointer| {
                chrome.covers(pointer)
                    || card_before.is_some_and(|card| card.contains(pointer))
                    || self.draw_gizmo.field_contains(pointer)
                    || self.contour_gizmo.field_contains(pointer)
                    || self.draft_menu.rect().is_some_and(|menu| menu.contains(pointer))
            });
        let content = chrome.content(area);
        self.scene_rect = Some(content);
        // The frame's card is redrawn below, in whichever mode owns the gesture; a frame that draws
        // no card leaves the rectangle empty, so `Chrome` never keeps a stale one.
        self.tool_card_rect = None;

        // The viewport is the whole central area; the layout divides the scene rectangle into
        // views. Every view has its own camera and every border is draggable. A press on a border
        // resizes without moving the target; a press anywhere else activates that view, whichever
        // button it is, and only the active view is ever manipulated (ADR 0079).
        let regions = self.layout.regions(content);
        let pressed = ui.input(|input| input.pointer.any_pressed());
        if pressed
            && !pointer_on_chrome
            && let Some(pointer) = response.interact_pointer_pos()
        {
            let primary = ui.input(|input| input.pointer.primary_pressed());
            if self.layout.press(content, pointer, primary) {
                // A border drag owns the primary button until it is released; nothing else in the
                // workspace reads the pointer meanwhile, so a resize cannot select an object or
                // move an axis under the user's hand.
                shell.divider_dragging = true;
            }
        }
        // A view border is the one thing inside the window that keeps an icon of its own: the
        // resize arrows say the edge can be dragged, so the shell leaves them and paints no square
        // (X40). Every tool gesture keeps the square, and the frame's cursor starts at `Default`.
        if shell.divider_dragging {
            let delta = response.drag_delta();
            self.layout.drag_by(content, delta);
            if !ui.input(|input| input.pointer.primary_down()) {
                self.layout.end_drag();
                shell.divider_dragging = false;
            }
            if let Some(axis) = self.layout.dragging_axis() {
                ui.ctx().set_cursor_icon(axis.cursor());
            }
            ui.ctx().request_repaint();
        } else if !pointer_on_chrome
            && let Some(pointer) = pointer
            && let Some(axis) = self.layout.border_at(content, pointer)
        {
            ui.ctx().set_cursor_icon(axis.cursor());
        }

        // The active view is the only target: orbit, pan, zoom, the presets, selection, the gizmos
        // and the cluster all act on it. A press is how the user points them at a neighbour.
        let target_leaf = self.layout.active();
        let target_rect =
            regions.iter().find(|(leaf, _)| *leaf == target_leaf).map(|(_, rect)| *rect).unwrap_or(content);
        let target_camera = self.layout.leaf(target_leaf).map(|leaf| leaf.camera()).unwrap_or_default();
        let pointer_over_active =
            pointer.is_some_and(|pointer| !chrome.covers(pointer) && target_rect.contains(pointer));
        // The scene's camera is the one the active view draws with; an authored camera wins over
        // both, exactly as it does for a single view (ADR 0049).
        self.scene.camera = crate::scene_build::scene_camera(self.session.project(), target_camera);
        let rect = target_rect;
        // The orientation widget belongs to the free corner of the view: below the ribbon band and
        // clear of both floating panels, mirrored to the left when the inspector leaves no room
        // (U73). The display-settings popover can turn it off, and a hidden widget draws nothing and
        // takes no click.
        let gizmo_rect = widget_placement(&shell.cluster, rect, chrome);
        // The shell's cursor keeps the corner widget on the system arrow: it is drawn on the scene
        // layer and is a widget, not the work area (X41).
        shell.orientation_rect = gizmo_rect.filter(|_| shell.cluster.widget_visible());

        let points_per_pixel = ui.ctx().pixels_per_point();
        let width = (area.width() * points_per_pixel).round().max(1.0) as u32;
        let height = (area.height() * points_per_pixel).round().max(1.0) as u32;
        if let Some(target) = self.target.as_mut() {
            target.resize(width, height);
        }

        let pointer_over_widget = !pointer_on_chrome
            && shell.cluster.widget_visible()
            && response.hover_pos().is_some_and(|pointer| gizmo_rect.is_some_and(|widget| widget.contains(pointer)));
        // A border drag owns the pointer: the tools, the gizmos and picking all stay out of its way
        // until it is released, so a resize can never select an object or move an axis.
        //
        // A pointer on the chrome owns the pointer only while no gesture is live: a panel or the
        // ribbon band is not the scene, so a press there never starts a gizmo, a draft, a selection
        // rectangle or a navigation drag. A gesture that already started keeps the pointer even if
        // the user drags it over a panel, so releasing there still commits the move — a carried
        // piece included, because its own distance fields are chrome around the hand (N24, N25).
        let capturing = self.translate_gizmo.is_dragging()
            || self.rotate_gizmo.is_dragging()
            || self.box_grips.is_dragging()
            || self.vertex_gizmo.is_dragging()
            || self.contour_gizmo.is_dragging()
            || self.extrude_gizmo.is_dragging()
            || self.gizmo.is_dragging();
        let interactive = !shell.divider_dragging && (!pointer_on_chrome || capturing);
        // A contour edit owns the pointer: the object gizmo and picking would fight the corner drag.
        let contour_editing = interactive && self.editor.contour_edit().is_some();
        // A drawing tool owns it too: the left button places points, not selections. A click on the
        // chrome is never a point, so the tool waits for a click on the scene.
        let drawing = interactive && !pointer_on_chrome && self.editor.drawing().is_some();
        let boxing = interactive && !pointer_on_chrome && self.editor.box_draft().is_some();
        // The pull arrow is the live gesture after an extrusion. A contour edit, a draft, a
        // placement or a sub-object mode takes the pointer from it, so the arrow never competes
        // with a grip, a point or a cube for the same click.
        if self.extrude_gizmo.is_active()
            && (contour_editing
                || drawing
                || boxing
                || self.editor.mesh_edit().is_some()
                || self.editor.fill_arm().is_some()
                || self.editor.void_armed())
        {
            self.cancel_extrude();
        }
        let pulling = interactive && self.extrude_gizmo.is_active();
        // The empty-space tool owns the pointer while it is armed: the next click creates the part
        // the ghost shows, not a selection. It yields to a live contour or sub-object edit.
        let fill_arm = if interactive && self.editor.contour_edit().is_none() && self.editor.mesh_edit().is_none() {
            self.editor.fill_arm()
        } else {
            None
        };
        let filling = fill_arm.is_some() && !pointer_on_chrome;
        // The ruler owns the next click without creating anything: two clicks set a length and the
        // document is never touched (ADR 0118). It yields to no other mode, because arming it left
        // them all.
        let measuring = interactive && !pointer_on_chrome && self.editor.measure().is_some();
        // The niche names the empty space under the pointer and creates nothing; it owns the press
        // the same way the ruler does, so a click never selects through it (ADR 0118).
        let voiding = interactive && !pointer_on_chrome && self.editor.void_armed();
        // An armed navigation gesture owns a primary-button drag, but not the other tools: while a
        // contour edit, a draft or a sub-object mode is live, that mode keeps the pointer.
        let navigation = shell.cluster.navigation;
        let navigating = navigation.is_some()
            && !contour_editing
            && !drawing
            && !boxing
            && !pulling
            && !filling
            && !measuring
            && !voiding;
        let mesh_object = if !interactive || contour_editing || drawing || pulling || filling || measuring || voiding {
            None
        } else {
            self.editor.mesh_edit().map(|edit| edit.object())
        };
        let navigating = navigating && mesh_object.is_none();
        // Face mode moves the union of the selected faces, so both sub-object modes feed the same
        // vertex gizmo and dispatch the same vertex command.
        let mesh_vertices = mesh_object.map(|object| self.mesh_selection_vertices(object)).unwrap_or_default();
        // Vertex editing owns the pointer: the object gizmo would move the whole mesh under the
        // user's hand, so it is neither updated nor painted until the mode ends.
        let vertex_update = if let Some(object) = mesh_object {
            let target = vertex_gizmo::MeshTarget {
                camera: self.scene.camera,
                project: self.session.project(),
                object,
                vertices: &mesh_vertices,
            };
            self.vertex_gizmo.update(ui, &response, rect, target, pointer_over_widget, handle_length)
        } else {
            self.vertex_gizmo.reset();
            Ok(None)
        };
        match vertex_update {
            Ok(Some(commit)) => {
                if let Err(error) = self.apply_mesh_commit(commit) {
                    self.last_error = Some(error);
                }
            }
            Ok(None) => {}
            Err(error) => self.last_error = Some(error),
        }

        // The mode keeps its gizmo while a plate or a panel has the pointer: only a mode that ended
        // drops the pointer state, or a press on a number would cancel the latch it belongs to.
        if self.editor.contour_edit().is_some() {
            // A latched move keeps following the pointer over its own plates, which are the one
            // surface that belongs to the move rather than to the scene.
            if contour_editing || self.contour_gizmo.is_latched() {
                let action = match self.contour_gizmo.update(
                    &response,
                    rect,
                    self.scene.camera,
                    self.session.project(),
                    &mut self.editor,
                ) {
                    Ok(action) => action,
                    Err(error) => {
                        self.last_error = Some(error);
                        contour_gizmo::GizmoAction::Idle
                    }
                };
                self.apply_contour_action(ui.ctx(), action);
            }
        } else {
            self.contour_gizmo.reset();
        }

        if drawing {
            // Shift locks the next segment to an axis; Alt drops the end and grid snaps and takes
            // the pointer as it is. Tab, handled above, chooses straight or arc input.
            let modifiers = DrawModifiers {
                axis_lock: ui.input(|input| input.modifiers.shift),
                ignore_snaps: ui.input(|input| input.modifiers.alt),
            };
            let action = match self.draw_gizmo.update(
                &response,
                rect,
                self.scene.camera,
                &mut self.editor,
                pointer_over_widget,
                modifiers,
            ) {
                Ok(action) => action,
                Err(error) => {
                    self.last_error = Some(error);
                    DrawAction::Nothing
                }
            };
            let ended = self.apply_draw_action(ui.ctx(), action);
            // The right button commits a fixed figure, the CAD habit that keeps the left button
            // free for placing points. A polyline keeps its chain, so the button opens the menu
            // that chooses whether the next segment is straight or an arc instead (ADR 0128).
            if !ended && !pointer_over_widget && response.clicked_by(egui::PointerButton::Secondary) {
                if self.editor.drawing().is_some_and(|draft| draft.tool().closes()) {
                    if let Some(position) = response.interact_pointer_pos() {
                        self.context_menu.close();
                        self.draft_menu.open(position);
                    }
                } else if let Err(error) = self.commit_drawing(ui::language(ui.ctx())) {
                    self.last_error = Some(error);
                }
            }
        } else if self.editor.drawing().is_none() {
            // The tool ended: its pointer state and its right-click menu go with it. A pointer that
            // merely rests on chrome — a panel, a plate, the menu itself — keeps the frozen plane,
            // the last hover and the plates, which is what lets the plates be clicked (ADR 0126).
            self.draw_gizmo.reset();
            self.draft_menu.close();
        }

        if measuring {
            // Shift locks the second point to the plane axis through the first; Alt measures on the
            // work plane and takes the pointer as it is, without a surface and without the grid.
            let modifiers = MeasureModifiers {
                axis_lock: ui.input(|input| input.modifiers.shift),
                ignore_snaps: ui.input(|input| input.modifiers.alt),
            };
            let changed = self.measure_gizmo.update(
                &response,
                rect,
                self.scene.camera,
                self.session.project(),
                &mut self.editor,
                self.draw_gizmo.grid(),
                pointer_over_widget,
                modifiers,
            );
            if changed {
                ui.ctx().request_repaint();
            }
        } else {
            self.measure_gizmo.reset();
        }

        if voiding {
            self.update_void(ui, &response, rect, pointer_over_widget);
        }

        if pulling {
            // The pull reads the pointer itself: a press on the arrow starts a drag, the release
            // writes the height and keeps pulling, and a click anywhere else finishes. One drag is
            // one `object.extrude.height.set`, so the whole adjustment is a single undo entry.
            match self.extrude_gizmo.update(ui, &response, rect, self.scene.camera, self.session.project()) {
                Ok(Some(event)) => {
                    if let Err(error) = self.apply_extrude_event(event, ui::language(ui.ctx())) {
                        self.last_error = Some(error);
                    }
                }
                Ok(None) => {}
                Err(error) => self.last_error = Some(error),
            }
        }

        if boxing {
            self.update_box(ui, &response, rect);
        }

        // A grip drag resizes a selected box in place: a press mints the edit scope that makes the
        // whole drag one undo entry, and every move writes the sides the pointer reaches (ADR 0092).
        let gripping = interactive
            && !contour_editing
            && !drawing
            && !boxing
            && !pulling
            && !filling
            && !measuring
            && !voiding
            && self.grip_target().is_some();
        if gripping {
            self.update_box_grips(ui, &response, rect);
        } else if !self.box_grips.is_dragging() {
            self.box_grips = BoxGrips::default();
        }

        if filling {
            self.update_fill(ui, &response, rect, fill_arm, card_before, opacity);
            if response.clicked_by(egui::PointerButton::Primary)
                && !pointer_over_widget
                && !pointer_on_chrome
                && !self.distance_covers(&response)
            {
                match self.create_fill(ui::language(ui.ctx())) {
                    Ok(()) => {
                        // The next void is a fresh gesture: its fields may take the keyboard again.
                        self.distance_fields.dismiss(ui.ctx());
                        self.distances = None;
                        self.tool_card_rect = None;
                    }
                    Err(error) => self.last_error = Some(error),
                }
            }
        } else {
            self.reset_fill(ui.ctx());
        }

        let target = gizmo_math::Target {
            camera: self.scene.camera,
            project: self.session.project(),
            selection: self.editor.selected_nodes(),
            snaps: settings.snaps.rule(),
            move_kind: MoveKind::of(shell.mode),
        };
        // A locked node keeps its transform: the guard is checked before the gizmo is updated, so a
        // drag never starts and no command is ever dispatched (ADR 0065). The status bar names the
        // node, so the missing handles read as a refusal instead of a bug.
        let locked_selection = self.editor.locked_selection(self.session.project());
        // The hold is not a mode: a tool that takes the pointer puts the piece back, because a piece
        // in hand may not hold a click back from a running draft, a pull, a grip or the ruler (N25).
        if self.translate_gizmo.is_following()
            && (contour_editing
                || drawing
                || boxing
                || pulling
                || filling
                || measuring
                || voiding
                || mesh_object.is_some()
                || locked_selection.is_some())
        {
            self.translate_gizmo.cancel();
        }
        let update: Result<Option<PlacementCommit>, String> = if !interactive
            || contour_editing
            || drawing
            || boxing
            || pulling
            || filling
            || measuring
            || voiding
            || self.box_grips.is_dragging()
            || navigating
            || mesh_object.is_some()
            || locked_selection.is_some()
        {
            Ok(None)
        } else {
            match Transform::of(shell.mode) {
                // No mode owns a handle, so no gizmo runs: a press where one would be is a pick.
                None => Ok(None),
                Some(Transform::Move | Transform::Furniture) => {
                    let pinned = self.move_pinned();
                    self.translate_gizmo.update(ui, &response, rect, target, pointer_over_widget, pinned, handle_length)
                }
                Some(Transform::Rotate) => {
                    self.rotate_gizmo.update(ui, &response, rect, target, pointer_over_widget, ring_factor)
                }
            }
        };
        match update {
            Ok(Some(commit)) => {
                if let Err(error) = self.apply_gizmo_commit(commit) {
                    self.last_error = Some(error);
                }
            }
            Ok(None) => {}
            Err(error) => self.last_error = Some(error),
        }
        // The camera gestures are the input of a move of the eye: the middle button, the right
        // button and the wheel, whatever tool holds the piece.
        let gesture = ui.input(|input| {
            input.pointer.middle_down()
                || input.pointer.secondary_down()
                || input.smooth_scroll_delta != egui::Vec2::ZERO
        });
        self.carry_cursor(ui.ctx(), gesture || navigating);
        if matches!(Transform::of(shell.mode), Some(Transform::Move | Transform::Furniture))
            && self.translate_gizmo.is_dragging()
        {
            self.show_move_distances(ui, rect, opacity);
        } else {
            self.clear_move_distances(ui.ctx());
        }
        if let Err(error) = self.sync_preview() {
            self.last_error = Some(error);
        }
        // A camera or a light has no drawable, so its glyph and its contribution to the frame are
        // driven by this list rather than by the uploaded geometry.
        let previewed = self.previewed_scene_placements();
        // A drag that holds a button freezes the camera, because the axis and the parameter under
        // the hand are read through it. A carried piece holds nothing, so the view keeps working
        // under it and the hold re-anchors to whatever the camera became (N25).
        let dragging = if contour_editing {
            self.contour_gizmo.is_dragging()
        } else if pulling {
            self.extrude_gizmo.is_dragging()
        } else if mesh_object.is_some() {
            self.vertex_gizmo.is_dragging()
        } else {
            self.translate_gizmo.is_held() || self.rotate_gizmo.is_dragging()
        };
        let consumed_click = if contour_editing
            || drawing
            || boxing
            || pulling
            || filling
            || measuring
            || voiding
            || self.box_grips.is_dragging()
        {
            true
        } else if mesh_object.is_some() {
            self.vertex_gizmo.consumed_click()
        } else {
            match Transform::of(shell.mode) {
                // The mode offers no handle, so nothing consumed the press: the click picks.
                None => false,
                Some(Transform::Move | Transform::Furniture) => self.translate_gizmo.consumed_click(),
                Some(Transform::Rotate) => self.rotate_gizmo.consumed_click(),
            }
        };

        // Looking through an authored camera pins the view: navigation would move a camera the
        // document owns, and a command is the only way to edit document data. The gesture acts on
        // the view under the pointer, so an orbit in one view leaves its neighbour alone.
        let navigable = viewport_owns_view(self.session.project());
        let navigation_rect = target_rect;
        let navigable = navigable && interactive;
        if navigable && response.hovered() && pointer_over_active && !dragging {
            let scroll = ui.input(|input| input.smooth_scroll_delta.y);
            if scroll != 0.0
                && let Some(at) = ui.input(|input| input.pointer.hover_pos())
            {
                self.zoom_about_pointer(
                    target_leaf,
                    target_rect,
                    at,
                    (-f64::from(scroll) * ZOOM_PER_SCROLL_POINT).exp(),
                );
            }
        }

        let drag = response.drag_delta();
        // An armed navigation mode turns the primary button into the camera gesture; the gizmo is
        // neither updated nor painted then, so the drag cannot move an axis under the user's hand.
        if navigable
            && !dragging
            && !self.gizmo.is_dragging()
            && navigating
            && response.dragged_by(egui::PointerButton::Primary)
        {
            match navigation {
                Some(NavigationMode::Orbit) => {
                    if let Some(leaf) = self.layout.leaf_mut(target_leaf) {
                        leaf.orbit(
                            -f64::from(drag.x) * ORBIT_RADIANS_PER_POINT,
                            -f64::from(drag.y) * ORBIT_RADIANS_PER_POINT,
                        );
                    }
                }
                Some(NavigationMode::Pan) => {
                    if let Some(leaf) = self.layout.leaf_mut(target_leaf) {
                        leaf.pan_pixels(f64::from(drag.x), f64::from(drag.y), f64::from(navigation_rect.height()));
                    }
                }
                Some(NavigationMode::Zoom) => {
                    if let Some(at) = response.interact_pointer_pos() {
                        self.zoom_about_pointer(
                            target_leaf,
                            navigation_rect,
                            at,
                            (-f64::from(drag.y) * ZOOM_PER_SCROLL_POINT).exp(),
                        );
                    }
                }
                None => {}
            }
        }
        // An axis drag owns the pointer; rotating the camera mid-drag would move the axis under the
        // pointer and change the drag parameter under the user's hand.
        if navigable && !dragging && !self.gizmo.is_dragging() && response.dragged_by(egui::PointerButton::Middle) {
            let modifiers = ui.input(|input| input.modifiers);
            if let Some(leaf) = self.layout.leaf_mut(target_leaf) {
                if modifiers.shift {
                    leaf.pan_pixels(f64::from(drag.x), f64::from(drag.y), f64::from(navigation_rect.height()));
                } else if modifiers.ctrl {
                    if let Some(at) = response.interact_pointer_pos() {
                        self.zoom_about_pointer(
                            target_leaf,
                            navigation_rect,
                            at,
                            (-f64::from(drag.y) * ZOOM_PER_SCROLL_POINT).exp(),
                        );
                    }
                } else {
                    leaf.orbit(
                        -f64::from(drag.x) * ORBIT_RADIANS_PER_POINT,
                        -f64::from(drag.y) * ORBIT_RADIANS_PER_POINT,
                    );
                }
            }
        } else if navigable
            && !dragging
            && !self.gizmo.is_dragging()
            && response.dragged_by(egui::PointerButton::Secondary)
            && let Some(leaf) = self.layout.leaf_mut(target_leaf)
        {
            leaf.pan_pixels(f64::from(drag.x), f64::from(drag.y), f64::from(navigation_rect.height()));
        }
        // The composite camera follows the navigated view unless an authored camera wins.
        let target_camera = self.layout.leaf(target_leaf).map(|leaf| leaf.camera()).unwrap_or(target_camera);
        self.scene.camera = crate::scene_build::scene_camera(self.session.project(), target_camera);

        let pointer_over_gizmo = response
            .interact_pointer_pos()
            .is_some_and(|pointer| gizmo_rect.is_some_and(|widget| widget.contains(pointer)));

        // The rubber band is the one gesture that starts where nothing else does: a press on the
        // scene with no object, no handle and no tool under it. It is armed here, after the gizmos
        // have claimed their presses, so a press that moved an axis or a grip never draws one.
        let band_starts = interactive
            && !pointer_on_chrome
            && !consumed_click
            && !pointer_over_gizmo
            && !contour_editing
            && !drawing
            && !boxing
            && !pulling
            && !filling
            && !measuring
            && !voiding
            && mesh_object.is_none()
            && navigation.is_none()
            && !self.translate_gizmo.is_dragging()
            && !self.rotate_gizmo.is_dragging()
            && !self.box_grips.is_dragging()
            && !self.gizmo.is_dragging()
            && ui.input(|input| input.pointer.primary_pressed())
            && response.interact_pointer_pos().is_some_and(|pointer| self.over_empty_space(rect, pointer, &previewed));
        self.update_band(ui, &response, rect, band_starts);
        // Whether the click opened a container instead of picking a node, so the furniture move
        // does not take a piece on the way into a level.
        let mut entered_level = false;
        let clicked = interactive
            && !pointer_on_chrome
            && if mesh_object.is_some() {
                // A double-click would otherwise toggle the same vertex twice; the first click of
                // the pair already selected it.
                response.clicked() && !response.double_clicked()
            } else {
                response.clicked() || response.double_clicked()
            };
        // A right click that never became a drag opens the actions of the node under the pointer:
        // the same list the tree row draws (U92, ADR 0115). The right *drag* is the camera pan
        // above, and egui's click distance is what tells the two apart, so a pan opens nothing.
        if interactive
            && !contour_editing
            && !drawing
            && !boxing
            && !pulling
            && !measuring
            && !voiding
            && !consumed_click
            && response.clicked_by(egui::PointerButton::Secondary)
            && let Some(pointer) = response.interact_pointer_pos()
        {
            let hit = scene_gizmo::pick(
                self.scene.camera,
                rect,
                self.session.project(),
                pointer,
                &previewed,
                self.editor.flags(),
            );
            let hit = match hit {
                Some(id) => Some(id),
                None => self.pick_scene(rect, pointer),
            };
            let subject = self.editor.context_menu_subject(self.session.project(), self.session.active_product(), hit);
            // One popup at a time: the draft's menu names a segment kind, and this one names a node.
            self.draft_menu.close();
            self.context_menu.open(subject, pointer);
            ui.ctx().request_repaint();
        }

        if !contour_editing
            && !drawing
            && !boxing
            && !pulling
            && !measuring
            && !voiding
            && clicked
            && !pointer_over_gizmo
            && !consumed_click
            && let Some(pointer) = response.interact_pointer_pos()
            && rect.width() > 0.0
            && rect.height() > 0.0
        {
            // The two modifiers a click may carry: Shift adds to the selection, Ctrl removes one
            // member, and the tree row reads the same pair, so both surfaces agree (U102).
            let pick = Pick::from_modifiers(&ui.input(|input| input.modifiers));
            if let Some(object) = mesh_object {
                let element = self.editor.mesh_edit().map(|edit| edit.element()).unwrap_or_default();
                let additive = ui.input(|input| input.modifiers.shift || input.modifiers.ctrl);
                match element {
                    MeshElement::Vertex => {
                        let hit =
                            mesh_pick::pick_vertex(self.scene.camera, rect, self.session.project(), object, pointer);
                        match hit {
                            Some(vertex) => {
                                self.editor.select_mesh_vertex(vertex, additive);
                            }
                            None if !additive => self.editor.clear_mesh_vertices(),
                            None => {}
                        }
                    }
                    MeshElement::Face => {
                        let hit =
                            mesh_pick::pick_face(self.scene.camera, rect, self.session.project(), object, pointer);
                        match hit {
                            Some(face) => {
                                self.editor.select_mesh_face(face, additive);
                            }
                            None if !additive => self.editor.clear_mesh_faces(),
                            None => {}
                        }
                    }
                }
                ui.ctx().request_repaint();
            } else if let Some(id) = scene_gizmo::pick(
                self.scene.camera,
                rect,
                self.session.project(),
                pointer,
                &previewed,
                self.editor.flags(),
            ) {
                // A camera or light icon has no mesh to ray-pick; it selects the node itself, not the
                // container above it.
                self.editor.pick_node(id, pick);
                if let Err(error) = self.upload_scene() {
                    self.last_error = Some(error);
                }
            } else {
                let hit = self.pick_scene(rect, pointer);
                if response.double_clicked() {
                    // Opening a product repaints its envelope without a document revision, so the
                    // scene is rebuilt here instead of waiting a frame for the next sync.
                    entered_level = self.editor.enter_viewport_level(&mut self.session, hit);
                    if entered_level {
                        if let Err(error) = self.refresh() {
                            self.last_error = Some(error);
                        }
                    } else if let Err(error) = self.upload_scene() {
                        self.last_error = Some(error);
                    }
                } else {
                    self.editor.select_viewport_hit(self.session.project(), self.session.active_product(), hit, pick);
                    if let Err(error) = self.upload_scene() {
                        self.last_error = Some(error);
                    }
                }
            }
            // The furniture move takes the piece the click named, by any part of its body: the click
            // has selected it, and the pointer carries it until the next click puts it down (N24). A
            // click that opened a container is navigation, not a hand on a piece.
            if Transform::of(shell.mode) == Some(Transform::Furniture) && !entered_level {
                self.take_furniture(pointer);
            }
        }

        // The style and the shadow switch are frame data the renderer reads when it draws, so a
        // button in the cluster or the View tab changes the next frame without re-uploading a
        // buffer. The edge list is not: switching between form edges and the full triangulation
        // reads a different cached list, so the last upload is replayed when the switch flips.
        self.layout.apply_frame_display(&mut self.scene);
        self.scene.contact_shadows.strength = ui::display::display_strength(settings.viewport.shadow_strength);
        self.scene.floor_opacity = ui::display::floor_opacity(settings.viewport.floor_opacity);
        if self.scene.all_edges != settings.viewport.all_edges {
            self.scene.all_edges = settings.viewport.all_edges;
            if let Err(error) = self.upload_scene() {
                self.last_error = Some(error);
            }
        }
        // A drag preview commits nothing, but the frame can still follow a moved light: the shader
        // reads one uniform, so a sun direction or a point-light position updates without a mesh
        // rebuild. With no moved light the base lighting is already correct.
        //
        // The passes and the image are the only part of the frame that needs a GPU. A headless
        // application draws the window's own background in their place, so the scene rectangle, the
        // layers and the overlays are still exactly what the window would draw (U77).
        match self.target.as_mut() {
            Some(target) => {
                let lighting = if previewed.is_empty() {
                    self.scene.lighting.clone()
                } else {
                    crate::scene_build::authored_lighting_with(self.session.project(), &previewed, self.editor.flags())
                        .unwrap_or_else(|| self.scene.lighting.clone())
                };
                // One pass per view into the same target: the camera comes from the view, the region
                // from its rectangle, and the aspect ratio from the region, so a narrow view is not
                // stretched by the workspace's shape.
                let views: Vec<ViewSpec> = regions
                    .iter()
                    .filter_map(|(leaf, view_rect)| {
                        let leaf = self.layout.leaf(*leaf)?;
                        let camera = crate::scene_build::scene_camera(self.session.project(), leaf.camera());
                        Some(ViewSpec {
                            camera,
                            region: view_region(area, *view_rect, points_per_pixel),
                            shading: leaf.style(),
                            // The strength and the colour are the workspace's; the switch is this
                            // view's, so one cell of a split draws the shadow and its neighbour does
                            // not (U107).
                            shadows: ui::display::contact_shadows(settings.viewport.shadow_strength, leaf.shadows()),
                        })
                    })
                    .collect();
                target.render(&self.scene, &lighting, &views);
                ui.painter().with_clip_rect(area).image(
                    target.texture_id,
                    area,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            }
            None => {
                ui.painter().with_clip_rect(area).rect_filled(area, 0.0, ui.visuals().extreme_bg_color);
            }
        }
        // The overlaid feedback is drawn per view, with that view's camera and rectangle, so a
        // camera glyph or a selection outline lands where its own pass drew the model.
        let open_boxes = open_frame::OpenBoxes::collect(
            &self.level_bounds,
            self.editor.open_containers(),
            self.scene.product_envelopes(),
        );
        self.brackets.clear();
        for (leaf, view_rect) in &regions {
            let active = *leaf == self.layout.active();
            let camera = self
                .layout
                .leaf(*leaf)
                .map(|leaf| crate::scene_build::scene_camera(self.session.project(), leaf.camera()))
                .unwrap_or(self.scene.camera);
            // The brackets come first: they mark the level the next click works in, and a gizmo over
            // them is a handle of the object that level holds (U89).
            self.brackets.extend(open_frame::show(ui, *view_rect, camera, &open_boxes));
            scene_gizmo::paint(
                ui,
                *view_rect,
                camera,
                self.session.project(),
                self.editor.selected_nodes(),
                &previewed,
                self.editor.flags(),
            );
            if active {
                self.paint_mesh_selection(ui, *view_rect);
                self.paint_selection_outline(ui, *view_rect);
            }
        }
        let paint_target = gizmo_math::Target {
            camera: self.scene.camera,
            project: self.session.project(),
            selection: self.editor.selected_nodes(),
            snaps: settings.snaps.rule(),
            move_kind: MoveKind::of(shell.mode),
        };
        // The object gizmo is hidden in mesh mode: it acts on the whole selection, not on vertices.
        if let Some(edit) = self.editor.contour_edit() {
            let action = self.contour_gizmo.paint(ui, rect, self.scene.camera, self.session.project(), edit);
            if let Some(action) = action {
                self.apply_contour_action(ui.ctx(), action);
            }
        } else if self.editor.drawing().is_some() {
            if let Some(draft) = self.editor.drawing() {
                self.draw_gizmo.paint(ui, rect, self.scene.camera, draft);
            }
            // The plates are drawn whatever the pointer rests on, so the numbers stay beside the
            // point they belong to and a typed value costs no second move of the hand (ADR 0126).
            let action = self.draw_gizmo.show_fields(ui.ctx(), rect, &mut self.editor);
            match action {
                Ok(None) => {}
                Ok(Some(action)) => {
                    self.apply_draw_action(ui.ctx(), action);
                }
                Err(error) => self.last_error = Some(error),
            }
        } else if let Some(draft) = self.editor.box_draft().copied() {
            let plane = WorkPlane::from_camera(self.scene.camera);
            box_gizmo::paint_box(ui, rect, self.scene.camera, plane, response.hover_pos(), &draft);
        } else if let Some(object) = mesh_object {
            let target = vertex_gizmo::MeshTarget {
                camera: self.scene.camera,
                project: self.session.project(),
                object,
                vertices: &mesh_vertices,
            };
            self.vertex_gizmo.paint(ui, rect, target, handle_length);
        } else if pulling {
            self.extrude_gizmo.paint(ui, rect, self.scene.camera, self.session.project());
        } else if filling {
            self.fill_gizmo.paint(ui, rect, self.scene.camera);
        } else if let Some(measure) = self.editor.measure() {
            self.measure_gizmo.paint(ui, rect, self.scene.camera, measure);
        } else if voiding {
            // The niche draws the cell it names: a wireframe box in the product's own frame, so the
            // void reads through the boards around it, with the three sides at its middle.
            if let Some(niche) = self.editor.void().niche() {
                self.void_gizmo.paint(ui, rect, self.scene.camera, self.session.project(), niche);
            }
        } else if navigating || locked_selection.is_some() {
            // The navigation gesture owns the viewport, and a locked selection owns nothing: an
            // axis under the pointer would compete with the camera drag, and a locked node draws no
            // handles because the drag it would start is refused.
        } else {
            // A box shows its grips instead of nothing: the faces and the base corners are the
            // three-side editor, and the placement handles still act beside them.
            self.box_grips.paint(ui, rect, self.scene.camera, self.session.project(), self.grip_target());
            match Transform::of(shell.mode) {
                None => {}
                Some(Transform::Move | Transform::Furniture) => {
                    self.translate_gizmo.paint(ui, rect, paint_target, handle_length);
                }
                Some(Transform::Rotate) => self.rotate_gizmo.paint(ui, rect, paint_target, ring_factor),
            }
        }
        // The rubber band is work-area paint: it lies over the image and the gizmos and under the
        // work cursor's own layer, and it is clipped to the view it was drawn in (ADR 0123).
        if let Some(band) = self.band {
            band.paint(ui, rect);
        }
        // The widget belongs to the view under the pointer, so clicking an axis snaps that view and
        // the readout names the camera it draws with.
        if self.session.project().active_camera().is_none()
            && let Some(widget) = gizmo_rect
            && shell.cluster.widget_visible()
            && let Some(action) = self.gizmo.show(ui, widget, target_camera, settings.viewport.roll_step_degrees)
        {
            match action {
                gizmo::GizmoAction::LookFrom(direction) => {
                    self.layout.active_leaf_mut().look_from(direction);
                }
                gizmo::GizmoAction::Home => self.home_view(),
                gizmo::GizmoAction::ToggleProjection => self.toggle_view_projection(),
                gizmo::GizmoAction::RollTo(degrees) => {
                    self.layout.active_leaf_mut().roll_to(degrees.radians_f64());
                }
                gizmo::GizmoAction::RollBy(degrees) => {
                    self.layout.active_leaf_mut().roll_by(degrees.radians_f64());
                }
            }
            let target_camera = self.layout.leaf(target_leaf).map(|leaf| leaf.camera()).unwrap_or(target_camera);
            self.scene.camera = crate::scene_build::scene_camera(self.session.project(), target_camera);
        }

        // The on-screen display hangs below the corner widget, or in the corner itself while the
        // cube is off: the block is a diagnostic a maker reads, so it does not go away with the
        // widget it happens to sit under (U106). It is paint, so it takes no click.
        if let Some(widget) = gizmo_rect
            && shell.cluster.osd_visible()
        {
            let lines = osd::lines(self.scene.camera, ui::language(ui.ctx()), self.osd.stats(&self.scene));
            let top = if shell.cluster.widget_visible() { widget.bottom() + osd::GAP } else { widget.top() };
            osd::paint(ui, rect, widget.right(), top + osd::height(lines.len()), &lines);
        }

        // The cluster floats over the drawn viewport, so it is the last thing painted inside it. The
        // tool card follows the pointer rather than the bottom centre, so the cluster never moves for
        // it (ADR 0119).
        let view = ClusterView {
            owns_view: viewport_owns_view(self.session.project()),
            projection: self.scene.camera.projection(),
            work_plane: WorkPlane::from_camera(target_camera).label(),
            layout: self.layout.kind(),
            maximized: self.layout.maximized(),
            preset: self.layout.preset(target_leaf),
            style: self.layout.active_style(),
        };
        match view_cluster::show(ui, content, &mut shell.cluster, view, opacity, 0.0) {
            Some(ClusterAction::FrameAll) => self.frame_all(),
            Some(ClusterAction::Home) => self.home_view(),
            Some(ClusterAction::ToggleProjection) => self.toggle_view_projection(),
            Some(ClusterAction::SetLayout(kind)) => self.set_layout(kind),
            Some(ClusterAction::ToggleMaximize) => self.layout.toggle_maximized(),
            Some(ClusterAction::SetPreset(preset)) => self.apply_view_preset(preset),
            Some(ClusterAction::SetStyle(style)) => {
                self.layout.set_active_style(style);
                // The style is frame data the renderer reads per view at draw time, so no upload.
                self.layout.apply_frame_display(&mut self.scene);
            }
            Some(ClusterAction::OpenSettings) => shell.open_settings = true,
            None => {}
        }
        self.show_tool_card(ui, content, pointer, card_before, opacity);
        self.show_draft_menu(ui, content, opacity);
        self.paint_view_frames(ui, content, &regions);
        self.show_context_menu(ui, content, opacity);
    }

    /// Draws the right-click menu of a running polyline and runs the kind it chose.
    ///
    /// A polyline keeps its chain, so the right button cannot finish it the way it finishes a fixed
    /// figure: the menu chooses whether the next segment is straight or an arc — the switch that
    /// used to sit on Tab — and offers the finish the status bar's *Finish* runs (ADR 0126, 0128).
    fn show_draft_menu(&mut self, ui: &mut egui::Ui, viewport: egui::Rect, opacity: u32) {
        let Some((arc, can_finish)) = self
            .editor
            .drawing()
            .filter(|draft| draft.tool().closes())
            .map(|draft| (draft.arc_mode(), draft.points().len() >= 2))
        else {
            self.draft_menu.close();
            return;
        };
        let choice = self.draft_menu.show(ui, viewport, arc, can_finish, opacity);
        let Some(choice) = choice else {
            return;
        };
        match choice {
            ui::DraftMenuChoice::Line => {
                self.editor.set_drawing_arc_mode(false);
            }
            ui::DraftMenuChoice::Arc => {
                self.editor.set_drawing_arc_mode(true);
            }
            // Finish is the status bar's own finish: the same commit path, so a chain committed
            // from the menu and from the bar cannot differ.
            ui::DraftMenuChoice::Finish => {
                if let Err(error) = self.commit_drawing(ui::language(ui.ctx())) {
                    self.last_error = Some(error);
                }
            }
        }
        ui.ctx().request_repaint();
    }

    /// Draws the context menu a right click opened and runs the one entry it chose.
    ///
    /// The list is `ui::node_actions`, the same one the tree row draws, so a capability
    /// cannot reach one surface and miss the other (U92). The chosen entry goes to the editor: a
    /// document edit is dispatched as its command and is one undo entry, while a view entry writes
    /// session state and leaves no revision behind.
    fn show_context_menu(&mut self, ui: &mut egui::Ui, viewport: egui::Rect, opacity: u32) {
        let source = ui::MenuSource {
            project: self.session.project(),
            flags: self.editor.flags(),
            selection: self.editor.selected_nodes(),
            active_product: self.session.active_product(),
        };
        let Some(action) = self.context_menu.show(ui, viewport, &source, opacity) else {
            return;
        };
        let lang = ui::language(ui.ctx());
        if let Err(error) = self.editor.apply_node_action(&mut self.session, action, lang) {
            self.last_error = Some(error);
        }
        ui.ctx().request_repaint();
    }

    /// Takes the selected piece in hand for the furniture move, unless it may travel nowhere.
    ///
    /// The click has already named the piece and set the selection, so this reads the selection and
    /// asks the document what may move: a piece with no allowed axis, a locked piece and an empty
    /// selection are all left alone, and the click stays an ordinary selection.
    fn take_furniture(&mut self, pointer: egui::Pos2) {
        let selection = self.editor.selected_nodes();
        let project = self.session.project();
        if selection.is_empty() || project.furniture_axes(selection) == [false, false, false] {
            return;
        }
        if self.editor.locked_selection(project).is_some() {
            return;
        }
        // The hold itself starts on the next frame, from this point: the gizmo has already run for
        // the frame this click was read in.
        self.translate_gizmo.grab(pointer);
    }

    /// Whether the furniture move is carrying a piece, for the status bar's own words (N24).
    pub(crate) fn holding_piece(&self) -> bool {
        self.translate_gizmo.is_following()
    }

    /// Settles what the carry asks of the pointer this frame (X69).
    ///
    /// The work area draws its own square and hides the system arrow (X40), so the carry asks the
    /// *window* for nothing at all: on a Wayland session the system pointer cannot be moved
    /// (`set_cursor_position` answers `NotSupported`), and a cursor shown through a viewport command
    /// fights `egui-winit`'s own icon, which re-shows it on every icon change. The carry keeps a hand
    /// of its own instead — [`Document::carry_hand`] — and the pointer is simply not drawn while a
    /// camera gesture slides the view under the piece it holds.
    pub(crate) fn carry_cursor(&mut self, _ctx: &egui::Context, gesture: bool) {
        self.cursor_hidden = gesture && self.translate_gizmo.is_following();
    }

    /// Whether the carry has put the pointer away this frame, for the shell's own square.
    ///
    /// Hiding the pointer is a question of not drawing it: a square standing still over a scene
    /// sliding under it is the picture of a piece being left behind, and it is not what happens.
    pub(crate) fn hides_pointer(&self) -> bool {
        self.cursor_hidden
    }

    /// Where the carry's hand is on screen, when it is not where the system pointer is.
    ///
    /// A camera gesture moves the view and the piece stays, so the hand is put back on the middle of
    /// the piece and the square is drawn there; the mouse then moves that hand by its own movement,
    /// so dragging carries the piece from the piece (X69).
    pub(crate) fn carry_hand(&self) -> Option<egui::Pos2> {
        self.translate_gizmo.hand()
    }

    /// The world distance the carried piece has travelled, for a test that watches the preview.
    ///
    /// The preview lives in the session overlay and the document is untouched until the drop, so a
    /// test cannot read the moved box from the project.
    #[cfg(test)]
    pub(crate) fn carried_delta(&self) -> geom::DVec3 {
        self.translate_gizmo.drag_delta()
    }

    /// The rectangle the open context menu drew last frame, which the shell hands to `Chrome`.
    pub(crate) fn menu_rect(&self) -> Option<egui::Rect> {
        self.context_menu.rect()
    }

    /// Divides the workspace into `kind`, keeping the cameras the views already have.
    pub(crate) fn set_layout(&mut self, kind: LayoutKind) {
        self.layout.set_kind(kind);
        self.scene.camera = crate::scene_build::scene_camera(self.session.project(), self.layout.active_camera());
    }

    /// Paints the accent frame of the active view and the borders between views.
    ///
    /// The frame is a thin stroke on the active leaf's rectangle, drawn after the cluster so it
    /// cannot be lost behind a widget; the borders are hairlines on the shared edges and are the
    /// only visible part of the layout, so a split reads as a surface rather than as clipped views.
    fn paint_view_frames(&self, ui: &egui::Ui, area: egui::Rect, regions: &[(crate::view_layout::LeafId, egui::Rect)]) {
        let painter = ui.painter().with_clip_rect(area);
        let line = theme::line(ui);
        for divider in self.layout.dividers(area) {
            let (start, end) = match divider.axis {
                crate::view_layout::SplitAxis::Columns => {
                    let x = divider.first.right();
                    (egui::pos2(x, divider.rect.top()), egui::pos2(x, divider.rect.bottom()))
                }
                crate::view_layout::SplitAxis::Rows => {
                    let y = divider.first.bottom();
                    (egui::pos2(divider.rect.left(), y), egui::pos2(divider.rect.right(), y))
                }
            };
            painter.line_segment([start, end], egui::Stroke::new(1.0, line));
        }
        if regions.len() > 1
            && let Some((_, active)) = regions.iter().find(|(leaf, _)| *leaf == self.layout.active())
        {
            painter.rect_stroke(
                active.shrink(0.5),
                egui::CornerRadius::ZERO,
                egui::Stroke::new(1.0, theme::accent(ui)),
                egui::StrokeKind::Inside,
            );
        }
    }

    /// Paints the selected sub-object elements over the viewport image.
    ///
    /// Markers and face fills are drawn by the egui painter, so sub-object mode needs no renderer
    /// change and leaves the uploaded scene untouched. In vertex mode every vertex is shown with
    /// the selected ones highlighted; in face mode only the selected faces are tinted.
    fn paint_mesh_selection(&self, ui: &mut egui::Ui, rect: egui::Rect) {
        let Some(edit) = self.editor.mesh_edit() else {
            return;
        };
        let Some((placement, mesh)) = mesh_pick::vertex_frame(self.session.project(), edit.object()) else {
            return;
        };
        let preview = self.vertex_gizmo.preview();
        let painter = ui.painter().with_clip_rect(rect);
        match edit.element() {
            MeshElement::Vertex => {
                let selected: HashSet<u32> = edit.vertices().iter().copied().collect();
                for (index, local) in mesh.positions().iter().enumerate() {
                    let Ok(vertex) = u32::try_from(index) else {
                        continue;
                    };
                    // A marker follows the preview during a drag, so the dots stay inside the moved
                    // mesh instead of lagging at the old positions until the release.
                    let local = preview.as_ref().and_then(|preview| preview.moved(vertex, *local)).unwrap_or(*local);
                    let world = mesh_pick::to_world(placement, local);
                    let Some(position) = gizmo_math::screen_point(self.scene.camera, rect, world) else {
                        continue;
                    };
                    let is_selected = selected.contains(&vertex);
                    let radius = if is_selected { MESH_VERTEX_RADIUS + 1.0 } else { MESH_VERTEX_RADIUS };
                    let color = if is_selected { gizmo_math::ACTIVE_COLOR } else { MESH_VERTEX_COLOR };
                    painter.circle_filled(position, radius, color);
                    painter.circle_stroke(
                        position,
                        radius,
                        egui::Stroke::new(1.0, egui::Color32::from_black_alpha(160)),
                    );
                }
            }
            MeshElement::Face => {
                for face in edit.faces() {
                    let Ok(index) = usize::try_from(*face) else {
                        continue;
                    };
                    let Some(polygon) = mesh.faces().get(index) else {
                        continue;
                    };
                    let mut points = Vec::with_capacity(polygon.len());
                    let mut complete = true;
                    for vertex in polygon {
                        let Ok(vertex_index) = usize::try_from(*vertex) else {
                            complete = false;
                            break;
                        };
                        let Some(local) = mesh.positions().get(vertex_index) else {
                            complete = false;
                            break;
                        };
                        let local =
                            preview.as_ref().and_then(|preview| preview.moved(*vertex, *local)).unwrap_or(*local);
                        let world = mesh_pick::to_world(placement, local);
                        let Some(position) = gizmo_math::screen_point(self.scene.camera, rect, world) else {
                            complete = false;
                            break;
                        };
                        points.push(position);
                    }
                    if !complete || points.len() < 3 {
                        continue;
                    }
                    painter.add(egui::Shape::convex_polygon(
                        points,
                        MESH_FACE_FILL,
                        egui::Stroke::new(1.5, gizmo_math::ACTIVE_COLOR),
                    ));
                }
            }
        }
    }

    /// Paints the selected line chains over the viewport image as a screen-space outline.
    ///
    /// The renderer draws a chain as a one-pixel `LineList`, which wgpu cannot widen, so a selected
    /// chain is repeated here: a dark underlay first, then the bright stroke. The chains come from
    /// the frame last uploaded, so a drag preview and its outline agree.
    fn paint_selection_outline(&self, ui: &mut egui::Ui, rect: egui::Rect) {
        if self.selected_chains.is_empty() {
            return;
        }
        let painter = ui.painter().with_clip_rect(rect);
        for points in selection_outline(self.scene.camera, rect, &self.selected_chains) {
            painter.add(egui::Shape::line(
                points.clone(),
                egui::Stroke::new(SELECTION_UNDERLAY_WIDTH, SELECTION_UNDERLAY),
            ));
            painter.add(egui::Shape::line(points, egui::Stroke::new(SELECTION_OUTLINE_WIDTH, SELECTION_OUTLINE)));
        }
    }

    /// Creates the product frame the card describes and opens it.
    ///
    /// The card is window chrome, so the shell hands the draft here: this tab dispatches
    /// `product.create` through its editor, which then opens the frame as a session view operation
    /// (ADR 0098, ADR 0100).
    pub(crate) fn create_product(
        &mut self,
        draft: ui::product_card::ProductDraft,
        lang: ui::Lang,
    ) -> Result<(), String> {
        self.editor.create_product(&mut self.session, draft, lang)
    }

    /// The ribbon over the design space: the tabs and the tool groups of the active tab.
    ///
    /// The strip floats on the middle layer instead of taking height from the viewport, so the scene
    /// image runs under it and a translucent ribbon blends with the furniture (U51, ADR 0080). It is
    /// anchored to `design` — the window below the header — so resizing a side panel never reflows
    /// it; the side panels are inset by [`Self::ribbon_inset`] instead. Returns whether the ribbon
    /// asked for the product-frame card, which the window owns.
    pub(crate) fn show_ribbon(
        &mut self,
        ui: &mut egui::Ui,
        design: egui::Rect,
        shell: &mut Shell,
        settings: &Settings,
    ) -> bool {
        let view = ui::RibbonView {
            perspective: matches!(
                crate::scene_build::scene_camera(self.session.project(), self.layout.active_camera()).projection(),
                Projection::Perspective { .. }
            ),
            owns_view: viewport_owns_view(self.session.project()),
            grid: self.draw_gizmo.grid(),
            // The shadow switch is the active view's; its intensity is the workspace's and has no
            // control in the ribbon (U107, U110).
            shadows: self.layout.active_shadows(),
        };
        let mut outcome = None;
        // Whether the ruler or the niche owned the pointer before this frame, so a cell that armed
        // or dropped one also resets the shell's own gizmos (ADR 0118).
        let was_measuring = self.editor.measure().is_some();
        let was_void = self.editor.void_armed();
        // The ribbon is its own chrome surface — one step above the window's surface and distinct
        // from the panel fill the side panels use behind their content. The Appearance slider scales
        // that fill's alpha, letting the scene it covers show through without changing any other
        // surface.
        let fill = theme::at_opacity(theme::ribbon(ui), settings.appearance.panel_opacity);
        // The band the strip occupies is also the band it owns the pointer over, so the tools behind
        // it never see a hover or a click there. The measured rectangle wins once egui has sized the
        // area, which is what keeps the blocked band and the drawn one the same.
        let band = shell.ribbon_band(design, self.editor.ribbon_open());
        let rect = strip_area(ui.ctx(), "ribbon", band, fill, |ui| {
            match self.editor.show_ribbon(ui, &mut self.session, view) {
                Ok(result) => outcome = Some(result),
                Err(error) => self.last_error = Some(error),
            }
        });
        // The measured strip wins over the deterministic height: a narrow window grows the body with
        // its scroll bar, and the side panels must start below whatever the ribbon actually covers.
        if rect.is_some() {
            shell.ribbon_rect = rect;
        }
        // An open method list hangs under the band and over the scene: the shell keeps its rectangle
        // so the viewport reads a press or the pointer inside it as chrome, and the list's own cursor
        // is the one the frame shows (X37).
        shell.list_rect = outcome.as_ref().and_then(|outcome| outcome.list_rect);
        let mut open_product_card = false;
        if let Some(outcome) = outcome {
            open_product_card = outcome.open_product_card;
            if outcome.started_drawing {
                self.draw_gizmo.reset();
                self.cancel_extrude();
                // Starting a drawing tool is a deliberate change of gesture: an armed navigation
                // mode must not take the primary button back when the draft ends.
                shell.cluster.navigation = None;
            }
            if self.editor.measure().is_some() && !was_measuring {
                // The ruler was armed from its cell: leave the shell state of the tool it replaced,
                // and a navigation mode that would take the next press for an orbit.
                self.start_measure(ui.ctx());
                shell.cluster.navigation = None;
            } else if was_measuring && self.editor.measure().is_none() {
                // A drawing or creation cell replaced it; the ruler's pointer state goes with it.
                self.cancel_measure();
            }
            if self.editor.void_armed() && !was_void {
                // The niche was armed from its cell or its key: leave the shell state of the tool it
                // replaced and the navigation mode that would take the next press.
                self.start_void(ui.ctx());
                shell.cluster.navigation = None;
            } else if was_void && !self.editor.void_armed() {
                // Another cell replaced it; the cell it named goes with the tool.
                self.cancel_void();
            }
            if let Some((object, height)) = outcome.extruded {
                // The body exists at the height the field named; the arrow lets the user pull it
                // from there, and the ribbon keeps that height for the next extrusion.
                self.contour_gizmo.reset();
                self.draw_gizmo.reset();
                self.extrude_gizmo.start(object, height);
            }
            // The ribbon reports a *Close* instead of running it: the shell owns the work plane,
            // the gizmo state and the scene rebuild, and every close takes the same path (ADR 0052).
            if outcome.close_draft
                && let Err(error) = self.close_draft(ui::language(ui.ctx()))
            {
                self.last_error = Some(error);
            }
            match outcome.view {
                Some(ui::RibbonViewAction::FrameAll) => self.frame_all(),
                Some(ui::RibbonViewAction::ToggleProjection) => {
                    self.toggle_view_projection();
                }
                Some(ui::RibbonViewAction::ToggleGrid) => self.draw_gizmo.toggle_grid(),
                Some(ui::RibbonViewAction::ToggleShadows) => self.toggle_shadows(),
                None => {}
            }
        }
        open_product_card
    }

    /// Turns the ground shadow of the active view on or off (U107).
    ///
    /// The switch is the view's, so the neighbour keeps drawing what it drew; the frame's fallback
    /// follows the active leaf and the per-view specs are built from each leaf at draw time, so the
    /// change costs no upload.
    pub(crate) fn toggle_shadows(&mut self) {
        self.layout.toggle_active_shadows();
        self.layout.apply_frame_display(&mut self.scene);
    }

    /// Reads the keys a running tool owns and leaves the rest to the shell.
    ///
    /// `modals` is the window's answer to "does a dialog or the palette own the keys this frame": a
    /// modal that is up keeps the keys for itself, exactly as a focused text field does. That covers
    /// the tool table as well: starting a tool under the Settings window would hand the next click
    /// to a tool the user never asked for. The armed navigation mode and the display popover live on
    /// the [`Shell`], so the two that concern them are read and written there.
    pub(crate) fn handle_keys(&mut self, ui: &egui::Ui, modals: bool, shell: &mut Shell) {
        // An open variant list owns Escape and Enter: the popup closes itself and runs its
        // highlighted method when the ribbon draws later in this frame, and one key must not also
        // drop a draft or commit it (U67).
        let list = self.editor.variant_list_open();
        let escape = !list && ui.input(|input| input.key_pressed(egui::Key::Escape));
        let enter = !list && ui.input(|input| input.key_pressed(egui::Key::Enter));
        if self.editor.contour_edit().is_some() {
            // The working copy is dropped by Escape and applied by Enter; typing in a field keeps
            // both keys for the field, and a corner drag keeps them for the drag. A latched move is
            // the first thing Escape gives up, so the corner returns to where it was latched and
            // the mode stays: the second Escape is the mode's own (ADR 0129).
            if !ui.ctx().text_edit_focused() && !self.contour_gizmo.is_dragging() && !self.contour_gizmo.holds_keys() {
                if escape {
                    if !self.contour_gizmo.abandon_latch(&mut self.editor) {
                        self.cancel_contour_edit();
                    }
                } else if enter {
                    let result = if self.contour_gizmo.is_latched() {
                        self.drop_contour_move()
                    } else {
                        self.apply_contour_edit()
                    };
                    if let Err(error) = result {
                        self.last_error = Some(error);
                    }
                }
            }
        } else if self.editor.drawing().is_some() {
            // Escape drops the draft and ends the tool; nothing reached the document and undo gains
            // no half figure. Enter and the right button commit a valid draft (ADR 0076), and a
            // refused commit keeps both the draft and the tool so the user can fix or cancel it.
            //
            // While a plate holds the keyboard the atom reads these keys itself: Enter places the
            // point with every plate and Escape drops the typed numbers before the tool, so one key
            // never means two things (ADR 0126). Tab is the atom's focus key, and a polyline's
            // segment kind is chosen from its right-click menu (ADR 0128).
            if !ui.ctx().text_edit_focused() && !self.draw_gizmo.holds_keys() {
                match drawing_key(escape, enter) {
                    Some(DraftKey::Cancel) => self.cancel_drawing(),
                    Some(DraftKey::Commit) => {
                        if let Err(error) = self.commit_drawing(ui::language(ui.ctx())) {
                            self.last_error = Some(error);
                        }
                    }
                    None => {}
                }
            }
        } else if self.editor.box_draft().is_some() || self.box_grips.is_dragging() {
            // Escape drops the base, the pull or the grip drag; the grip's edit scope is rolled
            // back, so a cancelled resize leaves no trace at all.
            if escape {
                self.cancel_box();
            } else if enter
                && !ui.ctx().text_edit_focused()
                && self.editor.box_draft().is_some_and(BoxDraft::is_pulling)
                && let Err(error) = self.finish_box(ui::language(ui.ctx()))
            {
                self.last_error = Some(error);
            }
        } else if self.extrude_gizmo.is_active() {
            // Escape drops the pull and its preview whatever has the focus: the field's text is part
            // of the preview, so a cancel is never only a text cancel. Enter is the bar's when the
            // field has the focus, because the field commits the typed height first and the bar
            // then applies it; outside the field Enter finishes here, before the viewport draws.
            if escape {
                self.cancel_extrude();
            } else if enter
                && !ui.ctx().text_edit_focused()
                && let (Some(object), Some(height), top) =
                    (self.extrude_gizmo.object(), self.extrude_gizmo.height(), self.extrude_gizmo.top())
                && let Err(error) = self.finish_extrude(object, height, top, ui::language(ui.ctx()))
            {
                self.last_error = Some(error);
            }
        } else if escape
            && !self.translate_gizmo.is_dragging()
            && !self.rotate_gizmo.is_dragging()
            && !self.vertex_gizmo.is_dragging()
            && !ui.ctx().text_edit_focused()
            // The exact-distance field owns Escape while it is active: the key returns that
            // direction to the pointer, and the armed tool stays armed (ADR 0103).
            && !self.distances.as_ref().is_some_and(Distances::is_active)
        {
            if modals {
                // The modal owns Escape for this frame and closes itself.
            } else if shell.cluster.settings_open {
                // The popover owns Escape for this frame and closes itself.
            } else if shell.cluster.navigation.is_some() {
                shell.cluster.navigation = None;
            } else if self.editor.mesh_edit().is_some() {
                self.editor.end_mesh_edit();
            } else if self.editor.measure().is_some() {
                // Escape leaves the ruler before it leaves a container level: the tool is the mode
                // the next click uses, and the points it held are view state, so nothing is undone.
                self.cancel_measure();
            } else if self.editor.fill_arm().is_some() {
                // Escape leaves the empty-space tool before it leaves a container level: the tool is
                // the mode the next click uses.
                self.editor.cancel_fill();
                self.reset_fill(ui.ctx());
            } else if self.editor.void_armed() {
                // Escape leaves the niche tool the same way, and writes nothing because the tool
                // wrote nothing: the point and the cell it named are view state (ADR 0118).
                self.cancel_void();
            } else if self.editor.creation_parent().is_some() {
                // The arm outranks leaving a container level: it is the mode the next creation uses,
                // and the chip names it until Escape or ✕ clears it (ADR 0056).
                self.editor.clear_creation_parent();
            } else if self.band.is_some() {
                // A live rubber band is the last gesture Escape gives up before a container level:
                // the rectangle is a gesture and not content, so the release that follows selects
                // nothing and nothing was written to undo (ADR 0137).
                self.band = None;
            } else {
                let before = self.editor.selected_nodes().to_vec();
                if self.editor.exit_level(&mut self.session) {
                    // Leaving the product repaints its envelope without a document revision, so the
                    // scene is rebuilt here instead of waiting a frame for the next sync.
                    if let Err(error) = self.refresh() {
                        self.last_error = Some(error);
                    }
                } else if self.editor.selected_nodes() != before.as_slice()
                    && let Err(error) = self.upload_scene()
                {
                    self.last_error = Some(error);
                }
            }
        }
        // A drawing shortcut starts its tool from anywhere outside a text field and outside a modal,
        // including while another draft runs: choosing a tool ends the running draft and starts the
        // new one (ADR 0076). The table is the one the toolbar hints show, so the key and the tooltip
        // cannot drift apart.
        if !modals && self.editor.contour_edit().is_none() && !ui.ctx().text_edit_focused() {
            let sides = self.editor.polygon_sides();
            if take_drawing_shortcut(ui.ctx(), &mut self.editor, sides) {
                self.draw_gizmo.reset();
                self.measure_gizmo.reset();
                self.cancel_extrude();
                shell.cluster.navigation = None;
            }
        }
    }
}

/// What the Delete key asks of the window.
///
/// One node goes at once, because undo covers it and a one-row removal is what the person just
/// looked at; two or more ask first, because a batch is removed in one step and a row the person
/// did not mean to take is easy to miss. A locked node holds the whole gesture back before either
/// happens (ADR 0065, ADR 0112).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DeleteRequest {
    /// Nothing was selected: the key did nothing.
    Nothing,
    /// Remove these nodes now; the document reports its own refusal in the status bar.
    Remove(Vec<Id>),
    /// Ask before removing these nodes.
    Ask(Vec<Id>),
    /// A locked node is part of the selection; the sentence names it.
    Refused(String),
}

/// The rule the Delete key follows, read from the selection, the document and the lock flags.
///
/// The decision is a plain function, so the shell test reads it without a window or a GPU, and the
/// removal it names is one `node.delete.many` whichever way it was reached.
pub(crate) fn delete_request(editor: &Editor, project: &Project, lang: Lang) -> DeleteRequest {
    let nodes = editor.selected_nodes().to_vec();
    if nodes.is_empty() {
        return DeleteRequest::Nothing;
    }
    if let Some(locked) = editor.locked_selection(project) {
        return DeleteRequest::Refused(editor.locked_refusal(project, locked, lang).unwrap_or_default());
    }
    if nodes.len() > 1 { DeleteRequest::Ask(nodes) } else { DeleteRequest::Remove(nodes) }
}

/// The command ids that remove or restructure a node, and so refuse a locked one.
///
/// The palette builds every parameter from the selection, so these are the commands whose target is
/// a node the user may have locked. The guard runs in the shell, before the dispatch, because a lock
/// is a guard on a gesture and not a document rule (ADR 0065).
const DESTRUCTIVE_COMMANDS: [&str; 5] = [
    engine::node_cmds::NODE_DELETE_ID,
    engine::node_cmds::NODE_DELETE_MANY_ID,
    engine::node_cmds::NODE_UNGROUP_ID,
    engine::node_cmds::NODE_COMPONENT_CONVERT_TO_GROUP_ID,
    engine::node_cmds::NODE_GROUP_ID,
];

/// The destructive command ids whose target is the whole selection rather than the primary node.
///
/// A group and a batch removal take every selected node, so one locked member refuses the gesture
/// the same way the tree's own guard does; the others name a single node.
const SELECTION_COMMANDS: [&str; 2] = [engine::node_cmds::NODE_GROUP_ID, engine::node_cmds::NODE_DELETE_MANY_ID];

/// The name of a locked node a finished gizmo drag would move, if there is one.
///
/// The viewport refuses the drag before it starts; this is the second gate, checked in
/// [`Document::apply_gizmo_commit`] so no other dispatch path can move a locked node either.
fn locked_change<'a>(project: &'a Project, commit: &PlacementCommit, editor: &Editor) -> Option<&'a str> {
    commit
        .changes
        .iter()
        .find(|change| editor.is_locked(change.node))
        .and_then(|change| project.node(change.node))
        .map(model::Node::name)
}

/// The refusal a destructive command gets while a node it targets is locked.
///
/// `id` is the stable command id the palette (or a future CLI) would dispatch; anything outside the
/// destructive set runs untouched. A command that names a single node asks about the primary
/// selection, and one that takes the whole selection refuses if any member is locked.
fn locked_command_refusal(editor: &Editor, project: &Project, id: &str, lang: Lang) -> Option<String> {
    if !DESTRUCTIVE_COMMANDS.contains(&id) {
        return None;
    }
    if SELECTION_COMMANDS.contains(&id) {
        let locked = editor.locked_selection(project)?;
        return editor.locked_refusal(project, locked, lang);
    }
    let primary = editor.selected_node()?;
    editor.locked_refusal(project, primary, lang)
}

/// Whether the viewport owns its view.
///
/// An authored camera stores its placement and projection in the document, so looking through one
/// pins both navigation and the projection toggle; a command is the only way to change either.
fn viewport_owns_view(project: &Project) -> bool {
    project.active_camera().is_none()
}

/// Where the corner orientation widget draws this frame, or `None` while it is hidden.
///
/// The placement is the free top corner of `view` below the ribbon band and beside both panels
/// ([`gizmo::widget_rect`]); the display switch in the cluster's settings popover turns the whole
/// widget off, and the shell then draws no cube, no home and no projection button and leaves the
/// click to the scene.
fn widget_placement(cluster: &ViewCluster, view: egui::Rect, chrome: &Chrome) -> Option<egui::Rect> {
    if cluster.widget_visible() || cluster.osd_visible() { gizmo::widget_rect(view, chrome) } else { None }
}

/// The exact distances of one translate drag, measured once when the drag starts.
///
/// The walls and the start faces are snapshots: the document still holds the start placements, so
/// the preview, the fields and the release read one measurement (ADR 0103).
#[derive(Clone, Copy, Debug)]
struct MoveDistances {
    /// The anchor node the measurement was taken on.
    node: Id,
    /// The world axis the drag runs along.
    axis: gizmo_math::Axis,
    /// The low face's world coordinate along the axis at the drag start.
    start_low: f64,
    /// The moved box's extent along the axis.
    extent: f64,
    /// The world centres of the two start faces.
    low_face: geom::DVec3,
    high_face: geom::DVec3,
    /// The moved box's own size, so a captured line can be drawn across it.
    size: geom::DVec3,
    /// The surfaces that stop the two faces, in the axis's world coordinates.
    wall_low: Option<f64>,
    wall_high: Option<f64>,
    /// The measurement the fields draw and the numbers they pin.
    distances: Distances,
}

/// Measures the moving box and the two surfaces along a drag axis (ADR 0103).
///
/// A translate drag reads this once, when it starts: the document still holds the start placements,
/// so the preview, the fields and the release commit one measurement.
fn measure_move(project: &Project, node: Id, axis: gizmo_math::Axis) -> Option<MoveDistances> {
    let (min, max) = project.world_box(node)?;
    let direction = axis.direction();
    let (low, high) = (min.dot(direction), max.dot(direction));
    if !(low.is_finite() && high.is_finite()) || high <= low {
        return None;
    }
    // The face centres, which is where the probe and the field anchors sit.
    let centre = (min + max) * 0.5;
    let low_face = centre - direction * (centre.dot(direction) - low);
    let high_face = centre + direction * (high - centre.dot(direction));
    // A probe starting exactly on a face can meet that face; one micrometre outside it cannot.
    const INSET_MM: f64 = 1.0e-3;
    let wall_low =
        project.probe(low_face - direction * INSET_MM, -direction).map(|probe| low - INSET_MM - probe.distance_mm);
    let wall_high =
        project.probe(high_face + direction * INSET_MM, direction).map(|probe| high + INSET_MM + probe.distance_mm);
    let distances = Distances::new(wall_low, wall_high, low, high)?;
    Some(MoveDistances {
        node,
        axis,
        start_low: low,
        extent: high - low,
        low_face,
        high_face,
        size: max - min,
        wall_low,
        wall_high,
        distances,
    })
}

/// The surfaces the drag's fields measure against, with a captured one standing where the rule put it.
///
/// The probe answers the first surface a ray meets, which is not necessarily the surface a snap
/// used: a coordinate plane has no body behind it, so the field would measure a wall the piece never
/// touched, or find none at all. The capture names the surface the rule used, so its coordinate
/// replaces the probe's answer on that face and the field shows the same surface the release
/// landed on (ADR 0146).
fn captured_walls(measure: &MoveDistances, capture: Option<model::Capture>) -> (Option<f64>, Option<f64>) {
    match capture {
        Some(capture) if capture.end == model::SnapEnd::Low => (Some(capture.at_mm), measure.wall_high),
        Some(capture) => (measure.wall_low, Some(capture.at_mm)),
        None => (measure.wall_low, measure.wall_high),
    }
}

/// Draws the surface a snap landed the piece on, across the face that met it.
///
/// A coordinate plane has no neighbour to look at, so a drag that jumped to it would otherwise show
/// only a number that moved on its own. The line lies in the captured surface and runs across the
/// moved box, in the helper ink the other guide geometry uses (ADR 0146).
fn paint_capture(
    painter: &egui::Painter,
    camera: Camera,
    viewport: egui::Rect,
    measure: &MoveDistances,
    centre: geom::DVec3,
    axis: gizmo_math::Axis,
    ink: egui::Color32,
) {
    let (first, second) = match axis {
        gizmo_math::Axis::X => (geom::DVec3::Y, geom::DVec3::Z),
        gizmo_math::Axis::Y => (geom::DVec3::X, geom::DVec3::Z),
        gizmo_math::Axis::Z => (geom::DVec3::X, geom::DVec3::Y),
    };
    let half = measure.size * 0.5;
    let stroke = egui::Stroke::new(1.0, ink);
    for (unit, reach) in [(first, half.dot(first)), (second, half.dot(second))] {
        let end = unit * (reach + CAPTURE_OVERHANG_MM);
        let (Some(from), Some(to)) = (
            gizmo_math::screen_point(camera, viewport, centre - end),
            gizmo_math::screen_point(camera, viewport, centre + end),
        ) else {
            continue;
        };
        painter.line_segment([from, to], stroke);
    }
}

/// The world axis a product-frame axis index names.
fn axis_unit(axis: usize) -> geom::DVec3 {
    match axis {
        0 => geom::DVec3::X,
        1 => geom::DVec3::Y,
        _ => geom::DVec3::Z,
    }
}

/// Whether a finish still has to write the height.
///
/// The pull closes on the first finish, so a second check — a double click, or a key and a button
/// in one frame — finds no pull and dispatches nothing: the check applies once however often it is
/// clicked (M71).
fn extrude_writes(committed: Option<Length>, height: Length) -> bool {
    committed.is_some_and(|committed| committed != height)
}

/// The body `object` would have at `height`, for the session preview.
///
/// The preview is the stored body with one number changed, so the frame shows the exact solid the
/// ✓ would store: the same path, the same features and the same evaluator (M71).
fn previewed_body(project: &Project, object: Id, height: Length, top: Option<Length>) -> Result<Geometry, String> {
    let node = project.object(object).ok_or_else(|| format!("{object:?} is not in the scene"))?;
    match node.geometry() {
        Geometry::Primitive(model::Primitive::Cone(cone)) => {
            cone_at(project, object, height, top.unwrap_or(cone.top_radius())).map(Geometry::Primitive)
        }
        Geometry::Path(path) => {
            // A profile has no height yet, so the preview is the body the ✓ would build from it
            // (ADR 0138).
            model::Constructive::extrude(path.clone(), height)
                .map(Geometry::Constructive)
                .map_err(|error| error.to_string())
        }
        _ => {
            let mut body = node
                .geometry()
                .as_constructive()
                .cloned()
                .ok_or_else(|| format!("{} is not a constructive body", node.name()))?;
            body.set_height(height).map_err(|error| error.to_string())?;
            Ok(Geometry::Constructive(body))
        }
    }
}

/// The cone `object` would have at `height` and `top`.
///
/// The base radius and the side count are the drawn ones, so the pull changes exactly the two
/// numbers it shows and a preview is the solid the ✓ would store (ADR 0141).
fn cone_at(project: &Project, object: Id, height: Length, top: Length) -> Result<model::Primitive, String> {
    let primitive = project.object_primitive(object).map_err(|error| error.to_string())?;
    let model::Primitive::Cone(cone) = primitive else {
        return Err(format!("{object:?} is not a cone"));
    };
    model::Primitive::cone(cone.base_radius(), top, height, cone.segments()).map_err(|error| error.to_string())
}

/// World placements a drag previews for cameras and lights.
///
/// Only top-most moved nodes matter: a moved container carries its whole subtree. Cameras and
/// lights below any of those nodes get the preview's rigid transform applied to their world
/// placement; a preview with no moved camera or light returns an empty list.
fn preview_nodes(project: &Project, selection: &[Id], preview: Option<Preview<'_>>) -> Vec<(Id, Placement)> {
    let Some(preview) = preview else {
        return Vec::new();
    };
    let moved: Vec<Id> = gizmo_math::move_targets(project, selection).into_iter().map(|target| target.node).collect();
    if moved.is_empty() {
        return Vec::new();
    }
    let mut placements = Vec::new();
    for kind in [StructuralKind::Camera, StructuralKind::Light] {
        for id in project.query(&NodeQuery::any().with_kind(kind)) {
            if !is_under_any(project, id, &moved) {
                continue;
            }
            let Some(world) = project.world_placement(id) else {
                continue;
            };
            if let Some(previewed) = preview.moved_world(world) {
                placements.push((id, previewed));
            }
        }
    }
    placements
}

/// Whether `node` is one of `ancestors` or sits below one of them.
///
/// The drag preview applies one rigid transform to a top-most moved node and everything below it,
/// so a camera or a light inside a moved container has to follow the same transform.
fn is_under_any(project: &Project, mut node: Id, ancestors: &[Id]) -> bool {
    loop {
        if ancestors.contains(&node) {
            return true;
        }
        match project.parent_of(node) {
            Some(parent) => node = parent,
            None => return false,
        }
    }
}

/// Projects the selected line chains into viewport-space polylines.
///
/// A chain is cut where a point falls behind the camera, so the overlay never draws a segment
/// through the viewer; a chain with fewer than two projected points contributes nothing. The
/// function reads only the stored points and world transforms, so a test can call it without a GPU.
fn selection_outline(camera: Camera, viewport: egui::Rect, chains: &[Polyline]) -> Vec<Vec<egui::Pos2>> {
    let mut outlines = Vec::new();
    for chain in chains {
        let mut current = Vec::with_capacity(chain.points.len());
        for point in chain.points.iter() {
            let world = chain.world_transform.transform_point3(*point);
            match gizmo_math::screen_point(camera, viewport, world) {
                Some(position) => current.push(position),
                None => {
                    if current.len() >= 2 {
                        outlines.push(std::mem::take(&mut current));
                    } else {
                        current.clear();
                    }
                }
            }
        }
        if current.len() >= 2 {
            outlines.push(current);
        }
    }
    outlines
}

/// A copy of the frame with one object's dragged vertices at their previewed positions.
///
/// The mesh is cloned, the selected vertices are offset and the clone is tessellated for this frame
/// only; the document keeps the old positions until `mesh.vertices.move` commits them. An object
/// that is not a mesh in the current revision leaves the frame unchanged.
fn preview_vertex_mesh(frame: SceneFrame, project: &Project, preview: VertexPreview<'_>) -> Result<SceneFrame, String> {
    let Some(mesh) = project.object(preview.object).and_then(|object| object.geometry().as_mesh()) else {
        return Ok(frame);
    };
    let mut moved = mesh.clone();
    for vertex in preview.vertices {
        let Ok(index) = usize::try_from(*vertex) else {
            continue;
        };
        let Some(position) = moved.positions().get(index).copied() else {
            continue;
        };
        let Some(target) = preview.moved(*vertex, position) else {
            continue;
        };
        moved.set_position(index, target).map_err(|error| error.to_string())?;
    }
    let data = object_mesh(&moved).map_err(|error| error.to_string())?;
    Ok(frame.with_drawable_mesh(preview.object, Arc::new(data)))
}

/// Whether `node` offers the three-length grips when it is the only selection.
///
/// A product frame owns an envelope, and those handles belong to the piece being worked in: a closed
/// product is one object in the scene, not a box being sized, and selecting it must not open handles
/// that write `product.envelope.set` (ADR 0098, X47). A solid box is a drawn profile, so its handles
/// are the contour's corner grips (ADR 0138).
fn offers_grips(project: &Project, node: Id, active_product: Option<Id>) -> bool {
    if project.container(node).is_some_and(|container| container.is_product()) {
        return active_product == Some(node);
    }
    crate::box_gizmo::is_box(project, node)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use geom::{DMat4, DQuat, DVec3};
    use model::{CameraProjection, ClassCode, Container, Envelope, LightKind};
    use scene::Projection;
    use testkit::{millimetres, sample_project};

    use super::*;
    use crate::icons;
    use crate::view_cluster::projection_button;

    #[test]
    fn a_typed_move_distance_pins_the_gap_to_the_next_surface() {
        // Two 600 mm pieces stand 400 mm apart: the probe finds the right wall and nothing on the
        // left, so one field is live and one is disabled.
        let root = Container::new(Id::new(1).expect("id"), "Root", Placement::IDENTITY, Vec::new());
        let mut project = Project::new(root).expect("an empty project");
        let root_id = project.root().id();
        let envelope = || Envelope::new(millimetres(600), millimetres(600), millimetres(600)).expect("a box");
        let moved = project
            .add_product(root_id, "Moved", ClassCode::GENERAL_PRODUCT, envelope(), Placement::IDENTITY)
            .expect("a product");
        let beside = Placement::new(DVec3::new(1000.0, 0.0, 0.0), DQuat::IDENTITY).expect("a placement");
        project
            .add_product(root_id, "Beside", ClassCode::GENERAL_PRODUCT, envelope(), beside)
            .expect("a second product");
        let measure = measure_move(&project, moved, gizmo_math::Axis::X).expect("two faces");
        assert_eq!(measure.start_low, 0.0);
        assert_eq!(measure.extent, 600.0);
        assert_eq!(measure.wall_low, None, "nothing stands to the left");
        let wall_high = measure.wall_high.expect("the second piece is the right wall");
        assert!((wall_high - 1000.0).abs() < 1.0e-6, "the shell face is the surface: {wall_high}");
        let mut distances = measure.distances;
        assert_eq!(distances.value(Side::High), Some(400.0));
        distances.place(Side::High, 100.0).expect("the piece fits the gap");
        assert!((distances.low() - 300.0).abs() < 1.0e-9, "{:?}", distances.low());
        let pinned = distances.pinned_low().expect("the typed distance pins the low face");
        assert!((pinned - measure.start_low - 300.0).abs() < 1.0e-9, "the drag delta is the typed gap");
    }

    #[test]
    fn the_field_at_a_captured_face_measures_the_surface_the_rule_used() {
        // A piece two millimetres off the Y = 0 plane: the rule snaps it flush, and the field has to
        // measure the plane the release landed on. The probe finds no wall there — a coordinate plane
        // has no body behind it — so without the capture the field would say the piece is free.
        let root = Container::new(Id::new(1).expect("id"), "Root", Placement::IDENTITY, Vec::new());
        let mut project = Project::new(root).expect("an empty project");
        let root_id = project.root().id();
        let envelope = Envelope::new(millimetres(600), millimetres(560), millimetres(720)).expect("a box");
        let at = Placement::new(DVec3::new(0.0, 1500.0, 0.0), DQuat::IDENTITY).expect("a placement");
        let piece = project.add_product(root_id, "Piece", ClassCode::GENERAL_PRODUCT, envelope, at).expect("a product");
        let measure = measure_move(&project, piece, gizmo_math::Axis::Y).expect("two faces");
        assert_eq!(measure.wall_low, None, "nothing stands below the piece");

        let rule = project.move_rule(&[piece], model::SnapSettings::default()).expect("a furniture drag");
        let limit = rule.limit(model::MoveAxis::Y, -1498.0);
        let capture = limit.capture.expect("the plane captured the drag");
        let (wall_low, wall_high) = captured_walls(&measure, Some(capture));
        assert_eq!(wall_low, Some(capture.at_mm), "the field measures the surface the rule named");
        assert_eq!(wall_high, measure.wall_high, "the other side keeps its probe");

        let mut distances = measure.distances;
        let low = measure.start_low + limit.distance_mm;
        distances.sync(wall_low, wall_high, low, low + measure.extent);
        assert_eq!(distances.value(Side::Low), Some(0.0), "the piece landed flush on the plane");
        assert_eq!(distances.low(), capture.at_mm, "the face is where the number says it is");
    }

    #[test]
    fn a_closed_product_offers_no_size_grips_until_it_is_opened() {
        let root = Container::new(Id::new(1).expect("id"), "Root", Placement::IDENTITY, Vec::new());
        let mut project = Project::new(root).expect("an empty project");
        let root_id = project.root().id();
        let envelope = Envelope::new(millimetres(700), millimetres(560), millimetres(720)).expect("an envelope");
        let product = project
            .add_product(root_id, "Cabinet", ClassCode::GENERAL_PRODUCT, envelope, Placement::IDENTITY)
            .expect("a product");
        assert!(!offers_grips(&project, product, None), "a closed piece is not a box being sized");
        assert!(offers_grips(&project, product, Some(product)), "the open piece offers its envelope");
    }

    #[test]
    fn a_cheap_rebuild_is_never_announced() {
        let mut rebuild = Rebuild::default();
        assert!(!rebuild.busy(true), "nothing is known about a scene that has not rebuilt yet");
        rebuild.record(Duration::from_millis(8));
        assert!(!rebuild.slow(), "8 ms is inside the budget");
        assert!(!rebuild.busy(true), "a cheap rebuild shows no message");
        assert!(!rebuild.defer(true), "and it does not cost a frame");
        assert!(!rebuild.busy(false));
    }

    #[test]
    fn a_simulated_slow_rebuild_reports_busy_and_waits_one_frame() {
        // A rebuild that overran the budget is announced before the next one runs: the first frame
        // paints «Перестроение…» and keeps the last good geometry, and the work runs on the next.
        let mut rebuild = Rebuild::default();
        rebuild.record(SLOW_REBUILD);
        assert!(rebuild.slow());
        assert!(rebuild.busy(true), "the status bar says the scene is rebuilding");
        assert!(rebuild.defer(true), "the frame does no work, so it stays responsive");
        assert!(rebuild.busy(true), "the message is still up on the frame that works");
        assert!(!rebuild.defer(true), "the next frame pays for the rebuild");
        rebuild.record(Duration::from_millis(4));
        assert!(!rebuild.busy(true), "a fast rebuild ends the message");
    }

    #[test]
    fn the_rebuild_flag_clears_when_nothing_is_pending() {
        let mut rebuild = Rebuild::default();
        rebuild.record(SLOW_REBUILD);
        assert!(rebuild.defer(true));
        assert!(!rebuild.defer(false), "an idle frame runs no work and clears the flag");
        assert!(rebuild.defer(true), "so the next slow rebuild is announced again");
    }

    #[test]
    fn the_check_writes_once_and_a_second_one_writes_nothing() {
        // The pull is open and holds the document's height: the first check writes nothing either,
        // because the body already has that height.
        assert!(!extrude_writes(Some(millimetres(100)), millimetres(100)), "no write when the document agrees");
        assert!(extrude_writes(Some(millimetres(100)), millimetres(250)), "a new height is written");
        assert!(
            !extrude_writes(None, millimetres(250)),
            "a closed pull writes nothing, so a double check is one write"
        );
    }

    /// Root (1): shelf (2), drawer (3) with front (4), rig (5) carrying a camera and a light.
    struct Fixture {
        project: Project,
        rig: Id,
        camera: Id,
        light: Id,
    }

    fn fixture() -> Fixture {
        let mut project = sample_project();
        let root = project.root().id();
        let rig = project
            .add_component(
                root,
                "Rig",
                Placement::new(DVec3::new(100.0, 0.0, 0.0), DQuat::IDENTITY).expect("placement"),
            )
            .expect("rig");
        let camera = project
            .add_camera(
                rig,
                "Camera",
                Placement::new(DVec3::new(0.0, 20.0, 0.0), DQuat::IDENTITY).expect("placement"),
                CameraProjection::DEFAULT,
            )
            .expect("camera");
        let light = project
            .add_light(
                rig,
                "Fill",
                Placement::new(DVec3::new(0.0, 0.0, 30.0), DQuat::IDENTITY).expect("placement"),
                LightKind::Point,
                [1.0; 3],
                1000.0,
            )
            .expect("light");
        Fixture { project, rig, camera, light }
    }

    #[test]
    fn the_display_switch_hides_the_corner_widget_without_touching_the_document() {
        // The widget is a view-state drawing, so its switch is one: a press on it draws nothing on
        // the next frame and cannot reach the document (U73).
        let session = crate::scene_build::demo_session().expect("the sample scene");
        let revision = session.project().revision();
        let view = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0));
        let chrome = Chrome {
            panels: crate::side_panels::PanelRects::default(),
            ribbon: egui::Rect::from_min_size(view.min, egui::vec2(view.width(), ui::toolbars::height(true))),
            list: None,
            menu: None,
            tool: None,
        };
        let mut cluster = ViewCluster::default();
        assert!(cluster.widget_visible(), "the corner widget is on until the switch turns it off");
        assert!(widget_placement(&cluster, view, &chrome).is_some(), "a plain view has a free corner");

        cluster.show_widget = false;
        assert!(
            widget_placement(&cluster, view, &chrome).is_none(),
            "a hidden widget draws no cube, no home and no projection button"
        );
        assert_eq!(session.project().revision(), revision, "a display switch is view state, never an edit");
    }

    #[test]
    fn the_display_strengths_reach_the_frame_and_the_shadow_does_not_cost_an_upload() {
        // U64: the grid and outline strengths are workspace values the renderer reads when it builds
        // the line buffers, so a change to one repaints the frame; U107: the shadow's intensity is
        // read from the view's uniform, so it never repaints anything. U109: the floor's opacity is
        // read by the same uniform, so it does not repaint either.
        let settings = ViewportSettings {
            grid_strength: 25,
            edge_strength: 0,
            shadow_strength: 80,
            floor_opacity: 40,
            ..ViewportSettings::default()
        };
        let display = Display::of(&settings, SceneColors::DARK);
        let mut frame = SceneFrame::default();
        display.apply(&mut frame);
        assert!((frame.grid.ink_strength - 0.25).abs() < 1.0e-6, "the grid strength reaches the frame");
        assert_eq!(frame.edge_strength, 0.0, "the outline strength reaches the frame");
        assert!((frame.contact_shadows.strength - 0.8).abs() < 1.0e-6, "and the shadow's");
        assert!((frame.floor_opacity - 0.4).abs() < 1.0e-6, "and the floor's own opacity");

        let quieter = Display::of(&ViewportSettings { grid_strength: 10, ..settings }, SceneColors::DARK);
        assert!(quieter.repaints(display), "a grid strength repaints the frame's own lines");
        let darker = Display::of(&ViewportSettings { edge_strength: 50, ..settings }, SceneColors::DARK);
        assert!(darker.repaints(display), "so does an outline strength");
        let dimmer = Display::of(&ViewportSettings { shadow_strength: 30, ..settings }, SceneColors::DARK);
        assert!(!dimmer.repaints(display), "the shadow's intensity is a uniform, never an upload");
        let ground = Display::of(&ViewportSettings { floor_opacity: 100, ..settings }, SceneColors::DARK);
        assert!(!ground.repaints(display), "and so is the floor's opacity");
        assert_ne!(ground.floor_opacity, display.floor_opacity, "though the two frames draw different floors");
        let same = Display::of(&settings, SceneColors::LIGHT);
        assert!(same.repaints(display), "and a theme is the colours the lines are drawn with");
    }

    #[test]
    fn a_theme_switch_repaints_the_frame_without_touching_the_document() {
        let session = crate::scene_build::demo_session().expect("the sample scene");
        let mut cache = EvaluationCache::new();
        let mut kernel = TruckKernel::new();
        let evaluated =
            evaluate(&mut cache, &mut kernel, &session, &ViewFlags::default(), None).expect("evaluated scene");
        let revision = session.project().revision();
        let settings = ViewportSettings::default();
        let mut dark = evaluated.frame.clone();
        let mut light = evaluated.frame;
        Display::of(&settings, SceneColors::DARK).apply(&mut dark);
        Display::of(&settings, SceneColors::LIGHT).apply(&mut light);

        assert_eq!(dark.background, SceneColors::DARK.background);
        assert_eq!(light.background, SceneColors::LIGHT.background, "the viewport clears to the light chrome");
        assert_eq!(light.grid.minor_color, SceneColors::LIGHT.grid.minor_color, "the floor grid follows the variant");
        assert_eq!(light.line_ink, SceneColors::LIGHT.line_ink, "the unlit chains follow it too");
        assert!(light.polylines().iter().all(|line| line.color.is_none()), "a drawn path takes the frame ink");
        let materials =
            |frame: &SceneFrame| frame.drawables().iter().map(|drawable| drawable.color).collect::<Vec<_>>();
        assert_eq!(materials(&light), materials(&dark), "the furniture keeps its material colours");
        assert_eq!(dark.grid.visible, light.grid.visible, "the viewport's grid switch is not the theme's");
        assert_eq!(session.project().revision(), revision, "a variant is view state, never an edit");
    }

    #[test]
    fn the_view_tabs_shadow_switch_leaves_the_neighbour_drawing_its_own() {
        // U107: the ground shadow is an attribute of the view. The View tab's cell flips the active
        // leaf alone, so a split workspace draws one cell with a shadow and the next without, and
        // neither the document nor the workspace's intensity moves.
        let session = crate::scene_build::demo_session().expect("the sample scene");
        let revision = session.project().revision();
        let settings = Settings::default();
        let shell = Shell::default();
        let mut document =
            Document::new(None, &settings, &shell, session, PreservedSections::new()).expect("a headless document");
        let area = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        document.layout.set_kind(crate::view_layout::LayoutKind::TwoColumns);
        let regions = document.layout.regions(area);
        let (first, second) = (regions[0].0, regions[1].0);
        assert!(document.layout.leaf(first).expect("first").shadows(), "a fresh split draws the shadow twice");

        document.toggle_shadows();
        assert!(!document.layout.leaf(first).expect("first").shadows(), "the active view lost its shadow");
        assert!(document.layout.leaf(second).expect("second").shadows(), "its neighbour still draws one");
        assert!(!document.frame().contact_shadows.visible, "the frame's fallback follows the active view");
        assert!(document.frame().contact_shadows.strength > 0.0, "the switch never rewrites the intensity");

        document.layout.set_active(second);
        document.toggle_shadows();
        assert!(!document.layout.leaf(second).expect("second").shadows());
        assert_eq!(document.session.project().revision(), revision, "a view switch is never an edit");
    }

    #[test]
    fn an_authored_camera_takes_the_view_out_of_the_viewport() {
        let mut fixture = fixture();
        assert!(viewport_owns_view(&fixture.project), "the built-in camera is view state");
        fixture.project.set_active_camera(Some(fixture.camera)).expect("activate the camera");
        assert!(!viewport_owns_view(&fixture.project), "an authored camera owns navigation and projection");
        fixture.project.set_active_camera(None).expect("clear the camera");
        assert!(viewport_owns_view(&fixture.project));
    }

    #[test]
    fn no_preview_moves_no_camera_or_light() {
        let fixture = fixture();
        assert!(
            preview_nodes(&fixture.project, &[fixture.rig], None).is_empty(),
            "without a drag the document is the only truth"
        );
    }

    #[test]
    fn moving_a_container_previews_the_cameras_and_lights_inside_it() {
        let fixture = fixture();
        let delta = DVec3::new(10.0, -5.0, 2.0);
        let preview = Preview::Translation { drawables: &[], delta };
        let moved = preview_nodes(&fixture.project, &[fixture.rig], Some(preview));
        assert_eq!(moved.len(), 2, "the camera and the light both follow the rig");
        for (node, placement) in moved {
            let world = fixture.project.world_placement(node).expect("world placement");
            assert_eq!(placement.translation(), world.translation() + delta, "node {node:?} keeps the rig's shift");
            assert_eq!(placement.rotation(), world.rotation(), "a translation keeps the rotation");
        }
    }

    #[test]
    fn turning_a_container_turns_the_nested_icon_about_the_preview_pivot() {
        let fixture = fixture();
        let pivot = DVec3::new(100.0, 0.0, 0.0);
        let rotation = DQuat::from_rotation_z(std::f64::consts::FRAC_PI_2);
        let preview = Preview::Rotation { drawables: &[], pivot, rotation };
        let moved = preview_nodes(&fixture.project, &[fixture.rig], Some(preview));
        assert_eq!(moved.len(), 2);
        for (node, placement) in moved {
            let world = fixture.project.world_placement(node).expect("world placement");
            let expected = pivot + rotation * (world.translation() - pivot);
            assert!(
                (placement.translation() - expected).length() < 1.0e-9,
                "node {node:?} turns about the preview pivot"
            );
            assert!(
                placement.rotation().dot(rotation * world.rotation()).abs() > 1.0 - 1.0e-9,
                "the world turn is pre-multiplied onto the stored rotation"
            );
        }
    }

    #[test]
    fn a_selected_camera_is_previewed_without_a_container() {
        let fixture = fixture();
        let delta = DVec3::new(0.0, 0.0, -7.0);
        let preview = Preview::Translation { drawables: &[], delta };
        let moved = preview_nodes(&fixture.project, &[fixture.camera], Some(preview));
        assert_eq!(moved.len(), 1);
        assert_eq!(moved[0].0, fixture.camera, "only the selected camera moves");
        let world = fixture.project.world_placement(fixture.camera).expect("world placement");
        assert_eq!(moved[0].1.translation(), world.translation() + delta);
    }

    #[test]
    fn a_preview_of_a_node_without_icons_lists_nothing() {
        let fixture = fixture();
        let preview = Preview::Translation { drawables: &[], delta: DVec3::X };
        assert!(
            preview_nodes(&fixture.project, &[Id::new(2).expect("id")], Some(preview)).is_empty(),
            "the shelf carries neither a camera nor a light"
        );
    }

    #[test]
    fn is_under_any_walks_up_to_the_moved_ancestor() {
        let fixture = fixture();
        assert!(is_under_any(&fixture.project, fixture.camera, &[fixture.rig]));
        assert!(is_under_any(&fixture.project, fixture.rig, &[fixture.rig]));
        assert!(is_under_any(&fixture.project, Id::new(4).expect("id"), &[Id::new(3).expect("id")]));
        assert!(!is_under_any(&fixture.project, Id::new(2).expect("id"), &[Id::new(3).expect("id")]));
        assert!(!is_under_any(&fixture.project, fixture.camera, &[fixture.light]));
    }

    #[test]
    fn the_projection_button_follows_the_camera_the_viewport_draws_with() {
        // The strip reads the frame camera, which is the authored camera when one is active and the
        // viewport's own camera otherwise. Both paths must lead to the icon of the mode that is
        // actually drawn, which is what ADR 0049 promises an authored camera.
        let mut fixture = fixture();
        let view = Camera::default();
        assert_eq!(
            projection_button(crate::scene_build::scene_camera(&fixture.project, view).projection()).0,
            icons::view::PERSPECTIVE,
            "with no authored camera the viewport draws its own perspective view"
        );

        fixture.project.set_active_camera(Some(fixture.camera)).expect("activate the camera");
        fixture
            .project
            .set_camera_projection(
                fixture.camera,
                CameraProjection::Orthographic { height: Length::from_millimetres(500).expect("height") },
            )
            .expect("set the projection");

        let drawn = crate::scene_build::scene_camera(&fixture.project, view);
        assert!(matches!(drawn.projection(), Projection::Orthographic { .. }));
        assert_eq!(
            projection_button(drawn.projection()).0,
            icons::view::ORTHOGRAPHIC,
            "an authored orthographic camera draws its own mode, not the viewport's"
        );
        assert!(!viewport_owns_view(&fixture.project), "and the toggle stays disabled");
    }

    fn outline_viewport() -> egui::Rect {
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::new(800.0, 600.0))
    }

    fn selection_chain(points: Vec<DVec3>) -> Polyline {
        Polyline::new(Id::new(7).expect("id"), Arc::from(points), DMat4::IDENTITY, SELECTION_LINE_COLOR)
    }

    #[test]
    fn selection_outline_projects_every_stored_point_at_the_stored_transform() {
        let camera = Camera::default();
        let viewport = outline_viewport();
        let points = vec![DVec3::new(-100.0, 0.0, 0.0), DVec3::ZERO, DVec3::new(100.0, 0.0, 0.0)];
        let outlines = selection_outline(camera, viewport, &[selection_chain(points.clone())]);
        assert_eq!(outlines.len(), 1, "an open chain is one outline");
        assert_eq!(outlines[0].len(), 3, "every stored point is projected, including the last");
        assert!(outlines[0].iter().all(|point| viewport.contains(*point)));

        let mut moved = selection_chain(points);
        moved.world_transform = DMat4::from_translation(DVec3::new(0.0, 0.0, 200.0));
        let moved_outlines = selection_outline(camera, viewport, &[moved]);
        assert_ne!(moved_outlines[0], outlines[0], "the outline reads the chain's world transform");
    }

    #[test]
    fn selection_outline_cuts_a_chain_where_it_falls_behind_the_camera() {
        let camera = Camera::default();
        let behind = camera.eye() - camera.forward() * 100.0;
        let points = vec![
            DVec3::new(-100.0, 0.0, 0.0),
            DVec3::ZERO,
            behind,
            DVec3::new(0.0, 100.0, 0.0),
            DVec3::new(100.0, 100.0, 0.0),
        ];
        let outlines = selection_outline(camera, outline_viewport(), &[selection_chain(points)]);
        assert_eq!(outlines.len(), 2, "the segment through the viewer is dropped");
        assert_eq!(outlines[0].len(), 2);
        assert_eq!(outlines[1].len(), 2);
    }
    /// A drag that started somewhere else and then met a lock is refused at the commit, with the
    /// node named, so no dispatch path can move a locked node.
    #[test]
    fn a_locked_node_refuses_a_gizmo_commit() {
        let project = sample_project();
        let shelf = Id::new(2).expect("shelf id");
        let mut editor = Editor::new();
        let placement = Placement::IDENTITY;
        let commit = PlacementCommit { changes: vec![gizmo_math::PlacementChange { node: shelf, placement }] };

        assert_eq!(locked_change(&project, &commit, &editor), None, "an unlocked node moves");
        editor.set_locked(shelf, true);
        assert_eq!(locked_change(&project, &commit, &editor), Some("Shelf"), "the refusal names the node");
    }

    #[test]
    fn a_locked_primary_selection_refuses_a_destructive_palette_command() {
        let project = sample_project();
        let shelf = Id::new(2).expect("shelf id");
        let mut editor = Editor::new();
        let delete = engine::node_cmds::NODE_DELETE_ID;

        assert_eq!(locked_command_refusal(&editor, &project, delete, text::Lang::En), None, "nothing is selected");
        editor.select_node(Some(shelf));
        assert_eq!(locked_command_refusal(&editor, &project, delete, text::Lang::En), None, "an unlocked node deletes");
        editor.set_locked(shelf, true);
        let refusal = locked_command_refusal(&editor, &project, delete, text::Lang::En).expect("a locked node refuses");
        assert!(refusal.contains("Shelf") && refusal.contains("locked"), "the sentence names it: {refusal}");
        assert_eq!(
            locked_command_refusal(&editor, &project, engine::node_cmds::NODE_RENAME_ID, text::Lang::En),
            None,
            "a command that removes nothing runs on a locked node"
        );

        editor.set_locked(shelf, false);
        assert_eq!(
            locked_command_refusal(&editor, &project, delete, text::Lang::En),
            None,
            "unlocking lets it through"
        );

        // A command that takes the whole selection refuses when any member is locked, the way the
        // tree's own guard does: a batch removal and a group are the two.
        let drawer = Id::new(3).expect("drawer id");
        editor.select_nodes(vec![shelf, drawer]);
        editor.set_locked(drawer, true);
        for id in [engine::node_cmds::NODE_DELETE_MANY_ID, engine::node_cmds::NODE_GROUP_ID] {
            let refusal = locked_command_refusal(&editor, &project, id, text::Lang::En)
                .unwrap_or_else(|| panic!("`{id}` must refuse a batch with a locked member"));
            assert!(refusal.contains("Drawer"), "the sentence names the locked row: {refusal}");
        }
        editor.set_locked(drawer, false);
        assert_eq!(
            locked_command_refusal(&editor, &project, engine::node_cmds::NODE_DELETE_MANY_ID, text::Lang::En),
            None,
            "an unlocked batch runs"
        );
    }

    #[test]
    fn the_delete_key_removes_one_node_at_once_and_asks_before_a_batch() {
        // ADR 0112: undo covers one row, so the key deletes it without a question; a batch is
        // removed in one step and asks first.
        let project = sample_project();
        let shelf = Id::new(2).expect("shelf id");
        let drawer = Id::new(3).expect("drawer id");
        let mut editor = Editor::new();

        assert_eq!(delete_request(&editor, &project, Lang::En), DeleteRequest::Nothing, "nothing is selected");
        editor.select_node(Some(shelf));
        assert_eq!(
            delete_request(&editor, &project, Lang::En),
            DeleteRequest::Remove(vec![shelf]),
            "one unlocked node goes without a question"
        );
        editor.select_nodes(vec![shelf, drawer]);
        assert_eq!(
            delete_request(&editor, &project, Lang::En),
            DeleteRequest::Ask(vec![shelf, drawer]),
            "a batch is the window's question"
        );
    }

    #[test]
    fn a_locked_node_refuses_the_whole_delete_gesture() {
        let project = sample_project();
        let shelf = Id::new(2).expect("shelf id");
        let mut editor = Editor::new();
        editor.select_node(Some(shelf));
        editor.set_locked(shelf, true);

        let request = delete_request(&editor, &project, Lang::En);
        let DeleteRequest::Refused(sentence) = request else {
            panic!("a locked node must refuse the gesture, got {request:?}");
        };
        assert!(sentence.contains("Shelf") && sentence.contains("locked"), "the sentence names it: {sentence}");
    }
}
