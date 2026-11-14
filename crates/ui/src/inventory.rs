//! The inventory of capabilities: one row per command, and the places it appears in.
//!
//! The interface keeps three registers that do not know each other — the engine's registry, the
//! ribbon map and the palette's parameter builders — so a capability could be added with no home,
//! or with two homes that disagree. This module is the fourth register that ends that: a row is
//! keyed by the command's stable id and says where the capability appears, who it applies to, its
//! default key, its artwork and its words (ADR 0117).
//!
//! A surface filters this table instead of writing its own list: the palette shows every row that
//! has a place, and the context menu (`crate::node_actions`) shows the rows whose [`Applies`] matches
//! the node under the pointer, on the tree row and in the viewport alike (U92). The ribbon stays the
//! curated lexicon — a cell names a *method* (`sketch.create.line`) and draws its own words, because
//! two methods can share one command — so the ribbon reads this table for the rows that are one
//! button and keeps its cells for the rest.
//!
//! What the table does not carry is behaviour: a row names an id, a message, an icon and a key, and
//! the command itself stays in `furnicraft-engine`. The gate in `cargo xtask ui-rules` fails when a
//! registered command has no row, when a row names an id no command registered, when an id carries
//! two labels or two keys, when an `Api` row claims another place, and when a live ribbon cell has
//! no artwork.

use furnicraft_text::Message;

use crate::icons::Icon;

/// One surface a capability appears in, or the one place that says it appears nowhere.
///
/// The list is fixed by ADR 0117. A row carries the places that are true today; a place is added
/// when the surface starts to draw the capability, never in advance, so a row is a measurement
/// rather than a wish.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    /// The ribbon's curated map has a live cell that commits it.
    Ribbon,
    /// The context menu offers it for a matching selection (U92).
    ContextMenu,
    /// The armed tool shows and edits its parameters in the tool card (U97).
    ToolCard,
    /// The palette can build its parameters and dispatch it.
    Palette,
    /// An inspector card, or the mode that card opens, dispatches it.
    Card,
    /// No surface draws it: it is meant for the API, the CLI and the assistant.
    Api,
}

/// Who a capability applies to, the column ADR 0117 calls the context.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Applies {
    /// It acts on the node under the pointer or on the selection, so a surface offers it only when
    /// a node of that kind is there.
    Context(ActsOn),
    /// It is a general action: a creation, a view toggle or a document-wide operation.
    Always,
}

/// The node a capability acts on.
///
/// The column is declared for every contextual row, not only for the ones a context menu draws
/// today, because the card (U93) and the tool card (U97) filter the same table by the node in front
/// of the person, and a second list of "what this acts on" is the defect ADR 0117 ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActsOn {
    /// Any node except the root frame, which is a frame rather than an object.
    Node,
    /// Two or more selected nodes: a batch the card and the palette offer once.
    Selection,
    /// A container: a group, a component or a product frame.
    Container,
    /// A panel, and only a panel.
    Panel,
    /// A mesh object: a primitive, a path, an extrusion or a collapsed body.
    Object,
    /// A camera the document owns.
    Camera,
    /// A light the document owns.
    Light,
}

/// One capability: the id it is known by, and what every surface needs to draw and run it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    /// The stable id: a command's id, or an atom's id when the mechanism has no command (0118).
    pub id: &'static str,
    /// The words the interface prints, which is the command's own label.
    pub label: Message,
    /// Where the capability appears; [`Place::Api`] is exclusive.
    pub places: &'static [Place],
    /// Whether it acts on a selection.
    pub applies: Applies,
    /// The default key, in the spelling the interface prints (`Ctrl+S`, `Delete`).
    pub key: Option<&'static str>,
    /// The artwork of the one button that draws it, or `None` when it has no button of its own.
    pub icon: Option<Icon>,
}

/// What a node is, for the one question a surface asks the table: does this capability act on it?
///
/// It is the table's vocabulary rather than a module's, because the context menu, the card and the
/// tool card all filter the inventory and must name the same kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    /// The world frame: a frame rather than an object, so only view entries act on it.
    Root,
    Panel,
    Object,
    /// A group, a component or a product frame; the three are told apart by the node's own flags.
    Container,
    Camera,
    Light,
    /// No node under the pointer.
    Empty,
}

impl NodeKind {
    /// Whether the kind is a node an edit command can act on.
    pub fn is_node(self) -> bool {
        !matches!(self, NodeKind::Root | NodeKind::Empty)
    }
}

impl Row {
    /// Whether the capability appears in `place`.
    pub fn has(&self, place: Place) -> bool {
        self.places.contains(&place)
    }

    /// Whether a menu offers this capability for a node of `kind`.
    ///
    /// The selection and the container are separate columns rather than node kinds: a batch is
    /// offered only while two or more nodes are selected, and only a menu knows that count.
    pub fn offers(&self, kind: NodeKind, many_selected: bool) -> bool {
        match self.applies {
            Applies::Always => false,
            Applies::Context(ActsOn::Node) => kind.is_node(),
            Applies::Context(ActsOn::Selection) => kind.is_node() && many_selected,
            Applies::Context(ActsOn::Container) => kind == NodeKind::Container,
            Applies::Context(ActsOn::Panel) => kind == NodeKind::Panel,
            Applies::Context(ActsOn::Object) => kind == NodeKind::Object,
            Applies::Context(ActsOn::Camera) => kind == NodeKind::Camera,
            Applies::Context(ActsOn::Light) => kind == NodeKind::Light,
        }
    }

    /// Whether any surface draws it. An [`Place::Api`] row is reached by an id, never by a click.
    pub fn on_a_surface(&self) -> bool {
        self.places.iter().any(|place| *place != Place::Api)
    }

    /// Whether one button draws it, which is what the artwork column records.
    pub fn draws_a_button(&self) -> bool {
        self.icon.is_some()
    }
}

/// The inventory: one row per registered command.
///
/// The order is the registry's own, so a reader can walk the two lists side by side. The three
/// rows that end in [`Place::Api`] are the orphan ids of ADR 0117, and each says in the audit note
/// (`plan/notes/U94.md`) why it has no surface.
pub const ROWS: &[Row] = &[
    Row {
        id: "panel.create",
        label: Message::CmdCreatePanel,
        places: &[Place::Ribbon, Place::Palette],
        applies: Applies::Always,
        key: None,
        icon: None,
    },
    Row {
        id: "panel.contour.create",
        label: Message::CmdCreateContourPanel,
        places: &[Place::Api],
        applies: Applies::Always,
        key: None,
        icon: None,
    },
    Row {
        id: "panel.outline.set",
        label: Message::CmdSetPanelOutline,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Panel),
        key: None,
        icon: None,
    },
    Row {
        id: "panel.outline.to_contour",
        label: Message::CmdBakePanelOutline,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Panel),
        key: None,
        icon: None,
    },
    Row {
        id: "assembly.create",
        label: Message::CmdCreateAssembly,
        places: &[Place::Ribbon, Place::Palette],
        applies: Applies::Always,
        key: None,
        icon: Some(crate::icons::scene::ASSEMBLY),
    },
    Row {
        id: "product.create",
        label: Message::CmdCreateProduct,
        places: &[Place::Ribbon],
        applies: Applies::Always,
        key: None,
        icon: Some(crate::icons::scene::PRODUCT),
    },
    Row {
        id: "product.envelope.set",
        label: Message::CmdSetProductEnvelope,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Container),
        key: None,
        icon: None,
    },
    Row {
        id: "panel.rename",
        label: Message::CmdRenamePanel,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Panel),
        key: None,
        icon: None,
    },
    Row {
        id: "node.rename",
        label: Message::CmdRename,
        places: &[Place::Palette, Place::Card],
        applies: Applies::Context(ActsOn::Node),
        key: None,
        icon: None,
    },
    Row {
        id: "panel.resize",
        label: Message::CmdResizePanel,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Panel),
        key: None,
        icon: None,
    },
    Row {
        id: "panel.material.set",
        label: Message::CmdSetPanelMaterial,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Panel),
        key: None,
        icon: None,
    },
    Row {
        id: "panel.edge_banding.set",
        label: Message::CmdSetEdgeBanding,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Panel),
        key: None,
        icon: None,
    },
    Row {
        id: "panel.edge_banding.clear",
        label: Message::CmdClearEdgeBanding,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Panel),
        key: None,
        icon: None,
    },
    Row {
        id: "panel.corner_banding.set",
        label: Message::CmdSetCornerBanding,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Panel),
        key: None,
        icon: None,
    },
    Row {
        id: "panel.corner_banding.clear",
        label: Message::CmdClearCornerBanding,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Panel),
        key: None,
        icon: None,
    },
    Row {
        id: "panel.ring_banding.set",
        label: Message::CmdSetContourBanding,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Panel),
        key: None,
        icon: None,
    },
    Row {
        id: "panel.ring_banding.clear",
        label: Message::CmdClearContourBanding,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Panel),
        key: None,
        icon: None,
    },
    Row {
        id: "panel.corner.set",
        label: Message::CmdSetCornerFeature,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Panel),
        key: None,
        icon: None,
    },
    Row {
        id: "panel.corner.clear",
        label: Message::CmdClearCornerFeature,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Panel),
        key: None,
        icon: None,
    },
    Row {
        id: "panel.bow.set",
        label: Message::CmdSetEdgeBow,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Panel),
        key: None,
        icon: None,
    },
    Row {
        id: "panel.bow.clear",
        label: Message::CmdClearEdgeBow,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Panel),
        key: None,
        icon: None,
    },
    Row {
        id: "panel.bevel.set",
        label: Message::CmdSetBevel,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Panel),
        key: None,
        icon: None,
    },
    Row {
        id: "panel.bevel.clear",
        label: Message::CmdClearBevel,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Panel),
        key: None,
        icon: None,
    },
    Row {
        id: "panel.machining.add",
        label: Message::CmdAddMachining,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Panel),
        key: None,
        icon: None,
    },
    Row {
        id: "panel.machining.update",
        label: Message::CmdUpdateMachining,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Panel),
        key: None,
        icon: None,
    },
    Row {
        id: "panel.machining.remove",
        label: Message::CmdRemoveMachining,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Panel),
        key: None,
        icon: None,
    },
    Row {
        id: "panel.machining.move",
        label: Message::CmdMoveMachining,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Panel),
        key: None,
        icon: None,
    },
    Row {
        id: "node.delete",
        label: Message::CmdDelete,
        places: &[Place::ContextMenu, Place::Palette, Place::Card],
        applies: Applies::Context(ActsOn::Node),
        key: Some("Delete"),
        icon: None,
    },
    Row {
        id: "node.delete.many",
        label: Message::CmdDeleteSelection,
        places: &[Place::Palette],
        applies: Applies::Context(ActsOn::Selection),
        key: None,
        icon: None,
    },
    Row {
        id: "node.duplicate",
        label: Message::CmdDuplicate,
        places: &[Place::ContextMenu, Place::Palette, Place::Card],
        applies: Applies::Context(ActsOn::Node),
        key: None,
        icon: None,
    },
    Row {
        id: "node.group",
        label: Message::CmdGroup,
        places: &[Place::ContextMenu, Place::Palette, Place::Card],
        applies: Applies::Context(ActsOn::Selection),
        key: None,
        icon: None,
    },
    Row {
        id: "node.ungroup",
        label: Message::CmdUngroup,
        places: &[Place::ContextMenu, Place::Palette, Place::Card],
        applies: Applies::Context(ActsOn::Container),
        key: None,
        icon: None,
    },
    Row {
        id: "node.placement.set",
        label: Message::CmdPlace,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Node),
        key: None,
        icon: None,
    },
    Row {
        id: "node.placement.world_zero",
        label: Message::CmdPlaceAtWorldZero,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Node),
        key: None,
        icon: None,
    },
    Row {
        id: "node.placement.set_many",
        label: Message::CmdPlaceSelection,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Selection),
        key: None,
        icon: None,
    },
    Row {
        id: "node.pivot.set",
        label: Message::CmdSetPivot,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Node),
        key: None,
        icon: None,
    },
    Row {
        id: "node.pivot.center",
        label: Message::CmdCenterPivot,
        places: &[Place::Palette, Place::Card],
        applies: Applies::Context(ActsOn::Node),
        key: None,
        icon: None,
    },
    Row {
        id: "node.origin.rebase",
        label: Message::CmdRebaseOrigin,
        places: &[Place::Api],
        applies: Applies::Context(ActsOn::Node),
        key: None,
        icon: None,
    },
    Row {
        id: "node.property.set",
        label: Message::CmdSetProperty,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Node),
        key: None,
        icon: None,
    },
    Row {
        id: "node.property.set_many",
        label: Message::CmdSetProperties,
        places: &[Place::Api],
        applies: Applies::Context(ActsOn::Node),
        key: None,
        icon: None,
    },
    Row {
        id: "node.property.clear",
        label: Message::CmdClearProperty,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Node),
        key: None,
        icon: None,
    },
    Row {
        id: "node.component.convert_to_group",
        label: Message::CmdConvertToGroup,
        places: &[Place::ContextMenu, Place::Palette, Place::Card],
        applies: Applies::Context(ActsOn::Container),
        key: None,
        icon: None,
    },
    Row {
        id: "object.primitive.create",
        label: Message::CmdCreatePrimitive,
        places: &[Place::Ribbon],
        applies: Applies::Always,
        key: None,
        icon: None,
    },
    Row {
        id: "object.primitive.replace",
        label: Message::CmdReplacePrimitive,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Object),
        key: None,
        icon: None,
    },
    Row {
        id: "object.extrude.create",
        label: Message::CmdCreateExtrusion,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Object),
        key: None,
        icon: None,
    },
    Row {
        id: "object.extrude.height.set",
        label: Message::CmdSetExtrusionHeight,
        places: &[Place::Ribbon],
        applies: Applies::Context(ActsOn::Object),
        key: None,
        icon: None,
    },
    Row {
        id: "object.contour.replace",
        label: Message::CmdReplaceContour,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Object),
        key: None,
        icon: None,
    },
    Row {
        id: "object.path.create",
        label: Message::CmdCreatePath,
        places: &[Place::Ribbon],
        applies: Applies::Always,
        key: None,
        icon: None,
    },
    Row {
        id: "object.path.extrude",
        label: Message::CmdExtrudePath,
        places: &[Place::Ribbon],
        applies: Applies::Context(ActsOn::Object),
        key: None,
        icon: Some(crate::icons::bodies::EXTRUDE),
    },
    Row {
        id: "object.path.replace",
        label: Message::CmdReplacePath,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Object),
        key: None,
        icon: None,
    },
    Row {
        id: "object.collapse",
        label: Message::CmdCollapseToMesh,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Object),
        key: None,
        icon: None,
    },
    Row {
        id: "mesh.vertices.move",
        label: Message::CmdMoveVertices,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Object),
        key: None,
        icon: None,
    },
    Row {
        id: "camera.create",
        label: Message::CmdCreateCamera,
        places: &[Place::Ribbon, Place::Palette, Place::Card],
        applies: Applies::Always,
        key: None,
        icon: Some(crate::icons::scene::CAMERA),
    },
    Row {
        id: "light.create",
        label: Message::CmdCreateLight,
        places: &[Place::Ribbon, Place::Palette, Place::Card],
        applies: Applies::Always,
        key: None,
        icon: Some(crate::icons::scene::LIGHT),
    },
    Row {
        id: "scene.active_camera.set",
        label: Message::CmdSetActiveCamera,
        places: &[Place::Palette, Place::Card],
        applies: Applies::Context(ActsOn::Camera),
        key: None,
        icon: None,
    },
    Row {
        id: "camera.projection.set",
        label: Message::CmdSetCameraProjection,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Camera),
        key: None,
        icon: None,
    },
    Row {
        id: "light.params.set",
        label: Message::CmdSetLightParameters,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Light),
        key: None,
        icon: None,
    },
];

/// The row a command id names, if the inventory knows it.
pub fn row(id: &str) -> Option<&'static Row> {
    ROWS.iter().find(|row| row.id == id)
}

/// The atom ids a row may be keyed by when the mechanism has no command (ADR 0118).
///
/// An atom is a mechanism two tools need — the snap resolver, the probe, the measure — and it gets
/// its own row here when its first surface lands (U98, U99). The gate accepts an atom id as a key
/// only because it is written down here, so a typo in a command id is still a failure.
pub const ATOMS: &[&str] = &[];

/// Every row that appears in `place`.
pub fn in_place(place: Place) -> impl Iterator<Item = &'static Row> {
    ROWS.iter().filter(move |row| row.has(place))
}

#[cfg(test)]
mod tests {
    use super::*;
    use furnicraft_engine::CommandRegistry;
    use furnicraft_text::Lang;
    use std::collections::BTreeMap;

    #[test]
    fn every_registered_command_has_one_row() {
        let registry = CommandRegistry::new();
        for spec in registry.specs() {
            let row = row(spec.id()).unwrap_or_else(|| panic!("`{}` has no inventory row", spec.id()));
            assert_eq!(row.label, spec.label(), "`{}` prints two different labels", spec.id());
        }
        assert_eq!(ROWS.len(), registry.specs().len(), "the inventory has a row for something else");
    }

    #[test]
    fn no_row_appears_twice() {
        let mut seen = BTreeMap::new();
        for row in ROWS {
            assert!(seen.insert(row.id, row.places).is_none(), "`{}` has two rows", row.id);
        }
    }

    #[test]
    fn the_api_is_the_one_place_that_excludes_the_others() {
        for row in ROWS {
            assert!(!row.places.is_empty(), "`{}` appears nowhere", row.id);
            if row.has(Place::Api) {
                assert_eq!(row.places, &[Place::Api], "`{}` claims the API and a surface", row.id);
                assert!(!row.on_a_surface(), "`{}` is drawn and declared API-only", row.id);
            } else {
                assert!(row.on_a_surface(), "`{}` has a place list with no surface", row.id);
            }
        }
    }

    #[test]
    fn a_key_and_an_icon_belong_to_one_row_each() {
        let mut keys = BTreeMap::new();
        let mut icons = BTreeMap::new();
        for row in ROWS {
            if let Some(key) = row.key {
                assert!(keys.insert(key, row.id).is_none(), "`{key}` is bound to two commands");
            }
            if let Some(icon) = row.icon {
                assert!(icons.insert(icon.uri(), row.id).is_none(), "one drawing names two commands");
            }
        }
    }

    #[test]
    fn the_palette_runs_exactly_the_commands_it_builds_parameters_for() {
        // The thirteen ids `Palette::params_for` answers for — twelve with parameters and rename
        // with its refusal. The table is the record of that set, so a builder that grows or shrinks
        // has to change the column with it (U94).
        let answered = in_place(Place::Palette).count();
        assert_eq!(answered, 13, "the palette row count moved");
        for row in in_place(Place::Palette) {
            assert!(!row.has(Place::Api), "`{}` is not reachable from the palette", row.id);
        }
    }

    #[test]
    fn every_row_prints_words_in_both_languages() {
        for row in ROWS {
            let english = row.label.text(Lang::En);
            let russian = row.label.text(Lang::Ru);
            assert!(!english.trim().is_empty(), "`{}` has no English label", row.id);
            assert!(!russian.trim().is_empty(), "`{}` has no Russian label", row.id);
        }
    }

    #[test]
    fn a_button_row_carries_drawn_art() {
        for row in ROWS.iter().filter(|row| row.draws_a_button()) {
            let icon = row.icon.expect("filtered to rows with art");
            assert!(icon.is_drawn(), "`{}` points at the placeholder `{}`", row.id, icon.uri());
        }
    }
}
