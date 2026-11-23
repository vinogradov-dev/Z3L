//! Test support: the fixtures every crate builds its tests from.
//!
//! Compiled for this crate's own tests and, through the `testing` feature, for `testkit`,
//! which re-exports [`millimetres`], [`open_path`], [`cube_mesh`], [`sample_project`],
//! [`sample_product_project`] and [`cross_rings`]. It is the
//! one place those fixtures are defined (C7): a crate's tests may not reach back into `testkit`,
//! because a dev-dependency on a crate that already depends on this one would compile this crate a
//! second time and every model type would come in two incompatible versions.
//!
//! Nothing here is product code; the feature is off in a release build.

use crate::{
    ClassCode, Container, Contour, ContourSegment, Envelope, Id, Mesh, Panel, PanelNominalSize, Path, Placement,
    Project,
};
use units::{Degrees, Length};

/// Whole millimetres as an exact length; the unit every fixture is written in.
#[allow(clippy::expect_used)]
pub fn millimetres(value: i64) -> Length {
    Length::from_millimetres(value).expect("whole millimetres fit a length")
}

/// An open polyline through `points` in millimetres — the drawn chain a test edits.
#[allow(clippy::expect_used)]
pub fn open_path(points: Vec<[Length; 2]>) -> Path {
    Path::polyline(points, false).expect("an open path")
}

/// A closed box of `edge` centred on the local origin.
///
/// Picking, gizmo and cache tests need a solid around the origin rather than the document's own
/// base-corner box, so the fixture names the centring it wants instead of borrowing the primitive's
/// anchor.
#[allow(clippy::expect_used)]
pub fn cube_mesh(edge: Length) -> Mesh {
    let half = Length::from_micrometres(edge.micrometres() / 2);
    let low = Length::from_micrometres(-half.micrometres());
    Mesh::cuboid([low; 3], [half; 3]).expect("a cube has a positive edge")
}

/// A small valid project: a root, one panel and one component container with a panel.
#[allow(clippy::expect_used)]
pub fn sample_project() -> Project {
    let size = PanelNominalSize::new(millimetres(700), millimetres(500)).expect("sample panel size is within limits");
    let shelf = Panel::new(Id::new(2).expect("non-zero id"), "Shelf", size, None, millimetres(18), Placement::IDENTITY)
        .expect("sample shelf is valid");
    let front = Panel::new(Id::new(4).expect("non-zero id"), "Front", size, None, millimetres(18), Placement::IDENTITY)
        .expect("sample front is valid");
    let mut drawer =
        Container::new(Id::new(3).expect("non-zero id"), "Drawer", Placement::IDENTITY, vec![front.into()]);
    drawer.set_component_marker(true).expect("sample drawer is a component");
    let root = Container::new(
        Id::new(1).expect("non-zero id"),
        "Root",
        Placement::IDENTITY,
        vec![shelf.into(), drawer.into()],
    );
    Project::new(root).expect("sample project is valid")
}

/// The sample project with one product beside its other nodes, and that product's id.
///
/// The product is a 600 x 560 x 720 mm box with the placeholder class and no children yet, so a
/// test opens it and creates furniture inside it (ADR 0098). Every other fixture and id stays the
/// same, which keeps the guard's change to the tests that create parts in one place.
#[allow(clippy::expect_used)]
pub fn sample_product_project() -> (Project, Id) {
    let mut project = sample_project();
    let root = project.root().id();
    let envelope = Envelope::new(millimetres(600), millimetres(560), millimetres(720)).expect("a valid box");
    let product = project
        .add_product(root, "Cabinet", ClassCode::GENERAL_PRODUCT, envelope, Placement::IDENTITY)
        .expect("a product beside the sample nodes");
    (project, product)
}

/// The two rings of the owner's `cross.z3l` intersection, as that file stored them (schema 31).
///
/// The drawing was a boolean's result written back into document units, so each ring's arc end is
/// derived from a rounded centre, radius and angles and misses the line it meets by 4.6, 6.7, 10.1
/// and 12.4 µm. That is inside the joint tolerance the document judges a stored ring by (ADR 0072)
/// and far outside the nanometre the geometry layer welds at, so the fixture is what a ring looks
/// like when it is closed for the document and open for an exact reader (X103). The two rings are
/// separate pieces of the same drawing, which makes the union of them a contour set of two.
#[allow(clippy::expect_used)]
pub fn cross_rings() -> [Contour; 2] {
    let point = |x: i64, y: i64| [Length::from_micrometres(x), Length::from_micrometres(y)];
    let line = |start: [i64; 2], end: [i64; 2]| {
        ContourSegment::line(point(start[0], start[1]), point(end[0], end[1])).expect("a stored line")
    };
    let arc = |center: [i64; 2], radius: i64, start_angle: i64, sweep: i64| {
        ContourSegment::arc(
            point(center[0], center[1]),
            Length::from_micrometres(radius),
            Degrees::from_millidegrees(start_angle),
            Degrees::from_millidegrees(sweep),
        )
        .expect("a stored arc")
    };
    let lower = Contour::new(vec![
        line([-341_635, -670_708], [341_634, -670_708]),
        line([341_634, -670_708], [341_634, -275_209]),
        arc([-107_616, -1_505_250], 1_309_514, 69_936, 30_358),
        line([-341_635, -216_816], [-341_635, -670_708]),
    ])
    .expect("the stored lower ring");
    let upper = Contour::new(vec![
        line([341_634, 283_109], [341_634, 670_708]),
        line([341_634, 670_708], [-341_635, 670_708]),
        line([-341_635, 670_708], [-341_635, 428_500]),
        arc([159_222, 1_104_075], 840_987, -126_552, 49_080),
    ])
    .expect("the stored upper ring");
    [lower, upper]
}
