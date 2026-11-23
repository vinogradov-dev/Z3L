//! The pull arrow a fresh extrusion shows.
//!
//! The ribbon creates a body at the height its field names; this gizmo then lets the user adjust
//! that height in place. Nothing is written while the pointer moves: the drag changes the height
//! this file remembers, the shell shows that height as a session preview of the body, and the
//! command bar's ✓ runs one `object.extrude.height.set`, so a whole adjustment is one undo entry
//! (M71).
//!
//! The same arrow also pulls the profile a body cell has just drawn (ADR 0138): the node is a path
//! for the length of the gesture, the ✓ extrudes it into the body with one `object.path.extrude`, and
//! the ✕ takes the profile back through the edit scope it was created in. So the box and the cylinder
//! are drawn by the sketch's own grammar and built on Z, without a second gesture of their own.
//!
//! And it pulls a cone, which is the one pull that sets two numbers (ADR 0141): the base circle is
//! drawn, the node enters the document at the ribbon's height inside an edit scope, and the card
//! carries the height `H` beside the top radius `R`, so a taper is chosen in the same breath and
//! written by the same ✓ — one `object.primitive.replace`, one undo entry.
//!
//! The preview is the body itself, evaluated at the height the arrow shows, so the user sees the
//! solid they are about to accept rather than a stand-in for it. The arrow sits on that top face,
//! at the contour's bounds centre, and points along the body's local `+Z`. A drag is read as a
//! position on that axis rather than as a screen distance, so the same gesture near the camera and
//! far from it produces the same number of millimetres.

use eframe::egui::{self, Pos2, Response, Stroke, Ui, Vec2};
use engine::EditScope;
use geom::{DVec2, DVec3};
use model::{Geometry, Id, Placement, PrimitiveShape, Project};
use scene::Camera;
use units::Length;

use crate::gizmo_math::{ACTIVE_COLOR, HIT_RADIUS, distance_to_segment, ray_to, screen_direction, screen_point};

/// Length of the pull arrow, in points.
const ARROW_LENGTH: f32 = 96.0;
/// Half-width of the arrow head, in points.
const ARROW_HEAD: f32 = 5.0;
/// How far the pointer must move before a press becomes a drag, in points.
const CLICK_SLOP: f32 = 3.0;
/// The smallest height a pull may leave, in millimetres.
pub(crate) const MIN_HEIGHT_MM: f64 = 1.0;

/// How many frames a fresh pull's field asks for the keyboard.
///
/// egui builds the widget after it is asked for the focus, so the request of the frame a new card
/// first appears is lost; asking on the next frame too gives the field the keyboard, and asking any
/// longer would hold it against the user's next click. A sketch plate counts the same way.
const FOCUS_REQUESTS: u8 = 2;

/// Colour of the arrow when the pointer is elsewhere.
const ARROW_COLOR: egui::Color32 = egui::Color32::from_rgb(120, 200, 255);

/// The node a pull is adjusting, with the height the document holds and the previewed one.
#[derive(Clone, Copy, Debug)]
struct Pull {
    /// The body, or the profile a body cell drew and the ✓ will extrude.
    object: Id,
    /// The height the document holds; `None` while the node is still a flat profile.
    committed: Option<Length>,
    /// The edit scope the profile entered the document in, so the ✓ joins that entry and the ✕
    /// takes the profile back.
    scope: Option<EditScope>,
    /// The height the arrow currently shows.
    height: Length,
    /// The radius of the top circle a cone pull sets; `None` for a body or a profile, whose top is
    /// its own contour (ADR 0141).
    top: Option<Length>,
}

/// One continuous drag on the arrow.
#[derive(Clone, Copy, Debug)]
struct Drag {
    /// Where the press landed, to tell a click from a drag.
    start_pointer: Pos2,
    /// The height at the press.
    start_height: Length,
    /// The axis position under the pointer at the press, in millimetres.
    start_parameter: f64,
    /// Whether the pointer moved far enough to change the height.
    moved: bool,
}

/// What the pull asks the shell to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExtrudeEvent {
    /// The arrow moved: show this height without writing it.
    Preview {
        /// The body being pulled.
        object: Id,
        /// The height the preview shows.
        height: Length,
        /// The top radius a cone pull shows; `None` for a body or a profile.
        top: Option<Length>,
    },
    /// The pull is done: write these numbers once, then drop the preview.
    Finish {
        /// The body being pulled.
        object: Id,
        /// The height the pull shows.
        height: Length,
        /// The top radius a cone pull shows; `None` for a body or a profile.
        top: Option<Length>,
    },
}

/// Where the body sits: its frame, the contour centre and the axis the arrow follows.
#[derive(Clone, Debug, PartialEq)]
struct Body {
    /// The node's world placement.
    placement: Placement,
    /// The contour's bounds centre in the node's local XY plane, in millimetres.
    centre: DVec2,
    /// The point on the contour plane that the axis runs through, in world millimetres.
    axis_origin: DVec3,
    /// The world direction of the body's local `+Z`.
    axis: DVec3,
}

impl Body {
    /// Reads the frame and contour centre of an extruded body or of a drawn profile.
    ///
    /// A constructive body stores its ring and its height; a path is the profile a body cell just
    /// drew, which has the same ring and no height yet (ADR 0138). Any other payload has no profile
    /// to pull.
    fn read(project: &Project, object: Id) -> Option<Self> {
        let node = project.object(object)?;
        // A cone's axis runs through the centre of the base circle it was drawn on, which is the
        // node's own origin; a body's or a profile's runs through the centre of its ring (ADR 0141).
        let centre = match node.geometry() {
            Geometry::Constructive(body) => {
                body.height()?;
                bounds_centre(&body.contour().ok()?)?
            }
            Geometry::Path(path) => bounds_centre(&path.to_contour().ok()?)?,
            Geometry::Primitive(body) => match body.base() {
                PrimitiveShape::Cone(_) => DVec2::ZERO,
                PrimitiveShape::Sphere(_) => return None,
            },
            // A contour set is several rings, so there is no one profile to pull yet: a set is
            // extruded area by area, which is a later change (G4).
            Geometry::Mesh(_) | Geometry::Contours(_) => return None,
        };
        let placement = project.world_placement(object)?;
        let axis_origin = placement.transform_point(DVec3::new(centre.x, centre.y, 0.0));
        let axis = placement.transform_vector(DVec3::Z).try_normalize()?;
        Some(Self { placement, centre, axis_origin, axis })
    }

    /// The centre of the top face at `height`.
    fn top(&self, height: Length) -> DVec3 {
        self.placement.transform_point(DVec3::new(self.centre.x, self.centre.y, height.millimetres_f64()))
    }
}

/// The centre of a contour's bounds, in millimetres.
fn bounds_centre(contour: &model::Contour) -> Option<DVec2> {
    let ([min_x, min_y], [max_x, max_y]) = contour.bounds_micrometres();
    let centre = DVec2::new((min_x as f64 + max_x as f64) / 2_000.0, (min_y as f64 + max_y as f64) / 2_000.0);
    centre.is_finite().then_some(centre)
}

/// Pointer state of the pull.
#[derive(Debug, Default)]
pub(crate) struct ExtrudeGizmo {
    pull: Option<Pull>,
    drag: Option<Drag>,
    hover: bool,
    /// How many frames the card's field still asks for the keyboard, so a fresh pull can be typed
    /// into without a click; zero once it has asked.
    focus: u8,
}

impl ExtrudeGizmo {
    /// Starts a pull on a body that was just extruded at `height`.
    pub(crate) fn start(&mut self, object: Id, height: Length) {
        self.start_pull(Pull { object, committed: Some(height), scope: None, height, top: None });
    }

    /// Starts a pull on a profile a body cell has just drawn (ADR 0138).
    ///
    /// The path is already in the document, inside `scope`; the ✓ extrudes it into the body and the
    /// ✕ restores the document the gesture began with, so a cancelled create leaves no profile
    /// behind. `height` is the typed default the ribbon would hand an extrusion.
    pub(crate) fn start_profile(&mut self, object: Id, scope: EditScope, height: Length) {
        self.start_pull(Pull { object, committed: None, scope: Some(scope), height, top: None });
    }

    /// Starts a pull on a body a click has just created, inside `scope` (G4).
    ///
    /// The area pull stores its body on the click, in the scope its ✓ closes, so the arrow adjusts the
    /// height the body already has and a ✕ rolls the whole pull back. The precedence is the cone's:
    /// the node exists, the number is typed or dragged, and one command closes the gesture.
    pub(crate) fn start_created(&mut self, object: Id, scope: EditScope, height: Length) {
        self.start_pull(Pull { object, committed: Some(height), scope: Some(scope), height, top: None });
    }

    /// Starts a pull on the cone a cell has just drawn and created (ADR 0141).
    ///
    /// The node is in the document already, at the height the ribbon names, inside `scope`; the ✓
    /// replaces its numbers and closes that scope, and the ✕ takes the node back with it. The top
    /// circle starts sharp, so the first thing the arrow drags is a plain cone.
    pub(crate) fn start_cone(&mut self, object: Id, scope: EditScope, height: Length) {
        self.start_pull(Pull { object, committed: Some(height), scope: Some(scope), height, top: Some(Length::ZERO) });
    }

    fn start_pull(&mut self, pull: Pull) {
        self.pull = Some(pull);
        self.drag = None;
        self.hover = false;
        self.focus = FOCUS_REQUESTS;
    }

    /// Drops the pull. The caller drops the preview overlay in the same breath.
    pub(crate) fn cancel(&mut self) {
        *self = Self::default();
    }

    /// Whether a body is waiting for its height.
    pub(crate) fn is_active(&self) -> bool {
        self.pull.is_some()
    }

    /// Whether the arrow is being dragged.
    pub(crate) fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// The body being pulled.
    pub(crate) fn object(&self) -> Option<Id> {
        self.pull.as_ref().map(|pull| pull.object)
    }

    /// The height the document holds for the body.
    ///
    /// `None` while the pulled node is still a flat profile: it has no height until the ✓
    /// extrudes it.
    pub(crate) fn committed(&self) -> Option<Length> {
        self.pull.as_ref().and_then(|pull| pull.committed)
    }

    /// The edit scope a drawn profile entered the document in.
    pub(crate) fn scope(&self) -> Option<EditScope> {
        self.pull.as_ref().and_then(|pull| pull.scope)
    }

    /// Whether the pull is growing a body out of a profile rather than adjusting one.
    pub(crate) fn is_growing(&self) -> bool {
        self.pull.as_ref().is_some_and(|pull| pull.committed.is_none())
    }

    /// The height the arrow currently shows.
    pub(crate) fn height(&self) -> Option<Length> {
        self.pull.as_ref().map(|pull| pull.height)
    }

    /// The top radius the card shows, or `None` when the pulled shape has no second number.
    pub(crate) fn top(&self) -> Option<Length> {
        self.pull.as_ref().and_then(|pull| pull.top)
    }

    /// Takes a top radius typed into the card as the one the arrow shows.
    pub(crate) fn set_top(&mut self, top: Option<Length>) {
        if let (Some(pull), Some(top)) = (self.pull.as_mut(), top)
            && pull.top.is_some()
        {
            pull.top = Some(top);
        }
    }

    /// Takes a height typed into the card's field as the one the arrow shows.
    ///
    /// The card and the arrow are one number: a field that wrote its own copy would leave the
    /// arrow at the dragged height and the field showing a value the next frame overwrites.
    pub(crate) fn set_height(&mut self, height: Length) {
        if let Some(pull) = self.pull.as_mut() {
            pull.height = height;
        }
    }

    /// Whether the command bar's field must take the focus this frame.
    ///
    /// The count falls by the read, so the field asks on the two frames its card settles and then
    /// leaves the keyboard alone instead of holding it against the user's next click.
    pub(crate) fn take_focus(&mut self) -> bool {
        let asking = self.focus > 0;
        self.focus = self.focus.saturating_sub(1);
        asking
    }

    /// Reads the pointer: a press on the arrow starts a drag, a move previews a height, a release
    /// keeps the preview open, and a click anywhere else finishes the pull.
    pub(crate) fn update(
        &mut self,
        ui: &Ui,
        response: &Response,
        viewport: egui::Rect,
        camera: Camera,
        project: &Project,
    ) -> Result<Option<ExtrudeEvent>, String> {
        let Some(pull) = self.pull.as_ref().copied() else {
            return Ok(None);
        };
        let Some(body) = Body::read(project, pull.object) else {
            // The body was deleted, undone or collapsed under the pull: there is nothing left to
            // adjust, so the arrow goes away instead of showing a stale number.
            self.cancel();
            return Ok(None);
        };
        let pointer = ui.input(|input| input.pointer.latest_pos());

        if let Some(mut drag) = self.drag.take() {
            let mut moved = false;
            if let Some(pointer) = pointer {
                if pointer.distance(drag.start_pointer) > CLICK_SLOP {
                    drag.moved = true;
                }
                if drag.moved
                    && let Some(parameter) =
                        ray_to(pointer, viewport, camera).closest_parameter_on_line(body.axis_origin, body.axis)
                    && let Some(height) = dragged_height(drag.start_height, drag.start_parameter, parameter)
                    && let Some(pull) = self.pull.as_mut()
                    && height != pull.height
                {
                    pull.height = height;
                    moved = true;
                }
            }
            if ui.input(|input| input.pointer.primary_down()) {
                self.drag = Some(drag);
                return Ok(self
                    .pull
                    .as_ref()
                    .map(|pull| ExtrudeEvent::Preview { object: pull.object, height: pull.height, top: pull.top })
                    .filter(|_| moved));
            }
            // The release keeps the pull open on the height it reached: the ✓ writes it, the ✕
            // drops it, and a drag never touches the document.
            return Ok(self
                .pull
                .as_ref()
                .map(|pull| ExtrudeEvent::Preview { object: pull.object, height: pull.height, top: pull.top })
                .filter(|_| moved));
        }

        self.hover = pointer.is_some_and(|pointer| arrow_hit(camera, viewport, &body, pull.height, pointer));
        let pressed = ui.input(|input| input.pointer.primary_pressed());
        if self.hover
            && pressed
            && let Some(pointer) = pointer
            && let Some(parameter) =
                ray_to(pointer, viewport, camera).closest_parameter_on_line(body.axis_origin, body.axis)
        {
            self.drag = Some(Drag {
                start_pointer: pointer,
                start_height: pull.height,
                start_parameter: parameter,
                moved: false,
            });
            return Ok(None);
        }

        if response.clicked_by(egui::PointerButton::Primary) {
            return Ok(Some(ExtrudeEvent::Finish { object: pull.object, height: pull.height, top: pull.top }));
        }
        Ok(None)
    }

    /// Paints the pull arrow on the body's top face.
    pub(crate) fn paint(&mut self, ui: &Ui, viewport: egui::Rect, camera: Camera, project: &Project) {
        let Some(pull) = self.pull.as_ref().copied() else {
            return;
        };
        let Some(body) = Body::read(project, pull.object) else {
            return;
        };
        let Some((base, tip)) = arrow_screen(camera, viewport, &body, pull.height) else {
            return;
        };
        let color = if self.hover || self.drag.is_some() { ACTIVE_COLOR } else { ARROW_COLOR };
        let painter = ui.painter().with_clip_rect(viewport);
        paint_arrow(&painter, base, tip, color);
    }
}

/// The screen ends of the pull arrow on the top face at `height`.
fn arrow_screen(camera: Camera, viewport: egui::Rect, body: &Body, height: Length) -> Option<(Pos2, Pos2)> {
    let base_world = body.top(height);
    let base = screen_point(camera, viewport, base_world)?;
    let step = (camera.distance() * 0.01).max(1.0);
    let ahead = screen_point(camera, viewport, base_world + body.axis * step)?;
    let direction = screen_direction(ahead - base)?;
    Some((base, base + direction * ARROW_LENGTH))
}

/// Whether the pointer grabs the arrow.
fn arrow_hit(camera: Camera, viewport: egui::Rect, body: &Body, height: Length, pointer: Pos2) -> bool {
    arrow_screen(camera, viewport, body, height)
        .and_then(|(base, tip)| distance_to_segment(pointer, base, tip))
        .is_some_and(|distance| distance <= HIT_RADIUS)
}

/// The height a drag reaches, clamped to a positive document length and rounded to whole
/// millimetres.
fn dragged_height(start: Length, start_parameter: f64, parameter: f64) -> Option<Length> {
    let millimetres = start.millimetres_f64() + (parameter - start_parameter);
    if !millimetres.is_finite() {
        return None;
    }
    let maximum = (model::MAX_CONTOUR_COORDINATE.micrometres() / 1_000) as f64;
    let whole = millimetres.round().clamp(MIN_HEIGHT_MM, maximum);
    Length::from_millimetres(whole as i64)
}

/// Paints one arrow with a solid head.
fn paint_arrow(painter: &egui::Painter, base: Pos2, tip: Pos2, color: egui::Color32) {
    let direction = (tip - base).normalized();
    let perpendicular = Vec2::new(-direction.y, direction.x);
    painter.line_segment([base, tip], Stroke::new(2.5, color));
    painter.circle_filled(base, 2.5, color);
    painter.add(egui::Shape::convex_polygon(
        vec![
            tip,
            tip - direction * 12.0 + perpendicular * ARROW_HEAD,
            tip - direction * 12.0 - perpendicular * ARROW_HEAD,
        ],
        color,
        Stroke::NONE,
    ));
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use model::{Constructive, Extrude, Feature, Geometry, Path};
    use testkit::millimetres;

    use super::*;

    fn viewport() -> egui::Rect {
        egui::Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))
    }

    /// A rectangle 100 × 50 mm extruded 100 mm at the scene root.
    fn body_project() -> (Project, Id) {
        let mut project = testkit::sample_project();
        let root = project.root().id();
        let path = Path::rectangle([millimetres(20), millimetres(10)], [millimetres(120), millimetres(60)])
            .expect("a rectangle");
        let body = Constructive::new(path, vec![Feature::Extrude(Extrude::new(millimetres(100)).expect("a height"))])
            .expect("a constructive body");
        let id = project.add_object(root, "Body", Placement::IDENTITY, Geometry::Constructive(body)).expect("body");
        (project, id)
    }

    #[test]
    fn a_body_reads_its_contour_centre_and_its_own_axis() {
        let (project, id) = body_project();
        let body = Body::read(&project, id).expect("a body");
        assert_eq!(body.centre, DVec2::new(70.0, 35.0), "the centre of the bounds");
        assert!((body.axis - DVec3::Z).length() < 1.0e-9);
        let top = body.top(millimetres(100));
        assert!((top - DVec3::new(70.0, 35.0, 100.0)).length() < 1.0e-9, "the arrow starts on the top face");
    }

    #[test]
    fn a_drawn_profile_can_be_pulled_and_a_baked_mesh_cannot() {
        let mut project = testkit::sample_project();
        let root = project.root().id();
        let path = Path::rectangle([millimetres(0), millimetres(0)], [millimetres(50), millimetres(50)]).expect("path");
        let id = project.add_object(root, "Drawing", Placement::IDENTITY, Geometry::Path(path)).expect("path");
        // A body cell draws its profile as a path and the pull grows the body from it (ADR 0138).
        let body = Body::read(&project, id).expect("a profile has a ring to pull");
        assert_eq!(body.centre, DVec2::new(25.0, 25.0), "the arrow stands on the ring's bounds centre");

        let mesh = testkit::cube_mesh(millimetres(10));
        let baked = project.add_object(root, "Baked", Placement::IDENTITY, Geometry::Mesh(mesh)).expect("mesh");
        assert!(Body::read(&project, baked).is_none(), "a baked mesh has no profile to pull");
    }

    #[test]
    fn a_drag_reads_millimetres_along_the_axis_not_screen_points() {
        // The parameter is a position on the body's own axis, so one parameter unit is one
        // millimetre whether the view is close or far.
        assert_eq!(dragged_height(millimetres(100), 100.0, 250.0), Some(millimetres(250)));
        assert_eq!(dragged_height(millimetres(100), 100.0, 40.0), Some(millimetres(40)));
        assert_eq!(dragged_height(millimetres(100), 100.0, -500.0), Some(millimetres(1)), "never zero or negative");
        assert_eq!(dragged_height(millimetres(100), 100.0, f64::NAN), None);
        assert_eq!(dragged_height(millimetres(100), 100.0, f64::INFINITY), None);
    }

    #[test]
    fn a_pull_starts_at_the_created_height_and_can_be_dropped() {
        let mut gizmo = ExtrudeGizmo::default();
        assert!(!gizmo.is_active());
        let object = Id::new(7).expect("id");
        gizmo.start(object, millimetres(100));
        assert!(gizmo.is_active());
        assert_eq!(gizmo.object(), Some(object));
        assert_eq!(gizmo.height(), Some(millimetres(100)));
        assert_eq!(gizmo.committed(), Some(millimetres(100)));
        gizmo.cancel();
        assert!(!gizmo.is_active());
        assert_eq!(gizmo.height(), None);
    }

    #[test]
    fn the_field_asks_for_the_focus_on_the_frames_its_card_settles() {
        let object = Id::new(7).expect("id");
        let mut gizmo = ExtrudeGizmo::default();
        gizmo.start(object, millimetres(100));
        assert!(gizmo.take_focus(), "a fresh pull can be typed into without a click");
        assert!(gizmo.take_focus(), "the ask is repeated, because the card's first frame loses it");
        assert!(!gizmo.take_focus(), "and then the field leaves the keyboard alone");
    }

    #[test]
    fn the_arrow_projects_above_the_top_face_and_is_grabbable() {
        let (project, id) = body_project();
        let body = Body::read(&project, id).expect("a body");
        let camera = Camera::default();
        let (base, tip) = arrow_screen(camera, viewport(), &body, millimetres(100)).expect("an arrow");
        assert!(viewport().contains(base));
        assert!(viewport().contains(tip));
        assert!((tip - base).length() > 60.0, "the arrow is a readable length");
        let mid = Pos2::new((base.x + tip.x) * 0.5, (base.y + tip.y) * 0.5);
        assert!(arrow_hit(camera, viewport(), &body, millimetres(100), mid));
        assert!(!arrow_hit(camera, viewport(), &body, millimetres(100), mid + Vec2::new(60.0, 0.0)));
    }

    /// Runs the gizmo over one frame with the pointer at `pointer` and an optional button change.
    fn run_frame(
        ctx: &egui::Context,
        gizmo: &mut ExtrudeGizmo,
        project: &Project,
        frame: u32,
        pointer: Pos2,
        button: Option<(Pos2, bool)>,
    ) -> Option<ExtrudeEvent> {
        let mut events = vec![egui::Event::PointerMoved(pointer)];
        if let Some((pos, pressed)) = button {
            events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
        let input = egui::RawInput {
            screen_rect: Some(viewport()),
            time: Some(f64::from(frame) * 0.1),
            predicted_dt: 0.1,
            events,
            ..Default::default()
        };
        let mut event = None;
        let output = ctx.run_ui(input, |ui| {
            let (_, response) = ui.allocate_exact_size(viewport().size(), egui::Sense::click_and_drag());
            event = gizmo.update(ui, &response, viewport(), Camera::default(), project).expect("update");
        });
        output.drop_without_applying_deltas();
        event
    }

    /// The height the document holds for an extruded body.
    fn committed_height(project: &Project, object: Id) -> Option<Length> {
        project.object(object).and_then(|node| node.geometry().as_constructive()).and_then(Constructive::height)
    }

    /// The pointer positions of one drag along the arrow, from the press to a point further out.
    fn drag_points(project: &Project, object: Id) -> (Pos2, Pos2) {
        let body = Body::read(project, object).expect("a body");
        let (base, tip) = arrow_screen(Camera::default(), viewport(), &body, millimetres(100)).expect("an arrow");
        let direction = (tip - base).normalized();
        (base + direction * 40.0, base + direction * 120.0)
    }

    #[test]
    fn a_drag_previews_the_height_without_touching_the_document() {
        let (project, id) = body_project();
        let revision = project.revision();
        let (start, end) = drag_points(&project, id);

        let mut gizmo = ExtrudeGizmo::default();
        gizmo.start(id, millimetres(100));
        let ctx = egui::Context::default();
        assert_eq!(run_frame(&ctx, &mut gizmo, &project, 0, start, None), None);
        assert_eq!(
            run_frame(&ctx, &mut gizmo, &project, 1, start, Some((start, true))),
            None,
            "a press starts the drag"
        );
        let Some(ExtrudeEvent::Preview { object, height, top }) = run_frame(&ctx, &mut gizmo, &project, 2, end, None)
        else {
            panic!("a drag previews the height it reached");
        };
        assert_eq!(object, id);
        assert_eq!(top, None, "a body sets one number, not two");
        assert!(height > millimetres(100), "the drag raised the preview, got {height}");
        assert_eq!(gizmo.height(), Some(height));
        assert_eq!(committed_height(&project, id), Some(millimetres(100)), "the body did not move");
        assert_eq!(project.revision(), revision, "a drag does not revise the document");
    }

    #[test]
    fn a_release_keeps_the_preview_and_writes_nothing() {
        let (project, id) = body_project();
        let revision = project.revision();
        let (start, end) = drag_points(&project, id);

        let mut gizmo = ExtrudeGizmo::default();
        gizmo.start(id, millimetres(100));
        let ctx = egui::Context::default();
        run_frame(&ctx, &mut gizmo, &project, 0, start, None);
        run_frame(&ctx, &mut gizmo, &project, 1, start, Some((start, true)));
        run_frame(&ctx, &mut gizmo, &project, 2, end, None);
        let released = run_frame(&ctx, &mut gizmo, &project, 3, end, Some((end, false)));
        assert_eq!(released, None, "the release adds no height the drag had not already shown");
        assert!(gizmo.is_active(), "the pull stays open until the check or the cross");
        assert_eq!(gizmo.committed(), Some(millimetres(100)), "the document still holds the created height");
        assert_eq!(project.revision(), revision);
    }

    #[test]
    fn a_click_outside_the_arrow_finishes_the_pull_at_its_height() {
        let (project, id) = body_project();
        let pointer = Pos2::new(700.0, 500.0);
        let mut gizmo = ExtrudeGizmo::default();
        gizmo.start(id, millimetres(100));
        let ctx = egui::Context::default();
        run_frame(&ctx, &mut gizmo, &project, 0, pointer, None);
        run_frame(&ctx, &mut gizmo, &project, 1, pointer, Some((pointer, true)));
        let event = run_frame(&ctx, &mut gizmo, &project, 2, pointer, Some((pointer, false)));
        assert_eq!(event, Some(ExtrudeEvent::Finish { object: id, height: millimetres(100), top: None }));
        assert!(Body::read(&project, id).is_some(), "finishing reads the body, it does not touch it");
    }
}
