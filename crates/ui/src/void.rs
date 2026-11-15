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
//! has no name. Nothing here writes: there is no document field, no revision and no undo entry.
//!
//! Changes when: a second niche shape needs the corners rather than the box, or the cursor rule for
//! the point inside the product changes.

use furnicraft_geom::DVec3;
use furnicraft_model::{Id, Niche, NicheRefusal, Project, SurfaceKind};
use furnicraft_scene::Ray;
use furnicraft_text::Message;
use furnicraft_units::Length;

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
}

impl VoidRefusal {
    /// The model's refusal in the atom's own vocabulary.
    pub const fn of(refusal: NicheRefusal) -> Self {
        match refusal {
            NicheRefusal::Outside => VoidRefusal::Outside,
            NicheRefusal::Unrepresentable => VoidRefusal::Unrepresentable,
        }
    }

    /// The sentence a surface prints, in the language it draws.
    pub const fn words(self) -> Message {
        match self {
            VoidRefusal::Outside => Message::VoidOutside,
            VoidRefusal::Unrepresentable => Message::VoidUnrepresentable,
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
    pub fn measure(&mut self, project: &Project, point: DVec3) -> bool {
        if let Some(measured) = &self.measured
            && (measured.point - point).length() < MEASURE_STEP_MM
        {
            return false;
        }
        let niche = project.niche(point).map_err(VoidRefusal::of);
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
        Some([length_from_millimetres(size.x)?, length_from_millimetres(size.y)?, length_from_millimetres(size.z)?])
    }
}

/// The point in the open product the pointer points at, in world millimetres.
///
/// The atom's cursor rule, shared by both tools that measure a niche. The ray enters the product's
/// authored envelope — taken in the product's own frame, because a turned piece is not its world
/// bounds — and then meets the first surface inside it. That face is what the maker sees under the
/// cursor, so the void is measured a micrometre in front of it; a ray that meets nothing but the
/// envelope answers the entry point, which is the whole reserved space. A pointer outside the
/// product answers `None`.
pub fn pointer_point(project: &Project, product: Id, ray: Ray) -> Option<DVec3> {
    let envelope = project.container(product)?.envelope()?;
    let size =
        DVec3::new(envelope.x().millimetres_f64(), envelope.y().millimetres_f64(), envelope.z().millimetres_f64());
    if !size.is_finite() || size.min_element() <= 0.0 {
        return None;
    }
    let placement = project.world_placement(product)?;
    let inverse = placement.inverse().ok()?;
    let origin = inverse.transform_point(ray.origin());
    let direction = inverse.transform_vector(ray.direction());
    let entry = box_entry(origin, direction, DVec3::ZERO, size)?;
    let local_entry = if entry <= 0.0 { origin } else { origin + direction * entry };
    let world_entry = placement.transform_point(local_entry);
    if !world_entry.is_finite() {
        return None;
    }
    let Some(step) = ray.direction().try_normalize() else {
        return Some(world_entry);
    };
    let inside = world_entry + step * ENTRY_INSET_MM;
    let Some(probe) = project.probe(inside, ray.direction()) else {
        return Some(inside);
    };
    // The envelope's own far face means the ray crossed the whole void: the entry point is the
    // measurement. Anything else is a surface the maker is looking at, and the free space is on
    // the side the ray came from.
    if probe.hit.kind == SurfaceKind::Envelope || probe.distance_mm <= ENTRY_INSET_MM {
        return Some(inside);
    }
    let point = probe.face.point + probe.face.normal * FACE_INSET_MM;
    point.is_finite().then_some(point)
}

/// The distance at which a ray enters a box, or the origin's own distance when it starts inside.
///
/// This is the cursor's resting place in the void: the face of the reserved space the maker is
/// pointing at. A ray that misses the box answers `None`.
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

/// A cell side in millimetres as the document's own `Length`, or `None` when it does not fit.
fn length_from_millimetres(value: f64) -> Option<Length> {
    if !value.is_finite() {
        return None;
    }
    let micrometres = (value * 1_000.0).round();
    if micrometres < i64::MIN as f64 || micrometres > i64::MAX as f64 {
        return None;
    }
    Some(Length::from_micrometres(micrometres as i64))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use std::f64::consts::FRAC_PI_2;

    use furnicraft_geom::DQuat;
    use furnicraft_model::{ClassCode, Container, Envelope, Placement};
    use furnicraft_units::Length;

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
        assert!(atom.measure(&project, DVec3::new(300.0, 280.0, 360.0)));
        let niche = atom.niche().expect("the point is inside the void").clone();
        assert_eq!(niche.product, product);
        assert_eq!(atom.size(), Some([millimetres(600), millimetres(560), millimetres(720)]));
        // A hand that holds still gains no new answer, and the value is the same one.
        assert!(!atom.measure(&project, DVec3::new(300.2, 280.0, 360.0)), "a step smaller than half a millimetre");
        assert_eq!(atom.niche(), Some(&niche));
        // A pointer that moved re-measures.
        assert!(atom.measure(&project, DVec3::new(300.0, 280.0, 361.0)));
    }

    #[test]
    fn a_point_outside_the_product_is_a_refusal_not_an_empty_value() {
        let (project, _) = piece();
        let mut atom = Void::new();
        assert!(atom.measure(&project, DVec3::new(-500.0, 0.0, 0.0)));
        assert_eq!(atom.niche(), None);
        let refusal = atom.refusal().expect("the pointer names no void");
        assert_eq!(refusal, VoidRefusal::Outside);
        assert_eq!(refusal.words(), Message::VoidOutside, "the sentence every surface prints");
        assert_eq!(atom.size(), None);
        assert!(atom.measure(&project, DVec3::new(300.0, 280.0, 360.0)), "the pointer can come back");
        assert_eq!(atom.refusal(), None);
    }

    #[test]
    fn clearing_forgets_the_point_and_the_cell() {
        let (project, _) = piece();
        let mut atom = Void::new();
        atom.measure(&project, DVec3::new(300.0, 280.0, 360.0));
        atom.clear();
        assert_eq!(atom, Void::new(), "a cleared atom is the one that was armed");
        assert_eq!(atom.point(), None);
        assert_eq!(atom.size(), None);
    }

    #[test]
    fn the_cursor_enters_the_envelope_at_its_near_face() {
        let (project, product) = piece();
        let ray = Ray::new(DVec3::new(300.0, -1000.0, 300.0), DVec3::Y).expect("a ray");
        let point = pointer_point(&project, product, ray).expect("the ray enters the piece");
        assert!((point.x - 300.0).abs() < 1e-9 && (point.z - 300.0).abs() < 1e-9, "was {point:?}");
        assert!((0.0..0.01).contains(&point.y), "a micrometre inside the near face: {point:?}");
        let inside = Ray::new(DVec3::new(300.0, 280.0, 300.0), DVec3::Y).expect("a ray");
        let point = pointer_point(&project, product, inside).expect("a camera inside starts where it is");
        assert!((point.y - 280.0).abs() < 0.01, "was {point:?}");
        let missing = Ray::new(DVec3::new(-50.0, -1000.0, 300.0), -DVec3::Y).expect("a ray");
        assert_eq!(pointer_point(&project, product, missing), None);
    }

    /// A board standing as the case's back wall, its inner face through `y = through` at `z = 0`
    /// and leaned by `degrees` about `+X`: a positive lean brings the face forward with height.
    fn leaned_wall(project: &mut Project, product: Id, through: f64, degrees: f64) {
        let wall = DQuat::from_rotation_z(-FRAC_PI_2) * DQuat::from_rotation_y(-FRAC_PI_2);
        let rotation = DQuat::from_rotation_x(degrees.to_radians()) * wall;
        let placement = Placement::new(DVec3::new(0.0, through, 0.0), rotation).expect("a wall placement");
        let nominal =
            furnicraft_model::PanelNominalSize::new(millimetres(1600), millimetres(600)).expect("a wall size");
        project.add_panel(product, "Wall", nominal, None, millimetres(18), placement).expect("add a wall");
    }

    #[test]
    fn the_pointer_measures_from_the_face_it_points_at() {
        let (mut project, product) = piece();
        leaned_wall(&mut project, product, 542.0, 0.0);
        let ray = Ray::new(DVec3::new(300.0, -1000.0, 300.0), DVec3::Y).expect("a ray");
        let point = pointer_point(&project, product, ray).expect("the face is in the piece");
        assert!((point.y - 541.999).abs() < 0.01, "the point rests in front of the wall: {point:?}");
        let mut atom = Void::new();
        atom.measure(&project, point);
        let niche = atom.niche().expect("the void ends at the wall");
        assert!(niche.local_max().y <= 542.0, "the niche ends at the wall: {niche:?}");
    }

    #[test]
    fn a_turned_product_is_entered_through_its_own_box() {
        let (mut project, product) = piece();
        let turn = Placement::IDENTITY
            .with_rotation_euler(furnicraft_model::EulerAngles {
                z: furnicraft_units::Degrees::from_millidegrees(90_000),
                ..furnicraft_model::EulerAngles::default()
            })
            .expect("a turn");
        project.set_node_placement(product, turn).expect("turn the piece");
        let world = |x: f64, y: f64, z: f64| turn.transform_point(DVec3::new(x, y, z));
        // The piece now spans x in [-560, 0] and y in [0, 600], so a ray at x = -500 points into it.
        let ray = Ray::new(world(300.0, -500.0, 300.0), turn.transform_vector(DVec3::Y)).expect("a ray");
        let point = pointer_point(&project, product, ray).expect("the turned piece is entered");
        let local = turn.inverse().expect("an inverse").transform_point(point);
        assert!((0.0..0.01).contains(&local.y), "a micrometre inside the piece's own face: {local:?}");
        // A ray aimed at the world bounds but outside the turned box has no point at all.
        let outside = Ray::new(DVec3::new(10.0, -1000.0, 300.0), DVec3::Y).expect("a ray");
        assert_eq!(pointer_point(&project, product, outside), None);
    }
}
