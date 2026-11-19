//! The niche tool's arm: whether the pointer names the empty space, and the atom that holds it.
//!
//! The atom lives in [`crate::void`] and knows only the point and the cell; this module owns the mode
//! transition, so arming the niche leaves every other pointer mode and disarming writes nothing. The
//! fill tool arms the same atom with its own gesture — the space under the pointer is the void it
//! fills — so one measurement serves both and the report row reads the value the fill ghost was built
//! from (ADR 0118).
//!
//! Changes when: the niche is armed or disarmed by another surface, or it stops owning the pointer.

use geom::{Contact, DVec3};
use model::Project;

use super::Editor;
use crate::void::Void;

impl Editor {
    /// The live niche, whether the niche tool or a fill tool is the one that measured it.
    pub const fn void(&self) -> &Void {
        &self.void
    }

    /// Whether the niche tool itself owns the pointer.
    ///
    /// The fill tool holds the same value while it is armed; the difference is that a fill creates a
    /// part on its click and the niche only reads, so the two presses never share a state.
    pub const fn void_armed(&self) -> bool {
        self.void_armed
    }

    /// Arms the niche tool: the pointer names the empty space until Escape or a second press.
    ///
    /// Starting any tool leaves the others, because two armed modes at once is a defect; the niche
    /// writes nothing, so leaving a draft costs no undo entry.
    pub fn arm_void(&mut self) {
        self.leave_pointer_modes();
        self.void_armed = true;
    }

    /// Drops the niche tool and the value it held; the document never changed.
    pub fn cancel_void(&mut self) {
        self.void_armed = false;
        self.void.clear();
    }

    /// Arms the niche tool, or leaves it when it already owns the pointer.
    ///
    /// The ribbon cell and the key both land here, so the two toggle the tool the same way.
    pub fn toggle_void(&mut self) {
        if self.void_armed {
            self.cancel_void();
        } else {
            self.arm_void();
        }
    }

    /// Drops the measured cell, and says whether there was one.
    ///
    /// A pointer that leaves the product names no space; the report row then says what to point at
    /// instead of printing the cell the hand has already left.
    pub fn void_clear(&mut self) -> bool {
        self.void.clear()
    }

    /// Measures the empty space around `point` for whichever tool armed the atom.
    ///
    /// Returns whether the value changed, so the caller can ask for a repaint. A pointer that moved
    /// less than the atom's own step changes nothing; with neither tool armed there is no value to
    /// keep, so the call does nothing at all.
    pub fn void_measure(&mut self, project: &Project, point: DVec3, contact: Option<&dyn Contact>) -> bool {
        if !self.void_armed && self.fill.is_none() {
            return false;
        }
        // A part switched off changes the void without moving the pointer, so the cell a still hand
        // holds is dropped: the atom measures the space the eye now reads (X73).
        let mut changed = false;
        let revision = self.flags.visibility_revision();
        if self.void_revision != revision {
            self.void_revision = revision;
            changed = self.void.clear();
        }
        changed | self.void.measure(project, point, self.flags.sight(), contact)
    }
}
