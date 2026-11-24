# 0238. A panel's role is the plane it lies in

- Status: Accepted; refines 0034 (a panel is no longer born unclassified) and 0099 (the fill keeps
  naming its class)
- Date: 2026-11-24

## Context

A class was a creation parameter. `panel.create` takes `class: Option<ClassCode>`, the empty-space
tool names the class its cell was, and a standard part names the class of its kind — but a board
drawn on a plane carried nothing: a contour ring, a rectangle on a work plane and, later, a board
drawn by points all reached the document unclassified, so a maker had to pick a class by hand before
the class rules could say anything about the part. The owner's ask: every panel needs its role, and
the role follows from the plane the board lies in.

## Decision

**One function, in the model.** `model::role_of_plane(normal)` takes a panel's face normal — its
thickness axis, the way face `A` points — carried into the parent's frame, so the part's place in the
room does not decide its role, and answers one of three classes:

- a plane within 45° of the horizontal is a **shelf** (`horizontal panel`), and the tie at exactly
  45° is a shelf;
- otherwise the plane is vertical, and its normal within 45° of the parent's `X` axis is a **post**
  (`vertical panel`); anything else, the tie included, is a **wall** (`back panel`).

The thresholds are compared squared (`2 n_z² ≥ |n|²`, `2 n_x² > |n|²`), so both ties are exact and
no rounding decides one; a test walks a sphere of directions and asserts the three answers partition
it. A vector that is not a direction — zero length, or a component that is not finite — answers
`None`, and the caller leaves the part unclassified instead of inventing a role.

**The creation paths that have no niche apply it, and an explicit class wins.** A ring drawn on a
plane has no void to name its class, so the contour tool writes the derived class into
`panel.create`; the fill keeps naming its class, because the class is what picks the section pair;
the standard part keeps the class of its kind. `panel.create` itself still takes a class and does not
derive one, so a caller that has measured the part stays in charge.

**A role arrives with the part.** The class a create command carries is written by that command: the
document moves one revision and one undo entry covers the board and its role.

## Consequences

- A board is classified from the moment it exists, so the class rules — which sizes move, whether an
  outline may be reshaped, which machining a class admits — apply to it from birth instead of after
  a manual assignment.
- The card shows the role the part carries and the class picker overrides it through
  `node.property.set`, in one undo entry. A role is a default, not a cage.
- The by-points tool and the rectangle-on-a-plane tool read the same function when they land, so the
  rule is written once.
- A creation that carries a class settles the revision after writing it (`settle_revision`), which
  is 0234's one-operation-one-number rule applied to a creation rather than a batch.

## Related

- [0034](0034-nodes-carry-typed-properties.md) — the class as a typed property.
- [0099](0099-the-class-fill-trait-names-the-size-pair.md) — the `fill` trait the niche fill reads.
- [0234](0234-a-batch-moves-the-revision-once.md) — one operation, one revision.
- [`crates/model/src/panel/classify.rs`](../../crates/model/src/panel/classify.rs) — `role_of_plane`.
