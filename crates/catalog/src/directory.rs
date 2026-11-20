//! The loaded directory: every registry row and every item, validated as one value.
//!
//! A reader holds one `Directory` of one revision and the shell swaps it whole (`§13`): there is no
//! half-loaded directory, and a package that fails validation leaves the previous value in place.

use std::collections::BTreeMap;

use model::ItemId;

use crate::error::CatalogError;
use crate::external::ExternalSystemRegistry;
use crate::family::FamilyRegistry;
use crate::item::NomenclatureItem;
use crate::kind::{KindCode, KindRegistry};
use crate::overlay::{OnHand, StockOverlay};
use crate::property::PropertyRegistry;

/// Everything the connected packages declare, indexed for the queries consumers use.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Directory {
    items: BTreeMap<ItemId, NomenclatureItem>,
    kinds: KindRegistry,
    properties: PropertyRegistry,
    families: FamilyRegistry,
    systems: ExternalSystemRegistry,
    by_kind: BTreeMap<KindCode, Vec<ItemId>>,
    by_slug: BTreeMap<KindCode, BTreeMap<String, ItemId>>,
    by_external: BTreeMap<(String, String), ItemId>,
    overlays: Vec<StockOverlay>,
    on_hand: BTreeMap<ItemId, OnHand>,
    pub(crate) currency: Option<crate::money::Currency>,
}

impl Directory {
    /// Validates the rows and builds the indexes, refusing the first row that breaks a rule.
    ///
    /// The order is the checklist of `§16`: an id claimed twice, then each item against the
    /// registries, then a slug two items claim inside one kind. Every refusal names the row, because
    /// a package arrives from a supplier and a person has to find the line.
    pub fn new(
        items: impl IntoIterator<Item = NomenclatureItem>,
        kinds: KindRegistry,
        properties: PropertyRegistry,
        families: FamilyRegistry,
        systems: ExternalSystemRegistry,
    ) -> Result<Self, CatalogError> {
        let mut directory = Self { kinds, properties, families, systems, ..Self::default() };
        for item in items {
            let id = item.id();
            if directory.items.contains_key(&id) {
                return Err(CatalogError::DuplicateItem { id });
            }
            let kind = item.kind();
            if let Some(other) = directory.by_slug.get(&kind).and_then(|slugs| slugs.get(item.slug().as_str())).copied()
            {
                return Err(CatalogError::DuplicateItemSlug { item: id, other, kind, slug: item.slug().to_string() });
            }
            directory.by_slug.entry(kind).or_default().insert(item.slug().to_string(), id);
            directory.by_kind.entry(kind).or_default().push(id);
            directory.items.insert(id, item);
        }
        let ids: Vec<ItemId> = directory.items.keys().copied().collect();
        for id in ids {
            let Some(item) = directory.items.get(&id).cloned() else {
                continue;
            };
            item.check(&directory)?;
            directory.index_external(&item)?;
        }
        // Prices outlive the rows that wrote them: the currency is the one every record agrees on, and
        // a second one is a package mistake rather than a reader's guess (`§11`).
        directory.currency = directory.declared_currency()?;
        Ok(directory)
    }

    /// The record with `id`, when the connected packages carry it.
    pub fn item(&self, id: ItemId) -> Option<&NomenclatureItem> {
        self.items.get(&id)
    }

    /// Every record of `kind`, in id order.
    pub fn items_of_kind(&self, kind: KindCode) -> impl Iterator<Item = &NomenclatureItem> {
        self.by_kind.get(&kind).into_iter().flatten().filter_map(|id| self.items.get(id))
    }

    /// The record of `kind` whose slug is `slug`, when one carries it.
    pub fn item_by_slug(&self, kind: KindCode, slug: &str) -> Option<&NomenclatureItem> {
        self.by_slug.get(&kind).and_then(|slugs| slugs.get(slug)).and_then(|id| self.items.get(id))
    }

    /// Every record, in id order.
    pub fn items(&self) -> impl Iterator<Item = &NomenclatureItem> {
        self.items.values()
    }

    /// The kind registry.
    pub const fn kinds(&self) -> &KindRegistry {
        &self.kinds
    }

    /// The property registry.
    pub const fn properties(&self) -> &PropertyRegistry {
        &self.properties
    }

    /// The family registry.
    pub const fn families(&self) -> &FamilyRegistry {
        &self.families
    }

    /// The overlays whose counts the directory carries, in name order.
    pub fn overlays(&self) -> &[StockOverlay] {
        &self.overlays
    }

    /// How many of `id` are on hand, with the overlay that measured it.
    ///
    /// A record with no overlay answers `None` and not a zero: a workshop may know a material it does
    /// not stock, and a zero is a count somebody measured.
    pub fn on_hand(&self, id: ItemId) -> Option<&OnHand> {
        self.on_hand.get(&id)
    }

    /// The same directory with the stock overlays a package carries, refusing a record counted twice.
    ///
    /// Stock is an overlay and not a property (`§11`): it resolves beside the records, it survives a
    /// record being absent, and removing the file leaves every other query answering as before.
    pub fn with_overlays(mut self, overlays: impl IntoIterator<Item = StockOverlay>) -> Result<Self, CatalogError> {
        let mut counted: BTreeMap<ItemId, OnHand> = BTreeMap::new();
        let mut names: BTreeMap<ItemId, String> = BTreeMap::new();
        let mut held: Vec<StockOverlay> = Vec::new();
        for overlay in overlays {
            for (id, count) in overlay.rows() {
                if let Some(other) = names.get(&id) {
                    return Err(CatalogError::StockClaimed {
                        item: id,
                        overlay: overlay.name().to_owned(),
                        other: other.clone(),
                    });
                }
                names.insert(id, overlay.name().to_owned());
                counted.insert(id, OnHand::of(&overlay, count));
            }
            held.push(overlay);
        }
        held.sort_by(|left, right| left.name().cmp(right.name()));
        self.overlays = held;
        self.on_hand = counted;
        Ok(self)
    }

    /// The registry of the foreign systems an item's keys name.
    pub const fn systems(&self) -> &ExternalSystemRegistry {
        &self.systems
    }

    /// The record a foreign system knows by `key`, when one claims it.
    ///
    /// An import matches its columns through this and never by a record's name (`§5.3`).
    pub fn by_external(&self, system: &str, key: &str) -> Option<&NomenclatureItem> {
        let id = self.by_external.get(&(system.to_owned(), key.to_owned()))?;
        self.items.get(id)
    }

    /// Files one record's foreign keys under the pair an import asks by, refusing a pair two claim.
    fn index_external(&mut self, item: &NomenclatureItem) -> Result<(), CatalogError> {
        for external in item.external() {
            let pair = (external.system.as_str().to_owned(), external.key.clone());
            if let Some(other) = self.by_external.get(&pair) {
                return Err(CatalogError::ExternalKeyClaimed {
                    item: item.id(),
                    other: *other,
                    system: pair.0,
                    key: pair.1,
                });
            }
            self.by_external.insert(pair, item.id());
        }
        Ok(())
    }

    /// The currency the directory's prices are written in, when any record is priced.
    pub const fn currency(&self) -> Option<&crate::money::Currency> {
        self.currency.as_ref()
    }

    /// Reads the one currency the priced records agree on, refusing a directory priced in two.
    fn declared_currency(&self) -> Result<Option<crate::money::Currency>, CatalogError> {
        let mut found: Option<(ItemId, &crate::money::Currency)> = None;
        for item in self.items.values() {
            for value in item.props().values() {
                let crate::property::PropertyValue::Money(price) = value else {
                    continue;
                };
                match found {
                    Some((other, other_currency)) if other_currency != price.currency() => {
                        return Err(CatalogError::MixedCurrency {
                            item: item.id(),
                            currency: price.currency().code().to_owned(),
                            other,
                            other_currency: other_currency.code().to_owned(),
                        });
                    }
                    Some(_) => {}
                    None => found = Some((item.id(), price.currency())),
                }
            }
        }
        Ok(found.map(|(_, currency)| currency.clone()))
    }

    /// How many records the directory carries.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether no package carries a record.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use std::collections::BTreeMap;

    use units::Length;

    use super::*;
    use crate::external::ExternalSystemRegistry;
    use crate::family::{Family, FamilyCode};
    use crate::kind::{Kind, Trait};
    use crate::money::{Currency, Money};
    use crate::price::Price;
    use crate::property::{Property, PropertyCode, PropertyType, PropertyValue};
    use crate::slug::Slug;

    fn id(package: u32, serial: u32) -> ItemId {
        ItemId::new(package, serial).expect("a non-zero item id")
    }

    fn kind() -> Kind {
        Kind::new(
            KindCode::new(1).expect("a kind code"),
            Slug::new("sheet-material").expect("a slug"),
            "Sheet material",
            [PropertyCode::new(1).expect("a property code")],
            [PropertyCode::new(1).expect("a property code")],
            [Trait::Sheet, Trait::Nestable],
        )
        .expect("a kind")
    }

    fn thickness() -> Property {
        Property::new(
            PropertyCode::new(1).expect("a property code"),
            Slug::new("thickness").expect("a slug"),
            "Thickness",
            PropertyType::Length { min: Length::from_micrometres(1_000), max: Length::from_micrometres(60_000) },
        )
        .expect("a property")
    }

    fn board(serial: u32, name: &str, slug: &str) -> NomenclatureItem {
        let props = BTreeMap::from([(
            PropertyCode::new(1).expect("a property code"),
            PropertyValue::Length(Length::from_micrometres(18_000)),
        )]);
        NomenclatureItem::new(
            id(712, serial),
            KindCode::new(1).expect("a kind code"),
            name,
            Slug::new(slug).expect("a slug"),
            props,
        )
        .expect("an item")
    }

    fn directory(items: impl IntoIterator<Item = NomenclatureItem>) -> Result<Directory, CatalogError> {
        directory_with(items, ExternalSystemRegistry::default())
    }

    fn directory_with(
        items: impl IntoIterator<Item = NomenclatureItem>,
        systems: ExternalSystemRegistry,
    ) -> Result<Directory, CatalogError> {
        Directory::new(
            items,
            KindRegistry::new([kind()]).expect("one kind"),
            PropertyRegistry::new([thickness()]).expect("one property"),
            FamilyRegistry::new([Family::new(
                FamilyCode::new(1).expect("a family code"),
                Slug::new("oak").expect("a slug"),
                "Oak",
            )
            .expect("a family")])
            .expect("one family"),
            systems,
        )
    }

    fn system(text: &str) -> crate::item::ExternalSystemId {
        crate::item::ExternalSystemId::new(text).expect("a system id")
    }

    fn declared_systems() -> ExternalSystemRegistry {
        ExternalSystemRegistry::new([crate::external::ExternalSystem::new(system("erp"), "ERP").expect("a row")])
            .expect("one system")
    }

    #[test]
    fn a_record_is_found_by_id_by_kind_and_by_slug() {
        let directory = directory([board(34, "White melamine", "chipboard-18-white")]).expect("a directory");
        let item = directory.item(id(712, 34)).expect("the record");
        assert_eq!(item.name(), "White melamine");

        let kind = KindCode::new(1).expect("a kind code");
        assert_eq!(directory.items_of_kind(kind).count(), 1);
        assert_eq!(directory.item_by_slug(kind, "chipboard-18-white").map(NomenclatureItem::id), Some(id(712, 34)));
        assert!(directory.item(id(712, 35)).is_none());
        assert!(directory.item_by_slug(kind, "chipboard-18-oak").is_none());
        assert_eq!(directory.len(), 1);
    }

    #[test]
    fn a_missing_required_property_names_the_item_and_the_row() {
        let bare = NomenclatureItem::new(
            id(712, 34),
            KindCode::new(1).expect("a kind code"),
            "White melamine",
            Slug::new("white").expect("a slug"),
            BTreeMap::new(),
        )
        .expect("an item");
        let refusal = directory([bare]).expect_err("a sheet without a thickness");
        let sentence = refusal.to_string();
        assert!(
            sentence.contains("712-34") && sentence.contains("`1`"),
            "the item and the property are named: {sentence}"
        );
    }

    #[test]
    fn a_value_outside_its_range_is_refused_with_the_item_and_the_property() {
        let props = BTreeMap::from([(
            PropertyCode::new(1).expect("a property code"),
            PropertyValue::Length(Length::from_micrometres(200_000)),
        )]);
        let too_thick = NomenclatureItem::new(
            id(712, 34),
            KindCode::new(1).expect("a kind code"),
            "Fat board",
            Slug::new("fat").expect("a slug"),
            props,
        )
        .expect("an item");
        let refusal = directory([too_thick]).expect_err("200 mm is not a board");
        let sentence = refusal.to_string();
        assert!(sentence.contains("712-34") && sentence.contains("200 mm"), "{sentence}");
    }

    #[test]
    fn an_item_id_and_a_slug_inside_one_kind_are_claimed_once() {
        let refusal = directory([board(34, "One", "white"), board(34, "Two", "oak")]).expect_err("one id, one record");
        assert!(refusal.to_string().contains("declared twice"), "{refusal}");

        let refusal = directory([board(34, "One", "white"), board(35, "Two", "white")]).expect_err("one slug per kind");
        let sentence = refusal.to_string();
        assert!(sentence.contains("712-35") && sentence.contains("712-34"), "both records are named: {sentence}");
    }

    #[test]
    fn an_unknown_kind_is_refused_before_its_properties_are_read() {
        let item = NomenclatureItem::new(
            id(712, 34),
            KindCode::new(9).expect("a kind code"),
            "Mystery",
            Slug::new("mystery").expect("a slug"),
            BTreeMap::new(),
        )
        .expect("an item");
        let refusal = directory([item]).expect_err("no package declares kind 9");
        assert!(refusal.to_string().contains("kind `9`"), "{refusal}");
    }

    /// A kind priced by the sheet, with one price row.
    fn priced_kind(traits: impl IntoIterator<Item = Trait>) -> Kind {
        Kind::new(
            KindCode::new(2).expect("a kind code"),
            Slug::new("sheet-material-priced").expect("a slug"),
            "Sheet material",
            [PropertyCode::new(2).expect("a property code")],
            [],
            traits,
        )
        .expect("a kind")
    }

    fn price_row() -> Property {
        Property::new(
            PropertyCode::new(2).expect("a property code"),
            Slug::new("price").expect("a slug"),
            "Price",
            PropertyType::Money,
        )
        .expect("a property")
    }

    fn priced(serial: u32, currency: &str) -> NomenclatureItem {
        let currency = Currency::new(currency, 100).expect("a currency");
        let props = BTreeMap::from([(
            PropertyCode::new(2).expect("a property code"),
            PropertyValue::Money(Price::typed(Money::from_minor(320_000), currency)),
        )]);
        NomenclatureItem::new(
            id(712, serial),
            KindCode::new(2).expect("a kind code"),
            "Oak veneered chipboard",
            Slug::new(format!("priced-{serial}")).expect("a slug"),
            props,
        )
        .expect("an item")
    }

    fn priced_directory(
        items: impl IntoIterator<Item = NomenclatureItem>,
        kind: Kind,
    ) -> Result<Directory, CatalogError> {
        Directory::new(
            items,
            KindRegistry::new([kind]).expect("one kind"),
            PropertyRegistry::new([price_row()]).expect("one property"),
            FamilyRegistry::default(),
            ExternalSystemRegistry::default(),
        )
    }

    #[test]
    fn a_priced_record_needs_a_kind_that_says_what_one_price_buys() {
        // The unit is a trait, so a price on a kind with none can never be read: the load refuses it
        // instead of leaving a consumer to guess.
        let refusal = priced_directory([priced(34, "EUR")], priced_kind([Trait::Sheet]))
            .expect_err("the kind names no price unit");
        let sentence = refusal.to_string();
        assert!(sentence.contains("712-34") && sentence.contains("no price unit"), "{sentence}");
    }

    #[test]
    fn a_directory_is_priced_in_one_currency_and_a_second_one_names_both_records() {
        let directory = priced_directory([priced(34, "EUR")], priced_kind([Trait::PriceSheet])).expect("one currency");
        assert_eq!(
            directory.currency().map(Currency::code),
            Some("EUR"),
            "the directory reads the money its records carry"
        );

        let refusal = priced_directory([priced(34, "EUR"), priced(35, "RUB")], priced_kind([Trait::PriceSheet]))
            .expect_err("one directory, one money");
        let sentence = refusal.to_string();
        assert!(sentence.contains("712-35") && sentence.contains("712-34"), "both records are named: {sentence}");
        assert!(sentence.contains("EUR") && sentence.contains("RUB"), "and both currencies: {sentence}");
    }

    #[test]
    fn a_foreign_key_reads_its_record_and_an_undeclared_system_is_refused() {
        let key = crate::item::ExternalKey::new(system("erp"), "A-1200").expect("a key");
        let anchored = board(34, "White melamine", "white").with_external(key).expect("a key");
        let directory = directory_with([anchored], declared_systems()).expect("a directory");
        assert_eq!(directory.by_external("erp", "A-1200").map(NomenclatureItem::id), Some(id(712, 34)));
        assert!(directory.by_external("erp", "A-1201").is_none(), "nothing is matched by name");

        // A system is a registry row: a key written with a word nobody declared is refused, so `1C`
        // and `1с` cannot become two worlds for one supplier.
        let stranger = crate::item::ExternalKey::new(system("1c"), "K-1").expect("a key");
        let stray = board(35, "Oak", "oak").with_external(stranger).expect("a key");
        let refusal = directory_with([stray], declared_systems()).expect_err("no package declares `1c`");
        let sentence = refusal.to_string();
        assert!(sentence.contains("`1c`") && sentence.contains("712-35"), "{sentence}");
    }

    #[test]
    fn two_records_claiming_one_foreign_key_are_refused() {
        let key = || crate::item::ExternalKey::new(system("erp"), "A-1200").expect("a key");
        let first = board(34, "White melamine", "white").with_external(key()).expect("a key");
        let second = board(35, "Oak", "oak").with_external(key()).expect("a key");
        let refusal = directory_with([first, second], declared_systems()).expect_err("one key, one record");
        let sentence = refusal.to_string();
        assert!(
            sentence.contains("712-35") && sentence.contains("712-34") && sentence.contains("A-1200"),
            "{sentence}"
        );
    }

    #[test]
    fn a_reference_must_resolve_to_a_record_of_the_directory() {
        let property = Property::new(
            PropertyCode::new(2).expect("a property code"),
            Slug::new("plug").expect("a slug"),
            "Plug",
            PropertyType::Ref,
        )
        .expect("a property");
        let kind = Kind::new(
            KindCode::new(2).expect("a kind code"),
            Slug::new("fastener").expect("a slug"),
            "Fastener",
            [PropertyCode::new(2).expect("a property code")],
            [],
            [Trait::Piece],
        )
        .expect("a kind");
        let item = NomenclatureItem::new(
            id(712, 40),
            KindCode::new(2).expect("a kind code"),
            "Dowel",
            Slug::new("dowel").expect("a slug"),
            BTreeMap::from([(PropertyCode::new(2).expect("a property code"), PropertyValue::Ref(id(712, 41)))]),
        )
        .expect("an item");
        let refusal = Directory::new(
            [item],
            KindRegistry::new([kind]).expect("one kind"),
            PropertyRegistry::new([property]).expect("one property"),
            FamilyRegistry::default(),
            ExternalSystemRegistry::default(),
        )
        .expect_err("712-41 is absent");
        assert!(refusal.to_string().contains("712-41"), "{refusal}");
    }
}
