//! The ruler's pointer: the snapped point under the cursor, the guide and the live length.
//!
//! The atom (`ui::Measure`) holds the two points and the value; this module resolves a
//! pointer to a point and paints what the next click would do. It is the ruler's only input path and
//! it reuses the shared mechanisms rather than copying them (ADR 0118):
//!
//! - the one probe (ADR 0103) answers the first furniture surface along the view ray, so a click
//!   lands on the face the person points at;
//! - when the ray meets nothing, the point is on the work plane through the one snap resolver
//!   (ADR 0051), with the 10 mm grid, the axis lock through the first point and Alt to take the
//!   pointer as it is.
//!
//! Nothing here writes: there is no document field, no revision and no undo entry, and the whole
//! overlay is `egui` paint. The status bar reads the same value from the atom, so the number at the
//! pointer and the number in the bar can never disagree.
//!
//! Changes when: the ruler gains a method, or the point it measures from changes.

use eframe::egui::{self, FontId, Pos2, Rect, Shape, Stroke, Ui};
use geom::DVec3;
use model::{Project, SurfaceKind};
use scene::Camera;
use ui::{Measure, MeasureStage, theme};
use units::Length;

use crate::gizmo_math::{
    SnapMode, SnapRequest, SnapTarget, WorkPlane, length_from_millimetres, ray_to, screen_point, snap_point,
};

/// Pointer distance inside which a snapped point draws its marker, in points.
const SNAP_RADIUS_POINTS: f32 = 10.0;
/// Half-diagonal of the square that marks the point a click would use, in points.
const MARKER_SIZE: f32 = 5.0;
/// Radius of a placed measurement point, in points.
const POINT_RADIUS: f32 = 3.0;
/// Gap between the pointer and the length label, in points.
const LABEL_OFFSET: egui::Vec2 = egui::vec2(14.0, 10.0);
/// Padding inside the length label, in points.
const LABEL_PADDING: egui::Vec2 = egui::vec2(5.0, 3.0);
/// How far inside a product's shell the ray restarts, in millimetres.
///
/// The reserved box around a product is a surface of the probe (ADR 0103), but the maker points at
/// the boards inside it, not at the near face of the box. A step of a micrometre is enough to leave
/// the face and small enough to keep every real surface in front of the point.
const SHELL_INSET_MM: f64 = 1.0e-3;
/// The product envelopes one ray may step through before the point is taken as it is.
///
/// Nested products are the awkward case; the bound keeps a pathological document from spinning.
const MAX_SHELLS: usize = 4;

/// The keyboard state a click carries while the ruler runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct MeasureModifiers {
    /// Shift locks the second point to the plane axis through the first.
    pub(crate) axis_lock: bool,
    /// Alt measures on the work plane and takes the pointer as it is: no surface, no grid.
    pub(crate) ignore_snaps: bool,
}

/// The point a click would measure, why it moved, and where it draws.
#[derive(Clone, Copy, Debug)]
pub(crate) struct MeasurePoint {
    /// The point in world micrometres, exactly as the atom stores it.
    pub(crate) point: [Length; 3],
    /// Where that point draws on the viewport.
    pub(crate) screen: Pos2,
    /// Why the point moved, for the marker.
    pub(crate) target: SnapTarget,
}

/// The ruler's live pointer state: the snapped point, or nothing while it waits for one.
#[derive(Debug, Default)]
pub(crate) struct MeasureGizmo {
    hover: Option<MeasurePoint>,
}

impl MeasureGizmo {
    /// Drops the hovered point, for example when the tool is disarmed.
    pub(crate) fn reset(&mut self) {
        self.hover = None;
    }

    /// The point the last frame resolved, for a test that reads what the pointer would measure.
    #[cfg(test)]
    pub(crate) fn hover(&self) -> Option<MeasurePoint> {
        self.hover
    }

    /// Applies the pointer to the armed ruler and reports whether the click changed the measurement.
    ///
    /// The pointer is resolved whether or not a click lands, so the value follows the hand before
    /// the second point is fixed. A measured ruler keeps its points, and the next click starts a new
    /// measurement through the atom.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn update(
        &mut self,
        response: &egui::Response,
        rect: Rect,
        camera: Camera,
        project: &Project,
        editor: &mut ui::Editor,
        grid: bool,
        pointer_over_widget: bool,
        modifiers: MeasureModifiers,
    ) -> bool {
        let Some(measure) = editor.measure() else {
            self.reset();
            return false;
        };
        let first = measure.first();
        let measured = measure.is_measured();
        let pointer = response.hover_pos().or_else(|| response.interact_pointer_pos());
        let Some(pointer) = pointer else {
            self.reset();
            return false;
        };
        let Some(point) = resolve(project, camera, rect, pointer, first, grid, modifiers) else {
            self.reset();
            return false;
        };
        self.hover = Some(point);
        if !measured {
            editor.measure_hover(point.point);
        }
        if pointer_over_widget || !response.clicked_by(egui::PointerButton::Primary) {
            return false;
        }
        editor.measure_click(point.point).is_some()
    }

    /// Paints the guide line, the placed points, the snapped marker and the live length.
    pub(crate) fn paint(&self, ui: &Ui, rect: Rect, camera: Camera, measure: &Measure) {
        let painter = ui.painter().with_clip_rect(rect);
        let accent = theme::accent(ui);
        let first = measure.first().and_then(|point| draw_point(camera, rect, point));
        let current = measure.current().and_then(|point| draw_point(camera, rect, point));
        if measure.stage() != MeasureStage::First
            && let (Some(first), Some(current)) = (first, current)
        {
            painter.line_segment([first, current], Stroke::new(1.5, accent));
        }
        if let Some(first) = first {
            painter.circle_filled(first, POINT_RADIUS, accent);
        }
        if let Some(current) = current {
            painter.circle_filled(current, POINT_RADIUS, accent);
        }
        if let Some(hover) = self.hover {
            paint_marker(&painter, hover, accent);
        }
        let Some(length) = measure.value() else {
            return;
        };
        // The live number sits at the hand; the fixed one stays at the middle of the line, so a
        // measured value does not chase the pointer away.
        let anchor =
            if measure.is_measured() { midpoint(first, current) } else { self.hover.map(|hover| hover.screen) };
        if let Some(anchor) = anchor {
            paint_label(&painter, rect, anchor, &length.to_string(), ui);
        }
    }
}

/// Resolves a viewport position to the point a click would measure.
///
/// The surface under the ray wins, because a face is a place the person can see; a ray that meets
/// nothing falls back to the work plane, and there the one resolver applies the grid, the axis
/// through the first point and Alt. A point that is not a finite micrometre value is no point at
/// all, so a broken camera or a hostile document resolves to `None` instead of a number.
fn resolve(
    project: &Project,
    camera: Camera,
    rect: Rect,
    pointer: Pos2,
    first: Option<[Length; 3]>,
    grid: bool,
    modifiers: MeasureModifiers,
) -> Option<MeasurePoint> {
    let ray = ray_to(pointer, rect, camera);
    if !modifiers.ignore_snaps
        && let Some(probe) = surface_under(project, ray.origin(), ray.direction())
    {
        return measure_point(probe, camera, rect, SnapTarget::Surface);
    }
    let plane = WorkPlane::from_camera(camera);
    let world = plane.ray_point(ray)?;
    let depth = (world - camera.eye()).dot(camera.forward());
    let radius = camera.world_per_point(depth, f64::from(rect.height())) * f64::from(SNAP_RADIUS_POINTS);
    let anchor = first.map(|point| plane.local(world_point(point)));
    let snapped = snap_point(SnapRequest {
        point: plane.local(world),
        anchor,
        ends: &[],
        mode: if grid { SnapMode::Grid } else { SnapMode::Free },
        axis_lock: modifiers.axis_lock && anchor.is_some(),
        end_radius: radius,
        ignore_snaps: modifiers.ignore_snaps,
    });
    measure_point(plane.world(snapped.point), camera, rect, snapped.target)
}

/// The first surface the ray really meets, stepping past the near face of a product's reserved box.
///
/// A product envelope is a probe surface (ADR 0103), but from outside the piece it is the first
/// thing the ray meets, so a ruler that took it would report the front of the reserved box while
/// the person points at a shelf behind it. The ray restarts a micrometre inside the envelope and
/// looks again, at most [`MAX_SHELLS`] times, so the boards inside win. Every other surface — a
/// panel, a body, the far envelope face of the product the point is now in — is the answer.
fn surface_under(project: &Project, origin: DVec3, direction: DVec3) -> Option<DVec3> {
    let mut from = origin;
    for _ in 0..=MAX_SHELLS {
        let probe = project.probe(from, direction)?;
        if probe.hit.kind != SurfaceKind::Shell {
            return Some(probe.face.point);
        }
        from = probe.face.point + direction * SHELL_INSET_MM;
    }
    None
}

/// One world point in document units and on screen, or nothing when it is not representable.
fn measure_point(world: DVec3, camera: Camera, rect: Rect, target: SnapTarget) -> Option<MeasurePoint> {
    let point =
        [length_from_millimetres(world.x)?, length_from_millimetres(world.y)?, length_from_millimetres(world.z)?];
    let screen = screen_point(camera, rect, world)?;
    Some(MeasurePoint { point, screen, target })
}

/// A stored point back in world millimetres, the boundary the geometry works in.
fn world_point(point: [Length; 3]) -> DVec3 {
    DVec3::new(point[0].millimetres_f64(), point[1].millimetres_f64(), point[2].millimetres_f64())
}

/// A stored point on the viewport, or `None` when it is behind the camera.
fn draw_point(camera: Camera, rect: Rect, point: [Length; 3]) -> Option<Pos2> {
    screen_point(camera, rect, world_point(point))
}

/// The middle of the measured segment, for the fixed label.
fn midpoint(first: Option<Pos2>, current: Option<Pos2>) -> Option<Pos2> {
    let (first, current) = (first?, current?);
    Some(first + (current - first) * 0.5)
}

/// The marker that says what the next click would use, drawn before the click so the snap is never
/// invisible (ADR 0051).
fn paint_marker(painter: &egui::Painter, hover: MeasurePoint, ink: egui::Color32) {
    // A surface point is exact and reads solid; a grid or axis point is a construction, so it is an
    // outline; a free point promises nothing and keeps the thinnest stroke.
    let (stroke, filled) = match hover.target {
        SnapTarget::Surface => (Stroke::new(1.6, ink), true),
        SnapTarget::Grid | SnapTarget::Axis => (Stroke::new(1.4, ink), false),
        SnapTarget::End | SnapTarget::Free => (Stroke::new(1.2, ink), false),
    };
    let center = hover.screen;
    let corners = vec![
        Pos2::new(center.x, center.y - MARKER_SIZE),
        Pos2::new(center.x + MARKER_SIZE, center.y),
        Pos2::new(center.x, center.y + MARKER_SIZE),
        Pos2::new(center.x - MARKER_SIZE, center.y),
    ];
    if filled {
        painter.add(Shape::convex_polygon(corners.clone(), stroke.color, Stroke::NONE));
    }
    painter.add(Shape::closed_line(corners, stroke));
}

/// The length next to the pointer on a readable background, kept inside the viewport.
fn paint_label(painter: &egui::Painter, rect: Rect, anchor: Pos2, label: &str, ui: &Ui) {
    let galley = painter.layout_no_wrap(label.to_owned(), FontId::proportional(12.0), theme::text(ui));
    let size = galley.size() + LABEL_PADDING * 2.0;
    let mut at = anchor + LABEL_OFFSET;
    if at.x + size.x > rect.right() {
        at.x = anchor.x - size.x - LABEL_OFFSET.x;
    }
    if at.y + size.y > rect.bottom() {
        at.y = anchor.y - size.y - LABEL_OFFSET.y;
    }
    let at = egui::pos2(at.x.max(rect.left()), at.y.max(rect.top()));
    let background = theme::at_opacity(theme::chrome(ui), 82);
    painter.rect_filled(Rect::from_min_size(at, size), 3.0, background);
    painter.galley(at + LABEL_PADDING, galley, theme::text(ui));
}
