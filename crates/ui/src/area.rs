//! The area of a drawing under the pointer: the face a pull would take (G4).
//!
//! A drawing of several rings encloses material, and its own boundaries divide that material into
//! the areas a maker points at. None of them is stored — [`model::Contours::areas`] derives them —
//! so an area has no id: this atom keeps the point the pointer resolved to in the drawing's own plane
//! and the face that contains it, and a command names the same face by the same point.
//!
//! The derivation is cached by the drawing and the document revision it was taken at, so a still
//! hand costs one comparison and a moving one costs one containment test per face. An edited drawing
//! derives its areas again in the same frame the new outline is drawn, because nothing about an area
//! outlives the rings.
//!
//! Nothing here writes: the highlight is paint, the click is a command, and the atom never reaches
//! the document.
//!
//! Changes when: a second surface reads an area, or the faces stop being derived by the model.

use geom::{DVec2, Region};
use model::{Id, Project};
use text::Message;
use units::Length;

/// What the pointer over a drawing names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AreaRefusal {
    /// The point falls outside every area of the drawing.
    Outside,
    /// The area under the pointer has a hole, and a body carries one ring yet.
    Hole,
}

impl AreaRefusal {
    /// The sentence a surface prints, in the language it draws.
    pub const fn words(self) -> Message {
        match self {
            Self::Outside => Message::AreaOutside,
            Self::Hole => Message::AreaHole,
        }
    }
}

/// One measurement: the point in the drawing's plane, and what the pointer found there.
#[derive(Clone, Debug, PartialEq)]
struct Measured {
    /// The drawing the point was read in.
    object: Id,
    /// The document revision the answer was derived at.
    ///
    /// A ring edit moves the face without moving the pointer, so a new revision measures the same
    /// point again instead of keeping a highlight that no longer stands on the drawing.
    revision: u64,
    /// The point, in the drawing's own plane and in micrometres.
    point: [Length; 2],
    /// The face under the point, or why there is none to pull.
    area: Result<Region, AreaRefusal>,
}

/// The live area under the pointer, with the drawing's faces cached beside it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Area {
    /// The drawing whose faces were derived, with the revision they were derived at.
    cached: Option<(Id, u64, Vec<Region>)>,
    measured: Option<Measured>,
}

impl Area {
    /// An atom that has measured nothing.
    pub const fn new() -> Self {
        Self { cached: None, measured: None }
    }

    /// Reads the area under `point` in `object`'s own plane, and says whether the value changed.
    ///
    /// A point that falls in a face with a hole still answers that face: the maker sees what the
    /// pointer rests on, and the sentence says why the pull is refused. A drawing that is not a
    /// contour set, or whose areas the geometry layer refuses, leaves the atom empty rather than
    /// guessing.
    pub fn measure(&mut self, project: &Project, object: Id, point: [Length; 2]) -> bool {
        let revision = project.revision().get();
        if let Some(measured) = &self.measured
            && measured.object == object
            && measured.point == point
            && measured.revision == revision
        {
            return false;
        }
        self.ensure_areas(project, object);
        let local = DVec2::new(point[0].millimetres_f64(), point[1].millimetres_f64());
        let area = match self.cached.as_ref().filter(|(id, _, _)| *id == object) {
            None => return self.clear(),
            Some((_, _, areas)) => match areas.iter().find(|area| area.contains(local)) {
                Some(face) if !face.holes().is_empty() => Err(AreaRefusal::Hole),
                Some(face) => Ok(face.clone()),
                None => Err(AreaRefusal::Outside),
            },
        };
        self.measured = Some(Measured { object, revision, point, area });
        true
    }

    /// Derives the drawing's faces once per revision, so a moving pointer reads a cached set.
    fn ensure_areas(&mut self, project: &Project, object: Id) {
        let revision = project.revision().get();
        if let Some((id, cached, _)) = &self.cached
            && *id == object
            && *cached == revision
        {
            return;
        }
        self.cached = project
            .object(object)
            .and_then(|node| node.geometry().as_contours())
            .and_then(|set| set.areas().ok())
            .map(|areas| (object, revision, areas));
    }

    /// Drops the measurement, and says whether there was a value to drop.
    ///
    /// The cached faces go with it: a value that is no longer shown must not come back as a hit on a
    /// drawing the pointer has left.
    pub fn clear(&mut self) -> bool {
        self.cached = None;
        self.measured.take().is_some()
    }

    /// The drawing the point was read in.
    pub fn object(&self) -> Option<Id> {
        self.measured.as_ref().map(|measured| measured.object)
    }

    /// The point the value was measured at, in the drawing's own plane.
    pub fn point(&self) -> Option<[Length; 2]> {
        self.measured.as_ref().map(|measured| measured.point)
    }

    /// The face under the point, once there is one to pull.
    pub fn region(&self) -> Option<&Region> {
        self.measured.as_ref().and_then(|measured| measured.area.as_ref().ok())
    }

    /// Why the point names no pullable area, when it does not.
    pub fn refusal(&self) -> Option<AreaRefusal> {
        self.measured.as_ref().and_then(|measured| measured.area.as_ref().err().copied())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use model::{Contours, Geometry, Path, Placement, Project};
    use testkit::{millimetres, sample_project};

    use super::*;

    /// A closed rectangle ring from `(x0, y0)` to `(x1, y1)` in millimetres.
    fn ring(x0: i64, y0: i64, x1: i64, y1: i64) -> Path {
        Path::rectangle([millimetres(x0), millimetres(y0)], [millimetres(x1), millimetres(y1)]).expect("a ring")
    }

    /// A project whose root holds one drawing of the given rings, and its id.
    fn drawing(rings: Vec<Path>) -> (Project, Id) {
        let mut project = sample_project();
        let root = project.root().id();
        let set = Contours::new(rings).expect("a contour set");
        let id = project.add_object(root, "Union", Placement::IDENTITY, Geometry::Contours(set)).expect("a set object");
        (project, id)
    }

    fn at(x: i64, y: i64) -> [Length; 2] {
        [millimetres(x), millimetres(y)]
    }

    #[test]
    fn the_pointer_names_the_gap_between_two_nested_rings_and_nothing_else() {
        let (project, set) = drawing(vec![ring(0, 0, 100, 50), ring(20, 10, 60, 30)]);
        let mut area = Area::new();

        assert!(area.measure(&project, set, at(5, 5)), "the frame is measured");
        assert_eq!(area.refusal(), Some(AreaRefusal::Hole), "the frame carries the opening as a hole");
        assert!(area.region().is_none(), "a hole is not a pullable area yet");

        assert!(area.measure(&project, set, at(40, 20)), "the opening is another area");
        assert_eq!(area.refusal(), None);
        assert!((area.region().expect("the opening").area() - 800.0).abs() < 1.0e-6);

        assert!(area.measure(&project, set, at(300, 300)), "a point outside every ring");
        assert_eq!(area.refusal(), Some(AreaRefusal::Outside));
        assert!(area.region().is_none());
    }

    #[test]
    fn a_still_pointer_measures_nothing_again() {
        let (project, set) = drawing(vec![ring(0, 0, 100, 50)]);
        let mut area = Area::new();
        assert!(area.measure(&project, set, at(50, 25)));
        assert!(!area.measure(&project, set, at(50, 25)), "the same point is the same answer");
        assert!(area.measure(&project, set, at(51, 25)), "a moved pointer measures again");
    }

    #[test]
    fn a_second_drawing_answers_its_own_areas() {
        // The cache holds one drawing, so a pointer that moves to another one never reads the first
        // drawing's faces: the areas are in the drawing's own plane and belong to its rings alone.
        let (mut project, first) = drawing(vec![ring(0, 0, 100, 50)]);
        let root = project.root().id();
        let second = project
            .add_object(
                root,
                "Nested",
                Placement::IDENTITY,
                Geometry::Contours(Contours::new(vec![ring(0, 0, 100, 50), ring(20, 10, 60, 30)]).expect("two rings")),
            )
            .expect("a second drawing");
        let mut area = Area::new();

        assert!(area.measure(&project, first, at(5, 5)), "the plain drawing is one area");
        assert_eq!(area.refusal(), None);
        assert!(area.measure(&project, second, at(5, 5)), "the nested drawing answers the frame");
        assert_eq!(area.refusal(), Some(AreaRefusal::Hole));
        assert_eq!(area.object(), Some(second));

        // A moved drawing keeps its faces: they are drawn in its own plane, so the pointer's point is
        // the only thing a placement could change.
        project
            .set_node_placement(
                second,
                Placement::IDENTITY.with_translation(millimetres(500), Length::ZERO, Length::ZERO).expect("a move"),
            )
            .expect("the drawing moves");
        assert!(area.measure(&project, second, at(5, 5)), "a new revision is measured again");
        assert_eq!(area.refusal(), Some(AreaRefusal::Hole), "the areas did not move with the node");
    }

    #[test]
    fn a_node_that_is_not_a_drawing_measures_nothing() {
        let mut project = sample_project();
        let root = project.root().id();
        let path = project
            .add_object(root, "Ring", Placement::IDENTITY, Geometry::Path(ring(0, 0, 10, 10)))
            .expect("a path object");
        let mut area = Area::new();

        area.measure(&project, path, at(5, 5));
        assert_eq!(area.object(), None, "a single ring is not a drawing of areas");
        assert!(area.region().is_none());
        assert!(!area.clear(), "there was nothing to drop");
    }
}
