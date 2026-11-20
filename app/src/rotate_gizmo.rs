//! The viewport rotate gizmo.
//!
//! Three rings are aligned with the world X, Y and Z axes and are projected over the selection's
//! manipulation pivot, the stored pivot of its primary node. A drag never mutates the document: it
//! turns a preview of every selected panel about that pivot, and the release dispatches one
//! `node.placement.set_many`. Because the rings are world-aligned, the preview turn is applied on
//! the left of each node's world rotation; the commit converts it into that node's parent frame and
//! pre-multiplies it onto the stored local quaternion. When the pivot is not the node's origin the
//! translation is adjusted too, so the ring centre stays a fixed point of the whole selection.
//!
//! Rings keep a roughly constant screen radius. A ring seen edge-on is not drawn: its plane is
//! nearly parallel to the view direction, the pointer cannot define an angle on it and a drag would
//! jump by half a turn.
//!
//! Two turns share the rings (X88): the free one follows the hand through whatever stands beside it,
//! and the object turn asks the document's rule how far it may go, so the ring stops where another
//! body stops it and the release writes a placement the write's guard accepts. The rule is read once,
//! when the drag starts, and every frame it answers the same angle the preview draws (ADR 0180).

use eframe::egui;
use egui::{Align2, Color32, FontId, Pos2, Response, Stroke, Ui};
use geom::{DQuat, DVec3};
use model::Id;
use scene::Camera;

use crate::gizmo_math::{
    ACTIVE_COLOR, Axis, MoveTarget, PlacementCommit, Preview, PreviewKey, Target, commit_rotation, distance_to_segment,
    ray_plane, ray_to, screen_point, stop_text,
};

/// Number of line segments in a ring.
const RING_SEGMENTS: usize = 64;

/// Radius inside which a pointer hits a ring, in points.
const HIT_RADIUS: f32 = 7.0;

/// Rings whose plane is closer than this to edge-on are not drawn or grabbed.
const EDGE_ON_LIMIT: f64 = 0.2;

/// One continuous rotation drag.
#[derive(Debug)]
struct Drag {
    node: Id,
    axis: Axis,
    drawables: Vec<Id>,
    targets: Vec<MoveTarget>,
    pivot: DVec3,
    previous_angle: f64,
    /// The angle the hand has turned, which the ring follows unchecked.
    angle: f64,
    /// The angle the document grants, which the preview draws and the release writes.
    ///
    /// It is the hand's own angle for the free turn, and the last angle the rule granted for the
    /// object turn, so the ring never shows a turn the release would refuse (X88).
    granted: f64,
    /// The body that stopped the turn, when one did.
    stop: Option<model::Stop>,
    /// The rule this drag is measured by, when the object turn is armed.
    rule: Option<model::RotateRule>,
    /// Whether the document's bodies measure this drag (ADR 0180).
    measured: bool,
}

impl Drag {
    /// Asks the rule how far the hand's angle may go, and keeps the answer for the frame.
    ///
    /// The bisection runs on the rule's own boxes and bodies, never on the document: the drag still
    /// writes nothing until the release, and every frame asks the same question of the same snapshot.
    fn measure(&mut self) {
        let (granted, stop) = match &self.rule {
            Some(rule) => {
                let limit = rule.limit(self.axis.direction(), self.angle);
                (limit.angle, limit.contact)
            }
            None => (self.angle, None),
        };
        self.granted = if granted.is_finite() { granted } else { 0.0 };
        self.stop = stop;
    }
}

/// Persistent state of the rotate gizmo.
#[derive(Debug, Default)]
pub(crate) struct RotateGizmo {
    drag: Option<Drag>,
    hover: Option<Axis>,
    consumed_click: bool,
}

impl RotateGizmo {
    /// Whether a ring drag is in progress.
    pub(crate) fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// Whether this frame's pointer release belongs to a gizmo drag and must not select.
    pub(crate) fn consumed_click(&self) -> bool {
        self.consumed_click
    }

    /// The drawable ids, pivot and world turn the renderer should preview, if a drag turned something.
    pub(crate) fn preview(&self) -> Option<Preview<'_>> {
        let drag = self.drag.as_ref()?;
        if drag.granted == 0.0 || !drag.granted.is_finite() {
            return None;
        }
        Some(Preview::Rotation {
            drawables: &drag.drawables,
            pivot: drag.pivot,
            rotation: DQuat::from_axis_angle(drag.axis.direction(), drag.granted),
        })
    }

    /// The preview identity used to avoid re-uploading an unchanged drag.
    ///
    /// It names the angle the preview draws, so a frame the rule holds at a face does not re-upload:
    /// a ring pressed into a body is still, and still is not a change (X88).
    pub(crate) fn preview_key(&self) -> Option<PreviewKey> {
        let drag = self.drag.as_ref()?;
        if drag.granted == 0.0 || !drag.granted.is_finite() {
            return None;
        }
        Some(PreviewKey::Rotation { node: drag.node, angle: drag.granted.to_bits() })
    }

    /// Starts, continues or finishes a drag and returns a placement to dispatch on release.
    ///
    /// `pointer_over_widget` is true when the pointer is over the orientation widget, which owns
    /// its own clicks. The caller must run this before camera orbit and discard the pointer's click
    /// when [`Self::consumed_click`] is true.
    ///
    /// `radius_factor` is the ring radius as a fraction of the camera distance, from the workspace
    /// settings (U86); the drawing and the hit test are handed the same value.
    pub(crate) fn update(
        &mut self,
        ui: &Ui,
        response: &Response,
        viewport: egui::Rect,
        target: Target<'_>,
        pointer_over_widget: bool,
        radius_factor: f64,
    ) -> Result<Option<PlacementCommit>, String> {
        self.consumed_click = false;
        if let Some(mut drag) = self.drag.take() {
            let (pointer, down, escape) = ui.input(|input| {
                (input.pointer.latest_pos(), input.pointer.primary_down(), input.key_pressed(egui::Key::Escape))
            });
            if escape {
                self.consumed_click = true;
                return Ok(None);
            }
            if let Some(pointer) = pointer
                && let Some(angle) = pointer_angle(pointer, viewport, target.camera, drag.pivot, drag.axis.direction())
            {
                let step = wrap_angle(angle - drag.previous_angle);
                if step.is_finite() {
                    drag.angle += step;
                    drag.previous_angle = angle;
                }
            }
            drag.measure();
            log::debug!(
                target: "z3l::rotate",
                "turn node={:?} axis={} pivot=({:.1},{:.1},{:.1}) asked={:.4} granted={:.4} contact={:?} blocked={}",
                drag.node,
                drag.axis.label(),
                drag.pivot.x,
                drag.pivot.y,
                drag.pivot.z,
                drag.angle,
                drag.granted,
                drag.stop,
                drag.stop.map_or_else(|| "none".to_owned(), stop_text),
            );
            if !down {
                self.consumed_click = true;
                let turn = DQuat::from_axis_angle(drag.axis.direction(), drag.granted);
                let commit = commit_rotation(&drag.targets, drag.pivot, turn, drag.measured)?;
                return Ok(commit);
            }
            self.drag = Some(drag);
            return Ok(None);
        }

        self.hover = None;
        if pointer_over_widget {
            return Ok(None);
        }
        let Some(pivot) = target.anchor_pivot() else {
            return Ok(None);
        };
        let Some(pointer) = response.hover_pos() else {
            return Ok(None);
        };
        let Some(axis) = hit_ring(target.camera, viewport, pivot, pointer, radius_factor) else {
            return Ok(None);
        };
        self.hover = Some(axis);
        if !ui.input(|input| input.pointer.primary_pressed()) {
            return Ok(None);
        }

        let axis_world = axis.direction();
        let Some(start_angle) = pointer_angle(pointer, viewport, target.camera, pivot, axis_world) else {
            return Ok(None);
        };
        let targets = target.move_targets();
        let Some(anchor) = targets.last() else {
            return Ok(None);
        };

        self.drag = Some(Drag {
            node: anchor.node,
            axis,
            drawables: target.drawables(),
            targets,
            pivot,
            previous_angle: start_angle,
            angle: 0.0,
            granted: 0.0,
            stop: None,
            rule: target.rotate_rule(),
            measured: target.rotate_kind.measured(),
        });
        if !ui.input(|input| input.pointer.primary_down()) {
            self.drag = None;
            self.consumed_click = true;
        }
        Ok(None)
    }

    /// Draws the rings over the viewport.
    pub(crate) fn paint(&self, ui: &Ui, viewport: egui::Rect, target: Target<'_>, radius_factor: f64) {
        let Some(pivot) = target.anchor_pivot() else {
            return;
        };
        let Some(origin) = screen_point(target.camera, viewport, pivot) else {
            return;
        };
        if !viewport.expand(64.0).contains(origin) {
            return;
        }
        let radius = world_radius(target.camera, radius_factor);
        let painter = ui.painter().with_clip_rect(viewport);
        let center_tone = if self.drag.is_some() { Color32::from_gray(140) } else { Color32::from_gray(220) };
        painter.circle_filled(origin, 3.0, center_tone);

        for axis in Axis::ALL {
            let axis_world = axis.direction();
            if axis_world.dot(target.camera.forward()).abs() < EDGE_ON_LIMIT {
                continue;
            }
            let Some(points) = ring_screen_points(target.camera, viewport, pivot, axis_world, radius) else {
                continue;
            };
            let active = self.drag.as_ref().is_some_and(|drag| drag.axis == axis);
            let hovered = self.hover == Some(axis);
            let mut color = if active || hovered { ACTIVE_COLOR } else { axis.color() };
            if self.drag.is_some() && !active {
                color = color.gamma_multiply(0.4);
            }
            let width = if active || hovered { 3.5 } else { 2.2 };
            for pair in points.windows(2) {
                painter.line_segment([pair[0], pair[1]], Stroke::new(width, color));
            }
            if let Some(label) = points.iter().min_by(|a, b| a.y.total_cmp(&b.y)) {
                painter.text(
                    *label - egui::vec2(0.0, 12.0),
                    Align2::CENTER_CENTER,
                    axis.label(),
                    FontId::proportional(12.0),
                    if active || hovered { ACTIVE_COLOR } else { Color32::from_gray(230) },
                );
            }
        }
    }
}

/// A stable orthonormal basis for the plane perpendicular to `normal`.
fn ring_basis(normal: DVec3) -> Option<(DVec3, DVec3)> {
    let seed = if normal.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
    let u = (seed - normal * seed.dot(normal)).try_normalize()?;
    Some((u, normal.cross(u)))
}

/// World radius that keeps a ring `factor` of the camera distance, never below one millimetre.
fn world_radius(camera: Camera, factor: f64) -> f64 {
    (camera.distance() * factor.max(0.0)).max(1.0)
}

/// The projected ring, closed by repeating its first point.
fn ring_screen_points(
    camera: Camera,
    viewport: egui::Rect,
    pivot: DVec3,
    axis_world: DVec3,
    radius: f64,
) -> Option<Vec<Pos2>> {
    let (u, v) = ring_basis(axis_world)?;
    let mut points = Vec::with_capacity(RING_SEGMENTS + 1);
    for index in 0..=RING_SEGMENTS {
        let turn = std::f64::consts::TAU * index as f64 / RING_SEGMENTS as f64;
        let point = pivot + (u * turn.cos() + v * turn.sin()) * radius;
        points.push(screen_point(camera, viewport, point)?);
    }
    Some(points)
}

/// The ring whose projected outline is nearest to `pointer`.
fn hit_ring(camera: Camera, viewport: egui::Rect, pivot: DVec3, pointer: Pos2, radius_factor: f64) -> Option<Axis> {
    let radius = world_radius(camera, radius_factor);
    let mut best: Option<(f32, Axis)> = None;
    for axis in Axis::ALL {
        let axis_world = axis.direction();
        if axis_world.dot(camera.forward()).abs() < EDGE_ON_LIMIT {
            continue;
        }
        let Some(points) = ring_screen_points(camera, viewport, pivot, axis_world, radius) else {
            continue;
        };
        for pair in points.windows(2) {
            let Some(distance) = distance_to_segment(pointer, pair[0], pair[1]) else {
                continue;
            };
            if distance <= HIT_RADIUS && best.is_none_or(|(nearest, _)| distance < nearest) {
                best = Some((distance, axis));
            }
        }
    }
    best.map(|(_, axis)| axis)
}

/// The pointer's angle on the ring plane, measured from the basis of [`ring_basis`].
fn pointer_angle(pointer: Pos2, viewport: egui::Rect, camera: Camera, pivot: DVec3, axis_world: DVec3) -> Option<f64> {
    let hit = ray_plane(ray_to(pointer, viewport, camera), pivot, axis_world)?;
    let (u, v) = ring_basis(axis_world)?;
    let radial = hit - pivot;
    let radial = radial - axis_world * radial.dot(axis_world);
    let radial = radial.try_normalize()?;
    Some(radial.dot(v).atan2(radial.dot(u)))
}

/// Wraps an angle into `(-π, π]`, so a pointer crossing the seam does not jump a full turn.
fn wrap_angle(angle: f64) -> f64 {
    let wrapped = angle.rem_euclid(std::f64::consts::TAU);
    if wrapped > std::f64::consts::PI { wrapped - std::f64::consts::TAU } else { wrapped }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use std::f64::consts::{FRAC_PI_2, FRAC_PI_4};

    use model::Placement;
    use units::Length;

    use super::*;

    /// The ring radius factor the tests draw and hit with, relative to the camera distance.
    const TEST_RING_FACTOR: f64 = 0.06;

    fn viewport() -> egui::Rect {
        egui::Rect::from_min_size(Pos2::ZERO, egui::Vec2::new(800.0, 600.0))
    }

    fn move_target(node: u64, start_local: Placement, parent: Placement, pivot_local: DVec3) -> MoveTarget {
        MoveTarget { node: Id::new(node).expect("id"), start_local, parent, pivot_local }
    }

    fn world_pivot(target: &MoveTarget) -> DVec3 {
        target.parent.transform_point(target.start_local.transform_point(target.pivot_local))
    }

    fn drag(axis: Axis, angle: f64) -> Drag {
        Drag {
            node: Id::new(2).expect("id"),
            axis,
            drawables: Vec::new(),
            targets: Vec::new(),
            pivot: DVec3::ZERO,
            previous_angle: 0.0,
            angle,
            granted: angle,
            stop: None,
            rule: None,
            measured: false,
        }
    }

    #[test]
    fn a_quarter_turn_premultiplies_the_world_turn_onto_the_local_frame() {
        let start = Placement::new(DVec3::new(1.0, 2.0, 3.0), DQuat::from_rotation_z(FRAC_PI_4)).expect("placement");
        let target = move_target(2, start, Placement::IDENTITY, DVec3::ZERO);
        let turn = DQuat::from_axis_angle(Axis::Y.direction(), FRAC_PI_2);
        let commit =
            commit_rotation(&[target], world_pivot(&target), turn, false).expect("valid rotation").expect("a turn");
        assert_eq!(commit.changes.len(), 1);
        let change = commit.changes[0];
        assert_eq!(change.placement.translation(), start.translation());
        let expected = (turn * start.rotation()).normalize();
        assert!(change.placement.rotation().dot(expected).abs() > 1.0 - 1.0e-12);
    }

    #[test]
    fn a_turn_is_expressed_in_the_parent_frame() {
        let parent = Placement::new(DVec3::ZERO, DQuat::from_rotation_z(FRAC_PI_4)).expect("placement");
        let start = Placement::new(DVec3::new(10.0, 20.0, 30.0), DQuat::from_rotation_z(0.2)).expect("placement");
        let target = move_target(2, start, parent, DVec3::new(1.0, 2.0, 3.0));
        let angle = FRAC_PI_2;
        let turn = DQuat::from_axis_angle(Axis::Y.direction(), angle);
        let commit =
            commit_rotation(&[target], world_pivot(&target), turn, false).expect("valid rotation").expect("a turn");
        let change = commit.changes[0];

        // In world space the preview and the commit must be the same left-multiplied turn.
        let world_before = parent.rotation() * start.rotation();
        let world_after = parent.rotation() * change.placement.rotation();
        let expected = turn * world_before;
        assert!(world_after.dot(expected).abs() > 1.0 - 1.0e-12, "world rotations diverged");

        // The pivot is fixed in the parent frame and therefore in world space too.
        let pivot_before = start.transform_point(target.pivot_local);
        let pivot_after = change.placement.transform_point(target.pivot_local);
        assert!(
            (pivot_before - pivot_after).length() < 2.0e-3,
            "pivot moved from {pivot_before:?} to {pivot_after:?} by more than the micrometre rounding"
        );
    }

    #[test]
    fn a_turn_is_never_measured_by_the_documents_bodies() {
        // A rotation changes where a body lies without asking any other body: the commit says so, and
        // the shell's write is the one the guard does not refuse (ADR 0180).
        let target = move_target(2, Placement::IDENTITY, Placement::IDENTITY, DVec3::ZERO);
        let turn = DQuat::from_axis_angle(Axis::Z.direction(), 0.5);
        let commit =
            commit_rotation(&[target], world_pivot(&target), turn, false).expect("valid rotation").expect("a turn");
        assert!(!commit.measured, "a turn is the maker's own hand");
    }

    #[test]
    fn a_zero_or_full_turn_dispatches_nothing() {
        let target = move_target(2, Placement::IDENTITY, Placement::IDENTITY, DVec3::ZERO);
        let pivot = world_pivot(&target);
        for angle in [0.0, std::f64::consts::TAU, -std::f64::consts::TAU] {
            let turn = DQuat::from_axis_angle(Axis::X.direction(), angle);
            assert_eq!(commit_rotation(&[target], pivot, turn, false).expect("no error"), None);
        }
        assert_eq!(
            commit_rotation(&[], pivot, DQuat::from_axis_angle(Axis::X.direction(), 0.5), false).expect("no error"),
            None
        );
    }

    #[test]
    fn a_turn_about_a_non_zero_pivot_keeps_that_point_fixed() {
        let start = Placement::new(DVec3::new(10.0, 20.0, 30.0), DQuat::IDENTITY).expect("placement");
        let target = move_target(2, start, Placement::IDENTITY, DVec3::new(1.0, 2.0, 3.0));
        let turn = DQuat::from_axis_angle(Axis::Z.direction(), FRAC_PI_2);
        let commit =
            commit_rotation(&[target], world_pivot(&target), turn, false).expect("valid rotation").expect("a turn");
        let change = commit.changes[0];

        let before = start.transform_point(target.pivot_local);
        let after = change.placement.transform_point(target.pivot_local);
        assert!((before - after).length() < 1.0e-9, "pivot moved from {before:?} to {after:?}");
        assert!(
            (change.placement.translation() - start.translation()).length() > 1.0,
            "a turn about a corner must adjust the translation"
        );
    }

    #[test]
    fn a_zero_pivot_preserves_the_translation_bit_for_bit() {
        let start = Placement::new(DVec3::new(1.0, 2.0, 3.0), DQuat::from_rotation_z(FRAC_PI_4)).expect("placement");
        let target = move_target(2, start, Placement::IDENTITY, DVec3::ZERO);
        let turn = DQuat::from_axis_angle(Axis::Y.direction(), FRAC_PI_2);
        let commit =
            commit_rotation(&[target], world_pivot(&target), turn, false).expect("valid rotation").expect("a turn");
        assert_eq!(commit.changes[0].placement.translation(), start.translation());
    }

    #[test]
    fn a_multi_selection_turns_every_node_about_the_anchor_pivot() {
        let first = Placement::new(DVec3::new(100.0, 0.0, 0.0), DQuat::IDENTITY).expect("placement");
        let anchor = Placement::new(DVec3::new(0.0, 200.0, 0.0), DQuat::IDENTITY).expect("placement");
        let targets = [
            move_target(2, first, Placement::IDENTITY, DVec3::ZERO),
            move_target(3, anchor, Placement::IDENTITY, DVec3::new(0.0, 10.0, 0.0)),
        ];
        let pivot = world_pivot(&targets[1]);
        assert_eq!(pivot, DVec3::new(0.0, 210.0, 0.0));
        let turn = DQuat::from_axis_angle(Axis::Z.direction(), FRAC_PI_2);
        let commit = commit_rotation(&targets, pivot, turn, false).expect("valid rotation").expect("a turn");
        assert_eq!(commit.changes.len(), 2);

        // The anchor's stored pivot stays where it was; the other node orbits the same point.
        let anchor_after = commit.changes[1].placement.transform_point(targets[1].pivot_local);
        assert!((anchor_after - pivot).length() < 1.0e-9, "anchor pivot moved to {anchor_after:?}");
        let first_after = commit.changes[0].placement.translation();
        let expected = pivot + turn * (first.translation() - pivot);
        assert!((first_after - expected).length() < 1.0e-6, "first moved to {first_after:?}");
    }

    #[test]
    fn the_preview_turns_about_the_world_axis_and_keeps_the_pivot() {
        let gizmo = RotateGizmo { drag: Some(drag(Axis::Z, FRAC_PI_2)), ..Default::default() };
        let Some(Preview::Rotation { pivot, rotation, .. }) = gizmo.preview() else {
            return;
        };
        assert_eq!(pivot, DVec3::ZERO);
        let point = rotation * DVec3::X;
        assert!((point - DVec3::Y).length() < 1.0e-12, "point was {point:?}");
        assert!(gizmo.preview_key().is_some());
    }

    #[test]
    fn the_pointer_angle_is_measured_on_the_ring_plane() {
        let camera = Camera::default();
        let pivot = camera.target();
        let center = screen_point(camera, viewport(), pivot).expect("centre in front");
        assert!(pointer_angle(center, viewport(), camera, pivot, DVec3::Z).is_none());

        let axis_world = DVec3::Z;
        let (u, _) = ring_basis(axis_world).expect("basis");
        let point = pivot + u * world_radius(camera, TEST_RING_FACTOR);
        let pointer = screen_point(camera, viewport(), point).expect("ring point in front");
        let angle = pointer_angle(pointer, viewport(), camera, pivot, axis_world).expect("angle");
        assert!(angle.abs() < 1.0e-5, "angle was {angle}");
    }

    #[test]
    fn wrap_angle_crosses_the_seam_without_a_full_turn_jump() {
        assert!((wrap_angle(std::f64::consts::PI + 0.1) - (-std::f64::consts::PI + 0.1)).abs() < 1.0e-12);
        assert!((wrap_angle(0.3) - 0.3).abs() < 1.0e-12);
        assert!((wrap_angle(-0.3) + 0.3).abs() < 1.0e-12);
    }

    #[test]
    fn the_default_view_shows_three_world_aligned_rings_and_hides_an_edge_on_one() {
        let camera = Camera::default();
        let pivot = camera.target();
        let mut visible = 0;
        for axis in Axis::ALL {
            let axis_world = axis.direction();
            if axis_world.dot(camera.forward()).abs() >= EDGE_ON_LIMIT {
                visible += 1;
                assert!(
                    ring_screen_points(camera, viewport(), pivot, axis_world, world_radius(camera, TEST_RING_FACTOR))
                        .is_some()
                );
            }
        }
        assert_eq!(visible, 3, "the default isometric view should show all rings");

        let mut top = Camera::default();
        top.look_from(DVec3::Z);
        let z = Axis::Z.direction();
        assert!(z.dot(top.forward()).abs() >= EDGE_ON_LIMIT);
        let x = Axis::X.direction();
        assert!(x.dot(top.forward()).abs() < EDGE_ON_LIMIT, "the X ring is edge-on from above");
    }

    #[test]
    fn a_pointer_on_a_projected_ring_hits_that_world_axis() {
        let camera = Camera::default();
        let pivot = camera.target();
        let radius = world_radius(camera, TEST_RING_FACTOR);
        let axis_world = Axis::Z.direction();
        let points = ring_screen_points(camera, viewport(), pivot, axis_world, radius).expect("ring");
        let pointer = points[1];
        assert_eq!(hit_ring(camera, viewport(), pivot, pointer, TEST_RING_FACTOR), Some(Axis::Z));
        assert_eq!(hit_ring(camera, viewport(), pivot, Pos2::new(790.0, 590.0), TEST_RING_FACTOR), None);
    }

    #[test]
    fn the_ring_radius_follows_the_camera_distance_and_its_factor() {
        let camera = Camera::default();
        let small = world_radius(camera, 0.02);
        let large = world_radius(camera, 0.2);
        assert!(large > small, "a larger factor draws a larger ring: {small} vs {large}");

        let mut closer = camera;
        closer.zoom(0.5);
        assert!(
            world_radius(closer, TEST_RING_FACTOR) < world_radius(camera, TEST_RING_FACTOR),
            "the ring keeps its share of the camera distance"
        );
    }

    /// A project whose root holds a board and a wall the board's turn reaches.
    ///
    /// The board is an object 500 x 20 x 700 mm with its own base corner at the world origin; the wall
    /// is a slab whose face stands at y = 200, inside the sweep of the board's far end. A turn about Z
    /// is what a hinge makes of it (X88).
    fn board_and_wall() -> (model::Project, Id, Id) {
        use model::{Container, Geometry, Mesh};
        let root = Container::new(Id::new(1).expect("id"), "Root", Placement::IDENTITY, Vec::new());
        let mut project = model::Project::new(root).expect("a project");
        let root = project.root().id();
        let at = |point: DVec3| {
            Placement::IDENTITY
                .with_translation(
                    Length::from_micrometres((point.x * 1000.0).round() as i64),
                    Length::from_micrometres((point.y * 1000.0).round() as i64),
                    Length::from_micrometres((point.z * 1000.0).round() as i64),
                )
                .expect("a placement")
        };
        let box_mesh = |size: (i64, i64, i64)| {
            let half = [size.0 / 2, size.1 / 2, size.2 / 2];
            let length = |value: i64| Length::from_millimetres(value).expect("a length");
            Mesh::cuboid(half.map(|value| length(-value)), half.map(length)).expect("a box")
        };
        let board = project
            .add_object(root, "Board", at(DVec3::new(250.0, 10.0, 350.0)), Geometry::Mesh(box_mesh((500, 20, 700))))
            .expect("a board");
        let wall = project
            .add_object(root, "Wall", at(DVec3::new(0.0, 210.0, 350.0)), Geometry::Mesh(box_mesh((3000, 20, 700))))
            .expect("a wall");
        (project, board, wall)
    }

    #[test]
    fn a_measured_turn_stops_at_the_body_and_a_free_one_keeps_the_hands_angle() {
        // X88's split, at the gizmo: the same drag asks the rule and stops on the wall's own face, or
        // takes the hand's angle and passes through. The release says which of the two it was, so the
        // write is refused where the ring was free (ADR 0180).
        let (project, board, wall) = board_and_wall();
        let rule = project.rotate_rule(&[board], Some(&contact_parry::Parry)).expect("a rotate rule");

        let mut measured = drag(Axis::Z, FRAC_PI_2);
        measured.pivot = rule.pivot();
        measured.rule = Some(rule);
        measured.measured = true;
        measured.measure();
        assert!(measured.granted > 0.0 && measured.granted < FRAC_PI_2, "the turn is cut: {measured:?}");
        assert_eq!(measured.stop.and_then(model::Stop::node), Some(wall), "and the wall is what stopped it");
        assert!(measured.measured, "the object turn's release is measured");

        let mut free = drag(Axis::Z, FRAC_PI_2);
        free.measure();
        assert_eq!(free.granted, FRAC_PI_2, "the free turn keeps the hand's own angle");
        assert_eq!(free.stop, None);
        assert!(!free.measured, "and its release writes what it showed");

        // The preview draws the granted angle, so a ring pressed into a wall does not show a turn the
        // release would refuse.
        let gizmo = RotateGizmo { drag: Some(measured), hover: None, consumed_click: false };
        let preview = gizmo.preview().expect("the turned board is previewed");
        let Preview::Rotation { rotation, .. } = preview else {
            panic!("a ring drag previews a rotation");
        };
        let expected = DQuat::from_axis_angle(DVec3::Z, gizmo.drag.as_ref().expect("a drag").granted);
        assert!((rotation.dot(expected).abs() - 1.0).abs() < 1.0e-12, "the preview draws the granted turn");
    }
}
