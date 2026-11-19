# 0168. The shell writes one log, and the carry logs its frame

- Status: Accepted
- Date: 2026-11-19

## Context

`log` has been in the workspace stack table since the first milestone ("the shell installs the sink")
and `app` has depended on it, but nothing ever called it: there was no sink, so every `log::` macro in
the tree would have been a no-op. The owner asked for a logging system while reporting a gesture
defect — a carried piece that jerked and jumped instead of travelling — which is a defect a person
cannot describe in words and a debugger cannot easily catch: it happens over a sequence of frames, at
one zoom, under one hand.

## Decision

**One sink, installed by the shell before anything else.** `app/src/logging.rs` is the only
`log::set_logger` in the tree. It writes `+<elapsed>ms <level> <target>: <message>` to
`<config>/z3l.log` and to stderr, appends across runs, starts the file over when it passes four
megabytes, and takes its level from `Z3L_LOG` (`off`, `error`, `warn`, `info`, `debug`, `trace`). The
default is `debug`: the log exists to answer a report, and an unknown word is the default rather than
an error, because a mistyped environment variable must never keep the window from opening.

**The carry logs the frame that a bug report needs.** One `debug` line per carried frame, target
`z3l::move`: the node, its world coordinate (the pivot plus the settled delta), the cursor in points,
the world point the hand meets, the delta the hand asked for, the delta the rule resolved, the delta
the carry settled on, whether the jump hold is active, and the contact and capture of each axis. The
held drag writes its one-axis line with the distance and the snap it landed on; taking a piece,
putting it down and cancelling are `info`, so a session reads as a spine with the frames under it.

**A log line is diagnostic and never a document.** An id, a coordinate, a decision, a sentence. No
panel geometry, no file bytes, no key, and nothing the model holds: the model and the other libraries
do not log, so a test that runs a thousand rules writes no log at all. The sink is a shell concern, as
the stack table says.

**The jump is measured against the hand, not against the wall clock or an absolute step.** The hold
added for ADR 0167 compared the rule's answer with a fixed fifteen millimetres and the pointer's
travel in points. At a zoomed-out view six points of pointer travel is a hundred millimetres, so every
ordinary frame read as a jump: the piece froze and crossed in held bursts — the jerk the owner
reported. The answer is a jump when it moves further than the hand itself travelled, past the same
margin.

## Consequences

- A defect report is a file and one sentence about the gesture; the three deltas say whether the rule,
  a snap or the hold moved the piece.
- Per-frame `debug` is a lot of lines, and that is why the file is bounded, appended and shared by
  stderr: a terminal session and a window session read the same stream, and a report is the file.
- A log left on by default costs a few lines per drag and nothing when idle. A maker who wants quiet
  sets `Z3L_LOG=info`.
- The gesture's numbers are now readable in the log, which is what turned the jerk into a one-line
  explanation instead of a guess.
