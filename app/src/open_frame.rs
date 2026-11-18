//! The corner brackets of the level that is open.
//!
//! Over the scene an open container and a closed one look identical: only the tree says which level
//! the pointer works in, and that is what decides what the next click does (X38). The shell draws a
//! small bracket at each of the eight corners of the open level's box — three short arms along the
//! box's own edges — so the level that owns the viewport says so in the picture (U89).
//!
//! A bracket marks a corner and leaves the faces alone: a wireframe over an open group would fight
//! the form edges the renderer already draws (X10). It is the guide ink and the width of the open
//! level (ADR 0111), screen space on the viewport's own layer, after the image and before the
//! gizmos, and it takes no click.
//!
//! The boxes are view state read from the last evaluation, so opening or closing a level paints on
//! the next frame and re-evaluates nothing. A closed container, a hidden one — its subtree draws
//! nothing, so it has no box — and a product that is not open draw nothing at all.

use std::collections::HashSet;

use eframe::egui;
use egui::{Pos2, Ui};
use geom::DVec3;
use model::Id;
use scene::{Camera, ProductEnvelope, ProductEnvelopeState};
use ui::theme;

use crate::gizmo_math::screen_point;
use crate::scene_build::LevelBounds;

/// An arm takes a fifth of the shortest box edge that meets its corner.
const ARM_FRACTION: f32 = 0.2;

/// The shortest an arm draws, in points: below this a corner reads as a dot, not as a bracket.
const ARM_MIN: f32 = 6.0;

/// The longest an arm draws, in points: above this the bracket reads as a box of its own.
const ARM_MAX: f32 = 18.0;

/// The shortest an edge may project and still draw an arm, in points.
///
/// Below a pixel the projected edge has no direction a reader could follow, so its corner keeps two
/// arms instead of three: a box seen face-on draws no arm along the axis that points at the camera.
const EDGE_MIN: f32 = 0.5;

/// One corner of an open level and the three arms it draws, in screen points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Bracket {
    /// The projected corner the arms start from.
    pub(crate) corner: Pos2,
    /// The three arm ends, one along each box edge that meets the corner. An edge seen end-on ends
    /// at the corner itself and draws nothing.
    pub(crate) arms: [Pos2; 3],
    /// The length the three arms share, in points.
    pub(crate) length: f32,
}

/// The world boxes the brackets draw on, in tree order.
///
/// A container's box is the eight corners the last evaluation measured for its whole subtree; a
/// product's is the authored envelope, which belongs to its own frame and is read from the frame it
/// already drew, so the two are never derived twice (ADR 0100).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct OpenBoxes(Vec<[DVec3; 8]>);

impl OpenBoxes {
    /// The boxes of the containers in `open` and of the one open product.
    pub(crate) fn collect(levels: &[LevelBounds], open: &HashSet<Id>, envelopes: &[ProductEnvelope]) -> Self {
        let mut boxes = Vec::new();
        for level in levels {
            if !open.contains(&level.id) {
                continue;
            }
            if let Some(corners) = level.corners {
                boxes.push(corners);
            }
        }
        for envelope in envelopes {
            if matches!(envelope.state, ProductEnvelopeState::Open) {
                boxes.push(envelope_corners(envelope));
            }
        }
        Self(boxes)
    }
}

/// The eight corners of a product's authored box in world space.
///
/// The bit order is the one [`geom::Bounds`] promises for a container's box: bit 0 picks
/// the `x` maximum, bit 1 the `y` and bit 2 the `z`, so both kinds of box bracket the same way. The
/// box is the one around the reserved footprint: a bracket marks the piece's extent, not its shape.
fn envelope_corners(envelope: &ProductEnvelope) -> [DVec3; 8] {
    let (min, max) = envelope.local_bounds().unwrap_or((DVec3::ZERO, DVec3::ZERO));
    let mut corners = [DVec3::ZERO; 8];
    for (index, corner) in corners.iter_mut().enumerate() {
        let pick = |bit: usize, low: f64, high: f64| if index & bit == 0 { low } else { high };
        *corner = envelope.world_transform.transform_point3(DVec3::new(
            pick(1, min.x, max.x),
            pick(2, min.y, max.y),
            pick(4, min.z, max.z),
        ));
    }
    corners
}

/// Paints the brackets of the open levels for one view and returns what it drew.
///
/// `viewport` is that view's own rectangle: a split layout paints one set of brackets per view, with
/// that view's camera, so a bracket lands where the pass under it drew the model.
pub(crate) fn show(ui: &Ui, viewport: egui::Rect, camera: Camera, boxes: &OpenBoxes) -> Vec<Bracket> {
    let brackets = brackets(camera, viewport, boxes);
    let stroke = egui::Stroke::new(theme::HELPER_OPEN_STROKE, theme::helper(ui));
    let painter = ui.painter().with_clip_rect(viewport);
    for bracket in &brackets {
        for end in bracket.arms {
            painter.line_segment([bracket.corner, end], stroke);
        }
    }
    brackets
}

/// The brackets of every box, projected through `camera` into `viewport`.
///
/// A corner behind the camera draws nothing, exactly like a camera or light glyph that is behind it.
fn brackets(camera: Camera, viewport: egui::Rect, boxes: &OpenBoxes) -> Vec<Bracket> {
    let mut brackets = Vec::new();
    for corners in &boxes.0 {
        for (index, corner) in corners.iter().enumerate() {
            let Some(centre) = screen_point(camera, viewport, *corner) else {
                continue;
            };
            // The corners one bit away share an edge with this one; the bit that differs says which
            // axis that edge runs along.
            let mut ends = [None; 3];
            let mut shortest = f32::INFINITY;
            for (slot, other) in ends.iter_mut().zip([index ^ 1, index ^ 2, index ^ 4]) {
                let Some(neighbour) = corners.get(other) else {
                    continue;
                };
                let Some(end) = screen_point(camera, viewport, *neighbour) else {
                    continue;
                };
                *slot = Some(end);
                let edge = end.distance(centre);
                if edge.is_finite() && edge >= EDGE_MIN {
                    shortest = shortest.min(edge);
                }
            }
            if !shortest.is_finite() {
                // Every edge that meets the corner ends on the camera's plane: one dot is not a bracket.
                continue;
            }
            let length = (shortest * ARM_FRACTION).clamp(ARM_MIN, ARM_MAX);
            let arms = ends.map(|end| match end {
                Some(end) if end.distance(centre) >= EDGE_MIN => centre + (end - centre).normalized() * length,
                _ => centre,
            });
            brackets.push(Bracket { corner: centre, arms, length });
        }
    }
    brackets
}

#[cfg(test)]
mod tests {
    use egui::{Rect, Vec2};
    use geom::{DMat4, DQuat};
    use scene::Projection;

    use super::*;

    fn viewport() -> Rect {
        Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))
    }

    /// A three-quarter view of a 800 x 400 x 300 box, so all eight corners are apart on screen.
    fn camera(distance: f64) -> Camera {
        Camera::new(DVec3::new(400.0, 200.0, 150.0), distance).expect("camera")
    }

    fn box_corners() -> [DVec3; 8] {
        let mut corners = [DVec3::ZERO; 8];
        for (index, corner) in corners.iter_mut().enumerate() {
            let pick = |bit: usize, high: f64| if index & bit == 0 { 0.0 } else { high };
            *corner = DVec3::new(pick(1, 800.0), pick(2, 400.0), pick(4, 300.0));
        }
        corners
    }

    fn one_box() -> OpenBoxes {
        OpenBoxes(vec![box_corners()])
    }

    fn level(id: u64, corners: Option<[DVec3; 8]>) -> LevelBounds {
        LevelBounds { id: Id::new(id).expect("id"), corners }
    }

    #[test]
    fn the_eight_corners_sit_on_the_projected_box_at_two_zoom_levels() {
        let corners = box_corners();
        for distance in [2000.0, 6000.0] {
            let camera = camera(distance);
            let drawn = brackets(camera, viewport(), &one_box());
            assert_eq!(drawn.len(), 8, "one bracket per box corner at {distance} mm");

            for (index, bracket) in drawn.iter().enumerate() {
                let expected = screen_point(camera, viewport(), corners[index]).expect("the corner is in front");
                assert!((bracket.corner - expected).length() < 1.0e-3, "corner {index} at {distance} mm");
                assert!(viewport().contains(bracket.corner), "corner {index} is inside the view");

                // One arm per edge of the box that meets the corner, along that edge's projection.
                let shortest = [1, 2, 4]
                    .map(|bit| {
                        let end = screen_point(camera, viewport(), corners[index ^ bit]).expect("the edge is in front");
                        end.distance(expected)
                    })
                    .into_iter()
                    .filter(|edge| *edge > f32::EPSILON)
                    .fold(f32::INFINITY, f32::min);
                assert!((bracket.length - (shortest * ARM_FRACTION).clamp(ARM_MIN, ARM_MAX)).abs() < 1.0e-4);

                for (arm, bit) in bracket.arms.into_iter().zip([1, 2, 4]) {
                    let end = screen_point(camera, viewport(), corners[index ^ bit]).expect("the edge is in front");
                    let direction = (end - expected).normalized();
                    let arm_direction = (arm - expected).normalized();
                    assert!((arm_direction - direction).length() < 1.0e-3, "corner {index} arm {bit} follows its edge");
                    assert!((arm - expected).length() <= bracket.length + 1.0e-3, "and stops inside the arm length");
                }
            }
        }
    }

    #[test]
    fn the_arm_caps_at_both_ends_of_the_zoom_range() {
        // Both ends of the clamp are reachable, which is what keeps a bracket legible close up and
        // far away; between them the arm is its own fifth of the shortest edge that meets the corner.
        let near = brackets(camera(800.0), viewport(), &one_box());
        let middle = brackets(camera(4000.0), viewport(), &one_box());
        let far = brackets(camera(12_000.0), viewport(), &one_box());
        assert!(near.iter().all(|bracket| bracket.length == ARM_MAX), "a close box caps every arm at 18 pt");
        assert!(far.iter().all(|bracket| bracket.length == ARM_MIN), "a distant one stops shrinking at 6 pt");
        assert!(
            middle.iter().all(|bracket| bracket.length > ARM_MIN && bracket.length < ARM_MAX),
            "at a working zoom the arm is the fifth of the shortest edge"
        );
        assert!(middle[0].length < near[0].length && far[0].length < middle[0].length, "the arm follows the zoom");
    }

    #[test]
    fn a_flat_box_draws_no_arm_along_the_edge_seen_end_on() {
        // A plan view flattens the z extent, so its edge projects onto one point and its arm is the
        // corner itself: a zero-length line the painter drops.
        let camera = Camera::new(DVec3::new(400.0, 200.0, 150.0), 2000.0)
            .expect("camera")
            .looked_from(DVec3::new(400.0, 200.0, 2150.0), DVec3::X, DVec3::Y, -DVec3::Z)
            .with_projection(Projection::Orthographic { height: 1000.0 });
        let drawn = brackets(camera, viewport(), &one_box());
        assert_eq!(drawn.len(), 8, "a top view still brackets every corner");
        for bracket in drawn {
            assert_eq!(bracket.arms[2], bracket.corner, "the edge towards the camera draws no arm");
            assert!(bracket.arms[0] != bracket.corner && bracket.arms[1] != bracket.corner);
        }
    }

    #[test]
    fn only_the_open_levels_hand_a_box_to_the_brackets() {
        let corners = box_corners();
        let group = level(2, Some(corners));
        let empty = level(3, None);
        let closed = HashSet::new();
        assert!(OpenBoxes::collect(&[group, empty], &closed, &[]).0.is_empty(), "a closed group draws nothing");

        let mut open = HashSet::new();
        open.insert(level(2, None).id);
        open.insert(level(3, None).id);
        let boxes = OpenBoxes::collect(&[group, empty], &open, &[]);
        assert_eq!(boxes.0.len(), 1, "an open level with nothing drawn adds no box, so no bracket");
        assert_eq!(boxes.0[0], corners);
    }

    fn envelope(id: u64, state: ProductEnvelopeState) -> ProductEnvelope {
        let placement = DMat4::from_rotation_translation(
            DQuat::from_rotation_z(std::f64::consts::FRAC_PI_2),
            DVec3::new(100.0, 0.0, 0.0),
        );
        ProductEnvelope::rectangular(Id::new(id).expect("id"), DVec3::new(600.0, 400.0, 720.0), placement, state)
    }

    #[test]
    fn only_the_open_product_brackets_its_authored_box() {
        let opening = envelope(4, ProductEnvelopeState::Open);
        let boxes = OpenBoxes::collect(&[], &HashSet::new(), std::slice::from_ref(&opening));
        assert_eq!(boxes.0.len(), 1, "the open product brackets its envelope");

        // The box follows the product's own frame, so a turned piece brackets a turned box.
        let corners = boxes.0[0];
        let (min, max) = opening.local_bounds().expect("a valid envelope");
        for (index, corner) in corners.iter().enumerate() {
            let pick = |bit: usize, low: f64, high: f64| if index & bit == 0 { low } else { high };
            let local = DVec3::new(pick(1, min.x, max.x), pick(2, min.y, max.y), pick(4, min.z, max.z));
            let expected = opening.world_transform.transform_point3(local);
            assert!((*corner - expected).length() < 1.0e-9, "corner {index} of the authored box");
        }

        for state in [ProductEnvelopeState::ClosedEmpty, ProductEnvelopeState::ClosedWithParts] {
            let closed = OpenBoxes::collect(&[], &HashSet::new(), &[envelope(5, state)]);
            assert!(closed.0.is_empty(), "a {state:?} product draws no bracket");
        }
    }

    #[test]
    fn every_arm_takes_the_guide_ink_at_the_open_level_width() {
        // ADR 0111: a guide takes the one amber and one of the three widths, so a bracket invents
        // neither and a colour literal never appears in this module.
        let camera = camera(2000.0);
        let boxes = one_box();
        let ctx = egui::Context::default();
        let mut ink = egui::Color32::TRANSPARENT;
        let mut drawn = Vec::new();
        let output = ctx.run_ui(egui::RawInput { screen_rect: Some(viewport()), ..Default::default() }, |ui| {
            ink = theme::helper(ui);
            drawn = show(ui, viewport(), camera, &boxes);
        });
        let strokes: Vec<egui::Stroke> = output
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::Shape::LineSegment { stroke, .. } => Some(*stroke),
                _ => None,
            })
            .collect();
        assert_eq!(strokes.len(), drawn.len() * 3, "three arms on every one of the eight corners");
        for stroke in strokes {
            assert_eq!(stroke.color, ink, "the bracket is the guide ink");
            assert_eq!(stroke.width, theme::HELPER_OPEN_STROKE, "at the width of the open level (U83)");
        }
        output.drop_without_applying_deltas();
    }
}
