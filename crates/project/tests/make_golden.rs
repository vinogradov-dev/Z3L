//! Regenerates the one checked-in golden file from the current serializer.
//!
//! The golden is the compatibility contract of the scene file: it is written by this build, it must
//! keep loading as the schema moves, and a schema bump regenerates it instead of leaving a second
//! file behind (ADR 0096). Run it deliberately, then review the diff:
//!
//! ```sh
//! cargo test -p project --test make_golden -- --ignored
//! ```
//!
//! Nothing in the normal test run touches the source tree, because the test is ignored by default.

#![allow(clippy::expect_used)]

use std::collections::BTreeMap;
use std::fs;

use model::{
    CameraNode, CameraProjection, ClassCode, Constructive, Container, CornerFeatureKind, CutoutDepth, CutoutShape,
    EdgeBanding, EdgeBevel, EdgeId, EdgeRef, Envelope, FaceSide, Footprint, Geometry, Id, ItemId, LightKind, LightNode,
    Object, Panel, PanelNominalSize, PanelRect, Path, Placement, Primitive, Project,
};
use project::SceneFile;
use serde_json::Value;
use units::{Degrees, Length};

fn millimetres(value: i64) -> Length {
    Length::from_millimetres(value).expect("whole millimetres fit a length")
}

fn id(value: u64) -> Id {
    Id::new(value).expect("a non-zero id")
}

/// The fixture every property of the current layout is checked against.
fn scene() -> SceneFile {
    let shelf = Panel::new(
        id(2),
        "Shelf",
        PanelNominalSize::new(millimetres(700), millimetres(500)).expect("size"),
        None,
        millimetres(18),
        Placement::IDENTITY,
    )
    .expect("the shelf is valid");
    let front = Panel::new(
        id(9),
        "Front",
        PanelNominalSize::new(millimetres(600), millimetres(140)).expect("size"),
        None,
        millimetres(18),
        Placement::IDENTITY,
    )
    .expect("the front is valid");

    let closed = Path::polyline(
        vec![
            [Length::ZERO, Length::ZERO],
            [millimetres(600), Length::ZERO],
            [millimetres(600), millimetres(400)],
            [Length::ZERO, millimetres(400)],
        ],
        true,
    )
    .expect("a closed ring");
    let constructive = Constructive::extrude(closed.clone(), millimetres(18)).expect("an extrusion");
    let cutter_ring = Path::rectangle([millimetres(200), millimetres(100)], [millimetres(80), millimetres(80)])
        .expect("a cutting ring");
    let cutter = Constructive::extrude(cutter_ring, millimetres(40)).expect("a cutting body");
    let drawn = Path::polyline(vec![[Length::ZERO, Length::ZERO], [millimetres(120), millimetres(40)]], false)
        .expect("an open path");
    // A contour boolean that leaves several pieces stores them as one drawing: the golden carries
    // the ring set, so a file written by this build is a file the next build loads.
    let second_ring =
        Path::rectangle([millimetres(200), Length::ZERO], [millimetres(320), millimetres(80)]).expect("a second ring");
    let contours = model::Contours::new(vec![closed.clone(), second_ring]).expect("a drawing of two rings");

    let mut drawer = Container::new(id(8), "Drawer", Placement::IDENTITY, vec![front.into()]);
    drawer.set_component_marker(true).expect("the drawer is a component");
    let root = Container::new(
        id(1),
        "Root",
        Placement::IDENTITY,
        vec![
            shelf.into(),
            Object::new(id(3), "Top", Placement::IDENTITY, Geometry::Constructive(constructive.clone())).into(),
            // A body that is the tool of a boolean step: the step names it and the golden carries the
            // reference, so a file written by this build is a file the next build loads.
            Object::new(id(11), "Cutter", Placement::IDENTITY, Geometry::Constructive(cutter)).into(),
            Object::new(id(4), "Sketch", Placement::IDENTITY, Geometry::Path(drawn)).into(),
            Object::new(id(10), "Union", Placement::IDENTITY, Geometry::Contours(contours)).into(),
            Object::new(
                id(5),
                "Ball",
                Placement::IDENTITY,
                Geometry::Primitive(Primitive::sphere(millimetres(50), 32, 16).expect("a sphere")),
            )
            .into(),
            CameraNode::new(id(6), "Presentation", Placement::IDENTITY, CameraProjection::DEFAULT)
                .expect("a camera")
                .into(),
            LightNode::new(id(7), "Key", Placement::IDENTITY, LightKind::Sun, [1.0, 0.95, 0.9], 2.0)
                .expect("a light")
                .into(),
            drawer.into(),
        ],
    );
    let mut project = Project::new(root).expect("the scene is valid");
    project
        .set_edge_banding(
            id(2),
            EdgeBanding::new(
                EdgeId::new(1).expect("edge"),
                ItemId::new(1, 1).expect("material"),
                false,
                millimetres(2),
                millimetres(22),
            )
            .expect("banding"),
        )
        .expect("banding on the shelf");
    project
        .add_cutout(
            id(2),
            CutoutShape::Rectangle(
                PanelRect::new(millimetres(50), millimetres(60), millimetres(200), millimetres(90)).expect("a cutout"),
            ),
            CutoutDepth::Blind { depth: millimetres(8) },
            FaceSide::A,
        )
        .expect("a cutout on the shelf");
    project
        .set_panel_bevel(
            id(2),
            EdgeBevel::new(EdgeRef::Rect { edge: EdgeId::new(4).expect("edge") }, Degrees::from_millidegrees(45_000))
                .expect("a 45 degree end bevel"),
        )
        .expect("bevel on the shelf");
    // A person's catalogue choice is a property of its own kind and not a document reference: the
    // record lives in the directory and outlives the scene (ADR 0148, 0184).
    project
        .set_property(id(2), "plug.confirmat", model::PropertyValue::item(ItemId::new(712, 34).expect("a record")))
        .expect("the shelf names the plug a person chose");
    project.set_active_camera(Some(id(6))).expect("an active camera");
    // An object marked as a tool survives the boolean it took part in, and the flag travels in the
    // file: the golden carries one marked object so the field is exercised both ways.
    project.set_object_tool(id(10), true).expect("the union is a tool");
    // A boolean step freezes the cutter's shape and spends the source: the golden carries the step
    // and the source removed, so a file written by this build is a file the next build loads.
    let cutter = project.object(id(11)).expect("the cutter").geometry().as_constructive().expect("a body").clone();
    let step =
        model::BooleanTool::new("Cutter", model::ToolBody::Constructive(cutter), Placement::IDENTITY).expect("a tool");
    project
        .apply_boolean(id(3), model::Boolean::new(model::BooleanOp::Difference, step), Some(id(11)))
        .expect("the top is cut by the cutter");
    // A primitive is a solid body in both directions, so the golden carries both: the ball is cut by
    // a drawn block, and the top is cut by a ball (ADR 0197).
    let block_ring = Path::rectangle([millimetres(-40), millimetres(-40)], [millimetres(40), millimetres(40)])
        .expect("a block ring");
    let block = Constructive::extrude(block_ring, millimetres(40)).expect("a block");
    let block_cutter =
        model::BooleanTool::new("Block", model::ToolBody::Constructive(block), Placement::IDENTITY).expect("a block");
    project
        .apply_boolean(id(5), model::Boolean::new(model::BooleanOp::Difference, block_cutter), None)
        .expect("the ball is cut by a block");
    let ball_cutter = model::BooleanTool::new(
        "Ball cutter",
        model::ToolBody::Primitive(Primitive::sphere(millimetres(20), 24, 12).expect("a ball")),
        Placement::IDENTITY,
    )
    .expect("a ball");
    project
        .apply_boolean(id(3), model::Boolean::new(model::BooleanOp::Difference, ball_cutter), None)
        .expect("the top is cut by a ball");
    // A product reserves a footprint, not only three sides: one corner is cut by a chamfer of two
    // unequal setbacks, so the golden carries the corner rule as offsets and never as an angle
    // (ADR 0159).
    let footprint = Footprint::cut([
        None,
        Some(CornerFeatureKind::Chamfer { incoming: millimetres(120), outgoing: millimetres(70) }),
        None,
        None,
    ]);
    let envelope = Envelope::with_footprint(millimetres(700), millimetres(600), millimetres(900), footprint)
        .expect("a corner cabinet reserves a valid footprint");
    project
        .add_product(id(1), "Corner cabinet", ClassCode::GENERAL_PRODUCT, envelope, Placement::IDENTITY)
        .expect("a product at the root");

    let mut extra = BTreeMap::new();
    extra.insert("generator".to_owned(), Value::String("z3l golden".to_owned()));
    SceneFile::new(project).with_extra(extra)
}

#[test]
#[ignore = "writes the checked-in golden; run it deliberately when the schema changes"]
fn regenerate_the_golden_file() {
    let text = scene().to_json().expect("the scene serializes");
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/scene.z3l");
    fs::write(&path, text).expect("write the golden file");
}
