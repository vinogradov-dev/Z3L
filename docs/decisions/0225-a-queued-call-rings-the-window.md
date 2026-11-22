# 0225. A queued call rings the window, and the frame that lands it asks for the next one

- Status: Accepted; completes the repainting clause 0223 left to X111
- Date: 2026-11-22

## Context

ADR 0207 made a tool call wait for the window's frame boundary, and ADR 0223 taught the window to
ask for a frame while a call waited in its queue and to drop a call whose caller had gone. Both
records stopped at the same wall: the window paints on input, so a call arriving while the person
was not touching it produced **no frame at all**, and the request that would have answered it waited
for a pointer event only a person could bring. ADR 0223 wrote its own boundary down as a consequence
("a window that never paints (X111) still times out") and left this task the rest.

Measured 2026-11-21 (X111, and again by the timings X107 filed): a `command_batch` over the channel
created its node, moved the revision and left the tab dirty, and nothing appeared until the pointer
entered the window. A dry run took 83 s and 53 s and a landing call 196 s, while the queries — read
in the window's own idle frame — answered in about a second. The document had the change; the frame
the person saw did not.

Two facts decide this. The queue is filled by the channel's thread and drained by the window's frame,
and only the second one can act — so the fill has to wake the drain, or the two never meet without a
person. And `mcp` is forbidden `egui` (ADR 0186): the wake cannot be a `Context`, so it is a closure
the host hands in and the channel calls.

## Decision

- **A queued call rings the host.** `Channel` keeps one bell, the socket rings it after every call it
  queues, and the host installs the wake once, where it opens the channel — a window passes
  `Context::request_repaint`, which `eframe` turns into a wake of the `winit` loop. This is the same
  shape ADR 0182 gave the file-manager dialog: a worker finishes off the frame path and asks the
  window for a frame, so an idle window costs nothing.
- **A call that arrived before the host watched rings when it starts watching.** The port is
  published before the wake is installed, and a client can connect in that gap; the mailbox
  remembers the call and rings on installation instead of leaving it to the next pointer.
- **The frame that answers a call asks for the frame after it.** A dispatch that landed, and a call
  that is held for a gesture, both ask `egui` for the next frame: the answer leaves the window inside
  the frame and before its paint, so the frame that names it must not be the last one. A read that
  moved nothing asks for none.
- **The request path names the frame.** The shell counts its frames and logs which one answered a
  call, so a person reading `z3l.log` can tell the frame that landed a change from the frame that
  showed the old one.
- **The headless door waits on the same bell, not on a clock.** `z3l mcp serve --attach` has no
  frame and no pointer: its loop sleeps until a call rings it and looks at its own stop flag every
  50 ms, which is the second that a closed standard input is noticed.
- **The host's seam is testable without a socket.** `Mailbox::post` queues a call the way the socket
  does, ringing the bell, so a frame test drives the real mailbox and the real wake with no client,
  no port and no runtime.

## Consequences

- A call from an agent is answered by the next frame whether or not a person is at the window, which
  is what makes the channel usable from another process at all: the 196-second landing call is a
  frame, not a pointer.
- The window's frame is asked for from another thread, so the shell's `Context` is cloned into the
  mailbox. `mcp` still knows no `egui`, and a door with no window — the headless one — installs a
  channel sender instead.
- A window that is asleep with a call waiting is now impossible in principle, not by luck: the only
  way into the queue is the ring beside it.
- An agent's call still costs one frame of latency, and a frame asks for one more after landing it,
  so a landed batch draws twice. The second frame is what paints the settled state the answer
  describes, and a still window costs nothing.
- A turn of the assistant panel drives itself from the same boundary but does not yet ask for its
  own frames; a turn therefore advances with the pointer, which is a gap this record names rather
  than fixes (debt).

## Related

- [0223](0223-a-call-waits-for-the-frame-that-has-the-window.md) — the queue, the held call and the
  deadline this record completes.
- [0207](0207-the-token-comes-first-and-the-window-answers-between-frames.md) — the handshake and
  the mailbox.
- [0182](0182-the-file-manager-chooses-the-path.md) — the worker that wakes the window, the same
  shape one door earlier.
- [0186](0186-the-mcp-crate-sits-beside-the-ui.md) — why `mcp` installs a closure and not a
  `Context`.
- `crates/mcp/src/channel.rs` (`Bell`, `Mailbox::watch`, `Mailbox::post`),
  `app/src/main.rs` (`attach_channel`, `serve_channel`, `frames`),
  `app/src/cli/served.rs` (`IDLE_WAIT`).
