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
//! The port is an adapter-shaped pair of traits, like `kernel` for geometry. [`Contact`] is asked
//! **once**, when a rule is built: it turns a [`Shape`] and the place it stands at into a [`Shared`]
//! body the adapter owns, so a drag asks a body every frame without a trait object threaded through
//! the rule. [`Solid`] is that body, and it answers two questions about a pair:
//!
//! - [`Solid::travel`] — how far this body goes along a direction before it first touches another,
//!   which is the cut a drag makes and the crossing a jump rests on;
//! - [`Solid::penetration`] — how deeply the two are sunk and along which normal, which is the
//!   destination test and the depth a pair the document already holds overlapping may not exceed;
//!   [`overlap`] turns it into the one predicate a refusal asks, against the document's own rounding.
//!
//! A gap distance and a broad phase are deliberately absent until a caller needs them: an unused
//! query is a promise the code cannot keep. The answer is pure — nothing is stored and nothing is
//! mutated — so a rule may ask it once per frame and take the same answer as the release asks.
use std::any::Any;
use std::fmt;
use std::sync::Arc;

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

/// One body's own surface, prepared by an adapter and posed in the world.
///
/// It is what a rule carries instead of a shape: the adapter built it once, the body knows where it
/// stands, and every question about a pair is asked of the two bodies themselves. No trait object
/// travels through the document, so a rule stores its bodies and its callers never see the adapter.
pub trait Solid: fmt::Debug + Any {
    /// The world bounds of this body, in millimetres: its own faces, not the box an adapter uses.
    fn bounds(&self) -> (DVec3, DVec3);

    /// The same body moved rigidly by `delta` millimetres, or `None` when the move is not finite.
    fn shifted(&self, delta: DVec3) -> Option<Shared>;

    /// How far this body travels along `direction` before it first touches `other`.
    ///
    /// The answer is in millimetres whatever the length of `direction`: the direction is taken as a
    /// heading, so a caller may pass a world axis or the vector between two places. `None` means the
    /// heading never meets the other body, and a pair that already penetrates answers zero, because it
    /// is touching now. A body another adapter made answers `None`: a contract is not a licence to
    /// guess.
    fn travel(&self, other: &dyn Solid, direction: DVec3) -> Option<f64>;

    /// How deeply this body is sunk into `other`, and along which normal it comes out.
    ///
    /// `None` when they only touch or stand apart: a face is contact and never an overlap, exactly as
    /// the document's placement guard reads it (ADR 0162).
    fn penetration(&self, other: &dyn Solid) -> Option<Penetration>;
}

/// A prepared body, shared by the rule it belongs to.
pub type Shared = Arc<dyn Solid>;

/// Whether two bodies overlap more deeply than `slack_mm` millimetres, the document's own rounding.
///
/// A pair that only touches is contact and never an overlap, so the answer is strict: the depth must
/// be more than the slack, which is the micrometre the document stores and rounds to. The slack is
/// plain millimetres because this module lives at the geometry boundary, where a length is a number of
/// millimetres; the document converts its own unit before asking.
pub fn overlap(a: &dyn Solid, b: &dyn Solid, slack_mm: f64) -> bool {
    a.penetration(b).is_some_and(|penetration| penetration.depth_mm > slack_mm)
}

/// The adapter: what turns a [`Shape`] into a body it can measure.
///
/// One implementation exists — `contact-parry` — and the shell constructs it once. A rule asks it for
/// every body it will measure when the rule is built, and never again: the expensive conversion
/// happens once per gesture, and a stub answers contact in a test without a collision engine.
pub trait Contact: fmt::Debug {
    /// This adapter's own representation of `shape` standing at `pose`, or `None` when it cannot
    /// measure that shape.
    ///
    /// A shape the adapter refuses is the caller's to handle: the document falls back to the reading
    /// it always had, so a hostile or degenerate body never blocks a drag.
    fn prepare(&self, shape: &Shape, pose: Pose) -> Option<Shared>;
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

    /// A body that is a box and answers from its own extents, so the port's contract is testable
    /// without a collision engine.
    #[derive(Debug)]
    struct Block {
        pose: Pose,
        half: DVec3,
    }

    /// The stub's own body, when the other side of a pair is one too.
    fn as_block(solid: &dyn Solid) -> Option<&Block> {
        (solid as &dyn Any).downcast_ref::<Block>()
    }

    impl Solid for Block {
        fn bounds(&self) -> (DVec3, DVec3) {
            (self.pose.translation - self.half, self.pose.translation + self.half)
        }

        fn shifted(&self, delta: DVec3) -> Option<Shared> {
            delta.is_finite().then(|| {
                Arc::new(Self { pose: Pose::new(self.pose.translation + delta, self.pose.rotation), half: self.half })
                    as Shared
            })
        }

        fn travel(&self, other: &dyn Solid, direction: DVec3) -> Option<f64> {
            let other = as_block(other)?;
            let heading = direction.normalize_or_zero();
            let reach = other.pose.translation - self.pose.translation;
            let axis = (0..3).find(|axis| heading[*axis].abs() > 0.5)?;
            let gap = (reach[axis].abs() - self.half[axis] - other.half[axis]).max(0.0);
            (reach[axis].signum() == heading[axis].signum()).then_some(gap)
        }

        fn penetration(&self, other: &dyn Solid) -> Option<Penetration> {
            let other = as_block(other)?;
            let reach = other.pose.translation - self.pose.translation;
            let mut least = f64::INFINITY;
            let mut normal = DVec3::X;
            for axis in 0..3 {
                let depth = self.half[axis] + other.half[axis] - reach[axis].abs();
                if depth < least {
                    least = depth;
                    normal = if reach[axis] < 0.0 { -DVec3::AXES[axis] } else { DVec3::AXES[axis] };
                }
            }
            (least > 0.0).then_some(Penetration { depth_mm: least, normal })
        }
    }

    /// The test adapter: it makes the stub body above and remembers nothing.
    #[derive(Debug)]
    struct Stub;

    impl Contact for Stub {
        fn prepare(&self, shape: &Shape, pose: Pose) -> Option<Shared> {
            let (low, high) = shape.bounds()?;
            Some(Arc::new(Block { pose, half: (high - low) / 2.0 }) as Shared)
        }
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
    fn a_body_a_rule_carries_answers_its_own_pairs() {
        let stub = Stub;
        let cube = stub.prepare(&cube(100.0), Pose::IDENTITY).expect("a cube is measurable");
        assert_eq!(cube.bounds(), (DVec3::splat(-50.0), DVec3::splat(50.0)));
        let away = cube.shifted(DVec3::new(200.0, 0.0, 0.0)).expect("a finite move");
        assert_eq!(cube.travel(&*away, DVec3::X), Some(100.0), "the faces meet after 100 mm");
        assert_eq!(cube.travel(&*away, DVec3::NEG_X), None, "the way back is clear");
        assert_eq!(cube.penetration(&*away), None, "a pair that stands apart is no overlap");
        let inside = cube.shifted(DVec3::new(50.0, 0.0, 0.0)).expect("a finite move");
        assert_eq!(cube.penetration(&*inside).map(|sunk| sunk.depth_mm), Some(50.0));
        assert!(overlap(&*cube, &*inside, 0.001), "sunk 50 mm is deeper than the document's rounding");
        assert!(!overlap(&*cube, &*away, 0.001), "a pair that stands apart is no overlap");
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
