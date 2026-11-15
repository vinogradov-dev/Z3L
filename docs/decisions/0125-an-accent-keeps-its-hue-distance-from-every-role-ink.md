# 0125. An accent keeps its hue distance from every role ink

- Status: Accepted
- Date: 2026-11-15

## Context

ADR 0090 and U70 gave the icon set seven role inks and said the action accent is chrome and never a
role ink. The rule was checked one way only: WCAG contrast, three to one on the panel and the ribbon.
Contrast measures brightness, and a hue collision can hide at a perfect contrast. Measured on the
presets of U71, the P2 azure accent sat 7.2 ΔE (CIE76) from the model blue in the dark variant and
8.6 in the light one, and the P4 mint accent 7.0 from the scene green — 1.9 in the light variant,
where the two were effectively one colour. An armed tool beside a model or scene glyph therefore
stopped reading as chrome, which is the failure the honesty clause of ADR 0090 was written against.

The role inks are preset-independent by design (a preset changes the neutrals and the accent, never
the meaning), so the collision had to be resolved on the accent side. The owner chose that road on
2026-11-15: keep the seven inks, move the two accents. The measurements, the candidates and the
rejected bands are in `plan/notes/T8.md`.

## Decision

**The accent of every preset is at least 20 ΔE (CIE76) from every role ink, in both variants.**

- The distance is `theme::delta_e`, the plain Euclidean CIELAB distance under D65; its floor is
  `theme::ACCENT_HUE_DISTANCE`. CIE76 is coarse enough for a floor of 20 and needs no colour
  appearance model in the tree.
- The accent also keeps the rule it already had: 3 : 1 on the panel, the ribbon and the surface,
  and the theme foreground on the accent fill, never a role ink.
- The role inks keep their own separation: the closest pair, `model`/`modify`, stays at 23.5 ΔE dark
  and 20.1 light, and a test walks every pair so a future ink cannot quietly land on its neighbour.
- A preset is named after its accent, so the two renamed looks carry the new words and the new
  stored names: P2 `ink_magenta` ("Ink · magenta") and P4 `slate_violet` ("Slate · violet").
  `ink_azure` and `slate_mint` are the same looks under their old names: the settings reader
  resolves them and rewrites the name, so a workspace opens on the look it chose and the next save
  carries the current word. An unknown name is still repaired to P1 and reported.
- `icons::tests::the_accent_clears_every_role_ink_by_hue` is the proof; it fails on the presets this
  record replaces.

## Consequences

- An armed tool, an active tab and a selection read as chrome beside every glyph, in every preset
  and both variants.
- The role inks stay a closed, preset-independent table; adding one now obliges a measurement
  against the accent and against the six others, not only a contrast check.
- The hue wheel is nearly full: cyan, blue, green, yellow, violet, grey and red are taken, so a new
  preset has only the magenta and orange bands to choose from, and the orange band is squeezed
  between the copper accent and the danger red (measured in `plan/notes/T8.md`).
- Two stored names joined the settings vocabulary. The pre-1.0 freedom of ADR 0096 covers the
  document format; a workspace file is the user's own, so an old name keeps loading rather than
  costing them their recent files and viewport defaults.
- `plan/notes/U70.md`, `plan/notes/U71.md`, `assets/README.md` and this record carry the rule.

## Related

- [ADR 0090](0090-icons-are-coloured-by-semantic-family.md) — the role inks and the one-ink fallback
  this record keeps.
- [ADR 0124](0124-the-icon-language-is-v2.md) — the seventh role (`reference`) the accent is
  measured against.
- [ADR 0089](0089-three-palettes-are-data-and-derive-their-states.md) — the presets, their ramps and
  the accent override.
- [`plan/notes/T8.md`](../../plan/notes/T8.md) — the measurements and the candidates.
