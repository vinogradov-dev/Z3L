//! A domain-free plan for building one solid body.
//!
//! A [`SolidPlan`] is the derived description of a solid: a base extrusion followed by boolean
//! steps, each of which owns a nested tool plan. It knows profiles, frames and distances and
//! nothing about furniture. A caller compiles document features into a plan and evaluates it with a
//! backend: [`evaluate`] drives the geometry kernel, while an analytic backend can answer the one
//! case where a polygon mesh is exact.
//!
//! A plan is never serialized. The document keeps the feature parameters and rebuilds the plan when
//! it changes, so the plan can be reshaped without touching the file format.
//!
//! Coordinates are millimetres, the unit of [`kernel`]. Conversion from the document's
//! integer micrometres happens at the caller's boundary, never in this module.

use geom::Frame;
use kernel::{BodyId, BooleanOp, KernelError, MeshBody, Profile, SolidKernel};

/// Largest nesting depth of a solid plan.
///
/// A plan is built by our own code, so this only guards a broken or hostile graph from a future
/// importer: evaluation reports an error instead of recursing without a bound.
pub const MAX_PLAN_DEPTH: usize = 32;

/// One extrusion: a closed profile swept along its frame's `+Z` axis.
#[derive(Clone, Debug, PartialEq)]
pub struct Extrude {
    profile: Profile,
    frame: Frame,
    height: f64,
}

impl Extrude {
    /// Validates the frame and the height and stores the extrusion.
    pub fn new(profile: Profile, frame: Frame, height: f64) -> Result<Self, KernelError> {
        if !height.is_finite() || height <= 0.0 {
            return Err(KernelError::InvalidInput("an extrusion height must be positive and finite".into()));
        }
        if !frame.is_orthonormal() {
            return Err(KernelError::InvalidInput("an extrusion frame is not orthonormal".into()));
        }
        Ok(Self { profile, frame, height })
    }

    /// The closed planar profile.
    pub const fn profile(&self) -> &Profile {
        &self.profile
    }

    /// The frame the profile lies in.
    pub const fn frame(&self) -> Frame {
        self.frame
    }

    /// The extrusion distance along the frame's `+Z` axis, in millimetres.
    pub const fn height(&self) -> f64 {
        self.height
    }
}

/// One boolean step applied to the body built so far.
#[derive(Clone, Debug, PartialEq)]
pub struct PlanStep {
    op: BooleanOp,
    tool: SolidPlan,
}

impl PlanStep {
    /// Combines a body with the body produced by `tool`.
    pub const fn new(op: BooleanOp, tool: SolidPlan) -> Self {
        Self { op, tool }
    }

    /// The boolean operation.
    pub const fn op(&self) -> BooleanOp {
        self.op
    }

    /// The plan that builds the second operand.
    pub const fn tool(&self) -> &SolidPlan {
        &self.tool
    }
}

/// What a plan builds its body from.
///
/// Two shapes start a solid: a closed profile swept along its frame's `+Z`, and a closed triangle
/// mesh that is already a solid. The second is how a shape that no extrusion describes — a sphere,
/// a cone, an imported part — becomes something a boolean can cut and be cut by.
#[derive(Clone, Debug, PartialEq)]
pub enum SolidBase {
    /// One extrusion of a closed profile.
    Extrude(Extrude),
    /// A closed triangle mesh, already the surface of a solid.
    Mesh(MeshBody),
}

impl SolidBase {
    /// The extrusion, when the base is one.
    pub const fn extrude(&self) -> Option<&Extrude> {
        match self {
            Self::Extrude(extrude) => Some(extrude),
            Self::Mesh(_) => None,
        }
    }

    /// The mesh, when the base is one.
    pub const fn mesh(&self) -> Option<&MeshBody> {
        match self {
            Self::Extrude(_) => None,
            Self::Mesh(body) => Some(body),
        }
    }
}

/// A plan to build one solid body.
///
/// The base builds the first body; every step then combines the body built so far with the body of
/// its tool plan. This is the shape the panel compiler needs: a core, then one subtraction per
/// cutout. A tool is itself a plan, so the tree can nest, but a flat sequence stays the common case.
#[derive(Clone, Debug, PartialEq)]
pub struct SolidPlan {
    base: SolidBase,
    steps: Vec<PlanStep>,
}

impl SolidPlan {
    /// What the body is built from.
    pub const fn base(&self) -> &SolidBase {
        &self.base
    }

    /// The boolean steps in application order.
    pub fn steps(&self) -> &[PlanStep] {
        &self.steps
    }
}

/// Builds a [`SolidPlan`] one operation at a time.
///
/// The builder is internal to evaluation: it exists to assemble a value and then disappears. The
/// plan it returns is the derived description a backend consumes.
#[derive(Clone, Debug, Default)]
pub struct PlanBuilder {
    base: Option<SolidBase>,
    steps: Vec<PlanStep>,
}

impl PlanBuilder {
    /// An empty builder.
    pub const fn new() -> Self {
        Self { base: None, steps: Vec::new() }
    }

    /// Sets the base extrusion.
    ///
    /// A plan has exactly one base; calling this twice is an error rather than a silent replacement.
    pub fn extrude(self, profile: Profile, frame: Frame, height: f64) -> Result<Self, KernelError> {
        self.base(SolidBase::Extrude(Extrude::new(profile, frame, height)?))
    }

    /// Sets the base mesh body.
    pub fn mesh(self, body: MeshBody) -> Result<Self, KernelError> {
        self.base(SolidBase::Mesh(body))
    }

    /// Sets the one base a plan has.
    fn base(mut self, base: SolidBase) -> Result<Self, KernelError> {
        if self.base.is_some() {
            return Err(KernelError::InvalidInput("a solid plan has one base".into()));
        }
        self.base = Some(base);
        Ok(self)
    }

    /// Appends a boolean step against the body built so far.
    pub fn boolean(mut self, op: BooleanOp, tool: SolidPlan) -> Self {
        self.steps.push(PlanStep::new(op, tool));
        self
    }

    /// Finishes the plan, refusing a builder with no base.
    pub fn build(self) -> Result<SolidPlan, KernelError> {
        let Some(base) = self.base else {
            return Err(KernelError::InvalidInput("a solid plan needs a base".into()));
        };
        Ok(SolidPlan { base, steps: self.steps })
    }
}

/// A sink for the operations of a solid plan.
///
/// The evaluator owns the traversal; a backend only turns one extrusion or one boolean into its
/// own body type. This is what lets the same plan drive the geometry kernel and the analytic mesh
/// path without duplicating the fold.
pub trait PlanBackend {
    /// The handle of a body built by this backend.
    type Body;

    /// Builds one extrusion.
    fn extrude(&mut self, extrude: &Extrude) -> Result<Self::Body, KernelError>;

    /// Builds one mesh body.
    fn mesh(&mut self, body: &MeshBody) -> Result<Self::Body, KernelError>;

    /// Combines two built bodies.
    fn boolean(&mut self, op: BooleanOp, target: Self::Body, tool: Self::Body) -> Result<Self::Body, KernelError>;
}

/// Evaluates a plan with any backend.
pub fn evaluate_with<B: PlanBackend>(plan: &SolidPlan, backend: &mut B) -> Result<B::Body, KernelError> {
    evaluate_at(plan, backend, 0)
}

fn evaluate_at<B: PlanBackend>(plan: &SolidPlan, backend: &mut B, depth: usize) -> Result<B::Body, KernelError> {
    if depth >= MAX_PLAN_DEPTH {
        return Err(KernelError::InvalidInput(format!("a solid plan nests deeper than {MAX_PLAN_DEPTH} steps")));
    }
    let mut body = match plan.base() {
        SolidBase::Extrude(extrude) => backend.extrude(extrude)?,
        SolidBase::Mesh(body) => backend.mesh(body)?,
    };
    for step in plan.steps() {
        let tool = evaluate_at(step.tool(), backend, depth + 1)?;
        body = backend.boolean(step.op(), body, tool)?;
    }
    Ok(body)
}

/// Adapts a [`SolidKernel`] to the plan evaluator.
pub struct KernelBackend<'a> {
    kernel: &'a mut dyn SolidKernel,
}

impl<'a> KernelBackend<'a> {
    /// Wraps a kernel.
    pub const fn new(kernel: &'a mut dyn SolidKernel) -> Self {
        Self { kernel }
    }
}

impl PlanBackend for KernelBackend<'_> {
    type Body = BodyId;

    fn extrude(&mut self, extrude: &Extrude) -> Result<BodyId, KernelError> {
        self.kernel.extrude(extrude.profile(), &extrude.frame(), extrude.height())
    }

    fn mesh(&mut self, body: &MeshBody) -> Result<BodyId, KernelError> {
        self.kernel.mesh(body)
    }

    fn boolean(&mut self, op: BooleanOp, target: BodyId, tool: BodyId) -> Result<BodyId, KernelError> {
        self.kernel.boolean(op, target, tool)
    }
}

/// Evaluates a plan through the geometry kernel and returns its body handle.
///
/// The caller owns the handle and must eventually release it through [`SolidKernel::clear`].
pub fn evaluate(plan: &SolidPlan, kernel: &mut dyn SolidKernel) -> Result<BodyId, KernelError> {
    evaluate_with(plan, &mut KernelBackend::new(kernel))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use geom::DVec3;
    use kernel_truck::TruckKernel;

    use super::*;

    fn rectangle(width: f64, height: f64) -> Profile {
        Profile::rectangle(width, height).expect("a positive rectangle")
    }

    fn plan(width: f64, height: f64, depth: f64, tool: Option<SolidPlan>) -> SolidPlan {
        let mut builder =
            PlanBuilder::new().extrude(rectangle(width, height), Frame::WORLD, depth).expect("a base extrusion");
        if let Some(tool) = tool {
            builder = builder.boolean(BooleanOp::Difference, tool);
        }
        builder.build().expect("a plan with a base")
    }

    fn plan_at(width: f64, height: f64, origin: DVec3, depth: f64) -> SolidPlan {
        PlanBuilder::new()
            .extrude(rectangle(width, height), Frame { origin, ..Frame::WORLD }, depth)
            .expect("a base extrusion")
            .build()
            .expect("a plan with a base")
    }

    #[test]
    fn a_single_extrusion_plan_builds_the_expected_volume() {
        let mut kernel = TruckKernel::new();
        let body = evaluate(&plan(100.0, 50.0, 18.0, None), &mut kernel).expect("evaluated body");
        let volume = kernel.measure(body).expect("body measure").volume;
        assert!((volume - 90_000.0).abs() < 1.0e-6, "volume was {volume}");
        assert_eq!(kernel.body_count(), 1);
    }

    #[test]
    fn a_boolean_step_subtracts_its_tool_plan() {
        let mut kernel = TruckKernel::new();
        // The tool overshoots the base below, as a cutting tool does, so the boolean has no
        // coincident faces to resolve.
        let tool = plan_at(10.0, 10.0, DVec3::new(10.0, 10.0, -1.0), 20.0);
        let body = plan(100.0, 50.0, 18.0, Some(tool));
        let body = evaluate(&body, &mut kernel).expect("evaluated body");
        let volume = kernel.measure(body).expect("body measure").volume;
        assert!((volume - (90_000.0 - 1_800.0)).abs() < 1.0e-6, "volume was {volume}");
    }

    #[test]
    fn a_plan_without_a_base_is_refused() {
        let error = PlanBuilder::new().build().expect_err("a plan needs a base");
        assert!(matches!(error, KernelError::InvalidInput(_)));
    }

    #[test]
    fn a_mesh_base_builds_the_body_it_describes() {
        // A unit tetrahedron with a 10 mm leg: a body no extrusion describes, so the plan hands the
        // mesh to the kernel as it is.
        use geom::DVec3;

        let corners =
            vec![DVec3::ZERO, DVec3::new(10.0, 0.0, 0.0), DVec3::new(0.0, 10.0, 0.0), DVec3::new(0.0, 0.0, 10.0)];
        let body = MeshBody::new(corners, vec![[0, 2, 1], [0, 1, 3], [1, 2, 3], [2, 0, 3]]).expect("a closed mesh");
        let plan = PlanBuilder::new().mesh(body).expect("a mesh base").build().expect("a plan");
        assert!(plan.base().extrude().is_none(), "the base is a mesh");
        assert!(plan.base().mesh().is_some(), "and it is there");

        let mut kernel = TruckKernel::new();
        let handle = evaluate(&plan, &mut kernel).expect("an evaluated body");
        let volume = kernel.measure(handle).expect("a measured body").volume;
        assert!((volume - 1000.0 / 6.0).abs() < 1.0, "volume was {volume}");
    }

    #[test]
    fn a_second_base_is_refused() {
        let builder = PlanBuilder::new().extrude(rectangle(10.0, 10.0), Frame::WORLD, 5.0).expect("first base");
        let error = builder.extrude(rectangle(10.0, 10.0), Frame::WORLD, 5.0).expect_err("a plan has one base");
        assert!(matches!(error, KernelError::InvalidInput(_)));
    }

    #[test]
    fn a_deeply_nested_plan_is_refused() {
        struct CountingBackend;

        impl PlanBackend for CountingBackend {
            type Body = u32;

            fn extrude(&mut self, _extrude: &Extrude) -> Result<u32, KernelError> {
                Ok(1)
            }

            fn mesh(&mut self, _body: &MeshBody) -> Result<u32, KernelError> {
                Ok(2)
            }

            fn boolean(&mut self, _op: BooleanOp, target: u32, _tool: u32) -> Result<u32, KernelError> {
                Ok(target)
            }
        }

        let mut nested = plan(10.0, 10.0, 5.0, None);
        for _ in 0..=MAX_PLAN_DEPTH {
            nested = PlanBuilder::new()
                .extrude(rectangle(10.0, 10.0), Frame::WORLD, 5.0)
                .expect("base")
                .boolean(BooleanOp::Difference, nested)
                .build()
                .expect("nested plan");
        }
        let error = evaluate_with(&nested, &mut CountingBackend).expect_err("the nesting is too deep");
        assert!(error.to_string().contains("nests deeper"), "{error}");
    }

    #[test]
    fn an_invalid_frame_or_height_is_refused() {
        let profile = rectangle(10.0, 10.0);
        assert!(Extrude::new(profile.clone(), Frame::WORLD, 0.0).is_err());
        assert!(Extrude::new(profile.clone(), Frame::WORLD, f64::NAN).is_err());
        let skewed = Frame { x: DVec3::new(1.0, 1.0, 0.0), ..Frame::WORLD };
        assert!(Extrude::new(profile, skewed, 5.0).is_err());
    }
}
