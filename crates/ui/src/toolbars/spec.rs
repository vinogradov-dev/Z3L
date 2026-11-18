//! The ribbon's map: five tabs, their groups, and the tools inside them.
//!
//! The map is data (`plan/notes/U68.md` holds the approved one). A tab is a noun that names the
//! domain — *Sketch, Model, Furniture, Scene, View* — a group is one operation class under it, a tool
//! is one command and a variant is one input method of that command; [`super::list`] draws them as
//! the list a cell opens, and each row is one [`Method`]. Every level
//! carries a namespaced id: `sketch`, `sketch.create`, `sketch.create.line`,
//! `sketch.create.circle.three-points`, so a saved layout, a settings value or an agent never depends
//! on a position in the table.
//!
//! The table holds order, words, artwork and ids — never behaviour. What a tool *does* is
//! [`ToolKind`], a small enum the renderer matches once; whether it can run right now is decided
//! against the [`super::RibbonContext`] at draw time. A tool whose command does not exist yet is
//! *planned*: it stays in the table so the map reads whole, and both it and a group of only planned
//! tools are skipped, so a tab with nothing live is not drawn.
//!
//! Every row is a constant, so the ribbon allocates nothing to read the map.

use engine::PrimitiveKind;
use model::{ClassCode, FillMode};
use text::Message;

use crate::draw::DrawTool;
use crate::icons::{self, Icon};

/// What a click on a tool runs.
///
/// This is the only code a row carries: the map says where a tool sits and what it is called, this
/// says what happens. The data of an action — an extrusion's height, the selected path, the circle a
/// primitive is built around — is not stored here; the renderer reads it from the ribbon state and
/// the context when the button is drawn, so a row is one constant for every document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolKind {
    /// Starts one drawing method.
    Draw(DrawTool),
    /// Starts one drawing method whose closed figure is the profile of a solid body.
    ///
    /// The box and cylinder cells carry the sketch's own rectangle and circle methods, so a body is
    /// drawn by every method the sketch offers and built on Z afterwards (ADR 0138).
    Body(DrawTool),
    /// Closes the running chain into a ring.
    CloseDraft,
    /// Drops the running chain; the document never changed.
    CancelDraft,
    /// Extrudes the selected closed path into a body.
    Extrude,
    /// Opens the card that creates a product frame.
    Product,
    /// Draws the product frame's reserved box: base rectangle, then height, then `product.create`.
    ProductDraw,
    /// Draws the product frame's base outline as a closed ring, then pulls its height.
    ProductContour,
    /// Arms the free-outline tool whose closed ring becomes one panel cut to it.
    PanelContour,
    /// Arms the empty-space tool with `class`; its `fill` trait picks the size pair, and `mode`
    /// says whether the part takes the largest rectangle that fits or the whole section.
    Fill {
        /// The class a post or a shelf fills the void with.
        class: ClassCode,
        /// How the part reads the section the walls leave.
        mode: FillMode,
    },
    /// Creates an assembly at the scene root.
    Assembly,
    /// Draws a primitive's placement: the circle the shape is built around.
    ///
    /// The sphere cell carries the sketch's own circle methods, so a sphere is drawn around its
    /// centre by every method a circle offers and its centre and radius become its parameters
    /// (ADR 0139).
    Primitive {
        /// Which primitive the drawn figure builds.
        kind: PrimitiveKind,
        /// Which circle method draws the figure.
        tool: DrawTool,
    },
    /// Creates a camera at the scene root.
    Camera,
    /// Creates a light at the scene root.
    Light,
    /// Frames the whole scene.
    FrameAll,
    /// Switches between perspective and orthographic.
    Projection,
    /// Turns the work-plane grid snap on or off.
    GridSnap,
    /// Turns the ground shadow of the active view on or off (U107).
    Shadows,
    /// Arms the ruler: two clicks measure a length and write nothing (ADR 0118).
    Measure,
    /// Arms the niche tool: the pointer names the empty space inside the open product and writes
    /// nothing (ADR 0118).
    Void,
}

/// One input method of a tool: what U67 draws as the tool's list.
///
/// The method a cell runs today is [`ToolSpec::run`]; this list holds the others, so the map says
/// "a circle can also be drawn through three points" without repeating the primary row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VariantSpec {
    /// The namespaced id, `tab.group.tool.variant`.
    pub id: &'static str,
    /// The name the list draws.
    pub label: Message,
    /// The artwork, or `None` while the method is planned.
    pub icon: Option<Icon>,
    /// What picking the method runs, or `None` while its command does not exist.
    pub run: Option<ToolKind>,
}

/// One row of a tool's method list, the tool's own method first.
///
/// The list is derived, not stored twice: the tool contributes its own row and each
/// [`VariantSpec`] contributes one more, so the map stays the single description of a tool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Method {
    /// The namespaced id: the tool's own id for its first method, a variant id for the rest.
    pub id: &'static str,
    /// The name the list draws.
    pub label: Message,
    /// The artwork, or `None` while the method is planned or has none of its own.
    pub icon: Option<Icon>,
    /// The shortcut label, when a key names this method.
    pub shortcut: Option<&'static str>,
    /// What picking the method runs, or `None` while it is planned.
    pub run: Option<ToolKind>,
}

impl Method {
    /// Whether the method can run today.
    pub const fn is_live(&self) -> bool {
        self.run.is_some()
    }
}

/// The post's second method: take the whole section instead of the largest fit.
///
/// A fill cell draws as one split cell rather than two cells a maker has to tell apart: the method
/// list is the choice between fitting and taking the shape, and the pinned method is the one the
/// cell runs (U74, ADR 0110). The rows are two builds of one panel, not two panels, and each row
/// carries its own glyph so the shape is not a blank line (U84).
const FILL_POST_SHAPE: &[VariantSpec] = &[VariantSpec {
    id: "furniture.panels.post.shape",
    label: Message::FillMethodShape,
    icon: Some(icons::contour::POLYGON),
    run: Some(ToolKind::Fill { class: ClassCode::VERTICAL_PANEL, mode: FillMode::Shape }),
}];

/// The shelf's second method: take the whole section instead of the largest fit.
const FILL_SHELF_SHAPE: &[VariantSpec] = &[VariantSpec {
    id: "furniture.panels.shelf.shape",
    label: Message::FillMethodShape,
    icon: Some(icons::contour::POLYGON),
    run: Some(ToolKind::Fill { class: ClassCode::HORIZONTAL_PANEL, mode: FillMode::Shape }),
}];

/// One tool: one cell of a group's row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToolSpec {
    /// The namespaced id, `tab.group.tool`.
    pub id: &'static str,
    /// The name the hint and the accessible label carry.
    pub label: Message,
    /// The artwork, or `None` while the tool is planned.
    pub icon: Option<Icon>,
    /// The method a click runs today, or `None` while the tool is planned.
    pub run: Option<ToolKind>,
    /// The name of the tool's own method, when its list names methods rather than repeating the
    /// tool: a circle's first row is «Centre and radius», an arc's is «Through 2 points».
    pub method: Option<Message>,
    /// The tool's other input methods, drawn as a list from U67 on.
    pub variants: &'static [VariantSpec],
}

impl ToolSpec {
    /// The key that runs this cell, from the inventory's one table (U96).
    ///
    /// A cell is a method, so its key is filed under its own id: the map draws the words and the
    /// inventory says which key runs it, and neither keeps a second copy of the other.
    pub fn key(&self) -> Option<&'static str> {
        crate::inventory::key_label(self.id)
    }

    /// Whether the tool can run anything today, which is whether it is drawn at all.
    pub fn is_live(&self) -> bool {
        self.run.is_some() || self.variants.iter().any(|variant| variant.run.is_some())
    }

    /// The method a click on the cell runs today.
    pub fn primary(&self) -> Option<ToolKind> {
        self.run.or_else(|| self.variants.iter().find_map(|variant| variant.run))
    }

    /// The tool's own method, the first row of its list.
    pub fn default_method(&'static self) -> Method {
        Method {
            id: self.id,
            label: self.method.unwrap_or(self.label),
            icon: self.icon,
            shortcut: self.key(),
            run: self.run,
        }
    }

    /// Every input method in list order: the tool's own first, then its variants.
    pub fn methods(&'static self) -> impl Iterator<Item = Method> {
        std::iter::once(self.default_method()).chain(self.variants.iter().map(|variant| Method {
            id: variant.id,
            label: variant.label,
            icon: variant.icon,
            shortcut: crate::inventory::key_label(variant.id),
            run: variant.run,
        }))
    }

    /// The methods that can run today, in list order.
    pub fn live_methods(&'static self) -> impl Iterator<Item = Method> {
        self.methods().filter(Method::is_live)
    }

    /// Whether a click on the cell opens a list instead of running the tool.
    ///
    /// One method is no choice: a tool that only knows one way to be built runs on the click and
    /// draws no caret.
    pub fn has_list(&'static self) -> bool {
        self.methods().count() > 1
    }

    /// The index of the method the workspace pinned, or the first method.
    ///
    /// A pinned id this build does not know — a setting written by a newer one, or a method that
    /// moved — falls back to the first method instead of leaving the tool unusable.
    pub fn pinned_index(&'static self, pinned: &std::collections::BTreeMap<String, String>) -> usize {
        let Some(pinned) = pinned.get(self.id) else { return 0 };
        self.methods().position(|method| method.id == pinned.as_str()).unwrap_or(0)
    }
}

/// The method id the map gives the drawing tool `tool`, which is where its key lives (U96).
///
/// The map already ties a tool to its method through [`ToolSpec::run`] and its variants; this is the
/// same tie read in the other direction, so the shell can ask for one tool's key without walking the
/// whole map every frame. A test holds the two together.
pub const fn method_id(tool: DrawTool) -> &'static str {
    match tool {
        DrawTool::Line => "sketch.create.line",
        DrawTool::LineByAngle => "sketch.create.line.angle",
        DrawTool::Polyline => "sketch.create.polyline",
        DrawTool::Arc => "sketch.create.arc",
        DrawTool::ArcFromCenter => "sketch.create.arc.centre",
        DrawTool::ArcByRadius => "sketch.create.arc.start-end-radius",
        DrawTool::Rectangle => "sketch.create.rectangle",
        DrawTool::RectangleFromCenter => "sketch.create.rectangle.centre",
        DrawTool::RectangleFromCenterDirected => "sketch.create.rectangle.centre-directed",
        DrawTool::RectangleTilted => "sketch.create.rectangle.edge",
        DrawTool::Circle => "sketch.create.circle",
        DrawTool::CircleDiameter => "sketch.create.circle.diameter",
        DrawTool::CircleThreePoints => "sketch.create.circle.three-points",
        DrawTool::CircleTangent => "sketch.create.circle.tangent",
        DrawTool::Sphere => "model.primitives.sphere",
        DrawTool::SphereDiameter => "model.primitives.sphere.diameter",
        DrawTool::SphereThreePoints => "model.primitives.sphere.three-points",
        DrawTool::Cone => "model.primitives.cone",
        DrawTool::ConeDiameter => "model.primitives.cone.diameter",
        DrawTool::ConeThreePoints => "model.primitives.cone.three-points",
        DrawTool::RegularPolygon => "sketch.create.polygon",
        DrawTool::PolygonApothem => "sketch.create.polygon.apothem",
        DrawTool::PolygonByEdge => "sketch.create.polygon.edge",
    }
}

/// The drawing tool a method id starts, if the method draws.
///
/// The shell walks the drawing tools, so this is the inverse of [`method_id`] over the same ten: a
/// method that does something else returns `None` and a key that names it is not a drawing key.
pub fn drawing_method(id: &str) -> Option<DrawTool> {
    crate::toolbars::CONTOUR_TOOLS.into_iter().find(|tool| method_id(*tool) == id)
}

/// The words a tool or one of its variants prints, by its id.
///
/// The Settings page walks the key table and asks this for every id that is not a command and not a
/// shell action, so a key never describes a method in words of its own (U96).
pub fn words(id: &str) -> Option<Message> {
    for tab in tabs() {
        for group in tab.groups {
            for tool in group.tools {
                if tool.id == id {
                    return Some(tool.label);
                }
                if let Some(variant) = tool.variants.iter().find(|variant| variant.id == id) {
                    return Some(variant.label);
                }
            }
        }
    }
    None
}

/// One captioned group inside a tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GroupSpec {
    /// The namespaced id, `tab.group`.
    pub id: &'static str,
    /// The caption under the group's row, drawn in capitals.
    pub label: Message,
    /// The group's tools in drawing order.
    pub tools: &'static [ToolSpec],
}

impl GroupSpec {
    /// Whether the group draws anything: a group of only planned tools is not drawn.
    pub fn is_live(&self) -> bool {
        self.tools.iter().any(ToolSpec::is_live)
    }

    /// The number of tool cells the group's row takes: one per live tool.
    ///
    /// A tool with more than one method also asks for the caret column beside its cell, so the drawn
    /// row is wider than this count by one caret per listed tool (X36).
    pub fn columns(&self) -> usize {
        self.tools.iter().filter(|tool| tool.is_live()).count()
    }

    /// The live tools of the group, in drawing order.
    pub fn live_tools(&'static self) -> impl Iterator<Item = &'static ToolSpec> {
        self.tools.iter().filter(|tool| tool.is_live())
    }
}

/// One tab of the ribbon: the surface a domain's tools belong to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TabSpec {
    /// The namespaced id, `tab`.
    pub id: &'static str,
    /// The name drawn on the tab button.
    pub label: Message,
    /// The groups in row order.
    pub groups: &'static [GroupSpec],
}

impl TabSpec {
    /// Whether the tab is drawn at all: a tab with no live group is not.
    pub fn is_drawn(&'static self) -> bool {
        self.groups.iter().any(GroupSpec::is_live)
    }

    /// The number of tool cells the tab's body takes, one per live tool of its groups.
    ///
    /// A split cell adds its caret column beside its cell, so this is the pitch the row keeps and not
    /// its drawn width (X36).
    pub fn columns(&'static self) -> usize {
        self.live_groups().map(GroupSpec::columns).sum()
    }

    /// The groups the body draws, in order; a group of only planned tools is skipped.
    pub fn live_groups(&'static self) -> impl Iterator<Item = &'static GroupSpec> {
        self.groups.iter().filter(|group| group.is_live())
    }
}

/// Every tab, in ribbon order.
pub static TABS: [TabSpec; 5] = [
    TabSpec { id: "sketch", label: Message::RibbonSketch, groups: &SKETCH },
    TabSpec { id: "model", label: Message::RibbonModel, groups: &MODEL },
    TabSpec { id: "furniture", label: Message::RibbonFurniture, groups: &FURNITURE },
    TabSpec { id: "scene", label: Message::RibbonScene, groups: &SCENE },
    TabSpec { id: "view", label: Message::RibbonView, groups: &VIEW },
];

/// Every tab, in ribbon order.
pub fn tabs() -> &'static [TabSpec] {
    &TABS
}

/// The tab with `id`, or `None` when no such tab exists.
pub fn tab(id: &str) -> Option<&'static TabSpec> {
    TABS.iter().find(|tab| tab.id == id)
}

/// The first tab that draws a body, which is where a fresh ribbon opens and where a ribbon falls
/// back to when its own tab has nothing live.
pub fn fallback_tab() -> Option<&'static TabSpec> {
    TABS.iter().find(|tab| tab.is_drawn())
}

/// The tool with `id`, or `None` when the map does not declare it.
pub fn tool(id: &str) -> Option<&'static ToolSpec> {
    for tab in tabs() {
        for group in tab.groups {
            for tool in group.tools {
                if tool.id == id {
                    return Some(tool);
                }
            }
        }
    }
    None
}

// ── The map ─────────────────────────────────────────────────────────────────

const SKETCH: [GroupSpec; 7] = [
    GroupSpec {
        id: "sketch.create",
        label: Message::GroupCreate,
        tools: &[
            ToolSpec {
                id: "sketch.create.line",
                label: Message::ToolLine,
                icon: Some(icons::contour::LINE),
                run: Some(ToolKind::Draw(DrawTool::Line)),
                method: Some(Message::ToolLineTwoPoints),
                variants: &[VariantSpec {
                    id: "sketch.create.line.angle",
                    label: Message::ToolLineAngle,
                    icon: Some(icons::contour::LINE_ANGLE),
                    run: Some(ToolKind::Draw(DrawTool::LineByAngle)),
                }],
            },
            ToolSpec {
                id: "sketch.create.polyline",
                label: Message::ToolPolyline,
                icon: Some(icons::contour::POLYLINE),
                run: Some(ToolKind::Draw(DrawTool::Polyline)),
                method: None,
                variants: &[],
            },
            ToolSpec {
                id: "sketch.create.rectangle",
                label: Message::ToolRectangle,
                icon: Some(icons::contour::RECTANGLE),
                run: Some(ToolKind::Draw(DrawTool::Rectangle)),
                method: Some(Message::ToolRectangleFromCorner),
                variants: &[
                    VariantSpec {
                        id: "sketch.create.rectangle.centre",
                        label: Message::ToolRectangleFromCentre,
                        icon: Some(icons::contour::RECTANGLE_CENTRE),
                        run: Some(ToolKind::Draw(DrawTool::RectangleFromCenter)),
                    },
                    VariantSpec {
                        id: "sketch.create.rectangle.centre-directed",
                        label: Message::ToolRectangleFromCentreDirected,
                        icon: Some(icons::contour::RECTANGLE_CENTRE_DIRECTED),
                        run: Some(ToolKind::Draw(DrawTool::RectangleFromCenterDirected)),
                    },
                    VariantSpec {
                        id: "sketch.create.rectangle.edge",
                        label: Message::ToolRectangleFromEdge,
                        icon: Some(icons::contour::RECTANGLE_EDGE),
                        run: Some(ToolKind::Draw(DrawTool::RectangleTilted)),
                    },
                ],
            },
            ToolSpec {
                id: "sketch.create.circle",
                label: Message::ToolCircle,
                icon: Some(icons::contour::CIRCLE),
                run: Some(ToolKind::Draw(DrawTool::Circle)),
                method: Some(Message::ToolCircleCentreRadius),
                variants: &[
                    VariantSpec {
                        id: "sketch.create.circle.diameter",
                        label: Message::ToolCircleCentreDiameter,
                        icon: Some(icons::contour::CIRCLE_DIAMETER),
                        run: Some(ToolKind::Draw(DrawTool::CircleDiameter)),
                    },
                    VariantSpec {
                        id: "sketch.create.circle.three-points",
                        label: Message::ToolCircleThreePoints,
                        icon: Some(icons::contour::CIRCLE_THREE_POINTS),
                        run: Some(ToolKind::Draw(DrawTool::CircleThreePoints)),
                    },
                    VariantSpec {
                        id: "sketch.create.circle.tangent",
                        label: Message::ToolCircleTangent,
                        icon: Some(icons::contour::CIRCLE_TANGENT),
                        run: Some(ToolKind::Draw(DrawTool::CircleTangent)),
                    },
                ],
            },
            ToolSpec {
                id: "sketch.create.arc",
                label: Message::ToolArc,
                icon: Some(icons::contour::ARC),
                run: Some(ToolKind::Draw(DrawTool::Arc)),
                method: Some(Message::ToolArcThreePoints),
                variants: &[
                    VariantSpec {
                        id: "sketch.create.arc.centre",
                        label: Message::ToolArcFromCentre,
                        icon: Some(icons::contour::ARC_CENTRE),
                        run: Some(ToolKind::Draw(DrawTool::ArcFromCenter)),
                    },
                    VariantSpec {
                        id: "sketch.create.arc.start-end-radius",
                        label: Message::ToolArcStartEndRadius,
                        icon: Some(icons::contour::ARC_RADIUS),
                        run: Some(ToolKind::Draw(DrawTool::ArcByRadius)),
                    },
                ],
            },
            ToolSpec {
                id: "sketch.create.polygon",
                label: Message::ToolRegularPolygon,
                icon: Some(icons::contour::POLYGON),
                run: Some(ToolKind::Draw(DrawTool::RegularPolygon)),
                method: Some(Message::ToolPolygonRadius),
                variants: &[
                    VariantSpec {
                        id: "sketch.create.polygon.apothem",
                        label: Message::ToolPolygonApothem,
                        icon: Some(icons::contour::POLYGON_APOTHEM),
                        run: Some(ToolKind::Draw(DrawTool::PolygonApothem)),
                    },
                    VariantSpec {
                        id: "sketch.create.polygon.edge",
                        label: Message::ToolPolygonEdge,
                        icon: Some(icons::contour::POLYGON_EDGE),
                        run: Some(ToolKind::Draw(DrawTool::PolygonByEdge)),
                    },
                ],
            },
        ],
    },
    GroupSpec {
        id: "sketch.modify",
        label: Message::GroupModify,
        tools: &[
            planned("sketch.modify.trim", Message::ToolTrim),
            planned("sketch.modify.extend", Message::ToolExtend),
            ToolSpec {
                id: "sketch.modify.offset",
                label: Message::ToolOffset,
                icon: None,
                run: None,
                method: None,
                variants: &[
                    VariantSpec {
                        id: "sketch.modify.offset.distance",
                        label: Message::ToolOffsetDistance,
                        icon: None,
                        run: None,
                    },
                    VariantSpec {
                        id: "sketch.modify.offset.through",
                        label: Message::ToolOffsetThrough,
                        icon: None,
                        run: None,
                    },
                ],
            },
            ToolSpec {
                id: "sketch.modify.fillet",
                label: Message::ToolFillet,
                icon: None,
                run: None,
                method: None,
                variants: &[
                    VariantSpec {
                        id: "sketch.modify.fillet.radius",
                        label: Message::ToolFilletRadius,
                        icon: None,
                        run: None,
                    },
                    VariantSpec {
                        id: "sketch.modify.fillet.two-lines",
                        label: Message::ToolFilletTwoLines,
                        icon: None,
                        run: None,
                    },
                ],
            },
            ToolSpec {
                id: "sketch.modify.chamfer",
                label: Message::ToolChamfer,
                icon: None,
                run: None,
                method: None,
                variants: &[
                    VariantSpec {
                        id: "sketch.modify.chamfer.distance",
                        label: Message::ToolChamferDistance,
                        icon: None,
                        run: None,
                    },
                    VariantSpec {
                        id: "sketch.modify.chamfer.two-lines",
                        label: Message::ToolChamferTwoLines,
                        icon: None,
                        run: None,
                    },
                ],
            },
            planned("sketch.modify.break", Message::ToolBreak),
        ],
    },
    GroupSpec {
        id: "sketch.construction",
        label: Message::GroupConstruction,
        tools: &[
            planned("sketch.construction.horizontal", Message::ToolConstructionHorizontal),
            planned("sketch.construction.vertical", Message::ToolConstructionVertical),
            planned("sketch.construction.parallel", Message::ToolConstructionParallel),
            planned("sketch.construction.angle", Message::ToolConstructionAngle),
        ],
    },
    GroupSpec {
        id: "sketch.dimensions",
        label: Message::GroupDimensions,
        tools: &[
            planned("sketch.dimensions.linear", Message::ToolDimensionLinear),
            planned("sketch.dimensions.angular", Message::ToolDimensionAngular),
            ToolSpec {
                id: "sketch.dimensions.radius",
                label: Message::ToolDimensionRadius,
                icon: None,
                run: None,
                method: None,
                variants: &[VariantSpec {
                    id: "sketch.dimensions.radius.diameter",
                    label: Message::ToolDiameter,
                    icon: None,
                    run: None,
                }],
            },
        ],
    },
    GroupSpec {
        id: "sketch.measure",
        label: Message::GroupMeasure,
        tools: &[ToolSpec {
            id: "sketch.measure.length",
            label: Message::ToolMeasure,
            icon: Some(icons::contour::MEASURE),
            run: Some(ToolKind::Measure),
            method: Some(Message::ToolMeasureLength),
            variants: &[],
        }],
    },
    GroupSpec {
        id: "sketch.booleans",
        label: Message::GroupBooleans,
        tools: &[
            planned("sketch.booleans.union", Message::ToolUnion),
            planned("sketch.booleans.subtract", Message::ToolSubtract),
            planned("sketch.booleans.intersect", Message::ToolIntersect),
        ],
    },
    GroupSpec {
        id: "sketch.draft",
        label: Message::GroupDraft,
        tools: &[
            ToolSpec {
                id: "sketch.draft.close",
                label: Message::Close,
                icon: Some(icons::contour::CLOSE),
                run: Some(ToolKind::CloseDraft),
                method: None,
                variants: &[],
            },
            ToolSpec {
                id: "sketch.draft.cancel",
                label: Message::Cancel,
                icon: Some(icons::contour::CANCEL),
                run: Some(ToolKind::CancelDraft),
                method: None,
                variants: &[],
            },
        ],
    },
];

const MODEL: [GroupSpec; 5] = [
    GroupSpec {
        id: "model.primitives",
        label: Message::GroupPrimitives,
        tools: &[
            ToolSpec {
                id: "model.primitives.cuboid",
                label: Message::ToolBox,
                icon: Some(icons::primitives::CUBOID),
                run: Some(ToolKind::Body(DrawTool::Rectangle)),
                method: Some(Message::ToolRectangleFromCorner),
                variants: &[
                    VariantSpec {
                        id: "model.primitives.cuboid.centre",
                        label: Message::ToolRectangleFromCentre,
                        icon: Some(icons::contour::RECTANGLE_CENTRE),
                        run: Some(ToolKind::Body(DrawTool::RectangleFromCenter)),
                    },
                    VariantSpec {
                        id: "model.primitives.cuboid.centre-directed",
                        label: Message::ToolRectangleFromCentreDirected,
                        icon: Some(icons::contour::RECTANGLE_CENTRE_DIRECTED),
                        run: Some(ToolKind::Body(DrawTool::RectangleFromCenterDirected)),
                    },
                    VariantSpec {
                        id: "model.primitives.cuboid.edge",
                        label: Message::ToolRectangleFromEdge,
                        icon: Some(icons::contour::RECTANGLE_EDGE),
                        run: Some(ToolKind::Body(DrawTool::RectangleTilted)),
                    },
                ],
            },
            ToolSpec {
                id: "model.primitives.cylinder",
                label: Message::ToolCylinder,
                icon: Some(icons::primitives::CYLINDER),
                run: Some(ToolKind::Body(DrawTool::Circle)),
                method: Some(Message::ToolCircleCentreRadius),
                variants: &[
                    VariantSpec {
                        id: "model.primitives.cylinder.diameter",
                        label: Message::ToolCircleCentreDiameter,
                        icon: Some(icons::contour::CIRCLE_DIAMETER),
                        run: Some(ToolKind::Body(DrawTool::CircleDiameter)),
                    },
                    VariantSpec {
                        id: "model.primitives.cylinder.three-points",
                        label: Message::ToolCircleThreePoints,
                        icon: Some(icons::contour::CIRCLE_THREE_POINTS),
                        run: Some(ToolKind::Body(DrawTool::CircleThreePoints)),
                    },
                    VariantSpec {
                        id: "model.primitives.cylinder.tangent",
                        label: Message::ToolCircleTangent,
                        icon: Some(icons::contour::CIRCLE_TANGENT),
                        run: Some(ToolKind::Body(DrawTool::CircleTangent)),
                    },
                ],
            },
            ToolSpec {
                id: "model.primitives.sphere",
                label: Message::ToolSphere,
                icon: Some(icons::primitives::SPHERE),
                run: Some(ToolKind::Primitive { kind: PrimitiveKind::Sphere, tool: DrawTool::Sphere }),
                method: Some(Message::ToolCentreRadius),
                variants: &[
                    VariantSpec {
                        id: "model.primitives.sphere.diameter",
                        label: Message::ToolCentreDiameter,
                        icon: Some(icons::contour::CIRCLE_DIAMETER),
                        run: Some(ToolKind::Primitive { kind: PrimitiveKind::Sphere, tool: DrawTool::SphereDiameter }),
                    },
                    VariantSpec {
                        id: "model.primitives.sphere.three-points",
                        label: Message::ToolThreePoints,
                        icon: Some(icons::contour::CIRCLE_THREE_POINTS),
                        run: Some(ToolKind::Primitive {
                            kind: PrimitiveKind::Sphere,
                            tool: DrawTool::SphereThreePoints,
                        }),
                    },
                ],
            },
            ToolSpec {
                id: "model.primitives.cone",
                label: Message::ToolCone,
                icon: Some(icons::primitives::CONE),
                run: Some(ToolKind::Primitive { kind: PrimitiveKind::Cone, tool: DrawTool::Cone }),
                method: Some(Message::ToolCentreRadius),
                variants: &[
                    VariantSpec {
                        id: "model.primitives.cone.diameter",
                        label: Message::ToolCentreDiameter,
                        icon: Some(icons::contour::CIRCLE_DIAMETER),
                        run: Some(ToolKind::Primitive { kind: PrimitiveKind::Cone, tool: DrawTool::ConeDiameter }),
                    },
                    VariantSpec {
                        id: "model.primitives.cone.three-points",
                        label: Message::ToolThreePoints,
                        icon: Some(icons::contour::CIRCLE_THREE_POINTS),
                        run: Some(ToolKind::Primitive { kind: PrimitiveKind::Cone, tool: DrawTool::ConeThreePoints }),
                    },
                ],
            },
            planned("model.primitives.pyramid", Message::ToolPyramid),
        ],
    },
    GroupSpec {
        id: "model.create",
        label: Message::GroupCreate,
        tools: &[
            ToolSpec {
                id: "model.create.extrude",
                label: Message::ToolExtrude,
                icon: Some(icons::bodies::EXTRUDE),
                run: Some(ToolKind::Extrude),
                method: None,
                variants: &[],
            },
            planned("model.create.revolve", Message::ToolRevolve),
            planned("model.create.loft", Message::ToolLoft),
            planned("model.create.sweep", Message::ToolSweep),
        ],
    },
    GroupSpec {
        id: "model.booleans",
        label: Message::GroupBooleans,
        tools: &[
            planned("model.booleans.union", Message::ToolUnion),
            planned("model.booleans.subtract", Message::ToolSubtract),
            planned("model.booleans.intersect", Message::ToolIntersect),
        ],
    },
    GroupSpec {
        id: "model.faces",
        label: Message::GroupFaces,
        tools: &[
            planned("model.faces.chamfer", Message::ToolChamfer),
            planned("model.faces.fillet", Message::ToolFillet),
            planned("model.faces.shell", Message::ToolShell),
            planned("model.faces.offset", Message::ToolOffsetFace),
            planned("model.faces.delete", Message::ToolDeleteFace),
        ],
    },
    GroupSpec {
        id: "model.bodies",
        label: Message::GroupBodies,
        tools: &[
            planned("model.bodies.join", Message::ToolJoin),
            planned("model.bodies.split", Message::ToolSplit),
            planned("model.bodies.scale", Message::ToolScale),
            planned("model.bodies.mirror", Message::ToolMirror),
        ],
    },
];

const FURNITURE: [GroupSpec; 6] = [
    GroupSpec {
        id: "furniture.product",
        label: Message::GroupProduct,
        tools: &[ToolSpec {
            id: "furniture.product.frame",
            label: Message::ToolProductFrame,
            icon: Some(icons::scene::PRODUCT),
            run: Some(ToolKind::Product),
            method: None,
            // The card is the numeric method and the drawn box the pointer method; the split cell
            // runs whichever the workspace pinned (U74, ADR 0101).
            variants: &[
                VariantSpec {
                    id: "furniture.product.frame.draw",
                    label: Message::ToolProductFrameDraw,
                    icon: Some(icons::scene::PRODUCT),
                    run: Some(ToolKind::ProductDraw),
                },
                VariantSpec {
                    id: "furniture.product.frame.contour",
                    label: Message::ToolProductFrameContour,
                    icon: Some(icons::scene::PRODUCT),
                    run: Some(ToolKind::ProductContour),
                },
            ],
        }],
    },
    GroupSpec {
        id: "furniture.panels",
        label: Message::GroupPanels,
        tools: &[
            ToolSpec {
                id: "furniture.panels.post",
                label: Message::ToolPost,
                icon: Some(icons::scene::POST),
                run: Some(ToolKind::Fill { class: ClassCode::VERTICAL_PANEL, mode: FillMode::Fit }),
                method: Some(Message::FillMethodFit),
                variants: FILL_POST_SHAPE,
            },
            ToolSpec {
                id: "furniture.panels.shelf",
                label: Message::ToolShelf,
                icon: Some(icons::scene::SHELF),
                run: Some(ToolKind::Fill { class: ClassCode::HORIZONTAL_PANEL, mode: FillMode::Fit }),
                method: Some(Message::FillMethodFit),
                variants: FILL_SHELF_SHAPE,
            },
            // The niche is the space the two cells above fill: it sits with them because a maker
            // reads the gap and the part together, and it creates nothing (ADR 0118).
            ToolSpec {
                id: "furniture.panels.void",
                label: Message::ToolVoid,
                icon: Some(icons::scene::VOID),
                run: Some(ToolKind::Void),
                method: None,
                variants: &[],
            },
            // The panel is drawn, not typed: a board outside a niche has no section to fill, so its
            // outline comes from the pointer and the close cuts it (ADR 0114).
            ToolSpec {
                id: "furniture.panels.contour",
                label: Message::ToolContourPanel,
                icon: Some(icons::scene::PANEL),
                run: Some(ToolKind::PanelContour),
                method: None,
                variants: &[],
            },
        ],
    },
    GroupSpec {
        id: "furniture.machining",
        label: Message::GroupMachining,
        tools: &[
            planned("furniture.machining.drill", Message::ToolDrill),
            planned("furniture.machining.groove", Message::ToolGroove),
            planned("furniture.machining.edgeband", Message::ToolEdgeband),
            planned("furniture.machining.hinge-hole", Message::ToolHingeHole),
        ],
    },
    GroupSpec {
        id: "furniture.fasteners",
        label: Message::GroupFasteners,
        tools: &[
            planned("furniture.fasteners.confirmat", Message::ToolConfirmat),
            planned("furniture.fasteners.cam", Message::ToolCam),
            planned("furniture.fasteners.dowel", Message::ToolDowel),
            planned("furniture.fasteners.minifix", Message::ToolMinifix),
            planned("furniture.fasteners.shelf-pin", Message::ToolShelfPin),
        ],
    },
    GroupSpec {
        id: "furniture.hardware",
        label: Message::GroupHardware,
        tools: &[
            planned("furniture.hardware.hinge", Message::ToolHinge),
            planned("furniture.hardware.runner", Message::ToolRunner),
            planned("furniture.hardware.handle", Message::ToolHandle),
            planned("furniture.hardware.foot", Message::ToolFoot),
        ],
    },
    GroupSpec {
        id: "furniture.checks",
        label: Message::GroupChecks,
        tools: &[
            planned("furniture.checks.collisions", Message::ToolCollisions),
            planned("furniture.checks.parts-list", Message::ToolPartsList),
            planned("furniture.checks.cut-list", Message::ToolCutList),
        ],
    },
];

const SCENE: [GroupSpec; 3] = [
    GroupSpec {
        id: "scene.objects",
        label: Message::GroupObjects,
        tools: &[
            ToolSpec {
                id: "scene.objects.camera",
                label: Message::ToolCamera,
                icon: Some(icons::scene::CAMERA),
                run: Some(ToolKind::Camera),
                method: None,
                variants: &[],
            },
            ToolSpec {
                id: "scene.objects.light",
                label: Message::ToolLight,
                icon: Some(icons::scene::LIGHT),
                run: Some(ToolKind::Light),
                method: None,
                variants: &[],
            },
            ToolSpec {
                id: "scene.objects.assembly",
                label: Message::ToolAssembly,
                icon: Some(icons::scene::ASSEMBLY),
                run: Some(ToolKind::Assembly),
                method: None,
                variants: &[],
            },
            planned("scene.objects.group", Message::ToolGroupNode),
        ],
    },
    GroupSpec {
        id: "scene.views",
        label: Message::GroupViews,
        tools: &[
            planned("scene.views.named", Message::ToolNamedView),
            planned("scene.views.standard", Message::ToolStandardView),
        ],
    },
    GroupSpec {
        id: "scene.materials",
        label: Message::GroupMaterials,
        tools: &[
            planned("scene.materials.catalogue", Message::ToolMaterialCatalogue),
            planned("scene.materials.assign", Message::ToolAssignMaterial),
        ],
    },
];

const VIEW: [GroupSpec; 5] = [
    GroupSpec {
        id: "view.projection",
        label: Message::GroupProjection,
        tools: &[
            ToolSpec {
                id: "view.projection.toggle",
                label: Message::ToolProjection,
                icon: Some(icons::view::ORTHOGRAPHIC),
                run: Some(ToolKind::Projection),
                method: None,
                variants: &[],
            },
            ToolSpec {
                id: "view.projection.frame-all",
                label: Message::ToolFrameAll,
                icon: Some(icons::view::FRAME_ALL),
                run: Some(ToolKind::FrameAll),
                method: None,
                variants: &[],
            },
        ],
    },
    GroupSpec {
        id: "view.display",
        label: Message::GroupDisplay,
        tools: &[
            planned("view.display.style", Message::ToolStyle),
            planned("view.display.contours", Message::ToolContours),
            // The shadow is a view mode: its switch sits in this group beside the intensity field
            // the group draws, and one cell of a split may draw it while its neighbour does not
            // (U107).
            ToolSpec {
                id: "view.display.shadows",
                label: Message::ToolShadows,
                icon: Some(icons::view::SHADOWS),
                run: Some(ToolKind::Shadows),
                method: None,
                variants: &[],
            },
            planned("view.display.section", Message::ToolSection),
        ],
    },
    GroupSpec {
        id: "view.layout",
        label: Message::GroupLayout,
        tools: &[
            planned("view.layout.one", Message::ToolLayoutOne),
            planned("view.layout.two", Message::ToolLayoutTwo),
            planned("view.layout.four", Message::ToolLayoutFour),
            planned("view.layout.maximize", Message::ToolMaximize),
        ],
    },
    GroupSpec {
        id: "view.navigation",
        label: Message::GroupNavigation,
        tools: &[
            planned("view.navigation.orbit", Message::ToolOrbit),
            planned("view.navigation.pan", Message::ToolPan),
            planned("view.navigation.zoom", Message::ToolZoom),
        ],
    },
    GroupSpec {
        id: "view.snaps",
        label: Message::GroupSnaps,
        tools: &[ToolSpec {
            id: "view.snaps.grid",
            label: Message::ToolGridSnap,
            icon: Some(icons::view::GRID_SNAP),
            run: Some(ToolKind::GridSnap),
            method: None,
            variants: &[],
        }],
    },
];

/// A tool whose command does not exist yet: it holds its place and its name in the map.
const fn planned(id: &'static str, label: Message) -> ToolSpec {
    ToolSpec { id, label, icon: None, run: None, method: None, variants: &[] }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use std::collections::HashSet;

    use super::*;

    /// Every tool of the map, live or planned, with the group it sits in.
    fn every_tool() -> Vec<(&'static GroupSpec, &'static ToolSpec)> {
        let mut tools = Vec::new();
        for tab in tabs() {
            for group in tab.groups {
                for tool in group.tools {
                    tools.push((group, tool));
                }
            }
        }
        tools
    }

    #[test]
    fn the_five_tabs_are_the_approved_nouns_in_order() {
        let names = tabs().iter().map(|tab| tab.label.text(text::Lang::En)).collect::<Vec<_>>();
        assert_eq!(names, ["Sketch", "Model", "Furniture", "Scene", "View"]);
        assert_eq!(tabs()[0].id, "sketch");
        assert_eq!(tab("furniture").map(|tab| tab.id), Some("furniture"));
        assert_eq!(tab("hardware"), None, "hardware is a group of furniture, not a tab");
        assert_eq!(fallback_tab().map(|tab| tab.id), Some("sketch"), "a fresh ribbon opens on the first drawn tab");
    }

    #[test]
    fn every_id_is_unique_and_namespaced_by_its_parent() {
        let mut seen = HashSet::new();
        for tab in tabs() {
            assert!(seen.insert(tab.id), "{} is declared twice", tab.id);
            assert!(!tab.id.contains('.'), "a tab id is one word: {}", tab.id);
            for group in tab.groups {
                assert!(
                    group.id.starts_with(tab.id) && group.id.contains('.'),
                    "{} does not sit in {}",
                    group.id,
                    tab.id
                );
                assert!(seen.insert(group.id), "{} is declared twice", group.id);
                for tool in group.tools {
                    let id = tool.id;
                    assert!(id.starts_with(group.id), "{id} does not sit in {}", group.id);
                    assert!(seen.insert(id), "{id} is declared twice");
                }
                for tool in group.tools {
                    for variant in tool.variants {
                        assert!(variant.id.starts_with(tool.id), "{} does not sit in {}", variant.id, tool.id);
                        assert!(seen.insert(variant.id), "{} is declared twice", variant.id);
                    }
                }
            }
        }
    }

    #[test]
    fn every_tool_is_either_live_or_planned_and_a_group_of_only_planned_tools_is_skipped() {
        let (mut live, mut planned_tools) = (0, 0);
        for (_, tool) in every_tool() {
            if tool.is_live() {
                live += 1;
                assert!(tool.primary().is_some(), "{} is live but runs nothing", tool.id);
                assert!(tool.icon.is_some(), "{} is live and has no artwork", tool.id);
            } else {
                planned_tools += 1;
                assert!(tool.primary().is_none());
            }
        }
        assert!(planned_tools > 0, "the map keeps room for the tools that are not built yet");
        assert!(live > 0, "the ribbon draws at least one live tool");
        for tab in tabs() {
            for group in tab.groups {
                assert_eq!(group.is_live(), group.columns() > 0, "{} is live exactly when it draws a cell", group.id);
                if !group.is_live() {
                    assert_eq!(group.live_tools().count(), 0, "{} is not live but counts a tool", group.id);
                }
            }
        }
        // A tab is drawn exactly when it owns a live group, so a planned-only tab cannot show.
        for tab in tabs() {
            assert_eq!(tab.is_drawn(), tab.live_groups().count() > 0, "{} disagrees with its groups", tab.id);
            if tab.is_drawn() {
                assert!(tab.columns() > 0);
            }
        }
    }

    #[test]
    fn every_live_row_of_the_approved_map_is_on_the_ribbon_today() {
        // The map is the contract: a tab, its groups and the tools the build can run.
        let drawn = |id: &str| tab(id).is_some_and(TabSpec::is_drawn);
        let live = |tab_id: &str| {
            let tab = tab(tab_id).expect("the tab exists");
            let mut ids = tab.live_groups().flat_map(GroupSpec::live_tools).map(|tool| tool.id).collect::<Vec<_>>();
            ids.sort_unstable();
            ids
        };
        assert!(drawn("sketch") && drawn("model") && drawn("furniture") && drawn("scene") && drawn("view"));
        assert_eq!(
            live("sketch"),
            [
                "sketch.create.arc",
                "sketch.create.circle",
                "sketch.create.line",
                "sketch.create.polygon",
                "sketch.create.polyline",
                "sketch.create.rectangle",
                "sketch.draft.cancel",
                "sketch.draft.close",
                "sketch.measure.length",
            ]
        );
        assert_eq!(
            live("model"),
            [
                "model.create.extrude",
                "model.primitives.cone",
                "model.primitives.cuboid",
                "model.primitives.cylinder",
                "model.primitives.sphere",
            ]
        );
        assert_eq!(
            live("furniture"),
            [
                "furniture.panels.contour",
                "furniture.panels.post",
                "furniture.panels.shelf",
                "furniture.panels.void",
                "furniture.product.frame",
            ]
        );
        assert_eq!(live("scene"), ["scene.objects.assembly", "scene.objects.camera", "scene.objects.light"]);
        assert_eq!(
            live("view"),
            ["view.display.shadows", "view.projection.frame-all", "view.projection.toggle", "view.snaps.grid"]
        );
    }

    #[test]
    fn every_drawing_tool_names_the_method_the_map_gives_it() {
        // The key of a drawing tool is filed under the method id the tool draws, so the two maps —
        // the tool's kind and the method's id — must name the same pair (U96). The tie is read from
        // the map first, so a method a cell's list starts and no key owns — the line by its angle is
        // one — is held to the same table (X53).
        for (_, spec) in every_tool() {
            for method in spec.methods() {
                if let Some(ToolKind::Draw(tool)) = method.run {
                    assert_eq!(method_id(tool), method.id, "`{}` answers another id", method.id);
                }
            }
        }
        for tool in crate::toolbars::CONTOUR_TOOLS {
            let id = method_id(tool);
            let method = every_tool()
                .into_iter()
                .flat_map(|(_, tool)| tool.methods())
                .find(|method| method.id == id)
                .unwrap_or_else(|| panic!("`{id}` is no method of the map"));
            assert_eq!(method.run, Some(ToolKind::Draw(tool)), "`{id}` does not draw the tool it is filed under");
            assert_eq!(drawing_method(id), Some(tool), "`{id}` answers no drawing tool");
            assert!(method.shortcut.is_some(), "`{id}` has no key in the inventory's table");
        }
    }

    #[test]
    fn the_grid_snap_is_filed_under_its_own_snaps_caption() {
        // The snap changes where a click lands, not how the scene looks, so it left *View › Display*
        // for a group the endpoint, the axis and the object snaps can join (U91).
        let view = tab("view").expect("the view tab");
        let snaps = view.groups.iter().find(|group| group.id == "view.snaps").expect("the snaps group");
        assert_eq!(snaps.label.text(text::Lang::En), "Snaps");
        assert_eq!(snaps.label.text(text::Lang::Ru), "Привязки");
        let tools = snaps.tools.iter().map(|tool| (tool.id, tool.run, tool.key())).collect::<Vec<_>>();
        assert_eq!(tools, vec![("view.snaps.grid", Some(ToolKind::GridSnap), Some("G"))], "the G key and the cell");
        let display = view.groups.iter().find(|group| group.id == "view.display").expect("the display group");
        assert!(
            display.tools.iter().all(|tool| tool.run != Some(ToolKind::GridSnap)),
            "the snap is still filed under Display"
        );
        // The Display group holds the shadow's switch and no number: the intensity is a workspace
        // value and its one control is the slider in Settings › View (U110).
        let shadows = display.tools.iter().find(|tool| tool.id == "view.display.shadows").expect("the shadow cell");
        assert_eq!(shadows.run, Some(ToolKind::Shadows));
        assert_eq!(shadows.icon, Some(icons::view::SHADOWS));
        assert_eq!(display.columns(), 1, "the switch is the group's only cell so far");
    }

    #[test]
    fn the_model_tab_offers_a_box_and_names_no_cube() {
        // The one solid primitive is a box with three independent sides; the cube, its edge and its
        // message left the ribbon with schema 23 (ADR 0101).
        let model = tab("model").expect("the model tab");
        for tool in model.live_groups().flat_map(GroupSpec::live_tools) {
            let label = tool.label.text(text::Lang::En);
            assert!(!tool.id.contains("cube"), "{} is still a cube cell", tool.id);
            assert!(!label.contains("Cube"), "{} still says cube", tool.id);
        }
        let box_tool = model
            .live_groups()
            .flat_map(GroupSpec::live_tools)
            .find(|tool| tool.run == Some(ToolKind::Body(DrawTool::Rectangle)))
            .expect("the box cell");
        assert_eq!(box_tool.id, "model.primitives.cuboid");
        assert_eq!(box_tool.label.text(text::Lang::En), "Box");
        assert_eq!(box_tool.icon, Some(icons::primitives::CUBOID));
        // The box and the cylinder are drawn, not placed: each cell lists the sketch's methods of
        // its own figure, so a body learns the whole grammar the sketch already had (ADR 0138).
        let methods: Vec<_> = box_tool.methods().map(|method| method.run).collect();
        assert_eq!(
            methods,
            vec![
                Some(ToolKind::Body(DrawTool::Rectangle)),
                Some(ToolKind::Body(DrawTool::RectangleFromCenter)),
                Some(ToolKind::Body(DrawTool::RectangleFromCenterDirected)),
                Some(ToolKind::Body(DrawTool::RectangleTilted)),
            ],
            "every rectangle method drives the box"
        );
        let cylinder = model
            .live_groups()
            .flat_map(GroupSpec::live_tools)
            .find(|tool| tool.id == "model.primitives.cylinder")
            .expect("the cylinder cell");
        let methods: Vec<_> = cylinder.methods().map(|method| method.run).collect();
        assert_eq!(
            methods,
            vec![
                Some(ToolKind::Body(DrawTool::Circle)),
                Some(ToolKind::Body(DrawTool::CircleDiameter)),
                Some(ToolKind::Body(DrawTool::CircleThreePoints)),
                Some(ToolKind::Body(DrawTool::CircleTangent)),
            ],
            "every circle method draws the cylinder's rim"
        );
    }

    #[test]
    fn the_sphere_cell_draws_its_three_rounds() {
        // The sphere is drawn, not placed by one click: its cell carries the circle methods, and each
        // method is one stage table entry with the plates of the circle it is built around (ADR 0139).
        let sphere = tab("model")
            .expect("the model tab")
            .live_groups()
            .flat_map(GroupSpec::live_tools)
            .find(|tool| tool.id == "model.primitives.sphere")
            .expect("the sphere cell");
        assert_eq!(sphere.label.text(text::Lang::En), "Sphere");
        assert_eq!(sphere.icon, Some(icons::primitives::SPHERE), "the cell keeps the sphere's own glyph");
        let methods = sphere.methods().map(|method| (method.id, method.label, method.run)).collect::<Vec<_>>();
        assert_eq!(
            methods,
            vec![
                (
                    "model.primitives.sphere",
                    Message::ToolCentreRadius,
                    Some(ToolKind::Primitive { kind: PrimitiveKind::Sphere, tool: DrawTool::Sphere })
                ),
                (
                    "model.primitives.sphere.diameter",
                    Message::ToolCentreDiameter,
                    Some(ToolKind::Primitive { kind: PrimitiveKind::Sphere, tool: DrawTool::SphereDiameter })
                ),
                (
                    "model.primitives.sphere.three-points",
                    Message::ToolThreePoints,
                    Some(ToolKind::Primitive { kind: PrimitiveKind::Sphere, tool: DrawTool::SphereThreePoints })
                ),
            ],
            "the centre, the diameter and three points draw the same ball"
        );
        assert_eq!(
            (methods[0].1.text(text::Lang::En), methods[1].1.text(text::Lang::En)),
            ("Centre and radius", "Centre and diameter")
        );
        assert_eq!(methods[2].1.text(text::Lang::En), "Through 3 points");
        for (id, _, run) in methods {
            let Some(ToolKind::Primitive { tool, .. }) = run else {
                panic!("`{id}` places a primitive");
            };
            assert_eq!(method_id(tool), id, "`{id}` answers another id");
        }
        for method in sphere.methods() {
            assert!(method.icon.is_some(), "`{}` draws its own glyph", method.id);
        }
    }

    #[test]
    fn the_frame_cell_offers_the_card_and_the_two_drawn_methods() {
        let frame = tab("furniture")
            .expect("furniture")
            .live_groups()
            .flat_map(GroupSpec::live_tools)
            .find(|tool| tool.id == "furniture.product.frame")
            .expect("the frame cell");
        let methods: Vec<_> = frame.methods().map(|method| (method.id, method.run)).collect();
        assert_eq!(
            methods,
            vec![
                ("furniture.product.frame", Some(ToolKind::Product)),
                ("furniture.product.frame.draw", Some(ToolKind::ProductDraw)),
                ("furniture.product.frame.contour", Some(ToolKind::ProductContour)),
            ],
            "the card is the tool's own method, the drawn box its second and the drawn outline its third (ADR 0101, ADR 0159)"
        );
        let drawn = frame.variants.first().expect("the drawn box");
        assert_eq!(drawn.label.text(text::Lang::En), "Frame by drawing");
        let contour = frame.variants.get(1).expect("the drawn outline");
        assert_eq!(contour.label.text(text::Lang::En), "Frame from contour");
        assert!(
            frame.variants.iter().all(|variant| variant.run.is_some()),
            "the split cell only offers a method that runs"
        );
    }

    #[test]
    fn every_tool_kind_appears_exactly_once() {
        let kinds = [
            ToolKind::CloseDraft,
            ToolKind::CancelDraft,
            ToolKind::Extrude,
            ToolKind::Product,
            ToolKind::ProductDraw,
            ToolKind::ProductContour,
            ToolKind::PanelContour,
            ToolKind::Fill { class: ClassCode::VERTICAL_PANEL, mode: FillMode::Fit },
            ToolKind::Fill { class: ClassCode::VERTICAL_PANEL, mode: FillMode::Shape },
            ToolKind::Fill { class: ClassCode::HORIZONTAL_PANEL, mode: FillMode::Fit },
            ToolKind::Fill { class: ClassCode::HORIZONTAL_PANEL, mode: FillMode::Shape },
            ToolKind::Void,
            ToolKind::Assembly,
            ToolKind::Camera,
            ToolKind::Light,
            ToolKind::FrameAll,
            ToolKind::Projection,
            ToolKind::GridSnap,
            ToolKind::Shadows,
            ToolKind::Measure,
            ToolKind::Primitive { kind: PrimitiveKind::Sphere, tool: DrawTool::Sphere },
            ToolKind::Primitive { kind: PrimitiveKind::Sphere, tool: DrawTool::SphereDiameter },
            ToolKind::Primitive { kind: PrimitiveKind::Sphere, tool: DrawTool::SphereThreePoints },
            ToolKind::Primitive { kind: PrimitiveKind::Cone, tool: DrawTool::Cone },
            ToolKind::Primitive { kind: PrimitiveKind::Cone, tool: DrawTool::ConeDiameter },
            ToolKind::Primitive { kind: PrimitiveKind::Cone, tool: DrawTool::ConeThreePoints },
            ToolKind::Body(DrawTool::Rectangle),
            ToolKind::Body(DrawTool::RectangleFromCenter),
            ToolKind::Body(DrawTool::RectangleFromCenterDirected),
            ToolKind::Body(DrawTool::RectangleTilted),
            ToolKind::Body(DrawTool::Circle),
            ToolKind::Body(DrawTool::CircleDiameter),
            ToolKind::Body(DrawTool::CircleThreePoints),
            ToolKind::Body(DrawTool::CircleTangent),
            ToolKind::Draw(DrawTool::Line),
            ToolKind::Draw(DrawTool::LineByAngle),
            ToolKind::Draw(DrawTool::Polyline),
            ToolKind::Draw(DrawTool::Arc),
            ToolKind::Draw(DrawTool::ArcFromCenter),
            ToolKind::Draw(DrawTool::ArcByRadius),
            ToolKind::Draw(DrawTool::Rectangle),
            ToolKind::Draw(DrawTool::RectangleFromCenter),
            ToolKind::Draw(DrawTool::RectangleFromCenterDirected),
            ToolKind::Draw(DrawTool::Circle),
            ToolKind::Draw(DrawTool::CircleDiameter),
            ToolKind::Draw(DrawTool::CircleThreePoints),
            ToolKind::Draw(DrawTool::CircleTangent),
            ToolKind::Draw(DrawTool::RegularPolygon),
            ToolKind::Draw(DrawTool::PolygonApothem),
            ToolKind::Draw(DrawTool::PolygonByEdge),
            ToolKind::Draw(DrawTool::RectangleTilted),
        ];
        let mut seen = Vec::new();
        for (_, tool) in every_tool() {
            if let Some(kind) = tool.run {
                seen.push((tool.id, kind));
            }
            for variant in tool.variants {
                if let Some(kind) = variant.run {
                    seen.push((variant.id, kind));
                }
            }
        }
        for kind in kinds {
            let holders = seen.iter().filter(|(_, held)| *held == kind).map(|(id, _)| *id).collect::<Vec<_>>();
            assert_eq!(holders.len(), 1, "{kind:?} is reachable from {holders:?}");
        }
        assert_eq!(seen.len(), kinds.len(), "a tool runs a kind the list does not name: {seen:?}");
    }

    #[test]
    fn the_row_is_counted_from_the_spec() {
        let sketch = tab("sketch").expect("sketch");
        // Six creation tools, the ruler and the draft pair.
        assert_eq!(sketch.columns(), 9);
        // The planned groups draw nothing, so the Model tab is the four primitives and the extrusion.
        assert_eq!(tab("model").expect("model").columns(), 5);
        assert_eq!(tab("furniture").expect("furniture").columns(), 5);
        assert_eq!(tab("scene").expect("scene").columns(), 3);
        assert_eq!(tab("view").expect("view").columns(), 4);
        let widest = tabs().iter().map(TabSpec::columns).max().unwrap_or(0);
        assert_eq!(widest, 9, "the sketch tab is the widest row of the map");
        for tab in tabs() {
            assert!(tab.columns() <= widest);
        }
        // The View tab's four cells are the projection pair, the shadow switch and the snap.
        assert_eq!(tab("view").expect("view").columns(), 2 + 1 + 1);
    }

    #[test]
    fn a_row_counts_its_live_tools_and_nothing_else() {
        for group in tabs().iter().flat_map(|tab| tab.groups) {
            assert_eq!(group.columns(), group.live_tools().count(), "{} counts more than its tools", group.id);
            assert!(group.columns() <= group.tools.len(), "{} counts a cell it does not hold", group.id);
        }
    }
}
