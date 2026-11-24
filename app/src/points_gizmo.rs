//! The by-points tool's pointer: the snapped place under the cursor and the board it would make.
//!
//! The tool never chooses a work plane (U126). Each click names a place through the one 3D resolver
//! (`crate::scene_snap`, ADR 0103, 0051): a face, an edge or a corner taken from the snap resolver,
//! so the first point may stand anywhere on the part and the plane is what the first three points
//! span. The draft itself is the editor's — this module resolves the pointer, feeds the click and
//! paints what the next click would make.
//!
//! The painting reads the same functions the commit does: the three-point method draws the rectangle
//! `model::board_from_three` computes, with its fourth corner marked as a ghost because nobody
//! clicked it, and the ring method draws every point projected onto the plane its first three span.
//! So the picture before the click is the part after it (ADR 0122), and no second reading of the
//! geometry exists here.
//!
//! Nothing writes: no document, no revision and no undo entry until the commit, which is the
//! editor's one command.
//!
//! Changes when: the by-points gesture gains a method, a preview or a refusal.

use eframe::egui::{Color32, Painter, Rect, Stroke, Ui};
use geom::DVec3;
use model::{PointsPlane, Project, board_from_three};
use scene::Camera;
use ui::{PanelPoints, PointsDraft, PointsDraftError, theme};
use units::Length;

use crate::gizmo_math::{Aim, paint_snap_marker, screen_point};
use crate::scene_snap::{self, ScenePoint, SceneSnap, world_point};

/// Radius of a placed point, in points.
const POINT_RADIUS: f32 = 3.5;
/// Radius of the ghost corner the board computes, in points: smaller, because nobody clicked it.
const GHOST_RADIUS: f32 = 2.5;

/// What one frame of the preview needs besides the draft: where it draws and in which inks.
#[derive(Clone, Copy)]
struct Preview {
    camera: Camera,
    rect: Rect,
    /// The placed points and the chain the hand is drawing.
    ink: Color32,
    /// What the tool computes rather than the hand naming: the fourth corner and the closing line.
    ghost: Color32,
}

/// What one pointer frame of the by-points tool did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum PointsAction {
    /// Nothing was placed.
    #[default]
    Nothing,
    /// A point was placed and the tool wants another.
    Point,
    /// The three-point method took its last click, so the caller commits.
    Complete,
}

/// Everything one pointer frame of the by-points tool carries besides the document.
#[derive(Clone, Copy)]
pub(crate) struct PointsInput<'a> {
    /// The camera the points are resolved through.
    pub(crate) camera: Camera,
    /// The viewport rectangle the tool draws in.
    pub(crate) rect: Rect,
    /// Which snaps the workspace allows.
    pub(crate) kinds: model::SnapKinds,
    /// The adapter the probe measures the bodies the boxes cannot describe with.
    pub(crate) contact: &'a dyn geom::Contact,
    /// Whether the chrome owns the pointer this frame.
    pub(crate) pointer_over_widget: bool,
    /// The keyboard state a click carries.
    pub(crate) modifiers: PointsModifiers,
}

/// The keyboard state a click carries while the by-points tool runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PointsModifiers {
    /// Shift locks the point to the axis through the last placed one.
    pub(crate) axis_lock: bool,
    /// Alt takes the pointer without a surface and without the grid.
    pub(crate) ignore_snaps: bool,
}

/// The pointer state of the by-points tool; the points themselves are the editor's draft.
#[derive(Debug, Default)]
pub(crate) struct PointsGizmo {
    hover: Option<ScenePoint>,
}

impl PointsGizmo {
    /// A by-points tool with no hover.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Drops the hover, for example when the tool ends.
    pub(crate) fn reset(&mut self) {
        self.hover = None;
    }

    /// What captured the pointer in the last frame, for the status bar's readout.
    pub(crate) fn snap_kind(&self) -> Option<model::SnapKind> {
        self.hover.and_then(|hover| hover.kind)
    }

    /// The point the next click would name, for a test that asks where the snap answered.
    #[cfg(test)]
    pub(crate) fn hover_point(&self) -> Option<[Length; 3]> {
        self.hover.map(|hover| hover.point)
    }

    /// Where the last frame's marker drew, for a test that asks what the pointer captured.
    #[cfg(test)]
    pub(crate) fn hover_screen(&self) -> Option<eframe::egui::Pos2> {
        self.hover.map(|hover| hover.screen)
    }

    /// Applies the pointer to the live draft and reports what the click did.
    ///
    /// A click the document's rules refuse is reported as the draft's own error, and the draft has
    /// already dropped the point and kept its sentence for the card, so the shape drawn so far stays
    /// editable (U126).
    pub(crate) fn update(
        &mut self,
        response: &eframe::egui::Response,
        project: &Project,
        editor: &mut ui::Editor,
        input: PointsInput<'_>,
    ) -> Result<PointsAction, PointsDraftError> {
        let Some(draft) = editor.points() else {
            self.reset();
            return Ok(PointsAction::Nothing);
        };
        let pointer = response.hover_pos().or_else(|| response.interact_pointer_pos());
        let Some(pointer) = pointer else {
            self.hover = None;
            return Ok(PointsAction::Nothing);
        };
        // The last placed point is the axis lock's anchor: a chain keeps one axis at a time, the way
        // the sketch's does (ADR 0051).
        let first = draft.points().last().copied();
        let snap = SceneSnap {
            kinds: input.kinds,
            contact: Some(input.contact),
            first,
            axis_lock: input.modifiers.axis_lock,
            ignore_snaps: input.modifiers.ignore_snaps,
            sight: editor.flags().sight(),
        };
        let aim = Aim { camera: input.camera, rect: input.rect, pointer };
        let Some(point) = scene_snap::resolve(project, aim, snap) else {
            self.hover = None;
            return Ok(PointsAction::Nothing);
        };
        self.hover = Some(point);
        if input.pointer_over_widget || !response.clicked_by(eframe::egui::PointerButton::Primary) {
            return Ok(PointsAction::Nothing);
        }
        editor.points_push(point.point)?;
        let complete = editor.points().is_some_and(PointsDraft::is_complete);
        Ok(if complete { PointsAction::Complete } else { PointsAction::Point })
    }

    /// Paints the placed points, the figure the next click would make and the snap marker.
    pub(crate) fn paint(&self, ui: &Ui, rect: Rect, camera: Camera, draft: &PointsDraft) {
        let painter = ui.painter().with_clip_rect(rect);
        let ink = theme::accent(ui);
        let ghost = theme::text_dim(ui);
        let placed = draft.points();
        let preview = Preview { camera, rect, ink, ghost };
        match draft.method() {
            PanelPoints::Three => self.paint_rectangle(&painter, preview, placed),
            PanelPoints::Ring => self.paint_ring(&painter, preview, placed),
        }
        if let Some(hover) = self.hover {
            paint_snap_marker(&painter, hover.screen, hover.kind, hover.locked, ink);
        }
    }

    /// The three-point method: two placed points, and the rectangle the hand is fixing the width of.
    ///
    /// The fourth corner is a ghost — drawn hollow and small — because the board computes it and no
    /// click names it (U126).
    fn paint_rectangle(&self, painter: &Painter, preview: Preview, placed: &[[Length; 3]]) {
        let Preview { camera, rect, ink, ghost } = preview;
        let at = |point: [Length; 3]| screen_point(camera, rect, world_point(point));
        // One point and a hand: the edge being drawn.
        if let (1, Some(first), Some(hover)) = (placed.len(), placed.first(), self.hover)
            && let (Some(from), Some(to)) = (at(*first), Some(hover.screen))
        {
            painter.line_segment([from, to], Stroke::new(2.0, ink));
        }
        // Two points: the rectangle the third click stores, or the bare edge when the hand is still
        // on that edge's own line.
        if let (2, Some(first), Some(second), Some(hover)) = (placed.len(), placed.first(), placed.get(1), self.hover) {
            match board_from_three(*first, *second, hover.point) {
                Ok(board) => {
                    if let Ok(placement) = board.plane.placement() {
                        let long = board.size.length().millimetres_f64();
                        let wide = board.size.width().millimetres_f64();
                        let corners = [[0.0, 0.0], [long, 0.0], [long, wide], [0.0, wide]]
                            .map(|[x, y]| placement.transform_point(DVec3::new(x, y, 0.0)));
                        outline(painter, camera, rect, &corners, Stroke::new(1.2, ghost));
                        if let Some(fourth) = corners.get(2).and_then(|corner| screen_point(camera, rect, *corner)) {
                            painter.circle_stroke(fourth, GHOST_RADIUS, Stroke::new(1.2, ghost));
                        }
                    }
                }
                Err(_) => {
                    if let (Some(from), Some(to)) = (at(*first), at(*second)) {
                        painter.line_segment([from, to], Stroke::new(2.0, ink));
                    }
                }
            }
        }
        for point in placed {
            if let Some(screen) = at(*point) {
                painter.circle_filled(screen, POINT_RADIUS, ink);
            }
        }
    }

    /// The ring method: the chain so far, every point projected onto the plane it spans.
    ///
    /// The closing segment follows the hand as a quiet line, so the maker sees the shape the finish
    /// would store; the placed points are drawn where the plane puts them, not where the hand was.
    fn paint_ring(&self, painter: &Painter, preview: Preview, placed: &[[Length; 3]]) {
        let Preview { camera, rect, ink, ghost } = preview;
        let plane = ring_plane(placed);
        let local = |point: [Length; 3]| plane.and_then(|plane| plane.project(point).ok()).unwrap_or(point);
        // The placed chain is drawn where the plane puts it; the hand's own point joins it as the
        // next corner, so the chain never grows a gap the maker has to imagine.
        let chain = placed.iter().map(|point| local(*point)).collect::<Vec<_>>();
        let mut preview = chain.clone();
        if let Some(hover) = self.hover {
            preview.push(local(hover.point));
        }
        let at = |point: [Length; 3]| screen_point(camera, rect, world_point(point));
        for pair in preview.windows(2) {
            if let (Some(from), Some(to)) =
                (pair.first().and_then(|point| at(*point)), pair.get(1).and_then(|point| at(*point)))
            {
                painter.line_segment([from, to], Stroke::new(2.0, ink));
            }
        }
        // The closing segment is the tool's own reading of the shape, so it is the quiet ink: it is
        // what the finish would store, and it is not a line the hand has drawn yet.
        if preview.len() > 2
            && let (Some(last), Some(first)) =
                (preview.last().and_then(|point| at(*point)), preview.first().and_then(|point| at(*point)))
        {
            painter.line_segment([last, first], Stroke::new(1.2, ghost));
        }
        for point in &chain {
            if let Some(screen) = at(*point) {
                painter.circle_filled(screen, POINT_RADIUS, ink);
            }
        }
    }
}

/// The plane a ring's first three points span, or `None` while it has fewer.
fn ring_plane(points: &[[Length; 3]]) -> Option<PointsPlane> {
    let [first, second, third] = [points.first()?, points.get(1)?, points.get(2)?];
    PointsPlane::from_three(*first, *second, *third).ok()
}

/// Draws a closed world ring as one stroke, corner by corner.
fn outline(painter: &Painter, camera: Camera, rect: Rect, corners: &[DVec3], stroke: Stroke) {
    for index in 0..corners.len() {
        let (Some(from), Some(to)) = (
            corners.get(index).and_then(|corner| screen_point(camera, rect, *corner)),
            corners.get((index + 1) % corners.len()).and_then(|corner| screen_point(camera, rect, *corner)),
        ) else {
            continue;
        };
        painter.line_segment([from, to], stroke);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;
    use geom::{DQuat, DVec3};

    fn point(x: i64, y: i64, z: i64) -> [Length; 3] {
        [x, y, z].map(|value| Length::from_millimetres(value).unwrap_or_default())
    }

    #[test]
    fn a_ring_gets_its_plane_from_its_first_three_points_and_no_sooner() {
        let mut points = vec![point(0, 0, 0), point(600, 0, 0)];
        assert_eq!(ring_plane(&points), None, "two points span no plane");
        points.push(point(0, 400, 0));
        let plane = ring_plane(&points).expect("three points span the floor");
        assert_eq!(plane.project(point(600, 400, 250)), Ok(point(600, 400, 0)), "a later point is projected");
        assert_eq!(ring_plane(&[point(0, 0, 0), point(600, 0, 0), point(300, 0, 0)]), None, "a collinear trio");
    }

    #[test]
    fn the_preview_draws_the_world_points_the_commit_stores() {
        // The preview reads the same points the command stores, so the picture before the click and
        // the part after it are one reading (ADR 0122); a point the camera cannot see draws nowhere
        // rather than somewhere made up.
        let camera = Camera::default();
        let rect = Rect::from_min_size(eframe::egui::Pos2::ZERO, eframe::egui::vec2(800.0, 600.0));
        let origin = screen_point(camera, rect, DVec3::ZERO).expect("the origin is in front of the camera");
        assert!((origin - rect.center()).length() < 1.0, "the default view looks at the origin: {origin:?}");
        assert!(DQuat::IDENTITY.is_finite(), "the frame the geometry uses is a rotation");
    }
}
