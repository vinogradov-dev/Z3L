//! The final render: one still image at print resolution, drawn by our own tracer.
//!
//! The viewport draws a frame as fast as a device can, and a catalogue needs the opposite trade: one
//! image, minutes of it, no aliasing and no noise. This crate owns that pipeline and nothing else —
//! the BVH over the frame's triangles, the sampler, the WGSL integrator and the tone map that ends
//! in a PNG. It borrows the device the shell already created instead of opening one: `wgpu` reaches
//! this crate as the shell's own handle, so the renderer and the tracer cannot drift into two
//! versions of the same API (`AGENTS.md` §5).
//!
//! Input is frame data, never document data: the triangles of a [`scene`] frame, the lights it
//! carries in watts with inverse-square falloff (ADR 0039) and the session camera. This crate reads
//! no `Project`, no catalogue and no file; the shell hands it a frame and takes an image back.
//!
//! Layer: L5, beside `render` and above `scene`. It must not depend on `render` (a sibling, and the
//! tracer owns its own pipelines), on `egui` or a window (the shell owns both), or on `model` (the
//! document is not this crate's input).
//!
//! Refuses: a size, a pixel count or a sample count outside the range this crate can allocate is a
//! [`FinalError`] naming the field and its limit, never an allocation that answers a hostile request
//! with an out-of-memory kill (`AGENTS.md` §4.4). A device that cannot run the tracer's shader is an
//! error the shell reports, not a silently black image.
//!
//! The techniques are implemented from their published sources and cited where they are used — the
//! integrator from Veach 1997 and *Physically Based Rendering*, the binned-SAH build from Wald 2007
//! — and `plan/notes/Y1.md` carries the measurements and the reasoning behind each choice.
//!
//! Changes when: what one image costs, how a sample is taken, or what the frame hands over changes.
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod bvh;
pub mod ray;
pub mod soup;
pub mod view;

pub use bvh::Bvh;
pub use ray::{Hit, Ray, Triangle};
pub use soup::{MAX_TRIANGLES, triangles_of};

/// The smallest side of a final image, in pixels.
///
/// Below this the request is a mistake rather than a small picture: a 4×4 crop of a cabinet shows
/// nothing a person made, and the tracer would still pay for a BVH, a device pass and a file.
pub const MIN_SIDE: u32 = 16;

/// The largest side of a final image, in pixels.
///
/// A print is capped by its paper, not by the device: 8192 is A2 at 300 dpi on its longer edge, and
/// a request beyond it is refused by name rather than clamped, because a clamped render is a wrong
/// image with no notice.
pub const MAX_SIDE: u32 = 8192;

/// The largest image the tracer accepts, in pixels.
///
/// A3 at 300 dpi is 17.4 megapixels, and the accumulator behind an image is 16 bytes per pixel
/// (RGBA, `f32`): the cap owns about 256 MB beside the BVH and the device's own buffers.
pub const MAX_PIXELS: u64 = 16_000_000;

/// The fewest samples per pixel a render may ask for.
pub const MIN_SAMPLES: u32 = 1;

/// The most samples per pixel a render may ask for.
///
/// A cap exists so that a request cannot become an unbounded job: at 4096 samples the pass is long
/// enough for a print and short enough that its progress and its cancel still mean something.
pub const MAX_SAMPLES: u32 = 4096;

/// What a final render was asked for: an image size and a sample count.
///
/// Every field is inside the range the crate can allocate, because the only constructor checks them;
/// the tracer therefore never validates a size again and never allocates what a request asked for
/// without a bound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Request {
    width: u32,
    height: u32,
    samples: u32,
}

impl Request {
    /// Checks one request against the limits above.
    ///
    /// The sentence names the field and the limit, so a refusal can be shown as it is: a session that
    /// asked for a 40000-pixel side reads which bound it crossed, not "invalid parameters".
    pub fn new(width: u32, height: u32, samples: u32) -> Result<Self, FinalError> {
        for side in [width, height] {
            if side < MIN_SIDE {
                return Err(FinalError::SideTooSmall { side, min: MIN_SIDE });
            }
            if side > MAX_SIDE {
                return Err(FinalError::SideTooLarge { side, max: MAX_SIDE });
            }
        }
        let pixels = u64::from(width) * u64::from(height);
        if pixels > MAX_PIXELS {
            return Err(FinalError::TooManyPixels { pixels, max: MAX_PIXELS });
        }
        if samples < MIN_SAMPLES {
            return Err(FinalError::TooFewSamples { samples, min: MIN_SAMPLES });
        }
        if samples > MAX_SAMPLES {
            return Err(FinalError::TooManySamples { samples, max: MAX_SAMPLES });
        }
        Ok(Self { width, height, samples })
    }

    /// The image width in pixels.
    pub fn width(self) -> u32 {
        self.width
    }

    /// The image height in pixels.
    pub fn height(self) -> u32 {
        self.height
    }

    /// The samples taken per pixel.
    pub fn samples(self) -> u32 {
        self.samples
    }

    /// The pixel count, which every buffer of one image is sized by.
    pub fn pixels(self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }
}

/// What the final render refuses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum FinalError {
    /// A side below [`MIN_SIDE`].
    #[error("a final render's side is at least {min} pixels, and {side} was asked for")]
    SideTooSmall {
        /// The side the request carried.
        side: u32,
        /// The smallest side the crate accepts.
        min: u32,
    },
    /// A side above [`MAX_SIDE`].
    #[error("a final render's side is at most {max} pixels, and {side} was asked for")]
    SideTooLarge {
        /// The side the request carried.
        side: u32,
        /// The largest side the crate accepts.
        max: u32,
    },
    /// An image whose pixel count is past [`MAX_PIXELS`].
    #[error("a final render holds at most {max} pixels, and {pixels} were asked for")]
    TooManyPixels {
        /// The pixel count the request carried.
        pixels: u64,
        /// The largest image the crate accepts.
        max: u64,
    },
    /// A sample count below [`MIN_SAMPLES`].
    #[error("a final render takes at least {min} sample per pixel, and {samples} were asked for")]
    TooFewSamples {
        /// The sample count the request carried.
        samples: u32,
        /// The fewest samples the crate accepts.
        min: u32,
    },
    /// A sample count above [`MAX_SAMPLES`].
    #[error("a final render takes at most {max} samples per pixel, and {samples} were asked for")]
    TooManySamples {
        /// The sample count the request carried.
        samples: u32,
        /// The most samples the crate accepts.
        max: u32,
    },
    /// A frame with more triangles than one render accepts.
    #[error("a final render holds at most {max} triangles, and this frame has {count}")]
    TooManyTriangles {
        /// The triangle count the frame carried.
        count: usize,
        /// The most triangles the crate accepts.
        max: usize,
    },
    /// A mesh the tracer cannot read: a triangle index past its vertices, or half a triangle.
    #[error("a mesh names the triangle index {index}, which it does not have; a final render refuses it")]
    MalformedMesh {
        /// The index that is not a whole triangle of the mesh.
        index: usize,
    },
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    #[test]
    fn a_print_sized_request_is_accepted() {
        // A4 at 300 dpi, the size a catalogue page is printed at.
        let request = Request::new(2480, 3508, 512).expect("A4 at 300 dpi is a legal request");
        assert_eq!(request.width(), 2480);
        assert_eq!(request.height(), 3508);
        assert_eq!(request.pixels(), 8_699_840);
        assert_eq!(request.samples(), 512);
    }

    #[test]
    fn both_end_points_of_every_number_are_refused_by_name() {
        let small = Request::new(MIN_SIDE - 1, 100, 1).expect_err("a side below the floor is a mistake");
        assert!(small.to_string().contains(&(MIN_SIDE - 1).to_string()), "{small}");
        assert!(small.to_string().contains(&MIN_SIDE.to_string()), "the sentence names the bounds: {small}");
        assert!(Request::new(MIN_SIDE, MIN_SIDE, MIN_SAMPLES).is_ok(), "the floor itself is legal");

        let large = Request::new(100, MAX_SIDE + 1, 1).expect_err("a side past the cap is refused");
        assert!(matches!(large, FinalError::SideTooLarge { side, max } if side == MAX_SIDE + 1 && max == MAX_SIDE));

        let many = Request::new(4001, 4000, 1).expect_err("16 megapixels is the cap, not one pixel more");
        assert!(matches!(many, FinalError::TooManyPixels { pixels, max } if pixels == 16_004_000 && max == MAX_PIXELS));
        assert!(Request::new(4000, 4000, 1).is_ok(), "the cap itself is legal");

        let few = Request::new(100, 100, MIN_SAMPLES - 1).expect_err("a sample count of zero draws nothing");
        assert!(matches!(few, FinalError::TooFewSamples { samples, min } if samples == 0 && min == MIN_SAMPLES));

        let too_many = Request::new(100, 100, MAX_SAMPLES + 1).expect_err("a sample count past the cap is refused");
        assert!(
            matches!(too_many, FinalError::TooManySamples { samples, max } if samples == MAX_SAMPLES + 1 && max == MAX_SAMPLES)
        );
    }

    #[test]
    fn the_pixel_cap_is_not_multiplied_into_an_overflow() {
        // Two sides inside the per-side cap whose product would wrap a `u32` in 32-bit arithmetic: the
        // pixel count is `u64` for exactly this request.
        let wide = Request::new(MAX_SIDE, MAX_SIDE, 1).expect_err("8192 squared is past the pixel cap");
        assert!(matches!(wide, FinalError::TooManyPixels { pixels, .. } if pixels == 67_108_864), "{wide}");
    }
}
