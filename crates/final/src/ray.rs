//! Rays and triangles: the one intersection test a final render starts from.
//!
//! A ray carries a unit direction, so `t` is a distance in millimetres and every test below compares
//! distances rather than parameters. A hit's normal is flipped to face the ray, so a two-sided
//! surface lights the same way from either side and no caller has to remember to check the sign.
//!
//! The test is Möller and Trumbore's, *Fast, Minimum Storage Ray/Triangle Intersection* (1997), for
//! its minimum storage and its one-sided early out; the slab test a box uses is the standard one,
//! with the per-axis reciprocal precomputed once per ray.
//!
//! Refuses: a ray with a zero or non-finite direction is `None`, not a direction of NaN that turns
//! every later comparison false; a triangle whose vertices are not finite is never hit, because its
//! determinant is not a number.

use geom::{Aabb, DVec3};

/// How close to parallel a ray and a triangle may be before the intersection is refused.
///
/// Lengths are millimetres, so the determinant is an area in mm²: a 100 mm² face gives one near 100,
/// and 1e-12 is a face seen exactly edge-on, which a render must treat as no hit rather than as a
/// division by almost nothing.
const PARALLEL_EPSILON: f64 = 1e-12;

/// A ray in millimetres, its direction unit length.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ray {
    origin: DVec3,
    direction: DVec3,
    inverse: DVec3,
}

impl Ray {
    /// A ray from `origin` towards `direction`, or `None` when either is not finite or the direction
    /// is zero.
    pub fn new(origin: DVec3, direction: DVec3) -> Option<Self> {
        if !origin.is_finite() || !direction.is_finite() {
            return None;
        }
        let length = direction.length();
        if length <= 0.0 || length.is_nan() {
            return None;
        }
        let direction = direction / length;
        let inverse = DVec3::new(1.0 / direction.x, 1.0 / direction.y, 1.0 / direction.z);
        Some(Self { origin, direction, inverse })
    }

    /// Where the ray starts, in world millimetres.
    pub fn origin(self) -> DVec3 {
        self.origin
    }

    /// The unit direction the ray travels along.
    pub fn direction(self) -> DVec3 {
        self.direction
    }

    /// The point at distance `t` along the ray.
    pub fn at(self, t: f64) -> DVec3 {
        self.origin + self.direction * t
    }

    /// The distance at which the ray enters `bounds`, or `None` when it misses inside
    /// `[t_min, t_max]`.
    ///
    /// The test is inclusive at both ends, because a point on a box's face is on the box; a component
    /// of the direction that is zero gives an infinite reciprocal, and the slab comparison then
    /// answers "inside" or "outside" without a special case for that axis. The distance is what
    /// traversal orders children by, so the nearer child is entered first.
    pub fn bounds_entry(self, bounds: Aabb, t_min: f64, t_max: f64) -> Option<f64> {
        let (min, max) = (bounds.min(), bounds.max());
        let mut entry = t_min;
        let mut exit = t_max;
        for axis in 0..3 {
            // A direction exactly along another axis gives an infinite reciprocal, and `0 * inf` is
            // not a number: the axis is a slab with no thickness to cross, so it is decided by
            // whether the origin is inside it.
            if self.inverse[axis].is_infinite() {
                if self.origin[axis] < min[axis] || self.origin[axis] > max[axis] {
                    return None;
                }
                continue;
            }
            let near = (min[axis] - self.origin[axis]) * self.inverse[axis];
            let far = (max[axis] - self.origin[axis]) * self.inverse[axis];
            let (near, far) = if near <= far { (near, far) } else { (far, near) };
            entry = entry.max(near);
            exit = exit.min(far);
            if exit < entry {
                return None;
            }
        }
        Some(entry)
    }

    /// Whether the ray meets `bounds` at a distance inside `[t_min, t_max]`.
    pub fn hits_bounds(self, bounds: Aabb, t_min: f64, t_max: f64) -> bool {
        self.bounds_entry(bounds, t_min, t_max).is_some()
    }
}

/// What a ray met: the distance, the normal facing the ray and the barycentric coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    /// The distance from the ray's origin, in millimetres.
    pub t: f64,
    /// The triangle's geometric normal, flipped to face the ray.
    pub normal: DVec3,
    /// The barycentric weight of the second vertex.
    pub u: f64,
    /// The barycentric weight of the third vertex.
    pub v: f64,
}

/// Three vertices in world millimetres, wound counter-clockwise as seen from the outside.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Triangle {
    /// The first vertex.
    pub v0: DVec3,
    /// The second vertex.
    pub v1: DVec3,
    /// The third vertex.
    pub v2: DVec3,
}

impl Triangle {
    /// A triangle from its three vertices.
    pub fn new(v0: DVec3, v1: DVec3, v2: DVec3) -> Self {
        Self { v0, v1, v2 }
    }

    /// The three vertices, in winding order.
    pub fn vertices(self) -> [DVec3; 3] {
        [self.v0, self.v1, self.v2]
    }

    /// The box around the triangle, or `None` when a vertex is not finite.
    ///
    /// A box built from `Aabb::of_points` skips a bad point rather than refusing it, which is right
    /// for a mesh and wrong here: one NaN vertex would leave a box that claims a triangle exists and
    /// an intersection test that can never answer, so the whole triangle is refused instead.
    pub fn bounds(self) -> Option<Aabb> {
        if !self.vertices().iter().all(|vertex| vertex.is_finite()) {
            return None;
        }
        Aabb::of_points(self.vertices())
    }

    /// The average of the three vertices, which the BVH bins by.
    pub fn centroid(self) -> DVec3 {
        (self.v0 + self.v1 + self.v2) / 3.0
    }

    /// The unit normal implied by the winding, or `None` for a triangle with no area.
    pub fn normal(self) -> Option<DVec3> {
        let normal = (self.v1 - self.v0).cross(self.v2 - self.v0);
        let length = normal.length();
        if length <= 0.0 || !normal.is_finite() {
            return None;
        }
        Some(normal / length)
    }

    /// The nearest hit on this triangle at a distance inside `[t_min, t_max]`.
    pub fn intersect(self, ray: Ray, t_min: f64, t_max: f64) -> Option<Hit> {
        let edge1 = self.v1 - self.v0;
        let edge2 = self.v2 - self.v0;
        let pvec = ray.direction.cross(edge2);
        let determinant = edge1.dot(pvec);
        if determinant.abs() <= PARALLEL_EPSILON {
            return None;
        }
        let inverse = 1.0 / determinant;
        let tvec = ray.origin - self.v0;
        let u = tvec.dot(pvec) * inverse;
        if !(0.0..=1.0).contains(&u) {
            return None;
        }
        let qvec = tvec.cross(edge1);
        let v = ray.direction.dot(qvec) * inverse;
        if v < 0.0 || u + v > 1.0 {
            return None;
        }
        let t = edge2.dot(qvec) * inverse;
        if !(t_min..=t_max).contains(&t) {
            return None;
        }
        let normal = self.normal()?;
        // A hit faces the ray: a render lights a face from both sides, and a caller that forgets the
        // flip would light a closed body's inside as if it were the outside.
        let normal = if normal.dot(ray.direction) > 0.0 { -normal } else { normal };
        Some(Hit { t, normal, u, v })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    fn plane() -> Triangle {
        // A two-metre square on the XY plane, wound towards +Z.
        Triangle::new(DVec3::new(0.0, 0.0, 0.0), DVec3::new(2000.0, 0.0, 0.0), DVec3::new(0.0, 2000.0, 0.0))
    }

    #[test]
    fn a_ray_meets_the_plane_where_it_should_and_the_normal_faces_it() {
        let triangle = plane();
        let ray = Ray::new(DVec3::new(500.0, 500.0, 1000.0), DVec3::new(0.0, 0.0, -1.0)).expect("a ray");
        let hit = triangle.intersect(ray, 0.0, f64::INFINITY).expect("the ray meets the plane");
        assert!((hit.t - 1000.0).abs() < 1e-9, "{hit:?}");
        assert!((hit.normal - DVec3::Z).length() < 1e-12, "the normal faces the ray: {hit:?}");
        let under = Ray::new(DVec3::new(500.0, 500.0, -1000.0), DVec3::new(0.0, 0.0, 1.0)).expect("a ray");
        let from_below = triangle.intersect(under, 0.0, f64::INFINITY).expect("two-sided");
        assert!((from_below.normal + DVec3::Z).length() < 1e-12, "the normal still faces the ray");
    }

    #[test]
    fn the_air_beside_the_plane_is_not_hit_and_a_zero_direction_is_no_ray() {
        let triangle = plane();
        let beside = Ray::new(DVec3::new(3000.0, 500.0, 1000.0), DVec3::new(0.0, 0.0, -1.0)).expect("a ray");
        assert!(triangle.intersect(beside, 0.0, f64::INFINITY).is_none());
        assert!(Ray::new(DVec3::ZERO, DVec3::ZERO).is_none());
        assert!(Ray::new(DVec3::new(f64::NAN, 0.0, 0.0), DVec3::Z).is_none());
        // Edge-on is no hit: the determinant is zero to within the epsilon.
        let along = Ray::new(DVec3::new(-100.0, 500.0, 0.0), DVec3::X).expect("a ray");
        assert!(triangle.intersect(along, 0.0, f64::INFINITY).is_none());
    }

    #[test]
    fn a_zero_area_triangle_has_no_normal_and_is_never_hit() {
        let flat = Triangle::new(DVec3::ZERO, DVec3::X, DVec3::X * 2.0);
        assert!(flat.normal().is_none());
        let ray = Ray::new(DVec3::new(0.5, 0.5, 10.0), -DVec3::Z).expect("a ray");
        assert!(flat.intersect(ray, 0.0, f64::INFINITY).is_none(), "a degenerate triangle is no face");
    }

    #[test]
    fn the_box_test_admits_the_faces_and_refuses_the_air() {
        let bounds = Aabb::of_points([DVec3::ZERO, DVec3::new(100.0, 100.0, 100.0)]).expect("a box");
        let through = Ray::new(DVec3::new(50.0, 50.0, -10.0), DVec3::Z).expect("a ray");
        assert!(through.hits_bounds(bounds, 0.0, f64::INFINITY));
        assert!(!through.hits_bounds(bounds, 0.0, 5.0), "the box is further than five millimetres");
        let aside = Ray::new(DVec3::new(150.0, 50.0, -10.0), DVec3::Z).expect("a ray");
        assert!(!aside.hits_bounds(bounds, 0.0, f64::INFINITY));
        // A ray exactly on a face is inside: the slabs are inclusive.
        let on_face = Ray::new(DVec3::new(0.0, 50.0, -10.0), DVec3::Z).expect("a ray");
        assert!(on_face.hits_bounds(bounds, 0.0, f64::INFINITY));
    }
}
