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
/// that changed something is exactly one undo entry, and one `undo` returns the cut list the turn
/// started from.
fn assert_one_entry(client: &mut Client, turn: &Turn) {
    let landed = client.revision();
    if landed == turn.revision {
        assert_eq!(client.parts(), turn.before, "a turn that wrote nothing leaves the cut list alone");
        return;
    }
    assert!(turn.mutating > 0, "the document moved without a call that could move it");
    let undone = client.call("undo", json!({}));
    assert!(!undone.failed, "the turn is undoable: {}", undone.body);
    assert_eq!(undone.body["can_undo"], json!(false), "the turn was exactly one undo entry: {}", undone.body);
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

    assert_one_entry(&mut client, &turn);
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
