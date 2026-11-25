//! Authored light objects: a sun, a point light or a rectangular emitter, with a colour and an
//! intensity.
//!
//! The built-in key light is a default, not a node. A light the user adds is document data with
//! undo, like a camera ([ADR 0036]).
//!
//! A rectangular emitter is the light a window is: a face of a given size in the light's own plane,
//! emitting along its local `-Z` the way a sun emits along its own (ADR 0247). The three kinds share
//! one colour and one intensity field — a multiplier for a sun, watts for the other two — and the
//! size is meaningful only for the rectangle, exactly as the watts are meaningless for the sun.
//!
//! [ADR 0036]: ../docs/decisions/0036-cameras-and-lights-are-scene-objects.md
//! [ADR 0247]: ../docs/decisions/0247-a-rectangular-light-is-a-face-at-watts.md

use schemars::JsonSchema;
use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use units::Length;

use crate::{Id, PivotLocal, Placement, Properties, PropertyError, PropertyValue};

/// The kinds of light a scene can carry.
///
/// A sun lights the whole scene from a direction; a point light radiates from its placement. The
/// built-in key light is a default, not a node ([ADR 0036]).
///
/// [ADR 0036]: ../docs/decisions/0036-cameras-and-lights-are-scene-objects.md
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LightKind {
    /// A directional light, like the sun.
    Sun,
    /// A light radiating from a point.
    Point,
    /// A light radiating from a rectangle in its own plane, like a window.
    Area,
}

impl LightKind {
    /// Whether this kind carries a size: only a rectangle has one, and its watts spread over its
    /// face instead of over a sphere (ADR 0039, ADR 0247).
    ///
    /// [ADR 0039]: ../docs/decisions/0039-point-lights-use-watts-and-inverse-square-falloff.md
    /// [ADR 0247]: ../docs/decisions/0247-a-rectangular-light-is-a-face-at-watts.md
    pub const fn has_area(self) -> bool {
        matches!(self, Self::Area)
    }
}

/// The size of a rectangular light's emitting face, in the light's own plane.
///
/// The face is centred on the light's origin and lies in its local `XY`, so a placement turns the
/// rectangle without moving its centre, and the light emits along local `-Z`. The size is carried
/// by every light — a sun and a point light read the default and never use it — because one wire
/// layout for the three kinds is what keeps a switch between them from losing an edit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LightSize {
    /// The side along the light's local `X`.
    width: Length,
    /// The side along the light's local `Y`.
    height: Length,
}

impl LightSize {
    /// The smallest side a rectangle may have.
    pub const MIN: Length = Length::from_micrometres(10_000);

    /// The largest side a rectangle may have: ten metres is a shop window, not a mistake to accept.
    pub const MAX: Length = Length::from_micrometres(10_000_000);

    /// A metre by a metre, which is a window a maker replaces with the real one.
    pub const DEFAULT: Self =
        Self { width: Length::from_micrometres(1_000_000), height: Length::from_micrometres(1_000_000) };

    /// Creates a size after checking that both sides are inside [`LightSize::MIN`]..=[`LightSize::MAX`].
    pub fn new(width: Length, height: Length) -> Result<Self, LightError> {
        if !valid_light_side(width) || !valid_light_side(height) {
            return Err(LightError::InvalidSize);
        }
        Ok(Self { width, height })
    }

    /// The side along the light's local `X`.
    pub const fn width(self) -> Length {
        self.width
    }

    /// The side along the light's local `Y`.
    pub const fn height(self) -> Length {
        self.height
    }
}

impl Default for LightSize {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// A light the user authored.
///
/// Like a camera it is a scene object with a parent, placement, properties, selection and undo. The
/// built-in key light is used when the scene has no light node.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LightNode {
    id: Id,
    name: String,
    placement: Placement,
    #[serde(default)]
    pivot_local: PivotLocal,
    #[serde(default)]
    properties: Properties,
    kind: LightKind,
    color: [f32; 3],
    intensity: f32,
    size: LightSize,
}

/// What a light is: its kind, colour, intensity and the size of a rectangle's face.
///
/// The four travel together because they are edited together — one command sets a light's kind,
/// colour, watts and size, and a light that took them one at a time could be caught half switched.
/// The size is a [`LightSize::DEFAULT`] for the two kinds that have no face.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LightSpec {
    /// Directional sun, point light or rectangular emitter.
    pub kind: LightKind,
    /// Linear RGB colour, each component inside `0.0..=1.0`.
    pub color: [f32; 3],
    /// A sun's irradiance multiplier, or a point or area light's radiant power in watts.
    pub intensity: f32,
    /// The face of a rectangular emitter; the two other kinds carry the default and ignore it.
    pub size: LightSize,
}

impl LightSpec {
    /// A sun of `color` and an intensity multiplier.
    pub const fn sun(color: [f32; 3], intensity: f32) -> Self {
        Self { kind: LightKind::Sun, color, intensity, size: LightSize::DEFAULT }
    }

    /// A point light of `color` and `intensity` watts.
    pub const fn point(color: [f32; 3], intensity: f32) -> Self {
        Self { kind: LightKind::Point, color, intensity, size: LightSize::DEFAULT }
    }

    /// A rectangle of `color`, `intensity` watts and `size`, facing along its own `-Z`.
    pub const fn area(color: [f32; 3], intensity: f32, size: LightSize) -> Self {
        Self { kind: LightKind::Area, color, intensity, size }
    }

    /// The same light of another kind, keeping its colour, watts and size.
    pub const fn with_kind(mut self, kind: LightKind) -> Self {
        self.kind = kind;
        self
    }
}

impl Default for LightSpec {
    fn default() -> Self {
        Self::sun([1.0; 3], 1.0)
    }
}

impl LightNode {
    /// Largest accepted intensity.
    pub const MAX_INTENSITY: f32 = 1_000_000.0;

    /// Creates a light after validating its colour, intensity and size.
    pub fn new(id: Id, name: impl Into<String>, placement: Placement, spec: LightSpec) -> Result<Self, LightError> {
        if !valid_light_color(spec.color) {
            return Err(LightError::InvalidColor);
        }
        if !valid_light_intensity(spec.intensity) {
            return Err(LightError::InvalidIntensity);
        }
        if !valid_light_size(spec.size) {
            return Err(LightError::InvalidSize);
        }
        Ok(Self {
            id,
            name: name.into(),
            placement,
            pivot_local: PivotLocal::ZERO,
            properties: Properties::new(),
            kind: spec.kind,
            color: spec.color,
            intensity: spec.intensity,
            size: spec.size,
        })
    }

    /// The stable light id.
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

    /// The manipulation pivot in the light's own frame.
    pub const fn pivot_local(&self) -> PivotLocal {
        self.pivot_local
    }

    /// The light kind.
    pub const fn kind(&self) -> LightKind {
        self.kind
    }

    /// The linear RGB colour.
    pub const fn color(&self) -> [f32; 3] {
        self.color
    }

    /// The intensity multiplier, or the radiant power in watts for the two lights that radiate.
    pub const fn intensity(&self) -> f32 {
        self.intensity
    }

    /// The face of a rectangular emitter, in the light's own plane.
    pub const fn size(&self) -> LightSize {
        self.size
    }

    /// The light's kind, colour, intensity and size, which is what a command writes.
    pub const fn spec(&self) -> LightSpec {
        LightSpec { kind: self.kind, color: self.color, intensity: self.intensity, size: self.size }
    }

    /// Replaces the kind, colour, intensity and size after validating them.
    pub fn set_params(&mut self, spec: LightSpec) -> Result<(), LightError> {
        if !valid_light_color(spec.color) {
            return Err(LightError::InvalidColor);
        }
        if !valid_light_intensity(spec.intensity) {
            return Err(LightError::InvalidIntensity);
        }
        if !valid_light_size(spec.size) {
            return Err(LightError::InvalidSize);
        }
        self.kind = spec.kind;
        self.color = spec.color;
        self.intensity = spec.intensity;
        self.size = spec.size;
        Ok(())
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

    /// The typed properties of this light.
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

pub(crate) fn valid_light_color(color: [f32; 3]) -> bool {
    color.iter().all(|value| value.is_finite() && (0.0..=1.0).contains(value))
}

pub(crate) fn valid_light_intensity(intensity: f32) -> bool {
    intensity.is_finite() && (0.0..=LightNode::MAX_INTENSITY).contains(&intensity)
}

fn valid_light_side(side: Length) -> bool {
    (LightSize::MIN..=LightSize::MAX).contains(&side)
}

pub(crate) fn valid_light_size(size: LightSize) -> bool {
    valid_light_side(size.width) && valid_light_side(size.height)
}

/// A serialization failure of a light node.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum LightError {
    /// A colour component is not finite or outside `0.0..=1.0`.
    #[error("light colour must be finite and inside the unit range")]
    InvalidColor,
    /// The intensity is not finite or outside its range.
    #[error("light intensity must be finite and non-negative")]
    InvalidIntensity,
    /// A rectangle's side is outside its range.
    #[error("a light's face must be at least 10 mm and at most 10 000 mm a side")]
    InvalidSize,
}

/// Serialized form of [`LightNode`]; validation happens in its `TryFrom` implementation.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LightNodeWire {
    id: Id,
    name: String,
    placement: Placement,
    #[serde(default)]
    pivot_local: PivotLocal,
    #[serde(default)]
    properties: Properties,
    kind: LightKind,
    color: [f32; 3],
    intensity: f32,
    size: LightSize,
}

impl TryFrom<LightNodeWire> for LightNode {
    type Error = LightError;

    fn try_from(wire: LightNodeWire) -> Result<Self, Self::Error> {
        if !valid_light_color(wire.color) {
            return Err(LightError::InvalidColor);
        }
        if !valid_light_intensity(wire.intensity) {
            return Err(LightError::InvalidIntensity);
        }
        if !valid_light_size(wire.size) {
            return Err(LightError::InvalidSize);
        }
        Ok(Self {
            id: wire.id,
            name: wire.name,
            placement: wire.placement,
            pivot_local: wire.pivot_local,
            properties: wire.properties,
            kind: wire.kind,
            color: wire.color,
            intensity: wire.intensity,
            size: wire.size,
        })
    }
}

impl<'de> Deserialize<'de> for LightNode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        LightNodeWire::deserialize(deserializer)?.try_into().map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;

    #[test]
    fn a_light_colour_or_intensity_outside_its_range_is_rejected_on_build_and_load() {
        let id = Id::new(6).expect("id");
        assert_eq!(
            LightNode::new(id, "Key", Placement::IDENTITY, LightSpec::point([1.5, 0.0, 0.0], 1.0)),
            Err(LightError::InvalidColor)
        );
        assert_eq!(
            LightNode::new(id, "Key", Placement::IDENTITY, LightSpec::point([1.0, 1.0, 1.0], f32::NAN)),
            Err(LightError::InvalidIntensity)
        );
        assert_eq!(
            LightNode::new(id, "Key", Placement::IDENTITY, LightSpec::point([0.0; 3], -1.0)),
            Err(LightError::InvalidIntensity)
        );

        let light = LightNode::new(id, "Key", Placement::IDENTITY, LightSpec::sun([1.0; 3], 1.0)).expect("light");
        let text = serde_json::to_string(&light).expect("serialize");
        let too_bright = text.replace("\"intensity\":1.0", "\"intensity\":-2.0");
        assert_ne!(too_bright, text);
        assert!(serde_json::from_str::<LightNode>(&too_bright).is_err());
    }

    #[test]
    fn a_rectangle_carries_a_size_and_the_two_other_kinds_carry_the_default() {
        let id = Id::new(7).expect("id");
        let window =
            LightSize::new(Length::from_micrometres(1_200_000), Length::from_micrometres(1_400_000)).expect("a window");
        let light =
            LightNode::new(id, "Window", Placement::IDENTITY, LightSpec::area([1.0; 3], 40.0, window)).expect("light");
        assert_eq!(light.kind(), LightKind::Area);
        assert_eq!(light.size(), window);
        assert_eq!(light.spec(), LightSpec::area([1.0; 3], 40.0, window));
        let sun = LightNode::new(id, "Key", Placement::IDENTITY, LightSpec::sun([1.0; 3], 1.0)).expect("sun");
        assert_eq!(sun.size(), LightSize::DEFAULT);
        assert!(LightKind::Area.has_area());
        assert!(!LightKind::Sun.has_area() && !LightKind::Point.has_area());
    }

    #[test]
    fn a_kind_this_build_does_not_know_is_a_readable_load_error() {
        // ADR 0096: a file from a newer build is a sentence, never a silent reinterpretation, and the
        // sentence names the word the document carried.
        let id = Id::new(9).expect("id");
        let light = LightNode::new(id, "Key", Placement::IDENTITY, LightSpec::sun([1.0; 3], 1.0)).expect("light");
        let text = serde_json::to_string(&light).expect("serialize");
        let unknown = text.replace("\"kind\":\"sun\"", "\"kind\":\"spot\"");
        assert_ne!(unknown, text);
        let error = serde_json::from_str::<LightNode>(&unknown).expect_err("a spot light is not a kind");
        assert!(error.to_string().contains("spot"), "{error}");
    }

    #[test]
    fn a_face_outside_its_range_is_refused_on_build_and_load() {
        let id = Id::new(8).expect("id");
        assert_eq!(
            LightSize::new(Length::from_micrometres(1_000), LightSize::DEFAULT.height()),
            Err(LightError::InvalidSize)
        );
        assert_eq!(
            LightSize::new(LightSize::DEFAULT.width(), Length::from_micrometres(20_000_000)),
            Err(LightError::InvalidSize)
        );
        assert_eq!(
            LightSize::new(Length::from_micrometres(-1_000_000), LightSize::DEFAULT.height()),
            Err(LightError::InvalidSize)
        );

        // Both extremes are inside the range, and a light of either survives the wire; a file whose
        // face is outside it is an error rather than a rectangle that gives no light.
        for size in [
            LightSize::new(LightSize::MIN, LightSize::MIN).expect("the smallest face"),
            LightSize::new(LightSize::MAX, LightSize::MAX).expect("the largest face"),
        ] {
            let light =
                LightNode::new(id, "Window", Placement::IDENTITY, LightSpec::area([1.0; 3], 1.0, size)).expect("light");
            let text = serde_json::to_string(&light).expect("serialize");
            let back: LightNode = serde_json::from_str(&text).expect("a face inside the range loads");
            assert_eq!(back.size(), size);
            let mut broken: serde_json::Value = serde_json::from_str(&text).expect("valid json");
            broken["size"]["width"] = serde_json::json!(0);
            assert!(serde_json::from_value::<LightNode>(broken).is_err(), "a face of no width is refused");
        }
    }
}
