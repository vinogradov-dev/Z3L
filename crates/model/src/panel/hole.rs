//! The holes a panel gets from the objects that drill it, derived and never stored.
//!
//! A panel stores what a person authored — its outline, its banding, its machining (`machining.rs`)
//! — and no drilled hole (ADR 0150). A hinge's cup bore belongs to the hinge, a runner's screw holes
//! to the runner, and a shelf pin to whatever placed it: the panel only receives the **crossing** of
//! a drilling object with its own body. That is what this module computes.
//!
//! A [`Drilling`] is a drilling object resolved into world millimetres — a start point, a travel
//! direction, a diameter and a depth. Resolving one out of a part document is the instance
//! resolver's job; this module sees only the resolved value, so a test can hand it a hinge without a
//! directory. [`derive_holes`] clips every drilling against the panel's *material* — the core with
//! its through cutouts removed, not the nominal rectangle — and answers one [`DerivedHoles`]: holes
//! in the panel's own frame, ordered by coordinates, plus the diagnostics of the crossings that are
//! degenerate.
//!
//! The values are derived geometry, held in integer micrometres like every persisted length: they
//! are never serialized, never a panel field, and the cache in `mesh` keys them by the panel, the
//! placements and the directory revision.

use geom::{DVec2, DVec3, Region, subtract_regions};
use thiserror::Error;
use units::Length;

use crate::{CutoutDepth, Id, Panel, PanelGeometry, Placement, PlacementError, ProjectError};

/// The shortest crossing a drilling object may have, in millimetres.
///
/// One micrometre is the document's own resolution, so anything shorter is a tangent or a graze
/// rather than a hole.
const CROSSING_EPSILON_MM: f64 = 0.001;

/// A drilling object resolved into world millimetres.
///
/// A part document carries drilling objects as path nodes with a diameter and a depth; an instance
/// places them with its own placement. This value is the result of that resolution: where the drill
/// starts, which way it travels, how wide it is and how far it goes. The direction is stored as a
/// unit vector, and the value is a geometry-boundary value like a [`Placement`], never document data.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drilling {
    source: Id,
    entry: DVec3,
    direction: DVec3,
    diameter: Length,
    depth: Length,
}

impl Drilling {
    /// Builds a drilling object from its resolved geometry.
    ///
    /// The direction is normalized, so a caller may hand over any non-zero vector. A non-finite
    /// point, a zero direction, a non-positive diameter or depth is a typed error, never a silent
    /// fallback: a hole in the wrong place is worse than a rule that says why.
    pub fn new(source: Id, entry: DVec3, direction: DVec3, diameter: Length, depth: Length) -> Result<Self, HoleError> {
        if !entry.is_finite() {
            return Err(HoleError::NonFiniteEntry);
        }
        if !direction.is_finite() || direction.length() <= CROSSING_EPSILON_MM {
            return Err(HoleError::DegenerateDirection);
        }
        if !diameter.is_positive() {
            return Err(HoleError::NonPositiveDiameter { diameter });
        }
        if !depth.is_positive() {
            return Err(HoleError::NonPositiveDepth { depth });
        }
        Ok(Self { source, entry, direction: direction.normalize(), diameter, depth })
    }

    /// The node that causes the hole.
    pub const fn source(self) -> Id {
        self.source
    }

    /// The point the drill starts from, in world millimetres.
    pub const fn entry(self) -> DVec3 {
        self.entry
    }

    /// The unit direction of travel, in world space.
    pub const fn direction(self) -> DVec3 {
        self.direction
    }

    /// The diameter of the bore.
    pub const fn diameter(self) -> Length {
        self.diameter
    }

    /// How far the drill travels from [`Drilling::entry`].
    pub const fn depth(self) -> Length {
        self.depth
    }
}

/// One hole: the crossing of a drilling object with one panel.
///
/// The coordinates are panel-local micrometres — `x` along the length, `y` along the width and `z`
/// across the thickness from face `B` at zero — so two builds produce the same numbers and a report
/// diff is readable. The entry is the point of the material where the bore starts; the depth is
/// measured from there along [`Hole::direction`], and a through hole's depth is the material the
/// bore crosses.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hole {
    source: Id,
    entry: [Length; 3],
    direction: DVec3,
    diameter: Length,
    depth: Length,
    through: bool,
}

impl Hole {
    /// The node that causes the hole.
    pub const fn source(&self) -> Id {
        self.source
    }

    /// Where the bore enters the material, in the panel's local frame.
    pub const fn entry(&self) -> [Length; 3] {
        self.entry
    }

    /// The unit direction of the bore, in the panel's local frame.
    pub const fn direction(&self) -> DVec3 {
        self.direction
    }

    /// The diameter of the bore.
    pub const fn diameter(&self) -> Length {
        self.diameter
    }

    /// How deep the bore goes from its entry.
    pub const fn depth(&self) -> Length {
        self.depth
    }

    /// Whether the bore crosses the whole material it met.
    pub const fn is_through(&self) -> bool {
        self.through
    }
}

/// Why a drilling object does not make a clean hole in one panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoleDiagnostic {
    /// The drilling object touches the panel but removes nothing: it grazes a face or a wall.
    Grazing {
        /// The node that causes the crossing.
        source: Id,
    },
    /// The bore is wider than the material it enters, so it would break out of the panel.
    TooWide {
        /// The node that causes the crossing.
        source: Id,
    },
}

impl HoleDiagnostic {
    /// The node the diagnostic is about.
    pub const fn source(self) -> Id {
        match self {
            Self::Grazing { source } | Self::TooWide { source } => source,
        }
    }
}

/// The holes one panel receives from its drilling objects, and the crossings that are degenerate.
///
/// The holes are ordered by their entry point in panel-local coordinates, then by diameter and
/// source, so the list is stable across builds and two panels never disagree about the order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DerivedHoles {
    holes: Vec<Hole>,
    diagnostics: Vec<HoleDiagnostic>,
}

impl DerivedHoles {
    /// The holes, ordered.
    pub fn holes(&self) -> &[Hole] {
        &self.holes
    }

    /// The crossings that are degenerate, ordered by source.
    pub fn diagnostics(&self) -> &[HoleDiagnostic] {
        &self.diagnostics
    }

    /// Whether the panel receives no hole.
    pub fn is_empty(&self) -> bool {
        self.holes.is_empty()
    }
}

/// Why a drilling object or the crossing of it with a panel is not usable.
#[derive(Clone, Debug, Error)]
pub enum HoleError {
    /// A drilling object's start point is not finite.
    #[error("a drilling object's entry point is not a finite point")]
    NonFiniteEntry,
    /// A drilling object's direction is absent or not finite.
    #[error("a drilling object's direction is not a usable vector")]
    DegenerateDirection,
    /// A drilling object's diameter is zero or negative.
    #[error("a drilling object's diameter must be positive, got {diameter:?}")]
    NonPositiveDiameter {
        /// The rejected diameter.
        diameter: Length,
    },
    /// A drilling object's depth is zero or negative.
    #[error("a drilling object's depth must be positive, got {depth:?}")]
    NonPositiveDepth {
        /// The rejected depth.
        depth: Length,
    },
    /// The panel's world placement cannot be inverted, so nothing can be brought into its frame.
    #[error("the panel's world placement cannot be inverted: {0}")]
    Frame(#[from] PlacementError),
    /// A stored cutout cannot become a region.
    #[error(transparent)]
    Machining(#[from] ProjectError),
    /// The panel's remaining material cannot be computed.
    #[error("the panel's material cannot be computed: {0}")]
    Material(#[from] geom::RegionError),
    /// A crossing of this panel falls outside the micrometre range a length may hold.
    #[error("the crossing of drilling object {object} falls outside the range of a length")]
    OutOfRange {
        /// The node that causes the crossing.
        object: Id,
    },
}

/// Derives the holes one panel receives from the drilling objects that reach it.
///
/// `placement` is the panel's world placement: the drilling objects are given in world space and are
/// brought into the panel's own frame exactly once. `panel` supplies its authored cutouts, because
/// the crossing is clipped against the material the person actually made — a bore near a rounded
/// corner or beside a notch follows the body, never the nominal rectangle.
///
/// A drilling object that the panel's body does not meet yields nothing: a hinge's screws reach the
/// carcass side, not the door, and every panel is offered every drilling object. A crossing that
/// touches the body without removing material, or a bore wider than the material it enters, is a
/// [`HoleDiagnostic`] naming the object — never a silently skipped hole.
pub fn derive_holes(
    panel: &Panel,
    geometry: &PanelGeometry,
    placement: Placement,
    drillings: &[Drilling],
) -> Result<DerivedHoles, HoleError> {
    if drillings.is_empty() {
        return Ok(DerivedHoles::default());
    }
    let to_local = placement.inverse()?;
    let material = material_region(panel, geometry)?;
    let thickness = geometry.thickness().millimetres_f64();
    let mut holes = Vec::new();
    let mut diagnostics = Vec::new();
    for drilling in drillings {
        match crossing(drilling, &to_local, &material, thickness)? {
            Crossing::Hole { hole, diagnostic } => {
                holes.push(hole);
                if let Some(diagnostic) = diagnostic {
                    diagnostics.push(diagnostic);
                }
            }
            Crossing::Grazing => diagnostics.push(HoleDiagnostic::Grazing { source: drilling.source() }),
            Crossing::Missing => {}
        }
    }
    // Two builds of one panel must produce the same list, so the order is a function of the values
    // and of nothing else: the entry point first, then the width, then the object that caused it.
    holes.sort_by(|first, second| {
        first.entry.cmp(&second.entry).then(first.diameter.cmp(&second.diameter)).then(first.source.cmp(&second.source))
    });
    diagnostics.sort_by_key(|diagnostic| diagnostic.source());
    Ok(DerivedHoles { holes, diagnostics })
}

/// What one drilling object does to the panel.
enum Crossing {
    /// The bore crosses material.
    Hole {
        hole: Hole,
        /// The bore is wider than the material it enters.
        diagnostic: Option<HoleDiagnostic>,
    },
    /// The drilling object touches the body but removes nothing.
    Grazing,
    /// The panel's body never meets the drilling object.
    Missing,
}

/// Clips one drilling object against the material prism `region × [0, thickness]`.
fn crossing(
    drilling: &Drilling,
    to_local: &Placement,
    material: &Region,
    thickness: f64,
) -> Result<Crossing, HoleError> {
    let origin = to_local.transform_point(drilling.entry());
    let direction = to_local.transform_vector(drilling.direction());
    if !origin.is_finite() || !direction.is_finite() {
        return Ok(Crossing::Missing);
    }
    let reach = drilling.depth().millimetres_f64();
    let radius = drilling.diameter().millimetres_f64() * 0.5;

    // The slab across the thickness: the same parameter as the in-plane spans, so the two clips
    // meet in one interval without a second unit.
    let Some((z_from, z_to)) = slab_interval(origin.z, direction.z, thickness) else {
        return Ok(Crossing::Missing);
    };
    let in_plane = DVec2::new(direction.x, direction.y);
    let in_plane_spans = if in_plane.length() <= CROSSING_EPSILON_MM {
        if material.contains(DVec2::new(origin.x, origin.y)) {
            vec![(f64::NEG_INFINITY, f64::INFINITY)]
        } else {
            return Ok(Crossing::Missing);
        }
    } else {
        material.line_intervals(DVec2::new(origin.x, origin.y), in_plane)
    };

    // The first material span the drill meets, clipped to the slab and to the drill's own reach.
    let mut touched = false;
    for (from, to) in in_plane_spans {
        let (enter, exit) = (from.max(z_from), to.min(z_to));
        if exit < enter - CROSSING_EPSILON_MM {
            continue;
        }
        // A span the drill's own travel never meets belongs to another object, not to this panel.
        if exit < -CROSSING_EPSILON_MM || enter > reach + CROSSING_EPSILON_MM {
            continue;
        }
        touched = true;
        let start = enter.max(0.0);
        let stop = exit.min(reach);
        if stop - start <= CROSSING_EPSILON_MM {
            continue;
        }
        let entry_point = origin + direction * start;
        let out_of_range = || HoleError::OutOfRange { object: drilling.source() };
        let entry = lengths_of(entry_point).ok_or_else(out_of_range)?;
        let depth = length_of(stop - start).ok_or_else(out_of_range)?;
        let through = exit <= reach + CROSSING_EPSILON_MM;
        let hole = Hole { source: drilling.source(), entry, direction, diameter: drilling.diameter(), depth, through };
        // The boundary met later along the axis is the surface the bore enters through.
        let bore = Bore { entry: entry_point, direction, radius, axis_start: enter, enters_on_a_face: from <= z_from };
        let diagnostic =
            bore.breaks_out(material, thickness).then_some(HoleDiagnostic::TooWide { source: drilling.source() });
        return Ok(Crossing::Hole { hole, diagnostic });
    }
    Ok(if touched { Crossing::Grazing } else { Crossing::Missing })
}

/// One bore as it meets the material: what the width rule reads.
struct Bore {
    entry: DVec3,
    direction: DVec3,
    radius: f64,
    /// The parameter where the axis enters the material: negative when the drill starts inside it.
    axis_start: f64,
    /// Whether the first boundary the axis crosses is a large face rather than the outline wall.
    enters_on_a_face: bool,
}

impl Bore {
    /// Whether the bore breaks out of the panel at the boundary it enters through.
    ///
    /// For a bore through a large face the circle it cuts must lie inside the material's outline; for
    /// a bore through the outline wall the circle must fit across the thickness. The circle is
    /// measured on the plane perpendicular to the axis, so an oblique bore is judged by the ellipse it
    /// projects onto the boundary it enters, which is exact for the face-normal and wall-parallel
    /// bores a shop drills.
    fn breaks_out(&self, material: &Region, thickness: f64) -> bool {
        // A drill that starts inside the material crosses no boundary on its way in.
        if self.axis_start < -CROSSING_EPSILON_MM {
            return false;
        }
        let normal = self.direction.z.abs();
        if self.enters_on_a_face {
            if normal <= CROSSING_EPSILON_MM {
                return false;
            }
            let projected = self.radius / normal;
            material.boundary_distance(DVec2::new(self.entry.x, self.entry.y)) < projected - CROSSING_EPSILON_MM
        } else {
            let across_thickness = self.radius * (1.0 - normal * normal).max(0.0).sqrt();
            self.entry.z.min(thickness - self.entry.z) < across_thickness - CROSSING_EPSILON_MM
        }
    }
}

/// The material the panel actually has: its core with every through cutout removed.
///
/// A through cutout is subtracted exactly, so the crossing follows a notch or a hole in the board.
/// A blind cutout removes no through material and is left to the solid alone: a bore that lands in
/// one is a question for the report, not for the hole's coordinates.
fn material_region(panel: &Panel, geometry: &PanelGeometry) -> Result<Region, HoleError> {
    let mut cuts = Vec::new();
    for machining in panel.machining() {
        let Some(cutout) = machining.cutout() else {
            continue;
        };
        if matches!(cutout.depth(), CutoutDepth::Through) {
            cuts.push(cutout.shape().region()?);
        }
    }
    if cuts.is_empty() {
        return Ok(geometry.core_region().clone());
    }
    let remaining = subtract_regions(std::slice::from_ref(geometry.core_region()), &cuts)?;
    // The document refuses a through cutout that empties or splits the core, so the remainder is one
    // positive region; a file that still holds one keeps the largest piece instead of a panic.
    let mut best: Option<Region> = None;
    for region in remaining {
        if best.as_ref().is_none_or(|current: &Region| region.area() > current.area()) {
            best = Some(region);
        }
    }
    Ok(best.unwrap_or_else(|| geometry.core_region().clone()))
}

/// The parameter span of the line through the slab `0..=thickness`, or `None` when it never enters.
fn slab_interval(origin_z: f64, direction_z: f64, thickness: f64) -> Option<(f64, f64)> {
    if direction_z.abs() <= CROSSING_EPSILON_MM {
        return (origin_z >= -CROSSING_EPSILON_MM && origin_z <= thickness + CROSSING_EPSILON_MM)
            .then_some((f64::NEG_INFINITY, f64::INFINITY));
    }
    let first = -origin_z / direction_z;
    let second = (thickness - origin_z) / direction_z;
    let (from, to) = if first <= second { (first, second) } else { (second, first) };
    (to - from > CROSSING_EPSILON_MM).then_some((from, to))
}

/// Rounds a local millimetre point to exact micrometres.
fn lengths_of(point: DVec3) -> Option<[Length; 3]> {
    Some([length_of(point.x)?, length_of(point.y)?, length_of(point.z)?])
}

/// Rounds one local millimetre coordinate to exact micrometres.
fn length_of(value: f64) -> Option<Length> {
    if !value.is_finite() {
        return None;
    }
    let micrometres = (value * 1_000.0).round();
    if !(micrometres >= i64::MIN as f64 && micrometres < i64::MAX as f64) {
        return None;
    }
    Some(Length::from_micrometres(micrometres as i64))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use std::f64::consts::FRAC_PI_2;

    use geom::{DQuat, DVec3};

    use super::*;
    use crate::fixtures::*;
    use crate::{
        CornerFeature, CornerFeatureKind, Cutout, CutoutShape, FaceSide, MachiningAnchor, MachiningId, MachiningParams,
        PanelNominalSize, RectCorner,
    };

    /// A door the size a kitchen uses, resolved at its own thickness.
    fn door() -> (Panel, PanelGeometry) {
        let panel = Panel::new(
            Id::new(2).expect("id"),
            "Door",
            PanelNominalSize::new(millimetres(700), millimetres(500)).expect("a door size"),
            None,
            millimetres(18),
            Placement::IDENTITY,
        )
        .expect("a door");
        let geometry = panel.geometry_with_fallbacks().expect("door geometry");
        (panel, geometry)
    }

    /// A drilling object placed in the panel's frame and then in the world by `placement`.
    fn drilling_in(
        placement: Placement,
        source: u64,
        local: DVec3,
        direction: DVec3,
        diameter: i64,
        depth: i64,
    ) -> Drilling {
        Drilling::new(
            Id::new(source).expect("id"),
            placement.transform_point(local),
            placement.transform_vector(direction),
            millimetres(diameter),
            millimetres(depth),
        )
        .expect("a drilling object")
    }

    fn entry(hole: &Hole) -> [Length; 3] {
        hole.entry()
    }

    #[test]
    fn a_hinge_on_a_placed_door_yields_an_exact_hole_set() {
        let (panel, geometry) = door();
        // The door stands at the far end of a carcass, turned a quarter turn: the drilling objects are
        // given in world space and must come back to panel-local coordinates exactly.
        let placement = Placement::new(DVec3::new(1_000.0, 500.0, 700.0), DQuat::from_rotation_z(FRAC_PI_2))
            .expect("a placed door");
        let cup = drilling_in(placement, 11, DVec3::new(350.0, 22.0, 18.0), -DVec3::Z, 35, 13);
        let upper_screw = drilling_in(placement, 12, DVec3::new(350.0, 40.0, 18.0), -DVec3::Z, 5, 13);
        let lower_screw = drilling_in(placement, 13, DVec3::new(350.0, 4.0, 18.0), -DVec3::Z, 5, 13);
        let holes = derive_holes(&panel, &geometry, placement, &[lower_screw, cup, upper_screw]).expect("holes");
        assert!(holes.diagnostics().is_empty(), "{:?}", holes.diagnostics());
        let holes = holes.holes();
        assert_eq!(holes.len(), 3, "the hinge drills three holes");
        // The list is ordered by coordinates, not by the order the objects arrived in.
        assert_eq!(entry(&holes[0]), [millimetres(350), millimetres(4), millimetres(18)]);
        assert_eq!(entry(&holes[1]), [millimetres(350), millimetres(22), millimetres(18)]);
        assert_eq!(entry(&holes[2]), [millimetres(350), millimetres(40), millimetres(18)]);
        let cup = holes[1];
        assert_eq!(cup.source(), Id::new(11).expect("id"));
        assert_eq!(cup.diameter(), millimetres(35));
        assert_eq!(cup.depth(), millimetres(13));
        assert!(!cup.is_through(), "a cup bore stops inside a door");
        assert_eq!(cup.direction(), -DVec3::Z, "the bore enters face A and travels down");
    }

    #[test]
    fn the_same_door_built_twice_yields_the_same_list() {
        let (panel, geometry) = door();
        let first = drilling_in(Placement::IDENTITY, 21, DVec3::new(100.0, 100.0, 18.0), -DVec3::Z, 35, 13);
        let second = drilling_in(Placement::IDENTITY, 22, DVec3::new(120.0, 80.0, 18.0), -DVec3::Z, 5, 13);
        let forward = derive_holes(&panel, &geometry, Placement::IDENTITY, &[first, second]).expect("holes");
        let backward = derive_holes(&panel, &geometry, Placement::IDENTITY, &[second, first]).expect("holes");
        assert_eq!(forward, backward);
    }

    #[test]
    fn a_drilling_object_that_misses_the_panel_yields_nothing() {
        let (panel, geometry) = door();
        // The hinge's screws reach the carcass side; the door is offered every object and meets none.
        let beside = drilling_in(Placement::IDENTITY, 31, DVec3::new(350.0, 600.0, 18.0), -DVec3::Z, 5, 13);
        let under = drilling_in(Placement::IDENTITY, 32, DVec3::new(350.0, 22.0, -40.0), -DVec3::Z, 35, 13);
        let holes = derive_holes(&panel, &geometry, Placement::IDENTITY, &[beside, under]).expect("holes");
        assert!(holes.is_empty(), "{:?}", holes.holes());
        assert!(holes.diagnostics().is_empty(), "a miss is not a defect");
    }

    #[test]
    fn a_through_drill_stops_at_the_far_face() {
        let (panel, geometry) = door();
        let drill = drilling_in(Placement::IDENTITY, 41, DVec3::new(100.0, 100.0, 18.0), -DVec3::Z, 8, 40);
        let holes = derive_holes(&panel, &geometry, Placement::IDENTITY, &[drill]).expect("holes");
        let hole = holes.holes()[0];
        assert!(hole.is_through(), "a drill longer than the board crosses it");
        assert_eq!(hole.depth(), millimetres(18), "the bore is the material it crossed");
        assert_eq!(entry(&hole), [millimetres(100), millimetres(100), millimetres(18)]);
    }

    #[test]
    fn a_blind_drill_stops_at_its_own_depth() {
        let (panel, geometry) = door();
        let drill = drilling_in(Placement::IDENTITY, 42, DVec3::new(100.0, 100.0, 0.0), DVec3::Z, 8, 6);
        let holes = derive_holes(&panel, &geometry, Placement::IDENTITY, &[drill]).expect("holes");
        let hole = holes.holes()[0];
        assert!(!hole.is_through());
        assert_eq!(hole.depth(), millimetres(6));
        assert_eq!(entry(&hole), [millimetres(100), millimetres(100), Length::ZERO], "it enters face B");
        assert_eq!(hole.direction(), DVec3::Z);
    }

    #[test]
    fn a_bore_through_the_edge_wall_starts_on_the_wall() {
        let (panel, geometry) = door();
        // A confirmat into the end of a shelf: the bore enters through the outline, not a face.
        let drill = drilling_in(Placement::IDENTITY, 51, DVec3::new(-20.0, 100.0, 9.0), DVec3::X, 7, 50);
        let holes = derive_holes(&panel, &geometry, Placement::IDENTITY, &[drill]).expect("holes");
        let hole = holes.holes()[0];
        assert_eq!(entry(&hole), [Length::ZERO, millimetres(100), millimetres(9)]);
        assert_eq!(hole.depth(), millimetres(30), "the depth starts where the bore meets the material");
        assert!(!hole.is_through());
        assert!(holes.diagnostics().is_empty(), "{:?}", holes.diagnostics());
    }

    #[test]
    fn a_bore_near_a_rounded_corner_follows_the_body_not_the_rectangle() {
        let (mut panel, _) = door();
        panel.upsert_corner_feature(
            CornerFeature::new(RectCorner::TopRight.id(), CornerFeatureKind::Radius { radius: millimetres(30) })
                .expect("a corner radius"),
        );
        let geometry = panel.geometry_with_fallbacks().expect("shaped geometry");
        // Down through the top wall at x = 690: a nominal rectangle would answer y = 500, the 30 mm
        // round answers y = 470 + sqrt(30² - 20²).
        let drill = drilling_in(Placement::IDENTITY, 61, DVec3::new(690.0, 510.0, 9.0), -DVec3::Y, 7, 60);
        let holes = derive_holes(&panel, &geometry, Placement::IDENTITY, &[drill]).expect("holes");
        let hole = holes.holes()[0];
        let expected = (470.0 + (900.0_f64 - 400.0).sqrt()) * 1_000.0;
        let [_, y, z] = entry(&hole);
        assert!((y.micrometres() as f64 - expected).abs() <= 1.0, "the wall is the arc, not the edge: {y:?}");
        assert_eq!(z, millimetres(9));
    }

    #[test]
    fn a_bore_that_would_break_out_of_the_edge_is_a_diagnostic() {
        let (panel, geometry) = door();
        // A 40 mm bore centred 10 mm from the front edge leaves the board by 10 mm.
        let drill = drilling_in(Placement::IDENTITY, 71, DVec3::new(300.0, 10.0, 18.0), -DVec3::Z, 40, 13);
        let holes = derive_holes(&panel, &geometry, Placement::IDENTITY, &[drill]).expect("holes");
        assert_eq!(holes.holes().len(), 1, "a defect never skips the hole silently");
        assert_eq!(holes.diagnostics(), &[HoleDiagnostic::TooWide { source: Id::new(71).expect("id") }]);
    }

    #[test]
    fn a_drill_that_touches_the_face_and_turns_away_grazes() {
        let (panel, geometry) = door();
        let drill = drilling_in(Placement::IDENTITY, 81, DVec3::new(300.0, 300.0, 18.0), DVec3::Z, 10, 6);
        let holes = derive_holes(&panel, &geometry, Placement::IDENTITY, &[drill]).expect("holes");
        assert!(holes.is_empty());
        assert_eq!(holes.diagnostics(), &[HoleDiagnostic::Grazing { source: Id::new(81).expect("id") }]);
    }

    #[test]
    fn a_through_cutout_removes_the_material_a_bore_would_cross() {
        let (mut panel, _) = door();
        panel.push_machining(crate::Machining::new(
            MachiningId::new(7).expect("machining id"),
            MachiningAnchor::face(FaceSide::A),
            MachiningParams::Cutout(Cutout::new(
                CutoutShape::Rectangle(sample_rect(100, 100, 200, 200)),
                CutoutDepth::Through,
            )),
        ));
        let geometry = panel.geometry_with_fallbacks().expect("geometry with a cutout");
        let in_the_hole = drilling_in(Placement::IDENTITY, 91, DVec3::new(200.0, 200.0, 18.0), -DVec3::Z, 35, 13);
        let beside_it = drilling_in(Placement::IDENTITY, 92, DVec3::new(400.0, 200.0, 18.0), -DVec3::Z, 35, 13);
        let holes = derive_holes(&panel, &geometry, Placement::IDENTITY, &[in_the_hole, beside_it]).expect("holes");
        assert_eq!(holes.holes().len(), 1, "the bore over the cutout crosses air");
        assert_eq!(holes.holes()[0].source(), Id::new(92).expect("id"));
    }

    #[test]
    fn a_drilling_object_refuses_a_degenerate_value() {
        let source = Id::new(1).expect("id");
        assert!(matches!(
            Drilling::new(source, DVec3::new(f64::NAN, 0.0, 0.0), DVec3::Z, millimetres(5), millimetres(5)),
            Err(HoleError::NonFiniteEntry)
        ));
        assert!(matches!(
            Drilling::new(source, DVec3::ZERO, DVec3::ZERO, millimetres(5), millimetres(5)),
            Err(HoleError::DegenerateDirection)
        ));
        assert!(matches!(
            Drilling::new(source, DVec3::ZERO, DVec3::Z, Length::ZERO, millimetres(5)),
            Err(HoleError::NonPositiveDiameter { .. })
        ));
        assert!(matches!(
            Drilling::new(source, DVec3::ZERO, DVec3::Z, millimetres(5), Length::ZERO),
            Err(HoleError::NonPositiveDepth { .. })
        ));
    }
}
