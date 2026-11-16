//! The selection and the set of open containers.
//!
//! Both are view state: `Selection` keeps the multi-selection in the order the user built it and
//! the open set remembers which containers show their children. The rules that turn a viewport hit
//! into a selected node live in `crate::target`; this module only applies them.
//!
//! A click carries a [`Pick`]: a plain one replaces the selection, Shift adds the node at the end of
//! the order or takes it out again, and Ctrl removes it and nothing else. The tree row and the
//! viewport read the same modifiers through that type, so the two surfaces cannot disagree.
//!
//! A drag on empty space draws a [`SelectionBand`] instead of a click. The direction of the drag
//! decides its rule — a band dragged right or down takes what it encloses, one dragged left or up
//! takes everything it touches (ADR 0137) — and the release applies that rule to the scene the
//! viewport last uploaded, through the same container resolution a click uses. The band reads the
//! projected silhouette of every shape, so what it takes is what the person saw it cover.
//!
//! Refuses nothing: an empty click replaces with nothing, a band that covers nothing replaces with
//! nothing, and `exit_level` with no open level above the selection does nothing, so a gesture the
//! scene cannot honour leaves the state unchanged.
//!
//! Changes when: a gesture selects, adds to the selection, or opens and closes a container.
//!
//! [ADR 0137]: ../../../docs/decisions/0137-a-rubber-band-selects-by-the-direction-it-is-dragged.md

use std::collections::HashSet;

use egui::{Pos2, Rect, Shape, Stroke};
use engine::Session;
use geom::DVec2;
use model::{Container, Id, Project};
use scene::{Camera, NdcRect, SceneFrame};

use super::Editor;
use crate::scene_tree::Pick;
use crate::target::{child_containing, resolve_viewport_pick};
use crate::theme;

/// A rubber band: the two corners a selection drag spans, in screen points.
///
/// It is view state, not document state: it exists while the pointer is down and is gone by the
/// frame after the release (ADR 0029). The press corner is kept, not only the rectangle, because a
/// window and a crossing band cover the same rectangle and differ only in which way it was drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SelectionBand {
    /// Where the press landed, in screen points.
    pub from: Pos2,
    /// Where the pointer is now, in screen points.
    pub to: Pos2,
}

impl SelectionBand {
    /// The band between two corners.
    pub const fn new(from: Pos2, to: Pos2) -> Self {
        Self { from, to }
    }

    /// The same band ending at `to`.
    pub const fn with_to(self, to: Pos2) -> Self {
        Self { to, ..self }
    }

    /// The rectangle the two corners span, whichever way round the drag went.
    pub fn rect(self) -> Rect {
        Rect::from_two_pos(self.from, self.to)
    }

    /// The rule the release applies, decided by the direction of the drag alone.
    ///
    /// The dominant axis is the one the drag mostly travelled, and along it the forward way is
    /// rightward or downward: that band encloses what it covers and draws a solid outline, the other
    /// way draws dashed and takes what it touches. A diagonal is read along its own dominant axis, so
    /// the answer never depends on which component happened to be a hair larger than a tie.
    pub fn rule(self) -> BandRule {
        let delta = self.to - self.from;
        let forward = if delta.x.abs() >= delta.y.abs() { delta.x >= 0.0 } else { delta.y >= 0.0 };
        if forward { BandRule::Window } else { BandRule::Crossing }
    }

    /// Paints the band over the viewport image, clipped to the view it was drawn in.
    ///
    /// It is chrome paint on the scene layer, so the work cursor is drawn over it and every panel,
    /// card and menu masks it (ADR 0123). A window draws a solid outline and a crossing band a
    /// dashed one, both in the selection ink, so which rule the release will apply is visible before
    /// it acts.
    pub fn paint(self, ui: &egui::Ui, viewport: Rect) {
        let rect = self.rect().intersect(viewport);
        if !rect.is_positive() {
            return;
        }
        let (ink, fill) = theme::selection_band();
        let painter = ui.painter().with_clip_rect(viewport);
        painter.rect_filled(rect, 0.0, fill);
        let outline = [rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom(), rect.left_top()];
        let stroke = Stroke::new(theme::BAND_STROKE, ink);
        match self.rule() {
            BandRule::Window => {
                painter.add(Shape::line(outline.to_vec(), stroke));
            }
            BandRule::Crossing => {
                painter.extend(Shape::dashed_line(&outline, stroke, theme::BAND_DASH, theme::BAND_GAP));
            }
        }
    }
}

/// What a band's release takes (ADR 0137).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BandRule {
    /// Everything the band encloses: a drag to the right or downward.
    Window,
    /// Everything the band touches: a drag to the left or upward.
    Crossing,
}

/// One view of the frame: the camera it is drawn through and the rectangle it occupies.
///
/// The band tests the frame the viewport last uploaded, so it covers the shapes the person saw and
/// not the document as it stands a frame later (ADR 0053).
#[derive(Clone, Copy)]
pub struct BandView<'a> {
    /// The uploaded scene.
    pub scene: &'a SceneFrame,
    /// The view's camera.
    pub camera: Camera,
    /// The view's rectangle, in screen points.
    pub viewport: Rect,
}

impl BandView<'_> {
    /// The band as the scene measures it: normalized device coordinates of this view.
    fn ndc(self, band: SelectionBand) -> NdcRect {
        let corner = |point: Pos2| {
            let ndc = |along: f32, extent: f32, flip: bool| {
                let extent = f64::from(extent);
                if extent <= 0.0 {
                    return 0.0;
                }
                let fraction = f64::from(along) / extent;
                if flip { 1.0 - 2.0 * fraction } else { 2.0 * fraction - 1.0 }
            };
            DVec2::new(
                ndc(point.x - self.viewport.left(), self.viewport.width(), false),
                ndc(point.y - self.viewport.top(), self.viewport.height(), true),
            )
        };
        NdcRect::from_corners(corner(band.from), corner(band.to))
    }

    /// The view's width over its height, the aspect the camera projects with.
    fn aspect(self) -> f64 {
        f64::from(self.viewport.width()) / f64::from(self.viewport.height())
    }
}

impl Editor {
    /// The primary selected node, if any.
    pub fn selected_node(&self) -> Option<Id> {
        self.selection.primary()
    }

    /// Every selected node in selection order, oldest first; the last one is the primary.
    ///
    /// Multi-selection is view state, so a gizmo drag moves all of these nodes as one document
    /// change while the inspector keeps showing the primary.
    pub fn selected_nodes(&self) -> &[Id] {
        self.selection.ids()
    }

    /// Replaces the selection with one node, or clears it for `None`.
    ///
    /// A deliberate selection also counts as the first show, so the tree does not replace it with
    /// the first panel afterwards.
    pub fn select_node(&mut self, node: Option<Id>) {
        self.initialized = true;
        match node {
            Some(node) => self.selection.replace(node),
            None => self.selection.clear(),
        }
    }

    /// Replaces the selection with several nodes, in the given order; the last one is the primary.
    ///
    /// The tree's Shift-click builds the list one row at a time; a surface that knows the set up
    /// front — a test, and later a marquee or the assistant — sets it here.
    pub fn select_nodes(&mut self, nodes: Vec<Id>) {
        self.initialized = true;
        self.selection.replace_all(nodes);
    }

    /// The containers whose contents the pointer reaches directly: the open levels of the viewport.
    ///
    /// The picker resolves a click through this set and the shell draws the bracket of every level in
    /// it (U89), so both read one field instead of each keeping a copy of the open state (ADR 0098).
    pub fn open_containers(&self) -> &HashSet<Id> {
        &self.open
    }

    /// Selects the result of a viewport click on `hit`, honouring the modifier the click carried.
    ///
    /// The viewport returns the panel under the pointer; a closed container above it is selected
    /// instead, so a click picks the cabinet rather than one of its boards. `open_product` is the
    /// product the session works in, which exposes its parts exactly as an open group does.
    ///
    /// A plain click on the scene replaces the selection and a plain click outside it clears it;
    /// Shift adds the picked node at the end of the order (a second Shift-click takes it back out)
    /// and Ctrl removes it alone. A modified click on empty space changes nothing, because there is
    /// no node for the modifier to act on.
    pub fn select_viewport_hit(&mut self, project: &Project, open_product: Option<Id>, hit: Option<Id>, pick: Pick) {
        self.initialized = true;
        match hit.and_then(|hit| resolve_viewport_pick(project, &self.open, open_product, hit)) {
            Some(node) => self.selection.pick(node, pick),
            None if pick == Pick::Replace => self.selection.clear(),
            None => {}
        }
    }

    /// Selects every object the band covers, honouring the rule its direction chose.
    ///
    /// The frame is the uploaded scene, so the band reads the geometry the person saw, and each
    /// shape it covers is resolved through the open levels exactly as a click would be: a closed
    /// cabinet is one object, and an open product exposes its parts. A closed container is covered
    /// when every shape it draws is, which is what "the band encloses this cabinet" means; a
    /// crossing band takes it when any shape it draws reaches the band (ADR 0022, ADR 0137).
    ///
    /// A plain band replaces the selection — a band over nothing clears it, like a click on empty
    /// space — Shift adds every member it takes, and Ctrl removes them. A band never toggles a
    /// member the way a Shift-click does: a release is one gesture over a set, not one node.
    pub fn select_band(&mut self, session: &Session, view: BandView<'_>, band: SelectionBand, pick: Pick) {
        self.initialized = true;
        if !view.viewport.is_positive() {
            return;
        }
        let project = session.project();
        let open_product = session.active_product();
        let rule = band.rule();
        // One entry per object: whether every shape it draws was enclosed, and whether any reached
        // the band. The order is the frame's, so the same band always builds the same selection.
        let mut objects: Vec<(Id, bool, bool)> = Vec::new();
        for (shape, coverage) in view.scene.band_hits(view.camera, view.aspect(), view.ndc(band)) {
            let Some(node) = resolve_viewport_pick(project, &self.open, open_product, shape) else {
                continue;
            };
            let index = match objects.iter().position(|(id, ..)| *id == node) {
                Some(index) => index,
                None => {
                    objects.push((node, true, false));
                    objects.len() - 1
                }
            };
            let Some(entry) = objects.get_mut(index) else {
                continue;
            };
            match coverage {
                scene::BandCoverage::Inside => entry.2 = true,
                scene::BandCoverage::Touched => {
                    entry.1 = false;
                    entry.2 = true;
                }
                // A shape the band does not reach is not enclosed either, which is what keeps a
                // window around one board of a closed cabinet from taking the whole cabinet.
                scene::BandCoverage::Missed => entry.1 = false,
            }
        }
        let taken: Vec<Id> = objects
            .into_iter()
            .filter(|(_, enclosed, reached)| match rule {
                BandRule::Window => *enclosed,
                BandRule::Crossing => *reached,
            })
            .map(|(id, ..)| id)
            .collect();
        let selection = match pick {
            Pick::Replace => taken,
            Pick::Add => {
                let mut selection = self.selection.ids().to_vec();
                for id in taken {
                    if !selection.contains(&id) {
                        selection.push(id);
                    }
                }
                selection
            }
            Pick::Remove => self.selection.ids().iter().copied().filter(|id| !taken.contains(id)).collect(),
        };
        self.selection.replace_all(selection);
    }

    /// Selects a node the shell already resolved, with the modifier its click carried.
    ///
    /// A camera or light glyph is picked as itself, without the container resolution a viewport hit
    /// goes through, so the shell hands the node over and the rule for the modifier stays here.
    pub fn pick_node(&mut self, node: Id, pick: Pick) {
        self.initialized = true;
        self.selection.pick(node, pick);
    }

    /// The subject a context menu opened at a viewport hit is about.
    ///
    /// It resolves the hit the way a click resolves its selection — through the open levels and
    /// the working product — so a right click on a board of a closed cabinet offers the cabinet's
    /// actions, not the board's. Empty space is its own subject, because the menu it opens is the
    /// document's rather than a node's.
    pub fn context_menu_subject(
        &self,
        project: &Project,
        open_product: Option<Id>,
        hit: Option<Id>,
    ) -> crate::node_actions::Subject {
        match hit.and_then(|hit| resolve_viewport_pick(project, &self.open, open_product, hit)) {
            Some(node) if node == project.root().id() => crate::node_actions::Subject::Root,
            Some(node) => crate::node_actions::Subject::Node(node),
            None => crate::node_actions::Subject::Empty,
        }
    }

    /// Acts on a double click in the viewport: opens a product, or enters one level below a group.
    ///
    /// A double click on a product opens it as the session's working product (ADR 0098, ADR 0100)
    /// and arms it for creation; a double click on a group reveals its contents and selects the
    /// child under the pointer. A double click on a panel, a loose object or the root does nothing
    /// beyond the selection the first click already made — in particular it never opens a product
    /// that does not own the hit.
    ///
    /// Returns whether the open product changed, so the shell can rebuild the envelope inks in the
    /// frame the click lands in.
    pub fn enter_viewport_level(&mut self, session: &mut Session, hit: Option<Id>) -> bool {
        let Some(hit) = hit else {
            return false;
        };
        self.initialized = true;
        let open_product = session.active_product();
        let project = session.project();
        let Some(selected) = resolve_viewport_pick(project, &self.open, open_product, hit) else {
            return false;
        };
        self.selection.replace(selected);
        if !project.is_container(selected) {
            return false;
        }
        if project.container(selected).is_some_and(Container::is_product) {
            return self.open_product(session, selected);
        }
        self.open.insert(selected);
        if let Some(child) = child_containing(project, selected, hit) {
            self.selection.replace(child);
        }
        false
    }

    /// Closes one level and returns the selection to the container that owns it.
    ///
    /// The current node closes when it is an open container; otherwise the nearest open ancestor
    /// closes. With no open container above the selection inside the product the session works in,
    /// that product closes instead, so the key leaves a piece the same way it leaves a group
    /// (ADR 0098). With nothing open this does nothing, and a selection outside the open product
    /// never closes it. Returns whether the open product closed, because that repaints its envelope
    /// without a document revision (ADR 0100).
    pub fn exit_level(&mut self, session: &mut Session) -> bool {
        let Some(selected) = self.selection.primary() else {
            return false;
        };
        let root = session.project().root().id();
        if session.project().is_container(selected) && self.open.remove(&selected) {
            if let Some(parent) = session.project().parent_of(selected)
                && parent != root
            {
                self.selection.replace(parent);
            }
            return false;
        }
        let mut node = selected;
        while let Some(parent) = session.project().parent_of(node) {
            if parent == root {
                break;
            }
            if self.open.remove(&parent) {
                self.selection.replace(parent);
                return false;
            }
            node = parent;
        }
        let Some(product) = session.active_product() else {
            return false;
        };
        if session.project().product_of(selected) != Some(product) {
            return false;
        }
        self.close_product(session);
        self.selection.replace(product);
        true
    }

    /// Drops open flags for nodes that no longer exist or no longer own children.
    pub(super) fn retain_open(&mut self, project: &Project) {
        self.open.retain(|id| project.is_container(*id));
    }
}
