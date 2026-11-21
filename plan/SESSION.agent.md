# Session

```text
task: J17
goal: a product cannot be opened over the channel, so no furniture part can be born

steps:
  [x] engine: `Session::open_named_product` and one refusal sentence for an id that is not a product
  [x] mcp: `product` on `command_run` and `command_batch`, the answer names `open_product`
  [x] the command line opens through the same call, so both doors print one sentence
  [x] tests: the panel lands inside the named product, none named is the command's own sentence,
      opening moves no revision and leaves no entry
  [x] ADR 0212, the PROJECT.md sentence, the map rows and this note
  [ ] the primary closes the entry after the merge: this worktree cannot write `STATE:` (P23)

now: the code, the tests and the ADR are green in this worktree; the primary merges and closes J17
blocked: P23 — a stream worktree refuses `task step` and `task close`, so the marker was set by hand
run: cargo test -p mcp --test served && cargo test -p engine --lib && cargo test -p app --bin z3l cli::recipe
resume: the primary merges stream/agent into dev and closes J17 with the merged hash
map: Tool server over stdio — crates/mcp/src/server.rs (CommandServer::dispatch)
decisions: the field is on the batch (ADR 0212), not a second tool; opening happens before the
  steps and is not one, so a rolled-back batch leaves the product open
```
