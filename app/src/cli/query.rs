//! The read door: one question about the scene, one JSON answer, and the resources a client reads.
//!
//! A question is answered by [`engine::Session::read`] and by nothing else, so this module is only
//! the door: it turns words into one typed [`Query`], prints the answer with the envelope a caller
//! needs — which question, which file and which revision it read — and lists the questions that are
//! stable enough to be read at an address instead of being called.
//!
//! A **resource** is a question with no argument: the tree, the reserved boxes, the cut list and the
//! panels' issues change only when the document does, so a client reads one by URI and caches the
//! answer by the revision it carries. The niche takes a point, so it is an argument of the tool call
//! and not a resource (ADR 0183, ADR 0185, ADR 0196).
//!
//! Nothing here writes: the session is built, read and dropped, so a query on a saved file is safe
//! to run while an editor holds the same scene open.
//!
//! Changes when: a question is added, a report's fields change, or the resource table grows.

use engine::Query;
use model::Project;
use project::SceneFile;
use serde_json::{Value, json};

use super::{CliError, QueryArgs};

/// The media type of every answer.
pub const MIME: &str = "application/json";

/// One resource: a stable question, read at an address instead of being called.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Resource {
    /// The URI the resource is read at, and the one a client caches by.
    pub uri: &'static str,
    /// The short name a resource list carries; it is the question's own name.
    pub name: &'static str,
    /// The sentence that says what an answer holds.
    pub description: &'static str,
    /// The question the URI names.
    pub query: Query,
}

impl Resource {
    /// Whether the resource answers at `uri`.
    fn is_at(&self, uri: &str) -> bool {
        self.uri.eq_ignore_ascii_case(uri)
    }
}

/// The stable questions, and the URI each answers to.
///
/// The four that need no argument are here; `niche` measures the space around a point, so its answer
/// is about a place the caller chose and it stays a tool argument.
pub const RESOURCES: [Resource; 4] = [
    Resource {
        uri: "z3l://scene/tree",
        name: "tree",
        description: "every node with its id, kind, name and children",
        query: Query::Tree { node: None },
    },
    Resource {
        uri: "z3l://scene/box",
        name: "box",
        description: "the box a product reserves, in its own frame and in world millimetres",
        query: Query::Box { node: None },
    },
    Resource {
        uri: "z3l://scene/parts",
        name: "parts",
        description: "the cut list -- the published `parts` table of the order package",
        query: Query::Parts,
    },
    Resource {
        uri: "z3l://scene/diagnostics",
        name: "diagnostics",
        description: "every panel problem the catalogue resolves, with a stable code",
        query: Query::Diagnostics,
    },
];

/// What one query line asks for: the table of resources, a question, or one resource.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ask {
    /// Print the resource table, which needs no scene.
    Resources,
    /// One of the questions, by the name the word form uses.
    Question(&'static str),
    /// One resource, by the URI a client reads it at.
    Resource(&'static Resource),
}

impl Ask {
    /// Reads what a line asks for.
    ///
    /// A URI is a resource and a bare word is a question; anything else is refused here, before a
    /// scene is read, with the sentence that names what is accepted.
    pub fn parse(word: &str) -> Result<Self, CliError> {
        if word.starts_with("z3l:") {
            return resource_at(word).map(Self::Resource).ok_or_else(|| CliError::Usage(no_resource(word)));
        }
        let Some(name) = Query::NAMES.iter().find(|name| name.eq_ignore_ascii_case(word)) else {
            return Err(CliError::Usage(format!(
                "`{word}` is neither a report nor a resource; name {} or {}",
                Self::names(),
                uris()
            )));
        };
        Ok(Self::Question(name))
    }

    /// The names a help text and a refusal list, in the order the answers follow one another.
    pub fn names() -> String {
        Query::NAMES.join(", ")
    }
}

/// The resource whose URI is `uri`, compared without case so a client's spelling is not a trap.
pub fn resource_at(uri: &str) -> Option<&'static Resource> {
    RESOURCES.iter().find(|resource| resource.is_at(uri))
}

/// Reads the scene and prints the answer the line asked for.
///
/// The resource table needs no scene, so a caller can list what it may read before it holds a file.
/// Every other line opens the session the window would open and asks it one question.
pub fn report(args: &QueryArgs) -> Result<String, CliError> {
    if args.ask == Ask::Resources {
        return text(listing());
    }
    let path = args
        .scene
        .as_deref()
        .ok_or_else(|| CliError::Usage("query needs a scene: `z3l query <scene.z3l> tree`".to_owned()))?;
    let file = path.file_name().map(|name| name.to_string_lossy().into_owned());
    let scene = SceneFile::read(path).map_err(CliError::Scene)?;
    let session = super::recipe::session_of(scene.into_document())?;
    let query = query_of(args, session.project())?;
    let rows = session.read(query).map_err(|error| CliError::Query(error.to_string()))?;
    let mut envelope = json!({
        "query": query.name(),
        "document": {"file": file, "revision": session.project().revision().get()},
        "rows": rows,
    });
    if let Ask::Resource(resource) = args.ask {
        insert(&mut envelope, "uri", Value::from(resource.uri));
    }
    text(envelope)
}

/// The question a line asks, with the argument it named resolved against the document.
///
/// Every refusal here is about the argument rather than the question, so the reason names the flag
/// that has no place instead of repeating the report's name.
fn query_of(args: &QueryArgs, project: &Project) -> Result<Query, CliError> {
    match args.ask {
        // `check` refused every scene and every flag the resource table would have carried.
        Ask::Resource(resource) => Ok(resource.query),
        Ask::Resources => Err(CliError::Usage("the resource list needs no scene".to_owned())),
        Ask::Question("tree") => Ok(Query::Tree { node: args.id }),
        Ask::Question("box") => Ok(Query::Box { node: args.id }),
        Ask::Question("bounds") => Ok(Query::Bounds { node: args.id.ok_or_else(bounds_needs_a_node)? }),
        Ask::Question("parts") => Ok(Query::Parts),
        Ask::Question("diagnostics") => Ok(Query::Diagnostics),
        Ask::Question("niche") => Ok(Query::Niche { at: point_of(args, project)? }),
        Ask::Question("materials") => Ok(Query::Materials),
        Ask::Question(other) => Err(CliError::Usage(format!("`{other}` is not a question this door reads"))),
    }
}

/// The refusal a `bounds` line without a node reads.
fn bounds_needs_a_node() -> CliError {
    CliError::Usage("`bounds` measures one node: name it with `--id <node>`".to_owned())
}

/// The point `niche` measures: the one the line names, or the origin of the node it names.
fn point_of(args: &QueryArgs, project: &Project) -> Result<[f64; 3], CliError> {
    match (args.at, args.id) {
        (Some(at), _) => Ok(at),
        (None, Some(id)) => {
            let origin = project.origin_world(id).ok_or_else(|| CliError::Query(format!("no node has id {id}")))?;
            Ok([origin.x, origin.y, origin.z])
        }
        (None, None) => Err(CliError::Usage("`niche` needs a point: `--at x,y,z` or `--id <node>`".to_owned())),
    }
}

/// Refuses an argument the question the line names does not read.
///
/// The words a line writes are the shell's, so the shell is where an argument that belongs to another
/// question is caught: a client that sends a typed selector cannot make the mistake at all.
pub fn check(args: &QueryArgs) -> Result<(), CliError> {
    if args.ask == Ask::Resources {
        return match (args.scene.is_some(), args.id, args.at) {
            (false, None, None) => Ok(()),
            _ => Err(CliError::Usage("the resource table is printed on its own: `z3l query --resources`".to_owned())),
        };
    }
    if args.scene.is_none() {
        return Err(CliError::Usage("query needs a scene: `z3l query <scene.z3l> tree`".to_owned()));
    }
    let (takes_node, takes_point) = match args.ask {
        Ask::Question("niche") => (true, true),
        Ask::Question("tree" | "box" | "bounds") => (true, false),
        _ => (false, false),
    };
    if args.ask == Ask::Question("bounds") && args.id.is_none() {
        return Err(bounds_needs_a_node());
    }
    if !takes_node && let Some(id) = args.id {
        return Err(CliError::Usage(format!("`--id {id}` has no place: {}", what(args.ask))));
    }
    if !takes_point && args.at.is_some() {
        return Err(CliError::Usage(format!("`--at` is a point, and {}", what(args.ask))));
    }
    Ok(())
}

/// What the thing a line names takes, for the sentence that refuses an argument.
fn what(ask: Ask) -> String {
    match ask {
        Ask::Question("niche") => "`niche` takes a point and a node".to_owned(),
        Ask::Question(name) => format!("`{name}` takes a node"),
        Ask::Resource(resource) => format!("`{}` is a resource, so it takes no `--id` or `--at`", resource.uri),
        Ask::Resources => "the resource table takes no argument".to_owned(),
    }
}

/// The resource table as one JSON array: the URI, the name, the sentence and the media type.
fn listing() -> Value {
    Value::Array(
        RESOURCES
            .iter()
            .map(|resource| {
                json!({
                    "uri": resource.uri,
                    "name": resource.name,
                    "description": resource.description,
                    "mimeType": MIME,
                })
            })
            .collect(),
    )
}

/// Every URI a line may name, for the sentence that lists them.
fn uris() -> String {
    RESOURCES.iter().map(|resource| resource.uri).collect::<Vec<_>>().join(", ")
}

/// The sentence a line gets for a URI no resource answers at.
fn no_resource(uri: &str) -> String {
    format!("`{uri}` is not a resource; read one of {}", uris())
}

/// One JSON value as the text the command writes.
fn text(value: Value) -> Result<String, CliError> {
    serde_json::to_string_pretty(&value).map_err(|error| CliError::Query(error.to_string()))
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

    use geom::{DQuat, DVec3};
    use model::{ClassCode, Envelope, LightKind, PanelNominalSize, Placement};
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
        let size = PanelNominalSize::new(millimetres(700), millimetres(500)).expect("a valid size");
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

    /// The report a line asks for, parsed back into JSON.
    fn report_of(line: &[&str]) -> Value {
        let command = crate::cli::parse(&words(line)).expect("the line parses");
        let crate::cli::Command::Query(args) = command else {
            panic!("the line names a query");
        };
        let text = report(&args).expect("the report is written");
        serde_json::from_str(&text).expect("the report is JSON")
    }

    /// The report a line is refused with.
    fn refusal(line: &[&str]) -> String {
        let command = crate::cli::parse(&words(line)).expect("the line reads its words");
        let crate::cli::Command::Query(args) = command else {
            panic!("the line names a query");
        };
        report(&args).expect_err("the line is refused").to_string()
    }

    #[test]
    fn a_report_names_the_question_the_file_and_the_revision_it_read() {
        let directory = scratch_dir("cli-query-envelope");
        let path = saved(&directory);
        let answer = report_of(&["query", &path.display().to_string(), "tree"]);
        assert_eq!(answer["query"], "tree");
        assert_eq!(answer["document"]["file"], "cabinet.z3l");
        assert!(
            answer["document"]["revision"].as_u64().is_some_and(|revision| revision > 0),
            "the answer says which revision it read: {answer}"
        );
        assert!(answer["rows"]["children"].as_array().is_some_and(|children| !children.is_empty()));
        assert!(answer.get("uri").is_none(), "a question asked by name is no resource read");
    }

    #[test]
    fn every_question_the_word_form_names_is_answered() {
        let directory = scratch_dir("cli-query-each");
        let path = saved(&directory);
        let scene = path.display().to_string();
        for (line, expected) in [
            (vec!["query", &scene, "tree"], "tree"),
            (vec!["query", &scene, "box"], "box"),
            (vec!["query", &scene, "bounds", "--id", "2"], "bounds"),
            (vec!["query", &scene, "parts"], "parts"),
            (vec!["query", &scene, "diagnostics"], "diagnostics"),
            (vec!["query", &scene, "niche", "--at", "300,280,360"], "niche"),
        ] {
            let answer = report_of(&line);
            assert_eq!(answer["query"], expected, "`{expected}` is answered by its own name");
            assert!(answer["rows"].is_array() || answer["rows"].is_object(), "{answer}");
        }
    }

    #[test]
    fn a_bounds_answer_measures_the_node_the_line_names() {
        let directory = scratch_dir("cli-query-bounds");
        let path = saved(&directory);
        let scene = path.display().to_string();
        let answer = report_of(&["query", &scene, "bounds", "--id", "2"]);
        assert_eq!(answer["rows"]["kind"], "panel");
        assert_eq!(answer["rows"]["local_size_mm"], json!([700.0, 500.0, 18.0]));

        let error = crate::cli::parse(&words(&["query", &scene, "bounds"])).expect_err("a box is the box of one node");
        assert!(error.to_string().contains("--id"), "the refusal names the flag: {error}");
    }

    #[test]
    fn a_niche_is_measured_at_a_point_or_at_a_node() {
        let directory = scratch_dir("cli-query-niche");
        let path = saved(&directory);
        let scene = path.display().to_string();
        let by_point = report_of(&["query", &scene, "niche", "--at", "300,280,360"]);
        let by_node = report_of(&["query", &scene, "niche", "--id", "7"]);
        assert_eq!(by_node["rows"], by_point["rows"], "the light's origin is the point the fixture names");
        assert_eq!(by_point["rows"]["product_id"], 5);
        assert_eq!(by_point["rows"]["size_mm"], json!([600.0, 560.0, 702.0]));

        let missing = refusal(&["query", &scene, "niche", "--at", "5000,5000,5000"]);
        assert!(missing.contains("outside every product"), "{missing}");
    }

    #[test]
    fn the_resources_are_listed_and_the_tree_resource_is_the_tree_report() {
        // One derivation, two doors: the URI is an address for the answer the word form prints.
        let listed = report_of(&["query", "--resources"]);
        let resources = listed.as_array().expect("one row per resource");
        assert_eq!(resources.len(), RESOURCES.len(), "every resource is listed");
        let uris: Vec<&str> = resources.iter().filter_map(|row| row["uri"].as_str()).collect();
        assert_eq!(uris, vec!["z3l://scene/tree", "z3l://scene/box", "z3l://scene/parts", "z3l://scene/diagnostics"]);
        assert!(resources.iter().all(|row| row["mimeType"] == MIME && row["description"].is_string()));
        assert!(
            !uris.iter().any(|uri| uri.ends_with("/niche")),
            "a niche takes a point, so it is no resource: {uris:?}"
        );

        let directory = scratch_dir("cli-query-resource");
        let path = saved(&directory);
        let scene = path.display().to_string();
        let by_name = report_of(&["query", &scene, "tree"]);
        let by_uri = report_of(&["query", &scene, "z3l://scene/tree"]);
        assert_eq!(by_uri["uri"], "z3l://scene/tree");
        assert_eq!(by_uri["query"], by_name["query"]);
        assert_eq!(by_uri["document"], by_name["document"]);
        assert_eq!(by_uri["rows"], by_name["rows"], "the resource and the report are the same derivation");

        // Every listed resource answers on the scene the fixture saved.
        for uri in uris {
            let answer = report_of(&["query", &scene, uri]);
            assert_eq!(answer["uri"], uri);
            assert!(answer["rows"].is_array() || answer["rows"].is_object(), "{answer}");
        }
    }

    #[test]
    fn an_unknown_selector_is_a_readable_refusal() {
        let error = crate::cli::parse(&words(&["query", "scene.z3l", "boxes"])).expect_err("there is no such report");
        let message = error.to_string();
        assert!(message.contains("boxes"), "the refusal quotes the name: {message}");
        for name in Query::NAMES {
            assert!(message.contains(name), "the refusal lists `{name}`: {message}");
        }

        let error = crate::cli::parse(&words(&["query", "scene.z3l", "z3l://scene/boxes"]))
            .expect_err("there is no such resource");
        let message = error.to_string();
        assert!(message.contains("z3l://scene/boxes"), "{message}");
        assert!(message.contains("z3l://scene/tree"), "the refusal lists the URIs: {message}");

        let error = crate::cli::parse(&words(&["query", "scene.z3l"])).expect_err("a report is required");
        assert!(error.to_string().contains("tree"), "{error}");
    }

    #[test]
    fn an_argument_the_question_does_not_read_is_refused_by_name() {
        assert_eq!(Ask::parse("TREE").expect("a name is not case sensitive"), Ask::Question("tree"));
        assert_eq!(
            resource_at("z3l://scene/parts").map(|resource| resource.query),
            Some(Query::Parts),
            "a URI names a resource and the question it reads"
        );

        for (line, expected) in [
            (vec!["query", "scene.z3l", "tree", "--at", "1,2,3"], "`--at` is a point"),
            (vec!["query", "scene.z3l", "bounds", "--id", "2", "--at", "1,2,3"], "`--at` is a point"),
            (vec!["query", "scene.z3l", "parts", "--id", "5"], "`--id 5` has no place"),
            (vec!["query", "scene.z3l", "diagnostics", "--id", "5"], "`--id 5` has no place"),
            (vec!["query", "scene.z3l", "z3l://scene/tree", "--id", "5"], "takes no `--id` or `--at`"),
        ] {
            let error = crate::cli::parse(&words(&line)).expect_err("the argument has no place");
            assert!(error.to_string().contains(expected), "`{expected}` is missing from: {error}");
        }

        let error = crate::cli::parse(&words(&["query", "--resources", "scene.z3l", "tree"]))
            .expect_err("the resource table takes no scene");
        assert!(error.to_string().contains("resource"), "{error}");
    }

    #[test]
    fn a_query_reads_the_file_and_never_writes_it() {
        let directory = scratch_dir("cli-query-pure");
        let path = saved(&directory);
        let before = std::fs::read_to_string(&path).expect("the fixture reads");
        for line in [
            vec!["query", &path.display().to_string(), "tree"],
            vec!["query", &path.display().to_string(), "parts"],
            vec!["query", &path.display().to_string(), "diagnostics"],
        ] {
            report_of(&line);
        }
        assert_eq!(std::fs::read_to_string(&path).expect("the fixture still reads"), before, "nothing was written");
    }
}
