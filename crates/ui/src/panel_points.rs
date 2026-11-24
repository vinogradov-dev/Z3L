//! The board a maker draws by points in space: the clicks it collects and the figure they name.
//!
//! Two methods share one draft (U126). The three-point method reads the first two clicks as one edge
//! and the third as the width, so the fourth corner is computed and the board is a standard
//! rectangle that commits on its third click; the ring method takes as many points as the maker
//! places, projects them onto the plane the first three span, and is closed by the card's finish.
//! The points are the snap resolver's own world micrometres, so the first click may stand on a face,
//! an edge or a corner and the plane is never chosen before it.
//!
//! Nothing here writes a document: the draft holds the points, the class a maker picked for the part
//! and the refusal of the last click, and `Editor::commit_points` is the one command that stores the
//! board. A refused point is dropped again, so the shape already drawn stays editable.
//!
//! Refuses a point that repeats the one before it, a chain longer than a ring allows, three points
//! that span no plane and a figure the panel's own rules reject; each refusal is a [`Message`] the
//! tool card shows, in the person's language.
//!
//! Changes when: a by-points method, the plane rule or what the draft refuses changes.

use model::{
    ClassCode, Contour, MAX_CONTOUR_SEGMENTS, PanelNominalSize, PointsError, PointsPlane, board_from_three,
    ring_from_points,
};
use text::Message;
use units::Length;

/// The two methods of the by-points cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelPoints {
    /// Three points: the first two are one edge, the third fixes the width, and the board is a
    /// standard rectangle that commits on the third click.
    Three,
    /// Any number of points: the first three fix the plane, every later point is projected onto it,
    /// and the card's finish (or Enter) closes the ring as it stands.
    Ring,
}

impl PanelPoints {
    /// The words the method list draws.
    pub const fn label(self) -> Message {
        match self {
            Self::Three => Message::PanelPointsThree,
            Self::Ring => Message::PanelPointsMany,
        }
    }

    /// Whether the method commits on the click that completes it, without a finish of its own.
    ///
    /// The three-point board is one gesture of three clicks; the ring is open-ended, so only the
    /// maker can say when it is done — a count above four reaches the document that way alone.
    pub const fn commits_on_click(self) -> bool {
        matches!(self, Self::Three)
    }
}

/// The figure a finished draft names.
#[derive(Clone, Debug, PartialEq)]
pub enum PointsFigure {
    /// A standard rectangle board: the plane it lies in and its two sides.
    Rect {
        /// The plane the three points span, as the frame the board is born in.
        plane: PointsPlane,
        /// The board's sides on that plane.
        size: PanelNominalSize,
    },
    /// A contour board: the plane the first three points span and the ring on it.
    Ring {
        /// The plane the first three points span, as the frame the board is born in.
        plane: PointsPlane,
        /// The closed ring in the plane's own coordinates.
        contour: Contour,
    },
}

/// Why a click or a figure was refused.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum PointsDraftError {
    /// The click repeats the previous point, so the segment would have no length.
    #[error("the point repeats the previous one")]
    DuplicatePoint,
    /// The chain already holds the largest number of points a ring can take.
    #[error("the ring holds at most {MAX_CONTOUR_SEGMENTS} points")]
    Full,
    /// The three-point method already holds every point it needs.
    #[error("the three-point board is complete")]
    Complete,
    /// The points name no board.
    #[error(transparent)]
    Points(#[from] PointsError),
}

impl PointsDraftError {
    /// The sentence the tool card shows, in the language the caller draws in.
    pub const fn message(&self) -> Message {
        match self {
            Self::DuplicatePoint => Message::PanelPointsDuplicate,
            Self::Full => Message::PanelPointsFull,
            Self::Complete => Message::PanelPointsComplete,
            Self::Points(PointsError::Collinear | PointsError::ZeroSide) => Message::PanelPointsCollinear,
            Self::Points(PointsError::NotEnoughPoints { .. }) => Message::PanelPointsNeedThree,
            Self::Points(_) => Message::PanelPointsUnusable,
        }
    }
}

/// The points of one board being drawn, the method that reads them and the class it will carry.
#[derive(Clone, Debug, PartialEq)]
pub struct PointsDraft {
    method: PanelPoints,
    points: Vec<[Length; 3]>,
    class: Option<ClassCode>,
    refusal: Option<Message>,
}

impl PointsDraft {
    /// An empty draft of `method`: no point, no class, no refusal.
    pub fn new(method: PanelPoints) -> Self {
        Self { method, points: Vec::new(), class: None, refusal: None }
    }

    /// Which of the two tools owns the clicks.
    pub const fn method(&self) -> PanelPoints {
        self.method
    }

    /// The points placed so far, in the order they were clicked.
    pub fn points(&self) -> &[[Length; 3]] {
        &self.points
    }

    /// The class the maker chose for the board, or `None` when its plane decides.
    pub const fn class(&self) -> Option<ClassCode> {
        self.class
    }

    /// Chooses the class the board will carry; `None` gives the role back to the plane.
    pub fn set_class(&mut self, class: Option<ClassCode>) {
        self.class = class;
    }

    /// The refusal of the last click, until the next point lands or the tool is dropped.
    pub const fn refusal(&self) -> Option<Message> {
        self.refusal
    }

    /// Whether the draft holds enough points for its method to commit.
    pub fn is_finished(&self) -> bool {
        self.points.len() >= 3
    }

    /// Whether the three-point method has taken its last click.
    pub fn is_complete(&self) -> bool {
        self.method.commits_on_click() && self.points.len() >= 3
    }

    /// Adds a clicked point, or refuses it and says why.
    ///
    /// The plane is fixed by the third point and checked on that click, so three points on one line
    /// are refused before the maker places a fourth; a ring that only crosses once it is closed is
    /// judged at its commit, because a chain still being drawn may pass itself on the way.
    pub fn push(&mut self, point: [Length; 3]) -> Result<(), PointsDraftError> {
        if self.is_complete() {
            return self.refuse(PointsDraftError::Complete);
        }
        if self.points.len() >= MAX_CONTOUR_SEGMENTS {
            return self.refuse(PointsDraftError::Full);
        }
        if self.points.last() == Some(&point) {
            return self.refuse(PointsDraftError::DuplicatePoint);
        }
        self.points.push(point);
        if self.points.len() == 3 {
            let [first, second, third] = self.first_three();
            if let Err(error) = PointsPlane::from_three(first, second, third) {
                self.points.pop();
                return self.refuse(PointsDraftError::Points(error));
            }
        }
        self.refusal = None;
        Ok(())
    }

    /// The figure the points name, or why they name none.
    ///
    /// This is the ghost the viewport draws and the board the commit stores, from one reading: the
    /// rectangle is computed from its three points and the ring from every point projected onto the
    /// plane, so the picture and the part cannot disagree (ADR 0122).
    pub fn figure(&self) -> Result<PointsFigure, PointsDraftError> {
        if self.points.len() < 3 {
            return Err(PointsDraftError::Points(PointsError::NotEnoughPoints { got: self.points.len() }));
        }
        match self.method {
            PanelPoints::Three => {
                let [first, second, third] = self.first_three();
                let board = board_from_three(first, second, third)?;
                Ok(PointsFigure::Rect { plane: board.plane, size: board.size })
            }
            PanelPoints::Ring => {
                let ring = ring_from_points(&self.points)?;
                Ok(PointsFigure::Ring { plane: ring.plane, contour: ring.contour })
            }
        }
    }

    /// The first three points, which are the ones that fix the plane.
    ///
    /// A caller that has checked [`Self::is_finished`] is the rule here; a short draft repeats its
    /// first point rather than inventing one, and the figure's own check refuses it.
    fn first_three(&self) -> [[Length; 3]; 3] {
        let first = self.points.first().copied().unwrap_or_default();
        let second = self.points.get(1).copied().unwrap_or(first);
        let third = self.points.get(2).copied().unwrap_or(second);
        [first, second, third]
    }

    /// Records the sentence and returns the refusal the caller reports.
    fn refuse<T>(&mut self, error: PointsDraftError) -> Result<T, PointsDraftError> {
        self.refusal = Some(error.message());
        Err(error)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    fn point(x: i64, y: i64, z: i64) -> [Length; 3] {
        [x, y, z].map(|value| Length::from_millimetres(value).unwrap_or_default())
    }

    fn rect() -> PointsDraft {
        let mut draft = PointsDraft::new(PanelPoints::Three);
        draft.push(point(0, 0, 0)).expect("the first point");
        draft.push(point(600, 0, 0)).expect("the second point");
        draft
    }

    #[test]
    fn the_third_click_completes_the_rectangle_and_sets_its_width() {
        let mut draft = rect();
        assert!(!draft.is_complete(), "two points are not a board");
        draft.push(point(0, 400, 0)).expect("the third point");
        assert!(draft.is_complete(), "the third click finishes the rectangle");
        let PointsFigure::Rect { size, .. } = draft.figure().expect("a rectangle") else {
            panic!("the three-point method draws a rectangle");
        };
        assert_eq!(size.length(), Length::from_millimetres(600).expect("600 mm"));
        assert_eq!(size.width(), Length::from_millimetres(400).expect("400 mm"));
        assert_eq!(draft.push(point(0, 100, 0)), Err(PointsDraftError::Complete), "a fourth click changes nothing");
    }

    #[test]
    fn three_points_on_one_line_are_refused_and_the_draft_keeps_two() {
        let mut draft = rect();
        assert_eq!(draft.push(point(300, 0, 0)), Err(PointsDraftError::Points(PointsError::Collinear)));
        assert_eq!(draft.points().len(), 2, "the refused point is not kept");
        assert_eq!(draft.refusal(), Some(Message::PanelPointsCollinear), "and the card has the sentence");
        // A point off that line is accepted and clears the sentence.
        draft.push(point(0, 400, 0)).expect("an off-line point");
        assert_eq!(draft.refusal(), None);
        assert!(draft.is_complete());
    }

    #[test]
    fn a_repeated_point_is_refused_with_its_own_sentence() {
        let mut draft = rect();
        assert_eq!(draft.push(point(600, 0, 0)), Err(PointsDraftError::DuplicatePoint));
        assert_eq!(draft.refusal(), Some(Message::PanelPointsDuplicate));
    }

    #[test]
    fn the_ring_takes_n_points_and_never_finishes_by_itself() {
        let mut draft = PointsDraft::new(PanelPoints::Ring);
        for corner in [point(0, 0, 0), point(600, 0, 0), point(600, 400, 0), point(0, 400, 0)] {
            draft.push(corner).expect("a corner of the ring");
            assert!(!draft.is_complete(), "a ring is finished by its maker, not by a count");
        }
        let PointsFigure::Ring { contour, .. } = draft.figure().expect("a ring") else {
            panic!("the ring method draws a contour");
        };
        assert_eq!(contour.segments().len(), 4, "four points make four sides");
        assert!(draft.is_finished(), "and the draft can be committed");
    }

    #[test]
    fn an_unfinished_draft_names_no_figure() {
        let draft = rect();
        assert_eq!(draft.figure(), Err(PointsDraftError::Points(PointsError::NotEnoughPoints { got: 2 })));
        assert_eq!(
            PointsDraft::new(PanelPoints::Ring).figure(),
            Err(PointsDraftError::Points(PointsError::NotEnoughPoints { got: 0 }))
        );
    }
}
