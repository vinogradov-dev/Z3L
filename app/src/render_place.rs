//! The render place's shell half: the worker that traces the image, and what the page shows meanwhile.
//!
//! The page (`ui::render_page`) owns the numbers and the drawing; this module owns the run. A final
//! render takes minutes, so it runs on its own thread and never on the frame path: the page polls the
//! run's own counter between frames, the Cancel button sets the run's own flag, and the finished
//! value is taken at the frame boundary exactly like a directory or a file-manager answer
//! (`docs/domain/nomenclature.md` §13, ADR 0182). Nothing here touches a document: the frame is a
//! clone of the one the viewport draws, and the only thing a run writes is one PNG (ADR 0210).
//!
//! The place is not a modal: it keeps its numbers and its last result while the person goes back to
//! the tree and opens it again, because a render is a place in the work, not a question about the
//! document in front.
//!
//! Refuses: a run started while another is in flight (the page disables the button, and a second
//! start is ignored here as well), and a machine with no adapter — that sentence lands in the page's
//! own report line instead of a process that dies.
//!
//! Changes when: what a run needs from the shell, or what the page shows while it runs, changes.

use std::sync::Arc;
use std::thread::JoinHandle;

use eframe::egui_wgpu;
use r#final::{Input, Progress, Report, Settings as RenderSettings, Snapshots, Tracer};

use ui::render_page::{Bounds, Settings, Stage};

/// One run: the counter the page reads and the thread that fills it.
#[derive(Debug)]
struct Job {
    progress: Arc<Progress>,
    thread: JoinHandle<Result<Report, String>>,
}

/// The render place: its numbers, its run and where the last one got to.
#[derive(Debug)]
pub(crate) struct RenderPlace {
    /// The size, the samples, the exposure and the file, as the person left them.
    settings: Settings,
    /// Where the last finished run got to.
    finished: Stage,
    /// The run in flight, if there is one.
    job: Option<Job>,
}

impl RenderPlace {
    /// A place that has never rendered, with the draft preset and no destination.
    pub(crate) fn new() -> Self {
        Self { settings: Settings::draft(), finished: Stage::Idle, job: None }
    }

    /// The limits the page keeps its numbers inside, read from the tracer's own constants.
    pub(crate) fn bounds() -> Bounds {
        Bounds {
            min_side: r#final::MIN_SIDE,
            max_side: r#final::MAX_SIDE,
            min_samples: r#final::MIN_SAMPLES,
            max_samples: r#final::MAX_SAMPLES,
            max_pixels: r#final::MAX_PIXELS,
        }
    }

    /// The numbers and the destination, for the page to edit.
    pub(crate) fn settings(&mut self) -> &mut Settings {
        &mut self.settings
    }

    /// Where the place stands this frame: the run's own counter while it moves, else what the last
    /// run left.
    pub(crate) fn stage(&mut self) -> Stage {
        self.take_finished();
        match &self.job {
            Some(job) => Stage::Running { done: job.progress.done(), total: job.progress.total() },
            None => self.finished.clone(),
        }
    }

    /// Whether a run is in flight, which the shell asks before it repaints for the next frame.
    pub(crate) fn is_running(&self) -> bool {
        self.job.is_some()
    }

    /// Where the image goes, for the file manager's answer to land in.
    pub(crate) fn set_path(&mut self, path: std::path::PathBuf) {
        self.settings.path = Some(path);
    }

    /// Reports a refusal the place made before a thread was started, so the page prints it.
    pub(crate) fn failed(&mut self, reason: impl Into<String>) {
        self.finished = Stage::Failed(reason.into());
    }

    /// Stops the run in flight at its next sample; the page keeps drawing until it answers.
    pub(crate) fn cancel(&mut self) {
        if let Some(job) = &self.job {
            job.progress.cancel();
        }
    }

    /// Takes the finished run's answer, if it has one, and returns whether the place changed.
    pub(crate) fn take_finished(&mut self) -> bool {
        let Some(job) = &self.job else { return false };
        if !job.thread.is_finished() {
            return false;
        }
        let Some(job) = self.job.take() else { return false };
        self.finished = match job.thread.join() {
            Ok(Ok(report)) => {
                Stage::Done { path: report.path, width: report.width, height: report.height, samples: report.samples }
            }
            Ok(Err(reason)) => {
                if job.progress.cancelled() {
                    Stage::Cancelled
                } else {
                    Stage::Failed(reason)
                }
            }
            Err(_) => Stage::Failed("the render thread stopped without an answer".to_owned()),
        };
        true
    }

    /// Starts a run over `frame`, from `camera`, at `revision`.
    ///
    /// The frame is cloned into the thread: the person may keep editing while the image is drawn, and
    /// a render of a document that changed under it would be an image nobody asked for. The device is
    /// the window's own, borrowed for the run (ADR 0210).
    pub(crate) fn start(
        &mut self,
        ctx: &eframe::egui::Context,
        render_state: Option<&egui_wgpu::RenderState>,
        frame: scene::SceneFrame,
        camera: scene::Camera,
        revision: u64,
    ) -> Result<(), String> {
        if self.job.is_some() {
            return Err("a render is already running".to_owned());
        }
        let Some(render_state) = render_state else {
            return Err("this machine has no device to render with".to_owned());
        };
        let Some(path) = self.settings.path.clone() else {
            return Err("choose a file to write the image to".to_owned());
        };
        let request = r#final::Request::new(self.settings.width, self.settings.height, self.settings.samples)
            .map_err(|error| error.to_string())?;
        // The page speaks percent of a white room and the render speaks linear radiance: the one
        // place the two vocabularies meet is here.
        let settings = RenderSettings::new(request, self.settings.exposure, path)
            .and_then(|settings| settings.with_environment(self.settings.studio / 100.0))
            .map_err(|error| error.to_string())?;
        let progress = Arc::new(Progress::new(request.samples()));
        // The mailbox the run publishes its checkpoints into. Nobody reads it yet: the place takes
        // the pictures at the frame boundary, in the next step of this task (Y6).
        let snapshots = Snapshots::new();
        let device = render_state.device.clone();
        let queue = render_state.queue.clone();
        let owned = Arc::clone(&progress);
        let ctx = ctx.clone();
        let spawned = std::thread::Builder::new().name("z3l-final-render".to_owned()).spawn(move || {
            let result = (|| {
                let mut tracer = Tracer::new(&device).map_err(|error| error.to_string())?;
                r#final::run(
                    &mut tracer,
                    &device,
                    &queue,
                    Input { frame: &frame, camera, revision },
                    &settings,
                    &owned,
                    &snapshots,
                )
                .map_err(|error| error.to_string())
            })();
            // A dropped context means the window closed under the run; nobody is left to tell.
            ctx.request_repaint();
            result
        });
        match spawned {
            Ok(thread) => {
                self.finished = Stage::Idle;
                self.job = Some(Job { progress, thread });
                Ok(())
            }
            Err(error) => Err(format!("the render thread could not be started: {error}")),
        }
    }

    /// Waits for the run in flight to answer, which a headless test does instead of drawing frames.
    #[cfg(test)]
    pub(crate) fn wait(&mut self) {
        let Some(job) = self.job.take() else { return };
        self.finished = match job.thread.join() {
            Ok(Ok(report)) => {
                Stage::Done { path: report.path, width: report.width, height: report.height, samples: report.samples }
            }
            Ok(Err(reason)) => {
                if job.progress.cancelled() {
                    Stage::Cancelled
                } else {
                    Stage::Failed(reason)
                }
            }
            Err(_) => Stage::Failed("the render thread stopped without an answer".to_owned()),
        };
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    #[test]
    fn a_place_starts_with_the_draft_and_no_destination() {
        let mut place = RenderPlace::new();
        assert!(!place.is_running());
        assert_eq!(place.stage(), Stage::Idle);
        assert_eq!(place.settings().path, None);
        let bounds = RenderPlace::bounds();
        assert_eq!(bounds.min_side, r#final::MIN_SIDE);
        assert_eq!(bounds.max_pixels, r#final::MAX_PIXELS);
        assert!(bounds.max_side >= 2480, "A4 at 300 dpi fits the cap");
        place.set_path("plate.png".into());
        assert_eq!(place.settings().path, Some("plate.png".into()));
    }

    /// One quad facing the camera, lit by the frame's own key light.
    fn fixture() -> (scene::SceneFrame, scene::Camera) {
        use std::sync::Arc;

        let mesh = Arc::new(geom::MeshData {
            positions: vec![[0.0, 0.0, 0.0], [400.0, 0.0, 0.0], [400.0, 400.0, 0.0], [0.0, 400.0, 0.0]],
            normals: Vec::new(),
            indices: vec![0, 1, 2, 0, 2, 3],
        });
        let mut frame = scene::SceneFrame::default();
        let placed = geom::DMat4::from_scale_rotation_translation(
            geom::DVec3::ONE,
            geom::DQuat::from_rotation_x(std::f64::consts::FRAC_PI_2),
            geom::DVec3::new(-200.0, 1000.0, -200.0),
        );
        let id = model::Id::new(1).expect("a non-zero id");
        frame.push(scene::Drawable::new(id, mesh, placed, [0.8, 0.7, 0.6, 1.0]));
        frame.lighting.key_direction = geom::DVec3::new(0.0, -0.7, 0.7).try_normalize().unwrap_or(geom::DVec3::Z);
        let camera = scene::Camera::new(geom::DVec3::new(0.0, 1000.0, 0.0), 2200.0).expect("a camera");
        (frame, camera)
    }

    #[test]
    fn a_render_from_the_place_writes_its_image_and_reports_it() {
        // The whole path through the shell's half: a run on its own thread over the frame the
        // viewport draws, a file on disk, and the place's own report. A machine with no adapter
        // says so instead of failing, like every device test in this tree (`AGENTS.md` §4.7).
        let Ok(state) = crate::ui_shots::render_state() else {
            println!("this machine has no GPU adapter to render a place's image with");
            return;
        };
        let ctx = eframe::egui::Context::default();
        let (frame, camera) = fixture();
        let mut place = RenderPlace::new();
        let path = testkit::scratch_dir("render-place").join("plate.png");
        place.set_path(path.clone());
        place.settings().width = 48;
        place.settings().height = 32;
        place.settings().samples = 2;
        place.start(&ctx, Some(&state), frame, camera, 1).expect("the run starts");
        assert!(place.is_running(), "the run is in flight");
        place.wait();
        assert!(!place.is_running());
        match place.stage() {
            Stage::Done { path: written, width, height, samples } => {
                assert_eq!(written, path);
                assert_eq!((width, height, samples), (48, 32, 2));
            }
            other => panic!("the run reported {other:?}"),
        }
        let bytes = std::fs::read(&path).expect("the image is on disk");
        assert!(bytes.len() > 100, "a PNG is not empty: {}", bytes.len());
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "the file is a PNG");
    }

    #[test]
    fn a_run_without_a_device_or_a_destination_is_a_sentence_not_a_thread() {
        let ctx = eframe::egui::Context::default();
        let mut place = RenderPlace::new();
        let frame = scene::SceneFrame::default();
        let camera = scene::Camera::new(geom::DVec3::ZERO, 1000.0).expect("a camera");
        let refused = place.start(&ctx, None, frame.clone(), camera, 1).expect_err("no device");
        assert!(refused.contains("device"), "{refused}");
        assert!(!place.is_running());
        // A destination and no device still refuses, and the run is not left half started.
        place.set_path("plate.png".into());
        let refused = place.start(&ctx, None, frame, camera, 1).expect_err("no device");
        assert!(refused.contains("device"), "{refused}");
        assert!(!place.take_finished(), "nothing finished, nothing to take");
    }
}
