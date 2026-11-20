//! The read model: one JSON report about a saved scene.
//!
//! A question is answered from the document and the catalogue and nothing else: no session state, no
//! window, no second definition of a number. Three of the five reports reuse a derivation that
//! already exists — `parts` is the published `parts` table of the order package, `tree` is the
//! document's own node walk and `diagnostics` is [`engine::Session::panel_diagnostics`] — so a
//! report and the editor cannot disagree (`docs/domain/prototypes.md` §8).
//!
//! Every report is one JSON object with the query name, the document it read and its rows, because a
//! caller that stores an answer must be able to say which revision it is about. Lengths are whole
//! micrometres, as the document stores them, and a world coordinate is millimetres with a `_mm` key,
//! because a placement is a float at the geometry boundary.
//!
//! Changes when: a report kind is added, or a report's fields change.

use engine::{PanelDiagnostic, Session};
use export::{DocumentMeta, ExportOptions, OrderPackage, TableData};
use geom::DVec3;
use model::{Id, Node, NodeQuery, Project, Sight};
use project::SceneFile;
use serde_json::{Map, Value, json};

use super::{CliError, QueryArgs};

/// The five reports a query can ask for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryKind {
    /// Every node with its parent, class, level, path and placement.
    Tree,
    /// The box a product reserves, in its own frame and in world millimetres.
    Box,
    /// The cut list, as the order package's `parts` table prints it.
    Parts,
    /// Every panel problem the catalogue resolves.
    Diagnostics,
    /// The empty space around a point.
    Niche,
}

impl QueryKind {
    /// The words a line or a refusal may name, in the order the help lists them.
    pub const WORDS: &'static str = "tree, box, parts, diagnostics or niche";

    /// Reads one report name.
    pub fn parse(word: &str) -> Result<Self, CliError> {
        match word {
            "tree" => Ok(Self::Tree),
            "box" => Ok(Self::Box),
            "parts" => Ok(Self::Parts),
            "diagnostics" => Ok(Self::Diagnostics),
            "niche" => Ok(Self::Niche),
            other => Err(CliError::Usage(format!("`{other}` is not a report; name {}", Self::WORDS))),
        }
    }

    /// The report's own name, as the JSON envelope repeats it.
    pub const fn word(self) -> &'static str {
        match self {
            Self::Tree => "tree",
            Self::Box => "box",
            Self::Parts => "parts",
            Self::Diagnostics => "diagnostics",
            Self::Niche => "niche",
        }
    }
}

/// Reads a scene and prints the report the line asked for.
///
/// Nothing here writes: the session is built, read and dropped, so a query on a saved file is safe
/// to run while an editor holds the same scene open.
pub fn report(args: &QueryArgs) -> Result<String, CliError> {
    let scene = SceneFile::read(&args.scene).map_err(CliError::Scene)?;
    let file = args.scene.file_name().map(|name| name.to_string_lossy().into_owned());
    let session = super::recipe::session_of(scene.into_document())?;
    let project = session.project();
    let rows = match args.kind {
        QueryKind::Tree => tree(project, args.id)?,
        QueryKind::Box => box_report(project, args.id)?,
        QueryKind::Parts => parts(project)?,
        QueryKind::Diagnostics => diagnostics(&session),
        QueryKind::Niche => niche(project, args)?,
    };
    let envelope = json!({
        "query": args.kind.word(),
        "document": {"file": file, "revision": project.revision().get()},
        "rows": rows,
    });
    serde_json::to_string_pretty(&envelope).map_err(|error| CliError::Query(error.to_string()))
}

/// The node tree: the root with its subtree, or one node with its own.
fn tree(project: &Project, id: Option<Id>) -> Result<Value, CliError> {
    let Some(id) = id else {
        let root = project.root();
        let mut value = json!({"id": root.id().get(), "kind": "container", "name": root.name()});
        let children: Vec<Value> = root.children().iter().map(|child| node_value(child, 0)).collect();
        insert(&mut value, "children", Value::Array(children));
        return Ok(value);
    };
    let node = project.node(id).ok_or_else(|| CliError::Query(no_node(id)))?;
    Ok(node_value(node, 0))
}

/// One node, its kind, its name and its subtree.
///
/// The walk stops at the document's own nesting limit, so a tree that somehow holds more levels than
/// the loader allows is cut short instead of exhausting the stack.
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
///
/// The box is authored data: `wide`, `deep` and `high` are the product's own envelope (ADR 0097,
/// ADR 0101) and the origin is where that envelope's minimum corner stands in the world.
fn box_report(project: &Project, id: Option<Id>) -> Result<Value, CliError> {
    let products: Vec<Id> = match id {
        Some(id) => {
            let named = project.container(id).filter(|container| container.envelope().is_some()).ok_or_else(|| {
                CliError::Query(format!("node {id} is not a product; `box` reports the box a product reserves"))
            })?;
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

/// The cut list, read from the one derivation the order package publishes.
fn parts(project: &Project) -> Result<Value, CliError> {
    let catalog = crate::scene_build::default_catalog().map_err(CliError::Catalog)?;
    let document = DocumentMeta { file: None, revision: project.revision().get(), hash: None };
    // `query parts` asks for the cut list, not for a price verdict: the age flag belongs to `export`.
    let options = ExportOptions { timestamp: None, document, order: None, price_max_age_days: None };
    let package = OrderPackage::build(project, &catalog, options).map_err(CliError::Export)?;
    let table = package
        .table("parts")
        .ok_or_else(|| CliError::Query("the order package publishes no `parts` table".to_owned()))?;
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
/// The code is the report's contract, so it is a fixed English word and not a translation; the
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
/// `--at` names the point in world millimetres; `--id` asks for the space at that node's origin, so
/// a caller that already holds a node id needs no second measurement. The sight passes everything —
/// a query has no session and therefore no hidden set — and the contact adapter is the shell's own,
/// so a splayed post or a plate with a hole bounds the cell here exactly as it does in the window.
fn niche(project: &Project, args: &QueryArgs) -> Result<Value, CliError> {
    let point = match (args.at, args.id) {
        (Some(at), _) => DVec3::new(at[0], at[1], at[2]),
        (None, Some(id)) => project.origin_world(id).ok_or_else(|| CliError::Query(no_node(id)))?,
        (None, None) => {
            return Err(CliError::Usage("`niche` needs a point: `--at x,y,z` or `--id <node>`".to_owned()));
        }
    };
    let contact = crate::scene_build::contact_engine();
    let niche = project.niche(point, Sight::none(), Some(&*contact)).map_err(|refusal| {
        CliError::Query(format!("no niche at [{}, {}, {}]: {}", point.x, point.y, point.z, refusal_word(refusal)))
    })?;
    let planes: Vec<Value> = niche
        .planes
        .iter()
        .map(|plane| json!({"normal": [plane.normal.x, plane.normal.y, plane.normal.z], "offset_mm": plane.offset}))
        .collect();
    let corners: Vec<Value> = niche.corners.iter().map(|corner| json!([corner.x, corner.y, corner.z])).collect();
    Ok(json!({
        "product_id": niche.product.get(),
        "shape": shape_word(&niche.shape),
        "local_min_mm": [niche.local_min.x, niche.local_min.y, niche.local_min.z],
        "size_mm": [niche.size.x, niche.size.y, niche.size.z],
        "min_mm": [niche.min.x, niche.min.y, niche.min.z],
        "planes": planes,
        "corners_mm": corners,
    }))
}

/// The niche's shape as the report names it.
const fn shape_word(shape: &model::NicheShape) -> &'static str {
    match shape {
        model::NicheShape::Box => "box",
        model::NicheShape::Section(_) => "section",
        model::NicheShape::Sloped => "sloped",
    }
}

/// Why a point has no niche, in the report's words.
const fn refusal_word(refusal: model::NicheRefusal) -> &'static str {
    match refusal {
        model::NicheRefusal::Outside => "the point is outside every product",
        model::NicheRefusal::Unrepresentable => "the free space is not one convex cell",
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

/// The sentence a report gives for an id the document does not hold.
fn no_node(id: Id) -> String {
    format!("no node has id {id}")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use geom::DQuat;
    use model::{ClassCode, Envelope, LightKind, Placement};
    use testkit::{millimetres, sample_project, scratch_dir};

    use super::*;

    use std::path::Path;

    /// A saved scene with one product, one panel beside it and a light inside it, and its path.
    ///
    /// The product holds one light and no body, so a niche measured inside it is the envelope's own
    /// box: a test that asks for free space should not have to guess how a part cuts it.
    fn saved(directory: &Path) -> std::path::PathBuf {
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
        let path = directory.join("cabinet.z3l");
        SceneFile::new(project).write(&path).expect("the fixture scene is saved");
        path
    }

    /// The words a test passes, without the program name.
    fn words(line: &[&str]) -> Vec<String> {
        line.iter().map(|word| (*word).to_owned()).collect()
    }

    /// The query a line names, already parsed.
    fn args_of(line: &[&str]) -> QueryArgs {
        let command = crate::cli::parse(&words(line)).expect("the line parses");
        match command {
            crate::cli::Command::Query(args) => args,
            other => panic!("the line names a query, not {other:?}"),
        }
    }

    /// The report a line asks for, parsed back into JSON.
    fn report_of(line: &[&str]) -> Value {
        let text = report(&args_of(line)).expect("the report is written");
        serde_json::from_str(&text).expect("the report is JSON")
    }

    #[test]
    fn the_tree_report_names_every_node_the_fixture_holds() {
        let directory = scratch_dir("cli-query-tree");
        let path = saved(&directory);
        let answer = report_of(&["query", &path.display().to_string(), "tree"]);
        assert_eq!(answer["query"], "tree");
        assert_eq!(answer["document"]["file"], "cabinet.z3l");
        let root = &answer["rows"];
        assert_eq!(root["id"], 1, "the report starts at the document root");
        let children = root["children"].as_array().expect("the root has children");
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
        let directory = scratch_dir("cli-query-subtree");
        let path = saved(&directory);
        let answer = report_of(&["query", &path.display().to_string(), "tree", "--id", "5"]);
        assert_eq!(answer["rows"]["name"], "Cabinet");
        assert_eq!(answer["rows"]["children"].as_array().map(Vec::len), Some(1));

        let args = args_of(&["query", &path.display().to_string(), "tree", "--id", "9999"]);
        let message = report(&args).expect_err("the id is not in the document").to_string();
        assert!(message.contains("9999"), "{message}");
    }

    #[test]
    fn the_box_report_prints_the_reserved_box_of_a_product() {
        let directory = scratch_dir("cli-query-box");
        let path = saved(&directory);
        let answer = report_of(&["query", &path.display().to_string(), "box"]);
        let rows = answer["rows"].as_array().expect("one row per product");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["node_id"], 5);
        assert_eq!(rows[0]["size_um"]["x"], 600_000);
        assert_eq!(rows[0]["size_um"]["y"], 560_000);
        assert_eq!(rows[0]["size_um"]["z"], 720_000);
        assert_eq!(rows[0]["origin_mm"], json!([0.0, 0.0, 0.0]));

        let args = args_of(&["query", &path.display().to_string(), "box", "--id", "2"]);
        let message = report(&args).expect_err("a panel reserves no box").to_string();
        assert!(message.contains("is not a product"), "{message}");
    }

    #[test]
    fn the_parts_report_is_the_published_table() {
        let directory = scratch_dir("cli-query-parts");
        let path = saved(&directory);
        let answer = report_of(&["query", &path.display().to_string(), "parts"]);
        let rows = answer["rows"].as_array().expect("one row per part");
        let ids: Vec<u64> = rows.iter().filter_map(|row| row["node_id"].as_u64()).collect();
        assert_eq!(ids, vec![2, 4, 6], "the sample's shelf and front and the panel the fixture added: {ids:?}");
        assert!(rows.iter().all(|row| row.get("length_um").is_some() && row.get("product_id").is_some()));
    }

    #[test]
    fn the_diagnostics_report_names_a_panel_without_material() {
        let directory = scratch_dir("cli-query-diagnostics");
        let path = saved(&directory);
        let answer = report_of(&["query", &path.display().to_string(), "diagnostics"]);
        let rows = answer["rows"].as_array().expect("one row per panel");
        assert_eq!(rows.len(), 3, "one row per panel in the fixture");
        let issues: Vec<&Value> = rows.iter().flat_map(|row| row["issues"].as_array().into_iter().flatten()).collect();
        assert!(issues.iter().any(|issue| issue["code"] == "no_material"), "a panel with no material is reported");
        assert!(issues.iter().all(|issue| issue["code"].as_str().is_some()), "every issue carries a code");
    }

    #[test]
    fn a_niche_is_measured_at_a_point_or_at_a_node() {
        let directory = scratch_dir("cli-query-niche");
        let path = saved(&directory);
        let answer = report_of(&["query", &path.display().to_string(), "niche", "--at", "300,280,360"]);
        assert_eq!(answer["query"], "niche");
        let rows = &answer["rows"];
        assert_eq!(rows["product_id"], 5);
        assert_eq!(rows["shape"], "box", "an empty product reserves a box");
        // The sample's own shelf stands under the void, so the free space starts at its top face.
        assert_eq!(rows["size_mm"], json!([600.0, 560.0, 702.0]));
        assert!(
            rows["planes"].as_array().is_some_and(|planes| planes.len() >= 6),
            "the envelope's six planes bound the void: {rows}"
        );

        let by_id = report_of(&["query", &path.display().to_string(), "niche", "--id", "7"]);
        assert_eq!(by_id["rows"]["product_id"], 5, "the light stands inside the product");
        assert_eq!(by_id["rows"]["size_mm"], json!([600.0, 560.0, 702.0]), "the same void from a node's origin");
    }

    #[test]
    fn a_niche_outside_every_product_is_a_sentence() {
        let directory = scratch_dir("cli-query-outside");
        let path = saved(&directory);
        let args = args_of(&["query", &path.display().to_string(), "niche", "--at", "5000,5000,5000"]);
        let message = report(&args).expect_err("the point is outside").to_string();
        assert!(message.contains("5000"), "the sentence names the point: {message}");
        assert!(message.contains("outside every product"), "{message}");
    }

    #[test]
    fn a_query_names_what_it_could_not_answer() {
        let error = crate::cli::parse(&words(&["query", "scene.z3l", "boxes"])).expect_err("there is no such report");
        assert!(error.to_string().contains("tree"), "the refusal lists the reports: {error}");
        let error = crate::cli::parse(&words(&["query", "scene.z3l"])).expect_err("a report is required");
        assert!(error.to_string().contains("tree"), "{error}");
        let error = crate::cli::parse(&words(&["query", "scene.z3l", "tree", "--id", "0"])).expect_err("zero is no id");
        assert!(error.to_string().contains("not a node id"), "{error}");
        let error = crate::cli::parse(&words(&["query", "scene.z3l", "niche", "--at", "1,2"]))
            .expect_err("three coordinates are required");
        assert!(error.to_string().contains("three coordinates"), "{error}");
        let error = crate::cli::parse(&words(&["query", "scene.z3l", "niche", "--at", "1,2,3,4"]))
            .expect_err("three coordinates are enough");
        assert!(error.to_string().contains("more than three"), "{error}");

        let directory = scratch_dir("cli-query-refused");
        let path = saved(&directory);
        let args = args_of(&["query", &path.display().to_string(), "niche"]);
        let message = report(&args).expect_err("a niche needs a point").to_string();
        assert!(message.contains("--at"), "{message}");
    }
}
