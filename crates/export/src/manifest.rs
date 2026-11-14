//! The package manifest: format identity, units, counts and the declaration of every table.
//!
//! The manifest is generated from [`crate::table::TABLES`], never written by hand, so a reader that
//! parses it knows the columns and units without a second document. It is the entry point of both
//! package forms.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::table::{Column, TABLES};

/// The format identity written into every manifest.
pub const FORMAT: &str = "furnicraft.order";

/// The major version of the published contract (ADR 0095).
pub const VERSION: u32 = 1;

/// The scale of the integer quaternion components: 1e9.
pub const ROTATION_SCALE: i64 = 1_000_000_000;

/// The unit names carried by the manifest.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Units {
    /// Lengths are integer micrometres.
    pub length: &'static str,
    /// Angles are integer millidegrees.
    pub angle: &'static str,
    /// Areas are integer square millimetres.
    pub area: &'static str,
    /// The integer quaternion scale.
    pub rotation_scale: i64,
}

/// The unit block every manifest carries.
pub const UNITS: Units = Units { length: "um", angle: "mdeg", area: "mm2", rotation_scale: ROTATION_SCALE };

/// What the package says about the scene it came from.
#[derive(Clone, Debug, Default, Serialize)]
pub struct DocumentMeta {
    /// The scene file name, when the caller knows it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// The document revision at export time.
    pub revision: u64,
    /// A hash of the scene file, when the caller computed one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
}

/// The order a package belongs to.
#[derive(Clone, Debug, Default, Serialize)]
pub struct OrderMeta {
    /// The workshop's order number.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number: Option<String>,
    /// The order revision the package was taken from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<u64>,
}

impl OrderMeta {
    /// Whether the order block carries nothing and is left out of the manifest.
    pub const fn is_empty(&self) -> bool {
        self.number.is_none() && self.version.is_none()
    }
}

/// One table declaration in the manifest.
#[derive(Clone, Debug, Serialize)]
pub struct TableEntry {
    /// The table name.
    pub name: &'static str,
    /// The JSON Lines file.
    pub file: &'static str,
    /// The key the records are ordered by.
    pub key: &'static str,
    /// The columns in record order.
    pub columns: &'static [Column],
}

/// The package manifest.
#[derive(Clone, Debug, Serialize)]
pub struct Manifest {
    /// The format identity, [`FORMAT`].
    pub format: &'static str,
    /// The major contract version, [`VERSION`].
    pub version: u32,
    /// The program version that wrote the package.
    pub app: &'static str,
    /// The export time in RFC 3339, absent for a golden package.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exported_at: Option<String>,
    /// The scene the package came from.
    pub document: DocumentMeta,
    /// The order, when the caller supplied one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order: Option<OrderMeta>,
    /// The units every integer value uses.
    pub units: Units,
    /// How many records each table holds.
    pub counts: BTreeMap<&'static str, usize>,
    /// Every table, in the order the directory and the single file use.
    pub tables: Vec<TableEntry>,
}

impl Manifest {
    /// Builds the manifest for one export.
    pub fn build(
        exported_at: Option<String>,
        document: DocumentMeta,
        order: Option<OrderMeta>,
        counts: BTreeMap<&'static str, usize>,
    ) -> Self {
        Self {
            format: FORMAT,
            version: VERSION,
            app: env!("CARGO_PKG_VERSION"),
            exported_at,
            document,
            order: order.filter(|order| !order.is_empty()),
            units: UNITS,
            counts,
            tables: TABLES
                .iter()
                .map(|table| TableEntry { name: table.name, file: table.file, key: table.key, columns: table.columns })
                .collect(),
        }
    }
}
