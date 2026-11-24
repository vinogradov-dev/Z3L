//! The empty-space tool: the ghost of the part the next click would create.
//!
//! A post or a shelf fills the niche under the pointer. The model answers the measurement
//! ([`Project::niche`], ADR 0103); this module turns the answer into the one panel the click would
//! create and paints it, and touches nothing else. The ghost carries exactly the outline, size,
//! placement and class the command receives, so what the maker sees is what the history records —
//! one `panel.create` per click, one undo entry.
//!
//! `Fill::HeightAndDepth` stands a part up as a post — the niche's height becomes the panel's
//! length, its depth the width, and the material's thickness runs along the free axis, where the
//! pointer sits, away from the face it measured (the post's stance, ADR 0105). `Fill::WidthAndDepth`
//! lays a part down as a shelf — the niche's width and depth become the panel's sides and the pointer
//! picks the height. The free-axis position moves in whole millimetres; the other sides are the
//! measured void exactly, so a part is flush with the walls it was cut to.
//!
//! A shelf in a case whose walls are not plumb takes the section the void leaves it (ADR 0099,
//! ADR 0106): the model cuts the niche's cell at the board's own thickness and answers a polygon, and
//! the click creates either the rectangle panel the tool has always created or a contour panel with
//! exactly that ring, through the same `panel.create`. A post takes the same cut in its own board
//! plane, so it stands against a splayed wall with the depth the slab really has.
//!
//! The ghost is shell paint, not the session preview overlay: an overlay entry is keyed by the id of
//! a node the document holds (ADR 0093), and a part that does not exist yet has none.
//!
//! The niche itself is not measured here: it comes from the one atom every tool that names a void
//! reads ([`ui::Void`], ADR 0118), so a fill and a report row cannot describe two
//! different spaces for one pointer. This module only turns the cell into the part a click would
//! create, and the point it was measured at is the atom's own cursor rule
//! ([`ui::pointer_point`]).

use eframe::egui::{self, FontId, Pos2, Rect, Stroke, Ui};
use geom::DVec3;
use model::{
    EdgeBevel, Fill, FillMode, FillMount, FillRefusal, FillRowAsk, FillRowPlan, FillRowRefusal, Id, Niche,
    PanelNominalSize, PanelOutline, PanelTrims, Placement, Project, Sight,
};
use scene::Camera;
use ui::theme;
use units::Length;

use crate::gizmo_math::screen_point;

/// The panel the next click would create.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FillGhost {
    /// The open product the panel lands in.
    pub(crate) parent: Id,
    /// The outline the panel is born with: the rectangle, or the ring a shaped void leaves.
    pub(crate) outline: PanelOutline,
    /// The panel's nominal length and width, exactly as the command will store them.
    pub(crate) nominal_size: PanelNominalSize,
    /// The panel's placement in the parent frame, exactly as the command will store it.
    pub(crate) placement: Placement,
    /// The void the part was measured against.
    pub(crate) niche: Niche,
    /// The panel's three extents, in millimetres, for the callouts.
    pub(crate) size_mm: DVec3,
    /// The panel's span along the free axis, in the parent frame and millimetres.
    ///
    /// The free axis is the panel's own thickness axis, so the two values are the base face and the
    /// far face. Which of them is the smaller one depends on the stance: a shelf grows upward from
    /// its base face, a post grows away from it, so a tool that assumes the translation is the low
    /// face is right for one stance and wrong for the other (ADR 0105).
    pub(crate) free_low: f64,
    /// The larger end of the panel along the free axis, in the parent frame and millimetres.
    pub(crate) free_high: f64,
    /// The end bevels the shaped fill measured, one per beveled outline edge.
    pub(crate) bevels: Vec<EdgeBevel>,
    /// The end trims the front is laid with, one per nominal edge (ADR 0240).
    ///
    /// A board that fills the opening carries none; a front laid over the frame carries the lap each
    /// end took, so the part the click writes is the front the eye measured.
    pub(crate) trims: PanelTrims,
    /// The outline's corners on the panel's base face, in world millimetres.
    world: Vec<DVec3>,
    /// The far face's corners: the base rim with every beveled edge set back, in the world.
    top: Vec<DVec3>,
    /// The midpoints of the two faces the distance fields measure, low end first.
    faces: [Option<DVec3>; 2],
    /// The panel's own length and width directions in the world, for the callouts.
    axes: [DVec3; 2],
}

impl FillGhost {
    /// The midpoint of the panel's face at the free axis' low end, in world millimetres.
    pub(crate) fn low_face(&self) -> Option<DVec3> {
        self.faces[0]
    }

    /// The midpoint of the panel's face at the free axis' high end, in world millimetres.
    pub(crate) fn high_face(&self) -> Option<DVec3> {
        self.faces[1]
    }
}

/// The average of a ring's corners, or `None` when it is empty or not finite.
fn midpoint(points: &[DVec3]) -> Option<DVec3> {
    if points.is_empty() {
        return None;
    }
    let mut sum = DVec3::ZERO;
    for point in points {
        sum += *point;
    }
    let middle = sum / points.len() as f64;
    middle.is_finite().then_some(middle)
}

/// What the ghost was planned from: the cell, the pointer and the tool's own numbers.
///
/// A plan is rebuilt only when one of these changed, so a still pointer over a still void re-plans
/// nothing; the cell is compared as a value because the atom owns the measurement, not this module.
#[derive(Clone, Debug, PartialEq)]
struct PlanKey {
    niche: Niche,
    point: DVec3,
    fill: Fill,
    mode: FillMode,
    mount: FillMount,
    thickness: Length,
    free: Option<f64>,
}

/// What the fill gesture asks its plan with: the class's pair, the build, the mounting and the board.
///
/// The four travel together because they are one arm's whole vocabulary (AGENTS §1.5): the axis pair
/// the class names, the build the cell ran, the mounting the card holds and the material's thickness.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FillAsk {
    /// The pair of the niche's sides the part takes.
    pub(crate) fill: Fill,
    /// How the part reads the section the walls leave.
    pub(crate) mode: FillMode,
    /// How the board sits against the frame: inside the opening, or laid over its face.
    pub(crate) mount: FillMount,
    /// The material's thickness, which is the part's third side.
    pub(crate) thickness: Length,
    /// The free-axis place a field pinned, or `None` while the pointer carries it.
    pub(crate) free: Option<f64>,
}

/// The row an evenly fill lays, from the anchor the first click made to the second click.
///
/// The anchor is the board the click named, frozen where it stood: every board of the row repeats
/// its outline, size and placement, one pitch further along the free axis (U125). The row itself is
/// a model answer re-derived per frame from the hand's span and the numbers the plate holds, so a
/// still hand re-plans nothing and the ghost is never a second measurement.
#[derive(Clone, Debug)]
pub(crate) struct RowState {
    /// The board the first click anchored.
    pub(crate) anchor: FillGhost,
    /// The direction the row repeats along, in world millimetres, a unit vector.
    pub(crate) direction: DVec3,
    /// The row as it stands, or the reason it cannot be laid.
    pub(crate) plan: Result<FillRowPlan, FillRowRefusal>,
    /// The count a field typed, while the field holds one; `None` lets the hand name it.
    pub(crate) count: Option<usize>,
}

/// The empty-space tool's live state: the ghost, or the reason there is none.
#[derive(Debug, Default)]
pub(crate) struct FillGizmo {
    ghost: Option<FillGhost>,
    refusal: Option<String>,
    /// The cell, the point and the numbers the ghost was planned from, so a still pointer over a
    /// still void plans nothing.
    measured: Option<PlanKey>,
    /// The row an evenly fill is laying, while the gesture holds one (U125).
    row: Option<RowState>,
}

impl FillGizmo {
    /// Drops the ghost and the refusal, for example when the tool is disarmed.
    pub(crate) fn reset(&mut self) {
        self.ghost = None;
        self.refusal = None;
        self.measured = None;
        self.row = None;
    }

    /// The row the evenly fill is laying, if the first click already anchored one.
    pub(crate) fn row(&self) -> Option<&RowState> {
        self.row.as_ref()
    }

    /// Anchors the ghost the click named and starts the row along `direction`.
    ///
    /// The click writes nothing: the anchor is a plan the hand spreads, and the second click is what
    /// makes the boards (U125). A default span of the whole void shows the row the maker would get
    /// before the hand moves.
    pub(crate) fn begin_row(&mut self, direction: DVec3, axis: usize) -> bool {
        let Some(anchor) = self.ghost.clone() else {
            return false;
        };
        let asked = FillRowAsk { start: anchor.free_low, span: None, count: None, gap: None };
        let plan = model::fill_row(&anchor.niche, anchor.size_mm.z, axis, asked);
        self.refusal = None;
        self.row = Some(RowState { anchor, direction, plan, count: None });
        true
    }

    /// Re-plans the anchored row from the hand's span and the plate's numbers.
    ///
    /// `span` is how far the hand travelled from the anchor, `start` the near face a typed field
    /// pinned, `gap` the far distance it fixed and `count` the number it asked for. A number the
    /// model cannot honour leaves the row's refusal on the state, which the plate and the ghost
    /// show; nothing is written and the anchor stays where the click put it.
    pub(crate) fn plan_row(
        &mut self,
        axis: usize,
        span: Option<f64>,
        start: Option<f64>,
        gap: Option<f64>,
        count: Option<usize>,
    ) -> bool {
        let Some(row) = self.row.as_mut() else {
            return false;
        };
        row.count = count;
        let asked = FillRowAsk { start: start.unwrap_or(row.anchor.free_low), span, count, gap };
        let plan = model::fill_row(&row.anchor.niche, row.anchor.size_mm.z, axis, asked);
        let changed = plan != row.plan;
        row.plan = plan;
        changed
    }

    /// Drops the anchored row and leaves the tool armed where it was.
    pub(crate) fn cancel_row(&mut self) -> bool {
        self.row.take().is_some()
    }

    /// The part the next click would create, if any.
    pub(crate) fn ghost(&self) -> Option<&FillGhost> {
        self.ghost.as_ref()
    }

    /// Plans the part a click would create in `niche` and records the ghost, unless nothing moved.
    ///
    /// The cell comes from the niche atom, so this call is the fill's reading of one measurement
    /// rather than a second probe (ADR 0118). Returns whether the ghost or the refusal changed, so
    /// the caller can ask for a repaint.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn hover(
        &mut self,
        project: &Project,
        product: Id,
        point: DVec3,
        niche: &Niche,
        ask: FillAsk,
        sight: Sight<'_>,
    ) -> bool {
        let FillAsk { fill, mode, mount, thickness, free } = ask;
        if self.measured.as_ref().is_some_and(|key| {
            key.niche == *niche
                && key.point == point
                && key.fill == fill
                && key.mode == mode
                && key.mount == mount
                && key.thickness == thickness
                && key.free == free
        }) {
            return false;
        }
        self.measured = Some(PlanKey { niche: niche.clone(), point, fill, mode, mount, thickness, free });
        match plan_at(project, product, point, niche, ask, sight) {
            Ok(ghost) => {
                self.ghost = Some(ghost);
                self.refusal = None;
            }
            Err(reason) => {
                self.ghost = None;
                self.refusal = Some(reason);
            }
        }
        true
    }

    /// Records that the pointer names no void at all, and says why.
    ///
    /// The atom's own refusal is the sentence: the pointer left the open product, or the space it
    /// points at is not one cell. A `None` sentence drops the ghost silently, which is what a frame
    /// with no resolved point wants.
    pub(crate) fn refuse(&mut self, reason: Option<String>) -> bool {
        let changed = self.ghost.is_some() || self.refusal.is_some() || self.row.is_some();
        self.ghost = None;
        self.refusal = reason;
        self.measured = None;
        self.row = None;
        changed
    }

    /// Paints the ghost, the row's boards, or the sentence that says why there is none.
    ///
    /// The outline is a wireframe, not a solid: it must read through the panels around it and never
    /// hide the void it measures. A rectangle draws its twelve edges; a shaped shelf draws its two
    /// rims and one edge per corner, so a trapezoid reads as a trapezoid before the click. A row
    /// repeats the anchored board's wireframe once per board and carries the sizes on the first one
    /// alone (U125).
    pub(crate) fn paint(&self, ui: &Ui, rect: Rect, camera: Camera) {
        let painter = ui.painter().with_clip_rect(rect);
        if let Some(row) = self.row.as_ref() {
            match &row.plan {
                Ok(plan) => {
                    for index in 0..plan.count.min(engine::MAX_FILL_ROW) {
                        let offset = row.direction * (plan.pitch * index as f64);
                        row.anchor.paint_at(ui, &painter, rect, camera, offset, index == 0);
                    }
                }
                Err(refusal) => label(ui, &painter, rect.center(), refusal.sentence()),
            }
            return;
        }
        let Some(ghost) = self.ghost.as_ref() else {
            // An armed tool with no ghost says why: the pointer is outside the open piece, the void
            // is too small for the material, or its section is not one shape.
            if let Some(reason) = self.refusal.as_deref() {
                label(ui, &painter, rect.center(), reason);
            }
            return;
        };
        ghost.paint_at(ui, &painter, rect, camera, DVec3::ZERO, true);
    }
}

impl FillGhost {
    /// Paints this board's wireframe and its three sizes, in the theme's accent, moved by `offset`.
    ///
    /// A row repeats one board, so the ghosts travel as one paint call per board and only the first
    /// carries the three sizes: a callout on every board of a stack would be a wall of numbers.
    fn paint_at(&self, ui: &Ui, painter: &egui::Painter, rect: Rect, camera: Camera, offset: DVec3, sizes: bool) {
        let ink = theme::accent(ui);
        let stroke = Stroke::new(1.6, ink);
        let count = self.world.len();
        if count < 3 || self.top.len() != count {
            return;
        }
        for index in 0..count {
            let next = (index + 1) % count;
            let (Some(base), Some(far), Some(corner), Some(raised)) =
                (self.world.get(index), self.world.get(next), self.top.get(index), self.top.get(next))
            else {
                continue;
            };
            for (from, to) in [(*base, *far), (*corner, *raised), (*base, *corner)] {
                if let (Some(start), Some(end)) =
                    (screen_point(camera, rect, from + offset), screen_point(camera, rect, to + offset))
                {
                    painter.line_segment([start, end], stroke);
                }
            }
        }
        if !sizes {
            return;
        }
        for (axis, millimetres) in [(0usize, self.size_mm.x), (1, self.size_mm.y)] {
            let Some(edge) = widest_edge(&self.world, self.axes[axis]) else {
                continue;
            };
            let Some(next) = self.world.get((edge + 1) % count) else {
                continue;
            };
            let Some(start) = self.world.get(edge) else {
                continue;
            };
            let Some(midpoint) = screen_point(camera, rect, (*start + *next) * 0.5 + offset) else {
                continue;
            };
            label(ui, painter, midpoint, &format!("{millimetres:.0}"));
        }
        if let (Some(base), Some(raised)) = (self.world.first(), self.top.first())
            && let Some(midpoint) = screen_point(camera, rect, (*base + *raised) * 0.5 + offset)
        {
            label(ui, painter, midpoint, &format!("{:.0}", self.size_mm.z));
        }
    }
}

/// The rim edge that runs furthest along `axis`, for a size callout.
fn widest_edge(points: &[DVec3], axis: DVec3) -> Option<usize> {
    let mut best: Option<(f64, usize)> = None;
    for index in 0..points.len() {
        let (Some(start), Some(end)) = (points.get(index), points.get((index + 1) % points.len())) else {
            continue;
        };
        let span = (*end - *start).dot(axis).abs();
        if best.is_none_or(|(widest, _)| span > widest) {
            best = Some((span, index));
        }
    }
    best.map(|(_, index)| index)
}

/// Plans the void `niche` at `point` and answers the part a click would create, fitted.
///
/// The test-only entry point: the shell always knows the mode its cell runs. The cell comes from the
/// niche atom in the shipped path; a test measures the same cell directly to keep the assertion about
/// the plan and not about the pointer.
#[cfg(test)]
pub(crate) fn plan(
    project: &Project,
    product: Id,
    point: DVec3,
    fill: Fill,
    thickness: Length,
    sight: Sight<'_>,
) -> Result<FillGhost, String> {
    let niche = project.niche(point, sight, None).map_err(|refusal| match refusal {
        model::NicheRefusal::Outside => refusal_sentence(FillRefusal::Outside),
        model::NicheRefusal::Unrepresentable => refusal_sentence(FillRefusal::Unrepresentable),
        model::NicheRefusal::Footprint => refusal_sentence(FillRefusal::Footprint),
    })?;
    let ask = FillAsk { fill, mode: FillMode::Fit, mount: FillMount::Inset, thickness, free: None };
    plan_at(project, product, point, &niche, ask, sight)
}

/// Plans the part a click would create in `niche` and answers the wireframe the maker sees.
///
/// The measurement is the atom's ([`ui::Void`]); this function only turns the answer into
/// the plan's base polygon on both faces, in the world, and the two faces the distance fields
/// measure. `free` pins the free-axis position — a typed distance fixes it, the pointer drives it
/// otherwise (ADR 0103).
pub(crate) fn plan_at(
    project: &Project,
    product: Id,
    point: DVec3,
    niche: &Niche,
    ask: FillAsk,
    sight: Sight<'_>,
) -> Result<FillGhost, String> {
    let FillAsk { fill, mode, mount, thickness, free } = ask;
    let request = model::FillRequest::new(point, fill, mode, thickness, free).with_mount(mount);
    // The plan stops the part at a hidden body's faces, so the ghost is the part the click creates.
    let plan = project.fill_plan_in(niche, product, request, sight).map_err(refusal_sentence)?;
    let parent_world = project.world_placement(product).ok_or_else(|| "The open product has no frame.".to_owned())?;
    let world_placement = Placement::compose(parent_world, plan.placement)
        .map_err(|_| "The part cannot be placed in the open product.".to_owned())?;
    let gap = thickness.millimetres_f64();
    let base = |x: f64, y: f64| world_placement.transform_point(DVec3::new(x, y, 0.0));
    let world = plan.corners.iter().map(|corner| base(corner.x, corner.y)).collect::<Vec<_>>();
    if world.len() < 3 || !gap.is_finite() || gap <= 0.0 {
        return Err("The outline of the void is not a ring.".to_owned());
    }
    // The far face is one material thickness along the panel's own Z axis: a shelf's thickness is
    // vertical, a post's runs across the void. Its corners are the plan's far ring, not the base rim
    // lifted, because a beveled edge sets the far face back into the wall's plane (ADR 0108).
    let top = plan
        .far_corners
        .iter()
        .map(|corner| world_placement.transform_point(DVec3::new(corner.x, corner.y, gap)))
        .collect::<Vec<_>>();
    if world.iter().chain(top.iter()).any(|corner| !corner.is_finite()) {
        return Err("The part cannot be placed in the open product.".to_owned());
    }
    let axes = [world_placement.transform_vector(DVec3::X), world_placement.transform_vector(DVec3::Y)];
    // The free axis is the panel's own thickness axis, so the marker it measures is the board's base
    // face or its far face, whichever the stance puts lower; the placement alone does not say which.
    let axis = model::free_axis(fill);
    let base_along = plan.placement.translation()[axis];
    let far_along = base_along + plan.placement.transform_vector(DVec3::Z)[axis] * gap;
    let (base_centre, far_centre) = (midpoint(&world), midpoint(&top));
    let faces = if base_along <= far_along { [base_centre, far_centre] } else { [far_centre, base_centre] };
    // The callouts measure the board, not its frame: a front's material is what the eye sees and
    // what the click cuts, and its frame is the opening the person controls the position by
    // (ADR 0240). For an ordinary fill the two are the same polygon.
    let size_mm = DVec3::new(reach(&world, axes[0]), reach(&world, axes[1]), gap);
    Ok(FillGhost {
        parent: plan.product,
        outline: plan.outline,
        nominal_size: plan.nominal_size,
        placement: plan.placement,
        niche: plan.niche,
        bevels: plan.bevels,
        trims: plan.trims,
        size_mm,
        free_low: plan.free_low,
        free_high: plan.free_high,
        world,
        top,
        faces,
        axes,
    })
}

/// How far a ring reaches along one direction, in millimetres.
fn reach(points: &[DVec3], direction: DVec3) -> f64 {
    let (low, high) = points.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(low, high), point| {
        let along = point.dot(direction);
        (low.min(along), high.max(along))
    });
    if high.is_finite() && low.is_finite() { high - low } else { 0.0 }
}

/// The reason a void cannot be filled, in the words the model states for every caller.
///
/// One wording serves the ghost, `panel.fill` and the query beside them, so a maker and a caller
/// read the same sentence about one measured void (N37).
fn refusal_sentence(refusal: FillRefusal) -> String {
    refusal.sentence().to_owned()
}

/// Paints one size at `position`, on a quiet plate so it stays legible over the scene.
fn label(ui: &Ui, painter: &egui::Painter, position: Pos2, text: &str) {
    let galley = painter.layout_no_wrap(text.to_owned(), FontId::monospace(11.0), theme::text(ui));
    let rect = Rect::from_center_size(position, galley.size() + egui::vec2(6.0, 2.0));
    painter.rect_filled(rect, 3.0, theme::surface(ui));
    painter.galley(rect.center() - galley.size() * 0.5, galley, theme::text(ui));
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use std::f64::consts::FRAC_PI_2;

    use geom::DQuat;
    use model::{ClassCode, Container, Envelope, Id, Placement, Project};
    use units::Length;

    use super::*;

    fn millimetres(value: i64) -> Length {
        Length::from_millimetres(value).expect("whole millimetres fit")
    }

    fn clean_project() -> Project {
        let root = Container::new(Id::new(1).expect("id"), "Root", Placement::IDENTITY, Vec::new());
        Project::new(root).expect("an empty project")
    }

    /// An empty product of `600 x 560 x 720` millimetres at the origin.
    fn piece() -> (Project, Id) {
        let mut project = clean_project();
        let root = project.root().id();
        let envelope = Envelope::new(millimetres(600), millimetres(560), millimetres(720)).expect("a box");
        let id = project
            .add_product(root, "Cabinet", ClassCode::GENERAL_PRODUCT, envelope, Placement::IDENTITY)
            .expect("a product");
        (project, id)
    }

    /// The cell the pointer at `point` names, as the atom would answer it.
    fn niche_at(project: &Project, point: DVec3) -> Niche {
        project.niche(point, Sight::none(), None).expect("the point names a void")
    }

    /// Whether every corner of the ghost lies inside the void it was measured against.
    fn fits(ghost: &FillGhost) -> bool {
        const TOLERANCE: f64 = 2.0e-3;
        ghost
            .world
            .iter()
            .chain(ghost.top.iter())
            .all(|corner| ghost.niche.planes.iter().all(|plane| plane.normal.dot(*corner) - plane.offset >= -TOLERANCE))
    }

    /// A board lying over the void and coming down towards the back: its underside is the ceiling.
    fn leaned_ceiling(project: &mut Project, product: Id) {
        let lean = 10.0_f64.to_radians();
        let (sin, cos) = lean.sin_cos();
        let along = DVec3::new(0.0, cos, -sin);
        let away = DVec3::new(0.0, sin, cos);
        let rotation = DQuat::from_rotation_axes(along.cross(away), along, away);
        let placement = Placement::new(DVec3::new(0.0, 0.0, 699.5), rotation).expect("a placement");
        let nominal = PanelNominalSize::new(millimetres(600), millimetres(620)).expect("a ceiling size");
        project.add_panel(product, "Ceiling", nominal, None, millimetres(18), placement).expect("add a ceiling");
    }

    /// A board standing as the case's back wall, its inner face through `y = through` at `z = 0`
    /// and leaned by `degrees` about `+X`: a positive lean brings the face forward with height.
    fn leaned_wall(project: &mut Project, product: Id, through: f64, degrees: f64) {
        let wall = DQuat::from_rotation_z(-FRAC_PI_2) * DQuat::from_rotation_y(-FRAC_PI_2);
        let rotation = DQuat::from_rotation_x(degrees.to_radians()) * wall;
        let placement = Placement::new(DVec3::new(0.0, through, 0.0), rotation).expect("a wall placement");
        let nominal = PanelNominalSize::new(millimetres(1600), millimetres(600)).expect("a wall size");
        project.add_panel(product, "Wall", nominal, None, millimetres(18), placement).expect("add a wall");
    }

    fn shelf_slot(project: &mut Project, product: Id, at: (i64, i64, i64)) {
        let nominal = PanelNominalSize::new(millimetres(600), millimetres(560)).expect("a shelf size");
        let placement = Placement::IDENTITY
            .with_translation(millimetres(at.0), millimetres(at.1), millimetres(at.2))
            .expect("a placement");
        project.add_panel(product, "Shelf", nominal, None, millimetres(18), placement).expect("add a shelf");
    }

    /// Two standing posts whose inner faces run from `(100, 0)` to `(200, 560)` and from
    /// `(500, 0)` to `(400, 560)`, both 720 mm tall and 1200 mm wide, in the post's stance.
    fn splayed_piece() -> (Project, Id) {
        let (mut project, product) = piece();
        let slant = 100.0 / 560.0_f64.hypot(100.0);
        let drop = 560.0 / 560.0_f64.hypot(100.0);
        for (through, normal) in [((100.0, 0.0), (drop, -slant)), ((500.0, 0.0), (-drop, -slant))] {
            let direction = (-normal.1, normal.0);
            let start_y = 280.0 - 600.0 * direction.1;
            let start_x = through.0 + (start_y - through.1) * (direction.0 / direction.1);
            // The stance: the length runs up from the floor, the thickness runs away from the free
            // space, and the node sits on the face the free space touches (ADR 0105).
            let rotation = DQuat::from_rotation_z(normal.1.atan2(normal.0)) * DQuat::from_rotation_y(-FRAC_PI_2);
            let translation = DVec3::new(start_x, start_y, 0.0);
            let placement = Placement::new(translation, rotation).expect("a placement");
            let nominal = PanelNominalSize::new(millimetres(720), millimetres(1200)).expect("a post size");
            project.add_panel(product, "Post", nominal, None, millimetres(18), placement).expect("add a post");
        }
        (project, product)
    }

    #[test]
    fn a_click_anchors_the_row_and_the_hand_and_a_typed_count_grow_it() {
        // U125: the anchor is the board the click named; the hand names the span from it, a typed
        // count takes the whole void clamped to the boards that fit, and a typed far gap keeps its
        // own distance while the neighbours share the rest.
        let (project, product) = piece();
        let point = DVec3::new(300.0, 280.0, 200.0);
        let niche = niche_at(&project, point);
        let ask = FillAsk {
            fill: Fill::WidthAndDepth,
            mode: FillMode::Fit,
            mount: FillMount::Inset,
            thickness: millimetres(16),
            free: None,
        };
        let mut gizmo = FillGizmo::default();
        gizmo.hover(&project, product, point, &niche, ask, Sight::none());
        assert!(gizmo.begin_row(DVec3::Z, 2), "the click anchors the ghost it saw");
        let anchor = gizmo.row().expect("a row").anchor.clone();
        let room = 720.0 - anchor.free_low;
        assert_eq!(
            gizmo.row().and_then(|row| row.plan.clone().ok()).map(|plan| plan.count),
            Some((room / 16.0) as usize)
        );

        // A hand a fifth of the way up the void holds the boards that fit in that span.
        assert!(gizmo.plan_row(2, Some(200.0), None, None, None));
        let plan = gizmo.row().and_then(|row| row.plan.clone().ok()).expect("a row of shelves");
        assert_eq!(plan.count, (200.0f64 / 16.0) as usize);
        assert!(plan.end <= 720.0 + 1.0e-6, "no board crosses the case's ceiling");

        // A typed count claims the whole void, clamped to what fits without crossing.
        gizmo.plan_row(2, Some(200.0), None, None, Some(10_000));
        let typed = gizmo.row().and_then(|row| row.plan.clone().ok()).expect("a row of shelves");
        assert_eq!(typed.count, (room / 16.0) as usize);
        assert_eq!(gizmo.row().and_then(|row| row.count), Some(10_000), "the field keeps what was typed");

        // A typed far gap is honoured, and the neighbours divide what is left.
        gizmo.plan_row(2, None, None, Some(40.0), Some(4));
        let fixed = gizmo.row().and_then(|row| row.plan.clone().ok()).expect("a row of shelves");
        assert_eq!(fixed.count, 4);
        assert!((fixed.high - 40.0).abs() < 1.0e-6, "the typed gap is the distance the row leaves: {}", fixed.high);
        let shared = (room - 4.0 * 16.0 - 40.0) / 3.0;
        assert!((fixed.gap - shared).abs() < 1.0e-6, "the neighbours share the rest equally: {}", fixed.gap);

        // Escape drops the row and leaves the tool's ghost where it was.
        assert!(gizmo.cancel_row());
        assert!(gizmo.row().is_none() && gizmo.ghost().is_some(), "the tool keeps its ghost");
    }

    #[test]
    fn a_shelf_takes_the_void_width_and_depth_and_the_pointer_height() {
        let (project, product) = piece();
        let ghost = plan(
            &project,
            product,
            DVec3::new(300.0, 280.0, 351.4),
            Fill::WidthAndDepth,
            millimetres(18),
            Sight::none(),
        )
        .expect("a shelf fills the void");
        assert_eq!(ghost.nominal_size.length(), millimetres(600));
        assert_eq!(ghost.nominal_size.width(), millimetres(560));
        assert_eq!(ghost.placement.translation(), DVec3::new(0.0, 0.0, 351.0), "the height moves in whole mm");
        assert_eq!(ghost.size_mm, DVec3::new(600.0, 560.0, 18.0));
        assert_eq!(ghost.parent, product);
        assert!(
            !ghost.outline.has_contour_features() && ghost.outline.contour().is_none(),
            "a rectangular void keeps the rectangle panel"
        );
        // The corners are the panel's box at that placement.
        assert_eq!(ghost.world[0], DVec3::new(0.0, 0.0, 351.0));
        assert_eq!(ghost.world[2], DVec3::new(600.0, 560.0, 351.0));
        assert_eq!(ghost.top[2], DVec3::new(600.0, 560.0, 369.0));
        // A shelf grows upward from its base face, so that face is the low end of the free axis.
        assert_eq!((ghost.free_low, ghost.free_high), (351.0, 369.0));
    }

    #[test]
    fn a_post_stands_on_the_voids_floor_and_spans_its_height_and_depth() {
        let (project, product) = piece();
        let ghost = plan(
            &project,
            product,
            DVec3::new(250.6, 280.0, 360.0),
            Fill::HeightAndDepth,
            millimetres(18),
            Sight::none(),
        )
        .expect("a post fills the void");
        assert_eq!(ghost.nominal_size.length(), millimetres(720), "the void height is the post's length");
        assert_eq!(ghost.nominal_size.width(), millimetres(560), "and the void depth its width");
        assert_eq!(ghost.size_mm, DVec3::new(720.0, 560.0, 18.0));
        // The post is the shelf turned to the right: its length runs up from the void's floor, so the
        // node is the base corner, one thickness to the +X side of the face the pointer measured.
        let near = |point: DVec3, wanted: DVec3| (point - wanted).length() < 1e-9;
        assert!(near(ghost.placement.translation(), DVec3::new(269.0, 0.0, 0.0)), "{:?}", ghost.placement);
        assert!(near(ghost.world[0], DVec3::new(269.0, 0.0, 0.0)), "the first corner is the base corner");
        assert!(near(ghost.world[1], DVec3::new(269.0, 0.0, 720.0)), "the second stands directly above it");
        assert!(
            near(ghost.world[1] - ghost.world[0], DVec3::new(0.0, 0.0, 720.0)),
            "so edge 1 is the bottom-to-top edge of the board, not the far face's"
        );
        assert!(
            ghost.world.iter().chain(ghost.top.iter()).any(|corner| near(*corner, DVec3::new(251.0, 0.0, 0.0))),
            "the post's thickness runs -X from its face on the pointer: {:?}",
            ghost.top
        );
        assert!(ghost.low_face().is_some_and(|face| (face.x - 251.0).abs() < 1e-9), "the fields measure the -X face");
        assert!((ghost.free_low - 251.0).abs() < 1e-9, "and the low end is that same face");
        assert!((ghost.free_high - 269.0).abs() < 1e-9, "with the board inside the void");
    }

    #[test]
    fn a_wall_stands_in_the_frontal_plane_and_its_ghost_is_the_plan() {
        // N28: the wall is the third fill pair, so the ghost needs no rule of its own — it draws the
        // plan the click will store, in the stance ADR 0105 fixes for the wall.
        let (project, product) = piece();
        let point = DVec3::new(300.0, 280.0, 360.0);
        let ghost = plan(&project, product, point, Fill::WidthAndHeight, millimetres(18), Sight::none())
            .expect("a wall fills the void");
        assert_eq!(ghost.nominal_size.length(), millimetres(720), "the niche height is the wall's length");
        assert_eq!(ghost.nominal_size.width(), millimetres(600), "and the niche width is its width");
        // The callouts measure the board's own span, so they agree with the polygon to a rounding
        // step and not to the bit.
        assert!((ghost.size_mm - DVec3::new(720.0, 600.0, 18.0)).length() < 1e-6, "{:?}", ghost.size_mm);
        let near = |point: DVec3, wanted: DVec3| (point - wanted).length() < 1e-9;
        assert!(near(ghost.placement.translation(), DVec3::new(0.0, 280.0, 0.0)), "{:?}", ghost.placement);
        // Vertex 1 is the base corner: the panel's length runs up, its width across the front and its
        // thickness towards the maker, so the board's base face is the one at the pointer's depth.
        assert!(near(ghost.world[0], DVec3::new(0.0, 280.0, 0.0)), "the first corner is the base corner");
        assert!(near(ghost.world[1], DVec3::new(0.0, 280.0, 720.0)), "the second stands directly above it");
        assert!(near(ghost.world[3], DVec3::new(600.0, 280.0, 0.0)), "the width runs across the front");
        assert!(near(ghost.top[0], DVec3::new(0.0, 298.0, 0.0)), "the material runs towards +Y");
        assert_eq!((ghost.free_low, ghost.free_high), (280.0, 298.0), "one thickness of depth");
        assert!(ghost.low_face().is_some_and(|face| (face.y - 280.0).abs() < 1e-9), "the fields measure the base face");
        // The ghost is the plan the command receives, to the micrometre.
        let stored = project
            .fill_plan(
                product,
                model::FillRequest::new(point, Fill::WidthAndHeight, FillMode::Fit, millimetres(18), None),
                Sight::none(),
                None,
            )
            .expect("the same plan");
        assert_eq!(ghost.placement, stored.placement, "the ghost stores the plan's placement");
        assert_eq!(ghost.nominal_size, stored.nominal_size, "and its size");
        assert_eq!(ghost.outline, stored.outline, "and its outline");
    }

    #[test]
    fn a_post_keeps_its_board_inside_the_void_at_both_ends_of_its_travel() {
        let (project, product) = piece();
        // The pointer's plane is the face the post grows away from, so the board spans one thickness
        // to its +X side; a sign slip there would leave the board hanging through a wall.
        for pointer in [0.0, 600.0] {
            let ghost = plan(
                &project,
                product,
                DVec3::new(pointer, 280.0, 360.0),
                Fill::HeightAndDepth,
                millimetres(18),
                Sight::none(),
            )
            .expect("a post fills the void");
            let xs: Vec<f64> = ghost.world.iter().chain(ghost.top.iter()).map(|corner| corner.x).collect();
            let low = xs.iter().copied().fold(f64::INFINITY, f64::min);
            let high = xs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            assert!(low >= -1e-9 && high <= 600.0 + 1e-9, "the post left the void at {pointer}: {low}..{high}");
            assert!((high - low - 18.0).abs() < 1e-9, "and it is one board thick: {low}..{high}");
        }
    }

    /// The ask the tests plan with: one pair, one build, the sample board, no pinned place.
    fn ask(fill: Fill, mode: FillMode) -> FillAsk {
        FillAsk { fill, mode, mount: FillMount::Inset, thickness: millimetres(18), free: None }
    }

    #[test]
    fn a_shelf_in_a_splayed_void_takes_the_trapezoid_outline() {
        let (project, product) = splayed_piece();
        let point = DVec3::new(300.0, 280.0, 360.0);
        let ghost = plan_at(
            &project,
            product,
            point,
            &niche_at(&project, point),
            ask(Fill::WidthAndDepth, FillMode::Shape),
            Sight::none(),
        )
        .expect("a shelf fills the splayed void");
        let outline = ghost.outline.contour().expect("the shelf takes the measured section");
        assert_eq!(ghost.nominal_size.length(), millimetres(400));
        assert_eq!(ghost.nominal_size.width(), millimetres(560));
        assert_eq!(ghost.placement.translation(), DVec3::new(100.0, 0.0, 360.0), "the section's own corner");
        let corners = outline.sampled_points(1.0).expect("the ring is line only");
        assert_eq!(
            corners,
            vec![
                [millimetres(0), millimetres(0)],
                [millimetres(400), millimetres(0)],
                [millimetres(300), millimetres(560)],
                [millimetres(100), millimetres(560)],
            ],
            "the ring is anchored at its own zero corner"
        );
        // The ghost's world corners are the same ring moved to the panel's corner.
        assert!((ghost.world[0] - DVec3::new(100.0, 0.0, 360.0)).length() < 1e-9, "{:?}", ghost.world);
        assert!((ghost.world[1] - DVec3::new(500.0, 0.0, 360.0)).length() < 1e-9, "{:?}", ghost.world);
        assert!(fits(&ghost), "the ghost stays inside the void: {:?}", ghost.world);
    }

    #[test]
    fn a_post_stands_in_a_splayed_void_at_the_slabs_depth() {
        // The posts narrow the void towards the back, so a board at x = 150..168 cannot be 560 mm
        // deep: the wall reaches x = 100 + 0.1786 y, and the board stops where the wall is.
        let (project, product) = splayed_piece();
        let ghost = plan(
            &project,
            product,
            DVec3::new(150.0, 200.0, 360.0),
            Fill::HeightAndDepth,
            millimetres(18),
            Sight::none(),
        )
        .expect("a post stands against the splayed wall");
        assert!(ghost.outline.contour().is_none(), "the slab's depth leaves an honest rectangle");
        assert_eq!(ghost.nominal_size.length(), millimetres(720), "the void's height is the post's length");
        let depth = ghost.nominal_size.width().millimetres_f64();
        assert!((depth - 280.0).abs() < 1.0, "the depth the slab really has, not the void's 560: {depth}");
        assert_eq!(ghost.placement.translation(), DVec3::new(168.0, 0.0, 0.0), "one thickness past the pointer");
        assert!(fits(&ghost), "the ghost stays inside the void: {:?}", ghost.world);
    }

    #[test]
    fn a_shelf_under_a_leaned_ceiling_keeps_its_whole_board_in_the_void() {
        let (mut project, product) = piece();
        leaned_ceiling(&mut project, product);
        let ghost = plan(
            &project,
            product,
            DVec3::new(300.0, 540.0, 590.0),
            Fill::WidthAndDepth,
            millimetres(18),
            Sight::none(),
        )
        .expect("a shelf fits under the leaned ceiling");
        assert!(ghost.outline.contour().is_none(), "the ceiling cuts the depth, not the corners");
        assert_eq!(ghost.placement.translation().z, 590.0);
        assert!(
            ghost.nominal_size.width() < millimetres(560),
            "the shelf stops under the ceiling: {:?}",
            ghost.nominal_size
        );
        assert!(fits(&ghost), "the ghost stays inside the void: {:?}", ghost.world);
    }

    #[test]
    fn a_cursor_on_the_envelope_face_still_measures_the_void() {
        let (project, product) = piece();
        let ghost =
            plan(&project, product, DVec3::new(300.0, 0.0, 360.0), Fill::WidthAndDepth, millimetres(18), Sight::none())
                .expect("the near face is inside the void");
        assert_eq!(ghost.nominal_size.length(), millimetres(600));
        assert!(ghost.outline.contour().is_none());
    }

    #[test]
    fn a_shelf_splits_the_void_and_the_ghost_fits_one_half() {
        let (mut project, product) = piece();
        shelf_slot(&mut project, product, (0, 0, 350));
        let ghost = plan(
            &project,
            product,
            DVec3::new(300.0, 280.0, 600.0),
            Fill::WidthAndDepth,
            millimetres(18),
            Sight::none(),
        )
        .expect("the upper niche takes a shelf");
        assert_eq!(ghost.placement.translation(), DVec3::new(0.0, 0.0, 600.0));
        assert_eq!(ghost.niche.size.z, 352.0, "the upper void starts on the shelf's top face");
    }

    #[test]
    fn the_free_axis_is_kept_inside_the_void() {
        let (mut project, product) = piece();
        shelf_slot(&mut project, product, (0, 0, 350));
        // A cursor at the very top of the upper void would put the shelf through the roof; it is
        // clamped so the whole thickness stays inside.
        let ghost = plan(
            &project,
            product,
            DVec3::new(300.0, 280.0, 720.0),
            Fill::WidthAndDepth,
            millimetres(18),
            Sight::none(),
        )
        .expect("a shelf");
        assert_eq!(ghost.placement.translation().z, 720.0 - 18.0);
    }

    #[test]
    fn a_void_thinner_than_the_material_is_refused() {
        // The slot between the void's floor and the shelf is 350 mm; make it a sliver instead.
        let (mut project, product) = piece();
        shelf_slot(&mut project, product, (0, 0, 4));
        let refusal =
            plan(&project, product, DVec3::new(300.0, 280.0, 1.0), Fill::WidthAndDepth, millimetres(18), Sight::none())
                .expect_err("no shelf fits a 4 mm gap");
        assert!(refusal.contains("thinner"), "{refusal}");
    }

    #[test]
    fn a_point_outside_the_open_product_is_refused() {
        let (project, product) = piece();
        let refusal =
            plan(&project, product, DVec3::new(-500.0, 0.0, 0.0), Fill::WidthAndDepth, millimetres(18), Sight::none())
                .expect_err("no void");
        assert!(refusal.contains("empty space"), "{refusal}");
    }

    #[test]
    fn the_ghost_paints_its_wireframe_and_three_sizes_offscreen() {
        let (project, product) = piece();
        let mut gizmo = FillGizmo::default();
        let point = DVec3::new(300.0, 280.0, 360.0);
        assert!(gizmo.hover(
            &project,
            product,
            point,
            &niche_at(&project, point),
            ask(Fill::WidthAndDepth, FillMode::Fit),
            Sight::none()
        ));
        let drawn = paint_ghost(&mut gizmo);
        for size in ["600", "560", "18"] {
            assert!(drawn.0.iter().any(|text| text == size), "the {size} mm callout is missing: {:?}", drawn.0);
        }
        assert!(drawn.1 >= 12, "the ghost draws a box's twelve edges, got {}", drawn.1);
    }

    #[test]
    fn a_beveled_ghost_sets_its_far_rim_back_into_the_wall() {
        let (mut project, product) = piece();
        leaned_wall(&mut project, product, 560.0, 10.0);
        let mut gizmo = FillGizmo::default();
        let point = DVec3::new(300.0, 280.0, 360.0);
        assert!(gizmo.hover(
            &project,
            product,
            point,
            &niche_at(&project, point),
            ask(Fill::WidthAndDepth, FillMode::Shape),
            Sight::none()
        ));
        let ghost = gizmo.ghost().expect("the leaned void takes a ghost");
        assert_eq!(ghost.bevels.len(), 1, "the wall's bevel travels with the ghost: {:?}", ghost.bevels);
        assert!(fits(ghost), "the beveled ghost stays inside the void");
        // The far rim is the base rim with the beveled edge set back, not the base rim lifted.
        let lift = DVec3::new(0.0, 0.0, 18.0);
        assert!(
            ghost.top.iter().zip(&ghost.world).any(|(top, base)| (*top - (*base + lift)).length() > 1.0),
            "the far rim is set back at the wall: {:?}",
            ghost.top
        );
        let back = ghost.world.iter().map(|corner| corner.y).fold(f64::NEG_INFINITY, f64::max);
        let far_back = ghost.top.iter().map(|corner| corner.y).fold(f64::NEG_INFINITY, f64::max);
        let setback = 18.0 * 10.0_f64.to_radians().tan();
        assert!(
            (back - far_back - setback).abs() <= 1.0e-3,
            "the far back edge is one setback forward of the base edge: {} vs {setback}",
            back - far_back
        );
    }

    #[test]
    fn a_shaped_ghost_paints_its_polygon_offscreen() {
        let (project, product) = splayed_piece();
        let mut gizmo = FillGizmo::default();
        let point = DVec3::new(300.0, 280.0, 360.0);
        assert!(gizmo.hover(
            &project,
            product,
            point,
            &niche_at(&project, point),
            ask(Fill::WidthAndDepth, FillMode::Shape),
            Sight::none()
        ));
        let drawn = paint_ghost(&mut gizmo);
        for size in ["400", "560", "18"] {
            assert!(drawn.0.iter().any(|text| text == size), "the {size} mm callout is missing: {:?}", drawn.0);
        }
        assert!(drawn.1 >= 12, "the ghost draws a polygon prism's twelve edges, got {}", drawn.1);
    }

    /// Paints the armed tool offscreen and answers the drawn texts and line count.
    fn paint_ghost(gizmo: &mut FillGizmo) -> (Vec<String>, usize) {
        let ctx = egui::Context::default();
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))),
                ..Default::default()
            },
            |ui| {
                gizmo.paint(
                    ui,
                    egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0)),
                    Camera::default(),
                );
            },
        );
        let drawn = crate::test_support::drawn_text(&output.shapes);
        fn count_lines(shape: &egui::Shape, lines: &mut usize) {
            match shape {
                egui::Shape::LineSegment { .. } => *lines += 1,
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        count_lines(shape, lines);
                    }
                }
                _ => {}
            }
        }
        let mut lines = 0;
        for clipped in &output.shapes {
            count_lines(&clipped.shape, &mut lines);
        }
        output.drop_without_applying_deltas();
        (drawn, lines)
    }
}
