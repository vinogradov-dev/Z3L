//! The planar ring model: one material component as exact closed rings.
//!
//! A [`Region`] is a closed outer [`Contour`] ring and zero or more hole rings, in millimetres
//! (ADR 0009, ADR 0012). Every ring stays exact: a line stays a line and an arc stays an arc, so
//! area, bounds and containment never depend on a sampling tolerance. [`Region::sample`] is the only
//! lossy step, and a caller picks its tolerance once, at the polygon-only kernel boundary.
//!
//! The contract: the constructors refuse an open, degenerate or non-finite ring and a ring that
//! repeats a neighbour, naming the ring and the point in [`RegionError`]. Coincidence is
//! [`POINT_EPSILON`] millimetres and an angle counts as inside a sweep at [`ANGLE_EPSILON`] radians;
//! the rectangle tracer in [`crate::rect`] welds at the same point bound instead of declaring its own.
//!
//! It does not combine regions: the general boolean is [`crate::boolean`], the exact orthogonal one
//! over plain rectangles is [`crate::rect`]. Holes are not checked against the outer ring here.
//!
//! Changes when: what a region is, or what an exact ring answers, changes.

use std::f64::consts::{FRAC_PI_2, PI, TAU};
use std::fmt;

use crate::rect::{Rect, RectError};
use crate::{Arc, Contour, ContourError, DVec2, Line, Segment};

/// Coincidence tolerance for ring points, in millimetres.
///
/// The rectangle boolean welds its corners at the same bound, so it reads this constant from here
/// instead of declaring a second one.
pub(crate) const POINT_EPSILON: f64 = 1.0e-9;

/// Tolerance for two angles of a ring to count as the same direction, in radians.
///
/// An arc angle closer than this to a sweep bound counts as on it, and the general boolean reads it
/// for a turn and a joint sweep, so it stays beside the ring model instead of being declared twice.
pub(crate) const ANGLE_EPSILON: f64 = 1.0e-9;

/// One material component in the plane: a closed outer ring and zero or more hole rings.
///
/// Every ring is an exact [`Contour`]; a ring built from sampled points is a contour of line
/// segments, so nothing is lost either way. Rings are validated on construction: each must be
/// closed. Holes are not checked against the outer ring here; [`crate::rect::regions_from_rects`]
/// produces consistent regions from the rectangle decomposition.
#[derive(Clone, Debug, PartialEq)]
pub struct Region {
    outer: Contour,
    holes: Vec<Contour>,
}

impl Region {
    /// Creates a region from exact closed rings.
    pub fn new(outer: Contour, holes: Vec<Contour>) -> Result<Self, RegionError> {
        validate_ring(&outer, RingKind::Outer)?;
        for (index, hole) in holes.iter().enumerate() {
            validate_ring(hole, RingKind::Hole { index })?;
        }
        Ok(Self { outer, holes })
    }

    /// Creates a region from sampled rings: every closing edge becomes an exact line segment.
    ///
    /// This is the bridge for a traced polyline such as the rectangle decomposition. A ring with
    /// fewer than three points, a non-finite point or a repeated neighbour is refused with the
    /// matching [`RegionError`].
    pub fn from_loops(outer: Vec<DVec2>, holes: Vec<Vec<DVec2>>) -> Result<Self, RegionError> {
        let outer = loop_to_contour(outer, RingKind::Outer)?;
        let holes = holes
            .into_iter()
            .enumerate()
            .map(|(index, hole)| loop_to_contour(hole, RingKind::Hole { index }))
            .collect::<Result<Vec<_>, _>>()?;
        Self::new(outer, holes)
    }

    /// The region of one axis-aligned rectangle at the origin.
    ///
    /// The outer ring runs counter-clockwise from the minimum corner. A zero, negative or
    /// non-finite width or height is [`RectError`], so a caller can report the bad dimension.
    pub fn rectangle(width: f64, height: f64) -> Result<Self, RegionError> {
        let rect = Rect::new(DVec2::ZERO, DVec2::new(width, height))?;
        Self::from_rect(rect)
    }

    /// The region of an already-validated rectangle.
    pub fn from_rect(rect: Rect) -> Result<Self, RegionError> {
        let min = rect.min();
        let max = rect.max();
        let ring = vec![min, DVec2::new(max.x, min.y), max, DVec2::new(min.x, max.y)];
        Self::from_loops(ring, Vec::new())
    }

    /// The outer ring.
    pub fn outer(&self) -> &Contour {
        &self.outer
    }

    /// The hole rings.
    pub fn holes(&self) -> &[Contour] {
        &self.holes
    }

    /// The material area: the outer ring's area minus every hole's area, in square millimetres.
    /// Ring orientation does not matter; an invalid region whose holes exceed the outer ring
    /// yields a non-positive value, which callers turn into a diagnostic, never a panic.
    pub fn area(&self) -> f64 {
        contour_area(&self.outer) - self.holes.iter().map(contour_area).sum::<f64>()
    }

    /// The material of this region on the free side of one line, or `None` when none of it is there.
    ///
    /// The line is `normal · point = offset` and the free side is `normal · point >= offset`, the
    /// convention a niche's wall planes already use. Rings stay exact: a cut line stays a line and an
    /// arc stays the shorter arc, because the clip is the boundary-of-the-set boolean of
    /// [`crate::boolean`] against a rectangle that covers the whole region on the free side — never a
    /// sample. A region entirely on the free side comes back as it went in; one entirely on the other
    /// side answers `None`.
    ///
    /// The outer ring is expected to be **convex**, which is what a wall plane can bound: the clip is
    /// then one piece or none. A ring that is not convex may be cut into several pieces, and only the
    /// largest is answered — a caller that can meet one needs the boolean itself.
    ///
    /// A zero, non-finite or NaN normal, or a non-finite offset, is `None`.
    pub fn clip_half_plane(&self, normal: DVec2, offset: f64) -> Result<Option<Self>, RegionError> {
        if !normal.is_finite() || !offset.is_finite() {
            return Ok(None);
        }
        let length = normal.length();
        if !length.is_finite() || length <= POINT_EPSILON {
            return Ok(None);
        }
        let unit = normal / length;
        let offset = offset / length;
        let Some((low, high)) = self.bounds() else {
            return Ok(None);
        };
        let span = high - low;
        // The covering piece has to reach the whole region from the line, whatever side it lies on,
        // so it is taken around the region's own centre projected onto that line.
        let reach = span.length() * 2.0 + 1.0;
        let middle = (low + high) * 0.5;
        let origin = middle - unit * (unit.dot(middle) - offset);
        let along = DVec2::new(-unit.y, unit.x);
        let corners = vec![
            origin + along * reach,
            origin - along * reach,
            origin - along * reach + unit * reach,
            origin + along * reach + unit * reach,
        ];
        let free = Region::from_loops(corners, Vec::new())?;
        let pieces = crate::boolean::intersect_regions(std::slice::from_ref(self), std::slice::from_ref(&free))?;
        Ok(pieces.into_iter().max_by(|first, second| first.area().total_cmp(&second.area())))
    }

    /// The axis-aligned bounds of every ring, in millimetres.
    ///
    /// Returns `None` only for a region with no rings, which the constructors reject. An arc
    /// contributes its own extremes, not its chord.
    pub fn bounds(&self) -> Option<(DVec2, DVec2)> {
        let mut bounds: Option<(DVec2, DVec2)> = None;
        for ring in std::iter::once(&self.outer).chain(self.holes.iter()) {
            let Some((min, max)) = ring.bounds() else {
                continue;
            };
            bounds = Some(match bounds {
                Some((current_min, current_max)) => (current_min.min(min), current_max.max(max)),
                None => (min, max),
            });
        }
        bounds
    }

    /// Even-odd containment: a point is inside the material when it is inside the outer ring and in
    /// no hole. The ray test walks exact lines and arcs, so a curved edge is never approximated.
    pub fn contains(&self, point: DVec2) -> bool {
        if !ring_contains(&self.outer, point) {
            return false;
        }
        for hole in &self.holes {
            if ring_contains(hole, point) {
                return false;
            }
        }
        true
    }

    /// The spans of the line `origin + t * direction` that lie inside the material.
    ///
    /// The line is solved against every ring segment's own supporting line or circle, never
    /// sampled: a straight edge is one crossing, an arc up to two, and a joint shared by two
    /// segments counts once. Each span between two consecutive crossings is classified by its
    /// midpoint with [`Region::contains`], so a hole ring removes its own span and a ring's
    /// orientation never matters. The spans come back sorted and only of positive length: a line
    /// that merely touches the boundary yields none.
    ///
    /// `direction` sets the parameter's scale — a point of the line is `origin + t * direction`, so
    /// a unit direction makes `t` a distance in millimetres. A zero, non-finite or degenerate
    /// direction is an empty result, and so is a line that misses the material.
    pub fn line_intervals(&self, origin: DVec2, direction: DVec2) -> Vec<(f64, f64)> {
        if !origin.is_finite() || !direction.is_finite() || direction.length() <= LINE_DIRECTION_EPSILON {
            return Vec::new();
        }
        let mut crossings = Vec::new();
        for ring in std::iter::once(&self.outer).chain(self.holes.iter()) {
            for segment in ring.segments() {
                crossings.extend(crossing_parameters(*segment, origin, direction));
            }
        }
        crossings.sort_by(|first, second| first.partial_cmp(second).unwrap_or(std::cmp::Ordering::Equal));
        crossings.dedup_by(|first, second| (*first - *second).abs() <= SPAN_TOLERANCE);
        let mut spans = Vec::new();
        for pair in crossings.windows(2) {
            let (from, to) = (pair[0], pair[1]);
            if to - from <= SPAN_TOLERANCE {
                continue;
            }
            let middle = origin + direction * ((from + to) * 0.5);
            if self.contains(middle) {
                spans.push((from, to));
            }
        }
        spans
    }

    /// The distance from `point` to the nearest ring, in millimetres.
    ///
    /// The answer is exact for every ring: a straight edge projects onto its own span, an arc onto
    /// its own sweep and its two ends. A point inside a notch and a point outside the outer ring are
    /// measured alike, so a caller that means "how much room is there around this point" gets the
    /// same number on either side of a boundary. An empty or non-finite input is
    /// [`f64::INFINITY`].
    pub fn boundary_distance(&self, point: DVec2) -> f64 {
        if !point.is_finite() {
            return f64::INFINITY;
        }
        let mut nearest = f64::INFINITY;
        for ring in std::iter::once(&self.outer).chain(self.holes.iter()) {
            for segment in ring.segments() {
                nearest = nearest.min(segment_distance(*segment, point));
            }
        }
        nearest
    }

    /// Samples every ring into a polyline at `tolerance` millimetres.
    ///
    /// This is the only lossy step in the module. A caller that reaches the polygon-only kernel
    /// picks the tolerance once and shares it with every other profile.
    pub fn sample(&self, tolerance: f64) -> Result<RegionSamples, ContourError> {
        let outer = self.outer.sample(tolerance)?;
        let holes = self.holes.iter().map(|hole| hole.sample(tolerance)).collect::<Result<Vec<_>, _>>()?;
        Ok(RegionSamples { outer, holes })
    }
}

/// Tolerance for two crossings of one line to count as the same point, in millimetres.
const SPAN_TOLERANCE: f64 = 1.0e-6;

/// The shortest direction a line query accepts, in millimetres.
///
/// A direction shorter than this cannot place a crossing: two crossings a micrometre apart in a
/// parameter that is already this small would be noise, not geometry.
const LINE_DIRECTION_EPSILON: f64 = 1.0e-9;

/// The parameters at which one ring segment meets the line `origin + t * direction`.
fn crossing_parameters(segment: Segment, origin: DVec2, direction: DVec2) -> Vec<f64> {
    match segment {
        Segment::Line(line) => {
            let edge = line.end() - line.start();
            let denominator = cross(direction, edge);
            if !denominator.is_finite() || denominator.abs() <= LINE_DIRECTION_EPSILON {
                return Vec::new();
            }
            let parameter = cross(line.start() - origin, edge) / denominator;
            if !parameter.is_finite() {
                return Vec::new();
            }
            let point = origin + direction * parameter;
            if line.contains_point(point, SPAN_TOLERANCE) { vec![parameter] } else { Vec::new() }
        }
        Segment::Arc(arc) => {
            let offset = origin - arc.center();
            let a = direction.length_squared();
            let b = 2.0 * offset.dot(direction);
            let c = offset.length_squared() - arc.radius() * arc.radius();
            let discriminant = b * b - 4.0 * a * c;
            if !discriminant.is_finite() || discriminant < 0.0 {
                return Vec::new();
            }
            let root = discriminant.sqrt();
            let mut parameters = Vec::with_capacity(2);
            for parameter in [(-b - root) / (2.0 * a), (-b + root) / (2.0 * a)] {
                if !parameter.is_finite() {
                    continue;
                }
                let point = origin + direction * parameter;
                if arc.contains_point(point, SPAN_TOLERANCE) {
                    parameters.push(parameter);
                }
            }
            parameters
        }
    }
}

/// The distance from `point` to one segment, measured on the segment's own extent.
fn segment_distance(segment: Segment, point: DVec2) -> f64 {
    match segment {
        Segment::Line(line) => {
            let edge = line.end() - line.start();
            let length_squared = edge.length_squared();
            if !length_squared.is_finite() || length_squared <= LINE_DIRECTION_EPSILON {
                return (point - line.start()).length();
            }
            let parameter = ((point - line.start()).dot(edge) / length_squared).clamp(0.0, 1.0);
            (point - (line.start() + edge * parameter)).length()
        }
        Segment::Arc(arc) => {
            let support = arc.nearest_on_support(point);
            match support {
                Some(foot) if arc.contains_point(foot, SPAN_TOLERANCE) => (point - foot).length(),
                _ => (point - arc.start()).length().min((point - arc.end()).length()),
            }
        }
    }
}

/// The two-dimensional cross product of two vectors.
fn cross(first: DVec2, second: DVec2) -> f64 {
    first.x * second.y - first.y * second.x
}

/// A region sampled into polylines: the outer ring and then the hole rings in ring order.
#[derive(Clone, Debug, PartialEq)]
pub struct RegionSamples {
    outer: Vec<DVec2>,
    holes: Vec<Vec<DVec2>>,
}

impl RegionSamples {
    /// The sampled outer ring; the first point is not repeated at the end.
    pub fn outer(&self) -> &[DVec2] {
        &self.outer
    }

    /// The sampled hole rings.
    pub fn holes(&self) -> &[Vec<DVec2>] {
        &self.holes
    }

    /// Consumes the samples into the outer ring and the hole rings.
    pub fn into_parts(self) -> (Vec<DVec2>, Vec<Vec<DVec2>>) {
        (self.outer, self.holes)
    }
}

/// Which ring a [`RegionError`] refers to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RingKind {
    /// The outer ring.
    Outer,
    /// The hole at `index`.
    Hole {
        /// Index of the hole.
        index: usize,
    },
}

impl fmt::Display for RingKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Outer => formatter.write_str("outer"),
            Self::Hole { index } => write!(formatter, "hole {index}"),
        }
    }
}

/// Why a [`Region`] cannot be constructed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegionError {
    /// A ring has fewer than three points.
    Degenerate {
        /// The ring that is degenerate.
        kind: RingKind,
    },
    /// A ring contains a point that is not finite.
    NonFinite {
        /// The ring that contains the point.
        kind: RingKind,
        /// Index of the offending point.
        index: usize,
    },
    /// Two consecutive points of a ring are closer than [`POINT_EPSILON`].
    DuplicatePoint {
        /// The ring that contains the points.
        kind: RingKind,
        /// Index of the first of the two points.
        index: usize,
    },
    /// A ring is an open contour, so it bounds no material.
    OpenRing {
        /// The ring that is open.
        kind: RingKind,
    },
    /// A ring cannot become a contour of line segments.
    Ring {
        /// The ring that cannot be built.
        kind: RingKind,
        /// Why the contour was refused.
        error: ContourError,
    },
    /// A rectangle the region would be built from has no positive extent.
    Rect(RectError),
    /// A boolean would examine more segment pairs than it may.
    TooComplex {
        /// The number of segments the operands hold.
        segments: usize,
        /// The largest pair count one boolean accepts.
        max: usize,
    },
    /// A boolean result ring could not be built.
    ResultRing {
        /// Why the ring was refused.
        error: ContourError,
    },
}

impl fmt::Display for RegionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Degenerate { kind } => write!(formatter, "the {kind} ring needs at least three points"),
            Self::NonFinite { kind, index } => {
                write!(formatter, "point {index} of the {kind} ring is not finite")
            }
            Self::DuplicatePoint { kind, index } => {
                write!(formatter, "point {index} of the {kind} ring repeats its neighbour")
            }
            Self::OpenRing { kind } => write!(formatter, "the {kind} ring is not closed"),
            Self::Ring { kind, error } => write!(formatter, "the {kind} ring is invalid: {error}"),
            Self::Rect(error) => write!(formatter, "{error}"),
            Self::TooComplex { segments, max } => write!(
                formatter,
                "a boolean over {segments} segments needs more than the {max} edge pairs it may examine"
            ),
            Self::ResultRing { error } => write!(formatter, "a boolean result ring is invalid: {error}"),
        }
    }
}

impl From<RectError> for RegionError {
    fn from(error: RectError) -> Self {
        Self::Rect(error)
    }
}

impl std::error::Error for RegionError {}

/// The unsigned area of one exact ring; zero for a ring that cannot be measured.
///
/// Shared with the general boolean, which pairs a result hole with the smallest ring that holds it.
pub(crate) fn contour_area(ring: &Contour) -> f64 {
    ring.signed_area().unwrap_or(0.0).abs()
}

/// Even-odd containment by a ray to the right: an odd number of crossings is inside.
///
/// Shared with the rectangle tracer, which assigns a traced hole to the outer ring that holds it.
pub(crate) fn ring_contains(ring: &Contour, point: DVec2) -> bool {
    if !point.is_finite() {
        return false;
    }
    let mut inside = false;
    for segment in ring.segments() {
        if segment_crossings(*segment, point) % 2 == 1 {
            inside = !inside;
        }
    }
    inside
}

/// Crossings of one segment with the horizontal ray that runs right from `point`.
///
/// A segment counts its start and not its end, so a shared joint is counted once. A curve that
/// merely touches the ray is not a crossing.
fn segment_crossings(segment: Segment, point: DVec2) -> usize {
    match segment {
        Segment::Line(line) => line_crossings(line, point),
        Segment::Arc(arc) => arc_crossings(arc, point),
    }
}

fn line_crossings(line: Line, point: DVec2) -> usize {
    let start = line.start();
    let end = line.end();
    if (start.y > point.y) == (end.y > point.y) {
        return 0;
    }
    let parameter = (point.y - start.y) / (end.y - start.y);
    let x = start.x + (end.x - start.x) * parameter;
    usize::from(x > point.x)
}

/// Crossings of an arc with the horizontal ray, split at the arc's Y extremes.
///
/// Splitting at every `±π/2` angle makes each piece monotone in Y, so the same half-open rule as a
/// line applies and a joint between two pieces is counted once.
fn arc_crossings(arc: Arc, point: DVec2) -> usize {
    let total = arc.sweep_angle().abs();
    if !total.is_finite() || total <= ANGLE_EPSILON {
        return 0;
    }
    let mut fractions = vec![0.0_f64];
    for step in -2..=2 {
        let angle = FRAC_PI_2 + step as f64 * PI;
        let delta = if arc.sweep_angle() > 0.0 {
            turn_angle(angle - arc.start_angle())
        } else {
            turn_angle(arc.start_angle() - angle)
        };
        if delta > ANGLE_EPSILON && delta < total - ANGLE_EPSILON {
            fractions.push(delta / total);
        }
    }
    fractions.push(1.0);
    fractions.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    fractions.dedup_by(|a, b| (*a - *b).abs() < ANGLE_EPSILON);

    let mut crossings = 0;
    for pair in fractions.windows(2) {
        let (Some(start), Some(end)) = (arc.point_at(pair[0]), arc.point_at(pair[1])) else {
            continue;
        };
        if (start.y > point.y) == (end.y > point.y) {
            continue;
        }
        let Some(middle) = arc.point_at((pair[0] + pair[1]) * 0.5) else {
            continue;
        };
        let side = (middle.x - arc.center().x).signum();
        if side == 0.0 {
            continue;
        }
        let ratio = ((point.y - arc.center().y) / arc.radius()).clamp(-1.0, 1.0);
        let x = arc.center().x + side * arc.radius() * (1.0 - ratio * ratio).max(0.0).sqrt();
        if x > point.x {
            crossings += 1;
        }
    }
    crossings
}

/// The same angle in `0..TAU`; shared with the general boolean, which measures a turn by it.
pub(crate) fn turn_angle(angle: f64) -> f64 {
    let mut value = angle % TAU;
    if value < 0.0 {
        value += TAU;
    }
    value
}

fn validate_ring(ring: &Contour, kind: RingKind) -> Result<(), RegionError> {
    if ring.is_closed() { Ok(()) } else { Err(RegionError::OpenRing { kind }) }
}

/// Builds an exact line contour from a point loop, closing it from the last point to the first.
///
/// Shared with the rectangle tracer, which turns each closed boundary walk into a ring.
pub(crate) fn loop_to_contour(points: Vec<DVec2>, kind: RingKind) -> Result<Contour, RegionError> {
    let mut points = points;
    if points.len() > 1
        && let (Some(first), Some(last)) = (points.first(), points.last())
        && (first.x - last.x).abs() <= POINT_EPSILON
        && (first.y - last.y).abs() <= POINT_EPSILON
    {
        points.pop();
    }
    if points.len() < 3 {
        return Err(RegionError::Degenerate { kind });
    }
    for (index, point) in points.iter().enumerate() {
        if !point.is_finite() {
            return Err(RegionError::NonFinite { kind, index });
        }
    }

    let mut segments = Vec::with_capacity(points.len());
    for (index, &start) in points.iter().enumerate() {
        let Some(&end) = points.get((index + 1) % points.len()) else {
            return Err(RegionError::Degenerate { kind });
        };
        let line = Line::new(start, end).map_err(|error| match error {
            ContourError::ZeroLength => RegionError::DuplicatePoint { kind, index },
            ContourError::InvalidInput(_) => RegionError::NonFinite { kind, index },
            error => RegionError::Ring { kind, error },
        })?;
        segments.push(Segment::Line(line));
    }
    match Contour::new(segments, true) {
        Ok(contour) => Ok(contour),
        Err(ContourError::Degenerate) => Err(RegionError::Degenerate { kind }),
        Err(error) => Err(RegionError::Ring { kind, error }),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn rect(x: f64, y: f64, width: f64, height: f64) -> Vec<DVec2> {
        vec![DVec2::new(x, y), DVec2::new(x + width, y), DVec2::new(x + width, y + height), DVec2::new(x, y + height)]
    }

    fn rounded_corner() -> f64 {
        let radius = 20.0;
        radius * radius - PI * radius * radius / 4.0
    }

    fn rounded_plate() -> Region {
        let radius = 20.0;
        let segments = vec![
            Segment::Line(Line::new(DVec2::ZERO, DVec2::new(100.0, 0.0)).expect("bottom")),
            Segment::Line(Line::new(DVec2::new(100.0, 0.0), DVec2::new(100.0, 50.0 - radius)).expect("right")),
            Segment::Arc(Arc::new(DVec2::new(100.0 - radius, 50.0 - radius), radius, 0.0, FRAC_PI_2).expect("corner")),
            Segment::Line(Line::new(DVec2::new(100.0 - radius, 50.0), DVec2::new(0.0, 50.0)).expect("top")),
            Segment::Line(Line::new(DVec2::new(0.0, 50.0), DVec2::ZERO).expect("left")),
        ];
        Region::new(Contour::new(segments, true).expect("rounded plate"), vec![]).expect("region")
    }

    fn rect_with_hole() -> Region {
        Region::from_loops(rect(0.0, 0.0, 100.0, 50.0), vec![rect(10.0, 10.0, 20.0, 10.0)]).expect("a rect with a hole")
    }

    #[test]
    fn a_rectangle_has_its_area() {
        let region = Region::from_loops(rect(0.0, 0.0, 100.0, 50.0), vec![]).expect("valid rect");
        assert!((region.area() - 5000.0).abs() < 1.0e-9);
    }

    #[test]
    fn the_rectangle_constructor_runs_counter_clockwise_from_the_origin() {
        let region = Region::rectangle(100.0, 50.0).expect("valid rectangle");
        assert_eq!(region.outer().segments().len(), 4);
        assert_eq!(region.outer().start(), Some(DVec2::ZERO));
        assert!(region.outer().is_counter_clockwise() == Some(true));
        assert_eq!(region.bounds(), Some((DVec2::ZERO, DVec2::new(100.0, 50.0))));
        assert!((region.area() - 5000.0).abs() < 1.0e-9);
        assert!(region.contains(DVec2::new(50.0, 25.0)));
        assert!(!region.contains(DVec2::new(150.0, 25.0)));
    }

    #[test]
    fn a_rectangle_sampled_through_the_region_keeps_its_four_corners() {
        let region = Region::rectangle(100.0, 50.0).expect("valid rectangle");
        let samples = region.sample(0.05).expect("samples");
        assert_eq!(samples.outer(), &rect(0.0, 0.0, 100.0, 50.0)[..]);
        assert!(samples.holes().is_empty());
        let (outer, holes) = samples.into_parts();
        assert_eq!(outer.len(), 4);
        assert!(holes.is_empty());
    }

    #[test]
    fn a_rectangle_without_positive_extent_is_rejected() {
        assert!(matches!(Region::rectangle(0.0, 10.0), Err(RegionError::Rect(RectError::Degenerate))));
        assert!(matches!(Region::rectangle(10.0, -1.0), Err(RegionError::Rect(RectError::Degenerate))));
        assert!(matches!(Region::rectangle(f64::NAN, 10.0), Err(RegionError::Rect(RectError::NonFinite))));
    }

    #[test]
    fn a_hole_subtracts_its_area() {
        let region = rect_with_hole();
        let outer_area = 100.0 * 50.0;
        let hole_area = 20.0 * 10.0;
        assert!((region.area() - (outer_area - hole_area)).abs() < 1.0e-9);
    }

    #[test]
    fn points_inside_the_material_are_contained() {
        let region = rect_with_hole();
        assert!(region.contains(DVec2::new(5.0, 5.0)));
        assert!(region.contains(DVec2::new(90.0, 40.0)));
    }

    #[test]
    fn a_point_in_a_hole_is_not_contained() {
        let region = rect_with_hole();
        assert!(!region.contains(DVec2::new(15.0, 15.0)));
    }

    #[test]
    fn degenerate_rings_are_rejected() {
        assert!(matches!(Region::from_loops(vec![], vec![]), Err(RegionError::Degenerate { kind: RingKind::Outer })));
        assert!(matches!(
            Region::from_loops(rect(0.0, 0.0, 100.0, 50.0), vec![vec![]]),
            Err(RegionError::Degenerate { kind: RingKind::Hole { index: 0 } })
        ));
    }

    #[test]
    fn duplicate_ring_points_are_rejected() {
        let ring = vec![DVec2::new(0.0, 0.0), DVec2::new(10.0, 0.0), DVec2::new(10.0, 0.0), DVec2::new(0.0, 10.0)];
        assert!(matches!(
            Region::from_loops(ring, vec![]),
            Err(RegionError::DuplicatePoint { kind: RingKind::Outer, index: 1 })
        ));
    }

    #[test]
    fn non_finite_ring_points_are_rejected() {
        let ring = vec![DVec2::new(0.0, 0.0), DVec2::new(f64::NAN, 5.0), DVec2::new(3.0, 0.0)];
        assert!(matches!(
            Region::from_loops(ring, vec![]),
            Err(RegionError::NonFinite { kind: RingKind::Outer, index: 1 })
        ));
    }

    #[test]
    fn an_open_ring_is_rejected() {
        let open = Contour::new(
            vec![
                Segment::Line(Line::new(DVec2::ZERO, DVec2::new(10.0, 0.0)).expect("bottom")),
                Segment::Line(Line::new(DVec2::new(10.0, 0.0), DVec2::new(10.0, 10.0)).expect("right")),
            ],
            false,
        )
        .expect("open path");
        assert!(matches!(Region::new(open, vec![]), Err(RegionError::OpenRing { kind: RingKind::Outer })));
    }

    /// The rounded plate's ring as the segments it came in as, for the clip's exactness tests.
    fn segments_of(region: &Region) -> Vec<Segment> {
        region.outer().segments().to_vec()
    }

    #[test]
    fn a_clip_that_misses_the_region_answers_it_unchanged() {
        // A wall that does not reach the frame changes nothing: the clip must hand back the very ring,
        // arcs and all (N33).
        let plate = rounded_plate();
        let clipped = plate.clip_half_plane(DVec2::new(1.0, 0.0), -10.0).expect("a clip").expect("the plate is there");
        assert_eq!(segments_of(&clipped), segments_of(&plate), "no edge moved, no arc became a line");
        assert!(clipped.outer().is_counter_clockwise() == Some(true));
        let far = plate.clip_half_plane(DVec2::new(0.0, 1.0), 500.0).expect("a clip");
        assert!(far.is_none(), "and a wall past the whole frame keeps nothing of it");
    }

    #[test]
    fn a_clip_through_an_arc_keeps_the_shorter_arc() {
        // The line `y = 45` runs through the plate's 20 mm corner arc, which is centred at (80, 30):
        // the kept part is `y <= 45`, so the arc runs from (100, 30) to the crossing at
        // (80 + sqrt(400 - 225), 45) = (93.23, 45).
        let plate = rounded_plate();
        let clipped =
            plate.clip_half_plane(DVec2::new(0.0, -1.0), -45.0).expect("a clip").expect("the plate is still there");
        let segments = segments_of(&clipped);
        assert_eq!(segments.len(), 5, "a line where the cut runs, and four pieces of the old ring");
        let arcs: Vec<Arc> = segments
            .iter()
            .filter_map(|segment| if let Segment::Arc(arc) = segment { Some(*arc) } else { None })
            .collect();
        let [arc] = arcs.as_slice() else {
            panic!("the corner is still one arc, was {arcs:?}");
        };
        assert!((arc.radius() - 20.0).abs() < 1.0e-9, "the frame's own radius, never a chord of it");
        assert!((arc.center() - DVec2::new(80.0, 30.0)).length() < 1.0e-9, "and its own centre");
        assert!((arc.start() - DVec2::new(100.0, 30.0)).length() < 1.0e-6, "the arc keeps its own start");
        assert!((arc.end() - DVec2::new(93.228_756_5, 45.0)).length() < 1.0e-5, "and ends on the cut line");
        assert!(clipped.area() < plate.area(), "the plate lost the corner above the line");
        assert!((clipped.bounds().expect("bounds").1.y - 45.0).abs() < 1.0e-6, "the cut is the new top");
    }

    #[test]
    fn a_clip_that_leaves_nothing_answers_none() {
        let plate = rounded_plate();
        assert!(plate.clip_half_plane(DVec2::new(0.0, 1.0), 500.0).expect("a clip").is_none());
        assert!(plate.clip_half_plane(DVec2::ZERO, 0.0).expect("a zero normal clips nothing").is_none());
        assert!(plate.clip_half_plane(DVec2::new(f64::NAN, 1.0), 0.0).expect("a NaN normal clips nothing").is_none());
    }

    #[test]
    fn a_clip_of_a_plain_rectangle_is_the_rectangle_beside_the_line() {
        let region = Region::from_loops(rect(0.0, 0.0, 100.0, 50.0), vec![]).expect("a rectangle");
        let clipped = region.clip_half_plane(DVec2::new(1.0, 0.0), 60.0).expect("a clip").expect("the right-hand part");
        let (low, high) = clipped.bounds().expect("bounds");
        assert!((low - DVec2::new(60.0, 0.0)).length() < 1.0e-6, "the cut is the new left edge: {low:?}");
        assert!((high - DVec2::new(100.0, 50.0)).length() < 1.0e-6, "{high:?}");
        assert!((clipped.area() - 40.0 * 50.0).abs() < 1.0e-6, "the area of the kept half: {}", clipped.area());
    }

    #[test]
    fn an_exact_ring_answers_area_bounds_and_containment_without_sampling() {
        let region = rounded_plate();
        assert!((region.area() - (5000.0 - rounded_corner())).abs() < 1.0e-9);
        assert_eq!(region.bounds(), Some((DVec2::ZERO, DVec2::new(100.0, 50.0))));
        assert!(region.contains(DVec2::new(50.0, 25.0)));
        assert!(!region.contains(DVec2::new(95.0, 45.0)), "the rounded corner is cut away");
        let samples = region.sample(0.05).expect("samples");
        assert!(samples.outer().len() > region.outer().segments().len());
    }

    #[test]
    fn an_arc_ring_contains_points_by_its_own_curve() {
        let ring = Contour::new(
            vec![
                Segment::Line(Line::new(DVec2::ZERO, DVec2::new(20.0, 0.0)).expect("bottom")),
                Segment::Arc(Arc::new(DVec2::new(20.0, 10.0), 10.0, -FRAC_PI_2, PI).expect("right half")),
                Segment::Line(Line::new(DVec2::new(20.0, 20.0), DVec2::ZERO).expect("left")),
            ],
            true,
        )
        .expect("arc ring");
        let region = Region::new(ring, vec![]).expect("region");
        assert!(region.contains(DVec2::new(28.0, 10.0)), "inside the bulge");
        assert!(region.contains(DVec2::new(22.0, 10.0)), "inside the bulge");
        assert!(!region.contains(DVec2::new(32.0, 10.0)), "right of the bulge");
        assert!(region.contains(DVec2::new(10.0, 5.0)), "inside the triangle");
        assert!(!region.contains(DVec2::new(5.0, 10.0)), "above the diagonal");
    }

    #[test]
    fn area_ignores_ring_orientation() {
        let clockwise_outer =
            vec![DVec2::new(0.0, 0.0), DVec2::new(0.0, 10.0), DVec2::new(10.0, 10.0), DVec2::new(10.0, 0.0)];
        let region = Region::from_loops(clockwise_outer, vec![rect(2.0, 2.0, 6.0, 6.0)]).expect("valid region");
        assert!((region.area() - 64.0).abs() < 1.0e-9);
    }

    #[test]
    fn a_line_through_a_rectangle_has_one_span_of_its_own_length() {
        let region = Region::rectangle(100.0, 50.0).expect("rectangle");
        let spans = region.line_intervals(DVec2::new(-10.0, 25.0), DVec2::X);
        assert_eq!(spans.len(), 1);
        assert!((spans[0].0 - 10.0).abs() < 1.0e-9, "enter at x = 0: {spans:?}");
        assert!((spans[0].1 - 110.0).abs() < 1.0e-9, "leave at x = 100: {spans:?}");
    }

    #[test]
    fn a_line_across_a_hole_has_two_spans() {
        let region = rect_with_hole();
        let spans = region.line_intervals(DVec2::new(-5.0, 15.0), DVec2::X);
        assert_eq!(spans.len(), 2, "the hole splits the line: {spans:?}");
        assert!((spans[0].0 - 5.0).abs() < 1.0e-9 && (spans[0].1 - 15.0).abs() < 1.0e-9);
        assert!((spans[1].0 - 35.0).abs() < 1.0e-9 && (spans[1].1 - 105.0).abs() < 1.0e-9, "{spans:?}");
    }

    #[test]
    fn a_line_that_misses_the_material_has_no_span() {
        let region = Region::rectangle(100.0, 50.0).expect("rectangle");
        assert!(region.line_intervals(DVec2::new(-10.0, 60.0), DVec2::X).is_empty());
        assert!(region.line_intervals(DVec2::ZERO, DVec2::ZERO).is_empty(), "a zero direction is no line");
    }

    #[test]
    fn a_line_that_only_touches_an_arc_has_no_span() {
        let region = rounded_plate();
        // The tangent to the corner arc at 45 degrees touches the region at exactly one point.
        let centre = DVec2::new(80.0, 30.0);
        let touch = centre + DVec2::new(1.0, 1.0).normalize() * 20.0;
        let spans = region.line_intervals(touch - DVec2::new(10.0, -10.0), DVec2::new(1.0, -1.0));
        assert!(spans.is_empty(), "a tangent line bounds no material: {spans:?}");
    }

    #[test]
    fn a_line_through_a_rounded_corner_enters_on_the_arc() {
        let region = rounded_plate();
        // Across the plate at 45 mm the right end is the corner arc, not the nominal 100 mm edge.
        let spans = region.line_intervals(DVec2::new(-10.0, 45.0), DVec2::X);
        assert_eq!(spans.len(), 1);
        let (start, end) = spans[0];
        assert!((start - 10.0).abs() < 1.0e-9, "the clean edge enters at 0: {spans:?}");
        // The arc is centred at (80, 30) with radius 20: y = 45 meets it at x = 80 + sqrt(400 - 225),
        // and the parameter is measured from the origin at x = -10.
        let expected = 90.0 + (400.0_f64 - 225.0).sqrt();
        assert!((end - expected).abs() < 1.0e-6, "the rounded corner ends the span at {expected}: {spans:?}");
    }

    #[test]
    fn the_parameter_measures_travel_in_the_supplied_direction() {
        let region = Region::rectangle(100.0, 50.0).expect("rectangle");
        let spans = region.line_intervals(DVec2::ZERO, DVec2::new(2.0, 0.0));
        assert_eq!(spans.len(), 1);
        assert!((spans[0].0 - 0.0).abs() < 1.0e-9);
        assert!((spans[0].1 - 50.0).abs() < 1.0e-9, "100 mm is fifty steps of two: {spans:?}");
    }

    #[test]
    fn boundary_distance_is_measured_on_the_rings_themselves() {
        let region = rect_with_hole();
        assert!((region.boundary_distance(DVec2::new(5.0, 25.0)) - 5.0).abs() < 1.0e-9, "to the left edge");
        assert!((region.boundary_distance(DVec2::new(20.0, 25.0)) - 5.0).abs() < 1.0e-9, "to the hole");
        assert!((region.boundary_distance(DVec2::new(-7.0, 25.0)) - 7.0).abs() < 1.0e-9, "from outside too");
        let to_hole_corner = (20.0_f64 * 20.0 + 5.0 * 5.0).sqrt();
        assert!(
            (region.boundary_distance(DVec2::new(50.0, 25.0)) - to_hole_corner).abs() < 1.0e-9,
            "the nearest is the hole's corner"
        );
    }

    #[test]
    fn boundary_distance_follows_an_arc_and_its_ends() {
        let region = rounded_plate();
        // The corner arc is centred at (80, 30) with radius 20.
        assert!((region.boundary_distance(DVec2::new(80.0, 30.0)) - 20.0).abs() < 1.0e-9, "to the corner arc");
        let from_corner = region.boundary_distance(DVec2::new(97.0, 47.0));
        let expected = 20.0 - (17.0_f64 * 17.0 + 17.0 * 17.0).sqrt();
        assert!((from_corner - expected.abs()).abs() < 1.0e-6, "measured on the support circle: {from_corner}");
    }
}
