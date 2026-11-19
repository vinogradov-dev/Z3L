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

use egui::{CornerRadius, Margin, Order, Rect, Stroke};

use model::{Container, Id, Project};
use text::Message;

use crate::inventory::{self, NodeKind, Place};
use crate::transform::TransformMode;
use crate::view_flags::ViewFlags;
use crate::{language, theme};

/// The width of the popup, in points: wide enough for the longest command label with its margin.
const MENU_WIDTH: f32 = 168.0;

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
}

/// The entries `subject` owns, in the order a menu draws them.
///
/// The capability entries are the inventory filtered by the node's kind, so a new command with a
/// context-menu place appears in both surfaces without either file changing. The session entries
/// are declared here because they dispatch no command: hiding a node is view state, not an edit.
pub fn entries(subject: Subject, source: &MenuSource<'_>) -> Vec<Entry> {
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
        });
        entries.push(Entry {
            id: None,
            label: if locked { Message::TreeUnlock } else { Message::TreeLock },
            action: NodeAction::SetLocked { node: id, locked: !locked },
            enabled: true,
            reason: None,
            section: false,
        });
        // Isolate needs a subject; show all is how a person finds a model that disappeared, so it
        // is the one entry empty space owns.
        entries.push(Entry {
            id: None,
            label: Message::TreeIsolate,
            action: NodeAction::Isolate { node: id },
            enabled: true,
            reason: Some(Message::TreeIsolateHint),
            section: true,
        });
    }
    entries.push(Entry {
        id: None,
        label: Message::TreeShowAll,
        action: NodeAction::ShowAll,
        enabled: true,
        reason: Some(Message::TreeShowAllHint),
        section: false,
    });

    // The window's four gestures, beside the view entries because they write no document either:
    // the mode is view state, so a node's row menu is where a maker reaches it without leaving the
    // piece in hand (U113). Empty space and the root draw no entry of their own here: a mode acts on
    // a selection, and the status bar's band is always one press away.
    if node.is_some() && std::env::var("Z3L_NO_MODES").is_err() {
        for (index, mode) in TransformMode::ALL.into_iter().enumerate() {
            entries.push(Entry {
                id: None,
                label: mode.words(),
                action: NodeAction::Mode(mode),
                enabled: true,
                reason: Some(mode.hint_words()),
                section: index == 0,
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
/// wording, order or enablement. A refused entry is drawn disabled with its reason as the hover
/// text; the caller closes its own surface.
pub fn show(ui: &mut egui::Ui, entries: &[Entry]) -> Option<NodeAction> {
    let lang = language(ui.ctx());
    let mut chosen = None;
    for (index, entry) in entries.iter().enumerate() {
        if entry.section && index > 0 {
            ui.separator();
        }
        let text = egui::RichText::new(entry.label.text(lang));
        let response = ui.add_enabled(entry.enabled, egui::Button::new(text).min_size(egui::vec2(MENU_WIDTH, 0.0)));
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
        let source = MenuSource { project, flags, selection, active_product: None };
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
                Message::CmdDuplicate,
                Message::CmdDelete,
            ],
            "a panel's menu: the eye, the padlock, the view entries and the four gestures, then its edits"
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
    fn the_four_gestures_are_session_entries_of_a_node_menu() {
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
            "the menu offers the four in the band's order"
        );
        for (entry, mode) in modes.iter().zip(TransformMode::ALL) {
            assert!(entry.id.is_none(), "a mode is view state, so it names no command");
            assert_eq!(entry.label, mode.words(), "the menu and the band print one word");
            assert_eq!(entry.reason, Some(mode.hint_words()), "and the hover says which gesture it is");
            assert!(entry.enabled, "a locked node refuses an edit, never a view mode");
        }
    }

    #[test]
    fn the_four_gestures_are_one_section_of_the_menu() {
        // Four entries that read as three groups would put a separator between two moves made the
        // same way: the mode band is one block, and the rows around it keep their own separators.
        let project = sample_project();
        let flags = ViewFlags::default();
        let entries = menu(&project, &flags, &[id(2)], Subject::Node(id(2)));
        let sections: Vec<bool> = entries
            .iter()
            .filter(|entry| matches!(entry.action, NodeAction::Mode(_)))
            .map(|entry| entry.section)
            .collect();
        assert_eq!(sections, vec![true, false, false, false], "the band opens one section, not four");
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
    fn every_context_menu_row_is_reachable_from_some_node() {
        // The table and the menu agree in both directions: an id declared for a context menu that
        // this module cannot run would be an entry that exists nowhere.
        let (mut project, product) = sample_product_project();
        let group = project.group_nodes(&[id(2), id(3)], "Group").expect("the sample nodes group");
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
        let source = MenuSource { project: &project, flags: &flags, selection: &selection, active_product: None };
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
        let source = MenuSource { project: &project, flags: &flags, selection: &selection, active_product: None };
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
