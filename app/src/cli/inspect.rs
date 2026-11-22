//! The inspection door: the rules' findings for a saved scene, as one JSON list.
//!
//! The pass is [`engine::Session::inspect`] and nothing else, so the command line, the tool server
//! and the assistant's panel cannot disagree about a finding (ADR 0191). The session is built the
//! way a recipe and a query build it — the scene, the build's catalogue and the contact adapter — so
//! a rule measures exactly what the window measures.
//!
//! Nothing is written: the file is read, the rules run in memory and the process exits. The answer
//! is the tool's own envelope, with the document it read beside the list, so a person can check a
//! scene and a client can compare the two doors.
//!
//! Changes when: a finding's wire shape changes, which is the tool's change too.

use engine::Finding;
use project::SceneFile;
use serde_json::{Value, json};

use super::{CliError, InspectArgs};

/// Reads the scene and prints the findings the rules prove, in the report's own order.
pub fn report(args: &InspectArgs) -> Result<String, CliError> {
    let file = args.scene.file_name().map(|name| name.to_string_lossy().into_owned());
    let scene = SceneFile::read(&args.scene).map_err(CliError::Scene)?;
    let mut session = super::recipe::session_of(scene.into_document())?;
    super::recipe::open_product(&mut session, args.product)?;
    let findings: Vec<Value> = session.inspect().iter().map(Finding::wire).collect();
    let envelope = json!({
        "document": {"file": file, "revision": session.project().revision().get()},
        "findings": findings,
    });
    serde_json::to_string_pretty(&envelope).map_err(|error| CliError::Query(error.to_string()))
}
