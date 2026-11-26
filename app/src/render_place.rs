//! The render place's shell half: the worker that traces the image, the pictures it publishes, and
//! what the page shows meanwhile.
//!
//! The page (`ui::render_page`) owns the numbers and the drawing; this module owns the run and the
//! picture. A final render takes minutes, so it runs on its own thread and never on the frame path:
//! the page polls the run's own counter between frames, the Cancel button sets the run's own flag,
//! and the finished value is taken at the frame boundary exactly like a directory or a file-manager
//! answer (`docs/domain/nomenclature.md` §13, ADR 0182). Nothing here touches a document: the frame
//! is a clone of the one the viewport draws, and the only thing a run writes is one PNG (ADR 0210).
//!
//! The picture is the place's surface (Y6, ADR 0250): the tracer publishes snapshots into a mailbox
//! of one, this module tones the newest into a texture egui draws and hands the page its
//! [`Picture`], and the last one stays there after the run, after a cancel and across documents. A
//! snapshot is linear radiance, so the exposure the page edits re-tones the picture it already has
//! instead of asking for another run — the curve is the one the file is written with.
//!
//! The place is not a modal: it keeps its numbers and its last picture while the person goes back to
//! the tree and opens it again, because a render is a place in the work, not a question about the
//! document in front.
//!
//! Refuses: a run started while another is in flight (the page disables the button, and a second
//! start is ignored here as well), and a machine with no adapter — that sentence lands in the page's
//! own status row instead of a process that dies.
//!
//! Changes when: what a run needs from the shell, or what the page shows while it runs, changes.

use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use eframe::egui;
use eframe::egui_wgpu;
use eframe::wgpu;
use r#final::{Image, Input, Progress, Report, Settings as RenderSettings, Snapshot, Snapshots, Tracer};

use ui::render_page::{Bounds, Picture, Run, Settings, Stage};

/// One run: the counter the page reads, the pictures it publishes and the thread that fills them.
#[derive(Debug)]
struct Job {
    progress: Arc<Progress>,
    thread: JoinHandle<Result<Report, String>>,
    /// The size the run was asked for, which the page keeps editing while it runs.
    width: u32,
    /// The height the run was asked for.
    height: u32,
    /// The samples it owes.
    total: u32,
    /// The file it writes.
    path: std::path::PathBuf,
    /// When it started, which is what the status row's clock counts from.
    started: Instant,
}

/// The picture the place shows: the last snapshot's radiance and the texture it is toned into.
///
/// The radiance is kept, not the bytes: the exposure is the page's and the curve is the file's, so a
/// person who drags the exposure sees the last picture again instead of waiting for another run.
struct Shown {
    image: Image,
    texture: wgpu::Texture,
    id: egui::TextureId,
    /// The texture's size in pixels, which is the picture's own and the aspect it keeps.
    size: [u32; 2],
    /// The exposure the texture was last toned with.
    exposure: f32,
}

/// The render place: its numbers, its run, its picture and where the last one got to.
pub(crate) struct RenderPlace {
    /// The size, the samples, the exposure and the file, as the person left them.
    settings: Settings,
    /// Where the last finished run got to.
    finished: Stage,
    /// The run in flight, if there is one.
    job: Option<Job>,
    /// The mailbox the run in flight publishes its pictures into.
    snapshots: Option<Arc<Snapshots>>,
    /// The last picture, which outlives the run that made it.
    shown: Option<Shown>,
}

impl RenderPlace {
    /// A place that has never rendered, with the draft preset and no destination.
    pub(crate) fn new() -> Self {
        Self { settings: Settings::draft(), finished: Stage::Idle, job: None, snapshots: None, shown: None }
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

    /// The picture the page draws, if a run has published one.
    pub(crate) fn picture(&self) -> Option<Picture> {
        self.shown.as_ref().map(|shown| Picture { texture: shown.id, width: shown.size[0], height: shown.size[1] })
    }

    /// Where the place stands this frame: the run's own counter while it moves, else what the last
    /// run left.
    pub(crate) fn stage(&mut self) -> Stage {
        match &self.job {
            Some(job) => Stage::Running(Self::run_of(job)),
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

    /// Takes the finished run's answer and the pictures it published, and re-tones the picture when
    /// the exposure moved. Returns whether the place changed, which is what asks for a repaint.
    ///
    /// The order matters: the thread is joined first, because the finished image is published just
    /// before it ends, and the mailbox is drained after — a picture taken before the join would leave
    /// the finished one waiting for the next frame.
    pub(crate) fn poll(&mut self, _ctx: &eframe::egui::Context, render_state: Option<&egui_wgpu::RenderState>) -> bool {
        let mut changed = self.take_finished();
        let Some(state) = render_state else { return changed };
        if let Some(snapshot) = self.snapshots.as_ref().and_then(|snapshots| snapshots.take()) {
            changed |= self.show(&snapshot, state);
        }
        changed | self.retone(state)
    }

    /// Takes the finished run's answer, if it has one, and returns whether the place changed.
    pub(crate) fn take_finished(&mut self) -> bool {
        let Some(job) = &self.job else { return false };
        if !job.thread.is_finished() {
            return false;
        }
        let Some(job) = self.job.take() else { return false };
        let cancelled = Self::run_of(&job);
        self.finished = match job.thread.join() {
            Ok(Ok(report)) => Stage::Done(Run {
                width: report.width,
                height: report.height,
                done: report.samples,
                total: report.samples,
                elapsed: job.started.elapsed(),
                estimate: None,
                path: Some(report.path),
            }),
            Ok(Err(reason)) => {
                if job.progress.cancelled() {
                    Stage::Cancelled(cancelled)
                } else {
                    Stage::Failed(reason)
                }
            }
            Err(_) => Stage::Failed("the render thread stopped without an answer".to_owned()),
        };
        true
    }

    /// What a run has taken so far: the numbers the status row prints while it moves.
    fn run_of(job: &Job) -> Run {
        let done = job.progress.done();
        let elapsed = job.started.elapsed();
        Run {
            width: job.width,
            height: job.height,
            done,
            total: job.total,
            elapsed,
            estimate: remaining(elapsed, done, job.total),
            path: Some(job.path.clone()),
        }
    }

    /// Puts `snapshot` on the screen, into the texture it already has when the size is the same.
    ///
    /// The picture's size is its own: a snapshot of another size gets another texture, because egui
    /// samples one view and a view has one size.
    fn show(&mut self, snapshot: &Snapshot, state: &egui_wgpu::RenderState) -> bool {
        let size = [snapshot.width(), snapshot.height()];
        if size[0] == 0 || size[1] == 0 {
            return false;
        }
        let exposure = self.settings.exposure;
        if self.shown.as_ref().is_none_or(|shown| shown.size != size) {
            self.release(state);
            let texture = picture_texture(state, size[0], size[1]);
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let id = state.renderer.write().register_native_texture(&state.device, &view, wgpu::FilterMode::Linear);
            self.shown = Some(Shown { image: snapshot.image().clone(), texture, id, size, exposure });
        } else if let Some(shown) = self.shown.as_mut() {
            shown.image = snapshot.image().clone();
            shown.exposure = exposure;
        }
        if let Some(shown) = self.shown.as_ref() {
            write_picture(state, shown, exposure);
        }
        true
    }

    /// Tones the picture the place already has with the exposure the page now shows.
    ///
    /// This is what makes the exposure a live control of the last image: no sample is traced, no file
    /// is written, and the picture the person looks at is the picture the next Render would write
    /// (Y6).
    fn retone(&mut self, state: &egui_wgpu::RenderState) -> bool {
        let exposure = self.settings.exposure;
        let Some(shown) = self.shown.as_mut() else { return false };
        if (shown.exposure - exposure).abs() <= f32::EPSILON {
            return false;
        }
        shown.exposure = exposure;
        write_picture(state, shown, exposure);
        true
    }

    /// Hands the texture back to the window's renderer, so a replaced picture leaves none behind.
    fn release(&mut self, state: &egui_wgpu::RenderState) {
        if let Some(shown) = self.shown.take() {
            state.renderer.write().free_texture(&shown.id);
        }
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
        let settings = RenderSettings::new(request, self.settings.exposure, path.clone())
            .and_then(|settings| settings.with_environment(self.settings.studio / 100.0))
            .map_err(|error| error.to_string())?;
        let progress = Arc::new(Progress::new(request.samples()));
        let snapshots = Arc::new(Snapshots::new());
        self.snapshots = Some(Arc::clone(&snapshots));
        let device = render_state.device.clone();
        let queue = render_state.queue.clone();
        let owned = Arc::clone(&progress);
        let published = Arc::clone(&snapshots);
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
                    &published,
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
                self.job = Some(Job {
                    progress,
                    thread,
                    width: request.width(),
                    height: request.height(),
                    total: request.samples(),
                    path,
                    started: Instant::now(),
                });
                Ok(())
            }
            Err(error) => Err(format!("the render thread could not be started: {error}")),
        }
    }

    /// Waits for the run in flight to answer and takes its last picture, which a headless test does
    /// instead of drawing frames.
    #[cfg(test)]
    pub(crate) fn wait(&mut self, ctx: &eframe::egui::Context, render_state: Option<&egui_wgpu::RenderState>) {
        // A test has no frames to wait for: the run's own end is the wait, and one poll takes the
        // answer and the finished picture together.
        while self.job.as_ref().is_some_and(|job| !job.thread.is_finished()) {
            std::thread::sleep(Duration::from_millis(2));
        }
        self.poll(ctx, render_state);
    }
}

/// The longest estimate the status row prints: a day.
///
/// Longer than that is a number no person acts on — a run that would take a week is answered by
/// stopping it — and a clock that big is a computation that overflowed rather than an estimate.
const LONGEST_ESTIMATE: Duration = Duration::from_secs(24 * 60 * 60);

/// How much longer a run of `total` samples needs, from the samples it has taken and the time it
/// spent on them; `None` before the first sample, because a duration with no sample behind it is a
/// guess, and `None` for an estimate past [`LONGEST_ESTIMATE`], which is not one either.
fn remaining(elapsed: Duration, done: u32, total: u32) -> Option<Duration> {
    if done == 0 || done >= total {
        return None;
    }
    let left = f64::from(total - done) / f64::from(done);
    let seconds = elapsed.as_secs_f64() * left;
    // A clock that cannot be printed is no estimate: a hostile duration becomes `None`, never a panic
    // (`AGENTS.md` §4.4).
    if !seconds.is_finite() || seconds > LONGEST_ESTIMATE.as_secs_f64() {
        return None;
    }
    Duration::try_from_secs_f64(seconds.max(0.0)).ok()
}

/// The texture a picture of this size is drawn through.
///
/// `Rgba8Unorm` is the format `register_native_texture` takes: the transfer function has already run
/// on the host (`Image::to_rgba8`), so the texture carries display values and the interface samples
/// them as they are — the same contract the viewport's own target has.
fn picture_texture(state: &egui_wgpu::RenderState, width: u32, height: u32) -> wgpu::Texture {
    state.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Z3L render place picture"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

/// Writes `shown`'s radiance into its texture, through the curve the file is written with.
fn write_picture(state: &egui_wgpu::RenderState, shown: &Shown, exposure: f32) {
    let [width, height] = shown.size;
    let pixels = shown.image.to_rgba8(exposure);
    state.queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &shown.texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &pixels,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4 * width), rows_per_image: Some(height) },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
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
        assert!(place.picture().is_none(), "a place that has not drawn anything shows no picture");
    }

    /// A floor and a standing panel, both pale, lit by the frame's key light: a picture with light
    /// in it, which is what an exposure can act on.
    fn fixture() -> (scene::SceneFrame, scene::Camera) {
        use std::sync::Arc;

        let quad = |side: f64| {
            Arc::new(geom::MeshData {
                positions: vec![
                    [0.0, 0.0, 0.0],
                    [side as f32, 0.0, 0.0],
                    [side as f32, side as f32, 0.0],
                    [0.0, side as f32, 0.0],
                ],
                normals: Vec::new(),
                indices: vec![0, 1, 2, 0, 2, 3],
            })
        };
        let mut frame = scene::SceneFrame::default();
        // The panel: the XY quad stood up on the X axis, then moved to y = 2000.
        let panel = geom::DMat4::from_scale_rotation_translation(
            geom::DVec3::ONE,
            geom::DQuat::from_rotation_x(std::f64::consts::FRAC_PI_2),
            geom::DVec3::new(-600.0, 2000.0, -600.0),
        );
        frame.push(scene::Drawable::new(
            model::Id::new(1).expect("a non-zero id"),
            quad(1200.0),
            panel,
            [0.9, 0.9, 0.88, 1.0],
        ));
        // The floor: the same quad lying in the z = 0 plane.
        let floor = geom::DMat4::from_translation(geom::DVec3::new(-1500.0, -1500.0, 0.0));
        frame.push(scene::Drawable::new(
            model::Id::new(2).expect("a non-zero id"),
            quad(3000.0),
            floor,
            [0.6, 0.6, 0.58, 1.0],
        ));
        frame.lighting.key_direction = geom::DVec3::new(0.0, -0.7, 0.7).try_normalize().unwrap_or(geom::DVec3::Z);
        frame.lighting.key_color = [1.0, 1.0, 1.0];
        let camera = scene::Camera::new(geom::DVec3::new(0.0, 2000.0, 300.0), 3000.0).expect("a camera");
        (frame, camera)
    }

    #[test]
    fn a_render_from_the_place_writes_its_image_and_keeps_the_picture_of_it() {
        // The whole path through the shell's half: a run on its own thread over the frame the
        // viewport draws, a file on disk, the place's own report and the picture the page draws. A
        // machine with no adapter says so instead of failing, like every device test in this tree
        // (`AGENTS.md` §4.7).
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
        place.wait(&ctx, Some(&state));
        assert!(!place.is_running());
        match place.stage() {
            Stage::Done(run) => {
                assert_eq!(run.path, Some(path.clone()));
                assert_eq!((run.width, run.height, run.done), (48, 32, 2));
                assert!(run.elapsed > Duration::ZERO, "the row has a clock");
            }
            other => panic!("the run reported {other:?}"),
        }
        let bytes = std::fs::read(&path).expect("the image is on disk");
        assert!(bytes.len() > 100, "a PNG is not empty: {}", bytes.len());
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "the file is a PNG");
        // The picture the page draws is the one the file holds, at the picture's own size.
        let picture = place.picture().expect("the finished picture is on the page");
        assert_eq!((picture.width, picture.height), (48, 32));

        // The exposure is a live control of the last picture: the same radiance, toned again with
        // the curve the file was written with, and no second run behind it (Y6).
        let shown = place.shown.as_ref().expect("the place keeps the picture's radiance");
        // The picture has light in it, which is what an exposure is a control of.
        let brightest = (0..32)
            .flat_map(|y| (0..48).map(move |x| (x, y)))
            .filter_map(|(x, y)| shown.image.radiance_at(x, y))
            .map(|pixel| pixel[0])
            .fold(0.0f32, f32::max);
        assert!(brightest > 0.05, "the fixture renders a lit picture: {brightest}");
        assert_ne!(shown.image.to_rgba8(1.0), shown.image.to_rgba8(4.0), "an exposure changes the pixels");
        place.settings().exposure = 4.0;
        assert!(place.poll(&ctx, Some(&state)), "the exposure re-toned the picture");
        assert_eq!(place.picture(), Some(picture), "the same texture, toned again");
        assert!(!place.is_running(), "a re-tone is not a run");
        assert!(!place.poll(&ctx, Some(&state)), "and the same exposure is not a second tone");
        place.release(&state);
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

    #[test]
    fn an_estimate_needs_a_sample_behind_it() {
        assert_eq!(remaining(Duration::from_secs(4), 0, 64), None, "no sample, no estimate");
        assert_eq!(remaining(Duration::from_secs(4), 64, 64), None, "a finished run needs no time");
        assert_eq!(remaining(Duration::from_secs(4), 16, 64), Some(Duration::from_secs(12)));
        assert_eq!(remaining(Duration::MAX, 1, 2), None, "a clock that cannot be printed is no estimate");
        assert_eq!(
            remaining(Duration::from_secs(4), 1, 1_000_000),
            None,
            "a week of rendering is not an estimate a person acts on"
        );
    }
}
