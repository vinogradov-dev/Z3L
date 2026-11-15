//! The numbers of the point a drawing tool is placing, and the point they place.
//!
//! A sketch point is not one number: a line has a length and an angle, a rectangle a width and a
//! height on their own sides, a circle a radius. This module answers one question per frame — *what
//! would the next click place, and which plates name it?* — for every drawing tool, so the shell
//! only maps work-plane points to the screen and hands the numbers to
//! [`FieldSet`](crate::FieldSet).
//!
//! The pointer drives every number, and a number the person typed replaces the one the pointer
//! gives: [`PointState::live`] reads what the pointer alone produces, [`PointState::placed`] reads
//! the same with every lock of a [`FieldSet`] applied, so a typed length survives a snap and a
//! typed `0` gives the number back to the pointer.
//!
//! **The smaller angle.** A segment's angle is read from the nearer end of the X axis: the right
//! half-plane reads from +X, the left half-plane from −X, and the plate shows at most half a turn.
//! The sweep is signed — positive counter-clockwise — so the guide arc always takes the short way.
//! A typed number is a direction from +X, so a value above 180 flips the reference to −X, which is
//! the same rule read the other way round (ADR 0128).
//!
//! It writes nothing: a draft is view state and the commit is one `object.path.create` elsewhere.
//!
//! Changes when: a drawing tool's numbers, the smaller-angle rule or a stage's guides change.

use core::f64::consts::PI;

use furnicraft_geom::DVec2;
use furnicraft_model::MAX_CONTOUR_COORDINATE;
use furnicraft_units::Length;

use crate::draw::{DrawDraft, DrawTool};
use crate::point_fields::{FieldParam, FieldRow, FieldSet};

/// The largest coordinate a drawing point may name, in millimetres.
const MAX_COORDINATE_MM: f64 = MAX_CONTOUR_COORDINATE.micrometres() as f64 / 1_000.0;

/// Which end of the X axis an angle is read from: the nearer one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XEnd {
    /// The positive end, `+X`.
    Plus,
    /// The negative end, `−X`.
    Minus,
}

impl XEnd {
    /// The direction of the end, in radians from +X, counter-clockwise.
    pub fn radians(self) -> f64 {
        if self == Self::Plus { 0.0 } else { PI }
    }
}

/// An angle as a sketch shows it: the nearer X end and the short sweep from it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SketchAngle {
    /// The end of the X axis the sweep is read from.
    pub end: XEnd,
    /// The sweep from that end to the segment, in degrees; positive is counter-clockwise.
    pub sweep: f64,
}

impl SketchAngle {
    /// The number a plate shows: the smaller angle, in degrees, never negative.
    pub fn shown(self) -> f64 {
        self.sweep.abs()
    }

    /// The direction the sweep ends at, in radians from +X, counter-clockwise.
    pub fn direction(self) -> f64 {
        self.end.radians() + self.sweep.to_radians()
    }

    /// The direction halfway along the sweep, in radians: where the angle's plate sits.
    pub fn middle(self) -> f64 {
        self.end.radians() + self.sweep.to_radians() / 2.0
    }
}

/// The angle a direction shows: the smaller one, read from the nearer end of the X axis.
///
/// A zero vector has no direction and reads as `0` from `+X`, which is the one place a sketch
/// falls back to a direction the pointer did not name.
pub fn sketch_angle(vector: DVec2) -> SketchAngle {
    let degrees = vector.y.atan2(vector.x).to_degrees();
    if degrees.abs() <= 90.0 {
        SketchAngle { end: XEnd::Plus, sweep: degrees }
    } else {
        SketchAngle { end: XEnd::Minus, sweep: if degrees > 0.0 { degrees - 180.0 } else { degrees + 180.0 } }
    }
}

/// One plate of the point being placed: the number, and the work-plane point it sits beside.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointRow {
    /// The parameter the plate edits.
    pub param: FieldParam,
    /// The live number, in the parameter's own unit.
    pub value: f64,
    /// The point of the work plane the plate belongs to, in millimetres.
    pub anchor: DVec2,
}

/// The reference guide: the X end the angle is read from and the sweep to the pointer.
///
/// The radius of both the axis and the arc is the length of the segment being placed, so the guide
/// is the same size as the thing it measures (ADR 0127).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reference {
    /// The point the axis starts at, in work-plane millimetres.
    pub anchor: DVec2,
    /// The radius of the axis and of the arc, in millimetres.
    pub radius: f64,
    /// The direction of the X end, in radians from +X, counter-clockwise.
    pub from: f64,
    /// The direction being placed, in radians from +X, counter-clockwise.
    pub to: f64,
}

/// The point the next click would place, with the plates and guides that name it.
#[derive(Clone, Debug, PartialEq)]
pub struct PointState {
    /// The point itself, in whole micrometres: what a click places.
    pub point: [Length; 2],
    /// The same point on the work plane, in millimetres.
    pub at: DVec2,
    /// The plates, in the order the tool declares them.
    pub rows: Vec<PointRow>,
    /// The chord being measured, when the figure has a second fixed point to measure from.
    pub construction: Option<[DVec2; 2]>,
    /// The X axis and the sweep the angle is read from.
    pub reference: Option<Reference>,
}

/// The measurement the next point of a tool is read from.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Shape {
    /// The point itself: X and Y.
    Position,
    /// A length and an angle from the anchor.
    Segment,
    /// A rectangle's two sides from the anchor: `centred` when the anchor is its middle.
    Sides { centred: bool },
    /// A radius from the anchor, and the direction of the rim point when `turned`.
    Radius { turned: bool },
    /// A direction at a radius the earlier point fixed.
    Sweep { radius: f64 },
}

/// What the next point of `draft` measures, and where it is measured from.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Stage {
    /// The work-plane point the numbers are measured from, in millimetres.
    anchor: DVec2,
    /// The numbers the point takes.
    shape: Shape,
    /// The angle the rectangle's sides are turned to, in radians; zero for every other shape.
    turn: f64,
}

impl PointState {
    /// What the pointer alone places at the running draft's next point.
    pub fn live(draft: &DrawDraft, pointer: DVec2) -> Option<Self> {
        Self::measure(draft, pointer, None)
    }

    /// What the plates place: the pointer with every number the person typed in its place.
    pub fn placed(draft: &DrawDraft, pointer: DVec2, fields: &FieldSet) -> Option<Self> {
        Self::measure(draft, pointer, Some(fields))
    }

    /// The point `draft` would place with `pointer`, and the plates that name it.
    fn measure(draft: &DrawDraft, pointer: DVec2, fields: Option<&FieldSet>) -> Option<Self> {
        let stage = stage(draft, pointer)?;
        let read = |param: FieldParam, live: f64| -> f64 {
            match fields {
                Some(fields) if fields.is_locked(param) => fields.value(param).unwrap_or(live),
                _ => live,
            }
        };
        let locked = |param: FieldParam| fields.is_some_and(|fields| fields.is_locked(param));
        let mut rows = Vec::new();
        let mut reference = None;
        let at = match stage.shape {
            Shape::Position => {
                let at = DVec2::new(read(FieldParam::X, pointer.x), read(FieldParam::Y, pointer.y));
                rows.push(PointRow { param: FieldParam::X, value: at.x, anchor: at });
                rows.push(PointRow { param: FieldParam::Y, value: at.y, anchor: at });
                at
            }
            Shape::Segment | Shape::Sweep { .. } => {
                let offset = pointer - stage.anchor;
                // A typed angle is a direction from +X; an untyped one is the pointer's own.
                let (direction, angle) = if locked(FieldParam::Angle) {
                    let typed = from_degrees(read(FieldParam::Angle, 0.0));
                    (typed, sketch_angle(typed))
                } else {
                    (offset, sketch_angle(offset))
                };
                let length = match stage.shape {
                    Shape::Sweep { radius } => radius,
                    _ => read(FieldParam::Length, offset.length()),
                };
                let at = stage.anchor + unit_or_x(direction) * length;
                if matches!(stage.shape, Shape::Segment) {
                    rows.push(PointRow {
                        param: FieldParam::Length,
                        value: length,
                        anchor: stage.anchor + (at - stage.anchor) / 2.0,
                    });
                }
                rows.push(PointRow {
                    param: FieldParam::Angle,
                    value: angle.shown(),
                    anchor: arc_anchor(stage.anchor, length, angle),
                });
                reference = Some(Reference {
                    anchor: stage.anchor,
                    radius: length,
                    from: angle.end.radians(),
                    to: angle.direction(),
                });
                at
            }
            Shape::Sides { centred } => {
                let local = to_local(pointer - stage.anchor, stage.turn);
                let sign_x = if local.x < 0.0 { -1.0 } else { 1.0 };
                let sign_y = if local.y < 0.0 { -1.0 } else { 1.0 };
                let live = |extent: f64| if centred { 2.0 * extent.abs() } else { extent.abs() };
                let width = read(FieldParam::Width, live(local.x));
                let height = read(FieldParam::Height, live(local.y));
                // A typed size keeps the quadrant the pointer is in: the corner (or the half
                // extents) is the typed number on the pointer's side of the anchor.
                let reach = if centred {
                    DVec2::new(width / 2.0 * sign_x, height / 2.0 * sign_y)
                } else {
                    DVec2::new(width * sign_x, height * sign_y)
                };
                let at = stage.anchor + to_world(reach, stage.turn);
                // The plates sit at the outer midpoint of their own side, so each number is beside
                // the edge it measures.
                let side = if centred { 0.0 } else { 0.5 };
                let width_row = DVec2::new(reach.x * side, reach.y);
                let height_row = DVec2::new(reach.x, reach.y * side);
                rows.push(PointRow {
                    param: FieldParam::Width,
                    value: width,
                    anchor: stage.anchor + to_world(width_row, stage.turn),
                });
                rows.push(PointRow {
                    param: FieldParam::Height,
                    value: height,
                    anchor: stage.anchor + to_world(height_row, stage.turn),
                });
                at
            }
            Shape::Radius { turned } => {
                let offset = pointer - stage.anchor;
                let radius = read(FieldParam::Radius, offset.length());
                let direction = if turned && locked(FieldParam::Rotation) {
                    from_degrees(read(FieldParam::Rotation, 0.0))
                } else {
                    offset
                };
                let rim = unit_or_x(direction);
                let at = stage.anchor + rim * radius;
                rows.push(PointRow {
                    param: FieldParam::Radius,
                    value: radius,
                    anchor: stage.anchor + rim * (radius / 2.0),
                });
                if turned {
                    let angle = sketch_angle(direction);
                    rows.push(PointRow {
                        param: FieldParam::Rotation,
                        value: degrees_or_zero(direction),
                        anchor: arc_anchor(stage.anchor, radius, angle),
                    });
                    reference = Some(Reference {
                        anchor: stage.anchor,
                        radius,
                        from: angle.end.radians(),
                        to: angle.direction(),
                    });
                }
                at
            }
        };
        Some(Self { point: to_lengths(at)?, at, construction: construction(draft, at), rows, reference })
    }
}

/// The chord a figure draws, when it has one to measure from.
///
/// An arc's chord is its start to the point being placed, and a pending arc's chord is the segment
/// it will close: both are a second fixed point the person can see the number against (ADR 0127).
fn construction(draft: &DrawDraft, at: DVec2) -> Option<[DVec2; 2]> {
    let start = draft.points().first().map(|point| plane_point(*point))?;
    let chord = match draft.tool() {
        DrawTool::Arc => draft.points().len() == 2,
        DrawTool::Polyline => draft.pending_arc().is_some(),
        _ => false,
    };
    chord.then_some([start, at])
}

/// What the next point of `draft` measures, or `None` when the tool takes no further point.
fn stage(draft: &DrawDraft, pointer: DVec2) -> Option<Stage> {
    let points = draft.points();
    let Some(first) = points.first().copied().map(plane_point) else {
        // Before the first click there is nothing to measure from: the point is X and Y of itself.
        return Some(Stage { anchor: pointer, shape: Shape::Position, turn: 0.0 });
    };
    let last = draft.last_end().map(plane_point).unwrap_or(first);
    let (anchor, shape, turn) = match draft.tool() {
        DrawTool::Line | DrawTool::Polyline => (last, Shape::Segment, 0.0),
        // The chord of a three-point arc runs from its start, so the angle has a fixed reference
        // and the chord can be drawn as a construction line.
        DrawTool::Arc => (first, Shape::Segment, 0.0),
        DrawTool::ArcFromCenter => {
            if let Some(start) = points.get(1).copied().map(plane_point) {
                (first, Shape::Sweep { radius: (start - first).length() }, 0.0)
            } else {
                (first, Shape::Segment, 0.0)
            }
        }
        DrawTool::Rectangle => (first, Shape::Sides { centred: false }, 0.0),
        DrawTool::RectangleFromCenter => (first, Shape::Sides { centred: true }, 0.0),
        DrawTool::RectangleFromCenterDirected => {
            if let Some(direction) = points.get(1).copied().map(plane_point) {
                let axis = direction - first;
                (first, Shape::Sides { centred: true }, axis.y.atan2(axis.x))
            } else {
                (first, Shape::Segment, 0.0)
            }
        }
        DrawTool::Circle => (first, Shape::Radius { turned: false }, 0.0),
        // Every point of a three-point circle rings the same way: each is a distance and an angle
        // from the one before it.
        DrawTool::CircleThreePoints => (last, Shape::Segment, 0.0),
        DrawTool::RegularPolygon => (first, Shape::Radius { turned: true }, 0.0),
    };
    Some(Stage { anchor, shape, turn })
}

/// The sketch point a plate sits at while an angle is drawn: on the arc, halfway along the sweep.
fn arc_anchor(anchor: DVec2, radius: f64, angle: SketchAngle) -> DVec2 {
    let middle = angle.middle();
    let reach = DVec2::new(middle.cos(), middle.sin()) * radius;
    anchor + reach
}

/// A direction as a unit vector, with `+X` for a point that has no direction at all.
fn unit_or_x(vector: DVec2) -> DVec2 {
    let length = vector.length();
    if length.is_finite() && length > 0.0 { vector / length } else { DVec2::X }
}

/// The unit vector of an angle read from +X, in degrees, counter-clockwise.
fn from_degrees(degrees: f64) -> DVec2 {
    let radians = degrees.to_radians();
    DVec2::new(radians.cos(), radians.sin())
}

/// The direction of a vector in degrees from +X, counter-clockwise, in `0..360`.
fn degrees_or_zero(vector: DVec2) -> f64 {
    let degrees = vector.y.atan2(vector.x).to_degrees();
    if degrees.is_finite() { degrees.rem_euclid(360.0) } else { 0.0 }
}

/// A work-plane point in whole micrometres, or `None` when it lies outside the drawing range.
fn to_lengths(point: DVec2) -> Option<[Length; 2]> {
    let x = micrometres(point.x)?;
    let y = micrometres(point.y)?;
    Some([x, y])
}

/// One coordinate in whole micrometres, or `None` when it is not a coordinate the document takes.
fn micrometres(millimetres: f64) -> Option<Length> {
    if !millimetres.is_finite() || millimetres.abs() > MAX_COORDINATE_MM {
        return None;
    }
    let micrometres = (millimetres * 1_000.0).round();
    if !micrometres.is_finite() || micrometres.abs() > i64::MAX as f64 {
        return None;
    }
    Some(Length::from_micrometres(micrometres as i64))
}

/// A document point in work-plane millimetres.
fn plane_point(point: [Length; 2]) -> DVec2 {
    DVec2::new(point[0].millimetres_f64(), point[1].millimetres_f64())
}

/// A rectangle's offset turned back into its own axes.
fn to_local(offset: DVec2, turn: f64) -> DVec2 {
    let (sin, cos) = turn.sin_cos();
    DVec2::new(offset.x * cos + offset.y * sin, -offset.x * sin + offset.y * cos)
}

/// A rectangle's own offset turned into the work plane.
fn to_world(offset: DVec2, turn: f64) -> DVec2 {
    let (sin, cos) = turn.sin_cos();
    DVec2::new(offset.x * cos - offset.y * sin, offset.x * sin + offset.y * cos)
}

/// The parameters a stage declares, for the test that every tool names its numbers.
#[cfg(test)]
fn params(state: &PointState) -> Vec<FieldParam> {
    state.rows.iter().map(|row| row.param).collect()
}

/// The plate `row` draws, at its screen anchor.
pub fn field_row(row: PointRow, anchor: egui::Pos2) -> FieldRow {
    FieldRow::for_param(row.param, row.value, anchor)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use egui::pos2;

    use super::*;

    fn millimetres(x: i64, y: i64) -> DVec2 {
        DVec2::new(x as f64, y as f64)
    }

    fn draft(tool: DrawTool, points: &[(i64, i64)]) -> DrawDraft {
        let mut draft = DrawDraft::with_sides(tool, 6);
        for (x, y) in points {
            draft.push([Length::from_micrometres(*x * 1_000), Length::from_micrometres(*y * 1_000)]).expect("a point");
        }
        draft
    }

    /// The plates of `draft` under `pointer`, as (parameter, value) pairs.
    fn plates(draft: &DrawDraft, pointer: DVec2) -> Vec<(FieldParam, f64)> {
        PointState::live(draft, pointer)
            .expect("the tool takes a point")
            .rows
            .iter()
            .map(|row| (row.param, (row.value * 10.0).round() / 10.0))
            .collect()
    }

    #[test]
    fn the_angle_reads_from_the_nearer_end_of_the_x_axis() {
        // One row per quadrant: the end the sweep is read from and the smaller angle it shows.
        let table = [
            (millimetres(100, 0), XEnd::Plus, 0.0),
            (millimetres(100, 100), XEnd::Plus, 45.0),
            (millimetres(-100, 100), XEnd::Minus, 45.0),
            (millimetres(-100, 0), XEnd::Minus, 0.0),
            (millimetres(-100, -100), XEnd::Minus, 45.0),
            (millimetres(100, -100), XEnd::Plus, 45.0),
            (millimetres(-100, 20), XEnd::Minus, 11.3),
            (millimetres(-100, -20), XEnd::Minus, 11.3),
        ];
        for (vector, end, shown) in table {
            let angle = sketch_angle(vector);
            assert_eq!(angle.end, end, "{vector:?} reads from {end:?}");
            assert!((angle.shown() - shown).abs() < 0.05, "{vector:?} shows {shown}");
            assert!(angle.shown() <= 180.0, "no angle shows more than half a turn");
            // The sweep points back at the vector, so the guide arc and the plate agree.
            let back = DVec2::new(angle.direction().cos(), angle.direction().sin());
            let direct = vector.normalize();
            assert!((back - direct).length() < 1e-9, "{vector:?} keeps its own direction");
        }
    }

    #[test]
    fn every_shipped_tool_declares_its_numbers() {
        // The first point of every tool is the point itself.
        for tool in [
            DrawTool::Line,
            DrawTool::Polyline,
            DrawTool::Arc,
            DrawTool::ArcFromCenter,
            DrawTool::Rectangle,
            DrawTool::RectangleFromCenter,
            DrawTool::RectangleFromCenterDirected,
            DrawTool::Circle,
            DrawTool::CircleThreePoints,
            DrawTool::RegularPolygon,
        ] {
            let fresh = DrawDraft::with_sides(tool, 6);
            let state = PointState::live(&fresh, millimetres(50, 25)).expect("a first point");
            assert_eq!(params(&state), vec![FieldParam::X, FieldParam::Y], "{tool:?} starts with a point");
        }

        /// A tool, the points it already holds and the numbers its next point takes.
        type Case = (DrawTool, Vec<(i64, i64)>, Vec<FieldParam>);
        let cases: Vec<Case> = vec![
            (DrawTool::Line, vec![(0, 0)], vec![FieldParam::Length, FieldParam::Angle]),
            (DrawTool::Polyline, vec![(0, 0)], vec![FieldParam::Length, FieldParam::Angle]),
            (DrawTool::Arc, vec![(0, 0), (100, 50)], vec![FieldParam::Length, FieldParam::Angle]),
            (DrawTool::ArcFromCenter, vec![(0, 0)], vec![FieldParam::Length, FieldParam::Angle]),
            (DrawTool::ArcFromCenter, vec![(0, 0), (100, 0)], vec![FieldParam::Angle]),
            (DrawTool::Rectangle, vec![(0, 0)], vec![FieldParam::Width, FieldParam::Height]),
            (DrawTool::RectangleFromCenter, vec![(0, 0)], vec![FieldParam::Width, FieldParam::Height]),
            (DrawTool::RectangleFromCenterDirected, vec![(0, 0)], vec![FieldParam::Length, FieldParam::Angle]),
            (
                DrawTool::RectangleFromCenterDirected,
                vec![(0, 0), (100, 0)],
                vec![FieldParam::Width, FieldParam::Height],
            ),
            (DrawTool::Circle, vec![(0, 0)], vec![FieldParam::Radius]),
            (DrawTool::CircleThreePoints, vec![(0, 0), (100, 0)], vec![FieldParam::Length, FieldParam::Angle]),
            (DrawTool::RegularPolygon, vec![(0, 0)], vec![FieldParam::Radius, FieldParam::Rotation]),
        ];
        for (tool, points, expected) in cases {
            let draft = draft(tool, &points);
            let state = PointState::live(&draft, millimetres(300, 400)).expect("a next point");
            assert_eq!(params(&state), expected, "{tool:?} after {points:?}");
        }
    }

    #[test]
    fn a_segment_shows_its_length_and_the_pointers_angle() {
        let line = draft(DrawTool::Line, &[(0, 0)]);
        assert_eq!(plates(&line, millimetres(600, 0)), vec![(FieldParam::Length, 600.0), (FieldParam::Angle, 0.0)]);
        // A direction below the axis is the smaller angle from +X, so 30 and not 330.
        let below = DVec2::new(600.0 * 30f64.to_radians().cos(), -300.0);
        assert_eq!(plates(&line, below), vec![(FieldParam::Length, 600.0), (FieldParam::Angle, 30.0)]);
        // The guide's radius is the segment's own length, and it starts at the anchor.
        let state = PointState::live(&line, millimetres(0, 600)).expect("a point");
        let reference = state.reference.expect("a segment has a reference");
        assert_eq!(reference.anchor, DVec2::ZERO);
        assert_eq!(reference.radius, 600.0);
        assert_eq!(reference.from, XEnd::Plus.radians());
        assert!((reference.to - PI / 2.0).abs() < 1e-9);
    }

    #[test]
    fn a_rectangle_puts_each_number_on_its_own_side() {
        let rectangle = draft(DrawTool::Rectangle, &[(0, 0)]);
        let state = PointState::live(&rectangle, millimetres(600, 400)).expect("a point");
        assert_eq!(state.point, [Length::from_micrometres(600_000), Length::from_micrometres(400_000)]);
        let width = state.rows.iter().find(|row| row.param == FieldParam::Width).expect("width");
        assert_eq!(width.value, 600.0);
        assert_eq!(width.anchor, millimetres(300, 400), "the width sits on the far horizontal side");
        let height = state.rows.iter().find(|row| row.param == FieldParam::Height).expect("height");
        assert_eq!(height.value, 400.0);
        assert_eq!(height.anchor, millimetres(600, 200), "the height sits on the far vertical side");

        // A rectangle from its centre measures twice the offset and puts its plates on the sides.
        let centred = draft(DrawTool::RectangleFromCenter, &[(0, 0)]);
        let state = PointState::live(&centred, millimetres(300, 200)).expect("a point");
        assert_eq!(
            plates(&centred, millimetres(300, 200)),
            vec![(FieldParam::Width, 600.0), (FieldParam::Height, 400.0)]
        );
        assert_eq!(state.rows.first().map(|row| row.anchor), Some(millimetres(0, 200)));
        assert_eq!(state.rows.get(1).map(|row| row.anchor), Some(millimetres(300, 0)));
    }

    #[test]
    fn a_circle_and_a_polygon_take_a_radius() {
        let circle = draft(DrawTool::Circle, &[(100, 100)]);
        assert_eq!(plates(&circle, millimetres(130, 140)), vec![(FieldParam::Radius, 50.0)]);
        let state = PointState::live(&circle, millimetres(130, 140)).expect("a point");
        assert!(state.reference.is_none(), "a circle has no angle to read");
        assert_eq!(state.rows.first().map(|row| row.anchor), Some(millimetres(115, 120)), "on the radius line");

        let polygon = draft(DrawTool::RegularPolygon, &[(0, 0)]);
        assert_eq!(
            plates(&polygon, millimetres(0, 100)),
            vec![(FieldParam::Radius, 100.0), (FieldParam::Rotation, 90.0)]
        );
        assert!(PointState::live(&polygon, millimetres(0, 100)).expect("a point").reference.is_some());
    }

    /// The plates a segment stage declares, for a set built by hand.
    fn segment_rows() -> Vec<FieldRow> {
        vec![
            FieldRow::for_param(FieldParam::Length, 0.0, pos2(0.0, 0.0)),
            FieldRow::for_param(FieldParam::Angle, 0.0, pos2(0.0, 0.0)),
        ]
    }

    #[test]
    fn a_typed_length_and_angle_place_the_point_exactly() {
        let line = draft(DrawTool::Line, &[(0, 0)]);
        let fields = FieldSet::with_typed(segment_rows(), &[(FieldParam::Length, 600.0), (FieldParam::Angle, 30.0)])
            .expect("two plates");
        // The pointer is somewhere else entirely: the typed numbers win.
        let state = PointState::placed(&line, millimetres(123, 456), &fields).expect("a point");
        let expected = DVec2::new(600.0 * 30f64.to_radians().cos(), 600.0 * 30f64.to_radians().sin());
        assert!((state.at - expected).length() < 1e-3, "{:?} is 600 mm at 30 degrees", state.at);
        assert_eq!(state.point[0], Length::from_micrometres(519_615));
        assert_eq!(state.point[1], Length::from_micrometres(300_000));
    }

    #[test]
    fn a_typed_number_outranks_the_snap_and_a_zero_gives_it_back() {
        let line = draft(DrawTool::Line, &[(0, 0)]);
        let typed = FieldSet::with_typed(segment_rows(), &[(FieldParam::Length, 300.0)]).expect("two plates");
        // The pointer snaps to a 500 mm grid point; the promise holds.
        let state = PointState::placed(&line, millimetres(500, 0), &typed).expect("a point");
        assert_eq!(state.point, [Length::from_micrometres(300_000), Length::ZERO]);

        // A typed zero drops the lock, so the pointer drives the point again.
        let released = FieldSet::with_typed(segment_rows(), &[(FieldParam::Length, 0.0)]).expect("two plates");
        let state = PointState::placed(&line, millimetres(500, 0), &released).expect("a point");
        assert_eq!(state.point[0], Length::from_micrometres(500_000));
    }

    #[test]
    fn a_pending_arc_draws_its_chord_and_an_arc_tool_its_own() {
        let mut polyline = draft(DrawTool::Polyline, &[(0, 0), (300, 0)]);
        assert!(PointState::live(&polyline, millimetres(300, 200)).expect("a point").construction.is_none());
        polyline.push_arc_through([Length::from_micrometres(300_000), Length::from_micrometres(200_000)]).expect("arm");
        let state = PointState::live(&polyline, millimetres(0, 200)).expect("a point");
        assert_eq!(state.construction, Some([DVec2::ZERO, millimetres(0, 200)]));

        let arc = draft(DrawTool::Arc, &[(0, 0), (50, 50)]);
        let state = PointState::live(&arc, millimetres(100, 0)).expect("a point");
        assert_eq!(state.construction, Some([DVec2::ZERO, millimetres(100, 0)]));
        // The chord is measured from the arc's start, not from its middle point.
        assert_eq!(state.reference.map(|reference| reference.anchor), Some(DVec2::ZERO));
    }

    #[test]
    fn a_hostile_pointer_places_nothing() {
        let line = draft(DrawTool::Line, &[(0, 0)]);
        for pointer in [DVec2::new(f64::NAN, 0.0), DVec2::new(0.0, f64::INFINITY), millimetres(20_000, 0)] {
            assert!(PointState::live(&line, pointer).is_none(), "{pointer:?} places no point");
        }
    }
}
