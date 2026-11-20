# 0185. The tool registry is one table, and the CLI and the MCP server are its doors

- Status: Accepted; narrows 0028's tool registry and 0183's "one headless door"
- Date: 2026-11-21

## Context

ADR 0028 planned an assistant core, `agent`, with a tool registry of its own: a permission policy, a
task runner and a `ChatModel` port. ADR 0183 then measured that the registry already exists —
`CommandRegistry::specs()` plus the session's queries — and that a headless recipe already reaches the
document through `Session::dispatch_in` inside one edit scope. A second registry built for a model
would be a second truth about what the program can do, and the two would drift the day a command
gained a parameter.

What the model needs from the wire is a tool list it can call, with the parameters of each tool and
the reason a tool is unavailable; what a client we do not ship (`pi`, a desktop assistant, a future
ACP agent) needs is a protocol it already speaks. Measured on this tree: no `mcp`, `chat` or `agent`
crate exists; `rmcp`, `tokio` and `schemars` are in no manifest; `z3l commands --json` prints an id, a
label and what a command creates, and cannot describe a command's parameters, which is the gap
`plan/debt.md` carries. A command's parameters are 51 typed `serde` structs with
`deny_unknown_fields`; nothing enumerates their fields.

The goal is stated by the owner and is not a detail: a person talks to the agent **inside the
application window**, and a client outside it may also connect, over MCP. Both must see one registry.

## Decision

- **One table, projected twice.** The MCP tools are a projection of `CommandRegistry::specs()` and the
  session's read-only queries, never a copy. The tools are coarse and few — `command_list`,
  `command_run`, `command_batch`, `query` — because a model that can call one command at a time spends
  its budget on transport, and because the coarse tools are the ones whose semantics we can state and
  test. Two tools per command id would be 51 × 2 names for no gain.
- **`command_list` says everything the caller needs to call a command**: the stable id, the English
  label, what it creates (the class level ADR 0098 gates), whether it is currently `enabled` **and the
  sentence that says why it is not**, and its parameters as a JSON schema. `enabled` with a reason is
  what makes a refusal actionable instead of mysterious.
- **A command declares its parameters from its own type.** `schemars` derives `JsonSchema` on the
  typed parameter structs, and `CommandSpec` exposes the schema next to the id and the label. The
  struct stays the single source of truth for validation, the UI's hints, the documentation and the
  model's tool schema. A test asserts that every command whose parameters are not the unit type
  yields a schema whose `required` set matches its struct, so a hand-written table cannot creep back
  in. This closes the gap ADR 0183 recorded in `plan/debt.md`.
- **`command_batch` is ADR 0183's recipe, not a second engine**: one session, one `EditScope`, one
  undo entry, all or nothing, and `dry_run` dispatches everything and writes nothing. `command_run`
  is the same call with one item.
- **Read-only first is a gate in the dispatcher, not a shorter tool list.** A plan phase refuses every
  command that mutates the document and answers with the `dry_run` plan plus a checklist, so a model
  sees the refusal of the thing it asked for instead of a list that silently lacks it.
- **Units on the wire are millimetres, degrees and intrinsic XYZ Euler angles**, converted in the
  adapter through `units`, never inside a command: the document keeps micrometres, millidegrees and a
  quaternion (ADR 0003, ADR 0024). A world coordinate keeps the `_mm` key ADR 0183 gave it.
- **No shell, no filesystem browser, no arbitrary path.** A path tool arrives only with a task that
  needs one, and then under `--read-root`/`--write-root`. `render_preview` arrives when an offscreen
  render exists; a GPU is not a prerequisite for control.
- **The wire is stdio. The application does not listen.** A client we do not ship starts
  `z3l mcp serve [<scene>]`, which dispatches through the same session code as `z3l run` and writes
  the scene the way `run` does. A client we do not ship is not a reason to open a port
  (ADR 0028, `AGENTS.md` §4.11). A bridge to an already open window would break that clause and is a
  separate decision nobody has taken.
- **The crate is `mcp`; there is no `agent` crate.** The name says what the crate is: a protocol
  adapter over the engine, with no brain of its own. The model stays behind a port (ADR 0028); the
  chat is one of its clients (ADR 0186, ADR 0187).

## Consequences

- A model, a shell script and the person at the window call the same ids and get the same refusals. A
  capability the agent has is a command the UI can show, and the other way round.
- Adding a command with parameters now costs one derive on its struct, and the tool schema updates
  itself. The alternative — a schema table written by hand — is what this record exists to prevent.
- `schemars` enters `engine`. It is a derive over `serde` types with no I/O, no runtime and no C
  sources, and it is the only new dependency a core crate takes for this feature.
- `rmcp` and `tokio` enter the tree, in `mcp` alone (ADR 0186). `engine`, `ui` and the shell stay
  synchronous.
- `commands --json` and the MCP tool list can no longer disagree about what a command takes, because
  both read `CommandSpec`.
- A refusal is a first-class result: a disabled command, a read-only session and a failed step all
  come back with the command's own sentence, which is what a model can act on.
- Parameter schemas describe shape, not meaning: a length in millimetres and a count of shelves are
  both `integer`, and only the command's refusal or its label says which is which. A model still has
  to read the label.

## Related

- [0028](0028-the-assistant-is-a-client-of-the-command-registry.md) — the assistant is a client, and
  its registry was to be `agent`'s; this record puts the registry in the commands and the protocol in
  `mcp`.
- [0183](0183-the-command-line-is-the-one-headless-door.md) — the recipe, the one edit scope and the
  `dry_run` semantics this record reuses; its "one headless door" becomes "one vocabulary, two
  surfaces".
- [0186](0186-the-mcp-crate-sits-beside-the-ui.md) — where the crate goes and what it may depend on.
- `AGENTS.md` §4.1, §4.11; `PROJECT.md` §Assistant.
