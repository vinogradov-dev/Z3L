//! The first answer a client reads: the domain the program is for, and the world of every command.
//!
//! `command_list` is the discovery door, and a caller that reads only ids and schemas infers the
//! world from a label — which is how a wardrobe was built in millimetres and a part was placed by a
//! mirrored side (X115, X118). The answer now carries `domain` (the guide) and one `family` word per
//! command, and the two are one projection: the families a command row claims and the command lists
//! the guide publishes are read from the same registry, and this test holds them to each other.

#![allow(clippy::expect_used)]

use engine::Session;
use mcp::listing;
use serde_json::Value;
use testkit::{sample_catalog, sample_project};

/// One session over the sample document and catalogue.
fn session() -> Session {
    Session::with_directory(sample_project(), sample_catalog())
}

/// The rows of the listing's command list.
fn rows(answer: &Value) -> Vec<Value> {
    answer["commands"].as_array().expect("the listing lists the commands").clone()
}

#[test]
fn the_listing_carries_the_domain_guide_the_engine_answers() {
    let answer = listing(&session());
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
    let answer = listing(&session());
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
    let answer = listing(&session());
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
