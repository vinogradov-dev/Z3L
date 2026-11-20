# 0170. A report names the part that was in the way

- Status: Accepted; extends what
  [0168](0168-the-shell-writes-one-log-and-the-carry-logs-its-frame.md) writes
- Date: 2026-11-19

## Context

The second logged session ended with a panel clamped at 466 mm while the hand asked for 2000, and the
line said only `contact=Some(Id(3))`. The numbers proved the drag was cut, and proved nothing about
*what* cut it: a reader cannot place `Id(3)`, cannot tell a panel from a carcass's envelope, and cannot
see how big the thing in the way is. The same log's `take` line named a bare `Id(2)`, so the piece was
as anonymous as the obstacle.

The owner's report — "механизм перемещения работает не правильно" — cannot be argued with a
diagnosis that leaves the obstacle unnamed.

## Decision

**A take names the piece.** `take node=… kind=panel|object|product|container|… name="…" pivot=…
box=… cursor=… axes=…`. The kind is the node's own, the name is what the tree shows, and the box is the
world box every later judgement is measured from. The `Node` itself is never printed: it carries the
geometry, and a log line never does.

**A cut names the obstacle.** The carry line gains `blocked=Some(Id(3) Panel (x,y,z)..(x,y,z))`, read
from the same predicate the drag uses, so the report answers which part, of what kind, and where it
stands. The rule exposes `MoveRule::blocked_at`, one function beside `free_at`, so the log cannot
disagree with the drag.

## Consequences

- A report of "it stopped" is a sentence about a named part with a box: the reader checks the box
  against the picture, and either the picture or the rule is wrong — instead of guessing.
- One more call per carried frame in the app. The rule stays pure; nothing is stored.
- The diagnostic says *what* stopped the drag, not *why the piece should have passed*: the judgement
  about the envelope of a product being a reservation rather than a solid remains the owner's call,
  and the log is what makes that question answerable.
