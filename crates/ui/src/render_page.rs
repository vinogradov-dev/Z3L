//! The Render place: one still image of the scene in front, drawn by the final render.
//!
//! This is a place, not a question: a render runs for minutes and the person may go back to the tree
//! while it does, so it gets a surface of its own — the `Render` tab beside the File menu — instead
//! of a modal that holds the window. The page owns the numbers (size, samples, exposure and the file
//! to write) and nothing else: the frame, the device, the worker thread and the file manager belong
//! to the shell, which hands this module a [`Page`] and performs the one [`Action`] it returns.
//!
//! The brief: a furniture maker wants the plate of the cabinet in front, at a size a workshop prints,
//! without leaving the editor. **Render** is the one primary action; the presets exist because a
//! draft to check the view and a plate for print are the two things a person actually chooses, and
//! the numbers stay editable beside them so a plate that is not either preset is still one field
//! away. Cancel replaces Render while a run is in flight, and nothing else on the page can be
//! pressed into a second run.
//!
//! Refuses: a size, a sample count or a pixel count outside the bounds the shell hands in (they are
//! the final render's own caps, so the field cannot offer what the tracer would refuse), and a render
//! with no destination. A refused number is clamped by the field itself and the button says why it
//! cannot run, rather than a run that fails after a minute.
//!
//! Changes when: what a render needs to be asked, or what it reports while it runs, changes.

use core::time::Duration;

use egui::{RichText, Ui};
use text::Message;

use crate::scrub::{CommitPolicy, Scrub};
use crate::theme;

/// The width of the form's one column, in points: a field and its unit on one line, and a path that
/// elides instead of pushing the row wide.
const COLUMN_WIDTH: f32 = 560.0;

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
    pub path: Option<std::path::PathBuf>,
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

/// Where the run is, as the page draws it.
#[derive(Clone, Debug, PartialEq)]
pub enum Stage {
    /// Nothing is running: the page offers its numbers and its Render button.
    Idle,
    /// A run is in flight; the page draws its progress and a Cancel.
    Running {
        /// How many samples are traced.
        done: u32,
        /// How many the request asks for.
        total: u32,
    },
    /// The image was written.
    Done {
        /// The file that was written.
        path: std::path::PathBuf,
        /// The image width in pixels.
        width: u32,
        /// The image height in pixels.
        height: u32,
        /// The samples each pixel took.
        samples: u32,
    },
    /// The person stopped the run, and nothing was written.
    Cancelled,
    /// The run refused, with the sentence that says why.
    Failed(String),
}

impl Stage {
    /// Whether a run is in flight, which is what a surface polls before it starts another.
    pub fn is_running(&self) -> bool {
        matches!(self, Self::Running { .. })
    }
}

/// One page: the scene it is about, the settings it edits and where the last run got to.
pub struct Page<'a> {
    /// The scene the image will be of, by the name the tab shows.
    pub scene: &'a str,
    /// The numbers and the destination, edited in place.
    pub settings: &'a mut Settings,
    /// The limits the numbers are kept inside.
    pub bounds: Bounds,
    /// Where the last run got to.
    pub stage: &'a Stage,
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
/// The area is filled — the place replaces the scene rather than floating over it — and the form
/// keeps one measured column so a field never stretches across a wide window.
pub fn show(ui: &mut Ui, page: Page<'_>) -> Option<Action> {
    let lang = crate::language(ui.ctx());
    let mut action = None;
    let Page { scene, settings, bounds, stage } = page;
    settings.clamp(bounds);
    let area = ui.available_rect_before_wrap().intersect(ui.clip_rect());
    egui::Frame::new().fill(theme::panel(ui)).show(ui, |ui| {
        ui.set_min_size(area.size());
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.add_space(theme::STEP * 4.0);
            ui.horizontal(|ui| {
                ui.add_space(theme::STEP * 4.0);
                ui.vertical(|ui| {
                    ui.set_max_width(COLUMN_WIDTH);
                    ui.heading(Message::RenderTitle.text(lang));
                    ui.label(RichText::new(Message::RenderOf.with(lang, [scene])).color(theme::text_dim(ui)));
                    ui.add_space(theme::STEP * 2.0);
                    if let Some(chosen) = numbers_column(ui, settings, bounds, stage, lang) {
                        action = Some(chosen);
                    }
                    ui.add_space(theme::STEP * 3.0);
                    destination(ui, settings, &mut action, lang);
                    ui.add_space(theme::STEP * 3.0);
                    run_row(ui, settings, stage, &mut action, lang);
                    ui.add_space(theme::STEP * 2.0);
                    report(ui, stage, lang);
                });
            });
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
        // A path is long and the row is one line wide: it elides, and the full path stays in the
        // hint a person reads before pressing Render.
        ui.scope(|ui| {
            ui.set_max_width(COLUMN_WIDTH - 96.0);
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

/// What the last run did, and how far the current one is.
fn report(ui: &mut Ui, stage: &Stage, lang: text::Lang) {
    match stage {
        Stage::Idle => {}
        Stage::Running { done, total } => {
            let fraction = if *total == 0 { 0.0 } else { *done as f32 / *total as f32 };
            ui.add(egui::ProgressBar::new(fraction).show_percentage());
            ui.label(
                RichText::new(Message::RenderSamplesDone.with(lang, [done.to_string(), total.to_string()]))
                    .color(theme::text_dim(ui)),
            );
        }
        Stage::Done { path, width, height, samples } => {
            ui.label(
                Message::RenderDone
                    .with(lang, [path.display().to_string(), format!("{width} × {height}"), samples.to_string()]),
            );
        }
        Stage::Cancelled => {
            ui.label(Message::RenderCancelled.text(lang));
        }
        Stage::Failed(reason) => {
            ui.label(RichText::new(Message::RenderFailed.with(lang, [reason.clone()])).color(theme::danger(ui)));
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    fn bounds() -> Bounds {
        Bounds { min_side: 16, max_side: 8192, min_samples: 1, max_samples: 4096, max_pixels: 16_000_000 }
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
    fn the_page_draws_in_every_stage_without_a_window() {
        let stages = [
            Stage::Idle,
            Stage::Running { done: 3, total: 8 },
            Stage::Done { path: "plate.png".into(), width: 2480, height: 3508, samples: 256 },
            Stage::Cancelled,
            Stage::Failed("no device".to_owned()),
        ];
        for stage in stages {
            let mut settings = Settings::draft();
            settings.path = Some("plate.png".into());
            egui::__run_test_ui(|ui| {
                let action =
                    show(ui, Page { scene: "Cabinet", settings: &mut settings, bounds: bounds(), stage: &stage });
                assert!(action.is_none(), "nothing was pressed in {stage:?}");
            });
        }
    }
}
