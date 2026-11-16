//! The ruler's arm: whether the next click measures, and the atom that holds the two points.
//!
//! The atom lives in [`crate::measure`] and knows only the points; this module owns the mode
//! transition, so arming the ruler leaves every other pointer mode and disarming writes nothing.
//! The shell resolves the pointer through the snap resolver and the probe and feeds the points
//! here, which is why no document, camera or scene type appears in this file (ADR 0118).
//!
//! Changes when: the ruler is armed or disarmed by another surface, or it stops owning the pointer.

use units::Length;

use super::Editor;
use crate::measure::{Measure, MeasureClick};

impl Editor {
    /// The armed ruler, or `None` when the next click selects or does something else.
    pub fn measure(&self) -> Option<&Measure> {
        self.measure.as_ref()
    }

    /// Arms the ruler with no point: the next click places the first one.
    ///
    /// Starting any tool leaves the others, because two armed modes at once is a defect; the ruler
    /// writes nothing, so leaving a draft costs no undo entry.
    pub fn arm_measure(&mut self) {
        self.leave_pointer_modes();
        self.measure = Some(Measure::new());
    }

    /// Drops the ruler and its points; the document never changed.
    pub fn cancel_measure(&mut self) {
        self.measure = None;
    }

    /// Follows the pointer with the live second point, while the ruler is armed.
    ///
    /// A measured value ignores the pointer, so the atom decides, not this method.
    pub fn measure_hover(&mut self, point: [Length; 3]) {
        if let Some(measure) = self.measure.as_mut() {
            measure.hover(point);
        }
    }

    /// Places a clicked point, or starts a new measurement after the last one was fixed.
    ///
    /// Returns what the click did, or `None` when the ruler is not armed.
    pub fn measure_click(&mut self, point: [Length; 3]) -> Option<MeasureClick> {
        self.measure.as_mut().map(|measure| measure.click(point))
    }
}
