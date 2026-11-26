//! The Render place: one still image of the scene in front, drawn by the final render.
//!
//! This is a place, not a question: a render runs for minutes and the person may go back to the tree
//! while it does, so it gets a surface of its own — the `Render` tab beside the File menu — instead
//! of a modal that holds the window. The page owns the numbers (size, samples, exposure and the file
//! to write) and nothing else: the frame, the device, the worker thread, the picture it publishes and
//! the file manager belong to the shell, which hands this module a [`Page`] and performs the one
//! [`Action`] it returns (Y6, ADR 0250).
//!
//! The brief: **the image is the surface, the numbers are a rail, the row under the picture says how
//! far the render is.** A furniture maker opens this place to look at the plate of the cabinet in
//! front, at a size a workshop prints, without leaving the editor — so the picture takes the room,
//! letterboxed against the panel ink rather than stretched or cropped, and the numbers stand in one
//! measured rail beside it. Under the picture sits the only line that is about the run: the
//! resolution, the samples it took and owes, the time it took and the time it needs, and the file it
//! is written to. **Render** is the one primary action; Cancel replaces it while a run is in flight
//! and nothing else on the page can be pressed into a second run.
//!
//! Refuses: a size, a sample count or a pixel count outside the bounds the shell hands in (they are
//! the final render's own caps, so the field cannot offer what the tracer would refuse), and a render
//! with no destination. A refused number is clamped by the field itself and the button says why it
//! cannot run, rather than a run that fails after a minute.
//!
//! Changes when: what a render needs to be asked, what it reports while it runs, or how the picture
//! takes the room, changes.

use core::time::Duration;
use std::path::PathBuf;

use egui::{RichText, Ui};
use text::Message;

use crate::scrub::{CommitPolicy, Scrub};
use crate::theme;

/// The rail's own width, in points: a name, a field and its unit on one line.
///
/// Measured from the widest row the rail draws — a size field with its `px` and the two preset
/// buttons above it — with the air a grid needs around it. The picture takes whatever is left, which
/// at the smallest window the shell allows is still the larger part of the place.
const RAIL_WIDTH: f32 = 320.0;

/// The gap between the picture, its rail and the edge of the place, in points.
const GAP: f32 = theme::STEP * 4.0;

/// The height of the status row under the picture: the window's own status row's height.
const STATUS_HEIGHT: f32 = theme::STATUS_HEIGHT;

/// The thickness of the progress bar above the status row.
const BAR_HEIGHT: f32 = theme::STEP * 1.5;

/// What separates two readings of the status row. Punctuation, not a sentence: the readings are
/// messages of their own.
const SEPARATOR: &str = "  ·  ";

/// The brightest studio the page offers, in percent of a white room (a radiance of one).
///
/// A hundred percent is a room as bright as white paper, which is already brighter than any workshop
/// a cabinet is photographed in; past it the field would be offering a light the picture cannot use
/// without an exposure the person has to correct afterwards.
pub const MAX_STUDIO_PERCENT: f32 = 100.0;

/// The limits a render's numbers are kept inside.
///
/// They are handed in by the shell from the final render's own constants (`final::MIN_SIDE` and its
/// neighbours) instead of being written here: two copies of a cap that disagree would offer a size
/// the tracer refuses, and a UI crate that depended on a `wgpu` crate to read a number would break
/// the layering (`AGENTS.md` §3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    /// The smallest side, in pixels.
    pub min_side: u32,
    /// The largest side, in pixels.
    pub max_side: u32,
    /// The fewest samples per pixel.
    pub min_samples: u32,
    /// The most samples per pixel.
    pub max_samples: u32,
    /// The largest image, in pixels.
    pub max_pixels: u64,
}

impl Bounds {
    /// The largest side this image may have, given its other side and the pixel cap.
    pub fn widest(self, other: u32) -> u32 {
        let other = u64::from(other.max(1));
        let by_pixels = self.max_pixels / other;
        u32::try_from(by_pixels).unwrap_or(u32::MAX).clamp(self.min_side, self.max_side)
    }
}

/// The two sizes a person actually chooses: a draft to check the view and a plate for print.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    /// A screen-sized plate at a sample count that finishes in seconds.
    Draft,
    /// A4 at 300 dpi with enough samples for print.
    Print,
}

impl Preset {
    /// The size and the sample count the preset sets.
    pub fn values(self) -> (u32, u32, u32) {
        match self {
            // 1920 by 1080 with four samples: the frame the screen shows, in a second or two.
            Self::Draft => (1920, 1080, 4),
            // A4 at 300 dpi, 256 samples: a catalogue plate.
            Self::Print => (2480, 3508, 256),
        }
    }

    /// The words the button carries.
    fn label(self) -> Message {
        match self {
            Self::Draft => Message::RenderPresetDraft,
            Self::Print => Message::RenderPresetPrint,
        }
    }
}

/// What one render is asked for, as the page edits it.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// The image width in pixels.
    pub width: u32,
    /// The image height in pixels.
    pub height: u32,
    /// The samples taken per pixel.
    pub samples: u32,
    /// The exposure stops applied before the filmic curve.
    pub exposure: f32,
    /// The studio behind the scene, in percent of a white room: what a ray sees where the frame has
    /// no geometry. Zero is no studio and the black background a render had before it could have one.
    pub studio: f32,
    /// Where the PNG goes; `None` until the person has chosen one.
    pub path: Option<PathBuf>,
}

impl Settings {
    /// The settings a page opens with: a draft, one stop, and no destination yet.
    pub fn draft() -> Self {
        let (width, height, samples) = Preset::Draft.values();
        Self { width, height, samples, exposure: 1.0, studio: 0.0, path: None }
    }

    /// Sets the numbers of `preset`, keeping the destination and the exposure.
    pub fn apply(&mut self, preset: Preset) {
        let (width, height, samples) = preset.values();
        self.width = width;
        self.height = height;
        self.samples = samples;
    }

    /// Keeps every number inside `bounds`, so the page can never ask for an image the tracer refuses.
    pub fn clamp(&mut self, bounds: Bounds) {
        self.width = self.width.clamp(bounds.min_side, bounds.max_side);
        self.height = self.height.clamp(bounds.min_side, bounds.max_side);
        let narrowest = bounds.widest(self.width);
        if self.height > narrowest {
            self.height = narrowest;
        }
        let with_height = bounds.widest(self.height);
        if self.width > with_height {
            self.width = with_height;
        }
        self.samples = self.samples.clamp(bounds.min_samples, bounds.max_samples);
        if !self.exposure.is_finite() || self.exposure <= 0.0 {
            self.exposure = 1.0;
        }
        if !self.studio.is_finite() {
            self.studio = 0.0;
        }
        self.studio = self.studio.clamp(0.0, MAX_STUDIO_PERCENT);
    }

    /// Why this render cannot start, if it cannot.
    pub fn refusal(&self) -> Option<Message> {
        match &self.path {
            None => Some(Message::RenderNeedsPath),
            Some(_) if u64::from(self.width) * u64::from(self.height) == 0 => Some(Message::RenderNeedsSize),
            Some(_) => None,
        }
    }
}

/// The picture the place draws: the texture the shell put the last snapshot in, and its own size.
///
/// The page never touches a device: it is handed a [`egui::TextureId`] and the picture's size, which
/// is the aspect it keeps, and paints that texture into the room it was given.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Picture {
    /// The texture the shell registered for the picture.
    pub texture: egui::TextureId,
    /// The picture's width in pixels.
    pub width: u32,
    /// The picture's height in pixels.
    pub height: u32,
}

/// What one run of the place has taken and where it got to: the numbers the status row prints.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    /// The image width the run was asked for.
    pub width: u32,
    /// The image height the run was asked for.
    pub height: u32,
    /// The samples it has taken.
    pub done: u32,
    /// The samples it owes.
    pub total: u32,
    /// How long it has taken.
    pub elapsed: Duration,
    /// How much longer it needs, from the samples it has taken; `None` before the first one.
    pub estimate: Option<Duration>,
    /// The file it writes, as it was chosen at the press.
    pub path: Option<PathBuf>,
}

impl Run {
    /// How far the run is, as `0.0..=1.0`, which a progress bar draws.
    pub fn fraction(&self) -> f32 {
        if self.total == 0 {
            return 0.0;
        }
        self.done.min(self.total) as f32 / self.total as f32
    }

    /// Whether the picture this run published is of the numbers the page offers now: a size or a
    /// sample count that moved needs another run, while the exposure only re-tones the picture.
    fn is_current(&self, settings: &Settings) -> bool {
        (self.width, self.height, self.total) == (settings.width, settings.height, settings.samples)
    }
}

/// Where the run is, as the page draws it.
#[derive(Clone, Debug, PartialEq)]
pub enum Stage {
    /// Nothing is running and nothing has been drawn yet: the page offers its numbers and its Render
    /// button.
    Idle,
    /// A run is in flight; the picture it has already published is on the screen beside these numbers.
    Running(Run),
    /// The image was written, and the picture of it is the file.
    Done(Run),
    /// The person stopped the run: nothing was written and the last picture stays.
    Cancelled(Run),
    /// The run refused, with the sentence that says why.
    Failed(String),
}

impl Stage {
    /// Whether a run is in flight, which is what a surface polls before it starts another.
    pub fn is_running(&self) -> bool {
        matches!(self, Self::Running(_))
    }

    /// The numbers of the run this stage is about, in every stage that has one.
    pub fn run(&self) -> Option<&Run> {
        match self {
            Self::Idle | Self::Failed(_) => None,
            Self::Running(run) | Self::Done(run) | Self::Cancelled(run) => Some(run),
        }
    }
}

/// One page: the scene it is about, the settings it edits, where the last run got to and its picture.
pub struct Page<'a> {
    /// The scene the image will be of, by the name the tab shows.
    pub scene: &'a str,
    /// The numbers and the destination, edited in place.
    pub settings: &'a mut Settings,
    /// The limits the numbers are kept inside.
    pub bounds: Bounds,
    /// Where the last run got to.
    pub stage: &'a Stage,
    /// The last picture the shell published, if a run has drawn one.
    pub picture: Option<Picture>,
}

/// What the page asked the shell to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// Start a render with these settings.
    Render,
    /// Stop the run in flight at its next sample.
    Cancel,
    /// Ask the system file manager where the PNG goes.
    Choose,
    /// Leave the place and go back to the document.
    Close,
}

/// Draws the page in the room the shell gives it and returns the one action it asked for, if any.
///
/// The page draws no host of its own: the shell hands it the workspace's area, exactly as it hands
/// the ribbon its band, so the frame keeps one owner per rectangle (ADR 0080, `cargo xtask ui-rules`).
/// The area is filled — the place replaces the scene rather than floating over it — and the picture
/// takes everything the rail does not (ADR 0250).
pub fn show(ui: &mut Ui, page: Page<'_>) -> Option<Action> {
    let lang = crate::language(ui.ctx());
    let mut action = None;
    let Page { scene, settings, bounds, stage, picture } = page;
    settings.clamp(bounds);
    let area = ui.available_rect_before_wrap().intersect(ui.clip_rect());
    egui::Frame::new().fill(theme::panel(ui)).show(ui, |ui| {
        ui.set_min_size(area.size());
        let room = picture_room(area);
        let rail = egui::Rect::from_min_max(egui::pos2(room.right() + GAP, area.top()), area.max);
        paint_picture(ui, room, picture, lang);
        status_row(ui, status_rect(area), stage, settings, lang);
        ui.scope_builder(egui::UiBuilder::new().max_rect(rail.shrink2(egui::vec2(GAP, GAP))), |ui| {
            rail_content(ui, scene, settings, bounds, stage, &mut action, lang);
        });
    });
    if ui.ctx().input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        action = Some(Action::Close);
    }
    if stage.is_running() {
        // The counter moves on the worker's thread; the page asks for the next frame itself so the
        // bar keeps up without the shell repainting for the whole run.
        ui.ctx().request_repaint_after(Duration::from_millis(120));
    }
    action
}

/// The picture's room in the page's own area: everything left of the rail, above the status row and
/// inside the place's own air.
///
/// Nothing about the picture's aspect: [`fitted`] letterboxes the image inside this rectangle, so a
/// portrait plate in a landscape window leaves bars of panel ink beside it.
pub(crate) fn picture_room(area: egui::Rect) -> egui::Rect {
    let rail = RAIL_WIDTH.min((area.width() * 0.45).max(0.0));
    egui::Rect::from_min_max(
        area.min,
        egui::pos2((area.right() - rail - GAP).max(area.left()), status_rect(area).top()),
    )
}

/// The status row's own rectangle: one line under the picture, with the progress bar above it.
pub(crate) fn status_rect(area: egui::Rect) -> egui::Rect {
    let height = STATUS_HEIGHT + BAR_HEIGHT + GAP;
    egui::Rect::from_min_max(
        egui::pos2(area.left(), (area.bottom() - height).max(area.top())),
        egui::pos2(area.right() - GAP, area.bottom()),
    )
}

/// The picture, fitted to its own aspect and centred in `room`, as a rectangle.
///
/// Letterboxed, never stretched and never cropped: a plate is judged by its proportions, and an image
/// that fills the room by distortion is a picture of a cabinet that does not exist.
pub(crate) fn fitted(room: egui::Rect, width: u32, height: u32) -> egui::Rect {
    let width = width.max(1) as f32;
    let height = height.max(1) as f32;
    let room_aspect = (room.width() / room.height().max(1.0)).max(f32::EPSILON);
    let aspect = width / height;
    let size = if aspect > room_aspect {
        egui::vec2(room.width(), room.width() / aspect)
    } else {
        egui::vec2(room.height() * aspect, room.height())
    };
    egui::Rect::from_center_size(room.center(), size)
}

/// Paints the picture in its room, or the word that says there is none yet.
fn paint_picture(ui: &Ui, room: egui::Rect, picture: Option<Picture>, lang: text::Lang) {
    let Some(picture) = picture else {
        ui.painter().text(
            room.center(),
            egui::Align2::CENTER_CENTER,
            Message::RenderNoPicture.text(lang),
            egui::FontId::proportional(14.0),
            theme::text_dim(ui),
        );
        return;
    };
    let rect = fitted(room, picture.width, picture.height);
    let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    ui.painter().image(picture.texture, rect, uv, theme::picture(ui));
}

/// The row under the picture: the bar while a run is in flight, and the numbers the run is.
///
/// One line, dim, in the place's own reading order: the resolution, the samples it took and owes, the
/// time it took, the time it needs and the file it is written to. A stage that has no file says so in
/// its own sentence instead of leaving the field out.
fn status_row(ui: &mut Ui, rect: egui::Rect, stage: &Stage, settings: &Settings, lang: text::Lang) {
    if matches!(stage, Stage::Idle) {
        return;
    }
    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
        ui.spacing_mut().item_spacing.y = theme::STEP;
        if let Stage::Running(run) = stage {
            ui.add_sized(
                egui::vec2(rect.width(), BAR_HEIGHT),
                egui::ProgressBar::new(run.fraction()).corner_radius(theme::RADIUS),
            );
        } else {
            ui.add_space(BAR_HEIGHT);
        }
        let text = match stage {
            Stage::Idle => String::new(),
            Stage::Running(run) | Stage::Cancelled(run) => readings(run, lang),
            Stage::Done(run) => readings(run, lang),
            Stage::Failed(reason) => Message::RenderFailed.with(lang, [reason.clone()]),
        };
        let colour = match stage {
            Stage::Failed(_) => theme::danger(ui),
            _ => theme::text_dim(ui),
        };
        ui.add(egui::Label::new(RichText::new(text).color(colour)).truncate());
        // A size or a sample count that moved after the run is a picture of other numbers: say so
        // rather than letting a person read the plate as the settings above it (Y6).
        if let Some(run) = stage.run()
            && !run.is_current(settings)
        {
            let note = Message::RenderPictureOlder
                .with(lang, [run.width.to_string(), run.height.to_string(), run.total.to_string()]);
            ui.add(egui::Label::new(RichText::new(note).color(theme::warn(ui))).truncate());
        }
    });
}

/// One run's readings, joined in a sentence a person reads once: size, samples, time, file.
fn readings(run: &Run, lang: text::Lang) -> String {
    let mut parts = vec![format!("{} × {}", run.width, run.height)];
    parts.push(Message::RenderSamplesDone.with(lang, [run.done.to_string(), run.total.to_string()]));
    parts.push(Message::RenderElapsed.with(lang, [clock(run.elapsed)]));
    if let Some(left) = run.estimate {
        parts.push(Message::RenderEstimate.with(lang, [clock(left)]));
    }
    match &run.path {
        Some(path) => parts.push(Message::RenderWrittenTo.with(lang, [path.display().to_string()])),
        None => parts.push(Message::RenderNeedsPath.text(lang).to_owned()),
    }
    parts.join(SEPARATOR)
}

/// A duration as a clock a person reads while waiting: `m:ss`, and `h:mm:ss` when it runs that long.
fn clock(elapsed: Duration) -> String {
    let seconds = elapsed.as_secs();
    let (hours, minutes, seconds) = (seconds / 3600, (seconds % 3600) / 60, seconds % 60);
    if hours > 0 { format!("{hours}:{minutes:02}:{seconds:02}") } else { format!("{minutes}:{seconds:02}") }
}

/// The rail beside the picture: what the render is of, the numbers it takes and the one button.
fn rail_content(
    ui: &mut Ui,
    scene: &str,
    settings: &mut Settings,
    bounds: Bounds,
    stage: &Stage,
    action: &mut Option<Action>,
    lang: text::Lang,
) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = theme::STEP * 2.0;
        ui.set_max_width(RAIL_WIDTH);
        ui.heading(Message::RenderTitle.text(lang));
        ui.label(RichText::new(Message::RenderOf.with(lang, [scene])).color(theme::text_dim(ui)));
        ui.add_space(theme::STEP * 2.0);
        if let Some(chosen) = numbers_column(ui, settings, bounds, stage, lang) {
            *action = Some(chosen);
        }
        ui.add_space(theme::STEP * 3.0);
        destination(ui, settings, action, lang);
        ui.add_space(theme::STEP * 3.0);
        run_row(ui, settings, stage, action, lang);
    });
}

/// The presets and the four numbers. Returns a preset the person pressed, if any.
///
/// The numbers sit in one grid of two columns — the name, then the field with its unit outside the
/// box — so a column of values lines up as one reading (ADR 0092, ADR 0116). A preset fills the three
/// numbers a person chooses by size; the exposure is never a preset, because it is the same for a
/// draft and for a plate of the same scene.
fn numbers_column(
    ui: &mut Ui,
    settings: &mut Settings,
    bounds: Bounds,
    stage: &Stage,
    lang: text::Lang,
) -> Option<Action> {
    let mut chosen = None;
    ui.horizontal(|ui| {
        for preset in [Preset::Draft, Preset::Print] {
            let (width, height, _samples) = preset.values();
            let size = format!("{width} × {height}");
            let label = Message::RenderPreset.with(lang, [preset.label().text(lang), size.as_str()]);
            if ui.add_enabled(!stage.is_running(), egui::Button::new(label)).clicked() {
                chosen = Some(Action::Render);
                settings.apply(preset);
            }
        }
    });
    ui.add_space(theme::STEP * 2.0);
    egui::Grid::new("render.numbers").num_columns(2).spacing([theme::STEP * 3.0, theme::STEP]).show(ui, |ui| {
        // The field sits in its own row inside the cell: the field draws the air before its unit,
        // and a grid refuses air (its rows are the spacing).
        let number = |ui: &mut Ui, name: Message, value: &mut u32, range: core::ops::RangeInclusive<u32>| {
            ui.label(name.text(lang));
            ui.horizontal(|ui| {
                Scrub::new(value).decimals(0).speed(2.0).range(range).policy(CommitPolicy::Live).show(ui);
            });
            ui.end_row();
        };
        let widest = bounds.widest(settings.height);
        number(ui, Message::RenderWidth, &mut settings.width, bounds.min_side..=widest);
        let tallest = bounds.widest(settings.width);
        number(ui, Message::RenderHeight, &mut settings.height, bounds.min_side..=tallest);
        number(ui, Message::RenderSamples, &mut settings.samples, bounds.min_samples..=bounds.max_samples);
        // The studio is a percentage of a white room, and the exposure a number of stops: both draw
        // their unit outside the box like every unit in the interface.
        ui.label(Message::RenderStudio.text(lang));
        ui.horizontal(|ui| {
            Scrub::new(&mut settings.studio)
                .decimals(0)
                .speed(1.0)
                .range(0.0..=MAX_STUDIO_PERCENT)
                .unit("%")
                .policy(CommitPolicy::Live)
                .show(ui);
        });
        ui.end_row();
        // The exposure is the one number with a unit: stops, drawn outside the box like every unit.
        ui.label(Message::RenderExposure.text(lang));
        ui.horizontal(|ui| {
            Scrub::new(&mut settings.exposure)
                .decimals(2)
                .speed(0.02)
                .range(0.01..=64.0)
                .unit("stops")
                .policy(CommitPolicy::Live)
                .show(ui);
        });
        ui.end_row();
    });
    chosen
}

/// The destination row: the path, and the one button that asks for another.
fn destination(ui: &mut Ui, settings: &Settings, action: &mut Option<Action>, lang: text::Lang) {
    ui.label(RichText::new(Message::RenderDestination.text(lang)).strong());
    ui.horizontal(|ui| {
        let shown = match &settings.path {
            Some(path) => path.display().to_string(),
            None => Message::RenderNoPath.text(lang).to_owned(),
        };
        // A path is long and the rail is one column wide: it elides, and the full path stays in the
        // hint a person reads before pressing Render.
        ui.scope(|ui| {
            ui.set_max_width((RAIL_WIDTH - 96.0).max(48.0));
            ui.add(egui::Label::new(RichText::new(shown.clone()).color(theme::text_dim(ui))).truncate())
                .on_hover_text(shown);
        });
        if ui.button(Message::RenderChoose.text(lang)).clicked() {
            *action = Some(Action::Choose);
        }
    });
}

/// The primary row: Render while nothing runs, Cancel while the run is in flight.
fn run_row(ui: &mut Ui, settings: &Settings, stage: &Stage, action: &mut Option<Action>, lang: text::Lang) {
    ui.horizontal(|ui| {
        if stage.is_running() {
            if ui.button(Message::RenderCancel.text(lang)).clicked() {
                *action = Some(Action::Cancel);
            }
            return;
        }
        let why = settings.refusal();
        let button = egui::Button::new(RichText::new(Message::RenderStart.text(lang)).size(14.0))
            .min_size(egui::vec2(120.0, 30.0));
        let response = ui.add_enabled(why.is_none(), button);
        if response.clicked() {
            *action = Some(Action::Render);
        }
        if let Some(why) = why {
            ui.label(RichText::new(why.text(lang)).color(theme::text_dim(ui)));
        }
    });
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    fn bounds() -> Bounds {
        Bounds { min_side: 16, max_side: 8192, min_samples: 1, max_samples: 4096, max_pixels: 16_000_000 }
    }

    fn picture(width: u32, height: u32) -> Picture {
        Picture { texture: egui::TextureId::Managed(7), width, height }
    }

    fn run(done: u32, total: u32) -> Run {
        Run {
            width: 1920,
            height: 1080,
            done,
            total,
            elapsed: Duration::from_secs(8),
            estimate: Some(Duration::from_secs(16)),
            path: Some("plate.png".into()),
        }
    }

    /// The meshes a frame painted with `texture`, which is how a picture reaches the screen.
    fn painted_with(output: &egui::FullOutput, texture: egui::TextureId) -> Vec<egui::Rect> {
        fn walk(shape: &egui::Shape, texture: egui::TextureId, out: &mut Vec<egui::Rect>) {
            match shape {
                egui::Shape::Mesh(mesh) if mesh.texture_id == texture => out.push(mesh.calc_bounds()),
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        walk(shape, texture, out);
                    }
                }
                _ => {}
            }
        }
        let mut out = Vec::new();
        for clipped in &output.shapes {
            walk(&clipped.shape, texture, &mut out);
        }
        out
    }

    #[test]
    fn a_preset_sets_the_numbers_and_keeps_the_destination() {
        let mut settings = Settings::draft();
        settings.path = Some("plate.png".into());
        settings.exposure = 2.0;
        settings.apply(Preset::Print);
        assert_eq!((settings.width, settings.height, settings.samples), (2480, 3508, 256));
        assert_eq!(settings.path, Some("plate.png".into()), "a preset is not a destination");
        assert_eq!(settings.exposure, 2.0, "and it is not an exposure");
    }

    #[test]
    fn a_clamped_request_is_one_the_tracer_accepts() {
        let mut settings =
            Settings { width: 40_000, height: 40_000, samples: 100_000, exposure: 0.0, studio: f32::NAN, path: None };
        settings.clamp(bounds());
        assert_eq!(settings.width, 8192);
        assert!(settings.height <= settings.width);
        assert!(u64::from(settings.width) * u64::from(settings.height) <= bounds().max_pixels);
        assert_eq!(settings.samples, 4096);
        assert_eq!(settings.exposure, 1.0, "a broken exposure falls back to one stop");
        assert_eq!(settings.studio, 0.0, "a broken studio falls back to none");

        // A tall request that fits the side cap but not the pixel cap is brought inside it.
        let mut tall = Settings { width: 8192, height: 8192, samples: 1, ..Settings::draft() };
        tall.clamp(bounds());
        assert!(u64::from(tall.width) * u64::from(tall.height) <= bounds().max_pixels, "{tall:?}");
        assert_eq!(bounds().widest(1000), 8192, "a narrow image takes the whole side cap");
        assert_eq!(bounds().widest(4000), 4000, "a wide one is bounded by the pixel cap");
    }

    #[test]
    fn the_studio_is_a_percentage_of_a_white_room_and_never_negative() {
        // A page opens with no studio, which is the black background a render always had, and the
        // field cannot be dragged past a white room or below none.
        assert_eq!(Settings::draft().studio, 0.0);
        for (asked, kept) in [(-30.0, 0.0), (0.0, 0.0), (45.0, 45.0), (100.0, 100.0), (400.0, 100.0)] {
            let mut settings = Settings::draft();
            settings.studio = asked;
            settings.clamp(bounds());
            assert_eq!(settings.studio, kept, "{asked} percent became {}", settings.studio);
        }
        assert_eq!(MAX_STUDIO_PERCENT, 100.0, "a white room is the brightest studio the field offers");
    }

    #[test]
    fn a_render_without_a_destination_is_refused_before_it_starts() {
        let mut settings = Settings::draft();
        assert_eq!(settings.refusal(), Some(Message::RenderNeedsPath));
        settings.path = Some("plate.png".into());
        assert_eq!(settings.refusal(), None);
        settings.width = 0;
        assert_eq!(settings.refusal(), Some(Message::RenderNeedsSize));
    }

    #[test]
    fn a_picture_is_letterboxed_in_its_room_and_never_stretched() {
        // A landscape picture in a square room uses the whole width and leaves bars above and below.
        let room = egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(400.0, 400.0));
        let wide = fitted(room, 1600, 800);
        assert_eq!(wide.size(), egui::vec2(400.0, 200.0), "the whole room's width, the picture's aspect");
        assert_eq!(wide.center(), room.center(), "fitted and centred, not pushed to a corner");
        // A portrait plate turns it the other way round.
        let tall = fitted(room, 800, 1600);
        assert_eq!(tall.size(), egui::vec2(200.0, 400.0));
        assert!(room.contains_rect(tall) && room.contains_rect(wide), "a picture never leaves its room");
        // The same picture in a bigger room is a bigger picture, which is the point of a room.
        let bigger = fitted(room.expand(200.0), 1600, 800);
        assert!(bigger.width() > wide.width(), "{} is not wider than {}", bigger.width(), wide.width());
    }

    #[test]
    fn the_status_row_reads_the_run_out_of_the_stage() {
        // The row is the page's own: it prints the size, the samples, the time and the file, and it
        // says when the picture no longer matches the numbers in the rail.
        let running = readings(&run(12, 64), text::Lang::En);
        for wanted in ["1920 × 1080", "12 of 64 samples", "0:08 elapsed", "about 0:16 left", "plate.png"] {
            assert!(running.contains(wanted), "the row does not say {wanted}: {running}");
        }
        let done = readings(&run(64, 64), text::Lang::En);
        assert!(done.contains("64 of 64 samples"), "{done}");
        assert!(done.contains("written to plate.png"), "{done}");
        // A picture of another size is not the numbers above it.
        let mut matching = Settings::draft();
        matching.path = Some("plate.png".into());
        let mut same = run(4, 4);
        same.width = matching.width;
        same.height = matching.height;
        assert!(same.is_current(&matching));
        same.total = 256;
        assert!(!same.is_current(&matching), "a plate's samples are not a draft's");
    }

    #[test]
    fn the_page_paints_the_picture_letterboxed_in_its_room_at_two_window_sizes() {
        // The page takes the room the shell gives it: the picture fills it in the picture's own
        // aspect beside the rail, and a narrower window is a smaller picture, never a cropped one.
        let texture = egui::TextureId::Managed(11);
        let mut widths = Vec::new();
        for screen in [egui::vec2(1400.0, 900.0), egui::vec2(900.0, 640.0)] {
            let ctx = egui::Context::default();
            let mut settings = Settings::draft();
            settings.path = Some("plate.png".into());
            let stage = Stage::Done(run(4, 4));
            let area = egui::Rect::from_min_size(egui::Pos2::ZERO, screen);
            let input = egui::RawInput { screen_rect: Some(area), ..Default::default() };
            let output = ctx.run_ui(input, |ui| {
                show(
                    ui,
                    Page {
                        scene: "Cabinet",
                        settings: &mut settings,
                        bounds: bounds(),
                        stage: &stage,
                        picture: Some(Picture { texture, width: 1600, height: 800 }),
                    },
                );
            });
            // A test owns the frame's textures: the delta is taken and dropped here, not by a renderer.
            let mut output = output;
            std::mem::take(&mut output.textures_delta).clear();
            let painted = painted_with(&output, texture);
            assert_eq!(painted.len(), 1, "one picture is painted at {screen:?}");
            let rect = painted[0];
            let room = picture_room(area);
            let expected = fitted(room, 1600, 800);
            assert_eq!(rect.size(), expected.size(), "the picture is fitted at {screen:?}");
            assert_eq!(rect.center(), expected.center(), "and centred at {screen:?}");
            assert!(room.contains_rect(rect), "the picture is inside its room at {screen:?}");
            assert!((rect.width() / rect.height() - 2.0).abs() < 1e-3, "the picture's aspect is kept");
            widths.push(rect.width());
        }
        assert!(widths[1] < widths[0], "a narrower window is a smaller picture: {widths:?}");
    }

    #[test]
    fn the_page_draws_in_every_stage_without_a_window() {
        let stages = [
            Stage::Idle,
            Stage::Running(run(3, 8)),
            Stage::Done(run(8, 8)),
            Stage::Cancelled(run(3, 8)),
            Stage::Failed("no device".to_owned()),
        ];
        for stage in stages {
            for picture in [None, Some(picture(1920, 1080))] {
                let mut settings = Settings::draft();
                settings.path = Some("plate.png".into());
                egui::__run_test_ui(|ui| {
                    let action = show(
                        ui,
                        Page { scene: "Cabinet", settings: &mut settings, bounds: bounds(), stage: &stage, picture },
                    );
                    assert!(action.is_none(), "nothing was pressed in {stage:?}");
                });
            }
        }
    }
}
