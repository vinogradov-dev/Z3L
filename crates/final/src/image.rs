//! The pixels of a final image: linear radiance in, one PNG out.
//!
//! The tracer accumulates radiance as `f32` linear RGB, one triple per pixel, because that is what a
//! light-transport estimator produces and what a second render can be compared with. Everything a
//! person looks at happens here, in one place: the exposure, the filmic curve that turns radiance
//! into a display value, the sRGB transfer function and the eight bits a file carries. The curve is
//! Narkowicz's 2015 ACES fit, chosen for one reason — a 1000 W lamp and a sunlit panel have to share
//! one image without either going flat — and the transfer function is the sRGB standard.
//!
//! Why the encoding is ours and not `image`'s: this crate writes one 8-bit RGBA PNG and nothing else,
//! and the `png` crate is already in the workspace for the interface shots. A codec package for
//! every format a person might want would be a dependency nothing here reads (`AGENTS.md` §4.8).
//!
//! Refuses: a radiance buffer that is not one triple per pixel, a size of zero, a channel that is not
//! finite, and a path that cannot be written — each is a [`FinalError`] sentence with the field or
//! the path in it, never a silently black pixel or a half-written file. A file is written beside its
//! destination and renamed onto it, so a failure leaves the old image and no `.part` behind.
//!
//! Changes when: the curve, the transfer function or the channel order of the file changes.

use std::io::Write as _;
use std::path::Path;

use crate::FinalError;

/// One rendered image: linear RGB radiance per pixel, row-major, the first row at the top.
#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    width: u32,
    height: u32,
    radiance: Vec<[f32; 3]>,
}

impl Image {
    /// A black image of the given size, or a refusal when the size does not multiply out.
    pub fn new(width: u32, height: u32) -> Result<Self, FinalError> {
        let pixels = u64::from(width).checked_mul(u64::from(height)).filter(|pixels| *pixels > 0);
        let Some(pixels) = pixels else {
            return Err(FinalError::ImageSize { width, height });
        };
        let Ok(capacity) = usize::try_from(pixels) else {
            return Err(FinalError::ImageSize { width, height });
        };
        Ok(Self { width, height, radiance: vec![[0.0; 3]; capacity] })
    }

    /// The average radiance of a render, from the accumulator the device wrote back.
    ///
    /// The accumulator holds one `vec4` per pixel — three channels and the number of samples behind
    /// them — summed over every sample, so the division here is the only place a sample count is
    /// trusted. The count is per pixel and not per image: a converged pixel retires before the last
    /// sample (`crate::denoise::MIN_SAMPLES`), so its fourth channel is smaller than the request's own
    /// count, and dividing it by the request would darken exactly the pixels that were cheap.
    pub fn from_accumulation(
        width: u32,
        height: u32,
        accumulated: &[[f32; 4]],
        samples: u32,
    ) -> Result<Self, FinalError> {
        let expected = u64::from(width).checked_mul(u64::from(height));
        if expected != Some(accumulated.len() as u64) || samples == 0 {
            return Err(FinalError::Accumulation { pixels: accumulated.len(), width, height, samples });
        }
        let mut image = Self::new(width, height)?;
        let offered = samples as f32;
        for (pixel, sum) in image.radiance.iter_mut().zip(accumulated) {
            // A pixel that reported no weight was never sampled: it is black rather than a division.
            let count = if sum[3] > 0.0 { sum[3] } else { offered };
            *pixel = [sum[0] / count, sum[1] / count, sum[2] / count];
        }
        Ok(image)
    }

    /// The image a filter produced: linear radiance per pixel, already averaged and already denoised.
    ///
    /// The four channels of an accumulator are not here — this picture is finished, so the only thing
    /// checked is that there is one triple per pixel and that every value is a number.
    pub fn from_linear(width: u32, height: u32, radiance: &[[f32; 3]]) -> Result<Self, FinalError> {
        let expected = u64::from(width).checked_mul(u64::from(height));
        if expected != Some(radiance.len() as u64) {
            return Err(FinalError::Accumulation { pixels: radiance.len(), width, height, samples: 0 });
        }
        let mut image = Self::new(width, height)?;
        for (pixel, value) in image.radiance.iter_mut().zip(radiance) {
            let channel = |channel: f32| if channel.is_finite() { channel.max(0.0) } else { 0.0 };
            *pixel = [channel(value[0]), channel(value[1]), channel(value[2])];
        }
        Ok(image)
    }

    /// The image width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// The image height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// The linear radiance at one pixel, or `None` outside the image.
    pub fn radiance_at(&self, x: u32, y: u32) -> Option<[f32; 3]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let index =
            usize::try_from(u64::from(y).checked_mul(u64::from(self.width))?.checked_add(u64::from(x))?).ok()?;
        self.radiance.get(index).copied()
    }

    /// Every pixel as eight-bit RGBA, row-major from the top.
    ///
    /// Alpha is opaque: a final image is a picture, and a transparent catalogue page is a later
    /// decision with its own command field.
    pub fn to_rgba8(&self, exposure: f32) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.radiance.len().saturating_mul(4));
        for pixel in &self.radiance {
            let [red, green, blue] = tone_map(*pixel, exposure).map(to_byte);
            bytes.extend_from_slice(&[red, green, blue, 255]);
        }
        bytes
    }
}

/// The display value of one linear channel with the given exposure, in `0..=1`.
///
/// The exposure multiplies before the curve, so it is a photographic stop rather than a brightness
/// knob on the finished image; the ACES fit compresses what is left, and the sRGB transfer function
/// is the one every screen expects. A channel that is not finite or negative is black: a hostile
/// sample must not become a hole in the picture.
pub fn display_channel(radiance: f32, exposure: f32) -> f32 {
    let exposure = if exposure.is_finite() && exposure > 0.0 { exposure } else { 1.0 };
    let value = radiance * exposure;
    if !value.is_finite() || value <= 0.0 {
        return 0.0;
    }
    srgb(aces(value))
}

/// One linear RGB radiance as a display value per channel.
pub fn tone_map(radiance: [f32; 3], exposure: f32) -> [f32; 3] {
    radiance.map(|channel| display_channel(channel, exposure))
}

/// The eight-bit value a display channel lands on, with a rounding step.
fn to_byte(channel: f32) -> u8 {
    let scaled = (channel.clamp(0.0, 1.0) * 255.0).round();
    if scaled.is_finite() { scaled as u8 } else { 0 }
}

/// Narkowicz's ACES filmic fit: `(x(ax+b))/(x(cx+d)+e)`, linear in, linear out.
///
/// The published constants; the fit is one multiply-add chain, which is why it is used instead of the
/// full ACES RRT, and its error is below a percent over the usable range.
fn aces(x: f32) -> f32 {
    const A: f32 = 2.51;
    const B: f32 = 0.03;
    const C: f32 = 2.43;
    const D: f32 = 0.59;
    const E: f32 = 0.14;
    ((x * (A * x + B)) / (x * (C * x + D) + E)).clamp(0.0, 1.0)
}

/// The sRGB transfer function: linear light to the encoded value a file carries.
fn srgb(linear: f32) -> f32 {
    if linear <= 0.003_130_8 { linear * 12.92 } else { 1.055 * linear.powf(1.0 / 2.4) - 0.055 }
}

/// The image as the bytes of one PNG: eight-bit RGBA, no interlacing, no metadata.
pub fn encode_png(image: &Image, exposure: f32) -> Result<Vec<u8>, FinalError> {
    let pixels = image.to_rgba8(exposure);
    encode_rgba8(image.width, image.height, &pixels)
}

/// The bytes of one PNG from pixels a caller already encoded: eight-bit RGBA, row-major, no
/// interlacing, no metadata.
///
/// The second caller is the agent's viewport preview (J10): `render` hands back the target's own
/// bytes, already gamma-corrected, so the tracer's curve must not run over them. One writer for the
/// one format keeps the preview and the final render from encoding two different pictures.
pub fn encode_rgba8(width: u32, height: u32, pixels: &[u8]) -> Result<Vec<u8>, FinalError> {
    let expected = (width as usize).checked_mul(height as usize).and_then(|pixels| pixels.checked_mul(4));
    if width == 0 || height == 0 || expected != Some(pixels.len()) {
        return Err(FinalError::ImageSize { width, height });
    }
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|error| FinalError::Image { reason: error.to_string() })?;
        writer.write_image_data(pixels).map_err(|error| FinalError::Image { reason: error.to_string() })?;
        writer.finish().map_err(|error| FinalError::Image { reason: error.to_string() })?;
    }
    Ok(bytes)
}

/// Writes encoded bytes to `path`, through a temporary file beside it.
///
/// The rename is what makes a cancelled or failed render leave the previous image untouched: a
/// person either has the new picture or the old one, never half of each. The temporary file shares
/// the directory with the destination, because a rename across filesystems is a copy and a copy is
/// another way to leave half a file.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), FinalError> {
    let mut temporary = path.to_path_buf();
    let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_else(|| "render".to_owned());
    temporary.set_file_name(format!(".{name}.part"));
    let fail = |reason: std::io::Error| FinalError::Write { path: path.to_path_buf(), reason: reason.to_string() };
    if let Err(error) = write_file(&temporary, bytes) {
        // A partial file is not the user's picture; take it away and report the destination.
        let _ = std::fs::remove_file(&temporary);
        return Err(fail(error));
    }
    std::fs::rename(&temporary, path).map_err(|error| {
        let _ = std::fs::remove_file(&temporary);
        fail(error)
    })
}

/// Writes bytes and flushes them, so the rename cannot publish a buffer the disk has not seen.
fn write_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let file = std::fs::File::create(path)?;
    let mut writer = std::io::BufWriter::new(file);
    writer.write_all(bytes)?;
    writer.flush()?;
    writer.get_ref().sync_all()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use testkit::scratch_dir;

    use super::*;

    fn image() -> Image {
        let mut image = Image::new(2, 2).expect("a two by two image");
        image.radiance = vec![[0.0, 0.0, 0.0], [0.18, 0.18, 0.18], [1.0, 1.0, 1.0], [40.0, 4.0, 0.4]];
        image
    }

    #[test]
    fn the_curve_is_monotonic_and_never_leaves_the_display_range() {
        let mut previous = -1.0f32;
        for step in 0..200 {
            let radiance = step as f32 * 0.1;
            let value = display_channel(radiance, 1.0);
            assert!(value >= previous, "the curve turned back at {radiance}: {value} after {previous}");
            assert!((0.0..=1.0).contains(&value), "{radiance} gave {value}");
            previous = value;
        }
        // A highlight past white is white, not a wrap.
        assert!(display_channel(10_000.0, 1.0) > 0.999, "{}", display_channel(10_000.0, 1.0));
        // A mid grey stays a mid grey: 0.18 linear is about the middle of the sRGB range.
        let middle = display_channel(0.18, 1.0);
        assert!((0.4..0.6).contains(&middle), "0.18 linear became {middle}");
    }

    #[test]
    fn exposure_is_a_stop_and_a_hostile_channel_is_black() {
        let dim = display_channel(0.05, 1.0);
        let bright = display_channel(0.05, 4.0);
        assert!(bright > dim, "four stops brighter is brighter: {bright} against {dim}");
        for hostile in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -1.0] {
            assert_eq!(display_channel(hostile, 1.0), 0.0, "{hostile} became a pixel");
        }
        // A broken exposure falls back to one stop rather than darkening the image to nothing.
        assert_eq!(display_channel(0.18, f32::NAN), display_channel(0.18, 1.0));
        assert_eq!(display_channel(0.18, 0.0), display_channel(0.18, 1.0));
    }

    #[test]
    fn the_transfer_function_matches_the_standard_at_its_own_knee() {
        // The two branches of the sRGB curve meet at the standard's knee (0.0031308).
        let knee = srgb(0.003_130_8);
        assert!((knee - 0.040_45).abs() < 1e-4, "{knee}");
        assert_eq!(srgb(0.0), 0.0);
        assert!((srgb(1.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn the_accumulator_is_divided_by_its_own_sample_count() {
        let accumulated = vec![[4.0, 2.0, 1.0, 0.0], [0.0, 0.0, 0.0, 0.0]];
        let image = Image::from_accumulation(2, 1, &accumulated, 4).expect("a two-pixel render");
        assert_eq!(image.radiance_at(0, 0), Some([1.0, 0.5, 0.25]));
        assert_eq!(image.radiance_at(1, 0), Some([0.0, 0.0, 0.0]));
        assert_eq!(image.radiance_at(2, 0), None, "past the last column there is no pixel");
        assert_eq!(image.radiance_at(0, 1), None, "and past the last row");

        // A pixel that retired early carries its own smaller count, and it is divided by that: the
        // same light taken by eight samples is not a quarter of it.
        let retired = vec![[4.0, 2.0, 1.0, 2.0], [8.0, 4.0, 2.0, 8.0]];
        let image = Image::from_accumulation(2, 1, &retired, 8).expect("a two-pixel render");
        assert_eq!(image.radiance_at(0, 0), Some([2.0, 1.0, 0.5]), "two samples, not eight");
        assert_eq!(image.radiance_at(1, 0), Some([1.0, 0.5, 0.25]), "eight samples, as offered");

        let short = Image::from_accumulation(2, 2, &accumulated, 4).expect_err("two pixels are not four");
        assert!(matches!(short, FinalError::Accumulation { pixels: 2, .. }), "{short}");
        let zero = Image::from_accumulation(2, 1, &accumulated, 0).expect_err("a sample count of zero divides nothing");
        assert!(matches!(zero, FinalError::Accumulation { samples: 0, .. }), "{zero}");
        assert!(Image::new(0, 10).is_err());
    }

    #[test]
    fn a_filtered_picture_keeps_its_light_and_refuses_what_is_not_a_number() {
        let radiance = vec![[0.25, 0.5, 1.0], [f32::NAN, -1.0, f32::INFINITY]];
        let image = Image::from_linear(2, 1, &radiance).expect("two pixels of a finished picture");
        assert_eq!(image.radiance_at(0, 0), Some([0.25, 0.5, 1.0]), "a finished picture is not divided again");
        assert_eq!(image.radiance_at(1, 0), Some([0.0, 0.0, 0.0]), "a broken pixel is black, not a hole");
        let short = Image::from_linear(2, 2, &radiance).expect_err("two pixels are not four");
        assert!(matches!(short, FinalError::Accumulation { pixels: 2, .. }), "{short}");
    }

    #[test]
    fn a_written_png_reads_back_with_its_size_and_its_pixels() {
        let image = image();
        let bytes = encode_png(&image, 1.0).expect("the image encodes");
        let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        let mut reader = decoder.read_info().expect("the bytes are a PNG");
        let mut buffer = vec![0u8; reader.output_buffer_size().expect("a bounded image")];
        let info = reader.next_frame(&mut buffer).expect("the frame decodes");
        assert_eq!((info.width, info.height), (2, 2));
        assert_eq!(info.color_type, png::ColorType::Rgba);
        let expected = image.to_rgba8(1.0);
        assert_eq!(buffer.get(..expected.len()), Some(expected.as_slice()), "the file carries the tone-mapped pixels");
        // The top-left pixel is black, its neighbour a mid grey and the bottom-right a hot highlight.
        assert_eq!(buffer.get(0..4), Some([0, 0, 0, 255].as_slice()));
        let middle = buffer.get(4).copied().unwrap_or(0);
        assert!((100..160).contains(&middle), "0.18 linear encoded as {middle}");
        // The bottom-right pixel is a hot orange: its red channel is past white and its blue is dark.
        let highlight = buffer.get(12..15).expect("four pixels");
        assert_eq!(highlight[0], 255, "a hot red is white in its own channel");
        assert!(highlight[2] < 220, "the blue channel keeps the colour: {highlight:?}");
    }

    #[test]
    fn a_png_written_from_ready_pixels_keeps_them_and_refuses_a_short_buffer() {
        // What the viewport preview hands over: the target's own bytes, already display-ready (J10).
        let pixels: Vec<u8> = vec![10, 20, 30, 255, 40, 50, 60, 255, 70, 80, 90, 255, 100, 110, 120, 255];
        let bytes = encode_rgba8(2, 2, &pixels).expect("ready pixels encode");
        let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        let mut reader = decoder.read_info().expect("the bytes are a PNG");
        let mut buffer = vec![0u8; reader.output_buffer_size().expect("a bounded image")];
        let info = reader.next_frame(&mut buffer).expect("the frame decodes");
        assert_eq!((info.width, info.height), (2, 2));
        assert_eq!(buffer, pixels, "no curve runs over pixels that are already display values");

        // A buffer that is not four bytes per pixel of the size is a picture that does not exist.
        assert!(matches!(encode_rgba8(2, 2, &pixels[..12]), Err(FinalError::ImageSize { .. })));
        assert!(matches!(encode_rgba8(0, 2, &[]), Err(FinalError::ImageSize { .. })));
    }

    #[test]
    fn a_write_leaves_the_old_file_or_the_new_one_and_never_a_partial() {
        let directory = scratch_dir("final-write");
        let path = directory.join("image.png");
        let bytes = encode_png(&image(), 1.0).expect("the image encodes");
        write_atomic(&path, &bytes).expect("the first write lands");
        assert_eq!(std::fs::read(&path).expect("the file is there"), bytes);

        let replacement = encode_png(&Image::new(1, 1).expect("a pixel"), 1.0).expect("the image encodes");
        write_atomic(&path, &replacement).expect("the second write replaces it");
        assert_eq!(std::fs::read(&path).expect("the file is there"), replacement);

        // A destination that cannot be written is a sentence naming it, and no `.part` is left.
        let broken = directory.join("no-such-folder").join("image.png");
        let error = write_atomic(&broken, &bytes).expect_err("a missing directory is not writable");
        assert!(error.to_string().contains("image.png"), "{error}");
        let leftovers: Vec<_> = std::fs::read_dir(&directory)
            .expect("the scratch directory")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".part"))
            .collect();
        assert!(leftovers.is_empty(), "a failed write left {leftovers:?}");
    }
}
