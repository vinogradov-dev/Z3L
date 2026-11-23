//! The menu a right click opens while a polyline is being drawn.
//!
//! A polyline keeps its chain, so the right button cannot finish it the way it finishes a fixed
//! figure. The button opens this menu instead: it chooses whether the next segment is straight or an
//! arc, the switch that used to sit on Tab before Tab became the point's own key, and it offers the
//! finish the armed tool's card draws as its check (ADR 0126, 0128, U115).
//!
//! It is a surface of its own rather than a row of the node context menu: that list is about a node
//! under the pointer, and a running draft names no node. The popup, its rectangle and its dismissal
//! are the shell's chrome: a press inside is never a pick and a press outside closes it, exactly as
//! the node menu behaves (U92, ADR 0080).
//!
//! Changes when: a draft gains a choice, or the popup's rows and dismissal change.

use egui::{CornerRadius, Margin, Order, Pos2, Rect, Stroke};

use crate::{Message, language, theme};

/// The width of the popup, in points.
const MENU_WIDTH: f32 = 168.0;

/// What the person chose in a running draft's menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DraftMenuChoice {
    /// The next segment is straight.
    Line,
    /// The next segment is an arc through the point under the pointer.
    Arc,
    /// Commit the chain as it stands.
    Finish,
}

/// The right-click menu of a running polyline, its place and the rectangle it drew last frame.
#[derive(Clone, Debug, Default)]
pub struct DraftMenu {
    open: Option<Pos2>,
    rect: Option<Rect>,
}

impl DraftMenu {
    /// Opens the menu at `position`, in viewport points.
    pub fn open(&mut self, position: Pos2) {
        self.open = Some(position);
        self.rect = None;
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

    /// The rectangle the open menu drew last frame, which is the one on screen while it stays up.
    pub fn rect(&self) -> Option<Rect> {
        self.rect
    }

    /// Draws the open menu over `viewport` and returns the choice this frame.
    ///
    /// `arc` is the kind the next segment already is, so the menu can name it as the current one
    /// instead of offering a press that changes nothing; `can_finish` is the draft's own rule,
    /// so a finish the command would refuse is drawn refused.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        viewport: Rect,
        arc: bool,
        can_finish: bool,
        opacity: u32,
    ) -> Option<DraftMenuChoice> {
        let Some(position) = self.open else {
            self.rect = None;
            return None;
        };
        if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.close();
            return None;
        }
        let mut chosen = None;
        let fill = theme::at_opacity(theme::floating(ui), opacity.clamp(80, 100));
        let response = egui::Area::new(egui::Id::unique("draft_menu"))
            .order(Order::Middle)
            .fixed_pos(position)
            .constrain_to(viewport)
            .show(ui.ctx(), |ui| {
                egui::Frame::new()
                    .fill(fill)
                    .stroke(Stroke::new(1.0, theme::line(ui)))
                    .corner_radius(CornerRadius::same(theme::RADIUS))
                    .inner_margin(Margin::symmetric(6, 4))
                    .show(ui, |ui| {
                        ui.set_min_width(MENU_WIDTH);
                        chosen = rows(ui, arc, can_finish);
                    });
            })
            .response;
        self.rect = Some(response.rect);
        if chosen.is_some() {
            self.close();
        }
        chosen
    }
}

/// The menu's rows, in the order a person reads them.
fn rows(ui: &mut egui::Ui, arc: bool, can_finish: bool) -> Option<DraftMenuChoice> {
    let lang = language(ui.ctx());
    let mut chosen = None;
    for (label, is_arc) in [(Message::DraftSegmentLine, false), (Message::DraftSegmentArc, true)] {
        // The current kind is drawn disabled with its sentence: a live row that does nothing when
        // pressed teaches the person nothing.
        let current = arc == is_arc;
        let response = ui.add_enabled(
            !current,
            egui::Button::new(egui::RichText::new(label.text(lang))).min_size(egui::vec2(MENU_WIDTH, 0.0)),
        );
        let response = if current { response.on_hover_text(Message::DraftSegmentCurrent.text(lang)) } else { response };
        if response.clicked() {
            chosen = Some(if is_arc { DraftMenuChoice::Arc } else { DraftMenuChoice::Line });
        }
    }
    ui.separator();
    let finish = ui.add_enabled(
        can_finish,
        egui::Button::new(egui::RichText::new(Message::DraftFinish.text(lang))).min_size(egui::vec2(MENU_WIDTH, 0.0)),
    );
    let finish = if can_finish { finish } else { finish.on_hover_text(Message::DraftNeedsTwo.text(lang)) };
    if finish.clicked() {
        chosen = Some(DraftMenuChoice::Finish);
    }
    chosen
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use egui::{Event, PointerButton, RawInput, Rect, pos2};

    use super::*;
    use crate::{Lang, set_language};

    const VIEWPORT: Rect = Rect { min: pos2(0.0, 0.0), max: pos2(800.0, 600.0) };

    fn ctx() -> egui::Context {
        let ctx = egui::Context::default();
        set_language(&ctx, Lang::En);
        ctx
    }

    /// Draws one frame of the menu and reports the choice it produced.
    fn frame(
        ctx: &egui::Context,
        menu: &mut DraftMenu,
        arc: bool,
        can_finish: bool,
        events: Vec<Event>,
    ) -> Option<DraftMenuChoice> {
        let input = RawInput { screen_rect: Some(VIEWPORT), events, ..Default::default() };
        let mut chosen = None;
        let output = ctx.run_ui(input, |ui| {
            chosen = menu.show(ui, VIEWPORT, arc, can_finish, 100);
        });
        output.drop_without_applying_deltas();
        chosen
    }

    /// Every text the last frame drew with the rectangle it took, so a test presses a row by name.
    fn drawn(ctx: &egui::Context, menu: &mut DraftMenu, arc: bool) -> Vec<(String, Rect)> {
        fn walk(shape: &egui::Shape, out: &mut Vec<(String, Rect)>) {
            match shape {
                egui::Shape::Text(text) => {
                    out.push((text.galley.text().to_owned(), Rect::from_min_size(text.pos, text.galley.size())));
                }
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        walk(shape, out);
                    }
                }
                _ => {}
            }
        }
        let input = RawInput { screen_rect: Some(VIEWPORT), ..Default::default() };
        let output = ctx.run_ui(input, |ui| {
            let _ = menu.show(ui, VIEWPORT, arc, true, 100);
        });
        let mut texts = Vec::new();
        for clipped in &output.shapes {
            walk(&clipped.shape, &mut texts);
        }
        output.drop_without_applying_deltas();
        texts
    }

    #[test]
    fn the_menu_names_the_current_kind_and_finishes_the_chain() {
        let ctx = ctx();
        let mut menu = DraftMenu::default();
        menu.open(pos2(200.0, 200.0));
        // The first frame lays the popup out, so the rows are read after it settles.
        frame(&ctx, &mut menu, false, true, Vec::new());
        assert!(menu.is_open(), "the menu stays up until a choice or a dismissal");
        assert!(menu.rect().is_some(), "the menu drew a rectangle the shell can keep");
        let labels = drawn(&ctx, &mut menu, false);
        let named = |needle: &str| labels.iter().any(|(line, _)| line.contains(needle));
        assert!(named("Straight segment"), "{labels:?}");
        assert!(named("Arc segment"), "{labels:?}");
        assert!(named("Finish the drawing"), "{labels:?}");
    }

    #[test]
    fn escape_and_a_choice_close_the_menu() {
        let ctx = ctx();
        let mut menu = DraftMenu::default();
        menu.open(pos2(10.0, 10.0));
        frame(&ctx, &mut menu, true, true, Vec::new());
        let escape = Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        assert_eq!(frame(&ctx, &mut menu, true, true, vec![escape]), None);
        assert!(!menu.is_open(), "Escape closes the menu");
        assert!(menu.rect().is_none(), "and drops the rectangle it drew");
    }

    #[test]
    fn a_press_on_the_arc_row_asks_for_an_arc() {
        let ctx = ctx();
        let mut menu = DraftMenu::default();
        menu.open(pos2(100.0, 100.0));
        // The first frame lays the popup out, so the rows are read from the frame after it.
        frame(&ctx, &mut menu, false, true, Vec::new());
        let rows = drawn(&ctx, &mut menu, false);
        let target = rows
            .iter()
            .find(|(line, _)| line.contains("Arc segment"))
            .map(|(_, rect)| rect.center())
            .expect("the arc row was drawn");
        let mut chosen = None;
        for pressed in [true, false] {
            let input = RawInput {
                screen_rect: Some(VIEWPORT),
                events: vec![
                    Event::PointerMoved(target),
                    Event::PointerButton {
                        pos: target,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                ..Default::default()
            };
            let output = ctx.run_ui(input, |ui| {
                chosen = menu.show(ui, VIEWPORT, false, true, 100).or(chosen);
            });
            output.drop_without_applying_deltas();
        }
        assert_eq!(chosen, Some(DraftMenuChoice::Arc), "the arc row asks for an arc");
        assert!(!menu.is_open(), "and the choice closes the menu");
    }
}
