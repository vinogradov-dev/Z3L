//! The system file manager: the one place the shell asks a person for a path.
//!
//! *Open* and *Save as* show the operating system's own dialog instead of a text field, filtered to
//! scene files and started in the folder of the document in front of the person (ADR 0182). The
//! dialog runs on a worker thread, so the frame that asked keeps drawing: a file manager in another
//! process must not freeze ours, and the answer is taken at the next frame boundary exactly like a
//! reloaded directory (`docs/domain/nomenclature.md` §13).
//!
//! Nothing here writes a document. A pick yields a path, and the path becomes the [`FileAction`] the
//! gesture would have been had the person typed it — the same open, the same save, the same
//! remember-the-recent-file rule.
//!
//! [`FileAction`]: crate::file_menu::FileAction

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use project::EXTENSION;
use text::{Lang, Message};

/// The extension a price list is written with: a supplier's export, saved as CSV.
pub(crate) const PRICE_LIST_EXTENSION: &str = "csv";

/// The extension a final render is written with.
pub(crate) const IMAGE_EXTENSION: &str = "png";

/// Which way the file manager is asked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PickMode {
    /// Read a scene from the file the person chooses.
    Open,
    /// Write the scene to the file the person names.
    Save,
    /// Read a price list for the directory application: a supplier's CSV, not a scene.
    Prices,
    /// Write a final render: a PNG, chosen by the render place.
    Image,
    /// Read a decor picture for a material: a PNG the library copies beside its file.
    Picture,
}

/// The order one gesture gives the system file manager.
///
/// It carries everything the dialog needs, so a test asserts the whole request — the folder the
/// picker opens in, the name the save field offers and the words the dialog prints — without a
/// window and without a file manager (X91).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Order {
    /// Read or write.
    pub(crate) mode: PickMode,
    /// The folder the picker starts in: the folder of the open document, when it has one.
    pub(crate) folder: Option<PathBuf>,
    /// The name the save field starts with; `None` when the person picks an existing file.
    pub(crate) name: Option<String>,
    /// The language the dialog's title and its filter are drawn in.
    pub(crate) lang: Lang,
}

impl Order {
    /// The title the dialog shows, in the language the frame draws in.
    pub(crate) fn title(&self) -> &'static str {
        match self.mode {
            PickMode::Open => Message::OpenSceneTitle,
            PickMode::Save => Message::SaveSceneAsTitle,
            PickMode::Prices => Message::PriceListTitle,
            PickMode::Image => Message::SaveImageAsTitle,
            PickMode::Picture => Message::ChoosePictureTitle,
        }
        .text(self.lang)
    }

    /// The name the filter reads by, in the dialog's own filter list.
    pub(crate) fn filter(&self) -> &'static str {
        match self.mode {
            PickMode::Prices => Message::PriceListFiles,
            PickMode::Image | PickMode::Picture => Message::ImageFiles,
            PickMode::Open | PickMode::Save => Message::SceneFiles,
        }
        .text(self.lang)
    }

    /// The one extension the dialog lists, so a picker never offers a scene where a price list goes.
    pub(crate) fn extension(&self) -> &'static str {
        match self.mode {
            PickMode::Prices => PRICE_LIST_EXTENSION,
            PickMode::Image | PickMode::Picture => IMAGE_EXTENSION,
            PickMode::Open | PickMode::Save => EXTENSION,
        }
    }
}

/// What a request to the file manager did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Started {
    /// The dialog is up, and its answer arrives at a later frame boundary.
    Asked,
    /// A pick was already up, so this request asked for nothing.
    Busy,
}

/// The path a save writes to: the chosen one, carrying the scene extension when it names none.
///
/// The dialog filters by [`EXTENSION`] but does not append it — the portal on Linux passes the typed
/// name through — so a scene saved as `cabinet` would be invisible to the next *Open*. The extension
/// is added here, before the file is written and before the tab is renamed.
pub(crate) fn scene_file(path: PathBuf) -> PathBuf {
    match path.extension() {
        Some(_) => path,
        None => path.with_extension(EXTENSION),
    }
}

/// The file manager, and the one pick in flight.
///
/// One pick at a time: a second dialog stacked over the first would answer one gesture twice, and
/// the shell has nothing to do with two paths.
#[derive(Debug, Default)]
pub(crate) struct Picker {
    /// What was asked and where its answer will arrive; `None` between picks.
    pending: Option<InFlight>,
    /// How many picks the shell has asked for: a headless run counts them instead of drawing them.
    #[cfg(test)]
    produced: usize,
}

/// The pick the shell is waiting for.
#[derive(Debug)]
struct InFlight {
    /// What was asked, kept until the answer lands, so the shell knows which gesture to perform.
    order: Order,
    /// Where the worker's word arrives: the chosen path, or `None` for a cancelled pick.
    answer: Receiver<Option<PathBuf>>,
    /// The test build's other end: the worker does not run there, so a test hands the answer over.
    #[cfg(test)]
    sitter: std::sync::mpsc::Sender<Option<PathBuf>>,
}

impl Picker {
    /// A picker with nothing in flight.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Asks the system file manager for a path, unless one is already up.
    ///
    /// The dialog is drawn by another process and the window is woken when it answers, so a frame
    /// never waits for a person to choose a file.
    #[cfg(not(test))]
    pub(crate) fn start(&mut self, ctx: &eframe::egui::Context, order: Order) -> Result<Started, String> {
        if self.pending.is_some() {
            return Ok(Started::Busy);
        }
        let (sitter, answer) = channel();
        let asked = order.clone();
        let ctx = ctx.clone();
        let spawned = std::thread::Builder::new().name("z3l-file-manager".to_owned()).spawn(move || {
            let picked = ask(&asked);
            // A dropped receiver means the window closed under the dialog; nobody is left to tell.
            let _ = sitter.send(picked);
            ctx.request_repaint();
        });
        match spawned {
            Ok(_) => {
                self.pending = Some(InFlight { order, answer });
                Ok(Started::Asked)
            }
            Err(error) => {
                log::warn!("the system file manager could not be started: {error}");
                Err(error.to_string())
            }
        }
    }

    /// Asks for a pick without a worker, so a headless test drives the answer by hand.
    #[cfg(test)]
    pub(crate) fn start(&mut self, _ctx: &eframe::egui::Context, order: Order) -> Result<Started, String> {
        if self.pending.is_some() {
            return Ok(Started::Busy);
        }
        let (sitter, answer) = channel();
        self.produced = self.produced.saturating_add(1);
        self.pending = Some(InFlight { order, answer, sitter });
        Ok(Started::Asked)
    }

    /// Takes the answer once it has landed, with the order that asked for it.
    ///
    /// `Some((order, None))` is a cancelled pick: the person closed the dialog, so nothing is opened
    /// and nothing is written.
    pub(crate) fn take(&mut self) -> Option<(Order, Option<PathBuf>)> {
        let pending = self.pending.as_ref()?;
        match pending.answer.try_recv() {
            Ok(path) => {
                let order = pending.order.clone();
                self.pending = None;
                Some((order, path))
            }
            Err(TryRecvError::Empty) => None,
            // The worker died without an answer: the dialog is gone, so the pick is cancelled
            // rather than left in flight forever.
            Err(TryRecvError::Disconnected) => {
                let order = pending.order.clone();
                self.pending = None;
                Some((order, None))
            }
        }
    }

    /// How many picks the shell has asked for, so a test proves a key started none.
    #[cfg(test)]
    pub(crate) fn produced(&self) -> usize {
        self.produced
    }

    /// Hands the shell the answer the system file manager would have given (X91).
    #[cfg(test)]
    pub(crate) fn answer(&mut self, path: Option<PathBuf>) -> bool {
        let Some(pending) = self.pending.as_ref() else {
            return false;
        };
        pending.sitter.send(path).is_ok()
    }
}

/// Asks the operating system's own dialog, on the worker thread, and returns what it chose.
///
/// A cancelled pick and an unavailable file manager are the same answer — `None` — because the
/// platform binding reports no refusal of its own; the status bar says nothing either way, and the
/// gap is filed in `plan/debt.md`.
#[cfg(not(test))]
fn ask(order: &Order) -> Option<PathBuf> {
    let mut dialog =
        rfd::AsyncFileDialog::new().set_title(order.title()).add_filter(order.filter(), &[order.extension()]);
    if let Some(folder) = &order.folder {
        dialog = dialog.set_directory(folder);
    }
    if let Some(name) = &order.name {
        dialog = dialog.set_file_name(name);
    }
    let picked = match order.mode {
        PickMode::Open | PickMode::Prices | PickMode::Picture => pollster::block_on(dialog.pick_file()),
        PickMode::Save | PickMode::Image => pollster::block_on(dialog.save_file()),
    };
    picked.map(|handle| handle.path().to_owned())
}

/// The folder a document's path is in, for a picker that starts where the document lives.
pub(crate) fn folder_of(path: Option<&Path>) -> Option<PathBuf> {
    path.and_then(Path::parent).filter(|parent| !parent.as_os_str().is_empty()).map(Path::to_owned)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    fn open_order() -> Order {
        Order { mode: PickMode::Open, folder: None, name: None, lang: Lang::En }
    }

    #[test]
    fn a_price_list_is_asked_for_as_a_csv_and_a_scene_as_a_scene() {
        // The dialog's filter is data on the order: a picker that offered a `.z3l` where a supplier's
        // export belongs would hand the directory a scene to read as prices.
        let prices = Order { mode: PickMode::Prices, ..open_order() };
        assert_eq!(prices.filter(), Message::PriceListFiles.text(Lang::En));
        assert_eq!(prices.extension(), PRICE_LIST_EXTENSION);
        assert_eq!(prices.title(), Message::PriceListTitle.text(Lang::En));
        let scene = open_order();
        assert_eq!(scene.extension(), EXTENSION);
        assert_ne!(scene.filter(), prices.filter(), "one dialog name per file kind");
    }

    #[test]
    fn a_cancelled_pick_answers_nothing_and_leaves_the_picker_free() {
        // The one sentence the task turns on: closing the file manager opens nothing and writes
        // nothing, and the next *Open* still asks (X91).
        let ctx = eframe::egui::Context::default();
        let mut picker = Picker::new();
        assert_eq!(picker.start(&ctx, open_order()), Ok(Started::Asked), "the first request is made");
        assert_eq!(picker.take(), None, "and the frame keeps drawing until the answer lands");
        assert!(picker.answer(None), "the test stands in for the worker");
        assert_eq!(picker.take(), Some((open_order(), None)), "a cancel is an answer with no path");
        assert_eq!(picker.take(), None, "an answer is taken once");
        assert_eq!(picker.start(&ctx, open_order()), Ok(Started::Asked), "and the next Open asks again");
    }

    #[test]
    fn a_second_request_waits_for_the_pick_that_is_up() {
        // One gesture, one dialog: a second *Open* while the file manager is up would stack two.
        let ctx = eframe::egui::Context::default();
        let mut picker = Picker::new();
        assert_eq!(picker.start(&ctx, open_order()), Ok(Started::Asked));
        let second = Order { mode: PickMode::Save, folder: None, name: Some("a.z3l".to_owned()), lang: Lang::En };
        assert_eq!(picker.start(&ctx, second), Ok(Started::Busy), "one pick at a time");
        assert_eq!(picker.take(), None, "and the answer still belongs to the first order");
        assert!(picker.answer(Some(PathBuf::from("/tmp/a.z3l"))));
        let (order, path) = picker.take().expect("the first order answers");
        assert_eq!(order.mode, PickMode::Open, "the order that asked is the order that answers");
        assert_eq!(path, Some(PathBuf::from("/tmp/a.z3l")));
    }

    #[test]
    fn the_order_names_the_dialogs_title_and_filter_in_its_own_language() {
        // The words of the dialog are ours, not the platform's: the title and the filter are the
        // same messages the interface draws elsewhere, and both languages answer (ADR 0086).
        assert_eq!(open_order().title(), "Open scene");
        assert_eq!(open_order().filter(), "Scene files");
        let save = Order { mode: PickMode::Save, lang: Lang::Ru, ..open_order() };
        assert_eq!(save.title(), "Сохранить сцену как");
        assert_eq!(save.filter(), "Сцены");
    }

    #[test]
    fn a_save_that_names_no_extension_gets_the_scene_one() {
        // The dialog filters by the extension but does not append it, so `cabinet` would be written
        // as a file the next *Open* cannot see.
        assert_eq!(scene_file(PathBuf::from("/tmp/cabinet")), PathBuf::from("/tmp/cabinet.z3l"));
        assert_eq!(scene_file(PathBuf::from("/tmp/cabinet.z3l")), PathBuf::from("/tmp/cabinet.z3l"));
        assert_eq!(
            scene_file(PathBuf::from("/tmp/cabinet.json")),
            PathBuf::from("/tmp/cabinet.json"),
            "a name the person spelled in full is theirs"
        );
    }

    #[test]
    fn the_picker_starts_where_the_document_lives() {
        assert_eq!(folder_of(Some(Path::new("/tmp/scenes/cabinet.z3l"))), Some(PathBuf::from("/tmp/scenes")));
        assert_eq!(folder_of(Some(Path::new("cabinet.z3l"))), None, "a bare name names no folder to open");
        assert_eq!(folder_of(None), None, "an unsaved scene leaves the folder to the file manager");
    }
}
