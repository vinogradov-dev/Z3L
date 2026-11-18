//! The one scene the exporter tests start from.
//!
//! It exercises every table: two products, a part in each, banding, a cutout, a contour panel, a
//! facade class, a fitting, a classed group, and a user property. The ids stay the fixture ids so a
//! golden package reads like a document rather than like a snapshot of an allocator.

#![allow(clippy::expect_used)]

use model::{
    ClassCode, Contour, ContourSegment, Cutout, CutoutDepth, CutoutShape, EdgeBanding, EdgeId, Envelope, FaceSide, Id,
    PanelNominalSize, PanelRect, Placement, Project, PropertyValue, SYSTEM_CLASS,
};
use testkit::{millimetres, sample_catalog, sample_product_project};
use units::Degrees;

/// The scene the exporter tests and the golden package are built from.
pub fn fixture() -> (Project, model::Catalog) {
    let (mut project, product) = sample_product_project();
    let catalog = sample_catalog();
    add_parts(&mut project, product);
    add_second_product(&mut project);
    (project, catalog)
}

fn add_parts(project: &mut Project, product: Id) {
    let size = PanelNominalSize::new(millimetres(700), millimetres(500)).expect("a valid size");
    let side = project
        .add_panel(product, "Side", size, Some(testkit::sample_material_id()), millimetres(18), Placement::IDENTITY)
        .expect("a side is added");
    project
        .set_property(side, SYSTEM_CLASS, PropertyValue::class(ClassCode::VERTICAL_PANEL))
        .expect("a vertical panel");
    let band = EdgeBanding::new(
        EdgeId::new(1).expect("edge 1"),
        testkit::sample_edge_material_id(),
        true,
        millimetres(2),
        millimetres(22),
    )
    .expect("a band");
    project.set_edge_banding(side, band).expect("banding on edge 1");
    let cutout = Cutout::new(
        CutoutShape::Rectangle(
            PanelRect::new(millimetres(50), millimetres(50), millimetres(100), millimetres(60)).expect("a rect"),
        ),
        CutoutDepth::Through,
    );
    project.add_cutout(side, cutout.shape().clone(), cutout.depth(), FaceSide::A).expect("a cutout");

    let shelf = project
        .add_panel(
            product,
            "Shelf",
            PanelNominalSize::new(millimetres(650), millimetres(450)).expect("a valid size"),
            Some(testkit::sample_material_id()),
            millimetres(18),
            Placement::IDENTITY,
        )
        .expect("a shelf is added");
    project
        .set_property(shelf, SYSTEM_CLASS, PropertyValue::class(ClassCode::HORIZONTAL_PANEL))
        .expect("a shelf class");

    let door = project
        .add_panel(
            product,
            "Door",
            PanelNominalSize::new(millimetres(600), millimetres(700)).expect("a valid size"),
            Some(testkit::sample_material_id()),
            millimetres(18),
            Placement::IDENTITY,
        )
        .expect("a door is added");
    project
        .set_property(door, SYSTEM_CLASS, PropertyValue::class(ClassCode::from_code(102_000).expect("a hinged door")))
        .expect("a facade class");

    let fitting = project.add_component(product, "Hinge", Placement::IDENTITY).expect("a fitting container");
    project
        .set_property(fitting, SYSTEM_CLASS, PropertyValue::class(ClassCode::from_code(500_201).expect("a cup hinge")))
        .expect("a fitting class");

    project.set_property(product, "customer", PropertyValue::text("Workshop")).expect("a user property");
}

fn add_second_product(project: &mut Project) {
    let root = project.root().id();
    let product = project
        .add_product(
            root,
            "Wardrobe",
            ClassCode::GENERAL_PRODUCT,
            Envelope::new(millimetres(900), millimetres(600), millimetres(2100)).expect("a valid box"),
            Placement::IDENTITY,
        )
        .expect("a second product");
    let back = project
        .add_panel(
            product,
            "Back",
            PanelNominalSize::new(millimetres(880), millimetres(2050)).expect("a valid size"),
            Some(testkit::sample_material_id()),
            millimetres(18),
            Placement::IDENTITY,
        )
        .expect("a back panel");
    project
        .set_property(back, SYSTEM_CLASS, PropertyValue::class(ClassCode::from_code(101_200).expect("a back panel")))
        .expect("a back class");

    // A round shelf draws arcs, so the contour table carries a curve and not only lines.
    let radius = millimetres(400);
    let center = [radius, radius];
    let arcs: Vec<ContourSegment> = [0, 90, 180, 270]
        .into_iter()
        .map(|start| {
            ContourSegment::arc(
                center,
                radius,
                Degrees::from_millidegrees(start * 1_000),
                Degrees::from_millidegrees(90_000),
            )
            .expect("a quarter arc")
        })
        .collect();
    let ring = Contour::new(arcs).expect("a closed circle ring");
    let round = project
        .add_panel_contour(
            product,
            "Round shelf",
            &ring,
            Some(testkit::sample_material_id()),
            millimetres(18),
            Placement::IDENTITY,
        )
        .expect("a contour panel");
    project
        .set_property(round, SYSTEM_CLASS, PropertyValue::class(ClassCode::HORIZONTAL_PANEL))
        .expect("a shelf class");
}
