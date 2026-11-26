//! The selected camera card.
//!
//! A camera is a scene object ([ADR 0036]): the card renames it, edits its projection and its lens,
//! and can make it the camera the viewport renders from. The projection is two numeric fields — a
//! field of view in degrees or an orthographic height in millimetres — and the lens is two more, the
//! aperture radius and the focus distance in millimetres; each one commits as a scrub: a drag writes
//! the document while the hand moves and is one undo entry, typed text lands after a short debounce.
//! The active-camera toggle is a single command of its own.
//!
//! The lens is saved with the camera ([ADR 0249]), so a scene renders the picture its author framed
//! from the window, the command line and the assistant alike.
//!
//! [ADR 0036]: ../docs/decisions/0036-cameras-and-lights-are-scene-objects.md
//! [ADR 0249]: ../docs/decisions/0249-a-cameras-lens-is-document-data.md

use egui::{TextEdit, Ui};
use engine::node_cmds::NODE_RENAME_ID;
use engine::scene_cmds::{CAMERA_LENS_SET_ID, CAMERA_PROJECTION_SET_ID, SCENE_ACTIVE_CAMERA_SET_ID};
use engine::{CameraLensSet, CameraProjectionSet, EditScope, NodeRename, SceneActiveCameraSet, Session};
use model::{CameraLens, CameraProjection, Id};
use text::{Lang, Message};
use units::{Degrees, Length};

use crate::command::{dispatch, json};
use crate::scrub::{self, CommitPolicy, Scrub};
use crate::section::section;
use crate::theme;

/// The field of view a perspective camera starts from when the draft has no perspective value.
const DEFAULT_FOV_DEGREES: f64 = 50.0;

/// The height an orthographic camera starts from when the draft has no orthographic value.
const DEFAULT_HEIGHT_MILLIMETRES: f64 = 2_000.0;

/// Persistent state of the camera card.
#[derive(Debug, Default)]
pub(crate) struct CameraCard {
    draft: Option<Draft>,
    error: Option<String>,
}

/// The values being edited and the projection they were read from.
///
/// The numbers are the field's own units — degrees and millimetres — because that is what a scrub
/// moves; the document keeps [`Degrees`] and [`Length`], and the card converts at the command edge.
#[derive(Clone, Debug, PartialEq)]
struct Draft {
    camera: Id,
    base: (CameraProjection, CameraLens),
    name: String,
    perspective: bool,
    fov: f64,
    height: f64,
    aperture_mm: f64,
    focus_mm: f64,
}

impl Draft {
    fn new(camera: Id, name: &str, projection: CameraProjection, lens: CameraLens) -> Self {
        let (fov, height) = match projection {
            CameraProjection::Perspective { fov_y } => (fov_y.degrees_f64(), DEFAULT_HEIGHT_MILLIMETRES),
            CameraProjection::Orthographic { height } => (DEFAULT_FOV_DEGREES, height.millimetres_f64()),
        };
        let perspective = matches!(projection, CameraProjection::Perspective { .. });
        Self {
            camera,
            base: (projection, lens),
            name: name.to_owned(),
            perspective,
            fov,
            height,
            aperture_mm: lens.aperture().millimetres_f64(),
            focus_mm: lens.focus().millimetres_f64(),
        }
    }
}

impl CameraCard {
    /// Draws the camera card and dispatches accepted edits.
    pub(crate) fn show(&mut self, ui: &mut Ui, session: &mut Session, camera_id: Id) {
        // One token per frame for the two projection fields: the edit that takes it is one undo entry.
        let fresh = session.edit_scope();
        let Some(camera) = session.project().camera(camera_id) else {
            self.draft = None;
            self.error = None;
            return;
        };
        let name = camera.name().to_owned();
        let project = (camera.projection(), camera.lens());
        if self
            .draft
            .as_ref()
            .is_none_or(|draft| draft.camera != camera_id || draft.base != project || draft.name != name)
        {
            self.error = None;
            self.draft = Some(Draft::new(camera_id, &name, project.0, project.1));
        }
        let Some(draft) = self.draft.as_mut() else {
            return;
        };

        let lang = crate::language(ui.ctx());
        let mut rename = None;
        let mut name_error = None;
        ui.separator();
        section(ui, "camera.name", Message::SectionName.text(lang), true, |ui| {
            ui.horizontal(|ui| {
                let response = ui.add(TextEdit::singleline(&mut draft.name).desired_width(180.0));
                let clicked = ui.button(Message::CmdRename.text(lang)).clicked();
                if response.lost_focus() || clicked {
                    let trimmed = draft.name.trim();
                    if trimmed.is_empty() {
                        name_error = Some(Message::CameraNameEmpty.text(lang).to_owned());
                    } else if trimmed != name {
                        rename = Some(trimmed.to_owned());
                    }
                }
            });
        });

        let mut switched = false;
        let mut commit = None;
        // The lens takes its own token: the two commands write two fields of one camera, and a frame
        // that edits only the focus must not rewrite the projection beside it (Y11).
        let mut lens_commit = None;
        let mut cancelled = Vec::new();
        let mut refused = None;
        ui.separator();
        section(ui, "camera.projection", Message::SectionProjection.text(lang), true, |ui| {
            ui.horizontal(|ui| {
                switched |= ui.radio_value(&mut draft.perspective, true, Message::Perspective.text(lang)).changed();
                switched |= ui.radio_value(&mut draft.perspective, false, Message::Orthographic.text(lang)).changed();
            });
            if draft.perspective {
                ui.horizontal(|ui| {
                    ui.label(Message::FieldOfView.text(lang));
                    let outcome = Scrub::new(&mut draft.fov)
                        .speed(0.2)
                        .range(1.0..=179.0)
                        .decimals(3)
                        .unit("°")
                        .parse(scrub::parse_degrees)
                        .policy(CommitPolicy::Live)
                        .edit(fresh)
                        .show(ui);
                    if outcome.commit {
                        commit = outcome.scope;
                    }
                    refused = refused.or(outcome.error);
                    cancelled.extend(outcome.cancelled);
                });
                ui.label(
                    Message::BetweenRange
                        .with(lang, [CameraProjectionBounds::MIN_FOV, CameraProjectionBounds::MAX_FOV]),
                );
            } else {
                ui.horizontal(|ui| {
                    ui.label(Message::FieldHeight.text(lang));
                    let outcome = Scrub::new(&mut draft.height)
                        .speed(5.0)
                        .range(0.001..=1_000_000.0)
                        .decimals(3)
                        .unit("mm")
                        .parse(scrub::parse_millimetres)
                        .policy(CommitPolicy::Live)
                        .edit(fresh)
                        .show(ui);
                    if outcome.commit {
                        commit = outcome.scope;
                    }
                    refused = refused.or(outcome.error);
                    cancelled.extend(outcome.cancelled);
                });
                ui.label(Message::PositiveAtMost.with(lang, [CameraProjectionBounds::MAX_HEIGHT]));
            }
        });

        // The lens: an aperture radius and a focus distance, both in millimetres, both saved with the
        // camera (Y11, ADR 0249). A radius of zero is the pinhole the camera was born as.
        ui.separator();
        section(ui, "camera.lens", Message::SectionLens.text(lang), true, |ui| {
            ui.horizontal(|ui| {
                ui.label(Message::LensAperture.text(lang));
                let outcome = Scrub::new(&mut draft.aperture_mm)
                    .speed(1.0)
                    .range(0.0..=CameraLensBounds::MAX_APERTURE_MM)
                    .decimals(1)
                    .unit("mm")
                    .policy(CommitPolicy::Live)
                    .edit(fresh)
                    .show(ui);
                if outcome.commit {
                    lens_commit = outcome.scope;
                }
                refused = refused.or(outcome.error);
                cancelled.extend(outcome.cancelled);
            });
            ui.horizontal(|ui| {
                ui.label(Message::LensFocus.text(lang));
                let outcome = Scrub::new(&mut draft.focus_mm)
                    .speed(20.0)
                    .range(CameraLensBounds::MIN_FOCUS_MM..=CameraLensBounds::MAX_FOCUS_MM)
                    .decimals(0)
                    .unit("mm")
                    .policy(CommitPolicy::Live)
                    .edit(fresh)
                    .show(ui);
                if outcome.commit {
                    lens_commit = outcome.scope;
                }
                refused = refused.or(outcome.error);
                cancelled.extend(outcome.cancelled);
            });
            ui.label(Message::LensRange.with(
                lang,
                [
                    CameraLensBounds::MIN_APERTURE_MM.to_string().as_str(),
                    CameraLensBounds::MAX_APERTURE_MM.to_string().as_str(),
                    CameraLensBounds::MIN_FOCUS_MM.to_string().as_str(),
                    CameraLensBounds::MAX_FOCUS_MM.to_string().as_str(),
                ],
            ));
        });

        // Take a snapshot so the document borrow ends before a command runs: both readers run here,
        // while the draft is still borrowed, and nothing below touches it again.
        let next = projection_from_draft(draft, lang);
        let next_lens = lens_from_draft(draft, lang);
        if let Some(message) = refused {
            ui.colored_label(theme::danger(ui), message.text(lang));
        }
        if let Some(error) = &self.error {
            ui.colored_label(theme::danger(ui), error);
        }

        for scope in cancelled {
            session.cancel_edit(scope);
        }

        if let Some(name) = rename {
            match apply_rename(session, camera_id, name) {
                Ok(()) => {
                    self.error = None;
                    self.draft = None;
                }
                Err(error) => self.error = Some(error),
            }
            ui.ctx().request_repaint();
            return;
        }
        let projection_change = if !switched && commit.is_none() {
            None
        } else {
            match next {
                Ok(next) => Some(Ok((commit, next))),
                Err(error) => Some(Err(error)),
            }
        };
        let dispatched = projection_change.is_some();
        let refused_name = name_error.is_some();
        match projection_change {
            Some(Ok((scope, next))) => match apply_projection(session, camera_id, next, scope) {
                Ok(()) => {
                    self.error = None;
                    self.draft = None;
                }
                Err(error) => self.error = Some(error),
            },
            Some(Err(error)) => self.error = Some(error),
            None => {
                if let Some(error) = refused {
                    self.error = Some(error.text(lang).to_owned());
                } else if let Some(error) = name_error {
                    self.error = Some(error);
                }
            }
        }
        // The lens is one command of its own, and it runs only when one of its two fields committed:
        // a drag of the aperture is one entry, and a projection edit never rewrites it.
        let lens_dispatched = if lens_commit.is_some() {
            match next_lens {
                Err(error) => self.error = Some(error),
                Ok(lens) => match apply_lens(session, camera_id, lens, lens_commit) {
                    Ok(()) => self.draft = None,
                    Err(error) => self.error = Some(error),
                },
            }
            true
        } else {
            false
        };
        if dispatched || lens_dispatched || refused.is_some() || refused_name {
            ui.ctx().request_repaint();
        }

        let mut active = session.project().active_camera() == Some(camera_id);
        ui.separator();
        section(ui, "camera.active", Message::SectionRenderView.text(lang), true, |ui| {
            if ui.checkbox(&mut active, Message::RenderFromCamera.text(lang)).changed() {
                let request = SceneActiveCameraSet { camera: active.then_some(camera_id) };
                if let Err(error) = set_active_camera(session, request) {
                    self.error = Some(error);
                }
                ui.ctx().request_repaint();
            }
        });
    }
}

/// Ranges shown in the card; they mirror the validation in the model.
struct CameraProjectionBounds;

impl CameraProjectionBounds {
    const MIN_FOV: Degrees = Degrees::from_millidegrees(1_000);
    const MAX_FOV: Degrees = Degrees::from_millidegrees(179_000);
    const MAX_HEIGHT: Length = Length::from_micrometres(1_000_000_000);
}

/// Ranges shown for the lens; they mirror the validation in the model.
struct CameraLensBounds;

impl CameraLensBounds {
    const MIN_APERTURE_MM: f64 = 0.0;
    const MAX_APERTURE_MM: f64 = 500.0;
    const MIN_FOCUS_MM: f64 = 1.0;
    const MAX_FOCUS_MM: f64 = 1_000_000.0;
}

/// Reads the draft into a lens, refusing a value that cannot be the camera's.
fn lens_from_draft(draft: &Draft, lang: Lang) -> Result<CameraLens, String> {
    let aperture = Length::from_millimetres_f64(draft.aperture_mm)
        .ok_or_else(|| Message::FieldOfViewError.with(lang, [Message::LengthErrorOutOfRange.text(lang)]))?;
    let focus = Length::from_millimetres_f64(draft.focus_mm)
        .ok_or_else(|| Message::FieldOfViewError.with(lang, [Message::LengthErrorOutOfRange.text(lang)]))?;
    CameraLens::new(aperture, focus).map_err(|_| {
        Message::LensRange.with(
            lang,
            [
                CameraLensBounds::MIN_APERTURE_MM.to_string().as_str(),
                CameraLensBounds::MAX_APERTURE_MM.to_string().as_str(),
                CameraLensBounds::MIN_FOCUS_MM.to_string().as_str(),
                CameraLensBounds::MAX_FOCUS_MM.to_string().as_str(),
            ],
        )
    })
}

/// Applies the lens; with a scope, the whole drag is one undo entry.
fn apply_lens(session: &mut Session, camera: Id, lens: CameraLens, scope: Option<EditScope>) -> Result<(), String> {
    let params = json(&CameraLensSet { camera, lens })?;
    match scope {
        Some(scope) => {
            session.dispatch_in(scope, CAMERA_LENS_SET_ID, &params).map(|_| ()).map_err(|error| error.to_string())
        }
        None => dispatch(session, CAMERA_LENS_SET_ID, &params).map(|_| ()),
    }
}

/// Reads the draft into a projection, refusing a value that cannot be the camera's.
fn projection_from_draft(draft: &Draft, lang: Lang) -> Result<CameraProjection, String> {
    if draft.perspective {
        let fov_y = scrub::degrees(draft.fov)
            .ok_or_else(|| Message::FieldOfViewError.with(lang, [Message::DegreesErrorOutOfRange.text(lang)]))?;
        let projection = CameraProjection::Perspective { fov_y };
        if !projection.is_valid() {
            return Err(Message::FovMustBeBetween
                .with(lang, [CameraProjectionBounds::MIN_FOV, CameraProjectionBounds::MAX_FOV]));
        }
        Ok(projection)
    } else {
        let height = scrub::millimetres(draft.height)
            .ok_or_else(|| Message::OrthoHeightError.with(lang, [Message::LengthErrorOutOfRange.text(lang)]))?;
        let projection = CameraProjection::Orthographic { height };
        if !projection.is_valid() {
            return Err(Message::OrthoHeightPositiveAtMost.with(lang, [CameraProjectionBounds::MAX_HEIGHT]));
        }
        Ok(projection)
    }
}

fn apply_rename(session: &mut Session, camera: Id, name: String) -> Result<(), String> {
    let params = json(&NodeRename { node: camera, name })?;
    dispatch(session, NODE_RENAME_ID, &params).map(|_| ())
}

/// Sets the projection; with a scope, the whole drag is one undo entry.
fn apply_projection(
    session: &mut Session,
    camera: Id,
    projection: CameraProjection,
    scope: Option<EditScope>,
) -> Result<(), String> {
    let params = json(&CameraProjectionSet { camera, projection })?;
    match scope {
        Some(scope) => {
            session.dispatch_in(scope, CAMERA_PROJECTION_SET_ID, &params).map(|_| ()).map_err(|error| error.to_string())
        }
        None => dispatch(session, CAMERA_PROJECTION_SET_ID, &params).map(|_| ()),
    }
}

fn set_active_camera(session: &mut Session, request: SceneActiveCameraSet) -> Result<(), String> {
    let params = json(&request)?;
    dispatch(session, SCENE_ACTIVE_CAMERA_SET_ID, &params).map(|_| ())
}
#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use engine::CameraCreate;
    use engine::scene_cmds::CAMERA_CREATE_ID;
    use model::Placement;
    use testkit::{sample_catalog, sample_project};

    use super::*;

    fn camera_session() -> (Session, Id) {
        let mut session = Session::with_directory(sample_project(), sample_catalog());
        let root = session.project().root().id();
        let params = json(&CameraCreate {
            parent: root,
            name: "Camera".to_owned(),
            placement: Placement::IDENTITY,
            projection: CameraProjection::DEFAULT,
        })
        .expect("params");
        let camera =
            dispatch(&mut session, CAMERA_CREATE_ID, &params).expect("add camera").created_id().expect("a camera id");
        (session, camera)
    }

    #[test]
    fn the_draft_shows_and_parses_the_current_projection() {
        let (_, camera) = camera_session();
        let draft = Draft::new(camera, "Camera", CameraProjection::DEFAULT, CameraLens::PINHOLE);
        assert!(draft.perspective);
        assert_eq!(draft.fov, 50.0);
        assert_eq!(projection_from_draft(&draft, Lang::En), Ok(CameraProjection::DEFAULT));

        let draft = Draft { perspective: false, height: 2_000.0, ..draft };
        assert_eq!(
            projection_from_draft(&draft, Lang::En),
            Ok(CameraProjection::Orthographic { height: Length::from_millimetres(2_000).expect("height") })
        );
    }

    #[test]
    fn the_lens_is_read_from_the_camera_and_written_back_as_one_command() {
        // The lens is the camera's own setting (Y11, ADR 0249): the card reads it from the document,
        // turns two millimetre fields into one command, and applies it as one undo entry.
        let (mut session, camera) = camera_session();
        let draft = Draft::new(camera, "Camera", CameraProjection::DEFAULT, CameraLens::PINHOLE);
        assert_eq!(draft.aperture_mm, 0.0, "a camera is born a pinhole");
        assert_eq!(draft.focus_mm, 1_200.0, "and carries the default focus");
        assert_eq!(lens_from_draft(&draft, Lang::En), Ok(CameraLens::PINHOLE));

        let lensed = Draft { aperture_mm: 45.0, focus_mm: 2_000.0, ..draft.clone() };
        let lens = lens_from_draft(&lensed, Lang::En).expect("a lens");
        assert_eq!(lens.aperture(), Length::from_millimetres(45).expect("a radius"));
        assert_eq!(lens.focus(), Length::from_millimetres(2_000).expect("a focus"));

        // One dispatch, one revision, and the document holds what the card read.
        let before = session.project().revision();
        apply_lens(&mut session, camera, lens, None).expect("apply the lens");
        assert_eq!(session.project().camera(camera).expect("camera").lens(), lens);
        assert_eq!(session.project().revision(), before.checked_next().expect("one revision"));
        session.undo().expect("undo");
        assert_eq!(session.project().camera(camera).expect("camera").lens(), CameraLens::PINHOLE);

        // A radius past the cap and a focus of no distance are sentences, not dispatches.
        let too_wide = Draft { aperture_mm: 900.0, ..lensed.clone() };
        let refusal = lens_from_draft(&too_wide, Lang::En).expect_err("900 mm is not a lens");
        assert!(refusal.contains("500"), "the sentence names the cap: {refusal}");
        let no_focus = Draft { focus_mm: 0.0, ..lensed };
        assert!(lens_from_draft(&no_focus, Lang::En).is_err());
    }

    #[test]
    fn a_value_outside_the_range_is_reported_instead_of_dispatching() {
        let (session, camera) = camera_session();
        let before = session.project().revision();
        let draft = Draft::new(camera, "Camera", CameraProjection::DEFAULT, CameraLens::PINHOLE);

        let outside = Draft { fov: 0.0, ..draft.clone() };
        assert!(projection_from_draft(&outside, Lang::En).is_err());
        let zero_height = Draft { perspective: false, height: 0.0, ..draft };
        assert!(projection_from_draft(&zero_height, Lang::En).is_err());
        assert_eq!(session.project().revision(), before);
    }

    #[test]
    fn dragging_the_field_of_view_is_one_undo_entry() {
        let (mut session, camera) = camera_session();
        let before = session.project().camera(camera).expect("camera").projection();
        let scope = session.edit_scope();
        for fov in [55.0, 60.0, 65.0] {
            let draft = Draft { fov, ..Draft::new(camera, "Camera", before, CameraLens::PINHOLE) };
            let next = projection_from_draft(&draft, Lang::En).expect("a valid projection");
            apply_projection(&mut session, camera, next, Some(scope)).expect("set projection");
        }
        session.end_edit();
        assert_eq!(
            session.project().camera(camera).expect("camera").projection(),
            CameraProjection::Perspective { fov_y: Degrees::from_millidegrees(65_000) }
        );
        session.undo().expect("undo the drag");
        assert_eq!(
            session.project().camera(camera).expect("camera").projection(),
            before,
            "one undo returns the projection the drag started from"
        );
    }

    #[test]
    fn the_active_camera_toggle_dispatches_one_command() {
        let (mut session, camera) = camera_session();
        set_active_camera(&mut session, SceneActiveCameraSet { camera: Some(camera) }).expect("activate");
        assert_eq!(session.project().active_camera(), Some(camera));
        set_active_camera(&mut session, SceneActiveCameraSet { camera: None }).expect("clear");
        assert_eq!(session.project().active_camera(), None);
        assert!(session.can_undo());
    }
}
