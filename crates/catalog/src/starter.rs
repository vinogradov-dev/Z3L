//! The starter package: the rows a workspace opens with, written as data.
//!
//! A workspace with no directory cannot make a panel, and a package is what a directory is made of,
//! so the first package is shipped in code the same way its file will be shipped later: two kinds,
//! their property rows and the records a workshop starts from. Nothing here is a constant a consumer
//! reads — the reader asks the [`Directory`] it builds, exactly as it will ask a package off disk.
//!
//! The board is stocked as a 2800 x 2070 mm sheet and priced by the sheet, because the order package
//! writes both and an empty stock block would leave those columns untested (ADR 0120).

use std::collections::BTreeMap;

use model::ItemId;
use units::Length;

use crate::directory::Directory;
use crate::error::CatalogError;
use crate::family::FamilyRegistry;
use crate::item::NomenclatureItem;
use crate::kind::{Kind, KindCode, KindRegistry, Trait};
use crate::money::{Currency, Money};
use crate::property::{Property, PropertyCode, PropertyRegistry, PropertyType, PropertyValue};
use crate::slug::Slug;
use crate::view::{COLOR_SLUG, SHEET_LENGTH_SLUG, SHEET_WIDTH_SLUG, THICKNESS_SLUG, WIDTH_SLUG};

/// The package number the starter package owns.
pub const STARTER_PACKAGE: u32 = 1;

/// The kind code of a sheet material.
pub const SHEET_KIND_CODE: u16 = 1;
/// The kind code of an edge banding.
pub const BANDING_KIND_CODE: u16 = 2;

/// The property naming what a record is made of, in the workshop's words: `chipboard`, `abs`.
pub const MATERIAL_SLUG: &str = "material";
/// The property naming the stock group a record is bought in.
pub const GROUP_SLUG: &str = "group";
/// The property naming the price of one purchase unit.
pub const PRICE_SLUG: &str = "price";

/// The smallest board, band or sheet side a package may declare, in millimetres.
pub const MIN_SIZE_MM: i64 = 1;
/// The largest thickness or width a package may declare, in millimetres.
///
/// One metre, the document's own ceiling for a board's thickness: the row is shared by every kind
/// that carries it, so it holds the widest value a part may take rather than the common one.
pub const MAX_SIZE_MM: i64 = 1_000;
/// The largest sheet side a package may declare, in millimetres.
pub const MAX_SHEET_MM: i64 = 6_000;

/// A board the package carries.
#[derive(Clone, Debug)]
struct BoardRow {
    serial: u32,
    name: String,
    material: String,
    thickness: Length,
    color: String,
    sheet: Option<(Length, Length)>,
    group: Option<String>,
    price: Option<Money>,
}

/// An edge band the package carries.
#[derive(Clone, Debug)]
struct BandRow {
    serial: u32,
    name: String,
    material: String,
    thickness: Length,
    width: Length,
    color: String,
}

/// A package built row by row, then read as one [`Directory`].
#[derive(Clone, Debug, Default)]
pub struct Starter {
    boards: Vec<BoardRow>,
    bands: Vec<BandRow>,
    currency: Option<Currency>,
}

/// A code that is always a property code: a constant table cannot fail, and the smallest code is the
/// one a table of our own rows falls back to.
fn code(value: u16) -> PropertyCode {
    PropertyCode::new(value).unwrap_or(PropertyCode::MIN)
}

fn kind_code(value: u16) -> KindCode {
    KindCode::new(value).unwrap_or(KindCode::MIN)
}

impl Starter {
    /// The two kinds and their property rows, with no record yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a board: the record a panel's material names.
    pub fn board(
        mut self,
        serial: u32,
        name: &str,
        material: &str,
        thickness: Length,
        color: &str,
        sheet: Option<(Length, Length)>,
    ) -> Self {
        self.boards.push(BoardRow {
            serial,
            name: name.to_owned(),
            material: material.to_owned(),
            thickness,
            color: color.to_owned(),
            sheet,
            group: None,
            price: None,
        });
        self
    }

    /// Adds the stock group and the whole-sheet price to the last board added.
    pub fn stocked(mut self, group: &str, price: Money) -> Self {
        if let Some(board) = self.boards.last_mut() {
            board.group = Some(group.to_owned());
            board.price = Some(price);
        }
        self
    }

    /// Adds an edge band: the record a panel's band names.
    pub fn banding(
        mut self,
        serial: u32,
        name: &str,
        material: &str,
        thickness: Length,
        width: Length,
        color: &str,
    ) -> Self {
        self.bands.push(BandRow {
            serial,
            name: name.to_owned(),
            material: material.to_owned(),
            thickness,
            width,
            color: color.to_owned(),
        });
        self
    }

    /// The currency the package's prices are written in.
    pub fn currency(mut self, currency: Option<Currency>) -> Self {
        self.currency = currency;
        self
    }

    /// Reads the rows as one directory, refusing a duplicate id or an out-of-range value.
    pub fn directory(self) -> Result<Directory, CatalogError> {
        let properties = properties()?;
        let kinds = KindRegistry::new([sheet_kind()?, banding_kind()?])?;
        let mut items = Vec::with_capacity(self.boards.len() + self.bands.len());
        for board in &self.boards {
            let mut props = BTreeMap::new();
            props.insert(code(THICKNESS), PropertyValue::Length(board.thickness));
            props.insert(code(COLOR), PropertyValue::Text(board.color.clone()));
            props.insert(code(MATERIAL), PropertyValue::Text(board.material.clone()));
            if let Some((length, width)) = board.sheet {
                props.insert(code(SHEET_LENGTH), PropertyValue::Length(length));
                props.insert(code(SHEET_WIDTH), PropertyValue::Length(width));
            }
            if let Some(group) = &board.group {
                props.insert(code(GROUP), PropertyValue::Text(group.clone()));
            }
            if let Some(price) = board.price {
                props.insert(code(PRICE), PropertyValue::Money(price));
            }
            let id =
                ItemId::new(STARTER_PACKAGE, board.serial).ok_or(CatalogError::Empty { row: "a board's serial" })?;
            let slug = Slug::new(format!("board-{}", board.serial))?;
            items.push(NomenclatureItem::new(id, kind_code(SHEET_KIND_CODE), board.name.clone(), slug, props)?);
        }
        for band in &self.bands {
            let mut props = BTreeMap::new();
            props.insert(code(THICKNESS), PropertyValue::Length(band.thickness));
            props.insert(code(WIDTH), PropertyValue::Length(band.width));
            props.insert(code(COLOR), PropertyValue::Text(band.color.clone()));
            props.insert(code(MATERIAL), PropertyValue::Text(band.material.clone()));
            let id = ItemId::new(STARTER_PACKAGE, band.serial).ok_or(CatalogError::Empty { row: "a band's serial" })?;
            let slug = Slug::new(format!("band-{}", band.serial))?;
            items.push(NomenclatureItem::new(id, kind_code(BANDING_KIND_CODE), band.name.clone(), slug, props)?);
        }
        let directory = Directory::new(items, kinds, properties, FamilyRegistry::default())?;
        Ok(directory.with_currency(self.currency))
    }
}

/// The property codes of the starter package, in the order its rows declare them.
const THICKNESS: u16 = 1;
const WIDTH: u16 = 2;
const COLOR: u16 = 3;
const SHEET_LENGTH: u16 = 4;
const SHEET_WIDTH: u16 = 5;
const MATERIAL: u16 = 6;
const GROUP: u16 = 7;
const PRICE: u16 = 8;

fn slug(text: &str) -> Result<Slug, CatalogError> {
    Slug::new(text)
}

fn millimetre_range(max_mm: i64) -> PropertyType {
    PropertyType::Length {
        min: Length::from_micrometres(MIN_SIZE_MM.saturating_mul(1_000)),
        max: Length::from_micrometres(max_mm.saturating_mul(1_000)),
    }
}

fn properties() -> Result<PropertyRegistry, CatalogError> {
    PropertyRegistry::new([
        Property::new(code(THICKNESS), slug(THICKNESS_SLUG)?, "Thickness", millimetre_range(MAX_SIZE_MM))?,
        Property::new(code(WIDTH), slug(WIDTH_SLUG)?, "Width", millimetre_range(MAX_SIZE_MM))?,
        Property::new(code(COLOR), slug(COLOR_SLUG)?, "Decor", PropertyType::Text { max_chars: 48 })?,
        Property::new(code(SHEET_LENGTH), slug(SHEET_LENGTH_SLUG)?, "Sheet length", millimetre_range(MAX_SHEET_MM))?,
        Property::new(code(SHEET_WIDTH), slug(SHEET_WIDTH_SLUG)?, "Sheet width", millimetre_range(MAX_SHEET_MM))?,
        Property::new(code(MATERIAL), slug(MATERIAL_SLUG)?, "Made of", PropertyType::Text { max_chars: 48 })?,
        Property::new(code(GROUP), slug(GROUP_SLUG)?, "Stock group", PropertyType::Text { max_chars: 64 })?,
        Property::new(code(PRICE), slug(PRICE_SLUG)?, "Price", PropertyType::Money)?,
    ])
}

fn sheet_kind() -> Result<Kind, CatalogError> {
    let accepted = [THICKNESS, COLOR, SHEET_LENGTH, SHEET_WIDTH, MATERIAL, GROUP, PRICE].map(code);
    Kind::new(
        kind_code(SHEET_KIND_CODE),
        slug("sheet-material")?,
        "Sheet material",
        accepted,
        [code(THICKNESS)],
        [Trait::Sheet, Trait::Nestable, Trait::Shop, Trait::PriceSheet],
    )
}

fn banding_kind() -> Result<Kind, CatalogError> {
    let accepted = [THICKNESS, WIDTH, COLOR, MATERIAL, PRICE].map(code);
    Kind::new(
        kind_code(BANDING_KIND_CODE),
        slug("edge-banding")?,
        "Edge banding",
        accepted,
        [code(THICKNESS), code(WIDTH)],
        [Trait::Banded, Trait::Bought, Trait::PriceMetre],
    )
}

impl Directory {
    /// The currency the directory's prices are written in, when its packages name one.
    pub const fn currency(&self) -> Option<&Currency> {
        self.currency.as_ref()
    }

    /// The same directory with the currency its prices are written in.
    pub fn with_currency(mut self, currency: Option<Currency>) -> Self {
        self.currency = currency;
        self
    }
}

/// The directory a workspace opens with: an oak and a white board, and their two edges.
pub fn default_directory() -> Result<Directory, CatalogError> {
    let millimetres = |value: i64| Length::from_micrometres(value.saturating_mul(1_000));
    let sheet = Some((millimetres(2_800), millimetres(2_070)));
    Starter::new()
        .board(1, "Oak veneered chipboard", "chipboard", millimetres(18), "oak", sheet)
        .stocked("chipboard-18-oak", Money::from_minor(320_000))
        .board(2, "White melamine chipboard", "chipboard", millimetres(18), "white", sheet)
        .stocked("chipboard-18-white", Money::from_minor(240_000))
        .banding(101, "Oak ABS edge", "abs", millimetres(2), millimetres(22), "oak")
        .banding(102, "White ABS edge", "abs", millimetres(2), millimetres(22), "white")
        .currency(Currency::new("RUB", 100).ok())
        .directory()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    #[test]
    fn the_default_directory_answers_a_board_and_a_band() {
        let directory = default_directory().expect("the starter package");
        let oak = ItemId::new(STARTER_PACKAGE, 1).expect("an item id");
        let board = directory.board(oak).expect("a board");
        assert_eq!(board.name, "Oak veneered chipboard");
        assert_eq!(board.thickness, Length::from_micrometres(18_000));
        let sheet = board.sheet.expect("the board is stocked");
        assert_eq!(
            (sheet.length, sheet.width),
            (Length::from_micrometres(2_800_000), Length::from_micrometres(2_070_000))
        );
        assert_eq!(directory.boards().count(), 2);

        let edge = ItemId::new(STARTER_PACKAGE, 101).expect("an item id");
        let band = directory.banding(edge).expect("a band");
        assert_eq!((band.thickness, band.width), (Length::from_micrometres(2_000), Length::from_micrometres(22_000)));
        assert!(directory.board(edge).is_none(), "a band is not a board");
        assert_eq!(directory.currency().map(Currency::code), Some("RUB"));
    }

    #[test]
    fn a_board_without_a_serial_is_refused() {
        let refusal = Starter::new()
            .board(0, "No serial", "chipboard", Length::from_micrometres(18_000), "white", None)
            .directory()
            .expect_err("a record needs a serial");
        assert!(refusal.to_string().contains("serial"), "{refusal}");
    }

    #[test]
    fn a_value_outside_the_declared_range_is_refused_with_the_record_named() {
        let refusal = Starter::new()
            .board(7, "Fat board", "chipboard", Length::from_micrometres(5_000_000), "white", None)
            .directory()
            .expect_err("5 m is not a board");
        let sentence = refusal.to_string();
        assert!(sentence.contains("1-7") && sentence.contains("5000 mm"), "{sentence}");
    }
}
