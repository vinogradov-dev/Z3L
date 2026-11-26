//! The workshop's material library, read again when a person changes it by hand.
//!
//! The library has no editor yet: it is one file and a folder of pictures a shop edits with a text
//! editor and a paint program (`crate::materials`, ADR 0243). The editor therefore depends on a
//! revision it does not own, and it watches it exactly the way it watches the nomenclature
//! directory: **off the frame path**, on a worker that polls the stamp, reads and validates a new
//! library when the stamp moved, and hands it over. A replaced decor picture moves the stamp too, so
//! a scan a shop saved over reaches the frame without restarting the program.
//!
//! A folder is the whole workspace's, so the shell applies one snapshot to every open tab. What each
//! tab re-derives afterwards is the frame alone: a material is not the document, so no panel, its
//! thickness, its geometry or the undo history depends on it.

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::Duration;

use materials::{Library, Revision, Store};

/// How often the worker reads the folder's stamp.
///
/// One stat of the library file and of each picture every few hundred milliseconds is nothing, and a
/// material is edited by a person rather than by a loop. What the interval must not become is a
/// per-frame stat: the frame reads a channel, which costs the same whether the folder is there or
/// not.
#[cfg(not(test))]
const POLL: Duration = Duration::from_millis(400);

/// The interval a test build polls at, so a changed fixture does not cost the suite a wait.
#[cfg(test)]
const POLL: Duration = Duration::from_millis(2);

/// The library a workspace starts from, and what the watcher must know to take over from it.
pub(crate) struct Start {
    /// The records a new session opens with: the workshop's, or none at all.
    pub(crate) library: Library,
    /// The stamp the folder was read at; a worker that starts here finds nothing new.
    pub(crate) revision: Option<Revision>,
    /// Why the workshop's library was not read, when it was not. The caller shows the sentence and
    /// works from the records it has, because a library that broke is a fact about the workshop and
    /// not a reason to draw somebody else's boards.
    pub(crate) refusal: Option<String>,
}

/// Opens the workshop's library, or the empty one when the workspace has no folder.
///
/// `None` is a workspace with no config directory at all: there is nothing to watch and no material
/// is named, so every frame keeps the shell's placeholder tones.
pub(crate) fn open(store: Option<&Store>) -> Start {
    let Some(store) = store else {
        return Start { library: Library::default(), revision: None, refusal: None };
    };
    // The stamp is read before the snapshot, so a write that lands between the two reads moves the
    // stamp the watcher holds and is caught by its next poll instead of being lost.
    let revision = store.revision().ok();
    match store.read() {
        Ok(library) => Start { library, revision, refusal: None },
        Err(refusal) => Start { library: Library::default(), revision, refusal: Some(refusal.to_string()) },
    }
}

/// A newer library, or the sentence that refused one, waiting for a frame boundary.
pub(crate) enum Reload {
    /// A whole snapshot, validated off the frame path and ready to swap in.
    Loaded(Arc<Library>),
    /// A revision this build cannot read; the records in use stay in place.
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
    /// Watches `store` from `revision`, handing every later library to the shell.
    ///
    /// `revision` is the stamp of the same folder read a moment earlier: a worker that started
    /// anywhere else would reload the records it was handed, and a worker with no folder does
    /// nothing at all.
    pub(crate) fn start(store: Option<Store>, revision: Option<Revision>) -> Self {
        let (events, inbox) = mpsc::channel();
        let (tick, ticks) = mpsc::channel();
        let Some(store) = store else {
            return Self { inbox, _tick: None };
        };
        let spawned = thread::Builder::new()
            .name("z3l-materials".to_owned())
            .spawn(move || poll(store, &ticks, &events, revision));
        match spawned {
            Ok(_) => Self { inbox, _tick: Some(tick) },
            Err(error) => {
                // No watcher, no hot reload: the editor keeps working on the snapshot it has, and
                // the next start reads the folder again.
                log::warn!("the material library is not watched: {error}");
                Self { inbox, _tick: None }
            }
        }
    }

    /// The next library waiting at this frame boundary, if the worker has finished one.
    ///
    /// This is the whole cost of hot reload on the frame: one non-blocking receive.
    pub(crate) fn take(&self) -> Option<Reload> {
        self.inbox.try_recv().ok()
    }
}

/// The worker: poll the stamp, read a new library when it moved, hand it over.
fn poll(store: Store, ticks: &Receiver<()>, events: &Sender<Reload>, held: Option<Revision>) {
    let mut held = held;
    // One refusal is reported once: a folder that stays broken must not print a sentence per poll,
    // and it must not be read again either, so its stamp is held like a good one's.
    let mut reported: Option<String> = None;
    loop {
        match ticks.recv_timeout(POLL) {
            Err(RecvTimeoutError::Timeout) => {}
            // The shell is gone; nothing is left to hand a library to.
            Ok(()) | Err(RecvTimeoutError::Disconnected) => return,
        }
        let revision = match store.revision() {
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
        match store.read() {
            Ok(library) => {
                reported = None;
                if events.send(Reload::Loaded(Arc::new(library))).is_err() {
                    return;
                }
            }
            Err(error) => {
                let refusal = error.to_string();
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

    use std::fs::File;
    use std::path::Path;
    use std::time::{Duration, Instant};

    use materials::{Base, Decor, Material, MaterialId, Turns};
    use units::Length;

    use super::*;

    fn millimetres(value: i64) -> Length {
        Length::from_millimetres(value).expect("a length")
    }

    /// Writes one opaque texel as a PNG at `path`, creating its folder.
    fn write_png(path: &Path, texel: [u8; 4]) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("the textures folder");
        }
        let file = File::create(path).expect("a fresh file");
        let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), 1, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("a PNG header");
        writer.write_image_data(&texel).expect("the texel");
    }

    /// A store in a folder this test owns, emptied before use.
    fn scratch(name: &str) -> Store {
        Store::at(testkit::scratch_dir(&format!("materials-watch-{name}")))
    }

    /// Writes a library of one oak board faced with `texel` and returns its record.
    fn write_board(store: &Store, texel: [u8; 4]) -> Material {
        write_png(&store.textures().join("oak.png"), texel);
        let decor = Decor::new("textures/oak.png", millimetres(600), Turns::ALONG).expect("a decor");
        let base = Base::new([0.6, 0.45, 0.25], Some(decor), 1.0, 0.0, 1.5, 0.0, [0.0; 3]).expect("a board");
        let board = Material::of_base(MaterialId::new(1).expect("id"), "Oak", base).expect("a record");
        store.write(&Library::new([board.clone()]).expect("one record")).expect("the write");
        board
    }

    /// Waits for the worker to hand something over: a library arrives in a poll or two, and a test
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
    fn a_workspace_with_no_folder_names_no_material() {
        let start = open(None);
        assert!(start.library.is_empty());
        assert_eq!(start.revision, None);
        assert_eq!(start.refusal, None);
        assert!(Watch::start(None, None).take().is_none(), "nothing to watch is nothing to hand over");
    }

    #[test]
    fn a_folder_that_names_no_material_reads_as_an_empty_library() {
        let store = scratch("absent");
        let start = open(Some(&store));
        assert!(start.library.is_empty(), "a first run is not an error");
        assert_eq!(start.refusal, None);
        assert!(start.revision.is_some(), "there is still a stamp to watch");
    }

    #[test]
    fn a_replaced_picture_reaches_the_shell_without_a_restart() {
        let store = scratch("replaced");
        let board = write_board(&store, [255, 0, 0, 255]);
        let start = open(Some(&store));
        let held = start.library.get(board.id()).expect("the record");
        let texel = |library: &Library| {
            let decor = library.get(board.id()).and_then(Material::base).and_then(Base::decor).expect("a decor");
            let picture =
                store.picture(library.get(board.id()).expect("the record")).expect("a picture").expect("a decor");
            (decor.picture().to_owned(), picture.texel(0, 0))
        };
        assert_eq!(texel(&start.library).1, Some([255, 0, 0, 255]), "the picture the shop saved first");

        let watch = Watch::start(Some(store.clone()), start.revision);
        assert!(watch.take().is_none(), "a folder nobody touched hands nothing over");

        // A person saves another scan over the file; the library's own bytes are untouched.
        std::thread::sleep(Duration::from_millis(2));
        write_png(&store.textures().join("oak.png"), [0, 0, 255, 255]);
        match wait(&watch) {
            Reload::Loaded(library) => {
                assert_eq!(texel(&library).1, Some([0, 0, 255, 255]), "the new scan is what the frame is built from");
                assert_eq!(
                    library.get(board.id()).map(Material::name),
                    Some(held.name()),
                    "the record itself is the one the file still names"
                );
            }
            Reload::Refused(refusal) => panic!("a replaced picture is not a broken library: {refusal}"),
        }
    }

    #[test]
    fn a_broken_library_is_refused_once_and_the_records_in_use_stay() {
        let store = scratch("broken");
        write_board(&store, [10, 20, 30, 255]);
        let start = open(Some(&store));
        assert_eq!(start.library.len(), 1);
        let watch = Watch::start(Some(store.clone()), start.revision);

        std::fs::write(store.library_path(), r#"{ "version": 99 }"#).expect("break the file");
        match wait(&watch) {
            Reload::Loaded(_) => panic!("a library of another schema is refused, not loaded"),
            Reload::Refused(refusal) => {
                assert!(refusal.contains("99"), "the sentence names what the file declares: {refusal}");
                assert!(refusal.contains("library.json"), "and the file a person has to open: {refusal}");
            }
        }
        assert!(watch.take().is_none(), "a refusal the shell has seen again is not repeated every poll");
    }
}
