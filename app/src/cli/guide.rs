//! The domain guide on the command line: one JSON value, the same one the tool server answers.
//!
//! A capability the assistant has must be reachable from a shell too (AGENTS.md §4.11), and the
//! guide is the capability that says what the program is for. There is one value, in
//! [`engine::guide`], so a script and a model reading `command_list` cannot be told two different
//! things about the program.
//!
//! Changes when: nothing — the guide's fields are the engine's; this module only prints them.

/// The guide as pretty JSON, ready for a terminal or a pipe.
///
/// A value that cannot be serialized is answered with an empty object instead of a failure: the
/// guide is a constant tree of owned values, and a caller that reads `{}` sees that something is
/// wrong without a panic in the shell.
pub fn report() -> String {
    serde_json::to_string_pretty(&engine::guide()).unwrap_or_else(|_| "{}".to_owned())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    #[test]
    fn the_line_prints_the_engine_guide() {
        let printed = report();
        let value: serde_json::Value = serde_json::from_str(&printed).expect("the report is JSON");
        assert_eq!(value, engine::guide(), "the command line and the tool server read one value");
        assert_eq!(value["worlds"].as_array().map(Vec::len), Some(4), "the four families");
    }
}
