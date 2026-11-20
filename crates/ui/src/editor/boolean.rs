//! The booleans' one arm: which operation waits, the target it acts on, and what was picked beside
//! it.
//!
//! A boolean takes two operands, so the operation is a mode rather than a button that commits: the
//! target is the node that was selected when the cell was pressed, every other picked node joins as
//! the second operand, and the result draws in place of the target through the session overlay while
//! the person keeps picking (ADR 0093). The ✓ is one command (ADR 0194); escaping writes nothing and
//! costs no undo entry.
//!
//! Two operand kinds share the arm, because the gesture, the overlay, the ✓ and the Escape are the
//! same and only what the command stores differs. A **ring** boolean combines drawn figures and
//! leaves one new drawing where the base was (G3). A **body** boolean appends a step to the target
//! body's own history and leaves the tool body in the tree (W1): the cutter is not spent by the
//! operation and may cut several bodies.
//!
//! The operands come from the selection, so a plain click and a Shift-click pick with the habit
//! every other selection already has. Only a node of the arm's kind joins: for rings a path object
//! or a body's own profile (X98), for bodies a constructive body and nothing else.
//!
//! Changes when: a boolean stops being a mode, its picks stop being the selection, or the two kinds
//! stop sharing a gesture.

use engine::{ContourBoolean, ContourBooleanPlan, ContourOp, OBJECT_BOOLEAN_SET_ID, ObjectBooleanSet, Session};
use model::{BooleanOp, Constructive, Geometry, Id, Node, Project};
use text::{Lang, Message};

use super::Editor;
use crate::command::{dispatch, json};

/// What one boolean combines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BooleanOperand {
    /// Closed rings: the result is a new drawing of the combined figures (G3).
    Rings,
    /// Solid bodies: the step is appended to the target body's history, so the target is edited and
    /// the tool stays in the tree (W1).
    Bodies,
}

impl BooleanOperand {
    /// The words that name what the person has to pick.
    pub const fn picks(self) -> Message {
        match self {
            Self::Rings => Message::BooleanPick,
            Self::Bodies => Message::BooleanPickBody,
        }
    }

    /// The words that name what the arm is waiting for when nothing is picked yet.
    pub const fn needs(self) -> Message {
        match self {
            Self::Rings => Message::BooleanNeedsRing,
            Self::Bodies => Message::BooleanNeedsBody,
        }
    }
}

/// The armed boolean: its operands' kind, its operation, its target and what the last preview was
/// built from.
#[derive(Clone, Debug, PartialEq)]
pub struct BooleanArm {
    kind: BooleanOperand,
    op: ContourOp,
    base: Id,
    /// The operands the preview was built from, in selection order.
    operands: Vec<Id>,
    /// The document revision the preview was built at, so an edited operand re-derives the result.
    revision: u64,
    /// Whether a preview is on screen.
    previewed: bool,
    /// Why the last attempt drew nothing, in words; `None` when the result is on screen.
    refusal: Option<String>,
}

impl BooleanArm {
    /// The operation the next ✓ runs.
    pub const fn op(&self) -> ContourOp {
        self.op
    }

    /// What the arm combines.
    pub const fn kind(&self) -> BooleanOperand {
        self.kind
    }

    /// The node the result is drawn in place of: the ring the drawing is born at, or the body the
    /// step is appended to.
    pub const fn base(&self) -> Id {
        self.base
    }

    /// The operands the preview was built from.
    pub fn operands(&self) -> &[Id] {
        &self.operands
    }

    /// Whether a result is on screen and the ✓ can store it.
    pub const fn is_ready(&self) -> bool {
        self.previewed
    }

    /// Why the last attempt drew nothing, or `None` when the result is on screen.
    pub fn refusal(&self) -> Option<&str> {
        self.refusal.as_deref()
    }

    /// The words that name the operation.
    pub const fn label(&self) -> Message {
        match self.op {
            ContourOp::Union => Message::ToolUnion,
            ContourOp::Subtract => Message::ToolSubtract,
            ContourOp::Intersect => Message::ToolIntersect,
        }
    }
}

/// The document operation one armed operation means.
///
/// The two triples are one set of operations described by two crates: `ContourOp` names what a
/// drawing's boolean stores, `BooleanOp` what a body's feature history stores.
pub const fn boolean_op(op: ContourOp) -> BooleanOp {
    match op {
        ContourOp::Union => BooleanOp::Union,
        ContourOp::Subtract => BooleanOp::Difference,
        ContourOp::Intersect => BooleanOp::Intersection,
    }
}

impl Editor {
    /// The armed boolean, when one owns the picks.
    pub fn boolean_arm(&self) -> Option<&BooleanArm> {
        self.boolean.as_ref()
    }

    /// Arms `op` on the one selected ring, and says whether it could.
    ///
    /// A boolean needs a base ring, so the cell is off without one: the ring under the primary
    /// selection is the frame the result is born in, and everything picked afterwards is an operand.
    pub fn arm_boolean(&mut self, op: ContourOp, project: &Project) -> bool {
        let Some(base) = self.selection.primary() else {
            return false;
        };
        if !engine::is_contour_operand(project, base) {
            return false;
        }
        self.arm(BooleanOperand::Rings, op, base, project);
        true
    }

    /// Arms `op` on the one selected body, and says whether it could.
    ///
    /// The target body must be constructive: a baked mesh has no history to append a step to, and a
    /// primitive is not a boolean body yet. The tool is picked next, so the arm starts with none.
    pub fn arm_body_boolean(&mut self, op: ContourOp, project: &Project) -> bool {
        let Some(base) = self.selection.primary() else {
            return false;
        };
        if !is_body(project, base) {
            return false;
        }
        self.arm(BooleanOperand::Bodies, op, base, project);
        true
    }

    /// Arms one operand kind on `base`, dropping every other mode the pointer could be in.
    fn arm(&mut self, kind: BooleanOperand, op: ContourOp, base: Id, project: &Project) {
        self.leave_pointer_modes();
        self.boolean = Some(BooleanArm {
            kind,
            op,
            base,
            operands: Vec::new(),
            revision: project.revision().get(),
            previewed: false,
            refusal: None,
        });
    }

    /// Drops the arm and the preview it drew; nothing was written, so undo is untouched.
    pub fn cancel_boolean(&mut self, session: &mut Session) {
        self.boolean = None;
        session.clear_preview();
    }

    /// Rebuilds the preview from the selection, and says whether anything changed.
    ///
    /// The picks are the selection: the target plus every other picked operand, in click order. The
    /// plan is recomputed when the picks or the document move, so an operand that is edited, turned
    /// or moved re-derives the result the frame shows. A refusal keeps the last honest picture off
    /// the screen and says why, because a preview that lies is worse than no preview.
    pub fn update_boolean(&mut self, session: &mut Session) -> bool {
        let Some(arm) = self.boolean.as_ref() else {
            return false;
        };
        let (base, kind, op, built_at) = (arm.base, arm.kind, arm.op, arm.revision);
        let project = session.project();
        let operands: Vec<Id> = self
            .selection
            .ids()
            .iter()
            .copied()
            .filter(|id| *id != base)
            .filter(|id| match kind {
                BooleanOperand::Rings => engine::is_contour_operand(project, *id),
                BooleanOperand::Bodies => is_body(project, *id),
            })
            .collect();
        let revision = project.revision().get();
        if operands == arm.operands && revision == built_at {
            return false;
        }
        let built = match (kind, operands.first()) {
            (_, None) => Err(None),
            (BooleanOperand::Rings, Some(_)) => {
                let request = ContourBoolean { base, operands: operands.clone(), name: String::new() };
                match ContourBooleanPlan::new(project, &request, op).and_then(|plan| plan.geometry()) {
                    Ok(geometry) => Ok(geometry),
                    Err(error) => Err(Some(error.to_string())),
                }
            }
            (BooleanOperand::Bodies, Some(tool)) => match body_of(project, base) {
                // A target that stopped being a constructive body — collapsed while the arm was
                // open — draws nothing and says nothing extra: the status bar already asks for the
                // operands the arm needs.
                None => Err(None),
                Some(body) => match boolean_preview(project, body, base, *tool, op) {
                    Ok(geometry) => Ok(geometry),
                    Err(reason) => Err(Some(reason)),
                },
            },
        };
        let (previewed, refusal) = match built {
            Ok(geometry) => {
                session.set_preview_geometry(base, geometry);
                (true, None)
            }
            Err(reason) => {
                session.clear_preview();
                (false, reason)
            }
        };
        if let Some(arm) = self.boolean.as_mut() {
            arm.operands = operands;
            arm.revision = revision;
            arm.previewed = previewed;
            arm.refusal = refusal;
        }
        true
    }

    /// Stores the previewed result as one command and leaves the mode.
    ///
    /// A ring boolean creates a drawing, so its name is the operation's word and the first free
    /// number; a body boolean appends a step to a body that already has a name, so it creates
    /// nothing and the selection stays on the target it edited.
    pub fn commit_boolean(&mut self, session: &mut Session, lang: Lang) -> Result<Id, String> {
        let Some(arm) = self.boolean.clone() else {
            return Err(Message::BooleanNeedsRing.text(lang).to_owned());
        };
        if !arm.is_ready() {
            return Err(arm.refusal.unwrap_or_else(|| arm.kind.needs().text(lang).to_owned()));
        }
        let created = match arm.kind {
            BooleanOperand::Rings => {
                let name = free_name(session.project(), arm.label().text(lang));
                let request = ContourBoolean { base: arm.base, operands: arm.operands.clone(), name };
                dispatch(session, arm.op.command(), &json(&request)?)?.created_id()
            }
            BooleanOperand::Bodies => {
                let Some(tool) = arm.operands.first().copied() else {
                    return Err(Message::BooleanNeedsBody.text(lang).to_owned());
                };
                let request = ObjectBooleanSet { object: arm.base, op: boolean_op(arm.op), tool };
                dispatch(session, OBJECT_BOOLEAN_SET_ID, &json(&request)?)?;
                Some(arm.base)
            }
        };
        session.clear_preview();
        self.boolean = None;
        self.select_created(created);
        created.ok_or_else(|| arm.kind.needs().text(lang).to_owned())
    }
}

/// Whether `id` is a constructive body a boolean may act on or take as its tool.
fn is_body(project: &Project, id: Id) -> bool {
    body_of(project, id).is_some()
}

/// The constructive payload of one body, when the node carries one.
fn body_of(project: &Project, id: Id) -> Option<&Constructive> {
    project.object(id).and_then(|object| object.geometry().as_constructive())
}

/// The geometry the target would have with one step appended, or why it would draw nothing.
///
/// The step is checked with the same rule the command applies, so the arm, the preview and the ✓
/// answer one question. The candidate is a copy: the document keeps its own history until the ✓.
fn boolean_preview(
    project: &Project,
    body: &Constructive,
    target: Id,
    tool: Id,
    op: ContourOp,
) -> Result<Geometry, String> {
    project.validate_boolean_tool(target, tool).map_err(|error| error.to_string())?;
    let mut candidate = body.clone();
    candidate.push_boolean(model::Boolean::new(boolean_op(op), tool)).map_err(|error| error.to_string())?;
    project.boolean_tools_of(target, &candidate).map_err(|error| error.to_string())?;
    Ok(Geometry::Constructive(candidate))
}

/// `word` with the first free number, so a second boolean of one drawing does not collide.
fn free_name(project: &Project, word: &str) -> String {
    let mut number = 1u32;
    loop {
        let candidate = format!("{word} {number}");
        if !name_taken(project.root().children(), &candidate, 1) {
            return candidate;
        }
        number = number.saturating_add(1);
        if number > 10_000 {
            return candidate;
        }
    }
}

/// Whether any object in the tree already carries `candidate`, searched to the depth limit.
fn name_taken(children: &[Node], candidate: &str, depth: u32) -> bool {
    if depth > Project::MAX_ASSEMBLY_DEPTH {
        return false;
    }
    for child in children {
        if let Node::Object(object) = child
            && object.name() == candidate
        {
            return true;
        }
        if let Some(kids) = child.children()
            && name_taken(kids, candidate, depth.saturating_add(1))
        {
            return true;
        }
    }
    false
}
