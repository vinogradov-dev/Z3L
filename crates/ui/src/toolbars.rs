//! The ribbon: one row of tabs over the tools of the active tab.
//!
//! The map — five tabs that name a domain (*Sketch, Model, Furniture, Scene, View*), the groups of
//! operations under them and the tools inside them — is one const table in [`spec`]. This module
//! draws it with one loop: a tab is drawn when it owns a live group, a group when it owns a live
//! tool, and a planned tool (no command yet) is skipped, so the ribbon never shows a greyed "coming
//! soon" row and a tab with nothing to run is not drawn at all.
//!
//! Whether a tool can run *here* is code, not data: [`ready::ready`] decides it against the
//! [`RibbonContext`] — a catalogue material, a closed path, a running draft, a viewport that owns its
//! projection — and returns the hint that says what is missing, so a disabled cell still explains
//! itself. Whether the body is open, which tab is active, the typed defaults it remembers, which
//! method each tool is pinned to and which list is open is view state in [`state::Toolbars`]: never
//! document data; the fold and the pin are also workspace settings the shell remembers.
//!
//! The ribbon holds tools only (U76): a number a tool needs is a typed session default, and the
//! object card or the operation's own command bar is where the user edits it, so no cell is a form.
//!
//! This module is the drawing: [`show`] and the row, cell and list wiring. The map is [`spec`], the
//! state is [`state`], the readiness table is [`ready`] and the popup under a cell is [`list`].
//!
//! A cell is one glyph in a [`RIBBON_CELL_WIDTH`] by [`RIBBON_ROW_HEIGHT`] cell, so a row is one pitch
//! and no tool sizes itself from its name (ADR 0067, revised by ADR 0091). The group's caption sits in
//! the quieter band under the row, which is what makes the whole body 80 points tall and the folded
//! ribbon its 29-point tab row.
//!
//! Every button is the icon-only [`icons::ribbon_cell`]: the ribbon names a tool through its hint and
//! through the list under the cell a click opens ([`list`], U67); the panel headers and the floating
//! viewport cluster keep the icon-only [`icons::tool_button`], whose row could not carry a second
//! line.
//!
//! A tool with more than one input method is a split button (U74): [`icons::ribbon_split_cell`] gives
//! it a main face that is a whole cell and runs the method the workspace pinned, plus a caret face
//! added beside it that opens its variant list, while a tool with one method keeps the whole cell and
//! runs on the click. The pin is a workspace setting, the open list is view state, and the key of the
//! tool runs the pinned method without opening anything.

use std::collections::BTreeMap;

use egui::Ui;
use engine::{ContourOp, PrimitiveKind};
use model::{ClassCode, FillMode, Id};
use text::{Lang, Message};
use units::Length;

use self::spec::{GroupSpec, TabSpec, ToolKind, ToolSpec};
use crate::draw::{DrawIntent, DrawTool};
use crate::icons::{self, Icon};
use crate::theme::{RIBBON_CAPTION_HEIGHT, RIBBON_CELL_WIDTH, RIBBON_ROW_HEIGHT, RIBBON_RULE, RIBBON_TAB_HEIGHT};

pub mod list;
pub mod ready;
pub mod spec;
pub mod state;

pub use self::ready::draft_can_close;
pub use self::state::Toolbars;

use self::ready::{projection_icon, ready, snap_mode_icon};

/// Height of a new extrusion, in millimetres; also the height the next extrusion starts at.
pub const DEFAULT_EXTRUDE_HEIGHT_MM: i64 = 100;

/// One action a toolbar panel asks the editor to run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolAction {
    /// Starts a drawing tool; `sides` is the regular polygon's side count and is ignored by the
    /// other tools.
    Draw {
        /// Which tool to arm.
        tool: DrawTool,
        /// The side count the polygon tool uses.
        sides: u32,
    },
    /// Starts a drawing tool whose closed figure becomes the profile of a solid body.
    ///
    /// One `object.path.create` puts the profile in the document and the shell then pulls its
    /// height in the same edit scope, so the ring and the body are one gesture and one undo entry
    /// (ADR 0138).
    DrawBody {
        /// Which rectangle or circle method draws the profile.
        tool: DrawTool,
    },
    /// Opens the product-frame card; the card dispatches `product.create`.
    CreateProductCard,
    /// Arms the rectangle tool with the frame as its owner: the figure fixes the reserved box's base
    /// and the height pulled from it dispatches `product.create` (N19, ADR 0101).
    DrawProductFrame {
        /// Which rectangle method draws the base.
        tool: DrawTool,
    },
    /// Arms the free-outline tool whose closed ring becomes a frame's base outline.
    DrawProductFrameContour,
    /// Arms the free-outline tool, whose closed ring becomes one panel cut to it.
    DrawPanelContour,
    /// Arms the empty-space tool with the class whose `fill` trait picks the axis pair.
    Fill {
        /// The class a post or a shelf fills the void with.
        class: ClassCode,
        /// How the part reads the section the walls leave.
        mode: FillMode,
    },
    /// Creates an assembly at the scene root.
    CreateAssembly,
    /// Draws a primitive's placement: the circle the shape is built around.
    ///
    /// The sphere cell carries the sketch's own circle methods, so the button hands the editor a
    /// drawing tool and no size: the plates carry the numbers and the commit stores the primitive
    /// (ADR 0139).
    DrawPrimitive {
        /// Which primitive the drawn figure builds.
        kind: PrimitiveKind,
        /// Which circle method draws the figure.
        tool: DrawTool,
    },
    /// Turns the selected drawing into bodies: one for a path, one per ring for a contour set.
    Extrude {
        /// The drawing object — a closed path or a contour set — that becomes bodies.
        object: Id,
        /// Height along the drawing's local Z.
        height: Length,
    },
    /// Arms the area pull on the selected drawing: the pointer names an area, a click pulls it (G4).
    AreaPull {
        /// The contour set whose areas the pointer reads.
        object: Id,
    },
    /// Creates a camera at the scene root.
    CreateCamera,
    /// Creates a sun light at the scene root.
    CreateLight,
    /// Closes the running drawing into a ring.
    CloseDrawing,
    /// Drops the running drawing; the document never changed.
    CancelDrawing,
    /// Frames the whole scene in the viewport.
    FrameAll,
    /// Switches the viewport between perspective and orthographic.
    ToggleProjection,
    /// Turns one snap kind on or off (U116).
    ToggleSnap(model::SnapKind),
    /// Turns every snap kind on, or every kind off (U116).
    ToggleSnapAll,
    /// Switches the snap mode between 3D and 2.5D (U119).
    ToggleSnapMode,
    /// Turns the ground shadow of the active view on or off (U107).
    ToggleShadows,
    /// Arms the ruler, or disarms it when it already is: two clicks measure a length (ADR 0118).
    Measure,
    /// Arms one contour boolean over the selection, or drops it when it is already armed (G3).
    ContourBoolean(ContourOp),
    /// Arms one body boolean over the selected body, or drops it when it is already armed (W1).
    BodyBoolean(ContourOp),
}

/// Every drawing tool a key starts, in shortcut order: the draw group then the shapes.
///
/// Only a tool whose own method carries a key is here, because this table is the shell's key path
/// and the Settings page's shortcut list. A method reached through a cell's list instead — the
/// circle by diameter, the arc by its radius, whose plain and shifted chords belong to the methods
/// beside them — has no row of its own; the cell's key runs it once the workspace pins it (U67,
/// U96).
pub const CONTOUR_TOOLS: [DrawTool; 10] = [
    DrawTool::Line,
    DrawTool::Polyline,
    DrawTool::Arc,
    DrawTool::ArcFromCenter,
    DrawTool::Rectangle,
    DrawTool::RectangleFromCenter,
    DrawTool::RectangleFromCenterDirected,
    DrawTool::Circle,
    DrawTool::CircleThreePoints,
    DrawTool::RegularPolygon,
];

/// The tool that owns the next pointer events, drawn as pressed on the strip.
///
/// Only the arming tools have a state to show: a drawing tool holds the pointer until the chain is
/// committed — and a primitive's round and a body's profile are drafts too — while a box waits for its
/// base rectangle. A one-shot creation — a panel, an assembly, a camera or a light — runs and
/// forgets, so it is never armed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolActivity {
    /// A drawing tool is building a chain.
    Draw(DrawTool),
    /// A drawing tool is building the profile of a solid body.
    ///
    /// The draft is the sketch draft; only what its commit becomes differs, so the pressed cell is the
    /// body cell and not the sketch tool whose glyph the draft borrows (ADR 0138).
    Body(DrawTool),
    /// A drawing tool is building the outline of one panel.
    ///
    /// The draft is the same; only what its close becomes differs, so the pressed cell is the contour
    /// cell and not the sketch tool whose glyph the draft borrows (ADR 0114).
    PanelContour,
    /// A primitive is waiting for its work-plane click.
    Primitive(PrimitiveKind),
    /// A rectangle method is drawing a product frame's reserved box.
    ///
    /// The draft is the sketch draft; only what its commit becomes differs, so the pressed cell is the
    /// frame cell and not the box cell whose glyph the draft borrows (N19, ADR 0101).
    ProductBox(DrawTool),
    /// The frame's reserved box is pulling its height.
    ProductFrame,
    /// A drawing tool is building the base outline of a product frame.
    ProductContour,
    /// The empty-space tool is armed with a fill class and its mode.
    Fill {
        /// The class the armed cell fills the void with.
        class: ClassCode,
        /// How the armed cell reads the section.
        mode: FillMode,
    },
    /// The ruler is armed and owns the next click.
    Measure,
    /// The area pull is armed: the pointer names an area of the selected drawing, and a click pulls
    /// it into a body of its own (G4).
    AreaPull,
    /// A contour boolean is armed and collects the picked rings.
    ContourBoolean(ContourOp),
    /// A body boolean is armed and collects the target and its tool.
    BodyBoolean(ContourOp),
}

/// The activity whose button is pressed, from what the editor holds.
///
/// The arguments are the sources of an armed tool, not a decision: a running draft names its tool
/// and what it will become, a box draft names the field it will write and a fill arm names its
/// class. None outranks another because the editor never holds two at once.
pub fn tool_activity(
    drawing: Option<(DrawTool, DrawIntent)>,
    frame: bool,
    fill: Option<(ClassCode, FillMode)>,
    measure: bool,
    pull: bool,
    boolean: Option<(crate::editor::BooleanOperand, ContourOp)>,
) -> Option<ToolActivity> {
    let drawing = match (drawing, frame, fill, measure) {
        (Some((tool, DrawIntent::Path)), _, _, _) => Some(ToolActivity::Draw(tool)),
        (Some((tool, DrawIntent::Body)), _, _, _) => Some(ToolActivity::Body(tool)),
        (Some((_, DrawIntent::Panel)), _, _, _) => Some(ToolActivity::PanelContour),
        (Some((tool, DrawIntent::ProductBox)), _, _, _) => Some(ToolActivity::ProductBox(tool)),
        (Some((_, DrawIntent::Product)), _, _, _) => Some(ToolActivity::ProductContour),
        // A primitive's placement is a draft like any other, so the pressed cell is the sphere's.
        (Some((_, DrawIntent::Primitive(kind))), _, _, _) => Some(ToolActivity::Primitive(kind)),
        (None, true, _, _) => Some(ToolActivity::ProductFrame),
        (None, false, Some((class, mode)), _) => Some(ToolActivity::Fill { class, mode }),
        (None, false, None, true) => Some(ToolActivity::Measure),
        (None, false, None, false) => None,
    };
    // A boolean is a mode of its own that owns the picks, so it never shares the pressed cell with
    // a running draft or a fill (G3). The two operand kinds are two cells on two tabs, and the kind
    // decides which one draws pressed (W1).
    drawing.or(pull.then_some(ToolActivity::AreaPull)).or(boolean.map(|(kind, op)| match kind {
        crate::editor::BooleanOperand::Rings => ToolActivity::ContourBoolean(op),
        crate::editor::BooleanOperand::Bodies => ToolActivity::BodyBoolean(op),
    }))
}

/// Whether the button for one tool is the armed one.
///
/// A creation that runs and forgets is never armed, so its button stays idle however long the
/// selection sits there.
fn is_armed(activity: Option<ToolActivity>, kind: ToolKind) -> bool {
    match (activity, kind) {
        (Some(ToolActivity::Draw(armed)), ToolKind::Draw(tool)) => armed == tool,
        (Some(ToolActivity::Body(armed)), ToolKind::Body(tool)) => armed == tool,
        (Some(ToolActivity::PanelContour), ToolKind::PanelContour) => true,
        (Some(ToolActivity::Primitive(armed)), ToolKind::Primitive { kind, .. }) => armed == kind,
        (Some(ToolActivity::ProductBox(armed)), ToolKind::ProductBox(tool)) => armed == tool,
        // The frame cell runs the card as its primary method, so the drawn method's row carries the
        // pressed state and the cell shows it through the list (U74).
        (Some(ToolActivity::ProductFrame), ToolKind::ProductBox(_)) => true,
        (Some(ToolActivity::ProductContour), ToolKind::ProductContour) => true,
        // One cell fills every void, so an armed fill presses it whatever class or build it took
        // (U123): the card and the part's own menu say which one it is.
        (Some(ToolActivity::Fill { .. }), ToolKind::Fill) => true,
        (Some(ToolActivity::Measure), ToolKind::Measure) => true,
        // The pull owns the pointer and the Extrude cell's second method is the one that armed it
        // (G4).
        (Some(ToolActivity::AreaPull), ToolKind::AreaPull) => true,
        (Some(ToolActivity::ContourBoolean(armed)), ToolKind::ContourBoolean(op)) => armed == op,
        (Some(ToolActivity::BodyBoolean(armed)), ToolKind::BodyBoolean(op)) => armed == op,
        _ => false,
    }
}

/// One key that starts a drawing tool, with the modifier it needs.
///
/// A shifted letter is the alternate of the same family: `A` draws an arc through points and
/// `Shift+A` an arc from its centre, `R` a rectangle from two corners and `Shift+R` one from its
/// centre.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shortcut {
    /// The key that starts the tool.
    pub key: egui::Key,
    /// Whether Shift must be held with it.
    pub shift: bool,
}

/// The shortcut that starts a drawing tool, read from the inventory's key table (U96).
///
/// The key is filed under the method the tool draws, so the button hint, the tool list and the
/// shell's key reader all print and consume one row.
pub fn shortcut(tool: DrawTool) -> Option<Shortcut> {
    let binding = crate::inventory::binding(spec::method_id(tool))?;
    Some(Shortcut { key: binding.key, shift: binding.chord == crate::inventory::Chord::Shift })
}

/// The label of a tool's shortcut, for the button hint.
pub fn shortcut_label(tool: DrawTool) -> Option<&'static str> {
    crate::inventory::key_label(spec::method_id(tool))
}

/// The strip's label for a drawing tool: its name plus the one gesture the tool does not show.
///
/// The label is the button's tooltip and the text a placeholder asset falls back to; it is not the
/// name of the object the tool creates, which stays [`DrawTool::label`].
pub const fn strip_label(tool: DrawTool) -> Message {
    match tool {
        DrawTool::Polyline => Message::PolylineStripLabel,
        _ => tool.label(),
    }
}

/// The view state the ribbon reads from the shell: how the camera and the work plane are set.
///
/// The ribbon is a UI surface in `ui`, but the projection, the grid snap and the framing
/// belong to the shell that owns the view camera and the drawing gizmo. They arrive as this small
/// snapshot, and a click comes back as a [`RibbonViewAction`], so the ribbon neither reads nor
/// writes the viewport.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RibbonView {
    /// The view camera is perspective rather than orthographic.
    pub perspective: bool,
    /// The viewport owns its projection; an authored camera pins it.
    pub owns_view: bool,
    /// Which snap kinds capture the pointer (U116).
    pub snaps: model::SnapKinds,
    /// How a snap reads a corner above the drawing plane (U119).
    pub snap_mode: model::SnapMode,
    /// The active view draws the ground shadow (U107).
    pub shadows: bool,
}

/// What one ribbon click asked the shell's view to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RibbonViewAction {
    /// Frame the whole scene.
    FrameAll,
    /// Switch between perspective and orthographic.
    ToggleProjection,
    /// Turn one snap kind on or off (U116).
    ToggleSnap(model::SnapKind),
    /// Turn every snap kind on, or every kind off (U116).
    ToggleSnapAll,
    /// Make every snap answer the 2.5D way, or the 3D way (U119).
    ToggleSnapMode,
    /// Turn the active view's ground shadow on or off (U107).
    ToggleShadows,
}

/// What drawing the ribbon changed: whether a tool started, and the one view action it asked for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RibbonOutcome {
    /// A drawing tool started, so the shell resets its pointer state.
    pub started_drawing: bool,
    /// The running ring must be closed and committed.
    ///
    /// The ribbon reports the *Close* instead of running it, because closing owns the work plane,
    /// the gizmo state and the scene rebuild — all of them the shell's. One close path serves the
    /// ribbon's Draft group and a click on the first draft point (ADR 0052).
    pub close_draft: bool,
    /// The view action the ribbon asked for, if any.
    pub view: Option<RibbonViewAction>,
    /// The body a path extrusion just created, with the height it used.
    ///
    /// The shell starts the pull arrow on that body; the height is session view state, so the next
    /// extrusion starts here instead of at the default.
    pub extruded: Option<(Id, Length)>,
    /// The *Product frame* cell asked for its card.
    ///
    /// The card belongs to the window, like Settings and the File dialogs, so the ribbon reports
    /// the request and the shell draws it (ADR 0100).
    pub open_product_card: bool,
    /// The rectangle the open method list covered, when one was drawn this frame.
    ///
    /// The list is a surface over the scene, so the shell treats this rectangle as chrome: a press
    /// or a tool cursor inside it belongs to the list, not to the viewport (X37).
    pub list_rect: Option<egui::Rect>,
}

/// What the ribbon reads from the scene, the pointer and the view camera while it draws.
///
/// The values travel together because every tab reads the same picture: what the catalogue offers,
/// what is selected, whether that selection is a closed path, which tool is armed, how the viewport
/// draws, and what the running draft has built so far.
#[derive(Clone, Copy, Debug, Default)]
pub struct RibbonContext {
    /// The catalogue has at least one panel material.
    pub has_panel_material: bool,
    /// A product is open, so a niche is offered (ADR 0098).
    pub product_open: bool,
    /// The primary selection, if any.
    pub selected: Option<Id>,
    /// Whether the selection is a path, and whether that path is closed.
    pub selected_path: Option<bool>,
    /// Whether the selection draws one closed ring a boolean can take: a path object or a body's
    /// own profile (X98).
    pub selected_ring: bool,
    /// Whether the selection is a contour set, whose enclosed areas the Extrude cell pulls (G4).
    pub selected_drawing: bool,
    /// Whether the selection is a solid body — a drawn body or a primitive — which is what a body
    /// boolean edits and what it cuts with (W1).
    pub selected_body: bool,
    /// The tool that owns the next click.
    pub active: Option<ToolActivity>,
    /// The view is perspective rather than orthographic.
    pub perspective: bool,
    /// Whether the viewport owns its projection; an authored camera pins it.
    pub owns_view: bool,
    /// Which snap kinds capture the pointer (U116).
    pub snaps: model::SnapKinds,
    /// How a snap reads a corner above the drawing plane (U119).
    pub snap_mode: model::SnapMode,
    /// Whether the active view draws the ground shadow (U107).
    pub shadows: bool,
    /// A running draft: its tool, how many points it has, and whether it is closed.
    pub draft: Option<(DrawTool, usize, bool)>,
    /// What the next click of the Panel cell makes: the class and the build the card holds (U123).
    pub build: crate::editor::PanelBuild,
}

/// Draws the ribbon: one row of tabs and, while it is open, the groups of the active tab.
///
/// Every tab that owns a live group is drawn, and the one whose groups are shown carries the accent.
/// Clicking the active tab, or double-clicking any tab label, toggles the body, so a collapsed ribbon
/// keeps only its tab row. The armed tool draws pressed and a tool whose action needs a draft or a
/// viewport that owns its projection is drawn disabled; the hint says what is missing.
///
/// Returns the rectangle of the open method list, when a cell drew one: the popup hangs under the
/// band and over the scene, so the shell needs it for the frame it is on (X37).
pub fn show(
    ui: &mut Ui,
    state: &mut Toolbars,
    context: RibbonContext,
    actions: &mut Vec<ToolAction>,
) -> Option<egui::Rect> {
    let lang = crate::language(ui.ctx());
    let mut list_rect = None;
    ui.scope(|ui| {
        ui.set_width(ui.available_width());
        // The rows sit edge to edge: the drawn height is the sum of the metrics and nothing else,
        // because the shell reserves exactly [`height`] over the design space (ADR 0080).
        ui.spacing_mut().item_spacing.y = 0.0;
        state.retain_tab();
        state.retain_list();
        tab_row(ui, state, lang);
        rule(ui);
        if state.is_open() {
            list_rect = ribbon_body(ui, state, context, actions, lang);
            rule(ui);
        }
    });
    list_rect
}

/// The outer margin of the ribbon's overlay frame, in points (horizontal, vertical).
///
/// The ribbon floats over the design space instead of taking height from it (U51), so its height and
/// the band the shell reserves for it must agree exactly: this constant is the single source the
/// frame the ribbon draws and [`height`] both read. The vertical margin is zero, because the height
/// is the approved 29/76 metric itself and every row of it is a control row.
pub const RIBBON_FRAME_MARGIN: [i8; 2] = [6, 0];

/// The height the ribbon overlay covers over the design space, in points.
///
/// The shell reserves this band for the ribbon and never lets the scene, the side panels or the
/// framing use it, so a translucent ribbon blends with the furniture it covers (U51). It is
/// deterministic from the open state so the strip and the reserved inset agree without a frame of
/// lag when the body opens or closes: the tab row and its rule folded, plus the tool row, the caption
/// band and the rule under it when the body is open.
pub fn height(open: bool) -> f32 {
    let body = if open { RIBBON_ROW_HEIGHT + RIBBON_CAPTION_HEIGHT + RIBBON_RULE } else { 0.0 };
    RIBBON_TAB_HEIGHT + RIBBON_RULE + body
}

/// One hairline across the strip, the rule that closes a row.
fn rule(ui: &mut Ui) {
    let width = ui.available_width().max(0.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, RIBBON_RULE), egui::Sense::hover());
    ui.painter().rect_filled(rect, egui::CornerRadius::ZERO, crate::theme::line(ui));
}

/// The one row of tabs, each drawn in the ribbon's own rhythm.
fn tab_row(ui: &mut Ui, state: &mut Toolbars, lang: Lang) {
    let height = RIBBON_TAB_HEIGHT;
    ui.horizontal(|ui| {
        ui.set_height(height);
        let active = state.tab();
        let open = state.is_open();
        for tab in spec::tabs() {
            if !tab.is_drawn() {
                continue;
            }
            let selected = tab.id == active.id && open;
            let response = tab_button(ui, tab, selected, height, lang);
            // A double-click also reports a click in the same frame, so it is tested first: the two
            // gestures would otherwise cancel each other out and the body would never close.
            if response.double_clicked() {
                apply_tab_gesture(state, tab, true);
            } else if response.clicked() {
                apply_tab_gesture(state, tab, false);
            }
        }
    });
}

/// What one gesture on a tab does: open its body, or collapse and restore it.
///
/// A single click on another tab opens that tab; a single click on the open tab collapses the body,
/// because the tab already says where the tools are. A double-click collapses or restores whichever
/// tab it lands on, which is the office-ribbon habit and works without aiming at the open tab.
fn apply_tab_gesture(state: &mut Toolbars, tab: &'static TabSpec, double: bool) {
    if double {
        state.set_tab(tab);
        state.toggle();
    } else if tab.id == state.tab().id {
        state.toggle();
    } else {
        state.set_tab(tab);
        state.set_open(true);
    }
}

/// One tab: its name in the body type, on a control fill with the accent underline while it is open.
///
/// The open tab is marked by the accent, not by a second palette entry: the whole ribbon is chrome,
/// and one saturated line on the row already says which group row belongs to which tab. Every tab
/// has the same inner padding and a minimum width, so a four-letter name does not read as a stamp
/// next to a long one.
fn tab_button(ui: &mut Ui, tab: &'static TabSpec, selected: bool, height: f32, lang: Lang) -> egui::Response {
    let font = egui::FontId::proportional(12.0);
    let galley =
        ui.fonts_mut(|fonts| fonts.layout_no_wrap(tab.label.text(lang).to_owned(), font, egui::Color32::PLACEHOLDER));
    let width = (galley.size().x + 6.0 * crate::theme::STEP).max(crate::theme::RIBBON_TAB_MIN_WIDTH);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());
    if ui.is_rect_visible(rect) {
        if selected {
            ui.painter().rect_filled(rect, egui::CornerRadius::same(crate::theme::RADIUS), crate::theme::control(ui));
            let underline = egui::Rect::from_min_max(
                egui::pos2(rect.left() + 2.0, rect.bottom() - 2.0),
                egui::pos2(rect.right() - 2.0, rect.bottom()),
            );
            ui.painter().rect_filled(underline, egui::CornerRadius::ZERO, crate::theme::accent(ui));
        } else if response.hovered() {
            ui.painter().rect_filled(
                rect,
                egui::CornerRadius::same(crate::theme::RADIUS),
                crate::theme::control_hover(ui),
            );
        }
        let color = if selected { crate::theme::text(ui) } else { crate::theme::text_dim(ui) };
        let pos = egui::pos2(rect.center().x - galley.size().x / 2.0, rect.center().y - galley.size().y / 2.0);
        ui.painter().galley(pos, galley, color);
        response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, tab.label.text(lang)));
    }
    response
}

/// The body under the tab row: the active tab's live groups in one row, separated by dividers.
///
/// The row is as wide as the map says — the tab's [`TabSpec::columns`] cells — and a group widens to
/// its own caption if the caption is wider than its cells, so a one-tool group still names itself.
/// The widest row of the map is ten cells, well inside the smallest window, so the body needs no
/// scroll area.
fn ribbon_body(
    ui: &mut Ui,
    state: &mut Toolbars,
    context: RibbonContext,
    actions: &mut Vec<ToolAction>,
    lang: Lang,
) -> Option<egui::Rect> {
    let tab = state.tab();
    if !tab.is_drawn() {
        return None;
    }
    let body_height = RIBBON_ROW_HEIGHT + RIBBON_CAPTION_HEIGHT;
    let mut list_rect = None;
    ui.horizontal(|ui| {
        // Cells sit edge to edge inside a group: the whole cell is the button, and the only air
        // between two controls is the padding the cell already carries (ADR 0067).
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.set_height(body_height);
        for (index, group) in tab.live_groups().enumerate() {
            if index > 0 {
                crate::theme::divider_of_height(ui, body_height);
            }
            list_rect = show_group(ui, group, state, context, actions, lang).or(list_rect);
        }
    });
    list_rect
}

/// One group: its row of cells over the darker band that captions it.
fn show_group(
    ui: &mut Ui,
    group: &'static GroupSpec,
    state: &mut Toolbars,
    context: RibbonContext,
    actions: &mut Vec<ToolAction>,
    lang: Lang,
) -> Option<egui::Rect> {
    let mut list_rect = None;
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let row = ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.set_height(RIBBON_ROW_HEIGHT);
            for tool in group.live_tools() {
                list_rect = show_tool(ui, tool, state, context, actions, lang).or(list_rect);
            }
        });
        caption_band(ui, row.response.rect.width(), group.label.text(lang));
    });
    list_rect
}

/// The caption band under one group: the quiet strip with the group's name in capitals.
///
/// The band is [`crate::theme::caption_band`], one step from a control toward the chrome: the darkest
/// surface alone read as a hole under every group (U75). The name is centred in the band, which is
/// tall enough at [`RIBBON_CAPTION_HEIGHT`] to keep air above and below the capitals.
fn caption_band(ui: &mut Ui, row_width: f32, caption: &str) {
    let galley = ui.fonts_mut(|fonts| {
        fonts.layout_no_wrap(
            caption.to_uppercase(),
            egui::FontId::proportional(icons::RIBBON_CAPTION_POINT_SIZE),
            egui::Color32::PLACEHOLDER,
        )
    });
    let width = row_width.max(galley.size().x);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, RIBBON_CAPTION_HEIGHT), egui::Sense::hover());
    ui.painter().rect_filled(rect, egui::CornerRadius::ZERO, crate::theme::caption_band(ui));
    let pos = egui::pos2(rect.center().x - galley.size().x / 2.0, rect.center().y - capitals_centre(&galley));
    ui.painter().galley(pos, galley, crate::theme::text_dim(ui));
}

/// The middle of a caption's capitals, measured from the top of the galley.
///
/// The line box of a galley carries the descent, which capitals never use, so centring the box leaves
/// the name two points high in the band (U75). Centring the block between the tallest glyph and the
/// baseline puts every caption of the strip on one baseline — a `Д` leg or a `Ц` tail hangs as type
/// does — instead of lifting the words that have one. A layout with no glyph falls back to the box.
fn capitals_centre(galley: &egui::Galley) -> f32 {
    let baseline = galley.rows.first().and_then(|row| row.row.glyphs.first().map(|glyph| row.pos.y + glyph.pos.y));
    match baseline {
        Some(baseline) => (galley.mesh_bounds.top() + baseline) / 2.0,
        None => galley.size().y / 2.0,
    }
}

/// One tool cell: its glyph, and the action a click asks the editor to run.
///
/// A tool with more than one input method is a split button (U74): its main face is a full cell that
/// runs the method the workspace pinned and names it in the hint, and a caret face beside it opens
/// the list of the other methods, so the row keeps one glyph pitch and one glyph size whether a tool
/// has a choice or not (X36). A tool with one method keeps the whole cell and runs on the click, so a
/// tool that has no choice carries no caret.
fn show_tool(
    ui: &mut Ui,
    tool: &'static ToolSpec,
    state: &mut Toolbars,
    context: RibbonContext,
    actions: &mut Vec<ToolAction>,
    lang: Lang,
) -> Option<egui::Rect> {
    let kind = tool.primary()?;
    let icon = tool.icon?;
    if !tool.has_list() {
        // The projection button draws the mode the view is *in*, so the glyph flips with the camera
        // while the hint names the switch.
        let icon = match kind {
            ToolKind::Projection => projection_icon(context.perspective),
            ToolKind::SnapMode => snap_mode_icon(context.snap_mode),
            _ => icon,
        };
        let ready = ready(kind, tool, state, context, lang);
        let response = icons::ribbon_cell(ui, RIBBON_CELL_WIDTH, icon, &ready.hint, ready.enabled, ready.armed);
        if response.clicked()
            && let Some(action) = ready.action
        {
            actions.push(action);
        }
        return None;
    }
    let rows = method_rows(tool, state, context, lang);
    let pinned = state.pinned_index(tool);
    // The armed method is what the cell shows: picking a mode from the list must change the face
    // and the pressed state, not just the ghost, or the choice looks like it did not take (ADR 0110).
    let shown = rows.iter().position(|row| row.armed).unwrap_or(pinned);
    let face = main_face(tool, &rows, shown)?;
    let caret_hint = Message::ToolMethods.text(lang);
    let cell =
        icons::ribbon_split_cell(ui, RIBBON_CELL_WIDTH, face.icon, &face.hint, caret_hint, face.enabled, face.armed);
    if face.enabled && cell.main.clicked() {
        if let Some(action) = face.action {
            actions.push(action);
        }
        // The main face runs its method whether the list is open or not; an open list goes away with
        // the run, because the click answered the question the list was asking.
        state.close_list();
    }
    if cell.caret.clicked() {
        state.toggle_list(tool);
    }
    if !state.list_open_for(tool) {
        return None;
    }
    let shown = list::show(ui, &mut state.list, tool, &rows, &cell.main, pinned, lang);
    apply_choice(state, tool, &rows, shown.choice, actions);
    shown.rect
}

/// What the main face of a split cell draws and runs.
///
/// It is the pinned method, not the tool: the glyph is the method's artwork when it has any and the
/// tool's otherwise, because a method that shares its tool's shape must not leave the face blank,
/// and the hint is the row's own, so a method that cannot run here still says what is missing.
struct MainFace {
    /// The glyph of the face.
    icon: Icon,
    /// The hint, which names the method and its key rather than the tool.
    hint: String,
    /// Whether the method can run here.
    enabled: bool,
    /// Whether the method owns the next click of the viewport.
    armed: bool,
    /// What a click asks the editor to run.
    action: Option<ToolAction>,
}

/// The main face of a split cell: the row of the pinned or armed method, read as one cell.
///
/// A fill cell always draws the tool's own glyph: the two rows are two builds of one panel, and the
/// cell must keep saying «Post» or «Shelf» as the mode changes (ADR 0110). Any other method draws
/// its own artwork, falling back to the tool's when it shares it.
fn main_face(tool: &'static ToolSpec, rows: &[list::Row], index: usize) -> Option<MainFace> {
    let row = rows.get(index)?;
    let icon = match row.method.run {
        Some(ToolKind::Fill) => tool.icon?,
        _ => row.method.icon.or(tool.icon)?,
    };
    Some(MainFace { icon, hint: row.hint.clone(), enabled: row.enabled, armed: row.armed, action: row.action })
}
/// The rows of one tool's list: every method the map declares, in order, planned ones included.
///
/// A planned method stays in the list as a disabled row, unlike a planned tool, which the ribbon
/// skips: the map has room to say «a circle can also be drawn tangent to two lines» before the
/// command exists, and the row says so without pretending to run (U67).
fn method_rows(tool: &'static ToolSpec, state: &Toolbars, context: RibbonContext, lang: Lang) -> Vec<list::Row> {
    tool.methods()
        .map(|method| {
            let ready = method.run.map(|kind| ready(kind, tool, state, context, lang));
            let hint = match ready.as_ref() {
                None => method.label.text(lang).to_owned(),
                // A method that cannot run here says what is missing.
                Some(ready) if !ready.enabled => ready.hint.clone(),
                // A fill's own hint names the tool and then the method — «Post: Rectangle» —
                // because the two fills are two cells whose methods would otherwise read the same.
                // A body cell's method says what it builds — «Box: draw its base on the work plane»
                // — because «From corner» alone would name the sketch tool it borrows (ADR 0138).
                // The two extrude methods name the payload the press takes — «Extrude the selected
                // path», «Pull an area of the selected drawing» — because the cell's one glyph says
                // nothing about which drawing it holds (X100).
                Some(ready)
                    if matches!(
                        method.run,
                        Some(ToolKind::Fill | ToolKind::Body(_) | ToolKind::Extrude | ToolKind::AreaPull)
                    ) =>
                {
                    ready.hint.clone()
                }
                // A primitive cell names its shape as well: a sphere and a cone share the sketch's
                // circle methods, so the method alone would read twice in one row of the ribbon
                // (ADR 0139, 0141).
                Some(_) if matches!(method.run, Some(ToolKind::Primitive { .. })) => {
                    Message::ToolMethodName.with(lang, [tool.label.text(lang), method.label.text(lang)])
                }
                // Every other tool names the method alone: the glyph already says which tool it is.
                Some(_) => icons::tool_hint(method.label.text(lang), method.shortcut),
            };
            list::Row {
                method,
                hint,
                enabled: ready.as_ref().is_some_and(|ready| ready.enabled),
                armed: ready.as_ref().is_some_and(|ready| ready.armed),
                action: ready.and_then(|ready| ready.action),
            }
        })
        .collect()
}

/// Applies what one open list asked for: run a method, pin it, or nothing.
fn apply_choice(
    state: &mut Toolbars,
    tool: &'static ToolSpec,
    rows: &[list::Row],
    choice: Option<list::ListChoice>,
    actions: &mut Vec<ToolAction>,
) {
    match choice {
        Some(list::ListChoice::Run(index)) => {
            if let Some(action) = rows.get(index).and_then(|row| row.action) {
                actions.push(action);
            }
            state.close_list();
        }
        Some(list::ListChoice::Pin(index)) => {
            if let Some(row) = rows.get(index) {
                state.pin(tool, row.method);
            }
        }
        None => {}
    }
}

/// The drawing tool a key press runs.
///
/// A key that names a tool's own method runs the method the workspace pinned for that tool, without
/// opening the list; a key that names one of the other methods (`Shift+C` for a circle through three
/// points) still runs exactly that method. The map is the only source: a tool whose pinned id is
/// unknown in this build falls back to its first method.
pub fn shortcut_method(tool: DrawTool, pinned: &BTreeMap<String, String>) -> DrawTool {
    let Some(spec) = tool_of_default(tool) else { return tool };
    let index = spec.pinned_index(pinned);
    match spec.methods().nth(index).and_then(|method| method.run) {
        Some(ToolKind::Draw(pinned_tool)) => pinned_tool,
        _ => tool,
    }
}

/// The tool whose own method is `tool`, the one its key is the shortcut of.
fn tool_of_default(tool: DrawTool) -> Option<&'static ToolSpec> {
    for tab in spec::tabs() {
        for group in tab.live_groups() {
            for tool_spec in group.live_tools() {
                if tool_spec.default_method().run == Some(ToolKind::Draw(tool)) {
                    return Some(tool_spec);
                }
            }
        }
    }
    None
}

/// What one ribbon cell draws and what a click on it runs.
#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;
    use crate::draw::DEFAULT_POLYGON_SIDES;
    use crate::toolbars::ready::{extrude_hint, ready};
    use crate::toolbars::spec::{self, TabSpec, VariantSpec};

    /// A context where every live tool can run and the viewport owns its view.
    fn full_context() -> RibbonContext {
        RibbonContext {
            has_panel_material: true,
            product_open: true,
            selected: None,
            selected_path: None,
            selected_ring: false,
            selected_drawing: false,
            selected_body: false,
            active: None,
            perspective: true,
            owns_view: true,
            snaps: model::SnapKinds::ALL,
            snap_mode: model::SnapMode::default(),
            shadows: true,
            draft: None,
            build: crate::editor::PanelBuild::default(),
        }
    }

    /// A tab that holds only planned tools, so the ribbon's own rules can be tested without editing
    /// the approved map.
    static PLANNED_ONLY: TabSpec = TabSpec {
        id: "test",
        label: Message::RibbonScene,
        groups: &[GroupSpec { id: "test.group", label: Message::GroupCreate, tools: &[] }],
    };

    /// The tab `id`, as the map declares it.
    fn tab(id: &str) -> &'static TabSpec {
        spec::tab(id).expect("the map declares this tab")
    }

    /// Runs the ribbon with `context` and returns its text shapes and accessible labels.
    fn draw(state: &mut Toolbars, context: RibbonContext) -> (Vec<String>, Vec<String>, Vec<ToolAction>) {
        draw_in(state, context, Lang::En)
    }

    /// Runs the ribbon in one language, so a switch can be compared without a document.
    fn draw_in(
        state: &mut Toolbars,
        context: RibbonContext,
        lang: Lang,
    ) -> (Vec<String>, Vec<String>, Vec<ToolAction>) {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        crate::set_language(&ctx, lang);
        let mut actions = Vec::new();
        let output = ctx.run_ui(Default::default(), |ui| {
            show(ui, state, context, &mut actions);
        });
        let texts = drawn_text(&output.shapes);
        let labels = accessible_labels(&output);
        output.drop_without_applying_deltas();
        (texts, labels, actions)
    }

    /// Runs the ribbon for `frames` frames on one context, so the popup the ribbon opens can finish
    /// its sizing pass and draw its contents.
    fn draw_frames(
        state: &mut Toolbars,
        context: RibbonContext,
        frames: usize,
    ) -> (Vec<String>, Vec<String>, Vec<ToolAction>) {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        crate::set_language(&ctx, Lang::En);
        let mut actions = Vec::new();
        let mut out = (Vec::new(), Vec::new(), Vec::new());
        for _ in 0..frames {
            let output = ctx.run_ui(Default::default(), |ui| {
                show(ui, state, context, &mut actions);
            });
            out = (drawn_text(&output.shapes), accessible_labels(&output), actions.clone());
            output.drop_without_applying_deltas();
        }
        out
    }

    /// The tool `id` of the map.
    fn tool(id: &str) -> &'static ToolSpec {
        spec::tool(id).expect("the map declares this tool")
    }

    /// The hints of the cells a tab draws, with its own tab buttons and the caret faces filtered out.
    ///
    /// A split cell publishes two labels — the method and the list behind the caret — and this helper
    /// answers the tool row, not the caret; [`caret_hints`] counts those.
    fn tool_hints(state: &mut Toolbars, context: RibbonContext) -> Vec<String> {
        let (_, labels, _) = draw(state, context);
        let tabs = spec::tabs().iter().map(|tab| tab.label.text(Lang::En)).collect::<Vec<_>>();
        let caret = Message::ToolMethods.text(Lang::En);
        let mut hints = labels
            .into_iter()
            .filter(|label| !tabs.iter().any(|tab| tab == label) && label != caret)
            .collect::<Vec<_>>();
        hints.sort();
        hints
    }

    /// The number of listed methods a caret face asks the list for.
    fn caret_hints(state: &mut Toolbars, context: RibbonContext) -> usize {
        let (_, labels, _) = draw(state, context);
        let caret = Message::ToolMethods.text(Lang::En);
        labels.iter().filter(|label| label.as_str() == caret).count()
    }

    #[test]
    fn the_ribbon_starts_open_on_a_tab_that_draws_a_row() {
        let mut state = Toolbars::new();
        assert!(state.is_open(), "a fresh ribbon shows its body");
        assert_eq!(state.tab().id, "sketch", "a fresh ribbon opens on the first drawn tab");
        assert!(!state.toggle());
        assert!(!state.is_open());
        assert!(state.toggle());
        assert!(state.is_open());
        assert!(!state.set_open(false));
        assert!(!state.is_open());
    }

    #[test]
    fn every_tab_draws_its_live_tools_and_nothing_else() {
        // The map is the contract: these are the hints of the live tools of each tab, and a planned
        // row must not reach the ribbon at all.
        let expected: [(&str, &[&str]); 5] = [
            (
                "sketch",
                &[
                    "A draft needs two segments to close",
                    "By radius  (N)",
                    "Centre and radius  (C)",
                    "From corner  (R)",
                    // The three boolean cells arm a mode over the selection; their hints are their
                    // own names, so the row reads which operation the next pick serves (G3).
                    "Intersect",
                    "Measure  (M)",
                    "No draft is running  (Esc)",
                    "Polyline · RMB line/arc · Alt ignores snaps  (P)",
                    "Subtract",
                    "Through 3 points  (A)",
                    "Two points  (L)",
                    "Union",
                ],
            ),
            (
                "model",
                &[
                    "Box: draw its base on the work plane",
                    // The three body boolean cells arm a mode over the selected body; like the
                    // contour cells, their hint is their own name (W1).
                    "Intersect",
                    // A primitive cell names its shape and its pinned method: a sphere and a cone
                    // share the sketch's circle methods, so the method alone would read twice.
                    "Cone: Centre and radius",
                    "Cylinder: draw its base on the work plane",
                    "Select a closed path or a drawing first",
                    "Sphere: Centre and radius",
                    "Subtract",
                    "Union",
                ],
            ),
            (
                "furniture",
                &["Panel from contour: click the outline, then close the ring", "Panel: Rectangle", "Product frame"],
            ),
            ("scene", &["Assembly", "Camera", "Light"]),
            (
                "view",
                &[
                    "All snaps",
                    "Centre",
                    // The mode cell's hint is the sentence about the two readings, because one
                    // glyph cannot carry them (U119).
                    "How a snap reads a corner above the drawing plane: 2.5D takes its foot on the \
                     plane, 3D takes the point itself. The cell shows the reading in force; press to \
                     switch.",
                    "Corner",
                    "Crossing",
                    "Frame all  (F)",
                    "Grid  (G)",
                    "Middle",
                    "On a face",
                    "On an edge",
                    "Perpendicular",
                    "Shadows  (S)",
                    "Switch the projection  (O)",
                    "Tangent",
                    "World axes",
                ],
            ),
        ];
        for (id, hints) in expected {
            let mut state = Toolbars::new();
            state.set_tab(tab(id));
            let drawn = tool_hints(&mut state, full_context());
            let mut wanted = hints.to_vec();
            wanted.sort_unstable();
            assert_eq!(drawn, wanted, "the {id} tab draws a different set of tools");
            let listed = tab(id).live_groups().flat_map(GroupSpec::live_tools).filter(|tool| tool.has_list()).count();
            assert_eq!(caret_hints(&mut state, full_context()), listed, "one caret face per tool with a list");
        }
    }

    #[test]
    fn a_group_of_planned_tools_is_skipped_with_its_caption() {
        let mut state = Toolbars::new();
        state.set_tab(tab("sketch"));
        let (texts, _, _) = draw(&mut state, full_context());
        for caption in ["СОЗДАТЬ", "ЧЕРНОВИК"] {
            let caption = if caption == "СОЗДАТЬ" { "CREATE" } else { "DRAFT" };
            assert!(texts.iter().any(|text| text == caption), "{caption} names a live group: {texts:?}");
        }
        for caption in ["MODIFY", "CONSTRUCTION", "DIMENSIONS"] {
            assert!(!texts.iter().any(|text| text == caption), "{caption} holds no live tool: {texts:?}");
        }
        // The sketch tab's three cells combine **drawings** (G3), and since W2 their caption says so:
        // a body is cut on the Model tab, whose own group keeps the word "Booleans".
        assert!(texts.iter().any(|text| text == "CONTOURS"), "CONTOURS holds three live tools: {texts:?}");
        assert!(!texts.iter().any(|text| text == "BOOLEANS"), "the sketch tab cuts no body: {texts:?}");
    }

    #[test]
    fn the_view_tab_captions_the_snaps_group_and_the_display_group_that_owns_the_shadow() {
        // U91: the grid snap changes where a click lands, so it left *View › Display* for a caption
        // of its own and a person reads where a snap lives. Display holds the shadow's switch and its
        // one intensity, so it is captioned again (U107).
        for (lang, snaps, display) in [(Lang::En, "SNAPS", "DISPLAY"), (Lang::Ru, "ПРИВЯЗКИ", "ОТОБРАЖЕНИЕ")]
        {
            let mut state = Toolbars::new();
            state.set_tab(tab("view"));
            let (texts, _, _) = draw_in(&mut state, full_context(), lang);
            assert!(texts.iter().any(|text| text == snaps), "{snaps} captions the snaps group: {texts:?}");
            assert!(texts.iter().any(|text| text == display), "{display} holds a live tool: {texts:?}");
        }
    }

    #[test]
    fn a_tab_with_nothing_live_is_not_drawn_and_is_never_active() {
        assert!(!PLANNED_ONLY.is_drawn());
        assert_eq!(PLANNED_ONLY.columns(), 0);
        let mut state = Toolbars::new();
        state.set_tab(&PLANNED_ONLY);
        let (texts, _, _) = draw(&mut state, full_context());
        assert_eq!(state.tab().id, "sketch", "the ribbon fell back to the first drawn tab");
        assert!(!texts.iter().any(|text| text == PLANNED_ONLY.label.text(Lang::En) && text == "Scene" && false));
    }

    #[test]
    fn the_body_is_one_tool_row_and_one_caption_band() {
        // The approved body is the 34 pt tool row and the 16 pt caption band: a row that grows an
        // extra field band is a regression (U68, revised by U75).
        let mut state = Toolbars::new();
        state.set_tab(tab("model"));
        let ctx = egui::Context::default();
        let mut drawn = 0.0;
        let output = ctx.run_ui(Default::default(), |ui| {
            let before = ui.min_rect().bottom();
            show(ui, &mut state, full_context(), &mut Vec::new());
            drawn = ui.min_rect().bottom() - before;
        });
        output.drop_without_applying_deltas();
        assert!((drawn - self::height(true)).abs() < 0.01, "the open ribbon draws {drawn} pt");
        assert!((drawn - 80.0).abs() < 0.01, "the approved open ribbon is 80 pt, not {drawn}");
    }

    #[test]
    fn every_group_caption_sits_centred_with_air_above_and_below() {
        // U75: at 12 pt a 9.5 pt line of capitals touched the tool row above it and the rule below.
        // The band is 16 pt now, painted a step off the control tone instead of the darkest chrome,
        // and every caption of every drawn tab keeps its air on both sides in both languages.
        for lang in [Lang::En, Lang::Ru] {
            let ctx = egui::Context::default();
            crate::theme::install(&ctx);
            crate::set_language(&ctx, lang);
            let mut checked = 0;
            for spec in spec::tabs().iter().filter(|tab| tab.is_drawn()) {
                let mut state = Toolbars::new();
                state.set_tab(spec);
                let mut fill = egui::Color32::TRANSPARENT;
                let output = ctx.run_ui(Default::default(), |ui| {
                    show(ui, &mut state, full_context(), &mut Vec::new());
                    fill = crate::theme::caption_band(ui);
                });
                let band = band_of(&output.shapes, fill);
                let band = band.unwrap_or_else(|| panic!("the {} tab captions no group", spec.id));
                assert!(
                    (band.height() - RIBBON_CAPTION_HEIGHT).abs() < 0.01,
                    "the {} band is {:.2} pt tall",
                    spec.id,
                    band.height()
                );
                let mut baseline: Option<f32> = None;
                for group in spec.live_groups() {
                    let name = group.label.text(lang).to_uppercase();
                    let caps = text_cap_line(&output.shapes, &name);
                    let (top, line) = caps.unwrap_or_else(|| panic!("{} is not captioned", group.id));
                    let ink = text_ink(&output.shapes, &name).expect("a drawn caption has glyphs");
                    let above = top - band.top();
                    let below = band.bottom() - line;
                    assert!(
                        ((top + line) / 2.0 - band.center().y).abs() < 0.5,
                        "{}: the capitals are not centred: {above:.2} pt above, {below:.2} pt below",
                        group.id
                    );
                    assert!(above >= 3.0, "{}: the capitals keep {above:.2} pt above them", group.id);
                    assert!(below >= 3.0, "{}: the capitals keep {below:.2} pt below them", group.id);
                    assert!(
                        ink.top() >= band.top() && ink.bottom() <= band.bottom(),
                        "{}: the glyphs leave the band: {ink:?} in {band:?}",
                        group.id
                    );
                    match baseline {
                        Some(first) => assert!(
                            (line - first).abs() < 0.01,
                            "{}: its baseline is {:.2} pt off the group beside it",
                            group.id,
                            line - first
                        ),
                        None => baseline = Some(line),
                    }
                    checked += 1;
                }
                output.drop_without_applying_deltas();
            }
            assert!(checked >= 5, "every drawn tab was read, not {checked} captions");
        }
    }

    #[test]
    fn the_reserved_height_is_the_ribbon_it_draws() {
        // The shell reserves `height(open)` for the floating ribbon and the strip draws inside it;
        // a mismatch would leave a side panel header under the strip or a gap above it (U51).
        for open in [false, true] {
            let mut state = Toolbars::new();
            state.set_open(open);
            let ctx = egui::Context::default();
            crate::theme::install(&ctx);
            let mut height = 0.0;
            let output = ctx.run_ui(Default::default(), |ui| {
                let before = ui.min_rect().bottom();
                show(ui, &mut state, full_context(), &mut Vec::new());
                height = ui.min_rect().bottom() - before;
            });
            output.drop_without_applying_deltas();
            let expected = self::height(open) - 2.0 * f32::from(RIBBON_FRAME_MARGIN[1]);
            assert!(
                (height - expected).abs() < 0.01,
                "the ribbon draws {height} pt inside its frame but reserves {expected} pt (open: {open})"
            );
        }
        assert_eq!(height(false), 29.0, "the folded ribbon is its tab row and one hairline");
        assert_eq!(height(true), 80.0, "the open ribbon is the tab row, the tool row and the band");
    }

    #[test]
    fn collapsing_keeps_the_tab_row_and_drops_the_body() {
        let mut state = Toolbars::new();
        state.set_tab(tab("scene"));
        let (open_texts, _, _) = draw(&mut state, full_context());
        assert!(open_texts.iter().any(|text| text == "OBJECTS"), "the group caption is drawn: {open_texts:?}");
        state.set_open(false);
        let (closed_texts, _, _) = draw(&mut state, full_context());
        assert!(closed_texts.iter().any(|text| text == "Scene"), "the tab row survives: {closed_texts:?}");
        assert!(!closed_texts.iter().any(|text| text == "OBJECTS"), "no caption without a body: {closed_texts:?}");
    }

    #[test]
    fn clicking_the_open_tab_collapses_and_restores_it() {
        let mut state = Toolbars::new();
        assert_eq!(state.tab().id, "sketch");
        apply_tab_gesture(&mut state, tab("sketch"), false);
        assert!(!state.is_open(), "a click on the open tab collapses the body");
        apply_tab_gesture(&mut state, tab("sketch"), false);
        assert!(state.is_open(), "the same click restores it");
    }

    #[test]
    fn clicking_another_tab_opens_it_and_a_double_click_collapses_it() {
        let mut state = Toolbars::new();
        apply_tab_gesture(&mut state, tab("model"), false);
        assert_eq!(state.tab().id, "model");
        assert!(state.is_open(), "switching tabs shows the new body");
        apply_tab_gesture(&mut state, tab("model"), true);
        assert!(!state.is_open(), "a double-click collapses the body");
        apply_tab_gesture(&mut state, tab("model"), true);
        assert!(state.is_open(), "a double-click restores it");
        apply_tab_gesture(&mut state, tab("scene"), true);
        assert_eq!(state.tab().id, "scene", "a double-click also switches tabs");
        assert!(!state.is_open());
    }

    #[test]
    fn only_the_arming_tool_owns_the_next_click() {
        let post = ClassCode::VERTICAL_PANEL;
        let path = DrawIntent::Path;
        let panel = DrawIntent::Panel;
        let frame = DrawIntent::Product;
        let frame_box = DrawIntent::ProductBox;
        let sphere = DrawIntent::Primitive(PrimitiveKind::Sphere);
        let cases = [
            (None, false, None, false, None),
            (Some((DrawTool::Line, path)), false, None, false, Some(ToolActivity::Draw(DrawTool::Line))),
            (
                Some((DrawTool::Rectangle, DrawIntent::Body)),
                false,
                None,
                false,
                Some(ToolActivity::Body(DrawTool::Rectangle)),
            ),
            (Some((DrawTool::Polyline, panel)), false, None, false, Some(ToolActivity::PanelContour)),
            // A frame's base is a draft of its own: the rectangle presses the drawn box's row and the
            // free outline the contour's (N19, ADR 0101, ADR 0159).
            (
                Some((DrawTool::Rectangle, frame_box)),
                false,
                None,
                false,
                Some(ToolActivity::ProductBox(DrawTool::Rectangle)),
            ),
            (Some((DrawTool::Polyline, frame)), false, None, false, Some(ToolActivity::ProductContour)),
            // A sphere is a draft too: the round it draws presses the sphere's own cell.
            (
                Some((DrawTool::Sphere, sphere)),
                false,
                None,
                false,
                Some(ToolActivity::Primitive(PrimitiveKind::Sphere)),
            ),
            (None, true, None, false, Some(ToolActivity::ProductFrame)),
            (
                None,
                false,
                Some((post, FillMode::Fit)),
                false,
                Some(ToolActivity::Fill { class: post, mode: FillMode::Fit }),
            ),
            (None, false, None, true, Some(ToolActivity::Measure)),
            (Some((DrawTool::Arc, path)), false, None, false, Some(ToolActivity::Draw(DrawTool::Arc))),
            (None, false, None, false, Some(ToolActivity::ContourBoolean(ContourOp::Union))),
            (None, false, None, false, Some(ToolActivity::BodyBoolean(ContourOp::Subtract))),
            // A boolean owns the picks, so a running draft outranks it: two pressed cells for one
            // pointer would lie about which tool the next click serves.
            (Some((DrawTool::Line, path)), false, None, false, Some(ToolActivity::Draw(DrawTool::Line))),
        ];
        for (drawing, frame, fill, measure, expected) in cases {
            // A case whose expected activity is a boolean arms one; every other case leaves it off.
            let boolean = match expected {
                Some(ToolActivity::ContourBoolean(op)) => Some((crate::editor::BooleanOperand::Rings, op)),
                Some(ToolActivity::BodyBoolean(op)) => Some((crate::editor::BooleanOperand::Bodies, op)),
                _ => None,
            };
            assert_eq!(
                tool_activity(drawing, frame, fill, measure, false, boolean),
                expected,
                "drawing={drawing:?} frame={frame} fill={fill:?} measure={measure} boolean={boolean:?}"
            );
        }
    }

    #[test]
    fn an_armed_area_pull_is_the_pressed_extrude_cell() {
        let activity = tool_activity(None, false, None, false, true, None);
        assert_eq!(activity, Some(ToolActivity::AreaPull));
        assert_eq!(tool_activity(None, false, None, false, false, None), None, "no pull, no armed cell");
    }

    #[test]
    fn a_button_is_pressed_only_for_its_own_activity() {
        let cases = [
            (None, ToolKind::Draw(DrawTool::Line), false),
            (Some(ToolActivity::Draw(DrawTool::Line)), ToolKind::Draw(DrawTool::Line), true),
            (Some(ToolActivity::Draw(DrawTool::Line)), ToolKind::Draw(DrawTool::Circle), false),
            (Some(ToolActivity::Draw(DrawTool::Line)), ToolKind::PanelContour, false),
            (Some(ToolActivity::PanelContour), ToolKind::PanelContour, true),
            (Some(ToolActivity::PanelContour), ToolKind::Draw(DrawTool::Polyline), false),
            (
                Some(ToolActivity::Primitive(PrimitiveKind::Sphere)),
                ToolKind::Primitive { kind: PrimitiveKind::Sphere, tool: DrawTool::Sphere },
                true,
            ),
            (
                Some(ToolActivity::Primitive(PrimitiveKind::Sphere)),
                ToolKind::Primitive { kind: PrimitiveKind::Sphere, tool: DrawTool::SphereDiameter },
                true,
            ),
            (
                Some(ToolActivity::Primitive(PrimitiveKind::Sphere)),
                ToolKind::Primitive { kind: PrimitiveKind::Sphere, tool: DrawTool::SphereThreePoints },
                true,
            ),
            (Some(ToolActivity::Body(DrawTool::Rectangle)), ToolKind::Body(DrawTool::Rectangle), true),
            (Some(ToolActivity::Body(DrawTool::Rectangle)), ToolKind::Body(DrawTool::Circle), false),
            (Some(ToolActivity::Body(DrawTool::Circle)), ToolKind::Draw(DrawTool::Circle), false),
            (Some(ToolActivity::Primitive(PrimitiveKind::Sphere)), ToolKind::Draw(DrawTool::Circle), false),
            (Some(ToolActivity::Primitive(PrimitiveKind::Sphere)), ToolKind::Light, false),
            (Some(ToolActivity::ProductFrame), ToolKind::ProductBox(DrawTool::Rectangle), true),
            (Some(ToolActivity::ProductBox(DrawTool::Rectangle)), ToolKind::ProductBox(DrawTool::Rectangle), true),
            (Some(ToolActivity::ProductBox(DrawTool::Rectangle)), ToolKind::ProductContour, false),
            (Some(ToolActivity::ProductFrame), ToolKind::Product, false),
            (
                Some(ToolActivity::ProductFrame),
                ToolKind::Primitive { kind: PrimitiveKind::Sphere, tool: DrawTool::Sphere },
                false,
            ),
            (Some(ToolActivity::Measure), ToolKind::Measure, true),
            (Some(ToolActivity::Measure), ToolKind::FrameAll, false),
            (Some(ToolActivity::Draw(DrawTool::Line)), ToolKind::Measure, false),
        ];
        for (activity, kind, expected) in cases {
            assert_eq!(is_armed(activity, kind), expected, "activity={activity:?} kind={kind:?}");
        }
    }

    #[test]
    fn the_ribbon_marks_exactly_the_armed_tool() {
        let cases = [
            (None, "sketch", 0, "an idle ribbon presses nothing"),
            (Some(ToolActivity::Draw(DrawTool::Line)), "sketch", 1, "the armed Line button is pressed"),
            (Some(ToolActivity::Primitive(PrimitiveKind::Sphere)), "model", 1, "the armed Sphere button is pressed"),
        ];
        for (activity, id, expected, reason) in cases {
            let ctx = egui::Context::default();
            let mut state = Toolbars::new();
            state.set_tab(tab(id));
            let mut actions = Vec::new();
            let mut pressed_fill = None;
            let context = RibbonContext { active: activity, ..full_context() };
            let output = ctx.run_ui(Default::default(), |ui| {
                pressed_fill = Some(ui.visuals().selection.bg_fill);
                show(ui, &mut state, context, &mut actions);
            });
            let pressed_fill = pressed_fill.expect("the ribbon ran");
            let pressed = drawn_fill(&output.shapes, pressed_fill);
            output.drop_without_applying_deltas();
            assert_eq!(pressed, expected, "{reason}");
        }
    }

    #[test]
    fn the_bodies_group_only_offers_extrude_for_a_closed_path() {
        let mut state = Toolbars::new();
        state.set_tab(tab("model"));
        let (_, labels, _) = draw(&mut state, full_context());
        assert!(labels.iter().any(|label| label.contains("Select a closed path or a drawing first")));
        assert_eq!(state.extrude_height(), Length::from_millimetres(100).expect("height"));
    }

    #[test]
    fn a_cell_paints_one_icon_and_keeps_its_size() {
        // A ribbon cell is a glyph in a fixed cell: one image, 26 pt of artwork, 38 x 34 of cell
        // (ADR 0067 revised by ADR 0091).
        let icon = icons::contour::LINE;
        assert!(icon.is_drawn(), "the Line asset is drawn, so the test exercises the image path");
        let ctx = egui::Context::default();
        let mut size = None;
        let output = ctx.run_ui(Default::default(), |ui| {
            let response = icons::ribbon_cell(ui, RIBBON_CELL_WIDTH, icon, "Line  (L)", true, false);
            size = Some(response.rect.size());
        });
        let images = count_images(&output.shapes);
        let texts = drawn_text(&output.shapes);
        output.drop_without_applying_deltas();
        assert_eq!(size, Some(egui::vec2(RIBBON_CELL_WIDTH, RIBBON_ROW_HEIGHT)), "the cell is the metric");
        assert_eq!(images, 1, "the icon is painted as one image");
        assert!(texts.is_empty(), "a cell carries no caption: {texts:?}");
    }

    #[test]
    fn a_body_enables_the_three_boolean_cells_and_a_path_payload_alone_does_not() {
        // The context's ring is the profile a body grew from, so the three cells run on a body
        // exactly as on the drawing it was extruded from (X98); the extrude cell keeps its own rule,
        // because a body has no path payload to extrude a second time.
        let mut state = Toolbars::new();
        state.set_tab(tab("sketch"));
        let node = Id::new(7).expect("id");
        let body = RibbonContext { selected: Some(node), selected_ring: true, ..full_context() };
        let path = RibbonContext { selected: Some(node), selected_path: Some(true), ..full_context() };
        for id in ["sketch.booleans.union", "sketch.booleans.subtract", "sketch.booleans.intersect"] {
            let cell = tool(id);
            let kind = cell.run.expect("a boolean cell runs a tool");
            assert!(ready(kind, cell, &state, body, Lang::En).enabled, "{id} runs on a body's profile");
            assert!(!ready(kind, cell, &state, path, Lang::En).enabled, "{id} needs a ring, not a path alone");
        }
        let extrude = tool("model.create.extrude");
        let kind = extrude.run.expect("the extrude cell runs a tool");
        assert!(!ready(kind, extrude, &state, body, Lang::En).enabled, "a body is not a path to extrude");
        assert!(ready(kind, extrude, &state, path, Lang::En).enabled, "a closed path still extrudes");
    }

    #[test]
    fn a_drawing_extrudes_whole_from_the_cell_and_pulls_one_area_from_its_second_method() {
        // One press turns the whole drawing into bodies — a ring into one, a set into one per ring
        // (X100) — and the pointer gesture that pulls one area out of a drawing is the same cell's
        // second method, so a set offers both doors and a closed path offers only the first.
        let state = Toolbars::new();
        let node = Id::new(7).expect("id");
        let cell = tool("model.create.extrude");
        let kind = cell.run.expect("the extrude cell runs a tool");
        let area = cell
            .live_methods()
            .find(|method| matches!(method.run, Some(ToolKind::AreaPull)))
            .expect("the cell offers the area pull as a method");
        let area_kind = area.run.expect("the method runs");

        let drawing = RibbonContext { selected: Some(node), selected_drawing: true, ..full_context() };
        let whole = ready(kind, cell, &state, drawing, Lang::En);
        assert!(whole.enabled, "a drawing of several rings extrudes whole");
        assert!(matches!(whole.action, Some(ToolAction::Extrude { object, .. }) if object == node));
        assert_eq!(whole.hint, "Extrude the selected drawing");

        let pull = ready(area_kind, cell, &state, drawing, Lang::En);
        assert!(pull.enabled, "the drawing has areas to name");
        assert_eq!(pull.action, Some(ToolAction::AreaPull { object: node }));
        assert!(!pull.armed, "the method is not pressed until it arms the pointer");
        assert_eq!(pull.hint, "Pull an area of the selected drawing");

        let armed = RibbonContext { active: Some(ToolActivity::AreaPull), ..drawing };
        assert!(ready(area_kind, cell, &state, armed, Lang::En).armed, "the method draws pressed once armed");

        let path = RibbonContext { selected: Some(node), selected_path: Some(true), ..full_context() };
        assert!(ready(kind, cell, &state, path, Lang::En).enabled, "a closed path still extrudes");
        let pull = ready(area_kind, cell, &state, path, Lang::En);
        assert!(!pull.enabled, "one ring has no areas to choose");
        assert_eq!(pull.hint, "Select a drawing of several rings first");
        assert_eq!(pull.action, None);
    }

    #[test]
    fn the_extrude_hint_names_what_is_missing() {
        let node = Id::new(7).expect("id");
        let nothing = "Select a closed path or a drawing first";
        assert_eq!(extrude_hint(None, None, false).text(Lang::En), nothing);
        assert_eq!(extrude_hint(None, Some(true), false).text(Lang::En), nothing);
        assert_eq!(extrude_hint(Some(node), None, false).text(Lang::En), "The selected object is not a drawing");
        assert_eq!(extrude_hint(Some(node), None, true).text(Lang::En), "Extrude the selected drawing");
        assert_eq!(
            extrude_hint(Some(node), Some(false), false).text(Lang::En),
            "The selected path is open; close it first"
        );
        assert_eq!(extrude_hint(Some(node), Some(true), false).text(Lang::En), "Extrude the selected path");
    }

    #[test]
    fn an_open_path_leaves_the_extrude_button_off() {
        let mut state = Toolbars::new();
        state.set_tab(tab("model"));
        let node = Id::new(7).expect("id");
        let context = RibbonContext { selected: Some(node), selected_path: Some(false), ..full_context() };
        let (_, labels, actions) = draw(&mut state, context);
        assert!(
            labels.iter().any(|label| label.contains("The selected path is open; close it first")),
            "the ribbon says why an open path cannot be extruded: {labels:?}"
        );
        assert!(actions.is_empty(), "a disabled button asks for no action");
    }

    #[test]
    fn a_closed_path_offers_one_extrusion_at_the_typed_height() {
        let mut state = Toolbars::new();
        state.set_tab(tab("model"));
        state.set_extrude_height(Length::from_millimetres(24).expect("height"));
        let node = Id::new(7).expect("id");
        let context = RibbonContext { selected: Some(node), selected_path: Some(true), ..full_context() };
        let (_, labels, actions) = draw(&mut state, context);
        assert!(labels.iter().any(|label| label.contains("Extrude the selected path")));
        assert_eq!(state.extrude_height(), Length::from_millimetres(24).expect("height"));
        assert!(actions.is_empty(), "drawing alone runs no tool");
    }

    #[test]
    fn the_ribbon_starts_on_the_typed_defaults() {
        // The numbers a tool needs are typed session defaults (U76): always valid, never text. A
        // primitve's size is no longer one of them, because the round it is built around is drawn
        // (ADR 0139).
        let state = Toolbars::new();
        assert_eq!(
            state.extrude_height(),
            Length::from_millimetres(DEFAULT_EXTRUDE_HEIGHT_MM).expect("the default height is a length")
        );
        assert_eq!(state.polygon_sides(), DEFAULT_POLYGON_SIDES);
    }

    #[test]
    fn the_sphere_cell_draws_its_round_with_the_pinned_method() {
        let tool = tool("model.primitives.sphere");
        let mut state = Toolbars::new();
        state.set_tab(tab("model"));
        let (actions, _) = click_cell(&mut state, tool, main_point());
        assert_eq!(
            actions,
            vec![ToolAction::DrawPrimitive { kind: PrimitiveKind::Sphere, tool: DrawTool::Sphere }],
            "the cell hands the editor the circle method and no size"
        );
        // A pinned method is what a click and the tool's key run, exactly as for a body cell.
        let pinned = BTreeMap::from([(tool.id.to_owned(), "model.primitives.sphere.three-points".to_owned())]);
        state.set_pinned_variants(pinned);
        let (actions, _) = click_cell(&mut state, tool, main_point());
        assert_eq!(
            actions,
            vec![ToolAction::DrawPrimitive { kind: PrimitiveKind::Sphere, tool: DrawTool::SphereThreePoints }],
            "the pinned method draws the round"
        );
    }

    #[test]
    fn a_body_cell_starts_the_sketch_method_that_draws_its_profile() {
        // A box and a cylinder are drawn, so their cells hand the editor a drawing tool and no size:
        // the profile's own plates carry the numbers (ADR 0138).
        let box_cell = tool("model.primitives.cuboid");
        let mut state = Toolbars::new();
        state.set_tab(tab("model"));
        let (actions, _) = click_cell(&mut state, box_cell, main_point());
        assert_eq!(actions, vec![ToolAction::DrawBody { tool: DrawTool::Rectangle }], "the box draws its base");

        let cylinder_cell = tool("model.primitives.cylinder");
        let (actions, _) = click_cell(&mut state, cylinder_cell, main_point());
        assert_eq!(actions, vec![ToolAction::DrawBody { tool: DrawTool::Circle }], "the cylinder draws its rim");
    }

    #[test]
    fn a_polygon_is_started_with_the_default_side_count() {
        let polygon = tool("sketch.create.polygon");
        let mut state = Toolbars::new();
        state.set_tab(tab("sketch"));
        let (actions, _) = click_cell(&mut state, polygon, main_point());
        assert_eq!(
            actions,
            vec![ToolAction::Draw { tool: DrawTool::RegularPolygon, sides: DEFAULT_POLYGON_SIDES }],
            "the polygon starts at the typed side count and the object card edits it afterwards"
        );
    }

    #[test]
    fn no_tab_draws_a_text_field() {
        // The ribbon is tools only: a tab that drew a `TextEdit` would be a form in the tool row.
        for tab in spec::tabs() {
            let mut state = Toolbars::new();
            state.set_tab(tab);
            let ctx = egui::Context::default();
            ctx.enable_accesskit();
            crate::set_language(&ctx, Lang::En);
            let output = ctx.run_ui(Default::default(), |ui| {
                show(ui, &mut state, full_context(), &mut Vec::new());
            });
            let text_inputs =
                accessible_roles(&output).into_iter().filter(|role| *role == egui::Role::TextInput).count();
            output.drop_without_applying_deltas();
            assert_eq!(text_inputs, 0, "{} draws a text field in its tool row", tab.id);
        }
    }

    #[test]
    fn the_three_primitive_buttons_say_what_they_draw() {
        let mut state = Toolbars::new();
        state.set_tab(tab("model"));
        let (_, labels, actions) = draw(&mut state, full_context());
        for label in
            ["Box: draw its base on the work plane", "Cylinder: draw its base on the work plane", "Centre and radius"]
        {
            assert!(labels.iter().any(|text| text.contains(label)), "{label} is missing: {labels:?}");
        }
        assert!(actions.is_empty(), "drawing alone runs no tool");
    }

    #[test]
    fn a_panel_without_a_catalogue_material_cannot_create_a_panel() {
        let mut state = Toolbars::new();
        state.set_tab(tab("furniture"));
        let context = RibbonContext { has_panel_material: false, ..full_context() };
        let (_, _, actions) = draw(&mut state, context);
        assert!(actions.is_empty(), "the Panel tool is disabled without a material");
    }

    #[test]
    fn the_panel_cell_arms_the_build_the_card_holds_only_inside_an_open_product() {
        // One cell, whatever the card says the part becomes (U123): the class and the build are the
        // card's, so the same cell arms a post, a shelf or a wall without being redrawn.
        let panel = tool("furniture.panels.panel");
        let kind = panel.primary().expect("a live tool");
        let state = Toolbars::new();
        let closed = RibbonContext { product_open: false, ..full_context() };
        let cell = ready(kind, panel, &state, closed, Lang::En);
        assert!(!cell.enabled, "a void needs an open product");
        assert!(
            !cell.hint.contains(Message::FillNeedsProduct.text(Lang::En)),
            "the hint names the tool: {}",
            cell.hint
        );
        assert!(cell.hint.contains(panel.label.text(Lang::En)), "the dim cell names itself: {}", cell.hint);
        assert_eq!(cell.action, None);

        for (class, mode) in [
            (ClassCode::VERTICAL_PANEL, FillMode::Fit),
            (ClassCode::HORIZONTAL_PANEL, FillMode::Shape),
            (ClassCode::BACK_PANEL, FillMode::Fit),
        ] {
            let context = RibbonContext {
                build: crate::editor::PanelBuild {
                    class,
                    mode,
                    overlay: false,
                    spread: crate::editor::FillSpread::One,
                },
                ..full_context()
            };
            let cell = ready(kind, panel, &state, context, Lang::En);
            assert!(cell.enabled, "an open piece offers its niche");
            assert_eq!(cell.action, Some(ToolAction::Fill { class, mode }), "the card's build is what the click arms");
        }
        assert_eq!(panel.icon, Some(icons::scene::BOARD), "the cell draws the plain board");
    }

    #[test]
    fn every_tool_has_a_distinct_shortcut() {
        // A shifted letter is a different shortcut from the bare one, so the table is unique on
        // the pair and not on the key alone.
        let mut seen = std::collections::HashSet::new();
        for tool in CONTOUR_TOOLS {
            let key = shortcut(tool).expect("every drawing tool has a key");
            let name = match key.shift {
                true => format!("Shift+{}", key.key.name()),
                false => key.key.name().to_owned(),
            };
            assert!(seen.insert(name.clone()), "{name} is reused by {}", tool.label());
            assert_eq!(shortcut_label(tool), Some(name.as_str()));
        }
        assert_eq!(shortcut(DrawTool::Line), Some(Shortcut { key: egui::Key::L, shift: false }));
        assert_eq!(shortcut(DrawTool::Rectangle), Some(Shortcut { key: egui::Key::R, shift: false }));
        assert_eq!(shortcut(DrawTool::RectangleFromCenter), Some(Shortcut { key: egui::Key::R, shift: true }));
        assert_eq!(shortcut(DrawTool::RegularPolygon), Some(Shortcut { key: egui::Key::N, shift: false }));
    }

    #[test]
    fn the_draft_group_offers_close_only_for_a_closable_draft() {
        assert!(draft_can_close(DrawTool::Polyline, 2, false), "two segments and the closing one make a ring");
        assert!(!draft_can_close(DrawTool::Polyline, 1, false), "one segment cannot close");
        assert!(!draft_can_close(DrawTool::Polyline, 4, true), "a closed ring cannot close again");
        assert!(!draft_can_close(DrawTool::Line, 2, false), "a line never closes");

        let mut state = Toolbars::new();
        state.set_tab(tab("sketch"));
        // A pending arc is a segment the draft has drawn, so a chain of two lines plus the arc can
        // close; the ribbon cannot know it is pending, so two segments is the whole test.
        let context = RibbonContext { draft: Some((DrawTool::Polyline, 2, false)), ..full_context() };
        let (_, labels, _) = draw(&mut state, context);
        assert!(labels.iter().any(|label| label.contains("Close the draft into a ring")), "the draft group is drawn");
        assert!(labels.iter().any(|label| label.contains("Cancel the draft")), "a draft can always cancel");
    }

    #[test]
    fn a_group_divider_spans_the_whole_body() {
        // The owner's complaint: the divider stopped in the middle of the tool row, so a group
        // boundary looked like a stray tick. It is as tall as the tool row plus its caption band.
        let mut state = Toolbars::new();
        state.set_tab(tab("sketch"));
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let body = RIBBON_ROW_HEIGHT + RIBBON_CAPTION_HEIGHT;
        let mut dividers = Vec::new();
        let output = ctx.run_ui(Default::default(), |ui| {
            show(ui, &mut state, full_context(), &mut Vec::new());
        });
        collect_vertical_lines(&output.shapes, &mut dividers);
        output.drop_without_applying_deltas();
        assert!(!dividers.is_empty(), "the Sketch tab separates its two live groups");
        for height in dividers {
            assert!((height - body).abs() < 0.5, "a group divider is {height} pt tall, the body is {body} pt");
        }
    }

    #[test]
    fn the_display_group_draws_the_shadow_switch_and_no_number() {
        // U110: the ribbon carries the per-view switch alone; the intensity is a workspace value and
        // its one control is the slider in Settings › View.
        let mut state = Toolbars::new();
        state.set_tab(tab("view"));
        let mut context = full_context();
        context.shadows = true;
        let (texts, labels, actions) = draw(&mut state, context);
        assert!(labels.iter().any(|label| label == "Shadows  (S)"), "the switch names itself and its key: {labels:?}");
        assert!(!texts.iter().any(|text| text == "%"), "the group draws no number: {texts:?}");
        assert!(actions.is_empty(), "drawing a cell writes nothing");

        let asked = ready(ToolKind::Shadows, tool("view.display.shadows"), &state, context, Lang::En);
        assert_eq!(asked.action, Some(ToolAction::ToggleShadows));
        assert!(asked.armed, "a shadow that is on draws pressed");
        assert!(asked.enabled, "a view always has its switch");

        context.shadows = false;
        let off = ready(ToolKind::Shadows, tool("view.display.shadows"), &state, context, Lang::En);
        assert!(!off.armed, "a shadow that is off draws quiet");
    }

    #[test]
    fn the_master_switch_reads_the_whole_set_and_asks_for_it() {
        // U116: one cell turns every snap on or off. It draws pressed only while every kind is on, and
        // from any other state — a mixed set included — one press turns the set on.
        let mut state = Toolbars::new();
        state.set_tab(tab("view"));
        let all = tool("view.snaps.all");
        let mut context = full_context();
        context.snaps = model::SnapKinds::ALL;
        let on = ready(ToolKind::SnapAll, all, &state, context, Lang::En);
        assert!(on.armed, "every kind on draws pressed");
        assert_eq!(on.action, Some(ToolAction::ToggleSnapAll), "and the press asks for the set");
        assert_eq!(on.hint, "All snaps", "the cell names itself");

        let mut mixed = model::SnapKinds::ALL;
        mixed.vertex = false;
        context.snaps = mixed;
        assert!(!ready(ToolKind::SnapAll, all, &state, context, Lang::En).armed, "a mixed set draws quiet");

        context.snaps = model::SnapKinds::none();
        assert!(!ready(ToolKind::SnapAll, all, &state, context, Lang::En).armed, "a set with nothing on draws quiet");

        // The cell is drawn in the group's first column, before the kinds it switches.
        let (_, labels, _) = draw(&mut state, full_context());
        assert!(labels.iter().any(|label| label == "All snaps"), "the cell is drawn: {labels:?}");
    }

    #[test]
    fn the_mode_cell_draws_the_reading_in_force_and_asks_for_the_switch() {
        // U119: one cell names the snap mode. The glyph is the mode in force — the point itself, or
        // its foot on the plane — and a press asks for the other reading; the hint says what the two
        // readings are, because a glyph cannot.
        let mut state = Toolbars::new();
        state.set_tab(tab("view"));
        let cell = tool("view.snaps.mode");
        let mut context = full_context();
        context.snap_mode = model::SnapMode::Space;
        let space = ready(ToolKind::SnapMode, cell, &state, context, Lang::En);
        assert_eq!(space.action, Some(ToolAction::ToggleSnapMode), "the press asks for the other reading");
        assert!(!space.armed, "a mode is not a switch that is on or off");
        assert!(space.enabled, "a viewport that owns its view always has a snap mode");
        assert_eq!(space.hint, Message::SnapModeHint.text(Lang::En), "the hint names both readings");
        assert_eq!(ready::snap_mode_icon(context.snap_mode), icons::view::SNAP_SPACE, "3D draws the point itself");

        context.snap_mode = model::SnapMode::Foot;
        assert_eq!(ready::snap_mode_icon(context.snap_mode), icons::view::SNAP_FOOT, "2.5D draws its foot");
        let (_, labels, _) = draw(&mut state, context);
        assert!(labels.iter().any(|label| label == Message::SnapModeHint.text(Lang::En)), "the cell is drawn");
    }

    #[test]
    fn a_row_and_a_tab_count_tools_only() {
        for tab in spec::tabs() {
            let mut columns = 0;
            for group in tab.live_groups() {
                assert_eq!(group.columns(), group.live_tools().count(), "{} counts more than its tools", group.id);
                columns += group.columns();
            }
            assert_eq!(tab.columns(), columns, "{} disagrees with its groups", tab.id);
        }
    }

    #[test]
    fn every_drawn_row_fits_the_narrowest_window() {
        // A split cell adds its caret column beside the cell (X36), so the body is wider than its cell
        // count by one caret per listed tool. The narrowest window Z3L opens is 640 x 480, and
        // the ribbon has no scroll area by design (ADR 0091), so a row that outgrew the window would
        // be clipped and lose a tool.
        const MIN_WINDOW_WIDTH: f32 = 640.0;
        for spec in spec::tabs().iter().filter(|tab| tab.is_drawn()) {
            let ctx = egui::Context::default();
            crate::theme::install(&ctx);
            let mut state = Toolbars::new();
            state.set_tab(spec);
            let mut drawn = 0.0;
            let output = ctx.run_ui(Default::default(), |ui| {
                let left = ui.min_rect().left();
                ribbon_body(ui, &mut state, full_context(), &mut Vec::new(), Lang::En);
                drawn = ui.min_rect().right() - left;
            });
            output.drop_without_applying_deltas();
            assert!(
                drawn <= MIN_WINDOW_WIDTH,
                "the {} body draws {drawn} pt, wider than the {MIN_WINDOW_WIDTH} pt window",
                spec.id
            );
        }
    }

    #[test]
    fn a_tool_with_more_than_one_method_has_a_list_and_one_method_has_none() {
        let line = tool("sketch.create.line");
        let polyline = tool("sketch.create.polyline");
        let circle = tool("sketch.create.circle");
        assert!(line.has_list(), "a line names a method besides two points");
        assert!(!polyline.has_list(), "polyline has one way to be drawn");
        assert!(!tool("sketch.create.polyline").variants.is_empty() || !polyline.has_list());
        assert!(circle.has_list());

        let mut state = Toolbars::new();
        assert!(state.open_list(circle));
        assert!(!state.open_list(polyline), "a one-method tool opens nothing");
        assert!(state.list_open_for(circle), "the refused open left the open list alone");
        state.close_list();
        assert!(state.list().is_none());
    }

    #[test]
    fn the_pinned_method_falls_back_to_the_first_and_drives_the_shortcut() {
        let circle = tool("sketch.create.circle");
        let mut pinned = BTreeMap::new();
        assert_eq!(circle.pinned_index(&pinned), 0, "an unpinned tool runs its own method");
        assert_eq!(shortcut_method(DrawTool::Circle, &pinned), DrawTool::Circle);

        pinned.insert(circle.id.to_owned(), "sketch.create.circle.three-points".to_owned());
        assert_eq!(circle.pinned_index(&pinned), 2);
        assert_eq!(
            shortcut_method(DrawTool::Circle, &pinned),
            DrawTool::CircleThreePoints,
            "the tool's key runs the pinned method with the list closed"
        );

        pinned.insert(circle.id.to_owned(), "sketch.create.circle.retired".to_owned());
        assert_eq!(circle.pinned_index(&pinned), 0, "an unknown id falls back to the first method");
        assert_eq!(shortcut_method(DrawTool::Circle, &pinned), DrawTool::Circle);
        assert_eq!(
            shortcut_method(DrawTool::Polyline, &pinned),
            DrawTool::Polyline,
            "a tool with one method is never redirected"
        );
    }

    #[test]
    fn the_rows_are_the_map_in_order_and_a_planned_method_stays_disabled() {
        let circle = tool("sketch.create.circle");
        let rows = method_rows(circle, &Toolbars::new(), full_context(), Lang::En);
        let ids = rows.iter().map(|row| row.method.id).collect::<Vec<_>>();
        assert_eq!(
            ids,
            [
                "sketch.create.circle",
                "sketch.create.circle.diameter",
                "sketch.create.circle.three-points",
                "sketch.create.circle.tangent",
            ],
            "the list is the map, in order, without a duplicate row"
        );
        assert!(rows.iter().all(|row| row.enabled), "every circle method runs");
        assert_eq!(
            rows[0].method.label,
            Message::ToolCircleCentreRadius,
            "the first row names the method, not the tool"
        );
        assert_eq!(rows[1].method.label, Message::ToolCircleCentreDiameter, "the diameter is a method of its own");
        assert_eq!(
            rows[1].action,
            Some(ToolAction::Draw { tool: DrawTool::CircleDiameter, sides: DEFAULT_POLYGON_SIDES })
        );
        assert_eq!(
            rows[3].action,
            Some(ToolAction::Draw { tool: DrawTool::CircleTangent, sides: DEFAULT_POLYGON_SIDES }),
            "the tangent row runs the method it names"
        );

        // A method the build does not have yet still draws its row and runs nothing.
        let offset = tool("sketch.modify.offset");
        let rows = method_rows(offset, &Toolbars::new(), full_context(), Lang::En);
        assert_eq!(rows.len(), 3, "the tool's own method and the two still planned");
        assert_eq!(rows[1].method.id, "sketch.modify.offset.distance");
        assert!(!rows[1].enabled, "a planned method is not enabled");
        assert!(rows[1].action.is_none(), "a planned method runs nothing");
        assert!(rows.iter().filter(|row| row.method.id != "sketch.modify.offset").all(|row| !row.enabled));

        // The line's own second method is the typed pair, and it runs (X53).
        let line = tool("sketch.create.line");
        let rows = method_rows(line, &Toolbars::new(), full_context(), Lang::En);
        assert_eq!(rows.len(), 2, "the line's own method and the pair of numbers");
        assert_eq!(rows[1].method.id, "sketch.create.line.angle");
        assert_eq!(rows[1].method.label, Message::ToolLineAngle, "the row names the typed entry");
        assert!(rows[1].enabled, "the by-angle method runs");
        assert_eq!(
            rows[1].action,
            Some(ToolAction::Draw { tool: DrawTool::LineByAngle, sides: DEFAULT_POLYGON_SIDES }),
            "the by-angle row starts its own tool"
        );
    }

    #[test]
    fn a_row_choice_runs_its_action_and_a_pin_keeps_the_list_open() {
        let circle = tool("sketch.create.circle");
        let mut state = Toolbars::new();
        assert!(state.open_list(circle));
        let rows = method_rows(circle, &state, full_context(), Lang::En);
        let mut actions = Vec::new();

        apply_choice(&mut state, circle, &rows, Some(list::ListChoice::Pin(2)), &mut actions);
        assert!(actions.is_empty(), "a pin runs nothing");
        assert_eq!(
            state.pinned_variants().get(circle.id).map(String::as_str),
            Some("sketch.create.circle.three-points")
        );
        assert!(state.list_open_for(circle), "pinning keeps the list open under the new accent");

        apply_choice(&mut state, circle, &rows, Some(list::ListChoice::Run(2)), &mut actions);
        assert_eq!(actions, vec![ToolAction::Draw { tool: DrawTool::CircleThreePoints, sides: DEFAULT_POLYGON_SIDES }]);
        assert!(!state.list_open_for(circle), "running a row closes the list");
    }

    #[test]
    fn a_main_face_click_runs_the_pinned_method_and_opens_nothing() {
        let circle = tool("sketch.create.circle");
        let mut state = Toolbars::new();
        state.set_tab(tab("sketch"));
        state.pin(circle, circle.methods().nth(2).expect("the three-point method"));
        let (actions, _) = click_cell(&mut state, circle, main_point());
        assert_eq!(
            actions,
            vec![ToolAction::Draw { tool: DrawTool::CircleThreePoints, sides: DEFAULT_POLYGON_SIDES }],
            "the main face runs exactly one method"
        );
        assert!(state.list().is_none(), "a main-face click opens nothing");
    }

    #[test]
    fn a_caret_click_opens_and_closes_the_list_without_running_anything() {
        let circle = tool("sketch.create.circle");
        let mut state = Toolbars::new();
        state.set_tab(tab("sketch"));
        let (actions, _) = click_cell(&mut state, circle, caret_point());
        assert!(actions.is_empty(), "the caret opens the list instead of running a method: {actions:?}");
        assert!(state.list_open_for(circle), "the caret opens the list");

        let (actions, _) = click_cell(&mut state, circle, caret_point());
        assert!(actions.is_empty(), "closing runs nothing either: {actions:?}");
        assert!(state.list().is_none(), "the second caret click closes the list");
    }

    #[test]
    fn a_main_face_click_closes_the_open_list() {
        // The list is a question; answering it by running the method takes the question away, so a
        // second click on the main face does not leave a popup hanging under the cell.
        let circle = tool("sketch.create.circle");
        let mut state = Toolbars::new();
        state.set_tab(tab("sketch"));
        assert!(state.open_list(circle));
        let (actions, _) = click_cell(&mut state, circle, main_point());
        assert_eq!(actions, vec![ToolAction::Draw { tool: DrawTool::Circle, sides: DEFAULT_POLYGON_SIDES }]);
        assert!(state.list().is_none(), "the main face closes the list it answered");
    }

    #[test]
    fn the_panel_cell_names_the_method_the_card_built() {
        // One cell, one live method — fill the niche the pointer is in — and the card says what the
        // part becomes. The build is no longer a method list: two builds were never two tools, and a
        // class is what a maker reads on the card and on the part (U123).
        let panel = tool("furniture.panels.panel");
        let mut state = Toolbars::new();
        state.set_tab(tab("furniture"));
        let methods = panel.methods().collect::<Vec<_>>();
        assert_eq!(methods.len(), 2, "the cell offers filling a void and drawing a board");
        assert_eq!(methods[0].id, "furniture.panels.panel", "the tool's own method keeps its id");
        assert_eq!(methods[0].run, Some(ToolKind::Fill), "the first row fills the niche under the pointer");
        assert_eq!(methods[1].id, "furniture.panels.panel.rectangle", "a method keeps its namespaced id");
        assert_eq!(methods[1].label.text(Lang::En), "Rectangle", "the drawn board is the second row");
        assert!(methods[1].run.is_none(), "the drawing intent that cuts a rectangle board is not in the tree yet");

        let (actions, labels) = click_cell(&mut state, panel, main_point());
        assert_eq!(
            actions,
            vec![ToolAction::Fill { class: ClassCode::VERTICAL_PANEL, mode: FillMode::Fit }],
            "an untouched card is a post fitted to the void"
        );
        assert!(
            labels.iter().any(|label| label == "Panel: Rectangle"),
            "the hint names the cell and the build the card holds: {labels:?}"
        );
    }

    #[test]
    fn the_panels_group_is_three_cells_and_nothing_else() {
        // The owner's group (U123): the panel, the contour board and the by-points board. The post,
        // shelf, wall and niche cells and the three `.shape` rows retire, and a workspace that pinned
        // one of them loads as no pin.
        let group = spec::tabs()
            .iter()
            .flat_map(|tab| tab.groups)
            .find(|group| group.id == "furniture.panels")
            .expect("the panels group");
        assert_eq!(
            group.tools.iter().map(|tool| tool.id).collect::<Vec<_>>(),
            ["furniture.panels.panel", "furniture.panels.contour", "furniture.panels.points"],
            "three cells, in that order"
        );
        for retired in [
            "furniture.panels.post",
            "furniture.panels.shelf",
            "furniture.panels.wall",
            "furniture.panels.void",
            "furniture.panels.post.shape",
            "furniture.panels.shelf.shape",
            "furniture.panels.wall.shape",
        ] {
            assert!(
                spec::tabs()
                    .iter()
                    .flat_map(|tab| tab.groups)
                    .flat_map(|group| group.tools)
                    .all(|tool| tool.id != retired && tool.variants.iter().all(|variant| variant.id != retired)),
                "`{retired}` is not a tool or a method of the map"
            );
            assert!(spec::words(retired).is_none(), "and it names no method in the settings");
        }
        let pinned = std::collections::BTreeMap::from([(
            "furniture.panels.panel".to_owned(),
            "furniture.panels.post".to_owned(),
        )]);
        let panel = tool("furniture.panels.panel");
        assert_eq!(panel.pinned_index(&pinned), 0, "a pin naming a retired cell is no pin");
    }

    #[test]
    fn a_circle_by_diameter_and_an_arc_by_radius_are_live_methods_with_artwork() {
        // C4: the two methods the person asked for are cells of their own, each with a drawn glyph,
        // and the two rows that ran nothing before are gone.
        for (id, drawn) in [
            ("sketch.create.circle.diameter", DrawTool::CircleDiameter),
            ("sketch.create.arc.start-end-radius", DrawTool::ArcByRadius),
        ] {
            let method = spec::tabs()
                .iter()
                .flat_map(|tab| tab.groups)
                .flat_map(|group| group.tools)
                .flat_map(ToolSpec::methods)
                .find(|method| method.id == id)
                .unwrap_or_else(|| panic!("`{id}` is no method of the map"));
            assert_eq!(method.run, Some(ToolKind::Draw(drawn)), "`{id}` draws its own tool");
            assert!(method.icon.is_some_and(Icon::is_drawn), "`{id}` carries its own artwork");
        }
        for gone in ["sketch.create.circle.two-points", "sketch.create.arc.three-points"] {
            assert!(
                spec::tabs()
                    .iter()
                    .flat_map(|tab| tab.groups)
                    .flat_map(|group| group.tools)
                    .flat_map(ToolSpec::methods)
                    .all(|method| method.id != gone),
                "`{gone}` is gone: the method it promised is now a cell"
            );
        }
    }

    #[test]
    fn the_open_panel_list_draws_both_methods() {
        let panel = tool("furniture.panels.panel");
        let mut state = Toolbars::new();
        state.set_tab(tab("furniture"));
        assert!(state.open_list(panel));
        let (texts, _, _) = draw_frames(&mut state, full_context(), 3);
        assert!(texts.iter().any(|text| text == "Fill in the niche"), "the live row is drawn: {texts:?}");
        assert!(texts.iter().any(|text| text == "Rectangle"), "the drawn-board row is drawn: {texts:?}");
    }

    #[test]
    fn the_panel_cell_draws_pressed_while_a_fill_is_armed_whatever_it_builds() {
        // One cell serves every class, so the pressed state answers "a fill owns the next click",
        // not "this exact class" (U123).
        let panel = tool("furniture.panels.panel");
        for class in [ClassCode::VERTICAL_PANEL, ClassCode::HORIZONTAL_PANEL, ClassCode::BACK_PANEL] {
            let context = RibbonContext {
                active: Some(ToolActivity::Fill { class, mode: FillMode::Shape }),
                build: crate::editor::PanelBuild {
                    class,
                    mode: FillMode::Shape,
                    overlay: false,
                    spread: crate::editor::FillSpread::One,
                },
                ..full_context()
            };
            let rows = method_rows(panel, &Toolbars::new(), context, Lang::En);
            let face = main_face(panel, &rows, 0).expect("a face");
            assert!(face.armed, "{class:?}: the cell draws pressed");
            assert_eq!(face.icon, icons::scene::BOARD, "the cell keeps the board's glyph");
            assert_eq!(face.hint, "Panel: Niche shape", "the face names the build the click will use");
            assert_eq!(face.action, Some(ToolAction::Fill { class, mode: FillMode::Shape }));
        }
    }

    #[test]
    fn a_disabled_fill_face_names_itself_and_its_missing_material_rather_than_the_product() {
        let post = tool("furniture.panels.panel");
        let state = Toolbars::new();
        let closed = method_rows(post, &state, RibbonContext { product_open: false, ..full_context() }, Lang::En);
        let face = main_face(post, &closed, 0).expect("a face");
        assert!(!face.enabled);
        assert_eq!(face.action, None);
        assert!(
            !face.hint.contains(Message::FillNeedsProduct.text(Lang::En)),
            "the product is not written: {}",
            face.hint
        );
        assert!(
            face.hint.contains(Message::FillMethodFit.text(Lang::En)),
            "a dim cell still names the tool and its method: {}",
            face.hint
        );

        let materialless =
            method_rows(post, &state, RibbonContext { has_panel_material: false, ..full_context() }, Lang::En);
        let face = main_face(post, &materialless, 0).expect("a face");
        assert!(
            face.hint.contains(Message::FillNeedsMaterial.text(Lang::En)),
            "a missing material is still explained: {}",
            face.hint
        );
    }

    #[test]
    fn a_pin_carries_the_main_face_to_the_new_method() {
        let circle = tool("sketch.create.circle");
        let mut state = Toolbars::new();
        state.set_tab(tab("sketch"));
        let rows = method_rows(circle, &state, full_context(), Lang::En);
        let own = main_face(circle, &rows, state.pinned_index(circle)).expect("the tool's own method");
        assert_eq!(own.icon, icons::contour::CIRCLE, "the face draws the method the workspace pinned");
        assert_eq!(own.hint, "Centre and radius  (C)", "the hint names the method and its key, not the tool");

        state.pin(circle, rows[2].method);
        let rows = method_rows(circle, &state, full_context(), Lang::En);
        let pinned = main_face(circle, &rows, state.pinned_index(circle)).expect("a face");
        assert_eq!(pinned.icon, icons::contour::CIRCLE_THREE_POINTS, "the face follows the pin at once");
        assert_eq!(pinned.hint, "Circle through 3 points  (Shift+C)", "the hint follows the pin at once");

        let (actions, labels) = click_cell(&mut state, circle, main_point());
        assert_eq!(
            actions,
            vec![ToolAction::Draw { tool: DrawTool::CircleThreePoints, sides: DEFAULT_POLYGON_SIDES }],
            "the main face runs the method the pin put there"
        );
        assert!(
            labels.iter().any(|label| label == "Circle through 3 points  (Shift+C)"),
            "the cell publishes the pinned method: {labels:?}"
        );
        assert!(labels.iter().any(|label| label == Message::ToolMethods.text(Lang::En)), "the caret names the list");
    }

    #[test]
    fn a_pinned_method_without_artwork_keeps_the_tool_glyph() {
        // A method that shares its tool's shape has no drawing of its own; the face must fall back
        // to the tool rather than leave a blank cell.
        // The ids are the map's own, because the key of a method is filed under its id (U96).
        static ARTWORKLESS: ToolSpec = ToolSpec {
            id: "sketch.create.circle",
            label: Message::ToolCircle,
            icon: Some(icons::contour::CIRCLE),
            run: Some(ToolKind::Draw(DrawTool::Circle)),
            method: Some(Message::ToolCircleCentreRadius),
            variants: &[VariantSpec {
                id: "sketch.create.circle.three-points",
                label: Message::ToolCircleThreePoints,
                icon: None,
                run: Some(ToolKind::Draw(DrawTool::CircleThreePoints)),
            }],
        };
        let rows = method_rows(&ARTWORKLESS, &Toolbars::new(), full_context(), Lang::En);
        let face = main_face(&ARTWORKLESS, &rows, 1).expect("a face");
        assert_eq!(face.icon, icons::contour::CIRCLE, "a method without artwork keeps the tool's glyph");
        assert_eq!(face.hint, "Circle through 3 points  (Shift+C)", "the hint still names the method");
    }

    #[test]
    fn the_pin_and_the_key_keep_their_columns_apart_in_both_languages() {
        // The pin is a control inside the row, and the key is a label: the columns are measured once
        // for the whole list, so the pin can never be drawn over a key (U74).
        let row = egui::Rect::from_min_size(egui::pos2(4.0, 40.0), egui::vec2(list::WIDTH, list::ROW_HEIGHT));
        let ctx = egui::Context::default();
        let output = ctx.run_ui(Default::default(), |ui| {
            for lang in [Lang::En, Lang::Ru] {
                for id in ["sketch.create.circle", "sketch.create.line", "sketch.create.arc", "sketch.create.rectangle"]
                {
                    let rows = method_rows(tool(id), &Toolbars::new(), full_context(), lang);
                    let columns = list::columns(row, list::shortcut_column(ui, &rows));
                    assert!(
                        columns.shortcut.right() < columns.pin.left(),
                        "{id} {lang:?}: the key {columns:?} reaches the pin"
                    );
                    assert!(
                        columns.label.right() <= columns.shortcut.left(),
                        "{id} {lang:?}: the name {columns:?} reaches the key"
                    );
                    assert_eq!(columns.glyph.left(), row.left() + 10.0, "the glyph keeps the row's inset");
                    assert!(columns.label.left() > columns.glyph.right(), "the name follows the glyph");
                    assert_eq!(columns.pin.width(), 20.0, "the pin's column is the pin's control");
                }
            }
        });
        output.drop_without_applying_deltas();
    }

    #[test]
    fn a_tab_change_and_a_folded_body_close_the_list() {
        let circle = tool("sketch.create.circle");
        let mut state = Toolbars::new();
        state.set_tab(tab("sketch"));
        assert!(state.open_list(circle));
        apply_tab_gesture(&mut state, tab("model"), false);
        assert!(state.list().is_none(), "a tab change closes the list");

        state.set_tab(tab("sketch"));
        assert!(state.open_list(circle));
        state.set_open(false);
        assert!(state.list().is_none(), "a folded body has no cell to anchor the list to");
    }

    #[test]
    fn a_list_that_outlived_its_tab_is_dropped_before_it_draws() {
        let circle = tool("sketch.create.circle");
        let mut state = Toolbars::new();
        state.set_tab(tab("model"));
        // A state put out of step by hand, the way a lost tab would leave it.
        state.list = Some(list::VariantList::new(circle));
        let (texts, _, _) = draw(&mut state, full_context());
        assert!(state.list().is_none(), "the ribbon drops a list it cannot anchor");
        assert!(!texts.iter().any(|text| text == "TANGENTS"), "no list is drawn: {texts:?}");
    }

    #[test]
    fn the_open_list_draws_its_header_its_rows_and_no_footer() {
        let circle = tool("sketch.create.circle");
        let mut state = Toolbars::new();
        state.set_tab(tab("sketch"));
        assert!(state.open_list(circle));
        let (texts, labels, actions) = draw_frames(&mut state, full_context(), 3);
        assert!(texts.iter().any(|text| text == "Circle"), "the header names the tool: {texts:?}");
        assert!(texts.iter().any(|text| text == "C"), "the header carries the shortcut");
        assert!(texts.iter().any(|text| text == "Centre and radius"), "the tool's own method is a row: {texts:?}");
        assert!(texts.iter().any(|text| text == "Circle through 3 points"), "a live method is a row: {texts:?}");
        assert!(!texts.iter().any(|text| text == "BUILD"), "no caption band is drawn: {texts:?}");
        assert!(
            !texts.iter().any(|text| text.contains("Enter") || text.contains("Esc")),
            "the list has no footer: {texts:?}"
        );
        assert!(labels.iter().any(|label| label.contains("Circle")), "the rows are announced: {labels:?}");
        assert!(actions.is_empty(), "opening and drawing the list runs nothing");
    }

    #[test]
    fn the_list_draws_the_rows_in_the_order_the_map_declares() {
        // One flat list of names (U84): the map's order is the drawn order, at one row pitch, with
        // no band between two rows to read over.
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let ctx = egui::Context::default();
        crate::set_language(&ctx, Lang::En);
        let circle = tool("sketch.create.circle");
        let rows = method_rows(circle, &Toolbars::new(), full_context(), Lang::En);
        let mut slot = Some(list::VariantList::new(circle));
        let (_, output) = warm_list(&ctx, screen, &mut slot);
        let tops = rows
            .iter()
            .map(|row| {
                let label = row.method.label.text(Lang::En);
                text_rect(&output.shapes, label).unwrap_or_else(|| panic!("the frame does not draw {label:?}")).top()
            })
            .collect::<Vec<_>>();
        assert!(tops.windows(2).all(|pair| pair[0] < pair[1]), "the drawn order is the declared order: {tops:?}");
        for pair in tops.windows(2) {
            assert!((pair[1] - pair[0] - list::ROW_HEIGHT).abs() < 0.5, "two rows are one row pitch apart: {tops:?}");
        }
    }

    #[test]
    fn a_list_is_its_header_plus_one_row_per_method() {
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let ctx = egui::Context::default();
        crate::set_language(&ctx, Lang::En);
        let circle = tool("sketch.create.circle");
        let rows = method_rows(circle, &Toolbars::new(), full_context(), Lang::En);
        let mut slot = Some(list::VariantList::new(circle));
        let rect = list_rect(&ctx, screen, &mut slot).expect("the popup reported its rectangle");
        assert_eq!(
            rect.height(),
            list::height(rows.len()) + 2.0 * list::FRAME_STROKE,
            "the drawn list is its header and one row per method, plus the frame's hairline"
        );
    }

    #[test]
    fn every_open_list_stays_inside_the_window_at_both_horizontal_edges() {
        // The list is anchored to its cell and flips to the other side of the anchor at an edge;
        // this is the approved anatomy's promise, and a window narrower than the list is out of
        // scope because the shell enforces a larger minimum.
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        for anchor_x in [8.0, 752.0] {
            let ctx = egui::Context::default();
            crate::set_language(&ctx, Lang::En);
            let mut slot = Some(list::VariantList::new(tool("sketch.create.circle")));
            let mut anchor = None;
            for _ in 0..2 {
                anchor = list_frame(&ctx, screen, anchor_x, &mut slot, Vec::new());
            }
            let anchor = anchor.expect("the cell drew");
            let circle = tool("sketch.create.circle");
            let rect = egui::Popup::from_response(&anchor)
                .id(egui::Id::unique(("ribbon.variants", circle.id)))
                .get_popup_rect()
                .expect("the list drew once and reports its rect");
            assert!(screen.contains_rect(rect), "the list at x = {anchor_x} leaves the window: {rect:?} in {screen:?}");
        }
    }

    #[test]
    fn escape_closes_the_open_list() {
        let circle = tool("sketch.create.circle");
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let ctx = egui::Context::default();
        crate::set_language(&ctx, Lang::En);
        let mut slot = Some(list::VariantList::new(circle));
        for _ in 0..2 {
            list_frame(&ctx, screen, 8.0, &mut slot, Vec::new());
        }
        assert!(slot.is_some(), "the list is open");
        list_frame(&ctx, screen, 8.0, &mut slot, vec![key_event(egui::Key::Escape)]);
        assert!(slot.is_none(), "Escape closes the list");
    }

    #[test]
    fn a_click_outside_the_list_closes_it() {
        let circle = tool("sketch.create.circle");
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let ctx = egui::Context::default();
        crate::set_language(&ctx, Lang::En);
        let mut slot = Some(list::VariantList::new(circle));
        for _ in 0..3 {
            list_frame(&ctx, screen, 8.0, &mut slot, Vec::new());
        }
        assert!(slot.is_some(), "the list is open");
        let outside = egui::pos2(600.0, 500.0);
        list_frame(&ctx, screen, 8.0, &mut slot, vec![pointer_button(outside, true)]);
        assert!(slot.is_some(), "the press alone is not a click");
        list_frame(&ctx, screen, 8.0, &mut slot, vec![pointer_button(outside, false)]);
        assert!(slot.is_none(), "a click outside the list closes it");
    }

    #[test]
    fn a_click_inside_the_list_keeps_it_open() {
        // The header is inside the frame and runs nothing: only a click elsewhere closes the list,
        // so a pointer that lands on the list does not dismiss it.
        let circle = tool("sketch.create.circle");
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let ctx = egui::Context::default();
        crate::set_language(&ctx, Lang::En);
        let mut slot = Some(list::VariantList::new(circle));
        for _ in 0..3 {
            list_frame(&ctx, screen, 8.0, &mut slot, Vec::new());
        }
        let inside = egui::pos2(150.0, 80.0);
        list_frame(&ctx, screen, 8.0, &mut slot, vec![pointer_button(inside, true)]);
        list_frame(&ctx, screen, 8.0, &mut slot, vec![pointer_button(inside, false)]);
        assert!(slot.is_some(), "a click inside the list leaves it open");
    }

    #[test]
    fn the_pin_is_drawn_only_on_the_hovered_or_the_highlighted_row() {
        // The pin is not part of the resting row: it appears on the row under the pointer and on the
        // row the keyboard reached, and its column stays reserved either way (U74).
        let circle = tool("sketch.create.circle");
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let ctx = egui::Context::default();
        crate::set_language(&ctx, Lang::En);
        // The keyboard sits on the last row, so the resting list draws exactly one pin.
        let mut slot = Some(list::VariantList { tool: circle.id, highlighted: 3 });
        let (_, idle) = warm_list(&ctx, screen, &mut slot);
        let base = count_images(&idle.shapes);
        assert_eq!(base, 6, "the header's glyph, the four methods and the keyboard row's pin");
        let idle_label = text_place(&idle.shapes, "Centre and radius");
        let idle_key = text_place(&idle.shapes, "Shift+C");
        // Hovering is resolved from the previous pass, so the pointer moves and the next frame paints.
        let hover = row_of(&idle.shapes, "Circle through 3 points").center();
        idle.drop_without_applying_deltas();
        let (_, moved) = list_paint(&ctx, screen, 8.0, &mut slot, vec![pointer_moved(hover)]);
        moved.drop_without_applying_deltas();
        let (_, hovered) = list_paint(&ctx, screen, 8.0, &mut slot, Vec::new());
        assert_eq!(count_images(&hovered.shapes), base + 1, "the pin appears on the row under the pointer");
        assert_eq!(text_place(&hovered.shapes, "Centre and radius"), idle_label, "the name does not move");
        assert_eq!(text_place(&hovered.shapes, "Shift+C"), idle_key, "the key does not move");
        hovered.drop_without_applying_deltas();

        // The keyboard on the same row, with the pointer away from the list.
        slot.as_mut().expect("the list is open").highlighted = 2;
        let (_, away) = list_paint(&ctx, screen, 8.0, &mut slot, vec![pointer_moved(egui::pos2(700.0, 500.0))]);
        away.drop_without_applying_deltas();
        let (_, keyed) = list_paint(&ctx, screen, 8.0, &mut slot, Vec::new());
        assert_eq!(count_images(&keyed.shapes), base, "the one pin moves with the keyboard instead of adding a second");
        let reached = row_of(&keyed.shapes, "Circle through 3 points");
        assert_eq!(fills_at(&keyed.shapes, reached).len(), 1, "the row the keyboard reached is washed");
        keyed.drop_without_applying_deltas();
    }

    #[test]
    fn the_pin_stays_under_a_pointer_that_reaches_it() {
        // The pin is a second control inside the row: a pointer that reaches it must keep the pin
        // drawn and the row washed, or the control disappears from under the hand (U74).
        let circle = tool("sketch.create.circle");
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let ctx = egui::Context::default();
        crate::set_language(&ctx, Lang::En);
        let mut slot = Some(list::VariantList { tool: circle.id, highlighted: 3 });
        let (_, idle) = warm_list(&ctx, screen, &mut slot);
        let base = count_images(&idle.shapes);
        let row = row_of(&idle.shapes, "Circle through 3 points");
        idle.drop_without_applying_deltas();
        assert_eq!(base, 6, "the header's glyph, the four methods and the keyboard row's pin");

        // One frame per step: the pointer reaches the row, then the pin at the row's right edge.
        let pin = egui::pos2(row.right() - list::ROW_INSET - list::PIN_COLUMN / 2.0, row.center().y);
        for pos in [row.center(), pin, pin] {
            let (_, output) = list_paint(&ctx, screen, 8.0, &mut slot, vec![pointer_moved(pos)]);
            output.drop_without_applying_deltas();
        }
        let (_, reached) = list_paint(&ctx, screen, 8.0, &mut slot, Vec::new());
        assert_eq!(count_images(&reached.shapes), base + 1, "the pin stays under the pointer that reached it");
        assert_eq!(fills_at(&reached.shapes, row).len(), 1, "the row stays washed under the pin");
        reached.drop_without_applying_deltas();
    }

    #[test]
    fn the_pinned_row_keeps_its_mark_when_the_pin_is_hidden() {
        // The accent bar and the fill are what says "this is the main method"; the pin only says what
        // a click on it would do, so it can hide without taking the mark away (U74).
        let circle = tool("sketch.create.circle");
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let ctx = egui::Context::default();
        crate::set_language(&ctx, Lang::En);
        // The keyboard sits on the diameter row, so the pinned first row carries no pin of its own.
        let mut slot = Some(list::VariantList { tool: circle.id, highlighted: 1 });
        let (_, idle) = warm_list(&ctx, screen, &mut slot);
        assert_eq!(count_images(&idle.shapes), 6, "the header's glyph, the four methods and the keyboard row's pin");
        let mut marked = fills_at(&idle.shapes, row_of(&idle.shapes, "Centre and radius"));
        assert_eq!(marked.len(), 2, "the pinned row keeps its fill and its accent bar");
        assert!(marked.iter().all(|fill| *fill != egui::Color32::TRANSPARENT), "the mark is painted");
        marked.sort_unstable_by_key(|fill| fill.to_array());
        assert_ne!(marked[0], marked[1], "the bar is the accent, not the row's own fill");
        assert!(
            fills_at(&idle.shapes, row_of(&idle.shapes, "Circle through 3 points")).is_empty(),
            "a row that is neither hovered nor pinned carries no mark"
        );
        idle.drop_without_applying_deltas();
    }

    /// Warms the open circle list for three frames and returns the anchor and the last paint.
    fn warm_list(
        ctx: &egui::Context,
        screen: egui::Rect,
        slot: &mut Option<list::VariantList>,
    ) -> (egui::Response, egui::FullOutput) {
        for _ in 0..2 {
            let (_, output) = list_paint(ctx, screen, 8.0, slot, Vec::new());
            output.drop_without_applying_deltas();
        }
        let (anchor, output) = list_paint(ctx, screen, 8.0, slot, Vec::new());
        (anchor.expect("the cell drew"), output)
    }

    /// The rect of the row whose name is `label`, read from where the frame painted that name.
    ///
    /// The bands above the rows are drawn by the caller, so a test asks the paint where a row is
    /// instead of recomputing the layout a second time.
    fn row_of(shapes: &[egui::epaint::ClippedShape], label: &str) -> egui::Rect {
        let name = text_rect(shapes, label).unwrap_or_else(|| panic!("the frame does not draw {label:?}"));
        egui::Rect::from_center_size(name.center(), egui::vec2(list::WIDTH, list::ROW_HEIGHT))
    }

    /// The fills of the rects painted over the height of `row`: its own fill and its marks.
    fn fills_at(shapes: &[egui::epaint::ClippedShape], row: egui::Rect) -> Vec<egui::Color32> {
        fn walk(shape: &egui::Shape, row: egui::Rect, out: &mut Vec<egui::Color32>) {
            match shape {
                egui::Shape::Rect(painted)
                    if (painted.rect.top() - row.top()).abs() < 0.5
                        && (painted.rect.bottom() - row.bottom()).abs() < 0.5
                        && painted.fill != egui::Color32::TRANSPARENT =>
                {
                    out.push(painted.fill);
                }
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        walk(shape, row, out);
                    }
                }
                _ => {}
            }
        }

        let mut out = Vec::new();
        for clipped in shapes {
            walk(&clipped.shape, row, &mut out);
        }
        out
    }

    /// The ink of one drawn text: the tight box around its glyphs, or `None` when it is not drawn.
    ///
    /// The line box of a galley is taller than its capitals, so a clearance measured from
    /// [`text_rect`] would be the air around the line, not around the name.
    fn text_ink(shapes: &[egui::epaint::ClippedShape], text: &str) -> Option<egui::Rect> {
        fn walk(shape: &egui::Shape, text: &str, out: &mut Option<egui::Rect>) {
            match shape {
                egui::Shape::Text(drawn) if drawn.galley.text() == text => {
                    *out = Some(drawn.galley.mesh_bounds.translate(drawn.pos.to_vec2()));
                }
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        walk(shape, text, out);
                    }
                }
                _ => {}
            }
        }

        let mut out = None;
        for clipped in shapes {
            walk(&clipped.shape, text, &mut out);
        }
        out
    }

    /// The top of one drawn text's capitals and its baseline, in frame coordinates.
    ///
    /// A caption is centred on this block, not on its line box or its whole ink: the box carries the
    /// descent and the ink may carry a `Д` leg or a `Ц` tail, and both would move the name off the
    /// baseline its neighbours share (U75).
    fn text_cap_line(shapes: &[egui::epaint::ClippedShape], text: &str) -> Option<(f32, f32)> {
        fn walk(shape: &egui::Shape, text: &str, out: &mut Option<(f32, f32)>) {
            match shape {
                egui::Shape::Text(drawn) if drawn.galley.text() == text => {
                    let baseline = drawn
                        .galley
                        .rows
                        .first()
                        .and_then(|row| row.row.glyphs.first().map(|glyph| row.pos.y + glyph.pos.y));
                    if let Some(baseline) = baseline {
                        *out = Some((drawn.pos.y + drawn.galley.mesh_bounds.top(), drawn.pos.y + baseline));
                    }
                }
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        walk(shape, text, out);
                    }
                }
                _ => {}
            }
        }

        let mut out = None;
        for clipped in shapes {
            walk(&clipped.shape, text, &mut out);
        }
        out
    }

    /// The tallest rect painted with `fill`, which for a caption is its band.
    fn band_of(shapes: &[egui::epaint::ClippedShape], fill: egui::Color32) -> Option<egui::Rect> {
        fn walk(shape: &egui::Shape, fill: egui::Color32, out: &mut Option<egui::Rect>) {
            match shape {
                egui::Shape::Rect(painted) if painted.fill == fill => {
                    let taller = out.is_none_or(|rect: egui::Rect| painted.rect.height() > rect.height());
                    if taller {
                        *out = Some(painted.rect);
                    }
                }
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        walk(shape, fill, out);
                    }
                }
                _ => {}
            }
        }

        let mut out = None;
        for clipped in shapes {
            walk(&clipped.shape, fill, &mut out);
        }
        out
    }

    /// The rect of one drawn text, or `None` when the frame does not draw it.
    fn text_rect(shapes: &[egui::epaint::ClippedShape], text: &str) -> Option<egui::Rect> {
        fn walk(shape: &egui::Shape, text: &str, out: &mut Option<egui::Rect>) {
            match shape {
                egui::Shape::Text(drawn) if drawn.galley.text() == text => {
                    *out = Some(egui::Rect::from_min_size(drawn.pos, drawn.galley.size()));
                }
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        walk(shape, text, out);
                    }
                }
                _ => {}
            }
        }

        let mut out = None;
        for clipped in shapes {
            walk(&clipped.shape, text, &mut out);
        }
        out
    }

    /// One frame of a window with a single open list under a cell at `anchor_x`.
    fn list_frame(
        ctx: &egui::Context,
        screen: egui::Rect,
        anchor_x: f32,
        slot: &mut Option<list::VariantList>,
        events: Vec<egui::Event>,
    ) -> Option<egui::Response> {
        let (anchor, output) = list_paint(ctx, screen, anchor_x, slot, events);
        output.drop_without_applying_deltas();
        anchor
    }

    /// One frame of the open circle list under a cell at `anchor_x`, with its paint.
    ///
    /// The pin, the accent bar and the columns are shapes rather than state, so a test reads them off
    /// the frame instead of asking the model to repeat what it drew.
    fn list_paint(
        ctx: &egui::Context,
        screen: egui::Rect,
        anchor_x: f32,
        slot: &mut Option<list::VariantList>,
        events: Vec<egui::Event>,
    ) -> (Option<egui::Response>, egui::FullOutput) {
        let circle = tool("sketch.create.circle");
        let mut anchor = None;
        let input = egui::RawInput { screen_rect: Some(screen), events, ..Default::default() };
        let output = ctx.run_ui(input, |ui| {
            egui::Area::new(egui::Id::unique("ribbon.list.test")).fixed_pos(egui::pos2(anchor_x, 32.0)).show(
                ui.ctx(),
                |ui| {
                    let (_, response) =
                        ui.allocate_exact_size(egui::vec2(RIBBON_CELL_WIDTH, RIBBON_ROW_HEIGHT), egui::Sense::click());
                    let rows = method_rows(circle, &Toolbars::new(), full_context(), Lang::En);
                    list::show(ui, slot, circle, &rows, &response, 0, Lang::En);
                    anchor = Some(response);
                },
            );
        });
        (anchor, output)
    }

    /// The rectangle the open circle list reports over three frames, once the popup has settled.
    fn list_rect(ctx: &egui::Context, screen: egui::Rect, slot: &mut Option<list::VariantList>) -> Option<egui::Rect> {
        let circle = tool("sketch.create.circle");
        let mut shown = None;
        for _ in 0..3 {
            let input = egui::RawInput { screen_rect: Some(screen), ..Default::default() };
            let output = ctx.run_ui(input, |ui| {
                egui::Area::new(egui::Id::unique("ribbon.list.rect.test")).fixed_pos(egui::pos2(8.0, 32.0)).show(
                    ui.ctx(),
                    |ui| {
                        let (_, response) = ui.allocate_exact_size(
                            egui::vec2(RIBBON_CELL_WIDTH, RIBBON_ROW_HEIGHT),
                            egui::Sense::click(),
                        );
                        let rows = method_rows(circle, &Toolbars::new(), full_context(), Lang::En);
                        shown = list::show(ui, slot, circle, &rows, &response, 0, Lang::En).rect;
                    },
                );
            });
            output.drop_without_applying_deltas();
        }
        shown
    }

    /// One key press, as the platform hands it to `egui`.
    fn key_event(key: egui::Key) -> egui::Event {
        egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE }
    }

    /// One pointer move, as the platform hands it to `egui`.
    fn pointer_moved(pos: egui::Pos2) -> egui::Event {
        egui::Event::PointerMoved(pos)
    }

    /// The rect a tool cell takes when a test draws it on its own: a full cell and the caret column a
    /// split cell adds beside it, so a cell of either kind fits where the row would put it.
    fn cell_rect() -> egui::Rect {
        egui::Rect::from_min_size(
            egui::pos2(16.0, 16.0),
            egui::vec2(RIBBON_CELL_WIDTH + icons::RIBBON_CARET_WIDTH, RIBBON_ROW_HEIGHT),
        )
    }

    /// The middle of a split cell's main face, which is the middle of a plain cell.
    fn main_point() -> egui::Pos2 {
        let rect = cell_rect();
        egui::pos2(rect.left() + RIBBON_CELL_WIDTH / 2.0, rect.center().y)
    }

    /// The middle of a split cell's caret face, which begins where the main face ends.
    fn caret_point() -> egui::Pos2 {
        let rect = cell_rect();
        egui::pos2(rect.left() + RIBBON_CELL_WIDTH + icons::RIBBON_CARET_WIDTH / 2.0, rect.center().y)
    }

    /// Draws one tool cell at [`cell_rect`], clicks `pos`, and reports what it ran and what it said.
    ///
    /// The cell is drawn alone, so a test aims at a face by its own metric instead of counting the
    /// cells before it on the tab.
    fn click_cell(state: &mut Toolbars, tool: &'static ToolSpec, pos: egui::Pos2) -> (Vec<ToolAction>, Vec<String>) {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        crate::set_language(&ctx, Lang::En);
        let mut actions = Vec::new();
        let mut labels = Vec::new();
        for pressed in [None, Some(true), Some(false)] {
            let events = match pressed {
                Some(pressed) => vec![pointer_button(pos, pressed)],
                None => vec![pointer_moved(pos)],
            };
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 300.0))),
                events,
                ..Default::default()
            };
            let output = ctx.run_ui(input, |ui| {
                ui.scope_builder(egui::UiBuilder::new().max_rect(cell_rect()), |ui| {
                    show_tool(ui, tool, state, full_context(), &mut actions, Lang::En);
                });
            });
            labels = accessible_labels(&output);
            output.drop_without_applying_deltas();
        }
        (actions, labels)
    }

    /// The drawn text of one frame with the place of its galley, so a test can see a label move.
    fn text_places(shapes: &[egui::epaint::ClippedShape]) -> Vec<(String, egui::Pos2)> {
        fn walk(shape: &egui::Shape, out: &mut Vec<(String, egui::Pos2)>) {
            match shape {
                egui::Shape::Text(text) => out.push((text.galley.text().to_owned(), text.pos)),
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        walk(shape, out);
                    }
                }
                _ => {}
            }
        }

        let mut out = Vec::new();
        for clipped in shapes {
            walk(&clipped.shape, &mut out);
        }
        out
    }

    /// The place of one drawn text in a frame, or a failure that names what was drawn instead.
    fn text_place(shapes: &[egui::epaint::ClippedShape], text: &str) -> egui::Pos2 {
        text_places(shapes)
            .into_iter()
            .find(|(drawn, _)| drawn == text)
            .map(|(_, pos)| pos)
            .unwrap_or_else(|| panic!("the frame does not draw {text:?}"))
    }

    /// One mouse-button change at `pos`.
    fn pointer_button(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// Counts the painted rects filled with `fill`, the selection fill of a pressed button.
    fn drawn_fill(shapes: &[egui::epaint::ClippedShape], fill: egui::Color32) -> usize {
        fn walk(shape: &egui::Shape, fill: egui::Color32, out: &mut usize) {
            match shape {
                egui::Shape::Rect(rect) if rect.fill == fill => *out += 1,
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        walk(shape, fill, out);
                    }
                }
                _ => {}
            }
        }

        let mut out = 0;
        for clipped in shapes {
            walk(&clipped.shape, fill, &mut out);
        }
        out
    }

    /// Counts painted images, the icon of a tool cell.
    fn count_images(shapes: &[egui::epaint::ClippedShape]) -> usize {
        fn walk(shape: &egui::Shape, out: &mut usize) {
            match shape {
                egui::Shape::Mesh(_) => *out += 1,
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        walk(shape, out);
                    }
                }
                _ => {}
            }
        }

        let mut out = 0;
        for clipped in shapes {
            walk(&clipped.shape, &mut out);
        }
        out
    }

    /// Collects the height of every painted vertical line segment, a divider or a `vline`.
    fn collect_vertical_lines(shapes: &[egui::epaint::ClippedShape], out: &mut Vec<f32>) {
        fn walk(shape: &egui::Shape, out: &mut Vec<f32>) {
            match shape {
                egui::Shape::LineSegment { points, .. } => {
                    if (points[0].x - points[1].x).abs() < 0.5 {
                        out.push((points[0].y - points[1].y).abs());
                    }
                }
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        walk(shape, out);
                    }
                }
                _ => {}
            }
        }
        for clipped in shapes {
            walk(&clipped.shape, out);
        }
    }

    /// The accessible roles this frame published, so a test can see a text field among the cells.
    fn accessible_roles(output: &egui::FullOutput) -> Vec<egui::Role> {
        output
            .platform_output
            .accesskit_update
            .as_ref()
            .map(|update| update.nodes.iter().map(|(_, node)| node.role()).collect())
            .unwrap_or_default()
    }

    /// The accessible labels this frame published: a drawn icon carries its hint here, not as text.
    fn accessible_labels(output: &egui::FullOutput) -> Vec<String> {
        output
            .platform_output
            .accesskit_update
            .as_ref()
            .map(|update| update.nodes.iter().filter_map(|(_, node)| node.label().map(str::to_owned)).collect())
            .unwrap_or_default()
    }

    /// Collects the text of every text shape, including nested ones.
    fn drawn_text(shapes: &[egui::epaint::ClippedShape]) -> Vec<String> {
        fn walk(shape: &egui::Shape, out: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(text) => out.push(text.galley.text().to_owned()),
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        walk(shape, out);
                    }
                }
                _ => {}
            }
        }

        let mut out = Vec::new();
        for clipped in shapes {
            walk(&clipped.shape, &mut out);
        }
        out
    }

    #[test]
    fn switching_the_language_redraws_the_ribbon() {
        let mut state = Toolbars::new();
        let (english, _, _) = draw_in(&mut state, full_context(), Lang::En);
        let (russian, _, _) = draw_in(&mut state, full_context(), Lang::Ru);
        assert!(!english.iter().any(|text| text == "СОЗДАТЬ"));
        assert!(russian.iter().any(|text| text == "СОЗДАТЬ"), "the Russian group caption is drawn: {russian:?}");
        assert_ne!(english, russian, "the same ribbon draws different words");

        let mut state = Toolbars::new();
        let (_, english, _) = draw_in(&mut state, full_context(), Lang::En);
        let mut state = Toolbars::new();
        let (_, russian, _) = draw_in(&mut state, full_context(), Lang::Ru);
        assert!(english.iter().any(|hint| hint.contains("Two points  (L)")));
        assert!(russian.iter().any(|hint| hint.contains("По двум точкам  (L)")), "the hint is translated: {russian:?}");
    }
}
