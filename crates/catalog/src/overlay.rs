//! Stock on hand, which is not a property of a record.
//!
//! A price is slowly moving data a workshop owns, so it lives on the record (`crate::price`); stock on
//! hand is volatile, it arrives from another system, and it must not make the directory invalid when a
//! supplier's count is a day old. So it lives beside the directory in an **overlay**: a file a package
//! carries, naming items by `ItemId` only, with its own source and date
//! (`docs/domain/nomenclature.md` §2, §11).
//!
//! Removing an overlay leaves the directory complete: every query that does not ask for stock answers
//! exactly as before, and a record with no overlay answers `None` rather than a zero — a workshop may
//! know a material it does not stock.

use std::collections::BTreeMap;

use model::ItemId;

use crate::error::CatalogError;
use crate::item::ExternalSystemId;
use crate::property::Date;

/// One overlay's rows: stock on hand, measured by `source` on `date`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StockOverlay {
    name: String,
    source: ExternalSystemId,
    date: Date,
    rows: BTreeMap<ItemId, u64>,
}

impl StockOverlay {
    /// Builds an overlay, refusing one file that names an item twice.
    pub fn new(
        name: impl Into<String>,
        source: ExternalSystemId,
        date: Date,
        rows: impl IntoIterator<Item = (ItemId, u64)>,
    ) -> Result<Self, CatalogError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(CatalogError::Empty { row: "an overlay name" });
        }
        let mut counted: BTreeMap<ItemId, u64> = BTreeMap::new();
        for (id, on_hand) in rows {
            if counted.insert(id, on_hand).is_some() {
                return Err(CatalogError::DuplicateStock { overlay: name, item: id });
            }
        }
        Ok(Self { name, source, date, rows: counted })
    }

    /// The overlay's name, which is the file it came from without its extension.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The system that measured the counts.
    pub const fn source(&self) -> &ExternalSystemId {
        &self.source
    }

    /// The day the counts were measured.
    pub const fn date(&self) -> Date {
        self.date
    }

    /// The counts, in item id order.
    pub fn rows(&self) -> impl Iterator<Item = (ItemId, u64)> + '_ {
        self.rows.iter().map(|(id, on_hand)| (*id, *on_hand))
    }

    /// The count one item has on hand, when this overlay names it.
    pub fn on_hand(&self, id: ItemId) -> Option<u64> {
        self.rows.get(&id).copied()
    }

    /// How many records the overlay names.
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Whether the overlay names no record.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

/// Stock on hand as a consumer reads it: the count, and the overlay that measured it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OnHand {
    count: u64,
    overlay: String,
    source: ExternalSystemId,
    date: Date,
}

impl OnHand {
    /// The count the source measured.
    pub const fn count(&self) -> u64 {
        self.count
    }

    /// The overlay the count came from.
    pub fn overlay(&self) -> &str {
        &self.overlay
    }

    /// The system that measured it.
    pub const fn source(&self) -> &ExternalSystemId {
        &self.source
    }

    /// The day it was measured.
    pub const fn date(&self) -> Date {
        self.date
    }

    /// The overlay row, as a consumer asks for it.
    pub(crate) fn of(overlay: &StockOverlay, count: u64) -> Self {
        Self { count, overlay: overlay.name.clone(), source: overlay.source.clone(), date: overlay.date }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    fn id(serial: u32) -> ItemId {
        ItemId::new(712, serial).expect("an item id")
    }

    fn system() -> ExternalSystemId {
        ExternalSystemId::new("erp").expect("a system")
    }

    #[test]
    fn an_overlay_names_a_count_per_item_with_its_source_and_date() {
        let date = Date::new(2026, 11, 20).expect("a date");
        let overlay = StockOverlay::new("erp-stock", system(), date, [(id(34), 12), (id(35), 0)]).expect("an overlay");
        assert_eq!(overlay.name(), "erp-stock");
        assert_eq!(overlay.date(), date);
        assert_eq!(overlay.on_hand(id(34)), Some(12));
        assert_eq!(overlay.on_hand(id(36)), None, "a record the overlay does not name has no count");
        assert_eq!(overlay.rows().count(), 2);
    }

    #[test]
    fn an_overlay_that_names_one_item_twice_is_refused() {
        let date = Date::new(2026, 11, 20).expect("a date");
        let refusal = StockOverlay::new("erp-stock", system(), date, [(id(34), 12), (id(34), 8)])
            .expect_err("one file, one count per item");
        assert!(refusal.to_string().contains("712-34") && refusal.to_string().contains("erp-stock"), "{refusal}");
        assert!(
            StockOverlay::new(" ", system(), date, []).is_err(),
            "an overlay a person cannot name in a refusal is not a row"
        );
    }
}
