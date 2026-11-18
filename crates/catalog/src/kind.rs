//! Kinds and traits: what a record is, and the closed vocabulary the code reads.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::CatalogError;
use crate::property::PropertyCode;
use crate::slug::Slug;

/// A kind's code, allocated by the package that declares the row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "u16", into = "u16")]
pub struct KindCode(u16);

impl KindCode {
    /// The smallest code: a constant table of our own rows falls back to it.
    pub const MIN: Self = Self(1);

    /// Creates a code from a non-zero number.
    pub const fn new(value: u16) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    /// The number the package wrote.
    pub const fn get(self) -> u16 {
        self.0
    }
}

impl fmt::Display for KindCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

impl TryFrom<u16> for KindCode {
    type Error = CatalogError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        Self::new(value).ok_or(CatalogError::Empty { row: "a kind code" })
    }
}

impl From<KindCode> for u16 {
    fn from(code: KindCode) -> Self {
        code.0
    }
}

/// A closed behaviour flag: what a consumer asks about an item instead of its kind.
///
/// The list is closed on purpose (ADR 0094): a trait is behaviour we implement, so a package adds a
/// kind, never a trait. A consumer reads `Stock::Sheet` or `Banded` and never the kind's slug, which
/// is why a supplier's own vocabulary needs no code change.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Trait {
    /// Bought as a sheet: a stock block with two sides.
    Sheet,
    /// Bought by length: a stock block with one length or a profile.
    Linear,
    /// Bought by the piece or the set.
    Piece,
    /// A panel made from it can be edge-banded, and the item can be the banding.
    Banded,
    /// A part made from it is nested into a sheet.
    Nestable,
    /// Not tracked on a part: a paint, an adhesive, a screw.
    Consumable,
    /// The workshop buys the finished thing.
    Bought,
    /// The workshop makes it from bought stock.
    Shop,
    /// One price buys a sheet.
    PriceSheet,
    /// One price buys a metre.
    PriceMetre,
    /// One price buys a piece.
    PricePiece,
    /// One price buys a set.
    PriceSet,
    /// The item may carry a part document that instantiates other items.
    Expandable,
}

impl Trait {
    /// Every trait, in the order a card lists them.
    pub const ALL: [Self; 13] = [
        Self::Sheet,
        Self::Linear,
        Self::Piece,
        Self::Banded,
        Self::Nestable,
        Self::Consumable,
        Self::Bought,
        Self::Shop,
        Self::PriceSheet,
        Self::PriceMetre,
        Self::PricePiece,
        Self::PriceSet,
        Self::Expandable,
    ];

    /// Whether the item is bought as a sheet, which is what a panel's material needs.
    pub const fn is_sheet(self) -> bool {
        matches!(self, Self::Sheet)
    }
}

/// A registry row saying what an item is and which properties it may carry.
///
/// A kind declares its slug, its label, the properties it accepts, which of those it requires and
/// the traits it carries. The row arrives from a package, so a workshop's own kind of fastener is a
/// line in a file and not a variant in our source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Kind {
    code: KindCode,
    slug: Slug,
    label: String,
    accepts: BTreeSet<PropertyCode>,
    required: BTreeSet<PropertyCode>,
    traits: BTreeSet<Trait>,
}

impl Kind {
    /// Builds a kind row, refusing one that requires a property it does not accept.
    pub fn new(
        code: KindCode,
        slug: Slug,
        label: impl Into<String>,
        accepts: impl IntoIterator<Item = PropertyCode>,
        required: impl IntoIterator<Item = PropertyCode>,
        traits: impl IntoIterator<Item = Trait>,
    ) -> Result<Self, CatalogError> {
        let label = label.into();
        if label.trim().is_empty() {
            return Err(CatalogError::Empty { row: "a kind label" });
        }
        let accepts: BTreeSet<PropertyCode> = accepts.into_iter().collect();
        let required: BTreeSet<PropertyCode> = required.into_iter().collect();
        if let Some(property) = required.difference(&accepts).next().copied() {
            return Err(CatalogError::RequiredNotAccepted { kind: code, property });
        }
        Ok(Self { code, slug, label, accepts, required, traits: traits.into_iter().collect() })
    }

    /// The row's code.
    pub const fn code(&self) -> KindCode {
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

    /// Every property the kind accepts.
    pub const fn accepts(&self) -> &BTreeSet<PropertyCode> {
        &self.accepts
    }

    /// The properties the kind requires.
    pub const fn required(&self) -> &BTreeSet<PropertyCode> {
        &self.required
    }

    /// The traits the kind carries.
    pub const fn traits(&self) -> &BTreeSet<Trait> {
        &self.traits
    }

    /// Whether the kind carries `wanted`.
    pub fn has(&self, wanted: Trait) -> bool {
        self.traits.contains(&wanted)
    }
}

/// Every kind the connected packages declare.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KindRegistry {
    rows: BTreeMap<KindCode, Kind>,
    by_slug: BTreeMap<String, KindCode>,
}

impl KindRegistry {
    /// Builds the registry, refusing a code or a slug two rows claim.
    pub fn new(rows: impl IntoIterator<Item = Kind>) -> Result<Self, CatalogError> {
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
    pub fn get(&self, code: KindCode) -> Option<&Kind> {
        self.rows.get(&code)
    }

    /// The row with `slug`, when a package declares it.
    pub fn by_slug(&self, slug: &str) -> Option<&Kind> {
        self.by_slug.get(slug).and_then(|code| self.rows.get(code))
    }

    /// Every row, in code order.
    pub fn iter(&self) -> impl Iterator<Item = &Kind> {
        self.rows.values()
    }

    /// How many kinds the directory declares.
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Whether no package declares a kind.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    fn code(value: u16) -> KindCode {
        KindCode::new(value).expect("a non-zero kind code")
    }

    fn property(value: u16) -> PropertyCode {
        PropertyCode::new(value).expect("a non-zero property code")
    }

    fn sheet() -> Kind {
        Kind::new(
            code(1),
            Slug::new("sheet-material").expect("a slug"),
            "Sheet material",
            [property(1), property(2)],
            [property(1)],
            [Trait::Sheet, Trait::Nestable, Trait::PriceSheet],
        )
        .expect("a kind")
    }

    #[test]
    fn a_kind_says_which_properties_it_accepts_and_which_it_requires() {
        let row = sheet();
        assert_eq!(row.slug().as_str(), "sheet-material");
        assert!(row.accepts().contains(&property(2)), "an optional property is accepted");
        assert!(row.required().contains(&property(1)));
        assert!(row.has(Trait::Sheet) && row.has(Trait::Nestable) && !row.has(Trait::Banded));
    }

    #[test]
    fn a_kind_that_requires_a_property_it_refuses_is_an_error() {
        let error = Kind::new(code(1), Slug::new("plug").expect("a slug"), "Plug", [], [property(9)], [])
            .expect_err("a required property must be accepted");
        assert!(error.to_string().contains("requires property `9`"), "{error}");
    }

    #[test]
    fn a_registry_refuses_a_repeated_code_and_a_repeated_slug() {
        let twice = KindRegistry::new([sheet(), sheet()]).expect_err("one code cannot name two kinds");
        assert!(twice.to_string().contains("declared twice"), "{twice}");

        let other_code =
            Kind::new(code(2), Slug::new("sheet-material").expect("a slug"), "Another", [], [], []).expect("a kind");
        let twice = KindRegistry::new([sheet(), other_code]).expect_err("one slug cannot name two kinds");
        assert!(twice.to_string().contains("sheet-material"), "{twice}");

        let registry = KindRegistry::new([sheet()]).expect("one row");
        assert_eq!(registry.len(), 1);
        assert!(registry.by_slug("sheet-material").is_some());
        assert!(registry.get(code(2)).is_none());
    }

    #[test]
    fn a_trait_is_written_as_its_kebab_case_slug() {
        assert_eq!(serde_json::to_string(&Trait::PriceSheet).expect("serialize"), "\"price-sheet\"");
        assert_eq!(
            serde_json::from_str::<Trait>("\"expandable\"").expect("read back"),
            Trait::Expandable,
            "the closed vocabulary keeps the slug a package writes"
        );
        assert!(serde_json::from_str::<Trait>("\"bendable\"").is_err(), "a new trait is a code change");
        assert!(KindCode::new(0).is_none());
        assert!(serde_json::from_str::<KindCode>("0").is_err());
    }
}
