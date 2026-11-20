//! The header: the product mark, the file and history actions, the tab row and the window's switches.
//!
//! The header is chrome, not a document surface. It owns no document state and dispatches no
//! command by itself: it returns a [`HeaderAction`] and the shell performs it, exactly like the
//! File menu and the tool strip. The window keeps its system frame (`AGENTS.md` §4.10 is about the
//! document; the decision to keep the frame is the owner's), so the header carries no close,
//! minimize or maximize button: its right edge closes with the settings gear.
//!
//! The right cluster is where the window's own modes live: the five transform gestures as one band
//! (ADR 0113), the ribbon fold, the two panel switches and the settings gear. The band is always
//! visible — a gesture is one press away in every state of every tool — and it left the status bar
//! because a 28 point row of readouts cannot share its width with five cells (U115).
//!
//! Every divider on this row separates a meaning group — identity, file, history, tabs, settings —
//! and comes from [`theme::divider_of_height`], as tall as the controls it separates, so the header
//! and the ribbon share one rhythm and no hairline stops halfway down a button.
//!
//! The tab row holds one tab per open scene (U45): its body activates that scene, its cross asks to
//! close it, the marker says whether it has unsaved changes, and the plus beside the row opens a new
//! empty one. The header never knows a path, a session or an undo stack: every fact on the row is
//! handed in by the shell.

use eframe::egui;
use eframe::egui::Ui;
use text::Message;
use ui::TransformMode;

use crate::icons::{self, Icon};
use crate::tabs::TabId;
use crate::theme;

/// The widest a tab title may draw before it elides.
///
/// A file name is a label, not a message: a long one is cut with an ellipsis so the row keeps room
/// for the next tab, and the full name stays in the tooltip.
const TAB_TITLE_WIDTH: f32 = 150.0;

/// The room a tab keeps beside its title: the unsaved marker and the close cross, in points.
const MARKER_ROOM: f32 = 34.0;

/// How much of the accent the active tab's fill carries, from 0 to 1.
///
/// Enough to separate it from the header surface at a glance, not so much that the file name stops
/// reading as text: the accent also marks the bottom edge at full strength.
const ACTIVE_TAB_TINT: f32 = 0.28;

/// The height of the accent bar under the active tab, in points.
const ACTIVE_TAB_BAR: f32 = 2.0;

/// What the header asks the shell to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeaderAction {
    /// Show the start surface: new, open, recent and templates.
    Home,
    /// Undo the last document change.
    Undo,
    /// Redo the last undone change.
    Redo,
    /// Save to the current path, or ask for one.
    Save,
    /// Open the application settings.
    Settings,
    /// Open a new, empty scene in a new tab.
    NewTab,
    /// Fold both side panels to strips, or unfold the pair the last press folded.
    CollapsePanels,
    /// Hide both side panels, or show the pair the last press put away.
    HidePanels,
    /// Fold or unfold the ribbon band.
    ToggleRibbon,
    /// Arm one transform gesture for the whole window; the mode is view state, so no undo entry and
    /// no revision are involved (ADR 0113).
    SetMode(TransformMode),
    /// Show the document of this tab.
    ActivateTab(TabId),
    /// Close this tab; the shell decides whether unsaved changes need a question.
    CloseTab(TabId),
}

/// One open scene, as the tab row draws it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabView {
    /// The identity the shell activates or closes.
    pub id: TabId,
    /// The file name, or the name an unsaved scene goes by.
    pub title: String,
    /// Whether the document has unsaved changes.
    pub dirty: bool,
    /// Whether this is the scene the workspace shows.
    pub active: bool,
}

/// The state the header draws from; nothing here is document data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeaderState {
    /// The open scenes in row order.
    pub tabs: Vec<TabView>,
    /// Whether there is an undo entry.
    pub can_undo: bool,
    /// Whether there is a redo entry.
    pub can_redo: bool,
    /// Whether the ribbon's tool body is unfolded, so its button names the next press.
    pub ribbon_open: bool,
    /// Whether both side panels are hidden, so the button offers to bring them back.
    pub panels_hidden: bool,
    /// Whether both side panels are folded, so the button offers to unfold them.
    pub panels_collapsed: bool,
    /// The transform gesture the viewport offers, which the band beside the switches arms.
    pub mode: TransformMode,
}

/// Draws the header row and returns the one action it asked for, if any.
///
/// The File menu is passed in as a closure so the shell keeps its existing [`crate::file_menu`]
/// menu, its dialogs and its path handling: the header only gives it a place to live on the row.
///
/// The row is two groups: the mark, the file and history actions and the tab row on the left, the
/// window's switches on the right. The right group's width is measured first, on a hidden pass of
/// the same widgets, so the tab row is given the room that is left instead of growing under the
/// switches: the tab strip elides and clips at that boundary, which is what a row that outgrows the
/// window has always done (X94's rule, applied to the header — U115).
pub fn show(ui: &mut Ui, state: &HeaderState, file_menu: impl FnOnce(&mut Ui)) -> Option<HeaderAction> {
    let lang = ui::language(ui.ctx());
    let height = theme::HEADER_HEIGHT;
    let mut action = None;
    egui::Frame::new().fill(theme::surface(ui)).inner_margin(egui::Margin::symmetric(8, 0)).show(ui, |ui| {
        // One step between the controls rather than the workspace's step and a half: the band and
        // the four switches need that room on the narrowest window the shell opens, and a divider
        // already carries a step of air on either side (U115).
        ui.spacing_mut().item_spacing.x = theme::STEP;
        let reserve = cluster_width(ui, state) + ui.spacing().item_spacing.x;
        // `Sides` gives the left group the row and pins the right group to its far edge; two nested
        // layouts would leave the gear wherever the tab strip happened to end.
        let (left, right) = egui::Sides::new().height(height).shrink_right().show(
            ui,
            |ui| {
                // The left group ends where the switches begin: a group whose content outgrows its
                // room is cut at that edge instead of painting under the band, which is the one
                // control on this row that may never be hidden (U115).
                let room = ui.available_rect_before_wrap();
                let rect = egui::Rect::from_min_max(
                    room.min,
                    egui::pos2((room.right() - reserve).max(room.left()), room.bottom()),
                );
                ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                    ui.set_clip_rect(ui.clip_rect().intersect(rect));
                    let mut action = None;
                    let mark = icons::wordmark(ui, 14.0, theme::text(ui));
                    mark.on_hover_text(theme::PRODUCT_NAME)
                        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, theme::PRODUCT_NAME));
                    ui.add_space(8.0);
                    theme::divider_of_height(ui, theme::HEADER_CONTROL_HEIGHT);

                    if icon_button(ui, icons::view::HOME, Message::HeaderHome.text(lang), true, false).clicked() {
                        action = Some(HeaderAction::Home);
                    }
                    theme::divider_of_height(ui, theme::HEADER_CONTROL_HEIGHT);

                    file_menu(ui);
                    theme::divider_of_height(ui, theme::HEADER_CONTROL_HEIGHT);

                    let hint = keyed_hint(Message::HeaderSave, ui::inventory::FILE_SAVE, lang);
                    if icon_button(ui, icons::actions::FILE_SAVE, &hint, true, false).clicked() {
                        action = Some(HeaderAction::Save);
                    }
                    theme::divider_of_height(ui, theme::HEADER_CONTROL_HEIGHT);

                    let hint = if state.can_undo {
                        keyed_hint(Message::HeaderUndo, ui::inventory::EDIT_UNDO, lang)
                    } else {
                        Message::HeaderNothingToUndo.text(lang).to_owned()
                    };
                    if icon_button(ui, icons::actions::UNDO, &hint, state.can_undo, false).clicked() && state.can_undo {
                        action = Some(HeaderAction::Undo);
                    }
                    let hint = if state.can_redo {
                        keyed_hint(Message::HeaderRedo, ui::inventory::EDIT_REDO, lang)
                    } else {
                        Message::HeaderNothingToRedo.text(lang).to_owned()
                    };
                    if icon_button(ui, icons::actions::REDO, &hint, state.can_redo, false).clicked() && state.can_redo {
                        action = Some(HeaderAction::Redo);
                    }
                    theme::divider_of_height(ui, theme::HEADER_CONTROL_HEIGHT);

                    // The tab row gets what is left of the row once the switches have their measured
                    // share, so it stops before the band instead of growing under it.
                    if let Some(tab_action) = tab_row(ui, state, ui.available_width()) {
                        action = Some(tab_action);
                    }
                    action
                })
                .inner
            },
            |ui| cluster(ui, state),
        );
        action = left.or(right);
    });
    action
}

/// The right cluster: the settings gear, the ribbon fold, the two panel switches and the mode band.
///
/// The order is the one a right-to-left layout draws from the window's edge inwards, so the gear is
/// the row's last control and the band stands beside the tab row (U115).
fn cluster(ui: &mut Ui, state: &HeaderState) -> Option<HeaderAction> {
    let lang = ui::language(ui.ctx());
    let mut action = None;
    if icon_button(ui, icons::actions::SETTINGS, Message::SettingsTitle.text(lang), true, false).clicked() {
        action = Some(HeaderAction::Settings);
    }
    theme::divider_of_height(ui, theme::HEADER_CONTROL_HEIGHT);
    let (ribbon_icon, ribbon_hint) = if state.ribbon_open {
        (icons::actions::COLLAPSE, Message::FoldRibbon)
    } else {
        (icons::actions::EXPAND, Message::UnfoldRibbon)
    };
    if icon_button(ui, ribbon_icon, ribbon_hint.text(lang), true, !state.ribbon_open).clicked() {
        action = Some(HeaderAction::ToggleRibbon);
    }
    let (hide_icon, hide_hint) = if state.panels_hidden {
        (icons::actions::VISIBLE, Message::ShowAllPanels)
    } else {
        (icons::actions::HIDDEN, Message::HideAllPanels)
    };
    let hide_hint = keyed_hint(hide_hint, ui::inventory::PANELS_HIDE, lang);
    if icon_button(ui, hide_icon, &hide_hint, true, state.panels_hidden).clicked() {
        action = Some(HeaderAction::HidePanels);
    }
    let (collapse_hint, collapse_selected) =
        if state.panels_collapsed { (Message::ExpandAllPanels, true) } else { (Message::CollapseAllPanels, false) };
    let collapse_hint = keyed_hint(collapse_hint, ui::inventory::PANELS_COLLAPSE, lang);
    if icon_button(ui, icons::actions::PANEL_LEFT, &collapse_hint, true, collapse_selected).clicked() {
        action = Some(HeaderAction::CollapsePanels);
    }
    theme::divider_of_height(ui, theme::HEADER_CONTROL_HEIGHT);
    if let Some(mode) = mode_band(ui, state.mode) {
        action = Some(HeaderAction::SetMode(mode));
    }
    action
}

/// The width the right cluster wants, measured on a hidden pass of the same widgets.
///
/// The probe is a hidden ui clipped to nothing: it paints nothing, takes no click and keeps its
/// galleys out of the frame, so the cost is one more layout of the row's switches and five words
/// (X94). Without it the tab row is laid out from the row's left edge and the band, which is pinned
/// to the right, paints over the open file's name on the narrowest window.
fn cluster_width(ui: &mut Ui, state: &HeaderState) -> f32 {
    let probe =
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(ui.available_width().max(0.0), theme::HEADER_HEIGHT));
    let mut probe_ui = ui.new_child(
        egui::UiBuilder::new()
            .id_salt("header_cluster")
            .max_rect(probe)
            .layout(egui::Layout::left_to_right(egui::Align::Center))
            .invisible(),
    );
    probe_ui.set_clip_rect(egui::Rect::NOTHING);
    cluster(&mut probe_ui, state);
    probe_ui.min_rect().width()
}

/// The size a mode's glyph takes in the band, in points.
///
/// It is the inline size: the band is window chrome, and a toolbar glyph would push the last cell
/// past the switch that follows it on the narrowest window the shell opens (U113).
const MODE_ICON_POINT_SIZE: f32 = 14.0;

/// The window's transform gesture as one band: *Select*, *Move*, *Object*, *Rotate* and *Turn* side
/// by side, the armed one filled.
///
/// States of one value are one control, not five switches: the band behind them says they belong
/// together, and the words alone name each one because the header has no room for five cells that
/// print their keys as well. The key is not lost — the hover names it, and so do the node menu and
/// the Settings page — and the binding itself is unchanged (U115).
///
/// The mode is the window's view state, so the band reads it and returns the one pressed; the shell
/// writes it, and no command and no undo entry is involved.
fn mode_band(ui: &mut Ui, mode: TransformMode) -> Option<TransformMode> {
    let lang = ui::language(ui.ctx());
    let band = egui::Frame::new()
        .fill(theme::control(ui))
        .corner_radius(egui::CornerRadius::same(theme::RADIUS))
        .inner_margin(egui::Margin::symmetric(1, 0));
    let mut chosen = mode;
    let mut inner = band.show(ui, |ui| {
        // Touching cells are what makes one band out of five states; the row's own gap stays.
        ui.spacing_mut().item_spacing.x = 0.0;
        // One point a side is what five cells leave of a header that also carries the mark, the file
        // actions, the tab row and four view switches.
        ui.spacing_mut().button_padding.x = 1.0;
        let mut clicked = false;
        // The right cluster lays its children out right to left, and a band reads left to right: the
        // cells are added from the last to the first, so *Select* is the leftmost of the five and the
        // order a person reads is the order of [`TransformMode::ALL`].
        for entry in TransformMode::ALL.into_iter().rev() {
            let text = entry.cell(ui, lang);
            let image = entry.icon().and_then(|icon| ui::icons::icon_image(ui, icon, MODE_ICON_POINT_SIZE));
            let response = match image {
                Some(image) => {
                    let response = ui.add(egui::Button::image_and_text(image, text).selected(chosen == entry));
                    if response.clicked() {
                        chosen = entry;
                    }
                    response
                }
                None => ui.selectable_value(&mut chosen, entry, text),
            };
            clicked |= response.on_hover_text(icons::tool_hint(entry.hint(lang), entry.key())).clicked();
        }
        clicked
    });
    // The band names itself for a screen reader, and its own sentence answers the frame's hover: a
    // cell keeps the hint that names its gesture (U118's rule for the shading band).
    inner
        .response
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Group, ui.is_enabled(), Message::Gizmo.text(lang)));
    inner.response = inner.response.on_hover_text(Message::Gizmo.text(lang));
    inner.inner.then_some(chosen)
}

/// One row of tabs and the plus that opens a new scene, clipped to the `room` the switches leave.
///
/// A row that outgrows its room is cut at that edge and its titles elide, because the row owns the
/// space between the history buttons and the window's switches and nothing beyond it: the band on
/// the right is always visible (U115).
fn tab_row(ui: &mut Ui, state: &HeaderState, room: f32) -> Option<HeaderAction> {
    let lang = ui::language(ui.ctx());
    let mut action = None;
    let available = ui.available_rect_before_wrap();
    let rect = egui::Rect::from_min_size(available.min, egui::vec2(room.max(1.0), available.height()));
    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
        ui.set_clip_rect(ui.clip_rect().intersect(rect));
        ui.horizontal(|ui| {
            for tab in &state.tabs {
                match tab_view(ui, tab) {
                    Some(TabClick::Activate) => action = Some(HeaderAction::ActivateTab(tab.id)),
                    Some(TabClick::Close) => action = Some(HeaderAction::CloseTab(tab.id)),
                    None => {}
                }
            }
            let hint = keyed_hint(Message::HeaderNewScene, ui::inventory::FILE_NEW, lang);
            if icon_button(ui, icons::actions::ADD, &hint, true, false).clicked() {
                action = Some(HeaderAction::NewTab);
            }
        });
    });
    action
}

/// What a click on one tab asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TabClick {
    /// Show this scene.
    Activate,
    /// Close it.
    Close,
}

/// One open file: its name, the unsaved marker, its close button and its pressed state.
///
/// The active tab is filled with the accent and underlined by an accent bar, because the header
/// surface and the tab surface were too close to tell apart at a glance. The cross always closes and
/// the body always activates: the whole tab is one click target, so the answer comes from where the
/// pointer was, not from which of the two overlapping widgets egui handed the click to.
fn tab_view(ui: &mut Ui, tab: &TabView) -> Option<TabClick> {
    let mut close_rect = None;
    let mut close_clicked = false;
    let frame = egui::Frame::new()
        .fill(if tab.active { active_tab_fill(ui) } else { egui::Color32::TRANSPARENT })
        .inner_margin(egui::Margin::symmetric(10, 3))
        .corner_radius(egui::CornerRadius::same(theme::RADIUS));
    let inner = frame.show(ui, |ui| {
        ui.horizontal(|ui| {
            // The scene in front of the user carries the full text colour, the others step back.
            let colour = if tab.active { theme::text(ui) } else { theme::text_dim(ui) };
            // The title takes the width its name needs, capped twice: at [`TAB_TITLE_WIDTH`], so a
            // long path-free name cannot push its neighbour out of the row, and at what the row has
            // left for it once the unsaved marker and the cross have their room (U115). The full
            // name stays in the tooltip.
            let room = (ui.available_width() - MARKER_ROOM).max(0.0);
            ui.scope(|ui| {
                ui.set_max_width(TAB_TITLE_WIDTH.min(room));
                ui.add(egui::Label::new(egui::RichText::new(&tab.title).size(12.0).color(colour)).truncate())
                    .on_hover_text(&tab.title);
            });
            if tab.dirty {
                ui.label(egui::RichText::new("●").size(9.0).color(theme::warn(ui)))
                    .on_hover_text(Message::HeaderUnsaved.text(ui::language(ui.ctx())));
            }
            let close = icons::icon_button_at(
                ui,
                icons::window::CLOSE,
                11.0,
                Message::HeaderCloseTab.text(ui::language(ui.ctx())),
                true,
                false,
            );
            close_clicked = close.clicked();
            close_rect = Some(close.rect);
        });
    });
    let rect = inner.response.rect;
    if tab.active {
        // A bar on the bottom edge, its corners rounded like the tab's, so the mark follows the
        // shape instead of boxing it: one accent line says which scene the workspace shows.
        let radius = theme::RADIUS;
        let bar =
            egui::Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - ACTIVE_TAB_BAR), rect.right_bottom());
        ui.painter().rect_filled(bar, egui::CornerRadius { nw: 0, ne: 0, sw: radius, se: radius }, theme::accent(ui));
    }
    // The padding around the body is part of the tab too: a click on it activates the scene instead
    // of doing nothing, which is what a tab strip is expected to do.
    let hit = ui.interact(rect, ui.scope_id().with(("tab", tab.id)), egui::Sense::click());
    if !hit.clicked() && !close_clicked {
        return None;
    }
    let pointer = hit.interact_pointer_pos().or_else(|| ui.input(|input| input.pointer.interact_pos()));
    tab_click(close_rect, pointer)
}

/// What a click on a tab asked for: the cross closes it, anything else activates it.
///
/// The rule is a function of the pointer rather than of the widget responses, because the tab body
/// and its cross overlap: whichever of the two egui gives the click to, the answer must be the same.
fn tab_click(close: Option<egui::Rect>, pointer: Option<egui::Pos2>) -> Option<TabClick> {
    match pointer {
        Some(pointer) if close.is_some_and(|close| close.contains(pointer)) => Some(TabClick::Close),
        Some(_) => Some(TabClick::Activate),
        None => None,
    }
}

/// The fill of the active tab: the tab surface pulled towards the accent.
///
/// A mix rather than a translucent overlay, so the result is one opaque colour that reads the same
/// on the dark and the light theme and never lets the header surface show through.
fn active_tab_fill(ui: &Ui) -> egui::Color32 {
    let base = egui::Rgba::from(theme::panel(ui));
    let accent = egui::Rgba::from(theme::accent(ui));
    egui::Color32::from(base * (1.0 - ACTIVE_TAB_TINT) + accent * ACTIVE_TAB_TINT)
}

/// The header's icon button: a drawn icon, or its label if the asset is still empty.
///
/// The button is one header control — [`theme::HEADER_CONTROL_HEIGHT`] tall, square — so it lines up
/// with the `File` text button beside it instead of standing a few points taller. The icon itself is
/// 18 pt inside that cell, which is the optical size the header needs.
/// A header button's hint: the action's words and the key the inventory binds to it.
///
/// The key is data, so the header never spells `Ctrl+S` itself: a remapped binding reads correctly
/// here without a second edit (U96).
fn keyed_hint(words: Message, id: &str, lang: text::Lang) -> String {
    icons::tool_hint(words.text(lang), ui::inventory::key_label(id))
}

fn icon_button(ui: &mut Ui, icon: Icon, label: &str, enabled: bool, selected: bool) -> egui::Response {
    icons::icon_button_sized(ui, icon, theme::HEADER_CONTROL_HEIGHT, 18.0, label, enabled, selected)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use egui::epaint::Shape;

    use super::*;

    fn tab(id: u64, title: &str, dirty: bool, active: bool) -> TabView {
        TabView { id: TabId(id), title: title.to_owned(), dirty, active }
    }

    fn state(tabs: Vec<TabView>, can_undo: bool) -> HeaderState {
        HeaderState {
            tabs,
            can_undo,
            can_redo: false,
            ribbon_open: true,
            panels_hidden: false,
            panels_collapsed: false,
            mode: TransformMode::Select,
        }
    }

    /// Runs the header and returns its text shapes.
    fn drawn(state: &HeaderState) -> Vec<String> {
        drawn_in(state, text::Lang::En)
    }

    /// Runs the header in one language, so a switch can be compared without a document.
    fn drawn_in(state: &HeaderState, lang: text::Lang) -> Vec<String> {
        let ctx = egui::Context::default();
        theme::install(&ctx);
        ctx.enable_accesskit();
        ui::set_language(&ctx, lang);
        let output = ctx.run_ui(Default::default(), |ui| {
            show(ui, state, |_ui| {});
        });
        let mut out = Vec::new();
        for clipped in &output.shapes {
            walk(&clipped.shape, &mut out);
        }
        output.drop_without_applying_deltas();
        out
    }

    fn walk(shape: &Shape, out: &mut Vec<String>) {
        match shape {
            Shape::Text(text) => out.push(text.galley.text().to_owned()),
            Shape::Vec(shapes) => {
                for shape in shapes {
                    walk(shape, out);
                }
            }
            _ => {}
        }
    }

    /// Every filled rectangle the header drew, by colour.
    fn fills(shape: &Shape, out: &mut Vec<egui::Color32>) {
        match shape {
            Shape::Rect(rect) => out.push(rect.fill),
            Shape::Vec(shapes) => {
                for shape in shapes {
                    fills(shape, out);
                }
            }
            _ => {}
        }
    }

    /// Every textured rectangle the header drew, so a test can see an image and not only text.
    fn textured_rects(state: &HeaderState) -> Vec<egui::Rect> {
        fn walk_textured(shape: &Shape, out: &mut Vec<egui::Rect>) {
            match shape {
                // A painter image is a textured mesh, not a rect: its bounds are its vertices'.
                Shape::Mesh(mesh) => {
                    let mut bounds = egui::Rect::NOTHING;
                    for vertex in &mesh.vertices {
                        bounds = bounds.union(egui::Rect::from_min_max(vertex.pos, vertex.pos));
                    }
                    if bounds.is_positive() {
                        out.push(bounds);
                    }
                }
                Shape::Vec(shapes) => {
                    for shape in shapes {
                        walk_textured(shape, out);
                    }
                }
                _ => {}
            }
        }
        let ctx = egui::Context::default();
        theme::install(&ctx);
        let output = ctx.run_ui(Default::default(), |ui| {
            show(ui, state, |_ui| {});
        });
        let mut out = Vec::new();
        for clipped in &output.shapes {
            walk_textured(&clipped.shape, &mut out);
        }
        output.drop_without_applying_deltas();
        out
    }

    /// One frame of the header at `screen` and the shapes it drew.
    fn header_frame(state: &HeaderState, lang: text::Lang, screen: egui::Rect) -> egui::FullOutput {
        let ctx = egui::Context::default();
        theme::install(&ctx);
        ctx.enable_accesskit();
        ui::set_language(&ctx, lang);
        ctx.run_ui(egui::RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| {
            ui.set_width(screen.width());
            // The shell hands `show` its real File menu; the stand-in takes the same text button so a
            // measurement of the row is the row the shell draws.
            show(ui, state, |ui| {
                let lang = ui::language(ui.ctx());
                ui.add_sized(
                    egui::vec2(theme::HEADER_TEXT_WIDTH, theme::HEADER_CONTROL_HEIGHT),
                    egui::Button::new(Message::File.text(lang)),
                );
            });
        })
    }

    /// The painted part of every drawn text, with its clip rect applied, and its words.
    fn painted_text(output: &egui::FullOutput) -> Vec<(egui::Rect, String)> {
        let mut out = Vec::new();
        for clipped in &output.shapes {
            let mut texts = Vec::new();
            walk(&clipped.shape, &mut texts);
            if let egui::Shape::Text(text) = &clipped.shape {
                let painted = clipped.shape.visual_bounding_rect().intersect(clipped.clip_rect);
                out.push((painted, text.galley.text().to_owned()));
            }
        }
        out
    }

    /// The band's five cells, in the order the band draws them.
    fn band_cells(state: &HeaderState, lang: text::Lang, screen: egui::Rect) -> Vec<(egui::Rect, String)> {
        let output = header_frame(state, lang, screen);
        let cells = crate::test_support::drawn_text_rects(&output.shapes);
        output.drop_without_applying_deltas();
        let words: Vec<String> = TransformMode::ALL.iter().map(|mode| mode.caption(lang)).collect();
        cells.into_iter().filter(|(_, text)| words.contains(text)).collect()
    }

    #[test]
    fn the_header_bands_the_five_gestures_in_order() {
        // U115: the five transform modes left the status bar for the header's right cluster, so the
        // gesture is always one press away whatever tool is armed. The band prints the words, and
        // the key is in the hover, in the node menu and on the Settings page.
        let state = state(vec![tab(0, "cabinet.z3l", false, true)], false);
        for lang in [text::Lang::En, text::Lang::Ru] {
            let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 400.0));
            let mut cells = band_cells(&state, lang, screen);
            assert_eq!(cells.len(), TransformMode::ALL.len(), "every mode draws one cell in {lang:?}: {cells:?}");
            cells.sort_by(|left, right| left.0.left().total_cmp(&right.0.left()));
            let words: Vec<&str> = cells.iter().map(|(_, text)| text.as_str()).collect();
            let band: Vec<String> = TransformMode::ALL.iter().map(|mode| mode.caption(lang)).collect();
            assert_eq!(
                words,
                band.iter().map(String::as_str).collect::<Vec<_>>(),
                "the band reads in the declared order"
            );
        }
    }

    #[test]
    fn a_click_on_a_mode_returns_that_mode() {
        // The band reads the window's mode and returns the one pressed; the shell writes it and no
        // command, revision or undo entry is involved (ADR 0113).
        let ctx = egui::Context::default();
        theme::install(&ctx);
        let state = state(vec![tab(0, "cabinet.z3l", false, true)], false);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 400.0));
        let caption = TransformMode::Object.caption(text::Lang::En);
        let mut at = None;
        let mut asked = None;
        let mut asked_any = None;
        for frame in 0..6 {
            let press = |pressed| egui::Event::PointerButton {
                pos: at.unwrap_or_default(),
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            let events = match (frame, at) {
                (0, _) | (_, None) => Vec::new(),
                (1, Some(at)) => vec![egui::Event::PointerMoved(at)],
                (2, Some(_)) => vec![press(true)],
                (3, Some(_)) => vec![press(false)],
                _ => Vec::new(),
            };
            let output = ctx.run_ui(egui::RawInput { screen_rect: Some(screen), events, ..Default::default() }, |ui| {
                asked = show(ui, &state, |_ui| {});
            });
            if asked.is_some() {
                asked_any = asked.take();
            }
            if frame == 0 {
                at = crate::test_support::drawn_text_rects(&output.shapes)
                    .into_iter()
                    .find(|(_, text)| *text == caption)
                    .map(|(rect, _)| rect.center());
                assert!(at.is_some(), "the band draws {caption}");
            }
            output.drop_without_applying_deltas();
        }
        assert_eq!(asked_any, Some(HeaderAction::SetMode(TransformMode::Object)), "a click arms the gesture it names");
    }

    #[test]
    fn the_band_fits_the_narrowest_window_beside_the_view_switches() {
        // The band is window chrome, so it must fit the narrowest window the shell opens (640 pt)
        // with the mark, the file actions and the tab row to its left and four view switches beside
        // it: a mode nobody can see is a mode nobody has. The words alone are what makes that fit —
        // the key stays in the hover — and the left group is cut at the switches' edge rather than
        // painted under the band. The tab row is the first thing the row gives up, which is what a
        // row that outgrows its window has always done.
        let state = state(vec![tab(0, "cabinet.z3l", false, true)], false);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 400.0));
        for lang in [text::Lang::En, text::Lang::Ru] {
            let mut cells = band_cells(&state, lang, screen);
            cells.sort_by(|left, right| left.0.left().total_cmp(&right.0.left()));
            let [first, ..] = cells.as_slice() else { panic!("the band draws its cells in {lang:?}") };
            let last = cells.last().expect("the band draws its last cell");
            assert!(
                last.0.right() <= 500.0,
                "four view switches and the gear still fit beside the band in {lang:?}: {}",
                last.0.right()
            );
            assert!(
                last.0.right() - first.0.left() <= 300.0,
                "the five words hold under half the row in {lang:?}: {}",
                last.0.right() - first.0.left()
            );
            let output = header_frame(&state, lang, screen);
            let painted = painted_text(&output);
            output.drop_without_applying_deltas();
            // The file menu and the history row are not cut away by the band.
            let word = Message::File.text(lang);
            let file = painted.iter().find(|(_, text)| text == word);
            let (file, _) = file.unwrap_or_else(|| panic!("the file menu is drawn in {lang:?}"));
            assert!(!file.is_negative(), "the file menu keeps a painted rectangle in {lang:?}");
            // Nothing of the left group paints over the band.
            let boundary = first.0.left() - theme::STEP;
            let band: Vec<&str> = words_of(lang);
            let crossing: Vec<(egui::Rect, String)> = painted
                .iter()
                .filter(|(rect, text)| rect.right() > boundary && !band.contains(&text.as_str()))
                .cloned()
                .collect();
            assert!(crossing.is_empty(), "no left-group text paints under the band in {lang:?}: {crossing:?}");
        }
    }

    /// The band's five words, which are the row's own text on the right.
    fn words_of(lang: text::Lang) -> Vec<&'static str> {
        TransformMode::ALL.iter().map(|mode| mode.label(lang)).collect()
    }

    #[test]
    fn the_band_names_itself_as_one_control() {
        // Five touching cells are one control: the frame carries a group name for a screen reader,
        // so the band is not read as five unrelated buttons (U118's rule, applied to the mode band).
        let state = state(vec![tab(0, "cabinet.z3l", false, true)], false);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 400.0));
        let ctx = egui::Context::default();
        theme::install(&ctx);
        ctx.enable_accesskit();
        let output = ctx.run_ui(egui::RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| {
            show(ui, &state, |_ui| {});
        });
        let groups: Vec<String> = output
            .platform_output
            .accesskit_update
            .as_ref()
            .map(|update| {
                update
                    .nodes
                    .iter()
                    .filter(|(_, node)| node.role() == egui::accesskit::Role::Group)
                    .filter_map(|(_, node)| node.label().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        output.drop_without_applying_deltas();
        assert!(
            groups.contains(&Message::Gizmo.text(text::Lang::En).to_owned()),
            "the band carries one group name: {groups:?}"
        );
    }

    #[test]
    fn the_header_draws_the_wordmark_and_names_every_open_file() {
        let header = state(vec![tab(0, "cabinet.z3l", false, true), tab(1, "shelf.z3l", false, false)], false);
        let marks = textured_rects(&header);
        assert!(
            marks.iter().any(|rect| (rect.width() / rect.height() - icons::LOGO_ASPECT).abs() < 0.01),
            "the header draws the wordmark at its own aspect: {marks:?}"
        );
        let texts = drawn(&header);
        assert!(texts.iter().any(|text| text == "cabinet.z3l"), "the first tab is drawn: {texts:?}");
        assert!(texts.iter().any(|text| text == "shelf.z3l"), "and so is the second: {texts:?}");
        assert!(!texts.iter().any(|text| text == "●"), "a clean scene has no unsaved marker: {texts:?}");
    }

    #[test]
    fn only_an_unsaved_tab_carries_the_marker() {
        let texts = drawn(&state(vec![tab(0, "cabinet.z3l", true, true), tab(1, "shelf.z3l", false, false)], false));
        assert_eq!(texts.iter().filter(|text| *text == "●").count(), 1, "one marker, on the dirty tab: {texts:?}");
    }

    #[test]
    fn an_empty_history_starts_with_nothing_to_undo() {
        let state = state(vec![tab(0, "cabinet.z3l", false, true)], false);
        assert!(!state.can_undo, "a fresh scene has no command to take back");
        assert!(!state.can_redo);
    }

    #[test]
    fn the_settings_button_sits_on_the_far_edge_of_the_row() {
        // The gear is the only control on the right group; a header that lays the two groups out
        // one after another leaves it clipped or floating mid-row, which is how it looked before
        // `Sides` gave the right group its own edge.
        let ctx = egui::Context::default();
        theme::install(&ctx);
        let mut edge = 0.0;
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 400.0))),
            ..Default::default()
        };
        let output = ctx.run_ui(input, |ui| {
            let state = state(vec![tab(0, "cabinet.z3l", false, true)], false);
            show(ui, &state, |_ui| {});
            edge = ui.min_rect().right();
        });
        output.drop_without_applying_deltas();
        assert!(edge > 1100.0, "the row reaches its own right edge: {edge}");
    }

    #[test]
    fn a_tab_body_click_asks_for_nothing() {
        // The body of the active tab is the document already on screen; a click on it changes no
        // state, so the header reports no action.
        let ctx = egui::Context::default();
        theme::install(&ctx);
        let output = ctx.run_ui(Default::default(), |ui| {
            let state = state(vec![tab(0, "cabinet.z3l", false, true)], false);
            show(ui, &state, |_ui| {});
        });
        output.drop_without_applying_deltas();
    }

    #[test]
    fn the_cross_closes_the_tab_and_the_body_activates_it() {
        // The body and the cross overlap, so the answer is read from the pointer: a click that
        // landed on the cross used to activate the tab it was meant to close (X32).
        let close = egui::Rect::from_min_size(egui::pos2(104.0, 4.0), egui::vec2(12.0, 12.0));
        assert_eq!(
            tab_click(Some(close), Some(egui::pos2(110.0, 10.0))),
            Some(TabClick::Close),
            "a click on the cross closes the scene"
        );
        assert_eq!(
            tab_click(Some(close), Some(egui::pos2(40.0, 10.0))),
            Some(TabClick::Activate),
            "a click on the name shows the scene"
        );
        assert_eq!(
            tab_click(Some(close), Some(egui::pos2(8.0, 19.0))),
            Some(TabClick::Activate),
            "the padding belongs to the tab body"
        );
        assert_eq!(tab_click(Some(close), None), None, "a click with no position asks for nothing");
    }

    #[test]
    fn the_active_tab_carries_the_accent_bar() {
        // The tab surface and the header surface are close enough that the fill alone did not read:
        // the scene in front of the user is marked by an accent bar under its name (X32).
        let ctx = egui::Context::default();
        theme::install(&ctx);
        let accent = ctx.style_of(egui::Theme::Dark).visuals.hyperlink_color;
        let state = state(vec![tab(0, "cabinet.z3l", false, true), tab(1, "shelf.z3l", false, false)], false);
        let output = ctx.run_ui(Default::default(), |ui| {
            show(ui, &state, |_ui| {});
        });
        let mut drawn = Vec::new();
        for clipped in &output.shapes {
            fills(&clipped.shape, &mut drawn);
        }
        output.drop_without_applying_deltas();
        assert_eq!(
            drawn.iter().filter(|fill| **fill == accent).count(),
            1,
            "exactly the active tab draws the accent, and it draws it solid: {drawn:?}"
        );
    }

    fn labels_in(state: &HeaderState, lang: text::Lang) -> Vec<String> {
        let ctx = egui::Context::default();
        theme::install(&ctx);
        ctx.enable_accesskit();
        ui::set_language(&ctx, lang);
        let output = ctx.run_ui(Default::default(), |ui| {
            show(ui, state, |_ui| {});
        });
        let labels = output
            .platform_output
            .accesskit_update
            .as_ref()
            .map(|update| update.nodes.iter().filter_map(|(_, node)| node.label().map(str::to_owned)).collect())
            .unwrap_or_default();
        output.drop_without_applying_deltas();
        labels
    }

    #[test]
    fn switching_the_language_redraws_the_header() {
        let state = state(vec![tab(0, "cabinet.z3l", false, true)], true);
        let english = labels_in(&state, text::Lang::En);
        let russian = labels_in(&state, text::Lang::Ru);
        assert!(english.iter().any(|label| label.contains("Home")), "{english:?}");
        assert!(russian.iter().any(|label| label.contains("Главная")), "{russian:?}");
        assert_ne!(english, russian, "the same header names its controls differently");
    }
}
