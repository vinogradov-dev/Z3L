//! The two guides a sketch draws over the model: the construction line and the reference line.
//!
//! A drawing operation measures one of two things and both need to be seen before the click. The
//! **construction line** is the chord that is being measured — an arc's start to its end, a node's
//! old position to the one under the pointer — and it is the guide amber, dashed, at the weight of
//! a guide (ADR 0111). The **reference line** is the zero the angle is read from and the sweep that
//! shows it: a dot line from the anchor along +X at the radius of the segment, and an arc of that
//! same radius up to the direction under the pointer. It is the thinnest mark in the scene, drawn as
//! two dots — light inside dark — because it lies over an arbitrary board and no themed colour reads
//! on all of them (ADR 0127).
//!
//! The module takes screen points: the tool maps its work plane and camera to the screen, so the
//! guides never see a camera, a plane or a document, and a stroke is a rule in points rather than a
//! world size (ADR 0111).
//!
//! Changes when: a guide gains a kind, or its weight, dash or dot pattern changes.

use egui::{Color32, Painter, Pos2, Shape, Stroke, Ui};

use crate::theme;

/// One dash of a construction line, in points.
const CONSTRUCTION_DASH: f32 = 6.0;

/// The gap after one dash of a construction line, in points.
const CONSTRUCTION_GAP: f32 = 4.0;

/// The shortest arc worth sampling, in points of length.
const ARC_STEP: f32 = 4.0;

/// The fewest segments an angular arc is sampled into.
const ARC_MIN_STEPS: usize = 8;

/// The most segments an angular arc is sampled into.
const ARC_MAX_STEPS: usize = 96;

/// Paints the construction line between two screen points: the chord being measured.
///
/// The line is the guide amber and dashed, so it reads as the program's own help rather than as a
/// form edge, and it is never mistaken for the geometry that will be created.
pub fn construction(ui: &Ui, painter: &Painter, from: Pos2, to: Pos2) {
    let stroke = Stroke::new(theme::HELPER_STROKE, theme::helper(ui));
    painter.extend(Shape::dashed_line(&[from, to], stroke, CONSTRUCTION_DASH, CONSTRUCTION_GAP));
}

/// Paints the reference line at `anchor`: the +X axis of `radius` and the arc up to `angle`.
///
/// `angle` is in radians from +X, counter-clockwise, and the screen's y axis points down, so the arc
/// turns upward for a positive angle. A radius or an angle that is not finite, a radius at or under
/// zero and an angle of zero draw nothing: the reference has nothing to show until the pointer is
/// off the axis.
pub fn reference(painter: &Painter, anchor: Pos2, radius: f32, angle: f32) {
    reference_between(painter, anchor, radius, 0.0, angle);
}

/// Paints the reference line from the X end at `from` to the direction at `to`, both in radians.
///
/// An angle is read from the nearer end of the X axis (ADR 0128), so the axis is not always `+X`:
/// the caller says which end it chose, and the arc sweeps between the two directions the short way
/// because the difference the caller passes is already the signed sweep.
pub fn reference_between(painter: &Painter, anchor: Pos2, radius: f32, from: f32, to: f32) {
    if !anchor.x.is_finite() || !anchor.y.is_finite() || !radius.is_finite() || radius <= 0.0 {
        return;
    }
    let (dark, light) = theme::reference_inks();
    let axis = [anchor, Pos2::new(anchor.x + radius * from.cos(), anchor.y - radius * from.sin())];
    dots(painter, &axis, dark, light);
    if to.is_finite() && (to - from).abs() > f32::EPSILON {
        let arc = arc_points(anchor, radius, from, to);
        dots(painter, &arc, dark, light);
    }
}

/// The screen points of an angular arc: `from` and `to` in radians from +X, counter-clockwise.
///
/// The sampling follows the arc's length, so a short sweep of a small radius is a few segments and a
/// full turn of a long one is [`ARC_MAX_STEPS`]; a dot line needs the points to be close enough to
/// follow the curve, not to be smooth geometry.
pub fn arc_points(anchor: Pos2, radius: f32, from: f32, to: f32) -> Vec<Pos2> {
    if !radius.is_finite() || radius <= 0.0 || !from.is_finite() || !to.is_finite() {
        return Vec::new();
    }
    let sweep = to - from;
    if sweep.abs() <= f32::EPSILON {
        return Vec::new();
    }
    let steps = ((radius * sweep.abs() / ARC_STEP).ceil() as usize).clamp(ARC_MIN_STEPS, ARC_MAX_STEPS);
    let mut points = Vec::with_capacity(steps.saturating_add(1));
    for step in 0..=steps {
        let fraction = step as f32 / steps as f32;
        let angle = from + sweep * fraction;
        points.push(Pos2::new(anchor.x + radius * angle.cos(), anchor.y - radius * angle.sin()));
    }
    points
}

/// Paints one two-tone dot line: the dark dots first, then the light centre inside them.
fn dots(painter: &Painter, path: &[Pos2], dark: Color32, light: Color32) {
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
    fn the_axis_starts_at_the_anchor_and_reaches_the_radius() {
        let ctx = egui::Context::default();
        let anchor = pos2(200.0, 300.0);
        let radius = 120.0;
        let mut shapes = Vec::new();
        let output = ctx.run_ui(egui::RawInput { screen_rect: Some(VIEWPORT), ..Default::default() }, |ui| {
            reference(ui.painter(), anchor, radius, 0.0);
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
        assert!(reach >= anchor.x + radius - theme::REFERENCE_SPACING, "the axis reaches the radius: {reach}");
        assert!(dots.iter().all(|(_, _, radius)| *radius <= theme::REFERENCE_DOT), "no dot is a line");
        assert!(dots.iter().any(|(_, fill, _)| *fill == light), "the light centre is inside the dark dot");
    }

    #[test]
    fn a_direction_draws_the_arc_it_sweeps() {
        let anchor = pos2(100.0, 200.0);
        let quarter = std::f32::consts::FRAC_PI_2;
        let points = arc_points(anchor, 50.0, 0.0, quarter);
        let first = points.first().copied().expect("the arc starts");
        let last = points.last().copied().expect("the arc ends");
        assert!((first - pos2(150.0, 200.0)).length() < 1e-3, "the arc starts on +X: {first:?}");
        assert!((last - pos2(100.0, 150.0)).length() < 1e-3, "a counter-clockwise quarter turns up: {last:?}");
        assert!(points.len() >= ARC_MIN_STEPS, "a short arc is still sampled");
    }

    #[test]
    fn the_reference_reads_from_the_end_the_angle_names() {
        // The nearer end of the X axis is not always +X: the axis is dotted along −X and the arc
        // still takes the short way, so a segment in the left half-plane is measured from there.
        let ctx = egui::Context::default();
        let anchor = pos2(400.0, 300.0);
        let mut shapes = Vec::new();
        let output = ctx.run_ui(egui::RawInput { screen_rect: Some(VIEWPORT), ..Default::default() }, |ui| {
            reference_between(ui.painter(), anchor, 60.0, std::f32::consts::PI, 2.0 * std::f32::consts::FRAC_PI_3);
        });
        shapes.extend(output.shapes.clone());
        output.drop_without_applying_deltas();
        let dots = circles(&shapes);
        let reach = dots.iter().map(|(centre, _, _)| centre.x).fold(anchor.x, f32::min);
        assert!(reach <= anchor.x - 60.0 + theme::REFERENCE_SPACING, "the axis runs along minus X: {reach}");
        assert!(
            dots.iter().any(|(centre, _, _)| centre.y < anchor.y - 40.0),
            "the arc turns down from −X to the segment below it: {dots:?}"
        );
    }

    #[test]
    fn a_hostile_reference_draws_nothing() {
        let ctx = egui::Context::default();
        let anchor = pos2(200.0, 300.0);
        for (radius, angle) in [(0.0, 1.0), (-5.0, 1.0), (f32::NAN, 1.0), (f32::INFINITY, 0.0)] {
            let mut shapes = Vec::new();
            let output = ctx.run_ui(egui::RawInput { screen_rect: Some(VIEWPORT), ..Default::default() }, |ui| {
                reference(ui.painter(), anchor, radius, angle);
            });
            shapes.extend(output.shapes.clone());
            output.drop_without_applying_deltas();
            assert!(circles(&shapes).is_empty(), "radius {radius}, angle {angle} draws nothing");
        }
        // A hostile angle still has a sound axis: the direction is unknown, so only the axis shows.
        let mut shapes = Vec::new();
        let output = ctx.run_ui(egui::RawInput { screen_rect: Some(VIEWPORT), ..Default::default() }, |ui| {
            reference(ui.painter(), anchor, 40.0, f32::NAN);
        });
        shapes.extend(output.shapes.clone());
        output.drop_without_applying_deltas();
        let dots = circles(&shapes);
        assert!(!dots.is_empty(), "the axis is drawn");
        assert!(
            dots.iter().all(|(centre, _, _)| (centre.y - anchor.y).abs() < 1e-3),
            "no arc is drawn for an unknown angle: {dots:?}"
        );
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
