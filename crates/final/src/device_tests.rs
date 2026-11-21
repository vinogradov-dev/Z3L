//! The final render's tests that need a device, beside the one that only needs a parser.
//!
//! Most of this crate is checked without a GPU: the BVH, the intersection test, the camera mapping and
//! the tone map are Rust values with Rust tests. Three things are not: the WGSL integrator has to mean
//! the same as the Rust it transcribes, a render has to end in a file, and cancelling has to leave the
//! previous file alone. The shader's syntax is parsed by `naga` on every machine, and the render tests
//! take a device when the machine has one and say so when it does not — a test that silently passed on
//! a machine with no adapter would be a test of nothing (`AGENTS.md` §4.7).
//!
//! The fixture is the smallest scene that shows the contract: a floor and a panel lit by the key
//! light, seen from a pinned camera. The image is small — 64 by 48 — because what is asserted is which
//! pixel is lit, not what colour a cabinet is; a minute-long render makes a test nobody runs.

#![allow(clippy::expect_used)]

use std::sync::Arc;

use geom::{DMat4, DQuat, DVec3, MeshData};
use scene::{Camera, Drawable, SceneFrame};

use crate::job::{Input, Settings, run};
use crate::view::aspect;
use crate::{FinalError, Progress, RENDER_FINAL_ID, Request, Tracer, faces_of};

/// A device and a queue with no surface behind them, or `None` when the machine has neither.
fn gpu_device() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).ok()?;
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()
}

/// One quad on the XY plane, `side` millimetres square, facing +Z.
fn quad(side: f32) -> Arc<MeshData> {
    Arc::new(MeshData {
        positions: vec![[0.0, 0.0, 0.0], [side, 0.0, 0.0], [side, side, 0.0], [0.0, side, 0.0]],
        normals: Vec::new(),
        indices: vec![0, 1, 2, 0, 2, 3],
    })
}

fn id(value: u64) -> model::Id {
    model::Id::new(value).expect("a non-zero id")
}

/// The world point a test looks for in the picture: the panel's own centre.
const PANEL_CENTRE: DVec3 = DVec3::new(0.0, 2000.0, 300.0);

/// A floor and a standing panel, both pale, lit by the frame's key light and seen from a pinned camera.
///
/// The world is Z-up, so the floor is the `z = 0` plane and the panel stands in the `y = 2000` plane
/// facing the camera. Anything above the panel is background, which is what makes "the empty corner is
/// black" a statement about this scene rather than about the curve.
fn fixture() -> (SceneFrame, Camera) {
    let mut frame = SceneFrame::default();
    // The panel: the XY quad stood up on the X axis, then moved to y = 2000.
    let panel = DMat4::from_scale_rotation_translation(
        DVec3::ONE,
        DQuat::from_rotation_x(std::f64::consts::FRAC_PI_2),
        DVec3::new(-600.0, 2000.0, -600.0),
    );
    frame.push(Drawable::new(id(1), quad(1200.0), panel, [0.9, 0.9, 0.88, 1.0]));
    // The floor: the same quad lying in the z = 0 plane, wide enough to fill the lower part of the view.
    let floor = DMat4::from_translation(DVec3::new(-1500.0, -1500.0, 0.0));
    frame.push(Drawable::new(id(2), quad(3000.0), floor, [0.6, 0.6, 0.58, 1.0]));
    // The key light comes from behind the camera and above, so both surfaces face it.
    frame.lighting.key_direction = DVec3::new(0.0, -0.7, 0.7).try_normalize().unwrap_or(DVec3::Z);
    frame.lighting.key_color = [1.0, 1.0, 1.0];
    let camera = pinned_camera();
    (frame, camera)
}

/// A camera at an explicit eye and basis, the way an authored camera pins one.
fn pinned_camera() -> Camera {
    let eye = DVec3::new(0.0, -3000.0, 1200.0);
    let forward = (PANEL_CENTRE - eye).try_normalize().unwrap_or(DVec3::Y);
    let right = forward.cross(DVec3::Z).try_normalize().unwrap_or(DVec3::X);
    let up = right.cross(forward).try_normalize().unwrap_or(DVec3::Z);
    Camera::default().looked_from(eye, right, up, forward)
}

/// The image pixel a world point lands on, or `None` when it is behind the camera.
fn pixel_of(camera: Camera, point: DVec3, request: Request) -> Option<(u32, u32)> {
    let ndc = camera.project_ndc(point, aspect(request.width(), request.height())?)?;
    let x = (ndc.x + 1.0) * 0.5 * f64::from(request.width());
    let y = (1.0 - ndc.y) * 0.5 * f64::from(request.height());
    Some((
        x.floor().clamp(0.0, f64::from(request.width() - 1)) as u32,
        y.floor().clamp(0.0, f64::from(request.height() - 1)) as u32,
    ))
}

/// The eight-bit pixels of the PNG a render wrote, row-major from the top.
fn read_png(path: &std::path::Path) -> (u32, u32, Vec<[u8; 4]>) {
    let file = std::fs::File::open(path).expect("the written file");
    let decoder = png::Decoder::new(std::io::BufReader::new(file));
    let mut reader = decoder.read_info().expect("the file is a PNG");
    let mut buffer = vec![0u8; reader.output_buffer_size().expect("a bounded image")];
    let info = reader.next_frame(&mut buffer).expect("the frame decodes");
    let pixels = buffer.as_chunks::<4>().0.to_vec();
    (info.width, info.height, pixels)
}

/// A fresh directory and an image path inside it for one test.
///
/// One directory per test, because `testkit::scratch_dir` empties the directory it names and every
/// test in this binary shares one process id: two tests that shared a name would delete each other's
/// file halfway through.
fn scratch_png(name: &str) -> std::path::PathBuf {
    testkit::scratch_dir(&format!("final-{name}")).join("image.png")
}

#[test]
fn the_integrator_parses_as_wgsl_before_any_device_sees_it() {
    let source = include_str!("gpu/shader.wgsl");
    let module = naga::front::wgsl::parse_str(source).expect("the integrator is valid WGSL");
    let mut validator =
        naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all());
    validator.validate(&module).expect("the integrator validates");
    // The one thing a parser cannot see: the entry point the pipeline names must exist.
    assert!(module.entry_points.iter().any(|entry| entry.name == "main"), "the pipeline names `main`");
}

#[test]
fn a_render_twice_at_one_revision_builds_one_tree() {
    let (frame, _camera) = fixture();
    let Some((device, _queue)) = gpu_device() else {
        println!("this machine has no GPU adapter to key a tree by revision with");
        return;
    };
    let mut tracer = Tracer::new(&device).expect("the integrator compiles");
    assert!(tracer.prepare(&device, &frame, 7).expect("the frame uploads"), "the first upload builds a tree");
    assert!(!tracer.prepare(&device, &frame, 7).expect("the same revision again"), "one revision, one tree");
    assert!(tracer.prepare(&device, &frame, 8).expect("a changed revision"), "a changed revision rebuilds");
    assert_eq!(tracer.build_count(), 2, "one tree per revision, not one per render");
    assert_eq!(tracer.revision(), Some(8));
    assert_eq!(faces_of(&frame).expect("the fixture flattens").len(), 4, "two quads are four faces");
}

#[test]
fn a_pixel_is_lit_where_the_viewport_projects_the_panel_it_sees() {
    let (frame, camera) = fixture();
    let Some((device, queue)) = gpu_device() else {
        println!("this machine has no GPU adapter to match a camera with");
        return;
    };
    let path = scratch_png("camera");
    let request = Request::new(64, 48, 16).expect("a small render");
    let settings = Settings::default_for(request, &path).expect("a render setting");
    let mut tracer = Tracer::new(&device).expect("the integrator compiles");
    let progress = Progress::new(request.samples());
    run(&mut tracer, &device, &queue, Input { frame: &frame, camera, revision: 3 }, &settings, &progress)
        .expect("the fixture renders");

    let (width, height, pixels) = read_png(&path);
    assert_eq!((width, height), (64, 48), "the file is the size that was asked for");
    let Some((x, y)) = pixel_of(camera, PANEL_CENTRE, request) else {
        panic!("the panel's centre is in front of the camera");
    };
    let index = (y * width + x) as usize;
    let lit = pixels.get(index).copied().expect("the projected pixel is inside the image");
    assert!(lit[0] > 20, "the panel's centre lands at ({x}, {y}) and is lit: {lit:?}");
    // One pixel of slop on each side: the claim is about the camera, not about a rounding rule.
    for offset in [index.saturating_sub(1), index + 1] {
        let neighbour = pixels.get(offset).copied().expect("a neighbour pixel");
        assert!(neighbour[0] > 10, "the panel is wider than one pixel: {neighbour:?}");
    }
    // The top-left corner of the image is above the panel and above the floor: nothing is there.
    assert_eq!(pixels.first().copied(), Some([0, 0, 0, 255]), "the empty corner is black");
}

#[test]
fn the_entry_point_writes_one_png_and_reports_what_it_wrote() {
    let (frame, camera) = fixture();
    let Some((device, queue)) = gpu_device() else {
        println!("this machine has no GPU adapter to write a final image with");
        return;
    };
    let path = scratch_png("fixture");
    let request = Request::new(48, 32, 4).expect("a small render");
    let settings = Settings::default_for(request, &path).expect("a render setting");
    let mut tracer = Tracer::new(&device).expect("the integrator compiles");
    // A render of a tracer that was never prepared is a sentence, not an empty picture.
    let unprepared = Progress::new(request.samples());
    let refusal = tracer
        .render(&device, &queue, camera, &frame.lighting, request, &unprepared)
        .expect_err("nothing was uploaded");
    assert!(matches!(refusal, FinalError::Device { .. }), "{refusal}");

    let progress = Progress::new(request.samples());
    let report = run(&mut tracer, &device, &queue, Input { frame: &frame, camera, revision: 1 }, &settings, &progress)
        .expect("the fixture renders");
    assert_eq!(report.path, path);
    assert_eq!((report.width, report.height), (48, 32));
    assert_eq!(report.samples, 4);
    assert_eq!(progress.done(), 4, "every sample was taken and counted");
    assert!((progress.fraction() - 1.0).abs() < 1e-6);
    let (width, height, pixels) = read_png(&path);
    assert_eq!((width, height), (48, 32));
    assert!(pixels.iter().any(|pixel| pixel[0] > 0), "the image is not empty");

    // The same frame and camera at the same revision reuse the tree.
    let second = Progress::new(request.samples());
    run(&mut tracer, &device, &queue, Input { frame: &frame, camera, revision: 1 }, &settings, &second)
        .expect("a second render of the same revision");
    assert_eq!(tracer.build_count(), 1, "the second render rebuilt no tree");
}

#[test]
fn a_cancelled_render_writes_nothing_and_leaves_the_old_file() {
    let (frame, camera) = fixture();
    let Some((device, queue)) = gpu_device() else {
        println!("this machine has no GPU adapter to cancel a render on");
        return;
    };
    let path = scratch_png("cancelled");
    // A picture from an earlier render stands at the destination: cancelling must not disturb it.
    let previous = b"the picture that was there before".to_vec();
    std::fs::write(&path, &previous).expect("the earlier picture is written");

    let request = Request::new(48, 32, 8).expect("a render to cancel");
    let settings = Settings::default_for(request, &path).expect("a render setting");
    let mut tracer = Tracer::new(&device).expect("the integrator compiles");
    let progress = Progress::new(request.samples());
    progress.cancel();
    let error = run(&mut tracer, &device, &queue, Input { frame: &frame, camera, revision: 2 }, &settings, &progress)
        .expect_err("a cancelled render does not finish");
    assert!(matches!(error, FinalError::Cancelled { samples: 0, total: 8 }), "{error}");
    assert!(error.is_cancelled());
    assert_eq!(std::fs::read(&path).expect("the old picture is still there"), previous, "the file is untouched");
    assert_eq!(progress.done(), 0, "no sample was traced");

    // The session starts again: the cancel is a value, not a broken tracer.
    let fresh = Progress::new(request.samples());
    run(&mut tracer, &device, &queue, Input { frame: &frame, camera, revision: 2 }, &settings, &fresh)
        .expect("a second attempt after a cancel renders");
    assert_ne!(std::fs::read(&path).expect("the new picture"), previous, "the new image replaced it");
    assert_eq!(fresh.done(), request.samples());
}

#[test]
fn the_entry_point_names_itself_and_a_mismatched_counter_is_a_sentence() {
    assert_eq!(RENDER_FINAL_ID, "render.final");
    let error = FinalError::Progress { total: 2, samples: 4 };
    assert!(error.to_string().contains('2') && error.to_string().contains('4'), "{error}");
}
