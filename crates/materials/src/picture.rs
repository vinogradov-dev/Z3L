//! The picture a decor is faced with: one PNG file beside the library, read as eight-bit RGBA.
//!
//! A decor is a scan of a board sample, so it is a file a person saves and replaces; the library
//! names it by a relative path and this crate reads it, without a hash pinning the version the way a
//! part document is pinned (ADR 0149): a decor is replaced and not versioned, and the frame reads
//! the new picture on the next build.
//!
//! The texels are what [`scene::DecorImage`] carries — sRGB-encoded eight-bit RGBA, row-major from
//! the top — so the shell hands the picture to the frame without a second encoding. The file's own
//! format is PNG and nothing else: it is the one picture format already in the tree, it is lossless
//! (a decor's grain is the data), and a camera's JPEG would put its own edges into a board.
//!
//! Refuses: a file that cannot be read, a file that is not a PNG, and a picture with no texel or a
//! side past [`MAX_PICTURE_SIDE`] — a hostile file must not become an allocation.
//!
//! [`scene::DecorImage`]: https://docs.rs/scene

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

/// The longest side one picture may have, and therefore the most texels it may hold.
///
/// The same cap the frame's decor carries: 4096 texels a side covers a 1.2 m sample at 0.3 mm per
/// texel, finer than any shop scans, and the square is 64 MB of eight-bit RGBA.
pub const MAX_PICTURE_SIDE: u32 = 4096;

/// Why a decor's picture could not be read.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PictureError {
    /// The file could not be opened or read.
    #[error("cannot read the decor picture {path}: {reason}")]
    Read {
        /// The picture the record named, as the library's folder resolves it.
        path: PathBuf,
        /// The reason the operating system gave.
        reason: String,
    },
    /// The file is not a picture this build reads.
    #[error("the decor picture {path} is not a PNG this build reads: {reason}")]
    Format {
        /// The picture the record named.
        path: PathBuf,
        /// What the decoder said.
        reason: String,
    },
    /// A side of zero, or a picture with no texel at all.
    #[error("the decor picture {path} is {width} by {height}: every side must be at least one texel")]
    Empty {
        /// The picture the record named.
        path: PathBuf,
        /// The width the file declares.
        width: u32,
        /// The height the file declares.
        height: u32,
    },
    /// A side past [`MAX_PICTURE_SIDE`].
    #[error("the decor picture {path} is {width} by {height}: a side is longer than {MAX_PICTURE_SIDE} texels")]
    TooWide {
        /// The picture the record named.
        path: PathBuf,
        /// The width the file declares.
        width: u32,
        /// The height the file declares.
        height: u32,
    },
    /// A decoded buffer that is not one texel per four bytes.
    #[error("the decor picture {path} decoded {bytes} bytes for {width} by {height} texels")]
    Length {
        /// The picture the record named.
        path: PathBuf,
        /// The width the file declares.
        width: u32,
        /// The height the file declares.
        height: u32,
        /// How many bytes arrived.
        bytes: usize,
    },
}

/// The texels of one decor picture, eight-bit sRGB RGBA, row-major from the top.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picture {
    width: u32,
    height: u32,
    rgba8: Vec<u8>,
}

impl Picture {
    /// Reads the PNG at `path`.
    ///
    /// The decoder is told to normalise every input to eight-bit colour and to expand a grey or a
    /// paletted image to RGB/RGBA, so a shop's scan arrives in one layout whatever it was saved as.
    /// The size is read from the header and checked before the buffer is allocated.
    pub fn read(path: &Path) -> Result<Self, PictureError> {
        let file = File::open(path)
            .map_err(|error| PictureError::Read { path: path.to_path_buf(), reason: error.to_string() })?;
        let mut decoder = png::Decoder::new(BufReader::new(file));
        decoder.set_transformations(png::Transformations::normalize_to_color8() | png::Transformations::ALPHA);
        let mut reader = decoder
            .read_info()
            .map_err(|error| PictureError::Format { path: path.to_path_buf(), reason: error.to_string() })?;
        let (width, height) = {
            let info = reader.info();
            (info.width, info.height)
        };
        if width == 0 || height == 0 {
            return Err(PictureError::Empty { path: path.to_path_buf(), width, height });
        }
        if width > MAX_PICTURE_SIDE || height > MAX_PICTURE_SIDE {
            return Err(PictureError::TooWide { path: path.to_path_buf(), width, height });
        }
        let size = reader.output_buffer_size().ok_or_else(|| PictureError::Format {
            path: path.to_path_buf(),
            reason: "the picture's pixel data does not multiply out".to_owned(),
        })?;
        let mut buffer = vec![0u8; size];
        let frame = reader
            .next_frame(&mut buffer)
            .map_err(|error| PictureError::Format { path: path.to_path_buf(), reason: error.to_string() })?;
        let rgba8 = match frame.color_type {
            png::ColorType::Rgba => buffer,
            // A PNG without an alpha channel is an opaque decor; the shader reads four channels.
            png::ColorType::Rgb => {
                let mut rgba = Vec::with_capacity(buffer.len() / 3 * 4);
                for texel in buffer.as_chunks::<3>().0 {
                    rgba.extend_from_slice(texel);
                    rgba.push(u8::MAX);
                }
                rgba
            }
            // A scan saved as grey is a decor like any other; its one channel is its three.
            png::ColorType::Grayscale => {
                let mut rgba = Vec::with_capacity(buffer.len() * 4);
                for value in buffer {
                    rgba.extend_from_slice(&[value, value, value, u8::MAX]);
                }
                rgba
            }
            png::ColorType::GrayscaleAlpha => {
                let mut rgba = Vec::with_capacity(buffer.len() * 2);
                for [value, alpha] in buffer.as_chunks::<2>().0.iter().copied() {
                    rgba.extend_from_slice(&[value, value, value, alpha]);
                }
                rgba
            }
            other => {
                return Err(PictureError::Format {
                    path: path.to_path_buf(),
                    reason: format!("{other:?} is not a picture a decor can be faced with"),
                });
            }
        };
        let expected =
            usize::try_from(u64::from(width) * u64::from(height)).ok().and_then(|texels| texels.checked_mul(4));
        if expected != Some(rgba8.len()) {
            return Err(PictureError::Length { path: path.to_path_buf(), width, height, bytes: rgba8.len() });
        }
        Ok(Self { width, height, rgba8 })
    }

    /// The picture's width in texels.
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// The picture's height in texels.
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// The texels, four bytes each, row-major from the top.
    pub fn rgba8(&self) -> &[u8] {
        &self.rgba8
    }

    /// The texel at `x`, `y`, or `None` outside the picture.
    pub fn texel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let index = usize::try_from(u64::from(y) * u64::from(self.width) + u64::from(x)).ok()?.checked_mul(4)?;
        let texel = self.rgba8.get(index..index.checked_add(4)?)?;
        let texel: [u8; 4] = texel.try_into().ok()?;
        Some(texel)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use std::path::PathBuf;

    use super::*;

    /// Writes `pixels` as a PNG at `path`, one entry per texel, RGBA8.
    fn write_png(path: &Path, width: u32, height: u32, pixels: &[[u8; 4]]) {
        let file = File::create(path).expect("a fresh file");
        let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let bytes: Vec<u8> = pixels.iter().flatten().copied().collect();
        let mut writer = encoder.write_header().expect("a PNG header");
        writer.write_image_data(&bytes).expect("the pixels");
    }

    fn scratch(name: &str) -> PathBuf {
        testkit::scratch_dir(&format!("materials-picture-{name}"))
    }

    #[test]
    fn a_picture_reads_back_the_texels_it_was_written_with() {
        let path = scratch("round-trip").join("oak.png");
        let pixels = [[255, 0, 0, 255], [0, 0, 255, 255], [0, 255, 0, 128], [9, 9, 9, 9]];
        write_png(&path, 2, 2, &pixels);
        let picture = Picture::read(&path).expect("the file is a PNG");
        assert_eq!((picture.width(), picture.height()), (2, 2));
        assert_eq!(picture.texel(0, 0), Some(pixels[0]));
        assert_eq!(picture.texel(1, 1), Some(pixels[3]), "row-major from the top");
        assert_eq!(picture.texel(2, 0), None, "outside the picture is none, not a panic");
        assert_eq!(picture.rgba8().len(), 16);
    }

    #[test]
    fn a_missing_file_and_a_file_that_is_not_a_png_are_readable_refusals() {
        let folder = scratch("broken");
        let missing = Picture::read(&folder.join("absent.png")).expect_err("a missing picture");
        assert!(missing.to_string().contains("absent.png"), "{missing}");
        let text = folder.join("notes.png");
        std::fs::write(&text, "this is not a picture").expect("write");
        let not_png = Picture::read(&text).expect_err("not a PNG");
        assert!(not_png.to_string().contains("notes.png") && not_png.to_string().contains("PNG"), "{not_png}");
    }

    #[test]
    fn a_picture_past_the_cap_is_refused_before_it_is_allocated() {
        let path = scratch("wide").join("wide.png");
        let pixels = vec![[0u8, 0, 0, 255]; usize::try_from(MAX_PICTURE_SIDE + 1).expect("a small side")];
        write_png(&path, MAX_PICTURE_SIDE + 1, 1, &pixels);
        let error = Picture::read(&path).expect_err("a side past the cap");
        assert_eq!(error, PictureError::TooWide { path: path.clone(), width: MAX_PICTURE_SIDE + 1, height: 1 });
        assert!(error.to_string().contains("4096"), "the sentence names the cap: {error}");
    }
}
