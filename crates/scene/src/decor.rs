//! The decor a surface is faced with: the board's picture, the size it covers and the way it runs.
//!
//! One value for both consumers. A viewport uploads the image as a texture and multiplies it into the
//! fragment's colour; the final render carries the same image into the tracer and does the same
//! arithmetic in its own shader. A material the viewport and the render disagree about is two
//! materials, and a shop that chooses a decor would see the two disagree.
//!
//! The picture is sRGB-encoded eight-bit RGBA, row-major from the top, one texel per four bytes: it is
//! what a PNG carries and what a decor scan is. The shader turns it into linear light where it samples
//! it, so the value never holds a second colour space that a reader could mistake for the first.
//!
//! The size is the board's, not the image's: a decor is a scan of a sample, and `covers` says how many
//! millimetres of board one image width spans. Texels are square, so the height follows from the
//! image's own aspect — an anisotropic decor would shear every grain it drew.
//!
//! Refuses: an image with no texel, a side of zero, a side past [`MAX_DECOR_SIDE`] (a hostile record
//! must not become an allocation), and a size that covers no board at all.

use std::fmt;

use geom::DVec2;
use units::Length;

/// The longest side one decor may have, and therefore the most texels it may hold.
///
/// A decor is a picture of a board sample: 4096 texels a side covers a 1.2 m sample at 0.3 mm per
/// texel, finer than any shop scans, and the square is 64 MB of eight-bit RGBA. The cap is what a
/// frame refuses, not what a shop may not want.
pub const MAX_DECOR_SIDE: u32 = 4096;

/// Why a decor was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DecorError {
    /// A side of zero, or an image with no texel at all.
    #[error("a decor image is {width} by {height}: every side must be at least one texel")]
    Empty {
        /// The width that was asked for.
        width: u32,
        /// The height that was asked for.
        height: u32,
    },
    /// A side past [`MAX_DECOR_SIDE`].
    #[error("a decor image is {width} by {height}: a side is longer than {MAX_DECOR_SIDE} texels")]
    TooWide {
        /// The width that was asked for.
        width: u32,
        /// The height that was asked for.
        height: u32,
    },
    /// A byte count that is not one texel per four bytes.
    #[error("a decor image of {width} by {height} carries {bytes} bytes instead of {texels} texels")]
    Length {
        /// The width that was asked for.
        width: u32,
        /// The height that was asked for.
        height: u32,
        /// How many texels the size multiplies out to.
        texels: u64,
        /// How many bytes arrived.
        bytes: usize,
    },
    /// A size that covers no board.
    #[error("a decor covers {covers:?}: it must cover more than nothing")]
    Covers {
        /// The length the record named.
        covers: Length,
    },
}

/// A stable name for one decor image, so a device cache uploads it once and not once per frame.
///
/// The number belongs to whoever built the frame — the shell, for a material that names a file — and
/// two frames that name the same decor file with one number are describing the same texture. It is
/// never derived from a pointer: a cache keyed by address would re-upload a decor every time a frame
/// was rebuilt.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DecorId(u64);

impl DecorId {
    /// The decor `value`, or `None` for zero, which no decor is.
    pub const fn new(value: u64) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    /// The number itself.
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for DecorId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "decor {}", self.0)
    }
}

/// Quarter turns the picture is turned by around the surface's normal, `0..=3`.
///
/// A decor has a grain, and a piece of board is cut along it or across it. The document does not store
/// a grain direction yet (`PROJECT.md`, "Grain"), so the turn arrives with the material and the shell
/// decides what a record says; a tracer never reads a document.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Turns(u8);

impl Turns {
    /// The picture as its own file has it: its width runs along the surface's local `+X`.
    pub const ALONG: Self = Self(0);
    /// A quarter turn: the picture's width — its grain — runs along the surface's local `+Y`.
    pub const ACROSS: Self = Self(1);

    /// `turns` quarter turns, counted modulo four, so a hostile count is a repaired decor and never a
    /// refused frame.
    pub const fn new(turns: u8) -> Self {
        Self(turns % 4)
    }

    /// How many quarter turns.
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl Default for Turns {
    fn default() -> Self {
        Self::ALONG
    }
}

/// The texels of one decor image, eight-bit sRGB RGBA, row-major from the top.
#[derive(Clone, Debug)]
pub struct DecorImage {
    width: u32,
    height: u32,
    texels: Vec<[u8; 4]>,
}

impl DecorImage {
    /// The image of `width` by `height` texels, one entry per texel.
    ///
    /// The count is checked before the buffer is believed, so a size that does not multiply out is a
    /// refusal naming both numbers rather than a shifted picture.
    pub fn new(width: u32, height: u32, texels: Vec<[u8; 4]>) -> Result<Self, DecorError> {
        if width == 0 || height == 0 {
            return Err(DecorError::Empty { width, height });
        }
        if width > MAX_DECOR_SIDE || height > MAX_DECOR_SIDE {
            return Err(DecorError::TooWide { width, height });
        }
        let expected = u64::from(width) * u64::from(height);
        if usize::try_from(expected) != Ok(texels.len()) {
            return Err(DecorError::Length { width, height, texels: expected, bytes: texels.len() });
        }
        Ok(Self { width, height, texels })
    }

    /// An image read from the bytes a PNG decodes to: eight-bit RGBA, row-major from the top.
    pub fn from_rgba8(width: u32, height: u32, bytes: &[u8]) -> Result<Self, DecorError> {
        let expected = u64::from(width) * u64::from(height);
        if usize::try_from(expected.saturating_mul(4)).ok() != Some(bytes.len()) {
            return Err(DecorError::Length { width, height, texels: expected, bytes: bytes.len() });
        }
        let (rows, _) = bytes.as_chunks::<4>();
        let texels = rows.to_vec();
        Self::new(width, height, texels)
    }

    /// The image's width in texels.
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// The image's height in texels.
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Every texel, row-major from the top.
    pub fn texels(&self) -> &[[u8; 4]] {
        &self.texels
    }

    /// The texel at `x`, `y`, or `None` outside the image.
    pub fn texel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let index = usize::try_from(u64::from(y) * u64::from(self.width) + u64::from(x)).ok()?;
        self.texels.get(index).copied()
    }
}

/// One decor: the picture, the board it covers and the way its grain runs.
///
/// Two decors are the same decor when they are the same record — the id, the size and the turns — and
/// not when their bytes match: a frame asks whether it changed, and comparing a megabyte of texels to
/// answer that would be the frame's cost instead of the decor's.
#[derive(Clone, Debug)]
pub struct Decor {
    id: DecorId,
    image: std::sync::Arc<DecorImage>,
    covers: Length,
    turns: Turns,
}

impl Decor {
    /// The decor `id` of `image`, repeating every `covers` millimetres, turned by `turns`.
    pub fn new(
        id: DecorId,
        image: std::sync::Arc<DecorImage>,
        covers: Length,
        turns: Turns,
    ) -> Result<Self, DecorError> {
        if covers.millimetres_f64() <= 0.0 {
            return Err(DecorError::Covers { covers });
        }
        Ok(Self { id, image, covers, turns })
    }

    /// The decor's stable name.
    pub const fn id(&self) -> DecorId {
        self.id
    }

    /// The picture's texels.
    pub fn image(&self) -> &DecorImage {
        &self.image
    }

    /// How many millimetres of board one image width spans.
    pub const fn covers(&self) -> Length {
        self.covers
    }

    /// The quarter turns the picture is turned by.
    pub const fn turns(&self) -> Turns {
        self.turns
    }

    /// The texture coordinate of `local`, a point in the drawable's own frame in millimetres.
    ///
    /// The picture is projected flat onto the drawable's own `XY` plane, which is the panel's frame
    /// (`ADR 0004`: the zero corner at the minimum, faces `A` and `B` at `z = thickness` and `z = 0`).
    /// Texels are square: `covers` spans the image's width, and its height follows from the aspect, so
    /// a grain drawn in the picture keeps its proportions on the board.
    ///
    /// A coordinate past the picture is not clamped: the renderer's sampler repeats the image, which
    /// is what a board's decor does — a sample is a window onto a repeating print.
    pub fn uv(&self, local: DVec2) -> [f32; 2] {
        let width = f64::from(self.image.width());
        let height = f64::from(self.image.height());
        let covered = self.covers.millimetres_f64();
        let across = covered;
        let down = covered * height / width;
        let (x, y) = match self.turns.get() {
            1 => (local.y, -local.x),
            2 => (-local.x, -local.y),
            3 => (-local.y, local.x),
            _ => (local.x, local.y),
        };
        if across <= 0.0 || down <= 0.0 {
            return [0.0, 0.0];
        }
        [(x / across) as f32, (y / down) as f32]
    }
}

impl PartialEq for Decor {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.covers == other.covers && self.turns == other.turns
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(width: u32, height: u32) -> std::sync::Arc<DecorImage> {
        let texels = vec![[255, 0, 0, 255]; usize::try_from(width * height).unwrap_or(0)];
        std::sync::Arc::new(DecorImage::new(width, height, texels).expect("a small image"))
    }

    fn decor(turns: Turns) -> Decor {
        let id = DecorId::new(7).expect("a non-zero decor");
        let covers = Length::from_millimetres(600).expect("600 mm");
        // Eight by four texels: the image is twice as wide as it is tall.
        Decor::new(id, image(8, 4), covers, turns).expect("a decor")
    }

    #[test]
    fn a_decor_covers_the_board_size_it_names_with_square_texels() {
        let decor = decor(Turns::ALONG);
        // 600 mm across the image's eight texels is 75 mm per texel; its four rows cover 300 mm.
        assert_eq!(decor.uv(DVec2::new(0.0, 0.0)), [0.0, 0.0]);
        assert_eq!(decor.uv(DVec2::new(300.0, 0.0)), [0.5, 0.0]);
        assert_eq!(decor.uv(DVec2::new(0.0, 150.0)), [0.0, 0.5]);
        // One whole repeat is one whole picture in both directions.
        assert_eq!(decor.uv(DVec2::new(600.0, 300.0)), [1.0, 1.0]);
        // Texels stay square: 600 mm of board is two pictures tall, not one.
        assert_eq!(decor.uv(DVec2::new(0.0, 600.0)), [0.0, 2.0]);
    }

    #[test]
    fn a_turned_decor_runs_its_grain_the_other_way() {
        let along = decor(Turns::ALONG);
        let across = decor(Turns::ACROSS);
        // The grain is the picture's own width; a point along the board's length is a point along the
        // grain until the decor is turned, and across it afterwards.
        assert_eq!(along.uv(DVec2::new(300.0, 0.0)), [0.5, 0.0]);
        // Turned once, the picture's 600 mm width runs along the board's height and its 300 mm height
        // along the board's length, so the same point is one whole picture down the grain.
        assert_eq!(across.uv(DVec2::new(300.0, 0.0)), [0.0, -1.0]);
        assert_eq!(across.uv(DVec2::new(0.0, 150.0)), [0.25, 0.0]);
        // Two quarter turns are the picture upside down, and three are the mirror of one.
        assert_eq!(decor(Turns::new(2)).uv(DVec2::new(300.0, 150.0)), [-0.5, -0.5]);
        assert_eq!(decor(Turns::new(3)).uv(DVec2::new(300.0, 150.0)), [-0.25, 1.0]);
        assert_eq!(Turns::new(5), Turns::ACROSS, "a count is a count modulo four");
    }

    #[test]
    fn an_image_is_refused_before_it_is_believed() {
        assert_eq!(DecorImage::new(0, 4, Vec::new()).err(), Some(DecorError::Empty { width: 0, height: 4 }));
        assert_eq!(
            DecorImage::new(MAX_DECOR_SIDE + 1, 1, Vec::new()).err(),
            Some(DecorError::TooWide { width: MAX_DECOR_SIDE + 1, height: 1 })
        );
        assert_eq!(
            DecorImage::new(MAX_DECOR_SIDE, MAX_DECOR_SIDE + 1, Vec::new()).err(),
            Some(DecorError::TooWide { width: MAX_DECOR_SIDE, height: MAX_DECOR_SIDE + 1 })
        );
        // A size that multiplies out past its buffer is a refusal naming both, not a shifted picture.
        let short = DecorImage::new(4, 4, vec![[0, 0, 0, 255]; 15]);
        assert_eq!(short.err(), Some(DecorError::Length { width: 4, height: 4, texels: 16, bytes: 15 }));
    }

    #[test]
    fn an_image_reads_from_the_bytes_a_png_decodes_to() {
        let bytes = [1u8, 2, 3, 255, 4, 5, 6, 255];
        let image = DecorImage::from_rgba8(2, 1, &bytes).expect("two texels");
        assert_eq!(image.texel(0, 0), Some([1, 2, 3, 255]));
        assert_eq!(image.texel(1, 0), Some([4, 5, 6, 255]));
        assert_eq!(image.texel(2, 0), None);
        // Three bytes per texel is not what a decoder hands over.
        assert!(DecorImage::from_rgba8(2, 1, &bytes[..7]).is_err());
    }

    #[test]
    fn a_decor_that_covers_nothing_is_refused() {
        let id = DecorId::new(1).expect("one");
        let covers = Length::from_millimetres(0).expect("zero millimetres");
        assert_eq!(Decor::new(id, image(2, 2), covers, Turns::ALONG).err(), Some(DecorError::Covers { covers }));
        assert_eq!(DecorId::new(0), None, "no decor is number zero");
    }

    #[test]
    fn two_decors_are_one_decor_when_they_are_one_record() {
        let id = DecorId::new(9).expect("nine");
        let covers = Length::from_millimetres(1200).expect("1200 mm");
        let first = Decor::new(id, image(4, 4), covers, Turns::ALONG).expect("a decor");
        let second = Decor::new(id, image(2, 2), covers, Turns::ALONG).expect("a decor");
        assert_eq!(first, second, "the id names the decor, not the bytes");
        let turned = Decor::new(id, image(4, 4), covers, Turns::ACROSS).expect("a decor");
        assert_ne!(first, turned, "a turn is a different decor");
    }
}
