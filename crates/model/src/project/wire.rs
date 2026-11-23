//! The serialized envelope of a project.
//!
//! Loading reads the document into a value, checks the schema number, and only then parses the tree
//! through a wire struct and validates it. The number comes first because it is the one fact that
//! tells a person which build wrote the file: read afterwards, a file from an older layout fails
//! inside the first field that moved, and every schema bump makes that worse (ADR 0096). Unknown
//! top-level fields are rejected; the reserved property keys are validated on the same pass. Exactly
//! one schema is accepted — the current one — so a file from any other build is a readable error and
//! never a silent conversion.

use serde::Deserialize;
use serde::de::{self, Deserializer};
use serde_json::Value;

use crate::project::Project;
use crate::tree::find_node;
use crate::*;

use super::{next_id_after, scan_tree};

/// The top-level field that names the schema which wrote the document.
const SCHEMA_FIELD: &str = "schema_version";

impl<'de> Deserialize<'de> for Project {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::from_value(Value::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

impl Project {
    /// Reads a document from a JSON value, refusing another schema before it reads the tree.
    ///
    /// This is what the scene envelope calls: the value a caller already holds is moved into the
    /// typed document rather than copied, so a file costs one JSON tree. A body that carries no
    /// number, or a number another build wrote, answers with the schema sentence whatever its
    /// layout; a body that carries the current number and is still broken answers with the field
    /// that broke, because the layout is genuinely wrong.
    pub fn from_value(value: Value) -> Result<Self, serde_json::Error> {
        check_schema(&value).map_err(de::Error::custom)?;
        let wire = serde_json::from_value::<ProjectWire>(value)?;
        Self::try_from(wire).map_err(de::Error::custom)
    }
}

/// Refuses a document whose schema number is absent or is not the current one.
fn check_schema(value: &Value) -> Result<(), ProjectError> {
    let supported = Project::CURRENT_SCHEMA_VERSION;
    match value.get(SCHEMA_FIELD) {
        Some(found) if found.as_u64() == Some(u64::from(supported)) => Ok(()),
        // The number as the file spells it, so a string or a fraction names itself instead of
        // reading as "invalid type", and an absent field says so.
        found => Err(ProjectError::UnsupportedSchemaVersion {
            found: found.map_or_else(|| "missing".to_owned(), Value::to_string),
            supported,
        }),
    }
}

/// Serialized form of [`Project`]; validation happens in its `TryFrom` implementation.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProjectWire {
    schema_version: u32,
    revision: Revision,
    #[serde(default)]
    next_id: Option<Id>,
    root: Container,
    #[serde(default)]
    active_camera: Option<Id>,
}

impl TryFrom<ProjectWire> for Project {
    type Error = ProjectError;

    fn try_from(wire: ProjectWire) -> Result<Self, Self::Error> {
        if wire.schema_version != Project::CURRENT_SCHEMA_VERSION {
            return Err(ProjectError::UnsupportedSchemaVersion {
                found: wire.schema_version.to_string(),
                supported: Project::CURRENT_SCHEMA_VERSION,
            });
        }
        // The root defines the world frame and has no pivot to edit, so a stored offset is a
        // contradiction and not something to normalize away.
        let root = wire.root;
        if root.pivot_local() != PivotLocal::ZERO {
            return Err(ProjectError::RootPivotNotZero { found: root.pivot_local() });
        }
        let max_id = scan_tree(&root)?;
        let next_id = match wire.next_id {
            Some(next_id) if next_id.get() <= max_id => {
                return Err(ProjectError::NextIdNotAfterMax { next: next_id, max: max_id });
            }
            Some(next_id) => next_id,
            None => next_id_after(max_id)?,
        };
        let active_camera = match wire.active_camera {
            Some(id) if find_node(&root, id).is_some_and(|node| node.as_camera().is_some()) => Some(id),
            Some(id) => return Err(ProjectError::CameraNotFound { id }),
            None => None,
        };
        Ok(Self {
            schema_version: Project::CURRENT_SCHEMA_VERSION,
            revision: wire.revision,
            next_id,
            root,
            active_camera,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::fixtures::*;
    use crate::*;

    #[test]
    fn reserved_property_keys_are_validated_on_load() {
        let mut wire = serde_json::to_value(sample_project()).expect("serialize project");
        wire["root"]["properties"] = serde_json::json!({"component": {"kind": "text", "value": "yes"}});
        let error = serde_json::from_value::<Project>(wire).expect_err("invalid reserved property");
        assert!(error.to_string().contains("must be a boolean"), "error was {error}");
    }

    #[test]
    fn projects_reject_any_other_schema_and_unknown_fields() {
        let project = sample_project();
        let text = serde_json::to_string(&project).expect("serialize project");
        let current = format!("\"schema_version\":{}", Project::CURRENT_SCHEMA_VERSION);

        for other in [Project::CURRENT_SCHEMA_VERSION + 1, Project::CURRENT_SCHEMA_VERSION - 1] {
            let replaced = text.replacen(&current, &format!("\"schema_version\":{other}"), 1);
            assert_ne!(replaced, text, "the fixture must carry the schema line");
            let error = serde_json::from_str::<Project>(&replaced).expect_err("another schema is refused");
            assert!(error.to_string().contains("unsupported schema version"), "error was {error}");
        }

        let prefix = text.strip_suffix('}').expect("project JSON is an object");
        let extra = format!("{prefix},\"extra\":1}}");
        assert!(serde_json::from_str::<Project>(&extra).is_err());
    }

    #[test]
    fn a_body_with_no_fields_answers_with_its_schema_and_not_a_field() {
        let current = Project::CURRENT_SCHEMA_VERSION;
        let older = serde_json::json!({ "schema_version": current - 1 });
        let error = serde_json::from_value::<Project>(older).expect_err("another schema is refused");
        assert!(error.to_string().contains("unsupported schema version"), "error was {error}");
        assert!(error.to_string().contains(&(current - 1).to_string()), "the error names the version found");
        assert!(!error.to_string().contains("missing field"), "the tree is not read at all: {error}");

        for absent in [serde_json::json!({}), serde_json::json!({ "schema_version": "current" })] {
            let error = serde_json::from_value::<Project>(absent).expect_err("a number is required");
            assert!(error.to_string().contains("unsupported schema version"), "error was {error}");
            assert!(!error.to_string().contains("missing field"), "the tree is not read at all: {error}");
        }
        let missing =
            serde_json::from_value::<Project>(serde_json::json!({})).expect_err("a document with no number is refused");
        assert!(missing.to_string().contains("missing"), "the absent number says so: {missing}");
    }

    #[test]
    fn a_body_with_the_current_schema_and_no_fields_is_a_field_error() {
        let body = serde_json::json!({ "schema_version": Project::CURRENT_SCHEMA_VERSION });
        let error = serde_json::from_value::<Project>(body).expect_err("the layout is broken");
        assert!(error.to_string().contains("missing field `revision`"), "error was {error}");
    }

    #[test]
    fn a_root_with_a_pivot_offset_is_rejected() {
        let mut wire = serde_json::to_value(sample_project()).expect("serialize project");
        wire["root"]["pivot_local"] = serde_json::json!([1000, 0, 0]);
        let error = serde_json::from_value::<Project>(wire).expect_err("the root defines the world frame");
        assert!(error.to_string().contains("root's pivot must be zero"), "error was {error}");
    }

    #[test]
    fn duplicate_machining_ids_are_rejected_on_load() {
        let project = sample_project();
        let mut wire = serde_json::to_value(&project).expect("serialize project");
        let panel = &mut wire["root"]["children"][0]["panel"];
        panel["machining"] = serde_json::json!([
            {
                "id": 2,
                "anchor": {"kind": "face", "side": "A"},
                "params": {
                    "kind": "cutout",
                    "shape": {"rectangle": {"x": 10_000, "y": 10_000, "width": 100_000, "height": 50_000}},
                    "depth": "through"
                }
            }
        ]);
        let error = serde_json::from_value::<Project>(wire).expect_err("a machining id repeats the panel id");
        assert!(error.to_string().contains("duplicate"), "error was {error}");
    }
}
