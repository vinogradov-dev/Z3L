//! Shared math and presentation for the viewport.
//!
//! The translate and rotate gizmos project world points with the camera, hit-test their handles in
//! screen space and compute one placement per moved node; the shell dispatches them as a single
//! `node.placement.set_many`. The work plane turns a pointer into the world plane a drawing lands
//! on, and the snap functions turn that raw point into the point a drawing tool stores. Keeping the
//! axis colours, the projection, the work plane, the snapping and the commit types here means the
//! gizmos and the drawing tools cannot drift apart.

use std::f64::consts::FRAC_PI_2;

use eframe::egui;
use egui::{Align2, Color32, FontId, Pos2, Shape, Stroke, Ui, Vec2};
use engine::{Dimension, NodeRules};
use geom::{DMat4, DQuat, DVec2, DVec3};
use model::{Id, MoveAxis, MoveSettings, Placement, Project};
use scene::{Camera, MAX_PITCH, MIN_PITCH, Projection, Ray};
use ui::TransformMode;
use units::Length;

/// Radius inside which a pointer hits a handle, in points: the largest a handle may have.
///
/// The translate and vertex gizmos scale it with their own handle length (U86); the pull arrow of
/// the extrude tool uses it as it stands.
pub(crate) const HIT_RADIUS: f32 = 8.0;

/// The share of a handle's length a pointer may miss by and still hit it.
const HIT_RADIUS_FRACTION: f32 = 0.1;

/// The smallest hit zone a short handle keeps, in points.
const MIN_HIT_RADIUS: f32 = 2.5;

/// The share of a handle next to its origin that never starts a drag, capped so a long handle
/// still answers a pointer close to the pivot.
const ORIGIN_DEAD_ZONE_FRACTION: f32 = 0.15;
const MAX_ORIGIN_DEAD_ZONE: f32 = 10.0;

/// The arrowhead's length as a share of a handle, bounded by these two values in points.
const ARROW_LENGTH_FRACTION: f32 = 0.14;
const MIN_ARROW_LENGTH: f32 = 3.0;
const MAX_ARROW_LENGTH: f32 = 9.0;

/// Axes closer than this to the view direction are not drawn.
const VIEW_ALIGNMENT_LIMIT: f64 = 0.985;

/// Every local axis a handle may name, for a gizmo that is not gated by a class.
pub(crate) const ALL_AXES: [bool; 3] = [true, true, true];

/// How close the orbit angles must be to a standard view, in radians.
///
/// A snapped view is exact; any orbit moves the camera far outside this tolerance and becomes an
/// axonometric view. A top or bottom view sits exactly at its pole (ADR 0057), so its pitch matches
/// the limit within this tolerance.
const STANDARD_VIEW_TOLERANCE: f64 = 1.0e-9;

/// Gamma-space colours for the three axes.
const X_COLOR: Color32 = Color32::from_rgb(232, 80, 80);
const Y_COLOR: Color32 = Color32::from_rgb(92, 204, 116);
const Z_COLOR: Color32 = Color32::from_rgb(92, 140, 255);

/// Colour of the handle under the pointer or being dragged.
pub(crate) const ACTIVE_COLOR: Color32 = Color32::from_rgb(255, 220, 96);

/// One local or world axis of a gizmo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    /// All three axes in display order.
    pub(crate) const ALL: [Self; 3] = [Self::X, Self::Y, Self::Z];

    /// The unit vector of this axis in the frame the gizmo draws it in.
    pub(crate) fn direction(self) -> DVec3 {
        match self {
            Self::X => DVec3::X,
            Self::Y => DVec3::Y,
            Self::Z => DVec3::Z,
        }
    }

    /// The one-letter label shown next to a handle.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::X => "X",
            Self::Y => "Y",
            Self::Z => "Z",
        }
    }

    /// Its index in a node's local frame, where X is the length, Y the width and Z the thickness.
    pub(crate) const fn index(self) -> usize {
        match self {
            Self::X => 0,
            Self::Y => 1,
            Self::Z => 2,
        }
    }

    /// The world axis a translate drag of this handle runs along.
    pub(crate) const fn move_axis(self) -> MoveAxis {
        match self {
            Self::X => MoveAxis::X,
            Self::Y => MoveAxis::Y,
            Self::Z => MoveAxis::Z,
        }
    }

    /// This axis' component of a world vector.
    pub(crate) fn component(self, vector: DVec3) -> f64 {
        match self {
            Self::X => vector.x,
            Self::Y => vector.y,
            Self::Z => vector.z,
        }
    }

    /// The local dimension this axis changes, for the class gate.
    pub(crate) const fn dimension(self) -> Dimension {
        match self {
            Self::X => Dimension::Length,
            Self::Y => Dimension::Width,
            Self::Z => Dimension::Thickness,
        }
    }

    /// The gamma-space colour of this axis.
    pub(crate) fn color(self) -> Color32 {
        match self {
            Self::X => X_COLOR,
            Self::Y => Y_COLOR,
            Self::Z => Z_COLOR,
        }
    }
}

/// What a gizmo points at in one frame.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Target<'a> {
    /// The camera that projects the handles.
    pub(crate) camera: Camera,
    /// The document whose nodes are being placed.
    pub(crate) project: &'a Project,
    /// The contact adapter the document's rule measures a body with, exactly as the write's guard
    /// asks it: one adapter, so a preview cannot show a place the release refuses (ADR 0179).
    pub(crate) contact: &'a dyn geom::Contact,
    /// The selected nodes, oldest first; the last movable one anchors the gizmo.
    pub(crate) selection: &'a [Id],
    /// What an object drag snaps to and stops at, as the workspace settings chose it (ADR 0146,
    /// ADR 0177).
    ///
    /// It travels here rather than as another argument because a drag that starts reads it once,
    /// with the camera and the selection it starts under.
    pub(crate) moves: MoveSettings,
    /// Which move a translate drag is: the free one, or the furniture one that obeys the document's
    /// rules and offers only the axes they allow (N22, ADR 0147).
    pub(crate) move_kind: MoveKind,
    /// Which turn a ring drag is: the free one, or the one the document's bodies stop (X88).
    pub(crate) rotate_kind: RotateKind,
}

/// Which move a translate drag makes.
///
/// The two tools share every other part of the gesture — the pivot, the preview, the exact-distance
/// fields and the one `node.placement.set_many` on release — and differ in one question: does the
/// document's rule measure the drag, or the pointer's own travel (N22, ADR 0147).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MoveKind {
    /// The pointer's own distance, rounded to the document's micrometre: no contact and no snap.
    Free,
    /// The document's rules: the axes a piece allows, contact measured by its faces, and the
    /// workspace's snaps.
    Object,
}

impl MoveKind {
    /// Which of the two moves a mode makes, or the free one where the mode is not a move at all.
    ///
    /// A rotate has no use for it, and `Select` draws no gizmo, so the two answer the free move: the
    /// question is only ever asked by a translate drag. The mode is the window's, named in
    /// `ui::transform` beside its key (U113); this is the gesture it hands the gizmo.
    pub(crate) fn of(mode: TransformMode) -> Self {
        match mode {
            TransformMode::Object => Self::Object,
            TransformMode::Select | TransformMode::Move | TransformMode::Rotate | TransformMode::ObjectRotate => {
                Self::Free
            }
        }
    }

    /// The axes a drag of this kind offers the selection.
    ///
    /// The free move keeps the three world axes everywhere; the object move asks the document
    /// which axes the pieces may travel along, so a panel is held to its own thickness axis and a
    /// floor-bound frame keeps its height (N22).
    pub(crate) fn axes(self, project: &Project, selection: &[Id]) -> [bool; 3] {
        match self {
            Self::Free => ALL_AXES,
            Self::Object => project.move_axes(selection),
        }
    }
}

/// Which turn a ring drag makes.
///
/// The two turns share the rings, the pivot, the preview and the one `node.placement.set_many` on
/// release, and differ in one question: does the document's rule measure the turn, or the hand
/// alone (X88, ADR 0180).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RotateKind {
    /// The hand's own angle: the rings turn the selection through whatever stands beside it.
    Free,
    /// The document's rules: the ring stops where another body does, and the release is refused where
    /// it would put a body inside one.
    Object,
}

impl RotateKind {
    /// Which of the two turns a mode makes, or the free one where the mode is not a turn at all.
    ///
    /// A move has no use for it, and `Select` draws no gizmo, so the two answer the free turn: the
    /// question is only ever asked by a ring drag.
    pub(crate) fn of(mode: TransformMode) -> Self {
        match mode {
            TransformMode::ObjectRotate => Self::Object,
            TransformMode::Select | TransformMode::Move | TransformMode::Object | TransformMode::Rotate => Self::Free,
        }
    }

    /// Whether the document's bodies measure a turn of this kind.
    pub(crate) const fn measured(self) -> bool {
        matches!(self, Self::Object)
    }
}

impl Target<'_> {
    /// The rule a ring drag of this kind is measured by, or `None` when the hand turns alone.
    ///
    /// It is read once, when the drag starts, exactly as the move rule is: the document does not
    /// change while the ring is held, so the preview and the release read one measurement (X88).
    pub(crate) fn rotate_rule(&self) -> Option<model::RotateRule> {
        self.rotate_kind.measured().then(|| self.project.rotate_rule(self.selection, Some(self.contact))).flatten()
    }

    /// The movable nodes the active gizmo acts on, in selection order.
    pub(crate) fn move_targets(&self) -> Vec<MoveTarget> {
        move_targets(self.project, self.selection)
    }

    /// The node the gizmo sits on: the last movable node in selection order.
    ///
    /// Choosing the last one keeps the gizmo on the primary selection whenever that node is
    /// movable; when the primary is the root or covered by a selected ancestor, the gizmo falls
    /// back to the ancestor that will actually move.
    pub(crate) fn anchor(&self) -> Option<Id> {
        self.move_targets().last().map(|target| target.node)
    }

    /// The world pivot of the node the gizmo sits on.
    pub(crate) fn anchor_pivot(&self) -> Option<DVec3> {
        self.project.pivot_world(self.anchor()?)
    }

    /// Every scene item that moves with the selection, in target order.
    ///
    /// A panel, an object and a path carry their own id. A product's dashed envelope does too, so
    /// the frame follows an object drag instead of staying behind until the release.
    pub(crate) fn drawables(&self) -> Vec<Id> {
        let mut drawables = Vec::new();
        for target in self.move_targets() {
            if self.project.container(target.node).is_some_and(|container| container.envelope().is_some()) {
                drawables.push(target.node);
            }
            drawables.extend(self.project.geometry_ids_under(target.node).unwrap_or_default());
        }
        drawables
    }
}

/// What a stop names in a log, with the way the rule measured it.
///
/// The move and the turn both stop at a body, and a report that said only "blocked" would leave the
/// reader to guess which one: the id, the kind and the bounds of the surface are the rule's own
/// reading, so the line is the measurement rather than a second opinion (ADR 0170, X88).
pub(crate) fn stop_text(stop: model::Stop) -> String {
    match stop {
        model::Stop::Floor => "Some(Floor)".to_owned(),
        model::Stop::Node { id, kind, min, max } => format!(
            "Some({id:?} {kind:?} ({:.1},{:.1},{:.1})..({:.1},{:.1},{:.1}))",
            min.x, min.y, min.z, max.x, max.y, max.z
        ),
    }
}

/// One node a gizmo drag moves, with the frame it started in.
#[derive(Clone, Copy, Debug)]
pub(crate) struct MoveTarget {
    /// The node whose stored placement changes.
    pub(crate) node: Id,
    /// Its placement relative to the parent at the start of the drag.
    pub(crate) start_local: Placement,
    /// The parent's world placement at the start of the drag.
    pub(crate) parent: Placement,
    /// The node's stored pivot in its own frame.
    pub(crate) pivot_local: DVec3,
}

/// Movable nodes of a selection: the top-most ones, without the root and without the unknown.
///
/// A container and one of its descendants can both be selected; only the container moves, so the
/// descendant is not shifted twice. Order follows the selection, so the last entry is the primary.
pub(crate) fn move_targets(project: &Project, selection: &[Id]) -> Vec<MoveTarget> {
    let root = project.root().id();
    project
        .top_most_nodes(selection)
        .into_iter()
        .filter(|node| *node != root)
        .filter_map(|node| {
            let parent = project.parent_of(node)?;
            Some(MoveTarget {
                node,
                start_local: project.node(node)?.placement(),
                parent: project.world_placement(parent)?,
                pivot_local: project.pivot_local_mm(node)?,
            })
        })
        .collect()
}

/// One node's new placement, computed by a gizmo and dispatched as part of one command.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PlacementChange {
    /// The node whose placement changed.
    pub(crate) node: Id,
    /// The new placement relative to its parent.
    pub(crate) placement: Placement,
}

/// The placements a finished drag asks the shell to dispatch.
///
/// A drag on a multi-selection produces one commit with one change per moved node, so the shell
/// dispatches a single `node.placement.set_many` and the whole drag is one undo entry.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PlacementCommit {
    /// The changed nodes; never empty.
    pub(crate) changes: Vec<PlacementChange>,
    /// Whether the document's bodies measured this gesture.
    ///
    /// A drag of the object tool is measured and its write is refused when it lands a body inside
    /// another; the free move and the rotation are the maker's own hand and write what they showed
    /// (ADR 0180).
    pub(crate) measured: bool,
}

/// Converts a world-space shift into one placement per target, or `None` when nothing moved.
///
/// The shift is converted into each node's parent frame, so a selection whose nodes have different
/// parents still moves rigidly. Every translation is rounded to whole micrometres, which is the
/// precision the inspector shows and the document keeps.
pub(crate) fn commit_translation(
    targets: &[MoveTarget],
    delta_world: DVec3,
    measured: bool,
) -> Result<Option<PlacementCommit>, String> {
    if delta_world == DVec3::ZERO {
        return Ok(None);
    }
    if !delta_world.is_finite() {
        return Err("the drag produced a non-finite translation".to_owned());
    }
    let mut changes = Vec::new();
    for target in targets {
        let delta_parent = target.parent.rotation().inverse() * delta_world;
        let translation = target.start_local.translation() + delta_parent;
        if !translation.is_finite() {
            return Err("the drag moved the node to a non-finite position".to_owned());
        }
        let placement =
            Placement::new(translation, target.start_local.rotation()).map_err(|error| error.to_string())?;
        let placement = quantised(placement)?;
        if placement != target.start_local {
            changes.push(PlacementChange { node: target.node, placement });
        }
    }
    Ok(unchanged_or(changes, measured))
}

/// Converts a world-space turn about `pivot_world` into one placement per target.
///
/// The anchor is the last target: its stored pivot defines the centre exactly, and every other
/// target is turned about the same world point. A node whose translation already sits on the centre
/// keeps its translation bit-for-bit; every other translation is rounded to whole micrometres.
/// `measured` is the drag's own answer: the object turn's release is refused where it would put a
/// body inside another, while the free turn writes what it showed (ADR 0180, X88).
pub(crate) fn commit_rotation(
    targets: &[MoveTarget],
    pivot_world: DVec3,
    rotation_world: DQuat,
    measured: bool,
) -> Result<Option<PlacementCommit>, String> {
    if !rotation_world.is_finite() || (rotation_world.length() - 1.0).abs() > 1.0e-9 {
        return Err("the drag produced a non-finite rotation".to_owned());
    }
    let anchor = targets.last().map(|target| target.node);
    let mut changes = Vec::new();
    for target in targets {
        let parent_rotation = target.parent.rotation();
        let delta_parent = (parent_rotation.inverse() * rotation_world * parent_rotation).normalize();
        if !delta_parent.is_finite() {
            return Err("the drag produced a non-finite rotation".to_owned());
        }
        let centre = if Some(target.node) == anchor {
            target.start_local.transform_point(target.pivot_local)
        } else {
            parent_rotation.inverse() * (pivot_world - target.parent.translation())
        };
        if !centre.is_finite() {
            return Err("the drag produced a non-finite rotation centre".to_owned());
        }
        let rotation = (delta_parent * target.start_local.rotation()).normalize();
        if !rotation.is_finite() {
            return Err("the drag produced a non-finite rotation".to_owned());
        }
        let start_translation = target.start_local.translation();
        let placement = if centre == start_translation {
            // A turn about the node's own translation keeps it exactly.
            Placement::new(start_translation, rotation).map_err(|error| error.to_string())?
        } else {
            let translation = centre + delta_parent * (start_translation - centre);
            if !translation.is_finite() {
                return Err("the drag moved the node to a non-finite position".to_owned());
            }
            quantised(Placement::new(translation, rotation).map_err(|error| error.to_string())?)?
        };
        if !placement.same_transform(target.start_local) {
            changes.push(PlacementChange { node: target.node, placement });
        }
    }
    Ok(unchanged_or(changes, measured))
}

/// Packs the changed nodes into a commit, or `None` when a drag ended where it started.
fn unchanged_or(changes: Vec<PlacementChange>, measured: bool) -> Option<PlacementCommit> {
    if changes.is_empty() { None } else { Some(PlacementCommit { changes, measured }) }
}

/// Rounds a placement's translation to whole micrometres at the editing boundary.
fn quantised(placement: Placement) -> Result<Placement, String> {
    let [x, y, z] = placement
        .translation_lengths()
        .ok_or_else(|| "the drag moved the node outside the editable millimetre range".to_owned())?;
    placement.with_translation(x, y, z).map_err(|error| error.to_string())
}

/// The scene change a gizmo drag is previewing, without touching the document.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Preview<'a> {
    /// Shift the listed drawables by a world-space delta.
    Translation {
        /// Drawables to move.
        drawables: &'a [Id],
        /// World-space shift.
        delta: DVec3,
    },
    /// Turn the listed drawables about a world-space pivot.
    Rotation {
        /// Drawables to turn.
        drawables: &'a [Id],
        /// World-space point the turn is centred on.
        pivot: DVec3,
        /// World-space turn.
        rotation: DQuat,
    },
}

impl Preview<'_> {
    /// The drawable ids the preview moves.
    pub(crate) fn drawables(&self) -> &[Id] {
        match self {
            Self::Translation { drawables, .. } | Self::Rotation { drawables, .. } => drawables,
        }
    }

    /// The world point a preview puts `point` at.
    ///
    /// It is the rigid transform the drawables take, applied to one point: an overlay that marks a
    /// carried node — the corner brackets of the level that is open — reads this instead of the
    /// placement the document still holds (X75).
    pub(crate) fn moved_point(&self, point: DVec3) -> DVec3 {
        match self {
            Self::Translation { delta, .. } => point + *delta,
            Self::Rotation { pivot, rotation, .. } => *pivot + *rotation * (point - *pivot),
        }
    }

    /// The world placement a node would have if the preview were committed.
    ///
    /// A camera or a light has no drawable, so the scene frame cannot preview it through its
    /// geometry. The same rigid transform that moves the drawables is applied to the node's world
    /// placement instead: a translation shifts it and a rotation turns it about the drag pivot.
    /// Returns `None` when the result is not a valid placement.
    pub(crate) fn moved_world(&self, world: Placement) -> Option<Placement> {
        match self {
            Self::Translation { delta, .. } => Placement::new(world.translation() + delta, world.rotation()).ok(),
            Self::Rotation { pivot, rotation, .. } => {
                let translation = *pivot + *rotation * (world.translation() - *pivot);
                Placement::new(translation, (*rotation * world.rotation()).normalize()).ok()
            }
        }
    }
}

/// Bit-exact identity of a preview, used to skip an unchanged re-upload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PreviewKey {
    /// A translation preview: node and delta bits.
    Translation {
        /// The node being dragged.
        node: Id,
        /// `x`, `y`, `z` of the delta as raw bits.
        delta: [u64; 3],
    },
    /// A rotation preview: node and turn bits.
    Rotation {
        /// The node being dragged.
        node: Id,
        /// The accumulated local turn as raw bits.
        angle: u64,
    },
    /// A vertex-move preview: the edited object and the local offset in micrometres.
    Vertex {
        /// The object whose vertices move.
        object: Id,
        /// The quantised local offset per axis.
        offset: [i64; 3],
    },
}

/// The world point under a viewport position.
pub(crate) fn ray_to(pointer: Pos2, viewport: egui::Rect, camera: Camera) -> Ray {
    let (x, y) = normalized_pointer(pointer, viewport);
    camera.ray_ndc(x, y, aspect(viewport))
}

/// A world point in viewport points, or `None` when it is behind the camera.
pub(crate) fn screen_point(camera: Camera, viewport: egui::Rect, point: DVec3) -> Option<Pos2> {
    let ndc = camera.project_ndc(point, aspect(viewport))?;
    let x = viewport.left() + (ndc.x as f32 + 1.0) * 0.5 * viewport.width();
    let y = viewport.top() + (1.0 - ndc.y as f32) * 0.5 * viewport.height();
    (x.is_finite() && y.is_finite()).then_some(Pos2::new(x, y))
}

/// The world point under the pointer, taken on the plane through the camera's target.
///
/// The wheel zooms about this point, so what the maker is looking at keeps its place: a zoom about
/// the view's centre slid a carried piece out of the hand on every step (X67). The plane through the
/// target is where the projection's scale is exact for both projections; a ray along the view
/// direction has no point on it, which is the answer `None`.
pub(crate) fn view_plane_point(pointer: Pos2, viewport: egui::Rect, camera: Camera) -> Option<DVec3> {
    plane_point(pointer, viewport, camera, camera.target(), camera.forward())
}

/// The world point where the pointer's ray meets the plane through `origin` with `normal`.
///
/// A carried piece that may travel on more than one axis is met on such a plane: the pointer meets
/// it at one world point, and the difference of two such points is the delta the document's rule
/// measures axis by axis. A plane the ray runs along, or one that lies behind the eye, has no point
/// on it — a pointer can only meet a plane it looks at — which is the answer `None`.
pub(crate) fn plane_point(
    pointer: Pos2,
    viewport: egui::Rect,
    camera: Camera,
    origin: DVec3,
    normal: DVec3,
) -> Option<DVec3> {
    let ray = ray_to(pointer, viewport, camera);
    let facing = ray.direction().dot(normal);
    if facing.abs() <= f64::EPSILON {
        return None;
    }
    let depth = (origin - ray.origin()).dot(normal) / facing;
    (depth > 0.0 && depth.is_finite()).then(|| ray.origin() + ray.direction() * depth)
}

/// The plane a carried piece travels in while its class lets it move on more than one axis.
///
/// The hand keeps the piece: the pointer meets this plane at one world point, and the difference of
/// two such points is the delta the document's rule answers axis by axis. Which plane that is
/// decides whether the piece stays under the pointer, so the plane is the one the piece may really
/// move in and not the one facing the eye (X75): a floor-bound frame is met on the floor, and only a
/// piece whose three axes are free — a camera, a light, a body — is met on the camera's own plane,
/// where a delta in depth is a delta the piece can take.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum CarryPlane {
    /// The plane that faces the camera, for a piece no axis holds back.
    Camera,
    /// The plane across the one axis the piece may not travel on — the floor for a floor-bound
    /// product, whose height its class keeps.
    Across(Axis),
}

impl CarryPlane {
    /// The plane a piece whose axes are `allowed` is carried in, from the axes the tool offers.
    ///
    /// One allowed axis is not a plane and never reaches here (a panel keeps the hand-travel latch);
    /// three leave nothing to hold the piece back, and two are met across the axis they refuse.
    pub(crate) fn of(allowed: [bool; 3]) -> Self {
        let mut refused = Axis::ALL.into_iter().filter(|axis| !axis_allowed(allowed, *axis));
        match (refused.next(), refused.next()) {
            (Some(axis), None) => Self::Across(axis),
            _ => Self::Camera,
        }
    }

    /// The world point the pointer holds the piece at on this plane through `origin`.
    pub(crate) fn point(self, pointer: Pos2, viewport: egui::Rect, camera: Camera, origin: DVec3) -> Option<DVec3> {
        let normal = match self {
            Self::Camera => camera.forward(),
            Self::Across(axis) => axis.direction(),
        };
        plane_point(pointer, viewport, camera, origin, normal)
    }
}

/// The reference guide as screen polylines: the axis and the sweep sampled on the work plane and
/// projected point by point.
///
/// A plane circle seen at an angle projects to an ellipse, so the guide's sweep is not a screen-space
/// circle struck from the anchor with the plane's angles: both the sketch point and the latched
/// corner come through here, and either projection is the plane's own figure (ADR 0133). Samples a
/// camera cannot place are dropped; a path with fewer than two of them draws nothing.
pub(crate) fn reference_screen(
    reference: ui::Reference,
    project: impl Fn(DVec2) -> Option<Pos2>,
) -> (Vec<Pos2>, Vec<Pos2>) {
    let (axis, sweep) = reference.samples();
    (axis.into_iter().filter_map(&project).collect(), sweep.into_iter().filter_map(&project).collect())
}

/// Rounds a millimetre coordinate to a whole micrometre, or `None` when it is not representable.
pub(crate) fn length_from_millimetres(value: f64) -> Option<Length> {
    let micrometres = value * 1_000.0;
    if !micrometres.is_finite() || micrometres < i64::MIN as f64 || micrometres > i64::MAX as f64 {
        return None;
    }
    Some(Length::from_micrometres(micrometres.round() as i64))
}

/// Distance from `point` to a screen-space segment.
pub(crate) fn distance_to_segment(point: Pos2, start: Pos2, end: Pos2) -> Option<f32> {
    let segment = end - start;
    let length_squared = segment.length_sq();
    if length_squared <= f32::EPSILON {
        return None;
    }
    let along = ((point - start).dot(segment) / length_squared).clamp(0.0, 1.0);
    Some((point - (start + segment * along)).length())
}

/// The point where a ray meets a plane, or `None` when they are parallel or the plane is behind.
pub(crate) fn ray_plane(ray: Ray, plane_point: DVec3, normal: DVec3) -> Option<DVec3> {
    let denominator = ray.direction().dot(normal);
    if denominator.abs() < 1.0e-9 {
        return None;
    }
    let distance = (plane_point - ray.origin()).dot(normal) / denominator;
    if !distance.is_finite() || distance <= 0.0 {
        return None;
    }
    Some(ray.origin() + ray.direction() * distance)
}

/// A right-handed world plane the drawing tools place points on.
///
/// A plane the view picks passes through the world origin, so a drawing is stored in world-aligned
/// plane coordinates; a plane the document places — a contour ring's own — carries the node's origin
/// instead (U119). `u` and `v` follow the screen of the view that selected the plane: a shape drawn to
/// the right and up lands on positive `u` and `v` in both the viewport and the document.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct WorkPlane {
    origin: DVec3,
    normal: DVec3,
    u: DVec3,
    v: DVec3,
}

impl WorkPlane {
    /// World XY: the plane a perspective or axonometric view draws on.
    pub(crate) const WORLD_XY: Self = Self { origin: DVec3::ZERO, normal: DVec3::Z, u: DVec3::X, v: DVec3::Y };

    /// The plane a placement puts its local XY on: a contour ring's own plane, whose zero is the
    /// node's origin (U119).
    ///
    /// The plane of a contour is where its corners are stored, so a ring drawn on a panel's face
    /// offers its box in the plane of that face and not in the world XY.
    pub(crate) fn from_placement(placement: Placement) -> Option<Self> {
        let u = placement.rotation() * DVec3::X;
        let v = placement.rotation() * DVec3::Y;
        let normal = u.cross(v);
        if !u.is_finite() || !v.is_finite() || normal.length() <= 1.0e-9 {
            return None;
        }
        Some(Self { origin: placement.translation(), normal, u, v })
    }

    /// The plane an axis-aligned orthographic view draws on, or world XY for every other view.
    ///
    /// A top or bottom view is recognised by its pitch at a pole; a side view needs an exact
    /// quarter-turn yaw, because any other yaw is an axonometric view and draws on world XY.
    pub(crate) fn from_camera(camera: Camera) -> Self {
        if !matches!(camera.projection(), Projection::Orthographic { .. }) {
            return Self::WORLD_XY;
        }
        let pitch = camera.pitch();
        if (pitch - MAX_PITCH).abs() <= STANDARD_VIEW_TOLERANCE {
            return Self { origin: DVec3::ZERO, normal: DVec3::Z, u: DVec3::X, v: DVec3::Y };
        }
        if (pitch - MIN_PITCH).abs() <= STANDARD_VIEW_TOLERANCE {
            return Self { origin: DVec3::ZERO, normal: -DVec3::Z, u: DVec3::X, v: -DVec3::Y };
        }
        if pitch.abs() > STANDARD_VIEW_TOLERANCE {
            return Self::WORLD_XY;
        }
        let quarter = camera.yaw() / FRAC_PI_2;
        let nearest = quarter.round();
        if (quarter - nearest).abs() > STANDARD_VIEW_TOLERANCE {
            return Self::WORLD_XY;
        }
        match (nearest as i64).rem_euclid(4) {
            0 => Self { origin: DVec3::ZERO, normal: DVec3::X, u: DVec3::Y, v: DVec3::Z },
            1 => Self { origin: DVec3::ZERO, normal: DVec3::Y, u: -DVec3::X, v: DVec3::Z },
            2 => Self { origin: DVec3::ZERO, normal: -DVec3::X, u: -DVec3::Y, v: DVec3::Z },
            _ => Self { origin: DVec3::ZERO, normal: -DVec3::Y, u: DVec3::X, v: DVec3::Z },
        }
    }

    /// The plane normal, pointing from the plane towards the viewer in a standard view.
    pub(crate) const fn normal(self) -> DVec3 {
        self.normal
    }

    /// The same plane moved to pass through `origin`.
    ///
    /// A tool with no drawing plane of its own — a vertex drag — still needs a plane to measure the
    /// pointer against: the one it works at is the plane through the point it carries (U119).
    pub(crate) const fn through(self, origin: DVec3) -> Self {
        Self { origin, ..self }
    }

    /// The world point at plane coordinates `local`.
    #[allow(dead_code, reason = "the commit and snap steps place local points in U13-U17")]
    pub(crate) fn world(self, local: DVec2) -> DVec3 {
        self.origin + self.u * local.x + self.v * local.y
    }

    /// The plane coordinates of a world point on the plane.
    pub(crate) fn local(self, world: DVec3) -> DVec2 {
        let offset = world - self.origin;
        DVec2::new(offset.dot(self.u), offset.dot(self.v))
    }

    /// A placement whose local XY is this plane.
    ///
    /// Local X maps to `u`, local Y to `v` and local Z to the normal, so a path stored in plane
    /// coordinates keeps its orientation in the document. The translation is the plane's own zero,
    /// which is the world origin for a plane the view picked.
    pub(crate) fn placement(self) -> Option<Placement> {
        let basis = DMat4::from_cols(
            self.u.extend(0.0),
            self.v.extend(0.0),
            self.normal().extend(0.0),
            DVec3::ZERO.extend(1.0),
        );
        let rotation = DQuat::from_mat4(&basis).normalize();
        Placement::new(self.origin, rotation).ok()
    }

    /// A placement whose local origin sits at plane coordinates `local`.
    ///
    /// The rotation is the plane's own, so the object's local XY lies on the work plane while its
    /// origin is the clicked point.
    #[allow(dead_code, reason = "the primitive placement consumes it in the shell in U21")]
    pub(crate) fn placement_at(self, local: DVec2) -> Option<Placement> {
        let rotation = self.placement()?.rotation();
        Placement::new(self.world(local), rotation).ok()
    }

    /// The world point where a ray meets the plane, or `None` when it is parallel or behind.
    #[allow(dead_code, reason = "the drawing tools start from these points in U13-U14")]
    pub(crate) fn ray_point(self, ray: Ray) -> Option<DVec3> {
        ray_plane(ray, self.origin, self.normal())
    }

    /// The plane coordinates under a viewport position.
    #[allow(dead_code, reason = "the drawing tools consume the mapping in U13-U14")]
    pub(crate) fn pointer_point(self, camera: Camera, viewport: egui::Rect, pointer: Pos2) -> Option<DVec2> {
        self.ray_point(ray_to(pointer, viewport, camera)).map(|world| self.local(world))
    }

    /// The plane as the model's snap resolver reads it: the same origin, `u` and `v`.
    pub(crate) fn snap_plane(self) -> model::SnapPlane {
        model::SnapPlane::new(self.origin, self.u, self.v).unwrap_or_else(model::SnapPlane::world_xy)
    }

    /// The plane name shown in the strip: `XY`, `XZ` or `YZ`.
    pub(crate) fn label(self) -> &'static str {
        let normal = self.normal();
        if normal.x.abs() > 0.5 {
            "YZ"
        } else if normal.y.abs() > 0.5 {
            "XZ"
        } else {
            "XY"
        }
    }
}

/// Half the width of a snap marker's glyph, in points.
pub(crate) const MARKER_SIZE: f32 = 5.0;

/// Pointer distance inside which a snap captures the pointer, in points.
///
/// The tolerance travels to the resolver as millimetres, measured at the depth the hand works at:
/// only the camera knows how much one viewport point covers there (ADR 0051, U116).
pub(crate) const SNAP_RADIUS_POINTS: f32 = 10.0;

/// The pointer's snap question on the work plane: the ray, the plane it meets and the reach.
///
/// The three travel together because they are one place — the pixel and the two values that turn it
/// into a ray — so a tool hands the resolver one value instead of four (AGENTS §1.5).
#[derive(Clone, Copy, Debug)]
pub(crate) struct PlaneSnap {
    /// The pointer's ray, in the world.
    pub(crate) ray: Ray,
    /// The plane the tool draws on.
    pub(crate) plane: WorkPlane,
    /// The capture radius, in millimetres, at the depth of that point.
    pub(crate) tolerance: Length,
    /// The depth that radius was measured at: the pointer's reach scales with depth (U116).
    pub(crate) depth: f64,
    /// The camera that made the ray, so the answer can be put back on screen.
    pub(crate) camera: Camera,
    /// The viewport the pointer is in.
    pub(crate) rect: egui::Rect,
}

impl PlaneSnap {
    /// The pointer's question on `plane`, or `None` when the ray misses it or its depth is unusable.
    pub(crate) fn at(camera: Camera, rect: egui::Rect, pointer: Pos2, plane: WorkPlane) -> Option<Self> {
        let ray = ray_to(pointer, rect, camera);
        let world = plane.ray_point(ray)?;
        let depth = (world - camera.eye()).dot(camera.forward());
        let millimetres = camera.world_per_point(depth, f64::from(rect.height())) * f64::from(SNAP_RADIUS_POINTS);
        let tolerance = length_from_millimetres(millimetres)?;
        Some(Self { ray, plane, tolerance, depth, camera, rect })
    }

    /// The request one tool's pointer makes of the resolver.
    ///
    /// `target` is the tool's own word on what it does with the answer: a tool that stores a point in
    /// space takes the candidate, and one that draws on a plane declares which reading of a candidate
    /// above it the workspace is in ([`model::SnapTarget`], U119).
    pub(crate) fn request<'a>(
        self,
        kinds: model::SnapKinds,
        target: model::SnapTarget,
        anchor: Option<DVec3>,
        ends: &'a [DVec3],
        axis_lock: bool,
        face: Option<DVec3>,
    ) -> model::SnapRequest<'a> {
        model::SnapRequest {
            ray_origin: self.ray.origin(),
            ray_direction: self.ray.direction(),
            tolerance: self.tolerance,
            tolerance_depth: self.depth,
            kinds,
            plane: self.plane.snap_plane(),
            target,
            anchor,
            ends,
            axis_lock,
            face,
        }
    }
}

/// The marker that says what captured the pointer: one glyph per kind, in one ink.
///
/// The glyph is the kind and the ink is the theme's, so a maker learns the ten shapes once and reads
/// them on every tool. A locked point — Shift's axis lock — draws the diamond the draft always drew,
/// because a constraint the hand stated is not a snap (ADR 0051, U116).
pub(crate) fn paint_snap_marker(
    painter: &egui::Painter,
    at: Pos2,
    kind: Option<model::SnapKind>,
    locked: bool,
    ink: Color32,
) {
    let stroke = Stroke::new(1.6, ink);
    let size = MARKER_SIZE;
    match (kind, locked) {
        (Some(model::SnapKind::Vertex), _) => {
            painter.rect_stroke(
                egui::Rect::from_center_size(at, Vec2::splat(size * 1.6)),
                egui::CornerRadius::ZERO,
                stroke,
                egui::StrokeKind::Middle,
            );
        }
        (Some(model::SnapKind::Intersection), _) => {
            painter.line_segment([at + Vec2::new(-size, -size), at + Vec2::new(size, size)], stroke);
            painter.line_segment([at + Vec2::new(-size, size), at + Vec2::new(size, -size)], stroke);
        }
        (Some(model::SnapKind::Midpoint), _) => {
            let corners = vec![at + Vec2::new(0.0, -size), at + Vec2::new(size, size), at + Vec2::new(-size, size)];
            painter.add(Shape::closed_line(corners, stroke));
        }
        (Some(model::SnapKind::Centre), _) => {
            painter.circle_stroke(at, size, stroke);
            painter.circle_filled(at, 1.5, ink);
        }
        (Some(model::SnapKind::Perpendicular), _) => {
            painter.line_segment([at + Vec2::new(-size, size), at + Vec2::new(size, size)], stroke);
            painter.line_segment([at + Vec2::new(0.0, size), at + Vec2::new(0.0, -size)], stroke);
        }
        (Some(model::SnapKind::Tangent), _) => {
            painter.circle_stroke(at + Vec2::new(0.0, size * 0.4), size * 0.7, stroke);
            painter.line_segment([at + Vec2::new(-size, -size), at + Vec2::new(size, -size)], stroke);
        }
        (Some(model::SnapKind::Edge), _) => {
            painter.line_segment([at + Vec2::new(-size, 0.0), at + Vec2::new(size, 0.0)], stroke);
        }
        (Some(model::SnapKind::Face), _) => {
            let corners =
                vec![at + Vec2::new(0.0, -size), at + Vec2::new(size, size * 0.6), at + Vec2::new(-size, size * 0.6)];
            painter.add(Shape::convex_polygon(corners, ink.gamma_multiply(0.35), stroke));
        }
        (Some(model::SnapKind::Axis), _) => {
            painter.line_segment([at + Vec2::new(-size * 1.3, 0.0), at + Vec2::new(size * 1.3, 0.0)], stroke);
            painter.line_segment([at + Vec2::new(0.0, -size * 0.5), at + Vec2::new(0.0, size * 0.5)], stroke);
        }
        (Some(model::SnapKind::Grid), _) => {
            painter.rect_filled(egui::Rect::from_center_size(at, Vec2::splat(size)), 0.0, ink);
        }
        (None, true) => {
            let corners = vec![
                at + Vec2::new(0.0, -size),
                at + Vec2::new(size, 0.0),
                at + Vec2::new(0.0, size),
                at + Vec2::new(-size, 0.0),
            ];
            painter.add(Shape::closed_line(corners, Stroke::new(1.4, ink)));
        }
        (None, false) => {
            painter.circle_stroke(at, 2.5, Stroke::new(1.2, ink));
        }
    }
}

fn normalized_pointer(pointer: Pos2, viewport: egui::Rect) -> (f64, f64) {
    let width = f64::from(viewport.width());
    let height = f64::from(viewport.height());
    if width <= 0.0 || height <= 0.0 {
        return (0.0, 0.0);
    }
    let x = (f64::from(pointer.x - viewport.left()) / width) * 2.0 - 1.0;
    let y = 1.0 - (f64::from(pointer.y - viewport.top()) / height) * 2.0;
    (x, y)
}

fn aspect(viewport: egui::Rect) -> f64 {
    let width = f64::from(viewport.width());
    let height = f64::from(viewport.height());
    if width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0 { width / height } else { 1.0 }
}

/// A unit screen direction, or `None` when the projected segment is too short to be stable.
pub(crate) fn screen_direction(delta: Vec2) -> Option<Vec2> {
    (delta.length() >= 0.5).then(|| delta.normalized())
}

/// The handles one gizmo offers in a frame: which axes its class allows and how long each draws.
///
/// The length is screen points, resolved from the workspace settings once per frame; the drawing
/// and the hit test read the same value, so the arrow a user sees is the arrow a drag grabs (U86).
#[derive(Clone, Copy, Debug)]
pub(crate) struct HandleSpec {
    /// Which local axes are drawn and hit, in [`Axis::index`] order.
    pub(crate) axes: [bool; 3],
    /// The screen length of one handle, in points.
    pub(crate) length: f32,
}

/// One projected axis handle: the axis, its screen-space tip and its unit screen direction.
#[derive(Clone, Copy, Debug)]
pub(crate) struct AxisHandle {
    /// The axis this handle moves along.
    pub(crate) axis: Axis,
    /// The tip in viewport points.
    pub(crate) tip: Pos2,
    /// The unit direction from the origin towards the tip.
    pub(crate) direction: Vec2,
}

/// Which local axes a node's class lets a gizmo change, in [`Axis::index`] order.
///
/// The vertex gizmo edits geometry, so it offers the handles the class's `extent` allows: a free
/// class all three, a linear stock part only the length, a bought fitting none. An unclassified
/// node is free, which is what keeps the tool usable before a class is assigned.
pub(crate) fn editable_axes(project: &Project, node: Id) -> [bool; 3] {
    let rules = NodeRules::of(project, node);
    let mut axes = ALL_AXES;
    for axis in Axis::ALL {
        if let Some(slot) = axes.get_mut(axis.index()) {
            *slot = rules.may_resize(axis.dimension());
        }
    }
    axes
}

/// Whether a handle list lets `axis` be drawn and hit.
fn axis_allowed(axes: [bool; 3], axis: Axis) -> bool {
    axes.get(axis.index()).copied().unwrap_or(true)
}

/// The projected handles of a gizmo at `origin_world`, skipping axes that point at the camera and
/// the axes the class does not let change.
///
/// `spec` names the axes to draw and the screen length each one takes, in points.
pub(crate) fn axis_handles(
    camera: Camera,
    viewport: egui::Rect,
    origin_world: DVec3,
    origin: Pos2,
    spec: HandleSpec,
) -> Vec<AxisHandle> {
    let step = (camera.distance() * 0.01).max(1.0);
    let mut handles = Vec::with_capacity(Axis::ALL.len());
    for axis in Axis::ALL {
        if !axis_allowed(spec.axes, axis) || axis.direction().dot(camera.forward()).abs() > VIEW_ALIGNMENT_LIMIT {
            continue;
        }
        let Some(end) = screen_point(camera, viewport, origin_world + axis.direction() * step) else {
            continue;
        };
        let Some(direction) = screen_direction(end - origin) else {
            continue;
        };
        handles.push(AxisHandle { axis, tip: origin + direction * spec.length.max(0.0), direction });
    }
    handles
}

/// The axis whose handle is nearest to `pointer`, ignoring the dead zone around the origin.
pub(crate) fn hit_axis(
    camera: Camera,
    viewport: egui::Rect,
    origin_world: DVec3,
    pointer: Pos2,
    spec: HandleSpec,
) -> Option<Axis> {
    let origin = screen_point(camera, viewport, origin_world)?;
    if !viewport.expand(64.0).contains(origin) {
        return None;
    }
    let radius = hit_radius(spec.length);
    let dead_zone = origin_dead_zone(spec.length);
    let mut best: Option<(f32, Axis)> = None;
    for handle in axis_handles(camera, viewport, origin_world, origin, spec) {
        let distance = distance_to_handle(pointer, origin, handle.tip, dead_zone);
        let Some(distance) = distance else {
            continue;
        };
        if distance <= radius && best.is_none_or(|(nearest, _)| distance < nearest) {
            best = Some((distance, handle.axis));
        }
    }
    best.map(|(_, axis)| axis)
}

/// The axis a hand's travel names, for a tool that draws no handle.
///
/// The object move is taken by the piece's body rather than by an arrow (N24, ADR 0151), so the
/// direction has to be read from the pointer itself: each axis the tool may travel is projected at
/// the piece's pivot and the one whose screen direction lies nearest to `travel` wins, whichever way
/// the travel points, because the sign is the hand's and not this choice. An axis that points at the
/// camera is not a candidate, exactly as it draws no handle.
///
/// The handle it answers with carries the axis' screen direction, which is what the carry measures
/// the hand against. `travel` is the pointer's movement since the piece was taken, in points; a
/// travel too short to have a direction names nothing, so the piece does not jump on the first frame.
pub(crate) fn named_axis(
    camera: Camera,
    viewport: egui::Rect,
    origin_world: DVec3,
    travel: Vec2,
    axes: [bool; 3],
) -> Option<AxisHandle> {
    let travel = screen_direction(travel)?;
    let origin = screen_point(camera, viewport, origin_world)?;
    let step = (camera.distance() * 0.01).max(1.0);
    let mut best: Option<(f32, AxisHandle)> = None;
    for axis in Axis::ALL {
        if !axis_allowed(axes, axis) || axis.direction().dot(camera.forward()).abs() > VIEW_ALIGNMENT_LIMIT {
            continue;
        }
        let Some(end) = screen_point(camera, viewport, origin_world + axis.direction() * step) else {
            continue;
        };
        let Some(direction) = screen_direction(end - origin) else { continue };
        let score = direction.dot(travel).abs();
        if best.is_none_or(|(nearest, _)| score > nearest) {
            best = Some((score, AxisHandle { axis, tip: origin, direction }));
        }
    }
    best.map(|(_, handle)| handle)
}

/// The allowed axis a carried delta travels furthest along, if one moves at all.
///
/// A piece that may travel on more than one axis has no single direction to measure a gap by. The
/// exact-distance fields follow the axis the hand carries it furthest along, which is the one whose
/// face a typed number is about; the carry itself moves on every allowed axis at once.
pub(crate) fn dominant_axis(delta: DVec3, axes: [bool; 3]) -> Option<Axis> {
    let mut best: Option<(f64, Axis)> = None;
    for axis in Axis::ALL {
        if !axis_allowed(axes, axis) {
            continue;
        }
        let reach = axis.component(delta).abs();
        if reach.is_finite() && reach > 0.0 && best.is_none_or(|(widest, _)| reach > widest) {
            best = Some((reach, axis));
        }
    }
    best.map(|(_, axis)| axis)
}

/// Whether the axes let a hand carry a piece in more than one direction at once.
///
/// A panel offers exactly one axis and keeps the carry the hand's travel names (N22, ADR 0147); a
/// product, a body or a group offers two or three and is carried on the camera's own plane (X74).
pub(crate) fn planar(axes: [bool; 3]) -> bool {
    axes.into_iter().filter(|allowed| *allowed).count() > 1
}

/// The screen frame a carried piece's hand travel is read in: the axis' unit direction at the
/// pivot, and the millimetres along the axis one point of travel in it means.
///
/// The piece travels along a world line, and this is the inverse of how long that travel looks on
/// screen: one point of the hand is however many millimetres the axis needs to move the piece by one
/// point. The object move is carried by the hand (N24), so its distance is measured in points of
/// the hand and never by projecting a ray onto the line — that projection is unstable away from the
/// line (X65) and its slope at the pointer is not the piece's own.
pub(crate) fn travel_frame(
    camera: Camera,
    viewport: egui::Rect,
    origin_world: DVec3,
    axis: Axis,
) -> Option<(Vec2, f64)> {
    // An axis that points at the camera has neither a direction on screen nor a distance to travel.
    if axis.direction().dot(camera.forward()).abs() > VIEW_ALIGNMENT_LIMIT {
        return None;
    }
    let origin = screen_point(camera, viewport, origin_world)?;
    let step = (camera.distance() * 0.01).max(1.0);
    let along = screen_point(camera, viewport, origin_world + axis.direction() * step)?;
    let direction = screen_direction(along - origin)?;
    let on_screen = f64::from((along - origin).length());
    let per_point = step / on_screen;
    (per_point.is_finite() && per_point.abs() > f64::EPSILON).then_some((direction, per_point))
}

/// The hit radius of a handle of `length` points.
fn hit_radius(length: f32) -> f32 {
    (length.max(0.0) * HIT_RADIUS_FRACTION).clamp(MIN_HIT_RADIUS, HIT_RADIUS)
}

/// The stretch next to the origin of a handle of `length` points that starts no drag.
fn origin_dead_zone(length: f32) -> f32 {
    (length.max(0.0) * ORIGIN_DEAD_ZONE_FRACTION).min(MAX_ORIGIN_DEAD_ZONE)
}

/// Distance from `point` to the segment, or `None` inside the dead zone next to the origin.
fn distance_to_handle(point: Pos2, start: Pos2, end: Pos2, dead_zone: f32) -> Option<f32> {
    let segment = end - start;
    let length_squared = segment.length_sq();
    if length_squared <= f32::EPSILON {
        return None;
    }
    let length = length_squared.sqrt();
    let along = ((point - start).dot(segment) / length_squared).clamp(0.0, 1.0);
    if along * length < dead_zone {
        return None;
    }
    Some((point - (start + segment * along)).length())
}

/// Draws three axis handles at a world point.
///
/// `hover` is the axis under the pointer and `active` the axis being dragged. While a drag is
/// active the other axes fade, so the user can see which one owns the pointer.
pub(crate) fn paint_axis_handles(
    ui: &Ui,
    viewport: egui::Rect,
    camera: Camera,
    origin_world: DVec3,
    hover: Option<Axis>,
    active: Option<Axis>,
    spec: HandleSpec,
) {
    if !spec.axes.iter().any(|allowed| *allowed) {
        return;
    }
    let Some(origin) = screen_point(camera, viewport, origin_world) else {
        return;
    };
    if !viewport.expand(64.0).contains(origin) {
        return;
    }
    let handles = axis_handles(camera, viewport, origin_world, origin, spec);
    let painter = ui.painter().with_clip_rect(viewport);
    let center_tone = if active.is_some() { Color32::from_gray(140) } else { Color32::from_gray(220) };
    painter.circle_filled(origin, 3.0, center_tone);

    for handle in handles {
        let is_active = active == Some(handle.axis);
        let hovered = hover == Some(handle.axis);
        let mut color = if is_active || hovered { ACTIVE_COLOR } else { handle.axis.color() };
        if active.is_some() && !is_active {
            color = color.gamma_multiply(0.4);
        }
        let width = if is_active || hovered { 3.5 } else { 2.2 };
        painter.line_segment([origin, handle.tip], Stroke::new(width, color));

        // The head and the label shrink with the handle, so a small gizmo is a small gizmo and not
        // a short line under an arrow that swallowed it (U86).
        let arrow = (spec.length.max(0.0) * ARROW_LENGTH_FRACTION).clamp(MIN_ARROW_LENGTH, MAX_ARROW_LENGTH);
        let perpendicular = Vec2::new(-handle.direction.y, handle.direction.x);
        let base = handle.tip - handle.direction * arrow;
        painter.add(Shape::convex_polygon(
            vec![handle.tip, base + perpendicular * (arrow * 0.45), base - perpendicular * (arrow * 0.45)],
            color,
            Stroke::NONE,
        ));
        let label = handle.tip + handle.direction * (arrow + 3.0);
        painter.text(
            label,
            Align2::CENTER_CENTER,
            handle.axis.label(),
            FontId::proportional(12.0),
            if is_active || hovered { ACTIVE_COLOR } else { Color32::from_gray(230) },
        );
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use model::{
        CameraProjection, ClassCode, Container, Envelope, Geometry, LightKind, Panel, Project, PropertyValue,
        SYSTEM_CLASS,
    };
    use testkit::{millimetres, sample_panel_nominal_size, sample_panel_thickness, sample_project};

    use super::*;

    /// A project with a turned container that holds a panel, next to a loose panel.
    ///
    /// Root (1) ├ Cabinet (2), turned a quarter about Z, at x = 100 ─ Inner panel (4)
    ///          └ Outer panel (3)
    fn nested_project() -> Project {
        let size = sample_panel_nominal_size();
        let thickness = sample_panel_thickness();
        let panel = |id| {
            Panel::new(Id::new(id).expect("id"), "Panel", size, None, thickness, Placement::IDENTITY).expect("panel")
        };
        let cabinet = Container::new(
            Id::new(2).expect("id"),
            "Cabinet",
            Placement::new(DVec3::new(100.0, 0.0, 0.0), DQuat::from_rotation_z(std::f64::consts::FRAC_PI_2))
                .expect("placement"),
            vec![panel(4).into()],
        );
        let root =
            Container::new(Id::new(1).expect("id"), "Root", Placement::IDENTITY, vec![cabinet.into(), panel(3).into()]);
        Project::new(root).expect("project")
    }

    fn id(number: u64) -> Id {
        Id::new(number).expect("id")
    }

    fn assert_vector(actual: DVec3, expected: DVec3, what: &str) {
        assert!((actual - expected).length() < 1.0e-9, "{what}: actual {actual:?}, expected {expected:?}");
    }

    /// An orthographic camera snapped to the standard view that looks from `eye` towards the target.
    fn orthographic_view(eye: DVec3) -> Camera {
        let mut camera = Camera::default();
        camera.look_from(eye);
        camera.toggle_projection();
        camera
    }

    fn target<'a>(project: &'a Project, selection: &'a [Id]) -> Target<'a> {
        Target {
            camera: Camera::default(),
            project,
            contact: &engine::NoContact,
            selection,
            moves: MoveSettings::default(),
            move_kind: MoveKind::Free,
            rotate_kind: RotateKind::Free,
        }
    }

    /// The handle length the tests draw and grab with, in points.
    const TEST_HANDLE_LENGTH: f32 = 56.0;

    /// A handle spec with the test length and the given axes.
    fn spec(axes: [bool; 3]) -> HandleSpec {
        HandleSpec { axes, length: TEST_HANDLE_LENGTH }
    }

    fn viewport() -> egui::Rect {
        egui::Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))
    }

    fn origin_screen(camera: Camera) -> Pos2 {
        screen_point(camera, viewport(), camera.target()).expect("the target is in front")
    }

    #[test]
    fn a_hand_travel_names_the_axis_it_points_along() {
        // N24: the object move draws no handle, so the axis is read from the pointer's own
        // travel. The default view projects Z up and X to the right, which is what makes the two
        // travels below unambiguous.
        let camera = Camera::default();
        let origin = camera.target();
        let handles = axis_handles(camera, viewport(), origin, origin_screen(camera), spec(ALL_AXES));
        let direction = |axis: Axis| {
            handles.iter().find(|handle| handle.axis == axis).map(|handle| handle.direction).expect("a drawn axis")
        };
        for axis in Axis::ALL {
            let travel = direction(axis) * 40.0;
            assert_eq!(
                named_axis(camera, viewport(), origin, travel, ALL_AXES).map(|handle| handle.axis),
                Some(axis),
                "a travel along {axis:?} names {axis:?}"
            );
            // The same travel, backwards: the axis is the same and the sign comes from the hand.
            assert_eq!(
                named_axis(camera, viewport(), origin, -travel, ALL_AXES).map(|handle| handle.axis),
                Some(axis),
                "and so does its opposite"
            );
        }
        // The handle carries the axis' own screen direction, which the carry measures the hand by.
        let named = named_axis(camera, viewport(), origin, direction(Axis::Z) * 40.0, ALL_AXES).expect("an axis");
        assert!(
            (named.direction - direction(Axis::Z)).length() < 1.0e-5,
            "the named handle keeps the projected direction"
        );
        // Only one axis is offered, so the hand's travel is not consulted at all.
        let travel = direction(Axis::Z) * 40.0;
        assert_eq!(
            named_axis(camera, viewport(), origin, travel, [true, false, false]).map(|handle| handle.axis),
            Some(Axis::X)
        );
        // Nothing offered, nothing named.
        assert!(named_axis(camera, viewport(), origin, travel, [false; 3]).is_none(), "no axis is offered");
        // A hand that has not moved says nothing, so a taken piece never jumps on its first frame.
        assert!(named_axis(camera, viewport(), origin, Vec2::ZERO, ALL_AXES).is_none(), "a still hand names nothing");
        assert!(named_axis(camera, viewport(), origin, Vec2::new(0.3, 0.0), ALL_AXES).is_none(), "so does a stray");
    }

    #[test]
    fn the_travel_is_the_hand_s_own_and_the_ray_s_is_unstable() {
        // X65: a carried piece is measured by the hand's screen travel along the axis, scaled by how
        // long a millimetre of that axis looks. The defect this replaces was the ray's projection
        // onto the axis line, which ran away as soon as the pointer strayed sideways from that line.
        let camera = Camera::default();
        let origin = camera.target();
        let handles = axis_handles(camera, viewport(), origin, origin_screen(camera), spec(ALL_AXES));
        let direction =
            |axis: Axis| handles.iter().find(|h| h.axis == axis).map(|h| h.direction).expect("a drawn axis");
        let (screen, per_point) = travel_frame(camera, viewport(), origin, Axis::Z).expect("Z has a frame");
        assert!((screen - direction(Axis::Z)).length() < 1.0e-5, "the frame keeps the axis' screen direction");
        assert!(per_point > 0.0, "up the screen is up the world in the default view: {per_point}");

        // The scale is the axis' own, so one point of the hand is one point of the piece: the
        // millimetres it names move the piece exactly that far on screen, whatever the pointer does.
        let near = origin_screen(camera);
        let far = screen_point(camera, viewport(), origin + Axis::Z.direction() * per_point).expect("on screen");
        assert!(
            (f64::from((far - near).length()) - 1.0).abs() < 0.05,
            "one point of the hand is {} points of the piece",
            (far - near).length()
        );

        // The ray's own projection onto that line is what used to be read, and a hand that strays
        // sideways moves it by much more than the piece could have moved.
        let sideways = Vec2::new(-screen.y, screen.x);
        let parameter =
            |pointer: Pos2| ray_to(pointer, viewport(), camera).closest_parameter_on_line(origin, Axis::Z.direction());
        let from = parameter(near).expect("a ray through the axis");
        let jumped = parameter(near + sideways * 200.0).expect("a ray through the axis") - from;
        assert!(jumped.abs() > 10.0, "the ray's projection moves {jumped} mm for a sideways move of the hand");

        // An axis that points at the camera has no frame at all.
        let top = orthographic_view(DVec3::new(0.0, 0.0, 1.0));
        assert!(travel_frame(top, viewport(), top.target(), Axis::Z).is_none());
    }

    #[test]
    fn the_default_view_draws_three_distinct_handles() {
        let camera = Camera::default();
        let handles = axis_handles(camera, viewport(), camera.target(), origin_screen(camera), spec(ALL_AXES));
        assert_eq!(handles.len(), 3);
        for handle in &handles {
            assert!(handle.tip.is_finite());
            assert!((handle.direction.length() - 1.0).abs() < 1.0e-5);
            assert!(((handle.tip - origin_screen(camera)).length() - TEST_HANDLE_LENGTH).abs() < 0.5);
        }
        let z = handles.iter().find(|handle| handle.axis == Axis::Z).expect("Z handle");
        assert!(z.direction.y < -0.5, "Z should point up, direction was {:?}", z.direction);
    }

    #[test]
    fn a_view_aligned_axis_has_no_handle() {
        let mut camera = Camera::default();
        camera.look_from(DVec3::Z);
        let handles = axis_handles(camera, viewport(), camera.target(), origin_screen(camera), spec(ALL_AXES));
        assert!(handles.iter().all(|handle| handle.axis != Axis::Z), "handles were {handles:?}");
        assert_eq!(handles.len(), 2);
    }

    #[test]
    fn the_handle_nearest_the_pointer_wins_and_the_origin_is_dead() {
        let camera = Camera::default();
        let origin_world = camera.target();
        let origin = origin_screen(camera);
        let handles = axis_handles(camera, viewport(), origin_world, origin, spec(ALL_AXES));
        let x = handles.iter().find(|handle| handle.axis == Axis::X).expect("X handle");
        let pointer = origin + (x.tip - origin) * 0.6;
        assert_eq!(hit_axis(camera, viewport(), origin_world, pointer, spec(ALL_AXES)), Some(Axis::X));
        assert_eq!(hit_axis(camera, viewport(), origin_world, origin, spec(ALL_AXES)), None);
        assert_eq!(hit_axis(camera, viewport(), origin_world, origin + Vec2::new(300.0, 300.0), spec(ALL_AXES)), None);
    }

    #[test]
    fn the_handle_length_sets_both_the_tip_and_the_hit_radius() {
        let camera = Camera::default();
        let origin_world = camera.target();
        let origin = origin_screen(camera);
        let short = axis_handles(camera, viewport(), origin_world, origin, HandleSpec { axes: ALL_AXES, length: 8.0 });
        let x = short.iter().find(|handle| handle.axis == Axis::X).expect("X handle");
        let near = origin + (x.tip - origin) * 0.6;
        assert_eq!(
            hit_axis(camera, viewport(), origin_world, near, HandleSpec { axes: ALL_AXES, length: 8.0 }),
            Some(Axis::X),
            "a pointer on the short handle still grabs it"
        );

        let beyond = origin + x.direction * 40.0;
        assert_eq!(
            hit_axis(camera, viewport(), origin_world, beyond, HandleSpec { axes: ALL_AXES, length: 8.0 }),
            None,
            "the short handle ends at its tip, and its hit radius is a share of it"
        );
        assert_eq!(
            hit_axis(camera, viewport(), origin_world, beyond, HandleSpec { axes: ALL_AXES, length: 400.0 }),
            Some(Axis::X),
            "the long handle reaches the same pointer, so drawing and picking share one length"
        );
    }

    #[test]
    fn a_selection_moves_only_its_top_most_nodes() {
        let project = nested_project();
        let (cabinet, outer, inner) = (id(2), id(3), id(4));

        let targets = move_targets(&project, &[cabinet, inner, outer]);
        assert_eq!(targets.len(), 2, "the inner panel is covered by its cabinet");
        assert_eq!(targets[0].node, cabinet);
        assert_eq!(targets[1].node, outer);

        assert_eq!(move_targets(&project, &[inner]).len(), 1);
        assert!(move_targets(&project, &[project.root().id()]).is_empty(), "the root has no placement to edit");
        assert!(move_targets(&project, &[]).is_empty());
        assert!(move_targets(&project, &[id(99)]).is_empty());
    }

    #[test]
    fn a_selected_camera_and_light_are_move_targets() {
        let mut project = sample_project();
        let root = project.root().id();
        let camera =
            project.add_camera(root, "Camera", Placement::IDENTITY, CameraProjection::DEFAULT).expect("camera");
        let light = project.add_light(root, "Key", Placement::IDENTITY, LightKind::Sun, [1.0; 3], 1.0).expect("light");

        let targets = move_targets(&project, &[camera, light]);
        assert_eq!(
            targets.iter().map(|target| target.node).collect::<Vec<_>>(),
            vec![camera, light],
            "a camera and a light carry a placement, so the gizmo moves them"
        );
        assert_eq!(target(&project, &[camera]).anchor(), Some(camera));
        assert!(
            target(&project, &[camera]).drawables().is_empty(),
            "an icon stands for itself and has no geometry to preview"
        );
    }

    #[test]
    fn a_preview_moves_a_camera_or_light_world_placement_like_a_drawable() {
        let turn = DQuat::from_rotation_y(std::f64::consts::FRAC_PI_2);
        let start = Placement::new(DVec3::new(100.0, 200.0, 300.0), turn).expect("placement");

        let move_delta = DVec3::new(10.0, -20.0, 5.0);
        let moved = Preview::Translation { drawables: &[], delta: move_delta }
            .moved_world(start)
            .expect("a translated placement");
        assert_eq!(moved.translation(), DVec3::new(110.0, 180.0, 305.0));
        assert_eq!(moved.rotation(), turn, "a translation keeps the rotation");

        let pivot = DVec3::new(50.0, 0.0, 0.0);
        let z_turn = DQuat::from_rotation_z(std::f64::consts::FRAC_PI_2);
        let turned = Preview::Rotation { drawables: &[], pivot, rotation: z_turn }
            .moved_world(start)
            .expect("a turned placement");
        let expected = pivot + z_turn * (start.translation() - pivot);
        assert!((turned.translation() - expected).length() < 1.0e-9);
        assert!(
            turned.rotation().dot(z_turn * turn).abs() > 1.0 - 1.0e-9,
            "a world turn is pre-multiplied onto the stored rotation"
        );
    }

    #[test]
    fn a_target_carries_the_parent_frame_and_the_stored_pivot() {
        let mut project = nested_project();
        let cabinet = id(2);
        project.center_node_pivot(cabinet).expect("centre the cabinet pivot");
        let targets = move_targets(&project, &[cabinet]);
        let target = targets.first().copied().expect("one target");

        assert_eq!(target.start_local, project.node(cabinet).expect("node").placement());
        assert_eq!(target.parent, project.world_placement(project.root().id()).expect("root world"));
        assert_eq!(target.pivot_local, project.pivot_local_mm(cabinet).expect("pivot"));
        assert_ne!(target.pivot_local, DVec3::ZERO, "the target carries the authored pivot");
    }

    #[test]
    fn the_gizmo_anchors_on_the_last_movable_node_and_lists_its_drawables() {
        let project = nested_project();
        let (cabinet, outer, inner) = (id(2), id(3), id(4));
        let selection = [outer, cabinet, inner];
        let gizmo_target = target(&project, &selection);

        assert_eq!(gizmo_target.anchor(), Some(cabinet), "the last movable node anchors the gizmo");
        assert_eq!(gizmo_target.anchor_pivot(), project.pivot_world(cabinet));
        assert_eq!(gizmo_target.drawables(), vec![outer, inner], "each drawable is listed once, in target order");

        let inner_only = [inner];
        let leaf_target = target(&project, &inner_only);
        assert_eq!(leaf_target.anchor(), Some(inner), "a selected leaf is its own anchor");
        assert_eq!(leaf_target.drawables(), vec![inner]);
    }

    #[test]
    fn a_world_shift_moves_every_target_rigidly_through_its_own_frame() {
        let project = nested_project();
        let selection = [id(4), id(3)];
        let targets = move_targets(&project, &selection);
        let delta = DVec3::new(10.0, 4.0, -2.0);

        let commit = commit_translation(&targets, delta, true).expect("valid").expect("a move");
        assert_eq!(commit.changes.len(), 2);
        for (change, target) in commit.changes.iter().zip(&targets) {
            let before = target.parent.transform_point(target.start_local.translation());
            let after = target.parent.transform_point(change.placement.translation());
            assert!((after - before - delta).length() < 1.0e-9, "world shift was {:?}", after - before);
            assert_eq!(change.placement.rotation(), target.start_local.rotation(), "a move keeps the rotation");
        }
    }

    #[test]
    fn a_world_turn_keeps_the_anchor_pivot_fixed_and_orbits_the_rest() {
        let project = nested_project();
        let selection = [id(4), id(3)];
        let targets = move_targets(&project, &selection);
        let anchor = targets.last().copied().expect("anchor target");
        let pivot = project.pivot_world(anchor.node).expect("anchor pivot");
        assert_eq!(pivot, anchor.start_local.transform_point(anchor.pivot_local));
        let turn = DQuat::from_axis_angle(DVec3::Z, std::f64::consts::FRAC_PI_2);

        let commit = commit_rotation(&targets, pivot, turn, false).expect("valid").expect("a turn");
        assert_eq!(commit.changes.len(), 2);
        for (change, target) in commit.changes.iter().zip(&targets) {
            let before = target.parent.transform_point(target.start_local.translation());
            let after = target.parent.transform_point(change.placement.translation());
            let expected = pivot + turn * (before - pivot);
            assert!((after - expected).length() < 1.0e-6, "node {} landed at {after:?}", target.node);
        }
        let anchor_after = project
            .world_placement(project.root().id())
            .expect("root world")
            .transform_point(commit.changes[1].placement.transform_point(anchor.pivot_local));
        assert!((anchor_after - pivot).length() < 1.0e-6, "the anchor pivot moved to {anchor_after:?}");
    }

    #[test]
    fn a_drag_that_changes_nothing_returns_no_commit() {
        let project = nested_project();
        let targets = move_targets(&project, &[id(4), id(3)]);

        assert_eq!(commit_translation(&targets, DVec3::ZERO, true).expect("no error"), None);
        assert_eq!(commit_rotation(&targets, DVec3::ZERO, DQuat::IDENTITY, false).expect("no error"), None);
        assert_eq!(commit_translation(&[], DVec3::X, true).expect("no error"), None);
        assert_eq!(commit_rotation(&[], DVec3::ZERO, DQuat::from_rotation_z(0.5), false).expect("no error"), None);
    }

    #[test]
    fn axes_have_distinct_directions_and_colours() {
        assert_eq!(Axis::ALL.len(), 3);
        assert_eq!(Axis::X.direction(), DVec3::X);
        assert_eq!(Axis::Y.direction(), DVec3::Y);
        assert_eq!(Axis::Z.direction(), DVec3::Z);
        assert_ne!(Axis::X.color(), Axis::Y.color());
        assert_ne!(Axis::Y.color(), Axis::Z.color());
    }

    #[test]
    fn a_ray_meets_a_plane_only_in_front_of_the_camera() {
        let ray = Ray::new(DVec3::new(0.0, 0.0, 10.0), -DVec3::Z).expect("ray");
        let hit = ray_plane(ray, DVec3::ZERO, DVec3::Z).expect("hit");
        assert!(hit.length() < 1.0e-12);

        let behind = Ray::new(DVec3::new(0.0, 0.0, 10.0), DVec3::Z).expect("ray");
        assert!(ray_plane(behind, DVec3::ZERO, DVec3::Z).is_none());

        let parallel = Ray::new(DVec3::new(0.0, 0.0, 10.0), DVec3::X).expect("ray");
        assert!(ray_plane(parallel, DVec3::ZERO, DVec3::Z).is_none());
    }

    #[test]
    fn a_carried_piece_is_met_on_the_plane_its_class_allows() {
        // X75: the plane decides whether the piece stays under the hand. Two allowed axes are met
        // across the axis the class refuses — the floor, for a floor-bound product — and only a piece
        // with all three free is met on the camera's own plane.
        assert_eq!(CarryPlane::of([true, true, false]), CarryPlane::Across(Axis::Z));
        assert_eq!(CarryPlane::of([false, true, true]), CarryPlane::Across(Axis::X));
        assert_eq!(CarryPlane::of([true, false, true]), CarryPlane::Across(Axis::Y));
        assert_eq!(CarryPlane::of([true, true, true]), CarryPlane::Camera);

        // The pointer meets the floor at the point a maker sees there, so the piece's own screen
        // travel is the hand's: the projection of the hit is the pointer again.
        let camera = Camera::default();
        let viewport = egui::Rect::from_min_size(Pos2::ZERO, egui::Vec2::new(800.0, 600.0));
        let origin = DVec3::new(100.0, -200.0, 0.0);
        for pointer in [Pos2::new(400.0, 300.0), Pos2::new(260.0, 520.0)] {
            let hit =
                CarryPlane::Across(Axis::Z).point(pointer, viewport, camera, origin).expect("the hand meets the floor");
            assert!(hit.z.abs() < 1.0e-9, "the floor keeps the height: {hit:?}");
            let shown = screen_point(camera, viewport, hit).expect("the hit is in front of the camera");
            assert!((shown - pointer).length() < 0.5, "the hand is on the piece: {shown:?} vs {pointer:?}");
        }
        // A camera turned away from the plane leaves the plane's own point behind the ray: the answer
        // is `None`, and the carry keeps the distance it has instead of throwing the piece out.
        let mut behind = Camera::default();
        behind.orbit(std::f64::consts::PI, 0.0);
        let above = origin + DVec3::Z * 5000.0;
        assert!(CarryPlane::Across(Axis::Z).point(Pos2::new(400.0, 300.0), viewport, behind, above).is_none());
    }

    #[test]
    fn the_six_standard_views_select_their_world_plane() {
        let cases = [
            (DVec3::X, DVec3::X, DVec3::Y, DVec3::Z, "YZ"),
            (-DVec3::X, -DVec3::X, -DVec3::Y, DVec3::Z, "YZ"),
            (DVec3::Y, DVec3::Y, -DVec3::X, DVec3::Z, "XZ"),
            (-DVec3::Y, -DVec3::Y, DVec3::X, DVec3::Z, "XZ"),
            (DVec3::Z, DVec3::Z, DVec3::X, DVec3::Y, "XY"),
            (-DVec3::Z, -DVec3::Z, DVec3::X, -DVec3::Y, "XY"),
        ];
        for (eye, normal, u, v, label) in cases {
            let camera = orthographic_view(eye);
            if eye.z > 0.5 {
                assert_eq!(camera.pitch(), MAX_PITCH, "a top view reaches the pole");
            } else if eye.z < -0.5 {
                assert_eq!(camera.pitch(), MIN_PITCH, "a bottom view reaches the pole");
            }
            let plane = WorkPlane::from_camera(camera);
            assert_eq!(plane.label(), label, "looking from {eye:?}");
            assert_vector(plane.normal(), normal, "the plane normal");
            assert_vector(plane.world(DVec2::new(1.0, 0.0)), u, "the u axis");
            assert_vector(plane.world(DVec2::new(0.0, 1.0)), v, "the v axis");
            assert_vector(u.cross(v), normal, "a right-handed basis");
        }
    }

    #[test]
    fn a_perspective_or_axonometric_view_draws_on_world_xy() {
        assert_eq!(WorkPlane::from_camera(Camera::default()), WorkPlane::WORLD_XY);

        let mut axonometric = Camera::default();
        axonometric.look_from(DVec3::new(1.0, 1.0, 1.0));
        axonometric.toggle_projection();
        assert_eq!(WorkPlane::from_camera(axonometric), WorkPlane::WORLD_XY);

        let mut orbited = orthographic_view(DVec3::Y);
        orbited.orbit(0.2, 0.0);
        assert_eq!(WorkPlane::from_camera(orbited), WorkPlane::WORLD_XY, "a view off the axis is axonometric");

        let mut tilted = orthographic_view(DVec3::X);
        tilted.orbit(0.0, 0.2);
        assert_eq!(WorkPlane::from_camera(tilted), WorkPlane::WORLD_XY);
    }

    #[test]
    fn a_pointer_maps_to_the_work_plane_and_back() {
        for eye in [DVec3::X, -DVec3::X, DVec3::Y, -DVec3::Y, DVec3::Z, -DVec3::Z] {
            let camera = orthographic_view(eye);
            let plane = WorkPlane::from_camera(camera);
            for local in [DVec2::ZERO, DVec2::new(120.0, -45.0), DVec2::new(-300.0, 75.0)] {
                let world = plane.world(local);
                let pixel = screen_point(camera, viewport(), world).expect("the point is in front");
                let mapped = plane.pointer_point(camera, viewport(), pixel).expect("the ray meets the plane");
                assert!((mapped - local).length() < 1.0e-3, "from {eye:?}: {mapped:?} != {local:?}");
            }
        }

        let camera = Camera::default();
        let plane = WorkPlane::from_camera(camera);
        let world = DVec3::new(100.0, 50.0, 0.0);
        let pixel = screen_point(camera, viewport(), world).expect("the point is in front");
        let mapped = plane.pointer_point(camera, viewport(), pixel).expect("the ray meets the plane");
        assert!((mapped - DVec2::new(100.0, 50.0)).length() < 1.0e-3, "a perspective view lands on world XY");
    }

    #[test]
    fn a_reference_is_projected_from_the_work_plane() {
        // In perspective the work-plane circle projects to an ellipse and the plane's X direction
        // leaves the screen's horizontal. A guide drawn from the plane's angles in screen space would
        // be a circle struck from the anchor and an axis along the screen (ADR 0133).
        let camera = Camera::default();
        let plane = WorkPlane::from_camera(camera);
        let rect = viewport();
        let reference =
            ui::Reference { anchor: DVec2::ZERO, radius: 200.0, from: 0.0, to: std::f64::consts::FRAC_PI_2 };
        let project = |point| screen_point(camera, rect, plane.world(point));
        let (axis, sweep) = reference_screen(reference, project);
        assert_eq!(axis.len(), 2, "the axis is its two projected ends");
        assert_eq!(sweep.len(), 91, "a quarter turn is one sample per degree");

        // Every sample is the projection of a point of the plane's own circle.
        let (_, samples) = reference.samples();
        for (screen, plane_point) in sweep.iter().zip(samples) {
            let expected = screen_point(camera, rect, plane.world(plane_point)).expect("a point in front");
            assert!((*screen - expected).length() < 1e-6, "{screen:?} is the projection of {plane_point:?}");
        }

        // The screen radius changes along the sweep, so the arc is an ellipse and not a circle.
        let anchor = axis[0];
        let radii = sweep.iter().map(|point| (*point - anchor).length());
        let (min, max) = radii.fold((f32::INFINITY, 0.0_f32), |(min, max), radius| (min.min(radius), max.max(radius)));
        assert!(max / min > 1.05, "the plane circle projects to an ellipse: {min}..{max}");

        // The plane's X direction leaves the screen's own horizontal.
        let end = axis[1];
        let screen_angle = (end.y - anchor.y).atan2(end.x - anchor.x);
        assert!(screen_angle.abs() > 0.02, "the projected X axis is not the screen axis: {screen_angle}");
    }

    #[test]
    fn a_work_plane_placement_maps_local_xy_onto_the_plane() {
        for eye in [DVec3::X, -DVec3::X, DVec3::Y, -DVec3::Y, DVec3::Z, -DVec3::Z, DVec3::new(1.0, 1.0, 1.0)] {
            let camera = if eye == DVec3::new(1.0, 1.0, 1.0) {
                let mut camera = Camera::default();
                camera.look_from(eye);
                camera
            } else {
                orthographic_view(eye)
            };
            let plane = WorkPlane::from_camera(camera);
            let placement = plane.placement().expect("a plane has a rigid placement");
            assert_vector(placement.transform_point(DVec3::ZERO), DVec3::ZERO, "the plane runs through the origin");
            assert_vector(placement.transform_point(DVec3::X), plane.u, "local X is the plane u axis");
            assert_vector(placement.transform_point(DVec3::Y), plane.v, "local Y is the plane v axis");
            assert_vector(placement.transform_point(DVec3::Z), plane.normal(), "local Z is the plane normal");
        }
    }

    #[test]
    fn a_work_plane_placement_at_a_point_moves_the_origin_only() {
        let camera = orthographic_view(DVec3::Z);
        let plane = WorkPlane::from_camera(camera);
        let local = DVec2::new(120.0, -45.0);
        let placement = plane.placement_at(local).expect("a plane point has a placement");
        assert_vector(placement.transform_point(DVec3::ZERO), DVec3::new(120.0, -45.0, 0.0), "the origin is the click");
        assert_vector(placement.transform_point(DVec3::X), DVec3::new(121.0, -45.0, 0.0), "local X is the plane u");
        assert_vector(placement.transform_point(DVec3::Y), DVec3::new(120.0, -44.0, 0.0), "local Y is the plane v");
    }

    #[test]
    fn a_work_plane_behind_or_parallel_to_the_ray_has_no_point() {
        let plane = WorkPlane::WORLD_XY;
        let behind = Ray::new(DVec3::new(0.0, 0.0, 10.0), DVec3::Z).expect("ray");
        assert!(plane.ray_point(behind).is_none());
        let parallel = Ray::new(DVec3::new(0.0, 0.0, 10.0), DVec3::X).expect("ray");
        assert!(plane.ray_point(parallel).is_none());
    }

    #[test]
    fn every_snap_kind_draws_a_glyph_of_its_own() {
        // Ten kinds, ten glyphs: a maker learns the shapes once and reads them on every tool. A
        // glyph is signed by the primitives it draws and where they land, so a kind that painted
        // nothing, or the same picture as its neighbour, is the failure this test names (U116).
        let mut drawn = std::collections::BTreeSet::new();
        for kind in model::SnapKind::ALL {
            let mut output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
                paint_snap_marker(ui.painter(), Pos2::new(10.0, 10.0), Some(kind), false, Color32::WHITE);
            });
            output.textures_delta.clear();
            let signature = glyph(&output.shapes.iter().map(|clipped| &clipped.shape).collect::<Vec<_>>());
            assert!(!signature.is_empty(), "{kind:?} draws nothing");
            assert!(drawn.insert(signature.clone()), "{kind:?} draws the glyph of another kind: {signature:?}");
        }
        assert_eq!(drawn.len(), model::SnapKind::ALL.len(), "one glyph per kind");

        // The constraint the hand states draws the diamond the draft always drew.
        let mut output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
            paint_snap_marker(ui.painter(), Pos2::new(10.0, 10.0), None, true, Color32::WHITE);
        });
        output.textures_delta.clear();
        assert!(!glyph(&output.shapes.iter().map(|clipped| &clipped.shape).collect::<Vec<_>>()).is_empty());
    }

    /// The primitives a glyph draws and where they land: two glyphs differ when one number differs.
    fn glyph(shapes: &[&egui::Shape]) -> Vec<String> {
        let mut out = Vec::new();
        for shape in shapes {
            if let egui::Shape::Vec(inner) = shape {
                out.extend(glyph(&inner.iter().collect::<Vec<_>>()));
                continue;
            }
            let name = match shape {
                egui::Shape::Rect(_) => "rect",
                egui::Shape::Circle(_) => "circle",
                egui::Shape::LineSegment { .. } => "line",
                egui::Shape::Path(_) => "path",
                _ => "other",
            };
            let rect = shape.visual_bounding_rect();
            out.push(format!("{name} {:.1},{:.1},{:.1},{:.1}", rect.min.x, rect.min.y, rect.max.x, rect.max.y));
        }
        out
    }

    #[test]
    fn screen_segments_reject_degenerate_input() {
        assert!(distance_to_segment(Pos2::ZERO, Pos2::ZERO, Pos2::ZERO).is_none());
        assert_eq!(distance_to_segment(Pos2::new(0.0, 3.0), Pos2::ZERO, Pos2::new(10.0, 0.0)), Some(3.0));
        assert!(screen_direction(Vec2::ZERO).is_none());
        assert_eq!(screen_direction(Vec2::new(0.0, 4.0)), Some(Vec2::new(0.0, 1.0)));
    }
    /// A sample project with one classed cube at the root, and the cube's id.
    fn classed_cube(code: &str) -> (Project, Id) {
        let mut project = sample_project();
        let root = project.root().id();
        let mesh = testkit::cube_mesh(millimetres(100));
        let object = project.add_object(root, "Cube", Placement::IDENTITY, Geometry::Mesh(mesh)).expect("object");
        if !code.is_empty() {
            project
                .set_property(object, SYSTEM_CLASS, PropertyValue::class(code.parse().expect("class code")))
                .expect("class the cube");
        }
        (project, object)
    }

    #[test]
    fn a_product_drag_carries_its_envelope_in_the_preview() {
        let mut project = sample_project();
        let root = project.root().id();
        let envelope = Envelope::new(millimetres(600), millimetres(560), millimetres(720)).expect("a valid envelope");
        let class: ClassCode = "10.01.01".parse().expect("a base cabinet");
        let product = project.add_product(root, "Cabinet", class, envelope, Placement::IDENTITY).expect("a product");
        let selection = [product];
        assert!(
            target(&project, &selection).drawables().contains(&product),
            "the dashed frame is a scene item of the product's own id and must follow the drag"
        );

        // A loose panel carries its own id too; a panel is a drawable, not an envelope.
        let panel = Id::new(2).expect("id");
        let loose = [panel];
        assert_eq!(target(&project, &loose).drawables(), vec![panel]);
    }

    #[test]
    fn the_gizmo_offers_the_handles_the_class_allows() {
        let (project, cube) = classed_cube("");
        assert_eq!(editable_axes(&project, cube), ALL_AXES, "an unclassified mesh is free");
        let camera = Camera::default();
        let origin = origin_screen(camera);
        assert_eq!(axis_handles(camera, viewport(), camera.target(), origin, spec(ALL_AXES)).len(), 3);

        let (project, worktop) = classed_cube("20.01");
        let axes = editable_axes(&project, worktop);
        assert_eq!(axes, [true, false, false], "a worktop changes its length only");
        let handles = axis_handles(camera, viewport(), camera.target(), origin, spec(axes));
        assert_eq!(handles.len(), 1);
        assert_eq!(handles.first().map(|handle| handle.axis), Some(Axis::X));

        let (project, hinge) = classed_cube("50.02.01");
        assert_eq!(editable_axes(&project, hinge), [false; 3], "a hinge is never resized");
        assert!(
            axis_handles(camera, viewport(), camera.target(), origin, spec(editable_axes(&project, hinge))).is_empty()
        );
    }

    #[test]
    fn a_fixed_class_cannot_grab_a_handle() {
        let (project, hinge) = classed_cube("50.02.01");
        let camera = Camera::default();
        let origin_world = camera.target();
        let origin = origin_screen(camera);
        let free = axis_handles(camera, viewport(), origin_world, origin, spec(ALL_AXES));
        let x = free.iter().find(|handle| handle.axis == Axis::X).expect("X handle");
        let pointer = origin + (x.tip - origin) * 0.6;
        assert_eq!(hit_axis(camera, viewport(), origin_world, pointer, spec(ALL_AXES)), Some(Axis::X));
        assert_eq!(
            hit_axis(camera, viewport(), origin_world, pointer, spec(editable_axes(&project, hinge))),
            None,
            "a class with no extent offers no handle to grab"
        );
    }
}
