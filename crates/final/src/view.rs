//! The pixels of a final image and the ray each sample takes.
//!
//! An image is addressed the way a file is written: pixel `(0, 0)` is the top-left corner, so a row
//! index grows downwards while a normalised device coordinate grows upwards. That single flip is the
//! oldest mistake a camera-into-image path makes, so it lives in one function, is tested against the
//! viewport's own projection, and is transcribed into WGSL rather than re-derived there.
//!
//! The ray itself is the viewport's: [`camera_ray`] hands the normalised coordinate to
//! `Camera::ray_ndc`, which is what picking uses. A final render is then, by construction, the view a
//! person was looking at, and a test asserts it at a known point rather than trusting it.
//!
//! Refuses: a size of zero, a non-finite sample position or a broken aspect ratio is `None`, never a
//! ray of NaN that would trace an image of noise.

use geom::DVec2;
use scene::Camera;

use crate::ray::Ray;

/// The aspect ratio of an image, or `None` when its height is zero.
pub fn aspect(width: u32, height: u32) -> Option<f64> {
    if width == 0 || height == 0 {
        return None;
    }
    Some(f64::from(width) / f64::from(height))
}

/// The normalised device coordinate of a point given in image coordinates.
///
/// `(0, 0)` is the top-left corner of the image and `(width, height)` the bottom-right corner, so the
/// centre of pixel `(x, y)` is at `(x + 0.5, y + 0.5)`. The result is `(-1, -1)` at the bottom-left and
/// `(1, 1)` at the top-right, which is what the camera expects.
pub fn ndc_of_point(x: f64, y: f64, width: u32, height: u32) -> Option<DVec2> {
    let (width, height) = (f64::from(width), f64::from(height));
    if !x.is_finite() || !y.is_finite() || width <= 0.0 || height <= 0.0 {
        return None;
    }
    Some(DVec2::new(x / width * 2.0 - 1.0, 1.0 - y / height * 2.0))
}

/// The ray through one sample of an image.
///
/// The satellite: `jitter` is the sample's offset from the pixel's centre, in pixel units, so a
/// sampler hands over a value inside `[-0.5, 0.5)` on each axis and the ray stays inside its pixel.
pub fn camera_ray(camera: Camera, jitter: DVec2, pixel: (u32, u32), width: u32, height: u32) -> Option<Ray> {
    let ndc = ndc_of_point(f64::from(pixel.0) + 0.5 + jitter.x, f64::from(pixel.1) + 0.5 + jitter.y, width, height)?;
    let aspect = aspect(width, height)?;
    let ray = camera.ray_ndc(ndc.x, ndc.y, aspect);
    Ray::new(ray.origin(), ray.direction())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use geom::DVec3;

    use super::*;

    const WIDTH: u32 = 800;
    const HEIGHT: u32 = 600;

    fn camera() -> Camera {
        // A three-quarter view of a two-metre cabinet, the way the viewport opens.
        Camera::new(DVec3::new(0.0, 0.0, 1000.0), 3000.0).expect("a camera with a distance")
    }

    /// The image coordinate a normalised device coordinate lands on, the inverse of
    /// [`ndc_of_point`] written out independently so the test does not restate the implementation.
    fn image_point(ndc: DVec2) -> (f64, f64) {
        ((ndc.x + 1.0) * 0.5 * f64::from(WIDTH), (1.0 - ndc.y) * 0.5 * f64::from(HEIGHT))
    }

    /// How much of the world the image's full height covers at `depth`, in millimetres.
    fn visible_height(camera: Camera, depth: f64) -> f64 {
        match camera.projection() {
            scene::Projection::Perspective { fov_y } => 2.0 * depth * (fov_y * 0.5).tan(),
            scene::Projection::Orthographic { height } => height,
        }
    }

    #[test]
    fn the_mapping_is_the_inverse_of_the_viewports_projection() {
        let camera = camera();
        let aspect = aspect(WIDTH, HEIGHT).expect("an aspect ratio");
        for point in [DVec3::new(0.0, 0.0, 0.0), DVec3::new(800.0, 250.0, 120.0), DVec3::new(-900.0, -400.0, -60.0)] {
            let ndc = camera.project_ndc(point, aspect).expect("the point is in front of the camera");
            let (x, y) = image_point(ndc);
            let back = ndc_of_point(x, y, WIDTH, HEIGHT).expect("the image point maps");
            assert!((back.x - ndc.x).abs() < 1e-12 && (back.y - ndc.y).abs() < 1e-12, "{back:?} vs {ndc:?}");
        }
    }

    #[test]
    fn pixel_zero_zero_is_the_top_left_corner() {
        let camera = camera();
        let top_left = camera_ray(camera, DVec2::ZERO, (0, 0), WIDTH, HEIGHT).expect("a ray");
        let bottom_right = camera_ray(camera, DVec2::ZERO, (WIDTH - 1, HEIGHT - 1), WIDTH, HEIGHT).expect("a ray");
        // Orientation is read from the camera itself, so the test survives a change of default view.
        let (right, up) = (camera.right(), camera.up());
        let to_top_left = top_left.direction() - camera.forward();
        let to_bottom_right = bottom_right.direction() - camera.forward();
        assert!(to_top_left.dot(up) > 0.0, "the first row is above the centre: {to_top_left:?}");
        assert!(to_top_left.dot(right) < 0.0, "the first column is left of the centre: {to_top_left:?}");
        assert!(to_bottom_right.dot(up) < 0.0, "{to_bottom_right:?}");
        assert!(to_bottom_right.dot(right) > 0.0, "{to_bottom_right:?}");
    }

    #[test]
    fn a_pixels_ray_lands_within_one_pixel_of_a_point_the_viewport_projects() {
        let camera = camera();
        let aspect = aspect(WIDTH, HEIGHT).expect("an aspect ratio");
        let point = DVec3::new(420.0, 310.0, 60.0);
        let ndc = camera.project_ndc(point, aspect).expect("the point is visible");
        let (x, y) = image_point(ndc);
        let pixel = (
            (x - 0.5).round().clamp(0.0, f64::from(WIDTH - 1)) as u32,
            (y - 0.5).round().clamp(0.0, f64::from(HEIGHT - 1)) as u32,
        );
        let ray = camera_ray(camera, DVec2::ZERO, pixel, WIDTH, HEIGHT).expect("a ray");
        // The distance from the pixel's ray to the point, measured on the plane through the point
        // that faces the camera, is shorter than one pixel's width on that plane.
        let plane_depth = (point - camera.eye()).dot(camera.forward());
        let pixel_width = visible_height(camera, plane_depth) * aspect / f64::from(WIDTH);
        let along = (point - ray.origin()).dot(camera.forward()) / ray.direction().dot(camera.forward());
        let on_ray = ray.at(along);
        let gap = (on_ray - point).length();
        assert!(gap <= pixel_width, "the point is {gap} mm from the ray, and a pixel is {pixel_width} mm there");
    }

    #[test]
    fn a_jittered_sample_stays_inside_its_own_pixel() {
        let centre = ndc_of_point(100.5, 200.5, WIDTH, HEIGHT).expect("the centre maps");
        let left = ndc_of_point(100.0, 200.0, WIDTH, HEIGHT).expect("the corner maps");
        let right = ndc_of_point(101.0, 201.0, WIDTH, HEIGHT).expect("the corner maps");
        assert!(left.x < centre.x && centre.x < right.x, "the pixel spans its edges");
        assert!(left.y > centre.y && centre.y > right.y, "the row grows downwards");
        for jitter in [DVec2::new(-0.5, -0.5), DVec2::new(0.5, 0.5), DVec2::new(-0.5, 0.5)] {
            let sample = ndc_of_point(100.5 + jitter.x, 200.5 + jitter.y, WIDTH, HEIGHT).expect("the sample maps");
            assert!(sample.x >= left.x && sample.x <= right.x, "{jitter:?} left the pixel");
            assert!(sample.y <= left.y && sample.y >= right.y, "{jitter:?} left the pixel");
        }
    }

    #[test]
    fn an_orthographic_view_takes_parallel_rays_from_different_origins() {
        let mut camera = camera();
        camera.set_orthographic();
        let first = camera_ray(camera, DVec2::ZERO, (10, 10), WIDTH, HEIGHT).expect("a ray");
        let second = camera_ray(camera, DVec2::ZERO, (700, 400), WIDTH, HEIGHT).expect("a ray");
        assert!((first.direction() - second.direction()).length() < 1e-12, "an orthographic view is parallel");
        assert!((first.origin() - second.origin()).length() > 1.0, "each pixel starts somewhere else");
    }

    #[test]
    fn a_broken_size_or_a_broken_coordinate_is_no_image_point() {
        assert!(aspect(0, 100).is_none());
        assert!(aspect(100, 0).is_none());
        assert!(ndc_of_point(1.0, 1.0, 0, 100).is_none());
        assert!(ndc_of_point(f64::NAN, 1.0, 100, 100).is_none());
        assert!(camera_ray(camera(), DVec2::ZERO, (0, 0), 0, 0).is_none());
    }
}
