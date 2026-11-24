//! The ribbon: its open state, its active tab, the tool buttons and the actions they stand for.
//!
//! The ribbon draws from one `RibbonContext` and reports what it asked for; the drawing, the
//! creation and the extrusion the buttons start are commands, while the view actions and the close
//! of a draft belong to the shell that owns the view camera and the work plane (ADR 0052). The side
//! count and the extrude height are typed session defaults, so the last value a tool used carries
//! into the next one; a primitive's size is drawn rather than typed (ADR 0139).
//!
//! Refuses a panel or an assembly creation without a catalogue entry and an extrusion of an open
//! path; a refused action is the command's sentence and leaves the default untouched.
//!
//! Changes when: a tool appears on the ribbon, or a button's action changes.

use egui::Ui;

use engine::object_cmds::{OBJECT_EXTRUDE_HEIGHT_SET_ID, OBJECT_PATH_EXTRUDE_ID};
use engine::{EditScope, ObjectExtrudeHeightSet, ObjectPathExtrude, Session};
use model::{Id, Path, PathError, Project};
use text::{Lang, Message};
use units::Length;

use super::{BooleanOperand, Editor};
use crate::command::{dispatch, json};
use crate::toolbars::{self, ToolAction};

impl Editor {
    /// Draws the ribbon: the row of tabs and the tool groups of the active tab.
    ///
    /// The open body, the active tab and the typed defaults are view state owned by the editor, so
    /// they survive a selection change and a document command and never reach the scene file. A
    /// creation tool lands at the scene root unless a container was armed through an explicit entry
    /// (ADR 0056). The tool that owns the next click draws pressed, so the ribbon never hides what
    /// the pointer will do. The view actions it returns — frame all, the projection, the grid snap —
    /// belong to the shell, which owns the view camera and the drawing gizmo. So does the *Close* of
    /// a draft: the ribbon reports it, because closing owns the work plane, the gizmo state and the
    /// scene rebuild (ADR 0052).
    pub fn show_ribbon(
        &mut self,
        ui: &mut Ui,
        session: &mut Session,
        view: toolbars::RibbonView,
    ) -> Result<toolbars::RibbonOutcome, String> {
        self.retain_creation_parent(session.project());
        let has_panel_material = !session.directory().boards().collect::<Vec<_>>().is_empty();
        let selected = self.selection.primary();
        let selected_path = selected.and_then(|id| session.project().object_path(id).ok()).map(Path::is_closed);
        let selected_ring = selected.is_some_and(|id| engine::is_contour_operand(session.project(), id));
        // A drawing of several rings encloses areas rather than one ring, so the Extrude cell arms
        // the area pull instead of extruding a profile (G4).
        let selected_drawing = selected.is_some_and(|id| {
            session.project().object(id).is_some_and(|object| object.geometry().as_contours().is_some())
        });
        // A body boolean takes a solid: a drawn body or a primitive, either of which a step extends
        // and either of which a step may freeze as its tool. A baked mesh has no history (W1).
        let selected_body = selected.is_some_and(|id| {
            session.project().object(id).is_some_and(|object| {
                let geometry = object.geometry();
                geometry.as_constructive().is_some() || geometry.as_primitive().is_some()
            })
        });
        let active = toolbars::tool_activity(
            self.drawing.as_ref().map(|draft| (draft.tool(), self.drawing_intent)),
            self.box_draft().is_some(),
            self.fill_arm().map(|arm| (arm.class, arm.mode)),
            self.measure().is_some(),
            self.area_pull().is_some(),
            self.boolean_arm().map(|arm| (arm.kind(), arm.op())),
        );
        let draft = self.drawing.as_ref().map(|draft| (draft.tool(), draft.len(), draft.is_closed()));
        let context = toolbars::RibbonContext {
            has_panel_material,
            product_open: session.active_product().is_some(),
            selected,
            selected_path,
            selected_ring,
            selected_drawing,
            selected_body,
            active,
            perspective: view.perspective,
            owns_view: view.owns_view,
            snaps: view.snaps,
            snap_mode: view.snap_mode,
            shadows: view.shadows,
            draft,
            build: self.panel_build(),
        };
        let mut actions = Vec::new();
        let list_rect = toolbars::show(ui, &mut self.toolbars, context, &mut actions);
        let mut outcome = self.apply_ribbon_actions(session, actions, crate::language(ui.ctx()))?;
        outcome.list_rect = list_rect;
        Ok(outcome)
    }

    /// Runs the actions one drawn ribbon asked for and reports what the shell must still do.
    ///
    /// Split from [`Self::show_ribbon`] so the mapping from a clicked tool to its effect is testable
    /// without a window: the *Close* of a draft is reported as [`toolbars::RibbonOutcome::close_draft`]
    /// rather than run, because only the shell owns the work plane and the gizmo state.
    pub(super) fn apply_ribbon_actions(
        &mut self,
        session: &mut Session,
        actions: Vec<ToolAction>,
        lang: Lang,
    ) -> Result<toolbars::RibbonOutcome, String> {
        let target = self.creation_target(session.project());
        let mut outcome = toolbars::RibbonOutcome::default();
        for action in actions {
            match action {
                ToolAction::Draw { tool, sides } => {
                    self.start_drawing_with(tool, sides);
                    outcome.started_drawing = true;
                }
                ToolAction::DrawBody { tool } => {
                    self.start_body_drawing(tool);
                    outcome.started_drawing = true;
                }
                ToolAction::DrawPrimitive { tool, .. } => {
                    self.start_primitive_drawing(tool)?;
                    outcome.started_drawing = true;
                }
                ToolAction::CreateProductCard => outcome.open_product_card = true,
                ToolAction::DrawProductFrame { tool } => {
                    self.start_product_box_drawing(tool);
                    outcome.started_drawing = true;
                }
                ToolAction::DrawProductFrameContour => {
                    self.start_product_drawing();
                    outcome.started_drawing = true;
                }
                ToolAction::DrawPanelContour => {
                    self.start_panel_drawing();
                    outcome.started_drawing = true;
                }
                ToolAction::Measure => {
                    // A second press drops the ruler, like the fill arm: the mode owns the next
                    // clicks until the person leaves it, and Escape is not the only way out.
                    if self.measure().is_some() {
                        self.cancel_measure();
                    } else {
                        self.arm_measure();
                    }
                }
                ToolAction::ContourBoolean(op) => {
                    // The three cells arm one mode over the selection: the base ring is the one
                    // selected now, and every ring picked afterwards is an operand. A second press
                    // on the same operation drops the mode, like the ruler and the niche (G3).
                    if self.boolean_arm().is_some_and(|arm| arm.op() == op) {
                        self.cancel_boolean(session);
                    } else if !self.arm_boolean(op, session.project()) {
                        return Err(Message::BooleanNeedsRing.text(lang).to_owned());
                    }
                }
                ToolAction::BodyBoolean(op) => {
                    // The three Model cells arm one mode over the selection: the target is the body
                    // selected now and the tool is the next body picked. A second press on the same
                    // operation drops the mode, like every other armed tool (W1).
                    if self.boolean_arm().is_some_and(|arm| arm.op() == op && arm.kind() == BooleanOperand::Bodies) {
                        self.cancel_boolean(session);
                    } else if !self.arm_body_boolean(op, session.project()) {
                        return Err(Message::BooleanNeedsBody.text(lang).to_owned());
                    }
                }
                ToolAction::Fill { class, mode } => {
                    // A second click on the same build drops the tool; the arm is a mode, not a
                    // one-shot, because a maker places several posts in one piece. Pressing the cell
                    // with another class or build chosen on the card arms that one instead of
                    // dropping the tool, and the card's choice is remembered for the next click.
                    self.set_panel_build(crate::editor::PanelBuild { class, mode, ..self.panel_build() });
                    if self.fill_arm().is_some_and(|arm| arm.class == class && arm.mode == mode) {
                        self.cancel_fill();
                    } else {
                        self.arm_fill(session, class, mode, lang)?;
                    }
                }
                ToolAction::CreateAssembly => self.create_assembly(session, target)?,
                ToolAction::CreateCamera => self.create_camera(session, target)?,
                ToolAction::CreateLight => self.create_light(session, target)?,
                ToolAction::AreaPull { object } => self.arm_area_pull(object),
                ToolAction::Extrude { object, height } => {
                    self.extrude(session, object, height, lang)?;
                    outcome.extruded = Some((object, height));
                }
                ToolAction::CloseDrawing => outcome.close_draft = true,
                ToolAction::CancelDrawing => self.cancel_drawing(),
                ToolAction::FrameAll => outcome.view = Some(toolbars::RibbonViewAction::FrameAll),
                ToolAction::ToggleProjection => {
                    outcome.view = Some(toolbars::RibbonViewAction::ToggleProjection);
                }
                ToolAction::ToggleSnap(kind) => {
                    outcome.view = Some(toolbars::RibbonViewAction::ToggleSnap(kind));
                }
                ToolAction::ToggleSnapAll => {
                    outcome.view = Some(toolbars::RibbonViewAction::ToggleSnapAll);
                }
                ToolAction::ToggleSnapMode => {
                    outcome.view = Some(toolbars::RibbonViewAction::ToggleSnapMode);
                }
                ToolAction::ToggleShadows => outcome.view = Some(toolbars::RibbonViewAction::ToggleShadows),
            }
        }
        Ok(outcome)
    }

    /// The regular polygon side count a new polygon starts with.
    ///
    /// The key handler starts a polygon with the same count the ribbon would, so a keyboard
    /// shortcut and a button click never build different figures.
    pub fn polygon_sides(&self) -> u32 {
        self.toolbars.polygon_sides()
    }

    /// Turns a selected drawing into the bodies it encloses.
    ///
    /// A closed path becomes one body in place; a contour set becomes one body per ring, and the
    /// node keeps the first of them (X100). An open chain encloses no ring, so it is refused with
    /// the reason from `Path::to_contour` before a command runs. The node keeps its id, name,
    /// placement and pivot: the drawing is not left behind as a second object, and the selection
    /// stays on the one node the user was editing. A live contour edit of that node ends here: the
    /// node is a body now, and its pull arrow would compete with the corner grips for the pointer.
    pub(super) fn extrude(
        &mut self,
        session: &mut Session,
        object: Id,
        height: Length,
        lang: Lang,
    ) -> Result<(), String> {
        self.extrude_in(None, session, object, height, lang)
    }

    /// Turns a drawn profile into its body as one step of an open edit scope (ADR 0138).
    ///
    /// The body a cell drew goes through the same command, in the scope its profile was created in,
    /// so the ring and the body are one undo entry. `None` is the ribbon's plain extrusion, which is
    /// its own entry.
    pub fn extrude_in(
        &mut self,
        scope: Option<EditScope>,
        session: &mut Session,
        object: Id,
        height: Length,
        lang: Lang,
    ) -> Result<(), String> {
        drawing_ready(session.project(), object).map_err(|error| error.with(lang))?;
        let params = json(&ObjectPathExtrude { object, height })?;
        match scope {
            Some(scope) => {
                session.dispatch_in(scope, OBJECT_PATH_EXTRUDE_ID, &params).map_err(|error| error.to_string())?
            }
            None => dispatch(session, OBJECT_PATH_EXTRUDE_ID, &params)?,
        };
        self.end_contour_edit();
        self.select_node(Some(object));
        Ok(())
    }

    /// Sets one body's extrusion height and remembers it as the next default.
    ///
    /// The height is a typed session default, not document data: it carries into the next
    /// extrusion, while the document only ever sees the command. A refused height leaves both the
    /// document and the default untouched.
    pub fn set_extrude_height(&mut self, session: &mut Session, object: Id, height: Length) -> Result<(), String> {
        self.set_extrude_height_in(None, session, object, height)
    }

    /// Sets one body's extrusion height as a step of an open edit scope (ADR 0138).
    ///
    /// A pull that created its body already shares the scope, so the height the ✓ writes joins the
    /// same undo entry instead of starting a second one.
    pub fn set_extrude_height_in(
        &mut self,
        scope: Option<EditScope>,
        session: &mut Session,
        object: Id,
        height: Length,
    ) -> Result<(), String> {
        let params = json(&ObjectExtrudeHeightSet { object, height })?;
        match scope {
            Some(scope) => {
                session.dispatch_in(scope, OBJECT_EXTRUDE_HEIGHT_SET_ID, &params).map_err(|error| error.to_string())?
            }
            None => dispatch(session, OBJECT_EXTRUDE_HEIGHT_SET_ID, &params)?,
        };
        self.toolbars.set_extrude_height(height);
        Ok(())
    }

    /// The height the ribbon will hand the next extrusion.
    pub const fn extrude_height(&self) -> Length {
        self.toolbars.extrude_height()
    }

    /// Whether the ribbon's body is open.
    pub fn ribbon_open(&self) -> bool {
        self.toolbars.is_open()
    }

    /// Opens or closes the ribbon's body and returns its new state.
    pub fn set_ribbon_open(&mut self, open: bool) -> bool {
        self.toolbars.set_open(open)
    }

    /// The tab whose groups the ribbon draws.
    pub fn ribbon_tab(&self) -> &'static toolbars::spec::TabSpec {
        self.toolbars.tab()
    }

    /// Opens one ribbon tab.
    pub fn set_ribbon_tab(&mut self, tab: &'static toolbars::spec::TabSpec) -> &'static toolbars::spec::TabSpec {
        self.toolbars.set_tab(tab)
    }

    /// Whether a ribbon variant list is open, so the shell can leave Escape and Enter to it.
    ///
    /// The ribbon draws after the shell reads the keys, so the shell asks the previous frame's
    /// state: a list that was open when the key was pressed owns that key (U67).
    pub fn variant_list_open(&self) -> bool {
        self.toolbars.list().is_some()
    }

    /// The methods the workspace pinned, by tool id.
    ///
    /// A pin is a workspace setting, not scene data: the editor holds the copy the shell handed it
    /// when the tab opened, and the shell writes it back when it changes (U67).
    pub fn pinned_variants(&self) -> &std::collections::BTreeMap<String, String> {
        self.toolbars.pinned_variants()
    }

    /// Replaces the pinned methods, from the settings file or from the shell's own sync.
    pub fn set_pinned_variants(&mut self, pinned: std::collections::BTreeMap<String, String>) {
        self.toolbars.set_pinned_variants(pinned);
    }

    /// Starts the drawing tool a key names, honouring the method pinned for that tool.
    ///
    /// The key table names drawing tools, and a tool with a list has one key: pressing it runs the
    /// pinned method without opening the list, so the ribbon stays quiet while the keyboard works
    /// (U67).
    pub fn start_shortcut(&mut self, tool: crate::draw::DrawTool, sides: u32) {
        let tool = toolbars::shortcut_method(tool, self.toolbars.pinned_variants());
        self.start_drawing_with(tool, sides);
    }
}

/// Why a drawing cannot become a body.
///
/// The two refusals the press can meet before a command runs: a node that carries no drawing at
/// all, and a ring that does not close. Both become the sentence the ribbon shows, so the
/// keyboard, the cell and the shell refuse in one voice (X100).
enum DrawingRefusal {
    /// The node is not a path or a contour set.
    NotADrawing,
    /// A chain of the drawing does not enclose a ring.
    Open(PathError),
}

impl DrawingRefusal {
    /// The refusal in the user's language.
    fn with(self, lang: Lang) -> String {
        match self {
            Self::NotADrawing => Message::ExtrudeNotPath.text(lang).to_owned(),
            Self::Open(error) => Message::ExtrudeRefused.with(lang, [error]),
        }
    }
}

/// Checks that a node is a drawing whose every chain closes, without changing the document.
///
/// A contour set holds rings by construction, so it is ready as it stands; a path is checked
/// through the ring rules, which is where an open chain or a self-crossing drawing is refused.
fn drawing_ready(project: &Project, object: Id) -> Result<(), DrawingRefusal> {
    let node = project.object(object).ok_or(DrawingRefusal::NotADrawing)?;
    let geometry = node.geometry();
    if let Some(path) = geometry.as_path() {
        return path.to_contour().map(|_| ()).map_err(DrawingRefusal::Open);
    }
    if geometry.as_contours().is_some() {
        return Ok(());
    }
    Err(DrawingRefusal::NotADrawing)
}
