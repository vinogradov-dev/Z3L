//! The interface rendered offscreen: one PNG per cell of the look matrix (U81).
//!
//! The eyes-only list in `.pi/skills/ui-design/references/checklist.md` — the light theme, a panel
//! at 55 % over the model, a 200 % scale — cannot be answered from rectangles, and until now the
//! only way to answer it was to start the window while the owner was working. A shot draws the real
//! frame into a texture instead of a window, copies the pixels back and writes them under
//! `target/ui-shots`, so a session reads a picture without touching the owner's screen.
//!
//! The one test is `#[ignore]`: `cargo test` never touches a GPU, and a machine with no adapter
//! prints one sentence and skips. The pictures are looked at, never compared pixel for pixel.
//!
//! Changes when: the matrix, a shot's file name or the offscreen paint path changes.

#![allow(clippy::expect_used)]

use std::path::Path;
use std::sync::mpsc;

use eframe::egui;
use eframe::egui_wgpu;
use eframe::wgpu;
use ui::settings::{
    DEFAULT_CURSOR_POINTS, DEFAULT_PANEL_OPACITY, MAX_HANDLE_POINTS, MAX_PANEL_OPACITY, MAX_RING_PERCENT,
    MIN_HANDLE_POINTS, MIN_PANEL_OPACITY, MIN_RING_PERCENT, PanelState, Settings, ThemeChoice,
};
use ui::theme;
use ui::theme::ThemePreset;
use ui::ui_scale;

use crate::App;
use crate::contour_gizmo;

/// The window a shot is drawn for, in points: the default launch size (M2).
const WINDOW: egui::Vec2 = egui::vec2(1280.0, 840.0);

/// How many frames a shot settles over before its image is taken.
///
/// `egui` runs a sizing pass for a new area, and the panels and the ribbon are measured on the frame
/// after they open (U78), so the picture is the third frame and never the first.
const SETTLE_FRAMES: usize = 3;

/// How far a shot moves the fitted view in, so the model runs under both floating panels.
///
/// The launch view fits the whole scene and leaves the middle of the window to it; the checklist
/// asks what a 55 % panel does *over the model*, so the picture is taken from a view the user could
/// have zoomed to, not from the fitted one (U81).
const MODEL_ZOOM: f64 = 2.5;

/// The panel opacities the matrix renders: the two ends of the slider and the default between them.
const OPACITIES: [u32; 3] = [MIN_PANEL_OPACITY, DEFAULT_PANEL_OPACITY, MAX_PANEL_OPACITY];

/// The two theme variants, in the order the matrix renders them.
const VARIANTS: [ThemeChoice; 2] = [ThemeChoice::Dark, ThemeChoice::Light];

/// The two fold states: both panels unfolded, then both folded to their strips.
const FOLDS: [bool; 2] = [false, true];

/// The two scales the matrix renders: the reference and the largest the control offers.
const SCALES: [u32; 2] = [100, ui_scale::MAX_PERCENT];

/// One cell of the matrix: what one picture shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Shot {
    /// The look the whole window is painted with.
    preset: ThemePreset,
    /// The theme variant.
    variant: ThemeChoice,
    /// The opacity of the chrome drawn over the scene, in percent.
    opacity: u32,
    /// Whether both side panels are folded to their strips.
    folded: bool,
    /// The UI scale, in percent.
    scale: u32,
}

impl Shot {
    /// The file name of this shot, which says the cell it renders.
    ///
    /// The preset and the variant are the names the workspace file stores, so a picture and the
    /// workspace that would draw it carry the same words.
    fn name(&self) -> String {
        let variant = match self.variant {
            ThemeChoice::Light => "light",
            ThemeChoice::Dark | ThemeChoice::System => "dark",
        };
        let fold = if self.folded { "folded" } else { "expanded" };
        format!("{}-{variant}-opacity{}-{fold}-scale{}.png", self.preset.stored_name(), self.opacity, self.scale)
    }

    /// The workspace settings this cell draws with.
    fn settings(&self) -> Settings {
        let mut settings = Settings::default();
        settings.appearance.palette = self.preset;
        settings.appearance.theme = self.variant;
        settings.appearance.panel_opacity = self.opacity;
        settings.appearance.ui_scale = self.scale;
        let panel = PanelState { visible: true, expanded: !self.folded };
        settings.panels.scene = panel;
        settings.panels.inspector = panel;
        settings
    }
}

/// Every cell of the matrix: preset × variant × opacity × fold × scale.
fn shots() -> Vec<Shot> {
    let count = ThemePreset::ALL.len() * VARIANTS.len() * OPACITIES.len() * FOLDS.len() * SCALES.len();
    let mut shots = Vec::with_capacity(count);
    for preset in ThemePreset::ALL {
        for variant in VARIANTS {
            for opacity in OPACITIES {
                for folded in FOLDS {
                    for scale in SCALES {
                        shots.push(Shot { preset, variant, opacity, folded, scale });
                    }
                }
            }
        }
    }
    shots
}

/// The pixels one shot wrote, kept so the test can compare two cells without reading the files.
struct Picture {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl Picture {
    /// The number of pixels whose colour differs between two pictures.
    fn differing_pixels(&self, other: &Self) -> usize {
        if (self.width, self.height) != (other.width, other.height) {
            return self.width as usize * self.height as usize;
        }
        let (a, _) = self.rgba.as_chunks::<4>();
        let (b, _) = other.rgba.as_chunks::<4>();
        a.iter().zip(b).filter(|(a, b)| a != b).count()
    }

    /// The number of pixels within [`INK_TOLERANCE`] of `ink`.
    fn near(&self, ink: egui::Color32) -> usize {
        let (pixels, _) = self.rgba.as_chunks::<4>();
        pixels
            .iter()
            .filter(|pixel| {
                let gap = |channel: u8, ink: u8| i32::from(channel) - i32::from(ink);
                let (red, green, blue) = (gap(pixel[0], ink.r()), gap(pixel[1], ink.g()), gap(pixel[2], ink.b()));
                red * red + green * green + blue * blue <= INK_TOLERANCE * INK_TOLERANCE
            })
            .count()
    }

    /// The number of pixels in the picture.
    fn pixels(&self) -> usize {
        self.width as usize * self.height as usize
    }

    /// Whether the picture is a single flat colour, which no interface is.
    fn is_flat(&self) -> bool {
        let (pixels, _) = self.rgba.as_chunks::<4>();
        let Some(first) = pixels.first() else {
            return true;
        };
        !pixels.iter().any(|pixel| pixel != first)
    }
}

/// A device and a queue with no surface behind them, or the sentence that says why there is none.
///
/// The upload probe in `scene_build` and the agent's preview borrow the same device, so the shell
/// creates one only once (`crate::preview::render_state`).
pub(crate) fn render_state() -> Result<egui_wgpu::RenderState, String> {
    crate::preview::render_state()
}

/// Draws one cell offscreen, writes it to `path` and returns what it wrote.
fn capture(render_state: &egui_wgpu::RenderState, shot: &Shot, path: &Path) -> Result<Picture, String> {
    let ctx = egui::Context::default();
    let workspace = testkit::scratch_dir("ui-shots").join("workspace.json");
    let session = crate::scene_build::demo_session()?;
    let settings = shot.settings();
    let mut app = App::test_app(&ctx, Some(render_state.clone()), workspace, settings, session)?;
    app.tabs.active_mut().zoom_active_view(1.0 / MODEL_ZOOM);

    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, WINDOW);
    let jobs = settle(&mut app, &ctx, render_state, screen);
    let pixels_per_point = ctx.pixels_per_point();
    let size =
        [(WINDOW.x * pixels_per_point).round().max(1.0) as u32, (WINDOW.y * pixels_per_point).round().max(1.0) as u32];
    let rgba = paint(render_state, &jobs, size, pixels_per_point)?;
    // The scene texture is registered on the window's renderer and does not go away with the
    // document, so the next shot would leave one target per picture behind (U81).
    app.tabs.active_mut().release_target();
    testkit::write_png(path, size[0], size[1], &rgba).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(Picture { width: size[0], height: size[1], rgba })
}

/// Runs the interface for [`SETTLE_FRAMES`] frames and returns the last frame's paint jobs.
///
/// Every frame's texture deltas reach the renderer, because the font atlas grows between frames and
/// the last frame's glyphs were uploaded by the ones before it; only the last frame is painted.
fn settle(
    app: &mut App,
    ctx: &egui::Context,
    render_state: &egui_wgpu::RenderState,
    screen: egui::Rect,
) -> Vec<egui::ClippedPrimitive> {
    let mut jobs = Vec::new();
    for _ in 0..SETTLE_FRAMES {
        jobs = run_frame(app, ctx, render_state, screen, Vec::new());
    }
    jobs
}

/// Draws one frame of `app` with `events`, uploads what it new-textured and returns the jobs.
fn run_frame(
    app: &mut App,
    ctx: &egui::Context,
    render_state: &egui_wgpu::RenderState,
    screen: egui::Rect,
    events: Vec<egui::Event>,
) -> Vec<egui::ClippedPrimitive> {
    paint_frame(ctx, render_state, screen, events, |ui| {
        app.frame(ui);
    })
}

/// Draws one frame of `draw` with `events`, uploads what it new-textured and returns the jobs.
///
/// A shot of one overlay paints that overlay instead of the whole window, so the picture can be
/// taken against a surface the shell would not open on its own (X97's two boards).
fn paint_frame(
    ctx: &egui::Context,
    render_state: &egui_wgpu::RenderState,
    screen: egui::Rect,
    events: Vec<egui::Event>,
    draw: impl FnMut(&mut egui::Ui),
) -> Vec<egui::ClippedPrimitive> {
    let mut output = ctx.run_ui(egui::RawInput { screen_rect: Some(screen), events, ..Default::default() }, draw);
    let mut delta = std::mem::take(&mut output.textures_delta);
    {
        let mut renderer = render_state.renderer.write();
        for (id, images) in &delta.set {
            for image in images {
                renderer.update_texture(&render_state.device, &render_state.queue, *id, image);
            }
        }
        for id in &delta.free {
            renderer.free_texture(id);
        }
    }
    // `epaint` asserts on drop that a delta reached a renderer; the renderer above is that
    // renderer, and `clear` is how it stops asking.
    delta.clear();
    ctx.tessellate(output.shapes, output.pixels_per_point)
}

/// Paints the interface into a texture of `size` and copies its pixels back, top row first.
fn paint(
    render_state: &egui_wgpu::RenderState,
    jobs: &[egui::ClippedPrimitive],
    size: [u32; 2],
    pixels_per_point: f32,
) -> Result<Vec<u8>, String> {
    // No surface means the renderer picks `Rgba8Unorm`, which is the channel order the PNG gets;
    // a device that ever answers with another order would write a blue sky red, so it is refused.
    if render_state.target_format != wgpu::TextureFormat::Rgba8Unorm {
        return Err(format!("a shot needs an RGBA target, this device paints {:?}", render_state.target_format));
    }
    let device = &render_state.device;
    let queue = &render_state.queue;
    let extent = wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Z3L interface shot"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: render_state.target_format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let descriptor = egui_wgpu::ScreenDescriptor { size_in_pixels: size, pixels_per_point };
    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Z3L interface shot") });
    let buffers = {
        let mut renderer = render_state.renderer.write();
        renderer.update_buffers(device, queue, &mut encoder, jobs, &descriptor)
    };
    {
        let attachment = Some(wgpu::RenderPassColorAttachment {
            view: &view,
            depth_slice: None,
            resolve_target: None,
            // The frame paints its own background on every pixel, so the clear colour is only what
            // a gap would show: a hostile layout is easier to see against black than against white.
            ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
        });
        let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Z3L interface shot"),
            color_attachments: &[attachment],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let mut pass = pass.forget_lifetime();
        render_state.renderer.read().render(&mut pass, jobs, &descriptor);
    }

    // A copy out of a texture pads every row to a 256-byte boundary, so the buffer is wider than
    // the picture and the padding is dropped on the way back to a tightly packed row.
    let row_bytes = size[0].checked_mul(4).ok_or_else(|| format!("a {} pixel row does not fit", size[0]))?;
    let padded = row_bytes.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Z3L interface shot pixels"),
        size: u64::from(padded) * u64::from(size[1]),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(size[1]),
            },
        },
        extent,
    );
    queue.submit(buffers.into_iter().chain(std::iter::once(encoder.finish())));

    let slice = buffer.slice(..);
    let (sender, receiver) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|error| format!("the shot did not reach the CPU: {error}"))?;
    receiver
        .recv()
        .map_err(|error| format!("the shot was never mapped: {error}"))?
        .map_err(|error| format!("the shot could not be mapped: {error}"))?;

    let (padded, row_bytes) = (padded as usize, row_bytes as usize);
    let mut rgba = Vec::with_capacity(row_bytes.saturating_mul(size[1] as usize));
    let mut short = None;
    {
        let mapped = slice.get_mapped_range().map_err(|error| format!("the shot could not be read: {error}"))?;
        for row in 0..size[1] as usize {
            match mapped.get(row * padded..row * padded + row_bytes) {
                Some(line) => rgba.extend_from_slice(line),
                None => {
                    short = Some(row);
                    break;
                }
            }
        }
    }
    buffer.unmap();
    match short {
        Some(row) => Err(format!("the copy of row {row} is shorter than the picture")),
        None => Ok(rgba),
    }
}

#[test]
#[ignore = "writes target/ui-shots/*.png; run `cargo test -p z3l --bin z3l -- --ignored`"]
fn the_interface_renders_offscreen_to_one_png_per_cell() {
    let render_state = match render_state() {
        Ok(render_state) => render_state,
        Err(reason) => {
            println!("{reason}");
            return;
        }
    };
    let directory = testkit::ui_shots_dir().expect("the shots directory is created");

    // The one pair the test compares: the same cell at the two ends of the opacity slider. Every
    // other cell is written and dropped, because a picture of a 200 % window is seventeen megabytes
    // and the matrix would hold most of a gigabyte to compare two of them.
    let transparent = Shot {
        preset: ThemePreset::GraphiteCopper,
        variant: ThemeChoice::Dark,
        opacity: MIN_PANEL_OPACITY,
        folded: false,
        scale: 100,
    };
    let opaque = Shot { opacity: MAX_PANEL_OPACITY, ..transparent };
    let mut pair: [Option<Picture>; 2] = [None, None];

    let shots = shots();
    for shot in &shots {
        let path = directory.join(shot.name());
        let picture = capture(&render_state, shot, &path).expect("every cell renders");
        assert!(!picture.is_flat(), "{} draws less than one colour", path.display());
        if *shot == transparent {
            pair[0] = Some(picture);
        } else if *shot == opaque {
            pair[1] = Some(picture);
        }
    }
    println!("{} interface shots in {}", shots.len(), directory.display());

    // A 55 % panel and an opaque one reach different pixels: the scene under the chrome shows
    // through one and not the other. This is the only comparison the test makes; contrast, washout
    // and whether the model is legible through the fill are for the person looking.
    let transparent = pair[0].take().expect("the matrix drew the 55 % cell");
    let opaque = pair[1].take().expect("the matrix drew the 100 % cell");
    let differing = transparent.differing_pixels(&opaque);
    assert!(
        differing > 1_000,
        "the chrome reaches {differing} pixels at {MIN_PANEL_OPACITY} % but {MAX_PANEL_OPACITY} % draws the same picture"
    );
}

/// Draws the render place into one picture: the numbers, the destination and a run's progress.
///
/// The place replaces the workspace, so the picture shows what a person sees after pressing the
/// `Render` tab — and it is the only way to judge the page's density and its progress row, which a
/// rectangle test cannot answer (ADR 0211).
#[test]
#[ignore = "needs a GPU adapter; writes target/ui-shots/render-place.png"]
fn the_render_place_is_painted_offscreen() {
    let render_state = match render_state() {
        Ok(render_state) => render_state,
        Err(reason) => {
            println!("{reason}");
            return;
        }
    };
    let directory = testkit::ui_shots_dir().expect("the shots directory is created");
    let ctx = egui::Context::default();
    let workspace = testkit::scratch_dir("ui-shots/render-place").join("workspace.json");
    let session = crate::scene_build::demo_session().expect("the sample session");
    let mut app = App::test_app(&ctx, Some(render_state.clone()), workspace, Settings::default(), session)
        .expect("the shell builds");
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, WINDOW);
    let _ = settle(&mut app, &ctx, &render_state, screen);
    // The place, with a destination and a run that reports progress: the shot is taken between two
    // frames of a real render, so the bar and the counter are what the page actually draws.
    app.render_open = true;
    // A real run at a size that finishes in a second, so the picture shows the page's own report
    // line rather than a stage no person has reached yet.
    let image = directory.join("render-place-example.png");
    let _ = std::fs::remove_file(&image);
    app.render_place.set_path(image.clone());
    app.render_place.settings().width = 480;
    app.render_place.settings().height = 360;
    app.render_place.settings().samples = 4;
    app.start_render(&ctx);
    app.render_place.wait();
    assert!(image.is_file(), "the place wrote its image");
    let mut jobs = Vec::new();
    for _ in 0..SETTLE_FRAMES {
        jobs = run_frame(&mut app, &ctx, &render_state, screen, Vec::new());
    }
    let pixels_per_point = ctx.pixels_per_point();
    let size =
        [(WINDOW.x * pixels_per_point).round().max(1.0) as u32, (WINDOW.y * pixels_per_point).round().max(1.0) as u32];
    let rgba = paint(&render_state, &jobs, size, pixels_per_point).expect("the frame paints");
    let path = directory.join("render-place.png");
    testkit::write_png(&path, size[0], size[1], &rgba).expect("the picture is written");
    app.tabs.active_mut().release_target();
    let picture = Picture { width: size[0], height: size[1], rgba };
    assert!(!picture.is_flat(), "{} draws less than one colour", path.display());
    println!("the render place is in {}", path.display());
}

/// Draws the box tool's base rectangle and pulled height into one picture.
///
/// The gesture is driven through the real shell with pointer events, so the shot proves the whole
/// path — the plane mapping, the wireframe prism, the live size and the grips — not only the shape
/// the editor holds. Escape drops the draft afterwards: the shot writes no scene.
#[test]
#[ignore = "needs a GPU adapter; writes target/ui-shots/box-tool.png"]
fn the_box_tool_gesture_is_painted_offscreen() {
    let render_state = match render_state() {
        Ok(render_state) => render_state,
        Err(reason) => {
            println!("{reason}");
            return;
        }
    };
    let ctx = egui::Context::default();
    let workspace = testkit::scratch_dir("ui-shots").join("box-workspace.json");
    let session = engine::Session::with_directory(testkit::sample_project(), testkit::sample_catalog());
    let mut app = App::test_app(&ctx, Some(render_state.clone()), workspace, Settings::default(), session)
        .expect("the application builds");
    // The fill cells live on the Furniture tab, and the picture must show the armed cell, not the
    // tab a fresh workspace happens to open.
    if let Some(tab) = ui::toolbars::spec::tab("furniture") {
        app.tabs.active_mut().editor.set_ribbon_tab(tab);
    }
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, WINDOW);
    let _ = settle(&mut app, &ctx, &render_state, screen);

    // Two points on the work plane, and the height the pointer pulls above the base.
    let corners = {
        let document = app.tabs.active_mut();
        // The frame cell arms the sketch's own rectangle tool, so the base is two clicks of the
        // sketch draft and the height is pulled from the figure it makes (N19).
        document.editor.start_product_box_drawing(ui::DrawTool::Rectangle);
        let rect = document.scene_rect().expect("the settled frame gives the scene a rectangle");
        let camera = document.camera();
        let project = document.session.project();
        let _ = project;
        let corner = |x: f64, y: f64, z: f64| {
            crate::gizmo_math::screen_point(camera, rect, geom::DVec3::new(x, y, z))
                .expect("the work plane is on screen")
        };
        // The pull follows the normal through the base corner, which is where a maker drags it.
        let anchor = corner(-300.0, -200.0, 0.0);
        let pull = egui::Pos2::new(anchor.x, anchor.y - 140.0);
        (anchor, corner(300.0, 200.0, 0.0), pull)
    };
    let click = |pointer: egui::Pos2, pressed: bool| egui::Event::PointerButton {
        pos: pointer,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let frames = vec![
        vec![egui::Event::PointerMoved(corners.0), click(corners.0, true), click(corners.0, false)],
        vec![egui::Event::PointerMoved(corners.1)],
        vec![click(corners.1, true), click(corners.1, false)],
        vec![egui::Event::PointerMoved(corners.2)],
        // One more frame at the same pointer: the card's area lays itself out on the frame it
        // appears, so the picture is taken once it has.
        vec![egui::Event::PointerMoved(corners.2)],
    ];
    let mut jobs = Vec::new();
    for events in frames {
        let mut output = ctx.run_ui(egui::RawInput { screen_rect: Some(screen), events, ..Default::default() }, |ui| {
            app.frame(ui);
        });
        let mut delta = std::mem::take(&mut output.textures_delta);
        {
            let mut renderer = render_state.renderer.write();
            for (id, images) in &delta.set {
                for image in images {
                    renderer.update_texture(&render_state.device, &render_state.queue, *id, image);
                }
            }
            for id in &delta.free {
                renderer.free_texture(id);
            }
        }
        delta.clear();
        jobs = ctx.tessellate(output.shapes, output.pixels_per_point);
    }
    {
        let document = app.tabs.active_mut();
        let draft = document.editor.box_draft().cloned().expect("the gesture is pulling its height");
        assert!(draft.height().is_positive(), "the height follows the pointer");
        assert_eq!(draft.sizes().map(|sizes| sizes[0].is_positive()), Some(true), "the base is the drawn box");
        assert!(document.tool_card_is_up(), "the height pull draws its tool card: {draft:?}");
    }
    let pixels_per_point = ctx.pixels_per_point();
    let size =
        [(WINDOW.x * pixels_per_point).round().max(1.0) as u32, (WINDOW.y * pixels_per_point).round().max(1.0) as u32];
    let rgba = paint(&render_state, &jobs, size, pixels_per_point).expect("the frame paints");
    let directory = testkit::ui_shots_dir().expect("the shots directory is created");
    let path = directory.join("box-tool.png");
    testkit::write_png(&path, size[0], size[1], &rgba).expect("the picture is written");
    app.tabs.active_mut().release_target();
    let picture = Picture { width: size[0], height: size[1], rgba };
    assert!(!picture.is_flat(), "{} draws less than one colour", path.display());
    println!("the box tool is in {}", path.display());
}

/// The size of the contour overlay's own shot, in points.
const OVERLAY_SHOT: egui::Vec2 = egui::vec2(520.0, 360.0);

/// The scale the overlay's shot is drawn at, in pixels per point.
///
/// The pair is 2 and 3.5 points wide, and at 1:1 the casing under the ink is a band under a pixel:
/// a stroke that thin cannot be counted, and this picture is here to be counted as well as looked
/// at. At 2:1 the casing is a band of a pixel and a half and both inks have a solid core; the
/// picture is the same drawing at double size, so what it shows is what a person sees magnified.
const OVERLAY_SCALE: f32 = 2.0;

/// How far the camera of the overlay's shot stands from the drawing, in millimetres.
const OVERLAY_CAMERA_MM: f64 = 750.0;

/// The point of the drawing the overlay's camera looks at, in millimetres: the middle of the
/// sketch, so the whole shape is in the picture.
const OVERLAY_CAMERA_TARGET: [f64; 3] = [150.0, 80.0, 0.0];

/// How far one pixel of a shot may be from an ink and still count as that ink.
///
/// The strokes have solid cores, so the tolerance only absorbs the rounding of the colour that
/// reaches the target; it is smaller than the distance between any two of the inks and the board.
const INK_TOLERANCE: i32 = 12;

/// The two boards the overlay is drawn over: the bright beech face every panel gets, and the dark
/// tone every edge band wears (`scene_build`'s v1 placeholder materials).
///
/// A colour literal is not the theme's business in a test, and these two are read from the scene
/// rather than picked here, so a picture of the overlay moves when the material does.
fn boards() -> [(&'static str, egui::Color32); 2] {
    [
        ("beech", screen_ink(crate::scene_build::PANEL_COLORS[0])),
        ("band", screen_ink(crate::scene_build::EDGE_BAND_COLOR)),
    ]
}

/// A linear scene colour as the screen draws it: the sRGB transfer of each channel, without the
/// final render's tone curve, because the shot paints a surface rather than the light on one.
fn screen_ink(linear: [f32; 4]) -> egui::Color32 {
    let channel = |value: f32| {
        let value = value.clamp(0.0, 1.0);
        let encoded = if value <= 0.003_130_8 { value * 12.92 } else { 1.055 * value.powf(1.0 / 2.4) - 0.055 };
        (encoded * 255.0).round() as u8
    };
    egui::Color32::from_rgb(channel(linear[0]), channel(linear[1]), channel(linear[2]))
}

/// A project with one closed sketch, and an edit of it that has already moved a corner.
///
/// The working copy differs from the shape the edit started from, which is the state the owner's
/// report is about: both are on screen at once and the change has to be readable.
fn overlay_draft() -> (model::Project, ui::ContourEdit) {
    let mut project = testkit::sample_project();
    let root = project.root().id();
    let rectangle = model::Path::polyline(
        vec![
            [testkit::millimetres(0), testkit::millimetres(0)],
            [testkit::millimetres(300), testkit::millimetres(0)],
            [testkit::millimetres(300), testkit::millimetres(200)],
            [testkit::millimetres(0), testkit::millimetres(200)],
        ],
        true,
    )
    .expect("a closed rectangle");
    let sketch = project
        .add_object(root, "Sketch", model::Placement::IDENTITY, model::Geometry::Path(rectangle))
        .expect("a path object");
    let mut edit = ui::ContourEdit::start(&project, ui::ContourTarget::Path(sketch)).expect("a path is editable");
    let first = edit.draft().id_at(0).expect("a closed sketch has a first side");
    edit.move_corner_at(
        model::PathCorner::Start(first),
        [testkit::millimetres(60), testkit::millimetres(-40)],
        model::CornerMove::Free,
    )
    .expect("the corner moves");
    (project, edit)
}

/// Draws the contour overlay over a light and a dark board, in both variants (X97).
///
/// The owner reported the two shapes as almost one colour, which a rectangle test cannot answer:
/// the question is what the two inks look like over a material, and a material is a picture. The
/// shot paints the real gizmo — base, casing, working copy and grips — over the bright beech face
/// and the dark band tone, in both variants, and counts the pixels of each ink, so the pair is
/// known to reach the screen and the board to remain the ground of the picture.
#[test]
#[ignore = "needs a GPU adapter; writes target/ui-shots/contour-overlay-*.png"]
fn the_contour_overlay_reads_over_a_light_and_a_dark_board() {
    let render_state = match render_state() {
        Ok(render_state) => render_state,
        Err(reason) => {
            println!("{reason}");
            return;
        }
    };
    let directory = testkit::ui_shots_dir().expect("the shots directory is created");
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, OVERLAY_SHOT);
    let [x, y, z] = OVERLAY_CAMERA_TARGET;
    let camera =
        scene::Camera::new(geom::DVec3::new(x, y, z), OVERLAY_CAMERA_MM).expect("a camera at the shot's distance");
    let (project, edit) = overlay_draft();

    for dark in [true, false] {
        for (board_name, board) in boards() {
            let ctx = egui::Context::default();
            ctx.set_pixels_per_point(OVERLAY_SCALE);
            let preference = if dark { egui::ThemePreference::Dark } else { egui::ThemePreference::Light };
            theme::install_with(&ctx, preference, ThemePreset::GraphiteCopper, None);
            let mut inks = (egui::Color32::TRANSPARENT, egui::Color32::TRANSPARENT, egui::Color32::TRANSPARENT);
            let mut jobs = Vec::new();
            for _ in 0..SETTLE_FRAMES {
                jobs = paint_frame(&ctx, &render_state, screen, Vec::new(), |ui| {
                    ui.painter().rect_filled(screen, 0.0, board);
                    inks = (theme::original(ui), theme::working_copy(ui), theme::casing(ui));
                    let _ =
                        contour_gizmo::ContourGizmo::default().paint(ui, screen.shrink(24.0), camera, &project, &edit);
                });
            }
            let pixels_per_point = ctx.pixels_per_point();
            let size = [
                (OVERLAY_SHOT.x * pixels_per_point).round().max(1.0) as u32,
                (OVERLAY_SHOT.y * pixels_per_point).round().max(1.0) as u32,
            ];
            let rgba = paint(&render_state, &jobs, size, pixels_per_point).expect("the overlay paints");
            let path =
                directory.join(format!("contour-overlay-{}-{board_name}.png", if dark { "dark" } else { "light" }));
            testkit::write_png(&path, size[0], size[1], &rgba).expect("the picture is written");
            let picture = Picture { width: size[0], height: size[1], rgba };
            assert!(!picture.is_flat(), "{} draws less than one colour", path.display());
            // Both inks of the pair reach the screen, and the casing under the working copy is
            // there to hold it off a board of its own colour.
            let (original, working, casing) = (picture.near(inks.0), picture.near(inks.1), picture.near(inks.2));
            assert!(
                original >= 100,
                "{}: the shape the edit started from reaches {original} pixels of its ink",
                path.display()
            );
            assert!(working >= 800, "{}: the working copy reaches {working} pixels", path.display());
            assert!(casing >= 300, "{}: the casing reaches {casing} pixels", path.display());
            // The board is still the ground of the picture: the overlay is a line, not a veil.
            let board_pixels = picture.near(board);
            assert!(
                board_pixels * 2 > picture.pixels(),
                "{}: the board covers {board_pixels} of {} pixels",
                path.display(),
                picture.pixels()
            );
            println!("{}: original {original}, working {working}, casing {casing}", path.display());
        }
    }
}

/// Draws the sketch overlay while a corner above the work plane captures the pointer.
///
/// The picture is the one the owner's report could not be answered from rectangles: the marker of a
/// snap and the end of the preview edge must be the same pixel, because the click stores exactly the
/// point the marker names (U119). The mode is the workspace's 2.5D reading, so the answer is the
/// candidate's foot on the drawing plane and the preview runs from the first point to that foot.
#[test]
#[ignore = "needs a GPU adapter; writes target/ui-shots/snap-marker.png"]
fn the_snap_marker_is_painted_on_the_end_of_the_preview() {
    let render_state = match render_state() {
        Ok(render_state) => render_state,
        Err(reason) => {
            println!("{reason}");
            return;
        }
    };
    let ctx = egui::Context::default();
    let workspace = testkit::scratch_dir("ui-shots").join("snap-workspace.json");
    // A block hanging above the drawing plane, so the two readings of a candidate are far apart on
    // screen: the picture is about that distance, and a floor panel's 18 mm is a few pixels.
    let mut project = testkit::sample_project();
    let root = project.root().id();
    let raised = model::Placement::new(geom::DVec3::new(420.0, -320.0, 320.0), model::Placement::IDENTITY.rotation())
        .expect("a place above the plane");
    project
        .add_object(root, "Raised", raised, model::Geometry::Mesh(testkit::cube_mesh(testkit::millimetres(120))))
        .expect("a block above the plane");
    let session = engine::Session::with_directory(project, testkit::sample_catalog());
    let mut app = App::test_app(&ctx, Some(render_state.clone()), workspace, Settings::default(), session)
        .expect("the application builds");
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, WINDOW);
    let _ = settle(&mut app, &ctx, &render_state, screen);

    // A corner of that block, above the drawing plane, and the plane point the first click of the
    // sketch takes. The pointer goes where the eye sees the corner.
    let (first, candidate) = {
        let document = app.tabs.active_mut();
        document.editor.start_drawing(ui::DrawTool::Line);
        let rect = document.scene_rect().expect("the settled frame gives the scene a rectangle");
        let camera = document.camera();
        let project = document.session.project();
        let sight = document.editor.flags().sight();
        let candidate = project
            .snap_marks(sight)
            .into_iter()
            .find(|mark| mark.point.z > 250.0)
            .expect("the sample scene has a corner above the plane");
        let first = crate::gizmo_math::screen_point(camera, rect, geom::DVec3::new(140.0, -320.0, 0.0))
            .expect("the work plane is on screen");
        let at = crate::gizmo_math::screen_point(camera, rect, candidate.point).expect("the corner is on screen");
        (first, at)
    };
    let click = |pointer: egui::Pos2, pressed: bool| egui::Event::PointerButton {
        pos: pointer,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let frames = vec![
        vec![egui::Event::PointerMoved(first), click(first, true), click(first, false)],
        vec![egui::Event::PointerMoved(candidate)],
        vec![egui::Event::PointerMoved(candidate)],
        vec![egui::Event::PointerMoved(candidate)],
    ];
    let mut jobs = Vec::new();
    for events in frames {
        let mut output = ctx.run_ui(egui::RawInput { screen_rect: Some(screen), events, ..Default::default() }, |ui| {
            app.frame(ui);
        });
        let mut delta = std::mem::take(&mut output.textures_delta);
        {
            let mut renderer = render_state.renderer.write();
            for (id, images) in &delta.set {
                for image in images {
                    renderer.update_texture(&render_state.device, &render_state.queue, *id, image);
                }
            }
            for id in &delta.free {
                renderer.free_texture(id);
            }
        }
        delta.clear();
        jobs = ctx.tessellate(output.shapes, output.pixels_per_point);
    }

    // The marker is drawn at the point the draft stores, so the overlay's own value answers both:
    // the place the click takes and the end of the preview edge the picture shows.
    {
        let document = app.tabs.active();
        let placed = document.draw_gizmo.hover_point().expect("the pointer places a point");
        let marker = document.draw_gizmo.hover_screen().expect("a marker at the point it stores");
        let rect = document.scene_rect().expect("a drawn frame gives the scene a rectangle");
        let stored = geom::DVec3::new(placed[0].millimetres_f64(), placed[1].millimetres_f64(), 0.0);
        let drawn = crate::gizmo_math::screen_point(document.camera(), rect, stored).expect("the point is on screen");
        assert!(
            (marker - drawn).length() < 0.5,
            "the marker stands on the point the click stores: {marker:?} against {drawn:?}"
        );
        assert_ne!(
            document.draw_gizmo.snap_kind(),
            Some(model::SnapKind::Grid),
            "a corner of the scene took the pointer"
        );
    }

    let pixels_per_point = ctx.pixels_per_point();
    let size =
        [(WINDOW.x * pixels_per_point).round().max(1.0) as u32, (WINDOW.y * pixels_per_point).round().max(1.0) as u32];
    let rgba = paint(&render_state, &jobs, size, pixels_per_point).expect("the frame paints");
    let directory = testkit::ui_shots_dir().expect("the shots directory is created");
    let path = directory.join("snap-marker.png");
    testkit::write_png(&path, size[0], size[1], &rgba).expect("the picture is written");
    app.tabs.active_mut().release_target();
    let picture = Picture { width: size[0], height: size[1], rgba };
    assert!(!picture.is_flat(), "{} draws less than one colour", path.display());
    println!("the snap marker is in {}", path.display());
}

/// Draws a 3D sketch started on a board's top face: the plane guide and the marker on that face.
///
/// The picture is the one the 3D mode's switch cannot be answered from rectangles: the plane a sketch
/// is stored on is the face under its first click, so the guide's cross and the marker stand on the
/// board and not on the floor, and the preview edge runs in that face (U121). The mode is the
/// workspace's 3D reading; a click on the board's top face starts the draft and the pointer then
/// moves to its other corner.
#[test]
#[ignore = "needs a GPU adapter; writes target/ui-shots/work-plane.png"]
fn the_sketch_on_a_face_plane_is_painted_offscreen() {
    let render_state = match render_state() {
        Ok(render_state) => render_state,
        Err(reason) => {
            println!("{reason}");
            return;
        }
    };
    let ctx = egui::Context::default();
    let workspace = testkit::scratch_dir("ui-shots").join("plane-workspace.json");
    // A block standing 320 mm above the floor: its top face is far enough from the world XY for the
    // picture to show which plane the sketch took.
    let mut project = testkit::sample_project();
    let root = project.root().id();
    let raised = model::Placement::new(geom::DVec3::new(420.0, -320.0, 320.0), model::Placement::IDENTITY.rotation())
        .expect("a place above the floor");
    project
        .add_object(root, "Raised", raised, model::Geometry::Mesh(testkit::cube_mesh(testkit::millimetres(120))))
        .expect("a block above the floor");
    let session = engine::Session::with_directory(project, testkit::sample_catalog());
    let mut settings = Settings::default();
    settings.snaps.mode = model::SnapMode::Space;
    let mut app =
        App::test_app(&ctx, Some(render_state.clone()), workspace, settings, session).expect("the application builds");
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, WINDOW);
    let _ = settle(&mut app, &ctx, &render_state, screen);

    // The middle of the block's top face, its corners 60 mm either way, and the pointer beside one of
    // them so the sketch's own corner captures it in the 3D reading.
    let top = geom::DVec3::new(420.0, -320.0, 380.0);
    let (first, beside) = {
        let document = app.tabs.active_mut();
        document.editor.start_drawing(ui::DrawTool::Line);
        let rect = document.scene_rect().expect("the settled frame gives the scene a rectangle");
        let camera = document.camera();
        let first = crate::gizmo_math::screen_point(camera, rect, top).expect("the face is on screen");
        (first, first + egui::vec2(24.0, 0.0))
    };
    let click = |pointer: egui::Pos2, pressed: bool| egui::Event::PointerButton {
        pos: pointer,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let frames = vec![
        vec![egui::Event::PointerMoved(first)],
        vec![egui::Event::PointerMoved(first), click(first, true), click(first, false)],
        vec![egui::Event::PointerMoved(beside)],
        vec![egui::Event::PointerMoved(beside)],
    ];
    let mut jobs = Vec::new();
    for events in frames {
        let mut output = ctx.run_ui(egui::RawInput { screen_rect: Some(screen), events, ..Default::default() }, |ui| {
            app.frame(ui);
        });
        let mut delta = std::mem::take(&mut output.textures_delta);
        {
            let mut renderer = render_state.renderer.write();
            for (id, images) in &delta.set {
                for image in images {
                    renderer.update_texture(&render_state.device, &render_state.queue, *id, image);
                }
            }
            for id in &delta.free {
                renderer.free_texture(id);
            }
        }
        delta.clear();
        jobs = ctx.tessellate(output.shapes, output.pixels_per_point);
    }

    // The click on the face froze that face's plane, and the pointer beside the first point still
    // stores a point of it: the plane is not the floor, and the draft is not the camera's.
    {
        let document = app.tabs.active();
        let rect = document.scene_rect().expect("a drawn frame gives the scene a rectangle");
        let camera = document.camera();
        let plane = document.draw_gizmo.plane(camera);
        assert!(
            (plane.normal() - geom::DVec3::Z).length() < 1.0e-6,
            "the plane of the block's top face is horizontal: {:?}",
            plane.normal()
        );
        let plane_local = document.draw_gizmo.hover_point().expect("the pointer stores a point of the plane");
        let stored = plane.world(geom::DVec2::new(plane_local[0].millimetres_f64(), plane_local[1].millimetres_f64()));
        assert!((stored.z - top.z).abs() < 0.5, "the sketch is stored on the face, not on the floor: {stored:?}");
        let marker = document.draw_gizmo.hover_screen().expect("a marker at the point it stores");
        let drawn = crate::gizmo_math::screen_point(camera, rect, stored).expect("the point is on screen");
        assert!(
            (marker - drawn).length() < 0.5,
            "the marker stands on the point the click stores: {marker:?} against {drawn:?}"
        );
    }

    let pixels_per_point = ctx.pixels_per_point();
    let size =
        [(WINDOW.x * pixels_per_point).round().max(1.0) as u32, (WINDOW.y * pixels_per_point).round().max(1.0) as u32];
    let rgba = paint(&render_state, &jobs, size, pixels_per_point).expect("the frame paints");
    let directory = testkit::ui_shots_dir().expect("the shots directory is created");
    let path = directory.join("work-plane.png");
    testkit::write_png(&path, size[0], size[1], &rgba).expect("the picture is written");
    app.tabs.active_mut().release_target();
    let picture = Picture { width: size[0], height: size[1], rgba };
    assert!(!picture.is_flat(), "{} draws less than one colour", path.display());
    println!("the sketch on the face plane is in {}", path.display());
}

/// Draws the product card in both theme variants: its number column and its folded class tree.
///
/// The picture answers the two questions a rectangle cannot: whether the three fields start on one
/// x with their unit outside the box, and whether a fold reads as the scene tree's fold rather than
/// as an indent. The card is the numeric path into `product.create` (ADR 0100), so a picture of it
/// is read before the fields it is about to grow are added.
#[test]
#[ignore = "needs a GPU adapter; writes target/ui-shots/product-card-{dark,light}.png"]
fn the_product_card_is_painted_offscreen() {
    let render_state = match render_state() {
        Ok(render_state) => render_state,
        Err(reason) => {
            println!("{reason}");
            return;
        }
    };
    let directory = testkit::ui_shots_dir().expect("the shots directory is created");
    for variant in VARIANTS {
        let ctx = egui::Context::default();
        let workspace = testkit::scratch_dir("ui-shots").join("product-card-workspace.json");
        let session = engine::Session::with_directory(testkit::sample_project(), testkit::sample_catalog());
        let mut settings = Settings::default();
        settings.appearance.theme = variant;
        let mut app = App::test_app(&ctx, Some(render_state.clone()), workspace, settings, session)
            .expect("the application builds");
        // The picture is the settled card, not a frame of its fade-in: an `Area` fades over the
        // style's animation time, which the shot's rapid frames never reach.
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, WINDOW);
        let _ = settle(&mut app, &ctx, &render_state, screen);
        app.product_card.start();
        let jobs = settle(&mut app, &ctx, &render_state, screen);
        assert!(app.product_card.is_open(), "the card is up");
        let pixels_per_point = ctx.pixels_per_point();
        let size = [
            (WINDOW.x * pixels_per_point).round().max(1.0) as u32,
            (WINDOW.y * pixels_per_point).round().max(1.0) as u32,
        ];
        let rgba = paint(&render_state, &jobs, size, pixels_per_point).expect("the card paints");
        let name = match variant {
            ThemeChoice::Light => "product-card-light.png",
            ThemeChoice::Dark | ThemeChoice::System => "product-card-dark.png",
        };
        let path = directory.join(name);
        testkit::write_png(&path, size[0], size[1], &rgba).expect("the picture is written");
        app.tabs.active_mut().release_target();
        let picture = Picture { width: size[0], height: size[1], rgba };
        assert!(!picture.is_flat(), "{} draws less than one colour", path.display());
        println!("the product card is in {}", path.display());
    }
}

/// Draws a cone's base round and its pull card into one picture.
///
/// The cone cell draws the base circle and hands the pointer to the pull, whose card carries the
/// height `H` beside the top radius `R` (ADR 0141). The picture answers two questions a rectangle
/// cannot: whether the cone cell's glyph reads beside the box, cylinder and sphere in the *Model*
/// row, and whether two lettered fields fit one plate at the hand.
#[test]
#[ignore = "needs a GPU adapter; writes target/ui-shots/cone-pull.png"]
fn the_cone_pull_is_painted_offscreen() {
    let render_state = match render_state() {
        Ok(render_state) => render_state,
        Err(reason) => {
            println!("{reason}");
            return;
        }
    };
    let ctx = egui::Context::default();
    let workspace = testkit::scratch_dir("ui-shots").join("cone-workspace.json");
    let session = engine::Session::with_directory(testkit::sample_project(), testkit::sample_catalog());
    let mut app = App::test_app(&ctx, Some(render_state.clone()), workspace, Settings::default(), session)
        .expect("the application builds");
    // The picture must show the Model tab the cone cell lives on, not the tab a fresh workspace opens.
    if let Some(tab) = ui::toolbars::spec::tab("model") {
        app.tabs.active_mut().editor.set_ribbon_tab(tab);
    }
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, WINDOW);
    let _ = settle(&mut app, &ctx, &render_state, screen);

    // A base circle drawn by its centre and a rim point, which the cell commits as a sharp cone at
    // the ribbon's height and hands to the pull.
    let mm = testkit::millimetres;
    let object = {
        let document = app.tabs.active_mut();
        document.editor.start_primitive_drawing(ui::DrawTool::Cone).expect("the cone cell draws a base round");
        for point in [[0, 0], [250, 0]] {
            document.editor.drawing_point([mm(point[0]), mm(point[1])]).expect("a point of the base circle");
        }
        document.commit_drawing(text::Lang::En).expect("the cone's base commits");
        document.editor.selected_node().expect("the new cone is selected")
    };
    let pointer = screen.center();
    for _ in 0..2 {
        let _ = run_frame(&mut app, &ctx, &render_state, screen, vec![egui::Event::PointerMoved(pointer)]);
    }
    {
        let document = app.tabs.active_mut();
        document.preview_extrude(object, mm(400), Some(mm(90))).expect("the pulled cone previews");
        assert!(document.tool_card_is_up(), "the pull draws its card");
    }
    let jobs = run_frame(&mut app, &ctx, &render_state, screen, vec![egui::Event::PointerMoved(pointer)]);
    let pixels_per_point = ctx.pixels_per_point();
    let size =
        [(WINDOW.x * pixels_per_point).round().max(1.0) as u32, (WINDOW.y * pixels_per_point).round().max(1.0) as u32];
    let rgba = paint(&render_state, &jobs, size, pixels_per_point).expect("the frame paints");
    let directory = testkit::ui_shots_dir().expect("the shots directory is created");
    let path = directory.join("cone-pull.png");
    testkit::write_png(&path, size[0], size[1], &rgba).expect("the picture is written");
    app.tabs.active_mut().release_target();
    let picture = Picture { width: size[0], height: size[1], rgba };
    assert!(!picture.is_flat(), "{} draws less than one colour", path.display());
    println!("the cone's pull is in {}", path.display());
}

/// Draws a selected product frame with its fourteen grips into one picture.
///
/// A frame owns the same box editor as a solid (ADR 0101), with two differences the pixels have to
/// show: all eight corners are drawn, because the reserved box is tall, and the two top corners sit
/// on the reserved height rather than on the floor. The frame is created through `product.create`,
/// so the shot proves the grips read the envelope and not a primitive.
#[test]
#[ignore = "needs a GPU adapter; writes target/ui-shots/frame-grips.png"]
fn the_frame_grips_are_painted_offscreen() {
    let render_state = match render_state() {
        Ok(state) => state,
        Err(reason) => {
            println!("{reason}");
            return;
        }
    };
    let ctx = egui::Context::default();
    let workspace = testkit::scratch_dir("ui-shots").join("frame-grips-workspace.json");
    // An empty document, so the picture is the reserved box and nothing else.
    let session = crate::scene_build::empty_session().expect("an empty session");
    let mut app = App::test_app(&ctx, Some(render_state.clone()), workspace, Settings::default(), session)
        .expect("the application builds");
    // The fill cells live on the Furniture tab, and the picture must show the armed cell, not the
    // tab a fresh workspace happens to open.
    if let Some(tab) = ui::toolbars::spec::tab("furniture") {
        app.tabs.active_mut().editor.set_ribbon_tab(tab);
    }
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, WINDOW);
    let _ = settle(&mut app, &ctx, &render_state, screen);
    {
        let document = app.tabs.active_mut();
        let root = document.session.project().root().id();
        let params = serde_json::to_value(engine::ProductCreate {
            parent: root,
            name: "Cabinet".to_owned(),
            class: Some(model::ClassCode::GENERAL_PRODUCT),
            envelope: model::Envelope::new(
                testkit::millimetres(900),
                testkit::millimetres(560),
                testkit::millimetres(1200),
            )
            .expect("a frame"),
            placement: model::Placement::IDENTITY,
        })
        .expect("params");
        let frame = document
            .session
            .dispatch(engine::product_cmds::PRODUCT_CREATE_ID, &params)
            .expect("the frame is created")
            .created_id()
            .expect("an id");
        // The frame stays open, so its envelope is drawn in the active ink: a closed frame with no
        // parts draws a dimmer box, and a shot of dim lines is no eye check (ADR 0100).
        document.session.open_product(frame);
        document.editor.select_node(Some(frame));
    }
    // The bounds follow a rebuilt scene, so the fit waits for one frame before it fits the box.
    let _ = settle(&mut app, &ctx, &render_state, screen);
    {
        // Frame the box the grips belong to: a two-metre frame would otherwise stand with its top
        // corners past the camera that was fitted to the empty scene.
        let document = app.tabs.active_mut();
        document.frame_all();
    }
    let mut jobs = Vec::new();
    for _ in 0..SETTLE_FRAMES {
        let mut output = ctx.run_ui(egui::RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| {
            app.frame(ui);
        });
        let mut delta = std::mem::take(&mut output.textures_delta);
        {
            let mut renderer = render_state.renderer.write();
            for (id, images) in &delta.set {
                for image in images {
                    renderer.update_texture(&render_state.device, &render_state.queue, *id, image);
                }
            }
            for id in &delta.free {
                renderer.free_texture(id);
            }
        }
        delta.clear();
        jobs = ctx.tessellate(output.shapes, output.pixels_per_point);
    }
    {
        // The grips the picture shows are the frame's, not a solid's: eight corners and six faces.
        let document = app.tabs.active_mut();
        let selected = document.editor.selected_nodes();
        let [frame] = selected else { panic!("the frame is selected") };
        let body = crate::box_gizmo::BoxBody::read(document.session.project(), *frame)
            .expect("a selected frame is a box the grips can read");
        assert_eq!(body.grips().len(), 14, "a frame shows eight corners and six faces");
    }
    let pixels_per_point = ctx.pixels_per_point();
    let size =
        [(WINDOW.x * pixels_per_point).round().max(1.0) as u32, (WINDOW.y * pixels_per_point).round().max(1.0) as u32];
    let rgba = paint(&render_state, &jobs, size, pixels_per_point).expect("the frame paints");
    let directory = testkit::ui_shots_dir().expect("the shots directory is created");
    let path = directory.join("frame-grips.png");
    testkit::write_png(&path, size[0], size[1], &rgba).expect("the picture is written");
    app.tabs.active_mut().release_target();
    let picture = Picture { width: size[0], height: size[1], rgba };
    assert!(!picture.is_flat(), "{} draws less than one colour", path.display());
    println!("the frame grips are in {}", path.display());
}

/// Draws one selected sample panel with its transform gizmo and returns the picture.
///
/// The two sizes are drawn through the real shell, so the shot answers what the setting does to the
/// handles and to the rings, not only what `GizmoSettings` holds (U86).
fn gizmo_picture(
    render_state: &egui_wgpu::RenderState,
    mode: crate::TransformMode,
    handle_points: u32,
    ring_percent: u32,
    path: &Path,
) -> Result<Picture, String> {
    let ctx = egui::Context::default();
    let workspace = testkit::scratch_dir("ui-shots").join("gizmo-workspace.json");
    let session = engine::Session::with_directory(testkit::sample_project(), testkit::sample_catalog());
    let mut settings = Settings::default();
    settings.gizmo.handle_points = handle_points;
    settings.gizmo.ring_percent = ring_percent;
    let mut app =
        App::test_app(&ctx, Some(render_state.clone()), workspace, settings, session).expect("the application builds");
    app.shell.mode = mode;
    // The sample shelf is the one node the picture selects; its pivot is where the gizmo stands.
    let node = model::Id::new(2).ok_or_else(|| "the sample shelf has no id".to_owned())?;
    app.tabs.active_mut().editor.select_node(Some(node));

    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, WINDOW);
    let jobs = settle(&mut app, &ctx, render_state, screen);
    let pixels_per_point = ctx.pixels_per_point();
    let size =
        [(WINDOW.x * pixels_per_point).round().max(1.0) as u32, (WINDOW.y * pixels_per_point).round().max(1.0) as u32];
    let rgba = paint(render_state, &jobs, size, pixels_per_point)?;
    app.tabs.active_mut().release_target();
    testkit::write_png(path, size[0], size[1], &rgba).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(Picture { width: size[0], height: size[1], rgba })
}

/// Draws the smallest and the largest transform gizmo into four pictures.
///
/// The handles are shot in Move mode and the rings in Rotate mode, at both ends of each range, so a
/// session can compare the gizmo the default draws with the one the sliders can reach (U86).
#[test]
#[ignore = "needs a GPU adapter; writes target/ui-shots/gizmo-*.png"]
fn the_gizmo_sizes_are_painted_offscreen() {
    let render_state = match render_state() {
        Ok(render_state) => render_state,
        Err(reason) => {
            println!("{reason}");
            return;
        }
    };
    let directory = testkit::ui_shots_dir().expect("the shots directory is created");
    let small = gizmo_picture(
        &render_state,
        crate::TransformMode::Move,
        MIN_HANDLE_POINTS,
        MIN_RING_PERCENT,
        &directory.join("gizmo-handles-small.png"),
    )
    .expect("the small handles render");
    let large = gizmo_picture(
        &render_state,
        crate::TransformMode::Move,
        MAX_HANDLE_POINTS,
        MIN_RING_PERCENT,
        &directory.join("gizmo-handles-large.png"),
    )
    .expect("the large handles render");
    assert!(!small.is_flat() && !large.is_flat(), "both pictures draw more than one colour");
    assert!(
        large.differing_pixels(&small) > 500,
        "the handle length reaches the pixels: {} differ",
        large.differing_pixels(&small)
    );

    let small_ring = gizmo_picture(
        &render_state,
        crate::TransformMode::Rotate,
        ui::settings::DEFAULT_HANDLE_POINTS,
        MIN_RING_PERCENT,
        &directory.join("gizmo-rings-small.png"),
    )
    .expect("the small rings render");
    let large_ring = gizmo_picture(
        &render_state,
        crate::TransformMode::Rotate,
        ui::settings::DEFAULT_HANDLE_POINTS,
        MAX_RING_PERCENT,
        &directory.join("gizmo-rings-large.png"),
    )
    .expect("the large rings render");
    assert!(
        large_ring.differing_pixels(&small_ring) > 500,
        "the ring radius reaches the pixels: {} differ",
        large_ring.differing_pixels(&small_ring)
    );
    println!("the small and the large gizmo are in {}", directory.display());
}

/// A root and one empty product of `600 x 560 x 720` millimetres, the void a fill needs.
///
/// The sample project carries a shelf and a drawer at the origin, which stand inside a product
/// created there; the fill rightly refuses a void holding them, so the shot uses a product of its
/// own instead of the shared sample.
fn empty_product() -> (model::Project, model::Id) {
    use model::{ClassCode, Container, Envelope, Id, Placement, Project};

    let root = Container::new(Id::new(1).expect("an id"), "Root", Placement::IDENTITY, Vec::new());
    let mut project = Project::new(root).expect("an empty project");
    let envelope = Envelope::new(testkit::millimetres(600), testkit::millimetres(560), testkit::millimetres(720))
        .expect("a valid box");
    let product = project
        .add_product(project.root().id(), "Cabinet", ClassCode::GENERAL_PRODUCT, envelope, Placement::IDENTITY)
        .expect("a product");
    (project, product)
}

/// Draws the empty-space tool into two pictures, so its ghost and its menu can be looked at.
///
/// The matrix above varies the window's look; this shot varies the tool. It arms the shelf fill
/// inside an open product, moves the pointer onto the piece and renders the real frame offscreen, so
/// the ghost's wireframe, its three sizes and the two gap arrows with their fields are checked as
/// pixels and not only as shapes. A right click then opens the operation's own menu — the class, the
/// build and the spread, each with its tick in a column left of the label (U128) — and the second
/// picture is that frame.
#[test]
#[ignore = "needs a GPU adapter; writes target/ui-shots/fill-ghost.png and fill-menu.png"]
fn the_empty_space_ghost_is_painted_offscreen() {
    let render_state = match render_state() {
        Ok(render_state) => render_state,
        Err(reason) => {
            println!("{reason}");
            return;
        }
    };
    let ctx = egui::Context::default();
    let workspace = testkit::scratch_dir("ui-shots").join("fill-workspace.json");
    let (project, product) = empty_product();
    let mut session = engine::Session::with_directory(project, testkit::sample_catalog());
    assert!(session.open_product(product), "the fixture product opens");
    let mut app = App::test_app(&ctx, Some(render_state.clone()), workspace, Settings::default(), session)
        .expect("the application builds");
    // The fill cells live on the Furniture tab, and the picture must show the armed cell, not the
    // tab a fresh workspace happens to open.
    if let Some(tab) = ui::toolbars::spec::tab("furniture") {
        app.tabs.active_mut().editor.set_ribbon_tab(tab);
    }
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, WINDOW);
    let _ = settle(&mut app, &ctx, &render_state, screen);

    let pointer = {
        let document = app.tabs.active_mut();
        document
            .editor
            .arm_fill(&document.session, model::ClassCode::HORIZONTAL_PANEL, model::FillMode::Shape, text::Lang::En)
            .expect("the shelf fill arms inside the open product");
        let project = document.session.project();
        let envelope = project.container(product).and_then(|product| product.envelope()).expect("an envelope");
        let size = geom::DVec3::new(
            envelope.x().millimetres_f64(),
            envelope.y().millimetres_f64(),
            envelope.z().millimetres_f64(),
        );
        let placement = project.world_placement(product).expect("a frame");
        let centre = placement.transform_point(size * 0.5);
        let rect = document.scene_rect().expect("the settled frame gives the scene a rectangle");
        crate::gizmo_math::screen_point(document.camera(), rect, centre).expect("the piece is on screen")
    };

    let mut jobs = Vec::new();
    for frame in 0..SETTLE_FRAMES {
        let events = if frame == 0 { vec![egui::Event::PointerMoved(pointer)] } else { Vec::new() };
        let mut output = ctx.run_ui(egui::RawInput { screen_rect: Some(screen), events, ..Default::default() }, |ui| {
            app.frame(ui);
        });
        let mut delta = std::mem::take(&mut output.textures_delta);
        {
            let mut renderer = render_state.renderer.write();
            for (id, images) in &delta.set {
                for image in images {
                    renderer.update_texture(&render_state.device, &render_state.queue, *id, image);
                }
            }
            for id in &delta.free {
                renderer.free_texture(id);
            }
        }
        delta.clear();
        jobs = ctx.tessellate(output.shapes, output.pixels_per_point);
    }
    assert!(app.tabs.active().fill_gizmo.ghost().is_some(), "the pointer measured a ghost to draw");
    let pixels_per_point = ctx.pixels_per_point();
    let size =
        [(WINDOW.x * pixels_per_point).round().max(1.0) as u32, (WINDOW.y * pixels_per_point).round().max(1.0) as u32];
    let rgba = paint(&render_state, &jobs, size, pixels_per_point).expect("the frame paints");
    let directory = testkit::ui_shots_dir().expect("the shots directory is created");
    let path = directory.join("fill-ghost.png");
    testkit::write_png(&path, size[0], size[1], &rgba).expect("the picture is written");
    let picture = Picture { width: size[0], height: size[1], rgba };
    assert!(!picture.is_flat(), "{} draws less than one colour", path.display());
    println!("the fill ghost is in {}", path.display());

    // The operation's own menu opens on a right click, so the second picture is the same frame with
    // the choices in front of it.
    let button = |pressed| egui::Event::PointerButton {
        pos: pointer,
        button: egui::PointerButton::Secondary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    for frame in 0..SETTLE_FRAMES {
        let events = if frame == 0 { vec![button(true), button(false)] } else { Vec::new() };
        let mut output = ctx.run_ui(egui::RawInput { screen_rect: Some(screen), events, ..Default::default() }, |ui| {
            app.frame(ui);
        });
        let mut delta = std::mem::take(&mut output.textures_delta);
        {
            let mut renderer = render_state.renderer.write();
            for (id, images) in &delta.set {
                for image in images {
                    renderer.update_texture(&render_state.device, &render_state.queue, *id, image);
                }
            }
            for id in &delta.free {
                renderer.free_texture(id);
            }
        }
        delta.clear();
        jobs = ctx.tessellate(output.shapes, output.pixels_per_point);
    }
    assert!(app.tabs.active().menu_rect().is_some(), "the right click opened the operation's menu");
    let rgba = paint(&render_state, &jobs, size, pixels_per_point).expect("the frame paints");
    let menu = directory.join("fill-menu.png");
    testkit::write_png(&menu, size[0], size[1], &rgba).expect("the picture is written");
    app.tabs.active_mut().release_target();
    let picture = Picture { width: size[0], height: size[1], rgba };
    assert!(!picture.is_flat(), "{} draws less than one colour", menu.display());
    println!("the fill's menu is in {}", menu.display());
}

/// Draws the live row of an evenly fill, so its spread and its plates can be looked at.
///
/// «Равномерно» needs no anchoring click (U128): the pointer names a void and the boards are already
/// spread through it. The shot presses the count's plate, types four and settles, so the picture is
/// the row a maker would see while deciding — the arrows, the two numbers and the count beside the
/// hand.
#[test]
#[ignore = "needs a GPU adapter; writes target/ui-shots/fill-row.png"]
fn the_live_row_is_painted_offscreen() {
    let render_state = match render_state() {
        Ok(render_state) => render_state,
        Err(reason) => {
            println!("{reason}");
            return;
        }
    };
    let ctx = egui::Context::default();
    let workspace = testkit::scratch_dir("ui-shots").join("fill-row-workspace.json");
    let (project, product) = empty_product();
    let mut session = engine::Session::with_directory(project, testkit::sample_catalog());
    assert!(session.open_product(product), "the fixture product opens");
    let mut app = App::test_app(&ctx, Some(render_state.clone()), workspace, Settings::default(), session)
        .expect("the application builds");
    if let Some(tab) = ui::toolbars::spec::tab("furniture") {
        app.tabs.active_mut().editor.set_ribbon_tab(tab);
    }
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, WINDOW);
    let _ = settle(&mut app, &ctx, &render_state, screen);

    let pointer = {
        let document = app.tabs.active_mut();
        document.editor.set_panel_build(ui::PanelBuild {
            class: model::ClassCode::HORIZONTAL_PANEL,
            spread: ui::FillSpread::Even,
            ..ui::PanelBuild::default()
        });
        document
            .editor
            .arm_fill(&document.session, model::ClassCode::HORIZONTAL_PANEL, model::FillMode::Fit, text::Lang::En)
            .expect("the shelf fill arms inside the open product");
        let project = document.session.project();
        let envelope = project.container(product).and_then(|product| product.envelope()).expect("an envelope");
        let size = geom::DVec3::new(
            envelope.x().millimetres_f64(),
            envelope.y().millimetres_f64(),
            envelope.z().millimetres_f64(),
        );
        let placement = project.world_placement(product).expect("a frame");
        let centre = placement.transform_point(size * 0.5);
        let rect = document.scene_rect().expect("the settled frame gives the scene a rectangle");
        crate::gizmo_math::screen_point(document.camera(), rect, centre).expect("the piece is on screen")
    };

    let mut jobs = Vec::new();
    let run = |app: &mut App, events: Vec<egui::Event>, jobs: &mut Vec<egui::ClippedPrimitive>| {
        let mut output = ctx.run_ui(egui::RawInput { screen_rect: Some(screen), events, ..Default::default() }, |ui| {
            app.frame(ui);
        });
        let mut delta = std::mem::take(&mut output.textures_delta);
        {
            let mut renderer = render_state.renderer.write();
            for (id, images) in &delta.set {
                for image in images {
                    renderer.update_texture(&render_state.device, &render_state.queue, *id, image);
                }
            }
            for id in &delta.free {
                renderer.free_texture(id);
            }
        }
        delta.clear();
        *jobs = ctx.tessellate(output.shapes, output.pixels_per_point);
    };
    for frame in 0..SETTLE_FRAMES {
        let events = if frame == 0 { vec![egui::Event::PointerMoved(pointer)] } else { Vec::new() };
        run(&mut app, events, &mut jobs);
    }
    // The count is typed in the inspector's row; the shot writes it through the same path, so the
    // picture is the row a maker reads while deciding — four boards with the same air at both ends.
    app.tabs.active_mut().apply_tool_outcome(
        &ctx,
        ui::ToolOutcome {
            edits: vec![ui::ToolEdit { param: ui::ToolParam::Count, value: 4.0 }],
            ..ui::ToolOutcome::default()
        },
    );
    for _ in 0..SETTLE_FRAMES {
        run(&mut app, Vec::new(), &mut jobs);
    }
    assert!(app.tabs.active().fill_gizmo.row().is_some(), "the pointer spread a live row");

    let pixels_per_point = ctx.pixels_per_point();
    let size =
        [(WINDOW.x * pixels_per_point).round().max(1.0) as u32, (WINDOW.y * pixels_per_point).round().max(1.0) as u32];
    let rgba = paint(&render_state, &jobs, size, pixels_per_point).expect("the frame paints");
    let directory = testkit::ui_shots_dir().expect("the shots directory is created");
    let path = directory.join("fill-row.png");
    testkit::write_png(&path, size[0], size[1], &rgba).expect("the picture is written");
    app.tabs.active_mut().release_target();
    let picture = Picture { width: size[0], height: size[1], rgba };
    assert!(!picture.is_flat(), "{} draws less than one colour", path.display());
    println!("the live row is in {}", path.display());
}

/// A scene whose tree is taller than a panel and whose inspector card is the longest one.
///
/// Forty components make the tree scroll to a last row (the demo scene holds six), and the last
/// component holds a panel, so the inspector draws its placement, pivot, panel and property cards —
/// the body whose last row the horizontal bar used to cross (X39).
fn deep_session() -> Result<(engine::Session, model::Id), String> {
    use model::{Container, Id, PanelNominalSize, Placement, Project};

    let root_id = Id::new(1).ok_or("id 1 is the root")?;
    let root = Container::new(root_id, "Scene", Placement::IDENTITY, Vec::new());
    let mut project = Project::new(root).map_err(|error| error.to_string())?;
    let mut last = root_id;
    for index in 0..40 {
        last = project
            .add_component(root_id, format!("Cabinet {index:02}"), Placement::IDENTITY)
            .map_err(|error| error.to_string())?;
    }
    let millimetres = |value: i64| units::Length::from_millimetres(value).ok_or("a length");
    let size = PanelNominalSize::new(millimetres(600)?, millimetres(400)?).map_err(|error| error.to_string())?;
    let panel = project
        .add_panel(last, "Shelf", size, None, millimetres(18)?, Placement::IDENTITY)
        .map_err(|error| error.to_string())?;
    project.validate().map_err(|error| error.to_string())?;
    let session = engine::Session::with_directory(project, crate::scene_build::default_catalog()?);
    Ok((session, panel))
}

/// Draws both side panels scrolled to their last row in both themes, the way X39 hid a row.
///
/// The regression was pixels, not rectangles: the floating bar was drawn over the last row of the
/// inspector and the tree scrolled by its own route. The shot turns the wheel over each panel until
/// its body reaches the end, so the picture shows the row the bar would cover in either theme.
#[test]
#[ignore = "needs a GPU adapter; writes target/ui-shots/side-panels-{dark,light}.png"]
fn the_panels_last_row_is_painted_clear_of_the_bar() {
    let render_state = match render_state() {
        Ok(render_state) => render_state,
        Err(reason) => {
            println!("{reason}");
            return;
        }
    };
    let directory = testkit::ui_shots_dir().expect("the shots directory is created");
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, WINDOW);
    for variant in VARIANTS {
        let ctx = egui::Context::default();
        let workspace = testkit::scratch_dir("ui-shots").join("side-panels-workspace.json");
        let (session, panel) = deep_session().expect("the deep scene builds");
        let mut settings = Settings::default();
        settings.appearance.theme = variant;
        let mut app = App::test_app(&ctx, Some(render_state.clone()), workspace, settings, session)
            .expect("the application builds");
        app.tabs.active_mut().editor.select_node(Some(panel));
        let mut jobs = settle(&mut app, &ctx, &render_state, screen);

        // Every frame turns the wheel over one panel, alternating, so both bodies reach their end
        // before the picture is taken.
        let mut chrome = None;
        for step in 0..24 {
            let target = chrome.as_ref().and_then(
                |chrome: &crate::side_panels::Chrome| {
                    if step % 2 == 0 { chrome.panels.left } else { chrome.panels.right }
                },
            );
            let events = target.map(|rect| {
                vec![
                    egui::Event::PointerMoved(rect.center()),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0.0, -600.0),
                        phase: egui::TouchPhase::Move,
                        source: egui::MouseWheelSource::Wheel,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]
            });
            let mut output = ctx.run_ui(
                egui::RawInput { screen_rect: Some(screen), events: events.unwrap_or_default(), ..Default::default() },
                |ui| {
                    chrome = Some(app.frame(ui));
                },
            );
            let mut delta = std::mem::take(&mut output.textures_delta);
            {
                let mut renderer = render_state.renderer.write();
                for (id, images) in &delta.set {
                    for image in images {
                        renderer.update_texture(&render_state.device, &render_state.queue, *id, image);
                    }
                }
                for id in &delta.free {
                    renderer.free_texture(id);
                }
            }
            delta.clear();
            jobs = ctx.tessellate(output.shapes, output.pixels_per_point);
        }
        assert!(chrome.is_some(), "the frame drew both panels");

        let pixels_per_point = ctx.pixels_per_point();
        let size = [
            (WINDOW.x * pixels_per_point).round().max(1.0) as u32,
            (WINDOW.y * pixels_per_point).round().max(1.0) as u32,
        ];
        let rgba = paint(&render_state, &jobs, size, pixels_per_point).expect("the frame paints");
        let theme = match variant {
            ThemeChoice::Dark => "dark",
            ThemeChoice::Light => "light",
            ThemeChoice::System => "system",
        };
        let path = directory.join(format!("side-panels-{theme}.png"));
        testkit::write_png(&path, size[0], size[1], &rgba).expect("the picture is written");
        app.tabs.active_mut().release_target();
        let picture = Picture { width: size[0], height: size[1], rgba };
        assert!(!picture.is_flat(), "{} draws less than one colour", path.display());
        println!("the scrolled panels are in {}", path.display());
    }
}

/// Draws the work cursor and its crosshair into one picture per theme variant.
///
/// The square is the shell's own pointer (U88): whether its outline carries on a shaded board and
/// whether the crosshair is a guide rather than a form edge cannot be read from a rectangle, so this
/// is the eye check the task names. The files are `work-cursor-dark.png` and `work-cursor-light.png`.
#[test]
#[ignore = "needs a GPU adapter; writes target/ui-shots/work-cursor-*.png"]
fn the_work_cursor_is_painted_offscreen_in_both_themes() {
    let render_state = match render_state() {
        Ok(render_state) => render_state,
        Err(reason) => {
            println!("{reason}");
            return;
        }
    };
    let directory = testkit::ui_shots_dir().expect("the shots directory is created");
    for (name, variant) in [("dark", ThemeChoice::Dark), ("light", ThemeChoice::Light)] {
        let ctx = egui::Context::default();
        let workspace = testkit::scratch_dir("ui-shots").join(format!("cursor-{name}.json"));
        let mut settings = Settings::default();
        settings.appearance.theme = variant;
        settings.pointer.cursor_size = DEFAULT_CURSOR_POINTS;
        settings.pointer.crosshair = true;
        let session = crate::scene_build::demo_session().expect("the sample session builds");
        let mut app = App::test_app(&ctx, Some(render_state.clone()), workspace, settings, session)
            .expect("the application builds");
        app.tabs.active_mut().zoom_active_view(1.0 / MODEL_ZOOM);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, WINDOW);
        let _ = settle(&mut app, &ctx, &render_state, screen);
        // The pointer is inside the scene rectangle: the square no longer paints over a panel, a
        // toolbar or the corner widget, so the eye check has to look at the work area (X41).
        let pointer = app.tabs.active().scene_rect().expect("the settled frame gives the scene a rectangle").center();
        // The square is chrome over the scene, so the picture is taken once the pointer has been
        // reported and the frame that reads it has drawn.
        let mut jobs = Vec::new();
        for _ in 0..2 {
            let events = vec![egui::Event::PointerMoved(pointer)];
            let mut output =
                ctx.run_ui(egui::RawInput { screen_rect: Some(screen), events, ..Default::default() }, |ui| {
                    app.frame(ui);
                });
            let mut delta = std::mem::take(&mut output.textures_delta);
            {
                let mut renderer = render_state.renderer.write();
                for (id, images) in &delta.set {
                    for image in images {
                        renderer.update_texture(&render_state.device, &render_state.queue, *id, image);
                    }
                }
                for id in &delta.free {
                    renderer.free_texture(id);
                }
            }
            delta.clear();
            jobs = ctx.tessellate(output.shapes, output.pixels_per_point);
        }
        let drawn = app.shell.work_cursor.expect("the pointer over the scene draws the cursor");
        assert_eq!(drawn.square.center(), pointer, "the {name} frame draws the square on the pointer");
        assert!(drawn.crosshair.is_some(), "the {name} frame draws the crosshair the setting turned on");
        let pixels_per_point = ctx.pixels_per_point();
        let size = [
            (WINDOW.x * pixels_per_point).round().max(1.0) as u32,
            (WINDOW.y * pixels_per_point).round().max(1.0) as u32,
        ];
        let rgba = paint(&render_state, &jobs, size, pixels_per_point).expect("the frame paints");
        let path = directory.join(format!("work-cursor-{name}.png"));
        testkit::write_png(&path, size[0], size[1], &rgba).expect("the picture is written");
        app.tabs.active_mut().release_target();
        let picture = Picture { width: size[0], height: size[1], rgba };
        assert!(!picture.is_flat(), "{} draws less than one colour", path.display());
        println!("the {name} work cursor is in {}", path.display());
    }
}

/// Draws the inspector card of a panel, an object and a product frame in both theme variants.
///
/// The three cards are the inspector's only reading surface, and U93 rebuilt the object card as one
/// reading order (ADR 0116). Whether the head fits the panel's 226 pt column, whether the fields read
/// as one group and whether the action row carries in the light theme cannot be decided from
/// rectangles, so these six pictures are the eye check the task names: `card-<kind>-<variant>.png`.
/// The scene is left out: the question is the card's own order, not how it sits over the model,
/// which the full-window matrix already answers.
#[test]
#[ignore = "needs a GPU adapter; writes target/ui-shots/card-*.png"]
fn the_three_cards_are_painted_offscreen_in_both_themes() {
    let render_state = match render_state() {
        Ok(render_state) => render_state,
        Err(reason) => {
            println!("{reason}");
            return;
        }
    };
    let directory = testkit::ui_shots_dir().expect("the shots directory is created");
    let canvas = egui::Rect::from_min_size(egui::Pos2::ZERO, CARD_CANVAS);
    for (preference, name) in [(egui::ThemePreference::Dark, "dark"), (egui::ThemePreference::Light, "light")] {
        for (index, card) in ["panel", "object", "frame"].into_iter().enumerate() {
            let (session, selections) = card_session().expect("the card document builds");
            let mut session = session;
            let mut editor = ui::Editor::new();
            editor.select_node(Some(selections[index]));
            let ctx = egui::Context::default();
            ui::theme::install_with(&ctx, preference, ThemePreset::GraphiteCopper, None);
            ui::set_language(&ctx, ui::Lang::En);
            let mut jobs = Vec::new();
            for _ in 0..SETTLE_FRAMES {
                let mut output = ctx.run_ui(egui::RawInput { screen_rect: Some(canvas), ..Default::default() }, |ui| {
                    draw_card_column(ui, canvas, &mut editor, &mut session);
                });
                let mut delta = std::mem::take(&mut output.textures_delta);
                {
                    let mut renderer = render_state.renderer.write();
                    for (id, images) in &delta.set {
                        for image in images {
                            renderer.update_texture(&render_state.device, &render_state.queue, *id, image);
                        }
                    }
                    for id in &delta.free {
                        renderer.free_texture(id);
                    }
                }
                delta.clear();
                jobs = ctx.tessellate(output.shapes, output.pixels_per_point);
            }
            let pixels_per_point = ctx.pixels_per_point();
            let size = [
                (canvas.width() * pixels_per_point).round().max(1.0) as u32,
                (canvas.height() * pixels_per_point).round().max(1.0) as u32,
            ];
            let rgba = paint(&render_state, &jobs, size, pixels_per_point).expect("the card paints");
            let path = directory.join(format!("card-{card}-{name}.png"));
            testkit::write_png(&path, size[0], size[1], &rgba).expect("the picture is written");
            let picture = Picture { width: size[0], height: size[1], rgba };
            assert!(!picture.is_flat(), "{} draws less than one colour", path.display());
            println!("the {card} card is in {}", path.display());
        }
    }
}

/// The canvas a card picture is drawn on, in points.
const CARD_CANVAS: egui::Vec2 = egui::vec2(420.0, 1900.0);

/// The width of the inspector panel's column, in points (mirrors `side_panels::EXPANDED_WIDTH`).
const CARD_COLUMN: f32 = 226.0;

/// The gap between the canvas edge and the panel column, in points.
const CARD_MARGIN: f32 = 8.0;

/// Draws one inspector column on the canvas: the chrome behind it, then the card the selection owns.
fn draw_card_column(ui: &mut egui::Ui, canvas: egui::Rect, editor: &mut ui::Editor, session: &mut engine::Session) {
    let corner = egui::CornerRadius::same(ui::theme::RADIUS);
    ui.painter().rect_filled(canvas, corner, ui::theme::chrome(ui));
    let column = egui::Rect::from_min_size(
        canvas.min + egui::vec2(CARD_MARGIN, CARD_MARGIN),
        egui::vec2(CARD_COLUMN, canvas.height() - 2.0 * CARD_MARGIN),
    );
    ui.painter().rect_filled(column, corner, ui::theme::panel(ui));
    ui.scope_builder(egui::UiBuilder::new().max_rect(column.shrink(CARD_MARGIN)), |ui| {
        let _ = editor.show_card(ui, session, None);
    });
}

/// The document the three cards are shot from, and the node each of them is drawn for.
///
/// A panel, a primitive object and a product frame: the three kinds whose cards are different
/// surfaces rather than different fields of one card.
fn card_session() -> Result<(engine::Session, [model::Id; 3]), String> {
    use model::{Geometry, Placement, Primitive};
    let (mut project, frame) = testkit::sample_product_project();
    let root = project.root().id();
    let panel = project.root().children().first().map(model::Node::id).ok_or("no panel to select")?;
    let mm = testkit::millimetres;
    let sphere = Primitive::sphere(mm(400), 24, 12).map_err(|error| error.to_string())?;
    let object = project
        .add_object(root, "Ball", Placement::IDENTITY, Geometry::Primitive(sphere))
        .map_err(|error| error.to_string())?;
    Ok((engine::Session::with_directory(project, testkit::sample_catalog()), [panel, object, frame]))
}

/// Draws the rubber band mid-drag in both themes, for each of its two rules.
///
/// The band is translucent chrome over the model with a solid outline when it encloses and a dashed
/// one when it crosses (ADR 0137), and neither the fill's weight over a shaded board nor the
/// difference between the two outlines can be read from a rectangle. The four files are
/// `band-window-dark.png`, `band-crossing-dark.png` and the light pair.
/// The first node called `name` in the tree, so a shot can move a part without a click.
fn find_named(nodes: &[model::Node], name: &str) -> Option<model::Id> {
    for node in nodes {
        if node.name() == name {
            return Some(node.id());
        }
        if let Some(children) = node.children()
            && let Some(found) = find_named(children, name)
        {
            return Some(found);
        }
    }
    None
}

#[test]
#[ignore = "needs a GPU adapter; writes target/ui-shots/wireframe-ink.png"]
fn the_wireframe_on_the_dark_theme_draws_the_black_ink() {
    // U108 made the outline ink black on both variants; a wireframe has no face under its lines, so
    // this picture is the one that says how much of it survives on the dark background.
    let render_state = match render_state() {
        Ok(render_state) => render_state,
        Err(reason) => {
            println!("{reason}");
            return;
        }
    };
    let directory = testkit::ui_shots_dir().expect("the shots directory is created");
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, WINDOW);
    let ctx = egui::Context::default();
    let workspace = testkit::scratch_dir("ui-shots").join("wireframe-ink.json");
    let mut settings = Settings::default();
    settings.appearance.theme = ThemeChoice::Dark;
    settings.viewport.style = ui::display::ShadingStyle::Wireframe;
    let session = crate::scene_build::demo_session().expect("the sample session builds");
    let mut app =
        App::test_app(&ctx, Some(render_state.clone()), workspace, settings, session).expect("the application builds");
    let _ = settle(&mut app, &ctx, &render_state, screen);
    let jobs = run_frame(&mut app, &ctx, &render_state, screen, Vec::new());
    let pixels_per_point = ctx.pixels_per_point();
    let size =
        [(WINDOW.x * pixels_per_point).round().max(1.0) as u32, (WINDOW.y * pixels_per_point).round().max(1.0) as u32];
    let rgba = paint(&render_state, &jobs, size, pixels_per_point).expect("the frame paints");
    let path = directory.join("wireframe-ink.png");
    testkit::write_png(&path, size[0], size[1], &rgba).expect("the picture is written");
    app.tabs.active_mut().release_target();
    println!("the dark wireframe with the black ink is in {}", path.display());
}

#[test]
#[ignore = "needs a GPU adapter; writes target/ui-shots/floor-opacity-*.png"]
fn the_floor_at_full_and_at_forty_percent_shows_what_stands_under_it() {
    // U109: the plate is blended, so one scene under the same camera at 100 % and at 40 % is the pair
    // that says what the setting does — the shelf moved under the floor is hidden by the opaque
    // plate and seen through the tinted one, and the shadow beside it is drawn in both.
    let render_state = match render_state() {
        Ok(render_state) => render_state,
        Err(reason) => {
            println!("{reason}");
            return;
        }
    };
    let directory = testkit::ui_shots_dir().expect("the shots directory is created");
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, WINDOW);
    for opacity in [100, 40] {
        let ctx = egui::Context::default();
        let workspace = testkit::scratch_dir("ui-shots").join(format!("floor-opacity-{opacity}.json"));
        let mut settings = Settings::default();
        settings.appearance.theme = ThemeChoice::Dark;
        settings.viewport.shadow_strength = 65;
        settings.viewport.floor_opacity = opacity;
        let mut session = crate::scene_build::demo_session().expect("the sample session builds");
        let shelf =
            find_named(std::slice::from_ref(&model::Node::Container(session.project().root().clone())), "Loose shelf")
                .expect("the demo scene holds the loose shelf");
        let below =
            model::Placement::new(geom::DVec3::new(150.0, -500.0, -500.0), geom::DQuat::IDENTITY).expect("a placement");
        let params =
            serde_json::to_value(engine::NodePlacementSet { node: shelf, placement: below, unmeasured: false })
                .expect("params");
        session.dispatch("node.placement.set", &params).expect("the shelf moves under the floor");
        let mut app = App::test_app(&ctx, Some(render_state.clone()), workspace, settings, session)
            .expect("the application builds");
        let _ = settle(&mut app, &ctx, &render_state, screen);
        let jobs = run_frame(&mut app, &ctx, &render_state, screen, Vec::new());
        let pixels_per_point = ctx.pixels_per_point();
        let size = [
            (WINDOW.x * pixels_per_point).round().max(1.0) as u32,
            (WINDOW.y * pixels_per_point).round().max(1.0) as u32,
        ];
        let rgba = paint(&render_state, &jobs, size, pixels_per_point).expect("the frame paints");
        let path = directory.join(format!("floor-opacity-{opacity}.png"));
        testkit::write_png(&path, size[0], size[1], &rgba).expect("the picture is written");
        app.tabs.active_mut().release_target();
        println!("the floor at {opacity} % over a shelf below it is in {}", path.display());
    }
}

#[test]
#[ignore = "needs a GPU adapter; writes target/ui-shots/settings-view-*.png"]
fn the_settings_window_draws_the_floor_opacity_beside_the_three_strengths() {
    // U109: the floor's opacity is the fourth value in Settings › View, under one *Reset display*
    // with the three display strengths. The picture is the check that the row fits the page and
    // speaks both languages' words, which no assertion can see.
    let render_state = match render_state() {
        Ok(render_state) => render_state,
        Err(reason) => {
            println!("{reason}");
            return;
        }
    };
    let directory = testkit::ui_shots_dir().expect("the shots directory is created");
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, WINDOW);
    for (name, variant) in [("dark", ThemeChoice::Dark), ("light", ThemeChoice::Light)] {
        let ctx = egui::Context::default();
        let workspace = testkit::scratch_dir("ui-shots").join(format!("settings-view-{name}.json"));
        let mut settings = Settings::default();
        settings.appearance.theme = variant;
        settings.viewport.floor_opacity = 40;
        let session = crate::scene_build::demo_session().expect("the sample session builds");
        let mut app = App::test_app(&ctx, Some(render_state.clone()), workspace, settings, session)
            .expect("the application builds");
        app.settings_open = true;
        app.settings_section = ui::settings::SettingsSection::Viewport;
        let _ = settle(&mut app, &ctx, &render_state, screen);
        let jobs = run_frame(&mut app, &ctx, &render_state, screen, Vec::new());
        let pixels_per_point = ctx.pixels_per_point();
        let size = [
            (WINDOW.x * pixels_per_point).round().max(1.0) as u32,
            (WINDOW.y * pixels_per_point).round().max(1.0) as u32,
        ];
        let rgba = paint(&render_state, &jobs, size, pixels_per_point).expect("the frame paints");
        let path = directory.join(format!("settings-view-{name}.png"));
        testkit::write_png(&path, size[0], size[1], &rgba).expect("the picture is written");
        app.tabs.active_mut().release_target();
        println!("the {name} Settings › View page is in {}", path.display());
    }
}

#[test]
#[ignore = "needs a GPU adapter; writes target/ui-shots/display-strength-*.png"]
fn the_display_strengths_fade_the_grid_the_outlines_and_the_shadow() {
    // U64 shipped three display sliders and only one of them ever reached the frame; this pair is
    // the picture that says all three do — the grid, the form outlines and the ground shadow at the
    // workspace's own strengths and at zero.
    let render_state = match render_state() {
        Ok(render_state) => render_state,
        Err(reason) => {
            println!("{reason}");
            return;
        }
    };
    let directory = testkit::ui_shots_dir().expect("the shots directory is created");
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, WINDOW);
    for (name, grid, edges, shadows) in [("full", 100, 100, 65), ("none", 0, 0, 0)] {
        let ctx = egui::Context::default();
        let workspace = testkit::scratch_dir("ui-shots").join(format!("display-strength-{name}.json"));
        let mut settings = Settings::default();
        settings.appearance.theme = ThemeChoice::Dark;
        settings.viewport.grid_strength = grid;
        settings.viewport.edge_strength = edges;
        settings.viewport.shadow_strength = shadows;
        let session = crate::scene_build::demo_session().expect("the sample session builds");
        let mut app = App::test_app(&ctx, Some(render_state.clone()), workspace, settings, session)
            .expect("the application builds");
        // The fitted view, so the floor, its grid and the shadow under the model are all in frame.
        let _ = settle(&mut app, &ctx, &render_state, screen);
        let jobs = run_frame(&mut app, &ctx, &render_state, screen, Vec::new());
        let pixels_per_point = ctx.pixels_per_point();
        let size = [
            (WINDOW.x * pixels_per_point).round().max(1.0) as u32,
            (WINDOW.y * pixels_per_point).round().max(1.0) as u32,
        ];
        let rgba = paint(&render_state, &jobs, size, pixels_per_point).expect("the frame paints");
        let path = directory.join(format!("display-strength-{name}.png"));
        testkit::write_png(&path, size[0], size[1], &rgba).expect("the picture is written");
        app.tabs.active_mut().release_target();
        println!("the three display strengths at {name} are in {}", path.display());
    }
}

#[test]
#[ignore = "needs a GPU adapter; writes target/ui-shots/shadows-*.png"]
fn the_shadow_is_per_view_and_the_view_tab_carries_both_controls() {
    // U107: the View tab's Display group holds the shadow's switch and its one intensity, and a
    // split workspace darkens one cell's floor while the other keeps its own (ADR 0142).
    let render_state = match render_state() {
        Ok(render_state) => render_state,
        Err(reason) => {
            println!("{reason}");
            return;
        }
    };
    let directory = testkit::ui_shots_dir().expect("the shots directory is created");
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, WINDOW);
    // The 200 % cell is the one that shows whether the field and the glyph survive the scale.
    for (name, variant, scale) in
        [("dark", ThemeChoice::Dark, 100), ("light", ThemeChoice::Light, 100), ("dark-200", ThemeChoice::Dark, 200)]
    {
        let ctx = egui::Context::default();
        let workspace = testkit::scratch_dir("ui-shots").join(format!("shadows-{name}.json"));
        let mut settings = Settings::default();
        settings.appearance.theme = variant;
        settings.appearance.ui_scale = scale;
        let session = crate::scene_build::demo_session().expect("the sample session builds");
        let mut app = App::test_app(&ctx, Some(render_state.clone()), workspace, settings, session)
            .expect("the application builds");
        {
            let document = app.tabs.active_mut();
            document.set_ribbon_open(true);
            document.editor.set_ribbon_tab(ui::toolbars::spec::tab("view").expect("the view tab exists"));
        }
        let _ = settle(&mut app, &ctx, &render_state, screen);
        let jobs = run_frame(&mut app, &ctx, &render_state, screen, Vec::new());
        let pixels_per_point = ctx.pixels_per_point();
        let size = [
            (WINDOW.x * pixels_per_point).round().max(1.0) as u32,
            (WINDOW.y * pixels_per_point).round().max(1.0) as u32,
        ];
        let rgba = paint(&render_state, &jobs, size, pixels_per_point).expect("the frame paints");
        let path = directory.join(format!("shadows-view-tab-{name}.png"));
        testkit::write_png(&path, size[0], size[1], &rgba).expect("the picture is written");
        println!("the {name} View tab's Display group is in {}", path.display());
        if scale != 100 {
            continue;
        }

        // The same view with the switch off, for the eye to compare with the picture above: the
        // floor keeps its plate and loses the silhouette (U107).
        app.tabs.active_mut().toggle_shadows();
        let _ = settle(&mut app, &ctx, &render_state, screen);
        let jobs = run_frame(&mut app, &ctx, &render_state, screen, Vec::new());
        let rgba = paint(&render_state, &jobs, size, pixels_per_point).expect("the frame paints");
        let path = directory.join(format!("shadows-off-{name}.png"));
        testkit::write_png(&path, size[0], size[1], &rgba).expect("the picture is written");
        println!("the {name} shadow switched off is in {}", path.display());

        // Four cells of one workspace: the active front view draws no shadow while the top and the
        // axonometric cells keep theirs, so the split is one switch per view, not one per window.
        {
            let document = app.tabs.active_mut();
            document.toggle_shadows();
            document.set_layout(crate::view_layout::LayoutKind::Quad);
            document.toggle_shadows();
        }
        let _ = settle(&mut app, &ctx, &render_state, screen);
        let jobs = run_frame(&mut app, &ctx, &render_state, screen, Vec::new());
        let rgba = paint(&render_state, &jobs, size, pixels_per_point).expect("the frame paints");
        let path = directory.join(format!("shadows-split-{name}.png"));
        testkit::write_png(&path, size[0], size[1], &rgba).expect("the picture is written");
        app.tabs.active_mut().release_target();
        println!("the {name} four views with one shadow switched off are in {}", path.display());
    }
}

#[test]
#[ignore = "needs a GPU adapter; writes target/ui-shots/band-*.png"]
fn the_rubber_band_is_painted_offscreen_in_both_themes() {
    let render_state = match render_state() {
        Ok(render_state) => render_state,
        Err(reason) => {
            println!("{reason}");
            return;
        }
    };
    let directory = testkit::ui_shots_dir().expect("the shots directory is created");
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, WINDOW);
    for (name, variant) in [("dark", ThemeChoice::Dark), ("light", ThemeChoice::Light)] {
        for rule in ["window", "crossing"] {
            let ctx = egui::Context::default();
            let workspace = testkit::scratch_dir("ui-shots").join(format!("band-{name}-{rule}.json"));
            let mut settings = Settings::default();
            settings.appearance.theme = variant;
            let session = crate::scene_build::demo_session().expect("the sample session builds");
            let mut app = App::test_app(&ctx, Some(render_state.clone()), workspace, settings, session)
                .expect("the application builds");
            let _ = settle(&mut app, &ctx, &render_state, screen);
            // The whole piece stays in view: the shot is about the rectangle around it, so the
            // camera is not moved in the way the panel shots move it.
            let (low, high) = {
                let document = app.tabs.active();
                let frame = document.frame();
                let scene = document.scene_rect().expect("a settled frame gives the scene a rectangle");
                let mut model = egui::Rect::NOTHING;
                for drawable in frame.drawables() {
                    for position in drawable.mesh.positions.iter() {
                        let world = drawable.world_transform.transform_point3(geom::DVec3::new(
                            f64::from(position[0]),
                            f64::from(position[1]),
                            f64::from(position[2]),
                        ));
                        if let Some(point) = crate::gizmo_math::screen_point(frame.camera, scene, world) {
                            model = model.union(egui::Rect::from_min_max(point, point));
                        }
                    }
                }
                let margin = 40.0;
                (
                    egui::pos2(model.left() - margin, model.bottom() + margin),
                    egui::pos2(model.right() + margin, model.top() - margin),
                )
            };
            // A window is drawn from the low corner to the high one, a crossing band back again.
            let (from, to) = if rule == "window" { (low, high) } else { (high, low) };
            let _ = run_frame(&mut app, &ctx, &render_state, screen, vec![egui::Event::PointerMoved(from)]);
            let press = egui::Event::PointerButton {
                pos: from,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            };
            let _ = run_frame(&mut app, &ctx, &render_state, screen, vec![press]);
            let jobs = run_frame(&mut app, &ctx, &render_state, screen, vec![egui::Event::PointerMoved(to)]);
            let band = app.tabs.active().rubber_band().expect("the drag drew the band");
            assert_eq!(band.from, from, "the {name} {rule} band starts where the press landed");
            let pixels_per_point = ctx.pixels_per_point();
            let size = [
                (WINDOW.x * pixels_per_point).round().max(1.0) as u32,
                (WINDOW.y * pixels_per_point).round().max(1.0) as u32,
            ];
            let rgba = paint(&render_state, &jobs, size, pixels_per_point).expect("the frame paints");
            let path = directory.join(format!("band-{rule}-{name}.png"));
            testkit::write_png(&path, size[0], size[1], &rgba).expect("the picture is written");
            app.tabs.active_mut().release_target();
            let picture = Picture { width: size[0], height: size[1], rgba };
            assert!(!picture.is_flat(), "{} draws less than one colour", path.display());
            println!("the {name} {rule} band is in {}", path.display());
        }
    }
}
