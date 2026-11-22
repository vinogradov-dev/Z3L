//! The read model: one question about the scene, and the answer a derivation already owns.
//!
//! A question is answered from the document and the catalogue and nothing else: no window, no second
//! definition of a number. Every answer reuses the derivation that already exists — `parts` is the
//! published `parts` table of the order package, `tree` reads the document's own node tree,
//! `box` and `bounds` are the container's authored envelope and [`Project::bounds_local`],
//! `diagnostics` is [`Session::panel_diagnostics`] and `niche` is [`Project::niche`] — so an answer
//! and the editor cannot disagree (`docs/domain/prototypes.md` §8).
//!
//! Nothing here writes: a question reads the session and leaves its revision, its history and its
//! dirty flag exactly where they were, which is what makes it safe to ask one in the middle of a
//! person's edit.
//!
//! The answer is one JSON value with the rows a caller reads. Lengths are whole micrometres, as the
//! document stores them, and a world coordinate is millimetres under a `_mm` key, because a
//! placement is a float at the geometry boundary. The report's envelope — which question, which file
//! and which revision it read — belongs to the door that prints it, because a path is the shell's
//! business and not the document's.
//!
//! Changes when: a question is added, or an answer's fields change.

use export::{DocumentMeta, ExportError, ExportOptions, OrderPackage, TableData};
use geom::DVec3;
use model::{Id, NicheRefusal, Node, NodeQuery, Project, Sight};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::diagnostics::PanelDiagnostic;
use crate::session::Session;

/// One question about the scene, with the arguments it takes.
///
/// The selector is typed, so a question cannot carry an argument it does not read: a point belongs
/// to `niche` alone, a node to the two that narrow one, and a resource to the questions that take
/// neither. The name of a variant is the stable id
/// of the question on every door — the command line prints it, and the JSON wire form tags the
/// object with it — so [`Query::NAMES`] is the one list a refusal and a help text read.
///
/// The JSON form is one object per question, `{"kind": "tree", "node": 5}`, which is the shape a
/// tool schema describes (ADR 0185, ADR 0195).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub enum Query {
    /// Every node with its id, kind, name and children, from `node` down.
    ///
    /// The walk stops at the document's own nesting limit, so a tree that somehow holds more levels
    /// than the loader allows is cut short instead of exhausting the stack.
    Tree {
        /// The node the answer starts at; absent starts at the document root.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        node: Option<Id>,
    },
    /// The box every product reserves, or the one `node` names.
    ///
    /// The box is authored data: its size is the product's own envelope (ADR 0097, ADR 0101) and
    /// its origin is where that envelope's minimum corner stands in the world.
    Box {
        /// The product the answer is about; absent answers for every product.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        node: Option<Id>,
    },
    /// The box a node occupies, in its own frame and in the world.
    ///
    /// This is the measured box, not the authored envelope `box` reports: a panel's is its nominal
    /// rectangle times its thickness, a body's is its own geometry's bounds, and a container's — a
    /// product and the root included — is the union of the parts below it through their placements.
    /// A camera, a light and a container with no geometry hold no box and are refused.
    Bounds {
        /// The node whose box is answered.
        node: Id,
    },
    /// The cut list, as the order package's `parts` table prints it.
    Parts,
    /// Every panel problem the catalogue resolves, with a stable code.
    Diagnostics,
    /// The empty space around a point, as the fill tool measures it.
    ///
    /// The point is world millimetres and comes from the caller, so a question about a node's own
    /// origin is the caller's `origin_world` and not a second argument here.
    Niche {
        /// The point the void is measured at, in world millimetres.
        at: [f64; 3],
    },
}

impl Query {
    /// Every question's name, in the order a refusal and a help text list them.
    pub const NAMES: [&'static str; 6] = ["tree", "box", "bounds", "parts", "diagnostics", "niche"];

    /// The question's own name, as the JSON tag and the command line spell it.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Tree { .. } => "tree",
            Self::Box { .. } => "box",
            Self::Bounds { .. } => "bounds",
            Self::Parts => "parts",
            Self::Diagnostics => "diagnostics",
            Self::Niche { .. } => "niche",
        }
    }
}

/// Why a question has no answer: every variant prints one sentence the caller can act on.
#[derive(Debug, thiserror::Error)]
pub enum QueryError {
    /// No node holds the id the question named.
    #[error("no node has id {0}")]
    NoNode(Id),
    /// The node is not a product, so it reserves no box.
    #[error("node {0} is not a product; `box` reports the box a product reserves")]
    NotAProduct(Id),
    /// The node holds no geometry, so it occupies no box.
    #[error(
        "node {0} holds no geometry, so it occupies no box; `bounds` measures a panel, a body, a \
         product or a container with a part in it"
    )]
    NoGeometry(Id),
    /// The point stands outside every product, or the free space around it is not one cell.
    #[error("{}", no_niche_sentence(*at, *refusal))]
    NoNiche {
        /// The point the question measured, in world millimetres.
        at: [f64; 3],
        /// Why the cell could not be measured.
        refusal: NicheRefusal,
    },
    /// The order package the cut list comes from cannot be built.
    #[error(transparent)]
    Export(#[from] ExportError),
    /// The package publishes no `parts` table, which is a build the table registry did not make.
    #[error("the order package publishes no `parts` table")]
    NoPartsTable,
}

/// Answers one question, reading the session and writing nothing.
pub(crate) fn answer(session: &Session, query: Query) -> Result<Value, QueryError> {
    let project = session.project();
    match query {
        Query::Tree { node } => tree(project, node),
        Query::Box { node } => box_report(project, node),
        Query::Bounds { node } => bounds_report(project, node),
        Query::Parts => parts(project, session),
        Query::Diagnostics => Ok(diagnostics(session)),
        Query::Niche { at } => niche(session, at),
    }
}

/// The node tree: the root with its subtree, or one node with its own.
fn tree(project: &Project, id: Option<Id>) -> Result<Value, QueryError> {
    let Some(id) = id else {
        let root = project.root();
        let mut value = json!({"id": root.id().get(), "kind": "container", "name": root.name()});
        let children: Vec<Value> = root.children().iter().map(|child| node_value(child, 0)).collect();
        insert(&mut value, "children", Value::Array(children));
        return Ok(value);
    };
    let node = project.node(id).ok_or(QueryError::NoNode(id))?;
    Ok(node_value(node, 0))
}

/// One node, its kind, its name and its subtree.
fn node_value(node: &Node, depth: u32) -> Value {
    let mut value = json!({"id": node.id().get(), "kind": kind(node), "name": node.name()});
    if depth < Project::MAX_ASSEMBLY_DEPTH
        && let Some(children) = node.children()
    {
        let children: Vec<Value> = children.iter().map(|child| node_value(child, depth.saturating_add(1))).collect();
        insert(&mut value, "children", Value::Array(children));
    }
    value
}

/// The node kind a report prints.
const fn kind(node: &Node) -> &'static str {
    match node {
        Node::Container(_) => "container",
        Node::Panel(_) => "panel",
        Node::Object(_) => "object",
        Node::Camera(_) => "camera",
        Node::Light(_) => "light",
    }
}

/// The box every product reserves, or the one `--id` names.
fn box_report(project: &Project, id: Option<Id>) -> Result<Value, QueryError> {
    let products: Vec<Id> = match id {
        Some(id) => {
            let named = project
                .container(id)
                .filter(|container| container.envelope().is_some())
                .ok_or(QueryError::NotAProduct(id))?;
            vec![named.id()]
        }
        None => project.products(),
    };
    let rows: Vec<Value> = products
        .into_iter()
        .filter_map(|product| {
            let container = project.container(product)?;
            let envelope = container.envelope()?;
            let origin =
                project.world_placement(product).map(|placement| placement.translation()).unwrap_or(DVec3::ZERO);
            Some(json!({
                "node_id": product.get(),
                "name": container.name(),
                "size_um": {
                    "x": envelope.x().micrometres(),
                    "y": envelope.y().micrometres(),
                    "z": envelope.z().micrometres(),
                },
                "origin_mm": [origin.x, origin.y, origin.z],
            }))
        })
        .collect();
    Ok(Value::Array(rows))
}

/// The box a node occupies, in its own frame and in the world.
///
/// Every key names the frame it is in: `local_*` is the node's own axes, where its box does not turn,
/// and `world_*` is the document's. The eight world corners are the local box through the node's
/// placement, so a turned node answers the box it really stands in, and `world_min_mm` and
/// `world_max_mm` are the tightest world box around them — the same box for a node that did not turn.
fn bounds_report(project: &Project, id: Id) -> Result<Value, QueryError> {
    let (kind, name) = if id == project.root().id() {
        ("container", project.root().name())
    } else {
        let node = project.node(id).ok_or(QueryError::NoNode(id))?;
        (kind(node), node.name())
    };
    let (min, max) = project.bounds_local(id).ok_or(QueryError::NoGeometry(id))?;
    let placement = project.world_placement(id).ok_or(QueryError::NoGeometry(id))?;
    let mut corners = Vec::with_capacity(8);
    let mut world_min = DVec3::splat(f64::INFINITY);
    let mut world_max = DVec3::splat(f64::NEG_INFINITY);
    for x in [min.x, max.x] {
        for y in [min.y, max.y] {
            for z in [min.z, max.z] {
                let corner = placement.transform_point(DVec3::new(x, y, z));
                world_min = world_min.min(corner);
                world_max = world_max.max(corner);
                corners.push(json!([corner.x, corner.y, corner.z]));
            }
        }
    }
    if !world_min.is_finite() || !world_max.is_finite() {
        return Err(QueryError::NoGeometry(id));
    }
    Ok(json!({
        "node_id": id.get(),
        "kind": kind,
        "name": name,
        "local_min_mm": [min.x, min.y, min.z],
        "local_max_mm": [max.x, max.y, max.z],
        "local_size_mm": [max.x - min.x, max.y - min.y, max.z - min.z],
        "world_min_mm": [world_min.x, world_min.y, world_min.z],
        "world_max_mm": [world_max.x, world_max.y, world_max.z],
        "world_corners_mm": corners,
    }))
}

/// The cut list, read from the one derivation the order package publishes.
fn parts(project: &Project, session: &Session) -> Result<Value, QueryError> {
    // The cut list asks for no price verdict and no timestamp: the age flag belongs to `export`, and
    // a package built for one table needs no clock of its own.
    let document = DocumentMeta { file: None, revision: project.revision().get(), hash: None };
    let options = ExportOptions { timestamp: None, document, order: None, price_max_age_days: None };
    let package = OrderPackage::build(project, session.directory(), options)?;
    let table = package.table("parts").ok_or(QueryError::NoPartsTable)?;
    Ok(rows_of(table))
}

/// Every panel's issues, with a stable code and the id the issue names.
fn diagnostics(session: &Session) -> Value {
    let project = session.project();
    let rows: Vec<Value> = project
        .query(&NodeQuery::any())
        .into_iter()
        .filter(|id| project.panel(*id).is_some())
        .map(|id| {
            let issues: Vec<Value> = session.panel_diagnostics(id).iter().map(issue).collect();
            let name = project.panel(id).map(|panel| panel.name().to_owned()).unwrap_or_default();
            json!({"node_id": id.get(), "name": name, "issues": issues})
        })
        .collect();
    Value::Array(rows)
}

/// One diagnostic as a code and, when it names something, that id.
fn issue(diagnostic: &PanelDiagnostic) -> Value {
    let mut value = json!({"code": diagnostic_code(diagnostic)});
    let named = match diagnostic {
        PanelDiagnostic::NoMaterial
        | PanelDiagnostic::NonPositiveCore
        | PanelDiagnostic::InvalidOutline
        | PanelDiagnostic::CoreRemoved
        | PanelDiagnostic::CoreDisconnected => None,
        PanelDiagnostic::MissingMaterial { material } => Some(material.to_string()),
        PanelDiagnostic::MissingBandingMaterial { edge, .. } => Some(edge.get().to_string()),
        PanelDiagnostic::MissingCornerBandingMaterial { corner, .. } => Some(corner.get().to_string()),
        PanelDiagnostic::MissingRingBandingMaterial { segment, .. } => Some(segment.get().to_string()),
        PanelDiagnostic::MachiningOutsideCore { machining } => Some(machining.to_string()),
        PanelDiagnostic::BlindDepthNotLessThanThickness { machining, depth, thickness } => {
            insert(&mut value, "depth_um", Value::from(depth.micrometres()));
            insert(&mut value, "thickness_um", Value::from(thickness.micrometres()));
            Some(machining.to_string())
        }
    };
    if let Some(named) = named {
        insert(&mut value, "ref", Value::from(named));
    }
    value
}

/// The stable code of one diagnostic.
///
/// The code is the answer's contract, so it is a fixed English word and not a translation; the
/// sentence a person reads stays in `ui::panel_card::diagnostic_message`. A new variant turns this
/// match into a compile error instead of printing a silent word.
const fn diagnostic_code(diagnostic: &PanelDiagnostic) -> &'static str {
    match diagnostic {
        PanelDiagnostic::NoMaterial => "no_material",
        PanelDiagnostic::MissingMaterial { .. } => "missing_material",
        PanelDiagnostic::MissingBandingMaterial { .. } => "missing_banding_material",
        PanelDiagnostic::MissingCornerBandingMaterial { .. } => "missing_corner_banding_material",
        PanelDiagnostic::MissingRingBandingMaterial { .. } => "missing_ring_banding_material",
        PanelDiagnostic::NonPositiveCore => "non_positive_core",
        PanelDiagnostic::InvalidOutline => "invalid_outline",
        PanelDiagnostic::MachiningOutsideCore { .. } => "machining_outside_core",
        PanelDiagnostic::CoreRemoved => "core_removed",
        PanelDiagnostic::CoreDisconnected => "core_disconnected",
        PanelDiagnostic::BlindDepthNotLessThanThickness { .. } => "blind_depth_not_less_than_thickness",
    }
}

/// The empty space around a point, as the fill tool measures it.
///
/// The sight passes everything — a question has no window and therefore no hidden set — and the
/// contact adapter is the session's own, so a splayed post or a plate with a hole bounds the cell
/// here exactly as it does in the window (ADR 0183, X92).
fn niche(session: &Session, at: [f64; 3]) -> Result<Value, QueryError> {
    let point = DVec3::new(at[0], at[1], at[2]);
    let no_niche = |refusal| QueryError::NoNiche { at, refusal };
    let cell = session.project().niche(point, Sight::none(), Some(session.contact())).map_err(no_niche)?;
    let planes: Vec<Value> = cell
        .planes
        .iter()
        .map(|plane| json!({"normal": [plane.normal.x, plane.normal.y, plane.normal.z], "offset_mm": plane.offset}))
        .collect();
    let corners: Vec<Value> = cell.corners.iter().map(|corner| json!([corner.x, corner.y, corner.z])).collect();
    Ok(json!({
        "product_id": cell.product.get(),
        "shape": shape_word(&cell.shape),
        "local_min_mm": [cell.local_min.x, cell.local_min.y, cell.local_min.z],
        "size_mm": [cell.size.x, cell.size.y, cell.size.z],
        "min_mm": [cell.min.x, cell.min.y, cell.min.z],
        "planes": planes,
        "corners_mm": corners,
    }))
}

/// The sentence a point with no niche is refused by.
///
/// One wording serves the query and `panel.fill`, so a caller reads the same sentence whichever door
/// measured the void (N37).
pub(crate) fn no_niche_sentence(at: [f64; 3], refusal: NicheRefusal) -> String {
    format!("no niche at [{}, {}, {}]: {}", at[0], at[1], at[2], refusal_word(refusal))
}

/// Why a point has no niche, in the answer's words.
pub(crate) const fn refusal_word(refusal: NicheRefusal) -> &'static str {
    match refusal {
        NicheRefusal::Outside => "the point is outside every product",
        NicheRefusal::Unrepresentable => "the free space is not one convex cell",
        NicheRefusal::Footprint => "the frame's footprint is not one convex ring",
    }
}

/// The niche's shape as the answer names it.
const fn shape_word(shape: &model::NicheShape) -> &'static str {
    match shape {
        model::NicheShape::Box => "box",
        model::NicheShape::Section(_) => "section",
        model::NicheShape::Sloped => "sloped",
    }
}

/// One table as an array of records, its columns naming each field.
fn rows_of(table: &TableData) -> Value {
    let columns = table.spec().columns;
    Value::Array(
        table
            .rows()
            .iter()
            .map(|row| {
                let mut record = Map::new();
                for (column, cell) in columns.iter().zip(row) {
                    record.insert(column.name.to_owned(), cell.json());
                }
                Value::Object(record)
            })
            .collect(),
    )
}

/// Adds one field to a report object, leaving a value that is not an object alone.
fn insert(value: &mut Value, key: &str, field: Value) {
    if let Some(object) = value.as_object_mut() {
        object.insert(key.to_owned(), field);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use std::sync::Arc;

    use geom::DQuat;
    use model::{ClassCode, Envelope, LightKind, Placement};
    use testkit::{millimetres, sample_catalog, sample_project};

    use super::*;

    /// The fixture every answer is measured on: one product with a light inside it, a panel beside
    /// it, and the sample's own shelf and drawer.
    fn session() -> Session {
        let mut project = sample_project();
        let root = project.root().id();
        let envelope = Envelope::new(millimetres(600), millimetres(560), millimetres(720)).expect("a valid box");
        let product = project
            .add_product(root, "Cabinet", ClassCode::GENERAL_PRODUCT, envelope, Placement::IDENTITY)
            .expect("a product beside the sample nodes");
        let size = model::PanelNominalSize::new(millimetres(700), millimetres(500)).expect("a valid size");
        project.add_panel(root, "Side", size, None, millimetres(18), Placement::IDENTITY).expect("the side is added");
        let centre = Placement::new(DVec3::new(300.0, 280.0, 360.0), DQuat::IDENTITY).expect("a unit placement");
        project.add_light(product, "Key", centre, LightKind::Sun, [1.0; 3], 1.0).expect("the light is added");
        // The real contact adapter, so the niche of a splayed body is measured as the window does.
        Session::with_directory(project, sample_catalog()).with_contact(Arc::new(contact_parry::Parry))
    }

    /// The fixture with a 200 x 300 x 600 body inside the product, and the panel, the product and
    /// the body's ids.
    fn session_with_a_body(placement: Placement) -> (Session, Id, Id, Id) {
        let mut project = sample_project();
        let root = project.root().id();
        let envelope = Envelope::new(millimetres(600), millimetres(560), millimetres(720)).expect("a valid box");
        let product = project
            .add_product(root, "Cabinet", ClassCode::GENERAL_PRODUCT, envelope, Placement::IDENTITY)
            .expect("a product beside the sample nodes");
        let size = model::PanelNominalSize::new(millimetres(700), millimetres(500)).expect("a valid size");
        let panel = project
            .add_panel(root, "Side", size, None, millimetres(18), Placement::IDENTITY)
            .expect("the side is added");
        let base = model::Path::rectangle([millimetres(0), millimetres(0)], [millimetres(200), millimetres(300)])
            .expect("a rectangle");
        let body = model::Constructive::extrude(base, millimetres(600)).expect("an extrusion");
        let body = project
            .add_object(product, "Body", placement, model::Geometry::Constructive(body))
            .expect("a body inside the product");
        (Session::with_directory(project, sample_catalog()), panel, product, body)
    }

    /// Asserts one point of an answer, to the tolerance a quarter turn's cosine leaves behind.
    fn assert_point(value: &Value, expected: [f64; 3]) {
        let point = value.as_array().expect("a point is a list of numbers");
        assert_eq!(point.len(), 3, "a point is three numbers: {value}");
        for (index, expected) in expected.into_iter().enumerate() {
            let got = point.get(index).and_then(Value::as_f64).expect("a coordinate");
            assert!((got - expected).abs() < 1e-9, "coordinate {index} is {got}, not {expected}: {value}");
        }
    }

    #[test]
    fn a_bounds_answer_measures_a_body_a_panel_and_a_product() {
        let (session, panel, product, body) = session_with_a_body(Placement::IDENTITY);

        let answer = session.read(Query::Bounds { node: panel }).expect("the panel's box");
        assert_eq!(answer["kind"], "panel");
        assert_eq!(answer["name"], "Side");
        assert_eq!(answer["local_min_mm"], json!([0.0, 0.0, 0.0]));
        assert_eq!(answer["local_max_mm"], json!([700.0, 500.0, 18.0]));
        assert_eq!(answer["local_size_mm"], json!([700.0, 500.0, 18.0]));
        assert_eq!(answer["world_min_mm"], json!([0.0, 0.0, 0.0]), "an unturned part stands where its box says");

        let answer = session.read(Query::Bounds { node: body }).expect("the body's box");
        assert_eq!(answer["kind"], "object");
        assert_eq!(answer["name"], "Body");
        assert_eq!(answer["local_size_mm"], json!([200.0, 300.0, 600.0]), "a body answers its own geometry");

        // The measured box of a product is what it holds; the box it reserves is the envelope `box`
        // answers, and the two are different questions about one node.
        let answer = session.read(Query::Bounds { node: product }).expect("the product's box");
        assert_eq!(answer["kind"], "container");
        assert_eq!(answer["local_size_mm"], json!([200.0, 300.0, 600.0]), "the union of the parts below it");
        let reserved = session.read(Query::Box { node: Some(product) }).expect("the reserved box");
        assert_eq!(reserved[0]["size_um"]["x"], 600_000, "`box` still answers the authored envelope");
    }

    #[test]
    fn a_turned_node_answers_its_own_box_and_the_world_box_it_stands_in() {
        // A quarter turn about Z maps the body's own +X onto world +Y, so its 200-long side now runs
        // along Y and its near corner is no longer the minimum of the world box.
        let turned = DQuat::from_rotation_z(std::f64::consts::FRAC_PI_2);
        let placement = Placement::new(DVec3::new(100.0, 0.0, 0.0), turned).expect("a unit placement");
        let (session, _panel, _product, body) = session_with_a_body(placement);
        let answer = session.read(Query::Bounds { node: body }).expect("the turned body's box");
        assert_eq!(answer["local_min_mm"], json!([0.0, 0.0, 0.0]));
        assert_eq!(answer["local_max_mm"], json!([200.0, 300.0, 600.0]), "the own frame never turns with it");
        assert_point(&answer["world_min_mm"], [-200.0, 0.0, 0.0]);
        assert_point(&answer["world_max_mm"], [100.0, 200.0, 600.0]);
        let corners = answer["world_corners_mm"].as_array().expect("eight corners");
        assert_eq!(corners.len(), 8, "a box has eight corners");
        assert_point(&corners[0], [100.0, 0.0, 0.0]);
        assert_point(&corners[6], [-200.0, 200.0, 0.0]);
    }

    #[test]
    fn a_node_with_no_geometry_is_a_refusal_the_reader_can_act_on() {
        let session = session();
        let light = Id::new(7).expect("an id");
        let error = session.read(Query::Bounds { node: light }).expect_err("a light holds no geometry");
        let message = error.to_string();
        assert!(message.contains("node 7"), "the sentence names the node: {message}");
        assert!(message.contains("holds no geometry"), "{message}");
        assert!(message.contains("`bounds` measures"), "and says what `bounds` does read: {message}");

        // The fixture's product holds a light and nothing else.
        let product = Id::new(5).expect("an id");
        let error = session.read(Query::Bounds { node: product }).expect_err("the frame holds no geometry");
        assert!(error.to_string().contains("node 5"), "{error}");

        let missing = Id::new(9_999).expect("an id");
        let error = session.read(Query::Bounds { node: missing }).expect_err("no such node");
        assert!(error.to_string().contains("9999"), "{error}");
    }

    #[test]
    fn the_tree_answer_names_every_node_the_fixture_holds() {
        let session = session();
        let answer = session.read(Query::Tree { node: None }).expect("the tree is answered");
        assert_eq!(answer["id"], 1, "the answer starts at the document root");
        let children = answer["children"].as_array().expect("the root has children");
        let ids: Vec<u64> = children.iter().filter_map(|child| child["id"].as_u64()).collect();
        assert_eq!(ids, vec![2, 3, 5, 6], "the sample's shelf and drawer, the product and the panel: {ids:?}");
        let product = children.iter().find(|child| child["id"] == 5).expect("the product is listed");
        assert_eq!(product["kind"], "container");
        assert_eq!(product["name"], "Cabinet");
        let inside = product["children"].as_array().expect("the product holds the light");
        assert_eq!(inside.len(), 1);
        assert_eq!(inside[0]["kind"], "light");
    }

    #[test]
    fn a_tree_of_one_node_holds_that_subtree_only() {
        let session = session();
        let answer = session.read(Query::Tree { node: Id::new(5) }).expect("the subtree is answered");
        assert_eq!(answer["name"], "Cabinet");
        assert_eq!(answer["children"].as_array().map(Vec::len), Some(1));

        let missing = Id::new(9_999).expect("an id");
        let error = session.read(Query::Tree { node: Some(missing) }).expect_err("the id is not in the document");
        assert!(error.to_string().contains("9999"), "{error}");
    }

    #[test]
    fn the_box_answer_prints_the_reserved_box_of_a_product() {
        let session = session();
        let answer = session.read(Query::Box { node: None }).expect("the boxes are answered");
        let rows = answer.as_array().expect("one row per product");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["node_id"], 5);
        assert_eq!(rows[0]["size_um"]["x"], 600_000);
        assert_eq!(rows[0]["size_um"]["y"], 560_000);
        assert_eq!(rows[0]["size_um"]["z"], 720_000);
        assert_eq!(rows[0]["origin_mm"], json!([0.0, 0.0, 0.0]));

        let panel = Id::new(2).expect("an id");
        let error = session.read(Query::Box { node: Some(panel) }).expect_err("a panel reserves no box");
        assert!(error.to_string().contains("is not a product"), "{error}");
    }

    #[test]
    fn the_parts_answer_is_the_published_table() {
        let session = session();
        let answer = session.read(Query::Parts).expect("the cut list is answered");
        let rows = answer.as_array().expect("one row per part");
        let ids: Vec<u64> = rows.iter().filter_map(|row| row["node_id"].as_u64()).collect();
        assert_eq!(ids, vec![2, 4, 6], "the sample's shelf and front and the panel the fixture added: {ids:?}");
        assert!(rows.iter().all(|row| row.get("length_um").is_some() && row.get("product_id").is_some()));
    }

    #[test]
    fn the_diagnostics_answer_names_a_panel_without_material() {
        let session = session();
        let answer = session.read(Query::Diagnostics).expect("the issues are answered");
        let rows = answer.as_array().expect("one row per panel");
        assert_eq!(rows.len(), 3, "one row per panel in the fixture");
        let issues: Vec<&Value> = rows.iter().flat_map(|row| row["issues"].as_array().into_iter().flatten()).collect();
        assert!(issues.iter().any(|issue| issue["code"] == "no_material"), "a panel with no material is reported");
        assert!(issues.iter().all(|issue| issue["code"].as_str().is_some()), "every issue carries a code");
    }

    #[test]
    fn a_niche_is_measured_at_the_point_it_names() {
        let session = session();
        let answer = session.read(Query::Niche { at: [300.0, 280.0, 360.0] }).expect("the void is answered");
        assert_eq!(answer["product_id"], 5);
        assert_eq!(answer["shape"], "box", "an empty product reserves a box");
        // The sample's own shelf stands under the void, so the free space starts at its top face.
        assert_eq!(answer["size_mm"], json!([600.0, 560.0, 702.0]));
        assert!(
            answer["planes"].as_array().is_some_and(|planes| planes.len() >= 6),
            "the envelope's six planes bound the void: {answer}"
        );
    }

    #[test]
    fn a_niche_outside_every_product_is_a_sentence() {
        let session = session();
        let error = session.read(Query::Niche { at: [5_000.0; 3] }).expect_err("the point is outside");
        let message = error.to_string();
        assert!(message.contains("5000"), "the sentence names the point: {message}");
        assert!(message.contains("outside every product"), "{message}");
    }

    #[test]
    fn every_question_has_one_name_and_the_wire_form_speaks_it() {
        // The name is the id of a question on every door, so the list a refusal prints and the tag
        // the JSON form carries come from the same six variants.
        let questions = [
            Query::Tree { node: None },
            Query::Box { node: None },
            Query::Bounds { node: Id::new(2).expect("an id") },
            Query::Parts,
            Query::Diagnostics,
            Query::Niche { at: [0.0; 3] },
        ];
        let names: Vec<&str> = questions.iter().map(|query| query.name()).collect();
        assert_eq!(names, Query::NAMES, "the list is the enum's own names, in the same order");

        let node = Id::new(5).expect("an id");
        let wire = json!({"kind": "tree", "node": node});
        assert_eq!(
            serde_json::from_value::<Query>(wire.clone()).expect("the wire form parses"),
            Query::Tree { node: Some(node) }
        );
        assert_eq!(serde_json::to_value(Query::Tree { node: Some(node) }).expect("it writes back"), wire);
        assert_eq!(
            serde_json::from_value::<Query>(json!({"kind": "niche", "at": [1.0, 2.0, 3.0]}))
                .expect("a point is a niche"),
            Query::Niche { at: [1.0, 2.0, 3.0] }
        );

        let error = serde_json::from_value::<Query>(json!({"kind": "boxes"})).expect_err("there is no such question");
        let message = error.to_string();
        assert!(message.contains("boxes"), "the refusal quotes the name: {message}");
        assert!(message.contains("tree"), "and lists the six: {message}");

        // The schema a tool publishes names the same five kinds, so a model reads the selector the
        // parser enforces (ADR 0185, ADR 0195).
        let schema = serde_json::to_string(&schemars::schema_for!(Query)).expect("the schema is JSON");
        for name in Query::NAMES {
            assert!(schema.contains(&format!("\"{name}\"")), "the schema names `{name}`: {schema}");
        }
    }

    #[test]
    fn a_question_leaves_the_revision_and_the_history_untouched() {
        // The one rule of the read model: it reads. A question asked in the middle of a person's
        // edit must not add an undo entry, move the revision or mark the document dirty.
        let mut session = session();
        let product = Id::new(5).expect("an id");
        let rename = json!({"node": product, "name": "Case"});
        session.dispatch(crate::node_cmds::NODE_RENAME_ID, &rename).expect("the fixture is edited once");
        let before = session.project().revision();
        assert!(session.can_undo(), "the edit is one entry below the questions");

        for query in [
            Query::Tree { node: None },
            Query::Box { node: None },
            Query::Bounds { node: Id::new(6).expect("an id") },
            Query::Parts,
            Query::Diagnostics,
            Query::Niche { at: [300.0, 280.0, 360.0] },
        ] {
            session.read(query).expect("every question is answered");
            assert_eq!(session.project().revision(), before, "`{}` moves no revision", query.name());
            assert!(session.can_undo(), "`{}` adds no undo entry", query.name());
            assert!(!session.can_redo(), "`{}` clears no history", query.name());
            assert!(session.is_dirty(), "`{}` writes no revision", query.name());
        }
        session.undo().expect("the one edit is still the one entry");
        assert_eq!(
            session.project().container(product).map(model::Container::name),
            Some("Cabinet"),
            "undo restores the name, so no question touched the history"
        );
    }
}
