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

/// The operation a boolean step applies to the body built so far.
///
/// The document keeps its own name for the three operations the kernel port also knows: `model`
/// describes what the user asked for and never imports a geometry kernel (ADR 0001, ADR 0041).
///
/// [ADR 0001]: ../docs/decisions/0001-geometry-kernel.md
/// [ADR 0041]: ../docs/decisions/0041-every-solid-compiles-to-one-solid-plan.md
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BooleanOp {
    /// Keeps both bodies.
    Union,
    /// Cuts the tool out of the body built so far.
    Difference,
    /// Keeps only what both bodies share.
    Intersection,
}

/// Largest nesting depth of boolean steps inside one body.
///
/// A step carries the tool's own shape, and that shape may carry steps of its own, so a hostile
/// file could nest the history; the depth is bounded where the shape is stored, so no walk over a
/// document can recurse without a limit (ADR 0041, ADR 0197).
pub const MAX_BOOLEAN_DEPTH: usize = 8;

/// The shape a boolean step cuts with, frozen when the step was applied.
///
/// The cutter is a node the maker used once, not a live operand: the step stores the body it had
/// and where it stood in the target's own frame, so the result is a body of its own and a later
/// edit to the source changes nothing (ADR 0197). The [name](BooleanTool::name) is the source's,
/// kept so the card can say what cut the body.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "BooleanToolWire")]
pub struct BooleanTool {
    name: String,
    body: Constructive,
    placement: Placement,
}

impl BooleanTool {
    /// Stores the cutter's name, its body and its placement in the target's frame.
    ///
    /// The name is a label and nothing else: it is kept for the card, which says "difference · cup
    /// bore" rather than a number, and is never resolved back to a node. A blank or absurdly long
    /// name is refused, because a file must not carry a label no surface can print.
    pub fn new(name: impl Into<String>, body: Constructive, placement: Placement) -> Result<Self, BooleanError> {
        let name = name.into();
        if name.trim().is_empty() || name.chars().count() > MAX_TOOL_NAME {
            return Err(BooleanError::ToolName { max: MAX_TOOL_NAME });
        }
        Ok(Self { name, body, placement })
    }

    /// The name the cutter had when the step was applied.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The cutter's own feature history, as it stood when the step was applied.
    pub const fn body(&self) -> &Constructive {
        &self.body
    }

    /// The cutter's placement in the frame of the body it cut.
    pub const fn placement(&self) -> Placement {
        self.placement
    }
}

impl TryFrom<BooleanToolWire> for BooleanTool {
    type Error = BooleanError;

    fn try_from(wire: BooleanToolWire) -> Result<Self, Self::Error> {
        Self::new(wire.name, wire.body, wire.placement)
    }
}

/// Serialized form of [`BooleanTool`].
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BooleanToolWire {
    name: String,
    body: Constructive,
    placement: Placement,
}

/// A boolean step: the body built so far combined with a shape the maker cut it with.
///
/// The step is a snapshot, not a reference: the tool's body and placement live in the step, so the
/// result stands on its own, a later edit to the source changes nothing, and no file can name a
/// node that is gone (ADR 0197).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Boolean {
    op: BooleanOp,
    tool: BooleanTool,
}

impl Boolean {
    /// Stores the operation and the shape it cuts with.
    pub const fn new(op: BooleanOp, tool: BooleanTool) -> Self {
        Self { op, tool }
    }

    /// The boolean operation.
    pub const fn op(&self) -> BooleanOp {
        self.op
    }

    /// The shape the step cuts with, frozen when the step was applied.
    pub const fn tool(&self) -> &BooleanTool {
        &self.tool
    }
}

/// One step of a constructive object's feature history.
///
/// The first feature is always the extrusion of the object's own path; every later feature
/// combines the body built so far with something else ([`Feature::Boolean`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Feature {
    /// Extrusion of the constructive path.
    Extrude(Extrude),
    /// A boolean step against the body of another object.
    Boolean(Boolean),
}

/// Serialized form of [`Feature`]: an extrusion of the constructive path or a boolean step.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum FeatureWire {
    Extrude { height: Length },
    Boolean { op: BooleanOp, tool: BooleanToolWire },
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
        if feature_depth(&features) > MAX_BOOLEAN_DEPTH {
            return Err(ConstructiveError::TooDeepBooleans { max: MAX_BOOLEAN_DEPTH });
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
            Some(Feature::Boolean(_)) | None => None,
        }
    }

    /// Every boolean step, in application order.
    pub fn booleans(&self) -> impl Iterator<Item = &Boolean> {
        self.features.iter().filter_map(|feature| match feature {
            Feature::Boolean(boolean) => Some(boolean),
            Feature::Extrude(_) => None,
        })
    }

    /// Appends a boolean step after the body built so far.
    ///
    /// The step carries the shape it cuts with, so nothing is resolved here and no node can be
    /// named that a later load could not find (ADR 0197).
    pub fn push_boolean(&mut self, boolean: Boolean) -> Result<(), ConstructiveError> {
        if self.features.len() >= MAX_OBJECT_FEATURES {
            return Err(ConstructiveError::FeatureCount { count: self.features.len(), max: MAX_OBJECT_FEATURES });
        }
        let depth = 1usize.saturating_add(feature_depth(boolean.tool().body().features()));
        if depth > MAX_BOOLEAN_DEPTH {
            return Err(ConstructiveError::TooDeepBooleans { max: MAX_BOOLEAN_DEPTH });
        }
        self.features.push(Feature::Boolean(boolean));
        Ok(())
    }

    /// Removes the feature at `index` and returns it.
    ///
    /// The extrusion at index zero is the body's own sweep, so it is refused: removing it would
    /// leave a history with nothing to build.
    pub fn remove_feature(&mut self, index: usize) -> Result<Feature, ConstructiveError> {
        if index == 0 {
            return Err(ConstructiveError::MissingExtrude);
        }
        if index >= self.features.len() {
            return Err(ConstructiveError::FeatureIndex { index, count: self.features.len() });
        }
        Ok(self.features.remove(index))
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
            features.push(match feature {
                FeatureWire::Extrude { height } => Feature::Extrude(Extrude::new(height)?),
                FeatureWire::Boolean { op, tool } => Feature::Boolean(Boolean::new(op, BooleanTool::try_from(tool)?)),
            });
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

/// Largest number of characters a stored tool name may carry.
pub const MAX_TOOL_NAME: usize = 120;

/// A validation failure of a boolean step's stored shape.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum BooleanError {
    /// The name the cutter had is blank or too long.
    #[error("a boolean tool's name must carry 1 to {max} characters")]
    ToolName {
        /// The accepted maximum.
        max: usize,
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
    /// A boolean step carries a tool whose own history nests deeper than the document accepts.
    #[error("a body's boolean steps nest deeper than {max} levels")]
    TooDeepBooleans {
        /// The accepted maximum.
        max: usize,
    },
    /// A boolean step's stored shape is invalid.
    #[error(transparent)]
    Boolean(#[from] BooleanError),
    /// The feature index is outside the history, or names the extrusion at zero.
    #[error("feature {index} is outside a history of {count} features")]
    FeatureIndex {
        /// The rejected index.
        index: usize,
        /// The length of the history.
        count: usize,
    },
}

/// The nesting depth of a feature list: one for the body itself, one more per step that carries a
/// tool with steps of its own.
fn feature_depth(features: &[Feature]) -> usize {
    let mut depth = 1;
    for feature in features {
        if let Feature::Boolean(boolean) = feature {
            depth = depth.max(1usize.saturating_add(feature_depth(boolean.tool().body().features())));
        }
    }
    depth
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

    /// The one chain a payload draws, when it draws one.
    ///
    /// A path object draws the chain the user drew; a constructive body draws the path it grew from
    /// ([`Constructive::path`]), so a body's profile is editable and combinable wherever the ring
    /// lives instead of only in the object that happens to store it (X98). A contour set draws
    /// several rings, which no single chain describes; a mesh and a primitive draw none.
    pub const fn profile(&self) -> Option<&Path> {
        match self {
            Self::Path(path) => Some(path),
            Self::Constructive(constructive) => Some(constructive.path()),
            Self::Mesh(_) | Self::Primitive(_) | Self::Contours(_) => None,
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

    /// The shape a step cuts with: a 50 x 50 ring swept 200 mm, standing where the caller puts it.
    fn cutter(name: &str, at: (f64, f64, f64)) -> BooleanTool {
        let path = Path::rectangle([Length::ZERO, Length::ZERO], [millimetres(50), millimetres(50)]).expect("a ring");
        let body = Constructive::extrude(path, millimetres(200)).expect("a body");
        let placement = Placement::new(DVec3::new(at.0, at.1, at.2), DQuat::IDENTITY).expect("a placement");
        BooleanTool::new(name, body, placement).expect("a named tool")
    }

    fn scene(children: Vec<Node>) -> Container {
        Container::new(Id::new(1).expect("id"), "Root", Placement::IDENTITY, children)
    }

    #[test]
    fn a_boolean_step_carries_the_shape_it_cuts_with() {
        let mut project =
            Project::new(scene(vec![sample_body(5, 100, 50).into(), sample_body(6, 20, 20).into()])).expect("project");
        let body = Id::new(5).expect("id");
        let tool = Id::new(6).expect("id");
        project
            .apply_boolean(body, Boolean::new(BooleanOp::Difference, cutter("Cup bore", (25.0, 25.0, -10.0))), None)
            .expect("a step");

        let text = serde_json::to_string(&project).expect("serialize");
        assert!(text.contains("\"kind\":\"boolean\""), "the step is written by kind");
        assert!(text.contains("\"Cup bore\""), "the cutter's name travels with the step");
        let decoded: Project = serde_json::from_str(&text).expect("load");
        assert_eq!(decoded, project);

        let stored = decoded.object(body).expect("the body").geometry().as_constructive().expect("a body");
        let step = stored.booleans().next().expect("one step");
        assert_eq!(step.op(), BooleanOp::Difference);
        assert_eq!(step.tool().name(), "Cup bore");
        assert_eq!(step.tool().body().height(), Some(millimetres(200)), "the shape is frozen in the step");
        assert_eq!(step.tool().placement().translation(), DVec3::new(25.0, 25.0, -10.0));
        assert!(decoded.object(tool).is_some(), "a kept source is untouched");
    }

    #[test]
    fn applying_a_boolean_spends_its_cutter_in_the_same_revision() {
        let mut project =
            Project::new(scene(vec![sample_body(5, 100, 50).into(), sample_body(6, 20, 20).into()])).expect("project");
        let body = Id::new(5).expect("id");
        let tool = Id::new(6).expect("id");

        let revision = project
            .apply_boolean(body, Boolean::new(BooleanOp::Difference, cutter("Cutter", (0.0, 0.0, 0.0))), Some(tool))
            .expect("the step and the spent cutter");

        assert_eq!(revision.get(), 1, "one command, one revision");
        assert!(project.object(tool).is_none(), "the cutter leaves the tree with the step");
        let stored = project.object(body).expect("the body").geometry().as_constructive().expect("a body");
        assert_eq!(stored.booleans().count(), 1, "the result keeps the shape it cut with");
        assert_eq!(stored.booleans().next().expect("a step").tool().name(), "Cutter");

        // A step whose cutter was kept leaves that node alone.
        let kept = project
            .apply_boolean(body, Boolean::new(BooleanOp::Union, cutter("Kept", (0.0, 0.0, 0.0))), None)
            .expect("a step");
        assert_eq!(kept.get(), 2);
    }

    #[test]
    fn a_boolean_refuses_a_nameless_shape_and_a_history_that_nests_too_deep() {
        assert_eq!(
            BooleanTool::new(
                "  ",
                Constructive::extrude(
                    Path::rectangle([Length::ZERO, Length::ZERO], [millimetres(10), millimetres(10)],).expect("a ring"),
                    millimetres(10)
                )
                .expect("a body"),
                Placement::IDENTITY
            ),
            Err(BooleanError::ToolName { max: MAX_TOOL_NAME })
        );

        // A shape that carries a step of its own may be a cutter, but only so many levels deep.
        let mut nested = Constructive::extrude(
            Path::rectangle([Length::ZERO, Length::ZERO], [millimetres(10), millimetres(10)]).expect("a ring"),
            millimetres(10),
        )
        .expect("a body");
        for level in 1..=MAX_BOOLEAN_DEPTH {
            let inner = BooleanTool::new("Inner", nested.clone(), Placement::IDENTITY).expect("a tool");
            let mut outer = Constructive::extrude(
                Path::rectangle([Length::ZERO, Length::ZERO], [millimetres(10), millimetres(10)]).expect("a ring"),
                millimetres(10),
            )
            .expect("a body");
            let pushed = outer.push_boolean(Boolean::new(BooleanOp::Difference, inner));
            if level < MAX_BOOLEAN_DEPTH {
                pushed.expect("a level the document accepts");
                nested = outer;
            } else {
                assert_eq!(pushed, Err(ConstructiveError::TooDeepBooleans { max: MAX_BOOLEAN_DEPTH }));
            }
        }
    }

    #[test]
    fn a_removed_step_leaves_the_others_in_order_and_never_brings_its_cutter_back() {
        let mut project =
            Project::new(scene(vec![sample_body(5, 100, 50).into(), sample_body(6, 20, 20).into()])).expect("project");
        let five = Id::new(5).expect("id");
        let six = Id::new(6).expect("id");
        project
            .apply_boolean(five, Boolean::new(BooleanOp::Difference, cutter("First", (0.0, 0.0, 0.0))), Some(six))
            .expect("one step");
        project
            .apply_boolean(five, Boolean::new(BooleanOp::Union, cutter("Second", (0.0, 0.0, 0.0))), None)
            .expect("a second step");

        project.remove_object_boolean(five, 1).expect("remove the first step");
        let constructive = project.object(five).expect("body").geometry().as_constructive().expect("constructive");
        assert_eq!(constructive.features().len(), 2, "the extrusion and the second step remain");
        assert_eq!(
            constructive.booleans().map(Boolean::op).collect::<Vec<_>>(),
            vec![BooleanOp::Union],
            "the step that stayed is the one that was second"
        );
        assert!(project.object(six).is_none(), "the spent cutter does not return with undo-shaped edits");
        assert_eq!(project.remove_object_boolean(five, 0), Err(ProjectError::NoSuchBoolean { id: five, index: 0 }));
        assert_eq!(project.remove_object_boolean(five, 7), Err(ProjectError::NoSuchBoolean { id: five, index: 7 }));
    }

    #[test]
    fn an_unknown_boolean_operation_is_a_readable_load_error() {
        let mut project = Project::new(scene(vec![sample_body(5, 100, 50).into()])).expect("project");
        project
            .apply_boolean(
                Id::new(5).expect("id"),
                Boolean::new(BooleanOp::Difference, cutter("Cutter", (0.0, 0.0, 0.0))),
                None,
            )
            .expect("a step");
        let text = serde_json::to_string(&project).expect("serialize").replace("\"difference\"", "\"subtract\"");
        let error = serde_json::from_str::<Project>(&text).expect_err("an unknown operation is refused");
        assert!(error.to_string().contains("unknown variant"), "error was {error}");
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
    fn the_profile_is_a_paths_chain_or_the_one_a_body_grew_from() {
        // The one chain a payload draws, which is what a contour boolean reads: a body answers with
        // the path it was extruded from, so the ring is reachable wherever it lives (X98).
        let path = Path::rectangle([millimetres(0), millimetres(0)], [millimetres(100), millimetres(50)])
            .expect("a rectangle");
        assert_eq!(Geometry::Path(path.clone()).profile(), Some(&path), "a drawing is its own profile");
        let body = Constructive::extrude(path.clone(), millimetres(18)).expect("a body");
        assert_eq!(Geometry::Constructive(body).profile(), Some(&path), "a body owns the profile it grew from");
        let mesh = Geometry::Mesh(box_mesh());
        assert_eq!(mesh.profile(), None, "a baked mesh draws no chain");
        let sphere = Primitive::sphere(millimetres(20), 16, 8).expect("a sphere");
        assert_eq!(Geometry::Primitive(sphere).profile(), None, "a primitive draws no chain");
        let set = Contours::new(vec![path.clone()]).expect("a set");
        assert_eq!(Geometry::Contours(set).profile(), None, "a contour set draws several rings, not one");
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
