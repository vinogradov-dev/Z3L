# 0136. The header draws the wordmark in the text ink

- Status: Accepted
- Date: 2026-11-11

## Context

The header drew the product name as a text label beside an 18-point placeholder mark whose drawing
was a generic board with two shelves. A text label is not a mark: it renders in whatever face the
platform picks, it cannot be sized or aligned to the row, and it repeats in a system font what the
product's own drawing says better. The placeholder drew in the accent, and the accent is the one ink
the interface reserves for action, selection and result (`assets/icons/STYLE_GUIDE.md` §1) — so the
only element on the row that is *not* an action carried the action ink.

The name is now a drawing: `Z3L` in one stroke, three glyphs, one grid, chosen by the owner.

## Decision

- **One asset, monochrome.** `assets/logo.svg` is the wordmark: `viewBox="0 0 42 16"`, one white ink,
  no background, no gradient. It is not an icon and does not live in `assets/icons/`: it is not on the
  24-unit grid, it carries no role ink, and it has no per-theme copy, because the loader tints it with
  the ink the surface asks for.
- **The header draws it in the text ink, never the accent.** The accent is the user's — P1 copper, P2
  magenta, P4 violet (ADR 0063, 0089) — and it marks action; a fixed brand colour in the chrome would
  collide with two of the three presets (ADR 0125 holds the accent away from the role inks for the
  same reason). The orange and turquoise of the brand exist outside the window: the site, the README,
  the release and the application icon.
- **One constant for the ratio.** `icons::LOGO_ASPECT` is the only place 42:16 lives, and
  `icons::wordmark(ui, height, ink)` allocates by height through the same asset cache the icons use.
  `icons::mark` is gone: a square slot for a wide mark can only letterbox it.
- **The name stays reachable.** The mark carries `PRODUCT_NAME` as its hover and accessibility label,
  and the window title still reads the constant, so a screen reader and the task bar both name the
  product even though the row has no text.

## Consequences

- A rename or a redraw touches one asset and one constant; the header carries no glyph of the
  platform's font, so the row looks the same on every system.
- The brand pair cannot enter the chrome. That is a rule, not an oversight: the chrome belongs to the
  theme the user chose, and the brand's job is the surfaces outside it.
- `assets/README.md` now says what this file is and what it is not, because that file is the contract
  a drawing session reads before it adds one.
- The icon gate does not read the mark: `cargo xtask ui-rules` parses Rust for colour literals and
  hosts, and a monochrome asset with one ink has nothing for it to reject.

## References

- `assets/logo.svg` — the wordmark; `crates/ui/src/icons.rs` — `LOGO_ASPECT`, `wordmark`, the cache.
- `app/src/header.rs` — the row that draws it and the test that asserts its aspect.
- ADR [0063](0063-one-theme-owns-the-window.md), [0089](0089-three-palettes-are-data-and-derive-their-states.md),
  [0125](0125-an-accent-keeps-its-hue-distance-from-every-role-ink.md), [0135](0135-the-product-is-z3l.md).
- `assets/README.md` — the icon file format, and the section that excludes a mark from it.
- Plan: U105.
