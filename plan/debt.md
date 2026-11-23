# Known debt

Gaps accepted for now. Read the entries whose tag matches the area of the task in hand; the whole file
is not part of a session start. An entry leaves this file when the fix lands.

Tags: `[view-state]` session versus document, `[render]` lighting and drawing, `[mesh]` geometry
editing, `[object]` constructive objects, `[panel]` panels and banding, `[scene]` tree and files,
`[gizmo]` manipulation, `[types]` properties and furniture types, `[engine]` commands and history,
`[perf]` frame and picking cost, `[assistant]`, `[ui]` panels, toolbars and icon artwork, `[design]`
module size, layer arrows and duplicated knowledge, `[plan]` the plan tool and its format,
`[vendor]`.

- `[assistant]` `openssl-sys` is the one counted exception to the C-vendoring rule: the assistant's
  transport is `ureq` over the operating system's own TLS, and on Linux that is the system OpenSSL.
  `cargo xtask layers` allows the crate while the resolved graph leaves its `vendored` feature off and
  fails the moment a dependency turns it on, so what is forbidden is building OpenSSL from the C
  sources inside the crate, not binding the library the system already carries (J14, ADR 0189). It
  leaves when a maintained pure-Rust TLS provider exists whose crypto is neither `ring` nor
  `aws-lc-rs`; until then the vendored build is the breach, and the feature is the switch.

- `[assistant]` The panel's model cannot see the scene. The tool server draws a picture
  (`render_preview`, ADR 0228) and a process that owns the loop may ask for it over the channel, but
  the chat wire offers no such tool: its answer would have to travel on the OpenAI wire as an image,
  which the port does not carry (J29 lists `command_list`, `query` and `inspect` and stops there).
  A model in the panel therefore proves a closed cabinet, a door gap or a mirror by `query parts`,
  `query bounds` and `inspect`, and never by looking. It leaves when the wire carries one picture and
  the port answers it — the `Previewer` the shell already holds is half of that.

- `[engine]` A piece cannot be put into a frame it is not a child of. Measured by the acceptance test
  (J13, scenario 5): `product.placement.set` on a product whose box would land inside another
  product's envelope is refused — "`Распашной шкаф 800×460×2200` would come to rest inside `Ниша`" —
  and nothing reparents an authored piece: `product.create` takes the root or a plain group as its
  parent, `node.group` makes a group out of siblings and `node.ungroup` the reverse. The owner's
  sentence "поставь его в нишу 820 × 2140 × 580" therefore ends in a refusal, and the 60 mm by which
  the piece is taller than the niche is arithmetic the model does, because no rule measures a piece
  against a frame it is not part of. What would land it: a `node.reparent` command with the write
  guard's containment rule unchanged, or a "put this piece in that frame" operation that also records
  the intent, and either one is the owner's decision.
