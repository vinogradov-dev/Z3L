# 0193. An agent task is the acceptance test

- Status: Accepted
- Date: 2026-11-21

## Context

ADR 0028 asks for a whole task runnable by a scripted model without a network, and the assistant's
milestone repeats it. The risk this guards against is specific: a tool surface can pass fifty unit
tests and still be unusable by a model, because the units are wrong, because a refusal cannot be acted
on, because `enabled` never says why, or because the answer is nine hundred lines of JSON. None of that
shows up in a test of one command.

Measured: the tree has no integration test directory (`app/src/frame_tests.rs` drives the interface,
and each crate tests its own unit); the CLI's recipe tests (ADR 0183) are the closest thing to a task
run, and they assert on a file. There is no MCP client in the tree to speak to a server with.

## Decision

- **One integration test file runs whole tasks through the real protocol.** It starts the server the
  way a client does (a child `z3l mcp serve` over stdio, and, once `--attach` exists, the local
  channel), speaks MCP to it, plays the model with a **scripted** adapter — no network, no key — and
  asserts on the outcome, not on the calls.
- **Five scenarios are the first set**, and they are the owner's own words:
  1. "build a cabinet 800 × 2200 × 560" — a recipe through `product.build`, in `plan` first and then in
     `build`;
  2. "check it" — `inspect` returns findings a person can read, and the scripted model explains them;
  3. "make it 900 wide" — a change to existing work, not a new build, and the parts that depend on the
     width follow;
  4. "copy it and change the doors" — a copy plus an edit, in one turn;
  5. "put it in a niche 820 × 2140 × 580" — the furniture moved into a void, measured against the
     niche's own faces.
- **A scenario asserts through the surface a person has**: `inspect`/`query` for state, the file for
  persistence, `render_preview` once it exists for what is drawn. It never reaches into the
  `Project` — a test that reads the document directly would pass on a tool the UI cannot use.
- **Each scenario asserts the same four things**: the task completed within its budget; one undo entry
  per turn, and `undo` restores the document the scenario started from; the number of mutating calls a
  turn made; and that a refused call is reported as a refusal the next call can act on.
- **The scripted model is a fixture, not a mock of our own code**: it is a list of turns — the tool
  calls and the text — which is exactly what a recorded conversation is. A new scenario is a new
  fixture file, not new test code.
- **The file is slow and runs once.** A real subprocess and real dispatch take seconds, so the test
  keeps its scenarios in one binary and `cargo xtask ci` runs it with everything else; a per-command
  test stays in the crate that owns the command.

## Consequences

- The tool surface has a contract a person can read: five sentences in the owner's words, each one a
  file. A tool whose semantics change breaks a scenario, which is the point.
- The acceptance test is also the documentation: it is the only place that shows the order a model
  actually uses — list, plan, build, inspect, adjust.
- A scripted model means the test is deterministic and free, and it cannot catch a bad real model. That
  is a manual step with a key, named in the task that lands this file, not part of the gate.
- `render_preview` is asserted last, when it exists, so the scenario order in this record is also the
  order of the work.
- The five scenarios are the milestone's definition of done, so M10 closes on a green file rather than
  on a demo.

## Related

- [0185](0185-the-tool-registry-is-one-table-and-every-door-reads-it.md) — the tools the scenarios call;
  [0188](0188-a-local-control-channel-is-not-a-network-service.md) — the two ways a client reaches them.
- [0190](0190-plan-and-build-are-the-persons-switch.md),
  [0192](0192-a-recipe-is-rust-that-emits-commands.md) — the plan/build and recipe behaviour a scenario
  asserts.
- [0183](0183-the-command-line-is-the-one-headless-door.md) — the recipe run this test grows out of.
- `PROJECT.md` §Milestones (M10); `AGENTS.md` §4.7 (tests are the gate).
