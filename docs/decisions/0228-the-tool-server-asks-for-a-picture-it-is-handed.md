# 0228. The tool server asks for a picture it is handed

- Status: Accepted
- Date: 2026-11-23

## Context

ADR 0193 asserts a whole agent task through the surface a person has, and ADR 0028's last clause is
that the assistant sees the scene as data — with a screenshot channel carved out for the decision it
needs. A tool that returns what was built is that channel: a model that composed a cabinet can only
be sure of it by looking.

Measured before the first edit: nothing in the tree rendered without a window. `crates/render` owned
an offscreen colour target whose `COPY_SRC` usage was granted in a test build only, `final` owned the
one PNG writer and it took *radiance* rather than the target's already display-ready bytes, and
`mcp` is forbidden `wgpu`, `egui` and `eframe` (ADR 0186) — so the crate that answers tools cannot
own a device, and the crate that owns the device cannot answer a tool.

## Decision

- **The port is `mcp::Previewer`, the implementation is the shell's.** The trait takes the session and
  a checked `PreviewSize` and answers the bytes of one PNG; `app/src/preview.rs` implements it over a
  windowless `egui_wgpu::RenderState` — evaluate the session's scene, frame it, `render`'s
  `ViewportRenderer`, `read_color`, `encode_rgba8`. A server holds `Option<Arc<dyn Previewer>>`, and
  `None` is a process without a device: the tool then refuses with a sentence that says so, and every
  other tool still answers.
- **The picture is an image content block, not a field of the JSON.** `Answer` carries an optional
  PNG; `result()` appends `ContentBlock::image` beside the report of width, height, byte count and
  revision. A client draws it without decoding our answer, which is the whole point of the tool.
- **Two caps, both refused, neither clamped.** A side outside `MIN_SIDE`..=`MAX_SIDE` (64..=1600) is
  refused before anything is uploaded or drawn; an encoded picture over `MAX_BYTES` (4 MiB) is
  refused instead of sent. A clamp would answer a picture the call did not ask for with nothing saying
  so — the same reason `final` refuses a size past 8192 rather than shrinking it.
- **The framing is the document's.** An authored camera wins; otherwise the view is fitted to the
  scene, exactly as *Frame all* fits it. A headless process has no viewport at all — `engine::Session`
  holds no camera (ADR 0029) — so "what the person is looking at" cannot be the rule both doors keep,
  and a test could not tell which picture it measured.

## Consequences

- `render`'s colour target always carries `COPY_SRC` and gains `read_color`, which strips the row
  padding the format implies; `final` gains `encode_rgba8` and `encode_png` becomes its caller, so
  one crate still owns the one PNG writer.
- A preview costs one evaluation, one upload and one device pass. The frame a call is answered on is
  the gesture-free one (ADR 0223), so a preview never draws a drag that has not landed.
- The picture is the viewport's look, not the tracer's: a preview is the thing a person recognises,
  and a converged final render is seconds per image.
- base64 is twenty lines in `mcp` with a test. The protocol wants the string, and a dependency for one
  table is what `AGENTS.md` §4.8 rules out.
- `mcp` still cannot reach a device: the tree shows it in `cargo xtask layers`, and only `app` — the
  composition root — implements the trait.
- A picture cannot be served by `z3l mcp serve` on a machine with no adapter. That is a sentence in
  the answer, not a hidden tool: the tool list is the build's, and it does not depend on the GPU.

## Related

- [0186](0186-the-mcp-crate-sits-beside-the-ui.md) — why the device cannot live in `mcp`;
  [0188](0188-a-local-control-channel-is-not-a-network-service.md) — the two doors that answer it.
- [0193](0193-an-agent-task-is-the-acceptance-test.md) — the scenario that asserts through
  `render_preview`; [0028](0028-the-assistant-is-a-client-of-the-command-registry.md) — the
  screenshot channel this record decides.
- [0029](0029-view-state-belongs-to-the-session.md) — why no camera travels with the document.
