//! The niche tool's pointer: the empty space under the cursor, and the wireframe that names it.
//!
//! The atom ([`ui::Void`]) holds the point and the cell the model answered; this module
//! resolves a viewport position to a point inside the open product through the atom's own cursor
//! rule ([`ui::pointer_point`]), feeds it, and paints what the tool is reading. It is the
//! niche's only input path, and the fill tool resolves its pointer through the same function, so the
//! two tools cannot point at two different places in one frame (ADR 0118).
//!
//! Nothing here writes: there is no document field, no revision and no undo entry, and the whole
//! overlay is `egui` paint. The box is the cell's own box in the product's frame, which is the same
//! number the status bar prints, and it is drawn as a wireframe so the void reads through the boards
//! around it.
//!
//! The fill tool also reads its free axis here ([`free_at`]): the cell names the measured sides, and
//! the face under the pointer decides whether the point can say where the hand is along the one axis
//! that moves (N23).
//!
//! Changes when: the niche gains a second shape to draw, or the pointer stops resolving through the
//! atom's cursor rule.

use eframe::egui::{self, FontId, Pos2, Rect, Stroke, Ui};
use geom::DVec3;
use model::{Id, Niche, Project, Sight};
use scene::Camera;
use ui::{Message, theme};
use units::Length;

use crate::gizmo_math::screen_point;

/// Padding inside the size label, in points.
const LABEL_PADDING: egui::Vec2 = egui::vec2(6.0, 2.0);

/// The point in the open product the pointer names, in world millimetres.
///
/// The one cursor rule both niche tools share: the ray enters the product's own envelope and stops at
/// the first surface inside it (ADR 0109). A pointer outside the product, or a ray that misses the
/// envelope, answers `None`.
pub(crate) fn point_at(
    project: &Project,
    product: Id,
    camera: Camera,
    rect: Rect,
    pointer: Pos2,
    sight: Sight<'_>,
) -> Option<DVec3> {
    let ray = crate::gizmo_math::ray_to(pointer, rect, camera);
    ui::pointer_point(project, product, ray, sight)
}

/// What the pointer says about the cell's free axis, or `None` when it names no point at all.
///
/// The one cursor rule becomes one ray at the caller's end, and [`ui::free_place`] answers both
/// halves of the reading: the place the point names and the hand's own place on the cell's axis,
/// with the face that decides which of them the tool acts on (N23).
pub(crate) fn free_at(
    project: &Project,
    product: Id,
    niche: &Niche,
    axis: usize,
    ray: scene::Ray,
    sight: Sight<'_>,
) -> Option<ui::FreePlace> {
    ui::free_place(project, product, niche, axis, ray, sight)
}

/// The niche tool's live overlay: nothing of its own, because the atom holds every number.
#[derive(Debug, Default)]
pub(crate) struct VoidGizmo;

impl VoidGizmo {
    /// Drops the overlay: the atom owns the value, so there is nothing else to forget.
    pub(crate) fn reset(&mut self) {}

    /// Feeds the pointer to the armed niche and says whether the value changed.
    ///
    /// The atom decides whether the pointer moved enough to measure again, so a still hand costs one
    /// comparison. A pointer on a widget, or a frame with no pointer at all, leaves the value as it
    /// stands rather than dropping the cell under the maker's hand.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn update(
        &mut self,
        response: &egui::Response,
        rect: Rect,
        camera: Camera,
        project: &Project,
        editor: &mut ui::Editor,
        product: Option<Id>,
        pointer_over_widget: bool,
    ) -> bool {
        if pointer_over_widget {
            return false;
        }
        let pointer = response.hover_pos().or_else(|| response.interact_pointer_pos());
        let (Some(pointer), Some(product)) = (pointer, product) else {
            return false;
        };
        let sight = editor.flags().sight();
        let Some(point) = point_at(project, product, camera, rect, pointer, sight) else {
            // The pointer is in the scene but outside the open product: the row says what to point
            // at rather than keeping the cell the hand has already left.
            return editor.void_clear();
        };
        editor.void_measure(project, point)
    }

    /// Paints the cell's twelve edges and its three sides, in the theme's accent.
    pub(crate) fn paint(&self, ui: &Ui, rect: Rect, camera: Camera, project: &Project, niche: &Niche) {
        let painter = ui.painter().with_clip_rect(rect);
        let Some(placement) = project.world_placement(niche.product) else {
            return;
        };
        let Some(corners) = cell_corners(niche, placement) else {
            return;
        };
        let ink = theme::accent(ui);
        let stroke = Stroke::new(1.6, ink);
        for (from, to) in BOX_EDGES {
            let (Some(start), Some(end)) = (corners.get(from), corners.get(to)) else {
                continue;
            };
            if let (Some(start), Some(end)) = (screen_point(camera, rect, *start), screen_point(camera, rect, *end)) {
                painter.line_segment([start, end], stroke);
            }
        }
        let centre = corners.iter().sum::<DVec3>() / corners.len() as f64;
        let Some(centre) = screen_point(camera, rect, centre) else {
            return;
        };
        let (Some(length), Some(width), Some(height)) =
            (length_of(niche.size.x), length_of(niche.size.y), length_of(niche.size.z))
        else {
            return;
        };
        let label = Message::StatusSize.with(ui::language(ui.ctx()), [length, width, height]);
        paint_label(ui, &painter, rect, centre, &label);
    }
}

/// The eight corners of the cell's box, in world millimetres, corner first.
///
/// The box is the cell's own `local_min` and `size` in the product's frame — the same pair the fill
/// plan starts from — so the overlay and the part cannot disagree about the space they name.
fn cell_corners(niche: &Niche, placement: model::Placement) -> Option<[DVec3; 8]> {
    let low = niche.local_min;
    let high = niche.local_max();
    if !low.is_finite() || !high.is_finite() {
        return None;
    }
    let mut corners = [DVec3::ZERO; 8];
    for (index, corner) in corners.iter_mut().enumerate() {
        let x = if index & 1 == 0 { low.x } else { high.x };
        let y = if index & 2 == 0 { low.y } else { high.y };
        let z = if index & 4 == 0 { low.z } else { high.z };
        let world = placement.transform_point(DVec3::new(x, y, z));
        if !world.is_finite() {
            return None;
        }
        *corner = world;
    }
    Some(corners)
}

/// The twelve edges of a box whose corners are numbered by bit: `X` is bit 0, `Y` bit 1, `Z` bit 2.
const BOX_EDGES: [(usize, usize); 12] =
    [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)];

/// A cell side in millimetres, as the document's own unit, or `None` when it is not one.
///
/// The cell's corners are whole micrometres, so this is an exact conversion rather than a rounded
/// reading; a value outside the unit's range is simply not printed.
fn length_of(value: f64) -> Option<Length> {
    if !value.is_finite() {
        return None;
    }
    let micrometres = (value * 1_000.0).round();
    if micrometres < i64::MIN as f64 || micrometres > i64::MAX as f64 {
        return None;
    }
    Some(Length::from_micrometres(micrometres as i64))
}

/// The three sides at the middle of the cell, on a quiet plate so the number stays legible.
fn paint_label(ui: &Ui, painter: &egui::Painter, rect: Rect, anchor: Pos2, text: &str) {
    let galley = painter.layout_no_wrap(text.to_owned(), FontId::monospace(11.0), theme::text(ui));
    let size = galley.size() + LABEL_PADDING * 2.0;
    let mut at = anchor - size * 0.5;
    at.x = at.x.clamp(rect.left(), (rect.right() - size.x).max(rect.left()));
    at.y = at.y.clamp(rect.top(), (rect.bottom() - size.y).max(rect.top()));
    let plate = Rect::from_min_size(at, size);
    painter.rect_filled(plate, 3.0, theme::surface(ui));
    painter.galley(at + LABEL_PADDING, galley, theme::text(ui));
}
