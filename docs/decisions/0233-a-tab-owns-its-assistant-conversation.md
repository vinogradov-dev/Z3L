# 0233. A tab owns its assistant conversation, and one panel draws the tab in front

- Status: Accepted
- Date: 2026-11-23

## Context

The workspace holds several scenes at once (U45): the session, the undo history, the selection, the
cameras and the render target were moved into the tab payload so that activating a tab swaps the
whole workspace and reads no file. The assistant was not moved with them: `Chat` was one field on
the shell, holding the conversation, the backend, the turn, the edit scope of that turn and the
session id sent to a gateway as `x-opencode-session`.

Measured on this tree before the first edit:

- Two open scenes shared one transcript and one backend. A message sent in one tab answered in the
  other, and the `Chat::scope` that makes one message one undo entry belonged to whichever tab was
  in front, so a batch could land in a document the person was not reading.
- The agent mode already belonged to the session, so the switch survived a tab switch while the
  transcript around it did not — two halves of one surface disagreed about whose they were.
- `Chat::open` was the panel's own flag beside a conversation, so "is the panel up" and "what does
  it say" were one value even though the panel is one window (its rectangle lives in `egui` memory
  under one id) and the conversation is one scene's.

## Decision

- **The conversation is the tab's.** `Chat` lives in `Document`, beside the session and the editor:
  two open scenes hold two conversations, two backends, two turns and two session ids. Nothing of
  one tab's transcript reaches the other, and a turn paused by a switch resumes where it stopped.
- **The window is the workspace's.** Whether the panel is up is `App::assistant_open`; the panel's
  position and size stay the `egui` window's, under one id. The header's cell and `Ctrl+J` write the
  one flag, so the panel never blinks out because a tab that was opened with it closed came forward.
- **Only the tab in front is served.** `App::serve_chat` advances the active tab's turn, so a turn
  in a scene the person is not looking at advances no step behind their back and loses no answer:
  the reply stays in flight in its own backend until its tab comes forward.
- **The workspace's settings are applied to every tab**, not to the front one, because the backend
  is part of the conversation: a scene that comes forward is already able to send. A tab with a turn
  in flight keeps its backend until the turn is over, as it did before.
- **What stays workspace-level is what was never a scene's**: the assistant settings, the consent
  list per endpoint, the key's two homes, the agent process a workshop configured, and the preview
  window.

## Consequences

- One message is one undo entry in the tab it was sent in, because the scope is opened on that tab's
  session and never on a neighbour's.
- Each tab's chat carries its own gateway session id, so a Go gateway's prompt cache is per scene
  and two scenes never share one conversation id.
- An endpoint turn is genuinely paused by a switch: the request is in flight, and the frames that
  drive it are the frames its tab is in front. An agent **process** keeps working while its tab is
  behind, because the process owns its own loop; the calls it makes reach the window's one channel,
  and that channel answers the active document. The batch of a background process therefore lands
  in the front scene — the same rule a foreign client has always had — and routing a call to the tab
  that asked is a change to the channel's own contract (`plan/debt.md`).
- Two tabs that both sent a message run two backends and, for the process kind, two processes. A tab
  that never sent one starts nothing: the process is spawned by the first message, not by the tab.

## Related

- [0029](0029-view-state-belongs-to-the-session.md) — what belongs to a scene and what to the window.
- [0186](0186-the-assistant-is-a-layer-beside-the-ui.md) — `Chat` is the shell's half of the port.
- [0187](0187-a-chat-turn-is-one-undo-entry-and-a-plan-first.md) — the turn, its scope and its stop.
- [0189](0189-the-assistant-speaks-openai-compatible-wire.md) — the endpoint and the consent list.
- [0190](0190-plan-and-build-are-the-persons-switch.md) — the mode is the session's, and so the tab's.
- [0230](0230-an-agent-process-owns-the-model-loop.md) — the process kind whose calls take the
  channel.
