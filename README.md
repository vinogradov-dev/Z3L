# Z3L

Native, free and open-source 3D editor for parametric cabinet furniture: set dimensions, panels and hardware, get a 3D model — and, later, drawings and CNC programs.

Built with Rust, `wgpu` and `egui`: a native binary, with no Electron, no Tauri, no web UI and no JavaScript.

## Quick start

```sh
cargo run -p app            # opens the editor as `z3l`
cargo test --workspace
cargo xtask layers          # checks the crate dependency rules
```

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

MIT OR Apache-2.0, at your option: [`LICENSE-MIT`](LICENSE-MIT), [`LICENSE-APACHE`](LICENSE-APACHE).
