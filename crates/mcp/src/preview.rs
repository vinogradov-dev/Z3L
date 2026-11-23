//! What the agent sees of the scene: one picture, drawn by the shell and handed over as bytes.
//!
//! A tool that returns an image needs a device, and this crate is forbidden one — `mcp` may not
//! depend on `wgpu`, `egui` or `eframe` (ADR 0186). So the device stays where it already is, in the
//! shell, and this module holds the port: [`Previewer`] is implemented by the composition root
//! (`app`), which evaluates the session's scene, fits the view, renders it with `render` and encodes
//! one PNG. The tool asks for a picture it is handed, exactly the way a save asks for a
//! [`crate::SceneSink`] it is handed.
//!
//! Two bounds keep a picture from becoming a download nobody asked for, and both refuse rather than
//! clamp: a side outside [`MIN_SIDE`]..=[`MAX_SIDE`] is a request that names no size this program
//! draws, and an encoded picture over [`MAX_BYTES`] is one a client should not have to read. A clamp
//! would answer a different picture than the call asked for, with nothing saying so.
//!
//! Changes when: the bounds move, or the shape the shell is asked for changes.

use std::fmt;

use engine::Session;

/// The smallest side of a preview, in pixels.
///
/// A 32-pixel cabinet is four dark rectangles: below this the picture is a mistake rather than a
/// small look at the work.
pub const MIN_SIDE: u32 = 64;

/// The largest side of a preview, in pixels.
///
/// A preview is what a client *looks* at, not what it prints: 1600 is the longest side of a full
/// screen's viewport on a 1.5× display, and 1600 × 1600 RGBA is 10 MB before the encoder.
pub const MAX_SIDE: u32 = 1600;

/// The most a preview may weigh once encoded: 4 MiB.
///
/// It is measured on the PNG and not on the pixels, because the answer's size is what a client pays
/// for: base64 adds a third on top, and a picture past this is a scene the caller should query
/// rather than look at.
pub const MAX_BYTES: usize = 4 * 1024 * 1024;

/// The size of one preview, in pixels, already checked against the two bounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreviewSize {
    /// The picture's width, between [`MIN_SIDE`] and [`MAX_SIDE`].
    pub width: u32,
    /// The picture's height, between [`MIN_SIDE`] and [`MAX_SIDE`].
    pub height: u32,
}

impl PreviewSize {
    /// The size a request names, or the sentence that says which side is outside the bounds.
    ///
    /// The sentence names the side and the bound, so a caller that asked for a wall poster learns
    /// what to ask for instead of receiving a smaller picture than it asked for.
    pub fn new(width: u32, height: u32) -> Result<Self, String> {
        for (side, value) in [("width", width), ("height", height)] {
            if !(MIN_SIDE..=MAX_SIDE).contains(&value) {
                return Err(format!(
                    "a preview's {side} is {value} pixels; it must be between {MIN_SIDE} and {MAX_SIDE}"
                ));
            }
        }
        Ok(Self { width, height })
    }
}

/// Draws one picture of the session's scene.
///
/// `Debug` and `Send + Sync` for the reason [`crate::SceneSink`] is: the tool server holds it behind
/// an `Arc` and the protocol may call from any thread. The answer is the bytes of one PNG, or the
/// sentence that says why there are none — a scene that cannot be evaluated, a device that went
/// away, an encoder that refused.
///
/// The picture is framed by the document itself and never by a viewport: a headless process has no
/// window, so "what the person is looking at" is not a rule both doors can keep.
pub trait Previewer: fmt::Debug + Send + Sync {
    /// Draws the scene of `session` at `size` and answers its PNG.
    fn preview(&self, session: &Session, size: PreviewSize) -> Result<Vec<u8>, String>;
}

/// The bytes of `data` in base64, standard alphabet, padded.
///
/// Twenty lines instead of a dependency (`AGENTS.md` §4.8): the protocol wants the image data as a
/// base64 string, and the tree has no other reader of the table.
pub(crate) fn base64(data: &[u8]) -> String {
    const DIGITS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut text = String::with_capacity(data.len().div_ceil(3).saturating_mul(4));
    for chunk in data.chunks(3) {
        let first = chunk.first().copied().unwrap_or(0);
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        let triple = (u32::from(first) << 16) | (u32::from(second) << 8) | u32::from(third);
        let digit = |shift: u32| char::from(DIGITS.get(((triple >> shift) & 0x3f) as usize).copied().unwrap_or(b'A'));
        // One byte carries two characters and two pad marks, two bytes three and one, three bytes
        // all four.
        text.push(digit(18));
        text.push(digit(12));
        match chunk.len() {
            1 => text.push_str("=="),
            2 => {
                text.push(digit(6));
                text.push('=');
            }
            _ => {
                text.push(digit(6));
                text.push(digit(0));
            }
        }
    }
    text
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    #[test]
    fn a_size_outside_the_bounds_names_the_side_and_the_bound() {
        assert_eq!(PreviewSize::new(800, 600), Ok(PreviewSize { width: 800, height: 600 }));
        assert_eq!(PreviewSize::new(MIN_SIDE, MAX_SIDE), Ok(PreviewSize { width: MIN_SIDE, height: MAX_SIDE }));
        let small = PreviewSize::new(MIN_SIDE - 1, 600).expect_err("half a side below the floor");
        assert!(small.contains("width") && small.contains(&MIN_SIDE.to_string()), "{small}");
        let large = PreviewSize::new(800, MAX_SIDE + 1).expect_err("a wall poster");
        assert!(large.contains("height") && large.contains(&MAX_SIDE.to_string()), "{large}");
        assert!(PreviewSize::new(0, 0).is_err(), "an empty picture is not a small one");
    }

    #[test]
    fn base64_matches_the_alphabet_and_pads_a_short_tail() {
        // The published test vectors: one byte, two bytes, three bytes, and nothing.
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        // Every byte value survives the round trip's shape: four characters per three bytes.
        let bytes: Vec<u8> = (0..=255u8).collect();
        let text = base64(&bytes);
        assert_eq!(text.len(), bytes.len().div_ceil(3) * 4);
        assert!(text.chars().all(|character| character.is_ascii_alphanumeric() || "+/=".contains(character)));
    }
}
