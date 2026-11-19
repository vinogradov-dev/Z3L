//! The viewport translate gizmo.
//!
//! Three world-axis handles are projected over the selection's manipulation pivot, the stored pivot
//! of its primary node. A drag never mutates the document: it moves a preview of every selected
//! panel, and the release dispatches one `node.placement.set_many`. Each node's world-space delta is
//! converted into its own parent frame and rounded to micrometres, so a selection whose nodes have
//! different parents still moves rigidly and the inspector agrees with the document after the drag.
//!
//! A object drag is cut by the document's own rule (`Project::move_rule`): a floor-bound product
//! keeps its height, a shell stops at another shell and a body at another body, and the moved box
//! lands on a coordinate plane, on a neighbouring face or on the 1 mm grid (ADR 0102, ADR 0103). What
//! the drag snaps to, and how near, is the workspace's `MoveSettings`, read once when the drag
//! starts (ADR 0146). The same value carries the workspace's floor, which stops a body at Z = 0
//! whatever the snapping says (X87, ADR 0177). The rule is applied to the preview, and the release
//! commits that same distance, so a drag is one undo entry and the preview is not a promise the
//! command breaks.
//!
//! The two tools are held differently (N22, N24, ADR 0147, ADR 0151). The free `Move` keeps its three
//! arrows: a press on a handle and the release are one drag, and the pointer's own micrometre
//! distance is snapped to nothing at all. Handles keep a constant screen length, and an axis
//! within about ten degrees of the view direction is not drawn, because its projected direction is
//! unstable and a pointer drag would move it by metres.
//!
//! Both tools stop at a neighbour, and both stop for one reason: contact is the document's rule, and
//! the write refuses what a drag would have crossed, so a piece stops at a face instead of reaching a
//! sentence (X72, ADR 0162). Both tools read `Project::move_rule`: the free move drops the snaps and
//! keeps every axis and the floor, and the object move hands over the workspace's own settings.
//!
//! The object move draws **no handle at all**: a click on a piece's body takes it, the pointer
//! carries it with no button held, and the next click puts it down. What it may travel is the
//! document's answer: a panel offers only the axis its own thickness points at, so a shelf rises and
//! falls in its thickness direction and never across its own faces, and that one axis is the one the
//! hand's travel names, latched at the first movement so the piece cannot change direction under the
//! hand. A product, a body or a group offers two or three axes: the hand meets the piece on the plane
//! its class allows — the floor for a floor-bound frame, the camera's own plane for a piece no axis
//! holds back — and the document measures the proposed world delta axis by axis, so a floor-bound
//! frame crosses the floor while it keeps its height and a hung one moves everywhere (X74, X75). A
//! contact stops only the axis it meets, so the piece slides along the face rather than stopping
//! dead. Both tools keep the exact-distance fields, which are the typed path to the same distance
//! (ADR 0103), on the axis a multi-axis carry travels furthest along.

use eframe::egui;
use egui::{Response, Ui};
use geom::DVec3;
use model::{Capture, Id, MoveRule};
use scene::{Camera, MAX_DISTANCE_MM};

use crate::gizmo_math::{
    Axis, CarryPlane, HandleSpec, MoveKind, MoveTarget, PlacementCommit, Preview, PreviewKey, Target,
    commit_translation, dominant_axis, hit_axis, named_axis, paint_axis_handles, planar, ray_to, screen_point,
    travel_frame,
};

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
    /// The object move holds nothing: a click took the piece, the pointer carries it, and the
    /// next click puts it down (ADR 0151).
    Following {
        /// The axis the travel named, once the pointer moved far enough for the hand to say so.
        ///
        /// It is also the axis the exact-distance fields measure when the piece may travel on more
        /// than one axis: the one the carry's own delta travels furthest along (X74).
        axis: Option<Axis>,
        /// How the hand's pointer becomes a world delta.
        follow: Follow,
        /// The bond between the pointer and the piece it carries.
        hand: Hand,
    },
}

/// How a carried piece turns the hand's pointer into a world delta.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Follow {
    /// One allowed axis: the hand's travel names it and the hand's distance along it is the move
    /// (N22, ADR 0153).
    Axis,
    /// Two or three allowed axes: the hand's screen place, moved by the offset the click gave the
    /// pivot, meets the plane the piece may move in, and the world difference from the pivot is the
    /// delta the document answers axis by axis (X74, X75).
    Plane {
        /// The plane the piece travels in — the floor, or the camera's own plane.
        plane: CarryPlane,
    },
}

/// The screen bond between the hand and the piece it carries.
///
/// The pointer *hangs* the piece from itself: the pivot keeps the offset on screen that the click
/// gave it, and every frame's distance is read at that offset — solved for along one axis, or met
/// with the carry plane when the piece may travel on more than one (X75). What the bond is
/// deliberately not:
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
    /// The pivot's offset from the hand, in points: the bond the piece hangs by.
    ///
    /// The click lands on the body, so the pivot is not under the pointer; snapping it there would
    /// jump the piece by its own half-size on the frame the axis is named. The offset is kept in
    /// points and not in millimetres, because a carry plane meets the eye at a slant: a world
    /// difference measured at the take stops matching the hand as the piece changes depth (X75).
    offset: Option<egui::Vec2>,
    /// Where the hand is on screen now.
    ///
    /// It is the pointer while the hand tracks the mouse. After a camera gesture the view moved and
    /// the piece did not, so the hand is put back on the piece instead — the pointer itself cannot
    /// be moved on a Wayland session (`set_cursor_position` is unsupported there), and the work area
    /// already draws its own square, so the hand *is* the drawn pointer (X69). It follows the mouse
    /// by the mouse's own movement after that, so dragging still carries the piece 1:1.
    at: egui::Pos2,
    /// The pointer the window last reported, so the hand moves by the mouse's movement.
    seen: egui::Pos2,
    /// Whether the hand was put back on the piece and no longer sits where the pointer is.
    displaced: bool,
}

impl Hand {
    /// The hand of a piece taken by a click at `at`.
    const fn taken(at: egui::Pos2) -> Self {
        Self { from: at, offset: None, at, seen: at, displaced: false }
    }
}

impl Hold {
    /// The axis this hold moves along, while it has one.
    fn axis(self) -> Option<Axis> {
        match self {
            Self::Handle(axis) => Some(axis),
            Self::Following { axis, .. } => axis,
        }
    }

    /// Whether the piece is carried without a button, which is the object move.
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
    /// Whether the document's bodies measured this drag.
    ///
    /// Only the object tool asks the rule; the free move carries the piece where the pointer puts it
    /// and its write is not refused by the body rules (ADR 0180).
    measured: bool,
    /// The surface the face landed on this frame, when a snap — and not the pointer — placed it.
    capture: Option<Capture>,
    drawables: Vec<Id>,
    targets: Vec<MoveTarget>,
    start_origin: DVec3,
    /// The pointer's parameter along the latched axis the distance is measured from.
    ///
    /// It is set when the piece is taken and re-anchored whenever the view moves under the hand, so
    /// the piece keeps the distance it already has and the camera never drags it along (N25). A
    /// multi-axis carry reads the hand's bond from [`Hand::offset`] instead, on its own plane.
    start_parameter: f64,
    delta: DVec3,
    /// A jump the hand is asking for, and how long it has been asking.
    ///
    /// The rule answers the place a drag lands, so a piece carried at an obstacle crosses it in one
    /// frame as soon as a place beyond it fits (ADR 0166). That is too eager for a hand that merely
    /// brushed a face: a board brought up to a slab would flip above it. The carry therefore holds
    /// the crossing for a moment and takes it only while the hand keeps asking (X77).
    jump: Option<(DVec3, f32)>,
    /// The delta the hand itself named last frame, before the rule answered it.
    ///
    /// A jump is an answer that changes far more than the hand did; without this the carry could
    /// only compare the answer with an absolute distance, and at a zoom where a few points of
    /// pointer travel mean a hundred millimetres every ordinary frame would read as a jump (X79).
    asked: DVec3,
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
    /// The shell calls this when a tool takes the pointer: the object move blocks nothing, and a
    /// piece in hand is not a mode the rest of the editor has to wait for (N25).
    pub(crate) fn cancel(&mut self) {
        self.drag = None;
        self.request = None;
    }

    /// Takes the piece under `pointer` in hand, for the object move to carry (ADR 0151).
    ///
    /// The shell calls this after the click that named the piece has selected it; the hold itself
    /// starts on the next frame, from this point. A request that no object move picks up is
    /// dropped with the mode, so a stray click never arms a piece much later.
    pub(crate) fn grab(&mut self, pointer: egui::Pos2) {
        self.request = Some(pointer);
    }

    /// Where the carry's own hand is on screen, when a piece is in it.
    ///
    /// The work area draws its own pointer, so this is where the square goes: the pointer while the
    /// hand tracks the mouse, and the middle of the piece after a camera gesture put it back there
    /// (X69). A session that cannot move the system pointer — Wayland — never sees the difference.
    pub(crate) fn hand(&self) -> Option<egui::Pos2> {
        match self.drag.as_ref()?.hold {
            Hold::Following { hand, .. } if hand.displaced => Some(hand.at),
            _ => None,
        }
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
    /// A one-axis carry has none until the hand's travel names one, which is why the exact-distance
    /// fields appear on the frame the piece starts to move and not on the frame it is taken. A
    /// multi-axis carry has none until the delta travels along an allowed axis, and then answers the
    /// one it travels furthest along, which is the gap its fields measure (X74).
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
            // The held drag's one-axis line: the piece, the cursor, the distance the rule allowed
            // and the surface a snap landed it on (ADR 0168).
            log::debug!(
                target: "z3l::move",
                "drag node={:?} axis={:?} piece=({:.3},{:.3},{:.3}) cursor={} distance={:.3} capture={:?}",
                drag.node,
                axis,
                drag.start_origin.x + drag.delta.x,
                drag.start_origin.y + drag.delta.y,
                drag.start_origin.z + drag.delta.z,
                pointer.map_or_else(|| "none".to_owned(), |at| format!("({:.1},{:.1})", at.x, at.y)),
                distance,
                capture.map(|capture| (capture.surface, capture.at_mm)),
            );
        }
        if !down {
            self.consumed_click = true;
            let commit = commit_translation(&drag.targets, drag.delta, drag.measured)?;
            return Ok(commit);
        }
        self.drag = Some(drag);
        Ok(None)
    }

    /// Carries a piece the object move took: the pointer moves it, a click puts it down and
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
        if target.move_kind != MoveKind::Object {
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
            log::info!(
                target: "z3l::move",
                "drop node={:?} delta=({:.3},{:.3},{:.3})",
                drag.node,
                drag.delta.x,
                drag.delta.y,
                drag.delta.z
            );
            let commit = commit_translation(&drag.targets, drag.delta, drag.measured)?;
            return Ok(commit);
        }
        let Hold::Following { axis: held, follow, hand } = drag.hold else {
            return Ok(None);
        };
        let axes = target.move_kind.axes(target.project, target.selection);
        // A camera gesture is the eye's: the view slides under the piece and the piece keeps its
        // place — a panel set 200 mm from its neighbour stays 200 mm away while the maker pans
        // (X67). The document does not draw the pointer for the length of the gesture.
        let navigating = ui.input(|input| {
            input.pointer.middle_down()
                || input.pointer.secondary_down()
                || input.smooth_scroll_delta != egui::Vec2::ZERO
        });
        let camera_moved = self.last_view.is_some_and(|(camera, _)| camera != target.camera);
        let Some(pointer) = pointer else {
            self.drag = Some(drag);
            return Ok(None);
        };
        // The hand is the app's own square, not the system pointer: on a Wayland session the pointer
        // cannot be moved at all, and the work area already draws its own cursor (X40, X69). While
        // the hand tracks the mouse it *is* the mouse; once a camera gesture has put it back on the
        // piece it moves by the mouse's own movement, so dragging carries the piece 1:1 either way.
        let mut hand = hand;
        let moved = pointer - hand.seen;
        hand.seen = pointer;
        if hand.displaced && viewport.contains(pointer) {
            hand.at += moved;
        } else if hand.displaced {
            // The mouse left the work area, where the system pointer takes over again: the hand goes
            // back to it and the bond is re-read, so the piece stays where it is and nothing that is
            // hit-tested at the system pointer can disagree with the square (X70).
            hand.at = pointer;
            hand.displaced = false;
            hand.offset = None;
        } else {
            hand.at = pointer;
        }
        // The piece is held — it keeps its own distance — while the eye is moving, and on the frame
        // the hand is put back on it: a frame that moves the view must never move the piece (X67).
        let rehome = !navigating && (camera_moved || self.moved_by_gesture);
        let held_now = navigating || rehome;

        // A piece free on two or three axes is carried on the plane its class allows through its
        // pivot: the hand's screen place, moved by the offset the click gave the pivot, meets that
        // plane at one world point, and the world difference from the pivot is the delta the document
        // answers axis by axis. A contact then stops only the axis it meets, so the other axes keep
        // carrying the piece along the face (X74).
        if let Follow::Plane { plane } = follow {
            let mut delta = drag.delta;
            let mut capture = drag.capture;
            let typed = pinned.filter(|distance| distance.is_finite());
            let mut limits = None;
            let mut hand_world = None;
            let mut proposed = drag.asked;
            // The bond is the screen offset the click gave the pivot, so the piece keeps the place it
            // hangs at in *points* and not in millimetres: a plane meets the eye at a slant, and a
            // world delta that matches the pointer's travel at the take only shrinks the piece's own
            // screen travel as the piece changes depth (X75).
            // The bond between hand and pivot is read again when it is missing: a camera gesture or
            // a pointer that left the work area clears it, and a branch that only reads an offset it
            // already has would leave the piece standing still for the rest of the carry (X80).
            let offset = hand.offset.or_else(|| {
                screen_point(target.camera, viewport, drag.start_origin + drag.delta).map(|at| at - hand.at)
            });
            if let (false, Some(offset)) = (held_now, offset)
                && let Some(hit) = plane.point(hand.at + offset, viewport, target.camera, drag.start_origin)
            {
                let asked = bounded(hit - drag.start_origin);
                proposed = asked;
                hand_world = Some(hit);
                // The cut is measured from where the piece stands, not from the start of the drag: a
                // piece resting on the far face of a body must not be pulled back across it when the
                // hand drifts a millimetre into the body (X84, ADR 0173).
                limits = drag.rule.as_ref().map(|rule| rule.limits_from(drag.delta, asked));
                delta = limits.map_or(asked, |limits| {
                    DVec3::new(
                        limits[Axis::X.index()].distance_mm,
                        limits[Axis::Y.index()].distance_mm,
                        limits[Axis::Z.index()].distance_mm,
                    )
                });
            }
            // A typed number is the maker's own face and neither the rule nor a snap moves it
            // (ADR 0103, N26). It pins the axis the carry measured, and the other axes stay the
            // hand's, so a gap and a slide in the same gesture do not fight. The measured axis stays
            // the pinned one while the number holds, or the card would move to another gap mid-type.
            let measured = held.or_else(|| dominant_axis(delta, axes));
            if let (Some(axis), Some(typed)) = (measured, typed) {
                match axis {
                    Axis::X => delta.x = typed,
                    Axis::Y => delta.y = typed,
                    Axis::Z => delta.z = typed,
                }
            }
            let measured = if typed.is_some() { measured } else { dominant_axis(delta, axes) };
            if !held_now {
                capture = match (typed, measured, limits) {
                    (None, Some(axis), Some(limits)) => limits[axis.index()].capture,
                    _ => None,
                };
            }
            // The gesture is over and the camera moved under the piece: put the hand back on the
            // middle of the piece, which is where a maker reaches for it (X67, X69), and read the
            // bond again there so the frame that ends the gesture carries nothing.
            if rehome {
                self.moved_by_gesture = false;
                let middle = target
                    .project
                    .world_box(drag.node)
                    .map_or(drag.start_origin, |(low, high)| (low + high) * 0.5 + delta);
                if let Some(want) = screen_point(target.camera, viewport, middle).filter(|at| viewport.contains(*at)) {
                    hand.at = want;
                    hand.displaced = true;
                    hand.offset = screen_point(target.camera, viewport, drag.start_origin + delta).map(|at| at - want);
                }
            } else if navigating && camera_moved {
                self.moved_by_gesture = true;
            }
            // A typed number is the maker's own face and is never delayed; a rule's answer that
            // crosses an obstacle in one frame is held for a moment first. The step is compared with
            // the hand's own travel, never with an absolute distance: at a zoomed-out view a few
            // points of pointer travel is a hundred millimetres, and an absolute threshold would
            // freeze the piece and let it through in bursts (X79).
            let resolved = delta;
            if typed.is_some() {
                // A typed number is the maker's own face; a crossing the hand asked for before it is
                // no longer pending, or it would fire the moment the number is released (X80).
                drag.jump = None;
            }
            // A jump is taken only once the hand has left the body it is passing: a piece held at a
            // face waits while the pointer is still over that body, and goes the frame the pointer
            // comes out on the far side. The rule answers which body that is, so the log and the hold
            // ask one question (ADR 0172).
            let waiting = drag.rule.as_ref().zip(hand_world).and_then(|(rule, point)| rule.enclosing(point));
            // A place beyond a body the piece would pass through is a crossing, whatever the hand's
            // own speed: the rule answers where the piece lands, so a quick hand leaves no frame in
            // which the piece is seen at the face (ADR 0172). Only a place the rule grants whole is a
            // crossing — an answer cut at a face *is* the face, and the piece is already seen there.
            let granted = (delta - proposed).length() <= 1.0;
            let crossing = drag.rule.as_ref().filter(|_| granted).and_then(|rule| rule.crossing(drag.delta, proposed));
            if typed.is_none() {
                let dt = ui.input(|input| input.stable_dt);
                let hand_step = (proposed - drag.asked).length();
                let (settled, jump) =
                    settled_carry(drag.delta, delta, hand_step, drag.jump, dt, waiting.is_some(), crossing.is_some());
                delta = settled;
                drag.jump = jump;
                drag.asked = proposed;
            }
            log::debug!(
                target: "z3l::move",
                "carry node={:?} piece=({:.3},{:.3},{:.3}) cursor=({:.1},{:.1}) hand={} \
                 asked=({:.3},{:.3},{:.3}) resolved=({:.3},{:.3},{:.3}) delta=({:.3},{:.3},{:.3}) \
                 typed={:?} held={held_now} offset={} jump={:?} waiting={:?} crossing={:?} contact={:?} blocked={}",
                drag.node,
                drag.start_origin.x + delta.x,
                drag.start_origin.y + delta.y,
                drag.start_origin.z + delta.z,
                hand.at.x,
                hand.at.y,
                hand_world.map_or_else(
                    || "none".to_owned(),
                    |point| format!("({:.3},{:.3},{:.3})", point.x, point.y, point.z)
                ),
                proposed.x,
                proposed.y,
                proposed.z,
                resolved.x,
                resolved.y,
                resolved.z,
                delta.x,
                delta.y,
                delta.z,
                typed,
                offset.is_some(),
                drag.jump.map(|(_, age)| age),
                waiting,
                crossing,
                limits.map(|limits| [
                    (limits[0].contact, limits[0].capture),
                    (limits[1].contact, limits[1].capture),
                    (limits[2].contact, limits[2].capture),
                ]),
                drag.rule
                    .as_ref()
                    .and_then(|rule| rule.blocked_at(proposed))
                    .map_or_else(|| "none".to_owned(), stop_text),
            );
            drag.delta = delta;
            drag.capture = capture;
            drag.hold = Hold::Following { axis: measured, follow: Follow::Plane { plane }, hand };
            self.drag = Some(drag);
            return Ok(None);
        }

        let mut offset = hand.offset;
        // The axis is named once, by the first movement of the hand past a stray point, and kept: a
        // piece that re-decided every frame would flip near 45° and its distance would restart.
        let axis = match held {
            Some(axis) => Some(axis),
            None => {
                let named = hand.at - hand.from;
                (named.length() >= NAMING_TRAVEL)
                    .then(|| named_axis(target.camera, viewport, drag.start_origin, named, axes))
                    .flatten()
                    .map(|handle| handle.axis)
            }
        };
        let Some(axis) = axis else {
            // The hand has not said which way yet, so the piece stays where it was taken.
            drag.hold = Hold::Following { axis: None, follow: Follow::Axis, hand };
            self.drag = Some(drag);
            return Ok(None);
        };
        let pivot = drag.start_origin + drag.delta;
        if offset.is_none() {
            offset = screen_point(target.camera, viewport, pivot).map(|at| at - hand.at);
        }
        let carried = match offset {
            Some(offset) if pinned.is_none() && !held_now => self.hangs_from(
                viewport,
                target.camera,
                drag.start_origin,
                axis,
                hand.at + offset,
                drag.delta.dot(axis.direction()),
            ),
            _ => None,
        };
        // A typed distance is the maker's own number, and neither the rule nor a snap moves it: it
        // is the same branch a held handle takes, so a gap types the same in both tools (ADR 0103,
        // N26). The pointer's own answer is the only one the document's rule may cut.
        let (distance, capture) = match pinned.filter(|distance| distance.is_finite()) {
            Some(typed) => (Some(typed), None),
            None => carried
                .map(|scalar| scalar.clamp(-MAX_DISTANCE_MM, MAX_DISTANCE_MM))
                .map(|scalar| constrained_distance(drag.rule.as_ref(), axis, scalar))
                .map_or((None, None), |(distance, capture)| (Some(distance), capture)),
        };
        if let Some(distance) = distance.filter(|distance| distance.is_finite()) {
            drag.delta = axis.direction() * distance;
            drag.capture = capture;
        }
        // The gesture is over and the camera moved under the piece: put the *hand* back on the middle
        // of the piece, which is where a maker reaches for it. The view moved and the piece did not,
        // so this is the pointer that comes back — never the piece (X67, X69).
        if rehome {
            self.moved_by_gesture = false;
            let middle = target
                .project
                .world_box(drag.node)
                .map_or(drag.start_origin, |(low, high)| (low + high) * 0.5 + drag.delta);
            if let Some(want) = screen_point(target.camera, viewport, middle).filter(|at| viewport.contains(*at)) {
                hand.at = want;
                hand.displaced = true;
                // The bond is re-read at the new place, so the frame the hand moves carries nothing.
                hand.offset = screen_point(target.camera, viewport, drag.start_origin + drag.delta).map(|at| at - want);
            }
        } else if navigating && camera_moved {
            // The frame that ends the gesture is the one that has to remember it: the camera stops
            // moving on the very frame the hand can be put back.
            self.moved_by_gesture = true;
        }
        drag.hold = Hold::Following { axis: Some(axis), follow: Follow::Axis, hand };
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
        // the hold starts here on the next one. A request the object move no longer owns is
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

        if target.move_kind == MoveKind::Object {
            let Some(at) = request else {
                return Ok(None);
            };
            let moving: Vec<Id> = targets.iter().map(|target| target.node).collect();
            // The rule and the snap settings are read once, when the piece is taken, exactly as a
            // held drag reads them at the press: the preview and the drop then measure one document.
            let rule = target.project.move_rule(&moving, target.moves, Some(target.contact));
            // A piece that may travel on more than one axis is carried on the plane its class
            // allows, met where the hand is and where the pivot is: the plane is read at the take and
            // every later frame holds the piece by the offset the click gave it. A one-axis piece is
            // measured along its axis instead.
            let axes = target.move_kind.axes(target.project, target.selection);
            let follow = if planar(axes) { Follow::Plane { plane: CarryPlane::of(axes) } } else { Follow::Axis };
            // The grab is measured against the click and not against the frame the axis is named:
            // by then the hand has already travelled far enough to name one, and an offset read
            // there would swallow that travel and leave the piece standing still (X67).
            let offset = screen_point(target.camera, viewport, origin_world).map(|pivot| pivot - at);
            let hand = Hand { offset, ..Hand::taken(at) };
            // What was taken, by name and kind: a report names a node id the reader cannot place,
            // and the piece's own box is what every later judgement is measured from (ADR 0170).
            let taken = target.project.node(anchor.node).map_or_else(
                || "none".to_owned(),
                |node| {
                    let kind = match node {
                        model::Node::Panel(_) => "panel",
                        model::Node::Object(_) => "object",
                        model::Node::Container(container) if container.is_product() => "product",
                        model::Node::Container(_) => "container",
                        model::Node::Camera(_) => "camera",
                        model::Node::Light(_) => "light",
                    };
                    format!("kind={kind} name={:?}", node.name())
                },
            );
            log::info!(
                target: "z3l::move",
                "take node={:?} {taken} pivot=({:.3},{:.3},{:.3}) box={:?} cursor=({:.1},{:.1}) axes={:?}",
                anchor.node,
                origin_world.x,
                origin_world.y,
                origin_world.z,
                target.project.world_box(anchor.node),
                at.x,
                at.y,
                axes
            );
            self.drag = Some(Drag {
                node: anchor.node,
                hold: Hold::Following { axis: None, follow, hand },
                rule,
                measured: true,
                capture: None,
                drawables: target.drawables(),
                targets,
                start_origin: origin_world,
                start_parameter: 0.0,
                delta: DVec3::ZERO,
                jump: None,
                asked: DVec3::ZERO,
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
        // The free move is the maker's own hand: it snaps to nothing, it does not stop at a
        // neighbour and it does not stop on the floor, because control belongs to the object tool and
        // not to the hand that does not ask for it (X85, ADR 0180). The rule it does get is the
        // pointer's own distance on the axes it offers.
        let (rule, measured) = match target.move_kind {
            MoveKind::Free => (target.project.move_rule(&moving, target.moves.free(), Some(target.contact)), false),
            MoveKind::Object => (target.project.move_rule(&moving, target.moves, Some(target.contact)), true),
        };
        self.drag = Some(Drag {
            node: anchor.node,
            hold: Hold::Handle(axis),
            rule,
            measured,
            capture: None,
            drawables: target.drawables(),
            targets,
            start_origin: origin_world,
            start_parameter: parameter,
            delta: DVec3::ZERO,
            jump: None,
            asked: DVec3::ZERO,
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
    /// The object move draws none: a piece is taken by any part of its body, so an arrow over it
    /// would name a target the tool does not have (ADR 0151).
    pub(crate) fn paint(&self, ui: &Ui, viewport: egui::Rect, target: Target<'_>, handle_length: f32) {
        if target.move_kind == MoveKind::Object {
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

/// How much further than the hand the rule's answer may move before the carry calls it a jump, in
/// millimetres.
///
/// The hand's own travel is the measure, not an absolute step: at a zoomed-out view a few points of
/// pointer travel is a hundred millimetres, and an absolute threshold read every ordinary frame as a
/// jump, which froze the piece and let it through in bursts. An answer that moves no further than
/// the hand plus this margin is travel (X79).
const JUMP_MM: f64 = 15.0;

/// How long the hand must keep asking for a jump before the piece takes it, in seconds.
///
/// A sixth of a second is under the time it takes to read the jump, and long enough that a hand
/// passing over a face never flips the piece: the maker sees it rest on the face and only a
/// continuing pull carries it across.
const JUMP_HOLD: f32 = 0.12;

/// A carried delta after the jump hold: where the piece goes this frame, and the request it keeps.
///
/// A jump is the rule's answer changing further than the hand itself travelled, past [`JUMP_MM`]:
/// the hand moved a millimetre and the answer moved a face's thickness, which is a crossing and not
/// travel. The previous delta holds until the same request has been asked for [`JUMP_HOLD`] seconds;
/// a hand that asks for something else, or stops asking, clears the request and the piece follows it
/// at once.
fn settled_carry(
    previous: DVec3,
    asked: DVec3,
    hand_step: f64,
    pending: Option<(DVec3, f32)>,
    dt: f32,
    waiting: bool,
    crossing: bool,
) -> (DVec3, Option<(DVec3, f32)>) {
    let step = (asked - previous).length();
    let leaps = step.is_finite() && step > hand_step + JUMP_MM;
    if !crossing && !leaps {
        return (asked, None);
    }
    // The hand is still over the body the piece would cross: the piece stays on the face it reached
    // and the request keeps no age, so the wait does not turn into a head start. The cursor crossing
    // the body's thickness is the owner's own answer to when a crossing may be taken (ADR 0172).
    if waiting {
        return (previous, Some((previous, 0.0)));
    }
    // A crossing is held from the frame it is first asked for, whatever the hand does next: a quick
    // hand that jumps past a whole body in one frame still sees the piece rest on the face first.
    let (held, age) = pending.unwrap_or((previous, 0.0));
    let age = if age.is_finite() { age + dt } else { dt };
    if age >= JUMP_HOLD { (asked, None) } else { (held, Some((held, age))) }
}

/// The distance a drag may travel, and the surface it landed on: the pointer's own distance, cut by
/// the document's rule of the tool.
///
/// The rule, and the snap settings inside it, were read from the workspace and the document when the
/// drag started and the document still holds those placements, so the preview and the release read
/// the same answer. The free move's rule snaps to nothing, so its distance is the pointer's own
/// micrometre travel up to the first surface in the way.
fn constrained_distance(rule: Option<&MoveRule>, axis: Axis, distance: f64) -> (f64, Option<Capture>) {
    rule.map_or((distance, None), |rule| {
        let limit = rule.limit(axis.move_axis(), distance);
        (limit.distance_mm, limit.capture)
    })
}

/// A proposed world delta kept inside the distance a drag may travel along any axis.
///
/// The pointer meets a plane at one point, and a ray that grazes that plane meets it very far away:
/// a view barely above the floor would otherwise throw the piece kilometres on one frame. The bound
/// is the one the one-axis carry takes, so both tools stop at the same distance.
/// What stopped a drag, written as the log's own field: the part with its kind and world box, or the
/// workspace's floor, which has neither (X70, X87, ADR 0170, ADR 0177).
fn stop_text(stop: model::Stop) -> String {
    match stop {
        model::Stop::Floor => "Some(Floor)".to_owned(),
        model::Stop::Node { id, kind, min, max } => format!(
            "Some({id:?} {kind:?} ({:.1},{:.1},{:.1})..({:.1},{:.1},{:.1}))",
            min.x, min.y, min.z, max.x, max.y, max.z
        ),
    }
}

fn bounded(delta: DVec3) -> DVec3 {
    let inside = |value: f64| value.clamp(-MAX_DISTANCE_MM, MAX_DISTANCE_MM);
    DVec3::new(inside(delta.x), inside(delta.y), inside(delta.z))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use std::f64::consts::FRAC_PI_2;

    use egui::Pos2;
    use geom::DQuat;
    use model::{ClassCode, Envelope, MoveSettings, PanelNominalSize, Placement};
    use testkit::{millimetres, sample_project};
    use units::Length;

    use super::*;
    use crate::gizmo_math::move_targets;

    fn viewport() -> egui::Rect {
        egui::Rect::from_min_size(Pos2::ZERO, egui::Vec2::new(800.0, 600.0))
    }

    #[test]
    fn a_jump_is_held_until_the_hand_keeps_asking_for_it() {
        // X77: the rule answers the place a drag lands, so a piece carried at a slab crosses it in
        // one frame as soon as a place beyond it fits. The carry holds that crossing for a moment,
        // so a hand that merely brushed the face does not flip the piece over it.
        let start = DVec3::ZERO;
        let past = DVec3::new(400.0, 0.0, 0.0);
        let (settled, pending) = settled_carry(start, past, 1.0, None, 0.016, false, false);
        assert_eq!(settled, start, "the first frame asks: the piece stays at the face");
        let pending = pending.expect("the request is kept");
        let (settled, pending) = settled_carry(start, past, 0.5, Some(pending), 0.05, false, false);
        assert_eq!(settled, start, "a tenth of a second is still a pause");
        let (settled, pending) = settled_carry(start, past, 0.5, pending, 0.08, false, false);
        assert_eq!(settled, past, "a sixth of a second takes the place the hand kept naming");
        assert!(pending.is_none(), "and the request is spent");

        // A hand that travelled that far itself is travel, not a jump — however far the answer is.
        let (settled, _) = settled_carry(start, past, 400.0, None, 0.016, false, false);
        assert_eq!(settled, past, "the piece follows a hand that travelled the distance");
        // An ordinary step of a few millimetres is never a jump.
        let step = DVec3::new(3.0, 0.0, 0.0);
        assert_eq!(settled_carry(start, step, 3.0, None, 0.016, false, false).0, step);
    }

    #[test]
    fn a_crossing_is_held_even_when_the_hand_is_quick() {
        // X83: the previous test's jump was recognised by comparing the answer with the hand's own
        // travel, so a hand that crossed a body's whole thickness in one frame was read as ordinary
        // travel and the piece passed the face without ever being seen on it. A crossing named by the
        // rule is held whatever the hand did, and the hand being over the body holds it further.
        let start = DVec3::ZERO;
        let past = DVec3::new(600.0, 0.0, 0.0);
        let (settled, pending) = settled_carry(start, past, 600.0, None, 0.016, false, true);
        assert_eq!(settled, start, "the piece rests on the face the path crosses");
        let pending = pending.expect("the crossing is kept");
        let (settled, _) = settled_carry(start, past, 0.2, Some(pending), 0.05, false, true);
        assert_eq!(settled, start, "a tenth of a second is still the face");

        // The hand is still over the body: the wait does not run, and keeps no head start.
        let (settled, pending) = settled_carry(start, past, 0.2, None, 0.5, true, true);
        assert_eq!(settled, start, "the hand is over the body: the piece holds the face");
        let pending = pending.expect("the request is kept");
        assert_eq!(pending.1, 0.0, "and it keeps no age");
        let (settled, pending) = settled_carry(start, past, 0.2, Some(pending), 0.5, true, true);
        assert_eq!(settled, start, "half a second over the body is still the face");
        // The cursor comes out on the far side, the hold runs, and the piece goes.
        let (settled, _) = settled_carry(start, past, 0.2, pending, JUMP_HOLD, false, true);
        assert_eq!(settled, past, "the frame the hand comes out, the crossing is taken");
        // A path that crosses nothing follows the hand at once, however far it goes.
        let (settled, _) = settled_carry(start, past, 600.0, None, 0.016, false, false);
        assert_eq!(settled, past, "no body on the road: the piece follows");
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
            commit_translation(&[target], DVec3::new(10.0, 0.0, 0.0), true).expect("valid placement").expect("a move");
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
        let commit = commit_translation(&targets, DVec3::new(10.0, 4.0, 0.0), true).expect("valid").expect("a move");
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
        assert_eq!(commit_translation(&[target], DVec3::ZERO, true).expect("no error"), None);
        assert_eq!(commit_translation(&[target], DVec3::new(1.0e-7, 0.0, 0.0), true).expect("no error"), None);
        assert_eq!(commit_translation(&[], DVec3::new(1.0, 0.0, 0.0), true).expect("no error"), None);
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
        let rule = project.move_rule(&[cabinet], MoveSettings::default(), None).expect("an object drag");
        let (distance, capture) = constrained_distance(Some(&rule), Axis::X, 1000.0);
        assert_eq!(distance, 400.0);
        assert_eq!(capture, None, "a stop is the contact's, not a snap's");

        // The release commits exactly the previewed distance, so the preview is no lie.
        let targets = move_targets(&project, &[cabinet]);
        let commit = commit_translation(&targets, Axis::X.direction() * distance, true)
            .expect("a valid placement")
            .expect("the drag moved the cabinet");
        assert_eq!(commit.changes.len(), 1, "one drag is one placement and one undo entry");
        let moved = commit.changes[0].placement.translation();
        assert!((moved - DVec3::new(400.0, 0.0, 0.0)).length() < 1.0e-9, "was {moved:?}");
        assert!((moved.x + 600.0 - 1000.0).abs() < 1.0e-9, "the cabinet's far face touches the neighbour's");
        assert_eq!(project.origin_world(neighbour), Some(DVec3::new(1000.0, 0.0, 0.0)));
    }

    #[test]
    fn the_free_move_keeps_its_distance_and_does_not_stop_at_a_neighbour() {
        // No snapping and no grid is the free tool's, and no contact either: the bodies of the
        // document measure the object tool, not the hand that asks for nothing (ADR 0180).
        let mut project = sample_project();
        let loose = Id::new(2).expect("id");
        assert_eq!(constrained_distance(None, Axis::X, 97.4), (97.4, None), "no rule is the pointer's own travel");
        let free =
            project.move_rule(&[loose], MoveSettings::default().free(), None).expect("a loose board is still measured");
        assert_eq!(constrained_distance(Some(&free), Axis::Y, 97.4), (97.4, None), "and it snaps to nothing");
        // A third board beside the sample shelf is carried straight through it: nothing stands in a
        // free move's way.
        let root = project.root().id();
        let beside = project
            .add_panel(
                root,
                "Beside",
                PanelNominalSize::new(millimetres(100), millimetres(100)).expect("a small board"),
                None,
                millimetres(18),
                Placement::IDENTITY.with_translation(millimetres(900), Length::ZERO, Length::ZERO).expect("a place"),
            )
            .expect("a board beside the sample ones");
        let free =
            project.move_rule(&[beside], MoveSettings::default().free(), None).expect("the board has a contact rule");
        assert_eq!(
            constrained_distance(Some(&free), Axis::X, -500.0),
            (-500.0, None),
            "it passes the shelf instead of stopping at it"
        );
    }

    #[test]
    fn the_free_move_keeps_its_three_axes_and_no_control_at_all() {
        // The free tool draws all three arrows whatever the piece is (ADR 0151) and measures nothing:
        // it is the maker's own hand, so a body goes where the pointer puts it — below the floor and
        // through a neighbour — and only the object tool obeys the document's rules (ADR 0180).
        let project = sample_project();
        let loose = Id::new(2).expect("id");
        let free = project.move_rule(&[loose], MoveSettings::default().free(), None).expect("a loose board is carried");
        assert_eq!(free.axes(), [true, true, true], "the free move refuses no axis");
        let down = free.limit(model::MoveAxis::Z, -500.0);
        assert_eq!(down.distance_mm, -500.0, "the floor does not stop the free move");
        assert_eq!(down.contact, None, "and nothing is reported as standing in the way");
        assert_eq!(
            stop_text(model::Stop::Floor),
            "Some(Floor)",
            "the log still names the floor the object tool stops on"
        );
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
        let rule = project.move_rule(&[cabinet], MoveSettings::default(), None).expect("an object drag");
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
