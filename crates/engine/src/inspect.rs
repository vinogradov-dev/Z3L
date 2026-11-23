//! The inspector: the mistakes a rule can prove, found in one pass and explained by a model.
//!
//! A person can draw a carcass; a shelf four millimetres too wide, a rail through a door and a part
//! longer than the sheet are the mistakes that cost a workshop money, and a model that "looks" at the
//! scene cannot be trusted to see them (ADR 0191). This module is the rules: seven codes, each
//! reading the derivation that already owns its answer — the product's authored envelope, the
//! contact port's own-surface overlap, the class registry, the panel banding and the catalogue's
//! stock sheet — so the report and the editor cannot disagree about a number.
//!
//! A finding is a report and never document data: it names the document ids it blames, says which
//! code and severity it is, carries one sentence and the commands a fix would dispatch. Nothing is
//! stored, nothing is written and the pass is safe in the middle of a person's edit. The model does
//! not compute a finding; it reads the list and explains what a person would see in the workshop
//! (ADR 0191, ADR 0193).
//!
//! The shop's own numbers come from [`WorkshopProfile`], never from a literal: a workshop with
//! another sheet or another gap changes one value and the rules follow. A rule that cannot name the
//! node it blames does not ship, and a rule with no honest fix carries an empty command list rather
//! than a guess.
//!
//! Changes when: a code is added, a rule's threshold moves into the profile, or a finding's sentence
//! changes.

use std::cmp::Ordering;

use catalog::Directory;
use geom::{Contact, DVec2, DVec3, Region};
use model::{
    ClassCode, EdgeRef, Envelope, Id, Nesting, Node, Opening, Operation, PanelNominalSize, PanelOutline, Placement,
    Project, RectEdge, WorkshopProfile,
};
use serde::Serialize;
use serde_json::{Value, json};
use text::{Lang, Message};
use units::Length;

use crate::node_cmds::NodePlacementSet;
use crate::panel_cmds::{PanelEdgeBandingSet, PanelResize};

/// The document's own step, in millimetres: a length closer than this is the same length.
const TOLERANCE_MM: f64 = 1.0e-3;

/// The cosine above which two unit directions count as the same one.
const PARALLEL: f64 = 1.0e-6;

/// The largest gap at which two fronts still form one row, in millimetres.
///
/// A pair of fronts shares an opening, and its gap is a few millimetres; two fronts half a metre
/// apart are two openings and are measured alone.
const ROW_GAP_MM: f64 = 50.0;

/// The numeric field of a rule, in the order the record lists the seven codes.
///
/// The id is the stable English identifier a client reads, and the order is the stable order of the
/// report: severity first, then this order (ADR 0191).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Code {
    /// A node's bounds leave the product's authored envelope.
    EnvelopeOut,
    /// Two bodies share volume.
    BodyOverlap,
    /// A shelf does not fit the opening between its uprights.
    ShelfFit,
    /// A hanging rail stands where the workshop does not hang it.
    RailPlace,
    /// A visible edge carries no banding, or a hidden one does.
    EdgeBand,
    /// A front is the wrong size for its opening, or a pair's gap is wrong.
    DoorGap,
    /// A part does not fit the sheet it is cut from.
    SheetFit,
}

impl Code {
    /// Every code, in the report's stable order.
    pub const ALL: [Self; 7] = [
        Self::EnvelopeOut,
        Self::BodyOverlap,
        Self::ShelfFit,
        Self::RailPlace,
        Self::EdgeBand,
        Self::DoorGap,
        Self::SheetFit,
    ];

    /// The stable id a client and a fixture name.
    pub const fn id(self) -> &'static str {
        match self {
            Self::EnvelopeOut => "envelope_out",
            Self::BodyOverlap => "body_overlap",
            Self::ShelfFit => "shelf_fit",
            Self::RailPlace => "rail_place",
            Self::EdgeBand => "edge_band",
            Self::DoorGap => "door_gap",
            Self::SheetFit => "sheet_fit",
        }
    }

    /// The order of the code inside one severity.
    const fn rank(self) -> u8 {
        match self {
            Self::EnvelopeOut => 0,
            Self::BodyOverlap => 1,
            Self::ShelfFit => 2,
            Self::RailPlace => 3,
            Self::EdgeBand => 4,
            Self::DoorGap => 5,
            Self::SheetFit => 6,
        }
    }
}

/// How much a finding matters: a mistake that must be repaired, one worth a look, or a fact.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// The piece is wrong and the workshop would refuse it.
    Error,
    /// The piece can be made and a person should look at it.
    Warning,
    /// A fact about the piece that is not a defect.
    Info,
}

impl Severity {
    /// The word a client and a sentence read.
    pub const fn word(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
        }
    }

    /// The order of the report: errors, then warnings, then facts.
    const fn rank(self) -> u8 {
        match self {
            Self::Error => 0,
            Self::Warning => 1,
            Self::Info => 2,
        }
    }
}

/// One command a fix would dispatch: the stable id and the parameters it takes.
///
/// The parameters are the command's own JSON, in the units its schema names, so a caller can run
/// the step unchanged or hand it to a person. A rule with no honest fix carries an empty list.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SuggestedCommand {
    /// The stable command id.
    pub command: String,
    /// The parameters the command's own schema describes.
    pub params: Value,
}

/// One mistake, as the inspector reports it.
#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
    /// The stable code.
    pub code: Code,
    /// How much it matters.
    pub severity: Severity,
    /// The document ids the finding blames, never a mesh index.
    pub nodes: Vec<Id>,
    /// The sentence's key; [`Self::text`] fills its numbers.
    pub message: Message,
    /// The numbers the sentence carries, already formatted.
    pub args: Vec<String>,
    /// The commands a fix would dispatch, in the order a person should run them.
    pub suggested_commands: Vec<SuggestedCommand>,
}

impl Finding {
    /// The sentence a person reads, in `lang`.
    pub fn text(&self, lang: Lang) -> String {
        self.message.with(lang, &self.args)
    }

    /// The JSON a client reads: the wire's own English, its stable words and the commands.
    pub fn wire(&self) -> Value {
        json!({
            "code": self.code.id(),
            "severity": self.severity.word(),
            "nodes": self.nodes.iter().map(|id| id.get()).collect::<Vec<_>>(),
            "text": self.text(Lang::En),
            "suggested_commands": self.suggested_commands,
        })
    }
}

/// Runs the seven rules over the document and returns the findings in the report's stable order.
///
/// The pass reads the project, the catalogue, the profile and the contact adapter and writes
/// nothing; a product with no authored envelope, or a node the document cannot measure, is skipped
/// rather than guessed at.
pub fn inspect(
    project: &Project,
    directory: &Directory,
    profile: &WorkshopProfile,
    contact: &dyn Contact,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    for product in project.products() {
        let Some(container) = project.container(product) else {
            continue;
        };
        let Some(envelope) = container.envelope().cloned() else {
            continue;
        };
        let parts = parts_of(project, product);
        if parts.is_empty() {
            continue;
        }
        envelope_out(project, product, &envelope, &parts, &mut findings);
        body_overlap(project, &parts, contact, &mut findings);
        shelf_fit(project, product, &parts, profile, &mut findings);
        rail_place(project, product, &envelope, &parts, profile, &mut findings);
        edge_band(project, product, &parts, &mut findings);
        door_gap(project, product, &envelope, &parts, profile, &mut findings);
        sheet_fit(project, directory, &parts, profile, &mut findings);
    }
    findings.sort_by(|first, second| {
        (first.severity.rank(), first.code.rank(), first.nodes.first().copied()).cmp(&(
            second.severity.rank(),
            second.code.rank(),
            second.nodes.first().copied(),
        ))
    });
    findings.dedup();
    findings
}

/// One surface as a box in the product's own frame: the world box of the node, carried back.
fn box_in(project: &Project, product: Id, id: Id) -> Option<(DVec3, DVec3)> {
    let (min, max) = world_box(project, id)?;
    let inverse = project.world_placement(product)?.inverse().ok()?;
    aabb(corners(min, max).map(|corner| inverse.transform_point(corner)))
}

/// The world box a node occupies, measured by its own authored box.
///
/// A product is its envelope and every other node is the box the document keeps for it, so a turned
/// part is measured by its own corners and never by a second, looser box.
fn world_box(project: &Project, id: Id) -> Option<(DVec3, DVec3)> {
    if let Some(container) = project.container(id)
        && container.is_product()
    {
        let envelope = container.envelope()?;
        let max =
            DVec3::new(envelope.x().millimetres_f64(), envelope.y().millimetres_f64(), envelope.z().millimetres_f64());
        let world = project.world_placement(id)?;
        return aabb(corners(DVec3::ZERO, max).map(|corner| world.transform_point(corner)));
    }
    let (min, max) = project.bounds_local(id)?;
    let world = project.world_placement(id)?;
    aabb(corners(min, max).map(|corner| world.transform_point(corner)))
}

/// The eight corners of an axis-aligned box.
fn corners(min: DVec3, max: DVec3) -> impl Iterator<Item = DVec3> {
    let mut points = Vec::with_capacity(8);
    for x in [min.x, max.x] {
        for y in [min.y, max.y] {
            for z in [min.z, max.z] {
                points.push(DVec3::new(x, y, z));
            }
        }
    }
    points.into_iter()
}

/// The tightest box around a set of points, or `None` when a point is not finite.
fn aabb(points: impl Iterator<Item = DVec3>) -> Option<(DVec3, DVec3)> {
    let mut low = DVec3::splat(f64::INFINITY);
    let mut high = DVec3::splat(f64::NEG_INFINITY);
    let mut seen = false;
    for point in points {
        if !point.is_finite() {
            return None;
        }
        low = low.min(point);
        high = high.max(point);
        seen = true;
    }
    (seen && low.is_finite() && high.is_finite()).then_some((low, high))
}

/// The parts of one product: every drawable leaf below it, with a nested product treated as one
/// shell rather than as its children.
fn parts_of(project: &Project, product: Id) -> Vec<Id> {
    fn walk(node: &Node, out: &mut Vec<Id>, depth: u32) {
        if depth > Project::MAX_ASSEMBLY_DEPTH {
            return;
        }
        match node {
            Node::Panel(panel) => out.push(panel.id()),
            Node::Object(object) => {
                if !object.is_tool() {
                    out.push(object.id());
                }
            }
            Node::Container(container) => {
                if container.is_product() {
                    out.push(container.id());
                } else {
                    for child in container.children() {
                        walk(child, out, depth.saturating_add(1));
                    }
                }
            }
            Node::Camera(_) | Node::Light(_) => {}
        }
    }
    let mut parts = Vec::new();
    let Some(container) = project.container(product) else {
        return parts;
    };
    for child in container.children() {
        walk(child, &mut parts, 1);
    }
    parts
}

/// The name a sentence calls a node by: its own, or its id when it has none.
fn node_name(project: &Project, id: Id) -> String {
    project
        .node(id)
        .map(Node::name)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| format!("#{id}"))
}

/// The traits of a node's class, or `None` for an unclassified one.
fn traits_of(project: &Project, id: Id) -> Option<model::TraitSet> {
    project.class(id).filter(|code| !code.is_unclassified())?.traits()
}

/// Whether a node's class is `code` or a variant under it.
fn is_a(project: &Project, id: Id, code: ClassCode) -> bool {
    project.class(id).is_some_and(|class| !class.is_unclassified() && class.is_ancestor_or_self(code))
}

/// A number in millimetres, as a sentence writes it: whole when it is whole.
fn mm_text(value: f64) -> String {
    if !value.is_finite() || value.abs() > 1.0e12 {
        return "0".to_owned();
    }
    let rounded = (value * 10.0).round() / 10.0;
    if (rounded - rounded.round()).abs() < f64::EPSILON {
        format!("{}", rounded as i64)
    } else {
        format!("{rounded:.1}")
    }
}

/// A length as micrometres, clamped into the document's positive range.
fn micrometres(value: f64) -> Option<Length> {
    let round = (value * 1_000.0).round();
    if !round.is_finite() || round < 1.0 || round > 1.0e15 {
        return None;
    }
    Some(Length::from_micrometres(round as i64))
}

/// The shared length of two spans, zero when they only touch.
fn shared(first: (f64, f64), second: (f64, f64)) -> f64 {
    (first.1.min(second.1) - first.0.max(second.0)).max(0.0)
}

/// Whether two spans share more than the document's rounding.
fn overlaps(first: (f64, f64), second: (f64, f64)) -> bool {
    shared(first, second) > TOLERANCE_MM
}

/// The span of a box along one product axis.
fn span(box_: (DVec3, DVec3), axis: Axis) -> (f64, f64) {
    match axis {
        Axis::X => (box_.0.x, box_.1.x),
        Axis::Y => (box_.0.y, box_.1.y),
    }
}

/// A horizontal axis of the product's own frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Axis {
    /// The product's width.
    X,
    /// The product's depth.
    Y,
}

/// The product axis a panel's own length runs along, or `None` for a length that is not horizontal.
fn length_axis(project: &Project, product: Id, id: Id) -> Option<Axis> {
    let world = project.world_placement(id)?;
    let inverse = project.world_placement(product)?.inverse().ok()?;
    let along = inverse.transform_vector(world.transform_vector(DVec3::X));
    if along.x.abs() >= along.y.abs() { Some(Axis::X) } else { Some(Axis::Y) }
}

/// Which dimension of a panel runs along a product axis, for a resize a fix proposes.
fn dimension_along(project: &Project, product: Id, id: Id) -> Option<crate::class_rules::Dimension> {
    let world = project.world_placement(id)?;
    let inverse = project.world_placement(product)?.inverse().ok()?;
    let length = inverse.transform_vector(world.transform_vector(DVec3::X));
    let width = inverse.transform_vector(world.transform_vector(DVec3::Y));
    if length.x.abs() >= width.x.abs() {
        Some(crate::class_rules::Dimension::Length)
    } else {
        Some(crate::class_rules::Dimension::Width)
    }
}

/// Pushes one finding with its sentence and its commands.
fn push(
    out: &mut Vec<Finding>,
    code: Code,
    severity: Severity,
    nodes: Vec<Id>,
    message: Message,
    args: Vec<String>,
    suggested_commands: Vec<SuggestedCommand>,
) {
    out.push(Finding { code, severity, nodes, message, args, suggested_commands });
}

/// The commands a typed step would dispatch, or an empty list when its parameters cannot be built.
fn suggest<T: Serialize>(command: &str, params: &T) -> Vec<SuggestedCommand> {
    match serde_json::to_value(params) {
        Ok(params) => vec![SuggestedCommand { command: command.to_owned(), params }],
        Err(_) => Vec::new(),
    }
}

/// `envelope_out`: a node's authored or measured box leaves the product's reserved space.
fn envelope_out(project: &Project, product: Id, envelope: &Envelope, parts: &[Id], out: &mut Vec<Finding>) {
    let width = envelope.x().millimetres_f64();
    let depth = envelope.y().millimetres_f64();
    let height = envelope.z().millimetres_f64();
    for id in parts {
        let Some((min, max)) = box_in(project, product, *id) else {
            continue;
        };
        let mut worst = 0.0;
        let mut side = "X";
        for corner in corners(min, max) {
            let inside = envelope.holds(DVec2::new(corner.x, corner.y));
            let (spill, word) = if inside {
                if corner.z > height { (corner.z - height, "+Z") } else { (-corner.z, "-Z") }
            } else if envelope.footprint().is_rectangle() {
                [(-corner.x, "-X"), (corner.x - width, "+X"), (-corner.y, "-Y"), (corner.y - depth, "+Y")]
                    .into_iter()
                    .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal))
                    .unwrap_or((0.0, "X"))
            } else {
                (footprint_distance(envelope, DVec2::new(corner.x, corner.y)), "XY")
            };
            if spill > worst {
                worst = spill;
                side = word;
            }
        }
        if worst > TOLERANCE_MM {
            push(
                out,
                Code::EnvelopeOut,
                Severity::Error,
                vec![*id],
                Message::FindingEnvelopeOut,
                vec![node_name(project, *id), mm_text(worst), side.to_owned()],
                Vec::new(),
            );
        }
    }
}

/// How far a point lies outside a product's base shape, in millimetres.
fn footprint_distance(envelope: &Envelope, point: DVec2) -> f64 {
    let Ok(ring) = envelope.footprint().outline(envelope.x(), envelope.y()) else {
        return 0.0;
    };
    Region::new(ring, Vec::new()).ok().map_or(0.0, |region| region.boundary_distance(point))
}

/// `body_overlap`: two parts of one product share volume, while a body in its own case stays legal.
fn body_overlap(project: &Project, parts: &[Id], contact: &dyn Contact, out: &mut Vec<Finding>) {
    let measured: Vec<(Id, (DVec3, DVec3))> =
        parts.iter().filter_map(|id| project.world_box(*id).map(|box_| (*id, box_))).collect();
    for first in 0..measured.len() {
        for second in first + 1..measured.len() {
            let (Some((a, a_box)), Some((b, b_box))) = (measured.get(first), measured.get(second)) else {
                continue;
            };
            if !overlaps((a_box.0.x, a_box.1.x), (b_box.0.x, b_box.1.x))
                || !overlaps((a_box.0.y, a_box.1.y), (b_box.0.y, b_box.1.y))
                || !overlaps((a_box.0.z, a_box.1.z), (b_box.0.z, b_box.1.z))
            {
                continue;
            }
            if project.parts_overlap(*a, *b, Some(contact)) {
                push(
                    out,
                    Code::BodyOverlap,
                    Severity::Error,
                    vec![*a, *b],
                    Message::FindingBodyOverlap,
                    vec![node_name(project, *a), node_name(project, *b)],
                    Vec::new(),
                );
            }
        }
    }
}

/// The nearest uprights on either side of a horizontal part, and the clear opening between them.
///
/// A part rests in an opening when an upright of the product meets it at the same height on each
/// side of its length; a load-bearing bottom, a top the uprights stand under and a part hanging in
/// the air have no such pair and answer `None`, which is what keeps the shelf rule to shelves.
fn opening_around(part: (DVec3, DVec3), axis: Axis, uprights: &[(DVec3, DVec3)]) -> Option<(f64, f64)> {
    let span_range = span(part, axis);
    let height = (part.0.z, part.1.z);
    let mut left: Option<f64> = None;
    let mut right: Option<f64> = None;
    for box_ in uprights {
        if !overlaps(height, (box_.0.z, box_.1.z)) {
            continue;
        }
        let other = span(*box_, axis);
        let other_centre = (other.0 + other.1) / 2.0;
        let part_centre = (span_range.0 + span_range.1) / 2.0;
        if other.1 <= span_range.0 + TOLERANCE_MM {
            left = Some(left.map_or(other.1, |current: f64| current.max(other.1)));
        } else if other.0 >= span_range.1 - TOLERANCE_MM {
            right = Some(right.map_or(other.0, |current: f64| current.min(other.0)));
        } else if other_centre <= part_centre {
            // A part that runs into its upright is still measured against it: the upright's inner
            // face bounds the opening even when the part has crossed it. The overlap itself is the
            // body rule's finding, so the same mistake is reported by both codes and not hidden.
            left = Some(left.map_or(other.1, |current: f64| current.max(other.1)));
        } else {
            right = Some(right.map_or(other.0, |current: f64| current.min(other.0)));
        }
    }
    match (left, right) {
        (Some(left), Some(right)) if right > left => Some((left, right)),
        _ => None,
    }
}

/// `shelf_fit`: a horizontal panel is the width of the opening it crosses, less its clearance.
///
/// The rule reads a shelf by geometry and not by a name: the class registry keeps a bottom, a top and
/// a shelf in one row (`10.11`), so what makes a shelf is an upright on each side at its own height
/// (ADR 0094, `docs/domain/classes.md` §6.4). A load-bearing bottom and an overlaid top have none.
fn shelf_fit(project: &Project, product: Id, parts: &[Id], profile: &WorkshopProfile, out: &mut Vec<Finding>) {
    let gap = profile.shelf_gap.millimetres_f64();
    let loose = profile.shelf_loose.millimetres_f64();
    let horizontals: Vec<(Id, (DVec3, DVec3))> = parts
        .iter()
        .filter(|id| is_a(project, **id, ClassCode::HORIZONTAL_PANEL))
        .filter_map(|id| box_in(project, product, *id).map(|box_| (*id, box_)))
        .collect();
    let uprights: Vec<(DVec3, DVec3)> = parts
        .iter()
        .filter(|id| is_a(project, **id, ClassCode::VERTICAL_PANEL))
        .filter_map(|id| box_in(project, product, *id))
        .collect();
    for (id, part) in &horizontals {
        let Some(axis) = length_axis(project, product, *id) else {
            continue;
        };
        let Some((left, right)) = opening_around(*part, axis, &uprights) else {
            continue;
        };
        let opening = right - left;
        let width = span(*part, axis);
        let measured = width.1 - width.0;
        let target = opening - 2.0 * gap;
        let suggestion = |target: f64| -> Vec<SuggestedCommand> {
            let Some(panel) = project.panel(*id) else {
                return Vec::new();
            };
            let Some(dimension) = dimension_along(project, product, *id) else {
                return Vec::new();
            };
            let size = panel.nominal_size();
            let (length, width) = (size.length(), size.width());
            let (length, width) = match dimension {
                crate::class_rules::Dimension::Length => (micrometres(target), Some(width)),
                crate::class_rules::Dimension::Width => (Some(length), micrometres(target)),
                crate::class_rules::Dimension::Thickness => return Vec::new(),
            };
            let (Some(length), Some(width)) = (length, width) else {
                return Vec::new();
            };
            let Ok(nominal_size) = PanelNominalSize::new(length, width) else {
                return Vec::new();
            };
            suggest("panel.resize", &PanelResize { panel: *id, nominal_size })
        };
        if measured > opening + TOLERANCE_MM {
            push(
                out,
                Code::ShelfFit,
                Severity::Error,
                vec![*id],
                Message::FindingShelfWide,
                vec![node_name(project, *id), mm_text(measured - opening), mm_text(opening)],
                suggestion(target),
            );
        } else if target - measured > loose + TOLERANCE_MM {
            push(
                out,
                Code::ShelfFit,
                Severity::Warning,
                vec![*id],
                Message::FindingShelfLoose,
                vec![node_name(project, *id), mm_text(target - measured), mm_text(opening)],
                suggestion(target),
            );
        }
    }
}

/// `rail_place`: a hanging rail runs along the width, sits in its compartment and hangs where the
/// workshop hangs it.
fn rail_place(
    project: &Project,
    product: Id,
    envelope: &Envelope,
    parts: &[Id],
    profile: &WorkshopProfile,
    out: &mut Vec<Finding>,
) {
    let drop_wanted = profile.rail_drop.millimetres_f64();
    let drop_slack = profile.rail_tolerance.millimetres_f64();
    let clearance = profile.rail_clearance.millimetres_f64();
    let horizontals: Vec<(Id, (DVec3, DVec3))> = parts
        .iter()
        .filter(|id| is_a(project, **id, ClassCode::HORIZONTAL_PANEL))
        .filter_map(|id| box_in(project, product, *id).map(|box_| (*id, box_)))
        .collect();
    let uprights: Vec<(DVec3, DVec3)> = parts
        .iter()
        .filter(|id| is_a(project, **id, ClassCode::VERTICAL_PANEL))
        .filter_map(|id| box_in(project, product, *id))
        .collect();
    for id in parts.iter().filter(|id| is_a(project, **id, ClassCode::HANGING_RAIL)) {
        let Some(rail) = box_in(project, product, *id) else {
            continue;
        };
        let name = node_name(project, *id);
        let x_span = (rail.0.x, rail.1.x);
        let y_span = (rail.0.y, rail.1.y);
        let z_span = (rail.0.z, rail.1.z);
        let centre = (rail.0.z + rail.1.z) / 2.0;
        let along_x = (rail.1.x - rail.0.x) >= (rail.1.y - rail.0.y);
        let mut outside = !along_x;
        let mut left: Option<f64> = None;
        let mut right: Option<f64> = None;
        for box_ in &uprights {
            if !overlaps(z_span, (box_.0.z, box_.1.z)) {
                continue;
            }
            let upright_span = (box_.0.x, box_.1.x);
            if overlaps(x_span, upright_span) {
                outside = true;
            } else if upright_span.1 <= x_span.0 + TOLERANCE_MM {
                left = Some(left.map_or(upright_span.1, |current: f64| current.max(upright_span.1)));
            } else if upright_span.0 >= x_span.1 - TOLERANCE_MM {
                right = Some(right.map_or(upright_span.0, |current: f64| current.min(upright_span.0)));
            }
        }
        let compartment = match (left, right) {
            (Some(left), Some(right)) => (left, right),
            _ => (0.0, envelope.x().millimetres_f64()),
        };
        if x_span.0 < compartment.0 - TOLERANCE_MM || x_span.1 > compartment.1 + TOLERANCE_MM {
            outside = true;
        }
        let depth = envelope.y().millimetres_f64();
        if y_span.0 < -TOLERANCE_MM || y_span.1 > depth + TOLERANCE_MM {
            outside = true;
        }
        if outside {
            push(out, Code::RailPlace, Severity::Error, vec![*id], Message::FindingRailOutside, vec![name], Vec::new());
            continue;
        }
        let above = horizontals
            .iter()
            .filter(|(shelf, box_)| *shelf != *id && upper_overlap(rail, *box_))
            .min_by(|a, b| a.1.0.z.partial_cmp(&b.1.0.z).unwrap_or(Ordering::Equal));
        let top = above.map_or(envelope.z().millimetres_f64(), |(_, box_)| box_.0.z);
        let drop = top - centre;
        if (drop - drop_wanted).abs() > drop_slack + TOLERANCE_MM {
            push(
                out,
                Code::RailPlace,
                Severity::Warning,
                vec![*id],
                Message::FindingRailDrop,
                vec![name.clone(), mm_text(drop), mm_text(drop_wanted)],
                rail_fix(project, *id, top - drop_wanted - centre),
            );
        }
        for (shelf, box_) in &horizontals {
            if *shelf == *id || !overlaps(x_span, (box_.0.x, box_.1.x)) || !overlaps(y_span, (box_.0.y, box_.1.y)) {
                continue;
            }
            if box_.1.z <= z_span.0 + TOLERANCE_MM {
                let clear = z_span.0 - box_.1.z;
                if clear < clearance - TOLERANCE_MM {
                    push(
                        out,
                        Code::RailPlace,
                        Severity::Warning,
                        vec![*id, *shelf],
                        Message::FindingRailClearance,
                        vec![name.clone(), mm_text(clear), mm_text(clearance)],
                        Vec::new(),
                    );
                }
            }
        }
    }
}

/// Whether a horizontal part stands above the rail and shares its footprint.
fn upper_overlap(rail: (DVec3, DVec3), shelf: (DVec3, DVec3)) -> bool {
    shelf.0.z >= rail.1.z - TOLERANCE_MM
        && overlaps((rail.0.x, rail.1.x), (shelf.0.x, shelf.1.x))
        && overlaps((rail.0.y, rail.1.y), (shelf.0.y, shelf.1.y))
}

/// The placement that moves a rail by `delta` millimetres along the world's vertical.
fn rail_fix(project: &Project, id: Id, delta: f64) -> Vec<SuggestedCommand> {
    if !delta.is_finite() || delta.abs() <= TOLERANCE_MM {
        return Vec::new();
    }
    let (Some(parent), Some(node)) = (project.parent_of(id), project.node(id)) else {
        return Vec::new();
    };
    let Some(parent_world) = project.world_placement(parent) else {
        return Vec::new();
    };
    let local = parent_world.rotation().inverse() * DVec3::new(0.0, 0.0, delta);
    let placement = node.placement();
    let Ok(placement) = Placement::new(placement.translation() + local, placement.rotation()) else {
        return Vec::new();
    };
    suggest("node.placement.set", &NodePlacementSet { node: id, placement, unmeasured: false })
}

/// The outward direction of one edge of a rectangular panel, in the panel's own frame.
fn edge_normal(edge: RectEdge) -> DVec3 {
    match edge {
        RectEdge::Bottom => -DVec3::Y,
        RectEdge::Right => DVec3::X,
        RectEdge::Top => DVec3::Y,
        RectEdge::Left => -DVec3::X,
    }
}

/// `edge_band`: a maker sees the front end of every carcass board and every edge of a front.
fn edge_band(project: &Project, product: Id, parts: &[Id], out: &mut Vec<Finding>) {
    let Some(product_world) = project.world_placement(product) else {
        return;
    };
    let front = product_world.transform_vector(DVec3::Y);
    for id in parts {
        let Some(panel) = project.panel(*id) else {
            continue;
        };
        let Some(traits) = traits_of(project, *id) else {
            continue;
        };
        if !traits.operations().contains(Operation::EdgeBand) {
            continue;
        }
        if matches!(panel.outline(), PanelOutline::Contour { .. }) {
            continue;
        }
        let Some(world) = project.world_placement(*id) else {
            continue;
        };
        let closes = traits.opening() == Some(Opening::Closes);
        let banding = panel.edge_banding().first().map(|band| (band.material(), band.included_in_size()));
        for edge in RectEdge::ALL {
            let normal = world.transform_vector(edge_normal(edge));
            let visible = closes || normal.dot(front) > 1.0 - PARALLEL;
            let banded = panel.has_band_on(EdgeRef::Rect { edge: edge.id() });
            match (visible, banded) {
                (true, false) => {
                    let commands = banding.map_or_else(Vec::new, |(material, included_in_size)| {
                        suggest(
                            "panel.edge_banding.set",
                            &PanelEdgeBandingSet { panel: *id, edge: edge.id(), material, included_in_size },
                        )
                    });
                    push(
                        out,
                        Code::EdgeBand,
                        Severity::Warning,
                        vec![*id],
                        Message::FindingEdgeMissing,
                        vec![node_name(project, *id), edge.label().to_owned()],
                        commands,
                    );
                }
                (false, true) => push(
                    out,
                    Code::EdgeBand,
                    Severity::Info,
                    vec![*id],
                    Message::FindingEdgeHidden,
                    vec![node_name(project, *id), edge.label().to_owned()],
                    Vec::new(),
                ),
                _ => {}
            }
        }
    }
}

/// `door_gap`: the fronts of one opening share it, and the gap between them is the workshop's.
fn door_gap(
    project: &Project,
    product: Id,
    envelope: &Envelope,
    parts: &[Id],
    profile: &WorkshopProfile,
    out: &mut Vec<Finding>,
) {
    let gap = profile.door_gap.millimetres_f64();
    let tolerance = profile.door_tolerance.millimetres_f64();
    let opening = envelope.x().millimetres_f64();
    let mut fronts: Vec<(Id, (DVec3, DVec3))> = parts
        .iter()
        .filter(|id| traits_of(project, **id).and_then(|traits| traits.opening()) == Some(Opening::Closes))
        .filter_map(|id| box_in(project, product, *id).map(|box_| (*id, box_)))
        .collect();
    fronts.sort_by(|first, second| first.1.0.x.partial_cmp(&second.1.0.x).unwrap_or(Ordering::Equal));
    let mut row: Vec<(Id, (DVec3, DVec3))> = Vec::new();
    for front in fronts {
        if let Some(last) = row.last() {
            let adjacent = overlaps((last.1.0.z, last.1.1.z), (front.1.0.z, front.1.1.z))
                && overlaps((last.1.0.y, last.1.1.y), (front.1.0.y, front.1.1.y))
                && front.1.0.x - last.1.1.x <= ROW_GAP_MM;
            if !adjacent {
                check_row(project, product, &row, opening, gap, tolerance, out);
                row.clear();
            }
        }
        row.push(front);
    }
    check_row(project, product, &row, opening, gap, tolerance, out);
    for (front, _) in parts
        .iter()
        .filter(|id| traits_of(project, **id).and_then(|traits| traits.opening()) == Some(Opening::Closes))
        .filter_map(|id| box_in(project, product, *id).map(|box_| (*id, box_)))
    {
        for side in parts.iter().filter(|id| is_a(project, **id, ClassCode::VERTICAL_PANEL)) {
            if project.parts_overlap(front, *side, None) {
                push(
                    out,
                    Code::DoorGap,
                    Severity::Error,
                    vec![front, *side],
                    Message::FindingDoorOverlap,
                    vec![node_name(project, front), node_name(project, *side)],
                    Vec::new(),
                );
            }
        }
    }
}

/// Checks one row of fronts against the opening it shares.
fn check_row(
    project: &Project,
    product: Id,
    row: &[(Id, (DVec3, DVec3))],
    opening: f64,
    gap: f64,
    tolerance: f64,
    out: &mut Vec<Finding>,
) {
    let Some(first) = row.first() else {
        return;
    };
    if row.len() == 1 {
        let width = first.1.1.x - first.1.0.x;
        let wanted = opening - 2.0 * gap;
        if (width - wanted).abs() > tolerance + TOLERANCE_MM {
            push(
                out,
                Code::DoorGap,
                Severity::Warning,
                vec![first.0],
                Message::FindingDoorSize,
                vec![node_name(project, first.0), mm_text(width), mm_text(wanted)],
                resize_along(project, product, first.0, wanted),
            );
        }
        return;
    }
    let count = row.len() as f64;
    let wanted = (opening - (count - 1.0) * gap) / count;
    for pair in row.windows(2) {
        let (Some(left), Some(right)) = (pair.first(), pair.get(1)) else {
            continue;
        };
        let measured = right.1.0.x - left.1.1.x;
        if measured < -TOLERANCE_MM {
            push(
                out,
                Code::DoorGap,
                Severity::Error,
                vec![left.0, right.0],
                Message::FindingDoorOverlap,
                vec![node_name(project, left.0), node_name(project, right.0)],
                Vec::new(),
            );
        } else if (measured - gap).abs() > tolerance + TOLERANCE_MM {
            push(
                out,
                Code::DoorGap,
                Severity::Warning,
                vec![left.0, right.0],
                Message::FindingDoorGap,
                vec![node_name(project, left.0), node_name(project, right.0), mm_text(measured), mm_text(gap)],
                Vec::new(),
            );
        }
    }
    for (front, box_) in row {
        let width = box_.1.x - box_.0.x;
        if (width - wanted).abs() > tolerance + TOLERANCE_MM {
            push(
                out,
                Code::DoorGap,
                Severity::Warning,
                vec![*front],
                Message::FindingDoorSize,
                vec![node_name(project, *front), mm_text(width), mm_text(wanted)],
                resize_along(project, product, *front, wanted),
            );
        }
    }
}

/// A `panel.resize` that puts one dimension at `target` millimetres.
fn resize_along(project: &Project, product: Id, id: Id, target: f64) -> Vec<SuggestedCommand> {
    let Some(panel) = project.panel(id) else {
        return Vec::new();
    };
    let Some(dimension) = dimension_along(project, product, id) else {
        return Vec::new();
    };
    let size = panel.nominal_size();
    let (length, width) = match dimension {
        crate::class_rules::Dimension::Length => (micrometres(target), Some(size.width())),
        crate::class_rules::Dimension::Width => (Some(size.length()), micrometres(target)),
        crate::class_rules::Dimension::Thickness => return Vec::new(),
    };
    let (Some(length), Some(width)) = (length, width) else {
        return Vec::new();
    };
    let Ok(nominal_size) = PanelNominalSize::new(length, width) else {
        return Vec::new();
    };
    suggest("panel.resize", &PanelResize { panel: id, nominal_size })
}

/// `sheet_fit`: a part cut from a sheet fits it in one of the two turns a saw allows.
///
/// The document stores no grain direction yet (ADR 0216), so both quarter turns are allowed; the day
/// grain is document data this rule reads it and drops the turn the grain forbids.
fn sheet_fit(
    project: &Project,
    directory: &Directory,
    parts: &[Id],
    profile: &WorkshopProfile,
    out: &mut Vec<Finding>,
) {
    for id in parts {
        let Some(panel) = project.panel(*id) else {
            continue;
        };
        let Some(traits) = traits_of(project, *id) else {
            continue;
        };
        if traits.nesting() != Nesting::Member {
            continue;
        }
        let sheet = panel
            .material()
            .and_then(|material| directory.board(material))
            .and_then(|board| board.sheet)
            .map(|sheet| (sheet.length.millimetres_f64(), sheet.width.millimetres_f64()))
            .unwrap_or((profile.sheet.length.millimetres_f64(), profile.sheet.width.millimetres_f64()));
        let length = panel.nominal_size().length().millimetres_f64();
        let width = panel.nominal_size().width().millimetres_f64();
        let fits = (length <= sheet.0 + TOLERANCE_MM && width <= sheet.1 + TOLERANCE_MM)
            || (length <= sheet.1 + TOLERANCE_MM && width <= sheet.0 + TOLERANCE_MM);
        if !fits {
            push(
                out,
                Code::SheetFit,
                Severity::Error,
                vec![*id],
                Message::FindingSheet,
                vec![node_name(project, *id), mm_text(length), mm_text(width), mm_text(sheet.0), mm_text(sheet.1)],
                Vec::new(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use std::sync::Arc;

    use geom::DQuat;
    use model::{EdgeBanding, PanelNominalSize, Placement, PropertyValue, SYSTEM_CLASS, WorkshopProfile};
    use testkit::{
        sample_band_thickness, sample_band_width, sample_catalog, sample_edge_material_id, sample_material_id,
        sample_project,
    };
    use units::Length;

    use super::*;
    use crate::Session;

    fn mm(value: i64) -> Length {
        Length::from_millimetres(value).expect("a length")
    }

    fn flat(x: f64, y: f64, z: f64) -> Placement {
        Placement::new(DVec3::new(x, y, z), DQuat::IDENTITY).expect("a placement")
    }

    /// The post stance: the board's zero at the bottom, its thickness along the product's X.
    fn post(x: f64, y: f64, z: f64) -> Placement {
        Placement::new(DVec3::new(x, y, z), DQuat::from_rotation_y(-std::f64::consts::FRAC_PI_2))
            .expect("a post placement")
    }

    /// The wall stance: length up, width across the product's X, thickness along Y.
    fn wall(x: f64, y: f64, z: f64) -> Placement {
        Placement::new(DVec3::new(x, y, z), DQuat::from_xyzw(-0.5, -0.5, -0.5, 0.5)).expect("a wall placement")
    }

    fn size(length: i64, width: i64) -> PanelNominalSize {
        PanelNominalSize::new(mm(length), mm(width)).expect("a nominal size")
    }

    fn band(project: &mut Project, panel: Id, edge: RectEdge) {
        let banding =
            EdgeBanding::new(edge.id(), sample_edge_material_id(), true, sample_band_thickness(), sample_band_width())
                .expect("a banding");
        project.set_edge_banding(panel, banding).expect("the banding of one edge");
    }

    /// One clean cabinet: a bottom, two sides, a top, a back, a shelf and a pair of fronts.
    ///
    /// It is the scene every fixture starts from, so a finding in a test is the one thing the test
    /// changed and not a second mistake the scene carried all along.
    struct Cabinet {
        project: Project,
        product: Id,
        shelf: Id,
        left_front: Id,
    }

    impl Cabinet {
        fn clean() -> Self {
            let mut project = sample_project();
            let root = project.root().id();
            let envelope = Envelope::new(mm(600), mm(560), mm(720)).expect("an envelope");
            let product = project
                .add_product(root, "Cabinet", ClassCode::GENERAL_PRODUCT, envelope, Placement::IDENTITY)
                .expect("a product");
            let mut add = |name: &str,
                           length: i64,
                           width: i64,
                           thickness: i64,
                           class: ClassCode,
                           placement: Placement| {
                let id = project
                    .add_panel(product, name, size(length, width), Some(sample_material_id()), mm(thickness), placement)
                    .expect("a panel");
                project.set_property(id, SYSTEM_CLASS, PropertyValue::class(class)).expect("the class of a part");
                id
            };
            let bottom = add("Bottom", 600, 538, 18, ClassCode::HORIZONTAL_PANEL, flat(0.0, 4.0, 0.0));
            let left = add("Side left", 684, 538, 18, ClassCode::VERTICAL_PANEL, post(600.0, 4.0, 18.0));
            let right = add("Side right", 684, 538, 18, ClassCode::VERTICAL_PANEL, post(18.0, 4.0, 18.0));
            let top = add("Top", 600, 538, 18, ClassCode::HORIZONTAL_PANEL, flat(0.0, 4.0, 702.0));
            let _back = add("Back", 720, 600, 4, ClassCode::BACK_PANEL, wall(0.0, 0.0, 0.0));
            let shelf = add("Shelf", 564, 538, 18, ClassCode::HORIZONTAL_PANEL, flat(18.0, 4.0, 351.0));
            let door = "10.20".parse::<ClassCode>().expect("the hinged door row");
            let left_front = add("Front left", 684, 299, 18, door, wall(0.0, 542.0, 18.0));
            let right_front = add("Front right", 684, 299, 18, door, wall(301.0, 542.0, 18.0));
            for panel in [bottom, top, shelf, left, right] {
                band(&mut project, panel, RectEdge::Top);
            }
            for panel in [left_front, right_front] {
                for edge in RectEdge::ALL {
                    band(&mut project, panel, edge);
                }
            }
            Self { project, product, shelf, left_front }
        }

        fn add(
            &mut self,
            name: &str,
            length: i64,
            width: i64,
            thickness: i64,
            class: ClassCode,
            placement: Placement,
        ) -> Id {
            let id = self
                .project
                .add_panel(
                    self.product,
                    name,
                    size(length, width),
                    Some(sample_material_id()),
                    mm(thickness),
                    placement,
                )
                .expect("a panel");
            self.project.set_property(id, SYSTEM_CLASS, PropertyValue::class(class)).expect("the class");
            id
        }

        fn session(&self) -> Session {
            Session::with_directory(self.project.clone(), sample_catalog()).with_contact(Arc::new(contact_parry::Parry))
        }

        fn findings(&self) -> Vec<Finding> {
            self.session().inspect()
        }

        fn codes(&self) -> Vec<&'static str> {
            self.findings().iter().map(|finding| finding.code.id()).collect()
        }
    }

    #[test]
    fn a_clean_cabinet_has_no_finding() {
        let cabinet = Cabinet::clean();
        assert!(cabinet.findings().is_empty(), "a clean piece reports nothing: {:?}", cabinet.codes());
        assert_eq!(cabinet.codes(), Vec::<&str>::new());
    }

    #[test]
    fn a_part_outside_the_envelope_is_an_error() {
        let mut cabinet = Cabinet::clean();
        let far = cabinet.add("Far shelf", 100, 100, 18, ClassCode::HORIZONTAL_PANEL, flat(900.0, 4.0, 100.0));
        let findings = cabinet.findings();
        assert!(
            findings.iter().any(|finding| finding.code == Code::EnvelopeOut && finding.nodes.contains(&far)),
            "the part beyond +X is the finding: {:?}",
            cabinet.codes()
        );
        assert_eq!(findings.iter().filter(|finding| finding.code == Code::EnvelopeOut).count(), 1);
    }

    #[test]
    fn two_parts_in_one_place_are_an_error() {
        let mut cabinet = Cabinet::clean();
        let copy = cabinet.add("Copy", 564, 538, 18, ClassCode::HORIZONTAL_PANEL, flat(18.0, 4.0, 351.0));
        let findings = cabinet.findings();
        let overlap = findings.iter().find(|finding| finding.code == Code::BodyOverlap).expect("the copy overlaps");
        assert!(overlap.nodes.contains(&copy) && overlap.nodes.contains(&cabinet.shelf));
        assert_eq!(overlap.severity, Severity::Error);
    }

    #[test]
    fn a_shelf_wider_than_its_opening_is_an_error_and_names_its_fix() {
        let mut cabinet = Cabinet::clean();
        cabinet.project.set_panel_nominal_size(cabinet.shelf, size(580, 538)).expect("a wider shelf");
        let finding = cabinet
            .findings()
            .into_iter()
            .find(|finding| finding.code == Code::ShelfFit)
            .expect("the shelf no longer fits");
        assert_eq!(finding.severity, Severity::Error);
        assert_eq!(finding.nodes, vec![cabinet.shelf]);
        let fix = finding.suggested_commands.first().expect("a resize is the fix");
        assert_eq!(fix.command, "panel.resize");
        assert_eq!(fix.params["nominal_size"]["length"], 560_000, "the opening less two clearances");
    }

    #[test]
    fn a_loose_shelf_is_a_warning() {
        let mut cabinet = Cabinet::clean();
        cabinet.project.set_panel_nominal_size(cabinet.shelf, size(556, 538)).expect("a loose shelf");
        let finding = cabinet
            .findings()
            .into_iter()
            .find(|finding| finding.code == Code::ShelfFit)
            .expect("the loose shelf is reported");
        assert_eq!(finding.severity, Severity::Warning);
    }

    #[test]
    fn a_rail_below_the_top_is_a_warning_and_a_rail_at_the_drop_is_clean() {
        let mut cabinet = Cabinet::clean();
        let rail = ClassCode::HANGING_RAIL;
        let right = cabinet.add("Rail", 200, 100, 18, rail, flat(200.0, 200.0, 302.0));
        assert!(
            !cabinet.codes().contains(&"rail_place"),
            "a rail 40 mm under the shelf is where the workshop hangs it: {:?}",
            cabinet.codes()
        );
        cabinet.project.set_node_placement(right, flat(200.0, 200.0, 500.0)).expect("a rail hung too low");
        let finding = cabinet
            .findings()
            .into_iter()
            .find(|finding| finding.code == Code::RailPlace)
            .expect("the low rail is reported");
        assert_eq!(finding.severity, Severity::Warning);
        assert_eq!(finding.suggested_commands.first().map(|fix| fix.command.as_str()), Some("node.placement.set"));
    }

    #[test]
    fn a_visible_edge_without_banding_is_a_warning() {
        let mut cabinet = Cabinet::clean();
        cabinet.project.clear_edge_banding(cabinet.shelf, RectEdge::Top.id()).expect("no band");
        let finding = cabinet
            .findings()
            .into_iter()
            .find(|finding| finding.code == Code::EdgeBand && finding.severity == Severity::Warning)
            .expect("the unbanded front edge is reported");
        assert_eq!(finding.nodes, vec![cabinet.shelf]);
    }

    #[test]
    fn a_hidden_edge_that_carries_banding_is_info() {
        let mut cabinet = Cabinet::clean();
        band(&mut cabinet.project, cabinet.shelf, RectEdge::Bottom);
        let finding = cabinet
            .findings()
            .into_iter()
            .find(|finding| finding.code == Code::EdgeBand && finding.severity == Severity::Info)
            .expect("the banded hidden edge is reported");
        assert_eq!(finding.severity, Severity::Info);
    }

    #[test]
    fn a_pair_of_fronts_with_the_wrong_gap_is_a_warning() {
        let mut cabinet = Cabinet::clean();
        cabinet.project.set_panel_nominal_size(cabinet.left_front, size(684, 295)).expect("a narrow front");
        let findings = cabinet.findings();
        assert!(
            findings.iter().any(|finding| finding.code == Code::DoorGap && finding.severity == Severity::Warning),
            "the gap is no longer the workshop's: {:?}",
            cabinet.codes()
        );
    }

    #[test]
    fn a_front_through_a_side_is_an_error() {
        let mut cabinet = Cabinet::clean();
        cabinet
            .project
            .set_node_placement(cabinet.left_front, wall(0.0, 535.0, 18.0))
            .expect("a front pushed into the case");
        let findings = cabinet.findings();
        assert!(
            findings.iter().any(|finding| finding.code == Code::DoorGap && finding.severity == Severity::Error),
            "a front that runs onto a side is an error: {:?}",
            cabinet.codes()
        );
    }

    #[test]
    fn a_part_larger_than_the_sheet_is_an_error() {
        let mut cabinet = Cabinet::clean();
        let long = cabinet.add("Long", 3000, 100, 18, ClassCode::HORIZONTAL_PANEL, flat(0.0, 0.0, 0.0));
        let finding = cabinet
            .findings()
            .into_iter()
            .find(|finding| finding.code == Code::SheetFit)
            .expect("a part longer than the sheet is reported");
        assert_eq!(finding.nodes, vec![long]);
        assert!(finding.suggested_commands.is_empty(), "a shorter part is a decision, not a command");
    }

    #[test]
    fn changing_one_profile_value_changes_only_that_rule() {
        let cabinet = Cabinet::clean();
        let mut session = cabinet.session();
        let defaults = session.inspect();
        assert!(defaults.is_empty(), "the default profile fits this piece");
        session.set_profile(WorkshopProfile { door_gap: mm(3), ..WorkshopProfile::default() });
        let changed = session.inspect();
        assert_eq!(
            changed.iter().map(|finding| finding.code).collect::<Vec<_>>(),
            vec![Code::DoorGap],
            "only the front rule reads the front gap"
        );
    }

    #[test]
    fn every_finding_names_a_node_and_the_report_orders_errors_before_warnings() {
        let mut cabinet = Cabinet::clean();
        cabinet.add("Far", 100, 100, 18, ClassCode::HORIZONTAL_PANEL, flat(900.0, 0.0, 0.0));
        cabinet.add("Long", 3000, 100, 18, ClassCode::HORIZONTAL_PANEL, flat(0.0, 0.0, 0.0));
        cabinet.project.clear_edge_banding(cabinet.shelf, RectEdge::Top.id()).expect("no band");
        cabinet.project.set_panel_nominal_size(cabinet.left_front, size(684, 295)).expect("a narrow front");
        let findings = cabinet.findings();
        assert!(!findings.is_empty());
        for finding in &findings {
            assert!(!finding.nodes.is_empty(), "{} names no node", finding.code.id());
        }
        let severities: Vec<Severity> = findings.iter().map(|finding| finding.severity).collect();
        let sorted = {
            let mut copy = severities.clone();
            copy.sort();
            copy
        };
        assert_eq!(severities, sorted, "errors first, then warnings, then facts");
        let codes: Vec<&str> = findings.iter().map(|finding| finding.code.id()).collect();
        let errors: Vec<&str> = findings
            .iter()
            .filter(|finding| finding.severity == Severity::Error)
            .map(|finding| finding.code.id())
            .collect();
        assert_eq!(&codes[..errors.len()], errors.as_slice(), "the errors hold the front of the list");
    }
}
