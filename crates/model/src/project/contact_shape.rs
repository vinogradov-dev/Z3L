//! The shape a node's own surface is, built from its parametric data and never from the render mesh.
//!
//! The contact port measures a body by its own faces (ADR 0179), and this module is what it measures:
//! a [`Shape`] in the node's local frame, in millimetres. What is drawn is a tessellation chosen for
//! the screen — a sphere's band count, a contour's chord tolerance — and a measurement must not follow
//! that choice, so the shape comes from the parameters the document stores.
//!
//! Three answers exist and only three:
//!
//! - an **object** with a baked mesh is its triangles, and one with a primitive is the convex hull of
//!   that primitive's own surface (a ball is a ball, whether it is drawn in eight bands or eighty);
//! - a **panel, an envelope and any other container** is the box the document already measures: a
//!   rectangular board *is* its box, and a contoured one is measured by the box around its outline
//!   until that outline's own extrusion is built here — the reading the furniture rule has always had
//!   (ADR 0162);
//! - anything this module cannot describe — a constructive body, a path, a mesh larger than the port's
//!   own caps — answers `None`, and the caller keeps the world-box reading it had. A fallback that
//!   measures less precisely is honest; a shape invented from a tessellation is not.
use geom::{DVec3, MAX_CONVEX_POINTS, MAX_MESH_TRIANGLES, MAX_MESH_VERTICES, Shape};

use crate::{Geometry, Id, Mesh, Primitive, Project};

/// The shape of one node's own surface in its local frame, in millimetres, or `None` when the box
/// around it is the answer.
///
/// `local_min` and `local_max` are the local box the move rule already keeps for this node, so every
/// fallback here is the reading the caller had before the port existed.
pub(crate) fn node_shape(project: &Project, id: Id, local_min: DVec3, local_max: DVec3) -> Option<Shape> {
    let object = project.object(id)?;
    geometry_shape(object.geometry()).or_else(|| box_between(local_min, local_max))
}

/// The box between `min` and `max` as a convex hull of its eight corners.
pub(crate) fn box_between(min: DVec3, max: DVec3) -> Option<Shape> {
    if !min.is_finite() || !max.is_finite() {
        return None;
    }
    let mut corners = Vec::with_capacity(8);
    for x in [min.x, max.x] {
        for y in [min.y, max.y] {
            for z in [min.z, max.z] {
                corners.push(DVec3::new(x, y, z));
            }
        }
    }
    Shape::convex(corners).ok()
}

/// The shape of a geometry payload, or `None` when only the geometry kernel could describe it.
fn geometry_shape(geometry: &Geometry) -> Option<Shape> {
    match geometry {
        Geometry::Mesh(mesh) => mesh_shape(mesh),
        Geometry::Primitive(primitive) => primitive_shape(primitive),
        // A constructive body's faces come from the kernel and a drawn path has no solid yet: neither
        // is this layer's to build, so the caller keeps the box reading.
        Geometry::Constructive(_) | Geometry::Path(_) => None,
    }
}

/// A baked mesh as a closed triangle surface, its polygons fanned into triangles.
///
/// The port caps both counts so a hostile file cannot size an allocation; a mesh past the cap answers
/// `None` and is measured by its box instead.
fn mesh_shape(mesh: &Mesh) -> Option<Shape> {
    let positions = mesh.positions();
    if positions.len() > MAX_MESH_VERTICES {
        return None;
    }
    let vertices: Vec<DVec3> = positions
        .iter()
        .map(|point| DVec3::new(point[0].millimetres_f64(), point[1].millimetres_f64(), point[2].millimetres_f64()))
        .collect();
    let mut triangles: Vec<[u32; 3]> = Vec::new();
    for face in mesh.faces() {
        let Some(first) = face.first().copied() else {
            continue;
        };
        for window in face.windows(3) {
            triangles.push([first, window[1], window[2]]);
            if triangles.len() > MAX_MESH_TRIANGLES {
                return None;
            }
        }
    }
    if triangles.is_empty() {
        return None;
    }
    // A convex body is a hull, and the difference is not cosmetic: a triangle surface is a skin, so a
    // body that lies wholly inside another would share no face with it and read as no overlap at all.
    // A box, a wedge and a turned board are all convex, so this is the common case and it is exact.
    if convex(&vertices, &triangles) {
        return Shape::convex(vertices).ok();
    }
    Shape::mesh(vertices, triangles).ok()
}

/// Whether a triangle surface is the boundary of a convex body.
///
/// Every vertex must lie on or behind every oriented triangle's plane; one vertex in front of one
/// plane is a dent, and a dent is what makes a surface a skin rather than a solid. The test is
/// quadratic, so it runs only for a mesh small enough to be worth it: a face count past the cap
/// answers `false` and the mesh is measured as the surface it is.
fn convex(vertices: &[DVec3], triangles: &[[u32; 3]]) -> bool {
    const MAX_CONVEXITY_WORK: usize = 200_000;
    if vertices.len().saturating_mul(triangles.len()) > MAX_CONVEXITY_WORK {
        return false;
    }
    const SLACK_MM: f64 = 1.0e-6;
    let mut volume = 0.0;
    for triangle in triangles {
        let [a, b, c] = triangle.map(|index| vertices.get(index as usize).copied().unwrap_or_default());
        let normal = (b - a).cross(c - a);
        if !normal.is_finite() || normal.length_squared() <= f64::EPSILON {
            return false;
        }
        let plane = normal.dot(a);
        if vertices.iter().any(|vertex| normal.dot(*vertex) - plane > SLACK_MM) {
            return false;
        }
        volume += a.dot(b.cross(c)) / 6.0;
    }
    // A flat sheet is behind every one of its own planes and encloses nothing: a hull of it is no
    // body, so the surface is the honest answer and the port measures the sheet it is.
    volume.abs() > SLACK_MM
}

/// A primitive as the convex hull of its own surface.
///
/// Both primitives are convex and both are stored as parameters, so their hull is a handful of points
/// that does not depend on how finely either is drawn: a ball is the points of the sphere it names, and
/// a cone the two circles it names.
fn primitive_shape(primitive: &Primitive) -> Option<Shape> {
    let points = match primitive {
        Primitive::Sphere(sphere) => ball_points(sphere.radius().millimetres_f64(), sphere.segments(), sphere.bands()),
        Primitive::Cone(cone) => cone_points(
            cone.base_radius().millimetres_f64(),
            cone.top_radius().millimetres_f64(),
            cone.height().millimetres_f64(),
            cone.segments(),
        ),
    };
    if points.len() > MAX_CONVEX_POINTS {
        return None;
    }
    Shape::convex(points).ok()
}

/// The points of a ball of `radius`, at the tessellation the primitive stores.
fn ball_points(radius: f64, segments: u32, bands: u32) -> Vec<DVec3> {
    let (segments, bands) = (segments.max(3), bands.max(2));
    let mut points = vec![DVec3::new(0.0, 0.0, radius), DVec3::new(0.0, 0.0, -radius)];
    for band in 1..bands {
        let polar = std::f64::consts::PI * f64::from(band) / f64::from(bands);
        let (height, around) = (polar.cos(), polar.sin());
        for step in 0..segments {
            let angle = std::f64::consts::TAU * f64::from(step) / f64::from(segments);
            points.push(DVec3::new(radius * around * angle.cos(), radius * around * angle.sin(), radius * height));
        }
    }
    points
}

/// The points of a cone from `base` to `top` over `height`, standing on the XY plane.
fn cone_points(base: f64, top: f64, height: f64, segments: u32) -> Vec<DVec3> {
    let segments = segments.max(3);
    let mut points = Vec::with_capacity(2 * segments as usize + 1);
    for step in 0..segments {
        let angle = std::f64::consts::TAU * f64::from(step) / f64::from(segments);
        let (sin, cos) = angle.sin_cos();
        points.push(DVec3::new(base * cos, base * sin, 0.0));
        if top > 0.0 {
            points.push(DVec3::new(top * cos, top * sin, height));
        }
    }
    if top <= 0.0 {
        points.push(DVec3::new(0.0, 0.0, height));
    }
    points
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use geom::DVec3;
    use units::Length;

    use crate::{Container, Geometry, Id, Mesh, Placement, Primitive, Project};

    fn millimetres(value: i64) -> Length {
        Length::from_millimetres(value).expect("a length")
    }

    /// A project whose root holds nothing, so a shape test meets only what it adds.
    fn empty_project() -> Project {
        let root = Container::new(Id::new(1).expect("id"), "Root", Placement::IDENTITY, Vec::new());
        Project::new(root).expect("an empty project")
    }

    fn bounds(shape: &geom::Shape) -> (DVec3, DVec3) {
        shape.bounds().expect("a shape is bounded")
    }

    #[test]
    fn a_primitive_body_is_measured_by_the_surface_it_names() {
        let primitive = Primitive::sphere(millimetres(50), 16, 8).expect("a ball");
        let mut project = empty_project();
        let root = project.root().id();
        let id = project
            .add_object(root, "Ball", Placement::IDENTITY, Geometry::Primitive(primitive))
            .expect("add the ball");
        let shape =
            super::node_shape(&project, id, DVec3::splat(-50.0), DVec3::splat(50.0)).expect("a ball is measured");
        let (low, high) = bounds(&shape);
        // The hull of the named sphere reaches its radius: the tessellation decides how round the
        // surface is, never how far the body reaches.
        assert!((low.x + 50.0).abs() < 1.0e-9 && (high.x - 50.0).abs() < 1.0e-9, "{low:?}..{high:?}");
        assert!((low.z + 50.0).abs() < 1.0e-9 && (high.z - 50.0).abs() < 1.0e-9, "the poles are exact");
        assert!(matches!(shape, geom::Shape::Convex(_)), "a ball is convex and its hull is what is measured");
    }

    #[test]
    fn a_cone_is_measured_by_its_two_circles_and_not_by_its_box() {
        let cone = Primitive::cone(millimetres(40), Length::ZERO, millimetres(100), 12).expect("a cone");
        let mut project = empty_project();
        let root = project.root().id();
        let id =
            project.add_object(root, "Cone", Placement::IDENTITY, Geometry::Primitive(cone)).expect("add the cone");
        let shape = super::node_shape(&project, id, DVec3::new(-40.0, -40.0, 0.0), DVec3::new(40.0, 40.0, 100.0))
            .expect("a cone is measured");
        let geom::Shape::Convex(points) = &shape else {
            panic!("a cone is convex");
        };
        assert_eq!(points.len(), 13, "twelve base points and the apex, and the tessellation adds nothing");
        assert_eq!(points[12], DVec3::new(0.0, 0.0, 100.0), "the apex is its own point");
    }

    /// An outline in millimetres, extruded 20 mm up: a closed body with a dent in it.
    fn extruded(outline: &[(f64, f64)]) -> Mesh {
        let mut positions: Vec<[Length; 3]> = Vec::new();
        for (x, y) in outline {
            positions.push([millimetres(*x as i64), millimetres(*y as i64), Length::ZERO]);
        }
        for (x, y) in outline {
            positions.push([millimetres(*x as i64), millimetres(*y as i64), millimetres(20)]);
        }
        let count = u32::try_from(outline.len()).expect("a short outline");
        let mut faces: Vec<Vec<u32>> = Vec::new();
        let mut bottom: Vec<u32> = (0..count).collect();
        bottom.reverse();
        faces.push(bottom);
        faces.push((count..2 * count).collect());
        for index in 0..count {
            let next = (index + 1) % count;
            faces.push(vec![index, next, next + count, index + count]);
        }
        Mesh::new(positions, faces).expect("an extrusion")
    }

    #[test]
    fn a_baked_mesh_is_a_hull_when_it_is_convex_and_a_surface_when_it_is_not() {
        let mut project = empty_project();
        let root = project.root().id();
        let cube = Mesh::cuboid([Length::ZERO; 3], [millimetres(100); 3]).expect("a cube");
        let block = project.add_object(root, "Block", Placement::IDENTITY, Geometry::Mesh(cube)).expect("a block");
        let shape = super::node_shape(&project, block, DVec3::ZERO, DVec3::splat(100.0)).expect("a mesh is measured");
        assert!(matches!(shape, geom::Shape::Convex(_)), "a box is a hull, so a body inside it is no hidden hole");
        assert_eq!(bounds(&shape), (DVec3::ZERO, DVec3::splat(100.0)));

        // An L-shaped block: a dent in the outline, so the surface is the answer and not the hull
        // that would fill the dent in.
        let dented = extruded(&[(0.0, 0.0), (100.0, 0.0), (100.0, 40.0), (40.0, 40.0), (40.0, 100.0), (0.0, 100.0)]);
        let outline = project.add_object(root, "L", Placement::IDENTITY, Geometry::Mesh(dented)).expect("an L");
        let shape = super::node_shape(&project, outline, DVec3::ZERO, DVec3::splat(100.0)).expect("a mesh is measured");
        assert!(matches!(shape, geom::Shape::Mesh { .. }), "a dented body is a surface");
    }

    #[test]
    fn a_body_the_document_cannot_describe_is_left_to_its_box() {
        // A constructive payload's faces are the geometry kernel's: this layer names no shape for it,
        // so the caller keeps the box reading it had (ADR 0179).
        let project = empty_project();
        let root = project.root().id();
        assert!(
            super::node_shape(&project, root, DVec3::ZERO, DVec3::new(600.0, 300.0, 18.0)).is_none(),
            "a container is measured by the box the rule already keeps for it"
        );
        assert!(
            super::box_between(DVec3::ZERO, DVec3::splat(f64::NAN)).is_none(),
            "and no box is built from a number that is not one"
        );
        let boxed = super::box_between(DVec3::ZERO, DVec3::splat(10.0)).expect("a box");
        assert_eq!(bounds(&boxed), (DVec3::ZERO, DVec3::splat(10.0)));
    }
}
