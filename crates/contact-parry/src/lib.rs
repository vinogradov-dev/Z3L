//! The contact port implemented with `parry3d-f64`, in millimetres.
//!
//! `parry3d-f64` 0.31 is the double-precision build of the parry collision library: Apache-2.0, pure
//! Rust, no build script, and glam-based, so a Z3L vector and a parry vector are the same three
//! numbers and no conversion is lost between them. The double build is the one that matters here: the
//! document stores micrometres and a workshop is metres wide, which a single-precision build cannot
//! resolve (X85).
//!
//! This crate is the **only** place the third-party engine appears, exactly as `kernel-truck` is the
//! only place `truck` appears: `cargo xtask layers` fails otherwise. The port speaks millimetres, so
//! no unit conversion happens here — a parry distance is a Z3L millimetre.
//!
//! A convex hull and a triangle surface are the two shapes the document builds: a cone, a cylinder, a
//! ball, a box and a straight panel are hulls of their own points; a notched panel, a carcass with a
//! hole or a compound of convex parts is a closed triangle surface, which parry measures exactly.
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

use geom::{Contact, DVec3, Penetration, Pose, Shape, Shared, Solid};
use std::any::Any;
use std::sync::Arc;

use parry3d_f64::math::{Pose as ParryPose, Rot3, Vec3};
use parry3d_f64::query::{ShapeCastOptions, cast_shapes, contact as parry_contact};
use parry3d_f64::shape::SharedShape;

/// The furthest a single ask may travel, in millimetres: ten metres past any workshop.
const MAX_TRAVEL_MM: f64 = 10_000.0;

/// The contact port, answered by parry.
#[derive(Clone, Copy, Debug, Default)]
pub struct Parry;

/// One body as parry holds it: the shape and the pose it stands at.
///
/// A rule keeps as many of these as it measures, so the conversion from a Z3L shape happens once per
/// gesture; `shifted` moves the pose and shares the shape.
#[derive(Debug)]
struct Body {
    shape: SharedShape,
    pose: ParryPose,
}

impl Contact for Parry {
    fn prepare(&self, shape: &Shape, pose: Pose) -> Option<Shared> {
        let shape = match shape {
            Shape::Convex(points) => SharedShape::convex_hull(&points.iter().copied().map(vector).collect::<Vec<_>>())?,
            Shape::Mesh { vertices, triangles } => {
                SharedShape::trimesh(vertices.iter().copied().map(vector).collect(), triangles.clone()).ok()?
            }
        };
        Some(Arc::new(Body { shape, pose: parry_pose(pose) }) as Shared)
    }
}

impl Solid for Body {
    fn bounds(&self) -> (DVec3, DVec3) {
        let aabb = self.shape.compute_aabb(&self.pose);
        (point(aabb.mins), point(aabb.maxs))
    }

    fn shifted(&self, delta: DVec3) -> Option<Shared> {
        if !delta.is_finite() {
            return None;
        }
        let pose = ParryPose::from_parts(self.pose.translation + vector(delta), self.pose.rotation);
        Some(Arc::new(Body { shape: self.shape.clone(), pose }) as Shared)
    }

    fn travel(&self, other: &dyn Solid, direction: DVec3) -> Option<f64> {
        let body_b = as_body(other)?;
        let body_a = self;
        // The heading is taken as a heading: the answer is a distance in millimetres whatever the
        // vector's own length, which is what a caller passing a world axis and a caller passing the
        // vector between two places both mean.
        let length = direction.length();
        if !length.is_finite() || length <= 0.0 {
            return None;
        }
        let options = ShapeCastOptions {
            max_time_of_impact: MAX_TRAVEL_MM,
            target_distance: 0.0,
            // A pair that is already touching answers zero instead of being cast through: a drag
            // standing on a face must be able to say it stands on it.
            stop_at_penetration: true,
            compute_impact_geometry_on_penetration: false,
        };
        let hit = cast_shapes(
            &body_a.pose,
            vector(direction / length),
            &*body_a.shape,
            &body_b.pose,
            Vec3::ZERO,
            &*body_b.shape,
            options,
        )
        .ok()??;
        let travel = hit.time_of_impact;
        travel.is_finite().then_some(travel)
    }

    fn penetration(&self, other: &dyn Solid) -> Option<Penetration> {
        let body_b = as_body(other)?;
        // A zero prediction asks for the touching pair as well, so a face that only touches answers a
        // distance of zero and is left to the caller's slack.
        let found = parry_contact(&self.pose, &*self.shape, &body_b.pose, &*body_b.shape, 0.0).ok()??;
        if found.dist >= 0.0 {
            return None;
        }
        let depth_mm = -found.dist;
        depth_mm.is_finite().then_some(Penetration { depth_mm, normal: point(found.normal1) })
    }
}

/// This adapter's own body, when the other side of a pair was built by it.
///
/// A contract is not a licence to guess: a body of another adapter answers `None` rather than being
/// measured as something it is not.
fn as_body(solid: &dyn Solid) -> Option<&Body> {
    (solid as &dyn Any).downcast_ref::<Body>()
}

/// A Z3L millimetre point as parry's vector: the same three numbers, and the same type in the double
/// build, so this is a boundary written down rather than a conversion.
fn vector(value: DVec3) -> Vec3 {
    Vec3::new(value.x, value.y, value.z)
}

/// A parry vector as a Z3L millimetre point.
fn point(value: Vec3) -> DVec3 {
    DVec3::new(value.x, value.y, value.z)
}

/// A port pose as parry's frame.
///
/// parry 0.31 speaks the `glam` its own `glamx` was built against, which is one minor version behind
/// the workspace's, so the rotation crosses this boundary as four numbers rather than as a type. The
/// conversion is exact for a unit quaternion.
fn parry_pose(pose: Pose) -> ParryPose {
    let rotation = pose.rotation;
    ParryPose::from_parts(vector(pose.translation), Rot3::from_xyzw(rotation.x, rotation.y, rotation.z, rotation.w))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use geom::DQuat;

    use super::*;

    /// A box `size` millimetres on a side, centred on its own origin.
    fn cube(size: f64) -> Shape {
        let half = size / 2.0;
        let corners = [
            (-1.0, -1.0, -1.0),
            (1.0, -1.0, -1.0),
            (-1.0, 1.0, -1.0),
            (1.0, 1.0, -1.0),
            (-1.0, -1.0, 1.0),
            (1.0, -1.0, 1.0),
            (-1.0, 1.0, 1.0),
            (1.0, 1.0, 1.0),
        ];
        Shape::convex(corners.into_iter().map(|(x, y, z)| DVec3::new(x * half, y * half, z * half)).collect())
            .expect("a cube is a convex shape")
    }

    /// A ball of `radius` millimetres, as the hull of a tessellated sphere.
    fn ball(radius: f64) -> Shape {
        let rings = 8;
        let steps = 16;
        let mut points = vec![DVec3::new(0.0, 0.0, radius), DVec3::new(0.0, 0.0, -radius)];
        for ring in 1..rings {
            let polar = std::f64::consts::PI * ring as f64 / rings as f64;
            let (height, around) = (polar.cos(), polar.sin());
            for step in 0..steps {
                let angle = std::f64::consts::TAU * step as f64 / steps as f64;
                points.push(DVec3::new(radius * around * angle.cos(), radius * around * angle.sin(), radius * height));
            }
        }
        Shape::convex(points).expect("a ball is a convex shape")
    }

    fn prepared(shape: &Shape, pose: Pose) -> Shared {
        Parry.prepare(shape, pose).expect("parry measures a closed shape")
    }

    fn at(x: f64, y: f64, z: f64) -> Pose {
        Pose::new(DVec3::new(x, y, z), DQuat::IDENTITY)
    }

    #[test]
    fn a_box_pair_answers_its_sunk_depth() {
        let (a, b) = (prepared(&cube(100.0), at(0.0, 0.0, 0.0)), prepared(&cube(100.0), at(50.0, 0.0, 0.0)));
        let sunk = a.penetration(&*b).expect("the cubes overlap");
        assert!((sunk.depth_mm - 50.0).abs() < 1.0e-6, "was {sunk:?}");
        assert!(sunk.normal.x.abs() > 0.99, "and the way out is along X: {sunk:?}");
        assert!(sunk.is_overlap(0.001), "one micrometre is the document's rounding");

        // A pair that only touches is contact, and one that stands apart shares nothing.
        assert_eq!(a.penetration(&*prepared(&cube(100.0), at(100.0, 0.0, 0.0))), None, "a face is contact");
        assert_eq!(a.penetration(&*prepared(&cube(100.0), at(400.0, 0.0, 0.0))), None, "and this pair is apart");
        assert!(geom::overlap(&*a, &*b, 0.001), "50 mm is deeper than the document's rounding");
    }

    #[test]
    fn a_cast_stops_where_the_shapes_first_touch() {
        let a = prepared(&cube(100.0), at(0.0, 0.0, 0.0));
        let b = prepared(&cube(100.0), at(300.0, 0.0, 0.0));
        let travel = a.travel(&*b, DVec3::X);
        assert_eq!(travel.map(|mm| mm.round()), Some(200.0), "the near faces meet after 200 mm");
        assert_eq!(a.travel(&*b, DVec3::NEG_X), None, "the other way is clear");
        // The heading is a heading: a vector twice as long names the same touch.
        assert_eq!(a.travel(&*b, DVec3::X * 7.0).map(|mm| mm.round()), Some(200.0));
    }

    #[test]
    fn a_turned_board_is_measured_by_its_own_faces() {
        // ADR 0165's case: a 200 x 20 x 10 board turned 45 degrees reaches x = 141.42 with its world
        // box, so a box reading stops it at 500 - 141.42 = 358.58 mm. Its own corner at the height of
        // the neighbour is what meets the neighbour, and the board's face touches after 400 mm.
        let mut points = Vec::new();
        for x in [-100.0, 100.0] {
            for y in [-10.0, 10.0] {
                for z in [-5.0, 5.0] {
                    points.push(DVec3::new(x, y, z));
                }
            }
        }
        let quarter = std::f64::consts::FRAC_PI_4;
        let turned = Pose::new(DVec3::ZERO, DQuat::from_rotation_z(quarter));
        let board = prepared(&Shape::convex(points).expect("a board is a convex shape"), turned);
        let block = prepared(&cube(100.0), at(500.0, 0.0, 0.0));
        let travel = board.travel(&*block, DVec3::X).expect("the board meets the block");
        // The world box around the board reaches x = 77.78, so a box reading stops it at 372.22; the
        // board's own edge at the block's corner y = 50 only reaches x = 64.14, so it travels to
        // 450 - 64.14 = 385.86. The difference is the corner the box would have invented.
        let boxed = 450.0 - 77.78;
        assert!(travel > boxed + 1.0, "the box around the board is not the answer: {travel} vs {boxed}");
        assert!((travel - 385.86).abs() < 0.1, "the board's own edge meets at 385.86 mm: {travel}");
    }

    #[test]
    fn a_ball_slips_through_a_hole_and_rubs_its_rim() {
        let plate = prepared(&plate_with_a_hole(60.0), Pose::IDENTITY);
        // A ball 20 mm across the hole's 60 mm falls through it: the way is clear.
        let straight = prepared(&ball(20.0), at(0.0, 0.0, 200.0));
        assert_eq!(straight.travel(&*plate, DVec3::NEG_Z), None);
        // Moved onto the rim it meets the plate's own corner, where its surface is 20 mm from the
        // centre: the ball's centre comes down to z = 25, so it travels 175 mm and no further.
        let rubbed = prepared(&ball(20.0), at(30.0, 0.0, 200.0));
        let rub = rubbed.travel(&*plate, DVec3::NEG_Z).expect("the ball meets the rim");
        assert!((rub - 175.0).abs() < 1.0, "the rim is 20 mm from the ball's centre: {rub}");
    }

    /// A 200 x 200 x 10 plate with a square hole `hole` millimetres wide through its middle.
    ///
    /// The ring of the top and the bottom face is three quads each; the hole's four walls and the
    /// outer four close the surface, so the mesh is a closed body that is not convex.
    fn plate_with_a_hole(hole: f64) -> Shape {
        let (outer, half, thick) = (100.0, hole / 2.0, 5.0);
        let mut vertices: Vec<DVec3> = Vec::new();
        let mut triangles: Vec<[u32; 3]> = Vec::new();
        let mut quad = |a: DVec3, b: DVec3, c: DVec3, d: DVec3| {
            let base = u32::try_from(vertices.len()).expect("a small mesh");
            vertices.extend([a, b, c, d]);
            triangles.extend([[base, base + 1, base + 2], [base, base + 2, base + 3]]);
        };
        // The top ring is three quads around the hole, then the same ring from below.
        for z in [thick, -thick] {
            let corner = |x: f64, y: f64| DVec3::new(x, y, z);
            quad(corner(-outer, -outer), corner(-half, -outer), corner(-half, outer), corner(-outer, outer));
            quad(corner(-half, -outer), corner(half, -outer), corner(half, -half), corner(-half, -half));
            quad(corner(-half, half), corner(half, half), corner(half, outer), corner(-half, outer));
        }
        // The hole's four walls.
        let walls = [(-half, -half), (half, -half), (half, half), (-half, half)];
        for (index, (x, y)) in walls.iter().copied().enumerate() {
            let (next_x, next_y) = walls[(index + 1) % walls.len()];
            let (low, high) = (-thick, thick);
            quad(
                DVec3::new(x, y, low),
                DVec3::new(next_x, next_y, low),
                DVec3::new(next_x, next_y, high),
                DVec3::new(x, y, high),
            );
        }
        Shape::mesh(vertices, triangles).expect("a plate is a triangle surface")
    }
}
