//! The horizontal shape a product frame reserves: the rectangle, its cut corners, or a drawn ring.
//!
//! A product envelope is not only three lengths (ADR 0097): the space a corner cabinet reserves on the
//! floor is a rectangle with one corner cut off, and an end table may reserve a ring. The footprint is
//! the shape that rectangle is cut to, stored beside the three sides, and the sides `x` and `y` stay
//! the bounds of the shape so every reader that only wants the reserved box still gets one answer.
//!
//! A cut corner is stored as **offsets, never an angle**: the setback along each edge is what a maker
//! measures on a board, and a 120-degree cut is simply two unequal setbacks. The corner language is the
//! panel's own ([`CornerFeatureKind`]): a chamfer carries the two setbacks, a fillet a radius. The ring
//! is the panel outline's contour, so one generator draws both shapes and one set of refusals — a cut
//! that eats the whole edge, a ring whose bounds disagree with the sides — serves both.
//!
//! The outline generator is the panel's own ([`PanelOutline::ring_for`]), which keeps the two shapes
//! identical to a board cut to the same profile and gives the product no geometry code of its own.
//!
//! Changes when: the shape a footprint may take, the corner rule, or the ring's anchor changes.

use geom::Contour as OutlineRing;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    Contour, CornerFeature, CornerFeatureKind, Length, Panel, PanelContourError, PanelDimensionError, PanelNominalSize,
    PanelOutline, RectCorner,
};

/// The horizontal shape of a product envelope, on the product's base plane.
///
/// The shape is stored in the product's own frame: `[0..x] × [0..y]` are its bounds, the zero corner is
/// the node's origin, and the outline runs counter-clockwise from there (ADR 0083, ADR 0101). A
/// footprint is a reserved space and never a body: nothing is swept, meshed or shaded from it.
///
/// Validation belongs to [`crate::Envelope`], which is the only value that knows the bounds the shape
/// has to agree with; [`Footprint::outline`] is the one function that both checks and draws it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default, JsonSchema)]
#[serde(tag = "shape", rename_all = "snake_case")]
pub enum Footprint {
    /// The whole rectangle: no corner is cut, which is what a product starts as.
    #[default]
    Rectangle,
    /// The rectangle with one or more corners cut by a chamfer or a fillet.
    Cut {
        /// One treatment per nominal corner, counter-clockwise from the zero corner and keyed by
        /// [`RectCorner`]: `None` leaves that corner square.
        corners: [Option<CornerFeatureKind>; 4],
    },
    /// A closed ring drawn on the plane, stored in the product's own frame.
    Ring {
        /// The ring of directed lines and arcs, counter-clockwise, anchored at the zero corner.
        contour: Contour,
    },
}

impl Footprint {
    /// The plain rectangle a product starts as.
    pub const fn rectangle() -> Self {
        Self::Rectangle
    }

    /// A rectangle with one or more cut corners, keyed by [`RectCorner`].
    ///
    /// The value is not validated here: [`crate::Envelope`] checks it against the sides, because a cut
    /// is only too large relative to the edge it stands on.
    pub const fn cut(corners: [Option<CornerFeatureKind>; 4]) -> Self {
        Self::Cut { corners }
    }

    /// A closed ring drawn on the plane.
    ///
    /// [`crate::Envelope::with_ring`] is the creation path: it anchors the ring at its own minimum
    /// corner and takes the bounds from it, so a stored ring always agrees with its sides.
    pub const fn ring(contour: Contour) -> Self {
        Self::Ring { contour }
    }

    /// Whether the footprint is the plain rectangle.
    pub const fn is_rectangle(&self) -> bool {
        matches!(self, Self::Rectangle)
    }

    /// Whether the footprint is a drawn ring rather than a rectangle with cut corners.
    pub const fn is_ring(&self) -> bool {
        matches!(self, Self::Ring { .. })
    }

    /// The cut of one nominal corner, or `None` when the corner is square or the shape is a ring.
    pub fn corner_cut(&self, corner: RectCorner) -> Option<CornerFeatureKind> {
        let Self::Cut { corners } = self else {
            return None;
        };
        let index = RectCorner::ALL.iter().position(|candidate| *candidate == corner)?;
        corners.get(index).copied().flatten()
    }

    /// The cut corners in [`RectCorner::ALL`] order, empty for a rectangle or a ring.
    pub fn cuts(&self) -> [Option<CornerFeatureKind>; 4] {
        match self {
            Self::Cut { corners } => *corners,
            Self::Rectangle | Self::Ring { .. } => [None; 4],
        }
    }

    /// The drawn ring, when the footprint is one.
    pub const fn contour(&self) -> Option<&Contour> {
        match self {
            Self::Ring { contour } => Some(contour),
            Self::Rectangle | Self::Cut { .. } => None,
        }
    }

    /// The outline this footprint draws at `width` × `depth`, in product-local millimetres.
    ///
    /// The ring is counter-clockwise, starts on the zero corner's edges and carries arcs, so the scene's
    /// wireframe, the picking test and a panel cut to the same profile read one shape. This is also the
    /// validation: a corner cut that consumes its whole edge, a ring whose bounds are not the sides, or
    /// a corner dimension outside the panel's limits is a typed [`FootprintError`].
    pub fn outline(&self, width: Length, depth: Length) -> Result<OutlineRing, FootprintError> {
        let size = PanelNominalSize::new(width, depth).map_err(FootprintError::Dimension)?;
        let outline = match self {
            Self::Rectangle => PanelOutline::Rect { corner_features: Vec::new(), edge_bows: Vec::new() },
            Self::Cut { corners } => {
                let mut features = Vec::with_capacity(corners.len());
                for (index, kind) in corners.iter().enumerate() {
                    let Some(kind) = kind else {
                        continue;
                    };
                    let corner = RectCorner::ALL.get(index).copied().ok_or(FootprintError::CornerIndex { index })?;
                    let feature = CornerFeature::new(corner.id(), *kind).map_err(FootprintError::Dimension)?;
                    features.push(feature);
                }
                PanelOutline::Rect { corner_features: features, edge_bows: Vec::new() }
            }
            Self::Ring { contour } => PanelOutline::Contour { contour: contour.clone() },
        };
        let ring = outline.ring_for(size).map_err(FootprintError::Outline)?;
        // A drawn ring keeps an arc by its centre, radius and angles, so its joints can miss by the
        // document's own rounding — tens of micrometres on a metre-scale radius (ADR 0072). The exact
        // readers — the niche's half-plane cut, a region's containment test — need one boundary, so the
        // ring is welded on its lines here, once, for every reader. A ring the document's tolerance
        // cannot close is handed on as it came, and those readers refuse it instead of guessing (N34).
        Ok(ring.welded(|first, next| crate::joint_tolerance_mm(*first, *next)).unwrap_or(ring))
    }
}

/// Why a product footprint cannot be drawn at the sides it is stored with.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum FootprintError {
    /// The outline generator refused the shape: a cut that leaves no straight edge, or a ring whose
    /// bounds are not the sides it is stored with.
    #[error(transparent)]
    Outline(#[from] PanelContourError),
    /// A footprint side or a corner dimension is outside the panel limits, or not positive.
    #[error(transparent)]
    Dimension(#[from] PanelDimensionError),
    /// A cut was stored for a corner the rectangle does not have.
    #[error("the rectangle has no corner {index}")]
    CornerIndex {
        /// The rejected index into [`RectCorner::ALL`].
        index: usize,
    },
    /// A drawn ring cannot be moved to its own minimum corner.
    #[error(transparent)]
    Ring(#[from] crate::ContourError),
}

/// The ring a footprint occupies when the maker draws one.
///
/// A drawn ring is anchored at its own minimum corner before it is stored, exactly as a contour panel
/// is (ADR 0042), so the product's own zero stays its minimum corner and the outline needs no offset.
pub(crate) fn anchored_ring(contour: &Contour) -> Result<Contour, FootprintError> {
    Panel::anchored_contour(contour).map_err(FootprintError::Ring)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use geom::{DEFAULT_TANGENT_TOLERANCE, DVec2};
    use units::Length;

    use super::*;
    use crate::{Envelope, EnvelopeError, Path};

    fn millimetres(value: i64) -> Length {
        Length::from_millimetres(value).expect("whole millimetres fit a length")
    }

    fn chamfer(incoming: i64, outgoing: i64) -> Option<CornerFeatureKind> {
        Some(CornerFeatureKind::Chamfer { incoming: millimetres(incoming), outgoing: millimetres(outgoing) })
    }

    /// The outline's polyline, sampled finely enough that an arc reads as its curve.
    fn points(footprint: &Footprint, width: i64, depth: i64) -> Vec<DVec2> {
        footprint
            .outline(millimetres(width), millimetres(depth))
            .expect("a valid footprint")
            .sample(0.25)
            .expect("the outline samples")
    }

    #[test]
    fn the_plain_rectangle_draws_its_four_sides() {
        let outline = Footprint::rectangle().outline(millimetres(600), millimetres(500)).expect("a rectangle");
        assert_eq!(outline.segments().len(), 4, "one segment per side");
        assert!(!Footprint::rectangle().is_ring());
        assert_eq!(Footprint::rectangle().cuts(), [None; 4], "no corner is cut");
    }

    #[test]
    fn a_forty_five_degree_cut_is_two_equal_setbacks_and_a_hundred_and_twenty_degree_cut_is_a_ratio() {
        // A 45-degree cut has equal setbacks: the cut line runs at 45 degrees to both edges.
        let square = Footprint::cut([None, chamfer(100, 100), None, None]);
        let ring = points(&square, 600, 500);
        assert!(
            ring.iter().any(|point| (point - DVec2::new(500.0, 0.0)).length() < 0.01),
            "the cut starts a setback in"
        );
        assert!(ring.iter().any(|point| (point - DVec2::new(600.0, 100.0)).length() < 0.01), "and ends one out");
        assert!(
            !ring.iter().any(|point| (point - DVec2::new(600.0, 0.0)).length() < 0.01),
            "the square corner is gone"
        );

        // A 120-degree corner is the same rule with unequal offsets: the cut makes 120 degrees with
        // the edge it leaves, which is an outgoing setback of the incoming one times the square root
        // of three. No angle is stored; the maker's two offsets are.
        let oblique = Footprint::cut([None, chamfer(100, 173), None, None]);
        let ring = points(&oblique, 600, 500);
        let start = DVec2::new(500.0, 0.0);
        let end = DVec2::new(600.0, 173.0);
        let cut = (end - start).normalize();
        let leaving = DVec2::new(-1.0, 0.0);
        let degrees: f64 = cut.dot(leaving).clamp(-1.0, 1.0).acos().to_degrees();
        assert!((degrees - 120.0).abs() < 0.2, "the cut meets the edge at 120 degrees, was {degrees}");
        assert!(ring.iter().any(|point| (point - start).length() < 0.01));
        assert!(ring.iter().any(|point| (point - end).length() < 0.01));
    }

    #[test]
    fn the_cut_offsets_round_trip_through_the_file_unchanged() {
        let envelope = Envelope::with_footprint(
            millimetres(600),
            millimetres(500),
            millimetres(720),
            Footprint::cut([chamfer(120, 70), None, None, None]),
        )
        .expect("a corner cabinet");
        let text = serde_json::to_string(&envelope).expect("the envelope serializes");
        assert!(text.contains("\"incoming\":120000"), "the setback is stored as a length: {text}");
        assert!(text.contains("\"outgoing\":70000"), "and so is the other one: {text}");
        assert!(!text.contains("angle"), "a cut is offsets, never an angle: {text}");
        let back: Envelope = serde_json::from_str(&text).expect("the envelope reads back");
        assert_eq!(back, envelope, "the offsets survive the file");
        assert_eq!(back.footprint().cuts(), envelope.footprint().cuts());
    }

    #[test]
    fn a_fillet_is_a_tangent_arc_at_its_radius() {
        let filleted = Footprint::cut([None, Some(CornerFeatureKind::Radius { radius: millimetres(80) }), None, None]);
        let ring = filleted.outline(millimetres(600), millimetres(500)).expect("a rounded corner");
        let arcs = ring.segments().iter().filter(|segment| matches!(segment, geom::Segment::Arc(_))).count();
        assert_eq!(arcs, 1, "one arc replaces the corner");
        let samples = points(&filleted, 600, 500);
        assert!(
            !samples.iter().any(|point| (point - DVec2::new(600.0, 0.0)).length() < 1.0),
            "the sharp corner is gone"
        );
        // The arc's centre is the corner of the setback and every sample is one radius from it.
        let centre = DVec2::new(520.0, 80.0);
        let near: Vec<f64> =
            samples.iter().map(|point| (*point - centre).length()).filter(|distance| *distance <= 80.5).collect();
        assert!(!near.is_empty(), "the arc is sampled");
        assert!(near.iter().all(|distance| (distance - 80.0).abs() < 0.5), "every arc sample is one radius out");
    }

    #[test]
    fn a_cut_that_eats_a_whole_edge_is_a_readable_refusal() {
        // Two cuts on one 600 mm edge of 350 mm each leave nothing straight between them.
        let greedy = Footprint::cut([chamfer(10, 350), chamfer(350, 10), None, None]);
        let error = Envelope::with_footprint(millimetres(600), millimetres(500), millimetres(720), greedy)
            .expect_err("the edge is consumed");
        let sentence = error.to_string();
        assert!(sentence.contains("no straight edge"), "the refusal says what is wrong: {sentence}");
        assert!(matches!(error, EnvelopeError::Footprint { .. }), "and it is the footprint that is unusable");
    }

    #[test]
    fn a_ring_takes_its_bounds_from_its_own_outline() {
        let ring = Path::polyline(
            vec![
                [millimetres(120), millimetres(40)],
                [millimetres(520), millimetres(40)],
                [millimetres(520), millimetres(340)],
                [millimetres(120), millimetres(340)],
            ],
            true,
        )
        .expect("a closed ring")
        .to_contour()
        .expect("a contour");
        let envelope = Envelope::with_ring(&ring, millimetres(500)).expect("a drawn frame");
        assert!(envelope.footprint().is_ring());
        assert_eq!((envelope.x(), envelope.y(), envelope.z()), (millimetres(400), millimetres(300), millimetres(500)));
        let anchored = ring.translated([millimetres(-120), millimetres(-40)]).expect("the ring moves to its corner");
        assert_eq!(envelope.footprint().contour(), Some(&anchored), "the ring is anchored at its own minimum corner");
        let text = serde_json::to_string(&envelope).expect("the ring serializes");
        let back: Envelope = serde_json::from_str(&text).expect("the ring reads back");
        assert_eq!(back, envelope, "the drawn ring survives the file");
    }

    #[test]
    fn a_ring_whose_bounds_disagree_with_its_sides_is_refused() {
        let ring = Path::polyline(
            vec![
                [Length::ZERO, Length::ZERO],
                [millimetres(400), Length::ZERO],
                [millimetres(400), millimetres(300)],
                [Length::ZERO, millimetres(300)],
            ],
            true,
        )
        .expect("a closed ring")
        .to_contour()
        .expect("a contour");
        let error =
            Envelope::with_footprint(millimetres(500), millimetres(300), millimetres(500), Footprint::ring(ring))
                .expect_err("the sides are not the ring's bounds");
        assert!(error.to_string().contains("not usable"), "the refusal names the footprint: {error}");
    }

    #[test]
    fn the_corner_cut_is_read_by_the_corner_it_belongs_to() {
        let footprint = Footprint::cut([None, chamfer(100, 50), None, None]);
        assert_eq!(footprint.corner_cut(RectCorner::BottomRight), chamfer(100, 50));
        assert_eq!(footprint.corner_cut(RectCorner::BottomLeft), None);
        assert_eq!(Footprint::rectangle().corner_cut(RectCorner::BottomRight), None, "a rectangle cuts nothing");
        assert!(
            ring_has_no_corner(&footprint),
            "the joint between the cut and the edges is not smooth, so it is a corner"
        );
    }

    /// Whether the outline of `footprint` turns sharply somewhere, which is what a vertical edge marks.
    fn ring_has_no_corner(footprint: &Footprint) -> bool {
        let ring = footprint.outline(millimetres(600), millimetres(500)).expect("a valid footprint");
        (0..ring.segments().len()).any(|index| ring.joint_is_smooth(index, DEFAULT_TANGENT_TOLERANCE) == Some(false))
    }
}
