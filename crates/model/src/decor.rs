//! How a node places its material's picture on itself (Y15).
//!
//! A material's record says how much board its picture covers and how its grain runs, and every
//! board cut from that record inherits both. A cabinet is not one board: the door's grain runs along
//! its height while the shelf beside it runs along its width, and a small part shows the same
//! picture at a finer scale than a large one. The mapping is that per-node answer — a scale, an
//! offset and a quarter turn — stored on the node and composed with the record where the texture
//! coordinate is computed, so the picture stays one texture however many boards wear it.
//!
//! The mapping is document data and nothing else: it names no picture, no colour and no material,
//! and a node that carries none draws exactly as it did before the field existed. The geometry and
//! the renderer never store a resolved coordinate (`AGENTS.md` §4.2).

use serde::{Deserialize, Serialize};

use units::Length;

/// Longest scale and largest offset a mapping may name: one kilometre of board.
///
/// A decor scale is a physical length, so the document's own ceiling bounds it; the limit is what a
/// hostile file cannot pass, not what a shop may want.
pub const MAX_DECOR_EXTENT: Length = Length::from_micrometres(1_000_000_000);

/// Why a mapping was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DecorMappingError {
    /// A scale that covers no board, or one past the ceiling.
    #[error("a decor scale of {scale:?} of board must be more than nothing and at most {MAX_DECOR_EXTENT:?}")]
    Scale {
        /// The rejected scale.
        scale: Length,
    },
    /// An offset past the ceiling.
    #[error("a decor offset of {x:?}, {y:?} lies past {MAX_DECOR_EXTENT:?} of board")]
    Offset {
        /// The rejected offset along the node's own X.
        x: Length,
        /// The rejected offset along the node's own Y.
        y: Length,
    },
    /// A quarter-turn count past three, which no surface has.
    #[error("a decor turn of {turns} is past the three quarter turns a picture has")]
    Turns {
        /// The rejected count.
        turns: u8,
    },
}

/// How many quarter turns a node turns its material's picture by, around the surface's normal.
///
/// The same count [`materials::Decor`] and the frame's own decor carry, stated again here because
/// the document may depend on neither: a picture has a grain, and the turn is how a board is cut
/// along it or across it. Four values, because a quarter turn is what a printed picture can be
/// turned by without resampling it into a different scan.
///
/// [`materials::Decor`]: ../materials/index.html
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(try_from = "u8")]
pub struct Turns(u8);

impl Turns {
    /// The picture as its own file has it: its width runs along the node's local `+X`.
    pub const ALONG: Self = Self(0);
    /// A quarter turn: the picture's width runs along the node's local `+Y`.
    pub const ACROSS: Self = Self(1);

    /// `turns` quarter turns, or `None` past three, which is the count a file may name.
    pub const fn new(turns: u8) -> Option<Self> {
        if turns < 4 { Some(Self(turns)) } else { None }
    }

    /// How many quarter turns.
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for Turns {
    type Error = DecorMappingError;

    fn try_from(turns: u8) -> Result<Self, Self::Error> {
        Self::new(turns).ok_or(DecorMappingError::Turns { turns })
    }
}

/// Where a node puts its material's picture on itself: a scale, an offset and a quarter turn (Y15).
///
/// The scale is how much board one picture width spans, so a smaller number makes the grain finer.
/// The offset shifts the picture along the node's own `X` and `Y` by that many millimetres of board,
/// and it is measured in the node's frame rather than the picture's, so the two numbers a card shows
/// are the two axes a person sees. The turn adds to the record's own quarter turns.
///
/// A value is validated where it is built, so a zero scale can reach neither the document nor a
/// frame; a node that carries no mapping falls back to the record's numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(try_from = "DecorMappingWire")]
pub struct DecorMapping {
    scale: Length,
    offset: [Length; 2],
    turns: Turns,
}

impl DecorMapping {
    /// The mapping that spans `scale` of board per picture width, shifted by `offset` and turned by
    /// `turns`.
    ///
    /// Refuses a scale that covers no board, a number past [`MAX_DECOR_EXTENT`] and a turn count a
    /// picture does not have.
    pub fn new(scale: Length, offset: [Length; 2], turns: Turns) -> Result<Self, DecorMappingError> {
        if scale <= Length::ZERO || scale > MAX_DECOR_EXTENT {
            return Err(DecorMappingError::Scale { scale });
        }
        for component in offset {
            if component.micrometres().unsigned_abs() > MAX_DECOR_EXTENT.micrometres().unsigned_abs() {
                return Err(DecorMappingError::Offset { x: offset[0], y: offset[1] });
            }
        }
        Ok(Self { scale, offset, turns })
    }

    /// How much board one picture width spans.
    pub const fn scale(self) -> Length {
        self.scale
    }

    /// The shift of the picture along the node's own `X` and `Y`, in millimetres of board.
    pub const fn offset(self) -> [Length; 2] {
        self.offset
    }

    /// The quarter turns this mapping adds to the record's own.
    pub const fn turns(self) -> Turns {
        self.turns
    }
}

/// The serialized form of [`DecorMapping`]: the current layout, and nothing else.
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct DecorMappingWire {
    scale: Length,
    offset: [Length; 2],
    turns: u8,
}

impl TryFrom<DecorMappingWire> for DecorMapping {
    type Error = DecorMappingError;

    fn try_from(wire: DecorMappingWire) -> Result<Self, Self::Error> {
        let turns = Turns::new(wire.turns).ok_or(DecorMappingError::Turns { turns: wire.turns })?;
        Self::new(wire.scale, wire.offset, turns)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn millimetres(value: i64) -> Length {
        Length::from_millimetres(value).expect("whole millimetres fit a length")
    }

    #[test]
    fn a_mapping_states_the_three_numbers_a_frame_needs() {
        let mapping = DecorMapping::new(millimetres(300), [millimetres(-40), millimetres(15)], Turns::ACROSS)
            .expect("a mapping of a third of a metre, shifted and turned");
        assert_eq!(mapping.scale(), millimetres(300));
        assert_eq!(mapping.offset(), [millimetres(-40), millimetres(15)]);
        assert_eq!(mapping.turns(), Turns::ACROSS);
        assert_eq!(DecorMapping::new(millimetres(1), [Length::ZERO; 2], Turns::ALONG).map(|_| ()), Ok(()));
    }

    #[test]
    fn a_scale_that_covers_no_board_is_refused() {
        assert_eq!(
            DecorMapping::new(Length::ZERO, [Length::ZERO; 2], Turns::ALONG),
            Err(DecorMappingError::Scale { scale: Length::ZERO }),
            "a picture covering nothing has no coordinate"
        );
        assert_eq!(
            DecorMapping::new(millimetres(-600), [Length::ZERO; 2], Turns::ALONG),
            Err(DecorMappingError::Scale { scale: millimetres(-600) }),
            "a negative scale would mirror the grain"
        );
        let absurd = Length::from_micrometres(MAX_DECOR_EXTENT.micrometres().saturating_add(1));
        assert_eq!(
            DecorMapping::new(absurd, [Length::ZERO; 2], Turns::ALONG),
            Err(DecorMappingError::Scale { scale: absurd }),
            "a scale past the document ceiling is refused rather than drawn"
        );
    }

    #[test]
    fn an_offset_past_the_ceiling_and_a_turn_past_three_are_refused() {
        let far = Length::from_micrometres(MAX_DECOR_EXTENT.micrometres().saturating_add(1));
        assert_eq!(
            DecorMapping::new(millimetres(600), [far, Length::ZERO], Turns::ALONG),
            Err(DecorMappingError::Offset { x: far, y: Length::ZERO })
        );
        assert_eq!(Turns::new(4), None, "four quarter turns are the picture again");
        assert_eq!(Turns::try_from(9), Err(DecorMappingError::Turns { turns: 9 }));
    }

    #[test]
    fn a_mapping_is_written_as_its_three_numbers_and_a_bad_one_is_refused_by_the_file() {
        let mapping =
            DecorMapping::new(millimetres(600), [millimetres(10), Length::ZERO], Turns::ACROSS).expect("a mapping");
        let text = serde_json::to_string(&mapping).expect("a mapping serializes");
        assert_eq!(
            text,
            format!(
                "{{\"scale\":{},\"offset\":[{},0],\"turns\":1}}",
                millimetres(600).micrometres(),
                millimetres(10).micrometres()
            )
        );
        let read: DecorMapping = serde_json::from_str(&text).expect("and reads back");
        assert_eq!(read, mapping);
        assert!(
            serde_json::from_str::<DecorMapping>("{\"scale\":0,\"offset\":[0,0],\"turns\":0}").is_err(),
            "a zero scale is a readable refusal, not a coordinate"
        );
        assert!(
            serde_json::from_str::<DecorMapping>("{\"scale\":1,\"offset\":[0,0],\"turns\":7}").is_err(),
            "a turn count past three is refused"
        );
        assert!(
            serde_json::from_str::<DecorMapping>("{\"scale\":1,\"offset\":[0,0],\"turns\":0,\"x\":1}").is_err(),
            "an unknown field is refused"
        );
    }
}
