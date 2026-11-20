//! The work area's own pointer: a two-tone square the shell paints only over the scene.
//!
//! The system arrow points at a button, not at the place a click lands on, so the shell hides it
//! while the pointer is inside the scene rectangle and draws its own square at the point instead
//! (X41). Every panel, floating toolbar, text field, resizing border and menu keeps the system
//! cursor: that icon is the affordance of a system element, and a custom square over it hides what
//! the cursor is for. The square and the crosshair are painted on the scene layer, under the
//! floating chrome, so a card or a menu masks the guide instead of being crossed by it.
//!
//! The outline is two strokes of different value, dark outside and light inside, so one of them reads
//! on a white board, a dark board and a shaded face alike. The pointer is chrome, not a guide: it
//! takes the fixed pair of [`ui::theme::cursor_inks`] rather than the amber of ADR 0111.
//!
//! Changes when: the boundary of the work area, or the rule for a system element, changes. The size
//! and the crosshair switch belong to the workspace settings.

use eframe::egui;
use ui::settings::PointerSettings;
use ui::theme;

/// The width of the dark outer stroke, in points.
const OUTER_STROKE: f32 = 3.0;

/// The width of the light inner stroke, in points.
const INNER_STROKE: f32 = 1.0;

/// The side of the filled centre dot, in points.
const DOT: f32 = 2.0;

/// The layer the square and the crosshair are painted on: the scene, under every chrome surface.
///
/// A painter on `Background` adds a non-area layer after the central panel, so the guide and the
/// square lie over the image and its gizmos but under every `Middle` panel, toolbar and cluster and
/// every `Foreground` menu or modal (ADR 0123). A layer painter adds shapes and no interaction, so
/// [`egui::Context::layer_id_at`] never reports it.
fn layer() -> egui::LayerId {
    egui::LayerId::new(egui::Order::Background, egui::Id::unique("z3l_work_cursor"))
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
        let side = size.clamp(ui::settings::MIN_CURSOR_POINTS as f32, ui::settings::MAX_CURSOR_POINTS as f32);
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

/// Draws the work area's pointer for this frame and returns the geometry it used.
///
/// `scene` is the rectangle the work area owns: outside it, and over a `Middle`/`Foreground` surface
/// (a panel, the floating cluster, a popup, a modal), a widget with its own icon, the corner
/// orientation widget or a resizing border, the system cursor stays. `widget` is that corner
/// widget's rectangle, which is drawn on the scene layer and so cannot be found by the layer test.
pub(crate) fn show(
    ctx: &egui::Context,
    settings: &PointerSettings,
    scene: Option<egui::Rect>,
    widget: Option<egui::Rect>,
    hidden: bool,
    hand: Option<egui::Pos2>,
) -> Option<WorkCursor> {
    // The carry owns the hand while it is displaced: a camera gesture moved the view and not the
    // piece, so the square is drawn on the piece and the mouse moves it by its own movement (X69).
    let pointer = hand.or_else(|| ctx.pointer_hover_pos())?;
    let scene = scene?;
    if !scene.contains(pointer) || over_system_surface(ctx, pointer, widget) {
        return None;
    }
    // A pointer the carry has put away: a camera gesture under a piece in hand moves the world and
    // not the piece, and a square standing still over the moving scene is the picture of a piece
    // being left behind. The system arrow stays hidden all the same, so nothing shows at all
    // (ADR 0156, X69).
    if hidden {
        ctx.set_cursor_icon(egui::CursorIcon::None);
        return None;
    }
    let cursor = WorkCursor::new(pointer, settings.size_points(), settings.crosshair, scene);
    ctx.set_cursor_icon(egui::CursorIcon::None);
    paint(ctx, &cursor, scene);
    Some(cursor)
}

/// Whether the pointer is over a system surface rather than the work area.
///
/// The scene is the `Background` layer. A panel, the floating cluster, the command bar, a distance
/// field and every menu or modal is a `Middle` or `Foreground` layer; the corner orientation widget
/// is drawn on the scene layer, so its rectangle is checked by hand. A widget that set an icon of
/// its own (a text field's I-beam, a divider's resize arrows, a scrub's hidden pointer) keeps it.
fn over_system_surface(ctx: &egui::Context, pointer: egui::Pos2, widget: Option<egui::Rect>) -> bool {
    if widget.is_some_and(|rect| rect.contains(pointer)) {
        return true;
    }
    let work_area = ctx.layer_id_at(pointer).is_some_and(|layer| layer.order == egui::Order::Background);
    let owned = ctx.output(|output| output.cursor_icon) != egui::CursorIcon::Default;
    !work_area || owned
}

/// Paints the square, the centre dot and the crosshair in the two fixed inks.
///
/// Everything is clipped to the work area: the pointer may sit a few points from a panel, and the
/// square must not spill onto a system surface it does not own (X41).
fn paint(ctx: &egui::Context, cursor: &WorkCursor, view: egui::Rect) {
    let painter = ctx.layer_painter(layer()).with_clip_rect(view);
    let (dark, light) = theme::cursor_inks();
    painter.rect_stroke(cursor.square, 0.0, egui::Stroke::new(OUTER_STROKE, dark), egui::StrokeKind::Middle);
    painter.rect_stroke(cursor.square, 0.0, egui::Stroke::new(INNER_STROKE, light), egui::StrokeKind::Middle);
    let halo = (OUTER_STROKE - INNER_STROKE) / 2.0;
    painter.rect_filled(cursor.dot.expand(halo), 0.0, dark);
    painter.rect_filled(cursor.dot, 0.0, light);
    if let Some([horizontal, vertical]) = cursor.crosshair {
        for arm in [horizontal, vertical] {
            painter.rect_filled(arm.expand(halo), 0.0, dark);
            painter.rect_filled(arm, 0.0, light);
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
            let limit = size.clamp(ui::settings::MIN_CURSOR_POINTS as f32, ui::settings::MAX_CURSOR_POINTS as f32);
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
    fn the_work_area_paint_sits_under_every_chrome_surface() {
        assert_eq!(
            layer().order,
            egui::Order::Background,
            "the guide and the square are scene paint: every Middle panel and Foreground menu must cover them"
        );
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
            shown = show(ui.ctx(), &settings, Some(screen), None, false, None);
        });
        assert_eq!(output.platform_output.cursor_icon, egui::CursorIcon::Text, "the field owns the pointer");
        assert!(shown.is_none(), "the square must not cover the I-beam it would replace");
        output.drop_without_applying_deltas();
    }
}
