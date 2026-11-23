//! The assistant's outbound wire: one OpenAI-compatible completion per call, and the key's homes.
//!
//! This is the adapter ADR 0189 decides: `POST {base_url}/chat/completions` with the model's name as
//! data, so a server on this machine and a provider on the internet are the same client pointed at a
//! different URL. No provider name appears in a type, a match arm or a default.
//!
//! Three promises the shape keeps, each with a test:
//!
//! - **A frame never waits.** [`OpenAi::ask`] spawns one thread for the request and returns; the
//!   answer arrives through a channel that [`OpenAi::take`] reads without blocking, so the window
//!   keeps drawing while the model thinks (ADR 0187's `ask`/`take`, ADR 0189's "blocking and small").
//! - **The key is not in the tree.** It comes from `Z3L_AGENT_KEY`, else from `agent-key` beside the
//!   settings file with no group or other bits; it never enters a log line, a refusal, a chat line
//!   or `settings.json`, and [`Endpoint`]'s `Debug` prints a marker instead of it.
//! - **The wire's batch is narrower than the server's.** The acting tool takes `steps` and nothing
//!   else: `dry_run` is the session's mode (ADR 0190), `save` is the person's own *Save*, and the
//!   product is the one the window has open. A cloud model chooses none of the three.
//!
//! The consent to send anything at all is the shell's, because it is a workspace setting: this crate
//! knows an endpoint, not whether the person agreed to it (ADR 0189).
//!
//! Changes when: the wire, the key's homes or the tool surface change.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Duration;

use engine::{BatchStep, CommandRegistry};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use text::Lang;

use crate::chat::{BackendError, ChatBackend, Conversation, Reply, Speaker};
use crate::tools::{StepRequest, TOOL_BATCH};

/// The environment variable a key is read from first.
pub const KEY_ENV: &str = "Z3L_AGENT_KEY";

/// The file a key is read from second, in the folder the settings file lives in.
pub const KEY_FILE: &str = "agent-key";

/// The tool a model calls to make one batch of changes.
///
/// It is [`TOOL_BATCH`]'s id on purpose: one act has one name on both doors, and only its arguments
/// differ (see [`BatchCall`]).
pub const BATCH_TOOL: &str = TOOL_BATCH;

/// The tool a model calls to publish the steps it is working through.
pub const CHECKLIST_TOOL: &str = "todo_write";

/// How long one request may take before the turn is told the endpoint stopped answering.
///
/// A completion on a large model is minutes, not seconds; a deadline that fires during a good answer
/// is worse than waiting, so the bound is generous and the person's Stop is the fast way out.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(180);

/// One endpoint: where a completion is asked for and which model answers it.
///
/// The key is deliberately not a field. It is read from its two homes when a message is sent, by
/// [`OpenAi::ask`], so a key written while the window runs is picked up by the next message and no
/// frame ever reads the file; a value that cannot be printed cannot be printed by accident.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    base_url: String,
    model: String,
}

impl Endpoint {
    /// An endpoint, as the two settings made it.
    pub fn new(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        Self { base_url: base_url.into(), model: model.into() }
    }

    /// The base URL a request is sent under, as the settings carry it (`https://host/v1`).
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// The model name the request asks for.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// The URL one completion is posted to: the base URL with one trailing slash removed.
    pub fn chat_url(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
    }

    /// Whether the endpoint names both parts of a request, so a turn can be asked at all.
    pub fn is_configured(&self) -> bool {
        !self.base_url.trim().is_empty() && !self.model.trim().is_empty()
    }
}

/// The key a request carries, from its first home or its second.
///
/// `Z3L_AGENT_KEY` wins; a file beside the settings is the second home, because a person who runs one
/// shell may prefer a file to an export. A file other users can read is refused rather than used
/// (ADR 0189): the sentence names the mode to set and never the key.
pub fn key(folder: Option<&Path>) -> Result<String, BackendError> {
    key_from(std::env::var(KEY_ENV).ok(), folder)
}

/// [`key`] with the environment handed in, so a test can drive both homes without touching a
/// process-global variable (this crate forbids `unsafe`, and `set_var` is unsafe in edition 2024).
pub fn key_from(environment: Option<String>, folder: Option<&Path>) -> Result<String, BackendError> {
    if let Some(value) = environment.map(|value| value.trim().to_owned()).filter(|value| !value.is_empty()) {
        return Ok(value);
    }
    let Some(folder) = folder else {
        return Err(no_key());
    };
    let path = folder.join(KEY_FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Err(no_key()),
        Err(error) => {
            return Err(BackendError::new(format!("`{}` cannot be read: {error}", path.display())));
        }
    };
    let value = text.trim();
    if value.is_empty() {
        return Err(no_key());
    }
    if others_can_read(&path) {
        return Err(BackendError::new(format!(
            "`{}` is readable by other users: `chmod 600` it, or set `{KEY_ENV}`",
            path.display()
        )));
    }
    Ok(value.to_owned())
}

/// The sentence a workspace with no key reads, which names both homes and no secret.
fn no_key() -> BackendError {
    BackendError::new(format!(
        "the assistant has no key: set `{KEY_ENV}`, or write `{KEY_FILE}` with mode 0600 beside the settings"
    ))
}

/// Whether a file's permissions let someone other than its owner read it.
///
/// A permission check is a claim about a Unix mode; the other platforms carry an ACL this tree does
/// not inspect, so the file is trusted there and the environment stays the first home everywhere. A
/// file that cannot be stat'ed is not read either: the caller sees the read error instead.
#[cfg(unix)]
fn others_can_read(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|metadata| metadata.permissions().mode() & 0o077 != 0)
}

/// [`others_can_read`] on a platform without Unix modes: the file is trusted.
#[cfg(not(unix))]
fn others_can_read(_path: &Path) -> bool {
    false
}

/// The model behind the panel, on the OpenAI-compatible wire.
pub struct OpenAi {
    endpoint: Endpoint,
    /// The settings folder the key's second home sits in, when the workspace has one.
    folder: Option<PathBuf>,
    // Never printed: `ureq::Agent` has no useful `Debug` and a test that names the backend should
    // read the endpoint's own shape.
    agent: ureq::Agent,
    answer: Option<Receiver<Result<Reply, BackendError>>>,
}

/// The backend as a test's failure output and a log line read it: the endpoint, and no key.
impl fmt::Debug for OpenAi {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("OpenAi").field("endpoint", &self.endpoint).finish_non_exhaustive()
    }
}

impl OpenAi {
    /// A backend for `endpoint`, with the one agent every request of this session shares.
    pub fn new(endpoint: Endpoint, folder: Option<PathBuf>) -> Self {
        let config = ureq::Agent::config_builder().timeout_global(Some(REQUEST_TIMEOUT)).build();
        Self { endpoint, folder, agent: ureq::Agent::new_with_config(config), answer: None }
    }

    /// The endpoint this backend asks, for the shell's consent and the panel's words.
    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }
}

impl ChatBackend for OpenAi {
    fn ask(&mut self, transcript: &Conversation) {
        // The key is read here, once per message: a file nobody reads on a frame, and a key that is
        // never held by the backend while no message is being asked.
        let key = match key(self.folder.as_deref()) {
            Ok(key) => key,
            Err(error) => {
                self.answer = Some(ready(Err(error)));
                return;
            }
        };
        let body = request_body(&self.endpoint, transcript);
        let agent = self.agent.clone();
        let endpoint = self.endpoint.clone();
        let (sender, answer) = mpsc::channel();
        let spawned = std::thread::Builder::new().name("agent-request".to_owned()).spawn(move || {
            // A send fails only when the turn dropped the request it replaces; the answer is then
            // nobody's, which is exactly what the person's stop asked for.
            let _ = sender.send(complete(&agent, &endpoint, &key, &body));
        });
        self.answer = Some(match spawned {
            Ok(_) => answer,
            // A thread that never started answers nothing, so the turn is handed the sentence now
            // instead of waiting for a frame that never comes.
            Err(error) => ready(Err(BackendError::new(format!("the assistant cannot start a request: {error}")))),
        });
    }

    fn take(&mut self) -> Option<Result<Reply, BackendError>> {
        let answer = self.answer.as_ref()?;
        match answer.try_recv() {
            Ok(result) => {
                self.answer = None;
                Some(result)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.answer = None;
                Some(Err(BackendError::new("the assistant's request ended without an answer")))
            }
        }
    }

    fn waiting(&self) -> bool {
        self.answer.is_none()
    }
}

/// A channel that already holds one answer, so a request that never started still answers.
fn ready(result: Result<Reply, BackendError>) -> Receiver<Result<Reply, BackendError>> {
    let (sender, answer) = mpsc::channel();
    let _ = sender.send(result);
    answer
}

/// The arguments of the wire's `command_batch`: the steps of one answer, in order.
///
/// Narrower than the served tool's [`crate::BatchRequest`] by design, and the doc comment below is
/// what a model reads: the three options the server accepts are not a model's to choose.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BatchCall {
    /// The steps of the answer, in the order they must run. One call is one undo entry.
    pub steps: Vec<StepRequest>,
}

/// The arguments of the wire's `todo_write`: the steps the model is working through.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChecklistCall {
    /// The checklist, oldest step first, in the model's own words.
    pub items: Vec<String>,
}

/// The request body of one completion: the system message, the transcript and the tools.
fn request_body(endpoint: &Endpoint, transcript: &Conversation) -> Value {
    let mut messages = vec![json!({ "role": "system", "content": system_message() })];
    messages.extend(transcript.lines().iter().map(line_message));
    json!({
        "model": endpoint.model(),
        "messages": messages,
        "tools": tools(),
        "tool_choice": "auto",
    })
}

/// One line of the conversation as the wire's message with the same content.
///
/// The two speakers that are the program's own reports travel as `user` turns that say so, because
/// the wire has three roles and the reports are neither the person nor the model speaking.
fn line_message(line: &crate::chat::Line) -> Value {
    let role = match line.speaker {
        Speaker::Agent => "assistant",
        Speaker::Person | Speaker::Tool | Speaker::Note => "user",
    };
    let content = match line.speaker {
        Speaker::Person | Speaker::Agent => line.text.clone(),
        Speaker::Tool => format!("The program reports a tool call: {}", line.text),
        Speaker::Note => format!("The program reports: {}", line.text),
    };
    json!({ "role": role, "content": content })
}

/// The two tools a model may call, with the schema each one's own argument type publishes.
fn tools() -> Value {
    json!([
        {
            "type": "function",
            "function": {
                "name": BATCH_TOOL,
                "description": "Dispatch the steps of one answer as one batch and one undo entry, in \
                                order. A step names a stable command id and its parameters; a step \
                                that creates a node may be named with `as`, and a later step refers to \
                                it as {\"$ref\": \"<name>\"}. Every step runs, or a refusal takes the \
                                whole batch back. Whether the document is written is the person's own \
                                mode and not an argument: there is no `dry_run` and no `save` here.",
                "parameters": schema::<BatchCall>(),
            }
        },
        {
            "type": "function",
            "function": {
                "name": CHECKLIST_TOOL,
                "description": "Publish the steps you are working through, so the person watches the \
                                plan instead of guessing it. It changes no document: the list is chat \
                                state and is never undone.",
                "parameters": schema::<ChecklistCall>(),
            }
        },
    ])
}

/// One type's JSON Schema as the wire publishes it.
fn schema<T: JsonSchema>() -> Value {
    schemars::schema_for!(T).to_value()
}

/// What a model reads before its first call: the program, the plan, and the ids it may dispatch.
///
/// The domain is [`engine::guide`] — the same value `command_list` answers with, so the words cannot
/// drift between the two doors — and the ids come from the one registry, with one label each: the
/// whole catalogue is a few hundred tokens, where `command_list`'s schemas are not (J15).
fn system_message() -> String {
    let ids: serde_json::Map<String, Value> = CommandRegistry::new()
        .specs()
        .iter()
        .map(|spec| (spec.id().to_owned(), Value::String(spec.label().text(Lang::En).to_owned())))
        .collect();
    let index = Value::Object(ids);
    format!(
        "You are the assistant inside Z3L, a program a furniture maker uses to build cabinet furniture. \
         Answer the person in their language, in words. When they ask for a change, call `{BATCH_TOOL}` \
         once with every step of the answer: one call is one undo entry, and a refusal takes the whole \
         call back. Use `{CHECKLIST_TOOL}` to say which steps you are working through. Ask before you \
         guess a dimension the person did not give.\n\n\
         The program, as it describes itself:\n{guide}\n\n\
         The command ids of this build, each with its label:\n{catalogue}",
        guide = engine::guide(),
        catalogue = index,
    )
}

/// One completion, on the thread [`OpenAi::ask`] started.
fn complete(agent: &ureq::Agent, endpoint: &Endpoint, key: &str, body: &Value) -> Result<Reply, BackendError> {
    let response = agent
        .post(endpoint.chat_url())
        .header("Authorization", format!("Bearer {}", key.trim()))
        .header("Accept", "application/json")
        .send_json(body)
        .map_err(|error| refusal(endpoint, &error))?;
    let mut response = response;
    let value: Value = response
        .body_mut()
        .read_json()
        .map_err(|error| BackendError::new(format!("`{}` did not answer with JSON: {error}", endpoint.base_url())))?;
    reply_from(&value)
}

/// Why a request failed, in one sentence that names the endpoint and never the key.
fn refusal(endpoint: &Endpoint, error: &ureq::Error) -> BackendError {
    let sentence = match error {
        ureq::Error::StatusCode(code) => format!("`{}` answered HTTP {code}", endpoint.base_url()),
        ureq::Error::Timeout(_) => {
            format!("`{}` did not answer within {} seconds", endpoint.base_url(), REQUEST_TIMEOUT.as_secs())
        }
        other => format!("`{}` could not be reached: {other}", endpoint.base_url()),
    };
    BackendError::new(sentence)
}

/// The reply one completion holds: its words, its checklist and the batch its tool calls name.
fn reply_from(value: &Value) -> Result<Reply, BackendError> {
    let message = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .ok_or_else(|| BackendError::new("the endpoint answered no choice: the reply has no message to read"))?;
    let mut reply = Reply { text: words(message.get("content")), ..Reply::default() };
    if let Some(calls) = message.get("tool_calls").and_then(Value::as_array) {
        for call in calls {
            read_call(call, &mut reply)?;
        }
    }
    Ok(reply)
}

/// The words of one message, whether the endpoint sent a string or a list of parts.
fn words(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(text)) => text.trim().to_owned(),
        Some(Value::Array(parts)) => {
            let joined: Vec<&str> = parts
                .iter()
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .collect();
            joined.join("\n")
        }
        _ => String::new(),
    }
}

/// One tool call, added to `reply`: a batch's steps or the checklist, and nothing else.
fn read_call(call: &Value, reply: &mut Reply) -> Result<(), BackendError> {
    let function = call
        .get("function")
        .ok_or_else(|| BackendError::new("the endpoint sent a tool call with no function: nothing to dispatch"))?;
    let name = function.get("name").and_then(Value::as_str).unwrap_or_default();
    let arguments = match function.get("arguments") {
        // The wire sends the arguments as a JSON string; a gateway that sends the object itself is
        // read the same way rather than refused.
        Some(Value::String(text)) => serde_json::from_str::<Value>(text).map_err(|error| {
            BackendError::new(format!("`{name}` was called with arguments that are not JSON: {error}"))
        })?,
        Some(value) => value.clone(),
        None => json!({}),
    };
    match name {
        BATCH_TOOL => {
            let call: BatchCall = serde_json::from_value(arguments).map_err(|error| {
                BackendError::new(format!("`{name}` was called with arguments it does not take: {error}"))
            })?;
            reply.calls.extend(call.steps.into_iter().map(step_of));
        }
        CHECKLIST_TOOL => {
            let call: ChecklistCall = serde_json::from_value(arguments).map_err(|error| {
                BackendError::new(format!("`{name}` was called with arguments it does not take: {error}"))
            })?;
            reply.checklist.extend(call.items);
        }
        other => {
            return Err(BackendError::new(format!(
                "the endpoint called `{other}`, which the assistant does not offer: it knows `{BATCH_TOOL}` and \
                 `{CHECKLIST_TOOL}`"
            )));
        }
    }
    Ok(())
}

/// One wire step as the engine's step, with the name a later step may refer to.
fn step_of(step: StepRequest) -> BatchStep {
    match step.name {
        Some(name) => BatchStep::new(step.command, step.params).named(name),
        None => BatchStep::new(step.command, step.params),
    }
}

/// An answer shaped the way the wire carries it, for the tests that never start a request.
#[cfg(test)]
impl Reply {
    /// This reply as one completion's answer body.
    fn into_wire(self) -> serde_json::Value {
        let calls: Vec<Value> = self
            .calls
            .iter()
            .map(|step| {
                let arguments = json!({ "steps": [{ "command": step.command(), "params": step.params() }] });
                json!({ "function": { "name": BATCH_TOOL, "arguments": arguments.to_string() } })
            })
            .collect();
        json!({ "choices": [{ "message": { "content": self.text, "tool_calls": calls } }] })
    }
}

/// The module's tests: a stub endpoint on this machine, and a turn driven to its end.
#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use std::time::{Duration, Instant};

    use engine::{BatchOptions, Session};
    use model::Id;
    use serde_json::json;

    use super::*;
    use crate::chat::Turn;
    use crate::chat::tests::session as fixture;

    /// Drives one turn to its end, sleeping between frames so the worker can answer.
    ///
    /// It is the shell's half of `Turn::advance`: one scope per message, every batch inside it, and
    /// the scope closed when the turn is over.
    fn drive(backend: &mut dyn ChatBackend, session: &mut Session) -> Conversation {
        let scope = session.edit_scope();
        let mut conversation = Conversation::new();
        conversation.push(Speaker::Person, "rename the side");
        let mut turn = Turn::new();
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut dispatch = |steps: &[BatchStep]| session.agent_batch_in(scope, steps, BatchOptions::ALL_OR_NOTHING);
        while !turn.is_done() && Instant::now() < deadline {
            turn.advance(backend, &mut conversation, Lang::En, &mut dispatch);
            std::thread::sleep(Duration::from_millis(2));
        }
        session.end_edit();
        assert!(turn.is_done(), "the turn ended before its deadline");
        conversation
    }

    fn name(session: &Session, panel: Id) -> Option<String> {
        session.project().node(panel).map(|node| node.name().to_owned())
    }

    /// A scratch folder holding `agent-key` with owner-only permissions, and the key's text.
    ///
    /// It is the file home the tests read, so the request a stub receives carries a key that came
    /// through the same path a person's does.
    fn keyed(name: &str) -> PathBuf {
        let directory = testkit::scratch_dir(name);
        std::fs::create_dir_all(&directory).expect("scratch");
        let path = directory.join(KEY_FILE);
        std::fs::write(&path, KEY_TEXT).expect("write");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).expect("mode");
        }
        directory
    }

    /// The key every stub test sends.
    const KEY_TEXT: &str = "sk-test-secret";

    #[test]
    fn a_turn_reaches_the_stub_and_lands_one_undo_entry() {
        let (session_id, panel) = fixture();
        let mut session = session_id;
        let (url, requests) = testkit::stub_endpoint(testkit::batch_answer(panel, "Left"));
        let mut backend = OpenAi::new(Endpoint::new(url, "stub-model"), Some(keyed("openai-turn")));

        let conversation = drive(&mut backend, &mut session);

        let request = requests.recv_timeout(Duration::from_secs(5)).expect("the stub was asked");
        assert_eq!(request["model"], "stub-model", "the model name is data: {request}");
        assert_eq!(
            request["messages"][0]["role"], "system",
            "the guide reaches the model before its first call: {request}"
        );
        assert!(request["tools"].as_array().is_some_and(|tools| tools.len() == 2), "two tools are offered");
        assert_eq!(name(&session, panel).as_deref(), Some("Left"), "the answer landed");
        session.undo().expect("the whole answer is one entry");
        assert_eq!(name(&session, panel).as_deref(), Some("Side"), "one undo takes the answer back");
        assert!(
            conversation
                .lines()
                .iter()
                .any(|line| line.speaker == Speaker::Agent && line.text.starts_with("Renaming the side")),
            "the model's words are in the conversation"
        );
        assert!(
            conversation.lines().iter().any(|line| line.text.contains("as one undo entry")),
            "the turn reports what it did: {:?}",
            conversation.lines()
        );
    }

    #[test]
    fn changing_the_base_url_and_the_model_alone_reaches_another_endpoint() {
        // ADR 0189: a local server is the same client pointed elsewhere, so nothing but the two
        // settings may decide who answers.
        let (mut session, panel) = fixture();
        let (first_url, first_requests) = testkit::stub_endpoint(testkit::batch_answer(panel, "Left"));
        let (second_url, second_requests) = testkit::stub_endpoint(Reply::saying("nothing to do").into_wire());
        let mut first = OpenAi::new(Endpoint::new(first_url, "model-a"), Some(keyed("openai-first")));
        let mut second = OpenAi::new(Endpoint::new(second_url, "model-b"), Some(keyed("openai-second")));

        let _ = drive(&mut first, &mut session);
        drive_half(&mut second, &mut session);

        assert_eq!(first_requests.recv_timeout(Duration::from_secs(5)).expect("the first stub")["model"], "model-a");
        assert_eq!(second_requests.recv_timeout(Duration::from_secs(5)).expect("the second stub")["model"], "model-b");
    }

    /// One frame of a turn, which is all a backend needs to have started its request.
    fn drive_half(backend: &mut dyn ChatBackend, session: &mut Session) {
        let scope = session.edit_scope();
        let mut conversation = Conversation::new();
        conversation.push(Speaker::Person, "hello");
        let mut turn = Turn::new();
        let mut dispatch = |steps: &[BatchStep]| session.agent_batch_in(scope, steps, BatchOptions::ALL_OR_NOTHING);
        turn.advance(backend, &mut conversation, Lang::En, &mut dispatch);
        std::thread::sleep(Duration::from_millis(20));
        turn.advance(backend, &mut conversation, Lang::En, &mut dispatch);
        session.end_edit();
    }

    #[test]
    fn a_reply_that_only_speaks_asks_for_nothing() {
        let reply = reply_from(&Reply::saying("the scene has two panels").into_wire()).expect("a spoken answer");
        assert_eq!(reply.text, "the scene has two panels");
        assert!(reply.calls.is_empty(), "words alone dispatch nothing");
    }

    #[test]
    fn a_tool_the_assistant_does_not_offer_is_a_sentence_not_a_dispatch() {
        let value = json!({
            "choices": [{ "message": { "content": "", "tool_calls": [{
                "function": { "name": "shell", "arguments": "{}" }
            }] } }]
        });
        let error = reply_from(&value).expect_err("an unknown tool is refused");
        assert!(error.sentence().contains("`shell`"), "the sentence names the tool: {}", error.sentence());
    }

    #[test]
    fn the_key_is_the_environments_first_and_the_files_second() {
        let directory = testkit::scratch_dir("agent-key");
        std::fs::create_dir_all(&directory).expect("scratch");
        let path = directory.join(KEY_FILE);
        std::fs::write(&path, "  file-key\n").expect("write");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).expect("mode");
        }
        assert_eq!(key_from(Some("env-key".to_owned()), Some(&directory)).expect("the environment wins"), "env-key");
        assert_eq!(key_from(None, Some(&directory)).expect("the file is the second home"), "file-key");
        assert!(key_from(None, Some(&directory.join("nothing"))).is_err(), "no file is no key");
    }

    #[test]
    fn a_key_file_others_can_read_is_refused_before_it_is_used() {
        if !cfg!(unix) {
            return;
        }
        let directory = testkit::scratch_dir("agent-key-open");
        std::fs::create_dir_all(&directory).expect("scratch");
        let path = directory.join(KEY_FILE);
        std::fs::write(&path, "file-key").expect("write");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).expect("mode");
        }
        let error = key_from(None, Some(&directory)).expect_err("a world-readable key is refused");
        assert!(error.sentence().contains(KEY_FILE), "the sentence names the file: {}", error.sentence());
        assert!(!error.sentence().contains("file-key"), "and never the key");
    }

    #[test]
    fn the_key_never_prints_and_never_travels_to_an_error() {
        // Nothing listens on port 9, so this request fails; the sentence must not carry the key, and
        // neither must any value the backend hands a test or a log.
        let folder = keyed("openai-secret");
        let endpoint = Endpoint::new("http://127.0.0.1:9/v1", "model");
        let mut backend = OpenAi::new(endpoint, Some(folder));
        let printed = format!("{backend:?}");
        assert!(!printed.contains(KEY_TEXT), "the backend's own shape carries no key: {printed}");
        let mut conversation = Conversation::new();
        conversation.push(Speaker::Person, "hello");
        backend.ask(&conversation);
        let mut failure = None;
        for _ in 0..500 {
            if let Some(answer) = backend.take() {
                failure = Some(answer);
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let error = failure.expect("the request fails without a listener").expect_err("port 9 answers nothing");
        assert!(!error.sentence().contains(KEY_TEXT), "the refusal never names the key: {}", error.sentence());
    }

    #[test]
    fn a_workspace_with_no_key_answers_the_sentence_instead_of_asking() {
        let mut backend = OpenAi::new(Endpoint::new("http://127.0.0.1:9/v1", "model"), None);
        let mut conversation = Conversation::new();
        conversation.push(Speaker::Person, "hello");
        backend.ask(&conversation);
        let error = backend.take().expect("the answer is there at once").expect_err("no key, no request");
        assert!(error.sentence().contains(KEY_ENV), "the sentence names both homes: {}", error.sentence());
        assert!(error.sentence().contains(KEY_FILE));
    }

    #[test]
    fn the_endpoint_builds_the_url_the_wire_documents() {
        assert_eq!(
            Endpoint::new("https://api.example.com/v1/", "m").chat_url(),
            "https://api.example.com/v1/chat/completions"
        );
        assert!(!Endpoint::new("", "m").is_configured(), "an empty base URL is no endpoint");
        assert!(!Endpoint::new("https://h/v1", "",).is_configured(), "and so is an empty model");
    }
}
