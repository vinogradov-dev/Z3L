//! The contour booleans' arm: which operation waits, the base ring it acts on, and the rings the
//! selection adds beside it.
//!
//! A boolean takes two or more drawn rings, so the operation is a mode rather than a button that
//! commits: the base is the ring that was selected when the cell was pressed, every other selected
//! ring joins as an operand, and the result draws in place of the base through the session overlay
//! while the person keeps picking (ADR 0093). The ✓ is one command (ADR 0194); escaping writes
//! nothing and costs no undo entry.
//!
//! The rings come from the selection, so a plain click and a Shift-click pick operands with the
//! habit every other selection already has. A ring that is not a closed path never joins: a panel is
//! not an operand and neither is an open chain or a solid, and the preview's own refusal says which
//! ring failed.
//!
//! Changes when: a contour boolean stops being a mode, or its picks stop being the selection.

use engine::{ContourBoolean, ContourBooleanPlan, ContourOp, Session};
use model::{Id, Node, Project};
use text::{Lang, Message};

use super::Editor;
use crate::command::{dispatch, json};

/// The armed contour boolean: its operation, its base ring and what the last preview was built from.
#[derive(Clone, Debug, PartialEq)]
pub struct BooleanArm {
    op: ContourOp,
    base: Id,
    /// The operands the preview was built from, in selection order.
    operands: Vec<Id>,
    /// The document revision the preview was built at, so an edited ring re-derives the result.
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

    /// The ring the result is born in and replaces on screen.
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

impl Editor {
    /// The armed contour boolean, when one owns the picks.
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
        if !is_ring(project, base) {
            return false;
        }
        self.leave_pointer_modes();
        self.boolean = Some(BooleanArm {
            op,
            base,
            operands: Vec::new(),
            revision: project.revision().get(),
            previewed: false,
            refusal: None,
        });
        true
    }

    /// Drops the arm and the preview it drew; nothing was written, so undo is untouched.
    pub fn cancel_boolean(&mut self, session: &mut Session) {
        self.boolean = None;
        session.clear_preview();
    }

    /// Rebuilds the preview from the selection, and says whether anything changed.
    ///
    /// The picks are the selection: the base plus every other selected closed ring, in click order.
    /// The plan is recomputed when the picks or the document move, so an operand that is edited,
    /// turned or moved re-derives the result the frame shows. A refusal keeps the last honest
    /// picture off the screen and says why, because a preview that lies is worse than no preview.
    pub fn update_boolean(&mut self, session: &mut Session) -> bool {
        let Some(arm) = self.boolean.as_ref() else {
            return false;
        };
        let (base, op, built_at) = (arm.base, arm.op, arm.revision);
        let project = session.project();
        let operands: Vec<Id> =
            self.selection.ids().iter().copied().filter(|id| *id != base && is_ring(project, *id)).collect();
        let revision = project.revision().get();
        if operands == arm.operands && revision == built_at {
            return false;
        }
        let built = if operands.is_empty() {
            Err(None)
        } else {
            let request = ContourBoolean { base, operands: operands.clone(), name: String::new() };
            match ContourBooleanPlan::new(project, &request, op).and_then(|plan| plan.geometry()) {
                Ok(geometry) => Ok(geometry),
                Err(error) => Err(Some(error.to_string())),
            }
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
    /// The name is the operation's word and the first free number, because a boolean may run twice
    /// on one drawing and two objects with one name are a puzzle for the tree.
    pub fn commit_boolean(&mut self, session: &mut Session, lang: Lang) -> Result<Id, String> {
        let Some(arm) = self.boolean.clone() else {
            return Err(Message::BooleanNeedsRing.text(lang).to_owned());
        };
        if !arm.is_ready() {
            return Err(arm.refusal.unwrap_or_else(|| Message::BooleanNeedsRing.text(lang).to_owned()));
        }
        let name = free_name(session.project(), arm.label().text(lang));
        let request = ContourBoolean { base: arm.base, operands: arm.operands.clone(), name };
        let outcome = dispatch(session, arm.op.command(), &json(&request)?)?;
        session.clear_preview();
        self.boolean = None;
        let created = outcome.created_id();
        self.select_created(created);
        created.ok_or_else(|| Message::BooleanNeedsRing.text(lang).to_owned())
    }
}

/// Whether the node is a closed ring a boolean can read.
fn is_ring(project: &Project, id: Id) -> bool {
    project.object_path(id).is_ok_and(|path| path.is_closed() && path.to_contour().is_ok())
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
