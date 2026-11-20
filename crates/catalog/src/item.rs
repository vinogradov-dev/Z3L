//! The item envelope: one record of one package, and nothing a consumer may special-case.

use std::collections::BTreeSet;
use std::fmt;

use model::ItemId;
use serde::{Deserialize, Serialize};

use crate::directory::Directory;
use crate::error::CatalogError;
use crate::kind::KindCode;
use crate::property::{PropertyCode, PropertySet};
use crate::slug::Slug;

/// The longest item name a package may carry.
pub const MAX_ITEM_NAME_CHARS: usize = 120;

/// The most external keys one item may name.
pub const MAX_EXTERNAL_KEYS: usize = 8;

/// A foreign system an item is known in: `1c`, `erp`, `supplier-egger`.
///
/// A registry id rather than a free string, so `1C` and `1с` cannot become two worlds (`§5.3`). It
/// is a slug because that is exactly what it must be: an ASCII name nobody localises.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ExternalSystemId(Slug);

impl ExternalSystemId {
    /// Validates a system id.
    pub fn new(text: impl Into<String>) -> Result<Self, CatalogError> {
        Slug::new(text).map(Self)
    }

    /// The id as written.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl TryFrom<String> for ExternalSystemId {
    type Error = CatalogError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        Self::new(text)
    }
}

impl From<ExternalSystemId> for String {
    fn from(id: ExternalSystemId) -> Self {
        id.0.into()
    }
}

/// One row of an item's foreign keys: which system, and the key that system uses.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalKey {
    /// The system the key belongs to.
    pub system: ExternalSystemId,
    /// The key's value in that system.
    pub key: String,
}

impl ExternalKey {
    /// A row, refusing an empty key: nothing is matched by name, and nothing by an empty one.
    pub fn new(system: ExternalSystemId, key: impl Into<String>) -> Result<Self, CatalogError> {
        let key = key.into();
        if key.trim().is_empty() {
            return Err(CatalogError::Empty { row: "an external key" });
        }
        Ok(Self { system, key })
    }
}

/// One nomenclature record: an item of the directory.
///
/// The envelope is deliberately small (`§3`): an id, a kind, a name, a slug, an archived flag, typed
/// properties and the foreign keys an importer fills. Appearance, finish and the part document join
/// it with their readers, so no field exists here that nothing reads.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NomenclatureItem {
    id: ItemId,
    kind: KindCode,
    name: String,
    slug: Slug,
    #[serde(default)]
    archived: bool,
    #[serde(default)]
    props: PropertySet,
    #[serde(default)]
    external: Vec<ExternalKey>,
}

impl NomenclatureItem {
    /// Builds an item, refusing an empty name.
    pub fn new(
        id: ItemId,
        kind: KindCode,
        name: impl Into<String>,
        slug: Slug,
        props: PropertySet,
    ) -> Result<Self, CatalogError> {
        let name = name.into();
        let length = name.chars().count();
        if name.trim().is_empty() {
            return Err(CatalogError::Empty { row: "an item name" });
        }
        if length > MAX_ITEM_NAME_CHARS {
            return Err(CatalogError::TooLong { text: name, length, limit: MAX_ITEM_NAME_CHARS });
        }
        Ok(Self { id, kind, name, slug, archived: false, props, external: Vec::new() })
    }

    /// The same item marked out of production: it still resolves, and every card says so.
    pub fn archived(mut self) -> Self {
        self.archived = true;
        self
    }

    /// The same item with one more foreign key.
    pub fn with_external(mut self, key: ExternalKey) -> Result<Self, CatalogError> {
        if self.external.len() >= MAX_EXTERNAL_KEYS {
            return Err(CatalogError::TooLong {
                text: format!("{} external keys", self.external.len() + 1),
                length: self.external.len() + 1,
                limit: MAX_EXTERNAL_KEYS,
            });
        }
        self.external.push(key);
        Ok(self)
    }

    /// The record's id.
    pub const fn id(&self) -> ItemId {
        self.id
    }

    /// The kind the record is of.
    pub const fn kind(&self) -> KindCode {
        self.kind
    }

    /// The name a person reads.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The record's slug, unique within its kind.
    pub const fn slug(&self) -> &Slug {
        &self.slug
    }

    /// Whether the workshop retired the record; it still resolves.
    pub const fn is_archived(&self) -> bool {
        self.archived
    }

    /// Every property the record carries.
    pub const fn props(&self) -> &PropertySet {
        &self.props
    }

    /// One property's value, when the record carries it.
    pub fn prop(&self, code: PropertyCode) -> Option<&crate::property::PropertyValue> {
        self.props.get(&code)
    }

    /// The record's foreign keys.
    pub fn external(&self) -> &[ExternalKey] {
        &self.external
    }

    /// Checks the record against the registries it names, and nothing else.
    ///
    /// A package's reader hands every row through this: an unknown kind, an unknown property, a
    /// missing required property, a property the kind does not accept, a value outside its range and
    /// a repeated foreign system are each refused with the row named.
    pub fn check(&self, directory: &Directory) -> Result<(), CatalogError> {
        let kind =
            directory.kinds().get(self.kind).ok_or(CatalogError::UnknownKind { item: self.id, kind: self.kind })?;
        for code in self.props.keys() {
            let row = directory
                .properties()
                .get(*code)
                .ok_or(CatalogError::UnknownProperty { item: self.id, property: *code })?;
            if !kind.accepts().contains(code) {
                return Err(CatalogError::PropertyNotAccepted { item: self.id, kind: self.kind, property: *code });
            }
            let value = self.props.get(code).ok_or(CatalogError::UnknownProperty { item: self.id, property: *code })?;
            row.ty().check(value).map_err(|reason| CatalogError::Property {
                item: self.id,
                property: *code,
                reason,
            })?;
            if let crate::property::PropertyValue::Ref(target) = value
                && directory.item(*target).is_none()
            {
                return Err(CatalogError::Property {
                    item: self.id,
                    property: *code,
                    reason: format!("`{target}` is not an item of the connected packages"),
                });
            }
            // A price is read through the kind that carries it: the traits say what one price buys,
            // so a priced record whose kind names no unit cannot be answered and is refused here.
            if matches!(value, crate::property::PropertyValue::Money(_)) && kind.price_unit().is_none() {
                return Err(CatalogError::PriceWithoutUnit { item: self.id, kind: self.kind });
            }
        }
        for required in kind.required() {
            if !self.props.contains_key(required) {
                return Err(CatalogError::MissingProperty { item: self.id, kind: self.kind, property: *required });
            }
        }
        let mut systems: BTreeSet<&str> = BTreeSet::new();
        for key in &self.external {
            if !systems.insert(key.system.as_str()) {
                return Err(CatalogError::DuplicateExternalKey {
                    item: self.id,
                    system: key.system.as_str().to_owned(),
                });
            }
            // A system is a registry row, so a key written with a word nobody declared is refused
            // here rather than becoming a second world for one supplier (`§5.3`).
            if !directory.systems().has(&key.system) {
                return Err(CatalogError::UnknownSystem { item: self.id, system: key.system.as_str().to_owned() });
            }
        }
        Ok(())
    }
}

impl fmt::Display for NomenclatureItem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} {}", self.id, self.name)
    }
}
