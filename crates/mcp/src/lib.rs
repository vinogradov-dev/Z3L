//! The editor as a tool server a client we do not ship can drive.
//!
//! The tools are a projection of the command registry, never a second list of what the program can
//! do: `command_list` reads [`engine::CommandRegistry::specs`], `command_run` and `command_batch`
//! hand their steps to [`engine::Session::batch`] — the same call the command line's recipe and a
//! chat turn make — `query` asks [`engine::Session::read`] the questions it already answers, and
//! `inspect` reads the mistakes the rules prove ([`engine::Session::inspect`]) and `render_preview`
//! answers the scene as one picture, drawn by the shell through the [`Previewer`] port this crate
//! holds and never implements (ADR 0228). Coarse tools rather than two per command id, because a
//! model that spends its budget on transport never reaches the document (ADR 0185, ADR 0191).
//!
//! This crate is a protocol adapter and holds no brain: no tool decides anything, and no command is
//! re-implemented here. It sits beside `ui` at layer 7 and depends on `engine`, `model` and `text`;
//! it is the only crate allowed to depend on `rmcp` and on `tokio`, and it is forbidden `egui`,
//! `wgpu` and `eframe` (`cargo xtask layers`, ADR 0186). Nothing in the editor awaits it: the
//! served session runs on its own current-thread executor, and every tool dispatches synchronously
//! inside it, so a person's window is never blocked by a client we do not control.
//!
//! The wire is the process's own standard input and output. Nothing listens: `z3l mcp serve` speaks
//! stdio and binds no port, which is what keeps ADR 0028's "the application does not listen" true
//! while a foreign client can still work headless.
//!
//! The second door is [`channel`]: a loopback socket with a per-run token, which is how an agent
//! attaches to a window a person is looking at (ADR 0188). It binds `127.0.0.1` and nothing else, it
//! refuses a connection that does not present the token before a byte is read as a command, and it
//! never touches the document itself: a tool call becomes a [`Call`] the host answers at a frame
//! boundary (ADR 0187).
//!
//! Changes when: a tool is added, a tool's answer changes shape, or the crate gains a dependency.

#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod acp;
pub mod channel;
pub mod chat;
pub mod inspect;
pub mod openai;
pub mod preview;
mod report;
pub mod server;
pub mod tools;

use std::fmt;

use model::Project;
use rmcp::ServerHandler;
use tokio::io::{AsyncRead, AsyncWrite};

pub use acp::{Acp, PROTOCOL_VERSION, Program, Scripted as ScriptedProcess, Spawner, Started};
pub use channel::{Answering, BridgeError, Call, Channel, ChannelError, Mailbox, Pending, Wake, bridge, queued};
pub use chat::{BackendError, ChatBackend, Conversation, Line, Reply, Scripted, Speaker, Turn};
pub use inspect::{Tab, Window};
pub use openai::{Endpoint, KEY_ENV, KEY_FILE, OpenAi, key};
pub use preview::{MAX_BYTES, MAX_SIDE, MIN_SIDE, PreviewSize, Previewer};
pub use report::PLAN_NOTE;
pub use server::{
    CommandServer, Door, LIST_BUDGET_BYTES, asking, dispatch, findings, listing, preview, serve_session, undo,
};
pub use tools::{
    Answer, BatchRequest, CommandListRequest, FindingsRequest, InspectRequest, PreviewRequest, RunRequest, StepRequest,
    TOOL_BATCH, TOOL_FINDINGS, TOOL_INSPECT, TOOL_LIST, TOOL_PREVIEW, TOOL_QUERY, TOOL_RUN, TOOL_UNDO, UndoRequest,
};

/// Where a served session writes the document a batch changed.
///
/// The crate holds no path and no file I/O: the shell owns `project::SceneFile`, its atomic write
/// and the sections a newer build left in the file, so it hands the server one implementation and
/// the server never learns a directory (`AGENTS.md` §4.11, ADR 0185).
pub trait SceneSink: fmt::Debug + Send + Sync {
    /// Writes `project`, or the sentence that says why it could not be written.
    ///
    /// A caller reads `Err` as: the document changed and the file did not, which the answer a tool
    /// returns says in as many words instead of claiming a save that failed.
    fn save(&self, project: &Project) -> Result<(), String>;
}

/// Why serving stopped: the runtime, the handshake or the transport.
#[derive(Debug)]
pub enum ServeError {
    /// The executor the protocol runs on cannot be started on this machine.
    Runtime(String),
    /// The client and the server could not agree to begin.
    Start(String),
    /// The connection ended with a failure rather than with the client closing it.
    End(String),
}

impl fmt::Display for ServeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Runtime(message) => write!(formatter, "cannot start the protocol runtime: {message}"),
            Self::Start(message) => write!(formatter, "the client could not begin a session: {message}"),
            Self::End(message) => write!(formatter, "the session ended: {message}"),
        }
    }
}

impl std::error::Error for ServeError {}

/// The one runtime this crate builds: current-thread, because the channel's thread and the headless
/// session each serve one document and a second thread would buy nothing but a lock to cross.
pub(crate) fn make_runtime() -> Result<tokio::runtime::Runtime, ServeError> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| ServeError::Runtime(error.to_string()))
}

/// Serves `server` over one pair of streams until the client closes them.
///
/// The pair is what the process's standard input and output are, and what a test hands in to drive
/// the server without a second process: the protocol is newline-delimited JSON over a byte stream,
/// so a pipe, a terminal and a loopback socket are the same thing to it.
pub async fn serve<R, W, S>(server: S, reader: R, writer: W) -> Result<(), ServeError>
where
    R: AsyncRead + Send + Unpin + 'static,
    W: AsyncWrite + Send + Unpin + 'static,
    S: ServerHandler,
{
    let running =
        rmcp::serve_server(server, (reader, writer)).await.map_err(|error| ServeError::Start(error.to_string()))?;
    running.waiting().await.map_err(|error| ServeError::End(error.to_string()))?;
    Ok(())
}

/// Serves `server` over this process's standard streams and blocks until the client closes them.
///
/// This is the one place a runtime is built, and it is a current-thread one: the session is the one
/// writer of the document, so a second thread would buy nothing but a lock to cross. The function
/// is synchronous on purpose — the composition root starts it before any window exists and has no
/// runtime of its own (ADR 0186).
pub fn serve_stdio(server: CommandServer) -> Result<(), ServeError> {
    make_runtime()?.block_on(stdio(server))
}

/// Serves `server` over this process's standard streams on the runtime the caller built.
async fn stdio(server: CommandServer) -> Result<(), ServeError> {
    let (reader, writer) = rmcp::transport::stdio();
    serve(server, reader, writer).await
}
