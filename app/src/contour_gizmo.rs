//! The viewport gizmo for contour edit mode.
//!
//! The gizmo edits the editor's working copy, never the document: dragging a corner rewrites the
//! draft ring or chain, and double-clicking a side splits it there. Painting shows the draft over
//! the solid, with the original shape faintly behind it when the draft differs, so the user sees
//! the change before the single command that applies it.
//!
//! The pointer is mapped onto the node's local XY plane through its world placement, and that plane
//! is the plane the one snap resolver is asked about: a corner of another panel captures the dragged
//! corner by the workspace's mode — its foot on this plane in 2.5D, itself in 3D — and the marker is
//! painted where the working copy really put the corner, so the marker and the drop are one place
//! (U119). An arc corner is projected onto its own circle first, because an edit may slide an arc
//! corner along the arc but never change its centre or radius.
//!
//! A ring names every corner by the segment that starts on it; an open chain has a free end as well,
//! so a handle is addressed with [`PathCorner`] and the last segment's end is reachable.

use eframe::egui::{self, Pos2, Response, Ui};
use geom::{DVec2, DVec3};
use model::{
    Contour, ContourSegment, ContourSegmentEntry, ContourSegmentId, CornerMove, PathCorner, Placement, Project, Sight,
    SnapKind,
};
use scene::Camera;
use ui::{
    ContourDraft, ContourEdit, ContourGrip, ContourTarget, Editor, FieldLabel, FieldParam, FieldRow, FieldSet,
    GripNumber, GripValue, PointState, guides,
};
use units::Length;

use crate::gizmo_math::{
    ACTIVE_COLOR, SNAP_RADIUS_POINTS, WorkPlane, distance_to_segment, length_from_millimetres, paint_snap_marker,
    ray_plane, ray_to, reference_screen, screen_point,
};

/// Radius of an unselected grip handle, in points.
const GRIP_RADIUS: f32 = 4.5;
/// Pointer distance that grabs a grip, in points.
const GRIP_HIT_RADIUS: f32 = 9.0;
/// Pointer distance that picks a side for a split, in points.
const SEGMENT_HIT_RADIUS: f32 = 7.0;
/// Sampling tolerance for the painted ring, in micrometres.
const RING_TOLERANCE_MICROMETRES: f64 = 500.0;
/// Fraction of a side that stays unsplittable next to each corner.
const SPLIT_MARGIN: f64 = 0.02;
/// Maximum number of screen segments used to draw one arc.
const MAX_ARC_STEPS: usize = 64;
/// Colour of the ring the edit started from.
const BASE_COLOR: egui::Color32 = egui::Color32::from_rgba_premultiplied(150, 150, 150, 160);
/// Colour of the working copy.
const DRAFT_COLOR: egui::Color32 = egui::Color32::from_rgb(120, 200, 255);
/// Colour of an untouched grip handle.
const GRIP_COLOR: egui::Color32 = egui::Color32::from_rgb(240, 240, 240);

/// What the workspace's snap set and its mode say to one contour frame (U119).
///
/// The two travel together because the resolver reads both at once and the gizmo's signature is
/// already long: which kinds may capture, and how a candidate above the ring's plane is read.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ContourInput {
    /// Which kinds may capture a dragged corner.
    pub(crate) kinds: model::SnapKinds,
    /// The workspace's mode: a candidate above the plane captures by its foot in 2.5D, and only when
    /// it lies in the plane in 3D.
    pub(crate) mode: model::SnapMode,
}

/// One pointer frame of a grip: where the pointer is, which grip it would move and the view it is
/// seen through.
///
/// The three travel together because the resolver needs all of them and the gizmo's own call would
/// otherwise carry eight arguments (AGENTS §1.5, U119).
#[derive(Clone, Copy, Debug)]
struct Ask {
    /// The camera, its rectangle and the node's world frame.
    view: Screen,
    /// The grip the pointer would move.
    grip: ContourGrip,
    /// The pointer in viewport points.
    pointer: Pos2,
}

/// The view one contour gesture is resolved against: the camera, its rectangle and the node's
/// world frame.
#[derive(Clone, Copy, Debug)]
struct Screen {
    camera: Camera,
    rect: egui::Rect,
    placement: Placement,
}

/// One grip drag that is in progress.
#[derive(Clone, Copy, Debug)]
struct GripDrag {
    target: ContourTarget,
    grip: ContourGrip,
    /// The motion resolved once at the drag start: free, or along one named segment.
    motion: CornerMove,
}

/// The number that follows the cursor while a grip is dragged.
///
/// One plate of the point-field atom (`ui::FieldSet`), anchored at the pointer: the drag
/// keeps the number live, a typed value locks it, and `0` or Escape gives the grip its old value.
#[derive(Clone, Debug)]
struct GripField {
    grip: ContourGrip,
    kind: GripNumber,
    /// The motion segment for an along-contour drag, used to move the corner by a typed length.
    motion: Option<CornerMove>,
    param: FieldParam,
    fields: FieldSet,
    /// The value the drag started from, which Escape restores.
    start: Option<GripValue>,
}

/// What the numeric field asked the shell to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NumericAction {
    /// Apply the parsed number to the grip.
    Apply(ContourGrip, GripValue),
    /// Put the value the drag started from back.
    Restore(ContourGrip, GripValue),
    /// Apply a typed length to an along-contour corner, moving it along the segment.
    ApplyAlong(ContourGrip, GripValue, CornerMove),
}

/// What the gizmo asked the shell to do after one frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GizmoAction {
    /// The frame changed nothing.
    Idle,
    /// The working copy changed, so the viewport repaints.
    Changed,
    /// A number typed at a grip, for the shell to apply to the working copy.
    Numeric(NumericAction),
    /// The latched move is dropped: write the working copy as one command.
    Drop,
    /// The latched move is abandoned: the corner goes back to where it was latched.
    Abandon,
}

/// The latched move of one corner: the grammar `click, click, move, click` of a native editor.
///
/// The old position is the only fixed reference of a move, so it is the anchor of the two guides
/// and of the two numbers: the chord to the point under the pointer and the reference whose arc
/// radius is the distance moved (ADR 0127, 0129). The plates are the point-field atom, so a typed
/// number locks its parameter, a typed `0` gives it back to the pointer, and Escape drops the locks.
#[derive(Clone, Debug)]
struct CornerLatch {
    /// The corner that moves; a latch is only ever opened on a corner.
    grip: ContourGrip,
    /// Where the corner sat when the move was latched: the guide's anchor and what Escape restores.
    start: [Length; 2],
    /// The motion the move takes, resolved once at the latch from the along toggle.
    motion: CornerMove,
    /// The plates: the distance from `start` and the angle from `+X`.
    fields: FieldSet,
    /// The pointer in the node's local plane, for the plates and the guides.
    pointer: DVec2,
    /// The measurement of the last frame: what the plates name and the guides draw.
    state: PointState,
    /// Whether the working copy really left `start`, so a drop without motion writes nothing.
    moved: bool,
}

/// Pointer state of the contour gizmo; the working copy itself belongs to the editor.
#[derive(Debug, Default)]
pub(crate) struct ContourGizmo {
    drag: Option<GripDrag>,
    hover: Option<ContourGrip>,
    hover_segment: Option<ContourSegmentId>,
    /// The corner a click selected; a click on it again latches the move.
    selected: Option<ContourGrip>,
    /// The latched move, which follows the pointer until a click drops it.
    latch: Option<CornerLatch>,
    numeric: Option<GripField>,
    /// Whether a corner drag runs along the contour instead of freely.
    along: bool,
    /// The capture the resolver answered with while a corner was dragged or latched: the grip and
    /// the kind, so the marker is painted at the point the working copy holds (U119).
    snap: Option<(ContourGrip, SnapKind)>,
}

impl ContourGizmo {
    /// Drops every drag, hover, selection and numeric state, for example when the mode ends.
    pub(crate) fn reset(&mut self) {
        self.drag = None;
        self.hover = None;
        self.hover_segment = None;
        self.selected = None;
        self.latch = None;
        self.numeric = None;
        self.snap = None;
    }

    /// Whether a grip drag or its numeric field owns the keyboard right now.
    pub(crate) fn is_dragging(&self) -> bool {
        self.drag.is_some() || self.numeric.is_some()
    }

    /// The capture the pointer answered with while a corner was dragged or latched: the grip and the
    /// kind that took it, or `None` when the pointer's own point stands (U119).
    pub(crate) fn snap(&self) -> Option<(ContourGrip, SnapKind)> {
        self.snap
    }

    /// Whether a latched move is waiting for the click that drops it.
    pub(crate) fn is_latched(&self) -> bool {
        self.latch.is_some()
    }

    /// Whether a plate owns the keyboard, so the mode's own key reader stands aside.
    pub(crate) fn holds_keys(&self) -> bool {
        self.latch.as_ref().is_some_and(|latch| latch.fields.is_holding())
            || self.numeric.as_ref().is_some_and(|field| field.fields.is_holding())
    }

    /// Whether `point` lies on one of the plates, which are chrome and never the scene.
    pub(crate) fn field_contains(&self, point: Pos2) -> bool {
        self.latch.as_ref().is_some_and(|latch| latch.fields.contains(point))
            || self.numeric.as_ref().is_some_and(|field| field.fields.contains(point))
    }

    /// The move a drop writes: the corner, the point its plates name and the motion it takes.
    ///
    /// The plates may have locked a typed number on the frame the drop arrives, so the point is read
    /// from them here rather than from the working copy the last frame left behind.
    pub(crate) fn latched_move(&self) -> Option<(ContourGrip, [Length; 2], CornerMove)> {
        let latch = self.latch.as_ref()?;
        let state = PointState::moved(plane_point(latch.start), latch.pointer, Some(&latch.fields))?;
        Some((latch.grip, state.point, latch.motion))
    }

    /// Abandons the latched move: the corner is back where the latch found it.
    ///
    /// Returns whether there was a latch to abandon, so the caller's Escape ends the mode only
    /// when the move was not the first thing it had to give up.
    pub(crate) fn abandon_latch(&mut self, editor: &mut Editor) -> bool {
        self.snap = None;
        let Some(latch) = self.latch.take() else {
            return false;
        };
        if let ContourGrip::Corner(corner) = latch.grip {
            let _ = editor.contour_edit_move_corner_at(corner, latch.start, CornerMove::Free);
            editor.contour_edit_clear_collapse();
        }
        true
    }

    /// Whether a corner drag follows the contour instead of moving freely.
    pub(crate) fn along(&self) -> bool {
        self.along
    }

    /// Switches between a free corner drag and one along the contour.
    pub(crate) fn toggle_along(&mut self) {
        self.along = !self.along;
    }

    /// Applies the pointer to the working copy and reports what the shell must do with it.
    ///
    /// The gizmo owns the left button while the mode is live: a drag moves the grabbed grip, a
    /// double-click splits the side under the pointer, and a click selects a corner or latches its
    /// move (ADR 0129). A rejected edit leaves the draft at its last valid shape, so a grip simply
    /// stops when the ring would become invalid.
    pub(crate) fn update(
        &mut self,
        response: &Response,
        rect: egui::Rect,
        camera: Camera,
        project: &Project,
        editor: &mut Editor,
        input: ContourInput,
    ) -> Result<GizmoAction, String> {
        let Some(edit) = editor.contour_edit() else {
            self.reset();
            return Ok(GizmoAction::Idle);
        };
        let target = edit.target();
        let Some(placement) = project.world_placement(target.id()) else {
            self.reset();
            return Ok(GizmoAction::Idle);
        };
        let draft = edit.draft().clone();
        let pointer = self.pointer(response);
        let segment = pointer.and_then(|pointer| segment_at(camera, rect, placement, &draft, pointer));
        let grip = pointer.and_then(|pointer| grip_at(camera, rect, placement, &draft, pointer, segment));
        self.hover = grip;
        self.hover_segment = segment;

        // A latched move owns the pointer until a click drops it; nothing else may grab a grip or
        // split a side meanwhile, or the drop would start an edit of its own.
        if let Some(latched) = self.latch.as_ref().map(|latch| latch.grip) {
            let view = Screen { camera, rect, placement };
            let local =
                pointer.and_then(|pointer| self.snapped(editor, Ask { view, grip: latched, pointer }, project, input));
            if local.is_none() {
                self.snap = None;
            }
            let changed = self.follow(view, local, editor);
            // A press on one of the move's own plates is the plate's: it focuses or scrubs a number
            // and never drops the corner under it.
            if response.clicked_by(egui::PointerButton::Primary) && !pointer.is_some_and(|at| self.field_contains(at)) {
                return Ok(self.end_latch());
            }
            return Ok(if changed { GizmoAction::Changed } else { GizmoAction::Idle });
        }

        let mut changed = false;
        // A press that became a drag is the one-gesture move, so the release that ends it is never
        // also a click on the corner the drag just put somewhere else.
        let dragging = self.drag.is_some() || response.drag_started_by(egui::PointerButton::Primary);
        if response.drag_started_by(egui::PointerButton::Primary) {
            // Resolve the grabbed grip from the press origin, not the current pointer.
            // The pointer may have moved a few points before egui fires the drag threshold,
            // and the user clearly intended to grab whatever was under the initial press.
            let press_origin = response.ctx.input(|i| i.pointer.press_origin());
            let press_grip = press_origin.and_then(|origin| {
                let seg = segment_at(camera, rect, placement, &draft, origin);
                grip_at(camera, rect, placement, &draft, origin, seg)
            });
            self.drag = press_grip.map(|grip| GripDrag {
                target,
                grip,
                motion: self.motion_for(camera, rect, placement, &draft, grip, press_origin),
            });
            // A touched corner is the selected one, so the click that follows a drag latches its
            // move instead of selecting it a second time.
            self.selected = press_grip.filter(|grip| matches!(grip, ContourGrip::Corner(_)));
            if let Some(drag) = self.drag
                && let Some(origin) = press_origin
            {
                self.numeric = grip_field(&draft, drag.grip, origin, Some(drag.motion));
            }
        }
        if let Some(drag) = self.drag {
            if drag.target != target {
                self.drag = None;
                self.numeric = None;
            } else {
                if response.dragged_by(egui::PointerButton::Primary)
                    && let Some(pointer) = response.interact_pointer_pos()
                    && let Some(local) = self.snapped(
                        editor,
                        Ask { view: Screen { camera, rect, placement }, grip: drag.grip, pointer },
                        project,
                        input,
                    )
                    && let Some(local) = self.dragged_point(&draft, drag, local)
                    && let (Some(x), Some(y)) = (length_from_millimetres(local.x), length_from_millimetres(local.y))
                    && editor.contour_edit_drag_grip(drag.grip, [x, y], drag.motion).is_ok()
                {
                    changed = true;
                    // An along-contour drag that reaches the far end records a collapse, which
                    // Apply turns into a segment removal. Dragging back clears it.
                    if let (ContourGrip::Corner(corner), CornerMove::Along(segment)) = (drag.grip, drag.motion) {
                        let collapsed = editor
                            .contour_edit()
                            .and_then(|edit| {
                                let start = edit.base().grip_position(ContourGrip::Corner(corner))?;
                                let far_end = edit.base().motion_far_end(segment, start)?;
                                edit.draft().along_contour_length(segment, corner, far_end)
                            })
                            .is_some_and(|length| length.micrometres() <= 2);
                        if collapsed {
                            editor.contour_edit_record_collapse(segment);
                        } else {
                            editor.contour_edit_clear_collapse();
                        }
                    }
                }
                if let Some(field) = &mut self.numeric
                    && let Some(pointer) = pointer
                {
                    let param = field.param;
                    field.fields.set_anchor(param, pointer);
                    let value = if let Some(CornerMove::Along(segment)) = field.motion
                        && let ContourGrip::Corner(corner) = field.grip
                    {
                        editor.contour_edit().and_then(|edit| {
                            let start = edit.base().grip_position(ContourGrip::Corner(corner))?;
                            let far_end = edit.base().motion_far_end(segment, start)?;
                            edit.draft().along_contour_length(segment, corner, far_end).map(GripValue::Length)
                        })
                    } else {
                        editor.contour_edit().and_then(|edit| edit.grip_value(field.grip))
                    };
                    if let Some(value) = value {
                        field.fields.set_live(param, number_of(value));
                    }
                }
                if response.drag_stopped_by(egui::PointerButton::Primary) {
                    self.drag = None;
                    self.snap = None;
                }
            }
        }

        // An arc curve under the pointer also offers a radius grip, but a double-click there still
        // splits the arc: only a real handle blocks the split.
        let synthetic = matches!(grip, Some(ContourGrip::ArcRadius(..)));
        if self.drag.is_none()
            && self.numeric.is_none()
            && (grip.is_none() || synthetic)
            && response.double_clicked_by(egui::PointerButton::Primary)
            && let Some(segment) = segment
            && let Some(pointer) = response.interact_pointer_pos()
            && let Some(local) = local_point(pointer, rect, camera, placement)
            && let Some(curve) = draft.segment(segment)
            && let Some(point) = split_point(curve, local)
        {
            // A rejected split is a normal near-corner double-click; the side simply stays whole.
            if editor.contour_edit_split(segment, point).is_ok() {
                changed = true;
            }
        }

        // A click on an unselected corner selects it; a click on the selected corner latches its
        // move, and the next click drops it with one command and one undo entry (ADR 0129).
        if !dragging
            && self.drag.is_none()
            && response.clicked_by(egui::PointerButton::Primary)
            && let Some(pointer) = pointer
        {
            // The plate of a finished drag belongs to the drag, not to the click after it.
            self.numeric = None;
            match grip {
                Some(grip @ ContourGrip::Corner(_)) if self.selected == Some(grip) => {
                    let motion = self.motion_for(camera, rect, placement, &draft, grip, Some(pointer));
                    let view = Screen { camera, rect, placement };
                    let local = self.snapped(editor, Ask { view, grip, pointer }, project, input);
                    self.latch = local.and_then(|local| self.latch_move(&draft, grip, pointer, local, view, motion));
                }
                Some(grip @ ContourGrip::Corner(_)) => self.selected = Some(grip),
                _ => self.selected = None,
            }
        }
        Ok(if changed { GizmoAction::Changed } else { GizmoAction::Idle })
    }

    /// The point the pointer hands a grip, read through the one snap resolver, and the capture it
    /// answered with (U119).
    ///
    /// The ring's own plane is the plane the tool draws on, so the workspace's mode decides whether a
    /// candidate above that plane captures by its foot (2.5D) or only when it really lies in the
    /// plane (3D). The draft's other corners are the figure's own points, offered exactly as a
    /// running sketch offers the points it already holds; the corner being moved is left out, or it
    /// would capture itself. There is no previous point in a grip drag, so the constructions that
    /// need one — the perpendicular and the tangent — do not answer here.
    fn snapped(&mut self, editor: &mut Editor, ask: Ask, project: &Project, input: ContourInput) -> Option<DVec2> {
        let sight = editor.flags().sight();
        let draft = editor.contour_edit()?.draft();
        let (local, kind) = self.resolved(draft, ask, project, sight, input)?;
        self.snap = kind.map(|kind| (ask.grip, kind));
        Some(local)
    }

    /// The resolver's answer for one pointer position, in the node's own plane coordinates.
    ///
    /// The grid is the one kind left out: it rounds wherever the pointer is, which is right for
    /// placing a new point on the drawing plane and wrong for moving a corner the maker already
    /// measured — a 60.5 mm side would land on 60 the moment it was dragged. Every other kind, the
    /// coordinate axes included, says "this place is a real place", and those are the captures an
    /// edit wants.
    fn resolved(
        &self,
        draft: &ContourDraft,
        ask: Ask,
        project: &Project,
        sight: Sight<'_>,
        input: ContourInput,
    ) -> Option<(DVec2, Option<SnapKind>)> {
        let Ask { view, grip, pointer } = ask;
        let plane = WorkPlane::from_placement(view.placement)?;
        let ray = ray_to(pointer, view.rect, view.camera);
        let hit = plane.ray_point(ray)?;
        let depth = (hit - view.camera.eye()).dot(view.camera.forward());
        if !depth.is_finite() {
            return None;
        }
        let reach = view.camera.world_per_point(depth, f64::from(view.rect.height())) * f64::from(SNAP_RADIUS_POINTS);
        let tolerance = length_from_millimetres(reach)?;
        let ends = draft
            .grips()
            .into_iter()
            .filter(|other| *other != grip)
            .filter_map(|other| draft.grip_position(other))
            .map(|position| plane.world(plane_point(position)))
            .collect::<Vec<_>>();
        let request = model::SnapRequest {
            ray_origin: ray.origin(),
            ray_direction: ray.direction(),
            tolerance,
            tolerance_depth: depth,
            kinds: model::SnapKinds { grid: false, ..input.kinds },
            plane: plane.snap_plane(),
            target: input.mode.target(),
            anchor: None,
            ends: &ends,
            axis_lock: false,
            face: None,
        };
        let answer = project.snap(request, sight)?;
        Some((plane.local(answer.point), answer.kind))
    }

    /// The pointer the gizmo reads: the scene's hover, the drag's own position, and — while a move
    /// is latched — the raw pointer, because the latch's plates sit beside the point they measure
    /// and would otherwise freeze the move the moment the hand crossed one.
    fn pointer(&self, response: &Response) -> Option<Pos2> {
        response.hover_pos().or_else(|| response.interact_pointer_pos()).or_else(|| {
            self.latch.as_ref()?;
            response.ctx.input(|input| input.pointer.hover_pos())
        })
    }

    /// Opens the latched move of `grip`: the plates of its delta and the anchor it started from.
    fn latch_move(
        &self,
        draft: &ContourDraft,
        grip: ContourGrip,
        pointer: Pos2,
        local: DVec2,
        view: Screen,
        motion: CornerMove,
    ) -> Option<CornerLatch> {
        let start = draft.grip_position(grip)?;
        let anchor = plane_point(start);
        let live = PointState::moved(anchor, local, None)?;
        let rows = live
            .rows
            .iter()
            .map(|row| {
                let at = local_screen(view, row.anchor).unwrap_or(pointer);
                FieldRow::for_param(row.param, row.value, at)
            })
            .collect();
        let fields = FieldSet::new(rows)?;
        let state = PointState::moved(anchor, local, Some(&fields))?;
        Some(CornerLatch { grip, start, motion, fields, pointer: local, state, moved: false })
    }

    /// Moves the latched corner to what the pointer and the plates place, and keeps them live.
    ///
    /// A refused placement is the mode's usual silent refusal: the working copy stays at its last
    /// valid shape, so a corner dragged into a self-crossing ring simply stops.
    fn follow(&mut self, view: Screen, local: Option<DVec2>, editor: &mut Editor) -> bool {
        let Some(latch) = self.latch.as_mut() else {
            return false;
        };
        let Some(local) = local else {
            return false;
        };
        let anchor = plane_point(latch.start);
        let Some(live) = PointState::moved(anchor, local, None) else {
            return false;
        };
        for row in &live.rows {
            latch.fields.set_live(row.param, row.value);
            if let Some(at) = local_screen(view, row.anchor) {
                latch.fields.set_anchor(row.param, at);
            }
        }
        latch.pointer = local;
        let Some(state) = PointState::moved(anchor, local, Some(&latch.fields)) else {
            return false;
        };
        let (grip, motion, start) = (latch.grip, latch.motion, latch.start);
        latch.state = state.clone();
        let before = editor.contour_edit().and_then(|edit| edit.draft().grip_position(grip));
        let placed = editor.contour_edit_drag_grip(grip, state.point, motion).is_ok();
        let after = editor.contour_edit().and_then(|edit| edit.draft().grip_position(grip));
        latch.moved = after.is_some_and(|position| !near(position, start));
        placed && after != before
    }

    /// Ends the latched move: a corner that really moved is written, one that did not is dropped.
    fn end_latch(&mut self) -> GizmoAction {
        self.snap = None;
        let Some(latch) = self.latch.take() else {
            return GizmoAction::Idle;
        };
        self.selected = Some(latch.grip);
        if latch.moved { GizmoAction::Drop } else { GizmoAction::Idle }
    }

    /// The point a grip really receives, after the arc projection an along-contour corner needs.
    ///
    /// A free corner takes the pointer point directly and the arc endpoint rebuilds the arc through
    /// the other end and its current midpoint. An along-contour corner projects onto the arc's
    /// circle so the centre and radius stay fixed.
    fn dragged_point(&self, draft: &ContourDraft, drag: GripDrag, local: DVec2) -> Option<DVec2> {
        let ContourGrip::Corner(corner) = drag.grip else {
            return Some(local);
        };
        if matches!(drag.motion, CornerMove::Along(_)) { project_arc_corner(draft, corner, local) } else { Some(local) }
    }

    /// The motion a drag takes, resolved once from the toggle and the junction under the pointer.
    fn motion_for(
        &self,
        camera: Camera,
        rect: egui::Rect,
        placement: Placement,
        draft: &ContourDraft,
        grip: ContourGrip,
        pointer: Option<Pos2>,
    ) -> CornerMove {
        if !self.along {
            return CornerMove::Free;
        }
        let (ContourGrip::Corner(corner), Some(pointer)) = (grip, pointer) else {
            return CornerMove::Free;
        };
        along_segment(camera, rect, placement, draft, corner, pointer)
            .map(CornerMove::Along)
            .unwrap_or(CornerMove::Free)
    }

    /// Paints the base shape, the working copy, the grips and the numeric field.
    ///
    /// The numeric field is an editable widget, so the frame returns the action Enter or Escape
    /// asked for and the shell applies it after the paint borrow ends.
    pub(crate) fn paint(
        &mut self,
        ui: &Ui,
        rect: egui::Rect,
        camera: Camera,
        project: &Project,
        edit: &ContourEdit,
    ) -> Option<GizmoAction> {
        let placement = project.world_placement(edit.target().id())?;
        let painter = ui.painter().with_clip_rect(rect);
        if edit.is_dirty() {
            paint_draft(&painter, camera, rect, placement, edit.base(), egui::Stroke::new(1.0, BASE_COLOR));
        }
        paint_draft(&painter, camera, rect, placement, edit.draft(), egui::Stroke::new(2.0, DRAFT_COLOR));
        let grips = edit.draft().grips();
        for grip in &grips {
            let Some(local) = edit.draft().grip_position(*grip) else {
                continue;
            };
            let world = placement.transform_point(local_world(local));
            let Some(position) = screen_point(camera, rect, world) else {
                continue;
            };
            let selected = self.marked(*grip);
            let hovered = self.hover == Some(*grip);
            paint_grip(&painter, position, *grip, selected, hovered);
        }
        // The radius grip on a hovered arc curve is not part of the stable list, so paint it here.
        // The radius grip on a hovered arc curve or a circle rim is not part of the stable list, so paint it here.
        if let Some(grip @ (ContourGrip::ArcRadius(..) | ContourGrip::CircleRim(..))) = self.hover
            && !grips.contains(&grip)
            && let Some(local) = edit.draft().grip_position(grip)
        {
            let world = placement.transform_point(local_world(local));
            if let Some(position) = screen_point(camera, rect, world) {
                paint_grip(&painter, position, grip, false, true);
            }
        }
        // The marker stands where the working copy put the corner, so it names the place the drop
        // will write and never the candidate it came from (U119).
        if let Some((grip, kind)) = self.snap()
            && let Some(local) = edit.draft().grip_position(grip)
            && let Some(position) = local_screen(Screen { camera, rect, placement }, plane_point(local))
        {
            paint_snap_marker(&painter, position, Some(kind), false, ACTIVE_COLOR);
        }
        if let Some(action) = self.paint_latch(ui, Screen { camera, rect, placement }) {
            return Some(action);
        }
        self.paint_numeric(ui, rect).map(GizmoAction::Numeric)
    }

    /// Whether `grip` is the one a drag, a selection or a latched move is holding.
    fn marked(&self, grip: ContourGrip) -> bool {
        self.drag.is_some_and(|drag| drag.grip == grip)
            || self.latch.as_ref().is_some_and(|latch| latch.grip == grip)
            || self.selected == Some(grip)
    }

    /// Draws the guides of a latched move, then its two plates, and reads what they asked for.
    ///
    /// The construction line runs from the old position to the point the plates name and the
    /// reference arc has the distance moved as its radius, so the number and the picture are the
    /// same measurement (ADR 0127). The line, the ghost and the arc use the anchors
    /// [`PointState::moved`] gives, so the motion the working copy takes and the one drawn can only
    /// differ when the corner's own segment refuses the point.
    fn paint_latch(&mut self, ui: &Ui, view: Screen) -> Option<GizmoAction> {
        let latch = self.latch.as_mut()?;
        let painter = ui.painter().with_clip_rect(view.rect);
        paint_latch_guides(ui, &painter, view, latch);
        let action = latch.fields.show(ui.ctx(), view.rect);
        if action.cancel {
            // Escape with nothing typed: the latch is the first thing the mode gives up.
            return Some(GizmoAction::Abandon);
        }
        if action.commit {
            // Enter places the point: the drop writes the move it names.
            return Some(GizmoAction::Drop);
        }
        // A typed number, its zero and the Escape that drops every lock are the atom's own: the
        // plates keep following the pointer with the typed numbers in place (ADR 0126).
        if action.edit.is_some() || action.released {
            ui.ctx().request_repaint();
        }
        None
    }

    /// Draws the editable number beside the cursor and applies what it asked for.
    ///
    /// The plate is the point-field atom, so Tab, Enter, Escape and a typed zero behave exactly as
    /// they do at a drawing point: the value is live until typed, a typed number locks it, and a
    /// release hands the grip back its old value.
    fn paint_numeric(&mut self, ui: &Ui, rect: egui::Rect) -> Option<NumericAction> {
        let field = self.numeric.as_mut()?;
        let action = field.fields.show(ui.ctx(), rect);
        let grip = field.grip;
        let kind = field.kind;
        let motion = field.motion;
        let start = field.start;
        let live = field.fields.value(field.param);
        let mut asked = None;
        let mut close = false;
        if action.edit.is_some() || action.commit {
            if let Some(number) = action.edit.map(|edit| edit.value).or(live)
                && let Ok(value) = kind.parse(&number.to_string(), ui::language(ui.ctx()))
            {
                asked = Some(match motion {
                    Some(motion) => NumericAction::ApplyAlong(grip, value, motion),
                    None => NumericAction::Apply(grip, value),
                });
            }
            close = true;
        }
        if action.released || action.cancel {
            if let Some(start) = start {
                asked = Some(NumericAction::Restore(grip, start));
            }
            close = true;
        }
        if close {
            self.drag = None;
            self.numeric = None;
        }
        asked
    }
}

/// The number a grip opens, when it takes a number at all.
fn grip_field(draft: &ContourDraft, grip: ContourGrip, pointer: Pos2, motion: Option<CornerMove>) -> Option<GripField> {
    let mut along_range = None;
    let (kind, value, label, motion) = if let Some(motion @ CornerMove::Along(segment)) = motion
        && let ContourGrip::Corner(corner) = grip
    {
        // Along-contour mode: show the length from the far end of the motion segment. At the
        // moment the drag starts the corner is still at its base position, so the draft answers the
        // far end the whole drag keeps.
        let start = draft.grip_position(grip)?;
        let far_end = draft.motion_far_end(segment, start)?;
        let length = draft.along_contour_length(segment, corner, far_end)?;
        // The number's two ends are the segment's own: zero is the collapse, and the top is what the
        // segment still holds once it has grown, so the field cannot offer a number the command refuses.
        let (min, max) = draft.along_length_range(segment)?;
        along_range = Some(min.millimetres_f64()..=max.millimetres_f64());
        (GripNumber::Length, GripValue::Length(length), "L", Some(motion))
    } else {
        let kind = draft.grip_number(grip)?;
        let value = draft.grip_value(grip)?;
        let label = draft.grip_label(grip)?;
        (kind, value, label, None)
    };
    // No unit: the grip's symbol (`L`, `W`, `H`, `R`, `∠`) already says what the number is, and a
    // field that floats over the figure stays as small as the number it holds.
    let (param, unit, range) = match value {
        GripValue::Length(_) => (FieldParam::Length, "", along_range.unwrap_or(0.0..=1_000_000.0)),
        GripValue::Angle(_) => (FieldParam::Angle, "", -360.0..=360.0),
    };
    let row = FieldRow::new(param, Some(FieldLabel::Symbol(label)), number_of(value), unit, range, 1, pointer);
    let fields = FieldSet::new(vec![row])?;
    Some(GripField { grip, kind, motion, param, fields, start: Some(value) })
}

/// One grip value as the number the field shows: millimetres or degrees.
fn number_of(value: GripValue) -> f64 {
    match value {
        GripValue::Length(length) => length.millimetres_f64(),
        GripValue::Angle(angle) => angle.degrees_f64(),
    }
}

/// Paints one grip: a square for a centre, a circle for a point on the figure.
fn paint_grip(painter: &egui::Painter, position: Pos2, grip: ContourGrip, selected: bool, hovered: bool) {
    let radius = if selected || hovered { GRIP_RADIUS + 1.5 } else { GRIP_RADIUS };
    let color = if selected { ACTIVE_COLOR } else { GRIP_COLOR };
    let outline = egui::Stroke::new(1.0, egui::Color32::from_black_alpha(160));
    if is_center(grip) {
        let box_rect = egui::Rect::from_center_size(position, egui::vec2(radius, radius) * 2.0);
        painter.rect_filled(box_rect, 1.0, color);
        painter.rect_stroke(box_rect, 1.0, outline, egui::StrokeKind::Inside);
    } else {
        painter.circle_filled(position, radius, color);
        painter.circle_stroke(position, radius, outline);
    }
}

/// Whether a grip is a figure or arc centre, which paints as a square.
fn is_center(grip: ContourGrip) -> bool {
    matches!(
        grip,
        ContourGrip::CircleCenter
            | ContourGrip::ArcCenter(_)
            | ContourGrip::RectangleCenter
            | ContourGrip::PolygonCenter
    )
}

/// Paints one draft as a screen-space polyline, closed only when it is a ring.
fn paint_draft(
    painter: &egui::Painter,
    camera: Camera,
    rect: egui::Rect,
    placement: Placement,
    draft: &ContourDraft,
    stroke: egui::Stroke,
) {
    if let Some(shape) = draft_shape(camera, rect, placement, draft, stroke) {
        painter.add(shape);
    }
}

/// The screen-space polyline a draft paints, closed only when the draft is a ring.
///
/// The closed flag is what separates the two cases: a ring and a closed chain join their last
/// point back to the first, an open chain must not, or its free end would be drawn as a side.
fn draft_shape(
    camera: Camera,
    rect: egui::Rect,
    placement: Placement,
    draft: &ContourDraft,
    stroke: egui::Stroke,
) -> Option<egui::Shape> {
    match draft {
        ContourDraft::Ring(contour) => ring_shape(camera, rect, placement, contour, stroke),
        ContourDraft::Chain(path) => chain_shape(camera, rect, placement, path.entries(), path.is_closed(), stroke),
    }
}

/// The screen-space polyline of one open or closed chain.
fn chain_shape(
    camera: Camera,
    rect: egui::Rect,
    placement: Placement,
    entries: &[ContourSegmentEntry],
    closed: bool,
    stroke: egui::Stroke,
) -> Option<egui::Shape> {
    let mut screen = Vec::new();
    for entry in entries {
        for position in segment_screen(camera, rect, placement, entry.segment()) {
            if screen.last() != Some(&position) {
                screen.push(position);
            }
        }
    }
    (screen.len() >= 2)
        .then(|| if closed { egui::Shape::closed_line(screen, stroke) } else { egui::Shape::line(screen, stroke) })
}

/// The screen-space polyline of one ring, always closed.
fn ring_shape(
    camera: Camera,
    rect: egui::Rect,
    placement: Placement,
    contour: &Contour,
    stroke: egui::Stroke,
) -> Option<egui::Shape> {
    let points = contour.sampled_points(RING_TOLERANCE_MICROMETRES).ok()?;
    let mut screen = Vec::with_capacity(points.len());
    for point in points {
        let Some(position) = screen_point(camera, rect, placement.transform_point(local_world(point))) else {
            continue;
        };
        screen.push(position);
    }
    (screen.len() >= 3).then(|| egui::Shape::closed_line(screen, stroke))
}

/// The corners a draft draws a handle on, for the tests that check the corner list.
///
/// A ring has one handle per segment start. A chain has the same starts plus the free end when it
/// is open; a closed chain's last end is the first start and would put two handles on one point.
#[cfg(test)]
fn corner_handles(draft: &ContourDraft) -> Vec<PathCorner> {
    let mut corners: Vec<PathCorner> = draft.entries().iter().map(|entry| PathCorner::Start(entry.id())).collect();
    let open = draft.as_chain().is_some_and(|path| !path.is_closed());
    if open && let Some(last) = draft.entries().last() {
        corners.push(PathCorner::End(last.id()));
    }
    corners
}

/// The point a corner sits on, in the node's local plane.
fn corner_position(draft: &ContourDraft, corner: PathCorner) -> Option<[Length; 2]> {
    let segment = draft.segment(corner.segment())?;
    if corner.is_end() { segment.end().ok() } else { segment.start().ok() }
}

/// The grip under the pointer, if any.
///
/// A real handle wins over the radius grip a hovered arc curve offers; the synthetic grip carries
/// the point on the curve so a drag that starts there measures the arc's own radius.
fn grip_at(
    camera: Camera,
    rect: egui::Rect,
    placement: Placement,
    draft: &ContourDraft,
    pointer: Pos2,
    hover_segment: Option<ContourSegmentId>,
) -> Option<ContourGrip> {
    let mut best: Option<(f32, ContourGrip)> = None;
    for grip in draft.grips() {
        let Some(local) = draft.grip_position(grip) else {
            continue;
        };
        let Some(position) = screen_point(camera, rect, placement.transform_point(local_world(local))) else {
            continue;
        };
        let distance = position.distance(pointer);
        if distance <= GRIP_HIT_RADIUS && best.is_none_or(|(closest, _)| distance < closest) {
            best = Some((distance, grip));
        }
    }
    if let Some((_, grip)) = best {
        return Some(grip);
    }
    // A ring has no arc grips yet, so it must not offer a radius handle it cannot drag.
    draft.as_chain()?;
    let segment = hover_segment?;
    let curve = draft.segment(segment)?;
    if !curve.is_arc() {
        return None;
    }
    // A circle-tagged path exposes a rim grip that scales the whole circle.
    if draft.as_chain().is_some_and(|path| path.shape() == Some(model::PathShape::Circle)) {
        let local = local_point(pointer, rect, camera, placement)?;
        let point = arc_nearest(curve, local)?;
        return Some(ContourGrip::CircleRim(point));
    }
    let local = local_point(pointer, rect, camera, placement)?;
    let point = arc_nearest(curve, local)?;
    Some(ContourGrip::ArcRadius(segment, point))
}

/// The point of an arc that the pointer is closest to, when it lies inside the sweep.
fn arc_nearest(segment: ContourSegment, local: DVec2) -> Option<[Length; 2]> {
    let ContourSegment::Arc { center, radius, start_angle, sweep } = segment else {
        return None;
    };
    let center = DVec2::new(center[0].millimetres_f64(), center[1].millimetres_f64());
    let offset = local - center;
    if offset.length() <= f64::EPSILON {
        return None;
    }
    let point = center + offset.normalize() * radius.millimetres_f64();
    let angle = (point - center).y.atan2((point - center).x);
    let start = start_angle.radians_f64();
    let sweep = sweep.radians_f64();
    let turn = std::f64::consts::TAU;
    let mut travel = angle - start;
    travel = if sweep >= 0.0 { travel.rem_euclid(turn) } else { -((-travel).rem_euclid(turn)) };
    let inside = if sweep >= 0.0 { travel <= sweep } else { travel >= sweep };
    inside.then_some([length_from_millimetres(point.x)?, length_from_millimetres(point.y)?])
}

/// Whether two document points name the same joint, within the one-micrometre joint tolerance.
fn near(first: [Length; 2], second: [Length; 2]) -> bool {
    let dx = i128::from(first[0].micrometres()) - i128::from(second[0].micrometres());
    let dy = i128::from(first[1].micrometres()) - i128::from(second[1].micrometres());
    dx.abs() <= 1 && dy.abs() <= 1
}

/// The segment a corner should follow when the drag is constrained to the contour.
///
/// The corner joins up to two segments; the one whose projected curve lies nearest the pointer is
/// the one the user grabbed. The choice is made once, when the drag starts, so a drag that moves
/// past the other segment does not switch half way.
fn along_segment(
    camera: Camera,
    rect: egui::Rect,
    placement: Placement,
    draft: &ContourDraft,
    corner: PathCorner,
    pointer: Pos2,
) -> Option<ContourSegmentId> {
    let point = corner_position(draft, corner)?;
    let mut best: Option<(f32, ContourSegmentId)> = None;
    for entry in draft.entries() {
        let segment = entry.segment();
        let (Ok(start), Ok(end)) = (segment.start(), segment.end()) else {
            continue;
        };
        if !near(start, point) && !near(end, point) {
            continue;
        }
        let points = segment_screen(camera, rect, placement, segment);
        let mut distance = f32::INFINITY;
        for pair in points.windows(2) {
            if let Some(found) = distance_to_segment(pointer, pair[0], pair[1]) {
                distance = distance.min(found);
            }
        }
        if distance.is_finite() && best.is_none_or(|(closest, _)| distance < closest) {
            best = Some((distance, entry.id()));
        }
    }
    best.map(|(_, segment)| segment)
}

/// The segment whose projected curve is closest to the pointer, if within the hit radius.
fn segment_at(
    camera: Camera,
    rect: egui::Rect,
    placement: Placement,
    draft: &ContourDraft,
    pointer: Pos2,
) -> Option<ContourSegmentId> {
    let mut best: Option<(f32, ContourSegmentId)> = None;
    for entry in draft.entries() {
        let points = segment_screen(camera, rect, placement, entry.segment());
        for pair in points.windows(2) {
            let Some(distance) = distance_to_segment(pointer, pair[0], pair[1]) else {
                continue;
            };
            if distance <= SEGMENT_HIT_RADIUS && best.is_none_or(|(closest, _)| distance < closest) {
                best = Some((distance, entry.id()));
            }
        }
    }
    best.map(|(_, segment)| segment)
}

/// A segment sampled into screen points; a line needs two, an arc enough to look round.
fn segment_screen(camera: Camera, rect: egui::Rect, placement: Placement, segment: ContourSegment) -> Vec<Pos2> {
    let steps = match segment {
        ContourSegment::Line { .. } => 1,
        ContourSegment::Arc { sweep, .. } => {
            let degrees = (sweep.millidegrees().unsigned_abs() as f64) / 1_000.0;
            ((degrees / 10.0).ceil() as usize).clamp(8, MAX_ARC_STEPS)
        }
    };
    let Ok(geometry) = segment.to_geom() else {
        return Vec::new();
    };
    let mut points = Vec::with_capacity(steps + 1);
    for step in 0..=steps {
        let fraction = step as f64 / steps as f64;
        let Some(local) = geometry.point_at(fraction) else {
            continue;
        };
        let Some(position) = screen_point(camera, rect, placement.transform_point(DVec3::new(local.x, local.y, 0.0)))
        else {
            continue;
        };
        points.push(position);
    }
    points
}

/// The world point under a viewport position on the node's local XY plane, as local millimetres.
fn local_point(pointer: Pos2, rect: egui::Rect, camera: Camera, placement: Placement) -> Option<DVec2> {
    let ray = ray_to(pointer, rect, camera);
    let origin = placement.transform_point(DVec3::ZERO);
    let normal = placement.transform_vector(DVec3::Z);
    let hit = ray_plane(ray, origin, normal)?;
    let local = placement.rotation().inverse() * (hit - origin);
    let point = DVec2::new(local.x, local.y);
    point.is_finite().then_some(point)
}

/// A point of the node's local plane in viewport points.
fn local_screen(view: Screen, point: DVec2) -> Option<Pos2> {
    let world = view.placement.transform_point(DVec3::new(point.x, point.y, 0.0));
    screen_point(view.camera, view.rect, world)
}

/// A contour point as a point of the node's local plane, in millimetres.
fn plane_point(point: [Length; 2]) -> DVec2 {
    DVec2::new(point[0].millimetres_f64(), point[1].millimetres_f64())
}

/// Paints a latched move: the old position, the chord to the placed point and the reference.
///
/// The ghost marks the position the corner will leave, the dashed chord is the length the plate
/// shows, and the arc's radius is that same length, so the angle is read where it is measured
/// (ADR 0127).
fn paint_latch_guides(ui: &Ui, painter: &egui::Painter, view: Screen, latch: &CornerLatch) {
    let Some(from) = local_screen(view, plane_point(latch.start)) else {
        return;
    };
    let Some(to) = local_screen(view, latch.state.at) else {
        return;
    };
    guides::construction(ui, painter, from, to);
    painter.circle_stroke(from, GRIP_RADIUS, egui::Stroke::new(1.0, BASE_COLOR));
    let Some(reference) = latch.state.reference else {
        return;
    };
    let (axis, sweep) = reference_screen(reference, |point| local_screen(view, point));
    guides::reference(painter, &axis, &sweep);
}

/// Keeps an arc corner on its circle, or returns a line corner unchanged.
///
/// A corner joins two segments; when either side is an arc the corner is constrained by that arc's
/// circle, so the pointer point is projected onto it before the edit sees it.
fn project_arc_corner(draft: &ContourDraft, corner: PathCorner, point: DVec2) -> Option<DVec2> {
    for candidate in adjacent_segments(draft, corner).into_iter().flatten() {
        if let ContourSegment::Arc { center, radius, .. } = candidate {
            let center = DVec2::new(center[0].millimetres_f64(), center[1].millimetres_f64());
            let offset = point - center;
            if offset.length() <= f64::EPSILON {
                return None;
            }
            return Some(center + offset.normalize() * radius.millimetres_f64());
        }
    }
    Some(point)
}

/// The segments that share `corner`, in draft travel order.
///
/// A ring resolves an end to the next segment's start, so both slots exist. An open chain leaves
/// the slot across its free start or free end empty; a closed chain wraps to the other end.
fn adjacent_segments(draft: &ContourDraft, corner: PathCorner) -> [Option<ContourSegment>; 2] {
    let entries = draft.entries();
    let Some(index) = draft.index_of(corner.segment()) else {
        return [None, None];
    };
    let count = entries.len();
    if count == 0 {
        return [None, None];
    }
    match draft {
        ContourDraft::Ring(_) => {
            let start = if corner.is_end() { (index + 1) % count } else { index };
            let previous = (start + count - 1) % count;
            [entries.get(start).map(|entry| entry.segment()), entries.get(previous).map(|entry| entry.segment())]
        }
        ContourDraft::Chain(path) => {
            let Some(current) = entries.get(index).map(|entry| entry.segment()) else {
                return [None, None];
            };
            let neighbour = if corner.is_end() {
                if index + 1 < count {
                    Some(index + 1)
                } else if path.is_closed() {
                    Some(0)
                } else {
                    None
                }
            } else if index > 0 {
                Some(index - 1)
            } else if path.is_closed() {
                Some(count - 1)
            } else {
                None
            };
            [Some(current), neighbour.and_then(|neighbour| entries.get(neighbour)).map(|entry| entry.segment())]
        }
    }
}

/// The point on `segment` where a split is allowed, or `None` next to a corner.
///
/// The point is the pointer projected onto the segment's own curve; the margin keeps a double-click
/// near a corner from minting a segment a few micrometres long.
fn split_point(segment: ContourSegment, local: DVec2) -> Option<[Length; 2]> {
    let point = match segment {
        ContourSegment::Line { start, end } => {
            let start = DVec2::new(start[0].millimetres_f64(), start[1].millimetres_f64());
            let end = DVec2::new(end[0].millimetres_f64(), end[1].millimetres_f64());
            let direction = end - start;
            let length_squared = direction.length_squared();
            if length_squared <= f64::EPSILON {
                return None;
            }
            let along = (local - start).dot(direction) / length_squared;
            if along <= SPLIT_MARGIN || along >= 1.0 - SPLIT_MARGIN {
                return None;
            }
            start + direction * along
        }
        ContourSegment::Arc { center, radius, start_angle, sweep } => {
            let center = DVec2::new(center[0].millimetres_f64(), center[1].millimetres_f64());
            let offset = local - center;
            if offset.length() <= f64::EPSILON {
                return None;
            }
            let point = center + offset.normalize() * radius.millimetres_f64();
            let angle = (point - center).y.atan2((point - center).x);
            let start = start_angle.radians_f64();
            let sweep = sweep.radians_f64();
            let mut travel = angle - start;
            let turn = std::f64::consts::TAU;
            travel = if sweep >= 0.0 { travel.rem_euclid(turn) } else { -((-travel).rem_euclid(turn)) };
            let margin = sweep.abs() * SPLIT_MARGIN;
            let outside = if sweep >= 0.0 {
                travel <= margin || travel >= sweep - margin
            } else {
                travel >= -margin || travel <= sweep + margin
            };
            if outside {
                return None;
            }
            point
        }
    };
    Some([length_from_millimetres(point.x)?, length_from_millimetres(point.y)?])
}

/// A contour point in the node's local frame.
fn local_world(point: [Length; 2]) -> DVec3 {
    DVec3::new(point[0].millimetres_f64(), point[1].millimetres_f64(), 0.0)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use geom::DVec2;
    use model::{ContourSegment, Path};
    use testkit::{millimetres, open_path};
    use units::Degrees;

    use super::*;

    /// The viewport the gizmo projects into.
    fn viewport() -> egui::Rect {
        egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))
    }

    fn rectangle() -> Contour {
        Contour::from_polygon(vec![
            [millimetres(0), millimetres(0)],
            [millimetres(100), millimetres(0)],
            [millimetres(100), millimetres(60)],
            [millimetres(0), millimetres(60)],
        ])
        .expect("rectangle")
    }

    /// A 100 × 60 mm three-point chain, open at both ends.
    fn l_path() -> Path {
        open_path(vec![
            [millimetres(0), millimetres(0)],
            [millimetres(100), millimetres(0)],
            [millimetres(100), millimetres(60)],
        ])
    }

    /// A line joined to a quarter arc of radius 40, open at both ends.
    fn line_and_arc() -> Path {
        Path::new(
            vec![
                ContourSegment::line([millimetres(0), millimetres(0)], [millimetres(100), millimetres(0)])
                    .expect("line"),
                ContourSegment::arc(
                    [millimetres(100), millimetres(40)],
                    millimetres(40),
                    Degrees::from_millidegrees(-90_000),
                    Degrees::from_millidegrees(90_000),
                )
                .expect("arc"),
            ],
            false,
        )
        .expect("line and arc")
    }

    #[test]
    fn a_line_split_projects_the_pointer_and_refuses_the_corners() {
        let segment = rectangle().id_at(0).and_then(|id| rectangle().segment(id)).expect("bottom edge");
        let point = split_point(segment, DVec2::new(40.0, 3.0)).expect("inside the edge");
        assert_eq!(point, [millimetres(40), millimetres(0)], "the point sits on the line");

        assert!(split_point(segment, DVec2::new(0.5, 0.0)).is_none(), "too close to the start corner");
        assert!(split_point(segment, DVec2::new(99.5, 0.0)).is_none(), "too close to the end corner");
        assert!(split_point(segment, DVec2::new(140.0, 0.0)).is_none(), "past the end corner");
    }

    #[test]
    fn an_open_chain_puts_a_handle_on_its_free_end() {
        let path = l_path();
        let draft = ContourDraft::Chain(path.clone());
        let handles = corner_handles(&draft);
        let first = path.id_at(0).expect("first segment");
        let last = path.id_at(1).expect("last segment");
        assert_eq!(handles, vec![PathCorner::Start(first), PathCorner::Start(last), PathCorner::End(last)]);
        assert_eq!(corner_position(&draft, PathCorner::Start(first)), Some([millimetres(0), millimetres(0)]));
        assert_eq!(corner_position(&draft, PathCorner::Start(last)), Some([millimetres(100), millimetres(0)]));
        assert_eq!(
            corner_position(&draft, PathCorner::End(last)),
            Some([millimetres(100), millimetres(60)]),
            "the last point is the last segment's end"
        );
    }

    #[test]
    fn a_closed_chain_has_one_handle_per_corner() {
        let path =
            Path::rectangle([millimetres(0), millimetres(0)], [millimetres(100), millimetres(60)]).expect("rectangle");
        let draft = ContourDraft::Chain(path);
        let handles = corner_handles(&draft);
        assert_eq!(handles.len(), 4, "the last end is the first start and draws no second handle");
        assert!(handles.iter().all(|corner| !corner.is_end()));
    }

    #[test]
    fn an_arc_corner_is_projected_onto_its_circle() {
        let arc = ContourSegment::arc(
            [millimetres(100), millimetres(20)],
            millimetres(20),
            Degrees::from_millidegrees(-90_000),
            Degrees::from_millidegrees(90_000),
        )
        .expect("quarter arc");
        let contour = Contour::new(vec![
            ContourSegment::line([millimetres(0), millimetres(0)], [millimetres(100), millimetres(0)]).expect("line"),
            arc,
            ContourSegment::line([millimetres(120), millimetres(20)], [millimetres(120), millimetres(60)])
                .expect("line"),
            ContourSegment::line([millimetres(120), millimetres(60)], [millimetres(0), millimetres(60)]).expect("line"),
            ContourSegment::line([millimetres(0), millimetres(60)], [millimetres(0), millimetres(0)]).expect("line"),
        ])
        .expect("ring");
        let draft = ContourDraft::Ring(contour);
        let id = draft.entries().iter().find(|entry| entry.segment().is_arc()).map(|entry| entry.id()).expect("arc");
        let projected = project_arc_corner(&draft, PathCorner::Start(id), DVec2::new(125.0, 35.0)).expect("projection");
        let center = DVec2::new(100.0, 20.0);
        assert!(((projected - center).length() - 20.0).abs() < 1.0e-9, "the corner stays on the circle");
        assert!(projected.x > 100.0 && projected.y > 20.0, "the projection follows the pointer's quadrant");
    }

    #[test]
    fn an_open_chain_paints_as_an_open_polyline_ending_on_its_free_end() {
        let camera = Camera::default();
        let path = l_path();
        let draft = ContourDraft::Chain(path.clone());
        let shape = draft_shape(camera, viewport(), Placement::IDENTITY, &draft, egui::Stroke::new(2.0, DRAFT_COLOR))
            .expect("three points make a polyline");
        let egui::Shape::Path(painted) = shape else {
            panic!("a chain paints as a path, got {shape:?}");
        };
        assert!(!painted.closed, "an open chain must not join its last point back to the first");
        assert_eq!(painted.points.len(), 3, "one screen point per corner");

        let last = path.id_at(1).expect("last segment");
        let free_end = corner_position(&draft, PathCorner::End(last)).expect("the free end");
        let expected = screen_point(camera, viewport(), Placement::IDENTITY.transform_point(local_world(free_end)))
            .expect("the free end is in front");
        assert_eq!(painted.points.last().copied(), Some(expected), "the polyline ends on the last point");
    }

    #[test]
    fn a_closed_chain_and_a_ring_paint_closed() {
        let camera = Camera::default();
        let rectangle =
            Path::rectangle([millimetres(0), millimetres(0)], [millimetres(100), millimetres(60)]).expect("rectangle");
        let chain = draft_shape(
            camera,
            viewport(),
            Placement::IDENTITY,
            &ContourDraft::Chain(rectangle.clone()),
            egui::Stroke::new(2.0, DRAFT_COLOR),
        )
        .expect("a closed chain paints");
        assert!(matches!(chain, egui::Shape::Path(ref painted) if painted.closed), "a closed chain closes");

        let ring = draft_shape(
            camera,
            viewport(),
            Placement::IDENTITY,
            &ContourDraft::Ring(rectangle.to_contour().expect("ring")),
            egui::Stroke::new(2.0, DRAFT_COLOR),
        )
        .expect("a ring paints");
        assert!(matches!(ring, egui::Shape::Path(ref painted) if painted.closed), "a ring always closes");
    }

    #[test]
    fn a_draft_without_enough_screen_points_paints_nothing() {
        let camera = Camera::default();
        let single = Path::new(
            vec![
                ContourSegment::line([millimetres(0), millimetres(0)], [millimetres(100), millimetres(0)])
                    .expect("line"),
            ],
            false,
        )
        .expect("one line");
        assert!(
            draft_shape(
                camera,
                viewport(),
                Placement::IDENTITY,
                &ContourDraft::Chain(single),
                egui::Stroke::new(2.0, DRAFT_COLOR)
            )
            .is_some(),
            "one line still has two screen points and paints"
        );
        assert!(
            draft_shape(
                camera,
                egui::Rect::from_min_size(Pos2::ZERO, egui::Vec2::ZERO),
                Placement::IDENTITY,
                &ContourDraft::Chain(l_path()),
                egui::Stroke::new(2.0, DRAFT_COLOR)
            )
            .is_none(),
            "a zero-size viewport collapses every corner onto one point, so there is no line to draw"
        );
    }

    /// A project whose root holds one path object.
    fn path_project(path: Path) -> (Project, model::Id) {
        let mut project = testkit::sample_project();
        let root = project.root().id();
        let id =
            project.add_object(root, "Sketch", Placement::IDENTITY, model::Geometry::Path(path)).expect("path object");
        (project, id)
    }

    #[test]
    fn painting_an_open_chain_adds_one_open_polyline_and_a_handle_on_every_corner() {
        let path = l_path();
        let (project, object) = path_project(path.clone());
        let edit = ContourEdit::start(&project, ContourTarget::Path(object)).expect("a path is editable");
        let camera = Camera::default();
        let rect = viewport();

        let ctx = egui::Context::default();
        ctx.set_fonts(egui::FontDefinitions::empty());
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            ContourGizmo::default().paint(ui, rect, camera, &project, &edit);
        });

        let mut paths = Vec::new();
        let mut filled = 0;
        let mut outlined = 0;
        for clipped in &output.shapes {
            match &clipped.shape {
                egui::Shape::Path(painted) => paths.push(painted.clone()),
                egui::Shape::Circle(circle) if circle.fill.a() > 0 => filled += 1,
                egui::Shape::Circle(_) => outlined += 1,
                _ => {}
            }
        }
        assert_eq!(paths.len(), 1, "an untouched draft paints one polyline, not a base and a copy");
        assert!(!paths[0].closed, "the painted chain is open");
        assert_eq!(paths[0].points.len(), 3);
        assert_eq!(filled, 3, "two segment starts and the free end, and nothing else");
        assert_eq!(outlined, 3, "every handle keeps its dark outline");
        output.drop_without_applying_deltas();

        // A dirty draft paints the stored shape faintly behind the working copy.
        let mut edit = edit;
        let last = edit.draft().id_at(1).expect("last segment");
        edit.move_corner_at(PathCorner::End(last), [millimetres(140), millimetres(60)], CornerMove::Free)
            .expect("move the free end");
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            ContourGizmo::default().paint(ui, rect, camera, &project, &edit);
        });
        let paths: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::Shape::Path(painted) => Some(painted.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(paths.len(), 2, "the base chain and the working copy");
        assert!(paths.iter().all(|painted| !painted.closed), "both paint open");
        output.drop_without_applying_deltas();
    }

    #[test]
    fn a_chain_corner_next_to_an_arc_is_projected_onto_the_arc() {
        let path = line_and_arc();
        let draft = ContourDraft::Chain(path.clone());
        let line = path.id_at(0).expect("line");
        let arc = path.id_at(1).expect("arc");
        let center = DVec2::new(100.0, 40.0);

        let shared = project_arc_corner(&draft, PathCorner::End(line), DVec2::new(125.0, 35.0)).expect("projection");
        assert!(((shared - center).length() - 40.0).abs() < 1.0e-9, "the shared corner stays on the arc");
        let free_end = project_arc_corner(&draft, PathCorner::End(arc), DVec2::new(125.0, 35.0)).expect("projection");
        assert!(((free_end - center).length() - 40.0).abs() < 1.0e-9, "the free arc end stays on the circle");

        let line_start = PathCorner::Start(line);
        assert_eq!(
            project_arc_corner(&draft, line_start, DVec2::new(-10.0, 5.0)),
            Some(DVec2::new(-10.0, 5.0)),
            "a corner with no arc neighbour takes the pointer point"
        );
    }

    #[test]
    fn an_arc_split_needs_a_point_inside_the_sweep() {
        let arc = ContourSegment::arc(
            [millimetres(100), millimetres(20)],
            millimetres(20),
            Degrees::from_millidegrees(-90_000),
            Degrees::from_millidegrees(90_000),
        )
        .expect("quarter arc");
        let inside = split_point(arc, DVec2::new(115.0, 5.0)).expect("inside the sweep");
        let point = DVec2::new(inside[0].millimetres_f64(), inside[1].millimetres_f64());
        assert!(((point - DVec2::new(100.0, 20.0)).length() - 20.0).abs() < 1.0e-3, "on the circle");
        assert!(split_point(arc, DVec2::new(120.0, 25.0)).is_none(), "too close to the end corner");
        assert!(split_point(arc, DVec2::new(100.0, 20.0)).is_none(), "the centre has no direction");
    }

    fn screen_of(camera: Camera, point: [Length; 2]) -> Pos2 {
        screen_point(camera, viewport(), Placement::IDENTITY.transform_point(local_world(point))).expect("in front")
    }

    #[test]
    fn an_along_contour_drag_picks_the_segment_nearest_the_pointer() {
        let path =
            Path::rectangle([millimetres(0), millimetres(0)], [millimetres(100), millimetres(60)]).expect("rectangle");
        let draft = ContourDraft::Chain(path.clone());
        let bottom = path.id_at(0).expect("bottom side");
        let left = path.id_at(3).expect("left side");
        let corner = PathCorner::Start(bottom);
        let camera = Camera::default();

        let near_bottom = screen_of(camera, [millimetres(50), millimetres(0)]);
        assert_eq!(along_segment(camera, viewport(), Placement::IDENTITY, &draft, corner, near_bottom), Some(bottom));
        let near_left = screen_of(camera, [millimetres(0), millimetres(30)]);
        assert_eq!(along_segment(camera, viewport(), Placement::IDENTITY, &draft, corner, near_left), Some(left));
    }

    #[test]
    fn the_along_toggle_resolves_a_corner_motion_and_defaults_to_free() {
        let mut gizmo = ContourGizmo::default();
        assert!(!gizmo.along(), "free movement is the default");
        let path =
            Path::rectangle([millimetres(0), millimetres(0)], [millimetres(100), millimetres(60)]).expect("rectangle");
        let draft = ContourDraft::Chain(path.clone());
        let corner = PathCorner::Start(path.id_at(0).expect("bottom"));
        let camera = Camera::default();
        let pointer = Some(screen_of(camera, [millimetres(50), millimetres(0)]));
        assert_eq!(
            gizmo.motion_for(camera, viewport(), Placement::IDENTITY, &draft, ContourGrip::Corner(corner), pointer),
            CornerMove::Free
        );
        gizmo.toggle_along();
        assert_eq!(
            gizmo.motion_for(camera, viewport(), Placement::IDENTITY, &draft, ContourGrip::Corner(corner), pointer),
            CornerMove::Along(path.id_at(0).expect("bottom"))
        );
    }

    #[test]
    fn a_latch_measures_the_move_from_the_corner_it_took() {
        // The click that latches a corner opens the two numbers of its delta; the guides start at
        // the old position, which is the only fixed reference a move has (ADR 0129).
        let path = l_path();
        let draft = ContourDraft::Chain(path.clone());
        let corner = ContourGrip::Corner(PathCorner::Start(path.id_at(0).expect("the first segment")));
        let start = draft.grip_position(corner).expect("the corner");
        let camera = Camera::default();
        let pointer = screen_of(camera, [millimetres(60), millimetres(40)]);
        let view = Screen { camera, rect: viewport(), placement: Placement::IDENTITY };
        let local = local_point(pointer, view.rect, view.camera, view.placement).expect("the pointer is on the plane");
        let latch = ContourGizmo::default()
            .latch_move(&draft, corner, pointer, local, view, CornerMove::Free)
            .expect("a latch");
        assert_eq!(latch.start, start, "the latch keeps the old position");
        assert_eq!(latch.grip, corner);
        assert!(!latch.moved, "a latch that has not followed the pointer writes nothing");
        assert_eq!(latch.fields.rows().len(), 2, "a move is a distance and an angle");
        assert_eq!(latch.state.reference.map(|reference| reference.anchor), Some(plane_point(start)));
        let moved = (latch.state.at - plane_point(start)).length();
        assert!((latch.state.reference.map(|reference| reference.radius).unwrap_or_default() - moved).abs() < 1e-6);
        assert_eq!(latch.state.construction, Some([plane_point(start), latch.state.at]));
        assert_eq!(latch.fields.value(FieldParam::Length), Some(moved), "the plate shows the distance moved");
    }

    #[test]
    fn a_circle_paints_a_square_centre_and_four_round_quadrants() {
        let path = Path::circle([millimetres(10), millimetres(20)], millimetres(40)).expect("circle");
        let (project, object) = path_project(path);
        let edit = ContourEdit::start(&project, ContourTarget::Path(object)).expect("editable");
        let ctx = egui::Context::default();
        ctx.set_fonts(egui::FontDefinitions::empty());
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            ContourGizmo::default().paint(ui, viewport(), Camera::default(), &project, &edit);
        });
        let mut filled_circles = 0;
        let mut filled_rects = 0;
        for clipped in &output.shapes {
            match &clipped.shape {
                egui::Shape::Circle(circle) if circle.fill.a() > 0 => filled_circles += 1,
                egui::Shape::Rect(rect) if rect.fill.a() > 0 => filled_rects += 1,
                _ => {}
            }
        }
        assert_eq!(filled_circles, 4, "one round handle per quadrant");
        assert_eq!(filled_rects, 1, "the centre is the only square handle");
        output.drop_without_applying_deltas();
    }

    #[test]
    fn an_arc_curve_offers_a_radius_grip_away_from_its_midpoint() {
        let path = Path::new(
            vec![
                ContourSegment::arc(
                    [millimetres(0), millimetres(0)],
                    millimetres(200),
                    Degrees::ZERO,
                    Degrees::from_millidegrees(180_000),
                )
                .expect("a half circle"),
            ],
            false,
        )
        .expect("an arc chain");
        let draft = ContourDraft::Chain(path.clone());
        let arc = path.id_at(0).expect("arc");
        let camera = Camera::default();
        // A quarter of the way along the sweep, away from the persistent midpoint handle.
        let angle = 45.0_f64.to_radians();
        let point =
            [millimetres((200.0 * angle.cos()).round() as i64), millimetres((200.0 * angle.sin()).round() as i64)];
        let pointer = screen_of(camera, point);
        let grip = grip_at(camera, viewport(), Placement::IDENTITY, &draft, pointer, Some(arc));
        assert!(
            matches!(grip, Some(ContourGrip::ArcRadius(segment, _)) if segment == arc),
            "the hovered curve offers its radius: {grip:?}"
        );

        // The persistent midpoint handle wins when the pointer is right on it.
        let midpoint = draft.grip_position(ContourGrip::ArcPoint(arc)).expect("an arc midpoint");
        let pointer = screen_of(camera, midpoint);
        assert_eq!(
            grip_at(camera, viewport(), Placement::IDENTITY, &draft, pointer, Some(arc)),
            Some(ContourGrip::ArcPoint(arc))
        );
    }

    #[test]
    fn a_numeric_field_starts_from_the_grip_value_and_its_kind() {
        let circle =
            ContourDraft::Chain(Path::circle([millimetres(0), millimetres(0)], millimetres(40)).expect("circle"));
        let field = grip_field(&circle, ContourGrip::CircleQuadrant(0), Pos2::ZERO, None).expect("a radius field");
        assert_eq!(field.kind, GripNumber::Length);
        assert_eq!(field.fields.value(FieldParam::Length), Some(40.0));
        assert_eq!(field.start, Some(GripValue::Length(millimetres(40))));

        let arc = ContourDraft::Chain(line_and_arc());
        let id = arc.entries().last().expect("arc").id();
        let field =
            grip_field(&arc, ContourGrip::Corner(PathCorner::End(id)), Pos2::ZERO, None).expect("an angle field");
        assert_eq!(field.kind, GripNumber::Angle);
        assert_eq!(field.fields.value(FieldParam::Angle), Some(0.0));
        let label = field.fields.rows().first().and_then(|row| row.label);
        assert!(
            matches!(label, Some(FieldLabel::Symbol(symbol)) if symbol.contains('∠') || symbol == "R"),
            "the field is labelled: {label:?}"
        );

        assert!(grip_field(&circle, ContourGrip::CircleCenter, Pos2::ZERO, None).is_none(), "a centre has no number");
    }

    #[test]
    fn an_along_contour_numeric_field_shows_the_length_from_the_far_end() {
        let draft = ContourDraft::Chain(l_path());
        let first = draft.id_at(0).expect("first segment");
        let second = draft.id_at(1).expect("second segment");
        let corner = PathCorner::Start(second);
        let motion = Some(CornerMove::Along(first));
        let field = grip_field(&draft, ContourGrip::Corner(corner), Pos2::ZERO, motion).expect("a length field");
        assert_eq!(field.kind, GripNumber::Length);
        assert_eq!(
            field.fields.rows().first().and_then(|row| row.label),
            Some(FieldLabel::Symbol("L")),
            "the along-contour field is labelled L"
        );
        assert_eq!(field.fields.value(FieldParam::Length), Some(100.0), "the corner sits at the far end");
        assert_eq!(field.motion, Some(CornerMove::Along(first)));

        // An arc's value is its arc length, not its chord.
        let arc_draft = ContourDraft::Chain(line_and_arc());
        let arc = arc_draft.id_at(1).expect("arc segment");
        let arc_corner = PathCorner::End(arc);
        let field = grip_field(&arc_draft, ContourGrip::Corner(arc_corner), Pos2::ZERO, Some(CornerMove::Along(arc)))
            .expect("an arc length field");
        // A quarter arc of radius 40 is 40·π/2 ≈ 62.83 mm.
        let expected = (40.0 * std::f64::consts::FRAC_PI_2 * 1_000.0).round() as i64;
        assert_eq!(
            field.fields.value(FieldParam::Length),
            Some(Length::from_micrometres(expected).millimetres_f64()),
            "the arc length is 62.83 mm"
        );
    }

    #[test]
    fn a_grip_is_found_within_the_hit_radius_and_refused_outside_it() {
        let path = Path::circle([millimetres(0), millimetres(0)], millimetres(40)).expect("circle");
        let draft = ContourDraft::Chain(path.clone());
        let camera = Camera::default();
        let circle_centre_in_screen = screen_of(camera, [millimetres(0), millimetres(0)]);

        let right_on = grip_at(camera, viewport(), Placement::IDENTITY, &draft, circle_centre_in_screen, None);
        assert!(matches!(right_on, Some(ContourGrip::CircleCenter)), "exactly on the centre grabs it");

        let far_away = circle_centre_in_screen + egui::vec2(100.0, 0.0);
        let outside = grip_at(camera, viewport(), Placement::IDENTITY, &draft, far_away, None);
        assert!(outside.is_none(), "100 pt from the centre is well beyond the 9 pt hit radius");
    }

    #[test]
    fn a_grip_within_9_pt_is_still_grabbed_after_a_small_move() {
        let path = Path::circle([millimetres(0), millimetres(0)], millimetres(40)).expect("circle");
        let draft = ContourDraft::Chain(path.clone());
        let camera = Camera::default();
        let centre_screen = screen_of(camera, [millimetres(0), millimetres(0)]);

        // The press origin is right on the centre, so the grip resolves even if the drag
        // fires at a position 5 pt away (below the 9 pt hit radius from the centre).
        let at_origin = grip_at(camera, viewport(), Placement::IDENTITY, &draft, centre_screen, None);
        assert!(
            matches!(at_origin, Some(ContourGrip::CircleCenter)),
            "the grip resolves from the press origin, not the moved pointer"
        );

        // A pointer 5 pt from the centre is still within the 9 pt hit radius, so a grip that
        // started at the centre would still be held.
        let moved = centre_screen + egui::vec2(5.0, 0.0);
        let near_origin = grip_at(camera, viewport(), Placement::IDENTITY, &draft, moved, None);
        assert!(near_origin.is_some(), "5 pt from the centre is inside the hit radius");
    }

    #[test]
    fn length_rounding_refuses_values_that_cannot_be_stored() {
        assert_eq!(length_from_millimetres(1.2345), Some(Length::from_micrometres(1235)));
        assert!(length_from_millimetres(f64::NAN).is_none());
        assert!(length_from_millimetres(f64::INFINITY).is_none());
    }
}
