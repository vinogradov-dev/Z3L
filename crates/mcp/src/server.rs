//! The served session: one document, the coarse tools, and the protocol over a pair of streams.
//!
//! The server owns the one session a headless process has, behind a mutex because the protocol may
//! poll it from any thread and a person's window may hold the same handle later (ADR 0188). Every
//! tool dispatches synchronously inside the lock: the engine is synchronous by design, so a client
//! that calls two tools at once is serialized here instead of the document growing a second writer.
//!
//! A batch that lands is written through the [`SceneSink`] the shell installed, once, after the
//! engine installed it — the same order `z3l run` keeps. A headless session writes every landed
//! batch; the document a person is looking at is written only when the call asks with `save`, because
//! the tab is theirs and its dirty marker is the truth until they say otherwise. A write that fails,
//! and a save a scene with no path cannot take, are reported as failures and never as success: the
//! next client must not believe the file holds what it does not (ADR 0005, ADR 0188, X114).

use std::fmt;
use std::sync::{Arc, Mutex};

use engine::{AgentMode, BatchOptions, BatchStep, CommandRegistry, EditScope, Query, Session};
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
use crate::preview::{MAX_BYTES, PreviewSize, Previewer, base64};
use crate::report::{self, Write};
use crate::tools::{
    Answer, BatchRequest, CommandListRequest, FindingsRequest, PreviewRequest, RunRequest, StepRequest, TOOL_BATCH,
    TOOL_FINDINGS, TOOL_LIST, TOOL_PREVIEW, TOOL_QUERY, TOOL_RUN, TOOL_UNDO, UndoRequest, parse,
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
is a view operation and not an undo entry, and the answer says which product the batch ran in. Call \
`undo` to take back the last entry a turn landed — the document returns to exactly what it held \
before it — and read the revision the answer reports rather than assuming one. Read \
the scene with `query` and never guess a number: a length is integer micrometres, a placement's \
translation is floating-point millimetres, and an angle is integer millidegrees — every schema \
annotates the `unit` of each such field, and every answer names it (`size_um`, `origin_mm`). Call \
`inspect` to hear the mistakes the rules can prove — a part outside its product's envelope, two \
bodies sharing volume, a shelf that does not fit, a rail in the wrong place, a missing or hidden edge \
band, a wrong front gap and a part larger than its sheet — each naming the nodes it blames and the \
commands a fix would dispatch: the rules find the mistake and the model explains it. A person \
describes a piece from in front of it — the front is +Y, up is +Z, so the side they see on their left \
is +X — and another person may read the same words the other way round: read the description from \
the front and say back which axis you put each group of parts on. A window's scene is written to the \
file it came from only \
when a call asks with `save`, and a scene that was never saved refuses that ask: the person names \
the file once, and every later save reaches it.";

/// The one session a served process dispatches into, where its document is written, and what draws
/// its pictures.
#[derive(Clone)]
pub struct CommandServer {
    session: Arc<Mutex<Session>>,
    sink: Option<Arc<dyn SceneSink>>,
    previewer: Option<Arc<dyn Previewer>>,
}

impl std::fmt::Debug for CommandServer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CommandServer")
            .field("sink", &self.sink.is_some())
            .field("previewer", &self.previewer.is_some())
            .finish()
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
    ///
    /// `previewer` is `None` for a process that has no device to draw with — a machine with no
    /// adapter, or a caller that built the server without one. `render_preview` then refuses with
    /// that sentence instead of failing the protocol, so a client learns why there is no picture
    /// (ADR 0228).
    pub fn new(
        session: Arc<Mutex<Session>>,
        sink: Option<Arc<dyn SceneSink>>,
        previewer: Option<Arc<dyn Previewer>>,
    ) -> Self {
        Self { session, sink, previewer }
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

    /// The `undo` answer: one entry taken back, or the sentence that says why none was.
    pub fn undo(&self) -> Answer {
        let mut session = match self.session.lock() {
            Ok(session) => session,
            Err(_) => return Answer::refused(BUSY),
        };
        undo(&mut session)
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
            TOOL_UNDO => {
                let _: UndoRequest = parse(TOOL_UNDO, arguments).map_err(tool_error)?;
                Ok(self.undo())
            }
            TOOL_FINDINGS => {
                let _: FindingsRequest = parse(TOOL_FINDINGS, arguments).map_err(tool_error)?;
                Ok(self.findings())
            }
            TOOL_PREVIEW => {
                let request = parse::<PreviewRequest>(TOOL_PREVIEW, arguments).map_err(tool_error)?;
                Ok(self.preview(&request))
            }
            other => Err(ErrorData::invalid_params(
                format!(
                    "no tool is named `{other}`; this server offers {TOOL_LIST}, {TOOL_RUN}, {TOOL_BATCH}, \
                     {TOOL_UNDO}, {TOOL_QUERY}, {TOOL_FINDINGS}, {TOOL_PREVIEW}"
                ),
                None,
            )),
        }
    }

    /// The `inspect` answer: the rules' findings, and the revision they read.
    pub fn findings(&self) -> Answer {
        match self.session.lock() {
            Ok(session) => Answer::ok(findings(&session)),
            Err(_) => Answer::refused(BUSY),
        }
    }

    /// The `render_preview` answer: the scene as one picture, or the sentence that says why none.
    pub fn preview(&self, request: &PreviewRequest) -> Answer {
        match self.session.lock() {
            Ok(session) => preview(&session, request, self.previewer.as_deref()),
            Err(_) => Answer::refused(BUSY),
        }
    }
}

/// Where a batch is served from, which is the one thing a window and a headless session do
/// differently.
///
/// A headless session may write the scene it was handed, so a landed batch reaches the file. The
/// window writes only the save a caller asked for: the tab is dirty on purpose, the path may not
/// exist yet, and a file nobody asked for could overwrite work the person has not saved. The report
/// says which of the three happened instead of leaving a caller to guess (ADR 0005, ADR 0188, X114).
#[derive(Clone, Copy)]
pub enum Door<'a> {
    /// A session a process serves, with the writer the shell handed in, if it handed one in.
    ///
    /// Every landed batch that changed the document is written without asking again: naming the
    /// scene on `z3l mcp serve <scene>` was that ask, made once when the process started.
    Headless {
        /// Where a landed batch is written; `None` serves a document that lives in memory.
        sink: Option<&'a dyn SceneSink>,
    },
    /// The document a person is looking at: written when the caller asked with `save`, and never
    /// otherwise.
    Window {
        /// The file a save the caller asked for is written to: the one the scene was opened from or
        /// last saved to. `None` while the scene has no path, which refuses the ask.
        sink: Option<&'a dyn SceneSink>,
        /// The edit scope of an assistant turn in flight, so a process that edits a document in
        /// several batches of one message still leaves one undo entry (ADR 0187, ADR 0230).
        ///
        /// It is the shell's answer and not the request's: a caller cannot name a scope, because a
        /// scope is a promise the person's window makes about its own history.
        scope: Option<EditScope>,
    },
}

impl Door<'_> {
    /// The turn this door lands inside, when one is running.
    const fn scope(self) -> Option<EditScope> {
        match self {
            Self::Headless { .. } => None,
            Self::Window { scope, .. } => scope,
        }
    }
}

impl fmt::Debug for Door<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Headless { sink } => formatter.debug_tuple("Headless").field(&sink.is_some()).finish(),
            Self::Window { sink, .. } => formatter.debug_tuple("Window").field(&sink.is_some()).finish(),
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
/// answered a plan reads why nothing moved instead of guessing. Whether the file received the batch
/// is the door's rule and not the request's: see [`Door`].
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
    // A turn's scope is what makes several batches of one message one undo entry: the first pushes
    // the state before the message and the rest replace the document inside it (ADR 0187).
    let report = match door.scope() {
        Some(scope) => session.agent_batch_in(scope, &steps, options),
        None => session.agent_batch(&steps, options),
    };
    let write = match door {
        Door::Headless { sink: Some(sink) } => write_landed(sink, session, &report),
        Door::Headless { sink: None } => Write::NoFile,
        Door::Window { sink, .. } => write_asked(sink, session, &report, request.save),
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

/// The `undo` answer: the document took back one entry, and what it holds now.
///
/// It is a tool and not a command because a command is a step of a batch, and a batch is one undo
/// entry: taking one back from inside the entry it would be taking back is a contradiction. So the
/// history answers for itself, and the answer says the revision the document holds and whether
/// another entry is left, which is how a caller knows it undid exactly the turn it meant to.
///
/// [`AgentMode::Plan`] refuses it for the same reason it refuses a batch: undoing is a write, and plan
/// and build are the person's switch (ADR 0190). The sentence is the batch's own [`PLAN_NOTE`],
/// because a caller that reads it once should not have to learn a second wording for the same switch.
pub fn undo(session: &mut Session) -> Answer {
    if session.agent_mode() == AgentMode::Plan {
        return Answer::refused(report::PLAN_NOTE);
    }
    match session.undo() {
        Ok(()) => Answer::ok(json!({
            "revision": session.project().revision().get(),
            "can_undo": session.can_undo(),
        })),
        Err(error) => Answer::refused(error.to_string()),
    }
}

/// The `inspect` answer: the mistakes the rules prove, and the revision they read.
///
/// One pass, one list, ordered by severity then code (ADR 0191). It is a free function because the
/// window's door and the headless door answer it, and a second copy of the report is the second
/// reading ADR 0185 forbids. Nothing is written: the revision is the one the document already has.
pub fn findings(session: &Session) -> Value {
    let findings: Vec<Value> = session.inspect().iter().map(engine::Finding::wire).collect();
    json!({
        "revision": session.project().revision().get(),
        "findings": findings,
    })
}

/// The `render_preview` answer: the picture the shell drew, or the sentence that says why there is
/// none.
///
/// The bounds are checked in the order a caller pays for them: the size first, so a call asking for
/// a wall poster costs no upload and no render, and the encoded length after, because only an
/// encoder knows what a scene weighs. Both refuse — a clamp would answer a picture the call did not
/// ask for with nothing saying so (ADR 0228).
///
/// It is a free function because the window's door and the headless one answer it, and a second
/// reading of the caps is a second rule (§4.1 of `AGENTS.md`).
pub fn preview(session: &Session, request: &PreviewRequest, previewer: Option<&dyn Previewer>) -> Answer {
    let size = match PreviewSize::new(request.width, request.height) {
        Ok(size) => size,
        Err(sentence) => return Answer::refused(sentence),
    };
    let Some(previewer) = previewer else {
        return Answer::refused(NO_DEVICE);
    };
    let png = match previewer.preview(session, size) {
        Ok(png) => png,
        Err(sentence) => return Answer::refused(sentence),
    };
    if png.len() > MAX_BYTES {
        return Answer::refused(format!(
            "the picture encodes to {} bytes, over the {MAX_BYTES}-byte cap this tool sends: ask for a smaller size, or \
             read the scene with `query`",
            png.len()
        ));
    }
    Answer::ok(json!({
        "revision": session.project().revision().get(),
        "width": size.width,
        "height": size.height,
        "bytes": png.len(),
        "mime_type": MIME_PNG,
    }))
    .with_image(png)
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

/// Writes a headless session's landed batch through `sink` and says what that came to.
///
/// A batch that left the document exactly as it was has nothing new to write, and a batch that did
/// not land — refused, rolled back, or planned — has nothing to write either.
fn write_landed(sink: &dyn SceneSink, session: &Session, report: &engine::BatchReport) -> Write {
    match report.landed() && report.content_changed() {
        true => write_through(sink, session),
        false => Write::Nothing,
    }
}

/// Writes the window's document when the caller asked, and refuses the ask when there is no path.
///
/// The tab is the person's, so a landed batch that nobody asked to save stays dirty: the answer says
/// `window` rather than writing a file behind their back (ADR 0005, X114). A caller that did ask is
/// answered with the file's own words, and a scene that was never saved is where the ask ends — a
/// caller cannot name a path, so the sentence names the person's act instead of a silent new file
/// (ADR 0188). A batch that landed and changed nothing still writes, because the document the
/// person is looking at may hold earlier work the caller is asking to keep.
fn write_asked(sink: Option<&dyn SceneSink>, session: &Session, report: &engine::BatchReport, asked: bool) -> Write {
    match (asked, report.landed(), sink) {
        (false, ..) => Write::Window,
        (true, false, _) => Write::Nothing,
        (true, true, None) => Write::NoPath,
        (true, true, Some(sink)) => write_through(sink, session),
    }
}

/// Hands the document to `sink` and says what the write came to.
fn write_through(sink: &dyn SceneSink, session: &Session) -> Write {
    match sink.save(session.project()) {
        Ok(()) => Write::Written,
        Err(message) => Write::Failed(message),
    }
}

/// The media type of the one picture this server sends.
const MIME_PNG: &str = "image/png";

/// The sentence `render_preview` answers with in a process that has no device behind it.
///
/// It names the machine's own answer — no adapter — because the caller's next move is either a
/// different door or a different machine, and neither is a retry.
const NO_DEVICE: &str = "this process has no device to draw with, so `render_preview` cannot answer: the picture is drawn by the shell, and this one has no GPU adapter. Every other tool works here";

/// The sentence a tool answers with when the session cannot be locked.
///
/// A poisoned lock means a call panicked while it held the document, which this tree denies; the
/// sentence exists so the client hears about it instead of a hung connection.
const BUSY: &str = "the session is unavailable: a previous call left it in a broken state";

/// One row of `command_list`: the id, the label, what it creates and the schema of its parameters.
///
/// There is no `enabled` column: a command writes wherever its own parameters are valid, and the
/// open product is the *interface's* condition, not the command's — a discovery surface that asked
/// it would tell a caller "no" about a request the dispatch accepts (ADR 0226). What the parameters
/// decide is left to `command_run`, whose refusal is the command's own sentence.
fn command_row(_session: &Session, spec: engine::CommandSpec) -> Value {
    let mut row = json!({
        "id": spec.id(),
        "label": spec.label().text(Lang::En),
        "params": spec.params_schema(),
        "family": engine::Family::of(spec.id()).map(engine::Family::word),
    });
    if let Some(object) = row.as_object_mut()
        && let Some(word) = spec.creation_word()
    {
        object.insert("creates".to_owned(), json!(word));
    }
    row
}

/// The refusal a malformed request answers with, as the protocol's own invalid-parameters error.
pub(crate) fn tool_error(answer: Answer) -> ErrorData {
    let sentence =
        answer.body().get("error").and_then(Value::as_str).unwrap_or("the arguments cannot be read").to_owned();
    ErrorData::invalid_params(sentence, None)
}

/// The tool result a client reads: the JSON as text, the same JSON as a structured answer, and the
/// picture as a block of the protocol's own when the tool drew one.
pub(crate) fn result(answer: Answer) -> CallToolResult {
    let text = answer.body().to_string();
    let mut content = vec![ContentBlock::text(text)];
    if let Some(png) = answer.image() {
        content.push(ContentBlock::image(base64(png), MIME_PNG));
    }
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
pub fn serve_session(
    session: Session,
    sink: Option<Arc<dyn SceneSink>>,
    previewer: Option<Arc<dyn Previewer>>,
) -> CommandServer {
    CommandServer::new(Arc::new(Mutex::new(session)), sink, previewer)
}
