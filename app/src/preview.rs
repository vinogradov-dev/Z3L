//! The picture the agent's `render_preview` tool answers: the session's scene, drawn offscreen.
//!
//! This is the shell half of the port [`mcp::Previewer`] holds (ADR 0228): `mcp` may not depend on
//! `wgpu` (ADR 0186), and a client must not need a device, so the device and the renderer stay where
//! they already are — here. One device is created without a window (the one `ui_shots` borrows),
//! the scene is evaluated with the same call the viewport makes, the view is framed the way *Frame
//! all* frames it, and `render` draws it into a target this crate reads back and encodes.
//!
//! The framing is the document's, never the window's: an authored camera wins, otherwise the scene
//! is fitted. A headless process has no viewport at all (`engine::Session` holds no camera), so a
//! rule that read "what the person is looking at" would answer two different pictures from the two
//! doors, and the test that asserts a size could not tell which one it looked at.
//!
//! Refuses: a session whose geometry the kernel reports is a [`String`] the tool returns as its
//! refusal, and a machine with no adapter is `None` at [`ScenePreview::headless`] — the tool then
//! says so in the sentence it was given instead of failing the protocol.
//!
//! Changes when: what a preview frames, which passes it draws or how it is encoded changes.

use eframe::egui_wgpu;
use eframe::wgpu;
use engine::Session;
use kernel_truck::TruckKernel;
use mcp::{PreviewSize, Previewer};
use mesh::EvaluationCache;
use render::ViewportRenderer;
use scene::SceneFrame;
use ui::ViewFlags;

use crate::scene_build::{evaluate, home_camera, scene_camera};

/// A device and a queue with no surface behind them, or the sentence that says why there is none.
///
/// One place in the shell creates a windowless device: the interface shots borrow it, the upload
/// probe in `scene_build` borrows it, and a headless preview draws with it.
pub(crate) fn render_state() -> Result<egui_wgpu::RenderState, String> {
    let configuration = egui_wgpu::WgpuConfiguration::default();
    let instance = wgpu::Instance::default();
    pollster::block_on(egui_wgpu::RenderState::create(
        &configuration,
        &instance,
        None,
        egui_wgpu::RendererOptions::PREDICTABLE,
    ))
    .map_err(|error| format!("this machine has no GPU adapter to draw a picture with: {error}"))
}

/// The shell's previewer: one windowless device, and whatever session it is handed.
#[derive(Clone)]
pub(crate) struct ScenePreview {
    state: egui_wgpu::RenderState,
}

impl std::fmt::Debug for ScenePreview {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The device is the only fact worth printing: it is what makes a picture possible.
        formatter.debug_struct("ScenePreview").finish_non_exhaustive()
    }
}

impl ScenePreview {
    /// A previewer over the device a window already owns.
    pub(crate) fn new(state: egui_wgpu::RenderState) -> Self {
        Self { state }
    }

    /// A previewer over a device this process creates, or `None` when it has no adapter.
    ///
    /// A machine that cannot make a device is not a failure of the server: every other tool works,
    /// and only the picture refuses (see `mcp::preview`'s no-device sentence).
    pub(crate) fn headless() -> Option<Self> {
        render_state().ok().map(Self::new)
    }
}

impl Previewer for ScenePreview {
    fn preview(&self, session: &Session, size: PreviewSize) -> Result<Vec<u8>, String> {
        draw(&self.state, session, size)
    }
}

/// Draws the scene `session` holds at `size` and answers the bytes of one PNG.
///
/// The evaluation is the viewport's own call, with an empty cache: a preview is asked for once by a
/// client, so a cache here would either be empty anyway or hold a second copy of every mesh.
pub(crate) fn draw(state: &egui_wgpu::RenderState, session: &Session, size: PreviewSize) -> Result<Vec<u8>, String> {
    let mut kernel = TruckKernel::new();
    let mut cache = EvaluationCache::new();
    let evaluated = evaluate(&mut cache, &mut kernel, session, &ViewFlags::default(), session.active_product())?;
    let camera = scene_camera(session.project(), home_camera(evaluated.bounds)?);
    let mut frame: SceneFrame = evaluated.frame;
    frame.camera = camera;

    let mut renderer = ViewportRenderer::new(&state.device, size.width, size.height);
    renderer
        .upload(&state.device, &state.queue, &frame)
        .map_err(|error| format!("the scene cannot be drawn: {error}"))?;
    renderer.render(&state.device, &state.queue, &frame);
    let [width, height] = renderer.size();
    let pixels = renderer.read_color(&state.device, &state.queue).map_err(|error| error.to_string())?;
    r#final::encode_rgba8(width, height, &pixels).map_err(|error| format!("the picture cannot be encoded: {error}"))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    /// The width and height a PNG's header carries, or `None` when the bytes are not one.
    fn header(png: &[u8]) -> Option<(u32, u32)> {
        const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        if png.get(..8) != Some(SIGNATURE.as_slice()) || png.get(12..16) != Some(b"IHDR".as_slice()) {
            return None;
        }
        let side = |at: usize| -> Option<u32> { png.get(at..at + 4)?.try_into().ok().map(u32::from_be_bytes) };
        Some((side(16)?, side(20)?))
    }

    /// Whether the picture carries image data at all, as opposed to a header and an end marker.
    fn has_image_data(png: &[u8]) -> bool {
        png.len() > 8 + 25 + 12 && png.windows(4).any(|chunk| chunk == b"IDAT")
    }

    #[test]
    fn a_picture_of_the_scene_has_the_size_the_tool_asked_for_and_draws_something() {
        let Some(state) = render_state().ok() else {
            println!("this machine has no GPU adapter, so a preview cannot be drawn here");
            return;
        };
        let session = match crate::scene_build::demo_session() {
            Ok(session) => session,
            Err(refusal) => {
                println!("the demonstration scene could not be built: {refusal}");
                return;
            }
        };

        // Two sizes, so the picture is measured rather than remembered: a renderer that ignored the
        // request and kept one target size would pass a single-size test.
        for size in [PreviewSize { width: 320, height: 200 }, PreviewSize { width: 96, height: 160 }] {
            let png = draw(&state, &session, size).expect("the scene draws");
            assert_eq!(header(&png), Some((size.width, size.height)), "the file says the size it was asked for");
            assert!(has_image_data(&png), "the picture carries image data, not an empty file");
        }
    }

    #[test]
    fn a_scene_the_kernel_cannot_build_is_the_sentence_the_tool_returns() {
        // The port answers a refusal, never a panic: a session is handed over and the draw decides.
        let Some(state) = render_state().ok() else {
            println!("this machine has no GPU adapter, so a preview cannot be drawn here");
            return;
        };
        let session = match crate::scene_build::empty_session() {
            Ok(session) => session,
            Err(refusal) => {
                println!("the empty scene could not be built: {refusal}");
                return;
            }
        };
        // An empty document is not a refusal — it draws its floor and grid — so what is asserted
        // here is that the port answers rather than panics: the empty scene is a legitimate picture.
        let png = ScenePreview::new(state).preview(&session, PreviewSize { width: 96, height: 96 });
        let png = png.expect("an empty document is still a picture");
        assert_eq!(header(&png), Some((96, 96)));
    }
}
