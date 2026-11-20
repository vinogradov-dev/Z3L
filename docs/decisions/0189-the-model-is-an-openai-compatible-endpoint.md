# 0189. The model is an OpenAI-compatible endpoint, and the key is not in the settings file

- Status: Accepted
- Date: 2026-11-21

## Context

ADR 0028 leaves the provider open — "the language model is an injected adapter that may be local or
remote" — and requires that keys live in the OS keyring or a restricted configuration file, never in
the document, the repository or a log. Measured on this tree: no HTTP client, no TLS stack and no key
handling exist anywhere; `app/src/settings_store.rs` writes one `settings.json` in the platform's
settings directory, and that file is the kind of thing people copy between machines and sync through a
cloud folder.

The owner's constraint is concrete: the assistant must be usable with a cloud provider they already
pay for **and** with a server on their own machine (Ollama, LM Studio, vLLM, llama.cpp), and the
fifteen-line difference between those is a base URL and a model name. One adapter must serve both, or
the tree grows a provider matrix.

## Decision

- **One wire: the OpenAI-compatible `/v1/chat/completions` with tool calling.** `base_url` and `model`
  are data, never a type, and no provider name appears in a type, a match arm or a default. A local
  server is the same client pointed at `http://127.0.0.1:<port>/v1`.
- **DeepSeek is the documented example** (`https://api.deepseek.com/v1`, `deepseek-chat`) because it is
  the one the owner uses; it is an example in the docs and in the settings template, not a constant in
  the code. A test asserts that changing `base_url` and `model` alone reaches a different endpoint.
- **Anthropic and Gemini native protocols are not implemented now.** They arrive through an
  OpenAI-compatible gateway, or as another adapter behind the same port when a second real caller
  exists. Building two protocol adapters for one user is the speculative abstraction §8 forbids.
- **The key comes from the environment, else from a restricted file.** `Z3L_AGENT_KEY` first; else
  `agent-key` in the settings directory with owner-only permissions (`0600`); else the assistant
  reports "not configured". **The key never enters `settings.json`**, the scene file, a log line, a
  chat message, an error sentence or the audit sidecar; the keyring is a later improvement, not a
  blocker.
- **Disabled by default, with one explicit consent.** Before the first request the user sees what
  leaves the machine — the messages, the tool results and the scene digest — names the endpoint, and
  confirms once. The confirmation is remembered per endpoint, and clearing it re-asks.
- **The client is blocking and small**: `ureq`, in the chat crate only (ADR 0186). No `reqwest`, no
  second async runtime; a blocking call is moved off the UI thread by the crate that owns the backend
  and never awaited by `engine` or `ui`.
- **TLS comes from the operating system, not from C sources we compile.** This is the one place the
  owner's first sketch does not survive contact with the tree: `rustls` is pure Rust, but its crypto
  providers are not — `ring` compiles C and assembly and `aws-lc-rs` compiles C, which `AGENTS.md` §2
  forbids outright. The gate's ban list already names `openssl-sys` because it can build OpenSSL from
  vendored sources. So the transport decision is the OS TLS (`native-tls`: SChannel on Windows,
  SecureTransport on macOS, the system OpenSSL on Linux), and enabling it requires **one counted
  exception**: `openssl-sys` is allowed when, and only when, the resolved feature set does not carry
  `vendored`, proven by a test over the resolved dependency graph rather than by a comment. A pure
  Rust stack with a maintained, non-C provider is the alternative and is not one today.

## Consequences

- A user with a local model installs nothing extra and sends nothing off the machine; a user with a
  cloud key changes two settings.
- The tool-calling shape is the provider's, not ours: a model that cannot call tools cannot use the
  assistant, and a provider that spells tool calls differently is reached through a gateway.
- The key has two homes and an order, so "why is it not configured" has one answer a person can read.
- The consent screen is the only place the product says what data leaves the machine; it must stay
  readable and must name the endpoint, not "the cloud".
- The `openssl-sys` exception is a rule change in the gate, so it lands with a test and a note in
  `plan/debt.md` naming what would remove it (a maintained pure-Rust provider).
- A provider outage is a sentence in the panel and nothing else: no engine path awaits it, and the
  editor behaves as if the feature were off (ADR 0028).

## Related

- [0028](0028-the-assistant-is-a-client-of-the-command-registry.md) — the model is an injected
  adapter; this record decides its wire and its key.
- [0186](0186-the-mcp-crate-sits-beside-the-ui.md) — the crate that holds the transport.
- [0188](0188-a-local-control-channel-is-not-a-network-service.md) — the inbound side; this is the
  outbound one.
- `AGENTS.md` §2 and §4.8 (no C sources, dependency budget); `xtask/src/main.rs` (`C_VENDORING_CRATES`).
