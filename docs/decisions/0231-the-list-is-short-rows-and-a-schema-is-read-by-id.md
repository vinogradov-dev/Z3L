# 0231. The list is short rows, and a schema is read one id at a time

- Status: Accepted; amends the `command_list` answer 0185 states and the "the surface is the
  dictionary" consequence of 0227
- Date: 2026-11-23

## Context

ADR 0185 made `command_list` the discovery surface and gave every row the command's parameters as a
JSON Schema, derived from the typed struct its `run` parses. The schema is built by a function, so a
surprise could not creep in — but the list asked for all of them at once.

Measured 2026-11-21 (J15): one answer was **189 989 bytes**, about 40 151 tokens, of which 171 789
were schemas, 18 062 the domain guide of ADR 0215 and 4 816 the short rows. A caller whose context is
the cost does not read such an answer, it slices it — and a slice reads as a fact. One session saw
`sphere` and `cone` in `PrimitiveShape`, stopped at the end of the slice and concluded that the
registry had no box at all; `object.extrude.create` sat a few thousand bytes further on.

Measured 2026-11-23: the largest single schema is `panel.create` at 13 244 bytes, a fifth of what the
old list cost, and the short rows are 73 bytes a command.

## Decision

- **The list is one short row per command**: the stable id, the English label, the world it serves
  (the `family` word of ADR 0215) and what it creates. No schema, no verdict.
- **One command's schema is answered for the id that is named.** `CommandListRequest` carries
  `command: Option<String>`; with it the answer is that one row carrying `params`, and no guide. It
  is the same value `command_run` parses — `CommandSpec::params_schema`, from the same struct — so
  the two cannot drift, which is the whole of 0185's reason for the derive.
- **The list carries nothing but the domain.** `domain` (the guide) and one sentence saying how a
  schema is asked for; the guide is read once with the ids and not again per schema.
- **The answer has a published byte budget.** `mcp::LIST_BUDGET_BYTES` is 32 KiB and a test holds
  the list under it: the promise is a caller's context, and a number in a test is the only way a
  promise survives the next twenty commands.
- **No verdict per row, and the session's own refusal once.** ADR 0226 retired the `enabled` column
  because a command writes wherever its parameters are valid; the rows keep that rule. What a
  session cannot do here is the session's, so a plan-mode session answers `mode_note` with the
  sentence a batch and `undo` are refused with, and a build session answers nothing.
- **An id this build does not dispatch is a refusal naming the ids that exist**, because a caller
  that asked for one id is about to ask for another.

## Consequences

- A caller makes two calls where it made one: the list, then the schema of the command it is about
  to dispatch. The measured cost of that second call is one schema, not sixty-six.
- The guide is now most of the answer (18 062 of 23 278 bytes), so its growth is the next thing that
  meets the budget. The `## Where we are` of a later session reads that as the guide's problem and
  not the rows'.
- `z3l commands --json` is untouched: it is a person's and a recipe author's report, it has no
  context to spend, and it still prints every schema.
- A discovery surface for a *model* is now sized like one, and the CLI keeps the version a person
  can hold in an editor.

## Revisit if

A caller is seen spending its second call badly — asking for the same schema twice in one turn, or
reading only the list and guessing the parameters. The first has a cache or a batch-ask as its answer;
the second would say that the list needs one line of each schema rather than none.
