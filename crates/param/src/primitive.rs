//! Builds the polygon mesh of a parametric primitive.
//!
//! The parameters are document data in integer micrometres; this module is the boundary where the
//! ring trigonometry runs in floating point and the result is rounded back to lengths. The sphere is
//! latitude bands joined by triangles at the poles, spaced by equal steps of latitude rather than of
//! `z`: uniform `z` crowds every band into the poles and leaves a cap as wide as half the radius
//! (X55). The cone is two circles and the band between them, or one circle and an apex when its top
//! radius is zero. A box or a cylinder is a drawn profile swept by `crates/param`, not a parameter
//! set (ADR 0138).

use geom::{DVec3, Frame};
use kernel::MeshBody;
use model::{Cone, Mesh, Primitive, PrimitiveShape, Sphere};
use units::Length;

use crate::solid::{PlanBuilder, SolidPlan};
use crate::{CUTTER_OVERSHOOT_MM, ParamError};

/// The sine and cosine of one segment around a ring, in the XY plane.
fn ring_angle(segment: usize, segments: usize) -> (f64, f64) {
    let angle = std::f64::consts::TAU * segment as f64 / segments as f64;
    angle.sin_cos()
}

/// Rounds a float micrometre coordinate into a document length.
fn coordinate(value: f64) -> Result<Length, ParamError> {
    if !value.is_finite() || value < i64::MIN as f64 || value > i64::MAX as f64 {
        return Err(ParamError::PrimitiveMesh("a primitive coordinate is not a representable length".into()));
    }
    Ok(Length::from_micrometres(value.round() as i64))
}

/// Converts a vertex count to an index, refusing one that would overflow the index buffer.
fn index(value: usize) -> Result<u32, ParamError> {
    u32::try_from(value).map_err(|_| ParamError::PrimitiveMesh("a primitive has too many vertices to index".into()))
}

/// Builds the document mesh of one primitive shape.
///
/// The steps cut from the body are not part of this mesh: a body with a step has no analytic mesh
/// and is built by the geometry kernel ([`primitive_plan`]). A plain primitive is drawn from its
/// parameters, which is what keeps a ball from costing a kernel step in the viewport.
pub fn build_primitive_mesh(primitive: &Primitive) -> Result<Mesh, ParamError> {
    primitive_mesh(primitive.base(), SphereRings::Latitudes)
}

/// The mesh a shape is built from, with the rings the caller's use needs.
fn primitive_mesh(shape: PrimitiveShape, layout: SphereRings) -> Result<Mesh, ParamError> {
    match shape {
        PrimitiveShape::Sphere(sphere) => sphere_mesh(&sphere, layout),
        PrimitiveShape::Cone(cone) => cone_mesh(&cone),
    }
}

/// A ring of `segments` vertices at `radius`, at height `z`.
///
/// Counter-clockwise seen from `+Z`, which is the order both caps and every band are wound from.
fn push_ring(positions: &mut Vec<[Length; 3]>, radius: f64, z: Length, segments: usize) -> Result<(), ParamError> {
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
fn cone_mesh(cone: &Cone) -> Result<Mesh, ParamError> {
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
    let top_ring = |segment: usize| -> Result<u32, ParamError> { index(segments + segment % segments) };
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

/// Where a sphere's latitude rings stand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SphereRings {
    /// One ring at every band boundary, the equator among them: the mesh the viewport draws and
    /// `object.collapse` bakes, with a ring at the equator and equal meridian steps (X55).
    Latitudes,
    /// One ring per band, half a step away from each pole, so **no ring of vertices lies in the
    /// plane through the centre**.
    ///
    /// A ball is drawn around its centre (ADR 0139), which puts that plane on the face it was drawn
    /// on, and a ring of vertices in a face is the one case the geometry kernel cannot divide: it
    /// answers a boolean with a broken shell. Measured on `truck`: a board cut by a ball centred
    /// exactly on its face fails with a ring at the equator and cuts to the analytic half-volume
    /// without one. Only the kernel body is built this way, so the drawn surface keeps its rings.
    ///
    /// [ADR 0139]: ../docs/decisions/0139-a-sphere-is-drawn-around-its-centre.md
    OffEquator,
}

/// A closed banded surface with one vertex at each pole.
fn sphere_mesh(sphere: &Sphere, layout: SphereRings) -> Result<Mesh, ParamError> {
    let segments = sphere.segments() as usize;
    let bands = sphere.bands() as usize;
    let radius = sphere.radius().micrometres() as f64;
    let latitudes: Vec<f64> = match layout {
        SphereRings::Latitudes => (1..bands).map(|band| std::f64::consts::PI * band as f64 / bands as f64).collect(),
        SphereRings::OffEquator => {
            (0..bands).map(|band| std::f64::consts::PI * (band as f64 + 0.5) / bands as f64).collect()
        }
    };
    let mut positions = Vec::with_capacity(segments * latitudes.len() + 2);
    positions.push([Length::ZERO, Length::ZERO, coordinate(-radius)?]);
    for latitude in &latitudes {
        let (sin_lat, cos_lat) = latitude.sin_cos();
        let z = -radius * cos_lat;
        let ring = radius * sin_lat;
        for segment in 0..segments {
            let (sin, cos) = ring_angle(segment, segments);
            positions.push([coordinate(cos * ring)?, coordinate(sin * ring)?, coordinate(z)?]);
        }
    }
    let top = index(positions.len())?;
    positions.push([Length::ZERO, Length::ZERO, coordinate(radius)?]);
    let ring_index =
        |ring: usize, segment: usize| -> Result<u32, ParamError> { index(1 + ring * segments + segment % segments) };
    let rings = latitudes.len();
    let mut faces = Vec::new();
    for segment in 0..segments {
        let next = (segment + 1) % segments;
        faces.push(vec![0, ring_index(0, next)?, ring_index(0, segment)?]);
    }
    for ring in 0..rings.saturating_sub(1) {
        for segment in 0..segments {
            let next = (segment + 1) % segments;
            faces.push(vec![
                ring_index(ring, segment)?,
                ring_index(ring, next)?,
                ring_index(ring + 1, next)?,
                ring_index(ring + 1, segment)?,
            ]);
        }
    }
    for segment in 0..segments {
        let next = (segment + 1) % segments;
        faces.push(vec![
            top,
            ring_index(rings.saturating_sub(1), segment)?,
            ring_index(rings.saturating_sub(1), next)?,
        ]);
    }
    Ok(Mesh::new(positions, faces)?)
}

/// Compiles one primitive body — its shape and the steps cut from it — into a plan.
///
/// A primitive is not an extrusion, so the plan starts from the shape's own closed triangle mesh:
/// the surface the viewport draws, handed to the kernel as the body it already is. Every step is
/// then compiled where it stands, exactly as on a drawn body.
///
/// A **cutter** is grown by one micrometre along its own surface. The commonest cut of a workshop is
/// drawn on the face it cuts — a ball is drawn around its centre, so half of it is already inside the
/// board — and a body whose surface only *touches* another's is the case the kernel answers with
/// nothing at all, or with a broken shell. One micrometre is below the document's resolution, the
/// same trade the drawn cutter's own overshoot makes (ADR 0197).
pub fn primitive_plan(
    primitive: &Primitive,
    frame: Frame,
    depth: usize,
    cutter: bool,
) -> Result<SolidPlan, ParamError> {
    if depth >= crate::solid::MAX_PLAN_DEPTH {
        return Err(ParamError::Kernel(kernel::KernelError::InvalidInput(format!(
            "a feature history nests deeper than {} steps",
            crate::solid::MAX_PLAN_DEPTH
        ))));
    }
    let mesh = primitive_mesh(primitive.base(), SphereRings::OffEquator)?;
    let grow = if cutter { CUTTER_OVERSHOOT_MM } else { 0.0 };
    let body = mesh_body(&mesh, frame, grow)?;
    let mut builder = PlanBuilder::new().mesh(body)?;
    for boolean in primitive.steps() {
        let tool_plan = super::constructive::compile_tool(boolean.tool(), frame, depth.saturating_add(1))?;
        builder = builder.boolean(super::constructive::operation(boolean), tool_plan);
    }
    Ok(builder.build()?)
}

/// Compiles one primitive body and builds it through the geometry kernel.
///
/// The caller owns the returned handle and releases it through [`kernel::SolidKernel::clear`]. A
/// plain primitive needs no kernel: its mesh comes from its parameters, and only a body with a step
/// has a shape no analytic mesh describes.
pub fn evaluate_primitive_body(
    kernel: &mut dyn kernel::SolidKernel,
    primitive: &Primitive,
) -> Result<kernel::BodyId, ParamError> {
    let plan = primitive_plan(primitive, Frame::WORLD, 0, false)?;
    Ok(crate::solid::evaluate(&plan, kernel)?)
}

/// The closed triangle body of a document mesh, standing in `frame`, in millimetres.
///
/// The document mesh is a set of convex polygon faces in micrometres; the kernel takes triangles in
/// millimetres. The faces of a primitive are convex by construction, so a fan from the first vertex
/// keeps the surface closed and every edge shared by two triangles with opposite winding.
fn mesh_body(mesh: &Mesh, frame: Frame, grow: f64) -> Result<MeshBody, ParamError> {
    let mut positions: Vec<DVec3> = Vec::with_capacity(mesh.positions().len());
    for point in mesh.positions() {
        let local = DVec3::new(point[0].millimetres_f64(), point[1].millimetres_f64(), point[2].millimetres_f64());
        positions.push(frame.origin + frame.x * local.x + frame.y * local.y + frame.z * local.z);
    }
    let mut triangles: Vec<[u32; 3]> = Vec::with_capacity(mesh.faces().len().saturating_mul(2));
    for face in mesh.faces() {
        let Some(first) = face.first().copied() else {
            continue;
        };
        for corner in 1..face.len().saturating_sub(1) {
            let (second, third) = (face[corner], face[corner + 1]);
            triangles.push([first, second, third]);
        }
    }
    if grow > 0.0 {
        grow_body(&mut positions, &triangles, grow);
    }
    MeshBody::new(positions, triangles).map_err(ParamError::Kernel)
}

/// Moves every vertex `by` millimetres along the surface's own outward normal.
///
/// A vertex's normal is the sum of the normals of the triangles that meet at it, so a sphere grows
/// as a sphere and a flat face does not move sideways along its own edges. A vertex whose triangles
/// cancel has no direction to grow in and stays where it is: a degenerate sliver must not teleport.
fn grow_body(positions: &mut [DVec3], triangles: &[[u32; 3]], by: f64) {
    let mut normals = vec![DVec3::ZERO; positions.len()];
    for [a, b, c] in triangles {
        let (Some(first), Some(second), Some(third)) = (
            positions.get(*a as usize).copied(),
            positions.get(*b as usize).copied(),
            positions.get(*c as usize).copied(),
        ) else {
            continue;
        };
        let normal = (second - first).cross(third - first);
        for index in [*a as usize, *b as usize, *c as usize] {
            if let Some(slot) = normals.get_mut(index) {
                *slot += normal;
            }
        }
    }
    for (position, normal) in positions.iter_mut().zip(normals) {
        if normal.length_squared() > 1.0e-18 {
            *position += normal.normalize() * by;
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;
    use model::{DEFAULT_BANDS, DEFAULT_SEGMENTS};
    use testkit::millimetres;

    use kernel::{BooleanOp, SolidKernel};
    use kernel_truck::TruckKernel;
    use model::{Boolean, BooleanOp as ModelOp, BooleanTool, Constructive, Path, Placement, ToolBody};

    /// The volume of one primitive body, in cubic millimetres, at the tessellation it stores.
    fn volume_of_primitive(primitive: &Primitive) -> f64 {
        let mut kernel = TruckKernel::new();
        let plan = primitive_plan(primitive, Frame::WORLD, 0, false).expect("a plan");
        let body = crate::solid::evaluate(&plan, &mut kernel).expect("a body");
        let volume = kernel.measure(body).expect("a measured body").volume;
        kernel.clear();
        volume
    }

    /// The volume of one solid after `op` with `tool`, both built by the kernel.
    fn volume_after(subject: &Primitive, op: BooleanOp, tool: ToolBody, at: Placement) -> f64 {
        let mut kernel = TruckKernel::new();
        let target =
            crate::solid::evaluate(&primitive_plan(subject, Frame::WORLD, 0, false).expect("a plan"), &mut kernel)
                .expect("the subject");
        let tool_plan = match &tool {
            ToolBody::Constructive(body) => crate::constructive_plan(body).expect("a tool plan"),
            ToolBody::Primitive(body) => primitive_plan(body, frame_of(at), 0, true).expect("a tool plan"),
        };
        let tool_body = crate::solid::evaluate(&tool_plan, &mut kernel).expect("a tool");
        let result = kernel.boolean(op, target, tool_body).expect("the operation");
        let volume = kernel.measure(result).expect("a measured body").volume;
        kernel.clear();
        volume
    }

    /// The frame a placement stands in, in millimetres.
    fn frame_of(placement: Placement) -> Frame {
        let rotation = placement.rotation();
        Frame {
            origin: placement.translation(),
            x: rotation * DVec3::X,
            y: rotation * DVec3::Y,
            z: rotation * DVec3::Z,
        }
    }

    /// A 100 x 100 x 18 board, the body a workshop cuts.
    fn board() -> Constructive {
        let ring = Path::rectangle([Length::ZERO, Length::ZERO], [millimetres(100), millimetres(100)]).expect("a ring");
        Constructive::extrude(ring, millimetres(18)).expect("a board")
    }

    #[test]
    fn a_plain_ball_is_the_solid_its_parameters_describe() {
        let ball = Primitive::sphere(millimetres(20), 64, 32).expect("ball");
        let volume = volume_of_primitive(&ball);
        let ideal = 4.0 / 3.0 * std::f64::consts::PI * 8_000.0;
        // The mesh is inscribed in the sphere, so it holds a little less than the ideal solid.
        assert!((volume - ideal).abs() < ideal * 0.01, "volume was {volume}, expected about {ideal}");
    }

    #[test]
    fn a_ball_cuts_a_board_where_it_stands() {
        // The owner's first case: the ball is drawn on the board's face, so its centre is on that
        // face and half of it is already inside the material.
        let ball = Primitive::sphere(millimetres(10), 32, 16).expect("ball");
        let cutter = BooleanTool::new(
            "Ball",
            ToolBody::Primitive(ball),
            Placement::new(DVec3::new(50.0, 50.0, 0.0), geom::DQuat::IDENTITY).expect("a placement"),
        )
        .expect("a tool");
        // The board is the target, so it is compiled from its own history and cut with the ball.
        let mut kernel = TruckKernel::new();
        let target = crate::solid::evaluate(&crate::constructive_plan(&board()).expect("a board plan"), &mut kernel)
            .expect("a board");
        let tool = crate::solid::evaluate(
            &crate::constructive::compile_tool(&cutter, Frame::WORLD, 1).expect("a cutter plan"),
            &mut kernel,
        )
        .expect("a cutter");
        let cut = kernel.boolean(BooleanOp::Difference, target, tool).expect("the cut");
        let volume = kernel.measure(cut).expect("a measured body").volume;
        kernel.clear();
        let half = 4.0 / 3.0 * std::f64::consts::PI * 1_000.0 / 2.0;
        let expected = 180_000.0 - half;
        assert!((volume - expected).abs() < expected * 0.01, "volume was {volume}, expected about {expected}");
    }

    #[test]
    fn a_block_cuts_a_ball_leaving_a_rectangular_notch() {
        // The owner's second case, in the other direction: the ball is the target and a drawn body
        // is the cutter, which is what a primitive's own step list is for.
        let mut ball = Primitive::sphere(millimetres(20), 32, 16).expect("ball");
        let half = 8;
        let ring = Path::rectangle([millimetres(-half), millimetres(-half)], [millimetres(half), millimetres(half)])
            .expect("a ring");
        let block = Constructive::extrude(ring, millimetres(60)).expect("a block");
        let step = BooleanTool::new(
            "Block",
            ToolBody::Constructive(block),
            Placement::new(DVec3::new(0.0, 0.0, -30.0), geom::DQuat::IDENTITY).expect("a placement"),
        )
        .expect("a tool");
        ball.push_boolean(Boolean::new(ModelOp::Difference, step)).expect("a step");

        let volume = volume_of_primitive(&ball);

        // The 16 x 16 prism takes a square column out of the ball along its full height. Integrating
        // the ball's chord over that square gives the solid the ideal ball would lose; the mesh is
        // inscribed, so the result stands a little above the ideal difference.
        let steps = 400;
        let mut prism = 0.0;
        for i in 0..steps {
            for j in 0..steps {
                let x = -half as f64 + 2.0 * half as f64 * (i as f64 + 0.5) / steps as f64;
                let y = -half as f64 + 2.0 * half as f64 * (j as f64 + 0.5) / steps as f64;
                let chord = 400.0 - x * x - y * y;
                if chord > 0.0 {
                    prism += 2.0 * chord.sqrt();
                }
            }
        }
        let prism = prism * (2.0 * half as f64 / steps as f64).powi(2);
        let plain = volume_of_primitive(&Primitive::sphere(millimetres(20), 32, 16).expect("ball"));
        let expected = plain - prism;
        assert!((volume - expected).abs() < expected * 0.01, "volume was {volume}, expected about {expected}");
    }

    #[test]
    fn a_ball_carved_out_of_a_ball_leaves_a_spherical_scoop() {
        let target = Primitive::sphere(millimetres(20), 32, 16).expect("a ball");
        let scoop = Primitive::sphere(millimetres(20), 32, 16).expect("a ball");
        let volume = volume_after(
            &target,
            BooleanOp::Difference,
            ToolBody::Primitive(scoop),
            Placement::new(DVec3::new(12.0, 0.0, 0.0), geom::DQuat::IDENTITY).expect("a placement"),
        );
        let sphere = 4.0 / 3.0 * std::f64::consts::PI * 8_000.0;
        // Two balls of radius 20 whose centres stand 12 apart: the lens they share is the difference.
        let d = 12.0_f64;
        let r = 20.0_f64;
        let lens = std::f64::consts::PI / 12.0 * (4.0 * r + d) * (2.0 * r - d).powi(2);
        let expected = sphere - lens;
        assert!((volume - expected).abs() < expected * 0.02, "volume was {volume}, expected about {expected}");
    }

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
