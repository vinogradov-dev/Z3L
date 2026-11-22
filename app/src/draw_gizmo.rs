//! The viewport tool that draws on the work plane.
//!
//! While a drawing draft is live the gizmo owns the pointer: it maps the cursor onto the work
//! plane, snaps it to a draft end, an axis or the 10 mm grid, appends a point on a click and closes
//! a polyline through its first point. The right-click menu chooses whether the next polyline
//! segment is straight or an arc, so one chain carries both kinds; Alt drops every snap and takes
//! the pointer as it is.
//!
//! The point under the pointer carries its numbers: one plate per parameter, placed beside the thing
//! it measures, and the guides that read the angle from the nearer end of the X axis (ADR 0126,
//! 0127, 0128). A typed number locks its parameter and outranks a snap, Enter places the point with
//! every plate and Escape drops the typed numbers before it ends the tool. Painting shows the placed
//! chain, the shape the next click would make, the snap marker and the two guides. A preview that
//! cannot become a path keeps the last valid one, so a chain never blinks out; the reason is on
//! [`DrawGizmo::preview_error`]. The dummy document is never touched here; the commit is a separate
//! step, so Escape costs nothing.
//!
//! The plane is the one the pointer names until the draft places its first point: in the 3D snap mode
//! that is the face under the pointer — the face the ruler's own probe measures — so a sketch started
//! on a board's top face is stored in that face's plane, and in the 2.5D mode or over empty space it
//! is the camera's plane (U121). The first placed point freezes the choice, so orbiting the view
//! mid-draft reinterprets nothing, and the plane draws itself as a faint cross of its own axes while
//! the draft is open.

use eframe::egui::{self, Color32, Pos2, Shape, Stroke, Ui};
use geom::{Contact, DVec2, DVec3};
use model::{MAX_POLYGON_SIDES, MIN_POLYGON_SIDES, Path, Project, Sight};
use scene::Camera;
use ui::{DrawDraft, DrawTool, Editor, FieldParam, FieldSet, PointState, field_row, guides, theme};
use units::Length;

use crate::gizmo_math::{
    Aim, PlaneSnap, WorkPlane, length_from_millimetres, paint_snap_marker, reference_screen, screen_point,
    surface_under,
};

/// Radius of a placed draft point marker, in points.
const POINT_RADIUS: f32 = 3.5;
/// Sampling tolerance for a painted preview, in millimetres.
const PREVIEW_TOLERANCE_MM: f64 = 0.25;
/// How many segments each meridian of the sphere phantom is sampled into.
const PHANTOM_STEPS: usize = 48;
/// How long the plane guide's first world step is, in points at the plane's depth.
///
/// The span is then extended in screen space, so this only has to be long enough for the projected
/// direction to be well-conditioned.
const PLANE_GUIDE_STEP_POINTS: f64 = 8.0;

/// Colour of the points already placed.
const DRAFT_COLOR: Color32 = Color32::from_rgb(120, 200, 255);
/// Colour of the shape the next click would make.
const PREVIEW_COLOR: Color32 = Color32::from_rgb(255, 220, 96);
/// Colour of the phantom sphere the sphere tools stand on the plane: the preview's own ink, thin
/// enough to read the scene through it.
const PHANTOM_COLOR: Color32 = Color32::from_rgba_premultiplied(120, 100, 40, 110);
/// Colour of a draft end that would close the chain.
const CLOSE_COLOR: Color32 = Color32::from_rgb(120, 230, 140);

/// The keyboard state a click carries while a drawing tool runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DrawModifiers {
    /// Shift locks the next segment to the nearest axis.
    pub axis_lock: bool,
    /// Alt takes the pointer without snapping it to a draft end or the grid.
    pub ignore_snaps: bool,
}

/// What a click on the snapped point does to the live draft.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DrawClick {
    /// Append the point to the chain as a straight segment.
    Append([Length; 2]),
    /// Append the point as an arc through it, waiting for the next click.
    StartArc([Length; 2]),
    /// Close the chain through its first point.
    Close,
    /// The click cannot change the draft.
    Ignore,
}

/// What one pointer frame of the drawing tool did.
///
/// The gizmo never closes a draft itself: closing owns the work plane and the commit, both the
/// shell's (ADR 0052), so a close is reported and the caller runs the one close path.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum DrawAction {
    /// Nothing was placed.
    #[default]
    Nothing,
    /// A point was placed, by the pointer or by Enter, or an arc was armed.
    Point,
    /// The click landed on the chain's first point and asks for the ring to be closed.
    Close,
    /// Escape with nothing typed: the tool ends and the document is untouched.
    Cancel,
}

/// Everything one pointer frame of the drawing tool carries besides the camera and the draft.
///
/// The workspace's snap kinds, the mode that reads a candidate above the plane, the contact adapter
/// the probe measures bodies with, the chrome under the pointer and the keyboard travel together
/// because a frame reads all of them once and the frame's own signature is already at clippy's
/// argument threshold (AGENTS §1.5, U116).
#[derive(Clone, Copy, Debug)]
pub(crate) struct DrawInput<'a> {
    /// Which snaps the workspace allows.
    pub(crate) kinds: model::SnapKinds,
    /// How the workspace reads a candidate above the plane: the point itself, or its foot (U119).
    pub(crate) mode: model::SnapMode,
    /// The adapter the probe measures the bodies the boxes cannot describe with, so the face under
    /// the pointer is the one the ruler would find (U121).
    pub(crate) contact: &'a dyn Contact,
    /// Whether the chrome owns the pointer this frame.
    pub(crate) pointer_over_widget: bool,
    /// The keyboard state a click carries.
    pub(crate) modifiers: DrawModifiers,
}

/// Pointer state of the drawing tool; the draft itself belongs to the editor.
#[derive(Debug)]
pub(crate) struct DrawGizmo {
    hover: Option<SnapOutcome>,
    /// The plane the live draft draws on, kept for as long as the draft holds a point.
    ///
    /// Orbiting the view mid-drawing must not reinterpret the points already placed, so the plane
    /// is chosen while the draft is empty and kept from its first point until the tool ends (U121).
    plane: Option<WorkPlane>,
    /// The last preview that became a path, kept while the pointer sits on a hostile point.
    preview: Option<Path>,
    /// Why the current pointer cannot extend the draft, if it cannot.
    preview_error: Option<String>,
    /// The plates that name the point being placed, one set per point.
    fields: Option<FieldSet>,
    /// The stage the plates were built for: the tool and the number of points the draft holds.
    ///
    /// A stage is the pair, not the count alone: a tool change can keep the same point count, and
    /// the plates (with their typed locks) belong to one tool's point, so the new tool must rebuild
    /// them even when the shell did not reset this gizmo.
    stage: Option<(DrawTool, usize)>,
    /// The snapped pointer of the last frame, before any typed number moved it.
    pointer: Option<DVec2>,
    /// The point being placed, with the plates and guides that name it.
    state: Option<PointState>,
    /// The rectangle the plates drew last frame, so a press on one is chrome.
    field_rect: Option<egui::Rect>,
}

/// The snapped cursor and where on screen it landed.
#[derive(Clone, Copy, Debug)]
struct SnapOutcome {
    /// The point a click would use, in work-plane millimetres.
    point: [Length; 2],
    /// What captured it, or `None` for the pointer's own point.
    kind: Option<model::SnapKind>,
    /// Whether Shift's axis lock moved it onto a plane axis through the anchor.
    locked: bool,
    /// The snapped point in viewport points: the candidate itself, which a scene snap may draw
    /// above or below the plane the draft is stored on.
    screen: Pos2,
}

impl DrawGizmo {
    /// A drawing tool with no hover.
    pub(crate) fn new() -> Self {
        Self {
            hover: None,
            plane: None,
            preview: None,
            preview_error: None,
            fields: None,
            stage: None,
            pointer: None,
            state: None,
            field_rect: None,
        }
    }

    /// Drops the hover state, the frozen plane, the cached preview and the plates, for example when
    /// the tool ends.
    pub(crate) fn reset(&mut self) {
        self.hover = None;
        self.plane = None;
        self.preview = None;
        self.preview_error = None;
        self.fields = None;
        self.stage = None;
        self.pointer = None;
        self.state = None;
        self.field_rect = None;
    }

    /// The rectangle the plates drew last frame, when a drawing tool asks for numbers.
    #[cfg(test)]
    pub(crate) const fn field_rect(&self) -> Option<egui::Rect> {
        self.field_rect
    }

    /// Every plate of the point being placed, with the parameter it edits, after the last draw.
    ///
    /// A test reads the layout the person sees: which rows were drawn and where each one sits.
    #[cfg(test)]
    pub(crate) fn plate_rects(&self) -> Vec<(FieldParam, egui::Rect)> {
        let Some(fields) = self.fields.as_ref() else {
            return Vec::new();
        };
        fields.rows().iter().filter_map(|row| fields.plate_rect(row.param).map(|rect| (row.param, rect))).collect()
    }

    /// Whether `point` lies on one of the plates, which are chrome and never the scene.
    pub(crate) fn field_contains(&self, point: egui::Pos2) -> bool {
        self.fields.as_ref().is_some_and(|fields| fields.contains(point))
    }

    /// The point the plates place under the pointer, whether or not a click has placed it yet.
    #[cfg(test)]
    pub(crate) fn point_preview(&self) -> Option<[Length; 2]> {
        self.state.as_ref().map(|state| state.point)
    }

    /// Whether the plates hold the keyboard, so the tool's own key reader stands aside (ADR 0126).
    pub(crate) fn holds_keys(&self) -> bool {
        self.fields.as_ref().is_some_and(FieldSet::is_holding)
    }

    /// The last valid preview, for painting and for tests.
    pub(crate) fn preview(&self) -> Option<&Path> {
        self.preview.as_ref()
    }

    /// Why the current pointer cannot extend the draft, if it cannot.
    ///
    /// The status line shows this instead of letting a rejected point look like an accepted one.
    pub(crate) fn preview_error(&self) -> Option<&str> {
        self.preview_error.as_deref()
    }

    /// Records what the pointer at `point` would add: a new preview, or the reason there is none.
    ///
    /// A rejected point keeps the previous preview, so the chain the user already built stays on
    /// screen through an intermediate click that cannot join it. A point that simply adds nothing —
    /// the pointer sits on the chain's own end right after a click — keeps it too: clearing the
    /// cache there is what made the preview blink between clicks.
    fn refresh_preview(&mut self, draft: &DrawDraft, point: [Length; 2]) {
        match draft.preview(point) {
            Ok(Some(path)) => {
                self.preview = Some(path);
                self.preview_error = None;
            }
            Ok(None) => {
                self.preview_error = None;
                // A closed draft takes no next point, so the shape under the pointer is not a shape
                // this click would make: keeping it leaves a phantom segment on screen after the
                // ring has been closed through the chrome.
                if draft.is_closed() {
                    self.preview = None;
                }
            }
            Err(error) => self.preview_error = Some(error.to_string()),
        }
    }

    /// The plane the live draft draws on.
    pub(crate) fn plane(&self, camera: Camera) -> WorkPlane {
        self.plane.unwrap_or_else(|| WorkPlane::from_camera(camera))
    }

    /// The plane this frame draws on, and the one the draft keeps.
    ///
    /// A draft with no point follows the plane under the pointer, so a sketch started on a board's
    /// top face stores its points in that face's plane and a sketch started over empty space keeps
    /// the camera's one ([`live_plane`]). The first placed point freezes that plane: the question is
    /// answered once per draft, which is what lets the hand orbit the view without reinterpreting
    /// the points already placed (U121).
    fn plane_for(
        &mut self,
        draft: &DrawDraft,
        aim: Aim,
        project: &Project,
        input: DrawInput<'_>,
        sight: Sight<'_>,
    ) -> WorkPlane {
        if !draft.is_empty()
            && let Some(plane) = self.plane
        {
            return plane;
        }
        let plane = live_plane(aim, project, input, sight);
        self.plane = Some(plane);
        plane
    }

    /// Writes a typed side count into the running draft and returns the draft the frame must read.
    ///
    /// The side count is the figure's own number rather than the point's, so the plate edits the draft
    /// — the slot the ribbon fills — instead of the point being placed. A typed number locks its plate,
    /// which is what makes it a promise; a released plate leaves the count the draft already has. The
    /// draft clamps it to the model's range, so the preview, the commit and the document agree.
    fn apply_sides(&self, editor: &mut Editor, draft: DrawDraft) -> DrawDraft {
        let Some(fields) = self.fields.as_ref() else {
            return draft;
        };
        if !fields.is_locked(FieldParam::Sides) {
            return draft;
        }
        let Some(value) = fields.value(FieldParam::Sides) else {
            return draft;
        };
        let Some(sides) = whole_sides(value) else {
            return draft;
        };
        if editor.set_drawing_sides(sides) == Some(draft.sides()) {
            return draft;
        }
        editor.drawing().cloned().unwrap_or(draft)
    }

    /// What captured the pointer in the last frame, for the status bar's readout.
    pub(crate) fn snap_kind(&self) -> Option<model::SnapKind> {
        self.hover.and_then(|hover| hover.kind)
    }

    /// Where the last frame's marker drew, for a test that asks what the pointer captured.
    #[cfg(test)]
    pub(crate) fn hover_screen(&self) -> Option<Pos2> {
        self.hover.map(|hover| hover.screen)
    }

    /// The point the next click would place, for a test that asks where the snap answered.
    #[cfg(test)]
    pub(crate) fn hover_point(&self) -> Option<[Length; 2]> {
        self.hover.map(|hover| hover.point)
    }

    /// Applies the pointer to the live draft.
    ///
    /// A click appends the point the plates place — the snapped pointer with every typed number in
    /// its place — or asks for a polyline to close when the point is its first one. The gizmo owns
    /// the left button while the tool is live: the transform gizmos are hidden by the caller, and
    /// `pointer_over_widget` keeps the corner orientation widget usable.
    ///
    /// The returned [`DrawAction`] says what happened, which lets the caller commit a finished fixed
    /// tool and run the one close path for a ring. Clicking a fixed tool's last point does nothing.
    pub(crate) fn update(
        &mut self,
        response: &egui::Response,
        rect: egui::Rect,
        camera: Camera,
        project: &Project,
        editor: &mut Editor,
        input: DrawInput<'_>,
    ) -> Result<DrawAction, String> {
        let Some(draft) = editor.drawing().cloned() else {
            self.reset();
            return Ok(DrawAction::Nothing);
        };
        let pointer = response.hover_pos().or_else(|| response.interact_pointer_pos());
        let Some(pointer) = pointer else {
            self.hover = None;
            return Ok(DrawAction::Nothing);
        };
        let sight = editor.flags().sight();
        let hand = Aim { camera, rect, pointer };
        // The plane is the one the pointer names while the draft is empty and the one its first point
        // froze afterwards, so the face under the first click is where the whole sketch is stored
        // (U121).
        let plane = self.plane_for(&draft, hand, project, input, sight);
        let Some(aim) = PlaneSnap::at(hand.camera, hand.rect, hand.pointer, plane) else {
            self.hover = None;
            return Ok(DrawAction::Nothing);
        };
        let Some(snapped) = snap_cursor(&draft, aim, project, sight, input.kinds, input.mode, input.modifiers) else {
            self.hover = None;
            return Ok(DrawAction::Nothing);
        };
        // The plates show the numbers the pointer produces, and the point a click places is the
        // pointer with every number the person typed in its place (ADR 0126, 0128).
        let raw = plane_point(snapped.point);
        self.sync_fields(&draft, raw, plane, camera, rect);
        let draft = self.apply_sides(editor, draft);
        let placed = self.fields.as_ref().and_then(|fields| PointState::placed(&draft, raw, fields));
        let (point, at) = placed.as_ref().map_or((snapped.point, raw), |state| (state.point, state.at));
        // The marker stands on the candidate itself — a scene corner above or below the plane draws
        // where the eye sees it — and only a typed number moves it, because the point is then the
        // number's place on the plane and no longer the candidate's (U116).
        let typed = placed.as_ref().is_some_and(|state| state.point != snapped.point);
        let screen = match typed {
            true => screen_point(camera, rect, plane.world(at)).unwrap_or(snapped.screen),
            false => snapped.screen,
        };
        self.pointer = Some(raw);
        self.state = placed;
        self.hover = Some(SnapOutcome { point, kind: snapped.kind, locked: snapped.locked, screen });
        self.refresh_preview(&draft, point);

        // A click that the orientation widget owns must not also place a point.
        if input.pointer_over_widget || !response.clicked_by(egui::PointerButton::Primary) {
            return Ok(DrawAction::Nothing);
        }
        let action = apply_click(editor, &draft, point)?;
        if matches!(action, DrawAction::Point)
            && let Some(draft) = editor.drawing()
        {
            // The click moved the chain, so the preview belongs to the new state under the same
            // pointer rather than to the state before it.
            self.refresh_preview(draft, point);
        }
        Ok(action)
    }

    /// Keeps the plates of the point being placed current: one set per point, its numbers live.
    ///
    /// The set is rebuilt when the draft gains or loses a point, because that is what changes the
    /// numbers a tool asks for and resets the locks: a typed number is a promise for one point.
    /// Every other frame only moves the plates and refreshes the values the pointer produces.
    fn sync_fields(&mut self, draft: &DrawDraft, pointer: DVec2, plane: WorkPlane, camera: Camera, rect: egui::Rect) {
        let stage = (draft.tool(), draft.points().len());
        let Some(state) = PointState::live(draft, pointer) else {
            return;
        };
        let fallback = project(plane, camera, rect, pointer).unwrap_or(rect.center());
        let plates = |state: &PointState| {
            state
                .rows
                .iter()
                .map(|row| field_row(*row, project(plane, camera, rect, row.anchor).unwrap_or(fallback)))
                .collect::<Vec<_>>()
        };
        if self.stage != Some(stage) {
            self.stage = Some(stage);
            self.fields = FieldSet::new(plates(&state));
        }
        let Some(fields) = self.fields.as_mut() else {
            return;
        };
        for row in &state.rows {
            fields.set_live(row.param, row.value);
            fields.set_anchor(row.param, project(plane, camera, rect, row.anchor).unwrap_or(fallback));
        }
    }

    /// Draws the plates of the point being placed and runs the keys they own.
    ///
    /// The plates are drawn whatever the pointer rests on, so a number can be typed without moving
    /// the hand and the point on screen keeps its own numbers. Enter places the point the way a
    /// click does, with every plate; Escape drops the typed numbers first and ends the tool second,
    /// which is the order a CAD field teaches (ADR 0126).
    pub(crate) fn show_fields(
        &mut self,
        ctx: &egui::Context,
        rect: egui::Rect,
        editor: &mut Editor,
    ) -> Result<Option<DrawAction>, String> {
        let (action, drawn) = {
            let Some(fields) = self.fields.as_mut() else {
                self.field_rect = None;
                return Ok(None);
            };
            let action = fields.show(ctx, rect);
            (action, fields.rect())
        };
        self.field_rect = (drawn.width() > 0.0 && drawn.height() > 0.0).then_some(drawn);
        if !action.is_none() {
            ctx.request_repaint();
        }
        if action.cancel {
            editor.cancel_drawing();
            return Ok(Some(DrawAction::Cancel));
        }
        // A count typed in this frame is the figure's number: it reaches the draft here, so the next
        // click or Enter builds the polygon the plate promised rather than the one before it.
        self.apply_sides(editor, DrawDraft::default());
        if !action.commit {
            return Ok(None);
        }
        let (Some(draft), Some(pointer), Some(fields)) =
            (editor.drawing().cloned(), self.pointer, self.fields.as_ref())
        else {
            return Ok(None);
        };
        let Some(state) = PointState::placed(&draft, pointer, fields) else {
            return Ok(None);
        };
        apply_click(editor, &draft, state.point).map(Some)
    }

    /// Paints the plane guide, the placed chain, the preview, the points, the snap marker and the
    /// two guides of the point being placed.
    pub(crate) fn paint(&self, ui: &Ui, rect: egui::Rect, camera: Camera, draft: &DrawDraft) {
        let plane = self.plane(camera);
        let painter = ui.painter().with_clip_rect(rect);

        // The plane the sketch is stored on draws itself as a faint cross of its own axes, so a 3D
        // sketch on the side of a board says so before the first click does (U121).
        if let Some((axis, sweep)) = plane_axes(camera, rect, plane) {
            guides::work_plane(ui, &painter, axis, sweep);
        }

        match draft.to_path() {
            Ok(path) => paint_path(&painter, camera, rect, plane, &path, Stroke::new(2.0, DRAFT_COLOR)),
            // A chain that cannot become one path still has the segments it already stores: draw
            // them one by one so a ring rule or a missing point never makes the drawing vanish.
            Err(_) => {
                for segment in draft.segments() {
                    if let Ok(path) = Path::new(vec![*segment], false) {
                        paint_path(&painter, camera, rect, plane, &path, Stroke::new(2.0, DRAFT_COLOR));
                    }
                }
            }
        }
        if let Some(hover) = self.hover {
            if let Some(path) = self.preview() {
                paint_path(&painter, camera, rect, plane, path, Stroke::new(1.5, PREVIEW_COLOR));
            }
            // A sphere is not a figure in the document yet, so the ball it will become stands on the
            // plane as a phantom: the equator is the preview and two meridians make it read as a
            // ball rather than as a circle (ADR 0139).
            if draft.tool().primitive_kind().is_some()
                && let Some(path) = self.preview()
            {
                paint_phantom(&painter, camera, rect, plane, path);
            }
            for point in draft.points() {
                paint_point(&painter, camera, rect, plane, *point, DRAFT_COLOR, POINT_RADIUS);
            }
            let closing = closes_at(draft, hover.point);
            if closing {
                paint_point(&painter, camera, rect, plane, hover.point, CLOSE_COLOR, POINT_RADIUS + 2.0);
            }
            paint_marker(&painter, hover, theme::accent(ui), closing);
        } else {
            for point in draft.points() {
                paint_point(&painter, camera, rect, plane, *point, DRAFT_COLOR, POINT_RADIUS);
            }
        }
        if let Some(state) = &self.state {
            paint_guides(ui, &painter, camera, rect, plane, state);
        }
    }
}

/// Applies a click at `point` to the draft, the way the pointer and Enter both place one.
///
/// The ring is closed by the caller: one close path serves the viewport, the ribbon and the status
/// bar, and it commits in the same step (ADR 0052).
fn apply_click(editor: &mut Editor, draft: &DrawDraft, point: [Length; 2]) -> Result<DrawAction, String> {
    Ok(match click_action(draft, point) {
        DrawClick::Append(point) => {
            editor.drawing_point(point).map_err(|error| error.to_string())?;
            DrawAction::Point
        }
        DrawClick::StartArc(point) => {
            editor.drawing_arc_through(point).map_err(|error| error.to_string())?;
            DrawAction::Point
        }
        DrawClick::Close => DrawAction::Close,
        DrawClick::Ignore => DrawAction::Nothing,
    })
}

/// A work-plane point in viewport points.
fn project(plane: WorkPlane, camera: Camera, rect: egui::Rect, point: DVec2) -> Option<Pos2> {
    screen_point(camera, rect, plane.world(point))
}

/// Paints the two guides of the point being placed: the chord and the reference with its arc.
///
/// The guides are screen paint, so their weight is a rule and not a world size (ADR 0111), and every
/// point of them is a work-plane point the camera projects: the reference's axis and sweep come from
/// [`reference_screen`], so a tilted view draws the ellipse the plane's circle really is (ADR 0133).
fn paint_guides(
    ui: &Ui,
    painter: &egui::Painter,
    camera: Camera,
    rect: egui::Rect,
    plane: WorkPlane,
    state: &PointState,
) {
    let project = |point: DVec2| screen_point(camera, rect, plane.world(point));
    if let Some([from, to]) = state.construction
        && let (Some(from), Some(to)) = (project(from), project(to))
    {
        guides::construction(ui, painter, from, to);
    }
    // The lines a figure is built against are drawn with the same dashed guide, so the corner of a
    // tangent circle reads as the two lines it is measured from.
    for [from, to] in &state.legs {
        if let (Some(from), Some(to)) = (project(*from), project(*to)) {
            guides::construction(ui, painter, from, to);
        }
    }
    let Some(reference) = state.reference else {
        return;
    };
    let (axis, sweep) = reference_screen(reference, project);
    guides::reference(painter, &axis, &sweep);
}

/// The plane a draft with no point draws on: the face under the pointer, or the camera's plane.
///
/// The 3D mode takes the plane of the face the probe finds — the one reading the ruler measures, so
/// a sketch started on a board's top face stores its points in that face's plane — and keeps the
/// camera's plane when the pointer names no surface. The 2.5D mode keeps the reading it has always
/// had: the plane is where the view put it, and a candidate above it answers with its foot (U121).
fn live_plane(aim: Aim, project: &Project, input: DrawInput<'_>, sight: Sight<'_>) -> WorkPlane {
    let camera_plane = WorkPlane::from_camera(aim.camera);
    if input.mode == model::SnapMode::Foot {
        return camera_plane;
    }
    let ray = aim.ray();
    surface_under(project, input.contact, ray.origin(), ray.direction(), sight)
        .and_then(|probe| WorkPlane::from_face(probe.face.point, probe.face.normal))
        .unwrap_or(camera_plane)
}

/// The plane's own two axes on screen, for the guide that says where a sketch is stored.
///
/// Both spans run through the plane's point at the middle of the view and are then extended across
/// the whole view, so the cross reads as the plane rather than as a mark at one point of it. The
/// extension happens in screen space because a straight world line projects to a straight screen
/// line: extending the world span instead would run one end of it behind the eye in a wide view and
/// lose the whole guide. A plane the view looks along has no point at its middle and draws nothing;
/// one seen nearly edge-on draws a span almost collapsed, which is the truth about that view.
fn plane_axes(camera: Camera, rect: egui::Rect, plane: WorkPlane) -> Option<([Pos2; 2], [Pos2; 2])> {
    let centre = plane.pointer_point(camera, rect, rect.center())?;
    let origin = plane.world(centre);
    let depth = (origin - camera.eye()).dot(camera.forward());
    let step = camera.world_per_point(depth, f64::from(rect.height())) * PLANE_GUIDE_STEP_POINTS;
    if !step.is_finite() || step <= 0.0 {
        return None;
    }
    let span = |axis: DVec2| {
        let base = screen_point(camera, rect, origin)?;
        let next = screen_point(camera, rect, plane.world(centre + axis * step))?;
        let direction = next - base;
        let length = direction.length();
        if !length.is_finite() || length <= f32::EPSILON {
            return None;
        }
        let reach = direction / length * (rect.width() + rect.height());
        Some([base - reach, base + reach])
    };
    Some((span(DVec2::X)?, span(DVec2::Y)?))
}

/// The snapped cursor point, in document units and on screen.
///
/// The resolver answers the point the draft stores, so whatever captured the pointer — a corner, a
/// crossing, the grid — the marker is drawn at that point and the figure grows to it; a typed number
/// is the one thing that moves it elsewhere (U116, T17, U119).
fn snap_cursor(
    draft: &DrawDraft,
    aim: PlaneSnap,
    project: &Project,
    sight: Sight<'_>,
    kinds: model::SnapKinds,
    mode: model::SnapMode,
    modifiers: DrawModifiers,
) -> Option<SnapOutcome> {
    let plane = aim.plane;
    // The next segment starts where the last stored segment ended, so the axis lock and the
    // perpendicular and tangent constructions measure from the stored joint rather than from the
    // click that began it.
    let anchor = draft.last_end().map(|point| plane.world(plane_point(point)));
    // The last point is the segment start, so it is not offered as a place to join.
    let ends = draft
        .points()
        .split_last()
        .map_or(&[][..], |(_, ends)| ends)
        .iter()
        .map(|point| plane.world(plane_point(*point)))
        .collect::<Vec<_>>();
    // Alt takes the pointer as it is: no kind captures, and Shift's explicit lock still applies.
    let kinds = if modifiers.ignore_snaps { model::SnapKinds::none() } else { kinds };
    // The sketch draws on a plane, so it reads a candidate above that plane the way the workspace
    // chose: in 2.5D the answer is the candidate's foot on the plane, in 3D only a candidate that
    // lies in the plane captures and the answer is that point (U119).
    let target = mode.target();
    let answer = project.snap(aim.request(kinds, target, anchor, &ends, modifiers.axis_lock, None), sight)?;
    // The answer is the point the draft stores, so the marker is projected from that point and never
    // from the candidate it came from: one place, one marker (U119).
    let local = plane.local(answer.point);
    let point = [length_from_millimetres(local.x)?, length_from_millimetres(local.y)?];
    let stored = plane.world(DVec2::new(point[0].millimetres_f64(), point[1].millimetres_f64()));
    let screen = screen_point(aim.camera, aim.rect, stored)?;
    Some(SnapOutcome { point, kind: answer.kind, locked: answer.locked, screen })
}

/// The whole side count a plate's number names, inside the range a polygon accepts.
///
/// The plate's own range already keeps the number between three and the model's widest polygon, so
/// this is the one place the field's `f64` becomes the count: a value that is not a number in that
/// range is refused rather than clamped, because a plate can never show one.
fn whole_sides(value: f64) -> Option<u32> {
    if !value.is_finite() {
        return None;
    }
    let whole = value.round();
    if whole < f64::from(MIN_POLYGON_SIDES) || whole > f64::from(MAX_POLYGON_SIDES) {
        return None;
    }
    Some(whole as u32)
}

/// Whether the hovered point is the chain's first one, where a click closes the ring.
///
/// The same `can_close` the ribbon's Draft cell reads decides whether the ring can finish, so
/// the green node never promises a closure the command would refuse. A pending arc counts: it is
/// the closing segment, so the first node stays green while the arc waits and the pointer is on it.
fn closes_at(draft: &DrawDraft, point: [Length; 2]) -> bool {
    draft.can_close() && draft.points().first() == Some(&point)
}

/// What the click does, given the live draft and the snapped point.
///
/// While an arc waits for its end, every click ends it: the gesture is already chosen and a second
/// one would be ambiguous, so this test comes before the close-on-first-point check. Otherwise Tab
/// decides whether a polyline grows a straight segment or starts an arc.
fn click_action(draft: &DrawDraft, point: [Length; 2]) -> DrawClick {
    if draft.is_closed() {
        return DrawClick::Ignore;
    }
    if draft.pending_arc().is_some() {
        return DrawClick::Append(point);
    }
    if draft.tool().closes() && draft.points().first() == Some(&point) {
        return if draft.can_close() { DrawClick::Close } else { DrawClick::Ignore };
    }
    if let Some(need) = draft.tool().points()
        && draft.points().len() >= need
    {
        return DrawClick::Ignore;
    }
    if draft.last_end() == Some(point) {
        return DrawClick::Ignore;
    }
    if draft.arc_mode() && draft.tool().closes() && !draft.is_empty() {
        return DrawClick::StartArc(point);
    }
    DrawClick::Append(point)
}

/// A document point in work-plane millimetres.
fn plane_point(point: [Length; 2]) -> DVec2 {
    DVec2::new(point[0].millimetres_f64(), point[1].millimetres_f64())
}

/// Paints the two meridian rings of the phantom sphere a sphere tool stands on the plane.
///
/// The draft's own preview is the equator: the path it builds is the circle the ball is built
/// around, so only the two great circles through the plane's normal are added here. Each sample is
/// exactly `radius` from the centre, so the phantom is a ball of the number the plates name and not
/// an ellipse that happens to look round from one viewpoint.
fn paint_phantom(painter: &egui::Painter, camera: Camera, rect: egui::Rect, plane: WorkPlane, path: &Path) {
    let Some((centre, radius)) = path.circle_center_radius() else {
        return;
    };
    let radius = radius.millimetres_f64();
    if !radius.is_finite() || radius <= 0.0 {
        return;
    }
    for ring in phantom_rings(plane, plane_point(centre), radius) {
        let mut screen = Vec::with_capacity(PHANTOM_STEPS.saturating_add(1));
        for point in ring {
            let Some(position) = screen_point(camera, rect, point) else {
                continue;
            };
            screen.push(position);
        }
        if screen.len() >= 2 {
            painter.add(Shape::line(screen, Stroke::new(1.2, PHANTOM_COLOR)));
        }
    }
}

/// The two great circles through the phantom sphere's poles, in world millimetres.
///
/// One ring turns about the plane's X axis and the other about its Y, so the pair crosses at the
/// centre's normal and the ball reads as a ball from every viewpoint. Each ring repeats its first
/// sample at the end, so it is drawn as one closed line.
fn phantom_rings(plane: WorkPlane, centre: DVec2, radius: f64) -> [Vec<DVec3>; 2] {
    let mut rings = [Vec::new(), Vec::new()];
    for (index, axis) in [DVec2::X, DVec2::Y].into_iter().enumerate() {
        let Some(ring) = rings.get_mut(index) else {
            continue;
        };
        ring.reserve(PHANTOM_STEPS.saturating_add(1));
        for step in 0..=PHANTOM_STEPS {
            let angle = core::f64::consts::TAU * step as f64 / PHANTOM_STEPS as f64;
            let (sin, cos) = angle.sin_cos();
            let base = plane.world(centre + axis * (radius * cos));
            ring.push(base + plane.normal() * (radius * sin));
        }
    }
    rings
}

/// Paints one path as a screen-space polyline through its sampled points.
fn paint_path(
    painter: &egui::Painter,
    camera: Camera,
    rect: egui::Rect,
    plane: WorkPlane,
    path: &Path,
    stroke: Stroke,
) {
    let Ok(segments) = path.segments().map(|segment| segment.to_geom()).collect::<Result<Vec<_>, _>>() else {
        return;
    };
    let Ok(chain) = geom::Contour::new_with_joint_tolerance(segments, false, |first, next| {
        model::joint_tolerance_mm(*first, *next)
    }) else {
        return;
    };
    let Ok(samples) = chain.sample(PREVIEW_TOLERANCE_MM) else {
        return;
    };
    let mut screen = Vec::with_capacity(samples.len());
    for point in samples {
        let world = plane.world(point);
        let Some(position) = screen_point(camera, rect, world) else {
            continue;
        };
        screen.push(position);
    }
    if screen.len() >= 2 {
        painter.add(Shape::line(screen, stroke));
    }
}

/// Paints one point marker on the work plane.
fn paint_point(
    painter: &egui::Painter,
    camera: Camera,
    rect: egui::Rect,
    plane: WorkPlane,
    point: [Length; 2],
    color: Color32,
    radius: f32,
) {
    let Some(position) = screen_point(camera, rect, plane.world(plane_point(point))) else {
        return;
    };
    painter.circle_filled(position, radius, color);
    painter.circle_stroke(position, radius, Stroke::new(1.0, Color32::from_black_alpha(160)));
}

/// Paints the snap marker: the kind's own glyph, in the theme's ink.
///
/// A point that would close the ring keeps the green the chain has always drawn, because that is a
/// promise about the draft and not a kind of snap.
fn paint_marker(painter: &egui::Painter, hover: SnapOutcome, ink: Color32, closing: bool) {
    let ink = if closing { CLOSE_COLOR } else { ink };
    paint_snap_marker(painter, hover.screen, hover.kind, hover.locked, ink);
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use testkit::millimetres;
    use ui::DrawTool;

    use super::*;

    fn draft(tool: DrawTool, points: &[(i64, i64)]) -> DrawDraft {
        let mut draft = DrawDraft::new(tool);
        for (x, y) in points {
            draft.push([millimetres(*x), millimetres(*y)]).expect("a fresh point");
        }
        draft
    }

    /// The viewport the plane tests draw in, in points.
    const VIEWPORT: egui::Rect = egui::Rect { min: Pos2::new(0.0, 0.0), max: Pos2::new(800.0, 600.0) };

    /// A pointer frame of the workspace: every kind but the vertex off, so a capture is the sketch's
    /// own corner and nothing else.
    fn input<'a>(mode: model::SnapMode, contact: &'a dyn Contact) -> DrawInput<'a> {
        let mut kinds = model::SnapKinds::none();
        kinds.set(model::SnapKind::Vertex, true);
        DrawInput { kinds, mode, contact, pointer_over_widget: false, modifiers: DrawModifiers::default() }
    }

    /// A project with nothing in it, so a pointer can point at the empty space above the floor.
    fn empty_project() -> Project {
        let root = model::Container::new(
            model::Id::new(1).expect("a non-zero id"),
            "Root",
            model::Placement::IDENTITY,
            Vec::new(),
        );
        Project::new(root).expect("an empty project")
    }

    #[test]
    fn a_sketch_started_on_a_board_takes_that_face_as_its_plane() {
        // U121: a 3D sketch on a board's top face is stored in that face's plane, so its own corners
        // lie in the sketch plane and capture it in the 3D reading.
        let project = testkit::sample_project();
        let camera = Camera::default();
        let contact = contact_parry::Parry;
        // The middle of the sample shelf's top face, 18 mm above the world plane.
        let on_face = DVec3::new(350.0, 250.0, 18.0);
        let pointer = screen_point(camera, VIEWPORT, on_face).expect("the face is on screen");
        let aim = Aim { camera, rect: VIEWPORT, pointer };
        let sight = Sight::none();

        let mut gizmo = DrawGizmo::new();
        let empty = DrawDraft::new(DrawTool::Line);
        let plane = gizmo.plane_for(&empty, aim, &project, input(model::SnapMode::Space, &contact), sight);
        let origin = plane.world(DVec2::ZERO);
        assert!((plane.normal() - DVec3::Z).length() < 1.0e-6, "the face's plane is horizontal: {:?}", plane.normal());
        assert!((origin - on_face).length() < 0.01, "the plane runs through the face: {origin:?} against {on_face:?}");

        // The same pointer in the 2.5D reading keeps the camera's plane: the mode is the switch, not
        // the scene (U119, U121).
        let mut flat = DrawGizmo::new();
        let two_and_a_half = flat.plane_for(&empty, aim, &project, input(model::SnapMode::Foot, &contact), sight);
        assert_eq!(two_and_a_half, WorkPlane::from_camera(camera), "the 2.5D mode draws where the view says");

        // A chain whose corners lie in that face captures its own corner in the 3D mode: the pointer
        // sits a few points beside the first point, and the answer is that point exactly.
        let mut chain = DrawDraft::new(DrawTool::Line);
        chain.push([Length::ZERO, Length::ZERO]).expect("the first corner of the face");
        chain.push([millimetres(200), Length::ZERO]).expect("the second corner");
        let beside = screen_point(camera, VIEWPORT, origin).expect("the corner is on screen") + egui::vec2(3.0, 0.0);
        let factors = DrawModifiers::default();
        let snapped = PlaneSnap::at(camera, VIEWPORT, beside, plane).expect("the pointer meets the plane");
        let answer = snap_cursor(
            &chain,
            snapped,
            &project,
            sight,
            input(model::SnapMode::Space, &contact).kinds,
            model::SnapMode::Space,
            factors,
        )
        .expect("the sketch's own corner answers");
        assert_eq!(answer.point, [Length::ZERO, Length::ZERO], "the corner the chain stores");
        assert_eq!(answer.kind, Some(model::SnapKind::Vertex), "a corner of the figure, not the scene");
    }

    #[test]
    fn a_sketch_started_over_empty_space_keeps_the_camera_plane() {
        let project = empty_project();
        let contact = contact_parry::Parry;
        for camera in [Camera::default(), Camera::default()] {
            let pointer = VIEWPORT.center();
            let aim = Aim { camera, rect: VIEWPORT, pointer };
            let mut gizmo = DrawGizmo::new();
            let empty = DrawDraft::new(DrawTool::Line);
            let plane = gizmo.plane_for(&empty, aim, &project, input(model::SnapMode::Space, &contact), Sight::none());
            assert_eq!(plane, WorkPlane::from_camera(camera), "a pointer over nothing draws on the camera's plane");
        }
    }

    #[test]
    fn the_plane_a_draft_started_on_does_not_follow_the_camera() {
        // The plane is answered once per draft: orbiting the view and moving the hand over another
        // face mid-draft leaves the points already placed where they were (U121).
        let project = testkit::sample_project();
        let contact = contact_parry::Parry;
        let first_camera = Camera::default();
        let on_face = DVec3::new(350.0, 250.0, 18.0);
        let pointer = screen_point(first_camera, VIEWPORT, on_face).expect("the face is on screen");
        let aim = Aim { camera: first_camera, rect: VIEWPORT, pointer };
        let mut gizmo = DrawGizmo::new();
        let empty = DrawDraft::new(DrawTool::Line);
        let started = gizmo.plane_for(&empty, aim, &project, input(model::SnapMode::Space, &contact), Sight::none());

        let mut orbited = first_camera;
        orbited.orbit(0.6, -0.4);
        let ceiling = DVec3::new(200.0, 150.0, 400.0);
        let pointer = screen_point(orbited, VIEWPORT, ceiling).unwrap_or_else(|| VIEWPORT.center());
        let elsewhere = Aim { camera: orbited, rect: VIEWPORT, pointer };
        let mut started_draft = DrawDraft::new(DrawTool::Line);
        started_draft.push([Length::ZERO, Length::ZERO]).expect("the point the first click placed");
        let kept = gizmo.plane_for(
            &started_draft,
            elsewhere,
            &project,
            input(model::SnapMode::Space, &contact),
            Sight::none(),
        );
        assert_eq!(kept, started, "the plane stays where the first point was placed");
        assert_eq!(gizmo.plane(orbited), started, "and painting reads the same plane");
    }

    #[test]
    fn the_plane_guide_crosses_the_middle_of_the_view() {
        // The guide says where the sketch is stored: the plane's own two axes, both through the
        // plane's point at the middle of the view, so the cross covers the view at any zoom (U121).
        let camera = Camera::default();
        let plane = WorkPlane::from_face(DVec3::new(0.0, 0.0, 350.0), DVec3::Z).expect("a face plane");
        let (axis, sweep) = plane_axes(camera, VIEWPORT, plane).expect("a face seen from above draws its cross");
        let centre = VIEWPORT.center();
        for span in [axis, sweep] {
            let along = span[1] - span[0];
            let aside =
                |point: Pos2| (point - span[0] - along * ((point - span[0]).dot(along) / along.length_sq())).length();
            assert!(aside(centre) < 0.5, "the cross runs through the middle of the view: {span:?}");
            assert!(
                (span[1] - span[0]).length() > VIEWPORT.width().min(VIEWPORT.height()),
                "a span reaches across the view: {span:?}"
            );
        }

        // A plane the view looks along — the ray at the middle of the view runs in it — has no point
        // at its middle and draws nothing rather than a line the person would read as other plane.
        let edge_on = WorkPlane::from_face(DVec3::ZERO, camera.forward().cross(DVec3::Z)).expect("a plane");
        assert_eq!(plane_axes(camera, VIEWPORT, edge_on), None);
    }

    #[test]
    fn the_sphere_phantom_is_two_great_circles_of_the_drawn_radius() {
        // The equator is the draft's own preview; the two meridians are what makes the ball read as
        // a ball. Every sample must stand the drawn radius from the centre, and each ring must close
        // on its first sample (ADR 0139).
        let plane = WorkPlane::WORLD_XY;
        let centre = DVec2::new(100.0, 50.0);
        let radius = 40.0;
        let origin = plane.world(centre);
        let mut lifted = false;
        for ring in phantom_rings(plane, centre, radius) {
            assert_eq!(ring.len(), PHANTOM_STEPS + 1, "one sample per step and the closing one");
            let (Some(first), Some(last)) = (ring.first(), ring.last()) else {
                panic!("a ring has samples");
            };
            assert!((*first - *last).length() < 1e-9, "the last sample meets the first");
            for point in &ring {
                assert!(
                    ((*point - origin).length() - radius).abs() < 1e-9,
                    "{point:?} stands {radius} from the centre"
                );
                lifted |= (point.z - origin.z).abs() > radius * 0.9;
            }
        }
        assert!(lifted, "the meridians leave the plane, so the phantom is a ball and not a circle");
    }

    #[test]
    fn a_polyline_closes_through_its_first_point_only() {
        let polyline = draft(DrawTool::Polyline, &[(0, 0), (600, 0), (600, 400)]);
        let first = [millimetres(0), millimetres(0)];
        assert_eq!(click_action(&polyline, first), DrawClick::Close);
        assert_eq!(
            click_action(&polyline, [millimetres(300), millimetres(200)]),
            DrawClick::Append([millimetres(300), millimetres(200)])
        );

        let short = draft(DrawTool::Polyline, &[(0, 0), (600, 0)]);
        assert_eq!(click_action(&short, first), DrawClick::Ignore, "one segment cannot close a ring");
    }

    #[test]
    fn the_first_node_lights_up_when_a_click_would_close_the_chain() {
        // A chain of two segments is one click from a ring: the first node turns green under the
        // pointer. One segment is not, and a closed chain is finished, so neither lights up.
        let two = draft(DrawTool::Polyline, &[(0, 0), (600, 0), (600, 400)]);
        let first = [millimetres(0), millimetres(0)];
        assert!(closes_at(&two, first), "the first node closes the chain");
        assert!(!closes_at(&two, [millimetres(600), millimetres(400)]), "only the first node closes");

        let one = draft(DrawTool::Polyline, &[(0, 0), (600, 0)]);
        assert!(!closes_at(&one, first), "one segment cannot close");

        let mut closed = two.clone();
        closed.close().expect("close the triangle");
        assert!(!closes_at(&closed, first), "a closed chain cannot close again");
    }

    #[test]
    fn the_first_node_lights_up_with_a_pending_arc() {
        // The closing arc is what the user is drawing: while it waits for its end, the first node is
        // still the place that finishes the ring, so it must light up there too (ADR 0077).
        let mut draft = draft(DrawTool::Polyline, &[(0, 0), (600, 0), (600, 400)]);
        assert!(draft.toggle_arc_mode());
        draft.push_arc_through([millimetres(300), millimetres(300)]).expect("arm the closing arc");
        assert!(draft.pending_arc().is_some());
        let first = [millimetres(0), millimetres(0)];
        assert!(closes_at(&draft, first), "the first node closes through the pending arc");
        assert_eq!(click_action(&draft, first), DrawClick::Append(first), "the click ends the arc on it");
    }

    #[test]
    fn tab_starts_an_arc_only_inside_a_running_chain() {
        let mut polyline = draft(DrawTool::Polyline, &[(0, 0), (600, 0)]);
        let through = [millimetres(650), millimetres(50)];
        assert_eq!(click_action(&polyline, through), DrawClick::Append(through));
        assert!(polyline.toggle_arc_mode());
        assert_eq!(click_action(&polyline, through), DrawClick::StartArc(through));

        // A chain needs an anchor before an arc can leave it.
        let mut fresh = DrawDraft::new(DrawTool::Polyline);
        fresh.toggle_arc_mode();
        assert_eq!(click_action(&fresh, through), DrawClick::Append(through));
        // A fixed tool keeps its own gesture and never mixes.
        let line = draft(DrawTool::Line, &[(0, 0)]);
        assert_eq!(click_action(&line, through), DrawClick::Append(through));
    }

    #[test]
    fn a_pending_arc_takes_the_click_before_the_closing_point() {
        // Once an arc waits for its end, even a click on the chain's first point ends the arc; the
        // close rule then runs on the finished segment, where it belongs.
        let mut polyline = draft(DrawTool::Polyline, &[(0, 0), (600, 0), (600, 400)]);
        polyline.toggle_arc_mode();
        polyline.push_arc_through([millimetres(300), millimetres(300)]).expect("arm the arc");
        assert_eq!(
            click_action(&polyline, [millimetres(0), millimetres(0)]),
            DrawClick::Append([millimetres(0), millimetres(0)])
        );
    }

    #[test]
    fn a_fixed_tool_ignores_a_click_after_its_last_point() {
        let line = draft(DrawTool::Line, &[(0, 0), (600, 0)]);
        assert_eq!(click_action(&line, [millimetres(600), millimetres(400)]), DrawClick::Ignore);
        assert_eq!(
            click_action(&draft(DrawTool::Circle, &[(100, 100)]), [millimetres(0), millimetres(0)]),
            DrawClick::Append([millimetres(0), millimetres(0)])
        );
    }

    #[test]
    fn a_click_on_the_previous_point_or_a_closed_chain_does_nothing() {
        let line = draft(DrawTool::Line, &[(0, 0)]);
        let same = [millimetres(0), millimetres(0)];
        assert_eq!(click_action(&line, same), DrawClick::Ignore);

        let mut closed = draft(DrawTool::Polyline, &[(0, 0), (600, 0), (600, 400)]);
        closed.close().expect("close the triangle");
        assert_eq!(click_action(&closed, [millimetres(10), millimetres(10)]), DrawClick::Ignore);
    }

    #[test]
    fn the_through_point_of_an_arc_paints_the_chord_its_length_plate_measures() {
        // X51: on the arc's second click the arc itself cannot be drawn yet, so the length plate
        // would name a distance with no line under it. The frame paints the chord from the start to
        // the point this click places, and the plate on it reads that chord.
        let arc = draft(DrawTool::Arc, &[(0, 0)]);
        let camera = Camera::default();
        let plane = WorkPlane::from_camera(camera);
        let rect = egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0));
        for pointer in [DVec2::new(300.0, 0.0), DVec2::new(180.0, 160.0), DVec2::new(-90.0, 210.0)] {
            let state = PointState::live(&arc, pointer).expect("a through point");
            let [from, to] = state.construction.expect("the chord is measured");
            let start = project(plane, camera, rect, from).expect("the start projects");
            let end = project(plane, camera, rect, to).expect("the point projects");
            let ctx = egui::Context::default();
            let output = ctx.run_ui(egui::RawInput { screen_rect: Some(rect), ..Default::default() }, |ui| {
                paint_guides(ui, ui.painter(), camera, rect, plane, &state);
            });
            let dashes = dashes(&output.shapes);
            output.drop_without_applying_deltas();

            assert!(dashes.len() >= 3, "{pointer:?}: the chord is a dashed line, not one stroke");
            let span = end - start;
            let length = span.length();
            assert!(length > 1.0, "{pointer:?}: the chord is long enough to read on screen");
            let along = |point: Pos2| (point - start).dot(span) / length;
            let aside = |point: Pos2| (point - start - span * ((point - start).dot(span) / (length * length))).length();
            for (a, b) in &dashes {
                assert!(aside(*a) < 0.5 && aside(*b) < 0.5, "{pointer:?}: a dash leaves the chord: {a:?} {b:?}");
                assert!(along(*a) >= -0.5, "{pointer:?}: a dash starts before the arc's start");
            }
            let reach = dashes.iter().map(|(_, b)| along(*b)).fold(0.0_f32, f32::max);
            assert!(reach > length - 10.0, "{pointer:?}: the dashes reach {reach} of {length} on screen");

            let plate = state.rows.iter().find(|row| row.param == FieldParam::Length).expect("a length plate");
            assert!((plate.value - (to - from).length()).abs() < 1e-9, "{pointer:?}: the plate reads the chord");
        }
    }

    #[test]
    fn a_tangent_circle_paints_the_two_lines_it_is_built_against() {
        // X52: the circle does not exist until its last click, so the lines the person is naming have
        // to be on screen while they are clicked. Each is the same dashed guide a chord uses, drawn
        // from the corner past the click that named it.
        let corner = draft(DrawTool::CircleTangent, &[(0, 0)]);
        let two = draft(DrawTool::CircleTangent, &[(0, 0), (300, 0)]);
        let camera = Camera::default();
        let plane = WorkPlane::from_camera(camera);
        let rect = egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0));
        for (case, figure, lines) in [("one line", corner, 1), ("two lines", two, 2)] {
            let state = PointState::live(&figure, DVec2::new(0.0, 200.0)).expect("a point on the next line");
            assert_eq!(state.legs.len(), lines, "{case}: the lines are drawn");
            let ctx = egui::Context::default();
            let output = ctx.run_ui(egui::RawInput { screen_rect: Some(rect), ..Default::default() }, |ui| {
                paint_guides(ui, ui.painter(), camera, rect, plane, &state);
            });
            let dashes = dashes(&output.shapes);
            output.drop_without_applying_deltas();
            assert!(dashes.len() >= 3, "{case}: a line is dashed, not one stroke");
            let legs = state
                .legs
                .iter()
                .filter_map(|[from, to]| {
                    let from = project(plane, camera, rect, *from)?;
                    let to = project(plane, camera, rect, *to)?;
                    Some((from, to))
                })
                .collect::<Vec<_>>();
            assert_eq!(legs.len(), lines, "{case}: every line reaches the screen");
            for (a, b) in &dashes {
                let on_a_leg = legs.iter().any(|(from, to)| {
                    let span = *to - *from;
                    let length = span.length();
                    let aside =
                        |point: Pos2| (point - *from - span * ((point - *from).dot(span) / (length * length))).length();
                    let along = |point: Pos2| (point - *from).dot(span) / length;
                    aside(*a) < 0.5 && aside(*b) < 0.5 && along(*a) >= -0.5 && along(*b) <= length + 0.5
                });
                assert!(on_a_leg, "{case}: a dash leaves both lines: {a:?} {b:?}");
            }
        }
    }

    /// Every dash a frame painted, as the segment it draws.
    fn dashes(shapes: &[egui::epaint::ClippedShape]) -> Vec<(Pos2, Pos2)> {
        fn collect(shape: &egui::Shape, out: &mut Vec<(Pos2, Pos2)>) {
            match shape {
                Shape::LineSegment { points, .. } => out.push((points[0], points[1])),
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
    fn a_rejected_preview_keeps_the_last_valid_one() {
        let mut draft = DrawDraft::new(DrawTool::Arc);
        draft.push([millimetres(0), millimetres(0)]).expect("start");
        draft.push([millimetres(50), millimetres(50)]).expect("through");
        let mut gizmo = DrawGizmo::new();
        gizmo.refresh_preview(&draft, [millimetres(100), millimetres(0)]);
        assert!(gizmo.preview().is_some(), "three sound points preview an arc");
        assert_eq!(gizmo.preview_error(), None);

        // The third click is collinear, so the arc cannot exist; the last arc stays on screen and
        // the status line gets the reason instead of a blink.
        gizmo.refresh_preview(&draft, [millimetres(100), millimetres(100)]);
        assert!(gizmo.preview().is_some(), "the previous preview survives a rejected point");
        assert!(gizmo.preview_error().is_some(), "the reason is reported");

        gizmo.reset();
        assert!(gizmo.preview().is_none());
        assert_eq!(gizmo.preview_error(), None);
    }

    #[test]
    fn a_point_that_adds_nothing_keeps_the_preview() {
        // After a click the pointer sits on the segment's own stored end, so `preview` returns
        // `Ok(None)`; dropping the cache there made the chain blink between clicks.
        let mut draft = DrawDraft::new(DrawTool::Polyline);
        draft.push([millimetres(0), millimetres(0)]).expect("start");
        let mut gizmo = DrawGizmo::new();
        let next = [millimetres(600), millimetres(0)];
        gizmo.refresh_preview(&draft, next);
        assert!(gizmo.preview().is_some(), "the first segment previews");
        draft.push(next).expect("the click places it");
        gizmo.refresh_preview(&draft, next);
        assert_eq!(draft.preview(next), Ok(None), "the pointer adds nothing on the stored end");
        assert!(gizmo.preview().is_some(), "the last valid preview survives the click");
        assert_eq!(gizmo.preview_error(), None);
    }

    #[test]
    fn closing_the_draft_drops_the_preview() {
        // Closed through the chrome, the ring takes no next point, so the segment the pointer
        // previewed is not a segment this click would make: keeping it left a phantom line on screen
        // after the user finished the figure (X23).
        let mut draft = DrawDraft::new(DrawTool::Polyline);
        draft.push([millimetres(0), millimetres(0)]).expect("start");
        draft.push([millimetres(600), millimetres(0)]).expect("bottom");
        draft.push([millimetres(600), millimetres(400)]).expect("right");
        let mut gizmo = DrawGizmo::new();
        let next = [millimetres(0), millimetres(400)];
        gizmo.refresh_preview(&draft, next);
        assert!(gizmo.preview().is_some(), "an open chain previews the next segment");

        draft.close().expect("the ring closes");
        assert_eq!(draft.preview(next), Ok(None), "a closed draft previews nothing");
        gizmo.refresh_preview(&draft, next);
        assert!(gizmo.preview().is_none(), "the closed ring drops the segment it can never add");
        assert_eq!(gizmo.preview_error(), None, "a closed draft is not an error");
    }

    #[test]
    fn arming_an_arc_keeps_the_chain_preview() {
        // The click that arms an arc cannot preview an arc to itself (three equal points are
        // degenerate), so the last shape stays on screen while the pointer waits for the end.
        let mut draft = DrawDraft::new(DrawTool::Polyline);
        draft.push([millimetres(0), millimetres(0)]).expect("start");
        draft.push([millimetres(600), millimetres(0)]).expect("bottom");
        draft.push([millimetres(600), millimetres(400)]).expect("right");
        draft.toggle_arc_mode();
        let mut gizmo = DrawGizmo::new();
        let through = [millimetres(300), millimetres(300)];
        gizmo.refresh_preview(&draft, through);
        assert!(gizmo.preview().is_some(), "the next straight segment previews");
        draft.push_arc_through(through).expect("arm the arc");
        gizmo.refresh_preview(&draft, through);
        assert!(gizmo.preview().is_some(), "the pending arc never blanks the preview");
        assert!(gizmo.preview_error().is_some(), "the degenerate arc names its reason");
    }
}
