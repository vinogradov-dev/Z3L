//! The order package: a one-way, versioned snapshot other programs read for reports, drawings,
//! cutting lists and CNC (ADR 0095).
//!
//! The document is not self-contained (a panel stores a material id while names and thicknesses live
//! in the catalogue) and most of what a report needs is derivation (measured size with banding, area,
//! banding length, the owning product). This crate resolves those numbers once, writes them as flat
//! JSON Lines tables with an integer-only number contract, and embeds the catalogue and class rows
//! the order uses. The scene file stays the only import format: FurniCraft never reads a package back.
//!
//! The crate is a library with no window, no GPU and no file dialog, so a command line can write a
//! package without opening the editor (K3) and the assistant can later use the same surface.
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod error;
mod manifest;
mod rows;
mod table;
mod write;

pub use error::ExportError;
pub use manifest::{DocumentMeta, FORMAT, Manifest, OrderMeta, ROTATION_SCALE, UNITS, Units, VERSION};
pub use table::{
    BANDING_COLUMNS, CLASS_COLUMNS, CONTOUR_COLUMNS, Cell, Column, ColumnType, FACADE_COLUMNS, FITTING_COLUMNS,
    MACHINING_COLUMNS, MATERIAL_COLUMNS, NODE_COLUMNS, PART_COLUMNS, PRODUCT_COLUMNS, PROPERTY_COLUMNS, TABLES,
    TableData, TableSpec, Tables, table,
};
pub use write::{ExportOptions, OrderPackage, csv_name, now_timestamp, record_json, table_csv, timestamp_from_unix};

#[cfg(test)]
mod fixtures;
#[cfg(test)]
mod tests;
