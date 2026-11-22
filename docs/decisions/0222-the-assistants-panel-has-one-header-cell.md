# 0222. The assistant's panel has one header cell

- Status: Accepted; extends the place ADR 0209 left to the key
- Date: 2026-11-22

## Context

Owner report 2026-11-21: there is no plan/build switch in the interface, and the owner could not find
one. Measured: the switch exists and is the panel's — two segments beside the sentence they stand for
(ADR 0190) — but the panel was drawn by `Ctrl+J` alone, because no menu, ribbon cell or palette row
named `assistant.open`, and `Ctrl+J` is not a place a person looks for a panel. ADR 0209 recorded
exactly this and said what to do when the glyph arrives: "the header gains one cell and this record's
second bullet is what it must keep". The glyph could not be drawn in code (`AGENTS.md` §4.12), so the
cell waited for `assets/icons/actions/assistant.svg`.

## Decision

- **The header's right cluster gains one cell, `actions/assistant`**: a speech bubble with two lines,
  one `view` ink, a service glyph on the 24 grid and 18 pt inside a header control.
- **It stands beside the settings gear, at the row's far edge.** The gear and the assistant are the
  two surfaces that open over the scene and close without touching it, so they are one group; the
  ribbon fold, the two panel switches and the transform band follow inwards, and the row still reads
  right to left from the window's edge.
- **The cell is a toggle and its selected state is the panel being up**, like the two panel switches
  beside it. Its hint is the panel's word and the inventory's binding, so a remapped key prints
  correctly here without a second edit (ADR 0117, U96).
- **The key stays and is unchanged.** The cell and `Ctrl+J` are one act — `Chat::toggle` — so a panel
  that opens from two doors cannot close from one.
- **A plan answer says where the switch is.** The batch report's `mode_note` names the person's
  switch, the panel that holds it and the key that opens it, and it is carried only by a plan answer:
  `mode: plan` alone leaves a caller to take the dry run for a tool it has not learned and ask
  harder, and only the person can change the mode (ADR 0190).

## Consequences

- A person looking for the agent finds it where the window's own surfaces are; `Ctrl+J` becomes the
  faster door rather than the only one.
- The right cluster grows by one control and the tab row gives it the width, so a narrow window elides
  tab titles a little earlier. The cluster's measured width (`cluster_width`) is what keeps the band
  and the switches whole.
- No id is added: the inventory's `ACTIONS` row and `KEYS` row for `assistant.open` already existed,
  and the cell draws that action. `cargo xtask ui-rules` therefore reads the same two rows it read
  before.
- `mcp` sits beside `ui` and may not read the key table (ADR 0186), so the wire's sentence carries its
  own spelling of `Ctrl+J`; a test in `app` — the one crate that sees both — asserts the two agree.
- `assets/icons/STYLE_GUIDE.md` §19.3 gains the glyph's metaphor and §19.6 its file name, and
  `assets/README.md` its row, as §4.12 requires.

## Related

- [0209](0209-the-assistant-panel-is-a-window-and-its-turn-runs-between-frames.md) — the panel this
  record gives a place; its second bullet (the panel owns no document and dispatches nothing) holds.
- [0190](0190-plan-and-build-are-the-person-s-switch.md) — the switch the cell makes findable.
- [0117](0117-the-interface-is-one-inventory-of-capabilities.md) — the row, the place and the key the cell reads.
- `AGENTS.md` §4.12, `.pi/skills/place-capability/SKILL.md`, `.pi/skills/draw-icon/SKILL.md`.
