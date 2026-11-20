//! A node that holds children: an ordinary group or a furniture component.
//!
//! There is one container kind. A container is a component when a class, a prototype reference or
//! an explicit marker says so; an unmarked container is a group and can be flattened.
//! The project root is a container too, with an immutable id and a zero pivot.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use units::Length;

use crate::footprint::{FootprintError, anchored_ring};
use crate::{
    ClassCode, Contour, Footprint, Id, Node, PivotLocal, Placement, Properties, PropertyError, PropertyValue,
    SYSTEM_CLASS, SYSTEM_COMPONENT, SYSTEM_PROTOTYPE_ID, SYSTEM_PROTOTYPE_PARAMETERS, SYSTEM_PROTOTYPE_VERSION,
};

/// The authored box of a product: its width, depth, height and the shape its base is cut to.
///
/// The document stores the box a human decided on, never the bounds of its parts (ADR 0097). The
/// three sides are named after the product frame axes: `x` is the width, `y` the depth and `z` the
/// height. A product anchors at the minimum corner of its base rectangle, so the reserved space is
/// `[0..x] × [0..y] × [0..z]` in that frame and needs no offset. The [`Footprint`] cuts that base
/// rectangle — a corner kitchen cabinet with one corner off, an end table drawn as a ring — and `x`
/// and `y` stay the bounds of whatever shape it is, so every reader that only wants the reserved box
/// still gets one answer (ADR 0159).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "EnvelopeWire")]
pub struct Envelope {
    x: Length,
    y: Length,
    z: Length,
    #[serde(default, skip_serializing_if = "Footprint::is_rectangle")]
    footprint: Footprint,
}

impl Envelope {
    /// The largest side an envelope may have: 1000 m, the panel dimension ceiling.
    pub const MAX_SIDE: Length = Length::from_micrometres(1_000_000_000);

    /// Builds an envelope from three exact side lengths: the plain rectangular box.
    ///
    /// Every side must be positive and inside [`Envelope::MAX_SIDE`]; a bad number is a typed
    /// error, never a clamp.
    pub fn new(x: Length, y: Length, z: Length) -> Result<Self, EnvelopeError> {
        Self::with_footprint(x, y, z, Footprint::rectangle())
    }

    /// Builds an envelope whose base rectangle is cut as `footprint` says.
    ///
    /// The three sides are validated first, then the shape against them: a corner cut that leaves no
    /// straight edge and a ring whose bounds are not the sides are both typed refusals, never a
    /// clamp and never a silently reshaped box.
    pub fn with_footprint(x: Length, y: Length, z: Length, footprint: Footprint) -> Result<Self, EnvelopeError> {
        validate_side("width", x)?;
        validate_side("depth", y)?;
        validate_side("height", z)?;
        footprint.outline(x, y).map_err(|source| EnvelopeError::Footprint { source })?;
        Ok(Self { x, y, z, footprint })
    }

    /// Builds an envelope whose base is a closed ring drawn on the plane.
    ///
    /// The ring is anchored at its own minimum corner and its bounds become the width and the depth,
    /// so the product's zero stays its base min corner and a drawn piece needs no offset (ADR 0042,
    /// ADR 0082). The height is still the third side, pulled or typed like any other.
    pub fn with_ring(contour: &Contour, z: Length) -> Result<Self, EnvelopeError> {
        let ring = anchored_ring(contour).map_err(|source| EnvelopeError::Footprint { source })?;
        let (min, max) = ring.bounds_micrometres();
        let span = |side: &'static str, low: i64, high: i64| -> Result<Length, EnvelopeError> {
            let span = high.checked_sub(low).ok_or(EnvelopeError::RingBounds { side, low, high })?;
            Ok(Length::from_micrometres(span))
        };
        let width = span("width", min[0], max[0])?;
        let depth = span("depth", min[1], max[1])?;
        Self::with_footprint(width, depth, z, Footprint::ring(ring))
    }

    /// The width along the product frame's X axis.
    pub const fn x(&self) -> Length {
        self.x
    }

    /// The depth along the product frame's Y axis.
    pub const fn y(&self) -> Length {
        self.y
    }

    /// The height along the product frame's Z axis.
    pub const fn z(&self) -> Length {
        self.z
    }

    /// The three sides in frame order.
    pub const fn components(&self) -> [Length; 3] {
        [self.x, self.y, self.z]
    }

    /// The shape the base rectangle is cut to.
    pub const fn footprint(&self) -> &Footprint {
        &self.footprint
    }
}

/// The serialized form of [`Envelope`]; validation happens in its `TryFrom` implementation.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct EnvelopeWire {
    x: Length,
    y: Length,
    z: Length,
    #[serde(default)]
    footprint: Footprint,
}

impl TryFrom<EnvelopeWire> for Envelope {
    type Error = EnvelopeError;

    fn try_from(wire: EnvelopeWire) -> Result<Self, Self::Error> {
        Self::with_footprint(wire.x, wire.y, wire.z, wire.footprint)
    }
}

/// Why a product envelope is not usable.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum EnvelopeError {
    /// A side is zero or negative.
    #[error("product {side} must be positive, got {value}")]
    NonPositive {
        /// The rejected side.
        side: &'static str,
        /// The rejected value.
        value: Length,
    },
    /// A side exceeds the supported maximum.
    #[error("product {side} must not exceed {max}, got {value}")]
    TooLarge {
        /// The rejected side.
        side: &'static str,
        /// The rejected value.
        value: Length,
        /// The largest accepted value for this side.
        max: Length,
    },
    /// The footprint cannot be drawn at the sides it is stored with.
    #[error("the product footprint is not usable: {source}")]
    Footprint {
        /// Why the shape was refused.
        #[source]
        source: FootprintError,
    },
    /// A drawn ring's bounds cannot be expressed as two sides.
    #[error("the product ring's {side} spans {low}..{high} micrometres, which is not a length")]
    RingBounds {
        /// The side that could not be measured.
        side: &'static str,
        /// The ring's low coordinate on that axis, in micrometres.
        low: i64,
        /// The ring's high coordinate on that axis, in micrometres.
        high: i64,
    },
}

fn validate_side(side: &'static str, value: Length) -> Result<(), EnvelopeError> {
    if !value.is_positive() {
        return Err(EnvelopeError::NonPositive { side, value });
    }
    if value > Envelope::MAX_SIDE {
        return Err(EnvelopeError::TooLarge { side, value, max: Envelope::MAX_SIDE });
    }
    Ok(())
}

/// A node that can hold children: an ordinary group or a furniture component.
///
/// There is one container kind. "Assembly" is a word for a container marked as a component — a
/// cabinet, a drawer box, a facade or a hinge — by a non-zero `class`, a `prototype.id` or an
/// explicit `component = true`. An unmarked container is an ordinary group and can be flattened by
/// `node.ungroup`. The project root is a container.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Container {
    id: Id,
    name: String,
    placement: Placement,
    #[serde(default)]
    pivot_local: PivotLocal,
    children: Vec<Node>,
    #[serde(default)]
    properties: Properties,
    /// The authored box of a product; a group, a component and the root carry none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    envelope: Option<Envelope>,
}

impl Container {
    /// Creates an ordinary container. Global id uniqueness is checked by [`Project::validate`].
    ///
    /// The pivot starts at zero: a container is born at its own local zero, and a caller that wants
    /// the container grabbed at its content (a group) says so through [`crate::rebase_origin_at`],
    /// which also moves the placement. The root has its pivot forced back to zero by
    /// [`Project::new`].
    pub fn new(id: Id, name: impl Into<String>, placement: Placement, children: Vec<Node>) -> Self {
        Self {
            id,
            name: name.into(),
            placement,
            pivot_local: PivotLocal::ZERO,
            children,
            properties: Properties::new(),
            envelope: None,
        }
    }

    /// The stable container id.
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

    /// The manipulation pivot in the container's own frame.
    pub const fn pivot_local(&self) -> PivotLocal {
        self.pivot_local
    }

    /// The panels and nested containers in this container.
    pub fn children(&self) -> &[Node] {
        &self.children
    }

    /// Whether the two containers are the same node, ignoring the children they hold.
    ///
    /// A container's value carries its children, so comparing two of them says *changed* for every
    /// ancestor of a node that was created or removed. A caller that asks what a command changed
    /// wants the node itself — its name, its placement and pivot, its properties and its envelope —
    /// because a child that changed is reported as itself, by its own id.
    pub fn same_node(&self, other: &Self) -> bool {
        self.id == other.id
            && self.name == other.name
            && self.placement == other.placement
            && self.pivot_local == other.pivot_local
            && self.properties == other.properties
            && self.envelope == other.envelope
    }

    /// The typed properties of this container.
    pub const fn properties(&self) -> &Properties {
        &self.properties
    }

    /// The children for mutation inside the crate.
    pub(crate) fn children_mut(&mut self) -> &mut Vec<Node> {
        &mut self.children
    }

    /// Consumes the container, handing its children to the caller.
    pub(crate) fn into_children(self) -> Vec<Node> {
        self.children
    }

    /// Replaces the user-visible name; the caller has already rejected a blank one.
    pub(crate) fn set_name(&mut self, name: impl Into<String>) {
        self.name = name.into();
    }

    /// Replaces the placement relative to the parent frame.
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

    /// The class stored on the container, or [`ClassCode::UNCLASSIFIED`] when it has none.
    pub fn class(&self) -> ClassCode {
        match self.properties.get(SYSTEM_CLASS) {
            Some(PropertyValue::Class { value }) => *value,
            _ => ClassCode::UNCLASSIFIED,
        }
    }

    /// Whether the container plays the product role: its class has `level = Product` (ADR 0097).
    ///
    /// The role is a class trait and not a node kind, so a product is an ordinary container that
    /// answers this question; the envelope is the box it reserves.
    pub fn is_product(&self) -> bool {
        self.class().is_product_class()
    }

    /// The authored box of a product, or `None` for the root, a group or another component.
    pub const fn envelope(&self) -> Option<&Envelope> {
        self.envelope.as_ref()
    }

    /// Stores or clears the authored box; [`crate::Project::add_product`] is the creation path.
    pub(crate) fn set_envelope(&mut self, envelope: Option<Envelope>) {
        self.envelope = envelope;
    }

    /// Whether the container is marked as a furniture component.
    ///
    /// A non-zero class, a prototype reference or an explicit `component = true` all mark
    /// a component; an explicit `component = false` does not erase the other two markers.
    pub fn is_component(&self) -> bool {
        if let Some(PropertyValue::Bool { value: true }) = self.properties.get(SYSTEM_COMPONENT) {
            return true;
        }
        if let Some(PropertyValue::Class { value }) = self.properties.get(SYSTEM_CLASS)
            && !value.is_unclassified()
        {
            return true;
        }
        self.properties.contains_key(SYSTEM_PROTOTYPE_ID)
    }

    /// Sets the explicit component marker through the validated property path.
    pub fn set_component_marker(&mut self, component: bool) -> Result<(), PropertyError> {
        self.properties.insert(SYSTEM_COMPONENT, PropertyValue::bool(component)).map(|_| ())
    }

    /// Clears the component marker, the prototype keys, the class and the product envelope.
    ///
    /// A product is a component whose class gave it a role and whose envelope gave it a box, so a
    /// container converted to a plain group loses both in the same command (ADR 0097).
    pub fn clear_component_marker(&mut self) {
        self.envelope = None;
        self.properties.remove(SYSTEM_COMPONENT);
        self.properties.remove(SYSTEM_CLASS);
        self.properties.remove(SYSTEM_PROTOTYPE_ID);
        self.properties.remove(SYSTEM_PROTOTYPE_VERSION);
        self.properties.remove(SYSTEM_PROTOTYPE_PARAMETERS);
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
