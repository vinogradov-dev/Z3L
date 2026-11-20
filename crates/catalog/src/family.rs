//! Families: the grouping a workshop reasons with, and the rank inside it.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::CatalogError;
use crate::slug::Slug;

/// A family's code, allocated by the package that declares the row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "u16", into = "u16")]
pub struct FamilyCode(u16);

impl FamilyCode {
    /// Creates a code from a non-zero number.
    pub const fn new(value: u16) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    /// The number the package wrote.
    pub const fn get(self) -> u16 {
        self.0
    }
}

impl fmt::Display for FamilyCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

impl TryFrom<u16> for FamilyCode {
    type Error = CatalogError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        Self::new(value).ok_or(CatalogError::Empty { row: "a family code" })
    }
}

impl From<FamilyCode> for u16 {
    fn from(code: FamilyCode) -> Self {
        code.0
    }
}

/// One family: a grouping of records that stand in for each other.
///
/// A family is the answer to "the plug that belongs to this corpus" (`§10`), so it is a registry row
/// of its own and one registry serves every record: a board decor, its matching edge banding and a
/// plug meet in the same family. The **member** carries the rank an item has inside its family, so a
/// member nearest the family's reference sample wins; the family row names that sample and nothing
/// else, because one row cannot order many members.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Family {
    code: FamilyCode,
    slug: Slug,
    label: String,
    sample: Option<model::ItemId>,
}

impl Family {
    /// Builds a family row.
    pub fn new(code: FamilyCode, slug: Slug, label: impl Into<String>) -> Result<Self, CatalogError> {
        let label = label.into();
        if label.trim().is_empty() {
            return Err(CatalogError::Empty { row: "a family label" });
        }
        Ok(Self { code, slug, label, sample: None })
    }

    /// The same family with the record a workshop holds up as its reference sample.
    pub fn sampled(mut self, sample: model::ItemId) -> Self {
        self.sample = Some(sample);
        self
    }

    /// The record the family is read against, when a package names one.
    pub const fn sample(&self) -> Option<model::ItemId> {
        self.sample
    }

    /// The row's code.
    pub const fn code(&self) -> FamilyCode {
        self.code
    }

    /// The row's slug.
    pub const fn slug(&self) -> &Slug {
        &self.slug
    }

    /// The name a person reads.
    pub fn label(&self) -> &str {
        &self.label
    }
}

/// Every family the connected packages declare.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FamilyRegistry {
    rows: BTreeMap<FamilyCode, Family>,
    by_slug: BTreeMap<String, FamilyCode>,
}

impl FamilyRegistry {
    /// Builds the registry, refusing a code or a slug two rows claim.
    pub fn new(rows: impl IntoIterator<Item = Family>) -> Result<Self, CatalogError> {
        let mut registry = Self::default();
        for row in rows {
            if registry.rows.contains_key(&row.code) {
                return Err(CatalogError::DuplicateCode { code: row.code.to_string() });
            }
            if registry.by_slug.contains_key(row.slug.as_str()) {
                return Err(CatalogError::DuplicateSlug { slug: row.slug.to_string() });
            }
            registry.by_slug.insert(row.slug.to_string(), row.code);
            registry.rows.insert(row.code, row);
        }
        Ok(registry)
    }

    /// The row with `code`, when a package declares it.
    pub fn get(&self, code: FamilyCode) -> Option<&Family> {
        self.rows.get(&code)
    }

    /// The row with `slug`, when a package declares it.
    pub fn by_slug(&self, slug: &str) -> Option<&Family> {
        self.by_slug.get(slug).and_then(|code| self.rows.get(code))
    }

    /// Every row, in code order.
    pub fn iter(&self) -> impl Iterator<Item = &Family> {
        self.rows.values()
    }

    /// How many families the directory declares.
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Whether no package declares a family.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    fn family(slug: &str) -> Family {
        Family::new(FamilyCode::new(1).expect("a family code"), Slug::new(slug).expect("a slug"), "Oak")
            .expect("a family")
    }

    #[test]
    fn a_family_is_found_by_its_slug() {
        let registry = FamilyRegistry::new([family("oak")]).expect("one row");
        assert_eq!(registry.by_slug("oak").map(Family::label), Some("Oak"));
        assert!(registry.by_slug("white").is_none());
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn a_family_may_name_the_record_it_is_read_against() {
        let sample = model::ItemId::new(712, 34).expect("an item id");
        let row = family("oak").sampled(sample);
        assert_eq!(row.sample(), Some(sample));
        assert_eq!(family("white").sample(), None, "a family without a sample is read by rank alone");
    }

    #[test]
    fn two_rows_cannot_claim_one_slug() {
        let other = Family::new(FamilyCode::new(2).expect("a code"), Slug::new("oak").expect("a slug"), "Oak veneer")
            .expect("a family");
        let refusal = FamilyRegistry::new([family("oak"), other]).expect_err("one slug, one family");
        assert!(refusal.to_string().contains("oak"), "{refusal}");
    }
}
