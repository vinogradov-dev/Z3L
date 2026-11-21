//! The niche probe: the free space around a point, as the cell its walls bound.
//!
//! A niche is the space a piece of furniture reserves (ADR 0099); it is derived from the document
//! every time it is asked for and never stored (ADR 0102, ADR 0103). Two questions answer it:
//!
//! - [`Project::probe`] casts one ray from a point and reports the face it stops at — its distance,
//!   its plane and the furniture entity that owns it: a panel face, a geometry body, another
//!   product's shell, or a virtual face of the envelope of the piece the point sits in;
//! - [`Project::niche`] builds the free space around the point as a convex cell: the product's six
//!   envelope planes, clipped by the plane of every face a probe meets, then probed again from the
//!   cell's own corners until a round adds no plane. The cell answers the box the callouts draw,
//!   the supporting planes and corners a tool measures against, and the section a board of a given
//!   thickness takes ([`Niche::cut`]).
//!
//! The cell is why a board turned in space is measured at all: a leaned back wall, a ceiling that
//! comes down towards the back, a piece turned off the world axes (ADR 0106). The cell is built in
//! the product's own frame, so a turned product is measured like any other. A void that is not
//! convex — one holding something the cell would swallow — is refused with a reason the tool can
//! report rather than approximated.
//!
//! Both are pure queries: no session, no renderer, no document write. The surfaces are the same
//! boxes the object move rule measures against (`super::surface`), so the tool that fills a
//! niche and the tool that drags a piece cannot disagree about where a surface is. Where the box *is*
//! the body — a panel, a cuboid, a shell — the probe reads that box, which is exact to the micrometre;
//! only a body the box cannot describe, one whose own surface leaves a hole or never reaches the
//! corners of its box, is asked of the contact port (X92, ADR 0181).

use std::sync::OnceLock;

use geom::{Contact, Contour as Ring, DVec2, DVec3, Pose, Region, Segment, Shared};

use super::contact_shape;
use super::surface::{FaceHit, SurfaceBox, SurfaceKind, surface_box};
use crate::tree::object_bounds_mm;
use crate::*;

/// Tolerance for a measurement that is already a whole micrometre, in millimetres.
const SECTION_TOLERANCE: f64 = 1.0e-6;

/// Smallest share of a plane's normal that must lie in the plane it is cut with.
///
/// A plane almost level with the cut answers a line far outside the void, so it is left to the
/// constant test instead of producing a corner at a huge coordinate.
const SECTION_NORMAL: f64 = 1.0e-3;

/// Smallest sine of the angle between two planes that still meet in a corner or a line.
const SECTION_ANGLE: f64 = 1.0e-6;

/// Cosine above which two plane normals count as the same direction.
const PLANE_PARALLEL: f64 = 1.0 - 1.0e-6;

/// Cosine above which a plane's normal counts as level with an axis.
const NORMAL_LEVEL: f64 = 1.0e-6;

/// Tolerance for comparing a cell corner with a plane, a face or a box, in millimetres.
///
/// The probes report whole micrometres, so a corner where two walls meet can land a micrometre
/// outside the face that owns one of them: that is rounding, not a gap.
const COORDINATE_TOLERANCE: f64 = 2.0e-3;

/// The rounds one cell build may run before it refuses to spin.
const MAX_CELL_ROUNDS: usize = 6;

/// The supporting planes one cell may carry.
///
/// The envelope's footprint seeds the cell with one plane per ring edge, so a drawn ring already spends
/// its own sides before a wall of the case is measured: the budget holds a frame of up to thirty sides
/// and the floor and ceiling, and a ring past it is refused rather than measured with a wall dropped.
const MAX_CELL_PLANES: usize = 32;

/// The directions one round may probe.
const MAX_CELL_PROBES: usize = 64;

/// The sine of a right turn a footprint's ring may not make, which is a shape no one cell can hold.
///
/// The cell is planes only (ADR 0106), and a ring made of straight pieces and left turns is the one
/// shape those planes draw exactly.
const CONVEX_TURN: f64 = 1.0e-9;

/// The corners one cell or one cut may have.
const MAX_CELL_CORNERS: usize = 64;

/// What one probe stopped at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProbeHit {
    /// The node that owns the surface: a product for its shell or envelope, a panel, an object.
    pub id: Id,
    /// Which kind of surface the probe met.
    pub kind: SurfaceKind,
}

/// The face a probe met, in world millimetres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProbeFace {
    /// The point where the ray meets the face.
    pub point: DVec3,
    /// The face's unit normal, pointing back along the ray.
    ///
    /// The free space therefore lies in the half-space `normal · (point - hit) >= 0`, which is what
    /// lets a cell be built from probed faces.
    pub normal: DVec3,
}

/// The answer of one probe: the distance to the first face and what it belongs to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Probe {
    /// The distance along the probe direction, in millimetres, a whole number of micrometres.
    pub distance_mm: f64,
    /// The surface the ray stopped at.
    pub hit: ProbeHit,
    /// The face the ray stopped at.
    pub face: ProbeFace,
}

/// One supporting plane of a niche's free space, in the product's own frame.
///
/// The free space lies in the half-space `normal · point >= offset`: the normal points into the
/// void and the offset is in millimetres, so every point of the cell satisfies every plane of
/// [`Niche::planes`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NichePlane {
    /// The plane's unit normal, pointing into the free space.
    pub normal: DVec3,
    /// The plane's offset along that normal, in millimetres.
    pub offset: f64,
}

impl NichePlane {
    /// The signed distance from `point` to this plane, positive on the free side.
    fn distance_to(self, point: DVec3) -> f64 {
        self.normal.dot(point) - self.offset
    }
}

/// The shape of a niche.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NicheShape {
    /// The free space is exactly the box its corners span.
    Box,
    /// The free space is one section from its floor to its ceiling, stored as a ring in the
    /// product's own XY plane.
    ///
    /// The ring is the outline a part lying in that plane takes: a shelf in a case whose posts stand
    /// at an angle is a trapezoid, not the rectangle around it (ADR 0099). Its coordinates are the
    /// product frame's, in the document's micrometres.
    Section(Contour),
    /// The free space is a cell whose section changes along the product's height.
    ///
    /// A ceiling that comes down towards the back, a wall leaned across the section: one ring would
    /// be false somewhere in the void, so the planes and corners answer instead, and a board takes
    /// the cut it needs ([`Niche::cut`]).
    Sloped,
}

/// Why a point has no usable niche.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NicheRefusal {
    /// The point is outside every product, or is not a usable number.
    Outside,
    /// The free space around the point is not one convex cell: it holds something, or a wall does
    /// not reach the whole face the cell would need.
    Unrepresentable,
    /// The product's own footprint is not a convex ring, so the space it reserves is not one cell.
    ///
    /// A frame drawn as a ring with a notch — or with an arc that bulges into its own shape — reserves
    /// a space the cell's planes cannot bound, and a board cut to an approximation of it would stand in
    /// material the frame never gave away (ADR 0201).
    Footprint,
}

/// The empty space around a point inside a product.
///
/// Everything here is derived: the query stores nothing in the document. `local_min` and `size` are
/// the box the free space occupies, in the product's own frame, which is the frame a panel is placed
/// in; `min` is the same corner in world millimetres, so a tool never re-derives a position from a
/// cursor and a size. `planes` and `corners` are the exact cell, and `shape` names the case a board
/// reads directly.
#[derive(Clone, Debug, PartialEq)]
pub struct Niche {
    /// The product whose void was measured.
    pub product: Id,
    /// The box's smallest corner in the product's own frame, millimetres.
    pub local_min: DVec3,
    /// The box's size along the product's own axes, millimetres.
    pub size: DVec3,
    /// The same smallest corner in world millimetres.
    pub min: DVec3,
    /// The shape of the free space.
    pub shape: NicheShape,
    /// The cell's supporting planes, in the product's own frame.
    ///
    /// Together with [`Niche::corners`] this is the exact free space: every plane's free half-space,
    /// intersected.
    pub planes: Vec<NichePlane>,
    /// The cell's corners, in the product's own frame and whole micrometres.
    pub corners: Vec<DVec3>,
}

impl Niche {
    /// The box's far corner in the product's own frame, millimetres.
    pub fn local_max(&self) -> DVec3 {
        self.local_min + self.size
    }

    /// One side of the box along the product's own axes, in millimetres.
    pub fn side_mm(&self, axis: usize) -> f64 {
        match axis {
            0 => self.size.x,
            1 => self.size.y,
            _ => self.size.z,
        }
    }

    /// The free section's polygon, when the free space is one section from floor to ceiling.
    ///
    /// The ring is in the product's own XY plane, the plane a shelf's outline lies in.
    pub fn section(&self) -> Option<&Contour> {
        match &self.shape {
            NicheShape::Section(ring) => Some(ring),
            NicheShape::Box | NicheShape::Sloped => None,
        }
    }

    /// The free section as the polygon its chords draw, in the product's own frame, millimetres.
    ///
    /// A shelf searches its largest rectangle here rather than in the cell's planes: the shape is the
    /// frame's whole reserved ring and not the arm a pointer happens to stand in (ADR 0204). Each arc
    /// answers its own start, which is the chord — the wall a rect is measured against, and inside the
    /// frame wherever the frame's own curve bulges outward (ADR 0201).
    pub fn section_polygon(&self) -> Option<Vec<DVec2>> {
        let section = self.section()?.to_geometry().ok()?;
        let points: Vec<DVec2> = section.segments().iter().map(|segment| segment.start()).collect();
        (points.len() >= 3).then_some(points)
    }

    /// The free space a board of the given thickness takes, as a convex polygon.
    ///
    /// The cell is cut by the slab between `low` and `high` along the product's `axis` and the cut is
    /// projected on the two other axes, in `(u, v)` order: a shelf asks for `axis = 2` and
    /// `(u, v) = (0, 1)`, a board standing across the void for `axis = 0` and `(u, v) = (2, 1)`.
    /// Every plane's half-space is evaluated at the slab's own two ends, so a wall that leans across
    /// the slab limits the part at the face that would cut into it.
    ///
    /// The points are whole micrometres in the product's frame, running counter-clockwise in
    /// `(u, v)`. A slab that leaves no area — outside the void, or thinner than the gap the walls
    /// leave — answers `None`, as does an impossible axis order.
    pub fn cut(&self, axis: usize, (low, high): (f64, f64), (u, v): (usize, usize)) -> Option<Vec<DVec2>> {
        if !low.is_finite() || !high.is_finite() {
            return None;
        }
        let polygon = cut_planes(&self.planes, axis, low, high, u, v)?;
        Some(polygon.into_iter().map(quantised_point).collect())
    }
}

/// The surfaces one point is measured against.
struct Surroundings<'a> {
    /// The innermost envelope the point sits in, measured as an envelope rather than a shell.
    own: Option<(Id, SurfaceBox)>,
    /// Every other surface a ray can stop at: the bodies and the other products' shells.
    surfaces: Vec<Surface<'a>>,
}

/// One surface a probe may stop at, and the body behind it when the box cannot describe that body.
///
/// A panel, a product's envelope and a product's shell carry no geometry at all: the box *is* the
/// body, and the probe answers from it to the micrometre, which is what a cell cut into boards needs
/// (X92, ADR 0181). A body is built when a ray of one question first reaches for it and is then shared
/// by every other ray of that question, so a scene of a few hundred bodies builds only the handful a
/// probe could actually meet.
pub(super) struct Surface<'a> {
    /// The box the document keeps for this surface: the coarse reading every ray starts from.
    pub(super) box_: SurfaceBox,
    /// The geometry the body's own surface is built from, when the box may not describe it.
    geometry: Option<&'a Geometry>,
    /// The body the contact port measures, once a ray has asked for it.
    ///
    /// `None` inside means the box was the answer after all: no adapter, or a surface the box *is*.
    exact: OnceLock<Option<Shared>>,
}

impl<'a> Surface<'a> {
    /// A surface whose own body is the adapter's to measure.
    const fn body(box_: SurfaceBox, geometry: &'a Geometry) -> Self {
        Self { box_, geometry: Some(geometry), exact: OnceLock::new() }
    }

    /// A surface the box describes: a panel, an envelope, another product's shell.
    const fn boxed(box_: SurfaceBox) -> Self {
        Self { box_, geometry: None, exact: OnceLock::new() }
    }

    /// The face of this surface a ray meets, in world millimetres.
    ///
    /// The box answers first, because a body lies inside its box: a ray whose box it misses cannot meet
    /// the body at all. A surface whose own surface is not that box is then asked of the port, and a
    /// ray the body misses — down a hole, past the corner of a ball — meets no face, where the box
    /// would have answered with a wall the maker cannot see.
    fn face_hit(&self, point: DVec3, direction: DVec3, contact: Option<&dyn Contact>) -> Option<FaceHit> {
        let boxed = self.box_.face_hit(point, direction)?;
        let Some(body) = self.exact_body(contact) else {
            return Some(boxed);
        };
        let impact = body.ray(point, direction)?;
        if !(impact.distance_mm.is_finite() && impact.point.is_finite() && impact.normal.is_finite()) {
            return None;
        }
        // The axis is the box's own face, which the broad phase has already named: a body's own face
        // is not one of the box's six and this field is the box's reading of the surface.
        Some(FaceHit { distance: impact.distance_mm, point: impact.point, normal: impact.normal, axis: boxed.axis })
    }

    /// The body the adapter measures this surface as, or `None` when the box is the honest reading.
    ///
    /// The decision is the shape builder's ([`contact_shape::shape_beyond_box`]) and it is taken once,
    /// where the shape is built: a surface the box describes keeps the reading it always had.
    fn exact_body(&self, contact: Option<&dyn Contact>) -> Option<Shared> {
        let contact = contact?;
        let geometry = self.geometry?;
        self.exact
            .get_or_init(|| {
                let shape = contact_shape::shape_beyond_box(geometry, self.box_.local_min, self.box_.local_max)?;
                contact.prepare(&shape, Pose::new(self.box_.world.translation(), self.box_.world.rotation()))
            })
            .clone()
    }
}

/// The free space around a point, as the convex cell its supporting planes bound.
///
/// The corners are always derived from the planes, so a plane and the cell it cuts cannot disagree.
#[derive(Clone, Debug)]
struct Cell {
    planes: Vec<NichePlane>,
    /// The corners, sorted by their coordinates so the same cell always reports the same list.
    corners: Vec<DVec3>,
    /// The frame's own ring, and where in [`Cell::planes`] the planes a probe added begin.
    ///
    /// The planes the ring seeds are its **chords** (ADR 0201), which is the safe wall for a probe and
    /// an approximation of a curved edge. A section a board is cut to keeps the ring's own curve, so the
    /// ring is kept beside those planes and clipped by the walls the probes added — never by the chords
    /// it seeded. `None` for a rectangular frame, whose planes already are its ring (ADR 0201, N33).
    ring: Option<(Region, usize)>,
    /// The pointer's own place in the product frame, in millimetres.
    ///
    /// Every wall clips the ring **around** this point: a wall standing in one arm of an L bounds the
    /// part the pointer names and leaves the other arm's space alone (ADR 0204).
    point: DVec2,
    /// One flag per plane: whether it is the chord over an **arc** of the frame's ring.
    ///
    /// Such a plane is a conservative inner wall of the cell, because a chord lies inside the arc it
    /// spans. The ring itself is the boundary there, so a point of the bulge between a chord and its arc
    /// is inside the cell — the frame reserves it — and the flag keeps that plane out of the
    /// containment test while the cut, the corners and the probes keep the wall (N34).
    arc_chords: Vec<bool>,
}

impl Cell {
    /// The cell inside a product's envelope: its footprint's walls, floor and ceiling, before any part
    /// is met.
    ///
    /// The seed is the frame's own section and not the box around it (ADR 0159, ADR 0201): a rectangle
    /// answers the six planes it always did, one per side and one per height face, while a cut corner
    /// answers one upright plane per edge, so a part measured against it cannot grow into the corner the
    /// frame cut away. A ring drawn as any closed shape answers two readings of itself (ADR 0204): the
    /// **ring** is the space the frame reserves, whole, which is the shape a part takes; the **piece** is
    /// the convex cell around `point` (ADR 0203), which is what the planes, the corners and every stance
    /// but a shelf's are cut from. A ring still not convex as a piece is the footprint's own refusal, and
    /// so is a ring too fine for the plane budget — never a cell the walls were quietly dropped from.
    fn footprint(envelope: &Envelope, point: DVec2) -> Result<Self, NicheRefusal> {
        let outline =
            envelope.footprint().outline(envelope.x(), envelope.y()).map_err(|_| NicheRefusal::Unrepresentable)?;
        // Two readings of one frame: the ring is the space it reserves, whole and any shape; the piece
        // is the convex cell the pointer names, which is what the planes, the corners and every stance
        // but a shelf's are cut from (ADR 0203, ADR 0204).
        let whole = frame_ring(&outline)?;
        let piece = convex_piece(&whole, point)?;
        let polygon = chord_polygon(&piece);
        if polygon.len().saturating_add(2) > MAX_CELL_PLANES {
            return Err(NicheRefusal::Unrepresentable);
        }
        let mut planes: Vec<NichePlane> = Vec::with_capacity(polygon.len().saturating_add(2));
        let mut arc_chords: Vec<bool> = Vec::with_capacity(polygon.len().saturating_add(2));
        let segments = piece.segments();
        for index in 0..polygon.len() {
            let (Some(start), Some(end)) = (polygon.get(index), polygon.get((index + 1) % polygon.len())) else {
                return Err(NicheRefusal::Unrepresentable);
            };
            let edge = *end - *start;
            let inward = DVec2::new(-edge.y, edge.x).try_normalize().ok_or(NicheRefusal::Unrepresentable)?;
            planes.push(NichePlane { normal: DVec3::new(inward.x, inward.y, 0.0), offset: inward.dot(*start) });
            arc_chords.push(matches!(segments.get(index), Some(Segment::Arc(_))));
        }
        let height = envelope.z().millimetres_f64();
        planes.push(NichePlane { normal: DVec3::Z, offset: 0.0 });
        planes.push(NichePlane { normal: DVec3::NEG_Z, offset: -height });
        arc_chords.push(false);
        arc_chords.push(false);
        let mut cell = Self { planes, corners: Vec::new(), ring: None, point, arc_chords };
        if !cell.rebuild() {
            return Err(NicheRefusal::Unrepresentable);
        }
        // A box needs no second reading of its shape: the planes a rectangle seeds are its ring, to the
        // micrometre, and its cases must answer exactly what they always did (N33). Any other frame
        // reserves the shape it draws, so the ring is kept whole — an L answers an L, not the arm the
        // pointer happens to stand in (ADR 0204).
        if !envelope.footprint().is_rectangle() {
            let region = Region::new(whole, Vec::new()).map_err(|_| NicheRefusal::Unrepresentable)?;
            cell.ring = Some((region, cell.planes.len()));
        }
        Ok(cell)
    }

    /// The exact section the cell keeps from its floor to its ceiling, when its planes are chords.
    ///
    /// `None` when the cell has no ring of its own — a rectangular frame, whose section the planes
    /// already answer exactly. Otherwise the ring is clipped by every plane a probe added: a wall level
    /// with the section bounds no edge of it, and the height planes are the slab `cell_shape` already
    /// asked for. The clip is exact (`Region::clip_half_plane`), so a curve of the frame survives it and
    /// only a wall cuts it — the arc a row of radii draws is never replaced by its chord (N33).
    fn exact_section(&self) -> Option<Result<Region, NicheRefusal>> {
        let (ring, probed_at) = self.ring.as_ref()?;
        let mut section = ring.clone();
        for plane in self.planes.get(*probed_at..)? {
            let flat = DVec2::new(plane.normal.x, plane.normal.y);
            if flat.length() <= SECTION_NORMAL {
                continue;
            }
            // A wall bounds the free space the pointer is in: the clip keeps the piece beside it and
            // leaves the rest of a shape that is not convex alone (ADR 0204).
            match section.clip_half_plane_around(flat, plane.offset, self.point) {
                Ok(Some(clipped)) => section = clipped,
                // A wall the piece has already closed over leaves no section to answer: the cell's own
                // planes are the reading then, not a refusal of a void the pointer stands in.
                Ok(None) => return None,
                Err(_) => return Some(Err(NicheRefusal::Unrepresentable)),
            }
        }
        Some(Ok(section))
    }

    /// Rebuilds the corners from the planes, answering whether the planes bound a usable cell.
    ///
    /// Every triple of planes that meets inside all of them is a corner: a convex cell is the
    /// intersection of its planes, and it needs at least four corners to enclose anything.
    fn rebuild(&mut self) -> bool {
        let mut corners: Vec<DVec3> = Vec::new();
        for (index, first) in self.planes.iter().enumerate() {
            for (second_index, second) in self.planes.iter().enumerate().skip(index.saturating_add(1)) {
                for third in self.planes.iter().skip(second_index.saturating_add(1)) {
                    let Some(point) = corner_of(first, second, third) else {
                        continue;
                    };
                    if self.planes.iter().any(|plane| plane.distance_to(point) < -COORDINATE_TOLERANCE) {
                        continue;
                    }
                    if corners.iter().any(|seen| (*seen - point).length() <= COORDINATE_TOLERANCE) {
                        continue;
                    }
                    corners.push(point);
                }
            }
        }
        if corners.len() < 4 || corners.len() > MAX_CELL_CORNERS {
            self.corners.clear();
            return false;
        }
        corners.sort_by(corner_order);
        self.corners = corners;
        true
    }

    /// Whether the point lies in the cell: in its planes, or in the ring it keeps beside them.
    ///
    /// A cell that keeps no ring is its planes, to the micrometre. One that keeps a ring is asked its
    /// ring and its two heights instead, because the planes cannot describe a shape that is not convex:
    /// they are the chords of the pointer's piece, and a piece is smaller than the frame it came from
    /// (N34, ADR 0203, ADR 0204).
    fn contains(&self, point: DVec3) -> bool {
        let Some((ring, _)) = self.ring.as_ref() else {
            return self.corners.len() >= 4
                && self.planes.iter().all(|plane| plane.distance_to(point) >= -COORDINATE_TOLERANCE);
        };
        let Some((low, high)) = self.heights() else {
            return false;
        };
        if point.z < low - COORDINATE_TOLERANCE || point.z > high + COORDINATE_TOLERANCE {
            return false;
        }
        let local = DVec2::new(point.x, point.y);
        ring.contains(local) || ring.boundary_distance(local) <= COORDINATE_TOLERANCE
    }

    /// The two heights the cell spans, when every plane is flat or upright.
    ///
    /// A slanted wall crosses a height, so a cell that has one answers no pair: its free space is not
    /// one section and nothing that needs a ring asks it (`Cell::is_prism` is the same test).
    fn heights(&self) -> Option<(f64, f64)> {
        let mut low = f64::NEG_INFINITY;
        let mut high = f64::INFINITY;
        for plane in &self.planes {
            let normal = plane.normal;
            // An upright plane is a wall of the section and bounds no height.
            if normal.z.abs() <= NORMAL_LEVEL {
                continue;
            }
            // One that leans across a wall and the height is not a prism: no pair of heights describes it.
            if normal.x.abs() > NORMAL_LEVEL || normal.y.abs() > NORMAL_LEVEL {
                return None;
            }
            if normal.z > 0.0 {
                low = low.max(plane.offset);
            } else {
                high = high.min(-plane.offset);
            }
        }
        (low.is_finite() && high.is_finite() && high > low).then_some((low, high))
    }

    /// The box the cell's ring spans, with its two heights: the space the frame reserves, whole.
    fn region_bounds(&self) -> Option<(DVec2, DVec2, f64, f64)> {
        let (ring, _) = self.ring.as_ref()?;
        let (low, high) = ring.bounds()?;
        let (bottom, top) = self.heights()?;
        Some((low, high, bottom, top))
    }

    /// The ring's own corners at the floor and the ceiling: the cell as a tool reads its shape.
    fn region_corners(&self) -> Option<Vec<DVec3>> {
        let (ring, _) = self.ring.as_ref()?;
        let (_, _, bottom, top) = self.region_bounds()?;
        let mut corners = Vec::new();
        for segment in ring.outer().segments() {
            let start = segment.start();
            for z in [bottom, top] {
                corners.push(DVec3::new(start.x, start.y, z));
            }
        }
        (!corners.is_empty()).then_some(corners)
    }

    /// Whether the cell is one section from its floor to its ceiling, so one ring describes it.
    ///
    /// A cell whose planes are all either flat or upright is the section between two heights; one
    /// whose plane leans across both is not.
    fn is_prism(&self) -> bool {
        self.planes.iter().all(|plane| {
            let normal = plane.normal;
            let flat = normal.x.abs() <= NORMAL_LEVEL && normal.y.abs() <= NORMAL_LEVEL;
            let upright = normal.z.abs() <= NORMAL_LEVEL;
            flat || upright
        })
    }

    /// Whether `plane` removes any part of the cell; a plane the cell already lies on changes nothing.
    fn cuts(&self, plane: NichePlane) -> bool {
        let known = self.planes.iter().any(|existing| {
            existing.normal.dot(plane.normal) >= PLANE_PARALLEL
                && (existing.offset - plane.offset).abs() <= COORDINATE_TOLERANCE
        });
        if known {
            return false;
        }
        self.corners.iter().any(|corner| plane.distance_to(*corner) < -COORDINATE_TOLERANCE)
    }

    /// The cell cut by one more plane, or `None` when the cut leaves no cell or one plane too many.
    fn clipped(&self, plane: NichePlane) -> Option<Self> {
        if self.planes.len() >= MAX_CELL_PLANES {
            return None;
        }
        let mut cut = self.clone();
        cut.planes.push(plane);
        cut.arc_chords.push(false);
        cut.rebuild().then_some(cut)
    }

    /// The directions the next round probes from `point` in the product's frame.
    ///
    /// Each face normal finds the wall behind it and each corner finds whatever cuts the corner off.
    /// The first round also probes the envelope's twenty-six axis, edge and corner directions, so a
    /// wall standing between two cell corners is met before any corner exists to point at it.
    fn directions(&self, point: DVec3, first: bool) -> Vec<DVec3> {
        let mut directions: Vec<DVec3> = Vec::new();
        let mut add = |direction: DVec3| {
            let Some(unit) = direction.try_normalize() else {
                return;
            };
            if directions.len() >= MAX_CELL_PROBES {
                return;
            }
            if directions.iter().any(|seen| (*seen - unit).length() <= SECTION_ANGLE) {
                return;
            }
            directions.push(unit);
        };
        for plane in &self.planes {
            add(plane.normal);
        }
        for corner in &self.corners {
            add(*corner - point);
        }
        if first {
            for x in -1..=1 {
                for y in -1..=1 {
                    for z in -1..=1 {
                        let direction = DVec3::new(x as f64, y as f64, z as f64);
                        if direction.length_squared() > 0.0 {
                            add(direction);
                        }
                    }
                }
            }
        }
        directions
    }
}

impl Project {
    /// The distance from `point` to the first furniture face along `direction`.
    ///
    /// The ray stops at a panel face, a geometry body, another product's shell, or a virtual face of
    /// the envelope of the innermost product that contains the point. It never passes through a
    /// surface (ADR 0102) and it is measured in world millimetres, quantised to the document's
    /// micrometre. A point that is not finite, a direction that is zero or not finite, and a ray
    /// that meets nothing all answer `None`.
    ///
    /// `sight` is what the ray may pass: a part the user switched off is not there for the eye, so
    /// the ray crosses it and stops at the next face behind it (X73, ADR 0175). A caller that
    /// measures *material* — the distance fields of a drag — hands [`Sight::none`] and keeps every
    /// face in the way.
    ///
    /// `contact` is the geometry adapter that measures a body the boxes cannot describe: a ray passes
    /// through the hole in a plate and past the corner of a ball's box, and stops at the face behind
    /// it. Every other surface — a panel, a cuboid, a product's shell — is measured by its box, which
    /// is exact to the micrometre, so an ordinary cell keeps the planes it always had (X92, ADR 0181).
    /// A caller with no adapter keeps the box reading everywhere.
    pub fn probe(&self, point: DVec3, direction: DVec3, sight: Sight, contact: Option<&dyn Contact>) -> Option<Probe> {
        let surroundings = self.surroundings_around(point, sight);
        self.probe_within(point, direction, &surroundings, contact)
    }

    /// The empty space around `point`, as the convex cell its walls bound.
    ///
    /// The product is the innermost envelope that contains the point, and the cell is built in that
    /// product's own frame: the envelope's six planes, clipped by the plane of every face a probe
    /// meets, probed again from the cell's own corners until a round adds no plane. A rectangular
    /// case answers [`NicheShape::Box`]; a case with splayed posts answers the trapezoid as
    /// [`NicheShape::Section`]; a leaned wall or ceiling answers [`NicheShape::Sloped`] with the cell
    /// itself. A point outside every product is refused, and so is a void that holds a surface the
    /// cell did not cut against (ADR 0099, ADR 0106, ADR 0109). A wall bounds the void where its face
    /// stands, not only when it covers a whole face of the envelope: an open front under a short top
    /// still answers.
    ///
    /// `sight` is what the cell is measured through: a part the user switched off is not a wall of
    /// the void and does not make it non-convex, so the cell spans the space the eye reads (X73,
    /// ADR 0175). A hidden body standing in the cell is a boundary of the part that fills it, which
    /// the fill plan trims against, and never a slice out of the void.
    ///
    /// `contact` is the adapter that measures the walls the boxes cannot describe, exactly as
    /// [`Project::probe`] reads it: a splayed post, a plate with a hole and a ball bound the cell by
    /// their own faces, while every surface the box *is* keeps the box reading (X92, ADR 0181).
    pub fn niche(&self, point: DVec3, sight: Sight, contact: Option<&dyn Contact>) -> Result<Niche, NicheRefusal> {
        if !point.is_finite() {
            return Err(NicheRefusal::Outside);
        }
        let (product, envelope) = self.product_containing(point, sight).ok_or(NicheRefusal::Outside)?;
        let placement = self.world_placement(product).ok_or(NicheRefusal::Outside)?;
        let inverse = placement.inverse().map_err(|_| NicheRefusal::Outside)?;
        let local = inverse.transform_point(point);
        // The frame reserves its footprint and not its box, so the corner it cut away is outside the
        // piece: a pointer there names no void rather than a cell with no floor (ADR 0201).
        let own = self.container(product).and_then(Container::envelope).ok_or(NicheRefusal::Outside)?;
        if !local.is_finite() || !within_box(local, envelope.local_min, envelope.local_max) {
            return Err(NicheRefusal::Outside);
        }
        if !own.holds(DVec2::new(local.x, local.y)) {
            return Err(NicheRefusal::Outside);
        }
        let surroundings = self.surroundings_around(point, sight);
        let mut cell = Cell::footprint(own, DVec2::new(local.x, local.y))?;
        let mut settled = false;
        for round in 0..MAX_CELL_ROUNDS {
            let mut added = false;
            for direction in cell.directions(local, round == 0) {
                let world = placement.transform_vector(direction);
                let Some(probe) = self.probe_within(point, world, &surroundings, contact) else {
                    continue;
                };
                let Some(plane) = plane_of(probe.face, inverse) else {
                    continue;
                };
                if !cell.cuts(plane) {
                    continue;
                }
                let Some(cut) = cell.clipped(plane) else {
                    return Err(NicheRefusal::Unrepresentable);
                };
                cell = cut;
                added = true;
            }
            if !added {
                settled = true;
                break;
            }
        }
        if !settled || !cell.contains(local) || self.holds_body(&cell, &surroundings, inverse) {
            return Err(NicheRefusal::Unrepresentable);
        }
        // The box is the space the frame reserves, not the convex piece the planes draw: an L reports
        // the L's own bounds, so the callout and the ghost measure what a part takes (ADR 0204).
        let corners: Vec<DVec3> = cell
            .region_corners()
            .or_else(|| Some(cell.corners.clone()))
            .unwrap_or_default()
            .into_iter()
            .map(quantised_corner)
            .collect();
        let Some(first) = corners.first().copied() else {
            return Err(NicheRefusal::Unrepresentable);
        };
        let (low, high) = match cell.region_bounds() {
            Some((ring_low, ring_high, bottom, top)) => {
                (DVec3::new(ring_low.x, ring_low.y, bottom), DVec3::new(ring_high.x, ring_high.y, top))
            }
            None => (corners.iter().copied().fold(first, DVec3::min), corners.iter().copied().fold(first, DVec3::max)),
        };
        let size = high - low;
        let world_min = placement.transform_point(low);
        if !(size.is_finite() && size.min_element() > 0.0) || !world_min.is_finite() {
            return Err(NicheRefusal::Unrepresentable);
        }
        let shape = cell_shape(&cell, &corners, low, high)?;
        Ok(Niche { product, local_min: low, size, min: world_min, shape, planes: cell.planes, corners })
    }

    /// Whether a surface the cell did not cut against stands inside it.
    ///
    /// A body or another product's shell inside the cell makes the free space non-convex: a board cut
    /// to the cell would cut into it, so the void is refused until the query can answer several
    /// cells. Only a corner strictly inside counts, so the wall that bounded the cell — its own
    /// corners lie on the boundary — is not mistaken for an obstacle.
    fn holds_body(&self, cell: &Cell, surroundings: &Surroundings<'_>, inverse: Placement) -> bool {
        surroundings.surfaces.iter().any(|surface| {
            surface_corners(surface.box_).into_iter().any(|corner| {
                let local = inverse.transform_point(corner);
                local.is_finite() && cell.planes.iter().all(|plane| plane.distance_to(local) > COORDINATE_TOLERANCE)
            })
        })
    }

    /// The distance to the first furniture face along `direction`, against known surroundings.
    fn probe_within(
        &self,
        point: DVec3,
        direction: DVec3,
        surroundings: &Surroundings<'_>,
        contact: Option<&dyn Contact>,
    ) -> Option<Probe> {
        let (unit, surface, face) = self.first_face(point, direction, surroundings, contact)?;
        let distance = quantised(face.distance);
        let hit_point = point + unit * distance;
        if !hit_point.is_finite() {
            return None;
        }
        Some(Probe {
            distance_mm: distance,
            hit: ProbeHit { id: surface.id, kind: surface.kind },
            face: ProbeFace { point: hit_point, normal: face.normal },
        })
    }

    /// The innermost product whose envelope contains `point`, with that envelope's world box.
    ///
    /// A part the sight switched off holds no space: a hidden product is neither the frame the point
    /// is measured in nor the shell a ray stops at, so the point belongs to the product around it
    /// (X73).
    fn product_containing(&self, point: DVec3, sight: Sight) -> Option<(Id, SurfaceBox)> {
        let mut innermost: Option<(Id, SurfaceBox, f64)> = None;
        for product in self.products() {
            if sight.hides(self, product) {
                continue;
            }
            let Some(envelope) = self.envelope_surface(product) else {
                continue;
            };
            if !envelope.contains(point) || !self.reserves(product, envelope, point) {
                continue;
            }
            let volume = (envelope.max - envelope.min).length_squared();
            if innermost.is_none_or(|(_, _, best)| volume < best) {
                innermost = Some((product, envelope, volume));
            }
        }
        innermost.map(|(product, envelope, _)| (product, envelope))
    }

    /// Whether a product's footprint holds a point its envelope's box already contains.
    ///
    /// A frame reserves the shape of its footprint, not the box around it (ADR 0159), so the corner a
    /// cut removed is outside the piece: the innermost product a point belongs to is the one whose own
    /// section holds it. A drawn ring is tested by its own curve, never by its chords, so the answer is
    /// the one the picking rule gives (ADR 0201).
    fn reserves(&self, product: Id, surface: SurfaceBox, point: DVec3) -> bool {
        let Some(envelope) = self.container(product).and_then(Container::envelope) else {
            return false;
        };
        if envelope.footprint().is_rectangle() {
            return true;
        }
        let Ok(inverse) = surface.world.inverse() else {
            return false;
        };
        let local = inverse.transform_point(point);
        local.is_finite() && envelope.holds(DVec2::new(local.x, local.y))
    }

    /// Every surface around `point` and the innermost envelope that contains it.
    ///
    /// Panels and objects are bodies; a product's authored box is a shell. The innermost envelope is
    /// also returned so a probe can report it as an envelope face rather than as the shell of the
    /// piece it is standing in.
    fn surroundings_around(&self, point: DVec3, sight: Sight) -> Surroundings<'_> {
        let mut surfaces = Vec::new();
        for product in self.products() {
            if sight.hides(self, product) {
                continue;
            }
            if let Some(envelope) = self.envelope_surface(product) {
                surfaces.push(Surface::boxed(envelope));
            }
        }
        self.body_surfaces(&self.root, &mut surfaces, 1, sight);
        let own = self.product_containing(point, sight);
        // The innermost envelope is measured as an envelope, not as one more shell; keeping it in
        // the list would let a tie choose the shell and report the wrong kind.
        let own_id = own.map(|(product, _)| product);
        surfaces.retain(|surface| Some(surface.box_.id) != own_id);
        Surroundings { own, surfaces }
    }

    /// The world box of a product's authored envelope, as a shell surface.
    fn envelope_surface(&self, id: Id) -> Option<SurfaceBox> {
        let envelope = self.container(id)?.envelope()?;
        let size =
            DVec3::new(envelope.x().millimetres_f64(), envelope.y().millimetres_f64(), envelope.z().millimetres_f64());
        surface_box(id, DVec3::ZERO, size, self.world_placement(id)?, SurfaceKind::Shell)
    }

    /// Collects the panels and objects below `container` as body surfaces.
    ///
    /// A node the sight switched off is left out with its whole subtree, which is the walk the scene
    /// builder and the pick make: the tools and the frame name the same parts (X73).
    pub(super) fn body_surfaces<'a>(
        &'a self,
        container: &'a Container,
        surfaces: &mut Vec<Surface<'a>>,
        depth: u32,
        sight: Sight,
    ) {
        if depth > Self::MAX_ASSEMBLY_DEPTH {
            return;
        }
        for child in container.children() {
            if sight.hides(self, child.id()) {
                continue;
            }
            match child {
                Node::Panel(panel) => {
                    let size = panel.nominal_size();
                    let max = DVec3::new(
                        size.length().millimetres_f64(),
                        size.width().millimetres_f64(),
                        panel.last_known_thickness().millimetres_f64(),
                    );
                    if let Some(world) = self.world_placement(panel.id())
                        && let Some(surface) = surface_box(panel.id(), DVec3::ZERO, max, world, SurfaceKind::Panel)
                    {
                        surfaces.push(Surface::boxed(surface));
                    }
                }
                Node::Object(object) => {
                    if let Some((min, max)) = object_bounds_mm(object.geometry())
                        && let Some(world) = self.world_placement(object.id())
                        && let Some(surface) = surface_box(object.id(), min, max, world, SurfaceKind::Body)
                    {
                        surfaces.push(Surface::body(surface, object.geometry()));
                    }
                }
                Node::Container(inner) => self.body_surfaces(inner, surfaces, depth.saturating_add(1), sight),
                Node::Camera(_) | Node::Light(_) => {}
            }
        }
    }

    /// The first surface and face a normalized ray meets.
    ///
    /// The world boxes are asked first (inside [`Surface::face_hit`]) and the port last, because a body
    /// lies inside its box: a surface whose box the ray misses cannot be hit at all, and a surface whose
    /// box the ray meets further away than the answer already in hand cannot beat it. So a scene of a
    /// few hundred bodies prepares only the handful of shapes a ray could actually reach (X90,
    /// ADR 0179).
    fn first_face(
        &self,
        point: DVec3,
        direction: DVec3,
        surroundings: &Surroundings<'_>,
        contact: Option<&dyn Contact>,
    ) -> Option<(DVec3, SurfaceBox, FaceHit)> {
        if !point.is_finite() || !direction.is_finite() {
            return None;
        }
        let length = direction.length();
        if !length.is_finite() || length <= f64::EPSILON {
            return None;
        }
        let unit = direction / length;
        let own = surroundings.own;
        let mut best: Option<(SurfaceBox, FaceHit)> = None;
        if let Some((_, envelope)) = own
            && let Some(face) = envelope.face_hit(point, unit)
        {
            best = Some((SurfaceBox { kind: SurfaceKind::Envelope, ..envelope }, face));
        }
        for surface in &surroundings.surfaces {
            if own.is_some_and(|(product, _)| product == surface.box_.id) {
                continue;
            }
            let Some(face) = surface.face_hit(point, unit, contact) else {
                continue;
            };
            if best.is_none_or(|(_, nearest)| face.distance < nearest.distance) {
                best = Some((surface.box_, face));
            }
        }
        best.map(|(surface, face)| (unit, surface, face))
    }
}

/// The eight true corners of a surface box, in world millimetres.
///
///
/// The corners are the board's own, not the world box around a turned board: a slanted wall meets the
/// cell on its face, and the loose box around it would reach across the boundary.
pub(super) fn surface_corners(surface: SurfaceBox) -> Vec<DVec3> {
    let mut corners = Vec::with_capacity(8);
    for x in [surface.local_min.x, surface.local_max.x] {
        for y in [surface.local_min.y, surface.local_max.y] {
            for z in [surface.local_min.z, surface.local_max.z] {
                corners.push(surface.world.transform_point(DVec3::new(x, y, z)));
            }
        }
    }
    corners
}

/// The ring a frame reserves, turned counter-clockwise, whatever shape it is.
///
/// A frame may be drawn as any closed ring (ADR 0159), and the space it reserves is that ring — an L is
/// reserved in full by the part that drew it, and the check that the pointer stands inside it is the
/// caller's. The ring is *not* read as one convex cell here: that reading is [`convex_piece`]'s, and it
/// serves the planes and the stances that cannot answer a shape (ADR 0201, ADR 0203, ADR 0204).
fn frame_ring(outline: &Ring) -> Result<Ring, NicheRefusal> {
    let ring = match outline.is_counter_clockwise() {
        Some(true) => outline.clone(),
        Some(false) => outline.reversed().map_err(|_| NicheRefusal::Unrepresentable)?,
        None => return Err(NicheRefusal::Unrepresentable),
    };
    if ring.segments().len() < 3 || ring.segments().len().saturating_add(2) > MAX_CELL_PLANES {
        return Err(NicheRefusal::Unrepresentable);
    }
    Ok(ring)
}

/// The convex piece of a frame's ring around `point`, which is what the cell's planes describe.
///
/// A cell is planes and corners (ADR 0106), and a ring that is not convex has no one such cell, so the
/// ring is read at the point: **every edge whose free side holds the point clips it**, and what is left
/// is the convex piece of the frame the pointer names (ADR 0203). An L answers its bar for a point in
/// the bar, its column for a point in the column, and the square both arms share for a point in the
/// corner — because the tool cannot know which arm a hand in the corner meant, and the square is the
/// piece both answer. A ring that is already convex answers itself, with no clip at all, so a rectangle
/// or a cut-corner frame keeps exactly the cell it had (ADR 0201, ADR 0202).
///
/// The clip runs through the exact boolean (`Region::clip_half_plane_around`), so an arc of the frame
/// stays an arc. A ring still not convex after the clip — an arc bulging into its own shape — is the
/// footprint's own refusal.
fn convex_piece(ring: &Ring, point: DVec2) -> Result<Ring, NicheRefusal> {
    let ring = ring.clone();
    if is_convex(&ring) {
        return Ok(ring);
    }
    let mut piece = Region::new(ring.clone(), Vec::new()).map_err(|_| NicheRefusal::Unrepresentable)?;
    let polygon = chord_polygon(&ring);
    for index in 0..polygon.len() {
        let (Some(start), Some(end)) = (polygon.get(index), polygon.get((index + 1) % polygon.len())) else {
            return Err(NicheRefusal::Unrepresentable);
        };
        let edge = *end - *start;
        let Some(inward) = DVec2::new(-edge.y, edge.x).try_normalize() else {
            continue;
        };
        // A wall the point stands outside bounds the piece the point is *not* in.
        if inward.dot(point) < inward.dot(*start) - SECTION_TOLERANCE {
            continue;
        }
        piece = piece
            .clip_half_plane_around(inward, inward.dot(*start), point)
            .map_err(|_| NicheRefusal::Unrepresentable)?
            .ok_or(NicheRefusal::Footprint)?;
    }
    let ring = piece.outer().clone();
    if is_convex(&ring) { Ok(ring) } else { Err(NicheRefusal::Footprint) }
}

/// The chord polygon a ring's planes are built from, without the shape the ring reserves.
///
/// Whether a counter-clockwise ring is one convex piece: every joint turns left and every arc sweeps left.
fn is_convex(ring: &Ring) -> bool {
    let segments = ring.segments();
    if segments.len() < 3 {
        return false;
    }
    for (index, segment) in segments.iter().enumerate() {
        if !segment.start().is_finite() {
            return false;
        }
        // A counter-clockwise arc curves left; one stored the other way bulges into the shape.
        if let Segment::Arc(arc) = segment
            && arc.sweep_angle() <= 0.0
        {
            return false;
        }
        let Some(next) = segments.get((index + 1) % segments.len()) else {
            return false;
        };
        if segment.tangent_at_end().perp_dot(next.tangent_at_start()) < -CONVEX_TURN {
            return false;
        }
    }
    true
}

/// The chord polygon of a convex ring: one corner per segment, the arc's own start.
///
/// The chords are the cell's planes: a safe wall for a probe, because a convex ring's arc bulges away
/// from the shape it cuts and its chord therefore lies inside the frame (ADR 0201).
fn chord_polygon(ring: &Ring) -> Vec<DVec2> {
    ring.segments().iter().map(|segment| segment.start()).collect()
}

/// The shape a cell answers with: a box, one section, or the sloped cell itself.
fn cell_shape(cell: &Cell, corners: &[DVec3], low: DVec3, high: DVec3) -> Result<NicheShape, NicheRefusal> {
    // A box is the answer only when the planes' own box *is* the shape. A frame that keeps a ring is
    // asked its ring instead: its chords can bound the very box the corners span while the ring bulges
    // past it — the box's corners cannot see a curve (N34).
    if is_box(corners, low, high) && cell.ring.is_none() {
        return Ok(NicheShape::Box);
    }
    if !cell.is_prism() {
        return Ok(NicheShape::Sloped);
    }
    // A frame whose own ring is more than a box answers the ring itself, arcs and all, so a board cut
    // to it takes the case's shape rather than the chords of it (N33).
    if let Some(section) = cell.exact_section() {
        let region = section?;
        return document_ring(region.outer()).map(NicheShape::Section).ok_or(NicheRefusal::Unrepresentable);
    }
    let polygon = cut_planes(&cell.planes, 2, low.z, low.z, 0, 1).ok_or(NicheRefusal::Unrepresentable)?;
    ring(polygon).map(NicheShape::Section).ok_or(NicheRefusal::Unrepresentable)
}

/// Whether the corners are exactly the eight corners of the box they span.
fn is_box(corners: &[DVec3], low: DVec3, high: DVec3) -> bool {
    corners.len() == 8
        && corners.iter().all(|corner| {
            [0usize, 1, 2].iter().all(|axis| {
                let value = corner[*axis];
                (value - low[*axis]).abs() <= COORDINATE_TOLERANCE
                    || (value - high[*axis]).abs() <= COORDINATE_TOLERANCE
            })
        })
}

/// The plane of a probed face, in the product's frame.
fn plane_of(face: ProbeFace, inverse: Placement) -> Option<NichePlane> {
    let hit = inverse.transform_point(face.point);
    let normal = inverse.transform_vector(face.normal).try_normalize()?;
    if !hit.is_finite() {
        return None;
    }
    Some(NichePlane { normal, offset: normal.dot(hit) })
}

/// Where three planes meet, or `None` when they do not meet in a point.
fn corner_of(first: &NichePlane, second: &NichePlane, third: &NichePlane) -> Option<DVec3> {
    let across = second.normal.cross(third.normal);
    let determinant = first.normal.dot(across);
    if !determinant.is_finite() || determinant.abs() < SECTION_ANGLE {
        return None;
    }
    let point = (across * first.offset
        + third.normal.cross(first.normal) * second.offset
        + first.normal.cross(second.normal) * third.offset)
        / determinant;
    point.is_finite().then_some(point)
}

/// The cell cut by a slab along `axis`, as the convex polygon seen in the `(u, v)` axes.
///
/// Every plane's half-space is evaluated at the slab's own two ends, so the polygon is exactly the
/// set of `(u, v)` points whose whole slab lies in the cell. The corners are the pairwise meeting
/// points of the cut lines; the cell's own planes keep the polygon bounded.
fn cut_planes(planes: &[NichePlane], axis: usize, low: f64, high: f64, u: usize, v: usize) -> Option<Vec<DVec2>> {
    if axis > 2 || u > 2 || v > 2 || axis == u || u == v || axis == v || !low.is_finite() || !high.is_finite() {
        return None;
    }
    let (low, high) = if low <= high { (low, high) } else { (high, low) };
    if planes.len() < 4 {
        return None;
    }
    // Each plane as a line in the cut's own plane: `normal · (u, v) >= offset`.
    let mut lines: Vec<(DVec2, f64)> = Vec::with_capacity(planes.len());
    for plane in planes {
        let normal = plane.normal.to_array();
        let along = normal[axis];
        // The slab end that leaves the least room on this plane's free side.
        let base = if along >= 0.0 { along * low } else { along * high };
        let offset = plane.offset - base;
        let flat = DVec2::new(normal[u], normal[v]);
        if flat.length() < SECTION_NORMAL {
            // The plane is level with the cut: it holds over the whole cut or not at all.
            if base < plane.offset - COORDINATE_TOLERANCE {
                return None;
            }
            continue;
        }
        lines.push((flat, offset));
    }
    let mut points: Vec<DVec2> = Vec::new();
    for (index, first) in lines.iter().enumerate() {
        for second in lines.iter().skip(index.saturating_add(1)) {
            let determinant = first.0.x * second.0.y - first.0.y * second.0.x;
            if !determinant.is_finite() || determinant.abs() < SECTION_ANGLE {
                continue;
            }
            let point = DVec2::new(
                (first.1 * second.0.y - second.1 * first.0.y) / determinant,
                (first.0.x * second.1 - second.0.x * first.1) / determinant,
            );
            if !point.is_finite() {
                continue;
            }
            if lines.iter().any(|(normal, offset)| normal.dot(point) - offset < -COORDINATE_TOLERANCE) {
                continue;
            }
            if points.iter().any(|seen| (*seen - point).length() <= COORDINATE_TOLERANCE) {
                continue;
            }
            points.push(point);
        }
    }
    if points.len() < 3 || points.len() > MAX_CELL_CORNERS {
        return None;
    }
    order_ring(&mut points)?;
    Some(points)
}

/// Puts a convex polygon's corners in counter-clockwise order around their own centroid.
fn order_ring(points: &mut [DVec2]) -> Option<()> {
    let middle = centre(points)?;
    points.sort_by(|first, second| {
        let left = (first.y - middle.y).atan2(first.x - middle.x);
        let right = (second.y - middle.y).atan2(second.x - middle.x);
        left.total_cmp(&right).then((*first - middle).length_squared().total_cmp(&(*second - middle).length_squared()))
    });
    (signed_area(points) > SECTION_TOLERANCE).then_some(())
}

/// The signed area of a polygon; positive when its corners run counter-clockwise.
pub(super) fn signed_area(points: &[DVec2]) -> f64 {
    let mut total = 0.0;
    for index in 0..points.len() {
        let (Some(start), Some(end)) = (points.get(index), points.get((index + 1) % points.len())) else {
            continue;
        };
        total += start.x * end.y - end.x * start.y;
    }
    total * 0.5
}

/// The average of a polygon's corners, or `None` when there are none or it is not finite.
fn centre(points: &[DVec2]) -> Option<DVec2> {
    if points.is_empty() {
        return None;
    }
    let mut sum = DVec2::ZERO;
    for point in points {
        sum += *point;
    }
    let middle = sum / points.len() as f64;
    middle.is_finite().then_some(middle)
}

/// The geometry ring as the document ring a panel stores as its outline.
///
/// Every segment keeps its curve, so an arc of the frame stays an arc of the part; coordinates round to
/// the micrometre and angles to the millidegree the document holds. A ring the document cannot close is
/// no section at all.
pub(super) fn document_ring(ring: &Ring) -> Option<Contour> {
    let mut segments = Vec::with_capacity(ring.segments().len());
    for segment in ring.segments() {
        match ContourSegment::from_geom(*segment) {
            Ok(segment) => segments.push(segment),
            // A segment the document's step rounds away is the ring's own curve, not a hole in it: what
            // is left of the joint is a micrometre, which the ring's joint tolerance already covers.
            Err(ContourSegmentError::ZeroLength) => {}
            Err(_) => return None,
        }
    }
    Contour::new(segments).ok()
}

/// The polygon as the document ring a panel stores as its outline.
///
/// The polygon is measured as a geometry region first — a degenerate or self-crossing section is
/// refused there — and the document ring is built from the same corners, rounded to the micrometre.
fn ring(points: Vec<DVec2>) -> Option<Contour> {
    Region::from_loops(points.clone(), Vec::new()).ok()?;
    let points = points.into_iter().map(document_point).collect::<Option<Vec<_>>>()?;
    Contour::from_polygon(points).ok()
}

/// One point of a section as the document's micrometres.
fn document_point(point: DVec2) -> Option<[Length; 2]> {
    Some([Length::from_micrometres(micrometres(point.x)?), Length::from_micrometres(micrometres(point.y)?)])
}

/// A millimetre measurement as the document's whole micrometres, or `None` when it is not a number.
fn micrometres(value: f64) -> Option<i64> {
    let rounded = (value * 1000.0).round();
    if !rounded.is_finite() || rounded.abs() > i64::MAX as f64 {
        return None;
    }
    Some(rounded as i64)
}

/// Whether `point` lies inside the box, allowing the micrometre the document stores.
fn within_box(point: DVec3, min: DVec3, max: DVec3) -> bool {
    (0..3)
        .all(|axis| point[axis] >= min[axis] - COORDINATE_TOLERANCE && point[axis] <= max[axis] + COORDINATE_TOLERANCE)
}

/// The order corners are listed in, so the same cell always reports the same list.
fn corner_order(first: &DVec3, second: &DVec3) -> std::cmp::Ordering {
    first.x.total_cmp(&second.x).then(first.y.total_cmp(&second.y)).then(first.z.total_cmp(&second.z))
}

/// A point rounded to the micrometre the document stores.
fn quantised_point(point: DVec2) -> DVec2 {
    DVec2::new(quantised(point.x), quantised(point.y))
}

/// A cell corner rounded to the micrometre the document stores.
fn quantised_corner(point: DVec3) -> DVec3 {
    DVec3::new(quantised(point.x), quantised(point.y), quantised(point.z))
}

/// Rounds a measurement to the micrometre the document stores.
fn quantised(value: f64) -> f64 {
    match micrometres(value) {
        Some(whole) => whole as f64 / 1000.0,
        None => 0.0,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use geom::{DQuat, DVec3};
    use units::Degrees;

    use crate::fixtures::*;
    use crate::*;

    /// A product of `size` millimetres with its base-min corner at `at` in the root frame.
    fn product(project: &mut Project, size: (i64, i64, i64), at: (i64, i64, i64)) -> Id {
        let root = project.root().id();
        let (width, depth, height) = size;
        let envelope =
            Envelope::new(millimetres(width), millimetres(depth), millimetres(height)).expect("a valid envelope");
        let placement = Placement::IDENTITY
            .with_translation(millimetres(at.0), millimetres(at.1), millimetres(at.2))
            .expect("a placement");
        project.add_product(root, "Piece", ClassCode::GENERAL_PRODUCT, envelope, placement).expect("add product")
    }

    /// A panel of `length x width x thickness` millimetres inside `parent`, based at `at`.
    fn panel(project: &mut Project, parent: Id, at: (i64, i64, i64), size: (i64, i64, i64)) -> Id {
        let nominal = PanelNominalSize::new(millimetres(size.0), millimetres(size.1)).expect("a nominal size");
        let placement = Placement::IDENTITY
            .with_translation(millimetres(at.0), millimetres(at.1), millimetres(at.2))
            .expect("a placement");
        project.add_panel(parent, "Part", nominal, None, millimetres(size.2), placement).expect("add panel")
    }

    /// A board of `length x width x thickness` millimetres, turned by `rotation` and based at `at`.
    fn board(project: &mut Project, parent: Id, at: DVec3, rotation: DQuat, size: (i64, i64, i64)) -> Id {
        let nominal = PanelNominalSize::new(millimetres(size.0), millimetres(size.1)).expect("a board size");
        let placement = Placement::new(at, rotation).expect("a placement");
        project.add_panel(parent, "Board", nominal, None, millimetres(size.2), placement).expect("add a board")
    }

    /// A standing post whose inner face runs through `through`, angled so that face's normal
    /// pointing into the free space is `normal` (both in the product's XY plane, millimetres).
    ///
    /// The post is 720 mm tall, 18 mm thick and wide enough to cross the whole depth. It stands in
    /// the post's stance (ADR 0105): the panel's local X runs up the height, its local Y along the
    /// depth, and its local Z through the thickness away from the free space, so a quarter-turn about
    /// Y stands it up and a rotation about Z splays it. The face the free space touches is the
    /// board's own `z = 0` face and the node's base corner sits on the void's floor.
    fn angled_post(project: &mut Project, parent: Id, through: (f64, f64), normal: (f64, f64)) -> Id {
        let direction = (-normal.1, normal.0);
        // The panel is 1200 mm wide, so the start of its local Y is placed to centre that span on
        // the case's depth instead of hanging off one side.
        let start_y = 280.0 - 600.0 * direction.1;
        let start_x = through.0 + (start_y - through.1) * (direction.0 / direction.1);
        let angle = normal.1.atan2(normal.0);
        let rotation = DQuat::from_rotation_z(angle) * DQuat::from_rotation_y(-std::f64::consts::FRAC_PI_2);
        let translation = DVec3::new(start_x, start_y, 0.0);
        let placement = Placement::new(translation, rotation).expect("a placement");
        let nominal = PanelNominalSize::new(millimetres(720), millimetres(1200)).expect("a post size");
        project.add_panel(parent, "Post", nominal, None, millimetres(18), placement).expect("add a post")
    }

    /// A board standing up and leaning `lean` radians forward, its face on the piece's back edge.
    ///
    /// Its length climbs the leaned face, its thickness runs away from the void and its width runs
    /// back along the product's X, so the free space touches the board's own `z = 0` face.
    fn leaned_back_wall(project: &mut Project, parent: Id) -> Id {
        let lean = 15.0_f64.to_radians();
        let (sin, cos) = lean.sin_cos();
        let up = DVec3::new(0.0, -sin, cos);
        let away = DVec3::new(0.0, cos, sin);
        let rotation = DQuat::from_rotation_axes(up.cross(away), up, away);
        board(project, parent, DVec3::new(600.0, 560.0, 0.0), rotation, (600, 900, 18))
    }

    /// A board lying over the void and coming down towards the back: its underside is the void's
    /// ceiling, 699.5 mm above the floor at the front and 98 mm lower at the back.
    fn leaned_ceiling(project: &mut Project, parent: Id) -> Id {
        let lean = 10.0_f64.to_radians();
        let (sin, cos) = lean.sin_cos();
        let along = DVec3::new(0.0, cos, -sin);
        let away = DVec3::new(0.0, sin, cos);
        let rotation = DQuat::from_rotation_axes(along.cross(away), along, away);
        board(project, parent, DVec3::new(0.0, 0.0, 699.5), rotation, (600, 620, 18))
    }

    /// An empty document with a root and nothing else, so a product's own box is the only surface.
    fn clean_project() -> Project {
        let root = Container::new(Id::new(1).expect("id"), "Root", Placement::IDENTITY, Vec::new());
        Project::new(root).expect("an empty project")
    }

    /// An empty product of `600 x 560 x 720` millimetres at the origin.
    fn empty_piece() -> (Project, Id) {
        let mut project = clean_project();
        let id = product(&mut project, (600, 560, 720), (0, 0, 0));
        (project, id)
    }

    /// A product whose base is the given envelope, at `at` in the root frame.
    fn shaped(project: &mut Project, envelope: Envelope, at: (i64, i64, i64)) -> Id {
        let root = project.root().id();
        let placement = Placement::IDENTITY
            .with_translation(millimetres(at.0), millimetres(at.1), millimetres(at.2))
            .expect("a placement");
        project.add_product(root, "Piece", ClassCode::GENERAL_PRODUCT, envelope, placement).expect("add product")
    }

    /// A chamfer of the two setbacks the maker measures, in millimetres.
    fn chamfer(incoming: i64, outgoing: i64) -> Option<CornerFeatureKind> {
        Some(CornerFeatureKind::Chamfer { incoming: millimetres(incoming), outgoing: millimetres(outgoing) })
    }

    /// A corner cabinet: 600 wide, 500 deep, one 45-degree cut on the bottom-right corner.
    fn corner_cabinet(project: &mut Project) -> Id {
        let envelope = Envelope::with_footprint(
            millimetres(600),
            millimetres(500),
            millimetres(720),
            Footprint::cut([None, chamfer(100, 100), None, None]),
        )
        .expect("a corner cabinet");
        shaped(project, envelope, (0, 0, 0))
    }

    #[test]
    fn a_corner_cabinet_reserves_its_cut_corner_and_no_more() {
        // The frame reserves its footprint, not the box around it (ADR 0159): the cell is seeded from
        // the cut ring, so a part measured here stops on the cut face instead of growing into the
        // corner the frame gave away (N29, ADR 0201).
        let mut project = clean_project();
        let piece = corner_cabinet(&mut project);
        let niche = project
            .niche(DVec3::new(300.0, 250.0, 300.0), Sight::none(), None)
            .expect("the void inside the corner cabinet");
        assert_eq!(niche.product, piece);
        assert_eq!(niche.local_min, DVec3::ZERO);
        assert_eq!(niche.size, DVec3::new(600.0, 500.0, 720.0), "the box is the footprint's own bounds");
        let corners = niche
            .section()
            .expect("one section from the floor to the ceiling")
            .sampled_points(1.0)
            .expect("a section of straight edges");
        let expected = [
            [millimetres(0), millimetres(0)],
            [millimetres(500), millimetres(0)],
            [millimetres(600), millimetres(100)],
            [millimetres(600), millimetres(500)],
            [millimetres(0), millimetres(500)],
        ];
        assert_eq!(corners, expected, "the cut face is a wall of the cell");
        assert_eq!(niche.planes.len(), 7, "five upright walls, a floor and a ceiling");
        assert_eq!(niche.corners.len(), 10, "the prism of a five-sided ring");

        // The corner the frame cut away belongs to no piece, so a pointer there names no void.
        assert_eq!(
            project.niche(DVec3::new(590.0, 10.0, 300.0), Sight::none(), None),
            Err(NicheRefusal::Outside),
            "no cell may stand in the corner the frame does not reserve"
        );
    }

    #[test]
    fn a_drawn_ring_bounds_the_niche_by_its_own_ring() {
        let mut project = clean_project();
        let ring = Path::polyline(
            vec![
                [millimetres(0), millimetres(0)],
                [millimetres(600), millimetres(0)],
                [millimetres(600), millimetres(560)],
                [millimetres(300), millimetres(700)],
                [millimetres(0), millimetres(560)],
            ],
            true,
        )
        .expect("a closed ring")
        .to_contour()
        .expect("a contour");
        let envelope = Envelope::with_ring(&ring, millimetres(720)).expect("a drawn frame");
        shaped(&mut project, envelope, (0, 0, 0));
        let niche = project
            .niche(DVec3::new(300.0, 300.0, 300.0), Sight::none(), None)
            .expect("the void a drawn ring reserves");
        let corners = niche
            .section()
            .expect("the ring is one section from the floor to the ceiling")
            .sampled_points(1.0)
            .expect("a section of straight edges");
        let expected = [
            [millimetres(0), millimetres(0)],
            [millimetres(600), millimetres(0)],
            [millimetres(600), millimetres(560)],
            [millimetres(300), millimetres(700)],
            [millimetres(0), millimetres(560)],
        ];
        assert_eq!(corners, expected, "the drawn ring is the section, unchanged");
        assert_eq!(niche.size, DVec3::new(600.0, 700.0, 720.0));

        // A point the ring left outside itself is outside the piece, as picking already reads it.
        assert_eq!(
            project.niche(DVec3::new(590.0, 690.0, 300.0), Sight::none(), None),
            Err(NicheRefusal::Outside),
            "the cut above the slanted back is not part of the drawn piece"
        );
    }

    #[test]
    fn a_rounded_corner_keeps_the_frames_own_radius() {
        // The cell's planes take an arc by its chord (ADR 0201), and that stays the safe wall a probe
        // meets; the section a board is cut to keeps the frame's own curve, so the part here takes the
        // radius instead of the chamfer the chord would draw (N33). The chord of an 80 mm radius on a
        // 600 x 500 case runs from (520, 0) to (600, 80), inside the arc it spans.
        let mut project = clean_project();
        let envelope = Envelope::with_footprint(
            millimetres(600),
            millimetres(500),
            millimetres(720),
            Footprint::cut([None, Some(CornerFeatureKind::Radius { radius: millimetres(80) }), None, None]),
        )
        .expect("a rounded case");
        shaped(&mut project, envelope, (0, 0, 0));
        let niche = project
            .niche(DVec3::new(300.0, 250.0, 300.0), Sight::none(), None)
            .expect("the void inside the rounded case");
        let section = niche.section().expect("one section from the floor to the ceiling");
        assert_eq!(section.segments().len(), 5, "four sides and the corner arc");
        let arcs: Vec<geom::Arc> = section
            .segments()
            .filter_map(|segment| match segment.to_geom() {
                Ok(geom::Segment::Arc(arc)) => Some(arc),
                _ => None,
            })
            .collect();
        let [arc] = arcs.as_slice() else {
            panic!("the corner is one arc, was {arcs:?}");
        };
        assert!((arc.radius() - 80.0).abs() < 1.0e-3, "the frame's 80 mm radius, was {}", arc.radius());
        assert!(
            (arc.center() - geom::DVec2::new(520.0, 80.0)).length() < 1.0e-3,
            "and its own centre: {:?}",
            arc.center()
        );
        let region =
            geom::Region::new(section.to_geometry().expect("the ring draws in the geometry layer"), Vec::new())
                .expect("a region");
        assert!(
            region.contains(geom::DVec2::new(573.0, 27.0)),
            "the space between the chord and the arc is the part's"
        );
        assert!(!region.contains(geom::DVec2::new(597.0, 3.0)), "and the corner outside the arc is not");
    }

    #[test]
    fn a_wall_through_a_rounded_corner_cuts_the_arc_and_keeps_its_radius() {
        // A wall standing across the case at x = 560..578 cuts the frame's 80 mm corner arc, which is
        // centred at (520, 80): the section keeps the shorter arc, of the same radius, from (520, 0) to
        // the crossing (560, 80 - sqrt(6400 - 1600)) = (560, 10.72), and closes on the wall's face (N33).
        let mut project = clean_project();
        let envelope = Envelope::with_footprint(
            millimetres(600),
            millimetres(500),
            millimetres(720),
            Footprint::cut([None, Some(CornerFeatureKind::Radius { radius: millimetres(80) }), None, None]),
        )
        .expect("a rounded case");
        let piece = shaped(&mut project, envelope, (0, 0, 0));
        panel(&mut project, piece, (560, 0, 0), (18, 500, 720));
        let niche =
            project.niche(DVec3::new(300.0, 250.0, 300.0), Sight::none(), None).expect("the void beside the wall");
        let section = niche.section().expect("one section");
        assert_eq!(section.segments().len(), 5, "the wall's face, four pieces of the frame");
        let arcs: Vec<geom::Arc> = section
            .segments()
            .filter_map(|segment| match segment.to_geom() {
                Ok(geom::Segment::Arc(arc)) => Some(arc),
                _ => None,
            })
            .collect();
        let [arc] = arcs.as_slice() else {
            panic!("the corner is still one arc, was {arcs:?}");
        };
        assert!((arc.radius() - 80.0).abs() < 1.0e-3, "the frame's own radius survives the cut: {}", arc.radius());
        assert!((arc.center() - geom::DVec2::new(520.0, 80.0)).length() < 1.0e-3, "{:?}", arc.center());
        assert!(
            (arc.end() - geom::DVec2::new(560.0, 10.719_1)).length() < 1.0e-2,
            "it ends on the wall: {:?}",
            arc.end()
        );
        assert!((section.bounds_micrometres().1[0] - 560_000).abs() <= 1000, "the wall is the new edge");
    }

    /// A frame drawn as an L: 600 x 560 with a notch cut out of its top between x = 200 and x = 300.
    fn notched(project: &mut Project) -> Id {
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
        shaped(project, envelope, (0, 0, 0))
    }

    #[test]
    fn a_notched_frame_reserves_the_whole_shape_it_draws() {
        // The space a frame reserves is the ring it draws, whole (ADR 0204): every point of the L
        // answers the same shape — the arm a hand stands in is the cell's own planes' reading, not a
        // smaller void — and the box a callout draws is the frame's own (N34, N35).
        let mut project = clean_project();
        notched(&mut project);
        for point in [DVec3::new(100.0, 200.0, 300.0), DVec3::new(100.0, 520.0, 300.0), DVec3::new(450.0, 200.0, 300.0)]
        {
            let niche = project.niche(point, Sight::none(), None).expect("the ring reserves the point");
            assert_eq!(niche.local_min, DVec3::ZERO, "the notch's own corner is the frame's");
            assert_eq!(niche.size, DVec3::new(600.0, 560.0, 720.0), "the box is the L's, at {point:?}");
            let section = niche.section().expect("one section");
            assert_eq!(section.segments().len(), 8, "the whole L, notch and all, at {point:?}");
        }

        // The notch itself reserves nothing: a point there is outside the ring, not a cell.
        assert_eq!(
            project.niche(DVec3::new(250.0, 500.0, 300.0), Sight::none(), None),
            Err(NicheRefusal::Outside),
            "the notch is the corner the frame gave away"
        );
    }

    /// A frame whose top edge bows up by `sagitta` millimetres: a rectangle with a bulge.
    fn bowed(sagitta: f64) -> Envelope {
        let ring = geom::Contour::new(
            vec![
                geom::Segment::Line(geom::Line::new(geom::DVec2::ZERO, geom::DVec2::new(600.0, 0.0)).expect("bottom")),
                geom::Segment::Line(
                    geom::Line::new(geom::DVec2::new(600.0, 0.0), geom::DVec2::new(600.0, 560.0)).expect("right"),
                ),
                geom::Segment::Arc(
                    geom::Arc::from_endpoints_sagitta(
                        geom::DVec2::new(600.0, 560.0),
                        geom::DVec2::new(0.0, 560.0),
                        -sagitta,
                    )
                    .expect("a bowed top"),
                ),
                geom::Segment::Line(geom::Line::new(geom::DVec2::new(0.0, 560.0), geom::DVec2::ZERO).expect("left")),
            ],
            true,
        )
        .expect("a ring");
        let contour = Contour::new(
            ring.segments()
                .iter()
                .map(|segment| ContourSegment::from_geom(*segment).expect("a document segment"))
                .collect(),
        )
        .expect("a document ring");
        Envelope::with_ring(&contour, millimetres(720)).expect("a bowed frame")
    }

    #[test]
    fn a_point_in_a_bowed_frames_bulge_is_inside_the_void() {
        // The cell's planes are the ring's chords, and a chord lies inside the arc it spans, so a point of
        // the bulge is outside every plane and inside the frame. The frame reserves it, so the cell asks
        // its ring too, and the section keeps the curve the bulge is made of (N34).
        let mut project = clean_project();
        shaped(&mut project, bowed(100.0), (0, 0, 0));
        for y in [300.0, 620.0] {
            let niche = project
                .niche(DVec3::new(300.0, y, 300.0), Sight::none(), None)
                .expect("the frame reserves the bulge, so a point in it is inside the void");
            let section = niche.section().expect("one section");
            let highest = section
                .sampled_points(1000.0)
                .expect("the ring samples")
                .iter()
                .map(|point| point[1].millimetres_f64())
                .fold(f64::MIN, f64::max);
            assert!(highest > 650.0, "the section keeps the bulge's own curve at y = {y}, was {highest}");
        }
        // The box is the ring's own bounds, bulge and all: the frame reserves the height the arc adds,
        // so a callout reporting the chord would understate the space (ADR 0204).
        let plain =
            project.niche(DVec3::new(300.0, 300.0, 300.0), Sight::none(), None).expect("the void under the chord");
        assert!((plain.size.y - 660.0).abs() < 0.01, "the ring bounds the box: {:?}", plain.size);
    }

    #[test]
    fn a_ring_that_bulges_into_its_own_shape_is_still_a_refusal() {
        // The frame's own square with its top edge pulled down into the material: no clip around a point
        // can straighten an arc that bulges inwards, so the frame keeps its own refusal rather than a
        // cell that would cut into it (ADR 0203).
        let ring = geom::Contour::new(
            vec![
                geom::Segment::Line(geom::Line::new(geom::DVec2::ZERO, geom::DVec2::new(600.0, 0.0)).expect("bottom")),
                geom::Segment::Line(
                    geom::Line::new(geom::DVec2::new(600.0, 0.0), geom::DVec2::new(600.0, 560.0)).expect("right"),
                ),
                geom::Segment::Arc(
                    geom::Arc::from_endpoints_sagitta(
                        geom::DVec2::new(600.0, 560.0),
                        geom::DVec2::new(0.0, 560.0),
                        50.0,
                    )
                    .expect("an inward arc"),
                ),
                geom::Segment::Line(geom::Line::new(geom::DVec2::new(0.0, 560.0), geom::DVec2::ZERO).expect("left")),
            ],
            true,
        )
        .expect("a ring");
        let contour = Contour::new(
            ring.segments()
                .iter()
                .map(|segment| ContourSegment::from_geom(*segment).expect("a document segment"))
                .collect(),
        )
        .expect("a document ring");
        let envelope = Envelope::with_ring(&contour, millimetres(720)).expect("a bowed frame");
        let mut project = clean_project();
        shaped(&mut project, envelope, (0, 0, 0));
        assert_eq!(
            project.niche(DVec3::new(300.0, 200.0, 300.0), Sight::none(), None),
            Err(NicheRefusal::Footprint),
            "a ring that is not one convex piece still answers no cell"
        );
    }

    /// Whether a point is inside every supporting plane of the niche.
    fn fits(niche: &Niche, point: DVec3) -> bool {
        niche.planes.iter().all(|plane| plane.distance_to(point) >= -super::COORDINATE_TOLERANCE)
    }

    #[test]
    fn a_probe_stops_at_the_envelope_virtual_face() {
        let (project, piece) = empty_piece();
        let point = DVec3::new(300.0, 280.0, 300.0);
        let answer = project.probe(point, DVec3::Z, Sight::none(), None).expect("the envelope stops the ray");
        assert_eq!(answer.distance_mm, 420.0, "the roof is 720 - 300 away");
        assert_eq!(answer.hit, ProbeHit { id: piece, kind: SurfaceKind::Envelope });
        assert!((answer.face.normal - -DVec3::Z).length() < 1.0e-9, "the face looks back down the ray");
        assert!((answer.face.point - DVec3::new(300.0, 280.0, 720.0)).length() < 1.0e-9);
        let down = project.probe(point, -DVec3::Z, Sight::none(), None).expect("the floor is an envelope face too");
        assert_eq!(down.distance_mm, 300.0);
        assert_eq!(down.hit.kind, SurfaceKind::Envelope);
    }

    #[test]
    fn a_probe_stops_at_a_panel_and_reports_it() {
        let (mut project, piece) = empty_piece();
        let partition = panel(&mut project, piece, (300, 0, 0), (18, 560, 720));
        let answer = project
            .probe(DVec3::new(100.0, 280.0, 300.0), DVec3::X, Sight::none(), None)
            .expect("the panel stops the ray");
        assert_eq!(answer.distance_mm, 200.0);
        assert_eq!(answer.hit, ProbeHit { id: partition, kind: SurfaceKind::Panel });
    }

    #[test]
    fn a_probe_stops_at_another_product_shell_and_at_a_body() {
        let mut project = clean_project();
        let outer = product(&mut project, (2000, 1000, 1000), (0, 0, 0));
        let inner = product(&mut project, (300, 300, 300), (1000, 400, 400));
        let answer = project
            .probe(DVec3::new(500.0, 500.0, 500.0), DVec3::X, Sight::none(), None)
            .expect("the inner shell stops the ray");
        assert_eq!(answer.distance_mm, 500.0);
        assert_eq!(answer.hit, ProbeHit { id: inner, kind: SurfaceKind::Shell });
        assert_ne!(answer.hit.id, outer, "the shell of the piece the point sits in is not reported as a hit");

        // The same ray meets a geometry body before the shell.
        let root = project.root().id();
        let object = project
            .add_object(
                root,
                "Block",
                Placement::IDENTITY
                    .with_translation(millimetres(600), millimetres(495), millimetres(495))
                    .expect("a placement"),
                sample_object(90).geometry().clone(),
            )
            .expect("add object");
        let answer = project
            .probe(DVec3::new(500.0, 500.0, 500.0), DVec3::X, Sight::none(), None)
            .expect("the body stops the ray");
        assert_eq!(answer.distance_mm, 100.0);
        assert_eq!(answer.hit, ProbeHit { id: object, kind: SurfaceKind::Body });
    }

    #[test]
    fn an_ordinary_cell_keeps_the_planes_the_boxes_gave_it() {
        // X92: the port answers only for a body the box cannot describe, so a case of panels — and a
        // baked box standing in it as a wall — is measured exactly as it was before the port existed.
        // A cell is where a board is cut, and a plane moved by rounding is a silent defect in the part
        // made from it (ADR 0181).
        let (mut project, piece) = empty_piece();
        panel(&mut project, piece, (0, 0, 350), (600, 560, 18));
        let block =
            Mesh::cuboid([Length::ZERO; 3], [millimetres(18), millimetres(560), millimetres(350)]).expect("a block");
        let placement =
            Placement::IDENTITY.with_translation(millimetres(400), Length::ZERO, Length::ZERO).expect("a placement");
        project.add_object(piece, "Wall", placement, Geometry::Mesh(block)).expect("add a wall");
        let point = DVec3::new(200.0, 280.0, 100.0);
        let by_box = project.niche(point, Sight::none(), None).expect("the cell under the shelf");
        let by_port =
            project.niche(point, Sight::none(), Some(&contact_parry::Parry)).expect("the same cell through the port");
        assert_eq!(by_box, by_port, "the port must not move a plane the boxes already answered");
        assert!((by_port.local_max().x - 400.0).abs() < 1.0e-9, "the baked box is its own wall: {by_port:?}");

        // The probe answers the same distance and the same face with and without the adapter.
        assert_eq!(
            project.probe(DVec3::new(200.0, 280.0, 100.0), DVec3::X, Sight::none(), None),
            project.probe(DVec3::new(200.0, 280.0, 100.0), DVec3::X, Sight::none(), Some(&contact_parry::Parry)),
        );
    }

    #[test]
    fn a_probe_meets_a_balls_own_side_and_not_the_corner_of_its_box() {
        // A ball is the body the boxes cannot describe: the corner of its box stands in the free space
        // beside it, so a ray that meets that corner reads the wall the port answers rather than the
        // empty air beside the ball's own round side (X92, ADR 0181).
        let (mut project, piece) = empty_piece();
        let ball = Primitive::sphere(millimetres(100), 24, 12).expect("a ball");
        let placement = Placement::IDENTITY
            .with_translation(millimetres(300), millimetres(280), millimetres(300))
            .expect("a placement");
        let object = project.add_object(piece, "Ball", placement, Geometry::Primitive(ball)).expect("add a ball");
        let point = DVec3::new(150.0, 350.0, 300.0);
        let by_box = project.probe(point, DVec3::X, Sight::none(), None).expect("the box around the ball");
        let by_port =
            project.probe(point, DVec3::X, Sight::none(), Some(&contact_parry::Parry)).expect("the ball's own side");
        assert_eq!(by_box.distance_mm, 50.0, "the box's near face stands at 200");
        // The ray runs 70 mm off the ball's axis, so its side is sqrt(100^2 - 70^2) away from the
        // centre: 300 - 71.4 = 228.6 on the x axis, which is 78.6 mm from the point.
        assert!((by_port.distance_mm - 78.6).abs() < 1.0, "the ball's own side: {by_port:?}");
        assert_eq!(by_port.hit, ProbeHit { id: object, kind: SurfaceKind::Body });
        assert!(
            by_port.face.normal.dot(DVec3::X) < -0.5,
            "the face is the ball's side and looks back along the ray: {by_port:?}"
        );
    }

    #[test]
    fn a_niche_shrinks_around_a_part_on_the_probed_axis() {
        let (mut project, piece) = empty_piece();
        // A sheet from x = 300 to x = 318 stands across the piece.
        panel(&mut project, piece, (300, 0, 0), (18, 560, 720));
        let niche = project
            .niche(DVec3::new(450.0, 280.0, 300.0), Sight::none(), None)
            .expect("a niche to the right of the sheet");
        assert_eq!(niche.product, piece);
        assert_eq!(niche.local_min, DVec3::new(318.0, 0.0, 0.0), "the sheet is the near wall");
        assert_eq!(niche.size, DVec3::new(282.0, 560.0, 720.0));
        assert_eq!(niche.local_max(), DVec3::new(600.0, 560.0, 720.0));
        assert_eq!(niche.shape, NicheShape::Box);

        let niche = project
            .niche(DVec3::new(150.0, 280.0, 300.0), Sight::none(), None)
            .expect("a niche to the left of the sheet");
        assert_eq!(niche.local_min, DVec3::ZERO);
        assert_eq!(niche.size, DVec3::new(300.0, 560.0, 720.0), "it ends at the sheet's near face");
    }

    #[test]
    fn a_shelf_splits_the_piece_into_two_niches() {
        let (mut project, piece) = empty_piece();
        // A full-width shelf 18 mm thick, its underside at z = 350.
        panel(&mut project, piece, (0, 0, 350), (600, 560, 18));
        let below =
            project.niche(DVec3::new(300.0, 280.0, 100.0), Sight::none(), None).expect("the niche under the shelf");
        assert_eq!(below.local_min, DVec3::ZERO);
        assert_eq!(below.size, DVec3::new(600.0, 560.0, 350.0));
        assert_eq!(below.corners.len(), 8, "a box has eight corners");
        let above =
            project.niche(DVec3::new(300.0, 280.0, 500.0), Sight::none(), None).expect("the niche above the shelf");
        assert_eq!(above.local_min, DVec3::new(0.0, 0.0, 368.0), "the shelf's top face");
        assert_eq!(above.size, DVec3::new(600.0, 560.0, 352.0));
    }

    #[test]
    fn a_niche_reports_a_world_corner_and_a_shape() {
        let mut project = clean_project();
        product(&mut project, (600, 560, 720), (100, 200, 0));
        let niche = project.niche(DVec3::new(400.0, 480.0, 300.0), Sight::none(), None).expect("a niche");
        assert_eq!(niche.local_min, DVec3::ZERO, "the free box starts at the piece's own corner");
        assert_eq!(niche.min, DVec3::new(100.0, 200.0, 0.0), "and it reports that corner in the world");
        assert_eq!(niche.shape, NicheShape::Box);
        assert_eq!(niche.size, DVec3::new(600.0, 560.0, 720.0));
        assert_eq!(niche.side_mm(2), 720.0);
        assert_eq!(niche.planes.len(), 6, "the envelope's six planes bound an empty piece");
        assert_eq!(niche.corners.len(), 8);
    }

    #[test]
    fn two_angled_posts_leave_a_trapezoid_section() {
        // The inner faces run from (100, 0) to (200, 560) and from (500, 0) to (400, 560): the free
        // space between them narrows towards the back, which is a trapezoid and not a box.
        let (mut project, piece) = empty_piece();
        let slant = 100.0 / 560.0_f64.hypot(100.0);
        let drop = 560.0 / 560.0_f64.hypot(100.0);
        angular_pair(&mut project, piece, (drop, -slant), (-drop, -slant));
        let niche =
            project.niche(DVec3::new(300.0, 280.0, 300.0), Sight::none(), None).expect("a niche between the posts");
        let ring = niche.section().expect("the section is a polygon, not the box around it");
        let corners: Vec<[Length; 2]> = ring.sampled_points(1.0).expect("the ring is line only");
        let expected = [
            [millimetres(100), millimetres(0)],
            [millimetres(500), millimetres(0)],
            [millimetres(400), millimetres(560)],
            [millimetres(200), millimetres(560)],
        ];
        assert_eq!(corners, expected, "the trapezoid's own vertices, counter-clockwise from its corner");
        assert_eq!(niche.local_min, DVec3::new(100.0, 0.0, 0.0));
        assert_eq!(niche.size, DVec3::new(400.0, 560.0, 720.0), "the box around the section");
        assert_eq!(niche.corners.len(), 8, "two upright walls keep the cell a prism");
    }

    /// Places a mirrored pair of angled posts whose inner faces pass through (100, 0) and (500, 0).
    fn angular_pair(project: &mut Project, piece: Id, left_normal: (f64, f64), right_normal: (f64, f64)) {
        angled_post(project, piece, (100.0, 0.0), left_normal);
        angled_post(project, piece, (500.0, 0.0), right_normal);
    }

    #[test]
    fn a_leaned_back_wall_bounds_a_sloped_cell() {
        let (mut project, piece) = empty_piece();
        leaned_back_wall(&mut project, piece);
        let niche = project
            .niche(DVec3::new(300.0, 200.0, 300.0), Sight::none(), None)
            .expect("a cell in front of a leaned wall");
        assert_eq!(niche.shape, NicheShape::Sloped, "one ring would be false somewhere in the void");
        assert!(niche.section().is_none());
        assert_eq!(niche.local_min, DVec3::ZERO);
        assert_eq!(niche.size, DVec3::new(600.0, 560.0, 720.0), "the box around the cell");
        // The wall runs from (y = 560, z = 0) to (y = 367.2, z = 720): the top of the void is
        // 192.8 mm shallower than its floor.
        let top = niche.corners.iter().copied().filter(|corner| corner.z > 700.0);
        let deepest = top.map(|corner| corner.y).fold(f64::NEG_INFINITY, f64::max);
        assert!((deepest - 367.2).abs() < 1.0, "the cell's back edge at the top is {deepest}");
        for corner in &niche.corners {
            assert!(fits(&niche, *corner), "a corner left the cell: {corner:?}");
        }
    }

    #[test]
    fn a_shelf_under_a_leaned_wall_is_cut_where_the_wall_leans_in() {
        let (mut project, piece) = empty_piece();
        leaned_back_wall(&mut project, piece);
        let niche = project.niche(DVec3::new(300.0, 200.0, 300.0), Sight::none(), None).expect("a cell");
        // The wall leans 0.268 mm into the void per millimetre of height, so a shelf stops on the
        // wall's face at its own top instead of cutting into it.
        let low = niche.cut(2, (100.0, 118.0), (0, 1)).expect("a low shelf's footprint");
        let deepest = low.iter().map(|point| point.y).fold(f64::MIN, f64::max);
        assert!((deepest - 528.4).abs() < 1.0, "the low shelf ends on the wall at 118 mm: {deepest}");
        // The same shelf 500 mm higher is 134 mm shallower, which is the wall's own rake.
        let high = niche.cut(2, (600.0, 618.0), (0, 1)).expect("a high shelf's footprint");
        let deepest = high.iter().map(|point| point.y).fold(f64::MIN, f64::max);
        assert!((deepest - 394.4).abs() < 1.0, "the shelf ends on the wall at 618 mm: {deepest}");
        assert!(high.len() == 4, "the cut is still one rectangle: {high:?}");
    }

    #[test]
    fn a_leaned_ceiling_is_measured_over_its_whole_footprint() {
        let (mut project, piece) = empty_piece();
        leaned_ceiling(&mut project, piece);
        // The cursor's own height means nothing: the same cell answers from the front and the back.
        let front =
            project.niche(DVec3::new(300.0, 20.0, 300.0), Sight::none(), None).expect("a cell under the ceiling");
        let back =
            project.niche(DVec3::new(300.0, 540.0, 300.0), Sight::none(), None).expect("the same cell, further back");
        assert_eq!(front.shape, NicheShape::Sloped);
        assert_eq!(front.size, back.size, "the box is the void's, not the cursor's");
        assert!((front.size.z - 699.5).abs() < 0.5, "the highest point of the ceiling: {}", front.size.z);
        let shallow = |niche: &Niche| {
            niche
                .corners
                .iter()
                .copied()
                .filter(|corner| corner.y > 500.0)
                .map(|corner| corner.z)
                .fold(f64::MIN, f64::max)
        };
        assert!((shallow(&front) - 601.4).abs() < 1.0, "the ceiling is 98 mm lower at the back: {}", shallow(&front));
    }

    #[test]
    fn a_shelf_under_a_leaned_ceiling_stops_where_it_would_cut_in() {
        let (mut project, piece) = empty_piece();
        leaned_ceiling(&mut project, piece);
        let niche = project.niche(DVec3::new(300.0, 540.0, 300.0), Sight::none(), None).expect("a cell");
        let low = niche.cut(2, (100.0, 118.0), (0, 1)).expect("a low shelf's footprint");
        let deepest = low.iter().map(|point| point.y).fold(f64::MIN, f64::max);
        assert!((deepest - 560.0).abs() < 1.0, "a low shelf reaches the back: {deepest}");
        let high = niche.cut(2, (590.0, 608.0), (0, 1)).expect("a high shelf's footprint");
        let deepest = high.iter().map(|point| point.y).fold(f64::MIN, f64::max);
        assert!((deepest - 519.0).abs() < 1.5, "a shelf at 608 mm stops under the ceiling: {deepest}");
    }

    #[test]
    fn a_turned_piece_is_measured_in_its_own_frame() {
        let mut project = clean_project();
        let piece = product(&mut project, (600, 560, 720), (0, 0, 0));
        let turn = Placement::IDENTITY
            .with_rotation_euler(EulerAngles { z: Degrees::from_millidegrees(45_000), ..EulerAngles::default() })
            .expect("a turn");
        project.set_node_placement(piece, turn).expect("turn the piece");
        let inside = turn.transform_point(DVec3::new(300.0, 280.0, 300.0));
        let niche = project.niche(inside, Sight::none(), None).expect("a turned piece has a niche of its own");
        assert_eq!(niche.local_min, DVec3::ZERO, "measured in the piece's frame, not the world box");
        assert_eq!(niche.size, DVec3::new(600.0, 560.0, 720.0));
        assert_eq!(niche.shape, NicheShape::Box);
        assert_eq!(niche.min, DVec3::ZERO, "and the world corner is the piece's own zero corner");
    }

    #[test]
    fn a_body_standing_inside_the_cell_is_refused() {
        // A 90 mm block sits between the probe directions of a point in an otherwise empty piece: the
        // cell would swallow it, and a board cut to the cell would cut into it.
        let (mut project, _piece) = empty_piece();
        let root = project.root().id();
        project
            .add_object(
                root,
                "Block",
                Placement::IDENTITY
                    .with_translation(millimetres(500), millimetres(450), millimetres(200))
                    .expect("a placement"),
                sample_object(90).geometry().clone(),
            )
            .expect("add object");
        let point = DVec3::new(300.0, 280.0, 300.0);
        assert!(
            project.probe(point, DVec3::X, Sight::none(), None).is_some(),
            "the piece itself still answers a probe"
        );
        assert_eq!(
            project.niche(point, Sight::none(), None),
            Err(NicheRefusal::Unrepresentable),
            "a void holding a body is not a cell"
        );
    }

    #[test]
    fn a_wall_that_stops_halfway_still_bounds_the_void_beside_it() {
        // A post that reaches only 300 mm up still has an inner face, and the void beside it is one
        // cell. Its plane bounds the footprint for the whole height, so a shelf at any height stops
        // at the post instead of being refused (ADR 0109).
        let (mut project, piece) = empty_piece();
        let nominal = PanelNominalSize::new(millimetres(300), millimetres(560)).expect("a short post");
        let rotation = geom::DQuat::from_rotation_y(-std::f64::consts::FRAC_PI_2);
        let placement = Placement::new(DVec3::new(318.0, 0.0, 0.0), rotation).expect("a place");
        project.add_panel(piece, "Stub", nominal, None, millimetres(18), placement).expect("add a stub");
        let niche =
            project.niche(DVec3::new(150.0, 280.0, 200.0), Sight::none(), None).expect("the stub bounds the void");
        assert_eq!(niche.shape, NicheShape::Box);
        assert!((niche.local_max().x - 300.0).abs() < 1.0, "the post's inner face: {niche:?}");
        assert!((niche.size.z - 720.0).abs() < 1.0, "the case is still 720 tall: {niche:?}");
    }

    #[test]
    fn a_ceiling_that_covers_only_part_of_the_case_leaves_a_niche() {
        // The open-front case: a top over the back two thirds and two walls that stop with it. The
        // free space under the top is still one cell even though the top does not cover the whole
        // envelope, which is what the coverage test used to refuse (ADR 0109).
        let (mut project, piece) = empty_piece();
        let top = PanelNominalSize::new(millimetres(600), millimetres(400)).expect("a short top");
        let placement = Placement::IDENTITY
            .with_translation(millimetres(0), millimetres(0), millimetres(702))
            .expect("a top placement");
        project.add_panel(piece, "Top", top, None, millimetres(18), placement).expect("add a top");
        let wall = geom::DQuat::from_rotation_y(-std::f64::consts::FRAC_PI_2);
        for x in [18.0, 600.0] {
            let nominal = PanelNominalSize::new(millimetres(720), millimetres(400)).expect("a short wall");
            let placement = Placement::new(DVec3::new(x, 0.0, 0.0), wall).expect("a wall placement");
            project.add_panel(piece, "Wall", nominal, None, millimetres(18), placement).expect("add a wall");
        }
        for y in [100.0, 300.0, 500.0] {
            let niche =
                project.niche(DVec3::new(300.0, y, 300.0), Sight::none(), None).expect("a niche under the short top");
            assert_eq!(niche.shape, NicheShape::Box);
            assert!((niche.size.z - 702.0).abs() < 1.0, "the top still bounds the height: {niche:?}");
        }
    }

    #[test]
    fn a_point_outside_every_product_has_no_niche() {
        let (project, _) = empty_piece();
        assert_eq!(project.niche(DVec3::new(-100.0, 0.0, 0.0), Sight::none(), None), Err(NicheRefusal::Outside));
        assert_eq!(project.niche(DVec3::splat(f64::NAN), Sight::none(), None), Err(NicheRefusal::Outside));
    }

    #[test]
    fn a_cut_needs_three_axes_and_a_finite_slab() {
        let (project, _) = empty_piece();
        let niche = project.niche(DVec3::new(300.0, 280.0, 300.0), Sight::none(), None).expect("a niche");
        assert!(niche.cut(2, (100.0, 118.0), (0, 1)).is_some());
        assert_eq!(niche.cut(2, (100.0, 118.0), (2, 1)), None, "the cut cannot be seen along its own axis");
        assert_eq!(niche.cut(3, (100.0, 118.0), (0, 1)), None);
        assert_eq!(niche.cut(2, (f64::NAN, 0.0), (0, 1)), None);
        assert_eq!(niche.cut(2, (800.0, 820.0), (0, 1)), None, "a slab above the void has no footprint");
    }

    #[test]
    fn a_probe_needs_a_direction_and_finite_numbers() {
        let (project, _) = empty_piece();
        let point = DVec3::new(300.0, 280.0, 300.0);
        assert_eq!(project.probe(point, DVec3::ZERO, Sight::none(), None), None);
        assert_eq!(project.probe(DVec3::splat(f64::INFINITY), DVec3::X, Sight::none(), None), None);
        assert_eq!(project.probe(point, DVec3::splat(f64::NAN), Sight::none(), None), None);
    }

    /// The sight that switches `node` off, owned by the test so the borrow outlives the query.
    fn hiding(node: Id) -> HashSet<Id> {
        [node].into_iter().collect()
    }

    #[test]
    fn a_probe_passes_a_part_the_sight_switched_off() {
        let (mut project, piece) = empty_piece();
        let partition = panel(&mut project, piece, (300, 0, 0), (18, 560, 720));
        let point = DVec3::new(100.0, 280.0, 300.0);

        let visible = project.probe(point, DVec3::X, Sight::none(), None).expect("the sheet stops the ray");
        assert_eq!(visible.hit, ProbeHit { id: partition, kind: SurfaceKind::Panel });

        let hidden = hiding(partition);
        let passed = project.probe(point, DVec3::X, Sight::of(&hidden), None).expect("the ray reaches the far wall");
        assert_eq!(passed.distance_mm, 500.0, "it crosses the sheet and stops at the envelope");
        assert_eq!(passed.hit, ProbeHit { id: piece, kind: SurfaceKind::Envelope });
    }

    #[test]
    fn a_niche_spans_a_part_the_sight_switched_off() {
        let (mut project, piece) = empty_piece();
        let partition = panel(&mut project, piece, (300, 0, 0), (18, 560, 720));
        let point = DVec3::new(150.0, 280.0, 300.0);

        let visible = project.niche(point, Sight::none(), None).expect("the void in front of the sheet");
        assert_eq!(visible.size, DVec3::new(300.0, 560.0, 720.0));

        let hidden = hiding(partition);
        let spanned = project.niche(point, Sight::of(&hidden), None).expect("the hidden sheet is no wall");
        assert_eq!(spanned.local_min, DVec3::ZERO, "the cell reaches the case's own walls");
        assert_eq!(spanned.size, DVec3::new(600.0, 560.0, 720.0));
        assert_eq!(spanned.shape, NicheShape::Box, "a body the eye cannot see is not a body in the void");
    }

    #[test]
    fn a_hidden_container_hides_its_whole_subtree_from_the_tools() {
        let (mut project, piece) = empty_piece();
        let drawer = product(&mut project, (300, 300, 300), (100, 100, 100));
        let sheet = panel(&mut project, drawer, (100, 0, 0), (18, 300, 300));
        let point = DVec3::new(50.0, 250.0, 250.0);

        let visible = project.probe(point, DVec3::X, Sight::none(), None).expect("the drawer's shell stops the ray");
        assert_eq!(visible.hit, ProbeHit { id: drawer, kind: SurfaceKind::Shell });

        let hidden = hiding(drawer);
        let sight = Sight::of(&hidden);
        let passed = project.probe(point, DVec3::X, sight, None).expect("the ray crosses the whole hidden subtree");
        assert_eq!(passed.hit, ProbeHit { id: piece, kind: SurfaceKind::Envelope });
        assert_ne!(passed.hit.id, sheet, "the sheet goes with the container that holds it");
    }
}
