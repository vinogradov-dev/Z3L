//! The editor: the view state a UI session owns, split by the mode that owns it.
//!
//! `Editor` holds no document data. Every entry point reads a `Project` or a `Session` and
//! dispatches a command for anything that changes the scene; the struct itself is the one place
//! where the modes meet. Each mode keeps its state and the methods that enter, feed and leave it in
//! its own module: `selection.rs` (the selection and the open set), `visibility.rs` (the hidden
//! and locked nodes), `mesh.rs` (sub-object editing), `contour.rs` (the contour working copy), `drawing.rs` (the drawing draft and its commit),
//! `creation.rs` (the armed container, the box tool and the `create` commands), `ribbon.rs` (the
//! ribbon and the extrusion it offers) and `cards.rs` (the scene tree and the inspector).
//!
//! Refuses nothing at this level: a mode that cannot start returns `false` and leaves the state
//! untouched, so the shell can ignore a gesture it cannot honour.
//!
//! Changes when: the editor gains or loses a pointer mode, a card or a field they share.

use std::collections::HashSet;

use model::Id;

use crate::box_tool::BoxDraft;
use crate::camera_card::CameraCard;
use crate::container_card::ContainerCard;
use crate::contour_edit::ContourEdit;
use crate::draw::{DrawDraft, DrawIntent};
use crate::light_card::LightCard;
use crate::mesh_edit::MeshEdit;
use crate::object_card::ObjectCard;
use crate::panel_card::PanelCard;
use crate::pivot_card::PivotCard;
use crate::placement_card::PlacementCard;
use crate::properties_card::PropertiesCard;
use crate::scene_tree::Selection;
use crate::toolbars;
use crate::view_flags::ViewFlags;

pub use self::boolean::{BooleanArm, BooleanOperand};
pub use self::fill::{FillArm, PanelBuild};
pub use self::selection::{BandRule, BandView, SelectionBand};

mod area;
mod boolean;
mod cards;
mod contour;
mod creation;
mod drawing;
mod fill;
mod measure;
mod mesh;
mod ribbon;
mod selection;
mod visibility;
mod void;

#[cfg(test)]
mod tests;

/// Selection and draft state owned by the editor panels.
///
/// The composition root of the modes above: each method touches the state of one mode and no
/// method takes two modes at once, so the field list is the list of modes, not a god object.
#[derive(Debug, Default)]
pub struct Editor {
    selection: Selection,
    open: HashSet<Id>,
    /// Hidden and locked nodes of this session; the scene builder, picking and the gizmos read
    /// them, and neither flag ever reaches the document (ADR 0065).
    flags: ViewFlags,
    mesh_edit: Option<MeshEdit>,
    contour_edit: Option<ContourEdit>,
    drawing: Option<DrawDraft>,
    /// What the running draft will become: a path of its own, a board cut to its ring, a body's
    /// profile, a drawn primitive or a frame's base.
    drawing_intent: DrawIntent,
    /// The base a product frame's height is pulled from, and the height, before its one command.
    ///
    /// The base arrives from the drawing: the frame cell arms the sketch's rectangle or its free
    /// outline, and the finished figure becomes this draft (N19, ADR 0101, ADR 0159).
    box_draft: Option<BoxDraft>,
    /// The class the next click fills a niche with, when the empty-space tool is armed.
    fill: Option<FillArm>,
    /// What the next click of the Panel cell makes, until the empty-space tool is armed with it.
    panel_build: PanelBuild,
    /// The ruler: the points of the live measurement while the tool is armed (ADR 0118).
    measure: Option<crate::measure::Measure>,
    /// The niche under the pointer: one measurement every tool that names a void reads (ADR 0118).
    void: crate::void::Void,
    /// Whether the niche tool itself owns the pointer, rather than a fill that consumes its value.
    void_armed: bool,
    /// The area under the pointer: the face of a drawing a click would pull (G4).
    area: crate::area::Area,
    /// The drawing whose areas the armed pull reads, or `None` when no pull is armed.
    area_pull: Option<Id>,
    /// The visibility revision the measured niche was taken at, so a stale cell is dropped.
    ///
    /// Switching a part off changes the void without moving the pointer, so a hand that holds still
    /// still needs a new cell (X73).
    void_revision: u64,
    /// The armed contour boolean: its operation, its base ring and the picks it previewed.
    boolean: Option<BooleanArm>,
    /// The container the next created nodes land in, or `None` for the scene root.
    creation_parent: Option<Id>,
    /// The transform mode a menu entry asked for, until the window takes it.
    ///
    /// The mode is the window's view state and this editor belongs to one tab, so a menu entry
    /// records the wish here and the shell writes it once per frame (U113).
    mode_request: Option<crate::transform::TransformMode>,
    toolbars: toolbars::Toolbars,
    card: PanelCard,
    container_card: ContainerCard,
    object_card: ObjectCard,
    camera_card: CameraCard,
    light_card: LightCard,
    placement_card: PlacementCard,
    pivot_card: PivotCard,
    properties_card: PropertiesCard,
    error: Option<String>,
    initialized: bool,
}

impl Editor {
    /// Creates an editor with no node selected; the scene tree selects the first panel on first show.
    pub fn new() -> Self {
        Self::default()
    }

    /// The ribbon's view state, for a test that asserts opening its variant list writes nothing.
    #[cfg(test)]
    pub(crate) fn toolbars_mut(&mut self) -> &mut toolbars::Toolbars {
        &mut self.toolbars
    }

    /// Takes the transform mode a menu entry asked for, leaving none behind.
    ///
    /// The window owns the mode, so it drains this once per frame after the menus have drawn. One
    /// slot rather than a queue: only one menu is open at a time, and the last press is the one that
    /// counts.
    pub fn take_mode_request(&mut self) -> Option<crate::transform::TransformMode> {
        self.mode_request.take()
    }
}
