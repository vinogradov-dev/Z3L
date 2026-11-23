//! The first answer a client reads: the domain the program is for, the world of every command, and
//! the schema of the one command it is about to dispatch.
//!
//! `command_list` is the discovery door, and a caller that reads only ids and schemas infers the
//! world from a label — which is how a wardrobe was built in millimetres and a part was placed by a
//! mirrored side (X115, X118). The answer carries `domain` (the guide) and one `family` word per
//! command, and the two are one projection: the families a command row claims and the command lists
//! the guide publishes are read from the same registry, and this test holds them to each other.
//!
//! The rows are short and the schema is asked for by id (J15): one answer with every schema inline
//! was 189 989 bytes, about 40 151 tokens, and a caller that cannot hold an answer slices it and
//! reads the slice as a fact. What is asserted here is that the list stays inside its budget, that it
//! still holds every id, and that the schema an id is asked for is byte for byte the one a run
//! parses.

#![allow(clippy::expect_used)]

use engine::{AgentMode, CommandRegistry, Session};
use mcp::listing;
use serde_json::Value;
use testkit::{sample_catalog, sample_project};

/// One session over the sample document and catalogue.
fn session() -> Session {
    Session::with_directory(sample_project(), sample_catalog())
}

/// The rows of one listing's command list.
fn rows(answer: &Value) -> Vec<Value> {
    answer["commands"].as_array().expect("the listing lists the commands").clone()
}

/// The listing every command in short rows, as a caller that reads the surface first asks for it.
fn short_listing(session: &Session) -> Value {
    let answer = listing(session, None);
    assert!(!answer.failed(), "the list is not a refusal: {:?}", answer.body());
    answer.body().clone()
}

#[test]
fn the_listing_carries_the_domain_guide_the_engine_answers() {
    let answer = short_listing(&session());
    assert_eq!(answer["domain"], engine::guide(), "one guide, not a copy of one");
    let worlds = answer["domain"]["worlds"].as_array().expect("the four families");
    assert_eq!(worlds.len(), 4, "furniture, geometry, tree and scene");
    assert!(
        worlds.iter().all(|world| world["begin_with"].is_string() || world["family"] == "tree"),
        "each world says the command its work begins with"
    );
}

#[test]
fn every_command_row_says_which_world_it_serves() {
    let answer = short_listing(&session());
    let rows = rows(&answer);
    assert!(!rows.is_empty());
    for row in &rows {
        let family = row["family"].as_str().unwrap_or_else(|| panic!("{} has no family", row["id"]));
        assert!(
            ["furniture", "geometry", "tree", "scene"].contains(&family),
            "{} serves `{family}`, which is not a world",
            row["id"]
        );
    }
    let family_of =
        |id: &str| rows.iter().find(|row| row["id"] == id).and_then(|row| row["family"].as_str()).map(str::to_owned);
    assert_eq!(family_of("product.create").as_deref(), Some("furniture"), "a frame is furniture");
    assert_eq!(family_of("panel.create").as_deref(), Some("furniture"), "a part is furniture");
    assert_eq!(family_of("object.primitive.create").as_deref(), Some("geometry"), "a body is geometry");
    assert_eq!(family_of("node.rename").as_deref(), Some("tree"), "the tree is neither");
    assert_eq!(family_of("light.create").as_deref(), Some("scene"), "a light is the view");
}

#[test]
fn the_guide_and_the_command_rows_cannot_disagree_about_a_world() {
    let answer = short_listing(&session());
    let rows = rows(&answer);
    for world in answer["domain"]["worlds"].as_array().expect("the worlds") {
        let family = world["family"].as_str().expect("a family word");
        let listed: Vec<&str> =
            world["commands"].as_array().expect("a command list").iter().filter_map(Value::as_str).collect();
        let claimed: Vec<&str> =
            rows.iter().filter(|row| row["family"] == family).filter_map(|row| row["id"].as_str()).collect();
        assert_eq!(listed, claimed, "the `{family}` world lists exactly the commands that claim it");
        assert!(!listed.is_empty(), "a world nobody serves would be a sentence with no tool behind it");
    }
}

#[test]
fn the_short_list_holds_every_command_and_stays_inside_its_budget() {
    let answer = short_listing(&session());
    let rows = rows(&answer);
    let specs = CommandRegistry::new().specs();
    let listed: Vec<&str> = rows.iter().filter_map(|row| row["id"].as_str()).collect();
    let registered: Vec<&str> = specs.iter().map(|spec| spec.id()).collect();
    assert_eq!(listed, registered, "every command is listed once, in the registry's order");
    assert!(
        rows.iter().all(|row| row.get("params").is_none()),
        "no row carries a schema: that is what the list was too big to hold"
    );
    assert!(
        answer["schema_note"].as_str().is_some_and(|note| note.contains("`command`")),
        "the list says how a schema is asked for: {}",
        answer["schema_note"]
    );
    let bytes = answer.to_string().len();
    assert!(
        bytes < mcp::LIST_BUDGET_BYTES,
        "the list is {bytes} bytes, over the {} the promise allows: a caller's context is the cost \
         of every byte, so a new command adds one row and not a schema",
        mcp::LIST_BUDGET_BYTES
    );
}

#[test]
fn the_schema_of_one_id_is_the_one_a_run_is_refused_by() {
    let session = session();
    for spec in CommandRegistry::new().specs() {
        let answer = listing(&session, Some(spec.id()));
        assert!(!answer.failed(), "`{}` is a command: {:?}", spec.id(), answer.body());
        let body = answer.body();
        let rows = rows(body);
        let row = rows.first().expect("one row: the schema is answered for the id that was asked");
        assert_eq!(rows.len(), 1, "`{}` is one row", spec.id());
        assert_eq!(row["id"], spec.id(), "the row is the id that was asked for");
        assert_eq!(row["params"], spec.params_schema(), "`{}` publishes the schema its own parser reads", spec.id());
        assert!(body.get("domain").is_none(), "the guide is read once, with the list, and not again");
        assert!(body.get("schema_note").is_none(), "and so is the sentence that asks for a schema");
    }
}

#[test]
fn an_id_this_build_does_not_dispatch_is_refused_with_the_ids_that_exist() {
    let answer = listing(&session(), Some("panel.make"));
    assert!(answer.failed(), "there is no such command: {:?}", answer.body());
    let sentence = answer.body()["error"].as_str().expect("a refusal carries a sentence");
    assert!(sentence.contains("`panel.make`"), "the sentence names what was asked: {sentence}");
    for spec in CommandRegistry::new().specs() {
        assert!(sentence.contains(spec.id()), "the refusal names `{}`: {sentence}", spec.id());
    }
}

#[test]
fn a_plan_session_says_once_that_nothing_it_runs_here_writes() {
    // The rows carry no verdict (ADR 0226): what a session cannot do here is the session's own
    // answer, and a plan-mode session is the one whose answer is a refusal.
    let mut session = session();
    session.set_agent_mode(AgentMode::Plan);
    let answer = short_listing(&session);
    assert_eq!(answer["mode"], "plan", "the mode is the session's, and the list says which");
    assert_eq!(answer["mode_note"], mcp::PLAN_NOTE, "the sentence a batch is refused with, once");
    let mut built = session;
    built.set_agent_mode(AgentMode::Build);
    assert!(short_listing(&built).get("mode_note").is_none(), "a build session writes, so it has nothing to refuse");
}
