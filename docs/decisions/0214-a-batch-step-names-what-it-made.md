# 0214. A batch step names what it made, and a later step refers to the name

- Status: Accepted; the hole ADR 0198 leaves a recipe with
- Date: 2026-11-21

## Context

A batch promised one call and one undo entry (ADR 0183, ADR 0198), and a step carried a command id
and the parameters its own parser takes. Nothing in that let a later step name a node an earlier one
created: the id exists only after the step runs, so `command_batch` could not band the panel its
first step made. Measured while building the wardrobe recipe: 14 creations, 19 bandings and 5 bodies
became three batches and three undo entries, and the recipe itself has to write the ids of one fixed
scene (`3`, `4`, …) into its lines, which breaks the moment a document already holds anything.

The alternative shapes are all worse than the sentence they buy. A caller may guess an id — the
document allocates from a counter, so a guess is a wrong node or a refusal. A step may return the id
and the caller may come back — that is the second call the promise forbids. A placeholder may be
stored in the document and resolved on save — that puts a batch concept into the model and into a
file format that has no past before `1.0` (ADR 0096).

## Decision

- **A step names what it creates.** `BatchStep::new(..).named("side")`, `"as": "side"` on a tool
  step, `"as":"side"` on a recipe line. The name denotes the node the command reported as created
  (`CommandOutcome::created_id`); a step that creates no node may be named and cannot be referred to.
- **A reference is `{"$ref": "<name>"}` in place of an id.** One key, a non-empty string, no other
  member: an object that carries `$ref` is a reference or a mistake, never an ordinary parameter, so
  a misspelling is a refusal instead of a map handed to a command that wanted an id.
- **Resolved as the batch runs, on the copy.** The command receives the parameters the caller wrote
  with the reference replaced by the document id, so a command, a door and a test see no new
  concept, and nothing a batch writes holds a placeholder — a saved scene holds ids.
- **A reference may only name a step before it.** A name no earlier step gave, a name a step gave
  to a node it did not create, and a name a second step takes are three readable refusals, each in
  the command's own `invalid parameters` sentence and naming the step at fault. A batch already
  stops at its first refusal (ADR 0198), so a reference to a step that refused cannot arise: the
  loop never reaches it.
- **The name is the batch's, not the document's.** Nothing is serialized, a name is claimed once and
  only for the length of the call, and two batches of one chat turn do not see each other's names
  (ADR 0187 keeps the undo entry across them, not the vocabulary).
- **The answer echoes what the caller wrote.** A step's row carries `as` when it named itself and
  the parameters as sent, so a reader maps a reference to the id through the row of the step that
  created it, and the report still says what the document now holds by id.

## Consequences

- A build is one call and one undo entry, so the recipe's own lines no longer carry the ids of one
  fixed scene and a part can be built in a document that already holds something.
- The tools' step schema grows one optional key, and the recipe's line grows the same one; a client
  that never names a step reads an unchanged contract.
- Resolution walks the parameters of every step of a batch, including one that names nothing: a
  reference the batch cannot resolve is exactly the case where no name exists yet, and skipping the
  walk on that condition would hand the raw map to the command. The walk is bounded at
  thirty-two levels of nesting, and a batch is a turn, not a frame.
- The reference names a node. A step that creates only a machining record (a cutout, a groove) can
  be named and cannot be referred to; when a command needs to address one, that is a new decision —
  the id space is shared, so the mechanism is there and the vocabulary is not.

## Related

- [0198](0198-a-batch-is-planned-on-a-copy-and-lands-in-one-piece.md) — the batch this fills in, and
  the "it stops at its first refusal" that makes a dangling reference unreachable from the far side.
- [0183](0183-the-command-line-is-the-one-headless-door.md) — the recipe whose lines stop carrying ids.
- [0212](0212-a-batch-names-the-product-it-works-in.md) — the other field a batch grew, and the same
  argument: the batch is the unit a caller works in.
- [0096](0096-the-file-format-has-no-past-before-the-first-release.md) — why a placeholder never
  reaches the document.
- `AGENTS.md` §4.1, §4.2; `plan/notes/J18.md`.
