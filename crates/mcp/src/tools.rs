//! The four tools: what a client calls, what each takes, and what an answer is.
//!
//! A tool's arguments are typed structs that derive `serde` and `schemars`, and the schema a client
//! reads is derived from the struct the tool parses, so the two cannot drift apart — the same rule a
//! command's parameters follow (ADR 0185, ADR 0195). The names and the descriptions are English
//! prose because the reader is a model and not a person at the window: the interface's words come
//! from `text::Message`, and a sentence a client reads on the wire is the wire's own vocabulary, as
//! `CommandError`'s display already is.
//!
//! The tools are coarse and few on purpose. `command_list` is the discovery surface, `command_run`
//! is one step, `command_batch` is a list of steps that lands in one undo entry, and `query` is the
//! read model. Two tools per command id would be a hundred names for no gain.

use std::sync::Arc;

use engine::Query;
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
/// The tool that asks the scene a question and writes nothing.
pub const TOOL_QUERY: &str = "query";
/// The tool that reports the window's own state, which only an attached channel can answer.
pub const TOOL_INSPECT: &str = "ui_inspect";

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
    /// Dispatch the command and write nothing, reporting what would have landed.
    #[serde(default)]
    pub dry_run: bool,
}

impl RunRequest {
    /// The batch the run is: the same call with one step.
    ///
    /// `command_run` is not a second path through the engine — a run that took a different route
    /// could report a different undo entry than a batch of one (ADR 0185, ADR 0198).
    pub fn batch(&self) -> BatchRequest {
        BatchRequest {
            steps: vec![StepRequest { command: self.command.clone(), params: self.params.clone() }],
            atomic: true,
            dry_run: self.dry_run,
        }
    }
}

/// The arguments of `command_batch`: the steps, and how they land.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BatchRequest {
    /// The steps, dispatched in order.
    pub steps: Vec<StepRequest>,
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
}

/// One step of a batch: a command id and the parameters it takes.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StepRequest {
    /// The stable command id.
    pub command: String,
    /// The parameters the command takes; an empty object when it takes none.
    #[serde(default = "empty_params")]
    pub params: Value,
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

/// What one tool answered: the JSON the client reads, and whether the call failed.
///
/// A refusal is a result and not a protocol error: the command's own sentence is what a model can
/// act on, so it travels inside the answer with `failed` set, and the client still sees the report
/// of what did and did not land (ADR 0185).
#[derive(Clone, Debug, PartialEq)]
pub struct Answer {
    body: Value,
    failed: bool,
}

impl Answer {
    /// An answer that did what it was asked.
    pub const fn ok(body: Value) -> Self {
        Self { body, failed: false }
    }

    /// An answer that failed: a step refused, or the file could not be written.
    pub fn fail(body: Value) -> Self {
        Self { body, failed: true }
    }

    /// An answer that carries one sentence and nothing else, which is a refusal.
    pub fn refused(sentence: impl Into<String>) -> Self {
        Self::fail(json!({ "error": sentence.into() }))
    }

    /// The JSON the client reads.
    pub const fn body(&self) -> &Value {
        &self.body
    }

    /// Whether the call failed: a command refused, a step did not land, a save did not happen.
    pub const fn failed(&self) -> bool {
        self.failed
    }
}

/// The tool list a headless session offers: the four coarse tools and no window report.
pub fn definitions() -> Vec<Tool> {
    headless_definitions()
}

/// The four coarse tools, with the schema each one's own argument type publishes.
///
/// A schema is built per call, the way a command's own is: discovery is not the frame path, and a
/// value that lived in a `static` would have to be built at startup for a client that may never
/// connect.
pub fn headless_definitions() -> Vec<Tool> {
    vec![
        Tool::new(
            TOOL_LIST,
            "List every command this build can dispatch: the stable id, the English label, what it \
             creates, the JSON Schema of the parameters it takes, and whether the command can run in \
             this session — with the sentence that says why not when it cannot. Call this first: the \
             ids and the schemas a caller needs come from here.",
            object_schema::<CommandListRequest>(),
        ),
        Tool::new(
            TOOL_RUN,
            "Dispatch exactly one command by its stable id. The document change is one undo entry. \
             With `dry_run` the command runs and nothing is written, so the answer is the plan.",
            object_schema::<RunRequest>(),
        ),
        Tool::new(
            TOOL_BATCH,
            "Dispatch a list of commands as one operation: in order, and by default all or nothing, \
             so a refusal takes back every step before it and the whole list is one undo entry. The \
             answer reports which nodes the batch created, changed and removed, and what a file \
             write did. With `dry_run` every step runs and nothing is written. A session in plan \
             mode answers every batch this way whatever the request says; `command_list` names the \
             mode.",
            object_schema::<BatchRequest>(),
        ),
        Tool::new(
            TOOL_QUERY,
            "Ask the scene one question and write nothing: `tree`, `box`, `parts`, `diagnostics` or \
             `niche`. The answer is the report the command line prints, so a caller and the window \
             cannot disagree about the document.",
            object_schema::<Query>(),
        ),
    ]
}

/// The tool list an attached channel offers: the headless four and the window's own report.
///
/// The list is built once per `tools/list`, and the five names are the whole promise: a caller that
/// reads this list learns the same five tools on either kind of window.
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
            assert!(tools.len() >= 4, "the four coarse tools");
            for tool in &tools {
                // A tool takes one object, or — `query` — one of the five question objects the engine's
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
        assert_eq!(names, vec![TOOL_LIST, TOOL_RUN, TOOL_BATCH, TOOL_QUERY]);
        let offered = attached_definitions();
        let attached: Vec<&str> = offered.iter().map(|tool| tool.name.as_ref()).collect();
        assert_eq!(
            attached,
            vec![TOOL_LIST, TOOL_RUN, TOOL_BATCH, TOOL_QUERY, TOOL_INSPECT],
            "an attached channel adds the window's own report to the same four"
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
        let request = RunRequest { command: "node.rename".to_owned(), params: json!({"node": 1}), dry_run: true };
        let batch = request.batch();
        assert_eq!(batch.steps.len(), 1);
        assert_eq!(batch.steps.first().map(|step| step.command.as_str()), Some("node.rename"));
        assert!(batch.atomic, "one step is whole by definition");
        assert!(batch.dry_run, "the run's own flag travels with it");
    }
}
