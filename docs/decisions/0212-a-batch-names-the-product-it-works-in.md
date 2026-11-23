# 0212. A batch names the product it works in, and opening is not a step

- Status: Accepted; the field ADR 0098 leaves a caller without
- Date: 2026-11-21

## Context

ADR 0098 keeps exactly one product open, and a furniture part is born only inside it. Opening is a
session view operation and not a command: the window opens what a person double-clicks, and a
headless run opens with `--product`. A caller on the tool surface had no such door. Measured on this
tree: `product.create` over the channel succeeded and `command_list` then answered
`open_product: null`, so `query niche` refused and `panel.create` stayed `enabled: false` — every
furniture command was unreachable after the frame was made, which is the whole of a build.

Two shapes were possible: a field a batch carries, or a question about the session that sets it. A
question would be a second tool (or a write hidden in `query`, which reads and never writes), and the
state it sets is only useful to the batch that follows it — one call is the honest unit.

## Decision

- **`command_run` and `command_batch` take `product`,** the id of the product the call works in. The
  schema prints it like every other argument, so a model learns it from `command_list` and the tool
  description rather than from a text field.
- **Naming a product opens it before the steps run, and opening is not a step.** It moves no
  revision, leaves no undo entry, and a refusal takes back the change and not the product: the call
  works in the product it named even when its steps rolled back. Undo after a landed batch removes
  the parts and leaves the product open, which is the state the caller asked for.
- **The answer names the product the batch ran in.** The report carries `open_product`, the same
  field `command_list` prints, read from the session after the batch. A caller that omitted the field
  learns which product the person had open instead of assuming one.
- **None named is no guess.** A batch without `product` works in whatever product is open; a scene
  with two products and none open leaves the session alone and the first step that needs a product
  refuses in its own words — `Create a product first.` — the sentence `z3l run` already answers.
- **An id that is not a product is one sentence, and it writes nothing.**
  `Session::open_named_product` returns `OpenProductError`, and both doors print it: the tool surface
  answers the call refused, the command line fails the run with it. The open itself stays
  `Session::open_product`, so the double click, the flag and the field are one rule.
- **Plan mode does not stop the open.** ADR 0190 makes the mode a switch over what may write the
  document, and this writes no document state; a plan a caller reads is a plan of the product it
  named.

## Consequences

- A build over the channel is one call: the product, the parts and the undo entry they are.
- The person's open product can change under a call, because a caller may name another one. The tree
  and the inspector follow it, and one double click puts it back; the alternative — refusing to open
  while a person is looking — would make the channel useless in the window it exists for.
- `open_product` is a number and not a name: a caller that has just created a product must still read
  the id from the answer, and a reference to an id a step created stays J18's problem.
- The command line keeps `--product` and reads the same sentence the wire prints; the two doors agree
  about what an id that is not a product means.
- If a second product ever opens at once, this field keeps its meaning — which product the batch
  works in — and the session decides the rest.
