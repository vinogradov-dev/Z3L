//! One resolver answers what the pointer snapped to.
//!
//! Every tool that places a point asks the same question — "where did the pointer really land, and
//! what captured it?" — and the answer is the same set of kinds: a corner, a crossing, the middle of
//! an edge, a centre, a perpendicular foot, a tangent point, a point on an edge or a face, a world
//! axis and the sketch plane's grid. The candidates come from the document through its own queries,
//! the sketch plane decides the plane-local kinds, and the pointer's ray decides which candidate is
//! nearer. One atom, so two tools cannot disagree about where a corner is (ADR 0183).
//!
//! The resolver is a **measurement**, not a mutation: it reads the tree, the probe and the marks it
//! derives, and stores nothing. Its tolerance is handed in as a [`Length`], because only the camera
//! knows how many millimetres one viewport point covers at the depth the maker is working at
//! (ADR 0051).
//!
//! The kinds are ordered, and the order is the priority: a corner beats a crossing, a crossing beats
//! a midpoint, and so on down to the grid. A maker who points near both a corner and an edge means
//! the corner, even when the edge is a little nearer; the nearest candidate inside one kind wins it.
//!
//! A candidate may stand above or below the plane a tool draws on, and a maker expects two readings
//! of that: the **2.5D** snap takes such a candidate anywhere and answers its perpendicular foot, so a
//! top view lines a board up with a shadow (this is the mode a furniture maker works in), and the
//! **3D** snap answers the candidate itself and leaves it to the tool: a tool that stores a point in
//! space — the ruler, a vertex edit — takes it, and a tool that stores a point of its own plane
//! captures it only while the candidate lies in that plane. The tool declares which of the three it
//! wants ([`SnapTarget`]), the workspace says which mode it is in ([`SnapMode`]), and the resolver
//! returns the point the tool stores — never a marker separate from it (U116, T17, U119).
//!
//! [`Project::snap_marks`] is the same candidate set without the pointer: the points of the tree and
//! their kinds. The object move reads it to align a carried piece with a neighbour's centre, which is
//! the one coordinate its faces do not offer.
//!
//! A payload answers its own places and not the box around it: a panel offers its box, a swept
//! rectangle the box that body *is*, a swept circle its two rims and the two ends of its axis, a ball
//! its middle, its poles and its equator quadrants, a cone its rims, its ends and its apex, and a
//! baked mesh the box it was baked with (U120).
//!
//! Changes when: a snap kind appears or its priority changes, a document shape gains a corner, a
//! centre or an edge the pointer should see, or a snap mode appears.

use std::f64::consts::{FRAC_PI_2, TAU};

use geom::{DVec2, DVec3};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use units::Length;

use super::sight::Sight;
use crate::tree::object_bounds_mm;
use crate::{
    Constructive, Container, Contour, ContourSegment, Geometry, Id, Node, Placement, Primitive, PrimitiveShape, Project,
};

/// The step of the sketch plane's grid, in millimetres.
pub const SNAP_GRID_MM: f64 = 10.0;

/// Fewest chords an arc is sampled into, however short it is.
const MIN_ARC_SAMPLES: usize = 4;

/// Most chords an arc is sampled into: a snap needs the shape, not the tessellation.
const MAX_ARC_SAMPLES: usize = 64;

/// Tolerance for treating two directions as parallel or two points as one, in millimetres.
const DEGENERATE: f64 = 1.0e-9;

/// How far a candidate may stand off the plane and still count as lying in it, in millimetres.
///
/// The document's own quantum: a point is either in the plane or it is not, and the only slack a
/// candidate needs is the arithmetic of a placement that carried it there (U119).
const ON_PLANE_MM: f64 = 1.0e-3;

/// How much nearer two candidates must be to the pointer to count as the same place.
///
/// A box's top and bottom corners lie on one line through the eye, so they are equally near the ray;
/// the maker means the one the eye can see. A tie is broken by depth, and this is the depth equality
/// that makes two really different candidates two (U116).
const TIE: f64 = 1.0e-6;

/// What kind of point captured the pointer, in the order a pointer prefers them.
///
/// The variants are declared in priority order and [`SnapKind::ALL`] keeps it, so a resolver walks
/// the list from the corner down to the grid and the first kind that answers wins.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SnapKind {
    /// A corner of a panel, a body or a frame, a quadrant of a round body's rim, a ball's pole, or a
    /// point of the draft being drawn.
    Vertex,
    /// Where two edges cross in the sketch plane.
    Intersection,
    /// The middle of an edge.
    Midpoint,
    /// The centre of a face, a box, a primitive or a circle, or one end of a round body's axis.
    Centre,
    /// The foot of the perpendicular from the previous point onto an edge.
    Perpendicular,
    /// Where a line from the previous point touches a circle.
    Tangent,
    /// A point on an edge.
    Edge,
    /// The face under the pointer, when it lies in the sketch plane.
    Face,
    /// A point on one of the world axes, where the sketch plane holds it.
    Axis,
    /// The sketch plane's grid.
    Grid,
}

impl SnapKind {
    /// Every kind, in the order a pointer prefers them.
    pub const ALL: [Self; 10] = [
        Self::Vertex,
        Self::Intersection,
        Self::Midpoint,
        Self::Centre,
        Self::Perpendicular,
        Self::Tangent,
        Self::Edge,
        Self::Face,
        Self::Axis,
        Self::Grid,
    ];
}

/// Which kinds may capture a pointer.
///
/// The value is a workspace setting (`crates/ui/src/settings.rs` owns the file and the window), so
/// the model owns what each switch means and the default, and never stores one (ADR 0146). Every
/// switch is on in a fresh workspace: a snap the maker does not want is one switch away, and a
/// capture nobody expected is one marker away from being understood.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SnapKinds {
    /// A corner.
    pub vertex: bool,
    /// A crossing of two edges.
    pub intersection: bool,
    /// The middle of an edge.
    pub midpoint: bool,
    /// The centre of a face, a box or a circle.
    pub centre: bool,
    /// The perpendicular foot from the previous point.
    pub perpendicular: bool,
    /// The tangent point from the previous point.
    pub tangent: bool,
    /// A point on an edge.
    pub edge: bool,
    /// The face under the pointer.
    pub face: bool,
    /// The world axes.
    pub axis: bool,
    /// The sketch plane's grid.
    pub grid: bool,
}

impl SnapKinds {
    /// Every kind on: the workspace's default.
    pub const ALL: Self = Self {
        vertex: true,
        intersection: true,
        midpoint: true,
        centre: true,
        perpendicular: true,
        tangent: true,
        edge: true,
        face: true,
        axis: true,
        grid: true,
    };

    /// Every kind off: the pointer's own point, and the axis lock if the hand asks for one.
    pub const fn none() -> Self {
        Self {
            vertex: false,
            intersection: false,
            midpoint: false,
            centre: false,
            perpendicular: false,
            tangent: false,
            edge: false,
            face: false,
            axis: false,
            grid: false,
        }
    }

    /// Whether any kind is on.
    pub const fn any(self) -> bool {
        self.vertex
            || self.intersection
            || self.midpoint
            || self.centre
            || self.perpendicular
            || self.tangent
            || self.edge
            || self.face
            || self.axis
            || self.grid
    }

    /// Whether every kind is on: the state the master switch reads.
    pub const fn all_on(self) -> bool {
        self.vertex
            && self.intersection
            && self.midpoint
            && self.centre
            && self.perpendicular
            && self.tangent
            && self.edge
            && self.face
            && self.axis
            && self.grid
    }

    /// Turns every kind on or off at once: the master switch of the set.
    pub fn set_all(&mut self, on: bool) {
        *self = if on { Self::ALL } else { Self::none() };
    }

    /// Whether this kind may capture.
    pub const fn enabled(self, kind: SnapKind) -> bool {
        match kind {
            SnapKind::Vertex => self.vertex,
            SnapKind::Intersection => self.intersection,
            SnapKind::Midpoint => self.midpoint,
            SnapKind::Centre => self.centre,
            SnapKind::Perpendicular => self.perpendicular,
            SnapKind::Tangent => self.tangent,
            SnapKind::Edge => self.edge,
            SnapKind::Face => self.face,
            SnapKind::Axis => self.axis,
            SnapKind::Grid => self.grid,
        }
    }

    /// Turns one kind on or off.
    pub fn set(&mut self, kind: SnapKind, on: bool) {
        let slot = match kind {
            SnapKind::Vertex => &mut self.vertex,
            SnapKind::Intersection => &mut self.intersection,
            SnapKind::Midpoint => &mut self.midpoint,
            SnapKind::Centre => &mut self.centre,
            SnapKind::Perpendicular => &mut self.perpendicular,
            SnapKind::Tangent => &mut self.tangent,
            SnapKind::Edge => &mut self.edge,
            SnapKind::Face => &mut self.face,
            SnapKind::Axis => &mut self.axis,
            SnapKind::Grid => &mut self.grid,
        };
        *slot = on;
    }
}

impl Default for SnapKinds {
    fn default() -> Self {
        Self::ALL
    }
}

/// The pointer's reach: a world distance at the depth it was measured at.
///
/// A screen radius is what the maker sees, and the world distance it covers grows with depth, so a
/// candidate on a wall across the shop is not captured by the reach a board beside the hand needs.
#[derive(Clone, Copy, Debug)]
struct Reach {
    /// The reach at [`Reach::depth`], in millimetres.
    millimetres: f64,
    /// The depth from the eye the reach was measured at, in millimetres; zero keeps it everywhere.
    depth: f64,
}

impl Reach {
    /// The reach at a candidate `depth` millimetres from the eye.
    fn at(self, depth: f64) -> f64 {
        if self.depth <= 0.0 || !depth.is_finite() || depth <= 0.0 {
            return self.millimetres;
        }
        let scaled = self.millimetres * depth / self.depth;
        if scaled.is_finite() { scaled } else { self.millimetres }
    }
}

/// The plane a sketch draws on, in world millimetres.
///
/// `u` and `v` are orthonormal world directions and `origin` is the plane's zero, so a plane point is
/// `origin + u * x + v * y`. They are normalised on the way in: a caller that hands in the plane's
/// own basis never sees a difference, and a caller that hands in a scaled one still gets a grid step
/// measured in millimetres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SnapPlane {
    origin: DVec3,
    u: DVec3,
    v: DVec3,
    normal: DVec3,
}

impl SnapPlane {
    /// The plane through `origin` spanned by `u` and `v`, or `None` when the two are parallel.
    pub fn new(origin: DVec3, u: DVec3, v: DVec3) -> Option<Self> {
        if !origin.is_finite() || !u.is_finite() || !v.is_finite() {
            return None;
        }
        let length = u.length();
        if !length.is_finite() || length <= DEGENERATE {
            return None;
        }
        let u = u / length;
        let v = v - u * u.dot(v);
        let length = v.length();
        if !length.is_finite() || length <= DEGENERATE {
            return None;
        }
        let v = v / length;
        let normal = u.cross(v);
        if !normal.is_finite() || normal.length() <= DEGENERATE {
            return None;
        }
        Some(Self { origin, u, v, normal })
    }

    /// The world XY plane through the origin: the plane a perspective view draws on.
    pub const fn world_xy() -> Self {
        Self { origin: DVec3::ZERO, u: DVec3::X, v: DVec3::Y, normal: DVec3::Z }
    }

    /// The plane's zero point in the world.
    pub const fn origin(self) -> DVec3 {
        self.origin
    }

    /// The plane's first direction, a unit vector in the world.
    pub const fn u(self) -> DVec3 {
        self.u
    }

    /// The plane's second direction, a unit vector in the world.
    pub const fn v(self) -> DVec3 {
        self.v
    }

    /// The plane's normal, the unit vector `u × v`.
    pub const fn normal(self) -> DVec3 {
        self.normal
    }

    /// The world point at plane coordinates `local`.
    pub fn world(self, local: DVec2) -> DVec3 {
        self.origin + self.u * local.x + self.v * local.y
    }

    /// The plane coordinates of a world point.
    pub fn local(self, world: DVec3) -> DVec2 {
        let offset = world - self.origin;
        DVec2::new(offset.dot(self.u), offset.dot(self.v))
    }

    /// How far a world point stands off the plane, along the normal.
    pub fn distance(self, world: DVec3) -> f64 {
        (world - self.origin).dot(self.normal)
    }

    /// The world point where a ray meets the plane, or `None` when they are parallel or it is behind.
    pub fn intersect(self, origin: DVec3, direction: DVec3) -> Option<DVec3> {
        if !origin.is_finite() || !direction.is_finite() {
            return None;
        }
        let facing = direction.dot(self.normal);
        if facing.abs() <= DEGENERATE {
            return None;
        }
        let distance = (self.origin - origin).dot(self.normal) / facing;
        if !distance.is_finite() || distance <= 0.0 {
            return None;
        }
        let point = origin + direction * distance;
        point.is_finite().then_some(point)
    }
}

/// How a snap answers a candidate for a tool that draws on a plane.
///
/// A candidate may stand above or below that plane, and a maker expects two readings of it, so the
/// workspace carries the choice and every plane-bound tool reads the same value (U119). A tool with
/// no plane — the ruler, a vertex edit — is unaffected and always takes the point itself.
///
/// The default is the 2.5D reading, because it is the one a furniture maker works in: a top view
/// lines a board up with the shadow it casts on the drawing plane, and the drawing plane is the world
/// XY for every view but an axis-aligned orthographic one — so a 3D snap there would capture nothing
/// of the furniture standing above it. The 3D reading is the switch, and it becomes the useful one
/// once a sketch can take the plane of the face under its first click (U121).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SnapMode {
    /// 3D: the point itself. A tool that draws on a plane captures only a candidate that lies in it,
    /// so a figure is never built from a place it cannot contain.
    Space,
    /// 2.5D: a candidate above or below the plane captures, and the point is its perpendicular foot.
    /// The shadow a board casts on the drawing plane is a place on that plane, which is what a maker
    /// lines up in a top view.
    #[default]
    Foot,
}

impl SnapMode {
    /// Every mode, for the Settings row and the ribbon cell.
    pub const ALL: [Self; 2] = [Self::Space, Self::Foot];

    /// The target a plane-bound tool declares in this mode.
    pub const fn target(self) -> SnapTarget {
        match self {
            Self::Space => SnapTarget::InPlane,
            Self::Foot => SnapTarget::Foot,
        }
    }
}

/// What a tool can do with a snap answer, and so what the answer is.
///
/// The tool declares it, because only the tool knows whether its point may leave the plane it draws
/// on. The resolver returns the point the tool stores, so a caller paints its marker at
/// [`SnapAnswer::point`] and the marker names the place the click will take — one answer, one place
/// (U119).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SnapTarget {
    /// The tool takes the point where it is: the ruler and a vertex edit work in space, and the mode
    /// does not apply to them.
    Free,
    /// The tool stores a point of the request's plane and takes only a candidate in it: the 3D
    /// reading of a plane-bound tool.
    InPlane,
    /// The tool stores a point of the request's plane and takes a candidate from anywhere by its
    /// perpendicular foot: the 2.5D reading of a plane-bound tool.
    Foot,
}

impl SnapTarget {
    /// The point the tool stores for a candidate, or `None` when it is not a place this tool takes.
    ///
    /// The two plane targets answer the candidate's plane coordinates, so the answer is a point of
    /// the plane for a candidate that lies in it as well; the mode of a free tool changes nothing.
    pub fn answer(self, plane: SnapPlane, point: DVec3) -> Option<DVec3> {
        if !point.is_finite() {
            return None;
        }
        match self {
            Self::Free => Some(point),
            Self::Foot => Some(plane.world(plane.local(point))),
            Self::InPlane => {
                let offset = plane.distance(point);
                offset.is_finite().then_some(plane.world(plane.local(point))).filter(|_| offset.abs() <= ON_PLANE_MM)
            }
        }
    }
}

/// One frame of the pointer's snap question.
///
/// It carries the ray, the plane the tool draws on, the kinds the workspace allows and the two
/// points the sketch constructions need: the draft's own points ([`SnapRequest::ends`]) and the point
/// the next segment starts from ([`SnapRequest::anchor`]).
#[derive(Clone, Copy, Debug)]
pub struct SnapRequest<'a> {
    /// Where the pointer's ray starts: the camera's eye.
    pub ray_origin: DVec3,
    /// The pointer's ray, pointing into the scene. It need not be normalised.
    pub ray_direction: DVec3,
    /// How near a candidate must be to capture, in world millimetres, at the depth the tool measured
    /// it — the plane the pointer lands on.
    pub tolerance: Length,
    /// The depth from the eye at which [`SnapRequest::tolerance`] was measured, in millimetres.
    ///
    /// The pointer's reach is a screen radius and not a world distance: ten points cover a few
    /// millimetres on a board beside the hand and tens of metres on a wall across the shop. A
    /// candidate nearer or farther than that plane is measured by the same pointer, so its reach is
    /// scaled by its own depth. Zero means the tolerance is the reach everywhere (U116).
    pub tolerance_depth: f64,
    /// Which kinds may capture.
    pub kinds: SnapKinds,
    /// The plane the tool draws on.
    pub plane: SnapPlane,
    /// What the tool does with the answer, which fixes what the answer is (U119).
    pub target: SnapTarget,
    /// The previous point of the draft, in the world, when there is one.
    pub anchor: Option<DVec3>,
    /// The draft's own points, in the world: the `vertex` kind of the figure being drawn.
    pub ends: &'a [DVec3],
    /// Whether the hand asked for a straight segment: the point locks to a plane axis through the
    /// anchor. The lock is a constraint the maker states and not a kind, so it survives every
    /// switch and composes with the grid exactly as it always has (ADR 0051).
    pub axis_lock: bool,
    /// The face the tool itself measured under the pointer, in world millimetres.
    ///
    /// The surface is the tool's own reading — the ruler steps past a product's reserved box to the
    /// board behind it — and this resolver only gives it its place in the order: a measured face is
    /// a place the maker can see, so it beats the grid and loses to a corner (ADR 0183).
    pub face: Option<DVec3>,
}

/// Where the pointer landed, and what captured it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SnapAnswer {
    /// The point the tool stores: the candidate itself for a free tool, and the candidate's place on
    /// the tool's plane for a plane-bound one. A caller paints its marker here and nowhere else.
    pub point: DVec3,
    /// The kind that captured it, or `None` when the pointer's own point stands.
    pub kind: Option<SnapKind>,
    /// Whether the axis lock moved the point onto a plane axis through the anchor.
    pub locked: bool,
}

/// One point of the scene a snap may capture, and why.
///
/// The object move reads these to align a carried piece with a neighbour: the marks are the same
/// candidates the pointer resolves, so a drag and a click agree about where a cabinet's centre is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneMark {
    /// The node the point belongs to.
    pub id: Id,
    /// The point in world millimetres.
    pub point: DVec3,
    /// Why the point is a candidate.
    pub kind: SnapKind,
}

impl Project {
    /// Every point a snap may capture, and the kind of each, in the document's own order.
    ///
    /// The walk is the one every query makes: a node the sight switched off leaves its whole subtree
    /// out, and the nesting limit bounds a malformed tree. Only geometry nodes answer — a camera and
    /// a light have no corners a maker points at.
    pub fn snap_marks(&self, sight: Sight<'_>) -> Vec<SceneMark> {
        self.snap_scene(sight).marks
    }

    /// The snapped point the pointer's ray asks for, or `None` when the ray misses its plane.
    ///
    /// The kinds are tried in their own priority order, and the first one with a candidate inside the
    /// tolerance answers, in the shape [`SnapRequest::target`] asks for. When none does, the point is
    /// the ray's own point on the plane — locked to a plane axis through the anchor if the hand asked
    /// for that, rounded to the grid if the grid is on. A grid point is the plane's quantum and not a
    /// capture, so it rounds wherever the pointer is; every other kind needs the tolerance.
    pub fn snap(&self, request: SnapRequest<'_>, sight: Sight<'_>) -> Option<SnapAnswer> {
        let plane = request.plane;
        let free = plane.intersect(request.ray_origin, request.ray_direction)?;
        let tolerance = request.tolerance.millimetres_f64().max(0.0);
        if let Some(hit) = self.point_snap(&request, sight, free, tolerance) {
            return Some(hit);
        }
        if request.kinds.axis
            && let Some(point) = plane_axis(plane, plane.local(free), tolerance)
        {
            return Some(SnapAnswer { point: plane.world(point), kind: Some(SnapKind::Axis), locked: false });
        }
        let mut point = plane.local(free);
        let mut locked = false;
        let mut kind = None;
        match request.anchor.filter(|_| request.axis_lock) {
            Some(anchor) => {
                let (projected, axis) = snap_axis(point, plane.local(anchor));
                point = projected;
                locked = true;
                // The grid rounds the coordinate the lock left free, so a joined segment keeps the
                // axis exact and lands on a round number along it (ADR 0051).
                if request.kinds.grid {
                    point = round_free_coordinate(point, axis);
                }
            }
            None => {
                if request.kinds.grid {
                    point = snap_grid(point);
                    kind = Some(SnapKind::Grid);
                }
            }
        }
        Some(SnapAnswer { point: plane.world(point), kind, locked })
    }

    /// The first kind, in priority order, that has a candidate near the pointer.
    fn point_snap(
        &self,
        request: &SnapRequest<'_>,
        sight: Sight<'_>,
        free: DVec3,
        tolerance: f64,
    ) -> Option<SnapAnswer> {
        let reach = Reach { millimetres: tolerance, depth: request.tolerance_depth };
        let kinds = request.kinds;
        let wants_scene = kinds.vertex
            || kinds.intersection
            || kinds.midpoint
            || kinds.centre
            || kinds.perpendicular
            || kinds.tangent
            || kinds.edge;
        let scene = if wants_scene { self.snap_scene(sight) } else { Scene::default() };
        let plane = request.plane;
        // A candidate the target refuses is not a place this tool may take, so it is dropped before
        // the kind ranks its candidates: the next corner of the same kind is still a corner, and a
        // sketch in the 3D mode must not lose one because a nearer one stands off its plane (U119).
        let accept = |point: DVec3| request.target.answer(plane, point).is_some();
        let pointer = plane.local(free);
        let anchor = request.anchor.map(|anchor| plane.local(anchor));
        for kind in SnapKind::ALL {
            if !kinds.enabled(kind) {
                continue;
            }
            let point = match kind {
                SnapKind::Vertex => nearest_point_to_ray(
                    request
                        .ends
                        .iter()
                        .copied()
                        .chain(scene.marks.iter().filter(|mark| mark.kind == kind).map(|mark| mark.point)),
                    &accept,
                    request.ray_origin,
                    request.ray_direction,
                    reach,
                ),
                SnapKind::Intersection => intersection(plane, &scene.lines, pointer, tolerance),
                SnapKind::Midpoint | SnapKind::Centre => nearest_point_to_ray(
                    scene.marks.iter().filter(|mark| mark.kind == kind).map(|mark| mark.point),
                    &accept,
                    request.ray_origin,
                    request.ray_direction,
                    reach,
                ),
                SnapKind::Perpendicular => {
                    anchor.and_then(|anchor| perpendicular(plane, &scene.lines, anchor, pointer, tolerance))
                }
                SnapKind::Tangent => anchor.and_then(|anchor| tangent(plane, &scene.arcs, anchor, pointer, tolerance)),
                SnapKind::Edge => {
                    nearest_on_lines(&scene.lines, &accept, request.ray_origin, request.ray_direction, reach).or_else(
                        || nearest_on_arcs(&scene.arcs, &accept, request.ray_origin, request.ray_direction, reach),
                    )
                }
                SnapKind::Face => request.face.filter(|face| face.is_finite() && accept(*face)),
                SnapKind::Axis | SnapKind::Grid => None,
            };
            if let Some(point) = point
                .filter(|point| visible(*point, request, tolerance))
                .and_then(|point| request.target.answer(plane, point))
            {
                return Some(SnapAnswer { point, kind: Some(kind), locked: false });
            }
        }
        None
    }

    /// The document's snap candidates: the marks, the straight edges and the arcs.
    fn snap_scene(&self, sight: Sight<'_>) -> Scene {
        let mut scene = Scene::default();
        self.collect_snap_scene(self.root(), &mut scene, 0, sight);
        scene
    }

    fn collect_snap_scene(&self, container: &Container, scene: &mut Scene, depth: u32, sight: Sight<'_>) {
        if depth > Self::MAX_ASSEMBLY_DEPTH {
            return;
        }
        for child in container.children() {
            if sight.hides(self, child.id()) {
                continue;
            }
            let Some(world) = self.world_placement(child.id()) else {
                continue;
            };
            match child {
                Node::Panel(panel) => {
                    let size = panel.nominal_size();
                    let max = DVec3::new(
                        size.length().millimetres_f64(),
                        size.width().millimetres_f64(),
                        panel.last_known_thickness().millimetres_f64(),
                    );
                    scene.boxed(panel.id(), DVec3::ZERO, max, world);
                    if let Some(contour) = panel.outline_contour() {
                        scene.ring(panel.id(), contour, world, 0.0);
                    }
                }
                Node::Object(object) => scene.object(object.id(), object.geometry(), world),
                // (a container's envelope is filed below, beside its box)
                Node::Container(inner) => {
                    if let Some(envelope) = inner.envelope() {
                        let max = DVec3::new(
                            envelope.x().millimetres_f64(),
                            envelope.y().millimetres_f64(),
                            envelope.z().millimetres_f64(),
                        );
                        scene.boxed(inner.id(), DVec3::ZERO, max, world);
                        // The middle of a piece's reserved space is empty, so it is a place a pointer
                        // may name: the centre of a cabinet's inside, not of a board (U116).
                        scene.mark(inner.id(), world.transform_point(max * 0.5), SnapKind::Centre);
                    }
                    self.collect_snap_scene(inner, scene, depth.saturating_add(1), sight);
                }
                Node::Camera(_) | Node::Light(_) => {}
            }
        }
    }
}

/// The document's snap candidate geometry, in world millimetres.
#[derive(Default)]
struct Scene {
    marks: Vec<SceneMark>,
    lines: Vec<[DVec3; 2]>,
    arcs: Vec<SceneArc>,
}

/// One arc of the scene, sampled into the world.
struct SceneArc {
    /// The supporting circle's centre, in the world.
    centre: DVec3,
    /// The supporting circle's plane normal, in the world.
    normal: DVec3,
    /// The supporting circle's radius, in millimetres.
    radius: f64,
    /// The arc's chain, in travel order, in the world: the chords the edge kind measures against.
    points: Vec<DVec3>,
}

impl Scene {
    /// Files one mark.
    fn mark(&mut self, id: Id, point: DVec3, kind: SnapKind) {
        if point.is_finite() {
            self.marks.push(SceneMark { id, point, kind });
        }
    }

    /// Files one straight edge and its two ends.
    fn line(&mut self, id: Id, start: DVec3, end: DVec3) {
        if !start.is_finite() || !end.is_finite() {
            return;
        }
        self.line_mark(id, start, end);
        self.lines.push([start, end]);
    }

    /// Files the two ends and the middle of an edge a caller files by hand.
    fn line_mark(&mut self, id: Id, start: DVec3, end: DVec3) {
        self.mark(id, start, SnapKind::Vertex);
        self.mark(id, end, SnapKind::Vertex);
        self.mark(id, (start + end) * 0.5, SnapKind::Midpoint);
    }

    /// Files a box: its eight corners, twelve edge midpoints, six face centres and twelve edges.
    fn boxed(&mut self, id: Id, min: DVec3, max: DVec3, world: Placement) {
        if !min.is_finite() || !max.is_finite() || (max - min).min_element() < 0.0 {
            return;
        }
        let point = |x: f64, y: f64, z: f64| world.transform_point(DVec3::new(x, y, z));
        let corners = [
            point(min.x, min.y, min.z),
            point(max.x, min.y, min.z),
            point(max.x, max.y, min.z),
            point(min.x, max.y, min.z),
            point(min.x, min.y, max.z),
            point(max.x, min.y, max.z),
            point(max.x, max.y, max.z),
            point(min.x, max.y, max.z),
        ];
        for corner in corners.iter().copied() {
            self.mark(id, corner, SnapKind::Vertex);
        }
        for (index, corner) in corners.iter().copied().take(4).enumerate() {
            let next = corners[(index + 1) % 4];
            let above = corners[index + 4];
            let above_next = corners[(index + 1) % 4 + 4];
            self.mark(id, (corner + next) * 0.5, SnapKind::Midpoint);
            self.mark(id, (corner + above) * 0.5, SnapKind::Midpoint);
            self.mark(id, (above + above_next) * 0.5, SnapKind::Midpoint);
            self.lines.push([corner, next]);
            self.lines.push([above, above_next]);
            self.lines.push([corner, above]);
        }
        // The six face centres are places a pointer can name: each is on the surface of the part. The
        // middle of the box is inside the material, so a panel or a body offers no such mark — a
        // marker drawn in the middle of a board names no place a maker works at (U116).
        let centre = (min + max) * 0.5;
        for axis in 0..3 {
            let mut low = centre;
            let mut high = centre;
            low[axis] = min[axis];
            high[axis] = max[axis];
            self.mark(id, world.transform_point(low), SnapKind::Centre);
            self.mark(id, world.transform_point(high), SnapKind::Centre);
        }
    }

    /// Files a drawn ring, `lift` millimetres above the node's own plane: its corners, its segment
    /// midpoints, its arc centres and its curves.
    fn ring(&mut self, id: Id, contour: &Contour, world: Placement, lift: f64) {
        self.segments(id, contour.segments(), world, lift);
    }

    /// Files one chain of drawn segments, in the node's own plane at `z = lift`.
    fn segments(&mut self, id: Id, segments: impl Iterator<Item = ContourSegment>, world: Placement, lift: f64) {
        let at = |point: [Length; 2]| {
            world.transform_point(DVec3::new(point[0].millimetres_f64(), point[1].millimetres_f64(), lift))
        };
        for segment in segments {
            let (Ok(start), Ok(end)) = (segment.start(), segment.end()) else {
                continue;
            };
            let (start, end) = (at(start), at(end));
            match segment {
                ContourSegment::Line { .. } => self.line(id, start, end),
                ContourSegment::Arc { center, radius, start_angle, sweep } => {
                    let centre = at(center);
                    let radius = radius.millimetres_f64();
                    let (start_angle, sweep) = (start_angle.radians_f64(), sweep.radians_f64());
                    self.mark(id, start, SnapKind::Vertex);
                    self.mark(id, end, SnapKind::Vertex);
                    if let Some(middle) = segment_middle(segment) {
                        self.mark(id, at(middle), SnapKind::Midpoint);
                    }
                    self.mark(id, centre, SnapKind::Centre);
                    let normal = world.transform_vector(DVec3::Z);
                    let samples = arc_samples(radius, sweep);
                    let mut points = Vec::with_capacity(samples + 1);
                    for step in 0..=samples {
                        let turn = start_angle + sweep * (step as f64 / samples as f64);
                        let local = DVec3::new(
                            center[0].millimetres_f64() + radius * turn.cos(),
                            center[1].millimetres_f64() + radius * turn.sin(),
                            lift,
                        );
                        points.push(world.transform_point(local));
                    }
                    self.arcs.push(SceneArc { centre, normal, radius, points });
                }
            }
        }
    }

    /// Files an object: the places its own payload describes.
    ///
    /// The bound box is no longer the answer for every geometry: a box that stands around a round
    /// body offers eight corners in the air and neither the body's rim nor its axis (U120). A baked
    /// mesh is the one payload with no parameters left, so its box is the shape it *is*; every other
    /// payload files the places its numbers name.
    fn object(&mut self, id: Id, geometry: &Geometry, world: Placement) {
        let bounds = object_bounds_mm(geometry);
        match geometry {
            Geometry::Mesh(_) => {
                if let Some((min, max)) = bounds {
                    self.boxed(id, min, max, world);
                }
            }
            Geometry::Path(path) => self.segments(id, path.segments(), world, 0.0),
            Geometry::Contours(contours) => {
                for path in contours.paths() {
                    self.segments(id, path.segments(), world, 0.0);
                }
            }
            Geometry::Constructive(constructive) => {
                let Some((min, max)) = bounds else {
                    return;
                };
                self.body(id, constructive, min, max, world);
            }
            Geometry::Primitive(primitive) => self.primitive(id, primitive, world),
        }
    }

    /// Files a body swept from a drawn profile: its two rims and the axis it turns about.
    ///
    /// A rectangle swept on Z is a box to the micrometre, so the box's own corners, edge middles and
    /// six face centres are the places a maker works at, and they stay (U116). Any other profile — a
    /// drawn circle, a hexagon, a rectangle turned in its plane — leaves the corners of its box in the
    /// air, so the body answers the ring it was drawn from and the same ring lifted by the extrusion
    /// height, plus the middle of each end: the two places the axis offers (U120).
    fn body(&mut self, id: Id, constructive: &Constructive, min: DVec3, max: DVec3, world: Placement) {
        let Ok(contour) = constructive.contour() else {
            return;
        };
        self.ring(id, &contour, world, 0.0);
        if is_axis_rectangle(&contour) {
            self.boxed(id, min, max, world);
            return;
        }
        let height = constructive.height().map_or(0.0, |height| height.millimetres_f64());
        if !height.is_finite() || height <= 0.0 {
            return;
        }
        self.ring(id, &contour, world, height);
        let middle = (min + max) * 0.5;
        for z in [0.0, height] {
            self.mark(id, world.transform_point(DVec3::new(middle.x, middle.y, z)), SnapKind::Centre);
        }
    }

    /// Files a primitive's own places: a ball's middle and six quadrants, a cone's rims and ends.
    ///
    /// The match is over the payload and has no catch-all, so a new primitive is a compile error
    /// until it says where a pointer may capture it (U120).
    fn primitive(&mut self, id: Id, primitive: &Primitive, world: Placement) {
        match primitive.base() {
            PrimitiveShape::Sphere(sphere) => {
                let radius = sphere.radius().millimetres_f64();
                // The middle of a ball is a place the maker names even though the material is there
                // (ADR 0139); the poles and the equator quadrants are its own six extremes.
                self.mark(id, world.transform_point(DVec3::ZERO), SnapKind::Centre);
                for point in [
                    DVec3::new(0.0, 0.0, radius),
                    DVec3::new(0.0, 0.0, -radius),
                    DVec3::new(radius, 0.0, 0.0),
                    DVec3::new(-radius, 0.0, 0.0),
                    DVec3::new(0.0, radius, 0.0),
                    DVec3::new(0.0, -radius, 0.0),
                ] {
                    self.mark(id, world.transform_point(point), SnapKind::Vertex);
                }
            }
            PrimitiveShape::Cone(cone) => {
                let base = cone.base_radius().millimetres_f64();
                let height = cone.height().millimetres_f64();
                // The base circle is the round the maker drew, so its middle is the node's origin.
                self.mark(id, world.transform_point(DVec3::ZERO), SnapKind::Centre);
                self.rim(id, world, 0.0, base);
                if cone.is_sharp() {
                    // A sharp cone ends in one apex, which is the axis's other end.
                    self.mark(id, world.transform_point(DVec3::new(0.0, 0.0, height)), SnapKind::Vertex);
                } else {
                    let top = cone.top_radius().millimetres_f64();
                    self.rim(id, world, height, top);
                    self.mark(id, world.transform_point(DVec3::new(0.0, 0.0, height)), SnapKind::Centre);
                }
            }
        }
    }

    /// Files one round rim: its four quadrants, the middle of each quarter and its curve.
    ///
    /// The points come from the body's own frame, so a turned leg answers where it stands and not
    /// where the world axes point. The four quadrants and their middles are the places a drawn
    /// circle already offers, so a cone's rim and a swept circle's rim answer alike (U120).
    fn rim(&mut self, id: Id, world: Placement, z: f64, radius: f64) {
        if !radius.is_finite() || radius <= 0.0 || !z.is_finite() {
            return;
        }
        let at = |turn: f64| world.transform_point(DVec3::new(radius * turn.cos(), radius * turn.sin(), z));
        for quarter in [0.0, 1.0, 2.0, 3.0] {
            self.mark(id, at(quarter * FRAC_PI_2), SnapKind::Vertex);
            self.mark(id, at((quarter + 0.5) * FRAC_PI_2), SnapKind::Midpoint);
        }
        let centre = world.transform_point(DVec3::new(0.0, 0.0, z));
        let normal = world.transform_vector(DVec3::Z);
        let samples = arc_samples(radius, TAU);
        let mut points = Vec::with_capacity(samples + 1);
        for step in 0..=samples {
            let turn = TAU * (step as f64 / samples as f64);
            points.push(at(turn));
        }
        self.arcs.push(SceneArc { centre, normal, radius, points });
    }
}

/// Whether a drawn ring is an axis-aligned rectangle, so the body swept from it fills its box.
///
/// Four straight sides that run along the ring's own axes close a rectangle and nothing else, which
/// is the body a box describes to the micrometre. A tilted rectangle, a hexagon or a ring with a cut
/// corner fails here, and its box corners stand in the air around the body (U120).
fn is_axis_rectangle(contour: &Contour) -> bool {
    let mut sides = 0;
    for segment in contour.segments() {
        let ContourSegment::Line { start, end } = segment else {
            return false;
        };
        sides += 1;
        // Two coordinates that both move make a diagonal, and two that both stay make a point.
        if (start[0] == end[0]) == (start[1] == end[1]) {
            return false;
        }
    }
    sides == 4
}

/// The middle of a segment, in its own plane.
fn segment_middle(segment: ContourSegment) -> Option<[Length; 2]> {
    match segment {
        ContourSegment::Line { start, end } => Some([
            Length::from_micrometres((start[0].micrometres() + end[0].micrometres()) / 2),
            Length::from_micrometres((start[1].micrometres() + end[1].micrometres()) / 2),
        ]),
        ContourSegment::Arc { center, radius, start_angle, sweep } => {
            let half = start_angle.millidegrees() + sweep.millidegrees() / 2;
            let turn = units::Degrees::from_millidegrees(half).radians_f64();
            let radius = radius.millimetres_f64();
            let x = center[0].millimetres_f64() + radius * turn.cos();
            let y = center[1].millimetres_f64() + radius * turn.sin();
            Some([millimetres_round(x)?, millimetres_round(y)?])
        }
    }
}

/// A millimetre coordinate as a whole micrometre, or `None` when it is not representable.
fn millimetres_round(value: f64) -> Option<Length> {
    let micrometres = value * 1_000.0;
    if !micrometres.is_finite() || micrometres < i64::MIN as f64 || micrometres > i64::MAX as f64 {
        return None;
    }
    Some(Length::from_micrometres(micrometres.round() as i64))
}

/// How many chords an arc is sampled into, so a snap sees its shape.
fn arc_samples(radius: f64, sweep: f64) -> usize {
    let length = (radius * sweep.abs()).abs();
    let samples = (length / 2.0).ceil();
    if !samples.is_finite() {
        return MIN_ARC_SAMPLES;
    }
    (samples as usize).clamp(MIN_ARC_SAMPLES, MAX_ARC_SAMPLES)
}

/// The point on the world axes the sketch plane holds that is nearest `pointer`.
///
/// A world axis that lies in the plane is a line of it, and the plane point is projected onto it; an
/// axis that crosses the plane is a single point — the world origin, for a plane that passes through
/// it. Both are candidates: a maker drawing on the floor can start on the world X axis, and one
/// drawing on a side view can start where the two visible axes cross.
fn plane_axis(plane: SnapPlane, pointer: DVec2, tolerance: f64) -> Option<DVec2> {
    let mut best: Option<(f64, DVec2)> = None;
    for axis in [DVec3::X, DVec3::Y, DVec3::Z] {
        let along = axis.dot(plane.normal());
        let candidate = if along.abs() <= DEGENERATE {
            // The axis runs in the plane: project the pointer onto the line through the origin.
            let direction = DVec2::new(axis.dot(plane.u()), axis.dot(plane.v()));
            let length = direction.length();
            if length <= DEGENERATE {
                continue;
            }
            let direction = direction / length;
            Some(direction * pointer.dot(direction))
        } else {
            // The axis crosses the plane at one point, which is the world origin when the plane runs
            // through it.
            let crossing = plane.origin().dot(plane.normal()) / along;
            crossing.is_finite().then(|| plane.local(axis * crossing))
        };
        let Some(candidate) = candidate else {
            continue;
        };
        let distance = (candidate - pointer).length();
        if distance <= tolerance && best.is_none_or(|(nearest, _)| distance < nearest) {
            best = Some((distance, candidate));
        }
    }
    best.map(|(_, point)| point)
}

/// The crossing of two straight edges in the sketch plane, nearest the pointer.
///
/// The edges are taken as they lie in the plane: an edge that is not in it is projected, which is
/// what the maker sees when a figure is drawn over the boards behind it. Only the edges whose own
/// projection passes within the tolerance of the pointer are paired, because a crossing nearer than
/// that has both of its edges nearer than that too (ADR 0183).
fn intersection(plane: SnapPlane, lines: &[[DVec3; 2]], pointer: DVec2, tolerance: f64) -> Option<DVec3> {
    let near: Vec<(DVec2, DVec2)> = lines
        .iter()
        .map(|[start, end]| (plane.local(*start), plane.local(*end)))
        .filter(|(start, end)| point_to_segment(pointer, *start, *end) <= tolerance)
        .collect();
    let mut best: Option<(f64, DVec2)> = None;
    for (index, first) in near.iter().enumerate() {
        for second in near.iter().skip(index + 1) {
            let Some(point) = line_crossing(*first, *second) else {
                continue;
            };
            let distance = (point - pointer).length();
            if distance <= tolerance && best.is_none_or(|(nearest, _)| distance < nearest) {
                best = Some((distance, point));
            }
        }
    }
    best.map(|(_, point)| plane.world(point))
}

/// Where two segments cross, or `None` when their supports are parallel.
fn line_crossing(first: (DVec2, DVec2), second: (DVec2, DVec2)) -> Option<DVec2> {
    let (a, b) = first;
    let (c, d) = second;
    let first_direction = b - a;
    let second_direction = d - c;
    let denominator = first_direction.x * second_direction.y - first_direction.y * second_direction.x;
    if denominator.abs() <= DEGENERATE {
        return None;
    }
    let offset = c - a;
    let along = (offset.x * second_direction.y - offset.y * second_direction.x) / denominator;
    let across = (offset.x * first_direction.y - offset.y * first_direction.x) / denominator;
    if !(-1.0e-9..=1.0 + 1.0e-9).contains(&along) || !(-1.0e-9..=1.0 + 1.0e-9).contains(&across) {
        return None;
    }
    let point = a + first_direction * along;
    point.is_finite().then_some(point)
}

/// The foot of the perpendicular from `anchor` onto a straight edge, nearest the pointer.
fn perpendicular(
    plane: SnapPlane,
    lines: &[[DVec3; 2]],
    anchor: DVec2,
    pointer: DVec2,
    tolerance: f64,
) -> Option<DVec3> {
    let mut best: Option<(f64, DVec2)> = None;
    for [start, end] in lines {
        let (start, end) = (plane.local(*start), plane.local(*end));
        let direction = end - start;
        let length = direction.length_squared();
        if !length.is_finite() || length <= DEGENERATE {
            continue;
        }
        let along = (anchor - start).dot(direction) / length;
        if !(-1.0e-9..=1.0 + 1.0e-9).contains(&along) {
            continue;
        }
        let foot = start + direction * along;
        let distance = (foot - pointer).length();
        if distance <= tolerance && best.is_none_or(|(nearest, _)| distance < nearest) {
            best = Some((distance, foot));
        }
    }
    best.map(|(_, point)| plane.world(point))
}

/// The point where a line from `anchor` touches an arc's circle, nearest the pointer.
///
/// The tangency is the plane's own figure, so an arc is only a tangent target when its circle lies in
/// the sketch plane: a circle seen at any other angle projects to an ellipse, and the point where a
/// line touches an ellipse is not the point where it touches the circle the document stores. The
/// tangent point is then kept only when it really lies on the arc.
fn tangent(plane: SnapPlane, arcs: &[SceneArc], anchor: DVec2, pointer: DVec2, tolerance: f64) -> Option<DVec3> {
    let mut best: Option<(f64, DVec2)> = None;
    for arc in arcs.iter().filter(|arc| arc.normal.cross(plane.normal()).length() <= 1.0e-6) {
        if plane.distance(arc.centre).abs() > 1.0e-3 {
            continue;
        }
        let centre = plane.local(arc.centre);
        let offset = anchor - centre;
        let distance = offset.length();
        if !distance.is_finite() || distance <= arc.radius {
            continue;
        }
        let angle = offset.y.atan2(offset.x);
        let spread = (arc.radius / distance).acos();
        for turn in [angle + spread, angle - spread] {
            let point = centre + DVec2::new(turn.cos(), turn.sin()) * arc.radius;
            let near_pointer = (point - pointer).length();
            if near_pointer > tolerance || !on_arc(plane, arc, point, tolerance) {
                continue;
            }
            if best.is_none_or(|(nearest, _)| near_pointer < nearest) {
                best = Some((near_pointer, point));
            }
        }
    }
    best.map(|(_, point)| plane.world(point))
}

/// Whether a plane point lies on the arc's own chain, within `tolerance`.
fn on_arc(plane: SnapPlane, arc: &SceneArc, point: DVec2, tolerance: f64) -> bool {
    arc.points
        .windows(2)
        .map(|pair| (plane.local(pair[0]), plane.local(pair[1])))
        .any(|(start, end)| point_to_segment(point, start, end) <= tolerance.max(1.0e-3))
}

/// Whether a candidate is in front of the face the tool measured.
///
/// A tool that measures what the eye sees — the ruler — hands in the face it measured, so a corner or
/// a centre behind the first surface is not a place it may land on: the marker would name a point
/// hidden inside a board. A tool that draws on a plane hands in no face and keeps every candidate;
/// what it does with one is [`SnapTarget`]'s business — the 2.5D reading takes its shadow, the 3D one
/// refuses it (U116, U119).
fn visible(point: DVec3, request: &SnapRequest<'_>, tolerance: f64) -> bool {
    let Some(face) = request.face else {
        return true;
    };
    let Some(direction) = request.ray_direction.try_normalize() else {
        return true;
    };
    let face_along = (face - request.ray_origin).dot(direction);
    let point_along = (point - request.ray_origin).dot(direction);
    point_along <= face_along + tolerance
}

/// The nearest point of a segment to another point in the plane.
fn point_to_segment(point: DVec2, start: DVec2, end: DVec2) -> f64 {
    let direction = end - start;
    let length = direction.length_squared();
    if !length.is_finite() || length <= DEGENERATE {
        return (point - start).length();
    }
    let along = ((point - start).dot(direction) / length).clamp(0.0, 1.0);
    (point - (start + direction * along)).length()
}

/// The nearest point on a segment to a ray, with its distance, or `None` for a degenerate segment.
fn ray_to_segment(origin: DVec3, direction: DVec3, start: DVec3, end: DVec3) -> Option<(f64, DVec3)> {
    let segment = end - start;
    let length = segment.length_squared();
    if !length.is_finite() || length <= DEGENERATE {
        return None;
    }
    let offset = origin - start;
    let along = direction.dot(segment);
    let ray_along = direction.dot(direction);
    if !ray_along.is_finite() || ray_along <= DEGENERATE {
        return None;
    }
    let ray_offset = direction.dot(offset);
    let segment_offset = segment.dot(offset);
    let denominator = ray_along * length - along * along;
    // The ray starts at the eye and never runs backwards, so its parameter is clamped at zero; the
    // segment is clamped to its own ends, and one more pass keeps the two answers consistent.
    let mut ray_parameter =
        if denominator.abs() > DEGENERATE { (along * segment_offset - length * ray_offset) / denominator } else { 0.0 };
    ray_parameter = ray_parameter.max(0.0);
    let mut segment_parameter = (along * ray_parameter + segment_offset) / length;
    segment_parameter = segment_parameter.clamp(0.0, 1.0);
    ray_parameter = ((along * segment_parameter - ray_offset) / ray_along).max(0.0);
    let on_ray = origin + direction * ray_parameter;
    let on_segment = start + segment * segment_parameter;
    let distance = (on_ray - on_segment).length();
    distance.is_finite().then_some((distance, on_segment))
}

/// The nearest candidate to a ray, inside the tolerance, among those `accept` takes.
fn nearest_point_to_ray(
    points: impl Iterator<Item = DVec3>,
    accept: &dyn Fn(DVec3) -> bool,
    origin: DVec3,
    direction: DVec3,
    reach: Reach,
) -> Option<DVec3> {
    let direction = direction.try_normalize()?;
    let mut best: Option<(f64, f64, DVec3)> = None;
    for point in points {
        if !point.is_finite() || !accept(point) {
            continue;
        }
        let offset = point - origin;
        let along = offset.dot(direction);
        let distance = if along <= 0.0 { offset.length() } else { (offset - direction * along).length() };
        if distance > reach.at(along) {
            continue;
        }
        // A nearer place wins; an equal one is the same place seen twice — a box's top corner and
        // its bottom corner on one line through the eye — and the eye can see the front one.
        if best.is_none_or(|(nearest, nearest_along, _)| {
            distance < nearest - TIE || (distance <= nearest + TIE && along < nearest_along)
        }) {
            best = Some((distance, along, point));
        }
    }
    best.map(|(_, _, point)| point)
}

/// The point of the nearest straight edge to a ray, inside the tolerance, among those `accept` takes.
fn nearest_on_lines(
    lines: &[[DVec3; 2]],
    accept: &dyn Fn(DVec3) -> bool,
    origin: DVec3,
    direction: DVec3,
    reach: Reach,
) -> Option<DVec3> {
    let direction = direction.try_normalize()?;
    let mut best: Option<(f64, f64, DVec3)> = None;
    for [start, end] in lines.iter().copied() {
        let Some((distance, point)) = ray_to_segment(origin, direction, start, end) else {
            continue;
        };
        if !accept(point) {
            continue;
        }
        let along = (point - origin).dot(direction);
        if distance > reach.at(along) {
            continue;
        }
        if best.is_none_or(|(nearest, nearest_along, _)| {
            distance < nearest - TIE || (distance <= nearest + TIE && along < nearest_along)
        }) {
            best = Some((distance, along, point));
        }
    }
    best.map(|(_, _, point)| point)
}

/// The point of the nearest arc's chain to a ray, inside the tolerance, among those `accept` takes.
fn nearest_on_arcs(
    arcs: &[SceneArc],
    accept: &dyn Fn(DVec3) -> bool,
    origin: DVec3,
    direction: DVec3,
    reach: Reach,
) -> Option<DVec3> {
    let direction = direction.try_normalize()?;
    let mut best: Option<(f64, f64, DVec3)> = None;
    for arc in arcs {
        for pair in arc.points.windows(2) {
            let Some((distance, point)) = ray_to_segment(origin, direction, pair[0], pair[1]) else {
                continue;
            };
            if !accept(point) {
                continue;
            }
            let along = (point - origin).dot(direction);
            if distance > reach.at(along) {
                continue;
            }
            if best.is_none_or(|(nearest, nearest_along, _)| {
                distance < nearest - TIE || (distance <= nearest + TIE && along < nearest_along)
            }) {
                best = Some((distance, along, point));
            }
        }
    }
    best.map(|(_, _, point)| point)
}

/// Which plane axis a locked point follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapAxis {
    /// The `u` axis through the anchor: the anchor's `v` is kept.
    U,
    /// The `v` axis through the anchor: the anchor's `u` is kept.
    V,
}

/// Projects a plane point onto the `u` or `v` axis through `anchor`, whichever offset is larger.
///
/// The offsets are compared by magnitude and a tie locks to `u`, so the answer never depends on the
/// order of the components.
pub fn snap_axis(point: DVec2, anchor: DVec2) -> (DVec2, SnapAxis) {
    let offset = point - anchor;
    if offset.x.abs() >= offset.y.abs() {
        (DVec2::new(point.x, anchor.y), SnapAxis::U)
    } else {
        (DVec2::new(anchor.x, point.y), SnapAxis::V)
    }
}

/// Rounds both plane coordinates to the nearest grid multiple.
///
/// A non-finite coordinate is returned unchanged rather than turned into a grid value.
pub fn snap_grid(point: DVec2) -> DVec2 {
    DVec2::new(round_to_grid(point.x), round_to_grid(point.y))
}

/// Rounds the coordinate the axis lock left free, keeping the locked one exact.
pub fn round_free_coordinate(point: DVec2, axis: SnapAxis) -> DVec2 {
    match axis {
        SnapAxis::U => DVec2::new(round_to_grid(point.x), point.y),
        SnapAxis::V => DVec2::new(point.x, round_to_grid(point.y)),
    }
}

fn round_to_grid(value: f64) -> f64 {
    let rounded = (value / SNAP_GRID_MM).round() * SNAP_GRID_MM;
    if rounded.is_finite() { rounded } else { value }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use std::collections::HashSet;

    use super::*;
    use crate::fixtures::millimetres;
    use crate::testing::open_path;
    use crate::{Container, Geometry, Panel, PanelNominalSize, Path};

    fn id(value: u64) -> Id {
        Id::new(value).expect("non-zero id")
    }

    /// An empty project: a test's own node list.
    fn empty() -> Project {
        Project::new(Container::new(id(1), "Root", Placement::IDENTITY, Vec::new())).expect("an empty project")
    }

    /// A project with one 700 x 500 x 18 shelf at the world origin.
    fn shelf() -> Project {
        let size = PanelNominalSize::new(millimetres(700), millimetres(500)).expect("a panel size");
        let panel =
            Panel::new(id(2), "Shelf", size, None, millimetres(18), Placement::IDENTITY).expect("a shelf panel");
        Project::new(Container::new(id(1), "Root", Placement::IDENTITY, vec![panel.into()])).expect("a project")
    }

    /// A ray straight down through `(x, y)`, from high above the scene.
    fn down(x: f64, y: f64) -> (DVec3, DVec3) {
        (DVec3::new(x, y, 500.0), -DVec3::Z)
    }

    fn request(ray: (DVec3, DVec3), kinds: SnapKinds) -> SnapRequest<'static> {
        SnapRequest {
            ray_origin: ray.0,
            ray_direction: ray.1,
            tolerance: millimetres(5),
            tolerance_depth: 0.0,
            kinds,
            plane: SnapPlane::world_xy(),
            target: SnapTarget::Free,
            anchor: None,
            ends: &[],
            axis_lock: false,
            face: None,
        }
    }

    /// The same question from a tool that draws on the plane, in the mode the workspace chose.
    fn targeted(plane: SnapPlane, ray: (DVec3, DVec3), kinds: SnapKinds, target: SnapTarget) -> SnapRequest<'static> {
        SnapRequest { plane, target, ..request(ray, kinds) }
    }

    /// A plane parallel to the floor at `z`: a face a figure is drawn on, or the base itself.
    fn plane_at(z: f64) -> SnapPlane {
        SnapPlane::new(DVec3::new(0.0, 0.0, z), DVec3::X, DVec3::Y).expect("a plane parallel to the floor")
    }

    /// The settings that let one kind capture.
    fn only(kind: SnapKind) -> SnapKinds {
        let mut kinds = SnapKinds::none();
        kinds.set(kind, true);
        kinds
    }

    fn snapped(project: &Project, request: SnapRequest<'_>) -> SnapAnswer {
        project.snap(request, Sight::none()).expect("the ray meets the plane")
    }

    fn close(point: DVec3, expected: DVec3) -> bool {
        (point - expected).length() <= 1.0e-6
    }

    /// A project holding one object of `geometry` at the world origin.
    fn body_of(geometry: Geometry) -> Project {
        let mut project = empty();
        let root = project.root().id();
        project.add_object(root, "Body", Placement::IDENTITY, geometry).expect("a body");
        project
    }

    /// A project holding one drawn circle swept on Z, which is how a cylinder is built.
    fn swept_circle(radius: i64, height: i64) -> Project {
        let ring = Path::circle([millimetres(0), millimetres(0)], millimetres(radius)).expect("a circle");
        let body = Constructive::extrude(ring, millimetres(height)).expect("a swept circle");
        body_of(Geometry::Constructive(body))
    }

    /// The settings that let every kind but the grid capture: the grid rounds wherever the pointer
    /// is, so it answers even over empty air and tells nothing about the scene.
    fn scene_kinds() -> SnapKinds {
        let mut kinds = SnapKinds::ALL;
        kinds.set(SnapKind::Grid, false);
        kinds
    }

    /// How many distinct places of one kind the project offers: a place two payloads happen to file
    /// twice is one place a pointer can name.
    fn distinct(project: &Project, kind: SnapKind) -> usize {
        let mut points: Vec<DVec3> = Vec::new();
        for mark in project.snap_marks(Sight::none()).iter().filter(|mark| mark.kind == kind) {
            if !points.iter().any(|point| (*point - mark.point).length() <= 1.0e-6) {
                points.push(mark.point);
            }
        }
        points.len()
    }

    #[test]
    fn a_click_near_a_panel_corner_snaps_to_it() {
        let answer = snapped(&shelf(), request(down(698.0, 499.0), SnapKinds::ALL));
        assert_eq!(answer.kind, Some(SnapKind::Vertex), "the corner under the pointer");
        // The eye looks down, so the corner it can see is the one on the upper face.
        assert!(close(answer.point, DVec3::new(700.0, 500.0, 18.0)), "{:?}", answer.point);
        assert!(!answer.locked);
    }

    #[test]
    fn a_sketch_takes_the_candidate_in_its_plane_and_the_foot_of_one_that_is_not() {
        // One corner of the shelf, asked by a tool that draws on a plane: the 2.5D reading takes it
        // from anywhere and answers its perpendicular foot, the 3D reading refuses it and leaves the
        // pointer's own point (U119).
        let foot = targeted(plane_at(100.0), down(698.0, 499.0), only(SnapKind::Vertex), SnapTarget::Foot);
        let answer = snapped(&shelf(), foot);
        assert_eq!(answer.kind, Some(SnapKind::Vertex), "2.5D still takes a corner above the plane");
        assert!(close(answer.point, DVec3::new(700.0, 500.0, 100.0)), "the answer is the foot: {:?}", answer.point);

        let sharp = targeted(plane_at(100.0), down(698.0, 499.0), only(SnapKind::Vertex), SnapTarget::InPlane);
        let answer = snapped(&shelf(), sharp);
        assert_eq!(answer.kind, None, "3D takes no candidate that stands off the plane");
        assert!(
            close(answer.point, DVec3::new(698.0, 499.0, 100.0)),
            "the pointer's own plane point stands: {:?}",
            answer.point
        );
    }

    #[test]
    fn a_sketch_takes_a_corner_of_its_own_face_in_both_modes() {
        // Drawing on the board's top face: its corners lie in that plane, so the 3D reading takes
        // them exactly as the 2.5D one does. This is where a figure's plane comes from (U119).
        for target in [SnapTarget::InPlane, SnapTarget::Foot] {
            let request = targeted(plane_at(18.0), down(698.0, 499.0), only(SnapKind::Vertex), target);
            let answer = snapped(&shelf(), request);
            assert_eq!(answer.kind, Some(SnapKind::Vertex), "{target:?}");
            assert!(close(answer.point, DVec3::new(700.0, 500.0, 18.0)), "{target:?}: {:?}", answer.point);
        }
    }

    #[test]
    fn the_3d_mode_drops_the_candidate_off_the_plane_and_keeps_the_one_in_it() {
        // The board's upper and lower corners lie on one line through the eye. The 3D reading refuses
        // the upper one and still answers the lower one, because a kind ranks the candidates its
        // target takes and not the answer it then throws away (U119).
        let request = targeted(SnapPlane::world_xy(), down(698.0, 499.0), only(SnapKind::Vertex), SnapTarget::InPlane);
        let answer = snapped(&shelf(), request);
        assert_eq!(answer.kind, Some(SnapKind::Vertex));
        assert!(close(answer.point, DVec3::new(700.0, 500.0, 0.0)), "{:?}", answer.point);
    }

    #[test]
    fn every_answer_of_a_plane_tool_lies_on_its_plane() {
        // The invariant a marker stands on: whatever captured the pointer and whichever mode is in
        // force, the point a plane-bound tool stores is a point of that plane (U119).
        let plane = plane_at(40.0);
        for target in [SnapTarget::Foot, SnapTarget::InPlane] {
            for pointer in [down(698.0, 499.0), down(352.0, 251.0), down(200.0, 2.0)] {
                for kind in SnapKind::ALL {
                    let request = targeted(plane, pointer, only(kind), target);
                    if let Some(answer) = shelf().snap(request, Sight::none()) {
                        let off = plane.distance(answer.point).abs();
                        assert!(off <= 1.0e-6, "{target:?} {kind:?} left the plane by {off}: {:?}", answer.point);
                    }
                }
            }
        }
    }

    #[test]
    fn the_mode_is_the_target_a_plane_bound_tool_declares() {
        assert_eq!(SnapMode::Space.target(), SnapTarget::InPlane);
        assert_eq!(SnapMode::Foot.target(), SnapTarget::Foot);
        assert_eq!(SnapMode::default(), SnapMode::Foot, "a fresh workspace lines a board up by its shadow");
        assert_eq!(SnapMode::ALL, [SnapMode::Space, SnapMode::Foot], "two modes, two readings");
    }

    #[test]
    fn the_midpoint_and_the_centre_come_from_the_same_resolver() {
        let project = shelf();
        let midpoint = snapped(&project, request(down(702.0, 251.0), only(SnapKind::Midpoint)));
        assert_eq!(midpoint.kind, Some(SnapKind::Midpoint));
        assert!(close(midpoint.point, DVec3::new(700.0, 250.0, 18.0)), "{:?}", midpoint.point);

        let centre = snapped(&project, request(down(352.0, 251.0), only(SnapKind::Centre)));
        assert_eq!(centre.kind, Some(SnapKind::Centre), "the middle of a face");
        assert!(close(centre.point, DVec3::new(350.0, 250.0, 18.0)), "{:?}", centre.point);
    }

    #[test]
    fn a_point_on_an_edge_is_taken_from_the_edge_itself() {
        let answer = snapped(&shelf(), request(down(200.0, 2.0), only(SnapKind::Edge)));
        assert_eq!(answer.kind, Some(SnapKind::Edge));
        assert!(close(answer.point, DVec3::new(200.0, 0.0, 18.0)), "{:?}", answer.point);
    }

    #[test]
    fn a_corner_beats_a_nearer_edge() {
        // The pointer is 3 mm from the edge and 4.24 mm from the corner; the corner still wins,
        // because a maker pointing next to both means the corner.
        let answer = snapped(&shelf(), request(down(697.0, 3.0), SnapKinds::ALL));
        assert_eq!(answer.kind, Some(SnapKind::Vertex));
        assert!(close(answer.point, DVec3::new(700.0, 0.0, 18.0)), "{:?}", answer.point);
    }

    #[test]
    fn a_pair_on_one_line_through_the_eye_answers_the_nearer_one() {
        // A board's upper and lower corners lie on one line through the eye and are equally near the
        // ray; the maker means the one it can see, so depth breaks the tie and not the order the
        // document happens to hold (U116). Looking up answers the lower corner, looking down the upper.
        let from_above = snapped(&shelf(), request(down(698.0, 499.0), only(SnapKind::Vertex)));
        assert!(close(from_above.point, DVec3::new(700.0, 500.0, 18.0)), "{:?}", from_above.point);
        let mut below = request((DVec3::new(698.0, 499.0, -500.0), DVec3::Z), only(SnapKind::Vertex));
        below.tolerance = millimetres(5);
        let from_below = snapped(&shelf(), below);
        assert!(close(from_below.point, DVec3::new(700.0, 500.0, 0.0)), "{:?}", from_below.point);
    }

    #[test]
    fn a_disabled_kind_never_captures_and_the_tolerance_decides() {
        let project = shelf();
        let plain = snapped(&project, request(down(698.0, 499.0), SnapKinds::none()));
        assert_eq!(plain.kind, None, "no kind, no capture");
        assert!(close(plain.point, DVec3::new(698.0, 499.0, 0.0)), "the pointer's own point");

        // 6.4 mm from the corner: outside the 5 mm tolerance and inside an 8 mm one.
        let far = snapped(&project, request(down(705.0, 504.0), only(SnapKind::Vertex)));
        assert_eq!(far.kind, None, "the tolerance decides");
        let mut wide = request(down(705.0, 504.0), only(SnapKind::Vertex));
        wide.tolerance = millimetres(8);
        assert_eq!(snapped(&project, wide).kind, Some(SnapKind::Vertex), "a wider tolerance captures it");
    }

    #[test]
    fn the_drafts_own_points_are_vertices() {
        let ends = [DVec3::new(100.0, 100.0, 0.0)];
        let mut request = request(down(102.0, 101.0), only(SnapKind::Vertex));
        request.ends = &ends;
        let answer = snapped(&empty(), request);
        assert_eq!(answer.kind, Some(SnapKind::Vertex));
        assert_eq!(answer.point, ends[0]);
    }

    #[test]
    fn two_edges_crossing_in_the_sketch_plane_are_an_intersection() {
        let mut project = empty();
        let root = project.root().id();
        let rail = open_path(vec![[millimetres(0), millimetres(0)], [millimetres(700), millimetres(0)]]);
        let stile = open_path(vec![[millimetres(300), millimetres(-100)], [millimetres(300), millimetres(140)]]);
        project.add_object(root, "Rail", Placement::IDENTITY, Geometry::Path(rail)).expect("a rail");
        project.add_object(root, "Stile", Placement::IDENTITY, Geometry::Path(stile)).expect("a stile");

        let answer = snapped(&project, request(down(301.0, 1.0), only(SnapKind::Intersection)));
        assert_eq!(answer.kind, Some(SnapKind::Intersection));
        assert!(close(answer.point, DVec3::new(300.0, 0.0, 0.0)), "{:?}", answer.point);
    }

    #[test]
    fn the_perpendicular_foot_comes_from_the_previous_point() {
        let mut from_above = request(down(300.0, 2.0), only(SnapKind::Perpendicular));
        from_above.anchor = Some(DVec3::new(300.0, 50.0, 0.0));
        let answer = snapped(&shelf(), from_above);
        assert_eq!(answer.kind, Some(SnapKind::Perpendicular));
        assert!(close(answer.point, DVec3::new(300.0, 0.0, 0.0)), "{:?}", answer.point);

        // Without a previous point there is nothing to be perpendicular from.
        let alone = request(down(300.0, 2.0), only(SnapKind::Perpendicular));
        assert_eq!(snapped(&shelf(), alone).kind, None);
    }

    #[test]
    fn the_tangent_from_the_previous_point_touches_the_circle() {
        let mut project = empty();
        let root = project.root().id();
        let disc = Path::circle([millimetres(50), millimetres(50)], millimetres(50)).expect("a circle");
        project.add_object(root, "Disc", Placement::IDENTITY, Geometry::Path(disc)).expect("a disc");

        let mut request = request(down(60.0, 1.0), only(SnapKind::Tangent));
        request.anchor = Some(DVec3::new(300.0, 50.0, 0.0));
        let answer = snapped(&project, request);
        assert_eq!(answer.kind, Some(SnapKind::Tangent));
        // The tangent from (300, 50) to the circle at (50, 50) with r = 50 touches at (60, 1): the
        // radius and the tangent are square there.
        assert!((answer.point - DVec3::new(60.0, 1.0, 0.0)).length() <= 0.5, "{:?}", answer.point);
    }

    #[test]
    fn the_world_axes_are_lines_of_the_sketch_plane() {
        let answer = snapped(&shelf(), request(down(200.0, 3.0), only(SnapKind::Axis)));
        assert_eq!(answer.kind, Some(SnapKind::Axis));
        // The axis is a line of the plane, so its point is on the plane and not on a board above it.
        assert!(close(answer.point, DVec3::new(200.0, 0.0, 0.0)), "{:?}", answer.point);
    }

    #[test]
    fn the_grid_rounds_the_planes_own_quantum() {
        let answer = snapped(&shelf(), request(down(203.0, 207.0), only(SnapKind::Grid)));
        assert_eq!(answer.kind, Some(SnapKind::Grid));
        assert!(close(answer.point, DVec3::new(200.0, 210.0, 0.0)), "{:?}", answer.point);
    }

    #[test]
    fn the_axis_lock_keeps_one_coordinate_and_the_grid_rounds_the_other() {
        let mut request = request(down(203.0, 207.0), only(SnapKind::Grid));
        request.axis_lock = true;
        request.anchor = Some(DVec3::new(100.0, 100.0, 0.0));
        let answer = snapped(&shelf(), request);
        assert!(answer.locked, "the hand asked for a straight segment");
        assert_eq!(answer.kind, None, "the lock is a constraint, not a kind");
        assert!(close(answer.point, DVec3::new(100.0, 210.0, 0.0)), "{:?}", answer.point);
    }

    #[test]
    fn the_face_the_tool_measured_stands_above_the_grid_and_below_a_corner() {
        let project = shelf();
        let mut request = request(down(403.0, 253.0), SnapKinds::ALL);
        request.face = Some(DVec3::new(403.0, 253.0, 18.0));
        let answer = snapped(&project, request);
        assert_eq!(answer.kind, Some(SnapKind::Face), "the surface the pointer points at");
        assert!(close(answer.point, DVec3::new(403.0, 253.0, 18.0)), "{:?}", answer.point);

        // A corner still wins over the face, because a corner is an exact point of the figure — and
        // a corner *behind* the face the tool measured does not, because the maker cannot see it.
        let mut near_corner = request;
        near_corner.ray_origin = DVec3::new(698.0, 499.0, 500.0);
        near_corner.face = Some(DVec3::new(698.0, 499.0, 0.0));
        assert_eq!(snapped(&project, near_corner).kind, Some(SnapKind::Vertex));
        near_corner.face = Some(DVec3::new(698.0, 499.0, 0.0));
        let mut behind = near_corner;
        behind.ray_origin = DVec3::new(698.0, 499.0, 500.0);
        behind.face = Some(DVec3::new(698.0, 499.0, 400.0));
        assert_eq!(
            snapped(&project, behind).kind,
            Some(SnapKind::Face),
            "a corner behind the face is hidden from the eye"
        );
    }

    #[test]
    fn a_ray_along_the_plane_answers_nothing() {
        let project = shelf();
        let ray = (DVec3::new(0.0, 0.0, 0.0), DVec3::X);
        assert!(project.snap(request(ray, SnapKinds::ALL), Sight::none()).is_none());
    }

    #[test]
    fn the_marks_are_the_corners_the_middles_and_the_centres() {
        let marks = shelf().snap_marks(Sight::none());
        let count = |kind| marks.iter().filter(|mark| mark.kind == kind).count();
        assert_eq!(count(SnapKind::Vertex), 8, "a box's eight corners");
        assert_eq!(count(SnapKind::Midpoint), 12, "the middle of each of its edges");
        assert_eq!(count(SnapKind::Centre), 6, "the middle of each of its six faces");
        assert!(marks.iter().all(|mark| mark.id == id(2)), "every mark names the node it belongs to");
    }

    #[test]
    fn a_hidden_node_offers_no_marks() {
        let project = shelf();
        let hidden: HashSet<Id> = [id(2)].into_iter().collect();
        assert!(!project.snap_marks(Sight::none()).is_empty());
        assert!(project.snap_marks(Sight::of(&hidden)).is_empty(), "a hidden shelf has no corners");
    }

    #[test]
    fn a_swept_circle_answers_its_rims_and_its_axis_and_no_corner_of_its_box() {
        // U120: a cylinder is a drawn circle swept on Z. Its own places are the two rims and the
        // middle of each end; the square around its base used to answer with eight corners in the
        // air, while its upper rim and its axis were nowhere.
        let project = swept_circle(100, 200);
        let rim = snapped(&project, request(down(100.5, 0.5), only(SnapKind::Vertex)));
        assert_eq!(rim.kind, Some(SnapKind::Vertex), "a quadrant of a rim");
        assert!((rim.point.x - 100.0).abs() < 0.5 && rim.point.y.abs() < 0.5, "{:?}", rim.point);
        assert!(
            rim.point.z.abs() < 1.0e-6 || (rim.point.z - 200.0).abs() < 1.0e-6,
            "a rim point stands on the base or on the lifted ring: {:?}",
            rim.point
        );

        // The axis: the middle of an end face, which no rim offers.
        let axis = snapped(&project, request(down(0.5, 0.5), only(SnapKind::Centre)));
        assert_eq!(axis.kind, Some(SnapKind::Centre));
        assert!(axis.point.x.abs() < 0.5 && axis.point.y.abs() < 0.5, "on the axis: {:?}", axis.point);
        assert!(axis.point.z.abs() < 1.0e-6 || (axis.point.z - 200.0).abs() < 1.0e-6, "an end face: {:?}", axis.point);

        // The corner of the square around the base stands 41 mm off the rim, in the air.
        let air = snapped(&project, request(down(100.0, 100.0), scene_kinds()));
        assert_eq!(air.kind, None, "the corner of the square names nothing: {:?}", air.point);

        // Every mark the body offers is a place on it: a point of a rim at its radius, or a point
        // of the axis the two ends share. A micrometre of slack is the document's own quantum, which
        // the middle of a quarter arc is rounded to.
        let marks = project.snap_marks(Sight::none());
        assert!(!marks.is_empty());
        for mark in &marks {
            let radius = mark.point.truncate().length();
            assert!(
                (radius - 100.0).abs() <= 1.0e-3 || radius <= 1.0e-3,
                "{:?} is not on the swept circle (radius {radius})",
                mark.point
            );
        }
    }

    #[test]
    fn a_cone_answers_its_apex_its_base_circle_and_the_middle_of_each_end() {
        let cone = |base: i64, top: i64| {
            Primitive::cone(millimetres(base), millimetres(top), millimetres(200), 32).expect("a cone")
        };
        let sharp = body_of(Geometry::Primitive(cone(80, 0)));
        let apex = snapped(&sharp, request(down(1.0, 1.0), only(SnapKind::Vertex)));
        assert_eq!(apex.kind, Some(SnapKind::Vertex), "the apex");
        assert!(close(apex.point, DVec3::new(0.0, 0.0, 200.0)), "{:?}", apex.point);

        let rim = snapped(&sharp, request(down(0.5, 80.0), only(SnapKind::Edge)));
        assert_eq!(rim.kind, Some(SnapKind::Edge), "the base circle");
        assert!((rim.point - DVec3::new(0.5, 80.0, 0.0)).length() < 0.2, "{:?}", rim.point);

        // A truncated cone answers its top rim and the middle of its top face as well.
        let truncated = body_of(Geometry::Primitive(cone(80, 40)));
        let top = snapped(&truncated, request(down(40.5, 0.5), only(SnapKind::Vertex)));
        assert_eq!(top.kind, Some(SnapKind::Vertex), "a quadrant of the top rim");
        assert!(close(top.point, DVec3::new(40.0, 0.0, 200.0)), "{:?}", top.point);
        let middle = snapped(&truncated, request(down(0.5, 0.5), only(SnapKind::Centre)));
        assert_eq!(middle.kind, Some(SnapKind::Centre), "the middle of the top face");
        assert!(close(middle.point, DVec3::new(0.0, 0.0, 200.0)), "{:?}", middle.point);

        // Every mark stands inside the cone's own radii and height, so none of them is a corner of
        // the box around it.
        for mark in truncated.snap_marks(Sight::none()) {
            let radius = mark.point.truncate().length();
            let z = mark.point.z;
            assert!((-1.0e-3..=200.0 + 1.0e-3).contains(&z), "a mark at z = {z}: {:?}", mark.point);
            let allowed = 80.0 + (40.0 - 80.0) * (z / 200.0);
            assert!(radius <= allowed + 1.0e-3, "{:?} stands outside the cone", mark.point);
        }
    }

    #[test]
    fn every_mark_of_a_ball_lies_at_its_radius() {
        let ball = Primitive::sphere(millimetres(50), 16, 8).expect("a ball");
        let centre = DVec3::new(300.0, 20.0, 100.0);
        // A ball turned a quarter turn about X, so a mark taken from the world axes would show.
        let placed = Placement::new(centre, geom::DQuat::from_rotation_x(FRAC_PI_2)).expect("a place");
        let mut project = empty();
        let root = project.root().id();
        project.add_object(root, "Ball", placed, Geometry::Primitive(ball)).expect("a ball");

        let marks = project.snap_marks(Sight::none());
        assert_eq!(marks.len(), 7, "the middle, the two poles and the four equator quadrants");
        for mark in &marks {
            let distance = (mark.point - centre).length();
            assert!(
                distance <= 1.0e-6 || (distance - 50.0).abs() <= 1.0e-6,
                "{:?} stands {distance} mm from the middle, neither on it nor on the ball",
                mark.point
            );
        }

        // The poles follow the body's own frame: the turn about X stands them along the world Y.
        let pole = snapped(&project, request(down(300.5, -29.5), only(SnapKind::Vertex)));
        assert_eq!(pole.kind, Some(SnapKind::Vertex), "the pole");
        assert!(close(pole.point, DVec3::new(300.0, -30.0, 100.0)), "{:?}", pole.point);
    }

    #[test]
    fn a_swept_rectangle_keeps_the_marks_its_box_describes() {
        // The box *is* the body when the profile is a rectangle, so the places U116 promised a drawn
        // cube — the middle of every face and of every edge — stay exactly as they were (U120).
        let ring = Path::rectangle([millimetres(-100), millimetres(-75)], [millimetres(100), millimetres(75)])
            .expect("a rectangle");
        let box_ = Constructive::extrude(ring, millimetres(300)).expect("a box");
        let project = body_of(Geometry::Constructive(box_));
        // The drawn ring files the base corners a second time, which names a place and not a second
        // one: the counts below are of distinct points.
        assert_eq!(distinct(&project, SnapKind::Vertex), 8, "the eight corners");
        assert_eq!(distinct(&project, SnapKind::Midpoint), 12, "the middle of every edge");
        assert_eq!(distinct(&project, SnapKind::Centre), 6, "the middle of every face");

        let face = snapped(&project, request(down(100.0, 0.5), only(SnapKind::Centre)));
        assert_eq!(face.kind, Some(SnapKind::Centre), "the middle of a side face");
        assert!((face.point - DVec3::new(100.0, 0.0, 150.0)).length() < 0.5, "{:?}", face.point);
    }

    #[test]
    fn a_flat_drawing_answers_its_own_points_and_not_the_corners_of_its_box() {
        let diagonal = open_path(vec![[millimetres(0), millimetres(0)], [millimetres(100), millimetres(50)]]);
        let project = body_of(Geometry::Path(diagonal));
        let marks = project.snap_marks(Sight::none());
        assert_eq!(marks.len(), 3, "the two ends and the middle of the one line");
        assert!(
            !marks.iter().any(|mark| close(mark.point, DVec3::new(0.0, 50.0, 0.0))),
            "a corner of the box around a diagonal line is empty air"
        );
    }

    #[test]
    fn the_kinds_are_listed_in_priority_order() {
        assert_eq!(SnapKind::ALL[0], SnapKind::Vertex, "a corner is the first kind");
        assert_eq!(SnapKind::ALL.last().copied(), Some(SnapKind::Grid), "the grid is the last");
        assert!(SnapKind::ALL.windows(2).all(|pair| pair[0] < pair[1]), "the order is the derived one");

        let mut kinds = SnapKinds::none();
        assert!(!kinds.any(), "nothing is on");
        kinds.set(SnapKind::Centre, true);
        assert!(kinds.any() && kinds.enabled(SnapKind::Centre) && !kinds.enabled(SnapKind::Vertex));
        assert!(!kinds.all_on(), "one kind on is not the whole set");
        assert_eq!(SnapKinds::default(), SnapKinds::ALL, "a fresh workspace snaps everything");
        assert!(SnapKinds::ALL.all_on(), "and every kind is on in it");

        // The master switch: all off, then all on again, one value either way.
        kinds.set_all(true);
        assert_eq!(kinds, SnapKinds::ALL, "the master turns every kind on");
        kinds.set_all(false);
        assert_eq!(kinds, SnapKinds::none(), "and every kind off");
        assert!(!kinds.any() && !kinds.all_on());
    }
}
