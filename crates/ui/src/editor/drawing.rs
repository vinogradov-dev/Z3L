//! The drawing mode: the draft a draw tool is building on the work plane.
//!
//! The draft is view state until its one commit: `crate::draw::DrawDraft` collects the
//! clicks, and `commit_drawing` turns the finished chain into one command — `object.path.create`
//! for a sketch tool, or `panel.create` carrying the closed ring as its outline when the *Panel from
//! contour* cell armed the draft (ADR 0114). The shell measures the work plane, so a commit receives
//! the *world* frame and converts it to the parent's frame here. A committed path hands the new node
//! to the contour edit mode.
//!
//! Refuses to commit with no draft via `DrawDraftError`, a panel draft whose ring is still open, and
//! returns the command's own sentence when the document rejects the ring; a refused close reopens
//! the ring so the user keeps the shape.
//!
//! Changes when: a draw tool's input, the one commit path or what a draft can become changes.

use furnicraft_engine::Session;
use furnicraft_model::{Panel, PanelOutline, Placement};
use furnicraft_text::{Lang, Message};
use furnicraft_units::Length;

use super::Editor;
use crate::contour_edit::ContourTarget;
use crate::draw::{DEFAULT_POLYGON_SIDES, DrawDraft, DrawDraftError, DrawIntent, DrawTool, DrawnNode};

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
        self.pending_primitive = None;
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

    /// Commits the running draft as one node: a path object, or the panel cut to its ring.
    ///
    /// The drawing lands in the armed container, or at the scene root like every other creation
    /// tool. `placement` is the work plane's *world* frame, the one the shell can measure; the
    /// command stores a placement relative to the parent, so this converts it with the parent's
    /// inverse first. The command then moves the chain so its creation pivot is the node's local
    /// zero. A path becomes the selection and immediately enters contour edit mode with its grips, so
    /// the figure the user just drew can be reshaped without a second click; a panel is selected and
    /// needs none, because its ring is edited through its card. Escape or Enter leaves that mode. A
    /// refused commit leaves the draft and the selection untouched.
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
        let object = match intent {
            DrawIntent::Path => draft.commit(session, parent, placement, lang)?,
            DrawIntent::Panel => self.commit_panel(&draft, session, parent, placement, lang)?,
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
        Ok(DrawnNode { object, tool, intent })
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
        parent: furnicraft_model::Id,
        placement: Placement,
        lang: Lang,
    ) -> Result<furnicraft_model::Id, String> {
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
