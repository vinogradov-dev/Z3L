//! What one ribbon cell draws and runs: the one place where a map row meets the session.
//!
//! The map says *where* a tool sits and what it is called; this module says whether it can run
//! *here* — a catalogue material, a closed path, a running draft, a viewport that owns its
//! projection — and returns the sentence that says what is missing, so a disabled cell still
//! explains itself. It also holds the one rule the button and the draft agree on: when a running
//! chain can be closed.

use model::{FillMode, Id};
use text::{Lang, Message};

use super::spec::{ToolKind, ToolSpec};
use super::state::Toolbars;
use super::{RibbonContext, ToolAction, is_armed, shortcut_label, strip_label};
use crate::draw::DrawTool;
use crate::icons::{self, Icon, tool_hint};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Ready {
    /// The tool can run here.
    pub(super) enabled: bool,
    /// The button draws pressed: the tool owns the next click, or the toggle is on.
    pub(super) armed: bool,
    /// The tooltip and the accessible name, which is also the sentence of a refusal.
    pub(super) hint: String,
    /// The action a click asks for, unless the tool cannot run.
    pub(super) action: Option<ToolAction>,
}

/// Decides one tool against the picture the ribbon was handed.
///
/// This is the one place where a row's [`ToolKind`] meets the session: everything the map says about
/// *where* a tool sits is data, and everything about *whether* it can run is here, next to the words
/// that say why not.
pub(super) fn ready(kind: ToolKind, tool: &ToolSpec, state: &Toolbars, context: RibbonContext, lang: Lang) -> Ready {
    let RibbonContext {
        has_panel_material,
        product_open,
        selected,
        selected_path,
        selected_ring,
        selected_body,
        selected_primitive,
        active,
        owns_view,
        grid,
        shadows,
        draft,
        ..
    } = context;
    match kind {
        ToolKind::Draw(draw) => Ready {
            enabled: true,
            armed: is_armed(active, kind),
            hint: tool_hint(strip_label(draw).text(lang), shortcut_label(draw)),
            action: Some(ToolAction::Draw { tool: draw, sides: state.polygon_sides() }),
        },
        // A body cell draws the base with the sketch's own method and the shell pulls its height
        // afterwards, so the hint names the figure to draw rather than a click that would place it
        // (ADR 0138).
        ToolKind::Body(draw) => Ready {
            enabled: true,
            armed: is_armed(active, kind),
            hint: Message::PrimitiveDrawBase.with(lang, [tool.label.text(lang)]),
            action: Some(ToolAction::DrawBody { tool: draw }),
        },
        ToolKind::CloseDraft => {
            // Close needs a running draft that can close, cancel needs any running draft; both are
            // drawn so the pair never vanishes between clicks and the hint says why one is off.
            let (drawing, points, closed) = draft.unwrap_or((DrawTool::Line, 0, false));
            let close = draft_can_close(drawing, points, closed);
            let hint = if close { Message::DraftCloseHint.text(lang) } else { Message::DraftCloseNeedsTwo.text(lang) };
            Ready {
                enabled: close,
                armed: false,
                hint: hint.to_owned(),
                action: close.then_some(ToolAction::CloseDrawing),
            }
        }
        ToolKind::CancelDraft => {
            let running = draft.is_some();
            let hint = tool_hint(
                if running { Message::DraftCancelHint.text(lang) } else { Message::DraftNotRunning.text(lang) },
                Some("Esc"),
            );
            Ready { enabled: running, armed: false, hint, action: running.then_some(ToolAction::CancelDrawing) }
        }
        // A primitive cell draws the round the shape is built around, exactly as a body cell draws
        // its profile: the sphere's centre and radius reach the document from the plates, not from a
        // typed size and a bare click (ADR 0139).
        ToolKind::Primitive { kind: primitive, tool: draw } => Ready {
            enabled: true,
            armed: is_armed(active, kind),
            hint: Message::PrimitiveDrawRound.with(lang, [tool.label.text(lang)]),
            action: Some(ToolAction::DrawPrimitive { kind: primitive, tool: draw }),
        },
        ToolKind::Extrude => {
            // Only a closed path encloses a ring, so the button is off for anything else and the
            // hint names what is missing: a selection, a path, or a closed chain. The height is a
            // typed default, so a valid number is always there.
            let action = match (selected, selected_path) {
                (Some(object), Some(true)) => Some(ToolAction::ExtrudePath { object, height: state.extrude_height() }),
                _ => None,
            };
            Ready {
                enabled: action.is_some(),
                armed: false,
                hint: extrude_hint(selected, selected_path).text(lang).to_owned(),
                action,
            }
        }
        ToolKind::Product => Ready {
            enabled: true,
            armed: false,
            hint: tool_hint(tool.label.text(lang), tool.key()),
            action: Some(ToolAction::CreateProductCard),
        },
        // The drawn frame is the box gesture with the envelope as its owner: the same rectangle
        // method, the same plates and the same base a solid begins with (N19, ADR 0101).
        ToolKind::ProductBox(draw) => Ready {
            enabled: true,
            armed: is_armed(active, kind),
            hint: Message::PrimitiveDrawBase.with(lang, [tool.label.text(lang)]),
            action: Some(ToolAction::DrawProductFrame { tool: draw }),
        },
        ToolKind::ProductContour => Ready {
            enabled: true,
            armed: is_armed(active, kind),
            // The drawn footprint is the sketch tool with the envelope as its owner: the ring is the
            // frame's base, and its height is pulled from there (ADR 0159).
            hint: Message::ProductContourDraw.with(lang, [tool.label.text(lang)]),
            action: Some(ToolAction::DrawProductFrameContour),
        },
        ToolKind::PanelContour => {
            // A contour panel is a part of the open product (ADR 0098) and takes its thickness from a
            // catalogue material; the cell then arms the free-outline tool, whose close is the one
            // command that cuts the board (ADR 0114).
            let draw = Message::PanelContourDraw.with(lang, [tool.label.text(lang)]);
            // A object tool always names itself: the missing product disables the cell but is not
            // written in its hint, so a dim cell still says which tool it is (X46).
            let hint = if !has_panel_material { Message::FillNeedsMaterial.text(lang) } else { draw.as_str() };
            let enabled = product_open && has_panel_material;
            Ready {
                enabled,
                armed: is_armed(active, kind),
                hint: tool_hint(hint, tool.key()),
                action: enabled.then_some(ToolAction::DrawPanelContour),
            }
        }
        ToolKind::Fill { class, mode } => {
            // A niche belongs to the open product (ADR 0098) and a part needs a material to take its
            // thickness from; the class must actually carry the `fill` trait, so a table row that
            // lost it disables the cell instead of arming a tool with no axis pair.
            let fills = class.traits().and_then(|traits| traits.fill()).is_some();
            let method = match mode {
                FillMode::Fit => Message::FillMethodFit,
                FillMode::Shape => Message::FillMethodShape,
            };
            // The two methods of one cell would otherwise read the same, so a hint that can run
            // names the tool and then the method: «Post: Largest rectangle».
            let ready = Message::ToolMethodName.with(lang, [tool.label.text(lang), method.text(lang)]);
            let hint = if !has_panel_material {
                Message::FillNeedsMaterial.text(lang)
            } else if !fills {
                Message::FillNeedsFillTrait.text(lang)
            } else {
                ready.as_str()
            };
            let enabled = product_open && has_panel_material && fills;
            Ready {
                enabled,
                armed: is_armed(active, kind),
                hint: tool_hint(hint, tool.key()),
                action: enabled.then_some(ToolAction::Fill { class, mode }),
            }
        }
        ToolKind::Assembly => Ready {
            enabled: true,
            armed: false,
            hint: tool_hint(tool.label.text(lang), tool.key()),
            action: Some(ToolAction::CreateAssembly),
        },
        ToolKind::Camera => Ready {
            enabled: true,
            armed: false,
            hint: tool_hint(tool.label.text(lang), tool.key()),
            action: Some(ToolAction::CreateCamera),
        },
        ToolKind::Light => Ready {
            enabled: true,
            armed: false,
            hint: tool_hint(tool.label.text(lang), tool.key()),
            action: Some(ToolAction::CreateLight),
        },
        ToolKind::FrameAll => Ready {
            enabled: true,
            armed: false,
            hint: tool_hint(tool.label.text(lang), tool.key()),
            action: Some(ToolAction::FrameAll),
        },
        ToolKind::Projection => Ready {
            enabled: owns_view,
            armed: false,
            hint: tool_hint(Message::SwitchProjection.text(lang), tool.key()),
            action: owns_view.then_some(ToolAction::ToggleProjection),
        },
        ToolKind::Measure => Ready {
            // A measurement needs no document, no selection and no material: it is information
            // about what is already there, so the cell always runs (ADR 0118).
            enabled: true,
            armed: is_armed(active, kind),
            hint: tool_hint(tool.label.text(lang), tool.key()),
            action: Some(ToolAction::Measure),
        },
        // A contour boolean is a mode over the selection: the base ring is the one selected when
        // the cell was pressed, and every further ring picked while it is armed is an operand. The
        // cell is off until a closed ring is selected, so the hint says what to do instead of
        // arming a tool with nothing to combine (G3, ADR 0194).
        ToolKind::ContourBoolean(op) => {
            // The ring, not the path: a body's profile is the ring it grew from, so the three cells
            // enable on a body exactly as on the drawing it was extruded from (X98).
            let enabled = selected_ring;
            // A body is a solid: the cell says where bodies are cut instead of sitting dead, because
            // a body's profile is not a drawing to combine (W2, ADR 0194 revised).
            let hint = if enabled || !selected_body {
                tool_hint(tool.label.text(lang), tool.key())
            } else {
                Message::ContourBooleanBodiesOnModel.text(lang).to_owned()
            };
            Ready {
                enabled,
                armed: is_armed(active, kind),
                hint,
                action: enabled.then_some(ToolAction::ContourBoolean(op)),
            }
        }
        // A body boolean is a mode over one selected body: the target is the body selected when the
        // cell was pressed and the tool is the next body picked. The cell is off until a
        // constructive body is selected, so the hint says what to do instead of arming a tool with
        // nothing to combine (W1).
        ToolKind::BodyBoolean(op) => {
            let enabled = selected_body;
            // A primitive is a solid but not a body this boolean can cut with yet: the cell says
            // what it waits for instead of sitting there dead (W1, ADR 0197).
            let hint = if enabled || !selected_primitive {
                tool_hint(tool.label.text(lang), tool.key())
            } else {
                Message::BooleanNeedsBody.text(lang).to_owned()
            };
            Ready {
                enabled,
                armed: is_armed(active, kind),
                hint,
                action: enabled.then_some(ToolAction::BodyBoolean(op)),
            }
        }
        ToolKind::Void => Ready {
            // A niche belongs to the open product (ADR 0098), and naming it needs nothing else: no
            // material, no selection and no document write (ADR 0118).
            enabled: product_open,
            armed: is_armed(active, kind),
            hint: tool_hint(tool.label.text(lang), tool.key()),
            action: product_open.then_some(ToolAction::Void),
        },
        ToolKind::GridSnap => Ready {
            enabled: owns_view,
            armed: grid,
            hint: tool_hint(tool.label.text(lang), tool.key()),
            action: owns_view.then_some(ToolAction::ToggleGrid),
        },
        ToolKind::Shadows => Ready {
            // A view always has a shadow switch: no document, no selection and no material is
            // involved, and the press draws pressed while the shadow is on (U107).
            enabled: true,
            armed: shadows,
            hint: tool_hint(tool.label.text(lang), tool.key()),
            action: Some(ToolAction::ToggleShadows),
        },
    }
}

/// Whether a running draft can be closed into a ring: only a tool that closes, and only once the
/// chain has the two segments a ring needs, and only while it is still open.
///
/// Two segments are the whole requirement: a straight close adds the third, and a pending arc *is*
/// the closing segment (ADR 0077). This is the rule `DrawDraft::can_close` applies, so the button
/// and the first-node highlight never disagree with the command that runs.
pub const fn draft_can_close(tool: DrawTool, points: usize, closed: bool) -> bool {
    tool.closes() && points >= 2 && !closed
}

/// The icon of the projection toggle: the mode the view is in.
pub(super) fn projection_icon(perspective: bool) -> Icon {
    if perspective { icons::view::PERSPECTIVE } else { icons::view::ORTHOGRAPHIC }
}

/// The hint of the extrude button: what the tool does, or why it is off.
///
/// `selected_path` is `None` when the selection is not a path object, so a path that is merely
/// open gets its own message instead of being called "not a path".
pub(super) fn extrude_hint(selected: Option<Id>, selected_path: Option<bool>) -> Message {
    match (selected, selected_path) {
        (None, _) => Message::ExtrudeSelectPath,
        (Some(_), Some(true)) => Message::ExtrudeSelectedPath,
        (Some(_), Some(false)) => Message::ExtrudePathOpen,
        (Some(_), None) => Message::ExtrudeNotPath,
    }
}
