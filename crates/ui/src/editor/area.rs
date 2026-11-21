//! The area pull's arm: which drawing the pointer reads, and whether the pull owns the next click.
//!
//! The atom lives in [`crate::area`] and knows only the point and the face; this module owns the mode
//! transition and the drawing the mode was armed on. The Extrude cell arms it when a contour set is
//! selected (G4): the pointer then names an area of that drawing, a click takes it, and the shell
//! pulls the height of the body that click created.
//!
//! Starting any other tool drops the arm and the measured face, because two armed modes at once is a
//! defect; the pull writes nothing until its click, so leaving it costs no undo entry.
//!
//! Changes when: the pull stops reading the selected drawing, or a second surface arms it.

use model::{Id, Project};
use text::Message;
use units::Length;

use super::Editor;

impl Editor {
    /// The live area under the pointer, whichever surface armed it.
    pub const fn area(&self) -> &crate::area::Area {
        &self.area
    }

    /// The drawing whose areas the armed pull reads, or `None` when no pull is armed.
    pub const fn area_pull(&self) -> Option<Id> {
        self.area_pull
    }

    /// Arms the area pull on one drawing: the pointer names its areas until Escape or another tool.
    ///
    /// The drawing is the contour set the cell was pressed on, so the pull never has to guess which
    /// node a pointer far from the selection belongs to.
    pub fn arm_area_pull(&mut self, object: Id) {
        self.leave_pointer_modes();
        self.area_pull = Some(object);
    }

    /// Drops the pull and the face it measured; the document never changed.
    pub fn cancel_area_pull(&mut self) {
        self.area_pull = None;
        self.area.clear();
    }

    /// Feeds the pointer's point to the atom, and says whether the highlighted face changed.
    ///
    /// A pull that is not armed reads nothing, so the same call is safe from any frame.
    pub fn area_measure(&mut self, project: &Project, point: [Length; 2]) -> bool {
        let Some(object) = self.area_pull else {
            return false;
        };
        self.area.measure(project, object, point)
    }

    /// The face under the pointer, ready for the command that stores it, or the reason there is none.
    ///
    /// The command names an area by a point, so this is the whole of what a click needs: the drawing,
    /// the point, and the refusal word when the face cannot become a body.
    pub fn area_pick(&self) -> Option<(Id, [Length; 2])> {
        let object = self.area_pull?;
        Some((object, self.area.point()?))
    }

    /// The sentence that says why the face under the pointer cannot be pulled, when it cannot.
    pub fn area_refusal(&self) -> Option<Message> {
        self.area.refusal().map(crate::area::AreaRefusal::words)
    }
}
