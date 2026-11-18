//! The loaded directory: every registry row and every item, validated as one value.
//!
//! A reader holds one `Directory` of one revision and the shell swaps it whole (`§13`): there is no
//! half-loaded directory, and a package that fails validation leaves the previous value in place.

use std::collections::BTreeMap;

use model::ItemId;

use crate::error::CatalogError;
use crate::family::FamilyRegistry;
use crate::item::NomenclatureItem;
use crate::kind::{KindCode, KindRegistry};
use crate::property::PropertyRegistry;

/// Everything the connected packages declare, indexed for the queries consumers use.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Directory {
    items: BTreeMap<ItemId, NomenclatureItem>,
    kinds: KindRegistry,
    properties: PropertyRegistry,
    families: FamilyRegistry,
    by_kind: BTreeMap<KindCode, Vec<ItemId>>,
    by_slug: BTreeMap<KindCode, BTreeMap<String, ItemId>>,
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
    ) -> Result<Self, CatalogError> {
        let mut directory = Self { kinds, properties, families, ..Self::default() };
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
        for item in directory.items.values() {
            item.check(&directory)?;
        }
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
    use crate::family::{Family, FamilyCode};
    use crate::kind::{Kind, Trait};
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
        )
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
        )
        .expect_err("712-41 is absent");
        assert!(refusal.to_string().contains("712-41"), "{refusal}");
    }
}
