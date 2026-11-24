//! The parameters of an armed tool: one small plate at the pointer, the same rows in the inspector.
//!
//! An armed tool that asks for a number shows it here (ADR 0119): a `Middle`-layer plate beside the
//! pointer, one row per parameter, each row the shared scrubbing field of ADR 0092 with its unit
//! *outside* the box. The exact-distance fields of a niche draw through the same [row](row), so the
//! two surfaces share one layout, one commit path and one edit scope.
//!
//! A carried piece has no card at all (N26, ADR 0161): its two gaps are dimensioned in the scene, and
//! each field is a [plate](plate) of its own sitting in the middle of the arrow that measures it. The
//! plate is the one row that takes the keyboard and never the pointer.
//!
//! The inspector draws the same rows from the same draft: the shell builds one [`ToolView`] and both
//! surfaces read it, so the number at the hand and the number in the panel cannot drift apart. How
//! the operation's one commit is offered is the tool's choice ([`ToolCommit`]): the ✓ and ✕ pair, the
//! gesture's own Enter and Escape with a lettered plate that draws no button, or neither, for a tool
//! that commits on its own click. A fresh card's field takes the keyboard so its number can be typed
//! without a click; the shell owns the count of frames it asks for it.
//!
//! A tool that asks for no number draws one row — its name, its one switch and its pair — so a
//! running draft or contour edit keeps its accept and its drop at the hand and the row that names
//! the tool is the row that holds the buttons (U115). That row is the whole card for those tools: a
//! plate of two rows would cover the work the hand is measuring.
//!
//! Nothing here writes a document. The surface reports the numbers its fields committed, the button
//! pressed this frame, the field that took Enter and the rectangle it drew, which the shell hands to
//! `Chrome` for a card, so a press inside one is chrome and never a pick or a camera gesture
//! (ADR 0080). A plate returns its own rectangle for the caller to know where it stands, but it is
//! not chrome: a press on it belongs to the scene behind it (X66).
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
    /// The height a pull sets: a box's height, an extrusion's or a cone's.
    Height,
    /// The radius of the top circle a cone's pull sets; zero is the sharp apex (ADR 0141).
    TopRadius,
    /// The gap between the part's low face and the wall that stops it.
    LowGap,
    /// The gap between the part's high face and the wall that stops it.
    HighGap,
    /// How many parts a row holds, when the tool lays a row rather than one board (U125).
    Count,
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

/// One lettered row: a symbol, the number it names and the range the field keeps it inside.
fn lettered(
    param: ToolParam,
    symbol: &'static str,
    value: f64,
    range: RangeInclusive<f64>,
    decimals: usize,
) -> ToolRow {
    ToolRow {
        param,
        label: Some(FieldLabel::Symbol(symbol)),
        value,
        unit: "",
        range,
        decimals,
        locked: false,
        disabled: false,
        centre: false,
    }
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

/// One row of a card that picks among a few named options (U123).
///
/// A class, a build and a count are one question each: which of these is in force. The row draws its
/// options as chips in the order a maker reads them and marks the one that is on, so the card says
/// what the next click will make without a second surface.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolChoice {
    /// The options, in the order a maker reads them.
    pub options: Vec<Message>,
    /// Which option is in force.
    pub chosen: usize,
}

/// The option a card's choice row picked: the row of [`ToolView::choices`] and the option in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToolPick {
    /// Which choice row was pressed, as an index into [`ToolView::choices`].
    pub row: usize,
    /// Which option of that row was pressed.
    pub option: usize,
}

/// What both surfaces draw: the tool's name, its numbers and how its commit is offered.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolView {
    /// The tool's name, drawn above the rows when it has more than one.
    pub title: Option<Message>,
    /// The choices the tool asks before its numbers, in the order it declares them.
    pub choices: Vec<ToolChoice>,
    /// The numbers, in the order the tool declares them.
    pub rows: Vec<ToolRow>,
    /// The tool's one two-state method, drawn beside its name, when it offers one.
    pub toggle: Option<ToolToggle>,
    /// How the surface offers the tool's one commit.
    pub commit: ToolCommit,
    /// A line of context under the name, already in the person's language.
    ///
    /// An armed boolean prints the base its result is born in here (X99): the frame marks that
    /// operand with an ink of its own and the card names it, so the picture and the words agree.
    /// Empty for every tool whose name is the whole story.
    pub note: Option<String>,
}

/// The one two-state method a tool offers beside its numbers.
///
/// A method with two states is one switch, not two buttons: the words name the state the tool is in,
/// so a person reads what the next drag does rather than what a press would change it to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToolToggle {
    /// The words of the state the tool is in.
    pub words: Message,
    /// Whether the tool is in that state, which draws the switch pressed.
    pub on: bool,
}

/// How the tool's commit is offered to the hand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolCommit {
    /// The ✓ and ✕ pair under the rows, beside the gesture's own Enter and Escape, with the words
    /// the tool's own commit is known by.
    Pair {
        /// The hover and accessible name of ✓.
        apply: Message,
        /// The hover and accessible name of ✕.
        cancel: Message,
    },
    /// The keys alone: the card draws no button, so its field is the only control it holds and the
    /// gesture's Enter commits and Escape cancels (U104).
    Keys,
    /// Neither: the tool commits on its own gesture, like a fill's click.
    Gesture,
}

impl ToolView {
    /// A tool with one number, named by its title.
    pub fn height(title: Message, value: f64, range: RangeInclusive<f64>, decimals: usize) -> Self {
        Self {
            title: Some(title),
            choices: Vec::new(),
            rows: vec![ToolRow::height(value, range, decimals)],
            toggle: None,
            commit: ToolCommit::Pair { apply: Message::ApplyEnter, cancel: Message::CancelEsc },
            note: None,
        }
    }

    /// A tool that asks for no number: its name and its one commit, drawn as one row (U115).
    ///
    /// A running draft and a live contour edit are the two: the shape is already in the scene, so
    /// the only decisions left are to accept it or to drop it, and both sit on the row that names
    /// the tool.
    pub fn actions(title: Option<Message>, commit: ToolCommit) -> Self {
        Self { title, choices: Vec::new(), rows: Vec::new(), toggle: None, commit, note: None }
    }

    /// The same card with a line of context under its name, in the person's language.
    ///
    /// A boolean is the one tool that has something to say beside its name: which operand its result
    /// is born in (X99). It is a builder rather than a constructor argument, because every other
    /// tool has nothing to add and a parameter they all pass `None` for is a column of noise.
    pub fn with_note(mut self, note: Option<String>) -> Self {
        self.note = note;
        self
    }

    /// A tool whose one number is named by a symbol and nothing else.
    ///
    /// This is the plate rule of the sketch fields: `H` in front of the number says what it is, so
    /// the plate carries no word, no unit and no button pair — the gesture's own Enter commits and
    /// its Escape cancels, and the field is the only control that takes the keyboard (U104).
    pub fn lettered_height(symbol: &'static str, value: f64, range: RangeInclusive<f64>, decimals: usize) -> Self {
        Self {
            title: None,
            choices: Vec::new(),
            rows: vec![lettered(ToolParam::Height, symbol, value, range, decimals)],
            toggle: None,
            commit: ToolCommit::Keys,
            note: None,
        }
    }

    /// A pull's card: the height `H` a body rises by, and — for a cone — the top radius `R`.
    ///
    /// The two numbers are one plate because they are one shape: a taper is chosen in the same breath
    /// as the height it tapers over, and both are written by the same ✓ (ADR 0141). A pull with no
    /// `top` is the extrusion's one number and draws exactly as it did.
    pub fn pull(height: f64, top: Option<f64>, range: RangeInclusive<f64>, decimals: usize) -> Self {
        let mut rows = vec![lettered(ToolParam::Height, "H", height, range.clone(), decimals)];
        if let Some(top) = top {
            // A cone's top radius reaches zero, where the shape becomes the sharp apex, so its range
            // starts at the bottom rather than at the smallest positive length.
            rows.push(lettered(ToolParam::TopRadius, "R", top, 0.0..=*range.end(), decimals));
        }
        Self { title: None, choices: Vec::new(), rows, toggle: None, commit: ToolCommit::Keys, note: None }
    }

    /// Whether the surface draws anything at all.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty() && self.choices.is_empty()
    }

    /// The same card with the choices its tool asks before its numbers.
    pub fn with_choices(mut self, choices: Vec<ToolChoice>) -> Self {
        self.choices = choices;
        self
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
    /// Whether the tool's switch was pressed this frame.
    pub toggle: bool,
    /// The option a choice row picked this frame, if any.
    pub pick: Option<ToolPick>,
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

/// Draws one row as a plate of its own, centred on `at`, and reports what its field asked for.
///
/// This is the shape a gap takes in the scene: a carried piece's field sits in the middle of the
/// arrow that measures it (N26, ADR 0161), so the plate is one row of the shared field and nothing
/// else — no title, no label word and no button pair. Two surfaces are told apart by their id, so
/// each gap's field keeps its own keyboard focus as the plate travels with the piece.
///
/// The plate takes no pointer. A carried piece holds no button, so a press anywhere in the work area
/// is the click that puts it down; a plate that sensed the hand would take that click and scrub a
/// number into the gap the piece would then jump to (X66). Its field still takes the keyboard, which
/// is how a gap is typed.
pub fn plate(
    ui: &mut Ui,
    viewport: Rect,
    at: Pos2,
    id: Id,
    row: &mut ToolRow,
    focus: bool,
    opacity: u32,
) -> (ToolOutcome, Rect) {
    let fill = theme::at_opacity(theme::floating(ui), opacity.clamp(80, 100));
    let mut outcome = ToolOutcome::default();
    let rect = egui::Area::new(id)
        .order(Order::Middle)
        .pivot(Align2::CENTER_CENTER)
        .fixed_pos(at)
        .interactable(false)
        .constrain_to(viewport)
        .show(ui.ctx(), |ui| {
            let frame = egui::Frame::new()
                .fill(fill)
                .stroke(Stroke::new(1.0, theme::line(ui)))
                .corner_radius(CornerRadius::same(theme::RADIUS))
                .inner_margin(Margin::symmetric(5, 2));
            frame.show(ui, |ui| {
                outcome = one_row(ui, row, focus);
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

/// Draws the title, the rows, the switch and the button pair, and reports what they asked for.
fn body(ui: &mut Ui, view: &mut ToolView, focus: Option<ToolParam>) -> ToolOutcome {
    let lang = language(ui.ctx());
    let mut outcome = ToolOutcome::default();
    // The choices stand above the numbers: they say what the tool will make, and the numbers say
    // where. A card of choices alone draws them and nothing else.
    for (row, choice) in view.choices.iter().enumerate() {
        ui.horizontal(|ui| {
            for (option, label) in choice.options.iter().enumerate() {
                if ui.selectable_label(choice.chosen == option, label.text(lang)).clicked() {
                    outcome.pick = Some(ToolPick { row, option });
                }
            }
        });
    }
    let choices_only = view.rows.is_empty() && !view.choices.is_empty();
    if choices_only {
        note_ui(ui, view.note.as_deref());
        return outcome;
    }
    // A tool with no number is one row: its name, its one switch and its pair, so the card costs the
    // scene a line instead of a plate (U115).
    let one_row_card = view.rows.is_empty();
    // A tool with one number is named once, in the title: repeating it as a row label would be the
    // same word twice in a 100 pt plate.
    let titled = view.title.is_some() && (view.rows.len() > 1 || view.rows.iter().all(|row| row.label.is_none()));
    if one_row_card {
        ui.horizontal(|ui| {
            if let Some(title) = view.title {
                ui.label(RichText::new(title.text(lang)).strong());
            }
            outcome.toggle |= toggle_ui(ui, view.toggle, lang);
            pair_ui(ui, view.commit, &mut outcome, lang);
        });
        note_ui(ui, view.note.as_deref());
        return outcome;
    }
    if titled && let Some(title) = view.title {
        // The switch sits beside the name, and a card without one draws the name alone.
        match view.toggle {
            Some(_) => {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(title.text(lang)).strong());
                    outcome.toggle |= toggle_ui(ui, view.toggle, lang);
                });
            }
            None => {
                ui.label(RichText::new(title.text(lang)).strong());
            }
        }
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
    if matches!(view.commit, ToolCommit::Pair { .. }) {
        ui.add_space(2.0);
        ui.horizontal(|ui| pair_ui(ui, view.commit, &mut outcome, lang));
    }
    note_ui(ui, view.note.as_deref());
    outcome
}

/// The line of context under a tool's name: quiet type, one line, nothing to press.
///
/// A note names what the picture already marks — the base a boolean's result is born in (X99) — so
/// it is secondary ink at the small size: the reader's eye finds it after the operation's name and
/// before the ground it sits on.
fn note_ui(ui: &mut Ui, note: Option<&str>) {
    let Some(note) = note else {
        return;
    };
    ui.label(RichText::new(note).small().color(theme::text_dim(ui)));
}

/// The tool's ✓ and ✕, when its commit is the pair; the words are the tool's own.
///
/// The cross is `actions::dismiss`, which the style guide gives to closing a surface, and not
/// `actions::cancel`, the back arrow: a card's ✕ drops the preview and closes the card, and a plain
/// left arrow beside a check reads as *back* (ADR 0119 names the pair ✓ and ✕).
fn pair_ui(ui: &mut Ui, commit: ToolCommit, outcome: &mut ToolOutcome, lang: crate::Lang) {
    let ToolCommit::Pair { apply, cancel } = commit else {
        return;
    };
    if icons::icon_button(ui, icons::actions::CONFIRM, apply.text(lang)).clicked() {
        outcome.action = Some(ToolAction::Apply);
    }
    if icons::icon_button(ui, icons::actions::DISMISS, cancel.text(lang)).clicked() {
        outcome.action = Some(ToolAction::Cancel);
    }
}

/// The tool's one switch: the words of the state it is in, drawn pressed while it holds.
fn toggle_ui(ui: &mut Ui, toggle: Option<ToolToggle>, lang: crate::Lang) -> bool {
    let Some(toggle) = toggle else {
        return false;
    };
    ui.selectable_label(toggle.on, toggle.words.text(lang)).clicked()
}

/// Draws the one row of a plate and reports what it asked for, refusal included.
fn one_row(ui: &mut Ui, row: &mut ToolRow, focus: bool) -> ToolOutcome {
    let lang = language(ui.ctx());
    let mut outcome = ToolOutcome::default();
    if row.disabled {
        ui.label(RichText::new(Message::DistanceNoNeighbour.text(lang)).small().color(theme::text_dim(ui)));
        return outcome;
    }
    row_ui(ui, row, false, focus, &mut outcome);
    if let Some(message) = outcome.error {
        ui.colored_label(theme::danger(ui), message.text(lang));
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
        ToolView::height(Message::FrameHeight, 120.0, 1.0..=10_000.0, 3)
    }

    /// Draws one card frame at `pointer` and returns what it asked for and the rectangle it took.
    fn run(events: Vec<egui::Event>, pointer: Pos2, scale: f32) -> (ToolOutcome, Rect, Vec<String>) {
        run_view(view(), events, pointer, scale)
    }

    /// [`run`] over a card of the caller's own: a tool that is not a height plate.
    fn run_view(
        mut tool: ToolView,
        events: Vec<egui::Event>,
        pointer: Pos2,
        scale: f32,
    ) -> (ToolOutcome, Rect, Vec<String>) {
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(scale);
        crate::set_language(&ctx, crate::Lang::En);
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
    fn a_one_row_card_prints_the_note_under_its_name() {
        // X99: the armed boolean marks its base in the frame and names it on the card, so the eye
        // and the words say the same thing. The note is context: the name and its pair are drawn
        // first, and the note follows them in the quiet ink.
        let view = ToolView::actions(
            Some(Message::ToolSubtract),
            ToolCommit::Pair { apply: Message::ApplyEnter, cancel: Message::CancelEsc },
        )
        .with_note(Some(Message::BooleanBase.with(crate::Lang::En, ["Ring 2"])));
        let (_, _, texts) = run_view(view, Vec::new(), Pos2::new(200.0, 200.0), 1.0);
        let named = texts.iter().position(|text| text == "Subtract").expect("the card names the operation");
        let note = texts.iter().position(|text| text == "Base: Ring 2").expect("and its base: {texts:?}");
        assert!(note > named, "the note reads under the operation's name: {texts:?}");

        let bare = ToolView::actions(Some(Message::ToolSubtract), ToolCommit::Keys);
        let (_, _, texts) = run_view(bare, Vec::new(), Pos2::new(200.0, 200.0), 1.0);
        assert!(!texts.iter().any(|text| text.starts_with("Base:")), "a tool with no base prints no note");
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
    fn a_gap_plate_is_click_through_so_a_press_over_it_puts_the_piece_down() {
        // X66: the gap's field sits in the scene, over the piece and the space around it, so the
        // pointer must reach whatever is behind it — the press there is the click that puts the
        // piece down and never a scrub of the number it covers. A card instead senses the pointer:
        // its plate is a deliberate target (ADR 0119).
        let ctx = egui::Context::default();
        crate::set_language(&ctx, crate::Lang::En);
        let mut row = ToolRow::height(120.0, 1.0..=10_000.0, 3);
        let at = Pos2::new(200.0, 200.0);
        let mut rect = Rect::NOTHING;
        let mut layer = None;
        for _ in 0..3 {
            let output = ctx.run_ui(egui::RawInput { screen_rect: Some(viewport()), ..Default::default() }, |ui| {
                let shown = plate(ui, viewport(), at, Id::unique("gap_plate"), &mut row, false, 92);
                rect = shown.1;
                layer = ui.ctx().layer_id_at(at).map(|layer| layer.id);
            });
            output.drop_without_applying_deltas();
        }
        assert!(rect.contains(at), "the plate covers the point it measures: {rect:?}");
        assert_ne!(layer, Some(Id::unique("gap_plate")), "a gap's plate leaves the press to the scene");
    }

    #[test]
    fn a_gap_plate_still_takes_the_keyboard() {
        let ctx = egui::Context::default();
        crate::set_language(&ctx, crate::Lang::En);
        let mut row = ToolRow::height(120.0, 1.0..=10_000.0, 3);
        let mut focused = false;
        for frame in 0..4 {
            let focus = frame < 2;
            let output = ctx.run_ui(egui::RawInput { screen_rect: Some(viewport()), ..Default::default() }, |ui| {
                plate(ui, viewport(), Pos2::new(200.0, 200.0), Id::unique("gap_plate"), &mut row, focus, 92);
                focused = ui.ctx().text_edit_focused();
            });
            output.drop_without_applying_deltas();
        }
        assert!(focused, "a gap's plate still lets a number be typed");
    }

    /// The rectangle a drawn label took, so a test can press it.
    fn text_rect(output: &egui::FullOutput, text: &str) -> Option<Rect> {
        fn walk(shape: &egui::Shape, text: &str, found: &mut Option<Rect>) {
            match shape {
                egui::Shape::Text(galley) => {
                    if galley.galley.text() == text {
                        *found = Some(galley.visual_bounding_rect());
                    }
                }
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        walk(shape, text, found);
                    }
                }
                _ => {}
            }
        }
        let mut found = None;
        for clipped in &output.shapes {
            walk(&clipped.shape, text, &mut found);
        }
        found
    }

    #[test]
    fn a_choice_row_draws_its_options_and_reports_the_one_pressed() {
        // U123: the class and the build are one row each, so the card says what the next click makes
        // before it happens. A press answers the row and the option, and nothing else on the card is
        // touched by it.
        let mut tool = view().with_choices(vec![
            ToolChoice { options: vec![Message::ToolPost, Message::ToolShelf, Message::ToolWall], chosen: 0 },
            ToolChoice { options: vec![Message::FillMethodFit, Message::FillMethodShape], chosen: 0 },
        ]);
        let ctx = egui::Context::default();
        crate::set_language(&ctx, crate::Lang::En);
        let pointer = Pos2::new(200.0, 200.0);
        let mut events = Vec::new();
        let mut pressed = None;
        let mut seen = Vec::new();
        // A card is an area, so its first frame lays out and does not paint: the label's rectangle is
        // read from the second frame and the press lands on the third.
        for frame in 0..3 {
            let input = egui::RawInput { screen_rect: Some(viewport()), events: events.clone(), ..Default::default() };
            let output = ctx.run_ui(input, |ui| {
                pressed = show(ui, viewport(), pointer, &mut tool, None, 92).0.pick.or(pressed);
            });
            seen = texts_of(&output);
            if frame == 1 {
                let at = text_rect(&output, Message::FillMethodShape.text(crate::Lang::En))
                    .expect("the build row draws its shape option");
                let center = at.center();
                let button = |pressed| egui::Event::PointerButton {
                    pos: center,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                };
                events = vec![egui::Event::PointerMoved(center), button(true), button(false)];
            }
            output.drop_without_applying_deltas();
        }
        for words in ["Post", "Shelf", "Wall", "Rectangle", "Niche shape"] {
            assert!(seen.iter().any(|text| text == words), "`{words}` is on the card: {seen:?}");
        }
        assert_eq!(pressed, Some(ToolPick { row: 1, option: 1 }), "a press on the second row's second option");
    }

    #[test]
    fn a_card_of_choices_alone_draws_them_and_no_field() {
        let context = ToolView::actions(None, ToolCommit::Gesture)
            .with_choices(vec![ToolChoice { options: vec![Message::ToolPost, Message::ToolShelf], chosen: 1 }]);
        let (outcome, _, texts) = run_view(context, Vec::new(), Pos2::new(200.0, 200.0), 1.0);
        assert_eq!(outcome.pick, None, "drawing presses nothing");
        assert!(texts.iter().any(|text| text == "Post") && texts.iter().any(|text| text == "Shelf"), "{texts:?}");
        assert!(!texts.iter().any(|text| text == "mm"), "a card of choices draws no number: {texts:?}");
    }

    #[test]
    fn a_card_at_rest_asks_for_nothing() {
        let (outcome, rect, texts) = run(Vec::new(), Pos2::new(200.0, 200.0), 1.0);
        assert_eq!(outcome, ToolOutcome::default(), "drawing the card applies nothing");
        assert!(rect.width() > 0.0, "the card took a rectangle");
        assert!(texts.iter().any(|text| text == "Frame height"), "the tool's name is on the card: {texts:?}");
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
    fn a_tool_with_no_number_is_one_row_of_its_name_its_switch_and_its_pair() {
        // U115: a running draft and a live contour edit ask for no number, so their card is the one
        // row that names the tool and holds its two buttons. The switch is one control with two
        // states, and the words are the tool's own, so ✓ says what its commit does here.
        let ctx = egui::Context::default();
        crate::set_language(&ctx, crate::Lang::En);
        let mut tool = ToolView {
            choices: Vec::new(),
            title: Some(Message::ContourEdit),
            rows: Vec::new(),
            toggle: Some(ToolToggle { words: Message::AlongContour, on: false }),
            commit: ToolCommit::Pair { apply: Message::ApplyEnter, cancel: Message::CancelEsc },
            note: None,
        };
        let mut outcome = ToolOutcome::default();
        let mut rect = Rect::NOTHING;
        let mut texts = Vec::new();
        for _ in 0..2 {
            let output = ctx.run_ui(egui::RawInput { screen_rect: Some(viewport()), ..Default::default() }, |ui| {
                let shown = show(ui, viewport(), Pos2::new(200.0, 200.0), &mut tool, None, 92);
                outcome = shown.0;
                rect = shown.1;
            });
            texts = texts_of(&output);
            output.drop_without_applying_deltas();
        }
        assert_eq!(outcome, ToolOutcome::default(), "a card at rest asks for nothing");
        assert!(texts.iter().any(|text| text == "Contour edit"), "the row names the tool: {texts:?}");
        assert!(texts.iter().any(|text| text == "Along contour"), "and holds its one switch: {texts:?}");
        assert!(rect.height() < 2.0 * crate::theme::STATUS_HEIGHT, "the card is one row tall: {rect:?}");
    }

    #[test]
    fn the_pair_reports_the_button_that_was_pressed() {
        // The two buttons are the tool's own commit and cancel, so the card reports the one pressed
        // and the shell runs the path that already owns it (U115).
        for (words, action) in [(Message::FinishEnter, ToolAction::Apply), (Message::CancelEsc, ToolAction::Cancel)] {
            let ctx = egui::Context::default();
            crate::theme::install(&ctx);
            ctx.enable_accesskit();
            crate::set_language(&ctx, crate::Lang::En);
            let commit = ToolCommit::Pair { apply: Message::FinishEnter, cancel: Message::CancelEsc };
            let mut outcome = ToolOutcome::default();
            let mut at = None;
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
                let output = ctx
                    .run_ui(egui::RawInput { screen_rect: Some(viewport()), events, ..Default::default() }, |ui| {
                        pair_ui(ui, commit, &mut outcome, crate::Lang::En)
                    });
                if frame == 0 {
                    at = node_center(&output, words.text(crate::Lang::En));
                    assert!(at.is_some(), "the pair draws `{}`", words.text(crate::Lang::En));
                }
                output.drop_without_applying_deltas();
            }
            assert_eq!(outcome.action, Some(action), "a press on `{}` asks for it", words.text(crate::Lang::En));
        }
    }

    #[test]
    fn the_switch_reports_one_press() {
        // The switch has two states and the words name the one the tool is in, so a press asks for
        // the other one and the caller reads what the tool holds now (U115).
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        ctx.enable_accesskit();
        crate::set_language(&ctx, crate::Lang::En);
        let toggle = ToolToggle { words: Message::AlongContour, on: false };
        let mut pressed = false;
        let mut at = None;
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
            let output = ctx
                .run_ui(egui::RawInput { screen_rect: Some(viewport()), events, ..Default::default() }, |ui| {
                    pressed |= toggle_ui(ui, Some(toggle), crate::Lang::En)
                });
            if frame == 0 {
                at = node_center(&output, Message::AlongContour.text(crate::Lang::En));
            }
            output.drop_without_applying_deltas();
        }
        assert!(pressed, "a press on the switch asks for the other state");
    }

    /// The middle of the accessibility node labelled `label`, so a test can aim at a button.
    fn node_center(output: &egui::FullOutput, label: &str) -> Option<Pos2> {
        let update = output.platform_output.accesskit_update.as_ref()?;
        update.nodes.iter().find_map(|(_, node)| {
            if node.label() != Some(label) {
                return None;
            }
            let bounds = node.bounds()?;
            Some(Pos2::new(((bounds.x0 + bounds.x1) / 2.0) as f32, ((bounds.y0 + bounds.y1) / 2.0) as f32))
        })
    }

    #[test]
    fn a_disabled_row_says_why_and_takes_no_number() {
        let ctx = egui::Context::default();
        crate::set_language(&ctx, crate::Lang::En);
        let mut tool = ToolView {
            choices: Vec::new(),
            title: None,
            commit: ToolCommit::Gesture,
            toggle: None,
            note: None,
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
