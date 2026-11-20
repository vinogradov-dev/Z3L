//! The directory application's screen: one folder of nomenclature items, edited.
//!
//! The editor reads a directory and writes one record at a time (`docs/domain/nomenclature.md` §12,
//! §14, ADR 0158). It is not a second editor of the scene: it owns no document, it has no undo, and
//! its one write is a record plus the package's revision — the editor's undo history is untouched by
//! a directory revision (§13).
//!
//! Three facts shape the screen:
//!
//! - **it opens on a sentence, not on a failure.** A folder that does not read leaves the window open
//!   on what is wrong with it, because the person is holding a supplier's package and has to see the
//!   line the reader complains about.
//! - **the card shows the kind's rows, not the record's.** The fields come from the kind the item is
//!   of, so a package's own vocabulary arrives as fields and a workshop's property is editable the
//!   day its package declares it.
//! - **a save is one write and one sentence.** The card keeps a draft, the writer validates the
//!   candidate directory, and the package's revision moves last, so the editor applies the new rows
//!   at its next frame boundary without the maker restarting anything.
//!
//! What it is not: a 3D viewer (authoring a part is ordinary editor work on an empty document, ADR
//! 0149), a scene editor, or a place that writes a scene file.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use catalog::{
    Currency, Directory, DirectorySource, FolderSource, FolderWriter, KindCode, Money, NomenclatureItem, Price,
    Property, PropertyCode, PropertyType, PropertyValue, Slug,
};
use egui::{Align, Layout, TextEdit, Ui, vec2};
use model::ItemId;
use text::{Lang, Message};
use units::Length;

use crate::scrub::{CommitPolicy, Scrub};

/// The width of the item tree's column.
const TREE_WIDTH: f32 = 260.0;
/// The room the status line keeps at the bottom.
const STATUS_HEIGHT: f32 = 26.0;
/// The narrowest the card may get before the tree gives way.
const MIN_BODY: f32 = 240.0;
/// How many refused rows the status line quotes: a list of forty lines is a report, not a sentence.
const REFUSALS_SHOWN: usize = 3;

/// Which half of the tree column is showing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum View {
    /// The items the package carries.
    #[default]
    Items,
    /// The registries those items are read through.
    Registry,
}

/// One package the directory connects: the folder it lives in and the number its items carry.
#[derive(Clone, Debug)]
struct PackageInfo {
    folder: String,
    id: u32,
    label: String,
}

/// A property's value while the person is typing it.
///
/// A length is held in millimetres because that is what the field shows and what the maker types; the
/// conversion to micrometres happens once, when the draft becomes a record (`AGENTS.md` §4.3).
#[derive(Clone, Debug)]
enum ValueDraft {
    /// A length in millimetres or a count.
    Number(f64),
    /// A price: the amount in minor units, and the record's price it was read from.
    ///
    /// The price keeps its currency and its provenance, so a card that rewrites the amount of an
    /// imported price keeps the currency the import wrote it in; the source turns into `Typed` only
    /// when the amount itself changes (`§11`).
    Money {
        /// The amount in whole minor units of the price's currency.
        minor: f64,
        /// The record's price, or `None` while the package names no currency to write one in.
        price: Option<Price>,
    },
    /// A text.
    Text(String),
    /// One slug of a closed list.
    Choice(String),
    /// A value the card has no field for, kept exactly as the record holds it.
    Kept(PropertyValue),
}

/// The record being edited: everything the card shows, before it is a record again.
struct Draft {
    id: ItemId,
    kind: KindCode,
    name: String,
    slug: String,
    archived: bool,
    /// Whether the slug follows the name: a new record derives one until the person types their own.
    slug_follows_name: bool,
    /// One value per property the kind accepts.
    values: BTreeMap<PropertyCode, ValueDraft>,
    /// Whether the record is not in the folder yet.
    is_new: bool,
}

/// The directory application's state and its one screen.
///
/// The state is view state: which item is open, what the filter says, on which half of the column the
/// person is looking. Nothing here is a document, and nothing here is written to a scene file.
pub struct DirectoryEditor {
    root: PathBuf,
    source: FolderSource,
    packages: Vec<PackageInfo>,
    directory: Option<Directory>,
    /// Why the folder did not read, when it did not.
    refusal: Option<String>,
    /// The item the card shows.
    selection: Option<ItemId>,
    draft: Option<Draft>,
    /// The package a new record goes into, as an index into [`DirectoryEditor::packages`].
    new_package: usize,
    filter: String,
    view: View,
    /// The last thing the application said, and whether it is good news.
    status: Option<String>,
    /// Whether the person asked for a price list this frame; the shell takes it and asks for a file.
    import_requested: bool,
    /// Today, and the age past which a price is called stale, as the shell supplies them.
    ///
    /// The directory has no clock: the day a card judges a price against is the shell's, and the limit
    /// is the workshop's, so this window keeps neither rule of its own.
    price_aging: PriceAging,
}

/// How the card judges the age of a price: the day it is, and the workshop's limit in days.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct PriceAging {
    today: Option<catalog::Date>,
    max_age_days: Option<u32>,
}

impl PriceAging {
    /// How old a price is by this judgement, when both halves are known.
    fn freshness(&self, price: &Price) -> Option<catalog::Freshness> {
        Some(price.freshness(self.today?, self.max_age_days?))
    }
}

impl DirectoryEditor {
    /// Opens the folder at `root`, reading it at once.
    ///
    /// A folder that does not read is not a failure to open the window: the editor keeps the sentence
    /// and shows it, so the person can fix the file the sentence names and press *Reload*.
    pub fn open(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let source = FolderSource::at(&root);
        let mut editor = Self {
            root,
            source,
            packages: Vec::new(),
            directory: None,
            refusal: None,
            selection: None,
            draft: None,
            new_package: 0,
            filter: String::new(),
            view: View::Items,
            status: None,
            import_requested: false,
            price_aging: PriceAging::default(),
        };
        editor.reload();
        editor
    }

    /// The folder this application edits.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The directory as the last read left it, or `None` while it does not read.
    pub fn directory(&self) -> Option<&Directory> {
        self.directory.as_ref()
    }

    /// Why the folder did not read.
    pub fn refusal(&self) -> Option<&str> {
        self.refusal.as_deref()
    }

    /// What the application last said, for the status line and for a test.
    pub fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    /// The record the card is editing.
    pub fn open_item(&self) -> Option<ItemId> {
        self.draft.as_ref().map(|draft| draft.id)
    }

    /// Reads the folder again, keeping the open record when it still resolves.
    ///
    /// This is what the *Reload* button and a supplier's copied folder both do; the editor's own
    /// watcher does the same thing at a frame boundary (`app/src/directory_watch.rs`).
    pub fn reload(&mut self) {
        self.packages.clear();
        self.directory = None;
        self.refusal = None;
        match self.source.packages() {
            Ok(names) => {
                for folder in names {
                    match self.source.manifest(&folder) {
                        Ok(manifest) => {
                            self.packages.push(PackageInfo { folder, id: manifest.id, label: manifest.name })
                        }
                        Err(error) => {
                            self.refusal = Some(error.to_string());
                            return;
                        }
                    }
                }
            }
            Err(error) => {
                self.refusal = Some(error.to_string());
                return;
            }
        }
        match self.source.snapshot() {
            Ok(directory) => self.directory = Some(directory),
            Err(error) => self.refusal = Some(error.to_string()),
        }
        if let Some(id) = self.selection
            && self.directory.as_ref().is_none_or(|directory| directory.item(id).is_none())
        {
            self.selection = None;
            self.draft = None;
        }
    }

    /// Opens one record in the card.
    pub fn select(&mut self, id: ItemId) {
        let Some(directory) = self.directory.as_ref() else { return };
        let Some(item) = directory.item(id) else { return };
        self.selection = Some(id);
        self.draft = Some(Draft::of(directory, item, false));
        self.status = None;
    }

    /// Opens a record the folder does not carry yet: the next free serial of the chosen package.
    ///
    /// The serial is the package's next free number rather than a guess at a name, because the id is
    /// what a scene stores and a rename must not move it (§18.1 the id block is the package's).
    pub fn new_item(&mut self) {
        let Some(directory) = self.directory.as_ref() else { return };
        let Some(package) = self.packages.get(self.new_package).cloned() else { return };
        let serial = directory
            .items()
            .filter(|item| item.id().package() == package.id)
            .map(|item| item.id().serial())
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let Some(id) = ItemId::new(package.id, serial) else { return };
        let Some(kind) = directory.kinds().iter().next().map(|kind| kind.code()) else { return };
        self.selection = None;
        self.draft = Some(Draft::new(id, kind, directory));
        self.status = None;
    }

    /// Writes the open record into its package, and reads the folder again.
    ///
    /// One save is one record and one revision: the writer validates the whole candidate directory, so
    /// a value the reader would refuse is a sentence and an untouched folder, and the reader's stamp
    /// moves once, after the record is in place.
    pub fn save(&mut self, lang: Lang) -> Result<ItemId, String> {
        let draft = self.draft.as_ref().ok_or_else(|| Message::DirectoryPickItem.text(lang))?;
        let directory = self.directory.as_ref().ok_or_else(|| Message::DirectoryEmpty.text(lang))?;
        let item = draft.to_item(directory)?;
        let id = item.id();
        let package = self
            .packages
            .iter()
            .find(|package| package.id == id.package())
            .map(|package| package.folder.clone())
            .ok_or_else(|| format!("no connected package carries the id {}", id.serial()))?;
        let mut writer = FolderWriter::open(&self.root).map_err(|error| error.to_string())?;
        if let Err(refusal) = writer.save_item(&package, &item) {
            let sentence = refusal.to_string();
            self.status = Some(sentence.clone());
            return Err(sentence);
        }
        writer.close();
        self.reload();
        self.select(id);
        if let Some(draft) = self.draft.as_mut() {
            draft.is_new = false;
            draft.slug_follows_name = false;
        }
        self.status = Some(Message::DirectorySaved.text(lang).to_owned());
        Ok(id)
    }

    /// Sets the day a card judges a price against and the workshop's limit, in days.
    ///
    /// `None` in either half means "no verdict": a price is shown with its date and nothing is said
    /// about its age, because a rule the shell did not supply is not the card's to invent.
    pub fn set_price_aging(&mut self, today: Option<catalog::Date>, max_age_days: Option<u32>) {
        self.price_aging = PriceAging { today, max_age_days };
    }

    /// Whether the person pressed *Import prices*, as a request the shell performs.
    ///
    /// The editor creates no dialog and asks no operating system for a path (ADR 0182): the shell owns
    /// the system file manager, hands the chosen file back to [`DirectoryEditor::import_prices`], and
    /// this flag is the one word between them.
    pub fn take_import_request(&mut self) -> bool {
        std::mem::take(&mut self.import_requested)
    }

    /// Reads a supplier's price list and writes every row it applies as one revision per package.
    ///
    /// The batch plans in `catalog` and writes nothing: a row whose currency we cannot print, whose
    /// key names no record or whose price is not a number is refused with its line, and the rows that
    /// do resolve land in one write — one manifest bump per package, so a reader sees one change. The
    /// answer is the sentence the status line shows, and it is the error when nothing was written.
    pub fn import_prices(&mut self, path: &Path, lang: Lang) -> Result<String, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|error| Message::PriceListUnread.text(lang).replace("{}", &error.to_string()))?;
        let directory = self.directory.as_ref().ok_or_else(|| Message::DirectoryEmpty.text(lang))?;
        let batch = catalog::PriceBatch::read(&text, directory);
        let mut refused: Vec<String> =
            batch.refused().take(REFUSALS_SHOWN).map(|(line, reason)| format!("{line}: {reason}")).collect();
        let records = batch.records(directory);
        if records.is_empty() {
            let sentence = batch.sentence();
            let shown = if refused.is_empty() { String::new() } else { format!(" — {}", refused.join("; ")) };
            self.status = Some(format!("{sentence}{shown}"));
            return Err(sentence);
        }
        // One package, one revision: the batch is grouped because a package's manifest is its own
        // history, and a list may price records of several packages.
        let mut by_package: BTreeMap<u32, Vec<NomenclatureItem>> = BTreeMap::new();
        for record in records {
            by_package.entry(record.id().package()).or_default().push(record);
        }
        let mut writer = FolderWriter::open(&self.root).map_err(|error| error.to_string())?;
        for (package, records) in &by_package {
            let folder = self
                .packages
                .iter()
                .find(|info| info.id == *package)
                .map(|info| info.folder.clone())
                .ok_or_else(|| format!("no connected package carries the id prefix {package}"))?;
            if let Err(error) = writer.save_items(&folder, records) {
                let sentence = error.to_string();
                self.status = Some(sentence.clone());
                return Err(sentence);
            }
        }
        writer.close();
        self.reload();
        let mut sentence = batch.sentence();
        if !refused.is_empty() {
            refused.truncate(REFUSALS_SHOWN);
            sentence.push_str(&format!(" — {}", refused.join("; ")));
        }
        self.status = Some(sentence.clone());
        Ok(sentence)
    }

    /// Draws the window's body into `ui`.
    ///
    /// The shell gives one `Ui` and the editor creates no host of its own: a panel, a window or an
    /// area belongs to `app/src/directory_app.rs`, which owns this screen's frame (`cargo xtask
    /// ui-rules`).
    pub fn show(&mut self, ui: &mut Ui) {
        let lang = crate::language(ui.ctx());
        self.toolbar(ui, lang);
        ui.separator();
        let body = (ui.available_height() - STATUS_HEIGHT).max(MIN_BODY);
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(vec2(TREE_WIDTH, body), Layout::top_down(Align::Min), |ui| {
                egui::ScrollArea::vertical().id_salt("directory-tree").show(ui, |ui| match self.view {
                    View::Items => self.tree(ui, lang),
                    View::Registry => self.registry(ui, lang),
                });
            });
            ui.separator();
            ui.allocate_ui_with_layout(vec2(ui.available_width(), body), Layout::top_down(Align::Min), |ui| {
                egui::ScrollArea::vertical().id_salt("directory-card").show(ui, |ui| self.card(ui, lang));
            });
        });
        ui.separator();
        self.status_line(ui, lang);
    }

    /// The folder, the package a new record goes into, and the two actions.
    fn toolbar(&mut self, ui: &mut Ui, lang: Lang) {
        ui.horizontal(|ui| {
            ui.strong(Message::DirectoryTitle.text(lang));
            ui.separator();
            ui.small(Message::DirectoryFolder.text(lang));
            ui.monospace(self.root.to_string_lossy().to_string());
            if ui.button(Message::DirectoryReload.text(lang)).clicked() {
                self.reload();
            }
            ui.separator();
            if !self.packages.is_empty() {
                egui::ComboBox::from_id_salt("directory-package")
                    .selected_text(
                        self.packages.get(self.new_package).map_or_else(String::new, |package| package.label.clone()),
                    )
                    .show_ui(ui, |ui| {
                        for index in 0..self.packages.len() {
                            let label = self.packages.get(index).map_or_else(String::new, |p| p.label.clone());
                            ui.selectable_value(&mut self.new_package, index, label);
                        }
                    });
            }
            if ui.button(Message::DirectoryNewItem.text(lang)).clicked() {
                self.new_item();
            }
            if ui.button(Message::DirectorySave.text(lang)).clicked() {
                // The refusal is already the status line: `save` writes its own sentence.
                let _ = self.save(lang);
            }
            if ui.button(Message::DirectoryImportPrices.text(lang)).clicked() {
                // The shell owns the system file manager, so the press becomes a request it takes.
                self.import_requested = true;
            }
        });
    }

    /// The left column: the item list, or the registries it is read through.
    fn tree(&mut self, ui: &mut Ui, lang: Lang) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.view, View::Items, Message::DirectoryItems.text(lang));
            ui.selectable_value(&mut self.view, View::Registry, Message::DirectoryRegistry.text(lang));
        });
        ui.separator();
        ui.add(TextEdit::singleline(&mut self.filter).hint_text(Message::DirectoryFilter.text(lang)));
        let Some(directory) = self.directory.as_ref() else {
            ui.small(self.refusal.clone().unwrap_or_else(|| Message::DirectoryEmpty.text(lang).to_owned()));
            return;
        };
        let filter = self.filter.to_lowercase();
        let mut opened = None;
        for kind in directory.kinds().iter() {
            let mut items: Vec<(&str, ItemId, bool)> = directory
                .items_of_kind(kind.code())
                .map(|item| (item.name(), item.id(), item.is_archived()))
                .filter(|(name, _, _)| filter.is_empty() || name.to_lowercase().contains(&filter))
                .collect();
            items.sort_by(|left, right| left.0.cmp(right.0));
            if items.is_empty() {
                continue;
            }
            ui.add_space(4.0);
            ui.small(kind.label());
            for (name, id, archived) in items {
                let selected = self.draft.as_ref().is_some_and(|draft| draft.id == id);
                let label = if archived { format!("{name}  ·") } else { name.to_owned() };
                if ui.selectable_label(selected, label).clicked() {
                    opened = Some(id);
                }
            }
        }
        if let Some(id) = opened {
            self.select(id);
        }
    }

    /// The registry view: the kinds and the rows that make them, read-only.
    fn registry(&self, ui: &mut Ui, lang: Lang) {
        let Some(directory) = self.directory.as_ref() else {
            ui.small(Message::DirectoryEmpty.text(lang));
            return;
        };
        for kind in directory.kinds().iter() {
            ui.add_space(4.0);
            ui.strong(kind.label());
            ui.small(kind.slug().as_str());
            for code in kind.accepts() {
                let Some(row) = directory.properties().get(*code) else { continue };
                let required = if kind.required().contains(code) {
                    format!(" *  {}", Message::DirectoryRequired.text(lang))
                } else {
                    String::new()
                };
                ui.monospace(format!("{}  {}{required}", row.label(), type_name(row.ty())));
            }
        }
    }

    /// The card: the open record, its kind's rows, and the one writing action.
    fn card(&mut self, ui: &mut Ui, lang: Lang) {
        let Some(directory) = self.directory.as_ref() else {
            ui.small(self.refusal.clone().unwrap_or_else(|| Message::DirectoryEmpty.text(lang).to_owned()));
            return;
        };
        let Some(mut draft) = self.draft.take() else {
            ui.small(Message::DirectoryPickItem.text(lang));
            if self.status.is_some() {
                ui.separator();
                ui.small(self.status.clone().unwrap_or_default());
            }
            return;
        };

        let kinds: Vec<(KindCode, String)> =
            directory.kinds().iter().map(|kind| (kind.code(), kind.label().to_owned())).collect();
        ui.horizontal(|ui| {
            ui.small(Message::DirectoryIdentifier.text(lang));
            ui.monospace(format!("{}", draft.id));
            if draft.is_new {
                ui.separator();
                ui.small(Message::DirectoryKind.text(lang));
                let selected = kinds
                    .iter()
                    .find(|(code, _)| *code == draft.kind)
                    .map_or_else(String::new, |(_, label)| label.clone());
                egui::ComboBox::from_id_salt("directory-kind").selected_text(selected).show_ui(ui, |ui| {
                    for (code, label) in &kinds {
                        ui.selectable_value(&mut draft.kind, *code, label);
                    }
                });
            } else if let Some(kind) = directory.kinds().get(draft.kind) {
                ui.separator();
                ui.small(Message::DirectoryKind.text(lang));
                ui.label(kind.label());
            }
        });

        ui.horizontal(|ui| {
            ui.small(Message::DirectoryName.text(lang));
            ui.add(TextEdit::singleline(&mut draft.name));
        });
        ui.horizontal(|ui| {
            ui.small(Message::DirectorySlug.text(lang));
            if ui.add(TextEdit::singleline(&mut draft.slug)).changed() {
                draft.slug_follows_name = false;
            }
        });
        ui.checkbox(&mut draft.archived, Message::DirectoryArchived.text(lang));
        ui.separator();

        ui.strong(Message::DirectoryProperties.text(lang));
        if let Some(kind) = directory.kinds().get(draft.kind) {
            let rows: Vec<Property> =
                kind.accepts().iter().filter_map(|code| directory.properties().get(*code).cloned()).collect();
            egui::Grid::new("directory-props").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
                for row in rows {
                    let required = kind.required().contains(&row.code());
                    let label = if required { format!("{} *", row.label()) } else { row.label().to_owned() };
                    ui.label(label);
                    let value = draft.values.get(&row.code()).cloned();
                    // The number box draws its unit as a caption beside itself, which a grid cell
                    // cannot hold: a row of its own gives the field the layout it expects.
                    let mut written = None;
                    let aging = self.price_aging;
                    ui.horizontal(|ui| written = field(ui, row.ty(), value, aging, lang));
                    if let Some(value) = written {
                        draft.values.insert(row.code(), value);
                    }
                    ui.end_row();
                }
            });
        }

        if draft.slug_follows_name && !draft.name.is_empty() {
            draft.slug = slug_of(&draft.name);
        }
        self.draft = Some(draft);
    }

    /// The status line: where the folder is, and what the application last said.
    fn status_line(&self, ui: &mut Ui, lang: Lang) {
        ui.horizontal(|ui| {
            if let Some(package) = self.packages.first() {
                ui.small(format!("{}: {}", Message::DirectoryPackage.text(lang), package.label));
            }
            ui.separator();
            let sentence = self
                .status
                .clone()
                .or_else(|| self.refusal.clone())
                .unwrap_or_else(|| Message::DirectoryNothingSaved.text(lang).to_owned());
            ui.small(sentence);
        });
    }
}

impl Draft {
    /// A record that is not in the folder yet, with one field per property its kind accepts.
    fn new(id: ItemId, kind: KindCode, directory: &Directory) -> Self {
        let mut values = BTreeMap::new();
        if let Some(row) = directory.kinds().get(kind) {
            for code in row.accepts() {
                if let Some(property) = directory.properties().get(*code) {
                    values.insert(*code, ValueDraft::default_for(property.ty(), directory.currency()));
                }
            }
        }
        Self {
            id,
            kind,
            name: String::new(),
            slug: String::new(),
            archived: false,
            slug_follows_name: true,
            values,
            is_new: true,
        }
    }

    /// The edit of a record the folder carries.
    fn of(directory: &Directory, item: &NomenclatureItem, is_new: bool) -> Self {
        let kind = item.kind();
        let mut values = BTreeMap::new();
        if let Some(row) = directory.kinds().get(kind) {
            for code in row.accepts() {
                let Some(property) = directory.properties().get(*code) else { continue };
                let value = match item.prop(*code) {
                    Some(value) => ValueDraft::of(property.ty(), value),
                    None => ValueDraft::default_for(property.ty(), directory.currency()),
                };
                values.insert(*code, value);
            }
        }
        Self {
            id: item.id(),
            kind,
            name: item.name().to_owned(),
            slug: item.slug().as_str().to_owned(),
            archived: item.is_archived(),
            slug_follows_name: false,
            values,
            is_new,
        }
    }

    /// The record this draft writes, or the sentence that says why it cannot be one.
    fn to_item(&self, directory: &Directory) -> Result<NomenclatureItem, String> {
        let mut props = BTreeMap::new();
        for (code, value) in &self.values {
            let Some(row) = directory.properties().get(*code) else { continue };
            props.insert(*code, value.to_value(row.ty())?);
        }
        let slug = Slug::new(&self.slug).map_err(|error| error.to_string())?;
        let mut item = NomenclatureItem::new(self.id, self.kind, self.name.clone(), slug, props)
            .map_err(|error| error.to_string())?;
        if self.archived {
            item = item.archived();
        }
        Ok(item)
    }
}

impl ValueDraft {
    /// The value the card starts a row with when the record does not carry it.
    fn default_for(ty: &PropertyType, currency: Option<&Currency>) -> Self {
        match ty {
            PropertyType::Length { min, .. } => Self::Number(min.millimetres_f64()),
            PropertyType::Count { .. } => Self::Number(0.0),
            PropertyType::Money => {
                Self::Money { minor: 0.0, price: currency.map(|currency| Price::typed(Money::ZERO, currency.clone())) }
            }
            PropertyType::Text { .. } => Self::Text(String::new()),
            PropertyType::Enum(options) => Self::Choice(options.iter().next().cloned().unwrap_or_default()),
            PropertyType::Ref | PropertyType::Date | PropertyType::Color => {
                Self::Kept(PropertyValue::Text(String::new()))
            }
        }
    }

    /// The edit of a value the record already holds.
    fn of(ty: &PropertyType, value: &PropertyValue) -> Self {
        match (ty, value) {
            (PropertyType::Length { .. }, PropertyValue::Length(length)) => Self::Number(length.millimetres_f64()),
            (PropertyType::Count { .. }, PropertyValue::Count(count)) => Self::Number(*count as f64),
            (PropertyType::Money, PropertyValue::Money(price)) => {
                Self::Money { minor: price.value().minor() as f64, price: Some(price.clone()) }
            }
            (PropertyType::Text { .. }, PropertyValue::Text(text)) => Self::Text(text.clone()),
            (PropertyType::Enum(_), PropertyValue::Enum(slug)) => Self::Choice(slug.clone()),
            _ => Self::Kept(value.clone()),
        }
    }

    /// The value this edit writes, or the sentence that says why it is not one.
    fn to_value(&self, ty: &PropertyType) -> Result<PropertyValue, String> {
        match (ty, self) {
            (PropertyType::Length { .. }, Self::Number(millimetres)) => {
                let micrometres = (millimetres * 1_000.0).round();
                if !micrometres.is_finite() || micrometres.abs() > i64::MAX as f64 {
                    return Err("a length that is not a number of millimetres".to_owned());
                }
                Ok(PropertyValue::Length(Length::from_micrometres(micrometres as i64)))
            }
            (PropertyType::Count { .. }, Self::Number(count)) => {
                let count = count.max(0.0).round();
                if !count.is_finite() || count > u64::MAX as f64 {
                    return Err("a count that is not a whole number".to_owned());
                }
                Ok(PropertyValue::Count(count as u64))
            }
            (PropertyType::Money, Self::Money { minor, price }) => {
                let minor = minor.max(0.0).round();
                if !minor.is_finite() || minor > u64::MAX as f64 {
                    return Err("a price that is not a number of minor units".to_owned());
                }
                let amount = Money::from_minor(minor as u64);
                match price {
                    // An untouched row keeps the price it was read from, provenance and all, so a
                    // card that only renames a record never restamps a supplier's number.
                    Some(price) if price.value() == amount => Ok(PropertyValue::Money(price.clone())),
                    Some(price) => Ok(PropertyValue::Money(Price::typed(amount, price.currency().clone()))),
                    None => Err("the package names no currency to write a price in".to_owned()),
                }
            }
            (PropertyType::Text { .. }, Self::Text(text)) => Ok(PropertyValue::Text(text.clone())),
            (PropertyType::Enum(_), Self::Choice(slug)) => Ok(PropertyValue::Enum(slug.clone())),
            (_, Self::Kept(value)) => Ok(value.clone()),
            _ => Err("this row's value is not the type its registry declares".to_owned()),
        }
    }
}

/// One property's field: the one number box for a number, a line of text for a text, a list for a
/// closed set, and a read-only value for the rows this build has no field for yet.
fn field(
    ui: &mut Ui,
    ty: &PropertyType,
    value: Option<ValueDraft>,
    aging: PriceAging,
    lang: Lang,
) -> Option<ValueDraft> {
    let mut written = None;
    match (ty, value) {
        (PropertyType::Length { min, max }, Some(ValueDraft::Number(mut millimetres))) => {
            let range = min.millimetres_f64()..=max.millimetres_f64();
            let outcome =
                Scrub::new(&mut millimetres).decimals(1).unit("mm").range(range).policy(CommitPolicy::Live).show(ui);
            if outcome.commit {
                written = Some(ValueDraft::Number(millimetres));
            }
        }
        (PropertyType::Count { max }, Some(ValueDraft::Number(mut count))) => {
            let outcome =
                Scrub::new(&mut count).decimals(0).range(0.0..=*max as f64).policy(CommitPolicy::Live).show(ui);
            if outcome.commit {
                written = Some(ValueDraft::Number(count));
            }
        }
        (PropertyType::Money, Some(ValueDraft::Money { mut minor, price })) => {
            ui.horizontal(|ui| {
                let outcome = Scrub::new(&mut minor).decimals(0).policy(CommitPolicy::Live).show(ui);
                match &price {
                    Some(price) => {
                        ui.small(format!("{} · {}", price.currency().code(), price.source()));
                        if let Some(date) = price.date() {
                            ui.small(date.iso());
                        }
                        // The age is said in the card's own words and only when the shell supplied
                        // both halves of the rule, so a card never invents a shelf life.
                        match aging.freshness(price) {
                            Some(catalog::Freshness::Stale { days, limit_days }) => {
                                ui.small(Message::PriceStale.with(lang, [days, limit_days]));
                            }
                            Some(catalog::Freshness::Fresh { days }) => {
                                ui.small(Message::PriceFresh.with(lang, [days]));
                            }
                            Some(catalog::Freshness::Undated) => {
                                ui.small(Message::PriceUndated.text(lang));
                            }
                            None => {}
                        }
                    }
                    None => {
                        ui.small(Message::PriceNoCurrency.text(lang));
                    }
                }
                if outcome.commit {
                    written = Some(ValueDraft::Money { minor, price });
                }
            });
        }
        (PropertyType::Text { .. }, Some(ValueDraft::Text(mut text))) => {
            if ui.add(TextEdit::singleline(&mut text)).changed() {
                written = Some(ValueDraft::Text(text));
            }
        }
        (PropertyType::Enum(options), Some(ValueDraft::Choice(mut chosen))) => {
            egui::ComboBox::from_id_salt(format!("directory-enum-{chosen}")).selected_text(chosen.clone()).show_ui(
                ui,
                |ui| {
                    for option in options {
                        ui.selectable_value(&mut chosen, option.clone(), option.clone());
                    }
                },
            );
            written = Some(ValueDraft::Choice(chosen));
        }
        (_, Some(ValueDraft::Kept(value))) => {
            ui.monospace(describe(&value));
        }
        (_, None) => {
            ui.small("—");
        }
        _ => {
            ui.small("—");
        }
    }
    written
}

/// A value in the words a person reads, for the rows the card cannot edit yet.
fn describe(value: &PropertyValue) -> String {
    match value {
        PropertyValue::Length(length) => format!("{} mm", length.millimetres_f64()),
        PropertyValue::Count(count) => format!("{count}"),
        PropertyValue::Money(price) => format!("{price}"),
        PropertyValue::Text(text) => text.clone(),
        PropertyValue::Enum(slug) => slug.clone(),
        PropertyValue::Ref(id) => format!("{id}"),
        PropertyValue::Date(date) => format!("{date:?}"),
        PropertyValue::Color(rgb) => format!("{} {} {}", rgb.red, rgb.green, rgb.blue),
    }
}

/// The type of a registry row, in the words the registry view prints.
fn type_name(ty: &PropertyType) -> String {
    match ty {
        PropertyType::Length { min, max } => {
            format!("length {}…{} mm", min.millimetres_f64(), max.millimetres_f64())
        }
        PropertyType::Count { max } => format!("count to {max}"),
        PropertyType::Money => "price".to_owned(),
        PropertyType::Text { max_chars } => format!("text to {max_chars}"),
        PropertyType::Enum(options) => format!("one of {}", options.iter().cloned().collect::<Vec<_>>().join(", ")),
        PropertyType::Ref => "another item".to_owned(),
        PropertyType::Date => "a date".to_owned(),
        PropertyType::Color => "a colour".to_owned(),
    }
}

/// A slug derived from a name: letters and digits, everything else a dash.
fn slug_of(name: &str) -> String {
    let mut slug = String::new();
    for letter in name.chars() {
        if letter.is_ascii_alphanumeric() {
            slug.push(letter.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.trim_matches('-').to_owned()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use catalog::{KindWire, ManifestWire, PropertyTypeWire, PropertyWire, REGISTRY_VERSION, RegistryWire, Revision};
    use serde_json::json;

    use super::*;

    /// One package in a temporary folder: a sheet kind with a required thickness and a decor text.
    fn folder(name: &str) -> PathBuf {
        let root = testkit::scratch_dir(name);
        let package = root.join("packages").join("starter");
        std::fs::create_dir_all(package.join("items")).expect("the package folder");
        std::fs::write(root.join("directory.json"), json!({ "version": 1, "packages": ["starter"] }).to_string())
            .expect("the connected list");
        let manifest = ManifestWire::new(1, "Starter");
        std::fs::write(package.join("manifest.json"), serde_json::to_string(&manifest).expect("a manifest"))
            .expect("the manifest");
        let registry = RegistryWire {
            version: REGISTRY_VERSION,
            kinds: vec![KindWire {
                code: 1,
                slug: "sheet-material".to_owned(),
                label: "Sheet material".to_owned(),
                accepts: vec![1, 2],
                required: vec![1],
                traits: Vec::new(),
            }],
            properties: vec![
                PropertyWire {
                    code: 1,
                    slug: "thickness".to_owned(),
                    label: "Thickness".to_owned(),
                    ty: PropertyTypeWire::Length { min_um: 1_000, max_um: 40_000 },
                },
                PropertyWire {
                    code: 2,
                    slug: "decor".to_owned(),
                    label: "Decor".to_owned(),
                    ty: PropertyTypeWire::Text { max_chars: 48 },
                },
            ],
            families: Vec::new(),
            systems: Vec::new(),
        };
        std::fs::write(package.join("registry.json"), serde_json::to_string(&registry).expect("a registry"))
            .expect("the registry");
        root
    }

    #[test]
    fn a_new_item_with_properties_is_saved_and_read_back_on_a_new_revision() {
        // O6's Done: the person authors a record with its properties, saves it, and the editor reads
        // it back on a new revision.
        let root = folder("directory-save");
        let starter = FolderSource::at(&root);
        let before = starter.revision().expect("a stamp");
        let mut editor = DirectoryEditor::open(&root);
        assert!(editor.directory().is_some(), "the folder reads: {:?}", editor.refusal());

        editor.new_item();
        let draft = editor.draft.as_mut().expect("a new record");
        draft.name = "Oak shelf".to_owned();
        draft.slug = slug_of(&draft.name);
        draft.values.insert(PropertyCode::new(1).expect("a code"), ValueDraft::Number(18.0));
        draft.values.insert(PropertyCode::new(2).expect("a code"), ValueDraft::Text("oak".to_owned()));
        let id = editor.save(Lang::En).expect("the save");

        let after = FolderSource::at(&root).revision().expect("a stamp");
        assert_ne!(before, after, "the editor applies the new rows because the revision moved");
        let directory = editor.directory().expect("read back");
        let saved = directory.item(id).expect("the record the card wrote");
        assert_eq!(saved.name(), "Oak shelf");
        assert_eq!(saved.slug().as_str(), "oak-shelf");
        assert_eq!(
            saved.prop(PropertyCode::new(1).expect("a code")),
            Some(&PropertyValue::Length(Length::from_micrometres(18_000))),
            "the millimetres the field showed are micrometres in the record"
        );
        assert_eq!(editor.status(), Some(Message::DirectorySaved.text(Lang::En)));
        assert_eq!(editor.open_item(), Some(id), "the card keeps the record it saved");
    }

    /// A package whose sheet kind is priced by the sheet, with one record an `erp` list knows.
    fn priced_folder(name: &str) -> PathBuf {
        let root = testkit::scratch_dir(name);
        let package = root.join("packages").join("starter");
        std::fs::create_dir_all(package.join("items")).expect("the package folder");
        std::fs::write(root.join("directory.json"), json!({ "version": 1, "packages": ["starter"] }).to_string())
            .expect("the connected list");
        let manifest = ManifestWire::new(1, "Starter");
        std::fs::write(package.join("manifest.json"), serde_json::to_string(&manifest).expect("a manifest"))
            .expect("the manifest");
        std::fs::write(
            package.join("registry.json"),
            json!({
                "version": 1,
                "kinds": [{
                    "code": 1, "slug": "sheet-material", "label": "Sheet material",
                    "accepts": [1, 2], "required": [1], "traits": ["sheet", "price-sheet"]
                }],
                "properties": [
                    { "code": 1, "slug": "thickness", "label": "Thickness", "type": "length", "min_um": 1000, "max_um": 40000 },
                    { "code": 2, "slug": "price", "label": "Price", "type": "money" }
                ],
                "systems": [{ "id": "erp", "label": "ERP" }]
            })
            .to_string(),
        )
        .expect("the registry");
        std::fs::write(
            package.join("items").join("sheet-material-1.json"),
            json!({
                "id": "1-1", "kind": "sheet-material", "name": "Oak board", "slug": "oak-board",
                "props": { "thickness": { "type": "length", "value": 18000 } },
                "external": [{ "system": "erp", "key": "A-1" }]
            })
            .to_string(),
        )
        .expect("the record");
        root
    }

    #[test]
    fn the_card_judges_a_price_by_the_day_and_the_limit_the_shell_gives_it() {
        // O8: a card warns about a price older than the workshop's limit, and says nothing at all
        // about the age when the shell supplied no rule.
        let system = catalog::ExternalSystemId::new("erp").expect("a system");
        let currency = catalog::Currency::new("RUB", 100).expect("a currency");
        let price = Price::imported(
            Money::from_minor(320_000),
            currency,
            system,
            catalog::Date::new(2026, 1, 1).expect("a date"),
        );
        let today = catalog::Date::new(2026, 11, 20).expect("a date");
        let aging = PriceAging { today: Some(today), max_age_days: Some(180) };
        assert_eq!(aging.freshness(&price), Some(catalog::Freshness::Stale { days: 323, limit_days: 180 }));
        let patient = PriceAging { max_age_days: Some(365), ..aging };
        assert_eq!(patient.freshness(&price), Some(catalog::Freshness::Fresh { days: 323 }));
        assert_eq!(PriceAging { max_age_days: None, ..aging }.freshness(&price), None, "no limit, no verdict");
        assert_eq!(PriceAging::default().freshness(&price), None, "and no day either");

        let mut editor = DirectoryEditor::open(folder("directory-aging"));
        editor.set_price_aging(Some(today), Some(180));
        assert_eq!(editor.price_aging, aging, "the window keeps the rule it was given and no other");
    }

    #[test]
    fn a_price_list_is_one_write_and_the_status_line_names_the_rows_it_refused() {
        // O8's Done, through the screen: the person picks a supplier's CSV, the batch prices what its
        // keys name, and the revision moves once — not once per row.
        let root = priced_folder("directory-prices");
        let list = root.join("prices.csv");
        std::fs::write(
            &list,
            "system,key,price_minor,currency,date\n\
             erp,A-1,320000,RUB,2026-11-20\n\
             erp,A-9,240000,RUB,2026-11-20\n\
             erp,A-1,1,eur,2026-11-20\n",
        )
        .expect("the price list");
        let before = FolderSource::at(&root).revision().expect("a stamp");
        let mut editor = DirectoryEditor::open(&root);
        assert!(editor.directory().is_some(), "the folder reads: {:?}", editor.refusal());
        let sentence = editor.import_prices(&list, Lang::En).expect("one row is a price");
        assert!(sentence.contains("1 prices written, 2 rows refused"), "{sentence}");
        assert_eq!(editor.status(), Some(sentence.as_str()), "the status line is the batch's own sentence");
        assert!(
            sentence.contains("A-9") || sentence.contains("ISO 4217"),
            "the refused rows are quoted by line: {sentence}"
        );

        let after = FolderSource::at(&root).revision().expect("a stamp");
        assert_ne!(before, after, "the batch moved the revision");
        let directory = editor.directory().expect("read back");
        let id = ItemId::new(1, 1).expect("an id");
        let price = directory.price(directory.item(id).expect("the record")).expect("the price the batch wrote");
        assert_eq!(price.value().minor(), 320_000);
        assert_eq!(price.currency().code(), "RUB");
        assert_eq!(price.price.source().system().map(|system| system.as_str()), Some("erp"));
    }

    #[test]
    fn a_price_list_that_writes_nothing_is_a_sentence_and_leaves_the_folder_alone() {
        let root = priced_folder("directory-prices-refused");
        let list = root.join("prices.csv");
        std::fs::write(&list, "system,key,price_minor,currency,date\nerp,A-9,240000,RUB,2026-11-20\n")
            .expect("the price list");
        let before = FolderSource::at(&root).revision().expect("a stamp");
        let mut editor = DirectoryEditor::open(&root);
        let refusal = editor.import_prices(&list, Lang::En).expect_err("no record is named A-9");
        assert!(refusal.contains("no price was written"), "{refusal}");
        assert_eq!(FolderSource::at(&root).revision().expect("a stamp"), before, "nothing was written");
        assert!(editor.status().is_some_and(|status| status.contains("A-9")), "{:?} is the status", editor.status());
    }

    #[test]
    fn a_value_outside_its_range_is_a_sentence_and_the_folder_does_not_move() {
        let root = folder("directory-refused");
        let before: Revision = FolderSource::at(&root).revision().expect("a stamp");
        let mut editor = DirectoryEditor::open(&root);
        editor.new_item();
        let draft = editor.draft.as_mut().expect("a new record");
        draft.name = "Too thick".to_owned();
        draft.slug = "too-thick".to_owned();
        draft.values.insert(PropertyCode::new(1).expect("a code"), ValueDraft::Number(250.0));
        let refusal = editor.save(Lang::En).expect_err("a 250 mm sheet is not this package's row");
        assert!(refusal.contains("250"), "the sentence names the number: {refusal}");
        assert_eq!(FolderSource::at(&root).revision().expect("a stamp"), before, "nothing was written");
    }

    #[test]
    fn the_card_draws_the_record_it_opened() {
        // Headless: the surface is driven through a raw input pass and the result is read back from
        // the frame's own shapes, so the screen is checked without taking the person's display.
        let root = folder("directory-draws");
        let mut editor = DirectoryEditor::open(&root);
        editor.new_item();
        let draft = editor.draft.as_mut().expect("a new record");
        draft.name = "White melamine".to_owned();
        draft.slug = "white-melamine".to_owned();
        draft.values.insert(PropertyCode::new(1).expect("a code"), ValueDraft::Number(18.0));
        draft.values.insert(PropertyCode::new(2).expect("a code"), ValueDraft::Text("white".to_owned()));
        let id = editor.save(Lang::En).expect("the save");
        editor.select(id);

        let context = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 600.0))),
            ..Default::default()
        };
        let output = context.run_ui(input, |ui| editor.show(ui));
        let drawn = texts(&output.shapes);
        output.drop_without_applying_deltas();
        assert!(drawn.iter().any(|text| text.contains("White melamine")), "the card names the record: {drawn:?}");
        assert!(drawn.iter().any(|text| text.starts_with("18")), "and shows its thickness: {drawn:?}");
    }

    /// Every text one frame painted, so a headless test can read the screen.
    fn texts(shapes: &[egui::epaint::ClippedShape]) -> Vec<String> {
        let mut found = Vec::new();
        for shape in shapes {
            walk(&shape.shape, &mut found);
        }
        found
    }

    fn walk(shape: &egui::epaint::Shape, out: &mut Vec<String>) {
        match shape {
            egui::epaint::Shape::Text(text) => out.push(text.galley.text().to_owned()),
            egui::epaint::Shape::Vec(shapes) => {
                for shape in shapes {
                    walk(shape, out);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn a_slug_follows_a_name_until_the_person_types_their_own() {
        assert_eq!(slug_of("White melamine 18 mm"), "white-melamine-18-mm");
        assert_eq!(slug_of("  Oak / veneer  "), "oak-veneer");
    }
}
