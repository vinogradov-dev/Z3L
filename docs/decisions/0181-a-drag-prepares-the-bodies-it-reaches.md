# 0181 — A drag prepares the bodies it reaches, and the port can cast a ray

Date: 2026-11-19
Status: accepted

## Context

The contact port (ADR 0179) gave a body its own surface, and the object move measured a pair with
it. The rule prepared every obstacle when the drag started: three hundred bodies in a workshop-wide
scene meant three hundred convex hulls, forty milliseconds, and a hitch on the frame that began the
gesture. Every ask then measured all three hundred pairs, most of them across metres of empty air.

A probe — the ray behind a niche, a callout or a ruler — had the same gap from the other side: it
met the world box around a body, and the port could answer pairs but not a ray.

## Decision

**The port answers a ray.** `Solid::ray(origin, direction) -> Option<Impact>` returns the distance,
the point and the face normal of the first surface a ray meets; `contact-parry` implements it with
`RayCast`. The answer is a heading's distance in millimetres, like `travel`.

**A rule prepares a body when a pair first asks for it.** An obstacle keeps the geometry the shape
comes from and a shared `OnceLock`; the adapter is kept by the rule as an `Arc` (a new
`Contact::handle`, cheap because an adapter is a service, not a state). The build walks boxes only,
and the first ask on a pair builds the one shape it needs. The rule also culls by the world box:
two boxes that stand apart cannot be bodies that touch, because a body lies inside its box, and a
travel cuts only the pairs that overlap across it.

**The probe keeps reading boxes.** Measured against the bodies of an ordinary scene, the port's ray
moved a cell's plane by rounding rather than by a face — and a cell is where a board is cut, so the
suite's frame tests caught it. The probe therefore reads the boxes it always read, while
`probe`/`niche`/`fill_plan` already take the adapter: measuring a cell against a body the boxes
cannot describe (a notched panel, a plate with a hole) is its own change with its own test (X92).

## Consequences

- A drag in a scene of three hundred bodies builds in under a frame and answers a hundred and twenty
  asks in another: `a_drag_in_a_hall_of_three_hundred_bodies_is_built_at_once_and_measured_where_it_reaches`.
- The first ask on a pair costs the shape it needs, so a drag that walks into one neighbour pays for
  one neighbour.
- `MoveRule` holds the adapter, so a rule is no longer a value of the document alone. It stays
  single-threaded (`Rc`), which is what the shell is.
- The probe's deferral is named in the code and in the plan (X92), not left as an unexplained
  inconsistency: a cell cut from rounded planes would be a silent defect in the parts made from it.
