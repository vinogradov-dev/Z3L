//! The object move rule: how far one drag along a world axis may travel, and what a placement may
//! not come to rest inside.
//!
//! The translate gizmo proposes a world distance and this module answers with the distance the
//! document allows. The answer is about the **place the drag lands**, not the path it crosses: a
//! destination that is free is taken whole, so a piece is carried past an obstacle into the free
//! place beyond it, and only a destination that is not free is cut at the first face in the way
//! (ADR 0166). Four facts decide it and none of them is a code literal:
//!
//! - the **axes** the drag offers: a panel slides only along its own thickness, so the two in-plane
//!   directions are refused, and a floor-bound product keeps its height (N22, ADR 0147); a piece free
//!   on more than one axis is answered axis by axis, so a contact stops only the axis it meets and
//!   the free axes keep carrying it along the face (X74);
//! - the **mount** trait of a product's class: a `Floor` product keeps its height, a `Wall` product
//!   moves freely (ADR 0097);
//! - **contact**: a body stops at a body, a shell at a shell, and a carcass at a solid body that is
//!   not its own piece, while a legitimate nesting — a piece in its case, a unit inside another —
//!   stays legal (ADR 0102). A furniture piece keeps the world-box reading it always had; any other
//!   solid body is measured by its own faces, so a turned body stops where it touches and not at the
//!   box around it (ADR 0165);
//! - **snapping**: the moved box's faces fall onto the world coordinate planes (the floor included),
//!   onto the virtual faces of the product frames around it, onto a neighbouring part's face, or
//!   onto the 1 mm grid. A body measured by its own faces takes the planes and the grid and not
//!   another part's world box, which is the coarse reading it does not share (ADR 0165);
//! - **the floor**: the workspace keeps an invisible floor at Z = 0, and it is a lower bound on every
//!   moved body rather than a capture, so it stops a drag whatever the snapping is set to and stays
//!   off when the workspace turns it off (X87, ADR 0177).
//!
//! Which of those capture a drag, and how near one has to be, is not a code literal but a
//! [`SnapSettings`] value the shell reads from the workspace settings and passes in: snapping is a
//! taste and a workshop standard (ADR 0146). A capture is reported with the surface it landed on, so
//! the shell can draw the line the piece jumped to and measure its field against that same surface.
//!
//! The axes are resolved in one pass and each is decided where the axes before it put the piece: the
//! three answers are one placement, tested as one, so no combination of them can land the piece
//! inside a turned neighbour (ADR 0166).
//!
//! The same contact test is the document's own rule and not the tool's: [`Project::resting_conflict`]
//! answers whether a placement a caller asks to write would land a part inside one that stays behind,
//! and the placement commands refuse it (X72, ADR 0162). The write asks [`resting_inside`] and the
//! drag stops where that answer would refuse, so a drag reaches a contact before it reaches a
//! sentence. Contact is the document's and not a tool's, so the free move obeys it too, and the only
//! rule the free move does without is snapping (ADR 0162).
//!
//! A pair the document already holds overlapping has one way to go, and it is out: the place the part
//! stands in is the deepest one it may keep, so a destination that sinks it further is refused, and
//! the drag's rule reads that depth with the same number the write's guard does (X86, ADR 0178).
//!
//! The answer is derived from the placements the document holds, which during a drag are still the
//! drag's start placements, so the preview and the single `node.placement.set_many` on release
//! cannot disagree. Nothing is stored and nothing is mutated.
//!
//! [`Project::move_rule`] answers with the rule wherever a piece can be measured, under one
//! [`MoveSettings`] value the shell reads from the workspace. The panel axis is the one part of the
//! rule that needs no surface, so [`Project::move_axes`] answers it wherever the board stands; the
//! free move asks the same rule as [`MoveSettings::free`] reads it — all three axes and nothing to
//! snap to — and the floor stops it all the same (ADR 0162, ADR 0177).

use std::rc::Rc;
use std::sync::{Arc, OnceLock};

use geom::{Contact, DQuat, DVec3, Pose, Shared, overlap as overlaps};

use super::contact_shape;
use super::surface::{CONTACT_MM, Span, SurfaceBox, SurfaceKind, extent, separating_axes, surface_box};
use crate::project::Project;
use crate::tree::{object_bounds_mm, panel_bounds};
use crate::*;

/// The world axis an object drag runs along.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MoveAxis {
    /// The world X axis.
    X,
    /// The world Y axis.
    Y,
    /// The world Z axis, up.
    Z,
}

impl MoveAxis {
    /// All three axes in the order [`MoveAxis::index`] counts them.
    pub const ALL: [Self; 3] = [Self::X, Self::Y, Self::Z];

    /// The two axes perpendicular to this one, in frame order.
    const fn perpendicular(self) -> [Self; 2] {
        match self {
            Self::X => [Self::Y, Self::Z],
            Self::Y => [Self::X, Self::Z],
            Self::Z => [Self::X, Self::Y],
        }
    }

    /// The unit vector of this axis in the world.
    pub const fn direction(self) -> DVec3 {
        match self {
            Self::X => DVec3::X,
            Self::Y => DVec3::Y,
            Self::Z => DVec3::Z,
        }
    }

    /// Its slot in an axis table: X, Y, Z in that order.
    pub const fn index(self) -> usize {
        match self {
            Self::X => 0,
            Self::Y => 1,
            Self::Z => 2,
        }
    }
}

/// Every axis, for a drag the document has no rule about.
const ALL_MOVE_AXES: [bool; 3] = [true, true, true];

/// The asks one ring drag may make while it bisects a refused angle.
///
/// A turn of a full revolution is halved to [`TURN_RESOLUTION`] in twenty-three passes; the cap is
/// what keeps a hostile angle from spinning, and it is past the point the bisection has settled.
const MAX_TURN_STEPS: usize = 32;

/// How near the bisection must come to the refusing angle, in radians.
///
/// A micrometre of a turn at the far end of a workshop: a five-metre board sweeps 0.005 mm in it,
/// which is the document's own rounding and not a face (X88).
const TURN_RESOLUTION: f64 = 1.0e-6;

/// How far an object drag may go, and what stopped it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoveLimit {
    /// The signed distance the drag may travel along its world axis, in millimetres.
    ///
    /// It is a whole number of micrometres, which is the document's unit, so the preview and the
    /// committed placement are the same number.
    pub distance_mm: f64,
    /// What the drag stopped at, when something stopped it before the pointer did.
    pub contact: Option<Stop>,
    /// The surface the drag's face landed on, when a snap — and not the pointer — placed it.
    ///
    /// `None` while the drag runs on the grid, and `None` when a contact cut the drag short of the
    /// surface the snap had picked: the face landed on the contact, which is reported on its own.
    pub capture: Option<Capture>,
}

/// The end of a moved box that landed on a surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SnapEnd {
    /// The face at the low end of the drag axis.
    Low,
    /// The face at the high end of the drag axis.
    High,
}

/// What a snapped face landed on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SnapSurface {
    /// A world coordinate plane through the origin — X=0, Y=0 or Z=0, the floor included.
    ///
    /// Until a house-wall tool exists these planes stand in for the walls of a room (ADR 0146).
    Plane,
    /// Another product's authored envelope: its virtual face.
    Frame(Id),
    /// A panel's or a body's own face.
    Face(Id),
    /// The middle of another part's box: the coordinate its faces do not offer.
    Centre(Id),
}

/// The surface a dragged face landed on, and where that surface is.
///
/// A snap moves the piece on its own, so a capture the maker cannot see is a drag that feels
/// broken: the shell draws a line where this surface stands and measures the drag's field against
/// it, which is why the coordinate travels with the answer (ADR 0146).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Capture {
    /// Which face of the moved box landed.
    pub end: SnapEnd,
    /// The world coordinate of the captured surface along the drag axis, in millimetres.
    pub at_mm: f64,
    /// The surface the face landed on.
    pub surface: SnapSurface,
}

/// What stopped a drag short of the place the hand asked for.
///
/// A drag stops at a part of the document or at the workspace's invisible floor, and the two are one
/// answer so a log names either without a second field beside it (X87, ADR 0177).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Stop {
    /// A part of the document that met the piece.
    Node {
        /// The part that met it.
        id: Id,
        /// Which of its surfaces met: a panel face, a body, or a product's envelope.
        kind: SurfaceKind,
        /// The world bounds of that part, in millimetres.
        min: DVec3,
        /// The world bounds of that part, in millimetres.
        max: DVec3,
    },
    /// The workspace's invisible floor at Z = 0 (ADR 0177).
    Floor,
}

impl Stop {
    /// The part that stopped the drag, or `None` when the floor did.
    pub const fn node(self) -> Option<Id> {
        match self {
            Self::Node { id, .. } => Some(id),
            Self::Floor => None,
        }
    }

    /// The stop a part of the document gives a drag.
    pub(crate) fn part(obstacle: SurfaceBox) -> Self {
        Self::Node { id: obstacle.id, kind: obstacle.kind, min: obstacle.min, max: obstacle.max }
    }
}

/// What an object drag snaps to, and how near.
///
/// The values are a workspace setting (`crates/ui/src/settings.rs` holds the window and the file),
/// so the model owns their meaning and their defaults but never stores them: [`Project::move_rule`]
/// is handed the value, and a rule asked with the defaults behaves the way the editor always has
/// (ADR 0146).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SnapSettings {
    /// Which kinds capture: the world axes (the coordinate planes X=0, Y=0 and Z=0, the floor
    /// included), the face of another part, its centre, and the pointer kinds a drag does not read
    /// (U116).
    ///
    /// A drag answers a distance along one axis, so of the pointer kinds only `centre` adds a
    /// capture: every corner and edge midpoint of a box stands at one of its own spans, and the
    /// centre is the one coordinate the faces do not offer.
    pub kinds: SnapKinds,
    /// Whether another product's envelope captures a drag with its virtual faces.
    pub frames: bool,
    /// How near a surface must be, in millimetres, to capture a drag and make the piece jump.
    ///
    /// It is the distance at which the snap fires, not a gap the piece keeps: a capture lands the
    /// faces together. A negative value captures nothing.
    pub tolerance: Length,
}

impl SnapSettings {
    /// The tolerance a workspace that has not chosen one snaps at, in millimetres.
    ///
    /// Five millimetres is the maker's own reach: near enough that a piece only jumps when it was
    /// meant to touch, wide enough that the pointer does not have to land on the millimetre.
    pub const DEFAULT_TOLERANCE_MM: i64 = 5;
}

impl Default for SnapSettings {
    fn default() -> Self {
        Self {
            kinds: SnapKinds::ALL,
            frames: true,
            tolerance: Length::from_micrometres(Self::DEFAULT_TOLERANCE_MM * 1000),
        }
    }
}

/// Which world axes one drag offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveAxes {
    /// The document's answer: a panel travels along its own thickness, a floor-bound product keeps
    /// its height (N22, ADR 0147).
    Document,
    /// All three world axes, whatever the piece is: the free move keeps its three arrows (ADR 0151).
    Every,
}

/// How the workspace measures one drag: the axes, what it snaps to, and whether the floor stops it.
///
/// Every value here belongs to the workspace and is read once, when a piece is taken, so a change
/// applies to the next drag and never to the one under the hand (ADR 0146). They travel as one value
/// because a drag asks them together, and the two tools are two named readings of it: the object move
/// asks the document for its axes, the free move keeps all three (X87, ADR 0177).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MoveSettings {
    /// Which world axes the drag offers.
    pub axes: MoveAxes,
    /// What the drag captures, or `None` for a drag no surface and no coordinate plane may move.
    pub snaps: Option<SnapSettings>,
    /// Whether the workspace's invisible floor at Z = 0 is a lower bound on every moved body.
    pub floor: bool,
    /// Whether the document's bodies measure this drag at all.
    ///
    /// The object tool is the one that obeys the body rules: it snaps, it stops at a neighbour and it
    /// keeps the floor. The free move is the maker's own hand — a body goes where the pointer puts it,
    /// through a neighbour and below the floor — and a rotation is never measured (X85, ADR 0180).
    pub measured: bool,
}

impl Default for MoveSettings {
    fn default() -> Self {
        Self { axes: MoveAxes::Document, snaps: Some(SnapSettings::default()), floor: true, measured: true }
    }
}

impl MoveSettings {
    /// The free move's own reading: every axis, nothing to snap to, and nothing to measure.
    ///
    /// The free move is not the object tool: it carries a body by the pointer's own distance, through
    /// whatever stands in the way and below the workspace's floor, because the control belongs to the
    /// tool that obeys the document's rules and not to the hand that does not (ADR 0180). It keeps the
    /// object tool's pivots, its preview and its one undo entry, and nothing else.
    pub const fn free(self) -> Self {
        Self { axes: MoveAxes::Every, snaps: None, floor: false, measured: false }
    }
}

/// The grid step of an object move, in millimetres.
///
/// The maker's own clearance is one millimetre (ADR 0102), so an object drag lands on the whole
/// millimetre unless a surface captures it first.
const GRID_MM: f64 = 1.0;

impl Project {
    /// The rule that measures a drag of `moving`, or `None` when nothing movable is selected.
    ///
    /// The surfaces, the mount trait and the snap settings are read here, once, so the drag can ask
    /// the rule for every pointer position without walking the tree again: nothing the rule measured
    /// moves while the pointer travels, and a settings change mid-drag does not bend the preview
    /// away from the placement the release commits.
    ///
    /// Every selection is a drag of the object move, so every one of them lands on the workspace's
    /// snaps (ADR 0165). What each moved node allows, and how its contact is measured, is decided by
    /// the node: a furniture piece keeps its class rules and the world-box reading, any other solid
    /// body is free in all three axes and is measured by its own faces.
    pub fn move_rule(&self, moving: &[Id], settings: MoveSettings, contact: Option<&dyn Contact>) -> Option<MoveRule> {
        let targets = self.moved_targets(moving);
        let axes = match settings.axes {
            MoveAxes::Document => self.axes_of(&targets),
            MoveAxes::Every => ALL_MOVE_AXES,
        };
        self.rule_of(&targets, axes, settings, contact)
    }

    /// The rule that measures a turn of `moving` about the anchor's pivot, or `None` when nothing
    /// the selection holds can be carried.
    ///
    /// A ring drag turns the selection about one world point — the stored pivot of the last movable
    /// node, which is the anchor the rotation commit uses too — so the rule is built from the same
    /// movers and the same obstacles the move rule reads, and the whole turn is asked as one
    /// placement before it is cut. Nothing is prepared here beyond the boxes: a body's shape is built
    /// when a probe first reaches for it (X88, X90).
    pub fn rotate_rule(&self, moving: &[Id], contact: Option<&dyn Contact>) -> Option<RotateRule> {
        let targets = self.moved_targets(moving);
        let movers = self.movers(&targets, contact);
        if movers.is_empty() {
            return None;
        }
        let pivot = self.pivot_world(*targets.last()?)?;
        if !pivot.is_finite() {
            return None;
        }
        Some(RotateRule {
            pivot,
            movers,
            obstacles: self.obstacles_outside_a_tool(&targets),
            contact: contact.map(Contact::handle),
        })
    }

    /// The rule that measures a drag of `targets` under the workspace's settings.
    ///
    /// The axes are the caller's: the object tool hands the intersection of what every moved node
    /// allows, so a loose board keeps its own thickness axis wherever it stands (N22, ADR 0147), and
    /// the free tool hands all three.
    fn rule_of(
        &self,
        targets: &[Id],
        axes: [bool; 3],
        settings: MoveSettings,
        contact: Option<&dyn Contact>,
    ) -> Option<MoveRule> {
        if targets.is_empty() {
            return None;
        }
        let movers = self.movers(targets, contact);
        if movers.is_empty() {
            return None;
        }
        let floor = settings.floor.then(|| floor_bound(&movers)).flatten();
        // A drag the document does not measure carries no obstacles at all: the free move goes where
        // the hand puts it and only the pointer's own distance reaches the placement (ADR 0180).
        let obstacles = if settings.measured { self.obstacles_outside_a_tool(targets) } else { Vec::new() };
        Some(MoveRule { axes, obstacles, floor, movers, snaps: settings.snaps, contact: contact.map(Contact::handle) })
    }

    /// The part a proposed placement change would put inside another part, if any.
    ///
    /// This is the document's rule, asked before the write lands and not only under a gesture
    /// (ADR 0162): a body may not come to rest inside another body and a shell may not end up half
    /// inside another shell, while a piece inside a case's envelope stays legal (ADR 0102). The rule
    /// reads the document, never the sight: a part switched off in the tree is as much in the way as
    /// a visible one, because what is hidden is the picture and the pointer (X73).
    ///
    /// `placements` is what the caller asks for: every node whose stored placement changes, and the
    /// placement it receives. A container travels with its children, and a batch may name a container
    /// and one of its descendants. A pair that only touches is contact, not an overlap. A pair that
    /// already overlaps is the maker's to repair, and the repair has one direction: the depth the part
    /// stands at is the deepest it may keep, so every placement that sinks it further is refused while
    /// every way out is granted. A file written before this rule still opens and is repaired by hand
    /// (X86, ADR 0178).
    pub fn resting_conflict(&self, placements: &[(Id, Placement)], contact: Option<&dyn Contact>) -> Option<Resting> {
        let asked: Vec<Id> = placements.iter().map(|(id, _)| *id).collect();
        let targets = self.moved_targets(&asked);
        if targets.is_empty() {
            return None;
        }
        let obstacles = self.obstacles_outside_a_tool(&targets);
        for moved in &targets {
            let Some(lands) =
                self.world_placement_with(*moved, placements).and_then(|world| self.box_at(*moved, world))
            else {
                continue;
            };
            let stood = self.mover_box(*moved);
            let product = self.product_of(*moved);
            // The guard reads what the drag reads: a solid body by its own faces when the port can
            // describe it, a furniture piece by the world box it has always been measured by.
            let faces = !self.is_furniture(*moved);
            let landed = Measured::new(lands, faces.then(|| self.solid_of(lands, contact)).flatten());
            let held = stood.map(|box_| Measured::new(box_, faces.then(|| self.solid_of(box_, contact)).flatten()));
            let found = obstacles
                .iter()
                .position(|obstacle| resting_inside(held.clone(), &landed, &obstacle.measured(contact), product));
            if let Some(index) = found {
                return Some(Resting { moved: *moved, standing: obstacles.get(index)?.box_.id });
            }
        }
        None
    }

    /// Whether two nodes share volume, by the same reading the write refuses (ADR 0162, ADR 0178).
    ///
    /// A body inside a shell is a piece inside a case and stays legal; two boxes that only touch
    /// share no volume, so a face is contact and never an overlap; a body the contact port can
    /// describe is measured by its own faces and a furniture piece by the world box it has always
    /// been measured by ([`Self::resting_conflict`]). The inspector asks this of every pair of parts
    /// in a product, which is why it is public: a second reading of "two bodies intersect" would be
    /// a rule the write and the report could disagree about.
    pub fn parts_overlap(&self, first: Id, second: Id, contact: Option<&dyn Contact>) -> bool {
        if first == second {
            return false;
        }
        let (Some(first_box), Some(second_box)) = (self.mover_box(first), self.mover_box(second)) else {
            return false;
        };
        if nested(first_box, second_box, self.product_of(first))
            || nested(second_box, first_box, self.product_of(second))
        {
            return false;
        }
        let measure = |id: Id, box_: SurfaceBox| {
            let faces = !self.is_furniture(id);
            Measured::new(box_, faces.then(|| self.solid_of(box_, contact)).flatten())
        };
        measure(first, first_box).meets(&measure(second, second_box))
    }

    /// The obstacles a move or a turn of `targets` must respect.
    ///
    /// A body the maker marked as a **tool** passes through parts: it has to, because a cutter that
    /// cannot be carried into the body cannot cut it, and the mark is what says "this body is a
    /// tool, not a part" (W2, ADR 0197). Everything else keeps the rule that a part may not come to
    /// rest inside another, and the filter is applied once per rule instead of once per pair (X90).
    fn obstacles_outside_a_tool(&self, targets: &[Id]) -> Vec<Obstacle> {
        let marked = |id: Id| self.object(id).is_some_and(Object::is_tool);
        if targets.iter().any(|target| marked(*target)) {
            return Vec::new();
        }
        self.obstacles(targets).into_iter().filter(|obstacle| !marked(obstacle.box_.id)).collect()
    }

    /// The world axes a move of this selection offers, in [`MoveAxis`] order.
    ///
    /// A selection offers what every one of its top-most nodes offers, so a panel beside a frame is
    /// held to the panel's one axis. A `false` slot is an axis the tool does not draw and does not
    /// start a drag on: an armed handle that refuses the press is worse than no handle at all
    /// (ADR 0113).
    ///
    /// The answer is the tool's, not the document's, so it does not need the surfaces a full
    /// [`Project::move_rule`] collects: the panel axis holds wherever a panel stands, and a drag of
    /// any other body reads it without walking the tree.
    pub fn move_axes(&self, moving: &[Id]) -> [bool; 3] {
        let targets = self.moved_targets(moving);
        if targets.is_empty() {
            return ALL_MOVE_AXES;
        }
        self.axes_of(&targets)
    }

    /// The top-most moved nodes of a selection, without the root.
    fn moved_targets(&self, moving: &[Id]) -> Vec<Id> {
        let root = self.root.id();
        self.top_most_nodes(moving).into_iter().filter(|id| *id != root).collect()
    }

    /// The world axes a drag of `targets` offers: the intersection of what each one allows.
    fn axes_of(&self, targets: &[Id]) -> [bool; 3] {
        let mut axes = ALL_MOVE_AXES;
        for target in targets {
            let allowed = self.node_axes(*target);
            for (slot, allowed) in axes.iter_mut().zip(allowed) {
                *slot &= allowed;
            }
        }
        axes
    }

    /// The world axes one moved node offers an object drag.
    ///
    /// A **panel** offers one: the axis its own thickness points most nearly at, so it slides parallel
    /// to its own A and B faces and never across them. That is the whole of the panel rule and it is
    /// what the trade means: a shelf rises and falls in its thickness direction, a divider crosses the
    /// case and a back moves in and out (N22, ADR 0147).
    ///
    /// A **floor-bound product** keeps its height, so the vertical is refused by the class's `mount`
    /// trait (ADR 0097). Everything else — a wall-hung frame, a body, a group — is free on all three.
    fn node_axes(&self, id: Id) -> [bool; 3] {
        if self.panel(id).is_some() {
            return self.panel_axis(id).map_or(ALL_MOVE_AXES, |axis| {
                let mut axes = [false; 3];
                if let Some(slot) = axes.get_mut(axis.index()) {
                    *slot = true;
                }
                axes
            });
        }
        if self.keeps_height(id) {
            // A floor-bound product still slides along the floor and sticks to the pieces beside it.
            return [true, true, false];
        }
        ALL_MOVE_AXES
    }

    /// The world axis a panel slides along: the one its thickness points most nearly at.
    ///
    /// A panel's own frame puts its thickness on local Z, so the world normal is that axis turned by
    /// every frame above it. The two A and B faces stay parallel to each other under any translation,
    /// so what the rule holds is not the board's orientation but its own normal: the only direction it
    /// may travel is the one it is a slab across. A board turned off the world axes slides along the
    /// axis it leans towards, because every surface the rule measures is a world box (ADR 0102).
    fn panel_axis(&self, id: Id) -> Option<MoveAxis> {
        let normal = self.world_placement(id)?.transform_vector(DVec3::Z);
        let mut best = MoveAxis::X;
        let mut widest = 0.0;
        for axis in MoveAxis::ALL {
            let reach = normal.dot(axis.direction()).abs();
            if reach.is_finite() && reach > widest {
                widest = reach;
                best = axis;
            }
        }
        (widest > 0.0).then_some(best)
    }

    /// Whether `id` is a product whose class mounts on the floor, and so keeps its height.
    fn keeps_height(&self, id: Id) -> bool {
        self.container(id).is_some_and(Container::is_product)
            && self.class(id).and_then(|class| class.traits()).and_then(|traits| traits.mount()) == Some(Mount::Floor)
    }

    /// Whether `id` is furniture: a product, or a node inside a product.
    fn is_furniture(&self, id: Id) -> bool {
        self.container(id).is_some_and(Container::is_product) || self.product_of(id).is_some()
    }

    /// The world box each moved node carries, whether its contact is measured by its own faces, and
    /// the body the contact port measures when a drag has one.
    fn movers(&self, targets: &[Id], contact: Option<&dyn Contact>) -> Vec<Mover> {
        targets
            .iter()
            .filter_map(|id| {
                self.mover_box(*id).map(|box_| {
                    let faces = !self.is_furniture(*id);
                    Mover {
                        box_,
                        faces,
                        // Only a body measured by its own faces is handed to the port: a furniture
                        // piece keeps the world-box reading the trade has always used (ADR 0165).
                        solid: faces.then(|| self.solid_of(box_, contact)).flatten(),
                        product: self.product_of(*id),
                    }
                })
            })
            .collect()
    }

    /// Every surface a move of `targets` stays behind from, with the geometry each is measured by.
    ///
    /// Nothing is prepared here: the shape of a body is built when a pair first needs it, so a drag
    /// in a scene of a few hundred bodies prepares the handful it actually reaches (X90).
    fn obstacles(&self, targets: &[Id]) -> Vec<Obstacle> {
        self.obstacle_boxes(targets)
            .into_iter()
            .map(|box_| Obstacle {
                box_,
                geometry: self.object(box_.id).map(|object| object.geometry().clone()),
                solid: Rc::new(OnceLock::new()),
            })
            .collect()
    }

    /// One surface's own body, prepared by the contact adapter, or `None` when the document cannot
    /// describe its shape or no adapter is installed and the box reading stays.
    fn solid_of(&self, surface: SurfaceBox, contact: Option<&dyn Contact>) -> Option<Shared> {
        let contact = contact?;
        let shape = contact_shape::node_shape(self, surface.id, surface.local_min, surface.local_max)
            .or_else(|| contact_shape::box_between(surface.local_min, surface.local_max))?;
        contact.prepare(&shape, Pose::new(surface.world.translation(), surface.world.rotation()))
    }

    /// The world axis-aligned box a node occupies, in millimetres.
    ///
    /// It is the same box the move rule measures: a product's envelope, a panel's or object's own
    /// bounds, and for any other container the union of its parts. The distance fields of a drag
    /// read it, so the number they show and the contact the rule stops at are one measurement.
    pub fn world_box(&self, id: Id) -> Option<(DVec3, DVec3)> {
        self.mover_box(id).map(|surface| (surface.min, surface.max))
    }

    /// The world box one moved node carries: a product's envelope, a panel's or object's own
    /// bounds, and for any other container the union of the parts below it.
    fn mover_box(&self, id: Id) -> Option<SurfaceBox> {
        self.box_at(id, self.world_placement(id)?)
    }

    /// The world box a node occupies when it stands at `world`.
    ///
    /// The same box at the placement the document holds is [`Project::mover_box`]; a proposed write is
    /// measured through this one, so the refusal and the drag read one definition of a part's volume.
    fn box_at(&self, id: Id, world: Placement) -> Option<SurfaceBox> {
        if let Some(container) = self.container(id) {
            if container.is_product() {
                return self.envelope_at(id, world);
            }
            let (min, max) = panel_bounds(container.children(), Placement::IDENTITY, 0)?;
            return surface_box(id, min, max, world, SurfaceKind::Body);
        }
        if let Some(panel) = self.panel(id) {
            let size = panel.nominal_size();
            let max = DVec3::new(
                size.length().millimetres_f64(),
                size.width().millimetres_f64(),
                panel.last_known_thickness().millimetres_f64(),
            );
            return surface_box(id, DVec3::ZERO, max, world, SurfaceKind::Panel);
        }
        let object = self.object(id)?;
        let (min, max) = object_bounds_mm(object.geometry())?;
        surface_box(id, min, max, world, SurfaceKind::Body)
    }

    /// The world box of a product's authored envelope.
    fn envelope_box(&self, id: Id) -> Option<SurfaceBox> {
        self.envelope_at(id, self.world_placement(id)?)
    }

    /// The world box of a product's envelope when it stands at `world`.
    fn envelope_at(&self, id: Id, world: Placement) -> Option<SurfaceBox> {
        let envelope = self.container(id)?.envelope()?;
        let size =
            DVec3::new(envelope.x().millimetres_f64(), envelope.y().millimetres_f64(), envelope.z().millimetres_f64());
        surface_box(id, DVec3::ZERO, size, world, SurfaceKind::Shell)
    }

    /// The surfaces an object drag may meet: every other product's envelope, and every other
    /// panel and object.
    fn obstacle_boxes(&self, targets: &[Id]) -> Vec<SurfaceBox> {
        let mut boxes: Vec<SurfaceBox> = self
            .products()
            .into_iter()
            .filter(|product| !covered_by(self, *product, targets))
            .filter_map(|product| self.envelope_box(product))
            .collect();
        self.body_boxes(&self.root, targets, &mut boxes, 1);
        boxes
    }

    /// Collects the panels and objects below `container` that stay behind when `targets` move.
    fn body_boxes(&self, container: &Container, targets: &[Id], boxes: &mut Vec<SurfaceBox>, depth: u32) {
        if depth > Self::MAX_ASSEMBLY_DEPTH {
            return;
        }
        for child in container.children() {
            match child {
                Node::Panel(_) | Node::Object(_) => {
                    if covered_by(self, child.id(), targets) {
                        continue;
                    }
                    if let Some(box_) = self.mover_box(child.id()) {
                        boxes.push(box_);
                    }
                }
                Node::Container(inner) => self.body_boxes(inner, targets, boxes, depth.saturating_add(1)),
                Node::Camera(_) | Node::Light(_) => {}
            }
        }
    }
}

/// A part a placement would put inside another part.
///
/// It is what the write refuses and what the sentence names: the part being moved or created, and the
/// part that stays behind and is in the way (X72, ADR 0162).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Resting {
    /// The part the placement moves, or would create.
    pub moved: Id,
    /// The part that stays behind, which the moved box would come to rest inside.
    pub standing: Id,
}

/// Whether a box that lands at `now` comes to rest inside `obstacle`, where it did not before.
///
/// This is the document's own contact test, asked by [`Project::resting_conflict`] before a placement
/// is written (ADR 0162). A body meets a body and a shell meets a shell; a body inside a shell is a
/// piece inside a case and stays legal (ADR 0102). Two boxes that only touch share no volume, so a
/// face is contact and never a refusal, and the answer is exact rather than the world box around a
/// turned part: a board set flush in a case's corner shares its faces and nothing else.
///
/// `was` is the box the part stood with, or `None` for a part that does not exist yet. A pair that
/// already overlaps is the maker's to repair, and the repair has one direction: the place the part
/// stands in is the deepest one it may keep, so it comes out of another part and never further in
/// (X86, ADR 0178). A file written before this rule still opens, and its maker drags the part out and
/// types where it goes. The drag's own rule (`MoveRule::limit`) stops a furniture piece at the
/// world-box contact and a solid body at its exact face contact, and neither reading is later than
/// this answer, so a drag reaches a contact before it reaches a refusal (ADR 0165).
fn resting_inside(was: Option<Measured>, now: &Measured, obstacle: &Measured, product: Option<Id>) -> bool {
    // A legitimate nesting is not an overlap: a piece in its own case and a unit inside another stay
    // legal wherever they stand, and the drag's own destination test reads the same rule (X77).
    if nested(now.box_, obstacle.box_, product) {
        return false;
    }
    // A pair the document already holds overlapping is no new overlap only while the part is no
    // deeper: the depth it stands at is the most it may keep, so every motion is a way out
    // (X86, ADR 0178).
    if let Some(was) = was.filter(|was| was.meets(obstacle)) {
        return now.sunk(obstacle) > was.sunk(obstacle) + CONTACT_MM;
    }
    now.meets(obstacle)
}

/// One surface as both the world box and, when the contact port measured it, its own body.
///
/// The port is asked first and the box is the fallback: a body the document cannot describe, a
/// furniture piece and a session with no adapter installed all keep the reading they had (ADR 0179).
#[derive(Clone, Debug)]
struct Measured {
    /// The world box the rule already keeps for this surface.
    box_: SurfaceBox,
    /// The body the contact port prepared, when it could.
    solid: Option<Shared>,
}

impl Measured {
    /// One surface, with the body the port prepared for it when there is one.
    const fn new(box_: SurfaceBox, solid: Option<Shared>) -> Self {
        Self { box_, solid }
    }

    /// Whether the two are sunk into one another at all, by the port when both bodies exist.
    fn meets(&self, other: &Self) -> bool {
        match (self.solid.as_deref(), other.solid.as_deref()) {
            (Some(mine), Some(theirs)) => overlaps(mine, theirs, CONTACT_MM),
            _ => self.box_.meets(other.box_),
        }
    }

    /// How deeply this surface is sunk into `other`, in millimetres, by the same reading as
    /// [`Measured::meets`].
    fn sunk(&self, other: &Self) -> f64 {
        match (self.solid.as_deref(), other.solid.as_deref()) {
            (Some(mine), Some(theirs)) => mine.penetration(theirs).map_or(0.0, |sunk| sunk.depth_mm),
            _ => self.box_.depth(other.box_),
        }
    }
}

/// Whether one box legitimately holds the other, so the pair is no collision.
///
/// A body inside a shell is a piece inside a case, and a shell inside a shell is a unit inside
/// another; both are places a furniture project holds on purpose (ADR 0102). Two bodies have no
/// such place: a body inside a body is an overlap. Everything else — a carcass and a solid body
/// that merely stand beside each other — is measured, so a body no longer passes through a case and
/// the case no longer passes through the body (X77).
fn nested(held: SurfaceBox, holder: SurfaceBox, product: Option<Id>) -> bool {
    if held.is_shell() && holder.is_shell() {
        // Two products may nest, one wholly inside the other, and neither is a collision; two boxes
        // of one size stand in each other's place, which is an overlap and not a containment.
        return held.inside(holder) != holder.inside(held);
    }
    // A body is a piece of its own case and of nothing else: a shelf may sit inside the cabinet
    // whose child it is, and a lamp may not be pushed into that same cabinet's envelope (X77).
    product == Some(holder.id) && held.inside(holder)
}

/// One moved node's box, and the reading its contact is measured by.
///
/// A furniture piece keeps the world-box reading it always had: a panel inside a case is a slab in a
/// frame, and the box around a turned board is what the furniture rule has always measured
/// (ADR 0147, ADR 0162). Any other solid body is measured by its own faces, because the box around a
/// turned chair is not the chair: it stops the drag by the slack the turn adds, so the chair could not
/// be brought flush to an angled neighbour (ADR 0165).
#[derive(Clone, Debug)]
struct Mover {
    /// The world box the node occupies at the start of the drag.
    box_: SurfaceBox,
    /// The body the contact port measures, when the node is a solid body and an adapter is installed.
    ///
    /// A body's own surface is what a drag stops at, so a turned cone reaches the corner between two
    /// plates where its box would have stopped it a slack short (X85, ADR 0179). `None` keeps the
    /// world-box reading: a furniture piece, a body the document cannot describe, and a session with
    /// no adapter installed.
    solid: Option<Shared>,
    /// Whether contact with this node is measured by its own faces rather than by the world box.
    faces: bool,
    /// The product this node is a piece of, when it is one.
    ///
    /// A piece is legal inside its own case and nowhere else: a shelf inside its cabinet is a body
    /// in a shell, while a lamp pushed into the same cabinet is a body in a way (X77).
    product: Option<Id>,
}

/// One surface that stays behind, and the body the contact port measures it as.
///
/// The body is **not** prepared when the rule is built: a scene of a few hundred bodies would build a
/// hull for every one of them at the start of every drag, and a drag needs a handful. The shape is
/// built from the geometry it came from the first time a pair actually reaches for it, and the rule
/// keeps the adapter for that (X90, ADR 0181).
#[derive(Clone, Debug)]
struct Obstacle {
    box_: SurfaceBox,
    /// The geometry the body's own shape is built from, when the document describes it.
    geometry: Option<Geometry>,
    /// The body, once a pair has needed it: prepared on the first ask and shared by every clone.
    solid: Rc<OnceLock<Option<Shared>>>,
}

impl Obstacle {
    /// The body the adapter measures this surface as, prepared now when nothing has asked yet.
    ///
    /// `None` is the world-box reading: no adapter, or a shape the document cannot describe.
    fn body(&self, contact: Option<&dyn Contact>) -> Option<Shared> {
        let contact = contact?;
        self.solid
            .get_or_init(|| {
                let geometry = self.geometry.as_ref()?;
                let shape = contact_shape::body_shape(geometry, self.box_.local_min, self.box_.local_max)?;
                contact.prepare(&shape, Pose::new(self.box_.world.translation(), self.box_.world.rotation()))
            })
            .clone()
    }

    /// The surface with the body the adapter measures it as, or the box reading when it cannot.
    fn measured(&self, contact: Option<&dyn Contact>) -> Measured {
        Measured::new(self.box_, self.body(contact))
    }
}

impl std::ops::Deref for Obstacle {
    type Target = SurfaceBox;

    fn deref(&self) -> &SurfaceBox {
        &self.box_
    }
}

/// What one drag is measured against, taken once when the drag starts.
///
/// The document does not change while the pointer travels, so the moved boxes, the other surfaces
/// and the mount trait are read once and every later proposal is answered from them. Nothing is
/// stored in the document and nothing is mutated.
#[derive(Clone, Debug)]
pub struct MoveRule {
    /// Each moved node's world box at the start of the drag, and how its contact is measured.
    movers: Vec<Mover>,
    /// The surfaces that stay behind: the other products' envelopes, panels and objects, each with
    /// the body the contact port measures when the node can be described (`None` otherwise).
    obstacles: Vec<Obstacle>,
    /// Which world axes the drag offers, in [`MoveAxis`] order.
    ///
    /// It is the intersection of what every moved node allows, taken once when the drag starts: a
    /// panel's own thickness axis, a floor-bound product's two horizontal axes (N22, ADR 0147).
    axes: [bool; 3],
    /// What this drag snaps to, and how near, as the workspace settings chose it (ADR 0146), or
    /// `None` for a drag no grid and no surface may move: the free move.
    snaps: Option<SnapSettings>,
    /// The lowest this drag may travel down the world Z, in millimetres, when the workspace keeps a
    /// floor; `None` when it keeps none (X87, ADR 0177).
    floor: Option<f64>,
    /// The adapter that measures a body by its own faces, kept because the obstacles are prepared on
    /// demand and a drag asks the same rule every frame (X90, ADR 0181).
    contact: Option<Arc<dyn Contact>>,
}

impl MoveRule {
    /// The world axes the drag offers, in [`MoveAxis`] order.
    ///
    /// A caller that carries a piece on more than one axis reads this before it decides what the
    /// hand may propose: a panel offers one axis and a product the axes its mount leaves free.
    pub const fn axes(&self) -> [bool; 3] {
        self.axes
    }

    /// Whether a drag may move along `axis` at all.
    ///
    /// A panel offers only its own thickness axis and a floor-bound product keeps its height, so the
    /// other axes have no free distance; the handle list and the distance fields read this before
    /// offering a handle or a number (ADR 0113, N22).
    pub const fn allows(&self, axis: MoveAxis) -> bool {
        self.axes[axis.index()]
    }

    /// The distance a proposal may travel along a world axis.
    ///
    /// The proposal is zeroed on the vertical when a moved product is floor-bound, then snapped to
    /// the 1 mm grid or to a surface within [`SnapSettings::tolerance`] — or left exactly as it is
    /// when the drag snaps to nothing — and finally cut at the first surface in the way; the contact
    /// is reported so a caller can say what stopped the drag, and the capture so it can draw the
    /// surface the piece jumped to. A proposal that is not finite comes back as no movement.
    pub fn limit(&self, axis: MoveAxis, proposed_mm: f64) -> MoveLimit {
        if !proposed_mm.is_finite() {
            return MoveLimit { distance_mm: 0.0, contact: None, capture: None };
        }
        // An axis the drag does not offer moves nothing: a floor-bound product keeps its height and a
        // panel does not cross its own faces, and saying so here keeps the rule out of the gizmo.
        let distance = if self.allows(axis) { proposed_mm } else { 0.0 };
        let (snapped, capture) = match self.snaps {
            Some(snaps) => snap_distance(axis, &self.movers, &self.obstacles, distance, snaps),
            // A drag with no snapping keeps the pointer's own distance: no face, no plane and no grid.
            None => (distance, None),
        };
        // The rule answers the place the drag lands, not the path it crosses: a destination that is
        // free is taken as it is, even when the straight path from the start met a neighbour, and
        // only a destination that is not free is cut at the first contact (ADR 0166).
        let mut asks = [0.0; 3];
        asks[axis.index()] = snapped;
        let (distances, contacts) = self.resolve(asks, DVec3::ZERO);
        // A contact that cuts the snapped distance stops the piece short of the captured surface, so
        // the face landed on the contact and not on the snap: the stop is the answer, reported on its
        // own, and a capture would name a surface the piece never reached.
        let capture = if contacts[axis.index()].is_some() { None } else { capture };
        MoveLimit { distance_mm: quantised(distances[axis.index()]), contact: contacts[axis.index()], capture }
    }

    /// The distance a proposal may travel on every world axis at once.
    ///
    /// Each axis is measured on its own, so a face that stops one axis leaves the other two free: a
    /// piece dragged diagonally into a neighbour stops on the axis that meets it and keeps sliding on
    /// the axes beside it. The proposal is the whole carried vector from the start of the drag and the
    /// boxes the rule holds are the start boxes, so every slot is answered from one snapshot. A slot
    /// of an axis the drag does not offer is zero, exactly as [`MoveRule::limit`] answers it.
    pub fn limits(&self, proposed: DVec3) -> [MoveLimit; 3] {
        self.limits_from(DVec3::ZERO, proposed)
    }

    /// The same answer, resolved from the place the piece already stands at.
    ///
    /// A drag's proposal is measured from the start of the drag, but the piece is not there any more:
    /// it has been carried. A cut is measured from the hand's own previous place, so a piece resting
    /// on the far face of a body is not pulled back across that body to the near face when the hand
    /// drifts a millimetre into it — which is what made a block leap the whole width of a bar and
    /// back (ADR 0173). `from` is where the piece was drawn on the frame before.
    pub fn limits_from(&self, from: DVec3, proposed: DVec3) -> [MoveLimit; 3] {
        let proposed = [proposed.x, proposed.y, proposed.z];
        let mut limits = [MoveLimit { distance_mm: 0.0, contact: None, capture: None }; 3];
        let mut asks = [0.0_f64; 3];
        let mut captures: [Option<Capture>; 3] = [None; 3];
        for axis in MoveAxis::ALL {
            let index = axis.index();
            let ask = if self.allows(axis) { proposed[index] } else { 0.0 };
            let (distance, capture) = match self.snaps {
                Some(snaps) => snap_distance(axis, &self.movers, &self.obstacles, ask, snaps),
                None => (ask, None),
            };
            asks[index] = distance;
            captures[index] = capture;
        }
        // The whole destination is answered once, not axis by axis: a per-axis answer is exact for
        // each axis alone and can still combine into a place inside a turned neighbour, which is how
        // a diagonal carry used to cut a corner (ADR 0166).
        let from = if from.is_finite() { from } else { DVec3::ZERO };
        let (distances, contacts) = self.resolve(asks, from);
        for axis in MoveAxis::ALL {
            let index = axis.index();
            let capture = if contacts[index].is_some() { None } else { captures[index] };
            limits[index] = MoveLimit { distance_mm: quantised(distances[index]), contact: contacts[index], capture };
        }
        limits
    }

    /// The travel the document allows for a proposal, answered for the destination first.
    ///
    /// The whole proposal is one placement and is asked as one: if the piece may rest where all three
    /// axes together put it, all three are taken. Only a destination the document refuses is resolved
    /// axis by axis, each decided against the piece **standing where the axes before it put it** — an
    /// axis whose step is free is taken whole, and one whose step is not is cut at the first face in
    /// its way. That keeps a diagonal carry honest (no combination of the answers lands the piece
    /// inside a turned neighbour, ADR 0166) while a free axis still slides the piece along the face
    /// that stopped the axis before it (ADR 0171).
    ///
    /// The axis walk runs up to three passes, one per axis, so an axis cut on the first pass is asked
    /// again once the axes after it have moved the piece: a board whose X is stopped by a neighbour
    /// standing beside it is let through as soon as the carry's own Y clears that neighbour's face —
    /// the free place the hand is pointing at.
    ///
    /// The contact is the distance to the touch and is reported per axis, so the shell can say what
    /// stopped the drag. A pair that already overlaps is the maker's to repair, and an axis may only
    /// take the piece no deeper than it stands: the way out is always granted and going in is not, as
    /// the write's own guard reads it (X86, ADR 0162, ADR 0178).
    fn resolve(&self, asks: [f64; 3], from: DVec3) -> ([f64; 3], [Option<Stop>; 3]) {
        let whole = DVec3::new(asks[0], asks[1], asks[2]);
        // The destination first, as one placement: a slot the hand names is free even when every
        // single axis on the way to it is not, and refusing it axis by axis is how a board beside a
        // neighbour was never let past it.
        if whole.is_finite() && self.free_at(whole) {
            return (asks, [None; 3]);
        }
        // The piece stands where it stands, not where the drag started: every cut below is measured
        // from here, and an axis the hand does not move keeps its place.
        let mut distances = [from.x, from.y, from.z];
        let mut contacts: [Option<Stop>; 3] = [None; 3];
        let mut delta = from;
        for pass in 0..3 {
            let mut changed = false;
            for axis in MoveAxis::ALL {
                let index = axis.index();
                let ask = asks[index];
                if ask == 0.0 || !ask.is_finite() || distances[index] == ask {
                    continue;
                }
                let mut candidate = delta;
                candidate[index] = ask;
                if self.free_at(candidate) {
                    if delta.x != candidate.x || delta.y != candidate.y || delta.z != candidate.z {
                        changed = true;
                    }
                    delta = candidate;
                    distances[index] = ask;
                    contacts[index] = None;
                    continue;
                }
                let standing: Vec<Mover> = self
                    .movers
                    .iter()
                    .filter_map(|mover| {
                        let box_ = mover.box_.shifted(delta)?;
                        let solid = mover.solid.as_ref().and_then(|solid| solid.shifted(delta));
                        Some(Mover { box_, solid, faces: mover.faces, product: mover.product })
                    })
                    .collect();
                // The travel still to make from where the piece stands, and the face that stops it.
                let limit =
                    stop_at_contact(axis, ask - delta[index], &standing, &self.obstacles, self.contact.as_deref());
                let mut contact = limit.contact;
                let mut cut = delta[index] + limit.distance_mm;
                // The workspace's floor is a lower bound on the whole carry, so it cuts the vertical
                // axis exactly as a face does, and says so with a stop of its own kind so a log
                // names the floor and not a node (X87, ADR 0177).
                if axis == MoveAxis::Z
                    && let Some(bound) = self.floor
                    && cut < bound
                {
                    cut = bound;
                    contact = Some(Stop::Floor);
                }
                candidate[index] = cut;
                // The contact is the face the piece meets; should the cut land it in a corner the
                // axis could not see on its own, the axis keeps the piece where it stood and says
                // nothing about a face it never reached. An axis that does not move is honest; a
                // preview the release refuses is not.
                let contact = if !self.free_at(candidate) {
                    cut = delta[index];
                    candidate[index] = cut;
                    None
                } else {
                    contact
                };
                // A retry asks a cut axis again once the axes beside it have moved, and takes the
                // answer only when it is closer to the hand's own request: a pass may free an axis,
                // never lose a face the drag already rests on (X84).
                if pass > 0 && (cut - ask).abs() >= (delta[index] - ask).abs() {
                    continue;
                }
                if distances[index] != cut || contacts[index] != contact {
                    changed = true;
                }
                distances[index] = cut;
                contacts[index] = contact;
                delta = candidate;
            }
            if !changed {
                break;
            }
        }
        (distances, contacts)
    }

    /// Whether the piece may come to rest where `delta` puts it.
    ///
    /// The answers are about the destination, never about the straight path: a body carried past an
    /// obstacle lands beyond it as soon as the place it names is free, which is what lets a panel be
    /// set down behind a panel (ADR 0166). A legitimate nesting — a piece in a case, a unit inside
    /// another — is no overlap; a carcass and a body beside it are measured like two bodies; and the
    /// predicate is the same one the write's guard asks, so a drag never previews a place the release
    /// refuses.
    /// The first part the destination would come to rest inside, for the log.
    ///
    /// It is [`MoveRule::free_at`] with a name attached: the answer is the same one the drag uses, and
    /// the shell writes it when a drag is cut so a report says what stood in the way and how big it
    /// is, instead of leaving the reader to guess from the numbers (ADR 0170).
    pub fn blocked_at(&self, delta: DVec3) -> Option<Stop> {
        if !delta.is_finite() {
            return None;
        }
        // The floor is the lowest a body may go, so it answers before any part does: a place below it
        // is not free whatever stands there (X87, ADR 0177).
        if let Some(bound) = self.floor
            && delta.z < bound
        {
            return Some(Stop::Floor);
        }
        for mover in &self.movers {
            let Some(moved) = mover.box_.shifted(delta) else {
                // A body that cannot be carried has no free place to be: the answer names the body
                // itself, so a caller that reads the log still sees what refused the move.
                return Some(Stop::part(mover.box_));
            };
            // The body the port measures at the destination, and the one it measures where the piece
            // stands: the same two readings the furniture guard compares, so a drag never previews a
            // place the release refuses (ADR 0166, ADR 0179).
            let carried = mover.solid.as_ref().and_then(|solid| solid.shifted(delta));
            for obstacle in &self.obstacles {
                if nested(moved, obstacle.box_, mover.product) {
                    continue;
                }
                // A body lies inside its world box, so a pair whose boxes stand apart cannot touch:
                // the reading is answered without building either shape (X90).
                if !moved.overlaps_box(obstacle.box_) && !nested(moved, obstacle.box_, mover.product) {
                    continue;
                }
                // The place the piece stands in is the deepest one it may keep: a pair that already
                // overlaps and is no legitimate nesting is the maker's to repair, so every place
                // deeper than the start is refused and the piece has one way out and no way further
                // in (X86, ADR 0178). A nested piece is not that pair: it keeps its holder's walls
                // and is cut at one when it straddles it (ADR 0102).
                let landed = Measured::new(moved, carried.clone());
                let stood = Measured::new(mover.box_, mover.solid.clone());
                let standing = obstacle.measured(self.contact.as_deref());
                if !nested(mover.box_, obstacle.box_, mover.product)
                    && landed.sunk(&standing) <= stood.sunk(&standing) + CONTACT_MM
                {
                    continue;
                }
                let blocked = if mover.faces { landed.meets(&standing) } else { moved.overlaps_box(obstacle.box_) };
                if blocked {
                    return Some(Stop::part(obstacle.box_));
                }
            }
        }
        None
    }

    /// The body the hand is standing inside, when it is inside one.
    ///
    /// A carry crosses a body only once the hand itself has left it: a piece stopped at a face waits
    /// while the pointer is still over the body it is passing, and goes the frame the pointer comes
    /// out on the far side. That is the cursor crossing the thickness, which the owner named as the
    /// honest moment for a jump, and it reads the same in both directions instead of depending on how
    /// far the piece's own length has to travel (ADR 0172).
    ///
    /// A product's envelope does not gate: work happens inside a case, and a hand over a carcass is
    /// not a hand inside a body. Only a panel or an object does, and only one that is not a
    /// legitimate home of the carried piece.
    pub fn enclosing(&self, point: DVec3) -> Option<Id> {
        if !point.is_finite() {
            return None;
        }
        self.obstacles
            .iter()
            .filter(|obstacle| !obstacle.is_shell())
            .find(|obstacle| obstacle.contains(point))
            .map(|obstacle| obstacle.id)
    }

    /// The first body a straight carry from `from` to `to` passes through, when it passes one.
    ///
    /// A destination may be free while the road to it is not: the rule answers the place the piece
    /// lands (ADR 0166), so a piece whose whole body fits beyond a neighbour leaps there in one frame.
    /// With a quick hand that leap reads as passing through the neighbour, and there is no frame in
    /// which the piece is seen at the face it crossed. This query names that body, so the shell can
    /// rest the piece on the face for a moment before it takes the place (ADR 0172).
    ///
    /// The reading is the one the axis drag uses, on the moving box's own faces; a pair the document
    /// already holds overlapping is the maker's to repair and is not a crossing, and a piece sliding
    /// along a face crosses nothing.
    pub fn crossing(&self, from: DVec3, to: DVec3) -> Option<Id> {
        let direction = to - from;
        let reach = direction.length();
        if !direction.is_finite() || reach <= 0.0 {
            return None;
        }
        for mover in &self.movers {
            let start = mover.box_.shifted(from)?;
            for obstacle in &self.obstacles {
                if nested(start, obstacle.box_, mover.product) {
                    continue;
                }
                if mover.box_.depth(obstacle.box_) > CONTACT_MM {
                    // The pair already shares a volume, so the path is not the tool's to police: the
                    // piece leaves it and never goes deeper, which `blocked_at` measures (ADR 0178).
                    continue;
                }
                if let Some(touch) = blocking_along(direction, &start, obstacle)
                    && touch < reach
                {
                    return Some(obstacle.id);
                }
            }
        }
        None
    }

    fn free_at(&self, delta: DVec3) -> bool {
        // A pair the document already holds overlapping is the maker's to repair, and the repair has
        // one direction: the place the piece stands in is the deepest one it may keep, exactly as the
        // write's guard reads it (X86, ADR 0162, ADR 0178). A legitimate nesting is not that kind of
        // overlap — a piece in its case is nested, not stuck — so it is exempt at the destination.
        // [`MoveRule::blocked_at`] is that one reading, with the part named for a report.
        delta.is_finite() && self.blocked_at(delta).is_none()
    }
}

/// How far a ring drag may turn, and what stopped it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RotateLimit {
    /// The signed angle the drag may turn by about its world axis, in radians.
    ///
    /// The sign is the drag's own: a turn of `-0.4` is the same ring turned the other way. The angle
    /// is the largest one the document grants in that direction, so the preview drawn from it and the
    /// placement released from it are one number (X88).
    pub angle: f64,
    /// What the turn stopped at, when a body stopped it before the hand did.
    pub contact: Option<Stop>,
}

impl RotateLimit {
    /// The turn that moves nothing, which is the answer to a drag the document refuses from its first
    /// degree and to an axis that is not a direction.
    const STILL: Self = Self { angle: 0.0, contact: None };
}

/// What one turn of a ring drag is measured against, taken once when the drag starts.
///
/// A turn is not a carry: a body swinging about a hinge sweeps through poses no translation reaches,
/// and the world box around a turned board is not the board. So the rule asks the contact port for
/// the body **at the turned pose** and measures the pair with the same [`Measured`] values the move
/// rule and the write's guard use, which is what keeps a preview, a release and a refusal one answer
/// (X88, ADR 0162, ADR 0179).
///
/// The document does not change while the ring is held, so the moved boxes, the bodies they carry and
/// the surfaces that stay behind are read once. Nothing is stored in the document and nothing is
/// mutated.
#[derive(Clone, Debug)]
pub struct RotateRule {
    /// The world point every moved node turns about: the anchor's stored pivot (ADR 0026).
    pivot: DVec3,
    /// Every moved node's world box and its own body, where the drag started.
    movers: Vec<Mover>,
    /// Every surface the turn may come to rest inside, with the geometry each is measured by.
    obstacles: Vec<Obstacle>,
    /// The adapter the shapes are built by, kept for the pairs a frame actually reaches (X90).
    contact: Option<Arc<dyn Contact>>,
}

impl RotateRule {
    /// The world pivot the turn is measured about, in millimetres.
    pub const fn pivot(&self) -> DVec3 {
        self.pivot
    }

    /// The angle a turn about `axis` may make, and the body that stops it.
    ///
    /// The whole turn is asked first, so a place the document grants is taken as it is. Only a turn
    /// the document refuses is bisected: the angle is halved until the last granted turn is found, a
    /// couple of dozen asks that never touch the document, so a ring drag answers every frame without
    /// building a shape again (X88).
    pub fn limit(&self, axis: DVec3, angle: f64) -> RotateLimit {
        let Some(axis) = axis.try_normalize() else {
            return RotateLimit::STILL;
        };
        if !angle.is_finite() || angle == 0.0 {
            return RotateLimit::STILL;
        }
        // The turn as one placement, like the move rule's destination: a ring the document grants
        // whole is not cut up on the way to it.
        if self.blocked_at(axis, angle).is_none() {
            return RotateLimit { angle, contact: None };
        }
        // `low` is a turn the document grants and `high` one it refuses; the granted end is the
        // answer, so the release writes a placement the write's guard accepts.
        let mut low = 0.0;
        let mut high = angle;
        let mut contact = None;
        for _ in 0..MAX_TURN_STEPS {
            if (high - low).abs() <= TURN_RESOLUTION {
                break;
            }
            let middle = (low + high) * 0.5;
            match self.blocked_at(axis, middle) {
                None => low = middle,
                Some(stop) => {
                    high = middle;
                    contact = Some(stop);
                }
            }
        }
        RotateLimit { angle: low, contact }
    }

    /// The first part the turn would come to rest inside, for the log.
    ///
    /// It is [`RotateRule::limit`]'s predicate with a name attached, so the shell can say what stopped
    /// the ring instead of leaving the reader to guess from the angle (ADR 0170, X88).
    pub fn blocked_at(&self, axis: DVec3, angle: f64) -> Option<Stop> {
        let axis = axis.try_normalize()?;
        if !angle.is_finite() {
            return None;
        }
        let turn = DQuat::from_axis_angle(axis, angle);
        for mover in &self.movers {
            let Some(turned) = mover.box_.turned(turn, self.pivot) else {
                // A body that cannot be turned has no place to be: the answer names the body itself.
                return Some(Stop::part(mover.box_));
            };
            let carried = mover.solid.as_ref().and_then(|solid| solid.turned(turn, self.pivot));
            for obstacle in &self.obstacles {
                if nested(turned, obstacle.box_, mover.product) {
                    continue;
                }
                // A body lies inside its world box, so a pair whose boxes stand apart cannot touch:
                // the coarse box is the cull and never the answer (X90, X88).
                if !turned.overlaps_box(obstacle.box_) {
                    continue;
                }
                let stood = Measured::new(mover.box_, mover.solid.clone());
                let standing = obstacle.measured(self.contact.as_deref());
                let landed = Measured::new(turned, carried.clone());
                // The place the piece stands in is the deepest one it may keep, exactly as a carry
                // reads it: a pair the document already holds overlapping is the maker's to repair
                // and the turn has one way out and no way further in (X86, ADR 0178).
                if !nested(mover.box_, obstacle.box_, mover.product)
                    && landed.sunk(&standing) <= stood.sunk(&standing) + CONTACT_MM
                {
                    continue;
                }
                // The face and not the world box: a panel's own box turned is exact, and a body the
                // port describes is its own surface — the reading the write's guard asks, so a turn
                // the drag shows is a turn the release accepts (ADR 0162, ADR 0179).
                if landed.meets(&standing) {
                    return Some(Stop::part(obstacle.box_));
                }
            }
        }
        None
    }
}

/// Whether `node` is one of `targets` or sits below one of them.
fn covered_by(project: &Project, node: Id, targets: &[Id]) -> bool {
    let mut current = Some(node);
    let mut depth = 0;
    while let Some(id) = current {
        if targets.contains(&id) {
            return true;
        }
        if depth >= Project::MAX_ASSEMBLY_DEPTH {
            return false;
        }
        current = project.parent_of(id);
        depth = depth.saturating_add(1);
    }
    false
}

/// The proposed distance after snapping, and the surface a face landed on when one did.
///
/// A face of the moved box that is within [`SnapSettings::tolerance`] of a candidate surface wins:
/// a coordinate plane, or either face of an obstacle along the axis — a product's virtual face or a
/// part's own. Only a surface that faces the moving box across it counts, so a part in the far
/// corner of the scene cannot pull a drag. The nearest capture wins a tie by the order of the walk,
/// so the answer never depends on the direction the pointer approached from. A drag that snaps to
/// nothing rounds to the 1 mm grid and reports no capture.
fn snap_distance(
    axis: MoveAxis,
    movers: &[Mover],
    obstacles: &[Obstacle],
    proposed: f64,
    snaps: SnapSettings,
) -> (f64, Option<Capture>) {
    if proposed == 0.0 {
        return (0.0, None);
    }
    let tolerance = snaps.tolerance.millimetres_f64().max(0.0);
    let [first, second] = axis.perpendicular();
    let overlap = |a: Span, b: Span| a.min < b.max && b.min < a.max;
    let mut best: Option<(f64, f64, Capture)> = None;
    for mover in movers {
        let mover_first = mover.box_.span(first);
        let mover_second = mover.box_.span(second);
        let mut capture = |candidate: f64, surface: SnapSurface| {
            for (end, own) in [(SnapEnd::Low, mover.box_.span(axis).min), (SnapEnd::High, mover.box_.span(axis).max)] {
                let distance = candidate - own;
                let offset = (distance - proposed).abs();
                // A face behind the pointer would pull the drag backwards, which is not a snap but
                // a twitch: only a surface ahead of the proposal captures it.
                let reverses = distance * proposed < 0.0;
                if reverses || !distance.is_finite() || !offset.is_finite() || offset > tolerance {
                    continue;
                }
                if best.is_none_or(|(nearest, _, _)| offset < nearest) {
                    best = Some((offset, distance, Capture { end, at_mm: candidate, surface }));
                }
            }
        };
        // A body measured by its own faces is not captured by another part's world box: the box is
        // the coarse reading this mover no longer shares, and a face land on it would be a gap the
        // solid does not have (ADR 0165). The planes and the grid are the same for every body.
        if !mover.faces {
            for obstacle in obstacles {
                // A frame's virtual face captures only while the workspace snaps to frames. What it
                // does not do is stop being a stop: contact is the document's rule, not a taste
                // (ADR 0146).
                let target = if obstacle.is_shell() {
                    if !snaps.frames {
                        continue;
                    }
                    SnapSurface::Frame(obstacle.id)
                } else {
                    SnapSurface::Face(obstacle.id)
                };
                if !overlap(mover_first, obstacle.span(first)) || !overlap(mover_second, obstacle.span(second)) {
                    continue;
                }
                let span = obstacle.span(axis);
                capture(span.min, target);
                capture(span.max, target);
                // The middle of another product's reserved space is the one coordinate its faces do
                // not offer: a shelf lands at the mid-height of the cabinet, not on a wall. A body's
                // own middle is inside the body, where contact keeps a part out, so only a shell
                // offers it (U116).
                if obstacle.is_shell() && snaps.kinds.centre {
                    capture((span.min + span.max) * 0.5, SnapSurface::Centre(obstacle.id));
                }
            }
        }
        if snaps.kinds.axis {
            // A coordinate plane runs through the whole world, so it captures without a
            // perpendicular test — and the floor is the Z=0 plane like any other.
            capture(0.0, SnapSurface::Plane);
        }
    }
    best.map_or((round_to_grid(proposed), None), |(_, distance, capture)| (distance, Some(capture)))
}

/// Rounds a distance to the micrometre the document stores, so the preview and the placement that
/// records it are the same number.
fn quantised(value: f64) -> f64 {
    let rounded = (value * 1000.0).round() / 1000.0;
    if rounded.is_finite() { rounded } else { 0.0 }
}

/// Rounds a drag distance to the 1 mm grid.
fn round_to_grid(value: f64) -> f64 {
    let rounded = (value / GRID_MM).round() * GRID_MM;
    if rounded.is_finite() { rounded } else { 0.0 }
}

/// Whether two boxes can meet along `axis`, whatever their shapes are.
///
/// The travel runs along one axis, so the pair has to overlap on the other two before the bodies can
/// touch at all; a box apart there is a body apart there (X90).
fn swept_across(axis: MoveAxis, mover: &SurfaceBox, obstacle: &Obstacle) -> bool {
    let index = axis.index();
    (0..3).filter(|candidate| *candidate != index).all(|other| {
        let (low, high) = (mover.min[other].max(obstacle.min[other]), mover.max[other].min(obstacle.max[other]));
        high - low > CONTACT_MM
    })
}

/// The snapped distance cut at the first surface in its way.
fn stop_at_contact(
    axis: MoveAxis,
    distance: f64,
    movers: &[Mover],
    obstacles: &[Obstacle],
    contact: Option<&dyn Contact>,
) -> MoveLimit {
    let sign = if distance < 0.0 { -1.0 } else { 1.0 };
    if distance == 0.0 {
        return MoveLimit { distance_mm: 0.0, contact: None, capture: None };
    }
    let mut nearest: Option<(f64, Stop)> = None;
    for mover in movers {
        for obstacle in obstacles {
            // A body stops at a body, a shell at a shell, and a carcass at a solid body that is not
            // its own piece: only a legitimate nesting is exempt, and `blocking` knows that wall.
            // A body measured by its own faces is cut where its own surface first touches the other
            // body, and only a body the port cannot describe falls back to the box around it (X85).
            // A pair whose boxes stand apart on an axis across the travel cannot meet, whatever
            // their shapes are: the box around a body holds the body (X90).
            if !swept_across(axis, &mover.box_, obstacle) {
                continue;
            }
            let faces = match (mover.solid.as_ref(), obstacle.body(contact).as_deref()) {
                (Some(moving), Some(standing)) => moving.travel(standing, axis.direction() * sign),
                _ => blocking_faces(axis, sign, &mover.box_, obstacle),
            };
            let blocked = if mover.faces {
                faces
            } else {
                blocking(axis, sign, &mover.box_, obstacle, nested(mover.box_, obstacle.box_, mover.product))
            };
            let Some(limit) = blocked else {
                continue;
            };
            if nearest.is_none_or(|(best, _)| limit < best) {
                nearest = Some((limit, Stop::part(obstacle.box_)));
            }
        }
    }
    match nearest {
        Some((limit, contact)) if distance.abs() > limit => {
            MoveLimit { distance_mm: quantised(limit * sign), contact: Some(contact), capture: None }
        }
        _ => MoveLimit { distance_mm: distance, contact: None, capture: None },
    }
}

/// The lowest a drag may travel down the world Z, in millimetres, under the workspace's floor.
///
/// The bound is read from the moved bodies' own lowest point, so a body standing on the floor may not
/// sink at all and one standing a hand's width above it may descend that far and no further. A body
/// the document already holds below the floor is not pushed up by it: the bound forbids going deeper
/// and nothing else, so a piece left that way by a hand edit can still be lifted out (X87, ADR 0177).
fn floor_bound(movers: &[Mover]) -> Option<f64> {
    let lowest = movers.iter().map(|mover| mover.box_.min.z).fold(f64::INFINITY, f64::min);
    lowest.is_finite().then(|| (-lowest).min(0.0))
}

/// The first distance towards `sign` at which a solid body meets another, measured by their faces.
///
/// Both boxes are taken with their own frames, and the movement is a translation along one world
/// axis. The separating-axis test gives the answer exactly: for every axis the pair can be separated
/// along, the projections of the two boxes are intervals that move towards each other at the rate
/// the direction's own projection names, so the distance at which that axis stops separating is
/// `gap / rate`. The two solids first touch where the last separating axis closes, which is the
/// largest of those distances; an axis that never closes keeps them apart forever and answers
/// `None`, and a pair that already shares an axis is the maker's to repair, exactly as the write's
/// own guard reads it (ADR 0162, ADR 0165).
///
/// The distance is the exact touch, which is larger than the world-box reading of a turned body and
/// never smaller: a box contains its body, so the coarse reading stops the drag early and can never
/// let it through.
fn blocking_faces(axis: MoveAxis, sign: f64, moving: &SurfaceBox, obstacle: &SurfaceBox) -> Option<f64> {
    blocking_along(axis.direction() * sign, moving, obstacle)
}

/// The same face test for a translation in any direction, which is what a path needs.
///
/// `direction` is the whole travel of the moving box, so the answer is the distance at which the two
/// solids first touch along that path. Nothing else changes: the axes of the test, the contact
/// threshold and the reading of a pair that already shares an axis are the ones the axis drag uses.
fn blocking_along(direction: DVec3, moving: &SurfaceBox, obstacle: &SurfaceBox) -> Option<f64> {
    // The answer is millimetres, so the rate the gaps close at is read from a unit direction: a path
    // of any length is the same road, and the distances it reports are the ones a caller compares
    // with its own length.
    let length = direction.length();
    if !length.is_finite() || length <= 0.0 {
        return None;
    }
    let direction = direction / length;
    let mut reach: Option<f64> = None;
    for normal in separating_axes(*moving, *obstacle) {
        let (moving_low, moving_high) = extent(*moving, normal);
        let (obstacle_low, obstacle_high) = extent(*obstacle, normal);
        let rate = direction.dot(normal);
        if !rate.is_finite() {
            return None;
        }
        // The moving box is below the obstacle on this axis, or above it, and the gap between them
        // closes only while the direction carries it that way.
        let (gap, closing) = if moving_high <= obstacle_low {
            (obstacle_low - moving_high, rate > 0.0)
        } else if obstacle_high <= moving_low {
            (moving_low - obstacle_high, rate < 0.0)
        } else {
            continue;
        };
        if !gap.is_finite() {
            return None;
        }
        // A gap thinner than the document's own contact threshold is contact: a cross axis of two
        // turned boxes carries rounding noise of that size, and a face that already touches must not
        // read as a separation that never closes. A touch that the direction carries into the face is
        // the stop at zero; a touch the direction leaves is no stop at all (X77).
        if gap <= crate::project::surface::CONTACT_MM {
            if closing {
                reach = Some(reach.map_or(0.0, |best: f64| best.max(0.0)));
            }
            continue;
        }
        if !closing {
            return None;
        }
        let gap = gap / rate.abs();
        reach = Some(reach.map_or(gap, |best: f64| best.max(gap)));
    }
    reach
}

/// The distance towards `sign` before `moving` meets `obstacle`, or `None` when this pair does not
/// block the drag.
///
/// The two boxes must share a volume across the movement — touching faces are contact, not overlap.
/// Along it, a body may only stay clear of another body, while one shell may sit inside another:
/// a drag that keeps the containment stops at the wall it would cross, and a pair that already
/// overlaps one another partially is not the tool's to police — the piece keeps the place it stands
/// in and every way out of it (`blocked_at` measures the depth, ADR 0178).
fn blocking(axis: MoveAxis, sign: f64, moving: &SurfaceBox, obstacle: &SurfaceBox, nested: bool) -> Option<f64> {
    let [first, second] = axis.perpendicular();
    let overlap = |a: Span, b: Span| a.min < b.max && b.min < a.max;
    let moving_first = moving.span(first);
    let obstacle_first = obstacle.span(first);
    let moving_second = moving.span(second);
    let obstacle_second = obstacle.span(second);
    if !overlap(moving_first, obstacle_first) || !overlap(moving_second, obstacle_second) {
        return None;
    }
    let contains = |outer: Span, inner: Span| outer.min <= inner.min && inner.max <= outer.max;
    let shells = nested;
    let moving_span = moving.span(axis);
    let obstacle_span = obstacle.span(axis);
    let inside = obstacle_span.min <= moving_span.min && moving_span.max <= obstacle_span.max;
    let holds = moving_span.min <= obstacle_span.min && obstacle_span.max <= moving_span.max;
    let contained =
        shells && inside && contains(obstacle_first, moving_first) && contains(obstacle_second, moving_second);
    let containing =
        shells && holds && contains(moving_first, obstacle_first) && contains(moving_second, obstacle_second);
    // A sliver thinner than the document's own contact threshold is a touch after rounding, not a
    // pair already sunk into one another: measuring from a place the piece has just been stopped at
    // must not read as an overlap, or the retry pass would lose the face the drag rests on (X84).
    let distance = if sign >= 0.0 {
        if moving_span.max <= obstacle_span.min + crate::project::surface::CONTACT_MM {
            (obstacle_span.min - moving_span.max).max(0.0)
        } else if moving_span.min >= obstacle_span.max - crate::project::surface::CONTACT_MM {
            return None;
        } else if contained {
            obstacle_span.max - moving_span.max
        } else if containing {
            obstacle_span.min - moving_span.min
        } else {
            return None;
        }
    } else if moving_span.min >= obstacle_span.max - crate::project::surface::CONTACT_MM {
        (moving_span.min - obstacle_span.max).max(0.0)
    } else if moving_span.max <= obstacle_span.min + crate::project::surface::CONTACT_MM {
        return None;
    } else if contained {
        moving_span.min - obstacle_span.min
    } else if containing {
        moving_span.max - obstacle_span.max
    } else {
        return None;
    };
    distance.is_finite().then_some(distance)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use geom::{DQuat, DVec3};
    use units::Degrees;

    use crate::fixtures::*;
    use crate::*;

    /// A product of `width x depth x height` millimetres, standing at `(x, y, z)` in the root frame.
    fn product(project: &mut Project, code: &str, size: (i64, i64, i64), at: (i64, i64, i64)) -> Id {
        let root = project.root().id();
        let (width, depth, height) = size;
        let envelope =
            Envelope::new(millimetres(width), millimetres(depth), millimetres(height)).expect("a valid envelope");
        let placement = Placement::IDENTITY
            .with_translation(millimetres(at.0), millimetres(at.1), millimetres(at.2))
            .expect("a floor placement");
        project.add_product(root, "Piece", class(code), envelope, placement).expect("add product")
    }

    /// A `700 x 500 x 18` millimetre panel inside `parent`, with its base corner at `at`.
    fn panel(project: &mut Project, parent: Id, name: &str, at: (i64, i64, i64)) -> Id {
        let placement = Placement::IDENTITY
            .with_translation(millimetres(at.0), millimetres(at.1), millimetres(at.2))
            .expect("a panel placement");
        project.add_panel(parent, name, sample_nominal(), None, sample_thickness(), placement).expect("add panel")
    }

    /// The same panel, turned `degrees` about `axis` in its own frame, so its thickness points
    /// another way: a quarter turn about Y makes a divider, about X a back.
    fn turned_panel(project: &mut Project, parent: Id, name: &str, axis: char, degrees: i64) -> Id {
        let id = panel(project, parent, name, (0, 0, 0));
        let turn = Degrees::from_millidegrees(degrees * 1000);
        let angles = match axis {
            'x' => EulerAngles { x: turn, ..EulerAngles::default() },
            'y' => EulerAngles { y: turn, ..EulerAngles::default() },
            _ => EulerAngles { z: turn, ..EulerAngles::default() },
        };
        let placement = Placement::IDENTITY.with_rotation_euler(angles).expect("a quarter turn");
        project.set_node_placement(id, placement).expect("turn the panel");
        id
    }

    /// The sample scene with one empty product of `600 x 560 x 720` millimetres at the origin.
    fn product_project(code: &str) -> (Project, Id) {
        let mut project = sample_project();
        let id = product(&mut project, code, (600, 560, 720), (0, 0, 0));
        (project, id)
    }

    /// A project whose root holds nothing, so a snap test meets only what it adds.
    fn empty_project() -> Project {
        let root = Container::new(Id::new(1).expect("id"), "Root", Placement::IDENTITY, Vec::new());
        Project::new(root).expect("an empty project")
    }

    /// The answer of a drag that snapped to nothing and was stopped by nothing.
    fn moved(distance_mm: f64) -> MoveLimit {
        MoveLimit { distance_mm, contact: None, capture: None }
    }

    /// A limit as a test reads it: how far the drag goes and which part stopped it.
    ///
    /// The surface and the bounds a stop reports are how the rule measured that part, so a test names
    /// the part itself and leaves the reading to the rule (X87). A drag the floor stopped answers
    /// `(distance, None)`, and the test that is about the floor asks [`MoveRule::blocked_at`] for it.
    fn stops(limit: MoveLimit) -> (f64, Option<Id>) {
        (limit.distance_mm, limit.contact.and_then(Stop::node))
    }

    /// A workspace that snaps with the settings this test states, and keeps its floor.
    fn snapping(planes: bool, frames: bool) -> SnapSettings {
        let mut kinds = SnapKinds::ALL;
        kinds.axis = planes;
        SnapSettings { kinds, frames, ..SnapSettings::default() }
    }

    /// The workspace's settings for the tests here: the object move's snaps, and the floor it keeps.
    fn settings(snaps: SnapSettings) -> MoveSettings {
        MoveSettings { snaps: Some(snaps), ..MoveSettings::default() }
    }

    fn rule(project: &Project, moving: &[Id]) -> MoveRule {
        rule_with(project, moving, SnapSettings::default())
    }

    fn rule_with(project: &Project, moving: &[Id], snaps: SnapSettings) -> MoveRule {
        project.move_rule(moving, settings(snaps), None).expect("an object move")
    }

    fn limit(project: &Project, moving: &[Id], axis: MoveAxis, proposed: f64) -> MoveLimit {
        rule(project, moving).limit(axis, proposed)
    }

    /// A box object: a baked mesh of `size` millimetres, centred on its own frame.
    fn block_object(project: &mut Project, name: &str, placement: Placement, size: (f64, f64, f64)) -> Id {
        let half = DVec3::new(size.0 / 2.0, size.1 / 2.0, size.2 / 2.0);
        let mesh = crate::Mesh::cuboid(
            [-half.x as i64, -half.y as i64, -half.z as i64]
                .map(|value| Length::from_millimetres(value).unwrap_or_default()),
            [half.x as i64, half.y as i64, half.z as i64]
                .map(|value| Length::from_millimetres(value).unwrap_or_default()),
        )
        .expect("a box mesh");
        let root = project.root().id();
        project.add_object(root, name, placement, crate::Geometry::Mesh(mesh)).expect("an object")
    }

    /// A primitive body: a ball of `radius` millimetres, centred on its own frame.
    fn primitive_object(project: &mut Project, name: &str, placement: Placement, radius: i64) -> Id {
        let primitive =
            crate::Primitive::sphere(Length::from_millimetres(radius).expect("a radius"), 16, 8).expect("a ball");
        let root = project.root().id();
        project.add_object(root, name, placement, crate::Geometry::Primitive(primitive)).expect("an object")
    }

    #[test]
    fn a_drag_in_a_hall_of_three_hundred_bodies_is_built_at_once_and_measured_where_it_reaches() {
        // A workshop-wide scene: three hundred balls standing 500 mm apart. The drag's own frame has
        // to pay for the bodies it reaches, not for the hall: the shapes are built when a pair first
        // asks for one, so the build is a walk over the boxes and the asks touch a handful (X90).
        let mut project = empty_project();
        let mut bodies = Vec::new();
        for index in 0..300 {
            let x = f64::from(index % 20) * 500.0 - 5000.0;
            let y = f64::from(index / 20) * 500.0 - 3000.0;
            bodies.push(primitive_object(&mut project, "Ball", place((x, y, 0.0)), 150));
        }
        let hand = MoveSettings { snaps: None, ..MoveSettings::default() };
        let started = std::time::Instant::now();
        let rule = project.move_rule(&bodies[..1], hand, Some(&contact_parry::Parry)).expect("a rule");
        let build = started.elapsed();
        let started = std::time::Instant::now();
        for step in 0..120 {
            let _ = rule.blocked_at(DVec3::new(10.0, 0.0, f64::from(step)));
        }
        let asks = started.elapsed();
        // The bounds are two frames of a slow machine, generous enough not to be flaky and tight
        // enough to catch a rule that prepares the hall: the fail an eager build gives is 40 ms.
        assert!(build < std::time::Duration::from_millis(16), "the build costs one frame at most: {build:?}");
        assert!(asks < std::time::Duration::from_millis(16), "and 120 asks fit in a frame: {asks:?}");
    }

    #[test]
    fn a_ball_is_measured_by_its_own_surface_and_not_by_the_box_around_it() {
        // A 100 mm ball dragged beside a block that stands 30 mm off its line. The box around the
        // ball reaches y = 50 and meets the block at x = 200; the ball's own surface passes the
        // block's near corner at (250, 30), which is 50 mm from the centre at x = 210. The ten
        // millimetres between the two answers are the corner the box invented (X85, ADR 0179).
        let mut project = empty_project();
        let ball = primitive_object(&mut project, "Ball", place((0.0, 0.0, 0.0)), 50);
        let other = block_object(&mut project, "Block", place((300.0, 80.0, 0.0)), (100.0, 100.0, 100.0));

        // The object tool's own reading without snapping: measured, and every axis free.
        let measured_free = MoveSettings { snaps: None, ..MoveSettings::default() };
        let boxed = project.move_rule(&[ball], measured_free, None).expect("a box rule");
        let measured = project.move_rule(&[ball], measured_free, Some(&contact_parry::Parry)).expect("a measured rule");
        let (by_box, _) = stops(boxed.limit(MoveAxis::X, 300.0));
        let (by_surface, contact) = stops(measured.limit(MoveAxis::X, 300.0));
        assert!((by_box - 200.0).abs() < 0.05, "the box around the ball meets the block at 200: {by_box}");
        // A ball is the hull of the tessellation its parameters name, so its surface is a facet
        // inside the ideal sphere and it meets the corner a millimetre later than the ideal 210.
        assert!((by_surface - 211.0).abs() < 1.0, "the ball's own surface meets it at 211: {by_surface}");
        assert_eq!(contact, Some(other), "and the block is what it meets");
        assert!(by_surface > by_box, "the box is a smaller answer than the body it holds");
    }

    /// A cone body: base radius 40 mm, apex up, 100 mm tall, standing on its own origin.
    fn cone_object(project: &mut Project, name: &str, placement: Placement) -> Id {
        let cone = crate::Primitive::cone(
            Length::from_millimetres(40).expect("a radius"),
            Length::ZERO,
            Length::from_millimetres(100).expect("a height"),
            16,
        )
        .expect("a cone");
        let root = project.root().id();
        project.add_object(root, name, placement, crate::Geometry::Primitive(cone)).expect("an object")
    }

    #[test]
    fn a_turned_cone_reaches_a_corner_as_far_as_its_own_surface_allows() {
        // The owner's report: a turned cone is a box for contact, so its invisible cuboid meets the
        // corner between two plates long before the apex does. With the port the cone's own surface
        // is what stops it, and the stop names the plate it met (X85, ADR 0179).
        let mut project = empty_project();
        let quarter = std::f64::consts::FRAC_PI_4;
        let turned = Placement::new(DVec3::ZERO, geom::DQuat::from_rotation_z(quarter)).expect("a turn");
        let cone = cone_object(&mut project, "Cone", turned);
        // Two plates standing in the corner the cone is carried into.
        block_object(&mut project, "Wall", place((400.0, -100.0, -100.0)), (20.0, 400.0, 400.0));
        let floor = block_object(&mut project, "Floor", place((100.0, 200.0, -100.0)), (600.0, 20.0, 400.0));

        let measured_free = MoveSettings { snaps: None, ..MoveSettings::default() };
        let diagonal = DVec3::new(300.0, 200.0, 0.0);
        let boxed = project.move_rule(&[cone], measured_free, None).expect("a box rule");
        let measured = project.move_rule(&[cone], measured_free, Some(&contact_parry::Parry)).expect("a measured rule");
        let by_box = boxed.limits(diagonal);
        let by_surface = measured.limits(diagonal);
        let travel = |limits: [MoveLimit; 3]| (limits[0].distance_mm.powi(2) + limits[1].distance_mm.powi(2)).sqrt();
        assert!(
            travel(by_surface) > travel(by_box),
            "the cone's own surface reaches further than its box: {} vs {}",
            travel(by_surface),
            travel(by_box)
        );
        let stopped = by_surface.iter().find_map(|limit| limit.contact).map(|stop| stop.node());
        assert!(stopped.is_some(), "the corner stops the carry and the log names what stopped it");
        let _ = floor;
    }

    #[test]
    fn a_barrel_rests_its_own_side_on_a_face() {
        // A cone with two equal circles is a barrel: its side is what rests on the plate, and the
        // write grants the place the drag stops at while the box around it would sink it (X85).
        let mut project = empty_project();
        // The cone lies on its side, its axis along world X, so its barrel is what comes down.
        let lying =
            Placement::new(DVec3::ZERO, geom::DQuat::from_rotation_y(-std::f64::consts::FRAC_PI_2)).expect("a turn");
        // It starts well above the plate, so the pair is no overlap to begin with and the guard's
        // answer is about the place asked for and not about a repair (X86).
        let above = |z: f64| Placement::new(DVec3::new(0.0, 0.0, z), lying.rotation()).expect("a place");
        let barrel = cone_object(&mut project, "Barrel", above(200.0));
        block_object(&mut project, "Plate", place((0.0, 0.0, 0.0)), (600.0, 600.0, 20.0));
        // Its side reaches 40 mm below its own axis, so an axis 50 mm above the origin rests the
        // barrel on the plate's top face at z = 10: the guard grants the touch and refuses deeper.
        let resting = above(50.0);
        assert_eq!(
            project.resting_conflict(&[(barrel, resting)], Some(&contact_parry::Parry)),
            None,
            "the barrel's side rests on the plate's face"
        );
        let sunk = above(20.0);
        assert!(
            project.resting_conflict(&[(barrel, sunk)], Some(&contact_parry::Parry)).is_some(),
            "and half its base below the face is an overlap"
        );
    }

    #[test]
    fn the_free_move_is_the_makers_hand_and_measures_nothing() {
        // Control belongs to the object tool (ADR 0180): the free move carries a body by the pointer's
        // own distance, through a neighbour and below the workspace's floor, and reports no contact.
        let mut project = empty_project();
        let ball = primitive_object(&mut project, "Ball", place((0.0, 0.0, 0.0)), 50);
        block_object(&mut project, "Block", place((300.0, 0.0, 0.0)), (100.0, 100.0, 100.0));
        let hand = MoveSettings::default().free();
        let carried = project.move_rule(&[ball], hand, Some(&contact_parry::Parry)).expect("a free rule");
        let into = carried.limit(MoveAxis::X, 300.0);
        assert_eq!(into.distance_mm, 300.0, "the hand's own distance reaches the block's middle");
        assert_eq!(into.contact, None, "and nothing is named as standing in the way");
        let down = carried.limit(MoveAxis::Z, -500.0);
        assert_eq!(down.distance_mm, -500.0, "the workspace's floor does not stop the hand either");
        assert_eq!(down.contact, None);
        // The same distances asked of the object tool are cut and stopped (ADR 0162, ADR 0177).
        let measured = MoveSettings { snaps: None, ..MoveSettings::default() };
        let rule = project.move_rule(&[ball], measured, Some(&contact_parry::Parry)).expect("a measured rule");
        assert!(
            rule.limit(MoveAxis::X, 300.0).distance_mm < 300.0,
            "the object move stops at the block: {:?}",
            rule.limit(MoveAxis::X, 300.0)
        );
        assert_eq!(rule.limit(MoveAxis::Z, -500.0).distance_mm, 0.0, "and on the floor");
    }

    #[test]
    fn the_write_reads_the_same_surface_the_drag_reads() {
        // The guard is the drag's own question asked before a placement lands. With the port it
        // grants the place the ball's own surface leaves free, which the box around it refuses
        // (ADR 0166, ADR 0179).
        let mut project = empty_project();
        let ball = primitive_object(&mut project, "Ball", place((0.0, 0.0, 0.0)), 50);
        block_object(&mut project, "Block", place((300.0, 80.0, 0.0)), (100.0, 100.0, 100.0));
        let measured_free = MoveSettings { snaps: None, ..MoveSettings::default() };
        let measured = project.move_rule(&[ball], measured_free, Some(&contact_parry::Parry)).expect("a measured rule");
        let landing = measured.limit(MoveAxis::X, 300.0).distance_mm;
        let placed = place((landing, 0.0, 0.0));

        assert!(
            project.resting_conflict(&[(ball, placed)], None).is_some(),
            "the box around the ball puts it inside the block"
        );
        assert_eq!(
            project.resting_conflict(&[(ball, placed)], Some(&contact_parry::Parry)),
            None,
            "and its own surface leaves it free, which is the place the drag showed"
        );
    }

    #[test]
    fn a_panel_offers_only_its_own_thickness_axis() {
        // N22: a board slides along its own normal and never across it, so a shelf rises in its
        // thickness direction and the two in-plane handles are not offered at all.
        let (mut project, piece) = product_project("10.01.01");
        let shelf = panel(&mut project, piece, "Shelf", (0, 0, 0));
        assert_eq!(project.move_axes(&[shelf]), [false, false, true], "a shelf's thickness is its upright axis");
        let rule = rule(&project, &[shelf]);
        assert!(rule.allows(MoveAxis::Z), "the shelf rises and falls");
        assert!(!rule.allows(MoveAxis::X) && !rule.allows(MoveAxis::Y), "and does not slide in its own plane");
        assert_eq!(rule.limit(MoveAxis::X, 100.0), moved(0.0), "a drag across the board moves nothing");
        assert_eq!(rule.limit(MoveAxis::Z, 40.4), moved(40.0), "along its thickness it keeps the move grid");
    }

    #[test]
    fn a_divider_crosses_the_case_and_a_back_moves_in_and_out() {
        // The same board, turned: its thickness points along X for a divider and along Y for a back,
        // and the axis it offers follows the board rather than the world (N22).
        let (mut project, piece) = product_project("10.01.01");
        let divider = turned_panel(&mut project, piece, "Divider", 'y', 90);
        assert_eq!(project.move_axes(&[divider]), [true, false, false], "a divider crosses the case");
        assert_eq!(
            rule(&project, &[divider]).limit(MoveAxis::X, 120.4),
            moved(120.0),
            "and its drag runs along the width of the case"
        );

        let back = turned_panel(&mut project, piece, "Back", 'x', 90);
        assert_eq!(project.move_axes(&[back]), [false, true, false], "a back moves in and out");
    }

    #[test]
    fn a_quarter_turned_product_of_panels_is_measured_in_the_world_frame() {
        // The board's own frame is what the rule reads, so a turned cabinet turns its shelf's axis
        // with it: the shelf that rose in world Z now slides in world X (ADR 0024, N22).
        let mut project = empty_project();
        let piece = product(&mut project, "10.01.02", (600, 560, 720), (0, 0, 0));
        let shelf = panel(&mut project, piece, "Shelf", (0, 0, 0));
        let turn = Degrees::from_millidegrees(90_000);
        let quarters = Placement::IDENTITY
            .with_rotation_euler(EulerAngles { y: turn, ..EulerAngles::default() })
            .expect("a quarter turn");
        project.set_node_placement(piece, quarters).expect("turn the case");
        assert_eq!(project.move_axes(&[shelf]), [true, false, false]);
    }

    #[test]
    fn a_selection_offers_only_the_axes_every_part_of_it_allows() {
        // Two boards that cannot slide rigidly anywhere offer no handle at all, which is honest: the
        // mode draws nothing rather than an armed handle that refuses the press (ADR 0113).
        let (mut project, piece) = product_project("10.01.01");
        let shelf = panel(&mut project, piece, "Shelf", (0, 0, 0));
        let back = turned_panel(&mut project, piece, "Back", 'x', 90);
        assert_eq!(project.move_axes(&[shelf, back]), [false, false, false]);

        // A frame beside a board offers its own axes, and the intersection keeps what they share: a
        // wall-hung frame offers all three, so this selection slides the board along Z alone.
        let neighbour = product(&mut project, "10.01.02", (600, 560, 720), (1000, 0, 0));
        assert_eq!(project.move_axes(&[shelf, neighbour]), [false, false, true]);
    }

    #[test]
    fn the_panel_axis_holds_outside_a_product_and_the_move_snaps_anywhere() {
        // A board at the root is not furniture, but it is still a board: it slides along its
        // thickness only (N22), while its drag is the object move, so it lands on the workspace's
        // grid like every other body (ADR 0165).
        let project = sample_project();
        let loose = Id::new(2).expect("id");
        assert_eq!(project.move_axes(&[loose]), [false, false, true], "the sample shelf is a board");
        let loose_rule = rule_with(&project, &[loose], SnapSettings::default());
        assert!(!loose_rule.allows(MoveAxis::X), "and the tool keeps it in its own plane");
        assert_eq!(loose_rule.limit(MoveAxis::Z, 40.4), moved(40.0), "a drag lands on the 1 mm grid");
        assert_eq!(project.move_axes(&[]), [true, true, true], "nothing selected offers every axis");
    }

    /// A panel placement `(x, y, z)` millimetres inside the root frame.
    fn place(at: (f64, f64, f64)) -> Placement {
        Placement::IDENTITY
            .with_translation(
                Length::from_micrometres((at.0 * 1000.0).round() as i64),
                Length::from_micrometres((at.1 * 1000.0).round() as i64),
                Length::from_micrometres((at.2 * 1000.0).round() as i64),
            )
            .expect("a placement")
    }

    /// A solid box of `size` millimetres at `at`, with its own base corner at the node's origin.
    fn solid_box(project: &mut Project, parent: Id, name: &str, size: (i64, i64, i64), at: Placement) -> Id {
        let (x, y, z) = size;
        let corner = |dx: i64, dy: i64, dz: i64| [millimetres(dx), millimetres(dy), millimetres(dz)];
        let vertices = vec![
            corner(0, 0, 0),
            corner(x, 0, 0),
            corner(x, y, 0),
            corner(0, y, 0),
            corner(0, 0, z),
            corner(x, 0, z),
            corner(x, y, z),
            corner(0, y, z),
        ];
        let faces = vec![
            vec![0, 3, 2, 1],
            vec![4, 5, 6, 7],
            vec![0, 1, 5, 4],
            vec![1, 2, 6, 5],
            vec![2, 3, 7, 6],
            vec![3, 0, 4, 7],
        ];
        let mesh = Mesh::new(vertices, faces).expect("a box mesh");
        project.add_object(parent, name, at, Geometry::Mesh(mesh)).expect("a solid body")
    }

    /// A placement turned `millidegrees` about Z, its origin at `(x, y, z)` millimetres.
    fn turned_z(millidegrees: i64, at: (i64, i64, i64)) -> Placement {
        let euler = EulerAngles { z: Degrees::from_millidegrees(millidegrees), ..EulerAngles::default() };
        Placement::IDENTITY
            .with_rotation_euler(euler)
            .and_then(|placement| placement.with_translation(millimetres(at.0), millimetres(at.1), millimetres(at.2)))
            .expect("a turned placement")
    }

    #[test]
    fn a_turned_solid_stops_at_its_own_face_and_not_at_the_box_around_it() {
        // ADR 0165: a 200 x 20 x 10 board turned 45 degrees has a world box that reaches
        // x = 141.42, but at the height of the neighbour (y = 0..100) its own far corner only reaches
        // x = 100. The box around the board therefore stops the drag at 500 - 141.42 = 358.58 mm,
        // while the board's own face meets the neighbour after 400 mm: the tip at 141.42 is already
        // above the neighbour's y = 100 face and never touches it.
        let mut project = empty_project();
        let root = project.root().id();
        let board = solid_box(&mut project, root, "Board", (200, 20, 10), turned_z(45_000, (0, 0, 0)));
        let neighbour = solid_box(&mut project, root, "Neighbour", (100, 100, 100), place((500.0, 0.0, 0.0)));
        // A destination inside the neighbour is cut at the true touch.
        assert_eq!(
            stops(limit(&project, &[board], MoveAxis::X, 500.0)),
            (400.0, Some(neighbour)),
            "the board's corner (500, 100) is the touch, not the box's corner (500, 141.42)"
        );
    }

    #[test]
    fn a_carried_body_lands_in_the_free_place_beyond_a_neighbour() {
        // X77: the rule answers the place the piece lands, not the path it crosses. A destination
        // beside and beyond the neighbour is free, so the piece goes there; a destination that
        // straddles the neighbour is cut at its face.
        let mut project = empty_project();
        let root = project.root().id();
        let body = solid_box(&mut project, root, "Body", (100, 100, 100), place((0.0, 0.0, 0.0)));
        let neighbour = solid_box(&mut project, root, "Neighbour", (100, 100, 100), place((300.0, 0.0, 0.0)));
        assert_eq!(
            stops(limit(&project, &[body], MoveAxis::X, 250.0)),
            (200.0, Some(neighbour)),
            "a destination inside the neighbour is the contact"
        );
        assert_eq!(
            limit(&project, &[body], MoveAxis::X, 1000.0),
            moved(1000.0),
            "a free destination beyond it is taken whole: the piece jumps past"
        );
        // And a furniture piece obeys the same rule: a board behind a board is reachable.
        let (mut project, piece) = product_project("10.01.01");
        let shelf = panel(&mut project, piece, "Shelf", (0, 0, 0));
        let above = panel(&mut project, piece, "Above", (0, 0, 1000));
        assert_eq!(
            limit(&project, &[shelf], MoveAxis::Z, 1200.0),
            moved(1200.0),
            "the board rises past its neighbour and lands in the free place beyond"
        );
        assert!(above == above);
    }

    #[test]
    fn a_board_beside_a_standing_one_passes_it_where_the_hand_names_free_space() {
        // X82: the log of the third session had the hand at (-171, 236) and the answer at (-94, 236),
        // held for a second and a half while the pointer kept travelling. The destination was free —
        // the board had cleared the standing one in Y — but every single axis on its own was not, and
        // the axis walk never asked the whole vector. The place the hand names is asked first now.
        let mut project = empty_project();
        let root = project.root().id();
        // The boxes of the log: the board is 620 x 90 x 100 at (-210,-110,0), the standing body
        // 60 x 400 x 100 at (-364,-309,0), and they overlap on 26 mm of Y and all of Z.
        let board = solid_box(&mut project, root, "Board", (620, 90, 100), place((-210.0, -110.0, 0.0)));
        let standing = solid_box(&mut project, root, "Standing", (60, 400, 100), place((-364.0, -309.0, 0.0)));
        let rule = rule(&project, &[board]);
        let asked = DVec3::new(-171.0, 236.0, 0.0);
        let limits = rule.limits(asked);
        let travelled = DVec3::new(
            limits[MoveAxis::X.index()].distance_mm,
            limits[MoveAxis::Y.index()].distance_mm,
            limits[MoveAxis::Z.index()].distance_mm,
        );
        assert_eq!(travelled, asked, "the slot in front of the standing board is taken whole");
        assert!(limits.iter().all(|limit| limit.contact.is_none()));
        // Behind it is the same free slot, and the other way round the standing board.
        let far = rule.limits(DVec3::new(-171.0, -347.0, 0.0));
        assert_eq!(
            DVec3::new(far[0].distance_mm, far[1].distance_mm, far[2].distance_mm),
            DVec3::new(-171.0, -347.0, 0.0),
            "the space behind the standing board is the same jump"
        );
        // Without the clearing Y there is no slot: the board stops at the standing one's face.
        let straight = rule.limits(DVec3::new(-171.0, 0.0, 0.0));
        assert_eq!(
            stops(straight[MoveAxis::X.index()]),
            (-94.0, Some(standing)),
            "in line with it the board meets its face and goes no further"
        );
    }

    #[test]
    fn a_path_that_passes_a_body_is_named_and_a_slide_along_it_is_not() {
        // X83: a destination may be free while the road to it is not, and with a quick hand no frame
        // shows the piece at the face it crossed. The rule names that body, so the carry can rest the
        // piece on the face first; a path that stops short of it, or slides along a face, crosses
        // nothing and is ordinary travel.
        let mut project = empty_project();
        let root = project.root().id();
        let body = solid_box(&mut project, root, "Body", (100, 100, 100), place((0.0, 0.0, 0.0)));
        let neighbour = solid_box(&mut project, root, "Neighbour", (100, 100, 100), place((300.0, 0.0, 0.0)));
        let beyond = rule(&project, &[body]);
        assert_eq!(
            beyond.crossing(DVec3::ZERO, DVec3::new(500.0, 0.0, 0.0)),
            Some(neighbour),
            "a place beyond the neighbour crosses it"
        );
        assert_eq!(
            beyond.crossing(DVec3::ZERO, DVec3::new(150.0, 0.0, 0.0)),
            None,
            "a place short of the neighbour crosses nothing"
        );
        assert_eq!(
            beyond.crossing(DVec3::new(0.0, -400.0, 0.0), DVec3::new(0.0, 400.0, 0.0)),
            None,
            "a slide beside the neighbour crosses nothing"
        );
        // Beside a face and touching it: a slide in Y is along the face, not through it.
        let mut project = empty_project();
        let root = project.root().id();
        let body = solid_box(&mut project, root, "Body", (100, 100, 100), place((0.0, 0.0, 0.0)));
        let neighbour = solid_box(&mut project, root, "Neighbour", (100, 100, 100), place((250.0, 0.0, 0.0)));
        let touching = rule(&project, &[body]);
        assert_eq!(touching.crossing(DVec3::ZERO, DVec3::new(0.0, 300.0, 0.0)), None, "a face is not a road");
        assert_eq!(
            touching.crossing(DVec3::ZERO, DVec3::new(400.0, 0.0, 0.0)),
            Some(neighbour),
            "and the same neighbour is crossed in X"
        );
        // The hand over a body is named too: that is the cursor gate the carry waits on.
        assert_eq!(touching.enclosing(DVec3::new(300.0, 0.0, 0.0)), Some(neighbour), "the hand is over it");
        assert_eq!(touching.enclosing(DVec3::new(0.0, -400.0, 0.0)), None, "and here it is not");
    }

    #[test]
    fn a_piece_resting_past_a_body_stays_there_and_is_not_pulled_back_across_it() {
        // X84: the case from the log. The piece is 60 wide and 950 deep, the bar 1270 long and 70
        // deep; the piece's two homes are the bar's near face (delta 200) and its far face (delta
        // 1530). The pointer drifted one millimetre back into the bar and the answer was measured
        // from the start of the drag, so the piece was pulled across the whole bar to the near face
        // and flew 1330 mm to the left. A cut is measured from where the piece stands.
        let mut project = empty_project();
        let root = project.root().id();
        let piece = solid_box(&mut project, root, "Piece", (60, 950, 100), place((-630.0, -490.0, 0.0)));
        let bar = solid_box(&mut project, root, "Bar", (1270, 70, 100), place((-370.0, -510.0, 0.0)));
        let rule = rule(&project, &[piece]);
        let straight = rule.limits(DVec3::new(1529.0, 0.0, 0.0));
        assert_eq!(
            stops(straight[MoveAxis::X.index()]),
            (200.0, Some(bar)),
            "a drag that starts on the near side stops at the near face"
        );
        let standing = DVec3::new(1530.0, 0.0, 0.0);
        let drift = rule.limits_from(standing, DVec3::new(1529.0, 0.0, 0.0));
        assert_eq!(
            stops(drift[MoveAxis::X.index()]),
            (1530.0, Some(bar)),
            "a piece resting on the far face stays at it when the hand drifts into the bar"
        );
        let onward = rule.limits_from(standing, DVec3::new(1600.0, 0.0, 0.0));
        assert_eq!(onward[MoveAxis::X.index()].distance_mm, 1600.0, "and follows the hand onward");
        let back = rule.limits_from(standing, DVec3::new(100.0, 0.0, 0.0));
        assert_eq!(back[MoveAxis::X.index()].distance_mm, 100.0, "the near side is still reachable");
    }

    #[test]
    fn a_diagonal_carry_never_lands_inside_a_turned_neighbour() {
        // X77: each axis is exact for itself and the three answers can still combine into a place
        // inside a turned neighbour, which is how a diagonal carry used to cut a corner. The
        // destination is now answered once for the whole vector, so the preview is a placement the
        // write's guard accepts.
        let mut project = empty_project();
        let root = project.root().id();
        let body = solid_box(&mut project, root, "Body", (100, 100, 100), place((0.0, 0.0, 0.0)));
        solid_box(&mut project, root, "Board", (400, 20, 200), turned_z(45_000, (300, -100, 0)));
        let proposed = DVec3::new(380.0, -10.0, 0.0);
        let direct = place((proposed.x, proposed.y, proposed.z));
        assert!(
            project.resting_conflict(&[(body, direct)], None).is_some(),
            "the hand's own diagonal would land the body inside the turned board"
        );
        let body_rule = rule(&project, &[body]);
        let limits = body_rule.limits(proposed);
        let delta = DVec3::new(
            limits[MoveAxis::X.index()].distance_mm,
            limits[MoveAxis::Y.index()].distance_mm,
            limits[MoveAxis::Z.index()].distance_mm,
        );
        let landing = place((delta.x, delta.y, delta.z));
        assert_eq!(
            project.resting_conflict(&[(body, landing)], None),
            None,
            "the preview the rule answers is a placement the write accepts"
        );
        assert!(limits.iter().any(|limit| limit.contact.is_some()), "and the corner is what stopped it");
    }

    #[test]
    fn a_carcass_and_a_body_stop_each_other_and_a_piece_in_the_case_does_not() {
        // X77: a shell and a body were skipped as a pair, so a cabinet's envelope passed through a
        // solid. Only a legitimate nesting is exempt: a shelf in its own case moves inside it.
        let (mut project, piece) = product_project("10.01.01");
        let shelf = panel(&mut project, piece, "Shelf", (0, 0, 0));
        let frame = product(&mut project, "10.01.01", (600, 560, 720), (0, 0, 1000));
        assert_eq!(
            stops(limit(&project, &[shelf], MoveAxis::Z, 1100.0)),
            (982.0, Some(frame)),
            "the shelf is not part of that frame, so its envelope stops the shelf"
        );
        // A shelf inside its own case is a body in a shell: the destination inside the case is free.
        let (mut project, piece) = product_project("10.01.01");
        let shelf = panel(&mut project, piece, "Shelf", (0, 0, 100));
        assert_eq!(limit(&project, &[shelf], MoveAxis::Z, 500.0), moved(500.0), "a piece moves inside its case");
        // A solid body beside a carcass stops at its envelope.
        let mut project = empty_project();
        let root = project.root().id();
        let cabinet = product(&mut project, "10.01.01", (600, 560, 720), (0, 0, 0));
        let body = solid_box(&mut project, root, "Body", (100, 100, 100), place((800.0, 0.0, 0.0)));
        assert_eq!(
            stops(limit(&project, &[body], MoveAxis::X, -300.0)),
            (-200.0, Some(cabinet)),
            "the body stops at the carcass's envelope and is not pushed into it"
        );
        assert_eq!(
            limit(&project, &[body], MoveAxis::X, -1000.0),
            moved(-1000.0),
            "a free place beyond the carcass is the hand's to take"
        );
    }

    #[test]
    fn a_solid_sticks_its_end_to_the_slab_above_it() {
        // X77: a hand that asks for five millimetres into the slab is answered with the face, so the
        // post's top end stays on the slab's underside instead of being carried through it.
        let mut project = empty_project();
        let root = project.root().id();
        // The post hangs 100 mm above the floor, so the floor plane does not capture this drag.
        let post = solid_box(&mut project, root, "Post", (18, 500, 300), place((0.0, 0.0, 100.0)));
        let slab = solid_box(&mut project, root, "Slab", (600, 500, 20), place((0.0, 0.0, 400.0)));
        assert_eq!(
            stops(limit(&project, &[post], MoveAxis::Z, 5.0)),
            (0.0, Some(slab)),
            "the top of the post meets the underside of the slab and stays there"
        );
        // A place beyond the slab is a place of its own: the far side is reachable on purpose.
        assert_eq!(limit(&project, &[post], MoveAxis::Z, 500.0), moved(500.0));
    }

    #[test]
    fn a_solid_body_slides_along_a_face_and_keeps_the_other_axes_free() {
        let mut project = empty_project();
        let root = project.root().id();
        let body = solid_box(&mut project, root, "Body", (100, 100, 100), place((0.0, 0.0, 0.0)));
        let neighbour = solid_box(&mut project, root, "Neighbour", (100, 100, 100), place((300.0, 0.0, 0.0)));
        let body_rule = rule(&project, &[body]);
        let limits = body_rule.limits(DVec3::new(250.0, 60.0, 30.0));
        assert_eq!(stops(limits[MoveAxis::X.index()]), (200.0, Some(neighbour)), "the neighbour's face stops X");
        assert_eq!(limits[MoveAxis::Y.index()].distance_mm, 60.0, "and the hand's Y travel slides along it");
        assert_eq!(limits[MoveAxis::Z.index()].distance_mm, 30.0, "while the vertical is free for a solid");
    }

    #[test]
    fn a_write_that_sinks_a_board_into_its_neighbour_is_refused_and_a_touch_is_not() {
        // X72 in the model: a board asked to come to rest inside another board is refused, and the
        // refusal names both parts. The sample shelf is 700 x 500 x 18 at the origin, so a second
        // board beside it stands clear at x = 720 and touches it at x = 700.
        let mut project = empty_project();
        let root = project.root().id();
        let a = panel(&mut project, root, "Left", (0, 0, 0));
        let b = panel(&mut project, root, "Right", (720, 0, 0));
        assert_eq!(project.resting_conflict(&[(b, place((720.0, 0.0, 0.0)))], None), None, "the board is where it is");
        assert_eq!(project.resting_conflict(&[(b, place((700.0, 0.0, 0.0)))], None), None, "and touching is contact");
        assert_eq!(
            project.resting_conflict(&[(b, place((650.0, 0.0, 0.0)))], None),
            Some(Resting { moved: b, standing: a }),
            "half a board inside its neighbour is a refusal"
        );
        // A move of the whole selection keeps their relation and is no overlap at all.
        assert_eq!(
            project.resting_conflict(&[(a, place((0.0, -100.0, 0.0))), (b, place((720.0, -100.0, 0.0)))], None),
            None,
            "a selection that moves rigidly stays as it was"
        );
    }

    /// The body a boolean cuts and the cutter a maker owns, both constructive: a 100 mm slab and a
    /// 50 mm cutter, 200 mm apart on the X axis.
    fn block_pair(project: &mut Project, root: Id) -> (Id, Id) {
        let block = |project: &mut Project, name: &str, at: (i64, i64, i64), side: i64| {
            let body = Constructive::extrude(
                Path::rectangle([Length::ZERO, Length::ZERO], [millimetres(side), millimetres(side)]).expect("a ring"),
                millimetres(18),
            )
            .expect("a body");
            project.add_object(root, name, place3(at), Geometry::Constructive(body)).expect("a body")
        };
        (block(project, "Slab", (0, 0, 0), 100), block(project, "Cutter", (200, 0, 0), 50))
    }

    #[test]
    fn a_marked_tool_sinks_into_a_body_and_an_unmarked_body_may_not() {
        // A cutter has to enter the body it cuts, and it cannot say so any other way than by being
        // marked a tool: the guard and both drag rules read the mark (W2, ADR 0197). Everything
        // else keeps the rule that a part may not come to rest inside another.
        let mut project = empty_project();
        let root = project.root().id();
        let (slab, cutter) = block_pair(&mut project, root);

        assert_eq!(
            project.resting_conflict(&[(cutter, place3((50, 50, 0)))], None),
            Some(Resting { moved: cutter, standing: slab }),
            "an unmarked body is a part, and a part may not sink into another"
        );
        assert_eq!(stops(limit(&project, &[cutter], MoveAxis::X, -150.0)).1, Some(slab), "and the drag stops at it");

        project.set_object_tool(cutter, true).expect("mark the cutter as a tool");
        assert_eq!(project.resting_conflict(&[(cutter, place3((50, 50, 0)))], None), None, "a tool may sink in");
        assert_eq!(
            stops(limit(&project, &[cutter], MoveAxis::X, -150.0)),
            (-150.0, None),
            "and the drag carries it in"
        );

        // The mark is a property of the body, not of a gesture: a tool beside the body still keeps
        // its own moves, and the body it stands in the way of is not a wall any more.
        project.set_object_tool(cutter, false).expect("unmark it");
        assert_eq!(stops(limit(&project, &[cutter], MoveAxis::X, -150.0)).1, Some(slab), "the part stops again");
    }

    /// A placement at whole millimetres.
    fn place3(at: (i64, i64, i64)) -> Placement {
        Placement::IDENTITY
            .with_translation(millimetres(at.0), millimetres(at.1), millimetres(at.2))
            .expect("a placement")
    }

    #[test]
    fn a_board_the_document_holds_in_another_slides_along_it_and_comes_out() {
        // A file written before the rule, or a typed coordinate, can hold two boards that already
        // overlap, and the write must let the maker repair it by hand. The measure is the depth the
        // pair is sunk to, and two coplanar boards are sunk by their own thickness: no slide along
        // the plane deepens that, so every placement along the face and every way out is granted
        // (X86, ADR 0162, ADR 0178).
        let mut project = empty_project();
        let root = project.root().id();
        let shelf = panel(&mut project, root, "Shelf", (0, 0, 0));
        panel(&mut project, root, "Neighbour", (600, 0, 0));
        // 100 mm of the shelf's 700 is inside the neighbour's 700, and the pair shares its thickness.
        assert_eq!(project.resting_conflict(&[(shelf, place((-650.0, 0.0, 0.0)))], None), None, "sliding out is legal");
        assert_eq!(project.resting_conflict(&[(shelf, place((0.0, 0.0, 0.0)))], None), None, "staying is legal");
        assert_eq!(
            project.resting_conflict(&[(shelf, place((0.0, -100.0, 0.0)))], None),
            None,
            "sliding along the face is legal too"
        );
        assert_eq!(
            project.resting_conflict(&[(shelf, place((50.0, 0.0, 0.0)))], None),
            None,
            "and so is sliding further behind it, which sinks the boards no deeper"
        );
    }

    #[test]
    fn a_body_standing_inside_another_is_dragged_out_and_never_deeper() {
        // X86: the drag and the write read one measure. A body the document already holds inside
        // another is carried out of it whole, and every axis that would sink it further is refused,
        // so the piece has exactly one way to go and the log names the body it is leaving.
        let mut project = empty_project();
        let root = project.root().id();
        let body = solid_box(&mut project, root, "Body", (100, 100, 100), place((0.0, 0.0, 0.0)));
        let host = solid_box(&mut project, root, "Host", (100, 100, 100), place((50.0, 0.0, 0.0)));
        let rule = rule(&project, &[body]);
        // 50 mm of the two bodies is shared, so going deeper along +X is refused before it starts…
        assert_eq!(stops(rule.limit(MoveAxis::X, 40.0)), (0.0, None), "a deeper place is not taken");
        assert_eq!(
            rule.blocked_at(DVec3::new(40.0, 0.0, 0.0)).and_then(Stop::node),
            Some(host),
            "and the answer names the body it is inside"
        );
        // …while the way out is the whole distance, in the drag and in the write alike.
        assert_eq!(rule.limit(MoveAxis::X, -400.0), moved(-400.0), "coming out is granted whole");
        assert_eq!(project.resting_conflict(&[(body, place((-400.0, 0.0, 0.0)))], None), None, "and so is the write");
        assert_eq!(
            project.resting_conflict(&[(body, place((40.0, 0.0, 0.0)))], None),
            Some(Resting { moved: body, standing: host }),
            "while the same place is refused by the write's own guard"
        );
    }

    #[test]
    fn a_piece_inside_a_case_stays_legal_and_a_shell_may_not_cross_a_shell() {
        // A body inside a shell is a shelf in a case, and it is legal at its own place and anywhere a
        // shelf would be set down; two shells know only containment, so a cabinet half inside another
        // is the one shell case the write refuses (ADR 0102).
        let mut project = empty_project();
        let case = product(&mut project, "10.01.01", (600, 560, 720), (0, 0, 0));
        let shelf = panel(&mut project, case, "Shelf", (0, 0, 0));
        assert_eq!(project.resting_conflict(&[(shelf, place((0.0, 0.0, 100.0)))], None), None, "a shelf in its case");
        let neighbour = product(&mut project, "10.01.01", (600, 560, 720), (2000, 0, 0));
        assert_eq!(
            project.resting_conflict(&[(neighbour, place((400.0, 0.0, 0.0)))], None),
            Some(Resting { moved: neighbour, standing: case }),
            "a cabinet driven half into another's envelope is refused"
        );
        let nested = product(&mut project, "10.01.02", (300, 300, 300), (2050, 50, 50));
        assert_eq!(
            project.resting_conflict(&[(nested, place((2100.0, 80.0, 60.0)))], None),
            None,
            "a cabinet wholly inside another's envelope stays legal"
        );
    }

    #[test]
    fn a_body_dragged_down_stops_on_the_invisible_floor_and_the_answer_names_it() {
        // X87: the workspace keeps a floor at Z = 0. A body standing a hand's width above it may
        // descend that far and no further, and the stop is the floor's own kind so a log says Floor
        // and not the node it happens to measure (ADR 0177).
        let mut project = empty_project();
        let root = project.root().id();
        let body = solid_box(&mut project, root, "Body", (100, 100, 100), place((0.0, 0.0, 100.0)));
        let rule = rule(&project, &[body]);
        let answer = rule.limit(MoveAxis::Z, -500.0);
        assert_eq!(answer.distance_mm, -100.0, "it may descend to the floor and not through it");
        assert_eq!(answer.contact, Some(Stop::Floor), "and the stop names the floor, not a node");
        assert_eq!(answer.capture, None, "the floor stops a drag and never captures one");
        assert_eq!(
            rule.blocked_at(DVec3::new(0.0, 0.0, -500.0)),
            Some(Stop::Floor),
            "the log's own question names the floor too"
        );
        assert_eq!(rule.blocked_at(DVec3::new(0.0, 0.0, -100.0)), None, "standing on it is a free place");
        assert_eq!(rule.limit(MoveAxis::Z, 250.0), moved(250.0), "and lifting the body is the hand's");

        // A diagonal carry keeps the two axes the floor does not bound.
        let carried = rule.limits(DVec3::new(120.0, 60.0, -300.0));
        assert_eq!(carried[MoveAxis::Z.index()].distance_mm, -100.0, "the vertical stops at the floor");
        assert_eq!(carried[MoveAxis::Z.index()].contact, Some(Stop::Floor));
        assert_eq!(stops(carried[MoveAxis::X.index()]), (120.0, None), "while X still arrives");
        assert_eq!(stops(carried[MoveAxis::Y.index()]), (60.0, None), "and so does Y");
    }

    #[test]
    fn the_floor_is_a_stop_the_workspace_turns_off_and_not_a_snap() {
        // The switch beside the snapping boxes is the workspace's: off means no floor at all, and on
        // it stops a drag whatever the snapping is set to (ADR 0146, ADR 0177).
        let mut project = empty_project();
        let root = project.root().id();
        let body = solid_box(&mut project, root, "Body", (100, 100, 100), place((0.0, 0.0, 0.0)));
        let unsnapped = project
            .move_rule(&[body], MoveSettings { snaps: None, ..MoveSettings::default() }, None)
            .expect("a body is measured without a snap");
        assert_eq!(
            unsnapped.limit(MoveAxis::Z, -80.0).contact,
            Some(Stop::Floor),
            "nothing snapping to the floor plane does not open the floor"
        );
        assert_eq!(unsnapped.limit(MoveAxis::Z, -80.0).distance_mm, 0.0, "a body on the floor stays on it");

        let open = project
            .move_rule(&[body], MoveSettings { floor: false, ..MoveSettings::default() }, None)
            .expect("a body is measured without a floor");
        assert_eq!(open.limit(MoveAxis::Z, -80.0), moved(-80.0), "with the floor off the body goes below it");
        assert_eq!(open.blocked_at(DVec3::new(0.0, 0.0, -80.0)), None, "and nothing is in the way");
    }

    #[test]
    fn a_body_the_document_holds_below_the_floor_may_be_lifted_out() {
        // A hand-edited file may hold a piece under the floor. The bound forbids going deeper and
        // nothing else, so such a piece is never pushed up by a drag (X87).
        let mut project = empty_project();
        let root = project.root().id();
        let sunk = solid_box(&mut project, root, "Sunk", (100, 100, 100), place((0.0, 0.0, -50.0)));
        let rule = rule(&project, &[sunk]);
        assert_eq!(rule.limit(MoveAxis::Z, -40.0).distance_mm, 0.0, "it does not sink any deeper");
        assert_eq!(rule.limit(MoveAxis::Z, -40.0).contact, Some(Stop::Floor));
        assert_eq!(rule.limit(MoveAxis::Z, 200.0), moved(200.0), "and it is lifted out by hand");
        assert_eq!(rule.limit(MoveAxis::X, 120.0), moved(120.0), "a sideways drag is untouched");
    }

    #[test]
    fn a_floor_bound_product_and_a_wall_class_read_the_floor_their_own_way() {
        // A floor-bound class already keeps its height, so the floor adds nothing to it; a wall class
        // travels in Z and meets the floor rather than a class rule (X87).
        let (floor, floor_id) = product_project("10.01.01");
        let answer = limit(&floor, &[floor_id], MoveAxis::Z, -300.0);
        assert_eq!(answer.distance_mm, 0.0, "a base cabinet still keeps its height");
        assert_eq!(answer.contact, None, "and the floor is not what says so: its class is");

        let mut project = empty_project();
        let hung = product(&mut project, "10.01.02", (600, 560, 720), (0, 0, 1000));
        let rule = rule(&project, &[hung]);
        assert_eq!(
            stops(rule.limit(MoveAxis::Z, -2000.0)),
            (-1000.0, None),
            "a hung cabinet comes down the whole way to the floor"
        );
        assert_eq!(rule.limit(MoveAxis::Z, -2000.0).contact, Some(Stop::Floor), "where the floor meets it");
        assert_eq!(rule.limit(MoveAxis::Z, 300.0), moved(300.0), "and it may still be hung higher");
    }

    #[test]
    fn the_mount_trait_decides_whether_a_product_keeps_its_height() {
        let (floor, floor_id) = product_project("10.01.01");
        assert_eq!(
            limit(&floor, &[floor_id], MoveAxis::Z, 120.0),
            MoveLimit { distance_mm: 0.0, contact: None, capture: None },
            "a base cabinet stands on the floor and cannot be lifted"
        );

        assert!(!rule(&floor, &[floor_id]).allows(MoveAxis::Z), "the vertical axis has no free distance");
        assert!(rule(&floor, &[floor_id]).allows(MoveAxis::X), "the floor plane stays free");

        let (wall, wall_id) = product_project("10.01.02");
        assert_eq!(
            limit(&wall, &[wall_id], MoveAxis::Z, 120.0),
            MoveLimit { distance_mm: 120.0, contact: None, capture: None },
            "a wall cabinet hangs where the maker puts it"
        );

        // The same node, not a second one: the row decides, never the container or the command.
        let mut changed = floor;
        changed
            .set_property(floor_id, SYSTEM_CLASS, PropertyValue::class(class("10.01.02")))
            .expect("a wall class keeps the envelope");
        assert_eq!(
            limit(&changed, &[floor_id], MoveAxis::Z, 120.0),
            MoveLimit { distance_mm: 120.0, contact: None, capture: None },
            "the same product may be hung once its class says so"
        );
    }

    #[test]
    fn a_floor_bound_product_still_slides_and_snaps_on_the_floor() {
        let (project, id) = product_project("10.01.01");
        assert_eq!(limit(&project, &[id], MoveAxis::X, 297.4), moved(297.0));
    }

    #[test]
    fn a_shell_stops_at_contact_with_another_shell() {
        let mut project = empty_project();
        let cabinet = product(&mut project, "10.01.01", (600, 560, 720), (0, 0, 0));
        let neighbour = product(&mut project, "10.01.01", (600, 560, 720), (1000, 0, 0));
        // The cabinet spans x = 0..600 and the neighbour starts at 1000: a destination on the
        // neighbour is cut at its face, and the same drag goes past it when the place is free.
        assert_eq!(stops(limit(&project, &[cabinet], MoveAxis::X, 700.0)), (400.0, Some(neighbour)));
        // The neighbour may come back the same way and stop at its own contact.
        assert_eq!(stops(limit(&project, &[neighbour], MoveAxis::X, -700.0)), (-400.0, Some(cabinet)));
    }

    #[test]
    fn a_multi_axis_proposal_slides_along_the_axis_a_contact_leaves_free() {
        let (mut project, cabinet) = product_project("10.01.01");
        let neighbour = product(&mut project, "10.01.01", (600, 560, 720), (1000, 0, 0));
        let rule = project.move_rule(&[cabinet], settings(SnapSettings::default()), None).expect("a furniture rule");

        let limits = rule.limits(DVec3::new(1000.0, 120.0, 30.0));
        assert_eq!(stops(limits[MoveAxis::X.index()]), (400.0, Some(neighbour)), "the neighbour stops X at its face");
        assert_eq!(
            limits[MoveAxis::Y.index()],
            MoveLimit { distance_mm: 120.0, contact: None, capture: None },
            "and the hand's Y travel keeps sliding along that face"
        );
        assert_eq!(
            limits[MoveAxis::Z.index()],
            MoveLimit { distance_mm: 0.0, contact: None, capture: None },
            "while the floor-bound class keeps its height"
        );
    }

    #[test]
    fn the_class_decides_every_axis_of_a_multi_axis_proposal() {
        let (mut project, id) = product_project("10.01.01");
        let floor = project.move_rule(&[id], settings(SnapSettings::default()), None).expect("a floor rule");
        assert_eq!(floor.axes(), [true, true, false], "a base cabinet keeps its height");
        assert_eq!(floor.limits(DVec3::new(0.0, 0.0, 120.0))[MoveAxis::Z.index()].distance_mm, 0.0);

        project.set_property(id, SYSTEM_CLASS, PropertyValue::class(class("10.01.02"))).expect("a wall class");
        let wall = project.move_rule(&[id], settings(SnapSettings::default()), None).expect("a wall rule");
        assert_eq!(wall.axes(), [true, true, true], "the same frame may be hung once its class says so");
        assert_eq!(
            wall.limits(DVec3::new(0.0, 0.0, 120.0))[MoveAxis::Z.index()].distance_mm,
            120.0,
            "and the vertical is the hand's to carry"
        );
    }

    #[test]
    fn a_contained_shell_moves_inside_until_it_would_cross_a_wall() {
        let mut project = sample_project();
        let outer = product(&mut project, "10.01.02", (2000, 1000, 2000), (0, 0, 0));
        let inner = product(&mut project, "10.01.02", (600, 500, 700), (700, 200, 500));
        // A destination that straddles the outer piece's ceiling is cut at that wall…
        assert_eq!(
            stops(limit(&project, &[inner], MoveAxis::Z, 1000.0)),
            (800.0, Some(outer)),
            "a unit inside a niche stops where its top meets the ceiling"
        );
        assert_eq!(
            stops(limit(&project, &[inner], MoveAxis::Z, -600.0)),
            (-500.0, Some(outer)),
            "and down until it stands on the floor of the outer piece"
        );
        // …while a destination wholly beyond it is free, and the unit lands on the outer's top.
        assert_eq!(
            limit(&project, &[inner], MoveAxis::Z, 1500.0),
            MoveLimit {
                distance_mm: 1500.0,
                contact: None,
                capture: Some(Capture { end: SnapEnd::Low, at_mm: 2000.0, surface: SnapSurface::Frame(outer) })
            },
            "a free place beyond the case is reachable: the unit jumps out and lands on it"
        );
    }

    #[test]
    fn a_body_stops_at_a_body() {
        // A shelf is a board, so the object move slides it along its thickness — upright (N22) —
        // and the contact it meets is the panel above it in the same case.
        let (mut project, piece) = product_project("10.01.01");
        let shelf = panel(&mut project, piece, "Shelf", (0, 1000, 0));
        let above = panel(&mut project, piece, "Above", (0, 1000, 1000));
        assert_eq!(
            stops(limit(&project, &[shelf], MoveAxis::Z, 1000.0)),
            (982.0, Some(above)),
            "the shelf rises 982 mm, which is where its 18 mm board meets the panel above"
        );
    }

    /// The answer of a drag whose high face landed `at` millimetres on `surface`.
    fn landed(distance_mm: f64, at: f64, surface: SnapSurface) -> MoveLimit {
        MoveLimit { distance_mm, contact: None, capture: Some(Capture { end: SnapEnd::High, at_mm: at, surface }) }
    }

    #[test]
    fn a_drag_lands_on_the_grid_and_on_a_face() {
        // A lone part inside a piece: nothing to meet, so the 1 mm grid decides.
        let (mut project, piece) = product_project("10.01.01");
        let shelf = panel(&mut project, piece, "Shelf", (0, 1000, 0));
        assert_eq!(limit(&project, &[shelf], MoveAxis::Z, 97.4), moved(97.0));

        // A neighbour frame's virtual face at z = 1000 captures the shelf's upper face, and a body
        // never stops at a shell, so the face — not the grid — is the answer.
        let frame = product(&mut project, "10.01.01", (600, 560, 720), (0, 0, 1000));
        let shelf = panel(&mut project, piece, "Second shelf", (0, 0, 0));
        assert_eq!(
            limit(&project, &[shelf], MoveAxis::Z, 979.4),
            landed(982.0, 1000.0, SnapSurface::Frame(frame)),
            "the shelf's upper face lands on the neighbouring frame's virtual face, not on the grid"
        );

        // The same drag in a world where the frame is gone: the part's own face is the surface the
        // face meets, so a neighbouring part is a snap target too.
        let mut project = empty_project();
        let piece = product(&mut project, "10.01.01", (600, 560, 720), (0, 0, 0));
        let shelf = panel(&mut project, piece, "Shelf", (0, 0, 0));
        let beside = panel(&mut project, piece, "Beside", (0, 0, 1000));
        assert_eq!(
            limit(&project, &[shelf], MoveAxis::Z, 979.4),
            landed(982.0, 1000.0, SnapSurface::Face(beside)),
            "a neighbouring part face is a snap target too"
        );
    }

    #[test]
    fn a_drag_lands_on_the_middle_of_the_space_it_stands_in() {
        // The middle of a box is the one coordinate its faces do not offer, so the centre kind lands
        // a shelf at the mid-height of the cabinet's reserved space (U116). A body's own middle is
        // inside the body, where contact keeps a part out, so only a shell offers it.
        let mut project = empty_project();
        let piece = product(&mut project, "10.01.01", (600, 560, 720), (0, 0, 0));
        let shelf = panel(&mut project, piece, "Shelf", (0, 0, 0));
        // The envelope spans z = 0..720, so its middle stands at 360: the shelf's upper face lands
        // there, which is a travel of 342 mm.
        assert_eq!(
            limit(&project, &[shelf], MoveAxis::Z, 340.6),
            MoveLimit {
                distance_mm: 342.0,
                contact: None,
                capture: Some(Capture { end: SnapEnd::High, at_mm: 360.0, surface: SnapSurface::Centre(piece) })
            }
        );

        // The same drag with the centre kind off lands on the millimetre grid again.
        let mut kinds = SnapKinds::ALL;
        kinds.centre = false;
        let without = rule_with(&project, &[shelf], SnapSettings { kinds, ..SnapSettings::default() });
        assert_eq!(without.limit(MoveAxis::Z, 340.6), moved(341.0), "the grid rounds the travel");
    }

    #[test]
    fn a_wall_product_lands_on_the_floor() {
        let mut project = empty_project();
        let hanging = product(&mut project, "10.01.02", (600, 560, 720), (0, 0, 3));
        assert_eq!(
            limit(&project, &[hanging], MoveAxis::Z, -1.5),
            MoveLimit {
                distance_mm: -3.0,
                contact: None,
                capture: Some(Capture { end: SnapEnd::Low, at_mm: 0.0, surface: SnapSurface::Plane })
            },
            "a piece 1.5 mm above the floor lands on it"
        );
    }

    #[test]
    fn a_drag_of_a_solid_body_takes_the_planes_and_the_grid_and_not_another_parts_box() {
        // A body that is not furniture is a solid: the object move lands it on the coordinate planes
        // and the millimetre grid, while another part's world box captures nothing, because that box
        // is the coarse reading the body is no longer measured by (ADR 0165). Contact is still the
        // document's rule (ADR 0162).
        let mut project = empty_project();
        let root = project.root().id();
        let body = solid_box(&mut project, root, "Body", (100, 100, 100), place((0.0, 0.0, 1500.0)));
        let plane = Some(Capture { end: SnapEnd::Low, at_mm: 0.0, surface: SnapSurface::Plane });
        assert_eq!(
            rule(&project, &[body]).limit(MoveAxis::Z, -1498.0),
            MoveLimit { distance_mm: -1500.0, contact: None, capture: plane },
            "the body lands on the floor plane"
        );
        assert_eq!(limit(&project, &[body], MoveAxis::Z, -97.4), moved(-97.0), "and on the millimetre grid");

        // A neighbour 2 mm above: a furniture piece would jump onto its face, a solid body does not,
        // because the face it would land on is the neighbour's own and no world box stands in for it.
        let under = solid_box(&mut project, root, "Under", (100, 100, 100), place((0.0, 0.0, 1702.0)));
        assert_eq!(limit(&project, &[body], MoveAxis::Z, 100.0), moved(100.0), "no face capture for a solid");

        // The same pair of bodies is still contact: a solid body may not be laid inside another.
        assert_eq!(
            stops(limit(&project, &[body], MoveAxis::Z, 300.0)),
            (102.0, Some(under)),
            "it stops where its own face meets the neighbour"
        );

        // The free move keeps its own reading: no grid, no plane, no face, the pointer's distance.
        let free = project.move_rule(&[body], MoveSettings::default().free(), None).expect("a solid body");
        assert_eq!(free.limit(MoveAxis::Z, 40.4), moved(40.4), "the free move snaps to nothing");
    }

    #[test]
    fn a_non_finite_proposal_moves_nothing() {
        let (project, id) = product_project("10.01.01");
        assert_eq!(rule(&project, &[id]).limit(MoveAxis::X, f64::NAN), moved(0.0));
    }

    #[test]
    fn a_turned_product_is_measured_by_its_world_box() {
        let (mut project, cabinet) = product_project("10.01.01");
        let neighbour = product(&mut project, "10.01.01", (600, 560, 720), (1000, -600, 0));
        // Turn the cabinet a quarter about Z: its depth (560) now runs along +X and its width along
        // -Y, so its world box reaches x = 560 and the neighbour stands in that turned frame. A
        // destination on the neighbour is cut after 440 mm.
        let turned = Placement::IDENTITY
            .with_rotation_euler(EulerAngles { z: Degrees::from_millidegrees(-90_000), ..EulerAngles::default() })
            .expect("a quarter turn");
        project.set_node_placement(cabinet, turned).expect("turn the cabinet");
        assert_eq!(stops(limit(&project, &[cabinet], MoveAxis::X, 1100.0)), (440.0, Some(neighbour)));
    }

    #[test]
    fn a_frame_lands_on_the_coordinate_plane_within_the_tolerance() {
        // An empty world, a piece 1500 mm out along +Y: its low face is the node's own origin, and
        // dragging it back towards the origin brings that face near the Y = 0 plane.
        let mut project = empty_project();
        let piece = product(&mut project, "10.01.01", (600, 560, 720), (0, 1500, 0));
        let plane = Some(Capture { end: SnapEnd::Low, at_mm: 0.0, surface: SnapSurface::Plane });
        assert_eq!(
            limit(&project, &[piece], MoveAxis::Y, -1498.0),
            MoveLimit { distance_mm: -1500.0, contact: None, capture: plane },
            "a face 2 mm off the plane jumps onto it and lands flush"
        );
        assert_eq!(
            limit(&project, &[piece], MoveAxis::Y, -1470.0),
            moved(-1470.0),
            "a face 30 mm off the plane is past the tolerance and stays where the pointer put it"
        );
        // A frame at +X meets the X = 0 plane the same way: the planes are three, not one.
        let mut project = empty_project();
        let piece = product(&mut project, "10.01.01", (600, 560, 720), (1500, 0, 0));
        assert_eq!(
            limit(&project, &[piece], MoveAxis::X, -1498.0).capture,
            plane,
            "the X = 0 plane is a candidate too"
        );
    }

    #[test]
    fn the_tolerance_comes_from_the_settings_and_not_from_a_literal() {
        let mut project = empty_project();
        let piece = product(&mut project, "10.01.01", (600, 560, 720), (0, 1500, 0));
        // The face lands 3 mm off the plane: the default 5 mm captures it, a 2 mm workspace does not.
        let near = rule_with(&project, &[piece], SnapSettings::default()).limit(MoveAxis::Y, -1497.0);
        assert_eq!(near.distance_mm, -1500.0);
        assert!(near.capture.is_some());
        let tight = SnapSettings { tolerance: millimetres(2), ..SnapSettings::default() };
        assert_eq!(
            rule_with(&project, &[piece], tight).limit(MoveAxis::Y, -1497.0),
            moved(-1497.0),
            "a 2 mm tolerance lets a 3 mm gap alone"
        );
        // The value is a length in the document's unit, and the default is the maker's 5 mm.
        assert_eq!(SnapSettings::default().tolerance, millimetres(SnapSettings::DEFAULT_TOLERANCE_MM));
    }

    #[test]
    fn two_frames_meet_flush_inside_the_tolerance_and_stay_apart_beyond() {
        let mut project = empty_project();
        let moving = product(&mut project, "10.01.01", (600, 560, 720), (0, 0, 0));
        let neighbour = product(&mut project, "10.01.01", (600, 560, 720), (1000, 0, 0));
        // The moving frame spans x = 0..600; a 1000 mm drag would leave its far face at 1600, so the
        // contact stops it at 400 and the two faces are flush. A drag short of that stops where the
        // snap put it: 2 mm of gap collapses onto the neighbour, 30 mm does not.
        assert_eq!(
            limit(&project, &[moving], MoveAxis::X, 398.0),
            MoveLimit {
                distance_mm: 400.0,
                contact: None,
                capture: Some(Capture { end: SnapEnd::High, at_mm: 1000.0, surface: SnapSurface::Frame(neighbour) })
            },
            "two frames 2 mm apart jump together"
        );
        assert_eq!(limit(&project, &[moving], MoveAxis::X, 370.0), moved(370.0), "two frames 30 mm apart stay apart");
    }

    #[test]
    fn turning_the_planes_off_leaves_the_piece_on_the_grid() {
        let mut project = empty_project();
        let piece = product(&mut project, "10.01.01", (600, 560, 720), (0, 1500, 0));
        let no_planes = rule_with(&project, &[piece], snapping(false, true));
        assert_eq!(
            no_planes.limit(MoveAxis::Y, -1498.0),
            moved(-1498.0),
            "with the planes off the drag stays where the pointer put it"
        );
        // The floor is the Z = 0 plane and belongs to the same switch: a 1.5 mm gap no longer snaps.
        let mut project = empty_project();
        let hanging = product(&mut project, "10.01.02", (600, 560, 720), (0, 0, 3));
        assert_eq!(
            rule_with(&project, &[hanging], snapping(false, true)).limit(MoveAxis::Z, -1.5),
            moved(-2.0),
            "the floor snap is off with the planes"
        );
    }

    #[test]
    fn turning_the_frames_off_drops_the_capture_and_keeps_the_stop() {
        let mut project = empty_project();
        let moving = product(&mut project, "10.01.01", (600, 560, 720), (0, 0, 0));
        let neighbour = product(&mut project, "10.01.01", (600, 560, 720), (1000, 0, 0));
        let no_frames = rule_with(&project, &[moving], snapping(true, false));
        // The capture to the neighbour's virtual face is gone, so the 2 mm gap stays open…
        assert_eq!(no_frames.limit(MoveAxis::X, 398.0), moved(398.0), "a frame is not a snap candidate");
        // …while the contact is the document's rule and still stops the drag flush against it.
        assert_eq!(
            stops(no_frames.limit(MoveAxis::X, 1000.0)),
            (400.0, Some(neighbour)),
            "turning frames off does not let a shell through another shell"
        );
    }

    /// The placement a turn about `pivot` gives the node that stands at `local`.
    fn turned_from(local: Placement, pivot: DVec3, angle: f64, axis: DVec3) -> Placement {
        let turn = DQuat::from_axis_angle(axis, angle);
        Placement::new(pivot + turn * (local.translation() - pivot), turn * local.rotation()).expect("a placement")
    }

    #[test]
    fn a_turn_of_a_door_into_a_wall_stops_on_its_face() {
        // X88's own case: a door hinged at its corner, turned into the case it belongs to. The drag
        // answers the angle it may make, names the wall that stops it, and the release writes a
        // placement the write's guard accepts — the drag, the release and the refusal are one
        // measurement (ADR 0180, ADR 0179).
        let mut project = empty_project();
        let door = block_object(&mut project, "Door", place((250.0, 10.0, 350.0)), (500.0, 20.0, 700.0));
        // The hinge is the door's own bottom corner, which is the world origin: a pivot is not the
        // node's origin and the turn reads it, exactly as the ring does (ADR 0026).
        project
            .set_node_pivot(door, PivotLocal::new(millimetres(-250), millimetres(-10), millimetres(-350)))
            .expect("a pivot at the hinge");
        let wall = block_object(&mut project, "Wall", place((0.0, 310.0, 350.0)), (3000.0, 20.0, 700.0));
        let rule = project.rotate_rule(&[door], Some(&contact_parry::Parry)).expect("a rotate rule");
        assert_eq!(rule.pivot(), DVec3::ZERO, "the rule turns about the ring's own centre");

        let quarter = std::f64::consts::FRAC_PI_2;
        let stop = rule.limit(DVec3::Z, quarter);
        assert!(stop.angle > 0.0 && stop.angle < quarter, "the turn into the wall is cut: {stop:?}");
        assert_eq!(stop.contact.and_then(Stop::node), Some(wall), "and the wall is named");
        // The door's far corner stands on the wall's face: 500 sin θ + 20 cos θ = 300.
        let corner = DQuat::from_axis_angle(DVec3::Z, stop.angle) * DVec3::new(500.0, 20.0, 0.0);
        assert!((corner.y - 300.0).abs() < 0.01, "the far corner meets the wall at 300: {corner:?}");

        // The way out of the case is granted whole and nothing is in the way.
        let away = rule.limit(DVec3::Z, -quarter);
        assert_eq!(away.angle, -quarter, "a turn away from the wall is the hand's own");
        assert_eq!(away.contact, None);

        // The release reads the drag's own answer: the granted turn is written where one degree
        // further is refused.
        let start = project.object(door).expect("the door").placement();
        let accepted = turned_from(start, DVec3::ZERO, stop.angle, DVec3::Z);
        assert_eq!(
            project.resting_conflict(&[(door, accepted)], Some(&contact_parry::Parry)),
            None,
            "the turn the drag showed is the turn the write takes"
        );
        let deeper = turned_from(start, DVec3::ZERO, stop.angle + 1.0_f64.to_radians(), DVec3::Z);
        assert_eq!(
            project.resting_conflict(&[(door, deeper)], Some(&contact_parry::Parry)).map(|resting| resting.standing),
            Some(wall),
            "a degree further is inside the wall, and the guard names it"
        );
    }

    #[test]
    fn a_turn_of_a_cone_reaches_further_than_the_box_around_it() {
        // The rule asks the port for the body at the turned pose, so a cone tips over its own surface
        // and not the corner its box would invent: measured against the same wall, the box stops the
        // turn eight degrees early (X85, X88).
        let mut project = empty_project();
        let root = project.root().id();
        let cone = crate::Primitive::cone(millimetres(100), Length::ZERO, millimetres(720), 32).expect("a cone");
        let cone =
            project.add_object(root, "Cone", place((0.0, 0.0, 0.0)), crate::Geometry::Primitive(cone)).expect("a cone");
        let wall = block_object(&mut project, "Wall", place((410.0, 0.0, 250.0)), (20.0, 1000.0, 1500.0));

        let quarter = std::f64::consts::FRAC_PI_2;
        let by_box = project.rotate_rule(&[cone], None).expect("a box rule");
        let by_port = project.rotate_rule(&[cone], Some(&contact_parry::Parry)).expect("a measured rule");
        let boxed = by_box.limit(DVec3::Y, quarter);
        let measured = by_port.limit(DVec3::Y, quarter);
        assert!(measured.angle > boxed.angle + 1.0_f64.to_radians(), "the cone beats its box: {measured:?}");
        assert_eq!(measured.contact.and_then(Stop::node), Some(wall), "the wall stops the tip");
        // The cone's own apex stands on the wall's face: 720 sin θ = 400.
        let apex = DQuat::from_axis_angle(DVec3::Y, measured.angle) * DVec3::new(0.0, 0.0, 720.0);
        assert!((apex.x - 400.0).abs() < 0.01, "the tip meets the wall at 400: {apex:?}");
    }

    #[test]
    fn a_crossed_pair_stops_the_carry_however_far_the_hand_jumped() {
        // X93's own case: a 1010 mm board carried across a 1090 mm one, 60 mm into it in both
        // directions. The carry asks only the place a frame lands, so a hand that jumps past a board
        // never sees the face it went through: the port answers the depth of a crossing, and the piece
        // stops where the box around the neighbour always said it would.
        let mut project = empty_project();
        let standing = block_object(&mut project, "Standing", place((-330.0, -115.0, 0.0)), (60.0, 1090.0, 100.0));
        let carried = block_object(&mut project, "Carried", place((292.0, 275.0, 0.0)), (1010.0, 60.0, 100.0));
        let rule = project
            .move_rule(&[carried], MoveSettings::default(), Some(&contact_parry::Parry))
            .expect("a measured rule");
        // The crossing 60 mm deep, then the same crossing a frame later: the hand moved 40 mm between
        // two frames of a slow window, which is where the piece used to go straight through.
        for delta in [DVec3::new(-328.0, -26.0, 0.0), DVec3::new(-450.0, 0.0, 0.0)] {
            let stop = rule.blocked_at(delta).expect("the standing board is in the way");
            assert_eq!(stop.node(), Some(standing), "and it is the one named");
        }
        // A place the two boards do not share is still the hand's own.
        assert_eq!(rule.blocked_at(DVec3::new(0.0, 385.0, 0.0)), None, "a clear place is granted");
    }

    #[test]
    fn a_turn_whose_destination_crosses_a_board_is_measured() {
        // The turn reads the same port as the carry (X93): a board standing beside a strip and turned
        // by a quarter ends across it, which is the crossing the port used to answer as nothing. The
        // deep destination is named, and the ring is cut where the board's far corner meets the strip
        // — 505 sin θ + 30 cos θ = 270, the strip's near face.
        let mut project = empty_project();
        let strip = block_object(&mut project, "Strip", place((0.0, 300.0, 0.0)), (1010.0, 60.0, 18.0));
        let upright = block_object(&mut project, "Upright", place((0.0, 0.0, 0.0)), (1010.0, 60.0, 100.0));
        let rule = project.rotate_rule(&[upright], Some(&contact_parry::Parry)).expect("a measured rule");
        let quarter = std::f64::consts::FRAC_PI_2;

        let deep = rule.blocked_at(DVec3::Z, quarter).expect("the quarter turn crosses the strip");
        assert_eq!(deep.node(), Some(strip), "and the strip is the one named");

        let stop = rule.limit(DVec3::Z, quarter);
        assert!(stop.angle > 0.0 && stop.angle < quarter, "the turn into the strip is cut: {stop:?}");
        assert_eq!(stop.contact.and_then(Stop::node), Some(strip), "and the strip stops it");
        let corner = DQuat::from_axis_angle(DVec3::Z, stop.angle) * DVec3::new(505.0, 30.0, 0.0);
        assert!((corner.y - 270.0).abs() < 0.05, "the far corner meets the strip at 270: {corner:?}");

        // A turn the strip is nowhere near is the hand's own and names nothing — the board is long in
        // both directions, so a quarter either way sweeps a far corner into the strip.
        let away = rule.limit(DVec3::Z, -0.1);
        assert_eq!(away.angle, -0.1, "a turn clear of the strip is granted whole");
        assert_eq!(away.contact, None);
    }
}
