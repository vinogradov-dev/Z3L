//! Test fixtures and helpers shared by Z3L crates.
//!
//! Development-only: nothing in the product may depend on this crate.
//!
//! A helper lives here when more than one crate's tests need it. `millimetres`, `open_path`,
//! `cube_mesh`, `sample_project` and `sample_product_project` are defined in
//! `model::testing` and re-exported here, because that is the only crate that can define
//! them for both itself and everyone else (C7).
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

use std::io;
use std::path::{Path, PathBuf};

use catalog::{Currency, Directory, Money, Starter};
use model::{ItemId, PanelNominalSize, PanelRect};
use units::Length;

/// The fixtures every crate's tests start from, defined once in `model::testing`
/// (C7): a crate cannot re-export them back into `model`, because the dev-dependency
/// cycle would compile the model twice and its types would not match.
pub use model::testing::{cube_mesh, millimetres, open_path, sample_product_project, sample_project};

/// A unique, empty scratch directory under the session temp root.
///
/// A fixture that writes a file needs a path nobody else uses: the name keeps the call sites of one
/// test binary apart and the process id keeps two binaries running at once apart. The directory is
/// removed and created again, so a test that died halfway never hands its leftovers to the next
/// run. Failures are left to the test: a directory that cannot be made shows up as a missing file.
pub fn scratch_dir(name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!("z3l-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    let _ = std::fs::create_dir_all(&directory);
    directory
}

/// The package number the sample fixtures' directory uses.
pub const SAMPLE_PACKAGE: u32 = 1;

/// The stable id of the sample panel material.
#[allow(clippy::expect_used)]
pub fn sample_material_id() -> ItemId {
    ItemId::new(SAMPLE_PACKAGE, 1).expect("non-zero serial")
}

/// The stable id of the sample edge-banding material.
#[allow(clippy::expect_used)]
pub fn sample_edge_material_id() -> ItemId {
    ItemId::new(SAMPLE_PACKAGE, 101).expect("non-zero serial")
}

/// The fallback thickness of a sample panel.
pub const fn sample_panel_thickness() -> Length {
    Length::from_micrometres(18_000)
}

/// A 5 mm fallback thickness for material-change diagnostics.
pub const fn sample_thin_panel_thickness() -> Length {
    Length::from_micrometres(5_000)
}

/// A 500 mm banding thickness, wide enough to empty the sample core.
pub const fn sample_wide_band_thickness() -> Length {
    Length::from_micrometres(500_000)
}

/// A 2 mm banding profile thickness in the panel plane.
pub const fn sample_band_thickness() -> Length {
    Length::from_micrometres(2_000)
}

/// A 22 mm banding width.
pub const fn sample_band_width() -> Length {
    Length::from_micrometres(22_000)
}

/// A 700 x 500 mm nominal panel used by tests that need a valid size.
#[allow(clippy::expect_used)]
pub fn sample_panel_nominal_size() -> PanelNominalSize {
    PanelNominalSize::new(
        Length::from_millimetres(700).expect("700 mm fits a length"),
        Length::from_millimetres(500).expect("500 mm fits a length"),
    )
    .expect("sample panel size is within limits")
}

/// A second valid nominal panel size, used by resize tests.
#[allow(clippy::expect_used)]
pub fn sample_resized_panel_nominal_size() -> PanelNominalSize {
    PanelNominalSize::new(
        Length::from_millimetres(900).expect("900 mm fits a length"),
        Length::from_millimetres(600).expect("600 mm fits a length"),
    )
    .expect("sample panel size is within limits")
}

/// A 500 x 500 mm nominal panel, narrow enough to leave the 600 mm cutout fully outside the core.
#[allow(clippy::expect_used)]
pub fn sample_narrow_panel_nominal_size() -> PanelNominalSize {
    PanelNominalSize::new(
        Length::from_millimetres(500).expect("500 mm fits a length"),
        Length::from_millimetres(500).expect("500 mm fits a length"),
    )
    .expect("sample panel size is within limits")
}

/// The starter package with an 18 mm chipboard and a 2 x 22 mm PVC edge.
///
/// The board is stocked as a 2800 x 2070 mm sheet in the group `chipboard-18` and priced at 3200.00
/// roubles a sheet, so every fixture that exports a package exercises the stock block and the money
/// columns instead of an empty corner of the contract.
#[allow(clippy::expect_used)]
pub fn sample_catalog() -> Directory {
    let millimetres = |value: i64| Length::from_micrometres(value * 1_000);
    let sheet = Some((millimetres(2_800), millimetres(2_070)));
    Starter::new()
        .board(1, "Chipboard", "chipboard", sample_panel_thickness(), "white", sheet)
        .stocked("chipboard-18", Money::from_minor(320_000))
        .banding(101, "PVC edge", "pvc", millimetres(2), millimetres(22), "white")
        .currency(Currency::new("RUB", 100).ok())
        .directory()
        .expect("the sample package is valid")
}

/// The starter package with a board of `board` thickness and an edge band of `band` by `width`.
#[allow(clippy::expect_used)]
pub fn sample_catalog_with(board: Length, band: Length, width: Length) -> Directory {
    Starter::new()
        .board(1, "Thick board", "mdf", board, "white", None)
        .banding(101, "Thick edge", "abs", band, width, "white")
        .currency(Currency::new("RUB", 100).ok())
        .directory()
        .expect("the fixture package is valid")
}

/// The starter package with one board of `thickness` and no edge band.
///
/// A test that needs a board other than the sample one asks for it here instead of building a
/// catalogue of its own, so every fixture speaks the directory's language.
#[allow(clippy::expect_used)]
pub fn sample_catalog_with_thickness(thickness: Length) -> Directory {
    Starter::new()
        .board(1, "Thin board", "mdf", thickness, "white", None)
        .currency(Currency::new("RUB", 100).ok())
        .directory()
        .expect("the fixture package is valid")
}

/// The starter package with one edge band of `thickness` and `width`, and no board.
#[allow(clippy::expect_used)]
pub fn sample_catalog_with_band(thickness: Length, width: Length) -> Directory {
    Starter::new()
        .banding(101, "Thick edge", "abs", thickness, width, "white")
        .currency(Currency::new("RUB", 100).ok())
        .directory()
        .expect("the fixture package is valid")
}

/// A 100 x 50 mm cutout rectangle at `(10, 10)` on the sample panel.
#[allow(clippy::expect_used)]
pub fn sample_cutout_rect() -> PanelRect {
    PanelRect::new(
        Length::from_millimetres(10).expect("10 mm fits a length"),
        Length::from_millimetres(10).expect("10 mm fits a length"),
        Length::from_millimetres(100).expect("100 mm fits a length"),
        Length::from_millimetres(50).expect("50 mm fits a length"),
    )
    .expect("sample cutout rectangle is valid")
}

/// A rectangle that overlaps [`sample_cutout_rect`] on the sample panel.
#[allow(clippy::expect_used)]
pub fn sample_overlapping_cutout_rect() -> PanelRect {
    PanelRect::new(
        Length::from_millimetres(50).expect("50 mm fits a length"),
        Length::from_millimetres(20).expect("20 mm fits a length"),
        Length::from_millimetres(100).expect("100 mm fits a length"),
        Length::from_millimetres(50).expect("50 mm fits a length"),
    )
    .expect("overlapping cutout rectangle is valid")
}

/// A 50 x 50 mm rectangle near the right edge of the 700 mm sample panel.
#[allow(clippy::expect_used)]
pub fn sample_edge_cutout_rect() -> PanelRect {
    PanelRect::new(
        Length::from_millimetres(600).expect("600 mm fits a length"),
        Length::from_millimetres(10).expect("10 mm fits a length"),
        Length::from_millimetres(50).expect("50 mm fits a length"),
        Length::from_millimetres(50).expect("50 mm fits a length"),
    )
    .expect("edge cutout rectangle is valid")
}

/// A rectangle that starts entirely past the 700 mm sample panel.
#[allow(clippy::expect_used)]
pub fn sample_outside_cutout_rect() -> PanelRect {
    PanelRect::new(
        Length::from_millimetres(720).expect("720 mm fits a length"),
        Length::from_millimetres(10).expect("10 mm fits a length"),
        Length::from_millimetres(100).expect("100 mm fits a length"),
        Length::from_millimetres(50).expect("50 mm fits a length"),
    )
    .expect("outside cutout rectangle is valid")
}

/// A rectangle that crosses the right edge of the 700 mm sample panel.
#[allow(clippy::expect_used)]
pub fn sample_notch_cutout_rect() -> PanelRect {
    PanelRect::new(
        Length::from_millimetres(650).expect("650 mm fits a length"),
        Length::from_millimetres(10).expect("10 mm fits a length"),
        Length::from_millimetres(100).expect("100 mm fits a length"),
        Length::from_millimetres(50).expect("50 mm fits a length"),
    )
    .expect("notch cutout rectangle is valid")
}

/// An 8 mm blind depth for the 18 mm sample panel.
#[allow(clippy::expect_used)]
pub fn sample_blind_depth() -> Length {
    Length::from_millimetres(8).expect("8 mm fits a length")
}

/// The directory the interface shots go to, created if it is not there: `target/ui-shots`.
///
/// `CARGO_TARGET_DIR` wins when the environment names one, so a session that builds into another
/// directory still finds its pictures; otherwise the workspace root is two levels above this
/// crate. A shot is a picture a person looks at, so the directory is collected under the build
/// directory rather than in the source tree, and `cargo clean` takes it away with everything else.
pub fn ui_shots_dir() -> io::Result<PathBuf> {
    let target = match std::env::var_os("CARGO_TARGET_DIR") {
        Some(directory) => PathBuf::from(directory),
        None => Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .map(|root| root.join("target"))
            .ok_or_else(|| io::Error::other("the testkit directory has no workspace root above it"))?,
    };
    let directory = target.join("ui-shots");
    std::fs::create_dir_all(&directory)?;
    Ok(directory)
}

/// Writes tightly packed 8-bit RGBA `pixels` as a PNG at `path`.
///
/// The shots are looked at, never compared pixel by pixel, so the encoder is deliberately plain:
/// one image, eight bits per channel, straight alpha, no interlacing. A buffer that is not
/// `width × height × 4` bytes is an [`io::ErrorKind::InvalidInput`] instead of a half-written file.
pub fn write_png(path: &Path, width: u32, height: u32, pixels: &[u8]) -> io::Result<()> {
    let expected = usize::try_from(width)
        .ok()
        .and_then(|width| usize::try_from(height).ok().and_then(|height| width.checked_mul(height)))
        .and_then(|pixels| pixels.checked_mul(4));
    if expected != Some(pixels.len()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("a {width} × {height} RGBA image needs {} bytes, not {}", expected.unwrap_or(0), pixels.len()),
        ));
    }
    let file = std::fs::File::create(path)?;
    let mut encoder = png::Encoder::new(io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(io::Error::other)?;
    writer.write_image_data(pixels).map_err(io::Error::other)?;
    writer.finish().map_err(io::Error::other)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    /// Four pixels of known colours, so a round trip proves the channel order and the row order.
    const PIXELS: [u8; 16] = [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255];

    #[test]
    fn a_written_png_reads_back_with_the_pixels_and_the_size_it_was_given() {
        let path = super::scratch_dir("png-round-trip").join("two-by-two.png");
        super::write_png(&path, 2, 2, &PIXELS).expect("the shot is written");
        let file = std::io::BufReader::new(std::fs::File::open(&path).expect("the file is there"));
        let decoder = png::Decoder::new(file);
        let mut reader = decoder.read_info().expect("the file is a PNG");
        let mut buffer = vec![0; reader.output_buffer_size().expect("a bounded image")];
        let info = reader.next_frame(&mut buffer).expect("the image decodes");
        assert_eq!((info.width, info.height), (2, 2), "the size the writer was given is the size the file says");
        assert_eq!(info.color_type, png::ColorType::Rgba, "and the channels it was given");
        assert_eq!(buffer, PIXELS, "every channel comes back where it was");
    }

    #[test]
    fn a_buffer_that_is_not_the_image_size_is_refused_before_the_file_is_touched() {
        let directory = super::scratch_dir("png-size");
        let path = directory.join("short.png");
        let error = super::write_png(&path, 2, 2, &PIXELS[..12]).expect_err("a short buffer is an error");
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
        assert!(!path.exists(), "and the refused shot leaves no file behind");
    }

    #[test]
    fn the_interface_shots_go_to_a_directory_under_the_build_directory() {
        let directory = super::ui_shots_dir().expect("the shots directory is created");
        assert!(directory.is_dir(), "which exists");
        assert!(directory.ends_with("target/ui-shots"), "and is the one the task names: {}", directory.display());
    }
}
