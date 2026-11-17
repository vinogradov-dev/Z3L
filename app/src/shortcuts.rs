//! The shell's keyboard: the global shortcuts and the drawing tools' keys.
//!
//! No key code lives here. Every reader asks `ui::inventory::KEYS` by id — [`take_shortcuts`]
//! for the shell's actions and commands, [`take_drawing_shortcut`] for the drawing tools — so a key is
//! data and a remapped binding is an edit to that table (U96). [`take_shortcuts`] reads and consumes
//! them in one pass, so one key does one thing; [`drawing_key`] reads the two keys a running draft
//! obeys. Every reader reports what was pressed and the shell performs it: no shortcut mutates the
//! document.
//!
//! Changes when: a binding in the inventory's key table is added, moved or removed.

use eframe::egui;
use ui::Editor;

/// The global shortcuts, read in one pass and consumed so one key does one thing.
///
/// Each field is the id a binding runs, and the binding itself — the modifiers, the key and the words —
/// comes from the inventory's key table. The drawing tools read their own keys, so only the shell's
/// actions, the delete command and the view keys live at this level.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Shortcuts {
    pub(crate) save: bool,
    /// Open an empty scene in a new tab.
    pub(crate) new_tab: bool,
    /// Close the active tab, asking first when it has unsaved changes.
    pub(crate) close_tab: bool,
    pub(crate) undo: bool,
    pub(crate) redo: bool,
    /// Remove the selection: one node at once, a multi-selection behind one question (ADR 0112).
    pub(crate) delete: bool,
    pub(crate) frame: bool,
    pub(crate) projection: bool,
    /// Turn the drawing grid on or off; only a running draft or the ruler reads it.
    pub(crate) grid_snap: bool,
    /// Turn the active view's ground shadow on or off (U107).
    pub(crate) shadows: bool,
    /// Arm the ruler, or leave it when it is already armed.
    pub(crate) measure: bool,
    /// Arm the niche tool, or leave it when it is already armed.
    pub(crate) niche: bool,
    pub(crate) maximize: bool,
    pub(crate) palette: bool,
    /// Hide both side panels, or show the pair the last press put away.
    pub(crate) hide_panels: bool,
    /// Fold both side panels to strips, or unfold the pair the last press folded.
    pub(crate) collapse_panels: bool,
}

#[cfg(test)]
impl Shortcuts {
    /// Whether the frame pressed the binding with `id`, for the test that walks the key table.
    fn pressed(&self, id: &str) -> bool {
        use ui::inventory::{EDIT_REDO, EDIT_UNDO, FILE_CLOSE, FILE_NEW, FILE_SAVE, PALETTE_OPEN};
        use ui::inventory::{PANELS_COLLAPSE, PANELS_HIDE};
        match id {
            FILE_NEW => self.new_tab,
            FILE_CLOSE => self.close_tab,
            FILE_SAVE => self.save,
            EDIT_UNDO => self.undo,
            EDIT_REDO => self.redo,
            PALETTE_OPEN => self.palette,
            PANELS_HIDE => self.hide_panels,
            PANELS_COLLAPSE => self.collapse_panels,
            "node.delete" => self.delete,
            "view.projection.frame-all" => self.frame,
            "view.projection.toggle" => self.projection,
            "view.snaps.grid" => self.grid_snap,
            "view.display.shadows" => self.shadows,
            "view.layout.maximize" => self.maximize,
            "sketch.measure.length" => self.measure,
            "furniture.panels.void" => self.niche,
            _ => false,
        }
    }

    /// How many of the shell's actions the frame asked for.
    fn pressed_count(&self) -> usize {
        [
            self.save,
            self.new_tab,
            self.close_tab,
            self.undo,
            self.redo,
            self.delete,
            self.frame,
            self.projection,
            self.grid_snap,
            self.shadows,
            self.measure,
            self.niche,
            self.maximize,
            self.palette,
            self.hide_panels,
            self.collapse_panels,
        ]
        .iter()
        .filter(|pressed| **pressed)
        .count()
    }
}

/// What one frame's keys ask of a running drawing draft.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DraftKey {
    /// Drop the draft; the tool leaves nothing in the document.
    Cancel,
    /// Commit the draft as one path.
    Commit,
}

/// The key a running draft obeys: Escape cancels, Enter commits (ADR 0076).
///
/// Escape wins a frame that somehow carried both, because the destructive key is the one the user
/// pressed to get out of the gesture. The right button is handled by the viewport separately.
pub(crate) const fn drawing_key(escape: bool, enter: bool) -> Option<DraftKey> {
    if escape {
        Some(DraftKey::Cancel)
    } else if enter {
        Some(DraftKey::Commit)
    } else {
        None
    }
}

pub(crate) fn take_shortcuts(ctx: &egui::Context) -> Shortcuts {
    ctx.input_mut(|input| Shortcuts {
        // The two panel pairs are read with the more specific one first: a pattern without `Shift`
        // matches a press that carries it, so reading the plain pair first would swallow
        // `Ctrl+Shift+0` and the fold would never run (U78).
        collapse_panels: consume(input, ui::inventory::PANELS_COLLAPSE),
        hide_panels: consume(input, ui::inventory::PANELS_HIDE),
        save: consume(input, ui::inventory::FILE_SAVE),
        new_tab: consume(input, ui::inventory::FILE_NEW),
        close_tab: consume(input, ui::inventory::FILE_CLOSE),
        undo: consume(input, ui::inventory::EDIT_UNDO),
        redo: consume(input, ui::inventory::EDIT_REDO),
        delete: consume(input, "node.delete"),
        frame: consume(input, "view.projection.frame-all"),
        projection: consume(input, "view.projection.toggle"),
        grid_snap: consume(input, "view.snaps.grid"),
        shadows: consume(input, "view.display.shadows"),
        measure: consume(input, "sketch.measure.length"),
        niche: consume(input, "furniture.panels.void"),
        maximize: consume(input, "view.layout.maximize"),
        palette: consume(input, ui::inventory::PALETTE_OPEN),
    })
}

/// Consumes the key the inventory binds to `id`, and says whether the frame carried it.
///
/// A key is data: this reads the modifier pair and the key from the one table, so moving a binding
/// is an edit there and never here. An id the table does not know is not a key, and reading it is a
/// silent `false` — the inventory's own tests are what refuse an id that names nothing.
fn consume(input: &mut egui::InputState, id: &str) -> bool {
    let Some(binding) = ui::inventory::binding(id) else { return false };
    input.consume_key(binding.chord.modifiers(), binding.key)
}

/// Consumes at most one drawing-tool key and starts its tool.
///
/// The table is [`ui::toolbars::shortcut`], the same one the button hints print, so the
/// key and the tooltip cannot drift apart. Returns whether a key was taken. A running draft is
/// replaced, because of ADR 0076: the old draft wrote nothing and the new tool owns the next click.
/// A key that names a tool's own method runs the method the workspace pinned for it, without opening
/// the variant list (U67).
///
/// The shifted keys are read first: a pattern without `Shift` matches a press that carries it, so a
/// plain key read first would swallow its own `Shift` variant and `Shift+R` would start the plain
/// rectangle instead of the pinned method (U78).
pub(crate) fn take_drawing_shortcut(ctx: &egui::Context, editor: &mut Editor, sides: u32) -> bool {
    for shift in [true, false] {
        for tool in ui::toolbars::CONTOUR_TOOLS {
            let Some(shortcut) = ui::toolbars::shortcut(tool) else {
                continue;
            };
            if shortcut.shift != shift {
                continue;
            }
            let modifiers = if shift { egui::Modifiers::SHIFT } else { egui::Modifiers::NONE };
            if ctx.input_mut(|input| input.consume_key(modifiers, shortcut.key)) {
                editor.start_shortcut(tool, sides);
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use scene::Camera;
    use ui::display::{OpeningView, ShadingStyle};
    use ui::{DrawDraft, DrawTool};
    use units::Length;

    use super::*;

    /// One key press, as the platform would hand it to `egui`.
    fn key_input(key: egui::Key, modifiers: egui::Modifiers) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))),
            events: vec![egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers }],
            ..Default::default()
        }
    }

    #[test]
    fn escape_cancels_a_draft_and_enter_commits_it() {
        assert_eq!(drawing_key(true, false), Some(DraftKey::Cancel), "Escape cancels");
        assert_eq!(drawing_key(false, true), Some(DraftKey::Commit), "Enter commits");
        assert_eq!(drawing_key(true, true), Some(DraftKey::Cancel), "the way out wins a frame with both");
        assert_eq!(drawing_key(false, false), None, "no key changes the draft");
    }

    #[test]
    fn a_drawing_shortcut_replaces_the_running_draft() {
        // `R` while a polyline runs starts the rectangle tool with an empty draft (ADR 0076).
        let ctx = egui::Context::default();
        let mut editor = Editor::new();
        editor.start_drawing(DrawTool::Polyline);
        editor.drawing_point([Length::ZERO, Length::ZERO]).expect("start");
        editor.drawing_point([Length::from_millimetres(600).expect("x"), Length::ZERO]).expect("end");

        let mut started = false;
        let output = ctx.run_ui(key_input(egui::Key::R, egui::Modifiers::NONE), |ui| {
            let sides = editor.polygon_sides();
            started = take_drawing_shortcut(ui.ctx(), &mut editor, sides);
        });
        output.drop_without_applying_deltas();
        assert!(started, "the key is taken");
        let draft = editor.drawing().expect("the rectangle tool is live");
        assert_eq!(draft.tool(), DrawTool::Rectangle);
        assert!(draft.points().is_empty(), "the running polyline is gone");
    }

    #[test]
    fn a_drawing_shortcut_takes_one_key_at_a_time() {
        // A plain letter consumes the frame; a second tool would otherwise start from the same
        // key press through the remainder of the table.
        let ctx = egui::Context::default();
        let mut editor = Editor::new();
        let mut started = false;
        let output = ctx.run_ui(key_input(egui::Key::N, egui::Modifiers::NONE), |ui| {
            let sides = editor.polygon_sides();
            started = take_drawing_shortcut(ui.ctx(), &mut editor, sides);
        });
        output.drop_without_applying_deltas();
        assert!(started);
        assert_eq!(editor.drawing().map(DrawDraft::tool), Some(DrawTool::RegularPolygon));

        // The same frame cannot start a second tool: the table already consumed its key.
        let output = ctx.run_ui(Default::default(), |ui| {
            let sides = editor.polygon_sides();
            assert!(!take_drawing_shortcut(ui.ctx(), &mut editor, sides), "no new key");
        });
        output.drop_without_applying_deltas();
        assert_eq!(editor.drawing().map(DrawDraft::tool), Some(DrawTool::RegularPolygon));
    }

    #[test]
    fn the_advertised_shortcuts_are_consumed() {
        let ctx = egui::Context::default();
        let mut read = Shortcuts::default();
        let output = ctx.run_ui(key_input(egui::Key::Z, egui::Modifiers::CTRL), |ui| {
            read = take_shortcuts(ui.ctx());
        });
        output.drop_without_applying_deltas();
        assert_eq!(read, Shortcuts { undo: true, ..Shortcuts::default() });

        let output = ctx.run_ui(key_input(egui::Key::F, egui::Modifiers::NONE), |ui| {
            read = take_shortcuts(ui.ctx());
        });
        output.drop_without_applying_deltas();
        assert_eq!(read, Shortcuts { frame: true, ..Shortcuts::default() });

        // `F` with a modifier is a different shortcut and must not reframe.
        let output = ctx.run_ui(key_input(egui::Key::F, egui::Modifiers::CTRL), |ui| {
            read = take_shortcuts(ui.ctx());
        });
        output.drop_without_applying_deltas();
        assert_eq!(read, Shortcuts::default());

        let output = ctx.run_ui(key_input(egui::Key::K, egui::Modifiers::COMMAND), |ui| {
            read = take_shortcuts(ui.ctx());
        });
        output.drop_without_applying_deltas();
        assert_eq!(read, Shortcuts { palette: true, ..Shortcuts::default() });
    }

    #[test]
    fn the_delete_key_is_a_plain_key() {
        // `Delete` removes the selection and belongs to the shell like the modified pair: a
        // `Ctrl+Delete` is another shortcut and must not delete a selection by accident.
        let ctx = egui::Context::default();
        let mut read = Shortcuts::default();
        let output = ctx.run_ui(key_input(egui::Key::Delete, egui::Modifiers::NONE), |ui| {
            read = take_shortcuts(ui.ctx());
        });
        output.drop_without_applying_deltas();
        assert_eq!(read, Shortcuts { delete: true, ..Shortcuts::default() });

        let output = ctx.run_ui(key_input(egui::Key::Delete, egui::Modifiers::CTRL), |ui| {
            read = take_shortcuts(ui.ctx());
        });
        output.drop_without_applying_deltas();
        assert_eq!(read, Shortcuts::default());
    }

    #[test]
    fn the_shifted_panel_pair_is_not_swallowed_by_the_plain_one() {
        // `consume_key` ignores an extra `Shift`, so the fold has to be read before the hide, or
        // `Ctrl+Shift+0` would hide both panels and the fold shortcut would never run (U78).
        let ctx = egui::Context::default();
        let mut read = Shortcuts::default();
        let output =
            ctx.run_ui(key_input(egui::Key::Num0, egui::Modifiers { shift: true, ..egui::Modifiers::CTRL }), |ui| {
                read = take_shortcuts(ui.ctx());
            });
        output.drop_without_applying_deltas();
        assert_eq!(read, Shortcuts { collapse_panels: true, ..Shortcuts::default() });

        let output = ctx.run_ui(key_input(egui::Key::Num0, egui::Modifiers::CTRL), |ui| {
            read = take_shortcuts(ui.ctx());
        });
        output.drop_without_applying_deltas();
        assert_eq!(read, Shortcuts { hide_panels: true, ..Shortcuts::default() });
    }

    #[test]
    fn a_shifted_key_starts_its_own_variant_and_never_its_plain_tool() {
        // The table advertises `Shift+R` for the rectangle from its centre; reading the plain keys
        // first would take the Shift press for the plain rectangle (U78).
        let ctx = egui::Context::default();
        for (key, modifiers, expected) in [
            (egui::Key::R, egui::Modifiers::SHIFT, DrawTool::RectangleFromCenter),
            (egui::Key::R, egui::Modifiers::NONE, DrawTool::Rectangle),
            (egui::Key::A, egui::Modifiers::SHIFT, DrawTool::ArcFromCenter),
            (egui::Key::C, egui::Modifiers::SHIFT, DrawTool::CircleThreePoints),
        ] {
            let mut editor = Editor::new();
            let output = ctx.run_ui(key_input(key, modifiers), |ui| {
                assert!(take_drawing_shortcut(ui.ctx(), &mut editor, 6), "{key:?} with {modifiers:?} starts a tool");
            });
            output.drop_without_applying_deltas();
            assert_eq!(editor.drawing().map(DrawDraft::tool), Some(expected), "{key:?} with {modifiers:?}");
        }
    }

    #[test]
    fn a_modified_key_never_starts_a_tool() {
        // `Ctrl+N` opens a tab; it must not reach the tool table as the polygon key.
        let ctx = egui::Context::default();
        let mut editor = Editor::new();
        let output = ctx.run_ui(key_input(egui::Key::N, egui::Modifiers::CTRL), |ui| {
            assert!(!take_drawing_shortcut(ui.ctx(), &mut editor, 6), "a Ctrl pair belongs to the shell");
        });
        output.drop_without_applying_deltas();
        assert!(editor.drawing().is_none());
    }

    #[test]
    fn every_key_in_the_table_runs_the_id_it_names() {
        // The key table is the one record of a binding, so a press of each row must reach the reader
        // that row names: a drawing key starts the tool its method id belongs to, and every other
        // key reaches exactly the shell action or command the row is filed under (U96).
        for binding in ui::inventory::KEYS {
            let modifiers = binding.chord.modifiers();
            if let Some(tool) = ui::toolbars::spec::drawing_method(binding.id) {
                let ctx = egui::Context::default();
                let mut editor = Editor::new();
                let mut started = false;
                let output = ctx.run_ui(key_input(binding.key, modifiers), |ui| {
                    started = take_drawing_shortcut(ui.ctx(), &mut editor, 6);
                });
                output.drop_without_applying_deltas();
                assert!(started, "`{}` starts no tool", binding.label);
                assert_eq!(
                    editor.drawing().map(DrawDraft::tool),
                    Some(tool),
                    "`{}` does not start `{}`",
                    binding.label,
                    binding.id
                );
                continue;
            }
            let ctx = egui::Context::default();
            let mut read = Shortcuts::default();
            let output = ctx.run_ui(key_input(binding.key, modifiers), |ui| {
                read = take_shortcuts(ui.ctx());
            });
            output.drop_without_applying_deltas();
            assert!(read.pressed(binding.id), "`{}` does not run `{}`", binding.label, binding.id);
            assert_eq!(read.pressed_count(), 1, "`{}` runs more than one action", binding.label);
        }
    }

    #[test]
    fn alt_w_is_consumed_and_a_single_layout_refuses_it() {
        let ctx = egui::Context::default();
        let mut read = Shortcuts::default();
        let output = ctx.run_ui(key_input(egui::Key::W, egui::Modifiers::ALT), |ui| {
            read = take_shortcuts(ui.ctx());
        });
        output.drop_without_applying_deltas();
        assert_eq!(read, Shortcuts { maximize: true, ..Shortcuts::default() });

        // A plain `W` is a different key and must not expand anything.
        let output = ctx.run_ui(key_input(egui::Key::W, egui::Modifiers::NONE), |ui| {
            read = take_shortcuts(ui.ctx());
        });
        output.drop_without_applying_deltas();
        assert_eq!(read, Shortcuts::default());

        // One view already fills the workspace, so the key is a no-op in the single layout.
        let mut layout =
            crate::view_layout::Layout::new(Camera::default(), OpeningView::Axonometric, ShadingStyle::default());
        layout.toggle_maximized();
        assert!(!layout.maximized());
    }
}
