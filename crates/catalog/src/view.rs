//! Narrow typed views: what a consumer asks the directory, so it never matches a kind code.
//!
//! A panel's material is a *board*, an edge band is a *banding*: the consumer says so, the directory
//! answers from the kind's traits and its reserved property slugs (`docs/domain/nomenclature.md`
//! §15). The slugs below are the one vocabulary the directory and its readers share; a kind declares
//! them like any other property, so a workshop's own sheet kind answers the same query.

use model::ItemId;
use units::Length;

use crate::directory::Directory;
use crate::item::NomenclatureItem;
use crate::kind::Trait;
use crate::price::PriceView;
use crate::property::{PropertyType, PropertyValue};
use crate::slug::Slug;

/// The property naming a board's or a band's thickness.
pub const THICKNESS_SLUG: &str = "thickness";
/// The property naming an edge band's width.
pub const WIDTH_SLUG: &str = "width";
/// The property naming what a record looks like, as the decor's own name: `oak`, `white`.
///
/// A text rather than a colour: a workshop reads its decor by name, and the scene's tint arrives
/// with the appearance block, which has its own consumer (`§7`).
pub const COLOR_SLUG: &str = "color";
/// The property naming the stock group a record is bought in.
pub const GROUP_SLUG: &str = "group";
/// The property naming the price of one purchase unit.
pub const PRICE_SLUG: &str = "price";
/// The property naming the long side of the sheet a board is bought in.
pub const SHEET_LENGTH_SLUG: &str = "sheet-length";
/// The property naming the short side of that sheet.
pub const SHEET_WIDTH_SLUG: &str = "sheet-width";

/// The sheet a board is bought in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SheetView {
    /// The long side.
    pub length: Length,
    /// The short side.
    pub width: Length,
}

/// A board, seen the way a panel that is made from it needs it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoardView {
    /// The record.
    pub id: ItemId,
    /// The name a person reads.
    pub name: String,
    /// The nominal thickness a panel takes from it.
    pub thickness: Length,
    /// The decor's name, when the record says: `oak`, `white`.
    pub color: Option<String>,
    /// The sheet it is bought in, when the record carries one.
    pub sheet: Option<SheetView>,
}

/// An edge band, seen the way a panel's band needs it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BandingView {
    /// The record.
    pub id: ItemId,
    /// The name a person reads.
    pub name: String,
    /// The thickness the band adds.
    pub thickness: Length,
    /// The width the band is cut from.
    pub width: Length,
    /// The band's own name for its decor, when the record says.
    pub color: Option<String>,
}

/// The stock block a record is bought in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StockView {
    /// The long side of the sheet.
    pub length: Length,
    /// The short side of the sheet.
    pub width: Length,
    /// The workshop's own stock group, when it has one.
    pub group: Option<String>,
    /// The price of one sheet, with the unit the kind buys in, when the record carries one.
    pub price: Option<PriceView>,
}

impl Directory {
    /// The board with `id`, when the record is of a kind that is bought as a sheet.
    ///
    /// A record that is not a sheet, or one whose kind declares no `thickness`, is not a board: the
    /// answer is `None` and the caller keeps its own fallback, exactly as an absent material does.
    pub fn board(&self, id: ItemId) -> Option<BoardView> {
        let item = self.item(id)?;
        let kind = self.kinds().get(item.kind())?;
        if !kind.has(Trait::Sheet) {
            return None;
        }
        let thickness = self.length(item, THICKNESS_SLUG)?;
        let sheet = match (self.length(item, SHEET_LENGTH_SLUG), self.length(item, SHEET_WIDTH_SLUG)) {
            (Some(length), Some(width)) => Some(SheetView { length, width }),
            _ => None,
        };
        Some(BoardView {
            id,
            name: item.name().to_owned(),
            thickness,
            color: self.text(item, COLOR_SLUG).map(str::to_owned),
            sheet,
        })
    }

    /// The edge band with `id`, when the record is of a banded kind that is not a sheet.
    pub fn banding(&self, id: ItemId) -> Option<BandingView> {
        let item = self.item(id)?;
        let kind = self.kinds().get(item.kind())?;
        if !kind.has(Trait::Banded) || kind.has(Trait::Sheet) {
            return None;
        }
        Some(BandingView {
            id,
            name: item.name().to_owned(),
            thickness: self.length(item, THICKNESS_SLUG)?,
            width: self.length(item, WIDTH_SLUG)?,
            color: self.text(item, COLOR_SLUG).map(str::to_owned),
        })
    }

    /// Every board the directory carries, in id order.
    pub fn boards(&self) -> impl Iterator<Item = BoardView> + '_ {
        self.items().filter_map(move |item| self.board(item.id()))
    }

    /// Every edge band the directory carries, in id order.
    pub fn bandings(&self) -> impl Iterator<Item = BandingView> + '_ {
        self.items().filter_map(move |item| self.banding(item.id()))
    }

    /// A length property of `item`, looked up by the row's slug and read only when it is a length.
    pub fn length(&self, item: &NomenclatureItem, slug: &str) -> Option<Length> {
        let (_, value) = self.prop_by_slug(item, slug)?;
        match value {
            PropertyValue::Length(length) => Some(*length),
            _ => None,
        }
    }

    /// A text property of `item`, looked up by the row's slug and read only when it is a text.
    pub fn text<'a>(&'a self, item: &'a NomenclatureItem, slug: &str) -> Option<&'a str> {
        match self.prop_by_slug(item, slug)?.1 {
            PropertyValue::Text(text) => Some(text.as_str()),
            _ => None,
        }
    }

    /// The stock block of `item`: a sheet, a price and a group, when the record carries them.
    pub fn stock(&self, item: &NomenclatureItem) -> Option<StockView> {
        let length = self.length(item, SHEET_LENGTH_SLUG)?;
        let width = self.length(item, SHEET_WIDTH_SLUG)?;
        let group = self.text(item, GROUP_SLUG).map(str::to_owned);
        Some(StockView { length, width, group, price: self.price(item) })
    }

    /// The code of the row `item`'s kind keeps its price in, when it accepts one.
    ///
    /// An importer writes a price by this code and never by a constant, because a workshop's kind may
    /// name the row `price` and a supplier's kind may name it something else.
    pub fn price_code(&self, item: &NomenclatureItem) -> Option<crate::property::PropertyCode> {
        let kind = self.kinds().get(item.kind())?;
        let wanted = Slug::new(PRICE_SLUG).ok()?;
        kind.accepts()
            .iter()
            .copied()
            .find(|code| self.properties().get(*code).is_some_and(|row| row.slug() == &wanted))
    }

    /// The price `item` carries, with the unit its kind buys in.
    ///
    /// The stored value is an amount with its currency and its provenance; the unit is answered from
    /// the kind's traits, so a record cannot disagree with its own kind about what a price is for
    /// (`§11`). A record with no price, or whose kind names no unit, answers `None` and the caller
    /// keeps its own fallback.
    pub fn price(&self, item: &NomenclatureItem) -> Option<PriceView> {
        let (_, value) = self.prop_by_slug(item, PRICE_SLUG)?;
        let PropertyValue::Money(price) = value else {
            return None;
        };
        let unit = self.kinds().get(item.kind())?.price_unit()?;
        Some(PriceView { price: price.clone(), unit })
    }

    /// The record's value of the property whose row is named `slug`, with its declared type.
    ///
    /// A property the kind does not accept is not the record's: a package may declare a row the kind
    /// never uses, and a reader must not answer from it.
    pub(crate) fn prop_by_slug<'a>(
        &'a self,
        item: &'a NomenclatureItem,
        slug: &str,
    ) -> Option<(PropertyType, &'a PropertyValue)> {
        let kind = self.kinds().get(item.kind())?;
        let wanted = Slug::new(slug).ok()?;
        let code = kind
            .accepts()
            .iter()
            .copied()
            .find(|code| self.properties().get(*code).is_some_and(|row| row.slug() == &wanted))?;
        let row = self.properties().get(code)?;
        let value = item.prop(code)?;
        Some((row.ty().clone(), value))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use std::collections::BTreeMap;

    use super::*;
    use crate::family::FamilyRegistry;
    use crate::kind::{Kind, KindCode};
    use crate::money::{Currency, Money};
    use crate::price::{Price, PriceSource, PriceUnit};
    use crate::property::{Date, Property, PropertyCode, PropertyRegistry};
    use crate::slug::Slug;

    fn code(value: u16) -> PropertyCode {
        PropertyCode::new(value).expect("a non-zero code")
    }

    fn slug(text: &str) -> Slug {
        Slug::new(text).expect("a slug")
    }

    /// The starter package's sheet and banding rows, as a package would declare them.
    fn registry() -> (PropertyRegistry, Kind) {
        let mm = |value: i64| Length::from_micrometres(value * 1_000);
        let range = |min: i64, max: i64| PropertyType::Length { min: mm(min), max: mm(max) };
        let properties = PropertyRegistry::new([
            Property::new(code(1), slug(THICKNESS_SLUG), "Thickness", range(1, 100)).expect("a row"),
            Property::new(code(2), slug(WIDTH_SLUG), "Width", range(1, 100)).expect("a row"),
            Property::new(code(3), slug(COLOR_SLUG), "Decor", PropertyType::Text { max_chars: 48 }).expect("a row"),
            Property::new(code(4), slug(SHEET_LENGTH_SLUG), "Sheet length", range(1, 6_000)).expect("a row"),
            Property::new(code(5), slug(SHEET_WIDTH_SLUG), "Sheet width", range(1, 6_000)).expect("a row"),
            Property::new(code(6), slug(PRICE_SLUG), "Price", PropertyType::Money).expect("a row"),
        ])
        .expect("five rows");
        let kind = Kind::new(
            KindCode::new(1).expect("a kind code"),
            slug("sheet-material"),
            "Sheet material",
            [code(1), code(2), code(3), code(4), code(5), code(6)],
            [code(1)],
            [Trait::Sheet, Trait::Nestable, Trait::PriceSheet],
        )
        .expect("a kind");
        (properties, kind)
    }

    fn item(id: ItemId, kind: u16, props: BTreeMap<PropertyCode, PropertyValue>) -> NomenclatureItem {
        NomenclatureItem::new(id, KindCode::new(kind).expect("a kind code"), "White melamine", slug("white"), props)
            .expect("an item")
    }

    fn directory(item: NomenclatureItem, kinds: impl IntoIterator<Item = Kind>) -> Directory {
        let (properties, _) = registry();
        Directory::new(
            [item],
            crate::kind::KindRegistry::new(kinds).expect("kinds"),
            properties,
            FamilyRegistry::default(),
            crate::external::ExternalSystemRegistry::default(),
        )
        .expect("a directory")
    }

    #[test]
    fn a_sheet_record_answers_with_its_thickness_colour_and_sheet() {
        let millimetres = |value: i64| Length::from_micrometres(value * 1_000);
        let props = BTreeMap::from([
            (code(1), PropertyValue::Length(millimetres(18))),
            (code(3), PropertyValue::Text("white".to_owned())),
            (code(4), PropertyValue::Length(millimetres(2_800))),
            (code(5), PropertyValue::Length(millimetres(2_070))),
        ]);
        let (_, kind) = registry();
        let id = ItemId::new(712, 34).expect("an item id");
        let directory = directory(item(id, 1, props), [kind]);
        let board = directory.board(id).expect("a board");
        assert_eq!(board.thickness, millimetres(18));
        assert_eq!(board.color.as_deref(), Some("white"));
        assert_eq!(board.sheet, Some(SheetView { length: millimetres(2_800), width: millimetres(2_070) }));
        assert_eq!(directory.boards().count(), 1);
    }

    #[test]
    fn a_stocked_board_answers_its_price_with_the_unit_the_kind_buys() {
        let millimetres = |value: i64| Length::from_micrometres(value * 1_000);
        let currency = Currency::new("RUB", 100).expect("a currency");
        let date = Date::new(2026, 11, 20).expect("a date");
        let system = crate::item::ExternalSystemId::new("erp").expect("a system");
        let price = Price::imported(Money::from_minor(320_000), currency, system.clone(), date);
        let props = BTreeMap::from([
            (code(1), PropertyValue::Length(millimetres(18))),
            (code(4), PropertyValue::Length(millimetres(2_800))),
            (code(5), PropertyValue::Length(millimetres(2_070))),
            (code(6), PropertyValue::Money(price.clone())),
        ]);
        let (_, kind) = registry();
        let id = ItemId::new(712, 34).expect("an item id");
        let directory = directory(item(id, 1, props), [kind]);
        let stock = directory.stock(directory.item(id).expect("the record")).expect("a stock block");
        let priced = stock.price.expect("the price the record carries");
        assert_eq!(priced.unit, PriceUnit::Sheet, "a sheet-material kind is priced by the sheet");
        assert_eq!(priced.value().minor(), 320_000);
        assert_eq!(priced.currency().code(), "RUB");
        assert_eq!(priced.price.source(), &PriceSource::imported(system));
        assert_eq!(priced.price.date(), Some(date));
        assert_eq!(directory.currency().map(Currency::code), Some("RUB"));
    }

    #[test]
    fn a_record_of_a_kind_that_is_not_a_sheet_is_not_a_board() {
        let props = BTreeMap::from([(code(1), PropertyValue::Length(Length::from_micrometres(2_000)))]);
        let fastener = Kind::new(
            KindCode::new(2).expect("a kind code"),
            slug("fastener"),
            "Fastener",
            [code(1)],
            [],
            [Trait::Piece, Trait::Bought],
        )
        .expect("a kind");
        let id = ItemId::new(712, 40).expect("an item id");
        let directory = directory(item(id, 2, props), [fastener]);
        assert!(directory.board(id).is_none(), "a bought piece is not a board");
        assert!(directory.banding(id).is_none(), "and not a band either");
    }

    #[test]
    fn a_sheet_without_a_declared_thickness_answers_nothing_rather_than_zero() {
        let (_, declared) = registry();
        // The same kind with nothing required: a package may declare a sheet whose thickness a
        // workshop fills in later, and the directory must answer `None` rather than a zero board.
        let kind = Kind::new(
            declared.code(),
            declared.slug().clone(),
            "Sheet material",
            [code(1), code(2), code(3), code(4), code(5), code(6)],
            [],
            [Trait::Sheet],
        )
        .expect("a kind");
        let id = ItemId::new(712, 34).expect("an item id");
        let directory = directory(item(id, 1, BTreeMap::new()), [kind]);
        assert!(directory.board(id).is_none(), "the caller keeps its own fallback");
    }
}
