//! The File menu and the destructive question a close or a delete asks.
//!
//! The menu is a view over one piece of the document's shell state — the path the scene came from —
//! plus the pick it asks the shell for. It never touches the document or the filesystem itself; it
//! returns a [`FileAction`] and the application performs it through the scene store. A path is never
//! typed: *Open* and *Save as* return the [`Order`] the shell hands the system's own file manager
//! (ADR 0182), and the header, the Home surface and `Ctrl+S` all reach it through the same command.
//!
//! Opening and *New* both mean "a new tab" (U45), so the menu never discards a scene and never has
//! to ask: the questions left are a tab close with unsaved changes, a quit with them, and a Delete
//! over more than one node, which [`Confirm`] draws for the shell because all three belong to the
//! window and not to one document (ADR 0112).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use eframe::egui;
use eframe::egui::{Id, Modal, Ui};
use project::EXTENSION;
use text::{Lang, Message};

use crate::file_picker::{Order, PickMode, folder_of};

/// One file operation the user asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FileAction {
    /// Open a new, empty scene in a new tab.
    New,
    /// Read the scene at this path.
    Open(PathBuf),
    /// Write the current scene to this path.
    Save(PathBuf),
    /// Ask the system file manager for a path (ADR 0182).
    Pick(Order),
    /// Close the active tab, asking first when its scene has unsaved changes.
    Close,
    /// Quit the application, asking once when any open scene has unsaved changes.
    Quit,
}

/// What a File menu entry means before it becomes an action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MenuCommand {
    New,
    Open,
    Save,
    SaveAs,
    Close,
    Quit,
}

/// What a destructive question was answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Answer {
    /// Perform the gesture the question names.
    Proceed,
    /// Leave everything as it is.
    Cancel,
}

/// A destructive gesture waiting for the user to confirm it.
///
/// The shell owns one of these while the question is up: it names a tab, the window or a selection,
/// so it outlives the document it names, and switching tabs must not lose it. The three gestures
/// that lose work ask through it — closing a tab, quitting the window and deleting more than one
/// node (ADR 0112) — so the window draws one question and not three.
#[derive(Clone, Debug)]
pub(crate) struct Confirm {
    /// The question, already built from what it is about.
    title: String,
    /// One sentence naming what is lost.
    body: String,
    /// The label of the button that performs the gesture.
    proceed: String,
}

impl Confirm {
    /// Asks about the scene `title` names, the way the tab row shows it.
    pub(crate) fn tab(title: impl Into<String>, lang: Lang) -> Self {
        Self {
            title: Message::CloseTitle.with(lang, [title.into()]),
            body: Message::CloseBody.text(lang).to_owned(),
            proceed: Message::DiscardAndClose.text(lang).to_owned(),
        }
    }

    /// Asks about the whole window, when `unsaved` open scenes would go with it.
    pub(crate) fn quit(unsaved: usize, lang: Lang) -> Self {
        let body = if unsaved == 1 {
            Message::QuitOneBody.text(lang).to_owned()
        } else {
            Message::counted(unsaved, Message::QuitManyOne, Message::QuitManyFew, Message::QuitManyMany).text(lang)
        };
        Self {
            title: Message::QuitTitle.with(lang, [crate::theme::PRODUCT_NAME]),
            body,
            proceed: Message::DiscardAndQuit.text(lang).to_owned(),
        }
    }

    /// Asks before removing `count` selected nodes in one step.
    ///
    /// The count is the question's subject, so the answer cannot be mistaken for a single row: the
    /// body says that undo is one step too (ADR 0112).
    pub(crate) fn delete(count: usize, lang: Lang) -> Self {
        Self {
            title: Message::counted(
                count,
                Message::DeleteNodesTitleOne,
                Message::DeleteNodesTitleFew,
                Message::DeleteNodesTitleMany,
            )
            .text(lang),
            body: Message::DeleteNodesBody.text(lang).to_owned(),
            proceed: Message::CmdDelete.text(lang).to_owned(),
        }
    }

    /// Draws the question and returns the answer the user picked, if any.
    ///
    /// Escape is the second *Cancel*: a question that owns the keys for the frame it is up has to
    /// answer the way out, or a person who pressed a destructive key feels trapped (ADR 0112).
    pub(crate) fn show(&self, ctx: &egui::Context) -> Option<Answer> {
        let lang = ui::language(ctx);
        let mut answer = None;
        Modal::new(Id::unique("scene-confirm")).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.heading(&self.title);
            ui.label(&self.body);
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button(&self.proceed).clicked() {
                    answer = Some(Answer::Proceed);
                }
                if ui.button(Message::Cancel.text(lang)).clicked() {
                    answer = Some(Answer::Cancel);
                }
            });
        });
        if answer.is_none() && ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            answer = Some(Answer::Cancel);
        }
        answer
    }
}

/// The File menu and the file the scene came from.
#[derive(Debug, Default)]
pub(crate) struct FileMenu {
    current: Option<PathBuf>,
}

impl FileMenu {
    /// Creates a menu with no current file.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Records the file the scene was last opened from or saved to.
    pub(crate) fn set_path(&mut self, path: Option<PathBuf>) {
        self.current = path;
    }

    /// Draws the File menu button on an existing row.
    ///
    /// The button is a plain menu: the header decides where it sits and draws its own dividers, so
    /// this function creates no panel of its own. *New* and *Open* open a tab, so neither of them
    /// discards the scene in front of the user and neither has to ask.
    pub(crate) fn show(&mut self, ui: &mut Ui) -> Option<FileAction> {
        let lang = ui::language(ui.ctx());
        let mut action = None;
        let button = egui::Button::new(Message::File.text(lang))
            .min_size(egui::vec2(crate::theme::HEADER_TEXT_WIDTH, crate::theme::HEADER_CONTROL_HEIGHT));
        egui::containers::menu::MenuButton::from_button(button).ui(ui, |ui| {
            if ui.button(Message::MenuNew.text(lang)).clicked() {
                ui.close();
                action = self.command(MenuCommand::New, lang);
            }
            if ui.button(Message::MenuOpen.text(lang)).clicked() {
                ui.close();
                action = self.command(MenuCommand::Open, lang);
            }
            ui.separator();
            if ui.button(Message::MenuSave.text(lang)).clicked() {
                ui.close();
                action = self.command(MenuCommand::Save, lang);
            }
            if ui.button(Message::MenuSaveAs.text(lang)).clicked() {
                ui.close();
                action = self.command(MenuCommand::SaveAs, lang);
            }
            ui.separator();
            if ui.button(Message::MenuCloseTab.text(lang)).clicked() {
                ui.close();
                action = self.command(MenuCommand::Close, lang);
            }
            if ui.button(Message::MenuExit.text(lang)).clicked() {
                ui.close();
                action = self.command(MenuCommand::Quit, lang);
            }
        });
        action
    }

    /// The Save path of the header's save button.
    ///
    /// The header calls the same [`MenuCommand::Save`] the File menu does, so the two cannot drift
    /// into one saving silently and the other opening a picker.
    pub(crate) fn save(&mut self, lang: Lang) -> Option<FileAction> {
        self.command(MenuCommand::Save, lang)
    }

    /// Asks for the pick the Home surface's *Open* entry wants.
    ///
    /// Home has no menu of its own, so it asks for the same order the File menu builds.
    pub(crate) fn request_open(&mut self, lang: Lang) -> Option<FileAction> {
        self.command(MenuCommand::Open, lang)
    }

    /// The file the scene came from, if it has one.
    pub(crate) fn path(&self) -> Option<&Path> {
        self.current.as_deref()
    }

    /// The bare file name the header's tab draws; the dirty marker is its own glyph there.
    pub(crate) fn file_name(&self, lang: Lang) -> String {
        self.current
            .as_deref()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            .unwrap_or(Message::Untitled.text(lang))
            .to_owned()
    }

    /// Turns a menu entry into the action the shell performs.
    fn command(&mut self, command: MenuCommand, lang: Lang) -> Option<FileAction> {
        match command {
            MenuCommand::New => Some(FileAction::New),
            MenuCommand::Open => Some(FileAction::Pick(self.order(PickMode::Open, lang))),
            // Saving over the file the scene came from is not a pick: the person already chose it.
            MenuCommand::Save => match &self.current {
                Some(path) => Some(FileAction::Save(path.clone())),
                None => Some(FileAction::Pick(self.order(PickMode::Save, lang))),
            },
            MenuCommand::SaveAs => Some(FileAction::Pick(self.order(PickMode::Save, lang))),
            // Both are window operations, not file operations: the shell closes the tab or the
            // window, and the unsaved-changes question belongs there because it names a tab.
            MenuCommand::Close => Some(FileAction::Close),
            MenuCommand::Quit => Some(FileAction::Quit),
        }
    }

    /// The order a pick gives the file manager: where it opens and what name it offers.
    ///
    /// A pick starts where the document lives, and a save offers the file's own name, so saving a
    /// scene as another keeps its spelling; a scene that was never saved offers `scene.z3l`.
    fn order(&self, mode: PickMode, lang: Lang) -> Order {
        let current = self.current.as_deref();
        let name = current.and_then(Path::file_name).map(|name| name.to_string_lossy().into_owned());
        Order {
            mode,
            folder: folder_of(current),
            name: match mode {
                PickMode::Open => None,
                PickMode::Save => Some(name.unwrap_or_else(|| format!("scene.{EXTENSION}"))),
            },
            lang,
        }
    }
}

/// Top-level sections of a loaded scene that this build does not understand.
pub(crate) type PreservedSections = BTreeMap<String, serde_json::Value>;

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    #[test]
    fn save_without_a_path_asks_the_file_manager_for_a_name() {
        // The scene was never saved, so the picker offers `scene.z3l` and there is no folder to
        // start in: the file manager's own last folder stands in (ADR 0182).
        let mut menu = FileMenu::new();
        let expected = Order { mode: PickMode::Save, folder: None, name: Some("scene.z3l".to_owned()), lang: Lang::En };
        assert_eq!(menu.command(MenuCommand::Save, Lang::En), Some(FileAction::Pick(expected)));
    }

    #[test]
    fn save_with_a_path_returns_it() {
        // Saving over the file the scene came from is no pick at all: the person already chose it.
        let mut menu = FileMenu::new();
        let path = PathBuf::from("/tmp/scene.z3l");
        menu.set_path(Some(path.clone()));
        assert_eq!(menu.command(MenuCommand::Save, Lang::En), Some(FileAction::Save(path)));
    }

    #[test]
    fn open_and_save_as_start_where_the_document_lives() {
        let mut menu = FileMenu::new();
        menu.set_path(Some(PathBuf::from("/tmp/scenes/cabinet.z3l")));
        let opened = menu.command(MenuCommand::Open, Lang::En).expect("the open order");
        assert_eq!(
            opened,
            FileAction::Pick(Order {
                mode: PickMode::Open,
                folder: Some(PathBuf::from("/tmp/scenes")),
                name: None,
                lang: Lang::En,
            }),
            "an open pick offers no name: the person picks an existing file"
        );
        let saved = menu.command(MenuCommand::SaveAs, Lang::En).expect("the save order");
        assert_eq!(
            saved,
            FileAction::Pick(Order {
                mode: PickMode::Save,
                folder: Some(PathBuf::from("/tmp/scenes")),
                name: Some("cabinet.z3l".to_owned()),
                lang: Lang::En,
            }),
            "save as starts in the document's folder and keeps its name"
        );
    }

    #[test]
    fn a_new_scene_opens_a_tab_without_asking() {
        // Opening a tab discards nothing, so neither New nor Open has a question to ask (U45); the
        // one question left is a tab close, which the shell draws through `Confirm`.
        let mut menu = FileMenu::new();
        menu.set_path(Some(PathBuf::from("/tmp/cabinet.z3l")));
        assert_eq!(menu.command(MenuCommand::New, Lang::En), Some(FileAction::New));
        assert_eq!(menu.save(Lang::En), Some(FileAction::Save(PathBuf::from("/tmp/cabinet.z3l"))));
    }

    #[test]
    fn the_close_question_names_the_tab_and_offers_both_answers() {
        // The question belongs to the shell, so this test only proves it is the document's name it
        // asks about and that both answers exist; the modal itself needs an egui pass.
        let question = Confirm::tab("cabinet.z3l", Lang::En);
        assert_eq!(question.title, "Close \u{201c}cabinet.z3l\u{201d}?");
        assert!(question.body.contains("unsaved changes"));
        assert_ne!(Answer::Proceed, Answer::Cancel, "cancel and confirm are different answers");
    }

    #[test]
    fn the_delete_question_counts_the_selection_and_names_undo() {
        // ADR 0112: the question is about the whole selection at once, so it says how many nodes go
        // and that one undo brings them back.
        let two = Confirm::delete(2, Lang::En);
        assert_eq!(two.title, "Delete 2 objects?");
        assert!(two.body.contains("Undo (Ctrl+Z)"), "the body says undo is one step: {}", two.body);
        assert_eq!(two.proceed, Message::CmdDelete.text(Lang::En), "the answer is the command's own word");
        assert_eq!(Confirm::delete(5, Lang::Ru).title, "Удалить 5 объектов?", "the Russian plural applies");
        assert_eq!(Confirm::delete(3, Lang::Ru).title, "Удалить 3 объекта?");
    }

    #[test]
    fn the_quit_question_counts_the_unsaved_scenes() {
        let one = Confirm::quit(1, Lang::En);
        assert_eq!(one.title, format!("Quit {}?", crate::theme::PRODUCT_NAME));
        assert!(one.body.contains("One open scene"), "one scene reads as a sentence, not as `1 scenes`: {}", one.body);
        let many = Confirm::quit(3, Lang::En);
        assert!(many.body.contains("3 open scenes"), "and a count reads as one: {}", many.body);
        assert_eq!(many.proceed, "Discard and quit");
    }

    #[test]
    fn the_menu_names_close_and_exit_after_the_file_entries() {
        // *Close tab* and *Exit* are window operations, so the menu reports them as actions the
        // shell performs rather than opening a dialog of its own (X32).
        let mut menu = FileMenu::new();
        assert_eq!(menu.command(MenuCommand::Close, Lang::En), Some(FileAction::Close));
        assert_eq!(menu.command(MenuCommand::Quit, Lang::En), Some(FileAction::Quit));
    }

    #[test]
    fn the_file_name_and_the_path_come_from_the_open_file() {
        let mut menu = FileMenu::new();
        assert_eq!(menu.file_name(Lang::En), "Untitled");
        assert_eq!(menu.path(), None);
        menu.set_path(Some(PathBuf::from("/tmp/cabinet.z3l")));
        assert_eq!(menu.file_name(Lang::En), "cabinet.z3l");
        assert_eq!(menu.path(), Some(Path::new("/tmp/cabinet.z3l")), "the path names the tab for a re-open");
        menu.set_path(None);
        assert_eq!(menu.file_name(Lang::En), "Untitled", "a new scene has no file to overwrite");
    }
}
