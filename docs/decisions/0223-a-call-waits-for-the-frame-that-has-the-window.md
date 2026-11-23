# 0223. A call waits for the frame that has the window, and a caller that has gone is not performed

- Status: Accepted; amends the `busy` answer of 0207
- Date: 2026-11-22

## Context

ADR 0207 gave a window two answers to a call it could not take at once: the `busy` sentence for a
piece in hand or a question waiting for the person, and the same sentence from the channel's
deadline when no frame answered at all. Both stood in front of the same words, and neither said
what had happened to the document.

Measured on this tree (X107, 2026-11-21): a `command_batch` over the channel created its node, moved
the revision and left the tab dirty, and the caller read *the window did not answer: it is busy with
a gesture no assistant may interrupt*. The window had no gesture in it at all: it repaints on input,
so no frame ran for the caller's whole 120 s deadline, the answer went to nobody, and the first
pointer event after that ran the call — the edit landed in a document whose caller had already been
told nothing had. A caller that retried built the part twice; a caller that believed the sentence
reported a failure that did not happen (the clause about input-triggered repainting is X111).

Two facts decide this: a call that lands is always answered with its own report, so a landed call is
never refused for a gesture — the sentence the caller read came from the deadline, not from the
window — and a call still sitting in the mailbox is performed whatever the caller is doing, because
nothing in the queue knows the caller has gone.

## Decision

- **A dispatch waits for the first frame that has the window to itself.** A piece carried by the
  pointer and a question waiting for the person outlive a frame, and both now *delay* a call instead
  of refusing it: the call stays in the window's queue, and the frame after the gesture answers it
  with the report of what landed. "Not now" becomes "in the next frame that is ours", which is the
  same barrier ADR 0187 asks for — a command never interleaves with a gesture — without the refusal
  that lied about the document.
- **Reads are answered whatever the window is doing.** A listing, a query and the window's own report
  move nothing, so a question about the document is never held behind a gesture.
- **A held call asks for the frame it needs.** While a call waits in the queue the window requests a
  repaint, so the call is answered by the next frame rather than by the next pointer event. That is
  the clause of X111 that belongs to the channel; the rest of the window's repainting is that task's.
- **A call whose caller has gone is not performed.** Before a call that can move the document, the
  window asks the mailbox entry whether the caller is still waiting, and drops the call when it is
  not. A caller gives up at the channel's deadline, so the entry is the only place that knows; a
  change for a caller that will never hear about it is work nobody asked for and a part the next
  attempt builds twice.
- **The deadline sentence states what is unknown.** It names the deadline and tells the caller to
  read the scene before repeating the call. It never names a gesture, because the channel does not
  know what the window was doing — only that it did not answer.
- **`mcp::busy` goes.** The last word a caller gets is derived from what happened to the document:
  a batch's own report, the window's own report, the command's own refusal, or the one sentence that
  says the answer did not arrive.

## Consequences

- A caller that meets a gesture gets the report of a landed batch instead of a refusal, so it neither
  retries a build nor reports a failure it did not see. The person's gesture is never interleaved.
- A call can now wait for as long as a gesture lasts, bounded by the channel's 120 s deadline. A
  window that never paints (X111) still times out, but the call is dropped rather than performed, so
  the timeout means "not performed" in every case but the one below.
- A caller may still time out while the window is inside the dispatch. Then the answer is in flight
  and the change is in the document, which the sentence admits ("read the scene before repeating")
  instead of claiming either outcome. Exactly-once would need a call id the doors do not have; that
  is a separate decision, and no door needs it while one retry is humanly rare.
- The queue can hold entries across frames, so a person who closes the channel loses them: those
  callers read *the window is gone*, which is true.

## Related

- [0207](0207-the-token-comes-first-and-the-window-answers-between-frames.md) — the handshake, the
  mailbox and the `busy` answer this record amends.
- [0187](0187-a-chat-turn-is-one-undo-entry-and-a-plan-first.md) — the frame boundary a call waits for.
- [0188](0188-a-local-control-channel-is-not-a-network-service.md) — the permission and its bounds.
- `crates/mcp/src/channel.rs` (`Pending::abandoned`, the deadline answer),
  `app/src/main.rs` (`serve_channel`).
