# 0234. A batch moves the document's revision once, and the answer says how many entries it left

- Status: Accepted
- Date: 2026-11-23

## Context

ADR 0183 makes a batch one undo entry: the steps run on a copy of the document and the copy is
installed only when the batch may land, so one undo takes the whole list back. The revision did not
follow that rule. Every command advances the revision by one — `Project::bump_revision` is the
command's own write — so a batch of thirty-nine steps moved the number by fifty-three in the
acceptance run over the channel, counting the pieces the steps made along the way.

Measured before the first edit: `Session::land` installed the copy as it came, with the revision the
steps had accumulated, and `crates/mcp/src/report.rs` reported that number. A caller who watches the
revision to know whether the document changed could not tell one operation from one command, and the
answer could not say how many entries a batch left, because the report knew nothing of the history.

## Decision

- **One batch is one revision.** `Project::settle_revision(before)` makes the document's revision the
  successor of the one the batch started from, whatever the steps inside did. The number stays
  monotonic: no state between the steps was ever published, so nothing that cached a revision ever
  saw a higher one, and a cache that rebuilds on a change rebuilds once.
- **The answer says the entries it left.** `BatchReport::entries` is one for a batch that landed as
  an operation of its own, none for a batch that merged into an open edit — a chat turn keeps its
  whole message in one entry (ADR 0187) — or for a batch that wrote nothing. `command_batch` answers
  it beside the revision, so the count of operations is observable from outside the process.
- **A refused batch still moves nothing.** The copy is not installed, so neither the revision nor
  the history changes; only the report is returned (ADR 0198).

## Consequences

A batch is one operation in every observable number: one undo entry, one revision and one entry
reported. A caller can poll the revision and rebuild once per operation instead of once per step.

The cost is one write to the document after a batch lands: the trial's accumulated revision is
replaced by the operation's own. A step inside a batch can no longer be identified from outside by
its revision, which was never a promise — the step reports its own outcome, and a caller that needs
to tell steps apart reads `steps` in the answer.

Revisit if a caller ever needs per-step revisions for a progress bar inside one operation; that
would be a change in the report, not a return to the accumulating counter.
