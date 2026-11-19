//! The scene that proves the contact port on a hole: a plate with a square hole through it and a
//! ball above it. The workspace has no boolean operations yet, so the hole is a ring mesh — which is
//! exactly the case the port was built for: a box around the plate would stop the ball, a plate with
//! a hole lets it through (X85, ADR 0179).
//!
//! Nothing is committed: the scene is a fixture, and a scene belongs to the maker's disk and not to
//! the repository. The ordinary test writes it to a temporary file, reads it back and measures it, so
//! the format and the behaviour are verified together; the ignored test writes the same scene to a
//! fixed temporary path and prints it, which is how a person opens it in the editor.
#![allow(clippy::expect_used)]

use std::path::PathBuf;

use geom::DVec3;
use model::{Container, Geometry, Id, Mesh, MoveAxis, MoveSettings, Placement, Primitive, Project};
use project::SceneFile;
use units::Length;

/// Where the fixture is written: the system's temporary directory, never the repository.
fn scene_path() -> PathBuf {
    std::env::temp_dir().join(format!("z3l-through-a-hole.{}", project::EXTENSION))
}

fn millimetres(value: i64) -> Length {
    Length::from_millimetres(value).expect("a length")
}

/// A plate `size` millimetres square, `thickness` thick, with a `hole` square hole through its middle.
fn plate_with_a_hole(size: f64, hole: f64, thickness: f64) -> Mesh {
    let (outer, half) = (size / 2.0, hole / 2.0);
    let (top, bottom) = (thickness / 2.0, -thickness / 2.0);
    let mut positions: Vec<[Length; 3]> = Vec::new();
    let mut faces: Vec<Vec<u32>> = Vec::new();
    let mut quad =
        |positions: &mut Vec<[Length; 3]>, a: (f64, f64), b: (f64, f64), c: (f64, f64), d: (f64, f64), z: f64| {
            let base = u32::try_from(positions.len()).expect("a small mesh");
            for (x, y) in [a, b, c, d] {
                positions.push([millimetres(x as i64), millimetres(y as i64), millimetres(z as i64)]);
            }
            faces.push(vec![base, base + 1, base + 2, base + 3]);
        };
    // The ring of the top and of the bottom, then the hole's four walls and the outer four.
    for z in [top, bottom] {
        quad(&mut positions, (-outer, -outer), (-half, -outer), (-half, outer), (-outer, outer), z);
        quad(&mut positions, (-half, -outer), (half, -outer), (half, -half), (-half, -half), z);
        quad(&mut positions, (-half, half), (half, half), (half, outer), (-half, outer), z);
    }
    let walls = [(-half, -half), (half, -half), (half, half), (-half, half)];
    for (index, (x, y)) in walls.iter().copied().enumerate() {
        let (next_x, next_y) = walls[(index + 1) % walls.len()];
        let base = u32::try_from(positions.len()).expect("a small mesh");
        for (px, py, pz) in [(x, y, bottom), (next_x, next_y, bottom), (next_x, next_y, top), (x, y, top)] {
            positions.push([millimetres(px as i64), millimetres(py as i64), millimetres(pz as i64)]);
        }
        faces.push(vec![base, base + 1, base + 2, base + 3]);
    }
    Mesh::new(positions, faces).expect("a ring plate")
}

/// The scene: a plate with a hole, a ball above the hole and a turned board beside it.
fn through_a_hole() -> Project {
    let root = Container::new(Id::new(1).expect("an id"), "Through a hole", Placement::IDENTITY, Vec::new());
    let mut project = Project::new(root).expect("an empty project");
    let root_id = project.root().id();
    project
        .add_object(
            root_id,
            "Plate with a hole",
            Placement::IDENTITY.with_translation(Length::ZERO, Length::ZERO, millimetres(200)).expect("a place"),
            Geometry::Mesh(plate_with_a_hole(400.0, 140.0, 10.0)),
        )
        .expect("the plate");
    project
        .add_object(
            root_id,
            "Ball 60",
            Placement::IDENTITY.with_translation(Length::ZERO, Length::ZERO, millimetres(350)).expect("a place"),
            Geometry::Primitive(Primitive::sphere(millimetres(30), 24, 12).expect("a ball")),
        )
        .expect("the ball");
    project
        .add_object(
            root_id,
            "Turned board",
            Placement::IDENTITY
                .with_translation(millimetres(320), Length::ZERO, millimetres(300))
                .and_then(|place| {
                    Placement::new(place.translation(), geom::DQuat::from_rotation_z(std::f64::consts::FRAC_PI_4))
                })
                .expect("a place"),
            Geometry::Mesh(Mesh::cuboid([millimetres(-100); 3], [millimetres(100); 3]).expect("a cube")),
        )
        .expect("the board");
    project.validate().expect("a valid scene");
    project
}

/// The ball's id, by name.
fn ball(project: &Project) -> Id {
    project
        .geometry_ids_under(project.root().id())
        .expect("a tree")
        .into_iter()
        .find(|id| project.node(*id).is_some_and(|node| node.name() == "Ball 60"))
        .expect("the ball")
}

#[test]
#[ignore = "writes the fixture to a temporary file to open in the editor"]
fn write_the_scene() {
    let path = scene_path();
    SceneFile::from_document(through_a_hole()).write(&path).expect("the scene is written");
    println!("scene: {}", path.display());
}

#[test]
fn the_scene_carries_a_ball_through_the_hole_and_no_further_than_a_face() {
    // Written, read back and measured, so the fixture is checked as a file and not only in memory:
    // the temporary file is removed with the test process and never enters the repository.
    let path = scene_path();
    SceneFile::from_document(through_a_hole()).write(&path).expect("the scene is written");
    let text = std::fs::read_to_string(&path).expect("the scene is readable");
    let scene = SceneFile::parse(&text).expect("a readable scene");
    let project = scene.document();
    let ball = ball(project);
    let measured = MoveSettings { snaps: None, ..MoveSettings::default() };

    // Straight down through the hole: the plate's own faces are the answer and the way is clear,
    // where the box around the plate would stop the ball on its top face.
    let by_port = project.move_rule(&[ball], measured, Some(&contact_parry::Parry)).expect("a measured rule");
    let by_box = project.move_rule(&[ball], measured, None).expect("a box rule");
    let drop = DVec3::new(0.0, 0.0, -400.0);
    // The plate stands at z = 200, so the ball has room below it: the way through the hole is clear
    // and the workspace's floor is the only thing that stops it, where the box around the plate stops
    // the ball on the plate's top face at 205 (X85, ADR 0177, ADR 0179).
    let through = by_port.limit(MoveAxis::Z, drop.z);
    let blocked = by_box.limit(MoveAxis::Z, drop.z);
    assert_eq!(through.distance_mm, -320.0, "the ball passes through the hole and stops on the floor");
    assert!(blocked.distance_mm > drop.z, "the box around the plate stops the ball above the hole");
    assert_eq!(through.contact, Some(model::Stop::Floor), "and what stops the real body is the floor");
    assert!(blocked.contact.and_then(model::Stop::node).is_some(), "the box names the plate");
}
