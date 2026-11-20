//! A price, what one price buys, and where it came from.
//!
//! A price is not a bare amount (`docs/domain/nomenclature.md` §11, ADR 0148): it carries the
//! currency it is written in, the source that wrote it and the date that source measured it, because
//! a price arrives from a supplier's list and goes stale. The unit — one price buys a sheet, a metre,
//! a piece or a set — is **not** stored: it is a trait of the kind, so a record cannot disagree with
//! its own kind about what a price is for.
//!
//! An importer fills many prices at once; the batch is one write and one revision of the directory
//! (`FolderWriter::save_items`), and the rows it carries are [`Price`] values, so a currency that is
//! not a usable ISO 4217 code never reaches a record.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::item::ExternalSystemId;
use crate::kind::{Kind, Trait};
use crate::money::{Currency, Money};
use crate::property::Date;

/// What one price buys, read from the kind's traits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PriceUnit {
    /// One price buys a sheet.
    Sheet,
    /// One price buys a metre.
    Metre,
    /// One price buys a piece.
    Piece,
    /// One price buys a set.
    Set,
}

impl PriceUnit {
    /// Every unit, in the order a card lists them.
    pub const ALL: [Self; 4] = [Self::Sheet, Self::Metre, Self::Piece, Self::Set];

    /// The unit a kind buys in, when its traits name exactly one.
    ///
    /// A kind a package forgot to give a unit answers `None`: a record priced in it is refused when
    /// the directory loads, because a price nobody can read a unit from is not a price.
    pub fn of(kind: &Kind) -> Option<Self> {
        Self::ALL.into_iter().find(|unit| kind.has(unit.trait_of()))
    }

    /// How many of [`PriceUnit::ALL`] a kind names; more than one is a package mistake.
    pub fn count_of(kind: &Kind) -> usize {
        Self::ALL.iter().filter(|unit| kind.has(unit.trait_of())).count()
    }

    /// The trait that names this unit on a kind row.
    pub const fn trait_of(self) -> Trait {
        match self {
            Self::Sheet => Trait::PriceSheet,
            Self::Metre => Trait::PriceMetre,
            Self::Piece => Trait::PricePiece,
            Self::Set => Trait::PriceSet,
        }
    }

    /// The word a sentence or a card uses for one unit.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Sheet => "sheet",
            Self::Metre => "metre",
            Self::Piece => "piece",
            Self::Set => "set",
        }
    }
}

impl fmt::Display for PriceUnit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

/// Where a price came from: a person at the card, or an import that names its system.
///
/// Provenance is part of the value because it is what a person asks when a price surprises them: who
/// wrote it, and out of which system. An imported price names an `ExternalSystemId` from the
/// registry, so `1C` and `1с` cannot become two answers to the same question.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PriceSource {
    /// A person typed it into the record.
    Typed,
    /// An import wrote it, out of `system`.
    Imported {
        /// The foreign system the batch arrived from.
        system: ExternalSystemId,
    },
}

impl PriceSource {
    /// The source of a price an import wrote.
    pub fn imported(system: ExternalSystemId) -> Self {
        Self::Imported { system }
    }

    /// The system the price came out of, when an import wrote it.
    pub fn system(&self) -> Option<&ExternalSystemId> {
        match self {
            Self::Typed => None,
            Self::Imported { system } => Some(system),
        }
    }
}

impl fmt::Display for PriceSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Typed => formatter.write_str("typed"),
            Self::Imported { system } => write!(formatter, "imported from {}", system.as_str()),
        }
    }
}

/// One price as a record carries it: the amount in whole minor units, its currency and its provenance.
///
/// The amount [`Money`] is unsigned and unitless; this value adds the two facts that make it a price
/// a workshop can act on and no reader has to guess: which currency the minor units belong to, and
/// where and when the number came from. A currency is a validated [`Currency`], so a price without a
/// usable one cannot be built at all — the only way a package can offer one is a file whose currency
/// the reader refuses.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Price {
    /// Whole minor units of `currency`.
    #[serde(rename = "minor")]
    value: Money,
    /// The currency the minor units are written in.
    currency: Currency,
    /// Who wrote the price.
    source: PriceSource,
    /// When the source measured it; a price a person typed has no date until an import dates it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    date: Option<Date>,
}

impl Price {
    /// A price a person typed at the card: the amount, and the currency the package writes in.
    pub fn typed(value: Money, currency: Currency) -> Self {
        Self { value, currency, source: PriceSource::Typed, date: None }
    }

    /// A price an import wrote, out of `system`, measured on `date`.
    pub fn imported(value: Money, currency: Currency, system: ExternalSystemId, date: Date) -> Self {
        Self { value, currency, source: PriceSource::imported(system), date: Some(date) }
    }

    /// The amount in whole minor units.
    pub const fn value(&self) -> Money {
        self.value
    }

    /// The currency the amount is written in.
    pub const fn currency(&self) -> &Currency {
        &self.currency
    }

    /// Who wrote the price.
    pub const fn source(&self) -> &PriceSource {
        &self.source
    }

    /// When the source measured it, when it did.
    pub const fn date(&self) -> Option<Date> {
        self.date
    }
}

/// How old a price is, as a card or a report says it.
///
/// The directory has no clock: a caller supplies today's date and the workshop's limit, and the rule
/// is arithmetic between two dates. A price a person typed carries no date, so it can be neither
/// fresh nor stale and says exactly that — the alternative, treating an undated price as old, would
/// warn about a number nobody has measured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Freshness {
    /// Dated inside the limit.
    Fresh {
        /// How many days old the price is.
        days: u64,
    },
    /// Dated further back than the limit.
    Stale {
        /// How many days old the price is.
        days: u64,
        /// The limit it passed.
        limit_days: u64,
    },
    /// The price carries no date, because a person typed it.
    Undated,
}

impl Freshness {
    /// Whether the freshness is worth a word in a card.
    pub const fn is_stale(self) -> bool {
        matches!(self, Self::Stale { .. })
    }
}

impl fmt::Display for Freshness {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Fresh { days } => {
                write!(formatter, "{days} days old")
            }
            Self::Stale { days, limit_days } => {
                write!(formatter, "{days} days old, past the {limit_days}-day limit")
            }
            Self::Undated => formatter.write_str("no date"),
        }
    }
}

impl Price {
    /// How old the price is on `today`, against a workshop's `limit_days`.
    ///
    /// A date later than today answers `Fresh` with zero days: a clock that disagrees with an import
    /// by a few hours is not a reason to call a price stale.
    pub fn freshness(&self, today: Date, limit_days: u32) -> Freshness {
        let Some(date) = self.date else {
            return Freshness::Undated;
        };
        let elapsed = today.days_since_epoch().saturating_sub(date.days_since_epoch()).max(0);
        let days = u64::try_from(elapsed).unwrap_or(u64::MAX);
        let limit_days = u64::from(limit_days);
        if days > limit_days { Freshness::Stale { days, limit_days } } else { Freshness::Fresh { days } }
    }
}

impl fmt::Display for Price {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} {}", self.value.minor(), self.currency.code())
    }
}

/// A price with the unit its kind buys, as a consumer asks for it (`§15`).
///
/// The unit is answered beside the stored price rather than kept in it: a record's kind decides what
/// a price is for, and a copy inside the value could disagree with the kind after a row changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PriceView {
    /// The stored price and its provenance.
    pub price: Price,
    /// What one price buys, from the kind's traits.
    pub unit: PriceUnit,
}

impl PriceView {
    /// The amount in whole minor units.
    pub const fn value(&self) -> Money {
        self.price.value()
    }

    /// The currency the amount is written in.
    pub const fn currency(&self) -> &Currency {
        self.price.currency()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;
    use crate::kind::KindCode;
    use crate::slug::Slug;

    fn kind(traits: impl IntoIterator<Item = Trait>) -> Kind {
        Kind::new(
            KindCode::new(1).expect("a kind code"),
            Slug::new("sheet-material").expect("a slug"),
            "Sheet material",
            [],
            [],
            traits,
        )
        .expect("a kind")
    }

    fn euro() -> Currency {
        Currency::new("EUR", 100).expect("a currency")
    }

    #[test]
    fn a_price_keeps_its_currency_source_and_date() {
        let typed = Price::typed(Money::from_minor(12_345), euro());
        assert_eq!(typed.value().minor(), 12_345);
        assert_eq!(typed.currency().code(), "EUR");
        assert_eq!(typed.source(), &PriceSource::Typed);
        assert_eq!(typed.date(), None, "a typed price is not dated by a clock the directory does not have");

        let system = ExternalSystemId::new("supplier-egger").expect("a system");
        let date = Date::new(2026, 11, 20).expect("a date");
        let imported = Price::imported(Money::from_minor(9_990), euro(), system.clone(), date);
        assert_eq!(imported.source().system(), Some(&system));
        assert_eq!(imported.date(), Some(date));
        assert_eq!(imported.to_string(), "9990 EUR");
    }

    #[test]
    fn a_price_is_as_old_as_the_dates_say() {
        let date = Date::new(2026, 1, 1).expect("a date");
        let today = Date::new(2026, 11, 20).expect("a date");
        let euro = Currency::new("EUR", 100).expect("a currency");
        let system = ExternalSystemId::new("erp").expect("a system");
        let price = Price::imported(Money::from_minor(100), euro, system, date);
        let freshness = price.freshness(today, 180);
        assert!(freshness.is_stale(), "323 days is past a 180-day limit");
        assert_eq!(freshness, Freshness::Stale { days: 323, limit_days: 180 });
        assert_eq!(price.freshness(today, 365), Freshness::Fresh { days: 323 });
        assert_eq!(price.freshness(today, 365).to_string(), "323 days old", "a fresh price still says how old it is");
        // A clock behind the import is not a stale price.
        assert_eq!(price.freshness(date, 0), Freshness::Fresh { days: 0 });
        assert_eq!(
            Price::typed(Money::ZERO, Currency::new("EUR", 100).expect("a currency")).freshness(today, 1),
            Freshness::Undated
        );
    }

    #[test]
    fn a_price_written_and_read_back_keeps_its_provenance() {
        let system = ExternalSystemId::new("erp").expect("a system");
        let price =
            Price::imported(Money::from_minor(240_000), euro(), system, Date::new(2026, 11, 20).expect("a date"));
        let json = serde_json::to_string(&price).expect("serialize");
        assert!(json.contains("\"minor\":240000"), "{json}");
        assert!(json.contains("\"imported\""), "{json}");
        assert_eq!(serde_json::from_str::<Price>(&json).expect("read back"), price);
    }

    #[test]
    fn a_price_without_a_usable_currency_is_refused_where_it_is_written() {
        // The value only exists with a validated `Currency`, so the one way a package can offer a
        // price we cannot read is a file, and the reader is what refuses it — with the sentence the
        // currency type itself speaks.
        let json = r#"{"minor":100,"currency":{"code":"eur","minor_scale":100},"source":"typed"}"#;
        let refusal = serde_json::from_str::<Price>(json).expect_err("a lower-case code is not a currency");
        assert!(refusal.to_string().contains("ISO 4217"), "{refusal}");

        let json = r#"{"minor":100,"currency":{"code":"EUR","minor_scale":0},"source":"typed"}"#;
        assert!(serde_json::from_str::<Price>(json).is_err(), "a currency with no minor units is not usable");
        assert!(
            serde_json::from_str::<Price>(r#"{"minor":100,"currency":{"code":"EUR","minor_scale":100}}"#).is_err(),
            "a price always names its source"
        );
    }

    #[test]
    fn the_unit_a_price_buys_is_the_kind_trait() {
        assert_eq!(PriceUnit::of(&kind([Trait::PriceSheet])), Some(PriceUnit::Sheet));
        assert_eq!(PriceUnit::of(&kind([Trait::PriceMetre, Trait::Banded])), Some(PriceUnit::Metre));
        assert_eq!(PriceUnit::of(&kind([Trait::Banded])), None, "a kind with no unit buys in nothing");
        assert_eq!(PriceUnit::Sheet.trait_of(), Trait::PriceSheet);
        assert_eq!(PriceUnit::Set.name(), "set");
    }

    #[test]
    fn a_kind_that_names_two_price_units_is_refused() {
        // The unit is read from the traits, so two of them would leave the reading to the order the
        // file lists them in: the row itself refuses.
        let refusal = Kind::new(
            KindCode::new(1).expect("a kind code"),
            Slug::new("sheet-material").expect("a slug"),
            "Sheet material",
            [],
            [],
            [Trait::PricePiece, Trait::PriceSet],
        )
        .expect_err("one kind buys in one unit");
        assert!(refusal.to_string().contains("more than one price unit"), "{refusal}");
    }
}
