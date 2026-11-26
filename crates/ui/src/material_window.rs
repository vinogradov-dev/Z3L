//! The material window: the workshop's render materials, and the one record being edited.
//!
//! A material editor is a tool a person keeps open beside the picture, not a page of the workspace
//! settings: the window is opened from the render place, it floats over the work, and it can be moved
//! out of the way of the model a decor is chosen for (ADR 0251). The material editors of the
//! renderers a shop knows set the bar — the materials listed on the left, the chosen record's
//! channels on the right, a swatch that shows the decor at its own scale and turn — and nothing of
//! their look is copied (`AGENTS.md` §4.6).
//!
//! The window is a view of one library and a draft of one record. It writes no file and mutates no
//! document: a settled edit becomes one [`Act`] the shell performs — the atomic write of
//! `library.json` through the library's own store, and the snapshot every open tab then derives its
//! frame from (`app/src/materials_watch.rs`). The numbers are the crate's own: `Scrub` offers the
//! ranges `materials` validates, so the page cannot build a record the crate refuses except by the
//! one rule a pair of switches states — a record with neither a base nor a coat is no surface at all.
//!
//! Deleting is allowed even when objects name the record: they keep the reference, the frame draws
//! them with the default material, and the id they still name is listed in the danger ink so a shop
//! can put a record back under it (ADR 0251).
//!
//! Changes when: what a record is made of changes (ADR 0243), or where a material is edited moves.

use std::collections::{BTreeMap, HashMap};
use std::ops::RangeInclusive;

use egui::{Align2, Color32, Context, FontId, Id, Rect, Response, RichText, Sense, Stroke, StrokeKind, TextureHandle};
use egui::{Ui, Window, pos2};
use materials::{Base, Coat, Decor, Library, MAX_ABSORPTION, MAX_IOR, Material, MaterialError, MaterialId, Turns};
use text::{Lang, Message};
use units::Length;

use crate::scrub::Scrub;
use crate::theme;

/// The window's first width in points: the list, the fields and room for a number.
pub const WIDTH: f32 = 760.0;

/// The window's first height in points.
pub const HEIGHT: f32 = 600.0;

/// The narrowest the window may be dragged, so a field keeps its unit beside it.
pub const MIN_WIDTH: f32 = 620.0;

/// The list column's width, in points: a name and the record's number.
const LIST_WIDTH: f32 = 200.0;

/// One row of the list, in points.
const ROW_HEIGHT: f32 = 20.0;

/// The swatch's size in points, where a decor's scale and turn are read.
const SWATCH: egui::Vec2 = egui::vec2(240.0, 150.0);

/// How many millimetres of board the swatch shows, so a decor's coverage has a scale to be seen at.
///
/// A door is about 300 mm wide, which is the size a maker reads a grain at; the number belongs to the
/// swatch and is never a document value.
const SWATCH_MM: f32 = 300.0;

/// How long a settled edit waits before it is written, in seconds.
///
/// The same pause the one number field waits for (ADR 0092): a dragged roughness is one write when
/// the hand stops rather than forty while it moves, and a name typed letter by letter is one record.
pub const COMMIT_SECONDS: f64 = 0.4;

/// The smallest coverage a decor may name, in millimetres.
pub const MIN_COVERS_MM: u32 = 1;

/// The largest coverage a decor may name, in millimetres.
///
/// A picture repeating over ten metres is a picture nobody reads, and a field needs two ends.
pub const MAX_COVERS_MM: u32 = 10_000;

/// The coverage a record starts from, in millimetres: a board's own repeat, which is where a shop
/// starts before it measures a scan.
pub const DEFAULT_COVERS_MM: u32 = 600;

/// The one number field's width inside the window, in points: three of them share a row.
const FIELD_WIDTH: f32 = 52.0;

/// What the window asks the shell to do, once per act.
#[derive(Clone, Debug, PartialEq)]
pub enum Act {
    /// File this record: the one its id names, or a new one carrying a free id.
    Set(Box<Material>),
    /// Remove the record; the documents that name it keep the reference (ADR 0251).
    Remove(MaterialId),
    /// Ask the person for a decor picture; the shell copies it beside the library and answers with
    /// the path a record carries.
    Pick(MaterialId),
}

/// The picture the record being edited is faced with, as the shell decoded it.
///
/// `ui` never touches a path: the shell reads the PNG through `materials::Store` and hands its texels
/// here, where the swatch tiles them at the record's own coverage and turn.
#[derive(Clone, Copy)]
pub struct Preview<'a> {
    /// The record the picture belongs to.
    pub material: MaterialId,
    /// The decor's path, so a scan a person replaced is a new picture rather than a cached one.
    pub picture: &'a str,
    /// The texels, row-major from the top, eight-bit RGBA.
    pub image: &'a egui::ColorImage,
}

/// What the open documents say about the library: who names each record, and which ids they name
/// that the library no longer holds.
///
/// The shell derives it from the documents and refreshes it when a revision moves, because a frame
/// never walks a tree (`AGENTS.md` §2). `ui` reads it and changes nothing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Use {
    /// The node names that carry each record, in tree order, tab by tab.
    pub named: BTreeMap<MaterialId, Vec<String>>,
    /// The ids a document names that the library does not hold.
    pub missing: Vec<MaterialId>,
}

impl Use {
    /// The nodes that name `material`, or an empty list.
    fn of(&self, material: MaterialId) -> &[String] {
        self.named.get(&material).map(Vec::as_slice).unwrap_or_default()
    }
}

/// The window itself: whether it is up, the record in the fields and the sentence it shows.
#[derive(Default)]
pub struct MaterialWindow {
    open: bool,
    /// The record the fields hold; `None` while the list has nothing chosen.
    editing: Option<Editing>,
    /// The last refusal, or what the shell said a delete did.
    notice: Option<String>,
    /// Whether the fields hold an edit that has not been written yet.
    dirty: bool,
    /// When the fields were last touched, on the frame's clock.
    touched: Option<f64>,
    /// Whether the next act is written at once, without waiting for the pause (a switch, a picture).
    commit_now: bool,
    /// The pictures uploaded for the swatch, keyed by the record and the path they were read from.
    textures: HashMap<(u32, String), TextureHandle>,
}

impl MaterialWindow {
    /// A window that is not up.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the window is up.
    pub const fn is_open(&self) -> bool {
        self.open
    }

    /// Opens the window, keeping the record it was last left on.
    pub const fn open(&mut self) {
        self.open = true;
    }

    /// Closes the window.
    pub const fn close(&mut self) {
        self.open = false;
    }

    /// The sentence the shell shows under the fields: a write that refused, or a library that did.
    pub fn refused(&mut self, sentence: impl Into<String>) {
        self.notice = Some(sentence.into());
    }

    /// The sentence the window shows, for a shell test that reads what the person reads.
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    /// The record landed in the library: the draft is no longer a record being made.
    pub fn filed(&mut self, material: MaterialId) {
        self.notice = None;
        if let Some(editing) = self.editing.as_mut()
            && editing.id == material
        {
            editing.fresh = false;
        }
    }

    /// The picture the shell copied into the library's folder, named as the record carries it.
    ///
    /// The decor is switched on with it: a person who chose a scan means the record to be faced with
    /// it, and the act that names it is written at once.
    pub fn picture(&mut self, path: impl Into<String>) {
        let Some(editing) = self.editing.as_mut() else {
            return;
        };
        editing.picture = path.into();
        editing.decor = true;
        self.dirty = true;
        self.commit_now = true;
    }

    /// Removes a record deleted elsewhere, so the fields stop showing it.
    pub fn forget(&mut self, material: MaterialId) {
        if self.editing.as_ref().is_some_and(|editing| editing.id == material) {
            self.editing = None;
            self.dirty = false;
            self.commit_now = false;
        }
    }

    /// The record the fields hold, if any, so a shell test can name what is on screen.
    pub fn editing(&self) -> Option<MaterialId> {
        self.editing.as_ref().map(|editing| editing.id)
    }

    /// The act the settled draft is worth, if it is due.
    fn settle(&mut self, now: f64, force: bool) -> Option<Act> {
        if !self.dirty {
            return None;
        }
        if !force && now - self.touched.unwrap_or(now) < COMMIT_SECONDS {
            return None;
        }
        let editing = self.editing.as_ref()?;
        self.dirty = false;
        match editing.record() {
            Ok(material) => Some(Act::Set(Box::new(material))),
            Err(refusal) => {
                self.notice = Some(refusal.to_string());
                None
            }
        }
    }

    /// The texture the swatch draws, uploading the picture the first time it is shown.
    fn texture(&mut self, ctx: &Context, preview: Option<Preview<'_>>) -> Option<TextureHandle> {
        let preview = preview?;
        let key = (preview.material.get(), preview.picture.to_owned());
        if let Some(texture) = self.textures.get(&key) {
            return Some(texture.clone());
        }
        let texture = ctx.load_texture(
            format!("decor-{}-{}", preview.material.get(), preview.picture),
            preview.image.clone(),
            egui::TextureOptions::LINEAR_REPEAT,
        );
        // One picture per record, and a replaced scan takes its own key: a session that edits thirty
        // materials holds thirty pictures, and the list is dropped when the window closes.
        self.textures.insert(key, texture.clone());
        Some(texture)
    }

    /// The changes the fields made this frame, applied to the window once the record is put back.
    fn absorbed(&mut self, flags: Flags) {
        self.dirty |= flags.dirty;
        self.commit_now |= flags.commit_now;
        if let Some(touched) = flags.touched {
            self.touched = Some(self.touched.map_or(touched, |known| known.max(touched)));
        }
    }
}

/// What the fields asked for this frame, kept out of the window while the record is borrowed.
#[derive(Clone, Copy, Debug, Default)]
struct Flags {
    /// A number or a name moved.
    dirty: bool,
    /// The act is written at once: a switch or a picture, never a drag.
    commit_now: bool,
    /// When the last change happened, on the frame's clock.
    touched: Option<f64>,
}

impl Flags {
    /// A typed or dragged value moved.
    fn typed(&mut self, ui: &Ui) {
        self.dirty = true;
        self.touched = Some(ui.input(|input| input.time));
    }

    /// A switch was pressed: one act, written now.
    fn switched(&mut self, ui: &Ui) {
        self.typed(ui);
        self.commit_now = true;
    }
}

/// Draws the window and returns the one act it is due, if any.
///
/// The shell performs the act after this frame's drawing: it writes the library through its store,
/// swaps the snapshot into every tab and answers a picture request. Nothing here reads a path or
/// writes a file.
pub fn show(
    ctx: &Context,
    window: &mut MaterialWindow,
    library: &Library,
    used: &Use,
    preview: Option<Preview<'_>>,
) -> Option<Act> {
    if !window.open {
        return None;
    }
    let lang = crate::language(ctx);
    // A record removed while the window was closed leaves nothing to edit.
    if let Some(editing) = window.editing.as_ref()
        && !editing.fresh
        && library.get(editing.id).is_none()
    {
        window.editing = None;
    }
    let mut open = true;
    let mut act = None;
    Window::new(Message::MaterialsTitle.text(lang))
        .id(Id::unique("material_window"))
        .open(&mut open)
        .default_size(egui::vec2(WIDTH, HEIGHT))
        .min_width(MIN_WIDTH)
        .resizable(true)
        .show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(LIST_WIDTH);
                    list(ui, window, library, used, &mut act);
                });
                ui.separator();
                ui.vertical(|ui| {
                    editor(ui, window, library, used, preview, &mut act);
                });
            });
        });
    window.open = open;
    if !window.open {
        // A closed window holds no picture: the textures go with the screen that drew them.
        window.textures.clear();
        return None;
    }
    let now = ctx.input(|input| input.time);
    let force = std::mem::take(&mut window.commit_now);
    let act = act.or_else(|| window.settle(now, force));
    if window.dirty {
        // The pause has to end without the pointer moving, or a typed name is never written.
        let waited = now - window.touched.unwrap_or(now);
        let left = (COMMIT_SECONDS - waited).max(0.0);
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(left));
    }
    act
}

/// The left column: the library's records, the missing ids, and the two buttons that act on them.
fn list(ui: &mut Ui, window: &mut MaterialWindow, library: &Library, used: &Use, act: &mut Option<Act>) {
    let lang = crate::language(ui.ctx());
    ui.horizontal(|ui| {
        if ui.button(Message::MaterialNew.text(lang)).clicked() {
            match library.next_id() {
                Some(id) => {
                    window.editing = Some(Editing::new(id, lang));
                    window.dirty = true;
                    window.commit_now = true;
                    window.notice = None;
                }
                None => window.refused(Message::MaterialNoFreeId.text(lang)),
            }
        }
        let can_delete = window.editing.is_some();
        let delete = ui.add_enabled(can_delete, egui::Button::new(Message::MaterialDelete.text(lang)));
        if delete.on_hover_text(Message::MaterialDeleteHint.text(lang)).clicked()
            && let Some(editing) = window.editing.take()
        {
            *act = Some(Act::Remove(editing.id));
            window.dirty = false;
            window.commit_now = false;
        }
    });
    ui.add_space(theme::STEP);
    if library.is_empty() {
        ui.label(RichText::new(Message::MaterialsEmpty.text(lang)).color(theme::text_dim(ui)));
    }
    let room = (ui.available_height() - ROW_HEIGHT).max(ROW_HEIGHT);
    egui::ScrollArea::vertical().id_salt("material.list").max_height(room).show(ui, |ui| {
        for record in library.records() {
            row(ui, window, record, used, act);
        }
        // A record the documents still name and the library no longer holds: the object keeps the
        // reference and the frame draws the default material, so the id stands here in the danger
        // ink rather than being forgotten (ADR 0251).
        for &missing in &used.missing {
            if library.get(missing).is_some() {
                continue;
            }
            let sentence = Message::MaterialMissing.with(lang, [missing.get()]);
            let button = ui.add(egui::Button::new(RichText::new(sentence).color(theme::danger(ui))));
            if button.on_hover_text(Message::MaterialMissingCreate.text(lang)).clicked() {
                window.editing = Some(Editing::new(missing, lang));
                window.dirty = true;
                window.commit_now = true;
                window.notice = None;
            }
        }
    });
}

/// One record of the list: its colour, its name, its number and the nodes that name it.
fn row(ui: &mut Ui, window: &mut MaterialWindow, record: &Material, used: &Use, act: &mut Option<Act>) {
    let lang = crate::language(ui.ctx());
    let selected = window.editing.as_ref().is_some_and(|editing| editing.id == record.id());
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), ROW_HEIGHT), Sense::click());
    let painter = ui.painter_at(rect);
    if selected {
        painter.rect_filled(rect, theme::RADIUS, theme::accent_fill(ui));
    } else if response.hovered() {
        painter.rect_filled(rect, theme::RADIUS, theme::control_hover(ui));
    }
    let chip = Rect::from_min_size(rect.min + egui::vec2(3.0, 4.0), egui::vec2(12.0, 12.0));
    painter.rect_filled(chip, 2, record_ink(ui, record));
    painter.rect_stroke(chip, 2, Stroke::new(1.0, theme::line(ui)), StrokeKind::Inside);
    painter.text(
        pos2(chip.right() + 6.0, rect.center().y),
        Align2::LEFT_CENTER,
        record.name(),
        FontId::proportional(12.0),
        theme::text(ui),
    );
    painter.text(
        pos2(rect.right() - 4.0, rect.center().y),
        Align2::RIGHT_CENTER,
        Message::MaterialId.with(lang, [record.id().get()]),
        FontId::proportional(10.5),
        theme::text_dim(ui),
    );
    let hint: String = match used.of(record.id()) {
        [] => Message::MaterialNamedByNone.text(lang).to_owned(),
        names => Message::MaterialNamedBy.with(lang, [names.join(", ")]),
    };
    if response.on_hover_text(hint).clicked() {
        // A pending edit is written before another record takes the fields, so a click never eats a
        // number the person has just typed.
        if let Some(settled) = window.settle(ui.input(|input| input.time), true) {
            *act = Some(settled);
        }
        window.editing = Some(Editing::of(record));
        window.notice = None;
        window.dirty = false;
        window.commit_now = false;
    }
}

/// The right column: the record's name, its swatch and every channel, base and coat.
fn editor(
    ui: &mut Ui,
    window: &mut MaterialWindow,
    library: &Library,
    used: &Use,
    preview: Option<Preview<'_>>,
    act: &mut Option<Act>,
) {
    let lang = crate::language(ui.ctx());
    if window.editing.is_none() && !library.is_empty() {
        // The window opens on a record: an editor with no fields would leave a person looking for a
        // row with nothing on screen saying what a row is.
        window.editing = library.records().next().map(Editing::of);
    }
    // The record is taken out of the window while it is drawn, so the fields and the window are two
    // borrows and not one; the flags carry back what the fields changed.
    let Some(mut editing) = window.editing.take() else {
        ui.label(RichText::new(Message::MaterialsEmpty.text(lang)).color(theme::text_dim(ui)));
        return;
    };
    let texture = window.texture(ui.ctx(), preview);
    let mut flags = Flags::default();
    ui.horizontal(|ui| {
        ui.label(Message::MaterialName.text(lang));
        if ui.add(egui::TextEdit::singleline(&mut editing.name).desired_width(200.0)).changed() {
            flags.typed(ui);
        }
    });
    ui.label(RichText::new(Message::MaterialId.with(lang, [editing.id.get()])).small().color(theme::text_dim(ui)));
    ui.add_space(theme::STEP);
    swatch(ui, &editing, texture.as_ref());
    if !used.of(editing.id).is_empty() {
        let names = used.of(editing.id).join(", ");
        ui.label(RichText::new(Message::MaterialNamedBy.with(lang, [names])).small().color(theme::text_dim(ui)));
    }

    ui.add_space(theme::STEP);
    if ui.checkbox(&mut editing.base, Message::MaterialBase.text(lang)).changed() {
        flags.switched(ui);
    }
    if editing.base {
        egui::Grid::new("material.base").num_columns(2).spacing([theme::STEP * 3.0, theme::STEP]).show(ui, |ui| {
            ui.label(Message::MaterialColour.text(lang));
            ui.horizontal(|ui| {
                for channel in &mut editing.colour {
                    if number(ui, channel, 0.0..=1.0, "").changed() {
                        flags.typed(ui);
                    }
                }
            });
            ui.end_row();
            let mut dragged = |ui: &mut Ui, name: Message, value: &mut f32, range: RangeInclusive<f32>| {
                ui.label(name.text(lang));
                if number(ui, value, range, "").changed() {
                    flags.typed(ui);
                }
                ui.end_row();
            };
            dragged(ui, Message::MaterialRoughness, &mut editing.roughness, 0.0..=1.0);
            dragged(ui, Message::MaterialMetallic, &mut editing.metallic, 0.0..=1.0);
            dragged(ui, Message::MaterialIor, &mut editing.ior, 1.0..=MAX_IOR);
            dragged(ui, Message::MaterialTransmission, &mut editing.transmission, 0.0..=1.0);
            ui.label(Message::MaterialAbsorption.text(lang));
            ui.horizontal(|ui| {
                for channel in &mut editing.absorption {
                    if number(ui, channel, 0.0..=MAX_ABSORPTION, "").changed() {
                        flags.typed(ui);
                    }
                }
            });
            ui.end_row();
        });
        ui.horizontal(|ui| {
            if ui.checkbox(&mut editing.decor, Message::MaterialDecor.text(lang)).changed() {
                flags.switched(ui);
            }
            // The picture is chosen, never typed: a decor is a file the library copies beside itself
            // and names by a relative path (`materials::Decor`), so the picker is the only door.
            if ui.add_enabled(editing.decor, egui::Button::new(Message::MaterialPicture.text(lang))).clicked() {
                *act = Some(Act::Pick(editing.id));
            }
            if editing.decor && editing.picture.trim().is_empty() {
                ui.label(RichText::new(Message::MaterialPictureNone.text(lang)).color(theme::text_dim(ui)));
            } else if editing.decor {
                ui.label(RichText::new(editing.picture.as_str()).color(theme::text_dim(ui)));
            }
        });
        if editing.decor {
            ui.horizontal(|ui| {
                ui.label(Message::MaterialCovers.text(lang));
                let mut covers = editing.covers_mm;
                let field =
                    Scrub::new(&mut covers).decimals(0).speed(1.0).range(MIN_COVERS_MM..=MAX_COVERS_MM).unit("mm");
                if ui.add(field).changed() {
                    editing.covers_mm = covers;
                    flags.typed(ui);
                }
                ui.label(Message::MaterialTurns.text(lang));
                let mut turns = editing.turns;
                for quarter in 0..4u8 {
                    let label = Message::MaterialTurn.with(lang, [u32::from(quarter) * 90]);
                    if ui.radio_value(&mut turns, quarter, label).changed() {
                        editing.turns = turns;
                        flags.switched(ui);
                    }
                }
            });
        }
    }

    ui.add_space(theme::STEP);
    if ui.checkbox(&mut editing.coat, Message::MaterialCoat.text(lang)).changed() {
        flags.switched(ui);
    }
    if editing.coat {
        egui::Grid::new("material.coat").num_columns(2).spacing([theme::STEP * 3.0, theme::STEP]).show(ui, |ui| {
            let mut dragged = |ui: &mut Ui, name: Message, value: &mut f32, range: RangeInclusive<f32>, unit: &str| {
                ui.label(name.text(lang));
                if number(ui, value, range, unit).changed() {
                    flags.typed(ui);
                }
                ui.end_row();
            };
            dragged(ui, Message::MaterialWeight, &mut editing.weight, 0.0..=1.0, "");
            dragged(ui, Message::MaterialGloss, &mut editing.gloss, 0.0..=100.0, "%");
            dragged(ui, Message::MaterialCoatIor, &mut editing.coat_ior, 1.0..=MAX_IOR, "");
        });
    }
    if let Some(notice) = window.notice.as_deref() {
        ui.add_space(theme::STEP);
        ui.label(RichText::new(notice).color(theme::danger(ui)));
    }
    window.absorbed(flags);
    window.editing = Some(editing);
}

/// One number of the record: the one scrubbing field, with its unit outside the box.
fn number(ui: &mut Ui, value: &mut f32, range: RangeInclusive<f32>, unit: &str) -> Response {
    let mut field = Scrub::new(value).decimals(2).speed(0.01).range(range).width(FIELD_WIDTH);
    if !unit.is_empty() {
        field = field.unit(unit);
    }
    ui.add(field)
}

/// The swatch: the base's colour, with the decor tiled at its own coverage and turn.
fn swatch(ui: &mut Ui, editing: &Editing, texture: Option<&TextureHandle>) {
    let (rect, _) = ui.allocate_exact_size(SWATCH, Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, theme::RADIUS, ink(editing.colour));
    let outline = Stroke::new(1.0, theme::line(ui));
    painter.rect_stroke(rect, 0, outline, StrokeKind::Inside);
    let picture = match (texture, editing.decor) {
        (Some(texture), true) if !editing.picture.trim().is_empty() => texture,
        _ => return,
    };
    // The swatch shows `SWATCH_MM` of board and one tile is `covers` millimetres of it, so the quad's
    // uv runs that many repeats across and down; the texture repeats, so one quad is every tile.
    let across = SWATCH_MM / (editing.covers_mm as f32).max(1.0);
    let down = across * SWATCH.y / SWATCH.x;
    let corners = [rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom()];
    let tile = [pos2(0.0, 0.0), pos2(across, 0.0), pos2(across, down), pos2(0.0, down)];
    // A quarter turn moves the picture's top-left corner to the next corner of the swatch, so the
    // uv a corner takes steps back by the turn.
    let start = usize::from((4 - editing.turns % 4) % 4);
    let mut mesh = egui::Mesh::with_texture(picture.id());
    for (index, corner) in corners.into_iter().enumerate() {
        mesh.vertices.push(egui::epaint::Vertex {
            pos: corner,
            uv: tile[(index + start) % 4],
            color: theme::picture(ui),
        });
    }
    mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    painter.add(egui::Shape::mesh(mesh));
    painter.rect_stroke(rect, 0, outline, StrokeKind::Inside);
}

/// A record's colour as the ink the list's chip draws.
///
/// A record with no base is a finish — a lacquer on its own — and it has no albedo to show: its chip
/// stays the control's ink, which reads as an empty swatch beside the boards' colours.
fn record_ink(ui: &Ui, record: &Material) -> Color32 {
    record.base().map_or_else(|| theme::control(ui), |base| ink(base.colour()))
}

/// A linear-RGB albedo as the eight-bit ink the chrome draws.
///
/// The tracer's colour is linear and `egui` paints gamma-space bytes, so the swatch and the chip apply
/// the sRGB transfer once here instead of showing a board darker than the render will draw it.
fn ink(colour: [f32; 3]) -> Color32 {
    let channel = |value: f32| {
        let clamped = value.clamp(0.0, 1.0);
        let encoded = if clamped <= 0.003_130_8 { clamped * 12.92 } else { 1.055 * clamped.powf(1.0 / 2.4) - 0.055 };
        (encoded * 255.0).round().clamp(0.0, 255.0) as u8
    };
    Color32::from_rgb(channel(colour[0]), channel(colour[1]), channel(colour[2]))
}

/// The record as the fields hold it, before the crate validates it.
///
/// Every switch, number and picture the window offers is here and nowhere else: the record is built
/// from these fields and a refusal names the channel the crate refused, so the window never restates
/// a range a record owns.
#[derive(Clone, Debug)]
struct Editing {
    id: MaterialId,
    /// True while the fields make a record the library does not hold yet.
    fresh: bool,
    name: String,
    base: bool,
    colour: [f32; 3],
    roughness: f32,
    metallic: f32,
    ior: f32,
    transmission: f32,
    absorption: [f32; 3],
    decor: bool,
    picture: String,
    covers_mm: u32,
    turns: u8,
    coat: bool,
    weight: f32,
    gloss: f32,
    coat_ior: f32,
}

impl Editing {
    /// The fields a fresh record starts from: a matte board, so a person sees a material the moment
    /// they press *New material*.
    fn new(id: MaterialId, lang: Lang) -> Self {
        Self {
            id,
            fresh: true,
            name: Message::MaterialDefaultName.with(lang, [id.get()]),
            base: true,
            colour: [0.6, 0.6, 0.6],
            roughness: 1.0,
            metallic: 0.0,
            ior: 1.5,
            transmission: 0.0,
            absorption: [0.0; 3],
            decor: false,
            picture: String::new(),
            covers_mm: DEFAULT_COVERS_MM,
            turns: 0,
            coat: false,
            weight: 1.0,
            gloss: 70.0,
            coat_ior: Coat::IOR,
        }
    }

    /// The fields of `record`.
    fn of(record: &Material) -> Self {
        let base = record.base();
        let decor = base.and_then(Base::decor);
        let coat = record.coat();
        Self {
            id: record.id(),
            fresh: false,
            name: record.name().to_owned(),
            base: base.is_some(),
            colour: base.map_or([0.6, 0.6, 0.6], Base::colour),
            roughness: base.map_or(1.0, Base::roughness),
            metallic: base.map_or(0.0, Base::metallic),
            ior: base.map_or(1.5, Base::ior),
            transmission: base.map_or(0.0, Base::transmission),
            absorption: base.map_or([0.0; 3], Base::absorption),
            decor: decor.is_some(),
            picture: decor.map_or_else(String::new, |decor| decor.picture().to_owned()),
            covers_mm: decor.map_or(DEFAULT_COVERS_MM, |decor| covers_of(decor.covers())),
            turns: decor.map_or(0, |decor| decor.turns().get()),
            coat: coat.is_some(),
            weight: coat.map_or(1.0, Coat::weight),
            gloss: coat.map_or(70.0, Coat::gloss),
            coat_ior: coat.map_or(Coat::IOR, Coat::ior),
        }
    }

    /// The record the fields describe, or the crate's sentence naming what is wrong with it.
    fn record(&self) -> Result<Material, MaterialError> {
        let base = if self.base {
            let decor = match (self.decor, self.picture.trim()) {
                (true, picture) if !picture.is_empty() => {
                    let covers = Length::from_millimetres(i64::from(self.covers_mm))
                        .ok_or(MaterialError::Covers { covers: Length::ZERO })?;
                    let turns = Turns::new(self.turns).ok_or(MaterialError::Turns { turns: self.turns })?;
                    Some(Decor::new(picture, covers, turns)?)
                }
                _ => None,
            };
            Some(Base::new(
                self.colour,
                decor,
                self.roughness,
                self.metallic,
                self.ior,
                self.transmission,
                self.absorption,
            )?)
        } else {
            None
        };
        let coat = if self.coat { Some(Coat::new(self.weight, self.gloss, self.coat_ior)?) } else { None };
        Material::new(self.id, self.name.clone(), base, coat)
    }
}

/// A decor's coverage in whole millimetres, inside the ends the field offers.
fn covers_of(covers: Length) -> u32 {
    let millimetres = covers.micrometres() / 1_000;
    u32::try_from(millimetres).unwrap_or(MAX_COVERS_MM).clamp(MIN_COVERS_MM, MAX_COVERS_MM)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    fn id(value: u32) -> MaterialId {
        MaterialId::new(value).expect("a non-zero id")
    }

    fn editing() -> Editing {
        Editing::new(id(1), Lang::En)
    }

    /// The record an act files, so a test reads the fields rather than the enum.
    fn record_of(act: Act) -> Material {
        match act {
            Act::Set(material) => *material,
            other => panic!("not a record: {other:?}"),
        }
    }

    #[test]
    fn a_fresh_record_is_a_board_the_crate_accepts() {
        let record = editing().record().expect("a matte board is a record");
        assert_eq!(record.id(), id(1));
        assert_eq!(record.name(), "Material 1", "a made record is named after its number");
        let base = record.base().expect("a base");
        assert_eq!((base.roughness(), base.ior()), (1.0, 1.5));
        assert!(base.decor().is_none() && record.coat().is_none(), "no picture and no coat until asked for");
    }

    #[test]
    fn a_record_with_neither_a_base_nor_a_coat_is_refused_and_says_why() {
        let mut fields = editing();
        fields.base = false;
        let error = fields.record().expect_err("no surface at all");
        assert_eq!(error, MaterialError::NoSurface, "the crate's own sentence names the missing pair");
        fields.coat = true;
        assert!(fields.record().is_ok(), "a coat alone is a lacquer");
    }

    #[test]
    fn a_hostile_channel_is_refused_by_the_record_and_names_the_field() {
        let mut fields = editing();
        fields.ior = 0.5;
        let error = fields.record().expect_err("an index below one is not glass");
        assert!(matches!(error, MaterialError::Field { field: "base ior", .. }), "{error:?}");
        assert!(error.to_string().contains("base ior"), "the sentence names the channel: {error}");
        fields.ior = MAX_IOR;
        fields.roughness = -0.2;
        assert!(matches!(fields.record(), Err(MaterialError::Field { field: "base roughness", .. })));
        fields.roughness = 1.0;
        fields.decor = true;
        fields.picture = "textures/oak.png".to_owned();
        fields.covers_mm = 0;
        assert!(matches!(fields.record(), Err(MaterialError::Covers { .. })), "a decor covering nothing");
    }

    #[test]
    fn a_picture_the_person_chose_switches_the_decor_on_and_is_written_at_once() {
        let mut window = MaterialWindow::new();
        window.open();
        window.editing = Some(editing());
        window.picture("textures/oak.png");
        let record = record_of(window.settle(0.0, true).expect("an act at once"));
        let decor = record.base().and_then(Base::decor).expect("a decor");
        assert_eq!(decor.picture(), "textures/oak.png");
        assert_eq!(decor.covers(), Length::from_millimetres(600).expect("a length"));
        assert!(window.settle(1.0, true).is_none(), "one picture, one act");
    }

    #[test]
    fn a_settled_edit_waits_for_the_pause_and_a_forced_one_does_not() {
        let mut window = MaterialWindow::new();
        window.open();
        window.editing = Some(editing());
        window.dirty = true;
        window.touched = Some(10.0);
        assert!(window.settle(10.1, false).is_none(), "a typed name is not written per keystroke");
        assert!(window.settle(10.0 + COMMIT_SECONDS, false).is_some(), "and it is written when the hand stops");
        window.dirty = true;
        assert!(window.settle(0.0, true).is_some(), "a switch is one act, however recently it moved");
    }

    #[test]
    fn a_record_removed_elsewhere_leaves_the_fields_with_nothing() {
        let mut window = MaterialWindow::new();
        let oak = Material::of_base(id(4), "Oak", Base::matte([0.5, 0.4, 0.3]).expect("a board")).expect("a record");
        window.editing = Some(Editing::of(&oak));
        window.forget(id(4));
        assert_eq!(window.editing(), None);
        assert_eq!(window.settle(0.0, true), None, "a record that is gone has nothing to write");
    }

    #[test]
    fn a_record_reads_back_as_the_fields_it_was_made_of() {
        let decor = Decor::new("textures/oak.png", Length::from_millimetres(900).expect("a length"), Turns::ACROSS)
            .expect("a decor");
        let base = Base::new([0.62, 0.42, 0.22], Some(decor), 0.7, 0.0, 1.9, 0.3, [0.5, 1.5, 2.5]).expect("a board");
        let coat = Coat::new(0.75, 70.0, 1.62).expect("a coat");
        let record = Material::new(id(7), "Oak", Some(base), Some(coat)).expect("a record");
        let fields = Editing::of(&record);
        assert_eq!(fields.covers_mm, 900);
        assert_eq!(fields.turns, Turns::ACROSS.get());
        let rebuilt = fields.record().expect("the fields describe the record they came from");
        assert_eq!(rebuilt, record, "a record the window opened and wrote back is the same record");
    }

    #[test]
    fn the_swatch_shows_a_linear_albedo_in_the_gamma_space_the_chrome_paints() {
        assert_eq!(ink([0.0, 0.0, 0.0]), Color32::BLACK);
        assert_eq!(ink([1.0, 1.0, 1.0]), Color32::WHITE);
        let mid = ink([0.215_86, 0.215_86, 0.215_86]);
        assert_eq!(mid.r(), 128, "linear 0.216 is the middle of the eight-bit scale, not 55");
    }
}
