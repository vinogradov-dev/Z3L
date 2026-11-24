//! The Z3L document: the container tree, panels, machining, banding, materials and the scene
//! objects the user authors.
//!
//! Pure serializable data plus its invariants. No geometry kernel, no GPU, no UI, no file I/O. The
//! crate root is a facade: each module owns one family of values or rules and its public items are
//! re-exported here, so callers keep writing `model::Type`.
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

use std::collections::HashSet;

use units::Length;

mod camera;
mod class;
mod container;
mod contour;
mod contour_edit;
mod contour_frames;
#[cfg(test)]
mod fixtures;
mod footprint;
mod frame;
mod ids;
mod light;
mod mesh;
mod node;
mod object;
mod panel;
mod panel_features;
mod panel_type;
mod placement;
mod primitive;
mod profile;
mod project;
mod properties;
mod query;
mod side;
#[cfg(any(test, feature = "testing"))]
pub mod testing;
mod tree;

pub use camera::{CameraError, CameraNode, CameraProjection};
pub use class::{
    CLASS_MAX_CODE, ClassCode, ClassCodeError, ClassPrefix, ClassRow, Extent, Fill, Level, Measure, Mount, Nesting,
    Opening, Operation, OperationSet, Supply, TraitSet, registry,
};
pub use container::{Container, Envelope, EnvelopeError};
pub use contour::{
    Contour, ContourError, ContourSegment, ContourSegmentEntry, ContourSegmentError, MAX_CONTOUR_COORDINATE,
    MAX_CONTOUR_SEGMENTS, MAX_POLYGON_SIDES, MIN_POLYGON_SIDES, Path, PathError, PathShape, joint_tolerance_mm,
};
pub use contour_edit::{ContourEditError, CornerMove, PathCorner};
pub use contour_frames::{ArcFrame, CircleFrame, PolygonFrame, RectangleFrame};
pub use footprint::{Footprint, FootprintError};
pub use frame::{Anchored, FrameAnchor, FrameError, Rebased, rebase_origin_at};
pub use ids::{ContourSegmentId, EdgeId, Id, ItemId, ItemIdError, MachiningId, Revision, VertexId};
pub use light::{LightError, LightKind, LightNode};
pub use mesh::{MAX_FACE_VERTICES, MAX_MESH_FACES, MAX_MESH_VERTICES, Mesh, MeshError};
pub use node::Node;
pub use object::{
    AreaError, Boolean, BooleanError, BooleanOp, BooleanTool, Constructive, ConstructiveError, Contours, ContoursError,
    Extrude, Feature, Geometry, MAX_BOOLEAN_DEPTH, MAX_CONTOURS, MAX_OBJECT_FEATURES, MAX_TOOL_NAME, Object, ToolBody,
};
pub use panel::{
    BakedOutline, BandingSpec, ContourPath, CoreViolation, CornerBanding, Cutout, CutoutDepth, CutoutShape,
    DerivedHoles, Drilling, EdgeBanding, EdgeRef, EntryFace, FaceSide, Hole, HoleDiagnostic, HoleError,
    MACHINING_DESCRIPTORS, Machining, MachiningAnchor, MachiningCapabilities, MachiningCategory, MachiningDescriptor,
    MachiningKind, MachiningParams, MachiningStage, Panel, PanelContourError, PanelDimensionError, PanelError,
    PanelGeometry, PanelGeometryError, PanelNominalSize, PanelOutline, PanelRect, PanelRectError, RING_TOLERANCE_MM,
    RectCorner, RectEdge, RingBanding, SegmentSource, classify_outline, derive_holes, role_of_plane,
};
pub use panel_features::{
    BevelError, CornerFeature, CornerFeatureKind, EdgeBevel, EdgeBow, MAX_BEVEL_ANGLE, MAX_TRIM, PanelTrims, TrimError,
};
pub use panel_type::PanelType;
pub use placement::{EulerAngles, PivotLocal, Placement, PlacementError};
pub use primitive::{
    Cone, DEFAULT_BANDS, DEFAULT_SEGMENTS, MAX_BANDS, MAX_PRIMITIVE_SIZE, MAX_SEGMENTS, MIN_BANDS, MIN_PRIMITIVE_SIZE,
    MIN_SEGMENTS, Primitive, PrimitiveError, PrimitiveKind, PrimitiveShape, Sphere,
};
pub use profile::{Sheet, WorkshopProfile};
pub use project::{
    Capture, FillMode, FillPlan, FillRefusal, FillRequest, MoveAxes, MoveAxis, MoveLimit, MoveRule, MoveSettings,
    Niche, NichePlane, NicheRefusal, NicheShape, Probe, ProbeFace, ProbeHit, Project, ProjectError, Resting,
    RotateLimit, RotateRule, SNAP_GRID_MM, SceneMark, Sight, SnapAnswer, SnapAxis, SnapEnd, SnapKind, SnapKinds,
    SnapMode, SnapPlane, SnapRequest, SnapSettings, SnapSurface, SnapTarget, Stop, SurfaceKind, free_axis,
    round_free_coordinate, snap_axis, snap_grid,
};
pub use properties::{
    Properties, PropertyChange, PropertyError, PropertyValue, SYSTEM_CLASS, SYSTEM_COMPONENT, SYSTEM_PROTOTYPE_ID,
    SYSTEM_PROTOTYPE_PARAMETERS, SYSTEM_PROTOTYPE_VERSION,
};
pub use query::{NodeQuery, PropertyPredicate, StructuralKind};
pub use side::Side;
