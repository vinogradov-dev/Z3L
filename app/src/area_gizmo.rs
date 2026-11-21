//! The area pull's pointer: the face of a drawing under the cursor, and the highlight that names it.
//!
//! The atom ([`ui::Area`]) holds the point and the face; this module resolves a viewport position to
//! a point in the drawing's own plane and paints what the atom read. It is the pull's only input
//! path, so the highlight and the command the click dispatches can never name two different points
//! (G4).
//!
//! Nothing here writes: the highlight is `egui` paint, the preview of the pulled body is the session
//! overlay, and the click is one `object.area.extrude`.
//!
//! Changes when: the face stops being drawn in screen space, or the pointer stops resolving to a
//! point in the drawing's own plane.

use eframe::egui::{self, Mesh, Pos2, Rect, Shape, Stroke, Ui};
use geom::{DVec3, Region};
use model::{Id, Placement, Project};
use scene::Camera;
use ui::theme;
use units::Length;

use crate::gizmo_math::{ray_to, screen_point};

/// How far off the drawing's plane a ray may run before the plane is edge-on, in the plane's normal.
///
/// A ray that runs along the plane meets it everywhere, so a pointer over a drawing seen edge-on
/// names no point rather than a point a thousand millimetres away.
const EDGE_ON: f64 = 1.0e-6;

/// Tolerance at which a face is sampled for the screen, in millimetres.
///
/// The highlight follows a board's edge to a fifth of a millimetre, which is finer than a point on
/// screen at any zoom a maker works at, and coarse enough that a circle stays a short chain.
const SAMPLE_MM: f64 = 0.2;

/// The point under the pointer in the drawing's own plane, in micrometres.
///
/// The ray is carried into the node's frame, where the drawing lies on `z = 0`; a ray that meets the
/// plane behind the camera, or runs along it, names nothing.
pub(crate) fn point_at(
    project: &Project,
    object: Id,
    camera: Camera,
    rect: Rect,
    pointer: Pos2,
) -> Option<[Length; 2]> {
    let placement = project.world_placement(object)?;
    let inverse = placement.inverse().ok()?;
    let ray = ray_to(pointer, rect, camera);
    let origin = inverse.transform_point(ray.origin());
    let direction = inverse.transform_vector(ray.direction());
    if !origin.is_finite() || !direction.is_finite() || direction.z.abs() <= EDGE_ON {
        return None;
    }
    let distance = -origin.z / direction.z;
    if !distance.is_finite() || distance <= 0.0 {
        return None;
    }
    let point = origin + direction * distance;
    Some([Length::from_millimetres_f64(point.x)?, Length::from_millimetres_f64(point.y)?])
}

/// The area pull's paint: the face under the pointer, filled and outlined.
///
/// The face is drawn as the plate it is, so a maker reads which area the click would take before the
/// click. A face that cannot be pulled yet — one with a hole — is drawn as its boundaries only, in
/// the warning ink, and the status bar carries the sentence: colour alone never states a refusal
/// (X97).
pub(crate) fn paint(ui: &Ui, rect: Rect, camera: Camera, project: &Project, object: Id, area: &Region) {
    let Some(placement) = project.world_placement(object) else {
        return;
    };
    let painter = ui.painter().with_clip_rect(rect);
    let pullable = area.holes().is_empty();
    let outline = if pullable { theme::accent(ui) } else { theme::warn(ui) };
    let Some(outer) = ring_points(placement, camera, rect, area.outer()) else {
        return;
    };
    for hole in area.holes() {
        if let Some(points) = ring_points(placement, camera, rect, hole) {
            painter.add(Shape::closed_line(points, Stroke::new(theme::HELPER_STROKE, outline)));
        }
    }
    let sampled_outline = Shape::closed_line(outer.clone(), Stroke::new(theme::HELPER_STROKE, outline));
    if !pullable {
        painter.add(sampled_outline);
        return;
    }
    if let Some(mesh) = filled(&outer, theme::at_opacity(theme::accent_fill(ui), 26)) {
        painter.add(mesh);
    }
    painter.add(sampled_outline);
}

/// The face as a translucent mesh, or `None` when it is too fine or too thin to fill.
fn filled(points: &[Pos2], fill: egui::Color32) -> Option<Shape> {
    let world: Vec<DVec3> = points.iter().map(|point| DVec3::new(point.x as f64, point.y as f64, 0.0)).collect();
    let triangles = geom::triangulate_face(&world).ok()?;
    let vertices: Vec<egui::epaint::Vertex> = points
        .iter()
        .map(|point| egui::epaint::Vertex { pos: *point, uv: egui::epaint::WHITE_UV, color: fill })
        .collect();
    let mut mesh = Mesh { indices: Vec::with_capacity(triangles.len() * 3), vertices, ..Mesh::default() };
    for [a, b, c] in triangles {
        mesh.indices.extend([a, b, c]);
    }
    Some(Shape::mesh(mesh))
}

/// One sampled ring of the face in screen points, or `None` when the camera sees too little of it.
fn ring_points(placement: Placement, camera: Camera, rect: Rect, ring: &geom::Contour) -> Option<Vec<Pos2>> {
    let samples = ring.sample(SAMPLE_MM).ok()?;
    let mut points = Vec::with_capacity(samples.len());
    for point in samples {
        let world = placement.transform_point(DVec3::new(point.x, point.y, 0.0));
        points.push(screen_point(camera, rect, world)?);
    }
    (points.len() >= 3).then_some(points)
}
