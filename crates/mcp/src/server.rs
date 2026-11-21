//! The served session: one document, four tools, and the protocol over a pair of streams.
//!
//! The server owns the one session a headless process has, behind a mutex because the protocol may
//! poll it from any thread and a person's window may hold the same handle later (ADR 0188). Every
//! tool dispatches synchronously inside the lock: the engine is synchronous by design, so a client
//! that calls two tools at once is serialized here instead of the document growing a second writer.
//!
//! A batch that lands is written through the [`SceneSink`] the shell installed, once, after the
//! engine installed it — the same order `z3l run` keeps. A write that fails is reported as a
//! failure and never as success: the next client must not believe the file holds what it does not.

use std::fmt;
use std::sync::{Arc, Mutex};

use engine::{BatchOptions, BatchStep, CommandRegistry, Query, Session};
use rmcp::RoleServer;
use rmcp::ServerHandler;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ErrorData, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerConfig,
};
use rmcp::service::{MaybeSendFuture, RequestContext};
use serde_json::{Map, Value, json};
use text::Lang;

use crate::SceneSink;
use crate::report::{self, Write};
use crate::tools::{
    Answer, BatchRequest, CommandListRequest, RunRequest, StepRequest, TOOL_BATCH, TOOL_LIST, TOOL_QUERY, TOOL_RUN,
    parse,
};

/// What the server tells a client about the vocabulary it is about to use.
///
/// It is one paragraph of instructions because a model reads it once, before its first call, and
/// the alternative — a model guessing the parameter shape — is what the schemas exist to prevent.
const INSTRUCTIONS: &str = "\
These tools drive Z3L, a cabinet-furniture editor. Call `command_list` first: it names every \
command this build can dispatch, the JSON Schema of the parameters each one takes, and the \
commands that cannot run in this session with the sentence that says why. Dispatch a change with \
`command_run` or, for several changes that must land together, `command_batch` — the whole batch is \
one undo entry, and by default a refusal takes back every step before it. A step names what it \
creates with `as` and a later step refers to that name as `{\"$ref\": \"<name>\"}`, so a part and \
the machining on it are one call. A furniture part is born only inside the open product, so name one with `product` when the scene holds several: opening it \
is a view operation and not an undo entry, and the answer says which product the batch ran in. Read \
the scene with `query` and never guess a number: a length on the wire is millimetres, an angle is \
degrees and intrinsic XYZ Euler angles.";

/// The one session a served process dispatches into, and where its document is written.
#[derive(Clone)]
pub struct CommandServer {
    session: Arc<Mutex<Session>>,
    sink: Option<Arc<dyn SceneSink>>,
}

impl std::fmt::Debug for CommandServer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("CommandServer").field("sink", &self.sink.is_some()).finish()
    }
}

impl CommandServer {
    /// Serves one session, writing it through `sink` when a batch lands.
    ///
    /// The session is handed in as a handle rather than owned, so the window that shows the same
    /// document and the tool server can share it, and a test keeps its own way to read the history
    /// the tools moved (`AGENTS.md` §4.11).
    ///
    /// `sink` is `None` for a session served without a scene: its document lives in memory, and every
    /// answer says that the file was not written rather than pretending it was.
    pub fn new(session: Arc<Mutex<Session>>, sink: Option<Arc<dyn SceneSink>>) -> Self {
        Self { session, sink }
    }

    /// The `command_list` answer: every command, what it takes, and whether it can run here.
    pub fn list(&self) -> Answer {
        let session = match self.session.lock() {
            Ok(session) => session,
            Err(_) => return Answer::refused(BUSY),
        };
        Answer::ok(listing(&session))
    }

    /// The `command_run` and `command_batch` answer: the report of one batch, and the write.
    pub fn dispatch(&self, request: &BatchRequest) -> Answer {
        let mut session = match self.session.lock() {
            Ok(session) => session,
            Err(_) => return Answer::refused(BUSY),
        };
        let sink = self.sink.as_deref();
        dispatch(&mut session, request, Door::Headless { sink })
    }

    /// The `query` answer: the read model's own report, untouched.
    pub fn ask(&self, query: Query) -> Answer {
        let session = match self.session.lock() {
            Ok(session) => session,
            Err(_) => return Answer::refused(BUSY),
        };
        asking(&session, query)
    }

    /// Performs one tool call, naming the tool in the sentence a malformed request answers with.
    fn call(&self, name: &str, arguments: Option<Map<String, Value>>) -> Result<Answer, ErrorData> {
        match name {
            TOOL_LIST => {
                let _: CommandListRequest = parse(TOOL_LIST, arguments).map_err(tool_error)?;
                Ok(self.list())
            }
            TOOL_RUN => {
                let request = parse::<RunRequest>(TOOL_RUN, arguments).map_err(tool_error)?;
                Ok(self.dispatch(&request.batch()))
            }
            TOOL_BATCH => {
                let request = parse::<BatchRequest>(TOOL_BATCH, arguments).map_err(tool_error)?;
                Ok(self.dispatch(&request))
            }
            TOOL_QUERY => {
                let query = parse::<Query>(TOOL_QUERY, arguments).map_err(tool_error)?;
                Ok(self.ask(query))
            }
            other => Err(ErrorData::invalid_params(
                format!(
                    "no tool is named `{other}`; this server offers {TOOL_LIST}, {TOOL_RUN}, {TOOL_BATCH}, {TOOL_QUERY}"
                ),
                None,
            )),
        }
    }
}

/// Where a batch is served from, which is the one thing a window and a headless session do
/// differently.
///
/// A headless session may write the scene it was handed, so a landed batch reaches the file. The
/// window never writes: the tab is dirty on purpose, the path may not exist yet, and the person's
/// *Save* is the write. The report says which of the two happened instead of leaving a caller to
/// guess (ADR 0005, ADR 0188).
#[derive(Clone, Copy)]
pub enum Door<'a> {
    /// A session a process serves, with the writer the shell handed in, if it handed one in.
    Headless {
        /// Where a landed batch is written; `None` serves a document that lives in memory.
        sink: Option<&'a dyn SceneSink>,
    },
    /// The document a person is looking at: changed in the window, written by its own *Save*.
    Window,
}

impl fmt::Debug for Door<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Headless { sink } => formatter.debug_tuple("Headless").field(&sink.is_some()).finish(),
            Self::Window => formatter.write_str("Window"),
        }
    }
}

/// The `command_list` answer: every command this build dispatches, what it takes, which world it
/// serves, whether it can run in `session` — and the domain guide, so the first call a client makes
/// teaches it the program and not only the ids.
///
/// It is a free function because two doors ask it — the headless server and the attached window —
/// and a second copy of the projection is the second registry ADR 0185 forbids.
pub fn listing(session: &Session) -> Value {
    let registry = CommandRegistry::new();
    let commands: Vec<Value> = registry.specs().iter().map(|spec| command_row(session, *spec)).collect();
    json!({
        "revision": session.project().revision().get(),
        "open_product": session.active_product().map(|id| id.get()),
        "mode": session.agent_mode().word(),
        "domain": engine::guide(),
        "commands": commands,
    })
}

/// The `command_run` and `command_batch` answer: the report of one batch, and what the file holds.
///
/// The batch goes through [`Session::agent_batch`], so the mode a person left the session in decides
/// what a call may do: `plan` runs the same steps with writing off and answers with the plan, and no
/// request field can talk it into writing (ADR 0190). The answer names the mode, so a model that was
/// answered a plan reads why nothing moved instead of guessing.
pub fn dispatch(session: &mut Session, request: &BatchRequest, door: Door<'_>) -> Answer {
    if request.steps.is_empty() {
        return Answer::refused("a batch names at least one step; nothing was dispatched");
    }
    // Opening is a view operation and not a step (ADR 0098): it is done before the batch rather
    // than as one of its steps, so a refusal takes back the change and not the product the caller
    // works in. An id that is not a product is the only way this fails, and it writes nothing.
    if let Some(id) = request.product
        && let Err(refusal) = session.open_named_product(id)
    {
        return Answer::refused(refusal.to_string());
    }
    let steps: Vec<BatchStep> = request.steps.iter().map(step_of).collect();
    let options = BatchOptions::ALL_OR_NOTHING.atomic(request.atomic).dry_run(request.dry_run);
    let report = session.agent_batch(&steps, options);
    let write = match door {
        Door::Headless { sink: Some(sink) } => sink_write(sink, session, &report),
        Door::Headless { sink: None } => Write::NoFile,
        Door::Window => Write::Window,
    };
    let body = report::batch(
        &report,
        &steps,
        &write,
        session.project().revision().get(),
        session.agent_mode(),
        session.active_product(),
    );
    if report.refusal().is_some() || write.failed() { Answer::fail(body) } else { Answer::ok(body) }
}

/// The engine step a request's step is: the command, the parameters, and the name a later step of
/// the same batch refers to what it made by.
pub(crate) fn step_of(step: &StepRequest) -> BatchStep {
    let engine_step = BatchStep::new(step.command.clone(), step.params.clone());
    match &step.name {
        Some(name) => engine_step.named(name.clone()),
        None => engine_step,
    }
}

/// The `query` answer: the read model's own report, untouched.
pub fn asking(session: &Session, query: Query) -> Answer {
    match session.read(query) {
        Ok(answer) => Answer::ok(json!({
            "revision": session.project().revision().get(),
            "answer": answer,
        })),
        Err(error) => Answer::refused(error.to_string()),
    }
}

/// Writes a landed batch through `sink` and says what that came to.
fn sink_write(sink: &dyn SceneSink, session: &Session, report: &engine::BatchReport) -> Write {
    if !report.landed() || !report.content_changed() {
        return Write::Nothing;
    }
    match sink.save(session.project()) {
        Ok(()) => Write::Written,
        Err(message) => Write::Failed(message),
    }
}

/// The sentence a tool answers with when the session cannot be locked.
///
/// A poisoned lock means a call panicked while it held the document, which this tree denies; the
/// sentence exists so the client hears about it instead of a hung connection.
const BUSY: &str = "the session is unavailable: a previous call left it in a broken state";

/// One row of `command_list`: the id, the label, what it creates, the schema and the session's answer.
///
/// `enabled` is the session's half of the command's own check: a part is born only inside an open
/// product, and that is the condition a caller can learn before it writes a request. What the
/// parameters decide is left to `command_run`, whose refusal is the command's own sentence — a list
/// carries no request to judge (ADR 0098, ADR 0185).
fn command_row(session: &Session, spec: engine::CommandSpec) -> Value {
    let unavailable = spec.unavailable(session);
    let mut row = json!({
        "id": spec.id(),
        "label": spec.label().text(Lang::En),
        "params": spec.params_schema(),
        "family": engine::Family::of(spec.id()).map(engine::Family::word),
        "enabled": unavailable.is_none(),
    });
    let Some(object) = row.as_object_mut() else {
        return row;
    };
    if let Some(word) = spec.creation_word() {
        object.insert("creates".to_owned(), json!(word));
    }
    if let Some(refusal) = unavailable {
        object.insert("reason".to_owned(), json!(refusal.to_string()));
    }
    row
}

/// The refusal a malformed request answers with, as the protocol's own invalid-parameters error.
pub(crate) fn tool_error(answer: Answer) -> ErrorData {
    let sentence =
        answer.body().get("error").and_then(Value::as_str).unwrap_or("the arguments cannot be read").to_owned();
    ErrorData::invalid_params(sentence, None)
}

/// The tool result a client reads: the JSON as text, and the same JSON as a structured answer.
pub(crate) fn result(answer: Answer) -> CallToolResult {
    let text = answer.body().to_string();
    let content = vec![ContentBlock::text(text)];
    let mut result = if answer.failed() { CallToolResult::error(content) } else { CallToolResult::success(content) };
    result.structured_content = Some(answer.body().clone());
    result
}

impl ServerHandler for CommandServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(INSTRUCTIONS)
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, ErrorData>> + MaybeSendFuture + '_ {
        std::future::ready(Ok(ListToolsResult::with_all_items(crate::tools::definitions())))
    }

    fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<CallToolResponse, ErrorData>> + MaybeSendFuture + '_ {
        std::future::ready(
            self.call(request.name.as_ref(), request.arguments).map(|answer| CallToolResponse::from(result(answer))),
        )
    }
}

/// A session handle and its sink, made from the pieces a caller already has.
///
/// A convenience for a door that owns a `Session` and a way to write it, so neither the command
/// line nor a test has to know how the server keeps them.
pub fn serve_session(session: Session, sink: Option<Arc<dyn SceneSink>>) -> CommandServer {
    CommandServer::new(Arc::new(Mutex::new(session)), sink)
}
