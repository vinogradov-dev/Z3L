//! The assistant panel: the conversation, the person's mode switch and the one message a turn is.
//!
//! The panel is the shell's half of ADR 0186 — `mcp::chat` holds the port and the turn, this file
//! holds the state a person touches. It draws the conversation the turn appended to, the two modes
//! as one switch beside the words they change, the model's checklist, and one field with Send. It
//! never dispatches: a turn is driven at the frame boundary by [`crate::App::serve_chat`], so an
//! answer lands between frames and never inside a drag (ADR 0187), and the mode the switch sets
//! lives on the session, so it gates every agent door and not the person's own commands
//! (ADR 0190).
//!
//! Three states are visible without a tooltip, because a furniture maker must see them at a glance:
//! which mode the session is in (the chosen segment and its one sentence), whether the agent is
//! working (the words under the conversation and a Stop where Send was), and what the last answer
//! did to the document (the turn's own note line, which says *plan* or *one undo entry*).
//!
//! A backend that owns the model loop reports its work as it happens — the tool it called and what
//! came back — so those lines are drawn between the messages, dim and marked, instead of being
//! folded into the answer's own words (ADR 0230).
//!
//! It is a `Window`, like Settings and Home: the panel is not a document surface, it opens over the
//! scene and closes without touching it. Its one row in `HOST_OWNERS` is the surface it draws
//! (`xtask/src/ui_rules.rs`). The height is the person's: the rows under the conversation take what
//! they need from the bottom, and the transcript fills what is left, so a taller window reads more
//! of a long answer and a short one keeps the room those rows ask for (X131).
//!
//! One window, one tab: this state belongs to the tab payload (`crate::document::Document`), so two
//! open scenes hold two conversations, two backends and two turns, and a message sent in one is
//! never in the other (J24). The window itself is the workspace's — whether it is up, where it sits
//! and how wide it is come from the shell — so switching tabs changes what the panel *says* and not
//! whether it is on screen.
//!
//! Changes when: the panel's states, its words, its buttons or the turn's presentation change.

use eframe::egui::{self, Context, Id, RichText, Ui};
use engine::{AgentMode, EditScope, Finding, Session};
use mcp::{ChatBackend, Conversation, Speaker, Turn};
use text::{Lang, Message};
use ui::settings::EndpointProblem;
use ui::theme;

/// The width and height the panel opens at, in points.
///
/// The height is only where the window opens: the transcript takes whatever the person drags the
/// window to, and the window never shrinks below the rows it draws beneath it (X131).
const WIDTH: f32 = 420.0;
const HEIGHT: f32 = 520.0;

/// The findings a panel lists before it says how many are left.
///
/// Six rows keep the conversation, the findings and the field in one window a person can read at a
/// glance; the tool answer carries the whole list for the model.
const FINDINGS_SHOWN: usize = 6;

/// The field's own id, so the caret the shell asks for on the frame the panel opens lands in the
/// message and in no other text edit on the screen (X122).
const FIELD_ID: &str = "assistant_field";

/// The id one chat keeps for the whole of its life, sent to an OpenCode Go gateway as
/// `x-opencode-session`. It is opaque to the gateway and only needs to be stable for one chat, so a
/// process counter and the wall clock make it unique without a dependency or a secret.
fn new_session_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SESSIONS: AtomicU64 = AtomicU64::new(1);
    let n = SESSIONS.fetch_add(1, Ordering::Relaxed);
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("z3l-{t:x}-{n:x}-{:x}", std::process::id())
}

/// What the panel asked the shell to do this frame, if anything.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ChatAction {
    /// Nothing was clicked.
    #[default]
    None,
    /// Send what the person typed as one turn.
    Send,
    /// Stop the turn in flight at its next call boundary.
    Stop,
    /// Send the waiting message to the endpoint the panel just named.
    AllowEndpoint,
    /// Keep the message: the endpoint was named and the person said not now.
    DenyEndpoint,
    /// Put the session in this mode.
    Mode(AgentMode),
    /// Close the panel.
    Close,
}

/// The assistant's panel state: what a person typed, what was said and the turn in flight.
///
/// The backend is the model's adapter (ADR 0028): `None` is the state the feature is born in — no
/// endpoint, no request, no network — and the panel says so instead of pretending to think.
pub(crate) struct Chat {
    /// What the person has typed and not sent.
    pub(crate) input: String,
    /// The conversation, which is chat state and never document data (ADR 0187).
    pub(crate) conversation: Conversation,
    /// The model's adapter, if one is connected.
    pub(crate) backend: Option<Box<dyn ChatBackend>>,
    /// The turn in flight.
    turn: Option<Turn>,
    /// The edit scope of the turn in flight, so every answer of one message is one undo entry.
    scope: Option<EditScope>,
    /// The agent-process command this panel installed, so a frame can tell a change from the same one.
    agent: Option<mcp::Program>,
    /// The endpoint this panel installed, so a frame can tell a change from the same one.
    model: Option<mcp::Endpoint>,
    /// This conversation's stable id, sent as `x-opencode-session` to a Go gateway so its routing and
    /// prompt cache survive the backend being rebuilt. One chat, one id; the person never sees it.
    session: String,
    /// The endpoint a person has to agree to before the message they typed may leave the machine.
    consent: Option<String>,
    /// Whether the workspace names an agent process the window offers no channel to.
    blocked: bool,
    /// The clause that stops the workspace's address from being used, when the page named one.
    problem: Option<EndpointProblem>,
    /// The checklist the model last sent; it stays after the turn ends and dies with the tab.
    checklist: Vec<String>,
    /// What the turn last did — the command it dispatched or the tool it read with — so the progress
    /// line says what is happening instead of only that something is (J26).
    step: Option<String>,
    /// When the turn in flight was sent, so the panel can say how long the person has waited.
    ///
    /// A waiting person reads seconds and knows the program is working; a spinner says nothing about
    /// whether an answer is near (J26).
    started: Option<std::time::Instant>,
    /// Whether the field should take the caret the next time the panel draws.
    ///
    /// The shell asks once, when the window comes up, and the flag is spent in the frame that honours
    /// it: a field that asked every frame could never give the keyboard back to the inspector (X122).
    field_focus: bool,
    /// The rules' findings for the revision the panel last read.
    findings: Vec<Finding>,
    /// The revision [`Self::findings`] was derived from, so a frame polls the rules once.
    findings_revision: Option<u64>,
}

impl Default for Chat {
    fn default() -> Self {
        Self::new()
    }
}

impl Chat {
    /// A panel with nothing said, nothing typed and no model connected.
    pub(crate) fn new() -> Self {
        Self {
            input: String::new(),
            conversation: Conversation::new(),
            backend: None,
            turn: None,
            scope: None,
            agent: None,
            model: None,
            consent: None,
            blocked: false,
            problem: None,
            session: new_session_id(),
            checklist: Vec::new(),
            step: None,
            started: None,
            field_focus: false,
            findings: Vec::new(),
            findings_revision: None,
        }
    }

    /// The edit scope of the turn in flight, which is what makes a batch that arrives over the
    /// channel part of the message the person sent (ADR 0187, ADR 0230).
    pub(crate) fn scope(&self) -> Option<EditScope> {
        self.scope
    }

    /// Starts the agent process the workspace names, or lets it go.
    ///
    /// A process is started once per command and never per frame, and a command that was cleared
    /// ends its process. A backend this shell did not install — a test's script, a future adapter —
    /// is left alone: only a backend this panel created is the settings' to replace.
    ///
    /// With no channel open the panel installs nothing and says why: a process with no door to the
    /// document could talk and do nothing, which is worse than a sentence (ADR 0188).
    pub(crate) fn apply_agent(&mut self, command: Option<mcp::Program>, channel_open: bool) {
        self.blocked = command.is_some() && !channel_open;
        let wanted = command.filter(|_| channel_open);
        if wanted == self.agent {
            return;
        }
        // A turn in flight belongs to the process that is running it: the setting takes effect when
        // the message is finished, so nothing is cut in half.
        if self.turn.is_some() {
            return;
        }
        match wanted {
            Some(program) => {
                self.backend = Some(Box::new(mcp::Acp::new(program.clone())));
                self.agent = Some(program);
            }
            None => {
                if self.agent.take().is_some() {
                    self.backend = None;
                }
            }
        }
    }

    /// Installs the OpenAI-compatible backend the workspace names, or lets it go.
    ///
    /// Two ways to give the panel a model, one turn: an agent process owns the loop (ADR 0230) and
    /// this endpoint is the wire a cloud provider and a local server answer the same way (ADR 0189).
    /// The shell offers only one of them, so `None` here means "no process is configured" and not
    /// "forget what a test installed": a backend this shell did not create is left alone, exactly as
    /// [`Self::apply_agent`] leaves it.
    ///
    /// A turn in flight belongs to the backend that is running it, so the settings take effect when
    /// the message is finished. The key is not read here: it is the backend's to find when a message
    /// is sent, so no frame touches the file and a key written while the window runs is picked up by
    /// the next message (ADR 0189).
    pub(crate) fn apply_model(&mut self, wanted: Option<mcp::Endpoint>, key_folder: Option<std::path::PathBuf>) {
        let same = match (&self.model, &wanted) {
            (Some(current), Some(next)) => current == next,
            // A backend this shell did not create is left alone.
            (None, None) => true,
            _ => false,
        };
        if same || self.turn.is_some() {
            return;
        }
        self.backend = wanted.clone().map(|endpoint| {
            Box::new(mcp::OpenAi::with_session(endpoint, key_folder.clone(), self.session.clone()))
                as Box<dyn ChatBackend>
        });
        self.model = wanted;
    }

    /// Names the reason the workspace's model configuration cannot be used, or clears it.
    ///
    /// The settings page owns the rule and the shell reads it every frame, because the panel is what
    /// a person sees next: a message sent to a typed-but-unusable address answers the clause that is
    /// wrong instead of the generic "no model" sentence.
    pub(crate) fn apply_problem(&mut self, problem: Option<EndpointProblem>) {
        self.problem = problem;
    }

    /// Whether the panel is asking an endpoint rather than a process, which the shell asks before it
    /// names the key's homes: a process brings its own credentials.
    pub(crate) fn asks_an_endpoint(&self) -> bool {
        self.model.is_some()
    }

    /// The endpoint a message would leave for and that the person has not agreed to yet.
    ///
    /// It is asked before the message is dispatched, so a workspace with an endpoint still sends
    /// nothing until somebody says yes (ADR 0189). The consent list is the workspace's, so it is the
    /// shell that holds it and this panel only reports which endpoint its backend asks.
    pub(crate) fn unconsented(&self, assistant: &ui::settings::AssistantSettings) -> Option<String> {
        let endpoint = self.model.as_ref()?.base_url().trim();
        (!assistant.consents(endpoint)).then(|| endpoint.to_owned())
    }

    /// Holds the typed message back until the person answers the question the panel is drawing.
    pub(crate) fn request_consent(&mut self, endpoint: String) {
        self.consent = Some(endpoint);
    }

    /// The endpoint waiting for an answer, which the panel draws as one card above the field.
    pub(crate) fn consent_pending(&self) -> Option<&str> {
        self.consent.as_deref()
    }

    /// The endpoint the person just allowed, taken so one click cannot allow twice.
    pub(crate) fn take_consent(&mut self) -> Option<String> {
        self.consent.take()
    }

    /// The person said not now: the message stays in the field and nothing leaves.
    pub(crate) fn clear_consent(&mut self) {
        self.consent = None;
    }

    /// Says one line of the shell's own voice in the conversation, before any turn has begun.
    ///
    /// It is how a message that cannot be sent is answered: the missing key is a sentence a person
    /// reads in the panel they typed in, not a modal and not silence.
    pub(crate) fn note(&mut self, text: String) {
        self.conversation.push(Speaker::Note, text);
    }

    /// The findings the panel last read, in the report's own order.
    #[cfg(test)]
    pub(crate) fn findings(&self) -> &[Finding] {
        &self.findings
    }

    /// Whether a turn is running: the field says so and Send gives way to Stop.
    pub(crate) fn is_busy(&self) -> bool {
        self.turn.is_some()
    }

    /// Asks for the caret in the field the next time the panel draws.
    ///
    /// Opening the panel is opening a field: the shell calls this from the one act that shows it, so
    /// the first key a person types is the message and not a viewport shortcut (X122).
    pub(crate) fn focus_field(&mut self) {
        self.field_focus = true;
    }

    /// Re-reads the rules when the document has moved since the last look.
    ///
    /// The pass is a derivation and costs a walk of the tree and a pair of boxes per part, so it runs
    /// once per revision and never per frame. Closing the panel keeps the list, like the
    /// conversation, and reopening on the same revision shows it without a second walk.
    pub(crate) fn refresh_findings(&mut self, session: &Session) {
        let revision = session.project().revision().get();
        if self.findings_revision == Some(revision) {
            return;
        }
        self.findings = session.inspect();
        self.findings_revision = Some(revision);
    }

    /// Starts a turn on `text`, or says why it cannot.
    ///
    /// A message with no model behind it leaves a note in the conversation rather than an empty
    /// field and no answer, and one turn at a time keeps the conversation and the history readable.
    pub(crate) fn begin(&mut self, session: &mut Session, text: String, lang: Lang) {
        let text = text.trim().to_owned();
        if text.is_empty() || self.turn.is_some() {
            return;
        }
        self.input.clear();
        self.conversation.push(Speaker::Person, text);
        if self.backend.is_none() {
            let sentence = match self.problem {
                Some(problem) => Message::AssistantProblemFix.with(lang, [problem.sentence(lang)]),
                None => Message::AssistantNotConfigured.text(lang).to_owned(),
            };
            self.conversation.push(Speaker::Note, sentence);
            return;
        }
        // The live scene is the model's map: the root id, the open product and the nodes. It is
        // replaced for every message, so the model never asks the person for an id the window already
        // knows (X125).
        self.conversation.set_brief(engine::scene_brief(session));
        self.scope = Some(session.edit_scope());
        self.checklist.clear();
        self.step = None;
        self.started = Some(std::time::Instant::now());
        self.turn = Some(Turn::new());
    }

    /// The person's stop: it lands at the turn's next call boundary, never inside a batch.
    pub(crate) fn stop(&mut self) {
        if let Some(turn) = self.turn.as_mut() {
            turn.stop();
        }
    }

    /// Drives the turn one step, at the frame boundary.
    ///
    /// Every batch of the message lands in one scope, so `undo` takes the whole answer back in one
    /// step; the scope closes here when the turn is over, whether it spoke, failed, ran out of
    /// calls or was stopped. A turn that dispatches nothing leaves no entry at all.
    pub(crate) fn serve(&mut self, session: &mut Session, lang: Lang) {
        let Some(mut turn) = self.turn.take() else {
            return;
        };
        let Some(backend) = self.backend.as_deref_mut() else {
            self.turn = None;
            self.scope = None;
            return;
        };
        let scope = *self.scope.get_or_insert_with(|| session.edit_scope());
        {
            // The door the window is: every batch of the message in its one scope, and the reads
            // answered by the functions the tool server publishes, so the model in the panel and a
            // model over MCP read one answer (ADR 0185).
            let conversation = &mut self.conversation;
            let mut door = mcp::MessageDoor::new(session, scope);
            turn.advance(backend, conversation, lang, &mut door);
        }
        self.checklist = turn.checklist().to_vec();
        self.step = turn.current().map(str::to_owned);
        if turn.is_done() {
            session.end_edit();
            self.scope = None;
            self.started = None;
        } else {
            self.turn = Some(turn);
        }
    }

    /// Draws the panel and answers the one action it asked for.
    ///
    /// `held` is the shell's gesture and not the turn: a drag or a pending question pauses the turn
    /// at the next frame boundary, so Send waits while it lasts. Whether the agent is working is the
    /// panel's own to read ([`Self::is_busy`]) — the Stop button is the turn's state and a shell
    /// flag anywhere else in the signature hid it for the whole life of a turn.
    pub(crate) fn show(
        &mut self,
        ctx: &Context,
        open: &mut bool,
        mode: AgentMode,
        lang: Lang,
        held: bool,
    ) -> ChatAction {
        let mut action = ChatAction::None;
        egui::Window::new(Message::SectionAssistant.text(lang))
            .id(Id::unique("assistant_panel"))
            .open(open)
            .default_size(egui::vec2(WIDTH, HEIGHT))
            .min_width(WIDTH)
            .resizable(true)
            .show(ctx, |ui| {
                action = self.body(ui, mode, lang, held);
            });
        if !*open {
            action = ChatAction::Close;
        }
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            action = ChatAction::Close;
        }
        action
    }

    /// The panel's inside: the switch, the transcript, the findings, the checklist and the field.
    ///
    /// The rows under the conversation are laid out from the bottom as one panel, so the transcript
    /// gets exactly the height that is left: the window's height is the person's, and a taller
    /// window shows a taller transcript instead of blank space under the field (X131). Drawing the
    /// transcript first with a fixed cap is what pinned the window's height to its content in both
    /// directions, so a drag inward hit the cap and a drag outward ran off the screen.
    fn body(&mut self, ui: &mut Ui, mode: AgentMode, lang: Lang, held: bool) -> ChatAction {
        let mut action = ChatAction::None;
        self.mode_switch(ui, mode, lang, &mut action);
        ui.add_space(theme::STEP);
        ui.separator();
        egui::Panel::bottom("assistant_footer")
            .frame(egui::Frame::NONE)
            .show_separator_line(false)
            .show(ui, |ui| self.footer(ui, lang, held, &mut action));
        egui::ScrollArea::vertical()
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .show(ui, |ui| self.transcript(ui, lang));
        action
    }

    /// Everything under the conversation: the rules' findings, the model's checklist, what the turn
    /// is doing and the one field with its button.
    fn footer(&mut self, ui: &mut Ui, lang: Lang, held: bool, action: &mut ChatAction) {
        self.findings_section(ui, lang);
        if !self.checklist.is_empty() {
            ui.separator();
            ui.label(RichText::new(Message::AssistantChecklist.text(lang)).strong());
            for item in &self.checklist {
                ui.label(RichText::new(format!("· {item}")).color(theme::text_dim(ui)));
            }
        }
        ui.separator();
        if self.is_busy() {
            // The turn asks for the frames that draw it: an answer arriving in pieces and a clock
            // that keeps moving are both invisible without one, and the person is looking at a
            // panel that would otherwise only move when the pointer does (J26, ADR 0211).
            ui.ctx().request_repaint();
            let calls = self.turn.as_ref().map_or(0, |turn| turn.calls()).max(1).to_string();
            let seconds = self.started.map_or(0, |started| started.elapsed().as_secs()).to_string();
            ui.label(RichText::new(Message::AssistantWorking.with(lang, [seconds, calls])).color(theme::text_dim(ui)));
            if let Some(step) = &self.step {
                ui.label(
                    RichText::new(Message::AssistantWorkingStep.with(lang, [step.clone()])).color(theme::text_dim(ui)),
                );
            }
        }
        if self.backend.is_none() {
            let (words, ink) = if self.blocked {
                (Message::AssistantNoChannel.text(lang).to_owned(), theme::warn(ui))
            } else if let Some(problem) = self.problem {
                (Message::AssistantProblemFix.with(lang, [problem.sentence(lang)]), theme::warn(ui))
            } else {
                (Message::AssistantNotConfigured.text(lang).to_owned(), theme::warn(ui))
            };
            ui.label(RichText::new(words).color(ink));
        }
        if let Some(endpoint) = self.consent_pending() {
            self.consent_card(ui, endpoint, lang, action);
        }
        self.field(ui, lang, held, action);
    }

    /// The one question a message to a new endpoint asks, in the panel the person is already reading.
    ///
    /// It names the endpoint and what leaves the machine, and it is the only place the program asks:
    /// the answer is the workspace's, remembered per endpoint, so a second message to the same place
    /// does not ask again (ADR 0189). Nothing is sent while the card is drawn.
    fn consent_card(&self, ui: &mut Ui, endpoint: &str, lang: Lang, action: &mut ChatAction) {
        ui.separator();
        ui.label(RichText::new(Message::AssistantConsentTitle.text(lang)).strong());
        let body = Message::AssistantConsentBody.with(lang, [endpoint.to_owned()]);
        ui.label(RichText::new(body).color(theme::warn(ui)));
        ui.horizontal(|ui| {
            if ui.button(RichText::new(Message::AssistantSend.text(lang)).strong()).clicked() {
                *action = ChatAction::AllowEndpoint;
            }
            if ui.button(Message::AssistantConsentDeny.text(lang)).clicked() {
                *action = ChatAction::DenyEndpoint;
            }
        });
    }

    /// The two modes as one switch: the chosen segment, and the sentence it stands for.
    fn mode_switch(&self, ui: &mut Ui, mode: AgentMode, lang: Lang, action: &mut ChatAction) {
        ui.horizontal(|ui| {
            for (candidate, words) in
                [(AgentMode::Plan, Message::AssistantPlanMode), (AgentMode::Build, Message::AssistantBuildMode)]
            {
                let chosen = candidate == mode;
                let fill = if chosen { theme::accent_fill(ui) } else { theme::control(ui) };
                let button = egui::Button::new(RichText::new(words.text(lang)).strong()).fill(fill);
                if ui.add(button).clicked() && !chosen {
                    *action = ChatAction::Mode(candidate);
                }
            }
        });
        let hint = match mode {
            AgentMode::Plan => Message::AssistantPlanHint,
            AgentMode::Build => Message::AssistantBuildHint,
        };
        ui.label(RichText::new(hint.text(lang)).color(theme::text_dim(ui)));
    }

    /// The rules' findings: what the code proved, before the model explains it.
    ///
    /// The person reads this list with or without a model connected, so it is the panel's own and
    /// never the model's answer. A row is one finding: the severity in its ink and the sentence the
    /// rule wrote, in the interface's language. The list is capped so a scene with thirty mistakes
    /// still leaves the conversation readable; the tool answer carries every one.
    fn findings_section(&self, ui: &mut Ui, lang: Lang) {
        ui.separator();
        ui.label(RichText::new(Message::AssistantFindings.text(lang)).strong());
        if self.findings.is_empty() {
            ui.label(RichText::new(Message::AssistantFindingsNone.text(lang)).color(theme::text_dim(ui)));
            return;
        }
        for finding in self.findings.iter().take(FINDINGS_SHOWN) {
            let (ink, word) = match finding.severity {
                engine::Severity::Error => (theme::danger(ui), Message::FindingSeverityError),
                engine::Severity::Warning => (theme::warn(ui), Message::FindingSeverityWarning),
                engine::Severity::Info => (theme::text_dim(ui), Message::FindingSeverityInfo),
            };
            ui.label(RichText::new(format!("{}: {}", word.text(lang), finding.text(lang))).color(ink));
        }
        if let Some(more) = self.findings.len().checked_sub(FINDINGS_SHOWN).filter(|more| *more > 0) {
            ui.label(
                RichText::new(Message::AssistantFindingsMore.with(lang, [more.to_string()])).color(theme::text_dim(ui)),
            );
        }
    }

    /// The conversation: who spoke, in the ink the speaker deserves.
    fn transcript(&self, ui: &mut Ui, lang: Lang) {
        if self.conversation.is_empty() {
            ui.label(RichText::new(Message::AssistantPlaceholder.text(lang)).color(theme::text_dim(ui)));
            return;
        }
        for line in self.conversation.lines() {
            match line.speaker {
                Speaker::Person => {
                    let words = format!("{}: {}", Message::AssistantYou.text(lang), line.text);
                    ui.label(RichText::new(words).strong());
                }
                Speaker::Agent => {
                    ui.label(line.text.as_str());
                }
                Speaker::Tool => {
                    // Work, not speech: the marker says the agent did something, and the dim ink
                    // keeps it out of the conversation's own rhythm (ADR 0230).
                    ui.label(RichText::new(format!("· {}", line.text)).color(theme::text_dim(ui)));
                }
                Speaker::Note => {
                    ui.label(RichText::new(line.text.as_str()).italics().color(theme::text_dim(ui)));
                }
            }
            ui.add_space(theme::STEP);
        }
    }

    /// The conversation as one text, for the copy button: the same lines the panel draws, with the
    /// speaker named so a reader outside the window knows who said what.
    pub(crate) fn transcript_text(&self, lang: Lang) -> String {
        let mut text = String::new();
        for line in self.conversation.lines() {
            let who = match line.speaker {
                Speaker::Person => Message::AssistantYou,
                Speaker::Agent => Message::AssistantAgent,
                Speaker::Tool => Message::AssistantTool,
                Speaker::Note => Message::AssistantNote,
            };
            text.push_str(who.text(lang));
            text.push_str(": ");
            text.push_str(&line.text);
            text.push('\n');
        }
        text
    }

    /// The one field and its one button: Enter and Send are the same gesture.
    ///
    /// The button is Send while the panel is idle and Stop while the turn runs, and both facts are
    /// read here: `working` is the turn ([`Self::is_busy`]) and `held` is the shell's gesture, which
    /// only delays the next message. Reading the gesture for both facts is what drew Send while a
    /// turn ran, so the agent could not be stopped from the panel at all.
    ///
    /// The button holds the right end of the row and the field takes what is left, so the row is
    /// measured from the button outwards. A field that leaves a guessed width for its button asks
    /// the window for more room whenever the button is wider than the guess — «Отправить» is — and
    /// the panel then widens by itself, a little every frame (X108). The row is the height
    /// `ui.horizontal` takes, so the panel's rows keep one rhythm.
    ///
    /// The field takes the caret the frame the window opens, so the person types instead of finding
    /// it: opening the panel is opening a field (X122). While a turn runs the field stays enabled — a
    /// person composes the next message instead of waiting for the answer — and only Stop changed.
    fn field(&mut self, ui: &mut Ui, lang: Lang, held: bool, action: &mut ChatAction) {
        let working = self.is_busy();
        let enabled = self.backend.is_some() && !held;
        let ready = enabled && !working && !self.input.trim().is_empty();
        // The caret is spent here, and only a field that takes keys takes it: a disabled field would
        // hold the keyboard while nothing could be typed into it, and the shell stops reading its own
        // shortcuts for a frame a focused text edit owns (X122).
        let caret = std::mem::take(&mut self.field_focus) && enabled;
        let (entered, asked) = ui
            .allocate_ui_with_layout(
                egui::vec2(ui.available_width(), ui.spacing().interact_size.y),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    let asked = if working {
                        // The button that was Send is Stop while the agent works: one place, one meaning.
                        ui.add(egui::Button::new(Message::AssistantStop.text(lang)))
                            .clicked()
                            .then_some(ChatAction::Stop)
                    } else {
                        ui.add_enabled(ready, egui::Button::new(Message::AssistantSend.text(lang)))
                            .clicked()
                            .then_some(ChatAction::Send)
                    };
                    // The copy sits beside Send, not on a row of its own: a second row would be
                    // height the transcript cannot have, and a short label leaves the field wide in
                    // both languages.
                    let copy = ui.add_enabled(
                        !self.conversation.is_empty(),
                        egui::Button::new(Message::AssistantCopy.text(lang)),
                    );
                    if copy.clicked() {
                        let text = self.transcript_text(lang);
                        ui.ctx().copy_text(text);
                    }
                    let response = ui.add_enabled(
                        enabled,
                        egui::TextEdit::singleline(&mut self.input)
                            .id(Id::unique(FIELD_ID))
                            .hint_text(Message::AssistantPlaceholder.text(lang))
                            .desired_width(ui.available_width()),
                    );
                    if caret {
                        response.request_focus();
                    }
                    let entered = response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                    (entered, asked)
                },
            )
            .inner;
        if let Some(asked) = asked {
            *action = asked;
        }
        if entered && ready {
            // The person sent with Enter, and the field asks for the caret back: the next message is
            // typed where the last one was written instead of the person clicking the field again
            // (X122, the same act the panel's opening is).
            self.field_focus = true;
            *action = ChatAction::Send;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_conversation_copies_with_its_speakers() {
        // The copy button's text, so a person can hand a whole turn to another reader without
        // retyping it and without the window's colours: the speaker is a word, not an ink.
        let mut chat = Chat::new();
        chat.conversation.push(Speaker::Person, "создай куб 100 200 900");
        chat.conversation.push(Speaker::Agent, "Готово");
        chat.conversation.push(Speaker::Tool, "объект создан");
        chat.conversation.push(Speaker::Note, "одна запись истории");

        let text = chat.transcript_text(Lang::En);

        assert!(text.contains("You: создай куб 100 200 900"), "{text}");
        assert!(text.contains("Agent: Готово"), "{text}");
        assert!(text.contains("Tool: объект создан"), "{text}");
        assert!(text.contains("Note: одна запись истории"), "{text}");
    }
}
