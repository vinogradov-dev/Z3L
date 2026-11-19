//! The contact port: what a body's own surface is, and the questions a drag asks of two of them.
//!
//! The document measures contact with the world box around a body, which is a coarse reading: a
//! turned cone is a box, so its invisible cuboid meets a plate long before the apex reaches the corner
//! between two plates. This crate is the port that answers instead with the body's **own surface**: a
//! drag, a probe and a write ask the same function, so a preview, a release and a refusal cannot
//! disagree (X85).
//!
//! A [`Shape`] is the body's geometry in its **own frame**, in millimetres — a convex hull, or a
//! closed triangle surface for a body that is not convex (a notched panel, a carcass with a door hole).
//! The document builds it from the node's own parametric data and never from the render mesh: what is
//! drawn is a tessellation chosen for the screen, and a measurement must not follow that choice.
//!
//! The port is an adapter-shaped trait, like `kernel` for geometry: this module holds the types and
//! the contract, `contact-parry` (L2) does the arithmetic, and the document never sees the
//! third-party type. Two roads are deliberately absent until a caller needs
//! them: a gap distance, and a broad phase. A drag asks
//!
//! - [`Contact::travel`] — how far the mover goes along a direction before it first touches the other
//!   body, which is the cut a drag makes and the crossing a jump rests on;
//! - [`Contact::penetration`] — how deeply the two are sunk and along which normal, which is the
//!   destination test and the depth a pair the document already holds overlapping may not exceed;
//! - [`Contact::overlap`] — the penetration against the document's own rounding, which is the one
//!   predicate both of those questions reduce to.
//!
//! The answer is pure: nothing is stored here and nothing is mutated, so a rule may ask it once per
//! frame and take the same answer as the release asks.
use std::any::Any;
use std::fmt;

use thiserror::Error;

use crate::{DQuat, DVec3};

/// The most points one convex hull may carry, so a hostile file cannot size an allocation.
pub const MAX_CONVEX_POINTS: usize = 4096;

/// The most vertices one triangle surface may carry.
pub const MAX_MESH_VERTICES: usize = 8192;

/// The most triangles one triangle surface may carry.
pub const MAX_MESH_TRIANGLES: usize = 16384;

/// A body's own surface, given in its own frame, in millimetres.
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    /// The convex hull of these points: a cone, a cylinder, a ball, a box, a straight panel.
    Convex(Vec<DVec3>),
    /// A closed triangle surface: the body's own faces, for a body that is not convex.
    Mesh {
        /// The points the triangles are joined through, in the body's own frame.
        vertices: Vec<DVec3>,
        /// The triangles, each one three indices into `vertices`, counter-clockwise from outside.
        triangles: Vec<[u32; 3]>,
    },
}

impl Shape {
    /// A convex shape from the points that bound it, which may number more than its corners.
    pub fn convex(points: Vec<DVec3>) -> Result<Self, ShapeError> {
        if points.len() < 4 {
            return Err(ShapeError::TooFewPoints);
        }
        if points.len() > MAX_CONVEX_POINTS {
            return Err(ShapeError::TooManyPoints);
        }
        if points.iter().any(|point| !point.is_finite()) {
            return Err(ShapeError::NotFinite);
        }
        Ok(Self::Convex(points))
    }

    /// A closed triangle surface from its points and its triangles.
    pub fn mesh(vertices: Vec<DVec3>, triangles: Vec<[u32; 3]>) -> Result<Self, ShapeError> {
        if vertices.len() > MAX_MESH_VERTICES {
            return Err(ShapeError::TooManyVertices);
        }
        if triangles.len() > MAX_MESH_TRIANGLES {
            return Err(ShapeError::TooManyTriangles);
        }
        if vertices.iter().any(|point| !point.is_finite()) {
            return Err(ShapeError::NotFinite);
        }
        let count = u32::try_from(vertices.len()).map_err(|_| ShapeError::TooManyVertices)?;
        for triangle in &triangles {
            let [a, b, c] = *triangle;
            if a >= count || b >= count || c >= count {
                return Err(ShapeError::IndexOutOfRange);
            }
            if a == b || b == c || a == c {
                return Err(ShapeError::BadTriangle);
            }
        }
        Ok(Self::Mesh { vertices, triangles })
    }

    /// The box around the shape in its own frame, or `None` for a shape with no surface.
    pub fn bounds(&self) -> Option<(DVec3, DVec3)> {
        let (points, empty) = match self {
            Self::Convex(points) => (points.as_slice(), points.is_empty()),
            Self::Mesh { vertices, .. } => (vertices.as_slice(), vertices.is_empty()),
        };
        if empty {
            return None;
        }
        let low = points.iter().copied().fold(DVec3::splat(f64::INFINITY), DVec3::min);
        let high = points.iter().copied().fold(DVec3::splat(f64::NEG_INFINITY), DVec3::max);
        (low.is_finite() && high.is_finite()).then_some((low, high))
    }
}

/// A shape the port refuses to measure.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum ShapeError {
    /// A convex hull needs at least four points to enclose a volume.
    #[error("a convex shape needs at least four points")]
    TooFewPoints,
    /// More points than [`MAX_CONVEX_POINTS`].
    #[error("a convex shape may carry at most {MAX_CONVEX_POINTS} points")]
    TooManyPoints,
    /// More vertices than [`MAX_MESH_VERTICES`].
    #[error("a triangle surface may carry at most {MAX_MESH_VERTICES} vertices")]
    TooManyVertices,
    /// More triangles than [`MAX_MESH_TRIANGLES`].
    #[error("a triangle surface may carry at most {MAX_MESH_TRIANGLES} triangles")]
    TooManyTriangles,
    /// A point that is not a number.
    #[error("a shape point is not a number")]
    NotFinite,
    /// A triangle that names a point the shape does not have.
    #[error("a triangle names a point the shape does not have")]
    IndexOutOfRange,
    /// A triangle that names one point twice.
    #[error("a triangle names one point twice")]
    BadTriangle,
}

/// Where a shape stands: the rigid frame its own points are taken from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    /// The world position of the shape's own frame, in millimetres.
    pub translation: DVec3,
    /// The world rotation of the shape's own frame.
    pub rotation: DQuat,
}

impl Pose {
    /// The world frame itself.
    pub const IDENTITY: Self = Self { translation: DVec3::ZERO, rotation: DQuat::IDENTITY };

    /// A pose at `translation`, turned by `rotation`.
    pub const fn new(translation: DVec3, rotation: DQuat) -> Self {
        Self { translation, rotation }
    }
}

/// How deeply two shapes are sunk into one another.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Penetration {
    /// The depth of the overlap, in millimetres; a pair that only touches is no penetration at all.
    pub depth_mm: f64,
    /// The contact normal of the first shape, pointing from it towards the second.
    pub normal: DVec3,
}

impl Penetration {
    /// Whether the pair is sunk deeper than `slack_mm` millimetres, the document's own rounding.
    ///
    /// A pair that only touches is contact and never an overlap, so the answer is strict: the depth
    /// must be more than the slack, which is the micrometre the document stores and rounds to. The
    /// slack is plain millimetres because this module lives at the geometry boundary, where a length
    /// is a number of millimetres; the document converts its own unit before asking.
    pub fn is_overlap(&self, slack_mm: f64) -> bool {
        self.depth_mm > slack_mm
    }
}

/// One shape as the loaded adapter holds it: built once, asked many times.
///
/// A drag asks the same two shapes every frame, so the adapter converts each of them once when the
/// rule is built and keeps its own representation here. The box is opaque to this crate — the adapter
/// that made it is the only code that reads it back, and a wrong type answers `None` rather than a
/// panic.
pub struct Prepared(Box<dyn Any>);

impl Prepared {
    /// Wraps the adapter's own representation of one shape.
    pub fn new<T: Any>(body: T) -> Self {
        Self(Box::new(body))
    }

    /// The adapter's representation, when this handle was made by that adapter.
    pub fn get<T: Any>(&self) -> Option<&T> {
        self.0.downcast_ref::<T>()
    }
}

impl fmt::Debug for Prepared {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Prepared")
    }
}

/// The questions a drag asks of two bodies' own surfaces.
///
/// Every method is pure and takes the whole question as arguments: the two prepared shapes and the
/// poses they stand at, so one rule may measure many pairs without a second call and a settings change
/// can never bend an answer the release would not give.
pub trait Contact: fmt::Debug {
    /// The adapter's own representation of `shape`, or `None` when it cannot measure that shape.
    ///
    /// A shape the adapter refuses is the caller's to handle: the document falls back to the reading it
    /// always had, so a hostile or degenerate body never blocks a drag.
    fn prepare(&self, shape: &Shape) -> Option<Prepared>;

    /// How far shape `a`, standing at `at_a`, travels along `direction` before it first touches `b`.
    ///
    /// The answer is in millimetres whatever the length of `direction`: the direction is taken as a
    /// heading, so a caller may pass a world axis or the vector between two places. `None` means the
    /// heading never meets `b` — the way is clear, or the two can never touch along it — and a pair
    /// that already penetrates answers zero, because it is touching now.
    fn travel(&self, a: &Prepared, at_a: Pose, direction: DVec3, b: &Prepared, at_b: Pose) -> Option<f64>;

    /// How deeply the two shapes are sunk, and along which normal.
    ///
    /// `None` when they only touch or stand apart: a face is contact and never an overlap, exactly as
    /// the document's placement guard reads it (ADR 0162).
    fn penetration(&self, a: &Prepared, at_a: Pose, b: &Prepared, at_b: Pose) -> Option<Penetration>;

    /// Whether the two shapes overlap more deeply than `slack_mm`, the document's own rounding.
    fn overlap(&self, a: &Prepared, at_a: Pose, b: &Prepared, at_b: Pose, slack_mm: f64) -> bool {
        self.penetration(a, at_a, b, at_b).is_some_and(|penetration| penetration.is_overlap(slack_mm))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    fn cube(size: f64) -> Shape {
        let half = size / 2.0;
        Shape::convex(
            [
                (-1.0, -1.0, -1.0),
                (1.0, -1.0, -1.0),
                (-1.0, 1.0, -1.0),
                (1.0, 1.0, -1.0),
                (-1.0, -1.0, 1.0),
                (1.0, -1.0, 1.0),
                (-1.0, 1.0, 1.0),
                (1.0, 1.0, 1.0),
            ]
            .into_iter()
            .map(|(x, y, z)| DVec3::new(x * half, y * half, z * half))
            .collect(),
        )
        .expect("a cube is a convex shape")
    }

    #[test]
    fn a_convex_shape_needs_four_finite_points() {
        assert_eq!(Shape::convex(vec![DVec3::ZERO; 3]), Err(ShapeError::TooFewPoints));
        let mut points = vec![DVec3::ZERO; 4];
        points[2] = DVec3::splat(f64::NAN);
        assert_eq!(Shape::convex(points), Err(ShapeError::NotFinite));
        assert!(Shape::convex(vec![DVec3::ZERO; 4]).is_ok(), "four points are enough for the port");
    }

    #[test]
    fn a_triangle_surface_is_checked_by_its_indices() {
        let vertices = vec![DVec3::ZERO, DVec3::X, DVec3::Y];
        assert!(Shape::mesh(vertices.clone(), vec![[0, 1, 2]]).is_ok());
        assert_eq!(Shape::mesh(vertices.clone(), vec![[0, 1, 3]]), Err(ShapeError::IndexOutOfRange));
        assert_eq!(Shape::mesh(vertices, vec![[0, 1, 1]]), Err(ShapeError::BadTriangle));
    }

    #[test]
    fn the_bounds_of_a_shape_are_its_own_frame() {
        let (low, high) = cube(100.0).bounds().expect("a cube is bounded");
        assert_eq!((low, high), (DVec3::splat(-50.0), DVec3::splat(50.0)));
    }

    #[test]
    fn a_prepared_shape_is_only_read_by_the_adapter_that_made_it() {
        let prepared = Prepared::new(7_u32);
        assert_eq!(prepared.get::<u32>(), Some(&7));
        assert_eq!(prepared.get::<u64>(), None, "another adapter's representation is no answer");
    }

    #[test]
    fn an_overlap_is_strictly_deeper_than_the_documents_rounding() {
        let slack = 0.001;
        let touching = Penetration { depth_mm: 0.0, normal: DVec3::X };
        assert!(!touching.is_overlap(slack), "a face is contact, not an overlap");
        let sunk = Penetration { depth_mm: 0.002, normal: DVec3::X };
        assert!(sunk.is_overlap(slack));
    }
}
