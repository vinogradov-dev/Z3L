//! Builds the polygon mesh of a parametric primitive.
//!
//! The parameters are document data in integer micrometres; this module is the boundary where the
//! ring trigonometry runs in floating point and the result is rounded back to lengths. The sphere is
//! latitude bands joined by triangles at the poles, spaced by equal steps of latitude rather than of
//! `z`: uniform `z` crowds every band into the poles and leaves a cap as wide as half the radius
//! (X55). The cone is two circles and the band between them, or one circle and an apex when its top
//! radius is zero. A box or a cylinder is a drawn profile swept by `crates/param`, not a parameter
//! set (ADR 0138).

use model::{Cone, Mesh, Primitive, Sphere};
use units::Length;

use crate::MeshError;

/// The sine and cosine of one segment around a ring, in the XY plane.
fn ring_angle(segment: usize, segments: usize) -> (f64, f64) {
    let angle = std::f64::consts::TAU * segment as f64 / segments as f64;
    angle.sin_cos()
}

/// Rounds a float micrometre coordinate into a document length.
fn coordinate(value: f64) -> Result<Length, MeshError> {
    if !value.is_finite() || value < i64::MIN as f64 || value > i64::MAX as f64 {
        return Err(MeshError::Invalid("a primitive coordinate is not a representable length".into()));
    }
    Ok(Length::from_micrometres(value.round() as i64))
}

/// Converts a vertex count to an index, refusing one that would overflow the index buffer.
fn index(value: usize) -> Result<u32, MeshError> {
    u32::try_from(value).map_err(|_| MeshError::Invalid("a primitive has too many vertices to index".into()))
}

/// Builds the document mesh of one primitive.
pub(crate) fn build_primitive_mesh(primitive: &Primitive) -> Result<Mesh, MeshError> {
    match primitive {
        Primitive::Sphere(sphere) => sphere_mesh(sphere),
        Primitive::Cone(cone) => cone_mesh(cone),
    }
}

/// A ring of `segments` vertices at `radius`, at height `z`.
///
/// Counter-clockwise seen from `+Z`, which is the order both caps and every band are wound from.
fn push_ring(positions: &mut Vec<[Length; 3]>, radius: f64, z: Length, segments: usize) -> Result<(), MeshError> {
    for segment in 0..segments {
        let (sin, cos) = ring_angle(segment, segments);
        positions.push([coordinate(cos * radius)?, coordinate(sin * radius)?, z]);
    }
    Ok(())
}

/// One circle on the base plane, one above it, and the band between them.
///
/// The base circle is the round the maker drew, so its centre is the local zero and the body rises on
/// `+Z` (ADR 0141). A top radius of zero replaces the second ring with one apex vertex and the band
/// with triangles, so a sharp cone has no degenerate top cap.
///
/// [ADR 0141]: ../docs/decisions/0141-a-cone-is-drawn-by-its-base-circle.md
fn cone_mesh(cone: &Cone) -> Result<Mesh, MeshError> {
    let segments = cone.segments() as usize;
    let top_radius = cone.top_radius().micrometres() as f64;
    let height = cone.height();
    let mut positions = Vec::with_capacity(segments * 2 + 1);
    push_ring(&mut positions, cone.base_radius().micrometres() as f64, Length::ZERO, segments)?;
    let apex = if cone.is_sharp() {
        let apex = index(positions.len())?;
        positions.push([Length::ZERO, Length::ZERO, height]);
        Some(apex)
    } else {
        push_ring(&mut positions, top_radius, height, segments)?;
        None
    };
    let top_ring = |segment: usize| -> Result<u32, MeshError> { index(segments + segment % segments) };
    let mut faces = Vec::with_capacity(segments + 2);
    // The base cap faces away from the body, so it is wound clockwise seen from `+Z`.
    faces.push((0..segments).rev().map(|segment| segment as u32).collect());
    for segment in 0..segments {
        let next = (segment + 1) % segments;
        let lower = index(segment)?;
        let lower_next = index(next)?;
        match apex {
            Some(apex) => faces.push(vec![lower, lower_next, apex]),
            None => faces.push(vec![lower, lower_next, top_ring(next)?, top_ring(segment)?]),
        }
    }
    if apex.is_none() {
        faces.push((segments..segments * 2).map(|vertex| vertex as u32).collect());
    }
    Ok(Mesh::new(positions, faces)?)
}

/// A closed banded surface with one vertex at each pole.
fn sphere_mesh(sphere: &Sphere) -> Result<Mesh, MeshError> {
    let segments = sphere.segments() as usize;
    let bands = sphere.bands() as usize;
    let radius = sphere.radius().micrometres() as f64;
    let mut positions = Vec::with_capacity(segments * (bands - 1) + 2);
    positions.push([Length::ZERO, Length::ZERO, coordinate(-radius)?]);
    for band in 1..bands {
        let latitude = std::f64::consts::PI * band as f64 / bands as f64;
        let (sin, cos) = latitude.sin_cos();
        let z = -radius * cos;
        let ring = radius * sin;
        for segment in 0..segments {
            let (sin, cos) = ring_angle(segment, segments);
            positions.push([coordinate(cos * ring)?, coordinate(sin * ring)?, coordinate(z)?]);
        }
    }
    let top = index(positions.len())?;
    positions.push([Length::ZERO, Length::ZERO, coordinate(radius)?]);
    let ring_index = |band: usize, segment: usize| -> Result<u32, MeshError> {
        index(1 + (band - 1) * segments + segment % segments)
    };
    let mut faces = Vec::new();
    for segment in 0..segments {
        let next = (segment + 1) % segments;
        faces.push(vec![0, ring_index(1, next)?, ring_index(1, segment)?]);
    }
    for band in 1..bands - 1 {
        for segment in 0..segments {
            let next = (segment + 1) % segments;
            faces.push(vec![
                ring_index(band, segment)?,
                ring_index(band, next)?,
                ring_index(band + 1, next)?,
                ring_index(band + 1, segment)?,
            ]);
        }
    }
    for segment in 0..segments {
        let next = (segment + 1) % segments;
        faces.push(vec![top, ring_index(bands - 1, segment)?, ring_index(bands - 1, next)?]);
    }
    Ok(Mesh::new(positions, faces)?)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;
    use model::{DEFAULT_BANDS, DEFAULT_SEGMENTS};
    use testkit::millimetres;

    /// Newell's normal of a polygon.
    fn normal(mesh: &Mesh, face: &[u32]) -> [f64; 3] {
        let point = |index: u32| {
            let position = mesh.positions()[index as usize];
            [position[0].millimetres_f64(), position[1].millimetres_f64(), position[2].millimetres_f64()]
        };
        let mut normal = [0.0; 3];
        for (index, vertex) in face.iter().enumerate() {
            let current = point(*vertex);
            let next = point(face[(index + 1) % face.len()]);
            normal[0] += (current[1] - next[1]) * (current[2] + next[2]);
            normal[1] += (current[2] - next[2]) * (current[0] + next[0]);
            normal[2] += (current[0] - next[0]) * (current[1] + next[1]);
        }
        normal
    }

    /// Every face of a convex primitive centred on the origin points away from it.
    fn assert_outward(mesh: &Mesh) {
        for (index, face) in mesh.faces().iter().enumerate() {
            let mut centroid = [0.0; 3];
            for vertex in face {
                let position = mesh.positions()[*vertex as usize];
                for axis in 0..3 {
                    centroid[axis] += position[axis].millimetres_f64() / face.len() as f64;
                }
            }
            let normal = normal(mesh, face);
            let dot = (0..3).map(|axis| normal[axis] * centroid[axis]).sum::<f64>();
            assert!(dot > 0.0, "face {index} at {centroid:?} has normal {normal:?} pointing inward");
        }
    }

    #[test]
    fn a_sphere_is_a_closed_banded_surface_of_the_stored_radius() {
        let primitive = Primitive::sphere(millimetres(50), 16, 8).expect("sphere");
        let mesh = build_primitive_mesh(&primitive).expect("mesh");
        assert_eq!(mesh.positions().len(), primitive.vertex_count());
        assert_eq!(mesh.faces().len(), primitive.face_count());
        assert_eq!(mesh.bounds_micrometres(), ([-50_000; 3], [50_000; 3]));
        assert_outward(&mesh);
    }

    #[test]
    fn the_default_tessellation_still_builds_the_same_counts() {
        let primitive = Primitive::sphere(millimetres(50), DEFAULT_SEGMENTS, DEFAULT_BANDS).expect("sphere");
        let mesh = build_primitive_mesh(&primitive).expect("mesh");
        let segments = DEFAULT_SEGMENTS as usize;
        let bands = DEFAULT_BANDS as usize;
        assert_eq!(mesh.positions().len(), 2 + segments * (bands - 1));
        assert_eq!(mesh.faces().len(), segments * bands);
    }

    /// One latitude ring's (distance from the axis, height) in micrometres, read from its first
    /// vertex; every vertex of a ring shares both within rounding.
    fn ring_section(mesh: &Mesh, band: usize, segments: usize) -> (f64, f64) {
        let vertex = mesh.positions()[1 + (band - 1) * segments];
        let radius = (vertex[0].micrometres() as f64).hypot(vertex[1].micrometres() as f64);
        (radius, vertex[2].micrometres() as f64)
    }

    #[test]
    fn the_pole_ring_sits_one_latitude_step_from_the_pole() {
        let radius = 50_000.0;
        let bands = DEFAULT_BANDS as usize;
        let primitive = Primitive::sphere(millimetres(50), DEFAULT_SEGMENTS, DEFAULT_BANDS).expect("sphere");
        let mesh = build_primitive_mesh(&primitive).expect("mesh");
        let (ring, z) = ring_section(&mesh, 1, DEFAULT_SEGMENTS as usize);
        let step = std::f64::consts::PI / bands as f64;
        // Uniform `z` spacing left this ring at 0.484 r from the axis; one latitude step is 0.195 r.
        assert!((ring - radius * step.sin()).abs() <= 1.5, "the pole ring radius is {ring}");
        assert!((z + radius * step.cos()).abs() <= 1.0, "the pole ring height is {z}");
    }

    #[test]
    fn the_latitude_bands_are_equal_meridian_steps() {
        let radius = 50_000.0;
        let segments = DEFAULT_SEGMENTS as usize;
        let bands = DEFAULT_BANDS as usize;
        let primitive = Primitive::sphere(millimetres(50), DEFAULT_SEGMENTS, DEFAULT_BANDS).expect("sphere");
        let mesh = build_primitive_mesh(&primitive).expect("mesh");
        // The meridian section from pole to pole: the two poles and a point per ring.
        let mut section = vec![(0.0, -radius)];
        for band in 1..bands {
            section.push(ring_section(&mesh, band, segments));
        }
        section.push((0.0, radius));
        let ideal = 2.0 * radius * (std::f64::consts::PI / (2.0 * bands as f64)).sin();
        let mut spans = Vec::with_capacity(section.len() - 1);
        for pair in section.windows(2) {
            let (low, high) = (pair[0], pair[1]);
            let span = (high.0 - low.0).hypot(high.1 - low.1);
            assert!((span - ideal).abs() <= 1.5, "a band spans {span}, one meridian step is {ideal}");
            spans.push(span);
        }
        let low = spans.iter().copied().fold(f64::INFINITY, f64::min);
        let high = spans.iter().copied().fold(0.0_f64, f64::max);
        assert!(high <= 2.0 * low, "the widest band {high} is more than twice the narrowest {low}");
    }

    /// The signed volume of a closed mesh: positive when every face is wound outwards.
    fn signed_volume(mesh: &Mesh) -> f64 {
        let point = |index: u32| {
            let position = mesh.positions()[index as usize];
            [position[0].millimetres_f64(), position[1].millimetres_f64(), position[2].millimetres_f64()]
        };
        let mut volume = 0.0;
        for face in mesh.faces() {
            for pair in 1..face.len() - 1 {
                let [a, b, c] = [point(face[0]), point(face[pair]), point(face[pair + 1])];
                volume += (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                    + a[2] * (b[0] * c[1] - b[1] * c[0]))
                    / 6.0;
            }
        }
        volume
    }

    #[test]
    fn a_truncated_cone_is_the_analytic_frustum_to_its_own_winding() {
        let (base, top, height) = (40.0_f64, 20.0_f64, 100.0_f64);
        let primitive =
            Primitive::cone(millimetres(40), millimetres(20), millimetres(100), DEFAULT_SEGMENTS).expect("cone");
        let mesh = build_primitive_mesh(&primitive).expect("mesh");
        assert_eq!(mesh.positions().len(), primitive.vertex_count());
        assert_eq!(mesh.faces().len(), primitive.face_count());
        assert_eq!(mesh.bounds_micrometres(), ([-40_000, -40_000, 0], [40_000, 40_000, 100_000]));
        // A fan of 32 sides inscribes the circles, so the polygonal volume undershoots the analytic
        // one by about half a percent; the sign is the winding test, the magnitude the shape's.
        let analytic = std::f64::consts::PI * height * (base * base + base * top + top * top) / 3.0;
        let measured = signed_volume(&mesh);
        assert!(measured > 0.0, "every face is wound outwards, measured {measured} mm³");
        assert!((measured - analytic).abs() / analytic < 0.01, "measured {measured} mm³, analytic {analytic}");
    }

    #[test]
    fn a_sharp_cone_ends_in_one_apex_and_one_cap() {
        let primitive =
            Primitive::cone(millimetres(40), Length::ZERO, millimetres(100), DEFAULT_SEGMENTS).expect("a sharp cone");
        let mesh = build_primitive_mesh(&primitive).expect("mesh");
        let segments = DEFAULT_SEGMENTS as usize;
        assert_eq!(mesh.positions().len(), segments + 1, "one ring and the apex");
        assert_eq!(mesh.faces().len(), segments + 1, "the band and the base cap, and no top face");
        // Every side face is a triangle ending in the apex, and only the base cap has a ring's worth
        // of corners: a degenerate top cap would show up as a second polygon here.
        let triangles = mesh.faces().iter().filter(|face| face.len() == 3).count();
        assert_eq!(triangles, segments, "every band face is a triangle");
        assert_eq!(mesh.faces().iter().filter(|face| face.len() == segments).count(), 1, "one base cap");
        let analytic = std::f64::consts::PI * 40.0 * 40.0 * 100.0 / 3.0;
        let measured = signed_volume(&mesh);
        assert!(measured > 0.0, "the apex closes the body, measured {measured} mm³");
        assert!((measured - analytic).abs() / analytic < 0.01, "measured {measured} mm³, analytic {analytic}");
    }

    #[test]
    fn a_cone_whose_top_outgrows_its_base_is_still_closed() {
        let primitive =
            Primitive::cone(millimetres(10), millimetres(40), millimetres(100), DEFAULT_SEGMENTS).expect("splayed");
        let mesh = build_primitive_mesh(&primitive).expect("mesh");
        let analytic = std::f64::consts::PI * 100.0 * (10.0 * 10.0 + 10.0 * 40.0 + 40.0 * 40.0) / 3.0;
        let measured = signed_volume(&mesh);
        assert!((measured - analytic).abs() / analytic < 0.01, "measured {measured} mm³, analytic {analytic}");
    }

    #[test]
    fn changing_the_sides_changes_the_mesh() {
        let coarse = Primitive::sphere(millimetres(50), 8, 4).expect("coarse");
        let fine = Primitive::sphere(millimetres(50), 16, 8).expect("fine");
        let coarse = build_primitive_mesh(&coarse).expect("coarse mesh");
        let fine = build_primitive_mesh(&fine).expect("fine mesh");
        assert_ne!(coarse.positions().len(), fine.positions().len());
        assert_ne!(coarse.faces().len(), fine.faces().len());
    }
}
