# 0190. Plan and build are the person's switch

- Status: Accepted; refines the read-only first turn of 0187
- Date: 2026-11-21

## Context

ADR 0187 said the assistant starts read-only and a person turns editing on. The owner names the two
states and the rule that keeps them honest: **plan** and **build**, and the switch belongs to the
person, never to the model. The obvious implementation — hide the mutating tools from the list a model
sees — does not work here, because the tool surface is coarse by design (ADR 0185): with one
`command_run` tool there is nothing to filter, and a model that is merely *not told* about a capability
is one prompt away from asking for it by id.

Measured: `Session` has no read-only mode today (no `reading` flag, no capability check in
`Session::dispatch`); the CLI's `--dry-run` dispatches everything and writes nothing (ADR 0183), which
is the mechanism a plan needs and the one a plan should reuse rather than re-implement.

## Decision

- **Two modes, `plan` and `build`, held by the session** and shown in the panel. `plan` refuses every
  command that mutates the document **at the dispatcher**, with the command's own sentence plus what
  the refusal means, and `build` runs it. The mode lives with the session's other view state
  (ADR 0029), never in the document, and it is not a settings value that survives a restart into
  `build` without a person's act.
- **The model has no tool that changes the mode.** The switch is a control a person operates; a tool
  that could flip it would make the whole mode theatre. The tool list is the same in both modes.
- **`plan` answers with the `dry_run` result.** The dispatcher runs the same commands with writing off
  and reports what would be created, changed and removed, plus the refusals. A plan that cannot be
  dispatched is a plan that is wrong, and the person sees that before anything moves.
- **A checklist is chat state, not document data.** The turn's `todo_write` items are shown beside the
  conversation, keep their order, and vanish with the tab. They are never undoable, never serialized
  into the scene and never part of an audit record of the document.
- **Leaving `build` cancels a running turn** at the next call boundary (ADR 0187). Leaving `plan` for
  `build` is one action a person performs and one line in the panel that says the next answer may
  change the document.
- **The first enable lands in `plan`**, per ADR 0187 and ADR 0028. `build` is the second act, and the
  panel says which mode it is in where the person types.

## Consequences

- A refusal is now a normal, readable answer instead of a missing capability, and a model that is
  denied learns why in words it can act on.
- The dispatcher needs a capability check before the `enabled` check, so the refusal order is
  defined: read-only first, availability second, validation third. That order is what the tests
  assert.
- `dry_run` becomes a first-class operation of the session, not a flag of the command line: the CLI
  and the MCP server call the same code, which is how the two answers stay identical.
- Two modes mean two visible states of one panel, and the interface must make them distinguishable
  without a tooltip — a furniture maker should see at a glance whether the agent may touch the
  document.
- A person who leaves `build` on overnight has left an agent able to edit their work; the mode is
  per-tab session state and a fresh tab starts in `plan`.

## Related

- [0187](0187-a-chat-turn-is-one-undo-entry-and-a-plan-first.md) — the turn, its undo entry and its
  cancellation; this record names its two modes.
- [0185](0185-the-tool-registry-is-one-table-and-every-door-reads-it.md) — coarse tools, and why a
  list filter is not the gate.
- [0183](0183-the-command-line-is-the-one-headless-door.md) — the `dry_run` a plan reuses.
- `AGENTS.md` §4.1, §4.11.
