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

use engine::{BatchStep, CommandRegistry, Query};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use text::Lang;

use crate::chat::{BATCH_TOOL, BackendError, CHECKLIST_TOOL, ChatBackend, Conversation, Reply, ToolCall};
use crate::tools::{CommandListRequest, FindingsRequest, StepRequest, TOOL_FINDINGS, TOOL_LIST, TOOL_QUERY};

/// The environment variable a key is read from first.
pub const KEY_ENV: &str = "Z3L_AGENT_KEY";

/// The file a key is read from second, in the folder the settings file lives in.
pub const KEY_FILE: &str = "agent-key";

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
    /// This chat's stable id, sent as `x-opencode-session` when the endpoint is a Go gateway. The
    /// name is the gateway's, not a setting: the person never sees it and no other provider is asked
    /// for it (ADR 0189).
    session: String,
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
    ///
    /// The agent names its TLS provider explicitly: [`ureq::tls::TlsProvider`] defaults to Rustls
    /// and "is never picked up automatically", so an https endpoint would otherwise panic on the
    /// request thread with the `rustls` feature off. `native-tls` is the provider this workspace
    /// compiles (ADR 0189).
    pub fn new(endpoint: Endpoint, folder: Option<PathBuf>) -> Self {
        Self::with_session(endpoint, folder, String::new())
    }

    /// The same backend, keeping the chat's id for a gateway that routes on it.
    ///
    /// The id is opaque; one chat keeps one value for its whole life, so a gateway's routing and
    /// prompt cache survive a backend rebuilt by a settings change. An empty value gets a generated
    /// one, for a caller that did not make a chat (a test, a one-shot tool).
    pub fn with_session(endpoint: Endpoint, folder: Option<PathBuf>, session: String) -> Self {
        let session = if session.trim().is_empty() { default_session() } else { session };
        let tls = ureq::tls::TlsConfig::builder().provider(ureq::tls::TlsProvider::NativeTls).build();
        // The status is read by [`complete`] so the provider's own sentence reaches the panel; the
        // default would turn a 400 into a bare `StatusCode` with the body discarded.
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(REQUEST_TIMEOUT))
            .http_status_as_error(false)
            .tls_config(tls)
            .build();
        Self { endpoint, folder, session, agent: ureq::Agent::new_with_config(config), answer: None }
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
        let session = self.session.clone();
        let (sender, answer) = mpsc::channel();
        let spawned = std::thread::Builder::new().name("agent-request".to_owned()).spawn(move || {
            // A send fails only when the turn dropped the request it replaces; the answer is then
            // nobody's, which is exactly what the person's stop asked for.
            let _ = sender.send(complete(&agent, &endpoint, &key, &body, &session));
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

    fn cancel(&mut self) {
        // The safe point for a request already sent: dropping the receiver makes the worker's send
        // fail, and the turn ends on this frame. A blocking TLS read cannot be aborted without a
        // socket handle, so the gateway may finish the request; its answer is nobody's.
        self.answer = None;
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
    let mut system = system_message();
    if !transcript.brief().trim().is_empty() {
        system.push_str("\n\nThe scene right now, live from the window; never ask the person to repeat it:\n");
        system.push_str(transcript.brief().trim());
    }
    let mut messages = vec![json!({ "role": "system", "content": system })];
    messages.extend(transcript.messages().iter().cloned());
    json!({
        "model": endpoint.model(),
        "messages": messages,
        "tools": tools(),
        "tool_choice": "auto",
    })
}

/// The tools a model may call, with the schema each one's own argument type publishes.
///
/// Five and not two: a write and a checklist, and the three reads the tool server already publishes
/// — `command_list`, `query` and `inspect`. The reads are the same functions a foreign client calls,
/// so a model in the panel and a model over MCP read one answer (ADR 0185). Without them a model can
/// only guess a command's parameter names, and every guess that is wrong costs a whole batch and a
/// request (J14 measured the guess; X130 measured the cost).
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
        {
            "type": "function",
            "function": {
                "name": TOOL_LIST,
                "description": "Read what this build can dispatch: every command id with its English \
                                label and the world it serves, plus the domain guide. Name one command \
                                with `command` to read the JSON Schema of its parameters — the field \
                                names, the unit of each and what the command refuses. Call it before \
                                you send a command whose parameters you have not read in this \
                                conversation: a step whose parameters do not match is refused and \
                                takes the whole batch back.",
                "parameters": schema::<CommandListRequest>(),
            }
        },
        {
            "type": "function",
            "function": {
                "name": TOOL_QUERY,
                "description": "Ask the open document a question and write nothing: the node tree, \
                                one node's box or measured bounds, the parts table, the panel \
                                diagnostics, the niche around a point, or `materials` — the records \
                                the workshop's catalogue carries, with the id a board or a band is \
                                named by and its thickness. Call it to see what a batch left before \
                                you say what it did, and to name a material a command needs.",
                "parameters": schema::<Query>(),
            }
        },
        {
            "type": "function",
            "function": {
                "name": TOOL_FINDINGS,
                "description": "Read the mistakes the rules can prove, in severity order: a part \
                                outside the product envelope, two bodies in the same place, a shelf \
                                that does not fit, a part that leaves its sheet. Call it after a \
                                build and before you speak.",
                "parameters": schema::<FindingsRequest>(),
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
         Answer the person in their language, in a few sentences. When they ask for a change, call \
         `{BATCH_TOOL}` once with every step of the answer: one call is one undo entry, and a refusal \
         takes the whole call back. Use `{CHECKLIST_TOOL}` to say which steps you are working through.\n\n\
         Read before you write. A command's fields and units are in its schema and nowhere else: before \
         a step whose parameters you have not read in this conversation, call `{TOOL_LIST}` with that \
         command id and read it. `{TOOL_QUERY}` answers what the document holds — the tree, a node's \
         box or measured bounds, the parts table, the diagnostics, and the workshop's catalogue of \
         materials, which is the only place a board's id and thickness come from — and \
         `{TOOL_FINDINGS}` answers the mistakes the rules can prove. Call both after a batch and before you say what it did. A \
         refusal is an answer you can act on: its sentence names the field or the unit, so fix that and \
         call again in the same turn instead of asking the person.\n\n\
         The scene brief below is live and authoritative: it names the root container, the open product \
         and the nodes with their ids. Never ask the person for an id, a unit or a side order that the \
         brief or the house convention answers. A free body belongs under the root and needs no product. \
         Sizes are millimetres in width × depth × height order unless the person says otherwise; convert \
         them to the command's micrometres yourself and never show micrometres to the person. When the \
         person gives the numbers, act in the same turn; ask only when a required value is absent and \
         no convention covers it. Three unequal sides are a box, not a cube: do not correct the person. \
         A placement is `{{\"translation\": [x, y, z], \"rotation\": [x, y, z, w]}}` with the identity \
         rotation `[0, 0, 0, 1]`, never a bare array; a box is one `object.cuboid.create` with all \
         three sides, never one body per face.\n\n\
         The program, as it describes itself:\n{guide}\n\n\
         The command ids of this build, each with its label:\n{catalogue}",
        guide = engine::guide(),
        catalogue = index,
    )
}

/// Whether `base_url` is an OpenCode Go gateway: those route on a session header the OpenAI wire
/// does not name, and the program knows it by the address rather than asking the person (ADR 0231).
///
/// The predicate is the gateway's own name and path, so the `zen/go` and `inference/go/...` forms
/// both match and no other endpoint is sent the header.
fn is_opencode_go(base_url: &str) -> bool {
    let url = base_url.trim().to_ascii_lowercase();
    url.contains("opencode.ai") && url.contains("/go")
}

/// A stable id for a caller that did not make a chat, so a Go request never carries an empty header.
fn default_session() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SESSIONS: AtomicU64 = AtomicU64::new(1);
    let n = SESSIONS.fetch_add(1, Ordering::Relaxed);
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("z3l-{t:x}-{n:x}-{:x}", std::process::id())
}

/// One completion, on the thread [`OpenAi::ask`] started.
fn complete(
    agent: &ureq::Agent,
    endpoint: &Endpoint,
    key: &str,
    body: &Value,
    session: &str,
) -> Result<Reply, BackendError> {
    let mut request = agent
        .post(endpoint.chat_url())
        .header("Authorization", format!("Bearer {}", key.trim()))
        .header("Accept", "application/json");
    if is_opencode_go(endpoint.base_url()) {
        request = request.header("x-opencode-session", session);
    }
    let mut response = request.send_json(body).map_err(|error| refusal(endpoint, &error))?;
    if !response.status().is_success() {
        let code = response.status().as_u16();
        let body = response.body_mut().with_config().limit(MAX_ERROR_BYTES).read_to_string().unwrap_or_default();
        return Err(status_refusal(endpoint, code, &body));
    }
    let value: Value = response
        .body_mut()
        .read_json()
        .map_err(|error| BackendError::new(format!("`{}` did not answer with JSON: {error}", endpoint.base_url())))?;
    reply_from(&value)
}

/// How much of a provider's refusal is read before the sentence is built.
const MAX_ERROR_BYTES: u64 = 4096;

/// How many characters of the provider's message a refusal carries, so a page never floods the chat.
const MAX_ERROR_CHARS: usize = 300;

/// A status the endpoint answered: the code and, when the body carries one, the provider's sentence.
fn status_refusal(endpoint: &Endpoint, code: u16, body: &str) -> BackendError {
    let sentence = match provider_message(body) {
        Some(message) => format!("`{}` answered HTTP {code}: {message}", endpoint.base_url()),
        None => format!("`{}` answered HTTP {code}", endpoint.base_url()),
    };
    BackendError::new(sentence)
}

/// The provider's own sentence from an error body: OpenAI's `error.message`, a bare `message`, or
/// nothing. Whitespace is folded and the text is capped, so a refusal stays one readable line.
fn provider_message(body: &str) -> Option<String> {
    let value: Value = serde_json::from_str(body).ok()?;
    let message = value
        .get("error")
        .and_then(|error| error.get("message"))
        .or_else(|| value.get("message"))
        .and_then(Value::as_str)?
        .trim();
    if message.is_empty() {
        return None;
    }
    let folded = message.split_whitespace().collect::<Vec<_>>().join(" ");
    Some(folded.chars().take(MAX_ERROR_CHARS).collect())
}

/// Why a request failed, in one sentence that names the endpoint and never the key.
///
/// A status is not routed here: [`complete`] reads its body first, because the code alone was the
/// reason a gateway's `MissingSessionID` stayed invisible (J23).
fn refusal(endpoint: &Endpoint, error: &ureq::Error) -> BackendError {
    let sentence = match error {
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

/// One tool call, added to `reply`: the call itself, and what the turn reads out of it.
///
/// Every call is kept whole, because it is what the history is sent back with. A batch is also read
/// into steps and a checklist into items; a read is not parsed here at all — the door that answers
/// it parses the call, so its refusal is in the model's history instead of in the turn's end. An
/// argument list that cannot be read becomes a fault on the call, which the turn answers the same
/// way: a model that mistyped one key is told which key, in the same turn.
fn read_call(call: &Value, reply: &mut Reply) -> Result<(), BackendError> {
    let function = call
        .get("function")
        .ok_or_else(|| BackendError::new("the endpoint sent a tool call with no function: nothing to dispatch"))?;
    let name = function.get("name").and_then(Value::as_str).unwrap_or_default().to_owned();
    let id = call.get("id").and_then(Value::as_str).map(str::to_owned).unwrap_or_else(|| format!("call-{name}"));
    let (arguments, unreadable) = match function.get("arguments") {
        // The wire sends the arguments as a JSON string; a gateway that sends the object itself is
        // read the same way rather than refused.
        Some(Value::String(text)) => match serde_json::from_str::<Value>(text) {
            Ok(value) => (value, None),
            Err(error) => (json!({}), Some(format!("`{name}` was called with arguments that are not JSON: {error}"))),
        },
        Some(value) => (value.clone(), None),
        None => (json!({}), None),
    };
    let reading = |sentence: String| ToolCall::new(id.clone(), name.clone(), arguments.clone()).faulted(sentence);
    let mut recorded = ToolCall::new(id.clone(), name.clone(), arguments.clone());
    match name.as_str() {
        BATCH_TOOL => match unreadable {
            Some(sentence) => reply.tool_calls.push(reading(sentence)),
            None => match serde_json::from_value::<BatchCall>(arguments.clone()) {
                Ok(call) => {
                    reply.calls.extend(call.steps.into_iter().map(step_of));
                    reply.tool_calls.push(recorded);
                }
                Err(error) => reply
                    .tool_calls
                    .push(reading(format!("`{name}` was called with arguments it does not take: {error}"))),
            },
        },
        CHECKLIST_TOOL => match unreadable {
            Some(sentence) => reply.tool_calls.push(reading(sentence)),
            None => match serde_json::from_value::<ChecklistCall>(arguments.clone()) {
                Ok(call) => {
                    reply.checklist.extend(call.items);
                    reply.tool_calls.push(recorded);
                }
                Err(error) => reply
                    .tool_calls
                    .push(reading(format!("`{name}` was called with arguments it does not take: {error}"))),
            },
        },
        _ => {
            recorded.fault = unreadable;
            reply.tool_calls.push(recorded);
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
    /// This reply as one completion's answer body: its words and every call it made.
    fn into_wire(self) -> serde_json::Value {
        let calls: Vec<Value> = self
            .tool_calls
            .iter()
            .map(|call| {
                json!({
                    "id": call.id,
                    "type": "function",
                    "function": { "name": call.name, "arguments": call.arguments.to_string() },
                })
            })
            .collect();
        let content = if self.text.trim().is_empty() { Value::Null } else { Value::String(self.text) };
        json!({ "choices": [{ "message": { "content": content, "tool_calls": calls } }] })
    }
}

/// The module's tests: a stub endpoint on this machine, and a turn driven to its end.
#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use std::time::{Duration, Instant};

    use engine::Session;
    use model::Id;
    use serde_json::json;

    use super::*;
    use crate::chat::tests::session as fixture;
    use crate::chat::{Speaker, Turn};

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
        let mut door = crate::reads::MessageDoor::new(session, scope);
        while !turn.is_done() && Instant::now() < deadline {
            turn.advance(backend, &mut conversation, Lang::En, &mut door);
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
        let offered: Vec<&str> = request["tools"]
            .as_array()
            .map(|tools| tools.iter().filter_map(|tool| tool["function"]["name"].as_str()).collect::<Vec<&str>>())
            .unwrap_or_default();
        assert_eq!(
            offered,
            vec![BATCH_TOOL, CHECKLIST_TOOL, TOOL_LIST, TOOL_QUERY, TOOL_FINDINGS],
            "one write, one checklist and the three reads the tool server publishes"
        );
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

    /// Waits for the worker's answer, the way a frame reads it across several frames.
    fn wait(backend: &mut OpenAi) -> Option<Result<Reply, BackendError>> {
        for _ in 0..500 {
            if let Some(answer) = backend.take() {
                return Some(answer);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        None
    }

    /// One frame of a turn, which is all a backend needs to have started its request.
    fn drive_half(backend: &mut dyn ChatBackend, session: &mut Session) {
        let scope = session.edit_scope();
        let mut conversation = Conversation::new();
        conversation.push(Speaker::Person, "hello");
        let mut turn = Turn::new();
        let mut door = crate::reads::MessageDoor::new(session, scope);
        turn.advance(backend, &mut conversation, Lang::En, &mut door);
        std::thread::sleep(Duration::from_millis(20));
        turn.advance(backend, &mut conversation, Lang::En, &mut door);
        session.end_edit();
    }

    #[test]
    fn an_opencode_go_endpoint_carries_its_session_and_another_endpoint_does_not() {
        // X124: the owner's Go gateway answered 400 MissingSessionID while the same curl worked. The
        // gateway routes on a header the OpenAI wire does not name, so the program names it by the
        // address and sends one stable id per chat. No header field is offered, and no other
        // endpoint is given the header.
        let (mut session, panel) = fixture();
        let (url, requests) = testkit::stub_endpoint_request(testkit::batch_answer(panel, "Left"));
        // The predicate is by gateway name and path; point that shape at the loopback stub.
        let go_url = url.replace("127.0.0.1", "opencode.ai@127.0.0.1").replace("/v1", "/go/v1");
        let mut backend =
            OpenAi::with_session(Endpoint::new(go_url, "stub-model"), Some(keyed("openai-go")), "ses_1".to_owned());
        let _ = drive(&mut backend, &mut session);
        let request = requests.recv_timeout(Duration::from_secs(5)).expect("the stub was asked");
        assert!(
            request.headers.iter().any(|(name, value)| name == "x-opencode-session" && value == "ses_1"),
            "the Go gateway carries the chat's id: {:?}",
            request.headers
        );

        let (mut session, panel) = fixture();
        let (url, requests) = testkit::stub_endpoint_request(testkit::batch_answer(panel, "Left"));
        let mut backend = OpenAi::new(Endpoint::new(url, "stub-model"), Some(keyed("openai-plain")));
        let _ = drive(&mut backend, &mut session);
        let request = requests.recv_timeout(Duration::from_secs(5)).expect("the stub was asked");
        assert!(
            !request.headers.iter().any(|(name, _)| name == "x-opencode-session"),
            "another endpoint is not sent the gateway's header: {:?}",
            request.headers
        );
    }

    #[test]
    fn a_stop_drops_the_answer_in_flight() {
        // The person's Stop must end the turn now, not at the provider's time. Dropping the receiver
        // is the backend's safe point; the turn pushes its stopped line on the same frame.
        let (url, _requests) = testkit::stub_endpoint(Reply::saying("ok").into_wire());
        let mut backend = OpenAi::new(Endpoint::new(url, "stub-model"), Some(keyed("openai-cancel")));
        let mut conversation = Conversation::new();
        conversation.push(Speaker::Person, "hello");
        backend.ask(&conversation);
        assert!(!backend.waiting(), "the request is in flight");
        backend.cancel();
        assert!(backend.waiting(), "the stop takes the answer away");
        assert!(backend.take().is_none(), "and nothing is handed to the turn");
    }

    #[test]
    fn a_status_refusal_carries_the_providers_own_sentence() {
        // The owner's go gateway answered 400 `MissingSessionID`; the panel read only "HTTP 400"
        // because the body was discarded. The refusal now carries the provider's sentence, capped,
        // and still never the key.
        let body = r#"{"type":"error","error":{"type":"MissingSessionID","message":"Request is missing x-opencode-session and cannot be routed efficiently."}}"#;
        let (url, _requests) = testkit::stub_endpoint_status(400, body);
        let mut backend = OpenAi::new(Endpoint::new(url, "stub-model"), Some(keyed("openai-refusal")));
        let mut conversation = Conversation::new();
        conversation.push(Speaker::Person, "hello");

        backend.ask(&conversation);
        let error = wait(&mut backend).expect("the turn answers").expect_err("HTTP 400 is a refusal");

        assert!(error.sentence().contains("HTTP 400"), "{}", error.sentence());
        assert!(
            error.sentence().contains("x-opencode-session"),
            "the provider's own words reach the panel: {}",
            error.sentence()
        );
        assert!(!error.sentence().contains(KEY_TEXT), "the refusal never names the key");
    }

    #[test]
    fn the_scene_brief_and_the_house_conventions_reach_the_system_message() {
        // X125: a model without the root id asked the person for one. The brief is the answer, and
        // the conventions are what keep it from asking about units or side order.
        let mut transcript = Conversation::new();
        transcript.set_brief("root container id = 7\n- a free body belongs under the root");
        let body = request_body(&Endpoint::new("https://api.example.com/v1", "m"), &transcript);
        let system = body["messages"][0]["content"].as_str().expect("a system message");
        assert!(system.contains("root container id = 7"), "the live id reaches the model");
        assert!(system.contains("width × depth × height"), "the side order is stated");
        assert!(system.contains("never show micrometres"), "the person sees millimetres");
        assert!(system.contains("Never ask the person for an id"), "and an id is never demanded");
        assert!(system.contains("\"translation\""), "the placement shape is stated");
        assert!(system.contains("never one body per face"), "one box is one body");
    }

    #[test]
    fn a_reply_that_only_speaks_asks_for_nothing() {
        let reply = reply_from(&Reply::saying("the scene has two panels").into_wire()).expect("a spoken answer");
        assert_eq!(reply.text, "the scene has two panels");
        assert!(reply.calls.is_empty(), "words alone dispatch nothing");
    }

    #[test]
    fn a_tool_the_door_does_not_know_is_carried_to_it_instead_of_ending_the_turn() {
        // The name is not the wire's to refuse: the door answers it with the sentence that names the
        // tools it does offer, and the model reads that refusal as the answer to its own call. The
        // turn ends on an answer that names no call, never on a name it did not expect.
        let value = json!({
            "choices": [{ "message": { "content": "", "tool_calls": [{
                "id": "call-9",
                "function": { "name": "shell", "arguments": "{}" }
            }] } }]
        });
        let reply = reply_from(&value).expect("an unknown tool is the door's to answer");
        assert!(reply.calls.is_empty(), "it dispatches nothing");
        assert_eq!(reply.tool_calls.len(), 1);
        assert_eq!(reply.tool_calls[0].name, "shell");
        assert_eq!(reply.tool_calls[0].id, "call-9", "the answer is tied to the call");
        assert!(reply.tool_calls[0].fault.is_none(), "the name is read; only its answer refuses it");
    }

    #[test]
    fn the_key_is_the_environments_first_and_the_files_second() {
        // Both homes are values here, never the process's: a test that reads `std::env` answers
        // differently on a configured machine, which is how `cargo xtask ci` stopped being green
        // for the owner (X130).
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
        let missing = key_from(None, Some(&directory.join("nothing"))).expect_err("no file is no key");
        assert!(missing.sentence().contains(KEY_ENV), "the refusal names the variable: {}", missing.sentence());
        assert!(missing.sentence().contains(KEY_FILE), "and the file beside the settings: {}", missing.sentence());
        let nowhere = key_from(None, None).expect_err("no home at all is no key");
        assert!(nowhere.sentence().contains(KEY_ENV) && nowhere.sentence().contains(KEY_FILE));
    }

    #[test]
    fn the_environment_names_the_first_home() {
        // `std::env::set_var` is unsafe in edition 2024 and this crate forbids `unsafe`, so the
        // variable's home is proved by re-running this binary with the variable set: the child reads
        // a real environment through [`key`] itself, which is the path a person's shell takes.
        let binary = std::env::current_exe().expect("the test binary's own path");
        let child = std::process::Command::new(binary)
            .args(["openai::tests::environment_home_probe", "--exact", "--ignored"])
            .env(KEY_ENV, "env-key")
            .output()
            .expect("a child of the same test binary");
        assert!(
            child.status.success(),
            "a child with `{KEY_ENV}` set finds the key: {}{}",
            String::from_utf8_lossy(&child.stdout),
            String::from_utf8_lossy(&child.stderr)
        );
    }

    #[test]
    #[ignore = "run by `the_environment_names_the_first_home` in a child process"]
    fn environment_home_probe() {
        assert_eq!(key(None).expect("the environment names the key"), "env-key");
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
    fn the_transport_is_the_operating_systems_tls() {
        // ADR 0189: `native-tls` is the only provider this tree compiles. ureq's default is
        // Rustls, and it never switches by itself, so an https endpoint would panic on the worker
        // instead of answering.
        let backend = OpenAi::new(Endpoint::new("https://api.example.com/v1", "model"), None);
        assert_eq!(
            backend.agent.config().tls_config().provider(),
            ureq::tls::TlsProvider::NativeTls,
            "the agent asks for the provider the workspace enables"
        );
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
