//! The card that creates a product frame: three sides and a type.
//!
//! A maker starts a piece by saying how big its reserved space is and what the piece is
//! (ADR 0100). The card is the numeric path to `product.create`: it draws the three sides in whole
//! millimetres on one label column, offers the registry's product classes as the same family/type
//! tree the class picker draws — folded, so the case a shop builds is a row and not a scroll — and
//! returns a [`ProductDraft`]. It never dispatches a command and never touches the document — the
//! editor turns the draft into `product.create` and then opens the new frame as a session view
//! operation (ADR 0098).
//!
//! The drawn gesture (a base rectangle and a pulled height) reaches the same command through the
//! box tool, which the ribbon offers as this tool's second method (ADR 0101): the two entries build
//! one document, and this card is not a second creation path, only the typed one.
//!
//! Changes when: the product frame gains a field, the class tree changes how it is folded or
//! searched, or the numeric defaults move.

use egui::collapsing_header::CollapsingState;
use egui::{Button, Key, Modal, RichText, ScrollArea, TextEdit, Ui};
use model::{ClassCode, Envelope};
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

    /// The draft the fields describe, or `None` when a side is not a valid envelope.
    pub fn draft(&self) -> Option<ProductDraft> {
        let side = |millimetres: f64| Length::from_millimetres(millimetres.round() as i64);
        let envelope = Envelope::new(side(self.width)?, side(self.depth)?, side(self.height)?).ok()?;
        Some(ProductDraft { envelope, class: self.class })
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
            ui.horizontal(|ui| {
                ui.label(Message::PropertyKindClass.text(lang));
                // The chosen class is named beside the tree, so the placeholder it defaults to is
                // visible even while the tree is folded away from its row.
                let current = self.class.label().map(|label| label.text(lang)).unwrap_or_default();
                ui.label(RichText::new(current).strong());
            });
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
