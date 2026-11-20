//! The written form of a package: the JSON a workshop edits by hand before it has an editor.
//!
//! Two things are deliberately *not* written as the models hold them:
//!
//! - an item names its kind and its properties by **slug**, not by code, because a person editing a
//!   package reads `"thickness": 18000`, not `"1": {"type": "length", "value": 18000}`. The loader
//!   resolves a slug to its code through the registry and refuses one no row declares;
//! - a physical size is written in micrometres (`min_um`, `max_um`) like the scene file, so one
//!   integer travels from a package to a panel and back without a rounding step.
//!
//! Every form carries a version and refuses `deny_unknown_fields`: an unknown key is a mistake in a
//! hand-edited file, not a value to ignore (`AGENTS.md` §4.5).

use std::collections::BTreeMap;

use model::ItemId;
use serde::{Deserialize, Serialize};

use crate::error::CatalogError;
use crate::family::{Family, FamilyCode};
use crate::item::{ExternalKey, ExternalSystemId, NomenclatureItem};
use crate::kind::{Kind, KindCode, Trait};
use crate::property::{Property, PropertyCode, PropertyType, PropertyValue};
use crate::slug::Slug;

/// The registry schema this build writes and reads.
pub const REGISTRY_VERSION: u32 = 1;
/// The manifest schema this build writes and reads.
pub const MANIFEST_VERSION: u32 = 1;
/// The directory schema this build writes and reads.
pub const DIRECTORY_VERSION: u32 = 1;

/// A package's manifest: who it is and what it calls itself.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestWire {
    /// The schema of this file.
    pub version: u32,
    /// The package's number: the first half of every item id it owns.
    pub id: u32,
    /// The name a person reads.
    pub name: String,
    /// The workshop's own version of the package's content, shown in the directory.
    #[serde(default)]
    pub package_version: u32,
    /// The packages this one was written against, by number and content version.
    ///
    /// A package that names another's kinds, properties or families declares it here, so a
    /// directory that lacks it — or carries an older version of it — refuses the load and says so,
    /// instead of resolving a record through a row that changed under it (`§12`, `§16`).
    #[serde(default)]
    pub requires: Vec<RequiresWire>,
}

/// One package this one depends on, in written form.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequiresWire {
    /// The required package's number.
    pub id: u32,
    /// The required package's name, for the sentence a person reads.
    pub name: String,
    /// The content version this package was written against.
    #[serde(default)]
    pub package_version: u32,
}

/// The registry rows a package carries.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryWire {
    /// The schema of this file.
    pub version: u32,
    /// The kinds the package declares.
    #[serde(default)]
    pub kinds: Vec<KindWire>,
    /// The properties the package declares.
    #[serde(default)]
    pub properties: Vec<PropertyWire>,
    /// The families the package declares.
    #[serde(default)]
    pub families: Vec<FamilyWire>,
    /// The foreign systems the package's records name their keys in.
    #[serde(default)]
    pub systems: Vec<SystemWire>,
}

/// One kind row, in written form.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KindWire {
    /// The kind's code.
    pub code: u16,
    /// The kind's slug.
    pub slug: String,
    /// The kind's name.
    pub label: String,
    /// The properties it accepts, by code.
    #[serde(default)]
    pub accepts: Vec<u16>,
    /// The properties it requires, by code.
    #[serde(default)]
    pub required: Vec<u16>,
    /// The traits it carries.
    #[serde(default)]
    pub traits: Vec<Trait>,
}

/// One property row, in written form.
///
/// The value type is flattened into the row, so its `type` tag is what closes the file: an unknown
/// type is refused. A stray extra key beside the tag is the one thing this form tolerates, because
/// `serde` cannot flatten a tagged enum under `deny_unknown_fields`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropertyWire {
    /// The property's code.
    pub code: u16,
    /// The property's slug: what an item's row is keyed by.
    pub slug: String,
    /// The property's name.
    pub label: String,
    /// Its value type and range.
    #[serde(flatten)]
    pub ty: PropertyTypeWire,
}

/// A property's value type, in written form.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
pub enum PropertyTypeWire {
    /// A length between two ends, in micrometres.
    Length {
        /// The smallest accepted length.
        min_um: i64,
        /// The largest accepted length.
        max_um: i64,
    },
    /// An unsigned count up to a maximum.
    Count {
        /// The largest accepted count.
        max: u64,
    },
    /// Whole minor units of the directory's currency.
    Money,
    /// A text of at most `max_chars` characters.
    Text {
        /// The longest accepted text.
        max_chars: usize,
    },
    /// One value of a closed list.
    Enum {
        /// The accepted values.
        values: Vec<String>,
    },
    /// Another item's id.
    Ref,
    /// A calendar date.
    Date,
    /// A colour, for display.
    Color,
}

/// One foreign system, in written form.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemWire {
    /// The id a key is written with.
    pub id: String,
    /// The name a person reads.
    pub label: String,
}

/// One family row, in written form.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FamilyWire {
    /// The family's code.
    pub code: u16,
    /// The family's slug.
    pub slug: String,
    /// The family's name.
    pub label: String,
}

/// One record, in written form.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemWire {
    /// The record's id, written `package-serial`.
    pub id: ItemId,
    /// The kind, by slug.
    pub kind: String,
    /// The name a person reads.
    pub name: String,
    /// The record's slug inside its kind.
    pub slug: String,
    /// Whether the workshop retired it; it still resolves.
    #[serde(default)]
    pub archived: bool,
    /// The properties, keyed by the property row's slug.
    #[serde(default)]
    pub props: BTreeMap<String, PropertyValue>,
    /// The keys other systems know the record by.
    #[serde(default)]
    pub external: Vec<ExternalKey>,
}

impl ManifestWire {
    /// The manifest of a package with `id` and `name`.
    pub fn new(id: u32, name: &str) -> Self {
        Self { version: MANIFEST_VERSION, id, name: name.to_owned(), package_version: 1, requires: Vec::new() }
    }
}

impl RequiresWire {
    /// One requirement: package `id`, called `name`, at no less than `package_version`.
    pub fn new(id: u32, name: &str, package_version: u32) -> Self {
        Self { id, name: name.to_owned(), package_version }
    }
}

impl RegistryWire {
    /// An empty registry at the current schema.
    pub fn new() -> Self {
        Self { version: REGISTRY_VERSION, ..Self::default() }
    }

    /// Builds the registry rows, refusing a code, a slug or an enum list the row rules reject.
    pub fn rows(&self) -> Result<RegistryRows, CatalogError> {
        let kinds = self
            .kinds
            .iter()
            .map(|kind| {
                Kind::new(
                    KindCode::new(kind.code).ok_or(CatalogError::Empty { row: "a kind code" })?,
                    Slug::new(kind.slug.clone())?,
                    kind.label.clone(),
                    kind.accepts.iter().filter_map(|code| PropertyCode::new(*code)).collect::<Vec<_>>(),
                    kind.required.iter().filter_map(|code| PropertyCode::new(*code)).collect::<Vec<_>>(),
                    kind.traits.iter().copied(),
                )
            })
            .collect::<Result<Vec<_>, CatalogError>>()?;
        let properties = self
            .properties
            .iter()
            .map(|property| {
                Property::new(
                    PropertyCode::new(property.code).ok_or(CatalogError::Empty { row: "a property code" })?,
                    Slug::new(property.slug.clone())?,
                    property.label.clone(),
                    property.ty.to_type(),
                )
            })
            .collect::<Result<Vec<_>, CatalogError>>()?;
        let families = self
            .families
            .iter()
            .map(|family| {
                Family::new(
                    FamilyCode::new(family.code).ok_or(CatalogError::Empty { row: "a family code" })?,
                    Slug::new(family.slug.clone())?,
                    family.label.clone(),
                )
            })
            .collect::<Result<Vec<_>, CatalogError>>()?;
        let systems = self
            .systems
            .iter()
            .map(|system| {
                crate::external::ExternalSystem::new(
                    crate::item::ExternalSystemId::new(system.id.clone())?,
                    system.label.clone(),
                )
            })
            .collect::<Result<Vec<_>, CatalogError>>()?;
        Ok(RegistryRows { kinds, properties, families, systems })
    }
}

impl PropertyTypeWire {
    /// The runtime type, refusing a range that runs backwards.
    pub fn to_type(&self) -> PropertyType {
        match self {
            Self::Length { min_um, max_um } => PropertyType::Length {
                min: units::Length::from_micrometres(*min_um),
                max: units::Length::from_micrometres(*max_um),
            },
            Self::Count { max } => PropertyType::Count { max: *max },
            Self::Money => PropertyType::Money,
            Self::Text { max_chars } => PropertyType::Text { max_chars: *max_chars },
            Self::Enum { values } => PropertyType::Enum(values.iter().cloned().collect()),
            Self::Ref => PropertyType::Ref,
            Self::Date => PropertyType::Date,
            Self::Color => PropertyType::Color,
        }
    }
}

impl ItemWire {
    /// The written form of one record: its kind and every property named by slug.
    ///
    /// The reverse of [`ItemWire::to_item`], and the only way a record reaches a folder: the file a
    /// workshop reads names `"thickness": 18000`, never a property code. A code no row declares is
    /// refused here rather than written, because a package that cannot be read back is worse than a
    /// refused save.
    pub fn from_item(
        item: &NomenclatureItem,
        kinds: &crate::kind::KindRegistry,
        properties: &crate::property::PropertyRegistry,
    ) -> Result<Self, CatalogError> {
        let kind = kinds.get(item.kind()).ok_or(CatalogError::UnknownKind { item: item.id(), kind: item.kind() })?;
        let mut props = BTreeMap::new();
        for (code, value) in item.props() {
            let row =
                properties.get(*code).ok_or(CatalogError::UnknownProperty { item: item.id(), property: *code })?;
            props.insert(row.slug().as_str().to_owned(), value.clone());
        }
        Ok(Self {
            id: item.id(),
            kind: kind.slug().as_str().to_owned(),
            name: item.name().to_owned(),
            slug: item.slug().as_str().to_owned(),
            archived: item.is_archived(),
            props,
            external: item.external().to_vec(),
        })
    }

    /// The record, with its kind and every property resolved through the registries.
    ///
    /// A slug names the kind and each property, so an unknown one is refused here with the word the
    /// file used; the item's own rules are checked by [`NomenclatureItem::check`] once the whole
    /// directory is built.
    pub fn to_item(
        &self,
        kinds: &crate::kind::KindRegistry,
        properties: &crate::property::PropertyRegistry,
    ) -> Result<NomenclatureItem, CatalogError> {
        let kind = kinds.by_slug(&self.kind).ok_or(CatalogError::UnknownKindSlug { slug: self.kind.clone() })?;
        let mut props = BTreeMap::new();
        for (slug, value) in &self.props {
            let row = properties.by_slug(slug).ok_or(CatalogError::UnknownPropertySlug { slug: slug.clone() })?;
            if kind.accepts().contains(&row.code()) {
                props.insert(row.code(), value.clone());
            } else {
                return Err(CatalogError::PropertyNotAccepted {
                    item: self.id,
                    kind: kind.code(),
                    property: row.code(),
                });
            }
        }
        let mut item =
            NomenclatureItem::new(self.id, kind.code(), self.name.clone(), Slug::new(self.slug.clone())?, props)?;
        if self.archived {
            item = item.archived();
        }
        for key in &self.external {
            item = item.with_external(key.clone())?;
        }
        Ok(item)
    }
}

impl ExternalKey {
    /// The row in a form a package can write.
    pub fn wire(&self) -> (String, String) {
        (self.system.as_str().to_owned(), self.key.clone())
    }

    /// The row from its written pair.
    pub fn from_wire(system: &str, key: &str) -> Result<Self, CatalogError> {
        Self::new(ExternalSystemId::new(system)?, key)
    }
}

/// The registries a package's rows build, before any other package is merged in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RegistryRows {
    /// The kinds the package declares.
    pub kinds: Vec<Kind>,
    /// The properties it declares.
    pub properties: Vec<Property>,
    /// The families it declares.
    pub families: Vec<Family>,
    /// The foreign systems its records' keys name.
    pub systems: Vec<crate::external::ExternalSystem>,
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;
    use crate::money::Money;
    use crate::property::Date;

    #[test]
    fn a_property_row_writes_its_range_in_micrometres() {
        let row = PropertyWire {
            code: 1,
            slug: "thickness".to_owned(),
            label: "Thickness".to_owned(),
            ty: PropertyTypeWire::Length { min_um: 1_000, max_um: 1_000_000 },
        };
        let json = serde_json::to_string(&row).expect("serialize");
        assert!(json.contains("\"type\":\"length\""), "{json}");
        assert!(json.contains("\"max_um\":1000000"), "{json}");
        let back: PropertyWire = serde_json::from_str(&json).expect("read back");
        assert_eq!(back, row);
    }

    #[test]
    fn a_registry_carries_the_foreign_systems_its_records_name() {
        let bare = r#"{"version":1,"kinds":[],"properties":[],"families":[]}"#;
        let rows = serde_json::from_str::<RegistryWire>(bare).expect("a registry with no systems still reads").rows();
        assert!(rows.expect("rows").systems.is_empty(), "the field is optional, so an older file loads");

        let declared = r#"{"version":1,"kinds":[],"properties":[],"families":[],
            "systems":[{"id":"erp","label":"ERP"}]}"#;
        let rows = serde_json::from_str::<RegistryWire>(declared).expect("a registry").rows().expect("rows");
        let registry = crate::external::ExternalSystemRegistry::new(rows.systems).expect("one system");
        let id = crate::item::ExternalSystemId::new("erp").expect("a system");
        assert_eq!(registry.get(&id).map(|row| row.label()), Some("ERP"));
    }

    #[test]
    fn an_unknown_key_is_refused_rather_than_ignored() {
        let manifest = r#"{"version": 1, "id": 1, "name": "Starter", "colour": "red"}"#;
        assert!(serde_json::from_str::<ManifestWire>(manifest).is_err(), "a mistyped key is a mistake, not data");
    }

    #[test]
    fn a_record_names_its_kind_and_properties_by_slug() {
        let properties = crate::property::PropertyRegistry::new([Property::new(
            PropertyCode::new(1).expect("a code"),
            Slug::new("thickness").expect("a slug"),
            "Thickness",
            PropertyType::Length {
                min: units::Length::from_micrometres(1_000),
                max: units::Length::from_micrometres(50_000),
            },
        )
        .expect("a row")])
        .expect("one row");
        let kinds = crate::kind::KindRegistry::new([Kind::new(
            KindCode::new(1).expect("a code"),
            Slug::new("sheet-material").expect("a slug"),
            "Sheet material",
            [PropertyCode::new(1).expect("a code")],
            [PropertyCode::new(1).expect("a code")],
            [Trait::Sheet],
        )
        .expect("a kind")])
        .expect("one kind");
        let wire = ItemWire {
            id: ItemId::new(712, 34).expect("an id"),
            kind: "sheet-material".to_owned(),
            name: "White melamine".to_owned(),
            slug: "white".to_owned(),
            archived: false,
            props: BTreeMap::from([(
                "thickness".to_owned(),
                PropertyValue::Length(units::Length::from_micrometres(18_000)),
            )]),
            external: Vec::new(),
        };
        let item = wire.to_item(&kinds, &properties).expect("a record");
        assert_eq!(item.kind(), KindCode::new(1).expect("a code"));
        assert!(item.prop(PropertyCode::new(1).expect("a code")).is_some(), "the slug became its code");
    }

    #[test]
    fn a_price_travels_through_a_file_with_its_currency_and_its_source() {
        let properties = crate::property::PropertyRegistry::new([Property::new(
            PropertyCode::new(1).expect("a code"),
            Slug::new("price").expect("a slug"),
            "Price",
            PropertyType::Money,
        )
        .expect("a row")])
        .expect("one row");
        let kinds = crate::kind::KindRegistry::new([Kind::new(
            KindCode::new(1).expect("a code"),
            Slug::new("sheet-material").expect("a slug"),
            "Sheet material",
            [PropertyCode::new(1).expect("a code")],
            [],
            [Trait::PriceSheet],
        )
        .expect("a kind")])
        .expect("one kind");
        let currency = crate::money::Currency::new("RUB", 100).expect("a currency");
        let system = crate::item::ExternalSystemId::new("supplier-egger").expect("a system");
        let date = Date::new(2026, 11, 20).expect("a date");
        let price = crate::price::Price::imported(Money::from_minor(320_000), currency, system.clone(), date);
        let wire = ItemWire {
            id: ItemId::new(712, 34).expect("an id"),
            kind: "sheet-material".to_owned(),
            name: "Oak veneered chipboard".to_owned(),
            slug: "oak".to_owned(),
            archived: false,
            props: BTreeMap::from([("price".to_owned(), PropertyValue::Money(price.clone()))]),
            external: Vec::new(),
        };
        let json = serde_json::to_string(&wire).expect("serialize");
        assert!(json.contains("\"minor\":320000") && json.contains("supplier-egger"), "{json}");
        let back: ItemWire = serde_json::from_str(&json).expect("read back");
        let item = back.to_item(&kinds, &properties).expect("a record");
        let read = item.prop(PropertyCode::new(1).expect("a code")).expect("the price");
        assert_eq!(read, &PropertyValue::Money(price));
    }

    #[test]
    fn a_file_price_with_a_currency_we_cannot_print_is_refused_with_the_iso_sentence() {
        let record = r#"{"id":"712-34","kind":"sheet-material","name":"Oak","slug":"oak",
            "props":{"price":{"type":"money","value":{"minor":100,
            "currency":{"code":"eur","minor_scale":100},"source":"typed"}}}}"#;
        let refusal = serde_json::from_str::<ItemWire>(record).expect_err("a lower-case code is not a currency");
        assert!(refusal.to_string().contains("ISO 4217"), "{refusal}");
    }

    #[test]
    fn a_misspelled_property_is_refused_with_the_word_the_file_used() {
        let properties = crate::property::PropertyRegistry::default();
        let kinds = crate::kind::KindRegistry::default();
        let wire = ItemWire {
            id: ItemId::new(712, 34).expect("an id"),
            kind: "sheet-material".to_owned(),
            name: "White melamine".to_owned(),
            slug: "white".to_owned(),
            archived: false,
            props: BTreeMap::from([("colour".to_owned(), PropertyValue::Text("white".to_owned()))]),
            external: Vec::new(),
        };
        let refusal = wire.to_item(&kinds, &properties).expect_err("no such kind");
        assert!(refusal.to_string().contains("sheet-material"), "{refusal}");
    }
}
