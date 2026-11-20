//! A recipe: one command per line, one batch, one undo entry, one write.
//!
//! A recipe is JSON Lines — one object per line, `{"command": <id>, "params": {...}}` — because a
//! line is a step and a diff of two recipes reads as a diff of two runs (the same argument the order
//! package makes for its tables, ADR 0095). Nothing here mutates a document directly: every line is
//! a [`engine::BatchStep`] and the whole list is one [`engine::Session::batch`], so undo, the class
//! guard and the command's own validation stay the only door (`docs/domain/prototypes.md` §8). The
//! batch is the same operation a tool over the same commands calls, so the two answers cannot
//! disagree (ADR 0190, ADR 0197).
//!
//! A run is all or nothing and one undo entry: the batch plans the whole list on a copy of the
//! document and installs it in one piece, and the scene is written once, at the end, through
//! [`project::SceneFile`] and only when every line succeeded and `--dry-run` is absent. A failing
//! line reports its number and the command's own sentence, and writes nothing at all — a recipe that
//! half applied would be a scene nobody asked for.
//!
//! Changes when: the step shape grows a key, or a run stops being one undo entry.

use std::path::Path;

use engine::{BatchOptions, BatchReport, BatchStep, Session, StepOutcome};
use project::SceneFile;
use serde_json::{Map, Value};

use super::{CliError, RunArgs};

/// One line of a recipe, already read and checked.
#[derive(Clone, Debug, PartialEq)]
struct Step {
    /// The line it came from, counting from one, so an error can name it.
    line: usize,
    /// The stable command id it dispatches.
    command: String,
    /// The parameters it hands that command; an empty object when the line omits them.
    params: Value,
}

impl Step {
    /// The engine step this line dispatches into the batch.
    fn batch(&self) -> BatchStep {
        BatchStep::new(self.command.clone(), self.params.clone())
    }
}

/// Runs a recipe against a saved scene and returns the lines to print.
///
/// The scene is read once, the commands run as one batch, and the document is written once: to
/// `--out` when the line names one, over the scene read otherwise. `--dry-run` plans the whole batch
/// without changing the document, so a recipe can be checked against a real document before it
/// changes a file.
pub fn perform(args: &RunArgs) -> Result<Vec<String>, CliError> {
    let scene = SceneFile::read(&args.scene).map_err(CliError::Scene)?;
    let extra = scene.extra().clone();
    let mut session = session_of(scene.into_document())?;
    open_product(&mut session, args.product)?;
    let steps = parse(&read(&args.commands)?)?;
    if steps.is_empty() {
        return Ok(vec![format!("{}: the recipe holds no step, so nothing was written", args.commands.display())]);
    }
    let batch: Vec<BatchStep> = steps.iter().map(Step::batch).collect();
    let report = session.batch(&batch, BatchOptions::ALL_OR_NOTHING.dry_run(args.dry_run));
    if let Some((number, error)) = report.refusal() {
        return Err(dispatch_error(&steps, number, error));
    }
    let mut lines = performed(&steps, &report);
    let revision = session.project().revision().get();
    if args.dry_run {
        lines.push(format!("dry run: {} steps, revision {revision}, nothing was written", steps.len()));
        return Ok(lines);
    }
    let target = args.out.as_deref().unwrap_or(args.scene.as_path());
    SceneFile::new(session.project().clone()).with_extra(extra).write(target).map_err(CliError::Scene)?;
    lines.push(format!("{} steps, revision {revision} -> {}", steps.len(), target.display()));
    Ok(lines)
}

/// Opens the session a headless run works in: the document, the default catalogue and the contact
/// adapter, which is the same three the window gives a scene (`docs/domain/scene-file.md` records
/// that the file carries no catalogue, so a headless run starts from the build's own).
///
/// `query` opens the same session, so a report and a recipe read one document in one way.
pub(super) fn session_of(document: model::Project) -> Result<Session, CliError> {
    let catalog = crate::scene_build::default_catalog().map_err(CliError::Catalog)?;
    Ok(Session::with_directory(document, catalog).with_contact(crate::scene_build::contact_engine()))
}

/// Opens the product the recipe builds in.
///
/// A furniture part is born only inside the open product (ADR 0098), and opening one is session
/// state, not a command: the window opens what the person double-clicks. A headless run has no
/// person, so `--product` names one and a scene with exactly one product opens it — one product is
/// no guess. A scene with several and no flag leaves the workspace closed, and the command's own
/// refusal says so.
fn open_product(session: &mut Session, product: Option<model::Id>) -> Result<(), CliError> {
    let chosen = match product {
        Some(id) => Some(id),
        None => match session.project().products().as_slice() {
            [only] => Some(*only),
            _ => None,
        },
    };
    match chosen {
        Some(id) if session.open_product(id) => Ok(()),
        Some(id) => Err(CliError::Recipe(format!("no product has id {id}; `--product` names a product of the scene"))),
        None => Ok(()),
    }
}

/// The lines a run prints before its own last line: one per step that ran, numbered by the recipe's
/// own lines so the output of a recipe with blank lines still points at the file.
///
/// A refused step is not printed: the refusal becomes the error the run fails with.
fn performed(steps: &[Step], report: &BatchReport) -> Vec<String> {
    report
        .steps()
        .iter()
        .zip(steps)
        .filter_map(|(step, source)| {
            let StepOutcome::Ran(outcome) = step.outcome() else {
                return None;
            };
            Some(match outcome.created_id() {
                Some(id) => format!("{}: {} created {id}", source.line, step.command()),
                None => format!("{}: {} ok", source.line, step.command()),
            })
        })
        .collect()
}

/// The error a refused batch fails the run with: the recipe line, the command and its own sentence.
///
/// A batch is one answer, so the run stops at the first refusal; the report is what says what the
/// applied steps would have been, and the command line writes none of it.
fn dispatch_error(steps: &[Step], number: usize, error: &engine::CommandError) -> CliError {
    match steps.get(number.saturating_sub(1)) {
        Some(step) => CliError::Dispatch { line: step.line, command: step.command.clone(), error: error.clone() },
        None => CliError::Recipe(format!("step {number} refused: {error}")),
    }
}

/// Reads the recipe file, naming it when it cannot be read.
fn read(path: &Path) -> Result<String, CliError> {
    std::fs::read_to_string(path).map_err(|error| CliError::Recipe(format!("cannot read {}: {error}", path.display())))
}

/// Reads every step of a recipe, naming the line that is not one.
///
/// A blank line is skipped so a recipe can be laid out in groups. Every other line is one JSON
/// object with `command` and, optionally, `params`; any other key is refused rather than ignored,
/// because a misspelt `param` would otherwise run the command with no parameters at all.
fn parse(text: &str) -> Result<Vec<Step>, CliError> {
    let mut steps = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let number = index.saturating_add(1);
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(line)
            .map_err(|error| CliError::Recipe(format!("line {number}: not JSON: {error}")))?;
        let Value::Object(object) = value else {
            return Err(CliError::Recipe(format!("line {number}: a step is one JSON object, not {}", kind(&value))));
        };
        steps.push(step(number, object)?);
    }
    Ok(steps)
}

/// Reads one step out of the object a line held.
fn step(number: usize, object: Map<String, Value>) -> Result<Step, CliError> {
    for key in object.keys() {
        if key != "command" && key != "params" {
            return Err(CliError::Recipe(format!(
                "line {number}: `{key}` is not a step key; a step names `command` and `params`"
            )));
        }
    }
    let command = match object.get("command") {
        Some(Value::String(id)) if !id.trim().is_empty() => id.clone(),
        Some(other) => {
            return Err(CliError::Recipe(format!("line {number}: `command` is a command id, not {}", kind(other))));
        }
        None => return Err(CliError::Recipe(format!("line {number}: the step names no `command`"))),
    };
    let params = match object.get("params") {
        None => Value::Object(Map::new()),
        Some(Value::Object(params)) => Value::Object(params.clone()),
        Some(other) => {
            return Err(CliError::Recipe(format!("line {number}: `params` is one JSON object, not {}", kind(other))));
        }
    };
    Ok(Step { line: number, command, params })
}

/// What a JSON value is, for a sentence that says what was written instead.
const fn kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a flag",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use model::{ClassCode, Envelope, Id, Placement};
    use testkit::{millimetres, sample_project, scratch_dir};

    use super::*;

    /// One recipe line that creates a panel under `parent`.
    fn panel_line(parent: Id, name: &str) -> String {
        format!(
            concat!(
                r#"{{"command":"panel.create","params":{{"parent":{},"name":"{}","#,
                r#""nominal_size":{{"length":700000,"width":500000}},"material":"1-1","#,
                r#""placement":{{"translation":[0,0,0],"rotation":[0,0,0,1]}}}}}}"#
            ),
            parent.get(),
            name
        )
    }

    /// One recipe line that renames a node without creating anything.
    fn rename_line(id: Id) -> String {
        format!(r#"{{"command":"node.rename","params":{{"node":{},"name":"Frame"}}}}"#, id.get())
    }

    /// The recipe most tests run: two creations and one edit, three lines in all.
    fn recipe(parent: Id) -> String {
        format!("{}\n\n{}\n{}\n", panel_line(parent, "Side"), panel_line(parent, "Shelf"), rename_line(parent))
    }

    /// A saved scene with one product, and the scene's path with the root's and the product's ids.
    fn saved(directory: &Path) -> (std::path::PathBuf, Id, Id) {
        let mut project = sample_project();
        let root = project.root().id();
        let envelope = Envelope::new(millimetres(600), millimetres(560), millimetres(720)).expect("a valid box");
        let product = project
            .add_product(root, "Cabinet", ClassCode::GENERAL_PRODUCT, envelope, Placement::IDENTITY)
            .expect("a product beside the sample nodes");
        let path = directory.join("cabinet.z3l");
        SceneFile::new(project).write(&path).expect("the fixture scene is saved");
        (path, root, product)
    }

    /// The words a test passes, without the program name.
    fn words(line: &[&str]) -> Vec<String> {
        line.iter().map(|word| (*word).to_owned()).collect()
    }

    /// The panels a container holds.
    fn children(project: &model::Project, container: Id) -> usize {
        project.container(container).map_or(0, |container| container.children().len())
    }

    #[test]
    fn a_three_line_recipe_builds_the_scene_in_one_undo_entry() {
        let directory = scratch_dir("cli-recipe-group");
        let (path, _root, product) = saved(&directory);
        let recipe_path = directory.join("build.jsonl");
        std::fs::write(&recipe_path, recipe(product)).expect("the recipe is written");
        let command = crate::cli::parse(&words(&[
            "run",
            &path.display().to_string(),
            "--commands",
            &recipe_path.display().to_string(),
        ]))
        .expect("the line parses");
        let crate::cli::Command::Run(args) = command else {
            panic!("the line names a run");
        };
        let lines = perform(&args).expect("the recipe runs");
        assert_eq!(lines.len(), 4, "one line per step and one for the write: {lines:?}");
        assert!(lines.first().is_some_and(|line| line.starts_with("1: panel.create")), "{lines:?}");
        assert!(
            lines.get(1).is_some_and(|line| line.starts_with("3: panel.create")),
            "the blank line keeps the recipe's own numbering: {lines:?}"
        );

        let written = SceneFile::read(&path).expect("the scene is written back");
        let project = written.document();
        assert_eq!(children(project, product), 2, "two panels were created under the product");
        assert_eq!(project.container(product).map(model::Container::name), Some("Frame"), "the rename happened");
    }

    #[test]
    fn the_whole_recipe_is_one_undo_entry() {
        let directory = scratch_dir("cli-recipe-undo");
        let (_path, _root, product) = saved(&directory);
        let scene = SceneFile::read(&directory.join("cabinet.z3l")).expect("the fixture reads");
        let mut session = session_of(scene.into_document()).expect("a session");
        open_product(&mut session, Some(product)).expect("the fixture holds its product");
        let steps = parse(&recipe(product)).expect("the recipe parses");
        let batch: Vec<BatchStep> = steps.iter().map(Step::batch).collect();
        let report = session.batch(&batch, BatchOptions::ALL_OR_NOTHING);
        assert_eq!(report.effect(), engine::BatchEffect::Landed, "the whole recipe is one batch");
        assert_eq!(report.steps().len(), 3);
        assert_eq!(children(session.project(), product), 2, "both panels are there before the undo");
        session.undo().expect("one undo entry covers the whole recipe");
        assert_eq!(children(session.project(), product), 0, "one undo removes both panels");
        assert!(!session.can_undo(), "the recipe left exactly one entry");
    }

    #[test]
    fn a_dry_run_leaves_the_file_untouched() {
        let directory = scratch_dir("cli-recipe-dry");
        let (path, _root, product) = saved(&directory);
        let before = std::fs::read_to_string(&path).expect("the fixture reads");
        let recipe_path = directory.join("build.jsonl");
        std::fs::write(&recipe_path, recipe(product)).expect("the recipe is written");
        let args = RunArgs { scene: path.clone(), commands: recipe_path, out: None, product: None, dry_run: true };
        let lines = perform(&args).expect("the recipe runs");
        assert!(lines.last().is_some_and(|line| line.starts_with("dry run: 3 steps")), "{lines:?}");
        assert_eq!(std::fs::read_to_string(&path).expect("the fixture still reads"), before, "nothing was written");
    }

    #[test]
    fn an_out_path_is_written_instead_of_the_scene_read() {
        let directory = scratch_dir("cli-recipe-out");
        let (path, _root, product) = saved(&directory);
        let before = std::fs::read_to_string(&path).expect("the fixture reads");
        let recipe_path = directory.join("build.jsonl");
        std::fs::write(&recipe_path, recipe(product)).expect("the recipe is written");
        let out = directory.join("built.z3l");
        let args = RunArgs {
            scene: path.clone(),
            commands: recipe_path,
            out: Some(out.clone()),
            product: None,
            dry_run: false,
        };
        perform(&args).expect("the recipe runs");
        assert_eq!(std::fs::read_to_string(&path).expect("the scene still reads"), before, "the input is untouched");
        let built = SceneFile::read(&out).expect("the result is a scene");
        assert_eq!(children(built.document(), product), 2);
    }

    #[test]
    fn a_failing_step_names_its_line_and_writes_nothing() {
        let directory = scratch_dir("cli-recipe-fail");
        let (path, _root, product) = saved(&directory);
        let before = std::fs::read_to_string(&path).expect("the fixture reads");
        let recipe_path = directory.join("build.jsonl");
        let missing = Id::new(9_999).expect("an id");
        let text = format!("{}\n{}\n", panel_line(product, "Side"), panel_line(missing, "Ghost"));
        std::fs::write(&recipe_path, text).expect("the recipe is written");
        let args = RunArgs { scene: path.clone(), commands: recipe_path, out: None, product: None, dry_run: false };
        let error = perform(&args).expect_err("the second step cannot land");
        let message = error.to_string();
        assert!(
            message.starts_with("line 2 `panel.create`:"),
            "the sentence names the line and the command: {message}"
        );
        assert_eq!(std::fs::read_to_string(&path).expect("the scene still reads"), before, "nothing was written");
    }

    #[test]
    fn a_line_that_is_not_a_step_names_its_line() {
        for (text, expected) in [
            (r#"{"param":{}}"#, "is not a step key"),
            (r#"{"command":"node.rename","param":{}}"#, "is not a step key"),
            (r#"{"command":7}"#, "`command` is a command id"),
            (r#"{"command":"node.rename","params":[]}"#, "`params` is one JSON object"),
            ("[]", "a step is one JSON object"),
            ("not json", "not JSON"),
        ] {
            let error = parse(text).expect_err("the line is not a step");
            let message = error.to_string();
            assert!(message.starts_with("line 1:"), "{message}");
            assert!(message.contains(expected), "`{expected}` is missing from: {message}");
        }
    }

    #[test]
    fn a_blank_line_is_skipped_and_keeps_the_numbering() {
        let id = Id::new(1).expect("an id");
        let steps = parse(&format!("\n  \n{}\n", rename_line(id))).expect("one step");
        assert_eq!(steps.len(), 1);
        assert_eq!(steps.first().map(|step| step.line), Some(3), "the step keeps the line it came from");
        assert!(parse("").expect("an empty recipe parses").is_empty(), "an empty recipe holds no step");
    }

    #[test]
    fn a_run_without_a_recipe_names_the_missing_flag() {
        let error = crate::cli::parse(&words(&["run", "scene.z3l"])).expect_err("`--commands` is required");
        assert!(error.to_string().contains("--commands"), "{error}");
    }
}
