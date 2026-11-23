//! Contour booleans: what two drawn rings leave when they are united, subtracted or intersected.
//!
//! Every command reads its operands' rings into the base ring's flat frame, runs the exact 2D
//! overlay of ADR 0043 ([`geom::union_regions`] and its two siblings) and stores the result as a
//! **new object** beside the base, in the base's frame: one ring is a path, several rings are a
//! contour set ([`model::Contours`]). The sources are consumed in the same command and the same
//! undo entry, except an object marked as a tool, which stays in the tree and never reaches the
//! result (ADR 0194).
//!
//! An operation that leaves nothing to draw changes nothing and says why: rings that never cross
//! leave what they were, an intersection of two separate figures is empty, and a hole is an area
//! rather than a ring, which the document does not carry yet (G4).
//!
//! [`ContourBooleanPlan`] is the whole calculation without the write, so a preview draws exactly
//! what the ✓ stores (ADR 0093).
//!
//! [ADR 0043]: ../docs/decisions/0043-region-booleans-trace-the-boundary-of-the-set.md

use std::collections::BTreeSet;

use geom::{Contact, Contour as GeometryContour, DVec2, DVec3, Region, RegionError};
use model::{ContourSegment, Contours, Geometry, Id, Path, Placement, PlacementError, Project, ProjectError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use text::Message;
use thiserror::Error;

use crate::commands::{CommandError, CommandOutcome, CommandSpec};
use crate::node_cmds::{parse, validate_name};
use crate::session::Session;

/// Stable id of the union command.
pub const CONTOUR_UNION_ID: &str = "contour.boolean.union";

/// User-facing label of the union command.
pub const CONTOUR_UNION_LABEL: Message = Message::ToolUnion;

/// Stable id of the subtraction command.
pub const CONTOUR_SUBTRACT_ID: &str = "contour.boolean.subtract";

/// User-facing label of the subtraction command.
pub const CONTOUR_SUBTRACT_LABEL: Message = Message::ToolSubtract;

/// Stable id of the intersection command.
pub const CONTOUR_INTERSECT_ID: &str = "contour.boolean.intersect";

/// User-facing label of the intersection command.
pub const CONTOUR_INTERSECT_LABEL: Message = Message::ToolIntersect;

/// Largest number of rings one operation may take besides its base.
///
/// The overlay tests every pair of edges, so the input is bounded here rather than refused deep in
/// the geometry layer.
pub const MAX_BOOLEAN_OPERANDS: usize = 32;

/// How far off the base plane an operand's normal may lean before the rings are not coplanar.
///
/// A drawing is exact in its own plane, so the only quantity that can be off is the placement's own
/// rounding; a micrometre of lean over a metre is already far past this bound.
const PLANE_TOLERANCE: f64 = 1.0e-6;

/// Parameters of a contour boolean.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContourBoolean {
    /// The base ring: what the others are united with or intersected with, and what is cut in a
    /// subtraction. The result is born in this ring's frame.
    pub base: Id,
    /// The other rings, one or more, none of them the base.
    pub operands: Vec<Id>,
    /// User-visible name of the object the operation creates.
    pub name: String,
}

/// The `contour.boolean.union` command.
pub(crate) const CONTOUR_UNION: CommandSpec =
    CommandSpec::new::<ContourBoolean>(CONTOUR_UNION_ID, CONTOUR_UNION_LABEL, union_enabled, union_run);

/// The `contour.boolean.subtract` command.
pub(crate) const CONTOUR_SUBTRACT: CommandSpec =
    CommandSpec::new::<ContourBoolean>(CONTOUR_SUBTRACT_ID, CONTOUR_SUBTRACT_LABEL, subtract_enabled, subtract_run);

/// The `contour.boolean.intersect` command.
pub(crate) const CONTOUR_INTERSECT: CommandSpec =
    CommandSpec::new::<ContourBoolean>(CONTOUR_INTERSECT_ID, CONTOUR_INTERSECT_LABEL, intersect_enabled, intersect_run);

/// Which overlay a contour boolean runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContourOp {
    /// Everything either figure covers.
    Union,
    /// The base with the operands cut out of it.
    Subtract,
    /// What every figure covers at once.
    Intersect,
}

impl ContourOp {
    /// The command id that stores this operation.
    pub const fn command(self) -> &'static str {
        match self {
            Self::Union => CONTOUR_UNION_ID,
            Self::Subtract => CONTOUR_SUBTRACT_ID,
            Self::Intersect => CONTOUR_INTERSECT_ID,
        }
    }

    /// The regions the operation's membership rule leaves.
    fn combine(self, base: &Region, operands: &[Region]) -> Result<Vec<Region>, RegionError> {
        match self {
            Self::Union => {
                let mut all = Vec::with_capacity(operands.len() + 1);
                all.push(base.clone());
                all.extend_from_slice(operands);
                geom::union_regions(&all)
            }
            Self::Subtract => geom::subtract_regions(std::slice::from_ref(base), operands),
            Self::Intersect => geom::intersect_regions(std::slice::from_ref(base), operands),
        }
    }
}

/// What one contour boolean would write, computed without touching the document.
///
/// A preview and the command read this one value, so the frame the user judges and the object the
/// ✓ stores cannot disagree (ADR 0093). The plan carries the base's frame, the rings the operation
/// leaves and the sources the command will consume.
#[derive(Clone, Debug, PartialEq)]
pub struct ContourBooleanPlan {
    parent: Id,
    placement: Placement,
    paths: Vec<Path>,
    consumed: Vec<Id>,
}

impl ContourBooleanPlan {
    /// Reads the operands, runs the overlay and names what the command would store.
    pub fn new(project: &Project, params: &ContourBoolean, op: ContourOp) -> Result<Self, BooleanError> {
        let base_object = project.object(params.base).ok_or(ProjectError::NodeNotFound { id: params.base })?;
        let parent = project.parent_of(params.base).ok_or(ProjectError::NodeNotFound { id: params.base })?;
        let placement = base_object.placement();

        let base_ring = ring_in_base(project, params.base, params.base)?;
        let base_region = Region::new(base_ring, Vec::new())?;
        let mut operand_regions = Vec::with_capacity(params.operands.len());
        for id in &params.operands {
            operand_regions.push(Region::new(ring_in_base(project, params.base, *id)?, Vec::new())?);
        }

        let regions = op.combine(&base_region, &operand_regions)?;
        if regions.is_empty() {
            return Err(BooleanError::NothingCreated { why: "the rings do not meet, so nothing is left" });
        }
        if regions.iter().any(|region| !region.holes().is_empty()) {
            return Err(BooleanError::Hole);
        }
        let unchanged = regions.len() == 1
            && std::iter::once(&base_region).chain(operand_regions.iter()).any(|input| &regions[0] == input);
        if unchanged {
            return Err(BooleanError::NothingCreated {
                why: "the result is one of the picked rings, so the operation leaves it as it was",
            });
        }

        let mut paths = Vec::with_capacity(regions.len());
        for region in &regions {
            paths.push(path_of_ring(region.outer())?);
        }

        let mut consumed = Vec::with_capacity(params.operands.len() + 1);
        for id in std::iter::once(&params.base).chain(params.operands.iter()) {
            let object = project.object(*id).ok_or(ProjectError::NodeNotFound { id: *id })?;
            if !object.is_tool() {
                consumed.push(*id);
            }
        }
        Ok(Self { parent, placement, paths, consumed })
    }

    /// The geometry the new object carries: one ring is a path, several are a contour set.
    pub fn geometry(&self) -> Result<Geometry, BooleanError> {
        match self.paths.as_slice() {
            [] => Err(BooleanError::NothingCreated { why: "the operation leaves no ring" }),
            [single] => Ok(Geometry::Path(single.clone())),
            several => Ok(Geometry::Contours(Contours::new(several.to_vec())?)),
        }
    }

    /// The container the new object lands in: the base's own.
    pub const fn parent(&self) -> Id {
        self.parent
    }

    /// The placement of the new object: the base's, so every ring stays where the user drew it.
    pub const fn placement(&self) -> Placement {
        self.placement
    }

    /// The rings the operation leaves, in the order the overlay produced them.
    pub fn paths(&self) -> &[Path] {
        &self.paths
    }

    /// The sources the command consumes, in selection order: the base first.
    pub fn consumed(&self) -> &[Id] {
        &self.consumed
    }
}

/// Why a contour boolean cannot store its result.
#[derive(Clone, Debug, Error)]
pub enum BooleanError {
    /// A node the operation names is not a drawn contour, so it has no ring.
    #[error("node {id} is not a drawn contour, so a boolean cannot take it")]
    NotAContour {
        /// The node that was named.
        id: Id,
    },
    /// A named contour is an open chain, which encloses no material.
    #[error("contour {id} is an open chain, so it encloses no area to combine")]
    OpenRing {
        /// The chain that was named.
        id: Id,
    },
    /// A named contour is not a ring the document accepts.
    #[error("contour {id} is not a ring: {error}")]
    Ring {
        /// The chain that was named.
        id: Id,
        /// The ring rule it broke.
        error: String,
    },
    /// Two rings do not lie in one plane, so they cannot be combined in 2D.
    #[error("contour {id} does not lie in the plane of the base ring")]
    OffPlane {
        /// The chain that leans out of the plane.
        id: Id,
    },
    /// The operation would leave nothing to draw.
    #[error("nothing to create: {why}")]
    NothingCreated {
        /// The reason in words, so the status bar reads as a sentence.
        why: &'static str,
    },
    /// The result has a hole, and a hole is an area rather than a ring.
    #[error(
        "the result is a hole, and a hole is an area rather than a ring; subtracting a figure that never crosses the outer ring leaves nothing to draw"
    )]
    Hole,
    /// Two operands are the same node, or more were named than one operation takes.
    #[error("{why}")]
    Operands {
        /// The reason in words.
        why: String,
    },
    /// The document refused a step.
    #[error(transparent)]
    Model(#[from] ProjectError),
    /// A placement between two drawings is not a real frame.
    #[error(transparent)]
    Placement(#[from] PlacementError),
    /// The overlay itself failed.
    #[error(transparent)]
    Overlay(#[from] RegionError),
    /// The ring the overlay produced cannot be stored as a document path.
    #[error("the operation produced a ring the document cannot hold: {error}")]
    ResultRing {
        /// The ring rule it broke.
        error: String,
    },
    /// The rings the overlay produced cannot be stored as a contour set.
    #[error(transparent)]
    Contours(#[from] model::ContoursError),
}

/// Reads one operand's ring into the base's flat frame.
///
/// An operand is a node that draws one closed chain — a path object or a constructive body, whose
/// profile is the path it grew from ([`Geometry::profile`]) — so a body drawn and extruded is an
/// operand exactly as the drawing it was made of (X98). A drawing is a flat shape in its own `XY`
/// plane, so the frame between two drawings is a turn about the origin, a move, and — for a drawing
/// that faces down — a mirror. Any other relationship is refused: two rings that do not share a
/// plane have no 2D overlay.
fn ring_in_base(project: &Project, base: Id, node: Id) -> Result<GeometryContour, BooleanError> {
    let path = operand_profile(project, node).ok_or(BooleanError::NotAContour { id: node })?;
    if !path.is_closed() {
        return Err(BooleanError::OpenRing { id: node });
    }
    let contour = path.to_contour().map_err(|error| BooleanError::Ring { id: node, error: error.to_string() })?;
    let ring = contour.to_geometry().map_err(|error| BooleanError::Ring { id: node, error: error.to_string() })?;
    if node == base {
        return Ok(ring);
    }
    let base_world = project.world_placement(base).ok_or(ProjectError::NodeNotFound { id: base })?;
    let node_world = project.world_placement(node).ok_or(ProjectError::NodeNotFound { id: node })?;
    let relative = Placement::compose(base_world.inverse()?, node_world)?;
    let rotation = relative.rotation();
    let x_axis = rotation * DVec3::X;
    let z_axis = rotation * DVec3::Z;
    if z_axis.x.abs() > PLANE_TOLERANCE || z_axis.y.abs() > PLANE_TOLERANCE {
        return Err(BooleanError::OffPlane { id: node });
    }
    let translation = relative.translation();
    let offset = DVec2::new(translation.x, translation.y);
    let placed = ring
        .placed(x_axis.y.atan2(x_axis.x), z_axis.z < 0.0, offset)
        .map_err(|error| BooleanError::Ring { id: node, error: error.to_string() })?;
    Ok(placed)
}

/// One exact ring as a document path, with fresh segment ids.
fn path_of_ring(ring: &GeometryContour) -> Result<Path, BooleanError> {
    let mut segments = Vec::with_capacity(ring.segments().len());
    for segment in ring.segments() {
        segments.push(
            ContourSegment::from_geom(*segment)
                .map_err(|error| BooleanError::ResultRing { error: error.to_string() })?,
        );
    }
    Path::new(segments, true).map_err(|error| BooleanError::ResultRing { error: error.to_string() })
}

/// Whether the parameters name rings a boolean could read, without running the overlay.
fn operands_ready(session: &Session, params: &Value, command: &'static str) -> bool {
    let Ok(params) = parse::<ContourBoolean>(params, command) else {
        return false;
    };
    if params.name.trim().is_empty() || params.operands.is_empty() {
        return false;
    }
    if params.operands.len() > MAX_BOOLEAN_OPERANDS {
        return false;
    }
    let project = session.project();
    let mut seen = BTreeSet::new();
    seen.insert(params.base);
    if !is_contour_operand(project, params.base) {
        return false;
    }
    for id in &params.operands {
        if !seen.insert(*id) || !is_contour_operand(project, *id) {
            return false;
        }
    }
    true
}

/// The closed chain a node draws, when it draws one.
///
/// The **drawn** ring a contour boolean reads: a path object's chain and nothing else.
///
/// A constructive body deliberately does not answer. Combining bodies is what the Model tab's
/// booleans do, on solids; combining their profiles here silently turned a body into a drawing and
/// consumed the body on the way (ADR 0194, revised by W2). A contour set, a mesh, a primitive and a
/// node that is not an object answer with nothing either.
fn operand_profile(project: &Project, id: Id) -> Option<&Path> {
    project.object(id).and_then(|object| object.geometry().as_path())
}

/// Whether a node draws one closed ring a contour boolean can take, at any placement.
///
/// The three Sketch cells and the armed mode both read this. A body is not one: it is cut on the
/// Model tab, where the operands are solids (ADR 0194, revised by W2).
pub fn is_contour_operand(project: &Project, id: Id) -> bool {
    operand_profile(project, id).is_some_and(|path| path.is_closed() && path.to_contour().is_ok())
}

fn union_enabled(session: &Session, params: &Value) -> bool {
    operands_ready(session, params, CONTOUR_UNION_ID)
}

fn subtract_enabled(session: &Session, params: &Value) -> bool {
    operands_ready(session, params, CONTOUR_SUBTRACT_ID)
}

fn intersect_enabled(session: &Session, params: &Value) -> bool {
    operands_ready(session, params, CONTOUR_INTERSECT_ID)
}

fn union_run(
    project: &mut Project,
    _catalog: &catalog::Directory,
    params: &Value,
    _contact: &dyn Contact,
) -> Result<CommandOutcome, CommandError> {
    run(project, params, ContourOp::Union)
}

fn subtract_run(
    project: &mut Project,
    _catalog: &catalog::Directory,
    params: &Value,
    _contact: &dyn Contact,
) -> Result<CommandOutcome, CommandError> {
    run(project, params, ContourOp::Subtract)
}

fn intersect_run(
    project: &mut Project,
    _catalog: &catalog::Directory,
    params: &Value,
    _contact: &dyn Contact,
) -> Result<CommandOutcome, CommandError> {
    run(project, params, ContourOp::Intersect)
}

/// Runs one contour boolean and writes its object, then consumes the sources.
fn run(project: &mut Project, params: &Value, op: ContourOp) -> Result<CommandOutcome, CommandError> {
    let command = op.command();
    let params = parse::<ContourBoolean>(params, command)?;
    validate_name(&params.name, command)?;
    let plan = ContourBooleanPlan::new(project, &params, op).map_err(|error| refuse(command, error))?;
    let geometry = plan.geometry().map_err(|error| refuse(command, error))?;
    let id = project.add_object(plan.parent(), params.name, plan.placement(), geometry)?;
    let consumed = plan.consumed();
    if !consumed.is_empty() {
        project.remove_nodes(consumed)?;
    }
    Ok(CommandOutcome::created(id))
}

/// The command's refusal: the reason in words, tagged with the command that asked.
fn refuse(command: &'static str, error: BooleanError) -> CommandError {
    CommandError::InvalidParams { command, message: error.to_string() }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use model::{Geometry, Id, Path, Placement, Project};
    use testkit::{cross_rings, millimetres, sample_catalog, sample_project};
    use units::Length;

    use crate::CommandRegistry;
    use crate::area_cmds::{OBJECT_AREA_EXTRUDE_ID, ObjectAreaExtrude};

    use super::*;

    /// A rectangle ring from `(x0, y0)` to `(x1, y1)` in millimetres, as a path object at `parent`.
    fn ring(project: &mut Project, parent: Id, name: &str, x0: i64, y0: i64, x1: i64, y1: i64) -> Id {
        let path = Path::rectangle([millimetres(x0), millimetres(y0)], [millimetres(x1), millimetres(y1)])
            .expect("a rectangle ring");
        project.add_object(parent, name, Placement::IDENTITY, Geometry::Path(path)).expect("a path object")
    }

    /// A drawn circle of 50 mm radius at `(x, 0)` in millimetres, as a path object at `parent`.
    fn circle(project: &mut Project, parent: Id, name: &str, x: i64) -> Id {
        let path = Path::circle([millimetres(x), millimetres(0)], millimetres(50)).expect("a circle");
        project.add_object(parent, name, Placement::IDENTITY, Geometry::Path(path)).expect("a circle object")
    }

    /// A session whose root holds one rectangle ring per tuple, in the order given.
    fn rings(rects: &[(i64, i64, i64, i64)]) -> (Session, Vec<Id>) {
        let mut project = sample_project();
        let root = project.root().id();
        let mut ids = Vec::with_capacity(rects.len());
        for (index, (x0, y0, x1, y1)) in rects.iter().enumerate() {
            ids.push(ring(&mut project, root, &format!("Ring {index}"), *x0, *y0, *x1, *y1));
        }
        (Session::with_directory(project, sample_catalog()), ids)
    }

    fn params(op_name: &str, base: Id, operands: &[Id]) -> Value {
        serde_json::to_value(ContourBoolean { base, operands: operands.to_vec(), name: op_name.to_owned() })
            .expect("params serialize")
    }

    /// The segment count of every ring a contour set holds, in the order the overlay stored them.
    fn segment_counts(project: &Project, id: Id) -> Vec<usize> {
        let set = project.object(id).expect("the new object").geometry().as_contours().expect("a contour set");
        set.paths().iter().map(|path| path.entries().len()).collect()
    }

    /// The area of a stored ring, in square millimetres, through the geometry layer.
    fn ring_area(path: &Path) -> f64 {
        let ring = path.to_contour().expect("a ring").to_geometry().expect("the geometry ring");
        geom::Region::new(ring, Vec::new()).expect("a region").area()
    }

    #[test]
    fn a_union_of_two_overlapping_rings_is_one_new_object_and_one_undo_entry() {
        let (mut session, ids) = rings(&[(0, 0, 100, 50), (50, 25, 150, 75)]);
        let (base, other) = (ids[0], ids[1]);

        let outcome = session.dispatch(CONTOUR_UNION_ID, &params("Union", base, &[other])).expect("union");

        let id = outcome.created_id().expect("the command returns the new object");
        let project = session.project();
        let object = project.object(id).expect("the new object");
        assert_eq!(object.name(), "Union");
        let path = object.geometry().as_path().expect("a union of two crossing rings is one ring");
        assert_eq!(path.bounds_micrometres(), ([0, 0], [150_000, 75_000]), "the union keeps both extents");
        assert!((ring_area(path) - 8_750.0).abs() < 0.01, "the shared corner is counted once");
        assert!(project.object(base).is_none(), "the base is consumed");
        assert!(project.object(other).is_none(), "the cutter is consumed");

        session.undo().expect("one undo entry");
        assert!(session.project().object(id).is_none(), "undo takes the result away");
        assert!(session.project().object(base).is_some(), "undo puts the base back");
        assert!(session.project().object(other).is_some(), "undo puts the cutter back");
        assert!(!session.can_undo(), "the whole operation was one entry");
    }

    #[test]
    fn a_subtraction_opens_a_notch_and_keeps_one_ring() {
        let (mut session, ids) = rings(&[(0, 0, 100, 50), (80, -10, 120, 30)]);

        let outcome = session.dispatch(CONTOUR_SUBTRACT_ID, &params("Notched", ids[0], &[ids[1]])).expect("subtract");

        let id = outcome.created_id().expect("a new object");
        let path = session
            .project()
            .object(id)
            .expect("the new object")
            .geometry()
            .as_path()
            .expect("a notch leaves one ring")
            .clone();
        assert_eq!(path.bounds_micrometres(), ([0, 0], [100_000, 50_000]), "a notch keeps the extents");
        assert!((ring_area(&path) - (5_000.0 - 20.0 * 30.0)).abs() < 0.01, "the notch is not material");
    }

    #[test]
    fn an_intersection_keeps_the_area_both_rings_cover() {
        let (mut session, ids) = rings(&[(0, 0, 100, 50), (50, 25, 150, 75)]);

        let outcome = session.dispatch(CONTOUR_INTERSECT_ID, &params("Shared", ids[0], &[ids[1]])).expect("intersect");

        let id = outcome.created_id().expect("a new object");
        let path =
            session.project().object(id).expect("the new object").geometry().as_path().expect("one ring").clone();
        assert_eq!(path.bounds_micrometres(), ([50_000, 25_000], [100_000, 50_000]));
        assert!((ring_area(&path) - 50.0 * 25.0).abs() < 0.01, "only the shared corner is material");
    }

    /// The two rings of the owner's `cross.z3l` intersection miss each other by 4.6, 6.7, 10.1 and
    /// 12.4 µm at their arc joints — the rounding a converted arc carries in the document. The
    /// document accepted the drawing, the boolean answered "the rings do not meet" and the area
    /// pull found no area, so a stored drawing was a dead operand (X103).
    #[test]
    fn the_two_rings_of_the_cross_drawing_union_into_one_object_whose_caps_pull() {
        let mut project = sample_project();
        let root = project.root().id();
        let mut bars = Vec::new();
        for (index, ring) in cross_rings().into_iter().enumerate() {
            let path = Path::from(ring);
            bars.push(
                project
                    .add_object(root, format!("Bar {index}"), Placement::IDENTITY, Geometry::Path(path))
                    .expect("a drawn ring"),
            );
        }
        let mut session = Session::with_directory(project, sample_catalog());

        let outcome = session.dispatch(CONTOUR_UNION_ID, &params("Cross", bars[0], &[bars[1]])).expect("the union");
        let id = outcome.created_id().expect("the command returns the new object");
        let pieces = session
            .project()
            .object(id)
            .expect("the new object")
            .geometry()
            .as_contours()
            .expect("two separate pieces are one contour set")
            .paths()
            .len();
        assert_eq!(pieces, 2, "both bars are pieces of the union");

        // Each cap is an area of the drawing, so a pull from inside either of them reaches it.
        let mut pulled = Vec::new();
        for (x, y) in [(0, -400), (0, 400)] {
            let pull = serde_json::to_value(ObjectAreaExtrude {
                object: id,
                area: [millimetres(x), millimetres(y)],
                height: millimetres(18),
                name: "Cap".to_owned(),
            })
            .expect("params serialize");
            let outcome = session.dispatch(OBJECT_AREA_EXTRUDE_ID, &pull).expect("the cap has an area");
            let body = session.project().object(outcome.created_id().expect("the command returns a body"));
            let bounds = body
                .expect("the new body")
                .geometry()
                .profile()
                .expect("a body draws its profile")
                .bounds_micrometres();
            pulled.push(bounds);
        }
        let (lower, upper) = (pulled[0], pulled[1]);
        assert_eq!(lower.0[1], -670_708, "the lower body starts on the bar's stored bottom edge");
        assert!(lower.1[1] > -300_000, "the lower cap's rounded end is part of the body: {lower:?}");
        assert_eq!(upper.1[1], 670_708, "the upper body ends on the bar's stored top edge");
        assert!(upper.0[1] < 300_000, "the upper cap's rounded end is part of the body: {upper:?}");
    }

    /// A union that meets a whole circle leaves it as one 360° arc, and the document refused that
    /// ring, so the drawing the boolean produced could not be stored at all: "a contour needs at
    /// least three segments, got 1" (X104). A circle beside another figure is a circle, not a
    /// three-segment ring invented to satisfy a count.
    #[test]
    fn a_union_that_meets_a_whole_circle_stores_one_ring_per_piece() {
        let mut project = sample_project();
        let root = project.root().id();
        let first = circle(&mut project, root, "Circle", 0);
        let rectangle = ring(&mut project, root, "Rectangle", 200, -50, 300, 50);
        let mut session = Session::with_directory(project, sample_catalog());

        let outcome = session.dispatch(CONTOUR_UNION_ID, &params("Both", first, &[rectangle])).expect("the union");
        let id = outcome.created_id().expect("the command returns one object");
        let counts = segment_counts(session.project(), id);
        assert_eq!(counts, vec![1, 4], "the circle is one full arc and the rectangle its four lines");

        // Two circles are two rings, one arc each.
        let mut project = sample_project();
        let root = project.root().id();
        let first = circle(&mut project, root, "First", 0);
        let second = circle(&mut project, root, "Second", 200);
        let mut session = Session::with_directory(project, sample_catalog());
        let outcome = session.dispatch(CONTOUR_UNION_ID, &params("Both", first, &[second])).expect("the union");
        let id = outcome.created_id().expect("the command returns one object");
        assert_eq!(segment_counts(session.project(), id), vec![1, 1], "each circle is one full arc");
    }

    #[test]
    fn a_subtraction_that_would_leave_a_hole_creates_nothing() {
        let (mut session, ids) = rings(&[(0, 0, 100, 50), (20, 10, 60, 40)]);
        let before = session.project().revision();

        let error =
            session.dispatch(CONTOUR_SUBTRACT_ID, &params("Hole", ids[0], &[ids[1]])).expect_err("a hole is refused");

        assert!(error.to_string().contains("hole"), "the refusal names the reason: {error}");
        assert_eq!(session.project().revision(), before, "a refusal writes nothing");
        assert!(session.project().object(ids[0]).is_some(), "the base stays");
        assert!(session.project().object(ids[1]).is_some(), "the cutter stays");
    }

    #[test]
    fn a_subtraction_that_splits_a_ring_leaves_a_set_of_two_rings() {
        let (mut session, ids) = rings(&[(0, 0, 100, 50), (-10, 10, 110, 40)]);

        let outcome = session.dispatch(CONTOUR_SUBTRACT_ID, &params("Split", ids[0], &[ids[1]])).expect("subtract");

        let id = outcome.created_id().expect("a new object");
        let contours = session
            .project()
            .object(id)
            .expect("the new object")
            .geometry()
            .as_contours()
            .expect("two pieces are a contour set");
        assert_eq!(contours.len(), 2, "the band cuts the board in two");
        let mut tops: Vec<i64> = contours.paths().iter().map(|path| path.bounds_micrometres().1[1]).collect();
        tops.sort_unstable();
        assert_eq!(tops, vec![10_000, 50_000], "one strip below the band and one above it");
    }

    #[test]
    fn rings_in_different_frames_land_in_the_base_frame() {
        let mut project = sample_project();
        let root = project.root().id();
        let base = ring(&mut project, root, "Here", 0, 0, 100, 50);
        // A ring drawn in its own frame and moved 200 mm along X by its placement: the union must
        // read it where it stands, not where it was drawn.
        let placed = Placement::new(geom::DVec3::new(200.0, 0.0, 0.0), geom::DQuat::IDENTITY).expect("a move");
        let path =
            Path::rectangle([millimetres(0), millimetres(0)], [millimetres(100), millimetres(50)]).expect("a ring");
        let moved = project.add_object(root, "There", placed, Geometry::Path(path)).expect("an object");
        let mut session = Session::with_directory(project, sample_catalog());

        let outcome = session.dispatch(CONTOUR_UNION_ID, &params("Union", base, &[moved])).expect("union");

        let id = outcome.created_id().expect("a new object");
        let contours = session
            .project()
            .object(id)
            .expect("the new object")
            .geometry()
            .as_contours()
            .expect("two separate rings are a contour set");
        assert_eq!(contours.len(), 2, "the rings do not touch, so both are kept");
        let mut boxes: Vec<([i64; 2], [i64; 2])> = contours.paths().iter().map(Path::bounds_micrometres).collect();
        boxes.sort_unstable();
        assert_eq!(boxes, vec![([0, 0], [100_000, 50_000]), ([200_000, 0], [300_000, 50_000])]);
    }

    #[test]
    fn a_ring_that_faces_sideways_is_refused_rather_than_guessed() {
        let mut project = sample_project();
        let root = project.root().id();
        let base = ring(&mut project, root, "Board", 0, 0, 100, 50);
        let path =
            Path::rectangle([millimetres(0), millimetres(0)], [millimetres(100), millimetres(50)]).expect("a ring");
        // A quarter turn about X stands the drawing up: its plane no longer meets the base's.
        let stood = Placement::new(geom::DVec3::ZERO, geom::DQuat::from_rotation_x(std::f64::consts::FRAC_PI_2))
            .expect("a turn");
        let upright = project.add_object(root, "Upright", stood, Geometry::Path(path)).expect("an object");
        let mut session = Session::with_directory(project, sample_catalog());

        let error = session.dispatch(CONTOUR_UNION_ID, &params("Union", base, &[upright])).expect_err("off plane");
        assert!(error.to_string().contains("plane"), "the refusal names the plane: {error}");
    }

    #[test]
    fn an_open_chain_is_neither_enabled_nor_stored() {
        let mut project = sample_project();
        let root = project.root().id();
        let base = ring(&mut project, root, "Board", 0, 0, 100, 50);
        let open = Path::polyline(vec![[millimetres(0), millimetres(0)], [millimetres(100), millimetres(0)]], false)
            .expect("an open chain");
        let chain = project.add_object(root, "Chain", Placement::IDENTITY, Geometry::Path(open)).expect("a object");
        let mut session = Session::with_directory(project, sample_catalog());

        let params = params("Union", base, &[chain]);
        let registry = CommandRegistry::new();
        let command = registry.get(CONTOUR_UNION_ID).expect("registered");
        assert!(!command.enabled(&session, &params), "an open chain arms nothing");
        let error = session.dispatch(CONTOUR_UNION_ID, &params).expect_err("an open chain is refused");
        assert!(error.to_string().contains("open chain"), "the refusal names the chain: {error}");
    }

    #[test]
    fn a_tool_ring_survives_and_an_unmarked_one_is_consumed() {
        let mut project = sample_project();
        let root = project.root().id();
        let base = ring(&mut project, root, "Board", 0, 0, 100, 50);
        let cutter = ring(&mut project, root, "Stays", 50, 25, 150, 75);
        project.set_object_tool(cutter, true).expect("mark the cutter");
        let mut session = Session::with_directory(project, sample_catalog());

        let outcome = session.dispatch(CONTOUR_UNION_ID, &params("Union", base, &[cutter])).expect("union");

        assert!(session.project().object(outcome.created_id().expect("a new object")).is_some());
        assert!(session.project().object(base).is_none(), "an unmarked ring is consumed");
        assert!(session.project().object(cutter).is_some(), "a tool stays where it is");
    }

    #[test]
    fn a_circle_cut_by_a_rectangle_stores_the_arc_bounded_piece() {
        // The owner's own case on 2026-11-20: a 650.308 mm circle at (90, -40) and a 730 x 810 board
        // at (155, 645). The intersection is one ring of arcs and lines, and it was refused because
        // the stored arc's endpoint comes from the sum of two rounded angles, twice the slack the
        // document allowed (ADR 0072); the ring now stores and draws.
        let mut project = sample_project();
        let root = project.root().id();
        let circle =
            Path::circle([millimetres(90), millimetres(-40)], Length::from_micrometres(650_308)).expect("a circle");
        let board = Path::rectangle([millimetres(-210), millimetres(240)], [millimetres(520), millimetres(1_050)])
            .expect("a board");
        let base = project.add_object(root, "Board", Placement::IDENTITY, Geometry::Path(board)).expect("an object");
        let cutter =
            project.add_object(root, "Circle", Placement::IDENTITY, Geometry::Path(circle)).expect("an object");
        let mut session = Session::with_directory(project, sample_catalog());

        let outcome =
            session.dispatch(CONTOUR_INTERSECT_ID, &params("Corner", base, &[cutter])).expect("the ring stores");

        let id = outcome.created_id().expect("a new object");
        let path = session.project().object(id).expect("the new object").geometry().as_path().expect("one ring");
        assert!(path.is_closed(), "the result is a ring");
        // The three straight edges are the board's and exact; the arc's own endpoint is derived from
        // whole millidegrees, so it lands within the document's joint tolerance of the edge it meets
        // — 11 µm of radius plus the rounding of its centre (ADR 0072). The piece is the board's
        // corner up to the circle's top.
        let (min, max) = path.bounds_micrometres();
        assert_eq!((min[1], max[0]), (240_000, 520_000), "the bottom and the right edge are the board's");
        assert!((-210_000 - 13..=-210_000).contains(&min[0]), "the left edge is the board's within the slack: {min:?}");
        assert!((610_308 - 13..=610_308).contains(&max[1]), "the top is the circle's within the slack: {max:?}");
    }

    #[test]
    fn two_rings_that_never_meet_leave_nothing_to_intersect() {
        let (mut session, ids) = rings(&[(0, 0, 100, 50), (500, 500, 600, 550)]);

        let error =
            session.dispatch(CONTOUR_INTERSECT_ID, &params("None", ids[0], &[ids[1]])).expect_err("nothing in common");
        assert!(error.to_string().contains("nothing"), "the refusal says so: {error}");
        assert!(session.project().object(ids[0]).is_some());
        assert!(session.project().object(ids[1]).is_some());
    }

    #[test]
    fn the_plan_is_the_command_without_the_write() {
        let mut project = sample_project();
        let root = project.root().id();
        let base = ring(&mut project, root, "Left", 0, 0, 100, 50);
        let other = ring(&mut project, root, "Right", 50, 25, 150, 75);
        let revision = project.revision();
        let request = ContourBoolean { base, operands: vec![other], name: "Union".to_owned() };

        let plan = ContourBooleanPlan::new(&project, &request, ContourOp::Union).expect("a plan");
        assert_eq!(project.revision(), revision, "computing the result writes nothing");
        let previewed = plan.geometry().expect("the previewed geometry");
        assert_eq!(plan.consumed(), &[base, other], "both sources go, the base first");
        assert_eq!(plan.parent(), root);
        assert_eq!(plan.placement(), Placement::IDENTITY);

        let mut session = Session::with_directory(project, sample_catalog());
        let before = session.project().revision();
        let outcome =
            session.dispatch(CONTOUR_UNION_ID, &serde_json::to_value(request).expect("params")).expect("union");
        let written = session
            .project()
            .object(outcome.created_id().expect("the new object"))
            .expect("the object")
            .geometry()
            .clone();
        assert_eq!(previewed, written, "the frame a preview draws is the object the ok stores");
        assert!(session.project().revision() > before, "the command itself does move the revision");
    }

    /// A project whose root holds one extruded rectangle per tuple, and those bodies' ids.
    fn body_project(rects: &[(i64, i64, i64, i64)]) -> (Project, Vec<Id>) {
        let mut project = sample_project();
        let root = project.root().id();
        let mut ids = Vec::with_capacity(rects.len());
        for (index, (x0, y0, x1, y1)) in rects.iter().enumerate() {
            let id = ring(&mut project, root, &format!("Body {index}"), *x0, *y0, *x1, *y1);
            project.extrude_object_path(id, millimetres(18)).expect("the ring becomes a body");
            ids.push(id);
        }
        (project, ids)
    }

    #[test]
    fn a_body_is_refused_and_named_as_one_the_model_tab_cuts() {
        // W2, ADR 0194 revised: a contour boolean combines **drawings**. A body is a solid, and
        // combining its profile here turned the body into a drawing and consumed it on the way —
        // which is what the owner met when a subtraction left a contour instead of a body.
        let (mut project, ids) = body_project(&[(0, 0, 100, 50), (50, 25, 150, 75)]);
        let root = project.root().id();
        let drawn = ring(&mut project, root, "Drawn", 50, 25, 150, 75);
        let [first, second] = [ids[0], ids[1]];
        let mut session = Session::with_directory(project, sample_catalog());
        let before = session.project().revision();
        let before_count = session.project().root().children().len();

        for (base, operands) in [(first, vec![drawn]), (first, vec![second])] {
            let error = session
                .dispatch(CONTOUR_UNION_ID, &params("Union", base, &operands))
                .expect_err("a body is not a drawing");
            let sentence = error.to_string();
            assert!(sentence.contains("drawn contour"), "the refusal says what it needs: {sentence}");
        }
        assert_eq!(session.project().revision(), before, "a refused boolean writes nothing");
        assert_eq!(session.project().root().children().len(), before_count, "and creates nothing");
        for body in [first, second] {
            assert!(session.project().object(body).is_some(), "the body stays where it is");
        }
    }

    #[test]
    fn a_node_that_draws_no_ring_is_named_by_the_refusal() {
        let mut project = sample_project();
        let root = project.root().id();
        let base = ring(&mut project, root, "Board", 0, 0, 100, 50);
        let ball = model::Primitive::sphere(millimetres(20), 16, 8).expect("a sphere");
        let sphere = project.add_object(root, "Ball", Placement::IDENTITY, Geometry::Primitive(ball)).expect("a ball");
        let mut session = Session::with_directory(project, sample_catalog());

        let error = session
            .dispatch(CONTOUR_UNION_ID, &params("Union", base, &[sphere]))
            .expect_err("a primitive draws no ring");

        let sentence = error.to_string();
        assert!(sentence.contains(&sphere.to_string()), "the refusal names the node: {sentence}");
        assert!(sentence.contains("drawn contour"), "and says what it lacks: {sentence}");
    }

    #[test]
    fn the_ready_ring_is_a_drawn_contour_and_nothing_else() {
        let mut project = sample_project();
        let root = project.root().id();
        let drawn = ring(&mut project, root, "Drawn", 0, 0, 100, 50);
        let body = ring(&mut project, root, "Body", 0, 0, 100, 50);
        project.extrude_object_path(body, millimetres(18)).expect("a body");
        let open = testkit::open_path(vec![[millimetres(0), millimetres(0)], [millimetres(100), millimetres(0)]]);
        let chain = project.add_object(root, "Chain", Placement::IDENTITY, Geometry::Path(open)).expect("a chain");
        let ball = model::Primitive::sphere(millimetres(20), 16, 8).expect("a sphere");
        let sphere = project.add_object(root, "Ball", Placement::IDENTITY, Geometry::Primitive(ball)).expect("a ball");
        let mesh = project
            .add_object(root, "Baked", Placement::IDENTITY, Geometry::Mesh(testkit::cube_mesh(millimetres(10))))
            .expect("a mesh object");
        let set = Contours::new(vec![
            Path::rectangle([millimetres(0), millimetres(0)], [millimetres(10), millimetres(10)]).expect("a ring"),
            Path::rectangle([millimetres(20), millimetres(20)], [millimetres(30), millimetres(30)]).expect("a ring"),
        ])
        .expect("a set");
        let split =
            project.add_object(root, "Split", Placement::IDENTITY, Geometry::Contours(set)).expect("a set object");
        let panel = project.panel(Id::new(2).expect("the sample panel id")).expect("the sample shelf").id();

        assert!(is_contour_operand(&project, drawn), "a drawn ring is an operand");
        assert!(!is_contour_operand(&project, body), "a body is cut on the Model tab, not turned into a drawing here");
        assert!(!is_contour_operand(&project, chain), "an open chain is not");
        assert!(!is_contour_operand(&project, sphere), "a primitive is not");
        assert!(!is_contour_operand(&project, mesh), "a mesh is not");
        assert!(!is_contour_operand(&project, split), "a contour set draws several rings, not one");
        assert!(!is_contour_operand(&project, panel), "a panel is not an operand");
    }
}
