//! The render material slot of a surface-bearing node: one picker, two inspector cards.
//!
//! A panel and an object each wear one record of the workshop's render library (ADR 0245), and the
//! slot is edited where the node is: the panel's card beside its catalogue material, the object's
//! card between the head and its numbers. Both draw this row and neither keeps its own copy, so the
//! two cards cannot offer a different list of materials.
//!
//! The row is a view: it reads [`Session::materials`] and answers the slot a person chose, and the
//! card turns that answer into one `node.material.set` — one revision, one undo entry. A node whose
//! record the library has lost keeps the number it names and says so in the danger ink, because a
//! scene travels between workshops with the ids it carried in (ADR 0245, ADR 0251).

use egui::{ComboBox, RichText, Ui};
use engine::Session;
use materials::{Library, MaterialId};
use model::{Id, MaterialRef};
use text::{Lang, Message};

use crate::theme;

/// How wide the picker's face is, in points: wide enough for the longest name a record may carry.
const PICKER_WIDTH: f32 = 180.0;

/// Draws the node's material picker and answers the slot chosen this frame.
///
/// `None` means nothing was chosen; `Some(None)` clears the slot, so a node with no material is the
/// same edit as a node with a different one. The first row is the clear, the rest are the library's
/// records in its own order, and an empty library says so once instead of offering nothing.
pub(crate) fn row(
    ui: &mut Ui,
    session: &Session,
    node: Id,
    current: Option<MaterialRef>,
) -> Option<Option<MaterialRef>> {
    let lang = crate::language(ui.ctx());
    let mut chosen = None;
    ui.horizontal(|ui| {
        ui.label(Message::SectionSurface.text(lang));
        chosen = picker(ui, session, node, current);
    });
    sentences(ui, session, current);
    chosen
}

/// The same picker for a card that folds its rows under a section title, so the label is not said
/// twice: the panel card's `Surface` section draws this.
pub(crate) fn field(
    ui: &mut Ui,
    session: &Session,
    node: Id,
    current: Option<MaterialRef>,
) -> Option<Option<MaterialRef>> {
    let chosen = picker(ui, session, node, current);
    sentences(ui, session, current);
    chosen
}

/// The closed picker and its list: the clear first, then the library's records by name.
fn picker(ui: &mut Ui, session: &Session, node: Id, current: Option<MaterialRef>) -> Option<Option<MaterialRef>> {
    let lang = crate::language(ui.ctx());
    let library = session.materials();
    let mut chosen = None;
    ComboBox::from_id_salt(("node_material_slot", node.get()))
        .width(PICKER_WIDTH)
        .selected_text(face(library, current, lang))
        .show_ui(ui, |ui| {
            if ui.selectable_label(current.is_none(), Message::NoMaterial.text(lang)).clicked() && current.is_some() {
                chosen = Some(None);
            }
            for (id, name) in listing(library) {
                let mine = wears(current, id);
                if ui.selectable_label(mine, name).clicked() && !mine {
                    chosen = MaterialRef::new(id.get()).map(Some);
                }
            }
        });
    chosen
}

/// The rows the picker offers, in the library's own order.
///
/// A record is offered by the name a person reads: the library refuses two records that share one,
/// so a name is a key and the picker needs no second column (ADR 0244).
fn listing(library: &Library) -> impl Iterator<Item = (MaterialId, &str)> {
    library.records().map(|record| (record.id(), record.name()))
}

/// What the row says under the picker: an empty library once, or the number a lost record left.
fn sentences(ui: &mut Ui, session: &Session, current: Option<MaterialRef>) {
    let lang = crate::language(ui.ctx());
    let library = session.materials();
    if library.is_empty() {
        ui.label(RichText::new(Message::MaterialsEmpty.text(lang)).color(theme::text_dim(ui)));
    } else if let Some(reference) = current
        && record_of(library, reference).is_none()
    {
        ui.colored_label(theme::danger(ui), Message::MaterialSlotMissing.with(lang, [reference]));
    }
}

/// What the closed picker prints: the record's name, the fact that a slot is empty, or the number
/// the library no longer holds.
fn face(library: &Library, current: Option<MaterialRef>, lang: Lang) -> String {
    match current {
        None => Message::NoMaterial.text(lang).to_owned(),
        Some(reference) => match record_of(library, reference) {
            Some(record) => record.name().to_owned(),
            None => Message::MissingMaterial.with(lang, [reference]),
        },
    }
}

/// Whether the node's slot names this record.
fn wears(current: Option<MaterialRef>, record: MaterialId) -> bool {
    current.and_then(|reference| MaterialId::new(reference.get())) == Some(record)
}

/// The record a reference names, or `None` when this library holds no such material.
///
/// The document names the number the library handed out (ADR 0245), so the conversion is the only
/// place the two types meet inside the interface.
fn record_of(library: &Library, reference: MaterialRef) -> Option<&materials::Material> {
    library.get(MaterialId::new(reference.get())?)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    fn library(names: &[(&str, u32)]) -> Library {
        let records = names.iter().map(|(name, id)| {
            let id = MaterialId::new(*id).expect("a non-zero id");
            let base = materials::Base::matte([0.48, 0.36, 0.24]).expect("a matte base");
            materials::Material::of_base(id, *name, base).expect("a record")
        });
        Library::new(records).expect("a library")
    }

    #[test]
    fn the_face_names_the_record_the_slot_holds_and_the_lost_number_a_gone_one() {
        let lang = Lang::En;
        let filled = library(&[("Oak natural", 1)]);
        let oak = MaterialRef::new(1).expect("a material");
        assert_eq!(face(&filled, Some(oak), lang), "Oak natural");
        assert_eq!(face(&filled, None, lang), "No material");
        let gone = MaterialRef::new(7).expect("a material");
        assert_eq!(face(&filled, Some(gone), lang), "Missing material 7", "the number is what is left");
        assert!(wears(Some(oak), MaterialId::new(1).expect("id")), "the slot names the record");
        assert!(!wears(Some(gone), MaterialId::new(1).expect("id")), "another record is another slot");
        assert!(record_of(&filled, gone).is_none(), "the library has no such material");
        assert!(record_of(&filled, oak).is_some(), "and it has this one");
    }

    #[test]
    fn the_picker_lists_the_library_by_name_and_offers_nothing_when_it_is_empty() {
        let filled = library(&[("Oak natural", 1), ("Walnut", 2)]);
        let names: Vec<&str> = listing(&filled).map(|(_, name)| name).collect();
        assert_eq!(names, ["Oak natural", "Walnut"], "the list is the library's own order and its names");
        let ids: Vec<u32> = listing(&filled).map(|(id, _)| id.get()).collect();
        assert_eq!(ids, [1, 2], "the id is what the slot stores");
        assert_eq!(listing(&Library::default()).count(), 0, "an empty library offers no row to choose");
    }
}
