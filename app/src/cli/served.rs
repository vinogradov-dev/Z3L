//! `z3l mcp serve`: the editor as a tool server over this process's standard streams.
//!
//! The session is the one a headless run has — the scene read once, the build's default catalogue
//! and the real contact adapter — and the door adds nothing to it: a tool dispatches through
//! [`engine::Session::batch`], which is what `z3l run` calls, so one undo entry, all or nothing and
//! a refusal's own sentence are the same on both doors (ADR 0183, ADR 0198).
//!
//! A batch that lands is written over the scene the line read, once, at the end of the call;
//! `--out` does not exist here because a served session lives for many calls and "the result" is
//! every one of them. A session served without a scene writes nothing and says so in every answer,
//! and the sections a newer build left in a file ride back on each write (ADR 0096).
//!
//! The report of a run never reaches standard output: that stream is the protocol's, and a stray
//! line would be a broken frame. What a person needs to read goes to the log sink on stderr.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use model::Project;
use project::SceneFile;
use serde_json::Value;

use super::{CliError, McpArgs};

/// Opens the session a served run works in, with the writer that saves its scene.
///
/// Split from [`perform`] so the composition — which document, which product, which writer — can be
/// checked without a protocol and without blocking on a stream.
pub(super) fn open(args: &McpArgs) -> Result<(engine::Session, Option<Arc<dyn mcp::SceneSink>>), CliError> {
    let (mut session, sink) = match &args.scene {
        Some(path) => {
            let scene = SceneFile::read(path).map_err(CliError::Scene)?;
            let extra = scene.extra().clone();
            let session = super::recipe::session_of(scene.into_document())?;
            let writer = SceneWriter { path: path.clone(), extra };
            (session, Some(Arc::new(writer) as Arc<dyn mcp::SceneSink>))
        }
        None => (
            crate::scene_build::empty_session_with(crate::scene_build::default_catalog().map_err(CliError::Catalog)?)
                .map_err(CliError::Catalog)?,
            None,
        ),
    };
    super::recipe::open_product(&mut session, args.product)?;
    Ok((session, sink))
}

/// Serves the tools over standard input and output until the client closes them.
///
/// Nothing is printed: the streams carry the protocol, so a report would be a broken frame. The one
/// sentence a person may want — that the server is up, and that a scene is or is not written — goes
/// to the log.
pub(super) fn perform(args: &McpArgs) -> Result<(), CliError> {
    let (session, sink) = open(args)?;
    log::info!(
        target: "z3l::shell",
        "serving the tools on stdio over {}",
        match &args.scene {
            Some(path) => path.display().to_string(),
            None => "a document that is not written".to_owned(),
        }
    );
    mcp::serve_stdio(mcp::serve_session(session, sink)).map_err(|error| CliError::Mcp(error.to_string()))
}

/// The scene a served session writes, and the sections this build must give back.
#[derive(Debug)]
struct SceneWriter {
    /// The file the scene was read from, written again when a batch lands.
    path: PathBuf,
    /// The top-level sections a newer build left in the file, carried back on every write.
    extra: BTreeMap<String, Value>,
}

impl mcp::SceneSink for SceneWriter {
    fn save(&self, project: &Project) -> Result<(), String> {
        SceneFile::new(project.clone())
            .with_extra(self.extra.clone())
            .write(&self.path)
            .map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use model::{ClassCode, Envelope, Id, Placement};
    use serde_json::json;
    use testkit::{millimetres, sample_project, scratch_dir};

    use super::*;

    /// A saved scene with one product, returned with its path and the product's id.
    fn saved(directory: &std::path::Path) -> (PathBuf, Id) {
        let mut project = sample_project();
        let root = project.root().id();
        let envelope = Envelope::new(millimetres(600), millimetres(560), millimetres(720)).expect("a valid box");
        let product = project
            .add_product(root, "Cabinet", ClassCode::GENERAL_PRODUCT, envelope, Placement::IDENTITY)
            .expect("a product beside the sample nodes");
        let path = directory.join("cabinet.z3l");
        SceneFile::new(project).write(&path).expect("the fixture scene is saved");
        (path, product)
    }

    #[test]
    fn the_line_names_its_subcommand_before_it_does_anything() {
        let error = crate::cli::parse(&["mcp".to_owned()]).expect_err("`mcp` alone names no job");
        assert!(error.to_string().contains("`mcp` needs a subcommand"), "{error}");
        let error = crate::cli::parse(&["mcp".to_owned(), "nonsense".to_owned()]).expect_err("there is one subcommand");
        assert!(error.to_string().contains("`mcp nonsense` is not a subcommand"), "{error}");
        let error =
            crate::cli::parse(&["mcp".to_owned(), "serve".to_owned(), "one.z3l".to_owned(), "two.z3l".to_owned()])
                .expect_err("one scene is served at a time");
        assert!(error.to_string().contains("reads one scene"), "{error}");
    }

    #[test]
    fn the_line_reads_the_scene_and_the_product() {
        let parsed = crate::cli::parse(&[
            "mcp".to_owned(),
            "serve".to_owned(),
            "cabinet.z3l".to_owned(),
            "--product".to_owned(),
            "7".to_owned(),
        ])
        .expect("the line parses");
        let product = Id::new(7).expect("an id");
        assert_eq!(
            parsed,
            crate::cli::Command::Mcp(McpArgs { scene: Some(PathBuf::from("cabinet.z3l")), product: Some(product) }),
            "the line names the scene and the product the tools work in"
        );
        let bare = crate::cli::parse(&["mcp".to_owned(), "serve".to_owned()]).expect("no scene is legal");
        assert_eq!(bare, crate::cli::Command::Mcp(McpArgs { scene: None, product: None }));
    }

    #[test]
    fn the_line_reads_a_scene_and_serves_the_product_inside_it() {
        let directory = scratch_dir("cli-mcp-open");
        let (path, product) = saved(&directory);
        let args = McpArgs { scene: Some(path.clone()), product: None };
        let (session, sink) = open(&args).expect("the scene serves");
        assert_eq!(session.active_product(), Some(product), "one product opens by itself");
        assert!(sink.is_some(), "a scene is written back");
    }

    #[test]
    fn a_line_without_a_scene_serves_a_document_that_is_not_written() {
        let (session, sink) = open(&McpArgs { scene: None, product: None }).expect("an empty document serves");
        assert!(session.project().products().is_empty(), "nothing was read");
        assert!(sink.is_none(), "there is no file, so the answers must not claim one");
    }

    #[test]
    fn a_written_scene_keeps_the_sections_this_build_does_not_know() {
        let directory = scratch_dir("cli-mcp-write");
        let (path, product) = saved(&directory);
        // A newer build's section, which this one must give back unchanged (ADR 0096).
        let mut text: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("the fixture reads"))
                .expect("a scene envelope");
        if let Some(object) = text.as_object_mut() {
            object.insert("future_section".to_owned(), json!({"kept": true}));
        }
        std::fs::write(&path, serde_json::to_string(&text).expect("the envelope serializes"))
            .expect("the fixture is written");

        let (mut session, sink) =
            open(&McpArgs { scene: Some(path.clone()), product: None }).expect("the scene serves");
        let sink = sink.expect("a scene is written back");
        session
            .dispatch("node.rename", &json!({"node": product.get(), "name": "Renamed"}))
            .expect("the product is renamed");
        sink.save(session.project()).expect("the writer saves the scene");
        let written: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("the scene reads back"))
                .expect("a scene envelope");
        assert_eq!(written["future_section"], json!({"kept": true}), "the unknown section rode back");
        let reread = SceneFile::read(&path).expect("the write is a scene");
        assert_eq!(
            reread.document().container(product).map(model::Container::name),
            Some("Renamed"),
            "the document the sink was handed is the file's"
        );
    }
}
