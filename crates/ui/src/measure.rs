//! The ruler: a measurement held as view state, never a document object (ADR 0118).
//!
//! The atom is deliberately dumb about input: it stores the two points a click gave it and the value
//! between them, and it knows nothing about the camera, the work plane, snapping, the probe or the
//! status bar. The shell resolves a pointer to a snapped point through the one snap resolver (0051)
//! and the one probe (0103) and hands the point here, so the ruler cannot grow a second point path.
//!
//! Three stages: waiting for the first point, waiting for the second while the value follows the
//! pointer, and measured. A click in the measured stage starts a new measurement at that point
//! rather than extending the last one; dropping the atom is Escape and writes nothing, because
//! nothing was written. A length is an integer micrometre `Length` like every other number in the
//! domain, and the value is only ever computed, never stored as floating point.
//!
//! Changes when: the ruler gains a method (an angle, a radius), or the click sequence changes.

use units::Length;

/// Where the ruler stands, which is what the status bar names in words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeasureStage {
    /// Armed with no point yet: the next click places the first one.
    First,
    /// One point placed: the value follows the pointer and the next click finishes it.
    Second,
    /// Two points placed: the value stands until the next click starts a new measurement.
    Measured,
}

/// What one click did to the ruler.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeasureClick {
    /// The click placed the first point of a new measurement.
    Started,
    /// The click placed the second point and fixed the value.
    Finished,
}

/// The live measurement: the first point, the point under the pointer, and whether it is fixed.
///
/// A point is the document's own `[Length; 3]` in world micrometres, so a measurement is the same
/// number whichever camera, view or workstation produced it. The atom holds no scene reference and
/// no revision: it cannot write, and a test drives it without a document.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Measure {
    /// The first clicked point, or `None` while the ruler waits for it.
    first: Option<[Length; 3]>,
    /// The second point: the one under the pointer until a click fixes it.
    current: Option<[Length; 3]>,
    /// Whether the second point is fixed, so a moving pointer leaves the value alone.
    complete: bool,
}

impl Measure {
    /// A ruler waiting for its first point.
    pub const fn new() -> Self {
        Self { first: None, current: None, complete: false }
    }

    /// The stage the ruler is in.
    pub fn stage(&self) -> MeasureStage {
        if self.first.is_none() {
            MeasureStage::First
        } else if self.complete {
            MeasureStage::Measured
        } else {
            MeasureStage::Second
        }
    }

    /// The first point, once it is placed.
    pub const fn first(&self) -> Option<[Length; 3]> {
        self.first
    }

    /// The second point as the shell resolved it: the pointer until a click fixes it.
    pub const fn current(&self) -> Option<[Length; 3]> {
        self.current
    }

    /// Whether the second point is fixed.
    pub const fn is_measured(&self) -> bool {
        self.complete
    }

    /// The length between the two points, or `None` while the first one is missing.
    ///
    /// The value follows the pointer between the clicks and stays put once the second one lands, so
    /// the same length is drawn by the viewport and by the status bar. A point that would overflow
    /// the distance (hostile or corrupted data) answers `None` instead of a clamped number.
    pub fn value(&self) -> Option<Length> {
        distance(self.first?, self.current?)
    }

    /// Follows the pointer while the second point is not fixed.
    ///
    /// A measured ruler ignores the pointer: its value is the one the last click fixed, and moving
    /// the hand must not quietly change a number the person is reading.
    pub fn hover(&mut self, point: [Length; 3]) {
        if !self.complete {
            self.current = Some(point);
        }
    }

    /// Places a point, starting a new measurement when the last one is already fixed.
    pub fn click(&mut self, point: [Length; 3]) -> MeasureClick {
        if self.first.is_none() || self.complete {
            self.first = Some(point);
            self.current = Some(point);
            self.complete = false;
            MeasureClick::Started
        } else {
            self.current = Some(point);
            self.complete = true;
            MeasureClick::Finished
        }
    }

    /// Drops every point: the shell does this when the tool is disarmed.
    pub fn clear(&mut self) {
        self.first = None;
        self.current = None;
        self.complete = false;
    }
}

/// The straight-line distance between two points, in whole micrometres.
///
/// The squares are accumulated in `f64` only after each difference is a checked `i64`, so an
/// overflow is refused instead of wrapped. A sum that is not finite or does not fit a `Length` is
/// not a distance this program can show, so it answers `None`.
fn distance(from: [Length; 3], to: [Length; 3]) -> Option<Length> {
    let squared = from.iter().zip(to.iter()).try_fold(0.0f64, |total, (from, to)| {
        let delta = from.micrometres().checked_sub(to.micrometres())? as f64;
        Some(total + delta * delta)
    })?;
    let micrometres = squared.sqrt();
    if !micrometres.is_finite() || micrometres > i64::MAX as f64 {
        return None;
    }
    Some(Length::from_micrometres(micrometres.round() as i64))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A point in whole millimetres, the way a snapped click arrives.
    fn point(x: i64, y: i64, z: i64) -> [Length; 3] {
        [
            Length::from_millimetres(x).unwrap_or_default(),
            Length::from_millimetres(y).unwrap_or_default(),
            Length::from_millimetres(z).unwrap_or_default(),
        ]
    }

    #[test]
    fn two_clicks_fix_the_distance_and_a_third_starts_again() {
        let mut measure = Measure::new();
        assert_eq!(measure.stage(), MeasureStage::First);
        assert_eq!(measure.value(), None, "nothing is measured before the first point");

        assert_eq!(measure.click(point(0, 0, 0)), MeasureClick::Started);
        assert_eq!(measure.stage(), MeasureStage::Second);
        measure.hover(point(300, 400, 0));
        assert_eq!(measure.value(), Length::from_millimetres(500), "the value follows the pointer");

        assert_eq!(measure.click(point(300, 400, 0)), MeasureClick::Finished);
        assert_eq!(measure.stage(), MeasureStage::Measured);
        assert_eq!(measure.value(), Length::from_millimetres(500), "the click fixes the length");
        assert_eq!(measure.first(), Some(point(0, 0, 0)));

        measure.hover(point(1000, 1000, 1000));
        assert_eq!(measure.value(), Length::from_millimetres(500), "a measured value ignores the pointer");

        assert_eq!(measure.click(point(18, 0, 0)), MeasureClick::Started);
        assert_eq!(measure.stage(), MeasureStage::Second, "the third click starts a new measurement");
        assert_eq!(measure.first(), Some(point(18, 0, 0)));
        assert_eq!(measure.value(), Some(Length::ZERO), "the new value starts at its own point");
    }

    #[test]
    fn a_hostile_distance_is_refused_instead_of_wrapping() {
        let mut measure = Measure::new();
        let far = Length::from_micrometres(i64::MAX);
        let near = Length::from_micrometres(i64::MIN);
        measure.click([far, near, far]);
        measure.click([near, far, near]);
        assert_eq!(measure.value(), None, "a distance that overflows is not a number");
    }

    #[test]
    fn clearing_drops_every_point() {
        let mut measure = Measure::new();
        measure.click(point(1, 2, 3));
        measure.click(point(4, 5, 6));
        measure.clear();
        assert_eq!(measure, Measure::new(), "a cleared ruler is the one that was armed");
        assert_eq!(measure.stage(), MeasureStage::First);
    }
}
