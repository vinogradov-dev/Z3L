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

// ---------------------------------------------------------------------------------------------------
// The clauses Y2 owes: a furnace, a converged reference, and a pane over a patterned floor.
// ---------------------------------------------------------------------------------------------------

/// The linear radiance of one accumulation, per pixel, summed over its samples and divided once.
fn linear_of(accumulation: &crate::Accumulation) -> Vec<[f32; 3]> {
    let samples = accumulation.samples();
    let divisor = samples.max(1) as f32;
    accumulation.sums().iter().map(|sum| [sum[0] / divisor, sum[1] / divisor, sum[2] / divisor]).collect()
}

/// A triangle soup from positions and indices, as the frame's drawables carry.
fn mesh(positions: Vec<[f32; 3]>, indices: Vec<u32>) -> Arc<MeshData> {
    Arc::new(MeshData { positions, normals: Vec::new(), indices })
}

/// A sphere of `radius` millimetres at the origin, its normal from the winding.
///
/// A convex body is the furnace's simplest subject: a ray from its surface leaves it, so a white
/// sphere under a uniform environment has nothing to reflect and must vanish into it.
fn sphere(radius: f32, segments: u32) -> Arc<MeshData> {
    let rings = segments.max(6);
    let mut positions = Vec::new();
    for ring in 0..=rings {
        let theta = std::f32::consts::PI * ring as f32 / rings as f32;
        let (sin_theta, cos_theta) = theta.sin_cos();
        for segment in 0..=segments {
            let phi = 2.0 * std::f32::consts::PI * segment as f32 / segments as f32;
            let (sin_phi, cos_phi) = phi.sin_cos();
            positions.push([radius * sin_theta * cos_phi, radius * sin_theta * sin_phi, radius * cos_theta]);
        }
    }
    let stride = segments + 1;
    let mut indices = Vec::new();
    for ring in 0..rings {
        for segment in 0..segments {
            let first = ring * stride + segment;
            let next = first + stride;
            // Counter-clockwise seen from outside, so the winding gives the outward normal.
            indices.extend_from_slice(&[first, next, first + 1]);
            indices.extend_from_slice(&[first + 1, next, next + 1]);
        }
    }
    mesh(positions, indices)
}

/// One horizontal quad at `z`, from `(x0, y0)` to `(x1, y1)`, facing up.
fn plate(x0: f32, y0: f32, x1: f32, y1: f32, z: f32) -> Arc<MeshData> {
    mesh(vec![[x0, y0, z], [x1, y0, z], [x1, y1, z], [x0, y1, z]], vec![0, 1, 2, 0, 2, 3])
}

/// The frame of one final render of `frame` at `camera`, as an accumulation a test can measure.
fn trace_linear(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    frame: &SceneFrame,
    camera: Camera,
    request: Request,
) -> crate::Accumulation {
    let mut tracer = Tracer::new(device).expect("the integrator compiles");
    tracer.prepare(device, frame, 91).expect("the frame uploads");
    let progress = Progress::new(request.samples());
    tracer.render(device, queue, camera, &frame.lighting, request, &progress).expect("the frame renders")
}

#[test]
fn a_white_sphere_in_a_white_room_disappears() {
    // The furnace, which is the one test that catches every energy error at once: with a reflectance
    // of one and a uniform environment, the sphere must return exactly the environment's radiance, so
    // the picture is flat. A grey sphere returns exactly its albedo times that radiance, so the same
    // test reads the albedo out of the image rather than trusting it.
    let Some((device, queue)) = gpu_device() else {
        println!("this machine has no GPU adapter to hold a sphere in a white room");
        return;
    };
    let camera = Camera::new(DVec3::ZERO, 2000.0).expect("a camera");
    let request = Request::new(32, 32, 8).expect("a small render");
    for (albedo, name) in [([1.0f32; 3], "white"), ([0.5, 0.5, 0.5], "grey")] {
        let mut frame = SceneFrame::default();
        frame.push(Drawable::new(id(1), sphere(400.0, 24), DMat4::IDENTITY, [albedo[0], albedo[1], albedo[2], 1.0]));
        // No lamp at all: the environment is the only light in the room.
        frame.lighting.key_color = [0.0; 3];
        frame.lighting.points.clear();
        frame.lighting.environment = [1.0, 1.0, 1.0];
        let accumulation = trace_linear(&device, &queue, &frame, camera, request);
        let linear = linear_of(&accumulation);
        let centre = linear.get(16 * 32 + 16).copied().expect("the centre pixel");
        let corner = linear.first().copied().expect("the corner pixel");
        let expected = albedo[0];
        assert!(
            (centre[0] - expected).abs() < 0.01 * expected,
            "a {name} sphere returns {expected} and the furnace says {:?}",
            centre
        );
        assert!(
            (centre[1] - expected).abs() < 0.01 * expected,
            "the furnace is white, so every channel agrees: {centre:?}"
        );
        // The background is the environment itself, whatever the sphere's albedo is.
        assert!((corner[0] - 1.0).abs() < 1.0e-3, "the room is white: {corner:?}");
    }
}

#[test]
fn a_two_hundred_sample_image_agrees_with_a_converged_reference() {
    // The clause about convergence: the estimator a person waits for at 200 samples must already be
    // the converged picture within the noise the final image will carry.
    let (mut frame, camera) = fixture();
    let Some((device, queue)) = gpu_device() else {
        println!("this machine has no GPU adapter to converge a reference on");
        return;
    };
    // A point light with a real shadow adds the bounce the fixture lacks, so the reference is not a
    // picture of direct light alone.
    frame.lighting.points.push(scene::PointLight {
        position: DVec3::new(-400.0, 400.0, 800.0),
        color: [1.0, 0.9, 0.8],
        power: 4.0e6,
    });
    let size = Request::new(48, 36, 1).expect("a small image");
    let request = |samples: u32| Request::new(size.width(), size.height(), samples).expect("a sample count");
    let reference = linear_of(&trace_linear(&device, &queue, &frame, camera, request(4096)));
    let rough = linear_of(&trace_linear(&device, &queue, &frame, camera, request(200)));
    assert_eq!(reference.len(), rough.len());
    let mut squared = 0.0f64;
    let mut power = 0.0f64;
    let mut counted = 0.0f64;
    for (reference, rough) in reference.iter().zip(rough.iter()) {
        for channel in 0..3 {
            let (reference, rough) = (f64::from(reference[channel]), f64::from(rough[channel]));
            squared += (reference - rough) * (reference - rough);
            power += reference * reference;
            counted += 1.0;
        }
    }
    // Measured against the picture's own root mean square, not its mean: most of this frame is the
    // black background, and a mean diluted by it would say more about the empty corner than about the
    // error on the panel a person looks at.
    let rmse = (squared / counted).sqrt();
    let rms = (power / counted).sqrt();
    assert!(rms > 0.01, "the fixture is lit: rms {rms}");
    let relative = rmse / rms;
    assert!(relative < 0.02, "200 samples are within two percent of 4096: {relative} (rmse {rmse}, rms {rms})");
}

/// The camera of the pane tests: an orthographic view at forty degrees from the vertical.
///
/// An orthographic view is what makes the displacement arithmetic exact — every ray has the same
/// direction — and forty degrees is steep enough to see the floor and shallow enough that a ray
/// crosses a hundred millimetres of glass on its way down.
fn pane_camera() -> (Camera, f64) {
    let incidence = 40.0f64.to_radians();
    let forward = DVec3::new(incidence.sin(), 0.0, -incidence.cos());
    let right = DVec3::new(incidence.cos(), 0.0, incidence.sin());
    let up = right.cross(forward);
    // The centre ray meets the floor near the colour edge, which is the feature the shift moves.
    let eye = DVec3::new(-2000.0, 0.0, 2400.0);
    let camera = Camera::default()
        .looked_from(eye, right, up, forward)
        .with_projection(scene::Projection::Orthographic { height: 300.0 });
    (camera, incidence)
}

/// A horizontal slab of two parallel plates, `thickness` millimetres thick, its underside at `base`.
///
/// The pane is two faces and no rim: a beam enters the top, travels through the tinted medium and
/// leaves the bottom, which is what a glass front is and what the displacement and the absorption of
/// one thickness are measured on.
fn slab(base: f32, thickness: f32, surface: scene::Surface) -> Vec<Drawable> {
    let top = plate(-5000.0, -5000.0, 5000.0, 5000.0, base + thickness);
    let bottom = mesh(
        vec![[-5000.0, -5000.0, base], [5000.0, 5000.0, base], [5000.0, -5000.0, base], [-5000.0, 5000.0, base]],
        vec![0, 1, 2, 0, 3, 1],
    );
    vec![
        Drawable::new(id(11), bottom, DMat4::IDENTITY, [0.0, 0.0, 0.0, 1.0]).with_surface(surface),
        Drawable::new(id(12), top, DMat4::IDENTITY, [0.0, 0.0, 0.0, 1.0]).with_surface(surface),
    ]
}

/// A floor of two colours meeting at `x = 0`: black on the left, white on the right, lit only by the
/// environment so that what the camera reads is the colour and nothing else.
fn two_tone_floor() -> SceneFrame {
    let mut frame = SceneFrame::default();
    frame.push(Drawable::new(id(1), plate(-5000.0, -5000.0, 0.0, 5000.0, 0.0), DMat4::IDENTITY, [0.0, 0.0, 0.0, 1.0]));
    frame.push(Drawable::new(id(2), plate(0.0, -5000.0, 5000.0, 5000.0, 0.0), DMat4::IDENTITY, [1.0, 1.0, 1.0, 1.0]));
    frame.lighting.key_color = [0.0; 3];
    frame.lighting.environment = [1.0, 1.0, 1.0];
    frame
}

/// The first column of the middle row whose red channel crosses half the plate it ends in.
///
/// The mean over three rows, so a single bright sample on the dark side cannot pass for the edge.
fn edge_column(linear: &[[f32; 3]], width: u32, height: u32) -> Option<u32> {
    let mean_of = |column: u32| -> f32 {
        let mut total = 0.0;
        for row in [height / 2 - 1, height / 2, height / 2 + 1] {
            total += linear.get((row * width + column) as usize).map(|pixel| pixel[0]).unwrap_or(0.0);
        }
        total / 3.0
    };
    let brightest = (0..width).map(mean_of).fold(0.0f32, f32::max);
    if brightest <= 0.0 {
        return None;
    }
    (0..width).find(|column| mean_of(*column) > 0.5 * brightest)
}

#[test]
fn a_pane_displaces_the_floor_it_covers_by_the_angle_it_refracts() {
    // One slab, one incidence: what is under it moves by the amount a parallel plate moves a beam,
    // read out of the picture because that is what a catalogue shows.
    let Some((device, queue)) = gpu_device() else {
        println!("this machine has no GPU adapter to look through a pane with");
        return;
    };
    let (camera, incidence) = pane_camera();
    let request = Request::new(64, 48, 64).expect("a small render");
    let plain = linear_of(&trace_linear(&device, &queue, &two_tone_floor(), camera, request));
    let mut glazed = two_tone_floor();
    for drawable in slab(
        100.0,
        400.0,
        scene::Surface { roughness: 0.0, metallic: 0.0, ior: 1.5, transmission: 1.0, absorption: [0.0; 3] },
    ) {
        glazed.push(drawable);
    }
    let through = linear_of(&trace_linear(&device, &queue, &glazed, camera, request));
    let (width, height) = (request.width(), request.height());
    let plain_edge = edge_column(&plain, width, height).expect("the bare floor has a colour edge");
    let pane_edge = edge_column(&through, width, height).expect("the floor under the pane has a colour edge");

    // The displacement of a parallel plate, `t (tan θi - tan θt)`, in world millimetres along the
    // floor; the image's own scale then says how many columns that is.
    let ior = 1.5f64;
    let sin_t = incidence.sin() / ior;
    let cos_t = (1.0 - sin_t * sin_t).sqrt();
    let thickness = 400.0f64;
    let displaced = thickness * (incidence.tan() - sin_t / cos_t);
    let right = DVec3::new(incidence.cos(), 0.0, incidence.sin());
    let half_width = 150.0 * f64::from(width) / f64::from(height);
    let predicted = displaced * right.x / half_width * f64::from(width) / 2.0;
    let measured = f64::from(pane_edge) - f64::from(plain_edge);
    assert!(measured > 0.0, "the beam inside the glass is steeper, so the edge moves along the view");
    assert!(
        (measured - predicted).abs() <= 2.0,
        "the edge moves {measured} columns ({plain_edge} to {pane_edge}) and the plate displaces it {predicted:.1}"
    );
}

/// What a pane over an empty room returns, per unit of the room's radiance.
///
/// Every path is a series: the beam that reflects off the first face sees the room directly, and the
/// beam that crosses pays both interfaces and the tint over the distance it travels inside, with each
/// further round trip inside the slab paying twice that distance and one more internal reflection.
fn expected_through(ior: f64, thickness: f64, incidence: f64, sigma: f64) -> f64 {
    if ior <= 1.0 {
        // A slab that does not bend the beam and does not reflect it: the tint over the plain path.
        return (-sigma * thickness / incidence.cos()).exp();
    }
    let cos_i = incidence.cos();
    let sin_t = incidence.sin() / ior;
    let cos_t = (1.0 - sin_t * sin_t).sqrt();
    let inside = thickness / cos_t;
    // The first interface takes the beam from air: `eta` is the material's index, not its reciprocal.
    let reflectance = crate::bsdf::fresnel_dielectric(cos_i, ior);
    let one_way = (-sigma * inside).exp();
    let mut expected = reflectance;
    let mut term = (1.0 - reflectance) * (1.0 - reflectance) * one_way;
    for _ in 0..64 {
        expected += term;
        term *= reflectance * one_way * one_way;
    }
    expected
}

#[test]
fn a_pane_darkens_what_is_behind_it_by_the_tint_and_the_path_it_travels_inside() {
    // Looking through a pane at the empty room is a path with no bounce and no variance: the picture is
    // the environment times the interfaces' Fresnel and the Beer-Lambert loss over the distance the
    // beam travels inside, and every number of it is known before the render starts.
    let Some((device, queue)) = gpu_device() else {
        println!("this machine has no GPU adapter to look through a pane with");
        return;
    };
    let (camera, incidence) = pane_camera();
    let request = Request::new(16, 16, 64).expect("a tiny render");
    let absorption = [0.001f32, 0.002, 0.004];
    // Glass, a slab that refracts nothing (the tint alone), and a double thickness of it: the last two
    // pin the path the absorption is taken over, the first two the interfaces.
    let cases = [
        ("clear glass", 1.5f64, 400.0f64, [0.0f32; 3]),
        ("tinted glass", 1.5, 400.0, absorption),
        ("a slab that refracts nothing", 1.0, 400.0, absorption),
        ("the same slab twice as thick", 1.0, 800.0, absorption),
    ];
    for (name, ior, thickness, tint) in cases {
        // A tinted glass pane sums an infinity of internal reflections; the first four orders carry
        // it to a fifth of a percent and the rest are under the image's own noise.
        let tolerance = if ior > 1.0 && tint != [0.0; 3] { 0.05 } else { 0.02 };
        let mut frame = SceneFrame::default();
        frame.lighting.key_color = [0.0; 3];
        frame.lighting.environment = [1.0, 1.0, 1.0];
        let surface =
            scene::Surface { roughness: 0.0, metallic: 0.0, ior: ior as f32, transmission: 1.0, absorption: tint };
        for drawable in slab(100.0, thickness as f32, surface) {
            frame.push(drawable);
        }
        let linear = linear_of(&trace_linear(&device, &queue, &frame, camera, request));
        let centre = linear.get(8 * 16 + 8).copied().expect("the centre pixel");
        let mean = linear.iter().fold([0.0f64; 3], |mut sum, pixel| {
            for (slot, channel) in sum.iter_mut().zip(pixel.iter()) {
                *slot += f64::from(*channel);
            }
            sum
        });
        let count = linear.len() as f64;
        println!("{name} mean {:?} centre {centre:?}", mean.map(|value| value / count));
        for channel in 0..3 {
            let expected = expected_through(ior, thickness, incidence, f64::from(tint[channel]));
            let measured = f64::from(centre[channel]);
            assert!(
                (measured - expected).abs() < tolerance,
                "{name} channel {channel}: the picture reads {measured}, the tint names {expected}"
            );
        }
    }
}

#[test]
fn the_radiance_clamp_is_out_of_reach_and_measurably_biased_when_it_is_not() {
    // Y2 carries one clamp on what a single sample may bring, and owes its bias measured rather than
    // felt. The measurement is two renders of the same plate under a lamp of two powers: light is
    // linear, so the image must scale with the lamp until a sample meets the clamp.
    let Some((device, queue)) = gpu_device() else {
        println!("this machine has no GPU adapter to measure a clamp on");
        return;
    };
    let camera = Camera::new(DVec3::ZERO, 1500.0).expect("a camera looking at the plate");
    let request = Request::new(16, 16, 8).expect("a tiny render");
    let plate_of = |power: f32| -> SceneFrame {
        let mut frame = SceneFrame::default();
        frame.push(Drawable::new(
            id(1),
            plate(-800.0, -800.0, 800.0, 800.0, 0.0),
            DMat4::IDENTITY,
            [1.0, 1.0, 1.0, 1.0],
        ));
        frame.lighting.key_color = [0.0; 3];
        frame.lighting.environment = [0.0; 3];
        // A lamp a hundred millimetres above the plate: its irradiance is `power / (4 PI d^2)`.
        frame.lighting.points.push(scene::PointLight {
            position: DVec3::new(0.0, 0.0, 100.0),
            color: [1.0, 1.0, 1.0],
            power,
        });
        frame
    };
    let centre_of = |frame: &SceneFrame| -> f64 {
        let linear = linear_of(&trace_linear(&device, &queue, frame, camera, request));
        f64::from(linear.get(8 * 16 + 8).copied().expect("the centre pixel")[0])
    };
    // Light is linear, so the picture of a lamp must be the lamp. One watt is far below the clamp.
    let dim = centre_of(&plate_of(1.0));
    let brighter = centre_of(&plate_of(2.0));
    assert!(dim > 0.0, "the plate under a watt is visible");
    assert!((brighter - 2.0 * dim).abs() < 0.02 * dim, "twice the light, twice the picture: {dim} then {brighter}");

    // A lamp of two terawatts, a hundred millimetres from the plate, puts every sample past the
    // clamp: the picture stops scaling, and the difference is the bias it costs.
    let huge = centre_of(&plate_of(2.0e12));
    let unclamped = 2.0e12 * dim;
    let lost = (1.0 - huge / unclamped) * 100.0;
    println!(
        "the clamp reads {huge} where the unclamped picture would be {unclamped}: {lost:.1}% of the light is gone"
    );
    assert!(huge > 0.0, "a clamped picture is a picture, not a hole");
    assert!(huge < unclamped, "the clamp fires on a lamp this bright");
}
