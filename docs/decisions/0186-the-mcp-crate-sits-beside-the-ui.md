# 0186. The MCP crate sits beside the UI, and the chat panel lives in the shell

- Status: Accepted; narrows 0028's layering
- Date: 2026-11-21

## Context

ADR 0028 put the assistant core between `engine` (L6) and `ui` (L7), moved `ui`, the shell and
`testkit` up one number, and warned that the crates and the layer map change in the same commit that
creates them. The crate would own the tool registry, the permission policy, the task runner and a
`ChatModel` port, and it would have no HTTP client, no async runtime, no `egui` and no `wgpu`.

Two things measured since then change the shape. First, the registry is the engine's
(ADR 0185), so the crate holds a protocol and a model transport, not a brain. Second, the surface that
needs the network is a chat panel, and the shell already owns surfaces that are not document widgets:
`app/src/header.rs`, `status_bar.rs`, `side_panels.rs` and a whole second application in
`directory_app.rs`. The layer map today is `engine` 6, `ui` 7, `app` 8 with no room between, and a
crate that must serve both `ui` and the shell cannot be placed at the same number as `ui`: a crate may
depend only on lower layers.

An async runtime under `ui` would also be a rule to bend: `ui` never sees `wgpu`, `engine` has no
runtime, and `cargo xtask layers` enforces both.

## Decision

- **A new crate `mcp` at layer 7, beside `ui`.** It depends on `engine`, `model`, `units` and `text`;
  it is the only crate allowed to depend on `rmcp`, `tokio` and `cap-std`, and it is forbidden
  `egui`, `eframe`, `wgpu` the way `engine` is. The two layer-7 crates do not depend on each other.
- **The layer map does not move.** There is no `agent` crate, `ui` stays at 7 and `app` at 8. The
  reason ADR 0028 gave for the move — "the chat panel needs agent state" — does not apply to a panel
  that the shell draws and the shell wires.
- **The chat panel is a shell surface**: `app/src/chat/`, drawn with `ui`'s atoms, theme and words
  like every other chrome surface, and wired there, because the backend handle, its thread and its
  cancellation belong to the composition root and not to a document widget. `cargo xtask ui-rules`
  gains the row that names the surface it owns.
- **One port, three implementations, all in `mcp`.** `ChatBackend` is what the panel talks to; a
  scripted backend runs a whole turn without a network (the acceptance ADR 0028 demands), an
  HTTP one reaches a model API, and a process one speaks to an agent that attaches over MCP. `ui`
  never sees the port.
- **The core stays synchronous.** `tokio` runs inside `mcp` only; the panel drains a channel once per
  frame and never blocks the UI thread on a future. A model call that hangs is a spinner, not a frozen
  window.
- **`schemars` lives in `engine`**, where the parameter structs live (ADR 0185). No other new
  dependency enters a core crate: the transport crates stay in `mcp`.
- **Nothing listens.** ADR 0028's "the app never listens" stands. The editor serves MCP over the stdio
  of a process it spawns, and `z3l mcp serve` over its own stdio. A loopback bridge to an already open
  window is the one shape that would break the clause; it is not part of this decision, and it is a
  question for the owner rather than a task.
- **Crate, layer map, `xtask`, `PROJECT.md` and `AGENTS.md` §3 change in one commit**, as ADR 0028 and
  §3 require. Until that commit the record is the only trace.

## Consequences

- No layer is renumbered, so four worktrees do not have to merge a new number into their own tables
  for a crate that does not exist yet.
- `mcp` is the only asynchronous place in the tree: a failing provider degrades to "the assistant is
  unavailable", and no editor path can await it.
- The chat cannot be reused by another GUI without `app`, which is the right price: the panel is
  chrome, and chrome is the shell's.
- A surface that the shell owns needs its `HOST_OWNERS` row and its inventory entries when it lands,
  so the interface stays one inventory of capabilities (ADR 0117) rather than a second one.
- `mcp` may not depend on `ui`, so a chat message cannot use a `ui` widget type. The port carries
  plain data — role, text, a tool call and its result, a refusal — and the panel renders it.
- The dependency budget grows by three crates in one leaf: `rmcp` and `tokio` for the protocol, and
  `cap-std` when a sandboxed path tool exists. Each needs its one sentence of justification in the
  commit that adds it, and none of them compiles C sources.

## Related

- [0028](0028-the-assistant-is-a-client-of-the-command-registry.md) — the assistant is a client; this
  record replaces its `agent` crate and its layer move.
- [0185](0185-the-tool-registry-is-one-table-and-every-door-reads-it.md) — the registry, the schema
  and the stdio transport.
- [0187](0187-a-chat-turn-is-one-undo-entry-and-a-plan-first.md) — what a turn does to the document.
- `AGENTS.md` §3 (crate map and layering), `xtask/src/main.rs` (the layer table).
