//! The drawing mode: the draft a draw tool is building on the work plane.
//!
//! The draft is view state until its one commit: `crate::draw::DrawDraft` collects the
//! clicks, and `commit_drawing` turns the finished chain into one command — `object.path.create`
//! for a sketch tool, `panel.create` carrying the closed ring as its outline when the *Panel from
//! contour* cell armed the draft (ADR 0114), or a path the shell extrudes at once when a body cell
//! drew a profile (ADR 0138). The shell measures the work plane, so a commit receives
//! the *world* frame and converts it to the parent's frame here. A committed path hands the new node
//! to the contour edit mode, a body's profile to the height pull instead.
//!
//! Refuses to commit with no draft via `DrawDraftError`, a panel draft whose ring is still open, and
//! returns the command's own sentence when the document rejects the ring; a refused close reopens
//! the ring so the user keeps the shape.
//!
//! Changes when: a draw tool's input, the one commit path or what a draft can become changes.

use engine::{EditScope, Session};
use model::{
    DEFAULT_BANDS, DEFAULT_SEGMENTS, Panel, PanelOutline, Placement, Primitive, PrimitiveError, PrimitiveKind,
};
use text::{Lang, Message};
use units::Length;

use super::Editor;
use crate::contour_edit::ContourTarget;
use model::PathShape;

use crate::box_tool::{BoxBase, BoxDraft};
use crate::draw::{DEFAULT_POLYGON_SIDES, DrawDraft, DrawDraftError, DrawIntent, DrawTool, DrawnNode};

/// The name a drawn body commits under: the cell the profile belongs to, not the tool.
///
/// A rectangle is a box and a circle is a cylinder whatever method drew them, so the node says what
/// the body is; any other ring keeps the drawing's own name (ADR 0138).
fn body_name(profile: &model::Path, lang: Lang) -> String {
    match profile.shape() {
        Some(PathShape::Rectangle) => Message::ToolBox.text(lang).to_owned(),
        Some(PathShape::Circle) => Message::ToolCylinder.text(lang).to_owned(),
        _ => Message::DrawingName.with(lang, [Message::GroupBodies.text(lang)]),
    }
}

/// The primitive a drawn figure becomes: the kind its cell places, sized by the figure's own radius.
///
/// A sphere stores the radius, the side count and the latitude bands, and the drawn circle supplies
/// the first of them; the tessellation is the model's default, which the card then edits (ADR 0054,
/// 0139). A cone reads the same circle as its base and starts sharp, so `height` — the ribbon's
/// height, or the one the pull has reached — is the distance it rises; the top radius is the pull's
/// own number and is typed there (ADR 0140).
fn drawn_primitive(kind: PrimitiveKind, radius: Length, height: Length) -> Result<Primitive, PrimitiveError> {
    match kind {
        PrimitiveKind::Sphere => Primitive::sphere(radius, DEFAULT_SEGMENTS, DEFAULT_BANDS),
        PrimitiveKind::Cone => Primitive::cone(radius, Length::ZERO, height, DEFAULT_SEGMENTS),
    }
}

impl Editor {
    /// The live drawing draft, when a drawing tool is building a chain.
    pub fn drawing(&self) -> Option<&DrawDraft> {
        self.drawing.as_ref()
    }

    /// What the running draft will become, which is a path object unless a cell armed a board.
    pub const fn drawing_intent(&self) -> DrawIntent {
        self.drawing_intent
    }

    /// Starts an empty drawing draft with `tool` and leaves the other pointer modes.
    ///
    /// The regular polygon uses the default side count; every other tool ignores the count, so
    /// this is the plain "start" a caller without a side field uses.
    pub fn start_drawing(&mut self, tool: DrawTool) {
        self.start_drawing_with(tool, DEFAULT_POLYGON_SIDES);
    }

    /// Arms the drafting of one panel's outline: the same draft, cut to a board on its close.
    ///
    /// The free-outline tool is the one that can enclose any board, and *Close* is the operation that
    /// finishes it; the commit then dispatches `panel.create` with the ring as the outline instead of
    /// committing a path the user never asked for (ADR 0042, 0114). Arming writes nothing, and a
    /// draft the document refuses keeps its ring editable like any other.
    pub fn start_panel_drawing(&mut self) {
        self.start_drawing_with(DrawTool::Polyline, DEFAULT_POLYGON_SIDES);
        self.drawing_intent = DrawIntent::Panel;
    }

    /// Arms the profile of a solid body: the figure a body cell draws before its height is pulled.
    ///
    /// The box and cylinder cells offer the sketch grammar's own rectangle and circle tools
    /// (ADR 0138), so the draft is the sketch draft and the figure is a closed ring; the commit
    /// stores that ring as a path and the shell pulls its height, which turns the node into the body.
    /// Only a tool that closes its figure can be a profile, so an open chain never reaches the pull.
    pub fn start_body_drawing(&mut self, tool: DrawTool) {
        self.start_drawing_with(tool, DEFAULT_POLYGON_SIDES);
        self.drawing_intent = DrawIntent::Body;
    }

    /// Arms the base rectangle of a product frame: the figure a body cell draws before its height.
    ///
    /// A frame reserves a box of three independent sides, and the sketch's rectangle tool is the one
    /// that draws it (N19, ADR 0101): the plates, the typed width and depth, the Enter that places the
    /// second corner and the preview are the sketch's own, exactly as they are for a solid's profile.
    /// Only what the finished figure becomes differs — a body is swept on Z, a frame is pulled — so the
    /// intent, not the tool, says which cell armed the draft. Nothing reaches the document before the
    /// pull's one `product.create`.
    pub fn start_product_box_drawing(&mut self, tool: DrawTool) {
        self.start_drawing_with(tool, DEFAULT_POLYGON_SIDES);
        self.drawing_intent = DrawIntent::ProductBox;
    }

    /// Arms the base outline of a product frame: the ring its reserved space stands on.
    ///
    /// The free-outline tool draws any closed ring, because a frame is not always a rectangle: a
    /// corner cabinet and a drawn end table are the same gesture with a different base (ADR 0159).
    /// The ring is view state until the height is pulled from it, and nothing is written before that
    /// pull's one `product.create`.
    pub fn start_product_drawing(&mut self) {
        self.start_drawing_with(DrawTool::Polyline, DEFAULT_POLYGON_SIDES);
        self.drawing_intent = DrawIntent::Product;
    }

    /// Turns the closed figure a frame draft drew into the box draft whose height is pulled.
    ///
    /// The figure becomes the draft's base. A rectangle drawn between two corners is the box's own
    /// base: its minimum corner is the frame's zero and its bounds are the width and the depth, so a
    /// drawn frame reserves the same plain box the card writes, and the corner grips still read it as
    /// a box (ADR 0101). Any other closed figure is kept as the ring it was drawn as, anchored by the
    /// envelope's own rule (ADR 0159). The height pull and its commit are the drawn frame's own, so a
    /// footprint and a rectangle end in one command. A ring that is still open is refused with the
    /// sentence the *Close* operation would give, and the draft stays editable.
    pub fn start_product_pull(&mut self, lang: Lang) -> Result<(), String> {
        if self.drawing.is_none() {
            return Err(DrawDraftError::NotStarted.to_string());
        }
        // A fixed figure is closed by the tool that drew it; only a chain is closed here, and only a
        // chain that can close — a rectangle of two corners has no closing segment.
        if self.drawing.as_ref().is_some_and(|draft| draft.tool().closes()) {
            self.close_drawing().map_err(|_| Message::PanelContourOpen.text(lang).to_owned())?;
        }
        let Some(draft) = self.drawing.as_ref() else {
            return Err(DrawDraftError::NotStarted.to_string());
        };
        let path = draft.to_path().map_err(|error| error.to_string())?;
        let contour = path.to_contour().map_err(|error| error.to_string())?;
        let (min, max) = contour.bounds_micrometres();
        let corner = [Length::from_micrometres(min[0]), Length::from_micrometres(min[1])];
        let span = |axis: usize| -> Option<Length> {
            let low = min.get(axis).copied()?;
            let high = max.get(axis).copied()?;
            Some(Length::from_micrometres(high.saturating_sub(low)))
        };
        let base = match (draft.tool(), span(0), span(1)) {
            // A rectangle on the plane's own axes is the box: three sides and no footprint, which is
            // the document the card writes and the shape the corner grips edit.
            (DrawTool::Rectangle, Some(width), Some(depth)) => {
                BoxDraft::from_rectangle(BoxBase { corner, width, depth })
            }
            _ => BoxDraft::from_ring(corner, contour),
        };
        self.box_draft = Some(base);
        self.drawing = None;
        self.drawing_intent = DrawIntent::Path;
        Ok(())
    }

    /// Arms a primitive's placement: the circle a sphere is built around (ADR 0139).
    ///
    /// The sphere cell carries the sketch's own circle methods, so the draft is the sketch draft and
    /// the figure is a closed circle; the commit stores no path — the circle's centre is the
    /// primitive's origin and its radius is the primitive's own number. A tool that places no
    /// primitive is refused rather than started as a figure, because a map row is data.
    pub fn start_primitive_drawing(&mut self, tool: DrawTool) -> Result<(), String> {
        let kind = tool.primitive_kind().ok_or_else(|| format!("The {} tool places no primitive.", tool.label()))?;
        self.start_drawing_with(tool, DEFAULT_POLYGON_SIDES);
        self.drawing_intent = DrawIntent::Primitive(kind);
        Ok(())
    }

    /// Starts an empty drawing draft with `tool` and the regular polygon's side count.
    ///
    /// A running draft is replaced, because it wrote nothing and undo gains no half figure
    /// (ADR 0076): choosing another tool is one gesture, not "cancel, then start". The count is
    /// clamped by [`DrawDraft::with_sides`], so the caller can pass anything and the document still
    /// receives a polygon it accepts. A draft starts as a path, so the sketch cells never inherit a
    /// board's intent from the cell before them.
    pub fn start_drawing_with(&mut self, tool: DrawTool, sides: u32) {
        self.mesh_edit = None;
        self.contour_edit = None;
        self.box_draft = None;
        self.fill = None;
        self.measure = None;
        self.drawing = Some(DrawDraft::with_sides(tool, sides));
        self.drawing_intent = DrawIntent::Path;
    }

    /// Flips the running polyline between straight and arc input.
    ///
    /// Returns the mode after the call, or `None` when no draft is running. Other tools have no
    /// second mode, so the mode they report stays false.
    pub fn toggle_drawing_arc_mode(&mut self) -> Option<bool> {
        self.drawing.as_mut().map(DrawDraft::toggle_arc_mode)
    }

    /// Chooses whether the running polyline's next segment is an arc.
    ///
    /// Returns the mode after the call, or `None` when no draft is running. Other tools have no
    /// second mode, so the mode they report stays false.
    pub fn set_drawing_arc_mode(&mut self, arc: bool) -> Option<bool> {
        self.drawing.as_mut().map(|draft| draft.set_arc_mode(arc))
    }

    /// Chooses the side count the running draft's polygon is built with.
    ///
    /// The count is the figure's own number, so a plate at the cursor writes it here rather than into
    /// the point: the draft clamps it to the model's range and returns the count it now holds, or
    /// `None` when no draft is running. The document sees nothing until the commit, exactly as for
    /// every other number of a drawing.
    pub fn set_drawing_sides(&mut self, sides: u32) -> Option<u32> {
        self.drawing.as_mut().map(|draft| draft.set_sides(sides))
    }

    /// Appends a work-plane point to the running draft as a straight segment.
    pub fn drawing_point(&mut self, point: [Length; 2]) -> Result<(), DrawDraftError> {
        self.drawing.as_mut().ok_or(DrawDraftError::NotStarted)?.push(point)
    }

    /// Starts an arc at the running draft's last end through `point`.
    ///
    /// The next [`Editor::drawing_point`] ends the arc and the segment after it starts at the stored
    /// arc end, so a chain that mixes both kinds still closes (ADR 0048).
    pub fn drawing_arc_through(&mut self, point: [Length; 2]) -> Result<(), DrawDraftError> {
        self.drawing.as_mut().ok_or(DrawDraftError::NotStarted)?.push_arc_through(point)
    }

    /// Undoes the last drawing action: a close first, then the last point.
    ///
    /// Returns `false` when no draft is running or it is empty and open.
    pub fn undo_drawing(&mut self) -> bool {
        self.drawing.as_mut().is_some_and(DrawDraft::undo)
    }

    /// Commits the running draft as one node: a path object, the panel cut to its ring, a body's
    /// profile or the primitive the figure is built around.
    ///
    /// The drawing lands in the armed container, or at the scene root like every other creation
    /// tool. `placement` is the work plane's *world* frame, the one the shell can measure; the
    /// command stores a placement relative to the parent, so this converts it with the parent's
    /// inverse first. The command then moves the chain so its creation pivot is the node's local
    /// zero. A path becomes the selection and immediately enters contour edit mode with its grips, so
    /// the figure the user just drew can be reshaped without a second click; a panel is selected and
    /// needs none, because its ring is edited through its card. A body's profile is selected and left
    /// to the shell, which pulls its height in the same edit scope; Escape during that pull takes the
    /// profile back. A primitive is selected at once: its figure is not stored, so the drawn circle's
    /// centre and radius live on as the node's placement and its parameter (ADR 0139). Escape or
    /// Enter leaves the contour mode. A refused commit leaves the draft and the selection untouched.
    pub fn commit_drawing(
        &mut self,
        session: &mut Session,
        placement: Placement,
        lang: Lang,
    ) -> Result<DrawnNode, String> {
        let Some(draft) = self.drawing.clone() else {
            return Err(DrawDraftError::NotStarted.to_string());
        };
        let parent = self.creation_target(session.project());
        let placement = session.project().placement_in_parent(parent, placement).map_err(|error| error.to_string())?;
        let tool = draft.tool();
        let intent = self.drawing_intent;
        // A body is swept from a profile, so an open chain is refused before anything is written:
        // the shell's pull has no ring to extrude. The cells offer only closing tools, so this
        // guards a caller rather than a gesture.
        if intent == DrawIntent::Body && !draft.to_path().is_ok_and(|path| path.is_closed()) {
            return Err(Message::BodyProfileOpen.text(lang).to_owned());
        }
        // A body's ring and the body itself are one gesture, so the profile enters the document in
        // the scope the extrusion closes; every other figure is its own entry (ADR 0092, 0138). A
        // cone's base circle is not the whole shape either: its height and its top circle are chosen
        // on the pull, so it enters the document in the scope that pull closes (ADR 0141).
        let scope = match intent {
            DrawIntent::Body => Some(session.edit_scope()),
            DrawIntent::Primitive(PrimitiveKind::Cone) => Some(session.edit_scope()),
            _ => None,
        };
        let object = match intent {
            DrawIntent::Path => draft.commit_in(None, session, parent, placement, None, lang)?,
            DrawIntent::Body => {
                let profile = draft.to_path().map_err(|error| error.to_string())?;
                let name = body_name(&profile, lang);
                draft.commit_in(scope, session, parent, placement, Some(&name), lang)?
            }
            DrawIntent::Panel => self.commit_panel(&draft, session, parent, placement, lang)?,
            DrawIntent::Primitive(kind) => self.commit_primitive(&draft, session, parent, placement, kind, scope)?,
            // A frame's base is not a node: the figure waits for the height pull and the one
            // `product.create` that the shell runs from the frame draft (ADR 0101, ADR 0159).
            DrawIntent::Product | DrawIntent::ProductBox => {
                return Err(Message::ProductFrameNeedsHeight.text(Lang::En).to_owned());
            }
        };
        self.drawing = None;
        self.drawing_intent = DrawIntent::Path;
        self.selection.replace(object);
        // A deliberate selection also counts as the first show, so the tree does not replace the
        // freshly drawn node with the first panel on the next frame.
        self.initialized = true;
        if intent == DrawIntent::Path {
            // The new node is selected and editable, so the drawing hands straight over to the
            // grips; a tool that somehow produced a node without a path simply stays out of edit
            // mode.
            self.start_contour_edit(session.project(), ContourTarget::Path(object));
        }
        Ok(DrawnNode { object, tool, intent, scope })
    }

    /// Commits the running ring as one panel, the board the user drew.
    ///
    /// The ring *is* the outline: `panel.create` accepts a closed contour and takes the nominal size
    /// the ring measures, so the board is one command and one undo entry with no path left beside it
    /// (ADR 0042, 0114). An open chain is refused with a sentence rather than the document's error,
    /// because closing the ring is what the tool asks for; a ring the panel's own rules reject — a
    /// sliver, a self-crossing chain — keeps the draft and reports that the outline is unusable.
    fn commit_panel(
        &mut self,
        draft: &DrawDraft,
        session: &mut Session,
        parent: model::Id,
        placement: Placement,
        lang: Lang,
    ) -> Result<model::Id, String> {
        if !draft.is_closed() {
            return Err(Message::PanelContourOpen.text(lang).to_owned());
        }
        let path = draft.to_path().map_err(|error| error.to_string())?;
        let contour = path.to_contour().map_err(|error| error.to_string())?;
        let anchored = Panel::anchored_contour(&contour).map_err(|_| Message::PanelContourUnusable.text(lang))?;
        let nominal_size = Panel::contour_size(&anchored).map_err(|_| Message::PanelContourUnusable.text(lang))?;
        self.create_panel_sized(
            session,
            parent,
            Message::ToolPanel.text(lang).to_owned(),
            nominal_size,
            Some(PanelOutline::Contour { contour }),
            Vec::new(),
            placement,
            None,
        )
    }

    /// Commits the figure a primitive cell drew as one `object.primitive.create`.
    ///
    /// The figure is the circle the primitive is built around: its centre becomes the node's origin
    /// — the plane's own rotation keeps the primitive's axes on the work plane — and its radius
    /// becomes the sphere's radius (ADR 0139). The drawn circle is not stored, so the node is a
    /// parameter set from the first frame and its card edits the radius, the side count and the
    /// bands. `placement` is the work plane's frame relative to the parent; a primitive is centred
    /// on its local zero, so the centre is composed onto that frame.
    fn commit_primitive(
        &mut self,
        draft: &DrawDraft,
        session: &mut Session,
        parent: model::Id,
        placement: Placement,
        kind: PrimitiveKind,
        scope: Option<EditScope>,
    ) -> Result<model::Id, String> {
        let path = draft.to_path().map_err(|error| error.to_string())?;
        let (centre, radius) = path
            .circle_center_radius()
            .ok_or_else(|| "The drawn figure is not a round, so it builds no primitive.".to_owned())?;
        let primitive = drawn_primitive(kind, radius, self.extrude_height()).map_err(|error| error.to_string())?;
        let offset = Placement::IDENTITY
            .with_translation(centre[0], centre[1], Length::ZERO)
            .map_err(|error| error.to_string())?;
        let placement = Placement::compose(placement, offset).map_err(|error| error.to_string())?;
        self.create_primitive_node(session, parent, primitive, placement, scope)
    }

    /// Closes the running ring and commits it as one node.
    ///
    /// This is the only operation behind *Close*, whichever surface asked for it: the ribbon, the
    /// status bar and a click on the first draft point all end here (ADR 0052). Closing and
    /// committing belong together, because a closed draft that never reached the document is a ring
    /// the user already finished and cannot edit.
    ///
    /// A refused close changes nothing — the ring stays open and editable, and no command ran. A
    /// close the document then refuses reopens the ring for the same reason: the user keeps the
    /// shape they drew and can fix it without redrawing.
    pub fn close_and_commit_drawing(
        &mut self,
        session: &mut Session,
        placement: Placement,
        lang: Lang,
    ) -> Result<DrawnNode, String> {
        self.close_drawing().map_err(|error| error.to_string())?;
        match self.commit_drawing(session, placement, lang) {
            Ok(drawn) => Ok(drawn),
            Err(error) => {
                self.reopen_drawing();
                Err(error)
            }
        }
    }

    /// Closes the running draft back to its first point.
    ///
    /// A primitive that only closes: the one operation the chrome runs is
    /// [`Self::close_and_commit_drawing`], so a closed draft cannot be left behind.
    pub(crate) fn close_drawing(&mut self) -> Result<(), DrawDraftError> {
        self.drawing.as_mut().ok_or(DrawDraftError::NotStarted)?.close()
    }

    /// Reopens a closed ring, so a refused commit leaves the drawing editable.
    fn reopen_drawing(&mut self) {
        if let Some(draft) = self.drawing.as_mut()
            && draft.is_closed()
        {
            draft.undo();
        }
    }

    /// Drops the running draft; the document never changed.
    pub fn cancel_drawing(&mut self) {
        self.drawing = None;
        self.drawing_intent = DrawIntent::Path;
    }
}
