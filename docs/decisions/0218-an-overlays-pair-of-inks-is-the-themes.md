# 0218. An overlay's pair of inks is the theme's, and its quiet half is a neutral

- Status: Accepted
- Date: 2026-11-22

## Context

The contour edit draws the shape it started from behind the shape it is making, and the owner could
not read the change: the two were a near-white grey at 63 % and a pale blue, two high values a hue
apart, and the grips and the renderer's own selection cyan sat in the same family on the same shape
(X97). Both inks were hand-picked constants in `app/src/contour_gizmo.rs`, and the colour rule of
ADR 0063 read `crates/ui` only — one line exempted the whole shell, so nothing caught them.

An overlay is not chrome. A chrome ink is measured against a panel it always sits on; an ink drawn
over the model lies on a material of any colour, and it is lifted off the workspace before use so it
reads at both ends (ADR 0121, 0123). A single ink still lands on a material of its own colour: the
default preset's copper accent is 1.05:1 against a beech face and 1.9:1 against a dark edge band.

## Decision

- **Four named inks**, each a function of `crates/ui/src/theme.rs`:
  `theme::original` — the dim text ink folded 35 % into the workspace, the quiet half;
  `theme::working_copy` — the action accent lifted 25 % off the workspace, the loud half;
  `theme::casing` — the workspace's own ink, drawn wider under the working copy;
  and the grip, which takes `theme::text`.
- **The pair is two signals.** The inks differ in value by at least `ORIGINAL_VALUE_GAP` and in
  colour by at least `ACCENT_HUE_DISTANCE`, and the shape that started the edit is drawn thinner and
  dashed, so the roles read where a material flattens the difference (WCAG 1.4.1).
- **The quiet one is a neutral.** It carries no tint, so it is never read as a dimmer copy of the
  working copy and never as a tint of the material; `ORIGINAL_CONTRAST` is its floor on the
  workspace, under the 3:1 a meaningful graphic gets, because it is context and not an action.
- **The loud one is cased.** A hairline of the surface behind the model bounds it, so a material of
  the ink's own colour cannot swallow it, and the casing is the variant's own workspace rather than
  a black or white constant — the rule the work cursor and the selected chain already follow.
- **The rule reads the shell.** `cargo xtask ui-rules` checks `app/src` for colour literals like
  `crates/ui`, and `SHELL_INK_DEBT` names the modules that still owe their inks. A module leaves the
  list when its inks become theme functions; the list never grows.

## Consequences

The contour overlay is the first paid: it draws no colour literal, and its two inks, its casing and
its dashing are one rule a test pins on both variants of all three presets and a shot renders over a
bright beech face and a dark band in both. The rest of the shell — the box tool's size label, the
drawing draft, the axis and ring inks — still owes its inks, and `plan/debt.md` keeps the list under
`[gizmo]`; until then those eight modules are the only exemption, and a ninth cannot appear without
the gate failing.

The cost is that a new overlay must name its inks in the theme instead of picking them where it is
drawn, and that an ink over the model is chosen by two rules at once — where it sits in the palette
and what it is drawn over.

Revisit this if the palette ever owns a cool ink of its own: the working copy would then separate
from a warm material by hue as well as by its casing.
