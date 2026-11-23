//! The three panel types a maker names, and the frame each one stands in.
//!
//! A panel's local frame is the same for every type: `X` is its length, `Y` its width and `Z` its
//! thickness, and the thickness is the material's number rather than the caller's. What a type
//! decides is which way that frame stands in the product, and which class the part plays — so a
//! caller that says «стойка» gets an upright board without turning a quaternion by hand (X117).
//!
//! The rotations are the ones the fill tool already measured ([`crate::Fill`] and the stance it
//! drives, ADR 0105, ADR 0082). This type is the adapter between the maker's word and that table,
//! so one caller — the typed create, the void fill or a later points-and-contour create — cannot
//! disagree about where a board's zero corner is.

use geom::DQuat;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{ClassCode, Fill};

/// A standard panel: the upright, the horizontal and the board that stands in the frontal plane.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PanelType {
    /// A horizontal board: its length runs along the product's `X`, its width along `Y`, and its
    /// thickness up `Z`.
    Shelf,
    /// An upright board: its length runs up, its width along the product's depth, and its thickness
    /// across the product's `X`.
    Post,
    /// A board in the frontal plane: its length runs up, its width across the product, and its
    /// thickness along the product's depth.
    Wall,
}

impl PanelType {
    /// Every type, in the order the picker and the guide list them.
    pub const ALL: [Self; 3] = [Self::Shelf, Self::Post, Self::Wall];

    /// The word a caller sends and the guide prints.
    pub const fn word(self) -> &'static str {
        match self {
            Self::Shelf => "shelf",
            Self::Post => "post",
            Self::Wall => "wall",
        }
    }

    /// The axis pair the type fills, which is what its class's traits carry.
    pub const fn fill(self) -> Fill {
        match self {
            Self::Shelf => Fill::WidthAndDepth,
            Self::Post => Fill::HeightAndDepth,
            Self::Wall => Fill::WidthAndHeight,
        }
    }

    /// The registry code of the class a part of this type plays.
    ///
    /// `10.11` horizontal, `10.10` upright and `10.12` back panel, the three rows
    /// `docs/domain/classes.md` names for the three fills. A registry that lost one of them answers
    /// `None` rather than a guessed code.
    pub const fn code(self) -> u32 {
        match self {
            Self::Shelf => 101_100,
            Self::Post => 101_000,
            Self::Wall => 101_200,
        }
    }

    /// The class the type plays, or `None` when the registry does not carry its row.
    pub fn class(self) -> Option<ClassCode> {
        ClassCode::from_code(self.code())
    }

    /// The rotation that stands the panel's local frame in the product.
    ///
    /// The length axis never points down: a board is born standing on its base face, which is what
    /// keeps its zero corner at the bottom of the part instead of hanging the part under it (X117).
    pub fn rotation(self) -> DQuat {
        crate::project::fill::stance_rotation(self.fill())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_type_stands_its_length_axis_up_or_level_and_never_down() {
        // The local `X` is the length and `+Z` is up: a shelf lays its length along `+X`, and both
        // upright types stand theirs along `+Z`. No type points its length at `−Z`, which is the
        // bug the typed create exists to prevent (X117, ADR 0082).
        for panel in PanelType::ALL {
            let length = panel.rotation() * geom::DVec3::X;
            let upwards = match panel {
                PanelType::Shelf => (length - geom::DVec3::X).length() < 1.0e-12,
                PanelType::Post | PanelType::Wall => (length - geom::DVec3::Z).length() < 1.0e-12,
            };
            assert!(upwards, "{} turns its length axis to {length:?}", panel.word());
        }
    }

    #[test]
    fn every_type_names_a_class_the_registry_carries_and_the_fill_it_fills() {
        for panel in PanelType::ALL {
            let class = panel.class().expect("the registry carries the type's row");
            assert_eq!(class.value(), panel.code(), "{}", panel.word());
            let traits = class.row().and_then(|row| row.traits()).expect("the row has traits");
            assert_eq!(traits.fill(), Some(panel.fill()), "{} fills its own pair", panel.word());
        }
    }

    #[test]
    fn a_type_serializes_as_its_word() {
        assert_eq!(serde_json::to_value(PanelType::Post).expect("a word"), serde_json::json!("post"));
        assert_eq!(serde_json::from_value::<PanelType>(serde_json::json!("wall")).expect("a type"), PanelType::Wall);
    }
}
