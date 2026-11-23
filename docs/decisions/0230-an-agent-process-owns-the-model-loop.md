# 0230. An agent process owns the model loop, and the bridge is what it holds

- Status: Accepted
- Date: 2026-11-23

## Context

ADR 0186 left the chat panel with one port and named three implementations: a scripted backend, an
OpenAI-compatible endpoint (ADR 0189, J14) and "a process one speaks to an agent that attaches over
MCP". ADR 0188 confirmed that the same channel carries that second kind of client. Only the scripted
one exists, so a person who already runs an agent — the case the owner described as "the process owns
the model loop, the editor owns the document and the tools" — cannot use it at all.

ACP is the protocol for that arrangement: the editor is the *client*, the agent is a process it
starts, and the process decides everything the model decides. Measured against the protocol as it is
published today (v1, `agentclientprotocol.com`):

- The transport is the process's own standard streams, one JSON-RPC frame per line. Streamable HTTP
  is an RFD and not part of v1.
- `session/new` carries the MCP servers the agent should connect to, and a **stdio** server is the one
  transport every agent must support; HTTP and SSE are optional agent capabilities.
- The agent reports its work through `session/update` notifications — `agent_message_chunk`,
  `tool_call`, `tool_call_update`, `plan` — and ends a turn with the `session/prompt` response and a
  stop reason. `session/cancel` is a notification the client sends.
- The agent may call back for `fs/read_text_file`, `fs/write_text_file`, `terminal/*` and
  `session/request_permission`, and the client's `initialize` says which of those it offers.

Measured on this tree before the first edit: the window's channel (`mcp::channel`) speaks MCP over a
loopback socket behind a `token\n` handshake, so it is not an MCP endpoint any agent could name as
`{type: "http"}`; `ChatBackend` had no way to report a tool call, only a batch of our own commands;
`mcp::dispatch` opened its own undo entry per batch, so a process making three calls in one message
would leave three entries; and `AssistantSettings` had the channel switch and nothing else.

## Decision

- **A fourth use of the same door, not a new one.** The agent process is handed one MCP server:
  `z3l mcp bridge`, which reads the port and token the window published in the settings folder,
  presents the token, and copies bytes between its standard streams and the socket. The tools, the
  plan gate and the undo history stay the window's; the bridge speaks no protocol of its own, so it
  can grant nothing, refuse nothing and read nothing. The token never enters the process's arguments
  or environment — the file ADR 0188 publishes is the one way in.
- **`crates/mcp/src/acp.rs` is the backend**, and the process is behind a `Spawner` port: `Program`
  starts a real command, and a test hands in a scripted peer on two operating-system pipes. The
  window never waits: two threads own the streams, `ask` posts an event and `take` reads what arrived.
- **The conversation is the protocol's own messages.** Chunks are accumulated into one line per
  paragraph, `tool_call` is a `Speaker::Tool` line, `tool_call_update` with `status: failed` is a
  `Note` carrying the sentence the window answered with, and `plan` becomes the checklist. `Reply`
  gained the lines and a `more` flag: a piece of an answer is drawn at once and keeps its request in
  flight, so a person watches a process work instead of a spinner.
- **The person's stop travels.** `ChatBackend::cancel` is new and the turn calls it on the frame the
  stop is taken; for ACP it is `session/cancel`, and the process answers with its own stop reason.
- **No file and no terminal.** `initialize` advertises neither `fs` nor `terminal`, and a process that
  asks anyway is refused with a sentence. `session/request_permission` is answered with the first
  option that allows, because the client's real gate is the window's own: the person's plan switch is
  asked of every mutating call at the door (ADR 0190), so a permission question cannot grant more.
- **One message is one undo entry, across the channel.** `Door::Window` gained the turn's edit scope:
  a batch that arrives while a turn is open lands inside it, so a process that edits in five calls
  leaves one entry. The scope is the shell's answer and not a request field — no caller can name one.
- **The command is settings, and empty by default.** `AssistantSettings.agent` holds one line: the
  program first, then its arguments. The panel starts the process once per configured command and
  never per frame, installs nothing while the channel is switched off, and says so in the panel's own
  words instead of letting a process talk into a document it cannot reach.
- **A process that dies is a turn that stops, not a document half-written.** A batch is all or nothing
  in the window, so what the process did before it died is exactly what the last completed batch left,
  and the turn's entry closes with the turn.

## Consequences

- The assistant has a second real backend without a network, a key or a paid account, which is what
  the owner's "use an agent you already run" asks for.
- A process is a new surface on the same token: it can call every tool the channel carries, and the
  only thing bounding it is the plan switch and the tools' own refusals. That is the same bound the
  person's panel has, which is the point, but it is worth saying out loud.
- A window without a channel offers a configured process no tools. The panel names the switch instead
  of failing silently, because that is the support question ADR 0188 predicted.
- `mcp` now starts processes, so it depends on `std::process` and on two threads of its own; the
  layer map, the dependency list and the forbidden-crate rules are unchanged.
- The bridge takes no arguments, which means one window per settings folder is one bridge target —
  the same limit the channel already had.
- Streaming is a promise the port now keeps for one backend; the OpenAI-compatible one may use the
  same `more` flag or answer once, and the panel needs no change either way.

## Related

- [0188](0188-a-local-control-channel-is-not-a-network-service.md) — the channel, the token and the
  sentence that the same channel carries a second kind of client.
- [0186](0186-the-mcp-crate-sits-beside-the-ui.md) — the port, the crate and the three backends.
- [0187](0187-a-chat-turn-is-one-undo-entry-and-a-plan-first.md) — the turn, the entry and the stop.
- ADR 0190 (the two modes), ADR 0193 (the acceptance scenarios the next task asserts).
