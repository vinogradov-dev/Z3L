//! Typed properties: what an item may carry, and the value it carries.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use model::ItemId;
use serde::{Deserialize, Serialize};
use units::Length;

use crate::error::CatalogError;
use crate::money::Money;
use crate::slug::Slug;

/// A property's code, allocated by the package that declares the row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "u16", into = "u16")]
pub struct PropertyCode(u16);

impl PropertyCode {
    /// The smallest code: a constant table of our own rows falls back to it, because zero is not a
    /// code and a table of constants cannot fail.
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

impl fmt::Display for PropertyCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

impl TryFrom<u16> for PropertyCode {
    type Error = CatalogError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        Self::new(value).ok_or(CatalogError::Empty { row: "a property code" })
    }
}

impl From<PropertyCode> for u16 {
    fn from(code: PropertyCode) -> Self {
        code.0
    }
}

/// A colour in the document's colour form, for display.
///
/// The directory stores numbers a person's monitor shows; the shell translates them into whatever
/// the render side wants, so `scene` and `render` never see a directory type (`§7`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rgb {
    /// Red channel.
    pub red: u8,
    /// Green channel.
    pub green: u8,
    /// Blue channel.
    pub blue: u8,
}

impl Rgb {
    /// A colour from its three channels.
    pub const fn new(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue }
    }
}

/// A calendar date, as a package writes it.
///
/// The directory carries no clock and no time zone: a price has a date because a person typed one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "DateWire", into = "DateWire")]
pub struct Date {
    year: i32,
    month: u8,
    day: u8,
}

/// The three numbers a date is written with, so nothing is parsed out of a string.
#[derive(Serialize, Deserialize)]
struct DateWire {
    year: i32,
    month: u8,
    day: u8,
}

impl Date {
    /// The first year a date may name: the Gregorian calendar's own start.
    pub const MIN_YEAR: i32 = 1582;
    /// The last year a date may name.
    pub const MAX_YEAR: i32 = 9999;

    /// Validates a date, refusing a month or a day the calendar does not have.
    pub fn new(year: i32, month: u8, day: u8) -> Result<Self, CatalogError> {
        let days = days_in_month(year, month);
        let readable =
            (Self::MIN_YEAR..=Self::MAX_YEAR).contains(&year) && days.is_some_and(|days| (1..=days).contains(&day));
        if readable {
            Ok(Self { year, month, day })
        } else {
            Err(CatalogError::Date { text: format!("{year}-{month:02}-{day:02}") })
        }
    }

    /// The four-digit year.
    pub const fn year(self) -> i32 {
        self.year
    }

    /// The month, January being one.
    pub const fn month(self) -> u8 {
        self.month
    }

    /// The day of the month.
    pub const fn day(self) -> u8 {
        self.day
    }
}

impl TryFrom<DateWire> for Date {
    type Error = String;

    fn try_from(wire: DateWire) -> Result<Self, Self::Error> {
        let days = days_in_month(wire.year, wire.month);
        let readable = (Date::MIN_YEAR..=Date::MAX_YEAR).contains(&wire.year)
            && days.is_some_and(|last| (1..=last).contains(&wire.day));
        if readable {
            Ok(Self { year: wire.year, month: wire.month, day: wire.day })
        } else {
            Err(format!("`{}-{:02}-{:02}` is not a date", wire.year, wire.month, wire.day))
        }
    }
}

impl From<Date> for DateWire {
    fn from(date: Date) -> Self {
        Self { year: date.year, month: date.month, day: date.day }
    }
}

/// How many days `month` has in `year`, or `None` when `month` is not a month.
fn days_in_month(year: i32, month: u8) -> Option<u8> {
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => Some(31),
        4 | 6 | 9 | 11 => Some(30),
        2 if leap => Some(29),
        2 => Some(28),
        _ => None,
    }
}

/// What a property's value is, and the range it must stay inside.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PropertyType {
    /// A length in micrometres between two ends.
    Length {
        /// The smallest accepted length, inclusive.
        min: Length,
        /// The largest accepted length, inclusive.
        max: Length,
    },
    /// An unsigned count up to a maximum.
    Count {
        /// The largest accepted count, inclusive.
        max: u64,
    },
    /// Whole minor units of the catalogue's currency.
    Money,
    /// A text of at most `max_chars` characters.
    Text {
        /// The longest accepted text, in characters.
        max_chars: usize,
    },
    /// One slug of a closed list the row carries.
    Enum(BTreeSet<String>),
    /// Another item's id.
    Ref,
    /// A calendar date.
    Date,
    /// A colour, for display.
    Color,
}

impl PropertyType {
    /// Whether `value` is this type and inside its range; the reason when it is not.
    pub fn check(&self, value: &PropertyValue) -> Result<(), String> {
        match (self, value) {
            (Self::Length { min, max }, PropertyValue::Length(length)) => {
                if length < min || length > max {
                    return Err(format!("{length} is outside {min}…{max}"));
                }
                Ok(())
            }
            (Self::Count { max }, PropertyValue::Count(count)) => {
                if count > max {
                    return Err(format!("{count} is more than the {max} this property allows"));
                }
                Ok(())
            }
            (Self::Money, PropertyValue::Money(_)) | (Self::Ref, PropertyValue::Ref(_)) => Ok(()),
            (Self::Text { max_chars }, PropertyValue::Text(text)) => {
                let length = text.chars().count();
                if length > *max_chars {
                    return Err(format!("the text is {length} characters long; the limit is {max_chars}"));
                }
                if text.trim().is_empty() {
                    return Err("the text is empty".to_owned());
                }
                Ok(())
            }
            (Self::Enum(values), PropertyValue::Enum(chosen)) => {
                if !values.contains(chosen) {
                    return Err(format!(
                        "`{chosen}` is not one of {}",
                        values.iter().cloned().collect::<Vec<_>>().join(", ")
                    ));
                }
                Ok(())
            }
            (Self::Date, PropertyValue::Date(_)) | (Self::Color, PropertyValue::Color(_)) => Ok(()),
            _ => Err(format!("{} is not the value this property declares", value.type_name())),
        }
    }
}

/// One property registry row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Property {
    code: PropertyCode,
    slug: Slug,
    label: String,
    ty: PropertyType,
}

impl Property {
    /// Builds a row, refusing a closed list that is empty or repeats a value.
    pub fn new(
        code: PropertyCode,
        slug: Slug,
        label: impl Into<String>,
        ty: PropertyType,
    ) -> Result<Self, CatalogError> {
        let label = label.into();
        if label.trim().is_empty() {
            return Err(CatalogError::Empty { row: "a property label" });
        }
        if let PropertyType::Enum(values) = &ty
            && values.is_empty()
        {
            return Err(CatalogError::EnumList { slug: slug.to_string() });
        }
        Ok(Self { code, slug, label, ty })
    }

    /// The row's code.
    pub const fn code(&self) -> PropertyCode {
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

    /// The value type and the range it declares.
    pub const fn ty(&self) -> &PropertyType {
        &self.ty
    }
}

/// A value an item carries, stored typed so no reader interprets a string.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "kebab-case")]
pub enum PropertyValue {
    /// A length in micrometres.
    Length(Length),
    /// An unsigned count.
    Count(u64),
    /// Whole minor units of the catalogue's currency.
    Money(Money),
    /// A text.
    Text(String),
    /// One slug of a closed list.
    Enum(String),
    /// Another item's id.
    Ref(ItemId),
    /// A calendar date.
    Date(Date),
    /// A colour, for display.
    Color(Rgb),
}

impl PropertyValue {
    /// What the value calls itself in a refusal.
    pub const fn type_name(&self) -> &'static str {
        match self {
            Self::Length(_) => "a length",
            Self::Count(_) => "a count",
            Self::Money(_) => "a price",
            Self::Text(_) => "a text",
            Self::Enum(_) => "a listed value",
            Self::Ref(_) => "an item reference",
            Self::Date(_) => "a date",
            Self::Color(_) => "a colour",
        }
    }
}

/// The typed properties one item carries, by code.
pub type PropertySet = BTreeMap<PropertyCode, PropertyValue>;

/// Every property the connected packages declare.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PropertyRegistry {
    rows: BTreeMap<PropertyCode, Property>,
}

impl PropertyRegistry {
    /// Builds the registry, refusing a code two rows claim.
    pub fn new(rows: impl IntoIterator<Item = Property>) -> Result<Self, CatalogError> {
        let mut registry = Self::default();
        for row in rows {
            if registry.rows.contains_key(&row.code) {
                return Err(CatalogError::DuplicateCode { code: row.code.to_string() });
            }
            registry.rows.insert(row.code, row);
        }
        Ok(registry)
    }

    /// The row with `code`.
    pub fn get(&self, code: PropertyCode) -> Option<&Property> {
        self.rows.get(&code)
    }

    /// Every row, in code order.
    pub fn iter(&self) -> impl Iterator<Item = &Property> {
        self.rows.values()
    }

    /// How many properties the directory declares.
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Whether no package declares a property.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    fn code(value: u16) -> PropertyCode {
        PropertyCode::new(value).expect("a non-zero property code")
    }

    fn length_row() -> Property {
        Property::new(
            code(1),
            Slug::new("thickness").expect("a slug"),
            "Thickness",
            PropertyType::Length { min: Length::from_micrometres(1_000), max: Length::from_micrometres(100_000) },
        )
        .expect("a property")
    }

    #[test]
    fn a_length_inside_its_range_is_accepted_and_one_outside_is_refused() {
        let row = length_row();
        let inside = PropertyValue::Length(Length::from_micrometres(18_000));
        assert_eq!(row.ty().check(&inside), Ok(()));
        let too_thick = PropertyValue::Length(Length::from_micrometres(400_000));
        let reason = row.ty().check(&too_thick).expect_err("outside the range");
        assert!(reason.contains("400 mm"), "the sentence quotes the value and the ends: {reason}");
    }

    #[test]
    fn a_value_of_another_type_says_which_one_it_found() {
        let row = length_row();
        let reason = row.ty().check(&PropertyValue::Text("18".to_owned())).expect_err("a text is not a length");
        assert!(reason.contains("a text"), "{reason}");
    }

    #[test]
    fn a_closed_list_refuses_a_word_outside_it() {
        let values = BTreeSet::from(["veneer".to_owned(), "lacquer".to_owned()]);
        let ty = PropertyType::Enum(values);
        assert_eq!(ty.check(&PropertyValue::Enum("veneer".to_owned())), Ok(()));
        let reason = ty.check(&PropertyValue::Enum("paint".to_owned())).expect_err("not listed");
        assert!(reason.contains("paint") && reason.contains("veneer"), "the refusal lists what is accepted: {reason}");
    }

    #[test]
    fn a_date_knows_which_days_a_month_has() {
        assert!(Date::new(2024, 2, 29).is_ok(), "a leap year has a 29th");
        assert!(Date::new(2023, 2, 29).is_err());
        assert!(Date::new(2024, 13, 1).is_err());
        assert!(Date::new(2024, 4, 31).is_err());
        let refusal = Date::new(2023, 2, 29).expect_err("a day the month does not have");
        assert!(refusal.to_string().contains("2023-02-29"), "the sentence quotes the date: {refusal}");
        let date = Date::new(2026, 11, 18).expect("a date");
        assert_eq!((date.year(), date.month(), date.day()), (2026, 11, 18));
        assert!(
            serde_json::from_str::<Date>("{\"year\":2024,\"month\":2,\"day\":30}").is_err(),
            "a hand-written package is refused, not reinterpreted"
        );
    }

    #[test]
    fn a_property_value_is_stored_typed() {
        let value = PropertyValue::Length(Length::from_micrometres(18_000));
        let json = serde_json::to_string(&value).expect("serialize");
        assert_eq!(json, "{\"type\":\"length\",\"value\":18000}");
        assert_eq!(serde_json::from_str::<PropertyValue>(&json).expect("read back"), value);
    }

    #[test]
    fn a_registry_refuses_a_code_two_rows_claim() {
        let twice = PropertyRegistry::new([length_row(), length_row()]).expect_err("one code, one row");
        assert!(twice.to_string().contains("declared twice"), "{twice}");
    }
}
