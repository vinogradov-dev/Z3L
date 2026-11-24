//! The by-points tool: the board a maker draws by naming points in space (U126).
//!
//! The arm owns the mode and the one command: the draft in [`crate::panel_points`] collects the
//! points, and [`Editor::commit_points`] turns the figure they name into one `panel.create` — a
//! standard rectangle or a contour board, with the class its plane implies. The shell resolves each
//! point through the snap resolver, so no point of this tool comes from a work plane; that is the
//! whole difference from the contour cell, whose plane is chosen before the first click.
//!
//! Nothing is written before the commit: arming, feeding a point and dropping the draft are view
//! state, so Escape costs no undo entry and a refused commit keeps the points editable.
//!
//! Changes when: the by-points gesture gains a method, a refusal or a second command.

use engine::Session;
use geom::DVec3;
use model::{ClassCode, Id, Panel, PanelOutline, role_of_plane};
use text::{Lang, Message};
use units::Length;

use super::Editor;
use crate::editor::PanelShape;
use crate::panel_points::{PanelPoints, PointsDraft, PointsDraftError, PointsFigure};

impl Editor {
    /// Arms the by-points tool with one of its two methods.
    ///
    /// Starting it leaves every other pointer mode, because two armed modes at once is a defect; the
    /// tool wrote nothing, so leaving a draft costs no undo entry (ADR 0076, 0122).
    pub fn start_points(&mut self, method: PanelPoints) {
        self.leave_pointer_modes();
        self.points = Some(PointsDraft::new(method));
    }

    /// The board being drawn, or `None` when the tool is not armed.
    pub fn points(&self) -> Option<&PointsDraft> {
        self.points.as_ref()
    }

    /// The draft, for the parts of the gesture that change it — its class and its undo.
    pub fn points_mut(&mut self) -> Option<&mut PointsDraft> {
        self.points.as_mut()
    }

    /// Places one snapped point, or refuses it with the sentence the card shows.
    pub fn points_push(&mut self, point: [Length; 3]) -> Result<(), PointsDraftError> {
        self.points
            .as_mut()
            .ok_or(PointsDraftError::Points(model::PointsError::NotEnoughPoints { got: 0 }))?
            .push(point)
    }

    /// Chooses the class the board will carry: `None` gives the role back to its plane.
    ///
    /// A menu key overrides the derived role before the click (ADR 0239), so three points on a
    /// leaned side give a post without the maker having to correct it afterwards.
    pub fn set_points_class(&mut self, class: Option<ClassCode>) {
        if let Some(draft) = self.points.as_mut() {
            draft.set_class(class);
        }
    }

    /// Drops the draft and the tool; the document never changed.
    pub fn cancel_points(&mut self) {
        self.points = None;
    }

    /// Commits the points as one board: one `panel.create`, one undo entry.
    ///
    /// The rectangle is stored as the nominal rectangle the board is, so it is a standard panel —
    /// the one way this program makes a standard board out of a plane it was given. The ring is
    /// stored as the outline it is, and the model anchors both at their base corner (ADR 0042,
    /// 0083, 0238). The class is the maker's choice when there is one and the plane's role
    /// otherwise; a figure the document rejects leaves the draft live and reports its sentence.
    pub fn commit_points(&mut self, session: &mut Session, lang: Lang) -> Result<Id, String> {
        let draft = self.points.clone().ok_or_else(|| Message::PanelPointsNeedThree.text(lang).to_owned())?;
        if !draft.is_finished() {
            return Err(Message::PanelPointsNeedThree.text(lang).to_owned());
        }
        let figure = draft.figure().map_err(|error| error.message().text(lang).to_owned())?;
        let parent = self.creation_target(session.project());
        let (frame, size, outline) = match figure {
            PointsFigure::Rect { plane, size } => (plane.placement().map_err(|error| error.to_string())?, size, None),
            PointsFigure::Ring { plane, contour } => {
                let anchored = Panel::anchored_contour(&contour)
                    .map_err(|_| Message::PanelPointsUnusable.text(lang).to_owned())?;
                let size =
                    Panel::contour_size(&anchored).map_err(|_| Message::PanelPointsUnusable.text(lang).to_owned())?;
                (plane.placement().map_err(|error| error.to_string())?, size, Some(PanelOutline::Contour { contour }))
            }
        };
        let placement = session.project().placement_in_parent(parent, frame).map_err(|error| error.to_string())?;
        // The role is measured in the parent's frame, because that is the frame the class rules
        // speak about; a maker's choice wins over the plane's own answer.
        let class = draft.class().or_else(|| role_of_plane(placement.transform_vector(DVec3::Z)));
        let id = self.create_panel_sized(
            session,
            parent,
            Message::ToolPanel.text(lang).to_owned(),
            size,
            PanelShape { outline, ..Default::default() },
            placement,
            class,
        )?;
        self.points = None;
        Ok(id)
    }
}
