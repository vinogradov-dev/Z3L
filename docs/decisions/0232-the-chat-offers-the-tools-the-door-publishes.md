# 0232. The chat offers the tools the door publishes, and the history keeps every call

- Status: Accepted; supersedes the "no read tool" ruling of J14 and refines 0185, 0189 and 0190
- Date: 2026-11-23

## Context

The panel's model reached the registry through two tools: `command_batch` and `todo_write`. The
system message carried the domain guide and every command id with its label, and no parameter
schema, because J14 ruled `command_list` and `query` out as "a second dispatch path inside the
backend".

Measured against the owner's own endpoint (`deepseek-v4.1-flash` on a Go gateway, 2026-11-23), the
same message and the same prompt as the panel:

- **A schema-blind model guesses.** For «построй куб 100 200 900» the first answer called
  `object.cuboid.create` with `{parent, x, y, z, placement}`; the command wants
  `parent, name, width, depth, height, placement` and refuses with `unknown field x`. A cabinet
  recipe took **six refusals out of eight answers**, every one of them a guessed field name or a
  guessed type (`class: "cabinet"`, `class: 1011`, `panel.create` with `depth`, then `x`, then
  `thickness`). Each refusal took the whole batch back, and the work started again from nothing.
- **The answering model's `content` is `null`.** This model speaks only through tool calls. The wire
  replayed `content` alone, so the assistant turn vanished from the history: the next request held
  the person's message and a pile of "the program reports: …" lines, with no record of what the
  model had asked for. It planned the same build again — measured: the third answer re-sent the box
  that had just landed.
- **A turn ended only on a reply with no call at all.** A model that always calls a tool therefore
  ran to `MAX_CALLS` (12 requests) and stopped on the budget, which is what a person saw as "thinking
  for a long time" and no result. The opposite case was silent: a reply carrying only `todo_write`
  ended the turn with no note at all.
- **The other door did not have this problem.** The same model over MCP reads
  `command_list --command panel.create` first, and the cabinet was built there in one batch of
  thirty-nine steps.

## Decision

- **The chat offers the three reads the tool server publishes.** `command_list` (every row, or one
  command's schema), `query` (the tree, a box, measured bounds, the parts table, the diagnostics, the
  catalogue's materials, a niche) and `inspect` (the findings). The schemas are the tool server's own
  request types, so a chat model and a foreign client read one schema and one answer (ADR 0185,
  ADR 0231).
- **The turn answers a read; the backend does not.** `mcp::reads::MessageDoor` holds the session and
  the message's edit scope: `Session::agent_batch_in` for a batch and `listing`, `asking` and
  `findings` for a read — the same functions `CommandServer` calls. No second projection, no second
  path into the document.
- **The history carries what the model did.** `Conversation` keeps the wire's messages beside the
  lines the panel draws: one assistant message per answer with its `tool_calls`, and one `tool`
  message per call carrying the answer, tied by the wire's id. A read that came before a batch is
  the schema that build used; a batch that came before a read is what a verification read sees.
- **A turn ends on an answer that names no call**, which is the model saying it is done. A batch
  that landed no longer ends the turn by itself, and a reply with only a checklist no longer ends it
  silently.
- **The same batch is never run twice in one turn.** A verbatim repeat is answered with the sentence
  that says so — to the model and to the person — because a box that landed once is not a second
  part (X130).
- **An unreadable call is an answer, not the end of a turn.** A call whose arguments cannot be read
  is carried into the history with the parser's sentence as its result, so the model corrects one
  key in the same turn instead of the turn dying on a transport complaint.
- **The catalogue is a question.** `query` gained `materials`: every record the session's snapshot
  carries, with the id a command names a board or a band by, its kind, the thickness it comes in, the
  sheet it is bought in and the width a band covers. A model with no file system could not name a
  material before, and `panel.create` requires one (X131).

## Consequences

- A chat model now does what a model over MCP does: read the schema, build in one batch, read what
  landed, speak. The two doors differ in transport only.
- The panel draws a read as one dim `Tool` line — the tool's name and its arguments, shortened — so
  a person watches the work instead of a spinner, and the Stop button has something to stop.
- Reads do not spend the reply budget; a reply that makes three reads is still one request. Refusals
  still count, because a refusal is a result the model must act on.
- The panel's `Todo`-style checklist is unchanged: it is chat state and never undoable (ADR 0190).
- A model that repeats itself is answered, not obeyed, so a duplicated part can no longer land in
  one turn. A duplicate across two messages is still the person's to undo.

## Related

- [0185](0185-the-tool-registry-is-one-table-and-every-door-reads-it.md) — one registry, coarse tools.
- [0187](0187-a-chat-turn-is-one-undo-entry-and-a-plan-first.md) — the turn, its scope and its stop.
- [0189](0189-the-assistant-speaks-openai-compatible-wire.md) — the endpoint adapter this refines.
- [0190](0190-plan-and-build-are-the-persons-switch.md) — the mode stays the person's; the tools are
  the same in both.
- [0230](0230-an-agent-process-owns-the-model-loop.md) — a process already had the whole surface;
  this is the wire catching up.
- [0231](0231-the-list-is-short-rows-and-a-schema-is-read-by-id.md) — why the schema is read by id.
- `AGENTS.md` §4.11.
