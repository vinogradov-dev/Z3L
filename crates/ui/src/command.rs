//! Small helpers shared by the editor panels for turning UI actions into engine commands.

use engine::{CommandOutcome, EditScope, Session};
use serde::Serialize;
use serde_json::Value;

pub(crate) fn json<T: Serialize>(params: &T) -> Result<Value, String> {
    serde_json::to_value(params).map_err(|error| format!("could not encode command parameters: {error}"))
}

pub(crate) fn dispatch(session: &mut Session, id: &str, params: &Value) -> Result<CommandOutcome, String> {
    session.dispatch(id, params).map_err(|error| error.to_string())
}

/// Dispatches one command as a step of an open edit scope (ADR 0092).
///
/// A gesture that writes more than once — a dragged field, a pulled height, a drawn base whose body
/// is still being chosen — dispatches in the scope its first write opened, so the whole gesture is
/// one undo entry. `None` is the plain one-entry write.
pub(crate) fn dispatch_in(
    scope: Option<EditScope>,
    session: &mut Session,
    id: &str,
    params: &Value,
) -> Result<CommandOutcome, String> {
    match scope {
        Some(scope) => session.dispatch_in(scope, id, params).map_err(|error| error.to_string()),
        None => dispatch(session, id, params),
    }
}
