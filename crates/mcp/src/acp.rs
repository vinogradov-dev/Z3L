//! An agent process that owns the model loop, spoken to over the Agent Client Protocol.
//!
//! This is the third [`ChatBackend`] (ADR 0186), and the one a person uses with an agent they
//! already run: the panel sends the message, the process decides what to do with it, and the
//! editor owns the document and the tools. The protocol is ACP over the process's own standard
//! streams (ADR 0230): the client sends `initialize`, `session/new` with the editor's channel as
//! one MCP server, one `session/prompt` per message, and `session/cancel` when the person stops.
//! The conversation is drawn from the protocol's own notifications — what the agent says, the tool
//! calls it makes and what came back — so the panel shows a process's work instead of its claims.
//!
//! Three promises the shape keeps, and what each one costs:
//!
//! - **The tools stay the server's.** The process is handed one MCP server: `z3l mcp bridge`, the
//!   pipe into the window's own control channel ([`crate::channel::bridge`]). Every call it makes
//!   is answered by the window at a frame boundary with the window's policy, so a process cannot
//!   do more than the panel that started it — and because the token is read from the settings
//!   folder, it never enters the process's arguments or environment.
//! - **No file and no terminal.** The client advertises neither `fs` nor `terminal`, and a process
//!   that asks anyway is refused: a scene is not a directory, and the tools are the way in.
//! - **The window never waits.** Everything that blocks — the process's streams — lives on two
//!   threads this module owns; [`Acp::ask`] posts an event and returns, [`Acp::take`] reads what
//!   arrived since the last frame.
//!
//! A process that dies mid-turn leaves the document as the last completed batch left it: a batch
//! is all or nothing in the window, and the turn's undo entry closes with the turn (ADR 0187).
//!
//! Changes when: the protocol version this build speaks changes, or the editor starts offering a
//! capability it does not offer today.

use std::collections::BTreeMap;
use std::fmt;
use std::io::{BufRead, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;

use serde_json::{Value, json};

use crate::chat::{BackendError, ChatBackend, Conversation, Line, Reply, Speaker};

/// The protocol version this build speaks: ACP's major version, one integer.
pub const PROTOCOL_VERSION: u64 = 1;

/// The name the agent sees this editor by.
pub const CLIENT_NAME: &str = "Z3L";

/// The name the window's own tool server is offered to the process under.
pub const SERVER_NAME: &str = "z3l";

/// The most a single frame from the process may weigh, in bytes.
///
/// A tool answer can carry a picture — `render_preview` sends up to four megabytes encoded — so the
/// cap is generous; what it stops is a process that never ends a line, which would otherwise grow
/// this process's memory until it dies.
const MAX_FRAME: usize = 32 * 1024 * 1024;

/// One process that has started and speaks the protocol on its streams.
pub struct Started {
    /// What the process says.
    pub reader: Box<dyn BufRead + Send>,
    /// What is said to the process.
    pub writer: Box<dyn Write + Send>,
    /// Ends the process, so a stopped turn does not leave it running behind the window.
    pub ending: Box<dyn Fn() + Send + Sync>,
}

impl fmt::Debug for Started {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("Started").finish_non_exhaustive()
    }
}

/// How one agent process starts: the seam a test hands a scripted process in through.
///
/// A real run is [`Program`]; a test gives [`Scripted`] and the whole turn — handshake, prompt,
/// notifications, a tool call and its refusal — runs over pipes with no binary and no network.
pub trait Spawner: fmt::Debug + Send {
    /// Starts the process, or says why it could not.
    fn spawn(&self) -> Result<Started, BackendError>;
}

/// The command a person configured: one program and its arguments.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Program {
    /// The program to run.
    pub program: String,
    /// The arguments it is run with, in order.
    pub args: Vec<String>,
}

impl Program {
    /// A program with no arguments.
    pub fn new(program: impl Into<String>) -> Self {
        Self { program: program.into(), args: Vec::new() }
    }

    /// The same program with these arguments.
    pub fn with_args(mut self, args: Vec<String>) -> Self {
        self.args = args;
        self
    }

    /// The command a person typed on one line: the program first, then its arguments.
    ///
    /// The settings keep one line because a workshop types a command the way it types one into a
    /// shell, and a person who needs a space inside an argument needs a shell more than this field.
    /// An empty line is no command at all, which is the state the panel says out loud.
    pub fn parse(command: &str) -> Option<Self> {
        let mut words = command.split_whitespace();
        let program = words.next()?;
        Some(Self { program: program.to_owned(), args: words.map(str::to_owned).collect() })
    }
}

impl Spawner for Program {
    fn spawn(&self) -> Result<Started, BackendError> {
        if self.program.trim().is_empty() {
            return Err(BackendError::new("no agent process is configured"));
        }
        let mut child = Command::new(&self.program)
            .args(&self.args)
            // The process's own stderr is its own business; its standard streams are the protocol.
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| BackendError::new(format!("`{}` did not start: {error}", self.program)))?;
        let reader = child.stdout.take().ok_or_else(|| BackendError::new("the agent process gave no output"))?;
        let writer = child.stdin.take().ok_or_else(|| BackendError::new("the agent process took no input"))?;
        let held = std::sync::Arc::new(std::sync::Mutex::new(child));
        let ending = std::sync::Arc::clone(&held);
        Ok(Started {
            reader: Box::new(std::io::BufReader::new(reader)),
            writer: Box::new(writer),
            ending: Box::new(move || {
                if let Ok(mut child) = ending.lock() {
                    end(&mut child);
                }
            }),
        })
    }
}

/// Ends one process: asking first, and taking it down only if it did not listen.
fn end(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// A backend whose process is a script the test wrote.
///
/// It is the acceptance ADR 0028 demands — a whole task runs with no network and no binary — moved
/// one protocol further out: the script is a real peer on real pipes, so the handshake, the frames
/// and the cancellation are exercised as the wire has them.
#[derive(Clone)]
pub struct Scripted {
    spawner: std::sync::Arc<dyn Fn() -> Result<Started, BackendError> + Send + Sync>,
}

impl Scripted {
    /// A scripted process behind the closure that starts it.
    pub fn new(spawner: impl Fn() -> Result<Started, BackendError> + Send + Sync + 'static) -> Self {
        Self { spawner: std::sync::Arc::new(spawner) }
    }
}

impl fmt::Debug for Scripted {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("Scripted").finish_non_exhaustive()
    }
}

impl Spawner for Scripted {
    fn spawn(&self) -> Result<Started, BackendError> {
        (self.spawner)()
    }
}

/// One command the shell gives the worker thread.
enum Event {
    /// The person's message: one `session/prompt`.
    Ask(String),
    /// The person's stop: one `session/cancel`.
    Cancel,
    /// A frame from the process.
    Frame(String),
    /// The process's stream ended, with the reason a person would read.
    Ended(String),
    /// The panel is gone: end the process and the worker.
    Stop,
}

/// What one request this client sent is still waiting for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Waiting {
    /// The handshake: which protocol version and which capabilities.
    Initialize,
    /// The session: which id this conversation has.
    New,
    /// The message: what the agent did, and why it stopped.
    Prompt,
}

/// The backend the panel drives: one process, one conversation, one message at a time.
///
/// The worker thread owns the process; this side owns two channels and nothing else, so a frame
/// never waits on a process, a socket or a lock.
pub struct Acp {
    events: Sender<Event>,
    answers: Receiver<Result<Reply, BackendError>>,
    /// The answer taken out of the channel by a `waiting` question, kept for `take`.
    peeked: std::sync::Mutex<Option<Result<Reply, BackendError>>>,
    worker: Option<JoinHandle<()>>,
}

impl fmt::Debug for Acp {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The process is the worker's, and the two channels are the whole of this side's state.
        formatter.debug_struct("Acp").finish_non_exhaustive()
    }
}

impl Acp {
    /// A backend that will start `program` the first time it is asked something.
    ///
    /// The window's channel is offered to the process as its one MCP server: the bridge is this
    /// build's own executable, and the token is read from the settings folder it publishes in, so
    /// nothing secret travels in the process's arguments.
    pub fn new(program: Program) -> Self {
        Self::with_spawner(Box::new(program))
    }

    /// The same backend over a spawner the caller owns, which is how a test hands in a script.
    pub fn with_spawner(spawner: Box<dyn Spawner>) -> Self {
        let (events, inbox) = mpsc::channel();
        let (outbox, answers) = mpsc::channel();
        let server = mcp_server();
        let asks = events.clone();
        let worker = std::thread::Builder::new().name("z3l-acp".to_owned()).spawn(move || {
            let mut driver = Driver::new(spawner, asks, outbox, server);
            driver.run(inbox);
        });
        Self { events, answers, peeked: std::sync::Mutex::new(None), worker: worker.ok() }
    }

    /// Sends one event to the worker, which is the only thing this side does to the process.
    fn send(&self, event: Event) {
        // A worker that has gone is a state the panel learns about through `take`: the channel's
        // sender fails, and the answer it would have written is the process's own ending.
        let _ = self.events.send(event);
    }

    /// Takes the answer waiting on the channel into the one slot this side holds, if it is empty.
    ///
    /// The slot is what lets `waiting` answer without spending the answer the frame is about to
    /// draw: the channel itself can only be read by taking from it.
    fn peek(&self) -> bool {
        let mut slot = match self.peeked.lock() {
            Ok(slot) => slot,
            // A poisoned slot holds no answer a frame could draw, and a backend that panicked
            // nowhere is the only way it is poisoned.
            Err(poisoned) => poisoned.into_inner(),
        };
        if slot.is_none() {
            *slot = self.answers.try_recv().ok();
        }
        slot.is_some()
    }
}

impl ChatBackend for Acp {
    fn ask(&mut self, transcript: &Conversation) {
        // The agent is told the message, not the history: ACP's own conversation lives in the
        // process, which is the point of this backend (ADR 0230).
        let said = transcript
            .lines()
            .iter()
            .rev()
            .find(|line| line.speaker == Speaker::Person)
            .map(|line| line.text.clone())
            .unwrap_or_default();
        self.send(Event::Ask(said));
    }

    fn take(&mut self) -> Option<Result<Reply, BackendError>> {
        self.peek();
        match self.peeked.lock() {
            Ok(mut slot) => slot.take(),
            Err(poisoned) => poisoned.into_inner().take(),
        }
    }

    fn waiting(&self) -> bool {
        !self.peek()
    }

    fn cancel(&mut self) {
        self.send(Event::Cancel);
    }
}

impl Drop for Acp {
    fn drop(&mut self) {
        self.send(Event::Stop);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// The one MCP server the process is given: this build's bridge into the open window's channel.
fn mcp_server() -> Value {
    let command = std::env::current_exe()
        .ok()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| "z3l".to_owned());
    json!([{
        "name": SERVER_NAME,
        "command": command,
        "args": ["mcp", "bridge"],
        "env": [],
    }])
}

/// The worker's state: the process, what is still being waited for, and what the panel is owed.
struct Driver {
    spawner: Box<dyn Spawner>,
    /// The worker's own end of its inbox, handed to the reader thread.
    events: Sender<Event>,
    answers: Sender<Result<Reply, BackendError>>,
    server: Value,
    writer: Option<Box<dyn Write + Send>>,
    ending: Option<Box<dyn Fn() + Send + Sync>>,
    alive: bool,
    waiting: BTreeMap<u64, Waiting>,
    next: u64,
    session: Option<String>,
    prompting: bool,
    /// A message that arrived before the session was ready.
    queued: Option<String>,
    /// What the agent has said and not yet been drawn: chunks are one paragraph, not ten.
    pending: String,
    /// The tool calls this turn made, by id, so a refusal can name the call it came from.
    titles: BTreeMap<String, String>,
    /// The plan the agent last sent, which the panel draws beside the conversation.
    checklist: Vec<String>,
}

impl Driver {
    fn new(
        spawner: Box<dyn Spawner>,
        events: Sender<Event>,
        answers: Sender<Result<Reply, BackendError>>,
        server: Value,
    ) -> Self {
        Self {
            spawner,
            events,
            answers,
            server,
            writer: None,
            ending: None,
            alive: false,
            waiting: BTreeMap::new(),
            next: 0,
            session: None,
            prompting: false,
            queued: None,
            pending: String::new(),
            titles: BTreeMap::new(),
            checklist: Vec::new(),
        }
    }

    /// The worker loop: one event at a time until the panel lets go.
    fn run(&mut self, inbox: Receiver<Event>) {
        while let Ok(event) = inbox.recv() {
            if !self.handle(event) {
                return;
            }
        }
    }

    /// One event, and whether the worker goes on.
    fn handle(&mut self, event: Event) -> bool {
        match event {
            Event::Ask(text) => self.ask(text),
            Event::Cancel => self.cancel(),
            Event::Frame(frame) => self.frame(&frame),
            Event::Ended(reason) => self.ended(&reason),
            Event::Stop => {
                self.end();
                return false;
            }
        }
        true
    }

    /// The person's message: start the process if it is not running, and prompt when it can listen.
    fn ask(&mut self, text: String) {
        if text.trim().is_empty() {
            return;
        }
        if !self.alive && !self.start() {
            return;
        }
        if self.prompting {
            // One message at a time: a second one waits for the first to finish (ADR 0187).
            self.queued = Some(text);
            return;
        }
        self.flush();
        match self.session.clone() {
            Some(session) => self.prompt(&session, &text),
            None => self.queued = Some(text),
        }
    }

    /// The person's stop: the process is asked to cancel the turn it is running.
    fn cancel(&mut self) {
        if !self.prompting {
            return;
        }
        let Some(session) = self.session.clone() else { return };
        let Some(writer) = self.writer.as_mut() else { return };
        let frame = json!({
            "jsonrpc": "2.0",
            "method": "session/cancel",
            "params": { "sessionId": session },
        });
        let _ = write_frame(writer, &frame);
    }

    /// Starts the process and sends the handshake; false when it could not start.
    fn start(&mut self) -> bool {
        let mut started = match self.spawner.spawn() {
            Ok(started) => started,
            Err(error) => {
                self.fail(error);
                return false;
            }
        };
        let reader = std::mem::replace(&mut started.reader, Box::new(std::io::empty()));
        self.writer = Some(std::mem::replace(&mut started.writer, Box::new(std::io::sink())));
        self.ending = Some(std::mem::replace(&mut started.ending, Box::new(|| {})));
        let events = self.events.clone();
        let spawned =
            std::thread::Builder::new().name("z3l-acp-reader".to_owned()).spawn(move || read_frames(reader, events));
        if let Err(error) = spawned {
            self.fail(BackendError::new(format!("the agent process could not be read: {error}")));
            return false;
        }
        self.alive = true;
        self.session = None;
        self.prompting = false;
        self.waiting.clear();
        self.titles.clear();
        let params = json!({
            "protocolVersion": PROTOCOL_VERSION,
            "clientCapabilities": {
                // A scene is not a directory and the tools are the way in, so the editor offers
                // neither file system access nor a terminal (ADR 0230).
                "fs": { "readTextFile": false, "writeTextFile": false },
            },
            "clientInfo": {
                "name": CLIENT_NAME,
                "title": CLIENT_NAME,
                "version": env!("CARGO_PKG_VERSION"),
            },
        });
        self.request(Waiting::Initialize, "initialize", params);
        true
    }

    /// Sends one request and remembers what it is waiting for.
    fn request(&mut self, waiting: Waiting, method: &str, params: Value) {
        let Some(writer) = self.writer.as_mut() else { return };
        self.next = self.next.saturating_add(1);
        let id = self.next;
        let frame = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        if write_frame(writer, &frame).is_ok() {
            self.waiting.insert(id, waiting);
        } else {
            let message = format!("`{method}` could not be sent to the agent process");
            self.fail(BackendError::new(message));
        }
    }

    /// One `session/prompt`: the whole message, and the turn the person will watch.
    fn prompt(&mut self, session: &str, text: &str) {
        self.prompting = true;
        self.pending.clear();
        self.titles.clear();
        let params = json!({
            "sessionId": session,
            "prompt": [{ "type": "text", "text": text }],
        });
        self.request(Waiting::Prompt, "session/prompt", params);
    }

    /// One frame from the process: an answer to this client, a notification, or a request of its own.
    fn frame(&mut self, frame: &str) {
        let Ok(value) = serde_json::from_str::<Value>(frame) else {
            log::warn!(target: "z3l::mcp", "the agent process sent a line that is not a frame");
            return;
        };
        let id = value.get("id").and_then(Value::as_u64);
        match (id, value.get("method").and_then(Value::as_str)) {
            (Some(id), None) => self.answer(id, &value),
            (Some(id), Some(method)) => self.asked(method, id, value.get("params")),
            (None, Some("session/update")) => self.update(value.get("params")),
            (None, Some(method)) => {
                log::debug!(target: "z3l::mcp", "the agent process notified `{method}`, which this build ignores");
            }
            (None, None) => {}
        }
    }

    /// The answer to a request this client sent.
    fn answer(&mut self, id: u64, value: &Value) {
        let Some(waiting) = self.waiting.remove(&id) else { return };
        if let Some(error) = value.get("error") {
            let message = error.get("message").and_then(Value::as_str).unwrap_or("the agent refused the request");
            self.fail(BackendError::new(format!("the agent process refused: {message}")));
            return;
        }
        let result = value.get("result").cloned().unwrap_or(Value::Null);
        match waiting {
            Waiting::Initialize => self.initialized(&result),
            Waiting::New => self.opened(&result),
            Waiting::Prompt => self.answered(&result),
        }
    }

    /// The handshake: the version must be the one this build speaks, and the session comes next.
    fn initialized(&mut self, result: &Value) {
        let version = result.get("protocolVersion").and_then(Value::as_u64).unwrap_or(PROTOCOL_VERSION);
        if version != PROTOCOL_VERSION {
            let message =
                format!("the agent process speaks ACP version {version}; this build speaks {PROTOCOL_VERSION}");
            self.fail(BackendError::new(message));
            return;
        }
        let cwd =
            std::env::current_dir().map(|path| path.to_string_lossy().into_owned()).unwrap_or_else(|_| "/".to_owned());
        let params = json!({ "cwd": cwd, "mcpServers": self.server });
        self.request(Waiting::New, "session/new", params);
    }

    /// The session: its id is what every later frame names, and a queued message goes out at once.
    fn opened(&mut self, result: &Value) {
        match result.get("sessionId").and_then(Value::as_str) {
            Some(session) => self.session = Some(session.to_owned()),
            None => {
                self.fail(BackendError::new("the agent process opened no session"));
                return;
            }
        }
        if let Some(text) = self.queued.take()
            && let Some(session) = self.session.clone()
        {
            self.prompt(&session, &text);
        }
    }

    /// The message's answer: what the agent stopped for, and the last of what it said.
    fn answered(&mut self, result: &Value) {
        self.prompting = false;
        let mut reply =
            Reply { lines: self.flush(), checklist: std::mem::take(&mut self.checklist), ..Reply::default() };
        if let Some(reason) = result.get("stopReason").and_then(Value::as_str)
            && reason == "refusal"
        {
            reply.lines.push(Line { speaker: Speaker::Note, text: format!("the agent declined: {}", reason) });
        }
        self.emit(reply);
        if let Some(text) = self.queued.take()
            && let Some(session) = self.session.clone()
        {
            self.prompt(&session, &text);
        }
    }

    /// One `session/update`: what the agent said, the tool it called, what came back.
    fn update(&mut self, params: Option<&Value>) {
        let Some(update) = params.and_then(|params| params.get("update")) else { return };
        match update.get("sessionUpdate").and_then(Value::as_str) {
            Some("agent_message_chunk") => {
                if let Some(text) = content_text(update) {
                    self.pending.push_str(&text);
                }
            }
            Some("tool_call") => {
                let id = update.get("toolCallId").and_then(Value::as_str).unwrap_or_default().to_owned();
                let title =
                    update.get("title").and_then(Value::as_str).map(str::to_owned).unwrap_or_else(|| id.clone());
                self.titles.insert(id, title.clone());
                let mut lines = self.flush();
                lines.push(Line { speaker: Speaker::Tool, text: title });
                self.emit(Reply::part(lines));
            }
            Some("tool_call_update") => {
                if update.get("status").and_then(Value::as_str) != Some("failed") {
                    return;
                }
                let id = update.get("toolCallId").and_then(Value::as_str).unwrap_or_default();
                let title = self.titles.get(id).cloned().unwrap_or_else(|| id.to_owned());
                let said = content_text(update).unwrap_or_default();
                let mut lines = self.flush();
                let text =
                    if said.trim().is_empty() { format!("{title}: refused") } else { format!("{title}: {said}") };
                lines.push(Line { speaker: Speaker::Note, text });
                self.emit(Reply::part(lines));
            }
            Some("plan") => {
                self.checklist = plan_entries(update);
            }
            // Species of update this build does not draw: a notice, a usage report, the list of the
            // agent's own commands, a compaction. Ignoring one costs nothing a person can see, and
            // the protocol says a client must ignore what it does not know.
            _ => {}
        }
    }

    /// A request the process makes of this client.
    fn asked(&mut self, method: &str, id: u64, params: Option<&Value>) {
        let answer = if method == "session/request_permission" {
            // The client's own gate is the window's: the plan switch is asked of every mutating
            // call at the door (ADR 0190), so a permission question here cannot grant more than
            // the person's mode already does.
            allow(params)
        } else {
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": -32601,
                    "message": "Z3L grants no file and no terminal: the document is the z3l tools' business",
                },
            })
        };
        if let Some(writer) = self.writer.as_mut() {
            let frame = if answer.get("jsonrpc").is_some() {
                answer
            } else {
                json!({ "jsonrpc": "2.0", "id": id, "result": answer })
            };
            let _ = write_frame(writer, &frame);
        }
    }

    /// The process's stream ended: the turn stops where the last completed batch left it.
    ///
    /// Only a process that ended while this side was waiting for something owes the panel a
    /// sentence: a process that had already answered leaves nothing to report, and an answer about
    /// nothing would end the next turn before it began.
    fn ended(&mut self, reason: &str) {
        let owed = self.prompting || !self.waiting.is_empty();
        self.end();
        if owed {
            let _ = self.answers.send(Err(BackendError::new(format!("the agent process ended: {reason}"))));
        } else {
            log::info!(target: "z3l::mcp", "the agent process ended: {reason}");
        }
    }

    /// The words the agent said and has not been drawn with, as one line.
    fn flush(&mut self) -> Vec<Line> {
        if self.pending.trim().is_empty() {
            self.pending.clear();
            return Vec::new();
        }
        let text = std::mem::take(&mut self.pending);
        vec![Line { speaker: Speaker::Agent, text }]
    }

    /// Hands one answer to the panel.
    fn emit(&self, reply: Reply) {
        let _ = self.answers.send(Ok(reply));
    }

    /// Tells the panel what went wrong and forgets the process, which the next message restarts.
    fn fail(&mut self, error: BackendError) {
        self.end();
        let _ = self.answers.send(Err(error));
    }

    /// Drops the process and everything this side believed about it.
    fn end(&mut self) {
        if let Some(ending) = self.ending.take() {
            ending();
        }
        self.writer = None;
        self.alive = false;
        self.session = None;
        self.prompting = false;
        self.queued = None;
        self.pending.clear();
        self.waiting.clear();
        self.titles.clear();
    }
}

/// The text one content block carries, or nothing when the block is not a text one.
fn content_text(value: &Value) -> Option<String> {
    let content = value.get("content").unwrap_or(value);
    match content.get("type").and_then(Value::as_str) {
        Some("text") => content.get("text").and_then(Value::as_str).map(str::to_owned),
        // A tool's own content list: the first text block is what a person reads.
        _ => content
            .as_array()
            .and_then(|blocks| blocks.iter().find_map(|block| content_text(block.get("content").unwrap_or(block)))),
    }
}

/// The steps of one plan, in the agent's own order.
fn plan_entries(update: &Value) -> Vec<String> {
    update
        .get("entries")
        .and_then(Value::as_array)
        .map(|entries| {
            entries.iter().filter_map(|entry| entry.get("content").and_then(Value::as_str)).map(str::to_owned).collect()
        })
        .unwrap_or_default()
}

/// The answer to a permission request: the first option that allows, or a refusal.
///
/// An agent that offers no allowing option is asking a question this client cannot answer, and the
/// protocol's own word for that is `cancelled`.
fn allow(params: Option<&Value>) -> Value {
    let chosen = params.and_then(|params| params.get("options")).and_then(Value::as_array).and_then(|options| {
        options.iter().find_map(|option| {
            let kind = option.get("kind").and_then(Value::as_str).unwrap_or_default();
            let id = option.get("optionId").and_then(Value::as_str)?;
            kind.starts_with("allow").then(|| id.to_owned())
        })
    });
    match chosen {
        Some(option) => json!({ "outcome": { "outcome": "selected", "optionId": option } }),
        None => json!({ "outcome": { "outcome": "cancelled" } }),
    }
}

/// Writes one frame on its own line, which is the whole transport.
fn write_frame(writer: &mut Box<dyn Write + Send>, frame: &Value) -> std::io::Result<()> {
    let mut line = serde_json::to_string(frame).map_err(std::io::Error::other)?;
    line.push('\n');
    writer.write_all(line.as_bytes())?;
    writer.flush()
}

/// Reads the process's frames into the worker's inbox until the stream ends.
fn read_frames(mut reader: Box<dyn BufRead + Send>, events: Sender<Event>) {
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => {
                let _ = events.send(Event::Ended("its standard output closed".to_owned()));
                return;
            }
            Ok(_) => {
                if line.len() > MAX_FRAME {
                    let _ = events.send(Event::Ended("it sent a frame larger than this build reads".to_owned()));
                    return;
                }
                if events.send(Event::Frame(line.trim_end().to_owned())).is_err() {
                    return;
                }
            }
            Err(error) => {
                let _ = events.send(Event::Ended(error.to_string()));
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    use super::*;

    /// How long a test waits for a process before calling it hung.
    const PATIENCE: Duration = Duration::from_secs(5);

    /// A scripted agent process behind this crate's own start-of-process type.
    ///
    /// The peer and its pipes come from `testkit`, so a test of this backend and a test of the whole
    /// window drive one scripted process, not two that could drift apart (ADR 0230).
    fn process(script: impl FnOnce(&mut testkit::acp::Peer) + Send + 'static) -> (Scripted, Arc<testkit::acp::Seen>) {
        let running = std::sync::Arc::new(std::sync::Mutex::new(testkit::acp::scripted(script)));
        let seen = match running.lock() {
            Ok(held) => held.seen(),
            Err(poisoned) => poisoned.into_inner().seen(),
        };
        let held = std::sync::Arc::clone(&running);
        let spawner = Scripted::new(move || {
            let mut running = match held.lock() {
                Ok(held) => held,
                Err(poisoned) => poisoned.into_inner(),
            };
            let (reader, writer) = running.start().map_err(BackendError::new)?;
            Ok(Started { reader, writer, ending: Box::new(|| {}) })
        });
        (spawner, seen)
    }

    /// Asks once and reads answers until the turn's last one, or until the process hangs.
    fn one_turn(backend: &mut Acp) -> Vec<Result<Reply, BackendError>> {
        let mut conversation = Conversation::new();
        conversation.push(Speaker::Person, "add a shelf");
        backend.ask(&conversation);
        let deadline = Instant::now() + PATIENCE;
        let mut answers = Vec::new();
        while Instant::now() < deadline {
            if let Some(answer) = backend.take() {
                let last = match &answer {
                    Ok(reply) => !reply.more,
                    Err(_) => true,
                };
                answers.push(answer);
                if last {
                    return answers;
                }
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        answers
    }

    #[test]
    fn one_turn_of_a_scripted_process_is_drawn_from_its_own_messages() {
        // The whole protocol in one test: the handshake, the session, the prompt, what the agent
        // said in two chunks, the tool it called, the refusal the window gave it and the plan it
        // kept. The panel draws all of it in the order the process said it (ADR 0230).
        let (spawner, seen) = process(|peer| {
            let Some(session) = peer.open() else { return };
            let Some(prompt) = peer.read() else { return };
            peer.send(json!({
                "jsonrpc": "2.0",
                "method": "session/update",
                "params": {
                    "sessionId": session,
                    "update": { "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": "Let me " } },
                },
            }));
            peer.send(json!({
                "jsonrpc": "2.0",
                "method": "session/update",
                "params": {
                    "sessionId": session,
                    "update": {
                        "sessionUpdate": "agent_message_chunk",
                        "content": { "type": "text", "text": "check the scene." },
                    },
                },
            }));
            peer.send(json!({
                "jsonrpc": "2.0",
                "method": "session/update",
                "params": {
                    "sessionId": session,
                    "update": {
                        "sessionUpdate": "tool_call",
                        "toolCallId": "t1",
                        "title": "command_batch · node.rename",
                        "kind": "edit",
                        "status": "pending",
                    },
                },
            }));
            peer.send(json!({
                "jsonrpc": "2.0",
                "method": "session/update",
                "params": {
                    "sessionId": session,
                    "update": {
                        "sessionUpdate": "tool_call_update",
                        "toolCallId": "t1",
                        "status": "failed",
                        "content": [{
                            "type": "content",
                            "content": { "type": "text", "text": "the document is in plan mode" },
                        }],
                    },
                },
            }));
            peer.send(json!({
                "jsonrpc": "2.0",
                "method": "session/update",
                "params": {
                    "sessionId": session,
                    "update": {
                        "sessionUpdate": "plan",
                        "entries": [{ "content": "rename the side", "status": "in_progress", "priority": "high" }],
                    },
                },
            }));
            peer.answer(&prompt, json!({ "stopReason": "end_turn" }));
        });
        let mut backend = Acp::with_spawner(Box::new(spawner));

        let answers = one_turn(&mut backend);
        assert_eq!(seen.complaint(), None, "the script did everything it was told to do");

        let mut lines = Vec::new();
        let mut finished = None;
        for answer in answers {
            let reply = answer.expect("no step of a healthy process is an error");
            lines.extend(reply.lines.clone());
            if !reply.more {
                finished = Some(reply);
            }
        }
        let drawn: Vec<(Speaker, &str)> = lines.iter().map(|line| (line.speaker, line.text.as_str())).collect();
        assert_eq!(
            drawn,
            vec![
                (Speaker::Agent, "Let me check the scene."),
                (Speaker::Tool, "command_batch · node.rename"),
                (Speaker::Note, "command_batch · node.rename: the document is in plan mode"),
            ],
            "the words, the call and its refusal, in the order the process said them"
        );
        let last = finished.expect("the turn ended with an answer of its own");
        assert_eq!(last.checklist, vec!["rename the side".to_owned()], "the plan is the panel's checklist");

        // The wire itself: the client named version one, opened a session with the window's bridge
        // as its one server, and sent the person's words as the prompt.
        let initialize = seen.method("initialize").expect("the handshake was sent");
        assert_eq!(initialize["params"]["protocolVersion"], json!(PROTOCOL_VERSION));
        assert_eq!(initialize["params"]["clientCapabilities"]["fs"]["readTextFile"], json!(false));
        let opened = seen.method("session/new").expect("the session was opened");
        assert_eq!(opened["params"]["mcpServers"][0]["name"], json!(SERVER_NAME));
        assert_eq!(opened["params"]["mcpServers"][0]["args"], json!(["mcp", "bridge"]));
        let prompt = seen.method("session/prompt").expect("the message was sent");
        assert_eq!(prompt["params"]["prompt"][0]["text"], json!("add a shelf"));
    }

    #[test]
    fn a_process_that_dies_mid_turn_says_so_and_answers_nothing_more() {
        // A process that ends after the prompt leaves the turn with a sentence and nothing else: a
        // batch is all or nothing in the window, so the document is what the last completed one
        // left, and the turn's entry closes with it (ADR 0187).
        let (spawner, seen) = process(|peer| {
            if peer.open().is_none() {
                return;
            }
            let _ = peer.read();
            // The process dies here, without answering the prompt.
        });
        let mut backend = Acp::with_spawner(Box::new(spawner));

        let answers = one_turn(&mut backend);
        let last = answers.last().expect("the turn got an answer");
        let error = last.as_ref().expect_err("a process that ended is an error, not a message");
        assert!(error.sentence().contains("the agent process ended"), "{error}");
        assert_eq!(seen.complaint(), None, "the client itself broke nothing");
    }

    #[test]
    fn a_version_this_build_does_not_speak_is_refused_before_a_session_opens() {
        let (spawner, seen) = process(|peer| {
            let Some(initialize) = peer.read() else { return };
            peer.answer(&initialize, json!({ "protocolVersion": PROTOCOL_VERSION + 1 }));
        });
        let mut backend = Acp::with_spawner(Box::new(spawner));

        let answers = one_turn(&mut backend);
        let error = answers.last().and_then(|answer| answer.as_ref().err()).expect("the version is refused");
        assert!(error.sentence().contains("ACP version 2"), "{error}");
        assert_eq!(seen.complaint(), None);
    }

    #[test]
    fn the_person_stop_is_a_cancel_the_process_hears() {
        // The process owns the loop, so stopping it is a notification it answers: the client sends
        // `session/cancel` and takes the reason the agent stopped (ADR 0230).
        let (asked, prompting) = std::sync::mpsc::channel();
        let (hang, heard) = std::sync::mpsc::channel();
        let (spawner, seen) = process(move |peer| {
            let Some(session) = peer.open() else { return };
            let Some(_prompt) = peer.read() else { return };
            let _ = asked.send(());
            // The agent works until it is told to stop.
            let Some(cancel) = peer.read() else { return };
            let _ = hang.send(cancel);
            peer.send(json!({
                "jsonrpc": "2.0",
                "method": "session/update",
                "params": {
                    "sessionId": session,
                    "update": { "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": "stopping" } },
                },
            }));
            let id = json!(3);
            peer.send(json!({ "jsonrpc": "2.0", "id": id, "result": { "stopReason": "cancelled" } }));
        });
        let mut backend = Acp::with_spawner(Box::new(spawner));
        let mut conversation = Conversation::new();
        conversation.push(Speaker::Person, "add a shelf");
        backend.ask(&conversation);

        prompting.recv_timeout(PATIENCE).expect("the process was given the message");
        backend.cancel();
        let cancel = heard.recv_timeout(PATIENCE).expect("the process was told to stop");
        assert_eq!(cancel["method"], json!("session/cancel"));

        let mut answers = Vec::new();
        let deadline = Instant::now() + PATIENCE;
        while Instant::now() < deadline {
            if let Some(answer) = backend.take() {
                let last = match &answer {
                    Ok(reply) => !reply.more,
                    Err(_) => true,
                };
                answers.push(answer);
                if last {
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(
            answers.iter().any(|answer| matches!(answer, Ok(reply)
            if reply.lines.iter().any(|line| line.text == "stopping"))),
            "the words that did arrive were drawn"
        );
        assert_eq!(seen.complaint(), None);
    }

    #[test]
    fn a_process_is_granted_no_file_and_no_terminal_but_its_permission_is_allowed() {
        // The editor advertises neither capability, and a process that asks anyway is refused. A
        // permission question is answered with the first allowing option, because the window's own
        // gate — the person's plan switch — is what actually decides whether a call lands (ADR 0190).
        let (spawner, seen) = process(|peer| {
            let Some(session) = peer.open() else { return };
            peer.send(json!({
                "jsonrpc": "2.0",
                "id": 90,
                "method": "fs/read_text_file",
                "params": { "sessionId": session, "path": "/etc/passwd" },
            }));
            let Some(refusal) = peer.answer_to(90) else { return };
            peer.send(json!({
                "jsonrpc": "2.0",
                "id": 91,
                "method": "session/request_permission",
                "params": {
                    "sessionId": session,
                    "toolCall": { "toolCallId": "t1" },
                    "options": [
                        { "optionId": "no", "name": "Reject", "kind": "reject_once" },
                        { "optionId": "yes", "name": "Allow once", "kind": "allow_once" },
                    ],
                },
            }));
            let Some(allowed) = peer.answer_to(91) else { return };
            // The two answers are asserted from the test's side, so the script keeps them before it
            // lets the turn finish.
            peer.seen().keep(json!({ "refusal": refusal, "allowed": allowed }));
            let Some(prompt) = peer.read() else { return };
            peer.answer(&prompt, json!({ "stopReason": "end_turn" }));
        });
        let mut backend = Acp::with_spawner(Box::new(spawner));
        let _ = one_turn(&mut backend);

        let answers = seen.frames();
        let pair =
            answers.iter().find(|frame| frame.get("refusal").is_some()).expect("the script recorded both answers");
        let refusal = &pair["refusal"];
        assert_eq!(refusal["error"]["code"], json!(-32601), "a method this client does not offer: {refusal}");
        assert!(
            refusal["error"]["message"].as_str().unwrap_or_default().contains("no file and no terminal"),
            "{refusal}"
        );
        assert_eq!(pair["allowed"]["result"]["outcome"]["outcome"], json!("selected"));
        assert_eq!(pair["allowed"]["result"]["outcome"]["optionId"], json!("yes"), "the first option that allows");
        assert_eq!(seen.complaint(), None);
    }

    #[test]
    fn a_configured_program_that_does_not_exist_is_one_readable_sentence() {
        let mut backend = Acp::new(Program::new("z3l-no-such-agent-binary"));
        let mut conversation = Conversation::new();
        conversation.push(Speaker::Person, "add a shelf");
        backend.ask(&conversation);
        let deadline = Instant::now() + PATIENCE;
        let mut sentence = None;
        while Instant::now() < deadline {
            if let Some(answer) = backend.take() {
                sentence = Some(answer.expect_err("a program that does not exist cannot answer"));
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        let sentence = sentence.expect("the failure arrived");
        assert!(sentence.sentence().contains("did not start"), "{sentence}");
    }

    #[test]
    fn no_message_is_no_prompt() {
        // A conversation whose last line is not the person's asks the process nothing: an empty
        // prompt would be a message the agent answers out of nowhere.
        let asked = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&asked);
        let (spawner, _seen) = process(move |peer| {
            if peer.open().is_some() {
                let _ = counter.fetch_add(1, Ordering::SeqCst);
            }
            // Nothing else is read: the client must never prompt.
            std::thread::sleep(Duration::from_millis(50));
        });
        let mut backend = Acp::with_spawner(Box::new(spawner));
        let mut conversation = Conversation::new();
        conversation.push(Speaker::Agent, "let me think");
        backend.ask(&conversation);
        std::thread::sleep(Duration::from_millis(80));
        assert!(backend.take().is_none(), "an empty message produced no answer");
    }
}
