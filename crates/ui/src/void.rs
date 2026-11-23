//! The niche atom: the empty space around the point under the pointer (ADR 0103, ADR 0118).
//!
//! A niche is a measurement, not a document object: [`Project::niche`] answers the convex cell of
//! wall planes around a point, and every consumer of that answer reads it from here instead of
//! probing the document again. The fill tools turn it into the part a click would create, a report
//! row names it, and the assistant will ask the same question — one measurement, one value.
//!
//! The atom is deliberately dumb about input: it stores the point a pointer resolved to and the
//! cell the model answered, and it knows nothing about the camera, the snapping, the cursor or the
//! side the tool is on. The shell resolves a viewport position to a point inside the open product
//! ([`pointer_point`], the atom's own cursor rule) and hands it here, so the niche cannot grow a
//! second point path.
//!
//! A pointer that moves less than [`MEASURE_STEP_MM`] re-measures nothing, and a point outside every
//! product is a refusal rather than an empty value, because the report row has to say why the space
//! has no name. The same ray answers the fill's free axis ([`free_place`]): which place along the
//! axis the point names, and whether the face under it can name one at all. Nothing here writes:
//! there is no document field, no revision and no undo entry.
//!
//! Changes when: a second niche shape needs the corners rather than the box, or the cursor rule for
//! the point inside the product changes.

use geom::{Contact, DVec2, DVec3, Region};
use model::{Envelope, Id, Niche, NicheRefusal, Project, Sight, SurfaceKind};
use scene::Ray;
use text::Message;
use units::Length;

/// How far the pointer may move, in millimetres, before the void is measured again.
pub const MEASURE_STEP_MM: f64 = 0.5;

/// How far inside the product the measured point is moved, in millimetres.
///
/// The envelope is a boundary, and a probe that starts exactly on it sees a zero-length face; one
/// micrometre of the document's own resolution puts the point in the free space without moving the
/// measurement a maker could see.
const ENTRY_INSET_MM: f64 = 1.0e-3;

/// How far off a surface the measured point rests, in millimetres.
///
/// A face is the wall of the void, so the point sits a micrometre on the free side of it. The same
/// resolution keeps a probe from starting on the face it just measured (ADR 0109).
const FACE_INSET_MM: f64 = 1.0e-3;

/// Why the point under the pointer names no niche.
///
/// The model's own refusal, kept as a value so every surface prints the same sentence instead of
/// inventing one: a point that is outside every product, and a space that is not one convex cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoidRefusal {
    /// The point is outside every product, or is not a usable number.
    Outside,
    /// The free space around the point is not one convex cell: it holds something, or a wall does
    /// not reach the face the cell would need.
    Unrepresentable,
    /// The open product's frame is not convex, so the space it reserves is not one cell either.
    Footprint,
}

impl VoidRefusal {
    /// The model's refusal in the atom's own vocabulary.
    pub const fn of(refusal: NicheRefusal) -> Self {
        match refusal {
            NicheRefusal::Outside => VoidRefusal::Outside,
            NicheRefusal::Unrepresentable => VoidRefusal::Unrepresentable,
            NicheRefusal::Footprint => VoidRefusal::Footprint,
        }
    }

    /// The sentence a surface prints, in the language it draws.
    pub const fn words(self) -> Message {
        match self {
            VoidRefusal::Outside => Message::VoidOutside,
            VoidRefusal::Unrepresentable => Message::VoidUnrepresentable,
            VoidRefusal::Footprint => Message::VoidFootprint,
        }
    }
}

/// The live niche: the point a pointer resolved to and the cell the model answered around it.
///
/// The value follows the pointer while a niche tool or a fill tool is armed, and it is dropped when
/// that tool leaves; nothing is stored in the document and no revision changes. The same struct is
/// what the fill plan, the status bar and a test read, so one pointer has one answer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Void {
    measured: Option<Measured>,
}

/// One measurement: the point, and the cell or the reason there is none.
#[derive(Clone, Debug, PartialEq)]
struct Measured {
    point: DVec3,
    /// The cell the model answered, or the refusal that stands in its place.
    niche: Result<Niche, VoidRefusal>,
}

impl Void {
    /// A niche atom that has measured nothing.
    pub const fn new() -> Self {
        Self { measured: None }
    }

    /// Measures the void around `point` unless the pointer has not moved, and says whether the value
    /// changed.
    ///
    /// The step is the pointer's own resolution, not the document's: a hand that holds still gains no
    /// new answer, and a measured point that stays inside the same cell keeps the value it has. A
    /// point that is not finite is the model's refusal, not a panic.
    ///
    /// `sight` is what the cell is measured through: a part the user switched off is invisible to the
    /// eye and to the pointer, so the void behind it is the void the maker reaches (X73).
    pub fn measure(
        &mut self,
        project: &Project,
        point: DVec3,
        sight: Sight<'_>,
        contact: Option<&dyn Contact>,
    ) -> bool {
        if let Some(measured) = &self.measured
            && (measured.point - point).length() < MEASURE_STEP_MM
        {
            return false;
        }
        let niche = project.niche(point, sight, contact).map_err(VoidRefusal::of);
        self.measured = Some(Measured { point, niche });
        true
    }

    /// Drops the measurement; the next pointer resolves it again.
    ///
    /// Returns whether there was a value to drop, so a caller that repaints on a change can tell a
    /// cleared cell from an empty atom.
    pub fn clear(&mut self) -> bool {
        self.measured.take().is_some()
    }

    /// The point the value was measured at, or `None` when nothing was.
    pub fn point(&self) -> Option<DVec3> {
        self.measured.as_ref().map(|measured| measured.point)
    }

    /// The cell around the point, once the model answered one.
    pub fn niche(&self) -> Option<&Niche> {
        self.measured.as_ref().and_then(|measured| measured.niche.as_ref().ok())
    }

    /// Why the point names no niche, when it does not.
    pub fn refusal(&self) -> Option<VoidRefusal> {
        self.measured.as_ref().and_then(|measured| measured.niche.as_ref().err().copied())
    }

    /// The product the measured cell belongs to, once there is one.
    pub fn product(&self) -> Option<Id> {
        self.niche().map(|niche| niche.product)
    }

    /// The cell's three sides in document units, for a surface that prints one number per axis.
    ///
    /// The cell's corners are whole micrometres, so the sides are exact `Length`s rather than a
    /// rounded millimetre reading; the value is the one the fill plan takes its part from.
    pub fn size(&self) -> Option<[Length; 3]> {
        let size = self.niche()?.size;
        Some([
            Length::from_millimetres_f64(size.x)?,
            Length::from_millimetres_f64(size.y)?,
            Length::from_millimetres_f64(size.z)?,
        ])
    }
}

/// What the pointer rests on in the open product: the point, and the face under it.
///
/// The face is what tells a fill whether the measured point can read the hand's place along the
/// part's free axis: a face that runs along that axis can, a face square to it cannot
/// ([`free_place`]). A ray that crosses the whole reserved space rests on no face of its own, so
/// the entry point comes alone.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerPoint {
    /// The measured point, a micrometre on the free side of the face, in world millimetres.
    pub point: DVec3,
    /// The face's outward normal, pointing back at the camera, when the point rests on one.
    pub face: Option<DVec3>,
}

/// The point in the open product the pointer points at, in world millimetres.
///
/// The atom's cursor rule, shared by both tools that measure a niche. The ray enters the **space the
/// product reserves** — its footprint's own ring extruded from the floor to the ceiling, taken in the
/// product's own frame because a turned piece is not its world bounds — and then meets the first
/// surface inside it. That face is what the maker sees under the cursor, so the void is measured a
/// micrometre in front of it; a ray that meets nothing but the envelope answers the entry point, which
/// is the whole reserved space. A pointer outside the product answers `None`.
///
/// The entry is the ring and not the box around it (ADR 0201, ADR 0204): a frame drawn with an arc
/// reserves nothing in the corner the arc cut away, so a ray that entered the box there would name a
/// point outside the void and no niche would answer — while the space behind that corner is right
/// there under the cursor.
pub fn pointer_point(
    project: &Project,
    product: Id,
    ray: Ray,
    sight: Sight<'_>,
    contact: Option<&dyn Contact>,
) -> Option<DVec3> {
    pointer_face(project, product, ray, sight, contact).map(|found| found.point)
}

/// The point in the open product the pointer points at, with the face it rests on.
///
/// [`pointer_point`] is this rule without the face, for the tools that only need the cell.
pub fn pointer_face(
    project: &Project,
    product: Id,
    ray: Ray,
    sight: Sight<'_>,
    contact: Option<&dyn Contact>,
) -> Option<PointerPoint> {
    let envelope = project.container(product)?.envelope()?;
    let placement = project.world_placement(product)?;
    let inverse = placement.inverse().ok()?;
    let origin = inverse.transform_point(ray.origin());
    let direction = inverse.transform_vector(ray.direction());
    let entry = reserved_entry(envelope, origin, direction)?;
    let local_entry = if entry <= 0.0 { origin } else { origin + direction * entry };
    let world_entry = placement.transform_point(local_entry);
    if !world_entry.is_finite() {
        return None;
    }
    let Some(step) = ray.direction().try_normalize() else {
        return Some(PointerPoint { point: world_entry, face: None });
    };
    let inside = world_entry + step * ENTRY_INSET_MM;
    let Some(probe) = project.probe(inside, ray.direction(), sight, contact) else {
        return Some(PointerPoint { point: inside, face: None });
    };
    // The envelope's own far face means the ray crossed the whole void: the entry point is the
    // measurement. Anything else is a surface the maker is looking at, and the free space is on
    // the side the ray came from.
    if probe.hit.kind == SurfaceKind::Envelope || probe.distance_mm <= ENTRY_INSET_MM {
        return Some(PointerPoint { point: inside, face: None });
    }
    let point = probe.face.point + probe.face.normal * FACE_INSET_MM;
    point.is_finite().then_some(PointerPoint { point, face: Some(probe.face.normal) })
}

/// How square a face may be to the free axis before it says nothing about the hand's place.
///
/// A face within about two and a half degrees of square to the axis names one coordinate for the
/// whole face, so nothing the pointer does along that face can move the part; a face that leans any
/// more reads the hand's place where it meets the maker's eye.
const SQUARE_FACE: f64 = 0.999;

/// The unit vector of one axis of a product frame.
const AXES: [DVec3; 3] = [DVec3::X, DVec3::Y, DVec3::Z];

/// What the pointer says about the axis a fill moves along.
///
/// Two numbers, because a face reads the axis in one of two ways. A face that runs along the axis —
/// a wall behind a shelf, the floor under a post — holds every place on the axis, so the point itself
/// is the hand's place and a tool reads it. A face square to the axis holds one: a pointer travels
/// over a board's top and the height it names never changes, so a part measured off that face sticks
/// at the board however far the hand moves (N23). `hand` is the other reading — the point on the
/// cell's own free axis nearest the view ray, the rule every drag in the viewport measures its travel
/// with — and `square` says which of the two the tool is to read. What the tool does with the
/// difference is its own business: the fill carries the part from the face the hand met.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FreePlace {
    /// The hand's place on the cell's free axis, in the product frame and millimetres.
    ///
    /// It is the reading a face square to the axis cannot give, so it is the one a tool has to act
    /// on there; where the face runs along the axis the point's own place is the exact answer.
    pub hand: f64,
    /// The place the point itself names, in the same frame and millimetres.
    pub point: f64,
    /// The face the point rests on, as its outward normal and its offset along that normal.
    pub face: Option<(DVec3, f64)>,
    /// Whether that face is square to the free axis, so the point cannot read the hand's place.
    pub square: bool,
}

/// What the pointer says about a cell's free axis, or `None` when it names no point at all.
///
/// The face under the point decides the reading ([`FreePlace`]); nothing here moves the part, and a
/// pointer outside the product answers nothing, which is the tool's business to hold on to.
pub fn free_place(
    project: &Project,
    product: Id,
    niche: &Niche,
    axis: usize,
    ray: Ray,
    sight: Sight<'_>,
    contact: Option<&dyn Contact>,
) -> Option<FreePlace> {
    let along = *AXES.get(axis)?;
    let found = pointer_face(project, product, ray, sight, contact)?;
    let placement = project.world_placement(product)?;
    let inverse = placement.inverse().ok()?;
    let local = inverse.transform_point(found.point);
    if !local.is_finite() {
        return None;
    }
    let face = found.face.map(|normal| (normal, normal.dot(found.point)));
    let square = found.face.is_some_and(|normal| inverse.transform_vector(normal)[axis].abs() >= SQUARE_FACE);
    let centre = niche.local_min + niche.size * 0.5;
    let hand = if centre.is_finite() {
        ray.closest_parameter_on_line(placement.transform_point(centre), placement.transform_vector(along))
            .map(|parameter| centre[axis] + parameter)
            .filter(|hand| hand.is_finite())
    } else {
        None
    };
    Some(FreePlace { hand: hand.unwrap_or(local[axis]), point: local[axis], face, square })
}

/// The distance at which a ray enters the space the product reserves, in the product's own frame.
///
/// The reserved space is the envelope's footprint ring carried from the floor to the ceiling, so the
/// entry is the first place on the ray that is inside the ring **and** between the two heights. Both
/// readings are exact: the ring answers its own crossings (an arc is solved, not sampled) and the
/// heights are one interval. The origin's own distance answers `0` when the ray starts inside, and a
/// ray that misses the ring or the height band answers `None`.
fn reserved_entry(envelope: &Envelope, origin: DVec3, direction: DVec3) -> Option<f64> {
    if !origin.is_finite() || !direction.is_finite() {
        return None;
    }
    let height = envelope.z().millimetres_f64();
    if !height.is_finite() || height <= 0.0 {
        return None;
    }
    let outline = envelope.footprint().outline(envelope.x(), envelope.y()).ok()?;
    let ring = Region::new(outline, Vec::new()).ok()?;
    let spans = ring.line_intervals(DVec2::new(origin.x, origin.y), DVec2::new(direction.x, direction.y));
    let heights = slab_interval(origin.z, direction.z, 0.0, height)?;
    let mut entry: Option<f64> = None;
    for (low, high) in spans {
        let from = low.max(heights.0).max(0.0);
        let to = high.min(heights.1);
        if to < from {
            continue;
        }
        entry = Some(entry.map_or(from, |seen: f64| seen.min(from)));
    }
    entry
}

/// The parameter interval in which the line `origin + t * direction` is between `low` and `high`.
///
/// One axis of a slab, the answer a box entry is built from: a step along the axis that misses the
/// band, and an origin outside it, both answer `None`.
fn slab_interval(origin: f64, direction: f64, low: f64, high: f64) -> Option<(f64, f64)> {
    if !origin.is_finite() || !direction.is_finite() {
        return None;
    }
    if direction.abs() <= 1.0e-9 {
        return (low..=high).contains(&origin).then_some((f64::NEG_INFINITY, f64::INFINITY));
    }
    let first = (low - origin) / direction;
    let second = (high - origin) / direction;
    Some((first.min(second), first.max(second)))
}

/// The distance at which a ray enters a box, or the origin's own distance when it starts inside.
///
/// A ray that misses the box answers `None`.
#[allow(dead_code)]
fn box_entry(origin: DVec3, direction: DVec3, min: DVec3, max: DVec3) -> Option<f64> {
    if !origin.is_finite() || !direction.is_finite() {
        return None;
    }
    let mut enter = f64::NEG_INFINITY;
    let mut exit = f64::INFINITY;
    for axis in 0..3 {
        let step = direction[axis];
        let low = min[axis];
        let high = max[axis];
        if step.abs() <= 1.0e-9 {
            if origin[axis] < low || origin[axis] > high {
                return None;
            }
            continue;
        }
        let first = (low - origin[axis]) / step;
        let second = (high - origin[axis]) / step;
        enter = enter.max(first.min(second));
        exit = exit.min(first.max(second));
    }
    if exit < enter || exit < 0.0 {
        return None;
    }
    Some(enter.max(0.0))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use std::f64::consts::FRAC_PI_2;

    use geom::DQuat;
    use model::{ClassCode, Container, Envelope, Placement};
    use units::Length;

    use super::*;

    fn millimetres(value: i64) -> Length {
        Length::from_millimetres(value).expect("whole millimetres fit")
    }

    fn clean_project() -> Project {
        let root = Container::new(Id::new(1).expect("id"), "Root", Placement::IDENTITY, Vec::new());
        Project::new(root).expect("an empty project")
    }

    /// An empty product of `600 x 560 x 720` millimetres at the origin.
    fn piece() -> (Project, Id) {
        let mut project = clean_project();
        let root = project.root().id();
        let envelope = Envelope::new(millimetres(600), millimetres(560), millimetres(720)).expect("a box");
        let id = project
            .add_product(root, "Cabinet", ClassCode::GENERAL_PRODUCT, envelope, Placement::IDENTITY)
            .expect("a product");
        (project, id)
    }

    #[test]
    fn one_point_names_the_cell_and_a_still_pointer_measures_nothing() {
        let (project, product) = piece();
        let mut atom = Void::new();
        assert_eq!(atom.niche(), None, "nothing is measured before the pointer");
        assert!(atom.measure(&project, DVec3::new(300.0, 280.0, 360.0), Sight::none(), None));
        let niche = atom.niche().expect("the point is inside the void").clone();
        assert_eq!(niche.product, product);
        assert_eq!(atom.size(), Some([millimetres(600), millimetres(560), millimetres(720)]));
        // A hand that holds still gains no new answer, and the value is the same one.
        assert!(
            !atom.measure(&project, DVec3::new(300.2, 280.0, 360.0), Sight::none(), None),
            "a step smaller than half a millimetre"
        );
        assert_eq!(atom.niche(), Some(&niche));
        // A pointer that moved re-measures.
        assert!(atom.measure(&project, DVec3::new(300.0, 280.0, 361.0), Sight::none(), None));
    }

    #[test]
    fn the_atom_measures_through_a_part_the_session_switched_off() {
        let (mut project, product) = piece();
        let nominal = model::PanelNominalSize::new(millimetres(18), millimetres(560)).expect("a sheet size");
        let at = Placement::IDENTITY
            .with_translation(millimetres(291), millimetres(0), millimetres(0))
            .expect("a placement");
        let sheet = project.add_panel(product, "Sheet", nominal, None, millimetres(720), at).expect("a sheet");
        let point = DVec3::new(100.0, 280.0, 360.0);

        let mut atom = Void::new();
        atom.measure(&project, point, Sight::none(), None);
        let visible = atom.niche().expect("the void in front of the sheet").clone();
        assert_eq!(visible.size, DVec3::new(291.0, 560.0, 720.0), "the sheet is a wall of the void");

        let hidden: std::collections::HashSet<Id> = [sheet].into_iter().collect();
        atom.clear();
        atom.measure(&project, point, Sight::of(&hidden), None);
        let through = atom.niche().expect("the hidden sheet is no wall").clone();
        assert_eq!(through.size, DVec3::new(600.0, 560.0, 720.0), "the cell spans the case it cannot see");
    }

    #[test]
    fn the_pointer_enters_the_space_the_frame_reserves_and_not_the_box_around_it() {
        // A frame drawn as an L reserves nothing in the corner of its box, so a ray that entered the
        // box there would name a point outside the void — no niche, no part — while the space beside
        // that corner is right under the cursor (ADR 0204, N35).
        let mut project = clean_project();
        let ring = model::Path::polyline(
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
        let ray = Ray::new(DVec3::new(900.0, 400.0, 300.0), DVec3::NEG_X).expect("a ray across the notch");
        let point = pointer_point(&project, product, ray, Sight::none(), None).expect("the ray enters the void");
        assert!((point.x - 300.0).abs() < 0.01, "the arm's own side is the entry, was {point:?}");
        assert!((point.y - 400.0).abs() < 0.01);
        assert!((point.z - 300.0).abs() < 0.01);
        // The box's own entry is inside the corner the frame never gave away, and it names no void.
        let boxed = DVec3::new(600.0, 400.0, 300.0);
        assert_eq!(project.niche(boxed, Sight::none(), None), Err(NicheRefusal::Outside));
    }

    #[test]
    fn a_frame_whose_ring_bulges_inwards_says_so_of_itself() {
        // A frame drawn as a ring builds the convex piece around the pointer (N34, ADR 0203), but a ring
        // whose own arc bulges into its shape is still no one cell, and the atom carries that reason
        // through so every surface prints the same sentence.
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
        let contour = model::Contour::new(
            ring.segments()
                .iter()
                .map(|segment| model::ContourSegment::from_geom(*segment).expect("a document segment"))
                .collect(),
        )
        .expect("a document ring");
        let envelope = Envelope::with_ring(&contour, millimetres(720)).expect("a bowed frame");
        let mut project = clean_project();
        let root = project.root().id();
        project
            .add_product(root, "Bowed", ClassCode::GENERAL_PRODUCT, envelope, Placement::IDENTITY)
            .expect("a bowed product");
        let mut atom = Void::new();
        atom.measure(&project, DVec3::new(300.0, 200.0, 300.0), Sight::none(), None);
        let refusal = atom.refusal().expect("a frame that reserves no cell names no void");
        assert_eq!(refusal, VoidRefusal::Footprint);
        assert_eq!(refusal.words(), Message::VoidFootprint, "the sentence every surface prints");
        assert_eq!(atom.niche(), None);
    }

    #[test]
    fn a_point_outside_the_product_is_a_refusal_not_an_empty_value() {
        let (project, _) = piece();
        let mut atom = Void::new();
        assert!(atom.measure(&project, DVec3::new(-500.0, 0.0, 0.0), Sight::none(), None));
        assert_eq!(atom.niche(), None);
        let refusal = atom.refusal().expect("the pointer names no void");
        assert_eq!(refusal, VoidRefusal::Outside);
        assert_eq!(refusal.words(), Message::VoidOutside, "the sentence every surface prints");
        assert_eq!(atom.size(), None);
        assert!(
            atom.measure(&project, DVec3::new(300.0, 280.0, 360.0), Sight::none(), None),
            "the pointer can come back"
        );
        assert_eq!(atom.refusal(), None);
    }

    #[test]
    fn clearing_forgets_the_point_and_the_cell() {
        let (project, _) = piece();
        let mut atom = Void::new();
        atom.measure(&project, DVec3::new(300.0, 280.0, 360.0), Sight::none(), None);
        atom.clear();
        assert_eq!(atom, Void::new(), "a cleared atom is the one that was armed");
        assert_eq!(atom.point(), None);
        assert_eq!(atom.size(), None);
    }

    #[test]
    fn the_cursor_enters_the_envelope_at_its_near_face() {
        let (project, product) = piece();
        let ray = Ray::new(DVec3::new(300.0, -1000.0, 300.0), DVec3::Y).expect("a ray");
        let point = pointer_point(&project, product, ray, Sight::none(), None).expect("the ray enters the piece");
        assert!((point.x - 300.0).abs() < 1e-9 && (point.z - 300.0).abs() < 1e-9, "was {point:?}");
        assert!((0.0..0.01).contains(&point.y), "a micrometre inside the near face: {point:?}");
        let inside = Ray::new(DVec3::new(300.0, 280.0, 300.0), DVec3::Y).expect("a ray");
        let point =
            pointer_point(&project, product, inside, Sight::none(), None).expect("a camera inside starts where it is");
        assert!((point.y - 280.0).abs() < 0.01, "was {point:?}");
        let missing = Ray::new(DVec3::new(-50.0, -1000.0, 300.0), -DVec3::Y).expect("a ray");
        assert_eq!(pointer_point(&project, product, missing, Sight::none(), None), None);
    }

    /// A board standing as the case's back wall, its inner face through `y = through` at `z = 0`
    /// and leaned by `degrees` about `+X`: a positive lean brings the face forward with height.
    fn leaned_wall(project: &mut Project, product: Id, through: f64, degrees: f64) {
        let wall = DQuat::from_rotation_z(-FRAC_PI_2) * DQuat::from_rotation_y(-FRAC_PI_2);
        let rotation = DQuat::from_rotation_x(degrees.to_radians()) * wall;
        let placement = Placement::new(DVec3::new(0.0, through, 0.0), rotation).expect("a wall placement");
        let nominal = model::PanelNominalSize::new(millimetres(1600), millimetres(600)).expect("a wall size");
        project.add_panel(product, "Wall", nominal, None, millimetres(18), placement).expect("add a wall");
    }

    #[test]
    fn the_pointer_measures_from_the_face_it_points_at() {
        let (mut project, product) = piece();
        leaned_wall(&mut project, product, 542.0, 0.0);
        let ray = Ray::new(DVec3::new(300.0, -1000.0, 300.0), DVec3::Y).expect("a ray");
        let point = pointer_point(&project, product, ray, Sight::none(), None).expect("the face is in the piece");
        assert!((point.y - 541.999).abs() < 0.01, "the point rests in front of the wall: {point:?}");
        let mut atom = Void::new();
        atom.measure(&project, point, Sight::none(), None);
        let niche = atom.niche().expect("the void ends at the wall");
        assert!(niche.local_max().y <= 542.0, "the niche ends at the wall: {niche:?}");
    }

    /// A board lying across the piece: its top face at `z = 368` names one height for the whole
    /// face, and its own slab is a cell no part fits.
    fn shelf(project: &mut Project, product: Id) {
        let nominal = model::PanelNominalSize::new(millimetres(600), millimetres(560)).expect("a shelf size");
        let placement = model::Placement::IDENTITY
            .with_translation(Length::ZERO, Length::ZERO, millimetres(350))
            .expect("a shelf placement");
        project.add_panel(product, "Shelf", nominal, None, millimetres(18), placement).expect("add a shelf");
    }

    /// A board standing across the piece's back: its inner face through `y = 560`, its normal along
    /// `-Y`, so it runs along the height and reads the hand exactly.
    fn back_wall(project: &mut Project, product: Id) {
        let rotation = DQuat::from_rotation_z(-FRAC_PI_2) * DQuat::from_rotation_y(-FRAC_PI_2);
        let placement = model::Placement::new(DVec3::new(0.0, 560.0, 0.0), rotation).expect("a wall placement");
        let nominal = model::PanelNominalSize::new(millimetres(1600), millimetres(600)).expect("a wall size");
        project.add_panel(product, "Wall", nominal, None, millimetres(18), placement).expect("add a wall");
    }

    #[test]
    fn a_face_square_to_the_axis_cannot_read_the_hand_but_a_face_along_it_can() {
        let (mut project, product) = piece();
        shelf(&mut project, product);
        back_wall(&mut project, product);
        let ray = |target: DVec3| {
            let from = DVec3::new(0.0, -800.0, target.z + 532.0);
            Ray::new(from, target - from).expect("a ray")
        };
        // The shelf's top face: the whole face names 368, so the point cannot say where the hand is
        // along the height, and the hand's own place on the cell's axis is a different number.
        let top = DVec3::new(200.0, 400.0, 368.0);
        let niche = project.niche(top + DVec3::Z * 1.0e-3, Sight::none(), None).expect("the cell above the shelf");
        let aim =
            free_place(&project, product, &niche, 2, ray(top), Sight::none(), None).expect("the pointer names a point");
        assert!(aim.square, "a top face is square to the height: {aim:?}");
        assert!((aim.point - 368.0).abs() <= 0.01, "the point rests on the face: {aim:?}");
        assert!((aim.hand - aim.point).abs() > 20.0, "the hand is elsewhere on the axis: {aim:?}");

        // The wall behind runs along the height, so the point itself is the height the pointer is on
        // and a tool reads that instead of the hand's projection.
        let across = Ray::new(DVec3::new(300.0, -800.0, 400.0), DVec3::Y).expect("a ray");
        let aim =
            free_place(&project, product, &niche, 2, across, Sight::none(), None).expect("the pointer names a point");
        assert!(!aim.square, "a wall is not square to the height: {aim:?}");
        assert!((aim.point - 400.0).abs() <= 0.01, "the height the pointer is on: {aim:?}");

        // A ray that crosses the whole reserved space rests on no face of its own, so the tool reads
        // the entry point: an empty piece answers nothing but its envelope.
        let (project, product) = piece();
        let through = Ray::new(DVec3::new(300.0, -1000.0, 300.0), DVec3::Y).expect("a ray");
        let niche =
            project.niche(DVec3::new(300.0, 1.0, 300.0), Sight::none(), None).expect("the whole reserved space");
        let aim =
            free_place(&project, product, &niche, 2, through, Sight::none(), None).expect("the ray crosses the void");
        assert!(aim.face.is_none() && !aim.square, "the entry point names no face: {aim:?}");
        assert!((aim.point - 300.0).abs() <= 0.01, "the entry point's own height: {aim:?}");

        // A ray that misses the piece names nothing at all, and neither does a fourth axis.
        let away = Ray::new(DVec3::new(-500.0, -800.0, 400.0), DVec3::X).expect("a ray");
        assert_eq!(free_place(&project, product, &niche, 2, away, Sight::none(), None), None);
        assert_eq!(free_place(&project, product, &niche, 7, through, Sight::none(), None), None, "no fourth axis");
    }

    #[test]
    fn a_turned_product_is_entered_through_its_own_box() {
        let (mut project, product) = piece();
        let turn = Placement::IDENTITY
            .with_rotation_euler(model::EulerAngles {
                z: units::Degrees::from_millidegrees(90_000),
                ..model::EulerAngles::default()
            })
            .expect("a turn");
        project.set_node_placement(product, turn).expect("turn the piece");
        let world = |x: f64, y: f64, z: f64| turn.transform_point(DVec3::new(x, y, z));
        // The piece now spans x in [-560, 0] and y in [0, 600], so a ray at x = -500 points into it.
        let ray = Ray::new(world(300.0, -500.0, 300.0), turn.transform_vector(DVec3::Y)).expect("a ray");
        let point = pointer_point(&project, product, ray, Sight::none(), None).expect("the turned piece is entered");
        let local = turn.inverse().expect("an inverse").transform_point(point);
        assert!((0.0..0.01).contains(&local.y), "a micrometre inside the piece's own face: {local:?}");
        // A ray aimed at the world bounds but outside the turned box has no point at all.
        let outside = Ray::new(DVec3::new(10.0, -1000.0, 300.0), DVec3::Y).expect("a ray");
        assert_eq!(pointer_point(&project, product, outside, Sight::none(), None), None);
    }
}
