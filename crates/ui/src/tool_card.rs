//! The parameters of an armed tool: one small plate at the pointer, the same rows in the inspector.
//!
//! An armed tool that asks for a number shows it here (ADR 0119): a `Middle`-layer plate beside the
//! pointer, one row per parameter, each row the shared scrubbing field of ADR 0092 with its unit
//! *outside* the box. The exact-distance fields of a niche keep their own placement beside the gap
//! they measure, but they draw through the same [row](row), so the two surfaces share one layout, one
//! commit path and one edit scope.
//!
//! The inspector draws the same rows from the same draft: the shell builds one [`ToolView`] and both
//! surfaces read it, so the number at the hand and the number in the panel cannot drift apart. How
//! the operation's one commit is offered is the tool's choice ([`ToolCommit`]): the ✓ and ✕ pair, the
//! gesture's own Enter and Escape with a lettered plate that draws no button, or neither, for a tool
//! that commits on its own click. A fresh card's field takes the keyboard so its number can be typed
//! without a click; the shell owns the count of frames it asks for it.
//!
//! Nothing here writes a document. The surface reports the numbers its fields committed, the button
//! pressed this frame, the field that took Enter and the rectangle it drew, which the shell hands to
//! `Chrome`, so a press inside the card is chrome and never a pick or a camera gesture (ADR 0080).
//! The shell still owns Escape, Enter and Tab for the running tool — `handle_keys` reads them before
//! the frame and the card only reports what its own focused field saw — so the card adds no second
//! key reader (ADR 0076).

use core::ops::RangeInclusive;

use egui::{Align2, CornerRadius, Id, Margin, Order, Pos2, Rect, RichText, Sense, Stroke, Ui};

use crate::icons;
use crate::scrub::Scrub;
use crate::theme;
use crate::{FieldLabel, Message, language};

/// The clearance between the pointer and the card's nearest corner, in points.
pub const CARD_GAP: f32 = 18.0;

/// The identity of one number an armed tool shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ToolParam {
    /// The height a pull sets: a box's height or an extrusion's.
    Height,
    /// The gap between the part's low face and the wall that stops it.
    LowGap,
    /// The gap between the part's high face and the wall that stops it.
    HighGap,
}

/// One row of the card: a name, a number and the range the shared field keeps it inside.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolRow {
    /// The parameter the row edits.
    pub param: ToolParam,
    /// What the row writes before its number; a tool that names its one number in the title
    /// carries none.
    pub label: Option<FieldLabel>,
    /// The number the field shows, in `unit`.
    pub value: f64,
    /// The unit, drawn dim outside the box.
    pub unit: &'static str,
    /// The range the field keeps the number inside.
    pub range: RangeInclusive<f64>,
    /// The digits after the decimal point; `0` draws a whole number.
    pub decimals: usize,
    /// Enter locked this face, so the row draws the lock glyph.
    pub locked: bool,
    /// The direction has no wall, so the row shows a dash and takes no number.
    pub disabled: bool,
    /// Whether the row's menu offers *Centred*, which only a measured gap can honour.
    pub centre: bool,
}

impl ToolRow {
    /// A row for the height a pull sets: the tool's name is the card's title, so the row has none.
    pub fn height(value: f64, range: RangeInclusive<f64>, decimals: usize) -> Self {
        Self {
            param: ToolParam::Height,
            label: None,
            value,
            unit: "mm",
            range,
            decimals,
            locked: false,
            disabled: false,
            centre: false,
        }
    }
}

/// What both surfaces draw: the tool's name, its numbers and how its commit is offered.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolView {
    /// The tool's name, drawn above the rows when it has more than one.
    pub title: Option<Message>,
    /// The numbers, in the order the tool declares them.
    pub rows: Vec<ToolRow>,
    /// How the surface offers the tool's one commit.
    pub commit: ToolCommit,
}

/// How the tool's commit is offered to the hand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolCommit {
    /// The ✓ and ✕ pair under the rows, beside the gesture's own Enter and Escape.
    Pair,
    /// The keys alone: the card draws no button, so its field is the only control it holds and the
    /// gesture's Enter commits and Escape cancels (U104).
    Keys,
    /// Neither: the tool commits on its own gesture, like a fill's click.
    Gesture,
}

impl ToolView {
    /// A tool with one number, named by its title.
    pub fn height(title: Message, value: f64, range: RangeInclusive<f64>, decimals: usize) -> Self {
        Self { title: Some(title), rows: vec![ToolRow::height(value, range, decimals)], commit: ToolCommit::Pair }
    }

    /// A tool whose one number is named by a symbol and nothing else.
    ///
    /// This is the plate rule of the sketch fields: `H` in front of the number says what it is, so
    /// the plate carries no word, no unit and no button pair — the gesture's own Enter commits and
    /// its Escape cancels, and the field is the only control that takes the keyboard (U104).
    pub fn lettered_height(symbol: &'static str, value: f64, range: RangeInclusive<f64>, decimals: usize) -> Self {
        let mut row = ToolRow::height(value, range, decimals);
        row.label = Some(FieldLabel::Symbol(symbol));
        row.unit = "";
        Self { title: None, rows: vec![row], commit: ToolCommit::Keys }
    }

    /// Whether the surface draws anything at all.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

/// What one row's field committed: the parameter it edits and the number it now holds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToolEdit {
    /// The parameter the field edits.
    pub param: ToolParam,
    /// The number the field committed.
    pub value: f64,
}

/// The button the card's pair reported this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolAction {
    /// ✓: the caller commits the operation, once.
    Apply,
    /// ✕: the caller drops the preview and writes nothing.
    Cancel,
}

/// What one frame of the card, or of the inspector's section, asked for.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ToolOutcome {
    /// The numbers to take, in the order the fields committed them.
    pub edits: Vec<ToolEdit>,
    /// The row whose lock glyph was pressed.
    pub lock: Option<ToolParam>,
    /// The row whose menu asked to centre the part.
    pub centre: Option<ToolParam>,
    /// The button pressed this frame, if any.
    pub action: Option<ToolAction>,
    /// The sentence a refused number left.
    pub error: Option<Message>,
    /// The row that took the keyboard, so the caller keeps the focus on one parameter.
    pub focused: Option<ToolParam>,
}

impl ToolOutcome {
    /// Whether the frame asked to move the part or its number.
    pub fn changed(&self) -> bool {
        !self.edits.is_empty() || self.lock.is_some() || self.centre.is_some()
    }
}

/// Draws the card near `pointer` and reports what it asked for and the rectangle it took.
///
/// The plate is kept inside `viewport`, so a pointer at the edge of the work area leaves the whole
/// card on screen instead of clipping it. `focus` names the row that takes the keyboard this frame,
/// for a tool that appears with a number ready to type.
pub fn show(
    ui: &mut Ui,
    viewport: Rect,
    pointer: Pos2,
    view: &mut ToolView,
    focus: Option<ToolParam>,
    opacity: u32,
) -> (ToolOutcome, Rect) {
    let fill = theme::at_opacity(theme::floating(ui), opacity.clamp(80, 100));
    let anchor = pointer + egui::vec2(CARD_GAP, CARD_GAP);
    let mut outcome = ToolOutcome::default();
    let rect = egui::Area::new(Id::unique("tool_card"))
        .order(Order::Middle)
        .pivot(Align2::LEFT_TOP)
        .fixed_pos(anchor)
        .constrain_to(viewport)
        .show(ui.ctx(), |ui| {
            let frame = egui::Frame::new()
                .fill(fill)
                .stroke(Stroke::new(1.0, theme::line(ui)))
                .corner_radius(CornerRadius::same(theme::RADIUS))
                .inner_margin(Margin::symmetric(8, 6));
            frame.show(ui, |ui| {
                // The card floats over the viewport, so it may never grow wider than the surface it
                // covers: a narrow window folds the rows instead of clipping them.
                ui.set_max_width((viewport.width() - 2.0 * CARD_GAP).max(0.0));
                outcome = body(ui, view, focus);
            });
        })
        .response
        .rect;
    (outcome, rect)
}

/// Draws the same rows inside the inspector, under the tool's name, and separates them from the
/// node's card below.
///
/// It draws no plate: the floating panel is the surface, and a second frame inside it would read as
/// a card within a card.
pub fn section(ui: &mut Ui, view: &mut ToolView, focus: Option<ToolParam>) -> ToolOutcome {
    let outcome = body(ui, view, focus);
    theme::divider(ui);
    ui.add_space(2.0);
    outcome
}

/// Draws the title, the rows and the button pair, and reports what they asked for.
fn body(ui: &mut Ui, view: &mut ToolView, focus: Option<ToolParam>) -> ToolOutcome {
    let lang = language(ui.ctx());
    let mut outcome = ToolOutcome::default();
    // A tool with one number is named once, in the title: repeating it as a row label would be the
    // same word twice in a 100 pt plate.
    let titled = view.title.is_some() && (view.rows.len() > 1 || view.rows.iter().all(|row| row.label.is_none()));
    if titled && let Some(title) = view.title {
        ui.label(RichText::new(title.text(lang)).strong());
    }
    // A single row carries its own name when it has one: a lettered plate has nothing else to say
    // what the number is, and a titled one keeps the title as its name.
    let labels = view.rows.len() > 1 || view.rows.iter().any(|row| row.label.is_some());
    let mut entered = false;
    for row in &mut view.rows {
        entered |= row_ui(ui, row, labels, focus == Some(row.param), &mut outcome);
    }
    // Enter in a focused field is the tool's commit as much as the ✓ is, and it is read after the
    // field drew: the number it just committed is the number the tool applies (M71, U104).
    if entered && view.commit != ToolCommit::Gesture {
        outcome.action = Some(ToolAction::Apply);
    }
    if let Some(message) = outcome.error {
        ui.colored_label(theme::danger(ui), message.text(lang));
    }
    if view.commit == ToolCommit::Pair {
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            if icons::icon_button(ui, icons::actions::CONFIRM, Message::ApplyEnter.text(lang)).clicked() {
                outcome.action = Some(ToolAction::Apply);
            }
            if icons::icon_button(ui, icons::actions::CANCEL, Message::CancelEsc.text(lang)).clicked() {
                outcome.action = Some(ToolAction::Cancel);
            }
        });
    }
    outcome
}

/// Draws one row through the shared field and reports the number it committed this frame.
///
/// The answer is whether the row's field took Enter, which is the card's commit and not only a
/// number: a field reports both from one frame (M71, U104).
fn row_ui(ui: &mut Ui, row: &mut ToolRow, labels: bool, focus: bool, outcome: &mut ToolOutcome) -> bool {
    let lang = language(ui.ctx());
    let mut submitted = false;
    ui.horizontal(|ui| {
        if labels && let Some(label) = row.label {
            match label {
                FieldLabel::Word(message) => ui.label(message.text(lang)),
                // A symbol is notation, so it is never translated: it sits small and dim in front of
                // the number the way a sketch plate writes it (U104).
                FieldLabel::Symbol(symbol) => ui.label(RichText::new(symbol).small().color(theme::text_dim(ui))),
            };
        }
        if row.disabled {
            ui.label(RichText::new("—").monospace().color(theme::text_dim(ui)));
            ui.label(RichText::new(row.unit).small().color(theme::text_dim(ui)));
            ui.label(RichText::new(Message::DistanceNoNeighbour.text(lang)).small().color(theme::text_dim(ui)));
            return;
        }
        if row.locked {
            let (rect, lock) =
                ui.allocate_exact_size(egui::vec2(icons::ICON_POINT_SIZE, icons::ICON_POINT_SIZE), Sense::click());
            icons::draw_icon(ui, icons::actions::LOCK, rect, icons::ICON_POINT_SIZE, theme::text_dim(ui));
            let lock = lock.on_hover_text(Message::DistanceLocked.with(lang, [format!("{:.0}", row.value)]));
            if lock.clicked() {
                outcome.lock = Some(row.param);
            }
        }
        let mut value = row.value;
        let field = Scrub::new(&mut value)
            .speed(0.5)
            .range(row.range.clone())
            .decimals(row.decimals)
            .unit(row.unit)
            .focus(focus)
            .show(ui);
        if field.response.has_focus() {
            outcome.focused = Some(row.param);
        }
        if field.commit {
            // A number equal to the one on screen still commits: typing it pins the face the pointer
            // was driving, which is not the same as leaving the pointer in charge.
            row.value = value;
            outcome.edits.push(ToolEdit { param: row.param, value });
        }
        if let Some(message) = field.error {
            outcome.error = Some(message);
        }
        if field.submit {
            submitted = true;
        }
        if row.centre {
            field.response.context_menu(|ui| {
                if ui.button(Message::DistanceCentred.text(lang)).clicked() {
                    outcome.centre = Some(row.param);
                    ui.close();
                }
            });
        }
    });
    submitted
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    fn viewport() -> Rect {
        Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))
    }

    fn view() -> ToolView {
        ToolView::height(Message::BoxHeight, 120.0, 1.0..=10_000.0, 3)
    }

    /// Draws one card frame at `pointer` and returns what it asked for and the rectangle it took.
    fn run(events: Vec<egui::Event>, pointer: Pos2, scale: f32) -> (ToolOutcome, Rect, Vec<String>) {
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(scale);
        crate::set_language(&ctx, crate::Lang::En);
        let mut tool = view();
        let mut outcome = ToolOutcome::default();
        let mut rect = Rect::NOTHING;
        let mut texts = Vec::new();
        for _ in 0..2 {
            let input = egui::RawInput { screen_rect: Some(viewport()), events: events.clone(), ..Default::default() };
            let output = ctx.run_ui(input, |ui| {
                let shown = show(ui, viewport(), pointer, &mut tool, None, 92);
                outcome = shown.0;
                rect = shown.1;
            });
            texts = texts_of(&output);
            output.drop_without_applying_deltas();
        }
        (outcome, rect, texts)
    }

    #[test]
    fn a_lettered_plate_names_its_number_and_offers_no_button() {
        let view = ToolView::lettered_height("H", 40.0, 1.0..=10_000.0, 3);
        assert_eq!(view.title, None, "no word stands above the number");
        assert_eq!(view.commit, ToolCommit::Keys, "the gesture's own keys are the commit");
        let [row] = view.rows.as_slice() else { panic!("a lettered plate holds one field") };
        assert_eq!(row.label, Some(FieldLabel::Symbol("H")), "the symbol names the number");
        assert_eq!(row.unit, "", "and no unit repeats what the symbol says");
    }

    #[test]
    fn a_field_asked_for_the_keyboard_takes_it_and_keeps_it() {
        // A card asks on the two frames its area settles: `egui` creates the widget after the ask,
        // so the request of the first frame is lost. The counted ask is what makes a fresh pull
        // typeable, and the count is why the field never holds the keyboard after that (U104).
        let ctx = egui::Context::default();
        crate::set_language(&ctx, crate::Lang::En);
        let mut tool = view();
        let mut states = Vec::new();
        for frame in 0..4 {
            let mut focused = false;
            let focus = (frame < 2).then_some(ToolParam::Height);
            let output = ctx.run_ui(egui::RawInput { screen_rect: Some(viewport()), ..Default::default() }, |ui| {
                show(ui, viewport(), Pos2::new(200.0, 200.0), &mut tool, focus, 92);
                focused = ui.ctx().text_edit_focused();
            });
            output.drop_without_applying_deltas();
            states.push(focused);
        }
        assert_eq!(states, vec![false, true, true, true], "the ask lands on the second frame and is kept");
    }

    #[test]
    fn a_card_at_rest_asks_for_nothing() {
        let (outcome, rect, texts) = run(Vec::new(), Pos2::new(200.0, 200.0), 1.0);
        assert_eq!(outcome, ToolOutcome::default(), "drawing the card applies nothing");
        assert!(rect.width() > 0.0, "the card took a rectangle");
        assert!(texts.iter().any(|text| text == "Box height"), "the tool's name is on the card: {texts:?}");
        assert!(texts.iter().any(|text| text == "120"), "the number is in the field: {texts:?}");
        assert!(texts.iter().any(|text| text == "mm"), "the unit is outside the box: {texts:?}");
    }

    #[test]
    fn the_card_sits_beside_the_pointer_and_stays_inside_the_view() {
        for scale in [1.0_f32, 2.0] {
            let pointer = Pos2::new(200.0, 300.0);
            let (_, rect, _) = run(Vec::new(), pointer, scale);
            assert!(viewport().contains_rect(rect), "at {scale}x the whole card is on screen: {rect:?}");
            assert!(rect.left() >= pointer.x, "at {scale}x the card clears the pointer it follows");
            assert!(rect.top() >= pointer.y, "at {scale}x the card clears the pointer it follows");
        }
    }

    #[test]
    fn a_card_at_the_far_corner_is_pulled_back_into_the_view() {
        let pointer = Pos2::new(795.0, 595.0);
        let (_, rect, _) = run(Vec::new(), pointer, 1.0);
        assert!(viewport().contains_rect(rect), "the plate is constrained to the view it covers: {rect:?}");
    }

    #[test]
    fn a_disabled_row_says_why_and_takes_no_number() {
        let ctx = egui::Context::default();
        crate::set_language(&ctx, crate::Lang::En);
        let mut tool = ToolView {
            title: None,
            commit: ToolCommit::Gesture,
            rows: vec![ToolRow {
                param: ToolParam::LowGap,
                label: Some(FieldLabel::Word(Message::ToolGapLow)),
                value: 0.0,
                unit: "mm",
                range: 0.0..=600.0,
                decimals: 0,
                locked: false,
                disabled: true,
                centre: true,
            }],
        };
        let mut texts = Vec::new();
        let mut outcome = ToolOutcome::default();
        for _ in 0..2 {
            let output = ctx.run_ui(egui::RawInput { screen_rect: Some(viewport()), ..Default::default() }, |ui| {
                outcome = section(ui, &mut tool, None);
            });
            texts = texts_of(&output);
            output.drop_without_applying_deltas();
        }
        assert_eq!(outcome, ToolOutcome::default(), "a dash writes nothing");
        assert!(texts.iter().any(|text| text == "No surface in this direction"), "{texts:?}");
        assert!(texts.iter().any(|text| text == "—"), "{texts:?}");
    }

    /// Every text the frame painted.
    fn texts_of(output: &egui::FullOutput) -> Vec<String> {
        fn collect(shape: &egui::Shape, texts: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(text) => texts.push(text.galley.text().to_owned()),
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect(shape, texts);
                    }
                }
                _ => {}
            }
        }
        let mut texts = Vec::new();
        for clipped in &output.shapes {
            collect(&clipped.shape, &mut texts);
        }
        texts
    }
}
