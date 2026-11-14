# 0121. The pointer is a two-tone square, painted above the chrome

- Status: Accepted; scope superseded by 0122, layer by 0123
- Date: 2026-11-14

## Context

U88 drew a square at the pointer in the guide ink of ADR 0111 and hid the system arrow only while the
pointer was over the scene. A live session showed three consequences: every tool could take the
pointer back (a contour showed the arrow, an armed primitive the crosshair), the square was painted
inside the viewport on the scene layer and the floating cluster and the product card were drawn over
it, and the one amber outline disappeared on a light board. The owner's rule is simpler: the square
is the window's pointer everywhere, and only working with a menu brings the system arrow back.

The guide ink is derived from the surface of the window, and a light theme's guide is dark amber. The
scene under the pointer is not the window's surface: it can be a white board, a black board or a
shaded face, and no single themed ink reads on all three. A pointer is also not a guide *over* the
model; it is chrome that must stay visible over every surface the window draws.

## Decision

**The work cursor is chrome, not a guide, and it owns the window's pointer.** The frame starts with
the system cursor at `Default`, draws every surface, and then paints the square last, on the
`Tooltip` layer, so no panel, toolbar, popup or dialog can cover it. The square is drawn at the
pointer everywhere; the system arrow comes back only where a `Foreground` layer owns the pointer (a
menu, a popup, a modal) or where a widget asked for an icon of its own (a text field, a view divider,
a scrub). A tool no longer owns a cursor: an armed contour, drawing, primitive or pull keeps the
square.

**Two fixed inks, not one themed ink.** The outline is a 3 pt dark stroke with a 1 pt light stroke on
the same rectangle; the dot and the crosshair use the same pair. At least one stroke reads on any
surface, including the themes that make ADR 0111's amber blend into a light board. The pair lives in
`theme.rs` as `cursor_inks()`, so the colour rule still has one owner; it is fixed rather than derived
because the scene, not the window, is the background.

## Consequences

The pointer cannot be swallowed by a later-drawn surface: the layer order is the guarantee, and a test
can assert it without a GPU. A surface that wants the system arrow must be a `Foreground` layer, which
is what menus, popups and modals already are; a new `Middle` overlay (a toolbar, a cluster) is
automatically under the square and needs no cursor code. ADR 0111 keeps the amber for guides that
belong to the model — an open level's brackets, a work plane — and the work cursor is recorded here as
the exception. The cost is a second colour pair in the theme and a paint that must run after every
other surface, which is one call at the end of the shell's frame.

## References

- `apps/furnicraft/src/work_cursor.rs` — the geometry, the layer and the two-stroke paint.
- `crates/ui/src/theme.rs` — `cursor_inks()`, `CURSOR_DARK`, `CURSOR_LIGHT`.
- `apps/furnicraft/src/main.rs` — the frame resets the cursor and paints the square last.
- ADR [0111](0111-a-guide-is-one-amber-in-screen-space.md) — the guide ink this record carves the
  pointer out of.
- Plan: U88 (the first square), X40 (the correction).
