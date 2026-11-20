//! The one column table: the manifest, the JSONL writer and the CSV copies are all derived from it.
//!
//! A table is a name, a file, a key and an ordered list of columns. Adding a column means editing
//! this file once: the manifest declaration, the record check, the JSON Lines writer and the
//! spreadsheet copy all read the same list, so they cannot drift apart. The order of the rows is the
//! table's sort order, fixed by the package builder, so one revision always produces the same bytes.

use serde::Serialize;

use crate::error::ExportError;

/// The type of a column value, as the manifest declares it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ColumnType {
    /// A signed 64-bit integer in the unit the column declares.
    Integer,
    /// UTF-8 text, never interpreted by the package.
    String,
    /// `true` or `false`.
    Boolean,
    /// A JSON object or array for operation-specific fields.
    Json,
}

impl ColumnType {
    /// The manifest spelling of the type.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Integer => "integer",
            Self::String => "string",
            Self::Boolean => "boolean",
            Self::Json => "json",
        }
    }
}

/// One declared column of one table.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Column {
    /// The key used in every record and in the CSV header.
    pub name: &'static str,
    /// The type of the value.
    #[serde(rename = "type")]
    pub kind: ColumnType,
    /// The unit of an integer value, absent when the value has none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<&'static str>,
}

impl Column {
    /// An integer column with no unit.
    pub const fn integer(name: &'static str) -> Self {
        Self { name, kind: ColumnType::Integer, unit: None }
    }

    /// A text column.
    pub const fn text(name: &'static str) -> Self {
        Self { name, kind: ColumnType::String, unit: None }
    }

    /// A boolean column.
    pub const fn boolean(name: &'static str) -> Self {
        Self { name, kind: ColumnType::Boolean, unit: None }
    }

    /// A JSON column.
    pub const fn json(name: &'static str) -> Self {
        Self { name, kind: ColumnType::Json, unit: None }
    }

    /// The same column with a declared unit.
    pub const fn in_unit(self, unit: &'static str) -> Self {
        Self { unit: Some(unit), ..self }
    }
}

/// One table of the package.
#[derive(Clone, Copy, Debug)]
pub struct TableSpec {
    /// The table name, which is also the manifest key and the source of the file name.
    pub name: &'static str,
    /// The file the JSON Lines records are written to.
    pub file: &'static str,
    /// The key the table is ordered by; a compound key lists its columns separated by `+`.
    pub key: &'static str,
    /// The columns, in the order a record and a CSV copy carry them.
    pub columns: &'static [Column],
}

/// The columns of `products`.
pub const PRODUCT_COLUMNS: &[Column] = &[
    Column::integer("node_id"),
    Column::integer("parent_id"),
    Column::text("name"),
    Column::text("class"),
    Column::text("level"),
    Column::integer("width_um").in_unit("um"),
    Column::integer("height_um").in_unit("um"),
    Column::integer("depth_um").in_unit("um"),
    Column::integer("content_width_um").in_unit("um"),
    Column::integer("content_height_um").in_unit("um"),
    Column::integer("content_depth_um").in_unit("um"),
    Column::integer("part_count"),
    Column::integer("sheet_area_mm2").in_unit("mm2"),
    Column::integer("banding_um").in_unit("um"),
    Column::integer("fitting_count"),
    Column::integer("price_minor"),
];

/// The columns of `nodes`.
pub const NODE_COLUMNS: &[Column] = &[
    Column::integer("node_id"),
    Column::integer("parent_id"),
    Column::text("name"),
    Column::text("class"),
    Column::text("level"),
    Column::text("path"),
    Column::integer("x_um").in_unit("um"),
    Column::integer("y_um").in_unit("um"),
    Column::integer("z_um").in_unit("um"),
    Column::integer("qw"),
    Column::integer("qx"),
    Column::integer("qy"),
    Column::integer("qz"),
];

/// The columns of `parts`.
pub const PART_COLUMNS: &[Column] = &[
    Column::integer("node_id"),
    Column::integer("product_id"),
    Column::text("class"),
    Column::text("material_id"),
    Column::integer("thickness_um").in_unit("um"),
    Column::integer("length_um").in_unit("um"),
    Column::integer("width_um").in_unit("um"),
    Column::integer("measured_length_um").in_unit("um"),
    Column::integer("measured_width_um").in_unit("um"),
    Column::integer("area_mm2").in_unit("mm2"),
    Column::text("grain"),
    Column::text("nesting"),
    Column::integer("banding_total_um").in_unit("um"),
    Column::integer("price_minor"),
];

/// The columns of `banding`.
pub const BANDING_COLUMNS: &[Column] = &[
    Column::integer("node_id"),
    Column::text("unit"),
    Column::text("material_id"),
    Column::integer("thickness_um").in_unit("um"),
    Column::integer("width_um").in_unit("um"),
    Column::boolean("included_in_size"),
    Column::integer("length_um").in_unit("um"),
];

/// The columns of `contours`.
pub const CONTOUR_COLUMNS: &[Column] = &[
    Column::integer("node_id"),
    Column::text("ring"),
    Column::integer("seq"),
    Column::text("kind"),
    Column::integer("x_um").in_unit("um"),
    Column::integer("y_um").in_unit("um"),
    Column::integer("x2_um").in_unit("um"),
    Column::integer("y2_um").in_unit("um"),
    Column::integer("cx_um").in_unit("um"),
    Column::integer("cy_um").in_unit("um"),
    Column::boolean("clockwise"),
];

/// The columns of `facades`.
pub const FACADE_COLUMNS: &[Column] = &[
    Column::integer("node_id"),
    Column::integer("product_id"),
    Column::text("class"),
    Column::text("material_id"),
    Column::integer("length_um").in_unit("um"),
    Column::integer("width_um").in_unit("um"),
    Column::integer("thickness_um").in_unit("um"),
    Column::text("supply"),
    Column::boolean("glass"),
];

/// The columns of `fittings`.
pub const FITTING_COLUMNS: &[Column] = &[
    Column::integer("node_id"),
    Column::integer("host_id"),
    Column::integer("product_id"),
    Column::text("class"),
    Column::text("article"),
    Column::integer("qty"),
];

/// The columns of `machining`.
pub const MACHINING_COLUMNS: &[Column] = &[
    Column::integer("machining_id"),
    Column::integer("node_id"),
    Column::text("op"),
    Column::text("face"),
    Column::integer("x_um").in_unit("um"),
    Column::integer("y_um").in_unit("um"),
    Column::integer("diameter_um").in_unit("um"),
    Column::integer("depth_um").in_unit("um"),
    Column::json("params"),
];

/// The columns of `materials`.
pub const MATERIAL_COLUMNS: &[Column] = &[
    Column::text("material_id"),
    Column::text("name"),
    Column::text("kind"),
    Column::integer("thickness_um").in_unit("um"),
    Column::integer("width_um").in_unit("um"),
    Column::text("color"),
    Column::integer("sheet_length_um").in_unit("um"),
    Column::integer("sheet_width_um").in_unit("um"),
    Column::text("stock_group"),
    Column::integer("price_minor"),
];

/// The columns of `sheets`.
pub const SHEET_COLUMNS: &[Column] = &[
    Column::text("material_id"),
    Column::text("stock_group"),
    Column::integer("sheet_length_um").in_unit("um"),
    Column::integer("sheet_width_um").in_unit("um"),
    Column::integer("usable_length_um").in_unit("um"),
    Column::integer("usable_width_um").in_unit("um"),
    Column::integer("price_minor"),
    Column::integer("part_count"),
    Column::integer("part_area_mm2").in_unit("mm2"),
    Column::integer("sheet_area_mm2").in_unit("mm2"),
];

/// The columns of `classes`.
pub const CLASS_COLUMNS: &[Column] = &[
    Column::text("code"),
    Column::text("parent"),
    Column::text("slug"),
    Column::text("label"),
    Column::text("supply"),
    Column::text("measure"),
    Column::text("nesting"),
    Column::text("extent"),
    Column::text("level"),
    Column::text("opening"),
    Column::text("mount"),
    Column::text("fill"),
    Column::json("operations"),
    Column::boolean("retired"),
];

/// The columns of `properties`.
pub const PROPERTY_COLUMNS: &[Column] =
    &[Column::integer("node_id"), Column::text("key"), Column::text("kind"), Column::json("value")];

/// Every table, in the order the manifest, the directory and the single file use.
pub const TABLES: &[TableSpec] = &[
    TableSpec { name: "products", file: "products.jsonl", key: "node_id", columns: PRODUCT_COLUMNS },
    TableSpec { name: "nodes", file: "nodes.jsonl", key: "node_id", columns: NODE_COLUMNS },
    TableSpec { name: "parts", file: "parts.jsonl", key: "node_id", columns: PART_COLUMNS },
    TableSpec { name: "banding", file: "banding.jsonl", key: "node_id+unit", columns: BANDING_COLUMNS },
    TableSpec { name: "contours", file: "contours.jsonl", key: "node_id+ring+seq", columns: CONTOUR_COLUMNS },
    TableSpec { name: "facades", file: "facades.jsonl", key: "node_id", columns: FACADE_COLUMNS },
    TableSpec { name: "fittings", file: "fittings.jsonl", key: "node_id", columns: FITTING_COLUMNS },
    TableSpec { name: "machining", file: "machining.jsonl", key: "machining_id", columns: MACHINING_COLUMNS },
    TableSpec { name: "materials", file: "materials.jsonl", key: "material_id", columns: MATERIAL_COLUMNS },
    TableSpec { name: "sheets", file: "sheets.jsonl", key: "material_id", columns: SHEET_COLUMNS },
    TableSpec { name: "classes", file: "classes.jsonl", key: "code", columns: CLASS_COLUMNS },
    TableSpec { name: "properties", file: "properties.jsonl", key: "node_id+key", columns: PROPERTY_COLUMNS },
];

/// Looks a table up by its name.
pub fn table(name: &str) -> Option<&'static TableSpec> {
    TABLES.iter().find(|table| table.name == name)
}

/// One value of one record.
#[derive(Clone, Debug, PartialEq)]
pub enum Cell {
    /// An integer in the unit its column declares.
    Integer(i64),
    /// UTF-8 text.
    Text(String),
    /// A flag.
    Boolean(bool),
    /// A JSON object or array.
    Json(serde_json::Value),
}

impl Cell {
    /// The type this value carries.
    pub const fn kind(&self) -> ColumnType {
        match self {
            Self::Integer(_) => ColumnType::Integer,
            Self::Text(_) => ColumnType::String,
            Self::Boolean(_) => ColumnType::Boolean,
            Self::Json(_) => ColumnType::Json,
        }
    }

    /// The type name a mismatch reports.
    pub const fn type_name(&self) -> &'static str {
        self.kind().as_str()
    }

    /// The value a JSON reader sees: an integer, a string, a flag or the stored JSON.
    ///
    /// The JSON Lines writer, the single-file form and the command line's `query` all read a cell
    /// through this one function, so a published record and a printed report cannot disagree about
    /// what a cell is.
    pub fn json(&self) -> serde_json::Value {
        match self {
            Self::Integer(value) => serde_json::Value::from(*value),
            Self::Text(value) => serde_json::Value::from(value.clone()),
            Self::Boolean(value) => serde_json::Value::from(*value),
            Self::Json(value) => value.clone(),
        }
    }
}

/// One table with its rows, in the order the writer emits them.
#[derive(Clone, Debug)]
pub struct TableData {
    spec: &'static TableSpec,
    rows: Vec<Vec<Cell>>,
}

impl TableData {
    /// An empty table for `spec`.
    pub const fn new(spec: &'static TableSpec) -> Self {
        Self { spec, rows: Vec::new() }
    }

    /// The table's declared shape.
    pub const fn spec(&self) -> &'static TableSpec {
        self.spec
    }

    /// The rows in emission order.
    pub fn rows(&self) -> &[Vec<Cell>] {
        &self.rows
    }

    /// Appends one row after checking it against the column table.
    ///
    /// The check is the invariant the manifest and the CSV copies rely on: a row that does not carry
    /// one correctly typed value per column is refused here, before any file is written.
    pub fn push(&mut self, row: Vec<Cell>) -> Result<(), ExportError> {
        if row.len() != self.spec.columns.len() {
            return Err(ExportError::ColumnCount {
                table: self.spec.name,
                got: row.len(),
                expected: self.spec.columns.len(),
            });
        }
        for (cell, column) in row.iter().zip(self.spec.columns) {
            if cell.kind() != column.kind {
                return Err(ExportError::ColumnType {
                    table: self.spec.name,
                    column: column.name,
                    expected: column.kind.as_str(),
                    got: cell.type_name(),
                });
            }
        }
        self.rows.push(row);
        Ok(())
    }
}

/// All tables of one package, in [`TABLES`] order.
#[derive(Clone, Debug)]
pub struct Tables {
    tables: Vec<TableData>,
}

impl Tables {
    /// One empty table per declaration, in the same order.
    pub fn new() -> Self {
        Self { tables: TABLES.iter().map(TableData::new).collect() }
    }

    /// The empty table named `name`, or `None` when no such table is declared.
    pub fn table(&self, name: &str) -> Option<&TableData> {
        self.tables.iter().find(|table| table.spec.name == name)
    }

    /// The empty table named `name` for a row builder.
    pub fn table_mut(&mut self, name: &str) -> Option<&mut TableData> {
        self.tables.iter_mut().find(|table| table.spec.name == name)
    }

    /// Every table, in declaration order.
    pub fn iter(&self) -> impl Iterator<Item = &TableData> {
        self.tables.iter()
    }
}

impl Default for Tables {
    fn default() -> Self {
        Self::new()
    }
}
