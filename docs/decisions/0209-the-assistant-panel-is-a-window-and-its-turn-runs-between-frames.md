# 0209. The assistant's panel is a window, and its turn is driven between frames

- Status: Accepted
- Date: 2026-11-21

## Context

ADR 0186 puts the assistant's panel in the shell and its port in `mcp`; ADR 0187 says a message is
one undo entry and a turn runs at a frame boundary; ADR 0190 makes plan and build the person's
switch. None of it named the surface itself, and the tree had no place for a conversation: the
window's surfaces are the header, the ribbon, two floating side panels, the view cluster, the tool
card, the status bar, the palette, Settings, Home and the product-frame card.

Measured before the first line: `engine::Session` had no mode, `mcp` had no chat module, and the
only doors an agent had were a process and a loopback channel (J5, J6). A chat could not be a third
door with its own dispatch path — that is the second writer ADR 0187 forbids — and it could not be
drawn in `crates/ui` either, because `ui` may not depend on `mcp`.

## Decision

- **The panel is a `Window`, drawn by `app/src/chat.rs`, opened by `Ctrl+J`.** Not a third floating
  side panel: the outline, the inspector and the scene are what a person works in, and a conversation
  that took a column of the viewport would narrow the cabinet. Settings and Home are the precedent —
  a surface that opens over the scene and closes without touching it — and one key plus one row in
  the inventory's `ACTIONS` table is the whole place it appears in until an icon is drawn for it.
- **The panel owns no document and dispatches nothing.** It draws the turn's conversation, its
  checklist and the two modes, and returns one `ChatAction`. `App::serve_chat` drives the turn at the
  frame boundary, one backend reply and one batch per frame, so a person's Stop lands between two
  calls and a dispatch never interleaves with a drag (ADR 0187).
- **A message is one edit scope.** The shell mints one `EditScope` per turn and every batch of that
  turn goes through `Session::agent_batch_in`, so `undo` takes the whole message back in one entry
  and a message that changed nothing leaves none.
- **The mode lives on the session, and the agent doors read it.** `AgentMode` is per-session view
  state (ADR 0029): `plan` turns every agent batch into the dry run the command line already had,
  and the person's own field and gizmo are not gated by it. A headless session is created in `build`
  (the process that started a tool server chose that, and its plan is `--dry-run`); the window's tab
  is created in `plan`, so the first answer may only propose.
- **The backend is a port, and the scripted one comes first.** `ChatBackend` is `ask`/`take` rather
  than one blocking call, because the HTTP adapter of ADR 0189 must wait on its own thread while the
  frame keeps drawing. `Scripted` answers from a fixed list, which is what lets a whole turn — the
  reply, the batch, the undo entry and the panel's note — run headlessly.
- **No model is connected, and the panel says so.** The network adapter, its consent screen and the
  key's two homes (ADR 0189) are one follow-up task, not a half-built client: with no backend the
  field is disabled and the panel names the state in a sentence instead of failing on the first send.

## Consequences

- The assistant is one more surface over the scene, not a change to the workspace; closing it costs
  nothing and its conversation dies with the tab.
- Two undo entries can never come from one message, and a stopped turn leaves the document exactly
  as the last completed batch left it.
- Until the network adapter lands, a window shows the panel and no model: the feature is honest about
  what it does not have, and a test drives the whole turn through the scripted backend.
- `Ctrl+J` is the panel's only place in the chrome. When an icon is drawn for it (ADR 0122, §4.12),
  the header gains one cell and this record's second bullet is what it must keep.

## Related

- [0187](0187-a-chat-turn-is-one-undo-entry-and-a-plan-first.md) — the turn this record places.
- [0190](0190-plan-and-build-are-the-persons-switch.md) — the mode the switch sets.
- [0189](0189-the-model-is-an-openai-compatible-endpoint.md) — the adapter that is still to come.
- [0186](0186-the-mcp-crate-sits-beside-the-ui.md), [0028](0028-the-assistant-is-a-client-of-the-command-registry.md).
- `AGENTS.md` §4.11; `.pi/skills/ui-design/references/surfaces.md`.
