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

use eframe::egui::{self, FontId, Pos2, Rect, Stroke, Ui};
use geom::DVec3;
use model::{Project, Sight};
use scene::Camera;
use ui::{Measure, MeasureStage, theme};
use units::Length;

use crate::gizmo_math::{
    Aim, PlaneSnap, WorkPlane, length_from_millimetres, paint_snap_marker, screen_point, surface_under,
};
/// Radius of a placed measurement point, in points.
const POINT_RADIUS: f32 = 3.0;
/// Gap between the pointer and the length label, in points.
const LABEL_OFFSET: egui::Vec2 = egui::vec2(14.0, 10.0);
/// Padding inside the length label, in points.
const LABEL_PADDING: egui::Vec2 = egui::vec2(5.0, 3.0);

/// Everything one pointer frame of the ruler carries besides the document and the pointer response.
///
/// The camera and the rectangle it draws in, the workspace's snap kinds, the chrome under the
/// pointer and the keyboard travel together: a frame reads them once, and the ruler's signature was
/// already too long for clippy without them (U116).
#[derive(Clone, Copy, Debug)]
pub(crate) struct MeasureInput {
    /// The camera the ruler measures through.
    pub(crate) camera: Camera,
    /// The viewport rectangle the ruler draws in.
    pub(crate) rect: Rect,
    /// Which snaps the workspace allows.
    pub(crate) kinds: model::SnapKinds,
    /// Whether the chrome owns the pointer this frame.
    pub(crate) pointer_over_widget: bool,
    /// The keyboard state a click carries.
    pub(crate) modifiers: MeasureModifiers,
}

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
    /// What captured it, or `None` for the pointer's own point.
    pub(crate) kind: Option<model::SnapKind>,
    /// Whether Shift's axis lock moved it onto a plane axis through the first point.
    pub(crate) locked: bool,
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

    /// What captured the pointer in the last frame, for the status bar's readout.
    pub(crate) fn snap_kind(&self) -> Option<model::SnapKind> {
        self.hover.and_then(|hover| hover.kind)
    }

    /// Applies the pointer to the armed ruler and reports whether the click changed the measurement.
    ///
    /// The pointer is resolved whether or not a click lands, so the value follows the hand before
    /// the second point is fixed. A measured ruler keeps its points, and the next click starts a new
    /// measurement through the atom.
    pub(crate) fn update(
        &mut self,
        response: &egui::Response,
        project: &Project,
        contact: &dyn geom::Contact,
        editor: &mut ui::Editor,
        input: MeasureInput,
    ) -> bool {
        let MeasureInput { camera, rect, .. } = input;
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
        // The ruler measures what the eye sees: a part switched off is crossed, not stopped at.
        let sight = editor.flags().sight();
        let aim = Aim { camera, rect, pointer };
        let Some(point) = resolve(project, Some(contact), aim, first, input.kinds, input.modifiers, sight) else {
            self.reset();
            return false;
        };
        self.hover = Some(point);
        if !measured {
            editor.measure_hover(point.point);
        }
        if input.pointer_over_widget || !response.clicked_by(egui::PointerButton::Primary) {
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
            paint_snap_marker(&painter, hover.screen, hover.kind, hover.locked, accent);
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
/// The surface under the ray is the tool's own reading — the probe steps past a product's reserved
/// box to the board behind it — and it wins because a face is a place the person can see. A ray that
/// meets nothing falls back to the work plane, and there the one resolver applies every kind the
/// workspace allows, the grid, the axis through the first point and Alt's "take the pointer as it
/// is" (ADR 0051, U116). A point that is not a finite micrometre value is no point at all, so a
/// broken camera or a hostile document resolves to `None` instead of a number.
fn resolve(
    project: &Project,
    contact: Option<&dyn geom::Contact>,
    aim: Aim,
    first: Option<[Length; 3]>,
    kinds: model::SnapKinds,
    modifiers: MeasureModifiers,
    sight: Sight<'_>,
) -> Option<MeasurePoint> {
    let Aim { camera, rect, pointer } = aim;
    let plane = WorkPlane::from_camera(camera);
    let face = if modifiers.ignore_snaps {
        None
    } else {
        PlaneSnap::at(camera, rect, pointer, plane)
            .and_then(|snap| face_under(project, contact, snap.ray.origin(), snap.ray.direction(), sight))
    };
    let Some(snap) = PlaneSnap::at(camera, rect, pointer, plane) else {
        return face.and_then(|point| measure_point(point, camera, rect, Some(model::SnapKind::Face), false));
    };
    let anchor = first.map(world_point);
    let kinds = if modifiers.ignore_snaps { model::SnapKinds::none() } else { kinds };
    // The ruler measures what the eye sees, so it takes the candidate where it is: a corner 300 mm
    // above the floor is a place of its own, not a shadow, and the mode never reaches it (U119).
    let target = model::SnapTarget::Free;
    let answer =
        project.snap(snap.request(kinds, target, anchor, &[], modifiers.axis_lock && anchor.is_some(), face), sight)?;
    measure_point(answer.point, camera, rect, answer.kind, answer.locked)
}

/// The first surface the ray really meets, stepping past the near face of a product's reserved box.
///
/// A product envelope is a probe surface (ADR 0103), but from outside the piece it is the first
/// thing the ray meets, so a ruler that took it would report the front of the reserved box while
/// the person points at a shelf behind it. [`surface_under`] steps past it and answers the point of
/// the face, which is the place this ruler measures.
fn face_under(
    project: &Project,
    contact: Option<&dyn geom::Contact>,
    origin: DVec3,
    direction: DVec3,
    sight: Sight<'_>,
) -> Option<DVec3> {
    let contact = contact?;
    surface_under(project, contact, origin, direction, sight).map(|probe| probe.face.point)
}

/// One world point in document units and on screen, or nothing when it is not representable.
fn measure_point(
    world: DVec3,
    camera: Camera,
    rect: Rect,
    kind: Option<model::SnapKind>,
    locked: bool,
) -> Option<MeasurePoint> {
    let point =
        [length_from_millimetres(world.x)?, length_from_millimetres(world.y)?, length_from_millimetres(world.z)?];
    let screen = screen_point(camera, rect, world)?;
    Some(MeasurePoint { point, screen, kind, locked })
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
