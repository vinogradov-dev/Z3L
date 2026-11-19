//! The viewport's four gestures, and the key each one answers.
//!
//! The transform mode is view state: it belongs to the window rather than to a document, it is never
//! serialized and a change of mode writes no undo entry (ADR 0113). It is named here rather than in
//! the shell because three surfaces read it — the status bar's band, the node menu the tree and the
//! viewport draw, and the Settings pages — and a value offered by two hand-written lists drifts apart
//! the moment either moves.
//!
//! The four are one band and four keys (U113): the mode answers its own row of the inventory's key
//! table, so the cell, the menu entry and the Settings row print one spelling and the shell consumes
//! the pair the table holds.
//!
//! Changes when: a gesture is added to or removed from the viewport, or a mode's words move.

use text::{Lang, Message};

use crate::inventory::{self, Binding};

/// Which transform gesture the viewport offers, if any.
///
/// `Select` is the state a window opens in, so a person who only wants to look, pick and read is
/// never handed a handle they did not ask for (ADR 0113).
///
/// The two moves are two tools and not one tool that guesses (N22, ADR 0147): `Move` keeps the
/// pointer's own distance and applies no rule, and `Object` moves any body by the document's own
/// rules — the axes a furniture piece allows, the neighbours that stop it, and the workspace's
/// snapping (ADR 0165).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TransformMode {
    /// No transform: a click selects, shift adds, a double click enters a level.
    #[default]
    Select,
    /// Three world-axis arrows move the selection freely, with no rule and no snapping.
    Move,
    /// The arrows move a body by the document's own rules: a panel along its thickness, a floor-bound
    /// frame in its own plane, a solid body until its face meets a neighbour's, and every drag landing
    /// on the workspace's snaps (ADR 0165).
    Object,
    /// Three local-axis rings turn the selection about its origin.
    Rotate,
}

impl TransformMode {
    /// Every mode, in the order the band and the menu list them.
    pub const ALL: [Self; 4] = [Self::Select, Self::Move, Self::Object, Self::Rotate];

    /// The inventory id the mode is filed under, and the id its key runs.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Select => inventory::TRANSFORM_SELECT,
            Self::Move => inventory::TRANSFORM_MOVE,
            Self::Object => inventory::TRANSFORM_OBJECT,
            Self::Rotate => inventory::TRANSFORM_ROTATE,
        }
    }

    /// The words the band's cell, the menu entry and the Settings row print.
    pub const fn words(self) -> Message {
        match self {
            Self::Select => Message::Select,
            Self::Move => Message::Move,
            Self::Object => Message::Object,
            Self::Rotate => Message::Rotate,
        }
    }

    /// The mode's name in `lang`.
    pub const fn label(self, lang: Lang) -> &'static str {
        self.words().text(lang)
    }

    /// The words of the sentence that says which gesture the mode offers.
    ///
    /// Each of the four carries one, because two moves side by side read as the same word: the
    /// difference between them is the rule, not the tool.
    pub const fn hint_words(self) -> Message {
        match self {
            Self::Select => Message::SelectHint,
            Self::Move => Message::MoveHint,
            Self::Object => Message::ObjectHint,
            Self::Rotate => Message::RotateHint,
        }
    }

    /// That sentence in `lang`, which is what a hover shows.
    pub const fn hint(self, lang: Lang) -> &'static str {
        self.hint_words().text(lang)
    }

    /// The icon the mode draws beside its word, when its artwork exists.
    ///
    /// The object move is the one mode with a drawing so far; the other three answer `None`, and a
    /// cell without an image is the cell it always was (`assets/README.md`).
    pub fn icon(self) -> Option<crate::icons::Icon> {
        match self {
            Self::Object => Some(crate::icons::transform::OBJECT),
            Self::Select | Self::Move | Self::Rotate => None,
        }
    }

    /// The binding the key table files under this mode's id.
    pub fn binding(self) -> Option<&'static Binding> {
        inventory::binding(self.id())
    }

    /// The printed spelling of the key that arms this mode.
    pub fn key(self) -> Option<&'static str> {
        inventory::key_label(self.id())
    }

    /// The cell the status bar draws: the name and, beside it in the small face, its key.
    ///
    /// The key is small and quiet because it is a reminder and not a second label: the band is
    /// chrome on the narrowest window the shell opens, and a key printed at the size of the name
    /// would push the last cell under the readouts on the right (U113).
    pub fn cell(self, ui: &egui::Ui, lang: Lang) -> egui::WidgetText {
        let (name, key) = match self.key() {
            Some(key) => (self.label(lang), format!("  ({key})")),
            None => (self.label(lang), String::new()),
        };
        let style = ui.style();
        let body = style.text_styles.get(&egui::TextStyle::Body).cloned().unwrap_or_default();
        let small = style.text_styles.get(&egui::TextStyle::Small).cloned().unwrap_or_default();
        let mut job = egui::text::LayoutJob::default();
        let plain = egui::TextFormat { font_id: body, color: egui::Color32::PLACEHOLDER, ..Default::default() };
        job.append(name, 0.0, plain);
        if !key.is_empty() {
            let quiet = egui::TextFormat { font_id: small, color: crate::theme::text_dim(ui), ..Default::default() };
            job.append(&key, 0.0, quiet);
        }
        job.into()
    }

    /// The letters the cell draws, as one string.
    ///
    /// The cell splits them into two faces and this does not, so a test and a hint can name the
    /// cell by the words a person reads without building an `egui` layout to do it.
    pub fn caption(self, lang: Lang) -> String {
        match self.key() {
            Some(key) => format!("{}  ({key})", self.label(lang)),
            None => self.label(lang).to_owned(),
        }
    }
}

/// The mode a key press or a menu entry names, if `id` is one of the four.
///
/// A menu entry knows its mode, so this exists for the one direction a surface without the value
/// needs: the shell reads a key by id, and a test asks the table whether an id is a mode at all.
pub fn from_id(id: &str) -> Option<TransformMode> {
    TransformMode::ALL.into_iter().find(|mode| mode.id() == id)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    #[test]
    fn every_mode_carries_a_id_a_key_and_words_in_both_languages() {
        // The four are one band and four keys, so a mode with no id would be a cell that cannot be
        // bound and a mode with no key a cell that teaches nothing (U113).
        for mode in TransformMode::ALL {
            assert!(from_id(mode.id()) == Some(mode), "`{}` reads back as its mode", mode.id());
            let binding = mode.binding().expect("every mode is bound to a key");
            assert_eq!(binding.id, mode.id(), "a mode's key runs the mode's own id");
            assert_eq!(mode.key(), Some(binding.label), "the cell prints the table's spelling");
            for lang in [Lang::En, Lang::Ru] {
                assert!(!mode.label(lang).trim().is_empty(), "`{}` has no name", mode.id());
                assert!(!mode.hint(lang).trim().is_empty(), "`{}` has no hover sentence", mode.id());
            }
        }
    }

    #[test]
    fn an_id_that_is_no_mode_is_no_mode() {
        assert!(from_id("node.delete").is_none());
        assert!(from_id("").is_none());
    }

    #[test]
    fn the_band_starts_in_select() {
        assert_eq!(TransformMode::default(), TransformMode::Select, "a window opens in Select (ADR 0113)");
        assert_eq!(TransformMode::ALL[0], TransformMode::Select, "and the band lists it first");
    }

    #[test]
    fn a_cell_draws_the_caption_a_test_reads() {
        // The cell splits the words into two faces and the caption does not: the two must spell the
        // same thing, or a test that looks for a cell by its words looks for a cell nobody drew.
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let mut drawn = Vec::new();
        let output = ctx.run_ui(Default::default(), |ui| {
            for mode in TransformMode::ALL {
                for lang in [Lang::En, Lang::Ru] {
                    let text = mode.cell(ui, lang).text().to_owned();
                    drawn.push((mode, lang, text));
                }
            }
        });
        output.drop_without_applying_deltas();
        for (mode, lang, text) in drawn {
            assert_eq!(text, mode.caption(lang), "`{}` draws the caption it prints", mode.id());
        }
    }
}
