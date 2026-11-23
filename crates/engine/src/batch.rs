//! A batch: many commands, one edit scope, and a report a person reads before it lands.
//!
//! A batch is what a headless recipe and an assistant turn have in common: a list of commands that
//! belongs together, so undo takes the whole answer back in one step (ADR 0183, ADR 0187). Running
//! it is one operation of the session rather than a loop in each door, because the command line and
//! the tool that serves a model must give the same answer to the same list (ADR 0190, ADR 0198).
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
//! A step may name what it makes, and a later step may refer to that name instead of an id it cannot
//! know yet (J18). The name lives on the step and the reference lives in its parameters, so a build
//! is one call and one undo entry; the reference is resolved as the batch runs and never reaches the
//! command, so no document and no saved scene ever holds a placeholder.
//!
//! Changes when: a batch gains an option, or the report grows a field a door needs.

use std::collections::{HashMap, HashSet};

use model::{Id, Node, NodeQuery, Project};
use serde_json::{Map, Value, json};

use crate::commands::{CommandError, CommandOutcome, CommandRegistry};
use crate::session::Session;

/// The key that makes an object in a step's parameters a reference to an earlier step's node.
///
/// A reference is `{"$ref": "<name>"}` and nothing else: one key, a non-empty string, and no
/// other member, so a misspelling cannot be read as an ordinary parameter object and handed to a
/// command that would refuse it as a map where it wanted an id.
pub const REFERENCE_KEY: &str = "$ref";

/// How deep a step's parameters may nest a reference.
///
/// The parameters of a real command nest three or four levels; the bound is what keeps a hostile
/// request from walking a document-sized value recursively (ADR 0198).
const MAX_DEPTH: usize = 32;

/// One command of a batch: the stable id it dispatches, the parameters it takes, and the name a
/// later step calls what it made.
#[derive(Clone, Debug, PartialEq)]
pub struct BatchStep {
    command: String,
    params: Value,
    name: Option<String>,
}

impl BatchStep {
    /// A step that dispatches `command` with `params`.
    ///
    /// The parameters are the command's own typed struct in JSON, exactly what
    /// [`crate::Session::dispatch`] takes, so a step cannot say anything a command would not
    /// accept.
    pub fn new(command: impl Into<String>, params: Value) -> Self {
        Self { command: command.into(), params, name: None }
    }

    /// The same step under `name`, which a later step of the batch may refer to as
    /// `{"$ref": "<name>"}`.
    ///
    /// The name denotes the node the command created, so a step that creates nothing can be named
    /// and cannot be referred to: the reference answer says which step it was. A name is claimed
    /// when its step runs, and a second step may not take a name an earlier one already holds.
    pub fn named(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// The stable command id this step dispatches.
    pub fn command(&self) -> &str {
        &self.command
    }

    /// The parameters handed to that command, as the caller wrote them: a reference is resolved on
    /// the copy the command reads and never replaces what the report echoes back.
    pub const fn params(&self) -> &Value {
        &self.params
    }

    /// The name the step gives the node it creates, when it gives one.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
}

/// What a named step of a batch made, for a step that refers to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Made {
    /// The step's number, counting from one, so a refusal can name it.
    step: usize,
    /// The node the step created; `None` when it ran and created no node.
    node: Option<Id>,
}

/// What a batch is asked to do with the document.
///
/// The two switches are independent: `atomic` speaks about what a refusal costs, `dry_run` about
/// whether the document is written at all (ADR 0198). Four combinations, four honest answers.
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
    entries: usize,
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

    /// The undo entries this batch left, so one operation can be told from one command.
    ///
    /// One for a batch that landed as an operation of its own, none for a batch that merged into an
    /// open edit — a chat turn keeps its whole message in one entry (ADR 0187) — or that wrote
    /// nothing. The revision moves by one either way, because a batch is one operation (X128).
    pub const fn entries(&self) -> usize {
        self.entries
    }

    /// Records the entries the history gained, so the answer says what the batch left behind.
    pub(crate) const fn with_entries(mut self, entries: usize) -> Self {
        self.entries = entries;
        self
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
            entries: 0,
        }
    }
}

/// Dispatches every step into `session` as one edit, stopping at the first refusal.
///
/// The steps share one scope, so a batch that lands is one undo entry. A refusal ends the list
/// because the steps after it were written against a document that does not hold the step before;
/// what the refusal costs is [`crate::Session::batch`]'s decision, not this loop's.
///
/// A reference is resolved against the names the steps before it claimed, so a step cannot refer to
/// one that has not run: that is the same rule as "a batch stops at its first refusal", read from
/// the other side.
pub(crate) fn run(session: &mut Session, steps: &[BatchStep]) -> Vec<StepReport> {
    let scope = session.begin_edit();
    let mut reports = Vec::with_capacity(steps.len());
    let mut made: HashMap<String, Made> = HashMap::new();
    for (index, step) in steps.iter().enumerate() {
        let number = index.saturating_add(1);
        match step_in(session, scope, step, number, &mut made) {
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

/// One step of a batch: the name it claims, the parameters with every reference resolved, and the
/// command's own answer.
///
/// The name is claimed before the command runs, so a second step that takes it is refused instead of
/// silently shadowing the first, and what the step made is recorded after it ran. The parameters are
/// walked even when no step has named anything yet: a reference the batch cannot resolve — a name no
/// step gave, or one a later step would have given — is exactly the case where the map is empty, and
/// it must be a refusal rather than a map handed to a command that wants an id.
fn step_in(
    session: &mut Session,
    scope: crate::session::EditScope,
    step: &BatchStep,
    number: usize,
    made: &mut HashMap<String, Made>,
) -> Result<CommandOutcome, CommandError> {
    // An unknown command is dispatched as it stands: its own `unknown command` is the better
    // sentence, and resolving parameters for a command that does not exist would answer second.
    let Some(spec) = CommandRegistry::new().get(step.command()) else {
        return session.dispatch_in(scope, step.command(), step.params());
    };
    let command = spec.id();
    if let Some(name) = step.name() {
        if name.trim().is_empty() {
            return Err(invalid(command, "a step's name is not blank".to_owned()));
        }
        if let Some(other) = made.get(name) {
            return Err(invalid(command, format!("step {} already named `{name}`", other.step)));
        }
    }
    let params = resolved(step.params(), made, command)?;
    let outcome = session.dispatch_in(scope, command, &params)?;
    if let Some(name) = step.name() {
        made.insert(name.to_owned(), Made { step: number, node: outcome.created_id() });
    }
    Ok(outcome)
}

/// The parameters with every reference replaced by the id an earlier step made.
fn resolved(params: &Value, made: &HashMap<String, Made>, command: &'static str) -> Result<Value, CommandError> {
    resolved_in(params, made, command, 0)
}

/// One value of the parameters, resolved at `depth` levels below the top.
fn resolved_in(
    value: &Value,
    made: &HashMap<String, Made>,
    command: &'static str,
    depth: usize,
) -> Result<Value, CommandError> {
    if depth > MAX_DEPTH {
        return Err(invalid(command, format!("its parameters nest deeper than {MAX_DEPTH} levels")));
    }
    match value {
        Value::Object(object) => match reference(object, made, command)? {
            Some(id) => Ok(json!(id.get())),
            None => {
                let mut resolved = Map::with_capacity(object.len());
                for (key, inner) in object {
                    resolved.insert(key.clone(), resolved_in(inner, made, command, depth.saturating_add(1))?);
                }
                Ok(Value::Object(resolved))
            }
        },
        Value::Array(items) => {
            let resolved: Result<Vec<Value>, CommandError> =
                items.iter().map(|item| resolved_in(item, made, command, depth.saturating_add(1))).collect();
            Ok(Value::Array(resolved?))
        }
        other => Ok(other.clone()),
    }
}

/// The id `object` refers to, when it is a reference at all.
///
/// An object that carries [`REFERENCE_KEY`] is a reference or a mistake, never an ordinary value: it
/// must hold that one key and a non-empty name, and the name must be one an earlier step claimed.
fn reference(
    object: &Map<String, Value>,
    made: &HashMap<String, Made>,
    command: &'static str,
) -> Result<Option<Id>, CommandError> {
    let Some(value) = object.get(REFERENCE_KEY) else {
        return Ok(None);
    };
    let Some(Value::String(name)) = Some(value) else {
        return Err(invalid(command, format!("`{REFERENCE_KEY}` names a step by its string name, not {value}")));
    };
    if object.len() != 1 {
        return Err(invalid(
            command,
            format!("a reference is `{{\"{REFERENCE_KEY}\": \"<name>\"}}` and holds no other key"),
        ));
    }
    if name.trim().is_empty() {
        return Err(invalid(command, format!("`{REFERENCE_KEY}` names no step: the name is blank")));
    }
    match made.get(name) {
        Some(Made { node: Some(id), .. }) => Ok(Some(*id)),
        Some(Made { step, node: None }) => {
            Err(invalid(command, format!("`{name}` is step {step}'s name, and step {step} created no node")))
        }
        None => Err(invalid(command, format!("`{name}` is not a name any earlier step of this batch gave"))),
    }
}

/// A step's own refusal, tagged with the command that would have run.
fn invalid(command: &'static str, message: String) -> CommandError {
    CommandError::InvalidParams { command, message }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The names three imagined steps claimed: one node, one node and a step that made nothing.
    fn made() -> HashMap<String, Made> {
        let mut made = HashMap::new();
        made.insert("side".to_owned(), Made { step: 1, node: Id::new(7) });
        made.insert("shelf".to_owned(), Made { step: 2, node: Id::new(9) });
        made.insert("rename".to_owned(), Made { step: 3, node: None });
        made
    }

    const COMMAND: &str = "panel.group";

    /// The refusal of a parameter set, as the sentence it prints.
    fn refusal(params: &Value) -> String {
        match resolved(params, &made(), COMMAND) {
            Ok(value) => panic!("the parameters resolved to {value}"),
            Err(error) => error.to_string(),
        }
    }

    #[test]
    fn a_reference_becomes_the_id_an_earlier_step_made() {
        assert_eq!(
            resolved(&json!({ "panel": { "$ref": "side" }, "edge": 3 }), &made(), COMMAND).expect("resolves"),
            json!({ "panel": 7, "edge": 3 })
        );
    }

    #[test]
    fn a_reference_is_found_inside_arrays_and_alongside_ordinary_values() {
        let params = json!({ "nodes": [{ "$ref": "side" }, 4, { "$ref": "shelf" }], "name": "$ref" });
        assert_eq!(
            resolved(&params, &made(), COMMAND).expect("resolves"),
            json!({ "nodes": [7, 4, 9], "name": "$ref" }),
            "a plain string that spells the key is a value, not a reference"
        );
    }

    #[test]
    fn a_name_no_earlier_step_gave_is_a_refusal() {
        let sentence = refusal(&json!({ "panel": { "$ref": "ghost" } }));
        assert!(sentence.contains("`ghost`"), "{sentence}");
        assert!(sentence.contains("not a name any earlier step"), "{sentence}");
    }

    #[test]
    fn a_name_a_step_made_nothing_under_is_a_refusal_naming_the_step() {
        let sentence = refusal(&json!({ "panel": { "$ref": "rename" } }));
        assert!(sentence.contains("step 3"), "{sentence}");
        assert!(sentence.contains("created no node"), "{sentence}");
    }

    #[test]
    fn a_reference_that_is_not_one_name_is_a_refusal() {
        let sentence = refusal(&json!({ "panel": { "$ref": 7 } }));
        assert!(sentence.contains("string name"), "{sentence}");
        let sentence = refusal(&json!({ "panel": { "$ref": "side", "edge": 3 } }));
        assert!(sentence.contains("holds no other key"), "{sentence}");
        let sentence = refusal(&json!({ "panel": { "$ref": "  " } }));
        assert!(sentence.contains("blank"), "{sentence}");
    }

    #[test]
    fn parameters_that_nest_too_deep_are_a_refusal_not_a_stack() {
        let mut params = json!("deep");
        for _ in 0..MAX_DEPTH {
            params = json!([params]);
        }
        let sentence = refusal(&json!({ "panel": params }));
        assert!(sentence.contains("nest deeper"), "{sentence}");
    }
}
