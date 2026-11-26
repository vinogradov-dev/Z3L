//! The selected light card.
//!
//! A light is a scene object ([ADR 0036]): the card renames it and edits its kind, colour and
//! intensity or power. Every number is a scrub and commits as one edit — a drag writes the document
//! while the hand moves and is one undo entry, typed text lands after a short debounce — so there is
//! no Apply button. A sun intensity is a multiplier; a point power is in watts ([ADR 0039]); a window
//! is watts over a face, and the card gives the face its two sides in millimetres ([ADR 0247]).
//!
//! [ADR 0036]: ../docs/decisions/0036-cameras-and-lights-are-scene-objects.md
//! [ADR 0039]: ../docs/decisions/0039-point-lights-use-watts-and-inverse-square-falloff.md
//! [ADR 0247]: ../docs/decisions/0247-a-rectangular-light-is-a-face-at-watts.md

use egui::{TextEdit, Ui};
use engine::node_cmds::NODE_RENAME_ID;
use engine::scene_cmds::LIGHT_PARAMS_SET_ID;
use engine::{EditScope, LightParamsSet, NodeRename, Session};
use model::{Id, LightKind, LightNode, LightSize};
use text::{Lang, Message};
use units::Length;

use crate::command::{dispatch, json};
use crate::scrub::{CommitPolicy, Scrub};
use crate::section::section;
use crate::theme;

/// Default point-light power in watts when a sun is switched to a point light.
const DEFAULT_POINT_POWER: f32 = 1000.0;

/// A sun intensity below this reads as a multiplier, not as a wattage.
const SUN_TO_POINT_THRESHOLD: f32 = 100.0;

/// Persistent state of the light card.
#[derive(Debug, Default)]
pub(crate) struct LightCard {
    draft: Option<Draft>,
    error: Option<String>,
}

/// Values being edited and the light they were read from.
#[derive(Clone, Debug, PartialEq)]
struct Draft {
    light: Id,
    base: (LightKind, [f32; 3], f32, LightSize),
    name: String,
    kind: LightKind,
    color: [f32; 3],
    intensity: f32,
    width_mm: f64,
    height_mm: f64,
}

impl Draft {
    fn new(light: Id, name: &str, kind: LightKind, color: [f32; 3], intensity: f32, size: LightSize) -> Self {
        Self {
            light,
            base: (kind, color, intensity, size),
            name: name.to_owned(),
            kind,
            color,
            intensity,
            width_mm: size.width().millimetres_f64(),
            height_mm: size.height().millimetres_f64(),
        }
    }
}

impl LightCard {
    /// Draws the light card and dispatches accepted edits.
    pub(crate) fn show(&mut self, ui: &mut Ui, session: &mut Session, light_id: Id) {
        let Some(light) = session.project().light(light_id) else {
            self.draft = None;
            self.error = None;
            return;
        };
        let name = light.name().to_owned();
        let base = (light.kind(), light.color(), light.intensity(), light.size());
        if self.draft.as_ref().is_none_or(|draft| draft.light != light_id || draft.base != base || draft.name != name) {
            self.error = None;
            self.draft = Some(Draft::new(light_id, &name, base.0, base.1, base.2, base.3));
        }
        let Some(draft) = self.draft.as_mut() else {
            return;
        };

        let lang = crate::language(ui.ctx());
        // One token per frame for the colour and intensity fields: the edit that takes it is one
        // undo entry.
        let fresh = session.edit_scope();
        let mut rename = None;
        let mut name_error = None;
        ui.separator();
        section(ui, "light.name", Message::SectionName.text(lang), true, |ui| {
            ui.horizontal(|ui| {
                let response = ui.add(TextEdit::singleline(&mut draft.name).desired_width(180.0));
                let clicked = ui.button(Message::CmdRename.text(lang)).clicked();
                if response.lost_focus() || clicked {
                    let trimmed = draft.name.trim();
                    if trimmed.is_empty() {
                        name_error = Some(Message::LightNameEmpty.text(lang).to_owned());
                    } else if trimmed != name {
                        rename = Some(trimmed.to_owned());
                    }
                }
            });
        });

        let mut switched = false;
        let mut commit = None;
        let mut cancelled = Vec::new();
        let mut refused = None;
        ui.separator();
        section(ui, "light.params", Message::SectionLight.text(lang), true, |ui| {
            ui.horizontal(|ui| {
                switched |= ui.radio_value(&mut draft.kind, LightKind::Sun, Message::Sun.text(lang)).changed();
                switched |= ui.radio_value(&mut draft.kind, LightKind::Point, Message::Point.text(lang)).changed();
                switched |= ui.radio_value(&mut draft.kind, LightKind::Area, Message::Area.text(lang)).changed();
            });
            // A radiated kind is measured in watts, so a sun multiplier becomes a lamp-sized power:
            // the two that radiate from a place spread it over a face or a sphere (ADR 0039, 0247).
            if switched && draft.kind != LightKind::Sun {
                draft.intensity = power_for_point_switch(draft.intensity);
            }
            for (component, value) in ["R", "G", "B"].into_iter().zip(draft.color.iter_mut()) {
                ui.horizontal(|ui| {
                    ui.label(component);
                    let outcome = Scrub::new(value)
                        .speed(0.01)
                        .range(0.0..=1.0)
                        .decimals(3)
                        .policy(CommitPolicy::Live)
                        .edit(fresh)
                        .show(ui);
                    if outcome.commit {
                        commit = outcome.scope;
                    }
                    refused = refused.or(outcome.error);
                    cancelled.extend(outcome.cancelled);
                });
            }
            ui.horizontal(|ui| {
                // A sun stores an irradiance multiplier; a point or area light stores radiant power
                // in watts (ADR 0039, 0247). The two units get their own label, unit and speed.
                let (label, unit, speed) = if draft.kind == LightKind::Sun {
                    (Message::Intensity.text(lang), "", 0.05)
                } else {
                    (Message::PowerWatts.text(lang), "W", 10.0)
                };
                ui.label(label);
                let outcome = Scrub::new(&mut draft.intensity)
                    .speed(speed)
                    .range(0.0..=LightNode::MAX_INTENSITY)
                    .decimals(2)
                    .unit(unit)
                    .policy(CommitPolicy::Live)
                    .edit(fresh)
                    .show(ui);
                if outcome.commit {
                    commit = outcome.scope;
                }
                refused = refused.or(outcome.error);
                cancelled.extend(outcome.cancelled);
            });
            // A window's face: two sides in millimetres, meaningful only for the kind that has one.
            if draft.kind == LightKind::Area {
                for (label, value) in [(Message::Width, &mut draft.width_mm), (Message::Height, &mut draft.height_mm)] {
                    let (side_commit, side_error, side_cancelled) = side_field(ui, lang, label, value, fresh);
                    commit = side_commit.or(commit);
                    refused = refused.or(side_error);
                    cancelled.extend(side_cancelled);
                }
            }
        });

        // Take a snapshot so the document borrow ends before a command runs.
        let fields = draft.clone();
        if let Some(error) = &self.error {
            ui.colored_label(theme::danger(ui), error);
        }

        if let Some(name) = rename {
            match apply_rename(session, light_id, name) {
                Ok(()) => {
                    self.error = None;
                    self.draft = None;
                }
                Err(error) => self.error = Some(error),
            }
            ui.ctx().request_repaint();
            return;
        }
        for scope in cancelled {
            session.cancel_edit(scope);
        }
        let commit = if switched { Some(None) } else { commit.map(Some) };
        if let Some(scope) = commit {
            match params_from_draft(&fields, lang).and_then(|request| apply_params(session, request, scope)) {
                Ok(()) => {
                    self.error = None;
                    if scope.is_some() {
                        // The drag continues from the document value next frame.
                        self.draft = None;
                    }
                }
                Err(error) => self.error = Some(error),
            }
            ui.ctx().request_repaint();
        } else if let Some(message) = refused {
            self.error = Some(message.text(lang).to_owned());
        } else if let Some(error) = name_error {
            self.error = Some(error);
        }
    }
}

/// A side of a face in whole millimetres, the unit the card reads and writes.
fn millimetres(value: Length) -> f64 {
    value.millimetres_f64()
}

/// One side of a window's face as a scrub in millimetres.
///
/// It returns the same three answers a field of the card does — the undo scope of a commit, the
/// refusal of a typed number, and a cancelled scope — because the caller treats them identically.
fn side_field(
    ui: &mut Ui,
    lang: Lang,
    label: Message,
    value: &mut f64,
    fresh: EditScope,
) -> (Option<EditScope>, Option<Message>, Vec<EditScope>) {
    let mut commit = None;
    let mut error = None;
    let mut cancelled = Vec::new();
    ui.horizontal(|ui| {
        ui.label(label.text(lang));
        let outcome = Scrub::new(value)
            .speed(1.0)
            .range(millimetres(LightSize::MIN)..=millimetres(LightSize::MAX))
            .decimals(0)
            .unit("mm")
            .policy(CommitPolicy::Live)
            .edit(fresh)
            .show(ui);
        if outcome.commit {
            commit = outcome.scope;
        }
        error = outcome.error;
        cancelled.extend(outcome.cancelled);
    });
    (commit, error, cancelled)
}

/// A sun intensity is a small multiplier; a point light needs a lamp-sized power in watts.
fn power_for_point_switch(intensity: f32) -> f32 {
    if intensity < SUN_TO_POINT_THRESHOLD { DEFAULT_POINT_POWER } else { intensity }
}

/// Builds the command request, refusing a value that is not finite or is outside its range.
fn params_from_draft(draft: &Draft, lang: Lang) -> Result<LightParamsSet, String> {
    let color = draft.color;
    if !color.iter().all(|value| value.is_finite() && (0.0..=1.0).contains(value)) {
        return Err(Message::ColourComponents.text(lang).to_owned());
    }
    if !draft.intensity.is_finite() || !(0.0..=LightNode::MAX_INTENSITY).contains(&draft.intensity) {
        let unit = if draft.kind == LightKind::Sun { Message::Intensity } else { Message::Power };
        return Err(
            Message::IntensityRange.with(lang, [unit.text(lang), LightNode::MAX_INTENSITY.to_string().as_str()])
        );
    }
    let size = LightSize::new(whole_millimetres(draft.width_mm), whole_millimetres(draft.height_mm)).map_err(|_| {
        Message::LightSizeRange.with(
            lang,
            [millimetres(LightSize::MIN).to_string().as_str(), millimetres(LightSize::MAX).to_string().as_str()],
        )
    })?;
    Ok(LightParamsSet { light: draft.light, kind: draft.kind, color, intensity: draft.intensity, size })
}

/// A card's millimetre reading as the document's length: a reading the document cannot hold is zero,
/// which the size check then refuses with the range rather than a rounded side nobody typed.
fn whole_millimetres(value: f64) -> Length {
    Length::from_millimetres_f64(value).unwrap_or(Length::ZERO)
}

fn apply_rename(session: &mut Session, light: Id, name: String) -> Result<(), String> {
    let params = json(&NodeRename { node: light, name })?;
    dispatch(session, NODE_RENAME_ID, &params).map(|_| ())
}

/// Applies the parameters; with a scope, the whole drag is one undo entry.
fn apply_params(session: &mut Session, request: LightParamsSet, scope: Option<EditScope>) -> Result<(), String> {
    let params = json(&request)?;
    match scope {
        Some(scope) => {
            session.dispatch_in(scope, LIGHT_PARAMS_SET_ID, &params).map(|_| ()).map_err(|error| error.to_string())
        }
        None => dispatch(session, LIGHT_PARAMS_SET_ID, &params).map(|_| ()),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use engine::LightCreate;
    use engine::scene_cmds::LIGHT_CREATE_ID;
    use model::Placement;
    use testkit::{sample_catalog, sample_project};

    use super::*;

    fn light_session() -> (Session, Id) {
        let mut session = Session::with_directory(sample_project(), sample_catalog());
        let root = session.project().root().id();
        let params = json(&LightCreate {
            parent: root,
            name: "Key".to_owned(),
            placement: Placement::IDENTITY,
            kind: LightKind::Sun,
            color: [1.0, 0.9, 0.8],
            intensity: 2.0,
            size: LightSize::DEFAULT,
        })
        .expect("params");
        let light =
            dispatch(&mut session, LIGHT_CREATE_ID, &params).expect("add light").created_id().expect("a light id");
        (session, light)
    }

    #[test]
    fn the_draft_reads_the_document_and_builds_one_request() {
        let (_, light) = light_session();
        let draft = Draft::new(light, "Key", LightKind::Sun, [1.0, 0.9, 0.8], 2.0, LightSize::DEFAULT);
        assert_eq!(
            params_from_draft(&draft, Lang::En),
            Ok(LightParamsSet {
                light,
                kind: LightKind::Sun,
                color: [1.0, 0.9, 0.8],
                intensity: 2.0,
                size: LightSize::DEFAULT,
            })
        );
    }

    #[test]
    fn an_out_of_range_value_is_refused_before_dispatch() {
        let (session, light) = light_session();
        let before = session.project().revision();
        let bad_color = Draft {
            light,
            base: (LightKind::Sun, [1.0; 3], 1.0, LightSize::DEFAULT),
            name: "Key".to_owned(),
            kind: LightKind::Point,
            color: [1.5, 0.0, 0.0],
            intensity: 1.0,
            width_mm: 1000.0,
            height_mm: 1000.0,
        };
        assert!(params_from_draft(&bad_color, Lang::En).is_err());
        let bad_intensity = Draft { color: [1.0; 3], intensity: f32::NAN, ..bad_color };
        assert!(params_from_draft(&bad_intensity, Lang::En).is_err());
        // A window's face is checked in the card too, so a scrub past its end is a sentence.
        let bad_face = Draft {
            light,
            base: (LightKind::Area, [1.0; 3], 40.0, LightSize::DEFAULT),
            name: "Window".to_owned(),
            kind: LightKind::Area,
            color: [1.0; 3],
            intensity: 40.0,
            width_mm: 1.0,
            height_mm: 1000.0,
        };
        let refusal = params_from_draft(&bad_face, Lang::En).expect_err("a 1 mm face is refused");
        assert_eq!(refusal, Message::LightSizeRange.with(Lang::En, ["10", "10000"]));
        assert_eq!(session.project().revision(), before);
    }

    #[test]
    fn switching_a_sun_to_a_point_light_uses_a_lamp_sized_power() {
        assert_eq!(power_for_point_switch(1.0), DEFAULT_POINT_POWER, "a sun multiplier becomes a lamp");
        assert_eq!(power_for_point_switch(2.5), DEFAULT_POINT_POWER);
        assert_eq!(power_for_point_switch(250.0), 250.0, "a value already in watts is kept");
        assert_eq!(power_for_point_switch(1000.0), 1000.0);
    }

    #[test]
    fn applying_changes_one_revision_and_can_be_undone() {
        let (mut session, light) = light_session();
        let request = LightParamsSet {
            light,
            kind: LightKind::Point,
            color: [0.2, 0.3, 0.4],
            intensity: 4.0,
            size: LightSize::DEFAULT,
        };
        apply_params(&mut session, request, None).expect("apply");
        let node = session.project().light(light).expect("light");
        assert_eq!(node.kind(), LightKind::Point);
        assert_eq!(node.color(), [0.2, 0.3, 0.4]);
        assert_eq!(node.intensity(), 4.0);
        session.undo().expect("undo");
        assert_eq!(session.project().light(light).expect("light").kind(), LightKind::Sun);
    }
}
