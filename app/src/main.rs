//! Z3L desktop application shell.
//!
//! This is the composition root: it creates the window, holds the open tabs, the workspace settings
//! and the window's own view state, and it draws the chrome around the active scene. Everything
//! that belongs to one scene — its session, its editor, its cameras, its renderer target — lives in
//! [`Document`], so activating a tab swaps the workspace without touching a file (U45).
//!
//! The shell dispatches no command itself and never writes to a document it does not own: the
//! header, the menus, the ribbon, the cluster and the keyboard all return what the user asked for,
//! and this file performs it through the active document.

#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod area_gizmo;
mod box_gizmo;
/// The assistant panel: the conversation the turn appends to, and the person's two modes (J7).
mod chat;
/// The command line: `z3l export` runs before the window exists (K3).
mod cli;
mod contour_gizmo;
/// The workshop's directory, read again when another application changes it (O5).
mod directory_app;
mod directory_watch;
mod document;
mod draw_gizmo;
mod extrude_gizmo;
mod file_menu;
/// The system file manager: *Open* and *Save as* ask the operating system for a path (X91).
mod file_picker;
mod fill_gizmo;
/// The shell's frame, asserted without a window (U78).
#[cfg(test)]
mod frame_tests;
mod gizmo;
mod gizmo_math;
mod header;
mod logging;
mod materials_watch;
mod measure_gizmo;
mod mesh_pick;
mod model_list;
mod open_frame;
/// The on-screen display: the diagnostic block in the free corner (U106).
mod osd;
mod points_gizmo;
mod preview;
mod render_place;
mod rotate_gizmo;
mod scene_build;
mod scene_gizmo;
mod scene_snap;
mod scene_writer;
mod settings_store;
mod shortcuts;
mod side_panels;
mod status_bar;
mod strip;
mod tabs;
#[cfg(test)]
mod test_support;
mod translate_gizmo;
/// The interface rendered offscreen, one PNG per cell of the look matrix (U81).
#[cfg(test)]
mod ui_shots;
mod vertex_gizmo;
mod view_cluster;
mod view_layout;
mod void_gizmo;
mod work_cursor;

use std::path::Path;
use std::sync::Arc;

use eframe::egui;
use eframe::egui_wgpu;
use model::Id;
use scene::SceneColors;
use ui::product_card::ProductCard;
use ui::settings::{Settings, SettingsSection};
use ui::ui_scale;
use ui::{Palette, TransformMode, icons, theme};

use crate::chat::ChatAction;
use crate::document::{DeleteRequest, Document, SnapChoice, delete_request};
use crate::file_menu::{Answer, Confirm, FileAction, PreservedSections};
use crate::file_picker::{PickMode, Picker, Started, scene_file};
use crate::header::{HeaderAction, HeaderState, TabView};
use crate::scene_build::empty_session_with;

/// The machine identity of the application: its window class and the directory its settings live in.
///
/// It is the short word rather than a reverse-DNS id, because it is a path component a person reads —
/// `~/.config/z3l` — and because the project owns no domain to name. A `.desktop`, Flatpak or Wayland
/// identity that wants reverse DNS takes its own line here and changes nothing else (ADR 0135).
pub(crate) const APP_ID: &str = "z3l";
use crate::settings_store::SettingsStore;
use crate::shortcuts::{take_assistant_key, take_shortcuts};
use crate::side_panels::{
    ANIMATION_SECONDS, Chrome, EXPANDED_WIDTH, PanelEvent, PanelHead, PanelRects, PanelSide, PanelSlot, Panels,
    STRIP_WIDTH, panel_top, scroll_body, show as show_panel,
};
use crate::strip::strip_rect;
use crate::tabs::{CloseOutcome, TabId, Tabs};
use crate::view_cluster::ViewCluster;

/// The transform gesture a [`TransformMode`] offers.
///
/// The four gestures read the same pivot and commit through the same command, and `Select` offers
/// none, so a caller asks the mode for one of these instead of matching five states twice. The five
/// modes themselves are named in `ui::transform`, beside the key that arms each one and the words
/// the band, the node menu and the Settings page print, because three surfaces read them (U113);
/// this file adds the gesture a mode hands the viewport, which is the shell's own business.
///
/// A gesture comes in two kinds, and the difference is the rule, not the tool (N22, ADR 0147, X88):
/// `Move` keeps the pointer's own distance and applies no rule, `Object` moves a body by the
/// document's own rules, `Rotate` turns the selection through whatever stands beside it, and
/// `ObjectRotate` turns it until another body stops it (ADR 0165, ADR 0180).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Transform {
    /// Three world-axis arrows move the selection without a rule.
    Move,
    /// The arrows move the selection by the document's own rules.
    Object,
    /// Three local-axis rings turn the selection about its origin.
    Rotate,
    /// The rings turn the selection until the document's bodies stop it.
    ObjectRotate,
}

impl Transform {
    /// The gesture this mode offers, or `None` when it offers no handle at all.
    ///
    /// The viewport draws and updates a gizmo only for a `Some`, so a press where a handle would be
    /// cannot start a drag the mode does not offer.
    pub(crate) fn of(mode: TransformMode) -> Option<Self> {
        match mode {
            TransformMode::Select => None,
            TransformMode::Move => Some(Self::Move),
            TransformMode::Object => Some(Self::Object),
            TransformMode::Rotate => Some(Self::Rotate),
            TransformMode::ObjectRotate => Some(Self::ObjectRotate),
        }
    }
}

/// A tab's session, in the mode the assistant is born in (ADR 0190).
///
/// A headless session is created in build: a process that started a tool server made that choice by
/// starting it, and the command line's own `--dry-run` is its plan. The window is the one place with
/// a person in front of the switch, so its tabs start in plan and the first answer may only propose —
/// the switch that lets an agent write is the person's second act.
fn plan_mode(mut session: engine::Session) -> engine::Session {
    session.set_agent_mode(engine::AgentMode::Plan);
    session
}

/// The window's own view state: what every tab shares and no document owns.
///
/// The folds, the floating cluster, the transform mode, the pointer, the measured ribbon band and
/// the viewport rectangle belong to the window, so switching a tab does not move them, while a tab
/// keeps its own camera and its own split layout (ADR 0029). A document borrows this state for the
/// frame it draws; it never stores any of it.
pub(crate) struct Shell {
    /// The two side panels: their visibility, their fold and the memories of the last *hide all*
    /// and *collapse all* press.
    pub(crate) panels: Panels,
    /// The floating viewport cluster: navigation, display style and settings.
    pub(crate) cluster: ViewCluster,
    /// Which transform gesture the viewport offers. Starts at [`TransformMode::Select`].
    pub(crate) mode: TransformMode,
    /// The pointer in viewport points from the previous frame, so a border hover can show its resize
    /// cursor and the wheel can tell whether it is over the active view.
    pub(crate) pointer_position: Option<egui::Pos2>,
    /// The band the floating ribbon covered on the last drawn frame. The side panels are inset by its
    /// height and the framing and the orientation widget stay below it (U51, ADR 0080). `None` until
    /// the first real frame, when the deterministic [`ui::toolbars::height`] stands in.
    pub(crate) ribbon_rect: Option<egui::Rect>,
    /// The rectangle the open method list covered on the last drawn frame.
    ///
    /// The list hangs under the ribbon band and over the scene, so it is chrome the frame must keep
    /// the pointer out of: a tool cursor or a pick inside it would belong to the viewport (X37).
    pub(crate) list_rect: Option<egui::Rect>,
    /// Whether a view border owns the primary button until it is released.
    pub(crate) divider_dragging: bool,
    /// Whether the frame being drawn rebuilt its scene, so the status bar can say «Перестроение…».
    pub(crate) rebuilding: bool,
    /// The cluster asked for Settings › View this frame; the shell opens it and clears this.
    pub(crate) open_settings: bool,
    /// The rectangle the corner orientation widget covered on the last drawn frame.
    ///
    /// It is a widget drawn on the scene layer, so the work cursor cannot find it by the layer test
    /// and asks for it by name: the widget keeps the system cursor (X41).
    pub(crate) orientation_rect: Option<egui::Rect>,
    /// The work cursor the last frame drew, or `None` while a system surface owns the pointer. A
    /// test-only snapshot of the geometry the shell painted (X40).
    #[cfg(test)]
    pub(crate) work_cursor: Option<work_cursor::WorkCursor>,
    /// The background, grid and line ink of the theme variant the window draws with.
    ///
    /// The viewport is a `wgpu` surface, so it cannot read the `egui` palette at draw time; the
    /// shell resolves the variant once per frame and every tab paints its frame with it (X8).
    pub(crate) colors: SceneColors,
}

impl Default for Shell {
    fn default() -> Self {
        Self {
            panels: Panels::default(),
            cluster: ViewCluster::default(),
            mode: TransformMode::Select,
            pointer_position: None,
            ribbon_rect: None,
            list_rect: None,
            divider_dragging: false,
            rebuilding: false,
            open_settings: false,
            orientation_rect: None,
            #[cfg(test)]
            work_cursor: None,
            colors: SceneColors::DARK,
        }
    }
}

impl Shell {
    /// The band the floating ribbon covers at the top of the viewport `area`.
    ///
    /// The measured rectangle from this frame wins, because the ribbon's body can be taller than its
    /// deterministic height in a narrow window; the open state's height stands in on the first frame.
    pub(crate) fn ribbon_band(&self, area: egui::Rect, ribbon_open: bool) -> egui::Rect {
        let height = self.ribbon_rect.map_or_else(|| ui::toolbars::height(ribbon_open), |rect| rect.height());
        strip_rect(area, height.min(area.height()))
    }

    /// The height the side panels start below, so the floating ribbon never hides their header.
    pub(crate) fn ribbon_inset(&self, ribbon_open: bool) -> f32 {
        self.ribbon_rect.map_or_else(|| ui::toolbars::height(ribbon_open), |rect| rect.height())
    }
}

/// A destructive gesture waiting for the user to confirm it.
///
/// The question belongs to the window, not to the document it names: it stays up while the scene it
/// would act on is still open, and an answer performs exactly the gesture it names. It owns the keys
/// for the frame it is up, like every other modal, so a second Delete cannot stack a second question
/// and Escape is the way out (ADR 0112).
struct PendingQuestion {
    /// What the answer does: drop one tab, leave the application, or remove a selection.
    gesture: Gesture,
    question: Confirm,
}

/// What a confirmed question performs.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Gesture {
    /// Drop this tab, even if another scene was activated while the question was up.
    Tab(TabId),
    /// Close the window; every open scene goes with it.
    Quit,
    /// Remove these nodes from the scene the question was asked over.
    Delete {
        /// The scene the question was asked in.
        tab: TabId,
        /// The nodes the question counted.
        nodes: Vec<Id>,
    },
}

/// The folders the shell watches off the frame path: the workshop's nomenclature directory and the
/// render material library beside the settings file.
///
/// One value rather than two fields, because every place that composes a window composes both and a
/// test's workspace is read exactly as a workshop's is.
struct Watching {
    /// The packages a shop connects, and the rows a panel is cut from.
    directory: directory_watch::Watch,
    /// The materials a facade is drawn with, and the pictures beside them (Y8).
    materials: materials_watch::Watch,
}

struct App {
    /// The open scenes, in the order the tab row draws them. Never empty.
    tabs: Tabs<Document>,
    /// The wgpu handles the window and every tab draw with; `None` in a headless run, where a test
    /// drives the same frame with no device, no adapter and no window (U77).
    render_state: Option<egui_wgpu::RenderState>,
    /// The window's own view state, shared by every tab.
    shell: Shell,
    /// The workspace remembered between runs: chrome, viewport defaults and recent files.
    settings: Settings,
    /// Where [`Settings`] is read from and written to.
    settings_store: SettingsStore,
    /// The workers that hand over a newer nomenclature directory and render material library, and
    /// their channels to this frame.
    ///
    /// The frame only takes what a worker finished: polling a folder's revision and validating its
    /// packages or materials both happen off the frame path (`docs/domain/nomenclature.md` §13).
    watching: Watching,
    /// The section the Settings window was last left on.
    settings_section: SettingsSection,
    /// The model list the *Assistant* page asked its endpoint for, and the last answer (ADR 0234).
    ///
    /// It is view state of this window: the request is made off the frame, the answer is thrown away
    /// when another address is asked for, and nothing of it is ever written to the settings file.
    assistant_models: model_list::Models,
    /// Whether the Settings window is open.
    settings_open: bool,
    /// Whether the Home surface is open.
    home_open: bool,
    /// Whether the workspace shows the render place instead of the scene (ADR 0211).
    render_open: bool,
    /// The render place's numbers, its run and where the last one got to.
    render_place: render_place::RenderPlace,
    /// The command palette and its last-used order; session state, never document data.
    palette: Palette,
    /// The product-frame card: window chrome, like the File dialogs, so it outlives a tab switch.
    product_card: ProductCard,
    /// Why the defaults were used instead of the saved workspace, shown in the status bar.
    settings_notice: Option<crate::settings_store::Notice>,
    /// The pick the system file manager is answering, if one is in flight (ADR 0182).
    picker: Picker,
    /// The tab that asked for the pick in flight, so its answer lands there and not in whichever tab
    /// is in front when the file manager returns (ADR 0081).
    pick_tab: Option<TabId>,
    /// The close or the deletion that is waiting for the user's answer.
    pending_question: Option<PendingQuestion>,
    /// Whether the frame should close the window: the context that sends the command is only at
    /// hand there, and quitting is a window operation, not a document one.
    quit_requested: bool,
    /// The local control channel an agent attaches to, while the settings let it listen.
    ///
    /// The channel owns the socket, the token file and its thread; dropping it closes the door and
    /// removes the file, so a closed window leaves nothing behind for a client to reach (ADR 0188).
    channel: Option<mcp::Channel>,
    /// What the channel queued since the last frame: the frame answers it before anything else.
    mailbox: Option<mcp::Mailbox>,
    /// The calls taken off the mailbox and not answered yet.
    ///
    /// A frame owns the list while it answers it, because an answer borrows the document the call is
    /// about; the queue itself is the channel's.
    queued: Vec<mcp::Pending>,
    /// The number of the frame the window is drawing, counted from one.
    ///
    /// The request path names it when it answers a call, so a person reading the log can tell which
    /// frame landed what an agent asked for — the frame a call waits for is the one that shows it
    /// (X111).
    frames: u64,
    /// Whether the assistant panel is on screen.
    ///
    /// The panel is one window over the workspace, so whether it is up, where it sits and how wide
    /// it is are the shell's; what it says is the active tab's ([`Document::chat`]). The header's
    /// cell and `Ctrl+J` are one act and both write this flag (X106, ADR 0222, J24).
    assistant_open: bool,
    /// The material window: the workshop's render materials and the one record it edits (Y14).
    ///
    /// It is a floating tool of the shell, opened from the render place, so it outlives a tab switch
    /// exactly as the render place does; the library it shows is the one every tab draws with.
    material_window: ui::material_window::MaterialWindow,
    /// What the open documents say about the library: the nodes that name each record, and the ids
    /// they name that the library no longer holds.
    ///
    /// Derived, never stored in a document: it is what the window lists, and it is rebuilt only when
    /// a revision or a library snapshot moved, because a frame never walks a tree (`AGENTS.md` §2).
    material_use: ui::material_window::Use,
    /// The revision and library version of every tab the map above was built from.
    material_use_key: Vec<(u64, u64)>,
    /// The decor picture of the record the window edits, decoded once per record and path.
    material_preview: Option<(materials::MaterialId, String, egui::ColorImage)>,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Result<Self, String> {
        let render_state = cc
            .wgpu_render_state
            .clone()
            .ok_or_else(|| format!("{} requires the wgpu renderer", theme::PRODUCT_NAME))?;
        let settings_store = SettingsStore::for_workspace();
        let loaded = settings_store.load();
        let (preference, preset, accent) = loaded.settings.install_args();
        theme::install_with(&cc.egui_ctx, preference, preset, accent);
        cc.egui_ctx.set_zoom_factor(ui_scale::factor(loaded.settings.appearance.ui_scale));
        Self::compose(&cc.egui_ctx, Some(render_state), settings_store, loaded)
    }

    /// Composes the window around the workshop's directory: the rows a session starts from, and the
    /// worker that reloads them when the folder changes underneath the editor.
    ///
    /// The folder is read once, on the startup path, and watched from there: the stamp is taken
    /// before the snapshot, so a write that lands between them is a reload and not a lost edit. A
    /// directory that could not be read keeps the starter rows and says so in the status bar, which
    /// is the one place the window reports a workspace-level failure.
    fn compose(
        ctx: &egui::Context,
        render_state: Option<egui_wgpu::RenderState>,
        settings_store: SettingsStore,
        loaded: crate::settings_store::Loaded,
    ) -> Result<Self, String> {
        let root = settings_store.directory_root();
        let start = directory_watch::open(root.as_deref());
        let directory_watch = directory_watch::Watch::start(root, start.revision);
        let store = settings_store.materials();
        let materials = materials_watch::open(store.as_ref());
        let materials_watch = materials_watch::Watch::start(store, materials.revision);
        let session = empty_session_with(start.directory)?.with_materials(materials.library);
        let watching = Watching { directory: directory_watch, materials: materials_watch };
        let mut app = Self::build(ctx, render_state, settings_store, loaded, session, watching)?;
        app.apply_channel(ctx);
        if let Some(refusal) = start.refusal {
            log::warn!("the workshop directory could not be read; the starter rows are in use: {refusal}");
            app.failed(refusal);
        }
        if let Some(refusal) = materials.refusal {
            log::warn!("the workshop material library could not be read; no material is named: {refusal}");
            app.failed(refusal);
        }
        Ok(app)
    }

    /// Builds the application without a window and without a GPU.
    ///
    /// A test's [`egui::Context`] stands in for the window and `settings_path` for the workspace
    /// file, so the real frame runs against the real interface while nothing is drawn into and no
    /// file in the user's config directory is read or written (U77).
    #[cfg(test)]
    fn headless(
        ctx: &egui::Context,
        settings_path: std::path::PathBuf,
        settings: Settings,
        session: engine::Session,
    ) -> Result<Self, String> {
        Self::test_app(ctx, None, settings_path, settings, session)
    }

    /// Builds the application for a test: no window, and a GPU only when one was handed over.
    ///
    /// A headless frame test passes `None` and draws no scene image; a shot passes a device and
    /// queue with no surface, so the real viewport renders into its target and egui paints the same
    /// interface a window would show, offscreen (U77, U81).
    #[cfg(test)]
    fn test_app(
        ctx: &egui::Context,
        render_state: Option<egui_wgpu::RenderState>,
        settings_path: std::path::PathBuf,
        settings: Settings,
        session: engine::Session,
    ) -> Result<Self, String> {
        let (preference, preset, accent) = settings.install_args();
        theme::install_with(ctx, preference, preset, accent);
        ctx.set_zoom_factor(ui_scale::factor(settings.appearance.ui_scale));
        let loaded = crate::settings_store::Loaded { settings, notice: None };
        let settings_store = SettingsStore::at(settings_path);
        // A test drives the same worker the window drives, over the folder beside its own settings
        // file. The stamp is taken now, so the folder the test wrote before it built the shell is
        // the state the shell starts from and only a later write is a reload (O5).
        let root = settings_store.directory_root();
        let directory = directory_watch::Watch::start(root.clone(), directory_watch::open(root.as_deref()).revision);
        // The library beside the test's own settings file, watched the same way the window watches
        // the workshop's: a test that replaces a picture drives the same reload path (Y8).
        let store = settings_store.materials();
        let materials = materials_watch::Watch::start(store.clone(), materials_watch::open(store.as_ref()).revision);
        Self::build(ctx, render_state, settings_store, loaded, session, Watching { directory, materials })
    }

    /// Composes the shell around one document, one workspace and, when the window has one, its GPU.
    ///
    /// This is the one place the window and a headless test part ways: everything below it — the
    /// frame, the chrome, the keys, the documents — runs the same code (U77).
    fn build(
        ctx: &egui::Context,
        render_state: Option<egui_wgpu::RenderState>,
        settings_store: SettingsStore,
        loaded: crate::settings_store::Loaded,
        session: engine::Session,
        watching: Watching,
    ) -> Result<Self, String> {
        let settings = loaded.settings;
        let shell = Shell {
            panels: Panels::from_settings(&settings.panels),
            colors: theme::scene_colors(ctx.theme()),
            ..Shell::default()
        };
        let document = Document::new(
            render_state.as_ref(),
            &settings,
            &shell,
            plan_mode(session),
            PreservedSections::new(),
            settings_store.materials(),
        )?;
        Ok(Self {
            tabs: Tabs::new(document),
            render_state,
            shell,
            settings,
            settings_store,
            watching,
            settings_section: SettingsSection::default(),
            assistant_models: model_list::Models::default(),
            settings_open: false,
            home_open: false,
            render_open: false,
            render_place: render_place::RenderPlace::new(),
            palette: Palette::new(),
            product_card: ProductCard::new(),
            settings_notice: loaded.notice,
            picker: Picker::new(),
            pick_tab: None,
            pending_question: None,
            quit_requested: false,
            channel: None,
            mailbox: None,
            queued: Vec::new(),
            frames: 0,
            assistant_open: false,
            material_window: ui::material_window::MaterialWindow::new(),
            material_use: ui::material_window::Use::default(),
            material_use_key: Vec::new(),
            material_preview: None,
        })
    }

    /// The active tab's assistant state: the conversation, the backend and the turn.
    ///
    /// Every door the assistant panel has reads the tab that is in front, so a message is always
    /// the front scene's and never a neighbour's (J24).
    fn chat(&self) -> &crate::chat::Chat {
        &self.tabs.active().chat
    }

    /// The active tab's assistant state, mutably.
    fn chat_mut(&mut self) -> &mut crate::chat::Chat {
        &mut self.tabs.active_mut().chat
    }

    /// Shows the assistant panel, or puts it away without touching any tab's conversation.
    ///
    /// The header's cell and `Ctrl+J` are one act, so both ask here rather than each writing the
    /// flag: a panel that opens from two doors and closes from one is the drift the inventory ends
    /// (X106, ADR 0222). Opening asks the panel for the caret: a person who opens a field means to
    /// type into it, and the panel is one field and one turn (X122).
    fn toggle_assistant(&mut self) {
        self.assistant_open = !self.assistant_open;
        if self.assistant_open {
            self.chat_mut().focus_field();
        }
    }

    /// Reports a failure in the active tab's status bar.
    ///
    /// An error is the tab's: a scene that could not be evaluated says so on its own status bar and
    /// leaves the neighbours alone.
    fn failed(&mut self, error: String) {
        self.tabs.active_mut().last_error = Some(error);
    }

    /// Draws the material window and performs the one act it asked for (Y14).
    ///
    /// The window writes nothing: an act is filed here — the record goes into the library, the whole
    /// library is written through its own store and swapped into every tab — and the sentence a
    /// refusal carries goes back into the window, which keeps the fields the person typed. A picture
    /// is the file manager's answer and the store's own copy; the shell only carries the path.
    fn material_window_frame(&mut self, ctx: &egui::Context) {
        if !self.material_window.is_open() {
            return;
        }
        let library = self.tabs.active().session.materials_shared();
        self.refresh_material_use(&library);
        self.refresh_material_preview(&library);
        let preview = self.material_preview.as_ref().map(|(material, picture, image)| ui::material_window::Preview {
            material: *material,
            picture,
            image,
        });
        let act =
            ui::material_window::show(ctx, &mut self.material_window, library.as_ref(), &self.material_use, preview);
        match act {
            Some(ui::material_window::Act::Set(material)) => self.file_material(*material),
            Some(ui::material_window::Act::Remove(material)) => self.remove_material(material),
            Some(ui::material_window::Act::Pick(_)) => self.ask_for_picture(ctx),
            None => {}
        }
    }

    /// Rebuilds what the documents say about the library, when a revision or a snapshot moved.
    ///
    /// The map is the window's list of users and of ids that are gone; it is derived, so it is rebuilt
    /// only when something it is derived from changed, and never on a frame that changed nothing.
    fn refresh_material_use(&mut self, library: &materials::Library) {
        let key: Vec<(u64, u64)> = self
            .tabs
            .iter()
            .map(|(_, document)| (document.session.project().revision().get(), document.session.materials_version()))
            .collect();
        if key == self.material_use_key {
            return;
        }
        let mut used = ui::material_window::Use::default();
        for (_, document) in self.tabs.iter() {
            for (reference, nodes) in document.session.project().nodes_by_material() {
                let Some(material) = materials::MaterialId::new(reference.get()) else {
                    continue;
                };
                let users = used.named.entry(material).or_default();
                users.extend(nodes.into_iter().map(|(_, name)| name.to_owned()));
            }
        }
        used.missing = used.named.keys().copied().filter(|id| library.get(*id).is_none()).collect();
        self.material_use = used;
        self.material_use_key = key;
    }

    /// Decodes the picture the record in the fields is faced with, for the swatch.
    ///
    /// One picture per record and per path: a scan a shop replaced is the path's own new picture, and
    /// a record the window leaves takes its picture with it. `ui` never reads a file, so the decode
    /// is here, beside every other read of the library's folder (ADR 0244).
    fn refresh_material_preview(&mut self, library: &materials::Library) {
        let Some(editing) = self.material_window.editing() else {
            self.material_preview = None;
            return;
        };
        let Some(record) = library.get(editing) else {
            self.material_preview = None;
            return;
        };
        let Some(picture) = record.base().and_then(materials::Base::decor).map(|decor| decor.picture().to_owned())
        else {
            self.material_preview = None;
            return;
        };
        if self.material_preview.as_ref().is_some_and(|(held, path, _)| *held == editing && *path == picture) {
            return;
        }
        self.material_preview = None;
        let Some(store) = self.settings_store.materials() else {
            return;
        };
        match store.picture(record) {
            Ok(Some(decoded)) => {
                let size = [decoded.width() as usize, decoded.height() as usize];
                let image = egui::ColorImage::from_rgba_unmultiplied(size, decoded.rgba8());
                self.material_preview = Some((editing, picture, image));
            }
            Ok(None) => {}
            Err(refusal) => {
                log::warn!("the decor of material {editing} could not be read: {refusal}");
                self.material_window.refused(refusal.to_string());
            }
        }
    }

    /// Files one record into the workshop's library: one atomic write, then every tab draws with it.
    fn file_material(&mut self, material: materials::Material) {
        let records: Vec<materials::Material> = self
            .tabs
            .active()
            .session
            .materials()
            .records()
            .filter(|record| record.id() != material.id())
            .cloned()
            .chain([material.clone()])
            .collect();
        if self.write_materials(records).is_some() {
            self.material_window.filed(material.id());
        }
    }

    /// Removes one record, leaving every reference to it in the documents (ADR 0251).
    fn remove_material(&mut self, material: materials::MaterialId) {
        let records: Vec<materials::Material> = self
            .tabs
            .active()
            .session
            .materials()
            .records()
            .filter(|record| record.id() != material)
            .cloned()
            .collect();
        if self.write_materials(records).is_some() {
            self.material_window.forget(material);
            self.material_preview = None;
        }
    }

    /// Writes a library the window built and hands it to every tab, or says why it could not.
    ///
    /// The whole library is validated before anything is written, so a second record named `Oak` or
    /// an id two records claim leaves the file exactly as it was; the write itself is the store's
    /// atomic one, so a crash during it cannot leave half a library behind. A snapshot the frame
    /// could not read is the window's own sentence, and the records in use stay where they are.
    fn write_materials(&mut self, records: Vec<materials::Material>) -> Option<std::sync::Arc<materials::Library>> {
        let library = match materials::Library::new(records) {
            Ok(library) => library,
            Err(refusal) => {
                self.material_window.refused(refusal.to_string());
                return None;
            }
        };
        let Some(store) = self.settings_store.materials() else {
            let lang = self.settings.appearance.language.lang();
            self.material_window.refused(text::Message::MaterialsNoFolder.text(lang));
            return None;
        };
        if let Err(refusal) = store.write(&library) {
            self.material_window.refused(refusal.to_string());
            return None;
        }
        let library = std::sync::Arc::new(library);
        for (_, document) in self.tabs.iter_mut() {
            document.set_materials(std::sync::Arc::clone(&library));
        }
        // The watcher sees the stamp move and hands the same library back; the tab then rebuilds one
        // frame it already has, which costs a frame and never a wrong picture.
        Some(library)
    }

    /// Asks the system file manager for a decor picture, starting in the library's own folder.
    fn ask_for_picture(&mut self, ctx: &egui::Context) {
        let lang = ui::language(ctx);
        let folder = self.settings_store.materials().map(|store| store.textures());
        let order = file_picker::Order { mode: PickMode::Picture, folder, name: None, lang };
        if let Err(error) = self.picker.start(ctx, order) {
            self.material_window.refused(error);
        }
    }

    /// Copies the picture the person chose beside the library and names it on the record.
    fn take_material_picture(&mut self, path: std::path::PathBuf) {
        let Some(store) = self.settings_store.materials() else {
            return;
        };
        match store.import_picture(&path) {
            Ok(named) => {
                self.material_preview = None;
                self.material_window.picture(named);
            }
            Err(refusal) => self.material_window.refused(refusal.to_string()),
        }
    }

    /// Swaps in the directory the worker finished since the last frame.
    ///
    /// The folder is the workshop's, so every open tab takes the same snapshot in the same frame,
    /// and each tab re-derives only what its own cache says changed. Nothing here writes a document:
    /// a directory update is not an edit, and a scene that was dirty before it is dirty after it
    /// (`docs/domain/nomenclature.md` §13). A revision that could not be read keeps the rows in use
    /// and says why in the status bar.
    fn take_directory(&mut self) {
        while let Some(reload) = self.watching.directory.take() {
            match reload {
                directory_watch::Reload::Loaded(directory) => {
                    for (_, document) in self.tabs.iter_mut() {
                        document.set_directory(Arc::clone(&directory));
                    }
                }
                directory_watch::Reload::Refused(refusal) => {
                    log::warn!("the workshop directory could not be read; the rows in use stay: {refusal}");
                    self.failed(refusal);
                }
            }
        }
    }

    /// Swaps in the material library the worker finished since the last frame.
    ///
    /// The library is the workshop's, so every open tab takes the same snapshot in the same frame,
    /// and each tab re-derives only its frame: no panel, its geometry or the undo history depends on
    /// a material (`AGENTS.md` §4.2). A library that could not be read keeps the records in use and
    /// says why in the status bar; a replaced picture arrives here as a new snapshot, which is what
    /// makes the new scan reach the screen without a restart.
    fn take_materials(&mut self) {
        while let Some(reload) = self.watching.materials.take() {
            match reload {
                materials_watch::Reload::Loaded(library) => {
                    for (_, document) in self.tabs.iter_mut() {
                        document.set_materials(Arc::clone(&library));
                    }
                }
                materials_watch::Reload::Refused(refusal) => {
                    log::warn!("the workshop material library could not be read; the records in use stay: {refusal}");
                    self.failed(refusal);
                }
            }
        }
    }

    /// Performs the gesture the file manager answered, or nothing when the pick was cancelled.
    ///
    /// A cancelled pick writes and opens nothing: the person closed the dialog, which is an answer
    /// and not a failure, so the status bar stays quiet (ADR 0182).
    fn take_pick(&mut self, ctx: &egui::Context) {
        let Some((order, answer)) = self.picker.take() else {
            return;
        };
        // A render's save belongs to the place, not to a tab: the answer is where the kept image is
        // written, and the page keeps drawing while the file manager is up (ADR 0182, Y12).
        if order.mode == PickMode::Image {
            if let Some(path) = answer {
                self.render_place.save(path);
            }
            return;
        }
        // A decor picture belongs to the material window, which is the shell's and not a tab's.
        if order.mode == PickMode::Picture {
            if let Some(path) = answer {
                self.take_material_picture(path);
            }
            return;
        }
        // The pick is the asking tab's (ADR 0081): its answer lands there even when another tab was
        // activated behind the file manager's own window.
        let Some(asking) = self.pick_tab.take() else {
            return;
        };
        let Some(path) = answer else {
            return;
        };
        if self.tabs.document(asking).is_none() {
            // The tab closed under the dialog: there is nothing left to save or to open into.
            return;
        }
        self.tabs.activate(asking);
        let action = match order.mode {
            PickMode::Open => FileAction::Open(path),
            // The dialog filters by the scene extension but does not append it, so a name typed
            // without one becomes a file the next *Open* can see.
            PickMode::Save => FileAction::Save(scene_file(path)),
            // The editor never asks for a price list: that pick belongs to the directory application.
            PickMode::Prices | PickMode::Image | PickMode::Picture => {
                // Both are answered before this point: the price list belongs to the directory
                // application, and an image to the render place.
                log::warn!("a pick that is not a scene reached the tab that asked for it");
                return;
            }
        };
        self.handle_file_action(action, ctx);
    }

    /// Performs the header action the row returned.
    ///
    /// Undo, redo and save are the same operations the menu and the keyboard already take, and every
    /// one of them acts on the active tab only, so the header adds a place to click and never a
    /// second code path into a document.
    fn handle_header_action(&mut self, action: HeaderAction, ctx: &egui::Context) {
        let lang = ui::language(ctx);
        match action {
            HeaderAction::Home => {
                self.home_open = true;
                self.settings_notice = None;
            }
            HeaderAction::Undo => self.tabs.active_mut().undo(),
            HeaderAction::Redo => self.tabs.active_mut().redo(),
            HeaderAction::Save => {
                let requested = self.tabs.active_mut().request_save(lang);
                if let Some(action) = requested {
                    self.handle_file_action(action, ctx);
                }
            }
            HeaderAction::Settings => {
                self.settings_open = true;
                self.settings_notice = None;
            }
            // The assistant's cell and its key are one act: the panel is the shell's, and the mode
            // switch inside it is the person's (ADR 0190, ADR 0222).
            HeaderAction::Assistant => self.toggle_assistant(),
            HeaderAction::CollapsePanels => self.shell.panels.toggle_collapse_all(),
            HeaderAction::HidePanels => self.shell.panels.toggle_hide_all(),
            HeaderAction::ToggleRibbon => {
                let open = !self.tabs.active().ribbon_open();
                self.tabs.active_mut().set_ribbon_open(open);
                self.sync_ribbon_fold();
            }
            // A gesture is the window's view state: the band arms it for every tab and no document
            // is written (ADR 0113, U115).
            HeaderAction::SetMode(mode) => self.shell.mode = mode,
            HeaderAction::NewTab => {
                if let Err(error) = self.new_tab() {
                    self.failed(error);
                }
            }
            HeaderAction::ActivateTab(id) => {
                self.tabs.activate(id);
                // A document tab is the way back from the render place: the place is a view of the
                // scene in front, never a replacement for it (ADR 0211).
                self.render_open = false;
            }
            HeaderAction::Render => self.render_open = !self.render_open,
            HeaderAction::CloseTab(id) => self.close_tab(id, lang),
        }
    }

    /// Performs the operation the File menu returned and reports a failure in the status bar.
    fn handle_file_action(&mut self, action: FileAction, ctx: &egui::Context) {
        let lang = ui::language(ctx);
        self.tabs.active_mut().last_error = None;
        let recorded = match &action {
            FileAction::Open(path) | FileAction::Save(path) => Some(path.clone()),
            FileAction::New | FileAction::Pick(_) | FileAction::Close | FileAction::Quit => None,
        };
        let result = match action {
            FileAction::New => self.new_tab(),
            FileAction::Open(path) => self.open_tab(&path),
            FileAction::Save(path) => self.tabs.active_mut().save(&path),
            // The file manager is asked, not waited for: the window keeps drawing and the answer
            // lands at the next frame boundary (ADR 0182).
            FileAction::Pick(order) => match self.picker.start(ctx, order) {
                Ok(Started::Asked) => {
                    self.pick_tab = Some(self.tabs.active_id());
                    Ok(())
                }
                Ok(Started::Busy) => Ok(()),
                Err(_) => Err(text::Message::PickFailed.text(lang).to_owned()),
            },
            FileAction::Close => {
                let active = self.tabs.active_id();
                self.close_tab(active, lang);
                Ok(())
            }
            FileAction::Quit => {
                self.request_quit(lang);
                Ok(())
            }
        };
        match result {
            Ok(()) => {
                if let Some(path) = recorded {
                    self.settings.remember(&path);
                    self.persist_settings();
                }
            }
            Err(error) => self.failed(error),
        }
    }

    /// Opens a scene in a new tab, or activates the tab that already holds it.
    ///
    /// The owner's rule: opening a path that is already open activates that tab instead of making a
    /// second copy of one file, so two tabs can never race to save the same scene. The catalogue the
    /// new session gets is the workspace's, taken from the scene in front of the user.
    fn open_tab(&mut self, path: &Path) -> Result<(), String> {
        if let Some(id) = self.tabs.find(|document| document.path() == Some(path)) {
            self.tabs.activate(id);
            return Ok(());
        }
        let catalog = self.tabs.active().session.directory_shared();
        // A tab opened beside another one opens on the same shared snapshots: the directory the
        // window holds and the materials every tab is drawn with (Y8, Y7).
        let library = self.tabs.active().session.materials_shared();
        let pictures = self.settings_store.materials();
        let document =
            Document::open(self.render_state.as_ref(), &self.settings, &self.shell, path, catalog, library, pictures)?;
        self.tabs.add(document);
        Ok(())
    }

    /// Opens an empty *Untitled* scene in a new tab.
    fn new_tab(&mut self) -> Result<(), String> {
        let library = self.tabs.active().session.materials_shared();
        let session = empty_session_with(self.tabs.active().session.directory_shared())?.with_materials(library);
        let settings = &self.settings;
        let shell = &self.shell;
        let pictures = self.settings_store.materials();
        let document =
            Document::new(self.render_state.as_ref(), settings, shell, session, PreservedSections::new(), pictures)?;
        self.tabs.add(document);
        Ok(())
    }

    /// Closes one tab, asking first when its scene has unsaved changes.
    ///
    /// The last tab is never removed: its payload becomes an empty scene, so the window always has
    /// one document to draw and the folds, the cluster and the settings keep their home.
    fn close_tab(&mut self, id: TabId, lang: text::Lang) {
        let unsaved = self.tabs.document(id).is_some_and(Document::is_dirty);
        match self.tabs.close(id, unsaved) {
            CloseOutcome::NeedsConfirmation(id) => {
                let title = self.tabs.document(id).map(|document| document.file_name(lang)).unwrap_or_default();
                self.pending_question =
                    Some(PendingQuestion { gesture: Gesture::Tab(id), question: Confirm::tab(title, lang) });
            }
            CloseOutcome::Closed(document) => self.forget(document),
            CloseOutcome::LastTab(id) => self.empty_tab(id),
            CloseOutcome::Missing(_) => {}
        }
    }

    /// Asks to quit when any open scene has unsaved changes, and quits at once when none has.
    ///
    /// One question covers the window: a quit is one gesture, and asking per tab would leave the
    /// user to answer the same question several times for one decision.
    fn request_quit(&mut self, lang: text::Lang) {
        let unsaved = self.tabs.iter().filter(|(_, document)| document.is_dirty()).count();
        match unsaved {
            0 => self.quit_requested = true,
            count => {
                self.pending_question =
                    Some(PendingQuestion { gesture: Gesture::Quit, question: Confirm::quit(count, lang) });
            }
        }
    }

    /// Answers the Delete key over the active document's selection.
    ///
    /// One node goes at once, because undo covers it; two or more ask first, because a batch is
    /// removed in one step and the question says how many rows it takes. A locked node refuses the
    /// whole gesture before either happens, with the sentence the tree shows for a locked row
    /// (ADR 0065, ADR 0112).
    fn delete_selection(&mut self, lang: text::Lang) {
        let request = {
            let document = self.tabs.active();
            delete_request(&document.editor, document.session.project(), lang)
        };
        match request {
            DeleteRequest::Nothing => {}
            DeleteRequest::Refused(sentence) => self.tabs.active_mut().last_error = Some(sentence),
            DeleteRequest::Remove(nodes) => self.tabs.active_mut().delete_nodes(&nodes),
            DeleteRequest::Ask(nodes) => {
                let tab = self.tabs.active_id();
                let question = Confirm::delete(nodes.len(), lang);
                self.pending_question = Some(PendingQuestion { gesture: Gesture::Delete { tab, nodes }, question });
            }
        }
    }

    /// Draws the pending question and performs the gesture the answer names.
    ///
    /// An answer is about the gesture the question was asked over, not about what is in front of the
    /// user: a discard drops exactly the tab it named, and a deletion removes exactly the nodes it
    /// counted, in the tab it was asked in.
    fn answer_question(&mut self, ctx: &egui::Context) {
        let Some(pending) = self.pending_question.as_ref() else {
            return;
        };
        let Some(answer) = pending.question.show(ctx) else {
            return;
        };
        let Some(pending) = self.pending_question.take() else {
            return;
        };
        if answer == Answer::Cancel {
            return;
        }
        match pending.gesture {
            Gesture::Quit => self.quit_requested = true,
            Gesture::Tab(tab) => match self.tabs.discard(tab) {
                CloseOutcome::Closed(document) => self.forget(document),
                CloseOutcome::LastTab(id) => self.empty_tab(id),
                CloseOutcome::NeedsConfirmation(_) | CloseOutcome::Missing(_) => {}
            },
            Gesture::Delete { tab, nodes } => {
                if let Some(document) = self.tabs.document_mut(tab) {
                    document.delete_nodes(&nodes);
                }
            }
        }
    }

    /// Frees the render target of a closed document.
    ///
    /// The texture is registered on the window's egui renderer and outlives the document that drew
    /// into it, so a workspace that opens and closes scenes has to hand it back; the document then
    /// drops with its own render pass, buffers and cache.
    fn forget(&mut self, document: Document) {
        document.release();
    }

    /// Replaces the last tab with an empty *Untitled* scene.
    fn empty_tab(&mut self, id: TabId) {
        let directory = self.tabs.active().session.directory_shared();
        let library = self.tabs.active().session.materials_shared();
        let pictures = self.settings_store.materials();
        let built = empty_session_with(directory).and_then(|session| {
            Document::new(
                self.render_state.as_ref(),
                &self.settings,
                &self.shell,
                plan_mode(session.with_materials(library)),
                PreservedSections::new(),
                pictures,
            )
        });
        match built {
            Ok(document) => {
                if let Some(closed) = self.tabs.replace(id, document) {
                    self.forget(closed);
                }
            }
            Err(error) => self.failed(error),
        }
    }

    /// Opens or closes the local control channel, so the settings switch is the door (ADR 0188).
    ///
    /// The channel listens only while the window is open and only while the workspace allows it: a
    /// switch pressed in the Settings window takes effect on that frame, and a closed window removes
    /// the token file a client would have read. The context is the one thing a channel cannot know
    /// (`mcp` is forbidden `egui`), so the frame hands it in as the wake a queued call rings (X111).
    fn apply_channel(&mut self, ctx: &egui::Context) {
        if !self.settings.assistant.attach {
            if let Some(channel) = self.channel.take() {
                log::info!(target: "z3l::shell", "the control channel on port {} is closed", channel.port());
            }
            self.mailbox = None;
            self.queued.clear();
            return;
        }
        if self.channel.is_some() || self.mailbox.is_some() {
            return;
        }
        let Some(folder) = self.settings_store.folder().map(std::path::Path::to_path_buf) else {
            log::info!(target: "z3l::shell", "no settings folder is known, so no agent can reach this window");
            return;
        };
        self.attach_channel(&folder, ctx);
    }

    /// Opens the channel in `folder`, keeping the window when it cannot be opened.
    ///
    /// A second window is the ordinary case, not an error a person has to solve: the first window's
    /// channel is the one a client reaches, and this one stays without a door and says so in the log
    /// (ADR 0188).
    ///
    /// The mailbox is told what to do the moment a call arrives: ask this window for a frame. A
    /// window paints on input, so a call that lands while the person is not touching it would
    /// otherwise wait for a pointer that may never come (X111, ADR 0225).
    fn attach_channel(&mut self, folder: &Path, ctx: &egui::Context) -> bool {
        match mcp::Channel::open(folder) {
            Ok((channel, mailbox)) => {
                log::info!(
                    target: "z3l::shell",
                    "an agent may reach this window on 127.0.0.1:{}; the token is in {}",
                    channel.port(),
                    channel.published_at().display()
                );
                let window = ctx.clone();
                mailbox.watch(std::sync::Arc::new(move || window.request_repaint()));
                self.channel = Some(channel);
                self.mailbox = Some(mailbox);
                true
            }
            Err(error) => {
                log::warn!(target: "z3l::shell", "{error}");
                false
            }
        }
    }

    /// Answers every call the channel queued, at the frame boundary.
    ///
    /// The queue is taken before anything this frame derives from it, so a command lands between
    /// frames and never inside a gesture, and the scene the person sees on this frame is the scene
    /// the agent left (ADR 0187). Two rules keep the answer honest (X107, ADR 0223):
    ///
    /// * a call that moves the document waits for the first frame that has the window to itself — a
    ///   piece in hand or a question waiting for the person delays the call instead of refusing it,
    ///   because a refusal says nothing about the edit that a late frame would perform;
    /// * a call whose caller has gone is dropped before anything reads it, so no change lands for a
    ///   caller that gave up at the channel's deadline and will never hear about it;
    /// * the frame that answers asks for the frame after it, and the call rings the window the
    ///   moment it is queued (`Mailbox::watch`), so nothing here waits for a pointer a person has to
    ///   bring (X111, ADR 0225).
    ///
    /// Reads are answered whatever the window is doing: a question about the document moves nothing.
    fn serve_channel(&mut self, ctx: &egui::Context) {
        if let Some(mailbox) = self.mailbox.as_mut() {
            self.queued.extend(mailbox.drain());
        }
        let mut held = Vec::new();
        let mut moved = false;
        for pending in std::mem::take(&mut self.queued) {
            if pending.abandoned() {
                log::info!(
                    target: "z3l::shell",
                    "a call was dropped: the client gave up before the window reached it"
                );
                continue;
            }
            if self.gesture_in_flight() && matches!(pending.call(), mcp::Call::Dispatch(_)) {
                held.push(pending);
                continue;
            }
            moved |= matches!(pending.call(), mcp::Call::Dispatch(_));
            let answer = self.answer_channel(pending.call());
            // The answer reaches the caller inside the frame and before the frame is painted, so
            // the frame it names is the one that landed the change (X111).
            log::info!(
                target: "z3l::shell",
                "the window answered a {} in frame {}",
                pending.call().word(),
                self.frames
            );
            pending.answer(answer);
        }
        self.queued = held;
        if moved || !self.queued.is_empty() {
            // A call that landed asks for the frame that shows it, and a held one asks for the frame
            // that can answer it: asking here is what keeps an agent's call from waiting for a
            // pointer that may never come (ADR 0223, X111).
            ctx.request_repaint();
        }
    }

    /// Answers one call against the document a person is looking at.
    ///
    /// [`Self::serve_channel`] decides what may be answered in this frame: a dispatch is only handed
    /// here by a frame that has the window to itself (ADR 0187, ADR 0223).
    fn answer_channel(&mut self, call: &mcp::Call) -> mcp::Answer {
        match call {
            mcp::Call::List(request) => mcp::listing(&self.tabs.active().session, request.command.as_deref()),
            mcp::Call::Ask(query) => mcp::asking(&self.tabs.active().session, *query),
            // Undoing is the person's own Ctrl+Z, performed by the tab whose history holds the entry
            // a caller is taking back; the session's dirty marker follows it, so the tab row says the
            // document moved (ADR 0190, ADR 0230).
            mcp::Call::Undo => mcp::undo(&mut self.tabs.active_mut().session),
            mcp::Call::Findings => mcp::Answer::ok(mcp::findings(&self.tabs.active().session)),
            mcp::Call::Inspect => match serde_json::to_value(self.window_state()) {
                Ok(value) => mcp::Answer::ok(value),
                // A window state is plain data by construction; a caller that reaches this arm is
                // told so instead of being handed a value that is not the window.
                Err(error) => mcp::Answer::refused(format!("the window cannot be reported: {error}")),
            },
            mcp::Call::Preview(request) => {
                // The picture is drawn with the device the window already owns, and the framing is
                // the document's, so the picture a caller reads is the one a headless process draws
                // (ADR 0228).
                let previewer = self.render_state.clone().map(preview::ScenePreview::new);
                mcp::preview(&self.tabs.active().session, request, previewer.as_ref().map(|p| p as &dyn mcp::Previewer))
            }
            mcp::Call::Dispatch(request) => {
                // A batch that arrives while a chat turn is open lands in the turn's own edit scope,
                // so a process that edits in several calls of one message leaves one undo entry
                // (ADR 0187, ADR 0230). The scope is read before the document is borrowed.
                let scope = self.chat().scope();
                let document = self.tabs.active_mut();
                // The save a caller asks for is the person's *Save* performed on their document, so
                // it goes through the same writer and to the same file (X114). A scene that was
                // never saved has no file yet, and the door says so instead of choosing a path.
                let writer = document.save_sink();
                let sink = writer.as_ref().map(|writer| writer as &dyn mcp::SceneSink);
                let answer = mcp::dispatch(&mut document.session, request, mcp::Door::Window { sink, scope });
                if answer.written() {
                    document.mark_saved();
                }
                answer
            }
        }
    }

    /// Whether a gesture holds the window, so a command would land in the middle of it.
    ///
    /// A dispatch happens between frames and so can never interleave with a gizmo drag; the two
    /// gestures that outlive a frame are a piece carried by the pointer and a question waiting for
    /// the person's answer, and both mean "not now" rather than "move the document" (ADR 0187).
    fn gesture_in_flight(&self) -> bool {
        self.pending_question.is_some() || self.tabs.active().holding_piece()
    }

    /// Gives every tab the model the workspace names, and takes it away when it names none.
    ///
    /// Two ways to one loop (ADR 0189, ADR 0230), and the process wins: a command the workshop typed
    /// starts an agent that owns the loop itself, and the endpoint is the OpenAI-compatible wire the
    /// panel asks only while that command is empty. Both are settings and the channel is the
    /// window's, so the two are read every frame and every tab is handed the answer; a backend this
    /// shell did not create — a test's script — is left alone.
    ///
    /// Every tab and not only the one in front: the backend is part of the tab's own conversation, so
    /// a settings change reaches the scenes that are not on screen and a tab that comes forward is
    /// already able to send (J24). A tab with a turn in flight keeps its backend until that turn is
    /// over, because the answer belongs to the process that is running it.
    ///
    /// The key is not read here: it belongs to the message that needs it, so no frame touches the
    /// file and the homes are named by the sentence a turn answers with when neither has one.
    fn apply_assistant(&mut self) {
        let assistant = self.settings.assistant.clone();
        let program = mcp::Program::parse(&assistant.agent);
        // A process wins the loop, so a wrong address matters only while no process is named: the
        // panel then answers the clause the settings page would have shown (ADR 0189).
        let problem = match (&program, assistant.problem()) {
            (None, Some(problem)) => Some(problem),
            _ => None,
        };
        let endpoint = match (program.clone(), assistant.endpoint_to_ask()) {
            (None, Some(base_url)) => Some(mcp::Endpoint::new(base_url, assistant.model.trim())),
            _ => None,
        };
        let folder = self.settings_store.folder().map(std::path::Path::to_path_buf);
        let channel_open = self.channel.is_some();
        for (_, document) in self.tabs.iter_mut() {
            document.chat.apply_agent(program.clone(), channel_open);
            document.chat.apply_problem(problem);
            document.chat.apply_model(endpoint.clone(), folder.clone());
        }
    }

    /// Drives the assistant's turn one step, at the frame boundary (ADR 0187).
    ///
    /// A turn asks the model for one reply per frame and dispatches at most one batch per frame, so
    /// the person's Stop lands between two calls and a drag is never interleaved with an answer. A
    /// gesture in flight pauses the turn instead of failing it: the next frame that has the window
    /// to itself carries on.
    ///
    /// Only the tab in front is served, so a turn in a scene the person is not looking at pauses
    /// where it stopped: its reply stays in flight in its own backend and resumes the frame its tab
    /// comes forward again, which is what makes switching tabs cost no answer (J24).
    fn serve_chat(&mut self, lang: text::Lang) {
        if self.gesture_in_flight() {
            return;
        }
        let document = self.tabs.active_mut();
        document.chat.serve(&mut document.session, lang);
    }

    /// Draws the assistant panel and performs the one action it asked for.
    ///
    /// The mode switch is the session's, so it survives a panel close and dies with the tab; leaving
    /// build ends a running turn at its next call boundary, because an answer that started while the
    /// agent could write must not keep writing after the person dropped to plan (ADR 0190). The
    /// rules' findings are re-read when the document moves, never per frame: the list is derived and
    /// the pass walks the tree (ADR 0191). The gesture is handed in so Send waits while a drag or a
    /// pending question has the window; whether the turn is running is the panel's own state, which
    /// is what draws Stop.
    fn assistant(&mut self, ctx: &egui::Context) {
        if !self.assistant_open {
            return;
        }
        let lang = ui::language(ctx);
        let mode = self.tabs.active().session.agent_mode();
        let held = self.gesture_in_flight();
        let open = &mut self.assistant_open;
        let document = self.tabs.active_mut();
        document.chat.refresh_findings(&document.session);
        match document.chat.show(ctx, open, mode, lang, held) {
            ChatAction::None => {}
            ChatAction::Send => {
                let text = self.chat().input.trim().to_owned();
                self.send_chat(text, lang);
            }
            ChatAction::Stop => self.chat_mut().stop(),
            ChatAction::AllowEndpoint => self.allow_endpoint(lang),
            ChatAction::DenyEndpoint => self.chat_mut().clear_consent(),
            ChatAction::Mode(mode) => {
                self.chat_mut().stop();
                self.tabs.active_mut().session.set_agent_mode(mode);
            }
            ChatAction::Close => self.assistant_open = false,
        }
    }

    /// Sends one message as one turn of the active tab's session, once the endpoint is agreed to.
    ///
    /// The question comes before the message: a workspace with an endpoint sends nothing until the
    /// person has seen what leaves the machine and said yes once for that endpoint (ADR 0189). The
    /// typed message stays in the field while the card is drawn, so "not now" costs nothing.
    fn send_chat(&mut self, text: String, lang: text::Lang) {
        // The key is read when a message is sent and not once per frame: a workspace whose key is
        // missing reads the sentence that names both homes instead of being asked a question about
        // a message that cannot go anywhere.
        if self.chat().asks_an_endpoint()
            && let Err(error) = mcp::key(self.settings_store.folder())
        {
            self.chat_mut().note(error.sentence().to_owned());
            return;
        }
        if let Some(endpoint) = self.chat().unconsented(&self.settings.assistant) {
            self.chat_mut().request_consent(endpoint);
            return;
        }
        let document = self.tabs.active_mut();
        document.chat.begin(&mut document.session, text, lang);
    }

    /// The person agreed to send to the endpoint the panel named: remember it, then send.
    ///
    /// The agreement is written now instead of when the settings window closes, because a promise
    /// that dies with a crash is a promise the program did not keep.
    fn allow_endpoint(&mut self, lang: text::Lang) {
        let Some(endpoint) = self.chat_mut().take_consent() else {
            return;
        };
        self.settings.assistant.remember_consent(&endpoint);
        self.persist_settings();
        let text = self.chat().input.trim().to_owned();
        self.send_chat(text, lang);
    }

    /// The window as `ui_inspect` reports it: the scenes, the one in front, the gesture and the
    /// selection, which are exactly the facts a headless session cannot have.
    fn window_state(&self) -> mcp::Window {
        let active = self.tabs.active_id();
        let mut index = 0;
        let mut tabs = Vec::new();
        for (position, (id, document)) in self.tabs.iter().enumerate() {
            if id == active {
                index = position;
            }
            tabs.push(mcp::Tab {
                title: document.file_name(text::Lang::En),
                path: document.path().map(|path| path.display().to_string()),
                dirty: document.is_dirty(),
                revision: document.session.project().revision().get(),
                product: document.session.active_product().map(model::Id::get),
            });
        }
        let document = self.tabs.active();
        mcp::Window {
            tabs,
            active: index,
            mode: self.shell.mode.id().to_owned(),
            selection: document.editor.selected_nodes().iter().map(|id| id.get()).collect(),
            busy: self.gesture_in_flight(),
            status: document.last_error.clone(),
        }
    }

    /// Writes the workspace settings and reports a failure in the status bar.
    ///
    /// The write is cheap and rare — a settings close, an opened or saved scene — so it happens at
    /// the moment the state changes instead of waiting for a shutdown hook a killed process can
    /// skip.
    fn persist_settings(&mut self) {
        match self.settings_store.save(&self.settings) {
            Ok(()) => self.settings_notice = None,
            Err(error) => self.failed(error),
        }
    }

    /// The ribbon over the design space: the tabs and the tool groups of the active tab.
    ///
    /// The strip floats on the middle layer instead of taking height from the viewport, so the scene
    /// image runs under it and a translucent ribbon blends with the furniture (U51, ADR 0080). The
    /// document draws it because the fold is the document's; the band it occupies is the window's.
    fn ribbon(&mut self, ui: &mut egui::Ui, design: egui::Rect) {
        if self.tabs.active_mut().show_ribbon(ui, design, &mut self.shell, &self.settings) {
            self.product_card.start();
        }
        self.sync_ribbon_fold();
        self.sync_pinned_variants();
    }

    /// Draws the product-frame card and creates the frame it describes.
    ///
    /// The card is the numeric path to `product.create`: the editor dispatches the command and
    /// opens the new frame as a session view operation, so the next part lands inside it (ADR 0100).
    /// A refused command leaves the card closed and the sentence on the status bar.
    fn product_frame_card(&mut self, ctx: &egui::Context) {
        let Some(draft) = self.product_card.show(ctx) else {
            return;
        };
        let lang = ui::language(ctx);
        if let Err(error) = self.tabs.active_mut().create_product(draft, lang) {
            self.failed(error);
        }
    }

    /// Remembers the ribbon's fold as a workspace setting and hands it to every open scene.
    ///
    /// The fold is chrome for the whole window rather than a property of one tab, so a tab that folds
    /// the ribbon folds it everywhere, and the next launch opens the way this one was left. The write
    /// happens on the one frame the fold changes, not per frame (U68).
    fn sync_ribbon_fold(&mut self) {
        let open = self.tabs.active().ribbon_open();
        if open == self.settings.ribbon.open {
            return;
        }
        self.settings.ribbon.open = open;
        // The measured band belongs to the fold it was drawn with. A remembered measurement would
        // keep the band of the state before — the strip's own height is a floor, so a folded ribbon
        // would hold the open band forever — and the fold would give the scene nothing (U78).
        self.shell.ribbon_rect = None;
        for (_, document) in self.tabs.iter_mut() {
            document.set_ribbon_open(open);
        }
        self.persist_settings();
    }

    /// Remembers the method the user pinned as a workspace setting and hands it to every open scene.
    ///
    /// The pin is global chrome, like the fold: one click in one tab changes the method every tab
    /// runs, and the next launch starts the same way. The write happens on the one frame the map
    /// changes, not per frame (U67).
    fn sync_pinned_variants(&mut self) {
        let active = self.tabs.active().pinned_variants();
        if *active == self.settings.ribbon.pinned_variants {
            return;
        }
        let pinned = active.clone();
        self.settings.ribbon.pinned_variants = pinned.clone();
        for (_, document) in self.tabs.iter_mut() {
            document.set_pinned_variants(pinned.clone());
        }
        self.persist_settings();
    }

    /// Remembers the panels' visibility and fold as workspace settings.
    ///
    /// The write happens on the one frame a panel gesture changes them, not per frame. The memory
    /// a collapse-all or hide-all press keeps stays in [`Panels`]; only the state the user sees is
    /// written, so a restart opens exactly the layout the window was left in.
    fn sync_panels(&mut self) {
        let panels = self.shell.panels;
        if self.settings.panels.scene == panels.scene && self.settings.panels.inspector == panels.inspector {
            return;
        }
        self.settings.panels.scene = panels.scene;
        self.settings.panels.inspector = panels.inspector;
        self.persist_settings();
    }

    /// The scene tree, as a floating panel over the left edge of the scene.
    ///
    /// The panel owns no heading and no tools of its own: the header row names it and folds it, and
    /// the body is only the tree. Grouping survives in the row context menu. The body scrolls through
    /// the same [`scroll_body`] the inspector uses, so both panels keep one inset and one route (X39).
    fn scene_tree(&mut self, ctx: &egui::Context, rect: egui::Rect) {
        let Self { shell, tabs, settings, .. } = self;
        let head = PanelHead { title: text::Message::PanelScene, icon: icons::scene::ASSEMBLY };
        let slot = PanelSlot { side: PanelSide::Left, id: "scene_tree", rect, state: shell.panels.scene, head };
        match show_panel(ctx, slot, settings.appearance.panel_opacity, |ui| {
            scroll_body(ui, |ui| tabs.active_mut().show_scene_tree(ui));
        }) {
            PanelEvent::Fold => shell.panels.scene.expanded = false,
            PanelEvent::Restore => shell.panels.scene.expanded = true,
            PanelEvent::None => {}
        }
    }

    /// The inspector, as a floating panel over the right edge of the scene.
    fn panel_card(&mut self, ctx: &egui::Context, rect: egui::Rect) {
        let Self { shell, tabs, settings, .. } = self;
        let head = PanelHead { title: text::Message::PanelInspector, icon: icons::actions::SETTINGS };
        let slot = PanelSlot { side: PanelSide::Right, id: "panel_card", rect, state: shell.panels.inspector, head };
        // The inspector's rows (the name field and its button, the property rows) are wider than a
        // 226 pt panel, so the body scrolls sideways instead of widening the panel (U73).
        match show_panel(ctx, slot, settings.appearance.panel_opacity, |ui| {
            scroll_body(ui, |ui| tabs.active_mut().show_card(ui));
        }) {
            PanelEvent::Fold => shell.panels.inspector.expanded = false,
            PanelEvent::Restore => shell.panels.inspector.expanded = true,
            PanelEvent::None => {}
        }
    }

    fn workspace(&mut self, ui: &mut egui::Ui, chrome: &Chrome) {
        egui::CentralPanel::default().show(ui, |ui| {
            let Self { tabs, shell, settings, .. } = self;
            let opacity = settings.appearance.panel_opacity;
            tabs.active_mut().viewport(ui, shell, settings, chrome, opacity);
        });
    }

    /// Draws the command palette and runs the one command it returned.
    ///
    /// The palette lists engine specs and builds their parameters from the current selection; it
    /// never mutates the document itself. A successful dispatch selects a node the command created
    /// and lets the next frame rebuild the scene from the new revision.
    fn command_palette(&mut self, ui: &mut egui::Ui) {
        let Self { palette, tabs, .. } = self;
        let context = tabs.active().palette_context();
        let Some(id) = palette.show(ui.ctx(), &tabs.active().session, &context) else {
            return;
        };
        // A locked node refuses a destructive command here, before the dispatch: the lock is a
        // guard on a gesture, so it never becomes a document rule (ADR 0065).
        if let Some(refusal) = tabs.active().locked_command_refusal(&id, ui::language(ui.ctx())) {
            tabs.active_mut().last_error = Some(refusal);
            return;
        }
        match palette.activate(&mut tabs.active_mut().session, &context, &id) {
            Ok(outcome) => {
                if let Some(created) = outcome.created_id() {
                    tabs.active_mut().editor.select_node(Some(created));
                }
            }
            Err(error) => tabs.active_mut().last_error = Some(error),
        }
    }

    /// Draws the work area's own pointer over the scene, and only there.
    ///
    /// Every panel, floating toolbar, text field, resizing border and menu keeps the system cursor;
    /// the corner orientation widget is handed in by name because it is drawn on the scene layer
    /// (X41). The square is painted last, so no surface can cover it inside the work area.
    ///
    /// The render place is the one workspace with no work area: it is a view of an image, nothing in
    /// it is drawn, dragged or picked, so the scene rectangle the document still carries from the
    /// viewport is not handed over and the system arrow stays over its rail, its status row and its
    /// air (X138).
    fn draw_work_cursor(&mut self, ctx: &egui::Context) {
        let scene = if self.render_open { None } else { self.tabs.active().scene_rect() };
        let widget = self.shell.orientation_rect;
        // The carry puts the pointer away while a camera gesture moves the view under the piece it
        // holds, and asks the window to bring it back to the piece once the gesture ends (X69).
        let hidden = self.tabs.active().hides_pointer();
        let hand = self.tabs.active().carry_hand();
        let cursor = work_cursor::show(ctx, &self.settings.pointer, scene, widget, hidden, hand);
        #[cfg(test)]
        {
            self.shell.work_cursor = cursor;
        }
        #[cfg(not(test))]
        let _ = cursor;
    }

    /// Draws the Settings window and applies what it changed.
    ///
    /// The window edits a copy-free `&mut Settings`; a change is applied to the running application
    /// at once and written to the config file when the window closes. The document never sees any of
    /// it, which is the point of keeping the workspace in its own file.
    fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            return;
        }
        let before = self.settings.clone();
        self.assistant_models.poll();
        let outcome =
            ui::settings::show(ctx, &mut self.settings, &mut self.settings_section, self.assistant_models.listed());
        if outcome.changed {
            self.apply_settings(&before, ctx);
        }
        // The page asked for an endpoint's models; the request is made off this frame and its answer
        // is drawn by a later one, so pressing the button never stalls the window (ADR 0234).
        if let Some(address) = outcome.list_models {
            let folder = self.settings_store.folder().map(std::path::Path::to_path_buf);
            self.assistant_models.request(&address, folder);
        }
        if outcome.close {
            self.settings_open = false;
            self.settings.sanitize();
            self.persist_settings();
        }
    }

    /// Applies the changed sections of the workspace settings.
    ///
    /// Appearance is the window's, so the theme and the zoom apply at once; the viewport section is
    /// pushed into the active scene, because a grid, an edge list or a style default is derived
    /// state that scene has to rebuild, and a theme switch repaints that scene's own colours in the
    /// same frame. The other tabs pick the new defaults up when they become active again.
    fn apply_settings(&mut self, before: &Settings, ctx: &egui::Context) {
        self.settings.sanitize();
        let colors_before = self.shell.colors;
        // The panels page edits the same state the header gestures write, so a change there replaces
        // the runtime pair and drops a memory that would restore an older one by surprise.
        if self.settings.panels != before.panels {
            self.shell.panels = Panels::from_settings(&self.settings.panels);
            self.shell.panels.forget_memory();
        }
        if self.settings.appearance != before.appearance {
            let (preference, preset, accent) = self.settings.install_args();
            theme::install_with(ctx, preference, preset, accent);
            ctx.set_zoom_factor(ui_scale::factor(self.settings.appearance.ui_scale));
            ui::set_language(ctx, self.settings.appearance.language.lang());
            ui::set_icon_color(ctx, self.settings.appearance.icon_color);
        }
        // The active variant is read after the install, so a theme switch in this very window
        // repaints the viewport now instead of on the next frame.
        self.shell.colors = theme::scene_colors(ctx.theme());
        if self.settings.viewport != before.viewport || self.shell.colors != colors_before {
            let Self { tabs, shell, settings, .. } = self;
            tabs.active_mut().apply_viewport_settings(&before.viewport, &settings.viewport, shell);
        }
        // A snap kind the Settings page switched applies to the whole workspace at once, exactly as
        // the panel visibility does: every tab reads one set (U116). The mode rides with it, because
        // it is the reading of the same candidates (U119).
        // The assistant section is the channel's door: a switch pressed here closes or opens it on
        // this frame, so the file a client reads describes the window that is actually there.
        if self.settings.assistant != before.assistant {
            self.apply_channel(ctx);
        }
        if self.settings.snaps.kinds() != before.snaps.kinds() || self.settings.snaps.mode != before.snaps.mode {
            let choice = SnapChoice { kinds: self.settings.snaps.kinds(), mode: self.settings.snaps.mode };
            for (_, document) in self.tabs.iter_mut() {
                document.set_snaps(choice);
            }
        }
    }

    /// Writes the snap choice a tab switched into the workspace settings, and into every other tab.
    ///
    /// The tab owns the live values — the Snaps group's cells are drawn from them inside that tab —
    /// and the workspace owns the file, so the shell carries the change across on the frame a cell
    /// was pressed. The kinds and the mode travel together because they are one section (U116, U119).
    fn take_snaps(&mut self) {
        let Some(SnapChoice { kinds, mode }) = self.tabs.active_mut().take_snaps_change() else {
            return;
        };
        self.settings.snaps.set_kinds(kinds);
        self.settings.snaps.mode = mode;
        for (_, document) in self.tabs.iter_mut() {
            document.set_snaps(SnapChoice { kinds, mode });
        }
        self.persist_settings();
    }

    /// Draws the Home surface and performs the one action it asked for.
    ///
    /// Home goes through the same File menu path as the header: *New* and an open file each land in
    /// a tab, so a scene with unsaved changes is never discarded behind the user's back.
    fn home_surface(&mut self, ctx: &egui::Context) {
        if !self.home_open {
            return;
        }
        let Some(action) = ui::home::show(ctx, &self.settings.recent) else {
            return;
        };
        self.home_open = false;
        match action {
            ui::home::HomeAction::New | ui::home::HomeAction::EmptyTemplate => {
                self.handle_file_action(FileAction::New, ctx);
            }
            ui::home::HomeAction::Open => {
                let lang = ui::language(ctx);
                if let Some(action) = self.tabs.active_mut().request_open(lang) {
                    self.handle_file_action(action, ctx);
                }
            }
            ui::home::HomeAction::OpenRecent(path) => {
                self.handle_file_action(FileAction::Open(path), ctx);
            }
            ui::home::HomeAction::Close => {}
        }
    }

    /// Draws one frame of the whole interface and performs what the user asked for.
    ///
    /// Everything the window shows happens here: the header, the menus, the ribbon, the floating
    /// panels, the scene, the cluster, the palette, the Settings window and the Home surface. The
    /// eframe trait method delegates to it, so a test drives the real interface with an
    /// [`egui::Context`] and no window at all (U77), and the chrome the frame gave the window comes
    /// back, so the presses it keeps and the rectangles it hands out are asserted without pixels
    /// (U78).
    /// Draws the render place in the workspace and performs the one action it asked for.
    ///
    /// The page edits the place's own numbers and draws its own report; the shell starts the run,
    /// stops it and saves the picture where the person says. The scene it renders is the one in front
    /// of the person — the same frame and the same camera the viewport draws — taken at the moment the
    /// button is pressed and cloned into the worker, so an edit made while it runs does not change the
    /// image under it (ADR 0210, ADR 0211).
    fn render_page(&mut self, ui: &mut egui::Ui) {
        let lang = ui::language(ui.ctx());
        let scene = self.tabs.active().file_name(lang);
        let stage = self.render_place.stage();
        let picture = self.render_place.picture();
        // The numbers, the kept image and the last save are borrowed from one place at once, so the
        // page draws them without the shell cloning a path every frame (Y12).
        let (settings, has_image, saved) = self.render_place.page_parts();
        let action = ui::render_page::show(
            ui,
            ui::render_page::Page {
                scene: &scene,
                settings,
                bounds: render_place::RenderPlace::bounds(),
                stage: &stage,
                picture,
                has_image,
                saved,
            },
        );
        match action {
            Some(ui::render_page::Action::Render) => self.start_render(ui.ctx()),
            Some(ui::render_page::Action::Cancel) => self.render_place.cancel(),
            Some(ui::render_page::Action::Save) => self.ask_for_image(ui.ctx()),
            Some(ui::render_page::Action::Materials) => self.material_window.open(),
            Some(ui::render_page::Action::Close) => self.render_open = false,
            None => {}
        }
    }

    /// Starts the run the page asked for, or reports on the place why it could not.
    fn start_render(&mut self, ctx: &egui::Context) {
        let document = self.tabs.active();
        let frame = document.drawn_frame().clone();
        // The image is the document's camera when it has one — with that camera's lens — and the view's
        // own camera otherwise, which is exactly what the viewport draws through (Y11).
        let camera = crate::scene_build::scene_camera(document.session.project(), document.camera());
        let revision = document.session.project().revision().get();
        let state = self.render_state.clone();
        if let Err(reason) = self.render_place.start(ctx, state.as_ref(), frame, camera, revision) {
            self.render_place.failed(reason);
        }
    }

    /// Saves the render place's picture where the person says, in the folder the scene lives in.
    fn ask_for_image(&mut self, ctx: &egui::Context) {
        let lang = ui::language(ctx);
        let document = self.tabs.active();
        let folder = file_picker::folder_of(document.path());
        let name = match document.path().and_then(|path| path.file_stem()) {
            Some(stem) => format!("{}.{}", stem.to_string_lossy(), file_picker::IMAGE_EXTENSION),
            None => format!("render.{}", file_picker::IMAGE_EXTENSION),
        };
        let order = file_picker::Order { mode: PickMode::Image, folder, name: Some(name), lang };
        if let Err(error) = self.picker.start(ctx, order) {
            self.render_place.failed(error);
        }
    }

    /// Draws the workspace under the header and returns the chrome it covered.
    ///
    /// The ribbon, the viewport, the two floating panels and the view cluster are one surface: the
    /// panels float over the scene, so the frame's geometry is computed once and handed to the
    /// viewport, and picking and the split views read exactly the rectangles the panels cover (M2).
    ///
    /// While the render place is open this whole surface is replaced by it: a render is a place in
    /// the work, not a question over the model, and a person who opens it means to see the image,
    /// the numbers and the progress rather than the cabinet under them (ADR 0211). Nothing floats
    /// over the place, so the chrome it covers is empty.
    fn design_area(&mut self, ui: &mut egui::Ui) -> Chrome {
        let design = ui.available_rect_before_wrap();
        if self.render_open {
            self.render_page(ui);
            self.status_bar(ui);
            return Chrome::empty();
        }
        self.ribbon(ui, design);
        self.status_bar(ui);
        // The viewport is the whole area under the header and above the status bar: the panels float
        // over it, so the rendered scene runs under their translucent fill (M2). The frame's
        // geometry is computed once and handed to the viewport, so picking and the split views read
        // exactly the rectangles the panels cover.
        let viewport = ui.available_rect_before_wrap().intersect(ui.clip_rect());
        let ribbon_open = self.tabs.active().ribbon_open();
        let band = self.shell.ribbon_band(viewport, ribbon_open);
        let band_height = self.shell.ribbon_inset(ribbon_open);
        let ctx = ui.ctx().clone();
        let animated_band =
            ctx.animate_value_with_time(egui::Id::unique("ribbon_band_height"), band_height, ANIMATION_SECONDS);
        let width = |id: &str, state: ui::settings::PanelState| {
            let target = if state.expanded { EXPANDED_WIDTH } else { STRIP_WIDTH };
            ctx.animate_value_with_time(egui::Id::unique((id, "panel_width")), target, ANIMATION_SECONDS)
        };
        let top = viewport.top() + panel_top(animated_band);
        let panels = &self.shell.panels;
        let rects = PanelRects::of(
            viewport,
            top,
            panels,
            (width("scene_tree", panels.scene), width("panel_card", panels.inspector)),
        );
        let chrome = Chrome {
            panels: rects,
            ribbon: band,
            list: self.shell.list_rect,
            menu: self.tabs.active().menu_rect(),
            tool: self.tabs.active().tool_card_rect(),
        };
        self.workspace(ui, &chrome);
        if let Some(rect) = rects.left {
            self.scene_tree(&ctx, rect);
        }
        if let Some(rect) = rects.right {
            self.panel_card(&ctx, rect);
        }
        self.sync_panels();
        // A node menu's four gestures are window state too, and the tab that drew the entry cannot
        // write it: the editor recorded the wish and the shell applies it here, after both menus
        // have drawn (U113).
        if let Some(mode) = self.tabs.active_mut().take_mode_request() {
            self.shell.mode = mode;
        }
        chrome
    }

    fn frame(&mut self, ui: &mut egui::Ui) -> Chrome {
        // The frame the request path names when it answers an agent's call: a frame that landed one
        // is the frame the person sees it in (X111).
        self.frames = self.frames.saturating_add(1);
        // The system cursor is the base the whole frame starts from: every surface may ask for an
        // icon of its own, and the shell replaces a `Default` with its square at the end (X40).
        ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
        // An agent's call is answered here, between frames, before anything this frame derives: a
        // command lands in the active document and the scene is built from what it left (ADR 0187).
        self.serve_channel(ui.ctx());
        // A chat turn takes the same boundary: one answer per frame, so the person's stop lands
        // between two calls and the scene this frame draws is what the agent left (ADR 0187).
        let lang = ui::language(ui.ctx());
        // An agent process the workspace names is started here, once, and given the channel this
        // window offers (ADR 0230). A command that is empty, or a channel that is closed, is the
        // panel's own state and not a failure.
        self.apply_assistant();
        self.serve_chat(lang);
        // A directory another application finished writing is swapped in here, at the frame
        // boundary, before anything this frame derives from it: the tree, the cards, the meshes and
        // the diagnostics all read the one snapshot, and the undo history never sees it (O5).
        self.take_directory();
        self.take_materials();
        // The file manager answers on its own schedule, so its path is taken at the same boundary
        // and becomes the open or the save the gesture would have been (ADR 0182).
        self.take_pick(ui.ctx());
        // A finished render is taken at the same boundary, and a run in flight asks for the next
        // frame so its counter keeps moving even while the person is back in the document
        // (ADR 0211).
        let state = self.render_state.clone();
        if self.render_place.poll(ui.ctx(), state.as_ref()) || self.render_place.is_running() {
            ui.ctx().request_repaint();
        }
        // The language is workspace settings, published once per frame so every surface draws the
        // same words without threading a `Lang` through every signature (U49); the icon-colour mode
        // is published the same way, so the rasteriser never needs the settings threaded to it.
        ui::set_language(ui.ctx(), self.settings.appearance.language.lang());
        ui::set_icon_color(ui.ctx(), self.settings.appearance.icon_color);
        // The theme variant is the window's, and `System` can follow the OS while the window is
        // open, so the shell re-reads it every frame instead of trusting the last settings change.
        self.shell.colors = theme::scene_colors(ui.ctx().theme());
        {
            let Self { tabs, shell, settings, .. } = self;
            // A scene whose last rebuild overran the budget says so before it runs, so the frame
            // that paints the message and the last good geometry is the frame before the work.
            shell.rebuilding = tabs.active().busy(&settings.viewport, shell.colors);
            if let Err(error) = tabs.active_mut().sync_scene(ui.ctx(), &settings.viewport, shell.colors) {
                shell.rebuilding = false;
                tabs.active_mut().last_error = Some(error);
            }
        }
        // A modal — the Settings window, the Home surface, the palette, a destructive question —
        // owns the keys for the frame it is up, so typing into a field never saves, undoes, reframes
        // or leaves a tool, and a question never answers itself with a second Delete.
        //
        // The assistant panel is a window, not a modal: it may be open while the person works, so it
        // keeps Escape and the tool keys for "handle_keys" but not the shell's Delete, Save or Undo.
        // Treating it as a modal is what made Delete do nothing with the chat open (X123).
        let modals = self.settings_open
            || self.home_open
            || self.palette.is_open()
            || self.product_card.is_open()
            || self.pending_question.is_some()
            // The material window is a floating editor with a field in it: while it is up, a key is
            // the editor's and never the scene's (Y14).
            || self.material_window.is_open();
        let chat_open = self.assistant_open;
        {
            let Self { tabs, shell, .. } = self;
            tabs.active_mut().handle_keys(ui, modals || chat_open, shell);
        }
        if let Err(error) = self.tabs.active_mut().sync_selection() {
            self.failed(error);
        }
        // A focused field owns the keys, so typing never saves, undoes, reframes or arms a tool. The
        // assistant's own key is the one exception: it is what opened the panel, and the field the
        // panel takes the caret in would otherwise leave a window only the pointer can close (X122).
        if ui.ctx().text_edit_focused() {
            if !modals && take_assistant_key(ui.ctx()) {
                self.toggle_assistant();
            }
        } else if !modals {
            let shortcuts = take_shortcuts(ui.ctx());
            if shortcuts.palette {
                self.palette.open();
            }
            if shortcuts.assistant {
                self.toggle_assistant();
            }
            let requested = self.tabs.active_mut().handle_shortcuts(shortcuts, ui::language(ui.ctx()));
            if let Some(action) = requested {
                self.handle_file_action(action, ui.ctx());
            }
            if shortcuts.new_tab {
                self.handle_file_action(FileAction::New, ui.ctx());
            }
            if shortcuts.close_tab {
                let active = self.tabs.active_id();
                self.close_tab(active, ui::language(ui.ctx()));
            }
            if shortcuts.delete {
                self.delete_selection(ui::language(ui.ctx()));
            }
            if shortcuts.grid_snap {
                self.tabs.active_mut().toggle_draw_grid();
            }
            if shortcuts.shadows {
                self.tabs.active_mut().toggle_shadows();
            }
            if shortcuts.measure {
                self.tabs.active_mut().toggle_measure(ui.ctx());
            }
            if shortcuts.niche {
                self.tabs.active_mut().toggle_void(ui.ctx());
            }
            if shortcuts.hide_panels {
                self.shell.panels.toggle_hide_all();
            }
            if shortcuts.collapse_panels {
                self.shell.panels.toggle_collapse_all();
            }
            // The four gestures are the window's view state, not the tab's: the key arms the mode
            // for every tab, and no document is written (ADR 0113, U113).
            if let Some(mode) = shortcuts.mode {
                self.shell.mode = mode;
            }
        }

        let header_state = {
            let active = self.tabs.active_id();
            HeaderState {
                tabs: self
                    .tabs
                    .iter()
                    .map(|(id, document)| TabView {
                        id,
                        title: document.file_name(ui::language(ui.ctx())),
                        dirty: document.is_dirty(),
                        active: id == active && !self.render_open,
                    })
                    .collect(),
                can_undo: self.tabs.active().can_undo(),
                can_redo: self.tabs.active().can_redo(),
                ribbon_open: self.tabs.active().ribbon_open(),
                panels_hidden: !self.shell.panels.any_visible(),
                panels_collapsed: self.shell.panels.all_collapsed(),
                mode: self.shell.mode,
                render_open: self.render_open,
                assistant_open: self.assistant_open,
            }
        };
        let mut header_action = None;
        let mut file_action = None;
        {
            let Self { tabs, .. } = self;
            egui::Panel::top("header").show(ui, |ui| {
                header_action = header::show(ui, &header_state, |ui| {
                    file_action = tabs.active_mut().show_file_menu(ui);
                });
            });
        }
        if let Some(action) = file_action {
            self.handle_file_action(action, ui.ctx());
        }
        if let Some(action) = header_action {
            self.handle_header_action(action, ui.ctx());
        }
        self.answer_question(ui.ctx());

        let chrome = self.design_area(ui);
        self.command_palette(ui);
        // The cluster's *Configure…* link is drawn inside the viewport, which only has the shell;
        // the window owns the Settings modal, so it opens it here on the same frame (U64).
        if std::mem::take(&mut self.shell.open_settings) {
            self.settings_open = true;
            self.settings_section = SettingsSection::Viewport;
            self.settings_notice = None;
        }
        self.settings_window(ui.ctx());
        // Escape closes the material window before the place behind it leaves, so one press closes
        // one thing; the key is taken here because the place draws earlier in the frame.
        if self.material_window.is_open()
            && ui.ctx().input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.material_window.close();
        }
        self.material_window_frame(ui.ctx());
        self.assistant(ui.ctx());
        self.take_snaps();
        self.product_frame_card(ui.ctx());
        self.home_surface(ui.ctx());
        self.draw_work_cursor(ui.ctx());
        // The close command needs the context, which only the frame has: everything else a quit
        // touches — the questions, the settings write — has already happened by now.
        if self.quit_requested {
            self.quit_requested = false;
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
        chrome
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.frame(ui);
    }

    fn on_exit(&mut self) {
        // The channel's file outlives nothing: a clean exit removes it, so a reader that cannot
        // connect reads "no window is open here" and never a token nobody answers (ADR 0188).
        if let Some(channel) = self.channel.take() {
            channel.shutdown();
        }
        self.persist_settings();
    }
}

/// Reads the command line first: an export or a help request finishes here, before eframe creates a
/// surface, so the same binary writes a package with no window, no GPU and no person (K3).
fn main() -> eframe::Result {
    // The sink is installed before anything can fail: the first tab, the command line and the window
    // all write through it, and a bug report is the file it names (ADR 0168).
    if let Some(path) = logging::install() {
        log::info!(target: "z3l::shell", "log file {}", path.display());
    }
    log::info!(target: "z3l::shell", "{} starts", theme::PRODUCT_NAME);
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = match cli::parse(&args) {
        Ok(command) => command,
        Err(error) => exit_with(cli::fail(&error, cli::EXIT_USAGE)),
    };
    match cli::run(&command) {
        Some(code) => exit_with(code),
        None => match command {
            cli::Command::Scene(path) => run_window(Some(path)),
            cli::Command::Directory(folder) => directory_app::run(folder),
            _ => run_window(None),
        },
    }
}

/// Ends the process with `code`, so the tail of `main` — the window — never runs after a command.
fn exit_with(code: i32) -> ! {
    std::process::exit(code)
}

/// Opens the window and runs the editor, on `startup`'s scene when the command line named one.
///
/// The scene opens through the same `open_tab` the File menu and the Home surface use, so a path
/// named on the command line and a path picked in the window produce the same tab, the same recent
/// entry and the same catalogue. A file that does not read is the tab's own error: the window stays
/// open on the sentence and the person can open another scene.
fn run_window(startup: Option<std::path::PathBuf>) -> eframe::Result {
    let options = native_options();
    eframe::run_native(
        theme::PRODUCT_NAME,
        options,
        Box::new(move |cc| {
            let mut app = App::new(cc)?;
            if let Some(path) = startup
                && let Err(error) = app.open_tab(&path)
            {
                app.failed(error);
            }
            Ok(Box::new(app))
        }),
    )
}

/// The window the shell asks `eframe` for: its title and its machine identity both come from the
/// product constants, so a rename cannot leave one of them behind (ADR 0135).
fn native_options() -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(theme::PRODUCT_NAME)
            .with_app_id(APP_ID)
            .with_inner_size([1280.0, 840.0])
            .with_min_inner_size([640.0, 480.0]),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use scene::Projection;

    use super::*;
    use crate::view_cluster::{cluster_anchor, projection_button};

    #[test]
    fn the_window_carries_the_product_identity() {
        let options = native_options();
        assert_eq!(options.viewport.title.as_deref(), Some(theme::PRODUCT_NAME), "the title is the product name");
        assert_eq!(
            options.viewport.app_id.as_deref(),
            Some(APP_ID),
            "the window class and the settings path share one id"
        );
        assert_eq!(APP_ID, APP_ID.to_lowercase(), "an app id is a path component, so it carries no capitals");
    }

    #[test]
    fn the_projection_button_draws_the_mode_the_view_is_in() {
        let cases = [
            (Projection::Perspective { fov_y: 0.9 }, icons::view::PERSPECTIVE, "orthographic"),
            (Projection::Orthographic { height: 600.0 }, icons::view::ORTHOGRAPHIC, "perspective"),
        ];
        for (projection, expected_icon, action) in cases {
            let (icon, hint) = projection_button(projection);
            assert_eq!(icon, expected_icon, "{projection:?} draws itself, not the mode a click would reach");
            assert!(hint.text(text::Lang::En).contains(action), "the hint names the switch to `{action}`: {hint:?}");
            assert!(hint.text(text::Lang::En).contains("(O)"), "the hint keeps the shortcut: {hint:?}");
        }
    }

    #[test]
    fn the_cluster_is_anchored_to_the_bottom_centre_of_the_viewport() {
        let wide = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 600.0));
        let narrow = egui::Rect::from_min_size(egui::pos2(240.0, 0.0), egui::vec2(460.0, 600.0));
        let offset = theme::FLOATING_OFFSET;
        for viewport in [wide, narrow] {
            let anchor = cluster_anchor(viewport, offset);
            assert!((anchor.x - viewport.center().x).abs() < f32::EPSILON, "the cluster is centred on x");
            assert!((anchor.y - (viewport.bottom() - offset)).abs() < f32::EPSILON, "and lifted off the bottom");
        }
        assert_ne!(
            cluster_anchor(wide, offset).x,
            cluster_anchor(narrow, offset).x,
            "folding a side panel moves the cluster with the surface it floats over"
        );
    }

    #[test]
    fn the_display_switch_controls_the_block() {
        let mut cluster = ViewCluster::default();
        assert!(!cluster.osd_visible(), "the block is off until the switch is turned on (U42)");
        cluster.show_osd = true;
        assert!(cluster.osd_visible(), "the switch controls the on-screen display (U106)");
    }

    #[test]
    fn the_ribbon_band_follows_the_measured_strip() {
        // The band the tools must stay clear of is the window's, not the scene's: it is the same
        // rectangle whichever tab is active, and the measured strip wins over the nominal height.
        let mut shell = Shell::default();
        let area = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 600.0));
        let unfolded = shell.ribbon_band(area, true).height();
        assert!(unfolded > 0.0, "an unfolded ribbon covers part of the viewport");
        assert!((shell.ribbon_inset(true) - unfolded).abs() < f32::EPSILON, "the panels start below the same band");
        assert_eq!(shell.ribbon_band(area, false).height(), ui::toolbars::height(false));

        shell.ribbon_rect = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 91.0)));
        assert!((shell.ribbon_inset(true) - 91.0).abs() < f32::EPSILON, "the strip that was drawn wins");
        assert!((shell.ribbon_band(area, true).height() - 91.0).abs() < f32::EPSILON);
        assert!(
            (shell.ribbon_band(area, false).height() - 91.0).abs() < f32::EPSILON,
            "a measured strip is the band whatever its nominal height was"
        );
    }
}
