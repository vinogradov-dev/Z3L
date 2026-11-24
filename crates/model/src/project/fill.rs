//! The fill plan: the one part a click in a void creates, in the mode the tool is in.
//!
//! A post, a shelf and a wall are the same question asked of the niche the walls leave (ADR 0099,
//! ADR 0103, ADR 0106): which part fills it exactly, where does it land, and which outline does it
//! store? The answer is one [`FillPlan`], derived for cadence-free reasons — the ghost the maker
//! sees, the distance fields that place it and the `panel.create` the click dispatches all read it,
//! so the picture, the numbers and the history cannot disagree.
//!
//! Both modes start from the same cut of the cell at the board's own thickness ([`Niche::cut`]), so
//! a part is inside the walls at both faces by construction:
//!
//! - [`FillMode::Fit`] takes the **largest rectangle** with sides on the panel's own axes that lies
//!   inside that section. It is the safe default: it always fits, and it keeps the rectangle panel
//!   with its resize, its card and its nominal fields (ADR 0042).
//! - [`FillMode::Shape`] takes the **whole section**: the part is the shape the walls allow, and
//!   [`classify_outline`] decides whether that shape is still a standard panel — a rectangle, a
//!   rectangle with chamfered corners, a rectangle with radii — before it falls back to a contour.
//!   A wall that meets the board at an angle gives the outline edge it bounds an **end bevel**
//!   (ADR 0107): the outline is the section at the wide face, and every edge whose wall tilts gets
//!   the angle that sets the far face back into the wall's plane, so the board meets a leaned
//!   board flush instead of cutting in or leaving a wedge of air (ADR 0108).
//!
//! Nothing here is stored: the plan is a query, like the niche it starts from.

use std::collections::HashSet;
use std::f64::consts::{FRAC_PI_2, PI};

use geom::{Contact, DQuat, DVec2, DVec3, Region};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use units::{Degrees, Length};

use super::niche::{document_ring, signed_area, surface_corners};
use super::surface::SurfaceBox;
use crate::*;

/// How far a wall may lean out of the board's own plane before its end face is not an end face.
///
/// A plane whose normal lies this close to the board's thickness axis bounds no outline edge, which
/// is the same threshold the niche's section cut uses to leave such a plane to its hold test.
const LEVEL_WALL: f64 = 1.0e-3;

/// Chord tolerance of the polygon a plan reports beside the shape it stores, in millimetres.
///
/// A plane can describe a straight wall exactly and a curve only by chords, so a shaped part keeps its
/// curve in the **outline** it stores — the panel's own ring, arcs and all — and reports the polygon the
/// ghost draws beside it. A quarter of a millimetre is the tolerance the shell already samples a frame's
/// footprint at, so the ghost and the frame it sits in are cut alike (N17, N33).
const FILL_SAMPLE_MM: f64 = 0.25;

/// How far a measured corner may sit off the wall line it was cut from, in millimetres.
///
/// The section's corners are whole micrometres, so a corner is a fraction of one off the exact
/// intersection of the two lines it came from; five micrometres leaves room and still separates
/// two walls that are not the same wall.
const WALL_TOLERANCE: f64 = 5.0e-3;

/// The steepest end face a fill gives its part.
///
/// A wall leaned past a mitre is nearer the board's own plane than to a side face, so the "end face"
/// that would meet it is a taper across the large face — a variable-thickness board, which is not
/// this feature — and the void it leaves across the slab is a wedge (N12). `Shape` then takes the
/// slab section with no end bevel; `Fit` answers the same cut (ADR 0109).
const MAX_FILL_BEVEL: Degrees = Degrees::from_millidegrees(45_000);

/// The numbers one fill asks with, so the plan's signature stays a request rather than a list.
///
/// The five travel together because they are one gesture: the point the pointer named, the class's
/// axis pair, the mode the cell ran, the material's thickness and the free-axis position the tool
/// chose — a typed field's number, or the hand's place on the axis. A fill plan, its refusal and the
/// ghost a maker sees all read this value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FillRequest {
    /// The point the pointer named inside the open product.
    pub point: DVec3,
    /// The pair of the niche's sides the part takes.
    pub fill: Fill,
    /// How the part reads the section the walls leave.
    pub mode: FillMode,
    /// The material's thickness, which is the part's third side.
    pub thickness: Length,
    /// The free-axis position the tool chose, or `None` while the point under the pointer picks it.
    ///
    /// A typed field pins the number and a moving hand drives it; a caller that holds neither leaves
    /// the point's own coordinate to answer. Either way it is rounded to whole millimetres and kept
    /// between the cell's walls (ADR 0103).
    pub free: Option<f64>,
    /// How the board sits against the frame: inside the opening, or laid over its face (ADR 0240).
    pub mount: FillMount,
}

impl FillRequest {
    /// One tool's ask: the point it points at, the pair and mode its class names, and the thickness.
    pub const fn new(point: DVec3, fill: Fill, mode: FillMode, thickness: Length, free: Option<f64>) -> Self {
        Self { point, fill, mode, thickness, free, mount: FillMount::Inset }
    }

    /// The same ask with the mounting the tool is in: the board fills the opening, or lies over the
    /// frame's face a lap past it (ADR 0240).
    pub const fn with_mount(self, mount: FillMount) -> Self {
        Self { mount, ..self }
    }
}

/// How a filled board sits against the frame the void is cut in.
///
/// A mounting is an operation rather than a stored form (the owner, 2026-11-24): an inset board fills
/// the opening, and an overlay board lies one thickness in front of it with its ends running past the
/// opening. Making a finished front the other way is a later operation that moves it in depth and
/// rewrites its end trims — the plan does not store which of the two a part is, its trims and its
/// placement say it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FillMount {
    /// The board fills the opening: its inner face is the void's own start plane. Today's fill.
    #[default]
    Inset,
    /// The board lies over the frame's face, running past the opening where the frame allows it.
    Overlay {
        /// The lap each end asks for, in outline-edge order, before the frame says what it may take.
        ///
        /// A creation asks every end for the workshop's number; a part re-planned from the void asks
        /// for the ends it already has, so a frame that lost a neighbour does not silently widen a
        /// front the person sized.
        laps: [Length; 4],
    },
}

impl FillMount {
    /// The lap each end asks for, in outline-edge order: zeros for a board inside the opening.
    pub const fn laps(self) -> [Length; 4] {
        match self {
            Self::Inset => [Length::ZERO; 4],
            Self::Overlay { laps } => laps,
        }
    }

    /// Whether the board lies over the frame's face rather than filling the opening.
    pub const fn is_overlay(self) -> bool {
        matches!(self, Self::Overlay { .. })
    }

    /// Whether the board fills the opening, which is the ordinary fill an absent parameter means.
    pub const fn is_inset(&self) -> bool {
        matches!(self, Self::Inset)
    }
}

/// How a fill tool reads the section the walls leave.
///
/// It is a wire word as well as a tool's mode: a command that fills a niche names it, so a caller
/// that cannot see the void asks for the largest rectangle or for the shape the walls leave.
#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FillMode {
    /// The largest rectangle that fits inside the section, keeping the standard rectangle panel.
    #[default]
    Fit,
    /// The section itself: the part takes the whole shape the walls allow.
    Shape,
}

/// The part a fill click would create.
///
/// `outline`, `nominal_size` and `placement` are exactly what `panel.create` stores, so a click
/// writes the plan rather than a second measurement of it.
#[derive(Clone, Debug, PartialEq)]
pub struct FillPlan {
    /// The open product the part lands in.
    pub product: Id,
    /// The outline the panel is born with: the nominal rectangle, a rectangle with corner
    /// treatments, or the measured ring.
    pub outline: PanelOutline,
    /// The panel's nominal length and width.
    pub nominal_size: PanelNominalSize,
    /// The panel's placement in the parent frame.
    pub placement: Placement,
    /// The void the part was measured against.
    pub niche: Niche,
    /// The plan's base-face polygon in panel-local millimetres, counter-clockwise.
    ///
    /// It is the shape the ghost draws: the inscribed rectangle in [`FillMode::Fit`], the section itself
    /// in [`FillMode::Shape`] — and where the frame's section carries an arc, the same ring sampled at
    /// [`FILL_SAMPLE_MM`], because a ghost is a wireframe and the panel keeps the arc.
    pub corners: Vec<DVec2>,
    /// The plan's far-face polygon, in the same frame and the same order as [`FillPlan::corners`].
    ///
    /// The base ring with every beveled edge set back by its wall's own angle, so the ghost draws
    /// the rim the finished panel ends at rather than the base rim lifted by the thickness.
    pub far_corners: Vec<DVec2>,
    /// The end trims the board was born with, in outline-edge order (ADR 0240).
    ///
    /// All zeros for a board that fills the opening; an overlay front carries the negative of the lap
    /// each end took, so the card reads back the number the plan measured and the frame cannot lie
    /// about how far the board runs.
    pub trims: PanelTrims,
    /// The end bevels the walls ask for, in ring-segment order.
    ///
    /// Empty when every wall meets the board square, which keeps [`FillMode::Shape`] on the standard
    /// panel whenever the section is one a rectangle draws. A beveled part is always a contour.
    pub bevels: Vec<EdgeBevel>,
    /// The smaller end of the part along the free axis, in the parent frame and millimetres.
    pub free_low: f64,
    /// The larger end of the part along the free axis, in the parent frame and millimetres.
    pub free_high: f64,
}

/// Why a point has no fillable niche.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FillRefusal {
    /// The point is outside every product, or is not a usable number.
    Outside,
    /// The free space around the point is not one convex cell.
    Unrepresentable,
    /// The open product's own footprint is not a convex ring, so the space it reserves is not one cell.
    Footprint,
    /// The void belongs to another product than the one the part is for.
    OtherProduct,
    /// Neither the product nor the measured point has a usable frame.
    NoFrame,
    /// The void is thinner than the material along the part's own free axis.
    TooThin,
    /// The void leaves no rectangle, or no ring, large enough to be a panel.
    TooSmall,
    /// The walls tilt against each other, so no one flat board meets them all.
    ///
    /// `Shape` falls back to the slab section without bevels, so [`Project::fill_plan`] does not
    /// answer this today; the variant is the classifier's own refusal and stays for a caller that
    /// asks a bevel question directly (ADR 0109).
    WallsDisagree,
    /// A wall lies so nearly level with the board that no end bevel can meet it.
    ///
    /// `Shape` falls back to the slab section, as above; the classifier's own refusal.
    Wedge,
    /// Only a board standing in the frontal plane can lie over the frame's face.
    NotFrontal,
    /// A part stands proud of the frame's face inside the opening, where the board would bury it.
    InTheWay,
}

impl std::fmt::Display for FillRefusal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.sentence())
    }
}

impl std::error::Error for FillRefusal {}

impl FillRefusal {
    /// The sentence the refusal is read by, in the wire's English.
    ///
    /// One wording serves the ghost a maker sees and the command a caller dispatches, so the two
    /// cannot explain one measured void differently.
    pub const fn sentence(self) -> &'static str {
        match self {
            Self::Outside => "Point at the empty space inside the open product.",
            Self::Unrepresentable => "The empty space here is not one straight shape; point at a plainer gap.",
            Self::Footprint => "The open piece's frame is not convex, so no one part takes its shape.",
            Self::OtherProduct => "The void belongs to another product than the one this part is for.",
            Self::NoFrame => "The open product has no frame.",
            Self::TooThin => "The void is thinner than the material.",
            Self::TooSmall => "The void is too small for a panel.",
            Self::WallsDisagree => {
                "The walls here lean against each other, so no flat board meets them all; use the largest fit."
            }
            Self::Wedge => {
                "The void narrows across the board's thickness, where no end bevel helps; use the largest fit."
            }
            Self::NotFrontal => "Only a front standing across the void can lie over the frame's face.",
            Self::InTheWay => {
                "A part stands proud of the frame's face inside the opening; the front has nowhere to lie."
            }
        }
    }
}

impl Project {
    /// Answers the part one click at `point` would create in `product`.
    ///
    /// `fill` is the class's own axis pair, `thickness` the material's thickness, `mode` how the
    /// section is read and `free` the free-axis position the tool chose — `None` while the point
    /// under the pointer picks it in whole millimetres, `Some` when a typed distance or the hand
    /// fixed it (ADR 0103). The result is the whole plan, so the caller never measures the void a
    /// second time.
    pub fn fill_plan(
        &self,
        product: Id,
        request: FillRequest,
        sight: Sight,
        contact: Option<&dyn Contact>,
    ) -> Result<FillPlan, FillRefusal> {
        let niche = self.niche(request.point, sight, contact).map_err(|refusal| match refusal {
            NicheRefusal::Outside => FillRefusal::Outside,
            NicheRefusal::Unrepresentable => FillRefusal::Unrepresentable,
            NicheRefusal::Footprint => FillRefusal::Footprint,
        })?;
        self.fill_plan_in(&niche, product, request, sight)
    }

    /// Answers the part one click at `point` would create, in a niche already measured.
    ///
    /// This is the one plan behind [`Project::fill_plan`]: a caller that holds a [`Niche`] — the
    /// niche atom, which measures the void once for every tool that consumes it (ADR 0118) — passes
    /// it here, so no surface probes the document a second time for the same answer.
    pub fn fill_plan_in(
        &self,
        niche: &Niche,
        product: Id,
        request: FillRequest,
        sight: Sight,
    ) -> Result<FillPlan, FillRefusal> {
        let FillRequest { point, fill, mode, thickness, free, mount } = request;
        if niche.product != product {
            return Err(FillRefusal::OtherProduct);
        }
        let parent_world = self.world_placement(product).ok_or(FillRefusal::NoFrame)?;
        let inverse = parent_world.inverse().map_err(|_| FillRefusal::NoFrame)?;
        let local = inverse.transform_point(point);
        let gap = thickness.millimetres_f64();
        if !local.is_finite()
            || !niche.size.is_finite()
            || !niche.local_min.is_finite()
            || !gap.is_finite()
            || gap <= 0.0
        {
            return Err(FillRefusal::TooSmall);
        }
        // A part the sight switched off is not a wall of the void the eye measures, but it is there
        // for the material: the plan stops the part at its faces (X73, ADR 0175). An overlay board
        // reads every part instead, because the frame it lies over is material whether the sight
        // draws it or not (ADR 0240).
        let hidden = self.hidden_boxes(parent_world, sight);
        let bodies = if mount.is_overlay() { self.part_boxes(parent_world) } else { Vec::new() };
        let (part, placement) = board(FillInput {
            niche,
            local,
            gap,
            mode,
            free,
            fill,
            thickness,
            hidden: &hidden,
            bodies: &bodies,
            mount,
        })?;
        // The free axis is the panel's own thickness axis, so the two ends are its base face and its
        // far face; which of them is lower depends on the stance (ADR 0105).
        let axis = free_axis(fill);
        let base = placement.translation()[axis];
        let far = base + placement.transform_vector(DVec3::Z)[axis] * gap;
        let (free_low, free_high) = if base <= far { (base, far) } else { (far, base) };
        if !free_low.is_finite() || !free_high.is_finite() {
            return Err(FillRefusal::NoFrame);
        }
        Ok(FillPlan {
            product,
            outline: part.outline,
            nominal_size: part.nominal_size,
            placement,
            niche: niche.clone(),
            corners: part.corners,
            far_corners: part.far_corners,
            trims: part.trims,
            bevels: part.bevels,
            free_low,
            free_high,
        })
    }
}

/// The axis of the open product's frame a fill's free position runs along.
///
/// A shelf's height and a post's place across the void both lie on the panel's own thickness axis.
pub const fn free_axis(fill: Fill) -> usize {
    match fill {
        Fill::WidthAndDepth => 2,
        Fill::HeightAndDepth => 0,
        Fill::WidthAndHeight => 1,
    }
}

/// What one row of boards asks: where the first board is, how far the hand travelled and what the
/// maker typed.
///
/// The four travel together because they are one gesture (AGENTS §1.5): the near face the first
/// click named, the span the hand pulled along the free axis, the count a field holds, and the far
/// gap a typed distance fixed. A row plan, its refusal and the ghosts a maker sees all read this
/// value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FillRowAsk {
    /// The first board's near face along the free axis, in the parent frame and millimetres.
    pub start: f64,
    /// How far the hand travelled from `start`; `None` while the row takes the whole void.
    pub span: Option<f64>,
    /// The count a field holds, clamped to the boards that fit without crossing.
    pub count: Option<usize>,
    /// The distance the maker fixed from the last board's far face to the wall, in millimetres.
    ///
    /// `None` while the row shares its free length out and leaves the leftover at the far end.
    pub gap: Option<f64>,
}

/// The row a fill click would lay: the board a [`FillPlan`] answers, repeated.
///
/// The numbers are the whole row: the count, the pitch the boards repeat at, and the two gaps the
/// plate reads. A caller draws the ghosts and writes the batch from this one answer, so the picture
/// and the history cannot disagree (N10, U125).
#[derive(Clone, Debug, PartialEq)]
pub struct FillRowPlan {
    /// How many boards the row holds, never fewer than one and never more than fit without crossing.
    pub count: usize,
    /// The distance from one board's near face to the next board's near face, in millimetres.
    pub pitch: f64,
    /// The gap between two neighbours, in millimetres.
    pub gap: f64,
    /// The first board's near face, in the parent frame and millimetres.
    pub start: f64,
    /// The last board's far face, the same frame.
    pub end: f64,
    /// The distance from the near wall to the first board's near face.
    pub low: f64,
    /// The distance from the last board's far face to the far wall: the leftover the row leaves.
    pub high: f64,
}

/// Why a row of boards cannot be laid in a void.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FillRowRefusal {
    /// The void has no room along the part's own free axis, or the ask is not a usable number.
    NoRoom,
    /// The pinned gap leaves less than one board between itself and the first board.
    GapTooWide,
    /// The row's own numbers do not agree: one board has no neighbour gap to share the space with.
    NotDivided,
}

impl std::fmt::Display for FillRowRefusal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.sentence())
    }
}

impl std::error::Error for FillRowRefusal {}

impl FillRowRefusal {
    /// The sentence the refusal is read by, in the wire's English.
    ///
    /// One wording serves the ghosts a maker sees and the caller that asks the same question, as
    /// [`FillRefusal::sentence`] does for one board.
    pub const fn sentence(self) -> &'static str {
        match self {
            Self::NoRoom => "The void has no room for a row of boards along this axis.",
            Self::GapTooWide => "The fixed gap is wider than the space the row has.",
            Self::NotDivided => "One board has no neighbour to share the space with; fix no gap or add boards.",
        }
    }
}

/// The row `thickness`-thick boards repeat into, along `axis` of `niche`.
///
/// This is the arithmetic of the gesture the owner asked for (U125): the first board is where the
/// click put it, the count is what the hand's span holds or what a field typed, and the free length
/// is shared between the boards and the far end, so the leftover falls there. A fixed far gap is
/// honoured instead of shared out, and the neighbours take what is left between them.
///
/// The void and the board are separate arguments because a ghost holds both already: the anchor's
/// [`FillGhost`](crate::FillGhost) reads its cell from the niche atom and its thickness from its own
/// third extent, so the shell asks this function with the numbers it has (ADR 0118).
pub fn fill_row(niche: &Niche, thickness: f64, axis: usize, ask: FillRowAsk) -> Result<FillRowPlan, FillRowRefusal> {
    if axis >= 3
        || !thickness.is_finite()
        || thickness <= 0.0
        || !ask.start.is_finite()
        || ask.span.is_some_and(|span| !span.is_finite())
        || ask.gap.is_some_and(|gap| !gap.is_finite() || gap < 0.0)
    {
        return Err(FillRowRefusal::NoRoom);
    }
    let wall_low = niche.local_min[axis];
    let wall_high = wall_low + niche.size[axis];
    let room = wall_high - ask.start;
    if !room.is_finite() || room < thickness {
        return Err(FillRowRefusal::NoRoom);
    }
    // The boards that fit without crossing: each takes one thickness, and the last one's far face
    // may touch the wall.
    let fits = |length: f64| {
        if length < thickness { 1 } else { (length / thickness + ROW_TOLERANCE).floor().max(1.0) as usize }
    };
    // A typed count claims the whole void; the hand's span claims only what it pulled, and the rest
    // of the length is the leftover the row leaves at the far end.
    let (span, count) = match ask.count {
        Some(count) => (room, count.clamp(1, fits(room))),
        None => {
            let span = ask.span.map_or(room, |span| span.clamp(thickness, room));
            (span, fits(span))
        }
    };
    let boards = count as f64 * thickness;
    let neighbours = count as f64 - 1.0;
    let gap = match ask.gap {
        // The free length is shared between the boards and the far end of the span: the last share
        // is the leftover, exactly as a row of shelves hangs in a case.
        None => (span - boards) / count as f64,
        Some(gap) => {
            if neighbours < 1.0 {
                // One board has no neighbour to divide the space with: the fixed gap must be the
                // one the span leaves, or the row cannot honour it.
                if (span - boards - gap).abs() > ROW_TOLERANCE {
                    return Err(FillRowRefusal::NotDivided);
                }
                0.0
            } else {
                let shared = (span - boards - gap) / neighbours;
                if shared < -ROW_TOLERANCE {
                    return Err(FillRowRefusal::GapTooWide);
                }
                shared.max(0.0)
            }
        }
    };
    let end = ask.start + boards + neighbours * gap;
    let row = FillRowPlan {
        count,
        pitch: thickness + gap,
        gap,
        start: ask.start,
        end,
        low: ask.start - wall_low,
        high: wall_high - end,
    };
    if !row.pitch.is_finite()
        || !row.end.is_finite()
        || !row.low.is_finite()
        || !row.high.is_finite()
        || row.low < -ROW_TOLERANCE
        || row.high < -ROW_TOLERANCE
    {
        return Err(FillRowRefusal::NoRoom);
    }
    Ok(row)
}

impl FillPlan {
    /// The row this board repeats into, along `axis`.
    ///
    /// The arithmetic of the gesture the owner asked for (U125): the first board is where the click
    /// put it, the count is what the hand's span holds or what a field typed, and the free length is
    /// shared between the boards and the far end, so the leftover falls there. A fixed far gap is
    /// honoured instead of shared out, and the neighbours take what is left between them.
    ///
    /// `axis` is [`free_axis`] of the class the plan was made for, so a shelf's row climbs, a post's
    /// runs across the void and a wall's runs in depth.
    pub fn row(&self, axis: usize, ask: FillRowAsk) -> Result<FillRowPlan, FillRowRefusal> {
        fill_row(&self.niche, self.free_high - self.free_low, axis, ask)
    }
}

/// How far a row's own arithmetic may sit off the void it fills, in millimetres.
///
/// The row's numbers are sums and divisions of millimetre coordinates, so a chain of them drifts by
/// a fraction of a micrometre; the tolerance keeps a row that exactly fills its void from being
/// refused as one that overshoots it by a rounding error.
const ROW_TOLERANCE: f64 = 1.0e-6;

/// The outline, size and base polygon one footprint becomes.
struct Part {
    outline: PanelOutline,
    nominal_size: PanelNominalSize,
    /// The base-face polygon in panel-local millimetres.
    corners: Vec<DVec2>,
    /// The far-face polygon in the same frame and order.
    far_corners: Vec<DVec2>,
    /// The end bevels, one per beveled ring segment.
    bevels: Vec<EdgeBevel>,
    /// The end trims the board carries, in outline-edge order.
    trims: PanelTrims,
}

/// How a fill's board sits in the product frame: which product axis each local axis runs along.
///
/// The shelf is the flat stance, the post is the shelf turned a quarter-turn to the right about `+Y`
/// and the wall is the post turned a quarter-turn to the right about `+Z` (ADR 0105), so the board's
/// own axes are a permutation of the product's with `+1` or `-1` on local `Y` and `Z`. A fill may
/// turn a board half a turn about its own length axis, which negates both (ADR 0108), and that is
/// the whole vocabulary this needs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Stance {
    /// The product axis the board's local `X` runs along.
    u: usize,
    /// The product axis the board's local `Y` runs along.
    v: usize,
    /// The product axis the board's local `Z` (its thickness) runs along.
    t: usize,
    /// Whether the board's local `Y` runs against the product's `v` axis.
    against_y: bool,
    /// Whether the board's local `Z` runs against the product's `t` axis.
    against_z: bool,
    /// Whether the board was turned about its length axis to take the other wide face.
    other_face: bool,
    /// The turn the stance applies before any reference-face turn.
    turn: DQuat,
}

impl Stance {
    /// The same axes seen from the other large face: the board turned about its own length axis.
    const fn turned(self) -> Self {
        Self { against_y: !self.against_y, against_z: !self.against_z, other_face: !self.other_face, ..self }
    }

    /// The board's rotation in the product frame.
    pub(crate) fn rotation(self) -> DQuat {
        if self.other_face { self.turn * DQuat::from_rotation_x(PI) } else { self.turn }
    }

    /// The product coordinate of the board's base face for a slab starting at `value`.
    fn face(self, value: f64, gap: f64) -> f64 {
        if self.against_z { value + gap } else { value }
    }

    /// The tilt of a wall's `t` normal in the board's own frame.
    fn along(self, normal: f64) -> f64 {
        if self.against_z { -normal } else { normal }
    }
}

/// The stance a fill's board takes before the walls decide which face is the wide one.
pub(crate) fn stance(fill: Fill) -> Stance {
    match fill {
        Fill::WidthAndDepth => {
            Stance { u: 0, v: 1, t: 2, against_y: false, against_z: false, other_face: false, turn: DQuat::IDENTITY }
        }
        Fill::HeightAndDepth => Stance {
            u: 2,
            v: 1,
            t: 0,
            against_y: false,
            against_z: true,
            other_face: false,
            turn: DQuat::from_rotation_y(-FRAC_PI_2),
        },
        // The wall is the post turned a quarter-turn to the right about `+Z`, so its length runs
        // up, its width across the front and its material towards `+Y`, face `A` pointing at the
        // maker (ADR 0105). Its free axis is the product's depth, which is where the pointer sits.
        Fill::WidthAndHeight => Stance {
            u: 2,
            v: 0,
            t: 1,
            against_y: false,
            against_z: false,
            other_face: false,
            turn: DQuat::from_rotation_z(-FRAC_PI_2) * DQuat::from_rotation_y(-FRAC_PI_2),
        },
    }
}

/// The rotation a fill's stance gives a board, for a caller that builds its own placement.
///
/// The typed create ([`crate::PanelType`]) reaches the same table through this, so a board made by
/// a command and a board made by a fill stand in one frame (X117).
pub(crate) fn stance_rotation(fill: Fill) -> DQuat {
    stance(fill).rotation()
}

/// One fill plan's inputs: the measurement, the tool's choice and the parts the sight switched off.
///
/// The eight travel together because they are one reading of one gesture (AGENTS §1.5): the cell the
/// niche atom measured, the point inside it, the material's gap, the mode, the free-axis place, the
/// class's axis pair, the thickness and the boxes a hidden body stands in the way with.
struct FillInput<'a> {
    niche: &'a Niche,
    local: DVec3,
    gap: f64,
    mode: FillMode,
    free: Option<f64>,
    fill: Fill,
    thickness: Length,
    hidden: &'a [HiddenBox],
    /// Every body's box in the product frame: what an overlay board reads the frame from.
    bodies: &'a [HiddenBox],
    mount: FillMount,
}

/// The board a click would create in `fill`'s stance: the part and its placement.
///
/// `value` is the free-axis position, which is the slab's low end in the product frame; the board
/// occupies one thickness from there. [`FillMode::Fit`] reads the section the slab takes at both
/// ends, so it fits whichever way the walls lean; [`FillMode::Shape`] reads the wide face and
/// bevels every edge whose wall tilts.
fn board(input: FillInput<'_>) -> Result<(Part, Placement), FillRefusal> {
    let FillInput { niche, local, gap, mode, free, fill, thickness, hidden, bodies, mount } = input;
    let stance = stance(fill);
    if mount.is_overlay() {
        // A board over the frame's face is a frontal part by construction (ADR 0240): its ends run
        // past the opening in the plane the void opens onto, and no other stance has such a plane.
        if fill != Fill::WidthAndHeight {
            return Err(FillRefusal::NotFrontal);
        }
        return overlay_board(OverlayInput {
            niche,
            stance,
            gap,
            asked: mount.laps().map(Length::millimetres_f64),
            bodies,
        });
    }
    let low = niche.local_min[stance.t];
    let high = niche.local_max()[stance.t] - gap;
    if high < low {
        return Err(FillRefusal::TooThin);
    }
    // A typed number is whole millimetres already and the hand's place is not: one rule measures
    // both, so the plan never stores a fraction the document cannot hold (ADR 0103).
    let value = follow(free.unwrap_or(local[stance.t]), low, high);
    // A hidden body is a boundary of the part and not a slice out of it: the footprint is cut back at
    // the body's face in both modes, so the part stops where the material starts (X73, ADR 0175).
    // The cuts are handed back with the polygon so an exact outline takes the same ones (ADR 0204).
    let trim = |polygon: Vec<DVec2>, stance: &Stance| -> Result<(Vec<DVec2>, Vec<Cut>), FillRefusal> {
        trim_footprint(polygon, hidden, stance, local, (value, value + gap))
    };
    let (stance, polygon, angles, cuts) = match mode {
        FillMode::Fit => {
            let footprint = fit_footprint(niche, &stance, (value, value + gap)).ok_or(FillRefusal::TooSmall)?;
            let (polygon, cuts) = trim(footprint, &stance)?;
            (stance, polygon, Vec::new(), cuts)
        }
        FillMode::Shape => match reference_face(niche, value, gap, stance) {
            Ok(stance) => {
                let face = stance.face(value, gap);
                let section = niche.cut(stance.t, (face, face), (stance.u, stance.v)).ok_or(FillRefusal::TooSmall)?;
                let (section, cuts) = trim(section, &stance)?;
                let angles = wall_bevels(niche, &section, face, &stance);
                (stance, section, angles, cuts)
            }
            // The wall planes here cannot all be met by one flat end face: two walls lean against
            // each other, or a wall lies nearly level with the board. The shape is still useful and
            // still fits if it takes the section the whole slab leaves — the same cut Fit reads —
            // and no end bevel, so the maker gets the shape of the niche with a sliver at a wall
            // instead of a refusal (ADR 0109).
            Err(FillRefusal::WallsDisagree | FillRefusal::Wedge) => {
                let section =
                    niche.cut(stance.t, (value, value + gap), (stance.u, stance.v)).ok_or(FillRefusal::TooSmall)?;
                let (section, cuts) = trim(section, &stance)?;
                (stance, section, Vec::new(), cuts)
            }
            Err(refusal) => return Err(refusal),
        },
    };
    let exact = exact_section(niche, mode, &stance, &cuts, DVec2::new(local[stance.u], local[stance.v]));
    let shape = PartShape {
        footprint: &polygon,
        trims: PanelTrims::ZERO,
        mode,
        stance: &stance,
        angles: &angles,
        thickness,
        exact,
        pointer: DVec2::new(local[stance.u], local[stance.v]),
    };
    let (part, anchor) = part_of(shape)?;
    // The panel's zero corner is the outline's own base corner, which the product frame answers from
    // the stance: the lower end of each axis the board runs with and the higher one where it runs
    // against, then the base face along the thickness.
    let mut translation = DVec3::ZERO;
    translation[stance.u] = anchor.x;
    translation[stance.v] = anchor.y;
    translation[stance.t] = stance.face(value, gap);
    let placement = Placement::new(translation, stance.rotation()).map_err(|_| FillRefusal::NoFrame)?;
    Ok((part, placement))
}

/// One overlay board's ask: the measured void, the stance it lies in and the frame's own parts.
struct OverlayInput<'a> {
    /// The cell the niche atom measured, in the product frame.
    niche: &'a Niche,
    /// The frontal stance the board lies in.
    stance: Stance,
    /// One material thickness.
    gap: f64,
    /// The lap each end asks for, millimetres, in outline-edge order.
    asked: [f64; 4],
    /// Every body's box in the product frame.
    bodies: &'a [HiddenBox],
}

/// What the frame answers an overlay board: where the opening's plane is and what stands there.
struct OverlayFrame<'a> {
    /// The plane the board's inner face lies on, along the thickness axis.
    start: f64,
    /// The lap each end asks for, millimetres, in outline-edge order.
    asked: [f64; 4],
    /// The frontal stance the board lies in.
    stance: Stance,
    /// Every body's box in the product frame.
    bodies: &'a [HiddenBox],
}

/// The board a click lays over the frame: the opening's rectangle, grown by the laps the frame
/// allows, one thickness in front of the void's start plane (ADR 0240).
///
/// The four numbers come from the frame, not from the settings alone. An end may run past the
/// opening only where a part stands at it to run over — the lap is that part's face measured from the
/// opening's own edge, never more than the workshop's number — and no part there is no lap, because a
/// board whose end had nothing behind it would hang in the air. A part standing proud of the face is
/// the other answer: the board stops at its near face and never runs over it, which is the rule the
/// maker reads as "it does not ride onto the neighbour".
fn overlay_board(input: OverlayInput<'_>) -> Result<(Part, Placement), FillRefusal> {
    let OverlayInput { niche, stance, gap, asked, bodies } = input;
    if !(gap.is_finite() && gap > 0.0 && asked.iter().all(|lap| lap.is_finite() && *lap >= 0.0)) {
        return Err(FillRefusal::TooSmall);
    }
    let start = niche.local_min[stance.t];
    // The opening where the board's own inner face lies: the widest the void is on the plane it
    // covers, taken as the rectangle a facade is. Both builds answer it, because a front covers the
    // opening rather than taking the shape a wall leaves inside it.
    let opening = niche.cut(stance.t, (start, start), (stance.u, stance.v)).ok_or(FillRefusal::TooSmall)?;
    let (low, high) = bounds_of(&opening)?;
    let frame = OverlayFrame { start, asked, stance, bodies };
    let (grown_low, grown_high) = overlay_rect(&frame, low, high)?;
    // A part proud of the face *inside* the opening is material the board would bury, and no end
    // stops it: the click is refused instead of cutting through it.
    for body in bodies {
        if body.min[stance.t] >= start - TRIM_TOLERANCE {
            continue;
        }
        let inside_u =
            body.min[stance.u] > grown_low.x + TRIM_TOLERANCE && body.max[stance.u] < grown_high.x - TRIM_TOLERANCE;
        let inside_v =
            body.min[stance.v] > grown_low.y + TRIM_TOLERANCE && body.max[stance.v] < grown_high.y - TRIM_TOLERANCE;
        if inside_u && inside_v {
            return Err(FillRefusal::InTheWay);
        }
    }
    let trims = overlay_trims([grown_low.y, grown_high.x, grown_high.y, grown_low.x], low, high)?;
    // The panel's frame is the opening's rectangle and the material is what the trims make of it
    // (ADR 0240): the frame's corner is where the part is placed, so the person controls the
    // position by the opening and never by a lap.
    let size = PanelNominalSize::new(
        length_of(high.x - low.x).ok_or(FillRefusal::TooSmall)?,
        length_of(high.y - low.y).ok_or(FillRefusal::TooSmall)?,
    )
    .map_err(|_| FillRefusal::TooSmall)?;
    let local = |point: DVec2| DVec2::new(point.x - low.x, point.y - low.y);
    let corners = vec![
        local(grown_low),
        DVec2::new(grown_high.x - low.x, grown_low.y - low.y),
        local(grown_high),
        DVec2::new(grown_low.x - low.x, grown_high.y - low.y),
    ];
    if corners.iter().any(|corner| !corner.is_finite()) {
        return Err(FillRefusal::TooSmall);
    }
    let part = Part {
        outline: PanelOutline::Rect { corner_features: Vec::new(), edge_bows: Vec::new() },
        nominal_size: size,
        corners: corners.clone(),
        far_corners: corners,
        bevels: Vec::new(),
        trims,
    };
    // The board's body runs one thickness from the plane its inner face lies on, so the panel's own
    // base face is the front one and the stance decides which end that is in the product frame.
    let mut translation = DVec3::ZERO;
    translation[stance.u] = low.x;
    translation[stance.v] = low.y;
    translation[stance.t] = stance.face(start - gap, gap);
    let placement = Placement::new(translation, stance.rotation()).map_err(|_| FillRefusal::NoFrame)?;
    Ok((part, placement))
}

/// The rectangle an overlay board takes, in the board's own two axes and the product frame.
///
/// The four ends in outline-edge order — 1 the end at zero width, 2 the end at the full length, 3 the
/// end at the full width, 4 the end at zero length, the numbering the card prints. An end starts on
/// the opening's own edge and runs outward only where the frame gives it something to run over: a
/// part that reaches the opening's plane from behind answers the lap its own face gives, capped by
/// the workshop's number. A part standing proud of the plane is the other answer — the end stops at
/// its near face, which trims the board back inside the opening when the neighbour reaches over it
/// (ADR 0240). The two are read together: an end is stopped only by a part that lies beside the board
/// as it is now, so a neighbour at one end never bites into another.
fn overlay_rect(frame: &OverlayFrame<'_>, low: DVec2, high: DVec2) -> Result<(DVec2, DVec2), FillRefusal> {
    let stance = frame.stance;
    // Edge, the axis it bounds, which way that end lies, the opening's edge on it and the other axis.
    let ends: [(usize, usize, f64, f64, usize); 4] = [
        (0, stance.v, -1.0, low.y, stance.u),
        (1, stance.u, 1.0, high.x, stance.v),
        (2, stance.v, 1.0, high.y, stance.u),
        (3, stance.u, -1.0, low.x, stance.v),
    ];
    let spans = [(low.x, high.x), (low.y, high.y)];
    let mut places = [low.y, high.x, high.y, low.x];
    for (index, (_, axis, outward, edge_at, across)) in ends.into_iter().enumerate() {
        // A part reaching behind the plane at this end gives the board a face to run over.
        let (span_lo, span_hi) = spans[if across == stance.u { 0 } else { 1 }];
        let mut reach: Option<f64> = None;
        for body in frame.bodies {
            if body.max[stance.t] <= frame.start + TRIM_TOLERANCE {
                continue;
            }
            if body.max[across] <= span_lo + TRIM_TOLERANCE || body.min[across] >= span_hi - TRIM_TOLERANCE {
                continue;
            }
            let extent = if outward > 0.0 {
                (body.max[axis] > edge_at + TRIM_TOLERANCE && body.min[axis] <= edge_at + TRIM_TOLERANCE)
                    .then_some(body.max[axis] - edge_at)
            } else {
                (body.min[axis] < edge_at - TRIM_TOLERANCE && body.max[axis] >= edge_at - TRIM_TOLERANCE)
                    .then_some(edge_at - body.min[axis])
            };
            if let Some(extent) = extent.filter(|extent| extent.is_finite() && *extent > 0.0) {
                reach = Some(reach.map_or(extent, |near: f64| near.min(extent)));
            }
        }
        let asked = *frame.asked.get(index).unwrap_or(&0.0);
        places[index] = edge_at + outward * reach.map_or(0.0, |reach| asked.min(reach));
    }
    // A part standing proud of the plane is what an end stops at, and one end's neighbour is read
    // against the other end as it stands after the last pass, so the two settle instead of arguing.
    for _ in 0..OVERLAY_CLIP_PASSES {
        let (u_lo, u_hi, v_lo, v_hi) = (places[3], places[1], places[0], places[2]);
        let mut moved = false;
        for (index, (_, axis, outward, edge_at, across)) in ends.into_iter().enumerate() {
            let (span_lo, span_hi) = if across == stance.u { (u_lo, u_hi) } else { (v_lo, v_hi) };
            let Some(mut place) = places.get(index).copied() else {
                continue;
            };
            for body in frame.bodies {
                if body.min[stance.t] >= frame.start - TRIM_TOLERANCE {
                    continue;
                }
                if body.max[across] <= span_lo + TRIM_TOLERANCE || body.min[across] >= span_hi - TRIM_TOLERANCE {
                    continue;
                }
                let stopped = if outward > 0.0 {
                    if body.max[axis] > edge_at + TRIM_TOLERANCE && body.min[axis] < place {
                        body.min[axis]
                    } else {
                        place
                    }
                } else if body.min[axis] < edge_at - TRIM_TOLERANCE && body.max[axis] > place {
                    body.max[axis]
                } else {
                    place
                };
                if stopped == place {
                    continue;
                }
                // A stop that would take the whole board is the other end's neighbour seen from this
                // side: it is read again once that end has settled, so this pass leaves it alone.
                let mut candidate = places;
                if let Some(slot) = candidate.get_mut(index) {
                    *slot = stopped;
                }
                let (next_u, next_v) = (candidate[1] - candidate[3], candidate[2] - candidate[0]);
                if !(next_u > TRIM_TOLERANCE && next_v > TRIM_TOLERANCE) {
                    continue;
                }
                place = stopped;
                moved = true;
                if let Some(slot) = places.get_mut(index) {
                    *slot = place;
                }
            }
        }
        if !moved {
            break;
        }
    }
    if !(places[1] - places[3] > TRIM_TOLERANCE && places[2] - places[0] > TRIM_TOLERANCE) {
        return Err(FillRefusal::InTheWay);
    }
    Ok((DVec2::new(places[3], places[0]), DVec2::new(places[1], places[2])))
}

/// The trim each end of an overlay board carries, from where the frame put it.
fn overlay_trims(places: [f64; 4], low: DVec2, high: DVec2) -> Result<PanelTrims, FillRefusal> {
    // The lap an end took is how far it lies outside the opening, positive outward in every case.
    let lap = |place: f64, edge_at: f64, outward: f64| outward * (place - edge_at);
    let laps = [
        lap(places[0], low.y, -1.0),
        lap(places[1], high.x, 1.0),
        lap(places[2], high.y, 1.0),
        lap(places[3], low.x, -1.0),
    ];
    let trim = |lap: f64| document_length(-lap).ok_or(FillRefusal::TooSmall);
    PanelTrims::new(trim(laps[0])?, trim(laps[1])?, trim(laps[2])?, trim(laps[3])?).map_err(|_| FillRefusal::TooSmall)
}

/// The box a polygon spans, or `None` when it has no usable corner.
fn bounds_of(points: &[DVec2]) -> Result<(DVec2, DVec2), FillRefusal> {
    let first = points.first().copied().ok_or(FillRefusal::TooSmall)?;
    if points.iter().any(|point| !point.is_finite()) {
        return Err(FillRefusal::TooSmall);
    }
    Ok(points.iter().copied().fold((first, first), |(low, high), point| (low.min(point), high.max(point))))
}

/// The polygon a fit searches: the frame's whole reserved shape when the stance is a shelf's.
///
/// A shelf's own plane is the plan, and the plan's free shape is the ring the frame draws, whole
/// (ADR 0204). The cell's `cut` is the convex piece the planes describe, which is the right reading for
/// a board standing across the void and a smaller one for a shelf: so a fit takes the section the frame
/// reserves, and falls back to the cell where the frame is a plain box.
fn fit_footprint(niche: &Niche, stance: &Stance, slab: (f64, f64)) -> Option<Vec<DVec2>> {
    let plan = stance.u == 0 && stance.v == 1 && stance.t == 2 && !stance.other_face;
    if plan && let Some(polygon) = niche.section_polygon() {
        return Some(polygon);
    }
    niche.cut(stance.t, slab, (stance.u, stance.v))
}

/// The frame's own section a part is cut to, when the cell can answer one.
///
/// A cell seeded from a shaped frame keeps that frame's ring exactly (ADR 0201, N33, ADR 0204), and a
/// prism's section is the same at every height, so a shelf takes the ring itself instead of the chords
/// of it. Two conditions hold it back: the part must be the shape rather than the largest fit, and its
/// own plane must *be* the section plane and keep the panel's own axes — a post or a wall stands across
/// the cell, where the section is a projection of the ring and not the ring.
///
/// A hidden body trims the ring by the half-planes the polygon trim chose, not by a polygon rule, so a
/// part that stops at a hidden part still keeps the frame's own arc (ADR 0204, X73, ADR 0175).
fn exact_section(niche: &Niche, mode: FillMode, stance: &Stance, cuts: &[Cut], point: DVec2) -> Option<Contour> {
    let flat = stance.u == 0 && stance.v == 1 && stance.t == 2;
    let square = !stance.against_y && !stance.against_z && !stance.other_face;
    if mode != FillMode::Shape || !flat || !square {
        return None;
    }
    let section = niche.section()?;
    if cuts.is_empty() {
        return Some(section.clone());
    }
    let ring = section.to_geometry().ok()?;
    let mut region = Region::new(ring, Vec::new()).ok()?;
    for cut in cuts {
        region = region.clip_half_plane_around(cut.normal, cut.offset, point).ok()??;
    }
    region_contour(&region)
}

/// The document ring a region's outer boundary draws, or `None` when it cannot be stored.
fn region_contour(region: &Region) -> Option<Contour> {
    let segments = region
        .outer()
        .segments()
        .iter()
        .map(|segment| ContourSegment::from_geom(*segment).ok())
        .collect::<Option<Vec<_>>>()?;
    Contour::new(segments).ok()
}

/// One half-plane a hidden body's face cuts away from the part: its normal and its offset.
#[derive(Clone, Copy, Debug)]
struct Cut {
    /// The normal of the kept side, in the board's own two axes.
    normal: DVec2,
    /// The offset of the face along that normal.
    offset: f64,
}

/// The stance whose base face is the wide one: the face a cut-only bevel can keep as the outline.
///
/// With `n` a wall's free-side normal in the board's own frame, face `B` answers
/// `n_xy . xy >= offset` and face `A` answers `n_xy . xy >= offset - n_z t`, so a wall with
/// `n_z < 0` narrows the board at `A` and leaves `B` the wide face. A cell whose walls all tilt the
/// other way is filled from `A` instead, which is the same board turned half a turn about its own
/// length axis: a placement, not a new stance (ADR 0108). Walls that disagree leave a void no flat
/// board fills, and so does a wall so nearly level with the board that its end face would be
/// steeper than a bevel may be.
fn reference_face(niche: &Niche, value: f64, gap: f64, stance: Stance) -> Result<Stance, FillRefusal> {
    let mut cuts = false;
    let mut grows = false;
    for plane in &niche.planes {
        let flat = plane.normal[stance.u].hypot(plane.normal[stance.v]);
        if flat < LEVEL_WALL {
            // A plane level with the board bounds no edge of the section; it only has to hold over
            // the whole slab, or the void is a wedge across the thickness.
            if [value, value + gap].into_iter().any(|end| plane.normal[stance.t] * end - plane.offset < -WALL_TOLERANCE)
            {
                return Err(FillRefusal::Wedge);
            }
            continue;
        }
        let Some(bevel) = wall_bevel(plane, &stance) else {
            continue;
        };
        if bevel >= MAX_FILL_BEVEL {
            return Err(FillRefusal::Wedge);
        }
        if stance.along(plane.normal[stance.t]) < 0.0 {
            cuts = true;
        } else {
            grows = true;
        }
    }
    match (cuts, grows) {
        (true, true) => Err(FillRefusal::WallsDisagree),
        (false, true) => Ok(stance.turned()),
        _ => Ok(stance),
    }
}

/// The bevel one wall asks for, in degrees, or `None` when it meets the board square.
///
/// A plane level with the board bounds no edge at all, and an angle that rounds to zero millidegrees
/// is not storable, so neither gets a record.
fn wall_bevel(plane: &NichePlane, stance: &Stance) -> Option<Degrees> {
    let flat = plane.normal[stance.u].hypot(plane.normal[stance.v]);
    if flat < LEVEL_WALL {
        return None;
    }
    let bevel = Degrees::from_radians((plane.normal[stance.t] / flat).abs().atan())?;
    (bevel.millidegrees() > 0).then_some(bevel)
}

/// The bevel each edge of the section asks for, in the section's own corner order.
///
/// An edge of a convex section lies on one of the cell's wall lines, so the wall that bounds it is
/// the plane whose flat normal faces the edge's outward side and whose line the edge's start point
/// sits on. The plane's normal points into the void and so back at the board, which is why it is the
/// edge's outward normal turned around. A wall level with the board bounds no edge, so it never
/// answers one.
fn wall_bevels(niche: &Niche, section: &[DVec2], face: f64, stance: &Stance) -> Vec<Option<Degrees>> {
    let mut bevels = Vec::with_capacity(section.len());
    for index in 0..section.len() {
        let (Some(start), Some(end)) = (section.get(index), section.get((index + 1) % section.len())) else {
            bevels.push(None);
            continue;
        };
        let direction = *end - *start;
        let length = direction.length();
        let Some(outward) = (length > 1.0e-9).then(|| DVec2::new(direction.y, -direction.x) / length) else {
            bevels.push(None);
            continue;
        };
        let mut found = None;
        for plane in &niche.planes {
            let Some(bevel) = wall_bevel(plane, stance) else {
                continue;
            };
            let flat = DVec2::new(plane.normal[stance.u], plane.normal[stance.v]);
            let width = flat.length();
            if width <= 0.0 || !width.is_finite() {
                continue;
            }
            let normal = flat / width;
            if normal.dot(outward) > -1.0 + 1.0e-6 {
                continue;
            }
            let offset = (plane.offset - plane.normal[stance.t] * face) / width;
            if (normal.dot(*start) - offset).abs() > WALL_TOLERANCE {
                continue;
            }
            found = Some(bevel);
            break;
        }
        bevels.push(found);
    }
    bevels
}

/// One part's own reading of the void: the footprint, the mode, the stance and what the frame adds.
struct PartShape<'a> {
    /// The free space in the board's own two axes, in the product frame.
    footprint: &'a [DVec2],
    /// The end trims the board carries: all zeros for a board that fills the opening.
    trims: PanelTrims,
    /// How the part takes it.
    mode: FillMode,
    /// The axes the board runs with.
    stance: &'a Stance,
    /// The bevel each edge of the footprint asks for, when a wall leans.
    angles: &'a [Option<Degrees>],
    /// The material's thickness.
    thickness: Length,
    /// The frame's own ring, when the part takes the shape rather than the largest fit.
    exact: Option<Contour>,
    /// The pointer's own place in the board's two axes: a fit goes where the hand points.
    pointer: DVec2,
}

/// The part a footprint becomes in `mode`, with the section point the panel is anchored at.
fn part_of(shape: PartShape<'_>) -> Result<(Part, DVec2), FillRefusal> {
    let PartShape { footprint, mode, stance, angles, thickness, exact, pointer, trims } = shape;
    let Some(first) = footprint.first().copied() else {
        return Err(FillRefusal::TooSmall);
    };
    if footprint.iter().any(|point| !point.is_finite()) {
        return Err(FillRefusal::TooSmall);
    }
    let low = footprint.iter().copied().fold(first, DVec2::min);
    let high = footprint.iter().copied().fold(first, DVec2::max);
    match mode {
        FillMode::Fit => {
            let (corner, size) = largest_rectangle_through(footprint, pointer)
                .or_else(|| largest_rectangle(footprint))
                .ok_or(FillRefusal::TooSmall)?;
            let nominal_size = PanelNominalSize::new(
                length_of(size.x).ok_or(FillRefusal::TooSmall)?,
                length_of(size.y).ok_or(FillRefusal::TooSmall)?,
            )
            .map_err(|_| FillRefusal::TooSmall)?;
            let corners =
                vec![DVec2::ZERO, DVec2::new(size.x, 0.0), DVec2::new(size.x, size.y), DVec2::new(0.0, size.y)];
            let outline = PanelOutline::Rect { corner_features: Vec::new(), edge_bows: Vec::new() };
            Ok((
                Part {
                    outline,
                    nominal_size,
                    corners: corners.clone(),
                    far_corners: corners,
                    bevels: Vec::new(),
                    trims,
                },
                corner,
            ))
        }
        FillMode::Shape if exact.is_some() => {
            let ring = exact.ok_or(FillRefusal::TooSmall)?;
            exact_part(&ring, stance, trims)
        }
        FillMode::Shape => {
            let ring = local_ring(footprint, angles, stance, low, high);
            let contour = contour_of(&ring.points)?;
            let nominal_size = Panel::contour_size(&contour).map_err(|_| FillRefusal::TooSmall)?;
            let mut bevels = Vec::new();
            for (index, bevel) in ring.bevels.iter().enumerate() {
                let Some(bevel) = *bevel else {
                    continue;
                };
                let Some(segment) = contour.id_at(index) else {
                    return Err(FillRefusal::TooSmall);
                };
                let record = EdgeBevel::new(EdgeRef::Ring { segment }, bevel).map_err(|_| FillRefusal::Wedge)?;
                bevels.push(record);
            }
            // A beveled board is a contour whatever its polygon looks like: the rectangle generator
            // has no place for an end bevel, so the classifier only serves the square cases.
            let outline = if bevels.is_empty() {
                classify_outline(&contour).unwrap_or(PanelOutline::Contour { contour })
            } else {
                PanelOutline::Contour { contour }
            };
            let far_corners = far_face(&outline, &bevels, &ring.points, thickness)?;
            let anchor = DVec2::new(low.x, if stance.against_y { high.y } else { low.y });
            Ok((Part { outline, nominal_size, corners: ring.points, far_corners, bevels, trims }, anchor))
        }
    }
}

/// The part whose outline is the frame's own ring: a curve of the frame stays a curve of the part.
///
/// The ring is moved to the panel's own frame once — the mirror the board's stance applies and the move
/// to the zero corner — as one rigid flat transform, so an arc stays an arc and the classifier can still
/// read the case's own radius. Nothing here bevels: a cell whose walls all stand plumb has no tilted
/// wall, so the far face is the base face. The polygon beside the outline is the same ring sampled, which
/// is the shape the ghost draws; the ring itself is what the panel stores (N33).
fn exact_part(section: &Contour, stance: &Stance, trims: PanelTrims) -> Result<(Part, DVec2), FillRefusal> {
    let ring = section.to_geometry().map_err(|_| FillRefusal::TooSmall)?;
    let (low, high) = ring.bounds().ok_or(FillRefusal::TooSmall)?;
    let anchor = DVec2::new(low.x, if stance.against_y { high.y } else { low.y });
    let offset = DVec2::new(-anchor.x, if stance.against_y { high.y } else { -anchor.y });
    let ring = ring.placed(0.0, stance.against_y, offset).map_err(|_| FillRefusal::TooSmall)?;
    let contour = document_ring(&ring).ok_or(FillRefusal::TooSmall)?;
    let nominal_size = Panel::contour_size(&contour).map_err(|_| FillRefusal::TooSmall)?;
    let outline = classify_outline(&contour).unwrap_or(PanelOutline::Contour { contour });
    let polygon = ring.sample(FILL_SAMPLE_MM).map_err(|_| FillRefusal::TooSmall)?;
    if polygon.len() < 3 {
        return Err(FillRefusal::TooSmall);
    }
    Ok((
        Part { outline, nominal_size, corners: polygon.clone(), far_corners: polygon, bevels: Vec::new(), trims },
        anchor,
    ))
}

/// A section in the panel's own plane, anchored at its zero corner, with the bevel on each edge.
struct LocalRing {
    /// The ring's corners, counter-clockwise from the panel's zero corner.
    points: Vec<DVec2>,
    /// The bevel the edge from `points[i]` to the next corner asks for, if any.
    bevels: Vec<Option<Degrees>>,
}

/// The section as the panel's own ring.
///
/// The board's own `Y` may run against the section's second axis, which mirrors the polygon and so
/// reverses its travel direction; the ring is turned back round, and each bevel travels with the
/// edge it belongs to so the two stay in step.
fn local_ring(footprint: &[DVec2], angles: &[Option<Degrees>], stance: &Stance, low: DVec2, high: DVec2) -> LocalRing {
    let mut points: Vec<DVec2> = footprint
        .iter()
        .map(|point| DVec2::new(point.x - low.x, if stance.against_y { high.y - point.y } else { point.y - low.y }))
        .collect();
    let mut bevels: Vec<Option<Degrees>> =
        (0..footprint.len()).map(|index| angles.get(index).copied().flatten()).collect();
    if stance.against_y {
        let count = points.len();
        points = (0..count).filter_map(|index| points.get((count - index) % count).copied()).collect();
        bevels = (0..count).filter_map(|index| bevels.get((count - 1 - index) % count).copied()).collect();
    }
    // Vertex 1 is the panel's zero corner (ADR 0105). The section comes back from the niche in its
    // own angular order, which starts at whichever corner the cut listed first; on a board turned
    // about its length axis that is the corner at the far end, so the first corner looked as if the
    // panel had been turned half a turn. Rotate the ring and its bevels together until the zero
    // corner leads, keeping the counter-clockwise travel.
    let start = points.iter().position(|point| point.x.abs() <= 1.0e-9 && point.y.abs() <= 1.0e-9).unwrap_or(0);
    if start != 0 {
        let count = points.len();
        points = (0..count).filter_map(|index| points.get((index + start) % count).copied()).collect();
        bevels = (0..count).filter_map(|index| bevels.get((index + start) % count).copied()).collect();
    }
    LocalRing { points, bevels }
}

/// The section as the document ring a panel stores, anchored at `(0, 0)`.
fn contour_of(points: &[DVec2]) -> Result<Contour, FillRefusal> {
    let corners = points
        .iter()
        .map(|point| Some([document_length(point.x)?, document_length(point.y)?]))
        .collect::<Option<Vec<_>>>()
        .ok_or(FillRefusal::TooSmall)?;
    Contour::from_polygon(corners).map_err(|_| FillRefusal::TooSmall)
}

/// The far face's polygon: the base ring with every beveled edge set back by its own angle.
fn far_face(
    outline: &PanelOutline,
    bevels: &[EdgeBevel],
    corners: &[DVec2],
    thickness: Length,
) -> Result<Vec<DVec2>, FillRefusal> {
    let Some(ring) = outline.contour() else {
        return Ok(corners.to_vec());
    };
    let mut setbacks = vec![0.0; corners.len()];
    for bevel in bevels {
        let EdgeRef::Ring { segment } = bevel.source() else {
            continue;
        };
        let Some(index) = ring.index_of(segment) else {
            continue;
        };
        let Some(slot) = setbacks.get_mut(index) else {
            continue;
        };
        *slot = bevel.setback(thickness).ok_or(FillRefusal::Wedge)?.millimetres_f64();
    }
    set_back(corners, &setbacks).ok_or(FillRefusal::TooSmall)
}

/// A convex ring with each edge's line moved into the body by that edge's own distance.
///
/// The moved lines meet at the inset corners, so the result is the ring the far face keeps. A ring
/// that collapses answers `None`.
fn set_back(corners: &[DVec2], setbacks: &[f64]) -> Option<Vec<DVec2>> {
    let count = corners.len();
    if count < 3 || setbacks.len() != count {
        return None;
    }
    let mut lines: Vec<(DVec2, DVec2)> = Vec::with_capacity(count);
    for index in 0..count {
        let start = *corners.get(index)?;
        let end = *corners.get((index + 1) % count)?;
        let direction = end - start;
        let length = direction.length();
        if !length.is_finite() || length <= 1.0e-9 {
            return None;
        }
        let inward = DVec2::new(-direction.y, direction.x) / length;
        let setback = *setbacks.get(index)?;
        if !setback.is_finite() || setback < 0.0 {
            return None;
        }
        lines.push((start + inward * setback, direction / length));
    }
    let mut points = Vec::with_capacity(count);
    for index in 0..count {
        let (previous, previous_direction) = *lines.get((index + count - 1) % count)?;
        let (current, current_direction) = *lines.get(index)?;
        let denominator = previous_direction.x * current_direction.y - previous_direction.y * current_direction.x;
        if !denominator.is_finite() {
            return None;
        }
        if denominator.abs() < 1.0e-9 {
            // Two edges in line: their moved lines are one line, and its start point is on it.
            points.push(current);
            continue;
        }
        let across = current - previous;
        let step = (across.x * current_direction.y - across.y * current_direction.x) / denominator;
        let point = previous + previous_direction * step;
        if !point.is_finite() {
            return None;
        }
        points.push(point);
    }
    (area2(&points) > 1.0e-9).then_some(points)
}

/// The signed area of a polygon, positive when its corners run counter-clockwise.
fn area2(points: &[DVec2]) -> f64 {
    let mut total = 0.0;
    for index in 0..points.len() {
        let (Some(start), Some(end)) = (points.get(index), points.get((index + 1) % points.len())) else {
            continue;
        };
        total += start.x * end.y - end.x * start.y;
    }
    total * 0.5
}

/// The largest axis-aligned rectangle inside a convex polygon, as its low corner and its size.
///
/// The polygon is the section the walls leave, in millimetres and counter-clockwise. Two facts make
/// the search exact rather than a search: for a convex polygon the slice at `x` is one interval
/// `[low(x), high(x)]`, `low` is convex and `high` is concave, so a rectangle `[u, v] × [y, z]` is
/// inside exactly when `y >= max(low(u), low(v))` and `z <= min(high(u), high(v))`. Its **log**
/// area is therefore a concave function of `(u, v)`, which has one maximum: the breakpoint pairs
/// seed the search and a nested ternary refinement finds that maximum, so no greedy growth from a
/// corner can answer a wider-looking rectangle than the widest one.
///
/// A polygon that leaves no rectangle at all — a line, a single point, a section thinner than the
/// document's micrometre — answers `None`.
/// The largest rectangle that lies inside the footprint **and contains `point`**.
///
/// A fit goes where the hand points: the largest rectangle of a shape that is not convex can stand in
/// another arm of it — an L's largest is a column the pointer may be nowhere near — and a part that
/// appeared across the case from the click would be a surprise (ADR 0204). `None` when no rectangle
/// through the point fits, which a caller answers with the search that is free to look anywhere.
fn largest_rectangle_through(points: &[DVec2], point: DVec2) -> Option<(DVec2, DVec2)> {
    search_rectangle(points, point.is_finite().then_some(point))
}

/// The largest rectangle inside the footprint, wherever it stands.
fn largest_rectangle(points: &[DVec2]) -> Option<(DVec2, DVec2)> {
    search_rectangle(points, None)
}

/// The largest axis-aligned rectangle inside a polygon, optionally through a point.
///
/// The sides of a largest rectangle lie on the shape's own lines or on the point's, so the candidates
/// are every pair of vertex x-coordinates: a rectilinear outline is answered exactly, and an arc, which
/// the ring answers by its chord, to the chord. Every candidate is checked against the whole polygon
/// ([`rect_inside`]), so a band the shape narrows in the middle can never pass a side.
fn search_rectangle(points: &[DVec2], through: Option<DVec2>) -> Option<(DVec2, DVec2)> {
    if points.len() < 3 {
        return None;
    }
    let first = points.first().copied()?;
    let min = points.iter().copied().fold(first, DVec2::min);
    let max = points.iter().copied().fold(first, DVec2::max);
    if !(max.x - min.x).is_finite() || !(max.y - min.y).is_finite() {
        return None;
    }
    let mut xs = breakpoints_x(points);
    if let Some(point) = through {
        if point.x < min.x || point.x > max.x || point.y < min.y || point.y > max.y {
            return None;
        }
        xs.push(point.x);
        xs.sort_by(|first, second| first.partial_cmp(second).unwrap_or(std::cmp::Ordering::Equal));
        xs.dedup_by(|first, second| (*first - *second).abs() <= 1.0e-9);
    }
    let mut best: Option<(f64, DVec2, DVec2)> = None;
    let mut consider = |u: f64, v: f64| {
        if let Some(point) = through
            && (u > point.x + 1.0e-9 || v < point.x - 1.0e-9)
        {
            return;
        }
        let Some((bottom, top)) = band_span(points, u, v) else {
            return;
        };
        if let Some(point) = through
            && (point.y < bottom - 1.0e-9 || point.y > top + 1.0e-9)
        {
            return;
        }
        let size = DVec2::new(v - u, top - bottom);
        let corner = DVec2::new(u, bottom);
        if size.x <= 0.0 || size.y <= 0.0 || !rect_inside(points, corner, size) {
            return;
        }
        let area = size.x * size.y;
        if best.is_none_or(|(seen, ..)| area > seen) {
            best = Some((area, corner, size));
        }
    };
    for (index, u) in xs.iter().copied().enumerate() {
        for v in xs.iter().copied().skip(index.saturating_add(1)) {
            consider(u, v);
        }
    }
    // A slanted wall puts the best rectangle's side between two vertices, which no candidate names: the
    // search is sharpened by thirds on each axis. The objective is concave in the other variable on a
    // convex shape, and every candidate is validated, so a shape that is not convex only loses the
    // sharpening and never gains a rectangle that stands in material.
    let mut low_u = min.x;
    let mut high_u = through.map_or(max.x, |point| point.x);
    for _ in 0..REFINE_STEPS {
        let first = low_u + (high_u - low_u) / 3.0;
        let second = high_u - (high_u - low_u) / 3.0;
        let first_best = best_v(points, first, through, max.x, &mut consider);
        let second_best = best_v(points, second, through, max.x, &mut consider);
        if first_best <= second_best {
            low_u = first;
        } else {
            high_u = second;
        }
    }
    let (_, corner, size) = best?;
    (size.x > 0.0 && size.y > 0.0).then_some((corner, size))
}

/// How many times the sharpening splits each axis.
///
/// Two thirds per step: forty steps shrink the span by more than ten orders of magnitude, which is past
/// the document's micrometre and stops before the search is comparing noise.
const REFINE_STEPS: usize = 40;

/// The best `v` above `u` for one outer candidate, recording every rectangle it evaluates.
fn best_v(points: &[DVec2], u: f64, through: Option<DVec2>, high: f64, consider: &mut impl FnMut(f64, f64)) -> f64 {
    let mut best = f64::NEG_INFINITY;
    let mut low = through.map_or(u, |point| point.x.max(u));
    let mut top = high;
    for _ in 0..REFINE_STEPS {
        let first = low + (top - low) / 3.0;
        let second = top - (top - low) / 3.0;
        let first_score = span_score(points, u, first, through);
        let second_score = span_score(points, u, second, through);
        consider(u, first);
        consider(u, second);
        best = best.max(first_score).max(second_score);
        if first_score <= second_score {
            low = first;
        } else {
            top = second;
        }
    }
    best
}

/// The log area of the rectangle between `u` and `v`, or negative infinity when there is none.
///
/// The search is *steered* by this number, so it carries the whole rule: a band that leaves the point
/// out is no candidate at all, or the sharpening would walk towards a rectangle the answer may not be.
fn span_score(points: &[DVec2], u: f64, v: f64, through: Option<DVec2>) -> f64 {
    let Some((bottom, top)) = band_span(points, u, v) else {
        return f64::NEG_INFINITY;
    };
    if let Some(point) = through
        && (point.y < bottom - 1.0e-9 || point.y > top + 1.0e-9)
    {
        return f64::NEG_INFINITY;
    }
    let (width, height) = (v - u, top - bottom);
    if width <= 0.0 || height <= 0.0 {
        return f64::NEG_INFINITY;
    }
    width.ln() + height.ln()
}

/// Every vertex x-coordinate, sorted, as the polygon's own breakpoints.
fn breakpoints_x(points: &[DVec2]) -> Vec<f64> {
    let mut xs = points.iter().map(|point| point.x).collect::<Vec<_>>();
    xs.sort_by(|first, second| first.partial_cmp(second).unwrap_or(std::cmp::Ordering::Equal));
    xs.dedup_by(|first, second| (*first - *second).abs() <= 1.0e-9);
    xs
}

/// The y-interval the whole band between `u` and `v` leaves inside a polygon.
///
/// A rectangle is inside only when **every** x between its sides is: on a convex polygon the two ends
/// are enough, because the span between them only narrows towards the middle — but a frame drawn as an
/// L or with an arc is not convex, and the wall that bounds the band can stand anywhere in it. The
/// candidates are the polygon's own breakpoints, where a vertical span is lowest or highest, plus the
/// two ends the search hands in.
fn band_span(points: &[DVec2], u: f64, v: f64) -> Option<(f64, f64)> {
    if u > v {
        return None;
    }
    let mut bottom = f64::NEG_INFINITY;
    let mut top = f64::INFINITY;
    let mut seen = false;
    let mut consider = |x: f64| {
        let Some((low, high)) = vertical_span(points, x) else {
            return;
        };
        seen = true;
        bottom = bottom.max(low);
        top = top.min(high);
    };
    consider(u);
    consider(v);
    for x in breakpoints_x(points) {
        if x > u + 1.0e-9 && x < v - 1.0e-9 {
            consider(x);
        }
    }
    (seen && top > bottom).then_some((bottom, top))
}

/// Whether an axis-aligned rectangle lies inside a simple polygon.
///
/// A rectangle is inside when its centre is and no edge of the polygon crosses its interior: a
/// boundary that steps into the rectangle must cross a side, so a crossing test over the ring answers
/// every notch, however the span at one x reads.
fn rect_inside(points: &[DVec2], corner: DVec2, size: DVec2) -> bool {
    if size.x <= 0.0 || size.y <= 0.0 {
        return false;
    }
    let (low, high) = (corner, corner + size);
    let centre = DVec2::new((low.x + high.x) * 0.5, (low.y + high.y) * 0.5);
    if !polygon_holds(points, centre) {
        return false;
    }
    !(0..points.len()).any(|index| {
        let (Some(start), Some(end)) = (points.get(index), points.get((index + 1) % points.len())) else {
            return true;
        };
        edge_crosses_rect(*start, *end, low, high)
    })
}

/// Whether the point is inside a simple polygon, by the crossing parity of a ray to `+x`.
fn polygon_holds(points: &[DVec2], point: DVec2) -> bool {
    let mut inside = false;
    for (index, start) in points.iter().enumerate() {
        let Some(end) = points.get((index + 1) % points.len()) else {
            return false;
        };
        if (start.y > point.y) != (end.y > point.y) {
            let t = (point.y - start.y) / (end.y - start.y);
            if point.x < start.x + (end.x - start.x) * t {
                inside = !inside;
            }
        }
    }
    inside
}

/// Whether a segment crosses the open interior of the rectangle `low..high`.
///
/// The segment is clipped against the rectangle's own sides (Liang and Barsky): a parameter interval
/// inside the box that is more than a point is a crossing, which is the only way a ring's boundary can
/// step into a rectangle while the rectangle's centre is inside the ring.
fn edge_crosses_rect(start: DVec2, end: DVec2, low: DVec2, high: DVec2) -> bool {
    let direction = end - start;
    let mut enter = 0.0f64;
    let mut exit = 1.0f64;
    for (origin, step, floor, ceiling) in [(start.x, direction.x, low.x, high.x), (start.y, direction.y, low.y, high.y)]
    {
        if step.abs() <= 1.0e-12 {
            if origin <= floor + 1.0e-9 || origin >= ceiling - 1.0e-9 {
                return false;
            }
            continue;
        }
        let first = (floor - origin) / step;
        let second = (ceiling - origin) / step;
        enter = enter.max(first.min(second));
        exit = exit.min(first.max(second));
        if exit <= enter + 1.0e-9 {
            return false;
        }
    }
    exit - enter > 1.0e-9
}

/// The interval a convex polygon leaves on the vertical line at `x`, in millimetres.
///
/// A line through a convex polygon meets its boundary in at most two points; collecting every
/// crossing and taking the extremes also answers a line through a vertical edge or a vertex, which
/// is where the search's candidates sit.
fn vertical_span(points: &[DVec2], x: f64) -> Option<(f64, f64)> {
    let mut low = f64::INFINITY;
    let mut high = f64::NEG_INFINITY;
    for (index, start) in points.iter().enumerate() {
        let end = points.get((index + 1) % points.len())?;
        let span = end.x - start.x;
        if span.abs() <= 1.0e-12 {
            if (x - start.x).abs() <= 1.0e-9 {
                low = low.min(start.y.min(end.y));
                high = high.max(start.y.max(end.y));
            }
            continue;
        }
        let t = (x - start.x) / span;
        if !(-1.0e-9..=1.0 + 1.0e-9).contains(&t) {
            continue;
        }
        let y = start.y + (end.y - start.y) * t;
        if !y.is_finite() {
            return None;
        }
        low = low.min(y);
        high = high.max(y);
    }
    (high > low).then_some((low, high))
}

/// The part's place along the free axis: whole millimetres, kept inside the void.
///
/// A number that is not finite names no place at all, so it answers the cell's low wall — the one
/// position the walls are known to hold — instead of a coordinate the document cannot store.
fn follow(value: f64, low: f64, high: f64) -> f64 {
    if !value.is_finite() || !low.is_finite() || !high.is_finite() {
        return low;
    }
    value.round().clamp(low, high)
}

/// A whole-millimetre measurement as the document's length, or `None` when it is unusable.
///
/// The model measures to the micrometre, but the free axis moves in whole millimetres (ADR 0103);
/// the measured sides keep their micrometre precision, so a part stays flush with the void.
fn length_of(millimetres: f64) -> Option<Length> {
    if !millimetres.is_finite() || millimetres <= 0.0 {
        return None;
    }
    document_length(millimetres)
}

/// A measurement as the document's whole micrometres, a corner at the origin included.
fn document_length(millimetres: f64) -> Option<Length> {
    if !millimetres.is_finite() {
        return None;
    }
    let micrometres = (millimetres * 1000.0).round();
    if !micrometres.is_finite() || micrometres.abs() > i64::MAX as f64 {
        return None;
    }
    Some(Length::from_micrometres(micrometres as i64))
}

/// How many times an overlay board's four ends are settled against the parts in front of the plane.
///
/// One end's answer changes the span the next one is measured against, and the spans only ever
/// shrink, so a handful of passes settles every case a frame can pose; the cap is what keeps a
/// pathological scene from looping.
const OVERLAY_CLIP_PASSES: usize = 8;

/// How near a face counts as touching, in millimetres: the document's own micrometre.
///
/// The measured point rests a micrometre off the face it was measured from, so a body whose face
/// stands there is contact and not a cut, exactly as a pair of boxes that share a face shares no
/// volume (ADR 0162).
const TRIM_TOLERANCE: f64 = 1.0e-3;

/// A hidden body's box in the open product's frame, millimetres.
#[derive(Clone, Copy, Debug)]
struct HiddenBox {
    min: DVec3,
    max: DVec3,
}

/// One body's box in the product's frame, or `None` when the body has no volume there.
fn body_box_under(inverse: Placement, surface: SurfaceBox) -> Option<HiddenBox> {
    let mut min = DVec3::splat(f64::INFINITY);
    let mut max = DVec3::splat(f64::NEG_INFINITY);
    for corner in surface_corners(surface) {
        let local = inverse.transform_point(corner);
        if !local.is_finite() {
            return None;
        }
        min = min.min(local);
        max = max.max(local);
    }
    (0..3).all(|axis| min[axis] < max[axis]).then_some(HiddenBox { min, max })
}

impl Project {
    /// The parts the sight switched off, as boxes in the open product's frame.
    ///
    /// A hidden part is not a wall of the void the eye measures, but it is still material, so the fill
    /// stops at its faces (X73, ADR 0175). The box is the body's own, taken through the product's
    /// placement: a turned body is read as the box around it in that frame, which trims a little more
    /// than it must and never less.
    fn hidden_boxes(&self, world: Placement, sight: Sight) -> Vec<HiddenBox> {
        let Some(inverse) = world.inverse().ok() else {
            return Vec::new();
        };
        self.hidden_body_surfaces(sight).into_iter().filter_map(|surface| body_box_under(inverse, surface)).collect()
    }

    /// Every part's box in the open product's frame, millimetres, drawn by the sight or not.
    ///
    /// An overlay board reads the frame from these boxes: what stands beside the opening gives an end
    /// something to run over and what stands in front of the face is what stops it, and neither
    /// question is a question about sight (ADR 0240).
    fn part_boxes(&self, world: Placement) -> Vec<HiddenBox> {
        let Some(inverse) = world.inverse().ok() else {
            return Vec::new();
        };
        let mut surfaces = Vec::new();
        self.body_surfaces(&self.root, &mut surfaces, 1, Sight::none());
        surfaces.into_iter().filter_map(|surface| body_box_under(inverse, surface.box_)).collect()
    }

    /// The body surfaces the sight switched off, in world millimetres.
    ///
    /// The walk is the niche's own, so the parts a fill trims against are the parts a probe passes:
    /// one reader of the tree cannot name a different set from the other.
    fn hidden_body_surfaces(&self, sight: Sight) -> Vec<SurfaceBox> {
        if sight.hidden().is_none_or(HashSet::is_empty) {
            return Vec::new();
        }
        let mut surfaces = Vec::new();
        self.body_surfaces(&self.root, &mut surfaces, 1, Sight::none());
        surfaces.retain(|surface| sight.hides(self, surface.box_.id));
        surfaces.into_iter().map(|surface| surface.box_).collect()
    }
}

/// The footprint cut back at every hidden body's face on the board's own plane.
///
/// A part the sight switched off is invisible to the eye and to the pointer, never to the material:
/// the ray that measured the void crossed it, so a board drawn across the whole cell would land
/// inside it. Every hidden body that reaches the board's slab cuts the footprint at one of its own
/// faces, and the cut keeps a side of the body rather than a slice of it: a door at the front trims
/// the depth, a post at the side trims the width. The side the measured point stands on wins, so the
/// part takes the space the maker pointed at; where the point lies inside the body's own footprint —
/// a hidden shelf under the hand, a facade the ray crossed on its way in — the cut that keeps the
/// most of the void takes it, which leaves that body's larger side rather than an empty plan. A body
/// that leaves nothing is `TooSmall`: the void has no room left to fill.
fn trim_footprint(
    polygon: Vec<DVec2>,
    hidden: &[HiddenBox],
    stance: &Stance,
    at: DVec3,
    slab: (f64, f64),
) -> Result<(Vec<DVec2>, Vec<Cut>), FillRefusal> {
    let mut polygon = polygon;
    let mut cuts = Vec::new();
    for body in hidden {
        // A body that only reaches the slab's own faces is contact, not a cut.
        if body.max[stance.t] <= slab.0 + TRIM_TOLERANCE || body.min[stance.t] >= slab.1 - TRIM_TOLERANCE {
            continue;
        }
        let (min, max) =
            (DVec2::new(body.min[stance.u], body.min[stance.v]), DVec2::new(body.max[stance.u], body.max[stance.v]));
        // A body that stands clear of what is left of the void cuts nothing.
        if !polygon_hits_rect(&polygon, min, max) {
            continue;
        }
        let mut best: Option<((bool, f64), Vec<DVec2>, Cut)> = None;
        for axis in [stance.u, stance.v] {
            // The two faces of the body on this axis, each keeping the side it bounds.
            for (face, side) in [(body.min[axis], -1.0), (body.max[axis], 1.0)] {
                let mut normal = DVec2::ZERO;
                normal[axis] = side;
                let Some(kept) = clip_half_plane(&polygon, normal, side * face) else {
                    continue;
                };
                let stands = side * (at[axis] - face) > TRIM_TOLERANCE;
                let score = (stands, signed_area(&kept));
                if best.as_ref().is_none_or(|(seen, ..)| score > *seen) {
                    best = Some((score, kept, Cut { normal, offset: side * face }));
                }
            }
        }
        match best {
            Some((_, kept, cut)) => {
                polygon = kept;
                cuts.push(cut);
            }
            None => return Err(FillRefusal::TooSmall),
        }
    }
    Ok((polygon, cuts))
}

/// Whether a convex polygon and an axis-aligned rectangle share any area.
///
/// The separating-axis test over the rectangle's two axes and the polygon's edge normals: a pair that
/// stands apart on one axis touches at most, which is contact and therefore no cut (ADR 0162).
fn polygon_hits_rect(polygon: &[DVec2], min: DVec2, max: DVec2) -> bool {
    let corners = [min, DVec2::new(max.x, min.y), max, DVec2::new(min.x, max.y)];
    let mut axes = vec![DVec2::X, DVec2::Y];
    for index in 0..polygon.len() {
        let (Some(start), Some(end)) = (polygon.get(index), polygon.get((index + 1) % polygon.len())) else {
            return false;
        };
        if let Some(unit) = (*end - *start).try_normalize() {
            axes.push(DVec2::new(-unit.y, unit.x));
        }
    }
    axes.into_iter().all(|axis| {
        let (low_polygon, high_polygon) = projection_span(polygon, axis);
        let (low_rect, high_rect) = projection_span(&corners, axis);
        high_polygon > low_rect + TRIM_TOLERANCE && high_rect > low_polygon + TRIM_TOLERANCE
    })
}

/// How far a set of points reaches along one axis.
fn projection_span(points: &[DVec2], axis: DVec2) -> (f64, f64) {
    points.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(low, high), point| {
        let reach = axis.dot(*point);
        (low.min(reach), high.max(reach))
    })
}

/// The footprint cut by one half-plane `normal · point >= offset`, or `None` when it keeps no area.
///
/// The footprint is convex and counter-clockwise, and a half-plane of a convex cell keeps it both:
/// the clip walks the edges, keeps every corner on the free side and adds the point where an edge
/// crosses the line.
fn clip_half_plane(polygon: &[DVec2], normal: DVec2, offset: f64) -> Option<Vec<DVec2>> {
    let reach = |point: DVec2| normal.dot(point) - offset;
    let mut kept: Vec<DVec2> = Vec::with_capacity(polygon.len().saturating_add(1));
    for index in 0..polygon.len() {
        let start = *polygon.get(index)?;
        let end = *polygon.get((index + 1) % polygon.len())?;
        let (start_in, end_in) = (reach(start), reach(end));
        if start_in >= -TRIM_TOLERANCE {
            push_corner(&mut kept, start);
        }
        let crossing = (start_in > TRIM_TOLERANCE && end_in < -TRIM_TOLERANCE)
            || (start_in < -TRIM_TOLERANCE && end_in > TRIM_TOLERANCE);
        if crossing {
            let step = start_in - end_in;
            if step.abs() > f64::EPSILON {
                let corner = start + (end - start) * (start_in / step);
                if corner.is_finite() {
                    push_corner(&mut kept, corner);
                }
            }
        }
    }
    (kept.len() >= 3).then_some(kept)
}

/// Pushes a corner unless the polygon already holds one at the same place.
fn push_corner(points: &mut Vec<DVec2>, corner: DVec2) {
    if points.iter().all(|seen| (*seen - corner).length() > TRIM_TOLERANCE) {
        points.push(corner);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::millimetres;

    /// The shelf plan of an empty `600 x 560 x 720` case, at the height the point names.
    ///
    /// The row tests read their numbers off this plan, so no test repeats the niche's arithmetic:
    /// the pair is a shelf's (width and depth), its free axis is the case's height, and the plan's
    /// two free ends are the board's base and far faces.
    fn shelf_plan(height: f64) -> FillPlan {
        let (project, product) = empty_product(600, 560, 720);
        let point = DVec3::new(300.0, 280.0, height);
        project
            .fill_plan(
                product,
                FillRequest::new(point, Fill::WidthAndDepth, FillMode::Fit, millimetres(16), Some(height)),
                Sight::none(),
                None,
            )
            .expect("the point names a shelf in the void")
    }

    /// A row's ask with the first board where the plan put it.
    fn ask(plan: &FillPlan, span: Option<f64>, count: Option<usize>, gap: Option<f64>) -> FillRowAsk {
        FillRowAsk { start: plan.free_low, span, count, gap }
    }

    #[test]
    fn a_row_shares_its_span_out_and_leaves_the_rest_at_the_far_end() {
        let plan = shelf_plan(200.0);
        let thickness = plan.free_high - plan.free_low;
        let axis = free_axis(Fill::WidthAndDepth);
        let row = plan.row(axis, ask(&plan, Some(160.0), None, None)).expect("a row fits");
        assert_eq!(row.count, 10, "160 mm holds ten 16 mm boards");
        assert!(row.gap.abs() < 1.0e-6, "ten boards fill their span with no gap left: {}", row.gap);
        assert!((row.pitch - thickness).abs() < 1.0e-6);
        // A span a little longer than fifteen boards leaves the extra millimetres to the far end.
        let loose = plan.row(axis, ask(&plan, Some(241.0), None, None)).expect("a row fits");
        assert_eq!(loose.count, 15, "241 mm still holds fifteen boards");
        assert!((loose.gap - 1.0 / 15.0).abs() < 1.0e-6, "the extra millimetre is shared: {}", loose.gap);
        assert!((loose.high - loose.gap - 0.0).abs() > 0.0, "the unused length stays at the far end");
    }

    #[test]
    fn a_row_grows_one_board_per_thickness_and_never_crosses() {
        let plan = shelf_plan(200.0);
        let axis = free_axis(Fill::WidthAndDepth);
        let wall = plan.niche.local_min[axis] + plan.niche.size[axis];
        let thickness = plan.free_high - plan.free_low;
        let mut previous = 0;
        let mut span = 0.0;
        while span <= plan.niche.size[axis] {
            let row = plan.row(axis, ask(&plan, Some(span), None, None)).expect("a row fits");
            assert!(row.count >= previous, "the count never shrinks as the hand pulls");
            assert!(
                row.count <= previous + 1,
                "{span} mm moved the count by more than one: {previous} -> {}",
                row.count
            );
            assert!(row.end <= wall + ROW_TOLERANCE, "a board crossed the wall at {span} mm");
            assert!(row.pitch >= thickness - ROW_TOLERANCE, "two boards crossed at {span} mm");
            previous = row.count;
            span += 1.0;
        }
        assert!(previous > 1, "the row grew past one board");
    }

    #[test]
    fn a_typed_count_takes_the_whole_void_and_a_fixed_gap_keeps_its_distance() {
        let plan = shelf_plan(200.0);
        let axis = free_axis(Fill::WidthAndDepth);
        let thickness = plan.free_high - plan.free_low;
        let room = plan.niche.local_min[axis] + plan.niche.size[axis] - plan.free_low;
        let typed = plan.row(axis, ask(&plan, None, Some(4), None)).expect("four boards fit");
        assert_eq!(typed.count, 4);
        assert!((typed.gap - (room - 4.0 * thickness) / 4.0).abs() < 1.0e-6, "the void is shared out: {}", typed.gap);
        assert!((typed.high - typed.gap).abs() < 1.0e-6, "the leftover at the far end is the shared gap");
        // A fixed far gap is honoured, and the neighbours divide what is left.
        let fixed = plan.row(axis, ask(&plan, None, Some(4), Some(40.0))).expect("a fixed gap fits");
        assert_eq!(fixed.count, 4);
        assert!((fixed.high - 40.0).abs() < 1.0e-6, "the fixed gap is what the row leaves: {}", fixed.high);
        let shared = (room - 4.0 * thickness - 40.0) / 3.0;
        assert!((fixed.gap - shared).abs() < 1.0e-6, "the neighbours share the rest: {}", fixed.gap);
        // A count past what fits is clamped instead of refused, and a gap wider than the void is not.
        let clamped = plan.row(axis, ask(&plan, None, Some(10_000), None)).expect("the count clamps");
        assert_eq!(clamped.count, (room / thickness + ROW_TOLERANCE).floor() as usize);
        assert_eq!(plan.row(axis, ask(&plan, None, Some(4), Some(room))), Err(FillRowRefusal::GapTooWide));
    }

    #[test]
    fn a_row_of_one_board_cannot_share_a_fixed_gap() {
        let plan = shelf_plan(200.0);
        let axis = free_axis(Fill::WidthAndDepth);
        let thickness = plan.free_high - plan.free_low;
        let room = plan.niche.local_min[axis] + plan.niche.size[axis] - plan.free_low;
        let span = 20.0;
        let row = plan.row(axis, ask(&plan, Some(span), None, None)).expect("one board in 20 mm");
        assert_eq!(row.count, 1);
        assert!((row.gap - (span - thickness)).abs() < 1.0e-6, "one board leaves its span at the end: {}", row.gap);
        // A typed count claims the whole void, so the only gap one board can honour is its own leftover.
        assert_eq!(plan.row(axis, ask(&plan, Some(span), Some(1), Some(4.0))), Err(FillRowRefusal::NotDivided));
        let exact = plan.row(axis, ask(&plan, None, Some(1), Some(room - thickness))).expect("its own leftover");
        assert!((exact.high - (room - thickness)).abs() < 1.0e-6);
    }

    #[test]
    fn a_void_narrower_than_one_board_has_no_row() {
        let plan = shelf_plan(200.0);
        let axis = free_axis(Fill::WidthAndDepth);
        let wall = plan.niche.local_min[axis] + plan.niche.size[axis];
        let thin = FillRowAsk { start: wall - 8.0, ..ask(&plan, None, None, None) };
        assert_eq!(plan.row(axis, thin), Err(FillRowRefusal::NoRoom));
        let nan = FillRowAsk { start: f64::NAN, ..ask(&plan, None, None, None) };
        assert_eq!(plan.row(axis, nan), Err(FillRowRefusal::NoRoom));
    }

    /// An empty piece with the given envelope at the origin: nothing of the sample scene stands in
    /// its void, so the niche is the box the walls reserve.
    fn empty_product(x: i64, y: i64, z: i64) -> (Project, Id) {
        let root = Container::new(Id::new(1).expect("an id"), "Root", Placement::IDENTITY, Vec::new());
        let mut project = Project::new(root).expect("an empty project");
        let root = project.root().id();
        let envelope = Envelope::new(millimetres(x), millimetres(y), millimetres(z)).expect("a valid box");
        let product = project
            .add_product(root, "Cabinet", ClassCode::GENERAL_PRODUCT, envelope, Placement::IDENTITY)
            .expect("a product");
        (project, product)
    }

    /// The void of a case whose two posts stand at an angle: the free section is a trapezoid that
    /// runs from (100, 0) and (500, 0) at the floor to (200, 560) and (400, 560) at the back.
    fn splayed_product() -> (Project, Id) {
        let (mut project, product) = empty_product(600, 560, 720);
        let slant = 100.0 / 560.0_f64.hypot(100.0);
        let drop = 560.0 / 560.0_f64.hypot(100.0);
        for (through, normal) in [((100.0, 0.0), (drop, -slant)), ((500.0, 0.0), (-drop, -slant))] {
            let direction = (-normal.1, normal.0);
            let start_y = 280.0 - 600.0 * direction.1;
            let start_x = through.0 + (start_y - through.1) * (direction.0 / direction.1);
            let rotation =
                DQuat::from_rotation_z(normal.1.atan2(normal.0)) * DQuat::from_rotation_y(std::f64::consts::FRAC_PI_2);
            let translation = DVec3::new(start_x, start_y, 720.0) - DVec3::new(normal.0, normal.1, 0.0) * 18.0;
            let placement = Placement::new(translation, rotation).expect("a post placement");
            let size = PanelNominalSize::new(millimetres(720), millimetres(1200)).expect("a post size");
            project
                .add_panel(product, "Post", size, None, millimetres(18), placement)
                .expect("the post lands in the open product");
        }
        (project, product)
    }

    /// A case whose four members leave a 600 x 720 opening: the picture of a front.
    ///
    /// Each member stands flush with the front plane (`y = 0`) and runs back through the case, so
    /// every edge of the opening has a part behind it for a lap to run over. The opening spans
    /// `x = 18..618` and `z = 18..738`; nothing stands in front of it.
    fn framed_product() -> (Project, Id) {
        let (mut project, product) = empty_product(636, 560, 756);
        // The board's own axes in the product's: local X down the depth, local Y up the height and
        // local Z across the thickness, so a member is a box the placement decides.
        let boxed = DQuat::from_rotation_axes(DVec3::Y, DVec3::Z, DVec3::X);
        let members = [
            (DVec3::ZERO, 560.0, 756.0),
            (DVec3::new(618.0, 0.0, 0.0), 560.0, 756.0),
            (DVec3::new(18.0, 0.0, 0.0), 600.0, 18.0),
            (DVec3::new(18.0, 0.0, 738.0), 600.0, 18.0),
        ];
        for (translation, length, width) in members {
            let placement = Placement::new(translation, boxed).expect("a member placement");
            let size = PanelNominalSize::new(
                Length::from_millimetres(length as i64).expect("a length"),
                Length::from_millimetres(width as i64).expect("a width"),
            )
            .expect("a member size");
            project.add_panel(product, "Member", size, None, millimetres(18), placement).expect("the member lands");
        }
        (project, product)
    }

    /// The front of the opening of a framed case, in world millimetres.
    fn opening(project: &Project, product: Id) -> DVec3 {
        project.world_placement(product).expect("a frame").transform_point(DVec3::new(318.0, 280.0, 378.0))
    }

    /// The plan an overlay click makes at the case's opening.
    fn overlay(project: &Project, product: Id, lap: Length) -> Result<FillPlan, FillRefusal> {
        project.fill_plan(
            product,
            FillRequest::new(opening(project, product), Fill::WidthAndHeight, FillMode::Fit, millimetres(18), None)
                .with_mount(FillMount::Overlay { laps: [lap; 4] }),
            Sight::none(),
            None,
        )
    }

    /// The board a plan describes: the material's own span, in panel-local millimetres.
    ///
    /// A panel's frame is its nominal rectangle and its material is what the trims make of it, so a
    /// front's *board* is the polygon and not the size (ADR 0240).
    fn board_span(plan: &FillPlan) -> (f64, f64) {
        let first = plan.corners.first().copied().expect("a corner");
        let (low, high) =
            plan.corners.iter().copied().fold((first, first), |(low, high), point| (low.min(point), high.max(point)));
        (high.x - low.x, high.y - low.y)
    }

    #[test]
    fn an_overlay_front_runs_a_lap_past_the_opening_and_lies_on_its_face() {
        // The task's own case (U124): a 600 x 720 opening, an 18 mm board and a 14 mm lap from the
        // workshop's numbers give a 628 x 748 front whose inner face is the plane the void opens
        // onto and whose body stands in front of it (ADR 0240). The frame stays the opening, so the
        // part's position is controlled by the corner of the opening and never by a lap.
        let (project, product) = framed_product();
        let plan = overlay(&project, product, millimetres(14)).expect("the frame gives the front its lap");
        assert_eq!(plan.nominal_size.length(), millimetres(720), "the frame is the opening's height");
        assert_eq!(plan.nominal_size.width(), millimetres(600), "and its width");
        assert_eq!(plan.trims.as_array(), [millimetres(-14); 4], "each end carries the lap the frame allowed");
        let (length, width) = board_span(&plan);
        assert!((length - 748.0).abs() < 1.0e-9, "the board is 748 long, got {length}");
        assert!((width - 628.0).abs() < 1.0e-9, "and 628 wide, got {width}");
        assert!(!plan.outline.has_contour_features() && plan.outline.contour().is_none(), "a front is a rectangle");
        // The frame's corner is the opening's own corner, and the board runs a lap past it.
        let placed = plan.placement.translation();
        assert!((placed.z - 18.0).abs() < 1.0e-9 && (placed.x - 18.0).abs() < 1.0e-9, "the opening's corner");
        // The body runs one thickness in front of the start plane, so its inner face is that plane.
        let gap = 18.0;
        for corner in &plan.corners {
            let inner = plan.placement.transform_point(DVec3::new(corner.x, corner.y, gap));
            assert!(inner.y.abs() < 1.0e-9, "the inner face lies on the opening's plane: {inner:?}");
            let outer = plan.placement.transform_point(DVec3::new(corner.x, corner.y, 0.0));
            assert!(outer.y < -gap + 1.0e-9, "the body stands proud of the face: {outer:?}");
        }
        let axis = free_axis(Fill::WidthAndHeight);
        assert!((plan.free_low - -18.0).abs() < 1.0e-9, "the front's outer face: {}", plan.free_low);
        assert!(plan.free_high.abs() < 1.0e-9, "and its inner face is the start plane: {}", plan.free_high);
        let _ = axis;
    }

    #[test]
    fn an_end_with_no_part_behind_it_takes_no_lap() {
        // The owner's rule: a board may run past the opening only where there is something to run
        // over. An empty case leaves every end on the opening's own edge.
        let (project, product) = empty_product(600, 560, 720);
        let plan = overlay(&project, product, millimetres(14)).expect("an empty case still takes a front");
        assert_eq!(plan.nominal_size.length(), millimetres(720), "the frame is the opening's height");
        assert_eq!(plan.nominal_size.width(), millimetres(600), "and its width");
        let (length, width) = board_span(&plan);
        assert!((length - 720.0).abs() < 1.0e-9 && (width - 600.0).abs() < 1.0e-9, "the board is the opening");
        assert!(plan.trims.is_zero(), "no end carries a trim");
    }

    #[test]
    fn a_part_standing_in_front_stops_the_lap_at_its_own_edge() {
        // A neighbour front already covers the left member and 22 mm of the opening: the new board
        // stops at its edge instead of running over it, and the end it stopped is trimmed back
        // inside the opening — a positive trim, which is the card's own number (ADR 0240).
        let (mut project, product) = framed_product();
        // A front of the wall's own stance: its length up the case, its width across and its
        // material towards the maker, so the body lies in the slab the board will take.
        let faced = DQuat::from_rotation_z(-FRAC_PI_2) * DQuat::from_rotation_y(-FRAC_PI_2);
        let placement = Placement::new(DVec3::new(0.0, -18.0, 0.0), faced).expect("a neighbour placement");
        let size = PanelNominalSize::new(millimetres(756), millimetres(40)).expect("a front size");
        project.add_panel(product, "Neighbour", size, None, millimetres(18), placement).expect("a neighbour");
        let plan = overlay(&project, product, millimetres(14)).expect("the front still lies");
        // The board's own axes: its length runs up the case and its width across it, so the neighbour
        // is at the end the card calls the bottom.
        assert_eq!(plan.nominal_size.width(), millimetres(600), "the frame is the opening, whatever the board does");
        let (_, board_width) = board_span(&plan);
        assert!(
            (board_width - 592.0).abs() < 1.0e-9,
            "the opening plus one lap, cut back at the neighbour: {board_width}"
        );
        assert_eq!(plan.trims.get(RectEdge::Bottom.id()), millimetres(22), "the end stops 22 mm inside the opening");
        assert_eq!(plan.trims.get(RectEdge::Top.id()), millimetres(-14), "the other end keeps its lap");
        let (board_length, _) = board_span(&plan);
        assert!((board_length - 748.0).abs() < 1.0e-9, "the height is untouched");
    }

    #[test]
    fn only_a_front_takes_a_mounting_and_an_inset_board_is_untouched() {
        // The mounting is the frontal board's: a shelf or a post stands across the void, so the model
        // refuses the over-fill by name rather than laying a board somewhere it cannot lie.
        let (project, product) = empty_product(600, 560, 720);
        let refused = project.fill_plan(
            product,
            FillRequest::new(centre(&project, product), Fill::WidthAndDepth, FillMode::Fit, millimetres(18), None)
                .with_mount(FillMount::Overlay { laps: [millimetres(14); 4] }),
            Sight::none(),
            None,
        );
        assert_eq!(refused, Err(FillRefusal::NotFrontal), "a shelf is not a front");

        // The inset board the same request makes is exactly the board it always was, and it carries
        // no trims.
        let inset = plan(&project, product, Fill::WidthAndHeight, FillMode::Fit, millimetres(18)).expect("a wall");
        assert_eq!(inset.nominal_size.width(), millimetres(600), "the opening's own width");
        assert!(inset.trims.is_zero(), "an inset board carries no trim");
    }

    /// The centre of the product's envelope, in world millimetres.
    fn centre(project: &Project, product: Id) -> DVec3 {
        let envelope = project.container(product).and_then(|product| product.envelope()).expect("an envelope");
        let size =
            DVec3::new(envelope.x().millimetres_f64(), envelope.y().millimetres_f64(), envelope.z().millimetres_f64());
        project.world_placement(product).expect("a frame").transform_point(size * 0.5)
    }

    fn plan(
        project: &Project,
        product: Id,
        fill: Fill,
        mode: FillMode,
        thickness: Length,
    ) -> Result<FillPlan, FillRefusal> {
        project.fill_plan(
            product,
            FillRequest::new(centre(project, product), fill, mode, thickness, None),
            Sight::none(),
            None,
        )
    }

    /// A board standing as a wall across the case's back, its inner face through `y = through` at
    /// `z = 0` and leaned by `degrees` about `+X`.
    ///
    /// The free space is in front of it. A positive lean brings the face forward as it rises, so the
    /// void narrows towards the top; a negative one widens it.
    fn leaned_wall(project: &mut Project, product: Id, through: f64, degrees: f64) -> Id {
        let lean = degrees.to_radians();
        let wall = DQuat::from_rotation_z(-FRAC_PI_2) * DQuat::from_rotation_y(-FRAC_PI_2);
        let rotation = DQuat::from_rotation_x(lean) * wall;
        let placement = Placement::new(DVec3::new(0.0, through, 0.0), rotation).expect("a wall placement");
        // The wall stance runs its length up the case and its width across it, so the nominal size
        // is the case's height by its width; the panel is longer than the case so its face still
        // covers the whole void once it leans.
        let nominal = PanelNominalSize::new(millimetres(1600), millimetres(600)).expect("a wall size");
        project.add_panel(product, "Wall", nominal, None, millimetres(18), placement).expect("add a wall")
    }

    /// A board leaning 10 degrees over the case, its underside the void's ceiling.
    fn leaned_ceiling(project: &mut Project, product: Id) {
        let lean = 10.0_f64.to_radians();
        let (sin, cos) = lean.sin_cos();
        let along = DVec3::new(0.0, cos, -sin);
        let away = DVec3::new(0.0, sin, cos);
        let rotation = DQuat::from_rotation_axes(along.cross(away), along, away);
        let placement = Placement::new(DVec3::new(0.0, 0.0, 699.5), rotation).expect("a ceiling placement");
        let nominal = PanelNominalSize::new(millimetres(600), millimetres(620)).expect("a ceiling size");
        project.add_panel(product, "Ceiling", nominal, None, millimetres(18), placement).expect("add a ceiling");
    }

    /// Whether both faces of the plan's body lie inside the void it was measured against.
    fn inside(plan: &FillPlan, thickness: Length) -> bool {
        const TOLERANCE: f64 = 2.0e-3;
        let gap = thickness.millimetres_f64();
        let mut points = Vec::new();
        for (corners, z) in [(&plan.corners, 0.0), (&plan.far_corners, gap)] {
            for corner in corners {
                points.push(plan.placement.transform_point(DVec3::new(corner.x, corner.y, z)));
            }
        }
        points.iter().all(|point| {
            point.is_finite()
                && plan.niche.planes.iter().all(|plane| plane.normal.dot(*point) - plane.offset >= -TOLERANCE)
        })
    }

    /// The base ring's edge that carries `bevel`, as its two corners in panel-local millimetres.
    fn beveled_edge(plan: &FillPlan, bevel: EdgeBevel) -> (DVec2, DVec2) {
        let ring = plan.outline.contour().expect("a beveled part is a contour");
        let EdgeRef::Ring { segment } = bevel.source() else {
            panic!("a fill bevels ring segments");
        };
        let index = ring.index_of(segment).expect("the bevel names a segment of the ring");
        let count = plan.corners.len();
        (plan.corners[index], plan.corners[(index + 1) % count])
    }

    /// How far the far edge under `bevel` sits from the wall plane `normal . p = offset`, in mm.
    fn far_edge_distance(plan: &FillPlan, bevel: EdgeBevel, normal: DVec3, offset: f64, thickness: Length) -> f64 {
        let (start, end) = beveled_edge(plan, bevel);
        let direction = end - start;
        let length = direction.length();
        let inward = DVec2::new(-direction.y, direction.x) / length;
        let setback = bevel.setback(thickness).expect("a setback").millimetres_f64();
        let gap = thickness.millimetres_f64();
        let mut worst: f64 = 0.0;
        for corner in [start, end] {
            let moved = corner + inward * setback;
            let point = plan.placement.transform_point(DVec3::new(moved.x, moved.y, gap));
            worst = worst.max((normal.dot(point) - offset).abs());
        }
        worst
    }

    #[test]
    fn a_shelf_against_a_leaned_wall_gets_the_walls_own_bevel() {
        let (mut project, product) = empty_product(600, 560, 720);
        leaned_wall(&mut project, product, 560.0, 10.0);
        let thickness = millimetres(18);
        let shaped = plan(&project, product, Fill::WidthAndDepth, FillMode::Shape, thickness).expect("a shaped shelf");
        assert!(shaped.outline.contour().is_some(), "a beveled board is a contour whatever its polygon looks like");
        assert_eq!(shaped.bevels.len(), 1, "only the leaning wall asks for one: {:?}", shaped.bevels);
        let bevel = shaped.bevels[0];
        assert_eq!(bevel.angle().millidegrees(), 10_000, "the wall's own lean");
        let lean = 10.0_f64.to_radians();
        let (sin, cos) = lean.sin_cos();
        // The wall's free-side normal and the offset of its plane through (0, 560, 0).
        let normal = DVec3::new(0.0, -cos, -sin);
        let distance = far_edge_distance(&shaped, bevel, normal, normal.dot(DVec3::new(0.0, 560.0, 0.0)), thickness);
        assert!(distance <= 1.0e-3, "the far edge lies in the wall's plane, off by {distance} mm");
        assert!(inside(&shaped, thickness), "and the whole board stays in the void");
        assert!(shaped.placement.transform_vector(DVec3::Z).z > 0.0, "face B is the wide one, so the board stands");

        // A plumb wall in the same case asks for nothing and keeps the standard panel.
        let (mut project, product) = empty_product(600, 560, 720);
        leaned_wall(&mut project, product, 560.0, 0.0);
        let square = plan(&project, product, Fill::WidthAndDepth, FillMode::Shape, thickness).expect("a square shelf");
        assert!(square.bevels.is_empty(), "a plumb wall needs no bevel: {:?}", square.bevels);
        assert_eq!(square.corners, square.far_corners, "so the far face is the base rim lifted");
        assert!(matches!(square.outline, PanelOutline::Rect { .. }), "and the part is the standard panel");
    }

    #[test]
    fn a_wall_leaned_the_other_way_turns_the_board_and_still_meets_it() {
        let (mut project, product) = empty_product(600, 560, 720);
        // The wall stands inside the case, so its face keeps bounding the void as it leans back.
        leaned_wall(&mut project, product, 400.0, -10.0);
        let thickness = millimetres(18);
        let shaped = plan(&project, product, Fill::WidthAndDepth, FillMode::Shape, thickness).expect("a shaped shelf");
        assert_eq!(shaped.bevels.len(), 1, "the wall still asks for its one bevel: {:?}", shaped.bevels);
        let bevel = shaped.bevels[0];
        assert_eq!(bevel.angle().millidegrees(), 10_000, "the same angle from the other side");
        // The wide face is the top here, so the board is turned about its length axis to keep the
        // broad face as the stored outline (ADR 0108).
        assert!(shaped.placement.transform_vector(DVec3::Z).z < 0.0, "the board is turned: {:?}", shaped.placement);
        let lean = 10.0_f64.to_radians();
        let (sin, cos) = lean.sin_cos();
        let normal = DVec3::new(0.0, -cos, sin);
        let distance = far_edge_distance(&shaped, bevel, normal, normal.dot(DVec3::new(0.0, 400.0, 0.0)), thickness);
        assert!(distance <= 1.0e-3, "the far edge lies in the wall's plane, off by {distance} mm");
        assert!(inside(&shaped, thickness), "and the whole board stays in the void");
    }

    #[test]
    fn a_turned_shape_starts_its_ring_at_the_panels_zero_corner() {
        // A wall that makes face A the wide one turns the board half a turn about its length axis
        // (ADR 0108). The section comes back from the niche in its own angular order, so without
        // the ring's turn the first corner sat at the far end and the panel looked turned half a
        // turn more than it is. Vertex 1 is the panel's zero corner (ADR 0105).
        let (mut project, product) = empty_product(600, 560, 720);
        leaned_wall(&mut project, product, 400.0, -10.0);
        let shaped =
            plan(&project, product, Fill::WidthAndDepth, FillMode::Shape, millimetres(18)).expect("a shaped shelf");
        assert!(shaped.placement.transform_vector(DVec3::Y).y < 0.0, "the board is turned: {:?}", shaped.placement);
        let ring = shaped.outline.contour().expect("a beveled board is a contour");
        let first = ring.segments().next().and_then(|segment| segment.start().ok()).expect("a first corner");
        assert_eq!(first, [Length::ZERO, Length::ZERO], "vertex 1 is the panel's zero corner");
        assert_eq!(shaped.corners.first(), Some(&DVec2::ZERO), "the ghost starts at the same corner");
    }

    #[test]
    fn a_void_whose_walls_lean_against_each_other_has_no_flat_board() {
        let shelf = stance(Fill::WidthAndDepth);
        let lean = 10.0_f64.to_radians();
        let (sin, cos) = lean.sin_cos();
        let narrows = NichePlane { normal: DVec3::new(0.0, -cos, -sin), offset: -cos * 100.0 };
        let widens = NichePlane { normal: DVec3::new(0.0, -cos, sin), offset: -cos * 500.0 };
        let floor = NichePlane { normal: DVec3::Z, offset: 0.0 };
        let walls = |planes: Vec<NichePlane>| Niche {
            product: Id::new(1).expect("an id"),
            local_min: DVec3::ZERO,
            size: DVec3::new(600.0, 560.0, 720.0),
            min: DVec3::ZERO,
            shape: NicheShape::Sloped,
            planes,
            corners: Vec::new(),
        };
        assert_eq!(
            reference_face(&walls(vec![narrows, widens, floor]), 100.0, 18.0, shelf),
            Err(FillRefusal::WallsDisagree),
            "one wall narrows the board at face A and the other widens it there"
        );
        assert_eq!(
            reference_face(&walls(vec![NichePlane { normal: DVec3::Z, offset: 110.0 }]), 100.0, 18.0, shelf),
            Err(FillRefusal::Wedge),
            "a floor inside the slab leaves no board at any face"
        );
        assert!(
            reference_face(&walls(vec![widens, floor]), 100.0, 18.0, shelf).is_ok_and(|stance| stance.other_face),
            "a wall that widens alone turns the board"
        );
        assert!(
            reference_face(&walls(vec![narrows, floor]), 100.0, 18.0, shelf).is_ok_and(|stance| !stance.other_face),
            "and one that narrows leaves it as it stands"
        );
    }

    #[test]
    fn a_shelf_under_a_leaned_ceiling_takes_the_slab_section_in_the_shape_mode() {
        // The ceiling is nearly level with the board, so no end face can meet it; the board still
        // fills the section the whole slab leaves, with no bevel, in both modes (ADR 0109).
        let (mut project, product) = empty_product(600, 560, 720);
        leaned_ceiling(&mut project, product);
        let thickness = millimetres(18);
        let shaped = plan(&project, product, Fill::WidthAndDepth, FillMode::Shape, thickness)
            .expect("the shape falls back to the slab section");
        assert!(shaped.bevels.is_empty(), "a level wall asks for no end face: {:?}", shaped.bevels);
        assert_eq!(shaped.corners, shaped.far_corners, "the far face is the base rim lifted");
        let fitted = plan(&project, product, Fill::WidthAndDepth, FillMode::Fit, thickness).expect("a fit plan");
        assert_eq!(shaped.nominal_size, fitted.nominal_size, "the slab section is the fit's rectangle here");
    }

    #[test]
    fn a_splayed_case_under_a_short_top_offers_every_build() {
        // The owner's case: two boards turned in the product frame and a top that does not reach
        // the open front. Every build must answer instead of refusing the void (ADR 0109).
        let (mut project, product) = splayed_product();
        let top = PanelNominalSize::new(millimetres(600), millimetres(400)).expect("a short top");
        let placement = Placement::IDENTITY
            .with_translation(millimetres(0), millimetres(0), millimetres(702))
            .expect("a top placement");
        project.add_panel(product, "Top", top, None, millimetres(18), placement).expect("add a top");
        let thickness = millimetres(18);
        for fill in [Fill::WidthAndDepth, Fill::HeightAndDepth, Fill::WidthAndHeight] {
            for mode in [FillMode::Fit, FillMode::Shape] {
                let planned = plan(&project, product, fill, mode, thickness)
                    .unwrap_or_else(|refusal| panic!("{fill:?}/{mode:?} refused: {refusal:?}"));
                assert!(planned.corners.len() >= 3, "{fill:?}/{mode:?} has a footprint");
                assert!(planned.nominal_size.length().millimetres_f64() > 0.0);
            }
        }
    }

    #[test]
    fn a_trapezoid_fits_the_largest_rectangle_and_takes_its_ring_as_the_shape() {
        let (project, product) = splayed_product();
        let thickness = millimetres(18);
        let fitted = plan(&project, product, Fill::WidthAndDepth, FillMode::Fit, thickness).expect("a fit plan");
        // The section narrows upwards, so the widest rectangle inside it starts where the walls are
        // at their narrowest: 200 mm across, the whole 560 mm depth.
        assert!((fitted.nominal_size.length().millimetres_f64() - 200.0).abs() <= 1.0e-3, "{:?}", fitted.nominal_size);
        assert!((fitted.nominal_size.width().millimetres_f64() - 560.0).abs() <= 1.0e-3, "{:?}", fitted.nominal_size);
        assert!(matches!(fitted.outline, PanelOutline::Rect { .. }), "a fit keeps the standard panel");
        assert!(!fitted.outline.has_contour_features(), "and carries no corner treatment");
        assert_eq!(fitted.corners.len(), 4, "the ghost draws a rectangle");

        let shaped = plan(&project, product, Fill::WidthAndDepth, FillMode::Shape, thickness).expect("a shape plan");
        let Some(ring) = shaped.outline.contour() else {
            panic!("the trapezoid is not a standard panel: {:?}", shaped.outline);
        };
        assert_eq!(ring.entries().len(), 4, "the trapezoid is stored as the ring the walls leave");
        assert_eq!(shaped.nominal_size, PanelNominalSize::new(millimetres(400), millimetres(560)).expect("a size"));
        assert!(shaped.corners.len() > 4 || !is_rectangle(&shaped.corners), "the shape is not the rectangle");
        assert!(shaped.bevels.is_empty(), "the posts stand plumb, so no edge needs a bevel");
    }

    /// Whether a polygon is the rectangle around it, for the shape-mode assertion above.
    fn is_rectangle(points: &[DVec2]) -> bool {
        if points.len() != 4 {
            return false;
        }
        let first = points.first().copied().unwrap_or(DVec2::ZERO);
        let low = points.iter().copied().fold(first, DVec2::min);
        let high = points.iter().copied().fold(first, DVec2::max);
        points.iter().all(|point| {
            ((point.x - low.x).abs() <= 1.0e-6 || (point.x - high.x).abs() <= 1.0e-6)
                && ((point.y - low.y).abs() <= 1.0e-6 || (point.y - high.y).abs() <= 1.0e-6)
        })
    }

    /// A corner cabinet: 600 wide, 500 deep, 720 tall, one 45-degree corner cut 100 mm along each edge.
    fn corner_cabinet() -> (Project, Id) {
        let footprint = Footprint::cut([
            None,
            Some(CornerFeatureKind::Chamfer { incoming: millimetres(100), outgoing: millimetres(100) }),
            None,
            None,
        ]);
        let envelope = Envelope::with_footprint(millimetres(600), millimetres(500), millimetres(720), footprint)
            .expect("a corner cabinet");
        let root = Container::new(Id::new(1).expect("an id"), "Root", Placement::IDENTITY, Vec::new());
        let mut project = Project::new(root).expect("an empty project");
        let root = project.root().id();
        let product = project
            .add_product(root, "Corner", ClassCode::GENERAL_PRODUCT, envelope, Placement::IDENTITY)
            .expect("a corner cabinet");
        (project, product)
    }

    /// The envelope of the open product, for the footprint tests.
    fn envelope_of(project: &Project, product: Id) -> Envelope {
        project.container(product).and_then(|product| product.envelope()).cloned().expect("a product has an envelope")
    }

    #[test]
    fn a_cut_corner_case_fits_the_largest_rectangle_inside_its_cut() {
        // The niche starts from the frame's footprint, so the cut face is a wall of the cell: the fit
        // rectangle is 500 x 500 (the full depth, one setback narrower) and not the 600 x 400 the box
        // around the cut would allow (N29, ADR 0201).
        let (project, product) = corner_cabinet();
        let thickness = millimetres(18);
        let fitted = plan(&project, product, Fill::WidthAndDepth, FillMode::Fit, thickness).expect("a fit plan");
        assert_eq!(fitted.nominal_size, PanelNominalSize::new(millimetres(500), millimetres(500)).expect("a size"));
        assert!(matches!(fitted.outline, PanelOutline::Rect { .. }), "a fit keeps the standard panel");
        assert!(inside(&fitted, thickness), "the rectangle lies inside every wall of the cell");
        let envelope = envelope_of(&project, product);
        for corner in &fitted.corners {
            let local = fitted.placement.transform_point(DVec3::new(corner.x, corner.y, 0.0));
            assert!(
                envelope.holds(DVec2::new(local.x, local.y)),
                "the fitted rectangle left the footprint at {corner:?}"
            );
        }
    }

    #[test]
    fn a_cut_corner_case_takes_its_own_outline_in_the_shape_mode() {
        let (project, product) = corner_cabinet();
        let shaped =
            plan(&project, product, Fill::WidthAndDepth, FillMode::Shape, millimetres(18)).expect("a shape plan");
        let features = shaped.outline.corner_features();
        let [feature] = features else {
            panic!("the cut is one corner feature, was {features:?}");
        };
        assert_eq!(
            feature.feature(),
            CornerFeatureKind::Chamfer { incoming: millimetres(100), outgoing: millimetres(100) },
            "the cut face is the case's own 45-degree corner"
        );
        assert_eq!(feature.corner(), VertexId::new(2).expect("the bottom-right corner"), "the cut belongs to it");
        assert_eq!(shaped.corners.len(), 5, "the shaped shelf is the pentagon the walls leave");
    }

    #[test]
    fn the_part_a_shaped_case_creates_stands_inside_its_footprint() {
        let thickness = millimetres(18);
        for fill in [Fill::WidthAndDepth, Fill::HeightAndDepth, Fill::WidthAndHeight] {
            for mode in [FillMode::Fit, FillMode::Shape] {
                // One case per part: a part left standing is material, and the next part would be
                // measured against it rather than against the frame (N36).
                let (mut project, product) = corner_cabinet();
                let envelope = envelope_of(&project, product);
                let plan = plan(&project, product, fill, mode, thickness).expect("a plan inside the cut case");
                let id = project
                    .add_panel_with_outline(
                        product,
                        "Part",
                        plan.nominal_size,
                        plan.outline.clone(),
                        &plan.bevels,
                        None,
                        thickness,
                        plan.placement,
                    )
                    .expect("the click creates the part");
                let panel = project.panel(id).expect("the part is in the tree");
                let ring = panel
                    .outline()
                    .ring_for(panel.nominal_size())
                    .expect("the stored outline draws its own ring")
                    .sample(0.25)
                    .expect("the ring samples");
                let gap = thickness.millimetres_f64();
                for point in &ring {
                    for face in [0.0, gap] {
                        let local = plan.placement.transform_point(DVec3::new(point.x, point.y, face));
                        assert!(
                            envelope.holds(DVec2::new(local.x, local.y)),
                            "{fill:?} in {mode:?} put a point of the part outside the frame: {point:?} at {face}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_rounded_corner_stands_as_the_frames_own_radius() {
        // The cell's planes take an arc by its chord, but the section a shelf is cut to keeps the frame's
        // curve (ADR 0201, N33): the part is the case's rounded corner, not the chamfer its chord spans.
        let footprint = Footprint::cut([None, Some(CornerFeatureKind::Radius { radius: millimetres(80) }), None, None]);
        let envelope = Envelope::with_footprint(millimetres(600), millimetres(500), millimetres(720), footprint)
            .expect("a rounded case");
        let root = Container::new(Id::new(1).expect("an id"), "Root", Placement::IDENTITY, Vec::new());
        let mut project = Project::new(root).expect("an empty project");
        let root = project.root().id();
        let product = project
            .add_product(root, "Rounded", ClassCode::GENERAL_PRODUCT, envelope.clone(), Placement::IDENTITY)
            .expect("a rounded case");
        let thickness = millimetres(18);
        let shaped = plan(&project, product, Fill::WidthAndDepth, FillMode::Shape, thickness).expect("a shape plan");
        let features = shaped.outline.corner_features();
        let [feature] = features else {
            panic!("the rounded corner is one corner feature, was {features:?}");
        };
        assert_eq!(
            feature.feature(),
            CornerFeatureKind::Radius { radius: millimetres(80) },
            "the frame's own radius, not its chord"
        );
        assert!(shaped.bevels.is_empty(), "a cell of plumb walls bevels nothing");
        assert_eq!(shaped.corners, shaped.far_corners, "the far face ends over the base rim");

        // What the click creates carries the radius too, and every point of it stands inside the frame.
        let id = project
            .add_panel_with_outline(
                product,
                "Shelf",
                shaped.nominal_size,
                shaped.outline.clone(),
                &shaped.bevels,
                None,
                thickness,
                shaped.placement,
            )
            .expect("the click creates the part");
        let panel = project.panel(id).expect("the part is in the tree");
        let stored = panel.outline().corner_features();
        assert_eq!(
            stored.first().map(|feature| feature.feature()),
            Some(CornerFeatureKind::Radius { radius: millimetres(80) })
        );
        let ring = panel
            .outline()
            .ring_for(panel.nominal_size())
            .expect("the stored outline draws its own ring")
            .sample(0.25)
            .expect("the ring samples");
        let gap = thickness.millimetres_f64();
        for point in &ring {
            for face in [0.0, gap] {
                let local = shaped.placement.transform_point(DVec3::new(point.x, point.y, face));
                assert!(
                    envelope.holds(DVec2::new(local.x, local.y)),
                    "the shelf left the frame it was cut to at {point:?}"
                );
            }
        }
    }

    #[test]
    fn a_notched_frame_builds_a_part_inside_the_arm_the_pointer_names() {
        // A frame drawn as any closed ring builds a part: the cell is the convex piece of the frame
        // around the pointer (N34, ADR 0203), and the part stands inside the frame it was cut to.
        let ring = Path::polyline(
            vec![
                [millimetres(0), millimetres(0)],
                [millimetres(600), millimetres(0)],
                [millimetres(600), millimetres(560)],
                [millimetres(300), millimetres(560)],
                [millimetres(300), millimetres(400)],
                [millimetres(200), millimetres(400)],
                [millimetres(200), millimetres(560)],
                [millimetres(0), millimetres(560)],
            ],
            true,
        )
        .expect("a closed ring")
        .to_contour()
        .expect("a contour");
        let envelope = Envelope::with_ring(&ring, millimetres(720)).expect("a notched frame");
        let root = Container::new(Id::new(1).expect("an id"), "Root", Placement::IDENTITY, Vec::new());
        let mut project = Project::new(root).expect("an empty project");
        let root = project.root().id();
        let product = project
            .add_product(root, "Notched", ClassCode::GENERAL_PRODUCT, envelope.clone(), Placement::IDENTITY)
            .expect("a notched frame");
        let thickness = millimetres(18);
        for mode in [FillMode::Fit, FillMode::Shape] {
            let plan = plan(&project, product, Fill::WidthAndDepth, mode, thickness).expect("a plan in the arm");
            // The pointer is the envelope's centre, which falls in the arm beside the notch.
            assert_eq!((plan.free_low, plan.free_high), (360.0, 378.0), "the part sits at the pointer's height");
            let id = project
                .add_panel_with_outline(
                    product,
                    "Part",
                    plan.nominal_size,
                    plan.outline.clone(),
                    &plan.bevels,
                    None,
                    thickness,
                    plan.placement,
                )
                .expect("the click creates the part");
            let panel = project.panel(id).expect("the part is in the tree");
            let stored = panel
                .outline()
                .ring_for(panel.nominal_size())
                .expect("the stored outline draws its own ring")
                .sample(0.25)
                .expect("the ring samples");
            let gap = thickness.millimetres_f64();
            for point in &stored {
                for face in [0.0, gap] {
                    let local = plan.placement.transform_point(DVec3::new(point.x, point.y, face));
                    assert!(
                        envelope.holds(DVec2::new(local.x, local.y)),
                        "{mode:?} put a point of the part outside the notched frame: {point:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_plain_box_gives_the_same_rectangle_in_both_modes() {
        let (project, product) = empty_product(600, 560, 720);
        let thickness = millimetres(18);
        let mut outlines = Vec::new();
        for mode in [FillMode::Fit, FillMode::Shape] {
            let plan = plan(&project, product, Fill::WidthAndDepth, mode, thickness).expect("a plan");
            assert_eq!(plan.nominal_size, PanelNominalSize::new(millimetres(600), millimetres(560)).expect("a size"));
            assert!(matches!(plan.outline, PanelOutline::Rect { .. }), "{mode:?} keeps the rectangle panel");
            assert!(plan.bevels.is_empty(), "{mode:?} asks for no bevel in a square void");
            assert_eq!(plan.corners, plan.far_corners, "{mode:?} ends the board over its own rim");
            outlines.push((plan.outline, plan.corners));
        }
        assert_eq!(outlines.first(), outlines.last(), "a rectangular void answers the same part either way");
    }

    #[test]
    fn the_fitted_rectangle_lies_inside_every_wall_over_the_whole_thickness() {
        let (project, product) = splayed_product();
        let thickness = millimetres(18);
        for fill in [Fill::WidthAndDepth, Fill::HeightAndDepth, Fill::WidthAndHeight] {
            let plan = plan(&project, product, fill, FillMode::Fit, thickness).expect("a fit plan");
            let gap = thickness.millimetres_f64();
            let mut points = Vec::new();
            for corner in &plan.corners {
                let base = plan.placement.transform_point(DVec3::new(corner.x, corner.y, 0.0));
                let far = plan.placement.transform_point(DVec3::new(corner.x, corner.y, gap));
                points.extend([base, far]);
            }
            for point in points {
                for plane in &plan.niche.planes {
                    let distance = plane.normal.dot(point) - plane.offset;
                    assert!(distance >= -2.0e-3, "{fill:?}: {:?} is {distance} from a wall", point);
                }
            }
        }
    }

    #[test]
    fn a_post_takes_the_cut_in_its_own_board_plane() {
        let (project, product) = splayed_product();
        let thickness = millimetres(18);
        let post = plan(&project, product, Fill::HeightAndDepth, FillMode::Fit, thickness).expect("a post plan");
        // A post stands up: its length is the void's height, its width the depth the slab leaves.
        assert!((post.nominal_size.length().millimetres_f64() - 720.0).abs() <= 1.0e-3, "{:?}", post.nominal_size);
        assert!(post.nominal_size.width().millimetres_f64() > 0.0);
        assert!((post.free_high - post.free_low - thickness.millimetres_f64()).abs() <= 1.0e-9, "one thickness");
    }

    #[test]
    fn a_wall_takes_the_voids_width_and_height_in_the_frontal_plane() {
        // The third pair of the fill trait (ADR 0200): a back board stands in the frontal plane, so
        // its length runs up the case and its width across the front, and the material is what the
        // pointer's own depth sets (ADR 0105).
        let (project, product) = empty_product(600, 560, 720);
        let thickness = millimetres(18);
        let close = |first: DVec3, second: DVec3| (first - second).length() <= 1.0e-9;
        for mode in [FillMode::Fit, FillMode::Shape] {
            let wall = plan(&project, product, Fill::WidthAndHeight, mode, thickness).expect("a wall fills the void");
            assert_eq!(wall.nominal_size.length(), millimetres(720), "{mode:?}: length is the niche's height");
            assert_eq!(wall.nominal_size.width(), millimetres(600), "{mode:?}: width is the niche's width");
            assert_eq!(
                wall.placement.translation(),
                DVec3::new(0.0, 280.0, 0.0),
                "{mode:?}: the node is the base corner at the pointer's depth"
            );
            assert!(close(wall.placement.transform_vector(DVec3::X), DVec3::Z), "{mode:?}: the length runs up");
            assert!(close(wall.placement.transform_vector(DVec3::Y), DVec3::X), "{mode:?}: the width runs across");
            assert!(close(wall.placement.transform_vector(DVec3::Z), DVec3::Y), "{mode:?}: the material faces front");
            assert_eq!((wall.free_low, wall.free_high), (280.0, 298.0), "{mode:?}: one thickness of depth");
            assert!(matches!(wall.outline, PanelOutline::Rect { .. }), "{mode:?}: a box keeps the rectangle panel");
            assert!(wall.bevels.is_empty(), "{mode:?}: plumb walls ask for no end face");
            assert!(wall.corners.iter().all(|corner| {
                corner.x >= -1.0e-6 && corner.x <= 720.0 + 1.0e-6 && corner.y >= -1.0e-6 && corner.y <= 600.0 + 1.0e-6
            }));
        }
        assert_eq!(free_axis(Fill::WidthAndHeight), 1, "the wall's free axis is the product's depth");
    }

    #[test]
    fn a_wall_in_a_splayed_case_takes_the_shaped_board() {
        // The posts lean in plan, so the board's two end faces are not square to them: Shape bevels
        // both ends into the posts' own planes (ADR 0107, 0108), where Fit keeps the rectangle.
        let (project, product) = splayed_product();
        let thickness = millimetres(18);
        let fitted = plan(&project, product, Fill::WidthAndHeight, FillMode::Fit, thickness).expect("a fit plan");
        assert!(matches!(fitted.outline, PanelOutline::Rect { .. }), "a fit keeps the standard panel");
        assert!((fitted.nominal_size.length().millimetres_f64() - 720.0).abs() <= 1.0e-3, "{:?}", fitted.nominal_size);
        // The posts lean in plan, so the slab is narrowest at its far face: the fit has to pass
        // through the whole thickness, and 600 − 200 · 298 / 560 is what it leaves. 300 mm at the
        // pointer's own depth would cut into both posts at the front.
        assert!((fitted.nominal_size.width().millimetres_f64() - 293.572).abs() <= 1.0e-2, "{:?}", fitted.nominal_size);

        let shaped = plan(&project, product, Fill::WidthAndHeight, FillMode::Shape, thickness).expect("a shape plan");
        assert!(shaped.outline.contour().is_some(), "a beveled board is a contour");
        assert_eq!(shaped.bevels.len(), 2, "both splayed posts ask for an end face: {:?}", shaped.bevels);
        let lean = Degrees::from_radians((100.0_f64 / 560.0).atan()).expect("a storable angle");
        for bevel in &shaped.bevels {
            assert_eq!(bevel.angle().millidegrees(), lean.millidegrees(), "the post's own lean");
        }
        assert_ne!(shaped.corners, shaped.far_corners, "the far face is set back into the walls");
        assert!(inside(&shaped, thickness), "the board stays in the void it was measured against");
        // The base rim is the wide face, 150..450 across at the pointer's depth; the end faces are
        // set back by the bevel, which leaves the far rim exactly the width the fit had to keep.
        let across = |points: &[DVec2]| {
            let low = points.iter().map(|corner| corner.y).fold(f64::INFINITY, f64::min);
            let high = points.iter().map(|corner| corner.y).fold(f64::NEG_INFINITY, f64::max);
            high - low
        };
        assert!(
            (across(&shaped.corners) - 300.0).abs() <= 1.0e-2,
            "the base rim is the wide face: {}",
            across(&shaped.corners)
        );
        assert!(
            (across(&shaped.far_corners) - fitted.nominal_size.width().millimetres_f64()).abs() <= 1.0e-2,
            "the far rim is the fitted width: {}",
            across(&shaped.far_corners)
        );
    }

    #[test]
    fn a_void_that_cannot_be_filled_says_why() {
        let (project, product) = empty_product(600, 560, 720);
        let thickness = millimetres(18);
        let outside = project.fill_plan(
            product,
            FillRequest::new(
                DVec3::new(-1000.0, -1000.0, -1000.0),
                Fill::WidthAndDepth,
                FillMode::Fit,
                thickness,
                None,
            ),
            Sight::none(),
            None,
        );
        assert_eq!(outside, Err(FillRefusal::Outside));
        let root = project.root().id();
        let wrong = project.fill_plan(
            root,
            FillRequest::new(centre(&project, product), Fill::WidthAndDepth, FillMode::Fit, thickness, None),
            Sight::none(),
            None,
        );
        assert_eq!(wrong, Err(FillRefusal::OtherProduct));
        let thin = project.fill_plan(
            product,
            FillRequest::new(
                centre(&project, product),
                Fill::WidthAndDepth,
                FillMode::Fit,
                Length::from_micrometres(1),
                None,
            ),
            Sight::none(),
            None,
        );
        assert!(thin.is_ok(), "a micrometre board still leaves a plan");
    }

    #[test]
    fn a_void_thinner_than_the_material_is_refused() {
        let (project, product) = empty_product(600, 560, 10);
        let plan = plan(&project, product, Fill::WidthAndDepth, FillMode::Fit, millimetres(18));
        assert_eq!(plan, Err(FillRefusal::TooThin));
    }

    #[test]
    fn the_largest_rectangle_of_a_convex_section_is_the_widest_one() {
        // A right triangle: the answer is the square the hypotenuse cuts, not a rectangle grown from
        // the corner, which is what a breakpoint-only search would miss.
        let triangle = vec![DVec2::new(0.0, 0.0), DVec2::new(100.0, 0.0), DVec2::new(0.0, 100.0)];
        let (corner, size) = largest_rectangle(&triangle).expect("a triangle holds a rectangle");
        assert!((size.x - 50.0).abs() <= 0.1 && (size.y - 50.0).abs() <= 0.1, "{corner:?} {size:?}");

        // The same question asked of a section that is already the rectangle: the rectangle itself.
        let box_section =
            vec![DVec2::new(0.0, 0.0), DVec2::new(200.0, 0.0), DVec2::new(200.0, 560.0), DVec2::new(0.0, 560.0)];
        let (corner, size) = largest_rectangle(&box_section).expect("a rectangle");
        assert_eq!((corner, size), (DVec2::ZERO, DVec2::new(200.0, 560.0)), "a rectangular section gives itself");

        // A diamond: the answer reaches into the slanted edges, which no vertex x-coordinate names.
        let diamond =
            vec![DVec2::new(50.0, 0.0), DVec2::new(100.0, 50.0), DVec2::new(50.0, 100.0), DVec2::new(0.0, 50.0)];
        let (corner, size) = largest_rectangle(&diamond).expect("a diamond holds a rectangle");
        assert!((size.x - 50.0).abs() <= 0.1 && (size.y - 50.0).abs() <= 0.1, "{corner:?} {size:?}");
    }

    #[test]
    fn a_section_with_no_area_holds_no_rectangle() {
        assert!(largest_rectangle(&[]).is_none());
        assert!(largest_rectangle(&[DVec2::ZERO, DVec2::new(100.0, 0.0)]).is_none());
        let line = vec![DVec2::ZERO, DVec2::new(100.0, 0.0), DVec2::new(200.0, 0.0)];
        assert!(largest_rectangle(&line).is_none(), "a line leaves no height");
    }

    #[test]
    fn a_turned_board_keeps_its_length_axis_and_takes_the_other_face() {
        for fill in [Fill::WidthAndDepth, Fill::HeightAndDepth, Fill::WidthAndHeight] {
            let stance = stance(fill);
            let turned = stance.turned();
            let (standing, over) = (stance.rotation(), turned.rotation());
            // The length axis is untouched, which is what makes the turn a placement of the same
            // stance rather than a fourth one (ADR 0105, ADR 0108).
            assert!((over.mul_vec3(DVec3::X) - standing.mul_vec3(DVec3::X)).length() <= 1.0e-9, "{fill:?}");
            assert!((over.mul_vec3(DVec3::Z) + standing.mul_vec3(DVec3::Z)).length() <= 1.0e-9, "{fill:?}");
            assert!((over.mul_vec3(DVec3::Y) + standing.mul_vec3(DVec3::Y)).length() <= 1.0e-9, "{fill:?}");
            // The base face moves to the other end of the slab the board takes.
            let (first, other) = if stance.against_z { (28.0, 10.0) } else { (10.0, 28.0) };
            assert!((stance.face(10.0, 18.0) - first).abs() <= 1.0e-9, "{fill:?}");
            assert!((turned.face(10.0, 18.0) - other).abs() <= 1.0e-9, "{fill:?}");
        }
    }

    #[test]
    fn the_free_axis_is_the_panel_thickness_axis_of_each_fill() {
        assert_eq!(free_axis(Fill::WidthAndDepth), 2);
        assert_eq!(free_axis(Fill::HeightAndDepth), 0);
        assert_eq!(free_axis(Fill::WidthAndHeight), 1);
    }
    /// A post standing across the whole depth of the case, its thickness from `x` to `x + 18`.
    fn standing_post(project: &mut Project, product: Id, x: f64) -> Id {
        let nominal = PanelNominalSize::new(millimetres(720), millimetres(560)).expect("a post size");
        let rotation = DQuat::from_rotation_y(FRAC_PI_2);
        let placement = Placement::new(DVec3::new(x, 0.0, 720.0), rotation).expect("a post placement");
        project.add_panel(product, "Post", nominal, None, millimetres(18), placement).expect("add a post")
    }

    /// The sight that switches `node` off, owned by the test so the borrow outlives the query.
    fn hiding(node: Id) -> HashSet<Id> {
        [node].into_iter().collect()
    }

    #[test]
    fn a_hidden_facade_bounds_the_part_and_not_the_void() {
        let (mut project, product) = empty_product(600, 560, 720);
        let facade = leaned_wall(&mut project, product, 0.0, 0.0);
        let point = DVec3::new(300.0, 300.0, 360.0);
        let request = FillRequest::new(point, Fill::WidthAndDepth, FillMode::Fit, millimetres(18), None);

        let visible = project.fill_plan(product, request, Sight::none(), None).expect("a shelf behind the facade");
        let hidden = hiding(facade);
        let through = project.fill_plan(product, request, Sight::of(&hidden), None).expect("the case's own void");

        assert!(
            (through.niche.size.y - visible.niche.size.y - 18.0).abs() < 1.0e-3,
            "the hidden facade is no wall of the void: {:?} against {:?}",
            through.niche.size,
            visible.niche.size
        );
        assert!(
            (through.nominal_size.width().millimetres_f64() - visible.nominal_size.width().millimetres_f64()).abs()
                < 1.0e-3,
            "the part stops at the facade's inner face either way: {:?} against {:?}",
            through.nominal_size,
            visible.nominal_size
        );
        assert!(
            (through.placement.translation() - visible.placement.translation()).length() < 1.0e-3,
            "and lands in the same place"
        );
    }

    /// A board lying across the whole case at `z`, `thickness` deep.
    fn lying_board(project: &mut Project, product: Id, z: i64, thickness: i64) -> Id {
        let nominal = PanelNominalSize::new(millimetres(600), millimetres(560)).expect("a board size");
        let placement = Placement::IDENTITY
            .with_translation(millimetres(0), millimetres(0), millimetres(z))
            .expect("a board placement");
        project.add_panel(product, "Board", nominal, None, millimetres(thickness), placement).expect("a board")
    }

    #[test]
    fn a_part_goes_around_a_hidden_body_standing_inside_the_void() {
        let (mut project, product) = empty_product(600, 560, 720);
        let post = standing_post(&mut project, product, 291.0);
        let thickness = millimetres(18);
        let hidden = hiding(post);
        let sight = Sight::of(&hidden);
        let request =
            |x: f64| FillRequest::new(DVec3::new(x, 300.0, 360.0), Fill::WidthAndDepth, FillMode::Fit, thickness, None);

        // A point beside the post: the void is cut at the post's own face and the shelf takes what
        // is left on that side of it.
        let beside = project.fill_plan(product, request(100.0), sight, None).expect("the void beside the hidden post");
        assert!(
            (beside.nominal_size.length().millimetres_f64() - 291.0).abs() < 1.0e-3,
            "the shelf stops at the post's near face: {:?}",
            beside.nominal_size
        );
        assert!(
            beside.corners.iter().all(|corner| corner.x <= 291.0 + 1.0e-3),
            "and never crosses it: {:?}",
            beside.corners
        );

        // A point the post itself stands over still plans a part: one rectangle cannot straddle the
        // body, so the larger side of what is left wins and the part is never a slice of it.
        let over = project.fill_plan(product, request(300.0), sight, None).expect("the larger side of the post");
        assert!(
            over.corners.iter().all(|corner| corner.x <= 291.0 + 1.0e-3 || corner.x >= 309.0 - 1.0e-3),
            "the part is on one side of the post: {:?}",
            over.corners
        );

        // Shown again, the post is a wall of the void and the same point is inside a niche.
        let shown = project.fill_plan(product, request(300.0), Sight::none(), None);
        assert!(shown.is_ok(), "the post's own faces bound the void when it is shown: {shown:?}");
    }

    #[test]
    fn a_part_never_runs_through_a_post_the_pointer_never_pointed_at() {
        // «В одном случае полка прошила стенку» — the owner, on a cabinet whose post stands in the far
        // arm of an L. The probes start at the pointer, so none of them met the post, and the ring the
        // part took ran through it. A body standing in the frame's own space is material of the void's
        // height, so the ring is clipped at its face (N36).
        let root = Container::new(Id::new(1).expect("an id"), "Root", Placement::IDENTITY, Vec::new());
        let mut project = Project::new(root).expect("an empty project");
        let ring = Path::polyline(
            vec![
                [Length::ZERO, Length::ZERO],
                [millimetres(600), Length::ZERO],
                [millimetres(600), millimetres(300)],
                [millimetres(300), millimetres(300)],
                [millimetres(300), millimetres(560)],
                [Length::ZERO, millimetres(560)],
            ],
            true,
        )
        .expect("a closed ring")
        .to_contour()
        .expect("a contour");
        let envelope = Envelope::with_ring(&ring, millimetres(720)).expect("a notched frame");
        let root = project.root().id();
        let product = project
            .add_product(root, "Notched", ClassCode::GENERAL_PRODUCT, envelope, Placement::IDENTITY)
            .expect("a notched product");
        // A post from the floor to the ceiling at x = 450, beyond the notch the pointer stands in.
        let nominal = PanelNominalSize::new(millimetres(720), millimetres(250)).expect("a post size");
        let placement =
            Placement::new(DVec3::new(450.0, 0.0, 720.0), DQuat::from_rotation_y(FRAC_PI_2)).expect("a post placement");
        project.add_panel(product, "Post", nominal, None, millimetres(18), placement).expect("add a post");

        // The pointer stands left of the post and past its far end, so no ray of its own meets the post
        // while the frame's own space runs right through it.
        let point = DVec3::new(400.0, 280.0, 300.0);
        let niche = project.niche(point, Sight::none(), None).expect("the void left of the post");
        assert!(niche.size.x <= 450.0 + 1.0e-3, "the post bounds the frame's own space: {:?}", niche.size);
        let plan = FillRequest::new(point, Fill::WidthAndDepth, FillMode::Shape, millimetres(18), None);
        let shaped = project.fill_plan(product, plan, Sight::none(), None).expect("a shelf beside the post");
        let ring = shaped.outline.ring_for(shaped.nominal_size).expect("the shelf's own ring");
        let reach = ring
            .sample(0.5)
            .expect("the ring samples")
            .iter()
            .map(|corner| shaped.placement.transform_point(DVec3::new(corner.x, corner.y, 0.0)).x)
            .fold(f64::MIN, f64::max);
        assert!(reach <= 450.0 + 1.0e-3, "no point of the shelf is inside the post: {reach}");
    }

    #[test]
    fn a_trimmed_shape_still_keeps_the_frames_own_radius() {
        // A hidden body stops the part at its face, and the part still takes the frame's shape: the
        // trim cuts the frame's own ring by the body's half-plane instead of swapping the ring for the
        // polygon of chords, so a rounded corner stays a radius (ADR 0204, X73).
        let root = Container::new(Id::new(1).expect("an id"), "Root", Placement::IDENTITY, Vec::new());
        let mut project = Project::new(root).expect("an empty project");
        let footprint = Footprint::cut([None, Some(CornerFeatureKind::Radius { radius: millimetres(80) }), None, None]);
        let envelope = Envelope::with_footprint(millimetres(600), millimetres(500), millimetres(720), footprint)
            .expect("a rounded case");
        let root = project.root().id();
        let product = project
            .add_product(root, "Rounded", ClassCode::GENERAL_PRODUCT, envelope, Placement::IDENTITY)
            .expect("a rounded product");
        // A sheet standing across the depth between x = 100 and x = 118, hidden from the eye and
        // present to the saw; the pointer is on its far side, where the frame's radius is.
        let nominal = PanelNominalSize::new(millimetres(720), millimetres(500)).expect("a sheet size");
        let placement = Placement::new(DVec3::new(100.0, 0.0, 720.0), DQuat::from_rotation_y(FRAC_PI_2))
            .expect("a sheet placement");
        let sheet =
            project.add_panel(product, "Sheet", nominal, None, millimetres(18), placement).expect("add a sheet");
        let hidden = hiding(sheet);
        let request = FillRequest::new(
            DVec3::new(400.0, 300.0, 300.0),
            Fill::WidthAndDepth,
            FillMode::Shape,
            millimetres(18),
            None,
        );
        let shaped = project.fill_plan(product, request, Sight::of(&hidden), None).expect("a trimmed shape");
        let ring = shaped.outline.ring_for(shaped.nominal_size).expect("the outline's ring");
        assert!(
            ring.segments().iter().any(|segment| matches!(segment, geom::Segment::Arc(_))),
            "the frame's rounded corner survives the trim: {:?}",
            shaped.outline
        );
        assert!(
            (shaped.nominal_size.length().millimetres_f64() - 482.0).abs() < 1.0e-3,
            "the sheet is the near wall: {:?}",
            shaped.nominal_size
        );
    }

    #[test]
    fn a_hidden_body_that_spans_the_whole_void_leaves_nothing_to_fill() {
        let (mut project, product) = empty_product(600, 560, 720);
        let board = lying_board(&mut project, product, 350, 18);
        let hidden = hiding(board);
        let planned = project.fill_plan(
            product,
            FillRequest::new(
                DVec3::new(300.0, 280.0, 360.0),
                Fill::WidthAndDepth,
                FillMode::Fit,
                millimetres(18),
                None,
            ),
            Sight::of(&hidden),
            None,
        );
        assert_eq!(planned, Err(FillRefusal::TooSmall), "the cut leaves no area at that height");
        let shown = project.fill_plan(
            product,
            FillRequest::new(
                DVec3::new(300.0, 280.0, 360.0),
                Fill::WidthAndDepth,
                FillMode::Fit,
                millimetres(18),
                None,
            ),
            Sight::none(),
            None,
        );
        assert!(shown.is_ok(), "and the board's own faces leave two niches when it is shown: {shown:?}");
    }
}

#[cfg(test)]
mod turned {
    #![allow(clippy::expect_used)]
    use super::*;
    use crate::fixtures::*;

    #[test]
    fn a_splayed_case_turned_in_two_axes_offers_every_build() {
        let root = Container::new(Id::new(1).expect("an id"), "Root", Placement::IDENTITY, Vec::new());
        let mut project = Project::new(root).expect("a project");
        let envelope = Envelope::new(millimetres(600), millimetres(560), millimetres(720)).expect("an envelope");
        let product = project
            .add_product(project.root().id(), "Piece", ClassCode::GENERAL_PRODUCT, envelope, Placement::IDENTITY)
            .expect("a product");
        let slant = 100.0 / 560.0_f64.hypot(100.0);
        let drop = 560.0 / 560.0_f64.hypot(100.0);
        for (through, normal) in [((100.0, 0.0), (drop, -slant)), ((500.0, 0.0), (-drop, -slant))] {
            let direction = (-normal.1, normal.0);
            let start_y = 280.0 - 600.0 * direction.1;
            let start_x = through.0 + (start_y - through.1) * (direction.0 / direction.1);
            let rotation = DQuat::from_rotation_y(10.0_f64.to_radians())
                * DQuat::from_rotation_z(normal.1.atan2(normal.0))
                * DQuat::from_rotation_y(std::f64::consts::FRAC_PI_2);
            let translation = DVec3::new(start_x, start_y, 720.0) - DVec3::new(normal.0, normal.1, 0.0) * 18.0;
            let placement = Placement::new(translation, rotation).expect("a post placement");
            let size = PanelNominalSize::new(millimetres(720), millimetres(1200)).expect("a post size");
            project.add_panel(product, "Post", size, None, millimetres(18), placement).expect("the post lands");
        }
        let top = PanelNominalSize::new(millimetres(600), millimetres(400)).expect("a short top");
        let placement = Placement::IDENTITY
            .with_translation(millimetres(0), millimetres(0), millimetres(702))
            .expect("a top placement");
        project.add_panel(product, "Top", top, None, millimetres(18), placement).expect("add a top");
        let point = DVec3::new(300.0, 280.0, 360.0);
        for fill in [Fill::WidthAndDepth, Fill::HeightAndDepth, Fill::WidthAndHeight] {
            for mode in [FillMode::Fit, FillMode::Shape] {
                let planned = project
                    .fill_plan(product, FillRequest::new(point, fill, mode, millimetres(18), None), Sight::none(), None)
                    .unwrap_or_else(|refusal| panic!("{fill:?}/{mode:?} refused: {refusal:?}"));
                assert!(planned.corners.len() >= 3, "{fill:?}/{mode:?} has a footprint");
            }
        }
    }
}
