# 0122. The custom pointer belongs to the work area

- Status: Accepted; layer clarified by 0123
- Date: 2026-11-14

## Context

X40 made the two-tone square the window's pointer on every surface that was not a menu. The owner
corrected the rule: the custom pointer belongs to the work area. On a panel, a widget, a text field,
a floating toolbar and a resizing border the system cursor is the affordance — the I-beam says a
field takes text, the resize arrow says a border moves, the arrow says a control is a control. A
square that replaces those icons hides what the surface is for. The owner's earlier sentence
("everything but menus") was wrong, and the session did not ask what "everything" meant for system
elements; the gate stayed green because X40's own `Done` asserted the square over a panel and the
floating cluster.

## Decision

**The square is drawn only inside the scene rectangle, and only over the scene layer.** The shell
computes the square when all four hold: the pointer is inside `Document::scene_rect`; the top
interactable layer at the pointer is `Order::Background`; the corner orientation widget is not under
the pointer; and no widget asked for a cursor of its own. The square and the crosshair are clipped
to the scene rectangle, so a pointer near a panel never spills the custom paint onto it.

Every other surface keeps the system cursor: the header, the status bar, both side panels, the
ribbon and its method list, the floating cluster, the command bar, the distance fields, every popup
and modal, every text field, and a split-view border (the resize arrows). The two-tone outline of
ADR 0121 stands; only its scope changes.

## Consequences

The shell needs the scene rectangle and the corner widget's rectangle, because the widget is drawn
on the scene layer and the layer test cannot see it. A new floating surface is a `Middle` area and
needs no cursor code; a new widget drawn directly on the scene layer must register its rectangle the
same way, or it will get the square. The rule is testable without a GPU: the surface under the
pointer decides the icon, and the scene rectangle decides whether the square exists.

## References

- `app/src/work_cursor.rs` — `show`, `over_system_surface`, the scene clip.
- `app/src/main.rs` — `Shell::orientation_rect`, `draw_work_cursor`.
- `app/src/document.rs` — the scene rectangle and the corner widget's placement.
- ADR [0121](0121-the-pointer-is-a-two-tone-square-above-the-chrome.md) — the outline and layer this
  record keeps, with its window-wide scope removed.
- Plan: U88 (the first square), X40 (the wrong scope), X41 (the correction).
