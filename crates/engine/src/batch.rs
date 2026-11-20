//! A batch: many commands, one edit scope, and a report a person reads before it lands.
//!
//! A batch is what a headless recipe and an assistant turn have in common: a list of commands that
//! belongs together, so undo takes the whole answer back in one step (ADR 0183, ADR 0187). Running
//! it is one operation of the session rather than a loop in each door, because the command line and
//! the tool that serves a model must give the same answer to the same list (ADR 0190, ADR 0197).
//!
//! The steps run on a copy of the document, and the copy is installed only when the batch may land
//! ([`Session::batch`]). That is what makes two promises cheap to keep: an all-or-nothing batch that
//! refuses leaves the document — its revision, its history and its saved marker — exactly as it
//! was, and a `dry_run` answers with the same report while writing nothing at all. A batch that
//! keeps what it applied (`atomic` off) installs the steps that ran and says so, instead of claiming
//! an atomicity it does not have.
//!
//! The report names what the batch created, changed and removed by document id, read from the two
//! documents rather than guessed from the commands, so a command that renames, moves or deletes is
//! reported like one that creates. A container is compared by its own fields and not by its
//! children, so a panel created inside a product reports the panel and not its parent as well.
//!
//! Changes when: a batch gains an option, or the report grows a field a door needs.

use std::collections::HashSet;

use model::{Id, Node, NodeQuery, Project};
use serde_json::Value;

use crate::commands::{CommandError, CommandOutcome};
use crate::session::Session;

/// One command of a batch: the stable id it dispatches and the parameters it takes.
#[derive(Clone, Debug, PartialEq)]
pub struct BatchStep {
    command: String,
    params: Value,
}

impl BatchStep {
    /// A step that dispatches `command` with `params`.
    ///
    /// The parameters are the command's own typed struct in JSON, exactly what
    /// [`crate::Session::dispatch`] takes, so a step cannot say anything a command would not
    /// accept.
    pub fn new(command: impl Into<String>, params: Value) -> Self {
        Self { command: command.into(), params }
    }

    /// The stable command id this step dispatches.
    pub fn command(&self) -> &str {
        &self.command
    }

    /// The parameters handed to that command.
    pub const fn params(&self) -> &Value {
        &self.params
    }
}

/// What a batch is asked to do with the document.
///
/// The two switches are independent: `atomic` speaks about what a refusal costs, `dry_run` about
/// whether the document is written at all (ADR 0197). Four combinations, four honest answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BatchOptions {
    atomic: bool,
    dry_run: bool,
}

impl BatchOptions {
    /// All or nothing, and the document is changed: what the command line runs.
    pub const ALL_OR_NOTHING: Self = Self { atomic: true, dry_run: false };

    /// The same options with `atomic` set: whether a refusal takes the whole batch back.
    pub const fn atomic(mut self, atomic: bool) -> Self {
        self.atomic = atomic;
        self
    }

    /// The same options with `dry_run` set: whether the batch only answers.
    pub const fn dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }

    /// Whether a refusal takes the whole batch back.
    pub const fn is_atomic(self) -> bool {
        self.atomic
    }

    /// Whether the batch only answers.
    pub const fn is_dry_run(self) -> bool {
        self.dry_run
    }
}

impl Default for BatchOptions {
    fn default() -> Self {
        Self::ALL_OR_NOTHING
    }
}

/// What a batch did, or would do, to the document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BatchEffect {
    /// Every step ran, and the document holds them as one undo entry.
    Landed,
    /// A step refused and the steps before it stay in the document, because the batch is not atomic.
    PartlyLanded,
    /// A step refused and nothing of the batch is in the document.
    ///
    /// That is the all-or-nothing answer, and it is also the answer when the first step of a batch
    /// that keeps what it applies refused: the variant describes the document, not the mechanism.
    RolledBack,
    /// A dry run: nothing was written, and the report says what would have landed.
    Planned,
}

impl BatchEffect {
    /// Whether the document holds the batch after the call.
    pub const fn landed(self) -> bool {
        matches!(self, Self::Landed | Self::PartlyLanded)
    }
}

/// What one step of a batch did.
#[derive(Clone, Debug, PartialEq)]
pub enum StepOutcome {
    /// The command ran. In a dry run or a rolled-back batch its change is one the report planned.
    Ran(CommandOutcome),
    /// The command refused the step, in its own words and with no batch words added.
    Refused(CommandError),
}

/// One step of a batch, as it was dispatched.
#[derive(Clone, Debug, PartialEq)]
pub struct StepReport {
    command: String,
    outcome: StepOutcome,
}

impl StepReport {
    /// A step that ran, with what the command reported.
    fn ran(command: &str, outcome: CommandOutcome) -> Self {
        Self { command: command.to_owned(), outcome: StepOutcome::Ran(outcome) }
    }

    /// A step the command refused.
    fn refused(command: &str, error: CommandError) -> Self {
        Self { command: command.to_owned(), outcome: StepOutcome::Refused(error) }
    }

    /// The stable command id the step dispatched.
    pub fn command(&self) -> &str {
        &self.command
    }

    /// What the command did, or the sentence with which it refused.
    pub const fn outcome(&self) -> &StepOutcome {
        &self.outcome
    }

    /// Why the command refused the step, when it did.
    pub const fn refusal(&self) -> Option<&CommandError> {
        match &self.outcome {
            StepOutcome::Refused(error) => Some(error),
            StepOutcome::Ran(_) => None,
        }
    }
}

/// What a batch created, changed and removed, and whether the document holds it.
///
/// The three id lists are the batch's own changes: in a dry run they are what would land, and in a
/// rolled-back batch they are what was taken back. [`BatchReport::effect`] is the sentence that says
/// which of the two a caller is reading.
#[derive(Clone, Debug, PartialEq)]
pub struct BatchReport {
    effect: BatchEffect,
    steps: Vec<StepReport>,
    created: Vec<Id>,
    changed: Vec<Id>,
    removed: Vec<Id>,
    content_changed: bool,
}

impl BatchReport {
    /// What the batch did, or would do, to the document.
    pub const fn effect(&self) -> BatchEffect {
        self.effect
    }

    /// Whether the document holds the batch: what it ran is there, behind one undo entry.
    pub const fn landed(&self) -> bool {
        self.effect.landed()
    }

    /// One entry per step that was dispatched, in order, including the step that refused.
    pub fn steps(&self) -> &[StepReport] {
        &self.steps
    }

    /// The nodes the batch created, in tree order.
    pub fn created(&self) -> &[Id] {
        &self.created
    }

    /// The nodes the batch changed, in tree order: a rename, a move, a new material, new machining.
    pub fn changed(&self) -> &[Id] {
        &self.changed
    }

    /// The nodes the batch removed, in tree order.
    pub fn removed(&self) -> &[Id] {
        &self.removed
    }

    /// Whether the batch changed the document at all, even where no node id names the change (the
    /// active camera). An empty batch answers `false`.
    pub const fn content_changed(&self) -> bool {
        self.content_changed
    }

    /// The step that refused the batch, and why: its number counting from one, and the command's
    /// own sentence.
    pub fn refusal(&self) -> Option<(usize, &CommandError)> {
        self.steps
            .iter()
            .enumerate()
            .find_map(|(index, step)| step.refusal().map(|error| (index.saturating_add(1), error)))
    }

    /// Builds the report of a batch that ran against `before` and left `after`.
    pub(crate) fn new(effect: BatchEffect, steps: Vec<StepReport>, before: &Project, after: &Project) -> Self {
        let changes = changes(before, after);
        Self {
            effect,
            steps,
            created: changes.created,
            changed: changes.changed,
            removed: changes.removed,
            content_changed: !before.same_content(after),
        }
    }
}

/// Dispatches every step into `session` as one edit, stopping at the first refusal.
///
/// The steps share one scope, so a batch that lands is one undo entry. A refusal ends the list
/// because the steps after it were written against a document that does not hold the step before;
/// what the refusal costs is [`crate::Session::batch`]'s decision, not this loop's.
pub(crate) fn run(session: &mut Session, steps: &[BatchStep]) -> Vec<StepReport> {
    let scope = session.begin_edit();
    let mut reports = Vec::with_capacity(steps.len());
    for step in steps {
        match session.dispatch_in(scope, step.command(), step.params()) {
            Ok(outcome) => reports.push(StepReport::ran(step.command(), outcome)),
            Err(error) => {
                reports.push(StepReport::refused(step.command(), error));
                break;
            }
        }
    }
    session.end_edit();
    reports
}

/// The document ids a batch created, changed and removed.
struct Changes {
    created: Vec<Id>,
    changed: Vec<Id>,
    removed: Vec<Id>,
}

/// Reads what differs between two documents, by node id.
///
/// The ids come from the document's own walk, so the report cannot disagree with the tree: a node
/// the batch created is reported even when the command that made it could not say so, and a panel
/// whose machining changed is reported because the panel is a different value.
///
/// A container is compared by its own fields ([`model::Container::same_node`]) and not by its
/// children, so a panel created inside a product reports the panel alone; the root is compared the
/// same way, and is reported as changed when a batch renames or moves it.
fn changes(before: &Project, after: &Project) -> Changes {
    let before_ids = before.query(&NodeQuery::any());
    let after_ids = after.query(&NodeQuery::any());
    let old: HashSet<Id> = before_ids.iter().copied().collect();
    let new: HashSet<Id> = after_ids.iter().copied().collect();
    let created = after_ids.iter().copied().filter(|id| !old.contains(id)).collect();
    let removed = before_ids.iter().copied().filter(|id| !new.contains(id)).collect();
    let mut changed = Vec::new();
    if before.root().id() == after.root().id() && !before.root().same_node(after.root()) {
        changed.push(before.root().id());
    }
    changed.extend(before_ids.iter().copied().filter(|id| new.contains(id) && !same_node(before, after, *id)));
    Changes { created, changed, removed }
}

/// Whether the node `id` holds the same content in both documents.
fn same_node(before: &Project, after: &Project, id: Id) -> bool {
    match (before.node(id), after.node(id)) {
        (Some(Node::Container(left)), Some(Node::Container(right))) => left.same_node(right),
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}
