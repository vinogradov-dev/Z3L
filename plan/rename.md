# The pre-release product rename

The record of task **R1**, closed at `d4a56f3`. The rename has landed, so this is history rather than
a procedure waiting to be taken: it keeps what the move measured — what the old name touched, what it
cost, the steps as they were run and the rules that outlived them. A future rename starts here and in
`plan/notes/R1.md`, which carries the search that chose the name, the registers it read and the roads
it closed.

Read it when a change adds a place where the product name appears: the identity has one home per
surface (ADR 0135), so a surface nobody thought about is the one thing a copy can hide in.

Measured on 2026-11-07 at commit `0b568c3` (`dev`), before the rename; `d4a56f3` is what the tree
became. `git show d4a56f3 --stat` measures the commit itself; §3's counts measure a tree that no
longer exists and are kept as the record of what it held.

## 1. Why the name was in the tree at all

Three different roles, with three different prices. Confusing them is what made a rename look
expensive.

1. **Crate identifiers.** `furnicraft-geom`, `use furnicraft_geom::DVec3` — a naming convention for
   workspace uniqueness, not a requirement. The project publishes nothing and has no
   `repository`/`publish` key in any `Cargo.toml`, so nothing outside the repo read these names.
   Became bare names: `use geom::DVec3`.
2. **Data identity on disk.** `FORMAT = "furnicraft.scene"` inside every `.fcs`, and the settings
   directory (`~/.config/furnicraft`, `%APPDATA%\FurniCraft`,
   `~/Library/Application Support/FurniCraft`). This was the only part that was a real contract.
   Became `project::FORMAT = "furniture.scene"`, the extension `project::EXTENSION = ".z3l"` and the
   settings directory named by `APP_ID`, today the word `z3l`.
3. **Words on the screen.** Window title, header mark, Home heading. Cheap even then:
   `theme::PRODUCT_NAME` already existed, it was just not used everywhere. Became one constant per
   surface, with a test that fails if a copy is missed.

Everything else — 20 `wgpu` debug labels in `crates/render/src/lib.rs`, `egui::Id` keys in
`crates/ui/src/icons.rs`, `//!` comments, prose in `docs/` — never had runtime meaning.

The name never reached the domain: no command id, no document field, no JSON key of the document
carried it. That is why the rename could not corrupt a document beyond the envelope's `format`
string, and why nothing was migrated.

## 2. Questions that shaped the work, and the answers it took

| Question | Answer recorded here |
|---|---|
| Is there a `apps/` level above the single app? | No reason. `app/` at the workspace root works: `members = ["crates/*", "app", "xtask", "state-contract"]`, `default-members = ["crates/*", "app"]`. Verified: builds, 1140 tests pass, `cargo xtask layers` ok. Taken. |
| Prefix on crate names, or bare `geom`/`model`? | Bare names work and read best: `use model::Project`. All of them are taken on crates.io (`geom`, `model`, `ui`, `render`, `mesh`, `param`, `units`, `kernel`, `engine`, `testkit`, `project`, `app`), which matters only if the crates are ever published; `scene` and `state-contract` are free. Keep the publish escape hatch: a prefix can be added later with `package = "..."` aliasing without touching a single `use`. Taken: bare. |
| Would `app-geom` / `app_model` be better than bare names? | It was a valid second choice with the same mechanical cost: `app-geom` is used as `app_geom`, so the same edit touches the same 597 references. It buys crates.io uniqueness and a visible workspace boundary, and it puts a five-letter prefix in every import. Bare names read better and the prefix survives publishing. Taken: bare, decided once, at R1. |
| Why is the envelope `format` a namespaced string at all? | Its only job is to reject a foreign JSON before parsing a document. `"scene"` alone is indistinguishable from another program's `"format": "scene"`; the dot is the usual `owner.thing` convention. The owner need not be a brand: `"furniture.scene"` keeps the check strong and survives any rename, which is why it is the recorded choice. |
| Backwards compatibility with old `.fcs` files? | Not needed — pre-release, no user files exist. Read one `format` string only, not two. |
| Does the settings directory need migration? | Not needed. Renaming it drops the saved theme, recent files and viewport defaults; acceptable before the first release. |
| Is `.fcs` a good extension? | It was `FurniCraft Scene` in disguise, so it changed with the name. It was **not a constant** then: two production literals in `app/src/file_menu.rs` (the `scene.fcs` default and the `"fcs"` filter) plus the fixtures around them (§5, step 1). Taken: `project::EXTENSION`, `.z3l`. |

## 3. Inventory (measured 2026-11-07; K2 added `furnicraft-export` on 2026-11-14 (four references in `Cargo.toml`, `xtask`, `PROJECT.md` and the lockfile) and `furnicraft-text`, added after the measurement, was already missing from the alternation; both are now included, so the counts below are a lower bound)

```sh
git grep -o -i furnicraft | wc -l                     # 1042 occurrences
git grep -l -i furnicraft | wc -l                     # 162 files
git grep -o -i furnicraft | cut -d/ -f1 | sed 's/:.*//' | sort | uniq -c | sort -rn
```

| Area | Occurrences | Files | What it is |
|---|---|---|---|
| `crates/` | 488 | 89 | 597 of the 652 Rust hits in the tree are crate references (`furnicraft[-_]<name>`); the other 55 sit on 53 lines |
| `app/` (`apps/furnicraft` today) | 181 | 16 | crate refs + 8 hardcoded `FurniCraft` in `main.rs` + settings paths |
| `docs/` | 65 | 32 | 26 crate refs, 12 `apps/furnicraft` paths, 12 bare `FurniCraft` prose, 14 bare `furnicraft` (the envelope id and `furnicraft_units`-style mentions) |
| `plan/` | 55 | 5 | 19 path hints in `BACKLOG.md`, 18 in `map.md`, 1 in `debt.md` |
| `.opencode/` | 57 | 4 | 32 of them in `skills/enforce-architecture/SKILL.md`; the skills teach the crate map by name |
| `xtask/src/main.rs` | 50 | 1 | the `CRATES` table, `DEV_ONLY`, `TRUCK_ADAPTER`, the `name != "furnicraft"` rule, tests |
| `Cargo.toml` | 15 | 1 | `[workspace.dependencies]` keys |
| `Cargo.lock` | 63 | 1 | regenerated, never hand-edited |
| `state-contract/` | 8 | 5 | doc examples and test fixtures only — cosmetic |
| `vendor/` | 3 | 2 | `vendor/AGENTS.md`, `vendor/truck-shapeops/PATCH.md` |
| `AGENTS.md` / `PROJECT.md` / `README.md` / `ATTRIBUTION.md` / `LICENSE-MIT` / `assets/README.md` | 57 | 6 | rules, crate map, product blurb |

`BRIEF.md` contains no product name: it is the owner's original brief and is never edited to match
the code.

## 4. What the rename cost

Measured by actually doing it in a scratch copy: mechanical rename + build + full test run + clippy +
`cargo xtask layers` green, **1140 tests passing, zero behaviour change**, 960 changed lines in 66
`.rs` files, 81 files total. The commit that landed it touched 338 files (2835 insertions, 3136
deletions), most of them `use furnicraft_x::` → `use x::`.

| Step | Time | Risk |
|---|---|---|
| 1. One constants home for the product identity (§5 step 1) | 1 h | none — it is a plain refactor with a test |
| 2. Mechanical crate/package rename (§5 step 2) | 2 h including the gate | low; the gate catches every miss |
| 3. Plan path hints and map rows (§5 step 3) | 30 min | the plan gate fails loudly if skipped |
| 4. Envelope `format` + goldens (§5 step 4) | 20 min | 8 fixture lines |
| 5. Prose in AGENTS/PROJECT/README/docs/skills/vendor (§5 step 5) | 1.5 h | drift: four registries must agree |
| **Total** | **~5–6 h, 2–3 commits** | |

After the rename, changing the visible name again is a one-line edit of `PRODUCT_NAME`.

## 5. The procedure, as it was run

R1 took four commits: `99e030a` (the philosophy and the name), `e0c22cc` (one home per surface),
`d4a56f3` (the tree — crates, paths, envelope, prose) and `58480ef` (the entry and the note say what
it did). The steps below are the ones it followed.

### Step 1 — give the identity one home

Four constants, one place each, and every call site reads them:

- `crates/ui/src/theme.rs` — `PRODUCT_NAME` (already existed, on-screen words only).
- the app shell — `APP_ID` (machine identity: window class, and the settings directory name). Set it
  through `egui::ViewportBuilder::with_app_id`, because `eframe` otherwise copies the `app_name`
  argument into `viewport.app_id` (`eframe-0.37.0/src/lib.rs:421`) and a marketing rename would then
  silently move `~/.config/<id>` and change the X11 `WM_CLASS` / Wayland app id.
- `crates/project` — `FORMAT` (exists) and a new `EXTENSION`.
- `crates/project` — `GENERATOR` name/version if a `generator` section is ever written; today the
  writer does not emit one.

Then remove the hardcoded duplicates:

- `main.rs`: `.with_title("FurniCraft")` and `"FurniCraft"` as the `run_native` app name → `PRODUCT_NAME`;
  the error string `"FurniCraft requires the wgpu renderer"` → `PRODUCT_NAME`;
  `struct FurniCraftApp` → `App`.
- `app/src/file_menu.rs` — `"scene.fcs"`, `eq_ignore_ascii_case("fcs")` and the dialog listing take
  `project::EXTENSION`.
- `app/src/settings_store.rs` — the three platform branches and `join("furnicraft")` go through `APP_ID`.

Tests: the header already asserts against `theme::PRODUCT_NAME`; add one that the window title, the
`app_id` and the settings directory all come from the constants, and one that the open dialog lists
files by `EXTENSION`.

### Step 2 — mechanical rename

Nothing here needs thought; run it in this order.

```sh
# package names of the 15 library crates
for f in crates/*/Cargo.toml; do
  sed -i -E 's/^name = "furnicraft-(units|geom|kernel|kernel-truck|model|param|mesh|project|export|scene|render|engine|ui|testkit|text)"$/name = "\1"/' "$f"
done

# workspace dependency keys that carry a path
sed -i -E 's/^furnicraft-(units|geom|kernel|kernel-truck|model|param|mesh|project|export|scene|render|engine|ui|testkit|text) = \{ path =/\1 = { path =/' Cargo.toml

# every member's dependency lines
sed -i -E 's/^furnicraft-(units|geom|kernel|kernel-truck|model|param|mesh|project|export|scene|render|engine|ui|testkit|text) = \{ workspace = true \}/\1 = { workspace = true }/' \
  Cargo.toml crates/*/Cargo.toml app/Cargo.toml

# Rust paths and doc references
find crates app -name '*.rs' -print0 | xargs -0 \
  sed -i -E 's/furnicraft_(units|geom|kernel|kernel_truck|model|param|mesh|project|export|scene|render|engine|ui|testkit|text)\b/\1/g'

# the app package and its directory
mv apps/furnicraft app && rmdir apps
# then in Cargo.toml: members = ["crates/*", "app", "xtask", "state-contract"],
#                    default-members = ["crates/*", "app"]
# and in app/Cargo.toml: name = "app", with [[bin]] name = "z3l"

rm Cargo.lock          # regenerate; never hand-edit it
cargo fmt --all        # shortening names re-wraps 28 files at max_width 120
```

### Step 3 — registers that must agree in the same commit

1. `xtask/src/main.rs` — every `CrateRule.name`, `DEV_ONLY`, `TRUCK_ADAPTER`, the composition-root
   exception `name != "app"`, the `forbidden` list of `render` (it named `furnicraft-model`), and the
   assertions inside the test module.
2. `AGENTS.md` — the crate map table (§3) and the companion text beside it.
3. `PROJECT.md` — the crate map table (§ around line 212) and the L8 row.
4. `.opencode/skills/enforce-architecture/SKILL.md` — the full per-crate layer list; it was the
   largest single block of names outside the code and it teaches the next agent. The same file is
   under `.pi/skills/` today.
5. `plan/BACKLOG.md` (19 hints), `plan/map.md` (18 paths), `plan/debt.md` (1). The plan gate fails
   on any hint naming a path that no longer exists (`state-contract/src/lib.rs:179`), so these had to
   land in the same commit as the move.

`README.md` (`cargo run -p furnicraft`), `PROJECT.md` prose, `assets/README.md` and `ATTRIBUTION.md`
took the same pass.

### Step 4 — the envelope

`crates/project/src/lib.rs:36` was the only runtime string. Change it, then the 8 fixtures under
`crates/project/tests/golden/` carry it on line 2 — rewrite that one line in each, or the loader
rejects them before any migration runs.

Do **not** touch `crates/project/tests/golden/scene-v8.fcs:216` `"generator": {"name": "FurniCraft"}`:
it is the preserved-unknown-section fixture, and `tests/scene_file.rs:112` asserts it survives a read
and a write. It is history, not output.

### Step 5 — prose

`AGENTS.md` §2 used to say "the product name is always FurniCraft in user-facing text" — that
sentence was the first to change. Then `docs/` (32 files: 12 bare mentions in `docs/domain/` and four
ADRs, plus `apps/furnicraft` paths in ten ADRs), `vendor/AGENTS.md`,
`vendor/truck-shapeops/PATCH.md`, `state-contract`'s examples and fixtures.

ADRs are habitually superseded, never rewritten; for a rename a one-line path fix is fine, because
the decision they record did not change.

## 6. Gotchas, each verified

- **`mod mesh;` and `mod project;` live inside `crates/model`.** A crate named `mesh` or `project`
  is shadowed by that module inside the crate, and `mesh::thing()` then resolves to the module, not
  the crate — reproduced as `E0425`. No conflict today (`model` may not depend on either), but if
  that dependency ever appears the module must be renamed (`mod mesh_value;`) or the path written
  `::mesh::…`. Same rule for `crates/ui`'s `mod command;` if a `command` crate is ever added. This is
  the one rule here that outlived the rename: `cargo xtask layers` enforces it (P16).
- **`ui` is also a parameter name** in 206 `|ui|` closures. `ui::theme::install()` still resolves to
  the crate, because Rust resolves extern crate names in paths before locals — verified in this
  workspace and in a minimal crate.
- **Shortening names re-wraps code.** `rustfmt.toml` sets `max_width = 120`, so `cargo fmt --all`
  rewrites 28 files that were only re-wrapped. Run it before the first gate, not after.
- **Every crate name is taken on crates.io.** Harmless while nothing is published; it is the one
  argument for a prefix.
- **`cargo xtask plan` is the trap.** It reads the hint paths and the commit hashes, and the rename
  commit itself makes the old hashes fine and the old paths dead. Do step 3 in the same commit.
- **`--all-targets` clippy is the real check**, because tests carry most of the render labels and
  fixture paths.

## 7. Gate

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo xtask layers
cargo xtask plan
# or: cargo xtask ci
```

Then check by hand: the window title, the header mark, the Home heading, the settings path, and that
`cargo run -p app` still opens the editor.

Measured on the scratch copy before the tree moved: 1140 tests, 0 failures, `layers: ok`, clippy
clean. The commit that landed it measured 2208 tests, clippy clean, `layers: 17 crates`,
`ui-rules: 93 files`, `plan: 255 tasks`, `docs: 136 decisions`, `skills: 18 files` (in
`plan/notes/R1.md`).

## 8. The questions the owner answered

- **The final product name: `Z3L`**, read as a word ("zel"), *Zero · 3 Laws* (ADR 0135,
  `docs/philosophy.md`). The candidates and the registers that closed them are in `plan/notes/R1.md`.
- **The app package and binary name: `app`, binary `z3l`** — `cargo run -p app` opens the editor and
  `target/debug/z3l` is the program.
- **The envelope identity: `"furniture.scene"`**, so the format string names a contract and not a
  brand, with the extension `.z3l` because a file association is what a person sees.
- **`APP_ID` is the short word `z3l`**, so the settings path reads `~/.config/z3l`. A reverse-DNS id
  (`dev.<owner>.z3l`) is the convention for `.desktop`, `flatpak` and Wayland; it is a one-line change
  when a package ships.

## 9. Where to look in five minutes

Do not re-read the old inventory. These commands show what the rename was and what it left, and they
are cheap.

```sh
git show d4a56f3 --stat                    # the mechanical commit: 338 files
git log --oneline --grep='(R1)'            # the four commits
git grep -n -E 'PRODUCT_NAME|APP_ID|FORMAT|EXTENSION' -- '*.rs'   # the four homes of the identity
git grep -c -i furnicraft -- ':!plan' ':!docs'                    # what is left outside the records
```

The old name survives on purpose in four places, and none of them is output: the golden's `generator`
object (§5 step 4), `plan/archive/done.md`, the closed entries in `plan/BACKLOG.md` that record what
the tree used to be, and this file.

To see a rename's worth of work again, re-create it from §5 step 2 on a scratch copy: the mechanical
part is four `sed`s and one `mv`, and it builds in one `cargo build --workspace`.
