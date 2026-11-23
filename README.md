# Z3L

Native, free and open-source 3D editor for parametric cabinet furniture: set dimensions, panels,
materials and hardware, get a 3D model — and, later, drawings and CNC programs.

Built with Rust, `wgpu` and `egui`: a native binary, with no Electron, no Tauri, no web UI and no
JavaScript. The product is named **Z3L**: a zero and three laws — `Local`, `Literal`, `Lasting`
([`docs/philosophy.md`](docs/philosophy.md)).

## What it does

- A document of panels, containers, materials, hardware and typed properties, edited through
  commands with undo/redo; every gesture is a command with a stable id, so the window, the command
  line and the tool server reach the same document.
- A 3D viewport: orbit/pan/zoom, shading, picking, a floor grid, dimensions and gizmos.
- An order package other programs read: one manifest and flat tables of the product, its parts and
  its materials ([`docs/domain/order-export.md`](docs/domain/order-export.md)).
- A final render: a path tracer that draws a saved scene to one PNG.
- Automation: a recipe runner, a query report, a hand-editable domain guide, and an MCP tool server
  over stdio ([`docs/domain/`](docs/domain)).

## Quick start

```sh
cargo run -p app            # opens the editor as `z3l`
cargo test --workspace
cargo xtask ci              # fmt, clippy, tests, layers, ui-rules, plan, docs, skills
```

## Command line

The same binary is the editor and the headless door to it; every subcommand reads a saved scene and
writes nothing unless it is asked:

```sh
z3l                              # open the editor
z3l scene.z3l                    # open the editor on a scene
z3l directory [folder]           # edit a nomenclature folder
z3l run scene.z3l --commands steps.jsonl [--out result.z3l] [--dry-run]
z3l export scene.z3l --out package/
z3l render scene.z3l --out plate.png [--size 1920x1080] [--samples 256] [--studio 60]
z3l query scene.z3l tree|box|bounds|parts|diagnostics|niche [--id <node>] [--at <x,y,z>]
z3l guide | z3l commands        # the domain guide, the command list
z3l mcp serve [scene.z3l]        # the tool server on stdin/stdout
```

`z3l --help` prints the subcommands and their flags.

## Diagnostics

The shell writes a log to `z3l.log` in the application's config directory (`~/.config/z3l/z3l.log` on
Linux, `%APPDATA%\z3l` on Windows, `~/Library/Application Support/z3l` on macOS) and to stderr. The
level comes from `Z3L_LOG`: `off`, `error`, `warn`, `info`, `debug` (the default) or `trace`. A carry
logs the moved node, its world coordinate, the cursor, the delta the hand asked for and the delta the
document allowed, so a defect report is the file and one sentence about the gesture.

## Where to read next

- [`PROJECT.md`](PROJECT.md) — the product, the domain vocabulary and the crate map.
- [`AGENTS.md`](AGENTS.md) — the rules every change follows.
- [`BRIEF.md`](BRIEF.md) — the owner's goals and constraints (in Russian).
- [`plan/STATUS.md`](plan/STATUS.md) — the current development state lives only here.

## License

Apache-2.0: [`LICENSE-APACHE`](LICENSE-APACHE). Copyright 2026 the Z3L authors.
