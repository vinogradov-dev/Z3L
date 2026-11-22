# 0226. A command names the piece it writes in, and the interface keeps it open

- Status: Accepted; amends the command-vs-button clause of 0098 and the `enabled` column of 0185
- Date: 2026-11-22

## Context

ADR 0098 put the "one open product" guard in the command: `panel.create` and every furniture
creation refused to run unless the target parent sat inside the session's open piece, and the
session asked that after a command ran on a copy. The record's words were "the guard lives in the
command, not in the button", and its reason was that a palette, a shortcut or a future assistant
meets the same check.

Measured 2026-11-22 (N37, X112, and the owner's answer to it): a build that a program performs has no
open piece at all. `product.create` writes a frame closed (ADR 0098), so an agent that had just made
the piece could not put the first shelf in it, and a caller that named the product it meant was
refused with «сначала создайте изделие» although the document held everything the command needed. The
owner's rule is the other one: **nothing prevents a program from writing a panel into a closed frame,
or with no frame at all — what matters is that the part is not an orphan and that it sits in the
piece's group.** A person still builds only inside the open piece, because that is how a person
chooses the piece they are working on; a program is told which piece the task means.

The guard also made `command_list` print `enabled: false` and a reason for a command the dispatch
would run the moment the piece was opened — or, after this change, would run anyway. A discovery
surface that says "no" about a request the engine accepts is worse than no column at all.

## Decision

- **A command's `run` is the document's gate, and the document has no "open".** A command validates
  its parameters against the tree — the parent is a container, the class may live there, the
  material is in the catalogue — and writes there. The open product is session view state (ADR 0029)
  and is not an argument of `run`, so it cannot be a condition of writing.
- **The open product is the interface's rule.** `CommandSpec::enabled` keeps asking
  `allow_creation`, and so does the editor's own creation path (`create_panel_sized`): a button, a
  palette entry or a ribbon cell is offered only inside the open piece, and a person's flow is
  unchanged. The refusal is the interface's sentence from the message table
  (`CreationNeedsProduct`, `CreationProductClosed`).
- **`Creation` describes, it does not gate.** It keeps the class level of the node a command makes —
  the `creates` word `command_list` prints — and loses the "parameter that names the parent" and the
  session-level `unavailable` answer. `CommandError::CreationRefused` goes with them.
- **`command_list` carries no verdict.** The row is the id, the English label, what it creates and the
  parameter schema; whether a request may run is decided by the request, whose refusal is the
  command's own sentence.
- **The interface names the piece where a caller cannot point.** `panel.fill` is the one command
  behind the three fill cells (N37): the point, the class and the mode in, the measured part out, the
  product named by the caller. A caller that is not sure which piece the task means reads
  `open_product` from the answer and asks the person to open it; it does not guess.

## Consequences

- A part can be written into a closed piece, and a caller that names no product can write one at the
  root. Nothing in the document says which is right: an orphan is a report's business, not a gate's,
  and the tree already shows the parent of every node.
- `z3l run` and the tool surface no longer need a product to be opened before a build works; `--product`
  and the `product` field still open it, because a person reads the answer's `open_product` to know
  where the work is.
- The document guard for "the class may not sit in this host" (ADR 0094) is untouched: it lives in
  the model, on the placement, and not in the session.
- A test that asserted the dispatch refusal now asserts the pair: `enabled` refuses outside the open
  piece, and the dispatch writes where the parameters name. Three surfaces changed together —
  `engine`, `ui`, `mcp` — because the rule had been stated in all three.
- Revisit if a caller is ever seen scattering parts: the answer is a diagnostic and a report sentence,
  not a return to a session condition in the write path.

## Related

- [0098](0098-one-product-is-open-and-parts-are-born-inside-it.md) — the open-piece rule, whose guard
  moves to the interface here.
- [0029](0029-view-state-belongs-to-the-session.md) — why openness cannot be a document condition.
- [0094](0094-a-class-registry-gives-the-machine-its-semantics.md) — the host rule that stays in the
  model.
- [0185](0185-the-tool-registry-is-one-table-and-every-door-reads-it.md) and
  [0215](0215-the-domain-is-taught-and-a-construction-is-composed.md) — the discovery surface whose
  `enabled` column is retired.
