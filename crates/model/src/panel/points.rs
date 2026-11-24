//! A board drawn by points in space: the plane the first three points span, and the figure on it.
//!
//! A board drawn on a work plane is stored in a plane the caller chose before the first click
//! (ADR 0114); this module answers the other order, which is the one the owner asked for (U126): the
//! points come first, the first three span the plane, and whatever is drawn afterwards is measured
//! in that plane. Two figures use it — the rectangle whose fourth corner is computed and the ring the
//! maker closes when he is done — so the plane is resolved once, here, and no tool guesses it.
//!
//! The arithmetic is floating point because it is geometry: a plane is a cross product, not a
//! document field. Every answer is converted back to whole micrometres before it leaves, so the
//! placement, the nominal size and the ring the caller stores are the document's own integers.
//!
//! Refuses a plane three points do not span, two points that coincide, a figure whose sides are
//! outside the panel's range and a coordinate the document cannot hold; the caller keeps its draft
//! and says so. Nothing here creates a node: the figure and its frame are the whole answer, and the
//! command that stores them is the caller's.
//!
//! Changes when: what a board drawn by points is, or the frame it is born in, changes.

use geom::{DMat3, DQuat, DVec3};
use units::Length;

use crate::{Contour, ContourSegment, PanelNominalSize, Placement, PlacementError};

/// Why a set of points names no board.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum PointsError {
    /// Fewer than three points: no plane is spanned.
    #[error("a board drawn by points needs three points, not {got}")]
    NotEnoughPoints {
        /// The number of points the caller supplied.
        got: usize,
    },
    /// Two consecutive points coincide, so the figure has a side of no length.
    #[error("two of the board's points coincide")]
    ZeroSide,
    /// The first three points lie on one line, so no plane passes through them.
    #[error("the first three points lie on one line, so they span no plane")]
    Collinear,
    /// The figure's sides are outside the range a panel can have.
    #[error("the board's sides are outside the panel range")]
    OutOfRange,
    /// A point that cannot be stored in document units: not finite, or too large for a length.
    #[error("a point of the board cannot be stored in document units")]
    NotRepresentable,
    /// The frame the points span is not a rigid placement.
    #[error(transparent)]
    Placement(#[from] PlacementError),
}

/// The plane three points span, as the frame a board is drawn in.
///
/// The origin is the first point, `x` the unit direction of the first edge (`first`→`second`) and
/// `y` the unit direction in the plane on the third point's side, so the third point's local width
/// is positive. The normal is `x × y`, the way a panel's face `A` points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointsPlane {
    origin: DVec3,
    x: DVec3,
    y: DVec3,
}

impl PointsPlane {
    /// The plane through three points, with the first point as its origin.
    ///
    /// The first two points fix the edge and the third the plane, so three points on one line are
    /// refused: a plane guessed from the first two would put the figure in a place nobody showed.
    pub fn from_three(first: [Length; 3], second: [Length; 3], third: [Length; 3]) -> Result<Self, PointsError> {
        let origin = world(first)?;
        let edge = world(second)? - origin;
        let x = unit(edge).ok_or(PointsError::ZeroSide)?;
        let span = world(third)? - origin;
        // The width is the part of the third point's offset square to the edge; a zero length is a
        // third point on the edge's own line, which spans no plane.
        let across = span - x * span.dot(x);
        let y = unit(across).ok_or(PointsError::Collinear)?;
        Ok(Self { origin, x, y })
    }

    /// The frame a board drawn in this plane is born in: its origin and its two axes.
    ///
    /// A panel's local zero is its outline's base corner (ADR 0083) and that corner is the first
    /// point, because `x` runs along the first edge and `y` on the third point's side.
    pub fn placement(self) -> Result<Placement, PointsError> {
        let z = self.x.cross(self.y);
        let rotation = DQuat::from_mat3(&DMat3::from_cols(self.x, self.y, z)).normalize();
        Placement::new(self.origin, rotation).map_err(PointsError::from)
    }

    /// The plane coordinate of a world point, in whole micrometres.
    ///
    /// The point is projected square onto the plane, which is what a ring drawn from points taken on
    /// other faces needs: the third point fixed the plane, and every later point names a place on it.
    pub fn local(self, point: [Length; 3]) -> Result<[Length; 2], PointsError> {
        let offset = world(point)? - self.origin;
        Ok([length(offset.dot(self.x))?, length(offset.dot(self.y))?])
    }

    /// The world point a stored point projects to on the plane.
    pub fn project(self, point: [Length; 3]) -> Result<[Length; 3], PointsError> {
        let [u, v] = self.local(point)?;
        let projected = self.origin + self.x * u.millimetres_f64() + self.y * v.millimetres_f64();
        Ok([length(projected.x)?, length(projected.y)?, length(projected.z)?])
    }

    /// How far along the first edge `point` lies, in whole micrometres.
    ///
    /// The first edge's own length is the same question asked of the second point, so the rectangle
    /// reads its long side here and not by subtracting coordinates.
    pub fn along(self, point: [Length; 3]) -> Result<Length, PointsError> {
        let offset = world(point)? - self.origin;
        length(offset.dot(self.x))
    }
}

/// The rectangle three points draw: the first edge, and the width the third point fixes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointsBoard {
    /// The plane the board lies in: the frame the panel is born in.
    pub plane: PointsPlane,
    /// The board's sides on the plane, in the frame's own axes.
    pub size: PanelNominalSize,
}

/// The board the three-point method draws.
///
/// The first two points are one edge and the third fixes the width, its distance from that edge
/// measured square to it in the plane; the fourth corner is therefore computed, and the figure is a
/// true rectangle — a standard panel, not a contour (ADR 0042, 0238). The third point need not be a
/// corner: only its distance from the edge matters, so a maker can click the far wall's face.
pub fn board_from_three(
    first: [Length; 3],
    second: [Length; 3],
    third: [Length; 3],
) -> Result<PointsBoard, PointsError> {
    let plane = PointsPlane::from_three(first, second, third)?;
    // `x` points the way the edge runs and `y` towards the third point, so both sides are the
    // positive distances the rectangle spans from its base corner.
    let long = plane.along(second)?;
    let wide = plane.local(third)?[1];
    let size = PanelNominalSize::new(long, wide).map_err(|_| PointsError::OutOfRange)?;
    Ok(PointsBoard { plane, size })
}

/// The ring a set of points encloses, measured on the plane its first three points span.
#[derive(Clone, Debug, PartialEq)]
pub struct PointsRing {
    /// The plane the ring lies in: the frame the panel is born in.
    pub plane: PointsPlane,
    /// The ring in the plane's coordinates, closed.
    ///
    /// It starts at the first point and is not yet anchored at its base corner: the panel's own
    /// creation path does that, so the ring and the placement move together (ADR 0083).
    pub contour: Contour,
}

/// The board the n-point method draws: every point projected onto the plane the first three span.
///
/// The first point starts the ring, the second gives its direction and the third the plane; a later
/// point taken on another face is projected onto that plane, so the ring is flat by construction.
pub fn ring_from_points(points: &[[Length; 3]]) -> Result<PointsRing, PointsError> {
    let mut points = points.iter().copied();
    let (Some(first), Some(second), Some(third)) = (points.next(), points.next(), points.next()) else {
        return Err(PointsError::NotEnoughPoints { got: 2 });
    };
    let plane = PointsPlane::from_three(first, second, third)?;
    let local = std::iter::once(first)
        .chain(std::iter::once(second))
        .chain(std::iter::once(third))
        .chain(points)
        .map(|point| plane.local(point))
        .collect::<Result<Vec<_>, _>>()?;
    let segments = (0..local.len())
        .map(|index| {
            let from = local[index];
            let to = local[(index + 1) % local.len()];
            ContourSegment::line(from, to).map_err(|_| PointsError::ZeroSide)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let contour = Contour::new(segments).map_err(|_| PointsError::OutOfRange)?;
    Ok(PointsRing { plane, contour })
}

/// A stored point as geometry millimetres, the one floating-point reading of the domain's integers.
fn world(point: [Length; 3]) -> Result<DVec3, PointsError> {
    let [x, y, z] = point.map(Length::millimetres_f64);
    let vector = DVec3::new(x, y, z);
    if vector.is_finite() { Ok(vector) } else { Err(PointsError::NotRepresentable) }
}

/// A direction of unit length, or `None` when the vector has none.
///
/// The threshold is one micrometre in millimetres: shorter than the document's own step, so a
/// direction that short is not a direction any figure could be measured along.
fn unit(vector: DVec3) -> Option<DVec3> {
    (vector.length_squared() > 1.0e-6).then(|| vector.normalize()).filter(|unit| unit.is_finite())
}

/// A geometry millimetre value as a stored length in whole micrometres.
fn length(millimetres: f64) -> Result<Length, PointsError> {
    Length::from_millimetres_f64(millimetres).ok_or(PointsError::NotRepresentable)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;
    use crate::role_of_plane;
    use geom::DVec3;

    /// A point in whole millimetres, the way a snapped click arrives.
    fn point(x: i64, y: i64, z: i64) -> [Length; 3] {
        [x, y, z].map(|value| Length::from_millimetres(value).unwrap_or_default())
    }

    /// A point in tenths of a millimetre, so a test can name a leaned plane exactly.
    fn tenths(x: i64, y: i64, z: i64) -> [Length; 3] {
        [x, y, z].map(|value| Length::from_micrometres(value * 100))
    }

    fn millimetres(length: Length) -> f64 {
        length.millimetres_f64()
    }

    #[test]
    fn three_points_give_a_rectangle_on_the_plane_they_span() {
        let board = board_from_three(point(0, 0, 0), point(600, 0, 0), point(0, 400, 0)).expect("a plane");
        assert_eq!(board.size.length(), Length::from_millimetres(600).expect("600 mm"));
        assert_eq!(board.size.width(), Length::from_millimetres(400).expect("400 mm"));
        let placement = board.plane.placement().expect("a rigid frame");
        assert!(placement.translation().abs_diff_eq(DVec3::ZERO, 1.0e-9), "the first point is the zero corner");
        let x = placement.transform_vector(DVec3::X);
        assert!(x.abs_diff_eq(DVec3::X, 1.0e-9), "the first edge runs along the world X here: {x:?}");
        // The fourth corner is computed, not clicked: it is the second point plus the width.
        let fourth = placement.transform_point(DVec3::new(600.0, 400.0, 0.0));
        assert!(
            fourth.abs_diff_eq(DVec3::new(600.0, 400.0, 0.0), 1.0e-9),
            "the corner beside the third point: {fourth:?}"
        );
        // A board lying flat is a shelf, and the role comes from the plane and not from a niche.
        assert_eq!(
            role_of_plane(placement.transform_vector(DVec3::Z)),
            Some(crate::ClassCode::HORIZONTAL_PANEL),
            "a board on the floor is a shelf"
        );
    }

    #[test]
    fn the_third_point_need_not_be_a_corner_it_fixes_the_width() {
        // A point on a leaned face 10 mm above the floor still measures the board's width square to
        // the edge, and the plane it spans tilts with it.
        let board = board_from_three(point(0, 0, 0), point(600, 0, 0), point(120, 400, 10)).expect("a plane");
        assert_eq!(board.size.length(), Length::from_millimetres(600).expect("600 mm"));
        let width = board.size.width().millimetres_f64();
        assert!((width - 400.125).abs() < 0.001, "the width is the distance square to the edge: {width}");
    }

    #[test]
    fn a_board_on_a_leaned_plane_is_born_with_the_role_that_plane_implies() {
        // A board standing on its edge along a wall: the normal is the wall's own, which is a post.
        let board = board_from_three(point(0, 0, 0), point(0, 600, 0), point(-18, 0, 400)).expect("a plane");
        let placement = board.plane.placement().expect("a rigid frame");
        let normal = placement.transform_vector(DVec3::Z);
        assert_eq!(
            role_of_plane(normal),
            Some(crate::ClassCode::VERTICAL_PANEL),
            "a leaned side is a post: {normal:?}"
        );
        // A shelf tipped more than 45 degrees is no longer a shelf.
        let lean = board_from_three(point(0, 0, 0), point(600, 0, 0), point(0, 300, 300)).expect("a plane");
        let normal = lean.plane.placement().expect("a rigid frame").transform_vector(DVec3::Z);
        assert!(normal.y.abs() > 0.1, "the normal leans: {normal:?}");
    }

    #[test]
    fn a_third_point_on_the_first_edge_spans_no_plane() {
        assert_eq!(
            board_from_three(point(0, 0, 0), point(600, 0, 0), point(300, 0, 0)),
            Err(PointsError::Collinear),
            "three points on one line name no plane"
        );
        assert_eq!(
            ring_from_points(&[point(0, 0, 0), point(600, 0, 0), point(300, 0, 0)]),
            Err(PointsError::Collinear)
        );
    }

    #[test]
    fn two_points_on_one_spot_are_no_edge() {
        assert_eq!(
            board_from_three(point(10, 10, 10), point(10, 10, 10), point(0, 400, 0)),
            Err(PointsError::ZeroSide)
        );
    }

    #[test]
    fn three_points_close_a_triangle_and_five_an_n_gon() {
        let base = [point(0, 0, 0), point(600, 0, 0), point(0, 400, 0)];
        let triangle = ring_from_points(&base).expect("a triangle");
        assert_eq!(triangle.contour.segments().len(), 3, "three points make three sides");
        let five = vec![point(0, 0, 0), point(600, 0, 0), point(900, 400, 0), point(300, 700, 0), point(-200, 400, 0)];
        let ring = ring_from_points(&five).expect("a pentagon");
        assert_eq!(ring.contour.segments().len(), 5, "n points make n sides");
        let (min, max) = ring.contour.bounds_micrometres();
        assert_eq!((min, max), ([-200_000, 0], [900_000, 700_000]), "the ring keeps the figure's own bounds");
    }

    #[test]
    fn a_point_taken_on_another_face_is_projected_onto_the_plane() {
        // The third point fixes the floor's plane; the fourth is clicked on a board 250 mm up and is
        // measured where it projects, not where it was clicked.
        let ring = ring_from_points(&[point(0, 0, 0), point(600, 0, 0), point(600, 400, 0), point(0, 400, 250)])
            .expect("a ring");
        let plane = ring.plane;
        assert_eq!(plane.project(point(0, 400, 250)), Ok(point(0, 400, 0)), "the height is dropped");
        assert_eq!(ring.contour.segments().len(), 4);
    }

    #[test]
    fn a_plane_is_a_frame_whose_axes_are_unit_and_square() {
        let board =
            board_from_three(tenths(12_345, -4_321, 987), tenths(13_345, 2_000, 1_500), tenths(12_000, 400, 4_000))
                .expect("a plane");
        let placement = board.plane.placement().expect("a rigid frame");
        let x = placement.transform_vector(DVec3::X);
        let y = placement.transform_vector(DVec3::Y);
        let z = placement.transform_vector(DVec3::Z);
        assert!((x.length() - 1.0).abs() < 1.0e-12 && (y.length() - 1.0).abs() < 1.0e-12);
        assert!(x.dot(y).abs() < 1.0e-12, "the axes are square");
        assert!(x.cross(y).abs_diff_eq(z, 1.0e-12), "the frame is right-handed");
        assert!(millimetres(board.size.length()) > 0.0 && millimetres(board.size.width()) > 0.0);
    }

    #[test]
    fn a_ring_needs_three_points() {
        assert_eq!(ring_from_points(&[point(0, 0, 0), point(600, 0, 0)]), Err(PointsError::NotEnoughPoints { got: 2 }));
        assert_eq!(ring_from_points(&[]), Err(PointsError::NotEnoughPoints { got: 2 }));
    }
}
