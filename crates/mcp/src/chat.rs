//! A chat turn: one backend, one dispatch at a time, and the person's stop between them.
//!
//! This is the port half of the assistant (ADR 0028, ADR 0186): the panel lives in the shell and
//! this module holds the conversation, the [`ChatBackend`] a model arrives through and the loop that
//! turns one answer into document changes. Nothing here draws and nothing here reaches the document:
//! a turn asks the backend for a reply, a reply that names commands becomes one batch, and the
//! shell — which owns the session a person is looking at — runs that batch through
//! [`engine::Session::agent_batch_in`] at a frame boundary (ADR 0187).
//!
//! Three promises the shape keeps, each with a test:
//!
//! - **One message is one undo entry.** The shell mints one edit scope per turn and every batch of
//!   that turn lands inside it, so `undo` takes the whole answer back in one step and a turn that
//!   dispatched nothing leaves no entry at all.
//! - **The stop is between calls.** A turn asks for at most one reply per frame and dispatches at
//!   most one batch per frame, so a person's stop lands between two calls: the document is exactly
//!   what the last completed batch left, never a command cut in half (a batch is all or nothing).
//! - **A backend that is still thinking is not a blocked frame.** [`ChatBackend::ask`] returns at
//!   once and the answer arrives through [`ChatBackend::take`], which is what lets the blocking HTTP
//!   adapter of ADR 0189 do its work on its own thread without the window waiting for it.
//!
//! The conversation is chat state: it is never serialised into the scene, never enters the history
//! and dies with the tab (ADR 0187 §0052, ADR 0028).
//!
//! Changes when: the turn's promises change, or a second backend kind appears.

use std::collections::VecDeque;
use std::fmt;

use engine::{BatchEffect, BatchReport, BatchStep};
use serde_json::{Value, json};
use text::{Lang, Message};
use thiserror::Error;

use crate::tools::{Answer, TOOL_BATCH};

/// The tool a model calls to make one batch of changes.
///
/// It is [`TOOL_BATCH`]'s id on purpose: one act has one name on both doors, and only its arguments
/// differ. The turn owns the name because the turn is what runs the batch.
pub const BATCH_TOOL: &str = TOOL_BATCH;

/// The tool a model calls to publish the steps it is working through.
pub const CHECKLIST_TOOL: &str = "todo_write";

/// How much of a call's arguments the panel writes on the line that shows the work.
///
/// A read is one small object; the cap is for the one that is not, because a line is read at a
/// glance and the whole argument is in the transcript the model holds.
const TOOL_LINE_CHARS: usize = 120;

/// What a turn answers a batch it has already run in this turn.
///
/// The same steps asked twice in one turn is the model repeating itself, not a second part: a box
/// that landed once does not land again, and saying so is what lets the model speak instead of
/// building a duplicate (X130).
const REPEAT_SENTENCE: &str = "these steps already ran in this turn; nothing was dispatched a second time";

/// The most backend replies one turn may ask for, so a model that loops cannot run forever.
///
/// It is a call budget and not a deadline (ADR 0187 leaves both to settings): a refusal is a result,
/// and the budget is what stops a model that keeps retrying one.
pub const MAX_CALLS: u32 = 12;

/// Who said one line of the conversation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Speaker {
    /// The person at the window.
    Person,
    /// The model.
    Agent,
    /// A tool call the agent made, in the agent's own words for it.
    ///
    /// It exists because a backend that owns the model loop does not answer with a batch of ours:
    /// the process decides which tool to call and calls it over the channel itself, so the panel
    /// can only report what the process said it was doing (ADR 0230). The panel draws it quieter
    /// than a message, because it is work and not speech.
    Tool,
    /// The turn's own report: what a batch did, why it was refused, why the turn stopped.
    ///
    /// It is the shell's voice, so it is a [`Message`] like every other word of the interface.
    Note,
}

/// One line of the conversation, in the words the interface draws.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    /// Who said it.
    pub speaker: Speaker,
    /// What was said.
    pub text: String,
}

/// One tool call as the model made it: the tool it named, the arguments it passed and the id the
/// wire answers it by.
///
/// The id is the wire's and it is what ties an answer to its call when the history goes back. The
/// name decides who answers: [`BATCH_TOOL`] is the turn's, and every other name is a read the door
/// answers with the same functions the tool server uses — so an unknown name comes back as a
/// refusal the model can read instead of an error that ends the turn.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolCall {
    /// The wire's id for this call, echoed on its answer.
    pub id: String,
    /// The tool the model called.
    pub name: String,
    /// The arguments it passed, as the object the published schema describes.
    pub arguments: Value,
    /// The sentence the call's own arguments could not be read, when they could not.
    ///
    /// A model that mistypes one key is answered with the sentence that names it, in the same turn,
    /// instead of a turn that ends on a transport complaint: the refusal is the model's to fix.
    pub fault: Option<String>,
}

impl ToolCall {
    /// A call with the wire's id, the tool's name and its arguments.
    pub fn new(id: impl Into<String>, name: impl Into<String>, arguments: Value) -> Self {
        Self { id: id.into(), name: name.into(), arguments, fault: None }
    }

    /// The same call, whose arguments could not be read, with the sentence that says why.
    pub fn faulted(mut self, sentence: impl Into<String>) -> Self {
        self.fault = Some(sentence.into());
        self
    }
}

/// One conversation: every line the person reads and every message the wire carries.
///
/// It is owned by the shell, before, during and after a turn, so the backend reads the same history
/// a person does and a stopped turn leaves the words it already said.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Conversation {
    lines: Vec<Line>,
    /// The same conversation as the wire carries it: one message per line, plus the tool calls and
    /// their answers.
    ///
    /// It is kept beside the lines rather than derived from them because a tool call is not a line:
    /// the model's own call and the program's answer to it are two messages that a person reads as
    /// one dim note. Replaying them is what stops a model from planning the same build again — with
    /// the calls dropped, the history says "the program reports **a refusal**" and nothing about
    /// what was asked (X130).
    messages: Vec<Value>,
    /// The live scene the shell hands the model before it asks, empty where the backend reads the
    /// scene itself. It is a system note, not a transcript line: the person never sees it, and it is
    /// replaced for every message instead of growing the history.
    brief: String,
    /// Which drawn line is the answer still arriving, so the words of one streamed answer grow in
    /// place instead of arriving as a label per chunk.
    ///
    /// It is an index and not a flag because a tool line may be drawn between two pieces of text:
    /// the piece after it starts a line of its own, and the one before it keeps the text it had.
    open: Option<usize>,
}

impl Conversation {
    /// An empty conversation.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds one line, to the panel and to the history the model reads.
    pub fn push(&mut self, speaker: Speaker, text: impl Into<String>) {
        let text = text.into();
        self.messages.push(message_of(speaker, &text));
        self.lines.push(Line { speaker, text });
        self.open = None;
    }

    /// Adds one line the panel draws and the history does not carry.
    ///
    /// It is how the program's own report of a batch travels: the model already holds the same
    /// sentence as the answer to its call, and a second copy as a `user` turn would be the program
    /// talking to itself. A tool call and its answer are the same case ([`Self::push_answer`],
    /// [`Self::push_tool_result`]).
    pub fn push_line(&mut self, speaker: Speaker, text: impl Into<String>) {
        self.lines.push(Line { speaker, text: text.into() });
    }

    /// Adds lines the backend already wrote, in their own order.
    ///
    /// A backend that speaks a protocol of its own reports what it said and what it did as lines of
    /// this conversation, so the panel draws one transcript whichever backend is behind it. They are
    /// drawn only: that backend writes its own history.
    pub fn extend(&mut self, lines: Vec<Line>) {
        for line in lines {
            self.push_line(line.speaker, line.text);
        }
    }

    /// Every line, oldest first.
    pub fn lines(&self) -> &[Line] {
        &self.lines
    }

    /// The conversation as the wire sends it back, oldest message first.
    pub fn messages(&self) -> &[Value] {
        &self.messages
    }

    /// Records the model's own answer: its words and the calls it made, as one message.
    ///
    /// An answer that names a tool is one assistant message carrying both, which is what the wire
    /// expects and what the model reads back: its own words and what it asked for.
    pub fn push_answer(&mut self, reply: &Reply) {
        // The words go into the line this answer is drawn in, whether it is still arriving or whole:
        // a piece carries what was said so far, so the line grows instead of a label appearing per
        // token, and the whole answer replaces it with the same text it ended on (J26).
        if !reply.text.trim().is_empty() {
            self.draw_so_far(&reply.text);
        }
        if reply.more {
            // Nothing enters the history yet: the wire says one assistant message per answer, and
            // half a sentence is not one.
            return;
        }
        self.open = None;
        if reply.tool_calls.is_empty() {
            if !reply.text.trim().is_empty() {
                self.messages.push(message_of(Speaker::Agent, &reply.text));
            }
            return;
        }
        let calls: Vec<Value> = reply.tool_calls.iter().map(tool_call_message).collect();
        let content = if reply.text.trim().is_empty() { Value::Null } else { Value::String(reply.text.clone()) };
        self.messages.push(json!({
            "role": "assistant",
            "content": content,
            "tool_calls": calls,
        }));
    }

    /// Draws the words of an answer still arriving, in the line that answer is growing in.
    ///
    /// The piece carries the whole of what was said so far rather than the delta, so this replaces
    /// the open line's text instead of appending: a backend that repeats itself, or a frame that
    /// takes two pieces at once, still leaves one answer on the screen.
    fn draw_so_far(&mut self, text: &str) {
        if let Some(line) = self.open.and_then(|open| self.lines.get_mut(open))
            && line.speaker == Speaker::Agent
        {
            line.text = text.to_owned();
            return;
        }
        self.lines.push(Line { speaker: Speaker::Agent, text: text.to_owned() });
        self.open = self.lines.len().checked_sub(1);
    }

    /// Records what one tool call answered, tied to it by the wire's id.
    pub fn push_tool_result(&mut self, call: &ToolCall, answer: &Value) {
        self.messages.push(json!({
            "role": "tool",
            "tool_call_id": call.id,
            "content": answer.to_string(),
        }));
    }

    /// Forgets everything, which is what closing the tab does with it.
    pub fn clear(&mut self) {
        self.lines.clear();
        self.messages.clear();
        self.brief.clear();
        self.open = None;
    }

    /// Whether the conversation holds nothing yet.
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Names the live scene the backend should read before its next answer.
    pub fn set_brief(&mut self, brief: impl Into<String>) {
        self.brief = brief.into();
    }

    /// The live scene, or an empty string where the backend reads it through its own tools.
    pub fn brief(&self) -> &str {
        &self.brief
    }
}

/// What the model answered: words for the person and, when it acts, one batch of commands.
///
/// The batch is the model's whole answer, not one command of it: a turn that needs twenty commands
/// sends them as one list, so the history stays one entry per message (ADR 0187). The checklist is
/// the turn's `todo_write` items — chat state that is never undoable and never serialised
/// (ADR 0190).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reply {
    /// What the model says. Empty when the answer is only a call.
    pub text: String,
    /// The commands the answer asks for, in order; empty when the answer only speaks.
    pub calls: Vec<BatchStep>,
    /// The steps the model says it is working through, shown beside the conversation.
    pub checklist: Vec<String>,
    /// Every tool call the answer made, in the order it made them.
    ///
    /// They are what the history is sent back with: the batch is one of them and a read is another,
    /// and a call whose arguments could not be read carries its sentence in
    /// [`ToolCall::fault`] so the turn answers it instead of ending.
    pub tool_calls: Vec<ToolCall>,
    /// Lines that are not the model speaking: a tool call it made and what came of it.
    ///
    /// A backend that owns the model loop reports its work through these instead of through
    /// [`Self::calls`], because the tools are the server's and the process calls them itself
    /// (ADR 0230). They are appended after [`Self::text`] and in the order the agent said them.
    pub lines: Vec<Line>,
    /// Whether this is part of an answer still being spoken rather than the answer.
    ///
    /// A streaming backend hands a chunk over as soon as it arrives, so the person watches the
    /// agent talk instead of a spinner. The request it answers stays in flight, which is what keeps
    /// [`Turn`] from asking again: only the last reply of an answer ends anything.
    ///
    /// The two halves of a piece mean different things and both are drawn at once:
    ///
    /// - [`Self::text`] is the answer **so far** and not the delta, so the panel replaces the line
    ///   it drew instead of adding one per token;
    /// - [`Self::lines`] are pieces drawn as they come and never replaced, which is how a backend
    ///   that owns the loop reports a tool it called between two sentences.
    ///
    /// Nothing enters the wire's history until the answer is whole: a model reads one assistant
    /// message per answer.
    pub more: bool,
}

impl Reply {
    /// An answer in words alone.
    pub fn saying(text: impl Into<String>) -> Self {
        Self { text: text.into(), ..Self::default() }
    }

    /// An answer that asks for one batch, shaped as the wire would have carried it.
    pub fn calling(calls: Vec<BatchStep>) -> Self {
        let arguments = json!({ "steps": calls.iter().map(step_message).collect::<Vec<_>>() });
        let call = ToolCall::new("call-batch", BATCH_TOOL, arguments);
        Self { calls, tool_calls: vec![call], ..Self::default() }
    }

    /// An answer that asked to read one thing, shaped as the wire would have carried it.
    pub fn reading(name: impl Into<String>, arguments: Value) -> Self {
        let name = name.into();
        let call = ToolCall::new(format!("call-{name}"), name, arguments);
        Self { tool_calls: vec![call], ..Self::default() }
    }

    /// One more piece of an answer still arriving: the lines it said since the last piece.
    pub fn part(lines: Vec<Line>) -> Self {
        Self { lines, more: true, ..Self::default() }
    }

    /// The words of an answer still arriving, as much of them as has arrived.
    ///
    /// The text is the whole answer so far, never the delta, so the panel draws one growing
    /// sentence instead of a label per token (J26).
    pub fn saying_so_far(text: impl Into<String>) -> Self {
        Self { text: text.into(), more: true, ..Self::default() }
    }
}

/// Why a backend could not answer.
///
/// It is one sentence the panel draws, so it names what failed and never carries a key, a token or
/// a request body (ADR 0189).
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[error("{0}")]
pub struct BackendError(String);

impl BackendError {
    /// A failure with the sentence a person reads.
    pub fn new(sentence: impl Into<String>) -> Self {
        Self(sentence.into())
    }

    /// The sentence.
    pub fn sentence(&self) -> &str {
        &self.0
    }
}

/// The model behind the panel: one request at a time, answered out of band.
///
/// The two methods are a request and its answer rather than one blocking call, because a real
/// backend waits on a network and the window must not (ADR 0189): `ask` starts the work and returns,
/// and the frame asks `take` until the answer is there. A scripted backend answers immediately.
pub trait ChatBackend: fmt::Debug + Send {
    /// Starts one request against `transcript`, replacing any request still in flight.
    fn ask(&mut self, transcript: &Conversation);

    /// The answer to the request in flight, or `None` while it is still coming.
    fn take(&mut self) -> Option<Result<Reply, BackendError>>;

    /// Whether the backend has an answer to hand over, so a frame can tell "working" from "ready"
    /// without spending the answer.
    fn waiting(&self) -> bool {
        true
    }

    /// Tells the backend to abandon the request in flight, at its next safe point.
    ///
    /// A backend that owns the model loop runs work this side cannot interrupt, so the person's
    /// stop has to travel: the process is asked to cancel and answers with what it had already
    /// done. A backend that answers one request at a time needs nothing.
    fn cancel(&mut self) {}
}

/// What a turn may do besides ask the model: run one batch and answer one read call.
///
/// The turn never touches a session. The shell owns the document, so it answers both — a batch
/// through `Session::agent_batch_in` and a read through the same functions the tool server
/// publishes — which is what keeps one projection of the registry on every door (ADR 0185,
/// ADR 0191). A scripted door in a test answers the same two calls with values it chose.
pub trait Tools {
    /// Runs the steps of one `command_batch` answer and reports what it did.
    fn dispatch(&mut self, steps: &[BatchStep]) -> BatchReport;
    /// Answers one read call, as the tool server would.
    fn read(&mut self, call: &ToolCall) -> Answer;
}

/// A backend that answers from a fixed script, so a whole turn runs in a test without a network.
///
/// It is the first backend the tree has (ADR 0187: "a scripted model must run a whole task in
/// tests"): a test hands in the answers a model would have produced and the frame drives them
/// through the real turn, the real session and the real undo history. A script that answers in
/// pieces writes them in order after the piece that names the answer: everything between one
/// non-`more` reply and the next belongs to the same request, exactly as a stream does (J26).
#[derive(Clone, Debug, Default)]
pub struct Scripted {
    answers: VecDeque<Result<Reply, BackendError>>,
    pending: Option<Result<Reply, BackendError>>,
}

impl Scripted {
    /// A backend that answers with `answers`, in order.
    pub fn new(answers: Vec<Reply>) -> Self {
        Self { answers: answers.into_iter().map(Ok).collect(), pending: None }
    }

    /// The same, where one answer is a failure instead of a reply.
    pub fn with_failure(answers: Vec<Reply>, failure: BackendError) -> Self {
        let mut scripted = Self::new(answers);
        scripted.answers.push_back(Err(failure));
        scripted
    }

    /// How many answers are left, which a test reads instead of counting the frames.
    pub fn remaining(&self) -> usize {
        self.answers.len()
    }
}

impl ChatBackend for Scripted {
    fn ask(&mut self, _transcript: &Conversation) {
        // A script that ran out says so instead of leaving the turn waiting forever for an answer
        // no backend will ever send.
        self.pending = Some(self.next_answer());
    }

    fn take(&mut self) -> Option<Result<Reply, BackendError>> {
        let answer = self.pending.take()?;
        // A piece of an answer keeps the request in flight, so the next piece of the script is what
        // the backend hands over next: a scripted stream needs no second request (J26).
        if answer.as_ref().is_ok_and(|reply| reply.more) {
            self.pending = Some(self.next_answer());
        }
        Some(answer)
    }

    fn waiting(&self) -> bool {
        self.pending.is_none()
    }
}

impl Scripted {
    /// The answer the script holds next, or the sentence that says it has none.
    fn next_answer(&mut self) -> Result<Reply, BackendError> {
        self.answers.pop_front().unwrap_or_else(|| Err(BackendError::new("the scripted backend holds no more answers")))
    }
}

/// One turn of the conversation: the answers it has asked for, the batch each one named, and the
/// person's stop.
///
/// The shell drives it one step per frame ([`Turn::advance`]) and owns the backend, the conversation
/// and the session; the turn owns only its own progress, so a stopped turn drops with the panel and
/// leaves the document to the history that already holds it.
#[derive(Debug)]
pub struct Turn {
    calls: u32,
    budget: u32,
    asked: bool,
    stopped: bool,
    done: bool,
    checklist: Vec<String>,
    /// The batch this turn has already dispatched, so an answer that repeats it verbatim does not
    /// build the same parts twice.
    ///
    /// Only a batch that ran is kept: a refused one may be corrected and asked again, which is what
    /// the refusal sentence is for.
    landed: Option<Vec<BatchStep>>,
    /// What the turn last did, named for the progress line the panel draws while it runs.
    ///
    /// It is the last step the agent dispatched or the tool it read with, in the agent's own words:
    /// a person waiting for an answer reads `panel.create` and knows what is happening, where a
    /// spinner tells them nothing (J26).
    current: Option<String>,
}

impl Default for Turn {
    fn default() -> Self {
        Self::new()
    }
}

impl Turn {
    /// A turn that has not asked anything yet.
    pub fn new() -> Self {
        Self {
            calls: 0,
            budget: MAX_CALLS,
            asked: false,
            stopped: false,
            done: false,
            checklist: Vec::new(),
            landed: None,
            current: None,
        }
    }

    /// The same turn with a budget of `calls` replies, which a test uses to reach the limit quickly.
    pub fn with_budget(mut self, calls: u32) -> Self {
        self.budget = calls;
        self
    }

    /// The person's stop. It lands at the next call boundary, never inside a batch.
    pub fn stop(&mut self) {
        self.stopped = true;
    }

    /// Whether the person stopped it.
    pub fn is_stopped(&self) -> bool {
        self.stopped
    }

    /// Whether the turn is over: the model spoke and asked for nothing, the backend failed, the
    /// budget ran out or the person stopped it.
    pub fn is_done(&self) -> bool {
        self.done
    }

    /// Whether the turn is waiting for the backend's answer.
    pub fn is_asking(&self) -> bool {
        self.asked && !self.done
    }

    /// The checklist the model last sent, in its own order.
    pub fn checklist(&self) -> &[String] {
        &self.checklist
    }

    /// How many replies the turn has taken, which the panel prints as "3/12" if it ever needs to.
    pub fn calls(&self) -> u32 {
        self.calls
    }

    /// What the turn last did — the step it dispatched or the read it made — while it runs.
    ///
    /// `None` before the first answer names anything, which is the state a person sees while the
    /// model is still thinking.
    pub fn current(&self) -> Option<&str> {
        self.current.as_deref()
    }

    /// One step of the turn, at a frame boundary.
    ///
    /// Exactly one of four things happens: the person's stop ends the turn without asking anything
    /// more, an answer that arrived is taken and its calls answered, a reply is asked for, or the
    /// budget ends the turn. `tools` runs one batch against the document the person is looking at
    /// and answers a read; the turn never touches a session itself.
    pub fn advance(
        &mut self,
        backend: &mut dyn ChatBackend,
        conversation: &mut Conversation,
        lang: Lang,
        tools: &mut dyn Tools,
    ) {
        if self.done {
            return;
        }
        if self.stopped {
            backend.cancel();
            conversation.push(Speaker::Note, Message::AssistantStoppedLine.text(lang));
            self.done = true;
            return;
        }
        if self.asked {
            let Some(answer) = backend.take() else {
                return;
            };
            self.asked = false;
            match answer {
                Err(error) => {
                    conversation.push(Speaker::Note, error.sentence().to_owned());
                    self.done = true;
                }
                Ok(reply) => {
                    self.receive(reply, conversation, lang, tools);
                }
            }
            return;
        }
        if self.calls >= self.budget {
            conversation.push(Speaker::Note, Message::AssistantBudgetLine.text(lang));
            self.done = true;
            return;
        }
        backend.ask(conversation);
        self.asked = true;
        self.calls = self.calls.saturating_add(1);
    }

    /// Takes one answer: its words, its checklist, its reads and, when it names them, its batch.
    ///
    /// Every call is answered, in the order the model made it, and the answer goes into the history
    /// tied to the call by the wire's id: a read that came before the batch is a schema the build
    /// used, and a batch that came before a read is what a verification read sees (ADR 0185). A
    /// piece of an answer still arriving appends what it says and leaves the request in flight,
    /// which is how a process that streams its work is watched instead of waited for (ADR 0230).
    fn receive(&mut self, reply: Reply, conversation: &mut Conversation, lang: Lang, tools: &mut dyn Tools) {
        conversation.push_answer(&reply);
        conversation.extend(reply.lines.clone());
        if reply.more {
            self.asked = true;
            return;
        }
        self.checklist = reply.checklist;
        if reply.tool_calls.is_empty() {
            // The model spoke and asked for nothing: the answer is the answer.
            self.done = true;
            return;
        }
        let mut dispatched = false;
        for call in reply.tool_calls.iter() {
            if let Some(fault) = &call.fault {
                conversation.push_line(Speaker::Tool, tool_line(call));
                conversation.push_tool_result(call, &json!({ "error": fault }));
                continue;
            }
            match call.name.as_str() {
                CHECKLIST_TOOL => {
                    conversation.push_tool_result(call, &json!({ "ok": true }));
                }
                BATCH_TOOL => {
                    // One batch per answer: `Reply::calls` is every step the answer named, so a
                    // second batch call in one message would run them twice.
                    if dispatched || self.landed.as_deref() == Some(reply.calls.as_slice()) {
                        conversation.push_line(Speaker::Tool, tool_line(call));
                        conversation.push_line(Speaker::Note, Message::AssistantRepeatedLine.text(lang));
                        conversation.push_tool_result(call, &json!({ "note": REPEAT_SENTENCE }));
                        continue;
                    }
                    dispatched = true;
                    if let Some(last) = reply.calls.last() {
                        self.current = Some(last.command().to_owned());
                    }
                    let report = tools.dispatch(&reply.calls);
                    let sentence = describe(&report, lang);
                    conversation.push_line(Speaker::Tool, tool_line(call));
                    conversation.push_line(Speaker::Note, sentence.clone());
                    conversation.push_tool_result(call, &json!({ "note": sentence }));
                    if !matches!(report.effect(), BatchEffect::RolledBack) {
                        self.landed = Some(reply.calls.clone());
                    }
                }
                _ => {
                    let answer = tools.read(call);
                    self.current = Some(call.name.clone());
                    conversation.push_line(Speaker::Tool, tool_line(call));
                    conversation.push_tool_result(call, answer.body());
                }
            }
        }
    }
}

/// One line of the conversation as the wire's message with the same content.
///
/// The two speakers that are the program's own reports travel as `user` turns that say so, because
/// the wire has three roles and the reports are neither the person nor the model speaking.
fn message_of(speaker: Speaker, text: &str) -> Value {
    let role = match speaker {
        Speaker::Agent => "assistant",
        Speaker::Person | Speaker::Tool | Speaker::Note => "user",
    };
    let content = match speaker {
        Speaker::Person | Speaker::Agent => text.to_owned(),
        Speaker::Tool => format!("The program reports a tool call: {text}"),
        Speaker::Note => format!("The program reports: {text}"),
    };
    json!({ "role": role, "content": content })
}

/// One tool call as the wire carries it: the id, the kind and the name with its arguments as JSON.
fn tool_call_message(call: &ToolCall) -> Value {
    json!({
        "id": call.id,
        "type": "function",
        "function": { "name": call.name, "arguments": call.arguments.to_string() },
    })
}

/// One step of a batch as the wire's `command_batch` carries it.
fn step_message(step: &BatchStep) -> Value {
    match step.name() {
        Some(name) => json!({ "command": step.command(), "params": step.params(), "as": name }),
        None => json!({ "command": step.command(), "params": step.params() }),
    }
}

/// The line a read leaves in the panel: the tool it named and, shortened, what it asked for.
fn tool_line(call: &ToolCall) -> String {
    let arguments = call.arguments.to_string();
    if arguments.chars().count() <= TOOL_LINE_CHARS {
        return format!("{} {arguments}", call.name);
    }
    let short: String = arguments.chars().take(TOOL_LINE_CHARS).collect();
    format!("{} {short}…", call.name)
}

/// What a batch did, in one sentence the panel draws and a person reads.
///
/// A planned batch says so instead of claiming a change: the mode is the person's, and the sentence
/// is where they meet it (ADR 0190). A refusal is appended because the command's own words are what
/// the model — and the person — act on.
fn describe(report: &BatchReport, lang: Lang) -> String {
    let counts =
        [report.created().len().to_string(), report.changed().len().to_string(), report.removed().len().to_string()];
    let mut sentence = match report.effect() {
        BatchEffect::Planned => Message::AssistantPlanLine.with(lang, counts),
        BatchEffect::Landed | BatchEffect::PartlyLanded => Message::AssistantLandedLine.with(lang, counts),
        BatchEffect::RolledBack => Message::AssistantRolledBackLine.with(lang, counts),
    };
    if let Some((_, error)) = report.refusal() {
        sentence.push_str(" · ");
        sentence.push_str(&Message::AssistantRefusedPart.with(lang, [error.to_string()]));
    }
    sentence
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use engine::{AgentMode, EditScope, Session};
    use model::{Id, Node};
    use serde_json::json;
    use testkit::{sample_catalog, sample_product_project};

    /// A session with one panel the tests can rename, shared with the wire's own tests.
    pub(crate) fn session() -> (Session, Id) {
        let (project, product) = sample_product_project();
        let mut session = Session::with_directory(project, sample_catalog());
        assert!(session.open_product(product), "the fixture product opens");
        let panel = session
            .dispatch(
                "panel.create",
                &json!({
                    "parent": product.get(),
                    "name": "Side",
                    "nominal_size": {"length": 700_000, "width": 500_000},
                    "material": "1-1",
                    "placement": {"translation": [0, 0, 0], "rotation": [0, 0, 0, 1]},
                }),
            )
            .expect("the sample session takes a panel")
            .created_id()
            .expect("the panel was created");
        (session, panel)
    }

    fn rename(panel: Id, name: &str) -> BatchStep {
        BatchStep::new("node.rename", json!({ "node": panel.get(), "name": name }))
    }

    /// Runs `frames` frame boundaries of one turn against `session`, in the turn's one scope.
    ///
    /// It is the shell's half of [`Turn::advance`]: mint one scope per message, dispatch every batch
    /// of the message inside it, and close it when the turn is over. The door is the window's own
    /// ([`crate::reads::MessageDoor`]), so a read is answered by the functions the tool server uses.
    fn drive(
        turn: &mut Turn,
        backend: &mut dyn ChatBackend,
        conversation: &mut Conversation,
        session: &mut Session,
        scope: EditScope,
        frames: usize,
    ) {
        let mut door = crate::reads::MessageDoor::new(session, scope);
        for _ in 0..frames {
            turn.advance(backend, conversation, Lang::En, &mut door);
            if turn.is_done() {
                break;
            }
        }
        if turn.is_done() {
            session.end_edit();
        }
    }

    fn name(session: &Session, panel: Id) -> Option<String> {
        session.project().node(panel).map(|node: &Node| node.name().to_owned())
    }

    #[test]
    fn a_read_is_answered_in_the_history_and_the_turn_goes_on() {
        // A model that cannot read the registry guesses a command's fields, and a history that drops
        // its own calls leaves it guessing again (X130). The read is answered by the door, the call
        // and its answer are in what the model reads back, and the turn asks again instead of
        // ending on the first read.
        let (mut session, panel) = session();
        let scope = session.edit_scope();
        let mut backend = Scripted::new(vec![
            Reply::reading(crate::tools::TOOL_LIST, json!({ "command": "panel.create" })),
            Reply::calling(vec![rename(panel, "Left")]),
        ]);
        let mut conversation = Conversation::new();
        conversation.push(Speaker::Person, "rename the side");
        let mut turn = Turn::new();

        drive(&mut turn, &mut backend, &mut conversation, &mut session, scope, 8);

        assert_eq!(name(&session, panel).as_deref(), Some("Left"), "the answer after the read landed");
        let messages = conversation.messages();
        let called = messages
            .iter()
            .find(|message| message["role"] == "assistant" && message["tool_calls"].is_array())
            .expect("the model's own call is in the history it reads back");
        assert_eq!(called["tool_calls"][0]["function"]["name"], crate::tools::TOOL_LIST);
        let answered =
            messages.iter().find(|message| message["role"] == "tool").expect("the read's answer is there too");
        assert_eq!(answered["tool_call_id"], called["tool_calls"][0]["id"], "the answer names its call");
        assert!(
            answered["content"].as_str().is_some_and(|text| text.contains("panel.create")),
            "and carries the schema of the command it asked for: {answered}"
        );
        assert!(
            conversation
                .lines()
                .iter()
                .any(|line| line.speaker == Speaker::Tool && line.text.contains(crate::tools::TOOL_LIST)),
            "the person sees the work without reading the transport"
        );
    }

    #[test]
    fn the_same_batch_twice_in_one_turn_builds_once() {
        // A model that repeats itself is one answer, not two parts: the second copy is answered with
        // the sentence that says so, and the document keeps one part and one undo entry.
        let (mut session, panel) = session();
        let scope = session.edit_scope();
        let step = rename(panel, "Left");
        let mut backend = Scripted::new(vec![
            Reply::calling(vec![step.clone()]),
            Reply::calling(vec![step]),
            Reply::saying("renamed"),
        ]);
        let mut conversation = Conversation::new();
        conversation.push(Speaker::Person, "rename the side");
        let mut turn = Turn::new();

        drive(&mut turn, &mut backend, &mut conversation, &mut session, scope, 8);

        assert!(turn.is_done(), "the spoken answer ends the turn");
        assert_eq!(name(&session, panel).as_deref(), Some("Left"));
        session.undo().expect("the turn is one entry");
        assert_eq!(name(&session, panel).as_deref(), Some("Side"), "the repeated batch left no second entry");
        assert!(
            conversation.lines().iter().any(|line| line.text == Message::AssistantRepeatedLine.text(Lang::En)),
            "the person reads why nothing happened the second time: {:?}",
            conversation.lines()
        );
    }

    #[test]
    fn one_answer_runs_as_one_undo_entry_and_a_stopped_turn_writes_nothing_more() {
        // The whole turn at a frame boundary: ask, take, dispatch. The scope is the shell's, so the
        // second answer must never run once the person has stopped the turn.
        let (mut session, panel) = session();
        let scope = session.edit_scope();
        let mut backend = Scripted::new(vec![
            Reply::calling(vec![rename(panel, "Left")]),
            Reply::calling(vec![rename(panel, "Right")]),
        ]);
        let mut conversation = Conversation::new();
        conversation.push(Speaker::Person, "rename the side");
        let mut turn = Turn::new();

        drive(&mut turn, &mut backend, &mut conversation, &mut session, scope, 1);
        assert!(turn.is_asking(), "the first frame only asks");
        drive(&mut turn, &mut backend, &mut conversation, &mut session, scope, 1);
        assert_eq!(name(&session, panel).as_deref(), Some("Left"), "the answer landed");

        // The person stops the turn between two calls: the document keeps the first answer alone.
        turn.stop();
        drive(&mut turn, &mut backend, &mut conversation, &mut session, scope, 1);
        assert!(turn.is_done());
        assert_eq!(name(&session, panel).as_deref(), Some("Left"), "the stopped answer never ran");
        session.undo().expect("the whole turn is one entry");
        assert_eq!(name(&session, panel).as_deref(), Some("Side"));
        session.undo().expect("the fixture's own creation is the entry under it");
        assert!(name(&session, panel).is_none(), "the turn left exactly one entry");
        assert!(conversation.lines().iter().any(|line| line.text == Message::AssistantStoppedLine.text(Lang::En)));
    }

    #[test]
    fn two_batches_of_one_turn_land_as_one_entry() {
        // ADR 0187: a model that needs twenty commands pays in calls, not in undo entries.
        let (mut session, panel) = session();
        let scope = session.edit_scope();
        let mut backend = Scripted::new(vec![
            Reply::calling(vec![rename(panel, "Left")]),
            Reply::calling(vec![rename(panel, "Right")]),
            Reply::saying("both sides renamed"),
        ]);
        let mut conversation = Conversation::new();
        let mut turn = Turn::new();

        drive(&mut turn, &mut backend, &mut conversation, &mut session, scope, 16);

        assert!(turn.is_done(), "the model's last answer asked for nothing");
        assert_eq!(name(&session, panel).as_deref(), Some("Right"));
        session.undo().expect("one entry holds both calls");
        assert_eq!(name(&session, panel).as_deref(), Some("Side"));
        session.undo().expect("the fixture's own creation is the entry under it");
        assert!(name(&session, panel).is_none(), "the two calls were one entry");
        assert_eq!(conversation.lines().last().map(|line| line.speaker), Some(Speaker::Agent));
    }

    #[test]
    fn a_plan_lands_nothing_and_says_what_it_would_do() {
        let (mut session, panel) = session();
        session.set_agent_mode(AgentMode::Plan);
        let scope = session.edit_scope();
        let mut backend = Scripted::new(vec![Reply::calling(vec![rename(panel, "Left")])]);
        let mut conversation = Conversation::new();
        let mut turn = Turn::new();

        drive(&mut turn, &mut backend, &mut conversation, &mut session, scope, 4);

        assert_eq!(name(&session, panel).as_deref(), Some("Side"), "nothing was written");
        session.undo().expect("the fixture's creation is the only entry in the history");
        assert!(name(&session, panel).is_none(), "a plan added no entry of its own");
        let note = conversation
            .lines()
            .iter()
            .find(|line| line.text.contains("Plan:"))
            .expect("the plan is reported in the conversation");
        assert_eq!(note.speaker, Speaker::Note);
        assert!(note.text.contains("Nothing is written"), "the note says nothing was written: {}", note.text);
    }

    #[test]
    fn a_backend_that_fails_ends_the_turn_with_its_own_sentence() {
        let (mut session, _panel) = session();
        let scope = session.edit_scope();
        let mut backend = Scripted::with_failure(vec![], BackendError::new("the endpoint did not answer"));
        let mut conversation = Conversation::new();
        let mut turn = Turn::new();

        drive(&mut turn, &mut backend, &mut conversation, &mut session, scope, 4);

        assert!(turn.is_done());
        assert_eq!(conversation.lines().last().map(|line| line.text.as_str()), Some("the endpoint did not answer"));
        session.undo().expect("the fixture's creation is the only entry in the history");
        assert!(!session.can_undo(), "a turn that could not answer wrote nothing");
    }

    #[test]
    fn the_budget_ends_a_turn_that_keeps_asking() {
        let (mut session, panel) = session();
        let scope = session.edit_scope();
        let mut backend = Scripted::new(vec![Reply::calling(vec![rename(panel, "Left")]); 6]);
        let mut conversation = Conversation::new();
        let mut turn = Turn::new().with_budget(2);

        drive(&mut turn, &mut backend, &mut conversation, &mut session, scope, 16);

        assert!(turn.is_done(), "the budget ends it");
        assert!(turn.calls() <= 2, "the turn asked for no more than its budget: {}", turn.calls());
        assert_eq!(
            conversation.lines().last().map(|line| line.text.as_str()),
            Some(Message::AssistantBudgetLine.text(Lang::En))
        );
        assert!(backend.remaining() > 0, "the script still had answers the budget refused");
        session.undo().expect("what the turn did land is one entry");
        assert_eq!(name(&session, panel).as_deref(), Some("Side"), "the budget stopped the second answer");
        session.undo().expect("the fixture's own creation is the entry under it");
        assert!(name(&session, panel).is_none());
    }

    /// A backend that answers one request in pieces, like a process that streams its work.
    ///
    /// It records the cancellation, because the person's stop has to reach a loop this side does
    /// not run (ADR 0230).
    #[derive(Debug, Default)]
    struct Streaming {
        pieces: std::collections::VecDeque<Reply>,
        cancelled: bool,
    }

    impl ChatBackend for Streaming {
        fn ask(&mut self, _transcript: &Conversation) {}

        fn take(&mut self) -> Option<Result<Reply, BackendError>> {
            self.pieces.pop_front().map(Ok)
        }

        fn waiting(&self) -> bool {
            self.pieces.is_empty()
        }

        fn cancel(&mut self) {
            self.cancelled = true;
        }
    }

    fn line(speaker: Speaker, text: &str) -> Line {
        Line { speaker, text: text.to_owned() }
    }

    #[test]
    fn a_piece_of_an_answer_is_drawn_and_the_request_stays_in_flight() {
        // A process that owns the model loop reports work as it happens: the words, the tool it
        // called and what came back are lines the panel draws between one frame and the next, and
        // the turn neither asks again nor ends until the last piece arrives (ADR 0230).
        let (mut session, _panel) = session();
        let scope = session.edit_scope();
        let mut backend = Streaming {
            pieces: std::collections::VecDeque::from(vec![
                Reply::part(vec![line(Speaker::Tool, "command_batch · node.rename")]),
                Reply::part(vec![line(Speaker::Note, "refused: the document is in plan")]),
                Reply::default(),
            ]),
            cancelled: false,
        };
        let mut conversation = Conversation::new();
        let mut turn = Turn::new();

        drive(&mut turn, &mut backend, &mut conversation, &mut session, scope, 2);
        assert!(turn.is_asking(), "the first piece leaves the request in flight");
        assert_eq!(turn.calls(), 1, "a piece is not a second request");
        assert_eq!(conversation.lines().len(), 1, "the tool call is drawn at once");

        drive(&mut turn, &mut backend, &mut conversation, &mut session, scope, 1);
        assert!(turn.is_asking(), "the refusal is another piece of the same answer");

        drive(&mut turn, &mut backend, &mut conversation, &mut session, scope, 1);
        assert!(turn.is_done(), "the last piece ends the turn");
        assert_eq!(turn.calls(), 1, "one message, one request");
        let drawn: Vec<(Speaker, &str)> =
            conversation.lines().iter().map(|line| (line.speaker, line.text.as_str())).collect();
        assert_eq!(
            drawn,
            vec![(Speaker::Tool, "command_batch · node.rename"), (Speaker::Note, "refused: the document is in plan")]
        );
        assert!(!backend.cancelled, "a turn that finished was never cancelled");
    }

    #[test]
    fn a_streamed_answer_grows_in_one_line_and_lands_one_history_entry() {
        // J26: a model that answers as it thinks is watched instead of waited for. The piece carries
        // the answer so far, so the panel draws one growing sentence and the wire's history takes the
        // whole answer once.
        let (mut session, _panel) = session();
        let scope = session.edit_scope();
        let mut backend = Streaming {
            pieces: std::collections::VecDeque::from(vec![
                Reply::saying_so_far("Renaming"),
                Reply::saying_so_far("Renaming the side"),
                Reply::saying("Renaming the side now."),
            ]),
            cancelled: false,
        };
        let mut conversation = Conversation::new();
        conversation.push(Speaker::Person, "rename the side");
        let mut turn = Turn::new();

        drive(&mut turn, &mut backend, &mut conversation, &mut session, scope, 1);
        assert!(turn.is_asking(), "the first frame only asks");
        let drawn = |conversation: &Conversation| -> Vec<String> {
            conversation
                .lines()
                .iter()
                .filter(|line| line.speaker == Speaker::Agent)
                .map(|line| line.text.clone())
                .collect()
        };
        assert!(drawn(&conversation).is_empty(), "nothing is drawn before the model speaks");

        drive(&mut turn, &mut backend, &mut conversation, &mut session, scope, 1);
        assert!(turn.is_asking(), "a piece leaves the request in flight");
        assert_eq!(drawn(&conversation), vec!["Renaming"], "the words so far are drawn at once");

        drive(&mut turn, &mut backend, &mut conversation, &mut session, scope, 1);
        assert_eq!(drawn(&conversation), vec!["Renaming the side"], "the same line grew instead of a second one");

        drive(&mut turn, &mut backend, &mut conversation, &mut session, scope, 1);
        assert!(turn.is_done(), "the whole answer ends the turn");
        assert_eq!(drawn(&conversation), vec!["Renaming the side now."], "no line was drawn twice");
        let spoken: Vec<&Value> =
            conversation.messages().iter().filter(|message| message["role"] == "assistant").collect();
        assert_eq!(spoken.len(), 1, "the history carries the answer once, whole: {spoken:?}");
        assert_eq!(spoken[0]["content"], "Renaming the side now.");
    }

    #[test]
    fn a_stop_in_the_middle_of_a_stream_keeps_what_was_already_said() {
        // J26: a streamed answer is read as it arrives, so the person's stop lands between two pieces
        // and not inside one: the words already drawn stay, the request is cancelled, and the turn
        // says it was stopped instead of pretending the answer finished.
        let (mut session, _panel) = session();
        let scope = session.edit_scope();
        let mut backend = Streaming {
            pieces: std::collections::VecDeque::from(vec![
                Reply::saying_so_far("Renaming"),
                Reply::saying_so_far("Renaming the side"),
                Reply::saying("Renaming the side now."),
            ]),
            cancelled: false,
        };
        let mut conversation = Conversation::new();
        conversation.push(Speaker::Person, "rename the side");
        let mut turn = Turn::new();

        drive(&mut turn, &mut backend, &mut conversation, &mut session, scope, 2);
        assert!(turn.is_asking(), "a piece left the request in flight");
        turn.stop();
        drive(&mut turn, &mut backend, &mut conversation, &mut session, scope, 1);

        assert!(turn.is_done(), "the stop ends the turn at the next piece boundary");
        assert!(backend.cancelled, "and reaches the backend that is writing the answer");
        let drawn: Vec<&str> = conversation
            .lines()
            .iter()
            .filter(|line| line.speaker == Speaker::Agent)
            .map(|line| line.text.as_str())
            .collect();
        assert_eq!(drawn, vec!["Renaming"], "what was said before the stop stays, and nothing more");
        assert_eq!(
            conversation.lines().last().map(|line| line.text.as_str()),
            Some(Message::AssistantStoppedLine.text(Lang::En))
        );
    }

    #[test]
    fn the_turn_names_the_step_it_last_dispatched() {
        // J26: the panel says what the agent is doing, and the turn is the only place that knows: a
        // read is named by its tool and a batch by the last command it dispatched.
        let (mut session, panel) = session();
        let scope = session.edit_scope();
        let mut backend = Scripted::new(vec![
            Reply::reading(crate::tools::TOOL_LIST, json!({ "command": "node.rename" })),
            Reply::calling(vec![rename(panel, "Left")]),
            Reply::saying("renamed"),
        ]);
        let mut conversation = Conversation::new();
        let mut turn = Turn::new();
        assert_eq!(turn.current(), None, "a turn that has not asked anything names no step");

        drive(&mut turn, &mut backend, &mut conversation, &mut session, scope, 1);
        assert_eq!(turn.current(), None, "the first frame only asks: nothing has happened yet");

        drive(&mut turn, &mut backend, &mut conversation, &mut session, scope, 1);
        assert_eq!(turn.current(), Some(crate::tools::TOOL_LIST), "the read is what the turn last did");

        drive(&mut turn, &mut backend, &mut conversation, &mut session, scope, 2);
        assert_eq!(turn.current(), Some("node.rename"), "and then the command it dispatched");
    }

    #[test]
    fn the_person_stop_reaches_a_process_that_owns_the_loop() {
        // A backend that runs the model loop cannot be stopped by not asking again: the stop has to
        // travel to it, and the turn says the person stopped it instead of pretending the answer
        // finished (ADR 0230).
        let (mut session, _panel) = session();
        let scope = session.edit_scope();
        let mut backend = Streaming {
            pieces: std::collections::VecDeque::from(vec![Reply::part(vec![line(Speaker::Tool, "query")])]),
            cancelled: false,
        };
        let mut conversation = Conversation::new();
        let mut turn = Turn::new();

        drive(&mut turn, &mut backend, &mut conversation, &mut session, scope, 1);
        assert!(turn.is_asking());
        turn.stop();
        drive(&mut turn, &mut backend, &mut conversation, &mut session, scope, 1);

        assert!(backend.cancelled, "the process was told to stop");
        assert!(turn.is_done());
        assert_eq!(
            conversation.lines().last().map(|line| line.text.as_str()),
            Some(Message::AssistantStoppedLine.text(Lang::En))
        );
    }

    #[test]
    fn a_script_that_runs_out_does_not_wait_forever() {
        let (mut session, _panel) = session();
        let scope = session.edit_scope();
        let mut backend = Scripted::new(vec![Reply::saying("first")]);
        let mut conversation = Conversation::new();
        let mut turn = Turn::new();

        drive(&mut turn, &mut backend, &mut conversation, &mut session, scope, 4);
        assert!(turn.is_done());
        assert_eq!(conversation.lines().last().map(|line| line.text.as_str()), Some("first"));

        // A second turn finds the script empty and is told so instead of waiting forever.
        let mut second = Turn::new();
        drive(&mut second, &mut backend, &mut conversation, &mut session, scope, 4);
        assert!(second.is_done());
        assert_eq!(backend.remaining(), 0);
        assert!(
            conversation.lines().last().is_some_and(|line| line.text.contains("no more answers")),
            "{:?}",
            conversation.lines().last()
        );
    }
}
