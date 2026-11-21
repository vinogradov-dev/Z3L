//! Parametric primitives: the shape parameters a mesh is derived from, and the steps cut from them.
//!
//! A primitive stores what the user can still change — a radius, the number of sides and latitude
//! bands — and never the mesh. The mesh is derived for the viewport and for `object.collapse`, so
//! raising a radius rebuilds one object rather than rewriting its vertices ([ADR 0054]). A box or a
//! cylinder is not a primitive: it is a drawn profile swept on Z, stored as a constructive body
//! ([ADR 0138]). The sphere stays a parameter set, but it is *placed* by drawing the circle it is
//! built around: the drawn centre becomes its origin and the drawn radius its stored number
//! ([ADR 0139]). The cone stays one for the same reason — a turned body is not a ring swept on Z —
//! and is placed by drawing its base circle on the work plane ([ADR 0141]).
//!
//! A primitive is a **body**, so a boolean can cut it as readily as it can be cut with: a block
//! subtracted from a ball leaves a rectangular notch, and the ball keeps its radius. The step is
//! frozen into the body exactly as on a drawn body ([ADR 0197]); the difference is only what the
//! steps start from — the shape's own parameters instead of a swept ring.
//!
//! [ADR 0054]: ../docs/decisions/0054-a-primitive-keeps-the-parameters-that-built-it.md
//! [ADR 0138]: ../docs/decisions/0138-a-solid-body-is-a-drawn-profile-swept-on-z.md
//! [ADR 0139]: ../docs/decisions/0139-a-sphere-is-drawn-around-its-centre.md
//! [ADR 0141]: ../docs/decisions/0141-a-cone-is-drawn-by-its-base-circle.md
//! [ADR 0197]: ../docs/decisions/0197-a-body-boolean-is-a-step-that-names-its-tool.md

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use units::Length;

use crate::MAX_OBJECT_FEATURES;
use crate::object::{Boolean, MAX_BOOLEAN_DEPTH};

/// Largest accepted primitive dimension: 1000 m, the same bound the panel rules use.
pub const MAX_PRIMITIVE_SIZE: Length = Length::from_micrometres(1_000_000_000);

/// Smallest accepted primitive dimension.
///
/// Two micrometres keep a positive half-extent when the mesh builder halves an edge or a height
/// into integer micrometres.
pub const MIN_PRIMITIVE_SIZE: Length = Length::from_micrometres(2);

/// Sides around a curved primitive when none is given.
pub const DEFAULT_SEGMENTS: u32 = 32;

/// Latitude bands between the two poles of a sphere when none is given.
pub const DEFAULT_BANDS: u32 = 16;

/// Fewest sides a curved primitive accepts.
pub const MIN_SEGMENTS: u32 = 3;

/// Most sides a curved primitive accepts.
pub const MAX_SEGMENTS: u32 = 256;

/// Fewest latitude bands a sphere accepts.
pub const MIN_BANDS: u32 = 2;

/// Most latitude bands a sphere accepts.
pub const MAX_BANDS: u32 = 128;

/// The primitive shapes the editor can build.
///
/// A box and a cylinder are constructive bodies a maker draws ([ADR 0138]); the sphere and the cone
/// are the shapes whose numbers still build them, because neither is a profile swept on Z.
///
/// [ADR 0138]: ../docs/decisions/0138-a-solid-body-is-a-drawn-profile-swept-on-z.md
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrimitiveKind {
    /// A sphere centred on the object's local origin.
    Sphere,
    /// A cone or a truncated cone standing on its base circle at `z = 0`.
    Cone,
}

impl PrimitiveKind {
    /// The shape name shown to the user.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Sphere => "Sphere",
            Self::Cone => "Cone",
        }
    }

    /// The lower-case shape name used in the scene tree.
    pub const fn shape_name(self) -> &'static str {
        match self {
            Self::Sphere => "sphere",
            Self::Cone => "cone",
        }
    }
}

/// A rejected primitive parameter set.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum PrimitiveError {
    /// A dimension is not positive or does not fit the accepted range.
    #[error("a primitive dimension must be between {min} and {max}, got {value}")]
    InvalidLength {
        /// The rejected length.
        value: Length,
        /// The smallest accepted length.
        min: Length,
        /// The largest accepted length.
        max: Length,
    },
    /// A curved primitive has too few or too many sides.
    #[error("a curved primitive needs between {min} and {max} sides, got {count}")]
    SegmentCount {
        /// The supplied side count.
        count: u32,
        /// The smallest accepted count.
        min: u32,
        /// The largest accepted count.
        max: u32,
    },
    /// A sphere has too few or too many latitude bands.
    #[error("a sphere needs between {min} and {max} latitude bands, got {count}")]
    BandCount {
        /// The supplied band count.
        count: u32,
        /// The smallest accepted count.
        min: u32,
        /// The largest accepted count.
        max: u32,
    },
    /// A cone's top radius is neither zero nor inside the accepted range.
    #[error("a cone's top radius is zero or between {min} and {max}, got {value}")]
    InvalidTopRadius {
        /// The rejected length.
        value: Length,
        /// The smallest accepted non-zero length.
        min: Length,
        /// The largest accepted length.
        max: Length,
    },
    /// A boolean step's frozen shape carries a body that nests too deeply.
    #[error("a body's boolean steps nest deeper than {max} levels")]
    TooDeepBooleans {
        /// The accepted maximum.
        max: usize,
    },
    /// The body already carries as many boolean steps as it may.
    #[error("a body accepts at most {max} boolean steps, got {count}")]
    StepCount {
        /// The supplied step count.
        count: usize,
        /// The accepted maximum.
        max: usize,
    },
    /// The step index names no step of this body.
    #[error("step {index} is outside a body of {count} steps")]
    StepIndex {
        /// The rejected index.
        index: usize,
        /// The number of steps the body carries.
        count: usize,
    },
}

/// Validates one primitive dimension.
fn validate_length(value: Length) -> Result<(), PrimitiveError> {
    if value < MIN_PRIMITIVE_SIZE || value > MAX_PRIMITIVE_SIZE {
        return Err(PrimitiveError::InvalidLength { value, min: MIN_PRIMITIVE_SIZE, max: MAX_PRIMITIVE_SIZE });
    }
    Ok(())
}

/// Validates a cone's top radius, where zero is the sharp case rather than a bad number.
///
/// A top radius below the smallest positive length is refused like any other dimension, so the
/// apex stays one number — zero — and never a nearly-zero ring the mesh cannot tell from it.
fn validate_top_radius(value: Length) -> Result<(), PrimitiveError> {
    if value.is_zero() {
        return Ok(());
    }
    if value < MIN_PRIMITIVE_SIZE || value > MAX_PRIMITIVE_SIZE {
        return Err(PrimitiveError::InvalidTopRadius { value, min: MIN_PRIMITIVE_SIZE, max: MAX_PRIMITIVE_SIZE });
    }
    Ok(())
}

/// Validates a curved primitive's side count.
fn validate_segments(segments: u32) -> Result<(), PrimitiveError> {
    if !(MIN_SEGMENTS..=MAX_SEGMENTS).contains(&segments) {
        return Err(PrimitiveError::SegmentCount { count: segments, min: MIN_SEGMENTS, max: MAX_SEGMENTS });
    }
    Ok(())
}

/// Validates a sphere's latitude band count.
fn validate_bands(bands: u32) -> Result<(), PrimitiveError> {
    if !(MIN_BANDS..=MAX_BANDS).contains(&bands) {
        return Err(PrimitiveError::BandCount { count: bands, min: MIN_BANDS, max: MAX_BANDS });
    }
    Ok(())
}

/// Serialized form of [`Sphere`]; validation happens in its `TryFrom` implementation.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SphereWire {
    radius: Length,
    segments: u32,
    bands: u32,
}

/// A sphere centred on the local origin.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "SphereWire")]
pub struct Sphere {
    radius: Length,
    segments: u32,
    bands: u32,
}

impl Sphere {
    /// Validates the radius, the side count and the band count, then stores them.
    pub fn new(radius: Length, segments: u32, bands: u32) -> Result<Self, PrimitiveError> {
        validate_length(radius)?;
        validate_segments(segments)?;
        validate_bands(bands)?;
        Ok(Self { radius, segments, bands })
    }

    /// The radius in every direction.
    pub const fn radius(&self) -> Length {
        self.radius
    }

    /// Sides around each latitude ring.
    pub const fn segments(&self) -> u32 {
        self.segments
    }

    /// Latitude bands between the two poles.
    pub const fn bands(&self) -> u32 {
        self.bands
    }
}

impl TryFrom<SphereWire> for Sphere {
    type Error = PrimitiveError;

    fn try_from(wire: SphereWire) -> Result<Self, Self::Error> {
        Self::new(wire.radius, wire.segments, wire.bands)
    }
}

/// Serialized form of [`Cone`]; validation happens in its `TryFrom` implementation.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ConeWire {
    base_radius: Length,
    top_radius: Length,
    height: Length,
    segments: u32,
}

/// A cone standing on its base circle, from `z = 0` up to `z = height`.
///
/// The base circle is the round the maker drew, so its centre is the node's origin and the body
/// grows on the object's local `+Z`, as a drawn profile is swept (ADR 0138). A top radius of zero is
/// a sharp cone with no top face; a larger one is the truncated cone a turned blank or a tapered
/// leg needs.
///
/// [ADR 0138]: ../docs/decisions/0138-a-solid-body-is-a-drawn-profile-swept-on-z.md
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "ConeWire")]
pub struct Cone {
    base_radius: Length,
    top_radius: Length,
    height: Length,
    segments: u32,
}

impl Cone {
    /// Validates the two radii, the height and the side count, then stores them.
    pub fn new(base_radius: Length, top_radius: Length, height: Length, segments: u32) -> Result<Self, PrimitiveError> {
        validate_length(base_radius)?;
        validate_top_radius(top_radius)?;
        validate_length(height)?;
        validate_segments(segments)?;
        Ok(Self { base_radius, top_radius, height, segments })
    }

    /// The radius of the base circle, which sits on the local `z = 0`.
    pub const fn base_radius(&self) -> Length {
        self.base_radius
    }

    /// The radius of the top circle, or zero for a sharp cone.
    pub const fn top_radius(&self) -> Length {
        self.top_radius
    }

    /// The distance from the base circle to the top, along the local `+Z`.
    pub const fn height(&self) -> Length {
        self.height
    }

    /// Sides around each circle.
    pub const fn segments(&self) -> u32 {
        self.segments
    }

    /// Whether the top radius is zero, so the cone ends in one apex vertex.
    pub const fn is_sharp(&self) -> bool {
        self.top_radius.is_zero()
    }
}

impl TryFrom<ConeWire> for Cone {
    type Error = PrimitiveError;

    fn try_from(wire: ConeWire) -> Result<Self, Self::Error> {
        Self::new(wire.base_radius, wire.top_radius, wire.height, wire.segments)
    }
}

/// The parameters one primitive shape is built from.
///
/// The inner tag is `shape`, because the outer [`Geometry`](crate::Geometry) payload already uses
/// `kind`. The parameters are the source of truth; the mesh is never stored beside them.
///
/// This is also what the creation commands carry: a maker starts a shape, never a history, so the
/// parameters travel alone and the steps arrive afterwards through `object.boolean.set`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "shape", rename_all = "snake_case")]
pub enum PrimitiveShape {
    /// A sphere built from latitude bands.
    Sphere(Sphere),
    /// A cone or a truncated cone built from two circles and a height.
    Cone(Cone),
}

impl PrimitiveShape {
    /// Which shape this is.
    pub const fn kind(&self) -> PrimitiveKind {
        match self {
            Self::Sphere(_) => PrimitiveKind::Sphere,
            Self::Cone(_) => PrimitiveKind::Cone,
        }
    }

    /// The lower-case shape name used in the scene tree.
    pub const fn shape_name(&self) -> &'static str {
        self.kind().shape_name()
    }

    /// The number of polygon faces the derived mesh has.
    ///
    /// It is computed from the parameters, so the card can show a face count without building the
    /// mesh: a sphere has one face per side and band, and a cone one side per segment plus its caps
    /// — the base cap always, the top cap only while the cone is truncated.
    pub const fn face_count(&self) -> usize {
        match self {
            Self::Sphere(sphere) => sphere.segments as usize * sphere.bands as usize,
            Self::Cone(cone) => cone.segments as usize + if cone.is_sharp() { 1 } else { 2 },
        }
    }

    /// The number of vertices the derived mesh has.
    pub const fn vertex_count(&self) -> usize {
        match self {
            Self::Sphere(sphere) => 2 + sphere.segments as usize * (sphere.bands as usize - 1),
            Self::Cone(cone) => {
                let segments = cone.segments as usize;
                segments + if cone.is_sharp() { 1 } else { segments }
            }
        }
    }

    /// The local bounding box of the derived surface, in micrometres.
    ///
    /// It is analytic, so the tree and the pivot do not need the evaluated mesh. A sphere is centred
    /// on the local zero; a cone stands on its base circle, so it reaches from `z = 0` to its height
    /// and its footprint is the wider of the two circles.
    pub const fn bounds_micrometres(&self) -> ([i64; 3], [i64; 3]) {
        match self {
            Self::Sphere(sphere) => {
                let radius = sphere.radius.micrometres();
                ([-radius; 3], [radius; 3])
            }
            Self::Cone(cone) => {
                let base = cone.base_radius.micrometres();
                let top = cone.top_radius.micrometres();
                let radius = if top > base { top } else { base };
                ([-radius, -radius, 0], [radius, radius, cone.height.micrometres()])
            }
        }
    }
}

/// Serialized form of [`Primitive`]; validation happens in its `TryFrom` implementation.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PrimitiveWire {
    base: PrimitiveShape,
    #[serde(default)]
    steps: Vec<Boolean>,
}

/// A parametric primitive **body**: the parameters its shape is built from and the steps cut from it.
///
/// A primitive is a solid, so it is an operand of a body boolean in both directions: a block
/// subtracted from a ball leaves a rectangular notch, and two balls leave a spherical scoop
/// ([ADR 0197]). The steps are frozen shapes, exactly as they are on a drawn body, and a body whose
/// step list is empty is built from its parameters alone — the analytic mesh the viewport draws.
///
/// [ADR 0197]: ../docs/decisions/0197-a-body-boolean-is-a-step-that-names-its-tool.md
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "PrimitiveWire")]
pub struct Primitive {
    base: PrimitiveShape,
    steps: Vec<Boolean>,
}

impl Primitive {
    /// A sphere with the given radius, side count and latitude bands, and no steps yet.
    pub fn sphere(radius: Length, segments: u32, bands: u32) -> Result<Self, PrimitiveError> {
        Ok(Self { base: PrimitiveShape::Sphere(Sphere::new(radius, segments, bands)?), steps: Vec::new() })
    }

    /// A cone or a truncated cone with the given base, top, height and side count, and no steps yet.
    pub fn cone(
        base_radius: Length,
        top_radius: Length,
        height: Length,
        segments: u32,
    ) -> Result<Self, PrimitiveError> {
        Ok(Self {
            base: PrimitiveShape::Cone(Cone::new(base_radius, top_radius, height, segments)?),
            steps: Vec::new(),
        })
    }

    /// Builds the default primitive of one kind from the creation size.
    ///
    /// The size is the full extent, which for the sphere is its diameter; the sphere uses the
    /// default tessellation. A drawn sphere bypasses this constructor: its radius is the circle the
    /// maker drew, so [`Primitive::sphere`] takes the number directly (ADR 0139).
    pub fn from_kind(kind: PrimitiveKind, size: Length) -> Result<Self, PrimitiveError> {
        validate_length(size)?;
        let radius = Length::from_micrometres(size.micrometres() / 2);
        match kind {
            PrimitiveKind::Sphere => Self::sphere(radius, DEFAULT_SEGMENTS, DEFAULT_BANDS),
            // A cone's size is its base diameter and its height at once; the drawn gesture supplies
            // both numbers itself, so this is the typed shortcut no surface builds.
            PrimitiveKind::Cone => Self::cone(radius, Length::ZERO, size, DEFAULT_SEGMENTS),
        }
    }

    /// The parameters the shape is built from.
    pub const fn base(&self) -> PrimitiveShape {
        self.base
    }

    /// The steps cut from the shape, in application order.
    pub fn steps(&self) -> &[Boolean] {
        &self.steps
    }

    /// Whether the body is still the plain shape, so its mesh comes from its parameters alone.
    pub fn is_plain(&self) -> bool {
        self.steps.is_empty()
    }

    /// Appends a boolean step after the shape built so far.
    ///
    /// The step carries the shape it cuts with, so nothing is resolved here and no node can be
    /// named that a later load could not find (ADR 0197). The step list and the nesting are bounded
    /// where the steps are stored, so a hostile file cannot make every walk over a document recurse
    /// without a limit.
    pub fn push_boolean(&mut self, boolean: Boolean) -> Result<(), PrimitiveError> {
        if self.steps.len() >= MAX_OBJECT_FEATURES {
            return Err(PrimitiveError::StepCount { count: self.steps.len(), max: MAX_OBJECT_FEATURES });
        }
        let depth = 1usize.saturating_add(boolean.tool().body().boolean_depth());
        if depth > MAX_BOOLEAN_DEPTH {
            return Err(PrimitiveError::TooDeepBooleans { max: MAX_BOOLEAN_DEPTH });
        }
        self.steps.push(boolean);
        Ok(())
    }

    /// Removes the step at `index` and returns it.
    pub fn remove_step(&mut self, index: usize) -> Result<Boolean, PrimitiveError> {
        if index >= self.steps.len() {
            return Err(PrimitiveError::StepIndex { index, count: self.steps.len() });
        }
        Ok(self.steps.remove(index))
    }

    /// Which shape this primitive is.
    pub const fn kind(&self) -> PrimitiveKind {
        self.base.kind()
    }

    /// The lower-case shape name used in the scene tree.
    pub const fn shape_name(&self) -> &'static str {
        self.base.shape_name()
    }

    /// The number of polygon faces the plain shape's derived mesh has.
    ///
    /// A body with steps is built by the geometry kernel, so its face count is what the kernel
    /// answers; this describes the shape the steps start from.
    pub const fn face_count(&self) -> usize {
        self.base.face_count()
    }

    /// The number of vertices the plain shape's derived mesh has.
    pub const fn vertex_count(&self) -> usize {
        self.base.vertex_count()
    }

    /// The vertex and face counts of the plain shape's derived mesh.
    pub const fn mesh_counts(&self) -> (usize, usize) {
        (self.vertex_count(), self.face_count())
    }

    /// The local bounding box of the derived body, in micrometres.
    ///
    /// A sphere is centred on the local zero; a cone stands on its base circle. A body with steps
    /// keeps the shape's box: a cut can only remove material from it.
    pub const fn bounds_micrometres(&self) -> ([i64; 3], [i64; 3]) {
        self.base.bounds_micrometres()
    }
}

impl TryFrom<PrimitiveWire> for Primitive {
    type Error = PrimitiveError;

    fn try_from(wire: PrimitiveWire) -> Result<Self, Self::Error> {
        let mut primitive = Primitive { base: wire.base, steps: Vec::new() };
        for boolean in wire.steps {
            primitive.push_boolean(boolean)?;
        }
        Ok(primitive)
    }
}

#[cfg(test)]
mod tests {
    use crate::testing::millimetres;

    use super::*;

    #[test]
    fn the_creation_size_builds_the_default_sphere() {
        let size = millimetres(100);
        let built = Primitive::from_kind(PrimitiveKind::Sphere, size).expect("sphere");
        let PrimitiveShape::Sphere(sphere) = built.base() else {
            panic!("a sphere");
        };
        assert_eq!(sphere.radius(), millimetres(50), "the diameter is the creation size");
        assert_eq!(sphere.segments(), DEFAULT_SEGMENTS);
        assert_eq!(sphere.bands(), DEFAULT_BANDS);
    }

    #[test]
    fn counts_and_bounds_follow_the_parameters() {
        let sphere = Primitive::sphere(millimetres(30), 8, 4).expect("sphere");
        assert_eq!(sphere.mesh_counts(), (2 + 8 * 3, 8 * 4));
        assert_eq!(sphere.bounds_micrometres(), ([-30_000; 3], [30_000; 3]));
        assert_eq!(sphere.kind(), PrimitiveKind::Sphere);
        assert_eq!(sphere.shape_name(), "sphere");
    }

    #[test]
    fn a_bad_dimension_is_rejected() {
        for value in [
            Length::ZERO,
            Length::from_micrometres(1),
            MAX_PRIMITIVE_SIZE.checked_add(Length::from_micrometres(1)).expect("sum"),
        ] {
            let error = Primitive::sphere(value, DEFAULT_SEGMENTS, DEFAULT_BANDS).expect_err("a bad radius");
            assert!(matches!(error, PrimitiveError::InvalidLength { .. }), "error was {error:?}");
        }
    }

    #[test]
    fn a_bad_tessellation_is_rejected() {
        assert_eq!(
            Primitive::sphere(millimetres(10), MAX_SEGMENTS + 1, DEFAULT_BANDS),
            Err(PrimitiveError::SegmentCount { count: MAX_SEGMENTS + 1, min: MIN_SEGMENTS, max: MAX_SEGMENTS })
        );
        assert_eq!(
            Primitive::sphere(millimetres(10), DEFAULT_SEGMENTS, 1),
            Err(PrimitiveError::BandCount { count: 1, min: MIN_BANDS, max: MAX_BANDS })
        );
    }

    #[test]
    fn a_cone_keeps_its_two_circles_and_its_height() {
        let cone = Primitive::cone(millimetres(40), millimetres(20), millimetres(100), 24).expect("cone");
        let PrimitiveShape::Cone(stored) = cone.base() else {
            panic!("a cone");
        };
        assert_eq!(stored.base_radius(), millimetres(40));
        assert_eq!(stored.top_radius(), millimetres(20));
        assert_eq!(stored.height(), millimetres(100));
        assert_eq!(stored.segments(), 24);
        assert!(!stored.is_sharp(), "a top radius is a truncated cone");
        assert_eq!(cone.kind(), PrimitiveKind::Cone);
        assert_eq!(cone.shape_name(), "cone");
    }

    #[test]
    fn a_zero_top_radius_is_the_sharp_cone() {
        let cone = Primitive::cone(millimetres(40), Length::ZERO, millimetres(100), 32).expect("a sharp cone");
        let PrimitiveShape::Cone(stored) = cone.base() else {
            panic!("a cone");
        };
        assert!(stored.is_sharp());
        // A sharp cone has one side face per segment and the base cap; a truncated one adds the top.
        assert_eq!(cone.mesh_counts(), (33, 33));
        let truncated = Primitive::cone(millimetres(40), millimetres(10), millimetres(100), 32).expect("frustum");
        assert_eq!(truncated.mesh_counts(), (64, 34));
    }

    #[test]
    fn a_cone_stands_on_its_base_circle() {
        let cone = Primitive::cone(millimetres(40), millimetres(10), millimetres(100), 16).expect("cone");
        assert_eq!(cone.bounds_micrometres(), ([-40_000, -40_000, 0], [40_000, 40_000, 100_000]));
        // A top wider than the base is still a cone; the footprint follows the wider circle.
        let splayed = Primitive::cone(millimetres(10), millimetres(40), millimetres(100), 16).expect("cone");
        assert_eq!(splayed.bounds_micrometres(), ([-40_000, -40_000, 0], [40_000, 40_000, 100_000]));
    }

    #[test]
    fn a_bad_top_radius_is_refused_with_its_own_sentence() {
        let error = Primitive::cone(millimetres(40), Length::from_micrometres(1), millimetres(100), 16)
            .expect_err("a nearly-zero ring is not the apex");
        assert!(matches!(error, PrimitiveError::InvalidTopRadius { .. }), "error was {error:?}");
        let error = Primitive::cone(
            millimetres(40),
            MAX_PRIMITIVE_SIZE.checked_add(millimetres(1)).expect("sum"),
            millimetres(100),
            16,
        )
        .expect_err("past the range");
        assert!(matches!(error, PrimitiveError::InvalidTopRadius { .. }), "error was {error:?}");
        let error = Primitive::cone(Length::ZERO, Length::ZERO, millimetres(100), 16).expect_err("no base");
        assert!(matches!(error, PrimitiveError::InvalidLength { .. }), "error was {error:?}");
        let error = Primitive::cone(millimetres(40), Length::ZERO, millimetres(100), 2).expect_err("two sides");
        assert!(matches!(error, PrimitiveError::SegmentCount { .. }), "error was {error:?}");
    }

    #[test]
    fn a_cone_round_trips_and_an_unknown_parameter_is_an_error() {
        let cone = Primitive::cone(millimetres(40), Length::ZERO, millimetres(100), 24).expect("cone");
        let text = serde_json::to_string(&cone).expect("serialize");
        assert!(text.contains("\"shape\":\"cone\""), "the shape is the inner tag: {text}");
        assert!(text.contains("\"base_radius\"") && text.contains("\"top_radius\""), "both circles: {text}");
        let decoded: Primitive = serde_json::from_str(&text).expect("deserialize");
        assert_eq!(decoded, cone);
        let extra = r#"{"shape":"cone","base_radius":40000,"top_radius":0,"height":100000,"segments":24,"bands":16}"#;
        assert!(serde_json::from_str::<Primitive>(extra).is_err(), "a sphere parameter is not a cone's");
        let bad = r#"{"shape":"cone","base_radius":40000,"top_radius":-1,"height":100000,"segments":24}"#;
        assert!(serde_json::from_str::<Primitive>(bad).is_err(), "a negative radius is refused on load");
    }

    #[test]
    fn a_primitive_round_trips_with_its_shape_tag() {
        let sphere = Primitive::sphere(millimetres(100), 24, 12).expect("sphere");
        let text = serde_json::to_string(&sphere).expect("serialize");
        assert!(text.contains("\"shape\":\"sphere\""), "the shape is the inner tag: {text}");
        assert!(!text.contains("\"kind\""), "the geometry tag is not repeated: {text}");
        let decoded: Primitive = serde_json::from_str(&text).expect("deserialize");
        assert_eq!(decoded, sphere);
    }

    #[test]
    fn a_retired_box_or_cylinder_shape_is_not_a_primitive() {
        // A box and a cylinder are drawn profiles now (ADR 0138), so schema 25 refuses the shapes
        // the retired parameters used to build instead of converting them silently (ADR 0096).
        for text in [
            r#"{"shape":"cuboid","width":600000,"depth":400000,"height":720000}"#,
            r#"{"shape":"cylinder","radius":50000,"height":100000,"segments":32}"#,
            r#"{"shape":"cube","edge":100000}"#,
        ] {
            assert!(serde_json::from_str::<Primitive>(text).is_err(), "a retired shape is not a primitive");
        }
    }

    #[test]
    fn an_unknown_field_is_rejected() {
        let text = r#"{"base":{"shape":"sphere","radius":100000,"segments":32,"bands":16},"steps":[]}"#;
        assert!(serde_json::from_str::<Primitive>(text).is_ok(), "the current layout still loads");
        let unknown =
            r#"{"base":{"shape":"sphere","radius":100000,"segments":32,"bands":16,"thickness":1},"steps":[]}"#;
        assert!(serde_json::from_str::<Primitive>(unknown).is_err(), "an unknown parameter is an error");
    }

    /// A step that cuts with a drawn body, standing at one placement.
    fn step(name: &str) -> Boolean {
        let body = drawn_body();
        Boolean::new(
            crate::BooleanOp::Difference,
            crate::BooleanTool::new(name, crate::ToolBody::Constructive(body), crate::Placement::IDENTITY)
                .expect("a tool"),
        )
    }

    /// A 10 x 10 ring swept 20 mm: a drawn body a step can cut with.
    fn drawn_body() -> crate::Constructive {
        let ring =
            crate::Path::rectangle([Length::ZERO, Length::ZERO], [millimetres(10), millimetres(10)]).expect("a ring");
        crate::Constructive::extrude(ring, millimetres(20)).expect("a body")
    }

    #[test]
    fn a_primitive_carries_the_steps_cut_from_it() {
        let mut ball = Primitive::sphere(millimetres(30), 16, 8).expect("ball");
        assert!(ball.is_plain(), "a new primitive is the plain shape");
        ball.push_boolean(step("Block")).expect("a step");
        assert_eq!(ball.steps().len(), 1);
        assert!(!ball.is_plain());

        let text = serde_json::to_string(&ball).expect("serialize");
        let mut decoded: Primitive = serde_json::from_str(&text).expect("load");
        assert_eq!(decoded, ball, "the steps travel with the shape");
        assert_eq!(decoded.base().kind(), PrimitiveKind::Sphere, "and the parameters are still the base");

        let removed = decoded.remove_step(0).expect("the step");
        assert_eq!(removed.tool().name(), "Block");
        assert!(decoded.is_plain(), "a body is plain again once its last step is gone");
        assert_eq!(decoded.remove_step(0), Err(PrimitiveError::StepIndex { index: 0, count: 0 }));
    }

    #[test]
    fn a_primitive_refuses_a_step_that_nests_too_deep() {
        // A chain of drawn bodies, each cutting with the previous one, reaches the document's limit;
        // a ball that cut with it would stand one level past it.
        let mut nested = drawn_body();
        for _ in 1..MAX_BOOLEAN_DEPTH {
            let inner =
                crate::BooleanTool::new("Inner", crate::ToolBody::Constructive(nested), crate::Placement::IDENTITY)
                    .expect("a tool");
            let mut outer = drawn_body();
            outer.push_boolean(Boolean::new(crate::BooleanOp::Difference, inner)).expect("a level inside the limit");
            nested = outer;
        }
        let tool = crate::BooleanTool::new("Outer", crate::ToolBody::Constructive(nested), crate::Placement::IDENTITY)
            .expect("a tool");
        let mut ball = Primitive::sphere(millimetres(30), 8, 4).expect("ball");
        assert_eq!(
            ball.push_boolean(Boolean::new(crate::BooleanOp::Difference, tool)),
            Err(PrimitiveError::TooDeepBooleans { max: MAX_BOOLEAN_DEPTH })
        );
        assert!(ball.is_plain(), "a refused step leaves the body untouched");
    }
}
