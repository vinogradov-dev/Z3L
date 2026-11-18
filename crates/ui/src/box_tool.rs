//! The frame tool's draft: a base on the work plane and the height pulled from it.
//!
//! A product frame reserves a prism, and it is drawn the way the extrusion works: the base on the
//! active work plane, fixed, then the height pulled along the plane's normal (ADR 0101). The draft is
//! view state until its one commit — the two base corners or the drawn ring, in work-plane
//! millimetres, and the pulled height — and nothing reaches the document until `commit_box`
//! dispatches `product.create`. Escape costs nothing because nothing was written. A solid box is not
//! drawn here: it is a profile swept on Z (ADR 0138).
//!
//! The base is either the rectangle two clicks fix or a closed ring drawn on the plane, which is the
//! footprint a frame reserves when the piece is not a box (ADR 0159): the two bases share the draft,
//! the height pull and the one command, so a corner cabinet and an end table are one gesture.
//!
//! The base rectangle names the frame's own frame: its minimum corner in the plane becomes the node's
//! origin, its sides become width and depth, and the normal becomes the height, so the space the
//! frame occupies is `[0..width] × [0..depth] × [0..height]` in its own frame — the anchor a panel
//! has and a product reserves (ADR 0083, ADR 0101).
//!
//! Refuses a second corner that makes a flat rectangle: a frame needs three positive sides, so a
//! degenerate base leaves the draft waiting for a corner that encloses space.
//!
//! [ADR 0083]: ../docs/decisions/0083-a-panel-is-anchored-at-its-base-corner.md
//! [ADR 0101]: ../docs/decisions/0101-a-frame-and-a-box-are-one-tool.md
//! [ADR 0138]: ../docs/decisions/0138-a-solid-body-is-a-drawn-profile-swept-on-z.md

use thiserror::Error;
use units::Length;

use model::{Contour, Envelope, EnvelopeError};

/// The smallest height a pull leaves, in millimetres.
pub const MIN_BOX_HEIGHT_MM: i64 = 1;

/// A rejected step of the box gesture.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum BoxDraftError {
    /// The corners do not enclose a rectangle with two positive sides.
    #[error("the base needs two corners that are apart on both plane axes")]
    FlatBase,
    /// The height is not a positive length.
    #[error("the height must be positive")]
    FlatHeight,
    /// The base rectangle was never fixed, so there is no box to build.
    #[error("the base rectangle is not finished")]
    NoBase,
}

/// The fixed base rectangle: its minimum corner and its two sides, in work-plane millimetres.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoxBase {
    /// The corner the box's origin sits on, in work-plane millimetres.
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

/// The base the height is pulled from and the height itself.
#[derive(Clone, Debug, PartialEq)]
pub struct BoxDraft {
    /// The first corner, once it is clicked.
    first: Option<[Length; 2]>,
    /// The corner a second click would fix the base at.
    live: Option<[Length; 2]>,
    /// The base rectangle once it is fixed.
    base: Option<BoxBase>,
    /// The drawn ring once its outline is finished.
    ring: Option<RingBase>,
    /// The height the pull currently shows.
    height: Length,
}

impl Default for BoxDraft {
    fn default() -> Self {
        Self::new()
    }
}

impl BoxDraft {
    /// An empty draft, waiting for the first corner of the base.
    pub const fn new() -> Self {
        Self { first: None, live: None, base: None, ring: None, height: Length::ZERO }
    }

    /// The first corner of the base, when it is already on the plane.
    pub fn first(&self) -> Option<[Length; 2]> {
        self.first
    }

    /// The corner the rectangle currently runs to: the pointer's plane point.
    pub fn live(&self) -> Option<[Length; 2]> {
        self.live
    }

    /// Sets the corner the live rectangle runs to.
    ///
    /// The shell maps the pointer onto the work plane every frame, so the rectangle the user sees is
    /// the rectangle the next click would fix.
    pub fn set_live(&mut self, corner: [Length; 2]) {
        if self.base.is_none() && self.ring.is_none() {
            self.live = Some(corner);
        }
    }

    /// A draft whose base is a ring already drawn on the plane, waiting for its height.
    ///
    /// The ring is stored as drawn; [`BoxDraft::envelope`] anchors it through the envelope's own rule,
    /// so the frame's zero is the ring's minimum corner and the sides are its bounds (ADR 0159).
    pub fn from_ring(corner: [Length; 2], contour: Contour) -> Self {
        Self {
            first: Some(corner),
            live: Some(corner),
            base: None,
            ring: Some(RingBase { corner, contour }),
            height: Length::ZERO,
        }
    }

    /// A click on the plane: the first fixes the anchor, the second fixes the base rectangle.
    ///
    /// Returns whether this click fixed the base, which is when the height pull begins. A click that
    /// would leave a flat rectangle is refused and leaves the draft waiting for another one.
    pub fn click(&mut self, corner: [Length; 2]) -> Result<bool, BoxDraftError> {
        if self.is_pulling() {
            return Ok(false);
        }
        let Some(first) = self.first else {
            self.first = Some(corner);
            self.live = Some(corner);
            return Ok(false);
        };
        self.live = Some(corner);
        let base = rectangle(first, corner).ok_or(BoxDraftError::FlatBase)?;
        self.base = Some(base);
        Ok(true)
    }

    /// The base rectangle, once it is fixed.
    pub const fn base(&self) -> Option<BoxBase> {
        self.base
    }

    /// The drawn ring, once its outline is finished.
    pub const fn ring(&self) -> Option<&RingBase> {
        self.ring.as_ref()
    }

    /// The corner of the base on the work plane, whichever base the draft holds.
    ///
    /// It is the frame's own zero, so the shell projects the pointer from it when the height is pulled
    /// (ADR 0082, ADR 0101).
    pub fn base_corner(&self) -> Option<[Length; 2]> {
        self.base.map(|base| base.corner).or_else(|| self.ring.as_ref().map(|ring| ring.corner))
    }

    /// The envelope the ✓ would commit, once both the base and a height exist.
    ///
    /// A rectangle base is three sides and a ring base is its ring and the height, so the two methods
    /// of the tool reach one command with one value and neither invents a shape (ADR 0159).
    pub fn envelope(&self) -> Option<Result<Envelope, EnvelopeError>> {
        self.base
            .map(|base| Envelope::new(base.width, base.depth, self.height))
            .or_else(|| self.ring.as_ref().map(|ring| Envelope::with_ring(&ring.contour, self.height)))
    }

    /// Whether the base is fixed and the gesture is pulling the height.
    pub const fn is_pulling(&self) -> bool {
        self.base.is_some() || self.ring.is_some()
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
        if !self.is_pulling() {
            return Err(BoxDraftError::NoBase);
        }
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
        let base = self.base?;
        Some([base.width, base.depth, self.height])
    }
}

/// The base rectangle between two opposite corners, or `None` when a side is not positive.
fn rectangle(first: [Length; 2], second: [Length; 2]) -> Option<BoxBase> {
    let axis = |index: usize| -> Option<(Length, Length)> {
        let start = first.get(index)?.micrometres();
        let end = second.get(index)?.micrometres();
        let side = end.checked_sub(start)?.checked_abs()?;
        let corner = start.min(end);
        Some((Length::from_micrometres(corner), Length::from_micrometres(side)))
    };
    let (x, width) = axis(0)?;
    let (y, depth) = axis(1)?;
    if width <= Length::ZERO || depth <= Length::ZERO {
        return None;
    }
    Some(BoxBase { corner: [x, y], width, depth })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use testkit::millimetres;

    use super::*;

    fn point(x: i64, y: i64) -> [Length; 2] {
        [millimetres(x), millimetres(y)]
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
        assert!(draft.is_pulling(), "the ring is the base, so the height is what is missing");
        assert_eq!(draft.sizes(), None, "a drawn ring has no three sides of its own");
        assert_eq!(draft.base_corner(), Some([millimetres(100), millimetres(50)]), "the pull starts at its corner");
        assert!(!draft.click(point(0, 0)).expect("a click while pulling"), "a click cannot re-anchor the ring");

        draft.set_height(millimetres(720)).expect("the pull");
        let envelope = draft.envelope().expect("a base").expect("a valid ring");
        assert!(envelope.footprint().is_ring(), "the envelope keeps the ring");
        assert_eq!(
            envelope.components(),
            [millimetres(600), millimetres(400), millimetres(720)],
            "the sides are the ring's bounds and the height is the pull's"
        );
    }

    #[test]
    fn the_first_click_anchors_and_the_second_fixes_the_base() {
        let mut draft = BoxDraft::new();
        assert!(!draft.click(point(600, 400)).expect("the anchor"), "the first click only anchors");
        assert_eq!(draft.first(), Some(point(600, 400)));
        assert!(draft.click(point(0, 0)).expect("the base"), "the second click fixes the base");
        assert!(draft.is_pulling());
        let base = draft.base().expect("a base");
        assert_eq!(base.corner, point(0, 0), "the corner is the minimum one, whichever order was clicked");
        assert_eq!((base.width, base.depth), (millimetres(600), millimetres(400)));
    }

    #[test]
    fn a_flat_base_is_refused_and_the_draft_waits() {
        let mut draft = BoxDraft::new();
        draft.click(point(100, 100)).expect("the anchor");
        assert_eq!(draft.click(point(400, 100)), Err(BoxDraftError::FlatBase), "no depth is no box");
        assert!(!draft.is_pulling(), "the base stays unfixed");
        assert!(draft.click(point(400, 300)).expect("a real base"));
        assert!(draft.is_pulling());
    }

    #[test]
    fn the_height_rounds_to_whole_millimetres_and_never_reaches_zero() {
        let mut draft = BoxDraft::new();
        draft.click(point(0, 0)).expect("anchor");
        draft.click(point(600, 400)).expect("base");
        assert_eq!(draft.sizes(), Some([millimetres(600), millimetres(400), Length::ZERO]), "no height yet");
        draft.set_height(Length::from_micrometres(719_600)).expect("a pulled height");
        assert_eq!(draft.height(), millimetres(720), "the pull rounds to the millimetre step");
        draft.set_height(Length::from_micrometres(-5_000)).expect_err("a negative pull is refused");
        assert_eq!(draft.height(), millimetres(720), "a refused pull keeps the last height");
        draft.set_height(Length::ZERO).expect_err("zero is not a solid");
        draft.set_height(Length::from_micrometres(1)).expect("one micrometre pulls up to one millimetre");
        assert_eq!(draft.height(), millimetres(1));
    }

    #[test]
    fn a_height_before_the_base_is_refused() {
        let mut draft = BoxDraft::new();
        assert_eq!(draft.set_height(millimetres(720)), Err(BoxDraftError::NoBase));
        assert_eq!(draft.sizes(), None);
    }
}
