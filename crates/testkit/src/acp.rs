//! A scripted agent process: the other end of the Agent Client Protocol, on real pipes.
//!
//! A backend that starts a process is hard to test honestly: a stub that answers Rust values proves
//! the state machine and nothing about the wire. This helper is the wire — two operating-system
//! pipes, a thread that reads frames and writes them back — so the handshake, the session, the
//! prompt, the notifications and the cancellation are all exercised as the protocol has them, with
//! no binary to run, no network and no key (ADR 0230).
//!
//! The test writes the script; this module holds the peer. It knows nothing about `mcp`, so a crate
//! that uses it wraps the two stream halves in whatever its own start-of-process type takes.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

/// One frame of the protocol, which is one JSON value on one line.
pub type Frame = Value;

/// The protocol version a scripted peer answers the handshake with when it agrees.
pub const PROTOCOL_VERSION: u64 = 1;

/// The two halves of the client's end of the pipes: what the process says, and what is said to it.
pub type Streams = (Box<dyn BufRead + Send>, Box<dyn Write + Send>);

/// What a scripted process saw and what it complained about.
#[derive(Debug, Default)]
pub struct Seen {
    frames: Mutex<Vec<Frame>>,
    complaint: Mutex<Option<String>>,
}

impl Seen {
    /// Keeps one frame the peer read or wrote.
    pub fn keep(&self, frame: Frame) {
        match self.frames.lock() {
            Ok(mut frames) => frames.push(frame),
            Err(poisoned) => poisoned.into_inner().push(frame),
        }
    }

    /// Records that the script could not do what it was written to do.
    pub fn complain(&self, sentence: impl Into<String>) {
        let sentence = sentence.into();
        match self.complaint.lock() {
            Ok(mut held) => *held = Some(sentence),
            Err(poisoned) => *poisoned.into_inner() = Some(sentence),
        }
    }

    /// Every frame the peer saw, in order.
    pub fn frames(&self) -> Vec<Frame> {
        match self.frames.lock() {
            Ok(frames) => frames.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    /// The first frame the client sent with `method`, which is how a test asserts the wire.
    pub fn method(&self, method: &str) -> Option<Frame> {
        self.frames().into_iter().find(|frame| frame.get("method").and_then(Value::as_str) == Some(method))
    }

    /// What the script could not do, if anything: an empty answer is a script that ran as written.
    pub fn complaint(&self) -> Option<String> {
        match self.complaint.lock() {
            Ok(held) => held.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }
}

/// The scripted process itself: the streams it speaks on and the frames it saw.
pub struct Process {
    reader: Option<Box<dyn BufRead + Send>>,
    writer: Option<Box<dyn Write + Send>>,
    seen: Arc<Seen>,
}

impl Process {
    /// The streams for the backend that starts this process, or the sentence that says why not.
    ///
    /// The script runs on a thread of its own from the moment this returns, and it ends when the
    /// client closes the pipes or the script returns — which is also how a test makes a process die
    /// in the middle of a turn.
    pub fn start(&mut self) -> Result<Streams, String> {
        let reader = self.reader.take().ok_or_else(|| "the scripted process was already started".to_owned())?;
        let writer = self.writer.take().ok_or_else(|| "the scripted process was already started".to_owned())?;
        Ok((reader, writer))
    }

    /// What the script saw, kept after the process started.
    pub fn seen(&self) -> Arc<Seen> {
        Arc::clone(&self.seen)
    }
}

impl std::fmt::Debug for Process {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Process").field("started", &self.reader.is_none()).finish()
    }
}

/// Starts a scripted agent process, with `script` playing the agent.
///
/// The script is a plain function over the peer, so a test reads like the conversation it asserts.
/// A script that returns is a process that exited.
pub fn scripted(script: impl FnOnce(&mut Peer) + Send + 'static) -> Process {
    let seen = Arc::new(Seen::default());
    let held = Arc::clone(&seen);
    let (client_reads, peer_writes) = match std::io::pipe() {
        Ok(pair) => pair,
        Err(error) => {
            seen.complain(format!("no pipe: {error}"));
            return Process { reader: None, writer: None, seen };
        }
    };
    let (peer_reads, client_writes) = match std::io::pipe() {
        Ok(pair) => pair,
        Err(error) => {
            seen.complain(format!("no pipe: {error}"));
            return Process { reader: None, writer: None, seen };
        }
    };
    let mut peer = Peer {
        reader: BufReader::new(peer_reads),
        writer: peer_writes,
        seen: Arc::clone(&held),
        held: VecDeque::new(),
    };
    std::thread::spawn(move || script(&mut peer));
    Process { reader: Some(Box::new(BufReader::new(client_reads))), writer: Some(Box::new(client_writes)), seen: held }
}

/// The agent's own end: frames in, frames out, and what it read for whoever reads next.
pub struct Peer {
    reader: BufReader<std::io::PipeReader>,
    writer: std::io::PipeWriter,
    seen: Arc<Seen>,
    /// Frames read while looking for an answer, kept for the next read.
    held: VecDeque<Frame>,
}

impl Peer {
    /// The script's record of what it saw and what it could not do.
    pub const fn seen(&self) -> &Arc<Seen> {
        &self.seen
    }

    /// The next frame the client sent, or nothing when the stream ended.
    pub fn read(&mut self) -> Option<Frame> {
        if let Some(frame) = self.held.pop_front() {
            return Some(frame);
        }
        let mut line = String::new();
        match self.reader.read_line(&mut line) {
            Ok(0) => {
                self.seen.complain("the client closed the stream");
                None
            }
            Ok(_) => match serde_json::from_str::<Frame>(&line) {
                Ok(frame) => {
                    self.seen.keep(frame.clone());
                    Some(frame)
                }
                Err(error) => {
                    self.seen.complain(format!("the client sent a line that is not a frame: {error}"));
                    None
                }
            },
            Err(error) => {
                self.seen.complain(error.to_string());
                None
            }
        }
    }

    /// The answer to the request `wanted`, reading past the frames in between.
    ///
    /// The client's own requests arrive whenever they arrive — the prompt is sent the moment the
    /// session opens — so a script that expects one answer reads past them without losing them.
    pub fn answer_to(&mut self, wanted: u64) -> Option<Frame> {
        let mut skipped = Vec::new();
        let mut found = None;
        for _ in 0..32 {
            let Some(frame) = self.read() else { break };
            if frame.get("method").is_none() && frame.get("id").and_then(Value::as_u64) == Some(wanted) {
                found = Some(frame);
                break;
            }
            skipped.push(frame);
        }
        for frame in skipped.into_iter().rev() {
            self.held.push_front(frame);
        }
        if found.is_none() {
            self.seen.complain(format!("no answer to request {wanted} arrived"));
        }
        found
    }

    /// One frame to the client.
    pub fn send(&mut self, frame: Frame) {
        let mut line = frame.to_string();
        line.push('\n');
        if let Err(error) = self.writer.write_all(line.as_bytes()).and_then(|()| self.writer.flush()) {
            self.seen.complain(error.to_string());
        }
    }

    /// The answer to the request in `frame`, which is what a peer does with every request.
    pub fn answer(&mut self, frame: &Frame, result: Frame) {
        let id = frame.get("id").cloned().unwrap_or(Value::Null);
        self.send(json!({ "jsonrpc": "2.0", "id": id, "result": result }));
    }

    /// One `session/update` notification for `session`.
    pub fn update(&mut self, session: &str, update: Frame) {
        self.send(json!({
            "jsonrpc": "2.0",
            "method": "session/update",
            "params": { "sessionId": session, "update": update },
        }));
    }

    /// One chunk of the agent's own words.
    pub fn say(&mut self, session: &str, text: &str) {
        self.update(
            session,
            json!({ "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": text } }),
        );
    }

    /// The handshake and the session, which every script does the same way: the answer is the id.
    pub fn open(&mut self) -> Option<String> {
        let initialize = self.read()?;
        self.answer(&initialize, json!({ "protocolVersion": PROTOCOL_VERSION }));
        let new = self.read()?;
        self.answer(&new, json!({ "sessionId": "s1" }));
        Some("s1".to_owned())
    }
}
