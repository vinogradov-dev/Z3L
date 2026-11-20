//! Free-form geometry objects: a baked mesh, a parametric primitive, a path or a contour with a
//! feature history.
//!
//! An object is a leaf in the container tree, placed like a panel but carrying no manufacturing
//! rules. A constructive payload stores the features it was built from; a primitive stores the
//! parameters that build its mesh. In both cases the mesh is derived and replaces the parameters
//! only when the user collapses it. A path payload is a flat drawing that has no solid yet.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use units::Length;

use crate::contour::{Contour, ContourError, MAX_CONTOUR_COORDINATE, Path, PathError};
use crate::mesh::Mesh;
use crate::primitive::Primitive;
use crate::{Id, PivotLocal, Placement, Properties, PropertyError, PropertyValue};

/// Largest number of features in a constructive object.
pub const MAX_OBJECT_FEATURES: usize = 32;

/// Largest number of rings in one contour set.
pub const MAX_CONTOURS: usize = 64;

/// Serialized form of [`Extrude`]; validation happens in its `TryFrom` implementation.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExtrudeWire {
    height: Length,
}

/// The first feature: the constructive path swept along `+Z` from `z = 0` to `z = height`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "ExtrudeWire")]
pub struct Extrude {
    height: Length,
}

impl Extrude {
    /// Validates the height and stores the feature.
    pub fn new(height: Length) -> Result<Self, ConstructiveError> {
        if !height.is_positive() || height.micrometres() > MAX_CONTOUR_COORDINATE.micrometres() {
            return Err(ConstructiveError::InvalidHeight { height, max: MAX_CONTOUR_COORDINATE });
        }
        Ok(Self { height })
    }

    /// The extrusion height, positive.
    pub const fn height(&self) -> Length {
        self.height
    }
}

impl TryFrom<ExtrudeWire> for Extrude {
    type Error = ConstructiveError;

    fn try_from(wire: ExtrudeWire) -> Result<Self, Self::Error> {
        Self::new(wire.height)
    }
}

/// One step of a constructive object's feature history.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Feature {
    /// Extrusion of the constructive path.
    Extrude(Extrude),
}

/// Serialized form of [`Feature`]: an extrusion of the constructive path.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum FeatureWire {
    Extrude { height: Length },
}

/// Serialized form of [`Constructive`]; validation happens in its `TryFrom` implementation.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConstructiveWire {
    path: Path,
    #[serde(default)]
    features: Vec<FeatureWire>,
}

/// A constructive geometry payload: the path it grew from and an ordered feature history.
///
/// The path and the feature parameters are the source of truth; the mesh is never stored in the
/// document and is not editable until [`Project::collapse_object`] bakes it ([ADR 0037]). An
/// extrusion owns its path: the body stores the chain the user drew, not a second copy of the ring
/// it encloses, so editing the drawing edits the body ([ADR 0074]).
///
/// [ADR 0037]: ../docs/decisions/0037-a-geometry-object-is-a-mesh-or-a-feature-history.md
/// [ADR 0074]: ../docs/decisions/0074-an-extrusion-owns-the-path-it-grew-from.md
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "ConstructiveWire")]
pub struct Constructive {
    path: Path,
    features: Vec<Feature>,
}

impl Constructive {
    /// Validates the path and the feature list and stores them.
    ///
    /// The first feature must be an extrusion, so the path must be a closed ring the document
    /// accepts; an open chain is refused before the document changes.
    pub fn new(path: Path, features: Vec<Feature>) -> Result<Self, ConstructiveError> {
        if features.is_empty() || features.len() > MAX_OBJECT_FEATURES {
            return Err(ConstructiveError::FeatureCount { count: features.len(), max: MAX_OBJECT_FEATURES });
        }
        if !matches!(features.first(), Some(Feature::Extrude(_))) {
            return Err(ConstructiveError::MissingExtrude);
        }
        if !path.is_closed() {
            return Err(ConstructiveError::OpenPath);
        }
        path.to_contour()?;
        Ok(Self { path, features })
    }

    /// Creates an object with a single extrusion of `path`.
    pub fn extrude(path: Path, height: Length) -> Result<Self, ConstructiveError> {
        Self::new(path, vec![Feature::Extrude(Extrude::new(height)?)])
    }

    /// The path the body grew from.
    pub const fn path(&self) -> &Path {
        &self.path
    }

    /// Replaces the path, keeping every feature.
    pub fn set_path(&mut self, path: Path) -> Result<(), ConstructiveError> {
        if !path.is_closed() {
            return Err(ConstructiveError::OpenPath);
        }
        path.to_contour()?;
        self.path = path;
        Ok(())
    }

    /// The ordered features, at least one.
    pub fn features(&self) -> &[Feature] {
        &self.features
    }

    /// The first extrusion, which every constructive object has.
    pub fn extrude_feature(&self) -> Option<&Extrude> {
        match self.features.first() {
            Some(Feature::Extrude(extrude)) => Some(extrude),
            None => None,
        }
    }

    /// The ring the stored path encloses, derived on read.
    ///
    /// The path is validated when it is stored, so this only fails on a document that was built
    /// without the constructor; it is the same ring the mesh is built from.
    pub fn contour(&self) -> Result<Contour, PathError> {
        self.path.to_contour()
    }

    /// The height of the first extrusion.
    pub fn height(&self) -> Option<Length> {
        self.extrude_feature().map(Extrude::height)
    }

    /// Replaces the height of the first extrusion, keeping the path and every other feature.
    pub fn set_height(&mut self, height: Length) -> Result<(), ConstructiveError> {
        let Some(Feature::Extrude(extrude)) = self.features.first_mut() else {
            return Err(ConstructiveError::MissingExtrude);
        };
        *extrude = Extrude::new(height)?;
        Ok(())
    }
}

impl TryFrom<ConstructiveWire> for Constructive {
    type Error = ConstructiveError;

    fn try_from(wire: ConstructiveWire) -> Result<Self, Self::Error> {
        let mut features = Vec::with_capacity(wire.features.len());
        for feature in wire.features {
            let FeatureWire::Extrude { height } = feature;
            features.push(Feature::Extrude(Extrude::new(height)?));
        }
        Self::new(wire.path, features)
    }
}

/// Serialized form of [`Contours`]; validation happens in its `TryFrom` implementation.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ContoursWire {
    paths: Vec<Path>,
}

/// A flat drawing of several closed rings: one contour boolean's result.
///
/// A boolean of two drawn figures leaves one piece, several pieces, or nothing. Several pieces are
/// this payload: a leaf that draws its rings and has no solid, so nothing extrudes it yet — the
/// area between its rings is derived on read, not stored (G4). Each ring is closed and encloses
/// material; a hole is not a ring of the set but an area, which this payload does not carry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "ContoursWire")]
pub struct Contours {
    paths: Vec<Path>,
}

impl Contours {
    /// Validates the rings and stores them in the order they are given.
    pub fn new(paths: Vec<Path>) -> Result<Self, ContoursError> {
        if paths.is_empty() {
            return Err(ContoursError::Empty);
        }
        if paths.len() > MAX_CONTOURS {
            return Err(ContoursError::Count { count: paths.len(), max: MAX_CONTOURS });
        }
        for (index, path) in paths.iter().enumerate() {
            if !path.is_closed() {
                return Err(ContoursError::OpenRing { index });
            }
            path.to_contour().map_err(|error| ContoursError::Ring { index, error })?;
        }
        Ok(Self { paths })
    }

    /// The rings, in the order they were stored.
    pub fn paths(&self) -> &[Path] {
        &self.paths
    }

    /// How many rings the drawing holds.
    pub fn len(&self) -> usize {
        self.paths.len()
    }

    /// Whether the drawing holds no ring; always false, because a set needs at least one.
    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    /// The box of every ring together, in micrometres.
    ///
    /// A set is never empty, so the box is always at least one ring's own box.
    pub fn bounds_micrometres(&self) -> ([i64; 2], [i64; 2]) {
        let mut bounds: Option<([i64; 2], [i64; 2])> = None;
        for path in &self.paths {
            let (min, max) = path.bounds_micrometres();
            bounds = Some(match bounds {
                None => (min, max),
                Some((low, high)) => {
                    ([low[0].min(min[0]), low[1].min(min[1])], [high[0].max(max[0]), high[1].max(max[1])])
                }
            });
        }
        bounds.unwrap_or(([0; 2], [0; 2]))
    }

    /// The same rings moved by a flat offset.
    pub fn translated(&self, offset: [Length; 2]) -> Result<Self, ContoursError> {
        let mut paths = Vec::with_capacity(self.paths.len());
        for (index, path) in self.paths.iter().enumerate() {
            paths.push(path.translated(offset).map_err(|error| ContoursError::Ring { index, error })?);
        }
        Self::new(paths)
    }
}

impl TryFrom<ContoursWire> for Contours {
    type Error = ContoursError;

    fn try_from(wire: ContoursWire) -> Result<Self, Self::Error> {
        Self::new(wire.paths)
    }
}

/// A validation failure of a contour set.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum ContoursError {
    /// The set holds no ring.
    #[error("a contour set needs at least one ring")]
    Empty,
    /// The set holds more rings than one drawing may carry.
    #[error("a contour set holds at most {max} rings, got {count}")]
    Count {
        /// The supplied ring count.
        count: usize,
        /// The accepted maximum.
        max: usize,
    },
    /// One ring is an open chain, which encloses no material.
    #[error("ring {index} of a contour set is an open chain")]
    OpenRing {
        /// Position of the rejected ring in the set.
        index: usize,
    },
    /// One ring is not a ring the document accepts.
    #[error("ring {index} of a contour set is not a valid ring: {error}")]
    Ring {
        /// Position of the rejected ring in the set.
        index: usize,
        /// The ring rule the ring broke.
        error: PathError,
    },
}

/// A validation failure of a constructive object.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ConstructiveError {
    /// The contour is not a valid closed polyline.
    #[error(transparent)]
    Contour(#[from] ContourError),
    /// The stored path is not a chain the document accepts.
    #[error(transparent)]
    Path(#[from] PathError),
    /// The extrusion height is not positive or is too large.
    #[error("an extrusion height must be positive and at most {max}, got {height}")]
    InvalidHeight {
        /// The rejected height.
        height: Length,
        /// The accepted maximum.
        max: Length,
    },
    /// The feature list is empty or too long.
    #[error("a constructive object needs between 1 and {max} features, got {count}")]
    FeatureCount {
        /// The supplied feature count.
        count: usize,
        /// The supported maximum.
        max: usize,
    },
    /// The first feature is not an extrusion.
    #[error("the first feature of a constructive object must be an extrusion")]
    MissingExtrude,
    /// The path is an open chain, which encloses no ring to extrude.
    #[error("a constructive object needs a closed path, not an open chain")]
    OpenPath,
}

/// The geometry payload of an object node.
///
/// Exactly one payload is stored. A `Mesh` is edited directly; a [`Constructive`] payload is
/// rebuilt from its features and is not editable as a mesh until the object is collapsed
/// ([ADR 0037]). A [`Primitive`] stores the parameters that build its mesh, and is likewise not
/// editable as a mesh until it is collapsed ([ADR 0054]). A [`Path`] is a flat chain of drawn
/// segments: it has no solid yet, and it becomes one only when it is closed and extruded.
///
/// [ADR 0037]: ../docs/decisions/0037-a-geometry-object-is-a-mesh-or-a-feature-history.md
/// [ADR 0054]: ../docs/decisions/0054-a-primitive-keeps-the-parameters-that-built-it.md
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Geometry {
    /// A baked mesh, edited directly.
    Mesh(Mesh),
    /// A contour and its feature history; the mesh is derived.
    Constructive(Constructive),
    /// A primitive shape and its parameters; the mesh is derived.
    Primitive(Primitive),
    /// A flat, open or closed chain of drawn segments.
    Path(Path),
    /// A flat drawing of several closed rings, the result of a contour boolean.
    Contours(Contours),
}

impl Geometry {
    /// The mesh, when the payload is a baked mesh.
    pub const fn as_mesh(&self) -> Option<&Mesh> {
        match self {
            Self::Mesh(mesh) => Some(mesh),
            Self::Constructive(_) | Self::Primitive(_) | Self::Path(_) | Self::Contours(_) => None,
        }
    }

    /// The mesh for mutation, when the payload is a baked mesh.
    pub const fn as_mesh_mut(&mut self) -> Option<&mut Mesh> {
        match self {
            Self::Mesh(mesh) => Some(mesh),
            Self::Constructive(_) | Self::Primitive(_) | Self::Path(_) | Self::Contours(_) => None,
        }
    }

    /// The feature history, when the payload is constructive.
    pub const fn as_constructive(&self) -> Option<&Constructive> {
        match self {
            Self::Constructive(constructive) => Some(constructive),
            Self::Mesh(_) | Self::Primitive(_) | Self::Path(_) | Self::Contours(_) => None,
        }
    }

    /// The feature history for mutation, when the payload is constructive.
    pub const fn as_constructive_mut(&mut self) -> Option<&mut Constructive> {
        match self {
            Self::Constructive(constructive) => Some(constructive),
            Self::Mesh(_) | Self::Primitive(_) | Self::Path(_) | Self::Contours(_) => None,
        }
    }

    /// The parametric primitive, when the payload is a primitive.
    pub const fn as_primitive(&self) -> Option<&Primitive> {
        match self {
            Self::Primitive(primitive) => Some(primitive),
            Self::Mesh(_) | Self::Constructive(_) | Self::Path(_) | Self::Contours(_) => None,
        }
    }

    /// The parametric primitive for mutation, when the payload is a primitive.
    pub const fn as_primitive_mut(&mut self) -> Option<&mut Primitive> {
        match self {
            Self::Primitive(primitive) => Some(primitive),
            Self::Mesh(_) | Self::Constructive(_) | Self::Path(_) | Self::Contours(_) => None,
        }
    }

    /// The flat drawing, when the payload is a path.
    pub const fn as_path(&self) -> Option<&Path> {
        match self {
            Self::Path(path) => Some(path),
            Self::Mesh(_) | Self::Constructive(_) | Self::Primitive(_) | Self::Contours(_) => None,
        }
    }

    /// The flat drawing for mutation, when the payload is a path.
    pub const fn as_path_mut(&mut self) -> Option<&mut Path> {
        match self {
            Self::Path(path) => Some(path),
            Self::Mesh(_) | Self::Constructive(_) | Self::Primitive(_) | Self::Contours(_) => None,
        }
    }

    /// The ring set, when the payload is a contour set.
    pub const fn as_contours(&self) -> Option<&Contours> {
        match self {
            Self::Contours(contours) => Some(contours),
            Self::Mesh(_) | Self::Constructive(_) | Self::Primitive(_) | Self::Path(_) => None,
        }
    }
}

/// A free-form geometry object: a named, placed leaf with a geometry payload.
///
/// A panel is not an object: panel rules are manufacturing rules. An object is a leaf in v1, so
/// several parts are grouped by wrapping them in a container.
///
/// An object may be marked as a **tool**: it takes part in a contour boolean and survives it, where
/// an unmarked operand is consumed. The mark is a property of the object and nothing else — a tool
/// is not linked to what the boolean produced, so its later shape never reaches the result.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Object {
    id: Id,
    name: String,
    placement: Placement,
    #[serde(default)]
    pivot_local: PivotLocal,
    #[serde(default)]
    properties: Properties,
    tool: bool,
    geometry: Geometry,
}

impl Object {
    /// Creates an object with a zero pivot; a new object is not a tool.
    pub fn new(id: Id, name: impl Into<String>, placement: Placement, geometry: Geometry) -> Self {
        Self {
            id,
            name: name.into(),
            placement,
            pivot_local: PivotLocal::ZERO,
            properties: Properties::new(),
            tool: false,
            geometry,
        }
    }

    /// The stable object id.
    pub const fn id(&self) -> Id {
        self.id
    }

    /// The user-visible name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The placement relative to the parent frame.
    pub const fn placement(&self) -> Placement {
        self.placement
    }

    /// The manipulation pivot in the object's own frame.
    pub const fn pivot_local(&self) -> PivotLocal {
        self.pivot_local
    }

    /// The geometry payload.
    pub const fn geometry(&self) -> &Geometry {
        &self.geometry
    }

    /// The geometry payload for mutation.
    pub const fn geometry_mut(&mut self) -> &mut Geometry {
        &mut self.geometry
    }

    /// Replaces the user-visible name; the caller has already rejected a blank one.
    pub(crate) fn set_name(&mut self, name: impl Into<String>) {
        self.name = name.into();
    }

    /// Replaces the placement relative to the parent container.
    pub(crate) fn set_placement(&mut self, placement: Placement) {
        self.placement = placement;
    }

    /// Replaces the stored manipulation pivot.
    pub(crate) fn set_pivot(&mut self, pivot_local: PivotLocal) {
        self.pivot_local = pivot_local;
    }

    /// Replaces the stable id when a subtree is copied.
    pub(crate) fn set_node_id(&mut self, id: Id) {
        self.id = id;
    }

    /// Whether the object is marked as a tool, so a contour boolean keeps it.
    pub const fn is_tool(&self) -> bool {
        self.tool
    }

    /// Marks or unmarks the object as a tool.
    pub(crate) fn set_tool(&mut self, tool: bool) {
        self.tool = tool;
    }

    /// The typed properties of this object.
    pub const fn properties(&self) -> &Properties {
        &self.properties
    }

    /// Stores one validated property.
    pub fn set_property(&mut self, key: &str, value: PropertyValue) -> Result<(), PropertyError> {
        self.properties.insert(key, value).map(|_| ())
    }

    /// Removes one property; returns whether it was present.
    pub fn clear_property(&mut self, key: &str) -> bool {
        self.properties.remove(key).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::*;
    use crate::*;

    use crate::tree::object_bounds_mm;
    use geom::{DQuat, DVec3};

    #[test]
    fn object_round_trips_in_the_current_schema() {
        let root = Container::new(Id::new(1).expect("id"), "Root", Placement::IDENTITY, vec![sample_object(5).into()]);
        let project = Project::new(root).expect("valid project");
        assert_eq!(project.schema_version(), Project::CURRENT_SCHEMA_VERSION);
        let object = project.object(Id::new(5).expect("id")).expect("object");
        assert_eq!(object.geometry().as_mesh().expect("mesh").faces().len(), 6);

        let text = serde_json::to_string(&project).expect("serialize project");
        assert!(text.contains(&format!("\"schema_version\":{}", Project::CURRENT_SCHEMA_VERSION)));
        assert!(text.contains("\"object\":{"));
        let decoded: Project = serde_json::from_str(&text).expect("deserialize project");
        assert_eq!(decoded, project);
    }

    #[test]
    fn an_object_saved_before_the_pivot_field_loads_with_a_zero_pivot() {
        let object = sample_object(5);
        let mut value = serde_json::to_value(&object).expect("serialize");
        if let Some(fields) = value.as_object_mut() {
            fields.remove("pivot");
        }
        let loaded: Object = serde_json::from_value(value).expect("load an older object");
        assert_eq!(loaded.pivot_local(), PivotLocal::ZERO);
        assert_eq!(loaded.geometry(), object.geometry(), "the rest of the object is unchanged");
    }

    #[test]
    fn object_node_commands_rename_place_duplicate_and_delete() {
        let root = Container::new(Id::new(1).expect("id"), "Root", Placement::IDENTITY, vec![sample_object(5).into()]);
        let mut project = Project::new(root).expect("valid project");
        let id = Id::new(5).expect("id");

        project.rename_node(id, "Bracket").expect("rename");
        assert_eq!(project.object(id).expect("object").name(), "Bracket");

        let placement = Placement::new(DVec3::new(10.0, 20.0, 30.0), DQuat::IDENTITY).expect("placement");
        project.set_node_placement(id, placement).expect("place");
        assert_eq!(project.object(id).expect("object").placement(), placement);

        let copy = project.duplicate_node(id).expect("duplicate");
        assert_ne!(copy, id);
        let copy_object = project.object(copy).expect("copied object");
        assert_eq!(copy_object.name(), "Bracket copy");
        assert_eq!(copy_object.geometry(), project.object(id).expect("object").geometry());

        project.remove_node(id).expect("delete");
        assert!(project.object(id).is_none());
        assert!(project.object(copy).is_some());
    }

    #[test]
    fn mesh_vertices_move_as_one_revision() {
        let root = Container::new(Id::new(1).expect("id"), "Root", Placement::IDENTITY, vec![sample_object(5).into()]);
        let mut project = Project::new(root).expect("valid project");
        let id = Id::new(5).expect("id");
        let before = project.object(id).expect("object").geometry().as_mesh().expect("mesh").positions()[0];
        let target = [millimetres(10), millimetres(20), millimetres(30)];

        let revision = project.move_mesh_vertices(id, &[(0, target)]).expect("move the vertex");

        assert_eq!(revision.get(), 1);
        assert_eq!(project.revision().get(), 1);
        let mesh = project.object(id).expect("object").geometry().as_mesh().expect("mesh");
        assert_eq!(mesh.positions()[0], target);
        assert_ne!(before, target);
        assert_eq!(mesh.positions().len(), 8, "a move changes no topology");
        assert_eq!(mesh.faces().len(), 6);
    }

    #[test]
    fn a_bad_vertex_batch_is_rejected_without_mutating() {
        let root = Container::new(Id::new(1).expect("id"), "Root", Placement::IDENTITY, vec![sample_object(5).into()]);
        let mut project = Project::new(root).expect("valid project");
        let id = Id::new(5).expect("id");
        let before = project.clone();
        let target = [millimetres(1), millimetres(2), millimetres(3)];

        assert_eq!(project.move_mesh_vertices(id, &[]), Err(ProjectError::NoVertexMoves));
        assert_eq!(
            project.move_mesh_vertices(id, &[(0, target), (0, target)]),
            Err(ProjectError::DuplicateVertex { object: id, vertex: 0 })
        );
        assert_eq!(
            project.move_mesh_vertices(id, &[(0, target), (8, target)]),
            Err(ProjectError::InvalidGeometry(MeshError::VertexIndexOutOfBounds { vertex: 8, count: 8 }))
        );
        assert_eq!(project.revision(), before.revision(), "a rejected batch does not bump the revision");
        assert!(project.same_content(&before));
    }

    #[test]
    fn a_non_mesh_node_has_no_editable_vertices() {
        let root = Container::new(
            Id::new(1).expect("id"),
            "Root",
            Placement::IDENTITY,
            vec![sample_panel(Id::new(2).expect("id"), "Shelf").into()],
        );
        let mut project = Project::new(root).expect("valid project");
        let target = [millimetres(1), millimetres(2), millimetres(3)];

        assert_eq!(
            project.move_mesh_vertices(Id::new(2).expect("id"), &[(0, target)]),
            Err(ProjectError::NotAMeshObject { id: Id::new(2).expect("id") })
        );
        assert_eq!(
            project.move_mesh_vertices(Id::new(99).expect("id"), &[(0, target)]),
            Err(ProjectError::NodeNotFound { id: Id::new(99).expect("id") })
        );
        assert_eq!(project.revision(), Revision::ZERO);
    }

    #[test]
    fn object_duplicate_id_is_rejected() {
        let root = Container::new(
            Id::new(1).expect("id"),
            "Root",
            Placement::IDENTITY,
            vec![sample_object(5).into(), sample_object(5).into()],
        );
        assert_eq!(Project::new(root), Err(ProjectError::DuplicateId(Id::new(5).expect("id"))));
    }

    #[test]
    fn query_selects_objects_by_structural_kind() {
        let root = Container::new(
            Id::new(1).expect("id"),
            "Root",
            Placement::IDENTITY,
            vec![sample_object(5).into(), sample_panel(Id::new(2).expect("id"), "Shelf").into()],
        );
        let project = Project::new(root).expect("valid project");
        let found = project.query(&NodeQuery::any().with_kind(StructuralKind::Object));
        assert_eq!(found, vec![Id::new(5).expect("id")]);
    }

    #[test]
    fn a_constructive_object_keeps_its_contour_and_collapses_to_a_mesh() {
        let root = Container::new(Id::new(1).expect("id"), "Root", Placement::IDENTITY, Vec::new());
        let mut project = Project::new(root).expect("valid project");
        let root = project.root().id();
        let constructive =
            Constructive::extrude(rectangle_contour(100, 50).into(), millimetres(18)).expect("constructive");
        let id = project
            .add_object(root, "Countertop", Placement::IDENTITY, Geometry::Constructive(constructive.clone()))
            .expect("object");

        let text = serde_json::to_string(&project).expect("serialize");
        assert!(text.contains("\"kind\":\"constructive\""), "the payload is tagged");
        assert_eq!(serde_json::from_str::<Project>(&text).expect("load"), project, "the history round-trips");
        assert!(project.object(id).expect("object").geometry().as_mesh().is_none(), "the mesh is derived");

        let wider = rectangle_contour(120, 50);
        let revision = project.set_object_body_path(id, wider.clone().into()).expect("replace the contour");
        assert_eq!(revision.get(), 2, "creation was revision one");
        let object = project.object(id).expect("object");
        assert_eq!(object.name(), "Countertop");
        assert_eq!(object.placement(), Placement::IDENTITY);
        assert_eq!(
            object.geometry().as_constructive().expect("constructive").contour(),
            Ok(wider),
            "the id, name and placement survive a contour edit"
        );
        assert_eq!(
            project.move_mesh_vertices(id, &[(0, [millimetres(1), millimetres(1), millimetres(1)])]),
            Err(ProjectError::NotAMeshObject { id })
        );

        let mesh = crate::testing::cube_mesh(millimetres(10));
        let revision = project.collapse_object(id, mesh.clone()).expect("collapse");
        assert_eq!(revision.get(), 3);
        let object = project.object(id).expect("object");
        assert_eq!(object.geometry().as_mesh(), Some(&mesh));
        assert!(object.geometry().as_constructive().is_none(), "collapse drops the history");
        assert_eq!(project.collapse_object(id, mesh), Err(ProjectError::NotDerivedObject { id }));
    }

    #[test]
    fn a_constructive_object_bounds_use_the_contour_and_height() {
        let geometry = Geometry::Constructive(
            Constructive::extrude(rectangle_contour(100, 50).into(), millimetres(18)).expect("constructive"),
        );
        let (min, max) = object_bounds_mm(&geometry).expect("bounds");
        assert!((min - DVec3::ZERO).length() < 1.0e-9);
        assert!((max - DVec3::new(100.0, 50.0, 18.0)).length() < 1.0e-9);
    }

    #[test]
    fn a_primitive_keeps_its_parameters_and_collapses_to_a_mesh() {
        let root = Container::new(Id::new(1).expect("id"), "Root", Placement::IDENTITY, Vec::new());
        let mut project = Project::new(root).expect("valid project");
        let root = project.root().id();
        let original = Primitive::sphere(millimetres(50), 16, 8).expect("sphere");
        let id = project
            .add_object(root, "Ball", Placement::IDENTITY, Geometry::Primitive(original.clone()))
            .expect("object");

        let text = serde_json::to_string(&project).expect("serialize");
        assert!(text.contains("\"kind\":\"primitive\""), "the payload is tagged: {text}");
        assert!(text.contains("\"shape\":\"sphere\""), "the shape is the inner tag: {text}");
        assert_eq!(serde_json::from_str::<Project>(&text).expect("load"), project, "the parameters round-trip");
        assert!(project.object(id).expect("object").geometry().as_mesh().is_none(), "the mesh is derived");

        let (min, max) = object_bounds_mm(project.object(id).expect("object").geometry()).expect("bounds");
        assert!((min - DVec3::new(-50.0, -50.0, -50.0)).length() < 1.0e-9);
        assert!((max - DVec3::new(50.0, 50.0, 50.0)).length() < 1.0e-9);

        let replacement = Primitive::sphere(millimetres(80), 24, 12).expect("sphere");
        let revision = project.set_object_primitive(id, replacement.clone()).expect("replace the primitive");
        assert_eq!(revision.get(), 2, "creation was revision one");
        assert_eq!(project.object_primitive(id), Ok(&replacement));
        assert_eq!(
            project.move_mesh_vertices(id, &[(0, [millimetres(1), millimetres(1), millimetres(1)])]),
            Err(ProjectError::NotAMeshObject { id })
        );

        let mesh = crate::testing::cube_mesh(millimetres(10));
        let revision = project.collapse_object(id, mesh.clone()).expect("collapse");
        assert_eq!(revision.get(), 3);
        let object = project.object(id).expect("object");
        assert_eq!(object.name(), "Ball", "collapse keeps every other field");
        assert_eq!(object.geometry().as_mesh(), Some(&mesh));
        assert!(object.geometry().as_primitive().is_none(), "collapse drops the parameters");
        assert_eq!(project.set_object_primitive(id, replacement), Err(ProjectError::NotAPrimitiveObject { id }));
    }

    /// A closed rectangle ring, 100 x 50 mm at the given offset.
    fn rectangle_ring(x: i64, y: i64) -> Path {
        Path::rectangle([millimetres(x), millimetres(y)], [millimetres(x + 100), millimetres(y + 50)])
            .expect("a rectangle ring")
    }

    #[test]
    fn a_contour_set_holds_closed_rings_and_moves_as_one() {
        let set = Contours::new(vec![rectangle_ring(0, 0), rectangle_ring(200, 0)]).expect("two rings");
        assert_eq!(set.len(), 2, "both rings are kept");
        assert!(!set.is_empty());
        assert_eq!(set.bounds_micrometres(), ([0, 0], [300_000, 50_000]), "the box holds both rings");

        let moved = set.translated([millimetres(10), millimetres(5)]).expect("a flat move");
        assert_eq!(moved.bounds_micrometres(), ([10_000, 5_000], [310_000, 55_000]));

        assert_eq!(Contours::new(Vec::new()), Err(ContoursError::Empty), "a set needs a ring");
        let open = sample_path();
        assert_eq!(Contours::new(vec![open]), Err(ContoursError::OpenRing { index: 0 }));
        let many = vec![rectangle_ring(0, 0); MAX_CONTOURS + 1];
        assert!(matches!(Contours::new(many), Err(ContoursError::Count { .. })));
    }

    #[test]
    fn a_contour_set_round_trips_and_a_set_object_reports_its_rings() {
        let root = Container::new(Id::new(1).expect("id"), "Root", Placement::IDENTITY, Vec::new());
        let mut project = Project::new(root).expect("valid project");
        let root = project.root().id();
        let set = Contours::new(vec![rectangle_ring(0, 0), rectangle_ring(200, 0)]).expect("two rings");
        let id = project
            .add_object(root, "Union", Placement::IDENTITY, Geometry::Contours(set.clone()))
            .expect("a set object is added like any other object");
        assert_eq!(project.object_contours(id), Ok(&set));
        assert_eq!(project.object_path(id), Err(ProjectError::NotAPathObject { id }));

        let text = serde_json::to_string(&project).expect("serialize");
        let decoded: Project = serde_json::from_str(&text).expect("deserialize");
        assert_eq!(decoded.object_contours(id), Ok(&set), "the set survives the file");
    }

    #[test]
    fn the_tool_mark_is_an_object_attribute_and_travels_in_the_file() {
        let mut project = crate::testing::sample_project();
        let root = project.root().id();
        let id = project
            .add_object(root, "Cutter", Placement::IDENTITY, Geometry::Path(rectangle_ring(0, 0)))
            .expect("a path object");
        assert!(!project.object(id).expect("object").is_tool(), "a new object is not a tool");
        project.set_object_tool(id, true).expect("mark the object");
        assert!(project.object(id).expect("object").is_tool());

        let text = serde_json::to_string(&project).expect("serialize");
        assert!(text.contains("\"tool\":true"), "the mark is a field of the object");
        let decoded: Project = serde_json::from_str(&text).expect("deserialize");
        assert!(decoded.object(id).expect("object").is_tool(), "the mark survives the file");

        // A panel is a node but not a geometry object, so it carries no such mark.
        let panel = project.root().children().first().expect("the shelf").id();
        assert_eq!(project.set_object_tool(panel, true), Err(ProjectError::NotAnObject { id: panel }));
        // A node nobody has is a missing node, not a wrong kind.
        let missing = Id::new(900).expect("id");
        assert_eq!(project.set_object_tool(missing, true), Err(ProjectError::NodeNotFound { id: missing }));
    }

    fn sample_path() -> Path {
        Path::new(
            vec![
                ContourSegment::line([millimetres(0), millimetres(0)], [millimetres(100), millimetres(0)])
                    .expect("line"),
                ContourSegment::line([millimetres(100), millimetres(0)], [millimetres(100), millimetres(50)])
                    .expect("line"),
            ],
            false,
        )
        .expect("open path")
    }

    #[test]
    fn a_path_object_round_trips_and_replaces_its_drawing() {
        let root = Container::new(Id::new(1).expect("id"), "Root", Placement::IDENTITY, Vec::new());
        let mut project = Project::new(root).expect("valid project");
        let root = project.root().id();
        let path = sample_path();
        let id = project
            .add_object(root, "Sketch", Placement::IDENTITY, Geometry::Path(path.clone()))
            .expect("a path object is added like any other object");

        assert_eq!(project.object_path(id), Ok(&path), "the drawing is queried by node id");
        assert!(project.object(id).expect("object").geometry().as_mesh().is_none(), "a path has no mesh");
        assert!(project.object(id).expect("object").geometry().as_constructive().is_none());

        let text = serde_json::to_string(&project).expect("serialize");
        assert!(text.contains("\"kind\":\"path\""), "the payload is tagged: {text}");
        let decoded: Project = serde_json::from_str(&text).expect("load");
        assert_eq!(decoded, project, "the path survives a save and load");
        assert_eq!(decoded.object_path(id), Ok(&path));

        let longer = Path::new(
            vec![
                ContourSegment::line([millimetres(0), millimetres(0)], [millimetres(100), millimetres(0)])
                    .expect("line"),
                ContourSegment::line([millimetres(100), millimetres(0)], [millimetres(100), millimetres(50)])
                    .expect("line"),
                ContourSegment::line([millimetres(100), millimetres(50)], [millimetres(0), millimetres(50)])
                    .expect("line"),
            ],
            false,
        )
        .expect("longer path");
        let revision = project.set_object_path(id, longer.clone()).expect("replace the drawing");
        assert_eq!(revision.get(), 2, "creation was revision one");
        assert_eq!(project.object_path(id), Ok(&longer));
        assert_eq!(project.object(id).expect("object").name(), "Sketch", "the node identity is unchanged");
    }

    #[test]
    fn a_path_command_refuses_other_payloads_and_missing_nodes() {
        let root = Container::new(Id::new(1).expect("id"), "Root", Placement::IDENTITY, vec![sample_object(5).into()]);
        let mut project = Project::new(root).expect("valid project");
        let mesh_id = Id::new(5).expect("id");

        assert_eq!(project.object_path(mesh_id), Err(ProjectError::NotAPathObject { id: mesh_id }));
        assert_eq!(project.set_object_path(mesh_id, sample_path()), Err(ProjectError::NotAPathObject { id: mesh_id }));
        assert_eq!(
            project.object_path(Id::new(99).expect("id")),
            Err(ProjectError::NodeNotFound { id: Id::new(99).expect("id") })
        );
        assert_eq!(project.revision(), Revision::ZERO, "a refused command does not bump the revision");
    }

    #[test]
    fn a_path_object_bounds_use_the_chain_extents() {
        let geometry = Geometry::Path(sample_path());
        let (min, max) = object_bounds_mm(&geometry).expect("bounds");
        assert!((min - DVec3::ZERO).length() < 1.0e-9);
        assert!((max - DVec3::new(100.0, 50.0, 0.0)).length() < 1.0e-9, "a path has no height");
    }

    #[test]
    fn extruding_a_path_turns_the_node_into_the_body_in_place() {
        let root = Container::new(Id::new(1).expect("id"), "Root", Placement::IDENTITY, Vec::new());
        let mut project = Project::new(root).expect("valid project");
        let container = project.root().id();
        let path =
            Path::rectangle([millimetres(0), millimetres(0)], [millimetres(100), millimetres(50)]).expect("rectangle");
        let id = project
            .add_object(container, "Plate", Placement::IDENTITY, Geometry::Path(path.clone()))
            .expect("a path object");

        let revision = project.extrude_object_path(id, millimetres(18)).expect("extrude in place");
        assert_eq!(revision.get(), 2, "creation was revision one");
        let object = project.object(id).expect("the node is still there");
        assert_eq!(object.name(), "Plate", "the node keeps its name");
        assert_eq!(object.geometry().as_path(), None, "the drawing is the body now");
        let body = object.geometry().as_constructive().expect("a constructive body");
        assert_eq!(body.path(), &path, "the body owns the one path, tag and ids included");
        assert_eq!(body.height(), Some(millimetres(18)));
        assert_eq!(project.root().children().len(), 1, "no sibling body is left behind");
        assert_eq!(
            project.extrude_object_path(id, millimetres(18)),
            Err(ProjectError::NotAPathObject { id }),
            "a body is not extruded a second time"
        );
        assert_eq!(project.revision(), revision, "a refused extrusion changes nothing");
    }

    #[test]
    fn an_open_path_is_refused_before_it_becomes_a_body() {
        let root = Container::new(Id::new(1).expect("id"), "Root", Placement::IDENTITY, Vec::new());
        let mut project = Project::new(root).expect("valid project");
        let container = project.root().id();
        let id = project
            .add_object(container, "Sketch", Placement::IDENTITY, Geometry::Path(sample_path()))
            .expect("a path object");

        assert!(matches!(
            project.extrude_object_path(id, millimetres(18)),
            Err(ProjectError::InvalidConstructive(ConstructiveError::OpenPath))
        ));
        assert!(project.object_path(id).is_ok(), "the drawing stays a drawing");
        assert_eq!(project.revision().get(), 1, "only the creation counted");
    }

    #[test]
    fn a_body_path_and_height_are_edited_in_place() {
        let root = Container::new(Id::new(1).expect("id"), "Root", Placement::IDENTITY, Vec::new());
        let mut project = Project::new(root).expect("valid project");
        let container = project.root().id();
        let path =
            Path::rectangle([millimetres(0), millimetres(0)], [millimetres(100), millimetres(50)]).expect("rectangle");
        let id = project
            .add_object(
                container,
                "Plate",
                Placement::IDENTITY,
                Geometry::Constructive(Constructive::extrude(path, millimetres(18)).expect("body")),
            )
            .expect("a body");

        let wider =
            Path::rectangle([millimetres(0), millimetres(0)], [millimetres(120), millimetres(50)]).expect("rectangle");
        project.set_object_body_path(id, wider.clone()).expect("replace the path");
        let body = project.object(id).expect("object").geometry().as_constructive().expect("body");
        assert_eq!(body.path(), &wider, "the body's own path changed");
        assert_eq!(body.contour().expect("ring").bounds_micrometres(), ([0, 0], [120_000, 50_000]));

        project.set_object_extrusion_height(id, millimetres(30)).expect("raise the body");
        let body = project.object(id).expect("object").geometry().as_constructive().expect("body");
        assert_eq!(body.height(), Some(millimetres(30)));
        assert_eq!(body.path(), &wider, "the height change keeps the path");
        assert_eq!(
            project.set_object_extrusion_height(id, Length::ZERO),
            Err(ProjectError::InvalidConstructive(ConstructiveError::InvalidHeight {
                height: Length::ZERO,
                max: MAX_CONTOUR_COORDINATE
            }))
        );
    }

    #[test]
    fn a_constructive_body_without_a_path_is_a_readable_error() {
        let missing = serde_json::json!({ "features": [{ "kind": "extrude", "height": 18_000 }] });
        let error = serde_json::from_value::<Constructive>(missing).expect_err("a body without a path is refused");
        assert!(error.to_string().contains("missing field `path`"), "error was {error}");
    }

    #[test]
    fn a_bad_extrusion_height_is_rejected_on_build_and_load() {
        let contour = rectangle_contour(10, 10);
        assert_eq!(
            Constructive::extrude(contour.clone().into(), Length::ZERO),
            Err(ConstructiveError::InvalidHeight { height: Length::ZERO, max: MAX_CONTOUR_COORDINATE })
        );
        let constructive = Constructive::extrude(contour.into(), millimetres(10)).expect("constructive");
        let text = serde_json::to_string(&constructive).expect("serialize");
        let too_high = text.replace("\"height\":10000", "\"height\":0");
        assert_ne!(text, too_high, "the fixture must contain the height in micrometres");
        assert!(serde_json::from_str::<Constructive>(&too_high).is_err());
    }
}
