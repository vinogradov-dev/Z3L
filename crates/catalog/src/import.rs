//! A price list, read as one batch: a supplier's CSV, matched by foreign key.
//!
//! A workshop receives prices as a file — a supplier's export, a spreadsheet saved as CSV — and the
//! directory application writes them all at once (`docs/domain/nomenclature.md` §11, ADR 0148). Three
//! rules make that safe:
//!
//! - **nothing is matched by name.** A row names the foreign `system` and the `key` that system uses,
//!   and the item is resolved through `Directory::by_external`; a row whose key no record claims is
//!   refused with the pair in the sentence, so a mistyped export cannot overwrite the wrong price.
//! - **a row is applied or refused, and the reason is a sentence.** A price that is not a number of
//!   minor units, a currency that is not a usable ISO 4217 code, a date the calendar does not have and
//!   a currency the directory does not use are each refused on their own line, and the good rows still
//!   land.
//! - **the batch is one write and one revision.** The records go through the writer once, so a reader
//!   polling the folder sees one change rather than one per row (§13).
//!
//! The batch plans in memory and writes nothing: a caller reads a file, plans, and hands the records
//! to `FolderWriter::save_items`, which validates the whole directory before it touches the disk.

use std::collections::BTreeMap;

use model::ItemId;

use crate::directory::Directory;
use crate::item::{ExternalKey, NomenclatureItem};
use crate::money::{Currency, Money};
use crate::price::Price;
use crate::property::{Date, PropertyValue};
use crate::view::PRICE_SLUG;

/// The header a price list must carry, in this order.
pub const PRICE_LIST_HEADER: [&str; 5] = ["system", "key", "price_minor", "currency", "date"];

/// What the batch did with one row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Applied {
    /// The price the row writes, and the record it lands on.
    Priced {
        /// The record the row's key resolved to.
        item: ItemId,
        /// The price the record will carry.
        price: Price,
    },
    /// Why the row was refused, with its line number.
    Refused {
        /// The line the file wrote the row on, counting the header as one.
        line: usize,
        /// What is wrong with it, in the words the status line shows.
        reason: String,
    },
}

/// A price list read against one directory: the rows it applies and the rows it refuses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PriceBatch {
    rows: Vec<Applied>,
}

impl PriceBatch {
    /// Reads a price list against `directory`, applying the rows it can and refusing the rest.
    ///
    /// The header is required and fixed, because a file whose column order changed is a different
    /// file: reading it by position would write a currency where a price belongs.
    pub fn read(text: &str, directory: &Directory) -> Self {
        let mut rows = Vec::new();
        let mut lines =
            text.lines().enumerate().map(|(index, line)| (index + 1, line.trim())).filter(|(_, line)| !line.is_empty());
        let Some((_, header)) = lines.next() else {
            return Self { rows: vec![Applied::Refused { line: 1, reason: "the file is empty".to_owned() }] };
        };
        let written: Vec<&str> = header.split(',').map(str::trim).collect();
        if written != PRICE_LIST_HEADER {
            return Self {
                rows: vec![Applied::Refused {
                    line: 1,
                    reason: format!("the header must be `{}`", PRICE_LIST_HEADER.join(",")),
                }],
            };
        }
        // One file is one money: the directory's currency when it has one, and else the first row's,
        // so a file that mixes two currencies is refused row by row instead of failing the write.
        let mut money = directory.currency().cloned();
        for (line, text) in lines {
            rows.push(Self::read_row(line, text, directory, &mut money));
        }
        if rows.is_empty() {
            rows.push(Applied::Refused { line: 2, reason: "the file holds no prices".to_owned() });
        }
        Self { rows }
    }

    /// One row, applied when it resolves and refused with its reason when it does not.
    fn read_row(line: usize, text: &str, directory: &Directory, money: &mut Option<Currency>) -> Applied {
        let fields: Vec<&str> = text.split(',').map(str::trim).collect();
        let [system, key, minor, currency, date] = fields.as_slice() else {
            return Applied::Refused {
                line,
                reason: format!(
                    "a row carries {} fields, and the header names {}",
                    fields.len(),
                    PRICE_LIST_HEADER.len()
                ),
            };
        };
        let minor = match minor.parse::<u64>() {
            Ok(minor) => minor,
            Err(_) => {
                return Applied::Refused { line, reason: format!("`{minor}` is not a price in whole minor units") };
            }
        };
        // The currency's own refusal is the sentence a person can act on: the code and the scale.
        let currency = match Currency::new(*currency, currency_scale(currency)) {
            Ok(currency) => currency,
            Err(error) => return Applied::Refused { line, reason: error.to_string() },
        };
        let date = match parse_date(date) {
            Ok(date) => date,
            Err(reason) => return Applied::Refused { line, reason },
        };
        let anchor = match ExternalKey::new(
            match crate::item::ExternalSystemId::new(*system) {
                Ok(id) => id,
                Err(error) => return Applied::Refused { line, reason: error.to_string() },
            },
            *key,
        ) {
            Ok(anchor) => anchor,
            Err(error) => return Applied::Refused { line, reason: error.to_string() },
        };
        let Some(item) = directory.by_external(anchor.system.as_str(), &anchor.key) else {
            return Applied::Refused {
                line,
                reason: format!("no record claims `{}` key `{}`", anchor.system.as_str(), anchor.key),
            };
        };
        match money {
            Some(held) if held != &currency => {
                return Applied::Refused {
                    line,
                    reason: format!("the file is written in {}, and the row in {}", held.code(), currency.code()),
                };
            }
            Some(_) => {}
            None => *money = Some(currency.clone()),
        }
        Applied::Priced {
            item: item.id(),
            price: Price::imported(Money::from_minor(minor), currency, anchor.system, date),
        }
    }

    /// The rows the batch applies, in file order.
    pub fn applied(&self) -> impl Iterator<Item = (ItemId, &Price)> {
        self.rows.iter().filter_map(|row| match row {
            Applied::Priced { item, price } => Some((*item, price)),
            Applied::Refused { .. } => None,
        })
    }

    /// The rows the batch refuses, in file order, with their line and reason.
    pub fn refused(&self) -> impl Iterator<Item = (usize, &str)> {
        self.rows.iter().filter_map(|row| match row {
            Applied::Refused { line, reason } => Some((*line, reason.as_str())),
            Applied::Priced { .. } => None,
        })
    }

    /// How many prices the batch writes.
    pub fn applied_count(&self) -> usize {
        self.applied().count()
    }

    /// How many rows it refuses.
    pub fn refused_count(&self) -> usize {
        self.refused().count()
    }

    /// The records the batch writes: every priced record with its price replaced.
    ///
    /// A record the batch does not name is not returned, so a caller writes only what changed. The
    /// price lands in the property row the directory's `price` slug names, because a card and an
    /// export read it by that slug and never by a code.
    pub fn records(&self, directory: &Directory) -> Vec<NomenclatureItem> {
        let mut written: BTreeMap<ItemId, NomenclatureItem> = BTreeMap::new();
        for (id, price) in self.applied() {
            let Some(item) = directory.item(id) else { continue };
            let Some(code) = directory.price_code(item) else { continue };
            let record = item.clone().with_prop(code, PropertyValue::Money(price.clone()));
            written.insert(id, record);
        }
        written.into_values().collect()
    }

    /// The one sentence the status line shows after a batch.
    pub fn sentence(&self) -> String {
        let applied = self.applied_count();
        let refused = self.refused_count();
        match (applied, refused) {
            (0, 0) => format!("no prices to read; the file names {PRICE_SLUG} by `{}`", PRICE_LIST_HEADER.join(",")),
            (applied, 0) => format!("{applied} prices written as one revision"),
            (0, refused) => format!("no price was written; {refused} rows refused"),
            (applied, refused) => format!("{applied} prices written, {refused} rows refused"),
        }
    }
}

/// How many minor units make one unit of `code`, in the shape of `Currency::new`.
///
/// The list holds the currencies a workshop in our market writes prices in, with the scale ISO 4217
/// gives them; anything else is refused by `Currency` as a code we cannot print. A price list that
/// names a currency outside this list is a decision, not a default: add it here with its scale.
fn currency_scale(code: &str) -> u32 {
    match code {
        // Two minor units in one unit.
        "EUR" | "USD" | "RUB" | "BYN" | "KZT" | "UAH" | "CHF" | "GBP" | "PLN" | "TRY" | "CNY" => 100,
        // A currency whose minor unit is the unit itself.
        "JPY" | "KRW" => 1,
        // Anything else: one, so `Currency::new` refuses the code only when it is not ISO-shaped, and
        // a card prints the amount as the file wrote it.
        _ => 1,
    }
}

/// Reads the `YYYY-MM-DD` a price list writes.
fn parse_date(text: &str) -> Result<Date, String> {
    let readable = |reason: &str| format!("`{text}` is not a date this calendar has {reason}");
    let fields: Vec<&str> = text.split('-').collect();
    let [year, month, day] = fields.as_slice() else {
        return Err(readable("— write a year, a month and a day"));
    };
    let year = year.parse::<i32>().map_err(|_| readable("— the year is not a number"))?;
    let month = month.parse::<u8>().map_err(|_| readable("— the month is not a number"))?;
    let day = day.parse::<u8>().map_err(|_| readable("— the day is not a number"))?;
    Date::new(year, month, day).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use std::collections::BTreeMap;

    use units::Length;

    use super::*;
    use crate::external::{ExternalSystem, ExternalSystemRegistry};
    use crate::family::FamilyRegistry;
    use crate::kind::{Kind, KindCode, KindRegistry, Trait};
    use crate::property::{Property, PropertyCode, PropertyRegistry, PropertyType};
    use crate::slug::Slug;

    fn code(value: u16) -> PropertyCode {
        PropertyCode::new(value).expect("a property code")
    }

    fn slug(text: &str) -> Slug {
        Slug::new(text).expect("a slug")
    }

    fn system(text: &str) -> crate::item::ExternalSystemId {
        crate::item::ExternalSystemId::new(text).expect("a system")
    }

    /// Two priced boards, known to `erp` as `A-1` and `A-2`, in red and in white.
    fn directory(currency: Option<&str>) -> Directory {
        let properties = PropertyRegistry::new([
            Property::new(
                code(1),
                slug("thickness"),
                "Thickness",
                PropertyType::Length { min: Length::from_micrometres(1_000), max: Length::from_micrometres(60_000) },
            )
            .expect("a row"),
            Property::new(code(2), slug(PRICE_SLUG), "Price", PropertyType::Money).expect("a row"),
        ])
        .expect("two rows");
        let kind = Kind::new(
            KindCode::new(1).expect("a kind code"),
            slug("sheet-material"),
            "Sheet material",
            [code(1), code(2)],
            [code(1)],
            [Trait::Sheet, Trait::PriceSheet],
        )
        .expect("a kind");
        let board = |serial: u32, minor: u64| {
            let mut props = BTreeMap::from([(code(1), PropertyValue::Length(Length::from_micrometres(18_000)))]);
            if let Some(currency) = currency {
                let currency = Currency::new(currency, 100).expect("a currency");
                props.insert(code(2), PropertyValue::Money(Price::typed(Money::from_minor(minor), currency)));
            }
            NomenclatureItem::new(
                ItemId::new(712, serial).expect("an item id"),
                KindCode::new(1).expect("a kind code"),
                format!("Board {serial}"),
                slug(&format!("board-{serial}")),
                props,
            )
            .expect("an item")
            .with_external(ExternalKey::new(system("erp"), format!("A-{serial}")).expect("a key"))
            .expect("a key")
        };
        let systems = ExternalSystemRegistry::new([ExternalSystem::new(system("erp"), "ERP").expect("a row")])
            .expect("one system");
        Directory::new(
            [board(1, 100), board(2, 200)],
            KindRegistry::new([kind]).expect("one kind"),
            properties,
            FamilyRegistry::default(),
            systems,
        )
        .expect("a directory")
    }

    const LIST: &str = "system,key,price_minor,currency,date\n\
                        erp,A-1,320000,RUB,2026-11-20\n\
                        erp,A-2,240000,RUB,2026-11-20\n";

    #[test]
    fn a_list_prices_the_records_its_keys_name() {
        let directory = directory(Some("RUB"));
        let batch = PriceBatch::read(LIST, &directory);
        assert_eq!(batch.applied_count(), 2);
        assert_eq!(batch.refused_count(), 0);
        let records = batch.records(&directory);
        assert_eq!(records.len(), 2);
        let price = |serial: u32| {
            let item =
                records.iter().find(|item| item.id() == ItemId::new(712, serial).expect("an id")).expect("a record");
            match item.prop(code(2)).expect("a price") {
                PropertyValue::Money(price) => price.clone(),
                other => panic!("a price, not {other:?}"),
            }
        };
        let first = price(1);
        assert_eq!(first.value().minor(), 320_000);
        assert_eq!(first.currency().code(), "RUB");
        assert_eq!(first.source().system(), Some(&system("erp")), "the row says where it came from");
        assert_eq!(first.date(), Some(Date::new(2026, 11, 20).expect("a date")));
        assert_eq!(price(2).value().minor(), 240_000);
    }

    #[test]
    fn a_price_without_a_usable_currency_is_refused_with_its_line_and_its_iso_sentence() {
        let directory = directory(Some("RUB"));
        let list = "system,key,price_minor,currency,date\n\
                    erp,A-1,320000,eur,2026-11-20\n\
                    erp,A-2,240000,RUB,2026-11-20\n";
        let batch = PriceBatch::read(list, &directory);
        assert_eq!(batch.applied_count(), 1, "the good row still lands");
        let refused: Vec<(usize, &str)> = batch.refused().collect();
        assert_eq!(refused.len(), 1);
        assert_eq!(refused[0].0, 2, "the line the file wrote it on");
        assert!(refused[0].1.contains("ISO 4217"), "{}", refused[0].1);
        let refused = batch.sentence();
        assert!(refused.contains("1 prices written, 1 rows refused"), "{refused}");
    }

    #[test]
    fn a_row_is_refused_when_its_key_names_no_record_or_its_amount_is_not_a_price() {
        let directory = directory(Some("RUB"));
        let list = "system,key,price_minor,currency,date\n\
                    erp,A-9,320000,RUB,2026-11-20\n\
                    erp,A-1,three,RUB,2026-11-20\n\
                    erp,A-1,320000,RUB,2026-02-30\n\
                    erp\n";
        let batch = PriceBatch::read(list, &directory);
        assert_eq!(batch.applied_count(), 0);
        let reasons: Vec<String> = batch.refused().map(|(_, reason)| reason.to_owned()).collect();
        assert_eq!(reasons.len(), 4);
        assert!(reasons[0].contains("no record claims `erp` key `A-9`"), "{}", reasons[0]);
        assert!(reasons[1].contains("not a price in whole minor units"), "{}", reasons[1]);
        assert!(reasons[2].contains("2026-02-30"), "the calendar names the day: {}", reasons[2]);
        assert!(reasons[3].contains("carries 1 fields"), "{}", reasons[3]);
    }

    #[test]
    fn a_file_whose_header_changed_or_that_holds_no_prices_is_refused_whole() {
        let directory = directory(Some("RUB"));
        let wrong = PriceBatch::read("key,system,price_minor,currency,date\nerp,A-1,1,RUB,2026-11-20\n", &directory);
        assert_eq!(wrong.applied_count(), 0, "a moved column would write a currency where a price belongs");
        assert!(wrong.refused().next().expect("a refusal").1.contains("the header must be"));

        let empty = PriceBatch::read("", &directory);
        assert!(empty.refused().next().expect("a refusal").1.contains("the file is empty"));

        let header_only = PriceBatch::read("system,key,price_minor,currency,date\n", &directory);
        assert!(header_only.refused().next().expect("a refusal").1.contains("no prices"));
    }

    #[test]
    fn a_row_in_another_currency_than_the_directory_is_refused_because_one_directory_is_one_money() {
        let directory = directory(Some("RUB"));
        let list = "system,key,price_minor,currency,date\nerp,A-1,3200,EUR,2026-11-20\n";
        let batch = PriceBatch::read(list, &directory);
        let reason = batch.refused().next().expect("a refusal").1;
        assert!(reason.contains("written in RUB") && reason.contains("EUR"), "{reason}");
    }

    #[test]
    fn a_file_that_mixes_two_currencies_is_one_money_and_the_second_row_is_refused() {
        // A directory with no price yet takes its currency from the file; a second currency in the
        // same file is refused on its own row rather than failing the whole write on a snapshot.
        let directory = directory(None);
        let list = "system,key,price_minor,currency,date\n\
                    erp,A-1,320000,RUB,2026-11-20\n\
                    erp,A-2,2400,EUR,2026-11-20\n";
        let batch = PriceBatch::read(list, &directory);
        assert_eq!(batch.applied_count(), 1);
        let reason: Vec<&str> = batch.refused().map(|(_, reason)| reason).collect();
        assert_eq!(reason.len(), 1);
        assert!(reason[0].contains("written in RUB") && reason[0].contains("EUR"), "{}", reason[0]);
        // And the record's kind buys by the sheet, so the price the row writes carries that unit.
        let records = batch.records(&directory);
        let priced = directory.price(&records[0]).expect("a price with its unit");
        assert_eq!(priced.unit, crate::price::PriceUnit::Sheet);
    }
}
