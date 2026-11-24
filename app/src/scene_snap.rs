//! The one point a click in space names: the surface under the pointer, or the snap resolver's own.
//!
//! Two tools name a place in the world this way — the ruler (ADR 0118) and the board drawn by points
//! (U126) — and neither may grow a second copy of the rule. The resolution is three steps, in this
//! order, and it is the order that makes a click predictable:
//!
//! 1. the first surface along the view ray, found by the one probe and stepped past a product's
//!    reserved box, because a face is a place the person can see (ADR 0103);
//! 2. the snap resolver's own candidate above the plane through the ray's foot, with every kind the
//!    workspace allows, the grid, the axis through the first point and Alt's "take the pointer as it
//!    is" (ADR 0051, 0199, U116);
//! 3. the point the eye sees, taken as it is rather than as its foot, because a corner 300 mm above
//!    the floor is a place of its own (U119).
//!
//! A point that is not a representable micrometre value is no point at all, so a broken camera or a
//! hostile document answers `None` instead of a number.
//!
//! Changes when: the resolver's order, its snap target or what counts as a place changes.

use eframe::egui::{Pos2, Rect};
use geom::DVec3;
use model::{Project, Sight, SnapKind, SnapKinds, SnapTarget};
use scene::Camera;
use units::Length;

use crate::gizmo_math::{Aim, PlaneSnap, WorkPlane, length_from_millimetres, screen_point, surface_under};

/// What a tool lets the resolver do, read once per frame.
#[derive(Clone, Copy)]
pub(crate) struct SceneSnap<'a> {
    /// Which snaps the workspace allows.
    pub(crate) kinds: SnapKinds,
    /// The adapter the probe measures the bodies the boxes cannot describe with.
    pub(crate) contact: Option<&'a dyn geom::Contact>,
    /// The point the axis lock measures from, when the tool has one.
    pub(crate) first: Option<[Length; 3]>,
    /// Shift's axis lock: the point keeps one axis of the first one.
    pub(crate) axis_lock: bool,
    /// Alt takes the pointer as it is: no surface and no grid.
    pub(crate) ignore_snaps: bool,
    /// What the eye sees, so a part switched off is crossed rather than stopped at.
    pub(crate) sight: Sight<'a>,
}

/// The point a click would name, in world micrometres, and where it draws.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ScenePoint {
    /// The point in world micrometres, exactly as a command would store it.
    pub(crate) point: [Length; 3],
    /// Where that point draws on the viewport.
    pub(crate) screen: Pos2,
    /// What captured it, or `None` for the pointer's own point.
    pub(crate) kind: Option<SnapKind>,
    /// Whether the axis lock moved it onto an axis through the tool's first point.
    pub(crate) locked: bool,
}

/// The point the pointer names this frame, or `None` when it names no place.
pub(crate) fn resolve(project: &Project, aim: Aim, snap: SceneSnap<'_>) -> Option<ScenePoint> {
    let Aim { camera, rect, pointer } = aim;
    let plane = WorkPlane::from_camera(camera);
    let plane_snap = PlaneSnap::at(camera, rect, pointer, plane);
    let face = if snap.ignore_snaps {
        None
    } else {
        plane_snap.and_then(|hit| face_under(project, snap.contact, hit.ray.origin(), hit.ray.direction(), snap.sight))
    };
    let Some(plane_snap) = plane_snap else {
        return face.and_then(|point| place(point, camera, rect, Some(SnapKind::Face), false));
    };
    let anchor = snap.first.map(world_point);
    let kinds = if snap.ignore_snaps { SnapKinds::none() } else { snap.kinds };
    let locked = snap.axis_lock && anchor.is_some();
    let answer = project.snap(plane_snap.request(kinds, SnapTarget::Free, anchor, &[], locked, face), snap.sight)?;
    place(answer.point, camera, rect, answer.kind, answer.locked)
}

/// The first surface the ray really meets, stepping past the near face of a product's reserved box.
///
/// A product envelope is a probe surface (ADR 0103), but from outside the piece it is the first
/// thing the ray meets, so a tool that took it would report the front of the reserved box while the
/// person points at a shelf behind it. [`surface_under`] steps past it and answers the point of the
/// face, which is the place a click names.
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
fn place(world: DVec3, camera: Camera, rect: Rect, kind: Option<SnapKind>, locked: bool) -> Option<ScenePoint> {
    let point =
        [length_from_millimetres(world.x)?, length_from_millimetres(world.y)?, length_from_millimetres(world.z)?];
    let screen = screen_point(camera, rect, world)?;
    Some(ScenePoint { point, screen, kind, locked })
}

/// A stored point back in world millimetres, the boundary the geometry works in.
pub(crate) fn world_point(point: [Length; 3]) -> DVec3 {
    DVec3::new(point[0].millimetres_f64(), point[1].millimetres_f64(), point[2].millimetres_f64())
}
