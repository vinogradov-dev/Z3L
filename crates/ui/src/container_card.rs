//! The selected-container card.
//!
//! A container is either an ordinary group or a furniture component. The difference is data, not a
//! separate node kind: a component carries a non-zero `class`, a `prototype.id` or
//! `component = true`. Both own children and are renamed through `node.rename`. Grouping and
//! ungrouping live in the scene tree so the same action covers several selected nodes.
//!
//! The card is also where the arm that receives the next creation lives: a container is armed from
//! here and disarmed from here, so the chip that named it in the status bar is gone and the state
//! belongs to the card of the node it acts on (U115, ADR 0056).

use egui::{RichText, TextEdit, Ui};
use engine::node_cmds::NODE_RENAME_ID;
use engine::{NodeRename, Session};
use model::{Id, Project};
use text::Message;

use crate::command::{dispatch, json};
use crate::section::section;
use crate::theme;

/// Persistent state of the container card.
#[derive(Debug, Default)]
pub(crate) struct ContainerCard {
    draft: Option<Draft>,
    error: Option<String>,
}

#[derive(Clone, Debug)]
struct Draft {
    container: Id,
    name: String,
}

/// What the container card asks the editor to do beyond the edits it dispatches itself.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ContainerAction {
    /// Nothing was pressed.
    #[default]
    None,
    /// Arm this container so the next created node lands inside it.
    AddChild,
    /// Disarm this container, so the next created node lands at the scene root again.
    ClearChild,
}

/// A container's role for display.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ContainerKind {
    Component,
    Group,
}

impl ContainerKind {
    const fn label(self) -> Message {
        match self {
            Self::Component => Message::ContainerRoleComponent,
            Self::Group => Message::ContainerRoleGroup,
        }
    }

    const fn description(self) -> Message {
        match self {
            Self::Component => Message::ContainerComponentDescription,
            Self::Group => Message::ContainerGroupDescription,
        }
    }
}

fn container_summary(project: &Project, id: Id) -> Option<(ContainerKind, &str, usize)> {
    let container = project.container(id)?;
    let kind = if project.is_component(id) { ContainerKind::Component } else { ContainerKind::Group };
    Some((kind, container.name(), container.children().len()))
}

impl ContainerCard {
    /// Draws the card for the selected container and dispatches accepted edits.
    ///
    /// Returns the creation request separately: arming a container is editor view state, so the
    /// card asks the editor to hold it instead of touching the document.
    ///
    /// `armed` is the editor's view state: this container is the one the next creation lands in, so
    /// the card offers to disarm it instead of arming it again (U115).
    pub(crate) fn show(
        &mut self,
        ui: &mut Ui,
        session: &mut Session,
        container_id: Id,
        armed: bool,
    ) -> ContainerAction {
        let Some((kind, name, children)) = container_summary(session.project(), container_id) else {
            self.draft = None;
            self.error = None;
            ui.label(Message::SelectNodeInTree.text(crate::language(ui.ctx())));
            return ContainerAction::None;
        };
        let name = name.to_owned();
        if self.draft.as_ref().is_none_or(|draft| draft.container != container_id) {
            self.error = None;
            self.draft = Some(Draft { container: container_id, name: name.clone() });
        }
        let is_root = container_id == session.project().root().id();
        let Some(draft) = self.draft.as_mut() else {
            return ContainerAction::None;
        };

        let mut action = ContainerAction::None;
        let mut rename = None;
        let mut name_error = None;
        ui.separator();
        section(ui, "container.name", Message::SectionName.text(crate::language(ui.ctx())), true, |ui| {
            ui.horizontal(|ui| {
                let response = ui.add(TextEdit::singleline(&mut draft.name).desired_width(180.0));
                let clicked = ui.button(Message::CmdRename.text(crate::language(ui.ctx()))).clicked();
                if response.lost_focus() || clicked {
                    let trimmed = draft.name.trim();
                    if trimmed.is_empty() {
                        name_error = Some(
                            Message::NameMustNotBeEmpty
                                .with(crate::language(ui.ctx()), [kind.label().text(crate::language(ui.ctx()))]),
                        );
                    } else if trimmed != name {
                        rename = Some(trimmed.to_owned());
                    }
                }
            });
        });

        ui.separator();
        let lang = crate::language(ui.ctx());
        section(ui, "container.summary", Message::SectionContainer.text(lang), true, |ui| {
            ui.label(RichText::new(kind.label().text(lang)).strong());
            ui.label(if is_root {
                Message::RootDescription.text(lang).to_owned()
            } else {
                kind.description().text(lang).to_owned()
            });
            ui.label(Message::ChildrenCount.with(lang, [children]));
        });

        if !is_root {
            ui.separator();
            let lang = crate::language(ui.ctx());
            section(ui, "container.creation", Message::SectionCreateInside.text(lang), true, |ui| {
                if armed {
                    // The arm is sticky across creations, so the card says where the next node lands
                    // and offers the one way out of it: a container armed much earlier must never
                    // keep receiving parts without a place that says so (ADR 0056, U115).
                    ui.label(Message::AddingTo.with(lang, [&name]));
                    if ui
                        .button(Message::CreateAtRoot.text(lang))
                        .on_hover_text(Message::CreateAtRootHint.text(lang))
                        .clicked()
                    {
                        action = ContainerAction::ClearChild;
                    }
                } else if ui
                    .button(Message::TreeAddChild.text(lang))
                    .on_hover_text(Message::AddChildHint.text(lang))
                    .clicked()
                {
                    action = ContainerAction::AddChild;
                }
            });
        }

        if let Some(error) = name_error {
            self.error = Some(error);
        }
        if let Some(new_name) = rename {
            match rename_container(session, container_id, new_name) {
                Ok(()) => {
                    self.error = None;
                    self.draft = None;
                }
                Err(error) => self.error = Some(error),
            }
            ui.ctx().request_repaint();
        }

        if let Some(error) = &self.error {
            ui.separator();
            ui.colored_label(theme::danger(ui), error);
        }
        action
    }
}

fn rename_container(session: &mut Session, node: Id, name: String) -> Result<(), String> {
    let params = json(&NodeRename { node, name })?;
    dispatch(session, NODE_RENAME_ID, &params).map(|_| ())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use testkit::{sample_catalog, sample_project};

    use super::*;

    /// The middle of the accessibility node labelled `label`, so a test can aim at a button.
    fn node_center(output: &egui::FullOutput, label: &str) -> Option<egui::Pos2> {
        let update = output.platform_output.accesskit_update.as_ref()?;
        update.nodes.iter().find_map(|(_, node)| {
            if node.label() != Some(label) {
                return None;
            }
            let bounds = node.bounds()?;
            Some(egui::pos2(((bounds.x0 + bounds.x1) / 2.0) as f32, ((bounds.y0 + bounds.y1) / 2.0) as f32))
        })
    }

    #[test]
    fn the_container_card_arms_and_disarms_the_next_creation() {
        // U115: the chip and the cross that cleared it left the status bar, so the card of the node
        // the arm acts on holds both halves — *Add child* arms it, and an armed container names
        // itself and offers the way back to the scene root (ADR 0056).
        for (armed, button, expected) in [
            (false, Message::TreeAddChild, ContainerAction::AddChild),
            (true, Message::CreateAtRoot, ContainerAction::ClearChild),
        ] {
            let ctx = egui::Context::default();
            crate::theme::install(&ctx);
            ctx.enable_accesskit();
            crate::set_language(&ctx, crate::Lang::En);
            let mut session = Session::with_directory(sample_project(), sample_catalog());
            let container = Id::new(3).expect("container id");
            let mut card = ContainerCard::default();
            let mut action = ContainerAction::None;
            let mut asked = None;
            let mut at = None;
            let mut texts = Vec::new();
            for frame in 0..5 {
                let press = |pressed| egui::Event::PointerButton {
                    pos: at.unwrap_or_default(),
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                };
                let events = match (frame, at) {
                    (0, _) | (_, None) => Vec::new(),
                    (1, Some(at)) => vec![egui::Event::PointerMoved(at)],
                    (2, Some(_)) => vec![press(true)],
                    (3, Some(_)) => vec![press(false)],
                    _ => Vec::new(),
                };
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 700.0))),
                        events,
                        ..Default::default()
                    },
                    |ui| action = card.show(ui, &mut session, container, armed),
                );
                if action != ContainerAction::None {
                    asked = Some(action);
                }
                if frame == 0 {
                    at = node_center(&output, button.text(crate::Lang::En));
                    assert!(at.is_some(), "the card draws `{}`", button.text(crate::Lang::En));
                }
                texts = Vec::new();
                collect_shape_text(&output.shapes, &mut texts);
                output.drop_without_applying_deltas();
            }
            if armed {
                let named = Message::AddingTo.with(crate::Lang::En, ["Drawer"]);
                assert!(texts.contains(&named), "the armed container names itself: {texts:?}");
            }
            assert_eq!(asked, Some(expected), "`{}` asks for its arm", button.text(crate::Lang::En));
        }
    }

    /// Every text the frame painted.
    fn collect_shape_text(shapes: &[egui::epaint::ClippedShape], out: &mut Vec<String>) {
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
        for clipped in shapes {
            walk(&clipped.shape, out);
        }
    }

    #[test]
    fn renaming_a_component_dispatches_through_the_engine() {
        let mut session = Session::with_directory(sample_project(), sample_catalog());
        let container = Id::new(3).expect("container id");
        rename_container(&mut session, container, "Drawer box".to_owned()).expect("rename");
        assert_eq!(session.project().container(container).expect("container").name(), "Drawer box");
        assert_eq!(container_summary(session.project(), container).expect("summary").0, ContainerKind::Component);
        session.undo().expect("undo");
        assert_eq!(session.project().container(container).expect("container").name(), "Drawer");
    }

    #[test]
    fn an_unmarked_container_shows_as_a_group() {
        let session = Session::with_directory(sample_project(), sample_catalog());
        let root = session.project().root().id();
        assert_eq!(container_summary(session.project(), root).expect("summary").0, ContainerKind::Group);
    }

    #[test]
    fn renaming_the_root_container_dispatches_through_the_engine() {
        let mut session = Session::with_directory(sample_project(), sample_catalog());
        let root = session.project().root().id();
        rename_container(&mut session, root, "Workshop".to_owned()).expect("rename root");
        assert_eq!(session.project().root().name(), "Workshop");
        assert_eq!(session.project().container(root).expect("container").name(), "Workshop");
    }
}
