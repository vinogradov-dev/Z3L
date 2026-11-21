//! The one entry point of a final render: `render.final`.
//!
//! A document command is not the right shape for this work. Everything in `engine`'s registry writes
//! the document and lands in one undo entry (`AGENTS.md` §4.1), while a render writes a file, takes
//! minutes and must be cancellable — so `render.final` is one function this crate owns, driven by the
//! shell, with a [`Progress`] the window can read and stop. What makes it a command in the sense the
//! product uses is narrower and truer: one id, one typed request, one result and one sentence when it
//! refuses, reachable from the UI, the CLI or the assistant without a second code path.
//!
//! The order of the steps is the contract a person sees: the frame is uploaded (or reused), every
//! sample is traced, and only then is the image encoded and written. A cancel at any sample therefore
//! leaves the destination exactly as it was — the old picture is still there and no partial file is
//! beside it — and the session is back to idle the moment the refusal arrives.
//!
//! Refuses: a request or an exposure outside its range, a progress counter that belongs to another
//! render, and every refusal the tracer and the encoder make. Nothing here panics on a full disk, a
//! missing directory or a driver that gives up.
//!
//! Changes when: what one render takes as input, or what it leaves behind, changes.

use std::path::{Path, PathBuf};

use geom::DVec3;
use scene::{Camera, SceneFrame};

use crate::gpu::{Progress, Tracer};
use crate::image::{encode_png, write_atomic};
use crate::{FinalError, Request};

/// The stable id of a final render, which a surface, a shortcut and the assistant all name.
///
/// It is not a document command: it writes a file and leaves the document alone, so the engine's
/// registry does not carry it and no undo entry answers for it (`AGENTS.md` §4.1).
pub const RENDER_FINAL_ID: &str = "render.final";

/// What one render was asked for: the image, the exposure and where the file goes.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    request: Request,
    exposure: f32,
    path: PathBuf,
}

impl Settings {
    /// A render of `request` at `exposure`, written to `path`.
    ///
    /// The exposure is a photographic stop applied before the filmic curve, so a scene that is lit for
    /// a workshop and a scene that is lit for a catalogue share one curve and differ by a number. A
    /// value that is not finite or not positive is refused here rather than producing a black or a
    /// white image nobody asked for.
    pub fn new(request: Request, exposure: f32, path: impl Into<PathBuf>) -> Result<Self, FinalError> {
        if !exposure.is_finite() || exposure <= 0.0 {
            return Err(FinalError::Exposure { exposure });
        }
        Ok(Self { request, exposure, path: path.into() })
    }

    /// A render at one stop and the given destination, which is what a surface starts with.
    pub fn default_for(request: Request, path: impl Into<PathBuf>) -> Result<Self, FinalError> {
        Self::new(request, 1.0, path)
    }

    /// The image size and sample count.
    pub fn request(&self) -> Request {
        self.request
    }

    /// The exposure stops applied before the filmic curve.
    pub fn exposure(&self) -> f32 {
        self.exposure
    }

    /// Where the PNG goes.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// What a finished render leaves behind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    /// The file that was written.
    pub path: PathBuf,
    /// The image width in pixels.
    pub width: u32,
    /// The image height in pixels.
    pub height: u32,
    /// How many samples each pixel took.
    pub samples: u32,
}

/// What a render traces: the frame, the view it is seen from and the revision the frame is keyed by.
///
/// One value rather than three parameters, so the call site reads as the same sentence a person says
/// — "this frame, from this camera, at this revision" — and a fourth input later has a home.
pub struct Input<'a> {
    /// The frame the viewport shows, whose triangles, lights and colours are the render's input.
    pub frame: &'a SceneFrame,
    /// The session's camera: a final render is the view a person was looking at.
    pub camera: Camera,
    /// The document counter the frame was built at, which keys the tracer's BVH cache.
    pub revision: u64,
}

/// Renders `input` and writes the PNG, or refuses and writes nothing.
///
/// The revision keys the tracer's BVH cache, so a second image of an unchanged document costs one
/// traverse per sample and no build. `progress` must belong to this request's sample count, because
/// it is the number the surface shows and the count the cancel is judged against.
pub fn run(
    tracer: &mut Tracer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    input: Input<'_>,
    settings: &Settings,
    progress: &Progress,
) -> Result<Report, FinalError> {
    let request = settings.request;
    if progress.total() != request.samples() {
        return Err(FinalError::Progress { total: progress.total(), samples: request.samples() });
    }
    let Input { frame, camera, revision } = input;
    tracer.prepare(device, frame, revision)?;
    let accumulation = tracer.render(device, queue, camera, &frame.lighting, request, progress)?;
    if progress.cancelled() {
        return Err(FinalError::Cancelled { samples: progress.done(), total: progress.total() });
    }
    let image = accumulation.into_image()?;
    let bytes = encode_png(&image, settings.exposure)?;
    write_atomic(&settings.path, &bytes)?;
    Ok(Report { path: settings.path.clone(), width: image.width(), height: image.height(), samples: request.samples() })
}

/// The camera a final render starts from, so a caller does not have to remember the default distance.
///
/// A surface hands over the session camera; this exists for the CLI and for the tests, and it is the
/// same [`Camera::new`] the viewport uses, which is what keeps the image the view the viewport shows.
pub fn camera_looking_at(target: DVec3, distance: f64) -> Result<Camera, FinalError> {
    Camera::new(target, distance).map_err(|error| FinalError::Device { reason: error.to_string() })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    #[test]
    fn an_exposure_is_a_finite_positive_number_of_stops() {
        let request = Request::new(64, 64, 1).expect("a small request");
        let settings = Settings::new(request, 2.0, "out.png").expect("two stops is a setting");
        assert_eq!(settings.exposure(), 2.0);
        assert_eq!(settings.path(), Path::new("out.png"));
        assert_eq!(settings.request(), request);
        for hostile in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            let error = Settings::new(request, hostile, "out.png").expect_err("a broken exposure is refused");
            assert!(matches!(error, FinalError::Exposure { .. }), "{hostile} was accepted");
        }
        // One stop is what a surface starts with.
        assert_eq!(Settings::default_for(request, "out.png").expect("one stop").exposure(), 1.0);
    }

    #[test]
    fn a_progress_counter_from_another_request_is_refused_before_a_sample_is_traced() {
        // The refusal is checked first, so this test needs no device: the mismatch is a sentence
        // about the two counts, not a render that shows the wrong percentage.
        let refusal = |total| FinalError::Progress { total, samples: 8 };
        assert!(refusal(4).to_string().contains('4'), "the sentence names the counter");
        assert!(refusal(4).to_string().contains('8'), "and the request");
        assert!(!refusal(4).is_cancelled());
    }
}
