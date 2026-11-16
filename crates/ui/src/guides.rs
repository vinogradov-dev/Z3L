//! The two guides a sketch draws over the model: the construction line and the reference line.
//!
//! A drawing operation measures one of two things and both need to be seen before the click. The
//! **construction line** is the chord that is being measured — an arc's start to its end, a node's
//! old position to the one under the pointer — and it is the guide amber, dashed, at the weight of
//! a guide (ADR 0111). The **reference line** is the zero the angle is read from and the sweep that
//! shows it: a dotted line from the anchor along the X end at the radius of the segment, and the arc
//! of that same radius up to the direction under the pointer. It is drawn as two dots — light inside
//! dark — because it lies over an arbitrary board and no themed colour reads on all of them
//! (ADR 0127); the dot is large enough to find, because the reference is the only thing a sketch
//! shows before its first click (ADR 0133).
//!
//! The module takes screen points: the tool maps its work plane and camera to the screen, so the
//! guides never see a camera, a plane or a document, and a stroke is a rule in points rather than a
//! world size (ADR 0111). The reference arrives as projected polylines rather than as an angle and a
//! screen radius, so a plane seen at an angle draws the ellipse it really is (ADR 0133).
//!
//! Changes when: a guide gains a kind, or its weight, dash or dot pattern changes.

use egui::{Color32, Painter, Pos2, Shape, Stroke, Ui};

use crate::theme;

/// One dash of a construction line, in points.
const CONSTRUCTION_DASH: f32 = 6.0;

/// The gap after one dash of a construction line, in points.
const CONSTRUCTION_GAP: f32 = 4.0;

/// Paints the construction line between two screen points: the chord being measured.
///
/// The line is the guide amber and dashed, so it reads as the program's own help rather than as a
/// form edge, and it is never mistaken for the geometry that will be created.
pub fn construction(ui: &Ui, painter: &Painter, from: Pos2, to: Pos2) {
    let stroke = Stroke::new(theme::HELPER_STROKE, theme::helper(ui));
    painter.extend(Shape::dashed_line(&[from, to], stroke, CONSTRUCTION_DASH, CONSTRUCTION_GAP));
}

/// Paints the reference line: the axis from the anchor and the sweep that shows the angle.
///
/// Both polylines are the work plane's points already projected by the caller, so the axis follows
/// the projection of its own direction and the sweep follows the projected circle — an ellipse in
/// perspective or axonometric view — instead of a screen-space circle struck from the anchor
/// (ADR 0133). The sweep's direction is read from the nearer end of the X axis (ADR 0128); the
/// caller passes the short way round.
///
/// A path with fewer than two finite points draws nothing: there is no direction to show yet.
pub fn reference(painter: &Painter, axis: &[Pos2], sweep: &[Pos2]) {
    let (dark, light) = theme::reference_inks();
    dots(painter, axis, dark, light);
    dots(painter, sweep, dark, light);
}

/// Paints one two-tone dot line: the dark dots first, then the light centre inside them.
fn dots(painter: &Painter, path: &[Pos2], dark: Color32, light: Color32) {
    if path.len() < 2 || path.iter().any(|point| !point.x.is_finite() || !point.y.is_finite()) {
        return;
    }
    painter.extend(Shape::dotted_line(path, dark, theme::REFERENCE_SPACING, theme::REFERENCE_DOT));
    painter.extend(Shape::dotted_line(path, light, theme::REFERENCE_SPACING, theme::REFERENCE_DOT_LIGHT));
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use egui::{Pos2, Rect, pos2};

    use super::*;

    const VIEWPORT: Rect = Rect { min: pos2(0.0, 0.0), max: pos2(800.0, 600.0) };

    /// Every painted circle's centre and fill colour.
    fn circles(shapes: &[egui::epaint::ClippedShape]) -> Vec<(Pos2, Color32, f32)> {
        fn collect(shape: &Shape, out: &mut Vec<(Pos2, Color32, f32)>) {
            match shape {
                Shape::Circle(circle) => out.push((circle.center, circle.fill, circle.radius)),
                Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect(shape, out);
                    }
                }
                _ => {}
            }
        }
        let mut out = Vec::new();
        for clipped in shapes {
            collect(&clipped.shape, &mut out);
        }
        out
    }

    #[test]
    fn the_reference_draws_the_projected_paths_it_is_given() {
        // The tool projects every sample of the plane's axis and sweep, so the guide is those two
        // polylines and nothing else: no angle and no screen radius reach this module (ADR 0133).
        let ctx = egui::Context::default();
        let anchor = pos2(200.0, 300.0);
        let axis = [anchor, pos2(320.0, 300.0)];
        let quarter: Vec<Pos2> = (0..=90)
            .map(|step| {
                let angle = std::f32::consts::FRAC_PI_2 * step as f32 / 90.0;
                pos2(anchor.x + 120.0 * angle.cos(), anchor.y - 120.0 * angle.sin())
            })
            .collect();
        let mut shapes = Vec::new();
        let output = ctx.run_ui(egui::RawInput { screen_rect: Some(VIEWPORT), ..Default::default() }, |ui| {
            reference(ui.painter(), &axis, &quarter);
        });
        shapes.extend(output.shapes.clone());
        output.drop_without_applying_deltas();
        let (dark, light) = theme::reference_inks();
        let dots = circles(&shapes);
        assert!(
            dots.iter().any(|(centre, fill, _)| *fill == dark && *centre == anchor),
            "the axis starts at the anchor"
        );
        let reach = dots.iter().map(|(centre, _, _)| centre.x).fold(anchor.x, f32::max);
        assert!(reach >= anchor.x + 120.0 - theme::REFERENCE_SPACING, "the axis reaches its end: {reach}");
        assert!(
            dots.iter().any(|(centre, _, _)| centre.y < anchor.y - 100.0),
            "the sweep follows the projected quarter turn: {dots:?}"
        );
        assert!(dots.iter().all(|(_, _, radius)| *radius <= theme::REFERENCE_DOT), "no dot is a line");
        assert!(dots.iter().any(|(_, fill, _)| *fill == light), "the light centre is inside the dark dot");
    }

    #[test]
    fn a_reference_path_with_no_direction_draws_nothing() {
        let ctx = egui::Context::default();
        let anchor = pos2(200.0, 300.0);
        let cases: [(&[Pos2], &[Pos2]); 3] = [
            (&[], &[]),
            (&[anchor], &[anchor]),
            (&[anchor, pos2(f32::NAN, 300.0)], &[anchor, pos2(200.0, f32::INFINITY)]),
        ];
        for (axis, sweep) in cases {
            let mut shapes = Vec::new();
            let output = ctx.run_ui(egui::RawInput { screen_rect: Some(VIEWPORT), ..Default::default() }, |ui| {
                reference(ui.painter(), axis, sweep);
            });
            shapes.extend(output.shapes.clone());
            output.drop_without_applying_deltas();
            assert!(circles(&shapes).is_empty(), "{axis:?} / {sweep:?} draws nothing");
        }
    }

    #[test]
    fn the_construction_line_is_a_dashed_guide_in_the_guide_ink() {
        let ctx = egui::Context::default();
        let mut ink = Color32::TRANSPARENT;
        let output = ctx.run_ui(egui::RawInput { screen_rect: Some(VIEWPORT), ..Default::default() }, |ui| {
            construction(ui, ui.painter(), pos2(20.0, 20.0), pos2(220.0, 20.0));
            ink = theme::helper(ui);
        });
        fn collect(shape: &Shape, strokes: &mut usize, colours: &mut Vec<Color32>) {
            match shape {
                Shape::Path(path) => {
                    *strokes += path.points.len().saturating_sub(1);
                    if let egui::epaint::ColorMode::Solid(colour) = path.stroke.color {
                        colours.push(colour);
                    }
                }
                Shape::LineSegment { points, stroke } => {
                    *strokes += 1;
                    let _ = points;
                    colours.push(stroke.color);
                }
                Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect(shape, strokes, colours);
                    }
                }
                _ => {}
            }
        }
        let mut strokes = 0;
        let mut colours = Vec::new();
        for clipped in &output.shapes {
            collect(&clipped.shape, &mut strokes, &mut colours);
        }
        output.drop_without_applying_deltas();
        assert!(strokes > 1, "a dashed line is several strokes, not one: {strokes}");
        assert!(!colours.is_empty(), "the dashes were painted");
        assert!(colours.iter().all(|colour| *colour == ink), "every dash is the guide ink");
    }
}
