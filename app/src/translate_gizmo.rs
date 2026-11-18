//! The viewport translate gizmo.
//!
//! Three world-axis handles are projected over the selection's manipulation pivot, the stored pivot
//! of its primary node. A drag never mutates the document: it moves a preview of every selected
//! panel, and the release dispatches one `node.placement.set_many`. Each node's world-space delta is
//! converted into its own parent frame and rounded to micrometres, so a selection whose nodes have
//! different parents still moves rigidly and the inspector agrees with the document after the drag.
//!
//! A furniture drag is cut by the document's own rule (`Project::move_rule`): a floor-bound product
//! keeps its height, a shell stops at another shell and a body at another body, and the moved box
//! lands on a coordinate plane, on a neighbouring face or on the 1 mm grid (ADR 0102, ADR 0103). What
//! the drag snaps to, and how near, is the workspace's `SnapSettings`, read once when the drag
//! starts (ADR 0146). The rule is applied to the preview, and the release commits that same
//! distance, so a drag is one undo entry and the preview is not a promise the command breaks.
//!
//! The two tools are held differently (N22, N24, ADR 0147, ADR 0151). The free `Move` keeps its three
//! arrows: a press on a handle and the release are one drag, and the pointer's own micrometre
//! distance applies no rule and no snap at all. Handles keep a constant screen length, and an axis
//! within about ten degrees of the view direction is not drawn, because its projected direction is
//! unstable and a pointer drag would move it by metres.
//!
//! The furniture move draws **no handle at all**: a click on a piece's body takes it, the pointer
//! carries it with no button held, and the next click puts it down. What it may travel is the
//! document's answer: a panel offers only the axis its own thickness points at, so a shelf rises and
//! falls in its thickness direction and never across its own faces. The axis is the allowed one the
//! hand's travel names best, latched at the first movement so the piece cannot change direction under
//! the hand. Both tools keep the exact-distance fields, which are the typed path to the same distance
//! (ADR 0103).

use eframe::egui;
use egui::{Response, Ui};
use geom::DVec3;
use model::{Capture, Id, MoveRule};
use scene::{Camera, MAX_DISTANCE_MM};

use crate::gizmo_math::{
    Axis, HandleSpec, MoveKind, MoveTarget, PlacementCommit, Preview, PreviewKey, Target, commit_translation, hit_axis,
    named_axis, paint_axis_handles, ray_to, screen_point, travel_frame,
};

/// How near the pointer has to be to the place the carry asked for to count as arrived.
///
/// A window rounds a cursor position to whole points and a display may scale it, so the answer is
/// never exact; a couple of points is well inside the grab of a piece.
const HOME_TOLERANCE: f32 = 3.0;

/// How many frames the carry waits for the pointer to come back before it gives up and carries on.
///
/// A platform that cannot move the pointer never answers; eight frames is long enough for the one
/// that can, and short enough that the piece does not stand still under a hand that is moving.
const HOME_TRIES: u8 = 8;

/// The smallest travel, in points, that names an axis.
///
/// Below it the hand has not said which way the piece goes, and a piece that picked a direction from
/// the noise of a still pointer would jump on the frame it was taken.
const NAMING_TRAVEL: f32 = 4.0;

/// How a hand holds the piece a drag moves.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Hold {
    /// The free move holds one arrow with the primary button: the axis is known at the press and the
    /// release commits the distance under the pointer.
    Handle(Axis),
    /// The furniture move holds nothing: a click took the piece, the pointer carries it, and the
    /// next click puts it down (ADR 0151).
    Following {
        /// The axis the travel named, once the pointer moved far enough for the hand to say so.
        axis: Option<Axis>,
        /// The bond between the pointer and the piece it hangs from itself.
        hand: Hand,
    },
}

/// The screen bond between the hand and the piece it carries.
///
/// The pointer *hangs* the piece from itself: the pivot keeps the offset on screen that the click
/// gave it, and the distance along the axis is solved for that offset every frame. What the distance
/// is deliberately not:
///
/// - the projection of the pointer's ray onto the axis line, which is unstable a hundred points off
///   the line and made the piece run away from the hand (ADR 0153);
/// - anything the camera did. Moving the view must not move a piece — a panel set 200 mm from its
///   neighbour stays 200 mm away while the maker pans, orbits or zooms. So the view holds the piece
///   and the *cursor* comes back to it instead: a maker carrying a piece cannot look around without
///   letting go of the pointer, and the pointer is what returns to the piece (X67, ADR 0156).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Hand {
    /// The pointer that took the piece, which the axis is named from.
    from: egui::Pos2,
    /// The pivot's offset from that pointer, in points, once the axis is named.
    ///
    /// The click lands on the body, so the pivot is not under the pointer; snapping it there would
    /// jump the piece by its own half-size on the frame the axis is named.
    offset: Option<egui::Vec2>,
}

impl Hold {
    /// The axis this hold moves along, while it has one.
    fn axis(self) -> Option<Axis> {
        match self {
            Self::Handle(axis) => Some(axis),
            Self::Following { axis, .. } => axis,
        }
    }

    /// Whether the piece is carried without a button, which is the furniture move.
    fn follows(self) -> bool {
        matches!(self, Self::Following { .. })
    }
}

/// One continuous move: one handle drag, or one piece carried by the pointer.
#[derive(Debug)]
struct Drag {
    node: Id,
    hold: Hold,
    /// The furniture rule this drag was measured against when it started, if any.
    rule: Option<MoveRule>,
    /// The surface the face landed on this frame, when a snap — and not the pointer — placed it.
    capture: Option<Capture>,
    drawables: Vec<Id>,
    targets: Vec<MoveTarget>,
    start_origin: DVec3,
    /// The pointer's parameter along the latched axis the distance is measured from.
    ///
    /// It is set when the piece is taken and re-anchored whenever the view moves under the hand, so
    /// the piece keeps the distance it already has and the camera never drags it along (N25).
    start_parameter: f64,
    delta: DVec3,
    /// The document the targets and the rule were read from.
    ///
    /// A command that lands under the hand makes both stale, so the hold ends rather than committing
    /// a distance measured against a scene that is no longer there (N25).
    revision: model::Revision,
}

/// Persistent state of the translate gizmo.
#[derive(Debug, Default)]
pub(crate) struct TranslateGizmo {
    drag: Option<Drag>,
    hover: Option<Axis>,
    consumed_click: bool,
    /// Where a click took a piece, until the next frame builds the hold from it.
    ///
    /// The click that takes a piece is read by the shell, after this gizmo has already run for the
    /// frame: it names the pointer here and the hold starts on the next frame, from that point, so
    /// the piece is measured where the hand took it and not where the pointer has drifted to.
    request: Option<egui::Pos2>,
    /// The camera and the viewport the last frame drew through.
    ///
    /// The carry compares them frame by frame: a camera that changed under a piece means the piece
    /// stayed where it is (X67) and the pointer has to come back to it (ADR 0156).
    last_view: Option<(Camera, egui::Rect)>,
    /// Whether a camera gesture moved the view under the piece, which the carry owes the pointer a
    /// return for once the gesture ends.
    moved_by_gesture: bool,
    /// Where the carry wants the pointer, while it waits for the window to put it there.
    ///
    /// A camera gesture moves the view and not the piece, so the cursor is left behind by the whole
    /// gesture. The carry asks the window to put it back on the piece instead of dragging the piece
    /// to the cursor. The count is the frames the request has gone unanswered: a platform that cannot
    /// move the pointer (a Wayland session, a window that refuses) falls back to carrying from where
    /// the pointer is rather than waiting forever.
    home: Option<(egui::Pos2, u8)>,
}

impl TranslateGizmo {
    /// Whether a drag is in progress: a handle held, or a piece carried by the pointer.
    pub(crate) fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// Whether a piece is carried by the pointer rather than by a held handle.
    pub(crate) fn is_following(&self) -> bool {
        self.drag.as_ref().is_some_and(|drag| drag.hold.follows())
    }

    /// Whether a physical button holds the drag, which is the one state that freezes the camera.
    ///
    /// A carried piece is not held: the viewport keeps orbiting, panning and zooming under it, and
    /// the hold re-anchors to whatever the view became (N25).
    pub(crate) fn is_held(&self) -> bool {
        self.drag.as_ref().is_some_and(|drag| !drag.hold.follows())
    }

    /// Drops the hold without writing anything, so the piece goes back where it was taken.
    ///
    /// The shell calls this when a tool takes the pointer: the furniture move blocks nothing, and a
    /// piece in hand is not a mode the rest of the editor has to wait for (N25).
    pub(crate) fn cancel(&mut self) {
        self.drag = None;
        self.request = None;
    }

    /// Takes the piece under `pointer` in hand, for the furniture move to carry (ADR 0151).
    ///
    /// The shell calls this after the click that named the piece has selected it; the hold itself
    /// starts on the next frame, from this point. A request that no furniture move picks up is
    /// dropped with the mode, so a stray click never arms a piece much later.
    pub(crate) fn grab(&mut self, pointer: egui::Pos2) {
        self.request = Some(pointer);
    }

    /// Where the carry asks the window to put the pointer, until it gets there (ADR 0156).
    ///
    /// The shell sends it as a viewport command every frame it is asked for, because a window may
    /// drop one; a platform that refuses it leaves the carry to the frames counter.
    pub(crate) fn cursor_home(&self) -> Option<egui::Pos2> {
        self.home.map(|(at, _)| at)
    }

    /// Whether this frame's pointer release belongs to a gizmo drag and must not select.
    pub(crate) fn consumed_click(&self) -> bool {
        self.consumed_click
    }

    /// The drawable ids and world delta the renderer should preview, if a drag moved something.
    pub(crate) fn preview(&self) -> Option<Preview<'_>> {
        let drag = self.drag.as_ref()?;
        if drag.delta == DVec3::ZERO {
            return None;
        }
        Some(Preview::Translation { drawables: &drag.drawables, delta: drag.delta })
    }

    /// The preview identity used to avoid re-uploading an unchanged drag.
    pub(crate) fn preview_key(&self) -> Option<PreviewKey> {
        let drag = self.drag.as_ref()?;
        if drag.delta == DVec3::ZERO {
            return None;
        }
        Some(PreviewKey::Translation {
            node: drag.node,
            delta: [drag.delta.x.to_bits(), drag.delta.y.to_bits(), drag.delta.z.to_bits()],
        })
    }

    /// The world axis of the drag in progress, if any.
    ///
    /// A carried piece has none until the hand's travel names one, which is why the exact-distance
    /// fields appear on the frame the piece starts to move and not on the frame it is taken.
    pub(crate) fn axis(&self) -> Option<Axis> {
        self.drag.as_ref().and_then(|drag| drag.hold.axis())
    }

    /// The node whose pivot anchors the drag.
    pub(crate) fn anchor(&self) -> Option<Id> {
        self.drag.as_ref().map(|drag| drag.node)
    }

    /// The world distance the drag has travelled so far.
    pub(crate) fn drag_delta(&self) -> DVec3 {
        self.drag.as_ref().map_or(DVec3::ZERO, |drag| drag.delta)
    }

    /// The surface this frame's drag landed on, when a snap placed it (ADR 0146).
    ///
    /// The shell draws the line where that surface stands and measures the drag's fields from it,
    /// so the picture, the numbers and the release name one surface.
    pub(crate) fn capture(&self) -> Option<Capture> {
        self.drag.as_ref().and_then(|drag| drag.capture)
    }

    /// Whether the drag's axis may carry a typed distance.
    ///
    /// A floor-bound product keeps its height and a panel never leaves its own thickness axis, so an
    /// axis the tool does not offer has no free distance to state; the fields are not offered there.
    pub(crate) fn exact_axis(&self) -> bool {
        let Some(drag) = self.drag.as_ref() else { return false };
        let Some(axis) = drag.hold.axis() else { return false };
        drag.rule.as_ref().is_none_or(|rule| rule.allows(axis.move_axis()))
    }

    /// Starts, continues or finishes a drag and returns a placement to dispatch on release.
    ///
    /// `pointer_over_widget` is true when the pointer is over the orientation widget, which owns
    /// its own clicks. The caller must run this before camera orbit and discard the pointer's click
    /// when [`Self::consumed_click`] is true.
    ///
    /// `pinned` is a distance the exact-distance fields fixed in world millimetres. It replaces the
    /// pointer for the frame, so typing a number takes the mouse out of the drag without ending it,
    /// and the release still commits the pinned distance as the one undo entry (ADR 0103).
    ///
    /// `handle_length` is the screen length of one handle, in points, from the workspace settings
    /// (U86); the drawing and the hit test are handed the same value.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn update(
        &mut self,
        ui: &Ui,
        response: &Response,
        viewport: egui::Rect,
        target: Target<'_>,
        pointer_over_widget: bool,
        pinned: Option<f64>,
        handle_length: f32,
    ) -> Result<Option<PlacementCommit>, String> {
        self.consumed_click = false;
        let outcome = if let Some(drag) = self.drag.take() {
            match drag.hold {
                Hold::Handle(axis) => self.continue_handle(ui, viewport, target, drag, axis, pinned),
                Hold::Following { .. } => self.continue_follow(ui, response, viewport, target, drag, pinned),
            }
        } else {
            self.begin(ui, response, viewport, target, pointer_over_widget, handle_length)
        };
        // The next frame compares the camera against this one, so the view of the frame that was
        // just drawn is the one it must keep the piece in.
        self.last_view = Some((target.camera, viewport));
        outcome
    }

    /// Carries on a held handle: the pointer's distance along the axis, cut by the rule, and the
    /// release is the commit.
    #[allow(clippy::too_many_arguments)]
    fn continue_handle(
        &mut self,
        ui: &Ui,
        viewport: egui::Rect,
        target: Target<'_>,
        mut drag: Drag,
        axis: Axis,
        pinned: Option<f64>,
    ) -> Result<Option<PlacementCommit>, String> {
        let (pointer, down, escape) = ui.input(|input| {
            (input.pointer.latest_pos(), input.pointer.primary_down(), input.key_pressed(egui::Key::Escape))
        });
        // Escape belongs to the field while a number is pinned: it returns the mouse first, and the
        // next press leaves the drag.
        if escape && pinned.is_none() {
            self.consumed_click = true;
            return Ok(None);
        }
        let (distance, capture) = match pinned {
            // A typed distance is the maker's own number, so no snap moves it.
            Some(pinned) => (pinned.is_finite().then_some(pinned), None),
            None => pointer
                .and_then(|pointer| {
                    ray_to(pointer, viewport, target.camera)
                        .closest_parameter_on_line(drag.start_origin, axis.direction())
                })
                .map(|parameter| {
                    let scalar = (parameter - drag.start_parameter).clamp(-MAX_DISTANCE_MM, MAX_DISTANCE_MM);
                    constrained_distance(drag.rule.as_ref(), axis, scalar)
                })
                .map_or((None, None), |(distance, capture)| (Some(distance), capture)),
        };
        if let Some(distance) = distance.filter(|distance| distance.is_finite()) {
            drag.delta = axis.direction() * distance;
            drag.capture = capture;
        }
        if !down {
            self.consumed_click = true;
            let commit = commit_translation(&drag.targets, drag.delta)?;
            return Ok(commit);
        }
        self.drag = Some(drag);
        Ok(None)
    }

    /// Carries a piece the furniture move took: the pointer moves it, a click puts it down and
    /// Escape puts it back (ADR 0151).
    ///
    /// No button is held here, so the hold ends on a *click* — press and release at one place — and
    /// not on a release: the press that drops the piece is the gizmo's and never the pick's, which
    /// [`Self::consumed_click`] reports to the shell.
    fn continue_follow(
        &mut self,
        ui: &Ui,
        response: &Response,
        viewport: egui::Rect,
        target: Target<'_>,
        mut drag: Drag,
        pinned: Option<f64>,
    ) -> Result<Option<PlacementCommit>, String> {
        // The mode is the window's one value, and it is read again here: a piece in hand whose tool
        // is switched under it goes back where it was taken, because nothing said where it goes.
        if target.move_kind != MoveKind::Furniture {
            return Ok(None);
        }
        let (pointer, escape) = ui.input(|input| (input.pointer.latest_pos(), input.key_pressed(egui::Key::Escape)));
        // Escape belongs to the field while a number is pinned, as it does in a held drag: the first
        // press returns the pointer and the next one puts the piece back.
        if escape && pinned.is_none() {
            self.consumed_click = true;
            return Ok(None);
        }
        // A command that landed under the hand — an undo, a delete, a rename from the tree — moved
        // the document the targets and the rule were read from, so the piece goes back instead of
        // committing a distance measured against a scene that is no longer there (N25).
        if target.project.revision() != drag.revision {
            return Ok(None);
        }
        // The piece is put down by a click whether or not the hand ever named a direction: a piece
        // taken and put back without travelling is no move, and the click still ends the hold.
        if response.clicked_by(egui::PointerButton::Primary) {
            self.consumed_click = true;
            let commit = commit_translation(&drag.targets, drag.delta)?;
            return Ok(commit);
        }
        let Hold::Following { axis: held, hand } = drag.hold else {
            return Ok(None);
        };
        let axes = target.move_kind.axes(target.project, target.selection);
        // The axis is named once, by the first movement of the hand past a stray point, and kept: a
        // piece that re-decided every frame would flip near 45° and its distance would restart.
        let axis = match held {
            Some(axis) => Some(axis),
            None => pointer.and_then(|pointer| {
                let named = pointer - hand.from;
                (named.length() >= NAMING_TRAVEL)
                    .then(|| named_axis(target.camera, viewport, drag.start_origin, named, axes))
                    .flatten()
                    .map(|handle| handle.axis)
            }),
        };
        let Some(axis) = axis else {
            // The hand has not said which way yet, so the piece stays where it was taken.
            self.drag = Some(drag);
            return Ok(None);
        };
        let Some(pointer) = pointer else {
            self.drag = Some(drag);
            return Ok(None);
        };
        // A camera gesture is the eye's: the view slides under the piece and the piece keeps its
        // place — a panel set 200 mm from its neighbour stays 200 mm away while the maker pans
        // (X67). The document hides the pointer for the length of the gesture and asks it back onto
        // the piece once the gesture is over (ADR 0156).
        let navigating = ui.input(|input| {
            input.pointer.middle_down()
                || input.pointer.secondary_down()
                || input.smooth_scroll_delta != egui::Vec2::ZERO
        });
        let camera_moved = self.last_view.is_some_and(|(camera, _)| camera != target.camera);
        // The pointer hangs the piece from itself: the pivot keeps the offset on screen the click
        // gave it. The click lands on the body, so the pivot is not under the pointer; snapping it
        // there would jump the piece by its own half-size on the frame the axis is named.
        let pivot = drag.start_origin + drag.delta;
        let mut offset = hand.offset;
        if offset.is_none() {
            offset = screen_point(target.camera, viewport, pivot).map(|at| at - pointer);
        }
        // The pointer comes back to the piece after the camera moved, and the piece is held while the
        // window carries the request out: dragging a piece to a pointer nobody moved is what tore the
        // piece off the hand in the first place (X67, ADR 0156).
        let waiting = match self.home {
            // The pointer arrived where the carry asked for it: the hand is on the piece again, and
            // the offset is re-read here so the frame itself carries nothing.
            Some((want, _)) if (pointer - want).length() <= HOME_TOLERANCE => {
                self.home = None;
                offset = screen_point(target.camera, viewport, pivot).map(|at| at - want);
                false
            }
            // A window that cannot move the pointer never answers: the carry resumes from the
            // pointer where it is, which is a carry that keeps the piece's metres and re-anchors.
            Some((_, tries)) if tries >= HOME_TRIES => {
                self.home = None;
                offset = screen_point(target.camera, viewport, pivot).map(|at| at - pointer);
                false
            }
            Some((want, tries)) => {
                offset = screen_point(target.camera, viewport, pivot).map(|at| at - want);
                self.home = Some((want, tries + 1));
                true
            }
            None => false,
        };
        // A camera that moved under a gesture has to be remembered: the frame that would ask for the
        // pointer is exactly the frame the gesture is still running in, and by the frame it ends the
        // camera has stopped moving and nothing would ask at all.
        if navigating && camera_moved {
            self.moved_by_gesture = true;
        }
        // The gesture is over and the camera moved under the piece: ask the pointer back onto the
        // middle of the piece, which is where the maker reaches for it. A piece the new view pushed
        // out of the window has nowhere to be asked for, so the carry keeps its distance and the
        // hand carries on from the pointer where it stands.
        if !waiting && !navigating && (camera_moved || self.moved_by_gesture) {
            self.moved_by_gesture = false;
            // The middle of the piece as it stands in the hand, not as the document still has it:
            // the preview is where the maker is looking.
            let middle =
                target.project.world_box(drag.node).map_or(pivot, |(low, high)| (low + high) * 0.5 + drag.delta);
            match screen_point(target.camera, viewport, middle) {
                Some(want) if viewport.contains(want) => self.home = Some((want, 0)),
                _ => self.home = None,
            }
        }
        // The piece is held — it keeps its own distance — while the eye is moving and while the
        // pointer is on its way back to it. Only a hand that is on the piece moves it.
        let held = waiting || navigating || self.home.is_some();
        let hand = Hand { from: hand.from, offset };
        let carried = match (offset, pinned) {
            // A typed distance is the maker's own number on the carried piece too.
            (_, Some(pinned)) => pinned.is_finite().then_some(pinned),
            (Some(offset), None) if !held => self.hangs_from(
                viewport,
                target.camera,
                drag.start_origin,
                axis,
                pointer + offset,
                drag.delta.dot(axis.direction()),
            ),
            _ => None,
        };
        let (distance, capture) = carried
            .map(|scalar| scalar.clamp(-MAX_DISTANCE_MM, MAX_DISTANCE_MM))
            .map(|scalar| constrained_distance(drag.rule.as_ref(), axis, scalar))
            .map_or((None, None), |(distance, capture)| (Some(distance), capture));
        if let Some(distance) = distance.filter(|distance| distance.is_finite()) {
            drag.delta = axis.direction() * distance;
            drag.capture = capture;
        }
        drag.hold = Hold::Following { axis: Some(axis), hand };
        self.drag = Some(drag);
        Ok(None)
    }

    /// The distance along the axis that puts the carried piece's pivot on `want`, in viewport points.
    ///
    /// The screen place is the invariant, not the millimetres: a camera gesture, a zoom or a view
    /// preset moves the projection, and the piece slides along its axis to stay where the hand holds
    /// it. The answer is solved for, because a projection is not linear in depth: each step measures
    /// the error the piece has left on screen and takes it out with the axis' own scale there. An
    /// axis that points at the camera has no screen place to keep, so the distance is left as it is.
    ///
    /// `guess` is where the piece stands, which the first step corrects: a frame that only moved the
    /// hand is a step of a few points, and a frame that moved the camera may be a step of hundreds.
    fn hangs_from(
        &self,
        viewport: egui::Rect,
        camera: Camera,
        origin: DVec3,
        axis: Axis,
        want: egui::Pos2,
        guess: f64,
    ) -> Option<f64> {
        let mut distance = guess;
        for _ in 0..8 {
            let at = screen_point(camera, viewport, origin + axis.direction() * distance)?;
            let error = want - at;
            if error.length() < 0.25 {
                break;
            }
            let (direction, per_point) = travel_frame(camera, viewport, origin + axis.direction() * distance, axis)?;
            let step = f64::from(error.dot(direction)) * per_point;
            if !step.is_finite() {
                return None;
            }
            distance = (distance + step).clamp(-MAX_DISTANCE_MM, MAX_DISTANCE_MM);
        }
        distance.is_finite().then_some(distance)
    }

    /// Starts a hold: a handle taken by a press, or a piece taken by a click on its body.
    ///
    /// `pointer_over_widget` is true when the pointer is over the orientation widget, which owns its
    /// own clicks.
    fn begin(
        &mut self,
        ui: &Ui,
        response: &Response,
        viewport: egui::Rect,
        target: Target<'_>,
        pointer_over_widget: bool,
        handle_length: f32,
    ) -> Result<Option<PlacementCommit>, String> {
        self.hover = None;
        // A click that took a piece is read by the shell after this gizmo has run for its frame, so
        // the hold starts here on the next one. A request the furniture move no longer owns is
        // dropped, so a click never arms a piece much later.
        let request = self.request.take();
        if pointer_over_widget {
            return Ok(None);
        }
        let targets = target.move_targets();
        let Some(anchor) = targets.last() else {
            return Ok(None);
        };
        let Some(origin_world) = target.project.pivot_world(anchor.node) else {
            return Ok(None);
        };

        if target.move_kind == MoveKind::Furniture {
            let Some(at) = request else {
                return Ok(None);
            };
            let moving: Vec<Id> = targets.iter().map(|target| target.node).collect();
            // The rule and the snap settings are read once, when the piece is taken, exactly as a
            // held drag reads them at the press: the preview and the drop then measure one document.
            let rule = target.project.move_rule(&moving, target.snaps);
            // The grab is measured against the click and not against the frame the axis is named:
            // by then the hand has already travelled far enough to name one, and an offset read
            // there would swallow that travel and leave the piece standing still (X67).
            let offset = screen_point(target.camera, viewport, origin_world).map(|pivot| pivot - at);
            let hand = Hand { from: at, offset };
            self.drag = Some(Drag {
                node: anchor.node,
                hold: Hold::Following { axis: None, hand },
                rule,
                capture: None,
                drawables: target.drawables(),
                targets,
                start_origin: origin_world,
                start_parameter: 0.0,
                delta: DVec3::ZERO,
                revision: target.project.revision(),
            });
            return Ok(None);
        }

        let Some(pointer) = response.hover_pos() else {
            return Ok(None);
        };
        let spec = HandleSpec { axes: target.move_kind.axes(target.project, target.selection), length: handle_length };
        let Some(axis) = hit_axis(target.camera, viewport, origin_world, pointer, spec) else {
            return Ok(None);
        };
        self.hover = Some(axis);
        if !ui.input(|input| input.pointer.primary_pressed()) {
            return Ok(None);
        }

        let Some(parameter) =
            ray_to(pointer, viewport, target.camera).closest_parameter_on_line(origin_world, axis.direction())
        else {
            return Ok(None);
        };

        let moving: Vec<Id> = targets.iter().map(|target| target.node).collect();
        // The free move applies no rule at all: the furniture snapping of ADR 0146 is the other
        // tool's, and a loose body keeps the pointer's own distance wherever it goes (N22).
        let rule = match target.move_kind {
            MoveKind::Free => None,
            MoveKind::Furniture => target.project.move_rule(&moving, target.snaps),
        };
        self.drag = Some(Drag {
            node: anchor.node,
            hold: Hold::Handle(axis),
            rule,
            capture: None,
            drawables: target.drawables(),
            targets,
            start_origin: origin_world,
            start_parameter: parameter,
            delta: DVec3::ZERO,
            revision: target.project.revision(),
        });
        if !ui.input(|input| input.pointer.primary_down()) {
            self.drag = None;
            self.consumed_click = true;
        }
        Ok(None)
    }

    /// Draws the handles over the viewport.
    ///
    /// The furniture move draws none: a piece is taken by any part of its body, so an arrow over it
    /// would name a target the tool does not have (ADR 0151).
    pub(crate) fn paint(&self, ui: &Ui, viewport: egui::Rect, target: Target<'_>, handle_length: f32) {
        if target.move_kind == MoveKind::Furniture {
            return;
        }
        let origin_world = if let Some(drag) = &self.drag {
            drag.start_origin + drag.delta
        } else {
            let Some(origin) = target.anchor_pivot() else {
                return;
            };
            origin
        };
        paint_axis_handles(
            ui,
            viewport,
            target.camera,
            origin_world,
            self.hover,
            self.drag.as_ref().and_then(|drag| drag.hold.axis()),
            HandleSpec { axes: target.move_kind.axes(target.project, target.selection), length: handle_length },
        );
    }
}

/// The distance a drag may travel, and the surface it landed on: the pointer's own distance, cut by
/// the furniture move rule.
///
/// The rule, and the snap settings inside it, were read from the workspace and the document when the
/// drag started and the document still holds those placements, so the preview and the release read
/// the same answer. A drag outside furniture has no rule and keeps its exact micrometre distance.
fn constrained_distance(rule: Option<&MoveRule>, axis: Axis, distance: f64) -> (f64, Option<Capture>) {
    rule.map_or((distance, None), |rule| {
        let limit = rule.limit(axis.move_axis(), distance);
        (limit.distance_mm, limit.capture)
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use std::f64::consts::FRAC_PI_2;

    use egui::Pos2;
    use geom::DQuat;
    use model::{ClassCode, Envelope, Placement, SnapSettings};
    use testkit::{millimetres, sample_project};
    use units::Length;

    use super::*;
    use crate::gizmo_math::move_targets;

    fn viewport() -> egui::Rect {
        egui::Rect::from_min_size(Pos2::ZERO, egui::Vec2::new(800.0, 600.0))
    }

    fn origin_screen(camera: Camera) -> Pos2 {
        crate::gizmo_math::screen_point(camera, viewport(), camera.target()).expect("the target is in front")
    }

    fn move_target(node: u64, parent_rotation: DQuat, start_local: Placement) -> MoveTarget {
        MoveTarget {
            node: Id::new(node).expect("id"),
            start_local,
            parent: Placement::new(DVec3::ZERO, parent_rotation).expect("placement"),
            pivot_local: DVec3::ZERO,
        }
    }

    #[test]
    fn a_world_delta_is_converted_into_a_rotated_parent_frame() {
        let target = move_target(2, DQuat::from_rotation_z(FRAC_PI_2), Placement::IDENTITY);
        let commit =
            commit_translation(&[target], DVec3::new(10.0, 0.0, 0.0)).expect("valid placement").expect("a move");
        assert_eq!(commit.changes.len(), 1);
        let change = commit.changes[0];
        let expected = DVec3::new(0.0, -10.0, 0.0);
        assert!(
            (change.placement.translation() - expected).length() < 1.0e-9,
            "translation was {:?}",
            change.placement.translation()
        );
        assert_eq!(change.placement.rotation(), Placement::IDENTITY.rotation());
    }

    #[test]
    fn a_batch_moves_a_selection_rigidly_across_different_parents() {
        let turned = Placement::new(DVec3::new(500.0, 0.0, 0.0), DQuat::from_rotation_z(FRAC_PI_2)).expect("placement");
        let targets = [
            move_target(2, DQuat::IDENTITY, Placement::IDENTITY),
            move_target(
                3,
                turned.rotation(),
                Placement::new(DVec3::new(1.0, 2.0, 3.0), DQuat::IDENTITY).expect("placement"),
            ),
        ];
        let commit = commit_translation(&targets, DVec3::new(10.0, 4.0, 0.0)).expect("valid").expect("a move");
        assert_eq!(commit.changes.len(), 2);
        assert_eq!(commit.changes[0].node, Id::new(2).expect("id"));
        assert_eq!(commit.changes[1].node, Id::new(3).expect("id"));

        // Every node receives the same world delta, expressed in its own parent frame.
        for (change, target) in commit.changes.iter().zip(targets) {
            let world_before = target.parent.transform_point(target.start_local.translation());
            let world_after = target.parent.transform_point(change.placement.translation());
            assert!(
                (world_after - world_before - DVec3::new(10.0, 4.0, 0.0)).length() < 1.0e-9,
                "world shift was {:?}",
                world_after - world_before
            );
        }
    }

    #[test]
    fn a_zero_delta_or_a_rounded_out_delta_dispatches_nothing() {
        let target = move_target(2, DQuat::IDENTITY, Placement::IDENTITY);
        assert_eq!(commit_translation(&[target], DVec3::ZERO).expect("no error"), None);
        assert_eq!(commit_translation(&[target], DVec3::new(1.0e-7, 0.0, 0.0)).expect("no error"), None);
        assert_eq!(commit_translation(&[], DVec3::new(1.0, 0.0, 0.0)).expect("no error"), None);
    }

    /// A `600 x 560 x 720` millimetre base cabinet at `x` millimetres, on the floor.
    fn add_cabinet(project: &mut model::Project, x: i64) -> Id {
        let root = project.root().id();
        let envelope = Envelope::new(millimetres(600), millimetres(560), millimetres(720)).expect("a valid envelope");
        let placement = Placement::IDENTITY
            .with_translation(millimetres(x), Length::ZERO, Length::ZERO)
            .expect("a floor placement");
        let class: ClassCode = "10.01.01".parse().expect("a base cabinet");
        project.add_product(root, "Cabinet", class, envelope, placement).expect("a product")
    }

    #[test]
    fn a_furniture_drag_stops_at_contact_and_the_release_lands_there() {
        let mut project = sample_project();
        let cabinet = add_cabinet(&mut project, 0);
        let neighbour = add_cabinet(&mut project, 1000);
        // The pointer pulls 1000 mm, the neighbour's wall is at 1000: the drag keeps 400.
        let rule = project.move_rule(&[cabinet], SnapSettings::default()).expect("a furniture drag");
        let (distance, capture) = constrained_distance(Some(&rule), Axis::X, 1000.0);
        assert_eq!(distance, 400.0);
        assert_eq!(capture, None, "a stop is the contact's, not a snap's");

        // The release commits exactly the previewed distance, so the preview is no lie.
        let targets = move_targets(&project, &[cabinet]);
        let commit = commit_translation(&targets, Axis::X.direction() * distance)
            .expect("a valid placement")
            .expect("the drag moved the cabinet");
        assert_eq!(commit.changes.len(), 1, "one drag is one placement and one undo entry");
        let moved = commit.changes[0].placement.translation();
        assert!((moved - DVec3::new(400.0, 0.0, 0.0)).length() < 1.0e-9, "was {moved:?}");
        assert!((moved.x + 600.0 - 1000.0).abs() < 1.0e-9, "the cabinet's far face touches the neighbour's");
        assert_eq!(project.origin_world(neighbour), Some(DVec3::new(1000.0, 0.0, 0.0)));
    }

    #[test]
    fn a_drag_outside_furniture_keeps_its_exact_distance() {
        let project = sample_project();
        let loose = Id::new(2).expect("id");
        assert!(project.move_rule(&[loose], SnapSettings::default()).is_none());
        assert_eq!(constrained_distance(None, Axis::X, 97.4), (97.4, None));
    }

    #[test]
    fn a_snapped_drag_reports_the_surface_the_release_lands_on() {
        // Two cabinets 400 mm apart. A 398 mm drag would leave a 2 mm gap, and the contact alone
        // would have stopped it at 400 a few frames later; the snap closes the gap under the hand
        // and says which surface it met, so the shell can draw the line and measure the field from
        // it (ADR 0146).
        let mut project = sample_project();
        let cabinet = add_cabinet(&mut project, 0);
        let neighbour = add_cabinet(&mut project, 1000);
        let rule = project.move_rule(&[cabinet], SnapSettings::default()).expect("a furniture drag");
        let (distance, capture) = constrained_distance(Some(&rule), Axis::X, 398.0);
        assert_eq!(distance, 400.0, "the drag jumps the 2 mm gap");
        let capture = capture.expect("a frame face captured it");
        assert_eq!(capture.at_mm, 1000.0);
        assert_eq!(capture.end, model::SnapEnd::High);
        assert_eq!(capture.surface, model::SnapSurface::Frame(neighbour));
    }

    #[test]
    fn a_parallel_axis_ray_has_no_start_parameter() {
        let camera = Camera::default();
        let origin = camera.target();
        let pointer = origin_screen(camera);
        let ray = ray_to(pointer, viewport(), camera);
        assert!(ray.closest_parameter_on_line(origin, ray.direction()).is_none());
    }
}
