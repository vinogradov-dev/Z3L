//! The pull of one area of a drawing: an area a contour set's rings enclose becomes a body.
//!
//! A drawing of several rings encloses material, and its own boundaries divide that material into
//! the areas a maker points at (G4). None of them is stored: [`model::Contours::areas`] derives the
//! faces of the drawing, and the area under the pointer is the one that contains the point the ray
//! hit. So an area has no id, and the command names it by a point inside it, in the set's own plane,
//! exactly as the pointer named it.
//!
//! The body is born beside the drawing in the drawing's own frame and takes the area's outer ring as
//! its profile, so every point of the new body stands where the area stood. The set itself is left
//! untouched: the maker pulled one area out of a drawing, not the drawing.
//!
//! A face with a hole is refused with a sentence rather than widened: a body carries one ring until
//! the document stores a profile with holes (G5), and a body with the hole drawn as a subtraction
//! would stop matching its area the moment its height changed.
//!
//! [G5]: ../plan/BACKLOG.md

use geom::{Contact, DVec2};
use model::{AreaError, Constructive, ConstructiveError, Geometry, Id, Path, PathError, Placement, Project};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use text::Message;
use thiserror::Error;
use units::Length;

use crate::commands::{CommandError, CommandOutcome, CommandSpec};
use crate::node_cmds::{parse, validate_name};
use crate::object_cmds::valid_extrusion_height;
use crate::session::Session;

/// Stable id of the area extrusion command.
pub const OBJECT_AREA_EXTRUDE_ID: &str = "object.area.extrude";

/// User-facing label of the area extrusion command.
pub const OBJECT_AREA_EXTRUDE_LABEL: Message = Message::CmdExtrudeArea;

/// Parameters for `object.area.extrude`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ObjectAreaExtrude {
    /// The contour set whose area is pulled.
    pub object: Id,
    /// A point inside the area to pull, in the set's own plane, in micrometres.
    ///
    /// The area is derived rather than stored, so a point names it: the one face of the drawing that
    /// contains it is the face the pointer was over.
    pub area: [Length; 2],
    /// Positive extrusion height along the set's `+Z`.
    pub height: Length,
    /// User-visible name of the body the command creates.
    pub name: String,
}

/// The `object.area.extrude` command.
pub(crate) const OBJECT_AREA_EXTRUDE: CommandSpec = CommandSpec::new::<ObjectAreaExtrude>(
    OBJECT_AREA_EXTRUDE_ID,
    OBJECT_AREA_EXTRUDE_LABEL,
    object_area_extrude_enabled,
    object_area_extrude_run,
);

fn object_area_extrude_enabled(session: &Session, params: &Value) -> bool {
    let Ok(params) = parse::<ObjectAreaExtrude>(params, OBJECT_AREA_EXTRUDE_ID) else {
        return false;
    };
    if !valid_extrusion_height(params.height) || params.name.trim().is_empty() {
        return false;
    }
    AreaBodyPlan::new(session.project(), &params).is_ok()
}

fn object_area_extrude_run(
    project: &mut Project,
    _catalog: &catalog::Directory,
    params: &Value,
    _contact: &dyn Contact,
) -> Result<CommandOutcome, CommandError> {
    let params = parse::<ObjectAreaExtrude>(params, OBJECT_AREA_EXTRUDE_ID)?;
    validate_name(&params.name, OBJECT_AREA_EXTRUDE_ID)?;
    if !valid_extrusion_height(params.height) {
        return Err(CommandError::InvalidParams {
            command: OBJECT_AREA_EXTRUDE_ID,
            message: "the extrusion height must be positive and finite".to_owned(),
        });
    }
    let plan = AreaBodyPlan::new(project, &params).map_err(refuse)?;
    let id = project.add_object(plan.parent, params.name, plan.placement, Geometry::Constructive(plan.body))?;
    Ok(CommandOutcome::created(id))
}

/// The whole calculation of one area pull, without the write.
///
/// Separate from the command so a refusal and a preview read the same body: the caller paints the
/// geometry the ✓ would store, and no second derivation can disagree with it (ADR 0093).
pub struct AreaBodyPlan {
    parent: Id,
    placement: Placement,
    body: Constructive,
}

impl AreaBodyPlan {
    /// Reads the area under the point and builds the body the pull would create.
    pub fn new(project: &Project, params: &ObjectAreaExtrude) -> Result<Self, AreaExtrudeError> {
        let node = project.object(params.object).ok_or(AreaExtrudeError::NotAContourSet { id: params.object })?;
        let set = node.geometry().as_contours().ok_or(AreaExtrudeError::NotAContourSet { id: params.object })?;
        let [x, y] = params.area;
        let point = DVec2::new(x.millimetres_f64(), y.millimetres_f64());
        let area = set.area_at(point).map_err(AreaExtrudeError::Area)?.ok_or(AreaExtrudeError::NoArea)?;
        if !area.holes().is_empty() {
            return Err(AreaExtrudeError::AreaWithHole);
        }
        let path = Path::from_geometry(area.outer()).map_err(AreaExtrudeError::Ring)?;
        let body = Constructive::extrude(path, params.height).map_err(AreaExtrudeError::Body)?;
        let parent = project.parent_of(params.object).ok_or(AreaExtrudeError::NotAContourSet { id: params.object })?;
        let placement = node.placement();
        Ok(Self { parent, placement, body })
    }

    /// The container the new body lands in: the drawing's own.
    pub const fn parent(&self) -> Id {
        self.parent
    }

    /// The frame the new body stands in: the drawing's own, so the area keeps its place.
    pub const fn placement(&self) -> Placement {
        self.placement
    }

    /// The body the pull would create.
    pub const fn body(&self) -> &Constructive {
        &self.body
    }
}

/// Why an area of a drawing cannot become a body.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum AreaExtrudeError {
    /// The node the command names is not a contour set, so it has no area.
    #[error("node {id} is not a drawing of several rings, so it has no area to pull")]
    NotAContourSet {
        /// The node the command named.
        id: Id,
    },
    /// The point falls outside every area of the drawing.
    #[error("the point is not inside any area of the drawing")]
    NoArea,
    /// The area under the point has a hole, which a body does not carry yet.
    #[error("the area under the pointer has a hole, and a body carries one ring yet")]
    AreaWithHole,
    /// The drawing's areas cannot be derived.
    #[error("{0}")]
    Area(AreaError),
    /// The area's ring cannot become a document path.
    #[error("the area's outline cannot be stored: {0}")]
    Ring(PathError),
    /// The body cannot be built from the area.
    #[error("{0}")]
    Body(ConstructiveError),
}

/// The command's refusal: the reason in words, tagged with the command that asked.
fn refuse(error: AreaExtrudeError) -> CommandError {
    CommandError::InvalidParams { command: OBJECT_AREA_EXTRUDE_ID, message: error.to_string() }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use model::{Contours, Geometry, Id, Path, Placement, Project};
    use testkit::{millimetres, sample_catalog, sample_project};

    use crate::CommandRegistry;

    use super::*;

    /// A closed rectangle ring from `(x0, y0)` to `(x1, y1)` in millimetres.
    fn ring(x0: i64, y0: i64, x1: i64, y1: i64) -> Path {
        Path::rectangle([millimetres(x0), millimetres(y0)], [millimetres(x1), millimetres(y1)]).expect("a ring")
    }

    /// A session whose root holds one contour set of the given rings.
    fn drawing(rings: Vec<Path>) -> (Session, Id) {
        let mut project = sample_project();
        let root = project.root().id();
        let set = Contours::new(rings).expect("a contour set");
        let id = project.add_object(root, "Union", Placement::IDENTITY, Geometry::Contours(set)).expect("a set object");
        (Session::with_directory(project, sample_catalog()), id)
    }

    /// A project whose root holds one contour set and one plain ring, for the kind refusals.
    fn drawing_and_path() -> (Project, Id, Id) {
        let mut project = sample_project();
        let root = project.root().id();
        let set = project
            .add_object(
                root,
                "Union",
                Placement::IDENTITY,
                Geometry::Contours(Contours::new(vec![ring(0, 0, 100, 50)]).expect("a contour set")),
            )
            .expect("a set object");
        let path = project
            .add_object(root, "Ring", Placement::IDENTITY, Geometry::Path(ring(200, 0, 300, 50)))
            .expect("a path object");
        (project, set, path)
    }

    fn params(object: Id, x: i64, y: i64, height: i64) -> Value {
        serde_json::to_value(ObjectAreaExtrude {
            object,
            area: [millimetres(x), millimetres(y)],
            height: millimetres(height),
            name: "Pulled area".to_owned(),
        })
        .expect("params serialize")
    }

    #[test]
    fn a_pulled_area_is_one_body_beside_the_drawing() {
        // A frame drawn as two nested rings: the opening is its own area.
        let (mut session, set) = drawing(vec![ring(0, 0, 100, 50), ring(20, 10, 60, 30)]);
        let revision = session.project().revision();

        let outcome = session.dispatch(OBJECT_AREA_EXTRUDE_ID, &params(set, 40, 20, 120)).expect("the area is pulled");

        let id = outcome.created_id().expect("the command returns the new body");
        let project = session.project();
        let object = project.object(id).expect("the new body exists");
        assert_eq!(object.name(), "Pulled area");
        let body = object.geometry().as_constructive().expect("a body, not a drawing");
        assert_eq!(body.height(), Some(millimetres(120)), "the typed height is stored");
        assert_eq!(
            body.contour().expect("a ring").bounds_micrometres(),
            ([20_000, 10_000], [60_000, 30_000]),
            "the body stands where the opening stood"
        );
        assert_eq!(
            project.object(set).expect("the drawing stays").geometry().as_contours().map(Contours::len),
            Some(2)
        );
        assert_eq!(project.revision().get(), revision.get() + 1, "one revision");
        assert_eq!(project.parent_of(id), project.parent_of(set), "the body is a sibling of the drawing");

        session.undo().expect("one undo entry");
        assert!(session.project().object(id).is_none(), "undo takes the body away");
        assert!(session.project().object(set).is_some(), "undo leaves the drawing");
        assert!(!session.can_undo(), "the whole pull was one entry");
    }

    #[test]
    fn an_area_with_a_hole_is_refused_and_writes_nothing() {
        let (mut session, set) = drawing(vec![ring(0, 0, 100, 50), ring(20, 10, 60, 30)]);
        let revision = session.project().revision();

        let error = session.dispatch(OBJECT_AREA_EXTRUDE_ID, &params(set, 5, 5, 120)).expect_err("a hole is refused");

        assert!(error.to_string().contains("hole"), "the sentence names the hole: {error}");
        assert_eq!(session.project().revision(), revision, "nothing was written");
        assert!(!session.can_undo(), "a refused command makes no entry");
        let spec = CommandRegistry.get(OBJECT_AREA_EXTRUDE_ID).expect("a registered command");
        assert!(!spec.enabled(&session, &params(set, 5, 5, 120)), "the cell is off over a hole");
    }

    #[test]
    fn a_point_outside_every_area_is_refused() {
        let (mut session, set) = drawing(vec![ring(0, 0, 100, 50)]);
        let revision = session.project().revision();

        let error = session.dispatch(OBJECT_AREA_EXTRUDE_ID, &params(set, 300, 300, 120)).expect_err("nothing to pull");

        assert!(error.to_string().contains("not inside any area"), "{error}");
        assert_eq!(session.project().revision(), revision);
    }

    #[test]
    fn a_node_that_is_not_a_drawing_is_refused() {
        let (project, set, path) = drawing_and_path();
        let mut session = Session::with_directory(project, sample_catalog());
        let spec = CommandRegistry.get(OBJECT_AREA_EXTRUDE_ID).expect("a registered command");

        let on_a_path = params(path, 5, 5, 100);
        let error = session.dispatch(OBJECT_AREA_EXTRUDE_ID, &on_a_path).expect_err("a path has no areas");
        assert!(error.to_string().contains("not a drawing"), "{error}");
        assert!(!spec.enabled(&session, &on_a_path), "the cell is off on a single ring");

        let missing = params(Id::new(9_999).expect("id"), 5, 5, 100);
        let error = session.dispatch(OBJECT_AREA_EXTRUDE_ID, &missing).expect_err("a missing node is refused");
        assert!(error.to_string().contains("not a drawing"), "{error}");

        assert!(spec.enabled(&session, &params(set, 5, 5, 100)), "a drawing with an area is pullable");
        assert!(session.project().object(path).is_some(), "the refused command wrote nothing");
    }

    #[test]
    fn one_ring_of_a_drawing_is_extruded_as_it_always_was() {
        let (mut session, set) = drawing(vec![ring(0, 0, 100, 50)]);

        let outcome = session.dispatch(OBJECT_AREA_EXTRUDE_ID, &params(set, 50, 25, 80)).expect("the area is pulled");

        let id = outcome.created_id().expect("the new body");
        let body = session.project().object(id).and_then(|object| object.geometry().as_constructive()).expect("a body");
        assert_eq!(body.contour().expect("a ring").bounds_micrometres(), ([0, 0], [100_000, 50_000]));
    }
}
