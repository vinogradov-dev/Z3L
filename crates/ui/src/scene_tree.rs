//! The scene tree.
//!
//! The tree mirrors the document hierarchy: containers contain panels and other containers. A panel
//! is an atomic leaf, so it is never shown with children and can never be ungrouped. A container is
//! an ordinary group or a furniture component; the component badge and its furniture type come from
//! the node's properties. Selection is view state and is shared with the viewport.
//!
//! Every row carries the session's eye and padlock. They are view state keyed by node `Id` (ADR
//! 0065), so the tree writes them directly instead of dispatching a command; a hidden subtree is
//! skipped by both the scene builder and picking, and a locked node is refused a drag and a
//! destructive edit by the shell.

use egui::collapsing_header::CollapsingState;
use egui::{Align2, Color32, CornerRadius, FontId, RichText, Sense, Ui, Vec2};
use engine::Session;
use model::{Container, Id, Node, Panel, Project};
use text::Message;

use crate::icons;
use crate::node_actions::{self, MenuSource, NodeAction, Subject};
use crate::theme;
use crate::view_flags::ViewFlags;

/// The glyph size of a row's visibility and lock toggles, in points.
const TOGGLE_GLYPH: f32 = 16.0;

/// The cell a toggle owns at the right of a row.
///
/// Wide enough for the caption of an asset that is still a placeholder, so the four labels do not
/// resize the row as they swap.
const TOGGLE_CELL: f32 = 34.0;

/// The face size of a toggle whose asset is still an empty placeholder.
const TOGGLE_TEXT_SIZE: f32 = 9.0;

/// How one click changes the selection: the plain click and its two modifiers.
///
/// The list is a document-wide habit rather than a tree or a viewport one, so the tree and the
/// viewport both read their modifiers here instead of each deciding what Ctrl means (U102).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Pick {
    /// A plain click: the selection becomes exactly the picked node.
    #[default]
    Replace,
    /// Shift: the node joins the end of the order, or leaves when it is already selected.
    Add,
    /// Ctrl (⌘ on macOS, so the same finger on every platform): the node leaves and nothing else
    /// changes. It wins when both modifiers are held, because it is the one that cannot be undone
    /// by a slip of the other key.
    Remove,
}

impl Pick {
    /// Reads the modifiers a click reported.
    pub fn from_modifiers(modifiers: &egui::Modifiers) -> Self {
        if modifiers.ctrl || modifiers.command {
            Self::Remove
        } else if modifiers.shift {
            Self::Add
        } else {
            Self::Replace
        }
    }
}

/// The selected document nodes, in click order. The last one is the primary selection.
#[derive(Debug, Default)]
pub(crate) struct Selection {
    ids: Vec<Id>,
}

impl Selection {
    /// The node whose card is shown and whose context actions are enabled.
    pub(crate) fn primary(&self) -> Option<Id> {
        self.ids.last().copied()
    }

    /// All selected node ids, oldest first.
    pub(crate) fn ids(&self) -> &[Id] {
        &self.ids
    }

    /// Whether the node is selected.
    pub(crate) fn contains(&self, id: Id) -> bool {
        self.ids.contains(&id)
    }

    /// Replaces the selection with one node.
    pub(crate) fn replace(&mut self, id: Id) {
        self.ids.clear();
        self.ids.push(id);
    }

    /// Replaces the selection with several nodes, in the given order.
    pub(crate) fn replace_all(&mut self, ids: Vec<Id>) {
        self.ids = ids;
    }

    /// Empties the selection.
    pub(crate) fn clear(&mut self) {
        self.ids.clear();
    }

    /// Adds the node at the end of the order, or removes it when it is already selected.
    ///
    /// The toggle is what Shift does: a second Shift-click on the same node takes it back out, so a
    /// member added by mistake is one modifier away from leaving.
    pub(crate) fn toggle(&mut self, id: Id) {
        if let Some(index) = self.ids.iter().position(|selected| *selected == id) {
            self.ids.remove(index);
        } else {
            self.ids.push(id);
        }
    }

    /// Removes the node and leaves every other member where it is.
    pub(crate) fn remove(&mut self, id: Id) {
        self.ids.retain(|selected| *selected != id);
    }

    /// Applies one click, with the modifier it carried, to the node it picked.
    pub(crate) fn pick(&mut self, id: Id, pick: Pick) {
        match pick {
            Pick::Replace => self.replace(id),
            Pick::Add => self.toggle(id),
            Pick::Remove => self.remove(id),
        }
    }

    /// Drops ids that no longer exist, for example after undo or delete.
    pub(crate) fn retain_existing(&mut self, project: &Project) {
        self.ids.retain(|id| project.contains_node(*id));
    }

    /// Selects the first panel in depth-first order.
    pub(crate) fn select_first_panel(&mut self, project: &Project) {
        if let Some(panel) = first_panel(project.root()) {
            self.replace(panel);
        }
    }
}

/// Draws the container tree, returning the actions the user requested.
///
/// The tree owns no toolbar and no heading: the surface it sits on draws its own header, and every
/// node action is reached from the row's context menu. The one multi-node action, grouping, is
/// offered there while two or more nodes are selected.
pub(crate) fn show(
    ui: &mut Ui,
    session: &Session,
    selection: &mut Selection,
    flags: &mut ViewFlags,
    base: Option<Id>,
) -> Vec<NodeAction> {
    let rows = Rows { project: session.project(), active: session.active_product(), base };
    let mut actions = Vec::new();
    show_container(ui, &rows, rows.project.root(), selection, flags, &mut actions, 0);
    actions
}

/// What every row of one tree frame reads: the document, the product it works in, and the base an
/// armed boolean is born in.
///
/// The three values are one argument because they change together and every level of the recursion
/// passes them down unchanged, so the row functions stay inside `clippy::too_many_arguments` without
/// an allow (AGENTS §1.5).
struct Rows<'a> {
    project: &'a Project,
    active: Option<Id>,
    /// The base ring of an armed boolean, drawn in the base ink rather than the selection ink (X99).
    base: Option<Id>,
}

/// What one row is in the frame's selection, and whether it is a boolean's base.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Role {
    /// The row is selected, so it carries the selection's wash.
    picked: bool,
    /// The row is the base an armed boolean is born in, so its name takes the base ink.
    base: bool,
}

/// The role of one node's row in this frame.
fn role_of(rows: &Rows<'_>, selection: &Selection, id: Id) -> Role {
    Role { picked: selection.contains(id), base: rows.base == Some(id) }
}

fn container_label(
    ui: &Ui,
    project: &Project,
    container: &Container,
    active: Option<Id>,
) -> (RichText, Option<String>) {
    let name = container.name();
    if container.id() == project.root().id() {
        return (RichText::new(name).strong(), None);
    }
    let lang = crate::language(ui.ctx());
    let (mut text, mut tooltip) = if !container.is_component() {
        (RichText::new(name), None)
    } else {
        match project.class(container.id()) {
            Some(class) if !class.is_unclassified() => {
                let code = class.to_string();
                // The row says what the digits mean; the code stays one hover away, because the
                // digits are what an export and a conversation with the shop use.
                let label = class.label().map(|label| label.text(lang)).unwrap_or(&code);
                (RichText::new(format!("{name}  [{label}]")), Some(code))
            }
            _ => (RichText::new(format!("{name}  [{}]", Message::TreeBadgeComponent.text(lang))), None),
        }
    };
    if active == Some(container.id()) {
        // The working product reads stronger than the closed pieces around it: new parts land in
        // it, so the tree says which piece is open without a second badge.
        text = text.strong();
        let hint = Message::TreeActiveProduct.text(lang).to_owned();
        tooltip = Some(match tooltip {
            Some(code) => format!("{hint}  ·  {code}"),
            None => hint,
        });
    }
    (text, tooltip)
}

fn show_container(
    ui: &mut Ui,
    rows: &Rows<'_>,
    container: &Container,
    selection: &mut Selection,
    flags: &mut ViewFlags,
    actions: &mut Vec<NodeAction>,
    depth: u32,
) {
    let id = container.id();
    let is_root = id == rows.project.root().id();
    let is_product = container.is_product();
    let state_id = ui.make_persistent_id(("scene_container", id.get()));
    let state = CollapsingState::load_with_default_open(ui.ctx(), state_id, depth < 2);
    let label = container_label(ui, rows.project, container, rows.active);
    let role = role_of(rows, selection, id);
    let (_toggle, header, _) = state.show_header(ui, |ui| row(ui, id, label.0, role, flags, !is_root)).body(|ui| {
        for child in container.children() {
            show_node(ui, rows, child, selection, flags, actions, depth.saturating_add(1));
        }
    });
    let mut header = header.inner;
    if let Some(tooltip) = label.1 {
        header = header.on_hover_text(tooltip);
    }
    if header.clicked() {
        select(ui, selection, id);
        // A click on a product row opens that product: the tree is the second half of the
        // double-click rule, and it is a view operation, never a document command (ADR 0098).
        if is_product {
            actions.push(NodeAction::OpenProduct { node: id });
        }
    }
    // The menu reads the flags after the row drew its own toggles, so its captions say what the
    // eye and the padlock show now instead of a copy taken before the row.
    let subject = if is_root { Subject::Root } else { Subject::Node(id) };
    header.context_menu(|ui| node_menu(ui, subject, rows, flags, selection, actions));
}

fn show_node(
    ui: &mut Ui,
    rows: &Rows<'_>,
    node: &Node,
    selection: &mut Selection,
    flags: &mut ViewFlags,
    actions: &mut Vec<NodeAction>,
    depth: u32,
) {
    match node {
        Node::Container(container) => show_container(ui, rows, container, selection, flags, actions, depth),
        Node::Panel(panel) => show_panel(ui, rows, panel, selection, flags, actions),
        Node::Object(object) => show_object(ui, rows, object, selection, flags, actions),
        Node::Camera(_) => show_leaf(ui, rows, node, Message::TreeBadgeCamera, selection, flags, actions),
        Node::Light(_) => show_leaf(ui, rows, node, Message::TreeBadgeLight, selection, flags, actions),
    }
}

/// A camera or a light row: a named leaf with a kind tag and the usual duplicate/delete actions.
fn show_leaf(
    ui: &mut Ui,
    rows: &Rows<'_>,
    node: &Node,
    kind: Message,
    selection: &mut Selection,
    flags: &mut ViewFlags,
    actions: &mut Vec<NodeAction>,
) {
    let id = node.id();
    let label = RichText::new(format!("{}  [{}]", node.name(), kind.text(crate::language(ui.ctx()))));
    let role = role_of(rows, selection, id);
    let response = row(ui, id, label, role, flags, true);
    if response.clicked() {
        select(ui, selection, id);
    }
    response.context_menu(|ui| node_menu(ui, Subject::Node(id), rows, flags, selection, actions));
}

fn show_object(
    ui: &mut Ui,
    rows: &Rows<'_>,
    object: &model::Object,
    selection: &mut Selection,
    flags: &mut ViewFlags,
    actions: &mut Vec<NodeAction>,
) {
    let id = object.id();
    let lang = crate::language(ui.ctx());
    let kind = match object.geometry() {
        model::Geometry::Mesh(_) => Message::TreeBadgeMesh,
        model::Geometry::Primitive(primitive) => match primitive.kind() {
            model::PrimitiveKind::Sphere => Message::TreeBadgeSphere,
            model::PrimitiveKind::Cone => Message::TreeBadgeCone,
        },
        model::Geometry::Constructive(_) => Message::TreeBadgeContour,
        model::Geometry::Path(_) => Message::TreeBadgePath,
        model::Geometry::Contours(_) => Message::TreeBadgeContours,
    };
    let label = RichText::new(format!("{}  [{}]", object.name(), kind.text(lang)));
    let role = role_of(rows, selection, id);
    let response = row(ui, id, label, role, flags, true);
    if response.clicked() {
        select(ui, selection, id);
    }
    response.context_menu(|ui| node_menu(ui, Subject::Node(id), rows, flags, selection, actions));
}

/// One row's context menu: the shared list of the node's actions, drawn where egui anchors it.
///
/// The entries come from [`crate::node_actions`], the same list the viewport popup draws, so a
/// command cannot reach one surface and miss the other. The tree closes its menu itself; the popup
/// owns its own surface and closes the same way.
fn node_menu(
    ui: &mut Ui,
    subject: Subject,
    rows: &Rows<'_>,
    flags: &ViewFlags,
    selection: &Selection,
    actions: &mut Vec<NodeAction>,
) {
    let source = MenuSource { project: rows.project, flags, selection: selection.ids(), active_product: rows.active };
    if let Some(action) = node_actions::show(ui, &node_actions::entries(subject, &source)) {
        actions.push(action);
        ui.close();
    }
}

fn show_panel(
    ui: &mut Ui,
    rows: &Rows<'_>,
    panel: &Panel,
    selection: &mut Selection,
    flags: &mut ViewFlags,
    actions: &mut Vec<NodeAction>,
) {
    let id = panel.id();
    let role = role_of(rows, selection, id);
    let response = row(ui, id, RichText::new(panel.name()), role, flags, true);
    if response.clicked() {
        select(ui, selection, id);
    }
    response.context_menu(|ui| node_menu(ui, Subject::Node(id), rows, flags, selection, actions));
}

/// Selects a row by the click it took, with the modifier the window reports for it.
fn select(ui: &Ui, selection: &mut Selection, id: Id) {
    selection.pick(id, Pick::from_modifiers(&ui.input(|input| input.modifiers)));
}

/// One row: its label on the left and the eye and padlock on the right.
///
/// The two toggles are revealed on hover, or kept visible while their flag is set, so a row at rest
/// reads as a name and a hidden or locked row still says why it behaves differently. They are
/// painted inside the row's own rectangle and reserve their cells at all times, so a hover never
/// moves the text. `toggles` is false for the root, which is a frame and not an object.
fn row(ui: &mut Ui, id: Id, label: RichText, role: Role, flags: &mut ViewFlags, toggles: bool) -> egui::Response {
    let height = ui.spacing().interact_size.y.max(TOGGLE_GLYPH + 2.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), height), Sense::hover());
    let row_id = ui.make_persistent_id(("scene_row", id.get()));
    if !ui.is_rect_visible(rect) {
        return ui.interact(rect, row_id, Sense::click());
    }
    let hidden = flags.is_hidden(id);
    let locked = flags.is_locked(id);
    let reserved = if toggles { 2.0 * TOGGLE_CELL } else { 0.0 };
    let label_right = (rect.right() - reserved).max(rect.left());
    let label_rect = egui::Rect::from_min_max(rect.min, egui::pos2(label_right, rect.bottom()));
    let response = ui.interact(label_rect, row_id, Sense::click());
    let hovered = ui.rect_contains_pointer(rect);
    let visuals = ui.style().interact_selectable(&response, role.picked);
    // The wash covers the toggles' cells too, so a hovered row reads as one row rather than as a
    // label that stops before two floating captions.
    if role.picked || hovered {
        ui.painter().rect_filled(rect, CornerRadius::same(theme::RADIUS), visuals.weak_bg_fill);
    }
    // The base of an armed boolean keeps its ink at rest, under the pointer and while it is also
    // picked: it is a role of the operation, not a state of the pointer (X99).
    let color = if hidden {
        theme::text_dim(ui)
    } else if role.base {
        theme::boolean_base(ui)
    } else {
        visuals.text_color()
    };
    let label = if hidden { label.color(color) } else { label };
    let full = label.text().to_owned();
    let padding = ui.spacing().button_padding.x;
    let galley = egui::WidgetText::from(label).into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        (label_rect.width() - 2.0 * padding).max(0.0),
        egui::TextStyle::Button,
    );
    let elided = galley.elided;
    let position = egui::pos2(label_rect.left() + padding, label_rect.center().y - 0.5 * galley.size().y);
    ui.painter().with_clip_rect(label_rect).galley(position, galley, color);
    // A name that does not fit is elided with `…`, so the full name is one hover away.
    let response = if elided { response.on_hover_text(full) } else { response };
    if toggles && rect.width() > reserved {
        let eye = egui::Rect::from_min_size(
            egui::pos2(rect.right() - reserved, rect.top()),
            egui::vec2(TOGGLE_CELL, rect.height()),
        );
        let lock =
            egui::Rect::from_min_size(egui::pos2(eye.right(), rect.top()), egui::vec2(TOGGLE_CELL, rect.height()));
        if hovered || hidden {
            let (icon, caption) = if hidden {
                (icons::actions::HIDDEN, Message::TreeShow)
            } else {
                (icons::actions::VISIBLE, Message::TreeHide)
            };
            if toggle(ui, eye, id, "visibility", icon, caption) {
                flags.set_hidden(id, !hidden);
            }
        }
        if hovered || locked {
            let (icon, caption) = if locked {
                (icons::actions::UNLOCK, Message::TreeUnlock)
            } else {
                (icons::actions::LOCK, Message::TreeLock)
            };
            if toggle(ui, lock, id, "lock", icon, caption) {
                flags.set_locked(id, !locked);
            }
        }
    }
    response
}

/// One 16 pt row toggle: its glyph, or its caption while the asset is an empty placeholder.
fn toggle(ui: &mut Ui, rect: egui::Rect, id: Id, kind: &str, icon: icons::Icon, caption: Message) -> bool {
    let response = ui.interact(rect, ui.make_persistent_id(("scene_toggle", id.get(), kind)), Sense::click());
    let wash = if response.is_pointer_button_down_on() {
        theme::control_active(ui)
    } else if response.hovered() {
        theme::control_hover(ui)
    } else {
        Color32::TRANSPARENT
    };
    if wash != Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, CornerRadius::same(theme::RADIUS), wash);
    }
    let ink = if response.hovered() { theme::text(ui) } else { theme::text_dim(ui) };
    if icon.is_drawn() {
        let glyph = egui::Rect::from_center_size(rect.center(), Vec2::splat(TOGGLE_GLYPH));
        icons::paint_icon(ui, icon, glyph, TOGGLE_GLYPH, ink);
    } else {
        ui.painter().with_clip_rect(rect).text(
            rect.center(),
            Align2::CENTER_CENTER,
            caption.text(crate::language(ui.ctx())),
            FontId::proportional(TOGGLE_TEXT_SIZE),
            ink,
        );
    }
    response.on_hover_text(caption.text(crate::language(ui.ctx()))).clicked()
}

/// Returns the first panel in depth-first order, used to open the card on a useful node.
pub(crate) fn first_panel(root: &Container) -> Option<Id> {
    first_panel_in(root.children())
}

fn first_panel_in(children: &[Node]) -> Option<Id> {
    for child in children {
        match child {
            Node::Panel(panel) => return Some(panel.id()),
            Node::Container(container) => {
                if let Some(id) = first_panel_in(container.children()) {
                    return Some(id);
                }
            }
            Node::Object(_) | Node::Camera(_) | Node::Light(_) => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use model::{CameraProjection, Id, LightKind, Node, Panel, PanelNominalSize, Placement};
    use testkit::{sample_catalog, sample_project};

    use super::*;

    /// Every text a run drew and the ink it was painted in, in paint order; the row captions and
    /// labels are all this test needs.
    fn drawn_text(shapes: &[egui::epaint::ClippedShape]) -> Vec<(String, Color32)> {
        fn walk(shape: &egui::Shape, out: &mut Vec<(String, Color32)>) {
            match shape {
                egui::Shape::Text(text) => out.push((text.galley.text().to_owned(), text.fallback_color)),
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

    /// The number of painted icon rasters: a drawn toggle is a texture mesh, not text.
    fn drawn_images(shapes: &[egui::epaint::ClippedShape]) -> usize {
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

    /// One run's texts and its painted icon rasters.
    fn draw(session: &Session, flags: &mut ViewFlags) -> (Vec<String>, usize) {
        let (texts, images) = draw_with(session, &mut Selection::default(), flags, None);
        (texts.iter().map(|(text, _)| text.clone()).collect(), images)
    }

    /// One run of the tree with the selection and the armed base the caller hands it.
    fn draw_with(
        session: &Session,
        selection: &mut Selection,
        flags: &mut ViewFlags,
        base: Option<Id>,
    ) -> (Vec<(String, Color32)>, usize) {
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let output = ctx.run_ui(Default::default(), |ui| {
            show(ui, session, selection, flags, base);
        });
        let texts = drawn_text(&output.shapes);
        let images = drawn_images(&output.shapes);
        output.drop_without_applying_deltas();
        (texts, images)
    }

    /// The ink a run painted `row` in, for a row the tree drew.
    fn ink_of_row(texts: &[(String, Color32)], needle: &str) -> Color32 {
        let (text, ink) = texts
            .iter()
            .find(|(text, _)| text.contains(needle))
            .unwrap_or_else(|| panic!("the tree drew no row named {needle}: {texts:?}"));
        assert!(text.contains(needle));
        *ink
    }

    /// The base ink the frame marks an armed boolean's target with, as the theme reads it.
    fn base_ink() -> Color32 {
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let mut ink = Color32::TRANSPARENT;
        let output = ctx.run_ui(Default::default(), |ui| ink = crate::theme::boolean_base(ui));
        output.drop_without_applying_deltas();
        ink
    }

    fn panel(id: u64) -> Panel {
        Panel::new(
            Id::new(id).expect("id"),
            format!("Panel {id}"),
            PanelNominalSize::new(
                units::Length::from_millimetres(600).expect("length"),
                units::Length::from_millimetres(400).expect("width"),
            )
            .expect("size"),
            None,
            units::Length::from_millimetres(18).expect("thickness"),
            Placement::IDENTITY,
        )
        .expect("panel")
    }

    fn container(id: u64, children: Vec<Node>) -> Container {
        Container::new(Id::new(id).expect("id"), format!("Container {id}"), Placement::IDENTITY, children)
    }

    #[test]
    fn a_booleans_base_row_reads_in_the_base_ink() {
        // X99: the viewport marks the base ring with an ink of its own while a boolean is armed, and
        // the tree marks the same node with the same ink, so the row and the ring are one statement.
        // The mark is the base's, not the selection's: a plain click on the tool drops the base from
        // the selection, and the base stays marked anyway.
        let session = Session::with_directory(sample_project(), sample_catalog());
        let shelf = Id::new(2).expect("shelf id");
        let mut flags = ViewFlags::default();

        let (plain, _) = draw_with(&session, &mut Selection::default(), &mut flags, None);
        let resting = ink_of_row(&plain, "Shelf");

        let (marked, _) = draw_with(&session, &mut Selection::default(), &mut flags, Some(shelf));
        assert_eq!(ink_of_row(&marked, "Shelf"), base_ink(), "the base row wears the base ink");
        assert_ne!(ink_of_row(&marked, "Shelf"), resting, "which is not the ink a row rests in");
        assert_eq!(ink_of_row(&marked, "Front"), ink_of_row(&plain, "Front"), "a row beside it is untouched");

        // Picked as well as base: one row, one role, and the base ink still wins.
        let mut selection = Selection::default();
        selection.pick(shelf, Pick::Replace);
        let (picked, _) = draw_with(&session, &mut selection, &mut flags, Some(shelf));
        assert_eq!(ink_of_row(&picked, "Shelf"), base_ink(), "a picked base keeps the ink it is marked with");

        // Dropping the mode returns the row to the ink every other row wears.
        let (dropped, _) = draw_with(&session, &mut Selection::default(), &mut flags, None);
        assert_eq!(ink_of_row(&dropped, "Shelf"), resting, "no boolean, no base ink");
    }

    #[test]
    fn a_hidden_container_still_lists_its_children() {
        // Hiding a branch is a viewport decision, not a tree one: the row stays where the user can
        // find it and switch it back on (ADR 0065).
        let session = Session::with_directory(sample_project(), sample_catalog());
        let mut flags = ViewFlags::default();
        flags.set_hidden(Id::new(3).expect("drawer id"), true);
        let (texts, _) = draw(&session, &mut flags);
        assert!(texts.iter().any(|text| text.contains("Front")), "the hidden drawer still lists its front: {texts:?}");
        assert!(texts.iter().any(|text| text.contains("Shelf")), "a sibling branch is listed too: {texts:?}");
    }

    #[test]
    fn a_row_at_rest_reveals_nothing_and_a_flagged_row_keeps_its_toggle() {
        let session = Session::with_directory(sample_project(), sample_catalog());
        let shelf = Id::new(2).expect("shelf id");
        let mut flags = ViewFlags::default();
        let (resting, resting_images) = draw(&session, &mut flags);
        assert!(
            !resting.iter().any(|text| text == "Hide" || text == "Lock"),
            "the eye and the padlock wait for the pointer: {resting:?}"
        );
        assert_eq!(resting_images, 0, "a row at rest paints no toggle glyph");

        flags.set_hidden(shelf, true);
        let (hidden, hidden_images) = draw(&session, &mut flags);
        assert!(!hidden.iter().any(|text| text == "Show"), "the eye is a glyph, not a caption: {hidden:?}");
        assert_eq!(hidden_images, 1, "a hidden row keeps its eye");
        assert!(
            !hidden.iter().any(|text| text == "Lock"),
            "an unlocked row still hides its padlock until hovered: {hidden:?}"
        );

        flags.set_locked(shelf, true);
        flags.set_hidden(shelf, false);
        let (locked, locked_images) = draw(&session, &mut flags);
        assert!(!locked.iter().any(|text| text == "Unlock"), "the padlock is a glyph, not a caption: {locked:?}");
        assert_eq!(locked_images, 1, "a locked row keeps its padlock");
    }

    #[test]
    fn a_component_badge_shows_the_class_label_instead_of_its_digits() {
        use engine::NodePropertySet;
        use engine::node_cmds::NODE_PROPERTY_SET_ID;
        use model::{PropertyValue, SYSTEM_CLASS};

        let mut session = Session::with_directory(sample_project(), sample_catalog());
        let drawer = Id::new(3).expect("drawer id");
        let (before, _) = draw(&session, &mut ViewFlags::default());
        assert!(before.iter().any(|text| text == "Drawer  [component]"), "a bare component says so: {before:?}");

        let params = crate::command::json(&NodePropertySet {
            node: drawer,
            key: SYSTEM_CLASS.to_owned(),
            value: PropertyValue::class("10.11".parse().expect("horizontal panel")),
        })
        .expect("class parameters");
        session.dispatch(NODE_PROPERTY_SET_ID, &params).expect("assign the class");

        let (after, _) = draw(&session, &mut ViewFlags::default());
        assert!(
            after.iter().any(|text| text == "Drawer  [Horizontal]"),
            "the badge names the class and keeps the code for the tooltip: {after:?}"
        );
    }

    #[test]
    fn first_panel_is_depth_first() {
        let project = sample_project();
        let expected = Id::new(2).expect("id");
        assert_eq!(first_panel(project.root()), Some(expected));
        assert_eq!(first_panel(&container(10, Vec::new())), None);
    }

    #[test]
    fn first_panel_descends_into_nested_containers() {
        let nested = container(30, vec![Node::Panel(panel(21)), Node::Panel(panel(22))]);
        let root = container(10, vec![Node::Container(nested), Node::Panel(panel(11))]);
        assert_eq!(first_panel(&root), Some(Id::new(21).expect("id")));
    }

    #[test]
    fn selection_replaces_and_toggles() {
        let first = Id::new(2).expect("id");
        let second = Id::new(3).expect("id");
        let mut selection = Selection::default();
        assert_eq!(selection.primary(), None);

        selection.replace(first);
        assert!(selection.contains(first));
        assert_eq!(selection.primary(), Some(first));

        selection.toggle(second);
        assert_eq!(selection.ids(), &[first, second]);
        assert_eq!(selection.primary(), Some(second));

        selection.toggle(first);
        assert_eq!(selection.ids(), &[second]);
        assert_eq!(selection.primary(), Some(second));

        selection.replace(first);
        assert_eq!(selection.ids(), &[first]);

        selection.clear();
        assert_eq!(selection.primary(), None);
        assert!(selection.ids().is_empty());
    }

    #[test]
    fn a_click_applies_its_modifier_to_the_order() {
        // U102: Shift adds at the end and takes the same node back out, Ctrl removes one member, and
        // a plain click replaces the order. A loft through several profiles reads this list in the
        // order the person built it, so an order that did not survive a toggle would be a wrong model.
        let first = Id::new(2).expect("id");
        let second = Id::new(3).expect("id");
        let third = Id::new(4).expect("id");
        let mut selection = Selection::default();

        selection.pick(first, Pick::Replace);
        selection.pick(second, Pick::Add);
        selection.pick(third, Pick::Add);
        assert_eq!(selection.ids(), &[first, second, third], "Shift appends in click order");
        assert_eq!(selection.primary(), Some(third), "the last one added is the primary");

        selection.pick(second, Pick::Add);
        assert_eq!(selection.ids(), &[first, third], "a second Shift-click takes the member out");
        assert_eq!(selection.primary(), Some(third), "and the order of the rest is kept");

        selection.pick(first, Pick::Remove);
        assert_eq!(selection.ids(), &[third], "Ctrl removes one member and leaves the rest alone");
        selection.pick(first, Pick::Remove);
        assert_eq!(selection.ids(), &[third], "removing a node that is not selected changes nothing");

        selection.pick(first, Pick::Replace);
        assert_eq!(selection.ids(), &[first], "a plain click replaces the order");
    }

    #[test]
    fn the_modifiers_name_their_own_pick() {
        use egui::Modifiers;
        let both = Modifiers { shift: true, ..Modifiers::CTRL };
        assert_eq!(Pick::from_modifiers(&Modifiers::NONE), Pick::Replace, "a plain click replaces");
        assert_eq!(Pick::from_modifiers(&Modifiers::SHIFT), Pick::Add, "Shift adds");
        assert_eq!(Pick::from_modifiers(&Modifiers::CTRL), Pick::Remove, "Ctrl removes");
        assert_eq!(Pick::from_modifiers(&Modifiers::COMMAND), Pick::Remove, "⌘ removes where it is the modifier");
        assert_eq!(Pick::from_modifiers(&both), Pick::Remove, "Ctrl wins when both are held");
    }

    #[test]
    fn selection_drops_removed_nodes() {
        let project = sample_project();
        let mut selection = Selection::default();
        selection.replace(Id::new(2).expect("id"));
        selection.toggle(Id::new(999).expect("id"));
        selection.retain_existing(&project);
        assert_eq!(selection.ids(), &[Id::new(2).expect("id")]);
        selection.replace(Id::new(3).expect("id"));
        selection.retain_existing(&project);
        assert!(selection.contains(Id::new(3).expect("id")));
    }

    /// The tree is a view and a set of structural edits only.
    ///
    /// Creation is not a `NodeAction`: the toolbar strip owns every tool, so neither context menu
    /// can grow a second creation path by adding a button back. Arming a container creates nothing,
    /// so `AddChild` is allowed and is checked here alongside the rest.
    #[test]
    fn the_tree_has_no_creation_action() {
        let kinds = [
            NodeAction::AddChild { parent: Id::new(3).expect("id") },
            NodeAction::Duplicate { node: Id::new(2).expect("id") },
            NodeAction::Delete { node: Id::new(2).expect("id") },
            NodeAction::Group { nodes: Vec::new() },
            NodeAction::Ungroup { node: Id::new(2).expect("id") },
            NodeAction::ConvertToGroup { node: Id::new(2).expect("id") },
        ];
        for action in kinds {
            let debug = format!("{action:?}");
            assert!(!debug.starts_with("Create"), "the tree must not create, got {debug}");
        }
    }

    #[test]
    fn selection_keeps_authored_cameras_and_lights() {
        let mut project = sample_project();
        let root = project.root().id();
        let camera =
            project.add_camera(root, "Camera", Placement::IDENTITY, CameraProjection::DEFAULT).expect("camera");
        let light = project.add_light(root, "Key", Placement::IDENTITY, LightKind::Sun, [1.0; 3], 1.0).expect("light");

        let mut selection = Selection::default();
        selection.replace_all(vec![camera, light]);
        selection.retain_existing(&project);
        assert_eq!(
            selection.ids(),
            &[camera, light],
            "a camera and a light are scene nodes, so the next frame keeps them selected"
        );
    }
}
