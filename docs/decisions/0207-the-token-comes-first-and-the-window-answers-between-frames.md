# 0207. The token comes first, and the window answers between frames

- Status: Accepted; fills in the handshake, the mailbox and `ui_inspect` of 0188 — the `busy`
  answer a gesture gave a call is superseded by [0223](0223-a-call-waits-for-the-frame-that-has-the-window.md)
- Date: 2026-11-21

## Context

ADR 0188 allowed a local control channel and fixed the three bounds — loopback only, a token minted
per run, a refusal before dispatch — but left four things open that a client we do not ship will
meet as a contract: how a connection presents its token, how a tool call reaches the document a
person is looking at without freezing the window, what a caller may learn about the window itself,
and where the flag lives.

Measured on this tree: `crates/mcp` serves one session behind a mutex, and a window does not have
one session — `app` keeps a `Session` per tab and swaps the active one. The frame already takes
work at its boundary (`take_directory`, `take_pick`), which is where ADR 0187's "a turn runs at a
frame boundary" belongs. `rmcp` serves any reader/writer pair, so the same framing that carries
stdio carries a socket.

## Decision

- **First line is the token, then `ok`, then the protocol.** A connection writes the hex token and a
  newline; the server answers `ok` or `refused` and closes. The line is read a byte at a time under
  a cap and a deadline, so the protocol's frames are never over-read, no buffer is sized by a peer,
  and the compare folds every byte. A refused connection never queues a call, which is what "refused
  before it is read as a command" means in code.
- **A call is a mailbox entry, not a lock.** The channel owns the socket and a queue; a tool call
  becomes a `Call` and waits for the host's answer. The window drains that queue at the top of its
  frame and answers against the active tab, so the document is written once, by one thread, between
  frames — never inside a drag, an armed tool or an undo (ADR 0187).
- **Two gestures make the answer `busy`.** A piece carried by the pointer and a question waiting for
  the person both outlive a frame, and both mean "not now" rather than "move the document".
- **`ui_inspect` is the shell's report, not a second read model.** It carries the open scenes (title,
  path, dirty, revision, open product), the active index, the armed gesture, the selection, the busy
  flag and the status sentence. The document stays `query`'s; this is only what a headless session
  cannot tell a client.
- **A batch in the window is not written to disk, and says so.** Its report carries `write: window`:
  the tab is dirty on purpose, the path may not exist yet, and the person's *Save* is the write.
- **The flag is a mode of `mcp serve`.** The window offers the channel by default and the Assistant
  page closes it; `z3l mcp serve --attach` serves the same channel from a process with no window,
  and stops when its standard input closes — the lifecycle a client that spawns it already has.
- **The token file is `channel.json` in the settings folder, mode `0600`,** published before the
  thread starts and removed on a clean exit. A live file whose port answers makes the next instance
  refuse without touching it: two windows are the ordinary case, and the first channel is the one a
  client reaches.

## Consequences

- A client needs no discovery beyond one file, and the wire is one line of prose to implement.
- A tool call is only as fast as the next frame, which is the price of one writer and the reason a
  client never waits inside a gesture: a 120 s deadline answers "the window did not answer" instead
  of hanging.
- The window writes nothing, so an agent's work is in the document and not in the file until the
  person saves; a client that needs the file calls no tool, because none of them writes one.
- A headless `--attach` lists `ui_inspect` and refuses it, because one tool list describes both
  doors; the sentence says that this process has no window rather than hiding the tool.
- The token is a secret: it never reaches a log line, and `Channel`'s own `Debug` prints the port and
  the file only.
- If a future client needs HTTP rather than this framing, the handshake and the mailbox stay; only
  the transport in front of them changes.

## Related

- [0188](0188-a-local-control-channel-is-not-a-network-service.md) — the permission and the bounds.
- [0187](0187-a-chat-turn-is-one-undo-entry-and-a-plan-first.md) — the frame boundary and the one
  undo entry a turn is.
- [0185](0185-the-tool-registry-is-one-table-and-every-door-reads-it.md),
  [0186](0186-the-mcp-crate-sits-beside-the-ui.md) — the tools and the crate that serves them.
- `crates/mcp/src/channel.rs`, `app/src/main.rs` (`serve_channel`).
