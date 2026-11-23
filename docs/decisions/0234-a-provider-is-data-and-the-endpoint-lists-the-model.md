# 0234. A provider is data: presets fill the address, and the endpoint lists the model

- Status: Accepted; amends 0189's "no provider name appears in a type, a match arm or a default"
- Date: 2026-11-23

## Context

ADR 0189 left the assistant with one wire and two free-text fields. The owner met the cost of that
in the first weeks: a base URL pasted into itself (J22), a gateway that routes on a header nobody
knew to send (X124, J23), a model name typed from memory and misspelled. They asked the program to
know its providers and to offer their models, "as SolidCraft does", and left the choice between the
small surface and a second wire: this record is the small one.

Measured 2026-11-23, over the owner's gateway without a key:

- `GET {base}/models` answers **without a credential**. The go tier lists 36 models, the plain tier
  105, and the same ids on both the `openai` and the `anthropic` family paths.
- The two families are real wires: `POST /inference/openai/v1/chat/completions` and
  `POST /inference/anthropic/v1/messages` each answer 401 without a key, while the cross paths
  answer 404. So a family is a protocol and not an alias of a path.
- **Every family serves every model.** The 105 ids of the `openai` family and of the `anthropic`
  family are the same set, so a `claude-*` model is reachable today through the OpenAI wire already
  implemented (ADR 0189). A protocol matrix chosen by model name would buy a second body shape and
  nothing a person can see.
- `inference/go/v1`, `inference/go/openai/v1` and `zen/go/v1` are three spellings of the go tier that
  all need `x-opencode-session`; the client recognises the tier by its name and path, not by which
  spelling a person pasted (X124).

## Decision

- **A preset is data, and it is only an address.** `ui::settings::PRESETS` is six rows of a name and
  a base URL — the go tier, the plain tier, DeepSeek, OpenAI, Ollama and LM Studio — and choosing
  one writes the address into the field and nothing else. No type, match arm or default of the wire
  names a provider; the table lives in the page, where a name is a word a person reads.
- **The row is derived from the field, never remembered beside it.** `AssistantSettings::preset()`
  finds the row whose address equals the field's; a typed address reads as *Other address* and the
  row writes nothing when it is chosen. The field stays the person's own, which is what the owner
  asked for explicitly: presets help, they never replace the manual setting.
- **The model list is the endpoint's answer.** `mcp::openai::models` reads `GET {base}/models` and
  parses `data[].id`, taking the OpenAI envelope and the Anthropic one because one gateway answers
  both. The key travels when one of its homes holds one — a listing is a read, and a workspace
  without a key can still see what an endpoint offers.
- **The listing is asked by a person, off the frame.** The page cannot speak HTTP (`ureq` lives in
  `mcp`, and `ui` may not depend on it), so the button reports the address through
  `SettingsOutcome::list_models`, the shell starts one request on a worker thread
  (`app/src/model_list.rs`) and hands the answer back as `ListedModels`. It is view state: one
  address at a time, nothing written to the settings file, gone when the page closes.
- **A refusal keeps the gateway's words.** A listing that fails draws the sentence the wire refused
  with, not "failed", which is the rule ADR 0227 states for every surface.

## Consequences

- A person names a model in two clicks and never types a header: the Go quirk stays the program's,
  keyed on the address (X124, unchanged).
- A new provider costs one row of data when its wire is OpenAI-compatible, and a served model costs
  nothing at all — the endpoint lists it.
- **The family matrix is not implemented and is not needed yet**, because every family serves every
  model. A model that is served only on the Anthropic or the Google wire would need that wire: the
  `anthropic` family path plus `/messages`, its content blocks and its `tool_use`/`tool_result`
  shapes, proven against a stub because no key is on this machine. Recorded in `plan/debt.md`.
- The preset table is a seed, not a catalogue: a workshop whose provider is missing types its
  address, exactly as before.
