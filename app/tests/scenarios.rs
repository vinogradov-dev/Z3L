//! The acceptance test ADR 0193 names: whole tasks, through the real protocol, by a scripted model.
//!
//! The server is a child process the test starts the way a client does — `z3l mcp serve <scene>` —
//! and the client is this file: newline-delimited JSON-RPC over that process's own standard streams,
//! which is what a foreign client speaks. The model is a list of tool calls, so the test needs no
//! network, no key and no adapter, and it reads nothing from this repository at run time except the
//! fixtures a scenario starts from: what it knows about the surface is what the surface answered.
//!
//! Every scenario asserts the outcome a person would see, through the tools a person's client has:
//! the cut list `query parts` prints, the findings `inspect` reports, the revision, and whether one
//! `undo` returns the document the turn started from. Nothing reaches into the `Project`: a test that
//! read the document directly would pass on a tool the window cannot use.
//!
//! It is slow on purpose (a process, a handshake and a real build per scenario) and it runs once, with
//! everything else `cargo test --workspace` runs.

#![allow(clippy::expect_used)]

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{Value, json};

/// The binary cargo built for this test: the server every scenario starts.
const Z3L: &str = env!("CARGO_BIN_EXE_z3l");

/// The protocol version the handshake names, which is one this build's server speaks.
const PROTOCOL: &str = "2025-06-18";

/// One path inside the fixtures directory.
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("recipes").join(name)
}

/// A scratch folder holding one workshop: its settings, its catalogue and the scene it serves.
///
/// The settings folder is a copy of the recipe's `catalogue/` rows, so a scenario builds against a
/// workshop this test owns instead of whatever the machine happens to stock (`app/tests/recipes`).
struct Workshop {
    root: PathBuf,
}

impl Workshop {
    /// A workshop named after its scenario, holding `scene` as the document the server serves.
    fn new(name: &str, scene: &str) -> Self {
        let root = std::env::temp_dir().join(format!("z3l-scenario-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let settings = root.join("config").join("z3l");
        std::fs::create_dir_all(&settings).expect("the settings folder");
        copy_dir(&fixture("catalogue"), &settings.join("nomenclature"));
        std::fs::copy(fixture(scene), root.join("scene.z3l")).expect("the scene copies");
        Self { root }
    }

    /// The `XDG_CONFIG_HOME` the child reads its settings and its catalogue from.
    fn config(&self) -> PathBuf {
        self.root.join("config")
    }

    /// The scene the server serves, and writes when a call lands.
    fn scene(&self) -> PathBuf {
        self.root.join("scene.z3l")
    }
}

impl Drop for Workshop {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Copies a fixture tree, which is two files deep at most.
fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("the directory copies");
    for entry in std::fs::read_dir(from).expect("the fixture reads") {
        let entry = entry.expect("a readable entry");
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).expect("the file copies");
        }
    }
}

/// One answer a tool gave: the JSON the protocol carried, and whether the call failed.
#[derive(Clone, Debug, PartialEq)]
struct Answer {
    body: Value,
    failed: bool,
}

/// The client: a child server, and the protocol written out over its streams.
struct Client {
    child: Child,
    writer: ChildStdin,
    reader: BufReader<ChildStdout>,
    next: u64,
}

impl Client {
    /// Starts `z3l mcp serve` on the workshop's scene and completes the handshake.
    fn start(workshop: &Workshop) -> Self {
        let mut child = Command::new(Z3L)
            .arg("mcp")
            .arg("serve")
            .arg(workshop.scene())
            .arg("--product")
            .arg("2")
            .env("XDG_CONFIG_HOME", workshop.config())
            .env("HOME", workshop.root.join("home"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("the tool server starts");
        let writer = child.stdin.take().expect("the child's standard input");
        let reader = BufReader::new(child.stdout.take().expect("the child's standard output"));
        let mut client = Self { child, writer, reader, next: 1 };
        let greeting = client.request(
            "initialize",
            json!({
                "protocolVersion": PROTOCOL,
                "capabilities": {},
                "clientInfo": {"name": "z3l-acceptance", "version": env!("CARGO_PKG_VERSION")},
            }),
        );
        assert!(
            greeting.get("protocolVersion").is_some(),
            "the server answers the handshake with a version: {greeting}"
        );
        client.notify("notifications/initialized", json!({}));
        client
    }

    /// One request, waited for by its id.
    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next = self.next.saturating_add(1);
        let frame = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        writeln!(self.writer, "{frame}").expect("the frame is written");
        self.writer.flush().expect("the frame reaches the server");
        loop {
            let mut line = String::new();
            let read = self.reader.read_line(&mut line).expect("the server answers or closes the stream");
            assert!(read > 0, "the server closed its stream before answering `{method}`");
            let frame: Value = serde_json::from_str(line.trim())
                .unwrap_or_else(|error| panic!("the server wrote a frame that is not JSON ({error}): {line}"));
            if frame.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if let Some(error) = frame.get("error") {
                panic!("`{method}` failed: {error}");
            }
            return frame.get("result").cloned().unwrap_or(Value::Null);
        }
    }

    /// One notification, which has no answer.
    fn notify(&mut self, method: &str, params: Value) {
        let frame = json!({"jsonrpc": "2.0", "method": method, "params": params});
        writeln!(self.writer, "{frame}").expect("the notification is written");
        self.writer.flush().expect("the notification reaches the server");
    }

    /// One tool call.
    fn call(&mut self, tool: &str, arguments: Value) -> Answer {
        let result = self.request("tools/call", json!({"name": tool, "arguments": arguments}));
        let failed = result.get("isError").and_then(Value::as_bool).unwrap_or(false);
        let body = match result.get("structuredContent") {
            Some(body) if !body.is_null() => body.clone(),
            _ => {
                let text = result
                    .get("content")
                    .and_then(Value::as_array)
                    .and_then(|blocks| blocks.first())
                    .and_then(|block| block.get("text"))
                    .and_then(Value::as_str)
                    .unwrap_or("{}");
                serde_json::from_str(text).unwrap_or_else(|error| panic!("`{tool}` answered text: {error}: {text}"))
            }
        };
        Answer { body, failed }
    }

    /// The cut list: the table a workshop orders from, which is what a person checks a build against.
    fn parts(&mut self) -> Value {
        let answer = self.call("query", json!({"kind": "parts"}));
        assert!(!answer.failed, "the cut list answers: {}", answer.body);
        answer.body["answer"].clone()
    }

    /// The revision the document holds, as the last answer said it.
    fn revision(&mut self) -> u64 {
        let answer = self.call("query", json!({"kind": "tree"}));
        assert!(!answer.failed, "the tree answers: {}", answer.body);
        answer.body["revision"].as_u64().unwrap_or_default()
    }

    /// Every command of one recipe, as the steps of one batch.
    fn steps(recipe: &str) -> Vec<Value> {
        let text = std::fs::read_to_string(fixture(recipe)).expect("the recipe reads");
        text.lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).expect("a recipe line is one step"))
            .collect()
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// What one turn of a scripted model did, as a person reading after it can see.
#[derive(Debug)]
struct Turn {
    /// The cut list the document held before the first call.
    before: Value,
    /// The revision before the first call.
    revision: u64,
    /// How many calls could have changed the document, whether or not they did.
    mutating: usize,
    /// The refusals the turn heard, in order.
    refusals: Vec<String>,
}

/// Whether a tool call can change the document.
fn changes_document(tool: &str, arguments: &Value) -> bool {
    match tool {
        "command_run" | "command_batch" => !arguments.get("dry_run").and_then(Value::as_bool).unwrap_or(false),
        "undo" => true,
        _ => false,
    }
}

/// Plays one turn: the calls in order, counting the ones that could change the document.
fn play(client: &mut Client, calls: &[(&str, Value)]) -> Turn {
    let before = client.parts();
    let revision = client.revision();
    let mut mutating = 0;
    let mut refusals = Vec::new();
    for (tool, arguments) in calls {
        if changes_document(tool, arguments) {
            mutating += 1;
        }
        let answer = client.call(tool, arguments.clone());
        if answer.failed {
            let sentence = answer.body.get("error").and_then(Value::as_str).map(str::to_owned).unwrap_or_else(|| {
                answer.body.get("refusal").map(ToString::to_string).unwrap_or_else(|| answer.body.to_string())
            });
            refusals.push(sentence);
        }
    }
    Turn { before, revision, mutating, refusals }
}

/// The four things ADR 0193 says every scenario asserts about a turn.
///
/// A turn that changed nothing leaves the document where it was and has nothing to take back; a turn
/// that changed something is exactly one undo entry, so one `undo` returns the cut list the turn
/// started from and leaves the entries before it in place — `earlier` says how many turns landed
/// before this one, which is what `can_undo` has to answer with.
fn assert_one_entry(client: &mut Client, turn: &Turn, earlier: usize) {
    let landed = client.revision();
    if landed == turn.revision {
        assert_eq!(client.parts(), turn.before, "a turn that wrote nothing leaves the cut list alone");
        return;
    }
    assert!(turn.mutating > 0, "the document moved without a call that could move it");
    let undone = client.call("undo", json!({}));
    assert!(!undone.failed, "the turn is undoable: {}", undone.body);
    assert_eq!(
        undone.body["can_undo"],
        json!(earlier > 0),
        "the turn was exactly one undo entry, and {earlier} came before it: {}",
        undone.body
    );
    assert_eq!(client.parts(), turn.before, "one undo restores the cut list the turn started from");
}

/// Scenario 1 of ADR 0193: build a cabinet 800 × 2200 × 560 from a recipe, in one turn.
#[test]
fn a_cabinet_is_built_from_one_recipe_in_one_turn() {
    let workshop = Workshop::new("build", "wardrobe-start.z3l");
    let mut client = Client::start(&workshop);

    let steps = Client::steps("wardrobe-800x460x2200.jsonl");
    assert_eq!(steps.len(), 38, "the recipe is the 38 commands the fixture records");
    let batch = json!({"steps": steps, "save": true});
    let turn = play(&mut client, &[("command_batch", batch)]);
    assert!(turn.refusals.is_empty(), "no step refused: {:?}", turn.refusals);
    assert_eq!(turn.mutating, 1, "a build is one call");

    let parts = client.parts();
    let rows = parts.as_array().expect("the cut list is a table");
    assert_eq!(rows.len(), 14, "the wardrobe is fourteen panels: {parts}");
    // The sizes the recipe's own README records, part by part, read back through the tool surface.
    let expected = [
        (800_000_u64, 438_000_u64, 18_000_u64),
        (2_114_000, 438_000, 18_000),
        (2_114_000, 438_000, 18_000),
        (800_000, 438_000, 18_000),
        (1_736_000, 438_000, 18_000),
        (764_000, 438_000, 18_000),
        (373_000, 438_000, 18_000),
        (373_000, 438_000, 18_000),
        (373_000, 438_000, 18_000),
        (373_000, 438_000, 18_000),
        (373_000, 438_000, 18_000),
        (2_150_000, 800_000, 4_000),
        (2_150_000, 399_000, 18_000),
        (2_150_000, 399_000, 18_000),
    ];
    for (row, (length, width, thickness)) in rows.iter().zip(expected) {
        assert_eq!(row["length_um"], json!(length), "the length of part {}: {row}", row["node_id"]);
        assert_eq!(row["width_um"], json!(width), "the width of part {}: {row}", row["node_id"]);
        assert_eq!(row["thickness_um"], json!(thickness), "the thickness of part {}: {row}", row["node_id"]);
    }

    assert_one_entry(&mut client, &turn, 0);
}

/// Scenario 2 of ADR 0193: check the piece, and act on what a refusal says.
#[test]
fn a_cabinet_is_checked_and_a_refusal_names_what_to_send() {
    let workshop = Workshop::new("check", "wardrobe-start.z3l");
    let mut client = Client::start(&workshop);

    let build = json!({"steps": Client::steps("wardrobe-800x460x2200.jsonl"), "save": true});
    let built = play(&mut client, &[("command_batch", build)]);
    assert!(built.refusals.is_empty(), "the build lands: {:?}", built.refusals);
    let after_build = client.parts();

    // The check turn: the rules' list and the panels' own diagnostics, and it writes nothing.
    let turn = play(&mut client, &[("inspect", json!({})), ("query", json!({"kind": "diagnostics"}))]);
    assert!(turn.refusals.is_empty(), "the check answers: {:?}", turn.refusals);
    assert_eq!((turn.mutating, client.revision()), (0, turn.revision), "a check writes nothing");
    let findings = client.call("inspect", json!({}));
    assert!(!findings.failed, "the inspection answers: {}", findings.body);
    assert_eq!(
        findings.body["findings"],
        json!([]),
        "the reference wardrobe is right: a finding here is a rule or the recipe, not the tool (J9)"
    );
    let diagnostics = client.call("query", json!({"kind": "diagnostics"}));
    assert!(!diagnostics.failed, "the diagnostics answer: {}", diagnostics.body);
    let rows = diagnostics.body["answer"].as_array().expect("one row per panel");
    assert_eq!(rows.len(), 14, "every panel is reported: {}", diagnostics.body);
    for row in rows {
        assert_eq!(row["issues"], json!([]), "panel {} carries no diagnostic: {row}", row["node_id"]);
    }
    assert_eq!(client.parts(), after_build, "the document did not move");

    // A refusal is a sentence the next call can act on: it names the field and the range.
    let refused = client.call(
        "command_run",
        json!({"command": "panel.resize", "params": {"panel": 3, "nominal_size": {"length": 0, "width": 438000}}}),
    );
    assert!(refused.failed, "a zero length is refused: {}", refused.body);
    let sentence = refused.body.to_string();
    assert!(sentence.contains("length"), "the refusal names the parameter: {sentence}");
}

/// The id of every node of a subtree, by the name the recipe gave it.
///
/// A model reads names, not ids: the tree is where a later turn finds the part a sentence names, so
/// the scenario reads it the same way and never carries an id from a fixture.
fn parts_by_name(client: &mut Client, product: u64) -> std::collections::BTreeMap<String, u64> {
    let answer = client.call("query", json!({"kind": "tree", "node": product}));
    assert!(!answer.failed, "the tree answers: {}", answer.body);
    let mut names = std::collections::BTreeMap::new();
    collect_names(&answer.body["answer"]["children"], &mut names);
    names
}

/// Walks a subtree, taking every node's name and id.
fn collect_names(children: &Value, into: &mut std::collections::BTreeMap<String, u64>) {
    for child in children.as_array().map(Vec::as_slice).unwrap_or_default() {
        if let (Some(name), Some(id)) = (child["name"].as_str(), child["id"].as_u64()) {
            into.insert(name.to_owned(), id);
        }
        collect_names(&child["children"], into);
    }
}

/// The sizes of the cut list, by node id: what a workshop would order.
fn cut_sizes(client: &mut Client) -> std::collections::BTreeMap<u64, (u64, u64, u64)> {
    let parts = client.parts();
    let mut sizes = std::collections::BTreeMap::new();
    for row in parts.as_array().map(Vec::as_slice).unwrap_or_default() {
        let number = |key: &str| row[key].as_u64().unwrap_or_else(|| panic!("`{key}` is a number: {row}"));
        sizes.insert(number("node_id"), (number("length_um"), number("width_um"), number("thickness_um")));
    }
    sizes
}

/// The rotation of a board standing on edge, as the recipe writes it: a quarter turn about Y.
fn post() -> [f64; 4] {
    let half = std::f64::consts::FRAC_1_SQRT_2;
    [0.0, -half, 0.0, half]
}
/// The rotation of a board lying against the wall, as the recipe writes it.
fn wall() -> [f64; 4] {
    [-0.5, -0.5, -0.5, 0.5]
}

/// Scenario 3 of ADR 0193: make it 900 wide — a change to existing work, composed by the model.
///
/// The extra 100 mm goes to the section the rail hangs in, so everything that spans the carcass grows,
/// everything inside the shelves' section stays, and the parts right of the divider move by the same
/// 100 mm. The order matters and is the model's own work: the frame first, so the parts may leave the
/// old box; then the left front, so the two fronts never cross; then the widths.
#[test]
fn the_cabinet_is_made_nine_hundred_wide_and_its_parts_follow() {
    let workshop = Workshop::new("widen", "wardrobe-start.z3l");
    let mut client = Client::start(&workshop);

    // The build is not what this scenario asserts (the first one does), but it is where the turn
    // starts: a document that already holds the wardrobe.
    let build = json!({"steps": Client::steps("wardrobe-800x460x2200.jsonl"), "save": true});
    let built = play(&mut client, &[("command_batch", build)]);
    assert!(built.refusals.is_empty(), "the build lands: {:?}", built.refusals);

    // The model's own turn, played from the names the tree answered with.
    let names = parts_by_name(&mut client, 2);
    let id = |name: &str| names.get(name).copied().unwrap_or_else(|| panic!("the wardrobe holds `{name}`: {names:?}"));
    let resize = |name: &str, length: u64, width: u64| {
        json!({"command": "panel.resize", "params": {
            "panel": id(name),
            "nominal_size": {"length": length, "width": width},
        }})
    };
    let place = |name: &str, translation: [f64; 3], rotation: [f64; 4]| {
        json!({"command": "node.placement.set", "params": {
            "node": id(name),
            "placement": {"translation": translation, "rotation": rotation},
        }})
    };
    let steps = json!({"steps": [
        {"command": "product.envelope.set", "params": {"node": 2, "envelope": {"x": 900000, "y": 460000, "z": 2200000}}},
        place("Дверь левая", [451.0, 442.0, 50.0], wall()),
        place("Стойка левая", [900.0, 4.0, 68.0], post()),
        place("Ножка 2", [810.0, 34.0, 0.0], [0.0, 0.0, 0.0, 1.0]),
        place("Ножка 4", [810.0, 352.0, 0.0], [0.0, 0.0, 0.0, 1.0]),
        resize("Дно опорное", 900_000, 438_000),
        resize("Крыша накладная", 900_000, 438_000),
        resize("Задняя стенка", 2_150_000, 900_000),
        resize("Антресольная полка", 864_000, 438_000),
        resize("Дверь правая", 2_150_000, 449_000),
        resize("Дверь левая", 2_150_000, 449_000),
        {"command": "object.extrude.height.set", "params": {"object": id("Штанга"), "height": 473000}},
        place("Штанга", [882.0, 210.0, 1687.5], post()),
    ]});
    let turn = play(&mut client, &[("command_batch", steps)]);
    assert!(turn.refusals.is_empty(), "every step of the widening lands: {:?}", turn.refusals);

    // The parts that depend on the width followed, part by part.
    let sizes = cut_sizes(&mut client);
    let expected = [
        ("Дно опорное", (900_000, 438_000, 18_000)),
        ("Стойка правая", (2_114_000, 438_000, 18_000)),
        ("Стойка левая", (2_114_000, 438_000, 18_000)),
        ("Крыша накладная", (900_000, 438_000, 18_000)),
        ("Перегородка", (1_736_000, 438_000, 18_000)),
        ("Антресольная полка", (864_000, 438_000, 18_000)),
        ("Полка 1", (373_000, 438_000, 18_000)),
        ("Полка 2", (373_000, 438_000, 18_000)),
        ("Полка 3", (373_000, 438_000, 18_000)),
        ("Полка 4", (373_000, 438_000, 18_000)),
        ("Полка 5", (373_000, 438_000, 18_000)),
        ("Задняя стенка", (2_150_000, 900_000, 4_000)),
        ("Дверь правая", (2_150_000, 449_000, 18_000)),
        ("Дверь левая", (2_150_000, 449_000, 18_000)),
    ];
    for (name, size) in expected {
        assert_eq!(sizes.get(&id(name)), Some(&size), "the size of `{name}`: {sizes:?}");
    }

    // And it is still a wardrobe the rules accept: nothing leaves the box, no shelf lost its fit.
    let findings = client.call("inspect", json!({}));
    assert_eq!(findings.body["findings"], json!([]), "the widened wardrobe is right: {}", findings.body);
    assert_one_entry(&mut client, &turn, 1);
}

/// The rows of the cut list that belong to one product.
fn rows_of(client: &mut Client, product: u64) -> Vec<Value> {
    let parts = client.parts();
    parts
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter(|row| row["product_id"].as_u64() == Some(product))
        .cloned()
        .collect()
}

/// The products the document holds, as `query box` reports them.
fn products(client: &mut Client) -> Vec<Value> {
    let answer = client.call("query", json!({"kind": "box"}));
    assert!(!answer.failed, "the products answer: {}", answer.body);
    answer.body["answer"].as_array().cloned().unwrap_or_default()
}

/// Scenario 4 of ADR 0193: copy it, then change the doors of the copy.
///
/// Two turns, and that is the surface's honest answer: a headless client has no message to group
/// calls in, so a batch is one entry and the second batch that edits what the first answer created is
/// another. (In the window the two calls of one message share the turn's scope and leave one entry —
/// ADR 0187, ADR 0230.) The scenario asserts the two entries and the rewind: two undos put the
/// document back to the one piece it started from.
#[test]
fn the_cabinet_is_copied_and_the_copy_wears_one_door() {
    let workshop = Workshop::new("copy", "wardrobe-start.z3l");
    let mut client = Client::start(&workshop);

    let build = json!({"steps": Client::steps("wardrobe-800x460x2200.jsonl"), "save": true});
    let built = play(&mut client, &[("command_batch", build)]);
    assert!(built.refusals.is_empty(), "the build lands: {:?}", built.refusals);
    assert_eq!(rows_of(&mut client, 2).len(), 14, "the piece is fourteen panels");

    // Turn one: copy the piece, and read what the answer says the copy is.
    let copied = play(
        &mut client,
        &[(
            "command_batch",
            json!({"steps": [
                {"command": "node.duplicate", "as": "copy", "params": {"node": 2}},
            ]}),
        )],
    );
    assert!(copied.refusals.is_empty(), "the copy lands: {:?}", copied.refusals);
    assert_eq!(client.parts().as_array().map(Vec::len), Some(28), "the copy holds the same fourteen");
    let boxes = products(&mut client);
    assert_eq!(boxes.len(), 2, "the document holds the piece twice: {boxes:?}");
    let copy = boxes
        .iter()
        .filter_map(|row| row["node_id"].as_u64())
        .find(|id| *id != 2)
        .expect("the copy is the product that is not the original");

    // Turn two: the copy wears one front instead of two, and the ids come from the tree the last
    // answer left, which is what a model reads them from.
    let names = parts_by_name(&mut client, copy);
    let door = |name: &str| names.get(name).copied().unwrap_or_else(|| panic!("the copy holds `{name}`: {names:?}"));
    let doored = play(
        &mut client,
        &[(
            "command_batch",
            json!({"steps": [
                {"command": "node.delete", "params": {"node": door("Дверь правая")}},
                {"command": "node.delete", "params": {"node": door("Дверь левая")}},
                {"command": "panel.create", "as": "front", "params": {
                    "parent": copy,
                    "name": "Дверь целая",
                    "nominal_size": {"length": 2150000, "width": 800000},
                    "material": "1-2",
                    "class": 102000,
                    "placement": {"translation": [0.0, 442.0, 50.0], "rotation": wall()},
                }},
                {"command": "panel.edge_banding.set", "params": {"panel": {"$ref": "front"}, "edge": 1, "material": "1-103", "included_in_size": true}},
                {"command": "panel.edge_banding.set", "params": {"panel": {"$ref": "front"}, "edge": 2, "material": "1-103", "included_in_size": true}},
                {"command": "panel.edge_banding.set", "params": {"panel": {"$ref": "front"}, "edge": 3, "material": "1-103", "included_in_size": true}},
                {"command": "panel.edge_banding.set", "params": {"panel": {"$ref": "front"}, "edge": 4, "material": "1-103", "included_in_size": true}},
            ]}),
        )],
    );
    assert!(doored.refusals.is_empty(), "the front lands: {:?}", doored.refusals);

    let original = rows_of(&mut client, 2);
    let edited = rows_of(&mut client, copy);
    assert_eq!(original.len(), 14, "the piece that was copied is untouched");
    assert_eq!(edited.len(), 13, "one front replaces the two: {edited:?}");
    let fronts: Vec<&Value> = edited.iter().filter(|row| row["class"] == json!("10.20")).collect();
    assert_eq!(fronts.len(), 1, "the copy wears one front: {fronts:?}");
    assert_eq!(fronts.first().and_then(|row| row["width_um"].as_u64()), Some(800_000), "and it spans the front");
    assert_eq!(original.iter().filter(|row| row["class"] == json!("10.20")).count(), 2, "the original keeps its pair");
    // The rewind, which is this scenario's assertion of the entry count: every turn is exactly one
    // entry, so each undo takes back the whole turn before it and nothing else.
    let undone = client.call("undo", json!({}));
    assert!(!undone.failed, "the front turn is one entry: {}", undone.body);
    assert_eq!(client.parts(), doored.before, "the copy wears its two doors again");
    assert_eq!(undone.body["can_undo"], json!(true), "the copy turn is still there: {}", undone.body);
    let undone = client.call("undo", json!({}));
    assert!(!undone.failed, "the copy turn is one entry too: {}", undone.body);
    assert_eq!(client.parts(), copied.before, "one piece is left, and it is fourteen panels");
    assert_eq!(undone.body["can_undo"], json!(true), "the build is still there: {}", undone.body);
    let undone = client.call("undo", json!({}));
    assert!(!undone.failed, "the build is one entry: {}", undone.body);
    assert_eq!(client.parts(), built.before, "and the scene is the one the fixture holds");
    assert_eq!(undone.body["can_undo"], json!(false), "nothing came before the build: {}", undone.body);
    assert_eq!(products(&mut client).len(), 1, "one piece is left");
}

/// One answer of `query`, as the object it carries.
fn ask(client: &mut Client, question: Value) -> Value {
    let answer = client.call("query", question.clone());
    assert!(!answer.failed, "`{question}` answers: {}", answer.body);
    answer.body["answer"].clone()
}

/// Scenario 5 of ADR 0193: put it in a niche 820 × 2140 × 580.
///
/// The niche is made as a frame of its own — an authored box that reserves the space — and the piece
/// is then put inside that box. The write refuses it, and that refusal is the scenario's finding: a
/// piece comes to rest inside a case only when it is that case's child, and no command makes the
/// wardrobe a child of the niche (`product.create` puts one in the root or a plain group, and nothing
/// reparents an authored piece). So the surface answers the person's sentence with the sentence that
/// says why, the batch takes back the niche it had made in the same turn, and the document is exactly
/// what it was. `plan/notes/J13.md` records what the scenario would need.
#[test]
fn the_cabinet_is_offered_a_niche_and_the_write_says_why_it_cannot_rest_there() {
    let workshop = Workshop::new("niche", "wardrobe-start.z3l");
    let mut client = Client::start(&workshop);

    let build = json!({"steps": Client::steps("wardrobe-800x460x2200.jsonl"), "save": true});
    let built = play(&mut client, &[("command_batch", build)]);
    assert!(built.refusals.is_empty(), "the build lands: {:?}", built.refusals);

    // The niche, as the frame that reserves 820 × 2140 × 580 beside the piece.
    let niche = json!({"command": "product.create", "as": "niche", "params": {
        "parent": 1,
        "name": "Ниша",
        "envelope": {"x": 820000, "y": 580000, "z": 2140000},
        "placement": {"translation": [910.0, 0.0, 0.0], "rotation": [0.0, 0.0, 0.0, 1.0]},
    }});
    let turn = play(&mut client, &[("command_batch", json!({"steps": [niche]}))]);
    assert!(turn.refusals.is_empty(), "the niche is made: {:?}", turn.refusals);
    let boxes = products(&mut client);
    let frame = boxes
        .iter()
        .find(|row| row["name"] == json!("Ниша"))
        .unwrap_or_else(|| panic!("the niche is a product of its own: {boxes:?}"));
    let size = &frame["size_um"];
    let face = |key: &str| size[key].as_u64().unwrap_or_default() as f64 / 1000.0;
    assert_eq!(
        (face("x"), face("y"), face("z")),
        (820.0, 580.0, 2140.0),
        "the niche reserves the space the owner asked for"
    );
    let origin: Vec<f64> =
        frame["origin_mm"].as_array().map(Vec::as_slice).unwrap_or_default().iter().filter_map(Value::as_f64).collect();

    // The empty cell inside it, measured by the probe a person's `V` tool uses.
    let cell = client.call("query", json!({"kind": "niche", "at": [1200.0, 200.0, 1000.0]}));
    assert!(!cell.failed, "the probe measures the space inside the niche: {}", cell.body);

    // The piece, moved into that space: the write says no, and names both of them.
    let piece = ask(&mut client, json!({"kind": "bounds", "node": 2}));
    let before = client.parts();
    let turn = play(
        &mut client,
        &[(
            "command_batch",
            json!({"steps": [
                {"command": "node.placement.set", "params": {
                    "node": 2,
                    "placement": {"translation": [920.0, 10.0, 0.0], "rotation": [0.0, 0.0, 0.0, 1.0]},
                }},
            ]}),
        )],
    );
    assert_eq!(turn.refusals.len(), 1, "the write refused: {turn:?}");
    let sentence = &turn.refusals[0];
    assert!(sentence.contains("Ниша"), "the refusal names the case it would rest in: {sentence}");
    assert!(sentence.contains("Распашной шкаф"), "and the piece it blames: {sentence}");
    assert_eq!(client.parts(), before, "and the document is exactly what it was");

    // The piece never left where it stood: its box is the one the build gave it.
    let after = ask(&mut client, json!({"kind": "bounds", "node": 2}));
    assert_eq!(after, piece, "a refused write moves nothing");
    let corner: Vec<f64> = after["world_min_mm"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(Value::as_f64)
        .collect();
    assert!(corner.iter().all(|value| value.abs() < 0.001), "the piece stands where the recipe put it: {corner:?}");
    assert!(
        after["world_max_mm"][2].as_f64().unwrap_or_default() > origin.get(2).copied().unwrap_or_default() + 2140.0,
        "and it is 2200 tall against a niche of 2140: the space is the niche's, the height is the piece's"
    );

    // The rewind, one entry per turn: the niche, and then the build.
    let undone = client.call("undo", json!({}));
    assert!(!undone.failed, "the niche is one entry: {}", undone.body);
    assert_eq!(products(&mut client).len(), 1, "the niche is gone and the piece stays");
    let undone = client.call("undo", json!({}));
    assert!(!undone.failed, "the build is one entry: {}", undone.body);
    assert_eq!(client.parts(), json!([]), "and the scene is the one the fixture holds");
    assert_eq!(undone.body["can_undo"], json!(false), "nothing came before the build: {}", undone.body);
}
