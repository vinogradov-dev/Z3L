//! The tools: what a client calls, what each takes, and what an answer is.
//!
//! A tool's arguments are typed structs that derive `serde` and `schemars`, and the schema a client
//! reads is derived from the struct the tool parses, so the two cannot drift apart — the same rule a
//! command's parameters follow (ADR 0185, ADR 0195). The names and the descriptions are English
//! prose because the reader is a model and not a person at the window: the interface's words come
//! from `text::Message`, and a sentence a client reads on the wire is the wire's own vocabulary, as
//! `CommandError`'s display already is.
//!
//! The tools are coarse and few on purpose. `command_list` is the discovery surface, `command_run`
//! is one step, `command_batch` is a list of steps that lands in one undo entry, `undo` takes one
//! entry back, `query` is the read model, `inspect` is the mistakes the rules prove and
//! `render_preview` is the scene as one picture. Two tools per command id would be a hundred names
//! for no gain.

use std::sync::Arc;

use engine::Query;
use model::Id;
use rmcp::model::{JsonObject, Tool};
use schemars::JsonSchema;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

/// The tool that lists what this build can dispatch.
pub const TOOL_LIST: &str = "command_list";
/// The tool that dispatches exactly one command.
pub const TOOL_RUN: &str = "command_run";
/// The tool that dispatches a list of commands as one operation.
pub const TOOL_BATCH: &str = "command_batch";
/// The tool that takes back the last undo entry.
pub const TOOL_UNDO: &str = "undo";
/// The tool that asks the scene a question and writes nothing.
pub const TOOL_QUERY: &str = "query";
/// The tool that reports the mistakes the rules can prove, and writes nothing.
pub const TOOL_FINDINGS: &str = "inspect";
/// The tool that reports the window's own state, which only an attached channel can answer.
pub const TOOL_INSPECT: &str = "ui_inspect";
/// The tool that draws the scene and answers the picture, which only a process with a device can.
pub const TOOL_PREVIEW: &str = "render_preview";

/// The arguments of `render_preview`: the size of the picture.
///
/// Both sides are pixels and both are bounded; a call that names no size asks for the default, and a
/// size outside the bounds is refused with the bound in the sentence rather than clamped into a
/// picture the caller did not ask for (ADR 0228).
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PreviewRequest {
    /// The picture's width in pixels, from 64 to 1600.
    #[serde(default = "default_width")]
    pub width: u32,
    /// The picture's height in pixels, from 64 to 1600.
    #[serde(default = "default_height")]
    pub height: u32,
}

impl Default for PreviewRequest {
    fn default() -> Self {
        Self { width: default_width(), height: default_height() }
    }
}

/// The width a call that names none asks for: 4:3, which is the viewport's own shape.
const fn default_width() -> u32 {
    640
}

/// The height a call that names none asks for.
const fn default_height() -> u32 {
    480
}

/// The arguments of `inspect`: none.
///
/// The rules read the document, the catalogue and the profile the session already holds, so a
/// caller has nothing to pass and a mistyped key is refused instead of ignored.
#[derive(Clone, Copy, Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FindingsRequest {}

/// The arguments of `undo`: none.
///
/// Taking back the last entry is one act with no parameter: which entry it is is the history's
/// answer, and a caller that wants two entries back calls it twice.
#[derive(Clone, Copy, Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UndoRequest {}

/// The arguments of `command_list`: none.
///
/// An empty object rather than no arguments at all, so a client that sends `{}` and one that sends
/// nothing are the same call, and a misspelt key is refused instead of ignored.
#[derive(Clone, Copy, Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CommandListRequest {}

/// The arguments of `command_run`: one command, with the parameters its schema describes.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunRequest {
    /// The stable command id, as `command_list` prints it.
    pub command: String,
    /// The parameters the command takes; an empty object when the command takes none.
    #[serde(default = "empty_params")]
    pub params: Value,
    /// The product the command works in; the open one when it is absent.
    ///
    /// A furniture part is born only inside the open product (ADR 0098), and opening one is session
    /// view state: it moves no revision and leaves no undo entry, so naming a product is not a step.
    #[serde(default)]
    pub product: Option<Id>,
    /// Dispatch the command and write nothing, reporting what would have landed.
    #[serde(default)]
    pub dry_run: bool,
    /// Write the scene after the command lands, so the file holds what it did.
    ///
    /// The field a batch carries, said once here so a run and a batch of one are the same call
    /// (ADR 0185, ADR 0198).
    #[serde(default)]
    pub save: bool,
}

impl RunRequest {
    /// The batch the run is: the same call with one step.
    ///
    /// `command_run` is not a second path through the engine — a run that took a different route
    /// could report a different undo entry than a batch of one (ADR 0185, ADR 0198).
    pub fn batch(&self) -> BatchRequest {
        BatchRequest {
            steps: vec![StepRequest::new(self.command.clone(), self.params.clone())],
            product: self.product,
            atomic: true,
            dry_run: self.dry_run,
            save: self.save,
        }
    }
}

/// The arguments of `command_batch`: the steps, and how they land.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BatchRequest {
    /// The steps, dispatched in order.
    pub steps: Vec<StepRequest>,
    /// The product the batch works in; the open one when it is absent.
    ///
    /// A furniture part is born only inside the open product (ADR 0098), and opening one is session
    /// view state: it moves no revision and leaves no undo entry, so naming a product is not a step
    /// of the batch and a refusal does not take it back. A scene with two products and none named
    /// leaves the workspace as it was, and the first step that needs a product refuses in its own
    /// words.
    #[serde(default)]
    pub product: Option<Id>,
    /// Keep the batch whole: a refusal takes back every step before it.
    ///
    /// The default is the command line's recipe and a chat turn's promise, one operation or none.
    /// `false` keeps the steps that ran, which is what a caller measuring one after another asks
    /// for; the answer says which of the two happened.
    #[serde(default = "yes")]
    pub atomic: bool,
    /// Dispatch every step and write nothing, reporting what would have landed.
    #[serde(default)]
    pub dry_run: bool,
    /// Write the scene after the batch lands, so the file holds what the batch did.
    ///
    /// The document a person is looking at is written only when the caller asks: a landed batch
    /// leaves the tab dirty and the person's own *Save* is the write, because a file nobody asked
    /// for could overwrite work they have not saved yet (ADR 0005, X114). The save goes to the file
    /// the scene was opened from or last saved to, and a scene that has none refuses with the
    /// sentence that names the person's act — a caller never chooses a path (ADR 0188).
    ///
    /// A headless session (`z3l mcp serve <scene>`) is written on every landed batch whether this is
    /// set or not: naming the file when the process started is the same ask, made once.
    #[serde(default)]
    pub save: bool,
}

/// One step of a batch: a command id, the parameters it takes, and the name a later step calls what
/// it made.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StepRequest {
    /// The stable command id.
    pub command: String,
    /// The parameters the command takes; an empty object when it takes none.
    #[serde(default = "empty_params")]
    pub params: Value,
    /// The name this step gives the node it creates, so a later step of the same batch can refer to
    /// it as `{"$ref": "<name>"}` in place of an id it cannot know yet.
    ///
    /// A name belongs to the batch, not to the document: it is resolved as the batch runs and never
    /// serialized. A step that creates no node may be named and cannot be referred to, and a name a
    /// second step takes is refused rather than shadowing the first.
    #[serde(rename = "as", default)]
    pub name: Option<String>,
}

impl StepRequest {
    /// A step that dispatches `command` with `params` and names nothing.
    pub fn new(command: impl Into<String>, params: Value) -> Self {
        Self { command: command.into(), params, name: None }
    }

    /// The same step under `name`, which a later step of the batch refers to.
    pub fn named(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
}

/// The empty parameter object a step with no parameters carries.
///
/// An omitted `params` is `{}` and not `null`: a null would reach the command's own parser and come
/// back as "invalid type: null", which says nothing about the command (the command line's recipe
/// reads a missing `params` the same way).
fn empty_params() -> Value {
    Value::Object(JsonObject::new())
}

/// The default of a switch that is on, spelled out because `Default` is not `true`.
const fn yes() -> bool {
    true
}

/// What one tool answered: the JSON the client reads, whether the call failed, and the picture when
/// the tool drew one.
///
/// A refusal is a result and not a protocol error: the command's own sentence is what a model can
/// act on, so it travels inside the answer with `failed` set, and the client still sees the report
/// of what did and did not land (ADR 0185). The image is a block of its own beside the JSON, because
/// the protocol has one and a client draws it without decoding our answer (ADR 0228).
#[derive(Clone, Debug, PartialEq)]
pub struct Answer {
    body: Value,
    failed: bool,
    image: Option<Vec<u8>>,
}

impl Answer {
    /// An answer that did what it was asked.
    pub const fn ok(body: Value) -> Self {
        Self { body, failed: false, image: None }
    }

    /// An answer that failed: a step refused, or the file could not be written.
    pub fn fail(body: Value) -> Self {
        Self { body, failed: true, image: None }
    }

    /// An answer that carries one sentence and nothing else, which is a refusal.
    pub fn refused(sentence: impl Into<String>) -> Self {
        Self::fail(json!({ "error": sentence.into() }))
    }

    /// The same answer with a PNG beside its report.
    pub fn with_image(mut self, png: Vec<u8>) -> Self {
        self.image = Some(png);
        self
    }

    /// The JSON the client reads.
    pub const fn body(&self) -> &Value {
        &self.body
    }

    /// The picture the answer carries, when the tool drew one.
    pub fn image(&self) -> Option<&[u8]> {
        self.image.as_deref()
    }

    /// Whether the call failed: a command refused, a step did not land, a save did not happen.
    pub const fn failed(&self) -> bool {
        self.failed
    }

    /// Whether the answer says the file holds the batch it reports.
    ///
    /// The wire's own word, read back where the file was written for the person: a save a caller
    /// asked for is their *Save*, performed on the document in front of them, so the shell clears
    /// the dirty marker on this answer and on nothing else (X114).
    pub fn written(&self) -> bool {
        self.body.get("written").and_then(Value::as_bool).unwrap_or(false)
    }
}

/// The tool list a headless session offers: the coarse tools and no window report.
pub fn definitions() -> Vec<Tool> {
    headless_definitions()
}

/// The coarse tools, with the schema each one's own argument type publishes.
///
/// A schema is built per call, the way a command's own is: discovery is not the frame path, and a
/// value that lived in a `static` would have to be built at startup for a client that may never
/// connect.
pub fn headless_definitions() -> Vec<Tool> {
    vec![
        Tool::new(
            TOOL_LIST,
            "List every command this build can dispatch: the stable id, the English label, the world \
             it serves (`furniture`, `geometry`, `tree`, `scene`), what it creates, the JSON Schema of \
             the parameters it takes, and whether the command can run in this session — with the \
             sentence that says why not when it cannot. The same answer carries `domain`: the guide a \
             caller reads before it composes anything, with the two worlds of the program, the order of \
             a build from scratch, the shapes a product frame takes, the stances of a panel, the \
             semantic classes with the words a person types for them, the units and the sides. Call \
             this first: the ids, the schemas and the domain come from here.",
            object_schema::<CommandListRequest>(),
        ),
        Tool::new(
            TOOL_RUN,
            "Dispatch exactly one command by its stable id. The document change is one undo entry. \
             With `product` the command works in that product, and naming it opens the product first: \
             a furniture part is born only inside the open product. Opening is a view operation, not \
             an undo entry. With `dry_run` the command runs and nothing is written, so the answer is \
             the plan. With `save` the scene is written to the file it was opened from once the \
             command lands; without it the window's document is left unsaved, and a scene that has no \
             path yet refuses the save instead of writing a new file.",
            object_schema::<RunRequest>(),
        ),
        Tool::new(
            TOOL_BATCH,
            "Dispatch a list of commands as one operation: in order, and by default all or nothing, \
             so a refusal takes back every step before it and the whole list is one undo entry. A step \
             may name what it creates with `as`, and a later step may refer to that name as \
             {\"$ref\": \"<name>\"} wherever the command's schema wants an id, so one build is one \
             call and one undo entry. The answer reports which nodes the batch created, changed and \
             removed, what a file write did, and the `open_product` it ran in. With `product` the \
             batch works in that product, and naming it opens the product first: a furniture part is \
             born only inside the open product, and opening is a view operation rather than a step. \
             With `dry_run` every step runs and nothing is written. A session in plan mode answers \
             every batch this way whatever the request says; `command_list` names the mode. Plan and \
             build are a person's switch, held by the session: no tool changes it, and a plan answer \
             carries the sentence that names the panel and the key in its `mode_note`. With `save` \
             the scene is written to the file it was opened from once the batch lands, which is how \
             an agent's build survives the window closing; without it the window's document stays \
             unsaved and its own Save is the write. A scene that has no path yet refuses the save \
             with the sentence that names the person's act, because a caller never chooses a path.",
            object_schema::<BatchRequest>(),
        ),
        Tool::new(
            TOOL_UNDO,
            "Take back the last undo entry of this session, so a turn that landed something wrong is 
             corrected instead of pushed further: the document returns to exactly what it held before 
             that entry, and the answer reports the revision it now holds and whether another entry is 
             left to take back. One message of a chat turn is one entry (ADR 0187), so this takes back 
             a whole message rather than one call of it. A session in plan mode refuses: plan and 
             build are the switch a person turns in the assistant panel, and undoing is a write. A 
             session with nothing to undo refuses with that sentence.",
            object_schema::<UndoRequest>(),
        ),
        Tool::new(
            TOOL_QUERY,
            "Ask the scene one question and write nothing: `tree`, `box`, `bounds`, `parts`, \
             `diagnostics` or `niche`. The answer is the report the command line prints, so a caller \
             and the window cannot disagree about the document.",
            object_schema::<Query>(),
        ),
        Tool::new(
            TOOL_FINDINGS,
            "Inspect the scene for the seven mistakes the rules can prove: a part that leaves its \
             product's envelope, two bodies that share volume, a shelf that does not fit the opening \
             between its uprights, a hanging rail in the wrong place, a visible edge with no banding \
             or a hidden one that carries it, a front whose gap or width is wrong, and a part larger \
             than the sheet it is cut from. The answer is one list ordered by severity then code: \
             every finding names the document ids it blames, says in one sentence what a person \
             would see, and carries the commands a fix would dispatch (an empty list when the fix is \
             a decision rather than a command). The pass writes nothing and moves no revision. Read \
             this before explaining a scene: the rules find the mistake and the model explains it.",
            object_schema::<FindingsRequest>(),
        ),
        Tool::new(
            TOOL_PREVIEW,
            "Draw the scene and answer the picture: one PNG of what this session holds, from the \
             document's own camera when it has one and otherwise framed to fit the whole scene. The \
             arguments are `width` and `height` in pixels, each from 64 to 1600, and the default is \
             640 by 480; a size outside the bounds is refused with the bound in the sentence, and a \
             picture that encodes to more than 4 MiB is refused rather than sent. The answer carries \
             the image as a block of its own beside a JSON report of the width, height, byte count \
             and revision it drew — a picture is what a person would look at, so look at it before \
             describing what you built. The call writes nothing and moves no revision.",
            object_schema::<PreviewRequest>(),
        ),
    ]
}

/// The tool list an attached channel offers: the headless tools and the window's own report.
///
/// The list is built once per `tools/list`, and the names are the whole promise: a caller that reads
/// this list learns the same tools on either kind of window, with `ui_inspect` added for the one a
/// person is looking at.
pub fn attached_definitions() -> Vec<Tool> {
    let mut tools = headless_definitions();
    tools.push(Tool::new(
        TOOL_INSPECT,
        "Report the window a person is looking at: which scenes are open, which one is in front, its \
         path, whether it holds unsaved changes and its revision, the armed transform gesture, the \
         selected nodes and the sentence the status bar shows. Call this before dispatching: a \
         command lands in the active scene, and this says which one that is.",
        object_schema::<InspectRequest>(),
    ));
    tools
}

/// The arguments of `ui_inspect`: none.
///
/// An empty object rather than no arguments at all, so a client that sends `{}` and one that sends
/// nothing are the same call, and a missed keystroke is refused instead of ignored.
#[derive(Clone, Copy, Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InspectRequest {}

/// The JSON Schema of one tool's arguments, as the object the protocol carries.
fn object_schema<T: JsonSchema>() -> Arc<JsonObject> {
    match schemars::schema_for!(T).to_value() {
        Value::Object(schema) => Arc::new(schema),
        // A struct's schema is an object by construction; a caller that reaches this arm would be
        // reading an empty schema, which the protocol allows and no test can construct.
        _ => Arc::new(JsonObject::new()),
    }
}

/// Reads a tool's arguments into the type its schema describes.
///
/// A malformed request is the caller's mistake and says so: the sentence names the tool and the
/// parser's own reading of the field, because the alternative is a command dispatched with
/// parameters nobody meant.
pub fn parse<T: DeserializeOwned>(tool: &str, arguments: Option<JsonObject>) -> Result<T, Answer> {
    let value = match arguments {
        Some(arguments) => Value::Object(arguments),
        None => Value::Object(JsonObject::new()),
    };
    serde_json::from_value(value)
        .map_err(|error| Answer::refused(format!("`{tool}` cannot read its arguments: {error}")))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    #[test]
    fn every_tool_publishes_a_schema_that_denies_a_key_it_does_not_read() {
        for tools in [headless_definitions(), attached_definitions()] {
            assert!(tools.len() >= 6, "the coarse tools");
            assert!(
                tools.iter().all(|tool| {
                    let schema = Value::Object((*tool.input_schema).clone());
                    schema.get("type").and_then(Value::as_str) == Some("object") || schema.get("oneOf").is_some()
                }),
                "every tool takes one object"
            );
            for tool in &tools {
                // A tool takes one object, or — `query` — one of the question objects the engine's
                // own enum describes. Either way every shape it accepts denies an unknown field, which
                // is the promise the parser keeps.
                let schema = Value::Object((*tool.input_schema).clone());
                let variants: Vec<&Value> = match schema.get("oneOf").and_then(Value::as_array) {
                    Some(variants) => variants.iter().collect(),
                    None => vec![&schema],
                };
                assert!(!variants.is_empty(), "{} describes no shape", tool.name);
                for shape in variants {
                    assert_eq!(shape.get("type").and_then(Value::as_str), Some("object"), "{}", tool.name);
                    assert_eq!(
                        shape.get("additionalProperties").and_then(Value::as_bool),
                        Some(false),
                        "{} refuses a key it does not read",
                        tool.name
                    );
                }
                assert!(
                    tool.description.as_ref().is_some_and(|text| !text.trim().is_empty()),
                    "{} says what it does",
                    tool.name
                );
            }
        }
        let headless = headless_definitions();
        let names: Vec<&str> = headless.iter().map(|tool| tool.name.as_ref()).collect();
        assert_eq!(names, vec![TOOL_LIST, TOOL_RUN, TOOL_BATCH, TOOL_UNDO, TOOL_QUERY, TOOL_FINDINGS, TOOL_PREVIEW]);
        let offered = attached_definitions();
        let attached: Vec<&str> = offered.iter().map(|tool| tool.name.as_ref()).collect();
        assert_eq!(
            attached,
            vec![TOOL_LIST, TOOL_RUN, TOOL_BATCH, TOOL_UNDO, TOOL_QUERY, TOOL_FINDINGS, TOOL_PREVIEW, TOOL_INSPECT],
            "an attached channel adds the window's own report to the same tools"
        );
    }

    #[test]
    fn a_run_names_the_size_of_the_picture_it_asks_for() {
        let asked =
            serde_json::from_value::<JsonObject>(json!({ "width": 320, "height": 200 })).expect("an arguments object");
        let asked: PreviewRequest = parse(TOOL_PREVIEW, Some(asked)).expect("both sides parse");
        assert_eq!((asked.width, asked.height), (320, 200));
        let default = parse::<PreviewRequest>(TOOL_PREVIEW, None).expect("a call that names no size parses");
        assert_eq!((default.width, default.height), (640, 480), "a preview has a default shape");
        let misspelt = serde_json::from_value::<JsonObject>(json!({ "widht": 320 })).expect("an arguments object");
        parse::<PreviewRequest>(TOOL_PREVIEW, Some(misspelt))
            .expect_err("a misspelt side is refused instead of ignored");
    }

    #[test]
    fn a_step_names_what_it_creates_and_a_later_step_refers_to_the_name() {
        let arguments: JsonObject = serde_json::from_value(json!({
            "steps": [
                {"command": "panel.create", "as": "side", "params": {"parent": 2}},
                {"command": "panel.edge_banding.set", "params": {"panel": {"$ref": "side"}}},
            ]
        }))
        .expect("an object of arguments");
        let request: BatchRequest = parse(TOOL_BATCH, Some(arguments)).expect("two steps");
        assert_eq!(request.steps.first().and_then(|step| step.name.as_deref()), Some("side"));
        assert_eq!(request.steps.get(1).and_then(|step| step.name.as_deref()), None);
        assert_eq!(
            request.steps.get(1).map(|step| step.params.clone()),
            Some(json!({"panel": {"$ref": "side"}})),
            "a reference is a value of the parameters, and the tool hands it on untouched"
        );
    }

    #[test]
    fn a_step_without_parameters_carries_an_empty_object() {
        let arguments: JsonObject =
            serde_json::from_value(json!({"steps": [{"command": "node.rename"}]})).expect("an object of arguments");
        let request: BatchRequest = parse(TOOL_BATCH, Some(arguments)).expect("the step names a command");
        let step = request.steps.first().expect("one step");
        assert_eq!(step.command, "node.rename");
        assert_eq!(step.params, json!({}), "an omitted `params` is an empty object, not null");
        assert!(request.atomic, "a batch is all or nothing unless the caller says otherwise");
        assert!(!request.dry_run);
    }

    #[test]
    fn a_key_the_tool_does_not_read_is_refused_with_its_name() {
        let arguments: JsonObject =
            serde_json::from_value(json!({"command": "node.rename", "param": {}})).expect("an object of arguments");
        let refusal = parse::<RunRequest>(TOOL_RUN, Some(arguments)).expect_err("`param` is not a key of a run");
        let message = refusal.body().get("error").and_then(Value::as_str).expect("a sentence");
        assert!(message.starts_with("`command_run` cannot read its arguments:"), "{message}");
        assert!(message.contains("param"), "the sentence names the key: {message}");
        assert!(refusal.failed());
    }

    #[test]
    fn a_run_is_a_batch_of_one_step() {
        let product = Id::new(2);
        let request = RunRequest {
            command: "node.rename".to_owned(),
            params: json!({"node": 1}),
            product,
            dry_run: true,
            save: true,
        };
        let batch = request.batch();
        assert_eq!(batch.steps.len(), 1);
        assert_eq!(batch.steps.first().map(|step| step.command.as_str()), Some("node.rename"));
        assert!(batch.atomic, "one step is whole by definition");
        assert!(batch.dry_run, "the run's own flag travels with it");
        assert!(batch.save, "and so does the save it asked for");
        assert_eq!(batch.product, product, "the product the run named travels with it");
    }
}
