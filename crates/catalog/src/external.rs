//! The systems an item is known in, as registry rows a package carries.
//!
//! An item's foreign keys name a system (`1c`, `erp`, `supplier-egger`) and an importer matches its
//! columns through them (`docs/domain/nomenclature.md` §5.3). The system is a registry row and not a
//! free string, so `1C`, `1с` and `1C:ERP` cannot become three worlds that name one supplier; a key
//! whose system no connected package declares is a load refusal, and nothing is ever matched by name.

use std::collections::BTreeMap;

use crate::error::CatalogError;
use crate::item::ExternalSystemId;

/// One foreign system: the id a key is written with, and the name a person reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalSystem {
    id: ExternalSystemId,
    label: String,
}

impl ExternalSystem {
    /// Builds a row, refusing an empty label: a system a person cannot read is not a row.
    pub fn new(id: ExternalSystemId, label: impl Into<String>) -> Result<Self, CatalogError> {
        let label = label.into();
        if label.trim().is_empty() {
            return Err(CatalogError::Empty { row: "a system label" });
        }
        Ok(Self { id, label })
    }

    /// The id a key is written with.
    pub const fn id(&self) -> &ExternalSystemId {
        &self.id
    }

    /// The name a person reads.
    pub fn label(&self) -> &str {
        &self.label
    }
}

/// Every foreign system the connected packages declare.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExternalSystemRegistry {
    rows: BTreeMap<ExternalSystemId, ExternalSystem>,
}

impl ExternalSystemRegistry {
    /// Builds the registry, refusing an id two rows claim.
    pub fn new(rows: impl IntoIterator<Item = ExternalSystem>) -> Result<Self, CatalogError> {
        let mut registry = Self::default();
        for row in rows {
            if registry.rows.contains_key(row.id()) {
                return Err(CatalogError::DuplicateSystem { system: row.id().as_str().to_owned() });
            }
            registry.rows.insert(row.id().clone(), row);
        }
        Ok(registry)
    }

    /// The row with `id`, when a package declares it.
    pub fn get(&self, id: &ExternalSystemId) -> Option<&ExternalSystem> {
        self.rows.get(id)
    }

    /// Whether a package declares `id`.
    pub fn has(&self, id: &ExternalSystemId) -> bool {
        self.rows.contains_key(id)
    }

    /// Every row, in id order.
    pub fn iter(&self) -> impl Iterator<Item = &ExternalSystem> {
        self.rows.values()
    }

    /// How many systems the directory declares.
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Whether no package declares a system.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    fn id(text: &str) -> ExternalSystemId {
        ExternalSystemId::new(text).expect("a system id")
    }

    #[test]
    fn a_system_is_named_once_and_a_key_reads_it_by_its_id() {
        let registry = ExternalSystemRegistry::new([
            ExternalSystem::new(id("1c"), "1C:ERP").expect("a row"),
            ExternalSystem::new(id("supplier-egger"), "Egger list").expect("a row"),
        ])
        .expect("two rows");
        assert_eq!(registry.len(), 2);
        assert_eq!(registry.get(&id("1c")).map(ExternalSystem::label), Some("1C:ERP"));
        assert!(!registry.has(&id("erp")), "a system nobody declared is unknown");

        let twice = ExternalSystemRegistry::new([
            ExternalSystem::new(id("1c"), "1C").expect("a row"),
            ExternalSystem::new(id("1c"), "One C").expect("a row"),
        ])
        .expect_err("one id names one system");
        assert!(twice.to_string().contains("`1c` is declared twice"), "{twice}");

        assert!(ExternalSystem::new(id("erp"), "  ").is_err(), "a row a person cannot read is refused");
    }
}
