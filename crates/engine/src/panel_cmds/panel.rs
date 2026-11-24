//! The panel itself: creation, stored outline, name, nominal size and body material.
//!
//! A panel is born from a nominal size or from another object's contour ring, and it stores its
//! outline as a ring once baked. Parameters are JSON objects with `deny_unknown_fields`; each
//! command validates through `Project` before it mutates and leaves the document untouched on
//! error, because the session runs it on a copy and swaps that copy in only on success.
//!
//! It refuses an empty name, a nominal size outside the panel's limit, a material the catalogue
//! does not hold or whose thickness the panel's geometry rejects, a source object without a
//! contour, and a panel that is not in the project.
//!
//! Changes when: a panel's creation, stored outline, name, nominal size or body material changes.
//!
//! It does not do: the edge and corner trims (`banding.rs`), the machining records
//! (`machining.rs`), or any geometry derivation.

use catalog::Directory;
use geom::Contact;
use geom::DVec3;
use model::{
    ClassCode, Contour, EdgeBevel, EdgeRef, Id, ItemId, Level, Operation, Panel, PanelNominalSize, PanelOutline,
    PanelType, Placement, Project, ProjectError, PropertyValue, SYSTEM_CLASS,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use units::{Degrees, Length};

use crate::class_rules::{Dimension, NodeRules, allow_creation};
use crate::commands::{CommandError, CommandOutcome, CommandSpec, Creation};
use crate::session::Session;
use text::Message;

/// Stable id of the panel creation command.
pub const CREATE_ID: &str = "panel.create";

/// User-facing label of the panel creation command.
pub const CREATE_LABEL: Message = Message::CmdCreatePanel;

/// Parameters for `panel.create`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PanelCreate {
    /// Id of the container that receives the panel.
    pub parent: Id,
    /// User-visible panel name; it must contain a non-whitespace character.
    pub name: String,
    /// Panel nominal length and width, the two sides the stance measures.
    ///
    /// The third side is the board's thickness, which the material's record carries: a panel's
    /// `length` runs along the stance's length axis, its `width` along the stance's width axis, and
    /// the thickness runs along the stance's thickness axis — the direction face `A` points. The
    /// guide's `stances` table is that frame written out for `shelf`, `post` and `wall`.
    pub nominal_size: PanelNominalSize,
    /// Panel-body material from the session catalogue.
    pub material: ItemId,
    /// Placement relative to the parent container.
    pub placement: Placement,
    /// The furniture class the new part plays, when the tool that created it has one.
    ///
    /// A fill tool knows the class whose `fill` trait chose the axis pair, so the post or shelf is
    /// born classified instead of unclassified. A panel created by the plain panel tool carries
    /// none, which is the long-standing default (ADR 0034).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class: Option<ClassCode>,
    /// The outline the panel is born with, when the tool measured one.
    ///
    /// A ring makes the new panel a contour panel from its first revision (ADR 0042): the ring is
    /// the panel's own outline anchored at its zero corner, and `nominal_size` must be the size of
    /// its bounds. A rectangle outline may carry corner treatments — the fill tool's classifier
    /// hands over the chamfers or radii the walls leave — and keeps its nominal size. The
    /// empty-space tool creates a shaped part this way, in the same one command and one undo entry,
    /// so a click is never a create followed by a corner-feature change.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outline: Option<PanelOutline>,
    /// The end bevels the shaped fill measured, one per outline edge (ADR 0107).
    ///
    /// They travel with the same command as the outline, so a board that meets a leaned wall flush
    /// is one create and one undo entry rather than a create followed by a bevel per edge. Each
    /// record names a rectangle edge or a ring segment of the outline above; the model validates
    /// both against the candidate panel before it enters the tree.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bevels: Vec<EdgeBevel>,
}

impl PanelCreate {
    /// Default nominal length of a newly created panel, in millimetres.
    pub const DEFAULT_LENGTH_MM: i64 = 600;
    /// Default nominal width of a newly created panel, in millimetres.
    pub const DEFAULT_WIDTH_MM: i64 = 400;

    /// The nominal size a new panel starts with: 600 × 400 mm.
    ///
    /// The default belongs to the command, not to a caller (C7): the ribbon and the palette both
    /// build their `panel.create` parameters from this one function.
    ///
    /// Returns [`CommandError::InvalidParams`] when a default side is not a usable length or the
    /// pair is outside the panel's limit, which cannot happen for these constants and is still
    /// reported rather than assumed.
    pub fn default_nominal_size() -> Result<PanelNominalSize, CommandError> {
        let length = Length::from_millimetres(Self::DEFAULT_LENGTH_MM)
            .ok_or_else(|| invalid_default("the default panel length is out of range"))?;
        let width = Length::from_millimetres(Self::DEFAULT_WIDTH_MM)
            .ok_or_else(|| invalid_default("the default panel width is out of range"))?;
        PanelNominalSize::new(length, width).map_err(|error| invalid_default(&error.to_string()))
    }

    fn parse(params: &Value) -> Result<Self, CommandError> {
        serde_json::from_value(params.clone())
            .map_err(|error| CommandError::InvalidParams { command: CREATE_ID, message: error.to_string() })
    }

    fn validate_name(&self) -> Result<(), CommandError> {
        if self.name.trim().is_empty() {
            return Err(CommandError::InvalidParams {
                command: CREATE_ID,
                message: "panel name must not be empty".to_owned(),
            });
        }
        Ok(())
    }
}

/// A default that the command's own constants failed to satisfy.
fn invalid_default(message: &str) -> CommandError {
    CommandError::InvalidParams { command: CREATE_ID, message: message.to_owned() }
}

/// The `panel.create` command.
pub(crate) const CREATE: CommandSpec =
    CommandSpec::new::<PanelCreate>(CREATE_ID, CREATE_LABEL, create_enabled, create_run)
        .creating(Creation::new(Level::Part));

/// Stable id of the contour-panel creation command.
pub const CONTOUR_CREATE_ID: &str = "panel.contour.create";

/// User-facing label of the contour-panel creation command.
pub const CONTOUR_CREATE_LABEL: Message = Message::CmdCreateContourPanel;

/// Parameters for `panel.contour.create`.
///
/// `source` is a constructive object in the scene: the new panel takes that object's contour ring
/// as its outline. The command always creates a panel; an existing panel never changes variant.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PanelContourCreate {
    /// Id of the container that receives the panel.
    pub parent: Id,
    /// User-visible panel name; it must contain a non-whitespace character.
    pub name: String,
    /// Constructive object whose contour becomes the panel outline.
    pub source: Id,
    /// Panel-body material from the session catalogue.
    pub material: ItemId,
    /// Offset composed on top of the source's frame, in the parent frame.
    ///
    /// The identity means "exactly over the source": the panel's local zero, its contour's base
    /// corner, is placed on the source's world corner (ADR 0083).
    pub placement: Placement,
}

impl PanelContourCreate {
    fn parse(params: &Value) -> Result<Self, CommandError> {
        serde_json::from_value(params.clone())
            .map_err(|error| CommandError::InvalidParams { command: CONTOUR_CREATE_ID, message: error.to_string() })
    }

    fn validate_name(&self) -> Result<(), CommandError> {
        if self.name.trim().is_empty() {
            return Err(CommandError::InvalidParams {
                command: CONTOUR_CREATE_ID,
                message: "panel name must not be empty".to_owned(),
            });
        }
        Ok(())
    }
}

/// The `panel.contour.create` command.
pub(crate) const CONTOUR_CREATE: CommandSpec = CommandSpec::new::<PanelContourCreate>(
    CONTOUR_CREATE_ID,
    CONTOUR_CREATE_LABEL,
    contour_create_enabled,
    contour_create_run,
)
.creating(Creation::new(Level::Part));

fn contour_create_enabled(session: &Session, params: &Value) -> bool {
    let Ok(params) = PanelContourCreate::parse(params) else {
        return false;
    };
    params.validate_name().is_ok()
        && session.project().is_container(params.parent)
        && allow_creation(session.project(), params.parent, session.active_product()).is_ok()
        && session.directory().board(params.material).is_some()
        && session.project().validate_panel_contour(params.parent, params.source).is_ok()
}

fn contour_create_run(
    project: &mut Project,
    catalog: &Directory,
    params: &Value,
    _contact: &dyn Contact,
) -> Result<CommandOutcome, CommandError> {
    let params = PanelContourCreate::parse(params)?;
    params.validate_name()?;
    let material = catalog.board(params.material).ok_or(CommandError::UnknownMaterial { material: params.material })?;
    let contour = project.object_contour(params.source)?.clone();
    let placement = over_source_placement(project, &params)?;
    let id = project.add_panel_contour(
        params.parent,
        params.name,
        &contour,
        Some(params.material),
        material.thickness,
        placement,
    )?;
    Ok(CommandOutcome::created(id))
}

/// Stable id of the end-bevel command.
pub const BEVEL_SET_ID: &str = "panel.bevel.set";

/// User-facing label of the end-bevel command.
pub const BEVEL_SET_LABEL: Message = Message::CmdSetBevel;

/// Parameters for `panel.bevel.set`.
///
/// The source is an outline edge — a rectangle edge or a ring segment — and the angle tilts the
/// end face across the thickness about that edge's line. A positive angle removes material; the
/// outline on face `B` is untouched, so the nominal size stays the stock size. The command follows
/// the class's contour trait, because a beveled end is an outline edit, and the card's shape section
/// is gated by the same trait.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PanelBevelSet {
    /// Id of the panel whose end is beveled.
    pub panel: Id,
    /// The outline edge the end face tilts about.
    pub source: EdgeRef,
    /// The tilt across the thickness, in millidegrees.
    pub angle: Degrees,
}

impl PanelBevelSet {
    fn parse(params: &Value) -> Result<Self, CommandError> {
        serde_json::from_value(params.clone())
            .map_err(|error| CommandError::InvalidParams { command: BEVEL_SET_ID, message: error.to_string() })
    }

    fn bevel(&self) -> Result<EdgeBevel, CommandError> {
        EdgeBevel::new(self.source, self.angle)
            .map_err(|error| CommandError::InvalidParams { command: BEVEL_SET_ID, message: error.to_string() })
    }
}

/// The `panel.bevel.set` command.
pub(crate) const BEVEL_SET: CommandSpec =
    CommandSpec::new::<PanelBevelSet>(BEVEL_SET_ID, BEVEL_SET_LABEL, bevel_set_enabled, bevel_set_run);

fn bevel_set_enabled(session: &Session, params: &Value) -> bool {
    let Ok(params) = PanelBevelSet::parse(params) else {
        return false;
    };
    let Ok(bevel) = params.bevel() else {
        return false;
    };
    class_allows_contour(session.project(), params.panel, BEVEL_SET_ID).is_ok()
        && session.project().validate_panel_bevel(params.panel, bevel).is_ok()
}

fn bevel_set_run(
    project: &mut Project,
    _catalog: &Directory,
    params: &Value,
    _contact: &dyn Contact,
) -> Result<CommandOutcome, CommandError> {
    let params = PanelBevelSet::parse(params)?;
    let bevel = params.bevel()?;
    class_allows_contour(project, params.panel, BEVEL_SET_ID)?;
    project.set_panel_bevel(params.panel, bevel)?;
    Ok(CommandOutcome::changed())
}

/// Stable id of the end-bevel clear command.
pub const BEVEL_CLEAR_ID: &str = "panel.bevel.clear";

/// User-facing label of the end-bevel clear command.
pub const BEVEL_CLEAR_LABEL: Message = Message::CmdClearBevel;

/// Parameters for `panel.bevel.clear`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PanelBevelClear {
    /// Id of the panel whose end bevel is removed.
    pub panel: Id,
    /// The outline edge to clear.
    pub source: EdgeRef,
}

impl PanelBevelClear {
    fn parse(params: &Value) -> Result<Self, CommandError> {
        serde_json::from_value(params.clone())
            .map_err(|error| CommandError::InvalidParams { command: BEVEL_CLEAR_ID, message: error.to_string() })
    }
}

/// The `panel.bevel.clear` command.
pub(crate) const BEVEL_CLEAR: CommandSpec =
    CommandSpec::new::<PanelBevelClear>(BEVEL_CLEAR_ID, BEVEL_CLEAR_LABEL, bevel_clear_enabled, bevel_clear_run);

fn bevel_clear_enabled(session: &Session, params: &Value) -> bool {
    let Ok(params) = PanelBevelClear::parse(params) else {
        return false;
    };
    session.project().panel(params.panel).is_some_and(|panel| panel.bevel_at(params.source).is_some())
}

fn bevel_clear_run(
    project: &mut Project,
    _catalog: &Directory,
    params: &Value,
    _contact: &dyn Contact,
) -> Result<CommandOutcome, CommandError> {
    let params = PanelBevelClear::parse(params)?;
    project.clear_panel_bevel(params.panel, params.source)?;
    Ok(CommandOutcome::changed())
}

/// The parent-frame placement that puts the new panel exactly over its source.
///
/// A node stores a placement relative to its parent and a panel's local zero is its contour's base
/// corner, so `offset ∘ inverse(parent_world) ∘ source_world` is the pre-rebase frame: the model
/// then appends the base corner in the source frame, and no world point moves (ADR 0083).
fn over_source_placement(project: &Project, params: &PanelContourCreate) -> Result<Placement, CommandError> {
    let invalid = |error: model::PlacementError| CommandError::InvalidParams {
        command: CONTOUR_CREATE_ID,
        message: error.to_string(),
    };
    let parent_world =
        project.world_placement(params.parent).ok_or(ProjectError::ContainerNotFound { id: params.parent })?;
    let source_world =
        project.world_placement(params.source).ok_or(ProjectError::NodeNotFound { id: params.source })?;
    let parent_inverse = parent_world.inverse().map_err(invalid)?;
    let over_source = Placement::compose(parent_inverse, source_world).map_err(invalid)?;
    Placement::compose(params.placement, over_source).map_err(invalid)
}

/// Stable id of the contour-outline replacement command.
pub const OUTLINE_SET_ID: &str = "panel.outline.set";

/// User-facing label of the contour-outline replacement command.
pub const OUTLINE_SET_LABEL: Message = Message::CmdSetPanelOutline;

/// Parameters for `panel.outline.set`.
///
/// The ring is in the panel-local plane and may sit anywhere; the command anchors it at the
/// panel-local zero and moves the panel frame with it, so the world-space shape stays fixed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PanelOutlineSet {
    /// Id of the contour panel whose outline changes.
    pub panel: Id,
    /// The new closed ring.
    pub contour: Contour,
}

impl PanelOutlineSet {
    fn parse(params: &Value) -> Result<Self, CommandError> {
        serde_json::from_value(params.clone())
            .map_err(|error| CommandError::InvalidParams { command: OUTLINE_SET_ID, message: error.to_string() })
    }
}

/// The `panel.outline.set` command.
pub(crate) const OUTLINE_SET: CommandSpec =
    CommandSpec::new::<PanelOutlineSet>(OUTLINE_SET_ID, OUTLINE_SET_LABEL, outline_set_enabled, outline_set_run);

fn outline_set_enabled(session: &Session, params: &Value) -> bool {
    let Ok(params) = PanelOutlineSet::parse(params) else {
        return false;
    };
    class_allows_contour(session.project(), params.panel, OUTLINE_SET_ID).is_ok()
        && session.project().validate_panel_outline(params.panel, &params.contour).is_ok()
}

fn outline_set_run(
    project: &mut Project,
    _catalog: &Directory,
    params: &Value,
    _contact: &dyn Contact,
) -> Result<CommandOutcome, CommandError> {
    let params = PanelOutlineSet::parse(params)?;
    class_allows_contour(project, params.panel, OUTLINE_SET_ID)?;
    project.set_panel_outline(params.panel, params.contour)?;
    Ok(CommandOutcome::changed())
}

/// Stable id of the outline-bake command.
pub const OUTLINE_CONTOUR_ID: &str = "panel.outline.to_contour";

/// User-facing label of the outline-bake command.
pub const OUTLINE_CONTOUR_LABEL: Message = Message::CmdBakePanelOutline;

/// Parameters for `panel.outline.to_contour`.
///
/// The command bakes a rectangle outline into the ring it draws, in one revision and one undo
/// entry, so *Edit contour* can reach a panel that is not a ring yet. Corner and bow parameters are
/// replaced by their curve segments; undo restores them (ADR 0061).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PanelOutlineToContour {
    /// Id of the rectangle panel whose outline is baked.
    pub panel: Id,
}

impl PanelOutlineToContour {
    fn parse(params: &Value) -> Result<Self, CommandError> {
        serde_json::from_value(params.clone())
            .map_err(|error| CommandError::InvalidParams { command: OUTLINE_CONTOUR_ID, message: error.to_string() })
    }
}

/// The `panel.outline.to_contour` command.
pub(crate) const OUTLINE_CONTOUR: CommandSpec = CommandSpec::new::<PanelOutlineToContour>(
    OUTLINE_CONTOUR_ID,
    OUTLINE_CONTOUR_LABEL,
    outline_contour_enabled,
    outline_contour_run,
);

fn outline_contour_enabled(session: &Session, params: &Value) -> bool {
    let Ok(params) = PanelOutlineToContour::parse(params) else {
        return false;
    };
    class_allows_contour(session.project(), params.panel, OUTLINE_CONTOUR_ID).is_ok()
        && session.project().validate_panel_contour_conversion(params.panel).is_ok()
}

fn outline_contour_run(
    project: &mut Project,
    _catalog: &Directory,
    params: &Value,
    _contact: &dyn Contact,
) -> Result<CommandOutcome, CommandError> {
    let params = PanelOutlineToContour::parse(params)?;
    class_allows_contour(project, params.panel, OUTLINE_CONTOUR_ID)?;
    project.convert_panel_to_contour(params.panel)?;
    Ok(CommandOutcome::changed())
}

fn create_enabled(session: &Session, params: &Value) -> bool {
    let Ok(params) = PanelCreate::parse(params) else {
        return false;
    };
    params.validate_name().is_ok()
        && session.project().is_container(params.parent)
        && allow_creation(session.project(), params.parent, session.active_product()).is_ok()
        && session.directory().board(params.material).is_some()
}

/// Stable id of the typed panel creation command.
pub const CREATE_STANDARD_ID: &str = "panel.create.standard";

/// User-facing label of the typed panel creation command.
pub const CREATE_STANDARD_LABEL: Message = Message::CmdCreateStandardPanel;

/// The typed panel creation command's parameters: a maker's word instead of a quaternion.
///
/// A panel's local frame is the same for every type — `X` is the length, `Y` the width and `Z` the
/// thickness, which the material carries — so the caller names the type and the base corner and the
/// command answers the rotation (X117). A type is a class and a stance, not a second kind of part:
/// anything else is drawn by the contour and points commands.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PanelCreateStandard {
    /// Id of the container that receives the panel.
    pub parent: Id,
    /// User-visible panel name; it must contain a non-whitespace character.
    pub name: String,
    /// Panel-body material from the session catalogue; its record carries the thickness.
    pub material: ItemId,
    /// The type: `shelf`, `post` or `wall`, which fixes the part's frame and its class.
    #[serde(rename = "type")]
    pub kind: PanelType,
    /// Where the panel's own zero corner stands: the base corner nearest the product's origin, in
    /// the parent's frame, millimetres.
    pub at: [f64; 3],
    /// Length: the panel's own `X`, in the type's frame.
    pub length: Length,
    /// Width: the panel's own `Y`, in the type's frame.
    pub width: Length,
}

impl PanelCreateStandard {
    fn parse(params: &Value) -> Result<Self, CommandError> {
        serde_json::from_value(params.clone())
            .map_err(|error| CommandError::InvalidParams { command: CREATE_STANDARD_ID, message: error.to_string() })
    }

    /// The request `panel.create` would take for this panel, so the two store a board one way.
    fn into_create(self) -> Result<PanelCreate, CommandError> {
        let class = self.kind.class().ok_or_else(|| CommandError::InvalidParams {
            command: CREATE_STANDARD_ID,
            message: format!("the class registry does not carry the code of a {}", self.kind.word()),
        })?;
        let nominal_size = PanelNominalSize::new(self.length, self.width)
            .map_err(|error| CommandError::InvalidParams { command: CREATE_STANDARD_ID, message: error.to_string() })?;
        let placement = Placement::new(DVec3::from_array(self.at), self.kind.rotation())
            .map_err(|error| CommandError::InvalidParams { command: CREATE_STANDARD_ID, message: error.to_string() })?;
        Ok(PanelCreate {
            parent: self.parent,
            name: self.name,
            nominal_size,
            material: self.material,
            placement,
            class: Some(class),
            outline: None,
            bevels: Vec::new(),
        })
    }
}

/// The `panel.create.standard` command.
pub(crate) const CREATE_STANDARD: CommandSpec = CommandSpec::new::<PanelCreateStandard>(
    CREATE_STANDARD_ID,
    CREATE_STANDARD_LABEL,
    create_standard_enabled,
    create_standard_run,
)
.creating(Creation::new(Level::Part));

/// Whether the typed create may run: the same conditions as `panel.create`.
fn create_standard_enabled(session: &Session, params: &Value) -> bool {
    PanelCreateStandard::parse(params).is_ok_and(|params| {
        !params.name.trim().is_empty()
            && session.project().is_container(params.parent)
            && allow_creation(session.project(), params.parent, session.active_product()).is_ok()
            && session.directory().board(params.material).is_some()
    })
}

/// Creates one typed panel.
fn create_standard_run(
    project: &mut Project,
    catalog: &Directory,
    params: &Value,
    _contact: &dyn Contact,
) -> Result<CommandOutcome, CommandError> {
    let create = PanelCreateStandard::parse(params)?.into_create()?;
    let id = create_panel(project, catalog, create)?;
    Ok(CommandOutcome::created(id))
}

fn create_run(
    project: &mut Project,
    catalog: &Directory,
    params: &Value,
    _contact: &dyn Contact,
) -> Result<CommandOutcome, CommandError> {
    let params = PanelCreate::parse(params)?;
    let id = create_panel(project, catalog, params)?;
    Ok(CommandOutcome::created(id))
}

/// Writes one panel from a typed request, and answers the id of the node it made.
///
/// This is the one creation path: `panel.create` parses its parameters into [`PanelCreate`] and a
/// command that plans a part — a fill of a measured niche — builds the same value and writes it
/// here, so the two cannot store a board differently.
pub(super) fn create_panel(
    project: &mut Project,
    catalog: &Directory,
    params: PanelCreate,
) -> Result<Id, CommandError> {
    params.validate_name()?;
    let material = catalog.board(params.material).ok_or(CommandError::UnknownMaterial { material: params.material })?;
    let thickness = material.thickness;
    let before = project.revision();
    let id = match params.outline.as_ref() {
        Some(PanelOutline::Contour { contour }) => {
            let anchored = Panel::anchored_contour(contour).map_err(ProjectError::from)?;
            let measured = Panel::contour_size(&anchored).map_err(ProjectError::from)?;
            if measured != params.nominal_size {
                return Err(CommandError::InvalidParams {
                    command: CREATE_ID,
                    message: "the outline and the nominal size do not describe the same panel".to_owned(),
                });
            }
            project.add_panel_contour_with_bevels(
                params.parent,
                params.name,
                &anchored,
                &params.bevels,
                Some(params.material),
                thickness,
                params.placement,
            )?
        }
        Some(outline) => project.add_panel_with_outline(
            params.parent,
            params.name,
            params.nominal_size,
            outline.clone(),
            &params.bevels,
            Some(params.material),
            thickness,
            params.placement,
        )?,
        None => {
            if params.bevels.is_empty() {
                project.add_panel(
                    params.parent,
                    params.name,
                    params.nominal_size,
                    Some(params.material),
                    thickness,
                    params.placement,
                )?
            } else {
                // A bevel list with no outline still names rectangle edges, so the plain rectangle is
                // created through the same path rather than dropping the records.
                project.add_panel_with_outline(
                    params.parent,
                    params.name,
                    params.nominal_size,
                    PanelOutline::default(),
                    &params.bevels,
                    Some(params.material),
                    thickness,
                    params.placement,
                )?
            }
        }
    };
    if let Some(class) = params.class {
        project.set_property(id, SYSTEM_CLASS, PropertyValue::class(class))?;
        // The role is part of the act of making the part, not a second change to it: the class is
        // written by the same command, so the document moves one revision and one undo entry covers
        // the board and its role — a create that took a class was two revisions before this
        // (ADR 0238, ADR 0234's rule applied to a creation rather than a batch).
        project.settle_revision(before);
    }
    Ok(id)
}

/// Stable id of the panel rename command.
pub const RENAME_ID: &str = "panel.rename";

/// User-facing label of the panel rename command.
pub const RENAME_LABEL: Message = Message::CmdRenamePanel;

/// Parameters for `panel.rename`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PanelRename {
    /// Id of the panel to rename.
    pub panel: Id,
    /// New user-visible panel name; it must contain a non-whitespace character.
    pub name: String,
}

impl PanelRename {
    fn parse(params: &Value) -> Result<Self, CommandError> {
        serde_json::from_value(params.clone())
            .map_err(|error| CommandError::InvalidParams { command: RENAME_ID, message: error.to_string() })
    }

    fn validate_name(&self) -> Result<(), CommandError> {
        if self.name.trim().is_empty() {
            return Err(CommandError::InvalidParams {
                command: RENAME_ID,
                message: "panel name must not be empty".to_owned(),
            });
        }
        Ok(())
    }
}

/// The `panel.rename` command.
pub(crate) const RENAME: CommandSpec =
    CommandSpec::new::<PanelRename>(RENAME_ID, RENAME_LABEL, rename_enabled, rename_run);

fn rename_enabled(session: &Session, params: &Value) -> bool {
    let Ok(params) = PanelRename::parse(params) else {
        return false;
    };
    params.validate_name().is_ok() && session.project().panel(params.panel).is_some()
}

fn rename_run(
    project: &mut Project,
    _catalog: &Directory,
    params: &Value,
    _contact: &dyn Contact,
) -> Result<CommandOutcome, CommandError> {
    let params = PanelRename::parse(params)?;
    params.validate_name()?;
    project.rename_panel(params.panel, params.name)?;
    Ok(CommandOutcome::changed())
}

/// Stable id of the panel resize command.
pub const RESIZE_ID: &str = "panel.resize";

/// User-facing label of the panel resize command.
pub const RESIZE_LABEL: Message = Message::CmdResizePanel;

/// Parameters for `panel.resize`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PanelResize {
    /// Id of the panel to resize.
    pub panel: Id,
    /// New nominal panel length and width.
    pub nominal_size: PanelNominalSize,
}

impl PanelResize {
    fn parse(params: &Value) -> Result<Self, CommandError> {
        serde_json::from_value(params.clone())
            .map_err(|error| CommandError::InvalidParams { command: RESIZE_ID, message: error.to_string() })
    }
}

/// The `panel.resize` command.
pub(crate) const RESIZE: CommandSpec =
    CommandSpec::new::<PanelResize>(RESIZE_ID, RESIZE_LABEL, resize_enabled, resize_run);

fn resize_enabled(session: &Session, params: &Value) -> bool {
    let Ok(params) = PanelResize::parse(params) else {
        return false;
    };
    class_allows_resize(session.project(), params.panel, params.nominal_size).is_ok()
        && session.project().validate_panel_resize(params.panel, params.nominal_size).is_ok()
}

fn resize_run(
    project: &mut Project,
    _catalog: &Directory,
    params: &Value,
    _contact: &dyn Contact,
) -> Result<CommandOutcome, CommandError> {
    let params = PanelResize::parse(params)?;
    class_allows_resize(project, params.panel, params.nominal_size)?;
    project.set_panel_nominal_size(params.panel, params.nominal_size)?;
    Ok(CommandOutcome::changed())
}

/// The class gate of a nominal-size change: only the dimensions the class leaves free may move.
///
/// A no-op resize is always allowed, so a field that reports the current size does not fail. An
/// unclassified panel is free, and the model still checks the geometry and the machining.
fn class_allows_resize(project: &Project, panel: Id, nominal_size: PanelNominalSize) -> Result<(), CommandError> {
    let Some(current) = project.panel(panel) else {
        return Ok(());
    };
    let rules = NodeRules::of(project, panel);
    let changes = [
        (Dimension::Length, nominal_size.length() != current.nominal_size().length()),
        (Dimension::Width, nominal_size.width() != current.nominal_size().width()),
    ];
    for (dimension, changed) in changes {
        if changed {
            rules.allow_resize(dimension).map_err(|refusal| CommandError::class(RESIZE_ID, refusal))?;
        }
    }
    Ok(())
}

/// The class gate of a panel outline edit: a class that admits no contour admits no reshaping.
fn class_allows_contour(project: &Project, panel: Id, command: &'static str) -> Result<(), CommandError> {
    NodeRules::of(project, panel)
        .allow_operation(Operation::Contour)
        .map_err(|refusal| CommandError::class(command, refusal))
}

/// Stable id of the panel material command.
pub const MATERIAL_SET_ID: &str = "panel.material.set";

/// User-facing label of the panel material command.
pub const MATERIAL_SET_LABEL: Message = Message::CmdSetPanelMaterial;

/// Parameters for `panel.material.set`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PanelMaterialSet {
    /// Id of the panel whose material changes.
    pub panel: Id,
    /// Panel-body material from the session catalogue.
    pub material: ItemId,
}

impl PanelMaterialSet {
    fn parse(params: &Value) -> Result<Self, CommandError> {
        serde_json::from_value(params.clone())
            .map_err(|error| CommandError::InvalidParams { command: MATERIAL_SET_ID, message: error.to_string() })
    }
}

/// The `panel.material.set` command.
pub(crate) const MATERIAL_SET: CommandSpec =
    CommandSpec::new::<PanelMaterialSet>(MATERIAL_SET_ID, MATERIAL_SET_LABEL, material_set_enabled, material_set_run);

fn material_set_enabled(session: &Session, params: &Value) -> bool {
    let Ok(params) = PanelMaterialSet::parse(params) else {
        return false;
    };
    let Some(material) = session.directory().board(params.material) else {
        return false;
    };
    session.project().panel(params.panel).is_some_and(|panel| panel.geometry(material.thickness).is_ok())
}

fn material_set_run(
    project: &mut Project,
    catalog: &Directory,
    params: &Value,
    _contact: &dyn Contact,
) -> Result<CommandOutcome, CommandError> {
    let params = PanelMaterialSet::parse(params)?;
    let material = catalog.board(params.material).ok_or(CommandError::UnknownMaterial { material: params.material })?;
    project.set_panel_material(params.panel, params.material, material.thickness)?;
    Ok(CommandOutcome::changed())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use model::{RectEdge, Revision};
    use serde_json::Value;
    use testkit::{sample_catalog, sample_project};

    use crate::commands::{CommandError, CommandRegistry};
    use crate::session::Session;

    use super::*;

    fn session() -> Session {
        Session::with_directory(sample_project(), sample_catalog())
    }

    fn params<T: serde::Serialize>(value: &T) -> Value {
        serde_json::to_value(value).expect("params serialize")
    }

    fn panel_id() -> Id {
        Id::new(2).expect("the sample panel")
    }

    fn set(angle: i64) -> PanelBevelSet {
        PanelBevelSet {
            panel: panel_id(),
            source: EdgeRef::Rect { edge: RectEdge::Bottom.id() },
            angle: Degrees::from_millidegrees(angle),
        }
    }

    #[test]
    fn a_typed_panel_is_born_on_its_base_corner_and_never_turned_upside_down() {
        // X117: the maker names a type and a base corner instead of a quaternion, so a build cannot
        // hang a board's origin above its geometry. Every type keeps its local zero corner at `at`
        // and its lowest point on that corner's height.
        for kind in PanelType::ALL {
            let (project, product) = testkit::sample_product_project();
            let mut session = Session::with_directory(project, sample_catalog());
            assert!(session.open_product(product), "the sample product opens");
            let request = PanelCreateStandard {
                parent: product,
                name: format!("{} panel", kind.word()),
                material: ItemId::new(1, 1).expect("the sample board"),
                kind,
                at: [100.0, 200.0, 300.0],
                length: Length::from_millimetres(600).expect("a length"),
                width: Length::from_millimetres(400).expect("a width"),
            };
            let asked = params(&request);
            let spec = CommandRegistry::new().get(CREATE_STANDARD_ID).expect("registered");
            assert!(spec.enabled(&session, &asked), "{} is offered", kind.word());

            let created = session
                .dispatch(CREATE_STANDARD_ID, &asked)
                .expect("the typed create runs")
                .created_id()
                .expect("a panel id");
            let placement = session.project().world_placement(created).expect("a placement");
            assert!(
                (placement.translation() - DVec3::new(100.0, 200.0, 300.0)).length() < 1.0e-9,
                "{} puts its zero corner at the point: {placement:?}",
                kind.word()
            );
            let size = session.project().panel(created).expect("a panel").nominal_size();
            assert_eq!(size.length(), Length::from_millimetres(600).expect("a length"), "{}", kind.word());
            assert_eq!(size.width(), Length::from_millimetres(400).expect("a width"), "{}", kind.word());
            let born = session.project().class(created);
            assert_eq!(born, kind.class(), "{} is born classified as its type", kind.word());
            let boxed = session.project().world_box(created).expect("a box");
            assert!((boxed.0.z - 300.0).abs() < 1.0e-9, "{} stands on its base corner: {boxed:?}", kind.word());
        }
    }

    #[test]
    fn one_bevel_command_is_one_undo_entry() {
        let mut session = session();
        let registry = CommandRegistry::new();
        let request = set(45_000);
        let spec = registry.get(BEVEL_SET_ID).expect("registered");
        assert!(spec.enabled(&session, &params(&request)), "the sample panel takes a bevel");
        assert_eq!(session.project().revision(), Revision::ZERO);

        session.dispatch(BEVEL_SET_ID, &params(&request)).expect("set the bevel");
        assert_eq!(session.project().revision().get(), 1);
        let panel = session.project().panel(panel_id()).expect("panel");
        assert_eq!(panel.bevels().len(), 1);

        session.undo().expect("undo the bevel");
        assert!(session.project().panel(panel_id()).expect("panel").bevels().is_empty());
        session.redo().expect("redo the bevel");
        assert_eq!(session.project().panel(panel_id()).expect("panel").bevels().len(), 1);

        let clear = PanelBevelClear { panel: panel_id(), source: request.source };
        assert!(
            registry.get(BEVEL_CLEAR_ID).expect("registered").enabled(&session, &params(&clear)),
            "a beveled edge can be cleared"
        );
        session.dispatch(BEVEL_CLEAR_ID, &params(&clear)).expect("clear the bevel");
        assert!(session.project().panel(panel_id()).expect("panel").bevels().is_empty());
        assert!(!registry.get(BEVEL_CLEAR_ID).expect("registered").enabled(&session, &params(&clear)));
        assert_eq!(
            session.dispatch(BEVEL_CLEAR_ID, &params(&clear)),
            Err(CommandError::Model(ProjectError::BevelNotFound { panel: panel_id(), on: request.source }))
        );
    }

    #[test]
    fn a_command_refuses_an_impossible_angle_a_banded_edge_and_a_missing_panel() {
        let mut session = session();
        let registry = CommandRegistry::new();
        let spec = registry.get(BEVEL_SET_ID).expect("registered");

        // The cap is a parameter error, not a model refusal: the record cannot hold the value.
        for angle in [0, 89_000, -10_000] {
            let request = set(angle);
            assert!(!spec.enabled(&session, &params(&request)), "angle {angle} is not a bevel");
            assert!(matches!(
                session.dispatch(BEVEL_SET_ID, &params(&request)),
                Err(CommandError::InvalidParams { command: BEVEL_SET_ID, .. })
            ));
        }

        // A banded edge has no square face left, and the band is what says so.
        session
            .dispatch(
                "panel.edge_banding.set",
                &params(&crate::panel_cmds::PanelEdgeBandingSet {
                    panel: panel_id(),
                    edge: RectEdge::Bottom.id(),
                    material: testkit::sample_edge_material_id(),
                    included_in_size: false,
                }),
            )
            .expect("band the bottom edge");
        let request = set(45_000);
        assert!(!spec.enabled(&session, &params(&request)));
        assert_eq!(
            session.dispatch(BEVEL_SET_ID, &params(&request)),
            Err(CommandError::Model(ProjectError::BevelOnBandedEdge { panel: panel_id(), on: request.source }))
        );

        // A panel that is not in the project is refused before anything else.
        let missing = PanelBevelSet { panel: Id::new(99).expect("id"), ..set(45_000) };
        assert_eq!(
            session.dispatch(BEVEL_SET_ID, &params(&missing)),
            Err(CommandError::Model(ProjectError::PanelNotFound { id: missing.panel }))
        );
    }
}
