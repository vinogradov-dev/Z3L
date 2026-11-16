//! The selected geometry object card.
//!
//! An object is a leaf with one geometry payload. A mesh object is edited directly and has no
//! feature history. A primitive object shows its parameters — three sides of a box, a radius, a
//! height, the number of sides and bands — because they are the source of truth and the mesh is
//! derived ([ADR 0054]). A constructive object shows its contour instead: the contour and the feature
//! height are the source of truth, and `object.collapse` bakes it into an editable mesh
//! ([ADR 0037]). A path object shows its flat drawing: it has no mesh and no feature height until
//! it becomes a contour or is extruded.
//!
//! The card has one reading order and no headers ([ADR 0116]): a head that says which object this
//! is and what it measures, the fields the selected kind can be edited by, one row of actions and
//! the detail of a mode while it runs. A kind that cannot be edited as a mesh is not offered a mesh
//! block, and a multi-selection is refused once rather than in every block.
//!
//! Every number in the card is the shared field ([ADR 0092]): a figure dimension, a primitive
//! parameter or a typed property is the same 76 px box with its unit outside and no Apply button.
//! A figure or primitive parameter re-cuts geometry and rebuilds a mesh, so its field hands the
//! number over at the release (or after the typing debounce, or on Enter) rather than on every frame
//! of a drag; one gesture is still one undo entry through the session edit scope.
//!
//! [ADR 0037]: ../docs/decisions/0037-a-geometry-object-is-a-mesh-or-a-feature-history.md
//! [ADR 0054]: ../docs/decisions/0054-a-primitive-keeps-the-parameters-that-built-it.md
//! [ADR 0092]: ../docs/decisions/0092-a-field-is-a-number-with-its-unit-outside.md
//! [ADR 0116]: ../docs/decisions/0116-the-object-card-is-one-reading-order.md

use core::ops::RangeInclusive;

use egui::{RichText, TextEdit, Ui};
use engine::node_cmds::NODE_RENAME_ID;
use engine::object_cmds::{
    OBJECT_COLLAPSE_ID, OBJECT_CONTOUR_REPLACE_ID, OBJECT_PATH_REPLACE_ID, OBJECT_PRIMITIVE_REPLACE_ID,
};
use engine::{
    EditScope, NodeRename, ObjectCollapse, ObjectContourReplace, ObjectPathReplace, ObjectPrimitiveReplace, Session,
};
use model::{
    Contour, ContourEditError, ContourSegment, ContourSegmentId, Geometry, Id, MAX_BANDS, MAX_POLYGON_SIDES,
    MAX_PRIMITIVE_SIZE, MAX_SEGMENTS, MIN_BANDS, MIN_POLYGON_SIDES, MIN_PRIMITIVE_SIZE, MIN_SEGMENTS, Path, PathShape,
    Primitive, Project,
};
use text::{Lang, Message};
use units::Length;

use crate::command::{dispatch, json};
use crate::mesh_edit::{MeshEdit, MeshElement};
use crate::scrub::{self, CommitPolicy, Scrub};
use crate::theme;

/// What the object card asks the editor to do.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ObjectAction {
    /// Nothing was pressed.
    #[default]
    None,
    /// Enter or leave sub-object editing.
    Toggle,
    /// Switch between vertex and face selection.
    Element(MeshElement),
    /// Enter contour editing on this constructive object or path.
    StartContour,
}

/// Most selected vertices the card lists with their positions; the count covers the rest.
const MAX_LISTED_VERTICES: usize = 12;

/// Persistent state of the object card.
#[derive(Debug, Default)]
pub(crate) struct ObjectCard {
    draft: Option<Draft>,
    contour_draft: Option<ContourDraft>,
    primitive_draft: Option<PrimitiveDraft>,
    figure_draft: Option<FigureDraft>,
    confirm_collapse: bool,
    error: Option<String>,
}

#[derive(Clone, Debug)]
struct Draft {
    object: Id,
    name: String,
}

/// Text being edited and the contour it was read from.
///
/// `base` detects a change made elsewhere, for example by undo: the card re-reads the document
/// instead of letting stale text overwrite a restored contour.
#[derive(Clone, Debug)]
struct ContourDraft {
    object: Id,
    base: Option<Contour>,
    text: String,
}

impl ContourDraft {
    fn new(object: Id, contour: Option<&Contour>) -> Self {
        Self { object, base: contour.cloned(), text: contour.map(format_contour).unwrap_or_default() }
    }
}

/// The numbers of one primitive parameter and the primitive they were read from.
///
/// `base` plays the same role as in [`ContourDraft`]: an edit made elsewhere — by undo, or by a
/// gizmo — re-reads the document instead of leaving a stale number in the field.
#[derive(Clone, Debug)]
struct PrimitiveDraft {
    object: Id,
    base: Primitive,
    fields: PrimitiveFields,
}

/// The parameter fields as the numbers the shared field edits, one per editable dimension.
///
/// The lengths are millimetres and the counts whole numbers, because that is what a scrub moves;
/// the document keeps [`Length`], and the card converts at the command edge.
#[derive(Clone, Debug, Default, PartialEq)]
struct PrimitiveFields {
    width: f64,
    depth: f64,
    height: f64,
    radius: f64,
    segments: u32,
    bands: u32,
}

impl PrimitiveFields {
    fn read(primitive: &Primitive) -> Self {
        let mut fields = Self::default();
        match primitive {
            Primitive::Cuboid(cuboid) => {
                fields.width = cuboid.width().millimetres_f64();
                fields.depth = cuboid.depth().millimetres_f64();
                fields.height = cuboid.height().millimetres_f64();
            }
            Primitive::Cylinder(cylinder) => {
                fields.radius = cylinder.radius().millimetres_f64();
                fields.height = cylinder.height().millimetres_f64();
                fields.segments = cylinder.segments();
            }
            Primitive::Sphere(sphere) => {
                fields.radius = sphere.radius().millimetres_f64();
                fields.segments = sphere.segments();
                fields.bands = sphere.bands();
            }
        }
        fields
    }
}

impl PrimitiveDraft {
    fn new(object: Id, primitive: &Primitive) -> Self {
        Self { object, base: primitive.clone(), fields: PrimitiveFields::read(primitive) }
    }

    /// The primitive the fields describe, or a readable reason they do not.
    ///
    /// The shape comes from `base`, so a field that belongs to another shape is never invented: a
    /// box has only its three sides and a sphere has no height.
    fn parse(&self, lang: Lang) -> Result<Primitive, String> {
        let length = |value: f64| {
            scrub::millimetres(value)
                .ok_or_else(|| Message::PrimitiveRefused.with(lang, [Message::LengthErrorOutOfRange.text(lang)]))
        };
        let primitive = match &self.base {
            Primitive::Cuboid(_) => {
                let width = length(self.fields.width)?;
                let depth = length(self.fields.depth)?;
                let height = length(self.fields.height)?;
                Primitive::cuboid(width, depth, height)
            }
            Primitive::Cylinder(_) => {
                Primitive::cylinder(length(self.fields.radius)?, length(self.fields.height)?, self.fields.segments)
            }
            Primitive::Sphere(_) => {
                Primitive::sphere(length(self.fields.radius)?, self.fields.segments, self.fields.bands)
            }
        };
        primitive.map_err(|error| Message::PrimitiveRefused.with(lang, [error.to_string()]))
    }
}

/// The figure a path was drawn as, and the fields the card exposes for it.
///
/// A tagged figure stores its own constraint set rather than a dimension (ADR 0073), so the fields
/// are read back from the geometry and written through the same setters the grips use. An untagged
/// path has fields only when it is a single arc, the one figure whose radius and end angle a card
/// can name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FigureKind {
    /// A closed circle of four quarter arcs.
    Circle,
    /// A closed rectangle.
    Rectangle,
    /// A closed regular polygon.
    Polygon,
    /// One arc segment, named by its id.
    Arc(ContourSegmentId),
}

impl FigureKind {
    /// The figure `path` carries, when the card has fields for it.
    fn of(path: &Path) -> Option<Self> {
        match path.shape() {
            Some(PathShape::Circle) => Some(Self::Circle),
            Some(PathShape::Rectangle) => Some(Self::Rectangle),
            Some(PathShape::RegularPolygon) => Some(Self::Polygon),
            None => {
                let [entry] = path.entries() else {
                    return None;
                };
                matches!(entry.segment(), ContourSegment::Arc { .. }).then_some(Self::Arc(entry.id()))
            }
        }
    }
}

/// The numbers of one figure and the path they were read from.
///
/// `base` plays the same role as in [`PrimitiveDraft`]: an edit made elsewhere — by undo, or by the
/// viewport — re-reads the document instead of leaving a stale number in the field.
#[derive(Clone, Debug)]
struct FigureDraft {
    object: Id,
    base: Path,
    kind: FigureKind,
    fields: FigureFields,
}

/// The figure fields as the numbers the shared field edits, one per dimension the figure has.
///
/// The lengths are millimetres and the angle is degrees, because that is what a scrub moves; the
/// document keeps [`Length`] and [`units::Degrees`], and the card converts once, when a
/// field hands its number over.
#[derive(Clone, Debug, Default, PartialEq)]
struct FigureFields {
    radius: f64,
    width: f64,
    height: f64,
    sides: u32,
    end_angle: f64,
}

impl FigureFields {
    /// Reads the fields the figure has from its own constraint frame.
    fn read(kind: FigureKind, path: &Path) -> Result<Self, String> {
        let mut fields = Self::default();
        match kind {
            FigureKind::Circle => {
                fields.radius =
                    path.circle_frame().map_err(|error| figure_error(error, Lang::En))?.radius.millimetres_f64()
            }
            FigureKind::Rectangle => {
                let frame = path.rectangle_frame().map_err(|error| figure_error(error, Lang::En))?;
                fields.width = frame.width.millimetres_f64();
                fields.height = frame.height.millimetres_f64();
            }
            FigureKind::Polygon => {
                let frame = path.polygon_frame().map_err(|error| figure_error(error, Lang::En))?;
                fields.sides = u32::try_from(frame.corners.len())
                    .map_err(|_| figure_error(ContourEditError::InvalidValue, Lang::En))?;
                fields.radius = frame.radius.millimetres_f64();
            }
            FigureKind::Arc(segment) => {
                let frame = path.arc_frame(segment).map_err(|error| figure_error(error, Lang::En))?;
                fields.radius = frame.radius.millimetres_f64();
                fields.end_angle = frame.end_angle().map_err(|error| figure_error(error, Lang::En))?.degrees_f64();
            }
        }
        Ok(fields)
    }
}

impl FigureDraft {
    fn new(object: Id, path: &Path, kind: FigureKind) -> Result<Self, String> {
        Ok(Self { object, base: path.clone(), kind, fields: FigureFields::read(kind, path)? })
    }

    /// The path the fields describe, or a readable reason they do not.
    ///
    /// Each setter keeps the shape tag and the ids of the segments it does not re-cut, so applying
    /// one field leaves the figure what it was. The card converts the field's millimetres and
    /// degrees to document units here, once, at the command edge.
    fn parse(&self, lang: Lang) -> Result<Path, String> {
        let mut path = self.base.clone();
        let length =
            |value: f64| scrub::millimetres(value).ok_or_else(|| figure_error(ContourEditError::InvalidValue, lang));
        match self.kind {
            FigureKind::Circle => {
                let radius = length(self.fields.radius)?;
                if radius != path.circle_frame().map_err(|error| figure_error(error, Lang::En))?.radius {
                    path = path.set_circle_radius(radius).map_err(|error| figure_error(error, Lang::En))?;
                }
            }
            FigureKind::Rectangle => {
                let width = length(self.fields.width)?;
                let height = length(self.fields.height)?;
                let frame = path.rectangle_frame().map_err(|error| figure_error(error, Lang::En))?;
                if width != frame.width {
                    path = path.set_rectangle_width(width).map_err(|error| figure_error(error, Lang::En))?;
                }
                if height != frame.height {
                    path = path.set_rectangle_height(height).map_err(|error| figure_error(error, Lang::En))?;
                }
            }
            FigureKind::Polygon => {
                let sides = self.fields.sides;
                let radius = length(self.fields.radius)?;
                let frame = path.polygon_frame().map_err(|error| figure_error(error, Lang::En))?;
                if usize::try_from(sides).ok() != Some(frame.corners.len()) {
                    path = path.set_polygon_sides(sides).map_err(|error| figure_error(error, Lang::En))?;
                }
                let current = path.polygon_frame().map_err(|error| figure_error(error, Lang::En))?.radius;
                if radius != current {
                    path = path.set_polygon_radius(radius).map_err(|error| figure_error(error, Lang::En))?;
                }
            }
            FigureKind::Arc(segment) => {
                let radius = length(self.fields.radius)?;
                let angle = scrub::degrees(self.fields.end_angle)
                    .ok_or_else(|| figure_error(ContourEditError::InvalidValue, lang))?;
                let frame = path.arc_frame(segment).map_err(|error| figure_error(error, Lang::En))?;
                if radius != frame.radius {
                    path = path.set_arc_radius(segment, radius).map_err(|error| figure_error(error, Lang::En))?;
                }
                let current = path
                    .arc_frame(segment)
                    .map_err(|error| figure_error(error, Lang::En))?
                    .end_angle()
                    .map_err(|error| figure_error(error, Lang::En))?;
                if angle != current {
                    path = path.set_arc_end_angle(segment, angle).map_err(|error| figure_error(error, Lang::En))?;
                }
            }
        }
        Ok(path)
    }
}

fn figure_error(error: ContourEditError, lang: Lang) -> String {
    Message::FigureRefused.with(lang, [error.to_string()])
}

/// What the card reports about one object, owned so no document borrow outlives the frame.
#[derive(Clone, Debug, PartialEq)]
struct ObjectSummary {
    name: String,
    kind: Message,
    is_mesh: bool,
    vertices: usize,
    faces: usize,
    contour: Option<Contour>,
    path: Option<Path>,
    height: Option<Length>,
    primitive: Option<Primitive>,
}

fn object_summary(project: &Project, id: Id) -> Option<ObjectSummary> {
    let object = project.object(id)?;
    match object.geometry() {
        Geometry::Mesh(mesh) => Some(ObjectSummary {
            name: object.name().to_owned(),
            kind: Message::PrimitiveBadgeMesh,
            is_mesh: true,
            vertices: mesh.positions().len(),
            faces: mesh.faces().len(),
            contour: None,
            path: None,
            height: None,
            primitive: None,
        }),
        Geometry::Primitive(primitive) => Some(ObjectSummary {
            name: object.name().to_owned(),
            kind: Message::PrimitiveBadgePrimitive,
            is_mesh: false,
            vertices: primitive.vertex_count(),
            faces: primitive.face_count(),
            contour: None,
            path: None,
            height: None,
            primitive: Some(primitive.clone()),
        }),
        Geometry::Constructive(constructive) => Some(ObjectSummary {
            name: object.name().to_owned(),
            kind: Message::PrimitiveBadgeConstructive,
            is_mesh: false,
            vertices: 0,
            faces: 0,
            contour: constructive.contour().ok(),
            path: Some(constructive.path().clone()),
            height: constructive.height(),
            primitive: None,
        }),
        Geometry::Path(path) => Some(ObjectSummary {
            name: object.name().to_owned(),
            kind: Message::PrimitiveBadgePath,
            is_mesh: false,
            vertices: 0,
            faces: 0,
            contour: None,
            path: Some(path.clone()),
            height: None,
            primitive: None,
        }),
    }
}

/// The interface's name for a primitive shape.
fn primitive_kind_label(kind: model::PrimitiveKind) -> Message {
    match kind {
        model::PrimitiveKind::Cuboid => Message::ToolBox,
        model::PrimitiveKind::Cylinder => Message::ToolCylinder,
        model::PrimitiveKind::Sphere => Message::ToolSphere,
    }
}

/// Whether the viewport contour edit mode can reshape this object.
///
/// A constructive object stores a ring and a path stores a chain; a mesh or a primitive stores
/// neither.
fn offers_contour_edit(summary: &ObjectSummary) -> bool {
    summary.contour.is_some() || summary.path.is_some()
}

/// What separates the measures on the head's one line.
///
/// A middle dot is punctuation between two numbers, not a word a translator would change.
const MEASURE_SEPARATOR: &str = " · ";

/// The measures of one object on one line: its shape, and what it is built from.
///
/// Nothing here is editable — every number the card can change is drawn as a field below — so the
/// line never repeats a field.
fn measure_line(summary: &ObjectSummary, lang: Lang) -> String {
    let mut parts = Vec::new();
    if let Some(primitive) = &summary.primitive {
        parts.push(primitive_kind_label(primitive.kind()).text(lang).to_owned());
    }
    if let Some(contour) = &summary.contour {
        parts.push(Message::ContourSegmentsCount.with(lang, [contour.segments().len()]));
        let arcs = contour.segments().filter(|segment| segment.is_arc()).count();
        if arcs > 0 {
            parts.push(Message::ArcsCount.with(lang, [arcs]));
        }
        if let Some(height) = summary.height {
            parts.push(Message::HeightRow.with(lang, [height]));
        }
    } else if let Some(path) = &summary.path {
        parts.push(Message::PathSegmentsCount.with(lang, [path.segments().len()]));
        let arcs = path.segments().filter(|segment| segment.is_arc()).count();
        if arcs > 0 {
            parts.push(Message::ArcsCount.with(lang, [arcs]));
        }
        parts.push(if path.is_closed() { Message::ClosedChain } else { Message::OpenChain }.text(lang).to_owned());
    } else {
        parts.push(Message::VerticesCount.with(lang, [summary.vertices]));
        parts.push(Message::FacesCount.with(lang, [summary.faces]));
    }
    parts.join(MEASURE_SEPARATOR)
}

/// The sentence that says what the document keeps for this kind of object.
fn source_truth(summary: &ObjectSummary) -> Message {
    if summary.primitive.is_some() {
        Message::PrimitiveSourceTruth
    } else if summary.contour.is_some() {
        Message::ContourSourceTruth
    } else if summary.path.is_some() {
        Message::PathSourceTruth
    } else {
        Message::MeshSourceTruth
    }
}

impl ObjectCard {
    /// Draws the card for the selected object and dispatches accepted edits.
    ///
    /// Returns what the user asked to do with sub-object or contour editing; the editor owns both
    /// modes. `contour_editing` tells the card that this object is already being reshaped in the
    /// viewport, so it shows the state instead of the entry button.
    pub(crate) fn show(
        &mut self,
        ui: &mut Ui,
        session: &mut Session,
        object_id: Id,
        mesh_edit: Option<&MeshEdit>,
        contour_editing: bool,
        single_selection: bool,
    ) -> ObjectAction {
        let lang = crate::language(ui.ctx());
        let Some(summary) = object_summary(session.project(), object_id) else {
            self.draft = None;
            self.contour_draft = None;
            self.confirm_collapse = false;
            self.error = None;
            ui.label(Message::SelectNodeInTree.text(lang));
            return ObjectAction::None;
        };
        // One token per frame for the number fields: the edit that takes it is one undo entry.
        let fresh = session.edit_scope();
        if self.draft.as_ref().is_none_or(|draft| draft.object != object_id) {
            self.error = None;
            self.confirm_collapse = false;
            self.contour_draft = None;
            self.primitive_draft = None;
            self.figure_draft = None;
            self.draft = Some(Draft { object: object_id, name: summary.name.clone() });
        }
        // A question about one object is void as soon as the selection is no longer that one.
        if !single_selection {
            self.confirm_collapse = false;
        }
        let is_constructive = summary.contour.is_some();
        if !is_constructive {
            self.contour_draft = None;
        } else if self
            .contour_draft
            .as_ref()
            .is_none_or(|draft| draft.object != object_id || draft.base != summary.contour)
        {
            self.contour_draft = Some(ContourDraft::new(object_id, summary.contour.as_ref()));
        }
        if let Some(primitive) = &summary.primitive {
            if self.primitive_draft.as_ref().is_none_or(|draft| draft.object != object_id || draft.base != *primitive) {
                self.primitive_draft = Some(PrimitiveDraft::new(object_id, primitive));
            }
        } else {
            self.primitive_draft = None;
        }
        self.sync_figure_draft(object_id, &summary);

        // One reading order, top to bottom: which object this is and what it measures, the numbers
        // this kind of geometry can be edited by, one row of actions, and the detail of a mode that
        // runs (ADR 0116). No block carries a header: the card is short enough that a fold only
        // hides the fields a person came for.
        let (rename, name_error) = self.head(ui, &summary, single_selection);
        let (figure_result, figure_report) = self.figure_fields(ui, &summary, contour_editing, fresh);
        // The viewport owns the working copy while the mode is live; a number replacement or a
        // collapse would silently make that copy stale.
        let (primitive_result, primitive_report) = self.primitive_fields(ui, &summary, fresh);
        let contour_result =
            if contour_editing || self.figure_draft.is_some() { None } else { self.contour_text(ui, &summary) };
        let row = self.action_row(ui, object_id, &summary, mesh_edit, contour_editing, single_selection);
        let mut action = row.action;
        if let Some(edit) = mesh_edit.filter(|edit| edit.object() == object_id)
            && let Some(element) = self.mesh_detail(ui, session, object_id, edit)
        {
            action = ObjectAction::Element(element);
        }
        let (collapse, cancel) = (row.collapse, row.cancel);

        // A field that refused its typed text kept its number: the box is red, the card says why.
        if let Some(message) = figure_report.refused.or(primitive_report.refused) {
            self.error = Some(message.text(lang).to_owned());
        }
        for scope in figure_report.cancelled.iter().chain(primitive_report.cancelled.iter()) {
            session.cancel_edit(*scope);
        }
        if let Some(error) = name_error {
            self.error = Some(error);
        }
        if let Some(new_name) = rename {
            match rename_object(session, object_id, new_name) {
                Ok(()) => {
                    self.error = None;
                    self.draft = None;
                }
                Err(error) => self.error = Some(error),
            }
            ui.ctx().request_repaint();
            return action;
        }
        if let Some(result) = figure_result {
            match result.and_then(|path| replace_path(session, object_id, path, figure_report.scope)) {
                Ok(()) => self.error = None,
                Err(error) => {
                    self.error = Some(error);
                    self.figure_draft = None;
                }
            }
            ui.ctx().request_repaint();
            return action;
        }
        if let Some(result) = primitive_result {
            match result.and_then(|primitive| replace_primitive(session, object_id, primitive, primitive_report.scope))
            {
                Ok(()) => self.error = None,
                Err(error) => {
                    self.error = Some(error);
                    self.primitive_draft = None;
                }
            }
            ui.ctx().request_repaint();
            return action;
        }
        if let Some(result) = contour_result {
            match result.and_then(|contour| replace_contour(session, object_id, contour)) {
                Ok(()) => {
                    self.error = None;
                    self.contour_draft = None;
                }
                Err(error) => self.error = Some(error),
            }
            ui.ctx().request_repaint();
            return action;
        }
        if cancel {
            self.confirm_collapse = false;
        }
        if collapse {
            match collapse_object(session, object_id) {
                Ok(()) => {
                    self.error = None;
                    self.confirm_collapse = false;
                    self.contour_draft = None;
                    self.primitive_draft = None;
                }
                Err(error) => self.error = Some(error),
            }
            ui.ctx().request_repaint();
        }

        if let Some(error) = &self.error {
            ui.separator();
            ui.colored_label(theme::danger(ui), error);
        }
        action
    }

    /// Draws the head of the card: what kind of object this is, its name and its measures.
    ///
    /// The badge, one measure line and one sentence say what the node is in three rows, which is
    /// what a header over the same facts cost on its own (ADR 0116).
    fn head(
        &mut self,
        ui: &mut Ui,
        summary: &ObjectSummary,
        single_selection: bool,
    ) -> (Option<String>, Option<String>) {
        let lang = crate::language(ui.ctx());
        let (rename, name_error) = self.name_row(ui, summary);
        ui.label(RichText::new(measure_line(summary, lang)).color(theme::text_dim(ui)));
        ui.label(RichText::new(source_truth(summary).text(lang)).color(theme::text_dim(ui)));
        if !single_selection {
            ui.label(Message::ObjectSelectSingle.text(lang));
        }
        (rename, name_error)
    }

    /// Draws the kind badge and the name field; returns the name to commit and why one was refused.
    ///
    /// The box commits on leaving it, which is the policy every number in the card already has: one
    /// value, one commit, and no second button to press for the same edit (ADR 0092).
    fn name_row(&mut self, ui: &mut Ui, summary: &ObjectSummary) -> (Option<String>, Option<String>) {
        let lang = crate::language(ui.ctx());
        let Some(draft) = self.draft.as_mut() else {
            return (None, None);
        };
        let mut rename = None;
        let mut name_error = None;
        ui.horizontal(|ui| {
            ui.label(RichText::new(summary.kind.text(lang)).strong());
            let width = (ui.available_width() - theme::STEP).max(64.0);
            let response = ui.add(TextEdit::singleline(&mut draft.name).desired_width(width));
            if response.lost_focus() {
                let trimmed = draft.name.trim();
                if trimmed.is_empty() {
                    name_error = Some(Message::ObjectNameEmpty.text(lang).to_owned());
                } else if trimmed != summary.name {
                    rename = Some(trimmed.to_owned());
                }
            }
        });
        (rename, name_error)
    }

    /// Keeps the figure draft in step with the path the document holds.
    ///
    /// The base path detects an edit made elsewhere — by undo, or by the viewport — so the fields
    /// never overwrite a restored figure with stale text.
    fn sync_figure_draft(&mut self, object: Id, summary: &ObjectSummary) {
        let Some(path) = &summary.path else {
            self.figure_draft = None;
            return;
        };
        let kind = FigureKind::of(path);
        let current = self
            .figure_draft
            .as_ref()
            .is_some_and(|draft| draft.object == object && draft.base == *path && Some(draft.kind) == kind);
        if !current {
            self.figure_draft = kind.and_then(|kind| FigureDraft::new(object, path, kind).ok());
        }
    }

    /// Draws the figure's own dimensions; returns the parsed path when a field handed its number over.
    ///
    /// Only the fields the figure has are shown: a circle a radius, a rectangle its width and
    /// height, a polygon its sides and radius, one arc its radius and end angle. While the viewport
    /// owns the working copy the fields are drawn disabled, so a card edit cannot make that copy
    /// stale and lose the drag the user is still making; the action row says why they are dead.
    fn figure_fields(
        &mut self,
        ui: &mut Ui,
        summary: &ObjectSummary,
        contour_editing: bool,
        fresh: EditScope,
    ) -> (Option<Result<Path, String>>, FieldReport) {
        let mut report = FieldReport::default();
        if summary.path.is_none() {
            return (None, report);
        }
        let lang = crate::language(ui.ctx());
        let Some(draft) = self.figure_draft.as_mut() else {
            return (None, report);
        };
        ui.separator();
        ui.add_enabled_ui(!contour_editing, |ui| match draft.kind {
            FigureKind::Circle => {
                report.absorb(length_row(ui, Message::FieldRadius.text(lang), &mut draft.fields.radius, fresh));
            }
            FigureKind::Rectangle => {
                report.absorb(length_row(ui, Message::FieldWidth.text(lang), &mut draft.fields.width, fresh));
                report.absorb(length_row(ui, Message::FieldHeight.text(lang), &mut draft.fields.height, fresh));
            }
            FigureKind::Polygon => {
                report.absorb(count_row(
                    ui,
                    Message::FieldSides.text(lang),
                    &mut draft.fields.sides,
                    MIN_POLYGON_SIDES..=MAX_POLYGON_SIDES,
                    fresh,
                ));
                report.absorb(length_row(ui, Message::FieldRadius.text(lang), &mut draft.fields.radius, fresh));
            }
            FigureKind::Arc(_) => {
                report.absorb(length_row(ui, Message::FieldRadius.text(lang), &mut draft.fields.radius, fresh));
                report.absorb(angle_row(ui, Message::FieldEndAngle.text(lang), &mut draft.fields.end_angle, fresh));
            }
        });
        if !report.committed {
            return (None, report);
        }
        (Some(draft.parse(lang)), report)
    }

    /// Draws the primitive parameters; returns the parsed primitive when a field handed its number over.
    ///
    /// Only the fields the shape has are shown: a box its width, depth and height, a cylinder a
    /// radius, a height and its sides, a sphere a radius, its sides and its bands.
    fn primitive_fields(
        &mut self,
        ui: &mut Ui,
        summary: &ObjectSummary,
        fresh: EditScope,
    ) -> (Option<Result<Primitive, String>>, FieldReport) {
        let mut report = FieldReport::default();
        let Some(primitive) = summary.primitive.as_ref() else {
            return (None, report);
        };
        let lang = crate::language(ui.ctx());
        let Some(draft) = self.primitive_draft.as_mut() else {
            return (None, report);
        };
        ui.separator();
        match primitive {
            Primitive::Cuboid(_) => {
                report.absorb(length_row(ui, Message::FieldWidth.text(lang), &mut draft.fields.width, fresh));
                report.absorb(length_row(ui, Message::FieldDepth.text(lang), &mut draft.fields.depth, fresh));
                report.absorb(length_row(ui, Message::FieldHeight.text(lang), &mut draft.fields.height, fresh));
            }
            Primitive::Cylinder(_) => {
                report.absorb(length_row(ui, Message::FieldRadius.text(lang), &mut draft.fields.radius, fresh));
                report.absorb(length_row(ui, Message::FieldHeight.text(lang), &mut draft.fields.height, fresh));
                report.absorb(count_row(
                    ui,
                    Message::FieldSides.text(lang),
                    &mut draft.fields.segments,
                    MIN_SEGMENTS..=MAX_SEGMENTS,
                    fresh,
                ));
            }
            Primitive::Sphere(_) => {
                report.absorb(length_row(ui, Message::FieldRadius.text(lang), &mut draft.fields.radius, fresh));
                report.absorb(count_row(
                    ui,
                    Message::FieldSides.text(lang),
                    &mut draft.fields.segments,
                    MIN_SEGMENTS..=MAX_SEGMENTS,
                    fresh,
                ));
                report.absorb(count_row(
                    ui,
                    Message::FieldBands.text(lang),
                    &mut draft.fields.bands,
                    MIN_BANDS..=MAX_BANDS,
                    fresh,
                ));
            }
        }
        if !report.committed {
            return (None, report);
        }
        (Some(draft.parse(lang)), report)
    }

    /// Draws the one row of actions this object has, and the sentence that explains it.
    ///
    /// A mesh is edited element by element, a constructive object or a path is reshaped in the
    /// viewport, and only a primitive or a constructive object can be collapsed into a mesh. The
    /// row therefore draws at most one mode button and, apart from the row's right end, one
    /// destructive entry. A multi-selection disables both and the head already says why, so the
    /// refusal is said once instead of once per section (ADR 0116).
    fn action_row(
        &mut self,
        ui: &mut Ui,
        object_id: Id,
        summary: &ObjectSummary,
        mesh_edit: Option<&MeshEdit>,
        contour_editing: bool,
        single_selection: bool,
    ) -> ActionRow {
        let lang = crate::language(ui.ctx());
        let mut row = ActionRow::default();
        ui.separator();
        // A collapse is the one destructive edit of the card: while it asks, the question takes
        // the row's place and nothing else can be pressed.
        if self.confirm_collapse {
            ui.colored_label(theme::warn(ui), Message::CollapseWarning.text(lang));
            ui.horizontal(|ui| {
                if ui.button(Message::CollapseToMesh.text(lang)).clicked() {
                    row.collapse = true;
                }
                if ui.button(Message::Cancel.text(lang)).clicked() {
                    row.cancel = true;
                }
            });
            return row;
        }
        // A collapse would make the viewport's working copy stale, so it is offered only when no
        // contour edit is running (ADR 0116).
        let collapsible = (summary.contour.is_some() || summary.primitive.is_some()) && !contour_editing;
        ui.horizontal(|ui| {
            ui.add_enabled_ui(single_selection, |ui| {
                if summary.is_mesh {
                    let editing = mesh_edit.is_some_and(|edit| edit.object() == object_id);
                    let label = if editing { Message::Done } else { Message::EditMesh };
                    if ui.button(label.text(lang)).clicked() {
                        row.action = ObjectAction::Toggle;
                    }
                } else if offers_contour_edit(summary)
                    && !contour_editing
                    && ui.button(Message::PanelEditContour.text(lang)).clicked()
                {
                    row.action = ObjectAction::StartContour;
                }
            });
            if collapsible {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_enabled_ui(single_selection, |ui| {
                        if ui.button(Message::CollapseToMeshEllipsis.text(lang)).clicked() {
                            self.confirm_collapse = true;
                        }
                    });
                });
            }
        });
        // The state of the mode says itself, in one sentence: the viewport owns the working copy
        // and the card's fields are dead until that copy is applied or dropped.
        if contour_editing {
            ui.label(Message::ObjectContourEditingInViewport.text(lang));
        } else if offers_contour_edit(summary) && single_selection {
            ui.label(RichText::new(Message::DragCornersHint.text(lang)).color(theme::text_dim(ui)));
        }
        row
    }

    /// Draws the detail of the live mesh mode: the element choice and what is selected.
    ///
    /// Returns the element the user switched to, so the editor owns the mode and the card only
    /// reports the press.
    fn mesh_detail(&mut self, ui: &mut Ui, session: &Session, object_id: Id, edit: &MeshEdit) -> Option<MeshElement> {
        let lang = crate::language(ui.ctx());
        let current = edit.element();
        let mut choice = current;
        ui.horizontal(|ui| {
            ui.selectable_value(&mut choice, MeshElement::Vertex, Message::VerticesPlural.text(lang));
            ui.selectable_value(&mut choice, MeshElement::Face, Message::FacesPlural.text(lang));
        });
        match current {
            MeshElement::Vertex => {
                ui.label(Message::SelectedVerticesCount.with(lang, [edit.vertices().len()]));
                let positions = selected_positions(session.project(), object_id, edit.vertices());
                for (vertex, position) in positions.iter().take(MAX_LISTED_VERTICES) {
                    ui.label(format!("v{vertex}  {}", format_position(*position)));
                }
                if positions.len() > MAX_LISTED_VERTICES {
                    let more = Message::counted(
                        positions.len() - MAX_LISTED_VERTICES,
                        Message::MoreCountOne,
                        Message::MoreCountFew,
                        Message::MoreCountMany,
                    );
                    ui.label(more.text(lang));
                }
                ui.label(Message::ClickVertexHint.text(lang));
            }
            MeshElement::Face => {
                ui.label(Message::SelectedFacesCount.with(lang, [edit.faces().len()]));
                let listed: Vec<String> =
                    edit.faces().iter().take(MAX_LISTED_VERTICES).map(|face| format!("f{face}")).collect();
                if !listed.is_empty() {
                    ui.label(listed.join("  "));
                }
                if edit.faces().len() > MAX_LISTED_VERTICES {
                    let more = Message::counted(
                        edit.faces().len() - MAX_LISTED_VERTICES,
                        Message::MoreCountOne,
                        Message::MoreCountFew,
                        Message::MoreCountMany,
                    );
                    ui.label(more.text(lang));
                }
                ui.label(Message::ClickFaceHint.text(lang));
            }
        }
        (choice != current).then_some(choice)
    }

    /// Draws the contour editor; returns the parsed replacement when Apply was pressed.
    ///
    /// The text editor writes corners, so it is only shown for a line-only ring; a ring with an arc
    /// cannot be described by corners and says so instead of silently dropping the arc.
    fn contour_text(&mut self, ui: &mut Ui, summary: &ObjectSummary) -> Option<Result<Contour, String>> {
        let contour = summary.contour.as_ref()?;
        let lang = crate::language(ui.ctx());
        ui.separator();
        if !contour.is_line_only() {
            ui.label(RichText::new(Message::ContourHasArcs.text(lang)).color(theme::text_dim(ui)));
            return None;
        }
        let draft = self.contour_draft.as_mut()?;
        let mut apply = false;
        ui.label(Message::ContourTextHint.with(lang, [contour_corners(contour).map_or(0, |corners| corners.len())]));
        ui.add(TextEdit::multiline(&mut draft.text).desired_rows(6).desired_width(220.0));
        if ui.button(Message::ApplyContour.text(lang)).clicked() {
            apply = true;
        }
        if !apply {
            return None;
        }
        Some(contour_from_text(&draft.text, lang))
    }
}

/// What the one row of actions asked for this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ActionRow {
    /// What the mode button asked the editor to do.
    action: ObjectAction,
    /// The collapse question was confirmed.
    collapse: bool,
    /// The collapse question was dropped.
    cancel: bool,
}

/// What the number fields of one section reported this frame.
///
/// A section draws several fields and at most one of them can hand a number over in a frame, so the
/// card collects their outcomes and dispatches once, after the drawing.
#[derive(Debug, Default)]
struct FieldReport {
    /// A field handed its number over.
    committed: bool,
    /// The undo scope the edit belongs to, minted when the drag or the typing began.
    scope: Option<EditScope>,
    /// Scopes Escape abandoned, which the card restores.
    cancelled: Vec<EditScope>,
    /// Why the last typed text was refused: the box is red and the card says why.
    refused: Option<Message>,
}

impl FieldReport {
    /// Folds one field's outcome into the section's report.
    fn absorb(&mut self, outcome: scrub::FieldOutcome) {
        if outcome.commit {
            self.committed = true;
            self.scope = outcome.scope;
        }
        if let Some(scope) = outcome.cancelled {
            self.cancelled.push(scope);
        }
        if let Some(message) = outcome.error {
            self.refused = Some(message);
        }
    }
}

/// How far a millimetre field of this card moves per pixel of travel.
const LENGTH_SPEED: f64 = 0.5;

/// How far a whole-number field of this card moves per pixel of travel.
///
/// A count spans a few dozen steps, so a slower field lets the hand land on one without overshooting.
const COUNT_SPEED: f64 = 0.05;

/// How far an angle field of this card moves per pixel of travel.
const ANGLE_SPEED: f64 = 1.0;

/// A labelled millimetre field: the shared box with the unit outside it.
///
/// The policy is on-release because a figure or primitive dimension re-cuts geometry and rebuilds a
/// mesh: the field shows the running number, and the document takes it once, at the release. The
/// offered range is the document's own bound on a figure or primitive dimension.
fn length_row(ui: &mut Ui, label: &str, value: &mut f64, fresh: EditScope) -> scrub::FieldOutcome {
    ui.horizontal(|ui| {
        ui.label(label);
        Scrub::new(value)
            .speed(LENGTH_SPEED)
            .range(MIN_PRIMITIVE_SIZE.millimetres_f64()..=MAX_PRIMITIVE_SIZE.millimetres_f64())
            .decimals(3)
            .unit("mm")
            .parse(scrub::parse_millimetres)
            .policy(CommitPolicy::OnRelease)
            .edit(fresh)
            .show(ui)
    })
    .inner
}

/// A labelled whole-number field.
fn count_row(
    ui: &mut Ui,
    label: &str,
    value: &mut u32,
    range: RangeInclusive<u32>,
    fresh: EditScope,
) -> scrub::FieldOutcome {
    ui.horizontal(|ui| {
        ui.label(label);
        Scrub::new(value).speed(COUNT_SPEED).range(range).policy(CommitPolicy::OnRelease).edit(fresh).show(ui)
    })
    .inner
}

/// A labelled degree field: the shared box with `°` outside it.
///
/// A written angle is relative to the arc's own start, so the stored end can sit a full turn away
/// from zero; the offered range is wide enough to show it without clamping.
fn angle_row(ui: &mut Ui, label: &str, value: &mut f64, fresh: EditScope) -> scrub::FieldOutcome {
    ui.horizontal(|ui| {
        ui.label(label);
        Scrub::new(value)
            .speed(ANGLE_SPEED)
            .range(-1_000_000.0..=1_000_000.0)
            .decimals(3)
            .unit("°")
            .parse(scrub::parse_degrees)
            .policy(CommitPolicy::OnRelease)
            .edit(fresh)
            .show(ui)
    })
    .inner
}

/// The selected vertices that still exist, with their local positions.
fn selected_positions(project: &Project, object: Id, vertices: &[u32]) -> Vec<(u32, [Length; 3])> {
    let Some(mesh) = project.object(object).and_then(|object| object.geometry().as_mesh()) else {
        return Vec::new();
    };
    vertices
        .iter()
        .filter_map(|vertex| {
            let index = usize::try_from(*vertex).ok()?;
            mesh.positions().get(index).map(|position| (*vertex, *position))
        })
        .collect()
}

fn format_position(position: [Length; 3]) -> String {
    format!("{}  {}  {}", position[0], position[1], position[2])
}

fn format_contour(contour: &Contour) -> String {
    contour_corners(contour)
        .unwrap_or_default()
        .iter()
        .map(|point| format!("{}, {}", point[0], point[1]))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The corner points of a line-only ring, in travel order; `None` when a segment is an arc.
///
/// The text editor writes corners, so a ring with an arc has no corner list to show: sampling it
/// would silently replace the arc with straight lines on the next Apply.
fn contour_corners(contour: &Contour) -> Option<Vec<[Length; 2]>> {
    contour
        .segments()
        .map(|segment| match segment {
            model::ContourSegment::Line { start, .. } => Some(start),
            model::ContourSegment::Arc { .. } => None,
        })
        .collect()
}

/// Parses one `X, Y` line per contour point, in millimetres.
fn contour_from_text(text: &str, lang: Lang) -> Result<Contour, String> {
    let mut points = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((x, y)) = line.split_once(',') else {
            return Err(Message::LineExpectedPoint.with(lang, [index.saturating_add(1)]));
        };
        let x = Length::parse_millimetres(x.trim()).map_err(|error| {
            Message::LineWithError
                .with(lang, [index.saturating_add(1).to_string(), crate::length_error(error).text(lang).to_owned()])
        })?;
        let y = Length::parse_millimetres(y.trim()).map_err(|error| {
            Message::LineWithError
                .with(lang, [index.saturating_add(1).to_string(), crate::length_error(error).text(lang).to_owned()])
        })?;
        points.push([x, y]);
    }
    Contour::from_polygon(points).map_err(|error| Message::FigureRefused.with(lang, [error.to_string()]))
}

fn rename_object(session: &mut Session, node: Id, name: String) -> Result<(), String> {
    let params = json(&NodeRename { node, name })?;
    dispatch(session, NODE_RENAME_ID, &params).map(|_| ())
}

fn replace_contour(session: &mut Session, object: Id, contour: Contour) -> Result<(), String> {
    let params = json(&ObjectContourReplace { object, path: contour.into() })?;
    dispatch(session, OBJECT_CONTOUR_REPLACE_ID, &params).map(|_| ())
}

/// Replaces the drawing of a path node or the path a constructive body owns.
///
/// The two payloads have their own command, and both store the `Path` itself rather than a ring,
/// so a figure's shape tag and its segment ids survive the edit (ADR 0073, ADR 0074).
fn replace_path(session: &mut Session, object: Id, path: Path, scope: Option<EditScope>) -> Result<(), String> {
    let body = session.project().object(object).is_some_and(|object| object.geometry().as_constructive().is_some());
    if body {
        let params = json(&ObjectContourReplace { object, path })?;
        apply_object_edit(session, OBJECT_CONTOUR_REPLACE_ID, &params, scope)
    } else {
        let params = json(&ObjectPathReplace { object, path })?;
        apply_object_edit(session, OBJECT_PATH_REPLACE_ID, &params, scope)
    }
}

fn replace_primitive(
    session: &mut Session,
    object: Id,
    primitive: Primitive,
    scope: Option<EditScope>,
) -> Result<(), String> {
    let params = json(&ObjectPrimitiveReplace { object, primitive })?;
    apply_object_edit(session, OBJECT_PRIMITIVE_REPLACE_ID, &params, scope)
}

/// Dispatches an object edit, folding it into `scope` when a field gesture is still running.
///
/// The field mints one scope per gesture, so a drag writes the document on every frame it commits
/// and leaves one undo entry; without a scope the edit is a plain command of its own.
fn apply_object_edit(
    session: &mut Session,
    id: &str,
    params: &serde_json::Value,
    scope: Option<EditScope>,
) -> Result<(), String> {
    match scope {
        Some(scope) => session.dispatch_in(scope, id, params).map(|_| ()).map_err(|error| error.to_string()),
        None => dispatch(session, id, params).map(|_| ()),
    }
}

fn collapse_object(session: &mut Session, object: Id) -> Result<(), String> {
    let params = json(&ObjectCollapse { object })?;
    dispatch(session, OBJECT_COLLAPSE_ID, &params).map(|_| ())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use engine::ObjectExtrudeCreate;
    use engine::object_cmds::OBJECT_EXTRUDE_CREATE_ID;
    use model::{Constructive, Placement};
    use testkit::{cube_mesh, millimetres, sample_catalog, sample_project};
    use units::{Degrees, Length};

    use super::*;

    fn rectangle(width: i64, height: i64) -> Contour {
        let millimetres = |value| Length::from_millimetres(value).expect("length");
        Contour::from_polygon(vec![
            [millimetres(0), millimetres(0)],
            [millimetres(width), millimetres(0)],
            [millimetres(width), millimetres(height)],
            [millimetres(0), millimetres(height)],
        ])
        .expect("rectangle")
    }

    /// The same rectangle moved so its bounds centre is the origin, the frame a creation stores.
    fn centred_rectangle(width: i64, height: i64) -> Contour {
        let millimetres = |value| Length::from_millimetres(value).expect("length");
        rectangle(width, height).translated([millimetres(-width / 2), millimetres(-height / 2)]).expect("a moved ring")
    }

    fn object_project() -> (model::Project, Id) {
        let mut project = sample_project();
        let root = project.root().id();
        let mesh = cube_mesh(Length::from_millimetres(100).expect("length"));
        let id = project.add_object(root, "Cube", Placement::IDENTITY, Geometry::Mesh(mesh)).expect("add object");
        (project, id)
    }

    fn constructive_session() -> (Session, Id) {
        let mut session = Session::with_catalog(sample_project(), sample_catalog());
        let value = json(&ObjectExtrudeCreate {
            parent: session.project().root().id(),
            name: "Top".to_owned(),
            contour: rectangle(100, 50),
            height: Length::from_millimetres(18).expect("height"),
            placement: Placement::IDENTITY,
        })
        .expect("params");
        let id = dispatch(&mut session, OBJECT_EXTRUDE_CREATE_ID, &value).expect("create").created_id().expect("id");
        (session, id)
    }

    #[test]
    fn summary_reports_a_mesh_object() {
        let (project, id) = object_project();
        let summary = object_summary(&project, id).expect("summary");
        assert_eq!(summary.kind, Message::PrimitiveBadgeMesh);
        assert_eq!(summary.vertices, 8);
        assert_eq!(summary.faces, 6);
        assert_eq!(summary.name, "Cube");
        assert_eq!(summary.contour, None);
        assert!(!offers_contour_edit(&summary), "a baked mesh has no contour or chain to reshape");
    }

    #[test]
    fn summary_reports_a_constructive_object() {
        let (session, id) = constructive_session();
        let summary = object_summary(session.project(), id).expect("summary");
        assert_eq!(summary.kind, Message::PrimitiveBadgeConstructive);
        assert_eq!(summary.contour, Some(centred_rectangle(100, 50)));
        assert_eq!(
            summary.path.as_ref().map(Path::to_contour),
            Some(Ok(centred_rectangle(100, 50))),
            "the body carries the path it grew from, anchored at its own zero"
        );
        assert_eq!(summary.height, Some(Length::from_millimetres(18).expect("height")));
        assert!(summary.contour.as_ref().is_some_and(|contour| contour.segments().len() == 4));
        assert!(offers_contour_edit(&summary), "a constructive object has a ring to reshape");
    }

    #[test]
    fn summary_reports_a_primitive_object_with_its_parameters() {
        let mut project = sample_project();
        let root = project.root().id();
        let primitive = Primitive::sphere(Length::from_millimetres(40).expect("radius"), 24, 12).expect("sphere");
        let id = project
            .add_object(root, "Ball", Placement::IDENTITY, Geometry::Primitive(primitive.clone()))
            .expect("add object");

        let summary = object_summary(&project, id).expect("summary");
        assert_eq!(summary.kind, Message::PrimitiveBadgePrimitive);
        assert!(!summary.is_mesh, "the mesh is derived, not stored");
        assert_eq!(summary.primitive, Some(primitive));
        assert_eq!(summary.contour, None);
        assert_eq!(summary.path, None);
        assert!(!offers_contour_edit(&summary), "a primitive stores parameters, not a contour");
        assert_eq!(summary.faces, 24 * 12);
        assert_eq!(summary.vertices, 2 + 24 * 11);
    }

    fn primitive_session(primitive: Primitive) -> (Session, Id) {
        let mut project = sample_project();
        let root = project.root().id();
        let id = project.add_object(root, "Shape", Placement::IDENTITY, Geometry::Primitive(primitive)).expect("add");
        let session = Session::with_catalog(project, sample_catalog());
        (session, id)
    }

    #[test]
    fn the_primitive_fields_read_and_parse_the_stored_parameters() {
        let cases = [
            Primitive::cuboid(
                Length::from_millimetres(600).expect("width"),
                Length::from_millimetres(400).expect("depth"),
                Length::from_millimetres(720).expect("height"),
            )
            .expect("box"),
            Primitive::cylinder(
                Length::from_millimetres(25).expect("radius"),
                Length::from_millimetres(80).expect("height"),
                20,
            )
            .expect("cylinder"),
            Primitive::sphere(Length::from_millimetres(40).expect("radius"), 24, 12).expect("sphere"),
        ];
        for primitive in cases {
            let draft = PrimitiveDraft::new(Id::new(1).expect("id"), &primitive);
            assert_eq!(draft.fields, PrimitiveFields::read(&primitive), "{primitive:?} reads back");
            assert_eq!(draft.parse(Lang::En).as_ref(), Ok(&primitive), "{primitive:?} parses from its own numbers");
        }
    }

    #[test]
    fn the_primitive_fields_refuse_a_value_the_document_cannot_hold() {
        let box_ = Primitive::cuboid(
            Length::from_millimetres(100).expect("width"),
            Length::from_millimetres(100).expect("depth"),
            Length::from_millimetres(100).expect("height"),
        )
        .expect("box");
        let mut draft = PrimitiveDraft::new(Id::new(1).expect("id"), &box_);
        draft.fields.width = 0.0;
        assert!(draft.parse(Lang::En).is_err(), "a zero side is not a solid");
        draft.fields.width = 100.0;
        draft.fields.depth = 2_000_000.0;
        assert!(draft.parse(Lang::En).is_err(), "a two-hundred-kilometre side is past the document's cap");

        let sphere = Primitive::sphere(Length::from_millimetres(40).expect("radius"), 24, 12).expect("sphere");
        let mut draft = PrimitiveDraft::new(Id::new(1).expect("id"), &sphere);
        draft.fields.segments = 2;
        assert!(draft.parse(Lang::En).is_err(), "two sides are not a ring");
        draft.fields.segments = 24;
        draft.fields.bands = 1;
        assert!(draft.parse(Lang::En).is_err(), "one band has no surface");
    }

    #[test]
    fn applying_new_parameters_is_one_command_and_keeps_the_id() {
        let original = Primitive::cuboid(
            Length::from_millimetres(600).expect("width"),
            Length::from_millimetres(400).expect("depth"),
            Length::from_millimetres(720).expect("height"),
        )
        .expect("box");
        let (mut session, id) = primitive_session(original.clone());
        let before = session.project().revision();
        let wider = Primitive::cuboid(
            Length::from_millimetres(700).expect("width"),
            Length::from_millimetres(400).expect("depth"),
            Length::from_millimetres(720).expect("height"),
        )
        .expect("box");
        replace_primitive(&mut session, id, wider.clone(), None).expect("replace the parameters");
        assert_eq!(session.project().revision(), before.checked_next().expect("revision"), "one revision per commit");
        assert_eq!(session.project().object_primitive(id), Ok(&wider));
        assert_eq!(session.project().object(id).expect("object").name(), "Shape", "only the parameters change");

        session.undo().expect("undo");
        assert_eq!(session.project().object_primitive(id), Ok(&original));
    }

    #[test]
    fn a_dragged_primitive_field_is_one_undo_entry() {
        let box_ = |width: f64| {
            Primitive::cuboid(
                scrub::millimetres(width).expect("a length"),
                Length::from_millimetres(400).expect("depth"),
                Length::from_millimetres(720).expect("height"),
            )
            .expect("box")
        };
        let original = box_(600.0);
        let (mut session, id) = primitive_session(original.clone());
        let scope = session.edit_scope();
        for width in [620.0, 640.0, 660.0] {
            replace_primitive(&mut session, id, box_(width), Some(scope))
                .expect("the field writes while the hand moves");
        }
        session.end_edit();
        assert_eq!(session.project().object_primitive(id), Ok(&box_(660.0)));
        session.undo().expect("undo the drag");
        assert_eq!(session.project().object_primitive(id), Ok(&original), "one undo returns the whole drag");
    }

    #[test]
    fn collapsing_a_primitive_is_undoable() {
        let primitive = Primitive::sphere(Length::from_millimetres(40).expect("radius"), 16, 8).expect("sphere");
        let (mut session, id) = primitive_session(primitive);
        collapse_object(&mut session, id).expect("collapse");
        let object = session.project().object(id).expect("object");
        assert!(object.geometry().as_mesh().is_some(), "collapse bakes the derived mesh");
        assert!(object.geometry().as_primitive().is_none(), "the parameters are dropped");
        assert!(collapse_object(&mut session, id).is_err(), "a baked mesh cannot collapse again");

        session.undo().expect("undo");
        assert!(session.project().object(id).expect("object").geometry().as_primitive().is_some());
    }

    #[test]
    fn an_unknown_id_has_no_summary() {
        let (project, _) = object_project();
        assert_eq!(object_summary(&project, Id::new(99).expect("id")), None);
    }

    #[test]
    fn summary_reports_a_path_object_as_a_flat_drawing() {
        use model::{ContourSegment, Path};

        let mut project = sample_project();
        let root = project.root().id();
        let millimetres = |value: i64| Length::from_millimetres(value).expect("length");
        let path = Path::new(
            vec![
                ContourSegment::line([millimetres(0), millimetres(0)], [millimetres(100), millimetres(0)])
                    .expect("line"),
                ContourSegment::line([millimetres(100), millimetres(0)], [millimetres(100), millimetres(50)])
                    .expect("line"),
            ],
            false,
        )
        .expect("open path");
        let id = project.add_object(root, "Sketch", Placement::IDENTITY, Geometry::Path(path)).expect("add");

        let summary = object_summary(&project, id).expect("summary");
        assert_eq!(summary.kind, Message::PrimitiveBadgePath);
        assert!(!summary.is_mesh, "a path has no editable mesh");
        assert_eq!(summary.contour, None);
        assert_eq!(summary.path.as_ref().map(|path| path.segments().len()), Some(2));
        assert_eq!(summary.path.as_ref().map(Path::is_closed), Some(false));
        assert!(offers_contour_edit(&summary), "a path chain is reshaped in the same viewport mode");
    }

    #[test]
    fn selected_positions_skip_indices_the_mesh_no_longer_has() {
        let (project, id) = object_project();
        let positions = selected_positions(&project, id, &[0, 3, 8]);
        assert_eq!(positions.len(), 2, "vertex 8 is outside a cube");
        assert_eq!(positions[0].0, 0);
        assert_eq!(positions[1].0, 3);
        let cube = cube_mesh(Length::from_millimetres(100).expect("length"));
        assert_eq!(positions[0].1, cube.positions()[0]);
        assert_eq!(positions[1].1, cube.positions()[3]);
        assert!(format_position(positions[0].1).ends_with(" mm"));
    }

    #[test]
    fn a_contour_text_round_trips_and_rejects_bad_lines() {
        let contour = rectangle(100, 50);
        let text = format_contour(&contour);
        assert_eq!(contour_from_text(&text, Lang::En), Ok(contour));

        assert!(contour_from_text("0, 0\n100, 0", Lang::En).is_err(), "a contour needs three points");
        assert!(contour_from_text("wide, 0\n100, 0\n100, 50", Lang::En).is_err(), "text must parse");
        assert!(contour_from_text("0 0\n100 0\n100 50", Lang::En).is_err(), "a comma separates the axes");
        assert!(
            contour_from_text("0, 0\n30, 0\n20, 20\n10, -10", Lang::En).is_err(),
            "a self-intersecting contour is refused"
        );
    }

    #[test]
    fn a_contour_with_an_arc_has_no_corner_text() {
        use model::ContourSegment;
        use units::Degrees;

        let millimetres = |value: i64| Length::from_millimetres(value).expect("length");
        let arc = ContourSegment::arc(
            [Length::ZERO, Length::ZERO],
            millimetres(100),
            Degrees::ZERO,
            Degrees::from_millidegrees(90_000),
        )
        .expect("quarter arc");
        let contour = Contour::new(vec![
            arc,
            ContourSegment::line([Length::ZERO, millimetres(100)], [Length::ZERO, Length::ZERO]).expect("line"),
            ContourSegment::line([Length::ZERO, Length::ZERO], [millimetres(100), Length::ZERO]).expect("line"),
        ])
        .expect("a quarter disc");
        assert!(!contour.is_line_only());
        assert_eq!(contour_corners(&contour), None, "corners cannot describe the arc");
        assert_eq!(format_contour(&contour), "", "the text editor has nothing to show");
    }

    #[test]
    fn renaming_an_object_dispatches_through_the_engine() {
        let (project, id) = object_project();
        let mut session = Session::with_catalog(project, sample_catalog());
        rename_object(&mut session, id, "Bracket".to_owned()).expect("rename");
        assert_eq!(session.project().object(id).expect("object").name(), "Bracket");
        session.undo().expect("undo");
        assert_eq!(session.project().object(id).expect("object").name(), "Cube");
    }

    #[test]
    fn replacing_the_contour_is_one_command_and_keeps_the_id() {
        let (mut session, id) = constructive_session();
        let before = session.project().revision();
        replace_contour(&mut session, id, rectangle(120, 60)).expect("replace");
        assert_eq!(session.project().revision(), before.checked_next().expect("revision"));
        let object = session.project().object(id).expect("object");
        assert_eq!(object.geometry().as_constructive().expect("constructive").contour(), Ok(rectangle(120, 60)));
        session.undo().expect("undo");
        assert_eq!(
            session.project().object(id).expect("object").geometry().as_constructive().expect("constructive").contour(),
            Ok(centred_rectangle(100, 50))
        );
    }

    #[test]
    fn collapsing_a_constructive_object_is_undoable() {
        let (mut session, id) = constructive_session();
        collapse_object(&mut session, id).expect("collapse");
        let object = session.project().object(id).expect("object");
        assert!(object.geometry().as_mesh().is_some());
        assert!(object.geometry().as_constructive().is_none());
        assert!(collapse_object(&mut session, id).is_err(), "a mesh object cannot collapse again");
        session.undo().expect("undo");
        assert!(session.project().object(id).expect("object").geometry().as_constructive().is_some());
    }

    #[test]
    fn extrude_parameters_create_the_object_the_card_expects() {
        let mut session = Session::with_catalog(sample_project(), sample_catalog());
        let value = json(&ObjectExtrudeCreate {
            parent: session.project().root().id(),
            name: "Top".to_owned(),
            contour: rectangle(100, 50),
            height: Length::from_millimetres(18).expect("height"),
            placement: Placement::IDENTITY,
        })
        .expect("params");
        let id = dispatch(&mut session, OBJECT_EXTRUDE_CREATE_ID, &value).expect("create").created_id().expect("id");
        let summary = object_summary(session.project(), id).expect("summary");
        assert_eq!(summary.kind, Message::PrimitiveBadgeConstructive);
        assert_eq!(summary.height, Some(Length::from_millimetres(18).expect("height")));
    }

    fn path_project(path: Path) -> (Session, Id) {
        let mut project = sample_project();
        let root = project.root().id();
        let id = project.add_object(root, "Figure", Placement::IDENTITY, Geometry::Path(path)).expect("add object");
        (Session::with_catalog(project, sample_catalog()), id)
    }

    fn figure_draft(id: Id, path: &Path) -> FigureDraft {
        let kind = FigureKind::of(path).expect("the path has a figure");
        FigureDraft::new(id, path, kind).expect("draft")
    }

    #[test]
    fn the_figure_fields_read_and_parse_a_circle() {
        let path = Path::circle([Length::ZERO, Length::ZERO], millimetres(40)).expect("circle");
        let mut draft = figure_draft(Id::new(1).expect("id"), &path);
        assert_eq!(draft.fields.radius, 40.0);

        draft.fields.radius = 60.0;
        let parsed = draft.parse(Lang::En).expect("a wider circle");
        assert_eq!(parsed.shape(), Some(PathShape::Circle), "the tag survives");
        assert_eq!(parsed.circle_frame().expect("frame").radius, millimetres(60));
        assert_eq!(parsed.entries().len(), 4, "a circle keeps its four quarter arcs");
    }

    #[test]
    fn the_figure_fields_read_and_parse_a_rectangle() {
        let path =
            Path::rectangle([Length::ZERO, Length::ZERO], [millimetres(100), millimetres(50)]).expect("rectangle");
        let mut draft = figure_draft(Id::new(1).expect("id"), &path);
        assert_eq!(draft.fields.width, 100.0);
        assert_eq!(draft.fields.height, 50.0);

        draft.fields.width = 130.0;
        draft.fields.height = 70.0;
        let parsed = draft.parse(Lang::En).expect("a wider rectangle");
        assert_eq!(parsed.shape(), Some(PathShape::Rectangle), "the tag survives");
        let frame = parsed.rectangle_frame().expect("frame");
        assert_eq!((frame.width, frame.height), (millimetres(130), millimetres(70)));
    }

    #[test]
    fn the_figure_fields_read_and_parse_a_polygon() {
        let path =
            Path::regular_polygon([Length::ZERO, Length::ZERO], [millimetres(30), Length::ZERO], 6).expect("hexagon");
        let mut draft = figure_draft(Id::new(1).expect("id"), &path);
        assert_eq!(draft.fields.sides, 6);
        assert_eq!(draft.fields.radius, 30.0);

        draft.fields.sides = 8;
        draft.fields.radius = 50.0;
        let parsed = draft.parse(Lang::En).expect("an octagon");
        assert_eq!(parsed.shape(), Some(PathShape::RegularPolygon), "the tag survives");
        let frame = parsed.polygon_frame().expect("frame");
        assert_eq!(frame.corners.len(), 8, "the side count is re-cut");
        assert_eq!(frame.radius, millimetres(50), "the radius follows the field's number");
    }

    #[test]
    fn the_figure_fields_read_and_parse_a_single_arc() {
        let path = Path::arc_from_center(
            [Length::ZERO, Length::ZERO],
            [millimetres(50), Length::ZERO],
            [Length::ZERO, millimetres(50)],
        )
        .expect("quarter arc");
        let segment = path.entries().first().expect("one segment").id();
        let mut draft = figure_draft(Id::new(1).expect("id"), &path);
        assert_eq!(draft.fields.radius, 50.0);
        assert_eq!(draft.fields.end_angle, 90.0);

        draft.fields.radius = 70.0;
        draft.fields.end_angle = 45.0;
        let parsed = draft.parse(Lang::En).expect("a shorter, wider arc");
        let frame = parsed.arc_frame(segment).expect("the segment id survives");
        assert_eq!(frame.radius, millimetres(70));
        assert_eq!(frame.end_angle().expect("end angle"), Degrees::from_millidegrees(45_000));
    }

    #[test]
    fn the_figure_fields_refuse_a_value_the_document_cannot_hold() {
        let path = Path::circle([Length::ZERO, Length::ZERO], millimetres(40)).expect("circle");
        let mut circle = figure_draft(Id::new(1).expect("id"), &path);
        circle.fields.radius = 0.0;
        assert!(circle.parse(Lang::En).is_err(), "a circle needs a positive radius");

        let polygon =
            Path::regular_polygon([Length::ZERO, Length::ZERO], [millimetres(30), Length::ZERO], 6).expect("hexagon");
        let mut draft = figure_draft(Id::new(1).expect("id"), &polygon);
        draft.fields.sides = 2;
        assert!(draft.parse(Lang::En).is_err(), "two sides are not a polygon");
        draft.fields.sides = 129;
        assert!(draft.parse(Lang::En).is_err(), "the polygon cap is 128 sides");

        let arc = Path::arc_from_center(
            [Length::ZERO, Length::ZERO],
            [millimetres(50), Length::ZERO],
            [Length::ZERO, millimetres(50)],
        )
        .expect("quarter arc");
        let mut draft = figure_draft(Id::new(1).expect("id"), &arc);
        draft.fields.end_angle = 0.0;
        assert!(draft.parse(Lang::En).is_err(), "an end angle on the start would have no sweep");
    }

    #[test]
    fn applying_a_figure_is_one_command_and_one_undo_entry() {
        let path = Path::circle([Length::ZERO, Length::ZERO], millimetres(40)).expect("circle");
        let (mut session, id) = path_project(path.clone());
        let before = session.project().revision();
        let mut draft = figure_draft(id, &path);
        draft.fields.radius = 70.0;
        replace_path(&mut session, id, draft.parse(Lang::En).expect("parse"), None).expect("replace the figure");

        assert_eq!(session.project().revision(), before.checked_next().expect("revision"), "one revision per commit");
        assert_eq!(
            session.project().object_path(id).expect("path").circle_frame().expect("frame").radius,
            millimetres(70)
        );
        session.undo().expect("undo");
        assert_eq!(
            session.project().object_path(id).expect("path").circle_frame().expect("frame").radius,
            millimetres(40),
            "one undo restores the figure"
        );
    }

    #[test]
    fn a_dragged_figure_field_is_one_undo_entry() {
        let path = Path::circle([Length::ZERO, Length::ZERO], millimetres(40)).expect("circle");
        let (mut session, id) = path_project(path.clone());
        let scope = session.edit_scope();
        for radius in [50.0, 60.0, 70.0] {
            let mut draft = figure_draft(id, &path);
            draft.fields.radius = radius;
            let parsed = draft.parse(Lang::En).expect("a wider circle");
            replace_path(&mut session, id, parsed, Some(scope)).expect("the field writes while the hand moves");
        }
        session.end_edit();
        assert_eq!(
            session.project().object_path(id).expect("path").circle_frame().expect("frame").radius,
            millimetres(70)
        );
        session.undo().expect("undo the drag");
        assert_eq!(
            session.project().object_path(id).expect("path").circle_frame().expect("frame").radius,
            millimetres(40),
            "one undo returns the whole drag"
        );
    }

    #[test]
    fn a_constructive_body_keeps_the_tag_of_the_figure_it_owns() {
        use model::Constructive;

        let path =
            Path::rectangle([Length::ZERO, Length::ZERO], [millimetres(100), millimetres(50)]).expect("rectangle");
        let body = Constructive::extrude(path.clone(), millimetres(18)).expect("body");
        let mut project = sample_project();
        let root = project.root().id();
        let id =
            project.add_object(root, "Top", Placement::IDENTITY, Geometry::Constructive(body)).expect("add the body");
        let mut session = Session::with_catalog(project, sample_catalog());

        let summary = object_summary(session.project(), id).expect("summary");
        let stored = summary.path.as_ref().expect("a body carries the path it owns");
        assert_eq!(stored, &path);
        assert_eq!(summary.contour.as_ref(), Some(&rectangle(100, 50)), "the ring is still reported");

        let mut draft = figure_draft(id, stored);
        draft.fields.width = 140.0;
        replace_path(&mut session, id, draft.parse(Lang::En).expect("parse"), None).expect("replace the body path");
        let body = session.project().object(id).expect("object").geometry().as_constructive().expect("body");
        assert_eq!(body.path().shape(), Some(PathShape::Rectangle), "the tag survives the card edit");
        assert_eq!(body.contour().expect("ring").bounds_micrometres(), ([-20_000, 0], [120_000, 50_000]));
    }

    /// Draws the object card for one session and returns the texts it painted.
    fn drew_card(session: &mut Session, id: Id) -> Vec<String> {
        drew_card_in_mode(session, id, false)
    }

    /// Draws the object card with the viewport either owning the working copy or not.
    fn drew_card_in_mode(session: &mut Session, id: Id, contour_editing: bool) -> Vec<String> {
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        crate::set_language(&ctx, Lang::En);
        let mut card = ObjectCard::default();
        let output = ctx.run_ui(Default::default(), |ui| {
            card.show(ui, session, id, None, contour_editing, true);
        });
        let texts = drawn_text(&output.shapes);
        output.drop_without_applying_deltas();
        texts
    }

    /// Whether the card drew a text that contains `needle`.
    fn drew(texts: &[String], needle: &str) -> bool {
        texts.iter().any(|text| text.contains(needle))
    }

    /// How many drawn texts say exactly `needle`, which a field label does once.
    fn count_exactly(texts: &[String], needle: &str) -> usize {
        texts.iter().filter(|text| *text == needle).count()
    }

    #[test]
    fn a_live_contour_edit_states_itself_once_and_offers_no_collapse() {
        let (mut session, id) = constructive_session();
        let texts = drew_card_in_mode(&mut session, id, true);
        assert!(texts.iter().any(|text| text.starts_with("Editing the contour in the viewport")), "{texts:?}");
        assert_eq!(
            texts.iter().filter(|text| text.contains("viewport")).count(),
            1,
            "one state, one sentence: {texts:?}"
        );
        assert!(!drew(&texts, "Collapse"), "a collapse would make the working copy stale: {texts:?}");
        assert!(!drew(&texts, "Edit contour"), "the mode is already running: {texts:?}");
    }

    #[test]
    fn the_card_draws_only_the_blocks_the_selected_kind_can_use() {
        let (project, id) = object_project();
        let mut mesh = Session::with_catalog(project, sample_catalog());
        let mesh = drew_card(&mut mesh, id);
        assert!(drew(&mesh, "Edit mesh"), "a mesh is edited element by element: {mesh:?}");
        assert!(drew(&mesh, "Vertices"), "and its counts are its measure: {mesh:?}");
        assert!(!drew(&mesh, "Edit contour"), "a baked mesh has no ring to reshape: {mesh:?}");
        assert!(!drew(&mesh, "Collapse"), "a mesh is already a mesh: {mesh:?}");
        assert!(!drew(&mesh, "Width"), "and it stores no parameter to type: {mesh:?}");

        let cuboid = Primitive::cuboid(millimetres(600), millimetres(400), millimetres(720)).expect("box");
        let (mut primitive, id) = primitive_session(cuboid);
        let primitive = drew_card(&mut primitive, id);
        for field in ["Width", "Depth", "Height"] {
            assert_eq!(count_exactly(&primitive, field), 1, "a box has one `{field}` field: {primitive:?}");
        }
        assert!(drew(&primitive, "Collapse to mesh"), "a primitive can be baked into a mesh: {primitive:?}");
        assert!(
            !drew(&primitive, "Edit mesh"),
            "its elements are derived, so it cannot be edited as one: {primitive:?}"
        );
        assert!(!drew(&primitive, "Edit contour"), "and it has no ring to reshape: {primitive:?}");

        // A body grown from a tagged figure is edited by its fields, and one grown from a bare ring
        // by its corners; neither offers the other's editor.
        let figure = Path::rectangle([Length::ZERO, Length::ZERO], [millimetres(100), millimetres(50)]).expect("ring");
        let body = Constructive::extrude(figure, millimetres(18)).expect("body");
        let mut project = sample_project();
        let root = project.root().id();
        let id = project.add_object(root, "Top", Placement::IDENTITY, Geometry::Constructive(body)).expect("body");
        let mut figure_session = Session::with_catalog(project, sample_catalog());
        let figure = drew_card(&mut figure_session, id);
        assert!(drew(&figure, "Edit contour"), "a body is reshaped in the viewport: {figure:?}");
        assert!(drew(&figure, "Collapse to mesh"), "and can be baked: {figure:?}");
        assert!(!drew(&figure, "Edit mesh"), "but not before it is baked: {figure:?}");
        assert!(!drew(&figure, "Apply contour"), "a tagged figure is edited by its fields: {figure:?}");
        for field in ["Width", "Height"] {
            assert_eq!(count_exactly(&figure, field), 1, "a rectangle has one `{field}` field: {figure:?}");
        }

        let (mut ring, id) = constructive_session();
        let ring = drew_card(&mut ring, id);
        assert!(drew(&ring, "Apply contour"), "a ring with no figure tag is edited as corners: {ring:?}");
        assert!(!drew(&ring, "Edit mesh"), "and still cannot be edited element by element: {ring:?}");

        let path =
            Path::rectangle([Length::ZERO, Length::ZERO], [millimetres(100), millimetres(50)]).expect("rectangle");
        let (mut path_session, id) = path_project(path);
        let path = drew_card(&mut path_session, id);
        assert!(drew(&path, "Edit contour"), "a drawing is reshaped in the viewport: {path:?}");
        assert!(!drew(&path, "Collapse"), "a drawing has no parameters to bake: {path:?}");
        assert!(!drew(&path, "Edit mesh"), "and no mesh to edit: {path:?}");
    }

    #[test]
    fn the_figure_card_draws_a_number_and_its_unit_and_no_apply() {
        let path =
            Path::rectangle([Length::ZERO, Length::ZERO], [millimetres(100), millimetres(50)]).expect("rectangle");
        let (mut session, id) = path_project(path);
        let texts = drew_card(&mut session, id);
        assert!(texts.iter().any(|text| text == "100"), "the width is the field's own number: {texts:?}");
        assert!(texts.iter().any(|text| text == "50"), "the height too: {texts:?}");
        assert!(texts.iter().any(|text| text == "mm"), "and its unit is drawn outside the box: {texts:?}");
        assert!(
            !texts.iter().any(|text| text.contains("Apply")),
            "a number no longer waits behind an Apply button: {texts:?}"
        );
    }

    #[test]
    fn the_primitive_card_draws_numbers_and_their_units_and_no_apply() {
        let primitive = Primitive::cylinder(millimetres(25), millimetres(80), 20).expect("the card has fields for it");
        let (mut session, id) = primitive_session(primitive);
        let texts = drew_card(&mut session, id);
        assert!(texts.iter().any(|text| text == "25"), "the radius is the field's own number: {texts:?}");
        assert!(texts.iter().any(|text| text == "80"), "the height too: {texts:?}");
        assert!(texts.iter().any(|text| text == "20"), "and the side count: {texts:?}");
        assert!(texts.iter().any(|text| text == "mm"), "the unit is drawn outside the box: {texts:?}");
        assert!(
            !texts.iter().any(|text| text.contains("Apply")),
            "a parameter no longer waits behind an Apply button: {texts:?}"
        );
    }

    fn drawn_text(shapes: &[egui::epaint::ClippedShape]) -> Vec<String> {
        use egui::epaint::Shape;
        fn walk(shape: &Shape, out: &mut Vec<String>) {
            match shape {
                Shape::Text(text) => out.push(text.galley.text().to_owned()),
                Shape::Vec(shapes) => {
                    for shape in shapes {
                        walk(shape, out);
                    }
                }
                _ => {}
            }
        }
        let mut out = Vec::new();
        for clipped in shapes {
            walk(&clipped.shape, &mut out);
        }
        out
    }

    fn card_texts(lang: Lang) -> (Vec<String>, u64, bool) {
        let (mut session, id) = constructive_session();
        let revision = session.project().revision().get();
        let undo = session.can_undo();
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        crate::set_language(&ctx, lang);
        let mut card = ObjectCard::default();
        let output = ctx.run_ui(Default::default(), |ui| {
            card.show(ui, &mut session, id, None, false, true);
        });
        let texts = drawn_text(&output.shapes);
        output.drop_without_applying_deltas();
        (texts, revision, undo)
    }

    #[test]
    fn switching_the_language_redraws_a_card_without_touching_history() {
        let (english, english_revision, english_undo) = card_texts(Lang::En);
        let (russian, russian_revision, russian_undo) = card_texts(Lang::Ru);
        assert!(english.iter().any(|text| text.starts_with("Contour segments")), "{english:?}");
        assert!(russian.iter().any(|text| text.starts_with("Сегментов контура")), "{russian:?}");
        assert_ne!(english, russian, "the same card draws different words");
        assert_eq!(english_revision, russian_revision, "a language switch moves no revision");
        assert_eq!(english_undo, russian_undo, "a language switch leaves the undo stack alone");
    }
}
