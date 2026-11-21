//! What this build can dispatch: one row per command id.
//!
//! The registry is the only list, so a surface that discovers commands cannot drift from the
//! dispatcher: an id printed here is an id [`engine::Session::dispatch`] accepts. A row says the id,
//! the English label, what the command creates — the level the class rules gate (ADR 0098) — and the
//! JSON Schema of its parameters, derived from the typed struct the command parses (ADR 0185). The
//! schema is the parser's own description, so a caller builds a request from the report instead of
//! from our source, and a wrong one still comes back as the command's own sentence.
//!
//! Changes when: a command gains a field the discovery report must carry.

use engine::{CommandRegistry, CommandSpec};
use serde_json::{Value, json};
use text::Lang;

/// One command as a report row: its id, its label, the level it creates and its parameter schema.
fn row(spec: CommandSpec) -> Value {
    let mut row = json!({
        "id": spec.id(),
        "label": spec.label().text(Lang::En),
        "params": spec.params_schema(),
    });
    if let (Some(word), Some(object)) = (spec.creation_word(), row.as_object_mut()) {
        object.insert("creates".to_owned(), Value::from(word));
    }
    row
}

/// The discovery report: every command id, as one JSON array or as one aligned line each.
///
/// The text form is for a person scanning ids in a terminal; the JSON form is what a recipe author
/// or an assistant reads, because only the JSON form can carry a schema. Both name the same
/// commands and the same levels, so neither is a second truth.
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
            match spec.creation_word() {
                Some(word) => format!("{:<width$}  {label}  (creates a {word})", spec.id()),
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
    fn every_command_carries_the_schema_of_its_parameters() {
        let rows: Vec<Value> = serde_json::from_str(&report(true)).expect("an array");
        for row in &rows {
            let schema = &row["params"];
            assert_eq!(schema["type"], "object", "{} takes one parameter object", row["id"]);
            assert_eq!(schema["additionalProperties"], false, "{} denies unknown fields", row["id"]);
            assert!(
                schema["properties"].as_object().is_some_and(|properties| !properties.is_empty()),
                "{} describes no field",
                row["id"]
            );
        }
    }

    #[test]
    fn the_schema_a_caller_reads_is_the_one_the_parser_enforces() {
        // A caller builds a request from this row: the five fields a `panel.create` request must
        // name, and the three it may leave out, are the derive's reading of the same `serde`
        // attributes the command's parser reads.
        let rows: Vec<Value> = serde_json::from_str(&report(true)).expect("an array");
        let create = rows.iter().find(|row| row["id"] == "panel.create").expect("panel.create is registered");
        assert_eq!(create["params"]["required"], json!(["parent", "name", "nominal_size", "material", "placement"]));
        let required = create["params"]["required"].as_array().expect("required is a list");
        for optional in ["class", "outline", "bevels"] {
            assert!(create["params"]["properties"][optional].is_object(), "{optional} is described");
            assert!(!required.iter().any(|name| name == optional), "{optional} may be left out");
        }
    }

    #[test]
    fn the_text_form_names_the_level_and_the_json_agrees() {
        let text = report(false);
        assert!(text.contains("panel.create"), "{text}");
        assert!(text.contains("(creates a part)"), "the text row says what it creates: {text}");
        assert!(report(true).starts_with('['), "the JSON form is one array");
    }
}
