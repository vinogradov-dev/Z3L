//! A node that holds children: an ordinary group or a furniture component.
//!
//! There is one container kind. A container is a component when a class, a prototype reference or
//! an explicit marker says so; an unmarked container is a group and can be flattened.
//! The project root is a container too, with an immutable id and a zero pivot.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use units::Length;

use geom::{DVec2, Region};

use crate::footprint::{FootprintError, anchored_ring};
use crate::{
    ClassCode, Contour, Footprint, Id, Node, PivotLocal, Placement, Properties, PropertyError, PropertyValue,
    SYSTEM_CLASS, SYSTEM_COMPONENT, SYSTEM_PROTOTYPE_ID, SYSTEM_PROTOTYPE_PARAMETERS, SYSTEM_PROTOTYPE_VERSION,
};

/// How far outside the sides a point may sit and still belong to the reserved space, in millimetres.
///
/// One micrometre: the document's own step, so a point stored on a face belongs to the piece on both
/// sides of it.
const SPACE_TOLERANCE: f64 = 1.0e-3;

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

    /// Whether a point in the product's own frame lies inside the space the envelope reserves.
    ///
    /// The reserved space is the footprint's section from the floor to the ceiling, not the box around
    /// it: `[0..x] × [0..y]` bounds the shape and a cut corner or a drawn ring then takes its own part
    /// of that rectangle away (ADR 0159). A point outside the section is outside the piece, which is
    /// what the picking rule and the niche probe both ask. The ring is read by its own curve, so an arc
    /// answers for the material it removes and never for its chord.
    pub fn holds(&self, local: DVec2) -> bool {
        let width = self.x.millimetres_f64();
        let depth = self.y.millimetres_f64();
        if !local.is_finite()
            || local.x < -SPACE_TOLERANCE
            || local.y < -SPACE_TOLERANCE
            || local.x > width + SPACE_TOLERANCE
            || local.y > depth + SPACE_TOLERANCE
        {
            return false;
        }
        if self.footprint.is_rectangle() {
            return true;
        }
        // A stored footprint has already been drawn once by `with_footprint`, so a ring that no longer
        // generates is a broken value rather than a point outside the piece.
        let Ok(ring) = self.footprint.outline(self.x, self.y) else {
            return false;
        };
        // A point stored on the boundary belongs to the piece on both sides of it, as the micrometre
        // of slack above says: the containment test reads the inside and the distance reads the edge.
        Region::new(ring, Vec::new())
            .is_ok_and(|region| region.contains(local) || region.boundary_distance(local) <= SPACE_TOLERANCE)
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

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;
    use crate::{CornerFeatureKind, Path};

    fn millimetres(value: i64) -> Length {
        Length::from_millimetres(value).expect("whole millimetres fit a length")
    }

    #[test]
    fn a_plain_box_holds_every_point_of_its_sides() {
        let envelope = Envelope::new(millimetres(600), millimetres(500), millimetres(720)).expect("a box");
        assert!(envelope.holds(DVec2::new(300.0, 250.0)));
        assert!(envelope.holds(DVec2::ZERO), "the zero corner is the piece's own");
        assert!(envelope.holds(DVec2::new(600.0, 500.0)), "and so is the far one");
        assert!(!envelope.holds(DVec2::new(600.5, 250.0)), "nothing lies beside the box");
        assert!(!envelope.holds(DVec2::new(300.0, -0.5)));
        assert!(!envelope.holds(DVec2::new(f64::NAN, 0.0)));
    }

    #[test]
    fn a_cut_corner_is_not_part_of_the_reserved_space() {
        // The cut runs from (500, 0) to (600, 100): the free side of it is `y >= x - 500`.
        let footprint = Footprint::cut([
            None,
            Some(CornerFeatureKind::Chamfer { incoming: millimetres(100), outgoing: millimetres(100) }),
            None,
            None,
        ]);
        let envelope =
            Envelope::with_footprint(millimetres(600), millimetres(500), millimetres(720), footprint).expect("a cut");
        assert!(envelope.holds(DVec2::new(400.0, 10.0)), "the floor beside the cut is reserved");
        assert!(envelope.holds(DVec2::new(550.0, 60.0)), "and so is the space in front of the cut face");
        assert!(envelope.holds(DVec2::new(550.0, 50.0)), "the cut face itself belongs to the piece");
        assert!(!envelope.holds(DVec2::new(590.0, 10.0)), "the corner the frame gave away is not reserved");
        assert!(!envelope.holds(DVec2::new(550.0, 40.0)));
    }

    #[test]
    fn a_drawn_ring_is_read_by_its_own_curve_and_not_by_its_chords() {
        // An 80 mm radius on the bottom-right corner of a 600 x 500 case: the arc is centred at
        // (520, 80), so the point 75 mm from that centre is inside the frame while the chord between
        // the two tangency points leaves it outside.
        let footprint = Footprint::cut([None, Some(CornerFeatureKind::Radius { radius: millimetres(80) }), None, None]);
        let envelope = Envelope::with_footprint(millimetres(600), millimetres(500), millimetres(720), footprint)
            .expect("a radius");
        assert!(envelope.holds(DVec2::new(573.0, 27.0)), "between the chord and the arc, the frame still reserves it");
        assert!(!envelope.holds(DVec2::new(597.0, 3.0)), "and the corner outside the arc it does not");
    }

    #[test]
    fn a_drawn_ring_holds_the_space_inside_it() {
        let ring = Path::polyline(
            vec![
                [Length::ZERO, Length::ZERO],
                [millimetres(600), Length::ZERO],
                [millimetres(600), millimetres(560)],
                [millimetres(0), millimetres(560)],
            ],
            true,
        )
        .expect("a closed ring")
        .to_contour()
        .expect("a contour");
        let envelope = Envelope::with_ring(&ring, millimetres(720)).expect("a drawn frame");
        assert!(envelope.footprint().is_ring());
        assert!(envelope.holds(DVec2::new(300.0, 280.0)));
        assert!(!envelope.holds(DVec2::new(700.0, 280.0)), "the box beside the ring is not the piece");
    }
}
