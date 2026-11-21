//! The viewport vertex gizmo for sub-object mesh editing.
//!
//! Three world-axis handles sit at the median of the selected vertices. A drag never mutates the
//! document: it turns the world-space shift into a local offset, previews the moved mesh and sends
//! every selected vertex to its new absolute local position in one `mesh.vertices.move` on release.
//!
//! Vertices are stored in the object's local frame, so the offset is rotated by the inverse of the
//! object's world placement. Rounding to whole micrometres at that boundary keeps the released
//! positions exactly what the preview showed.

use eframe::egui::{self, Response, Ui};
use geom::DVec3;
use model::{Id, Project, Sight, SnapKind};
use scene::{Camera, MAX_DISTANCE_MM};
use units::Length;

use crate::gizmo_math::{
    ACTIVE_COLOR, Axis, HandleSpec, PlaneSnap, PreviewKey, WorkPlane, editable_axes, hit_axis, paint_axis_handles,
    paint_snap_marker, ray_to, screen_point,
};
use crate::mesh_pick;

/// One continuous vertex drag.
#[derive(Debug)]
struct VertexDrag {
    object: Id,
    vertices: Vec<u32>,
    starts: Vec<[Length; 3]>,
    axis: Axis,
    start_origin: DVec3,
    start_parameter: f64,
    offset_world: DVec3,
    offset_local: [Length; 3],
}

/// The vertices a drag is moving in one object, with the offset they all share.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct VertexPreview<'a> {
    /// The object whose vertices move.
    pub(crate) object: Id,
    /// The selected vertex indices.
    pub(crate) vertices: &'a [u32],
    /// The local offset of every selected vertex, in micrometres.
    pub(crate) delta: [Length; 3],
}

impl VertexPreview<'_> {
    /// The previewed position of `vertex`, or `None` when it is not part of this preview.
    pub(crate) fn moved(&self, vertex: u32, position: [Length; 3]) -> Option<[Length; 3]> {
        if !self.vertices.contains(&vertex) {
            return None;
        }
        offset_position(position, self.delta)
    }
}

/// The batch a finished vertex drag asks the shell to dispatch as one `mesh.vertices.move`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MeshVertexCommit {
    /// The edited object.
    pub(crate) object: Id,
    /// One absolute local position per moved vertex.
    pub(crate) moves: Vec<(u32, [Length; 3])>,
}

/// What the vertex gizmo edits this frame: the object, its selected vertices and the document.
///
/// Grouped into one value so the gizmo entry points stay readable and a caller cannot pass another
/// object's vertices by mistake.
#[derive(Clone, Copy, Debug)]
pub(crate) struct MeshTarget<'a> {
    /// The camera the viewport uses.
    pub(crate) camera: Camera,
    /// The document the object and its world placement are read from.
    pub(crate) project: &'a Project,
    /// The object whose vertices move.
    pub(crate) object: Id,
    /// The selected vertex indices, in selection order.
    pub(crate) vertices: &'a [u32],
    /// Which snaps the workspace allows: every kind, because a vertex of a mesh is part of a solid
    /// and may land on any place the scene offers (U119).
    pub(crate) kinds: model::SnapKinds,
    /// The parts a query reads as switched off, so a hidden board answers no snap.
    pub(crate) sight: Sight<'a>,
}

/// Persistent state of the vertex gizmo.
#[derive(Debug, Default)]
pub(crate) struct VertexGizmo {
    drag: Option<VertexDrag>,
    hover: Option<Axis>,
    consumed_click: bool,
    /// The capture the resolver answered with while a vertex was dragged: what took the pointer and
    /// the world point it answers, so the glyph names the place the release writes (U119).
    snap: Option<(SnapKind, DVec3)>,
}

impl VertexGizmo {
    /// Whether a vertex drag is in progress.
    pub(crate) fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// Whether this frame's pointer release belongs to the gizmo and must not select.
    pub(crate) fn consumed_click(&self) -> bool {
        self.consumed_click
    }

    /// Forgets a drag when the editor leaves mesh mode or switches object.
    pub(crate) fn reset(&mut self) {
        self.drag = None;
        self.hover = None;
        self.consumed_click = false;
        self.snap = None;
    }

    /// The capture the pointer answered with while a vertex was dragged, for a test and the status
    /// bar: what took it and where it put the vertex (U119).
    pub(crate) fn snap(&self) -> Option<(SnapKind, DVec3)> {
        self.snap
    }

    /// The moved vertices the renderer and the markers should preview, if a drag moved them.
    pub(crate) fn preview(&self) -> Option<VertexPreview<'_>> {
        let drag = self.drag.as_ref()?;
        if drag.offset_local == [Length::ZERO; 3] {
            return None;
        }
        Some(VertexPreview { object: drag.object, vertices: &drag.vertices, delta: drag.offset_local })
    }

    /// The preview identity used to avoid re-uploading an unchanged drag.
    pub(crate) fn preview_key(&self) -> Option<PreviewKey> {
        let preview = self.preview()?;
        Some(PreviewKey::Vertex {
            object: preview.object,
            offset: [preview.delta[0].micrometres(), preview.delta[1].micrometres(), preview.delta[2].micrometres()],
        })
    }

    /// Starts, continues or finishes a vertex drag and returns the batch to dispatch on release.
    ///
    /// The caller runs this only while the editor is in mesh mode on `object`; `vertices` is the
    /// current vertex selection. If a drag is active it owns the pointer until release or Escape.
    /// `handle_length` is the screen length of one handle, in points, from the workspace settings
    /// (U86); the drawing and the hit test are handed the same value.
    pub(crate) fn update(
        &mut self,
        ui: &Ui,
        response: &Response,
        viewport: egui::Rect,
        target: MeshTarget<'_>,
        pointer_over_widget: bool,
        handle_length: f32,
    ) -> Result<Option<MeshVertexCommit>, String> {
        let MeshTarget { camera, project, object, vertices, .. } = target;
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
                && let Some(parameter) = ray_to(pointer, viewport, camera)
                    .closest_parameter_on_line(drag.start_origin, drag.axis.direction())
            {
                let (offset_world, capture) = snapped_offset(&drag, pointer, parameter, &target, viewport);
                self.snap = capture;
                drag.offset_world = offset_world;
                drag.offset_local =
                    world_offset_to_local(project, drag.object, drag.offset_world).unwrap_or([Length::ZERO; 3]);
            }
            if !down {
                self.consumed_click = true;
                self.snap = None;
                return Ok(drag.moved_positions().map(|moves| MeshVertexCommit { object: drag.object, moves }));
            }
            self.drag = Some(drag);
            return Ok(None);
        }

        self.hover = None;
        if pointer_over_widget || vertices.is_empty() {
            return Ok(None);
        }
        let Some(origin_world) = selection_origin(project, object, vertices) else {
            return Ok(None);
        };
        let Some(pointer) = response.hover_pos() else {
            return Ok(None);
        };
        let spec = HandleSpec { axes: editable_axes(project, object), length: handle_length };
        let Some(axis) = hit_axis(camera, viewport, origin_world, pointer, spec) else {
            return Ok(None);
        };
        self.hover = Some(axis);
        if !ui.input(|input| input.pointer.primary_pressed()) {
            return Ok(None);
        }
        let Some(parameter) =
            ray_to(pointer, viewport, camera).closest_parameter_on_line(origin_world, axis.direction())
        else {
            return Ok(None);
        };
        let (vertices, starts) = snapshot_vertices(project, object, vertices);
        if vertices.is_empty() {
            return Ok(None);
        }
        self.drag = Some(VertexDrag {
            object,
            vertices,
            starts,
            axis,
            start_origin: origin_world,
            start_parameter: parameter,
            offset_world: DVec3::ZERO,
            offset_local: [Length::ZERO; 3],
        });
        if !ui.input(|input| input.pointer.primary_down()) {
            self.drag = None;
            self.consumed_click = true;
        }
        Ok(None)
    }

    /// Draws the handles over the viewport.
    pub(crate) fn paint(&self, ui: &Ui, viewport: egui::Rect, target: MeshTarget<'_>, handle_length: f32) {
        let MeshTarget { camera, project, object, vertices, .. } = target;
        let origin_world = if let Some(drag) = &self.drag {
            drag.start_origin + drag.offset_world
        } else {
            let Some(origin) = selection_origin(project, object, vertices) else {
                return;
            };
            origin
        };
        paint_axis_handles(
            ui,
            viewport,
            camera,
            origin_world,
            self.hover,
            self.drag.as_ref().map(|drag| drag.axis),
            HandleSpec { axes: editable_axes(project, object), length: handle_length },
        );
        // The glyph names what took the pointer, and it stands on the point the release writes: the
        // handles' origin moved to that same place a line above (U119).
        if let Some((kind, point)) = self.snap()
            && let Some(at) = screen_point(camera, viewport, point)
        {
            paint_snap_marker(ui.painter(), at, Some(kind), false, ACTIVE_COLOR);
        }
    }
}

/// The world offset one drag hands its vertices, and the capture that decided it.
///
/// The axis's own offset is the answer until a candidate of the scene captures the pointer: a vertex
/// of a mesh is part of a solid, so a maker lines it up with another part's corner, edge or face, and
/// that is a place in space rather than a distance along one handle (U119). Every kind may capture —
/// the grid aside, which rounds wherever the pointer is and belongs to a drawing plane, not to a
/// solid — and the tool asks for the point itself ([`model::SnapTarget::Free`]), because a vertex
/// works in space and the workspace's 2.5D/3D mode does not apply to it.
fn snapped_offset(
    drag: &VertexDrag,
    pointer: egui::Pos2,
    parameter: f64,
    target: &MeshTarget<'_>,
    viewport: egui::Rect,
) -> (DVec3, Option<(SnapKind, DVec3)>) {
    let MeshTarget { camera, project, kinds, sight, .. } = *target;
    let scalar = (parameter - drag.start_parameter).clamp(-MAX_DISTANCE_MM, MAX_DISTANCE_MM);
    let axis_offset = drag.axis.direction() * scalar;
    if !scalar.is_finite() {
        return (DVec3::ZERO, None);
    }
    // The plane is the one the drag works at — through the point it carries, facing the eye — so the
    // kinds that live in a plane (a crossing, a world axis, the pointer's own point) have somewhere
    // to be measured, and a scene point stands off it rather than being dropped.
    let plane = WorkPlane::from_camera(camera).through(drag.start_origin);
    let answer = PlaneSnap::at(camera, viewport, pointer, plane).and_then(|aim| {
        let request =
            aim.request(model::SnapKinds { grid: false, ..kinds }, model::SnapTarget::Free, None, &[], false, None);
        project.snap(request, sight)
    });
    match answer.and_then(|answer| answer.kind.map(|kind| (kind, answer.point))) {
        Some((kind, point)) => (point - drag.start_origin, Some((kind, point))),
        None => (axis_offset, None),
    }
}

impl VertexDrag {
    /// The absolute positions a release should send, or `None` when nothing moved.
    ///
    /// A checked add keeps a hostile mesh position from wrapping; an overflow simply produces no
    /// commit instead of a corrupted vertex.
    fn moved_positions(&self) -> Option<Vec<(u32, [Length; 3])>> {
        if self.offset_local == [Length::ZERO; 3] {
            return None;
        }
        self.vertices
            .iter()
            .zip(&self.starts)
            .map(|(vertex, start)| Some((*vertex, offset_position(*start, self.offset_local)?)))
            .collect()
    }
}

/// The median world position of the selected vertices, or `None` for an empty or stale selection.
fn selection_origin(project: &Project, object: Id, vertices: &[u32]) -> Option<DVec3> {
    let (placement, mesh) = mesh_pick::vertex_frame(project, object)?;
    let mut sum = DVec3::ZERO;
    let mut count = 0.0;
    for vertex in vertices {
        let index = usize::try_from(*vertex).ok()?;
        let local = *mesh.positions().get(index)?;
        sum += mesh_pick::to_world(placement, local);
        count += 1.0;
    }
    (count > 0.0 && sum.is_finite()).then(|| sum / count)
}

/// Turns a world-space shift into the object's local frame, rounded to micrometres.
fn world_offset_to_local(project: &Project, object: Id, offset_world: DVec3) -> Option<[Length; 3]> {
    let world = project.world_placement(object)?;
    let local = world.rotation().inverse() * offset_world;
    if !local.is_finite() {
        return None;
    }
    let mut offset = [Length::ZERO; 3];
    for (slot, value) in offset.iter_mut().zip([local.x, local.y, local.z]) {
        let micrometres = value * 1_000.0;
        if !micrometres.is_finite() || micrometres < i64::MIN as f64 || micrometres > i64::MAX as f64 {
            return None;
        }
        *slot = Length::from_micrometres(micrometres.round() as i64);
    }
    Some(offset)
}

/// Adds two vertices component-wise, or `None` when a component leaves the `Length` range.
pub(crate) fn offset_position(position: [Length; 3], delta: [Length; 3]) -> Option<[Length; 3]> {
    Some([position[0].checked_add(delta[0])?, position[1].checked_add(delta[1])?, position[2].checked_add(delta[2])?])
}

/// The vertices that still exist with their starting local positions.
fn snapshot_vertices(project: &Project, object: Id, vertices: &[u32]) -> (Vec<u32>, Vec<[Length; 3]>) {
    let Some((_placement, mesh)) = mesh_pick::vertex_frame(project, object) else {
        return (Vec::new(), Vec::new());
    };
    let mut kept = Vec::with_capacity(vertices.len());
    let mut starts = Vec::with_capacity(vertices.len());
    for vertex in vertices {
        let Ok(index) = usize::try_from(*vertex) else {
            continue;
        };
        let Some(position) = mesh.positions().get(index) else {
            continue;
        };
        kept.push(*vertex);
        starts.push(*position);
    }
    (kept, starts)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use std::f64::consts::FRAC_PI_2;

    use geom::DQuat;
    use model::{Geometry, Placement};
    use testkit::{millimetres, sample_project};

    use super::*;

    fn cube_project() -> (Project, Id) {
        let mut project = sample_project();
        let root = project.root().id();
        let mesh = testkit::cube_mesh(millimetres(100));
        let id = project.add_object(root, "Cube", Placement::IDENTITY, Geometry::Mesh(mesh)).expect("object");
        (project, id)
    }

    #[test]
    fn the_gizmo_origin_is_the_median_of_the_selected_vertices() {
        let (project, cube) = cube_project();
        assert!(selection_origin(&project, cube, &[]).is_none());
        assert!(selection_origin(&project, cube, &[8]).is_none(), "a stale index has no origin");

        let origin = selection_origin(&project, cube, &[0, 1]).expect("origin");
        let (placement, mesh) = mesh_pick::vertex_frame(&project, cube).expect("mesh");
        let expected = (mesh_pick::to_world(placement, mesh.positions()[0])
            + mesh_pick::to_world(placement, mesh.positions()[1]))
            / 2.0;
        assert!((origin - expected).length() < 1.0e-12);
    }

    #[test]
    fn a_world_shift_is_rotated_into_the_object_frame() {
        let (mut project, cube) = cube_project();
        project
            .set_node_placement(
                cube,
                Placement::new(DVec3::new(500.0, 0.0, 0.0), DQuat::from_rotation_z(FRAC_PI_2)).expect("placement"),
            )
            .expect("turn the object");

        let offset = world_offset_to_local(&project, cube, DVec3::new(10.0, 0.0, 0.0)).expect("offset");
        assert_eq!(offset, [Length::ZERO, millimetres(-10), Length::ZERO]);
        assert!(world_offset_to_local(&project, cube, DVec3::new(f64::NAN, 0.0, 0.0)).is_none());
    }

    #[test]
    fn a_vertex_drag_lands_on_the_candidate_the_pointer_names() {
        // U119: the vertex editor asks the one resolver with every kind, and the point it stores is
        // the candidate itself — a mesh vertex is part of a solid, so it lines up with another
        // part's corner, edge or face. The tool declares `Free`, so the workspace's 2.5D/3D mode
        // does not apply to it: there is no plane in the request's answer, only a place in space.
        let (project, cube) = cube_project();
        let (placement, mesh) = mesh_pick::vertex_frame(&project, cube).expect("mesh");
        let start_origin = mesh_pick::to_world(placement, mesh.positions()[0]);
        let drag = VertexDrag {
            object: cube,
            vertices: vec![0],
            starts: vec![mesh.positions()[0]],
            axis: Axis::X,
            start_origin,
            start_parameter: 0.0,
            offset_world: DVec3::ZERO,
            offset_local: [Length::ZERO; 3],
        };
        let camera = Camera::default();
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let sight = Sight::none();
        let marks = project.snap_marks(sight);
        let mark = marks
            .iter()
            .find(|mark| (mark.point - start_origin).length() > 50.0)
            .expect("the sample scene offers a place away from the cube");
        let pointer = crate::gizmo_math::screen_point(camera, viewport, mark.point).expect("the mark is on screen");
        let target =
            MeshTarget { camera, project: &project, object: cube, vertices: &[0], kinds: model::SnapKinds::ALL, sight };
        let (offset, capture) = snapped_offset(&drag, pointer, 0.0, &target, viewport);
        assert_eq!(capture.map(|(kind, _)| kind), Some(mark.kind), "the capture names the kind that took the pointer");
        assert!(
            (offset - (mark.point - start_origin)).length() < 1.0e-9,
            "the dragged point lands on the candidate itself: {offset:?} against {:?}",
            mark.point
        );

        // With nothing to capture, the axis still rules: the handle's own distance is the offset.
        let quiet = MeshTarget { kinds: model::SnapKinds::none(), ..target };
        let (offset, capture) = snapped_offset(&drag, pointer, 25.0, &quiet, viewport);
        assert_eq!(capture, None, "no kind may capture");
        assert!((offset - Axis::X.direction() * 25.0).length() < 1.0e-9, "the axis offset stands: {offset:?}");
    }

    #[test]
    fn a_drag_previews_and_moves_every_selected_vertex() {
        let (project, cube) = cube_project();
        let (_, mesh) = mesh_pick::vertex_frame(&project, cube).expect("mesh");
        let starts = vec![mesh.positions()[0], mesh.positions()[4]];
        let delta = [millimetres(5), millimetres(-3), Length::ZERO];
        let drag = VertexDrag {
            object: cube,
            vertices: vec![0, 4],
            starts: starts.clone(),
            axis: Axis::X,
            start_origin: DVec3::ZERO,
            start_parameter: 0.0,
            offset_world: DVec3::new(5.0, -3.0, 0.0),
            offset_local: delta,
        };
        let gizmo = VertexGizmo { drag: Some(drag), ..Default::default() };

        let preview = gizmo.preview().expect("a preview");
        assert_eq!(preview.object, cube);
        assert_eq!(preview.vertices, [0, 4]);
        assert_eq!(preview.moved(0, starts[0]), Some(offset_position(starts[0], delta).expect("offset")));
        assert_eq!(preview.moved(1, starts[1]), None, "an unselected vertex does not move");
        assert!(matches!(gizmo.preview_key(), Some(PreviewKey::Vertex { object, .. }) if object == cube));
    }

    #[test]
    fn a_zero_or_overflowing_drag_commits_nothing() {
        let (project, cube) = cube_project();
        let (_, mesh) = mesh_pick::vertex_frame(&project, cube).expect("mesh");
        let drag = |delta| VertexDrag {
            object: cube,
            vertices: vec![0],
            starts: vec![mesh.positions()[0]],
            axis: Axis::X,
            start_origin: DVec3::ZERO,
            start_parameter: 0.0,
            offset_world: DVec3::ZERO,
            offset_local: delta,
        };

        let zero = VertexGizmo { drag: Some(drag([Length::ZERO; 3])), ..Default::default() };
        assert!(zero.preview().is_none());
        assert!(zero.preview_key().is_none());
        assert!(zero.drag.as_ref().and_then(VertexDrag::moved_positions).is_none());

        let huge = drag([Length::from_micrometres(i64::MAX); 3]);
        let mut huge = huge;
        huge.starts = vec![[Length::from_micrometres(i64::MAX); 3]];
        assert!(huge.moved_positions().is_none(), "a position may not wrap around");
        assert!(offset_position([Length::from_micrometres(i64::MAX); 3], [millimetres(1); 3]).is_none());
    }
}
