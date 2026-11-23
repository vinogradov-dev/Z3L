//! The status bar: what the armed tool waits for, what is selected and how the window is set.
//!
//! One thin row at the bottom of the window, in three zones of text: the armed tool and the step it
//! waits for on the left, the resolved selection in the middle — kind, name, size, material, banding
//! and the first warning, or the ruler's live length and the niche's cell while they run — and the
//! window's readouts on the right: the snap step, the projection, the scale and the idle/busy state.
//!
//! **It reports; it never acts.** Every instrument left this row for a toolbar or a card (U115): the
//! five transform modes are one band in the header's right cluster, the snap's switch is the View
//! tab's Snaps group under its own `view.snaps.grid` row, a draft's and a contour edit's accept and
//! drop are the one row of the armed tool's card, and the container that receives the next creation
//! is armed and disarmed in its own inspector card. A control here would compete with the readout
//! beside it on a row 28 points tall, and the row is what a glance reads while the hand works.
//!
//! It reads a [`StatusInfo`] snapshot of the session, the editor and the view camera, draws one row
//! and writes nothing: no document, no view state, no window state. The zones are owned rectangles
//! that clip to their own edges, so no zone paints under another (X94).
//!
//! Changes when: what the bar reports changes — not when a tool gains a gesture or a control, which
//! belongs to the tool's own module or to the surface that now owns it.

use catalog::Directory;
use eframe::egui;
use model::{Geometry, Id, Node, Project};
use scene::Projection;
use text::{Lang, Message};
use ui::{DrawTool, MeasureStage, MeshElement, VoidRefusal, theme};
use units::Length;

use crate::App;
use crate::document::Document;
use crate::view_cluster::NavigationMode;

/// What separates the parts of the selection context.
const SUMMARY_SEPARATOR: &str = " · ";

/// The status text and controls, read once per frame so both sides of the bar draw from data.
struct StatusInfo {
    error: Option<String>,
    /// Why the workspace defaults were used instead of the saved settings, if they were.
    notice: Option<crate::settings_store::Notice>,
    /// Whether a view fitted itself to the scene just now, and the bar still owes the sentence.
    fitted: bool,
    /// The live contour edit: its segment count, whether it differs from the document and whether
    /// a corner drag follows the contour.
    contour: Option<(usize, bool, bool)>,
    /// The live draft: its tool, its point count and whether the next segment is an arc.
    drawing: Option<(DrawTool, usize, bool)>,
    /// Why the pointer cannot extend the live draft, if it cannot.
    preview_error: Option<String>,
    /// The armed ruler: the step it waits for and the length it shows, if it has one.
    measure: Option<(MeasureStage, Option<Length>)>,
    /// The armed niche: the cell's three sides, or the reason the pointer names none.
    void: Option<Result<[Length; 3], VoidRefusal>>,
    /// The armed area pull: what the next click takes, or the reason the pointer names no pullable area.
    area: Option<Message>,
    /// Whether the object move is carrying a piece the pointer moves with no button held.
    holding: bool,
    /// Whether the drawing plane's grid snap is on.
    grid: bool,
    /// What captured the pointer in the last frame, when a snapping tool owns it (U116).
    snap: Option<model::SnapKind>,
    mesh: Option<(MeshElement, usize)>,
    /// The navigation mode armed in the floating cluster, if any.
    navigation: Option<NavigationMode>,
    /// The selection context of the centre zone.
    summary: StatusSummary,
    /// The projection the active view is drawing with.
    projection: Projection,
    /// Whether this frame evaluates the scene, which the right zone reports.
    rebuilding: bool,
}

/// The selection context, built once per frame from the selection the editor already resolved.
///
/// Every field is optional so a new parameter is one more field and one more chip, never a second
/// row. Derived values are read from the selected node alone: the summary never walks the tree and
/// never resolves a panel's geometry, which is the card's job.
#[derive(Clone, Debug, Default, PartialEq)]
struct StatusSummary {
    /// How many nodes the selection holds.
    count: usize,
    /// The selected node's name; absent for a multi-selection or an empty one.
    name: Option<String>,
    /// The node kind, such as *panel* or *component*.
    kind: Option<String>,
    /// A panel's nominal length, width and thickness.
    size: Option<String>,
    /// A panel's material name.
    material: Option<String>,
    /// The edge-banding thickness, when the panel carries a band.
    banding: Option<String>,
    /// The first problem the selection would report.
    warning: Option<StatusWarning>,
}

/// One warning the selection carries, drawn as a chip in the status bar.
#[derive(Clone, Debug, PartialEq)]
struct StatusWarning {
    /// The sentence the chip shows.
    text: String,
    /// What the user can do about it, shown on hover.
    hint: Option<String>,
}

impl StatusSummary {
    /// Builds the context of `selection`, whose primary is the last id.
    ///
    /// The editor hands in the selection it already resolved, so this reads the selected nodes only:
    /// no tree walk, no geometry solve and no second look at the document. The warning is the
    /// cheapest one to know — the editor's lock flag and a catalogue lookup; an outline that does not
    /// resolve needs the panel's geometry, which the inspector already computes.
    fn of(project: &Project, directory: &Directory, selection: &[Id], locked: Option<Id>, lang: Lang) -> Self {
        if selection.is_empty() {
            return Self::default();
        }
        let primary = selection.last().copied();
        let mut summary = Self { count: selection.len(), ..Self::default() };
        let Some(node) = primary.and_then(|id| project.node(id)) else {
            return summary;
        };
        // The warning is the primary's in either case; name, size and material describe one node,
        // so a multi-selection carries the count and stops there.
        summary.warning = warning_of(directory, node, locked, lang);
        if summary.count > 1 {
            return summary;
        }
        summary.name = Some(node.name().to_owned());
        match node {
            Node::Panel(panel) => {
                summary.kind = Some(Message::ToolPanel.text(lang).to_owned());
                let nominal = panel.nominal_size();
                summary.size = Some(
                    Message::StatusSize.with(lang, [nominal.length(), nominal.width(), panel.last_known_thickness()]),
                );
                summary.material =
                    panel.material().and_then(|id| directory.board(id)).map(|material| material.name.to_owned());
                summary.banding = panel.edge_banding().first().map(|band| {
                    let thickness = directory
                        .banding(band.material())
                        .map_or_else(|| band.last_known_thickness(), |material| material.thickness);
                    Message::StatusBanding.with(lang, [thickness])
                });
            }
            Node::Container(container) => {
                summary.kind = Some(
                    if container.is_component() {
                        Message::ContainerRoleComponent
                    } else {
                        Message::ContainerRoleGroup
                    }
                    .text(lang)
                    .to_owned(),
                );
            }
            Node::Object(object) => {
                summary.kind = Some(
                    match object.geometry() {
                        Geometry::Mesh(_) => Message::PrimitiveBadgeMesh,
                        Geometry::Constructive(_) => Message::PrimitiveBadgeConstructive,
                        Geometry::Primitive(_) => Message::PrimitiveBadgePrimitive,
                        Geometry::Path(_) => Message::PrimitiveBadgePath,
                        Geometry::Contours(_) => Message::PrimitiveBadgeContours,
                    }
                    .text(lang)
                    .to_owned(),
                );
            }
            Node::Camera(_) => summary.kind = Some(Message::ToolCamera.text(lang).to_owned()),
            Node::Light(_) => summary.kind = Some(Message::ToolLight.text(lang).to_owned()),
        }
        summary
    }

    /// Whether the zone has anything to draw.
    fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// The line the centre zone draws, or `None` when nothing is selected.
    fn line(&self, lang: Lang) -> Option<String> {
        // A multi-selection has no single name, size or material to report: the count is the one
        // fact that describes it without pretending it is one node.
        if self.count > 1 {
            return Some(Message::StatusSelectionCount.with(lang, [self.count]));
        }
        let parts: Vec<&str> = [&self.name, &self.kind, &self.size, &self.material, &self.banding]
            .into_iter()
            .flatten()
            .map(String::as_str)
            .collect();
        (!parts.is_empty()).then(|| parts.join(SUMMARY_SEPARATOR))
    }
}

/// The first problem of one selected node: a lock, then a missing panel material.
///
/// Every check is a flag or a catalogue lookup, so the frame path stays free of derived geometry.
/// A missing edge material or an unresolved outline is a panel diagnostic: it needs the resolved
/// panel, which the inspector computes for its own section and the bar does not repeat per frame.
fn warning_of(directory: &Directory, node: &Node, locked: Option<Id>, lang: Lang) -> Option<StatusWarning> {
    if locked == Some(node.id()) {
        return Some(StatusWarning {
            text: Message::LockedStatus.with(lang, [node.name()]),
            hint: Some(Message::UnlockHint.text(lang).to_owned()),
        });
    }
    let Node::Panel(panel) = node else {
        return None;
    };
    match panel.material() {
        None => Some(StatusWarning { text: Message::DiagNoMaterial.text(lang).to_owned(), hint: None }),
        Some(material) if directory.board(material).is_none() => {
            Some(StatusWarning { text: Message::DiagPanelMaterialMissing.with(lang, [material]), hint: None })
        }
        Some(_) => None,
    }
}

/// A quiet hint that truncates instead of running under the right-hand view controls.
///
/// `reserve` is the width the row still has to draw after this line — the centre's own context — so
/// the hint gives way to it: a help line is the first thing the row may elide, a node's name is not
/// (X94).
fn hint(ui: &mut egui::Ui, text: &str, reserve: f32) {
    line(ui, egui::RichText::new(text).color(theme::text_dim(ui)), reserve);
}

/// One truncating line of the command zone in its own style, leaving `reserve` points to its right.
///
/// The line is drawn in a scope narrowed to the room it may take, because egui truncates a label to
/// the width of the layout it sits in and not to the width the row wants left over.
fn line(ui: &mut egui::Ui, text: egui::RichText, reserve: f32) {
    let available = ui.available_rect_before_wrap();
    let room = (available.width() - reserve).max(0.0);
    let rect = egui::Rect::from_min_size(available.min, egui::vec2(room, available.height()));
    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
        ui.add(egui::Label::new(text).truncate());
    });
}

/// The armed tool, the gesture it expects and the counts it would affect.
///
/// Text only: every control this zone used to draw is now on the surface that owns it (U115), and
/// the row is what a glance reads while the hand works. `center` is the room the centre zone needs:
/// a line the command zone draws elides to what is left of that room, so the two never paint under
/// one another (X94).
fn status_command_ui(ui: &mut egui::Ui, info: &StatusInfo, center: f32) {
    let lang = ui::language(ui.ctx());
    if let Some(error) = &info.error {
        line(ui, egui::RichText::new(error).color(theme::danger(ui)), center);
        theme::divider(ui);
    } else if let Some(notice) = &info.notice {
        // A workspace that fell back to its defaults is a fact, not a failure: it uses the quiet
        // text colour and keeps the rest of the bar usable.
        hint(ui, &notice.text(lang), center);
        theme::divider(ui);
    } else if info.fitted {
        // A camera that moved with no hand on it says why (X119). The fit is a fact of the last few
        // seconds and not a failure, so it takes the quiet colour too, and the armed tool's own
        // sentence still follows it on the same row.
        hint(ui, Message::StatusFitted.text(lang), center);
        theme::divider(ui);
    }
    if let Some((segments, dirty, along)) = info.contour {
        // The working copy is the viewport's while it runs: the row says how much of the ring it
        // holds and whether it moved, and the accept and the drop are the card's one row (U115).
        let mut summary = Message::counted(
            segments,
            Message::ContourEditStatusOne,
            Message::ContourEditStatusFew,
            Message::ContourEditStatusMany,
        )
        .text(lang);
        if dirty {
            summary.push_str(Message::ContourModifiedSuffix.text(lang));
        }
        ui.label(summary);
        // The hint is the one the card's switch names, so the row and the card never tell the hand
        // two different things about the drag it is in the middle of (U115).
        hint(ui, if along { Message::ContourAlongHint } else { Message::ContourFreeHint }.text(lang), center);
        return;
    }
    if let Some((tool, points, arc_mode)) = info.drawing {
        let points_suffix = if points == 0 {
            String::new()
        } else {
            Message::counted(points, Message::PointsSuffixOne, Message::PointsSuffixFew, Message::PointsSuffixMany)
                .text(lang)
        };
        let kind_suffix = if tool.closes() {
            if arc_mode { Message::DrawArcSuffix.text(lang) } else { Message::DrawLineSuffix.text(lang) }
        } else {
            ""
        };
        ui.label(Message::DrawStatus.with(lang, [tool.label().text(lang), &points_suffix, kind_suffix]));
        // The draft's own *Close*, *Finish* and *Cancel* are the ribbon's Draft group and the card's
        // one row: a second set of buttons here competed with the count beside them (U115).
        hint(ui, Message::DrawHint.text(lang), center);
        if let Some(reason) = &info.preview_error {
            hint(ui, reason, center);
        }
        return;
    }
    if let Some((stage, _)) = info.measure {
        // The left zone says the step and the centre zone the number: the person reads one line
        // while the hand works, and the value never hides the instruction (ADR 0118).
        let words = match stage {
            MeasureStage::First => Message::MeasureFirstPoint,
            MeasureStage::Second => Message::MeasureSecondPoint,
            MeasureStage::Measured => Message::MeasureAgain,
        };
        hint(ui, words.text(lang), center);
        return;
    }
    if let Some(words) = info.area {
        // The pull is a read with one click: the left zone says what the pointer rests on, so a face
        // that cannot be pulled says so before the click instead of after it (G4).
        hint(ui, words.text(lang), center);
        return;
    }
    if info.void.is_some() {
        // The niche is a read with no step and no button: the left zone says what to point at and
        // the centre zone names the space, so the two never compete for the row (ADR 0118).
        hint(ui, Message::VoidHint.text(lang), center);
        return;
    }
    if let Some(mode) = info.navigation {
        // Escape ends the gesture, so the row names it and the sentence beside it says so: a *Done*
        // button would be the third way out of a mode the key already leaves (U115).
        ui.label(Message::NavigationStatus.with(lang, [mode.name().text(lang)]));
        hint(ui, mode.gesture().text(lang), center);
        return;
    }
    // The transform band is the header's, beside the ribbon and panel switches, where it never
    // competes with this row (U115).
    theme::divider(ui);
    if let Some((element, count)) = info.mesh {
        let counted = match element {
            MeshElement::Vertex => Message::counted(
                count,
                Message::MeshEditStatusOne,
                Message::MeshEditStatusFew,
                Message::MeshEditStatusMany,
            ),
            MeshElement::Face => Message::counted(
                count,
                Message::MeshFaceStatusOne,
                Message::MeshFaceStatusFew,
                Message::MeshFaceStatusMany,
            ),
        };
        ui.label(counted.text(lang));
        hint(ui, Message::MeshEditHint.text(lang), center);
    } else if info.holding {
        // The piece follows the pointer rather than a button, so the left zone says what ends the
        // hold: a click puts it down and Escape puts it back (ADR 0151).
        hint(ui, Message::ObjectHold.text(lang), center);
    } else {
        hint(ui, Message::ViewportHint.text(lang), center);
    }
}

impl App {
    /// The thin bar at the bottom of the window: what the armed tool waits for, what is selected and
    /// how the window is set.
    ///
    /// It writes nothing. The transform mode, the snap and the armed navigation gesture are the
    /// window's and each is drawn by the surface that owns it — the header's band, the View tab's
    /// Snaps group and the floating cluster — so the row reads them and never changes them (U115).
    pub(crate) fn status_bar(&mut self, ui: &mut egui::Ui) {
        let notice = self.settings_notice.clone();
        let lang = ui::language(ui.ctx());
        let info =
            self.tabs.active().status_info(notice.as_ref(), self.shell.cluster.navigation, self.shell.rebuilding, lang);
        egui::Panel::bottom("status_bar").show(ui, |ui| {
            ui.set_height(theme::STATUS_HEIGHT);
            status_row_ui(ui, &info);
        });
    }
}

/// The narrowest room the row keeps for the command beside the readouts, in points.
///
/// The command zone is text alone now that every control has left the row (U115): what it keeps is
/// enough for the armed tool's own sentence, and the readouts take the rest of the row's width up to
/// this floor. X94 measured the three zones against the narrowest window the shell opens.
const COMMAND_ZONE_MIN: f32 = 240.0;

/// The width of the row's two outer zones: the readouts take the room they measure, and the command
/// keeps the rest, never less than [`COMMAND_ZONE_MIN`] while the row has it.
///
/// The centre has no width of its own: it elides into what the command leaves inside its zone, so a
/// long node name never pushes the readouts off the row.
fn zone_widths(row: f32, readouts: f32) -> (f32, f32) {
    let right = readouts.max(0.0).min((row - COMMAND_ZONE_MIN).max(0.0));
    ((row - right).max(0.0), right)
}

/// The width the right zone's readouts want, measured on a hidden pass of the same widgets.
///
/// The zone is sized by its own content rather than by a share of the row, so the snap, the
/// projection and the scale are never cut on a window somebody works in. The probe is a hidden ui
/// clipped to nothing: it paints nothing, takes no click and keeps its galley out of the frame, so
/// the only cost is one more layout of four labels (X94).
fn readouts_width(ui: &mut egui::Ui, info: &StatusInfo) -> f32 {
    let probe =
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(ui.available_width().max(0.0), theme::STATUS_HEIGHT));
    let mut probe_ui = ui.new_child(
        egui::UiBuilder::new()
            .id_salt("status_readouts")
            .max_rect(probe)
            .layout(egui::Layout::left_to_right(egui::Align::Center))
            .invisible(),
    );
    // A clip rect with no area is the one way to keep a measured widget out of the frame: a hidden
    // ui still hands its shapes to the backend, and only an empty clip keeps them out of the list a
    // test reads and the tessellator draws.
    probe_ui.set_clip_rect(egui::Rect::NOTHING);
    status_right_ui(&mut probe_ui, info);
    probe_ui.min_rect().width()
}

/// The width the centre's context wants, measured the same hidden way and in the same order.
///
/// The command keeps the rest of its zone, so the hint beside the selection elides first: the row
/// reports what is selected before it explains how to navigate (X94).
fn center_width(ui: &mut egui::Ui, info: &StatusInfo, lang: Lang) -> f32 {
    let probe =
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(ui.available_width().max(0.0), theme::STATUS_HEIGHT));
    let mut probe_ui = ui.new_child(
        egui::UiBuilder::new()
            .id_salt("status_center")
            .max_rect(probe)
            .layout(egui::Layout::left_to_right(egui::Align::Center))
            .invisible(),
    );
    probe_ui.set_clip_rect(egui::Rect::NOTHING);
    status_center_zone_ui(&mut probe_ui, info, lang);
    probe_ui.min_rect().width()
}

/// The centre zone: the ruler's live length, the niche's cell or the selection's context.
///
/// The three are one zone rather than three: a frame reports one of them, and the divider that
/// separates the zone from the command is drawn by whichever one reports.
fn status_center_zone_ui(ui: &mut egui::Ui, info: &StatusInfo, lang: Lang) {
    if let Some((stage, value)) = info.measure {
        if let Some(length) = value {
            theme::divider(ui);
            status_measure_ui(ui, stage, length, lang);
        }
    } else if let Some(void) = &info.void {
        theme::divider(ui);
        status_void_ui(ui, void, lang);
    } else if !info.summary.is_empty() {
        theme::divider(ui);
        status_center_ui(ui, &info.summary, lang);
    }
}

/// The one row in three zones: the command on the left, the selection in the middle and the
/// window's display state on the right.
///
/// Each zone owns one rectangle and paints inside it only: a widget that does not fit is cut at the
/// boundary instead of running under the zone beside it, and the right zone is measured from its own
/// readouts, so the row cannot be pushed off the window's edge by the zone before it (X94). The
/// centre elides into what the command leaves, so an active tool never loses its sentence to a long
/// node name.
fn status_row_ui(ui: &mut egui::Ui, info: &StatusInfo) {
    let lang = ui::language(ui.ctx());
    let height = theme::STATUS_HEIGHT;
    let row = ui.available_rect_before_wrap();
    let row = egui::Rect::from_min_size(row.min, egui::vec2(row.width().max(0.0), height));
    let (command_width, readouts_width) = zone_widths(row.width(), readouts_width(ui, info));
    let center_room = center_width(ui, info, lang);
    let command_zone = egui::Rect::from_min_size(row.min, egui::vec2(command_width, height));
    let readouts_zone = egui::Rect::from_min_size(
        egui::pos2(row.right() - readouts_width, row.top()),
        egui::vec2(readouts_width, height),
    );
    ui.scope_builder(
        egui::UiBuilder::new().max_rect(command_zone).layout(egui::Layout::left_to_right(egui::Align::Center)),
        |ui| {
            ui.set_clip_rect(ui.clip_rect().intersect(command_zone));
            status_command_ui(ui, info, center_room);
            status_center_zone_ui(ui, info, lang);
        },
    );
    ui.scope_builder(
        egui::UiBuilder::new().max_rect(readouts_zone).layout(egui::Layout::left_to_right(egui::Align::Center)),
        |ui| {
            ui.set_clip_rect(ui.clip_rect().intersect(readouts_zone));
            status_right_ui(ui, info);
        },
    );
}

/// The centre zone while the ruler runs: the live length the atom holds.
///
/// One label, so the number is the loudest thing on the row while the left zone keeps the
/// instruction. A fixed value carries the "click for a new measurement" hint, so the two stages
/// are told apart without a second style for a number.
fn status_measure_ui(ui: &mut egui::Ui, stage: MeasureStage, length: Length, lang: Lang) {
    let value = Message::MeasureValue.with(lang, [length]);
    let response = ui.label(egui::RichText::new(&value).strong());
    if stage == MeasureStage::Measured {
        response.on_hover_text(Message::MeasureAgain.text(lang));
    }
}

/// The centre zone while the niche tool runs: the cell under the pointer, or why it has no name.
///
/// One label, like the ruler's: the three sides of the cell the fill tools, the ghost and this row
/// all read from one value (ADR 0118), and a pointer that names no space says so instead of leaving
/// the row empty.
fn status_void_ui(ui: &mut egui::Ui, void: &Result<[Length; 3], VoidRefusal>, lang: Lang) {
    let text = match void {
        Ok([length, width, height]) => {
            Message::VoidSize.with(lang, [Message::StatusSize.with(lang, [length, width, height])])
        }
        Err(refusal) => refusal.words().text(lang).to_owned(),
    };
    ui.label(egui::RichText::new(text).strong());
}

/// The centre zone: the selection's context, elided to the width it is given.
///
/// One label rather than a row of chips, so the whole line truncates with one ellipsis and one
/// tooltip: a `Label::truncate` keeps the full line in its hover text, which is where the parts that
/// did not fit stay readable.
///
/// A locked selection refuses the gizmo and the destructive commands, so the chip that names it is
/// drawn first, before the context that reads as its own line (ADR 0065).
fn status_center_ui(ui: &mut egui::Ui, summary: &StatusSummary, lang: Lang) {
    if let Some(warning) = &summary.warning {
        warning_chip(ui, warning);
        ui.add_space(theme::STEP);
    }
    let Some(line) = summary.line(lang) else {
        return;
    };
    ui.add(egui::Label::new(egui::RichText::new(&line)).truncate()).on_hover_text(&line);
}

/// The right zone: the snap, the projection, the scale and whether the scene is rebuilding.
///
/// Four readouts of the window's view state, drawn left to right in the rectangle their own content
/// measured, so the state ends at the window's edge. The snap is a readout and no longer a switch:
/// its one switch is the View tab's Snaps group, filed under its own `view.snaps.grid` row, and the
/// key `G` reaches it from anywhere (U115). The line still names the step and the key, because a
/// readout that says which snap is on should say where it is turned off; the ink is the readout's
/// one state — the step is written in the primary colour while it holds and dim while it does not.
fn status_right_ui(ui: &mut egui::Ui, info: &StatusInfo) {
    let lang = ui::language(ui.ctx());
    // The readout names the kind while a snap holds the pointer and the grid's own step while none
    // does: a capture nobody can name is a jump nobody can explain (U115, U116).
    let (snap_line, snap_ink) = match info.snap {
        Some(kind) => (Message::SnapCaptured.with(lang, [ui::snap::word(kind).text(lang)]), theme::text(ui)),
        None => (
            Message::SnapTenMillimetres.text(lang).to_owned(),
            if info.grid { theme::text(ui) } else { theme::text_dim(ui) },
        ),
    };
    ui.label(egui::RichText::new(snap_line).color(snap_ink)).on_hover_text(match info.snap {
        Some(kind) => ui::snap::hint(kind).text(lang),
        None => Message::ShortcutGrid.text(lang),
    });
    theme::divider(ui);
    let projection = match info.projection {
        Projection::Perspective { .. } => Message::Perspective,
        Projection::Orthographic { .. } => Message::Orthographic,
    };
    ui.label(egui::RichText::new(projection.text(lang)).color(theme::text_dim(ui)))
        .on_hover_text(Message::ShortcutProjection.text(lang));
    theme::divider(ui);
    let scale = ui::ui_scale::percent(ui.ctx().zoom_factor());
    ui.label(egui::RichText::new(Message::StatusScale.with(lang, [scale])).color(theme::text_dim(ui)))
        .on_hover_text(Message::ScaleHint.text(lang));
    theme::divider(ui);
    let (state, colour) = if info.rebuilding {
        (Message::StatusRebuilding, theme::warn(ui))
    } else {
        (Message::StatusReady, theme::text_dim(ui))
    };
    ui.label(egui::RichText::new(state.text(lang)).color(colour));
}

/// A warning as a tinted pill, so a refusal reads as a state and not as another hint.
fn warning_chip(ui: &mut egui::Ui, warning: &StatusWarning) -> egui::Response {
    let colour = theme::warn(ui);
    let frame = egui::Frame::new()
        .fill(colour.gamma_multiply(0.18))
        .corner_radius(egui::CornerRadius::same(theme::RADIUS))
        .inner_margin(egui::Margin::symmetric(5, 1));
    let response = frame.show(ui, |ui| ui.label(egui::RichText::new(&warning.text).color(colour).small())).response;
    match &warning.hint {
        Some(hint) => response.on_hover_text(hint),
        None => response,
    }
}

impl Document {
    /// Everything the bar reads from this document.
    ///
    /// The armed navigation gesture and the busy state are the window's, so they are handed in
    /// instead of read here; everything else belongs to the scene the tab holds.
    fn status_info(
        &self,
        notice: Option<&crate::settings_store::Notice>,
        navigation: Option<NavigationMode>,
        rebuilding: bool,
        lang: Lang,
    ) -> StatusInfo {
        let project = self.session.project();
        let selection = self.editor.selected_nodes();
        let locked = self.editor.locked_selection(project);
        StatusInfo {
            error: self.last_error.clone(),
            notice: notice.cloned(),
            fitted: self.fitted_recently(),
            contour: self
                .editor
                .contour_edit()
                .map(|edit| (edit.draft().segment_count(), edit.is_dirty(), self.contour_gizmo.along())),
            drawing: self.editor.drawing().map(|draft| (draft.tool(), draft.len(), draft.arc_mode())),
            preview_error: self.draw_gizmo.preview_error().map(str::to_owned),
            measure: self.editor.measure().map(|measure| (measure.stage(), measure.value())),
            // The report row reads the same atom the fill ghost and the part are built from, never a
            // second measurement of the space under the pointer (ADR 0118).
            area: self.editor.area_pull().map(|_| self.editor.area_refusal().unwrap_or(Message::ExtrudeSelectedArea)),
            void: self.editor.void_armed().then(|| {
                self.editor.void().size().ok_or_else(|| self.editor.void().refusal().unwrap_or(VoidRefusal::Outside))
            }),
            holding: self.holding_piece(),
            grid: self.snaps().grid,
            snap: self.draw_gizmo.snap_kind().or_else(|| self.measure_gizmo.snap_kind()),
            mesh: self.editor.mesh_edit().map(|edit| {
                let count = match edit.element() {
                    MeshElement::Vertex => edit.vertices().len(),
                    MeshElement::Face => edit.faces().len(),
                };
                (edit.element(), count)
            }),
            navigation,
            summary: StatusSummary::of(project, self.session.directory(), selection, locked, lang),
            projection: self.projection(),
            rebuilding,
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;
    use crate::test_support::drawn_text;
    use engine::panel_cmds::EDGE_BANDING_SET_ID;
    use engine::{PanelEdgeBandingSet, Session};
    use model::{Container, PanelNominalSize, Placement, RectEdge};
    use testkit::{millimetres, sample_catalog, sample_edge_material_id, sample_material_id, sample_project};

    fn idle_status() -> StatusInfo {
        StatusInfo {
            error: None,
            notice: None,
            fitted: false,
            contour: None,
            drawing: None,
            preview_error: None,
            measure: None,
            void: None,
            area: None,
            holding: false,
            grid: true,
            snap: None,
            mesh: None,
            navigation: None,
            summary: StatusSummary::default(),
            projection: Projection::Perspective { fov_y: 0.9 },
            rebuilding: false,
        }
    }

    /// A headless context with the theme, the language and accessibility on.
    fn context(lang: Lang) -> egui::Context {
        let ctx = egui::Context::default();
        ui::theme::install(&ctx);
        ctx.enable_accesskit();
        ui::set_language(&ctx, lang);
        ctx
    }

    /// One frame of the row at `width`, with the boundary the command and the readouts split at.
    fn row_frame(ctx: &egui::Context, info: &StatusInfo, width: f32) -> (egui::FullOutput, f32) {
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 200.0));
        let mut boundary = 0.0;
        let output = ctx.run_ui(egui::RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| {
            ui.set_width(width);
            ui.set_height(theme::STATUS_HEIGHT);
            boundary = zone_widths(width, readouts_width(ui, info)).0;
            status_row_ui(ui, info);
        });
        (output, boundary)
    }

    /// Every node of the frame that is a control — a button, a toggle, a field — with its words.
    ///
    /// The bar's own tree is a window, containers and labels; anything else is an instrument, which
    /// after U115 the row draws none of.
    fn controls(output: &egui::FullOutput) -> Vec<(egui::accesskit::Role, Option<String>)> {
        use egui::accesskit::Role;
        let nodes =
            output.platform_output.accesskit_update.as_ref().map(|update| update.nodes.as_slice()).unwrap_or_default();
        nodes
            .iter()
            .map(|(_, node)| (node.role(), node.label().map(str::to_owned)))
            .filter(|(role, _)| !matches!(role, Role::Window | Role::GenericContainer | Role::Label | Role::TextRun))
            .collect()
    }

    /// Everything one status draws, as the words a person reads.
    fn drawn(ctx: &egui::Context, info: &StatusInfo) -> Vec<String> {
        let (output, _) = row_frame(ctx, info, 1024.0);
        let texts = drawn_text(&output.shapes);
        output.drop_without_applying_deltas();
        texts
    }

    #[test]
    fn the_status_bar_draws_one_thin_row() {
        // U115: the bar is one row of text. It asks for nothing because it has nothing to ask with.
        let ctx = context(Lang::En);
        let (output, boundary) = row_frame(&ctx, &idle_status(), 1000.0);
        let texts = drawn_text(&output.shapes);
        let controls = controls(&output);
        output.drop_without_applying_deltas();
        assert!(texts.iter().any(|text| text.contains("LMB select")), "an idle bar says how to navigate: {texts:?}");
        assert!(boundary > 0.0 && boundary < 1000.0, "the row splits into a command and a readout: {boundary}");
        assert!(controls.is_empty(), "the bar publishes labels and no control: {controls:?}");
    }

    #[test]
    fn the_bar_holds_labels_only_while_a_tool_arms_it() {
        // The Done clause of U115: an armed draft, an armed contour edit and an idle frame all draw
        // text. Every instrument left the row, so a press anywhere on it would run nothing — and the
        // accessibility tree is where that is read.
        let long = StatusSummary {
            count: 1,
            name: Some("Front".to_owned()),
            kind: Some("Panel".to_owned()),
            size: Some("600 mm × 720 mm × 18 mm".to_owned()),
            material: Some("Chipboard".to_owned()),
            ..StatusSummary::default()
        };
        let cases: [(&str, StatusInfo, &[&str]); 5] = [
            ("idle", StatusInfo { summary: long.clone(), ..idle_status() }, &["Front · Panel"]),
            (
                "a draft",
                StatusInfo { drawing: Some((DrawTool::Polyline, 3, false)), ..idle_status() },
                &["Draw · Polyline", "3 points", "LMB add point"],
            ),
            (
                "a contour edit",
                StatusInfo { contour: Some((4, true, false)), ..idle_status() },
                &["Contour edit · 4 segments", "modified"],
            ),
            ("the ruler", StatusInfo { measure: Some((MeasureStage::First, None)), ..idle_status() }, &["Measure"]),
            (
                "the niche",
                StatusInfo { void: Some(Ok([millimetres(300), millimetres(400), millimetres(500)])), ..idle_status() },
                &["Niche"],
            ),
        ];
        for (name, info, words) in &cases {
            let ctx = context(Lang::En);
            let (output, _) = row_frame(&ctx, info, 1024.0);
            let texts = drawn_text(&output.shapes);
            let controls = controls(&output);
            output.drop_without_applying_deltas();
            for word in *words {
                assert!(texts.iter().any(|text| text.contains(word)), "{name} reports `{word}`: {texts:?}");
            }
            assert!(controls.is_empty(), "`{name}` draws no button, no toggle and no band: {controls:?}");
            assert!(!texts.iter().any(|text| text.contains('✕')), "`{name}` offers no cross: {texts:?}");
            assert!(!texts.iter().any(|text| text.contains("Close")), "`{name}` offers no Close: {texts:?}");
            assert!(!texts.iter().any(|text| text.contains("Done")), "`{name}` offers no Done: {texts:?}");
        }
    }

    #[test]
    fn the_status_bar_names_the_locked_node() {
        // The gizmo and the destructive commands refuse a locked node, so the centre zone carries a
        // warning that names it and a hint that says what unlocks it (ADR 0065).
        let ctx = egui::Context::default();
        ui::theme::install(&ctx);
        let summary = StatusSummary {
            count: 1,
            name: Some("Drawer".to_owned()),
            kind: Some("Panel".to_owned()),
            warning: Some(StatusWarning {
                text: Message::LockedStatus.with(Lang::En, ["Drawer"]),
                hint: Some(Message::UnlockHint.text(Lang::En).to_owned()),
            }),
            ..StatusSummary::default()
        };
        let output = ctx.run_ui(Default::default(), |ui| status_center_ui(ui, &summary, Lang::En));
        let texts = drawn_text(&output.shapes);
        output.drop_without_applying_deltas();
        assert!(texts.iter().any(|text| text.contains("Locked · Drawer")), "the locked node names itself: {texts:?}");
        assert!(texts.iter().any(|text| text.contains("Drawer · Panel")), "and keeps its context: {texts:?}");
    }

    #[test]
    fn the_status_bar_names_the_armed_navigation_gesture() {
        let ctx = context(Lang::En);
        let info = StatusInfo { navigation: Some(NavigationMode::Orbit), ..idle_status() };
        let texts = drawn(&ctx, &info);
        assert!(texts.iter().any(|text| text.contains("Navigation · Orbit")), "the armed mode is named: {texts:?}");
        assert!(
            texts.iter().any(|text| text.contains("Drag in the viewport to orbit")),
            "the bar names the gesture the mode expects: {texts:?}"
        );
        assert!(
            texts.iter().any(|text| text.contains("Esc done")),
            "and says that Escape is the way out, which is why no button is needed: {texts:?}"
        );
    }

    /// One *Front* panel with a material and an edge band next to a plain *Shelf*.
    fn banded_session() -> Session {
        let size = PanelNominalSize::new(millimetres(600), millimetres(720)).expect("a 600 × 720 panel");
        let front = model::Panel::new(
            Id::new(2).expect("id"),
            "Front",
            size,
            Some(sample_material_id()),
            millimetres(18),
            Placement::IDENTITY,
        )
        .expect("the front panel is valid");
        let shelf =
            model::Panel::new(Id::new(4).expect("id"), "Shelf", size, None, millimetres(18), Placement::IDENTITY)
                .expect("the shelf panel is valid");
        let root =
            Container::new(Id::new(1).expect("id"), "Root", Placement::IDENTITY, vec![front.into(), shelf.into()]);
        let mut session = Session::with_directory(Project::new(root).expect("the project is valid"), sample_catalog());
        let params = serde_json::to_value(PanelEdgeBandingSet {
            panel: Id::new(2).expect("id"),
            edge: RectEdge::Bottom.id(),
            material: sample_edge_material_id(),
            included_in_size: true,
        })
        .expect("the band parameters encode");
        session.dispatch(EDGE_BANDING_SET_ID, &params).expect("the front edge is banded");
        session
    }

    #[test]
    fn the_summary_reads_only_the_resolved_selection() {
        let session = banded_session();
        let front = Id::new(2).expect("id");
        let summary = StatusSummary::of(session.project(), session.directory(), &[front], None, Lang::En);
        let line = summary.line(Lang::En).expect("a selected panel has context");
        assert_eq!(summary.count, 1);
        assert_eq!(summary.name.as_deref(), Some("Front"));
        assert_eq!(summary.kind.as_deref(), Some("Panel"));
        assert_eq!(summary.size.as_deref(), Some("600 mm × 720 mm × 18 mm"));
        assert_eq!(summary.material.as_deref(), Some("Chipboard"));
        assert_eq!(summary.banding.as_deref(), Some("edge 2 mm"));
        assert!(
            line.starts_with("Front · Panel · 600 mm × 720 mm × 18 mm · Chipboard · edge 2 mm"),
            "the centre zone names the node, its kind, size, material and band: {line}"
        );
        assert!(!line.contains("Shelf"), "an unselected sibling never reaches the bar: {line}");
        assert!(summary.warning.is_none(), "nothing is wrong with the selected panel");
    }

    #[test]
    fn a_multi_selection_reports_its_count_and_nothing_more() {
        let session = banded_session();
        let selection = [Id::new(2).expect("id"), Id::new(4).expect("id")];
        let summary = StatusSummary::of(session.project(), session.directory(), &selection, None, Lang::En);
        assert_eq!(summary.count, 2);
        assert_eq!(summary.line(Lang::En).as_deref(), Some("Selected: 2"));
        assert!(summary.name.is_none(), "no single name describes two nodes");
    }

    #[test]
    fn an_empty_selection_has_no_context() {
        let session = banded_session();
        let summary = StatusSummary::of(session.project(), session.directory(), &[], None, Lang::En);
        assert_eq!(summary, StatusSummary::default());
        assert_eq!(summary.line(Lang::En), None);
    }

    #[test]
    fn the_summary_warns_about_a_lock_before_a_missing_material() {
        let session = banded_session();
        let front = Id::new(2).expect("id");
        let locked = StatusSummary::of(session.project(), session.directory(), &[front], Some(front), Lang::En);
        let warning = locked.warning.expect("a locked node warns");
        assert_eq!(warning.text, "Locked · Front");
        assert!(warning.hint.is_some(), "the chip says what unlocks it");

        let shelf = Id::new(4).expect("id");
        let plain = StatusSummary::of(session.project(), session.directory(), &[shelf], None, Lang::En);
        assert_eq!(plain.warning.expect("a panel without a material warns").text, "No panel material is assigned.");

        // The sample project's panels carry no catalogue material either, and an unlocked node with
        // no material is still only one warning, never a list.
        let project = sample_project();
        let summary = StatusSummary::of(&project, &sample_catalog(), &[Id::new(2).expect("id")], None, Lang::En);
        assert!(summary.warning.is_some());
    }

    /// The text, elision flag and width of every drawn galley, for the elision test.
    fn drawn_galleys(shapes: &[egui::epaint::ClippedShape]) -> Vec<(String, bool, f32)> {
        fn walk(shape: &egui::Shape, out: &mut Vec<(String, bool, f32)>) {
            match shape {
                egui::Shape::Text(text) => {
                    out.push((text.galley.text().to_owned(), text.galley.elided, text.galley.rect.width()));
                }
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        walk(shape, out);
                    }
                }
                _ => {}
            }
        }
        let mut out = Vec::new();
        for clipped in shapes {
            walk(&clipped.shape, &mut out);
        }
        out
    }

    #[test]
    fn a_long_context_elides_and_keeps_the_full_line_for_its_tooltip() {
        // `Label::truncate` draws an ellipsis and keeps the whole string in the galley, which is the
        // tooltip's text; the middle zone is the only zone that shrinks for it.
        let ctx = egui::Context::default();
        ui::theme::install(&ctx);
        let summary = StatusSummary {
            count: 1,
            name: Some("A very long cabinet name that cannot fit the middle zone at all".to_owned()),
            kind: Some("Panel".to_owned()),
            size: Some("600 mm × 720 mm × 18 mm".to_owned()),
            material: Some("Chipboard".to_owned()),
            banding: Some("edge 2 mm".to_owned()),
            warning: None,
        };
        let line = summary.line(Lang::En).expect("a line");
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(160.0, 40.0))),
            ..Default::default()
        };
        let output = ctx.run_ui(input, |ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(120.0, theme::STATUS_HEIGHT),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| status_center_ui(ui, &summary, Lang::En),
            );
        });
        let galleys = drawn_galleys(&output.shapes);
        output.drop_without_applying_deltas();
        let (text, elided, width) = galleys
            .iter()
            .find(|(text, _, _)| text == &line)
            .cloned()
            .unwrap_or_else(|| panic!("the centre zone drew the full line: {galleys:?}"));
        assert!(elided, "a line wider than its zone elides: {text}");
        assert!(width <= 121.0, "the elided line stays inside the zone: {width}");
    }

    #[test]
    fn the_right_zone_names_the_snap_step_and_the_busy_state() {
        // U115: the snap is a readout. It names its step and the key that turns it off, and the
        // press that used to flip it belongs to the View tab's Snaps group.
        let ctx = context(Lang::Ru);
        let busy = StatusInfo { rebuilding: true, ..idle_status() };
        let (output, _) = row_frame(&ctx, &busy, 1024.0);
        let texts = drawn_text(&output.shapes);
        let controls = controls(&output);
        output.drop_without_applying_deltas();
        assert!(texts.iter().any(|text| text.contains("Привязка 10 мм")), "the snap names its step: {texts:?}");
        assert!(texts.iter().any(|text| text.contains("Перестроение…")), "a rebuilding frame says so: {texts:?}");
        assert!(texts.iter().any(|text| text.contains("Перспективная")), "the projection is named: {texts:?}");
        assert!(controls.is_empty(), "the snap is a readout, not a switch: {controls:?}");

        let idle = StatusInfo { rebuilding: false, ..idle_status() };
        let public = drawn(&ctx, &idle);
        assert!(public.iter().any(|text| text.contains("Готово")), "an idle frame reports ready: {public:?}");
    }

    #[test]
    fn the_snap_readout_names_the_kind_that_captured_the_pointer() {
        // U116: a capture nobody can name is a jump nobody can explain, so the readout takes the
        // kind's own word while a snap holds the pointer and the grid's step while none does.
        let ctx = context(Lang::En);
        let info = StatusInfo { snap: Some(model::SnapKind::Intersection), ..idle_status() };
        let (output, _) = row_frame(&ctx, &info, 1024.0);
        let texts = drawn_text(&output.shapes);
        output.drop_without_applying_deltas();
        assert!(texts.iter().any(|text| text == "Snap: Crossing"), "the readout names the kind: {texts:?}");
        assert!(
            !texts.iter().any(|text| text.contains("Snap 10 mm")),
            "and it does not name the step as well: {texts:?}"
        );
    }

    #[test]
    fn the_snap_readout_is_quiet_while_the_snap_is_off() {
        // The ink is the readout's one state: the step is written in the primary colour while it
        // holds and in the dim one while it does not, so the line says which state the window is in
        // without becoming a control again.
        let ctx = context(Lang::En);
        let ink = |grid: bool| {
            let info = StatusInfo { grid, ..idle_status() };
            let (output, _) = row_frame(&ctx, &info, 1024.0);
            let colour = drawn_text_colours(&output.shapes)
                .into_iter()
                .find(|(text, _)| text == Message::SnapTenMillimetres.text(Lang::En))
                .map(|(_, colour)| colour);
            output.drop_without_applying_deltas();
            colour.expect("the snap readout is drawn")
        };
        let (on, off) = (ink(true), ink(false));
        assert_ne!(on, off, "the two states are told apart");
    }

    /// The colour of every drawn galley with its text, in paint order.
    fn drawn_text_colours(shapes: &[egui::epaint::ClippedShape]) -> Vec<(String, egui::Color32)> {
        fn walk(shape: &egui::Shape, out: &mut Vec<(String, egui::Color32)>) {
            match shape {
                egui::Shape::Text(text) => {
                    let colour =
                        text.galley.job.sections.first().map_or(egui::Color32::WHITE, |section| section.format.color);
                    out.push((text.galley.text().to_owned(), colour));
                }
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        walk(shape, out);
                    }
                }
                _ => {}
            }
        }
        let mut out = Vec::new();
        for clipped in shapes {
            walk(&clipped.shape, &mut out);
        }
        out
    }

    #[test]
    fn the_bar_draws_its_three_zones_in_a_narrow_window() {
        // The 1024 px window the owner checks by eye: the command, the selection and the display
        // state all draw, with no zone clipped away by another.
        let ctx = context(Lang::En);
        let summary = StatusSummary {
            count: 1,
            name: Some("Front".to_owned()),
            kind: Some("Panel".to_owned()),
            ..StatusSummary::default()
        };
        let info = StatusInfo { summary, ..idle_status() };
        let (output, boundary) = row_frame(&ctx, &info, 1024.0);
        let texts = drawn_text(&output.shapes);
        let crossing = crossings(&output.shapes, boundary);
        output.drop_without_applying_deltas();
        assert!(texts.iter().any(|text| text.contains("Front · Panel")), "the centre draws: {texts:?}");
        assert!(texts.iter().any(|text| text.contains("Snap 10 mm")), "the right draws: {texts:?}");
        assert!(texts.iter().any(|text| text.contains("LMB select")), "the left draws its hint: {texts:?}");
        assert!(crossing.is_empty(), "no zone paints under another at the default window: {crossing:?}");
    }

    /// The painted part of every shape that runs across `boundary`, with the zone it was clipped to.
    ///
    /// A shape's geometry may reach past a zone; what must not is the part that is painted, which is
    /// the shape intersected with the clip rect its zone gave it.
    fn crossings(shapes: &[egui::epaint::ClippedShape], boundary: f32) -> Vec<(egui::Rect, egui::Rect)> {
        let mut crossing = Vec::new();
        for clipped in shapes {
            let painted = clipped.shape.visual_bounding_rect().intersect(clipped.clip_rect);
            if !painted.is_positive() {
                continue;
            }
            if painted.left() < boundary && painted.right() > boundary {
                crossing.push((painted, clipped.clip_rect));
            }
        }
        crossing
    }

    #[test]
    fn no_zone_paints_under_another_in_the_narrowest_window() {
        // X94: the row is three zones with three rectangles. The narrowest window with a tool armed
        // is where they used to run into one another — the contour's buttons and the draft's painted
        // past the split, and the readouts were pushed off the window's edge by the row's own
        // cursor. Every shape's painted part has to stay on its own side of the boundary.
        assert_eq!(
            zone_widths(640.0, 5_000.0).0,
            COMMAND_ZONE_MIN,
            "the narrowest window leaves the command its floor and no more"
        );
        let long = StatusSummary {
            count: 1,
            name: Some("A very long cabinet name that cannot fit the middle zone at all".to_owned()),
            kind: Some("Panel".to_owned()),
            size: Some("600 mm × 720 mm × 18 mm".to_owned()),
            material: Some("Chipboard".to_owned()),
            banding: Some("edge 2 mm".to_owned()),
            warning: None,
        };
        let cases: [(&str, StatusInfo); 3] = [
            ("a long selection", StatusInfo { summary: long.clone(), ..idle_status() }),
            ("a contour edit", StatusInfo { contour: Some((4, true, false)), summary: long.clone(), ..idle_status() }),
            (
                "an armed draft",
                StatusInfo { drawing: Some((DrawTool::Polyline, 3, false)), summary: long.clone(), ..idle_status() },
            ),
        ];
        for lang in [Lang::En, Lang::Ru] {
            for (name, info) in &cases {
                let ctx = context(lang);
                let (output, boundary) = row_frame(&ctx, info, 640.0);
                let crossing = crossings(&output.shapes, boundary);
                output.drop_without_applying_deltas();
                assert!(
                    crossing.is_empty(),
                    "{name} in {lang:?} paints across the zones' boundary at {boundary}: {crossing:?}"
                );
            }
        }
    }
}
