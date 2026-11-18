# 0159. A product frame reserves a footprint, and a cut corner is two offsets

- Status: Accepted; refines 0097 (the envelope is a shape, not three lengths), 0100 (the frame is
  drawn and picked by its outline) and 0101 (the box tool edits the box the shape is cut from)
- Date: 2026-11-18

## Context

0097 made the product envelope three lengths and said in so many words that a sloped or reshaped one
is "a new field and a new decision". The owner now asks for exactly that field: the corner kitchen
cabinet whose reserved space is a rectangle with one 45-degree corner cut off, the end table with a
120-degree cut, and a frame drawn from a closed contour. None of the three is a box, and a maker
cannot set any of them up today.

The design pressure is on the *corner*, because that is where a furniture maker and a CAD program
usually disagree. A program stores an angle; a maker measures a setback along each edge with a tape,
because that is what the board is cut to. The panel already solved this: `CornerFeatureKind` stores a
chamfer as two setbacks (`incoming`, `outgoing`) and a fillet as a radius, and its outline generator
turns them into a ring. The product envelope has the same rectangle and the same corners, so one rule
serves both.

## Decision

**The envelope gains a footprint beside its sides.** `Envelope` keeps `x`, `y` and `z` — they stay the
**bounds** of the reserved shape, so the export row, the niche probe, the containment diagnostic and
everything that only wants a box keep one answer — and adds a `Footprint`:

| Shape | Stored | Drawn |
|---|---|---|
| `Rectangle` | nothing: the default | the four sides |
| `Cut` | one optional treatment per nominal corner | the rectangle with each corner cut |
| `Ring` | a closed contour anchored at the zero corner | the ring itself |

**A cut corner stores offsets, never an angle.** `Cut` holds `[Option<CornerFeatureKind>; 4]` in
`RectCorner` order, so a chamfer is `Chamfer { incoming, outgoing }` — two setbacks a maker measures —
and a fillet is `Radius { radius }`. A 45-degree cut is two equal offsets; a 120-degree cut is two
unequal ones. Nothing stores degrees, so no reader has to invert a tangent to know what was cut.

**The ring is the panel's own contour.** `Ring` holds the `Contour` a drawn shape produces, anchored
at its own minimum corner, so the product's zero stays the base minimum corner (0082, 0083). A frame
and a board cut to the same profile are therefore one shape in one code path, and a rounded end table
is an arc rather than a polygon nobody drew.

**One generator, one set of refusals.** `Footprint::outline` delegates to the panel's
`PanelOutline::ring_for`, so a cut that consumes a whole edge is the panel's own
`FeatureOverlap` — «corner features leave no straight edge on edge N» — and a ring whose bounds are
not the envelope's sides is `RingSizeMismatch`. The same call is the envelope's validation: a value
that exists was drawn successfully at least once.

**The scene draws and picks the outline, not the box.** `ProductEnvelope` carries the sampled outline
in local millimetres and the points where a vertical edge stands (a fillet's tangency is smooth, so it
stands no vertical). The renderer dashes the base ring, the top ring and one vertical per corner; a
rectangle therefore still draws the same twelve edges. Picking casts the ray into the product frame,
clips it to the box, and walks the intervals between its crossings of the outline: a click into a cut
corner, or outside a drawn ring, reserves nothing and picks nothing. The innermost-product rule now
compares the outline's own area times the height, so a cut piece really is the smaller one.

**The rectangle is the default and the cut is authored.** `Envelope::new` is unchanged for every
existing caller and a plain frame stores no footprint at all; `Envelope::with_footprint` and
`Envelope::with_ring` are the two authoring paths. A rectangle with no cut corner is never stored as
`Cut { … None … }`: one shape has one spelling.

**The grips read the shape.** A selected frame shows a corner grip only where its corner still exists,
so a cut corner has none, and a ring shows only the two height faces: its horizontal shape is drawn,
not dragged. The height faces edit `z` exactly as before.

## Consequences

- The schema gains one optional field on `Envelope`; by 0096 that is a bump and a regenerated golden
  file, no migration, no alias.
- Every reader that wants only a box keeps working unchanged through `x`/`y`/`z`; the niche and the
  containment test are bounds-based and stay so. The scene is the only reader that needs the shape.
- A future sloped or tapered envelope is still a new field: the footprint cuts the base only, and the
  top of the reserved space is the same outline.
- The card offers the footprint as structured rows: the corner type is chosen first and names the row
  that follows, so a fillet asks for a radius only after it is a fillet.
- Editing a cut or a ring needs no new command: `product.envelope.set` takes the whole envelope, as it
  already did.

## Related

- [0097](0097-a-product-is-a-container-with-an-authored-envelope.md) — the envelope this reshapes.
- [0100](0100-a-frame-is-created-opened-and-drawn.md) — the frame that is drawn and picked by it.
- [0101](0101-a-frame-and-a-box-are-one-tool.md) — the box the shape is cut from.
- [0082](0082-the-local-zero-is-the-creation-anchor.md),
  [0083](0083-a-panel-is-anchored-at-its-base-corner.md) — the anchor the ring is moved to.
- [`docs/domain/products.md`](../domain/products.md) — the product side of the reserved space.
