//! The local control channel, driven the way an agent drives it: a loopback socket and a token.
//!
//! The client is `rmcp`'s own and reaches the window over a real `TcpStream`, so what the test drives
//! is what another process drives — the token line, `tools/list`, `tools/call` — and the host is
//! this test, answering at the moment a frame would. The four clauses of J6 are asserted here:
//! a token reaches `command_list`, a request without one never reaches the dispatcher, the token file
//! is the owner's alone and gone after a clean exit, and a second instance does not take the first
//! one's channel (ADR 0188).

#![allow(clippy::expect_used)]

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use engine::{BatchStep, Query, Session};
use mcp::channel::{Call, Channel, FILE_NAME};
use mcp::{Answer, ChannelError, Door, TOOL_INSPECT, TOOL_LIST, dispatch, listing};
use rmcp::RoleClient;
use rmcp::ServiceExt;
use rmcp::model::CallToolRequestParams;
use rmcp::service::RunningService;
use serde_json::{Value, json};
use testkit::{sample_catalog, sample_project};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// How long a step of the test may take before it counts as a failure.
const DEADLINE: Duration = Duration::from_secs(10);

/// The budget a served call answers in when the host is asleep and waiting for the bell.
///
/// It covers the socket, the ring and the answer and nothing else: a host that waited for a pointer
/// event, or for a clock, would need a person or a timer this test never provides (X111).
const ANSWER_BUDGET: Duration = Duration::from_millis(500);

/// The port and the token the channel published, as any client reads them.
fn published(directory: &Path) -> (u16, String) {
    let text = std::fs::read_to_string(directory.join(FILE_NAME)).expect("the channel publishes itself");
    let value: Value = serde_json::from_str(&text).expect("the file is a channel");
    let port = value["port"].as_u64().expect("a port") as u16;
    let token = value["token"].as_str().expect("a token").to_owned();
    (port, token)
}

/// A client that opened the socket, presented the token and began the protocol.
async fn client(directory: &Path) -> RunningService<RoleClient, ()> {
    let (port, token) = published(directory);
    let mut stream = tokio::time::timeout(DEADLINE, TcpStream::connect(("127.0.0.1", port)))
        .await
        .expect("the connect answers")
        .expect("the channel listens on loopback");
    stream.write_all(format!("{token}\n").as_bytes()).await.expect("the token is written");
    let mut accepted = [0u8; 3];
    stream.read_exact(&mut accepted).await.expect("the server answers the token");
    assert_eq!(&accepted, b"ok\n", "a good token is answered before any frame");
    let (reader, writer) = stream.into_split();
    ().serve((reader, writer)).await.expect("the protocol begins")
}

/// One session with a product open, as the window's active tab holds.
fn session() -> Session {
    let project = sample_project();
    Session::with_directory(project, sample_catalog())
}

/// Answers one call the way the window's frame boundary does.
fn answer(session: &mut Session, call: &Call) -> Answer {
    match call {
        Call::List => Answer::ok(listing(session)),
        Call::Dispatch(request) => dispatch(session, request, Door::Window { sink: None, scope: None }),
        Call::Ask(query) => mcp::asking(session, *query),
        Call::Findings => Answer::ok(mcp::findings(session)),
        Call::Preview(request) => mcp::preview(session, request, None),
        Call::Inspect => Answer::ok(json!(mcp::Window::default())),
    }
}

/// Calls one tool and answers whatever the host is handed while the call is in flight.
///
/// The two run in one task on purpose: this is the frame boundary of the test — a call waits until
/// the host answers it, exactly as the window's own frame does.
async fn called(
    client: &mut RunningService<RoleClient, ()>,
    request: CallToolRequestParams,
    mailbox: &mut mcp::Mailbox,
    session: &mut Session,
) -> Value {
    let call = client.call_tool(request);
    tokio::pin!(call);
    let answered = loop {
        tokio::select! {
            answer = &mut call => break answer.expect("the call answers"),
            pending = mailbox.wait() => {
                let pending = pending.expect("the host still holds the mailbox");
                let answer = answer(session, pending.call());
                pending.answer(answer);
            }
        }
    };
    answered.structured_content.clone().expect("a structured answer")
}

#[tokio::test]
async fn a_token_reaches_the_commands_and_a_request_without_one_never_does() {
    let directory = testkit::scratch_dir("channel-supported");
    let (channel, mut mailbox) = Channel::open(&directory).expect("the channel opens");
    let mut session = session();
    let mut client = client(&directory).await;

    // What a window offers: the headless tools and the window's own report.
    let tools = tokio::time::timeout(DEADLINE, client.list_tools(None)).await.expect("the list answers").expect("ok");
    let names: Vec<&str> = tools.tools.iter().map(|tool| tool.name.as_ref()).collect();
    assert!(names.contains(&TOOL_INSPECT), "an attached window reports on itself: {names:?}");
    assert!(names.contains(&mcp::TOOL_FINDINGS), "and it inspects the document: {names:?}");
    assert!(names.contains(&mcp::TOOL_PREVIEW), "and it draws the picture it owns the device for: {names:?}");
    assert_eq!(names.len(), 7, "the coarse tools and the window report");

    // `command_list` is answered by the host, at the moment a frame would answer it.
    let body = called(&mut client, CallToolRequestParams::new(TOOL_LIST), &mut mailbox, &mut session).await;
    assert_eq!(body["revision"], json!(session.project().revision().get()));
    assert!(
        body["commands"].as_array().is_some_and(|rows| rows.len() > 50),
        "the whole registry is projected through the channel: {body}"
    );

    // `inspect` is the rules' answer, and it travels the same queue.
    let body = called(&mut client, CallToolRequestParams::new(mcp::TOOL_FINDINGS), &mut mailbox, &mut session).await;
    assert!(body["findings"].is_array(), "the inspection answers with a list: {body}");

    // `ui_inspect` is the window's own answer, and it travels the same queue.
    let body = called(&mut client, CallToolRequestParams::new(TOOL_INSPECT), &mut mailbox, &mut session).await;
    assert_eq!(body["tabs"], json!([]), "the host's window is empty in this test: {body}");

    // A peer that speaks the protocol without the token is refused, and no call reaches the host.
    let (port, _token) = published(&directory);
    let mut stranger = TcpStream::connect(("127.0.0.1", port)).await.expect("the channel listens");
    let hello = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}).to_string();
    stranger.write_all(format!("{hello}\n").as_bytes()).await.expect("a frame is written");
    let mut refused = String::new();
    stranger.read_to_string(&mut refused).await.expect("the server closes the connection");
    assert!(refused.starts_with("refused"), "the token comes first: {refused:?}");
    assert!(mailbox.take().is_none(), "an unauthenticated request never reaches the dispatcher");

    let _ = client.close().await;
    channel.shutdown();
    assert!(!directory.join(FILE_NAME).exists(), "a clean exit removes the token file");
}

#[tokio::test]
async fn a_served_call_rings_the_host_and_answers_inside_the_budget() {
    // X111: a window repaints on input, so a call that arrived while the person was not touching it
    // waited in the queue for a pointer event only the person could bring. The host of this test is
    // asleep until the channel rings it, so an answer here is the bell's own evidence: nothing in
    // this test polls the mailbox, moves a pointer or waits on a clock.
    let directory = testkit::scratch_dir("channel-budget");
    let (channel, mut mailbox) = Channel::open(&directory).expect("the channel opens");
    let rung = Arc::new(tokio::sync::Notify::new());
    mailbox.watch({
        let rung = Arc::clone(&rung);
        Arc::new(move || rung.notify_one())
    });
    let mut session = session();
    let mut client = client(&directory).await;

    let started = Instant::now();
    let answered = {
        let call = client.call_tool(CallToolRequestParams::new(TOOL_LIST));
        tokio::pin!(call);
        // The call is polled while the host sleeps, because that is what writes its frame to the
        // socket: no answer may arrive before the ring, and nothing polls the mailbox (X111).
        tokio::time::timeout(ANSWER_BUDGET, async {
            tokio::select! {
                answered = &mut call => panic!("the call answered before the host was rung: {answered:?}"),
                () = rung.notified() => {}
            }
        })
        .await
        .expect("the call rings the host");
        let pending = mailbox.take().expect("the ring names a call that is waiting");
        let asked = answer(&mut session, pending.call());
        pending.answer(asked);
        let answered = tokio::time::timeout(ANSWER_BUDGET, &mut call).await.expect("the answer arrives");
        answered.expect("the call answers")
    };
    assert!(answered.structured_content.is_some(), "the host's answer reached the client");
    assert!(
        started.elapsed() <= ANSWER_BUDGET,
        "a served call answers in {:?}, inside the budget written beside it",
        started.elapsed()
    );

    let _ = client.close().await;
    channel.shutdown();
}

#[tokio::test]
async fn a_second_instance_does_not_take_the_first_ones_channel() {
    let directory = testkit::scratch_dir("channel-second");
    let (first, first_mailbox) = Channel::open(&directory).expect("the first channel opens");
    let published_here = published(&directory);
    assert_eq!(published_here.0, first.port(), "the file names the channel that is up");

    let refused = Channel::open(&directory);
    assert!(
        matches!(refused, Err(ChannelError::Taken { port }) if port == published_here.0),
        "a live channel is not replaced by a second instance"
    );
    assert_eq!(published(&directory), published_here, "and its file is untouched");
    assert!(!refused.expect_err("the second channel is refused").to_string().is_empty());

    drop(first_mailbox);
    first.shutdown();
    assert!(!directory.join(FILE_NAME).exists(), "the first instance's clean exit removes its own file");

    // With the first gone, the folder is free again, and the new channel has a token of its own.
    let (second, _mailbox) = Channel::open(&directory).expect("the folder is free");
    assert_ne!(published(&directory).1, published_here.1, "a run mints its own token");
    assert_eq!(published(&directory).0, second.port());
}

#[tokio::test]
async fn a_window_batch_lands_in_the_document_and_says_the_file_is_the_persons() {
    // The one difference between a window and a headless session: the answer never claims a write
    // the person did not ask for, and the document still holds the change (ADR 0005, ADR 0188).
    let mut session = session();
    let steps = [BatchStep::new("node.rename", json!({"node": 1, "name": "Renamed"}))];
    let request = mcp::BatchRequest {
        steps: steps.iter().map(|step| mcp::StepRequest::new(step.command(), step.params().clone())).collect(),
        product: None,
        atomic: true,
        dry_run: false,
        save: false,
    };
    let answer = dispatch(&mut session, &request, Door::Window { sink: None, scope: None });
    let body = answer.body();
    assert_eq!(body["effect"], json!("landed"), "{body}");
    assert_eq!(body["written"], json!(false), "a window writes nothing behind the person's back");
    assert_eq!(body["write"], json!("window"), "and says why: {body}");
    let read = mcp::asking(&session, Query::Tree { node: None });
    assert!(read.body()["answer"].is_object(), "the document holds what the batch did");
}

#[tokio::test]
async fn a_save_a_scene_with_no_path_cannot_take_is_a_refusal_that_names_the_persons_act() {
    // X114: a caller may ask for the save, and a scene that was never written has nowhere to put
    // it. A caller never chooses a path (ADR 0188), so the answer is a failure with the sentence
    // that names the person's act — never a silent write into a new file.
    let mut session = session();
    let request = mcp::BatchRequest {
        steps: vec![mcp::StepRequest::new("node.rename", json!({"node": 1, "name": "Renamed"}))],
        product: None,
        atomic: true,
        dry_run: false,
        save: true,
    };
    let answer = dispatch(&mut session, &request, Door::Window { sink: None, scope: None });
    let body = answer.body();
    assert!(answer.failed(), "the ask was not honoured: {body}");
    assert_eq!(body["effect"], json!("landed"), "the document still moved: {body}");
    assert_eq!(body["write"], json!("no_path"), "{body}");
    assert_eq!(body["written"], json!(false), "{body}");
    let sentence = body["write_error"].as_str().expect("a sentence that says what to do");
    assert!(sentence.contains("Save as"), "it names the person's own act: {sentence}");
}
