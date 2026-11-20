//! A furniture surface as a box, in millimetres.
//!
//! Three pure queries measure the document against the same surfaces: the object move rule stops a
//! drag at one (`move_query`), the niche probe reports the first one along a ray (`niche`), and the
//! write refuses a placement that lands a part inside one that stays behind. They share this value so
//! they cannot disagree about where a surface is (ADR 0102, ADR 0103, ADR 0162).
//!
//! A surface carries two readings of the same box. The move rule needs the axis-aligned world bounds
//! of a turned body, so its eight corners are transformed and the tightest world box around them is
//! kept. The probe and the write's refusal need the true face, so the box keeps its own frame as well
//! and the ray — or the other box — is taken into it: a shelf in a splayed case is measured against
//! the slanted board and not against the box around it (ADR 0099, ADR 0162). Both readings are
//! measurement devices, never stored in the document.

use geom::DVec3;

use super::move_query::MoveAxis;
use crate::{Id, Placement};

/// What owns a furniture surface.
///
/// A product's authored envelope is a shell; a panel, an object or any other container is a body. The
/// probe adds [`SurfaceKind::Envelope`] for the face of the piece the ray started inside.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SurfaceKind {
    /// A virtual face of the product that holds the measured point.
    Envelope,
    /// A panel face.
    Panel,
    /// A geometry body: an object, a hardware model, a primitive.
    Body,
    /// Another product's envelope: its shell, not the space inside it.
    Shell,
}

/// One furniture surface as a box, in millimetres.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SurfaceBox {
    pub(crate) id: Id,
    /// The axis-aligned bounds of the surface in the world, millimetres.
    pub(crate) min: DVec3,
    pub(crate) max: DVec3,
    /// The surface's own box, before its placement: the true faces a probe meets.
    pub(crate) local_min: DVec3,
    pub(crate) local_max: DVec3,
    /// The node's world placement.
    pub(crate) world: Placement,
    pub(crate) kind: SurfaceKind,
}

/// One face of a surface a probe meets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FaceHit {
    /// The distance along the probed unit direction, in millimetres.
    pub(crate) distance: f64,
    /// The world point where the ray meets the face, in millimetres.
    pub(crate) point: DVec3,
    /// The face's unit normal in the world, pointing back along the ray.
    ///
    /// The free space therefore lies in the half-space `normal · (point - hit) >= 0`.
    pub(crate) normal: DVec3,
    /// The surface's own axis the face lies on: 0, 1 or 2.
    pub(crate) axis: usize,
}

/// The shortest shared volume that counts as an overlap, in millimetres: the document's own unit.
///
/// A placement is stored in whole micrometres, so a pair whose boxes are less than one micrometre
/// apart is touching after rounding and not one part sunk into another (ADR 0102, ADR 0162).
pub(crate) const CONTACT_MM: f64 = 0.001;

/// One axis of a [`SurfaceBox`].
#[derive(Clone, Copy, Debug)]
pub(crate) struct Span {
    pub(crate) min: f64,
    pub(crate) max: f64,
}

impl SurfaceBox {
    /// Whether the surface is a product shell rather than a body.
    pub(crate) fn is_shell(self) -> bool {
        self.kind == SurfaceKind::Shell
    }

    /// The box's extent on `axis`.
    pub(crate) fn span(self, axis: MoveAxis) -> Span {
        match axis {
            MoveAxis::X => Span { min: self.min.x, max: self.max.x },
            MoveAxis::Y => Span { min: self.min.y, max: self.max.y },
            MoveAxis::Z => Span { min: self.min.z, max: self.max.z },
        }
    }

    /// The extent of the volume this box shares with `other`, on each world axis, in millimetres.
    ///
    /// A zero extent is a pair that stands apart on that axis, so a box pair whose shared volume is
    /// zero on any axis is touching at most: a face is contact, not overlap (ADR 0102). These are the
    /// coarser reading the move rule measures and the fast one the exact test starts from.
    pub(crate) fn shared(self, other: Self) -> [f64; 3] {
        let mut shared = [0.0; 3];
        for (axis, extent) in shared.iter_mut().enumerate() {
            let low = self.min[axis].max(other.min[axis]);
            let high = self.max[axis].min(other.max[axis]);
            *extent = (high - low).max(0.0);
        }
        shared
    }

    /// The eight corners of the box in the world, in its own frame's order.
    pub(crate) fn corners(self) -> impl Iterator<Item = DVec3> {
        let (min, max) = (self.local_min, self.local_max);
        let world = self.world;
        [min.x, max.x].into_iter().flat_map(move |x| {
            [min.y, max.y]
                .into_iter()
                .flat_map(move |y| [min.z, max.z].into_iter().map(move |z| world.transform_point(DVec3::new(x, y, z))))
        })
    }

    /// Whether this box and `other` share a volume — a pair that only touches does not.
    ///
    /// The world box around a turned part is larger than the part (ADR 0102), so a *refusal* may not
    /// be read from it: two boards that meet flush have overlapping world boxes and share nothing.
    /// This is the exact answer, the separating-axis test over the two boxes' own three axes and the
    /// nine cross products of their edges: the boxes are apart as soon as one axis separates them by
    /// more than [`CONTACT_MM`], and a shared volume thinner than that on some axis is the document's
    /// own rounding and contact as well. A box whose geometry is not finite shares nothing.
    pub(crate) fn meets(self, other: Self) -> bool {
        // The world boxes are the cheap first test: an overlap here is impossible without one there.
        if self.shared(other).iter().any(|extent| *extent <= CONTACT_MM) {
            return false;
        }
        for axis in separating_axes(self, other) {
            let (low_a, high_a) = extent(self, axis);
            let (low_b, high_b) = extent(other, axis);
            if !low_a.is_finite() || !low_b.is_finite() || !high_a.is_finite() || !high_b.is_finite() {
                return false;
            }
            if high_a.min(high_b) - low_a.max(low_b) <= CONTACT_MM {
                return false;
            }
        }
        true
    }

    /// The same box moved rigidly by `delta` millimetres, or `None` when the move is not finite.
    ///
    /// It is how the rule asks what the drag would put where: the box keeps its own frame, so a turned
    /// body is measured as itself at the destination and not as the box around its start (ADR 0166).
    pub(crate) fn shifted(self, delta: DVec3) -> Option<Self> {
        if !delta.is_finite() {
            return None;
        }
        let world = Placement::new(self.world.translation() + delta, self.world.rotation()).ok()?;
        Some(Self { min: self.min + delta, max: self.max + delta, world, ..self })
    }

    /// The same box turned rigidly by `turn` about the world point `pivot`, or `None` when either is
    /// not a finite number.
    ///
    /// It is how a ring drag asks what the turn would put where: the box keeps its own frame, so a
    /// turned board is measured as itself at the destination and not as the box around it (X88). The
    /// world box around the turned box is rebuilt from the turned corners, exactly as
    /// [`surface_box`] built the start one.
    pub(crate) fn turned(self, turn: geom::DQuat, pivot: DVec3) -> Option<Self> {
        if !turn.is_finite() || !pivot.is_finite() {
            return None;
        }
        let translation = pivot + turn * (self.world.translation() - pivot);
        let world = Placement::new(translation, turn * self.world.rotation()).ok()?;
        surface_box(self.id, self.local_min, self.local_max, world, self.kind)
    }

    /// Whether this box lies wholly inside `outer`, by their world boxes.
    ///
    /// A body inside a shell is a piece inside a case and a unit inside another is a nested product:
    /// both are legal places and neither is a collision (ADR 0102). A box that pokes out of `outer`
    /// is not inside it, which is the conservative reading a containment exemption needs.
    pub(crate) fn inside(self, outer: Self) -> bool {
        (0..3).all(|axis| outer.min[axis] <= self.min[axis] && self.max[axis] <= outer.max[axis])
    }

    /// Whether the two world boxes share a volume thicker than the contact threshold.
    ///
    /// It is the coarse reading of [`SurfaceBox::meets`], used where the box around a body is the
    /// measure the tool always used; a coarse overlap is still an overlap, and a coarse clearance is a
    /// real clearance, because a box contains its body (ADR 0162, ADR 0166).
    pub(crate) fn overlaps_box(self, other: Self) -> bool {
        self.shared(other).iter().all(|extent| *extent > CONTACT_MM)
    }

    /// How deeply this box is sunk into `other`, in millimetres: the least overlap over every axis
    /// that could push them apart.
    ///
    /// It is the *going deeper* measure a drag and the write's guard read: a box pushed further into
    /// another can be separated along a longer travel than before, a board slid along a face keeps
    /// the depth it had (its thinnest overlap is its own thickness whichever way it slides), and a box
    /// that comes out answers zero, so contact stays contact. A shared volume would do neither: it
    /// grows under a rotation that sinks nothing and shrinks under a slide that changes no face
    /// (X86, ADR 0162, ADR 0178).
    pub(crate) fn depth(self, other: Self) -> f64 {
        let mut least = f64::INFINITY;
        for axis in separating_axes(self, other) {
            let (low_a, high_a) = extent(self, axis);
            let (low_b, high_b) = extent(other, axis);
            if !(low_a.is_finite() && high_a.is_finite() && low_b.is_finite() && high_b.is_finite()) {
                return 0.0;
            }
            least = least.min(high_a.min(high_b) - low_a.max(low_b));
        }
        if least.is_finite() { least.max(0.0) } else { 0.0 }
    }

    /// Whether `point` lies inside the box or on its faces.
    pub(crate) fn contains(self, point: DVec3) -> bool {
        point.x >= self.min.x
            && point.x <= self.max.x
            && point.y >= self.min.y
            && point.y <= self.max.y
            && point.z >= self.min.z
            && point.z <= self.max.z
    }

    /// The first face of this box a ray meets, in world millimetres.
    ///
    /// The ray is taken into the surface's own frame, so a turned panel is measured by its true face
    /// and not by the world box around it. A rotation keeps lengths, so the distance is the same in
    /// both frames. The returned normal always faces back along the ray, which puts the free space on
    /// the normal's side.
    pub(crate) fn face_hit(self, point: DVec3, direction: DVec3) -> Option<FaceHit> {
        let inverse = self.world.inverse().ok()?;
        let local_point = inverse.transform_point(point);
        let local_direction = inverse.transform_vector(direction);
        if !local_point.is_finite() || !local_direction.is_finite() {
            return None;
        }
        let (distance, axis, positive) = face_ahead(local_point, local_direction, self.local_min, self.local_max)?;
        let mut normal_local = DVec3::ZERO;
        normal_local[axis] = if positive { 1.0 } else { -1.0 };
        let mut normal = self.world.transform_vector(normal_local);
        if normal.dot(direction) > 0.0 {
            normal = -normal;
        }
        let hit = point + direction * distance;
        if !hit.is_finite() || !normal.is_finite() {
            return None;
        }
        Some(FaceHit { distance, point: hit, normal, axis })
    }
}

/// The unit axes a pair of boxes is separated along: each box's own three and the nine cross
/// products of their edges.
///
/// It is the axis set of the separating-axis test, written once so the refusal and the drag's own
/// stop cannot disagree about which directions a pair of turned bodies is apart in (ADR 0162,
/// ADR 0165). Two boxes whose faces are parallel have no cross axis to test: the cross product is
/// the zero vector and testing it would separate nothing.
pub(crate) fn separating_axes(first: SurfaceBox, second: SurfaceBox) -> Vec<DVec3> {
    let own_axes = |box_: SurfaceBox| [DVec3::X, DVec3::Y, DVec3::Z].map(|axis| box_.world.transform_vector(axis));
    let (own, theirs) = (own_axes(first), own_axes(second));
    let mut axes: Vec<DVec3> = Vec::with_capacity(15);
    axes.extend(own);
    axes.extend(theirs);
    for first in own {
        for second in theirs {
            let cross = first.cross(second);
            let length = cross.length();
            if length > 1.0e-9 {
                axes.push(cross / length);
            }
        }
    }
    axes
}

/// How far `box_` reaches along `axis`, as the lowest and highest corner.
pub(crate) fn extent(box_: SurfaceBox, axis: DVec3) -> (f64, f64) {
    box_.corners().fold((f64::INFINITY, f64::NEG_INFINITY), |(low, high), corner| {
        let reach = corner.dot(axis);
        (low.min(reach), high.max(reach))
    })
}

/// A local box through a node's world placement, as the world box around its eight corners.
///
/// A corner that is not finite is no box.
pub(crate) fn surface_box(id: Id, min: DVec3, max: DVec3, world: Placement, kind: SurfaceKind) -> Option<SurfaceBox> {
    let mut box_ = SurfaceBox {
        id,
        min: DVec3::splat(f64::INFINITY),
        max: DVec3::splat(f64::NEG_INFINITY),
        local_min: min,
        local_max: max,
        world,
        kind,
    };
    for x in [min.x, max.x] {
        for y in [min.y, max.y] {
            for z in [min.z, max.z] {
                let corner = world.transform_point(DVec3::new(x, y, z));
                if !corner.is_finite() {
                    return None;
                }
                box_.min = box_.min.min(corner);
                box_.max = box_.max.max(corner);
            }
        }
    }
    Some(box_)
}

/// The visible face a ray meets along a unit direction: distance, own axis and side, or `None`.
///
/// The slab test returns the near face for a point outside the box and the far face for a point
/// inside it; a ray that starts on a face and points inward meets the opposite face, while one that
/// starts on a face and points outward meets it at zero. That is what makes a point on the envelope
/// answer with a zero extent on the axis it leaves. The side is `true` for the box's positive face.
pub(crate) fn face_ahead(point: DVec3, direction: DVec3, min: DVec3, max: DVec3) -> Option<(f64, usize, bool)> {
    const EPSILON: f64 = 1.0e-9;
    let mut enter = f64::NEG_INFINITY;
    let mut exit = f64::INFINITY;
    let mut enter_face = (0usize, true);
    let mut exit_face = (0usize, true);
    for axis in 0..3 {
        let origin = point[axis];
        let step = direction[axis];
        let low = min[axis];
        let high = max[axis];
        if step.abs() <= EPSILON {
            if origin < low - EPSILON || origin > high + EPSILON {
                return None;
            }
            continue;
        }
        let first = (low - origin) / step;
        let second = (high - origin) / step;
        let near = first.min(second);
        let far = first.max(second);
        if near > enter {
            enter = near;
            enter_face = (axis, step < 0.0);
        }
        if far < exit {
            exit = far;
            exit_face = (axis, step > 0.0);
        }
    }
    if exit < enter || exit < 0.0 {
        return None;
    }
    let (axis, positive) = if enter > EPSILON { enter_face } else { exit_face };
    Some((if enter > EPSILON { enter } else { exit }, axis, positive))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    fn unit_box() -> (DVec3, DVec3) {
        (DVec3::ZERO, DVec3::new(10.0, 10.0, 10.0))
    }

    #[test]
    fn a_face_ahead_enters_from_outside_and_leaves_from_inside() {
        let (min, max) = unit_box();
        let outside = DVec3::new(-5.0, 5.0, 5.0);
        assert_eq!(face_ahead(outside, DVec3::X, min, max), Some((5.0, 0, false)), "the near face");
        let inside = DVec3::new(5.0, 5.0, 5.0);
        assert_eq!(face_ahead(inside, DVec3::X, min, max), Some((5.0, 0, true)), "the far face");
        let leaving = DVec3::new(0.0, 5.0, 5.0);
        assert_eq!(face_ahead(leaving, -DVec3::X, min, max), Some((0.0, 0, false)), "a face the ray already sits on");
        assert_eq!(face_ahead(leaving, DVec3::X, min, max), Some((10.0, 0, true)), "and the opposite one inward");
    }

    #[test]
    fn a_ray_that_misses_the_box_meets_no_face() {
        let (min, max) = unit_box();
        assert_eq!(face_ahead(DVec3::new(-5.0, 50.0, 5.0), DVec3::X, min, max), None);
        assert_eq!(
            face_ahead(DVec3::new(5.0, 5.0, 5.0), -DVec3::X, min, max),
            Some((5.0, 0, false)),
            "the far face backwards"
        );
        assert_eq!(face_ahead(DVec3::new(50.0, 5.0, 5.0), DVec3::X, min, max), None, "a box behind the ray");
    }

    /// A board's box: `length x width x thickness` millimetres in its own frame, at `world`.
    fn board(world: Placement, length: f64, width: f64, thickness: f64) -> SurfaceBox {
        surface_box(
            Id::new(1).expect("id"),
            DVec3::ZERO,
            DVec3::new(length, width, thickness),
            world,
            SurfaceKind::Panel,
        )
        .expect("a box")
    }

    #[test]
    fn a_pair_that_touches_shares_no_volume_and_a_pair_that_crosses_does() {
        // X72: the write's refusal is read from this answer, so a face is contact and never a refusal.
        let at = |x: f64| Placement::new(DVec3::new(x, 0.0, 0.0), geom::DQuat::IDENTITY).expect("a placement");
        let left = board(at(0.0), 700.0, 500.0, 18.0);
        assert!(!left.meets(board(at(700.0), 700.0, 500.0, 18.0)), "two boards face to face only touch");
        assert!(!left.meets(board(at(700.001), 700.0, 500.0, 18.0)), "a micrometre of air is air");
        assert!(left.meets(board(at(699.0), 700.0, 500.0, 18.0)), "a millimetre in is an overlap");
        assert!(left.meets(board(at(0.0), 700.0, 500.0, 18.0)), "and one board on another is an overlap");
    }

    #[test]
    fn two_turned_boards_that_meet_flush_share_nothing_though_their_world_boxes_do() {
        // The reason a refusal may not be read from the world box: two boards turned 45 degrees and
        // set face to face have world boxes that overlap deeply and faces that only touch. This is
        // the fill's own case — a shelf flush against a splayed post (ADR 0102, ADR 0162).
        let half = std::f64::consts::FRAC_1_SQRT_2;
        let quarter_turn = geom::DQuat::from_axis_angle(DVec3::Z, std::f64::consts::FRAC_PI_4);
        let here = Placement::new(DVec3::ZERO, quarter_turn).expect("a placement");
        // The second board starts where the first ends in the turned frame: an exact shared face.
        let beside = |shift: f64| {
            let step = 400.0 - shift;
            Placement::new(DVec3::new(step * half, step * half, 0.0), quarter_turn).expect("a placement")
        };
        let first = board(here, 400.0, 400.0, 18.0);
        let second = board(beside(0.0), 400.0, 400.0, 18.0);
        assert!(
            first.shared(second).iter().all(|extent| *extent > 0.0),
            "the world boxes overlap on every axis, which is what makes the coarse answer wrong: {:?}",
            first.shared(second)
        );
        assert!(!first.meets(second), "but the boards only meet along their faces");
        assert!(first.meets(board(beside(5.0), 400.0, 400.0, 18.0)), "five millimetres in is an overlap");
    }

    #[test]
    fn a_turned_body_is_measured_by_the_box_around_its_corners() {
        let turn = Placement::IDENTITY
            .with_rotation_euler(crate::EulerAngles {
                z: units::Degrees::from_millidegrees(45_000),
                ..crate::EulerAngles::default()
            })
            .expect("a turn");
        let box_ =
            surface_box(Id::new(1).expect("id"), DVec3::ZERO, DVec3::new(10.0, 2.0, 1.0), turn, SurfaceKind::Panel)
                .expect("a box");
        assert!((box_.min.x + std::f64::consts::SQRT_2).abs() < 1e-9, "was {box_:?}");
        assert!((box_.max.x - 5.0 * std::f64::consts::SQRT_2).abs() < 1e-9, "was {box_:?}");
        assert!((box_.max.y - 6.0 * std::f64::consts::SQRT_2).abs() < 1e-9, "was {box_:?}");
        assert!(!box_.is_shell());
    }

    #[test]
    fn a_probe_meets_the_true_face_of_a_turned_body() {
        // A 10 x 2 board turned 45 degrees: its world box is wider than the board, so a ray along X
        // that meets its world box would report a wall the board does not have.
        let turn = Placement::IDENTITY.with_rotation_euler(crate::EulerAngles {
            z: units::Degrees::from_millidegrees(45_000),
            ..crate::EulerAngles::default()
        });
        let turn = turn.expect("a turn");
        let board =
            surface_box(Id::new(1).expect("id"), DVec3::ZERO, DVec3::new(10.0, 2.0, 1.0), turn, SurfaceKind::Panel)
                .expect("a box");
        // The board's world box is wider than the board: a point can lie inside the box and still be
        // in the free space beside the board, so a ray from it must not meet a face there.
        let beside = DVec3::new(2.0, 8.4, 0.5);
        assert!(board.contains(beside), "the point is inside the world box");
        assert_eq!(board.face_hit(beside, -DVec3::X), None, "but the true board is not in its way");
        let inside = DVec3::new(0.0, 0.0, -5.0);
        let hit = board.face_hit(inside, DVec3::new(0.0, 0.0, 1.0)).expect("the board's own top face is met");
        assert!((hit.distance - 5.0).abs() < 1e-9, "was {hit:?}");
        assert!((hit.normal.dot(DVec3::Z) + 1.0).abs() < 1e-9, "the face looks back down the ray: {hit:?}");
        assert_eq!(hit.axis, 2);
    }
}
