//! The workshop's nomenclature directory, read again when another application changes it.
//!
//! The directory is edited in its own application (ADR 0148, `docs/domain/nomenclature.md` §13), so
//! the editor depends on a revision it does not own. That revision is watched **off the frame
//! path**: a worker thread polls the folder's stamp, reads and validates a new snapshot when the
//! stamp moved, and hands it over; the frame only takes what the worker has already finished and
//! swaps it whole at a frame boundary. A frame therefore costs one non-blocking receive whether the
//! workshop has ten records or ten thousand.
//!
//! A folder is the whole workspace's, so the shell applies one snapshot to every open tab. What each
//! tab re-derives afterwards is its own: a panel's thickness and its mesh follow the rows they were
//! built from, and the undo history is untouched because a directory is not the document.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::Duration;

use catalog::{Directory, DirectorySource, FolderSource, Revision};

/// The folder beside the workspace settings that holds the connected packages.
///
/// Settings and the directory sit beside each other, so a workshop copies one folder and moves both.
pub(crate) const DIRECTORY_DIR: &str = "nomenclature";

/// How often the worker reads the folder's stamp.
///
/// A stat of each file every few hundred milliseconds is nothing, and a directory is edited by a
/// person rather than by a loop. What the interval must not become is a per-frame stat: the frame
/// reads a channel, which costs the same whether the folder is there or not.
#[cfg(not(test))]
const POLL: Duration = Duration::from_millis(400);

/// The interval a test build polls at, so a changed fixture does not cost the suite a wait.
#[cfg(test)]
const POLL: Duration = Duration::from_millis(2);

/// The directory a workspace starts from, and what the watcher must know to take over from it.
pub(crate) struct Start {
    /// The rows a new session opens with: the workshop's, or the starter package.
    pub(crate) directory: Directory,
    /// The stamp the folder was read at; a worker that starts here finds nothing new.
    pub(crate) revision: Option<Revision>,
    /// Why the workshop's folder was not read, when it was not. The caller shows the sentence and
    /// works from the rows it has, because a directory that broke is a fact about the workshop and
    /// not a reason to work from somebody else's numbers (`§13`).
    pub(crate) refusal: Option<String>,
}

/// Opens the workshop's directory at `root`, keeping the starter rows when the folder brings none.
///
/// `None` is a workspace with no config directory at all: there is nothing to watch and the starter
/// package is the whole directory.
pub(crate) fn open(root: Option<&Path>) -> Start {
    let Some(root) = root else {
        return Start { directory: starter(), revision: None, refusal: None };
    };
    let source = FolderSource::at(root);
    // The stamp is read before the snapshot, so a write that lands between the two reads moves the
    // stamp the watcher holds and is caught by its next poll instead of being lost.
    let revision = source.revision().ok();
    match load(&source) {
        Ok(directory) => Start { directory, revision, refusal: None },
        Err(refusal) => Start { directory: starter(), revision, refusal: Some(refusal) },
    }
}

/// Reads one whole snapshot, or the sentence that refused it.
///
/// A folder that holds no package is the starter package, not an empty directory: a workshop that
/// connected nothing still has rows to work from, and an empty directory would leave every panel
/// card with nothing to offer. A single bad file is a refusal and never a half-loaded directory.
fn load(source: &FolderSource) -> Result<Directory, String> {
    let directory = source.snapshot().map_err(|error| error.to_string())?;
    if directory.is_empty() {
        return Ok(starter());
    }
    Ok(directory)
}

/// The bundled package every workshop starts from.
fn starter() -> Directory {
    catalog::default_directory().unwrap_or_default()
}

/// A newer directory, or the sentence that refused one, waiting for a frame boundary.
pub(crate) enum Reload {
    /// A whole snapshot, validated off the frame path and ready to swap in.
    Loaded(Arc<Directory>),
    /// A revision this build cannot read; the rows in use stay in place.
    Refused(String),
}

/// The worker that watches one folder, and the channel it hands snapshots over.
///
/// The shell owns one and drops it with the window. The worker spends its wait on a channel with a
/// timeout, so a closed channel ends it on the next tick and the thread never outlives the
/// application that started it.
pub(crate) struct Watch {
    inbox: Receiver<Reload>,
    /// Kept only so the worker's wait can end; nothing is ever sent on it.
    _tick: Option<Sender<()>>,
}

impl Watch {
    /// Watches `root` from `revision`, handing every later snapshot to the shell.
    ///
    /// `revision` is the stamp of the same folder read a moment earlier: a worker that started
    /// anywhere else would reload the rows it was handed, and a worker with no folder does nothing
    /// at all.
    pub(crate) fn start(root: Option<PathBuf>, revision: Option<Revision>) -> Self {
        let (events, inbox) = mpsc::channel();
        let (tick, ticks) = mpsc::channel();
        let Some(root) = root else {
            return Self { inbox, _tick: None };
        };
        let source = FolderSource::at(root);
        let spawned = thread::Builder::new()
            .name("z3l-directory".to_owned())
            .spawn(move || poll(source, &ticks, &events, revision));
        match spawned {
            Ok(_) => Self { inbox, _tick: Some(tick) },
            Err(error) => {
                // No watcher, no hot reload: the editor keeps working on the snapshot it has, and
                // the next start reads the folder again.
                log::warn!("the nomenclature directory is not watched: {error}");
                Self { inbox, _tick: None }
            }
        }
    }

    /// The next snapshot waiting at this frame boundary, if the worker has finished one.
    ///
    /// This is the whole cost of hot reload on the frame: one non-blocking receive.
    pub(crate) fn take(&self) -> Option<Reload> {
        self.inbox.try_recv().ok()
    }
}

/// The worker: poll the stamp, read a new snapshot when it moved, hand it over.
fn poll(source: FolderSource, ticks: &Receiver<()>, events: &Sender<Reload>, held: Option<Revision>) {
    let mut held = held;
    // One refusal is reported once: a folder that stays broken must not print a sentence per poll,
    // and it must not be read again either, so its stamp is held like a good one's.
    let mut reported: Option<String> = None;
    loop {
        match ticks.recv_timeout(POLL) {
            Err(RecvTimeoutError::Timeout) => {}
            // The shell is gone; nothing is left to hand a snapshot to.
            Ok(()) | Err(RecvTimeoutError::Disconnected) => return,
        }
        let revision = match source.revision() {
            Ok(revision) => revision,
            Err(error) => {
                // The folder cannot even be stat'ed, so no stamp can be compared: try again next
                // poll and say so once.
                held = None;
                let refusal = error.to_string();
                if reported.as_deref() != Some(refusal.as_str()) {
                    if events.send(Reload::Refused(refusal.clone())).is_err() {
                        return;
                    }
                    reported = Some(refusal);
                }
                continue;
            }
        };
        if held == Some(revision) {
            continue;
        }
        held = Some(revision);
        match load(&source) {
            Ok(directory) => {
                reported = None;
                if events.send(Reload::Loaded(Arc::new(directory))).is_err() {
                    return;
                }
            }
            Err(refusal) => {
                if reported.as_deref() != Some(refusal.as_str()) {
                    if events.send(Reload::Refused(refusal.clone())).is_err() {
                        return;
                    }
                    reported = Some(refusal);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use std::time::{Duration, Instant};

    use model::ItemId;

    use super::*;
    use crate::test_support::write_sample_folder;

    /// The board the fixture folder and the sample package agree on.
    fn board() -> ItemId {
        ItemId::new(1, 1).expect("the sample board's id")
    }

    fn millimetres(value: i64) -> units::Length {
        units::Length::from_millimetres(value).expect("a length")
    }

    /// Waits for the worker to hand something over: a snapshot arrives in a poll or two, and a test
    /// that gave up would report a timeout as a wrong answer.
    fn wait(watch: &Watch) -> Reload {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(reload) = watch.take() {
                return reload;
            }
            assert!(Instant::now() < deadline, "the worker handed nothing over before the deadline");
            thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn a_folder_a_workshop_never_connected_is_the_starter_package() {
        let root = testkit::scratch_dir("watch-absent");
        let start = open(Some(&root.join(DIRECTORY_DIR)));
        assert_eq!(start.refusal, None, "a folder that is not there is not a failure");
        assert_eq!(start.directory, starter(), "a workspace that connected nothing works from the starter rows");
        assert!(start.revision.is_some(), "there is still a stamp to watch");
    }

    #[test]
    fn a_changed_record_reaches_the_shell_as_one_whole_snapshot() {
        let root = testkit::scratch_dir("watch-reload");
        let folder = root.join(DIRECTORY_DIR);
        write_sample_folder(&folder, millimetres(18), "Chipboard");
        let start = open(Some(&folder));
        let name = |directory: &Directory| directory.board(board()).map(|board| board.name.clone());
        assert_eq!(name(&start.directory).as_deref(), Some("Chipboard"), "the folder is what the session holds");

        let watch = Watch::start(Some(folder.clone()), start.revision);
        assert!(watch.take().is_none(), "a folder nobody touched hands nothing over");

        write_sample_folder(&folder, millimetres(25), "Graphite oak");
        match wait(&watch) {
            Reload::Loaded(directory) => {
                let board = directory.board(board()).expect("the changed board is in the snapshot");
                assert_eq!(board.thickness, millimetres(25), "the snapshot carries the new row, not a patch");
                assert_eq!(board.name, "Graphite oak", "and the name a card prints");
            }
            Reload::Refused(refusal) => panic!("a readable folder is not refused: {refusal}"),
        }
    }

    #[test]
    fn a_broken_package_is_refused_once_and_the_rows_in_use_stay() {
        let root = testkit::scratch_dir("watch-broken");
        let folder = root.join(DIRECTORY_DIR);
        write_sample_folder(&folder, millimetres(18), "Chipboard");
        let start = open(Some(&folder));
        let watch = Watch::start(Some(folder.clone()), start.revision);

        let manifest = folder.join("packages").join("sample").join("manifest.json");
        std::fs::write(&manifest, r#"{ "version": 99, "id": 1, "name": "Broken" }"#).expect("break the manifest");
        match wait(&watch) {
            Reload::Loaded(_) => panic!("a package of another schema is refused, not loaded"),
            Reload::Refused(refusal) => {
                assert!(refusal.contains("schema 99"), "the sentence names what the file declares: {refusal}");
                assert!(refusal.contains("manifest.json"), "and the file a person has to open: {refusal}");
            }
        }
        assert!(watch.take().is_none(), "a refusal the shell has seen again is not repeated every poll");
    }
}
