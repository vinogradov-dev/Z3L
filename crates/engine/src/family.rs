//! Which world a command serves: one word per id, read from its area.
//!
//! A caller asked for a nut must not be handed the tools that make a panel, and a caller asked for a
//! carcass must not compose a body out of primitives. The registry said what a command creates and
//! what it takes, but never which world it belongs to, so the only way to tell was the label — a
//! sentence, not a field (ADR 0185). This module is that field.
//!
//! The answer is *derived*, not declared: the area in front of the first dot names the world, so a
//! new command inherits its family from the module that owns it, and the test that walks the
//! registry fails the day an area has no row here. Four families, one table, no second list.
//!
//! Changes when: a command area is added, or one moves between worlds.

use crate::CommandRegistry;

/// The world a command serves.
///
/// A request picks a family first and then a command from it: two worlds of the editor look alike
/// from the outside — «построить корпус» and «построить гайку» are one verb — and only the family
/// says whether the answer is a product frame with panels or a solid body.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Family {
    /// The product frame, the assemblies and the parts born inside it: `product.*`, `assembly.*`
    /// and `panel.*`.
    Furniture,
    /// Free solid bodies: `object.*` (primitives, extrusions, paths, booleans), `contour.*`, `area.*`
    /// and `mesh.*`.
    Geometry,
    /// The tree every node lives in: rename, move, place, group, delete — `node.*`.
    Tree,
    /// What is drawn and from where: `scene.*`, `camera.*` and `light.*`.
    Scene,
}

impl Family {
    /// Every family, in the order a guide lists them: the two worlds of the program first.
    pub const ALL: [Self; 4] = [Self::Furniture, Self::Geometry, Self::Tree, Self::Scene];

    /// The wire word of the family, which a discovery report prints and a guide repeats.
    pub const fn word(self) -> &'static str {
        match self {
            Self::Furniture => "furniture",
            Self::Geometry => "geometry",
            Self::Tree => "tree",
            Self::Scene => "scene",
        }
    }

    /// One sentence saying what the family is for, in the wire's own vocabulary.
    pub const fn serves(self) -> &'static str {
        match self {
            Self::Furniture => {
                "the product frame, the assemblies and the parts inside it: a carcass, a wardrobe, a shelf, a door"
            }
            Self::Geometry => "a free solid body that is no furniture part: a nut, a bracket, a template, a turned leg",
            Self::Tree => "what already exists: rename, move, place, group, copy and delete a node",
            Self::Scene => "the view of the scene: cameras, lights, the active camera",
        }
    }

    /// The family a command id belongs to, by the area in front of its first dot.
    ///
    /// `None` means the id's area has no row here, which is a defect the registry test reports
    /// rather than a family a caller has to guess.
    pub fn of(id: &str) -> Option<Self> {
        let area = id.split('.').next()?;
        Self::of_area(area)
    }

    /// The family an area name belongs to.
    fn of_area(area: &str) -> Option<Self> {
        match area {
            "panel" | "product" | "assembly" => Some(Self::Furniture),
            "object" | "contour" | "area" | "mesh" => Some(Self::Geometry),
            "node" => Some(Self::Tree),
            "scene" | "camera" | "light" => Some(Self::Scene),
            _ => None,
        }
    }

    /// Every command id of this family, in registry order.
    pub fn commands(self) -> impl Iterator<Item = &'static str> {
        CommandRegistry::new()
            .specs()
            .iter()
            .filter_map(move |spec| if Self::of(spec.id()) == Some(self) { Some(spec.id()) } else { None })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    #[test]
    fn every_registered_command_serves_exactly_one_family() {
        let specs = CommandRegistry::new().specs();
        for spec in specs {
            assert!(Family::of(spec.id()).is_some(), "{} has no family: its area is not in the table", spec.id());
        }
        let counted: usize = Family::ALL.iter().map(|family| family.commands().count()).sum();
        assert_eq!(counted, specs.len(), "the four families cover the registry once, with no command in two");
    }

    #[test]
    fn the_two_worlds_hold_the_commands_a_request_would_use() {
        let furniture: Vec<&str> = Family::Furniture.commands().collect();
        assert!(furniture.contains(&"product.create"));
        assert!(furniture.contains(&"panel.create"));
        assert!(furniture.iter().all(|id| !id.starts_with("object.")), "a body is not a furniture command");

        let geometry: Vec<&str> = Family::Geometry.commands().collect();
        assert!(geometry.contains(&"object.primitive.create"));
        assert!(geometry.contains(&"object.boolean.set"));
        assert!(geometry.iter().all(|id| !id.starts_with("panel.")), "a panel is not a body command");
    }

    #[test]
    fn a_family_is_named_by_the_area_in_front_of_the_dot() {
        assert_eq!(Family::of("panel.create"), Some(Family::Furniture));
        assert_eq!(Family::of("product.envelope.set"), Some(Family::Furniture));
        assert_eq!(Family::of("object.path.extrude"), Some(Family::Geometry));
        assert_eq!(Family::of("contour.union"), Some(Family::Geometry));
        assert_eq!(Family::of("node.rename"), Some(Family::Tree));
        assert_eq!(Family::of("light.create"), Some(Family::Scene));
        assert_eq!(Family::of("nothing.create"), None, "an unknown area has no family");
        assert_eq!(Family::of(""), None);
    }
}
