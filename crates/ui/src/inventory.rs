//! The inventory of capabilities: one row per command, and the places it appears in.
//!
//! The interface keeps three registers that do not know each other — the engine's registry, the
//! ribbon map and the palette's parameter builders — so a capability could be added with no home,
//! or with two homes that disagree. This module is the fourth register that ends that: a row is
//! keyed by the command's stable id and says where the capability appears, who it applies to, its
//! artwork and its words (ADR 0117), and [`KEYS`] beside it is the one table of keys.
//!
//! A surface filters this table instead of writing its own list: the palette shows every row that
//! has a place, and the context menu (`crate::node_actions`) shows the rows whose [`Applies`] matches
//! the node under the pointer, on the tree row and in the viewport alike (U92). The ribbon stays the
//! curated lexicon — a cell names a *method* (`sketch.create.line`) and draws its own words, because
//! two methods can share one command — so the ribbon reads this table for the rows that are one
//! button and keeps its cells for the rest.
//!
//! What the table does not carry is behaviour: a row names an id, a message, an icon and a key, and
//! the command itself stays in `engine`. The gate in `cargo xtask ui-rules` fails when a
//! registered command has no row, when a row names an id no command registered, when an id carries
//! two labels or two keys, when an `Api` row claims another place, and when a live ribbon cell has
//! no artwork.
//!
//! A key is data, not a match arm. [`KEYS`] holds every binding once — the modifiers, the key, the
//! spelling the interface prints and the id it runs — so the shell consumes a key by id
//! (`app/src/shortcuts.rs`), the ribbon prints the label of the method it belongs to, and
//! the Settings page lists the same rows. A user-assignable key would rewrite this table and nothing
//! else (U96); until then the defaults are what the table says.

use text::Message;

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
    /// The armed tool shows and edits its parameters in the tool card, or commits a running draft
    /// and a contour edit from that card's one row (U97, U115).
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
    /// The artwork of the one button that draws it, or `None` when it has no button of its own.
    pub icon: Option<Icon>,
}

/// The modifier pair a key is pressed with, named once so the printed label and the pair the shell
/// consumes are the same row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Chord {
    /// The key alone.
    Plain,
    /// Shift and the key.
    Shift,
    /// Ctrl and the key.
    Ctrl,
    /// Ctrl, Shift and the key.
    CtrlShift,
    /// Alt and the key.
    Alt,
    /// The platform's command key: Ctrl on Windows and Linux, Command on macOS.
    Command,
}

impl Chord {
    /// The pair the shell hands to `egui` when it consumes the key.
    pub fn modifiers(self) -> egui::Modifiers {
        match self {
            Chord::Plain => egui::Modifiers::NONE,
            Chord::Shift => egui::Modifiers::SHIFT,
            Chord::Ctrl => egui::Modifiers::CTRL,
            Chord::CtrlShift => egui::Modifiers { shift: true, ..egui::Modifiers::CTRL },
            Chord::Alt => egui::Modifiers::ALT,
            Chord::Command => egui::Modifiers::COMMAND,
        }
    }

    /// The word the interface prints in front of the key, in the spelling the label column uses.
    pub const fn prefix(self) -> &'static str {
        match self {
            Chord::Plain => "",
            Chord::Shift => "Shift+",
            Chord::Ctrl => "Ctrl+",
            Chord::CtrlShift => "Ctrl+Shift+",
            Chord::Alt => "Alt+",
            // The label says Ctrl on every platform, which is what the interface printed before the
            // table existed; a macOS spelling is a wording decision, not a binding one.
            Chord::Command => "Ctrl+",
        }
    }
}

/// One key binding: the id it runs, the key itself and the words the interface prints for it.
///
/// The row is the one record of a binding: the shell consumes the key by [`Binding::id`], the ribbon
/// prints [`Binding::label`] and the Settings page lists the pair, so a remapped key is a data edit
/// rather than a hunt through three files (ADR 0117).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Binding {
    /// What the key runs: a command's id, a method's id from the ribbon map, or an [`ACTIONS`] id.
    pub id: &'static str,
    /// The key that runs it.
    pub key: egui::Key,
    /// The modifier pair the key is pressed with.
    pub chord: Chord,
    /// The spelling the interface prints, checked against the pair by the inventory's own test.
    pub label: &'static str,
}

/// An interface action the shell runs from a key and no engine command answers for.
///
/// A file operation, an undo and a panel toggle belong to the shell rather than the document, so they
/// have no row in [`ROWS`] and no registry entry; they still need an id for a key to name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Action {
    /// The stable id, namespaced like a command's.
    pub id: &'static str,
    /// The words the Settings page prints, in both languages.
    pub words: Message,
}

/// The id of the file operation that opens an empty scene in a new tab.
pub const FILE_NEW: &str = "file.new";
/// The id of the operation that closes the active tab.
pub const FILE_CLOSE: &str = "file.close";
/// The id of the operation that saves the active scene.
pub const FILE_SAVE: &str = "file.save";
/// The id of the document undo.
pub const EDIT_UNDO: &str = "edit.undo";
/// The id of the document redo.
pub const EDIT_REDO: &str = "edit.redo";
/// The id of the command palette.
pub const PALETTE_OPEN: &str = "palette.open";
/// The id that opens the assistant panel, where a person talks to an agent (J7).
pub const ASSISTANT_OPEN: &str = "assistant.open";
/// The id that hides both side panels, or shows the pair the last press put away.
pub const PANELS_HIDE: &str = "view.panels.hide";
/// The id that folds both side panels to strips, or unfolds them again.
pub const PANELS_COLLAPSE: &str = "view.panels.collapse";
/// The id of the `Select` transform mode: view state, so no command answers for it.
pub const TRANSFORM_SELECT: &str = "view.transform.select";
/// The id of the free `Move` mode.
pub const TRANSFORM_MOVE: &str = "view.transform.move";
/// The id of the object move, which obeys the document's rules.
pub const TRANSFORM_OBJECT: &str = "view.transform.object";
/// The id of the `Rotate` mode.
pub const TRANSFORM_ROTATE: &str = "view.transform.rotate";
/// The id of the object turn, which stops where another body does.
pub const TRANSFORM_OBJECT_ROTATE: &str = "view.transform.object-rotate";

/// Every interface action the shell runs from a key, with no command behind it.
pub const ACTIONS: &[Action] = &[
    Action { id: FILE_NEW, words: Message::ShortcutNewScene },
    Action { id: FILE_CLOSE, words: Message::ShortcutCloseTab },
    Action { id: FILE_SAVE, words: Message::ShortcutSave },
    Action { id: EDIT_UNDO, words: Message::ShortcutUndo },
    Action { id: EDIT_REDO, words: Message::ShortcutRedo },
    Action { id: PALETTE_OPEN, words: Message::ShortcutPalette },
    Action { id: ASSISTANT_OPEN, words: Message::ShortcutAssistant },
    Action { id: PANELS_HIDE, words: Message::ShortcutHidePanels },
    Action { id: PANELS_COLLAPSE, words: Message::ShortcutCollapsePanels },
    // The five gestures the viewport offers are one band and five keys (U113, X88). They write no
    // document, so no command and no undo entry answers for them; the words are the band's own, so
    // the cell, the menu entry and the Settings row print one spelling.
    Action { id: TRANSFORM_SELECT, words: Message::Select },
    Action { id: TRANSFORM_MOVE, words: Message::Move },
    Action { id: TRANSFORM_OBJECT, words: Message::Object },
    Action { id: TRANSFORM_ROTATE, words: Message::Rotate },
    Action { id: TRANSFORM_OBJECT_ROTATE, words: Message::ObjectRotate },
];

/// Every key the interface binds, in the order the Settings page lists them.
///
/// The table is the keymap: the shell consumes a key by looking its id up here, the ribbon prints the
/// label of the method it belongs to, and the Settings page walks the rows. The drawing tools' keys
/// are filed under the map's method ids, because a command reached by six methods has six keys and
/// the method is what a key starts (ADR 0117, 0118).
///
pub const KEYS: &[Binding] = &[
    Binding { id: FILE_NEW, key: egui::Key::N, chord: Chord::Ctrl, label: "Ctrl+N" },
    Binding { id: FILE_CLOSE, key: egui::Key::W, chord: Chord::Ctrl, label: "Ctrl+W" },
    Binding { id: FILE_SAVE, key: egui::Key::S, chord: Chord::Ctrl, label: "Ctrl+S" },
    Binding { id: EDIT_UNDO, key: egui::Key::Z, chord: Chord::Ctrl, label: "Ctrl+Z" },
    Binding { id: EDIT_REDO, key: egui::Key::Y, chord: Chord::Ctrl, label: "Ctrl+Y" },
    Binding { id: PALETTE_OPEN, key: egui::Key::K, chord: Chord::Command, label: "Ctrl+K" },
    Binding { id: ASSISTANT_OPEN, key: egui::Key::J, chord: Chord::Ctrl, label: "Ctrl+J" },
    Binding { id: PANELS_HIDE, key: egui::Key::Num0, chord: Chord::Ctrl, label: "Ctrl+0" },
    Binding { id: PANELS_COLLAPSE, key: egui::Key::Num0, chord: Chord::CtrlShift, label: "Ctrl+Shift+0" },
    Binding { id: "view.layout.maximize", key: egui::Key::W, chord: Chord::Alt, label: "Alt+W" },
    // The band's five keys: `Q`, `W` and `E`, each free tool beside its measured pair on the shifted
    // letter, on the letters the sketch, the view and the niche do not hold (U113, X88). The object
    // move and the object turn are the shifted pairs of the free ones because they are the same
    // decision made by the document's rules (ADR 0147).
    Binding { id: TRANSFORM_SELECT, key: egui::Key::Q, chord: Chord::Plain, label: "Q" },
    Binding { id: TRANSFORM_MOVE, key: egui::Key::W, chord: Chord::Plain, label: "W" },
    Binding { id: TRANSFORM_OBJECT, key: egui::Key::W, chord: Chord::Shift, label: "Shift+W" },
    Binding { id: TRANSFORM_ROTATE, key: egui::Key::E, chord: Chord::Plain, label: "E" },
    Binding { id: TRANSFORM_OBJECT_ROTATE, key: egui::Key::E, chord: Chord::Shift, label: "Shift+E" },
    Binding { id: "node.delete", key: egui::Key::Delete, chord: Chord::Plain, label: "Delete" },
    Binding { id: "view.projection.toggle", key: egui::Key::O, chord: Chord::Plain, label: "O" },
    Binding { id: "view.projection.frame-all", key: egui::Key::F, chord: Chord::Plain, label: "F" },
    Binding { id: "view.snaps.grid", key: egui::Key::G, chord: Chord::Plain, label: "G" },
    // The ground shadow is a view mode, not a tool: its switch is drawn by the View tab's Display
    // group and filed as an atom, because it writes no document (U107).
    Binding { id: "view.display.shadows", key: egui::Key::S, chord: Chord::Plain, label: "S" },
    Binding { id: "sketch.create.line", key: egui::Key::L, chord: Chord::Plain, label: "L" },
    Binding { id: "sketch.create.polyline", key: egui::Key::P, chord: Chord::Plain, label: "P" },
    Binding { id: "sketch.create.arc", key: egui::Key::A, chord: Chord::Plain, label: "A" },
    Binding { id: "sketch.create.arc.centre", key: egui::Key::A, chord: Chord::Shift, label: "Shift+A" },
    Binding { id: "sketch.create.rectangle", key: egui::Key::R, chord: Chord::Plain, label: "R" },
    Binding { id: "sketch.create.rectangle.centre", key: egui::Key::R, chord: Chord::Shift, label: "Shift+R" },
    Binding { id: "sketch.create.rectangle.centre-directed", key: egui::Key::D, chord: Chord::Shift, label: "Shift+D" },
    Binding { id: "sketch.create.circle", key: egui::Key::C, chord: Chord::Plain, label: "C" },
    Binding { id: "sketch.create.circle.three-points", key: egui::Key::C, chord: Chord::Shift, label: "Shift+C" },
    Binding { id: "sketch.create.polygon", key: egui::Key::N, chord: Chord::Plain, label: "N" },
    Binding { id: "sketch.measure.length", key: egui::Key::M, chord: Chord::Plain, label: "M" },
    // The niche is filed under the cell that arms it: the atom's own row names the mechanism, the
    // map's method names the button (ADR 0117).
    Binding { id: "furniture.panels.void", key: egui::Key::V, chord: Chord::Plain, label: "V" },
];

/// The binding with `id`, if a key runs it.
pub fn binding(id: &str) -> Option<&'static Binding> {
    KEYS.iter().find(|binding| binding.id == id)
}

/// The printed spelling of the key that runs `id`, for a hint or a list column.
pub fn key_label(id: &str) -> Option<&'static str> {
    binding(id).map(|binding| binding.label)
}

/// The interface action `id` names, if it is a shell action rather than a command or a method.
pub fn action(id: &str) -> Option<&'static Action> {
    ACTIONS.iter().find(|action| action.id == id)
}

/// The words the interface prints for a binding that has no command and no method of its own.
///
/// A command's words are its row's, a method's are the map's, and a shell action carries its own; the
/// Settings page walks [`KEYS`] and asks this function, so one id is never described twice.
pub fn words(id: &str) -> Option<Message> {
    if let Some(row) = row(id) {
        return Some(row.label);
    }
    if let Some(action) = action(id) {
        return Some(action.words);
    }
    crate::toolbars::spec::words(id)
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

    /// The key that runs this row, if the interface binds one.
    ///
    /// The key is not a column of the row: a key runs a *method*, and one command can have six, so
    /// the binding table is keyed by the same id and this is the row's reading of it.
    pub fn key(&self) -> Option<&'static Binding> {
        binding(self.id)
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
        icon: None,
    },
    Row {
        id: "panel.fill",
        // The three ribbon cells arm the empty-space gesture, so the command they write through has
        // no button of its own: a caller that cannot point names the niche's point, its class and
        // its mode, and the same model answer makes the part (N37).
        label: Message::CmdFillNiche,
        places: &[Place::Api],
        applies: Applies::Always,
        icon: None,
    },
    Row {
        id: "panel.contour.create",
        label: Message::CmdCreateContourPanel,
        places: &[Place::Api],
        applies: Applies::Always,
        icon: None,
    },
    Row {
        id: "panel.outline.set",
        label: Message::CmdSetPanelOutline,
        // A contour edit commits from its card's one row as well as from the panel's card: the ✓ is
        // the same command, whichever surface the hand is nearest (U115).
        places: &[Place::Card, Place::ToolCard],
        applies: Applies::Context(ActsOn::Panel),
        icon: None,
    },
    Row {
        id: "panel.outline.to_contour",
        label: Message::CmdBakePanelOutline,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Panel),
        icon: None,
    },
    Row {
        id: "assembly.create",
        label: Message::CmdCreateAssembly,
        places: &[Place::Ribbon, Place::Palette],
        applies: Applies::Always,
        icon: Some(crate::icons::scene::ASSEMBLY),
    },
    Row {
        id: "product.create",
        label: Message::CmdCreateProduct,
        places: &[Place::Ribbon],
        applies: Applies::Always,
        icon: Some(crate::icons::scene::PRODUCT),
    },
    Row {
        id: "product.envelope.set",
        label: Message::CmdSetProductEnvelope,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Container),
        icon: None,
    },
    Row {
        id: "panel.rename",
        label: Message::CmdRenamePanel,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Panel),
        icon: None,
    },
    Row {
        id: "node.rename",
        label: Message::CmdRename,
        places: &[Place::Palette, Place::Card],
        applies: Applies::Context(ActsOn::Node),
        icon: None,
    },
    Row {
        id: "panel.resize",
        label: Message::CmdResizePanel,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Panel),
        icon: None,
    },
    Row {
        id: "panel.material.set",
        label: Message::CmdSetPanelMaterial,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Panel),
        icon: None,
    },
    Row {
        id: "panel.edge_banding.set",
        label: Message::CmdSetEdgeBanding,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Panel),
        icon: None,
    },
    Row {
        id: "panel.edge_banding.clear",
        label: Message::CmdClearEdgeBanding,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Panel),
        icon: None,
    },
    Row {
        id: "panel.corner_banding.set",
        label: Message::CmdSetCornerBanding,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Panel),
        icon: None,
    },
    Row {
        id: "panel.corner_banding.clear",
        label: Message::CmdClearCornerBanding,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Panel),
        icon: None,
    },
    Row {
        id: "panel.ring_banding.set",
        label: Message::CmdSetContourBanding,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Panel),
        icon: None,
    },
    Row {
        id: "panel.ring_banding.clear",
        label: Message::CmdClearContourBanding,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Panel),
        icon: None,
    },
    Row {
        id: "panel.corner.set",
        label: Message::CmdSetCornerFeature,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Panel),
        icon: None,
    },
    Row {
        id: "panel.corner.clear",
        label: Message::CmdClearCornerFeature,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Panel),
        icon: None,
    },
    Row {
        id: "panel.bow.set",
        label: Message::CmdSetEdgeBow,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Panel),
        icon: None,
    },
    Row {
        id: "panel.bow.clear",
        label: Message::CmdClearEdgeBow,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Panel),
        icon: None,
    },
    Row {
        id: "panel.bevel.set",
        label: Message::CmdSetBevel,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Panel),
        icon: None,
    },
    Row {
        id: "panel.bevel.clear",
        label: Message::CmdClearBevel,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Panel),
        icon: None,
    },
    Row {
        id: "panel.machining.add",
        label: Message::CmdAddMachining,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Panel),
        icon: None,
    },
    Row {
        id: "panel.machining.update",
        label: Message::CmdUpdateMachining,
        // The ring of a contour cutout is edited in the viewport and committed by the tool card's
        // ✓, which is this command (U115).
        places: &[Place::Card, Place::ToolCard],
        applies: Applies::Context(ActsOn::Panel),
        icon: None,
    },
    Row {
        id: "panel.machining.remove",
        label: Message::CmdRemoveMachining,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Panel),
        icon: None,
    },
    Row {
        id: "panel.machining.move",
        label: Message::CmdMoveMachining,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Panel),
        icon: None,
    },
    Row {
        id: "node.delete",
        label: Message::CmdDelete,
        places: &[Place::ContextMenu, Place::Palette, Place::Card],
        applies: Applies::Context(ActsOn::Node),
        icon: None,
    },
    Row {
        id: "node.delete.many",
        label: Message::CmdDeleteSelection,
        places: &[Place::Palette],
        applies: Applies::Context(ActsOn::Selection),
        icon: None,
    },
    Row {
        id: "node.duplicate",
        label: Message::CmdDuplicate,
        places: &[Place::ContextMenu, Place::Palette, Place::Card],
        applies: Applies::Context(ActsOn::Node),
        icon: None,
    },
    Row {
        id: "node.group",
        label: Message::CmdGroup,
        places: &[Place::ContextMenu, Place::Palette, Place::Card],
        applies: Applies::Context(ActsOn::Selection),
        icon: None,
    },
    Row {
        id: "node.ungroup",
        label: Message::CmdUngroup,
        places: &[Place::ContextMenu, Place::Palette, Place::Card],
        applies: Applies::Context(ActsOn::Container),
        icon: None,
    },
    Row {
        id: "node.placement.set",
        label: Message::CmdPlace,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Node),
        icon: None,
    },
    Row {
        id: "node.placement.world_zero",
        label: Message::CmdPlaceAtWorldZero,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Node),
        icon: None,
    },
    Row {
        id: "node.placement.set_many",
        label: Message::CmdPlaceSelection,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Selection),
        icon: None,
    },
    Row {
        id: "node.pivot.set",
        label: Message::CmdSetPivot,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Node),
        icon: None,
    },
    Row {
        id: "node.pivot.center",
        label: Message::CmdCenterPivot,
        places: &[Place::Palette, Place::Card],
        applies: Applies::Context(ActsOn::Node),
        icon: None,
    },
    Row {
        id: "node.origin.rebase",
        label: Message::CmdRebaseOrigin,
        places: &[Place::Api],
        applies: Applies::Context(ActsOn::Node),
        icon: None,
    },
    Row {
        id: "node.property.set",
        label: Message::CmdSetProperty,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Node),
        icon: None,
    },
    Row {
        id: "node.property.set_many",
        label: Message::CmdSetProperties,
        places: &[Place::Api],
        applies: Applies::Context(ActsOn::Node),
        icon: None,
    },
    Row {
        id: "node.property.clear",
        label: Message::CmdClearProperty,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Node),
        icon: None,
    },
    Row {
        id: "node.component.convert_to_group",
        label: Message::CmdConvertToGroup,
        places: &[Place::ContextMenu, Place::Palette, Place::Card],
        applies: Applies::Context(ActsOn::Container),
        icon: None,
    },
    Row {
        id: "object.primitive.create",
        label: Message::CmdCreatePrimitive,
        places: &[Place::Ribbon],
        applies: Applies::Always,
        icon: None,
    },
    Row {
        id: "object.primitive.replace",
        label: Message::CmdReplacePrimitive,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Object),
        icon: None,
    },
    Row {
        id: "object.extrude.create",
        label: Message::CmdCreateExtrusion,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Object),
        icon: None,
    },
    Row {
        id: "object.cuboid.create",
        // The registry's own word is the ribbon cell's, so the id and the button print one shape
        // (X105). No surface runs it yet: the box cell is the rectangle gesture (ADR 0138), and the
        // reason lives in `plan/debt.md`.
        label: Message::ToolBox,
        places: &[Place::Api],
        applies: Applies::Always,
        icon: None,
    },
    Row {
        id: "object.extrude.height.set",
        label: Message::CmdSetExtrusionHeight,
        places: &[Place::Ribbon],
        applies: Applies::Context(ActsOn::Object),
        icon: None,
    },
    Row {
        id: "object.contour.replace",
        label: Message::CmdReplaceContour,
        places: &[Place::Card, Place::ToolCard],
        applies: Applies::Context(ActsOn::Object),
        icon: None,
    },
    Row {
        id: "object.path.create",
        label: Message::CmdCreatePath,
        places: &[Place::Ribbon],
        applies: Applies::Always,
        icon: None,
    },
    Row {
        id: "object.path.extrude",
        label: Message::CmdExtrudeDrawing,
        places: &[Place::Ribbon],
        applies: Applies::Context(ActsOn::Object),
        icon: Some(crate::icons::bodies::EXTRUDE),
    },
    // The area pull is the Extrude cell's second method: a drawing of several rings has no one
    // profile to sweep, so this method arms the pointer and the click names an area of the drawing
    // (G4, X100). The first method of the same cell extrudes the whole drawing, one body per ring.
    // The command creates a body and leaves the drawing where it was. The row carries no artwork of
    // its own: the cell draws the extrude glyph, and a second drawing for one gesture would say the
    // two are different tools.
    Row {
        id: "object.area.extrude",
        label: Message::CmdExtrudeArea,
        places: &[Place::Ribbon],
        applies: Applies::Context(ActsOn::Object),
        icon: None,
    },
    // The contour booleans take a base ring and one or more rings beside it, so they act on a
    // selection rather than on the node under the pointer; the ribbon's three cells arm them and
    // the registered command behind each cell stores the result (G3, ADR 0194).
    Row {
        id: "contour.boolean.union",
        label: Message::ToolUnion,
        places: &[Place::Ribbon],
        applies: Applies::Context(ActsOn::Selection),
        icon: Some(crate::icons::contour::BOOLEAN_UNION),
    },
    Row {
        id: "contour.boolean.subtract",
        label: Message::ToolSubtract,
        places: &[Place::Ribbon],
        applies: Applies::Context(ActsOn::Selection),
        icon: Some(crate::icons::contour::BOOLEAN_SUBTRACT),
    },
    Row {
        id: "contour.boolean.intersect",
        label: Message::ToolIntersect,
        places: &[Place::Ribbon],
        applies: Applies::Context(ActsOn::Selection),
        icon: Some(crate::icons::contour::BOOLEAN_INTERSECT),
    },
    Row {
        id: "object.path.replace",
        label: Message::CmdReplacePath,
        places: &[Place::Card, Place::ToolCard],
        applies: Applies::Context(ActsOn::Object),
        icon: None,
    },
    // A body boolean is a mode: the Model tab's three cells arm it, the target is the selected
    // body and the tool is the next body picked, so it acts on the selection rather than on the one
    // node under the pointer. Its artwork belongs to the cells, one operation each, which is why the
    // row carries none (W1).
    Row {
        id: "object.boolean.set",
        label: Message::CmdSetBoolean,
        places: &[Place::Ribbon],
        applies: Applies::Context(ActsOn::Selection),
        icon: None,
    },
    // The mark says a body is a tool: the card offers it, and it is what lets a cutter enter the
    // body it cuts and survive the operation that spends an unmarked one (W2, ADR 0197).
    Row {
        id: "object.tool.set",
        label: Message::CmdSetToolMark,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Object),
        icon: None,
    },
    // The object card lists the steps a body carries and removes one from its row.
    Row {
        id: "object.boolean.remove",
        label: Message::CmdRemoveBoolean,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Object),
        icon: None,
    },
    Row {
        id: "object.collapse",
        label: Message::CmdCollapseToMesh,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Object),
        icon: None,
    },
    Row {
        id: "mesh.vertices.move",
        label: Message::CmdMoveVertices,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Object),
        icon: None,
    },
    Row {
        id: "camera.create",
        label: Message::CmdCreateCamera,
        places: &[Place::Ribbon, Place::Palette, Place::Card],
        applies: Applies::Always,
        icon: Some(crate::icons::scene::CAMERA),
    },
    Row {
        id: "light.create",
        label: Message::CmdCreateLight,
        places: &[Place::Ribbon, Place::Palette, Place::Card],
        applies: Applies::Always,
        icon: Some(crate::icons::scene::LIGHT),
    },
    Row {
        id: "scene.active_camera.set",
        label: Message::CmdSetActiveCamera,
        places: &[Place::Palette, Place::Card],
        applies: Applies::Context(ActsOn::Camera),
        icon: None,
    },
    Row {
        id: "camera.projection.set",
        label: Message::CmdSetCameraProjection,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Camera),
        icon: None,
    },
    Row {
        id: "light.params.set",
        label: Message::CmdSetLightParameters,
        places: &[Place::Card],
        applies: Applies::Context(ActsOn::Light),
        icon: None,
    },
    // The atoms: the last rows are the mechanisms a surface draws with no command behind them, so
    // neither has an id in the engine registry and neither writes an undo entry (ADR 0118). The
    // ruler is two clicks and a length; the niche is the empty space the pointer holds, which the
    // fill tools, the report row and the assistant read from the same value.
    Row {
        id: "sketch.measure.length",
        label: Message::ToolMeasureLength,
        places: &[Place::Ribbon],
        applies: Applies::Always,
        icon: Some(crate::icons::contour::MEASURE),
    },
    Row {
        id: "probe.void",
        label: Message::ToolVoid,
        places: &[Place::Ribbon, Place::ContextMenu],
        applies: Applies::Context(ActsOn::Container),
        icon: Some(crate::icons::scene::VOID),
    },
    // The snap resolver is the canonical atom: it reads the pointer's ray and the document, answers
    // a point and a kind, and writes nothing. The View tab's Snaps group draws its ten switches, and
    // the drawing tool, the ruler and the object move consume it (ADR 0118, ADR 0199, U116).
    Row {
        id: "snap.scene",
        label: Message::GroupSnaps,
        places: &[Place::Ribbon],
        applies: Applies::Always,
        icon: Some(crate::icons::view::SNAP_VERTEX),
    },
    // The ground shadow's switch is view state: it writes no document, leaves no undo entry and the
    // renderer reads it per view (U107). Its intensity is one workspace value with no command of its
    // own — a field on the Settings page, not a capability (U110).
    Row {
        id: "view.display.shadows",
        label: Message::ToolShadows,
        places: &[Place::Ribbon],
        applies: Applies::Always,
        icon: Some(crate::icons::view::SHADOWS),
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
pub const ATOMS: &[&str] = &["sketch.measure.length", "probe.void", "snap.scene", "view.display.shadows"];

/// Every row that appears in `place`.
pub fn in_place(place: Place) -> impl Iterator<Item = &'static Row> {
    ROWS.iter().filter(move |row| row.has(place))
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine::CommandRegistry;
    use std::collections::BTreeMap;
    use text::Lang;

    #[test]
    fn every_registered_command_has_one_row() {
        let registry = CommandRegistry::new();
        for spec in registry.specs() {
            let row = row(spec.id()).unwrap_or_else(|| panic!("`{}` has no inventory row", spec.id()));
            assert_eq!(row.label, spec.label(), "`{}` prints two different labels", spec.id());
        }
        // An atom row is the one exception to "a row is a command": it is declared in `ATOMS`
        // and the registry has no entry for it (ADR 0118, U98).
        assert_eq!(ROWS.len(), registry.specs().len() + ATOMS.len(), "the inventory has a row for something else");
        for atom in ATOMS {
            let row = row(atom).unwrap_or_else(|| panic!("the atom `{atom}` has no row"));
            assert!(!row.places.is_empty(), "the atom `{atom}` appears nowhere");
            assert!(
                registry.specs().iter().all(|spec| spec.id() != *atom),
                "`{atom}` is both an atom and a registered command"
            );
        }
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
    fn an_icon_belongs_to_one_row() {
        let mut icons = BTreeMap::new();
        for row in ROWS {
            if let Some(icon) = row.icon {
                assert!(icons.insert(icon.uri(), row.id).is_none(), "one drawing names two commands");
            }
        }
    }

    #[test]
    fn a_key_runs_one_id_and_prints_its_own_pair() {
        // The label is the row's spelling and the pair is what the shell consumes, so the two are
        // checked against each other: a key moved without its label is a key the Settings page lies
        // about (U96).
        let mut keys = BTreeMap::new();
        let mut ids = BTreeMap::new();
        let mut labels = BTreeMap::new();
        for binding in KEYS {
            let pair = format!("{:?}+{}", binding.chord, binding.key.name());
            assert!(keys.insert(pair.clone(), binding.id).is_none(), "`{pair}` runs two ids");
            assert!(ids.insert(binding.id, binding.label).is_none(), "`{}` has two keys", binding.id);
            assert!(labels.insert(binding.label, binding.id).is_none(), "`{}` prints twice", binding.label);
            assert_eq!(
                binding.label,
                format!("{}{}", binding.chord.prefix(), binding.key.name()),
                "`{}` prints a label its pair does not spell",
                binding.id
            );
        }
    }

    #[test]
    fn every_key_runs_something_the_interface_knows() {
        // A binding names a command, a shell action or a method of the ribbon map; a typo would be a
        // key that reads as bound and runs nothing.
        for binding in KEYS {
            assert!(
                words(binding.id).is_some(),
                "`{}` is bound to `{}`, which no row, action or method names",
                binding.label,
                binding.id
            );
        }
    }

    #[test]
    fn every_action_answers_to_a_key() {
        for action in ACTIONS {
            assert!(binding(action.id).is_some(), "`{}` is an action no key runs", action.id);
            assert!(row(action.id).is_none(), "`{}` is an action and a command", action.id);
        }
    }

    #[test]
    fn every_binding_prints_its_words_in_both_languages() {
        for binding in KEYS {
            let Some(words) = words(binding.id) else { continue };
            assert!(!words.text(Lang::En).trim().is_empty(), "`{}` has no English words", binding.label);
            assert!(!words.text(Lang::Ru).trim().is_empty(), "`{}` has no Russian words", binding.label);
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
