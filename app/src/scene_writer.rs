//! The one writer of a scene file: a path, the sections this build must give back, and the atomic
//! write the `project` crate performs.
//!
//! It is the composition root's implementation of [`mcp::SceneSink`], and the shell's own save uses
//! the same call, so a scene written for a caller and a scene written by the person's *Save* cannot
//! disagree about the bytes: the file's own atomic write and the top-level sections a newer build
//! left ride on both (ADR 0096, ADR 0185). The path is the one the scene was opened from or last
//! saved to — a caller never names one (ADR 0188) — and the writer knows nothing else: no document,
//! no session, no dirty marker.

use std::path::PathBuf;

use model::Project;
use project::{SceneFile, SceneSettings};

use crate::file_menu::PreservedSections;

/// Writes one scene to one file, carrying the sections this build does not understand.
#[derive(Debug)]
pub(crate) struct SceneWriter {
    /// The file the scene was opened from or last saved to, written again on every save.
    path: PathBuf,
    /// The top-level sections a newer build left in the file, given back on every write.
    extra: PreservedSections,
    /// The factory's numbers the scene was built with, given back with it (ADR 0240).
    settings: SceneSettings,
}

impl SceneWriter {
    /// A writer for `path` that gives `extra` back with the document it writes.
    pub(crate) fn new(path: impl Into<PathBuf>, extra: PreservedSections) -> Self {
        Self { path: path.into(), extra, settings: SceneSettings::default() }
    }

    /// Carries the scene's own factory numbers, so a save gives them back unchanged.
    pub(crate) fn with_settings(mut self, settings: SceneSettings) -> Self {
        self.settings = settings;
        self
    }

    /// Writes `project` to this writer's file, or the sentence that says why it could not.
    pub(crate) fn write(&self, project: &Project) -> Result<(), String> {
        SceneFile::new(project.clone())
            .with_extra(self.extra.clone())
            .with_settings(self.settings)
            .write(&self.path)
            .map_err(|error| {
                let file = self.path.display();
                format!("the scene could not be written to {file}: {error}")
            })
    }
}

impl mcp::SceneSink for SceneWriter {
    fn save(&self, project: &Project) -> Result<(), String> {
        self.write(project)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;
    use serde_json::json;
    use testkit::{sample_project, scratch_dir};

    #[test]
    fn a_section_this_build_does_not_know_rides_back_on_the_next_write() {
        // ADR 0096: an older build never silently destroys a newer build's data.
        let directory = scratch_dir("scene-writer-kept");
        let path = directory.join("scene.z3l");
        let writer = SceneWriter::new(path.clone(), PreservedSections::new());
        writer.write(&sample_project()).expect("the fixture is written");

        let mut envelope: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("the fixture reads"))
                .expect("a scene envelope");
        if let Some(object) = envelope.as_object_mut() {
            object.insert("future_section".to_owned(), json!({"kept": true}));
        }
        std::fs::write(&path, serde_json::to_string(&envelope).expect("the envelope serializes"))
            .expect("the fixture is written");

        let mut extra = PreservedSections::new();
        extra.insert("future_section".to_owned(), json!({"kept": true}));
        let writer = SceneWriter::new(path.clone(), extra);
        writer.write(&sample_project()).expect("the scene is written again");

        let written: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("the scene reads back"))
                .expect("a scene envelope");
        assert_eq!(written["future_section"], json!({"kept": true}), "the unknown section rode back");
    }

    #[test]
    fn a_refused_write_names_the_file_it_could_not_write() {
        let directory = scratch_dir("scene-writer-refused");
        // A directory is not a scene file: the write is refused by the filesystem itself.
        let path = directory.join("a-folder");
        std::fs::create_dir_all(&path).expect("the folder is made");
        let refused = SceneWriter::new(path.clone(), PreservedSections::new())
            .write(&sample_project())
            .expect_err("a folder is not a file");
        assert!(refused.contains(&path.display().to_string()), "the sentence names the file: {refused}");
    }
}
