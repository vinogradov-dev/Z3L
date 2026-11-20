//! The frame tool's draft: the base the sketch drew and the height pulled from it.
//!
//! A product frame reserves a prism, and it is drawn the way the extrusion works: the base on the
//! active work plane, fixed, then the height pulled along the plane's normal (ADR 0101). The base is
//! the sketch's own figure — the frame cell arms the rectangle tool the box cell arms, so the plates,
//! the typed width and depth and the preview are one tool for both owners (N19) — and a ring drawn
//! with the free-outline method is the base when the piece is not a box (ADR 0159). This draft is
//! what the finished figure becomes: view state until its one commit, and nothing reaches the
//! document before `commit_box` dispatches `product.create`. Escape costs nothing because nothing was
//! written. A solid box is not drawn here: it is a profile swept on Z (ADR 0138).
//!
//! A rectangle names the frame's own frame: its minimum corner in the plane becomes the node's
//! origin, its sides become width and depth, and the normal becomes the height, so the space the
//! frame occupies is `[0..width] × [0..depth] × [0..height]` in its own frame — the anchor a panel
//! has and a product reserves (ADR 0083, ADR 0101). A drawn ring is stored as drawn and the envelope
//! anchors it by its own rule, so the frame's zero is the ring's minimum corner (ADR 0159).
//!
//! [ADR 0083]: ../docs/decisions/0083-a-panel-is-anchored-at-its-base-corner.md
//! [ADR 0101]: ../docs/decisions/0101-a-frame-and-a-box-are-one-tool.md
//! [ADR 0138]: ../docs/decisions/0138-a-solid-body-is-a-drawn-profile-swept-on-z.md
//! [ADR 0159]: ../docs/decisions/0159-a-product-frame-reserves-a-footprint.md

use thiserror::Error;
use units::Length;

use model::{Contour, Envelope, EnvelopeError};

/// The smallest height a pull leaves, in millimetres.
pub const MIN_BOX_HEIGHT_MM: i64 = 1;

/// The height a fresh pull holds, in micrometres: the smallest one, until the pointer says more.
///
/// A pull that starts at zero would show a number below its own range and commit nothing; it starts
/// at the floor the gesture already has, exactly where a pull that never left the plane would stop.
const START_HEIGHT: Length = Length::from_micrometres(MIN_BOX_HEIGHT_MM * 1_000);

/// A rejected step of the box gesture.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum BoxDraftError {
    /// The height is not a positive length.
    #[error("the height must be positive")]
    FlatHeight,
    /// No frame is being drawn, so there is no height to set.
    #[error("no frame is being drawn")]
    NotStarted,
}

/// The fixed base rectangle the sketch drew: its minimum corner and its two sides.
///
/// The corner and the sides are in work-plane millimetres, and the corner is the frame's own zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoxBase {
    /// The corner the frame's origin sits on, in work-plane millimetres.
    pub corner: [Length; 2],
    /// The side along the plane's first axis.
    pub width: Length,
    /// The side along the plane's second axis.
    pub depth: Length,
}

/// A base that is a ring drawn on the plane, anchored at its own minimum corner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RingBase {
    /// The corner of the ring's bounds, in work-plane millimetres: the frame's own zero.
    pub corner: [Length; 2],
    /// The closed ring, in work-plane millimetres, starting at `corner`.
    pub contour: Contour,
}

/// The base the height is pulled from: the rectangle the sketch drew, or the ring it drew.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Base {
    /// A figure drawn between two corners on the plane's own axes: the plain box.
    Rectangle(BoxBase),
    /// Any other closed figure, kept as the ring it was drawn as.
    Ring(RingBase),
}

/// The base a frame's height is pulled from and the height itself.
///
/// The base arrives from the drawing rather than from clicks of this tool's own: the frame cell arms
/// the sketch's figure and the finished one fixes the base, so one gesture draws the frame's reserved
/// space and a solid's profile (N19, ADR 0101, ADR 0138).
#[derive(Clone, Debug, PartialEq)]
pub struct BoxDraft {
    /// The fixed base, whichever figure drew it.
    base: Base,
    /// The height the pull currently shows.
    height: Length,
}

impl BoxDraft {
    /// A draft whose base is the rectangle the sketch drew, waiting for its height.
    ///
    /// A rectangle on the plane's own axes is the box the card writes: three sides and no footprint,
    /// so a drawn frame and a typed one are the same document (ADR 0101). The height starts at the
    /// pull's floor, so the number the plate shows is always inside its own range.
    pub const fn from_rectangle(base: BoxBase) -> Self {
        Self { base: Base::Rectangle(base), height: START_HEIGHT }
    }

    /// A draft whose base is a ring already drawn on the plane, waiting for its height.
    ///
    /// The ring is stored as drawn; [`BoxDraft::envelope`] anchors it through the envelope's own rule,
    /// so the frame's zero is the ring's minimum corner and the sides are its bounds (ADR 0159).
    pub const fn from_ring(corner: [Length; 2], contour: Contour) -> Self {
        Self { base: Base::Ring(RingBase { corner, contour }), height: START_HEIGHT }
    }

    /// The drawn ring, once its outline is finished; `None` when the base is a rectangle.
    pub const fn ring(&self) -> Option<&RingBase> {
        match &self.base {
            Base::Ring(ring) => Some(ring),
            Base::Rectangle(_) => None,
        }
    }

    /// The corner of the base on the work plane, whichever base the draft holds.
    ///
    /// It is the frame's own zero, so the shell projects the pointer from it when the height is pulled
    /// (ADR 0082, ADR 0101).
    pub const fn base_corner(&self) -> [Length; 2] {
        match &self.base {
            Base::Rectangle(base) => base.corner,
            Base::Ring(ring) => ring.corner,
        }
    }

    /// The envelope the commit writes.
    ///
    /// A rectangle base is three sides and a ring base is its ring and the height, so the two methods
    /// of the tool reach one command with one value and neither invents a shape (ADR 0159).
    pub fn envelope(&self) -> Result<Envelope, EnvelopeError> {
        match &self.base {
            Base::Rectangle(base) => Envelope::new(base.width, base.depth, self.height),
            Base::Ring(ring) => Envelope::with_ring(&ring.contour, self.height),
        }
    }

    /// The height the pull shows.
    pub const fn height(&self) -> Length {
        self.height
    }

    /// Sets the height the pull shows, rounded to whole millimetres and never below one.
    ///
    /// The step is the gesture's own: a pull moves in millimetres and the document keeps
    /// micrometres, so the draft rounds once, here, instead of letting a float reach the command.
    pub fn set_height(&mut self, height: Length) -> Result<(), BoxDraftError> {
        if height <= Length::ZERO {
            return Err(BoxDraftError::FlatHeight);
        }
        let whole = height.millimetres_f64().round() as i64;
        self.height = Length::from_millimetres(whole.max(MIN_BOX_HEIGHT_MM)).ok_or(BoxDraftError::FlatHeight)?;
        Ok(())
    }

    /// The three sides the committed box will have: width, depth and height.
    ///
    /// A drawn ring has no three sides of its own; [`BoxDraft::envelope`] is what it commits, and the
    /// shell labels a rectangle's pull with this.
    pub fn sizes(&self) -> Option<[Length; 3]> {
        let Base::Rectangle(base) = &self.base else {
            return None;
        };
        Some([base.width, base.depth, self.height])
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use testkit::millimetres;

    use super::*;

    fn rectangle(width: i64, depth: i64) -> BoxBase {
        BoxBase { corner: [millimetres(100), millimetres(50)], width: millimetres(width), depth: millimetres(depth) }
    }

    #[test]
    fn a_drawn_rectangle_is_the_plain_box_the_card_writes() {
        let mut draft = BoxDraft::from_rectangle(rectangle(600, 400));
        assert_eq!(draft.base_corner(), [millimetres(100), millimetres(50)], "the pull starts at its corner");
        assert_eq!(draft.ring(), None, "a rectangle keeps no ring");
        draft.set_height(millimetres(720)).expect("the pull");
        let envelope = draft.envelope().expect("a valid rectangle");
        assert!(envelope.footprint().is_rectangle(), "a drawn rectangle reserves a plain box, like the card's");
        assert_eq!(envelope.components(), [millimetres(600), millimetres(400), millimetres(720)]);
    }

    #[test]
    fn a_drawn_ring_is_the_base_and_the_envelope_carries_it() {
        let ring = model::Path::polyline(
            vec![
                [millimetres(100), millimetres(50)],
                [millimetres(700), millimetres(50)],
                [millimetres(700), millimetres(450)],
                [millimetres(100), millimetres(450)],
            ],
            true,
        )
        .expect("a closed ring")
        .to_contour()
        .expect("a contour");
        let mut draft = BoxDraft::from_ring([millimetres(100), millimetres(50)], ring);
        assert_eq!(draft.sizes(), None, "a drawn ring has no three sides of its own");
        assert_eq!(draft.base_corner(), [millimetres(100), millimetres(50)], "the pull starts at its corner");

        draft.set_height(millimetres(720)).expect("the pull");
        let envelope = draft.envelope().expect("a valid ring");
        assert!(envelope.footprint().is_ring(), "the envelope keeps the ring");
        assert_eq!(
            envelope.components(),
            [millimetres(600), millimetres(400), millimetres(720)],
            "the sides are the ring's bounds and the height is the pull's"
        );
    }

    #[test]
    fn the_height_rounds_to_whole_millimetres_and_never_reaches_zero() {
        let mut draft = BoxDraft::from_rectangle(rectangle(600, 400));
        assert_eq!(
            draft.sizes(),
            Some([millimetres(600), millimetres(400), millimetres(MIN_BOX_HEIGHT_MM)]),
            "a fresh pull holds its floor"
        );
        draft.set_height(Length::from_micrometres(719_600)).expect("a pulled height");
        assert_eq!(draft.height(), millimetres(720), "the pull rounds to the millimetre step");
        draft.set_height(Length::from_micrometres(-5_000)).expect_err("a negative pull is refused");
        assert_eq!(draft.height(), millimetres(720), "a refused pull keeps the last height");
        draft.set_height(Length::ZERO).expect_err("zero is not a solid");
        draft.set_height(Length::from_micrometres(1)).expect("one micrometre pulls up to one millimetre");
        assert_eq!(draft.height(), millimetres(1));
    }
}
