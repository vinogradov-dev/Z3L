//! The JSON a tool answers with: one shape per call, read by a client we do not ship.
//!
//! The words are English and stable — they are the wire's contract, not a translation, the same
//! reading `z3l commands --json` takes. An answer never claims more than the engine measured: the
//! effect says whether the document holds the batch, the write says whether the file does, and a
//! refusal carries the command's own sentence instead of a summary of it (ADR 0185, ADR 0198).

use engine::{AgentMode, BatchEffect, BatchReport, BatchStep, StepOutcome};
use serde_json::{Value, json};

/// The sentence a save the caller asked for answers with while the scene has no path.
///
/// It names the person's own act, because a caller never chooses a path (ADR 0188) and a silent
/// write into a new file would put a document where nobody looks for it (X114).
pub const NO_PATH_NOTE: &str = "The scene has no path yet, so there is no file to write: a caller \
     never chooses one, and the person saves the scene once with Save as — after that `save` reaches \
     the file that names.";

/// What writing the document did after a batch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Write {
    /// Nothing landed, so there was nothing to write.
    Nothing,
    /// The document was written to the session's scene.
    Written,
    /// The session has no scene: it was served without one, so its document lives in memory only.
    NoFile,
    /// The document is the one a person is looking at, and the caller did not ask for a save.
    ///
    /// A batch that lands in an attached window is in the document and not on disk, and saying so
    /// is the whole point: the tab is dirty on purpose, and a file write nobody asked for could
    /// overwrite work the person has not saved yet (ADR 0005, X114).
    Window,
    /// The caller asked for a save while the scene has no path: the person's *Save as* names one.
    ///
    /// The document moved and no file holds it, exactly as a failed write, and the sentence that
    /// says what to do about it travels with the answer ([`NO_PATH_NOTE`]).
    NoPath,
    /// The write was attempted and failed, with the sentence that says why.
    Failed(String),
}

impl Write {
    /// Whether the answer must be read as a failure.
    ///
    /// A landed batch whose file could not be written is a failure a client has to see: the
    /// document moved and the file did not, and only the person in front of the client can decide
    /// what to do about it. A save the caller asked for on a scene with no path is the same failure
    /// one step earlier: the ask was honoured by the answer and not by the disk.
    pub const fn failed(&self) -> bool {
        matches!(self, Self::Failed(_) | Self::NoPath)
    }

    /// Whether the file holds the batch.
    pub const fn written(&self) -> bool {
        matches!(self, Self::Written)
    }

    /// The sentence a caller must read when the file does not hold the batch, if there is one.
    ///
    /// An I/O failure carries the writer's own words and a save with no path carries the one that
    /// names the person's act; neither is summarised here, because both are sentences a caller can
    /// act on (ADR 0198).
    pub const fn sentence(&self) -> Option<&str> {
        match self {
            Self::Failed(message) => Some(message.as_str()),
            Self::NoPath => Some(NO_PATH_NOTE),
            Self::Nothing | Self::Written | Self::NoFile | Self::Window => None,
        }
    }
}

/// The word a report uses for what a batch did, as the wire spells it.
const fn effect_word(effect: BatchEffect) -> &'static str {
    match effect {
        BatchEffect::Landed => "landed",
        BatchEffect::PartlyLanded => "partly_landed",
        BatchEffect::RolledBack => "rolled_back",
        BatchEffect::Planned => "planned",
    }
}

/// The sentence a plan answer carries: nothing was written, and the switch that would write is a
/// person's.
///
/// `mode: plan` alone leaves a caller to guess what it is looking at — a tool it has not learned, or
/// a door a person closed — and a client that guesses asks for the write harder. The sentence names
/// the place the switch lives and the key that opens it, so a caller can tell the person instead of
/// retrying (ADR 0190, X106).
///
/// The words are the wire's, not the interface's: the panel draws its own from `text::Message`. The
/// key is the inventory's binding, and the app asserts the spelling below against it, because `mcp`
/// sits beside `ui` and may not read the key table itself (ADR 0186).
pub const PLAN_NOTE: &str = "Nothing was written: plan and build are the switch a person turns in \
     the assistant panel, which the header's assistant button opens (Ctrl+J); no tool can change it.";

/// The report of one batch: its steps, the nodes it created, changed and removed, its refusal, the
/// mode that decided what it could do, what the file holds, and the product it ran in.
///
/// `open_product` is the session's view state read after the batch, so the answer says which product
/// the steps worked inside rather than leaving a caller to assume the one it named or the one the
/// person had open (ADR 0098, ADR 0212). A plan answer also carries [`PLAN_NOTE`], so the mode is
/// read as the person's switch and not as a tool the caller has yet to discover.
pub fn batch(
    report: &BatchReport,
    steps: &[BatchStep],
    write: &Write,
    revision: u64,
    mode: AgentMode,
    open_product: Option<model::Id>,
) -> Value {
    let rows: Vec<Value> = report
        .steps()
        .iter()
        .zip(steps)
        .map(|(report, step)| {
            let mut row = json!({"command": report.command(), "ok": report.refusal().is_none()});
            if let (StepOutcome::Ran(outcome), Some(object)) = (report.outcome(), row.as_object_mut())
                && let Some(created) = outcome.created_id()
            {
                object.insert("created".to_owned(), json!(created.get()));
            }
            if let (Some(error), Some(object)) = (report.refusal(), row.as_object_mut()) {
                object.insert("refusal".to_owned(), json!(error.to_string()));
            }
            if let (Some(name), Some(object)) = (step.name(), row.as_object_mut()) {
                // The name a step gave its node, beside the id the step reported, so a reader can map
                // a later step's reference to the node it resolved to without re-reading the request.
                object.insert("as".to_owned(), json!(name));
            }
            if step.params() != &Value::Object(serde_json::Map::new())
                && let Some(object) = row.as_object_mut()
            {
                // The parameters are echoed so a reader can tell two identical ids apart; a step
                // without parameters says nothing rather than an empty object per row. A reference
                // is echoed as the caller wrote it: the id it resolved to is the created id of the
                // step that named it, in that step's own row.
                object.insert("params".to_owned(), step.params().clone());
            }
            row
        })
        .collect();
    let mut body = json!({
        "effect": effect_word(report.effect()),
        "mode": mode.word(),
        "revision": revision,
        // The entries the batch left, beside the revision it moved to: one batch is one
        // operation, and the answer says so instead of leaving the caller to count steps (X128).
        "entries": report.entries(),
        "steps": rows,
        "created": ids(report.created()),
        "changed": ids(report.changed()),
        "removed": ids(report.removed()),
        "written": write.written(),
        "open_product": open_product.map(|id| id.get()),
    });
    if let Some(object) = body.as_object_mut() {
        object.insert("write".to_owned(), write_word(write));
        // The mode is the person's, and only a plan needs the sentence: a build answer carries the
        // document's own revision, which is what a caller acts on.
        if mode == AgentMode::Plan {
            object.insert("mode_note".to_owned(), json!(PLAN_NOTE));
        }
        if let Some((number, error)) = report.refusal() {
            let command = report.steps().get(number.saturating_sub(1)).map_or("", |step| step.command());
            object
                .insert("refusal".to_owned(), json!({"step": number, "command": command, "error": error.to_string()}));
        }
        if let Some(sentence) = write.sentence() {
            object.insert("write_error".to_owned(), json!(sentence));
        }
    }
    body
}

/// The one word that says what happened to the file.
///
/// It is the answer to "does the file hold what the document holds", and the six words are the six
/// honest answers: nothing landed, it was written, there is no file to write, the document is a
/// window's and the caller did not ask, the scene has no path to save to, the write failed.
fn write_word(write: &Write) -> Value {
    json!(match write {
        Write::Nothing => "nothing",
        Write::Written => "written",
        Write::NoFile => "no_file",
        Write::Window => "window",
        Write::NoPath => "no_path",
        Write::Failed(_) => "failed",
    })
}

/// The node ids of one list, as numbers.
fn ids(list: &[model::Id]) -> Value {
    Value::from(list.iter().map(|id| id.get()).collect::<Vec<u64>>())
}
