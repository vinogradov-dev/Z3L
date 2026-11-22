//! The served session, driven over a pipe by a client this tree did not write.
//!
//! The client is `rmcp`'s own, wired to the server through `tokio::io::duplex`: the pair is the
//! shape of the process's standard streams, so what the test drives is what a foreign client drives
//! — a newline-delimited JSON handshake, then `tools/list` and `tools/call` — without a second
//! process and without a window.
//!
//! What is asserted is the promise ADR 0185 makes for the whole feature: a call lists the commands
//! this build dispatches, a batch is one undo entry and one write, a refusal writes nothing, and a
//! command that cannot run says why in its own words.

#![allow(clippy::expect_used)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use engine::Session;
use mcp::{CommandServer, SceneSink, TOOL_BATCH, TOOL_FINDINGS, TOOL_LIST, TOOL_QUERY, TOOL_RUN};
use model::{ClassCode, Envelope, Id, Placement, Project};
use rmcp::ServiceExt;
use rmcp::model::CallToolRequestParams;
use serde_json::{Map, Value, json};
use testkit::{millimetres, sample_catalog, sample_project};

/// A sink that counts its writes and remembers nothing else.
///
/// The write itself belongs to the shell: the tool server only learns that one happened, and the
/// scene file's own atomic write and preserved sections stay in `app` (ADR 0185).
#[derive(Debug, Default)]
struct CountingSink {
    writes: AtomicUsize,
}

impl CountingSink {
    fn writes(&self) -> usize {
        self.writes.load(Ordering::SeqCst)
    }
}

impl SceneSink for CountingSink {
    fn save(&self, _project: &Project) -> Result<(), String> {
        self.writes.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

/// A session holding `products` products, and their ids.
fn session_with(products: usize) -> (Session, Vec<Id>) {
    let mut project = sample_project();
    let root = project.root().id();
    let mut ids = Vec::new();
    for index in 0..products {
        let name = format!("Piece {}", index.saturating_add(1));
        let envelope = Envelope::new(millimetres(600), millimetres(560), millimetres(720)).expect("a valid box");
        let id = project
            .add_product(root, &name, ClassCode::GENERAL_PRODUCT, envelope, Placement::IDENTITY)
            .expect("a product beside the sample nodes");
        ids.push(id);
    }
    (Session::with_directory(project, sample_catalog()), ids)
}

/// The parameters of one panel created under `parent`, as the command's schema describes them.
fn panel(parent: Id, name: &str) -> Value {
    json!({
        "parent": parent.get(),
        "name": name,
        "nominal_size": {"length": 700_000, "width": 500_000},
        "material": "1-1",
        "placement": {"translation": [0, 0, 0], "rotation": [0, 0, 0, 1]},
    })
}

/// One JSON object as the protocol hands arguments over.
fn arguments(value: Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}

/// The structured answer of a tool call.
fn body(result: &rmcp::model::CallToolResult) -> Value {
    result.structured_content.clone().unwrap_or(Value::Null)
}

/// Serves `server` over a pipe and hands back a client wired to it.
async fn connect(
    server: CommandServer,
) -> (rmcp::service::RunningService<rmcp::RoleClient, ()>, tokio::task::JoinHandle<Result<(), mcp::ServeError>>) {
    let (server_end, client_end) = tokio::io::duplex(64 * 1024);
    let (reader, writer) = tokio::io::split(server_end);
    let task = tokio::spawn(mcp::serve(server, reader, writer));
    let client = ().serve(client_end).await.expect("the client and the server agree to begin");
    (client, task)
}

#[tokio::test]
async fn a_foreign_client_lists_the_commands_and_batches_two_steps_into_one_undo_entry() {
    let (session, products) = session_with(1);
    let product = *products.first().expect("one product");
    let mut session = session;
    assert!(session.open_product(product), "the fixture opens the one product");
    let session = Arc::new(Mutex::new(session));
    let sink = Arc::new(CountingSink::default());
    let (mut client, task) =
        connect(CommandServer::new(Arc::clone(&session), Some(Arc::clone(&sink) as Arc<dyn SceneSink>))).await;

    // What the server offers: five coarse tools, not one tool per command id.
    let tools = client.list_tools(None).await.expect("the tool list answers");
    let names: Vec<&str> = tools.tools.iter().map(|tool| tool.name.as_ref()).collect();
    assert_eq!(names, vec![TOOL_LIST, TOOL_RUN, TOOL_BATCH, TOOL_QUERY, TOOL_FINDINGS]);

    // `command_list` names every command with the schema of its parameters.
    let listed = client.call_tool(CallToolRequestParams::new(TOOL_LIST)).await.expect("the list answers");
    let listed = body(&listed);
    assert_eq!(listed["open_product"], json!(product.get()), "the session says which product is open");
    let commands = listed["commands"].as_array().expect("the list holds one row per command");
    assert!(commands.len() > 50, "the whole registry is projected: {}", commands.len());
    let create = commands.iter().find(|row| row["id"] == "panel.create").expect("panel.create is listed");
    assert_eq!(create["creates"], json!("part"), "a part is what the command builds");
    assert!(create.get("enabled").is_none(), "a command writes where its parameters name, so no session column");
    assert_eq!(create["params"]["additionalProperties"], json!(false), "the schema is the parser's");

    // Two steps in one call: one undo entry, one write.
    let steps = json!({"steps": [
        {"command": "panel.create", "params": panel(product, "Side")},
        {"command": "panel.create", "params": panel(product, "Shelf")},
    ]});
    let answered = client
        .call_tool(CallToolRequestParams::new(TOOL_BATCH).with_arguments(arguments(steps)))
        .await
        .expect("the batch answers");
    let report = body(&answered);
    assert_eq!(report["effect"], json!("landed"), "{report}");
    assert_eq!(report["written"], json!(true), "{report}");
    assert_eq!(report["created"].as_array().map(Vec::len), Some(2), "{report}");
    assert_eq!(sink.writes(), 1, "the landed batch was written once");

    // The scene is one tree: an answer from `query` sees what the batch built.
    let asked = client
        .call_tool(CallToolRequestParams::new(TOOL_QUERY).with_arguments(arguments(json!({"kind": "tree"}))))
        .await
        .expect("the query answers");
    let tree = body(&asked);
    assert_eq!(tree["revision"], report["revision"], "the read model reads the same document");

    // `inspect` is the rules' report: one list, and each finding names the nodes it blames.
    let inspected = client.call_tool(CallToolRequestParams::new(TOOL_FINDINGS)).await.expect("the inspection answers");
    let inspection = body(&inspected);
    assert_eq!(inspection["revision"], report["revision"], "the inspection reads the same document");
    let findings = inspection["findings"].as_array().expect("the report holds a list");
    assert!(!findings.is_empty(), "the two panels leave the product's envelope: {inspection}");
    for finding in findings {
        assert!(finding["code"].is_string() && finding["severity"].is_string(), "{finding}");
        assert!(finding["nodes"].as_array().is_some_and(|nodes| !nodes.is_empty()), "{finding}");
        assert!(finding["text"].as_str().is_some_and(|text| !text.trim().is_empty()), "{finding}");
        assert!(finding["suggested_commands"].is_array(), "the command list is always present: {finding}");
    }

    // The history the tools moved: one entry for the batch, and `command_run` adds one of its own.
    let renamed = client
        .call_tool(CallToolRequestParams::new(TOOL_RUN).with_arguments(arguments(json!({
            "command": "node.rename",
            "params": {"node": product.get(), "name": "Wardrobe"},
        }))))
        .await
        .expect("a one-step run answers");
    assert_eq!(body(&renamed)["effect"], json!("landed"));
    assert_eq!(sink.writes(), 2, "the run landed and was written");
    {
        let mut session = session.lock().expect("the server did not poison the session");
        assert_eq!(session.project().container(product).map(model::Container::name), Some("Wardrobe"));
        session.undo().expect("the run is one entry");
        session.undo().expect("the batch is one entry");
        assert!(!session.can_undo(), "the two calls left exactly two entries");
        assert_eq!(session.project().container(product).map(|container| container.children().len()), Some(0));
    }

    let _ = client.close().await;
    let _ = task.await.expect("the server task never panics");
}

#[tokio::test]
async fn a_client_names_what_one_step_makes_and_the_next_step_refers_to_it() {
    // J18: a build is one call and one undo entry, so a step that bands a panel names the panel it
    // creates and the band step refers to the name instead of an id only the answer knows.
    let (mut session, products) = session_with(1);
    let product = *products.first().expect("one product");
    assert!(session.open_product(product), "the fixture opens the one product");
    let session = Arc::new(Mutex::new(session));
    let sink = Arc::new(CountingSink::default());
    let (mut client, task) =
        connect(CommandServer::new(Arc::clone(&session), Some(Arc::clone(&sink) as Arc<dyn SceneSink>))).await;
    let edge = testkit::sample_edge_material_id().to_string();

    let steps = json!({"steps": [
        {"command": "panel.create", "as": "side", "params": panel(product, "Side")},
        {"command": "panel.edge_banding.set", "params": {
            "panel": {"$ref": "side"},
            "edge": 1,
            "material": edge,
            "included_in_size": true,
        }},
    ]});
    let answered = client
        .call_tool(CallToolRequestParams::new(TOOL_BATCH).with_arguments(arguments(steps)))
        .await
        .expect("the batch answers");
    let report = body(&answered);
    assert_eq!(report["effect"], json!("landed"), "{report}");
    assert_eq!(report["written"], json!(true), "{report}");
    assert_eq!(report["steps"][0]["as"], json!("side"), "the answer says which id the name stands for");
    assert_eq!(report["steps"][1]["ok"], json!(true), "the reference resolved: {report}");
    assert_eq!(report["created"].as_array().map(Vec::len), Some(1), "one part, not two: {report}");
    assert_eq!(sink.writes(), 1, "one call, one write");

    let created = report["created"][0].as_u64().expect("the created id");
    let panel = Id::new(created).expect("a document id");
    {
        let mut session = session.lock().expect("the server did not poison the session");
        let stored = session.project().panel(panel).expect("the panel is in the document");
        assert_eq!(stored.edge_banding().len(), 1, "the band landed on the panel the name meant");
        session.undo().expect("the build is one entry");
        assert!(!session.can_undo(), "one call, one undo entry");
        assert!(session.project().panel(panel).is_none(), "and the one entry takes the whole build back");
    }

    let _ = client.close().await;
    let _ = task.await.expect("the server task never panics");
}

#[tokio::test]
async fn a_part_is_written_in_the_product_the_request_names_even_when_none_is_open() {
    // Two products and none open: a caller that is sure which piece it builds in names it, and the
    // open product is the window's own question rather than the command's (ADR 0098, ADR 0226).
    let (session, products) = session_with(2);
    let first = *products.first().expect("two products");
    let server = CommandServer::new(Arc::new(Mutex::new(session)), None);
    let (mut client, task) = connect(server).await;

    let listed = client.call_tool(CallToolRequestParams::new(TOOL_LIST)).await.expect("the list answers");
    let listed = body(&listed);
    assert_eq!(listed["open_product"], Value::Null, "no product is open");
    let commands = listed["commands"].as_array().expect("one row per command");
    let create = commands.iter().find(|row| row["id"] == "panel.create").expect("panel.create is listed");
    assert!(create.get("enabled").is_none(), "the list judges no request, so it carries no verdict");

    let steps = json!({"steps": [{"command": "panel.create", "params": panel(first, "Side")}]});
    let answered = client
        .call_tool(CallToolRequestParams::new(TOOL_BATCH).with_arguments(arguments(steps)))
        .await
        .expect("the batch answers");
    let report = body(&answered);
    assert_eq!(report["effect"], json!("landed"), "{report}");
    let created = report["created"].as_array().and_then(|ids| ids.first()).and_then(Value::as_u64);
    assert!(created.is_some(), "the part is named in the answer: {report}");

    let _ = client.close().await;
    let _ = task.await.expect("the server task never panics");
}

#[tokio::test]
async fn a_session_served_without_a_scene_changes_and_writes_nothing_to_a_file() {
    // `z3l mcp serve` without a scene is a document that lives in memory: the answer says so on
    // every landed batch instead of claiming a save that has no file behind it.
    let (session, products) = session_with(1);
    let product = *products.first().expect("one product");
    let mut session = session;
    assert!(session.open_product(product));
    let session = Arc::new(Mutex::new(session));
    let (mut client, task) = connect(CommandServer::new(Arc::clone(&session), None)).await;

    let run = json!({"command": "node.rename", "params": {"node": product.get(), "name": "Wardrobe"}});
    let answered = client
        .call_tool(CallToolRequestParams::new(TOOL_RUN).with_arguments(arguments(run)))
        .await
        .expect("the run answers");
    let report = body(&answered);
    assert_eq!(report["effect"], json!("landed"), "{report}");
    assert_eq!(report["write"], json!("no_file"), "there is no scene to write");
    assert_eq!(report["written"], json!(false));
    assert!(!answered.is_error.unwrap_or(true), "nothing failed: the document changed as asked");
    {
        let session = session.lock().expect("the server did not poison the session");
        assert!(session.can_undo(), "the change is one undo entry in the served document");
    }

    let _ = client.close().await;
    let _ = task.await.expect("the server task never panics");
}

#[tokio::test]
async fn a_refused_batch_takes_back_its_steps_and_writes_nothing() {
    let (session, products) = session_with(1);
    let product = *products.first().expect("one product");
    let mut session = session;
    assert!(session.open_product(product));
    let session = Arc::new(Mutex::new(session));
    let sink = Arc::new(CountingSink::default());
    let (mut client, task) =
        connect(CommandServer::new(Arc::clone(&session), Some(Arc::clone(&sink) as Arc<dyn SceneSink>))).await;

    let missing = Id::new(9_999).expect("an id the scene does not hold");
    let steps = json!({"steps": [
        {"command": "panel.create", "params": panel(product, "Side")},
        {"command": "panel.create", "params": panel(missing, "Ghost")},
    ]});
    let answered = client
        .call_tool(CallToolRequestParams::new(TOOL_BATCH).with_arguments(arguments(steps)))
        .await
        .expect("the refusal is an answer, not a broken session");
    assert!(answered.is_error.unwrap_or(false), "a refused batch is a failed call the caller sees");
    let refused = body(&answered);
    assert_eq!(refused["effect"], json!("rolled_back"), "{refused}");
    assert_eq!(refused["written"], json!(false), "{refused}");
    assert_eq!(refused["refusal"]["step"], json!(2), "the second step is the one that refused");
    assert_eq!(refused["refusal"]["command"], json!("panel.create"));
    assert!(
        refused["refusal"]["error"].as_str().is_some_and(|sentence| !sentence.trim().is_empty()),
        "the command's own sentence is what a model can act on: {refused}"
    );
    assert_eq!(sink.writes(), 0, "a batch that did not land is not written");
    {
        let session = session.lock().expect("the server did not poison the session");
        assert!(!session.can_undo(), "a rolled-back batch leaves no history");
        assert_eq!(session.project().container(product).map(|container| container.children().len()), Some(0));
    }

    let _ = client.close().await;
    let _ = task.await.expect("the server task never panics");
}

#[tokio::test]
async fn a_session_in_plan_mode_answers_every_call_with_a_plan_and_writes_nothing() {
    // ADR 0190: the mode belongs to the session and the person, so a client cannot talk it into
    // writing — the same batch answers a plan here and a landed change once the switch is build.
    let (session, products) = session_with(1);
    let product = *products.first().expect("one product");
    let mut session = session;
    assert!(session.open_product(product));
    session.set_agent_mode(engine::AgentMode::Plan);
    let session = Arc::new(Mutex::new(session));
    let sink = Arc::new(CountingSink::default());
    let (mut client, task) =
        connect(CommandServer::new(Arc::clone(&session), Some(Arc::clone(&sink) as Arc<dyn SceneSink>))).await;

    let listed = client.call_tool(CallToolRequestParams::new(TOOL_LIST)).await.expect("the list answers");
    assert_eq!(body(&listed)["mode"], json!("plan"), "the list says which mode the session answers in");

    let steps = json!({"steps": [{"command": "panel.create", "params": panel(product, "Side")}]});
    let answered = client
        .call_tool(CallToolRequestParams::new(TOOL_BATCH).with_arguments(arguments(steps.clone())))
        .await
        .expect("a plan is an answer");
    let plan = body(&answered);
    assert_eq!(plan["effect"], json!("planned"), "{plan}");
    assert_eq!(plan["mode"], json!("plan"), "{plan}");
    assert_eq!(plan["created"].as_array().map(Vec::len), Some(1), "the plan names what would be created");
    assert_eq!(plan["written"], json!(false), "a plan writes no file");
    // X106: a caller that reads only the answer learns that the switch is a person's and where the
    // key is, so a refused write is told to the person instead of being retried by the model.
    let note = plan["mode_note"].as_str().unwrap_or_else(|| panic!("a plan carries its note: {plan}"));
    assert!(note.contains("Ctrl+J"), "the note names the key that opens the panel: {note}");
    assert!(note.contains("a person"), "and says the switch is a person's, not a tool's: {note}");
    assert!(note.contains(mcp::PLAN_NOTE), "the note is the published sentence: {note}");
    assert_eq!(sink.writes(), 0);
    {
        let session = session.lock().expect("the server did not poison the session");
        assert!(!session.can_undo(), "a plan is not history");
        assert_eq!(session.project().container(product).map(|container| container.children().len()), Some(0));
    }

    // The person's switch is the session's: the same call lands once it is build.
    session.lock().expect("the session").set_agent_mode(engine::AgentMode::Build);
    let answered = client
        .call_tool(CallToolRequestParams::new(TOOL_BATCH).with_arguments(arguments(steps)))
        .await
        .expect("the batch answers");
    let report = body(&answered);
    assert_eq!(report["effect"], json!("landed"), "{report}");
    assert_eq!(report["mode"], json!("build"));
    assert_eq!(report.get("mode_note"), None, "a landed batch needs no sentence about a switch: {report}");
    assert_eq!(sink.writes(), 1);
    {
        let session = session.lock().expect("the server did not poison the session");
        assert!(session.can_undo(), "the landing batch is one entry");
        assert_eq!(session.project().container(product).map(|container| container.children().len()), Some(1));
    }

    let _ = client.close().await;
    let _ = task.await.expect("the server task never panics");
}

#[tokio::test]
async fn a_batch_names_the_product_it_works_in() {
    // A caller that is not the window has no double click to point at (ADR 0098), so the batch names
    // the product and the answer says which one the steps ran in (ADR 0212).
    let (session, products) = session_with(2);
    let second = *products.get(1).expect("the fixture makes two products");
    let first = *products.first().expect("the fixture makes two products");
    let before = session.project().revision().get();
    let session = Arc::new(Mutex::new(session));
    let (mut client, task) = connect(CommandServer::new(Arc::clone(&session), None)).await;

    let ghost = Id::new(9_999).expect("an id the scene does not hold");
    let unknown =
        json!({"product": ghost.get(), "steps": [{"command": "panel.create", "params": panel(second, "Side")}]});
    let refused = client
        .call_tool(CallToolRequestParams::new(TOOL_BATCH).with_arguments(arguments(unknown)))
        .await
        .expect("an id that is not a product is an answer");
    assert!(refused.is_error.unwrap_or(false), "the call refuses before it dispatches anything");
    let refused = body(&refused);
    assert!(refused["error"].as_str().is_some_and(|sentence| sentence.contains("no product has id 9999")), "{refused}");
    {
        let session = session.lock().expect("the server did not poison the session");
        assert_eq!(session.project().revision().get(), before, "a refused open writes nothing");
        assert_eq!(session.active_product(), None, "and opens nothing");
    }

    let named =
        json!({"product": second.get(), "steps": [{"command": "panel.create", "params": panel(second, "Side")}]});
    let answered = client
        .call_tool(CallToolRequestParams::new(TOOL_BATCH).with_arguments(arguments(named)))
        .await
        .expect("the batch answers");
    let report = body(&answered);
    assert_eq!(report["effect"], json!("landed"), "{report}");
    assert_eq!(report["open_product"], json!(second.get()), "the answer names the product it ran in: {report}");
    assert_eq!(report["created"].as_array().map(Vec::len), Some(1), "one panel landed: {report}");
    {
        let mut session = session.lock().expect("the server did not poison the session");
        assert_eq!(session.active_product(), Some(second), "the server opened the product it was told to");
        assert_eq!(session.project().container(second).map(|container| container.children().len()), Some(1));
        assert_eq!(
            session.project().container(first).map(|container| container.children().len()),
            Some(0),
            "the other product is untouched"
        );
        session.undo().expect("the batch is one entry");
        assert_eq!(session.project().container(second).map(|container| container.children().len()), Some(0));
        assert!(!session.can_undo(), "opening left no entry of its own");
        assert_eq!(session.active_product(), Some(second), "an undo takes back the panel, not the product");
    }

    // The list agrees with the batch: one answer about which product is open.
    let listed = client.call_tool(CallToolRequestParams::new(TOOL_LIST)).await.expect("the list answers");
    assert_eq!(body(&listed)["open_product"], json!(second.get()));

    let _ = client.close().await;
    let _ = task.await.expect("the server task never panics");
}

#[tokio::test]
async fn naming_a_product_opens_it_with_no_revision_and_no_undo_entry() {
    // Opening is session view state (ADR 0098): a batch whose steps all refuse still opened the
    // product it was told to, and neither the revision nor the history moved for it (ADR 0212).
    let (session, products) = session_with(2);
    let second = *products.get(1).expect("the fixture makes two products");
    let before = session.project().revision().get();
    let session = Arc::new(Mutex::new(session));
    let (mut client, task) = connect(CommandServer::new(Arc::clone(&session), None)).await;

    let missing = Id::new(9_999).expect("an id the scene does not hold");
    let named = json!({
        "product": second.get(),
        "steps": [{"command": "node.rename", "params": {"node": missing.get(), "name": "Ghost"}}],
    });
    let answered = client
        .call_tool(CallToolRequestParams::new(TOOL_BATCH).with_arguments(arguments(named)))
        .await
        .expect("the refusal is an answer");
    assert!(answered.is_error.unwrap_or(false), "the step refused, so the call failed");
    let refused = body(&answered);
    assert_eq!(refused["effect"], json!("rolled_back"), "{refused}");
    assert_eq!(refused["revision"], json!(before), "opening writes no revision: {refused}");
    assert_eq!(refused["open_product"], json!(second.get()), "the product is open: {refused}");
    {
        let session = session.lock().expect("the server did not poison the session");
        assert_eq!(session.project().revision().get(), before, "the document did not move");
        assert!(!session.can_undo(), "opening is not an undo entry, and the refusal is none either");
        assert_eq!(session.active_product(), Some(second));
        assert_eq!(session.agent_mode(), engine::AgentMode::Build, "the mode is untouched");
    }

    let _ = client.close().await;
    let _ = task.await.expect("the server task never panics");
}

#[tokio::test]
async fn a_frame_a_call_creates_is_open_and_the_first_part_needs_no_second_step() {
    // X112: ADR 0100 words a frame as created, opened and drawn in one act, and a caller that is
    // not the window had no second act to take, so the part after it was refused.
    let (session, _products) = session_with(0);
    let root = session.project().root().id();
    let session = Arc::new(Mutex::new(session));
    let (mut client, task) = connect(CommandServer::new(Arc::clone(&session), None)).await;

    let frame = json!({"command": "product.create", "params": {
        "parent": root.get(),
        "name": "Wardrobe",
        "envelope": {"x": 800_000, "y": 460_000, "z": 2_200_000},
        "placement": {"translation": [0, 0, 0], "rotation": [0, 0, 0, 1]},
    }});
    let answered = client
        .call_tool(CallToolRequestParams::new(TOOL_RUN).with_arguments(arguments(frame)))
        .await
        .expect("the frame is created");
    let report = body(&answered);
    assert_eq!(report["effect"], json!("landed"), "{report}");
    let product = report["created"][0].as_u64().expect("the answer names the frame it made");
    assert_eq!(report["open_product"], json!(product), "the frame the call made is the open one: {report}");

    // The first part lands in it with no other step: the caller names the piece, and the piece is
    // the one the answer said was open.
    let product = Id::new(product).expect("a document id");
    let steps = json!({"steps": [{"command": "panel.create", "params": panel(product, "Side")}]});
    let answered = client
        .call_tool(CallToolRequestParams::new(TOOL_BATCH).with_arguments(arguments(steps)))
        .await
        .expect("the part lands in the piece the call made");
    let report = body(&answered);
    assert_eq!(report["effect"], json!("landed"), "{report}");
    assert_eq!(report["open_product"], json!(product.get()), "the piece stays open while it is built: {report}");
    assert_eq!(report["created"].as_array().map(Vec::len), Some(1), "one part landed: {report}");
    {
        let session = session.lock().expect("the server did not poison the session");
        assert_eq!(session.active_product(), Some(product), "the session works in the frame the call made");
        assert_eq!(session.project().container(product).map(|container| container.children().len()), Some(1));
    }

    let _ = client.close().await;
    let _ = task.await.expect("the server task never panics");
}

#[tokio::test]
async fn a_client_reads_back_the_box_of_the_body_it_just_built() {
    // X110: a caller has no screen, so the box of what it built is a question it has to be able to
    // ask. The batch builds a 200 x 300 x 600 body and the next call measures it.
    let (mut session, products) = session_with(1);
    let product = *products.first().expect("one product");
    assert!(session.open_product(product), "the fixture opens the one product");
    let session = Arc::new(Mutex::new(session));
    let (mut client, task) = connect(CommandServer::new(Arc::clone(&session), None)).await;

    // A cuboid anchors at the centre of its base face, so its local box is 100 mm each way in X and
    // Y and 600 mm up: the answer a caller has to be able to check.
    let run = json!({"command": "object.cuboid.create", "params": {
        "parent": product.get(),
        "name": "Body",
        "width": 200_000,
        "depth": 300_000,
        "height": 600_000,
        "placement": {"translation": [0, 0, 0], "rotation": [0, 0, 0, 1]},
    }});
    let built = client
        .call_tool(CallToolRequestParams::new(TOOL_RUN).with_arguments(arguments(run)))
        .await
        .expect("the body is built");
    let report = body(&built);
    assert_eq!(report["effect"], json!("landed"), "{report}");
    let created = report["created"][0].as_u64().expect("the answer names the node it made");

    let asked = client
        .call_tool(CallToolRequestParams::new(TOOL_QUERY).with_arguments(arguments(json!({
            "kind": "bounds",
            "node": created,
        }))))
        .await
        .expect("the box is answered");
    let answer = body(&asked);
    assert_eq!(answer["answer"]["node_id"], json!(created), "{answer}");
    assert_eq!(answer["answer"]["kind"], json!("object"));
    assert_eq!(answer["answer"]["local_size_mm"], json!([200.0, 300.0, 600.0]), "{answer}");
    assert_eq!(answer["answer"]["local_min_mm"], json!([-100.0, -150.0, 0.0]), "{answer}");
    assert_eq!(answer["answer"]["local_max_mm"], json!([100.0, 150.0, 600.0]), "{answer}");

    // The product's measured box is the body it holds, which is a different number from the
    // envelope it reserves.
    let measured = client
        .call_tool(CallToolRequestParams::new(TOOL_QUERY).with_arguments(arguments(json!({
            "kind": "bounds",
            "node": product.get(),
        }))))
        .await
        .expect("the product's measured box is answered");
    assert_eq!(body(&measured)["answer"]["local_size_mm"], json!([200.0, 300.0, 600.0]));
    let reserved = client
        .call_tool(CallToolRequestParams::new(TOOL_QUERY).with_arguments(arguments(json!({
            "kind": "box",
            "node": product.get(),
        }))))
        .await
        .expect("the reserved box is answered");
    assert_eq!(body(&reserved)["answer"][0]["size_um"]["x"], json!(600_000), "`box` still answers the envelope");

    let _ = client.close().await;
    let _ = task.await.expect("the server task never panics");
}

/// The shelf the description asks for: 500 wide, 500 deep, in the sample catalogue's board.
const SHELF_UM: [u64; 2] = [500_000, 500_000];

/// The rod the description asks for: 30 by 30 across, 600 long.
const ROD_UM: [u64; 3] = [30_000, 30_000, 600_000];

#[tokio::test]
async fn a_caller_that_reads_only_the_surface_puts_the_shelves_where_the_person_said() {
    // J19, the acceptance test: a session with the built surface and nothing of this repository
    // written down. It reads the units, the class rows and the frame from `command_list`, the
    // piece's box from `query box`, and the shapes of the two commands from their schemas — then
    // resolves «слева пять полок, справа штанга» and writes the six parts in one batch.
    //
    // The reading of "left" is the caller's own: the surface states the frame (+X is the side a
    // person in front of the piece sees on their left) and the caller says its reading back. What is
    // asserted is that the numbers came from the answers, so a piece built at a thousand times its
    // size (X115) or on the mirrored side (X118) cannot pass this test.
    let (mut session, products) = session_with(1);
    let product = *products.first().expect("one product");
    assert!(session.open_product(product), "the fixture opens the one product");
    let session = Arc::new(Mutex::new(session));
    let (mut client, task) = connect(CommandServer::new(Arc::clone(&session), None)).await;

    // What the surface says about the vocabulary it is about to be used with.
    let listed = body(&client.call_tool(CallToolRequestParams::new(TOOL_LIST)).await.expect("the list answers"));
    let guide = &listed["domain"];
    assert!(
        guide["units"]["length"].as_str().is_some_and(|unit| unit.starts_with("micrometres")),
        "a length is micrometres: {}",
        guide["units"]["length"]
    );
    assert!(
        guide["units"]["placement"].as_str().is_some_and(|unit| unit.starts_with("millimetres")),
        "a placement is millimetres: {}",
        guide["units"]["placement"]
    );
    let shelf = guide["classes"]
        .as_array()
        .expect("classes")
        .iter()
        .flat_map(|group| group["classes"].as_array().into_iter().flatten())
        .find(|class| class["code"] == "10.11")
        .expect("the shelf row is published");
    assert_eq!(shelf["label"], json!("Horizontal"), "the word for a shelf is its class: {shelf}");
    assert_eq!(shelf["fill"], json!("w+d"), "a shelf takes the void's width and depth: {shelf}");
    let rule = guide["sides"]["rule"].as_str().unwrap_or_default();
    assert!(rule.contains("+X") && rule.contains("+Y"), "the frame says which axis the person's left is: {rule}");

    // The schema of the command says what the numbers mean, so the caller sends micrometres for the
    // size and millimetres for the placement's translation.
    let commands = listed["commands"].as_array().expect("one row per command");
    let create = commands.iter().find(|row| row["id"] == "panel.create").expect("panel.create is listed");
    // A field that names a struct is read through its definition, where the units of its own
    // numbers are written.
    let nominal = &create["params"]["$defs"]["PanelNominalSize"]["properties"];
    assert_eq!(nominal["length"]["unit"], json!("micrometres"), "the size is micrometres: {nominal}");
    assert_eq!(nominal["width"]["unit"], json!("micrometres"), "both of the size's numbers are: {nominal}");
    let placement = &create["params"]["properties"]["placement"];
    assert_eq!(placement["unit"], json!("millimetres"), "the translation is millimetres: {placement}");

    // The piece the person is talking about, as the document answers it.
    let reserved = body(
        &client
            .call_tool(CallToolRequestParams::new(TOOL_QUERY).with_arguments(arguments(json!({
                "kind": "box",
                "node": product.get(),
            }))))
            .await
            .expect("the box is answered"),
    );
    let boxed = &reserved["answer"][0];
    let width_mm = boxed["size_um"]["x"].as_f64().expect("the width") / 1_000.0;
    let depth_mm = boxed["size_um"]["y"].as_f64().expect("the depth") / 1_000.0;
    let origin = boxed["origin_mm"].as_array().expect("the origin");
    let origin_x = origin.first().and_then(Value::as_f64).expect("the x of the origin");
    let left_face = origin_x + width_mm;
    assert_eq!((width_mm, depth_mm), (600.0, 560.0), "the piece is the one the fixture made: {boxed}");

    // «Слева пять полок»: the shelf's far edge is the piece's +X face, so its near edge is a shelf's
    // length in from it, and the five of them climb the piece's height.
    let shelf_mm = SHELF_UM[0] as f64 / 1_000.0;
    let shelf_x = left_face - shelf_mm;
    let mut steps: Vec<Value> = Vec::new();
    for index in 0..5u64 {
        steps.push(json!({
            "command": "panel.create",
            "as": format!("shelf {}", index.saturating_add(1)),
            "params": {
                "parent": product.get(),
                "name": format!("Shelf {}", index.saturating_add(1)),
                "nominal_size": {"length": SHELF_UM[0], "width": SHELF_UM[1]},
                "material": "1-1",
                "placement": {
                    "translation": [shelf_x, 30.0, 20.0 + index as f64 * 140.0],
                    "rotation": [0.0, 0.0, 0.0, 1.0],
                },
            },
        }));
    }
    // «Справа штанга»: the -X face of the piece, which is the origin the box answer reports. No
    // command creates a fitting, so the rod is the body the guide says to build instead — and its
    // schema states where a body's own zero is.
    let cuboid = commands.iter().find(|row| row["id"] == "object.cuboid.create").expect("the body is listed");
    let anchor = cuboid["params"]["description"].as_str().unwrap_or_default();
    let deep = (depth_mm - ROD_UM[1] as f64 / 1_000.0) / 2.0;
    steps.push(json!({
        "command": "object.cuboid.create",
        "as": "rod",
        "params": {
            "parent": product.get(),
            "name": "Штанга",
            "width": ROD_UM[0],
            "depth": ROD_UM[1],
            "height": ROD_UM[2],
            "placement": {
                "translation": [origin_x + 10.0, deep, 60.0],
                "rotation": [0.0, 0.0, 0.0, 1.0],
            },
        },
    }));

    let answered = client
        .call_tool(CallToolRequestParams::new(TOOL_BATCH).with_arguments(arguments(json!({
            "product": product.get(),
            "steps": steps,
        }))))
        .await
        .expect("the batch answers");
    let report = body(&answered);
    assert_eq!(report["effect"], json!("landed"), "{report}");
    assert_eq!(report["steps"].as_array().map(Vec::len), Some(6), "five shelves and the rod: {report}");
    let created: Vec<u64> =
        report["created"].as_array().expect("the created ids").iter().filter_map(Value::as_u64).collect();
    assert_eq!(created.len(), 6, "{report}");

    // Every shelf is on the piece's +X side, and the rod is on its -X side: the sides the words named.
    for id in created.iter().take(5) {
        let measured = body(
            &client
                .call_tool(CallToolRequestParams::new(TOOL_QUERY).with_arguments(arguments(json!({
                    "kind": "bounds",
                    "node": id,
                }))))
                .await
                .expect("the shelf's box is answered"),
        );
        let min = measured["answer"]["world_min_mm"][0].as_f64().expect("the near edge");
        let max = measured["answer"]["world_max_mm"][0].as_f64().expect("the far edge");
        assert!((min - shelf_x).abs() < 0.001, "a shelf starts where the answer said: {measured}");
        assert!((max - left_face).abs() < 0.001, "a shelf ends on the piece's +X face: {measured}");
    }
    assert!(
        anchor.contains("minimum corner at its translation") && anchor.contains("centre of the base face"),
        "the body's placement puts its corner at the translation and its zero at the base centre: {anchor}"
    );
    let rod = body(
        &client
            .call_tool(CallToolRequestParams::new(TOOL_QUERY).with_arguments(arguments(json!({
                "kind": "bounds",
                "node": created.last(),
            }))))
            .await
            .expect("the rod's box is answered"),
    );
    let min = rod["answer"]["world_min_mm"][0].as_f64().expect("the rod's near edge");
    let max = rod["answer"]["world_max_mm"][0].as_f64().expect("the rod's far edge");
    assert!((min - origin_x - 10.0).abs() < 0.001, "the rod stands 10 mm in from the -X face: {rod}");
    let rod_mm = ROD_UM[0] as f64 / 1_000.0;
    assert!((max - origin_x - 10.0 - rod_mm).abs() < 0.001, "and is its own width thick: {rod}");

    let _ = client.close().await;
    let _ = task.await.expect("the server task never panics");
}
