//! The four sides of a piece of furniture, named the way a drawing names them.
//!
//! A piece has a front, a back, a left and a right, and every dimension a maker quotes is read
//! against them. Two readings follow from that and are written down with the sides because they are
//! the same vocabulary: a spacing between two shelves is the **clear opening** between them, not a
//! step or a spacing on centres, and the depth of a piece **includes its fronts**, because it is what
//! the piece occupies in the room.
//!
//! The frame fixes which side is which: the front is `+Y`, up is `+Z`, so the side the front view
//! shows on the viewer's left is `+X` — the direction `OpeningView::Left` looks from. The model states
//! the rule once, here, and a test in the interface holds [`Side::Left`] to that preset, so no reader
//! has to infer a side from a mirrored picture. `docs/domain/products.md` §2.1 states the three
//! conventions for people.

use geom::DVec3;
use serde::{Deserialize, Serialize};

/// One side of a piece of furniture, in the piece's own frame.
///
/// The names are the drawing's, not the screen's: [`Side::Left`] is the side a person standing in
/// front of the piece sees on their left, and [`Side::Front`] is where the doors and drawers face.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    /// The left-hand side, at `+X`: the front view shows it on the viewer's left.
    Left,
    /// The right-hand side, at `-X`.
    Right,
    /// The front, at `+Y`: the face the doors and drawers are on.
    Front,
    /// The back, at `-Y`: the face a piece stands against the wall with.
    Back,
}

impl Side {
    /// Every side, the left/right pair before the front/back pair.
    pub const ALL: [Self; 4] = [Self::Left, Self::Right, Self::Front, Self::Back];

    /// The outward direction of the side in the piece's own frame.
    ///
    /// This is the one place that answers which way a side faces: `+X` is left and `+Y` is front,
    /// exactly the directions the `Left` and `Front` view presets look from.
    pub const fn axis(self) -> DVec3 {
        match self {
            Self::Left => DVec3::X,
            Self::Right => DVec3::NEG_X,
            Self::Front => DVec3::Y,
            Self::Back => DVec3::NEG_Y,
        }
    }

    /// The side across the piece from this one.
    pub const fn opposite(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
            Self::Front => Self::Back,
            Self::Back => Self::Front,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_left_side_is_plus_x_and_the_front_is_plus_y() {
        // U111: the four sides are the drawing's, and the frame decides which way each faces. The
        // interface test ties `Left` and `Front` to the view presets; this one states the frame.
        assert_eq!(Side::Left.axis(), DVec3::X, "left is +X, seen from the front");
        assert_eq!(Side::Front.axis(), DVec3::Y, "the front is +Y, where the doors face");
    }

    #[test]
    fn the_four_sides_are_two_opposite_pairs() {
        assert_eq!(Side::ALL.len(), 4, "four sides, no fifth");
        for side in Side::ALL {
            assert_eq!(side.opposite().opposite(), side, "{side:?} returns from its opposite");
            assert_eq!(side.opposite().axis(), -side.axis(), "{side:?} faces across itself");
            assert!((side.axis().length() - 1.0).abs() < f64::EPSILON, "{side:?} is a unit direction");
        }
        assert_eq!(Side::Left.opposite(), Side::Right);
        assert_eq!(Side::Front.opposite(), Side::Back);
    }
}
