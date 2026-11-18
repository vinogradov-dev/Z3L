//! The rubber band: which of a frame's shapes a viewport rectangle covers.
//!
//! A pick answers "what is under this point"; a band answers "what does this rectangle take". The
//! band is drawn in screen space, so the test runs on the **projected silhouette** of every shape:
//! the convex hull of its projected vertices, which for a box — the furniture case — is exactly the
//! outline the viewport draws. A mesh is never walked triangle by triangle, so the cost is one
//! projection per vertex and one hull per shape, paid once when the drag is released.
//!
//! Three coverages come out of one walk: a shape is [`BandCoverage::Inside`] when every projected
//! vertex is inside the rectangle, [`BandCoverage::Touched`] when its silhouette reaches it without
//! lying inside, and [`BandCoverage::Missed`] otherwise. Every shape of the frame is reported, so a
//! caller can tell an object one of whose boards the band enclosed from an object it fully enclosed.
//! Which coverage a gesture acts on — a window or a crossing band — is a gesture decision and lives
//! in `ui` (ADR 0137); this module only says what a rectangle covers.
//!
//! Coordinates are normalized device coordinates, the space [`Camera::project_ndc`] returns and
//! [`Camera::ray_ndc`] accepts, so no viewport measurement enters the test: `(-1, -1)` is the
//! bottom-left corner of the view and `(1, 1)` the top-right.
//!
//! Changes when: what a band covers, or the outline it measures, changes.
//!
//! [ADR 0137]: ../docs/decisions/0137-a-rubber-band-selects-by-the-direction-it-is-dragged.md

use geom::{DVec2, DVec3};

use crate::{Camera, SceneFrame};

/// One view's rectangle in normalized device coordinates: the band a selection drag drew.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NdcRect {
    /// The low corner, component by component.
    pub min: DVec2,
    /// The high corner, component by component.
    pub max: DVec2,
}

impl NdcRect {
    /// The rectangle two corners span, whichever way round the band was drawn.
    ///
    /// The drag direction is lost here on purpose: a window and a crossing band cover the same
    /// rectangle, and only the rule they apply differs.
    pub fn from_corners(first: DVec2, second: DVec2) -> Self {
        Self {
            min: DVec2::new(first.x.min(second.x), first.y.min(second.y)),
            max: DVec2::new(first.x.max(second.x), first.y.max(second.y)),
        }
    }

    /// Whether the point lies inside the rectangle, edges included.
    pub fn contains(self, point: DVec2) -> bool {
        point.x >= self.min.x && point.x <= self.max.x && point.y >= self.min.y && point.y <= self.max.y
    }

    /// Whether the rectangle has no area, which covers nothing.
    pub fn is_empty(self) -> bool {
        self.max.x <= self.min.x || self.max.y <= self.min.y
    }

    /// The four corners, low to high and round again.
    fn corners(self) -> [DVec2; 4] {
        [
            DVec2::new(self.min.x, self.min.y),
            DVec2::new(self.max.x, self.min.y),
            DVec2::new(self.max.x, self.max.y),
            DVec2::new(self.min.x, self.max.y),
        ]
    }
}

/// How a shape's projected silhouette meets a band.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BandCoverage {
    /// Every projected point of the shape lies inside the band.
    Inside,
    /// The silhouette reaches the band without all of it lying inside, so the band crosses it.
    Touched,
    /// The band does not reach the shape — or nothing of the shape is in front of the camera, which
    /// a band cannot cover either.
    Missed,
}

impl SceneFrame {
    /// Every shape of the frame, in frame order, with how the band meets it.
    ///
    /// `aspect` is the viewport's width over its height, and `band` is the rectangle the drag drew
    /// in that viewport. Every shape is reported — a missed one as [`BandCoverage::Missed`] — so a
    /// caller that knows which shapes belong to one object can tell "all of it is inside" from "one
    /// board is". A shape is reported once per drawable, path or envelope; a frame that draws the
    /// same id twice reports it twice.
    ///
    /// An empty band covers nothing and reports nothing.
    pub fn band_hits(&self, camera: Camera, aspect: f64, band: NdcRect) -> Vec<(model::Id, BandCoverage)> {
        let mut hits = Vec::new();
        if band.is_empty() {
            return hits;
        }
        for drawable in &self.drawables {
            let world = |position: &[f32; 3]| {
                drawable.world_transform.transform_point3(DVec3::new(
                    f64::from(position[0]),
                    f64::from(position[1]),
                    f64::from(position[2]),
                ))
            };
            let shape = silhouette(camera, aspect, drawable.mesh.positions.iter().map(world));
            push(&mut hits, drawable.id, &shape, band);
        }
        for polyline in &self.polylines {
            let world = |point: &DVec3| polyline.world_transform.transform_point3(*point);
            let shape = silhouette(camera, aspect, polyline.points.iter().map(world));
            push(&mut hits, polyline.id, &shape, band);
        }
        for envelope in &self.product_envelopes {
            let shape = silhouette(camera, aspect, envelope_corners(envelope).into_iter());
            push(&mut hits, envelope.id, &shape, band);
        }
        hits
    }
}

/// The corner points of one product's authored box, in world space.
///
/// A band asks only whether a shape is covered, so the exact footprint is not needed: the eight
/// corners of the box around the reserved space are the cheapest conservative answer, and a
/// footprint cut inside that box can only be covered less. A malformed outline falls back to the
/// product's own zero so the test is never a panic.
fn envelope_corners(envelope: &crate::ProductEnvelope) -> [DVec3; 8] {
    let (min, max) = envelope.local_bounds().unwrap_or((DVec3::ZERO, DVec3::ZERO));
    let corner = |x: f64, y: f64, z: f64| envelope.world_transform.transform_point3(DVec3::new(x, y, z));
    [
        corner(min.x, min.y, min.z),
        corner(max.x, min.y, min.z),
        corner(max.x, max.y, min.z),
        corner(min.x, max.y, min.z),
        corner(min.x, min.y, max.z),
        corner(max.x, min.y, max.z),
        corner(max.x, max.y, max.z),
        corner(min.x, max.y, max.z),
    ]
}

/// Records how the band meets one shape.
fn push(hits: &mut Vec<(model::Id, BandCoverage)>, id: model::Id, shape: &[DVec2], band: NdcRect) {
    let coverage = if shape.is_empty() {
        BandCoverage::Missed
    } else if shape.iter().all(|point| band.contains(*point)) {
        BandCoverage::Inside
    } else if reaches(shape, band) {
        BandCoverage::Touched
    } else {
        BandCoverage::Missed
    };
    hits.push((id, coverage));
}

/// The convex hull of the points a camera can place, in projection order.
///
/// The hull is what a silhouette is: a box projects to the hull of its eight corners, a contour to
/// the hull of its vertices. Points at or behind the camera plane are dropped by the projection, so
/// a shape half behind the viewer keeps the outline of what is in front of it.
fn silhouette(camera: Camera, aspect: f64, points: impl Iterator<Item = DVec3>) -> Vec<DVec2> {
    let projected: Vec<DVec2> = points.filter_map(|point| camera.project_ndc(point, aspect)).collect();
    hull(projected)
}

/// The convex hull of `points`, counter-clockwise; one or two points are returned as they are.
///
/// Andrew's monotone chain: sort, then walk the lower and the upper hull. Collinear points are
/// dropped, so a straight silhouette stays a segment rather than a sliver.
fn hull(mut points: Vec<DVec2>) -> Vec<DVec2> {
    points.sort_by(|left, right| left.x.total_cmp(&right.x).then(left.y.total_cmp(&right.y)));
    points.dedup_by(|left, right| left.x == right.x && left.y == right.y);
    if points.len() < 3 {
        return points;
    }
    let mut lower: Vec<DVec2> = Vec::with_capacity(points.len());
    for point in &points {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], *point) <= 0.0 {
            lower.pop();
        }
        lower.push(*point);
    }
    let mut upper: Vec<DVec2> = Vec::with_capacity(points.len());
    for point in points.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], *point) <= 0.0 {
            upper.pop();
        }
        upper.push(*point);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// The z of the cross product of `o -> a` and `o -> b`, the side `b` lies on.
fn cross(origin: DVec2, a: DVec2, b: DVec2) -> f64 {
    (a.x - origin.x) * (b.y - origin.y) - (a.y - origin.y) * (b.x - origin.x)
}

/// Whether the projected `shape` meets `band` at all.
///
/// Two convex polygons overlap in three ways: a vertex of one lies inside the other, or their
/// edges cross. The band is axis-aligned, so its four corners and its four sides are the whole
/// test. A silhouette of one or two points has no corners, so only the point test and the edge
/// crossing apply — which is right: a segment through the band reaches it.
fn reaches(shape: &[DVec2], band: NdcRect) -> bool {
    if shape.iter().any(|point| band.contains(*point)) {
        return true;
    }
    let corners = band.corners();
    if shape.len() >= 3 && corners.iter().any(|corner| inside_hull(shape, *corner)) {
        return true;
    }
    if shape.len() < 2 {
        return false;
    }
    (0..shape.len()).any(|index| {
        let from = shape[index];
        let to = shape[(index + 1) % shape.len()];
        (0..4).any(|corner| segments_cross(from, to, corners[corner], corners[(corner + 1) % 4]))
    })
}

/// Whether `point` lies inside the convex `hull`, edges included.
///
/// The hull's winding is not trusted: the test reads the side of the first edge that is not
/// collinear and requires every other edge to agree with it.
fn inside_hull(hull: &[DVec2], point: DVec2) -> bool {
    if hull.len() < 3 {
        return false;
    }
    let mut side = 0.0;
    for index in 0..hull.len() {
        let edge = cross(hull[index], hull[(index + 1) % hull.len()], point);
        if edge.abs() <= 1.0e-9 {
            continue;
        }
        if side == 0.0 {
            side = edge.signum();
        } else if edge.signum() != side {
            return false;
        }
    }
    true
}

/// Whether the two segments meet, touching included.
fn segments_cross(first: DVec2, second: DVec2, third: DVec2, fourth: DVec2) -> bool {
    let sides = [
        cross(first, second, third),
        cross(first, second, fourth),
        cross(third, fourth, first),
        cross(third, fourth, second),
    ];
    let opposite = |left: f64, right: f64| (left > 0.0 && right < 0.0) || (left < 0.0 && right > 0.0);
    if opposite(sides[0], sides[1]) && opposite(sides[2], sides[3]) {
        return true;
    }
    const TOUCH: f64 = 1.0e-9;
    (sides[0].abs() <= TOUCH && on_segment(first, second, third))
        || (sides[1].abs() <= TOUCH && on_segment(first, second, fourth))
        || (sides[2].abs() <= TOUCH && on_segment(third, fourth, first))
        || (sides[3].abs() <= TOUCH && on_segment(third, fourth, second))
}

/// Whether `point`, known to be collinear with the segment, lies between its ends.
fn on_segment(from: DVec2, to: DVec2, point: DVec2) -> bool {
    point.x >= from.x.min(to.x)
        && point.x <= from.x.max(to.x)
        && point.y >= from.y.min(to.y)
        && point.y <= from.y.max(to.y)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use std::sync::Arc;

    use geom::{DMat4, MeshData};

    use super::*;
    use crate::{Drawable, Polyline, ProductEnvelope, ProductEnvelopeState};

    /// The aspect ratio the tests view through; any positive value projects the same way.
    const ASPECT: f64 = 16.0 / 9.0;

    fn id(value: u64) -> model::Id {
        model::Id::new(value).expect("id")
    }

    /// A flat square of `side` millimetres standing in the XY plane of its own frame.
    fn square(value: u64, at: DVec3, side: f32) -> Drawable {
        let half = side / 2.0;
        let mesh = Arc::new(MeshData {
            positions: vec![[-half, -half, 0.0], [half, -half, 0.0], [half, half, 0.0], [-half, half, 0.0]],
            normals: Vec::new(),
            indices: vec![0, 1, 2, 0, 2, 3],
        });
        Drawable::new(id(value), mesh, DMat4::from_translation(at), [1.0; 4])
    }

    /// The same square turned 45° about Z: a diamond whose bounds cover far more than it does.
    fn diamond(value: u64, side: f32) -> Drawable {
        let mesh = Arc::new(MeshData {
            positions: vec![[side, 0.0, 0.0], [0.0, side, 0.0], [-side, 0.0, 0.0], [0.0, -side, 0.0]],
            normals: Vec::new(),
            indices: vec![0, 1, 2, 0, 2, 3],
        });
        Drawable::new(id(value), mesh, DMat4::from_translation(DVec3::new(0.0, 0.0, 400.0)), [1.0; 4])
    }

    /// The projected silhouette of one drawable, the way the band sees it.
    fn outline(camera: Camera, drawable: &Drawable) -> Vec<DVec2> {
        silhouette(
            camera,
            ASPECT,
            drawable.mesh.positions.iter().map(|position| {
                drawable.world_transform.transform_point3(DVec3::new(
                    f64::from(position[0]),
                    f64::from(position[1]),
                    f64::from(position[2]),
                ))
            }),
        )
    }

    /// The band around `shape`, grown by `margin` on every side.
    fn around(shape: &[DVec2], margin: f64) -> NdcRect {
        let (min, max) = bounds(shape);
        NdcRect { min: DVec2::new(min.x - margin, min.y - margin), max: DVec2::new(max.x + margin, max.y + margin) }
    }

    fn bounds(shape: &[DVec2]) -> (DVec2, DVec2) {
        let mut min = DVec2::splat(f64::INFINITY);
        let mut max = DVec2::splat(f64::NEG_INFINITY);
        for point in shape {
            min = DVec2::new(min.x.min(point.x), min.y.min(point.y));
            max = DVec2::new(max.x.max(point.x), max.y.max(point.y));
        }
        (min, max)
    }

    /// A frame holding the drawables, in the order given.
    fn frame_of(drawables: &[Drawable]) -> SceneFrame {
        let mut frame = SceneFrame::default();
        for drawable in drawables {
            frame.push(drawable.clone());
        }
        frame
    }

    #[test]
    fn a_shape_inside_the_band_is_inside_and_one_the_band_cuts_is_touched() {
        let camera = Camera::default();
        let left = square(2, DVec3::new(-300.0, 0.0, 0.0), 100.0);
        let right = square(3, DVec3::new(250.0, 0.0, 0.0), 100.0);
        let frame = frame_of(&[left.clone(), right.clone()]);

        // A window around the left square takes it and misses the right one.
        let window = around(&outline(camera, &left), 0.01);
        assert_eq!(
            frame.band_hits(camera, ASPECT, window),
            vec![(id(2), BandCoverage::Inside), (id(3), BandCoverage::Missed)],
            "a missed shape is reported too, so a caller can tell a part from the whole"
        );

        // A band over the left half of the right square touches it without enclosing it.
        let (min, max) = bounds(&outline(camera, &right));
        let crossing = NdcRect {
            min: DVec2::new(min.x - 0.01, min.y - 0.01),
            max: DVec2::new((min.x + max.x) / 2.0, max.y + 0.01),
        };
        assert_eq!(
            frame.band_hits(camera, ASPECT, crossing),
            vec![(id(2), BandCoverage::Missed), (id(3), BandCoverage::Touched)]
        );

        // A band over the whole view encloses both.
        let all = NdcRect { min: DVec2::splat(-2.0), max: DVec2::splat(2.0) };
        assert_eq!(
            frame.band_hits(camera, ASPECT, all),
            vec![(id(2), BandCoverage::Inside), (id(3), BandCoverage::Inside)]
        );
    }

    #[test]
    fn a_band_reads_the_silhouette_and_not_the_screen_bounds() {
        // A diamond's screen bounds reach into four corners the shape never visits, so a band in
        // one of those corners must not take it: the band tests the projected outline, not a box.
        let camera = Camera::default();
        let diamond = diamond(4, 100.0);
        let frame = frame_of(std::slice::from_ref(&diamond));
        let shape = outline(camera, &diamond);
        let (min, max) = bounds(&shape);
        let step = (max - min) * 0.02;
        let corner = NdcRect { min: DVec2::new(min.x, max.y - step.y), max: DVec2::new(min.x + step.x, max.y) };
        assert_eq!(
            frame.band_hits(camera, ASPECT, corner),
            vec![(id(4), BandCoverage::Missed)],
            "a band beside the shape misses it"
        );
        // The middle of the shape is inside a band struck around its centre.
        let centre = (min + max) / 2.0;
        let middle = NdcRect {
            min: DVec2::new(centre.x - 0.02, centre.y - 0.02),
            max: DVec2::new(centre.x + 0.02, centre.y + 0.02),
        };
        assert_eq!(
            frame.band_hits(camera, ASPECT, middle),
            vec![(id(4), BandCoverage::Touched)],
            "the band lies inside the outline"
        );
    }

    #[test]
    fn a_line_chain_and_a_product_envelope_are_covered_too() {
        let camera = Camera::default();
        let line = Polyline::new(
            id(5),
            Arc::from(vec![DVec3::new(-200.0, 0.0, 0.0), DVec3::new(200.0, 0.0, 0.0)]),
            DMat4::IDENTITY,
            [0.5, 0.5, 0.5, 1.0],
        );
        let projected: Vec<DVec2> = line
            .points
            .iter()
            .filter_map(|point| camera.project_ndc(line.world_transform.transform_point3(*point), ASPECT))
            .collect();
        let (min, max) = bounds(&projected);
        let mut frame = SceneFrame::default();
        frame.push_polyline(line);
        let across = NdcRect {
            min: DVec2::new((min.x + max.x) / 2.0 - 0.01, min.y - 0.01),
            max: DVec2::new((min.x + max.x) / 2.0 + 0.01, max.y + 0.01),
        };
        assert_eq!(frame.band_hits(camera, ASPECT, across), vec![(id(5), BandCoverage::Touched)]);

        // A closed product draws no shapes of its own, but its box is still what a band takes.
        let envelope = ProductEnvelope::rectangular(
            id(6),
            DVec3::new(400.0, 300.0, 500.0),
            DMat4::IDENTITY,
            ProductEnvelopeState::ClosedWithParts,
        );
        let corners: Vec<DVec2> =
            envelope_corners(&envelope).into_iter().filter_map(|corner| camera.project_ndc(corner, ASPECT)).collect();
        let mut frame = SceneFrame::default();
        frame.push_product_envelope(envelope);
        assert_eq!(
            frame.band_hits(camera, ASPECT, around(&corners, 0.01)),
            vec![(id(6), BandCoverage::Inside)],
            "the envelope is covered as a box"
        );
    }

    #[test]
    fn an_empty_band_covers_nothing() {
        let camera = Camera::default();
        let frame = frame_of(&[square(7, DVec3::ZERO, 100.0)]);
        let point = NdcRect { min: DVec2::ZERO, max: DVec2::ZERO };
        assert!(point.is_empty());
        assert!(frame.band_hits(camera, ASPECT, point).is_empty());
    }

    #[test]
    fn a_shape_behind_the_camera_is_missed() {
        let camera = Camera::default();
        let behind = square(8, camera.eye() - camera.forward() * 500.0, 10.0);
        let frame = frame_of(std::slice::from_ref(&behind));
        assert!(outline(camera, &behind).is_empty(), "nothing of it is in front of the viewer");
        let all = NdcRect { min: DVec2::splat(-2.0), max: DVec2::splat(2.0) };
        assert_eq!(
            frame.band_hits(camera, ASPECT, all),
            vec![(id(8), BandCoverage::Missed)],
            "a band cannot cover what it cannot see"
        );
    }
}
