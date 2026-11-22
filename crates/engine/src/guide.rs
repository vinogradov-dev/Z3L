//! The domain a caller is building in, said once and served to every door.
//!
//! A model that is handed fifty ids and their schemas still does not know what the program is for.
//! It does not know that a build from scratch starts with a product frame, that the frame's base
//! rectangle may have a corner cut or a drawn ring, that a panel is born in one of three stances and
//! that its class names its role, or that a nut is not furniture at all. That knowledge lived in
//! `docs/` and in the heads of the people who wrote it, so every caller inferred it from labels and
//! got the units and the stances wrong (X115, X117, X118).
//!
//! This module is that knowledge as one value. Nothing here is a construction: it names no carcass,
//! no wardrobe and no nut, because a construction is a decision a caller makes out of parts the
//! program already has (ADR 0192, ADR 0215). It is built from the two tables that own the answers —
//! [`crate::Family`] over the command registry and `model::class::registry()` over the semantic
//! classes — so a word cannot drift from the thing it names, and the sentences a table cannot state
//! are counted and few.
//!
//! Changes when: the world model, the order of a build, the stance table or a unit changes.

use model::{ClassCode, Envelope, Footprint, Level, Placement, Side, Supply, registry};
use serde_json::{Value, json};
use text::Lang;
use units::Length;

use crate::family::Family;
use crate::panel_cmds::CREATE_ID as PANEL_CREATE_ID;
use crate::product_cmds::PRODUCT_CREATE_ID;

/// The radius the worked frame example rounds one front corner with: 100 mm.
pub const EXAMPLE_CORNER_RADIUS: Length = Length::from_micrometres(100_000);

/// The frame the guide's worked example builds: 800 × 560 × 2200 mm with one rounded front corner.
///
/// The numbers are a wardrobe because that is the piece this track exists for, and the corner is
/// the front-left one because an angular cabinet rounds the corner a person walks past. The value
/// is a real [`Envelope`], so a test can dispatch it and a reader cannot mistake the prose for a
/// shape the program does not accept.
pub fn example_frame() -> Option<Envelope> {
    let size = |millimetres: i64| Length::from_millimetres(millimetres);
    let corner = model::CornerFeatureKind::Radius { radius: EXAMPLE_CORNER_RADIUS };
    let mut corners = [None; 4];
    corners[2] = Some(corner);
    Envelope::with_footprint(size(800)?, size(560)?, size(2200)?, Footprint::cut(corners)).ok()
}

/// The one value every guide door prints.
pub fn guide() -> Value {
    json!({
        "domain": domain(),
        "worlds": worlds(),
        "steps": steps(),
        "frames": frames(),
        "stances": stances(),
        "niches": niches(),
        "classes": classes(),
        "units": units(),
        "sides": sides(),
    })
}

/// What the program is, and which reading wins when a word could mean two things.
fn domain() -> Value {
    json!({
        "name": "Z3L",
        "kind": "furniture",
        "rule": "This program builds furniture, and its words win. «Корпус» is a product frame with \
                 parts, not a sheet-metal box; «штанга» is a hanging rail, not a steel bar. A solid \
                 that is no furniture part at all — a nut, a bracket, a template — belongs to the \
                 geometry world and is built there.",
        "words": {
            "product": "one piece of furniture: a container whose class is a product class and which \
                        stores an authored envelope; its box is what the parts are measured against",
            "part": "a panel or a bought piece inside one product: an upright, a horizontal, a back, \
                     a door, a front, a worktop",
            "fitting": "a piece mounted on a part: a hinge, a runner, a handle, a hanging rail",
            "body": "a free solid built from primitives, extrusions, paths and booleans; it is not a \
                     part, no product contains it and it is cut from no sheet",
        },
    })
}

/// The four command families, each with the tools that serve it.
fn worlds() -> Value {
    Value::Array(
        Family::ALL
            .iter()
            .map(|family| {
                json!({
                    "family": family.word(),
                    "serves": family.serves(),
                    "begin_with": first_command(*family),
                    "commands": family.commands().collect::<Vec<&str>>(),
                })
            })
            .collect(),
    )
}

/// The command a family's work begins with, when it has one.
fn first_command(family: Family) -> Option<&'static str> {
    match family {
        Family::Furniture => Some(PRODUCT_CREATE_ID),
        Family::Geometry => Some("object.primitive.create"),
        Family::Scene => Some("camera.create"),
        Family::Tree => None,
    }
}

/// The order of a build from scratch: the frame, the parts, their edges, then what is mounted on them.
fn steps() -> Value {
    json!([
        {
            "order": 1,
            "step": "frame",
            "command": PRODUCT_CREATE_ID,
            "says": "A piece begins as a product frame: three authored sides and a footprint on its \
                     base plane. The frame never resizes with its parts (ADR 0098), and creating it \
                     opens it, so the first part lands inside it without a second step: every answer \
                     names `open_product` (ADR 0100).",
        },
        {
            "order": 2,
            "step": "parts",
            "command": PANEL_CREATE_ID,
            "says": "Every part is a panel created in one of the three stances below, with a class \
                     that names its role. Width, depth and height are solved from the frame: a part \
                     is placed by a length the caller computed, and the program keeps the number it \
                     was given rather than deriving it. The command names the piece it writes in; \
                     the interface offers creation only inside the piece the person has open \
                     (ADR 0098, ADR 0226).",
        },
        {
            "order": 3,
            "step": "edges",
            "command": "panel.edge_banding.set",
            "says": "An exposed edge takes a band, and the edge id names an edge in the panel's own \
                     frame, so the front edge of a shelf and of an upright are both edge 3.",
        },
        {
            "order": 4,
            "step": "fittings",
            "command": null,
            "says": "No command creates a fitting yet. The class registry names them (50.*) and the \
                     hanging rail is 20.06, so a caller knows what the piece is called, but the only \
                     way to put one in the scene today is to build it as a body in the geometry \
                     world — or to leave it out until the hardware family lands.",
        },
    ])
}

/// The shapes a product frame may take, with one real envelope a caller can read and copy.
fn frames() -> Value {
    let mut value = json!({
        "command": PRODUCT_CREATE_ID,
        "parameter": "envelope.footprint",
        "says": "The frame is a box cut to a footprint: the three sides are the bounds of the \
                 reserved space and the footprint says what the base rectangle is cut to. It is \
                 authored once, at creation, and no child edit changes it.",
        "shapes": [
            {
                "shape": "rectangle",
                "says": "The whole rectangle, which is also what an envelope without a footprint \
                         means. A plain cabinet.",
            },
            {
                "shape": "cut",
                "says": "The rectangle with one or more corners cut by a chamfer or a fillet. This \
                         is a radius front corner: an angular cabinet or a wardrobe with a rounded \
                         edge. `corners` holds four entries, counter-clockwise from the zero corner \
                         (bottom-left, bottom-right, top-right, top-left); `null` leaves that corner \
                         square.",
            },
            {
                "shape": "ring",
                "says": "A closed ring drawn on the base plane — a polygon, an arc or a mix, in the \
                         product's own frame. Its bounds are the three sides.",
            },
        ],
    });
    if let (Some(envelope), Some(object)) = (example_frame(), value.as_object_mut())
        && let Ok(example) = serde_json::to_value(envelope)
    {
        object.insert("example".to_owned(), example);
    }
    value
}

/// One stance of ADR 0105, as a rotation a caller can write into a placement.
///
/// The stance is the flat panel turned about a world axis, never mirrored: the shelf is the base,
/// the post is the shelf a quarter-turn to the right about `+Y`, and the wall is the post a
/// quarter-turn about `+Z`. The numbers are a quaternion `[x, y, z, w]` in the placement's own
/// order, so a caller who reads this table writes the stance and does not reinvent it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stance {
    /// The word for the stance.
    name: &'static str,
    /// How it is made from the flat panel.
    turns: &'static str,
    /// The rotation `[x, y, z, w]`, as a placement stores it.
    rotation: [f64; 4],
    /// Where the panel's local length runs.
    length: &'static str,
    /// Where the panel's local width runs.
    width: &'static str,
    /// Where the panel's thickness runs, which is the direction face `A` points.
    thickness: &'static str,
    /// Where vertex 1 — the node's origin — stands.
    vertex_1: &'static str,
}

impl Stance {
    /// Every stance of ADR 0105, the flat one first.
    pub const ALL: [Self; 3] = [
        Self {
            name: "shelf",
            turns: "unturned: the flat panel is the shelf",
            rotation: [0.0, 0.0, 0.0, 1.0],
            length: "+X",
            width: "+Y",
            thickness: "+Z",
            vertex_1: "the minimum x and y corner, on the bottom face",
        },
        Self {
            name: "post",
            turns: "-90 degrees about +Y",
            rotation: [0.0, -std::f64::consts::FRAC_1_SQRT_2, 0.0, std::f64::consts::FRAC_1_SQRT_2],
            length: "+Z",
            width: "+Y",
            thickness: "-X",
            vertex_1: "the bottom-rear corner; the board grows from there towards -X",
        },
        Self {
            name: "wall",
            turns: "the post turned -90 degrees about +Z",
            rotation: [-0.5, -0.5, -0.5, 0.5],
            length: "+Z",
            width: "+X",
            thickness: "+Y",
            vertex_1: "the corner at minimum x and minimum z",
        },
    ];

    /// The word for the stance.
    pub const fn name(self) -> &'static str {
        self.name
    }

    /// The rotation as a quaternion `[x, y, z, w]`.
    pub const fn rotation(self) -> [f64; 4] {
        self.rotation
    }

    /// The rotation as a placement rotation.
    pub fn placement_rotation(self) -> geom::DQuat {
        let [x, y, z, w] = self.rotation;
        geom::DQuat::from_xyzw(x, y, z, w)
    }

    /// The placement that turns the flat panel into this stance, at the parent frame's origin.
    pub fn placement(self) -> Option<Placement> {
        Placement::new(geom::DVec3::ZERO, self.placement_rotation()).ok()
    }
}

/// The empty space a point inside a piece names, and the one command that fills it.
///
/// A caller that knows every size and place builds its panels itself; a void of irregular shape is
/// where measuring beats computing, and the two doors below answer it from the same model. The rule
/// about *which* piece is the caller's own care: a caller that is not sure which product the task
/// means asks the person to open it instead of guessing (ADR 0098, ADR 0226).
fn niches() -> Value {
    json!({
        "says": "The empty space inside a piece is measured, not assumed: a point in world \
                 millimetres names one convex cell with its walls, and the part that fills it is \
                 derived from that cell. Use it when the void is not the rectangle you can compute \
                 — a splayed carcass, a cut corner, a ring — and build the panels by their numbers \
                 when you already know them.",
        "ask": {
            "query": "niche",
            "arguments": {"at": "[x, y, z] in world millimetres"},
            "answers": "the product the cell belongs to, the box it reserves, its planes and corners, \
                        and whether its section is a box, a section or a sloped cell",
        },
        "fill": {
            "command": "panel.fill",
            "arguments": {
                "parent": "the container the part lands in: the product or a group inside it",
                "at": "the point inside the void, world millimetres",
                "class": "the class the part plays; its fill trait chooses the axis pair",
                "mode": "`fit` for the largest rectangle, `shape` for the section the walls leave",
                "free": "the place along the free axis, millimetres; absent, the point answers",
                "name": "the part's name; absent, its class's word, made unique",
            },
            "answers": "the id of the created panel, its class, its nominal size and the stance the \
                        guide states; one revision and one undo entry",
            "refuses": "a point in no product, a void that is not one cell, a void of another product \
                        than `parent`, and a class whose traits carry no fill pair",
        },
        "which_product": "Name the product you are sure of. When a task does not say which piece it \
                          means and more than one product is in the scene, ask the person to open it \
                          — every answer names `open_product` — instead of writing into a guess.",
    })
}

/// The three stances, as the guide prints them.
fn stances() -> Value {
    Value::Array(
        Stance::ALL
            .iter()
            .map(|stance| {
                json!({
                    "stance": stance.name,
                    "turns": stance.turns,
                    "rotation": stance.rotation,
                    "length": stance.length,
                    "width": stance.width,
                    "thickness": stance.thickness,
                    "vertex_1": stance.vertex_1,
                })
            })
            .collect(),
    )
}

/// The semantic classes, grouped by the family heading that owns them.
///
/// The rows are the registry's, with the words a person may type for them: a caller that hears
/// «горизонт» finds `10.11` without us writing a second dictionary.
fn classes() -> Value {
    let mut groups: Vec<(u32, Value)> = Vec::new();
    for row in registry() {
        if row.is_family() {
            groups.push((
                row.code() / 10_000,
                json!({
                    "group": format!("{:02}", row.code() / 10_000),
                    "family": row.slug(),
                    "label": row.label().text(Lang::En),
                    "classes": [],
                }),
            ));
            continue;
        }
        if !row.is_assignable() {
            continue;
        }
        let Some(level) = row.traits().map(|traits| traits.level()) else {
            continue;
        };
        let Some((_, group)) = groups.last_mut() else {
            continue;
        };
        let class = json!({
            "code": ClassCode::from_code(row.code()).map(|code| code.to_string()),
            "slug": row.slug(),
            "label": row.label().text(Lang::En),
            "level": level_word(level),
            "supply": row.traits().map(|traits| supply_word(traits.supply())),
            "aliases": row.aliases(),
        });
        if let Some(rows) = group.get_mut("classes").and_then(Value::as_array_mut) {
            rows.push(class);
        }
    }
    Value::Array(groups.into_iter().map(|(_, group)| group).collect())
}

/// The wire word of a class level.
const fn level_word(level: Level) -> &'static str {
    match level {
        Level::Product => "product",
        Level::Component => "component",
        Level::Part => "part",
        Level::Fitting => "fitting",
    }
}

/// The wire word of a supply kind.
const fn supply_word(supply: Supply) -> &'static str {
    match supply {
        Supply::MadeFromSheet => "sheet",
        Supply::MadeFromLinear => "linear",
        Supply::BuyPiece => "bought",
        Supply::MadeInShop => "made-in-shop",
    }
}

/// The units every number travels in, which is the mistake that cost a whole build (X115).
fn units() -> Value {
    json!({
        "length": "micrometres, integer: every owning, index and size parameter, and every property \
                   of kind `length`",
        "placement": "millimetres, floating point: the translation of a placement, and a world \
                      point in an answer that names it `_mm`",
        "angle": "millidegrees, integer: an angle parameter and every property of kind `angle`",
        "rotation": "a unit quaternion [x, y, z, w] in a placement; Euler angles are a presentation \
                     and are never stored",
        "rule": "the published parameter schema says which shape a field has, and the field's name \
                 says its unit; a number that looks like a length is never assumed to be one",
    })
}

/// The four sides of a piece, with the axis each one faces.
fn sides() -> Value {
    json!({
        "rule": "A piece has a front, a back, a left and a right, and every dimension is read \
                 against them. The front view looks from +Y towards -Y with +Z up, so the side a \
                 viewer standing in front of the piece sees on their left is +X.",
        "axes": Side::ALL.iter().map(|side| {
            json!({
                "side": side_word(*side),
                "axis": axis_word(side.axis()),
            })
        }).collect::<Vec<Value>>(),
    })
}

/// The wire word of a side.
const fn side_word(side: Side) -> &'static str {
    match side {
        Side::Left => "left",
        Side::Right => "right",
        Side::Front => "front",
        Side::Back => "back",
    }
}

/// The signed axis a direction points along, as the wire spells it.
fn axis_word(axis: geom::DVec3) -> &'static str {
    let words = ["+X", "-X", "+Y", "-Y", "+Z", "-Z"];
    let vectors =
        [geom::DVec3::X, geom::DVec3::NEG_X, geom::DVec3::Y, geom::DVec3::NEG_Y, geom::DVec3::Z, geom::DVec3::NEG_Z];
    vectors
        .iter()
        .position(|candidate| (*candidate - axis).length() < 1e-9)
        .and_then(|index| words.get(index).copied())
        .unwrap_or("none")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use geom::DVec3;
    use model::Footprint;
    use serde_json::from_value;

    use crate::CommandRegistry;

    use super::*;

    /// Every command id the guide states under `command` and `begin_with`.
    fn named_commands(value: &Value, found: &mut Vec<String>) {
        match value {
            Value::Object(map) => {
                for (key, inner) in map {
                    if key == "command" || key == "begin_with" {
                        continue;
                    }
                    named_commands(inner, found);
                }
                for key in ["command", "begin_with"] {
                    if let Some(id) = map.get(key).and_then(Value::as_str) {
                        found.push(id.to_owned());
                    }
                }
            }
            Value::Array(items) => {
                for item in items {
                    named_commands(item, found);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn the_guide_names_only_commands_the_registry_holds() {
        let guide = guide();
        let mut found = Vec::new();
        named_commands(&guide, &mut found);
        assert!(!found.is_empty(), "the guide names the tools it expects a caller to use");
        for id in found {
            assert!(CommandRegistry::new().get(&id).is_some(), "the guide names `{id}`, which is not registered");
        }
    }

    #[test]
    fn every_world_lists_the_commands_of_its_family() {
        let guide = guide();
        let worlds = guide["worlds"].as_array().expect("worlds is an array");
        assert_eq!(worlds.len(), Family::ALL.len());
        for (world, family) in worlds.iter().zip(Family::ALL) {
            assert_eq!(world["family"], family.word());
            let listed: Vec<&str> =
                world["commands"].as_array().expect("a list").iter().filter_map(Value::as_str).collect();
            let expected: Vec<&str> = family.commands().collect();
            assert_eq!(listed, expected, "{} lists its family", family.word());
        }
    }

    #[test]
    fn every_assignable_class_appears_once_with_the_words_a_person_types() {
        let guide = guide();
        let groups = guide["classes"].as_array().expect("classes is an array");
        assert!(!groups.is_empty());
        let mut codes: Vec<&str> = Vec::new();
        for group in groups {
            for class in group["classes"].as_array().expect("rows") {
                let code = class["code"].as_str().expect("a code");
                codes.push(code);
                assert!(class["label"].as_str().is_some_and(|label| !label.is_empty()), "{code} has a label");
                assert!(class["level"].as_str().is_some(), "{code} says its level");
            }
        }
        let assignable = registry().iter().filter(|row| !row.is_family() && row.is_assignable()).count();
        assert_eq!(codes.len(), assignable, "one row per assignable class, no duplicates and none dropped");
        let shelf = groups
            .iter()
            .flat_map(|group| group["classes"].as_array().into_iter().flatten())
            .find(|class| class["code"] == "10.11")
            .expect("the horizontal row is published");
        let aliases: Vec<&str> =
            shelf["aliases"].as_array().expect("aliases").iter().filter_map(Value::as_str).collect();
        assert!(aliases.contains(&"полка"), "the row carries the word a person types: {aliases:?}");
    }

    #[test]
    fn the_stance_table_is_the_one_adr_0105_states() {
        let cases = [
            (Stance::ALL[0], DVec3::X, DVec3::Y, DVec3::Z),
            (Stance::ALL[1], DVec3::Z, DVec3::Y, DVec3::NEG_X),
            (Stance::ALL[2], DVec3::Z, DVec3::X, DVec3::Y),
        ];
        for (stance, length, width, thickness) in cases {
            let placement = stance.placement().expect("a unit rotation");
            let along = |axis: DVec3| placement.transform_vector(axis);
            assert!((along(DVec3::X) - length).length() < 1e-9, "{}: the length runs along {length}", stance.name());
            assert!((along(DVec3::Y) - width).length() < 1e-9, "{}: the width runs along {width}", stance.name());
            assert!(
                (along(DVec3::Z) - thickness).length() < 1e-9,
                "{}: the thickness runs along {thickness}",
                stance.name()
            );
        }
    }

    #[test]
    fn a_stance_puts_its_origin_on_the_bottom_face() {
        for stance in Stance::ALL {
            let placement = stance.placement().expect("a unit rotation");
            let origin = placement.transform_point(DVec3::ZERO);
            assert!(origin.z.abs() < 1e-9, "{}: vertex 1 is on the base, never on the ceiling", stance.name());
        }
    }

    #[test]
    fn the_worked_frame_is_a_radius_corner_the_program_accepts() {
        let envelope = example_frame().expect("the example envelope");
        let value = serde_json::to_value(&envelope).expect("the envelope serializes");
        let read: Envelope = from_value(value).expect("the example reads back as an envelope");
        assert_eq!(read, envelope);
        let corners = match read.footprint() {
            Footprint::Cut { corners } => corners,
            other => panic!("the example is a cut corner, not {other:?}"),
        };
        assert!(
            matches!(corners[2], Some(model::CornerFeatureKind::Radius { radius }) if radius == EXAMPLE_CORNER_RADIUS),
            "the front corner is a fillet with the documented radius"
        );
        assert!(corners[0].is_none() && corners[1].is_none() && corners[3].is_none(), "the other corners stay square");
    }

    #[test]
    fn the_guide_names_the_niche_and_the_piece_a_caller_works_in() {
        // A released build ships no prose: the one command that fills a void and the rule about which
        // piece to write in have to travel with the tool, or a caller guesses both (N37).
        let guide = guide();
        assert_eq!(guide["niches"]["ask"]["query"], "niche", "the measurement is a query a caller can ask");
        assert_eq!(guide["niches"]["fill"]["command"], "panel.fill", "the fill is one command");
        assert!(
            guide["niches"]["which_product"].as_str().is_some_and(|rule| rule.contains("open_product")),
            "the rule says where the open product is read"
        );
        assert!(
            guide["steps"][1]["says"].as_str().is_some_and(|says| says.contains("ADR 0226")),
            "the build's order says who decides the piece"
        );
        assert!(
            guide["steps"][0]["says"].as_str().is_some_and(|says| says.contains("open_product")),
            "the frame a caller makes is open, so the part after it needs no second step (X112)"
        );
    }

    #[test]
    fn the_guide_says_which_world_owns_a_nut_and_which_owns_a_carcass() {
        let guide = guide();
        let worlds = guide["worlds"].as_array().expect("worlds");
        let furniture = worlds.iter().find(|world| world["family"] == "furniture").expect("the furniture world");
        let geometry = worlds.iter().find(|world| world["family"] == "geometry").expect("the geometry world");
        assert_eq!(furniture["begin_with"], PRODUCT_CREATE_ID, "a piece begins with its frame");
        assert_eq!(geometry["begin_with"], "object.primitive.create", "a body begins with a primitive");
        assert!(
            guide["domain"]["rule"].as_str().is_some_and(|rule| rule.contains("geometry world")),
            "the rule says where a body goes"
        );
    }
}
