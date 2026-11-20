//! Test helpers the shell's own test modules share.
//!
//! Only a test build compiles this module: it holds the collector that walks an egui frame's shapes
//! for their text, so the status bar, the folded side panel and the strip assert what they drew
//! without a window or a GPU.
//!
//! Changes when: a shell test needs a new way to read what a surface drew.

use eframe::egui;
use std::path::Path;

use units::Length;

/// Writes the sample package as the folder a workshop edits: the `nomenclature/` a reader loads.
///
/// The rows are the ones `testkit::sample_catalog` builds — package 1's board `1-1` and band
/// `1-101` — in the written form `crates/catalog` reads, so a test changes one number the way the
/// directory application would and watches the editor pick the file up (O5). The package is named
/// `sample` rather than after a foreseeable workshop, because it is a fixture.
pub(crate) fn write_sample_folder(folder: &Path, board: Length, name: &str) {
    let package = folder.join("packages").join("sample");
    let items = package.join("items");
    std::fs::create_dir_all(&items).expect("the package folder");
    write_json(&folder.join("directory.json"), serde_json::json!({ "version": 1, "packages": ["sample"] }));
    write_json(
        &package.join("manifest.json"),
        serde_json::json!({ "version": 1, "id": 1, "name": "Sample", "package_version": 1 }),
    );
    write_json(
        &package.join("registry.json"),
        serde_json::json!({
            "version": 1,
            "properties": [
                { "code": 1, "slug": "thickness", "label": "Thickness", "type": "length", "min_um": 1000, "max_um": 1000000 },
                { "code": 2, "slug": "width", "label": "Width", "type": "length", "min_um": 1000, "max_um": 1000000 },
                { "code": 3, "slug": "color", "label": "Decor", "type": "text", "max_chars": 48 }
            ],
            "kinds": [{
                "code": 1, "slug": "sheet-material", "label": "Sheet material",
                "accepts": [1, 2, 3], "required": [1], "traits": ["sheet", "nestable"]
            }, {
                "code": 2, "slug": "edge-banding", "label": "Edge banding",
                "accepts": [1, 2, 3], "required": [1, 2], "traits": ["banded", "bought"]
            }],
            "families": []
        }),
    );
    write_json(
        &items.join("board-1.json"),
        serde_json::json!({
            "id": "1-1", "kind": "sheet-material", "name": name, "slug": "board-1",
            "props": {
                "thickness": { "type": "length", "value": board.micrometres() },
                "color": { "type": "text", "value": "white" }
            }
        }),
    );
    write_json(
        &items.join("band-101.json"),
        serde_json::json!({
            "id": "1-101", "kind": "edge-banding", "name": "PVC edge", "slug": "band-101",
            "props": {
                "thickness": { "type": "length", "value": 2000 },
                "width": { "type": "length", "value": 22000 },
                "color": { "type": "text", "value": "white" }
            }
        }),
    );
}

/// Writes one JSON file of a fixture folder.
fn write_json(path: &Path, value: serde_json::Value) {
    std::fs::write(path, value.to_string()).expect("the fixture folder is writable");
}

/// Collects the text of every text shape, including nested ones.
pub(crate) fn drawn_text(shapes: &[egui::epaint::ClippedShape]) -> Vec<String> {
    drawn_text_rects(shapes).into_iter().map(|(_, text)| text).collect()
}

/// Collects the rectangle of every text shape with the text it covers, including nested ones.
///
/// A test that asks whether a row is clipped needs where the glyphs landed, not only what they say:
/// the panels' scrollers are the surface that must keep the last row clear of the bar (X39).
/// A shape clipped to nothing is not drawn: `epaint` skips it, so a test skips it too. A hidden
/// measuring pass leaves its galleys in the list this way, and a test that counted them would
/// believe the row drew what the row only measured (X94).
pub(crate) fn drawn_text_rects(shapes: &[egui::epaint::ClippedShape]) -> Vec<(egui::Rect, String)> {
    fn walk(shape: &egui::Shape, out: &mut Vec<(egui::Rect, String)>) {
        match shape {
            egui::Shape::Text(text) => {
                out.push((egui::Rect::from_min_size(text.pos, text.galley.size()), text.galley.text().to_owned()));
            }
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    walk(shape, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for clipped in shapes {
        if clipped.clip_rect.is_positive() {
            walk(&clipped.shape, &mut out);
        }
    }
    out
}
