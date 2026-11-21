//! The JSON a tool answers with: one shape per call, read by a client we do not ship.
//!
//! The words are English and stable — they are the wire's contract, not a translation, the same
//! reading `z3l commands --json` takes. An answer never claims more than the engine measured: the
//! effect says whether the document holds the batch, the write says whether the file does, and a
//! refusal carries the command's own sentence instead of a summary of it (ADR 0185, ADR 0198).

use engine::{AgentMode, BatchEffect, BatchReport, BatchStep, StepOutcome};
use serde_json::{Value, json};

/// What writing the document did after a batch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Write {
    /// Nothing landed, so there was nothing to write.
    Nothing,
    /// The document was written to the session's scene.
    Written,
    /// The session has no scene: it was served without one, so its document lives in memory only.
    NoFile,
    /// The document is the one a person is looking at, so the window's own *Save* is the write.
    ///
    /// A batch that lands in an attached window is in the document and not on disk, and saying so
    /// is the whole point: the tab is dirty on purpose, and a file write nobody asked for could
    /// overwrite work the person has not saved yet.
    Window,
    /// The write was attempted and failed, with the sentence that says why.
    Failed(String),
}

impl Write {
    /// Whether the answer must be read as a failure.
    ///
    /// A landed batch whose file could not be written is a failure a client has to see: the
    /// document moved and the file did not, and only the person in front of the client can decide
    /// what to do about it.
    pub const fn failed(&self) -> bool {
        matches!(self, Self::Failed(_))
    }

    /// Whether the file holds the batch.
    pub const fn written(&self) -> bool {
        matches!(self, Self::Written)
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

/// The report of one batch: its steps, the nodes it created, changed and removed, its refusal, the
/// mode that decided what it could do, and what the file holds.
pub fn batch(report: &BatchReport, steps: &[BatchStep], write: &Write, revision: u64, mode: AgentMode) -> Value {
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
            if step.params() != &Value::Object(serde_json::Map::new())
                && let Some(object) = row.as_object_mut()
            {
                // The parameters are echoed so a reader can tell two identical ids apart; a step
                // without parameters says nothing rather than an empty object per row.
                object.insert("params".to_owned(), step.params().clone());
            }
            row
        })
        .collect();
    let mut body = json!({
        "effect": effect_word(report.effect()),
        "mode": mode.word(),
        "revision": revision,
        "steps": rows,
        "created": ids(report.created()),
        "changed": ids(report.changed()),
        "removed": ids(report.removed()),
        "written": write.written(),
    });
    if let Some(object) = body.as_object_mut() {
        object.insert("write".to_owned(), write_word(write));
        if let Some((number, error)) = report.refusal() {
            let command = report.steps().get(number.saturating_sub(1)).map_or("", |step| step.command());
            object
                .insert("refusal".to_owned(), json!({"step": number, "command": command, "error": error.to_string()}));
        }
        if let Write::Failed(message) = write {
            object.insert("write_error".to_owned(), json!(message));
        }
    }
    body
}

/// The one word that says what happened to the file.
///
/// It is the answer to "does the file hold what the document holds", and the five words are the
/// five honest answers: nothing landed, it was written, there is no file to write, the document is a
/// window's and its own *Save* writes it, the write failed.
fn write_word(write: &Write) -> Value {
    json!(match write {
        Write::Nothing => "nothing",
        Write::Written => "written",
        Write::NoFile => "no_file",
        Write::Window => "window",
        Write::Failed(_) => "failed",
    })
}

/// The node ids of one list, as numbers.
fn ids(list: &[model::Id]) -> Value {
    Value::from(list.iter().map(|id| id.get()).collect::<Vec<u64>>())
}
