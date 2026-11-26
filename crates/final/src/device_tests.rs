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
use scene::{Camera, Decor, DecorId, DecorImage, Drawable, SceneFrame, Turns};
use units::Length;

use crate::job::{Input, Settings, run};
use crate::view::aspect;
use crate::{FinalError, Progress, RENDER_FINAL_ID, Request, Snapshots, Tracer, Watch, faces_of};

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
fn a_checkpoint_shows_the_whole_frame_and_the_last_picture_is_the_written_file() {
    // Y6's contract, as a test: the picture arrives while the render runs, each one is of the whole
    // frame and counts more samples than the one before, and the last of them is the file.
    let (frame, camera) = fixture();
    let Some((device, queue)) = gpu_device() else {
        println!("this machine has no GPU adapter to watch a checkpoint of");
        return;
    };
    let path = scratch_png("checkpoints");
    let request = Request::new(128, 128, 16).expect("a small render");
    let settings = Settings::default_for(request, &path).expect("a render setting");
    let snapshots = Arc::new(Snapshots::new());
    // The run publishes faster than anything can read, so a reader takes pictures the way the window
    // does: from its own thread, between samples, keeping only the latest. A run of sixteen samples
    // publishes at every second sample, so the reader would have to be starved for half the render
    // to see fewer than three of them.
    let finished = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let seen: Vec<crate::Snapshot> = std::thread::scope(|scope| {
        let mailbox = Arc::clone(&snapshots);
        let stop = Arc::clone(&finished);
        let reader = scope.spawn(move || {
            let mut seen: Vec<crate::Snapshot> = Vec::new();
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                if let Some(snapshot) = mailbox.take() {
                    seen.push(snapshot);
                }
                std::thread::yield_now();
            }
            if let Some(snapshot) = mailbox.take() {
                seen.push(snapshot);
            }
            seen
        });
        let mut tracer = Tracer::new(&device).expect("the integrator compiles");
        let progress = Progress::new(request.samples());
        run(
            &mut tracer,
            &device,
            &queue,
            Input { frame: &frame, camera, revision: 9 },
            &settings,
            &progress,
            &snapshots,
        )
        .expect("the fixture renders");
        finished.store(true, std::sync::atomic::Ordering::Relaxed);
        let mut seen = reader.join().expect("the reader finishes");
        if let Some(last) = snapshots.take() {
            seen.push(last);
        }
        seen
    });
    assert!(seen.len() >= 3, "a run of sixteen samples published {} pictures", seen.len());
    for pair in seen.windows(2) {
        assert!(pair[0].samples() < pair[1].samples(), "the sample count went back");
    }
    for snapshot in &seen {
        assert_eq!((snapshot.width(), snapshot.height()), (128, 128), "a snapshot is not the frame");
    }
    // The last picture is the finished image, to the code value: the file is a lossless RGBA8 copy
    // of the same tone map the shell applies to the snapshot it keeps.
    let last = seen.last().expect("a picture was published");
    assert_eq!(last.samples(), request.samples(), "the last picture is the finished one");
    let (width, height, pixels) = read_png(&path);
    assert_eq!((width, height), (last.width(), last.height()));
    let expected = last.image().to_rgba8(settings.exposure());
    let written: Vec<u8> = pixels.iter().flatten().copied().collect();
    assert_eq!(expected, written, "the last picture is not the image the file holds");
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
    assert_eq!(faces_of(&frame).expect("the fixture flattens").faces.len(), 4, "two quads are four faces");
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
    run(
        &mut tracer,
        &device,
        &queue,
        Input { frame: &frame, camera, revision: 3 },
        &settings,
        &progress,
        &Snapshots::new(),
    )
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
        .render(
            &device,
            &queue,
            camera,
            &frame.lighting,
            request,
            Watch { progress: &unprepared, snapshots: &Snapshots::new() },
        )
        .expect_err("nothing was uploaded");
    assert!(matches!(refusal, FinalError::Device { .. }), "{refusal}");

    let progress = Progress::new(request.samples());
    let report = run(
        &mut tracer,
        &device,
        &queue,
        Input { frame: &frame, camera, revision: 1 },
        &settings,
        &progress,
        &Snapshots::new(),
    )
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
    run(
        &mut tracer,
        &device,
        &queue,
        Input { frame: &frame, camera, revision: 1 },
        &settings,
        &second,
        &Snapshots::new(),
    )
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
    let error = run(
        &mut tracer,
        &device,
        &queue,
        Input { frame: &frame, camera, revision: 2 },
        &settings,
        &progress,
        &Snapshots::new(),
    )
    .expect_err("a cancelled render does not finish");
    assert!(matches!(error, FinalError::Cancelled { samples: 0, total: 8 }), "{error}");
    assert!(error.is_cancelled());
    assert_eq!(std::fs::read(&path).expect("the old picture is still there"), previous, "the file is untouched");
    assert_eq!(progress.done(), 0, "no sample was traced");

    // The session starts again: the cancel is a value, not a broken tracer.
    let fresh = Progress::new(request.samples());
    run(
        &mut tracer,
        &device,
        &queue,
        Input { frame: &frame, camera, revision: 2 },
        &settings,
        &fresh,
        &Snapshots::new(),
    )
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
    // The raw picture, divided per pixel by the samples that pixel actually took: a pixel that
    // converged early has fewer, and dividing it by the request's count would darken it.
    let image = accumulation.raw_image().expect("the sums fit the size");
    let mut linear = Vec::with_capacity((image.width() * image.height()) as usize);
    for y in 0..image.height() {
        for x in 0..image.width() {
            linear.push(image.radiance_at(x, y).unwrap_or([0.0; 3]));
        }
    }
    linear
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
    let snapshots = Snapshots::new();
    tracer
        .render(device, queue, camera, &frame.lighting, request, Watch { progress: &progress, snapshots: &snapshots })
        .expect("the frame renders")
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

/// Two white spheres side by side under one lamp and no room, one matte and one lacquered.
///
/// A rim light and a black room: the matte sphere's rim is dark because a Lambertian surface turns
/// away from the lamp there, while the coat reflects it, so the only difference the picture can show
/// is the layer the right-hand sphere wears (ADR 0243).
fn lacquered_pair(coat: scene::Coat, lamp: DVec3) -> SceneFrame {
    let mut frame = SceneFrame::default();
    let matte =
        Drawable::new(id(1), sphere(400.0, 32), DMat4::from_translation(DVec3::new(-1000.0, 0.0, 0.0)), [1.0; 4]);
    let lacquered =
        Drawable::new(id(2), sphere(400.0, 32), DMat4::from_translation(DVec3::new(1000.0, 0.0, 0.0)), [1.0; 4])
            .with_surface(scene::Surface { coat, ..scene::Surface::MATTE });
    frame.push(matte);
    frame.push(lacquered);
    frame.lighting.key_direction = lamp;
    frame.lighting.key_color = [1.0, 1.0, 1.0];
    frame.lighting.points.clear();
    // A dim room fills the axis of both spheres, so the middle of the picture is a surface the two
    // have in common and the lamp only draws the edge.
    frame.lighting.environment = [0.25; 3];
    frame
}

/// The lamp that puts a lacquered sphere's sheen in the grazing band the test measures.
///
/// The band is a ring of the disc, and a ring is a band of surface angles: `75` degrees off the view
/// direction is where the layer's Fresnel is a quarter and falling to nothing directly behind the
/// camera. The sheen lands where the view mirrors about the normal, so the lamp is that mirror, found
/// by walking the point out along the normal it names until it settles.
fn sheen_lamp(centre: DVec3, radius: f64, eye: DVec3) -> DVec3 {
    const BAND: f64 = 75.0;
    let angle = BAND.to_radians();
    let mut normal = DVec3::Z;
    for _ in 0..8 {
        let point = centre + normal * radius;
        let wo = (eye - point).try_normalize().unwrap_or(DVec3::Y);
        let tangent = (DVec3::Z - wo * DVec3::Z.dot(wo)).try_normalize().unwrap_or(DVec3::X);
        normal = (wo * angle.cos() + tangent * angle.sin()).try_normalize().unwrap_or(DVec3::Z);
    }
    let point = centre + normal * radius;
    let wo = (eye - point).try_normalize().unwrap_or(DVec3::Y);
    (normal * (2.0 * wo.dot(normal)) - wo).try_normalize().unwrap_or(DVec3::Y)
}

/// A camera in front of the two spheres, looking along `+Y`.
fn pair_camera() -> Camera {
    let eye = DVec3::new(0.0, -4000.0, 0.0);
    let forward = DVec3::Y;
    let right = DVec3::X;
    let up = DVec3::Z;
    Camera::default().looked_from(eye, right, up, forward)
}

/// The pixels of one sphere that lie between `inner` and `outer` of its projected radius: their mean
/// luminance, their brightest, and the luminance of the pixel its centre lands on.
///
/// The sphere is a silhouette in the image and nothing else, so a band of the disc is a band of
/// surface angles: the rim is where the view grazes the surface.
fn sphere_band(
    picture: &[[f32; 3]],
    camera: Camera,
    request: Request,
    centre: DVec3,
    radius: f64,
    inner: f64,
    outer: f64,
) -> (f64, f64, f64) {
    let luminance =
        |pixel: &[f32; 3]| 0.2126 * f64::from(pixel[0]) + 0.7152 * f64::from(pixel[1]) + 0.0722 * f64::from(pixel[2]);
    let Some((cx, cy)) = pixel_of(camera, centre, request) else { return (0.0, 0.0, 0.0) };
    // The projected radius: the sphere's radius along `X`, which is the image's own across axis.
    let Some((rx, _)) = pixel_of(camera, centre + DVec3::new(radius, 0.0, 0.0), request) else {
        return (0.0, 0.0, 0.0);
    };
    let projected = f64::from(rx.abs_diff(cx)).max(1.0);
    let middle = picture.get((cy * request.width() + cx) as usize).map_or(0.0, luminance);
    let mut total = 0.0;
    let mut brightest = 0.0f64;
    let mut counted = 0.0;
    for y in 0..request.height() {
        for x in 0..request.width() {
            let (dx, dy) = (f64::from(x) + 0.5 - f64::from(cx), f64::from(y) + 0.5 - f64::from(cy));
            let distance = (dx * dx + dy * dy).sqrt() / projected;
            if distance < inner || distance > outer {
                continue;
            }
            if let Some(pixel) = picture.get((y * request.width() + x) as usize) {
                let value = luminance(pixel);
                total += value;
                brightest = brightest.max(value);
                counted += 1.0;
            }
        }
    }
    (if counted > 0.0 { total / counted } else { 0.0 }, brightest, middle)
}

#[test]
fn a_lacquered_sphere_sheens_where_the_matte_one_turns_away() {
    // The clause Y9 owes: the fourth surface a catalogue sells — a dielectric layer over a decal —
    // is a picture and not a number (ADR 0243). Two spheres of the same white board stand under one
    // lamp, and the right-hand one wears a full coat at 90 % gloss. On axis the layer reflects four
    // percent of the beam and passes the rest, so the two spheres still agree there; in the grazing
    // band the matte sphere has turned away from the lamp, where the layer's Fresnel is a quarter and
    // rising, so the lacquered one carries a sheen and the matte one does not.
    let Some((device, queue)) = gpu_device() else {
        println!("this machine has no GPU adapter to sheen a lacquered sphere on");
        return;
    };
    let camera = pair_camera();
    let eye = DVec3::new(0.0, -4000.0, 0.0);
    let left = DVec3::new(-1000.0, 0.0, 0.0);
    let right = DVec3::new(1000.0, 0.0, 0.0);
    let request = Request::new(320, 240, 256).expect("a small render");
    let frame = lacquered_pair(scene::Coat::glossed(1.0, 90.0, scene::Coat::IOR), sheen_lamp(right, 400.0, eye));
    let linear = linear_of(&trace_linear(&device, &queue, &frame, camera, request));
    let (_, matte_peak, _) = sphere_band(&linear, camera, request, left, 400.0, 0.9, 1.0);
    let (_, lacquered_peak, _) = sphere_band(&linear, camera, request, right, 400.0, 0.9, 1.0);
    println!("the rim peaks at {matte_peak} matte and {lacquered_peak} lacquered");
    assert!(matte_peak > 0.0 && lacquered_peak > 0.0, "both spheres are in the picture");
    assert!(
        lacquered_peak > matte_peak * 1.2,
        "the lacquered rim peaks at {lacquered_peak} and the matte rim at {matte_peak}, so the layer is not in the picture"
    );
    let (_, _, matte_axis) = sphere_band(&linear, camera, request, left, 400.0, 0.0, 0.2);
    let (_, _, lacquered_axis) = sphere_band(&linear, camera, request, right, 400.0, 0.0, 0.2);
    println!("the axis reads {matte_axis} matte and {lacquered_axis} lacquered");
    // A tenth of the value would pass a model that adds the layer's four percent instead of taking
    // it out of the base; a room of uniform radiance and a near-mirror layer is a furnace, where the
    // two spheres are the same number and not merely a close pair.
    let apart = (lacquered_axis - matte_axis).abs() / matte_axis.max(1.0e-6);
    assert!(apart < 0.02, "on axis the two spheres agree within two percent: {matte_axis} against {lacquered_axis}");
}

/// The linear radiance an eight-bit sRGB texel carries, which is what the shader decodes a picture to.
fn linear_of_texel(value: u8) -> f32 {
    let channel = f32::from(value) / 255.0;
    if channel <= 0.04045 { channel / 12.92 } else { ((channel + 0.055) / 1.055).powf(2.4) }
}

#[test]
fn a_boards_grain_reaches_the_filtered_picture() {
    // The clause Y9 owes the denoiser: a picture of a decor is a picture of the board, so the filter
    // must take the noise and leave the grain. The board is one texel per pixel across 64 columns,
    // waved at two values of albedo under a white room — the furnace, where a diffuse board returns
    // exactly its own albedo — so what the image should be is known without a reference render.
    let Some((device, queue)) = gpu_device() else {
        println!("this machine has no GPU adapter to filter a board's grain on");
        return;
    };
    const SIDE: f32 = 2000.0;
    const COLUMNS: u32 = 64;
    const CREST: u8 = 110;
    const TROUGH: u8 = 88;
    let texel = |x: u32| {
        let phase = (x as f32 / 8.0) * std::f32::consts::TAU;
        let value = if phase.cos() > 0.0 { CREST } else { TROUGH };
        [value, value, value, 255]
    };
    let mut texels = Vec::new();
    for _y in 0..COLUMNS {
        for x in 0..COLUMNS {
            texels.push(texel(x));
        }
    }
    let image = Arc::new(DecorImage::new(COLUMNS, COLUMNS, texels).expect("the picture"));
    let decor = Decor::new(
        DecorId::new(1).expect("one picture"),
        image,
        Length::from_millimetres(2000).expect("a size"),
        Turns::ALONG,
    )
    .expect("a decor");
    let mut frame = SceneFrame::default();
    frame.push(
        Drawable::new(id(1), board(SIDE), DMat4::from_translation(DVec3::new(-1000.0, -1000.0, 0.0)), [1.0; 4])
            .with_decor(decor),
    );
    frame.lighting.key_color = [0.0; 3];
    frame.lighting.points.clear();
    frame.lighting.environment = [1.0; 3];
    let camera = Camera::default().looked_from(DVec3::new(0.0, 0.0, 2000.0), DVec3::X, DVec3::Y, -DVec3::Z);
    let request = Request::new(COLUMNS, COLUMNS, 128).expect("a small render");
    let accumulation = trace_linear(&device, &queue, &frame, camera, request);
    let filtered = accumulation.denoised();
    // The two values the picture is made of are the two texels the board wears, with nothing between
    // them: a filter that averaged across the grain would leave the extremes short of them.
    let (mut low, mut high) = (f64::INFINITY, f64::NEG_INFINITY);
    for y in 8..COLUMNS - 8 {
        for x in 8..COLUMNS - 8 {
            let Some(pixel) = filtered.get((y * COLUMNS + x) as usize) else { continue };
            low = low.min(f64::from(pixel[0]));
            high = high.max(f64::from(pixel[0]));
        }
    }
    let expected = f64::from(linear_of_texel(TROUGH));
    let crest = f64::from(linear_of_texel(CREST));
    assert!(low <= expected + 0.01 && low >= expected - 0.01, "the trough is {low} and the texel is {expected}");
    assert!(high >= crest - 0.01 && high <= crest + 0.01, "the crest is {high} and the texel is {crest}");
}

/// A decor whose picture is two blocks: its left half red and its right half blue.
fn two_tone(covers: i64, turns: Turns) -> Decor {
    let texels = vec![[255, 0, 0, 255], [255, 0, 0, 255], [0, 0, 255, 255], [0, 0, 255, 255]];
    let image = Arc::new(DecorImage::new(4, 1, texels).expect("four texels"));
    let id = DecorId::new(1).expect("one picture");
    let covers = Length::from_millimetres(covers).expect("a length");
    Decor::new(id, image, covers, turns).expect("a decor")
}

/// A square board `side` millimetres across, facing `+Z`, with its zero corner at the minimum.
fn board(side: f32) -> Arc<MeshData> {
    Arc::new(MeshData {
        positions: vec![[0.0, 0.0, 0.0], [side, 0.0, 0.0], [side, side, 0.0], [0.0, side, 0.0]],
        normals: Vec::new(),
        indices: vec![0, 1, 2, 0, 2, 3],
    })
}

#[test]
fn a_decor_shows_the_board_and_a_turn_runs_its_grain_the_other_way() {
    // The clause about the catalogue's decor: a board is drawn with the picture the shop scanned,
    // at the size the record names, and a quarter turn makes the grain cross the board. The furnace
    // says what a diffuse surface returns under a uniform environment — its own albedo — so the
    // picture is read out of the image exactly and the light needs no lamp of its own.
    let Some((device, queue)) = gpu_device() else {
        println!("this machine has no GPU adapter to draw a decor with");
        return;
    };
    const SIDE: f32 = 2000.0;
    let request = Request::new(32, 32, 8).expect("a small render");
    let camera = Camera::default().looked_from(DVec3::new(0.0, 0.0, 2000.0), DVec3::X, DVec3::Y, -DVec3::Z);
    let frame = |turns| {
        let mut frame = SceneFrame::default();
        frame.push(
            Drawable::new(id(1), board(SIDE), DMat4::from_translation(DVec3::new(-1000.0, -1000.0, 0.0)), [1.0; 4])
                .with_decor(two_tone(2000, turns)),
        );
        // The environment is the only light in the room: a white studio and no lamp.
        frame.lighting.key_color = [0.0; 3];
        frame.lighting.points.clear();
        frame.lighting.environment = [1.0, 1.0, 1.0];
        frame
    };
    let pixel = |linear: &[[f32; 3]], x: usize, y: usize| linear.get(y * 32 + x).copied().expect("a pixel");

    let straight = linear_of(&trace_linear(&device, &queue, &frame(Turns::ALONG), camera, request));
    let left = pixel(&straight, 8, 16);
    let right = pixel(&straight, 24, 16);
    assert!(left[0] > 0.9 && left[2] < 0.1, "the board's left half is the picture's red one: {left:?}");
    assert!(right[2] > 0.9 && right[0] < 0.1, "and its right half is the blue one: {right:?}");

    // A quarter turn: the picture's own width — its grain — now runs along the board's height, so
    // the two blocks cross it instead of standing beside each other.
    let turned = linear_of(&trace_linear(&device, &queue, &frame(Turns::ACROSS), camera, request));
    let top = pixel(&turned, 16, 8);
    let bottom = pixel(&turned, 16, 24);
    assert!(top[2] > 0.9 && top[0] < 0.1, "the stripes cross the board: {top:?}");
    assert!(bottom[0] > 0.9 && bottom[2] < 0.1, "and they are the other way round: {bottom:?}");
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
        scene::Surface {
            roughness: 0.0,
            metallic: 0.0,
            ior: 1.5,
            transmission: 1.0,
            absorption: [0.0; 3],
            coat: scene::Coat::NONE,
        },
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
        let surface = scene::Surface {
            roughness: 0.0,
            metallic: 0.0,
            ior: ior as f32,
            transmission: 1.0,
            absorption: tint,
            coat: scene::Coat::NONE,
        };
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
fn a_frosted_pane_spreads_the_bar_behind_it_and_the_clear_one_does_not() {
    // Y10's clause, in the picture: the same bright bar under a pane of roughness zero and of 0.6. The
    // clear one is a sharp bar and the picture the tracer took before this entry, sample for sample;
    // the frosted one carries the bar's light into the dark air beside it, and the noise that spreads it
    // is the variance the entry adds, measured against a reference of the same frame.
    let Some((device, queue)) = gpu_device() else {
        println!("this machine has no GPU adapter to frost a pane on");
        return;
    };
    let incidence = 40.0f64.to_radians();
    let forward = DVec3::new(incidence.sin(), 0.0, -incidence.cos());
    let right = DVec3::new(incidence.cos(), 0.0, incidence.sin());
    let up = right.cross(forward);
    // A wide orthographic view, so the bar stays a bar while the lobe spreads it: the half-height is
    // 1200 mm, and the spread of a 0.6 pane over a 400 mm slab covers a tenth of the frame.
    let camera = Camera::default()
        .looked_from(DVec3::new(-2000.0, 0.0, 2400.0), right, up, forward)
        .with_projection(scene::Projection::Orthographic { height: 1200.0 });
    let bar = |frame: &mut SceneFrame| {
        // A black floor and one white bar across it: the room of a uniform environment returns exactly
        // its own radiance from a white board (the furnace), so the bar is the room and the floor is
        // nothing.
        for (object, from, to, colour) in [
            (1u64, -6000.0f32, -60.0f32, [0.0f32, 0.0, 0.0, 1.0]),
            (2, -60.0, 60.0, [1.0, 1.0, 1.0, 1.0]),
            (3, 60.0, 6000.0, [0.0, 0.0, 0.0, 1.0]),
        ] {
            frame.push(Drawable::new(id(object), plate(from, -6000.0, to, 6000.0, 0.0), DMat4::IDENTITY, colour));
        }
        frame.lighting.key_color = [0.0; 3];
        frame.lighting.points.clear();
        frame.lighting.environment = [1.0, 1.0, 1.0];
    };
    let pane = |roughness: f32| -> Vec<Drawable> {
        slab(
            100.0,
            400.0,
            scene::Surface {
                roughness,
                metallic: 0.0,
                ior: 1.5,
                transmission: 1.0,
                absorption: [0.0; 3],
                coat: scene::Coat::NONE,
            },
        )
    };
    let size = Request::new(96, 72, 1).expect("a small image");
    let request = |samples: u32| Request::new(size.width(), size.height(), samples).expect("a sample count");

    let mut measured = [(0.0f64, 0.0f64); 2];
    for (slot, roughness) in [0.0f32, 0.6].into_iter().enumerate() {
        let mut frame = SceneFrame::default();
        bar(&mut frame);
        for drawable in pane(roughness) {
            frame.push(drawable);
        }
        let reference = linear_of(&trace_linear(&device, &queue, &frame, camera, request(2048)));
        let sampled = linear_of(&trace_linear(&device, &queue, &frame, camera, request(64)));
        // The noise a pane of this width adds, as the mean distance from the converged picture: the
        // mean is the measure, not the root mean square, because a frosted pane finds its caustics by
        // chance and one bright sample would otherwise speak for the whole image.
        let noise = sampled
            .iter()
            .zip(&reference)
            .map(|(sampled, reference)| f64::from((sampled[0] - reference[0]).abs()))
            .sum::<f64>()
            / sampled.len() as f64;
        // The bar's own light in the picture: the plain pane reflects the studio at nine percent of it
        // and the bar itself reads half again, so everything above fifteen percent is the bar and what a
        // lobe spreads it into.
        let lit =
            |picture: &[[f32; 3]]| picture.iter().filter(|pixel| pixel[0] > 0.15).count() as f64 / picture.len() as f64;
        println!(
            "roughness {roughness}: noise {noise:.4}, {:.1}% of the picture carries the bar above fifteen percent",
            lit(&reference) * 100.0
        );
        if let Some(entry) = measured.get_mut(slot) {
            *entry = (noise, lit(&reference));
        }
    }
    let [(clear_noise, clear_lit), (frosted_noise, frosted_lit)] = measured;
    assert!(clear_lit > 0.0, "the clear pane shows the bar");
    assert!(
        frosted_lit > 2.0 * clear_lit,
        "a frosted pane spreads the bar over twice the picture: {clear_lit:.3} then {frosted_lit:.3}"
    );
    assert!(
        frosted_noise > 1.5 * clear_noise,
        "the spread is noise the lobe adds: {clear_noise:.4} then {frosted_noise:.4}"
    );
    assert!(clear_noise > 0.0, "a clear pane is still a picture the sampler draws");
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

#[test]
fn the_studio_is_the_render_settings_and_not_the_frames() {
    // The frame's own environment is zero here, so a picture that is not black can only come from the
    // command's studio: this is the test that the setting reaches the shader through `run`.
    let Some((device, queue)) = gpu_device() else {
        println!("this machine has no GPU adapter to light a studio with");
        return;
    };
    let mut frame = SceneFrame::default();
    frame.push(Drawable::new(id(1), plate(-800.0, -800.0, 800.0, 800.0, 0.0), DMat4::IDENTITY, [1.0, 1.0, 1.0, 1.0]));
    frame.lighting.key_color = [0.0; 3];
    frame.lighting.points.clear();
    assert_eq!(frame.lighting.environment, [0.0; 3], "the frame carries no studio of its own");
    let camera = Camera::new(DVec3::ZERO, 1500.0).expect("a camera looking at the plate");
    let request = Request::new(16, 16, 4).expect("a tiny render");
    let mut tracer = Tracer::new(&device).expect("the integrator compiles");
    let mut centre_of = |studio: f32, name: &str| -> [u8; 4] {
        let path = scratch_png(name);
        let settings = Settings::default_for(request, &path)
            .expect("a render setting")
            .with_environment(studio)
            .expect("a studio");
        let progress = Progress::new(request.samples());
        run(
            &mut tracer,
            &device,
            &queue,
            Input { frame: &frame, camera, revision: 5 },
            &settings,
            &progress,
            &Snapshots::new(),
        )
        .expect("the frame renders");
        let (width, _height, pixels) = read_png(&path);
        pixels.get((8 * width + 8) as usize).copied().expect("the centre pixel")
    };
    let dark = centre_of(0.0, "studio-none");
    let lit = centre_of(1.0, "studio-white");
    assert_eq!(dark, [0, 0, 0, 255], "no studio and no lamp is a black picture");
    assert!(lit[0] > 200, "a white studio returns the room: {lit:?}");
    assert_eq!((lit[0], lit[1], lit[2]), (lit[0], lit[0], lit[0]), "a neutral studio is not a colour");
}

/// The root mean square error of one picture against a reference, and the reference's own power.
///
/// Measured against the picture's own RMS rather than its mean, for the reason the convergence test
/// gives: most of a furniture frame is a dark background, and a mean diluted by it says more about
/// the empty corner than about the error on the surface a person looks at.
fn compare(picture: &[[f32; 3]], reference: &[[f32; 3]]) -> (f64, f64) {
    let mut squared = 0.0f64;
    let mut power = 0.0f64;
    let mut counted = 0.0f64;
    for (picture, reference) in picture.iter().zip(reference) {
        for channel in 0..3 {
            let (picture, reference) = (f64::from(picture[channel]), f64::from(reference[channel]));
            squared += (reference - picture) * (reference - picture);
            power += reference * reference;
            counted += 1.0;
        }
    }
    if counted == 0.0 {
        return (0.0, 0.0);
    }
    ((squared / counted).sqrt(), (power / counted).sqrt())
}

/// The frame two boards of different paint make, seen straight down from above under a lamp.
///
/// It is the denoiser's own fixture: the line between the boards is exactly the edge a filter must not
/// cross, so the pair is one thousand millimetres across, orthographic, and the line falls on a pixel
/// boundary — the filter is what is being measured, not the antialiasing of a slanted edge. The lamp's
/// bounce is the light that is not known before the render starts, so the estimate is noisy.
fn two_boards() -> (SceneFrame, Camera) {
    let mut frame = SceneFrame::default();
    frame.push(Drawable::new(id(1), plate(-500.0, -500.0, 0.0, 500.0, 0.0), DMat4::IDENTITY, [0.8, 0.8, 0.78, 1.0]));
    frame.push(Drawable::new(id(2), plate(0.0, -500.0, 500.0, 500.0, 0.0), DMat4::IDENTITY, [0.05, 0.05, 0.05, 1.0]));
    frame.lighting.key_color = [1.0, 1.0, 1.0];
    frame.lighting.key_direction = DVec3::new(-0.2, 0.2, 1.0).try_normalize().unwrap_or(DVec3::Z);
    // Three lamps, so a sample draws one of them by power instead of adding all of them: that draw is
    // what makes the estimate noisy, which is the thing the filter exists for.
    for (position, colour, power) in [
        (DVec3::new(-300.0, -300.0, 900.0), [1.0, 0.95, 0.9], 2.0e6),
        (DVec3::new(350.0, 250.0, 700.0), [0.9, 0.95, 1.0], 1.2e6),
        (DVec3::new(0.0, 400.0, 1200.0), [1.0, 1.0, 1.0], 3.0e6),
    ] {
        frame.lighting.points.push(scene::PointLight { position, color: colour, power });
    }
    // Straight down, twenty-five millimetres to a pixel: the join of the boards is the image's centre.
    let camera = Camera::default()
        .looked_from(DVec3::new(0.0, 0.0, 2000.0), DVec3::X, DVec3::Y, DVec3::NEG_Z)
        .with_projection(scene::Projection::Orthographic { height: 1200.0 });
    (frame, camera)
}

#[test]
fn a_hundred_and_twenty_eight_samples_denoised_beat_the_raw_image_against_a_reference() {
    // The entry's first clause: the picture a person waits for at 128 samples must be measurably
    // closer to the converged one after the filter than before it, and both numbers are printed so the
    // note can carry the measurement rather than the claim.
    let (frame, camera) = two_boards();
    let Some((device, queue)) = gpu_device() else {
        println!("this machine has no GPU adapter to filter a reference with");
        return;
    };
    let size = Request::new(256, 192, 1).expect("a small image");
    let request = |samples: u32| Request::new(size.width(), size.height(), samples).expect("a sample count");
    let reference = trace_linear(&device, &queue, &frame, camera, request(4096));
    let reference = linear_of(&reference);
    let rough = trace_linear(&device, &queue, &frame, camera, request(128));
    let raw = linear_of(&rough);
    let denoised = rough.denoised().to_vec();
    let (raw_error, rms) = compare(&raw, &reference);
    let (denoised_error, _) = compare(&denoised, &reference);
    println!(
        "128 samples: raw rmse {raw_error:.6} denoised {denoised_error:.6} against a 4096-sample reference of rms \
         {rms:.6} — {:.1}% closer",
        (1.0 - denoised_error / raw_error.max(f64::MIN_POSITIVE)) * 100.0
    );
    assert!(rms > 0.001, "the fixture is lit: rms {rms}");
    assert!(
        denoised_error < raw_error * 0.9,
        "the filter must take at least a tenth of the error away: {raw_error} became {denoised_error}"
    );
    // And it does not do it by smearing the line between the two boards: the join at the image's
    // centre keeps its contrast, which is the guide the shader wrote for each primary hit.
    let column = |picture: &[[f32; 3]], x: u32| {
        let sum: f64 = (40..160).map(|y| f64::from(picture[(y * 256 + x) as usize][0])).sum();
        (sum / 120.0) as f32
    };
    let raw_step = column(&raw, 108) - column(&raw, 148);
    let denoised_step = column(&denoised, 108) - column(&denoised, 148);
    println!("the join reads {raw_step:.5} raw and {denoised_step:.5} filtered");
    assert!(raw_step > 0.1, "the boards are two paints: the join is {raw_step}");
    assert!(denoised_step > raw_step * 0.99, "the join keeps its contrast: {raw_step} became {denoised_step}");
}

/// The eight-bit code values one picture lands on, which is what a person sees.
fn codes(picture: &[[f32; 3]]) -> Vec<f32> {
    picture
        .iter()
        .flat_map(|pixel| pixel.iter().map(|channel| (crate::image::tone_map([*channel; 3], 1.0)[0] * 255.0).round()))
        .collect()
}

#[test]
fn at_four_thousand_samples_the_filter_moves_the_picture_by_under_a_code_value() {
    // The entry's second clause, and the one that rules out a permanent blur: an image that is already
    // converged is not moved by a code value when the filter is done with it.
    let (frame, camera) = two_boards();
    let Some((device, queue)) = gpu_device() else {
        println!("this machine has no GPU adapter to converge a picture on");
        return;
    };
    let request = Request::new(128, 96, 4096).expect("a converged image");
    let accumulation = trace_linear(&device, &queue, &frame, camera, request);
    let raw = codes(&linear_of(&accumulation));
    let denoised = codes(accumulation.denoised());
    let mut squared = 0.0f64;
    for (raw, denoised) in raw.iter().zip(&denoised) {
        squared += f64::from((raw - denoised) * (raw - denoised));
    }
    let rms = (squared / raw.len().max(1) as f64).sqrt();
    println!("4096 samples: the filter moves the picture by {rms:.4} code values RMS");
    assert!(rms < 1.0, "a converged picture is not blurred: {rms:.4} code values RMS");
}

#[test]
fn a_converged_pixel_stops_at_the_floor_and_the_picture_is_still_the_room() {
    // The entry's third clause, adaptive retirement: a pixel whose error bar is under the target stops
    // being traced, and the count it stops at is the divisor its mean is averaged by. A white room
    // around a white plate is the deterministic case — one sample is exact — so every pixel retires at
    // the floor and the picture is still the radiance that was asked for.
    let Some((device, queue)) = gpu_device() else {
        println!("this machine has no GPU adapter to retire a pixel on");
        return;
    };
    let mut frame = SceneFrame::default();
    frame.push(Drawable::new(id(1), plate(-800.0, -800.0, 800.0, 800.0, 0.0), DMat4::IDENTITY, [1.0, 1.0, 1.0, 1.0]));
    frame.lighting.key_color = [0.0; 3];
    frame.lighting.points.clear();
    frame.lighting.environment = [1.0, 1.0, 1.0];
    let camera = Camera::new(DVec3::ZERO, 1500.0).expect("a camera looking at the plate");
    let request = Request::new(32, 32, 256).expect("a small render");
    let accumulation = trace_linear(&device, &queue, &frame, camera, request);
    let floor = crate::denoise::MIN_SAMPLES;
    assert_eq!(accumulation.sample_count(16, 16), Some(floor), "a converged pixel stops at the floor");
    assert_eq!(accumulation.sample_count(0, 0), Some(floor), "and so does the one beside it");
    let linear = linear_of(&accumulation);
    let centre = linear.get(16 * 32 + 16).copied().expect("the centre pixel");
    assert!((centre[0] - 1.0).abs() < 0.01, "the room is still white after the filter: {centre:?}");
    // The picture is not the filter's: a retired pixel's variance is zero, so it keeps its own value.
    let denoised = accumulation.denoised();
    let filtered = denoised.get(16 * 32 + 16).copied().expect("the centre pixel");
    assert!((filtered[0] - centre[0]).abs() < 1.0e-4, "a converged pixel is not moved: {centre:?} became {filtered:?}");
}

/// The irradiance a Lambertian rectangle gives a point on a parallel surface, in `W/m^2` of a unit
/// radiance source.
///
/// The integrand is the one the shader samples — `cos_emitter * cos_receiver / r^2` over the face —
/// integrated here by a midpoint rule fine enough that the number is the formula and not a guess.
fn irradiance_of_parallel_rectangle(centre: DVec3, half: (f64, f64), point: DVec3) -> f64 {
    const STEPS: usize = 120;
    let (step_x, step_y) = (half.0 * 2.0 / STEPS as f64, half.1 * 2.0 / STEPS as f64);
    let mut total = 0.0;
    for row in 0..STEPS {
        let y = -half.1 + (row as f64 + 0.5) * step_y;
        for column in 0..STEPS {
            let x = -half.0 + (column as f64 + 0.5) * step_x;
            let to_light = centre + DVec3::new(x, y, 0.0) - point;
            let squared = to_light.length_squared();
            // Both cosines are `dz / r` for two parallel faces, so the integrand is `dz^2 / r^4`.
            total += to_light.z * to_light.z / (squared * squared);
        }
    }
    total * step_x * step_y
}

/// The world point a pixel's centre ray meets the `z = 0` plate, or `None` for a ray that misses it.
fn plate_point_of(camera: Camera, request: Request, x: u32, y: u32) -> Option<DVec3> {
    let aspect = crate::view::aspect(request.width(), request.height())?;
    let ndc_x = 2.0 * (f64::from(x) + 0.5) / f64::from(request.width()) - 1.0;
    let ndc_y = 1.0 - 2.0 * (f64::from(y) + 0.5) / f64::from(request.height());
    let ray = camera.ray_ndc(ndc_x, ndc_y, aspect);
    let (origin, direction) = (ray.origin(), ray.direction());
    if direction.z.abs() < 1.0e-9 {
        return None;
    }
    let t = -origin.z / direction.z;
    (t > 0.0).then(|| origin + direction * t)
}

#[test]
fn a_window_lights_the_plate_under_it_by_the_irradiance_a_rectangle_gives() {
    // The clause about a soft source: a diffuse plate under a one-metre window must read the
    // irradiance the rectangle gives it, which this test computes twice — once by the tracer sampling
    // the face by solid angle, and once by integrating the same cosine law here, pixel by pixel.
    let Some((device, queue)) = gpu_device() else {
        println!("this machine has no GPU adapter to hold a window over a plate");
        return;
    };
    let half = 100.0;
    let distance = 1_000.0;
    let power = 100.0;
    let mut frame = SceneFrame::default();
    let white = [0.8, 0.8, 0.8, 1.0];
    frame.push(Drawable::new(id(1), plate(-2_000.0, -2_000.0, 2_000.0, 2_000.0, 0.0), DMat4::IDENTITY, white));
    // No key light and no studio: the window is the only light in the picture.
    frame.lighting.key_color = [0.0; 3];
    frame.lighting.environment = [0.0; 3];
    let window = scene::AreaLight {
        origin: DVec3::new(0.0, 0.0, distance),
        half_width: DVec3::new(half, 0.0, 0.0),
        half_height: DVec3::new(0.0, half, 0.0),
        color: [1.0; 3],
        power: power as f32,
    };
    frame.lighting.areas.push(window);
    // A second window of a different power and size beside the first, so the draw between two faces
    // and the power-weighted share are exercised on the same plate: MIS is only unbiased if the
    // strategy's own share is in both densities (L2).
    let narrow = scene::AreaLight {
        origin: DVec3::new(900.0, -400.0, 700.0),
        half_width: DVec3::new(300.0, 0.0, 0.0),
        half_height: DVec3::new(0.0, 450.0, 0.0),
        color: [0.9, 0.95, 1.0],
        power: 60.0,
    };
    frame.lighting.areas.push(narrow);
    // A camera under the window's edge, looking at the plate: its rays meet the floor, never the pane.
    let eye = DVec3::new(0.0, -3_000.0, 400.0);
    let forward = (-eye).try_normalize().unwrap_or(DVec3::Z);
    let right = forward.cross(DVec3::Z).try_normalize().unwrap_or(DVec3::X);
    let up = right.cross(forward).try_normalize().unwrap_or(DVec3::Z);
    let camera = Camera::default().looked_from(eye, right, up, forward);

    let request = Request::new(48, 48, 512).expect("a small render");
    let linear = linear_of(&trace_linear(&device, &queue, &frame, camera, request));

    // A patch of pixels, compared with the same patch of the analytic profile: one pixel's estimate
    // is a stopping rule's answer (the pixel retires when it looks converged), while a patch is the
    // estimator's expectation.
    assert!(
        (f64::from(window.radiance()[0]) - power / (std::f64::consts::PI * (2.0 * half / 1_000.0).powi(2))).abs()
            < 1.0e-2,
        "the frame's watts became radiance"
    );
    let mut measured = 0.0;
    let mut expected = 0.0;
    let mut counted = 0.0;
    for y in 22..27u32 {
        for x in 22..27u32 {
            let Some(point) = plate_point_of(camera, request, x, y) else {
                continue;
            };
            let value = linear.get((y * request.width() + x) as usize).and_then(|p| p.first()).copied();
            let value = f64::from(value.expect("the pixel is inside the image"));
            let irradiance = f64::from(window.radiance()[0])
                * irradiance_of_parallel_rectangle(window.origin, (half, half), point)
                + f64::from(narrow.radiance()[0])
                    * irradiance_of_parallel_rectangle(narrow.origin, (300.0, 450.0), point);
            measured += value;
            expected += 0.8 / std::f64::consts::PI * irradiance;
            counted += 1.0;
        }
    }
    assert!(counted >= 20.0, "the patch is on the plate");
    let (measured, expected) = (measured / counted, expected / counted);
    let relative = (measured - expected).abs() / expected;
    assert!(relative < 0.02, "the plate's patch reads {measured}, the cosine law says {expected}: {relative} off");

    // A window that faces away gives the plate nothing, and neither does one of no power: the face
    // emits from one side only, and a dark rectangle is a dark rectangle.
    let mut turned = frame.clone();
    // The same face the other way round: the height edge turned over is what makes it emit upwards.
    turned.lighting.areas = vec![scene::AreaLight { half_height: DVec3::new(0.0, -half, 0.0), ..window }];
    let dark = linear_of(&trace_linear(&device, &queue, &turned, camera, Request::new(24, 24, 16).expect("small")));
    let biggest = dark.iter().flatten().fold(0.0f64, |a, b| a.max(f64::from(*b)));
    assert!(biggest <= 1.0e-6, "a window seen from behind lights nothing: {biggest}");
    let mut unlit = frame.clone();
    unlit.lighting.areas = vec![scene::AreaLight { power: 0.0, ..window }];
    let unlit = linear_of(&trace_linear(&device, &queue, &unlit, camera, Request::new(24, 24, 16).expect("small")));
    assert!(unlit.iter().flatten().all(|channel| *channel <= 0.0), "a window of no power lights nothing");
}

#[test]
fn a_render_through_a_pinhole_is_the_render_it_always_was() {
    // The clause about costing nothing: a lens of no radius is the pinhole, sample for sample, so a
    // caller who names a focus and no aperture gets the identical picture (Y5).
    let (frame, camera) = fixture();
    let Some((device, queue)) = gpu_device() else {
        println!("this machine has no GPU adapter to compare two pinholes on");
        return;
    };
    let request = Request::new(32, 24, 16).expect("a small render");
    let flat = linear_of(&trace_linear(&device, &queue, &frame, camera, request));
    let opened = camera.with_lens(0.0, 2_000.0).expect("a lens of no radius");
    let again = linear_of(&trace_linear(&device, &queue, &frame, opened, request));
    assert_eq!(flat, again, "a pinhole is the picture every render had");

    // And the focus of a pinhole is a number nothing reads: two different ones are one picture.
    let nearer = camera.with_lens(0.0, 300.0).expect("a focus of no aperture");
    let closer = linear_of(&trace_linear(&device, &queue, &frame, nearer, request));
    assert_eq!(flat, closer, "a pinhole does not focus anywhere");
}

/// A small bright square at `(x, y, z)` facing the camera, with nothing else in the frame.
fn dot_at(x: f64, y: f64, z: f64, side: f64) -> (SceneFrame, Camera) {
    let mut frame = SceneFrame::default();
    let half = side * 0.5;
    let positions = vec![
        [(-half) as f32, (-half) as f32, 0.0],
        [half as f32, (-half) as f32, 0.0],
        [half as f32, half as f32, 0.0],
        [(-half) as f32, half as f32, 0.0],
    ];
    let mesh = mesh(positions, vec![0, 1, 2, 0, 2, 3]);
    let transform = DMat4::from_translation(DVec3::new(x, y, z));
    // The square lies in the XY plane facing -Z, which is where the camera and the key light stand.
    frame.push(Drawable::new(id(1), mesh, transform, [0.8, 0.8, 0.8, 1.0]));
    // The quad lies in the XY plane, so the light comes from -Z, which is where the camera stands.
    frame.lighting.key_direction = -DVec3::Z;
    frame.lighting.key_color = [std::f64::consts::PI as f32; 3];
    frame.lighting.environment = [0.0; 3];
    // The camera stays on the view's axis, so the quad's own place is what the picture measures.
    let eye = DVec3::new(0.0, 0.0, z - 1_000.0);
    let camera = Camera::default().looked_from(eye, DVec3::X, DVec3::Y, DVec3::Z);
    (frame, camera)
}

#[test]
fn a_wide_aperture_blurs_a_point_by_the_thin_lens_circle() {
    // The clause about the lens, measured on a picture: a board a metre in front of a camera focused at
    // two metres, seen through a lens of a hundred millimetres, has an edge that ramps over exactly the
    // circle of confusion the thin lens predicts.
    //
    // A disc kernel ramps from a tenth to nine tenths of the board's radiance across `0.695` of its
    // diameter (`C(s) = 0.5 - (s sqrt(r^2-s^2) + r^2 asin(s/r)) / (pi r^2)`), which is one number and
    // a row of pixels — a statistic of a *plane*, not of a dot, so a hundred samples of a point that
    // is smaller than a pixel are not what this test is made of.
    let Some((device, queue)) = gpu_device() else {
        println!("this machine has no GPU adapter to blur an edge with");
        return;
    };
    const HALF_WIDTH: f64 = 0.695;
    let distance = 1_000.0;
    // The board's left edge passes through the view's axis, which is where the thin lens is exact: a
    // point off the axis is a different distance from the eye, and the circle of confusion is a
    // statement about a plane's distance along the axis.
    let (frame, camera) = dot_at(300.0, 0.0, distance, 600.0);
    let focused = camera.with_lens(100.0, 2_000.0).expect("a lens");
    let request = Request::new(129, 129, 512).expect("a small render");
    let linear = linear_of(&trace_linear(&device, &queue, &frame, focused, request));

    // The board's left edge stands at `x = -300` millimetres, upright, so every row between the board's
    // top and bottom draws the same ramp: the profile is the mean of forty of them, which is what takes
    // the noise of one row out of a crossing.
    let profile = |linear: &[[f32; 3]]| -> Vec<f64> {
        (0..request.width())
            .map(|x| {
                let mut total = 0.0;
                for y in 44..84u32 {
                    total += f64::from(linear[(y * request.width() + x) as usize][0]);
                }
                total / 40.0
            })
            .collect()
    };
    let values = profile(&linear);
    let plateau = values[70..90].iter().sum::<f64>() / 20.0;
    assert!(plateau > 0.5, "the board is lit: {plateau}");
    // The crossing of a share of the plateau, interpolated between the two pixels that straddle it.
    let crossing = |share: f64| -> f64 {
        let level = plateau * share;
        for (x, pair) in values.windows(2).enumerate() {
            let (before, after) = (pair[0], pair[1]);
            if before < level && after >= level {
                let span = after - before;
                let within = if span > 0.0 { (level - before) / span } else { 0.0 };
                return x as f64 + within;
            }
        }
        f64::NAN
    };
    let (ten, ninety) = (crossing(0.1), crossing(0.9));
    assert!(ten.is_finite() && ninety.is_finite(), "the edge is inside the row: {ten} and {ninety}");
    let measured = (ten - ninety).abs() / HALF_WIDTH;

    let expected = crate::view::confusion_in_pixels(focused, distance, request.height());
    let relative = (measured - expected).abs() / expected;
    assert!(relative < 0.02, "the edge ramps over {measured} pixels, the thin lens says {expected} ({relative} off)");

    // A pinhole has no ramp at all: the same row jumps from black to the board within a pixel.
    let sharp = linear_of(&trace_linear(&device, &queue, &frame, camera, request));
    let sharp_values = profile(&sharp);
    let sharp_plateau = sharp_values[70..90].iter().sum::<f64>() / 20.0;
    let sharp_ten = sharp_values.iter().position(|value| *value >= sharp_plateau * 0.1).unwrap_or(0) as f64;
    let sharp_ninety = sharp_values.iter().position(|value| *value >= sharp_plateau * 0.9).unwrap_or(0) as f64;
    assert!((sharp_ten - sharp_ninety).abs() <= 1.5, "a pinhole draws the edge in one pixel");
}

#[test]
fn a_shallow_focus_reaches_the_reference_noise_level_at_its_own_sample_count() {
    // The clause about noise: a lens spends the same samples the estimator always spent, so the count
    // a person waits for must still land inside the two percent the pinhole image does (Y2). A wide
    // aperture is the worst case — the picture mixes two strategies across a blur — so the count this
    // test measures is the count `plan/notes/Y5.md` records.
    let (frame, camera) = fixture();
    let Some((device, queue)) = gpu_device() else {
        println!("this machine has no GPU adapter to converge a lens on");
        return;
    };
    let lensed = camera.with_lens(90.0, 2_200.0).expect("a lens");
    let size = Request::new(48, 36, 1).expect("a small image");
    let request = |samples: u32| Request::new(size.width(), size.height(), samples).expect("a sample count");
    let reference = linear_of(&trace_linear(&device, &queue, &frame, lensed, request(4096)));
    let rough = linear_of(&trace_linear(&device, &queue, &frame, lensed, request(256)));
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
    let rmse = (squared / counted).sqrt();
    let rms = (power / counted).sqrt();
    assert!(rms > 0.01, "the fixture is lit through the lens: rms {rms}");
    let relative = rmse / rms;
    assert!(
        relative < 0.02,
        "256 samples through a lens are within two percent of 4096: {relative} (rmse {rmse}, rms {rms})"
    );
}
