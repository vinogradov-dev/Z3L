//! The loaded library: every record, validated as one value.
//!
//! A reader holds one `Library` and the shell swaps it whole (`docs/domain/nomenclature.md` §13 for
//! the same rule): there is no half-loaded library, and a file that breaks a rule leaves the
//! records in use where they are. The records are indexed by their stable id, so a lookup costs one
//! comparison and the file's order is a person's reading order, not the renderer's.

use std::collections::{BTreeMap, HashSet};

use crate::id::MaterialId;
use crate::material::{Material, MaterialError};

/// Every material of one library, indexed by its stable id.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Library {
    records: BTreeMap<MaterialId, Material>,
}

impl Library {
    /// Validates the records and indexes them, refusing the first one that breaks a rule.
    ///
    /// An id two records claim and two records a person cannot tell apart by name are refused here,
    /// because the library is a shop's own list and a duplicate is a mistake in it, not a value the
    /// renderer could pick between.
    pub fn new(materials: impl IntoIterator<Item = Material>) -> Result<Self, MaterialError> {
        let mut records = BTreeMap::new();
        let mut names: HashSet<String> = HashSet::new();
        for material in materials {
            if records.contains_key(&material.id()) {
                return Err(MaterialError::Duplicate { id: material.id() });
            }
            if !names.insert(material.name().trim().to_lowercase()) {
                return Err(MaterialError::DuplicateName { name: material.name().to_owned() });
            }
            records.insert(material.id(), material);
        }
        Ok(Self { records })
    }

    /// The record `id` names, or `None` when this library holds no such material.
    ///
    /// A missing record is what the caller reports and never what it substitutes: a part that names
    /// material 7 does not become the eight one because the eight one exists
    /// (`docs/domain/nomenclature.md` §16).
    pub fn get(&self, id: MaterialId) -> Option<&Material> {
        self.records.get(&id)
    }

    /// Every record, in the order the ids sort.
    pub fn records(&self) -> impl Iterator<Item = &Material> {
        self.records.values()
    }

    /// The id a record this library has never held takes: the largest one plus one, or the first.
    ///
    /// Ids are never reused (a catalogue record points at one, and a scene stores the number a part
    /// was given), so a record made in the editor is filed under the next free number instead of a
    /// counter kept in the file. `None` when the largest id is the last number an id can carry, which
    /// no library a workshop fills reaches.
    pub fn next_id(&self) -> Option<MaterialId> {
        let last = self.records.keys().next_back().map_or(0, |id| id.get());
        MaterialId::new(last.saturating_add(1))
    }

    /// How many records the library holds.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Whether the library holds no record at all.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;
    use crate::material::{Base, Coat};

    fn id(value: u32) -> MaterialId {
        MaterialId::new(value).expect("a non-zero id")
    }

    fn veneer(value: u32, name: &str) -> Material {
        Material::of_base(id(value), name, Base::matte([0.6, 0.45, 0.25]).expect("a board")).expect("a record")
    }

    #[test]
    fn a_library_indexes_its_records_and_answers_a_missing_one_with_none() {
        let lacquer = Material::new(id(2), "Lacquer 90", None, Some(Coat::glossed(90.0).expect("a coat")));
        let library = Library::new([veneer(1, "Oak"), lacquer.expect("a coat record")]).expect("two records");
        assert_eq!(library.len(), 2);
        assert_eq!(library.get(id(1)).map(Material::name), Some("Oak"));
        assert_eq!(library.get(id(9)), None, "a missing record is never another material");
        assert_eq!(library.records().map(Material::id).collect::<Vec<_>>(), vec![id(1), id(2)], "by id");
    }

    #[test]
    fn a_new_record_takes_the_first_free_id_and_never_a_used_one() {
        assert_eq!(Library::default().next_id(), Some(id(1)), "the first material a shop makes is 1");
        let library = Library::new([veneer(3, "Oak"), veneer(1, "Beech")]).expect("two records");
        assert_eq!(library.next_id(), Some(id(4)), "past the largest, never into a gap a catalogue record may name");
    }

    #[test]
    fn a_duplicate_id_is_refused() {
        let error = Library::new([veneer(1, "Oak"), veneer(1, "Beech")]).expect_err("one id twice");
        assert_eq!(error, MaterialError::Duplicate { id: id(1) });
        assert!(error.to_string().contains("twice"));
    }

    #[test]
    fn two_records_a_person_cannot_tell_apart_are_refused() {
        let error = Library::new([veneer(1, "Oak"), veneer(2, " oak ")]).expect_err("one name twice");
        assert_eq!(error, MaterialError::DuplicateName { name: " oak ".to_owned() });
    }
}
