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
use text::{Lang, Message};
use thiserror::Error;

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

/// One conversation: the lines, in order, and nothing else.
///
/// It is owned by the shell, before, during and after a turn, so the backend reads the same history
/// a person does and a stopped turn leaves the words it already said.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Conversation {
    lines: Vec<Line>,
}

impl Conversation {
    /// An empty conversation.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds one line.
    pub fn push(&mut self, speaker: Speaker, text: impl Into<String>) {
        self.lines.push(Line { speaker, text: text.into() });
    }

    /// Adds lines the backend already wrote, in their own order.
    ///
    /// A backend that speaks a protocol of its own reports what it said and what it did as lines of
    /// this conversation, so the panel draws one transcript whichever backend is behind it.
    pub fn extend(&mut self, lines: Vec<Line>) {
        self.lines.extend(lines);
    }

    /// Every line, oldest first.
    pub fn lines(&self) -> &[Line] {
        &self.lines
    }

    /// Forgets everything, which is what closing the tab does with it.
    pub fn clear(&mut self) {
        self.lines.clear();
    }

    /// Whether the conversation holds nothing yet.
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
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
    pub more: bool,
}

impl Reply {
    /// An answer in words alone.
    pub fn saying(text: impl Into<String>) -> Self {
        Self { text: text.into(), ..Self::default() }
    }

    /// An answer that asks for one batch.
    pub fn calling(calls: Vec<BatchStep>) -> Self {
        Self { calls, ..Self::default() }
    }

    /// One more piece of an answer still arriving: the lines it said since the last piece.
    pub fn part(lines: Vec<Line>) -> Self {
        Self { lines, more: true, ..Self::default() }
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

/// A backend that answers from a fixed script, so a whole turn runs in a test without a network.
///
/// It is the first backend the tree has (ADR 0187: "a scripted model must run a whole task in
/// tests"): a test hands in the answers a model would have produced and the frame drives them
/// through the real turn, the real session and the real undo history.
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
        self.pending = Some(
            self.answers
                .pop_front()
                .unwrap_or_else(|| Err(BackendError::new("the scripted backend holds no more answers"))),
        );
    }

    fn take(&mut self) -> Option<Result<Reply, BackendError>> {
        self.pending.take()
    }

    fn waiting(&self) -> bool {
        self.pending.is_none()
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
}

impl Default for Turn {
    fn default() -> Self {
        Self::new()
    }
}

impl Turn {
    /// A turn that has not asked anything yet.
    pub fn new() -> Self {
        Self { calls: 0, budget: MAX_CALLS, asked: false, stopped: false, done: false, checklist: Vec::new() }
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

    /// One step of the turn, at a frame boundary.
    ///
    /// Exactly one of four things happens: the person's stop ends the turn without asking anything
    /// more, an answer that arrived is taken and its batch dispatched, a reply is asked for, or the
    /// budget ends the turn. `dispatch` runs one batch against the document the person is looking
    /// at and reports what it did; the turn never touches a session itself.
    pub fn advance(
        &mut self,
        backend: &mut dyn ChatBackend,
        conversation: &mut Conversation,
        lang: Lang,
        dispatch: &mut dyn FnMut(&[BatchStep]) -> BatchReport,
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
                    self.receive(reply, conversation, lang, dispatch);
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

    /// Takes one answer: its words, its checklist and, when it names commands, its one batch.
    ///
    /// A piece of an answer still arriving appends what it says and leaves the request in flight,
    /// which is how a process that streams its work is watched instead of waited for (ADR 0230).
    fn receive(
        &mut self,
        reply: Reply,
        conversation: &mut Conversation,
        lang: Lang,
        dispatch: &mut dyn FnMut(&[BatchStep]) -> BatchReport,
    ) {
        if !reply.text.trim().is_empty() {
            conversation.push(Speaker::Agent, reply.text);
        }
        conversation.extend(reply.lines);
        if reply.more {
            self.asked = true;
            return;
        }
        self.checklist = reply.checklist;
        if reply.calls.is_empty() {
            self.done = true;
            return;
        }
        let report = dispatch(&reply.calls);
        conversation.push(Speaker::Note, describe(&report, lang));
    }
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
    use engine::{AgentMode, BatchOptions, EditScope, Session};
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
    /// of the message inside it, and close it when the turn is over.
    fn drive(
        turn: &mut Turn,
        backend: &mut dyn ChatBackend,
        conversation: &mut Conversation,
        session: &mut Session,
        scope: EditScope,
        frames: usize,
    ) {
        let mut dispatch = |steps: &[BatchStep]| session.agent_batch_in(scope, steps, BatchOptions::ALL_OR_NOTHING);
        for _ in 0..frames {
            turn.advance(backend, conversation, Lang::En, &mut dispatch);
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
