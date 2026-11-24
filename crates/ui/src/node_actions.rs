//! The actions of the node under the pointer, in one list every context menu draws.
//!
//! The tree used to build its row menu inside itself, and the viewport built nothing: the actions a
//! piece has were two or three clicks away in a 600-line file, and a second menu would have been a
//! second list that disagrees with the first. This module is that list. A [`Subject`] is a node, the
//! root frame or empty space; [`entries`] answers with what that subject owns — the inventory's
//! contextual rows that act on its kind (ADR 0117), in the registry's order, plus the session
//! entries that write no document: the eye, the padlock, *Isolate*, *Show all*, the four transform
//! gestures and *Add child* and the open product (ADR 0065, ADR 0098, U113).
//!
//! A capability entry names its id, its label, whether it can run and the sentence that says why not,
//! so a menu cannot offer something the engine would refuse silently. The tree row and the viewport
//! popup draw the same list through [`show`]: an entry cannot exist in one and not the other.
//!
//! Changes when: a node kind gains an action, a command moves in or out of the context menu, or the
//! popup's anchors and dismissal change.

use egui::{Align2, CornerRadius, FontId, Margin, Order, Rect, Sense, Stroke, StrokeKind, Vec2};

use model::{ClassCode, Container, FillMode, Id, Project};
use text::{Lang, Message};

use crate::editor::{FILL_CLASSES, FillSpread, PanelBuild};
use crate::inventory::{self, NodeKind, Place};
use crate::transform::TransformMode;
use crate::view_flags::ViewFlags;
use crate::{icons, language, theme};

/// The width of the popup, in points: the tick column, the longest command label and a gutter on
/// the right, so a long word never reads as cropped by the frame.
const MENU_WIDTH: f32 = 178.0;

/// The height of one row, in points: a control the pointer finds without aiming.
const ROW_HEIGHT: f32 = 24.0;

/// The air between the popup's edge and the first column of its rows, in points.
const ROW_INSET: f32 = 10.0;

/// The width of the tick column, in points.
///
/// The column is reserved on every row, marked or not, so the labels of a section share one left
/// edge whether their row carries a tick, a box or nothing (U128).
const TICK_COLUMN: f32 = 18.0;

/// The air between the tick column and the label, in points.
const LABEL_GAP: f32 = 4.0;

/// The air kept on the right of every label, in points.
const LABEL_GUTTER: f32 = 14.0;

/// The side of the box a switch row draws, in points.
const SWITCH_BOX: f32 = 12.0;

/// The point size of a row's label, in points.
const LABEL_POINT_SIZE: f32 = 13.0;

/// The point size of a row's tick, in points.
const TICK_POINT_SIZE: f32 = 14.0;

/// The height of the hairline between two sections, in points.
const SEPARATOR_HEIGHT: f32 = 9.0;

/// What a context menu is about.
///
/// The root is its own subject rather than a node: the world frame owns no edit, so it offers the
/// view entries alone, and the tree is the only surface that can name it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Subject {
    /// The node under the pointer or the tree row.
    Node(Id),
    /// The root frame.
    Root,
    /// Empty space: no node under the pointer.
    Empty,
}

/// Something a context menu entry runs.
///
/// The command entries carry the node they act on instead of reading the selection, because the
/// menu acts on what the pointer names: a right click does not have to select first. The document
/// edits are dispatched as commands by the editor; the view entries are session state and leave no
/// undo entry (ADR 0065, ADR 0098).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeAction {
    /// Arms the container for the next creation; a view-state decision, not a second creation path.
    AddChild {
        parent: Id,
    },
    /// Makes this product the session's one open working product.
    OpenProduct {
        node: Id,
    },
    /// Opens this product and arms the niche tool, so the next pointer names the empty space inside
    /// it (ADR 0118).
    ///
    /// The arm is a read: it writes no field, no revision and no undo entry.
    ArmVoid {
        product: Id,
    },
    /// Leaves the open product, so no product receives new parts.
    CloseProduct,
    /// Rewrites a panel's build: how the part reads the section of the void it fills (U123).
    ///
    /// The class does not travel with it: a created part keeps the role it was born with.
    PanelBuild {
        /// The panel whose build changes.
        panel: model::Id,
        /// The build the part takes; the row offers the one it does not have.
        mode: model::FillMode,
    },
    Duplicate {
        node: Id,
    },
    Delete {
        node: Id,
    },
    Group {
        nodes: Vec<Id>,
    },
    Ungroup {
        node: Id,
    },
    ConvertToGroup {
        node: Id,
    },
    SetHidden {
        node: Id,
        hidden: bool,
    },
    SetLocked {
        node: Id,
        locked: bool,
    },
    Isolate {
        node: Id,
    },
    ShowAll,
    /// Sets one option of the fill the window has armed (U128).
    ///
    /// The choice is the operation's, not the document's: it lives in the editor's build, so the
    /// entry writes no field, no revision and no undo entry.
    Fill(FillPick),
    /// Arms one of the window's four transform gestures.
    ///
    /// The mode is the window's view state and not the document's, so the entry names it and the
    /// shell writes it: no command, no revision and no undo entry (ADR 0113, U113).
    Mode(TransformMode),
}

impl NodeAction {
    /// Whether the action removes something a person would have to rebuild by hand.
    ///
    /// A menu draws a destructive entry last: the row under the pointer is the one the hand is
    /// already on, so it is never the row that deletes.
    pub fn is_destructive(&self) -> bool {
        matches!(self, NodeAction::Delete { .. })
    }
}

/// The mark a row carries in the tick column, left of its label.
///
/// A mark is what the row's state looks like, never a second control: the whole row is the press
/// target, so a tick in the margin is a reading and not a checkbox to aim at.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mark {
    /// An ordinary entry: the column stays empty.
    #[default]
    None,
    /// One of a set of alternatives: a tick when this one is chosen, nothing when it is not.
    Chosen(bool),
    /// A switch: a box, ticked while it is on.
    Switch(bool),
}

impl Mark {
    /// The tick a switch carries when it is on, or the alternative that is currently chosen.
    pub const fn is_on(self) -> bool {
        matches!(self, Self::Chosen(true) | Self::Switch(true))
    }

    /// Whether the row draws a box in its tick column: a switch does, an alternative does not.
    pub const fn is_switch(self) -> bool {
        matches!(self, Self::Switch(_))
    }
}

/// The one option of a fill's menu that was picked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FillPick {
    /// The class the next click creates: a post, a shelf or a wall.
    Class(ClassCode),
    /// How the part reads the section the walls leave.
    Mode(FillMode),
    /// Whether the board sits inside the opening or is laid over the frame's face.
    Mount(bool),
    /// Whether the gesture lays one board or a row of them.
    Spread(FillSpread),
}

/// One row of a menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The capability's stable id, or `None` for a session entry that dispatches no command.
    pub id: Option<&'static str>,
    /// The words the entry prints: the command's own label, or the session entry's.
    pub label: Message,
    /// What choosing the entry does.
    pub action: NodeAction,
    /// Whether the node allows it now. A refused entry stays visible and says why.
    pub enabled: bool,
    /// The hover sentence: why the entry cannot run, or what it does when it can.
    pub reason: Option<Message>,
    /// Whether a separator line is drawn above the entry. A leading separator is never drawn.
    pub section: bool,
    /// The mark drawn in the tick column, left of the label (U128).
    pub mark: Mark,
}

/// What a menu is built from: the document, the session's view flags and its selection.
#[derive(Clone, Copy, Debug)]
pub struct MenuSource<'a> {
    /// The document the subject lives in.
    pub project: &'a Project,
    /// The session's eye and padlock, which the view entries toggle and the commands respect.
    pub flags: &'a ViewFlags,
    /// The selected nodes, oldest first: grouping needs two or more that may be grouped.
    pub selection: &'a [Id],
    /// The product the session works in, so its open/close entry reads the right way.
    pub active_product: Option<Id>,
    /// The build of the fill the window has armed, when one is.
    ///
    /// A tool in hand is what the next click is about, so a viewport popup opened while it runs is
    /// the operation's own menu and not the node's: the four choices stand where the node's rows
    /// would, and the node's rows stay in the tree (U128). The tree hands `None`, because its menu
    /// acts on a node whatever tool is armed.
    pub fill: Option<PanelBuild>,
}

/// The entries `subject` owns, in the order a menu draws them.
///
/// The capability entries are the inventory filtered by the node's kind, so a new command with a
/// context-menu place appears in both surfaces without either file changing. The session entries
/// are declared here because they dispatch no command: hiding a node is view state, not an edit.
pub fn entries(subject: Subject, source: &MenuSource<'_>) -> Vec<Entry> {
    if let Some(build) = source.fill {
        return fill_entries(build);
    }
    let project = source.project;
    let kind = kind_of(project, subject);
    let node = match subject {
        Subject::Node(id) => Some(id),
        Subject::Root | Subject::Empty => None,
    };
    let many = source.selection.len() >= 2;
    let hidden = node.is_some_and(|id| source.flags.is_hidden(id));
    let locked = node.is_some_and(|id| source.flags.is_locked(id));
    let container = container_of(project, subject);
    let mut entries = Vec::new();

    // The eye and the padlock, in the order the row draws them. The root is a frame, so hiding or
    // locking it would hide or freeze the whole scene and it offers neither.
    if let Some(id) = node {
        entries.push(Entry {
            id: None,
            label: if hidden { Message::TreeShow } else { Message::TreeHide },
            action: NodeAction::SetHidden { node: id, hidden: !hidden },
            enabled: true,
            reason: None,
            section: false,
            mark: Mark::None,
        });
        entries.push(Entry {
            id: None,
            label: if locked { Message::TreeUnlock } else { Message::TreeLock },
            action: NodeAction::SetLocked { node: id, locked: !locked },
            enabled: true,
            reason: None,
            section: false,
            mark: Mark::None,
        });
        // A panel's build is the one property its own menu rewrites, and its words name the build the
        // board does not have, so they come from the table's `live_words` (U123). A part whose class
        // fills nothing has no void to re-read and offers nothing.
        if let Some(id) = node
            && project.panel(id).is_some()
            && let Some(label) = inventory::live_words("panel.build.set", project, id)
            && project.class(id).and_then(|class| class.traits()).and_then(|traits| traits.fill()).is_some()
        {
            let mode = match label {
                Message::FillMethodShape => model::FillMode::Shape,
                _ => model::FillMode::Fit,
            };
            entries.push(Entry {
                id: Some("panel.build.set"),
                label,
                action: NodeAction::PanelBuild { panel: id, mode },
                enabled: true,
                reason: None,
                section: true,
                mark: Mark::None,
            });
        }
        // Isolate needs a subject; show all is how a person finds a model that disappeared, so it
        // is the one entry empty space owns.
        entries.push(Entry {
            id: None,
            label: Message::TreeIsolate,
            action: NodeAction::Isolate { node: id },
            enabled: true,
            reason: Some(Message::TreeIsolateHint),
            section: true,
            mark: Mark::None,
        });
    }
    entries.push(Entry {
        id: None,
        label: Message::TreeShowAll,
        action: NodeAction::ShowAll,
        enabled: true,
        reason: Some(Message::TreeShowAllHint),
        section: false,
        mark: Mark::None,
    });

    // The window's four gestures, beside the view entries because they write no document either:
    // the mode is view state, so a node's row menu is where a maker reaches it without leaving the
    // piece in hand (U113). Empty space and the root draw no entry of their own here: a mode acts on
    // a selection, and the header's band is always one press away (U115).
    if node.is_some() && std::env::var("Z3L_NO_MODES").is_err() {
        for (index, mode) in TransformMode::ALL.into_iter().enumerate() {
            entries.push(Entry {
                id: None,
                label: mode.words(),
                action: NodeAction::Mode(mode),
                enabled: true,
                reason: Some(mode.hint_words()),
                section: index == 0,
                mark: Mark::None,
            });
        }
    }

    // A container is where a part lands and, when it is a product, the piece the session works in.
    if let Some(id) = node
        && let Some(container) = container
    {
        entries.push(Entry {
            id: None,
            label: Message::TreeAddChild,
            action: NodeAction::AddChild { parent: id },
            enabled: true,
            reason: Some(Message::TreeAddChildHint),
            section: true,
            mark: Mark::None,
        });
        if container.is_product() {
            let active = source.active_product == Some(id);
            entries.push(Entry {
                id: None,
                label: if active { Message::TreeCloseProduct } else { Message::TreeOpenProduct },
                action: if active { NodeAction::CloseProduct } else { NodeAction::OpenProduct { node: id } },
                enabled: true,
                reason: None,
                section: false,
                mark: Mark::None,
            });
        }
    }

    // The capability entries, in the inventory's order so the table and the menu can be read side
    // by side. A row this module cannot run — the one that does not fit the node's own flag, or a
    // group the selection cannot form — is absent rather than refused, because it has no meaning
    // here. A locked node keeps every edit visible and refused, with the sentence that says so.
    let mut section = !entries.is_empty();
    let mut destructive = Vec::new();
    if let Some(id) = node {
        for row in inventory::in_place(Place::ContextMenu) {
            if !row.offers(kind, many) {
                continue;
            }
            let Some(action) = action_of(row.id, id, container, source) else {
                continue;
            };
            let refused = locked || holds_a_locked_member(row.id, source);
            let entry = Entry {
                id: Some(row.id),
                label: row.label,
                action,
                enabled: !refused,
                reason: refused.then_some(Message::MenuLocked),
                section: std::mem::replace(&mut section, false),
                mark: Mark::None,
            };
            if entry.action.is_destructive() {
                destructive.push(entry);
            } else {
                entries.push(entry);
            }
        }
    }
    for mut entry in destructive {
        entry.section = true;
        entries.push(entry);
    }
    entries
}

/// The fill's own menu: the class, the build, the spread and, for a wall, the mounting.
///
/// Every row carries a mark in the tick column: an alternative draws a tick on the one in force, and
/// the spread is a switch, so its box is drawn on its row alone. The order is the order a part is
/// thought about — what it is, how it reads the section, whether there is more than one of it, and
/// how it sits in the frame — and the mounting is absent where it means nothing (ADR 0240, U128).
fn fill_entries(build: PanelBuild) -> Vec<Entry> {
    let mut entries = Vec::new();
    // The menu reads as the tool reads: a post, a shelf, a wall. The class registry's own words
    // ("upright") belong to the tree, where a class is a part's semantics and not the next click
    // (U128).
    let words = [
        (ClassCode::VERTICAL_PANEL, Message::ToolPost),
        (ClassCode::HORIZONTAL_PANEL, Message::ToolShelf),
        (ClassCode::BACK_PANEL, Message::ToolWall),
    ];
    for (class, label) in words {
        entries.push(Entry {
            id: None,
            label,
            action: NodeAction::Fill(FillPick::Class(class)),
            enabled: true,
            reason: None,
            section: false,
            mark: Mark::Chosen(build.class == class),
        });
    }
    debug_assert_eq!(words.map(|(class, _)| class), FILL_CLASSES, "the menu and the arm offer one set of classes");
    for (mode, label) in [(FillMode::Fit, Message::FillMethodFit), (FillMode::Shape, Message::FillMethodShape)] {
        entries.push(Entry {
            id: None,
            label,
            action: NodeAction::Fill(FillPick::Mode(mode)),
            enabled: true,
            reason: None,
            section: mode == FillMode::Fit,
            mark: Mark::Chosen(build.mode == mode),
        });
    }
    entries.push(Entry {
        id: None,
        label: Message::FillSpreadEven,
        action: NodeAction::Fill(FillPick::Spread(build.spread.opposite())),
        enabled: true,
        reason: None,
        section: true,
        mark: Mark::Switch(build.spread.is_even()),
    });
    // A board laid over the frame is a frontal board: a shelf or a post stands across the void, so
    // the mounting is asked only where the answer means something (ADR 0240).
    if build.class.traits().and_then(|traits| traits.fill()) == Some(model::Fill::WidthAndHeight) {
        for (overlay, label) in [(false, Message::FillMountInset), (true, Message::FillMountOverlay)] {
            entries.push(Entry {
                id: None,
                label,
                action: NodeAction::Fill(FillPick::Mount(overlay)),
                enabled: true,
                reason: None,
                section: !overlay,
                mark: Mark::Chosen(build.overlay == overlay),
            });
        }
    }
    entries
}

/// Whether a batch capability names a member of the selection that is locked.
fn holds_a_locked_member(id: &str, source: &MenuSource<'_>) -> bool {
    id == "node.group" && source.selection.iter().any(|node| source.flags.is_locked(*node))
}

/// The container `subject` names, when it is one.
fn container_of(project: &Project, subject: Subject) -> Option<&Container> {
    match subject {
        Subject::Node(id) => project.container(id),
        Subject::Root | Subject::Empty => None,
    }
}

/// The action a capability row runs, when its node allows it at all.
///
/// The two container rows are alternatives: a group flattens into its parent, a component has to
/// become a group first, so each is offered only when the node's own flag says so. A row this
/// module cannot run is skipped rather than drawn disabled, because the table says what a menu
/// draws and a row that has no action here would be a missing entry, not a refused one.
fn action_of(id: &str, node: Id, container: Option<&Container>, source: &MenuSource<'_>) -> Option<NodeAction> {
    Some(match id {
        // The niche is a read inside the open product, so the entry only appears on a product the
        // menu can open; every other container is skipped rather than refused (ADR 0098, 0118).
        "probe.void" if container.is_some_and(Container::is_product) => NodeAction::ArmVoid { product: node },
        "node.duplicate" => NodeAction::Duplicate { node },
        "node.delete" => NodeAction::Delete { node },
        "node.group" if source.selection.len() >= 2 && source.project.validate_group(source.selection).is_ok() => {
            NodeAction::Group { nodes: source.selection.to_vec() }
        }
        "node.ungroup" if container.is_some_and(|container| !container.is_component()) => NodeAction::Ungroup { node },
        "node.component.convert_to_group" if container.is_some_and(Container::is_component) => {
            NodeAction::ConvertToGroup { node }
        }
        _ => return None,
    })
}

/// The kind of node `subject` names.
fn kind_of(project: &Project, subject: Subject) -> NodeKind {
    let Subject::Node(id) = subject else {
        return if subject == Subject::Root { NodeKind::Root } else { NodeKind::Empty };
    };
    if project.panel(id).is_some() {
        NodeKind::Panel
    } else if project.object(id).is_some() {
        NodeKind::Object
    } else if project.is_container(id) {
        NodeKind::Container
    } else if project.camera(id).is_some() {
        NodeKind::Camera
    } else if project.light(id).is_some() {
        NodeKind::Light
    } else {
        NodeKind::Empty
    }
}

/// Draws `entries` into the current `Ui`, returning the action the person chose this frame.
///
/// The drawing is shared by the tree row's menu and the viewport popup, so the two cannot drift in
/// wording, order or enablement. Every row is drawn here rather than handed to a button: the tick
/// column has to stand to the left of the label, on one left edge for the whole section, and a
/// button centres its text (U128). A refused entry keeps its place, drawn in the dim ink with its
/// reason as the hover text; the caller closes its own surface.
pub fn show(ui: &mut egui::Ui, entries: &[Entry]) -> Option<NodeAction> {
    let lang = language(ui.ctx());
    let mut chosen = None;
    for (index, entry) in entries.iter().enumerate() {
        if entry.section && index > 0 {
            separator(ui);
        }
        // A refused row takes no press: the pointer finds it, reads its reason and runs nothing.
        let sense = if entry.enabled { Sense::click() } else { Sense::hover() };
        let (rect, response) = ui.allocate_exact_size(Vec2::new(MENU_WIDTH, ROW_HEIGHT), sense);
        if ui.is_rect_visible(rect) {
            if entry.enabled && ui.rect_contains_pointer(rect) {
                ui.painter().rect_filled(rect, CornerRadius::same(3), theme::control_hover(ui));
            }
            row(ui, rect, entry, lang);
        }
        let response = match entry.reason {
            Some(reason) => response.on_hover_text(reason.text(lang)),
            None => response,
        };
        if response.clicked() {
            chosen = Some(entry.action.clone());
        }
    }
    chosen
}

/// Draws the hairline that opens a section, inset from the popup's edge so it reads as a rule and
/// not as a second frame.
fn separator(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(MENU_WIDTH, SEPARATOR_HEIGHT), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let run = egui::Rangef::new(rect.left() + ROW_INSET, rect.right() - LABEL_GUTTER);
    ui.painter().hline(run, rect.center().y, Stroke::new(1.0, theme::line(ui)));
}

/// Draws one row: its mark in the tick column and its label, left-aligned on the shared edge.
fn row(ui: &egui::Ui, rect: Rect, entry: &Entry, lang: Lang) {
    let painter = ui.painter();
    let ink = if entry.enabled { theme::text(ui) } else { theme::text_dim(ui) };
    let column = Rect::from_min_size(
        egui::pos2(rect.left() + ROW_INSET, rect.center().y - TICK_COLUMN * 0.5),
        Vec2::new(TICK_COLUMN, TICK_COLUMN),
    );
    // A switch draws its box on every row of the set, so the column says what pressing does: one of
    // these becomes true, or this one flips. An alternative leaves the column empty until chosen.
    if entry.mark.is_switch() {
        let box_rect = Rect::from_center_size(column.center(), Vec2::splat(SWITCH_BOX));
        let stroke = Stroke::new(1.2, if entry.mark.is_on() { theme::accent(ui) } else { theme::line(ui) });
        painter.rect_stroke(box_rect, CornerRadius::same(2), stroke, StrokeKind::Inside);
    }
    if entry.mark.is_on() {
        let tick_rect = Rect::from_center_size(column.center(), Vec2::splat(TICK_POINT_SIZE));
        icons::draw_icon(ui, icons::actions::CONFIRM, tick_rect, TICK_POINT_SIZE, theme::accent(ui));
    }
    let text_x = column.right() + LABEL_GAP;
    painter.text(
        egui::pos2(text_x, rect.center().y),
        Align2::LEFT_CENTER,
        entry.label.text(lang),
        FontId::proportional(LABEL_POINT_SIZE),
        ink,
    );
}

/// The context menu the viewport draws at the pointer.
///
/// It holds the subject and the anchor, so a frame that rebuilds the scene keeps the menu on the
/// node it names. The rectangle is the last drawn one, which the shell hands to [`Chrome::covers`]:
/// a press on the menu is chrome and never a pick, a border drag or a camera gesture (ADR 0080).
/// Escape closes it, and so does a press outside it — the shell reads the rectangle and keeps that
/// press from picking, so dismissing a menu never edits the scene underneath.
///
/// [`Chrome::covers`]: crate::theme
#[derive(Clone, Debug, Default)]
pub struct ViewportMenu {
    open: Option<Open>,
    rect: Option<Rect>,
}

/// Where the open menu is and what it is about.
#[derive(Clone, Copy, Debug)]
struct Open {
    subject: Subject,
    position: egui::Pos2,
}

impl ViewportMenu {
    /// Opens the menu for `subject` at `position`, in points.
    pub fn open(&mut self, subject: Subject, position: egui::Pos2) {
        self.open = Some(Open { subject, position });
    }

    /// Closes the menu; a second call does nothing.
    pub fn close(&mut self) {
        self.open = None;
        self.rect = None;
    }

    /// Whether a menu is up.
    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// What the open menu is about.
    pub fn subject(&self) -> Option<Subject> {
        self.open.map(|open| open.subject)
    }

    /// The rectangle the open menu drew last frame, which is the one on screen while it stays up.
    pub fn rect(&self) -> Option<Rect> {
        self.rect
    }

    /// Draws the open menu over `viewport` and returns the entry the person chose this frame.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        viewport: Rect,
        source: &MenuSource<'_>,
        opacity: u32,
    ) -> Option<NodeAction> {
        let Some(open) = self.open else {
            self.rect = None;
            return None;
        };
        if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.close();
            return None;
        }
        let entries = entries(open.subject, source);
        let fill = theme::at_opacity(theme::floating(ui), opacity.clamp(80, 100));
        let mut chosen = None;
        let response = egui::Area::new(egui::Id::unique("node_menu"))
            .order(Order::Middle)
            .fixed_pos(open.position)
            .constrain_to(viewport)
            .show(ui.ctx(), |ui| {
                egui::Frame::new()
                    .fill(fill)
                    .stroke(Stroke::new(1.0, theme::line(ui)))
                    .corner_radius(CornerRadius::same(theme::RADIUS))
                    .inner_margin(Margin::symmetric(6, 4))
                    .show(ui, |ui| {
                        ui.set_min_width(MENU_WIDTH);
                        chosen = show(ui, &entries);
                    });
            })
            .response;
        // A press inside the menu is the menu's own, and a press outside it dismisses it. The frame's
        // rectangle decides, not `Response::contains_pointer`: a popup the window had to pull back
        // inside the scene keeps the hit test of the place it would have had, so a menu taller than
        // the space under the pointer would dismiss itself on a press on one of its own rows (U113
        // measured this with ten entries). The same rectangle is what the shell hands to
        // `Chrome::covers`, so a press that keeps the menu up is also kept from the scene behind it.
        let pressed = ui.input(|input| input.pointer.any_pressed());
        let inside = ui.input(|input| input.pointer.interact_pos()).is_some_and(|pos| response.rect.contains(pos));
        let dismissed = pressed && !inside;
        if chosen.is_some() || dismissed {
            self.close();
        } else {
            self.rect = Some(response.rect);
        }
        chosen
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use model::testing::{sample_product_project, sample_project};
    use model::{CameraProjection, Placement};

    use super::*;

    fn id(value: u64) -> Id {
        Id::new(value).expect("non-zero id")
    }

    /// The menu of one subject in the sample project.
    fn menu(project: &Project, flags: &ViewFlags, selection: &[Id], subject: Subject) -> Vec<Entry> {
        let source = MenuSource { project, flags, selection, active_product: None, fill: None };
        entries(subject, &source)
    }

    fn labels(entries: &[Entry]) -> Vec<Message> {
        entries.iter().map(|entry| entry.label).collect()
    }

    /// Collects the text of every text shape and the rectangle it was drawn in, nested ones too.
    fn drawn_rects(shapes: &[egui::epaint::ClippedShape]) -> Vec<(Rect, String)> {
        use egui::epaint::Shape;
        fn walk(shape: &Shape, out: &mut Vec<(Rect, String)>) {
            match shape {
                Shape::Text(text) => {
                    out.push((Rect::from_min_size(text.pos, text.galley.size()), text.galley.text().to_owned()));
                }
                Shape::Vec(shapes) => {
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

    /// The popup a right click opens while the fill is armed.
    fn fill_menu(build: PanelBuild) -> Vec<Entry> {
        let project = sample_project();
        let flags = ViewFlags::default();
        let source =
            MenuSource { project: &project, flags: &flags, selection: &[], active_product: None, fill: Some(build) };
        entries(Subject::Empty, &source)
    }

    #[test]
    fn an_armed_fill_opens_the_operations_menu_and_no_node_row() {
        // U128: a tool in hand is what the next click is about, so the popup draws what the click
        // makes and nothing the node owns — the eye, the padlock and the transforms stay in the tree.
        let build = PanelBuild::default();
        let entries = fill_menu(build);
        assert_eq!(
            labels(&entries),
            vec![
                Message::ToolPost,
                Message::ToolShelf,
                Message::ToolWall,
                Message::FillMethodFit,
                Message::FillMethodShape,
                Message::FillSpreadEven,
            ],
            "a post's menu: the three classes, the two builds, the spread"
        );
        assert!(entries.iter().all(|entry| entry.id.is_none()), "no row of the operation names a command");
        assert!(
            entries.iter().all(|entry| entry.section
                == (entry.label == Message::FillMethodFit || entry.label == Message::FillSpreadEven)),
            "the classes open the menu, the build and the spread each open a section"
        );
    }

    #[test]
    fn the_menu_marks_the_choice_in_force_and_switches_the_spread() {
        // The mark is what the row's state looks like: an alternative carries a tick on the one in
        // force, and the spread is a switch, so its box is drawn on its own row alone.
        let build = PanelBuild {
            class: ClassCode::HORIZONTAL_PANEL,
            mode: model::FillMode::Shape,
            overlay: false,
            spread: FillSpread::Even,
        };
        let entries = fill_menu(build);
        let mark = |label: Message| {
            entries.iter().find(|entry| entry.label == label).map(|entry| entry.mark).expect("the row is drawn")
        };
        assert_eq!(mark(Message::ToolShelf), Mark::Chosen(true));
        assert_eq!(mark(Message::ToolPost), Mark::Chosen(false));
        assert_eq!(mark(Message::FillMethodShape), Mark::Chosen(true));
        assert_eq!(mark(Message::FillMethodFit), Mark::Chosen(false));
        assert_eq!(mark(Message::FillSpreadEven), Mark::Switch(true));
        let spread = entries.iter().find(|entry| entry.label == Message::FillSpreadEven).expect("the switch");
        assert_eq!(
            spread.action,
            NodeAction::Fill(FillPick::Spread(FillSpread::One)),
            "a press on a switch that is on turns it off"
        );
    }

    #[test]
    fn a_wall_offers_its_mounting_and_a_shelf_does_not() {
        // ADR 0240: a board laid over the frame is a frontal board, so the mounting is asked only of
        // the class that fills the opening's face.
        let wall = fill_menu(PanelBuild { class: ClassCode::BACK_PANEL, ..PanelBuild::default() });
        assert!(labels(&wall).contains(&Message::FillMountInset), "a wall chooses how it sits");
        assert!(labels(&wall).contains(&Message::FillMountOverlay));
        let shelf = fill_menu(PanelBuild { class: ClassCode::HORIZONTAL_PANEL, ..PanelBuild::default() });
        assert!(!labels(&shelf).contains(&Message::FillMountInset), "a shelf stands across the void");
    }

    #[test]
    fn a_marked_row_keeps_its_label_clear_of_the_tick_column() {
        // The tick stands left of the label on one edge for the whole section, so a row that gains a
        // mark never moves its own words (U128).
        let entries = fill_menu(PanelBuild {
            class: ClassCode::HORIZONTAL_PANEL,
            mode: model::FillMode::Fit,
            overlay: false,
            spread: FillSpread::Even,
        });
        let ctx = egui::Context::default();
        crate::set_language(&ctx, crate::Lang::En);
        let output = ctx.run_ui(Default::default(), |ui| {
            let _ = show(ui, &entries);
        });
        let rects = drawn_rects(&output.shapes);
        output.drop_without_applying_deltas();
        let marked = rects
            .iter()
            .find(|(_, text)| text == Message::ToolShelf.text(crate::Lang::En))
            .map(|(rect, _)| *rect)
            .expect("the chosen class is drawn");
        let unmarked = rects
            .iter()
            .find(|(_, text)| text == Message::ToolPost.text(crate::Lang::En))
            .map(|(rect, _)| *rect)
            .expect("the other class is drawn");
        assert!(
            (marked.left() - unmarked.left()).abs() < 0.5,
            "a mark never moves a label: {} against {}",
            marked.left(),
            unmarked.left()
        );
        assert!(
            (unmarked.left() - (ROW_INSET + TICK_COLUMN + LABEL_GAP)).abs() < 1.0,
            "the label stands after the tick column: {}",
            unmarked.left()
        );
    }

    #[test]
    fn a_panel_offers_the_view_entries_then_its_edits() {
        let project = sample_project();
        let flags = ViewFlags::default();
        let entries = menu(&project, &flags, &[id(2)], Subject::Node(id(2)));
        assert_eq!(
            labels(&entries),
            vec![
                Message::TreeHide,
                Message::TreeLock,
                Message::TreeIsolate,
                Message::TreeShowAll,
                Message::Select,
                Message::Move,
                Message::Object,
                Message::Rotate,
                Message::ObjectRotate,
                Message::CmdDuplicate,
                Message::CmdDelete,
            ],
            "a panel's menu: the eye, the padlock, the view entries and the five gestures, then its edits"
        );
    }

    #[test]
    fn a_component_is_offered_the_conversion_instead_of_ungrouping() {
        let project = sample_project();
        let flags = ViewFlags::default();
        let component = menu(&project, &flags, &[id(3)], Subject::Node(id(3)));
        assert_eq!(
            labels(&component),
            vec![
                Message::TreeHide,
                Message::TreeLock,
                Message::TreeIsolate,
                Message::TreeShowAll,
                Message::Select,
                Message::Move,
                Message::Object,
                Message::Rotate,
                Message::ObjectRotate,
                Message::TreeAddChild,
                Message::CmdDuplicate,
                Message::CmdConvertToGroup,
                Message::CmdDelete,
            ],
            "a component is a container: it takes a child and converts, it does not ungroup"
        );
        assert!(
            !component.iter().any(|entry| entry.action == NodeAction::Ungroup { node: id(3) }),
            "an ungrouped component loses its class, so the menu never offers it"
        );
    }

    #[test]
    fn a_camera_and_a_light_offer_the_same_edits_as_an_object() {
        let mut project = sample_project();
        let root = project.root().id();
        let camera =
            project.add_camera(root, "Camera", Placement::IDENTITY, CameraProjection::DEFAULT).expect("a camera");
        let flags = ViewFlags::default();
        let entries = menu(&project, &flags, &[camera], Subject::Node(camera));
        assert_eq!(
            labels(&entries),
            vec![
                Message::TreeHide,
                Message::TreeLock,
                Message::TreeIsolate,
                Message::TreeShowAll,
                Message::Select,
                Message::Move,
                Message::Object,
                Message::Rotate,
                Message::ObjectRotate,
                Message::CmdDuplicate,
                Message::CmdDelete,
            ],
            "a scene object is a node like any other, so it keeps the same entries"
        );
        assert!(entries.iter().all(|entry| entry.id != Some("panel.rename")));
    }

    #[test]
    fn empty_space_and_the_root_offer_the_document_view_action_only() {
        let project = sample_project();
        let flags = ViewFlags::default();
        for subject in [Subject::Empty, Subject::Root] {
            let entries = menu(&project, &flags, &[], subject);
            assert_eq!(labels(&entries), vec![Message::TreeShowAll], "{subject:?} owns one action");
            assert!(entries.iter().all(|entry| entry.id.is_none()), "and it is no command");
        }
    }

    #[test]
    fn the_five_gestures_are_session_entries_of_a_node_menu() {
        // U113: the mode is the window's view state and writes no document, so the menu offers it
        // with no command behind it — an entry that names the mode, says what it does on hover and
        // is never refused, not even on a locked node, which a mode does not edit.
        let project = sample_project();
        let mut flags = ViewFlags::default();
        flags.set_locked(id(2), true);
        let entries = menu(&project, &flags, &[id(2)], Subject::Node(id(2)));
        let modes: Vec<Entry> =
            entries.iter().filter(|entry| matches!(entry.action, NodeAction::Mode(_))).cloned().collect();
        assert_eq!(
            modes.iter().map(|entry| entry.action.clone()).collect::<Vec<_>>(),
            TransformMode::ALL.into_iter().map(NodeAction::Mode).collect::<Vec<_>>(),
            "the menu offers the five in the band's order"
        );
        for (entry, mode) in modes.iter().zip(TransformMode::ALL) {
            assert!(entry.id.is_none(), "a mode is view state, so it names no command");
            assert_eq!(entry.label, mode.words(), "the menu and the band print one word");
            assert_eq!(entry.reason, Some(mode.hint_words()), "and the hover says which gesture it is");
            assert!(entry.enabled, "a locked node refuses an edit, never a view mode");
        }
    }

    #[test]
    fn the_five_gestures_are_one_section_of_the_menu() {
        // Five entries that read as groups would put a separator between two gestures made the same
        // way: the mode band is one block, and the rows around it keep their own separators.
        let project = sample_project();
        let flags = ViewFlags::default();
        let entries = menu(&project, &flags, &[id(2)], Subject::Node(id(2)));
        let sections: Vec<bool> = entries
            .iter()
            .filter(|entry| matches!(entry.action, NodeAction::Mode(_)))
            .map(|entry| entry.section)
            .collect();
        assert_eq!(sections, vec![true, false, false, false, false], "the band opens one section, not five");
    }

    #[test]
    fn a_locked_node_keeps_its_edits_and_says_why_they_are_refused() {
        let project = sample_project();
        let mut flags = ViewFlags::default();
        flags.set_locked(id(2), true);
        let entries = menu(&project, &flags, &[id(2)], Subject::Node(id(2)));
        for entry in entries.iter().filter(|entry| entry.id.is_some()) {
            assert!(!entry.enabled, "`{}` is refused on a locked node", entry.label.text(crate::Lang::En));
            assert_eq!(entry.reason, Some(Message::MenuLocked), "and the entry says why");
        }
        let unlock = entries.iter().find(|entry| entry.label == Message::TreeUnlock).expect("the padlock flips");
        assert!(unlock.enabled, "unlocking is how the node becomes editable again");
    }

    #[test]
    fn a_multi_selection_adds_grouping_and_keeps_the_destructive_entry_last() {
        let project = sample_project();
        let flags = ViewFlags::default();
        let entries = menu(&project, &flags, &[id(2), id(3)], Subject::Node(id(2)));
        let labels = labels(&entries);
        assert!(labels.contains(&Message::CmdGroup), "two groupable nodes offer one group, got {labels:?}");
        let last = entries.last().expect("a menu is never empty");
        assert!(last.action.is_destructive(), "the entry under the pointer is never the one that deletes");
    }

    #[test]
    fn every_entry_is_either_an_inventory_row_or_a_declared_session_entry() {
        let project = sample_project();
        let flags = ViewFlags::default();
        for (node, subject) in [(id(2), Subject::Node(id(2))), (id(3), Subject::Node(id(3))), (id(1), Subject::Root)] {
            for entry in menu(&project, &flags, &[node], subject) {
                if let Some(id) = entry.id {
                    let row = inventory::row(id).unwrap_or_else(|| panic!("`{id}` has no inventory row"));
                    assert!(row.has(Place::ContextMenu), "`{id}` is drawn by a menu without declaring it");
                    assert_eq!(row.label, entry.label, "`{id}` prints two different labels");
                }
            }
        }
    }

    #[test]
    fn a_panel_offers_the_build_it_does_not_have() {
        // U123: the row is named after what pressing it makes, and the words come from the table, so
        // a rectangle board offers the niche's shape and a shaped board offers the rectangle.
        let (mut project, _product) = sample_product_project();
        project
            .set_property(id(2), model::SYSTEM_CLASS, model::PropertyValue::class(model::ClassCode::HORIZONTAL_PANEL))
            .expect("the panel takes a class");
        let flags = ViewFlags::default();
        let build = |project: &Project| {
            menu(project, &flags, &[id(2)], Subject::Node(id(2)))
                .into_iter()
                .find(|entry| entry.id == Some("panel.build.set"))
                .map(|entry| (entry.label, entry.action))
        };
        assert_eq!(
            build(&project),
            Some((Message::FillMethodShape, NodeAction::PanelBuild { panel: id(2), mode: model::FillMode::Shape })),
            "a rectangle board offers the niche's shape"
        );
        // Store a ring, and the row reads the other way: a shaped board offers the rectangle.
        let nominal = project.panel(id(2)).expect("panel").nominal_size();
        let l = nominal.length().micrometres();
        let w = nominal.width().micrometres();
        let corners = [(0, 0), (l, 0), (l, w), (0, w)];
        let point = |c: (i64, i64)| [units::Length::from_micrometres(c.0), units::Length::from_micrometres(c.1)];
        let ring = model::Contour::new(
            (0..corners.len())
                .map(|index| {
                    model::ContourSegment::line(point(corners[index]), point(corners[(index + 1) % corners.len()]))
                        .expect("a straight edge")
                })
                .collect(),
        )
        .expect("a closed rectangle");
        project
            .set_panel_shape(
                id(2),
                nominal,
                model::PanelOutline::Contour { contour: ring },
                &[],
                model::PanelTrims::ZERO,
            )
            .expect("the board takes a shaped outline");
        assert_eq!(
            build(&project),
            Some((Message::FillMethodFit, NodeAction::PanelBuild { panel: id(2), mode: model::FillMode::Fit })),
            "a shaped board offers the rectangle"
        );
    }

    #[test]
    fn every_context_menu_row_is_reachable_from_some_node() {
        // The table and the menu agree in both directions: an id declared for a context menu that
        // this module cannot run would be an entry that exists nowhere.
        let (mut project, product) = sample_product_project();
        let group = project.group_nodes(&[id(2), id(3)], "Group").expect("the sample nodes group");
        // A panel's build row asks the class to fill a void, so the fixture's panel is classed: a
        // row declared for the menu must be reachable from a node that can run it (U123).
        project
            .set_property(id(2), model::SYSTEM_CLASS, model::PropertyValue::class(model::ClassCode::HORIZONTAL_PANEL))
            .expect("the sample panel takes a class");
        let flags = ViewFlags::default();
        let subjects = [Subject::Node(id(2)), Subject::Node(id(3)), Subject::Node(group), Subject::Node(product)];
        for row in inventory::in_place(Place::ContextMenu) {
            assert!(
                subjects.iter().any(|subject| {
                    menu(&project, &flags, &[id(2), id(3)], *subject).iter().any(|entry| entry.id == Some(row.id))
                }),
                "`{}` is declared for a context menu that never draws it",
                row.id
            );
        }
    }

    #[test]
    fn a_product_offers_the_niche_and_a_panel_does_not() {
        // The niche is a read inside the open product: the entry arms the tool, so a product offers
        // it and every other container kind — a panel, a group — never (ADR 0098, 0118).
        let (project, product) = sample_product_project();
        let flags = ViewFlags::default();
        let product_menu = menu(&project, &flags, &[product], Subject::Node(product));
        let entry =
            product_menu.iter().find(|entry| entry.id == Some("probe.void")).expect("a product offers its niche");
        assert_eq!(entry.label, Message::ToolVoid);
        assert!(entry.enabled);
        assert_eq!(entry.action, NodeAction::ArmVoid { product });
        let panel_menu = menu(&project, &flags, &[id(2)], Subject::Node(id(2)));
        assert!(panel_menu.iter().all(|entry| entry.id != Some("probe.void")), "a panel owns no void to name");
    }

    #[test]
    fn a_popup_pulled_back_inside_the_scene_still_runs_the_row_under_the_press() {
        // A node's menu is taller than the gap a right click near the bottom of the scene leaves, so
        // the window pulls the popup back inside the view. A press on one of its own rows still has
        // to run that row: the rectangle the frame drew decides, not a hit test that goes stale on a
        // popup the window moved.
        let project = sample_project();
        let flags = ViewFlags::default();
        let selection = [id(2)];
        let source =
            MenuSource { project: &project, flags: &flags, selection: &selection, active_product: None, fill: None };
        let viewport = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 320.0));
        let mut menu = ViewportMenu::default();
        // Twenty points stay below the pointer, and ten rows need far more than that.
        menu.open(Subject::Node(id(2)), egui::pos2(100.0, 300.0));
        let ctx = egui::Context::default();
        crate::set_language(&ctx, crate::Lang::En);

        let frame = |menu: &mut ViewportMenu, events: Vec<egui::Event>| {
            let input = egui::RawInput { screen_rect: Some(viewport), events, ..Default::default() };
            let mut chosen = None;
            let output = ctx.run_ui(input, |ui| {
                chosen = menu.show(ui, viewport, &source, 90);
            });
            let rects = drawn_rects(&output.shapes);
            output.drop_without_applying_deltas();
            (rects, chosen)
        };

        // Two frames: the first lays the popup out, the second draws it where the window pulled it.
        frame(&mut menu, Vec::new());
        let (rects, chosen) = frame(&mut menu, Vec::new());
        assert!(chosen.is_none(), "an idle frame chooses nothing");
        let row = Message::TreeHide.text(crate::Lang::En);
        let at = rects
            .iter()
            .find(|(_, text)| text == row)
            .map(|(rect, _)| rect.center())
            .expect("the menu drew the row it offers");
        assert!(
            menu.rect().is_some_and(|drawn| drawn.top() < 300.0),
            "the menu was pulled up from where the press asked for it"
        );

        let (_, chosen) = frame(&mut menu, vec![egui::Event::PointerMoved(at)]);
        assert!(chosen.is_none(), "moving the pointer runs nothing");
        let button = |pressed: bool| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let (_, chosen) = frame(&mut menu, vec![button(true)]);
        assert!(chosen.is_none(), "the press alone runs nothing, and it does not dismiss the menu either");
        let (_, chosen) = frame(&mut menu, vec![button(false)]);
        assert_eq!(
            chosen,
            Some(NodeAction::SetHidden { node: id(2), hidden: true }),
            "the row under the press is the row that runs"
        );
        assert!(!menu.is_open(), "and the menu closes when it has run");
    }

    #[test]
    fn escape_closes_the_popup_and_an_outside_press_dismisses_it() {
        let project = sample_project();
        let flags = ViewFlags::default();
        let selection = [id(2)];
        let source =
            MenuSource { project: &project, flags: &flags, selection: &selection, active_product: None, fill: None };
        let viewport = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let mut menu = ViewportMenu::default();
        menu.open(Subject::Node(id(2)), egui::pos2(100.0, 100.0));
        assert!(menu.is_open());

        let ctx = egui::Context::default();
        crate::set_language(&ctx, crate::Lang::En);
        let input = egui::RawInput { screen_rect: Some(viewport), ..Default::default() };
        let mut output = ctx.run_ui(input, |ui| {
            assert!(menu.show(ui, viewport, &source, 90).is_none(), "a first frame chooses nothing");
        });
        output.textures_delta.clear();
        let rect = menu.rect().expect("the popup reported what it drew");
        assert!(rect.width() > 0.0 && rect.height() > 0.0, "the popup took a rectangle");

        let escape = egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let input = egui::RawInput { screen_rect: Some(viewport), events: vec![escape], ..Default::default() };
        let mut output = ctx.run_ui(input, |ui| {
            assert!(menu.show(ui, viewport, &source, 90).is_none());
        });
        output.textures_delta.clear();
        assert!(!menu.is_open(), "Escape leaves the menu without running anything");

        menu.open(Subject::Node(id(2)), egui::pos2(100.0, 100.0));
        let press = egui::Event::PointerButton {
            pos: egui::pos2(700.0, 500.0),
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        };
        let input = egui::RawInput {
            screen_rect: Some(viewport),
            events: vec![egui::Event::PointerMoved(egui::pos2(700.0, 500.0)), press],
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            assert!(menu.show(ui, viewport, &source, 90).is_none());
        });
        output.textures_delta.clear();
        assert!(!menu.is_open(), "a press outside the menu dismisses it");
        assert!(menu.rect().is_none(), "and the shell stops treating that spot as chrome");
    }
}
