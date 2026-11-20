//! Deriving every table from one walk of the document.
//!
//! The exporter is a consumer of the document, not a second model: it reads the tree, the class
//! traits, the envelope, the panel outline and banding, and the catalogue. Every number a report
//! needs is resolved here — measured size, area, banding length, the owning product, the nesting
//! flag, the purchase channel — so a reader never has to re-derive it and never disagrees with us.

use std::collections::{BTreeMap, BTreeSet};

use catalog::{Directory, StockView};
use geom::DVec3;
use model::{
    ClassCode, Contour, ContourSegment, CutoutDepth, CutoutShape, Extent, FaceSide, Fill, Id, ItemId, Level,
    MachiningParams, Measure, Mount, Nesting, Node, Opening, Panel, PanelGeometry, PanelNominalSize, Placement,
    Project, PropertyValue, SYSTEM_CLASS, SYSTEM_COMPONENT, SYSTEM_PROTOTYPE_ID, SYSTEM_PROTOTYPE_PARAMETERS,
    SYSTEM_PROTOTYPE_VERSION, Supply,
};
use serde_json::{Value, json};
use text::Lang;

use crate::write::PriceAging;
use units::Length;

use crate::error::ExportError;
use crate::manifest::{ROTATION_SCALE, RULES};
use crate::table::{Cell, Tables};

/// Builds every table of one package, ordered by its key.
pub fn build(project: &Project, catalog: &Directory, aging: PriceAging) -> Result<Tables, ExportError> {
    let mut context = Context { aging, ..Context::default() };
    for product in project.products() {
        context.products.insert(product.get(), ProductAgg::default());
    }
    let root = project.root();
    let root_id = root.id();
    let root_class = project.class(root_id).unwrap_or(ClassCode::UNCLASSIFIED);
    context.nodes.insert(root_id.get(), node_row(root_id, 0, root.name(), root.placement(), root_class, root.name()));
    note_class(&mut context, root_class);
    note_properties(&mut context, project.properties(root_id), root_id);
    walk(project, catalog, &mut context, root.children(), root_id, "", 1)?;

    let mut tables = Tables::new();
    push(&mut tables, "nodes", &context.nodes)?;
    push(&mut tables, "parts", &context.parts)?;
    push(&mut tables, "banding", &context.banding)?;
    push(&mut tables, "contours", &context.contours)?;
    push(&mut tables, "facades", &context.facades)?;
    push(&mut tables, "fittings", &context.fittings)?;
    push(&mut tables, "machining", &context.machining)?;
    push(&mut tables, "properties", &context.properties)?;
    push_sheets(&mut tables, &context.sheets)?;
    push_products(&mut tables, project, &context.products)?;
    push_materials(&mut tables, &context.materials)?;
    push_classes(&mut tables, &context.classes)?;
    Ok(tables)
}

/// Everything one walk collects, keyed by the column the table is ordered by.
#[derive(Default)]
struct Context {
    nodes: BTreeMap<u64, Vec<Cell>>,
    parts: BTreeMap<u64, Vec<Cell>>,
    banding: BTreeMap<(u64, String), Vec<Cell>>,
    contours: BTreeMap<(u64, String, usize), Vec<Cell>>,
    facades: BTreeMap<u64, Vec<Cell>>,
    fittings: BTreeMap<u64, Vec<Cell>>,
    machining: BTreeMap<u64, Vec<Cell>>,
    properties: BTreeMap<(u64, String), Vec<Cell>>,
    products: BTreeMap<u64, ProductAgg>,
    materials: BTreeMap<ItemId, MaterialFacts>,
    sheets: BTreeMap<ItemId, SheetAgg>,
    classes: BTreeSet<u32>,
    /// How old a price may be before this package calls it stale.
    aging: PriceAging,
}

/// What a product's parts add up to.
#[derive(Default)]
struct ProductAgg {
    part_count: u64,
    area_mm2: i128,
    banding_um: i64,
    fitting_count: u64,
    price_minor: i64,
    min: Option<[i64; 3]>,
    max: Option<[i64; 3]>,
}

/// What one stocked material's parts ask of it.
#[derive(Default)]
struct SheetAgg {
    stock: Option<StockFacts>,
    part_count: u64,
    part_area_mm2: i128,
}

/// The stock block of one catalogue material, as the package repeats it.
#[derive(Clone)]
struct StockFacts {
    stock_group: String,
    sheet_length_um: i64,
    sheet_width_um: i64,
    price_minor: i64,
    /// The day the price was measured, `YYYY-MM-DD`, empty when the record names none.
    price_date: String,
    /// One when the price is older than the workshop's limit, zero otherwise.
    price_stale: i64,
    sheet_area_mm2: i64,
}

/// One catalogue row as the package embeds it.
struct MaterialFacts {
    name: String,
    kind: String,
    thickness: Length,
    width: Length,
    color: String,
    stock: Option<StockFacts>,
}

/// One banding strip with its resolved length and profile.
struct BandFacts {
    unit: String,
    material: ItemId,
    thickness: Length,
    width: Length,
    included: bool,
    length_um: i64,
}

/// One panel resolved once, so its four tables read the same numbers.
struct PanelFacts<'a> {
    panel: &'a Panel,
    class: ClassCode,
    thickness: Length,
    geometry: PanelGeometry,
    segments: Vec<(geom::Segment, model::SegmentSource)>,
    bands: Vec<BandFacts>,
    banding_total_um: i64,
}

/// Walks the children of one container, in tree order.
fn walk(
    project: &Project,
    catalog: &Directory,
    context: &mut Context,
    children: &[Node],
    parent: Id,
    prefix: &str,
    depth: u32,
) -> Result<(), ExportError> {
    if depth > Project::MAX_ASSEMBLY_DEPTH {
        return Ok(());
    }
    for child in children {
        let id = child.id();
        let path = if prefix.is_empty() { child.name().to_owned() } else { format!("{prefix}/{}", child.name()) };
        let class = project.class(id).unwrap_or(ClassCode::UNCLASSIFIED);
        context.nodes.insert(id.get(), node_row(id, parent.get(), child.name(), child.placement(), class, &path));
        note_class(context, class);
        note_properties(context, project.properties(id), id);
        if level_of(class) == Some(Level::Fitting) {
            context.fittings.insert(id.get(), fitting_row(project, id, class));
        }
        match child {
            Node::Container(container) => {
                walk(project, catalog, context, container.children(), id, &path, depth.saturating_add(1))?;
            }
            Node::Panel(panel) => panel_rows(project, catalog, context, panel)?,
            Node::Object(_) | Node::Camera(_) | Node::Light(_) => {}
        }
    }
    Ok(())
}

/// Builds the part, banding, contour, machining and facade rows of one panel.
fn panel_rows(project: &Project, catalog: &Directory, context: &mut Context, panel: &Panel) -> Result<(), ExportError> {
    let facts = panel_facts(panel, catalog)?;
    let product = project.product_of(panel.id());
    let product_id = product.map_or(0, Id::get);
    let material_id = panel.material().map(|id| id.to_string()).unwrap_or_default();
    let class = facts.class;
    let size = panel.nominal_size();
    let area_mm2 = rounded_i64(facts.geometry.core_region().area()).unwrap_or(0);
    let stock = panel.material().and_then(|material| catalog.item(material)).and_then(|item| catalog.stock(item));
    let stock = stock.as_ref();
    let price_minor = part_cost(stock, area_mm2);

    context.parts.insert(
        panel.id().get(),
        vec![
            Cell::Integer(panel.id().get() as i64),
            Cell::Integer(product_id as i64),
            Cell::Text(class_text(class)),
            Cell::Text(material_id.clone()),
            Cell::Integer(facts.thickness.micrometres()),
            Cell::Integer(size.length().micrometres()),
            Cell::Integer(size.width().micrometres()),
            Cell::Integer(facts.geometry.overall_length().micrometres()),
            Cell::Integer(facts.geometry.overall_width().micrometres()),
            Cell::Integer(area_mm2),
            Cell::Text("none".to_owned()),
            Cell::Text(nesting_text(class).to_owned()),
            Cell::Integer(facts.banding_total_um),
            Cell::Integer(price_minor),
        ],
    );
    note_stock(context, panel.material(), stock, area_mm2);

    for band in &facts.bands {
        context.banding.insert(
            (panel.id().get(), band.unit.clone()),
            vec![
                Cell::Integer(panel.id().get() as i64),
                Cell::Text(band.unit.clone()),
                Cell::Text(band.material.to_string()),
                Cell::Integer(band.thickness.micrometres()),
                Cell::Integer(band.width.micrometres()),
                Cell::Boolean(band.included),
                Cell::Integer(band.length_um),
            ],
        );
        note_material(context, catalog, band.material, band.thickness, band.width);
    }
    if let Some(material) = panel.material() {
        note_material(context, catalog, material, panel.last_known_thickness(), Length::ZERO);
    }

    for (seq, (segment, _)) in facts.segments.iter().enumerate() {
        context.contours.insert((panel.id().get(), "outline".to_owned(), seq), contour_row(panel.id(), seq, *segment));
    }

    for machining in panel.machining() {
        let (op, x_um, y_um, depth_um, params) = machining_facts(machining.params())?;
        context.machining.insert(
            machining.id().get(),
            vec![
                Cell::Integer(machining.id().get() as i64),
                Cell::Integer(panel.id().get() as i64),
                Cell::Text(op.to_owned()),
                Cell::Text(machining.anchor().as_face().map(FaceSide::label).unwrap_or("").to_owned()),
                Cell::Integer(x_um),
                Cell::Integer(y_um),
                Cell::Integer(0),
                Cell::Integer(depth_um),
                Cell::Json(params),
            ],
        );
    }

    if class.traits().and_then(|traits| traits.opening()) == Some(Opening::Closes) {
        let glass = panel
            .material()
            .and_then(|material| catalog.item(material))
            .and_then(|item| catalog.text(item, catalog::MATERIAL_SLUG))
            .is_some_and(|made_of| made_of.to_ascii_lowercase().contains("glass"));
        context.facades.insert(panel.id().get(), facade_row(&facts, product_id, glass));
    }

    add_to_product(project, context, panel, facts.thickness, facts.banding_total_um, &facts.geometry, price_minor)
}

/// Resolves a panel once: thickness, geometry, nominal segments and every banding strip.
fn panel_facts<'a>(panel: &'a Panel, catalog: &Directory) -> Result<PanelFacts<'a>, ExportError> {
    let thickness = panel
        .material()
        .and_then(|material| catalog.board(material))
        .map_or_else(|| panel.last_known_thickness(), |material| material.thickness);
    let geometry = panel.geometry_with_banding(thickness, |spec| {
        catalog.banding(spec.material()).map_or_else(|| spec.last_known_thickness(), |material| material.thickness)
    })?;
    let (ring, sources) = panel.outline().ring_and_sources_for(panel.nominal_size())?;
    let segments: Vec<(geom::Segment, model::SegmentSource)> = ring.segments().iter().copied().zip(sources).collect();
    let mut bands = Vec::new();
    for band in panel.edge_banding() {
        let (thickness, width) =
            edge_profile(catalog, band.material(), band.last_known_thickness(), band.last_known_width());
        bands.push(BandFacts {
            unit: format!("edge:{}", band.edge().get()),
            material: band.material(),
            thickness,
            width,
            included: band.included_in_size(),
            length_um: segment_length(&segments, model::SegmentSource::Edge(band.edge())),
        });
    }
    for band in panel.corner_banding() {
        let (thickness, width) =
            edge_profile(catalog, band.material(), band.last_known_thickness(), band.last_known_width());
        bands.push(BandFacts {
            unit: format!("corner:{}", band.corner().get()),
            material: band.material(),
            thickness,
            width,
            included: band.included_in_size(),
            length_um: segment_length(&segments, model::SegmentSource::Corner(band.corner())),
        });
    }
    for band in panel.ring_banding() {
        let (thickness, width) =
            edge_profile(catalog, band.material(), band.last_known_thickness(), band.last_known_width());
        bands.push(BandFacts {
            unit: format!("segment:{}", band.segment().get()),
            material: band.material(),
            thickness,
            width,
            included: band.included_in_size(),
            length_um: segment_length(&segments, model::SegmentSource::Ring(band.segment())),
        });
    }
    let banding_total_um = bands.iter().fold(0i64, |total, band| total.saturating_add(band.length_um));
    Ok(PanelFacts { panel, class: panel_class(panel), thickness, geometry, segments, bands, banding_total_um })
}

/// The class stored on a panel, read from its own property bag.
fn panel_class(panel: &Panel) -> ClassCode {
    match panel.properties().get(SYSTEM_CLASS) {
        Some(PropertyValue::Class { value }) => *value,
        _ => ClassCode::UNCLASSIFIED,
    }
}

/// The catalogue profile of a banding material, or the record's fallback.
fn edge_profile(catalog: &Directory, material: ItemId, fallback: Length, fallback_width: Length) -> (Length, Length) {
    catalog.banding(material).map_or((fallback, fallback_width), |material| (material.thickness, material.width))
}

/// The nominal length of the segment a band covers, in micrometres.
fn segment_length(segments: &[(geom::Segment, model::SegmentSource)], source: model::SegmentSource) -> i64 {
    segments
        .iter()
        .find(|(_, candidate)| *candidate == source)
        .and_then(|(segment, _)| rounded_i64(segment.length() * 1000.0))
        .unwrap_or(0)
}

/// Adds one panel's numbers to the product that owns it, or does nothing outside a product.
fn add_to_product(
    project: &Project,
    context: &mut Context,
    panel: &Panel,
    thickness: Length,
    banding_um: i64,
    geometry: &PanelGeometry,
    price_minor: i64,
) -> Result<(), ExportError> {
    let Some(product) = project.product_of(panel.id()) else {
        return Ok(());
    };
    let Some(aggregate) = context.products.get_mut(&product.get()) else {
        return Ok(());
    };
    aggregate.part_count = aggregate.part_count.saturating_add(1);
    aggregate.area_mm2 =
        aggregate.area_mm2.saturating_add(i128::from(rounded_i64(geometry.core_region().area()).unwrap_or(0)));
    aggregate.banding_um = aggregate.banding_um.saturating_add(banding_um);
    aggregate.price_minor = aggregate.price_minor.saturating_add(price_minor);
    let Some(product_world) = project.world_placement(product) else {
        return Ok(());
    };
    let Some(panel_world) = project.world_placement(panel.id()) else {
        return Ok(());
    };
    let inverse = product_world.inverse()?;
    let local = Placement::compose(inverse, panel_world)?;
    if let Some((min, max)) = panel_box(panel.nominal_size(), thickness, local) {
        aggregate.min = Some(match aggregate.min {
            Some(current) => [current[0].min(min[0]), current[1].min(min[1]), current[2].min(min[2])],
            None => min,
        });
        aggregate.max = Some(match aggregate.max {
            Some(current) => [current[0].max(max[0]), current[1].max(max[1]), current[2].max(max[2])],
            None => max,
        });
    }
    Ok(())
}

/// The nominal box of a panel through a placement, in whole micrometres.
fn panel_box(size: PanelNominalSize, thickness: Length, placement: Placement) -> Option<([i64; 3], [i64; 3])> {
    let length = size.length().millimetres_f64();
    let width = size.width().millimetres_f64();
    let depth = thickness.millimetres_f64();
    let mut min = [i64::MAX; 3];
    let mut max = [i64::MIN; 3];
    for x in [0.0, length] {
        for y in [0.0, width] {
            for z in [0.0, depth] {
                let point = placement.transform_point(DVec3::new(x, y, z));
                let point =
                    [rounded_i64(point.x * 1000.0)?, rounded_i64(point.y * 1000.0)?, rounded_i64(point.z * 1000.0)?];
                for axis in 0..3 {
                    min[axis] = min[axis].min(point[axis]);
                    max[axis] = max[axis].max(point[axis]);
                }
            }
        }
    }
    Some((min, max))
}

/// The product row of one product.
fn push_products(
    tables: &mut Tables,
    project: &Project,
    aggregates: &BTreeMap<u64, ProductAgg>,
) -> Result<(), ExportError> {
    let table = tables.table_mut("products").ok_or(ExportError::MissingTable("products"))?;
    for (id, aggregate) in aggregates {
        let product = Id::new(*id);
        let Some(product) = product else {
            continue;
        };
        let Some(container) = project.container(product) else {
            continue;
        };
        let class = project.class(product).unwrap_or(ClassCode::UNCLASSIFIED);
        let envelope = container.envelope();
        let width = envelope.map_or(0, |box_| box_.x().micrometres());
        let depth = envelope.map_or(0, |box_| box_.y().micrometres());
        let height = envelope.map_or(0, |box_| box_.z().micrometres());
        let content = aggregate.min.zip(aggregate.max);
        let content_width = content.map_or(0, |(min, max)| max[0].saturating_sub(min[0]));
        let content_height = content.map_or(0, |(min, max)| max[2].saturating_sub(min[2]));
        let content_depth = content.map_or(0, |(min, max)| max[1].saturating_sub(min[1]));
        table.push(vec![
            Cell::Integer(*id as i64),
            Cell::Integer(project.parent_of(product).map_or(0, |parent| parent.get() as i64)),
            Cell::Text(container.name().to_owned()),
            Cell::Text(class_text(class)),
            Cell::Text(level_text(level_of(class)).to_owned()),
            Cell::Integer(width),
            Cell::Integer(height),
            Cell::Integer(depth),
            Cell::Integer(content_width),
            Cell::Integer(content_height),
            Cell::Integer(content_depth),
            Cell::Integer(i64::try_from(aggregate.part_count).unwrap_or(i64::MAX)),
            Cell::Integer(i64::try_from(aggregate.area_mm2).unwrap_or(i64::MAX)),
            Cell::Integer(aggregate.banding_um),
            Cell::Integer(i64::try_from(aggregate.fitting_count).unwrap_or(i64::MAX)),
            Cell::Integer(aggregate.price_minor),
        ])?;
    }
    Ok(())
}

/// The facade row of a panel whose class closes an opening.
fn facade_row(facts: &PanelFacts<'_>, product_id: u64, glass: bool) -> Vec<Cell> {
    let panel = facts.panel;
    let size = panel.nominal_size();
    let material_id = panel.material().map(|id| id.to_string()).unwrap_or_default();
    vec![
        Cell::Integer(panel.id().get() as i64),
        Cell::Integer(product_id as i64),
        Cell::Text(class_text(facts.class)),
        Cell::Text(material_id.clone()),
        Cell::Integer(size.length().micrometres()),
        Cell::Integer(size.width().micrometres()),
        Cell::Integer(facts.thickness.micrometres()),
        Cell::Text(supply_of(facts.class).map_or("", supply_text).to_owned()),
        Cell::Boolean(glass),
    ]
}

/// The fitting row of one node whose class names a fitting.
fn fitting_row(project: &Project, id: Id, class: ClassCode) -> Vec<Cell> {
    vec![
        Cell::Integer(id.get() as i64),
        Cell::Integer(project.parent_of(id).map_or(0, |parent| parent.get() as i64)),
        Cell::Integer(project.product_of(id).map_or(0, Id::get) as i64),
        Cell::Text(class_text(class)),
        Cell::Text(String::new()),
        Cell::Integer(1),
    ]
}

/// One row of the tree table.
fn node_row(id: Id, parent: u64, name: &str, placement: Placement, class: ClassCode, path: &str) -> Vec<Cell> {
    let translation = placement.translation();
    let rotation = placement.rotation();
    vec![
        Cell::Integer(id.get() as i64),
        Cell::Integer(parent as i64),
        Cell::Text(name.to_owned()),
        Cell::Text(class_text(class)),
        Cell::Text(level_text(level_of(class)).to_owned()),
        Cell::Text(path.to_owned()),
        Cell::Integer(rounded_i64(translation.x * 1000.0).unwrap_or(0)),
        Cell::Integer(rounded_i64(translation.y * 1000.0).unwrap_or(0)),
        Cell::Integer(rounded_i64(translation.z * 1000.0).unwrap_or(0)),
        Cell::Integer(scaled_quaternion(rotation.w)),
        Cell::Integer(scaled_quaternion(rotation.x)),
        Cell::Integer(scaled_quaternion(rotation.y)),
        Cell::Integer(scaled_quaternion(rotation.z)),
    ]
}

/// One row of the contour table.
fn contour_row(id: Id, seq: usize, segment: geom::Segment) -> Vec<Cell> {
    let start = segment.start();
    let x = micrometres(start.x);
    let y = micrometres(start.y);
    let (kind, x2, y2, cx, cy, clockwise) = match segment {
        geom::Segment::Line(_) => {
            let end = segment.end();
            ("line", micrometres(end.x), micrometres(end.y), 0, 0, false)
        }
        geom::Segment::Arc(arc) => {
            let center = arc.center();
            ("arc", 0, 0, micrometres(center.x), micrometres(center.y), arc.sweep_angle() < 0.0)
        }
    };
    vec![
        Cell::Integer(id.get() as i64),
        Cell::Text("outline".to_owned()),
        Cell::Integer(i64::try_from(seq).unwrap_or(i64::MAX)),
        Cell::Text(kind.to_owned()),
        Cell::Integer(x),
        Cell::Integer(y),
        Cell::Integer(x2),
        Cell::Integer(y2),
        Cell::Integer(cx),
        Cell::Integer(cy),
        Cell::Boolean(clockwise),
    ]
}

/// The operation, position, depth and JSON parameters of one machining record.
fn machining_facts(params: &MachiningParams) -> Result<(&'static str, i64, i64, i64, Value), ExportError> {
    let MachiningParams::Cutout(cutout) = params;
    let (x_um, y_um, shape) = match cutout.shape() {
        CutoutShape::Rectangle(rect) => (
            rect.x().micrometres(),
            rect.y().micrometres(),
            json!({
                "shape": "rectangle",
                "width_um": rect.width().micrometres(),
                "height_um": rect.height().micrometres(),
            }),
        ),
        CutoutShape::Contour(contour) => (0, 0, json!({ "shape": "contour", "segments": contour_json(contour)? })),
    };
    let (depth_um, depth) = match cutout.depth() {
        CutoutDepth::Through => (0, json!("through")),
        CutoutDepth::Blind { depth } => (depth.micrometres(), json!("blind")),
    };
    let mut shape = shape;
    if let Value::Object(ref mut object) = shape {
        object.insert("depth".to_owned(), depth);
    }
    Ok((params.kind().id(), x_um, y_um, depth_um, shape))
}

/// The segments of a contour cutout, as the operation-specific JSON parameters.
fn contour_json(contour: &Contour) -> Result<Value, ExportError> {
    let mut segments = Vec::with_capacity(contour.entries().len());
    for entry in contour.entries() {
        let segment = entry.segment();
        segments.push(match segment {
            ContourSegment::Line { start, end } => json!({
                "kind": "line",
                "x_um": start[0].micrometres(),
                "y_um": start[1].micrometres(),
                "x2_um": end[0].micrometres(),
                "y2_um": end[1].micrometres(),
            }),
            ContourSegment::Arc { center, radius, start_angle, sweep } => json!({
                "kind": "arc",
                "x_um": segment.start()?[0].micrometres(),
                "y_um": segment.start()?[1].micrometres(),
                "cx_um": center[0].micrometres(),
                "cy_um": center[1].micrometres(),
                "radius_um": radius.micrometres(),
                "start_mdeg": start_angle.millidegrees(),
                "sweep_mdeg": sweep.millidegrees(),
                "clockwise": sweep.millidegrees() < 0,
            }),
        });
    }
    Ok(Value::Array(segments))
}

/// Records one used class and its registered ancestors.
fn note_class(context: &mut Context, class: ClassCode) {
    let mut current = class;
    if current.is_unclassified() {
        return;
    }
    loop {
        if current.is_unclassified() {
            break;
        }
        context.classes.insert(current.value());
        let parent = current.parent();
        if parent == current {
            break;
        }
        current = parent;
    }
}

/// Records the user's properties of one node, skipping the reserved system keys.
fn note_properties(context: &mut Context, properties: Option<&model::Properties>, id: Id) {
    let Some(properties) = properties else {
        return;
    };
    for (key, value) in properties.iter() {
        if is_system_key(key) {
            continue;
        }
        context.properties.insert((id.get(), key.to_owned()), property_row(id, key, value));
    }
}

/// Whether a property key belongs to the document rather than to the user.
fn is_system_key(key: &str) -> bool {
    [SYSTEM_CLASS, SYSTEM_COMPONENT, SYSTEM_PROTOTYPE_ID, SYSTEM_PROTOTYPE_VERSION, SYSTEM_PROTOTYPE_PARAMETERS]
        .contains(&key)
}

/// One row of the property table.
fn property_row(id: Id, key: &str, value: &PropertyValue) -> Vec<Cell> {
    vec![
        Cell::Integer(id.get() as i64),
        Cell::Text(key.to_owned()),
        Cell::Text(value.kind().to_owned()),
        Cell::Json(property_json(value)),
    ]
}

/// The JSON machine value of a typed property.
fn property_json(value: &PropertyValue) -> Value {
    match value {
        PropertyValue::Bool { value } => Value::Bool(*value),
        PropertyValue::Integer { value } => Value::from(*value),
        PropertyValue::Length { value } => Value::from(value.micrometres()),
        PropertyValue::Angle { value } => Value::from(value.millidegrees()),
        PropertyValue::Text { value } => Value::String(value.clone()),
        PropertyValue::Choice { key, label } => json!({ "key": key, "label": label }),
        PropertyValue::Reference { id } => Value::from(id.get()),
        PropertyValue::Class { value } => Value::String(value.to_string()),
        PropertyValue::Structured { value } => value.clone(),
    }
}

/// Records a material row the order uses, once.
fn note_material(
    context: &mut Context,
    catalog: &Directory,
    material: ItemId,
    fallback_thickness: Length,
    fallback_width: Length,
) {
    let key = material;
    if context.materials.contains_key(&key) {
        return;
    }
    let made_of = catalog
        .item(material)
        .and_then(|item| catalog.text(item, catalog::MATERIAL_SLUG))
        .unwrap_or_default()
        .to_owned();
    let stocked = catalog.item(material).and_then(|item| catalog.stock(item));
    let facts = if let Some(board) = catalog.board(material) {
        MaterialFacts {
            name: board.name.clone(),
            kind: made_of,
            thickness: board.thickness,
            width: Length::ZERO,
            color: board.color.clone().unwrap_or_default(),
            stock: stocked.as_ref().map(|stock| stock_facts(stock, context.aging)),
        }
    } else if let Some(band) = catalog.banding(material) {
        MaterialFacts {
            name: band.name.clone(),
            kind: made_of,
            thickness: band.thickness,
            width: band.width,
            color: band.color.clone().unwrap_or_default(),
            stock: None,
        }
    } else {
        MaterialFacts {
            name: String::new(),
            kind: String::new(),
            thickness: fallback_thickness,
            width: fallback_width,
            color: String::new(),
            stock: None,
        }
    };
    context.materials.insert(key, facts);
}

/// The stock block of one material as the package repeats it, with the sheet area resolved once.
fn stock_facts(stock: &StockView, aging: PriceAging) -> StockFacts {
    StockFacts {
        stock_group: stock.group.clone().unwrap_or_default(),
        sheet_length_um: stock.length.micrometres(),
        sheet_width_um: stock.width.micrometres(),
        price_minor: stock.price.as_ref().map_or(0, |price| i64::try_from(price.value().minor()).unwrap_or(i64::MAX)),
        // The date travels even when the package makes no judgement: a reader that knows its own
        // shelf life can still see how old the number is.
        price_date: stock
            .price
            .as_ref()
            .and_then(|price| price.price.date())
            .map(catalog::Date::iso)
            .unwrap_or_default(),
        price_stale: stock
            .price
            .as_ref()
            .and_then(|price| aging.freshness(&price.price))
            .is_some_and(catalog::Freshness::is_stale)
            .into(),
        sheet_area_mm2: sheet_area_mm2(stock.length, stock.width),
    }
}

/// Records that one part is cut from a stocked material's sheet, so the sheet table can group it.
fn note_stock(context: &mut Context, material: Option<ItemId>, stock: Option<&StockView>, area_mm2: i64) {
    let (Some(material), Some(stock)) = (material, stock) else {
        return;
    };
    let aggregate = context.sheets.entry(material).or_default();
    aggregate.part_count = aggregate.part_count.saturating_add(1);
    aggregate.part_area_mm2 = aggregate.part_area_mm2.saturating_add(i128::from(area_mm2));
    if aggregate.stock.is_none() {
        aggregate.stock = Some(stock_facts(stock, context.aging));
    }
}

/// The share of one sheet's price a part consumes, rounded to the nearest minor unit.
///
/// A sheet price buys a whole nominal sheet, so a part costs what its core area is of that sheet. No
/// nesting efficiency is applied: how many parts a sheet really holds is the reader's layout, and a
/// guessed efficiency would be a rule the package never declared.
fn part_cost(stock: Option<&StockView>, part_area_mm2: i64) -> i64 {
    let (Some(stock), Some(price)) = (stock, stock.and_then(|stock| stock.price.as_ref())) else {
        return 0;
    };
    let sheet_area_mm2 = sheet_area_mm2(stock.length, stock.width);
    if part_area_mm2 <= 0 || sheet_area_mm2 <= 0 {
        return 0;
    }
    let numerator = i128::from(price.value().minor()) * i128::from(part_area_mm2);
    let denominator = i128::from(sheet_area_mm2);
    i64::try_from((numerator + denominator / 2) / denominator).unwrap_or(i64::MAX)
}

/// A sheet's area in whole square millimetres.
fn sheet_area_mm2(length: Length, width: Length) -> i64 {
    let area = i128::from(length.micrometres()) * i128::from(width.micrometres()) / 1_000_000;
    i64::try_from(area).unwrap_or(i64::MAX)
}

/// Writes the material rows in ascending id order.
fn push_materials(tables: &mut Tables, materials: &BTreeMap<ItemId, MaterialFacts>) -> Result<(), ExportError> {
    let table = tables.table_mut("materials").ok_or(ExportError::MissingTable("materials"))?;
    for (id, material) in materials {
        let stock = material.stock.as_ref();
        table.push(vec![
            Cell::Text(id.to_string()),
            Cell::Text(material.name.clone()),
            Cell::Text(material.kind.clone()),
            Cell::Integer(material.thickness.micrometres()),
            Cell::Integer(material.width.micrometres()),
            Cell::Text(material.color.clone()),
            Cell::Integer(stock.map_or(0, |stock| stock.sheet_length_um)),
            Cell::Integer(stock.map_or(0, |stock| stock.sheet_width_um)),
            Cell::Text(stock.map(|stock| stock.stock_group.clone()).unwrap_or_default()),
            Cell::Integer(stock.map_or(0, |stock| stock.price_minor)),
            Cell::Text(stock.map(|stock| stock.price_date.clone()).unwrap_or_default()),
            Cell::Integer(stock.map_or(0, |stock| stock.price_stale)),
        ])?;
    }
    Ok(())
}

/// Writes one sheet row per stocked material the order cuts from, in ascending material id order.
///
/// The row is the sheet geometry, the usable rectangle after the declared trim, the price and what the
/// order asks of it. It carries no layout and no sheet count: where a part lands and how many sheets
/// that needs is the nesting of the program that reads the package.
fn push_sheets(tables: &mut Tables, sheets: &BTreeMap<ItemId, SheetAgg>) -> Result<(), ExportError> {
    let table = tables.table_mut("sheets").ok_or(ExportError::MissingTable("sheets"))?;
    for (id, aggregate) in sheets {
        let Some(stock) = &aggregate.stock else {
            continue;
        };
        let part_count = i64::try_from(aggregate.part_count).unwrap_or(i64::MAX);
        let part_area = i64::try_from(aggregate.part_area_mm2).unwrap_or(i64::MAX);
        table.push(vec![
            Cell::Text(id.to_string()),
            Cell::Text(stock.stock_group.clone()),
            Cell::Integer(stock.sheet_length_um),
            Cell::Integer(stock.sheet_width_um),
            Cell::Integer(RULES.usable_length(stock.sheet_length_um)),
            Cell::Integer(RULES.usable_width(stock.sheet_width_um)),
            Cell::Integer(stock.price_minor),
            Cell::Text(stock.price_date.clone()),
            Cell::Integer(stock.price_stale),
            Cell::Integer(part_count),
            Cell::Integer(part_area),
            Cell::Integer(stock.sheet_area_mm2),
        ])?;
    }
    Ok(())
}

/// Writes the class rows in ascending code order, every ancestor included.
fn push_classes(tables: &mut Tables, classes: &BTreeSet<u32>) -> Result<(), ExportError> {
    let table = tables.table_mut("classes").ok_or(ExportError::MissingTable("classes"))?;
    for code in classes {
        let Some(code) = ClassCode::from_code(*code) else {
            continue;
        };
        let row = code.row();
        let traits = code.traits();
        let parent = code.parent();
        let operations = traits.map_or_else(Vec::new, |traits| {
            traits.operations().iter().map(|operation| Value::String(operation_text(operation).to_owned())).collect()
        });
        table.push(vec![
            Cell::Text(code.to_string()),
            Cell::Text(if parent.is_unclassified() { String::new() } else { parent.to_string() }),
            Cell::Text(row.map_or("", |row| row.slug()).to_owned()),
            Cell::Text(row.map_or("", |row| row.label().text(Lang::En)).to_owned()),
            Cell::Text(traits.map_or("", |traits| supply_text(traits.supply())).to_owned()),
            Cell::Text(traits.map_or("", |traits| measure_text(traits.measure())).to_owned()),
            Cell::Text(traits.map_or("", |traits| nesting_trait_text(traits.nesting())).to_owned()),
            Cell::Text(traits.map_or("", |traits| extent_text(traits.extent())).to_owned()),
            Cell::Text(level_text(traits.map(|traits| traits.level())).to_owned()),
            Cell::Text(traits.and_then(|traits| traits.opening()).map_or("", opening_text).to_owned()),
            Cell::Text(traits.and_then(|traits| traits.mount()).map_or("", mount_text).to_owned()),
            Cell::Text(traits.and_then(|traits| traits.fill()).map_or("", fill_text).to_owned()),
            Cell::Json(Value::Array(operations)),
            Cell::Boolean(row.is_some_and(|row| row.is_retired())),
        ])?;
    }
    Ok(())
}

/// Appends every row of one keyed map to its table.
fn push<K: Ord>(tables: &mut Tables, name: &'static str, rows: &BTreeMap<K, Vec<Cell>>) -> Result<(), ExportError> {
    let table = tables.table_mut(name).ok_or(ExportError::MissingTable(name))?;
    for row in rows.values() {
        table.push(row.clone())?;
    }
    Ok(())
}

/// The class text of a node: the dotted code, or empty when unclassified.
fn class_text(class: ClassCode) -> String {
    if class.is_unclassified() { String::new() } else { class.to_string() }
}

/// The role trait of a class.
fn level_of(class: ClassCode) -> Option<Level> {
    class.traits().map(|traits| traits.level())
}

/// The supply trait of a class, or nothing when the class is unknown.
fn supply_of(class: ClassCode) -> Option<Supply> {
    class.traits().map(|traits| traits.supply())
}

/// The nesting flag of a class, or empty when the class says nothing.
fn nesting_text(class: ClassCode) -> &'static str {
    class.traits().map_or("", |traits| nesting_trait_text(traits.nesting()))
}

/// The level spelling in the package.
fn level_text(level: Option<Level>) -> &'static str {
    match level {
        Some(Level::Product) => "Product",
        Some(Level::Component) => "Component",
        Some(Level::Part) => "Part",
        Some(Level::Fitting) => "Fitting",
        None => "",
    }
}

/// The report role of a class.
fn supply_text(supply: Supply) -> &'static str {
    match supply {
        Supply::MadeFromSheet => "sheet",
        Supply::MadeFromLinear => "linear",
        Supply::BuyPiece => "piece",
        Supply::MadeInShop => "shop",
    }
}

/// The measure unit of a class.
fn measure_text(measure: Measure) -> &'static str {
    match measure {
        Measure::Area => "area",
        Measure::Length => "length",
        Measure::Count => "count",
    }
}

/// The nesting trait of a class.
fn nesting_trait_text(nesting: Nesting) -> &'static str {
    match nesting {
        Nesting::Member => "member",
        Nesting::Never => "never",
    }
}

/// The extent trait of a class.
fn extent_text(extent: Extent) -> &'static str {
    match extent {
        Extent::Free => "free",
        Extent::LengthOnly => "length",
        Extent::None => "none",
    }
}

/// The opening trait of a class.
fn opening_text(opening: Opening) -> &'static str {
    match opening {
        Opening::Closes => "closes",
    }
}

/// The mount trait of a class.
fn mount_text(mount: Mount) -> &'static str {
    match mount {
        Mount::Floor => "floor",
        Mount::Wall => "wall",
    }
}

/// The fill trait of a class.
fn fill_text(fill: Fill) -> &'static str {
    match fill {
        Fill::HeightAndDepth => "height-and-depth",
        Fill::WidthAndDepth => "width-and-depth",
    }
}

/// The name of one machining operation.
fn operation_text(operation: model::Operation) -> &'static str {
    match operation {
        model::Operation::Contour => "contour",
        model::Operation::EdgeBand => "edge-band",
        model::Operation::Holes => "holes",
        model::Operation::Grooves => "grooves",
        model::Operation::CupBore => "cup-bore",
        model::Operation::MountPattern => "mount-pattern",
        model::Operation::Cutout => "cutout",
    }
}

/// Converts millimetres to whole micrometres, falling back to zero for a non-finite value.
fn micrometres(value: f64) -> i64 {
    rounded_i64(value * 1000.0).unwrap_or(0)
}

/// Rounds a finite value to a whole integer, or `None` when it is not finite or out of range.
fn rounded_i64(value: f64) -> Option<i64> {
    if !value.is_finite() {
        return None;
    }
    let rounded = value.round();
    if rounded < i64::MIN as f64 || rounded > i64::MAX as f64 {
        return None;
    }
    Some(rounded as i64)
}

/// One quaternion component, scaled to the manifest's integer scale.
fn scaled_quaternion(value: f64) -> i64 {
    rounded_i64(value * ROTATION_SCALE as f64).unwrap_or(0)
}
