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

use eframe::egui::{self, Color32, Pos2, Shape, Stroke, Ui};
use furnicraft_geom::DVec2;
use furnicraft_model::{MAX_POLYGON_SIDES, MIN_POLYGON_SIDES, Path};
use furnicraft_scene::Camera;
use furnicraft_ui::{DrawDraft, DrawTool, Editor, FieldParam, FieldSet, PointState, field_row, guides};
use furnicraft_units::Length;

use crate::gizmo_math::{
    SnapMode, SnapRequest, SnapTarget, WorkPlane, length_from_millimetres, ray_to, reference_screen, screen_point,
    snap_point,
};

/// Pointer distance inside which a draft end captures the pointer, in points.
const SNAP_RADIUS_POINTS: f32 = 10.0;
/// Radius of a placed draft point marker, in points.
const POINT_RADIUS: f32 = 3.5;
/// Sampling tolerance for a painted preview, in millimetres.
const PREVIEW_TOLERANCE_MM: f64 = 0.25;

/// Colour of the points already placed.
const DRAFT_COLOR: Color32 = Color32::from_rgb(120, 200, 255);
/// Colour of the shape the next click would make.
const PREVIEW_COLOR: Color32 = Color32::from_rgb(255, 220, 96);
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

/// How the live pointer is snapped to the work plane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SnapSettings {
    /// The 10 mm grid is on.
    grid: bool,
    /// Shift locks the next point to an axis through the anchor.
    axis_lock: bool,
    /// Alt drops the end and grid snaps.
    ignore_snaps: bool,
}

/// Pointer state of the drawing tool; the draft itself belongs to the editor.
#[derive(Debug)]
pub(crate) struct DrawGizmo {
    grid: bool,
    hover: Option<SnapOutcome>,
    /// The plane the draft started on, frozen while the tool runs.
    ///
    /// Orbiting the view mid-drawing must not reinterpret the points already placed, so the plane
    /// is chosen once and kept until the tool ends.
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
    /// The point a click would use.
    point: [Length; 2],
    /// Why the point moved.
    target: SnapTarget,
    /// The snapped point in viewport points.
    screen: Pos2,
}

impl DrawGizmo {
    /// A drawing tool with the 10 mm grid on and no hover.
    pub(crate) fn new() -> Self {
        Self {
            grid: true,
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

    /// Whether points snap to the 10 mm grid.
    pub(crate) const fn grid(&self) -> bool {
        self.grid
    }

    /// Turns the grid snap on or off.
    pub(crate) fn toggle_grid(&mut self) {
        self.grid = !self.grid;
        self.hover = None;
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
        editor: &mut Editor,
        pointer_over_widget: bool,
        modifiers: DrawModifiers,
    ) -> Result<DrawAction, String> {
        let Some(draft) = editor.drawing().cloned() else {
            self.reset();
            return Ok(DrawAction::Nothing);
        };
        let plane = *self.plane.get_or_insert_with(|| WorkPlane::from_camera(camera));
        let pointer = response.hover_pos().or_else(|| response.interact_pointer_pos());
        let Some(pointer) = pointer else {
            self.hover = None;
            return Ok(DrawAction::Nothing);
        };
        let Some(snapped) = snap_cursor(
            &draft,
            plane,
            camera,
            rect,
            pointer,
            SnapSettings { grid: self.grid, axis_lock: modifiers.axis_lock, ignore_snaps: modifiers.ignore_snaps },
        ) else {
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
        let screen = project(plane, camera, rect, at).unwrap_or(snapped.screen);
        self.pointer = Some(raw);
        self.state = placed;
        self.hover = Some(SnapOutcome { point, target: snapped.target, screen });
        self.refresh_preview(&draft, point);

        // A click that the orientation widget owns must not also place a point.
        if pointer_over_widget || !response.clicked_by(egui::PointerButton::Primary) {
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

    /// Paints the placed chain, the preview, the points, the snap marker and the two guides.
    pub(crate) fn paint(&self, ui: &Ui, rect: egui::Rect, camera: Camera, draft: &DrawDraft) {
        let plane = self.plane(camera);
        let painter = ui.painter().with_clip_rect(rect);

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
            for point in draft.points() {
                paint_point(&painter, camera, rect, plane, *point, DRAFT_COLOR, POINT_RADIUS);
            }
            if closes_at(draft, hover.point) {
                paint_point(&painter, camera, rect, plane, hover.point, CLOSE_COLOR, POINT_RADIUS + 2.0);
            }
            paint_marker(&painter, hover);
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
    let Some(reference) = state.reference else {
        return;
    };
    let (axis, sweep) = reference_screen(reference, project);
    guides::reference(painter, &axis, &sweep);
}

/// The snapped cursor point, in document units and on screen.
fn snap_cursor(
    draft: &DrawDraft,
    plane: WorkPlane,
    camera: Camera,
    rect: egui::Rect,
    pointer: Pos2,
    settings: SnapSettings,
) -> Option<SnapOutcome> {
    let ray = ray_to(pointer, rect, camera);
    let world = plane.ray_point(ray)?;
    let depth = (world - camera.eye()).dot(camera.forward());
    let radius = camera.world_per_point(depth, f64::from(rect.height())) * f64::from(SNAP_RADIUS_POINTS);
    let raw = plane.local(world);
    // The next segment starts where the last stored segment ended, so the axis and end snap use the
    // stored joint rather than the click that began it.
    let anchor = draft.last_end().map(plane_point);
    // The last point is the segment start, so it is not offered as a place to join.
    let ends = draft
        .points()
        .split_last()
        .map_or(&[][..], |(_, ends)| ends)
        .iter()
        .map(|point| plane_point(*point))
        .collect::<Vec<_>>();
    let snapped = snap_point(SnapRequest {
        point: raw,
        anchor,
        ends: &ends,
        mode: if settings.grid { SnapMode::Grid } else { SnapMode::Free },
        axis_lock: settings.axis_lock,
        end_radius: radius,
        ignore_snaps: settings.ignore_snaps,
    });
    let (x, y) = (length_from_millimetres(snapped.point.x), length_from_millimetres(snapped.point.y));
    let point = [x?, y?];
    let world = plane.world(DVec2::new(point[0].millimetres_f64(), point[1].millimetres_f64()));
    let screen = screen_point(camera, rect, world)?;
    Some(SnapOutcome { point, target: snapped.target, screen })
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
/// The same `can_close` the ribbon and the status bar read decides whether the ring can finish, so
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
    let Ok(chain) = furnicraft_geom::Contour::new_with_joint_tolerance(segments, false, |first, next| {
        furnicraft_model::joint_tolerance_mm(*first, *next)
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

/// Paints the snap marker: a diamond whose colour says what captured the pointer.
fn paint_marker(painter: &egui::Painter, hover: SnapOutcome) {
    let color = match hover.target {
        SnapTarget::End => CLOSE_COLOR,
        SnapTarget::Grid => Color32::from_gray(210),
        SnapTarget::Axis => Color32::from_rgb(255, 220, 96),
        // The drawing tools never resolve a surface point; the ruler does, and paints its own
        // marker. The arm keeps the match total rather than inventing a colour here.
        SnapTarget::Surface => Color32::from_rgb(120, 200, 255),
        SnapTarget::Free => Color32::from_gray(150),
    };
    let center = hover.screen;
    let size = 6.0;
    let corners = vec![
        Pos2::new(center.x, center.y - size),
        Pos2::new(center.x + size, center.y),
        Pos2::new(center.x, center.y + size),
        Pos2::new(center.x - size, center.y),
    ];
    painter.add(Shape::closed_line(corners, Stroke::new(1.5, color)));
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use furnicraft_testkit::millimetres;
    use furnicraft_ui::DrawTool;

    use super::*;

    fn draft(tool: DrawTool, points: &[(i64, i64)]) -> DrawDraft {
        let mut draft = DrawDraft::new(tool);
        for (x, y) in points {
            draft.push([millimetres(*x), millimetres(*y)]).expect("a fresh point");
        }
        draft
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

    #[test]
    fn a_grid_click_and_a_free_click_produce_different_points() {
        let draft = DrawDraft::new(DrawTool::Polyline);
        let mut gizmo = DrawGizmo::new();
        assert!(gizmo.grid());
        gizmo.toggle_grid();
        assert!(!gizmo.grid());
        gizmo.toggle_grid();
        assert!(gizmo.grid());
        let _ = draft;
    }
}
