//! The on-screen display: the diagnostic block in the free corner of the view.
//!
//! A furniture editor has no profiler a maker can read, so the program says out loud what a frame
//! costs. The block carries one line of camera state — the pitch, the yaw, the roll and the
//! projection the view is drawing with — and then the numbers that explain a scene that feels slow:
//! the frame rate and the frame time, how many objects and triangles the frame holds, and how long
//! the last rebuild, upload and pick took. Every number is measured by the code that spends it
//! ([`crate::document::Document`]) and read here on the next frame; nothing is estimated.
//!
//! The block is view state, not document data (ADR 0029): it is a switch in the display popover,
//! it survives the corner cube being turned off, and it is off by default. While it is on the shell
//! asks for a frame every frame, because a rate measured on a window that repaints only on input is
//! the idle rate, not the frame rate.
//!
//! Changes when: what the block reports, or where it hangs, changes. The words it prints live in
//! `crates/text`; the corner it hangs in belongs to [`crate::gizmo`].

use std::time::Duration;

use eframe::egui;
use egui::{FontId, Rect};
use scene::{Camera, Projection, SceneFrame};
use text::{Lang, Message};
use units::Degrees;

/// Point size of one line, matching the dimension labels of the chrome.
const POINT_SIZE: f32 = 9.5;

/// The height one line takes, in points, so the block reserves its rows before it lays them out.
pub(crate) const LINE_HEIGHT: f32 = 14.0;

/// The gap between the widget above the block and its first line, in points.
pub(crate) const GAP: f32 = 2.0;

/// How much of the newest frame the average takes.
///
/// A quarter keeps the number readable while it still follows a scene that becomes heavy, and one
/// hitch does not swing it to zero.
const RATE_WEIGHT: f64 = 0.25;

/// The scene cost of the frame on screen, in the numbers the block prints.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Cost {
    /// Drawables and line chains the uploaded frame holds.
    pub objects: usize,
    /// The triangles of its drawables.
    pub triangles: usize,
}

impl Cost {
    /// Counts a frame: one pass over its drawables, no vertex walked.
    pub(crate) fn of(frame: &SceneFrame) -> Self {
        let mut cost = Self { objects: frame.drawables().len() + frame.polylines().len(), triangles: 0 };
        for drawable in frame.drawables() {
            cost.triangles = cost.triangles.saturating_add(drawable.mesh.triangle_count());
        }
        cost
    }
}

/// The numbers the block prints, as the document measured them.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Stats {
    /// The smoothed frame time in milliseconds, `None` until a frame was measured.
    pub frame_ms: Option<f64>,
    /// The scene cost of the frame on screen.
    pub cost: Cost,
    /// How long the last rebuild took, if this scene was evaluated since it opened.
    pub rebuild: Option<Duration>,
    /// How long the last upload of the frame took in the renderer.
    pub upload: Option<Duration>,
    /// How long the last pick took.
    pub pick: Option<Duration>,
}

/// The frame meter: one smoothed frame time and the costs the document records as it works.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Meter {
    frame_ms: Option<f64>,
    rebuild: Option<Duration>,
    upload: Option<Duration>,
    pick: Option<Duration>,
}

impl Meter {
    /// Reads one frame's interval in seconds, as egui reports it.
    ///
    /// A non-finite or non-positive interval is ignored rather than averaged in: the shell measures
    /// on frames that were really drawn, so a gap of zero is a report about the clock, not a frame.
    pub(crate) fn tick(&mut self, dt: f32) {
        let ms = f64::from(dt) * 1000.0;
        if !ms.is_finite() || ms <= 0.0 {
            return;
        }
        self.frame_ms = Some(match self.frame_ms {
            Some(previous) => previous + (ms - previous) * RATE_WEIGHT,
            None => ms,
        });
    }

    /// Records how long one rebuild took.
    pub(crate) fn record_rebuild(&mut self, elapsed: Duration) {
        self.rebuild = Some(elapsed);
    }

    /// Records how long one upload of the frame took.
    pub(crate) fn record_upload(&mut self, elapsed: Duration) {
        self.upload = Some(elapsed);
    }

    /// Records how long one pick took.
    pub(crate) fn record_pick(&mut self, elapsed: Duration) {
        self.pick = Some(elapsed);
    }

    /// The numbers the block prints for `frame`.
    pub(crate) fn stats(&self, frame: &SceneFrame) -> Stats {
        Stats {
            frame_ms: self.frame_ms,
            cost: Cost::of(frame),
            rebuild: self.rebuild,
            upload: self.upload,
            pick: self.pick,
        }
    }

    /// The smoothed frame time in milliseconds, if a frame was measured.
    #[cfg(test)]
    pub(crate) fn frame_ms(&self) -> Option<f64> {
        self.frame_ms
    }
}

/// The lines the block draws, top to bottom, in `lang`.
///
/// The camera line is always there, because the block's switch is the one that names the view; the
/// others appear as soon as their number exists, so a window that has drawn no frame and picked
/// nothing prints no zero it has not measured.
pub(crate) fn lines(camera: Camera, lang: Lang, stats: Stats) -> Vec<String> {
    let mut lines = vec![camera_line(camera, lang)];
    if let Some(frame_ms) = stats.frame_ms.filter(|frame_ms| frame_ms.is_finite() && *frame_ms > 0.0) {
        let rate = (1_000.0 / frame_ms).round();
        let fps = format!("{rate:.0}");
        let millis = format!("{frame_ms:.1}");
        lines.push(Message::OsdRate.with(lang, [fps, millis]));
    }
    lines.push(Message::OsdScene.with(lang, [count(stats.cost.objects), count(stats.cost.triangles)]));
    let mut costs = Vec::new();
    for (message, elapsed) in
        [(Message::OsdRebuild, stats.rebuild), (Message::OsdUpload, stats.upload), (Message::OsdPick, stats.pick)]
    {
        if let Some(elapsed) = elapsed {
            costs.push(message.with(lang, [milliseconds(elapsed)]));
        }
    }
    if !costs.is_empty() {
        lines.push(costs.join(" · "));
    }
    lines
}

/// The pitch, yaw, roll and projection line.
///
/// Pitch and yaw are the camera's own orbit angles, wrapped to a signed degree so the line reads as
/// -51.7° rather than 308.3°; the roll is the camera's field about its own view axis, presented the
/// same way, so a 270° roll reads as -90.0°. The projection is named the way its toggle names it,
/// and every number is formatted through `units` so the line has one source of truth.
pub(crate) fn camera_line(camera: Camera, lang: Lang) -> String {
    let pitch = Degrees::from_radians(camera.pitch()).unwrap_or(Degrees::ZERO);
    let yaw = signed_degrees(camera.yaw());
    let roll = signed_degrees(camera.roll());
    let projection = match camera.projection() {
        Projection::Orthographic { .. } => Message::ProjectionOrthoShort,
        Projection::Perspective { .. } => Message::ProjectionPerspShort,
    };
    Message::OrientationReadout.with(
        lang,
        [pitch.format_fixed(1), yaw.format_fixed(1), roll.format_fixed(1), projection.text(lang).to_owned()],
    )
}

/// Wraps a yaw in radians into `(-π, π]` and returns it as degrees.
fn signed_degrees(yaw: f64) -> Degrees {
    let half_turn = std::f64::consts::PI;
    let wrapped = (yaw.rem_euclid(2.0 * half_turn) + half_turn).rem_euclid(2.0 * half_turn) - half_turn;
    Degrees::from_radians(wrapped).unwrap_or(Degrees::ZERO)
}

/// Paints the block with its last line's bottom at `bottom` and its right edge at `anchor_x`.
///
/// It is painted outside the corner widget's own rectangle, so it never takes a click; the block is
/// right-aligned to the corner it hangs in, because that corner is the view's edge and the text has
/// to grow into the scene rather than under a side panel. `clip` is the view it belongs to, so a
/// small view loses rows instead of drawing over the chrome below it.
pub(crate) fn paint(ui: &egui::Ui, clip: Rect, anchor_x: f32, bottom: f32, lines: &[String]) {
    let color = ui::theme::text_dim(ui);
    let font = FontId::proportional(POINT_SIZE);
    let painter = ui.painter().with_clip_rect(clip);
    let first = bottom - LINE_HEIGHT * lines.len() as f32;
    for (index, line) in lines.iter().enumerate() {
        let top = first + LINE_HEIGHT * index as f32;
        let galley = painter.layout_no_wrap(line.clone(), font.clone(), color);
        let position = egui::pos2(anchor_x - galley.size().x, top + (LINE_HEIGHT - galley.size().y) / 2.0);
        painter.galley(position, galley, color);
    }
}

/// The height the block takes for `count` lines, in points.
pub(crate) fn height(count: usize) -> f32 {
    LINE_HEIGHT * count as f32
}

/// `value` in milliseconds with one decimal place.
fn milliseconds(elapsed: Duration) -> String {
    format!("{:.1}", elapsed.as_secs_f64() * 1000.0)
}

/// A count with a space between thousands, so 4 800 000 triangles read at a glance.
fn count(value: usize) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(' ');
        }
        out.push(digit);
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use std::sync::Arc;

    use geom::{DMat4, MeshData};
    use model::Id;

    use super::*;

    #[test]
    fn the_rate_averages_the_frames_it_reads() {
        let mut meter = Meter::default();
        assert_eq!(meter.frame_ms(), None, "nothing is measured before a frame");
        meter.tick(0.020);
        let first = meter.frame_ms().expect("the first frame is the first measurement");
        assert!((first - 20.0).abs() < 1.0e-3, "the first frame is the first measurement: {first}");
        meter.tick(0.040);
        let smoothed = meter.frame_ms().expect("a frame was measured");
        assert!((smoothed - 25.0).abs() < 1.0e-3, "one slow frame moves the average a quarter of the way: {smoothed}");
        // A report from the clock is not a frame: zero and a NaN leave the number alone.
        meter.tick(0.0);
        meter.tick(f32::NAN);
        assert_eq!(meter.frame_ms(), Some(smoothed), "a report from the clock leaves the average alone");
    }

    #[test]
    fn the_block_names_the_frame_rate_and_the_scene_cost() {
        let mut frame = SceneFrame::default();
        for (index, triangles) in [1_usize, 2].into_iter().enumerate() {
            let Some(id) = u64::try_from(index + 1).ok().and_then(Id::new) else { continue };
            let mesh = MeshData {
                positions: vec![[0.0; 3]; triangles * 3],
                normals: Vec::new(),
                indices: (0..u32::try_from(triangles * 3).unwrap_or(0)).collect(),
            };
            frame.drawables.push(scene::Drawable::new(id, Arc::new(mesh), DMat4::IDENTITY, [1.0; 4]));
        }
        let mut meter = Meter::default();
        meter.tick(0.1);
        meter.record_rebuild(Duration::from_millis(12));
        meter.record_upload(Duration::from_millis(3));
        meter.record_pick(Duration::from_millis(161));
        let stats = meter.stats(&frame);
        assert_eq!(stats.cost, Cost { objects: 2, triangles: 3 });

        let text = lines(Camera::default(), Lang::En, stats).join("\n");
        assert!(text.contains("10 fps"), "the rate is the measured frame time: {text}");
        assert!(text.contains("100.0 ms"), "and its own frame time: {text}");
        assert!(text.contains("2 objects"), "the block names the frame's objects: {text}");
        assert!(text.contains("3 triangles"), "and its triangles: {text}");
        assert!(text.contains("rebuild 12.0 ms"), "the rebuild is measured where it runs: {text}");
        assert!(text.contains("pick 161.0 ms"), "so is the pick: {text}");
    }

    #[test]
    fn a_measured_number_is_printed_and_an_unmeasured_one_is_not() {
        let stats = Stats { cost: Cost { objects: 0, triangles: 0 }, ..Stats::default() };
        let text = lines(Camera::default(), Lang::En, stats).join("\n");
        assert!(!text.contains("fps"), "no frame was measured, so no rate is claimed: {text}");
        assert!(!text.contains("pick"), "and no pick is claimed: {text}");
        assert!(text.contains("0 objects"), "the empty frame is still named: {text}");
    }

    #[test]
    fn a_yaw_is_presented_as_a_signed_angle() {
        assert_eq!(signed_degrees(0.0), Degrees::ZERO);
        assert_eq!(signed_degrees(std::f64::consts::PI).format_fixed(1), "-180.0°", "a half turn wraps to -180°");
        assert_eq!(signed_degrees(-std::f64::consts::FRAC_PI_2).format_fixed(1), "-90.0°");
        // 308° is -52° once wrapped, which is what the user reads.
        let wrapped = signed_degrees(308.0_f64.to_radians());
        assert!((wrapped.degrees_f64() + 52.0).abs() < 1.0e-6, "wrapped to {wrapped}");
    }

    #[test]
    fn the_camera_line_names_the_angles_and_the_projection() {
        let mut camera = Camera::default();
        camera.look_from(geom::DVec3::new(0.6, 0.8, 0.5));
        assert!(matches!(camera.projection(), Projection::Perspective { .. }));
        let text = camera_line(camera, Lang::En);
        assert!(text.starts_with("Pitch "), "the line starts with the pitch: {text}");
        assert!(text.contains(" · Yaw "), "and continues with the yaw: {text}");
        assert!(text.contains(" · Roll "), "and the roll: {text}");
        assert!(text.ends_with("Persp"), "a perspective view names the mode: {text}");

        assert!(camera.toggle_projection().kept());
        assert!(camera_line(camera, Lang::En).ends_with("Ortho"), "an orthographic view names the mode");

        camera.roll_to(std::f64::consts::FRAC_PI_2);
        assert!(camera_line(camera, Lang::En).contains("Roll 90.0°"), "the line shows the camera's roll");
    }

    #[test]
    fn a_count_is_grouped_by_thousands() {
        assert_eq!(count(0), "0");
        assert_eq!(count(999), "999");
        assert_eq!(count(1_000), "1 000");
        assert_eq!(count(4_800_000), "4 800 000");
    }

    #[test]
    fn the_block_is_drawn_only_where_its_switch_asks() {
        // The block is paint and nothing else: it lays its rows out from the bottom it is given and
        // never takes a click, so an empty list draws nothing at all.
        let ctx = egui::Context::default();
        ui::theme::install(&ctx);
        let viewport = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let mut texts = 0;
        let output = ctx.run_ui(Default::default(), |ui| {
            paint(ui, viewport, viewport.right(), viewport.top() + 40.0, &[]);
        });
        fn walk(shape: &egui::Shape, out: &mut usize) {
            match shape {
                egui::Shape::Text(_) => *out += 1,
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        walk(shape, out);
                    }
                }
                _ => {}
            }
        }
        for clipped in &output.shapes {
            walk(&clipped.shape, &mut texts);
        }
        output.drop_without_applying_deltas();
        assert_eq!(texts, 0, "no line, no text");

        let drawn = egui::Context::default();
        ui::theme::install(&drawn);
        let mut texts = 0;
        let output = drawn.run_ui(Default::default(), |ui| {
            let lines = vec!["first".to_owned(), "second".to_owned()];
            paint(ui, viewport, viewport.right(), viewport.top() + height(lines.len()), &lines);
        });
        for clipped in &output.shapes {
            walk(&clipped.shape, &mut texts);
        }
        output.drop_without_applying_deltas();
        assert_eq!(texts, 2, "one line of text per row the block was given");
    }

    #[test]
    fn the_block_hangs_right_aligned_to_its_corner() {
        let ctx = egui::Context::default();
        ui::theme::install(&ctx);
        let viewport = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let corner = viewport.right() - 12.0;
        let output = ctx.run_ui(Default::default(), |ui| {
            let lines = vec!["wide line of text".to_owned(), "narrow".to_owned()];
            paint(ui, viewport, corner, viewport.top() + height(lines.len()), &lines);
        });
        let mut right = f32::NEG_INFINITY;
        let mut top = f32::INFINITY;
        for clipped in &output.shapes {
            if let egui::Shape::Text(text) = &clipped.shape {
                right = right.max(text.pos.x + text.galley.size().x);
                top = top.min(text.pos.y);
            }
        }
        output.drop_without_applying_deltas();
        assert!((right - corner).abs() < 0.5, "both rows end at the corner: {right} vs {corner}");
        assert!(top >= viewport.top(), "the block starts inside the view: {top}");
    }
}
