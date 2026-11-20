//! Building one package and writing its two forms: a directory of tables and a single JSON file.
//!
//! The directory form is the contract: `manifest.json` plus one JSON Lines file per table, and a CSV
//! copy beside each for spreadsheets. The single form embeds the same manifest and the same records
//! in one JSON object, for sending by mail. Both are written from the same in-memory tables, so they
//! cannot disagree.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::io::Write as _;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use catalog::Directory;
use model::Project;

use crate::error::ExportError;
use crate::manifest::{DocumentMeta, Manifest, MoneyUnit, OrderMeta};
use crate::rows;
use crate::table::{Cell, TableData, Tables};

/// What the caller tells the exporter about the export itself.
#[derive(Clone, Debug, Default)]
pub struct ExportOptions {
    /// The export time in RFC 3339, or `None` for a golden package.
    pub timestamp: Option<String>,
    /// The scene the package came from.
    pub document: DocumentMeta,
    /// The order, when the caller knows one.
    pub order: Option<OrderMeta>,
}

/// One built order package: its manifest and its records, ready to write.
#[derive(Clone, Debug)]
pub struct OrderPackage {
    manifest: Manifest,
    tables: Tables,
}

impl OrderPackage {
    /// Reads the document and the catalogue once and builds every table.
    ///
    /// Nothing is written here: a package that fails to build leaves the filesystem untouched. The
    /// records are ordered by their key, so two builds of one revision are identical.
    pub fn build(project: &Project, catalog: &Directory, options: ExportOptions) -> Result<Self, ExportError> {
        let tables = rows::build(project, catalog)?;
        let counts: BTreeMap<&'static str, usize> =
            tables.iter().map(|table| (table.spec().name, table.rows().len())).collect();
        let money = catalog
            .currency()
            .map(|currency| MoneyUnit { currency: currency.code().to_owned(), minor_scale: currency.minor_scale() });
        let manifest = Manifest::build(options.timestamp, options.document, options.order, money, counts);
        Ok(Self { manifest, tables })
    }

    /// The manifest this package declares.
    pub const fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// The table named `name`, or `None` when the package has no such table.
    pub fn table(&self, name: &str) -> Option<&TableData> {
        self.tables.table(name)
    }

    /// Every table, in manifest order.
    pub fn tables(&self) -> impl Iterator<Item = &TableData> {
        self.tables.iter()
    }

    /// Writes the directory form: the manifest, one JSON Lines file and one CSV copy per table.
    ///
    /// The directory is created when it does not exist. Files are written whole; a failure in the
    /// middle leaves the earlier files on disk, so a caller that needs all-or-nothing writes to a
    /// scratch directory and renames it afterwards.
    pub fn write_dir(&self, directory: &Path) -> Result<(), ExportError> {
        fs::create_dir_all(directory)?;
        let manifest = serde_json::to_string_pretty(&self.manifest)?;
        write_file(&directory.join("manifest.json"), manifest.as_bytes())?;
        for table in self.tables.iter() {
            let mut jsonl = String::new();
            for row in table.rows() {
                jsonl.push_str(&record_json(table, row)?);
                jsonl.push('\n');
            }
            write_file(&directory.join(table.spec().file), jsonl.as_bytes())?;
            let csv = table_csv(table)?;
            write_file(&directory.join(csv_name(table.spec().name)), csv.as_bytes())?;
        }
        Ok(())
    }

    /// Writes the single-file form: the same manifest and the same records in one JSON object.
    pub fn write_single(&self, path: &Path) -> Result<(), ExportError> {
        let mut text = String::from("{\"manifest\":");
        text.push_str(&serde_json::to_string(&self.manifest)?);
        text.push_str(",\"tables\":{");
        for (index, table) in self.tables.iter().enumerate() {
            if index > 0 {
                text.push(',');
            }
            text.push('"');
            text.push_str(table.spec().name);
            text.push_str("\":[");
            for (row_index, row) in table.rows().iter().enumerate() {
                if row_index > 0 {
                    text.push(',');
                }
                text.push_str(&record_json(table, row)?);
            }
            text.push(']');
        }
        text.push_str("}}\n");
        write_file(path, text.as_bytes())
    }
}

/// The CSV file name of one table.
pub fn csv_name(table: &str) -> String {
    format!("{table}.csv")
}

/// The compact JSON object of one record, with its columns in declaration order.
pub fn record_json(table: &TableData, row: &[Cell]) -> Result<String, ExportError> {
    let mut text = String::from("{");
    for (index, (column, cell)) in table.spec().columns.iter().zip(row).enumerate() {
        if index > 0 {
            text.push(',');
        }
        text.push('"');
        text.push_str(column.name);
        text.push_str("\":");
        text.push_str(&cell_json(cell)?);
    }
    text.push('}');
    Ok(text)
}

/// The JSON text of one cell.
fn cell_json(cell: &Cell) -> Result<String, ExportError> {
    Ok(serde_json::to_string(&cell.json())?)
}

/// The CSV copy of one table, with one header line and one line per record.
pub fn table_csv(table: &TableData) -> Result<String, ExportError> {
    let mut text = String::new();
    for (index, column) in table.spec().columns.iter().enumerate() {
        if index > 0 {
            text.push(',');
        }
        text.push_str(&csv_text(column.name));
    }
    text.push('\n');
    for row in table.rows() {
        for (index, cell) in row.iter().enumerate() {
            if index > 0 {
                text.push(',');
            }
            text.push_str(&csv_cell(cell)?);
        }
        text.push('\n');
    }
    Ok(text)
}

/// One cell as a CSV field.
fn csv_cell(cell: &Cell) -> Result<String, ExportError> {
    Ok(match cell {
        Cell::Integer(value) => value.to_string(),
        Cell::Boolean(value) => value.to_string(),
        Cell::Text(value) => csv_text(value),
        Cell::Json(value) => csv_text(&serde_json::to_string(value)?),
    })
}

/// Quotes a CSV field that needs it and doubles its quotes.
///
/// A field without a comma, a quote or a line break is written bare, which keeps a spreadsheet's
/// first rows readable; everything else is quoted, so no dialect question remains.
fn csv_text(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

/// Writes one file, replacing it when it exists.
fn write_file(path: &Path, bytes: &[u8]) -> Result<(), ExportError> {
    let mut file = fs::File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

/// The current time as an RFC 3339 UTC timestamp.
pub fn now_timestamp() -> Result<String, ExportError> {
    let seconds = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_| ExportError::Clock)?.as_secs();
    timestamp_from_unix(seconds)
}

/// Formats a Unix time as an RFC 3339 UTC timestamp without pulling in a date library.
///
/// The civil-date arithmetic is the standard era-based conversion: days since 1970 are split into
/// 400-year eras, then into years, months and days. A failure is only the counter not fitting the
/// calendar, which is a [`ExportError::Clock`].
pub fn timestamp_from_unix(seconds: u64) -> Result<String, ExportError> {
    let seconds = i64::try_from(seconds).map_err(|_| ExportError::Clock)?;
    let days = seconds.div_euclid(86_400);
    let rest = seconds.rem_euclid(86_400);
    let hour = rest / 3_600;
    let minute = (rest % 3_600) / 60;
    let second = rest % 60;
    let (year, month, day) = civil_from_days(days)?;
    let mut text = String::new();
    write!(&mut text, "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
        .map_err(|_| ExportError::Clock)?;
    Ok(text)
}

/// The civil date of a day count from 1970-01-01.
fn civil_from_days(days: i64) -> Result<(i64, i64, i64), ExportError> {
    let z = days.checked_add(719_468).ok_or(ExportError::Clock)?;
    let era = if z >= 0 { z } else { z.checked_sub(146_096).ok_or(ExportError::Clock)? } / 146_097;
    let day_of_era = z.checked_sub(era.checked_mul(146_097).ok_or(ExportError::Clock)?).ok_or(ExportError::Clock)?;
    let year_of_era = (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era.checked_add(era.checked_mul(400).ok_or(ExportError::Clock)?).ok_or(ExportError::Clock)?;
    let day_of_year = day_of_era
        .checked_sub(
            year_of_era
                .checked_mul(365)
                .and_then(|value| value.checked_add(year_of_era / 4))
                .and_then(|value| value.checked_sub(year_of_era / 100))
                .ok_or(ExportError::Clock)?,
        )
        .ok_or(ExportError::Clock)?;
    let month_part = day_of_year.checked_mul(5).and_then(|value| value.checked_add(2)).ok_or(ExportError::Clock)? / 153;
    let day = day_of_year
        .checked_sub(month_part.checked_mul(153).and_then(|value| value.checked_add(2)).ok_or(ExportError::Clock)? / 5)
        .and_then(|value| value.checked_add(1))
        .ok_or(ExportError::Clock)?;
    let month = if month_part < 10 { month_part + 3 } else { month_part - 9 };
    let year = if month <= 2 { year.checked_add(1).ok_or(ExportError::Clock)? } else { year };
    Ok((year, month, day))
}
