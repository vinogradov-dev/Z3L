//! `z3l render`: one final image of a saved scene, with no window.
//!
//! The image a catalogue needs is one still picture of the file a workshop saved, and producing it
//! must not need a person, a window or a GPU surface: the subcommand reads the scene, evaluates the
//! frame the viewport would draw, takes the active authored camera when the document has one and a
//! fitted view when it does not, and hands the frame to `final`'s one entry point — `render.final`
//! (ADR 0210). Nothing here mutates the document: the document is read, the image is written, and the
//! scene file is left exactly as it was.
//!
//! The frame is the viewport's, materials and all: the workshop's render library is read from the
//! workspace folder the window reads, so a panel cut from a named board wears its decor and its
//! finish in the image exactly as it does on screen (Y7, Y8, Y9). A library that cannot be read is a
//! line of the report and not a refusal — every board then keeps the shell's placeholder tone, which
//! is what a workshop that named nothing sees (Y8).
//!
//! The progress and the cancel the entry point carries are visible here as themselves: the samples
//! are counted by a `Progress` the tracer advances, a watcher thread prints the percentage as it
//! moves, and the same value is what a window would read between frames and stop from a button. The
//! command line has no button, so a run finishes or fails; stopping it belongs to the interface, which
//! will pass the same `Progress`.
//!
//! Refuses: a scene that does not read, a size, a sample count or an exposure outside `final`'s own
//! ranges (the refusal names the field and the limit), and a machine with no adapter that can trace —
//! each a sentence the shell prints, never a black file.
//!
//! Changes when: what a headless render takes from the line, or where it writes, changes.

use std::sync::atomic::{AtomicBool, Ordering};

use eframe::wgpu;
// The crate is named `final`, which Rust reserves, so every path into it is written raw. The package
// keeps the bare name the layer map and the plan use (ADR 0135, ADR 0210).
use engine::Session;
use r#final::{FinalError, Input, Progress, Request, Settings, Snapshots, Tracer, run};
use kernel_truck::TruckKernel;
use materials::Store;
use mesh::EvaluationCache;
use project::SceneFile;
use ui::ViewFlags;

use super::{CliError, RenderArgs};

/// The image size a run makes when the line does not name one: 1920 by 1080 is a catalogue plate at
/// the size a workshop screen shows, and it costs a fraction of the A4 print the interface asks for.
pub const DEFAULT_SIZE: (u32, u32) = (1920, 1080);

/// The samples per pixel a run takes when the line does not name a count.
///
/// Four is a draft: it shows the camera, the light and the silhouette in a second or two, and one
/// sample is not enough to see anything at a tie. A printable image names its own count.
pub const DEFAULT_SAMPLES: u32 = 4;

/// The session and the frame the viewport would draw of `document`, through `store`.
///
/// One function for the two halves a workshop expects to agree: the session carries the render
/// library and the frame is built with the folder its pictures live in, so a panel that names a
/// record arrives with that record's decor and finish instead of the placeholder tone (Y7, Y8, Y9).
/// A folder that cannot be read is the report's line and an empty library, never a failed run.
fn frame_of(
    document: model::Project,
    store: Option<&Store>,
) -> Result<(Session, crate::scene_build::EvaluatedScene, Option<String>), CliError> {
    let workshop = crate::materials_watch::open(store);
    let session = super::recipe::session_of(document)?.with_materials(workshop.library);
    let mut cache = EvaluationCache::new();
    let mut kernel = TruckKernel::new();
    let mut decors = scene::decor::Decors::new();
    let materials = crate::scene_build::FrameMaterials::new(session.materials(), store, &mut decors);
    let evaluated =
        crate::scene_build::evaluate_with(&mut cache, &mut kernel, &session, &ViewFlags::default(), None, materials)
            .map_err(CliError::Render)?;
    Ok((session, evaluated, workshop.refusal))
}

/// Renders a saved scene to one PNG and returns the lines to print.
pub fn perform(args: &RenderArgs) -> Result<Vec<String>, CliError> {
    let scene = SceneFile::read(&args.scene).map_err(CliError::Scene)?;
    // The workshop's own folder, the same one the window reads: the library and the pictures beside
    // the settings file, never a copy taken at the last save.
    let store = crate::settings_store::SettingsStore::for_workspace().materials();
    let (session, evaluated, refusal) = frame_of(scene.into_document(), store.as_ref())?;
    let fallback = crate::scene_build::home_camera(evaluated.bounds).map_err(CliError::Render)?;
    let camera = match args.camera {
        super::CameraChoice::Scene => crate::scene_build::scene_camera(session.project(), fallback),
        super::CameraChoice::Fit => fallback,
    };
    let revision = session.project().revision().get();
    let request = Request::new(args.width, args.height, args.samples).map_err(describe)?;
    let settings = Settings::new(request, args.exposure, &args.out)
        .and_then(|settings| settings.with_environment(args.studio / 100.0))
        .map(|settings| if args.no_denoise { settings.without_denoise() } else { settings })
        .map_err(describe)?;
    let (device, queue) = device()?;
    let mut tracer = Tracer::new(&device).map_err(describe)?;
    let progress = Progress::new(request.samples());
    // The command line shows a percentage, not a picture: a run publishes into a mailbox nobody
    // takes, which is exactly the case the replace rule is for (Y6).
    let snapshots = Snapshots::new();
    let stop = AtomicBool::new(false);
    let progress_lines = std::thread::scope(|scope| {
        scope.spawn(|| watch(&progress, &stop));
        let result = run(
            &mut tracer,
            &device,
            &queue,
            Input { frame: &evaluated.frame, camera, revision },
            &settings,
            &progress,
            &snapshots,
        );
        stop.store(true, Ordering::Relaxed);
        result
    });
    let report = progress_lines.map_err(describe)?;
    let mut lines = Vec::new();
    if let Some(refusal) = refusal {
        // Not a refusal of the run: the scene is still one image, and every board in it wears the
        // placeholder tone the shell starts from.
        lines.push(format!("the workshop's material library could not be read, so no board is faced: {refusal}"));
    }
    if !evaluated.failures.is_empty() {
        // A node the kernel refuses is not in the picture; saying so beats an image nobody can explain.
        lines.push(format!(
            "{} node(s) of this scene had no representable geometry and are not in the image",
            evaluated.failures.len()
        ));
        for failure in &evaluated.failures {
            lines.push(format!("  {}: {}", failure.id, failure.message));
        }
    }
    let bytes = std::fs::metadata(&report.path).map(|metadata| metadata.len()).unwrap_or(0);
    lines.push(format!(
        "{}: {} x {}, {} sample(s), {} bytes",
        report.path.display(),
        report.width,
        report.height,
        report.samples,
        bytes
    ));
    Ok(lines)
}

/// Prints the run's progress once per whole percent until the tracer is done or stopped.
///
/// One thread and one line per percent: a render is minutes long, and a command that prints nothing
/// for a minute reads as a command that hung. The percentage is the entry point's own counter — the
/// number a progress bar in the window draws — so the terminal and a surface never disagree.
fn watch(progress: &Progress, stop: &AtomicBool) {
    let mut shown = u32::MAX;
    while !stop.load(Ordering::Relaxed) {
        let percent = (progress.fraction() * 100.0).floor() as u32;
        if percent != shown && percent > 0 {
            eprintln!("render: {percent}% ({} of {} samples)", progress.done(), progress.total());
            shown = percent;
        }
        if progress.cancelled() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(120));
    }
}

/// A device and a queue with no surface behind them, or a sentence that says why there is none.
fn device() -> Result<(wgpu::Device, wgpu::Queue), CliError> {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        .map_err(|error| CliError::Render(format!("this machine has no device to render with: {error}")))?;
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
        .map_err(|error| CliError::Render(format!("this machine's device cannot render: {error}")))
}

/// The sentence a refusal from `final` carries, which is already written for a person.
fn describe(error: FinalError) -> CliError {
    CliError::Render(error.to_string())
}

/// A size from `1920x1080` or `1920X1080`, refused rather than guessed when it is not two numbers.
pub fn parse_size(text: &str) -> Result<(u32, u32), CliError> {
    let (width, height) = text
        .split_once(['x', 'X'])
        .ok_or_else(|| CliError::Usage(format!("`{text}` is not a size; name one as `1920x1080`")))?;
    let width = width
        .trim()
        .parse::<u32>()
        .map_err(|_| CliError::Usage(format!("`{text}` is not a size; name one as `1920x1080`")))?;
    let height = height
        .trim()
        .parse::<u32>()
        .map_err(|_| CliError::Usage(format!("`{text}` is not a size; name one as `1920x1080`")))?;
    Ok((width, height))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use materials::{Base, Decor, Library, Material, MaterialId, Turns};
    use model::{Container, PanelNominalSize, Placement, Project};
    use units::Length;

    use super::*;

    #[test]
    fn a_size_is_two_numbers_and_anything_else_is_a_sentence() {
        assert_eq!(parse_size("1920x1080").expect("a size"), (1920, 1080));
        assert_eq!(parse_size("64X48").expect("a capital X"), (64, 48));
        for broken in ["1920", "1920x", "x1080", "widexhigh", "-1x10", "1920,1080"] {
            let error = parse_size(broken).expect_err("a broken size is refused");
            assert!(error.to_string().contains(broken), "{broken}: {error}");
        }
    }

    /// The demo cabinet, rendered by the real command line into one picture a person can look at.
    ///
    /// Ignored because it needs a GPU adapter and writes a file; run it by hand to see what the
    /// pipeline does with real geometry — `cargo test -p app --bin z3l -- --ignored demo_cabinet`.
    #[test]
    #[ignore = "needs a GPU adapter; writes target/final/demo-cabinet.png"]
    fn the_demo_cabinet_renders_to_one_png() {
        let directory = testkit::scratch_dir("render-demo");
        let scene = directory.join("demo.z3l");
        let session = crate::scene_build::demo_session().expect("the demo scene");
        project::SceneFile::new(session.project().clone()).write(&scene).expect("the demo scene is written");
        let out = directory.join("demo-cabinet.png");
        let args = RenderArgs {
            scene,
            out: out.clone(),
            width: 960,
            height: 720,
            samples: 64,
            exposure: 1.0,
            studio: 30.0,
            camera: super::super::CameraChoice::Fit,
            no_denoise: false,
        };
        let lines = perform(&args).expect("the demo renders");
        assert!(out.is_file(), "{}", lines.join("\n"));
        let bytes = std::fs::read(&out).expect("the picture is readable");
        assert!(bytes.len() > 1000, "a picture of a cabinet is not three bytes: {}", bytes.len());
    }

    /// The document a scene file would hold: one shelf that names the record `slot`.
    fn shelf(slot: model::MaterialRef) -> Project {
        let root = Container::new(model::Id::new(1).expect("the root"), "Root", Placement::IDENTITY, Vec::new());
        let mut project = Project::new(root).expect("a project");
        let root = project.root().id();
        let size = PanelNominalSize::new(
            Length::from_millimetres(600).expect("a length"),
            Length::from_millimetres(400).expect("a width"),
        )
        .expect("a size");
        let panel = project
            .add_panel(
                root,
                "Shelf",
                size,
                None,
                Length::from_millimetres(18).expect("a thickness"),
                Placement::IDENTITY,
            )
            .expect("a shelf");
        project.set_node_material(panel, Some(slot)).expect("the material slot");
        project
    }

    /// A workshop folder of one record faced with a two-texel picture.
    fn folder(name: &str) -> Store {
        let store = Store::at(testkit::scratch_dir(name));
        let decor = Decor::new("textures/board.png", Length::from_millimetres(300).expect("a size"), Turns::ALONG)
            .expect("a decor");
        let base = Base::new([1.0; 3], Some(decor), 0.6, 0.0, 1.5, 0.0, [0.0; 3]).expect("a board");
        let record = Material::of_base(MaterialId::new(1).expect("a material"), "Test board", base).expect("a record");
        store.write(&Library::new([record]).expect("one record")).expect("the library file");
        let texels: [u8; 8] = [200, 120, 60, 255, 60, 40, 20, 255];
        std::fs::create_dir_all(store.textures()).expect("the textures folder");
        testkit::write_png(&store.textures().join("board.png"), 2, 1, &texels).expect("the picture");
        store
    }

    #[test]
    fn the_headless_frame_wears_the_workshops_decor() {
        // The render's own clause: the image a catalogue is given is the frame the viewport draws, so a
        // panel that names a record of the workshop's library arrives faced with that record's picture
        // and finish, and not with the placeholder tone the shell starts from (Y7, Y8, Y9).
        let slot = model::MaterialRef::new(1).expect("a material");
        let (_, evaluated, refusal) = frame_of(shelf(slot), Some(&folder("cli-render-decor"))).expect("a frame");
        assert!(refusal.is_none(), "the workshop's library was read: {refusal:?}");
        assert!(
            evaluated.frame.drawables.first().expect("the shelf draws").decor.is_some(),
            "the picture reaches the frame"
        );

        // A workspace with no workshop folder is the ordinary answer of a session that named nothing:
        // the frame is still one image, drawn in the tone every board was.
        let (_, plain, _) = frame_of(shelf(slot), None).expect("a frame");
        assert!(plain.frame.drawables.first().expect("the shelf draws").decor.is_none());
    }

    #[test]
    fn the_defaults_are_a_plate_a_screen_shows_and_a_draft_that_renders_in_seconds() {
        let (width, height) = DEFAULT_SIZE;
        assert!(Request::new(width, height, DEFAULT_SAMPLES).is_ok(), "the defaults are a legal request");
        assert_eq!(DEFAULT_SAMPLES, 4);
    }
}
