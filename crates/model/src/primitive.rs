//! Parametric primitives: the shape parameters a mesh is derived from.
//!
//! A primitive stores what the user can still change — a radius, the number of sides and latitude
//! bands — and never the mesh. The mesh is derived for the viewport and for `object.collapse`, so
//! raising a radius rebuilds one object rather than rewriting its vertices ([ADR 0054]). A box or a
//! cylinder is not a primitive: it is a drawn profile swept on Z, stored as a constructive body
//! ([ADR 0138]), so the sphere is the last shape whose numbers are a parameter set.
//!
//! [ADR 0054]: ../docs/decisions/0054-a-primitive-keeps-the-parameters-that-built-it.md
//! [ADR 0138]: ../docs/decisions/0138-a-solid-body-is-a-drawn-profile-swept-on-z.md

use serde::{Deserialize, Serialize};
use thiserror::Error;
use units::Length;

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
/// A box and a cylinder are constructive bodies a maker draws ([ADR 0138]); the sphere is the shape
/// whose numbers still build it, and the cone joins it when it is modelled.
///
/// [ADR 0138]: ../docs/decisions/0138-a-solid-body-is-a-drawn-profile-swept-on-z.md
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrimitiveKind {
    /// A sphere centred on the object's local origin.
    Sphere,
}

impl PrimitiveKind {
    /// The shape name shown to the user.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Sphere => "Sphere",
        }
    }

    /// The lower-case shape name used in the scene tree.
    pub const fn shape_name(self) -> &'static str {
        match self {
            Self::Sphere => "sphere",
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
}

/// Validates one primitive dimension.
fn validate_length(value: Length) -> Result<(), PrimitiveError> {
    if value < MIN_PRIMITIVE_SIZE || value > MAX_PRIMITIVE_SIZE {
        return Err(PrimitiveError::InvalidLength { value, min: MIN_PRIMITIVE_SIZE, max: MAX_PRIMITIVE_SIZE });
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
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SphereWire {
    radius: Length,
    segments: u32,
    bands: u32,
}

/// A sphere centred on the local origin.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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

/// A parametric primitive: the shape and the parameters that build its mesh.
///
/// The inner tag is `shape`, because the outer [`Geometry`](crate::Geometry) payload already uses
/// `kind`. Parameters are the source of truth; the mesh is never stored beside them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "shape", rename_all = "snake_case")]
pub enum Primitive {
    /// A sphere built from latitude bands.
    Sphere(Sphere),
}

impl Primitive {
    /// A sphere with the given radius, side count and latitude bands.
    pub fn sphere(radius: Length, segments: u32, bands: u32) -> Result<Self, PrimitiveError> {
        Ok(Self::Sphere(Sphere::new(radius, segments, bands)?))
    }

    /// Builds the default primitive of one kind from the creation size.
    ///
    /// The size is the full extent, which for the sphere is its diameter; the sphere uses the
    /// default tessellation.
    pub fn from_kind(kind: PrimitiveKind, size: Length) -> Result<Self, PrimitiveError> {
        validate_length(size)?;
        let radius = Length::from_micrometres(size.micrometres() / 2);
        match kind {
            PrimitiveKind::Sphere => Self::sphere(radius, DEFAULT_SEGMENTS, DEFAULT_BANDS),
        }
    }

    /// Which shape this primitive is.
    pub const fn kind(&self) -> PrimitiveKind {
        match self {
            Self::Sphere(_) => PrimitiveKind::Sphere,
        }
    }

    /// The lower-case shape name used in the scene tree.
    pub const fn shape_name(&self) -> &'static str {
        self.kind().shape_name()
    }

    /// The number of polygon faces the derived mesh has.
    ///
    /// It is computed from the parameters, so the card can show a face count without building the
    /// mesh: a sphere has one face per side and band.
    pub const fn face_count(&self) -> usize {
        match self {
            Self::Sphere(sphere) => sphere.segments as usize * sphere.bands as usize,
        }
    }

    /// The number of vertices the derived mesh has.
    pub const fn vertex_count(&self) -> usize {
        match self {
            Self::Sphere(sphere) => 2 + sphere.segments as usize * (sphere.bands as usize - 1),
        }
    }

    /// The vertex and face counts of the derived mesh.
    pub const fn mesh_counts(&self) -> (usize, usize) {
        (self.vertex_count(), self.face_count())
    }

    /// The local bounding box of the derived mesh, in micrometres.
    ///
    /// It is analytic, so the tree and the pivot do not need the evaluated mesh. A sphere is centred
    /// on the local zero.
    pub const fn bounds_micrometres(&self) -> ([i64; 3], [i64; 3]) {
        match self {
            Self::Sphere(sphere) => {
                let radius = sphere.radius.micrometres();
                ([-radius; 3], [radius; 3])
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::testing::millimetres;

    use super::*;

    #[test]
    fn the_creation_size_builds_the_default_sphere() {
        let size = millimetres(100);
        let Primitive::Sphere(sphere) = Primitive::from_kind(PrimitiveKind::Sphere, size).expect("sphere");
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
        let box_text = r#"{"shape":"cuboid","width":600000,"depth":400000,"height":720000}"#;
        assert!(serde_json::from_str::<Primitive>(box_text).is_err(), "there is no cuboid primitive");
        let cylinder_text = r#"{"shape":"cylinder","radius":50000,"height":100000,"segments":32}"#;
        assert!(serde_json::from_str::<Primitive>(cylinder_text).is_err(), "there is no cylinder primitive");
        let cube_text = r#"{"shape":"cube","edge":100000}"#;
        assert!(serde_json::from_str::<Primitive>(cube_text).is_err(), "the cube shape is gone with schema 22");
    }

    #[test]
    fn an_unknown_field_is_rejected() {
        let text = r#"{"shape":"sphere","radius":100000,"segments":32,"bands":16,"thickness":1}"#;
        assert!(serde_json::from_str::<Primitive>(text).is_err(), "an unknown parameter is an error");
    }
}
