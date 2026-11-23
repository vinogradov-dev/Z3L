# 0188. A local control channel is not a network service

- Status: Accepted; clarifies the listening clause of 0028
- Date: 2026-11-21

## Context

ADR 0028 says "the app never listens", and `AGENTS.md` §7 bans a web server. Read literally the first
sentence forbids the thing the owner now wants: an agent in the next process that builds in the window
a person is looking at, and an assistant in a browser-free desktop client that attaches to the same
window. Both are processes the same person started on the same machine; neither is reachable from a
network. The ban §7 states is about a web stack and about network reachability — a second way into a
document over HTTP, a browser target, a remote caller — not about IPC between the program's own parts.

Measured on this tree: no `TcpListener`, no HTTP client, no async runtime, no socket of any kind
exists; the only assistant surface is the headless command line (ADR 0183). `AGENTS.md` never uses the
word "listens"; the sentence lives in one record and, since ADR 0187's commit, in one paragraph of
`PROJECT.md`. Both reference products the owner studied keep a loopback channel with a per-run token,
which is the shape that makes the phrase and the feature compatible.

## Decision

- **A local control channel is allowed; a network service is not.** The channel binds `127.0.0.1` on
  an ephemeral port, and a Unix socket or a named pipe may replace it later without changing this
  record. Nothing is bound to a routable address, nothing is advertised, and a request that does not
  carry the run token is refused before it is read as a command.
- **The token is 256 bits from the operating system's CSPRNG, minted once per run**, and it goes with
  the port into one file in the settings directory, readable by the owner only (`0600`), removed on a
  clean exit. A stale file names a port that answers nothing, and a reader must treat a refused
  connection as "no window is open".
- **The channel carries the tool surface and nothing else**: the tools of ADR 0185 — commands, batches,
  queries, and in this mode `ui_inspect` and `ui_screenshot` — plus the handshake. It is not a file
  server, not a shell, and not a way to reach a path: a path tool exists only inside a declared
  sandbox root (ADR 0185).
- **`stdout` stays the protocol.** Diagnostics go to `stderr`, as the log sink already does (ADR 0183),
  so a headless server and a bridged window are both machine-readable.
- **Two modes of one server, and the headless one comes first.** `z3l mcp serve [<scene>]` speaks
  stdio and binds nothing; `--attach` starts the local channel described here. The headless mode ships
  first because it needs no window, no token and no lifecycle work.
- **It is on by default and switchable off** in the settings. A feature the owner has to discover a
  flag for is a feature that does not exist, and the exposure is one machine, one user and one token
  that dies with the process.
- **`PROJECT.md` and the index of ADR 0028 say what this means**: "no network service; a local
  control channel is how an agent attaches to a running window". The record itself is not rewritten
  because decisions are superseded or clarified, never edited in place.
- **The same channel carries a second kind of client.** ACP — an agent process that owns the model
  loop — travels the identical endpoint behind the same token, which is why the chat's backend trait
  has room for it (ADR 0186). A second backend must not be a second door.

## Consequences

- A model can now drive the document a person is looking at, and the person watches it happen — which
  is the whole point of the assistant, and impossible through stdio alone: a stdio child edits a file,
  not the open session.
- A listener is a new attack surface, and it is bounded by three things that must all hold: loopback
  only, a token that is not written to any log, and a refusal before dispatch. Each of the three gets
  a test, and the token file's permissions get one too.
- A second running instance cannot steal the channel: the port is ephemeral and the token file belongs
  to the instance that minted it.
- The settings switch and the token file are the two places a support question will land ("my agent
  cannot see the window"), so both are named in the panel's own help text.
- `--attach` needs the shell's lifecycle: the channel starts with the window, dies with it, and
  answers "busy" rather than blocking on a document a person is mid-drag in. A turn still dispatches
  at a frame boundary (ADR 0187).
- The clause this record reads narrowly is the product's strongest security sentence. If it is ever
  meant to forbid a listener absolutely, this record is the thing to supersede — and the feature goes
  with it.

## Related

- [0028](0028-the-assistant-is-a-client-of-the-command-registry.md) — "the app never listens",
  clarified here as "no network service".
- [0185](0185-the-tool-registry-is-one-table-and-every-door-reads-it.md),
  [0186](0186-the-mcp-crate-sits-beside-the-ui.md) — the tools the channel carries and the crate that
  serves them.
- [0187](0187-a-chat-turn-is-one-undo-entry-and-a-plan-first.md) — a turn's undo entry and its
  cancellation, which the channel does not change.
- [0189](0189-the-model-is-an-openai-compatible-endpoint.md) — the outbound side of the same feature.
