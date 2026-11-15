//! The numbers of the point being placed: one plate per parameter, its focus and its typed locks.
//!
//! A sketch point has more than one number and the numbers do not belong in one box: a segment's
//! length sits on the segment while its angle sits outside the arc that measures it, and a
//! rectangle's width and height sit on their own sides. A [`FieldSet`] therefore draws one plate per
//! parameter at the anchor its tool gives it, and owns the grammar that makes several plates one
//! point:
//!
//! - every plate shows the live value the pointer produces until the person types in it;
//! - a typed number locks its parameter and outranks the pointer: [`FieldSet::set_live`] never
//!   overwrites a locked row;
//! - one plate holds the keyboard, Tab moves it, and a typed `0` gives a parameter back to the
//!   pointer — a lock is a promise and zero is the way out;
//! - Enter places the point with every plate; Escape drops the locks first and cancels the tool
//!   second, so one key never means two things at once.
//!
//! Keys are read here because the point being placed writes nothing: the plates are view state, the
//! commit is the caller's, and no edit scope is minted. The plate is the shared [`Scrub`] of ADR 0092
//! with its unit outside the box, floating over the scene under the chrome (ADR 0123).
//!
//! Changes when: the grammar of a placed number, the plate's placement or the guide anchors change.

use core::ops::RangeInclusive;

use egui::{Align2, Context, CornerRadius, Id, Key, Margin, Modifiers, Pos2, Rect, RichText, Stroke};

use crate::scrub::Scrub;
use crate::theme;
use crate::{Lang, Message, language};

/// The clearance between a plate and the point it belongs to, in points.
const PLATE_GAP: f32 = 16.0;

/// The plate's opacity over the scene, in percent.
const PLATE_OPACITY: u32 = 96;

/// How many frames a plate asks for the keyboard before it leaves the keys alone.
const FOCUS_REQUESTS: u8 = 2;

/// One number of the point being placed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FieldParam {
    /// The point's abscissa on the work plane, in millimetres.
    X,
    /// The point's ordinate on the work plane, in millimetres.
    Y,
    /// A segment's length, or an arc's chord, in millimetres.
    Length,
    /// A direction from +X in the node's local plane, in degrees.
    Angle,
    /// The width of a rectangle, in millimetres.
    Width,
    /// The height of a rectangle, in millimetres.
    Height,
    /// A circle's or a polygon's radius, in millimetres.
    Radius,
    /// A circle's diameter, in millimetres.
    Diameter,
    /// A regular polygon's side count.
    Sides,
    /// A polygon's rotation: the angle of its second point from the first, in degrees.
    Rotation,
}

/// What a plate writes before its number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldLabel {
    /// A word, drawn in the workspace language.
    Word(Message),
    /// A symbol the drawing uses (`R`, `W`, `∠`); notation is not translated.
    Symbol(&'static str),
}

/// One plate: what it edits, where it sits and what it shows.
#[derive(Clone, Debug, PartialEq)]
pub struct FieldRow {
    /// The parameter the plate edits.
    pub param: FieldParam,
    /// What the plate writes before the number, when it writes anything.
    pub label: Option<FieldLabel>,
    /// The number the plate shows, in `unit`.
    pub value: f64,
    /// The unit, drawn dim outside the box.
    pub unit: &'static str,
    /// The range the field keeps the number inside.
    pub range: RangeInclusive<f64>,
    /// The digits after the decimal point; `0` draws a whole number.
    pub decimals: usize,
    /// The point on screen the plate belongs to: a segment's midpoint, its arc, the first point.
    pub anchor: Pos2,
}

impl FieldRow {
    /// A plate for `param` at `anchor`.
    pub const fn new(
        param: FieldParam,
        label: Option<FieldLabel>,
        value: f64,
        unit: &'static str,
        range: RangeInclusive<f64>,
        decimals: usize,
        anchor: Pos2,
    ) -> Self {
        Self { param, label, value, unit, range, decimals, anchor }
    }
}

/// The number one plate committed this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FieldEdit {
    /// The parameter the number belongs to.
    pub param: FieldParam,
    /// The number the person typed.
    pub value: f64,
}

/// What the plates asked the caller to do this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FieldAction {
    /// A number was typed and now locks its parameter.
    pub edit: Option<FieldEdit>,
    /// Enter: the point is placed with every plate's number.
    pub commit: bool,
    /// Escape or a typed `0`: the locks are gone and the pointer drives the point again.
    pub released: bool,
    /// Escape with nothing locked: the caller cancels its tool.
    pub cancel: bool,
}

impl FieldAction {
    /// Whether the frame asked for nothing at all.
    pub fn is_none(&self) -> bool {
        self.edit.is_none() && !self.commit && !self.released && !self.cancel
    }
}

/// The fields of one point stage, their focus and their typed locks.
#[derive(Clone, Debug)]
pub struct FieldSet {
    rows: Vec<FieldRow>,
    /// One flag per row: the person typed there, so the pointer may not overwrite it.
    locked: Vec<bool>,
    /// The row that holds the keyboard.
    focus: usize,
    /// How many frames the focused plate still asks for the keyboard.
    ///
    /// A plate lives in its own `Area`, and egui grants the focus a pass after the request: asking
    /// once leaves the field unfocused, asking twice settles it without stealing the keys back from
    /// a panel the person clicks later.
    request: u8,
    /// A plate held the keyboard last frame, so Tab belongs to the set.
    holding: bool,
    /// The rectangle of every plate after the last draw.
    rects: Vec<Rect>,
    /// The union of every plate's rectangle, for the caller's chrome hit test.
    rect: Rect,
    /// Why the last typed number was refused, for the status line.
    error: Option<Message>,
}

impl FieldSet {
    /// The plates of one point, the first of them holding the keyboard.
    ///
    /// `None` when there is no row: a set with no number is not a field set, and the caller must not
    /// draw an empty plate.
    pub fn new(rows: Vec<FieldRow>) -> Option<Self> {
        if rows.is_empty() {
            return None;
        }
        let locked = vec![false; rows.len()];
        let rects = vec![Rect::NOTHING; rows.len()];
        Some(Self {
            rows,
            locked,
            focus: 0,
            request: FOCUS_REQUESTS,
            holding: false,
            rects,
            rect: Rect::NOTHING,
            error: None,
        })
    }

    /// The plates, in the order the tool declared them.
    pub fn rows(&self) -> &[FieldRow] {
        &self.rows
    }

    /// The number in `param`'s plate, locked or live.
    pub fn value(&self, param: FieldParam) -> Option<f64> {
        self.index(param).map(|index| self.rows[index].value)
    }

    /// Whether the person typed `param`, so the pointer no longer drives it.
    pub fn is_locked(&self, param: FieldParam) -> bool {
        self.index(param).is_some_and(|index| self.locked[index])
    }

    /// Whether any parameter is locked.
    pub fn locked_any(&self) -> bool {
        self.locked.iter().any(|locked| *locked)
    }

    /// The parameter that holds the keyboard.
    pub fn focus(&self) -> Option<FieldParam> {
        self.rows.get(self.focus).map(|row| row.param)
    }

    /// The rectangle of `param`'s plate after the last draw, for a press on chrome.
    pub fn plate_rect(&self, param: FieldParam) -> Option<Rect> {
        self.index(param).and_then(|index| self.rects.get(index).copied())
    }

    /// The union of every plate's rectangle after the last draw.
    pub fn rect(&self) -> Rect {
        self.rect
    }

    /// Why the last typed number was refused, if it was.
    pub fn error(&self) -> Option<Message> {
        self.error
    }

    /// Writes the pointer's live value into `param`, unless the person typed there.
    pub fn set_live(&mut self, param: FieldParam, value: f64) {
        if let Some(index) = self.index(param)
            && !self.locked[index]
        {
            self.rows[index].value = value;
        }
    }

    /// Moves `param`'s plate to `anchor`, for a field that follows the pointer.
    pub fn set_anchor(&mut self, param: FieldParam, anchor: Pos2) {
        if let Some(index) = self.index(param) {
            self.rows[index].anchor = anchor;
        }
    }

    /// Moves the keyboard to the next plate and back to the first.
    pub fn focus_next(&mut self) {
        if self.rows.is_empty() {
            return;
        }
        self.focus = (self.focus + 1) % self.rows.len();
        self.request = FOCUS_REQUESTS;
    }

    /// Drops every lock, so the pointer drives every parameter again.
    pub fn reset(&mut self) {
        self.locked.fill(false);
        self.request = FOCUS_REQUESTS;
    }

    /// Draws every plate and reads the keys the set owns.
    ///
    /// The plates float inside `viewport` and are placed beside their anchors, on the side that
    /// leaves the point itself uncovered. A press inside a plate is chrome: `rect` reports what the
    /// caller must keep from the pointer.
    pub fn show(&mut self, ctx: &Context, viewport: Rect) -> FieldAction {
        let lang = language(ctx);
        let mut action = FieldAction::default();
        // Tab moves the keyboard before the plates draw, so the next plate takes it this frame.
        if self.holding && ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Tab)) {
            self.focus_next();
        }
        self.error = None;
        let asking = self.request > 0;
        let mut focused = None;
        let mut union = Rect::NOTHING;
        for index in 0..self.rows.len() {
            let request = asking && index == self.focus;
            let Some(result) = self.row_plate(ctx, index, viewport, request, lang) else {
                continue;
            };
            if let Some(rect) = self.rects.get(index).copied() {
                union = union.union(rect);
            }
            if result.focused {
                focused = Some(index);
            }
            if let Some(error) = result.error {
                self.error = Some(error);
            }
            if result.commit {
                let param = self.rows[index].param;
                self.rows[index].value = result.value;
                if result.value == 0.0 {
                    // Zero is the way out of a lock: the pointer takes the parameter back.
                    self.locked[index] = false;
                    action.released = true;
                } else {
                    self.locked[index] = true;
                    action.edit = Some(FieldEdit { param, value: result.value });
                }
            }
        }
        self.rect = union;
        if asking {
            self.request = self.request.saturating_sub(1);
        }
        let held = focused.or_else(|| self.holding.then_some(self.focus));
        self.holding = focused.is_some();
        if let Some(index) = held {
            if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Enter)) {
                action.commit = true;
            } else if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape)) {
                let lock = self.locked.get(index).copied().unwrap_or(false);
                if lock {
                    self.reset();
                    action.released = true;
                } else {
                    action.cancel = true;
                }
                self.release_keyboard(ctx, index);
            }
        }
        action
    }

    /// Hands the keyboard back to the tool, so the next Escape is not read twice.
    fn release_keyboard(&mut self, ctx: &Context, index: usize) {
        let Some(row) = self.rows.get(index) else {
            return;
        };
        let id = Id::unique(("point-field", row.param));
        ctx.memory_mut(|memory| memory.surrender_focus(id));
        self.holding = false;
    }

    /// Draws one plate and reports what its field said this frame.
    fn row_plate(
        &mut self,
        ctx: &Context,
        index: usize,
        viewport: Rect,
        request: bool,
        lang: Lang,
    ) -> Option<PlateResult> {
        let row = self.rows.get_mut(index)?;
        let (align, position) = plate_corner(row.anchor, viewport);
        let mut result = None;
        let rect = egui::Area::new(Id::unique(("point-field", row.param)))
            .order(egui::Order::Middle)
            .pivot(align)
            .fixed_pos(position)
            .constrain_to(viewport)
            .show(ctx, |ui| {
                let fill = theme::at_opacity(theme::floating(ui), PLATE_OPACITY);
                egui::Frame::new()
                    .fill(fill)
                    .stroke(Stroke::new(1.0, theme::line(ui)))
                    .corner_radius(CornerRadius::same(theme::RADIUS))
                    .inner_margin(Margin::symmetric(6, 3))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            match row.label {
                                Some(FieldLabel::Word(message)) => {
                                    ui.label(RichText::new(message.text(lang)).small().color(theme::text_dim(ui)));
                                }
                                Some(FieldLabel::Symbol(symbol)) => {
                                    ui.label(RichText::new(symbol).small().color(theme::text_dim(ui)));
                                }
                                None => {}
                            }
                            let mut value = row.value;
                            let field = Scrub::new(&mut value)
                                .range(row.range.clone())
                                .decimals(row.decimals)
                                .unit(row.unit)
                                .focus(request)
                                .show(ui);
                            result = Some(PlateResult {
                                value,
                                commit: field.commit,
                                focused: field.response.has_focus(),
                                error: field.error,
                            });
                        });
                    });
            })
            .response
            .rect;
        if let Some(slot) = self.rects.get_mut(index) {
            *slot = rect;
        }
        result
    }

    /// The position of `param`'s plate in `self.rows`.
    fn index(&self, param: FieldParam) -> Option<usize> {
        self.rows.iter().position(|row| row.param == param)
    }
}

/// What one plate's field said this frame.
#[derive(Clone, Copy, Debug)]
struct PlateResult {
    /// The number the field now holds.
    value: f64,
    /// The field committed a number (typed text or a drag).
    commit: bool,
    /// The field holds the keyboard.
    focused: bool,
    /// Why the typed text was refused.
    error: Option<Message>,
}

/// Where a plate sits relative to the point it belongs to: away from the point, towards the room.
///
/// The plate keeps a diagonal clearance from its anchor, so it never covers the point being placed,
/// and it leaves the view through the side with less room, which keeps it on screen at an edge.
fn plate_corner(anchor: Pos2, viewport: Rect) -> (Align2, Pos2) {
    let right = anchor.x < viewport.center().x;
    let down = anchor.y < viewport.center().y;
    let align = match (right, down) {
        (true, true) => Align2::LEFT_TOP,
        (false, true) => Align2::RIGHT_TOP,
        (true, false) => Align2::LEFT_BOTTOM,
        (false, false) => Align2::RIGHT_BOTTOM,
    };
    let dx = if right { PLATE_GAP } else { -PLATE_GAP };
    let dy = if down { PLATE_GAP } else { -PLATE_GAP };
    (align, anchor + egui::vec2(dx, dy))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use egui::{Event, Key, Modifiers, RawInput, pos2};

    use super::*;

    const VIEWPORT: Rect = Rect { min: pos2(0.0, 0.0), max: pos2(800.0, 600.0) };

    fn set() -> FieldSet {
        let rows = vec![
            FieldRow::new(FieldParam::Length, None, 500.0, "mm", 0.0..=10_000.0, 1, pos2(200.0, 200.0)),
            FieldRow::new(FieldParam::Angle, None, 30.0, "°", -360.0..=360.0, 1, pos2(260.0, 240.0)),
        ];
        FieldSet::new(rows).expect("two plates")
    }

    fn key(key: Key) -> Event {
        Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }
    }

    /// Draws one frame of the set and returns what it asked for.
    fn frame(ctx: &egui::Context, set: &mut FieldSet, events: Vec<Event>) -> FieldAction {
        let input = RawInput { screen_rect: Some(VIEWPORT), events, ..Default::default() };
        let mut action = FieldAction::default();
        let output = ctx.run_ui(input, |ui| {
            action = set.show(ui.ctx(), VIEWPORT);
        });
        output.drop_without_applying_deltas();
        action
    }

    fn ctx() -> egui::Context {
        let ctx = egui::Context::default();
        crate::set_language(&ctx, crate::Lang::En);
        ctx
    }

    #[test]
    fn a_typed_number_locks_its_parameter_and_the_pointer_cannot_overwrite_it() {
        let mut set = set();
        set.set_live(FieldParam::Length, 600.0);
        assert_eq!(set.value(FieldParam::Length), Some(600.0));
        // A typed number arrives through the field; the lock itself is the set's rule.
        set.locked[0] = true;
        set.set_live(FieldParam::Length, 700.0);
        assert_eq!(set.value(FieldParam::Length), Some(600.0), "a locked number is a promise");
        assert!(set.is_locked(FieldParam::Length));

        set.set_live(FieldParam::Angle, 45.0);
        assert_eq!(set.value(FieldParam::Angle), Some(45.0), "an unlocked number follows the pointer");

        set.reset();
        set.set_live(FieldParam::Length, 700.0);
        assert_eq!(set.value(FieldParam::Length), Some(700.0), "a reset gives the parameter back");
    }

    #[test]
    fn tab_moves_the_keyboard_to_the_next_plate() {
        let ctx = ctx();
        let mut set = set();
        assert_eq!(set.focus(), Some(FieldParam::Length));
        for _ in 0..3 {
            frame(&ctx, &mut set, Vec::new());
        }
        frame(&ctx, &mut set, vec![key(Key::Tab)]);
        assert_eq!(set.focus(), Some(FieldParam::Angle), "Tab moves the keyboard to the second plate");
        frame(&ctx, &mut set, vec![key(Key::Tab)]);
        assert_eq!(set.focus(), Some(FieldParam::Length), "and wraps back to the first");
    }

    #[test]
    fn typing_a_length_locks_it_and_enter_places_the_point() {
        let ctx = ctx();
        let mut set = set();
        // The first plate holds the keyboard, so the person types without reaching for the mouse:
        // the number is selected when the plate takes the focus, and typing replaces it.
        for _ in 0..3 {
            frame(&ctx, &mut set, Vec::new());
        }
        frame(&ctx, &mut set, vec![Event::Text("600".to_owned())]);
        let enter = frame(&ctx, &mut set, vec![key(Key::Enter)]);
        assert_eq!(enter.edit, Some(FieldEdit { param: FieldParam::Length, value: 600.0 }));
        assert!(enter.commit, "Enter places the point");
        assert_eq!(set.value(FieldParam::Length), Some(600.0));
        assert!(set.is_locked(FieldParam::Length));
        set.set_live(FieldParam::Length, 250.0);
        assert_eq!(set.value(FieldParam::Length), Some(600.0), "the pointer cannot move a typed number");
    }

    #[test]
    fn escape_drops_the_lock_first_and_cancels_the_tool_second() {
        let ctx = ctx();
        let mut set = set();
        set.locked[0] = true;
        set.locked[1] = true;
        frame(&ctx, &mut set, Vec::new());
        frame(&ctx, &mut set, Vec::new());
        let first = frame(&ctx, &mut set, vec![key(Key::Escape)]);
        assert!(first.released, "Escape drops the locks");
        assert!(!first.cancel, "and does not also cancel the tool");
        assert!(!set.locked_any(), "every parameter follows the pointer again");

        // The keyboard went back to the tool, so the next Escape is the tool's own.
        let second = frame(&ctx, &mut set, vec![key(Key::Escape)]);
        assert!(second.cancel || second.is_none(), "the second Escape is the tool's: {second:?}");
    }

    #[test]
    fn a_typed_zero_gives_the_parameter_back_to_the_pointer() {
        let ctx = ctx();
        let mut set = set();
        set.locked[0] = true;
        for _ in 0..3 {
            frame(&ctx, &mut set, Vec::new());
        }
        frame(&ctx, &mut set, vec![Event::Text("0".to_owned())]);
        let action = frame(&ctx, &mut set, vec![key(Key::Enter)]);
        assert!(action.released, "a typed zero releases the parameter");
        assert_eq!(action.edit, None, "zero is not a value to apply");
        assert!(!set.is_locked(FieldParam::Length));
        set.set_live(FieldParam::Length, 250.0);
        assert_eq!(set.value(FieldParam::Length), Some(250.0));
    }

    #[test]
    fn a_plate_clears_its_anchor_and_stays_in_the_view() {
        for anchor in [pos2(20.0, 20.0), pos2(780.0, 580.0), pos2(400.0, 300.0)] {
            let (_, position) = plate_corner(anchor, VIEWPORT);
            assert!(
                (position.x - anchor.x).abs() >= PLATE_GAP - f32::EPSILON
                    || (position.y - anchor.y).abs() >= PLATE_GAP - f32::EPSILON,
                "the plate keeps its clearance from {anchor:?}"
            );
        }
    }
}
