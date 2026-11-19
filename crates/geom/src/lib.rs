//! Geometry foundation shared by every Z3L crate.
//!
//! Vectors, matrices and Euler orders are `glam` types re-exported here; Z3L-specific shapes
//! (frames, bounds, meshes) live in this crate. Physical values are millimetres at this
//! boundary; the document keeps `units::Length` (micrometres).
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod boolean;
pub mod contact;
pub mod contour;
pub mod rect;
pub mod ring;
pub mod segment;
pub mod triangulate;

pub use boolean::{intersect_regions, subtract_regions, union_regions};
pub use contact::{
    Contact, MAX_CONVEX_POINTS, MAX_MESH_TRIANGLES, MAX_MESH_VERTICES, Penetration, Pose, Prepared, Shape, ShapeError,
};
pub use contour::{Contour, ContourError, DEFAULT_TANGENT_TOLERANCE};
pub use glam::{DMat4, DQuat, DVec2, DVec3, DVec4, EulerRot};
pub use rect::{Rect, RectError, connected_count, regions_from_rects, subtract_rects, total_area, union_rects};
pub use ring::{Region, RegionError, RegionSamples};
pub use segment::{Arc, Line, MAX_ARC_EXTENSION, Segment, point_on_circle};
pub use triangulate::{MAX_FACE_POINTS, TriangulateError, triangulate_face};

/// A right-handed local frame: `x` and `y` span the profile plane, `z` is the extrusion axis.
///
/// Coordinates are millimetres. The frame is always orthonormal with `z = x × y`; constructors
/// reject degenerate input instead of producing a skewed frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    /// Frame origin in world space.
    pub origin: DVec3,
    /// Unit vector of the local X axis.
    pub x: DVec3,
    /// Unit vector of the local Y axis.
    pub y: DVec3,
    /// Unit vector of the local Z axis (`x × y`).
    pub z: DVec3,
}

impl Frame {
    /// The world frame: origin at zero, axes aligned with world X/Y/Z.
    pub const WORLD: Frame = Frame { origin: DVec3::ZERO, x: DVec3::X, y: DVec3::Y, z: DVec3::Z };

    /// Builds a frame from an origin and two direction vectors.
    ///
    /// The directions are normalised and orthogonalised; `y` is projected onto the plane
    /// perpendicular to `x`. Returns `None` for a non-finite input or a zero-length direction.
    pub fn from_origin_axes(origin: DVec3, x: DVec3, y: DVec3) -> Option<Frame> {
        if !origin.is_finite() || !x.is_finite() || !y.is_finite() {
            return None;
        }
        let x = x.try_normalize()?;
        let y = (y - x * x.dot(y)).try_normalize()?;
        let z = x.cross(y);
        Some(Frame { origin, x, y, z })
    }

    /// Maps a point in the local profile plane to world space.
    pub fn point(&self, u: f64, v: f64) -> DVec3 {
        self.origin + self.x * u + self.y * v
    }

    /// Returns the frame translated along its local Z axis.
    pub fn offset(&self, distance: f64) -> Frame {
        Frame { origin: self.origin + self.z * distance, ..*self }
    }

    /// Checks that the axes are finite, unit length and mutually perpendicular.
    pub fn is_orthonormal(&self) -> bool {
        let unit = |v: DVec3| v.is_finite() && (v.length() - 1.0).abs() <= 1e-9;
        unit(self.x)
            && unit(self.y)
            && unit(self.z)
            && self.x.dot(self.y).abs() <= 1e-9
            && self.x.cross(self.y).dot(self.z) >= 1.0 - 1e-9
    }
}

/// An axis-aligned box in one frame, as two opposite corners.
///
/// A box is a value, not a computation: a mesh measures its own box once, when it is built, and a
/// scene walk turns that box's eight corners into a world box instead of transforming every vertex
/// (X56). Coordinates are in the frame the points live in — millimetres on both sides of the
/// geometry boundary.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    min: DVec3,
    max: DVec3,
}

impl Aabb {
    /// The box around a point cloud, or `None` when it holds no finite point.
    ///
    /// A non-finite point is skipped, not rejected: one bad vertex must not hide the rest of a mesh.
    pub fn of_points(points: impl IntoIterator<Item = DVec3>) -> Option<Self> {
        let mut box_: Option<Self> = None;
        for point in points {
            if !point.is_finite() {
                continue;
            }
            let point_box = Self { min: point, max: point };
            box_ = Some(match box_ {
                Some(current) => current.union(point_box),
                None => point_box,
            });
        }
        box_
    }

    /// The corner with the smallest coordinate on every axis.
    pub const fn min(self) -> DVec3 {
        self.min
    }

    /// The corner with the largest coordinate on every axis.
    pub const fn max(self) -> DVec3 {
        self.max
    }

    /// The centre of the box.
    pub fn center(self) -> DVec3 {
        (self.min + self.max) * 0.5
    }

    /// The radius of the sphere around the centre that contains the box.
    pub fn radius(self) -> f64 {
        ((self.max - self.min) * 0.5).length()
    }

    /// The smallest box around both boxes.
    pub fn union(self, other: Self) -> Self {
        Self {
            min: DVec3::new(self.min.x.min(other.min.x), self.min.y.min(other.min.y), self.min.z.min(other.min.z)),
            max: DVec3::new(self.max.x.max(other.max.x), self.max.y.max(other.max.y), self.max.z.max(other.max.z)),
        }
    }

    /// The world box around this box turned by a rigid transform.
    ///
    /// The eight corners are transformed, so the result is the box of the turned box: tight for a
    /// translation, a quarter turn or any other turn that maps an axis to an axis, and never smaller
    /// than the box of the transformed points for any other rotation — a conservative box is what
    /// framing, culling and picking need.
    pub fn transformed_by(self, transform: DMat4) -> Self {
        Self::of_points(self.corners().into_iter().map(|corner| transform.transform_point3(corner))).unwrap_or(self)
    }

    /// The eight corners of the box.
    ///
    /// Bit 0 of the index picks the `x` maximum, bit 1 the `y` and bit 2 the `z`, so the two corners
    /// that differ in one bit share one edge of the box — which is what a bracket needs to draw its
    /// three arms along.
    pub fn corners(self) -> [DVec3; 8] {
        let mut corners = [DVec3::ZERO; 8];
        for (index, corner) in corners.iter_mut().enumerate() {
            let pick = |bit: usize, low: f64, high: f64| if index & bit == 0 { low } else { high };
            *corner = DVec3::new(
                pick(1, self.min.x, self.max.x),
                pick(2, self.min.y, self.max.y),
                pick(4, self.min.z, self.max.z),
            );
        }
        corners
    }
}

/// An indexed triangle mesh in local millimetres, ready for the renderer.
///
/// Positions and normals are `f32` because this is the rendering boundary; the geometry kernel
/// keeps `f64`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeshData {
    /// Vertex positions, three floats per vertex.
    pub positions: Vec<[f32; 3]>,
    /// Vertex normals, three floats per vertex; empty when the mesh is flat-shaded per face.
    pub normals: Vec<[f32; 3]>,
    /// Triangle indices, three per triangle.
    pub indices: Vec<u32>,
}

impl MeshData {
    /// Number of vertices.
    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    /// Number of triangles.
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// True when the mesh has no triangles.
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// The box around the vertices in the mesh's own millimetres, or `None` when it has no finite
    /// vertex.
    ///
    /// Derived once per mesh and kept beside it: a scene walk then costs eight corners per drawable
    /// instead of a transform per vertex (X56).
    pub fn local_bounds(&self) -> Option<Aabb> {
        Aabb::of_points(
            self.positions
                .iter()
                .map(|position| DVec3::new(f64::from(position[0]), f64::from(position[1]), f64::from(position[2]))),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_frame_is_orthonormal() {
        assert!(Frame::WORLD.is_orthonormal());
    }

    #[test]
    fn frame_orthogonalises_axes() {
        let frame =
            Frame::from_origin_axes(DVec3::new(1.0, 2.0, 3.0), DVec3::new(2.0, 0.0, 0.0), DVec3::new(1.0, 3.0, 0.0));
        let frame = frame.expect("valid frame");
        assert!(frame.is_orthonormal());
        assert!((frame.x - DVec3::X).length() < 1e-12);
        assert!((frame.y - DVec3::Y).length() < 1e-12);
        assert!((frame.z - DVec3::Z).length() < 1e-12);
    }

    #[test]
    fn degenerate_frame_is_rejected() {
        assert!(Frame::from_origin_axes(DVec3::ZERO, DVec3::ZERO, DVec3::Y).is_none());
        assert!(Frame::from_origin_axes(DVec3::new(f64::NAN, 0.0, 0.0), DVec3::X, DVec3::Y).is_none());
    }

    #[test]
    fn mesh_counts() {
        let mesh = MeshData { positions: vec![[0.0; 3]; 3], normals: Vec::new(), indices: vec![0, 1, 2] };
        assert_eq!(mesh.vertex_count(), 3);
        assert_eq!(mesh.triangle_count(), 1);
        assert!(!mesh.is_empty());
    }

    #[test]
    fn a_point_cloud_box_holds_its_points_and_skips_the_infinite_ones() {
        let points = [DVec3::new(1.0, -2.0, 3.0), DVec3::new(-4.0, 5.0, 0.0), DVec3::new(f64::NAN, 0.0, 0.0)];
        let box_ = Aabb::of_points(points).expect("a finite point");
        assert_eq!(box_.min(), DVec3::new(-4.0, -2.0, 0.0));
        assert_eq!(box_.max(), DVec3::new(1.0, 5.0, 3.0));
        assert_eq!(box_.center(), DVec3::new(-1.5, 1.5, 1.5));
        assert_eq!(box_.corners().len(), 8);
        assert_eq!(box_.corners()[0], box_.min(), "bit 0 picks the `x` maximum, so zero is the minimum corner");
        assert_eq!(box_.corners()[7], box_.max());
        assert!(Aabb::of_points([DVec3::splat(f64::INFINITY)]).is_none(), "no finite point, no box");
    }

    #[test]
    fn a_quarter_turn_carries_the_box_exactly_and_any_other_turn_keeps_it_conservative() {
        let box_ = Aabb::of_points([DVec3::ZERO, DVec3::new(10.0, 20.0, 30.0)]).expect("a box");
        let turned = box_.transformed_by(DMat4::from_rotation_translation(
            DQuat::from_rotation_x(std::f64::consts::FRAC_PI_2),
            DVec3::new(100.0, 0.0, 0.0),
        ));
        // A quarter turn about X sends `y` to `z` and `z` to `-y`, so the box is exact.
        assert!((turned.min() - DVec3::new(100.0, -30.0, 0.0)).length() < 1.0e-12);
        assert!((turned.max() - DVec3::new(110.0, 0.0, 20.0)).length() < 1.0e-12);

        let slant = box_.transformed_by(DMat4::from_rotation_translation(
            DQuat::from_rotation_z(std::f64::consts::FRAC_PI_4),
            DVec3::ZERO,
        ));
        // A 45 degree turn widens the 10 x 20 footprint to (10 + 20) cos 45, which is the box of the
        // turned box: wider than the box of the turned vertices by the four corner gaps.
        let widened = 30.0 * std::f64::consts::FRAC_PI_4.cos();
        assert!(
            (slant.max().x - slant.min().x - widened).abs() < 1.0e-9,
            "a slanted box is the box of the turned box, never a smaller one"
        );
    }

    #[test]
    fn a_mesh_measures_its_own_box() {
        let mut mesh = MeshData { positions: vec![[0.0, 0.0, 0.0], [100.0, 200.0, 40.0]], ..MeshData::default() };
        let box_ = mesh.local_bounds().expect("the mesh has vertices");
        assert_eq!(box_.min(), DVec3::ZERO);
        assert_eq!(box_.max(), DVec3::new(100.0, 200.0, 40.0));
        mesh.positions.clear();
        assert!(mesh.local_bounds().is_none(), "an empty mesh has no box");
    }
}
