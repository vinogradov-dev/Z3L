# 0187. A chat turn is one undo entry, and the first turn is a plan

- Status: Accepted
- Date: 2026-11-21

## Context

The assistant's control surface is settled by ADR 0185 (one registry, coarse tools, a read-only gate in
the dispatcher) and its home by ADR 0186 (a shell panel over a `mcp` port). What is left is the part a
person actually feels: what one message does to their document, what they can take back, and what they
see while the agent works. The editor's own precedent is strict — a gizmo drag is one preview and one
command (ADR 0020), a multi-selection move is one undo entry (ADR 0033) — and a chat that left ten
undo entries per request would be the one place where the document moves in a way a person cannot
retrace.

Two failure modes are worth designing against before any code exists. A model that calls a command
per attribute turns "make the carcass 600 wide" into twenty entries and a history nobody can read; a
model that stops mid-batch leaves the document half built. ADR 0183 already answers the second for a
recipe — all or nothing, one edit scope — because a half-applied recipe is a scene nobody asked for.

## Decision

- **One message is one undo entry, named after the message.** Every command a turn dispatches goes
  through one `EditScope`, so `undo` takes the whole answer back in one step and the history reads as
  the conversation. A turn that dispatches nothing leaves no entry.
- **A turn runs at a frame boundary, never inside a gesture.** The panel dispatches through the
  shell's ordinary path between frames, so an assistant call cannot interleave with a drag, an armed
  tool or an undo the person is performing: one writer, one order.
- **The first enable is read-only.** The session starts with the mutating commands gated (ADR 0185),
  the agent answers with a plan, and a person turns editing on for that session. Read-only is the
  state the feature is born in, per ADR 0028.
- **A plan is the `dry_run` result, shown as a checklist**, not as prose the model wrote from
  imagination: the same dispatch runs with writing off, and what the person approves is what ran.
- **The stop is between calls.** Cancellation is checked at every call boundary, so a cancelled turn
  leaves the document exactly as the last completed batch left it — never a command cut in half,
  never a second undo entry.
- **After a person's own edit, the agent is told what changed.** The next turn receives a note — the
  nodes added, removed and moved since the turn's own last write — and the nodes the last turn created
  stay highlighted until the selection moves. Both are view state (ADR 0029); neither is document
  data.
- **Undo is the person's, and the model is told.** A turn that runs after an undone answer works on
  the document it finds, and the note names what disappeared.
- **A turn has a budget**: a maximum of calls, a maximum size of a result handed back and a deadline,
  all of them settings. A refusal counts as a result, because the command's own sentence is what the
  model can act on.
- **A conversation is not document data.** Messages, keys and the audit trail stay out of the scene
  file and out of the undo stack (ADR 0028); the product name in any text the UI shows comes from one
  constant (ADR 0135), and every UI string comes from `text::Message`.
- **Voice and images are not prerequisites.** A text turn is the whole v1; a screenshot channel and
  speech are separate decisions, and neither blocks control.

## Consequences

- The history stays legible: one answer, one step, one name. A person who dislikes what the agent did
  presses undo once.
- A model that wants many small calls pays for it in tool calls, not in undo entries.
- Approval is cheap and honest: the checklist a person sees is the dispatch that was actually
  attempted, so the plan cannot be more optimistic than the commands.
- Cancelling is predictable but coarse: a turn cannot stop inside a batch, which is the same price
  ADR 0183 accepts for one undo entry.
- The note after a person's edit is a derived summary, not a diff of the file, so it can say "three
  panels added" and never "byte 41 233 changed"; a model that needs the exact state calls `query`.
- Highlighting what a turn created needs a distinction the current selection does not carry, so a
  visible-marking atom is part of the panel's own work, not of the engine.

## Related

- [0185](0185-the-tool-registry-is-one-table-and-every-door-reads-it.md),
  [0186](0186-the-mcp-crate-sits-beside-the-ui.md) — the registry and its home.
- [0183](0183-the-command-line-is-the-one-headless-door.md) — one edit scope, all or nothing, and the
  `dry_run` that this record shows to a person.
- [0020](0020-the-translate-gizmo-previews-and-commits-one-placement.md),
  [0033](0033-a-multi-selection-is-placed-by-one-command.md) — the preview-then-one-command precedent.
- `AGENTS.md` §4.11; `PROJECT.md` §Assistant.
