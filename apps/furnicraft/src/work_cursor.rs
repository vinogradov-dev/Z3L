//! The window's own pointer: a two-tone square the shell paints over every surface but a menu.
//!
//! The system arrow points at a button, not at the place a click lands on, so the window hides it
//! while the pointer is over the interface and draws its own square at the point instead (X40). The
//! square is painted on the `Tooltip` layer after every panel, toolbar, popup and dialog has drawn,
//! so the bottom cluster and the status bar cannot cover it; a menu, a popup or a modal is a
//! `Foreground` layer, where the system arrow comes back and no square is painted.
//!
//! The outline is two strokes of different value, dark outside and light inside, so one of them reads
//! on a white board, a dark board and a shaded face alike. The pointer is chrome, not a guide: it
//! takes the fixed pair of [`furnicraft_ui::theme::cursor_inks`] rather than the amber of ADR 0111.
//!
//! Changes when: what a pointer must say on a surface changes, or a surface joins the two that may
//! ask for the system arrow. The size and the crosshair switch belong to the workspace settings.

use eframe::egui;
use furnicraft_ui::settings::PointerSettings;
use furnicraft_ui::theme;

/// The width of the dark outer stroke, in points.
const OUTER_STROKE: f32 = 3.0;

/// The width of the light inner stroke, in points.
const INNER_STROKE: f32 = 1.0;

/// The side of the filled centre dot, in points.
const DOT: f32 = 2.0;

/// The layer the square is painted on: above every panel, toolbar, popup and modal.
///
/// A layer painter adds shapes and no interaction, so the pointer stays clickable under the square
/// and [`egui::Context::layer_id_at`] never reports this layer.
fn layer() -> egui::LayerId {
    egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("furnicraft_work_cursor"))
}

/// The screen geometry the work cursor draws at the pointer.
///
/// The square and the dot are centred on the pointer; the crosshair arms, when the switch is on,
/// span the scene rectangle so a part lines up with a neighbour out of sight. Coordinates are screen
/// points, and the paint takes no click.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct WorkCursor {
    /// The square, centred on the pointer.
    pub square: egui::Rect,
    /// The filled light dot in the square's centre.
    pub dot: egui::Rect,
    /// The crosshair arms, or `None` while the switch is off.
    pub crosshair: Option<[egui::Rect; 2]>,
}

impl WorkCursor {
    /// The cursor centred on `pointer`, `size` points across, inside the scene `view`.
    ///
    /// The size is clamped here as well as in the workspace file, so a caller that has not read the
    /// settings through [`PointerSettings::size_points`] still draws a square a hand can see and
    /// grab.
    pub(crate) fn new(pointer: egui::Pos2, size: f32, crosshair: bool, view: egui::Rect) -> Self {
        let side = size.clamp(
            furnicraft_ui::settings::MIN_CURSOR_POINTS as f32,
            furnicraft_ui::settings::MAX_CURSOR_POINTS as f32,
        );
        Self {
            square: egui::Rect::from_center_size(pointer, egui::vec2(side, side)),
            dot: egui::Rect::from_center_size(pointer, egui::vec2(DOT, DOT)),
            crosshair: crosshair.then(|| {
                // An arm is a filled rectangle a stroke wide: a zero-height line draws nothing, and
                // the pair must read as one guide through the pointer.
                let half = INNER_STROKE / 2.0;
                [
                    egui::Rect::from_min_max(
                        egui::pos2(view.left(), pointer.y - half),
                        egui::pos2(view.right(), pointer.y + half),
                    ),
                    egui::Rect::from_min_max(
                        egui::pos2(pointer.x - half, view.top()),
                        egui::pos2(pointer.x + half, view.bottom()),
                    ),
                ]
            }),
        }
    }
}

/// Draws the shell's pointer for this frame and returns the geometry it used.
///
/// The square is the window's default: it is drawn wherever the pointer is, except over a menu, a
/// popup or a modal (a `Foreground` layer, where the system arrow picks a row) and over a widget
/// that asked for a cursor of its own (a text field, a divider, a scrub). `view` is the scene
/// rectangle the crosshair spans; the square itself is only clipped by the window.
pub(crate) fn show(ctx: &egui::Context, settings: &PointerSettings, view: Option<egui::Rect>) -> Option<WorkCursor> {
    let pointer = ctx.pointer_hover_pos()?;
    if asked_for_system_cursor(ctx, pointer) {
        return None;
    }
    let window = ctx.content_rect();
    let view = view.unwrap_or(window);
    let cursor = WorkCursor::new(pointer, settings.size_points(), settings.crosshair, view);
    ctx.set_cursor_icon(egui::CursorIcon::None);
    paint(ctx, &cursor, view);
    Some(cursor)
}

/// Whether a surface above the scene owns the pointer this frame.
///
/// A menu, a popup and a modal are `Foreground` layers: the square would cover the row it is meant
/// to pick, so the system arrow comes back. A widget that set an icon of its own (a text field's
/// I-beam, a divider's resize arrows, a scrub's hidden pointer) keeps it.
fn asked_for_system_cursor(ctx: &egui::Context, pointer: egui::Pos2) -> bool {
    let menu = ctx.layer_id_at(pointer).is_some_and(|layer| layer.order >= egui::Order::Foreground);
    let owned = ctx.output(|output| output.cursor_icon) != egui::CursorIcon::Default;
    menu || owned
}

/// Paints the square, the centre dot and the crosshair in the two fixed inks.
///
/// The square and the dot are clipped by the window, because the pointer has to stay visible over
/// the toolbars; the crosshair is clipped to the scene rectangle, because it is a guide across the
/// view and not a line across the chrome.
fn paint(ctx: &egui::Context, cursor: &WorkCursor, view: egui::Rect) {
    let painter = ctx.layer_painter(layer());
    let (dark, light) = theme::cursor_inks();
    painter.rect_stroke(cursor.square, 0.0, egui::Stroke::new(OUTER_STROKE, dark), egui::StrokeKind::Middle);
    painter.rect_stroke(cursor.square, 0.0, egui::Stroke::new(INNER_STROKE, light), egui::StrokeKind::Middle);
    let halo = (OUTER_STROKE - INNER_STROKE) / 2.0;
    painter.rect_filled(cursor.dot.expand(halo), 0.0, dark);
    painter.rect_filled(cursor.dot, 0.0, light);
    if let Some([horizontal, vertical]) = cursor.crosshair {
        let guide = painter.with_clip_rect(view);
        for arm in [horizontal, vertical] {
            guide.rect_filled(arm.expand(halo), 0.0, dark);
            guide.rect_filled(arm, 0.0, light);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The centre of a point on the screen, so a test names the pointer and not the layout.
    fn centre() -> egui::Pos2 {
        egui::pos2(320.0, 240.0)
    }

    fn view() -> egui::Rect {
        egui::Rect::from_min_max(egui::pos2(100.0, 80.0), egui::pos2(540.0, 400.0))
    }

    #[test]
    fn the_square_and_dot_stay_centred_between_the_size_limits() {
        for size in [0.0, 6.0, 12.0, 32.0, 64.0] {
            let cursor = WorkCursor::new(centre(), size, false, view());
            assert_eq!(cursor.square.center(), centre(), "the square is centred on the pointer at {size} pt");
            assert_eq!(cursor.dot.center(), centre(), "the dot is centred at {size} pt");
            let limit = size.clamp(
                furnicraft_ui::settings::MIN_CURSOR_POINTS as f32,
                furnicraft_ui::settings::MAX_CURSOR_POINTS as f32,
            );
            assert_eq!(cursor.square.width(), limit, "the square is the clamped setting at {size} pt");
        }
    }

    #[test]
    fn the_crosshair_spans_the_scene_and_keeps_the_pointer_in_its_centre() {
        let cursor = WorkCursor::new(centre(), 12.0, true, view());
        let [horizontal, vertical] = cursor.crosshair.expect("the switch draws both arms");
        assert_eq!((horizontal.left(), horizontal.right()), (view().left(), view().right()));
        assert_eq!((vertical.top(), vertical.bottom()), (view().top(), view().bottom()));
        assert_eq!(horizontal.center().y, centre().y);
        assert_eq!(vertical.center().x, centre().x);
        assert!(horizontal.height() > 0.0 && vertical.width() > 0.0, "an arm is a stroke wide");
    }

    #[test]
    fn the_two_inks_are_at_opposite_ends_of_the_brightness_range() {
        let (dark, light) = theme::cursor_inks();
        let luminance = |color: egui::Color32| {
            let channel = |value: u8| {
                let value = f32::from(value) / 255.0;
                if value <= 0.04045 { value / 12.92 } else { ((value + 0.055) / 1.055).powf(2.4) }
            };
            0.2126 * channel(color.r()) + 0.7152 * channel(color.g()) + 0.0722 * channel(color.b())
        };
        assert_ne!(dark, light, "one ink would vanish on a surface of its own value");
        assert!(luminance(dark) < 0.05, "the outer stroke is near black: {}", luminance(dark));
        assert!(luminance(light) > 0.9, "the inner stroke is near white: {}", luminance(light));
    }

    #[test]
    fn the_pointer_is_painted_above_every_foreground_surface() {
        assert_eq!(layer().order, egui::Order::Tooltip, "the square must lie over menus and chrome");
    }

    #[test]
    fn a_text_field_keeps_its_own_ibeam() {
        let ctx = egui::Context::default();
        let settings = PointerSettings::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(200.0, 100.0));
        let pointer = egui::pos2(80.0, 40.0);
        let mut text = String::new();
        let input = || egui::RawInput {
            screen_rect: Some(screen),
            events: vec![egui::Event::PointerMoved(pointer)],
            ..Default::default()
        };
        // The first frame gives the field a rectangle; the second one hovers it and sets the icon.
        let warm = ctx.run_ui(input(), |ui| {
            let field = egui::Rect::from_min_size(egui::pos2(20.0, 20.0), egui::vec2(160.0, 24.0));
            ui.put(field, egui::TextEdit::singleline(&mut text));
        });
        warm.drop_without_applying_deltas();
        let mut shown = None;
        let output = ctx.run_ui(input(), |ui| {
            let field = egui::Rect::from_min_size(egui::pos2(20.0, 20.0), egui::vec2(160.0, 24.0));
            ui.put(field, egui::TextEdit::singleline(&mut text));
            shown = show(ui.ctx(), &settings, Some(screen));
        });
        assert_eq!(output.platform_output.cursor_icon, egui::CursorIcon::Text, "the field owns the pointer");
        assert!(shown.is_none(), "the square must not cover the I-beam it would replace");
        output.drop_without_applying_deltas();
    }
}
