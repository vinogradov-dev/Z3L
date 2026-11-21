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
use mcp::{CommandServer, SceneSink, TOOL_BATCH, TOOL_LIST, TOOL_QUERY, TOOL_RUN};
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

    // What the server offers: four coarse tools, not one tool per command id.
    let tools = client.list_tools(None).await.expect("the tool list answers");
    let names: Vec<&str> = tools.tools.iter().map(|tool| tool.name.as_ref()).collect();
    assert_eq!(names, vec![TOOL_LIST, TOOL_RUN, TOOL_BATCH, TOOL_QUERY]);

    // `command_list` names every command with the schema of its parameters.
    let listed = client.call_tool(CallToolRequestParams::new(TOOL_LIST)).await.expect("the list answers");
    let listed = body(&listed);
    assert_eq!(listed["open_product"], json!(product.get()), "the session says which product is open");
    let commands = listed["commands"].as_array().expect("the list holds one row per command");
    assert!(commands.len() > 50, "the whole registry is projected: {}", commands.len());
    let create = commands.iter().find(|row| row["id"] == "panel.create").expect("panel.create is listed");
    assert_eq!(create["enabled"], json!(true), "an open product is what a part waits for");
    assert_eq!(create["creates"], json!("part"));
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
async fn a_command_that_cannot_run_says_why_before_it_is_called() {
    // Two products and none open: the rule one product opens itself cannot be a guess.
    let (session, _products) = session_with(2);
    let server = CommandServer::new(Arc::new(Mutex::new(session)), None);
    let (mut client, task) = connect(server).await;

    let listed = client.call_tool(CallToolRequestParams::new(TOOL_LIST)).await.expect("the list answers");
    let listed = body(&listed);
    assert_eq!(listed["open_product"], Value::Null, "no product is open");
    let commands = listed["commands"].as_array().expect("one row per command");
    let create = commands.iter().find(|row| row["id"] == "panel.create").expect("panel.create is listed");
    assert_eq!(create["enabled"], json!(false), "a part is born only inside an open product");
    assert_eq!(
        create["reason"],
        json!("Create a product first."),
        "the sentence is the command's own, from the message table"
    );
    // A scene object is not gated, so it answers that it can run.
    let primitive =
        commands.iter().find(|row| row["id"] == "object.primitive.create").expect("the primitive is listed");
    assert_eq!(primitive["enabled"], json!(true), "a primitive belongs to the scene, not to a product");

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
async fn a_scene_with_two_products_and_none_named_refuses_in_the_commands_words() {
    // The same sentence a person reads from `z3l run` on such a scene: the batch does not guess
    // between two products, and the first step that needs one says what to do.
    let (session, products) = session_with(2);
    let first = *products.first().expect("the fixture makes two products");
    let session = Arc::new(Mutex::new(session));
    let (mut client, task) = connect(CommandServer::new(Arc::clone(&session), None)).await;

    let steps = json!({"steps": [{"command": "panel.create", "params": panel(first, "Side")}]});
    let answered = client
        .call_tool(CallToolRequestParams::new(TOOL_BATCH).with_arguments(arguments(steps)))
        .await
        .expect("the refusal is an answer");
    assert!(answered.is_error.unwrap_or(false), "nothing landed, so the call failed");
    let refused = body(&answered);
    assert_eq!(refused["effect"], json!("rolled_back"), "{refused}");
    assert_eq!(refused["open_product"], Value::Null, "no product was opened by guessing: {refused}");
    assert_eq!(
        refused["refusal"]["error"],
        json!("`panel.create` cannot create furniture here: Create a product first."),
        "{refused}"
    );

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
