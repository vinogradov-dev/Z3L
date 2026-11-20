//! What this build can dispatch: one row per command id.
//!
//! The registry is the only list, so a surface that discovers commands cannot drift from the
//! dispatcher: an id printed here is an id [`engine::Session::dispatch`] accepts. A row says the id,
//! the English label and what the command creates — the level the class rules gate (ADR 0098) — and
//! nothing about the parameters, because a command's parameters are a typed struct with no schema
//! yet (`plan/debt.md`, the command-spec entry): a caller writes them from the command's own source
//! and the report tells it whether the document accepted them.
//!
//! Changes when: a command gains a field the discovery report must carry.

use engine::{CommandRegistry, CommandSpec};
use model::Level;
use serde_json::{Value, json};
use text::Lang;

/// The word a report uses for a class level.
///
/// The model names the levels but spells none of them for a machine, and this report is read by an
/// agent, so the words are English and stable: they are part of the report's contract, not a
/// translation. A new level turns the match into a compile error instead of printing a number.
const fn level_word(level: Level) -> &'static str {
    match level {
        Level::Product => "product",
        Level::Component => "component",
        Level::Part => "part",
        Level::Fitting => "fitting",
    }
}

/// One command as a report row: its id, its label and the level it creates.
fn row(spec: CommandSpec) -> Value {
    let mut row = json!({
        "id": spec.id(),
        "label": spec.label().text(Lang::En),
    });
    if let (Some(level), Some(object)) = (spec.creation_level(), row.as_object_mut()) {
        object.insert("creates".to_owned(), Value::from(level_word(level)));
    }
    row
}

/// The discovery report: every command id, as one JSON array or as one aligned line each.
///
/// The text form is for a person scanning ids in a terminal; the JSON form is what a recipe author
/// or an assistant reads. Both carry the same three fields, so neither is a second truth.
pub fn report(as_json: bool) -> String {
    let specs = CommandRegistry::new().specs();
    if as_json {
        let rows: Vec<Value> = specs.iter().map(|spec| row(*spec)).collect();
        return serde_json::to_string_pretty(&Value::Array(rows)).unwrap_or_default();
    }
    let width = specs.iter().map(|spec| spec.id().len()).max().unwrap_or(0);
    let lines: Vec<String> = specs
        .iter()
        .map(|spec| {
            let label = spec.label().text(Lang::En);
            match spec.creation_level() {
                Some(level) => format!("{:<width$}  {label}  (creates a {})", spec.id(), level_word(level)),
                None => format!("{:<width$}  {label}", spec.id()),
            }
        })
        .collect();
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    #[test]
    fn every_command_is_listed_once_with_its_label() {
        let specs = CommandRegistry::new().specs();
        let text = report(false);
        assert_eq!(text.lines().count(), specs.len(), "one line per command");
        let json = report(true);
        let rows: Vec<Value> = serde_json::from_str(&json).expect("the report is a JSON array");
        assert_eq!(rows.len(), specs.len());
        for (row, spec) in rows.iter().zip(specs) {
            assert_eq!(row["id"], spec.id(), "the id is the registry's");
            assert!(row["label"].as_str().is_some_and(|label| !label.is_empty()), "{} has a label", spec.id());
        }
    }

    #[test]
    fn a_creating_command_says_what_it_creates() {
        let rows: Vec<Value> = serde_json::from_str(&report(true)).expect("an array");
        let create = rows.iter().find(|row| row["id"] == "panel.create").expect("panel.create is registered");
        assert_eq!(create["creates"], "part", "a panel is a part");
        let rename = rows.iter().find(|row| row["id"] == "node.rename").expect("node.rename is registered");
        assert!(rename.get("creates").is_none(), "a rename creates nothing");
    }

    #[test]
    fn the_text_form_names_the_level_and_the_json_agrees() {
        let text = report(false);
        assert!(text.contains("panel.create"), "{text}");
        assert!(text.contains("(creates a part)"), "the text row says what it creates: {text}");
        assert!(report(true).starts_with('['), "the JSON form is one array");
    }
}
