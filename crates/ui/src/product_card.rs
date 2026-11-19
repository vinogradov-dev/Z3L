//! The card that creates a product frame: three sides, the footprint they bound, and a type.
//!
//! A maker starts a piece by saying how big its reserved space is, what shape its base is cut to and
//! what the piece is (ADR 0100, ADR 0159). The card is the numeric path to `product.create`: it draws
//! the three sides in whole millimetres on one label column, offers the registry's product classes as
//! the same family/type tree the class picker draws — folded, so the case a shop builds is a row and
//! not a scroll — names the position the chosen class carries, and returns a [`ProductDraft`]. It
//! never dispatches a command and never touches the document — the editor turns the draft into
//! `product.create` and then opens the new frame as a session view operation (ADR 0098).
//!
//! The card asks for no position field: the class already says whether the piece stands or hangs
//! (ADR 0097), so a stored field would be a second source of truth. The row under the class reads
//! that trait and is absent, rather than guessed, for a class that carries no mount.
//!
//! The footprint is four corner rows and the corner type is chosen first: a square corner asks for
//! nothing, a fillet asks for its radius and a chamfer for its two setbacks, which is the offset a
//! maker measures along each edge rather than an angle (ADR 0159). The drawer geometry is the panel's
//! own rule, so a cut that eats the whole edge is refused with the panel's sentence, shown under the
//! block instead of only on the status bar after a failed create.
//!
//! The drawn gesture (a base rectangle and a pulled height) reaches the same command through the
//! box tool, which the ribbon offers as this tool's second method (ADR 0101): the two entries build
//! one document, and this card is not a second creation path, only the typed one.
//!
//! Changes when: the product frame gains a field, the class tree changes how it is folded or
//! searched, or the numeric defaults move.

use egui::collapsing_header::CollapsingState;
use egui::{Button, Key, Modal, RichText, ScrollArea, TextEdit, Ui};
use model::{ClassCode, CornerFeatureKind, Envelope, EnvelopeError, Footprint, RectCorner};
use text::Message;
use units::Length;

use crate::class_picker::{ProductFamily, ProductType};
use crate::theme;

/// The width the card draws at, in points.
const WIDTH: f32 = 380.0;

/// The height one class row takes, in points.
const ROW_HEIGHT: f32 = 22.0;

/// The width the label column of the card's number block at least takes, in points.
///
/// The grid sizes the column to its widest word, so every field beside it starts on one x: a label
/// as wide as its own word is what made the three boxes wander.
const LABEL_WIDTH: f32 = 96.0;

/// The tallest the class list grows before it scrolls, in points.
///
/// Tall enough for the whole registry with every type folded: the fold is what keeps the list short,
/// so the card should not open on a scrollbar.
const MAX_LIST_HEIGHT: f32 = 264.0;

/// The width of a new frame's base, in millimetres, until the maker types another.
pub const DEFAULT_WIDTH_MM: f64 = 600.0;

/// The depth of a new frame's base, in millimetres.
pub const DEFAULT_DEPTH_MM: f64 = 560.0;

/// The height of a new frame, in millimetres.
pub const DEFAULT_HEIGHT_MM: f64 = 720.0;

/// The largest side the field offers: the envelope's own ceiling, in whole millimetres (ADR 0097).
fn max_side_mm() -> f64 {
    Envelope::MAX_SIDE.micrometres() as f64 / 1_000.0
}

/// The setback a corner row starts with, in millimetres, until the maker types another.
pub const DEFAULT_CUT_MM: f64 = 50.0;

/// One corner of the base rectangle as the card holds it, in whole millimetres.
///
/// The type is the row's first decision and the numbers follow it (ADR 0159): a chamfer stores the two
/// setbacks a maker measures along the edges, a fillet its radius. Nothing here is a document value
/// yet, so a half-typed cut cannot fail a command.
#[derive(Clone, Copy, Debug, PartialEq)]
enum CornerDraft {
    /// The corner is square: nothing is cut.
    Square,
    /// A fillet of this radius.
    Radius {
        /// The radius in millimetres.
        radius: f64,
    },
    /// A straight cut with these two setbacks.
    Chamfer {
        /// The setback along the edge that ends at the corner, in millimetres.
        incoming: f64,
        /// The setback along the edge that starts there, in millimetres.
        outgoing: f64,
    },
}

/// The corner type as the three selectable labels offer it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CornerType {
    Square,
    Radius,
    Chamfer,
}

impl CornerType {
    const ALL: [Self; 3] = [Self::Square, Self::Radius, Self::Chamfer];

    const fn label(self) -> Message {
        match self {
            Self::Square => Message::CornerNone,
            Self::Radius => Message::CornerRadius,
            Self::Chamfer => Message::CornerChamfer,
        }
    }

    /// The draft this type starts as, with the default setback.
    const fn default_draft(self) -> CornerDraft {
        match self {
            Self::Square => CornerDraft::Square,
            Self::Radius => CornerDraft::Radius { radius: DEFAULT_CUT_MM },
            Self::Chamfer => CornerDraft::Chamfer { incoming: DEFAULT_CUT_MM, outgoing: DEFAULT_CUT_MM },
        }
    }
}

impl CornerDraft {
    /// The type of this row, so the labels draw the current choice.
    const fn kind(self) -> CornerType {
        match self {
            Self::Square => CornerType::Square,
            Self::Radius { .. } => CornerType::Radius,
            Self::Chamfer { .. } => CornerType::Chamfer,
        }
    }

    /// The corner treatment the document stores, or `None` for a square corner or a bad number.
    fn feature(self) -> Option<CornerFeatureKind> {
        match self {
            Self::Square => None,
            Self::Radius { radius } => Some(CornerFeatureKind::Radius { radius: positive_side(radius)? }),
            Self::Chamfer { incoming, outgoing } => Some(CornerFeatureKind::Chamfer {
                incoming: positive_side(incoming)?,
                outgoing: positive_side(outgoing)?,
            }),
        }
    }
}

/// A positive whole-millimetre length, or `None` when the number is not one.
fn positive_side(millimetres: f64) -> Option<Length> {
    if !millimetres.is_finite() || millimetres <= 0.0 {
        return None;
    }
    let value = Length::from_millimetres(millimetres.round() as i64)?;
    value.is_positive().then_some(value)
}

/// What the maker asked for, ready for `product.create`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProductDraft {
    /// The authored width, depth and height of the reserved box.
    pub envelope: Envelope,
    /// The product class; the general-product placeholder until another is chosen.
    pub class: ClassCode,
}

/// The open card and what the maker has typed into it.
///
/// The three numbers and the query are drafts: they leave no trace in the document, and a cancelled
/// card keeps them for the next opening.
#[derive(Debug)]
pub struct ProductCard {
    open: bool,
    width: f64,
    depth: f64,
    height: f64,
    /// One cut per nominal corner, in [`RectCorner::ALL`] order; square until the maker says otherwise.
    corners: [CornerDraft; 4],
    class: ClassCode,
    query: String,
}

impl Default for ProductCard {
    fn default() -> Self {
        Self {
            open: false,
            width: DEFAULT_WIDTH_MM,
            depth: DEFAULT_DEPTH_MM,
            height: DEFAULT_HEIGHT_MM,
            corners: [CornerDraft::Square; 4],
            class: ClassCode::GENERAL_PRODUCT,
            query: String::new(),
        }
    }
}

impl ProductCard {
    /// Creates a closed card with the default sides and the placeholder class.
    pub fn new() -> Self {
        Self::default()
    }

    /// Opens the card with a fresh search; the sides and the class stay as they were left.
    pub fn start(&mut self) {
        self.open = true;
        self.query.clear();
    }

    /// Whether the card is up, so the window can keep the keys for it.
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Closes the card without creating anything.
    pub fn close(&mut self) {
        self.open = false;
    }

    /// The envelope the fields describe, or the refusal that says why they do not describe one.
    ///
    /// The sides are rounded to the millimetre the field steps by, so a frame is never half a
    /// millimetre out; the footprint is validated against those sides, which is where a cut that
    /// consumes a whole edge is refused.
    pub fn envelope(&self) -> Result<Envelope, EnvelopeError> {
        let side = |millimetres: f64| Length::from_millimetres(millimetres.round() as i64);
        let missing = |side: &'static str| EnvelopeError::NonPositive { side, value: Length::ZERO };
        let width = side(self.width).ok_or_else(|| missing("width"))?;
        let depth = side(self.depth).ok_or_else(|| missing("depth"))?;
        let height = side(self.height).ok_or_else(|| missing("height"))?;
        let mut cuts = [None; 4];
        for (index, corner) in self.corners.iter().enumerate() {
            let Some(slot) = cuts.get_mut(index) else {
                continue;
            };
            *slot = match corner.kind() {
                CornerType::Square => None,
                CornerType::Radius | CornerType::Chamfer => Some(corner.feature().ok_or_else(|| missing("corner"))?),
            };
        }
        let footprint = if cuts.iter().all(Option::is_none) { Footprint::rectangle() } else { Footprint::cut(cuts) };
        Envelope::with_footprint(width, depth, height, footprint)
    }

    /// The draft the fields describe, or `None` when they do not describe a valid envelope.
    pub fn draft(&self) -> Option<ProductDraft> {
        let envelope = self.envelope().ok()?;
        Some(ProductDraft { envelope, class: self.class })
    }

    /// Whether a corner is cut, so an unusable shape is worth explaining under the block.
    fn has_cut(&self) -> bool {
        self.corners.iter().any(|corner| corner.kind() != CornerType::Square)
    }

    /// Draws the open card and reports the draft the maker confirmed.
    ///
    /// Escape closes the card without a draft; Enter (while no field owns the keyboard) and the
    /// *Create* button confirm it. The card stays open when the draft cannot be built, so a bad
    /// number is fixed in place instead of dropping the work.
    pub fn show(&mut self, ctx: &egui::Context) -> Option<ProductDraft> {
        if !self.open {
            return None;
        }
        if ctx.input(|input| input.key_pressed(Key::Escape)) {
            self.open = false;
            return None;
        }
        let lang = crate::language(ctx);
        let mut chosen = None;
        Modal::new(egui::Id::unique("product-frame-card")).show(ctx, |ui| {
            ui.set_width(WIDTH);
            ui.heading(Message::ProductCardTitle.text(lang));
            ui.separator();

            // Three sides in whole millimetres: the field rounds a drag and a typed value alike to
            // the step the document stores, so a frame is never half a millimetre out. A grid sizes
            // the label column once, so the three boxes start on one x and a number is read down a
            // column instead of being hunted past a longer word.
            egui::Grid::new("product-frame-card.sides")
                .num_columns(2)
                .min_col_width(LABEL_WIDTH)
                .spacing([8.0, 6.0])
                .show(ui, |ui| {
                    for (label, value) in [
                        (Message::Width, &mut self.width),
                        (Message::Depth, &mut self.depth),
                        (Message::Height, &mut self.height),
                    ] {
                        ui.label(label.text(lang));
                        // The field and its unit are one horizontal: the shared box draws the unit
                        // beside itself, and a grid cell cannot take that space.
                        ui.horizontal(|ui| {
                            crate::scrub::Scrub::new(value)
                                .speed(0.5)
                                .range(1.0..=max_side_mm())
                                .decimals(0)
                                .unit("mm")
                                .show(ui);
                        });
                        ui.end_row();
                    }
                });

            ui.separator();
            // The footprint: four corner rows, and the type is each row's first decision. A square
            // corner asks for nothing, so the card stays three numbers until a maker cuts something.
            ui.label(Message::SectionFootprint.text(lang));
            egui::Grid::new("product-frame-card.corners")
                .num_columns(2)
                .min_col_width(LABEL_WIDTH)
                .spacing([8.0, 6.0])
                .show(ui, |ui| {
                    for (index, corner) in RectCorner::ALL.into_iter().enumerate() {
                        let Some(current) = self.corners.get(index).copied() else {
                            continue;
                        };
                        let mut draft = current;
                        ui.label(Message::CornerWith.with(lang, [corner.label()]));
                        ui.horizontal(|ui| {
                            for option in CornerType::ALL {
                                let chosen = draft.kind() == option;
                                if ui.selectable_label(chosen, option.label().text(lang)).clicked() && !chosen {
                                    draft = option.default_draft();
                                }
                            }
                            match draft {
                                CornerDraft::Square => {}
                                CornerDraft::Radius { mut radius } => {
                                    crate::scrub::Scrub::new(&mut radius)
                                        .speed(0.5)
                                        .range(0.1..=max_side_mm())
                                        .decimals(0)
                                        .unit("mm")
                                        .show(ui);
                                    draft = CornerDraft::Radius { radius };
                                }
                                CornerDraft::Chamfer { mut incoming, mut outgoing } => {
                                    ui.label(Message::Inward.text(lang));
                                    crate::scrub::Scrub::new(&mut incoming)
                                        .speed(0.5)
                                        .range(0.1..=max_side_mm())
                                        .decimals(0)
                                        .unit("mm")
                                        .show(ui);
                                    ui.label(Message::Outward.text(lang));
                                    crate::scrub::Scrub::new(&mut outgoing)
                                        .speed(0.5)
                                        .range(0.1..=max_side_mm())
                                        .decimals(0)
                                        .unit("mm")
                                        .show(ui);
                                    draft = CornerDraft::Chamfer { incoming, outgoing };
                                }
                            }
                        });
                        if draft != current
                            && let Some(slot) = self.corners.get_mut(index)
                        {
                            *slot = draft;
                        }
                        ui.end_row();
                    }
                });
            // A cut that leaves no straight edge is the panel's own refusal, said here while the
            // numbers are still being chosen instead of only after a create that never ran.
            if self.has_cut()
                && let Err(error) = self.envelope()
            {
                ui.label(RichText::new(error.to_string()).color(theme::danger(ui)));
            }

            ui.separator();
            ui.horizontal(|ui| {
                ui.label(Message::PropertyKindClass.text(lang));
                // The chosen class is named beside the tree, so the placeholder it defaults to is
                // visible even while the tree is folded away from its row.
                let current = self.class.label().map(|label| label.text(lang)).unwrap_or_default();
                ui.label(RichText::new(current).strong());
            });
            // Where the piece stands is the class's rule, not a question (ADR 0097): the card says it
            // and offers no field. A class without the trait draws no row at all, because a guess here
            // would be a second source of truth beside the registry.
            if let Some(position) = self.class.traits().and_then(|traits| traits.mount()) {
                ui.label(RichText::new(position.label().text(lang)).color(theme::text_dim(ui)));
            }
            ui.add(
                TextEdit::singleline(&mut self.query)
                    .hint_text(Message::ClassPickerSearch.text(lang))
                    .desired_width(f32::INFINITY),
            );
            let rows = crate::class_picker::product_rows(&self.query, lang);
            let force_open = !self.query.trim().is_empty();
            ui.spacing_mut().item_spacing.y = 2.0;
            ScrollArea::vertical().max_height(MAX_LIST_HEIGHT).show(ui, |ui| {
                if rows.is_empty() {
                    ui.label(RichText::new(Message::ClassPickerNoMatch.text(lang)).color(theme::text_dim(ui)));
                    return;
                }
                for family in &rows {
                    self.show_family(ui, family, force_open);
                }
            });

            ui.separator();
            ui.horizontal(|ui| {
                let cancel = ui.button(Message::Cancel.text(lang));
                if cancel.clicked() {
                    self.open = false;
                }
                let create = ui.add_enabled(self.draft().is_some(), Button::new(Message::GroupCreate.text(lang)));
                let entered = ui.ctx().input(|input| input.key_pressed(Key::Enter)) && !ui.ctx().text_edit_focused();
                if (create.clicked() || entered) && self.draft().is_some() {
                    chosen = self.draft();
                }
            });
        });
        if chosen.is_some() {
            self.open = false;
        }
        chosen
    }

    /// Draws one family of the class tree and the types folded under it.
    ///
    /// The family is a caption, not a class a piece can take, so its row folds on a click instead of
    /// selecting. A search opens every group it kept: a matching row hidden behind a fold is a
    /// result the maker cannot see.
    fn show_family(&mut self, ui: &mut Ui, family: &ProductFamily, force_open: bool) {
        let mut folded = false;
        let mut header =
            CollapsingState::load_with_default_open(ui.ctx(), fold_id(family.code), true).show_header(ui, |ui| {
                let caption = ui.label(RichText::new(&family.label).strong().color(theme::text_dim(ui)));
                folded = caption.clicked();
            });
        if force_open {
            header.set_open(true);
        } else if folded {
            header.toggle();
        }
        let _ = header.body(|ui| {
            for kind in &family.types {
                self.show_type(ui, kind, force_open);
            }
        });
    }

    /// Draws one product type: a selectable row, and the variants folded under it.
    ///
    /// A type is a class in its own right — the cabinet is one before it is a base or a wall
    /// cabinet — so the row selects it and the arrow alone folds the variants away. The type that
    /// holds the chosen class starts open, so the card never hides the row it says is current.
    fn show_type(&mut self, ui: &mut Ui, kind: &ProductType, force_open: bool) {
        if kind.variants.is_empty() {
            self.class_row(ui, kind.code, &kind.label);
            return;
        }
        let default_open = self.class.is_ancestor_or_self(kind.code);
        let mut header = CollapsingState::load_with_default_open(ui.ctx(), fold_id(kind.code), default_open)
            .show_header(ui, |ui| {
                self.class_row(ui, kind.code, &kind.label);
            });
        if force_open {
            header.set_open(true);
        }
        let _ = header.body(|ui| {
            for (code, label) in &kind.variants {
                self.class_row(ui, *code, label);
            }
        });
    }

    /// One selectable class row, drawn like a scene-tree row.
    ///
    /// A flat row with a wash on hover and on the class the piece carries, and the label at the
    /// left: the tree the owner reads it beside puts its words on one column, and a centred button
    /// would put the cabinet under the middle of the card instead.
    fn class_row(&mut self, ui: &mut Ui, code: ClassCode, label: &str) {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), ROW_HEIGHT), egui::Sense::hover());
        let id = ui.make_persistent_id(("product.card.row", code.value()));
        let response = ui.interact(rect, id, egui::Sense::click());
        let selected = code == self.class;
        let hovered = ui.rect_contains_pointer(rect);
        let visuals = ui.style().interact_selectable(&response, selected);
        if selected || hovered {
            ui.painter().rect_filled(rect, egui::CornerRadius::same(theme::RADIUS), visuals.weak_bg_fill);
        }
        let padding = ui.spacing().button_padding.x;
        let galley = egui::WidgetText::from(RichText::new(label)).into_galley(
            ui,
            Some(egui::TextWrapMode::Truncate),
            (rect.width() - 2.0 * padding).max(0.0),
            egui::TextStyle::Button,
        );
        let elided = galley.elided;
        let position = egui::pos2(rect.left() + padding, rect.center().y - 0.5 * galley.size().y);
        ui.painter().with_clip_rect(rect).galley(position, galley, visuals.text_color());
        if response.clicked() {
            self.class = code;
        }
        if elided {
            response.on_hover_text(label);
        }
    }
}

/// The id the class tree remembers one row's fold under.
///
/// A global id, not `ui.make_persistent_id`: the card is drawn in one place, so the fold survives a
/// closed and reopened card and a test can set it without knowing the modal's generated id.
fn fold_id(code: ClassCode) -> egui::Id {
    egui::Id::unique(("product.card.class", code.value()))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use text::Lang;

    use super::*;

    fn screen() -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))),
            ..Default::default()
        }
    }

    fn drawn_text_rects(shapes: &[egui::epaint::ClippedShape]) -> Vec<(String, egui::Rect)> {
        use egui::epaint::Shape;
        fn walk(shape: &Shape, out: &mut Vec<(String, egui::Rect)>) {
            match shape {
                Shape::Text(text) => {
                    let rect = egui::Rect::from_min_size(text.pos, text.galley.size());
                    out.push((text.galley.text().to_owned(), rect));
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

    fn drawn_text(shapes: &[egui::epaint::ClippedShape]) -> Vec<String> {
        drawn_text_rects(shapes).into_iter().map(|(text, _rect)| text).collect()
    }

    /// Draws the card for a few frames and returns every text the last one painted.
    fn present(card: &mut ProductCard, lang: Lang) -> (Vec<String>, Option<ProductDraft>) {
        let ctx = egui::Context::default();
        theme::install(&ctx);
        crate::set_language(&ctx, lang);
        let mut draft = None;
        let mut texts = Vec::new();
        for _ in 0..2 {
            let output = ctx.run_ui(screen(), |_ui| {
                draft = card.show(&ctx);
            });
            texts = drawn_text(&output.shapes);
            output.drop_without_applying_deltas();
        }
        (texts, draft)
    }

    #[test]
    fn the_card_asks_for_three_sides_and_lists_the_product_classes() {
        let mut card = ProductCard::new();
        card.start();
        let (texts, draft) = present(&mut card, Lang::En);

        assert!(draft.is_none(), "drawing the card creates nothing");
        assert!(texts.iter().any(|text| text == "New product frame"), "the card names itself: {texts:?}");
        for label in ["Width", "Depth", "Height"] {
            assert!(texts.iter().any(|text| text == label), "the card asks for {label}: {texts:?}");
        }
        assert!(
            texts.iter().any(|text| text.contains("General product")),
            "the placeholder class is listed: {texts:?}"
        );
        assert!(texts.iter().any(|text| text == "Class"), "the list is captioned: {texts:?}");
        assert!(texts.iter().any(|text| text == "Create"), "the card can be confirmed: {texts:?}");
        assert!(
            !texts.iter().any(|text| text.contains("ready for parts")),
            "the paragraph under the title is gone: {texts:?}"
        );
    }

    #[test]
    fn the_card_says_the_position_the_class_gives_and_never_asks() {
        let mut card = ProductCard::new();
        card.start();
        // The placeholder is a product class, so it stands on the floor: the card reports the rule
        // its class carries instead of asking for one.
        let (texts, _) = present(&mut card, Lang::En);
        assert!(texts.iter().any(|text| text == "On the floor"), "the placeholder stands: {texts:?}");
        assert!(!texts.iter().any(|text| text == "On the wall"), "and hangs nowhere: {texts:?}");

        card.class = "10.01.02".parse().expect("the wall cabinet");
        let (texts, _) = present(&mut card, Lang::En);
        assert!(texts.iter().any(|text| text == "On the wall"), "a wall cabinet hangs: {texts:?}");
        assert!(!texts.iter().any(|text| text == "On the floor"), "and does not stand: {texts:?}");

        // A fitting carries no mount, so the row is absent rather than guessed.
        card.class = "50.02".parse().expect("a cup hinge");
        let (texts, _) = present(&mut card, Lang::En);
        assert!(
            !texts.iter().any(|text| text == "On the floor" || text == "On the wall"),
            "a class without a mount draws no position row: {texts:?}"
        );
    }

    /// How many number fields the card draws: each one paints its unit.
    fn fields(texts: &[String]) -> usize {
        texts.iter().filter(|text| text.as_str() == "mm").count()
    }

    #[test]
    fn a_corner_asks_for_its_numbers_only_after_its_type_is_chosen() {
        let mut card = ProductCard::new();
        card.start();
        let (texts, _) = present(&mut card, Lang::En);
        assert!(texts.iter().any(|text| text == "Base outline"), "the footprint is captioned: {texts:?}");
        for label in ["Corner 1", "Corner 2", "Corner 3", "Corner 4"] {
            assert!(texts.iter().any(|text| text == label), "the card names {label}: {texts:?}");
        }
        for kind in ["None", "Radius", "Chamfer"] {
            assert!(texts.iter().any(|text| text == kind), "the corner type offers {kind}: {texts:?}");
        }
        assert_eq!(fields(&texts), 3, "three sides ask for a number and a square corner asks for none: {texts:?}");

        // The fillet is chosen first and only then does its radius appear.
        card.corners[1] = CornerDraft::Radius { radius: 80.0 };
        let (texts, _) = present(&mut card, Lang::En);
        assert_eq!(fields(&texts), 4, "a fillet asks for one radius: {texts:?}");
        assert!(texts.iter().any(|text| text == "80"), "the radius is the number chosen: {texts:?}");

        // A chamfer asks for its two setbacks along the edges, never for an angle.
        card.corners[1] = CornerDraft::Chamfer { incoming: 120.0, outgoing: 70.0 };
        let (texts, _) = present(&mut card, Lang::En);
        assert_eq!(fields(&texts), 5, "a chamfer asks for two offsets: {texts:?}");
        for label in ["in", "out"] {
            assert!(texts.iter().any(|text| text == label), "the chamfer names the edge {label}: {texts:?}");
        }
        assert!(texts.iter().any(|text| text == "120") && texts.iter().any(|text| text == "70"), "{texts:?}");
    }

    #[test]
    fn the_draft_carries_the_cut_offsets_and_no_footprint_when_every_corner_is_square() {
        let mut card = ProductCard::new();
        card.start();
        let plain = card.draft().expect("the defaults are a valid envelope");
        assert!(plain.envelope.footprint().is_rectangle(), "a plain frame stores no footprint at all");

        card.corners[1] = CornerDraft::Chamfer { incoming: 120.0, outgoing: 70.0 };
        card.corners[3] = CornerDraft::Radius { radius: 60.0 };
        let cut = card.draft().expect("an oblique corner is a valid envelope");
        let corners = cut.envelope.footprint().cuts();
        assert_eq!(
            corners,
            [
                None,
                Some(CornerFeatureKind::Chamfer {
                    incoming: Length::from_millimetres(120).expect("a side"),
                    outgoing: Length::from_millimetres(70).expect("a side"),
                }),
                None,
                Some(CornerFeatureKind::Radius { radius: Length::from_millimetres(60).expect("a side") }),
            ],
            "the offsets travel as lengths, in RectCorner order"
        );
        assert_eq!(
            cut.envelope.footprint().corner_cut(RectCorner::TopLeft),
            Some(CornerFeatureKind::Radius { radius: Length::from_millimetres(60).expect("a side") })
        );
    }

    #[test]
    fn a_cut_that_eats_a_whole_edge_is_refused_in_the_card() {
        let mut card = ProductCard::new();
        card.start();
        // Two 350 mm setbacks on one 600 mm edge leave nothing straight between them.
        card.corners[0] = CornerDraft::Chamfer { incoming: 10.0, outgoing: 350.0 };
        card.corners[1] = CornerDraft::Chamfer { incoming: 350.0, outgoing: 10.0 };
        assert!(card.draft().is_none(), "an unusable cut makes no draft, so Create stays disabled");
        let (texts, _) = present(&mut card, Lang::En);
        assert!(
            texts.iter().any(|text| text.contains("no straight edge")),
            "the card says why the cut is refused: {texts:?}"
        );
    }

    #[test]
    fn the_three_sides_share_one_label_column_and_one_field_column() {
        let ctx = egui::Context::default();
        theme::install(&ctx);
        crate::set_language(&ctx, Lang::En);
        let mut card = ProductCard::new();
        card.start();
        let mut painted = Vec::new();
        for _ in 0..2 {
            let output = ctx.run_ui(screen(), |_ui| {
                let _ = card.show(&ctx);
            });
            painted = drawn_text_rects(&output.shapes);
            output.drop_without_applying_deltas();
        }
        let rect = |text: &str| {
            painted
                .iter()
                .find(|(painted, _rect)| painted == text)
                .map(|(_text, rect)| *rect)
                .unwrap_or_else(|| panic!("the card paints {text:?}: {painted:?}"))
        };

        let width = rect("Width");
        let depth = rect("Depth");
        let height = rect("Height");
        assert_eq!(depth.left(), width.left(), "the labels share one column: {width:?} {depth:?}");
        assert_eq!(height.left(), width.left(), "the labels share one column: {width:?} {height:?}");

        let first = rect("600");
        let second = rect("560");
        let third = rect("720");
        assert_eq!(second.right(), first.right(), "the fields share one column: {first:?} {second:?}");
        assert_eq!(third.right(), first.right(), "the fields share one column: {first:?} {third:?}");
    }

    #[test]
    fn a_folded_type_lists_no_variant_and_the_chosen_branch_opens() {
        let mut card = ProductCard::new();
        card.start();
        // The card opens on the placeholder, which is a leaf, so the cabinet's variants are folded.
        let (texts, _) = present(&mut card, Lang::En);
        assert!(texts.iter().any(|text| text.contains("Cabinet")), "the type row is listed: {texts:?}");
        assert!(!texts.iter().any(|text| text.contains("Base cabinet")), "a folded type lists no variant: {texts:?}");

        // The type of the chosen class opens by itself, so the row the card calls current is never
        // hidden behind a fold.
        card.class = "10.01.02".parse().expect("the wall cabinet");
        let (texts, _) = present(&mut card, Lang::En);
        for row in ["Base cabinet", "Wall cabinet", "Tall cabinet"] {
            assert!(texts.iter().any(|text| text.contains(row)), "{row} is under the open type: {texts:?}");
        }

        // A search opens every group it kept: a match behind a fold is a result nobody can see.
        card.class = ClassCode::GENERAL_PRODUCT;
        card.query = "cabinet".to_owned();
        let (texts, _) = present(&mut card, Lang::En);
        assert!(texts.iter().any(|text| text.contains("Base cabinet")), "a matched group opens: {texts:?}");
    }

    #[test]
    fn a_search_narrows_the_class_list() {
        let mut card = ProductCard::new();
        card.start();
        card.query = "no such product".to_owned();
        let (texts, _) = present(&mut card, Lang::En);
        assert!(texts.iter().any(|text| text == "No class matches."), "a query with no answer says so: {texts:?}");
        assert!(!texts.iter().any(|text| text.starts_with("70.01")), "and hides the rows: {texts:?}");

        card.query = "general".to_owned();
        let (texts, _) = present(&mut card, Lang::En);
        assert!(
            texts.iter().any(|text| text.starts_with("70.01") && text.contains("General product")),
            "a matching query brings the row back: {texts:?}"
        );
    }

    #[test]
    fn the_draft_carries_the_three_whole_millimetre_sides_and_the_placeholder_class() {
        let mut card = ProductCard::new();
        card.start();
        card.width = 599.6;
        card.height = 2000.0;
        let (_, draft) = present(&mut card, Lang::En);
        assert!(draft.is_none(), "presenting alone creates nothing");

        let draft = card.draft().expect("the defaults are a valid envelope");
        assert_eq!(draft.envelope.x(), Length::from_millimetres(600).expect("a side"));
        assert_eq!(draft.envelope.y(), Length::from_millimetres(560).expect("a side"));
        assert_eq!(draft.envelope.z(), Length::from_millimetres(2000).expect("a side"));
        assert_eq!(draft.class, ClassCode::GENERAL_PRODUCT);
    }

    #[test]
    fn escape_closes_the_card_and_enter_creates() {
        let ctx = egui::Context::default();
        theme::install(&ctx);
        crate::set_language(&ctx, Lang::En);
        let mut card = ProductCard::new();
        card.start();

        let key = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let enter = ctx.run_ui(egui::RawInput { events: vec![key(egui::Key::Enter)], ..screen() }, |_ui| {
            let draft = card.show(&ctx);
            assert!(draft.is_some(), "Enter confirms the card");
        });
        enter.drop_without_applying_deltas();
        assert!(!card.is_open(), "a confirmed card closes");

        card.start();
        let escape = ctx.run_ui(egui::RawInput { events: vec![key(egui::Key::Escape)], ..screen() }, |_ui| {
            assert!(card.show(&ctx).is_none(), "Escape creates nothing");
        });
        escape.drop_without_applying_deltas();
        assert!(!card.is_open(), "and the card closes");
    }
}
