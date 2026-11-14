//! The exporter's own gate: the column table, the two writers, the number boundary and the golden.

#![allow(clippy::expect_used)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use furnicraft_model::{Catalog, EdgeBandingMaterial, PanelMaterial, Project};
use furnicraft_testkit::{
    millimetres, sample_band_thickness, sample_band_width, sample_edge_material_id, sample_material_id,
};
use serde_json::Value;

use crate::fixtures::fixture;
use crate::table::Cell;
use crate::*;

/// The options a golden package is built with: no clock, so two runs and the checked-in file agree.
fn golden_options(project: &Project) -> ExportOptions {
    ExportOptions {
        timestamp: None,
        document: DocumentMeta { file: Some("cabinet.fcs".to_owned()), revision: project.revision().get(), hash: None },
        order: Some(OrderMeta { number: Some("2026-114".to_owned()), version: Some(1) }),
    }
}

fn built() -> OrderPackage {
    let (project, catalog) = fixture();
    let options = golden_options(&project);
    OrderPackage::build(&project, &catalog, options).expect("the fixture builds a package")
}

fn scratch(name: &str) -> PathBuf {
    furnicraft_testkit::scratch_dir(name)
}

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/package")
}

#[test]
fn every_table_is_declared_once_with_a_key_it_carries() {
    let mut names = BTreeSet::new();
    for table in TABLES {
        assert!(names.insert(table.name), "table `{}` is declared twice", table.name);
        assert!(table.file.ends_with(".jsonl"), "`{}` writes a JSON Lines file", table.name);
        assert!(!table.columns.is_empty(), "`{}` has columns", table.name);
        for key in table.key.split('+') {
            assert!(
                table.columns.iter().any(|column| column.name == key),
                "`{}` orders by `{key}`, which is not one of its columns",
                table.name
            );
        }
        let mut columns = BTreeSet::new();
        for column in table.columns {
            assert!(columns.insert(column.name), "`{}` declares `{}` twice", table.name, column.name);
        }
    }
}

#[test]
fn a_record_is_checked_against_the_column_table() {
    let spec = table("materials").expect("the materials table is declared");
    let mut data = TableData::new(spec);
    assert!(matches!(
        data.push(vec![Cell::Integer(1)]),
        Err(ExportError::ColumnCount { table: "materials", expected: 6, .. })
    ));
    assert!(matches!(
        data.push(vec![
            Cell::Integer(1),
            Cell::Text("Board".to_owned()),
            Cell::Integer(18),
            Cell::Integer(18_000),
            Cell::Integer(0),
            Cell::Text("white".to_owned()),
        ]),
        Err(ExportError::ColumnType { table: "materials", column: "kind", .. })
    ));
}

#[test]
fn a_fixed_time_formats_as_rfc3339() {
    assert_eq!(timestamp_from_unix(0).expect("the epoch"), "1970-01-01T00:00:00Z");
    assert_eq!(timestamp_from_unix(1_700_000_000).expect("a known time"), "2023-11-14T22:13:20Z");
    assert_eq!(timestamp_from_unix(1_709_164_800).expect("the leap day"), "2024-02-29T00:00:00Z");
}

#[test]
fn the_manifest_lists_the_writer_tables_and_their_columns() {
    let directory = scratch("manifest");
    built().write_dir(&directory).expect("the package is written");
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(directory.join("manifest.json")).expect("the manifest"))
            .expect("the manifest is JSON");
    assert_eq!(manifest["format"], FORMAT);
    assert_eq!(manifest["version"], VERSION);
    assert_eq!(manifest["units"]["rotation_scale"], ROTATION_SCALE);
    let tables = manifest["tables"].as_array().expect("tables are an array");
    assert_eq!(tables.len(), TABLES.len());
    for (entry, spec) in tables.iter().zip(TABLES) {
        assert_eq!(entry["name"], spec.name);
        assert_eq!(entry["file"], spec.file);
        assert_eq!(entry["key"], spec.key);
        let columns = entry["columns"].as_array().expect("columns are an array");
        assert_eq!(columns.len(), spec.columns.len());
        for (column, declared) in columns.iter().zip(spec.columns) {
            assert_eq!(column["name"], declared.name);
            assert_eq!(column["type"], declared.kind.as_str());
        }
        let count = manifest["counts"][spec.name].as_u64().expect("a count");
        assert_eq!(
            count as usize,
            std::fs::read_to_string(directory.join(spec.file)).expect("a table").lines().count()
        );
    }
}

#[test]
fn every_csv_copy_carries_exactly_the_columns_its_table_declares() {
    let directory = scratch("csv-columns");
    built().write_dir(&directory).expect("the package is written");
    for spec in TABLES {
        let csv = std::fs::read_to_string(directory.join(csv_name(spec.name))).expect("a csv copy");
        let header = csv.lines().next().expect("a header line");
        let names: Vec<&str> = header.split(',').collect();
        let declared: Vec<&str> = spec.columns.iter().map(|column| column.name).collect();
        assert_eq!(names, declared, "the csv header of `{}` matches its declared columns", spec.name);
    }
}

#[test]
fn two_exports_of_one_revision_are_identical() {
    let package = built();
    let first = scratch("identical-first");
    let second = scratch("identical-second");
    package.write_dir(&first).expect("the first package is written");
    package.write_dir(&second).expect("the second package is written");
    assert_dirs_equal(&first, &second);
}

#[test]
fn the_single_form_embeds_the_same_manifest_and_tables() {
    let package = built();
    let directory = scratch("single-dir");
    package.write_dir(&directory).expect("the directory form is written");
    let single = directory.join("package.json");
    package.write_single(&single).expect("the single form is written");
    let value: Value = serde_json::from_slice(&std::fs::read(&single).expect("the single file")).expect("JSON");
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(directory.join("manifest.json")).expect("the manifest")).expect("JSON");
    assert_eq!(value["manifest"], manifest);
    for spec in TABLES {
        let lines = std::fs::read_to_string(directory.join(spec.file)).expect("a table");
        let expected: Vec<Value> = lines.lines().map(|line| serde_json::from_str(line).expect("a record")).collect();
        assert_eq!(value["tables"][spec.name], Value::Array(expected));
    }
}

#[test]
fn no_machine_table_carries_a_float() {
    let directory = scratch("no-floats");
    built().write_dir(&directory).expect("the package is written");
    for spec in TABLES {
        let lines = std::fs::read_to_string(directory.join(spec.file)).expect("a table");
        for line in lines.lines() {
            let record: Value = serde_json::from_str(line).expect("a record");
            assert!(!has_float(&record), "`{}` carries a float: {line}", spec.name);
        }
    }
}

#[test]
fn two_products_group_their_parts_by_product_id() {
    let package = built();
    let parts = package.table("parts").expect("the parts table");
    let products = package.table("products").expect("the products table");
    let product_ids: BTreeSet<i64> = products
        .rows()
        .iter()
        .filter_map(|row| match row.first() {
            Some(Cell::Integer(id)) => Some(*id),
            _ => None,
        })
        .collect();
    assert_eq!(product_ids.len(), 2, "the fixture carries two cabinets");
    let part_products: BTreeSet<i64> = parts
        .rows()
        .iter()
        .filter_map(|row| match row.get(1) {
            Some(Cell::Integer(id)) if *id != 0 => Some(*id),
            _ => None,
        })
        .collect();
    assert_eq!(part_products, product_ids, "every product owns at least one part");
}

#[test]
fn a_csv_copy_quotes_text_that_needs_it() {
    let spec = table("materials").expect("the materials table is declared");
    let mut data = TableData::new(spec);
    data.push(vec![
        Cell::Integer(1),
        Cell::Text("Board, oak".to_owned()),
        Cell::Text("chipboard".to_owned()),
        Cell::Integer(18_000),
        Cell::Integer(0),
        Cell::Text("white".to_owned()),
    ])
    .expect("a valid row");
    let csv = table_csv(&data).expect("the csv copy");
    assert!(csv.contains("\"Board, oak\""), "a comma in a name is quoted");
}

#[test]
fn a_glass_material_makes_its_facade_read_as_glass() {
    let (project, _) = fixture();
    let panel =
        PanelMaterial::new(sample_material_id(), "Glass", "glass", millimetres(18), "clear").expect("a glass material");
    let edge = EdgeBandingMaterial::new(
        sample_edge_material_id(),
        "PVC edge",
        "pvc",
        sample_band_thickness(),
        sample_band_width(),
        "white",
    )
    .expect("an edge material");
    let catalog = Catalog::new(vec![panel], vec![edge]).expect("a catalogue");
    let package = OrderPackage::build(&project, &catalog, golden_options(&project)).expect("the package");
    let facades = package.table("facades").expect("the facades table");
    assert_eq!(facades.rows().first().and_then(|row| row.get(8)), Some(&Cell::Boolean(true)));
}

#[test]
fn a_part_carries_the_sum_of_its_banding_units() {
    let package = built();
    let parts = package.table("parts").expect("the parts table");
    let banding = package.table("banding").expect("the banding table");
    for row in parts.rows() {
        let Some(Cell::Integer(node_id)) = row.first() else {
            continue;
        };
        let total = banding
            .rows()
            .iter()
            .filter(|band| matches!(band.first(), Some(Cell::Integer(id)) if id == node_id))
            .filter_map(|band| match band.get(6) {
                Some(Cell::Integer(length)) => Some(*length),
                _ => None,
            })
            .sum::<i64>();
        assert_eq!(row.get(12), Some(&Cell::Integer(total)), "node {node_id} totals its bands");
    }
}

#[test]
fn the_package_matches_the_golden() {
    let package = built();
    let update = std::env::var_os("FURNICRAFT_UPDATE_GOLDEN").is_some();
    let directory = if update { golden_dir() } else { scratch("golden") };
    package.write_dir(&directory).expect("the package is written");
    if update {
        return;
    }
    assert_dirs_equal(&directory, &golden_dir());
}

/// Every file of two package directories is byte-identical.
fn assert_dirs_equal(first: &Path, second: &Path) {
    let mut names: Vec<String> = std::fs::read_dir(first)
        .expect("the first directory")
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    names.sort();
    let mut expected: Vec<String> = std::fs::read_dir(second)
        .expect("the second directory")
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    expected.sort();
    assert_eq!(names, expected, "the two packages hold the same files");
    for name in names {
        let left = std::fs::read(first.join(&name)).expect("a file of the first package");
        let right = std::fs::read(second.join(&name)).expect("a file of the second package");
        assert_eq!(left, right, "`{name}` is byte-identical");
    }
}

fn has_float(value: &Value) -> bool {
    match value {
        Value::Number(number) => number.is_f64(),
        Value::Array(values) => values.iter().any(has_float),
        Value::Object(values) => values.values().any(has_float),
        _ => false,
    }
}
