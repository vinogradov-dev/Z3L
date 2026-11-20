//! The analytic mesh backend for one extrusion, and the compiler from a feature history to a plan.
//!
//! A single linear extrusion is exact as polygons: two caps and one quad per profile edge. The
//! viewport and `object.collapse` therefore need no geometry kernel, and the collapsed mesh is
//! exactly the mesh the user saw. A plan with a boolean steps outside that exact case and is
//! refused here; such a plan must go through the kernel backend instead ([`super::solid::evaluate`]).
//!
//! The document keeps micrometres, so this module converts the plan's millimetre profile back to
//! whole micrometres at the last moment. The round trip is exact for a contour that was sampled
//! from document units, which is why collapse does not move a vertex.

use geom::{DQuat, DVec3, Frame};
use kernel::{BooleanOp, KernelError, SolidKernel};
use model::{Boolean, Constructive, Mesh, Placement};
use units::Length;

use crate::solid::{self, Extrude, MAX_PLAN_DEPTH, PlanBackend, PlanBuilder, SolidPlan};
use crate::{ParamError, profile};

/// Micrometres in one millimetre, the boundary factor of the geometry layer.
const MICROMETRES_PER_MILLIMETRE: f64 = 1_000.0;

/// How far a cutter's sweep reaches past each of its own ends, in millimetres.
///
/// One micrometre, the document's own resolution: a cutter whose face lies on the body's face —
/// a hole cut from the board's surface — would otherwise share a face with it, and a boolean
/// between bodies that share a face answers with nothing.
const CUTTER_OVERSHOOT_MM: f64 = 0.001;

/// Builds the polygon mesh of a constructive object with one extrusion.
///
/// The work is routed through the same [`SolidPlan`] the kernel backend consumes; only the backend
/// differs. Coordinates stay in micrometres in the object's local frame, from `z = 0` to the
/// feature height.
pub fn build_constructive_mesh(constructive: &Constructive) -> Result<Mesh, ParamError> {
    if constructive.features().len() != 1 {
        return Err(ParamError::UnsupportedConstructive);
    }
    let plan = constructive_plan(constructive)?;
    solid::evaluate_with(&plan, &mut MeshBackend).map_err(ParamError::from)
}

/// Compiles one history and builds its body through the geometry kernel.
///
/// The caller owns the returned handle and releases it through [`kernel::SolidKernel::clear`].
/// This is the one way a boolean body becomes a solid: the analytic backend refuses a plan with a
/// step, because the combination of two extrusions is not exact as polygons.
pub fn evaluate_constructive_body(
    kernel: &mut dyn SolidKernel,
    constructive: &Constructive,
) -> Result<kernel::BodyId, ParamError> {
    let plan = constructive_plan(constructive)?;
    Ok(solid::evaluate(&plan, kernel)?)
}

/// Compiles one feature history into the plan a backend evaluates.
///
/// The base extrusion sweeps the body's own path; every boolean step appends a step whose tool is
/// the plan of the shape the step froze. Each frame is that shape's placement in the frame of the
/// body being compiled, so a tool and its own tools all land in one coordinate space.
///
/// Nesting is bounded by [`MAX_PLAN_DEPTH`], and the document bounds it again when it stores a step
/// (`model::MAX_BOOLEAN_DEPTH`), so a hostile file is an error instead of an unbounded recursion.
pub fn constructive_plan(constructive: &Constructive) -> Result<SolidPlan, ParamError> {
    compile(constructive, Frame::WORLD, 0)
}

fn compile(constructive: &Constructive, frame: Frame, depth: usize) -> Result<SolidPlan, ParamError> {
    compile_cutter(constructive, frame, depth, false)
}

/// Compiles one history, optionally as a **cutter**.
///
/// A cutter's sweep is extended by one micrometre past each of its ends. The commonest cut of a
/// workshop is a hole through a board, drawn with the cutter's own face on the board's face: the two
/// bodies then share a face, and the kernel answers a boolean between bodies that share one with
/// nothing at all. One micrometre past each end is below the document's own resolution, so the cut
/// keeps the face it was asked for and the boolean has something to resolve (the same trade the
/// panel cutouts and the end bevel already make). The body being cut is never extended: it is the
/// result.
fn compile_cutter(
    constructive: &Constructive,
    frame: Frame,
    depth: usize,
    cutter: bool,
) -> Result<SolidPlan, ParamError> {
    if depth >= MAX_PLAN_DEPTH {
        return Err(ParamError::Kernel(KernelError::InvalidInput(format!(
            "a feature history nests deeper than {MAX_PLAN_DEPTH} steps"
        ))));
    }
    let Some(extrude) = constructive.extrude_feature() else {
        return Err(ParamError::UnsupportedConstructive);
    };
    let contour = constructive.contour()?;
    let profile = profile::profile_of_document_contour(&contour)?;
    let (frame, height) = if cutter {
        (
            Frame { origin: frame.origin - frame.z * CUTTER_OVERSHOOT_MM, ..frame },
            extrude.height().millimetres_f64() + 2.0 * CUTTER_OVERSHOOT_MM,
        )
    } else {
        (frame, extrude.height().millimetres_f64())
    };
    let mut builder = PlanBuilder::new().extrude(profile, frame, height)?;
    for boolean in constructive.booleans() {
        let tool = boolean.tool();
        let tool_plan = compile_cutter(tool.body(), frame_of(tool.placement()), depth.saturating_add(1), true)?;
        builder = builder.boolean(operation(boolean), tool_plan);
    }
    Ok(builder.build()?)
}

/// The plan operation one document step means.
fn operation(boolean: &Boolean) -> BooleanOp {
    match boolean.op() {
        model::BooleanOp::Union => BooleanOp::Union,
        model::BooleanOp::Difference => BooleanOp::Difference,
        model::BooleanOp::Intersection => BooleanOp::Intersection,
    }
}

/// The frame a placement stands in, in millimetres.
fn frame_of(placement: Placement) -> Frame {
    let rotation: DQuat = placement.rotation();
    Frame { origin: placement.translation(), x: rotation * DVec3::X, y: rotation * DVec3::Y, z: rotation * DVec3::Z }
}

/// Turns one extrusion into the document mesh.
pub(crate) struct MeshBackend;

impl PlanBackend for MeshBackend {
    type Body = Mesh;

    fn extrude(&mut self, extrude: &Extrude) -> Result<Mesh, KernelError> {
        mesh_from_extrusion(extrude)
    }

    fn boolean(&mut self, _op: BooleanOp, _target: Mesh, _tool: Mesh) -> Result<Mesh, KernelError> {
        Err(KernelError::Unsupported("the analytic backend evaluates one extrusion"))
    }
}

/// Two cap faces and one quad per profile edge.
fn mesh_from_extrusion(extrude: &Extrude) -> Result<Mesh, KernelError> {
    let frame = extrude.frame();
    if frame.x != DVec3::X || frame.y != DVec3::Y || frame.z != DVec3::Z {
        return Err(KernelError::Unsupported("the analytic backend needs a world-aligned frame"));
    }
    let profile = extrude.profile();
    if !profile.holes.is_empty() {
        return Err(KernelError::Unsupported("the analytic backend needs a profile without holes"));
    }

    let bottom = micrometres(frame.origin.z)?;
    let top = micrometres(frame.origin.z + extrude.height())?;
    let mut points = Vec::with_capacity(profile.outer.len());
    for point in &profile.outer {
        let world = frame.point(point.x, point.y);
        points.push([micrometres(world.x)?, micrometres(world.y)?]);
    }
    let count = u32::try_from(points.len())
        .map_err(|_| KernelError::InvalidInput("the profile has more points than an index can address".into()))?;
    if count < 3 {
        return Err(KernelError::InvalidInput("an extrusion needs at least three profile points".into()));
    }

    let mut positions = Vec::with_capacity(points.len().saturating_mul(2));
    for [x, y] in &points {
        positions.push([Length::from_micrometres(*x), Length::from_micrometres(*y), Length::from_micrometres(bottom)]);
    }
    for [x, y] in &points {
        positions.push([Length::from_micrometres(*x), Length::from_micrometres(*y), Length::from_micrometres(top)]);
    }
    let mut faces = Vec::with_capacity(points.len().saturating_add(2));
    // The bottom cap is the contour reversed so its normal points down; the top keeps the
    // counter-clockwise order. Each side quad is counter-clockwise seen from outside.
    faces.push((0..count).rev().collect());
    faces.push((count..count.saturating_mul(2)).collect());
    for index in 0..count {
        let next = (index + 1) % count;
        faces.push(vec![index, next, next.saturating_add(count), index.saturating_add(count)]);
    }
    Mesh::new(positions, faces).map_err(|error| KernelError::InvalidInput(error.to_string()))
}

/// Converts a millimetre coordinate to whole document micrometres.
fn micrometres(value_mm: f64) -> Result<i64, KernelError> {
    let value = (value_mm * MICROMETRES_PER_MILLIMETRE).round();
    if !value.is_finite() || !(i64::MIN as f64..=i64::MAX as f64).contains(&value) {
        return Err(KernelError::InvalidInput("a point lies outside the document range".into()));
    }
    Ok(value as i64)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use kernel::{Profile, SolidKernel};
    use kernel_truck::TruckKernel;
    use model::{BooleanOp as ModelOp, BooleanTool, Contour, Feature, Id, Panel, PanelNominalSize, Placement};
    use testkit::millimetres;
    use units::Length;

    use super::*;
    use crate::build_panel;

    fn rectangle(size: i64, height: i64) -> Contour {
        Contour::from_polygon(vec![
            [Length::ZERO, Length::ZERO],
            [millimetres(size), Length::ZERO],
            [millimetres(size), millimetres(height)],
            [Length::ZERO, millimetres(height)],
        ])
        .expect("rectangle")
    }

    /// Signed volume of a polygon mesh through the divergence theorem.
    fn signed_volume(mesh: &Mesh) -> f64 {
        let point = |index: u32| {
            let position = mesh.positions()[index as usize];
            DVec3::new(position[0].millimetres_f64(), position[1].millimetres_f64(), position[2].millimetres_f64())
        };
        let mut total = 0.0;
        for face in mesh.faces() {
            for corner in 1..face.len().saturating_sub(1) {
                let a = point(face[0]);
                let b = point(face[corner]);
                let c = point(face[corner + 1]);
                total += a.dot(b.cross(c)) / 6.0;
            }
        }
        total
    }

    fn placed(x: f64, y: f64, z: f64) -> Placement {
        Placement::new(DVec3::new(x, y, z), DQuat::IDENTITY).expect("a placement")
    }

    /// The volume of one body a plan builds, in cubic millimetres.
    fn volume_of(body: &Constructive) -> f64 {
        let mut kernel = TruckKernel::new();
        let handle = evaluate_constructive_body(&mut kernel, body).expect("an evaluated body");
        let volume = kernel.measure(handle).expect("a measurable body").volume;
        kernel.clear();
        volume
    }

    #[test]
    fn a_single_extrusion_has_two_caps_and_one_quad_per_edge() {
        let constructive = Constructive::extrude(rectangle(100, 50).into(), millimetres(18)).expect("constructive");
        let mesh = build_constructive_mesh(&constructive).expect("mesh");
        assert_eq!(mesh.positions().len(), 8);
        assert_eq!(mesh.faces().len(), 6);
        assert!(mesh.faces().iter().all(|face| face.len() == 4));
        assert!((signed_volume(&mesh) - 100.0 * 50.0 * 18.0).abs() < 1.0e-6);
    }

    #[test]
    fn the_analytic_object_matches_the_kernel_panel_volume() {
        let constructive = Constructive::extrude(rectangle(700, 500).into(), millimetres(18)).expect("constructive");
        let mesh = build_constructive_mesh(&constructive).expect("mesh");
        let analytic = signed_volume(&mesh);

        let mut kernel = TruckKernel::new();
        let size = PanelNominalSize::new(millimetres(700), millimetres(500)).expect("panel size");
        let panel = Panel::new(Id::new(1).expect("id"), "Panel", size, None, millimetres(18), Placement::IDENTITY)
            .expect("panel");
        let geometry = panel.geometry_with_fallbacks().expect("geometry");
        let body = build_panel(&mut kernel, &panel, &geometry).expect("kernel body");
        let kernel_volume = kernel.measure(body).expect("measure").volume;
        assert!((analytic - kernel_volume).abs() < 1.0, "analytic {analytic} vs kernel {kernel_volume}");
    }

    #[test]
    fn a_boolean_plan_is_refused_by_the_analytic_backend() {
        let tool = PlanBuilder::new()
            .extrude(Profile::rectangle(10.0, 10.0).expect("tool"), Frame::WORLD, 18.0)
            .expect("tool")
            .build()
            .expect("tool plan");
        let plan = PlanBuilder::new()
            .extrude(Profile::rectangle(100.0, 50.0).expect("base"), Frame::WORLD, 18.0)
            .expect("base")
            .boolean(BooleanOp::Difference, tool)
            .build()
            .expect("boolean plan");
        let error = solid::evaluate_with(&plan, &mut MeshBackend).expect_err("the analytic backend refuses booleans");
        assert!(matches!(error, KernelError::Unsupported(_)), "{error}");
    }

    #[test]
    fn a_history_with_more_than_one_feature_is_not_evaluated_silently() {
        let extrude = model::Extrude::new(millimetres(5)).expect("extrude");
        let constructive = Constructive::new(
            rectangle(10, 10).into(),
            vec![Feature::Extrude(extrude.clone()), Feature::Extrude(extrude)],
        )
        .expect("two features are valid data");
        assert!(matches!(build_constructive_mesh(&constructive), Err(ParamError::UnsupportedConstructive)));
    }

    #[test]
    fn a_large_legal_ring_becomes_a_drawable_mesh() {
        use model::ContourSegment;
        use units::Degrees;

        let start = Degrees::from_millidegrees(0);
        let sweep = Degrees::from_millidegrees(350_000);
        let outer =
            ContourSegment::arc([Length::ZERO, Length::ZERO], millimetres(5_000), start, sweep).expect("outer arc");
        let inner = ContourSegment::arc(
            [Length::ZERO, Length::ZERO],
            millimetres(4_000),
            sweep,
            Degrees::from_millidegrees(-350_000),
        )
        .expect("inner arc");
        let contour = Contour::new(vec![
            outer,
            ContourSegment::line(outer.end().expect("outer end"), inner.start().expect("inner start"))
                .expect("inward line"),
            inner,
            ContourSegment::line(inner.end().expect("inner end"), outer.start().expect("outer start"))
                .expect("outward line"),
        ])
        .expect("a simple band");
        let constructive = Constructive::extrude(contour.into(), millimetres(18)).expect("constructive");
        // X2: the band is legal and its cap is one face; the shape-relative chord keeps the face
        // inside `MAX_FACE_VERTICES`, so the object draws instead of being refused.
        let mesh = build_constructive_mesh(&constructive).expect("the band becomes a mesh");
        let cap = mesh.faces().first().expect("a bottom cap");
        assert!(cap.len() > 3, "the cap follows the contour");
        assert!(cap.len() <= model::MAX_FACE_VERTICES, "one cap holds {} points", cap.len());
        assert_eq!(mesh.faces().len(), cap.len().saturating_add(2), "two caps and one quad per profile edge");
        // The band is 350 degrees of an annulus 4 m to 5 m wide, 18 mm thick.
        let expected = 350.0_f64.to_radians() * (5_000.0_f64.powi(2) - 4_000.0_f64.powi(2)) / 2.0 * 18.0;
        assert!(
            (signed_volume(&mesh) - expected).abs() < expected * 1.0e-3,
            "volume was {}, expected about {expected}",
            signed_volume(&mesh)
        );
    }

    #[test]
    fn a_rotated_frame_is_refused_by_the_analytic_backend() {
        let profile = Profile::rectangle(10.0, 10.0).expect("profile");
        let frame = Frame { x: DVec3::Y, y: -DVec3::X, ..Frame::WORLD };
        let extrude = Extrude::new(profile, frame, 5.0).expect("extrusion");
        let error = mesh_from_extrusion(&extrude).expect_err("a rotated frame has no analytic mesh");
        assert!(matches!(error, KernelError::Unsupported(_)), "{error}");
    }

    /// The shape a step cuts with: a 50 x 50 ring swept 30 mm, standing where the caller puts it.
    ///
    /// The tool passes through the base and sticks out of both faces, with every side strictly
    /// inside the base's outline, so the booleans have no coincident faces to resolve.
    fn through_tool() -> BooleanTool {
        let body = Constructive::extrude(rectangle(50, 50).into(), millimetres(30)).expect("tool");
        BooleanTool::new("Cutter", body, placed(25.0, 25.0, -5.0)).expect("a tool")
    }

    #[test]
    fn a_boolean_step_appends_the_tools_plan_where_the_step_puts_it() {
        let mut base = Constructive::extrude(rectangle(100, 50).into(), millimetres(18)).expect("base");
        base.push_boolean(Boolean::new(ModelOp::Difference, through_tool())).expect("a step");

        let plan = constructive_plan(&base).expect("a plan");

        assert_eq!(plan.steps().len(), 1);
        let step = plan.steps().first().expect("one step");
        assert_eq!(step.op(), BooleanOp::Difference);
        let frame = step.tool().base().frame();
        assert!((frame.origin.z + 5.0 + CUTTER_OVERSHOOT_MM).abs() < 1.0e-9, "the cutter reaches past its own end");
        assert!((frame.origin.x - 25.0).abs() < 1.0e-9, "and stands where the step froze it");
        let height = step.tool().base().height();
        assert!((height - (30.0 + 2.0 * CUTTER_OVERSHOOT_MM)).abs() < 1.0e-9, "the sweep covers both ends");
    }

    #[test]
    fn a_cutter_whose_face_lies_on_the_bodys_face_still_cuts() {
        // The owner's part, and the commonest cut of a workshop: a hole drawn from the board's own
        // surface, so the cutter's bottom face and the body's bottom face are one plane. Two bodies
        // that share a face make the kernel answer a boolean with nothing at all, and the cutter's
        // own micrometre past each end is what keeps the cut resolvable.
        let mut base = Constructive::extrude(rectangle(100, 100).into(), millimetres(18)).expect("base");
        let cutter = Constructive::extrude(rectangle(50, 50).into(), millimetres(10)).expect("cutter");
        let tool = BooleanTool::new("Flush", cutter, placed(25.0, 25.0, 0.0)).expect("a tool");
        base.push_boolean(Boolean::new(ModelOp::Difference, tool)).expect("a step");

        let volume = volume_of(&base);

        let expected = 180_000.0 - 50.0 * 50.0 * 10.0;
        assert!((volume - expected).abs() < 10.0, "volume was {volume}, expected about {expected}");
    }

    #[test]
    fn a_union_of_two_boxes_holds_the_volume_both_cover_once() {
        let mut base = Constructive::extrude(rectangle(100, 100).into(), millimetres(18)).expect("base");
        base.push_boolean(Boolean::new(ModelOp::Union, through_tool())).expect("a step");

        let volume = volume_of(&base);

        // The base and the part of the tool above and below it, with the shared 50 x 50 x 18 counted
        // once: 100 x 100 x 18 + 50 x 50 x 30 - 50 x 50 x 18. The cutter's micrometre at each end is
        // the trade the compile makes so a face lying on another face is not a shared face.
        let expected = 180_000.0 + 75_000.0 - 45_000.0;
        assert!((volume - expected).abs() < 10.0, "volume was {volume}, expected about {expected}");
    }

    #[test]
    fn a_difference_cuts_the_tools_overlap_out() {
        let mut base = Constructive::extrude(rectangle(100, 100).into(), millimetres(18)).expect("base");
        base.push_boolean(Boolean::new(ModelOp::Difference, through_tool())).expect("a step");

        let volume = volume_of(&base);

        assert!((volume - 135_000.0).abs() < 10.0, "volume was {volume}, expected about 135000");
    }

    #[test]
    fn an_intersection_keeps_only_what_both_bodies_share() {
        let mut base = Constructive::extrude(rectangle(100, 100).into(), millimetres(18)).expect("base");
        base.push_boolean(Boolean::new(ModelOp::Intersection, through_tool())).expect("a step");

        let volume = volume_of(&base);

        assert!((volume - 45_000.0).abs() < 10.0, "volume was {volume}, expected about 45000");
    }

    #[test]
    fn a_tool_with_steps_of_its_own_is_compiled_where_it_stands() {
        // The body cuts with a cutter, and the cutter is itself joined with a chimney: the plan
        // nests, and every frame is expressed in the frame of the body the step froze.
        let mut base = Constructive::extrude(rectangle(100, 100).into(), millimetres(18)).expect("base");
        let mut middle = Constructive::extrude(rectangle(50, 50).into(), millimetres(30)).expect("middle");
        // The chimney stands inside the middle's outline and sticks out of both of its faces, so the
        // step has no coincident faces and its own frame is the only thing under test.
        let chimney = Constructive::extrude(rectangle(20, 20).into(), millimetres(40)).expect("chimney");
        middle
            .push_boolean(Boolean::new(
                ModelOp::Union,
                BooleanTool::new("Chimney", chimney, placed(40.0, 40.0, -10.0)).expect("a tool"),
            ))
            .expect("its own step");
        let cutter =
            BooleanTool::new("Cutter", middle, placed(25.0, 25.0, -5.0)).expect("the cutter stands where it cuts");
        base.push_boolean(Boolean::new(ModelOp::Difference, cutter)).expect("a step");

        let volume = volume_of(&base);

        // The middle body removes 50 x 50 x 18 from the base; its chimney stands inside the base's
        // outline, so the union adds nothing the base does not already cover.
        let expected = 180_000.0 - 50.0 * 50.0 * 18.0;
        assert!((volume - expected).abs() < 10.0, "volume was {volume}, expected about {expected}");
    }
}
