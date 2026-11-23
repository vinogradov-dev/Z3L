//! The door a chat turn is: one message's edit scope, and the reads a model may make.
//!
//! A turn in the window is not a second way into the document. Its batch is
//! [`Session::agent_batch_in`] inside the one edit scope of the message (ADR 0187), and its reads
//! are [`listing`], [`asking`] and [`findings`] — the same functions the tool server answers a
//! foreign client with, so a model in the panel and a model over MCP read one answer and cannot be
//! told two different things about one build (ADR 0185, ADR 0191).
//!
//! The reads a model may make are the tool server's own three: `command_list` (the ids and the
//! schema of one), `query` (the tree, a box, the parts, the diagnostics, a niche) and `inspect` (the
//! rules' findings). A name that is none of them is answered with the sentence that says so,
//! because a model learns from a refusal and a turn that ends on an unknown tool teaches nothing.
//!
//! Changes when: the read tools change, or the shell stops owning the session a turn edits.

use engine::{BatchOptions, BatchReport, BatchStep, EditScope, Session};

use crate::chat::{BATCH_TOOL, CHECKLIST_TOOL, ToolCall, Tools};
use crate::server::{asking, findings, listing};
use crate::tools::{Answer, CommandListRequest, FindingsRequest, TOOL_FINDINGS, TOOL_LIST, TOOL_QUERY};

/// The door one chat message is: the session it edits, the scope every batch of it lands in, and
/// the reads the model may ask for between the batches.
#[derive(Debug)]
pub struct MessageDoor<'a> {
    session: &'a mut Session,
    scope: EditScope,
}

impl<'a> MessageDoor<'a> {
    /// The door over `session`, whose every batch lands inside `scope`.
    pub const fn new(session: &'a mut Session, scope: EditScope) -> Self {
        Self { session, scope }
    }
}

impl Tools for MessageDoor<'_> {
    fn dispatch(&mut self, steps: &[BatchStep]) -> BatchReport {
        self.session.agent_batch_in(self.scope, steps, BatchOptions::ALL_OR_NOTHING)
    }

    fn read(&mut self, call: &ToolCall) -> Answer {
        answer(self.session, call)
    }
}

/// One read tool answered, as the tool server answers it.
///
/// The arguments are parsed from the call as it arrived, so a call the model mistyped is refused in
/// the model's own words and the turn goes on: the parser is the tool's, and the tool's own sentence
/// is what it can act on.
pub fn answer(session: &Session, call: &ToolCall) -> Answer {
    match call.name.as_str() {
        TOOL_LIST => match serde_json::from_value::<CommandListRequest>(call.arguments.clone()) {
            Ok(request) => listing(session, request.command.as_deref()),
            Err(error) => Answer::refused(format!("`{}` cannot read its arguments: {error}", call.name)),
        },
        TOOL_QUERY => match serde_json::from_value(call.arguments.clone()) {
            Ok(query) => asking(session, query),
            Err(error) => Answer::refused(format!("`{}` cannot read its arguments: {error}", call.name)),
        },
        TOOL_FINDINGS => match serde_json::from_value::<FindingsRequest>(call.arguments.clone()) {
            Ok(_) => Answer::ok(findings(session)),
            Err(error) => Answer::refused(format!("`{}` cannot read its arguments: {error}", call.name)),
        },
        BATCH_TOOL | CHECKLIST_TOOL => {
            Answer::refused(format!("`{}` is the turn's own call and is not read", call.name))
        }
        other => Answer::refused(format!(
            "`{other}` is not a read the assistant makes: it knows `{TOOL_LIST}`, `{TOOL_QUERY}` and \
             `{TOOL_FINDINGS}`"
        )),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use serde_json::json;
    use testkit::{sample_catalog, sample_product_project};

    use super::*;

    fn session() -> Session {
        let (project, _product) = sample_product_project();
        Session::with_directory(project, sample_catalog())
    }

    #[test]
    fn a_read_is_answered_by_the_same_functions_the_tool_server_uses() {
        // The panel's model and a foreign client read one answer: `command_list` and `query` are
        // the server's own, and there is no second projection of the registry (ADR 0185).
        let session = session();
        let listed = answer(&session, &ToolCall::new("c1", TOOL_LIST, json!({ "command": "panel.create" })));
        assert!(!listed.failed(), "one command's row parses: {:?}", listed.body());
        assert_eq!(listed.body(), listing(&session, Some("panel.create")).body(), "the same answer, byte for byte");

        let asked = answer(&session, &ToolCall::new("c2", TOOL_QUERY, json!({ "kind": "tree" })));
        assert!(!asked.failed(), "the tree is a question the engine answers: {:?}", asked.body());
        assert_eq!(asked.body(), asking(&session, engine::Query::Tree { node: None }).body());

        let found = answer(&session, &ToolCall::new("c3", TOOL_FINDINGS, json!({})));
        assert_eq!(found.body(), &findings(&session));
    }

    #[test]
    fn a_tool_the_door_does_not_offer_is_a_sentence_and_not_the_end_of_a_turn() {
        // A model that invents a name is told which names exist, in the history it reads back.
        let refused = answer(&session(), &ToolCall::new("c1", "read_the_file", json!({ "path": "/etc/passwd" })));
        assert!(refused.failed());
        let sentence = refused.body().get("error").and_then(serde_json::Value::as_str).unwrap_or_default();
        assert!(sentence.contains(TOOL_LIST), "the refusal names what it may read: {sentence}");
        assert!(sentence.contains(TOOL_QUERY) && sentence.contains(TOOL_FINDINGS), "{sentence}");

        let mistyped = answer(&session(), &ToolCall::new("c2", TOOL_QUERY, json!({ "kind": "everythin" })));
        assert!(mistyped.failed(), "a misspelt kind is refused instead of guessed");
    }
}
