//! The local control channel: one loopback socket, one run token, and a mailbox to the host.
//!
//! This is the second door of ADR 0188, and the one that attaches an agent to a document a person is
//! looking at: a window has a session per tab and swaps the active one, so a server that owned a
//! session could never serve it. Instead the channel owns the socket, the token and the handshake,
//! and every tool call becomes a [`Call`] queued to the host, which answers at a frame boundary from
//! its own thread (`AGENTS.md` §4.11, ADR 0187). [`Pending::answer`] hides the `oneshot` behind it,
//! so the shell never names the runtime. A call that moves the document waits for the first frame
//! that has the window to itself — a gesture in hand delays it instead of refusing it — and a call
//! whose caller has gone before that is dropped instead of performed (ADR 0223).
//!
//! A queued call rings the host: a window paints on input, so a call that arrived while nothing
//! else happened used to wait in the queue for a pointer only a person can bring (X111). The host
//! hands in one wake (`Mailbox::watch`) — a window's `Context::request_repaint`, since this crate
//! knows no `egui` — and the socket rings it the moment a call is queued (ADR 0225).
//!
//! Three things bound the exposure, and each has a test (ADR 0188): the socket binds `127.0.0.1` on
//! an ephemeral port and nothing else, a connection that does not present the run token is refused
//! before a single byte is read as a command, and the token is written only into an owner-only file
//! in the settings directory, from which a client learns the port. The file is removed on a clean
//! exit: a reader that cannot connect reads "no window is open here", never "the window is broken".
//!
//! Changes when: the handshake, the token file, the wake a host installs or the set of calls a host
//! answers changes.

use std::fmt;
use std::io;
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use engine::Query;
use rmcp::RoleServer;
use rmcp::ServerHandler;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, ErrorData, ListToolsResult, PaginatedRequestParams, ServerCapabilities,
    ServerConfig,
};
use rmcp::service::{MaybeSendFuture, RequestContext};
use serde_json::{Map, Value, json};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener as AsyncListener;
use tokio::sync::{mpsc, oneshot};
use tokio::time::timeout;

use crate::ServeError;
use crate::server::{result, tool_error};
use crate::tools::{
    Answer, BatchRequest, CommandListRequest, FindingsRequest, PreviewRequest, RunRequest, TOOL_BATCH, TOOL_FINDINGS,
    TOOL_INSPECT, TOOL_LIST, TOOL_PREVIEW, TOOL_QUERY, TOOL_RUN, attached_definitions, parse,
};

/// The file the channel publishes its port and token in, inside the settings folder.
pub const FILE_NAME: &str = "channel.json";

/// The bytes of one run token: 256 bits from the operating system's generator.
const TOKEN_BYTES: usize = 32;

/// The characters the token takes on the wire, in hex: two per byte and nothing else.
const TOKEN_CHARS: usize = TOKEN_BYTES * 2;

/// The most a connection may send before its newline, so a hostile peer cannot grow the buffer.
const HANDSHAKE_CAP: u32 = 256;

/// How long a connection may take to present its token.
const HANDSHAKE_DEADLINE: Duration = Duration::from_secs(10);

/// How long a call waits for the host before the client is told the window is not answering.
const ANSWER_DEADLINE: Duration = Duration::from_secs(120);

/// How long a liveness probe waits for the loopback port, gate and nothing else.
const PROBE_DEADLINE: Duration = Duration::from_millis(250);

/// The line the server answers a good token with, and the one it refuses a bad one with.
const ACCEPTED: &[u8] = b"ok\n";
const REFUSED: &[u8] = b"refused\n";

/// The same good answer as the bridge reads it back, without the newline it was read with.
const ACCEPTED_TEXT: &str = "ok";

/// The sentence a call answers with when the host has gone away between the queue and the answer.
const GONE: &str = "the window is gone: the channel outlived the document it served";

/// The answer a call gives when the host did not answer before [`ANSWER_DEADLINE`].
///
/// It says what is *unknown* and never what the window was doing: the sentence that used to stand
/// here named a gesture, so a caller that had timed out while the window was merely idle read a
/// refusal for a call the window later performed — a change nobody could see, and a retry that built
/// a second body (X107). The deadline is the only fact the channel has, so it states that fact and
/// tells the caller what it must do before repeating the call.
fn silent() -> Answer {
    Answer::refused(format!(
        "the window did not answer within {} s: the call may or may not have landed, so read the \
         scene before repeating it — an answer that arrives after the deadline reaches nobody",
        ANSWER_DEADLINE.as_secs()
    ))
}

/// One request the host answers, at a frame boundary.
#[derive(Clone, Debug)]
pub enum Call {
    /// `command_list`: every command this build dispatches.
    List,
    /// `command_run` and `command_batch`: one batch of steps, exactly what `Session::batch` takes.
    Dispatch(BatchRequest),
    /// `query`: one question the read model already answers.
    Ask(Query),
    /// `inspect`: the mistakes the rules can prove, in the document a person is looking at.
    Findings,
    /// `render_preview`: the scene as one picture, drawn by the host that owns the device.
    Preview(PreviewRequest),
    /// `ui_inspect`: the window's own state.
    Inspect,
}

impl Call {
    /// The word a log line names this call by: what a caller asked for, in the protocol's own words.
    pub const fn word(&self) -> &'static str {
        match self {
            Self::List => "listing",
            Self::Dispatch(_) => "batch",
            Self::Ask(_) => "query",
            Self::Findings => "inspection",
            Self::Preview(_) => "preview",
            Self::Inspect => "window report",
        }
    }
}

/// What a host wants done when a call arrives, so its own frame can answer it.
///
/// This crate is forbidden `egui` (ADR 0186) and owns no window, so a host hands in one closure that
/// asks its frame for another one from any thread — a window passes
/// `Context::request_repaint`, which is what wakes a `winit` loop that has nothing to do.
pub type Wake = Arc<dyn Fn() + Send + Sync>;

/// The host's bell: rung by the socket when a call is queued, and watched once by the host.
///
/// A window paints on input, so a call that arrived while nothing else happened used to wait for a
/// pointer only a person can bring: the queue filled and no frame drained it (X111). The bell is the
/// one thing that turns "a call arrived" into "a frame is coming".
#[derive(Default)]
struct Bell {
    /// The host's wake, once it has told the mailbox where to ring.
    wake: Mutex<Option<Wake>>,
    /// A call was queued while no host was watching, so installing the wake rings at once: a window
    /// is never left asleep with a call waiting behind it.
    early: AtomicBool,
}

impl Bell {
    /// Queues `call` and rings the host, which is the one thing the socket and a test do.
    fn queue(&self, calls: &mpsc::UnboundedSender<Pending>, call: Call) -> Result<oneshot::Receiver<Answer>, Answer> {
        let (reply, answered) = oneshot::channel();
        calls.send(Pending { call, reply }).map_err(|_| Answer::refused(GONE))?;
        self.ring();
        Ok(answered)
    }

    /// Installs the host's wake, ringing it when a call arrived before anybody was watching.
    fn watch(&self, wake: Wake) {
        {
            let mut held = match self.wake.lock() {
                Ok(held) => held,
                // The panic that poisoned the lock belongs to the wake this replaces; the slot is
                // still the host's, so it is written as it would have been.
                Err(poisoned) => poisoned.into_inner(),
            };
            *held = Some(wake);
        }
        if self.early.swap(false, Ordering::SeqCst) {
            self.ring();
        }
    }

    /// Tells the host that a frame is needed, or remembers that it will be once it watches.
    fn ring(&self) {
        let wake = match self.wake.lock() {
            Ok(held) => held.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        };
        match wake {
            Some(wake) => wake(),
            None => self.early.store(true, Ordering::SeqCst),
        }
    }
}

impl fmt::Debug for Bell {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The wake is a closure, so the only fact worth printing is whether a host is watching.
        let watched = match self.wake.lock() {
            Ok(held) => held.is_some(),
            Err(poisoned) => poisoned.into_inner().is_some(),
        };
        formatter.debug_struct("Bell").field("watched", &watched).finish()
    }
}

/// A request waiting for the host, and the reply it is waiting on.
///
/// The host reads [`Self::call`], performs it against the document in front of the person, and
/// answers with [`Self::answer`]; dropping it instead tells the client the window is gone.
pub struct Pending {
    call: Call,
    reply: oneshot::Sender<Answer>,
}

impl Pending {
    /// What the host is asked to do.
    pub const fn call(&self) -> &Call {
        &self.call
    }

    /// Whether the client that asked has gone, so an answer would be written into nothing.
    ///
    /// A caller gives up at the channel's own [`ANSWER_DEADLINE`], and the host may still hold the
    /// call: performing it then would change the person's document for a caller that will never hear
    /// about it — the change a retry then builds twice (X107). The host reads this immediately before
    /// a call that can move the document and drops the call instead.
    pub fn abandoned(&self) -> bool {
        self.reply.is_closed()
    }

    /// Hands the answer to the client, or drops it when the client has already gone.
    pub fn answer(self, answer: Answer) {
        let _ = self.reply.send(answer);
    }
}

impl fmt::Debug for Pending {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("Pending").field("call", &self.call).finish_non_exhaustive()
    }
}

/// The host's end of the channel: what arrived since the last frame.
///
/// A frame takes everything waiting and answers it before it derives anything of its own, which is
/// the boundary ADR 0187 asks for: an assistant call cannot interleave with a drag, an armed tool or
/// an undo the person is performing.
#[derive(Debug)]
pub struct Mailbox {
    pending: mpsc::UnboundedReceiver<Pending>,
    /// The same queue, held so a host can reach it with no client — [`Self::post`] — and so the
    /// socket's end is never the only writer of the mailbox's own channel.
    sends: mpsc::UnboundedSender<Pending>,
    /// Rung when a call is queued: the difference between a window that paints and one that sleeps.
    bell: Arc<Bell>,
}

impl Mailbox {
    /// The next request waiting, without blocking.
    pub fn take(&mut self) -> Option<Pending> {
        self.pending.try_recv().ok()
    }

    /// Tells the mailbox what to do when a call arrives, which is how a window asks for a frame.
    ///
    /// A host sets it once, where it opens the channel, and the closure is called from the channel's
    /// own thread: the window's `Context::request_repaint` is what a live window passes (ADR 0225).
    pub fn watch(&self, wake: Wake) {
        self.bell.watch(wake);
    }

    /// Queues one call the way the socket queues it, and hands back what the host will answer.
    ///
    /// The seam a host's own test arrives through: a frame test drives the real mailbox and the real
    /// bell with no client, no socket and no runtime, so "a call woke the window" is asserted rather
    /// than hoped for (X111).
    #[doc(hidden)]
    pub fn post(&self, call: Call) -> Result<Answering, Answer> {
        self.bell.queue(&self.sends, call).map(Answering)
    }

    /// The next request, as a future: what a host with no frame to reach waits on.
    ///
    /// The window never uses this — it drains [`Self::drain`] at the top of every frame and would
    /// hang on a channel nobody is writing to — but a headless host has no frame and waits here.
    pub async fn wait(&mut self) -> Option<Pending> {
        self.pending.recv().await
    }

    /// Every request waiting now, so a frame answers them all and then works on its own state.
    ///
    /// The list is owned by the caller, so the frame can hold it while it borrows the document the
    /// answers are about.
    pub fn drain(&mut self) -> Vec<Pending> {
        std::iter::from_fn(|| self.take()).collect()
    }
}

/// One call and the answer a host gives it, apart from any socket.
///
/// This is the seam a host's own test drives: the socket, the token and the handshake are the
/// channel's own promise and are tested here, so a frame test can ask "does this frame answer the
/// queue?" without a second process, a client or a runtime.
#[doc(hidden)]
pub fn queued(call: Call) -> (Pending, Answering) {
    let (reply, answered) = oneshot::channel();
    (Pending { call, reply }, Answering(answered))
}

/// The other end of [`queued`]: what the host answered, read without a runtime.
#[doc(hidden)]
#[derive(Debug)]
pub struct Answering(oneshot::Receiver<Answer>);

impl Answering {
    /// The answer, once the host has given it; `None` when the request was dropped unanswered.
    ///
    /// It waits, so it is for a test that has already driven the frame that answers: a window never
    /// calls it.
    pub fn take(self) -> Option<Answer> {
        self.0.blocking_recv().ok()
    }
}

/// Why the channel could not be opened.
#[derive(Debug)]
pub enum ChannelError {
    /// The settings folder was not named, so there is nowhere to publish the port and token.
    NoDirectory,
    /// A channel is already published in this folder and it answers.
    Taken {
        /// The port the running channel holds.
        port: u16,
    },
    /// The socket, the file or the thread could not be created, with the reason.
    Failed(String),
}

impl fmt::Display for ChannelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDirectory => formatter.write_str("the settings folder is not known, so no token can be written"),
            Self::Taken { port } => {
                write!(formatter, "another window already holds the control channel on 127.0.0.1:{port}")
            }
            Self::Failed(reason) => write!(formatter, "the control channel could not start: {reason}"),
        }
    }
}

impl std::error::Error for ChannelError {}

/// Why a bridge could not carry the window's channel.
///
/// Every sentence says which of the three facts the caller needs: no window answers, the window
/// refused this run's token, or the pipe itself failed. It never carries a token (ADR 0188).
#[derive(Debug)]
pub enum BridgeError {
    /// No window is reachable at the port the settings folder names.
    NoWindow(PathBuf),
    /// A window answered and refused the token: the file belongs to another run.
    Refused(PathBuf),
    /// The runtime, the socket or the pipe failed, with the reason.
    Failed(String),
}

impl fmt::Display for BridgeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoWindow(file) => write!(
                formatter,
                "no Z3L window answers on the channel `{}` names: open the window and switch the local \
                 channel on in Settings",
                file.display()
            ),
            Self::Refused(file) => write!(
                formatter,
                "a Z3L window refused the token in `{}`: the file belongs to another run, so open the \
                 window again",
                file.display()
            ),
            Self::Failed(reason) => write!(formatter, "the channel could not be carried: {reason}"),
        }
    }
}

impl std::error::Error for BridgeError {}

/// Carries this process's standard streams into the window's channel and back, until either ends.
///
/// This is the fourth use of one door rather than a new one (ADR 0188): a program that speaks MCP
/// over its own stdio — an agent process the person already runs, handed the channel as one of its
/// MCP servers — is given `z3l mcp bridge` as that server's command, and every call it makes still
/// becomes a [`Call`] the window answers at a frame boundary. The bridge speaks no protocol of its
/// own: it presents the run token and then copies bytes, so it cannot grant a tool, refuse one, or
/// read a document.
///
/// The token comes from the file the window publishes, which is the way ADR 0188 says a client
/// learns the port: the process being bridged never sees the token in its own arguments or
/// environment.
pub fn bridge(directory: &Path) -> Result<(), BridgeError> {
    let runtime = crate::make_runtime().map_err(|error| BridgeError::Failed(error.to_string()))?;
    runtime.block_on(bridge_stream(directory, tokio::io::stdin(), tokio::io::stdout()))
}

/// [`bridge`] over one pair of streams, which is what a test hands in.
pub async fn bridge_stream<R, W>(directory: &Path, mut reader: R, mut writer: W) -> Result<(), BridgeError>
where
    R: AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    let file = Channel::file(directory);
    let published = read_published(&file).ok_or_else(|| BridgeError::NoWindow(file.clone()))?;
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, published.port));
    let mut socket = tokio::net::TcpStream::connect(address).await.map_err(|_| BridgeError::NoWindow(file.clone()))?;
    let presented = format!("{}\n", published.token);
    socket
        .write_all(presented.as_bytes())
        .await
        .map_err(|error| BridgeError::Failed(format!("present the token: {error}")))?;
    let answer = read_line(&mut socket).await.map_err(|error| BridgeError::Failed(error.to_string()))?;
    if answer != ACCEPTED_TEXT {
        return Err(BridgeError::Refused(file));
    }
    let (mut from_window, mut to_window) = socket.into_split();
    let up = async move {
        let carried = tokio::io::copy(&mut reader, &mut to_window).await;
        // The window reads until this end closes, so the end of the local input is said out loud.
        let _ = to_window.shutdown().await;
        carried
    };
    let down = async move {
        let carried = tokio::io::copy(&mut from_window, &mut writer).await;
        let _ = writer.flush().await;
        carried
    };
    tokio::pin!(up, down);
    // One side closing ends the bridge: a process that has finished its answer closes its own end,
    // and a window that has gone must not leave the process waiting on a pipe that will never move.
    let carried = tokio::select! {
        carried = &mut up => carried,
        carried = &mut down => carried,
    };
    carried.map_err(|error| BridgeError::Failed(error.to_string()))?;
    Ok(())
}

/// One running channel: its port, its token, its thread and the file that publishes it.
pub struct Channel {
    port: u16,
    token: String,
    stop: Option<oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
    file: PathBuf,
}

impl fmt::Debug for Channel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The token is never printed: a log line is the one place it could reach a bug report.
        formatter.debug_struct("Channel").field("port", &self.port).field("file", &self.file).finish()
    }
}

impl Channel {
    /// The file a folder publishes its channel in.
    pub fn file(directory: &Path) -> PathBuf {
        directory.join(FILE_NAME)
    }

    /// Binds the loopback port, mints a token, publishes both and starts the thread.
    ///
    /// A folder whose file names a channel that still answers is refused and left untouched: the
    /// setting is on by default, so a second window is the ordinary case, and the first window's
    /// channel is the one a client should reach (ADR 0188).
    pub fn open(directory: &Path) -> Result<(Self, Mailbox), ChannelError> {
        let file = Self::file(directory);
        if let Some(published) = read_published(&file)
            && answers(published.port)
        {
            return Err(ChannelError::Taken { port: published.port });
        }
        let address = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
        let listener = TcpListener::bind(address).map_err(|error| reason("bind", &error))?;
        let port = listener.local_addr().map_err(|error| reason("read the bound port", &error))?.port();
        listener.set_nonblocking(true).map_err(|error| reason("make the socket non-blocking", &error))?;
        let token = mint()?;
        let (calls, mailbox) = mpsc::unbounded_channel();
        let bell = Arc::new(Bell::default());
        publish(&file, port, &token)?;
        let served = token.clone();
        let rung = Arc::clone(&bell);
        let queued = calls.clone();
        let (stop, stopped) = oneshot::channel();
        let thread = std::thread::Builder::new()
            .name("z3l-channel".to_owned())
            .spawn(move || serve_loop(listener, served, calls, rung, stopped))
            .map_err(|error| reason("start the channel thread", &error))?;
        let mailbox = Mailbox { pending: mailbox, sends: queued, bell };
        Ok((Self { port, token, stop: Some(stop), thread: Some(thread), file }, mailbox))
    }

    /// The loopback port the channel answers on.
    pub const fn port(&self) -> u16 {
        self.port
    }

    /// The token a client must present, as the file publishes it.
    pub fn token(&self) -> &str {
        &self.token
    }

    /// The file that publishes this channel.
    pub fn published_at(&self) -> &Path {
        &self.file
    }

    /// Stops the thread and removes the file, which is what a clean exit owes a reader.
    pub fn shutdown(self) {
        drop(self);
    }
}

impl Drop for Channel {
    /// Releases the thread and the file even when a caller forgets [`Channel::shutdown`].
    ///
    /// The file is removed only when it is still this channel's: a second instance never removes the
    /// first one's token, and a channel that lost the file says so by answering nothing.
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        if read_published(&self.file).is_some_and(|published| published.token == self.token) {
            let _ = std::fs::remove_file(&self.file);
        }
    }
}

/// Everything a channel publishes about itself: where it answers and what it takes.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Published {
    port: u16,
    token: String,
}

/// The channel a file names, if the file is one of ours.
fn read_published(file: &Path) -> Option<Published> {
    let text = std::fs::read_to_string(file).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    let port = value.get("port")?.as_u64()?;
    let port = u16::try_from(port).ok()?;
    let token = value.get("token")?.as_str()?.to_owned();
    if token.len() != TOKEN_CHARS || !token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    Some(Published { port, token })
}

/// Whether something answers on the published loopback port.
fn answers(port: u16) -> bool {
    TcpStream::connect_timeout(&SocketAddr::from((Ipv4Addr::LOCALHOST, port)), PROBE_DEADLINE).is_ok()
}

/// Writes the port and token over the file, readable by the owner only.
fn publish(file: &Path, port: u16, token: &str) -> Result<(), ChannelError> {
    let directory = file.parent().ok_or(ChannelError::NoDirectory)?;
    std::fs::create_dir_all(directory).map_err(|error| reason("create the settings folder", &error))?;
    let body = json!({ "port": port, "token": token, "pid": std::process::id() }).to_string();
    let temp = file.with_extension("json.tmp");
    // A stale file is replaced rather than opened: the channel it named answered nothing.
    let _ = std::fs::remove_file(&temp);
    write_owner_only(&temp, body.as_bytes())?;
    if std::fs::rename(&temp, file).is_err() {
        // A platform that will not rename over an existing file: the stale one goes first.
        let _ = std::fs::remove_file(file);
        std::fs::rename(&temp, file).map_err(|error| reason("publish the token", &error))?;
    }
    Ok(())
}

/// Creates `path` readable by its owner only and writes `bytes` into it.
#[cfg(unix)]
fn write_owner_only(path: &Path, bytes: &[u8]) -> Result<(), ChannelError> {
    use std::io::Write as _;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|error| reason("create the token file", &error))?;
    file.set_permissions(std::fs::Permissions::from_mode(0o600))
        .map_err(|error| reason("restrict the token file", &error))?;
    file.write_all(bytes).map_err(|error| reason("write the token file", &error))?;
    file.sync_all().map_err(|error| reason("flush the token file", &error))
}

/// Creates `path` and writes `bytes` into it.
///
/// The file lives in the per-user settings folder, which the platform already restricts to the
/// account that owns it; the mode bits are the Unix half of the same promise.
#[cfg(not(unix))]
fn write_owner_only(path: &Path, bytes: &[u8]) -> Result<(), ChannelError> {
    use std::io::Write as _;

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| reason("create the token file", &error))?;
    file.write_all(bytes).map_err(|error| reason("write the token file", &error))?;
    file.sync_all().map_err(|error| reason("flush the token file", &error))
}

/// Mints one run token from the operating system's generator, in hex.
fn mint() -> Result<String, ChannelError> {
    let mut bytes = [0u8; TOKEN_BYTES];
    getrandom::fill(&mut bytes).map_err(|error| reason("mint the run token", &error))?;
    Ok(hex(&bytes))
}

/// Two characters per byte, lower case: what goes into the file and the handshake.
fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push(char::from(DIGITS[usize::from(byte >> 4)]));
    }
    for byte in bytes {
        text.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    text
}

/// Whether two tokens are the same, folded over every byte with no early return.
fn same(given: &str, expected: &str) -> bool {
    let given = given.as_bytes();
    let expected = expected.as_bytes();
    let mut difference = u8::from(given.len() != expected.len());
    for index in 0..expected.len() {
        difference |= given.get(index).copied().unwrap_or(0) ^ expected.get(index).copied().unwrap_or(0);
    }
    difference == 0
}

/// The accept loop: one task per connection, and a stop line the host can send.
fn serve_loop(
    listener: TcpListener,
    token: String,
    calls: mpsc::UnboundedSender<Pending>,
    bell: Arc<Bell>,
    mut stop: oneshot::Receiver<()>,
) {
    let runtime = match crate::make_runtime() {
        Ok(runtime) => runtime,
        Err(error) => {
            log::warn!("{error}");
            return;
        }
    };
    runtime.block_on(async move {
        let listener = match AsyncListener::from_std(listener) {
            Ok(listener) => listener,
            Err(error) => {
                log::warn!("the control channel could not take its socket: {error}");
                return;
            }
        };
        loop {
            tokio::select! {
                _ = &mut stop => break,
                accepted = listener.accept() => {
                    let Ok((stream, _peer)) = accepted else { continue };
                    let token = token.clone();
                    let calls = calls.clone();
                    let bell = Arc::clone(&bell);
                    tokio::spawn(async move {
                        if let Err(error) = connection(stream, &token, calls, &bell).await {
                            log::debug!("a control channel connection ended: {error}");
                        }
                    });
                }
            }
        }
    });
}

/// One connection: the token first, then the protocol on the same stream.
async fn connection(
    stream: tokio::net::TcpStream,
    token: &str,
    calls: mpsc::UnboundedSender<Pending>,
    bell: &Arc<Bell>,
) -> Result<(), ServeError> {
    let (mut reader, mut writer) = stream.into_split();
    let presented = match timeout(HANDSHAKE_DEADLINE, read_line(&mut reader)).await {
        Ok(Ok(presented)) => presented,
        // A peer that says nothing and leaves, and one that hangs: both are refused by closing.
        Ok(Err(error)) => return Err(ServeError::Start(error.to_string())),
        Err(_) => return Err(ServeError::Start("the token did not arrive in time".to_owned())),
    };
    if !same(&presented, token) {
        let _ = writer.write_all(REFUSED).await;
        let _ = writer.shutdown().await;
        return Ok(());
    }
    writer.write_all(ACCEPTED).await.map_err(|error| ServeError::Start(error.to_string()))?;
    crate::serve(AttachedServer { calls, bell: Arc::clone(bell) }, reader, writer).await
}

/// Reads one line of at most [`HANDSHAKE_CAP`] bytes, one byte at a time.
///
/// A byte at a time is the point: the token is read without over-reading, so the protocol's own
/// frames stay in the socket for the transport, and a peer cannot make the server allocate a line it
/// never finishes.
async fn read_line<R: AsyncRead + Unpin>(reader: &mut R) -> io::Result<String> {
    let mut line = String::new();
    for _ in 0..HANDSHAKE_CAP {
        let byte = reader.read_u8().await?;
        match byte {
            b'\n' => return Ok(line),
            b'\r' => {}
            byte if byte.is_ascii_graphic() => line.push(char::from(byte)),
            _ => break,
        }
    }
    // A peer that never finishes its line, or sends something that is not one: an empty token is
    // never the run's token, so the connection is refused by the compare that follows.
    Ok(String::new())
}

/// The served window: every tool becomes a call the host answers between frames.
struct AttachedServer {
    calls: mpsc::UnboundedSender<Pending>,
    /// Rung after every queued call, so the host paints the frame that answers it (X111).
    bell: Arc<Bell>,
}

impl AttachedServer {
    /// Queues one call, rings the host, and hands back the reply it will be answered with.
    fn queue(&self, call: Call) -> Result<oneshot::Receiver<Answer>, Answer> {
        self.bell.queue(&self.calls, call)
    }
}

/// The tool list of the attached channel: the headless tools and the window's own report.
const INSTRUCTIONS: &str = "\
These tools drive a running Z3L window: the document a person is looking at, not a file. Call \
`ui_inspect` first to learn which scenes are open, which one is in front and what is selected; \
`command_list` names every command this build dispatches with the JSON Schema of its parameters; \
`query` reads the scene and never writes; `render_preview` answers the picture the window draws. A change lands at the next frame of the window, so it is \
one undo entry the person can take back.";

impl ServerHandler for AttachedServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(INSTRUCTIONS)
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, ErrorData>> + MaybeSendFuture + '_ {
        std::future::ready(Ok(ListToolsResult::with_all_items(attached_definitions())))
    }

    fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<CallToolResponse, ErrorData>> + MaybeSendFuture + '_ {
        let name = request.name.to_string();
        let intent = intent(&name, request.arguments);
        async move {
            let call = intent?;
            let answer = match self.queue(call) {
                Err(answer) => answer,
                Ok(answered) => match timeout(ANSWER_DEADLINE, answered).await {
                    Ok(Ok(answer)) => answer,
                    Ok(Err(_)) => Answer::refused(GONE),
                    Err(_) => silent(),
                },
            };
            Ok(CallToolResponse::from(result(answer)))
        }
    }
}

/// Reads a tool call into the call the host performs, or refuses it as the protocol's own error.
fn intent(name: &str, arguments: Option<Map<String, Value>>) -> Result<Call, ErrorData> {
    let call = match name {
        TOOL_LIST => {
            let _: CommandListRequest = parse(TOOL_LIST, arguments).map_err(tool_error)?;
            Call::List
        }
        TOOL_RUN => {
            let request = parse::<RunRequest>(TOOL_RUN, arguments).map_err(tool_error)?;
            Call::Dispatch(request.batch())
        }
        TOOL_BATCH => {
            let request = parse::<BatchRequest>(TOOL_BATCH, arguments).map_err(tool_error)?;
            Call::Dispatch(request)
        }
        TOOL_QUERY => Call::Ask(parse::<Query>(TOOL_QUERY, arguments).map_err(tool_error)?),
        TOOL_FINDINGS => {
            let _: FindingsRequest = parse(TOOL_FINDINGS, arguments).map_err(tool_error)?;
            Call::Findings
        }
        TOOL_PREVIEW => Call::Preview(parse::<PreviewRequest>(TOOL_PREVIEW, arguments).map_err(tool_error)?),
        TOOL_INSPECT => Call::Inspect,
        other => {
            return Err(ErrorData::invalid_params(
                format!(
                    "no tool is named `{other}`; a window offers {TOOL_INSPECT}, {TOOL_LIST}, {TOOL_RUN}, \
                     {TOOL_BATCH}, {TOOL_QUERY}, {TOOL_FINDINGS}, {TOOL_PREVIEW}"
                ),
                None,
            ));
        }
    };
    Ok(call)
}

/// The sentence for a failure of the socket, the file or the thread, naming what failed.
fn reason(what: &str, error: &dyn fmt::Display) -> ChannelError {
    ChannelError::Failed(format!("{what}: {error}"))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    #[test]
    fn a_token_is_hex_and_differs_between_runs() {
        let first = mint().expect("the operating system gives bytes");
        let second = mint().expect("and gives them again");
        assert_eq!(first.len(), TOKEN_CHARS, "256 bits, two characters per byte");
        assert!(first.bytes().all(|byte| byte.is_ascii_hexdigit()), "{first}");
        assert_ne!(first, second, "two runs do not share a token");
    }

    #[test]
    fn the_token_compare_folds_every_byte_and_refuses_a_longer_or_shorter_one() {
        assert!(same("abc", "abc"));
        assert!(!same("abc", "abd"), "one byte past the start still differs");
        assert!(!same("ab", "abc"), "a shorter token is not a prefix match");
        assert!(!same("abcd", "abc"), "and neither is a longer one");
        assert!(!same("", "abc"), "an empty presentation is refused");
    }

    #[test]
    fn a_stale_file_is_not_a_running_channel() {
        let directory = testkit::scratch_dir("channel-stale");
        let file = Channel::file(&directory);
        let stale = json!({ "port": 1, "token": "a".repeat(TOKEN_CHARS) }).to_string();
        std::fs::write(&file, stale).expect("a stale file");
        assert!(read_published(&file).is_some(), "the file is one of ours");
        assert!(!answers(1), "and the port it names answers nothing");
        let (channel, _mailbox) = Channel::open(&directory).expect("a stale file is replaced");
        let published = read_published(&file).expect("the new channel publishes itself");
        assert_eq!(published.port, channel.port());
        assert_eq!(published.token, channel.token(), "the file names the channel that is up");
        channel.shutdown();
        assert!(!file.exists(), "a clean exit removes the file");
    }

    #[test]
    fn a_file_that_is_not_one_of_ours_is_ignored() {
        let directory = testkit::scratch_dir("channel-foreign");
        let file = Channel::file(&directory);
        std::fs::create_dir_all(&directory).expect("the folder");
        for text in ["", "{}", "not json", r#"{"port": "http", "token": "x"}"#, r#"{"port": 1, "token": "zz"}"#] {
            std::fs::write(&file, text).expect("a file");
            assert!(read_published(&file).is_none(), "{text:?} is not a channel");
        }
    }

    #[test]
    fn a_queued_call_rings_the_host_which_is_what_asks_a_window_for_a_frame() {
        // The window paints on input, so a call that arrives while nothing else happens must ring it:
        // otherwise the queue fills and no frame drains it (X111, ADR 0225). A call that arrives in
        // the moment between publishing the port and installing the wake is the same case one step
        // earlier, so installing the wake rings for it.
        let directory = testkit::scratch_dir("channel-ring");
        let (channel, mailbox) = Channel::open(&directory).expect("the channel opens");
        let rung = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counting = |clock: &std::sync::Arc<std::sync::atomic::AtomicUsize>| {
            let clock = std::sync::Arc::clone(clock);
            std::sync::Arc::new(move || {
                clock.fetch_add(1, Ordering::SeqCst);
            }) as Wake
        };

        let _answered = mailbox.post(Call::List).expect("the mailbox holds its own queue");
        assert_eq!(rung.load(Ordering::SeqCst), 0, "a call before the host watches has nobody to ring");
        mailbox.watch(counting(&rung));
        assert_eq!(rung.load(Ordering::SeqCst), 1, "the host is woken for the call it slept through");

        let _answered = mailbox.post(Call::Inspect).expect("a second call queues");
        assert_eq!(rung.load(Ordering::SeqCst), 2, "and every call after that rings at once");
        channel.shutdown();
    }

    /// A window of the test's own: a socket that demands the token and then echoes one frame.
    ///
    /// It is the handshake and the pipe in one place, with no rmcp and no document, so what the
    /// bridge is asserted for is exactly what it is: it presents the token and copies bytes.
    fn toy_window(listener: std::net::TcpListener, answer: &'static [u8]) -> std::thread::JoinHandle<String> {
        std::thread::spawn(move || {
            use std::io::{BufRead as _, BufReader, Read as _, Write as _};

            let (mut stream, _) = listener.accept().expect("the bridge connects");
            let mut reader = BufReader::new(stream.try_clone().expect("the socket clones for reading"));
            let mut seen = String::new();
            reader.read_line(&mut seen).expect("the token line");
            stream.write_all(answer).expect("the answer to the token");
            stream.flush().expect("flush");
            if answer == REFUSED {
                return seen;
            }
            let mut frame = [0u8; 6];
            reader.read_exact(&mut frame).expect("one frame");
            stream.write_all(&frame).expect("the echo");
            stream.flush().expect("flush");
            seen
        })
    }

    /// A settings folder publishing one loopback port and token, as a window does.
    fn publish_to(directory: &Path, port: u16, token: &str) {
        std::fs::create_dir_all(directory).expect("the folder");
        let body = json!({ "port": port, "token": token }).to_string();
        std::fs::write(Channel::file(directory), body).expect("the file");
    }

    #[test]
    fn the_bridge_presents_the_token_and_then_carries_bytes_both_ways() {
        // The program an agent process is handed speaks MCP over its own stdio; the window speaks it
        // over one loopback socket behind a run token. The bridge is the pipe between the two, and
        // it reads the token from the file the window published, so the token never enters the
        // bridged process's arguments or environment (ADR 0188).
        let runtime = crate::make_runtime().expect("a runtime");
        let directory = testkit::scratch_dir("channel-bridge");
        let token = mint().expect("the operating system gives bytes");
        let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("a loopback port");
        let port = listener.local_addr().expect("the bound port").port();
        publish_to(&directory, port, &token);
        let window = toy_window(listener, ACCEPTED);

        let (mut local, far) = tokio::io::duplex(64);
        let (reader, writer) = tokio::io::split(far);
        let (carried, echoed) = runtime.block_on(async move {
            let local_end = async move {
                local.write_all(b"hello\n").await.expect("a frame goes out");
                let mut echoed = [0u8; 6];
                local.read_exact(&mut echoed).await.expect("the answer comes back");
                drop(local);
                echoed
            };
            tokio::join!(bridge_stream(&directory, reader, writer), local_end)
        });
        carried.expect("the bridge carried the bytes");

        assert_eq!(&echoed, b"hello\n", "a frame crossed the bridge whole");
        let seen = window.join().expect("the window finished");
        assert_eq!(seen.trim_end(), token, "the bridge presented the published token");
    }

    #[test]
    fn a_folder_that_names_no_window_is_told_to_open_one() {
        // A stale file is the ordinary way this fails: the window closed and removed the token it
        // published, or the file names a port that answers nothing. The sentence says what to do
        // rather than naming a socket error (ADR 0188).
        let runtime = crate::make_runtime().expect("a runtime");
        let directory = testkit::scratch_dir("channel-bridge-none");
        let (first, second) = tokio::io::duplex(8);
        publish_to(&directory, 1, &mint().expect("a token"));
        let error = runtime.block_on(bridge_stream(&directory, first, second)).expect_err("nothing answers on port 1");
        assert!(matches!(error, BridgeError::NoWindow(_)), "{error:?}");
        assert!(error.to_string().contains("open the window"), "{error}");

        let empty = testkit::scratch_dir("channel-bridge-empty");
        let (first, second) = tokio::io::duplex(8);
        let error =
            runtime.block_on(bridge_stream(&empty, first, second)).expect_err("a folder with no file names no window");
        assert!(matches!(error, BridgeError::NoWindow(_)), "{error:?}");
    }

    #[test]
    fn a_window_that_refuses_the_token_is_named_rather_than_retried() {
        let runtime = crate::make_runtime().expect("a runtime");
        let directory = testkit::scratch_dir("channel-bridge-refused");
        let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("a loopback port");
        let port = listener.local_addr().expect("the bound port").port();
        publish_to(&directory, port, &mint().expect("a token"));
        let window = toy_window(listener, REFUSED);

        let (first, second) = tokio::io::duplex(8);
        let error =
            runtime.block_on(bridge_stream(&directory, first, second)).expect_err("a refused token ends the bridge");
        window.join().expect("the window finished");
        assert!(matches!(error, BridgeError::Refused(_)), "{error:?}");
        assert!(error.to_string().contains("refused the token"), "{error}");
    }

    #[cfg(unix)]
    #[test]
    fn the_token_file_is_readable_by_its_owner_alone() {
        use std::os::unix::fs::PermissionsExt as _;

        let directory = testkit::scratch_dir("channel-mode");
        let (channel, _mailbox) = Channel::open(&directory).expect("the channel opens");
        let mode = std::fs::metadata(channel.published_at()).expect("the file exists").permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "the token is the account's own: {mode:o}");
        channel.shutdown();
    }
}
