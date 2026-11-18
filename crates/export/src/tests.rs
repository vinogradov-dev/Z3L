//! The exporter's own gate: the column table, the two writers, the number boundary and the golden.

#![allow(clippy::expect_used)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use model::Project;
use serde_json::Value;
use testkit::{millimetres, sample_edge_material_id, sample_material_id};

use crate::fixtures::fixture;
use crate::table::Cell;
use crate::*;

/// The options a golden package is built with: no clock, so two runs and the checked-in file agree.
fn golden_options(project: &Project) -> ExportOptions {
    ExportOptions {
        timestamp: None,
        document: DocumentMeta { file: Some("cabinet.z3l".to_owned()), revision: project.revision().get(), hash: None },
        order: Some(OrderMeta { number: Some("2026-114".to_owned()), version: Some(1) }),
    }
}

fn built() -> OrderPackage {
    let (project, catalog) = fixture();
    let options = golden_options(&project);
    OrderPackage::build(&project, &catalog, options).expect("the fixture builds a package")
}

fn scratch(name: &str) -> PathBuf {
    testkit::scratch_dir(name)
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
        Err(ExportError::ColumnCount { table: "materials", expected: 10, .. })
    ));
    assert!(matches!(
        data.push(materials_row(Cell::Integer(18))),
        Err(ExportError::ColumnType { table: "materials", column: "kind", .. })
    ));
}

/// One valid `materials` row, with the `kind` cell replaced by whatever a test needs it to be.
fn materials_row(kind: Cell) -> Vec<Cell> {
    vec![
        Cell::Text("1-1".to_owned()),
        Cell::Text("Board".to_owned()),
        kind,
        Cell::Integer(18_000),
        Cell::Integer(0),
        Cell::Text("white".to_owned()),
        Cell::Integer(2_800_000),
        Cell::Integer(2_070_000),
        Cell::Text("chipboard-18".to_owned()),
        Cell::Integer(320_000),
    ]
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
    let mut row = materials_row(Cell::Text("chipboard".to_owned()));
    row[1] = Cell::Text("Board, oak".to_owned());
    data.push(row).expect("a valid row");
    let csv = table_csv(&data).expect("the csv copy");
    assert!(csv.contains("\"Board, oak\""), "a comma in a name is quoted");
}

#[test]
fn a_glass_material_makes_its_facade_read_as_glass() {
    let (project, _) = fixture();
    let catalog = catalog::Starter::new()
        .board(1, "Glass", "glass", millimetres(18), "clear", None)
        .directory()
        .expect("a package whose board is glass");
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
fn a_stocked_material_carries_its_sheet_group_and_price() {
    let package = built();
    let materials = package.table("materials").expect("the materials table");
    let board = materials
        .rows()
        .iter()
        .find(|row| matches!(row.first(), Some(Cell::Text(id)) if *id == sample_material_id().to_string()))
        .expect("the board row");
    assert_eq!(board.get(6), Some(&Cell::Integer(2_800_000)), "the sheet length");
    assert_eq!(board.get(7), Some(&Cell::Integer(2_070_000)), "the sheet width");
    assert_eq!(board.get(8), Some(&Cell::Text("chipboard-18".to_owned())), "the stock group");
    assert_eq!(board.get(9), Some(&Cell::Integer(320_000)), "the sheet price");
    let edge = materials
        .rows()
        .iter()
        .find(|row| matches!(row.first(), Some(Cell::Text(id)) if *id == sample_edge_material_id().to_string()))
        .expect("the edge row");
    assert_eq!(edge.get(6), Some(&Cell::Integer(0)), "banding is not cut from a sheet");
    assert_eq!(edge.get(8), Some(&Cell::Text(String::new())));
    assert_eq!(edge.get(9), Some(&Cell::Integer(0)));
}

#[test]
fn every_part_costs_a_rounded_share_of_its_sheet() {
    let package = built();
    let parts = package.table("parts").expect("the parts table");
    let sheet_area = 2800i128 * 2070;
    let mut total = 0i64;
    let board = sample_material_id().to_string();
    for row in parts.rows() {
        let Some(Cell::Integer(area)) = row.get(9) else {
            panic!("a part carries its area");
        };
        let expected = match row.get(3) {
            Some(Cell::Text(material)) if *material == board => {
                (i128::from(320_000) * i128::from(*area) + sheet_area / 2) / sheet_area
            }
            _ => 0,
        };
        let expected = i64::try_from(expected).expect("a price fits");
        assert_eq!(row.get(13), Some(&Cell::Integer(expected)), "a part costs its area of the sheet");
        total = total.saturating_add(expected);
    }
    assert!(total > 0, "the fixture prices its parts");
    let products = package.table("products").expect("the products table");
    let priced: i64 = products
        .rows()
        .iter()
        .filter_map(|row| match row.get(15) {
            Some(Cell::Integer(price)) => Some(*price),
            _ => None,
        })
        .sum();
    assert_eq!(priced, total, "a product totals the prices of its parts");
}

#[test]
fn a_stocked_material_the_order_cuts_gets_one_sheet_row() {
    let package = built();
    let sheets = package.table("sheets").expect("the sheets table");
    assert_eq!(sheets.rows().len(), 1, "the fixture cuts one stocked material");
    let row = sheets.rows().first().expect("one sheet row");
    assert_eq!(row[0], Cell::Text(sample_material_id().to_string()));
    assert_eq!(row[1], Cell::Text("chipboard-18".to_owned()));
    assert_eq!(row[2], Cell::Integer(2_800_000));
    assert_eq!(row[3], Cell::Integer(2_070_000));
    assert_eq!(row[4], Cell::Integer(RULES.usable_length(2_800_000)), "the trim is taken per side");
    assert_eq!(row[5], Cell::Integer(RULES.usable_width(2_070_000)));
    assert_eq!(row[6], Cell::Integer(320_000));
    assert_eq!(row[9], Cell::Integer(5_796_000), "2.8 x 2.07 m in square millimetres");

    let parts = package.table("parts").expect("the parts table");
    let of_material: Vec<&Vec<Cell>> = parts
        .rows()
        .iter()
        .filter(|part| matches!(part.get(3), Some(Cell::Text(id)) if *id == sample_material_id().to_string()))
        .collect();
    assert_eq!(row[7], Cell::Integer(i64::try_from(of_material.len()).expect("a count")));
    let area: i64 = of_material
        .iter()
        .filter_map(|part| match part.get(9) {
            Some(Cell::Integer(area)) => Some(*area),
            _ => None,
        })
        .sum();
    assert_eq!(row[8], Cell::Integer(area), "the row sums the areas of its parts");
}

#[test]
fn the_manifest_declares_the_rules_and_the_currency() {
    let directory = scratch("manifest-money");
    built().write_dir(&directory).expect("the package is written");
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(directory.join("manifest.json")).expect("the manifest"))
            .expect("the manifest is JSON");
    assert_eq!(manifest["rules"]["kerf_um"], RULES.kerf_um);
    assert_eq!(manifest["rules"]["trim_um"], RULES.trim_um);
    assert_eq!(manifest["money"]["currency"], "RUB");
    assert_eq!(manifest["money"]["minor_scale"], 100);
    assert_eq!(manifest["version"], 1, "the additive columns keep the major version");
}

#[test]
fn a_catalogue_without_stock_writes_no_sheet_row_and_no_currency() {
    let (project, _) = fixture();
    let catalog = catalog::Starter::new()
        .board(1, "Chipboard", "chipboard", millimetres(18), "white", None)
        .directory()
        .expect("a package with no stock block");
    let package = OrderPackage::build(&project, &catalog, golden_options(&project)).expect("the package");
    assert!(package.table("sheets").expect("the sheets table").rows().is_empty());
    assert!(package.manifest().money.is_none(), "no price, no currency block");
    let parts = package.table("parts").expect("the parts table");
    for part in parts.rows() {
        assert_eq!(part.get(13), Some(&Cell::Integer(0)), "an unpriced part costs zero");
    }
}

#[test]
fn a_rule_takes_the_trim_from_both_sides() {
    assert_eq!(Rules { kerf_um: 4_000, trim_um: 10_000 }.usable_length(2_800_000), 2_780_000);
    assert_eq!(Rules { kerf_um: 4_000, trim_um: 10_000 }.usable_width(2_070_000), 2_050_000);
    assert_eq!(Rules { kerf_um: 0, trim_um: 2_000 }.usable_length(1_000), 0, "a sheet narrower than its trim has none");
}

#[test]
fn the_package_matches_the_golden() {
    let package = built();
    let update = std::env::var_os("Z3L_UPDATE_GOLDEN").is_some();
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
