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
//!   another part's world box, which is the coarse reading it does not share (ADR 0165).
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
//! The answer is derived from the placements the document holds, which during a drag are still the
//! drag's start placements, so the preview and the single `node.placement.set_many` on release
//! cannot disagree. Nothing is stored and nothing is mutated.
//!
//! [`Project::move_rule`] answers with the rule wherever a piece can be measured. The panel axis is
//! the one part of the rule that needs no surface, so [`Project::move_axes`] answers it wherever the
//! board stands; the free move asks [`Project::contact_rule`], which refuses no axis and snaps to
//! nothing.

use geom::DVec3;

use super::surface::{Span, SurfaceBox, SurfaceKind, extent, separating_axes, surface_box};
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

/// How far an object drag may go, and what stopped it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoveLimit {
    /// The signed distance the drag may travel along its world axis, in millimetres.
    ///
    /// It is a whole number of micrometres, which is the document's unit, so the preview and the
    /// committed placement are the same number.
    pub distance_mm: f64,
    /// The node whose surface the drag stopped at, when contact stopped it before the pointer did.
    pub contact: Option<Id>,
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

/// What an object drag snaps to, and how near.
///
/// The values are a workspace setting (`crates/ui/src/settings.rs` holds the window and the file),
/// so the model owns their meaning and their defaults but never stores them: [`Project::move_rule`]
/// is handed the value, and a rule asked with the defaults behaves the way the editor always has
/// (ADR 0146).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SnapSettings {
    /// Whether the coordinate planes X=0, Y=0 and Z=0 capture a drag, the floor included.
    pub planes: bool,
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
        Self { planes: true, frames: true, tolerance: Length::from_micrometres(Self::DEFAULT_TOLERANCE_MM * 1000) }
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
    pub fn move_rule(&self, moving: &[Id], snaps: SnapSettings) -> Option<MoveRule> {
        let targets = self.moved_targets(moving);
        let axes = self.axes_of(&targets);
        self.rule_of(&targets, axes, Some(snaps))
    }

    /// The rule the free move is measured by: contact only.
    ///
    /// The free tool refuses no axis and snaps to nothing — the pointer's own distance, quantised to
    /// the micrometre the document stores — but it does not pass through a neighbour: contact is the
    /// document's rule and not a taste (ADR 0102, ADR 0162).
    pub fn contact_rule(&self, moving: &[Id]) -> Option<MoveRule> {
        let targets = self.moved_targets(moving);
        self.rule_of(&targets, ALL_MOVE_AXES, None)
    }

    /// The rule that measures a drag of `targets`, snapping only when `snaps` says so.
    ///
    /// The axes are the caller's: the object tool hands the intersection of what every moved node
    /// allows, so a loose board keeps its own thickness axis wherever it stands (N22, ADR 0147), and
    /// the free tool hands all three.
    fn rule_of(&self, targets: &[Id], axes: [bool; 3], snaps: Option<SnapSettings>) -> Option<MoveRule> {
        if targets.is_empty() {
            return None;
        }
        let movers = self.movers(targets);
        if movers.is_empty() {
            return None;
        }
        Some(MoveRule { axes, obstacles: self.obstacle_boxes(targets), movers, snaps })
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
    /// and one of its descendants. A pair that only touches is contact, not an overlap, and a pair
    /// that already overlaps is no *new* overlap: the answer refuses only a placement that brings a
    /// part into an overlap it did not have, so a file written before this rule opens and its maker
    /// drags the part out by hand.
    pub fn resting_conflict(&self, placements: &[(Id, Placement)]) -> Option<Resting> {
        let asked: Vec<Id> = placements.iter().map(|(id, _)| *id).collect();
        let targets = self.moved_targets(&asked);
        if targets.is_empty() {
            return None;
        }
        let obstacles = self.obstacle_boxes(&targets);
        for moved in &targets {
            let Some(lands) =
                self.world_placement_with(*moved, placements).and_then(|world| self.box_at(*moved, world))
            else {
                continue;
            };
            let stood = self.mover_box(*moved);
            let product = self.product_of(*moved);
            if let Some(standing) = obstacles.iter().find(|obstacle| resting_inside(stood, lands, **obstacle, product))
            {
                return Some(Resting { moved: *moved, standing: standing.id });
            }
        }
        None
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

    /// The world box each moved node carries, and whether its contact is measured by its own faces.
    fn movers(&self, targets: &[Id]) -> Vec<Mover> {
        targets
            .iter()
            .filter_map(|id| {
                self.mover_box(*id).map(|box_| Mover {
                    box_,
                    faces: !self.is_furniture(*id),
                    product: self.product_of(*id),
                })
            })
            .collect()
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
/// already overlaps is no new overlap: a file written before this rule opens, and its maker drags the
/// part out and types where it goes. The drag's own rule (`MoveRule::limit`) stops a furniture piece
/// at the world-box contact and a solid body at its exact face contact, and neither reading is later
/// than this answer, so a drag reaches a contact before it reaches a refusal (ADR 0165).
fn resting_inside(was: Option<SurfaceBox>, now: SurfaceBox, obstacle: SurfaceBox, product: Option<Id>) -> bool {
    // A legitimate nesting is not an overlap: a piece in its own case and a unit inside another stay
    // legal wherever they stand, and the drag's own destination test reads the same rule (X77).
    if nested(now, obstacle, product) {
        return false;
    }
    if was.is_some_and(|was| was.meets(obstacle)) {
        return false;
    }
    now.meets(obstacle)
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
#[derive(Clone, Copy, Debug)]
struct Mover {
    /// The world box the node occupies at the start of the drag.
    box_: SurfaceBox,
    /// Whether contact with this node is measured by its own faces rather than by the world box.
    faces: bool,
    /// The product this node is a piece of, when it is one.
    ///
    /// A piece is legal inside its own case and nowhere else: a shelf inside its cabinet is a body
    /// in a shell, while a lamp pushed into the same cabinet is a body in a way (X77).
    product: Option<Id>,
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
    /// The surfaces that stay behind: the other products' envelopes, panels and objects.
    obstacles: Vec<SurfaceBox>,
    /// Which world axes the drag offers, in [`MoveAxis`] order.
    ///
    /// It is the intersection of what every moved node allows, taken once when the drag starts: a
    /// panel's own thickness axis, a floor-bound product's two horizontal axes (N22, ADR 0147).
    axes: [bool; 3],
    /// What this drag snaps to, and how near, as the workspace settings chose it (ADR 0146), or
    /// `None` for a drag no grid and no surface may move: the free move.
    snaps: Option<SnapSettings>,
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
    /// stopped the drag. A pair that already overlaps is the maker's to repair and is no new overlap:
    /// such a piece can always be dragged out of it, as the write's own guard reads it (ADR 0162).
    fn resolve(&self, asks: [f64; 3], from: DVec3) -> ([f64; 3], [Option<Id>; 3]) {
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
        let mut contacts: [Option<Id>; 3] = [None; 3];
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
                    .filter_map(|mover| mover.box_.shifted(delta).map(|box_| Mover { box_, ..*mover }))
                    .collect();
                // The travel still to make from where the piece stands, and the face that stops it.
                let limit = stop_at_contact(axis, ask - delta[index], &standing, &self.obstacles);
                let mut cut = delta[index] + limit.distance_mm;
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
                    limit.contact
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
    pub fn blocked_at(&self, delta: DVec3) -> Option<(Id, SurfaceKind, DVec3, DVec3)> {
        if !delta.is_finite() {
            return None;
        }
        for mover in &self.movers {
            let moved = mover.box_.shifted(delta)?;
            for obstacle in &self.obstacles {
                if nested(moved, *obstacle, mover.product) {
                    continue;
                }
                if !nested(mover.box_, *obstacle, mover.product) {
                    let was =
                        if mover.faces { mover.box_.meets(*obstacle) } else { mover.box_.overlaps_box(*obstacle) };
                    if was {
                        continue;
                    }
                }
                let blocked = if mover.faces { moved.meets(*obstacle) } else { moved.overlaps_box(*obstacle) };
                if blocked {
                    return Some((obstacle.id, obstacle.kind, obstacle.min, obstacle.max));
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
                if nested(start, *obstacle, mover.product) {
                    continue;
                }
                if mover.box_.meets(*obstacle) {
                    // The pair already shares a volume, so the path is not the tool's to police.
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
        if !delta.is_finite() {
            return false;
        }
        for mover in &self.movers {
            let Some(moved) = mover.box_.shifted(delta) else {
                return false;
            };
            for obstacle in &self.obstacles {
                // A pair the document already holds overlapping is the maker's to repair: only a
                // *new* overlap is a collision, exactly as the write's guard reads it (ADR 0162). A
                // legitimate nesting is not that kind of overlap — a piece in its case is nested, not
                // stuck — so it is exempt at the start and judged at the destination.
                if nested(moved, *obstacle, mover.product) {
                    continue;
                }
                if !nested(mover.box_, *obstacle, mover.product) {
                    let was =
                        if mover.faces { mover.box_.meets(*obstacle) } else { mover.box_.overlaps_box(*obstacle) };
                    if was {
                        continue;
                    }
                }
                let blocked = if mover.faces { moved.meets(*obstacle) } else { moved.overlaps_box(*obstacle) };
                if blocked {
                    return false;
                }
            }
        }
        true
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
    obstacles: &[SurfaceBox],
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
                capture(obstacle.span(axis).min, target);
                capture(obstacle.span(axis).max, target);
            }
        }
        if snaps.planes {
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

/// The snapped distance cut at the first surface in its way.
fn stop_at_contact(axis: MoveAxis, distance: f64, movers: &[Mover], obstacles: &[SurfaceBox]) -> MoveLimit {
    let sign = if distance < 0.0 { -1.0 } else { 1.0 };
    if distance == 0.0 {
        return MoveLimit { distance_mm: 0.0, contact: None, capture: None };
    }
    let mut nearest: Option<(f64, Id)> = None;
    for mover in movers {
        for obstacle in obstacles {
            // A body stops at a body, a shell at a shell, and a carcass at a solid body that is not
            // its own piece: only a legitimate nesting is exempt, and `blocking` knows that wall.
            let blocked = if mover.faces {
                blocking_faces(axis, sign, &mover.box_, obstacle)
            } else {
                blocking(axis, sign, &mover.box_, obstacle, nested(mover.box_, *obstacle, mover.product))
            };
            let Some(limit) = blocked else {
                continue;
            };
            if nearest.is_none_or(|(best, _)| limit < best) {
                nearest = Some((limit, obstacle.id));
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
/// overlaps one another partially is not the tool's to police.
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

    use geom::DVec3;
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

    /// The answer of a drag that snapped to nothing.
    fn moved(distance_mm: f64) -> MoveLimit {
        MoveLimit { distance_mm, contact: None, capture: None }
    }

    /// A workspace that snaps with the settings this test states.
    fn snapping(planes: bool, frames: bool) -> SnapSettings {
        SnapSettings { planes, frames, ..SnapSettings::default() }
    }

    fn rule(project: &Project, moving: &[Id]) -> MoveRule {
        rule_with(project, moving, SnapSettings::default())
    }

    fn rule_with(project: &Project, moving: &[Id], snaps: SnapSettings) -> MoveRule {
        project.move_rule(moving, snaps).expect("an object move")
    }

    fn limit(project: &Project, moving: &[Id], axis: MoveAxis, proposed: f64) -> MoveLimit {
        rule(project, moving).limit(axis, proposed)
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
            limit(&project, &[board], MoveAxis::X, 500.0),
            MoveLimit { distance_mm: 400.0, contact: Some(neighbour), capture: None },
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
            limit(&project, &[body], MoveAxis::X, 250.0),
            MoveLimit { distance_mm: 200.0, contact: Some(neighbour), capture: None },
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
            straight[MoveAxis::X.index()],
            MoveLimit { distance_mm: -94.0, contact: Some(standing), capture: None },
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
            straight[MoveAxis::X.index()],
            MoveLimit { distance_mm: 200.0, contact: Some(bar), capture: None },
            "a drag that starts on the near side stops at the near face"
        );
        let standing = DVec3::new(1530.0, 0.0, 0.0);
        let drift = rule.limits_from(standing, DVec3::new(1529.0, 0.0, 0.0));
        assert_eq!(
            drift[MoveAxis::X.index()],
            MoveLimit { distance_mm: 1530.0, contact: Some(bar), capture: None },
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
            project.resting_conflict(&[(body, direct)]).is_some(),
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
            project.resting_conflict(&[(body, landing)]),
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
            limit(&project, &[shelf], MoveAxis::Z, 1100.0),
            MoveLimit { distance_mm: 982.0, contact: Some(frame), capture: None },
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
            limit(&project, &[body], MoveAxis::X, -300.0),
            MoveLimit { distance_mm: -200.0, contact: Some(cabinet), capture: None },
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
            limit(&project, &[post], MoveAxis::Z, 5.0),
            MoveLimit { distance_mm: 0.0, contact: Some(slab), capture: None },
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
        assert_eq!(
            limits[MoveAxis::X.index()],
            MoveLimit { distance_mm: 200.0, contact: Some(neighbour), capture: None },
            "the neighbour's face stops X"
        );
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
        assert_eq!(project.resting_conflict(&[(b, place((720.0, 0.0, 0.0)))]), None, "the board is where it is");
        assert_eq!(project.resting_conflict(&[(b, place((700.0, 0.0, 0.0)))]), None, "and touching is contact");
        assert_eq!(
            project.resting_conflict(&[(b, place((650.0, 0.0, 0.0)))]),
            Some(Resting { moved: b, standing: a }),
            "half a board inside its neighbour is a refusal"
        );
        // A move of the whole selection keeps their relation and is no overlap at all.
        assert_eq!(
            project.resting_conflict(&[(a, place((0.0, -100.0, 0.0))), (b, place((720.0, -100.0, 0.0)))]),
            None,
            "a selection that moves rigidly stays as it was"
        );
    }

    #[test]
    fn a_board_that_already_overlaps_can_be_pulled_out_and_is_never_refused_again() {
        // A file written before the rule can hold two boards that already overlap, and the write must
        // let the maker repair it by hand: only a *new* overlap is a refusal, so a pair the document
        // already holds is the maker's to move (ADR 0162).
        let mut project = empty_project();
        let root = project.root().id();
        let shelf = panel(&mut project, root, "Shelf", (0, 0, 0));
        panel(&mut project, root, "Neighbour", (600, 0, 0));
        // 100 mm of the shelf's 700 is inside the neighbour's 700, and every placement of the pair is
        // no new overlap: out of it, or further in.
        assert_eq!(project.resting_conflict(&[(shelf, place((-650.0, 0.0, 0.0)))]), None, "sliding out is legal");
        assert_eq!(project.resting_conflict(&[(shelf, place((0.0, 0.0, 0.0)))]), None, "staying is legal");
        assert_eq!(project.resting_conflict(&[(shelf, place((50.0, 0.0, 0.0)))]), None, "and deeper is not refused");
    }

    #[test]
    fn a_piece_inside_a_case_stays_legal_and_a_shell_may_not_cross_a_shell() {
        // A body inside a shell is a shelf in a case, and it is legal at its own place and anywhere a
        // shelf would be set down; two shells know only containment, so a cabinet half inside another
        // is the one shell case the write refuses (ADR 0102).
        let mut project = empty_project();
        let case = product(&mut project, "10.01.01", (600, 560, 720), (0, 0, 0));
        let shelf = panel(&mut project, case, "Shelf", (0, 0, 0));
        assert_eq!(project.resting_conflict(&[(shelf, place((0.0, 0.0, 100.0)))]), None, "a shelf in its case");
        let neighbour = product(&mut project, "10.01.01", (600, 560, 720), (2000, 0, 0));
        assert_eq!(
            project.resting_conflict(&[(neighbour, place((400.0, 0.0, 0.0)))]),
            Some(Resting { moved: neighbour, standing: case }),
            "a cabinet driven half into another's envelope is refused"
        );
        let nested = product(&mut project, "10.01.02", (300, 300, 300), (2050, 50, 50));
        assert_eq!(
            project.resting_conflict(&[(nested, place((2100.0, 80.0, 60.0)))]),
            None,
            "a cabinet wholly inside another's envelope stays legal"
        );
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
        assert_eq!(
            limit(&project, &[cabinet], MoveAxis::X, 700.0),
            MoveLimit { distance_mm: 400.0, contact: Some(neighbour), capture: None }
        );
        // The neighbour may come back the same way and stop at its own contact.
        assert_eq!(
            limit(&project, &[neighbour], MoveAxis::X, -700.0),
            MoveLimit { distance_mm: -400.0, contact: Some(cabinet), capture: None }
        );
    }

    #[test]
    fn a_multi_axis_proposal_slides_along_the_axis_a_contact_leaves_free() {
        let (mut project, cabinet) = product_project("10.01.01");
        let neighbour = product(&mut project, "10.01.01", (600, 560, 720), (1000, 0, 0));
        let rule = project.move_rule(&[cabinet], SnapSettings::default()).expect("a furniture rule");

        let limits = rule.limits(DVec3::new(1000.0, 120.0, 30.0));
        assert_eq!(
            limits[MoveAxis::X.index()],
            MoveLimit { distance_mm: 400.0, contact: Some(neighbour), capture: None },
            "the neighbour stops X at its face"
        );
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
        let floor = project.move_rule(&[id], SnapSettings::default()).expect("a floor rule");
        assert_eq!(floor.axes(), [true, true, false], "a base cabinet keeps its height");
        assert_eq!(floor.limits(DVec3::new(0.0, 0.0, 120.0))[MoveAxis::Z.index()].distance_mm, 0.0);

        project.set_property(id, SYSTEM_CLASS, PropertyValue::class(class("10.01.02"))).expect("a wall class");
        let wall = project.move_rule(&[id], SnapSettings::default()).expect("a wall rule");
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
            limit(&project, &[inner], MoveAxis::Z, 1000.0),
            MoveLimit { distance_mm: 800.0, contact: Some(outer), capture: None },
            "a unit inside a niche stops where its top meets the ceiling"
        );
        assert_eq!(
            limit(&project, &[inner], MoveAxis::Z, -600.0),
            MoveLimit { distance_mm: -500.0, contact: Some(outer), capture: None },
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
            limit(&project, &[shelf], MoveAxis::Z, 1000.0),
            MoveLimit { distance_mm: 982.0, contact: Some(above), capture: None },
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
            limit(&project, &[body], MoveAxis::Z, 300.0),
            MoveLimit { distance_mm: 102.0, contact: Some(under), capture: None },
            "it stops where its own face meets the neighbour"
        );

        // The free move keeps its own reading: no grid, no plane, no face, the pointer's distance.
        let free = project.contact_rule(&[body]).expect("a solid body has a contact rule");
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
        assert_eq!(
            limit(&project, &[cabinet], MoveAxis::X, 1100.0),
            MoveLimit { distance_mm: 440.0, contact: Some(neighbour), capture: None }
        );
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
            no_frames.limit(MoveAxis::X, 1000.0),
            MoveLimit { distance_mm: 400.0, contact: Some(neighbour), capture: None },
            "turning frames off does not let a shell through another shell"
        );
    }
}
