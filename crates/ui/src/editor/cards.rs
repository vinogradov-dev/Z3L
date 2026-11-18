//! The panels that draw the scene tree and the inspector card of the primary selection.
//!
//! The scene tree dispatches document commands; the inspector picks the card that fits the selected
//! node and hands each card the mode it may start, so the panel, the object, the camera and the
//! light card never own an edit mode themselves. Errors are shown in the tree and cleared by the
//! next action.
//!
//! Refuses nothing directly: a refused command is shown as its sentence, and the editable modes
//! (`start_mesh_edit`, `start_contour_edit`, `arm_creation`) refuse a node that cannot take them.
//!
//! Changes when: a card joins the inspector or a tree action changes.

use egui::Ui;

use engine::node_cmds::{
    NODE_COMPONENT_CONVERT_TO_GROUP_ID, NODE_DELETE_ID, NODE_DUPLICATE_ID, NODE_GROUP_ID, NODE_UNGROUP_ID,
};
use engine::{NodeComponentConvertToGroup, NodeDelete, NodeDuplicate, NodeGroup, NodeUngroup, Session};
use model::Id;
use text::Message;

use super::Editor;
use crate::command::{dispatch, json};
use crate::container_card::ContainerAction;
use crate::contour_edit::{ContourEdit, ContourTarget};
use crate::node_actions::NodeAction;
use crate::object_card::ObjectAction;
use crate::panel_card::PanelAction;
use crate::scene_tree;
use crate::theme;
use crate::tool_card::{ToolOutcome, ToolView};

impl Editor {
    /// Draws the scene tree and dispatches the tree actions.
    pub fn show_scene_tree(&mut self, ui: &mut Ui, session: &mut Session) {
        self.selection.retain_existing(session.project());
        self.retain_open(session.project());
        self.retain_flags(session.project());
        self.retain_creation_parent(session.project());
        if !self.initialized {
            self.selection.select_first_panel(session.project());
            self.initialized = true;
        }
        if let Some(error) = &self.error {
            ui.colored_label(theme::danger(ui), error);
            ui.separator();
        }
        let actions = scene_tree::show(ui, session, &mut self.selection, &mut self.flags);
        if actions.is_empty() {
            return;
        }
        self.error = None;
        for action in actions {
            if let Err(error) = self.apply_node_action(session, action, crate::language(ui.ctx())) {
                self.error = Some(error);
                break;
            }
        }
        ui.ctx().request_repaint();
    }

    /// Draws the inspector card for the primary selection, with the armed tool's parameters above it.
    ///
    /// A running tool that asks for a number draws the same rows as its card at the pointer (ADR
    /// 0119), from the draft the shell built; the outcome goes back to the shell, which takes the
    /// numbers through the tool's own path. Nothing here writes a document.
    pub fn show_card(&mut self, ui: &mut Ui, session: &mut Session, tool: Option<&mut ToolView>) -> ToolOutcome {
        let mut outcome = ToolOutcome::default();
        if let Some(view) = tool
            && !view.is_empty()
        {
            outcome = crate::tool_card::section(ui, view, None);
        }
        self.retain_mesh_edit(session.project());
        self.retain_contour_edit(session.project());
        let Some(primary) = self.selection.primary() else {
            ui.label(Message::SelectNodeInTree.text(crate::language(ui.ctx())));
            return outcome;
        };
        if session.project().parent_of(primary).is_some() {
            self.placement_card.show(ui, session, primary);
            self.pivot_card.show(ui, session, primary);
        }
        if session.project().panel(primary).is_some() {
            let contour_target = self.contour_edit.as_ref().map(ContourEdit::target);
            let single_selection = self.selection.ids().len() == 1;
            let action = self.card.show(ui, session, Some(primary), contour_target, single_selection);
            match action {
                PanelAction::None => {}
                PanelAction::StartContour => {
                    self.start_contour_edit(session.project(), ContourTarget::Panel(primary));
                }
                PanelAction::StartCutoutContour { panel, machining } => {
                    self.start_contour_edit(session.project(), ContourTarget::Cutout { panel, machining });
                }
            }
        } else if session.project().object(primary).is_some() {
            let single_selection = self.selection.ids().len() == 1;
            let contour_editing = self.contour_edit.as_ref().is_some_and(|edit| edit.target().id() == primary);
            let action =
                self.object_card.show(ui, session, primary, self.mesh_edit.as_ref(), contour_editing, single_selection);
            match action {
                ObjectAction::None => {}
                ObjectAction::Toggle => {
                    if self.mesh_edit.is_some() {
                        self.end_mesh_edit();
                    } else {
                        self.start_mesh_edit(session.project());
                    }
                }
                ObjectAction::Element(element) => self.set_mesh_element(element),
                ObjectAction::StartContour => {
                    let target = if session.project().object_path(primary).is_ok() {
                        ContourTarget::Path(primary)
                    } else {
                        ContourTarget::Object(primary)
                    };
                    self.start_contour_edit(session.project(), target);
                }
            }
        } else if session.project().camera(primary).is_some() {
            self.camera_card.show(ui, session, primary);
        } else if session.project().light(primary).is_some() {
            self.light_card.show(ui, session, primary);
        } else if session.project().is_container(primary) {
            if self.container_card.show(ui, session, primary) == ContainerAction::AddChild {
                self.arm_creation(session.project(), primary);
            }
        } else {
            ui.label(Message::SelectNodeInTree.text(crate::language(ui.ctx())));
        }
        self.properties_card.show(ui, session, primary);
        outcome
    }

    /// Runs one entry of the node's action list, from the tree row or the viewport popup.
    ///
    /// A locked node refuses every edit that removes or restructures it here, before the dispatch,
    /// and a document edit is one command with one undo entry while a view entry writes session
    /// state only (ADR 0065, ADR 0115).
    ///
    /// One entry of four — the transform mode — is the window's and not this tab's: it is recorded
    /// for the shell to take ([`Editor::take_mode_request`]), because the mode every tab shares
    /// cannot live in one tab's editor (U113).
    pub fn apply_node_action(
        &mut self,
        session: &mut Session,
        action: NodeAction,
        lang: crate::Lang,
    ) -> Result<(), String> {
        // A locked node refuses every edit that removes or restructures it. The guard is checked
        // before the dispatch, because it is a guard on a gesture and not a document rule
        // (ADR 0065).
        let targets: &[Id] = match &action {
            NodeAction::Delete { node } | NodeAction::Ungroup { node } | NodeAction::ConvertToGroup { node } => {
                std::slice::from_ref(node)
            }
            NodeAction::Group { nodes } => nodes,
            _ => &[],
        };
        for node in targets {
            if let Some(refusal) = self.locked_refusal(session.project(), *node, lang) {
                return Err(refusal);
            }
        }
        match action {
            NodeAction::AddChild { parent } => {
                self.arm_creation(session.project(), parent);
            }
            NodeAction::OpenProduct { node } => {
                self.open_product(session, node);
            }
            NodeAction::ArmVoid { product } => {
                // Opening first is what gives the tool a product to point into; the arm itself is
                // view state and writes nothing (ADR 0098, 0118).
                if self.open_product(session, product) {
                    self.arm_void();
                }
            }
            NodeAction::CloseProduct => self.close_product(session),
            NodeAction::Duplicate { node } => {
                let params = json(&NodeDuplicate { node })?;
                let outcome = dispatch(session, NODE_DUPLICATE_ID, &params)?;
                if let Some(id) = outcome.created_id() {
                    self.selection.replace(id);
                }
            }
            NodeAction::Delete { node } => {
                let parent = session.project().parent_of(node);
                let params = json(&NodeDelete { node })?;
                dispatch(session, NODE_DELETE_ID, &params)?;
                self.after_node_removed(session, parent);
            }
            NodeAction::Group { nodes } => {
                let params = json(&NodeGroup { nodes, name: Message::GeneratedGroup.text(lang).to_owned() })?;
                let outcome = dispatch(session, NODE_GROUP_ID, &params)?;
                if let Some(id) = outcome.created_id() {
                    self.selection.replace(id);
                }
            }
            NodeAction::Ungroup { node } => {
                let parent = session.project().parent_of(node);
                let params = json(&NodeUngroup { node })?;
                dispatch(session, NODE_UNGROUP_ID, &params)?;
                self.after_node_removed(session, parent);
            }
            NodeAction::ConvertToGroup { node } => {
                let parent = session.project().parent_of(node);
                let params = json(&NodeComponentConvertToGroup { node })?;
                dispatch(session, NODE_COMPONENT_CONVERT_TO_GROUP_ID, &params)?;
                self.after_node_removed(session, parent);
            }
            NodeAction::SetHidden { node, hidden } => {
                self.flags.set_hidden(node, hidden);
            }
            NodeAction::SetLocked { node, locked } => {
                self.flags.set_locked(node, locked);
            }
            NodeAction::Isolate { node } => {
                self.flags.isolate(session.project(), node);
            }
            NodeAction::ShowAll => self.flags.show_all(),
            // View state with no document behind it, so it leaves no revision and no undo entry:
            // the shell writes the mode after the menus have drawn.
            NodeAction::Mode(mode) => self.mode_request = Some(mode),
        }
        Ok(())
    }

    fn after_node_removed(&mut self, session: &Session, parent: Option<Id>) {
        self.selection.retain_existing(session.project());
        self.retain_open(session.project());
        self.retain_flags(session.project());
        self.retain_creation_parent(session.project());
        if self.selection.primary().is_none()
            && let Some(parent) = parent
            && session.project().contains_node(parent)
        {
            self.selection.replace(parent);
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;
    use engine::Session;
    use model::{Geometry, Placement};
    use testkit::{cube_mesh, millimetres, sample_catalog, sample_product_project};

    /// A session with one of each kind the inspector picks a card for, and their ids.
    ///
    /// The sample project already holds a panel and a component; the cube and the frame are added
    /// beside them, so the three cards are drawn from one document.
    fn three_kinds() -> (Session, Id, Id, Id) {
        let (mut project, product) = sample_product_project();
        let root = project.root().id();
        let cube = project
            .add_object(root, "Cube", Placement::IDENTITY, Geometry::Mesh(cube_mesh(millimetres(100))))
            .expect("the sample project takes a mesh object");
        let panel = project.root().children()[0].id();
        (Session::with_directory(project, sample_catalog()), panel, cube, product)
    }

    /// Draws the inspector for one selection and answers the texts it painted.
    fn inspector_texts(editor: &mut Editor, session: &mut Session, id: Id) -> Vec<String> {
        editor.selection.replace(id);
        let ctx = egui::Context::default();
        theme::install(&ctx);
        crate::set_language(&ctx, crate::Lang::En);
        let output = ctx.run_ui(Default::default(), |ui| {
            let _ = editor.show_card(ui, session, None);
        });
        let texts = drawn_text(&output.shapes);
        output.drop_without_applying_deltas();
        texts
    }

    fn drew(texts: &[String], needle: &str) -> bool {
        texts.iter().any(|text| text.contains(needle))
    }

    fn drawn_text(shapes: &[egui::epaint::ClippedShape]) -> Vec<String> {
        use egui::epaint::Shape;
        fn walk(shape: &Shape, out: &mut Vec<String>) {
            match shape {
                Shape::Text(text) => out.push(text.galley.text().to_owned()),
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
    fn the_inspector_draws_the_card_of_the_selected_kind_and_no_other() {
        let (mut session, panel, cube, product) = three_kinds();
        assert!(session.project().panel(panel).is_some(), "the fixture holds a panel");
        assert!(session.project().container(product).is_some_and(|c| c.is_product()), "and a product frame");
        let mut editor = Editor::new();

        let panel_card = inspector_texts(&mut editor, &mut session, panel);
        assert!(drew(&panel_card, "Dimensions"), "a panel is measured by its dimensions: {panel_card:?}");
        assert!(drew(&panel_card, "Material"), "and by its material: {panel_card:?}");
        assert!(!drew(&panel_card, "Edit mesh"), "a panel has no mesh mode: {panel_card:?}");
        assert!(!drew(&panel_card, "Collapse to mesh"), "and nothing to bake: {panel_card:?}");
        assert!(!drew(&panel_card, "Vertices"), "the object card did not draw either: {panel_card:?}");

        let object_card = inspector_texts(&mut editor, &mut session, cube);
        assert!(drew(&object_card, "Edit mesh"), "a mesh object is edited element by element: {object_card:?}");
        assert!(drew(&object_card, "Vertices"), "and its measures are drawn in the head: {object_card:?}");
        assert!(!drew(&object_card, "Dimensions"), "the object card is not the panel card: {object_card:?}");
        assert!(!drew(&object_card, "Create inside"), "nor is it the container card: {object_card:?}");

        let product_card = inspector_texts(&mut editor, &mut session, product);
        assert!(drew(&product_card, "Container"), "a product frame is a container: {product_card:?}");
        assert!(drew(&product_card, "Create inside"), "and the card offers to fill it: {product_card:?}");
        assert!(!drew(&product_card, "Edit mesh"), "a frame is not an object: {product_card:?}");
        assert!(!drew(&product_card, "Dimensions"), "and it is not a panel: {product_card:?}");

        // One inspector is drawn per frame, so the three selections never bleed into each other.
        assert!(!drew(&object_card, "Container"), "the object card drew a container section: {object_card:?}");
        assert!(!drew(&panel_card, "Edit mesh"), "{panel_card:?}");
    }
}
