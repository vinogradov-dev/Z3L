//! The library's stable name for one material.

use std::fmt;

use serde::{Deserialize, Serialize};

/// The library's stable name for one material, written as `id` and never reused.
///
/// The number belongs to the library's file and not to a catalogue record: a nomenclature item names
/// this id (`docs/domain/nomenclature.md` §7), so two records of a shop that are both oak name one
/// material and a library edit reaches both. Zero is not a material, which is what a missing
/// reference reads as.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "u32", into = "u32")]
pub struct MaterialId(u32);

impl MaterialId {
    /// The material `value`, or `None` for zero, which no material is.
    pub const fn new(value: u32) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    /// The number itself.
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl fmt::Display for MaterialId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "material {}", self.0)
    }
}

impl TryFrom<u32> for MaterialId {
    type Error = String;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Self::new(value).ok_or_else(|| "material id 0 names no material: ids start at 1".to_owned())
    }
}

impl From<MaterialId> for u32 {
    fn from(id: MaterialId) -> Self {
        id.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_names_no_material() {
        assert_eq!(MaterialId::new(0), None);
        assert_eq!(MaterialId::new(7).map(MaterialId::get), Some(7));
        assert!(serde_json::from_str::<MaterialId>("0").is_err(), "a file cannot name zero");
        assert_eq!(serde_json::from_str::<MaterialId>("7").ok(), MaterialId::new(7));
    }
}
