# Session

```text
task: J6
goal: the local channel attaches an agent to the window a person is looking at

steps:
  [x] crates/mcp: the loopback channel, the token file, the handshake, the mailbox and ui_inspect
  [x] the Assistant page, its one switch and its three messages
  [x] the window answers at its frame boundary; `mcp serve --attach` serves a headless session
  [x] ADR 0207, the PROJECT.md sentence, the map row and the research note
  [x] the gate: fmt, clippy, tests, layers, ui-rules, docs, skills — and `plan` after P23 landed
  [ ] the primary merges this branch into `dev` and closes the entry with the merged hash

now: the work is committed on stream/agent; only the merge into `dev` and the close remain
blocked: nothing
run: cargo xtask plan && cargo test -p mcp -p app
resume: in the primary checkout, merge stream/agent into dev and close J6 with the merged hash
```
