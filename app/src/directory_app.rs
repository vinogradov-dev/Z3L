//! The directory application: the window that edits a nomenclature folder.
//!
//! It is a second screen on the same stack, not a second editor (`docs/domain/nomenclature.md` §12,
//! ADR 0158): the editor reads the directory and never writes it, and this window does the opposite —
//! it owns a folder, one writer at a time, and no document at all. There is no scene, no selection of
//! nodes and no undo; a save is one record and one revision, and the editor applies it at its next
//! frame boundary (§13).
//!
//! Why a mode of the one binary rather than a second `[[bin]]`: the shell has no library — `app` is a
//! single binary root, so a second root could not read `settings.json` and would write the rule for
//! "where `nomenclature/` lives" a second time. `z3l directory` opens the same window with the same
//! rules, and the day `app` splits into a library and thin binaries (`plan/debt.md`), this window
//! moves without a rewrite.

use std::path::PathBuf;

use eframe::egui;
use ui::DirectoryEditor;
use ui::theme;

use crate::file_picker::{Order, PickMode, Picker, Started, folder_of};

/// How old a material's price may be before the card warns, in days.
///
/// A workshop's own number, so it belongs to the standards package O2 will bring; until that exists
/// the window carries the default every shop recognises, and `plan/debt.md` holds the gap.
const PRICE_MAX_AGE_DAYS: u32 = 365;

/// The day this window judges prices against, read once when it opens.
///
/// The directory has no clock (`docs/domain/nomenclature.md` §11): the shell reads today, the card
/// only compares it with the dates a price carries.
fn today() -> Option<catalog::Date> {
    let stamp = export::now_timestamp().ok()?;
    export::exported_on(&stamp)
}

/// The folder the window edits: the one the editor watches, or the one the command line named.
pub fn default_root() -> PathBuf {
    crate::settings_store::config_dir()
        .map(|folder| folder.join(crate::directory_watch::DIRECTORY_DIR))
        .unwrap_or_default()
}

/// Opens the directory window on `root`, or on the folder the editor watches.
///
/// The window is the application: it stays open on a refused folder and says which file is wrong,
/// because the person holding a supplier's package has to read the line the reader complains about.
pub fn run(root: Option<PathBuf>) -> eframe::Result {
    let folder = root.unwrap_or_else(default_root);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(format!("{} — {}", theme::PRODUCT_NAME, ui::Message::DirectoryTitle.text(ui::Lang::En)))
            .with_app_id(crate::APP_ID)
            .with_inner_size([1100.0, 720.0])
            .with_min_inner_size([720.0, 480.0]),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        theme::PRODUCT_NAME,
        options,
        Box::new(move |cc| {
            theme::install(&cc.egui_ctx);
            Ok(Box::new(DirectoryWindow::new(folder)))
        }),
    )
}

/// The window's state: the editor and nothing else.
///
/// The one key is Save, and it is the card's own action rather than a second way in: the card draws
/// the button, the key presses it.
struct DirectoryWindow {
    editor: DirectoryEditor,
    /// The system file manager, asked for one price list at a time (ADR 0182).
    picker: Picker,
}

impl DirectoryWindow {
    fn new(folder: PathBuf) -> Self {
        let mut editor = DirectoryEditor::open(folder);
        editor.set_price_aging(today(), Some(PRICE_MAX_AGE_DAYS));
        Self { editor, picker: Picker::new() }
    }

    /// Turns the card's *Import prices* press into a request to the system file manager.
    ///
    /// The dialog opens in the folder the directory lives in and lists CSV files: a price list is a
    /// supplier's export, never a scene, and the editor itself creates no window (ADR 0158).
    fn ask_for_prices(&mut self, ctx: &egui::Context) {
        let lang = ui::language(ctx);
        let order = Order { mode: PickMode::Prices, folder: folder_of(Some(self.editor.root())), name: None, lang };
        match self.picker.start(ctx, order) {
            Ok(Started::Asked | Started::Busy) => {}
            Err(reason) => log::warn!("the system file manager could not be opened: {reason}"),
        }
    }

    /// Takes the price list the person chose, once its answer has landed.
    ///
    /// A cancelled pick answers nothing and says nothing: closing the dialog is not a failure, and the
    /// next *Import prices* still asks.
    fn take_prices(&mut self, ctx: &egui::Context) {
        let Some((order, answer)) = self.picker.take() else {
            return;
        };
        if order.mode != PickMode::Prices {
            return;
        }
        let Some(path) = answer else {
            return;
        };
        let lang = ui::language(ctx);
        // The refusal is the status line: `import_prices` writes its own sentence there.
        let _ = self.editor.import_prices(&path, lang);
    }
}

impl eframe::App for DirectoryWindow {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.take_prices(ui.ctx());
        if self.editor.take_import_request() {
            self.ask_for_prices(ui.ctx());
        }
        if ui.input(|input| input.key_pressed(egui::Key::S) && input.modifiers.command) {
            let lang = ui::language(ui.ctx());
            // The refusal is the status line: `save` writes its own sentence there.
            let _ = self.editor.save(lang);
        }
        self.editor.show(ui);
    }
}
