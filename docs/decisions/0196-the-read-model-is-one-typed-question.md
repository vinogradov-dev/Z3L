# 0196. The read model is one typed question, and its stable answers are resources

- Status: Accepted; implements 0183's report set and 0185's "session read-only queries"
- Date: 2026-11-22

## Context

ADR 0183 put five reports on the command line and ADR 0185 said the assistant's tools are a projection
of the registry and "the session's read-only queries". Measured before this record: all five
derivations — the node walk, the reserved boxes, the published `parts` table, the panel diagnostics
and `Project::niche` — were rendered by `app/src/cli/query.rs`, inside the binary crate. `mcp` is a
library at layer 7 and `app` is the composition root at layer 8, so the crate that has to serve the
tool (ADR 0186) cannot reach them: the read model was reachable by a person with a terminal and by
nobody else. The command line also read its five names as loose words plus two optional flags, so
`parts --at 1,2,3` was silently answered, and nothing about the shape of a question was published for
a caller to build a request from.

## Decision

- **The question is one typed value in `engine`.** `Query` has five variants — `Tree`, `Box`, `Parts`,
  `Diagnostics`, `Niche` — and no variant carries an argument it does not read: the node belongs to the
  three that narrow one and the point to `niche` alone. It derives `serde` and `JsonSchema`, so the
  wire form a client sends (`{"kind": "tree", "node": 5}`) and the schema a tool publishes are the
  type's own, the way a command's parameters already are (ADR 0195). `Query::NAMES` and
  `Query::name()` are the one list a refusal, a help text and the JSON tag read.
- **`Session::read` answers it, and writes nothing.** A question leaves the revision, the history and
  the dirty flag exactly where they were, which is what lets a client ask one in the middle of a
  person's edit. Each answer reuses the derivation that already owns it, so the window, the command
  line and the assistant cannot disagree about what the document holds.
- **`engine` gains `export` for one derivation.** The cut list is the order package's `parts` table and
  `export` owns it; a second cut list in `engine` would be the second truth ADR 0095 exists to prevent.
  The edge follows the layers (`export` is 3, `engine` is 6) and adds no new crate to the tree.
- **The envelope belongs to the door.** Which file and which revision an answer is about is the shell's
  knowledge of paths, not the document's, so `Session::read` returns the rows and the door prints
  `query`, `document` and `rows`. A path in the engine would tie a session to a file it never opened.
- **The stable answers are resources.** The four questions that take no argument — `tree`, `box`,
  `parts`, `diagnostics` — are also read by URI, `z3l://scene/<name>`, and listed by
  `query --resources` with their media type. A resource is an address for an answer whose inputs change
  only when the document does, so a client caches it by the revision the answer carries. `niche`
  measures a place the caller chose, so it stays an argument of the tool call and is no resource.
- **The URI is the door's spelling, for now.** The table lives in `app/src/cli/query.rs`, beside the
  words, and moves into `mcp` when that crate lands (J5), the way the tool projection does. `engine`
  answers questions and knows nothing about addresses.
- **`niche` is measured through `Sight::none()`**, the sight ADR 0183 recorded: a question has no window
  and so no hidden set. A caller that needs a person's sight asks the editor, not the read model.

## Consequences

- A headless reader, the future MCP tool and the editor's inspector call one function, and a new
  question is one variant with one arm in `answer`.
- `engine` compiles `export` for every dependent. That is the price of "the cut list has one
  derivation"; a leaner door would have meant a second table.
- The resource table is a small, honest list rather than a second registry: four rows, each naming the
  query it reads, so a resource cannot answer something the tool cannot.
- A resource read is a snapshot of a *loaded file*, not of the window a person is looking at. Reaching
  the live document is the local control channel (ADR 0188, J6), and this record says so where a client
  would otherwise assume the address is live.
- The word form and the URI form are one parse: `tree` and `z3l://scene/tree` reach the same variant, and
  the test that reads both compares the rows rather than the envelopes.

## Related

- [0183](0183-the-command-line-is-the-one-headless-door.md) — the five reports and the envelope this
  record moves into the session and the door.
- [0185](0185-the-tool-registry-is-one-table-and-every-door-reads-it.md) — the four coarse tools and
  the schema derived from the type.
- [0186](0186-the-mcp-crate-sits-beside-the-ui.md) — why the read model could not stay in `app`.
- [0188](0188-a-local-control-channel-is-not-a-network-service.md) — the live document a resource read
  is not.
- [0195](0195-the-parameter-schema-derives-where-its-type-lives.md) — the schema rule this follows.
- `AGENTS.md` §3, §4.1, §4.11; `PROJECT.md` §Assistant.
