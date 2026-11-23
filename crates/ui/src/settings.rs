//! The workspace settings and their window.
//!
//! Settings are a view of the application, never a document surface: a change applies at once and
//! never reaches the `.z3l`, and a scene opened on another machine carries none of it. The window is
//! the only editor of [`Settings`]; persistence belongs to the shell, which owns the config path and
//! the atomic write (ADR 0068).
//!
//! The data is versioned and repaired on read. A missing, truncated or older file is not an error
//! the user has to solve: the loader falls back to the defaults and reports the reason in the status
//! bar. A value outside its range is clamped, so a hand-edited file cannot push the UI scale past
//! what the control offers.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use egui::{Align, Context, Id, Layout, Modal, Ui};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use text::{Lang, Message};

use crate::display::{OpeningView, ShadingStyle, shading_label};
use crate::scrub::Scrub;
use crate::{theme, ui_scale};

/// The settings schema this build writes and reads.
pub const SETTINGS_VERSION: u32 = 1;

/// The smallest grid step the settings offer, in millimetres.
pub const MIN_GRID_STEP_MM: u32 = 1;

/// The largest grid step the settings offer, in millimetres.
pub const MAX_GRID_STEP_MM: u32 = 10_000;

/// The smallest snap tolerance the settings offer, in millimetres.
///
/// One millimetre is the move grid itself, so the smallest setting still captures a drag that
/// has already landed: below it the field would offer a number no drag can use.
pub const MIN_SNAP_MM: u32 = 1;

/// The largest snap tolerance the settings offer, in millimetres.
///
/// Five centimetres is a hand's width across the shop: past it a piece jumps to a surface it was
/// nowhere near, and the drag stops feeling like the maker's own.
pub const MAX_SNAP_MM: u32 = 50;

/// The tolerance a fresh workspace snaps at, in millimetres.
pub const DEFAULT_SNAP_MM: u32 = 5;

/// The smallest hand-turn step of the roll ring, in degrees.
pub const MIN_ROLL_STEP_DEGREES: u32 = 1;

/// The largest hand-turn step of the roll ring, in degrees.
pub const MAX_ROLL_STEP_DEGREES: u32 = 15;

/// The smallest translate handle the settings offer, in points.
pub const MIN_HANDLE_POINTS: u32 = 8;

/// The largest translate handle the settings offer, in points.
pub const MAX_HANDLE_POINTS: u32 = 400;

/// The smallest rotate ring the settings offer, in percent of the camera distance.
pub const MIN_RING_PERCENT: u32 = 1;

/// The largest rotate ring the settings offer, in percent of the camera distance.
pub const MAX_RING_PERCENT: u32 = 30;

/// The smallest work cursor a hand can still see and grab, in points.
pub const MIN_CURSOR_POINTS: u32 = 6;

/// The largest work cursor, in points: past this the square hides the thing under the pointer.
pub const MAX_CURSOR_POINTS: u32 = 32;

/// The work cursor a fresh workspace draws, in points.
pub const DEFAULT_CURSOR_POINTS: u32 = 12;

/// The length of a translate handle a fresh workspace opens with, in points.
///
/// Below the 84 pt the shell drew before U86, because a gizmo larger than a small part covers the
/// face the person is judging.
pub const DEFAULT_HANDLE_POINTS: u32 = 56;

/// The radius of a rotate ring a fresh workspace opens with, in percent of the camera distance.
pub const DEFAULT_RING_PERCENT: u32 = 6;

/// The largest display strength the settings offer, in percent.
pub const MAX_DISPLAY_STRENGTH: u32 = 100;

/// The floor-grid brightness a fresh workspace opens with, in percent.
pub const DEFAULT_GRID_STRENGTH: u32 = 60;

/// The form-outline strength a fresh workspace opens with, in percent.
pub const DEFAULT_EDGE_STRENGTH: u32 = 70;

/// The ground-shadow strength a fresh workspace opens with, in percent.
///
/// The view's switch is beside it in the View tab's Display group, which is where the intensity is
/// edited (U107); this is only the value a workspace starts with and the one the reset restores.
pub const DEFAULT_SHADOW_STRENGTH: u32 = 40;

/// The most transparent the floor plate can be drawn, in percent.
///
/// Zero is a real answer, not the absence of one: the plate and the shadow it carries are gone,
/// because a shadow needs a surface to darken (U109).
pub const MIN_FLOOR_OPACITY: u32 = 0;

/// The most the floor plate can be drawn: opaque, the floor a workspace opens with.
pub const MAX_FLOOR_OPACITY: u32 = 100;

/// The floor opacity a fresh workspace opens with, in percent.
///
/// Opaque, so a new workspace draws exactly the floor it drew before U109 gained the field; the
/// person who wants to see a part through the ground lowers it.
pub const DEFAULT_FLOOR_OPACITY: u32 = 100;

/// How many recent files the workspace remembers.
pub const MAX_RECENT: usize = 10;

/// The theme preference offered by the settings window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeChoice {
    /// Light text on a dark surface.
    Dark,
    /// Dark text on a light surface.
    Light,
    /// Follow the operating system.
    #[default]
    System,
}

impl ThemeChoice {
    /// Every choice in the order the window lists them.
    pub const ALL: [Self; 3] = [Self::Dark, Self::Light, Self::System];

    /// The user-visible name.
    pub const fn label(self) -> Message {
        match self {
            Self::Dark => Message::ThemeDark,
            Self::Light => Message::ThemeLight,
            Self::System => Message::ThemeSystem,
        }
    }

    /// The `egui` preference that activates the choice.
    pub const fn preference(self) -> egui::ThemePreference {
        match self {
            Self::Dark => egui::ThemePreference::Dark,
            Self::Light => egui::ThemePreference::Light,
            Self::System => egui::ThemePreference::System,
        }
    }
}

/// The language the interface speaks, as the workspace stores it.
///
/// A saved `.z3l` never carries this: the language belongs to the workspace, not to the scene, so
/// the same file opens in whatever language the person looking at it chose.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    /// English.
    #[default]
    English,
    /// Russian.
    Russian,
}

impl Language {
    /// Every language in the order the window lists them.
    pub const ALL: [Self; 2] = [Self::English, Self::Russian];

    /// The message-table language this choice selects.
    pub const fn lang(self) -> Lang {
        match self {
            Self::English => Lang::En,
            Self::Russian => Lang::Ru,
        }
    }

    /// The choice that selects `lang`.
    pub const fn from_lang(lang: Lang) -> Self {
        match lang {
            Lang::En => Self::English,
            Lang::Ru => Self::Russian,
        }
    }

    /// The language's own name, never translated: a user who cannot read the current language has
    /// to be able to find their own in the list.
    pub const fn label(self) -> Message {
        match self {
            Self::English => Message::LanguageEnglish,
            Self::Russian => Message::LanguageRussian,
        }
    }
}

/// The narrowest panel opacity the settings offer, in percent.
pub const MIN_PANEL_OPACITY: u32 = 55;

/// The widest panel opacity the settings offer, in percent.
pub const MAX_PANEL_OPACITY: u32 = 100;

/// The opacity a fresh workspace opens with, in percent.
pub const DEFAULT_PANEL_OPACITY: u32 = 92;

/// The raster width below which [`IconColor::Auto`] falls back to one ink, in pixels.
///
/// A pixel count, not a point size: at 16 px a two-tone fill reads as a muddy blob and the outline
/// loses its weight, while at 20 px the three tones separate. A UI scale below 1 therefore falls
/// back on its own (`plan/notes/U70.md`).
pub const ICON_COLOR_MIN_PX: u32 = 20;

/// How the icon pipeline colours a glyph.
///
/// The setting is read by the icon pass (U70): a semantic family gives an icon its hue, and the
/// automatic mode drops to one ink where the raster is too small to carry a colour.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IconColor {
    /// A hue by semantic family, with a monochrome fallback below a 20 px raster.
    #[default]
    Auto,
    /// A hue by semantic family at every size.
    Always,
    /// One ink everywhere.
    Never,
}

impl IconColor {
    /// Every mode in the order the window lists them.
    pub const ALL: [Self; 3] = [Self::Auto, Self::Always, Self::Never];

    /// Whether a raster of `px` pixels carries the role inks rather than one tint.
    pub const fn uses_roles(self, px: u32) -> bool {
        match self {
            Self::Auto => px >= ICON_COLOR_MIN_PX,
            Self::Always => true,
            Self::Never => false,
        }
    }

    /// The user-visible name.
    pub const fn label(self) -> Message {
        match self {
            Self::Auto => Message::IconColorAuto,
            Self::Always => Message::IconColorAlways,
            Self::Never => Message::IconColorNever,
        }
    }
}

/// The *Appearance* section.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    /// The theme variant, or the operating system's preference.
    pub theme: ThemeChoice,
    /// The language the interface speaks.
    pub language: Language,
    /// The approved look the whole window is painted with.
    pub palette: theme::ThemePreset,
    /// The accent the user picked; `None` draws the accent the palette names.
    pub accent: Option<[u8; 3]>,
    /// The opacity of the chrome drawn over the scene, in percent.
    pub panel_opacity: u32,
    /// How the icon pipeline colours a glyph.
    pub icon_color: IconColor,
    /// The UI scale, in percent.
    pub ui_scale: u32,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: ThemeChoice::default(),
            language: Language::default(),
            palette: theme::ThemePreset::default(),
            accent: None,
            panel_opacity: DEFAULT_PANEL_OPACITY,
            icon_color: IconColor::default(),
            ui_scale: ui_scale::DEFAULT_PERCENT,
        }
    }
}

impl Appearance {
    /// The accent override as a colour, if the user picked one.
    pub fn accent_color(&self) -> Option<egui::Color32> {
        self.accent.map(|[r, g, b]| egui::Color32::from_rgb(r, g, b))
    }

    /// The accent the window draws with in the `dark` variant, override included.
    pub fn drawn_accent(&self, dark: bool) -> egui::Color32 {
        self.accent_color().unwrap_or_else(|| self.palette.palette(dark).accent)
    }
}

/// The *Viewport* section: what a new viewport opens with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewportSettings {
    /// The display style a viewport opens with.
    pub style: ShadingStyle,
    /// Whether the floor grid is drawn.
    pub grid: bool,
    /// The minor grid spacing, in millimetres.
    pub grid_step_mm: u32,
    /// The brightness of the work-plane floor grid, in percent; 0 hides it.
    pub grid_strength: u32,
    /// The strength of the form-outline pass, in percent; 0 leaves the fill only.
    pub edge_strength: u32,
    /// How far the ground shadow darkens the floor plate, in percent (U107).
    ///
    /// One value for every view and a workspace default, because the shadow's *switch* is the view's
    /// leaf and its intensity is a taste knob nobody wants four different answers to.
    pub shadow_strength: u32,
    /// How opaque the floor plate is, in percent (U109).
    ///
    /// Below 100 the plate is blended over what stands under it, so a part below the floor is seen
    /// through the floor's tint; at 0 the plate and its shadow are gone, because a shadow needs a
    /// surface to darken. Like the strengths it is read when the frame draws, so a drag costs no
    /// upload.
    pub floor_opacity: u32,
    /// Whether the edge pass draws every triangle edge, diagonals included.
    pub all_edges: bool,
    /// The direction a viewport opens from.
    pub opening_view: OpeningView,
    /// The step of a hand turn on the corner widget's roll ring, in degrees.
    pub roll_step_degrees: u32,
}

impl Default for ViewportSettings {
    fn default() -> Self {
        Self {
            style: ShadingStyle::default(),
            grid: true,
            grid_step_mm: 100,
            grid_strength: DEFAULT_GRID_STRENGTH,
            edge_strength: DEFAULT_EDGE_STRENGTH,
            shadow_strength: DEFAULT_SHADOW_STRENGTH,
            floor_opacity: DEFAULT_FLOOR_OPACITY,
            all_edges: false,
            opening_view: OpeningView::default(),
            roll_step_degrees: 1,
        }
    }
}

/// The *Gizmo* section: how large the transform gizmos draw under the pointer.
///
/// The sizes are screen terms — points for the translate handles, a percentage of the camera
/// distance for the rotate rings — so a hand that works at 4K and a hand that works at 1080p can
/// pick the size that suits it. Nothing here reaches the document: changing a size moves no
/// revision and leaves no undo entry (U86).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GizmoSettings {
    /// The length of a translate handle, in points.
    pub handle_points: u32,
    /// The radius of a rotate ring, in percent of the camera distance.
    pub ring_percent: u32,
}

impl Default for GizmoSettings {
    fn default() -> Self {
        Self { handle_points: DEFAULT_HANDLE_POINTS, ring_percent: DEFAULT_RING_PERCENT }
    }
}

impl GizmoSettings {
    /// The handle length a drag draws and hits with, in points.
    pub fn handle_length(&self) -> f32 {
        self.handle_points.clamp(MIN_HANDLE_POINTS, MAX_HANDLE_POINTS) as f32
    }

    /// The ring radius as a fraction of the camera distance.
    pub fn ring_factor(&self) -> f64 {
        f64::from(self.ring_percent.clamp(MIN_RING_PERCENT, MAX_RING_PERCENT)) / 100.0
    }
}

/// The *Snapping* section: what a pointer captures, what a drag stops at, and how near it must come.
///
/// Snapping is a taste and a workshop standard, so the values are the workspace's and never the
/// document's: turning a kind off costs no undo entry and leaves no revision behind. The model owns
/// what each value means (`model::SnapKinds`, `model::SnapSettings`, `model::MoveSettings`) and the
/// shell hands them over — once when a drag starts, every frame to a pointer tool — so a change
/// applies to the next pointer position and never bends a drag already under the hand (ADR 0146,
/// U116). The floor is a stop beside them rather than a capture: it answers whether a body may sink
/// below Z = 0 at all (X87, ADR 0177).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SnapSettings {
    /// Whether the world coordinate planes X=0, Y=0 and Z=0 capture, the floor included: the
    /// `axis` kind of the set.
    ///
    /// Until a house-wall tool exists they stand in for the walls of a room: a base cabinet against
    /// the back wall of a shop and a piece standing on the floor are the same snap.
    pub planes: bool,
    /// Whether another product's envelope captures a drag with its virtual faces.
    pub frames: bool,
    /// How near a surface must be to capture, in millimetres; zero captures nothing.
    pub tolerance_mm: u32,
    /// Whether the invisible floor at Z = 0 stops every moved body.
    ///
    /// It is a stop and not a snap, so it holds whatever the switches say, and a workspace that
    /// turns it off has no floor at all (ADR 0177).
    pub floor: bool,
    /// The pointer's own kinds: one switch per kind, all on in a fresh workspace (U116).
    pub kinds: PointerKinds,
    /// How a snap reads a corner above the drawing plane: the point itself, or its foot on the
    /// plane (U119, `model::SnapMode`).
    pub mode: model::SnapMode,
}

/// The pointer's snap kinds: what one click may capture in the scene.
///
/// The switches are saved under the names of the kinds the resolver answers with
/// (`model::SnapKind`), so the Settings page, the View tab's Snaps group and a file written on
/// another machine name the same ten things. A workspace written before the section existed snaps
/// everything, which is the fresh default.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PointerKinds {
    /// A corner.
    #[serde(default = "on")]
    pub vertex: bool,
    /// A crossing of two edges.
    #[serde(default = "on")]
    pub intersection: bool,
    /// The middle of an edge.
    #[serde(default = "on")]
    pub midpoint: bool,
    /// A centre.
    #[serde(default = "on")]
    pub centre: bool,
    /// A perpendicular foot.
    #[serde(default = "on")]
    pub perpendicular: bool,
    /// A tangent point.
    #[serde(default = "on")]
    pub tangent: bool,
    /// A point on an edge.
    #[serde(default = "on")]
    pub edge: bool,
    /// A point on a face.
    #[serde(default = "on")]
    pub face: bool,
    /// The drawing plane's grid.
    #[serde(default = "on")]
    pub grid: bool,
}

/// The default of a switch a workspace written before the section did not carry: on.
fn on() -> bool {
    true
}

impl Default for PointerKinds {
    fn default() -> Self {
        Self {
            vertex: true,
            intersection: true,
            midpoint: true,
            centre: true,
            perpendicular: true,
            tangent: true,
            edge: true,
            face: true,
            grid: true,
        }
    }
}

impl Default for SnapSettings {
    fn default() -> Self {
        Self {
            planes: true,
            frames: true,
            tolerance_mm: DEFAULT_SNAP_MM,
            floor: true,
            kinds: PointerKinds::default(),
            mode: model::SnapMode::default(),
        }
    }
}

impl SnapSettings {
    /// The kinds the pointer resolver reads: every switch of the section as one value.
    pub fn kinds(&self) -> model::SnapKinds {
        model::SnapKinds {
            vertex: self.kinds.vertex,
            intersection: self.kinds.intersection,
            midpoint: self.kinds.midpoint,
            centre: self.kinds.centre,
            perpendicular: self.kinds.perpendicular,
            tangent: self.kinds.tangent,
            edge: self.kinds.edge,
            face: self.kinds.face,
            axis: self.planes,
            grid: self.kinds.grid,
        }
    }

    /// Writes the kinds back into the section, so a toggle in a tab survives the restart.
    pub fn set_kinds(&mut self, kinds: model::SnapKinds) {
        self.planes = kinds.axis;
        self.kinds = PointerKinds {
            vertex: kinds.vertex,
            intersection: kinds.intersection,
            midpoint: kinds.midpoint,
            centre: kinds.centre,
            perpendicular: kinds.perpendicular,
            tangent: kinds.tangent,
            edge: kinds.edge,
            face: kinds.face,
            grid: kinds.grid,
        };
    }

    /// The value the move rule reads: the same facts, with the tolerance as the document's own unit so
    /// the model never sees a millimetre as a number.
    pub fn move_settings(&self) -> model::MoveSettings {
        let tolerance = i64::from(self.tolerance_mm.clamp(MIN_SNAP_MM, MAX_SNAP_MM));
        model::MoveSettings {
            axes: model::MoveAxes::Document,
            snaps: Some(model::SnapSettings {
                kinds: self.kinds(),
                frames: self.frames,
                tolerance: units::Length::from_micrometres(tolerance * 1000),
            }),
            floor: self.floor,
            // The snapping section feeds the object tool, which is the one the document measures.
            measured: true,
        }
    }
}

/// The *Work cursor* section: the square the shell draws at the pointer over the scene.
///
/// The size is a hand's preference, and a furniture maker lines two boards up across a view with
/// the crosshair on: both are screen terms and neither reaches the document (U88).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PointerSettings {
    /// The width of the square, in points; its centre dot keeps a fixed size.
    pub cursor_size: u32,
    /// Whether the cursor also draws a full crosshair across the view.
    pub crosshair: bool,
}

impl Default for PointerSettings {
    fn default() -> Self {
        Self { cursor_size: DEFAULT_CURSOR_POINTS, crosshair: false }
    }
}

impl PointerSettings {
    /// The square's width a frame draws, in points, inside the setting's limits.
    pub fn size_points(&self) -> f32 {
        self.cursor_size.clamp(MIN_CURSOR_POINTS, MAX_CURSOR_POINTS) as f32
    }
}

/// The ribbon's own state, remembered between runs.
///
/// The fold and the pinned method of each tool are the two ribbon values that outlive a run; which
/// tab is open, what the numeric fields hold and which variant list is open are session view state.
/// A file written before either field existed opens the body with the first method of every tool,
/// which is the state the workspace has always started in.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RibbonSettings {
    /// Whether the ribbon's tool body is drawn under its tab row.
    pub open: bool,
    /// The method a click and the tool's key run, by tool id: `sketch.create.circle` →
    /// `sketch.create.circle.three-points` (U67). An id this build does not know falls back to the
    /// tool's first method.
    pub pinned_variants: BTreeMap<String, String>,
}

impl Default for RibbonSettings {
    fn default() -> Self {
        Self { open: true, pinned_variants: BTreeMap::new() }
    }
}

/// Whether one side panel is on screen and whether it is unfolded.
///
/// The two states are independent: a panel may be visible as a folded strip, and a hidden panel
/// keeps the fold it had so the workspace can come back exactly as it left.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PanelState {
    /// Whether the panel is drawn at all.
    #[serde(default = "shown")]
    pub visible: bool,
    /// Whether the panel is unfolded rather than a strip.
    #[serde(default = "shown")]
    pub expanded: bool,
}

impl Default for PanelState {
    fn default() -> Self {
        Self { visible: true, expanded: true }
    }
}

/// A boolean whose absence means the panel is there, not that it was switched off.
const fn shown() -> bool {
    true
}

/// The *Panels* section: which side panels are shown and whether they are unfolded.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PanelsSettings {
    /// The scene tree on the left.
    pub scene: PanelState,
    /// The inspector on the right.
    pub inspector: PanelState,
}

/// The *Assistant* section: the local control channel an agent attaches to, the agent process the
/// panel starts, the endpoint a model is asked at, and the endpoints a person has agreed to.
///
/// The channel is on for a fresh workspace because a feature a person has to discover a flag for is a
/// feature that does not exist (ADR 0188). Its exposure is one machine, one account and one token that
/// dies with the process, and this switch is the one place a workshop closes the door.
///
/// Two ways to give the panel a model, and the process wins: a command the workshop types starts an
/// agent that owns the model loop (ADR 0230), and an endpoint is the OpenAI-compatible wire a cloud
/// provider and a server on this machine answer the same way (ADR 0189). Both are empty by default,
/// so a fresh workspace sends nothing anywhere.
///
/// The key is not here and never will be: it comes from `Z3L_AGENT_KEY` or from a file beside this
/// one, and the settings file is the kind of file people copy between machines. The consent list is
/// the only record of what a person agreed to send, one entry per endpoint, and clearing an entry is
/// how they ask to be asked again.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AssistantSettings {
    /// Whether the window offers the loopback control channel.
    pub attach: bool,
    /// The command an agent process is started with: one program and its arguments, on one line.
    pub agent: String,
    /// The OpenAI-compatible base URL a model is asked at (`https://host/v1`); empty for none.
    pub endpoint: String,
    /// The model name the endpoint answers with; empty for none.
    pub model: String,
    /// The endpoints a person has agreed to send a conversation to, oldest first.
    pub consent: Vec<String>,
}

impl Default for AssistantSettings {
    fn default() -> Self {
        Self { attach: true, agent: String::new(), endpoint: String::new(), model: String::new(), consent: Vec::new() }
    }
}

/// One address a person can fill the endpoint field with, and the word the picker shows for it.
///
/// A preset is **data, not a wire**: it writes a base URL into the field and nothing else, so a
/// workshop can edit the field afterwards and the request is still the one OpenAI-compatible wire
/// whatever was chosen (ADR 0235, which amends ADR 0189's "no provider name in a type"). The names
/// are the few addresses a furniture maker in this workshop is likely to be handed, not a catalogue:
/// a missing one costs a typed URL and no feature.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Preset {
    /// The word the picker shows, in both interface languages.
    pub name: Message,
    /// The base URL it fills the endpoint field with; the client appends `/chat/completions`.
    pub base_url: &'static str,
}

/// The presets the *Assistant* page offers, in the order it offers them.
///
/// Measured 2026-11-23: each address answers a model list at `{base}/models` and the two the same
/// gateway serves (`OpenCode` and `OpenCode Go`) differ only in the tier — `inference/openai/v1`
/// lists 105 models, `inference/go/v1` the 36 the subscription serves, and the `go` form is what the
/// client recognises as a gateway that routes on a session header (X124, ADR 0235). A local server
/// is the same field with its own port, which is why the last two presets name this computer.
pub const PRESETS: &[Preset] = &[
    Preset { name: Message::AssistantProviderOpenCodeGo, base_url: "https://opencode.ai/inference/go/v1" },
    Preset { name: Message::AssistantProviderOpenCode, base_url: "https://opencode.ai/inference/openai/v1" },
    Preset { name: Message::AssistantProviderDeepSeek, base_url: "https://api.deepseek.com/v1" },
    Preset { name: Message::AssistantProviderOpenAi, base_url: "https://api.openai.com/v1" },
    Preset { name: Message::AssistantProviderOllama, base_url: "http://localhost:11434/v1" },
    Preset { name: Message::AssistantProviderLmStudio, base_url: "http://localhost:1234/v1" },
];

/// One model an endpoint listed, as the page shows it.
///
/// The shell reads the endpoint's answer through `mcp` (the only crate that speaks HTTP) and hands
/// the rows here, so a page draws a choice without knowing what a wire is (ADR 0235).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelRow {
    /// The id a request asks for.
    pub id: String,
    /// The endpoint's own name for it, or the id when it has none.
    pub name: String,
}

/// What the shell knows about the models of the address the page shows.
///
/// It is view state: one listing per address, asked for by the page's button and thrown away when
/// the page closes, because a model list is the endpoint's answer and not a workspace setting.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ListedModels {
    /// The address these rows are about; a page showing another one draws nothing of them.
    pub address: String,
    /// Whether a listing of `address` is in flight.
    pub listing: bool,
    /// The rows, in the endpoint's own order.
    pub rows: Vec<ModelRow>,
    /// Why the last listing of `address` failed, in the client's own sentence.
    pub failed: Option<String>,
}

impl ListedModels {
    /// Whether the rows belong to `address`.
    pub fn is_for(&self, address: &str) -> bool {
        self.address == address.trim().trim_end_matches('/')
    }

    /// The note the page draws under the model field, when there is one to draw.
    ///
    /// It is the page's own sentence in every case but a refusal, which quotes the wire: a person
    /// who typed an address has to read what the address answered, not "failed" (ADR 0227).
    pub fn note(&self, address: &str, lang: Lang) -> Option<String> {
        if !self.is_for(address) {
            return None;
        }
        if self.listing {
            return Some(Message::AssistantModelsListing.text(lang).to_owned());
        }
        if let Some(failed) = &self.failed {
            return Some(Message::AssistantModelsFailed.with(lang, [failed.clone()]));
        }
        (self.rows.is_empty()).then(|| Message::AssistantModelsNone.text(lang).to_owned())
    }
}

impl AssistantSettings {
    /// The preset whose address is in the field, or `None` for a typed address.
    ///
    /// It is derived on read, so a preset cannot be remembered in the settings file and then
    /// disagree with the field: choosing one writes the address and nothing else.
    pub fn preset(&self) -> Option<usize> {
        let endpoint = self.endpoint.trim().trim_end_matches('/');
        PRESETS.iter().position(|preset| preset.base_url.trim_end_matches('/') == endpoint)
    }
}

/// Why a typed base URL cannot be used as one endpoint.
///
/// The wire appends `/chat/completions` to whatever the field holds, so an address that is not one
/// absolute `http(s)` URL produces a request to a path nobody serves — the owner met this as a plain
/// HTTP 404 after a URL was pasted into a URL that was already in the field. The settings page says
/// which clause is wrong before a message leaves; [`AssistantSettings::endpoint_to_ask`] stops the
/// request while the clause stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndpointProblem {
    /// A second `http://` or `https://` inside the address: a URL pasted into a URL.
    Doubled,
    /// The address does not start with `http://` or `https://`.
    Scheme,
    /// Nothing between the scheme and the first `/`, `?` or `#`.
    Host,
    /// A space or a line break, which no address carries.
    Space,
    /// The completion path is already there; the client appends it and a second one is a 404.
    Completion,
}

impl EndpointProblem {
    /// The clause the settings page and the panel read, never a sentence of its own.
    pub fn message(self) -> Message {
        match self {
            Self::Doubled => Message::AssistantEndpointDoubled,
            Self::Scheme => Message::AssistantEndpointNeedsScheme,
            Self::Host => Message::AssistantEndpointNoHost,
            Self::Space => Message::AssistantEndpointSpace,
            Self::Completion => Message::AssistantEndpointHasCompletion,
        }
    }

    /// The sentence the settings page draws and the panel repeats.
    pub fn sentence(self, lang: Lang) -> String {
        Message::AssistantEndpointProblem.with(lang, [self.message().text(lang)])
    }
}

/// The problem with `endpoint`, or `None` when it is usable or empty.
///
/// Empty is not a problem: a workspace with no endpoint is a workspace that asks nothing, and the
/// page says how to name one rather than calling the empty field wrong.
fn endpoint_problem(endpoint: &str) -> Option<EndpointProblem> {
    let endpoint = endpoint.trim();
    if endpoint.is_empty() {
        return None;
    }
    if endpoint.chars().any(char::is_whitespace) {
        return Some(EndpointProblem::Space);
    }
    let Some(rest) = endpoint.strip_prefix("https://").or_else(|| endpoint.strip_prefix("http://")) else {
        return Some(EndpointProblem::Scheme);
    };
    if rest.contains("://") {
        return Some(EndpointProblem::Doubled);
    }
    if endpoint.trim_end_matches('/').ends_with("/chat/completions") {
        return Some(EndpointProblem::Completion);
    }
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if host.is_empty() {
        return Some(EndpointProblem::Host);
    }
    None
}

impl AssistantSettings {
    /// The reason the typed address cannot be sent to, or `None` when it can or is empty.
    pub fn endpoint_problem(&self) -> Option<EndpointProblem> {
        endpoint_problem(&self.endpoint)
    }

    /// The endpoint a message would go to, or `None` when no model is configured.
    ///
    /// It answers with the endpoint as the person typed it, trimmed, so the consent list and the
    /// request name the same string. An address the client cannot use is not an endpoint: the page
    /// names the wrong clause and no request leaves. The agent process is not an endpoint and is not
    /// asked here.
    pub fn endpoint_to_ask(&self) -> Option<String> {
        let endpoint = self.endpoint.trim();
        let model = self.model.trim();
        (endpoint_problem(endpoint).is_none() && !endpoint.is_empty() && !model.is_empty()).then(|| endpoint.to_owned())
    }

    /// The reason this workspace's model cannot be asked, when it names a wrong address.
    ///
    /// An empty address or model is not a problem but an unconfigured assistant, so it answers
    /// `None`; the page still shows a wrong address while the person fills the rest in.
    pub fn problem(&self) -> Option<EndpointProblem> {
        if self.endpoint.trim().is_empty() || self.model.trim().is_empty() {
            return None;
        }
        self.endpoint_problem()
    }

    /// Whether the person has agreed to send a conversation to `endpoint`.
    pub fn consents(&self, endpoint: &str) -> bool {
        self.consent.iter().any(|held| held == endpoint)
    }

    /// Remembers one endpoint's agreement, once, at the end of the list.
    pub fn remember_consent(&mut self, endpoint: &str) {
        let endpoint = endpoint.trim();
        if endpoint.is_empty() || self.consents(endpoint) {
            return;
        }
        self.consent.push(endpoint.to_owned());
    }

    /// Forgets one endpoint's agreement, so the next message to it asks again.
    pub fn forget_consent(&mut self, endpoint: &str) {
        self.consent.retain(|held| held != endpoint);
    }
}

/// Everything the workspace remembers between runs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// The schema version written by this build.
    pub version: u32,
    /// Theme, palette, accent and UI scale.
    pub appearance: Appearance,
    /// Default display style, grid, edges and opening view.
    pub viewport: ViewportSettings,
    /// The size of the two transform gizmos.
    pub gizmo: GizmoSettings,
    /// What an object drag snaps to, and how near.
    pub snaps: SnapSettings,
    /// The size of the work cursor and whether it draws a crosshair.
    pub pointer: PointerSettings,
    /// Recently opened scenes, newest first.
    pub recent: Vec<PathBuf>,
    /// The fold of the ribbon and the pinned method of each tool.
    pub ribbon: RibbonSettings,
    /// The visibility and fold of the two side panels.
    pub panels: PanelsSettings,
    /// The local control channel an agent attaches to.
    pub assistant: AssistantSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            appearance: Appearance::default(),
            viewport: ViewportSettings::default(),
            gizmo: GizmoSettings::default(),
            snaps: SnapSettings::default(),
            pointer: PointerSettings::default(),
            recent: Vec::new(),
            ribbon: RibbonSettings::default(),
            panels: PanelsSettings::default(),
            assistant: AssistantSettings::default(),
        }
    }
}

/// A value the settings file carried that this build replaced with its default.
///
/// A file that loads with one unknown value is repaired rather than rejected: a palette name written
/// by a newer build must not cost the user their recent files and their viewport defaults, and the
/// shell reports the repair in the status bar instead of silently reinterpreting it (`AGENTS.md`
/// §4.5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Repair {
    /// The palette name is not one this build knows; the default preset is in use.
    UnknownPalette(String),
}

/// Why a settings file could not be used.
#[derive(Debug, Error)]
pub enum SettingsError {
    /// The file was written by another schema version.
    #[error("settings are version {found}, this build reads version {expected}")]
    Version {
        /// The version found in the file.
        found: u32,
        /// The version this build writes.
        expected: u32,
    },
    /// The file is not valid JSON or does not match the schema.
    #[error("settings are not readable: {0}")]
    Json(#[from] serde_json::Error),
}

impl Settings {
    /// Parses a settings file, rejecting a schema version this build does not know.
    ///
    /// The version is probed before the full schema is read, because `#[serde(default)]` would
    /// otherwise turn a missing version into the current one. Every value is clamped afterwards, so
    /// a hand-edited file still yields a usable workspace.
    pub fn from_json(text: &str) -> Result<Self, SettingsError> {
        Self::read_json(text).map(|(settings, _)| settings)
    }

    /// The same read, reporting the values it had to replace with their defaults.
    ///
    /// An unknown value is a report, not a rejection: the rest of the file — the recent list, the
    /// viewport defaults, the scale — still describes the workspace the user left.
    pub fn read_json(text: &str) -> Result<(Self, Vec<Repair>), SettingsError> {
        #[derive(Deserialize)]
        struct Probe {
            #[serde(default)]
            version: u32,
        }
        let probe: Probe = serde_json::from_str(text)?;
        if probe.version != SETTINGS_VERSION {
            return Err(SettingsError::Version { found: probe.version, expected: SETTINGS_VERSION });
        }
        let mut value: serde_json::Value = serde_json::from_str(text)?;
        let mut repairs = Vec::new();
        if let Some(repair) = canonical_palette(&mut value) {
            repairs.push(repair);
        }
        migrate_accent(&mut value);
        let mut settings: Self = serde_json::from_value(value)?;
        settings.version = SETTINGS_VERSION;
        settings.sanitize();
        Ok((settings, repairs))
    }

    /// Serialises the workspace as pretty JSON.
    pub fn to_json(&self) -> Result<String, SettingsError> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Clamps every ranged value and keeps the recent list bounded.
    pub fn sanitize(&mut self) {
        self.version = SETTINGS_VERSION;
        self.appearance.ui_scale = self.appearance.ui_scale.clamp(ui_scale::MIN_PERCENT, ui_scale::MAX_PERCENT);
        self.appearance.panel_opacity = self.appearance.panel_opacity.clamp(MIN_PANEL_OPACITY, MAX_PANEL_OPACITY);
        self.viewport.grid_step_mm = self.viewport.grid_step_mm.clamp(MIN_GRID_STEP_MM, MAX_GRID_STEP_MM);
        self.viewport.grid_strength = self.viewport.grid_strength.min(MAX_DISPLAY_STRENGTH);
        self.viewport.edge_strength = self.viewport.edge_strength.min(MAX_DISPLAY_STRENGTH);
        self.viewport.shadow_strength = self.viewport.shadow_strength.min(MAX_DISPLAY_STRENGTH);
        self.viewport.floor_opacity = self.viewport.floor_opacity.clamp(MIN_FLOOR_OPACITY, MAX_FLOOR_OPACITY);
        self.viewport.roll_step_degrees =
            self.viewport.roll_step_degrees.clamp(MIN_ROLL_STEP_DEGREES, MAX_ROLL_STEP_DEGREES);
        self.gizmo.handle_points = self.gizmo.handle_points.clamp(MIN_HANDLE_POINTS, MAX_HANDLE_POINTS);
        self.gizmo.ring_percent = self.gizmo.ring_percent.clamp(MIN_RING_PERCENT, MAX_RING_PERCENT);
        self.snaps.tolerance_mm = self.snaps.tolerance_mm.clamp(MIN_SNAP_MM, MAX_SNAP_MM);
        self.pointer.cursor_size = self.pointer.cursor_size.clamp(MIN_CURSOR_POINTS, MAX_CURSOR_POINTS);
        let mut seen = Vec::new();
        self.recent.retain(|path| {
            let fresh = !seen.contains(path);
            if fresh {
                seen.push(path.clone());
            }
            fresh
        });
        self.recent.truncate(MAX_RECENT);
    }

    /// Moves `path` to the front of the recent list.
    pub fn remember(&mut self, path: &Path) {
        self.recent.retain(|candidate| candidate != path);
        self.recent.insert(0, path.to_path_buf());
        self.recent.truncate(MAX_RECENT);
    }

    /// The theme, palette and accent as the values `theme::install_with` takes.
    pub const fn install_args(&self) -> (egui::ThemePreference, theme::ThemePreset, Option<[u8; 3]>) {
        (self.appearance.theme.preference(), self.appearance.palette, self.appearance.accent)
    }
}

/// Rewrites the stored palette name to the one this build writes, and reports a name it does not
/// know.
///
/// The stored name is read out of the JSON tree before the typed read, so one unknown name costs
/// one value and not the whole file. A value that is not a string at all is left for `serde` to
/// reject: that is a corrupt file, not an older one. A name an earlier build wrote for a look this
/// build has renamed resolves to the current name and is rewritten, so a saved workspace keeps its
/// look and the next save carries the current word (T8, ADR 0125).
fn canonical_palette(value: &mut serde_json::Value) -> Option<Repair> {
    let slot = value.get_mut("appearance")?.get_mut("palette")?;
    let serde_json::Value::String(name) = slot else {
        return None;
    };
    let preset = theme::ThemePreset::from_stored_name(name);
    let unknown = preset.is_none().then(|| name.clone());
    *slot = serde_json::Value::String(preset.unwrap_or_default().stored_name().to_owned());
    unknown.map(Repair::UnknownPalette)
}

/// Drops the accent a workspace written before U71 carried.
///
/// Before the presets there was one palette, so every file stored its accent whether or not the user
/// picked it; keeping that value would paint P1 graphite·copper in the old blue. A file without a
/// palette therefore starts on the accent its palette names, while an accent written by this build
/// is an override and is kept.
fn migrate_accent(value: &mut serde_json::Value) {
    let Some(appearance) = value.get_mut("appearance").and_then(serde_json::Value::as_object_mut) else {
        return;
    };
    if appearance.contains_key("palette") {
        return;
    }
    if appearance.contains_key("accent") {
        appearance.insert("accent".to_owned(), serde_json::Value::Null);
    }
}

/// Which section of the settings window is open.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SettingsSection {
    /// Theme, palette, accent and UI scale.
    #[default]
    Appearance,
    /// Display style, grid, edges and opening view.
    Viewport,
    /// Which side panels are shown and how opaque the chrome is.
    Panels,
    /// The shortcut table.
    Shortcuts,
    /// The assistant: the local control channel an agent attaches to.
    Assistant,
}

impl SettingsSection {
    /// Every section in the order the window lists them.
    pub const ALL: [Self; 5] = [Self::Appearance, Self::Viewport, Self::Panels, Self::Shortcuts, Self::Assistant];

    /// The user-visible name.
    pub const fn label(self) -> Message {
        match self {
            Self::Appearance => Message::SectionAppearance,
            Self::Viewport => Message::SectionViewport,
            Self::Panels => Message::SectionPanels,
            Self::Shortcuts => Message::SectionShortcuts,
            Self::Assistant => Message::SectionAssistant,
        }
    }
}

/// What the settings window reports back to the shell.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SettingsOutcome {
    /// A value changed and has to be applied and persisted.
    pub changed: bool,
    /// The window asked to close.
    pub close: bool,
    /// The address whose model list the page asked for, if the button was pressed this frame.
    ///
    /// The page cannot ask: `ureq` lives in `mcp` and this crate may not depend on it, so the shell
    /// takes the address, lists it off the frame and hands the rows back in [`ListedModels`]
    /// (ADR 0235).
    pub list_models: Option<String>,
}

/// Draws the Settings modal and returns whether a value changed or the window closed.
///
/// The window never writes a file: the shell applies the change at once and owns the atomic save.
/// Escape and a click on the backdrop close it, like every other modal of the application.
///
/// `models` is what the shell has listed for the address the page shows; the page draws it and asks
/// for another listing, and it never waits for one.
pub fn show(
    ctx: &Context,
    settings: &mut Settings,
    section: &mut SettingsSection,
    models: &ListedModels,
) -> SettingsOutcome {
    let mut outcome = SettingsOutcome::default();
    let lang = settings.appearance.language.lang();
    let response = Modal::new(Id::unique("settings_window")).show(ctx, |ui| {
        ui.set_width(760.0);
        ui.horizontal(|ui| {
            ui.heading(crate::text(ui.ctx(), Message::SettingsTitle));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button(crate::text(ui.ctx(), Message::Done)).clicked() {
                    outcome.close = true;
                }
            });
        });
        ui.separator();
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(150.0);
                for entry in SettingsSection::ALL {
                    ui.selectable_value(section, entry, entry.label().text(lang));
                }
            });
            ui.separator();
            ui.vertical(|ui| {
                ui.set_min_width(540.0);
                ui.set_min_height(420.0);
                outcome.changed |= match *section {
                    SettingsSection::Appearance => appearance_ui(ui, settings, lang),
                    SettingsSection::Viewport => viewport_ui(ui, settings, lang),
                    SettingsSection::Panels => panels_ui(ui, settings, lang),
                    SettingsSection::Shortcuts => shortcuts_ui(ui, lang),
                    SettingsSection::Assistant => assistant_ui(ui, settings, models, lang, &mut outcome.list_models),
                };
            });
        });
    });
    if response.should_close() {
        outcome.close = true;
    }
    if ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        outcome.close = true;
    }
    outcome
}

/// The *Appearance* page: theme, palette, accent, opacity, icons, language and scale.
fn appearance_ui(ui: &mut Ui, settings: &mut Settings, lang: Lang) -> bool {
    let mut changed = false;
    ui.heading(Message::SectionAppearance.text(lang));
    ui.label(egui::RichText::new(Message::Theme.text(lang)).color(theme::text_dim(ui)));
    ui.horizontal(|ui| {
        for choice in ThemeChoice::ALL {
            changed |= ui.radio_value(&mut settings.appearance.theme, choice, choice.label().text(lang)).changed();
        }
    });
    ui.add_space(theme::STEP);
    ui.label(egui::RichText::new(Message::Palette.text(lang)).color(theme::text_dim(ui)));
    ui.horizontal(|ui| {
        for preset in theme::ThemePreset::ALL {
            changed |= preset_card(ui, settings, preset, lang);
        }
    });
    ui.label(egui::RichText::new(Message::PaletteHint.text(lang)).small().color(theme::text_dim(ui)));
    ui.add_space(theme::STEP);
    ui.horizontal(|ui| {
        ui.label(Message::Accent.text(lang));
        // The button shows the colour the window actually draws with, so a palette's own accent is
        // the starting point rather than a placeholder; a pick replaces it for both variants.
        let drawn = settings.appearance.drawn_accent(ui.visuals().dark_mode).to_array();
        let mut bytes = [drawn[0], drawn[1], drawn[2]];
        // The colour well sizes itself from `interact_size`, which this theme keeps at zero width so
        // a text button is as wide as its caption; a well has to be told it is a square.
        let picked = ui
            .scope(|ui| {
                ui.spacing_mut().interact_size = egui::vec2(theme::HEADER_CONTROL_HEIGHT, theme::HEADER_CONTROL_HEIGHT);
                ui.color_edit_button_srgb(&mut bytes)
            })
            .inner;
        if picked.changed() {
            settings.appearance.accent = Some(bytes);
            changed = true;
        }
    })
    .response
    .on_hover_text(Message::AccentHint.text(lang));
    ui.add_space(theme::STEP);
    ui.horizontal(|ui| {
        ui.label(Message::IconColor.text(lang));
        for mode in IconColor::ALL {
            changed |= ui.radio_value(&mut settings.appearance.icon_color, mode, mode.label().text(lang)).changed();
        }
    });
    ui.label(egui::RichText::new(Message::IconColorHint.text(lang)).small().color(theme::text_dim(ui)));
    ui.add_space(theme::STEP);
    ui.label(egui::RichText::new(Message::Language.text(lang)).color(theme::text_dim(ui)));
    ui.horizontal(|ui| {
        for choice in Language::ALL {
            changed |= ui.radio_value(&mut settings.appearance.language, choice, choice.label().text(lang)).changed();
        }
    });
    ui.add_space(theme::STEP);
    // The control edits `egui`'s zoom factor and writes back the percentage it applied; the stored
    // scale follows the live value, so the next launch opens at the scale the user sees.
    changed |= ui_scale::show(ui, &mut settings.appearance.ui_scale).changed();
    ui.label(egui::RichText::new(Message::ScaleHint.text(lang)).small().color(theme::text_dim(ui)));
    ui.add_space(theme::STEP * 2.0);
    // Reset is the way back from a colour picked by hand; it keeps the language and the scale, which
    // are not part of the look and which the user tuned for their own screen.
    if ui.button(Message::ResetAppearance.text(lang)).on_hover_text(Message::ResetAppearanceHint.text(lang)).clicked() {
        settings.appearance = Appearance {
            language: settings.appearance.language,
            ui_scale: settings.appearance.ui_scale,
            ..Appearance::default()
        };
        changed = true;
    }
    changed
}

/// One palette card: the sweeps of the preset, its name, and a border that marks the active one.
///
/// The card is painted in the preset's own colours for the variant the window draws, so the two
/// cards that are not active still show what choosing them would do. Picking one drops the accent
/// override, because the accent on the card is part of the look it names.
fn preset_card(ui: &mut Ui, settings: &mut Settings, preset: theme::ThemePreset, lang: Lang) -> bool {
    let ramp = preset.palette(ui.visuals().dark_mode);
    let selected = settings.appearance.palette == preset;
    let stroke =
        egui::Stroke::new(if selected { 2.0 } else { 1.0 }, if selected { theme::accent(ui) } else { theme::line(ui) });
    let card = egui::Frame::new()
        .fill(ramp.panel)
        .stroke(stroke)
        .corner_radius(egui::CornerRadius::same(theme::RADIUS))
        .inner_margin(egui::Margin::same(6))
        .show(ui, |ui| {
            ui.set_min_width(96.0);
            ui.horizontal(|ui| {
                for fill in [ramp.chrome, ramp.control, ramp.text, ramp.accent] {
                    swatch(ui, fill);
                }
            });
            ui.label(egui::RichText::new(preset.label().text(lang)).small().color(ramp.text));
        });
    if !card.response.interact(egui::Sense::click()).clicked() {
        return false;
    }
    if selected {
        return false;
    }
    settings.appearance.palette = preset;
    settings.appearance.accent = None;
    true
}

/// A small rectangle of one colour, so a card shows the palette it names.
fn swatch(ui: &mut Ui, fill: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(16.0, 10.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, egui::CornerRadius::same(2), fill);
}

/// The *Assistant* page: the door an agent attaches to, the process it starts, the endpoint it asks
/// and the endpoints a person has agreed to send to.
///
/// The page names both places a support question lands — the channel switch and the token file in the
/// settings folder — because a person answering "my agent cannot see the window" has to know which of
/// the two is the door (ADR 0188). It also names the key's two homes and the one file the key is
/// never in, so "why is it not configured" has an answer a person can read (ADR 0189). Nothing here
/// reaches the network by itself: the channel binds loopback, and a message leaves only after the
/// panel has asked once per endpoint.
fn assistant_ui(
    ui: &mut Ui,
    settings: &mut Settings,
    models: &ListedModels,
    lang: Lang,
    list_models: &mut Option<String>,
) -> bool {
    ui.heading(Message::SectionAssistant.text(lang));
    ui.label(Message::AssistantWays.text(lang));

    // The address first: it is the path a person without an agent installation takes, and the
    // process is the advanced one. A single field per line and a full-width box keep a pasted URL
    // visible, which is where the doubled address of J22 came from.
    ui.add_space(theme::STEP * 2.0);
    ui.label(egui::RichText::new(Message::AssistantByAddress.text(lang)).strong());
    let mut changed = provider_ui(ui, settings, lang);
    ui.label(egui::RichText::new(Message::AssistantProviderHint.text(lang)).small().color(theme::text_dim(ui)));
    ui.label(Message::AssistantEndpoint.text(lang));
    changed |= ui
        .add(
            egui::TextEdit::singleline(&mut settings.assistant.endpoint)
                .hint_text(Message::AssistantEndpointHint.text(lang))
                .desired_width(ui.available_width()),
        )
        .changed();
    if let Some(problem) = settings.assistant.endpoint_problem() {
        let sentence = Message::AssistantEndpointProblem.with(lang, [problem.message().text(lang)]);
        ui.label(egui::RichText::new(sentence).color(theme::warn(ui)));
    } else {
        ui.label(egui::RichText::new(Message::AssistantEndpointHint.text(lang)).small().color(theme::text_dim(ui)));
    }
    // The model and its listing share a row: the field is still the truth (a model the endpoint does
    // not list yet is typed here), and the button asks the address what it answers. The picker below
    // is drawn from what came back and writes the field, so a person chooses an id they cannot
    // misspell instead of one they remember (ADR 0235).
    let mut listed = false;
    ui.horizontal(|ui| {
        ui.label(Message::AssistantModel.text(lang));
        let field = (ui.available_width() - 96.0).max(160.0);
        changed |= ui
            .add(
                egui::TextEdit::singleline(&mut settings.assistant.model)
                    .hint_text(Message::AssistantModelHint.text(lang))
                    .desired_width(field),
            )
            .changed();
        let askable = settings.assistant.endpoint_problem().is_none() && !settings.assistant.endpoint.trim().is_empty();
        let button = ui.add_enabled(askable, egui::Button::new(Message::AssistantModelsList.text(lang)));
        let button = button.on_hover_text(Message::AssistantModelsListHint.text(lang));
        if button.clicked() {
            listed = true;
        }
    });
    if listed {
        *list_models = Some(settings.assistant.endpoint.trim().to_owned());
    }
    let address = settings.assistant.endpoint.trim();
    if let Some(note) = models.note(address, lang) {
        let ink = if models.failed.is_some() && models.is_for(address) { theme::warn(ui) } else { theme::text_dim(ui) };
        ui.label(egui::RichText::new(note).small().color(ink));
    }
    if models.is_for(address) && !models.rows.is_empty() {
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("settings_assistant_models")
                .selected_text(Message::AssistantModelsPick.text(lang))
                .show_ui(ui, |ui| {
                    for row in &models.rows {
                        let label = if row.name == row.id {
                            row.id.clone()
                        } else {
                            Message::AssistantModelNamed.with(lang, [row.name.clone(), row.id.clone()])
                        };
                        changed |= ui.selectable_value(&mut settings.assistant.model, row.id.clone(), label).changed();
                    }
                });
        });
    }
    ui.label(egui::RichText::new(Message::AssistantModelHint.text(lang)).small().color(theme::text_dim(ui)));
    ui.label(egui::RichText::new(Message::AssistantKeyHint.text(lang)).small().color(theme::text_dim(ui)));
    ui.label(egui::RichText::new(Message::AssistantWhatNext.text(lang)).small().color(theme::text_dim(ui)));

    ui.add_space(theme::STEP * 2.0);
    ui.label(egui::RichText::new(Message::AssistantByProcess.text(lang)).strong());
    changed |= ui
        .checkbox(&mut settings.assistant.attach, Message::AttachChannel.text(lang))
        .on_hover_text(Message::AttachChannelHint.text(lang))
        .changed();
    ui.label(egui::RichText::new(Message::AttachChannelHint.text(lang)).small().color(theme::text_dim(ui)));
    ui.label(Message::AgentCommand.text(lang));
    changed |= ui
        .add(
            egui::TextEdit::singleline(&mut settings.assistant.agent)
                .hint_text(Message::AgentCommandHint.text(lang))
                .desired_width(ui.available_width()),
        )
        .changed();
    ui.label(egui::RichText::new(Message::AgentCommandHint.text(lang)).small().color(theme::text_dim(ui)));
    ui.label(egui::RichText::new(Message::AssistantProcessWins.text(lang)).small().color(theme::text_dim(ui)));

    ui.add_space(theme::STEP * 2.0);
    ui.label(Message::AssistantConsentList.text(lang));
    if settings.assistant.consent.is_empty() {
        ui.label(egui::RichText::new(Message::AssistantConsentNone.text(lang)).small().color(theme::text_dim(ui)));
    }
    // The list is read before it is written: a forget clicked this frame lands after the rows that
    // drew, so the row under the pointer is the row that disappears and no other. A row is also the
    // fastest way back from a mistyped address: *Use again* puts the remembered one in the field,
    // which is the first-run help a workshop already has without naming a provider in code.
    let mut forget = None;
    let mut again = None;
    for endpoint in &settings.assistant.consent {
        ui.horizontal(|ui| {
            ui.label(endpoint.as_str());
            if ui.button(Message::AssistantConsentUse.text(lang)).clicked() {
                again = Some(endpoint.clone());
            }
            if ui.button(Message::AssistantConsentForget.text(lang)).clicked() {
                forget = Some(endpoint.clone());
            }
        });
    }
    if let Some(endpoint) = again {
        settings.assistant.endpoint = endpoint;
        changed = true;
    }
    if let Some(endpoint) = forget {
        settings.assistant.forget_consent(&endpoint);
        changed = true;
    }
    changed
}

/// The preset row: a preset writes its address into the endpoint field, and *Other address* leaves
/// the field exactly as it is.
///
/// The row is derived from the field on every read (`AssistantSettings::preset`), so it never holds a
/// choice of its own: a pinned preset in the settings file would one day disagree with the address
/// beside it, and the field is the value the request is built from (ADR 0235).
fn provider_ui(ui: &mut Ui, settings: &mut Settings, lang: Lang) -> bool {
    ui.label(Message::AssistantProvider.text(lang));
    let before = settings.assistant.preset();
    let shown = match before.and_then(|index| PRESETS.get(index)) {
        Some(preset) => preset.name.text(lang),
        None => Message::AssistantProviderCustom.text(lang),
    };
    let mut chosen = before;
    egui::ComboBox::from_id_salt("settings_assistant_provider").width(260.0).selected_text(shown).show_ui(ui, |ui| {
        for (index, preset) in PRESETS.iter().enumerate() {
            ui.selectable_value(&mut chosen, Some(index), preset.name.text(lang));
        }
        ui.selectable_value(&mut chosen, None, Message::AssistantProviderCustom.text(lang));
    });
    let Some(preset) = chosen.and_then(|index| PRESETS.get(index)) else {
        return false;
    };
    if chosen == before {
        return false;
    }
    settings.assistant.endpoint = preset.base_url.to_owned();
    true
}

/// The *Panels* page: the two floating side panels and the opacity they share with the chrome.
fn panels_ui(ui: &mut Ui, settings: &mut Settings, lang: Lang) -> bool {
    let mut changed = false;
    ui.heading(Message::SectionPanels.text(lang));
    changed |= ui.checkbox(&mut settings.panels.scene.visible, Message::ScenePanelVisible.text(lang)).changed();
    changed |= ui.checkbox(&mut settings.panels.inspector.visible, Message::InspectorPanelVisible.text(lang)).changed();
    ui.add_space(theme::STEP);
    ui.horizontal(|ui| {
        ui.label(Message::PanelOpacity.text(lang));
        changed |= ui
            .add(
                egui::Slider::new(&mut settings.appearance.panel_opacity, MIN_PANEL_OPACITY..=MAX_PANEL_OPACITY)
                    .suffix(" %"),
            )
            .changed();
    });
    ui.label(egui::RichText::new(Message::PanelOpacityHint.text(lang)).small().color(theme::text_dim(ui)));
    ui.add_space(theme::STEP);
    ui.label(egui::RichText::new(Message::PanelsHint.text(lang)).small().color(theme::text_dim(ui)));
    ui.add_space(theme::STEP * 2.0);
    if ui.button(Message::ResetPanels.text(lang)).clicked() {
        settings.panels = PanelsSettings::default();
        changed = true;
    }
    changed
}

/// The *Viewport* page: the defaults a new viewport opens with.
fn viewport_ui(ui: &mut Ui, settings: &mut Settings, lang: Lang) -> bool {
    let mut changed = false;
    ui.heading(Message::SectionViewport.text(lang));
    ui.horizontal(|ui| {
        ui.label(Message::DisplayStyle.text(lang));
        egui::ComboBox::from_id_salt("settings_style")
            .selected_text(shading_label(settings.viewport.style, lang))
            .show_ui(ui, |ui| {
                for style in ShadingStyle::ALL {
                    changed |=
                        ui.selectable_value(&mut settings.viewport.style, style, shading_label(style, lang)).changed();
                }
            });
    });
    ui.horizontal(|ui| {
        ui.label(Message::OpeningView.text(lang));
        egui::ComboBox::from_id_salt("settings_opening_view")
            .selected_text(settings.viewport.opening_view.label(lang))
            .show_ui(ui, |ui| {
                for view in OpeningView::ALL {
                    changed |=
                        ui.selectable_value(&mut settings.viewport.opening_view, view, view.label(lang)).changed();
                }
            });
    });
    ui.add_space(theme::STEP);
    ui.horizontal(|ui| {
        ui.label(Message::RollRingStep.text(lang));
        changed |= ui
            .add(
                Scrub::new(&mut settings.viewport.roll_step_degrees)
                    .range(MIN_ROLL_STEP_DEGREES..=MAX_ROLL_STEP_DEGREES)
                    .unit("°")
                    .speed(0.2),
            )
            .on_hover_text(Message::RollRingStepHint.text(lang))
            .changed();
    });
    // The transform gizmos are sized by hand and apply at once, so they get their own heading
    // inside the view page instead of widening the viewport defaults above (U86).
    ui.add_space(theme::STEP);
    ui.label(egui::RichText::new(Message::Gizmo.text(lang)).strong());
    ui.horizontal(|ui| {
        ui.label(Message::GizmoHandleLength.text(lang));
        changed |= ui
            .add(
                Scrub::new(&mut settings.gizmo.handle_points)
                    .range(MIN_HANDLE_POINTS..=MAX_HANDLE_POINTS)
                    .unit("pt")
                    .speed(2.0),
            )
            .on_hover_text(Message::GizmoHandleLengthHint.text(lang))
            .changed();
    });
    ui.horizontal(|ui| {
        ui.label(Message::GizmoRingRadius.text(lang));
        changed |= ui
            .add(
                Scrub::new(&mut settings.gizmo.ring_percent)
                    .range(MIN_RING_PERCENT..=MAX_RING_PERCENT)
                    .unit("%")
                    .speed(0.2),
            )
            .on_hover_text(Message::GizmoRingRadiusHint.text(lang))
            .changed();
    });
    // The pointer wears its own size and its own crosshair switch, beside the gizmos it shares a
    // hand with (U88).
    ui.add_space(theme::STEP);
    ui.label(egui::RichText::new(Message::WorkCursor.text(lang)).strong());
    ui.horizontal(|ui| {
        ui.label(Message::CursorSize.text(lang));
        changed |= ui
            .add(
                Scrub::new(&mut settings.pointer.cursor_size)
                    .range(MIN_CURSOR_POINTS..=MAX_CURSOR_POINTS)
                    .unit("pt")
                    .speed(0.5),
            )
            .on_hover_text(Message::CursorSizeHint.text(lang))
            .changed();
    });
    changed |= ui
        .checkbox(&mut settings.pointer.crosshair, Message::CursorCrosshair.text(lang))
        .on_hover_text(Message::CursorCrosshairHint.text(lang))
        .changed();
    ui.label(egui::RichText::new(Message::CursorHint.text(lang)).small().color(theme::text_dim(ui)));
    changed |= ui.checkbox(&mut settings.viewport.grid, Message::ShowFloorGrid.text(lang)).changed();
    changed |= ui
        .add(
            Scrub::new(&mut settings.viewport.grid_step_mm)
                .range(MIN_GRID_STEP_MM..=MAX_GRID_STEP_MM)
                .unit("mm")
                .speed(5.0),
        )
        .changed();
    // Snapping sits beside the grid because it is the other half of the same question: what a
    // piece lands on when it is dragged (N21, ADR 0146).
    ui.add_space(theme::STEP);
    ui.label(egui::RichText::new(Message::Snapping.text(lang)).strong());
    // How a capture reads a corner above the plane is the section's first choice: the switches
    // below decide what may capture, this decides what the capture is (U119).
    ui.horizontal(|ui| {
        ui.label(Message::SnapMode.text(lang));
        for mode in model::SnapMode::ALL {
            changed |= ui
                .radio_value(&mut settings.snaps.mode, mode, crate::snap::mode_word(mode).text(lang))
                .on_hover_text(crate::snap::mode_hint(mode).text(lang))
                .changed();
        }
    });
    changed |= ui
        .checkbox(&mut settings.snaps.planes, Message::SnapPlanes.text(lang))
        .on_hover_text(Message::SnapPlanesHint.text(lang))
        .changed();
    changed |= ui
        .checkbox(&mut settings.snaps.frames, Message::SnapFrames.text(lang))
        .on_hover_text(Message::SnapFramesHint.text(lang))
        .changed();
    // The pointer's kinds, one row each: the same ten the View tab's Snaps group switches, so a
    // maker who learns the set in one place finds it in the other (U116). The coordinate planes
    // above are the `axis` kind and are not repeated here.
    for kind in model::SnapKind::ALL.into_iter().filter(|kind| *kind != model::SnapKind::Axis) {
        let slot = match kind {
            model::SnapKind::Vertex => &mut settings.snaps.kinds.vertex,
            model::SnapKind::Intersection => &mut settings.snaps.kinds.intersection,
            model::SnapKind::Midpoint => &mut settings.snaps.kinds.midpoint,
            model::SnapKind::Centre => &mut settings.snaps.kinds.centre,
            model::SnapKind::Perpendicular => &mut settings.snaps.kinds.perpendicular,
            model::SnapKind::Tangent => &mut settings.snaps.kinds.tangent,
            model::SnapKind::Edge => &mut settings.snaps.kinds.edge,
            model::SnapKind::Face => &mut settings.snaps.kinds.face,
            model::SnapKind::Grid => &mut settings.snaps.kinds.grid,
            model::SnapKind::Axis => continue,
        };
        changed |= ui
            .checkbox(slot, crate::snap::word(kind).text(lang))
            .on_hover_text(crate::snap::hint(kind).text(lang))
            .changed();
    }
    ui.horizontal(|ui| {
        ui.label(Message::SnapTolerance.text(lang));
        changed |= ui
            .add(Scrub::new(&mut settings.snaps.tolerance_mm).range(MIN_SNAP_MM..=MAX_SNAP_MM).unit("mm").speed(0.5))
            .on_hover_text(Message::SnapToleranceHint.text(lang))
            .changed();
    });
    // The floor follows the captures because it answers the same question, and is one line apart
    // from them because it is not a capture: a body is not carried under the bench by accident, and
    // a workspace with no floor says so here (X87, ADR 0177).
    changed |= ui
        .checkbox(&mut settings.snaps.floor, Message::FloorStop.text(lang))
        .on_hover_text(Message::FloorStopHint.text(lang))
        .changed();
    ui.separator();
    // The strengths the floating cluster used to draw: they describe how a view is painted, so
    // they live with the other display defaults and the cluster keeps only its switches (U64). The
    // shadow's intensity is here alone: its *switch* is the view's, which is why the View tab draws
    // the switch and no number (U107, U110). The floor's opacity joined them because the ground is
    // painted, not modelled: it is the fourth answer to "how is this view drawn" (U109).
    ui.label(egui::RichText::new(Message::DisplayHeading.text(lang)).strong());
    for (value, label, hint) in [
        (&mut settings.viewport.grid_strength, Message::GridStrength, Message::GridStrengthHint),
        (&mut settings.viewport.edge_strength, Message::EdgesStrength, Message::EdgesStrengthHint),
        (&mut settings.viewport.shadow_strength, Message::ShadowStrength, Message::ShadowStrengthHint),
    ] {
        changed |= ui
            .add(egui::Slider::new(value, 0..=MAX_DISPLAY_STRENGTH).text(label.text(lang)))
            .on_hover_text(hint.text(lang))
            .changed();
    }
    // The floor's own value has its own endpoints, because zero means something here that it does
    // not mean for a strength: no plate and no shadow at all (U109).
    changed |= ui
        .add(
            egui::Slider::new(&mut settings.viewport.floor_opacity, MIN_FLOOR_OPACITY..=MAX_FLOOR_OPACITY)
                .text(Message::FloorOpacity.text(lang)),
        )
        .on_hover_text(Message::FloorOpacityHint.text(lang))
        .changed();
    ui.horizontal(|ui| {
        if ui.button(Message::ResetDisplay.text(lang)).on_hover_text(Message::ResetDisplayHint.text(lang)).clicked() {
            settings.viewport.grid_strength = DEFAULT_GRID_STRENGTH;
            settings.viewport.edge_strength = DEFAULT_EDGE_STRENGTH;
            settings.viewport.shadow_strength = DEFAULT_SHADOW_STRENGTH;
            settings.viewport.floor_opacity = DEFAULT_FLOOR_OPACITY;
            changed = true;
        }
        ui.label(egui::RichText::new(Message::ResetDisplayHint.text(lang)).small().color(theme::text_dim(ui)));
    });
    ui.separator();
    // A diagnostic, not a user setting: it draws the triangulation diagonals the form-edge view
    // hides, so it sits behind the one fold a user has to open (U64).
    egui::CollapsingHeader::new(Message::SectionAdditional.text(lang)).default_open(false).show(ui, |ui| {
        changed |= ui
            .checkbox(&mut settings.viewport.all_edges, Message::ShowEveryTriangleEdge.text(lang))
            .on_hover_text(Message::AllEdgesHint.text(lang))
            .changed();
    });
    ui.label(egui::RichText::new(Message::ViewportDefaultsHint.text(lang)).small().color(theme::text_dim(ui)));
    changed
}

/// The *Shortcuts* page: the keys the shell already listens for.
fn shortcuts_ui(ui: &mut Ui, lang: Lang) -> bool {
    ui.heading(Message::SectionShortcuts.text(lang));
    ui.label(egui::RichText::new(Message::ShortcutsHint.text(lang)).small().color(theme::text_dim(ui)));
    egui::ScrollArea::vertical().max_height(280.0).show(ui, |ui| {
        egui::Grid::new("settings_shortcuts").num_columns(2).striped(true).show(ui, |ui| {
            for (key, action) in shortcut_table(lang) {
                ui.label(egui::RichText::new(key).strong());
                ui.label(action);
                ui.end_row();
            }
        });
    });
    false
}

/// The shortcut table, walked from the inventory's one key table (U96).
///
/// A row is a binding: the label the table prints and the words the id already carries — a command's
/// own label, a cell's or a method's name from the ribbon map, or a shell action's description. The
/// gestures a mouse carries are not keys and stay here, after the bindings, because no table binds
/// them and the page still has to say they exist.
pub fn shortcut_table(lang: Lang) -> Vec<(String, String)> {
    let mut table: Vec<(String, String)> = crate::inventory::KEYS
        .iter()
        .filter_map(|binding| {
            let words = crate::inventory::words(binding.id)?;
            Some((binding.label.to_owned(), words.text(lang).to_owned()))
        })
        .collect();
    let gestures: [(&str, Message); 4] = [
        ("Esc", Message::ShortcutCancel),
        ("MMB", Message::ShortcutOrbit),
        ("Shift+MMB", Message::ShortcutPan),
        ("Ctrl+MMB", Message::ShortcutZoom),
    ];
    for (gesture, message) in gestures {
        table.push((gesture.to_owned(), message.text(lang).to_owned()));
    }
    table
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use model::{Container, Id, Placement, Project};

    use super::*;

    /// ADR 0235: the assistant section is the address, the model, the process, the channel switch and
    /// the addresses a person agreed to — and nothing else. A header field, a pinned provider or a key
    /// would have to appear here first, which is what makes this a test rather than a remark.
    #[test]
    fn the_assistant_section_carries_no_header_and_no_key() {
        let json = serde_json::to_value(AssistantSettings::default()).expect("the section serializes");
        let mut keys: Vec<&str> =
            json.as_object().expect("a section is an object").keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["agent", "attach", "consent", "endpoint", "model"], "the section names no secret: {json}");
    }

    /// Every preset is one base URL the page accepts, and the row is derived from the field rather
    /// than remembered beside it, so what a person sees is what a request would use (ADR 0235).
    #[test]
    fn every_preset_is_one_address_the_page_accepts() {
        assert_eq!(PRESETS.len(), 6, "the table the owner asked for, and no more");
        for preset in PRESETS {
            let url = preset.base_url;
            assert!(url.starts_with("http://") || url.starts_with("https://"), "{url}");
            assert!(
                !url.ends_with('/'),
                "the client appends `/chat/completions`, so a trailing slash is a doubled one: {url}"
            );
            assert!(endpoint_problem(url).is_none(), "a preset is an address the page itself accepts: {url}");
            assert!(!preset.name.text(Lang::En).is_empty(), "every preset is named");
        }
        // *Other address* is the person's own field: a typed address is no preset, and the row says so
        // without touching what was typed.
        let typed =
            AssistantSettings { endpoint: "https://api.example.com/v1".to_owned(), ..AssistantSettings::default() };
        assert_eq!(typed.preset(), None, "a typed address is not a preset");
        let chosen = AssistantSettings { endpoint: PRESETS[0].base_url.to_owned(), ..AssistantSettings::default() };
        assert_eq!(chosen.preset(), Some(0), "a preset address is recognised with the row");
    }

    fn custom() -> Settings {
        Settings {
            version: SETTINGS_VERSION,
            appearance: Appearance {
                theme: ThemeChoice::Light,
                language: Language::Russian,
                palette: theme::ThemePreset::InkMagenta,
                accent: Some([0x11, 0x99, 0x44]),
                panel_opacity: 70,
                icon_color: IconColor::Never,
                ui_scale: 125,
            },
            viewport: ViewportSettings {
                style: ShadingStyle::Wireframe,
                grid: false,
                grid_step_mm: 25,
                grid_strength: 30,
                edge_strength: 80,
                shadow_strength: 55,
                floor_opacity: 45,
                all_edges: true,
                opening_view: OpeningView::Top,
                roll_step_degrees: 5,
            },
            gizmo: GizmoSettings { handle_points: 48, ring_percent: 7 },
            snaps: SnapSettings {
                planes: false,
                frames: true,
                tolerance_mm: 12,
                floor: false,
                kinds: PointerKinds { grid: false, ..PointerKinds::default() },
                mode: model::SnapMode::Foot,
            },
            pointer: PointerSettings { cursor_size: 20, crosshair: true },
            recent: vec![PathBuf::from("/tmp/cabinet.z3l"), PathBuf::from("/tmp/shelf.z3l")],
            ribbon: RibbonSettings {
                open: false,
                pinned_variants: BTreeMap::from([(
                    "sketch.create.circle".to_owned(),
                    "sketch.create.circle.three-points".to_owned(),
                )]),
            },
            panels: PanelsSettings {
                scene: PanelState { visible: false, expanded: true },
                inspector: PanelState { visible: true, expanded: false },
            },
            assistant: AssistantSettings {
                attach: false,
                agent: String::new(),
                endpoint: "https://api.example.com/v1".to_owned(),
                model: "a-model".to_owned(),
                consent: vec!["https://api.example.com/v1".to_owned()],
            },
        }
    }

    #[test]
    fn a_round_trip_through_json_restores_every_field() {
        let settings = custom();
        let json = settings.to_json().expect("serialise");
        let restored = Settings::from_json(&json).expect("parse");
        assert_eq!(restored, settings);
        assert_eq!(restored.appearance.palette, theme::ThemePreset::InkMagenta);
        assert_eq!(restored.appearance.accent, Some([0x11, 0x99, 0x44]));
        assert_eq!(restored.appearance.panel_opacity, 70);
        assert_eq!(restored.panels.scene, PanelState { visible: false, expanded: true });
        assert_eq!(restored.panels.inspector, PanelState { visible: true, expanded: false });
        assert_eq!(restored.appearance.icon_color, IconColor::Never);
        assert_eq!(restored.gizmo, GizmoSettings { handle_points: 48, ring_percent: 7 });
        assert_eq!(
            restored.snaps,
            SnapSettings {
                planes: false,
                frames: true,
                tolerance_mm: 12,
                floor: false,
                kinds: PointerKinds { grid: false, ..PointerKinds::default() },
                mode: model::SnapMode::Foot,
            }
        );
    }

    #[test]
    fn the_snap_mode_survives_a_restart_and_a_file_written_before_it_existed_keeps_the_foot() {
        // 3D is the other reading a maker asks for, so the setting writes it and the file carries it
        // back; the whole section is one value, so the mode is one of its switches (U119).
        let mut settings = Settings::default();
        settings.snaps.mode = model::SnapMode::Space;
        let json = settings.to_json().expect("serialise");
        let restored = Settings::from_json(&json).expect("parse");
        assert_eq!(restored.snaps.mode, model::SnapMode::Space, "the mode survives a restart");
        assert_eq!(restored.snaps.mode.target(), model::SnapTarget::InPlane, "and answers as the 3D reading");

        // A file written before the mode existed lands in the default, the 2.5D reading: a top view
        // lines a board up with its shadow, which is the work a furniture maker does there.
        let old = Settings::from_json(r#"{"version":1,"snaps":{"planes":true}}"#).expect("parse");
        assert_eq!(old.snaps.mode, model::SnapMode::Foot, "2.5D is the reading a fresh workspace starts in");
        assert_eq!(old.snaps.mode.target(), model::SnapTarget::Foot);
    }

    #[test]
    fn the_channel_switch_is_on_for_a_workspace_that_never_heard_of_it() {
        // A file from before the assistant section exists opens with the channel on: the default is
        // the feature, and the switch is only how a workshop closes it (ADR 0188).
        let settings = Settings::from_json(r#"{"version":1,"appearance":{"ui_scale":125}}"#).expect("parse");
        assert!(settings.assistant.attach, "a fresh workspace lets a local agent reach the window");
        assert_eq!(settings.appearance.ui_scale, 125, "and the rest of the file is the workspace's");

        let off = Settings::from_json(r#"{"version":1,"assistant":{"attach":false}}"#).expect("parse");
        assert!(!off.assistant.attach, "a closed door stays closed across a restart");
        let text = off.to_json().expect("serialise");
        assert!(!Settings::from_json(&text).expect("reread").assistant.attach);
    }

    #[test]
    fn an_agent_process_is_nothing_until_a_workshop_names_one() {
        // A process that runs a model is not started by a default: an empty command is no agent, and
        // the one a workshop types is remembered across a restart byte for byte (ADR 0230).
        let fresh = Settings::default();
        assert!(fresh.assistant.agent.is_empty(), "a fresh workspace starts no agent process");

        let typed = Settings::from_json(
            r#"{"version":1,"assistant":{"attach":true,"agent":"npx @zed-industries/claude-code-acp"}}"#,
        )
        .expect("parse");
        assert_eq!(typed.assistant.agent, "npx @zed-industries/claude-code-acp");
        let text = typed.to_json().expect("serialise");
        assert_eq!(
            Settings::from_json(&text).expect("reread").assistant.agent,
            "npx @zed-industries/claude-code-acp",
            "the command a person typed survives the write"
        );
    }

    #[test]
    fn a_consent_is_remembered_once_per_endpoint_and_forgetting_it_asks_again() {
        // ADR 0189: the confirmation is per endpoint, remembered across restarts, and clearing it
        // re-asks. The empty list is the state a workspace with no model is born in.
        let mut assistant = AssistantSettings::default();
        assert!(!assistant.consents("https://api.example.com/v1"), "nothing is agreed to by default");
        assistant.remember_consent("https://api.example.com/v1");
        assistant.remember_consent("https://api.example.com/v1");
        assert_eq!(assistant.consent.len(), 1, "one endpoint is one entry");
        assert!(assistant.consents("https://api.example.com/v1"));
        assert!(!assistant.consents("http://127.0.0.1:11434/v1"), "another endpoint is asked about again");
        assistant.remember_consent("  http://127.0.0.1:11434/v1  ");
        assert!(assistant.consents("http://127.0.0.1:11434/v1"), "an endpoint is remembered as typed, trimmed");

        let json = Settings { assistant: assistant.clone(), ..Settings::default() }.to_json().expect("serialise");
        assert_eq!(
            Settings::from_json(&json).expect("parse").assistant.consent,
            assistant.consent,
            "the agreement survives a restart, so the question is asked once"
        );

        assistant.forget_consent("https://api.example.com/v1");
        assert!(!assistant.consents("https://api.example.com/v1"), "forgotten, the next message asks again");
    }

    #[test]
    fn an_endpoint_is_asked_only_when_the_address_and_the_model_are_both_named() {
        let mut assistant = AssistantSettings::default();
        assert_eq!(assistant.endpoint_to_ask(), None, "a fresh workspace asks nothing anywhere");
        assistant.endpoint = "https://api.example.com/v1".to_owned();
        assert_eq!(assistant.endpoint_to_ask(), None, "an address with no model is not an endpoint yet");
        assistant.model = "a-model".to_owned();
        assert_eq!(assistant.endpoint_to_ask().as_deref(), Some("https://api.example.com/v1"));
        assistant.endpoint = "  https://api.example.com/v1  ".to_owned();
        assert_eq!(
            assistant.endpoint_to_ask().as_deref(),
            Some("https://api.example.com/v1"),
            "the endpoint is the string the consent list holds, trimmed"
        );
    }

    #[test]
    fn an_address_the_program_cannot_use_is_named_before_a_request() {
        // J22: the owner pasted a URL into the URL that was already in the field and the only answer
        // was the endpoint's 404. The field now names the clause before a message leaves, and an
        // address with a problem is not an endpoint at all.
        let mut assistant = AssistantSettings::default();
        assert_eq!(assistant.endpoint_problem(), None, "an empty field is not wrong");
        assistant.model = "a-model".to_owned();
        assistant.endpoint =
            "https://opencode.ai/inference/openaihttps://opencode.ai/inference/openai/v1/v1".to_owned();
        assert_eq!(assistant.endpoint_problem(), Some(EndpointProblem::Doubled), "the owner's address");
        assert_eq!(assistant.endpoint_to_ask(), None, "and it is never asked");

        for (endpoint, problem) in [
            ("api.example.com/v1", EndpointProblem::Scheme),
            ("https://", EndpointProblem::Host),
            ("https://api example.com/v1", EndpointProblem::Space),
            ("https://api.example.com/v1/chat/completions", EndpointProblem::Completion),
        ] {
            assistant.endpoint = endpoint.to_owned();
            assert_eq!(assistant.endpoint_problem(), Some(problem), "{endpoint}");
        }

        assistant.endpoint = "http://127.0.0.1:11434/v1".to_owned();
        assert_eq!(assistant.endpoint_problem(), None, "a server on this machine is a valid address");
        assert_eq!(assistant.endpoint_to_ask().as_deref(), Some("http://127.0.0.1:11434/v1"));
    }

    #[test]
    fn the_settings_file_carries_no_key() {
        // The assistant section is exactly the five fields below. A sixth that held a secret would
        // fail here before it could be written to the kind of file people copy between machines.
        let settings = custom();
        let json: serde_json::Value = serde_json::from_str(&settings.to_json().expect("serialise")).expect("json");
        let mut fields: Vec<&str> = json["assistant"]
            .as_object()
            .expect("the assistant section is an object")
            .keys()
            .map(String::as_str)
            .collect();
        fields.sort_unstable();
        assert_eq!(fields, ["agent", "attach", "consent", "endpoint", "model"], "no key, by construction");
    }

    #[test]
    fn a_workspace_written_before_the_kinds_existed_snaps_them_all() {
        // A file from before U116 carries no `kinds` section: every kind is on, exactly as a fresh
        // workspace is, and the section's own switches keep their values.
        let settings = Settings::from_json(r#"{"version":1,"snaps":{"planes":false,"tolerance_mm":12}}"#)
            .expect("parse a file written before the kinds existed");
        assert_eq!(settings.snaps.kinds, PointerKinds::default(), "every kind is on");
        assert!(!settings.snaps.planes, "and the switches the file carried keep their values");
        assert_eq!(settings.snaps.tolerance_mm, 12);

        // One kind off survives the write, and comes back as the model's own value.
        let mut toggled = settings.clone();
        let mut kinds = toggled.snaps.kinds();
        kinds.tangent = false;
        kinds.axis = true;
        toggled.snaps.set_kinds(kinds);
        let json = toggled.to_json().expect("serialise");
        let restored = Settings::from_json(&json).expect("parse");
        assert_eq!(restored.snaps.kinds(), kinds, "the toggles survive a restart");
        assert!(restored.snaps.planes, "the axis kind is the coordinate-planes switch");
    }

    #[test]
    fn the_snap_settings_default_to_the_maker_capture_and_are_clamped_on_read() {
        let defaults = SnapSettings::default();
        assert!(defaults.planes && defaults.frames, "a fresh workspace snaps to the planes and the frames");
        assert!(defaults.floor, "and keeps its floor, so a body is not carried under the bench");
        assert_eq!(defaults.tolerance_mm, DEFAULT_SNAP_MM);
        assert_eq!(DEFAULT_SNAP_MM, model::SnapSettings::DEFAULT_TOLERANCE_MM as u32, "one default, two crates");

        // The rule reads the workspace value as the document's own unit, never as millimetres.
        let settings = defaults.move_settings();
        assert!(settings.floor);
        let Some(snaps) = settings.snaps else { panic!("the object move snaps") };
        assert_eq!(snaps.tolerance, units::Length::from_micrometres(i64::from(DEFAULT_SNAP_MM) * 1000));
        assert!(snaps.kinds.axis && snaps.frames);

        // A hand-edited file cannot set a tolerance no field offers.
        let json = r#"{"version":1,"snaps":{"planes":false,"frames":false,"tolerance_mm":0}}"#;
        let settings = Settings::from_json(json).expect("parse and clamp");
        assert_eq!(settings.snaps.tolerance_mm, MIN_SNAP_MM, "zero captures nothing, so it is not offered");
        assert!(!settings.snaps.planes && !settings.snaps.frames, "the switches survive the clamp");
        assert!(settings.snaps.floor, "and a file written before the floor existed keeps it on");
        let json = r#"{"version":1,"snaps":{"tolerance_mm":900}}"#;
        let settings = Settings::from_json(json).expect("parse and clamp");
        assert_eq!(settings.snaps.tolerance_mm, MAX_SNAP_MM);

        // A workspace written before the section existed snaps the way the editor always did.
        let settings = Settings::from_json(r#"{"version":1}"#).expect("parse");
        assert_eq!(settings.snaps, SnapSettings::default());
    }

    #[test]
    fn the_gizmo_sizes_default_below_the_old_constants_and_are_clamped_on_read() {
        let defaults = GizmoSettings::default();
        assert!(defaults.handle_points < 84, "the handle is smaller than the old 84 pt constant");
        assert!(defaults.ring_percent < 9, "the ring is smaller than the old 9 % constant");

        // A hand-edited file cannot push a size past what the scrub field offers, and a broken
        // value costs one number, not the recent list or the rest of the workspace.
        let json = r#"{"version":1,"gizmo":{"handle_points":0,"ring_percent":900},"recent":["/tmp/cabinet.z3l"]}"#;
        let settings = Settings::from_json(json).expect("parse and repair");
        assert_eq!(settings.gizmo.handle_points, MIN_HANDLE_POINTS);
        assert_eq!(settings.gizmo.ring_percent, MAX_RING_PERCENT);
        assert_eq!(settings.recent, vec![PathBuf::from("/tmp/cabinet.z3l")], "the rest of the file survives");

        let json = r#"{"version":1,"gizmo":{"handle_points":900,"ring_percent":0}}"#;
        let settings = Settings::from_json(json).expect("parse and repair");
        assert_eq!(settings.gizmo.handle_points, MAX_HANDLE_POINTS);
        assert_eq!(settings.gizmo.ring_percent, MIN_RING_PERCENT);
    }

    #[test]
    fn the_work_cursor_size_is_clamped_to_what_a_hand_can_see_and_grab() {
        // A file written by hand or by a newer build holds a size this build cannot draw: the square
        // shrinks to the smallest one a hand can grab, or stops growing before it hides the part.
        for (written, expected) in [(2, MIN_CURSOR_POINTS), (64, MAX_CURSOR_POINTS), (0, MIN_CURSOR_POINTS)] {
            let json = format!(r#"{{"version":1,"pointer":{{"cursor_size":{written},"crosshair":true}}}}"#);
            let settings = Settings::from_json(&json).expect("parse and clamp");
            assert_eq!(settings.pointer.cursor_size, expected, "{written} pt is clamped to {expected}");
            assert_eq!(settings.pointer.size_points(), expected as f32);
            assert!(settings.pointer.crosshair, "the switch beside it survives the clamp");
        }
        // A workspace without the section draws the default square and the plain pointer.
        let settings = Settings::from_json(r#"{"version":1}"#).expect("parse");
        assert_eq!(settings.pointer, PointerSettings::default());
        assert_eq!(settings.pointer.size_points(), DEFAULT_CURSOR_POINTS as f32);
        assert!(!settings.pointer.crosshair);
    }

    #[test]
    fn the_gizmo_sizes_round_trip_through_json() {
        let mut settings = Settings::default();
        settings.gizmo.handle_points = 120;
        settings.gizmo.ring_percent = 12;
        let json = settings.to_json().expect("serialise");
        let restored = Settings::from_json(&json).expect("parse");
        assert_eq!(restored, settings);
        assert_eq!(restored.gizmo.handle_length(), 120.0);
        assert!((restored.gizmo.ring_factor() - 0.12).abs() < 1.0e-12);
    }

    #[test]
    fn a_truncated_file_is_an_error_the_loader_can_report() {
        let json = custom().to_json().expect("serialise");
        let truncated = &json[..json.len() / 2];
        assert!(Settings::from_json(truncated).is_err());
    }

    #[test]
    fn an_unknown_or_missing_version_is_refused() {
        let json = custom().to_json().expect("serialise");
        let newer = json.replace("\"version\": 1", "\"version\": 99");
        assert!(matches!(Settings::from_json(&newer), Err(SettingsError::Version { found: 99, .. })));
        let older = json.replace("\"version\": 1", "\"version\": 0");
        assert!(matches!(Settings::from_json(&older), Err(SettingsError::Version { found: 0, .. })));
    }

    #[test]
    fn a_value_outside_its_range_is_clamped_on_read() {
        let json = r#"{"version":1,"appearance":{"ui_scale":900,"panel_opacity":5},"viewport":{"grid_step_mm":0,"roll_step_degrees":0,"grid_strength":900,"edge_strength":900,"shadow_strength":900,"floor_opacity":900}}"#;
        let settings = Settings::from_json(json).expect("parse and repair");
        assert_eq!(settings.appearance.ui_scale, ui_scale::MAX_PERCENT);
        assert_eq!(settings.appearance.panel_opacity, MIN_PANEL_OPACITY);
        assert_eq!(settings.viewport.grid_step_mm, MIN_GRID_STEP_MM);
        assert_eq!(settings.viewport.roll_step_degrees, MIN_ROLL_STEP_DEGREES);
        assert_eq!(settings.viewport.grid_strength, MAX_DISPLAY_STRENGTH);
        assert_eq!(settings.viewport.edge_strength, MAX_DISPLAY_STRENGTH);
        assert_eq!(settings.viewport.shadow_strength, MAX_DISPLAY_STRENGTH);
        assert_eq!(settings.viewport.floor_opacity, MAX_FLOOR_OPACITY);
    }

    #[test]
    fn the_display_strengths_round_trip_and_reset_to_their_defaults() {
        let defaults = ViewportSettings::default();
        assert_eq!(
            (defaults.grid_strength, defaults.edge_strength, defaults.shadow_strength),
            (60, 70, 40),
            "a fresh workspace draws the owner's default display"
        );
        assert_eq!(defaults.floor_opacity, MAX_FLOOR_OPACITY, "a new workspace opens with the opaque floor");
        let mut settings = Settings::default();
        settings.viewport.grid_strength = 0;
        settings.viewport.edge_strength = 50;
        settings.viewport.shadow_strength = 100;
        settings.viewport.floor_opacity = 35;
        let json = settings.to_json().expect("serialise");
        let read = Settings::from_json(&json).expect("parse");
        assert_eq!(read.viewport.grid_strength, 0);
        assert_eq!(read.viewport.edge_strength, 50);
        assert_eq!(read.viewport.shadow_strength, 100);
        assert_eq!(read.viewport.floor_opacity, 35, "the floor's opacity survives a restart");
        assert_eq!(read, settings, "the workspace file keeps every display value");

        // A file written before the strengths moved out of the view cluster has no key for them.
        let json = r#"{"version":1,"viewport":{"grid":true}}"#;
        let settings = Settings::from_json(json).expect("load");
        assert_eq!(
            (settings.viewport.grid_strength, settings.viewport.edge_strength, settings.viewport.shadow_strength),
            (DEFAULT_GRID_STRENGTH, DEFAULT_EDGE_STRENGTH, DEFAULT_SHADOW_STRENGTH)
        );
        assert_eq!(
            settings.viewport.floor_opacity, DEFAULT_FLOOR_OPACITY,
            "a file from before the field loads the solid floor"
        );
    }

    #[test]
    fn the_floor_opacity_is_a_percentage_that_can_reach_zero() {
        // Zero is a real answer here, not a clamp: the plate and its shadow are gone (U109).
        assert_eq!(MIN_FLOOR_OPACITY, 0);
        const { assert!(MIN_FLOOR_OPACITY < DEFAULT_FLOOR_OPACITY) };
        const { assert!(DEFAULT_FLOOR_OPACITY <= MAX_FLOOR_OPACITY) };
        let mut settings = Settings::default();
        settings.viewport.floor_opacity = 0;
        settings.sanitize();
        assert_eq!(settings.viewport.floor_opacity, 0, "an empty floor is a legal display");
    }

    #[test]
    fn the_panel_opacity_is_a_percentage_inside_its_range() {
        assert_eq!(DEFAULT_PANEL_OPACITY, 92, "the owner's default leaves the scene a little visible");
        const { assert!(MIN_PANEL_OPACITY < DEFAULT_PANEL_OPACITY) };
        const { assert!(DEFAULT_PANEL_OPACITY < MAX_PANEL_OPACITY) };
        assert_eq!(Appearance::default().panel_opacity, DEFAULT_PANEL_OPACITY);

        let json = r#"{"version":1,"appearance":{"panel_opacity":100}}"#;
        assert_eq!(Settings::from_json(json).expect("parse").appearance.panel_opacity, MAX_PANEL_OPACITY);
        let json = r#"{"version":1,"appearance":{"panel_opacity":0}}"#;
        assert_eq!(Settings::from_json(json).expect("parse").appearance.panel_opacity, MIN_PANEL_OPACITY);
    }

    #[test]
    fn the_ribbon_fold_round_trips_through_the_workspace_file() {
        let mut settings = Settings::default();
        assert!(settings.ribbon.open, "a fresh workspace draws the ribbon body");
        settings.ribbon.open = false;
        let json = settings.to_json().expect("serialise");
        let read = Settings::from_json(&json).expect("parse");
        assert_eq!(read, settings, "the workspace file keeps the fold");
        assert!(!read.ribbon.open);

        // A file written before the section existed opens the body, which is what every build before
        // U68 did, and a file that still carries U71's `ribbon` object is read as one too.
        let json = r#"{"version":1,"appearance":{"theme":"dark"}}"#;
        assert!(Settings::from_json(json).expect("load").ribbon.open);
        let json = r#"{"version":1,"ribbon":{"opacity":0.51}}"#;
        assert!(Settings::from_json(json).expect("load").ribbon.open);
    }

    #[test]
    fn the_panel_pair_round_trips_and_a_missing_pair_opens_unfolded() {
        let mut settings = Settings::default();
        assert_eq!(settings.panels, PanelsSettings::default(), "a fresh workspace shows both panels unfolded");
        settings.panels.scene = PanelState { visible: false, expanded: true };
        settings.panels.inspector = PanelState { visible: true, expanded: false };
        let json = settings.to_json().expect("serialise");
        assert_eq!(Settings::from_json(&json).expect("parse"), settings, "the workspace file keeps the pair");

        // A file written before the section existed, or one that carries a partial panel object,
        // opens visible and unfolded: an absent boolean means the panel is there.
        let json = r#"{"version":1,"appearance":{"theme":"dark"}}"#;
        assert_eq!(Settings::from_json(json).expect("load").panels, PanelsSettings::default());
        let json = r#"{"version":1,"panels":{"scene":{"expanded":false}}}"#;
        let read = Settings::from_json(json).expect("load");
        assert_eq!(read.panels.scene, PanelState { visible: true, expanded: false });
        assert_eq!(read.panels.inspector, PanelState::default(), "the unnamed panel keeps its default");
    }

    #[test]
    fn the_pinned_methods_round_trip_through_the_workspace_file() {
        let mut settings = Settings::default();
        assert!(settings.ribbon.pinned_variants.is_empty(), "a fresh workspace pins nothing");
        settings
            .ribbon
            .pinned_variants
            .insert("sketch.create.circle".to_owned(), "sketch.create.circle.three-points".to_owned());
        let json = settings.to_json().expect("serialise");
        let read = Settings::from_json(&json).expect("parse");
        assert_eq!(read, settings, "the workspace file keeps the pin");
        assert_eq!(
            read.ribbon.pinned_variants.get("sketch.create.circle").map(String::as_str),
            Some("sketch.create.circle.three-points")
        );

        // A file written before U67 has no pin object: every tool runs its first method.
        let json = r#"{"version":1,"ribbon":{"open":true}}"#;
        assert!(Settings::from_json(json).expect("load").ribbon.pinned_variants.is_empty());
    }

    #[test]
    fn a_file_from_before_the_presets_starts_on_the_palette_accent() {
        let json = r#"{"version":1,"appearance":{"theme":"dark","accent":[47,159,212],"ui_scale":100},"ribbon":{"opacity":0.51}}"#;
        let (settings, repairs) = Settings::read_json(json).expect("a pre-U71 file loads");
        assert_eq!(settings.appearance.palette, theme::ThemePreset::GraphiteCopper);
        assert_eq!(settings.appearance.accent, None, "the old accent belonged to the old palette");
        assert_eq!(settings.appearance.panel_opacity, DEFAULT_PANEL_OPACITY);
        assert_eq!(settings.appearance.ui_scale, 100);
        assert!(repairs.is_empty());

        let json = r#"{"version":1,"appearance":{"palette":"slate_mint","accent":[17,153,68]}}"#;
        let settings = Settings::from_json(json).expect("load");
        assert_eq!(settings.appearance.accent, Some([17, 153, 68]), "an accent this build wrote is an override");
    }

    #[test]
    fn a_file_without_a_palette_opens_on_the_default_look() {
        let json = r#"{"version":1,"appearance":{"theme":"dark"}}"#;
        let (settings, repairs) = Settings::read_json(json).expect("parse");
        assert_eq!(settings.appearance.palette, theme::ThemePreset::GraphiteCopper);
        assert_eq!(settings.appearance.accent, None, "no override: the palette's own accent is drawn");
        assert_eq!(settings.appearance.panel_opacity, DEFAULT_PANEL_OPACITY);
        assert!(repairs.is_empty(), "a missing field is not a repair: {repairs:?}");
    }

    #[test]
    fn an_unknown_palette_is_repaired_and_reported() {
        let json = r#"{"version":1,"appearance":{"palette":"p3_graphite","ui_scale":125},"recent":["/tmp/a.z3l"]}"#;
        let (settings, repairs) = Settings::read_json(json).expect("one unknown value is not a corrupt file");
        assert_eq!(settings.appearance.palette, theme::ThemePreset::GraphiteCopper, "P1 stands in");
        assert_eq!(settings.appearance.ui_scale, 125, "the rest of the file survives");
        assert_eq!(settings.recent, vec![PathBuf::from("/tmp/a.z3l")]);
        assert_eq!(repairs, vec![Repair::UnknownPalette("p3_graphite".to_owned())]);
        assert_eq!(
            Settings::from_json(json).expect("the plain read").appearance.palette,
            theme::ThemePreset::GraphiteCopper
        );
    }

    #[test]
    fn a_palette_an_earlier_build_named_still_opens_its_look() {
        // T8 renamed the P2 and P4 looks after their accents, because a preset is named after the
        // colour it draws. A workspace written before it keeps the look it chose, and the next save
        // carries the current name (ADR 0125).
        for (old, renamed) in
            [("ink_azure", theme::ThemePreset::InkMagenta), ("slate_mint", theme::ThemePreset::SlateViolet)]
        {
            let json = format!(r#"{{"version":1,"appearance":{{"palette":"{old}"}},"recent":["/tmp/a.z3l"]}}"#);
            let (settings, repairs) = Settings::read_json(&json).expect("an old name is not a corrupt file");
            assert_eq!(settings.appearance.palette, renamed, "{old} still opens its look");
            assert!(repairs.is_empty(), "{old} is a name this build knows, not a repair: {repairs:?}");
            assert_eq!(settings.recent, vec![PathBuf::from("/tmp/a.z3l")], "the rest of the file survives");
            let saved = settings.to_json().expect("the workspace is written back");
            assert!(saved.contains(&format!("\"{}\"", renamed.stored_name())), "the save carries the current name");
            assert!(!saved.contains(old), "the save does not keep the old name");
        }
    }

    #[test]
    fn a_palette_that_is_not_a_name_is_still_a_corrupt_file() {
        let json = r#"{"version":1,"appearance":{"palette":7}}"#;
        assert!(Settings::from_json(json).is_err(), "a wrong type is corrupt, not older");
    }

    #[test]
    fn the_default_appearance_keeps_the_language_and_the_scale_of_a_reset() {
        // What the Reset button does: the look returns to the defaults, the words and the scale stay.
        let mut settings = custom();
        settings.appearance = Appearance {
            language: settings.appearance.language,
            ui_scale: settings.appearance.ui_scale,
            ..Appearance::default()
        };
        assert_eq!(settings.appearance.palette, theme::ThemePreset::GraphiteCopper);
        assert_eq!(settings.appearance.accent, None);
        assert_eq!(settings.appearance.panel_opacity, DEFAULT_PANEL_OPACITY);
        assert_eq!(settings.appearance.icon_color, IconColor::Auto);
        assert_eq!(settings.appearance.language, Language::Russian);
        assert_eq!(settings.appearance.ui_scale, 125);
    }

    #[test]
    fn the_icon_colour_mode_falls_back_below_twenty_pixels() {
        // The threshold is a pixel count, so a UI scale under 100 % falls back without a point-size
        // check somewhere else (U70).
        assert!(IconColor::Auto.uses_roles(ICON_COLOR_MIN_PX));
        assert!(!IconColor::Auto.uses_roles(ICON_COLOR_MIN_PX - 1));
        assert!(IconColor::Always.uses_roles(1), "always means always");
        assert!(!IconColor::Never.uses_roles(512), "never means never");
        assert_eq!(IconColor::default(), IconColor::Auto);
    }

    #[test]
    fn the_roll_step_defaults_to_one_and_is_clamped_to_the_ring_range() {
        assert_eq!(ViewportSettings::default().roll_step_degrees, 1);
        let json = r#"{"version":1,"viewport":{"roll_step_degrees":100}}"#;
        let settings = Settings::from_json(json).expect("parse and repair");
        assert_eq!(settings.viewport.roll_step_degrees, MAX_ROLL_STEP_DEGREES);

        let json = r#"{"version":1,"viewport":{"roll_step_degrees":7}}"#;
        let settings = Settings::from_json(json).expect("parse");
        assert_eq!(settings.viewport.roll_step_degrees, 7, "a value in range is kept");
    }

    #[test]
    fn the_recent_list_is_newest_first_and_bounded() {
        let mut settings = Settings::default();
        for index in 0..(MAX_RECENT + 4) {
            settings.remember(Path::new(&format!("/tmp/scene-{index}.z3l")));
        }
        assert_eq!(settings.recent.len(), MAX_RECENT);
        assert_eq!(settings.recent[0], PathBuf::from(format!("/tmp/scene-{}.z3l", MAX_RECENT + 3)));

        settings.remember(Path::new("/tmp/scene-5.z3l"));
        assert_eq!(settings.recent.len(), MAX_RECENT, "remembering an old path does not grow the list");
        assert_eq!(settings.recent[0], PathBuf::from("/tmp/scene-5.z3l"));
        let count = settings.recent.iter().filter(|path| path.ends_with("scene-5.z3l")).count();
        assert_eq!(count, 1, "an old path moves to the front instead of repeating");
    }

    #[test]
    fn settings_never_appear_in_a_serialised_document() {
        let root = Container::new(Id::new(1).expect("id"), "Scene", Placement::IDENTITY, Vec::new());
        let project = Project::new(root).expect("project");
        let document = serde_json::to_string(&project).expect("serialise");
        for key in [
            "ui_scale",
            "grid_step_mm",
            "tolerance_mm",
            "roll_step_degrees",
            "panel_opacity",
            "accent",
            "palette",
            "theme",
            "recent",
            "language",
            "icon_color",
        ] {
            assert!(!document.contains(key), "`{key}` is workspace state and must stay out of the .z3l");
        }
    }

    #[test]
    fn the_shortcut_table_names_every_drawing_tool() {
        let rows = shortcut_table(Lang::En);
        for tool in crate::toolbars::CONTOUR_TOOLS {
            assert!(
                rows.iter().any(|(_, action)| action == tool.label().text(Lang::En)),
                "{} is missing from the shortcut page",
                tool.label().text(Lang::En)
            );
        }
    }

    #[test]
    fn the_shortcut_table_names_every_transform_mode_with_its_key() {
        // U113: the band's four gestures are keys too, so the Settings page lists them with the rest
        // and prints the table's own spelling of the pair — one row per mode, in both languages.
        for lang in [Lang::En, Lang::Ru] {
            let rows = shortcut_table(lang);
            for mode in crate::transform::TransformMode::ALL {
                let key = mode.key().expect("a mode is bound to a key");
                assert!(
                    rows.iter().any(|(printed, action)| printed == key && action == mode.label(lang)),
                    "`{}` is missing from the shortcut page in {lang:?}",
                    mode.id()
                );
            }
        }
    }

    #[test]
    fn a_scene_saved_in_russian_loads_in_english_with_its_names_unchanged() {
        let root = Container::new(Id::new(1).expect("id"), "Шкаф", Placement::IDENTITY, Vec::new());
        let project = Project::new(root).expect("project");
        let json = serde_json::to_string(&project).expect("serialise");
        let restored: Project = serde_json::from_str(&json).expect("load");
        assert_eq!(restored.root().name(), "Шкаф", "the language never renames a node");
        assert_eq!(Language::from_lang(Lang::En), Language::English);
    }

    #[test]
    fn a_settings_file_without_a_language_opens_in_english() {
        let json = r#"{"version":1,"appearance":{"ui_scale":100}}"#;
        let settings = Settings::from_json(json).expect("parse");
        assert_eq!(settings.appearance.language, Language::English);
        assert_eq!(settings.appearance.language.lang(), Lang::En);
    }
}
