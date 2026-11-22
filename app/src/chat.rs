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
//! It is a `Window`, like Settings and Home: the panel is not a document surface, it opens over the
//! scene and closes without touching it. Its one row in `HOST_OWNERS` is the surface it draws
//! (`xtask/src/ui_rules.rs`).
//!
//! Changes when: the panel's states, its words, its buttons or the turn's presentation change.

use eframe::egui::{self, Context, Id, RichText, Ui};
use engine::{AgentMode, BatchOptions, BatchStep, EditScope, Finding, Session};
use mcp::{ChatBackend, Conversation, Speaker, Turn};
use text::{Lang, Message};
use ui::theme;

/// The width and height the panel opens at, in points.
const WIDTH: f32 = 420.0;
const HEIGHT: f32 = 520.0;

/// The height of the transcript above the field, in points.
const TRANSCRIPT_HEIGHT: f32 = 300.0;

/// The findings a panel lists before it says how many are left.
///
/// Six rows keep the conversation, the findings and the field in one window a person can read at a
/// glance; the tool answer carries the whole list for the model.
const FINDINGS_SHOWN: usize = 6;

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
    /// Whether the panel is on screen.
    pub(crate) open: bool,
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
    /// The checklist the model last sent; it stays after the turn ends and dies with the tab.
    checklist: Vec<String>,
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
            open: false,
            input: String::new(),
            conversation: Conversation::new(),
            backend: None,
            turn: None,
            scope: None,
            checklist: Vec::new(),
            findings: Vec::new(),
            findings_revision: None,
        }
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

    /// Opens the panel, or closes it without touching the turn's document.
    ///
    /// The header's cell and `Ctrl+J` are one act, so both ask here rather than each writing the
    /// flag: a panel that opens from two doors and closes from one is the drift the inventory ends
    /// (X106, ADR 0222). Closing keeps the conversation and the mode, which are the tab's.
    pub(crate) fn toggle(&mut self) {
        self.open = !self.open;
    }

    /// Re-reads the rules when the document has moved since the last look.
    ///
    /// The pass is a derivation and costs a walk of the tree and a pair of boxes per part, so it runs
    /// once per revision and never per frame. Closing the panel keeps the list, like the
    /// conversation, and reopening on the same revision shows it without a second walk.
    pub(crate) fn refresh_findings(&mut self, session: &Session) {
        if !self.open {
            return;
        }
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
            self.conversation.push(Speaker::Note, Message::AssistantNotConfigured.text(lang));
            return;
        }
        self.scope = Some(session.edit_scope());
        self.checklist.clear();
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
            let conversation = &mut self.conversation;
            let mut dispatch = |steps: &[BatchStep]| session.agent_batch_in(scope, steps, BatchOptions::ALL_OR_NOTHING);
            turn.advance(backend, conversation, lang, &mut dispatch);
        }
        self.checklist = turn.checklist().to_vec();
        if turn.is_done() {
            session.end_edit();
            self.scope = None;
        } else {
            self.turn = Some(turn);
        }
    }

    /// Draws the panel and answers the one action it asked for.
    pub(crate) fn show(&mut self, ctx: &Context, mode: AgentMode, lang: Lang, busy: bool) -> ChatAction {
        let mut action = ChatAction::None;
        let mut open = self.open;
        egui::Window::new(Message::SectionAssistant.text(lang))
            .id(Id::unique("assistant_panel"))
            .open(&mut open)
            .default_size(egui::vec2(WIDTH, HEIGHT))
            .min_width(WIDTH)
            .resizable(true)
            .show(ctx, |ui| {
                action = self.body(ui, mode, lang, busy);
            });
        if !open {
            action = ChatAction::Close;
        }
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            action = ChatAction::Close;
        }
        action
    }

    /// The panel's inside: the mode switch, the transcript, the checklist and the field.
    fn body(&mut self, ui: &mut Ui, mode: AgentMode, lang: Lang, busy: bool) -> ChatAction {
        let mut action = ChatAction::None;
        self.mode_switch(ui, mode, lang, &mut action);
        ui.add_space(theme::STEP);
        ui.separator();
        egui::ScrollArea::vertical()
            .max_height(TRANSCRIPT_HEIGHT)
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .show(ui, |ui| self.transcript(ui, lang));
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
            ui.label(RichText::new(Message::AssistantThinking.text(lang)).color(theme::text_dim(ui)));
        }
        if self.backend.is_none() {
            ui.label(RichText::new(Message::AssistantNotConfigured.text(lang)).color(theme::warn(ui)));
        }
        self.field(ui, lang, busy, &mut action);
        action
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
                Speaker::Note => {
                    ui.label(RichText::new(line.text.as_str()).italics().color(theme::text_dim(ui)));
                }
            }
            ui.add_space(theme::STEP);
        }
    }

    /// The one field and its one button: Enter and Send are the same gesture.
    ///
    /// The button holds the right end of the row and the field takes what is left, so the row is
    /// measured from the button outwards. A field that leaves a guessed width for its button asks
    /// the window for more room whenever the button is wider than the guess — «Отправить» is — and
    /// the panel then widens by itself, a little every frame (X108). The row is the height
    /// `ui.horizontal` takes, so the panel's rows keep one rhythm.
    fn field(&mut self, ui: &mut Ui, lang: Lang, busy: bool, action: &mut ChatAction) {
        let enabled = self.backend.is_some() && !busy;
        let ready = enabled && !self.input.trim().is_empty();
        let (entered, asked) = ui
            .allocate_ui_with_layout(
                egui::vec2(ui.available_width(), ui.spacing().interact_size.y),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    let asked = if busy {
                        // The button that was Send is Stop while the agent works: one place, one meaning.
                        ui.add(egui::Button::new(Message::AssistantStop.text(lang)))
                            .clicked()
                            .then_some(ChatAction::Stop)
                    } else {
                        ui.add_enabled(ready, egui::Button::new(Message::AssistantSend.text(lang)))
                            .clicked()
                            .then_some(ChatAction::Send)
                    };
                    let response = ui.add_enabled(
                        enabled,
                        egui::TextEdit::singleline(&mut self.input)
                            .hint_text(Message::AssistantPlaceholder.text(lang))
                            .desired_width(ui.available_width()),
                    );
                    let entered = response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                    (entered, asked)
                },
            )
            .inner;
        if let Some(asked) = asked {
            *action = asked;
        }
        if entered && ready {
            *action = ChatAction::Send;
        }
    }
}
